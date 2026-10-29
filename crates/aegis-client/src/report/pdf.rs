//! The TARA report laid out on A4 pages and written as a PDF (job 050,
//! ADR 0044). Since ADR 0056 the document model is shared with the HTML and
//! XLSX writers: this module only lays out and renders a [`ReportDoc`].
//!
//! Three steps, each small and testable:
//!
//! 1. [`super::doc::report_doc`] turns the analysis into blocks.
//! 2. `layout` places the blocks on A4 pages: greedy line wrapping with the
//!    real glyph widths of the embedded font, long words broken at the
//!    column edge, tables that continue on the next page with their header
//!    row repeated, and a row that is taller than a page split line by line.
//! 3. `render` writes the pages with `krilla`, adds the footer (the
//!    classification mark, the exporter and "Page n of N") and embeds the
//!    subsets of the font.
//!
//! The output is a function of the analysis and the [`ReportContext`] only,
//! so the same input gives the same bytes (rule P4).
//!
//! Text is placed left to right, glyph by glyph, with no kerning, ligatures
//! or right to left support. That fits the report (Latin and Japanese
//! scripts, names and numbers) and avoids the text shaper that `krilla` can
//! bring along, which is unmaintained (advisory RUSTSEC-2026-0206). A
//! character the fonts do not have is printed as `?`.
//!
//! Time and memory: O(project size). R6: the page count is capped by
//! [`MAX_PAGES`].

use std::collections::HashMap;
use std::sync::Arc;

use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::image::Image as PdfImage;
use krilla::metadata::{DateTime, Metadata};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, Stroke as PdfStroke, StrokeDash};
use krilla::text::{Font, GlyphId, KrillaGlyph};
use skrifa::instance::{LocationRef, Size as FontSize};
use skrifa::{FontRef, MetadataProvider};

use aegis_canvas::{Align, DisplayList, Item as CanvasItem, Paint, SvgTheme};

use super::ReportContext;
use super::doc::{Block, Figure, ReportDoc, Style};
use super::profile::ReportOptions;
use crate::workspace::Workspace;

const REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.otf");
const BOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-Bold.otf");
/// Noto Sans JP (OFL-1.1), the fallback for characters Inter does not carry
/// (Japanese and other CJK). Only the used glyphs are embedded in each PDF.
const CJK: &[u8] = include_bytes!("../../assets/fonts/NotoSansJP-Variable.ttf");

/// Most pages a report may have. A project that needs more is cut off with a
/// sentence on the last page (R6).
pub const MAX_PAGES: usize = 2000;

// ---- fonts and measuring --------------------------------------------------

/// One face: its bytes for krilla, and its metrics for measuring.
struct Face {
    font: Font,
    parsed: FontRef<'static>,
    units_per_em: f32,
    /// char -> glyph and advance in em, or `None` when the face lacks the
    /// character. Lookup only, never iterated (P4).
    cache: HashMap<char, Option<(u32, f32)>>,
}

impl Face {
    fn new(bytes: &'static [u8]) -> Result<Face, PdfError> {
        let font = Font::new(Arc::new(bytes.to_vec()).into(), 0).ok_or(PdfError::Font)?;
        let parsed = FontRef::new(bytes).map_err(|_| PdfError::Font)?;
        let units_per_em = f32::from(
            parsed
                .metrics(FontSize::unscaled(), LocationRef::default())
                .units_per_em,
        );
        Ok(Face {
            font,
            parsed,
            units_per_em,
            cache: HashMap::new(),
        })
    }

    /// The glyph and advance (in em) of `c`, or `None` when this face does
    /// not have the character.
    fn glyph(&mut self, c: char) -> Option<(u32, f32)> {
        if let Some(g) = self.cache.get(&c) {
            return *g;
        }
        let found = self
            .parsed
            .charmap()
            .map(c)
            .filter(|g| g.to_u32() != 0)
            .map(|gid| {
                let advance = self
                    .parsed
                    .glyph_metrics(FontSize::unscaled(), LocationRef::default())
                    .advance_width(gid)
                    .unwrap_or(0.0);
                (gid.to_u32(), advance / self.units_per_em)
            });
        self.cache.insert(c, found);
        found
    }
}

struct Fonts {
    regular: Face,
    bold: Face,
    cjk: Face,
}

impl Fonts {
    fn new() -> Result<Fonts, PdfError> {
        Ok(Fonts {
            regular: Face::new(REGULAR)?,
            bold: Face::new(BOLD)?,
            cjk: Face::new(CJK)?,
        })
    }

    /// The glyph and advance (in em) of `c`, with the CJK fallback, then `?`
    /// in the primary face. The bool says whether the glyph comes from the
    /// CJK face, so a run can be split by font.
    fn glyph(&mut self, bold: bool, c: char) -> (bool, u32, f32) {
        let primary = if bold {
            &mut self.bold
        } else {
            &mut self.regular
        };
        if let Some(g) = primary.glyph(c) {
            return (false, g.0, g.1);
        }
        if let Some(g) = self.cjk.glyph(c) {
            return (true, g.0, g.1);
        }
        let q = primary.glyph('?').unwrap_or((0, 0.5));
        (false, q.0, q.1)
    }

    /// Width of `text` at `size` points, with the CJK fallback.
    fn width(&mut self, bold: bool, text: &str, size: f32) -> f32 {
        text.chars().map(|c| self.glyph(bold, c).2).sum::<f32>() * size
    }
}

// ---- layout ---------------------------------------------------------------

const PAGE_W: f32 = 595.28;
const PAGE_H: f32 = 841.89;
const MARGIN: f32 = 48.0;
const FOOTER_H: f32 = 30.0;
pub(crate) const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN;
const BOTTOM: f32 = PAGE_H - MARGIN - FOOTER_H;
const BODY: f32 = 9.5;
const TABLE: f32 = 7.5;
const CELL_PAD: f32 = 3.0;

#[derive(Clone, Copy)]
enum Ink {
    Black,
    Grey,
    Red,
}

/// How a run of text looks.
#[derive(Clone, Copy)]
struct Look {
    size: f32,
    bold: bool,
    ink: Ink,
}

enum Item {
    Text {
        x: f32,
        y: f32,
        size: f32,
        bold: bool,
        ink: Ink,
        text: String,
    },
    /// A filled rectangle (header rows).
    Shade { x: f32, y: f32, w: f32, h: f32 },
    /// A thin rule.
    Rule { x1: f32, x2: f32, y: f32 },
    /// A framed drawing (a figure panel).
    Diagram {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        list: Arc<DisplayList>,
    },
    /// A decoded raster or rasterized SVG image (job 073, RPT-06).
    RasterImage {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        image: PdfImage,
    },
}

struct Pages {
    pages: Vec<Vec<Item>>,
    y: f32,
    truncated: bool,
}

impl Pages {
    fn new() -> Pages {
        Pages {
            pages: vec![Vec::new()],
            y: MARGIN,
            truncated: false,
        }
    }

    fn new_page(&mut self) {
        if self.pages.len() >= MAX_PAGES {
            self.truncated = true;
            return;
        }
        self.pages.push(Vec::new());
        self.y = MARGIN;
    }

    fn room(&self) -> f32 {
        BOTTOM - self.y
    }

    fn push(&mut self, item: Item) {
        if !self.truncated
            && let Some(page) = self.pages.last_mut()
        {
            page.push(item);
        }
    }
}

/// Splits `text` into lines no wider than `width`. Words are kept whole and
/// a word wider than the line is broken at the edge. O(chars).
fn wrap(fonts: &mut Fonts, bold: bool, text: &str, size: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut used = 0.0;
        let space = fonts.width(bold, " ", size);
        for word in paragraph.split_whitespace() {
            let w = fonts.width(bold, word, size);
            if w > width {
                // Flush the line, then cut the word where it no longer fits.
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                let mut piece = String::new();
                let mut piece_w = 0.0;
                for c in word.chars() {
                    let cw = fonts.glyph(bold, c).2 * size;
                    if piece_w + cw > width && !piece.is_empty() {
                        lines.push(std::mem::take(&mut piece));
                        piece_w = 0.0;
                    }
                    piece.push(c);
                    piece_w += cw;
                }
                line = piece;
                used = piece_w;
                continue;
            }
            let extra = if line.is_empty() { w } else { space + w };
            if used + extra > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                line = word.to_owned();
                used = w;
            } else {
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
                used += extra;
            }
        }
        lines.push(line);
    }
    lines
}

fn leading(size: f32) -> f32 {
    size * 1.4
}

fn text_block(p: &mut Pages, fonts: &mut Fonts, text: &str, look: Look, x: f32, width: f32) {
    let Look { size, bold, ink } = look;
    for line in wrap(fonts, bold, text, size, width) {
        if p.room() < leading(size) {
            p.new_page();
        }
        let y = p.y + size;
        p.push(Item::Text {
            x,
            y,
            size,
            bold,
            ink,
            text: line,
        });
        p.y += leading(size);
    }
}

fn heading(p: &mut Pages, fonts: &mut Fonts, level: u8, text: &str) {
    let (size, before, after) = match level {
        1 => (20.0, 0.0, 6.0),
        2 => (14.0, 16.0, 4.0),
        _ => (10.5, 10.0, 2.0),
    };
    // Keep a heading with at least two lines of what follows.
    if p.room() < before + leading(size) + 5.0 * leading(BODY) && p.y > MARGIN {
        p.new_page();
    } else if p.y > MARGIN {
        p.y += before;
    }
    let look = Look {
        size,
        bold: true,
        ink: Ink::Black,
    };
    text_block(p, fonts, text, look, MARGIN, CONTENT_W);
    p.y += after;
}

/// Column widths in points. Every column is at least as wide as its longest
/// header word (so a heading is never cut in the middle) and as its longest
/// short content word; what is left over is shared in proportion to how much
/// text each column holds. If even the minimums do not fit, they are scaled
/// down together. O(cells).
fn column_widths(fonts: &mut Fonts, headers: &[String], rows: &[Vec<String>]) -> Vec<f32> {
    let n = headers.len().max(1);
    let mut min = Vec::with_capacity(n);
    let mut weight = Vec::with_capacity(n);
    for (i, header) in headers.iter().enumerate() {
        let head_word = header
            .split_whitespace()
            .map(|w| fonts.width(true, w, TABLE))
            .fold(0.0_f32, f32::max);
        let body_word = rows
            .iter()
            .filter_map(|r| r.get(i))
            .flat_map(|c| c.split_whitespace())
            .map(|w| fonts.width(false, w, TABLE))
            .fold(0.0_f32, f32::max)
            .min(60.0);
        min.push(head_word.max(body_word) + 2.0 * CELL_PAD + 1.0);
        let longest = rows
            .iter()
            .filter_map(|r| r.get(i))
            .map(|c| c.chars().count())
            .max()
            .unwrap_or(1)
            .max(header.chars().count().min(14));
        #[allow(clippy::cast_precision_loss)]
        weight.push(longest.clamp(5, 48) as f32);
    }
    let min_sum: f32 = min.iter().sum();
    if min_sum >= CONTENT_W {
        return min.iter().map(|m| m * CONTENT_W / min_sum).collect();
    }
    let spare = CONTENT_W - min_sum;
    let weight_sum: f32 = weight.iter().sum();
    min.iter()
        .zip(&weight)
        .map(|(m, w)| m + spare * w / weight_sum)
        .collect()
}

fn table_header(p: &mut Pages, fonts: &mut Fonts, headers: &[String], widths: &[f32]) {
    let lines: Vec<Vec<String>> = headers
        .iter()
        .zip(widths)
        .map(|(h, w)| wrap(fonts, true, h, TABLE, w - 2.0 * CELL_PAD))
        .collect();
    let rows = lines.iter().map(Vec::len).max().unwrap_or(1);
    #[allow(clippy::cast_precision_loss)]
    let height = rows as f32 * leading(TABLE) + 2.0 * CELL_PAD;
    p.push(Item::Shade {
        x: MARGIN,
        y: p.y,
        w: CONTENT_W,
        h: height,
    });
    let mut x = MARGIN;
    for (cell, w) in lines.iter().zip(widths) {
        for (i, line) in cell.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let y = p.y + CELL_PAD + TABLE + i as f32 * leading(TABLE);
            p.push(Item::Text {
                x: x + CELL_PAD,
                y,
                size: TABLE,
                bold: true,
                ink: Ink::Black,
                text: line.clone(),
            });
        }
        x += w;
    }
    p.y += height;
}

fn table_block(p: &mut Pages, fonts: &mut Fonts, headers: &[String], rows: &[Vec<String>]) {
    if rows.is_empty() {
        let look = Look {
            size: BODY,
            bold: false,
            ink: Ink::Grey,
        };
        text_block(p, fonts, "None.", look, MARGIN, CONTENT_W);
        return;
    }
    let widths = column_widths(fonts, headers, rows);
    // The header and the first line of the first row stay together.
    if p.room() < 2.0 * leading(TABLE) + 4.0 * CELL_PAD + leading(TABLE) {
        p.new_page();
    }
    table_header(p, fonts, headers, &widths);
    for row in rows {
        let mut cells: Vec<std::vec::IntoIter<String>> = widths
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let text = row.get(i).map_or("", String::as_str);
                wrap(fonts, false, text, TABLE, w - 2.0 * CELL_PAD).into_iter()
            })
            .collect();
        loop {
            // How many lines still wait, and how many fit on this page.
            let waiting = cells.iter().map(|c| c.len()).max().unwrap_or(0);
            if waiting == 0 {
                break;
            }
            let lines_fit = ((p.room() - 2.0 * CELL_PAD) / leading(TABLE)).floor();
            if lines_fit < 1.0 {
                p.new_page();
                if p.truncated {
                    return;
                }
                table_header(p, fonts, headers, &widths);
                continue;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let take = waiting.min(lines_fit as usize);
            #[allow(clippy::cast_precision_loss)]
            let height = take as f32 * leading(TABLE) + 2.0 * CELL_PAD;
            let mut x = MARGIN;
            for (cell, w) in cells.iter_mut().zip(&widths) {
                for i in 0..take {
                    let Some(line) = cell.next() else { break };
                    #[allow(clippy::cast_precision_loss)]
                    let y = p.y + CELL_PAD + TABLE + i as f32 * leading(TABLE);
                    p.push(Item::Text {
                        x: x + CELL_PAD,
                        y,
                        size: TABLE,
                        bold: false,
                        ink: Ink::Black,
                        text: line,
                    });
                }
                x += w;
            }
            p.y += height;
            p.push(Item::Rule {
                x1: MARGIN,
                x2: MARGIN + CONTENT_W,
                y: p.y,
            });
        }
    }
    p.y += 6.0;
}

fn preformatted(p: &mut Pages, fonts: &mut Fonts, text: &str) {
    const INDENT: f32 = 4.0;
    for line in text.lines() {
        let spaces = line.chars().take_while(|c| *c == ' ').count();
        #[allow(clippy::cast_precision_loss)]
        let x = MARGIN + 6.0 + spaces as f32 * INDENT;
        let width = (CONTENT_W - (x - MARGIN)).max(60.0);
        let look = Look {
            size: TABLE + 1.0,
            bold: false,
            ink: Ink::Black,
        };
        text_block(p, fonts, line.trim_start(), look, x, width);
    }
    p.y += 6.0;
}

/// Places the panels of a figure: a caption, then the frame. A panel never
/// splits across pages; it moves to the next page when it does not fit.
fn figure_block(p: &mut Pages, fonts: &mut Fonts, figure: &Figure) {
    let caption = Look {
        size: TABLE + 1.0,
        bold: false,
        ink: Ink::Grey,
    };
    for panel in &figure.panels {
        let (w, h) = (
            panel.size.0.min(CONTENT_W),
            panel.size.1.min(BOTTOM - MARGIN - 20.0),
        );
        if p.room() < h + leading(caption.size) + 6.0 && p.y > MARGIN {
            p.new_page();
        }
        text_block(p, fonts, &panel.caption, caption, MARGIN, CONTENT_W);
        p.push(Item::Diagram {
            x: MARGIN,
            y: p.y,
            w,
            h,
            list: Arc::clone(&panel.list),
        });
        p.y += h + 8.0;
    }
    if figure.omitted > 0 {
        let note = format!(
            "{} more detail panels are not drawn. The outline below lists every node.",
            figure.omitted
        );
        text_block(p, fonts, &note, caption, MARGIN, CONTENT_W);
        p.y += 4.0;
    }
}

/// Decodes a PNG, JPEG, or sanitized static SVG into a `krilla` [`PdfImage`]
/// (job 073, RPT-06). Returns `None` when `bytes` is not a valid image so the
/// report builder degrades gracefully to the caption line.
fn decode_pdf_image(media_type: &str, bytes: &[u8]) -> Option<PdfImage> {
    if media_type == "image/png" || bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let data: krilla::Data = Arc::new(bytes.to_vec()).into();
        return PdfImage::from_png(data, true).ok();
    }
    if media_type == "image/jpeg" || bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        let data: krilla::Data = Arc::new(bytes.to_vec()).into();
        return PdfImage::from_jpeg(data, true).ok();
    }
    if media_type == "image/svg+xml" || bytes.iter().take(64).any(|b| *b == b'<') {
        let (rgba, w, h) = rasterize_static_svg(bytes)?;
        return Some(PdfImage::from_rgba8(rgba, w, h));
    }
    None
}

/// Minimal deterministic rasterizer for sanitized static SVG figures into an
/// RGBA8 pixel buffer so SVG figures render as real images in the PDF.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::too_many_lines
)]
fn rasterize_static_svg(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let lower = text.to_ascii_lowercase();
    let svg_pos = lower.find("<svg")?;
    let svg_end = lower[svg_pos..].find('>')? + svg_pos;
    let svg_tag = &text[svg_pos..=svg_end];

    let parse_attr = |tag: &str, name: &str| -> Option<String> {
        let pat = format!("{name}=");
        let idx = tag.find(&pat)?;
        let rest = &tag[idx + pat.len()..];
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let end = rest[1..].find(quote)?;
        Some(rest[1..=end].to_owned())
    };
    let parse_num = |s: &str| -> Option<f32> {
        let trimmed: String = s
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
            .collect();
        trimmed.parse::<f32>().ok()
    };

    let mut w_f = parse_attr(svg_tag, "width").and_then(|v| parse_num(&v));
    let mut h_f = parse_attr(svg_tag, "height").and_then(|v| parse_num(&v));
    if (w_f.is_none() || h_f.is_none())
        && let Some(vb) = parse_attr(svg_tag, "viewBox").or_else(|| parse_attr(svg_tag, "viewbox"))
    {
        let nums: Vec<f32> = vb
            .split(|c: char| c.is_ascii_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .filter_map(parse_num)
            .collect();
        if nums.len() == 4 && nums[2] > 0.0 && nums[3] > 0.0 {
            w_f.get_or_insert(nums[2]);
            h_f.get_or_insert(nums[3]);
        }
    }

    let w = (w_f.unwrap_or(320.0).round() as u32).clamp(16, 1024);
    let h = (h_f.unwrap_or(180.0).round() as u32).clamp(16, 1024);
    let mut rgba = vec![255u8; (w as usize) * (h as usize) * 4];

    // Light panel background (#F8FAFC) with a 1px border (#CBD5E1).
    for y in 0..h {
        for x in 0..w {
            let border = x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            let (r, g, b) = if border {
                (203, 213, 225)
            } else {
                (248, 250, 252)
            };
            let idx = ((y * w + x) as usize) * 4;
            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = 255;
        }
    }

    let parse_color = |raw: &str| -> Option<(u8, u8, u8)> {
        let s = raw.trim();
        if s.eq_ignore_ascii_case("none") || s.is_empty() {
            return None;
        }
        if let Some(hex) = s.strip_prefix('#') {
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                return Some((r, g, b));
            }
            if hex.len() == 3 {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                return Some((r, g, b));
            }
        }
        match s.to_ascii_lowercase().as_str() {
            "black" => Some((20, 24, 31)),
            "white" => Some((255, 255, 255)),
            "red" => Some((220, 38, 38)),
            "green" => Some((22, 163, 74)),
            "blue" => Some((37, 99, 235)),
            "gray" | "grey" => Some((100, 116, 139)),
            _ => Some((51, 65, 85)),
        }
    };

    let put_px = |buf: &mut [u8], x: i32, y: i32, (r, g, b): (u8, u8, u8)| {
        if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
            let idx = ((y as u32 * w + x as u32) as usize) * 4;
            buf[idx] = r;
            buf[idx + 1] = g;
            buf[idx + 2] = b;
            buf[idx + 3] = 255;
        }
    };

    let draw_line = |buf: &mut [u8], x0: f32, y0: f32, x1: f32, y1: f32, col: (u8, u8, u8)| {
        let steps = ((x1 - x0).abs().max((y1 - y0).abs()).ceil() as i32).clamp(1, 4096);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let px = (x0 + (x1 - x0) * t).round() as i32;
            let py = (y0 + (y1 - y0) * t).round() as i32;
            put_px(buf, px, py, col);
        }
    };

    // Scan element tags in order and rasterize basic shapes (<rect>, <circle>, <line>).
    let mut cursor = svg_end + 1;
    while let Some(open) = text[cursor..].find('<') {
        let start = cursor + open;
        let Some(close) = text[start..].find('>') else {
            break;
        };
        let tag = &text[start..=start + close];
        cursor = start + close + 1;
        if tag.starts_with("</") || tag.starts_with("<!") || tag.starts_with("<?") {
            continue;
        }
        let tag_name = tag[1..]
            .split(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let fill_col = parse_attr(tag, "fill")
            .and_then(|v| parse_color(&v))
            .unwrap_or((59, 130, 246));
        let stroke_col = parse_attr(tag, "stroke").and_then(|v| parse_color(&v));

        match tag_name.as_str() {
            "rect" => {
                let rx = parse_attr(tag, "x")
                    .and_then(|v| parse_num(&v))
                    .unwrap_or(0.0);
                let ry = parse_attr(tag, "y")
                    .and_then(|v| parse_num(&v))
                    .unwrap_or(0.0);
                let rw = parse_attr(tag, "width")
                    .and_then(|v| parse_num(&v))
                    .unwrap_or(w as f32 * 0.5);
                let rh = parse_attr(tag, "height")
                    .and_then(|v| parse_num(&v))
                    .unwrap_or(h as f32 * 0.5);
                let x0 = (rx.round() as i32).clamp(0, w as i32);
                let y0 = (ry.round() as i32).clamp(0, h as i32);
                let x1 = ((rx + rw).round() as i32).clamp(0, w as i32);
                let y1 = ((ry + rh).round() as i32).clamp(0, h as i32);
                for py in y0..y1 {
                    for px in x0..x1 {
                        put_px(&mut rgba, px, py, fill_col);
                    }
                }
                if let Some(sc) = stroke_col {
                    draw_line(&mut rgba, rx, ry, rx + rw, ry, sc);
                    draw_line(&mut rgba, rx + rw, ry, rx + rw, ry + rh, sc);
                    draw_line(&mut rgba, rx + rw, ry + rh, rx, ry + rh, sc);
                    draw_line(&mut rgba, rx, ry + rh, rx, ry, sc);
                }
            }
            "circle" => {
                let cx = parse_attr(tag, "cx")
                    .and_then(|v| parse_num(&v))
                    .unwrap_or(w as f32 * 0.5);
                let cy = parse_attr(tag, "cy")
                    .and_then(|v| parse_num(&v))
                    .unwrap_or(h as f32 * 0.5);
                let r = parse_attr(tag, "r")
                    .and_then(|v| parse_num(&v))
                    .unwrap_or(16.0);
                let r2 = r * r;
                let x0 = ((cx - r).floor() as i32).max(0);
                let x1 = ((cx + r).ceil() as i32).min(w as i32 - 1);
                let y0 = ((cy - r).floor() as i32).max(0);
                let y1 = ((cy + r).ceil() as i32).min(h as i32 - 1);
                for py in y0..=y1 {
                    for px in x0..=x1 {
                        let dx = px as f32 - cx;
                        let dy = py as f32 - cy;
                        if dx * dx + dy * dy <= r2 {
                            put_px(&mut rgba, px, py, fill_col);
                        }
                    }
                }
            }
            "line" => {
                let x1 = parse_attr(tag, "x1")
                    .and_then(|v| parse_num(&v))
                    .unwrap_or(0.0);
                let y1 = parse_attr(tag, "y1")
                    .and_then(|v| parse_num(&v))
                    .unwrap_or(0.0);
                let x2 = parse_attr(tag, "x2")
                    .and_then(|v| parse_num(&v))
                    .unwrap_or(w as f32);
                let y2 = parse_attr(tag, "y2")
                    .and_then(|v| parse_num(&v))
                    .unwrap_or(h as f32);
                draw_line(
                    &mut rgba,
                    x1,
                    y1,
                    x2,
                    y2,
                    stroke_col.unwrap_or((30, 41, 59)),
                );
            }
            _ => {}
        }
    }

    Some((rgba, w, h))
}

/// Places an embedded image (cover logo or attached figure, job 073, RPT-06):
/// renders the decoded raster or static SVG image into the PDF page and prints
/// its caption below. If `bytes` is not a valid image, falls back to the
/// caption line alone.
#[allow(clippy::cast_precision_loss)]
fn image_block(p: &mut Pages, fonts: &mut Fonts, media_type: &str, alt: &str, bytes: &[u8]) {
    let look = Look {
        size: BODY,
        bold: false,
        ink: Ink::Grey,
    };
    if let Some(image) = decode_pdf_image(media_type, bytes) {
        let (px_w, px_h) = image.size();
        let raw_w = (px_w.max(1) as f32).clamp(24.0, CONTENT_W);
        let raw_h = (px_h.max(1) as f32).clamp(24.0, 240.0);
        let max_w = CONTENT_W.min(360.0);
        let max_h = 200.0;
        let scale = (max_w / raw_w).min(max_h / raw_h).min(1.0);
        let w = (raw_w * scale).max(24.0);
        let h = (raw_h * scale).max(24.0);
        if p.room() < h + leading(BODY) + 10.0 && p.y > MARGIN {
            p.new_page();
        }
        p.push(Item::RasterImage {
            x: MARGIN,
            y: p.y,
            w,
            h,
            image,
        });
        p.y += h + 4.0;
        text_block(
            p,
            fonts,
            &format!("Figure: {alt} ({} bytes)", bytes.len()),
            look,
            MARGIN,
            CONTENT_W,
        );
        p.y += 4.0;
    } else {
        text_block(
            p,
            fonts,
            &format!("Logo: {alt} ({} bytes)", bytes.len()),
            look,
            MARGIN,
            CONTENT_W,
        );
        p.y += 4.0;
    }
}

fn layout(doc: &ReportDoc, fonts: &mut Fonts) -> Pages {
    let mut p = Pages::new();
    for block in &doc.blocks {
        if p.truncated {
            break;
        }
        match block {
            Block::Heading(level, text) => heading(&mut p, fonts, *level, text),
            Block::Paragraph(text, style) => {
                let (bold, ink) = match style {
                    Style::Normal => (false, Ink::Black),
                    Style::Muted => (false, Ink::Grey),
                    Style::Draft => (true, Ink::Red),
                };
                let look = Look {
                    size: BODY,
                    bold,
                    ink,
                };
                text_block(&mut p, fonts, text, look, MARGIN, CONTENT_W);
                p.y += 4.0;
            }
            Block::Table { headers, rows } => table_block(&mut p, fonts, headers, rows),
            Block::Preformatted(text) => preformatted(&mut p, fonts, text),
            Block::Figure(figure) => figure_block(&mut p, fonts, figure),
            Block::Image {
                media_type,
                alt,
                bytes,
            } => image_block(&mut p, fonts, media_type, alt, bytes),
        }
    }
    if p.truncated {
        p.pages.truncate(MAX_PAGES);
        if let Some(last) = p.pages.last_mut() {
            last.push(Item::Text {
                x: MARGIN,
                y: BOTTOM,
                size: BODY,
                bold: true,
                ink: Ink::Red,
                text: "The report was cut off here: it is too long for one PDF.".to_owned(),
            });
        }
    }
    p
}

// ---- writing the file -----------------------------------------------------

/// Why a PDF could not be made.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// The embedded font could not be read. A build fault, not a user error.
    #[error("the report font could not be read")]
    Font,
    /// The PDF library refused the document.
    #[error("the PDF could not be written: {0}")]
    Write(String),
}

fn ink(i: Ink) -> rgb::Color {
    match i {
        Ink::Black => rgb::Color::new(20, 24, 31),
        Ink::Grey => rgb::Color::new(95, 105, 117),
        Ink::Red => rgb::Color::new(170, 40, 30),
    }
}

fn fill(color: rgb::Color) -> Fill {
    Fill {
        paint: color.into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::default(),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_text(
    surface: &mut krilla::surface::Surface<'_>,
    fonts: &mut Fonts,
    bold: bool,
    x: f32,
    y: f32,
    size: f32,
    color: rgb::Color,
    text: &str,
) {
    // Split the run into consecutive glyphs of the same face (Inter or the
    // CJK fallback) and draw each segment with its own font, advancing x by
    // the real widths. Japanese and Latin can share one line.
    struct Run {
        cjk: bool,
        glyphs: Vec<KrillaGlyph>,
        width: f32,
    }
    let mut runs: Vec<Run> = Vec::new();
    for (at, c) in text.char_indices() {
        let (cjk, gid, advance) = fonts.glyph(bold, c);
        let glyph = KrillaGlyph::new(
            GlyphId::new(gid),
            advance,
            0.0,
            0.0,
            0.0,
            at..at + c.len_utf8(),
            None,
        );
        if let Some(last) = runs.last_mut()
            && last.cjk == cjk
        {
            last.glyphs.push(glyph);
            last.width += advance * size;
        } else {
            runs.push(Run {
                cjk,
                glyphs: vec![glyph],
                width: advance * size,
            });
        }
    }
    surface.set_fill(Some(fill(color)));
    let mut cx = x;
    for run in &runs {
        let font = if run.cjk {
            fonts.cjk.font.clone()
        } else if bold {
            fonts.bold.font.clone()
        } else {
            fonts.regular.font.clone()
        };
        surface.draw_glyphs(Point::from_xy(cx, y), &run.glyphs, font, text, size, false);
        cx += run.width;
    }
}

fn rectangle(
    surface: &mut krilla::surface::Surface<'_>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    c: rgb::Color,
) {
    let Some(rect) = Rect::from_xywh(x, y, w, h) else {
        return;
    };
    let mut b = PathBuilder::new();
    b.push_rect(rect);
    if let Some(path) = b.finish() {
        surface.set_fill(Some(fill(c)));
        surface.set_stroke(None);
        surface.draw_path(&path);
    }
}

/// A canvas color token as a PDF color (the light, print palette).
fn paint_color(paint: Paint) -> rgb::Color {
    let hex = SvgTheme::Light.hex(paint).trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    rgb::Color::new(byte(0), byte(2), byte(4))
}

fn pdf_stroke(s: aegis_canvas::Stroke) -> PdfStroke {
    PdfStroke {
        paint: paint_color(s.paint).into(),
        width: s.width.max(0.3),
        dash: s.dashed.then(|| StrokeDash {
            array: vec![4.0, 3.0],
            offset: 0.0,
        }),
        ..PdfStroke::default()
    }
}

/// A rectangle with rounded corners as a path.
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<krilla::geom::Path> {
    const KAPPA: f32 = 0.552_284_7;
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut b = PathBuilder::new();
    if r < 0.25 {
        b.push_rect(Rect::from_xywh(x, y, w, h)?);
        return b.finish();
    }
    let k = r * KAPPA;
    let (right, bottom) = (x + w, y + h);
    b.move_to(x + r, y);
    b.line_to(right - r, y);
    b.cubic_to(right - r + k, y, right, y + r - k, right, y + r);
    b.line_to(right, bottom - r);
    b.cubic_to(
        right,
        bottom - r + k,
        right - r + k,
        bottom,
        right - r,
        bottom,
    );
    b.line_to(x + r, bottom);
    b.cubic_to(x + r - k, bottom, x, bottom - r + k, x, bottom - r);
    b.line_to(x, y + r);
    b.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    b.close();
    b.finish()
}

/// Draws one panel: a frame, then the display list clipped to it.
#[allow(clippy::many_single_char_names)]
fn draw_diagram(
    surface: &mut krilla::surface::Surface<'_>,
    fonts: &mut Fonts,
    (x, y, w, h): (f32, f32, f32, f32),
    list: &DisplayList,
) {
    let Some(frame) = Rect::from_xywh(x, y, w, h) else {
        return;
    };
    let mut b = PathBuilder::new();
    b.push_rect(frame);
    let Some(frame_path) = b.finish() else { return };
    surface.set_fill(Some(fill(paint_color(Paint::Panel))));
    surface.set_stroke(Some(pdf_stroke(aegis_canvas::Stroke::solid(
        Paint::Rule,
        0.5,
    ))));
    surface.draw_path(&frame_path);
    surface.set_stroke(None);

    surface.push_clip_path(&frame_path, &FillRule::default());
    for item in &list.items {
        match item {
            CanvasItem::Rect {
                rect,
                fill: f,
                stroke,
                radius,
                ..
            } => {
                let Some(path) = rounded_rect(
                    x + rect.origin.x,
                    y + rect.origin.y,
                    rect.size.w,
                    rect.size.h,
                    *radius,
                ) else {
                    continue;
                };
                surface.set_fill(f.map(|p| fill(paint_color(p))));
                surface.set_stroke(stroke.map(pdf_stroke));
                surface.draw_path(&path);
            }
            CanvasItem::Curve {
                from,
                c1,
                c2,
                to,
                stroke,
            } => {
                let mut b = PathBuilder::new();
                b.move_to(x + from.x, y + from.y);
                b.cubic_to(x + c1.x, y + c1.y, x + c2.x, y + c2.y, x + to.x, y + to.y);
                if let Some(path) = b.finish() {
                    surface.set_fill(None);
                    surface.set_stroke(Some(pdf_stroke(*stroke)));
                    surface.draw_path(&path);
                }
            }
            CanvasItem::Polyline { points, stroke } => {
                let mut b = PathBuilder::new();
                for (i, pt) in points.iter().enumerate() {
                    if i == 0 {
                        b.move_to(x + pt.x, y + pt.y);
                    } else {
                        b.line_to(x + pt.x, y + pt.y);
                    }
                }
                if let Some(path) = b.finish() {
                    surface.set_fill(None);
                    surface.set_stroke(Some(pdf_stroke(*stroke)));
                    surface.draw_path(&path);
                }
            }
            CanvasItem::Text {
                at,
                text,
                size,
                bold,
                paint,
                align,
            } => {
                surface.set_stroke(None);
                let left = match align {
                    Align::Start => at.x,
                    Align::End => at.x - fonts.width(*bold, text, *size),
                };
                draw_text(
                    surface,
                    fonts,
                    *bold,
                    x + left,
                    y + at.y,
                    *size,
                    paint_color(*paint),
                    text,
                );
            }
        }
    }
    surface.pop();
    surface.set_stroke(None);
}

fn metadata(doc: &ReportDoc, ctx: &ReportContext) -> Metadata {
    let secs = ctx.now_ms / 1000;
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let (y, mo, d) = crate::time::civil_from_days(days);
    let in_day = secs % 86_400;
    let date = DateTime::new(u16::try_from(y).unwrap_or(1970))
        .month(u8::try_from(mo).unwrap_or(1))
        .day(u8::try_from(d).unwrap_or(1))
        .hour(u8::try_from(in_day / 3600).unwrap_or(0))
        .minute(u8::try_from(in_day % 3600 / 60).unwrap_or(0))
        .second(u8::try_from(in_day % 60).unwrap_or(0));
    Metadata::new()
        .title(doc.title.clone())
        .creator("AegisTARA".to_owned())
        .producer("AegisTARA".to_owned())
        .authors(vec![ctx.user.clone()])
        .language(doc.language.clone())
        .creation_date(date)
}

fn render(
    doc: &ReportDoc,
    ctx: &ReportContext,
    pages: &Pages,
    fonts: &mut Fonts,
) -> Result<Vec<u8>, PdfError> {
    let mut document = Document::new();
    document.set_metadata(metadata(doc, ctx));
    let total = pages.pages.len();
    for (n, items) in pages.pages.iter().enumerate() {
        let size = Size::from_wh(PAGE_W, PAGE_H).ok_or(PdfError::Font)?;
        let mut page = document.start_page_with(PageSettings::new(size));
        let mut surface = page.surface();
        for item in items {
            match item {
                Item::Shade { x, y, w, h } => {
                    rectangle(&mut surface, *x, *y, *w, *h, rgb::Color::new(233, 237, 240));
                }
                Item::Rule { x1, x2, y } => {
                    rectangle(
                        &mut surface,
                        *x1,
                        *y,
                        x2 - x1,
                        0.4,
                        rgb::Color::new(195, 203, 211),
                    );
                }
                Item::Diagram { x, y, w, h, list } => {
                    draw_diagram(&mut surface, fonts, (*x, *y, *w, *h), list);
                }
                Item::RasterImage { x, y, w, h, image } => {
                    if let Some(img_size) = Size::from_wh(*w, *h) {
                        surface.push_transform(&Transform::from_translate(*x, *y));
                        surface.draw_image(image.clone(), img_size);
                        surface.pop();
                    }
                }
                Item::Text {
                    x,
                    y,
                    size,
                    bold,
                    ink: i,
                    text,
                } => {
                    draw_text(&mut surface, fonts, *bold, *x, *y, *size, ink(*i), text);
                }
            }
        }
        // Footer: the watermark on the left, "Page n of N" on the right.
        let foot_y = PAGE_H - MARGIN + 6.0;
        rectangle(
            &mut surface,
            MARGIN,
            foot_y - 12.0,
            CONTENT_W,
            0.4,
            rgb::Color::new(195, 203, 211),
        );
        draw_text(
            &mut surface,
            fonts,
            false,
            MARGIN,
            foot_y,
            7.5,
            ink(Ink::Grey),
            &doc.watermark,
        );
        let label = format!("Page {} of {total}", n + 1);
        let w = fonts.width(false, &label, 7.5);
        draw_text(
            &mut surface,
            fonts,
            false,
            PAGE_W - MARGIN - w,
            foot_y,
            7.5,
            ink(Ink::Grey),
            &label,
        );
        surface.finish();
        page.finish();
    }
    document
        .finish()
        .map_err(|e| PdfError::Write(format!("{e:?}")))
}

/// The report as PDF bytes with every section and no filters. Time and
/// memory O(project size).
///
/// # Errors
///
/// [`PdfError`] if the font or the PDF library fails (neither depends on the
/// project).
pub fn report_pdf(ws: &Workspace, ctx: &ReportContext) -> Result<Vec<u8>, PdfError> {
    report_pdf_with_options(ws, ctx, &ReportOptions::default())
}

/// The report as PDF bytes for a profile (sections, filters, language,
/// branding). Time and memory O(project size).
///
/// # Errors
///
/// [`PdfError`] if the font or the PDF library fails.
pub fn report_pdf_with_options(
    ws: &Workspace,
    ctx: &ReportContext,
    opts: &ReportOptions,
) -> Result<Vec<u8>, PdfError> {
    let doc = super::doc::report_doc_with_options(ws, ctx, opts);
    let mut fonts = Fonts::new()?;
    let pages = layout(&doc, &mut fonts);
    render(&doc, ctx, &pages, &mut fonts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cjk_fallback_maps_japanese_without_a_question_mark() {
        let mut fonts = Fonts::new().expect("fonts load");
        // Japanese glyphs come from the CJK face, not a `?` (L10N-04).
        for c in ['日', '本', '語', 'レ', 'ポ', 'ー', 'ト'] {
            let (cjk, gid, _) = fonts.glyph(false, c);
            assert!(cjk, "{c} should use the CJK face");
            assert_ne!(gid, 0, "{c} should map to a real glyph");
        }
        // Latin still comes from Inter.
        let (cjk, gid, _) = fonts.glyph(false, 'A');
        assert!(!cjk);
        assert_ne!(gid, 0);
        // The width of a mixed run is the sum of both faces.
        let mixed = fonts.width(false, "A日", 10.0);
        let a = fonts.width(false, "A", 10.0);
        let hi = fonts.width(false, "日", 10.0);
        assert!((mixed - (a + hi)).abs() < 0.001, "{mixed} vs {a}+{hi}");
    }
}
