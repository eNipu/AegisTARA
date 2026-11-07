//! Binary figure attachments (job 073, `ARC-01`, `RPT-06`): PNG, JPEG, and
//! sanitized static SVG validation, dimensions, and size caps.
//!
//! Every figure attached to a project is checked before it enters storage:
//! - PNG files must carry a valid PNG signature, an `IHDR` first chunk with
//!   dimensions in `1..=16_384`, and an `IEND` trailer.
//! - JPEG files must start with `SOI` (`0xFF 0xD8`), end with `EOI`
//!   (`0xFF 0xD9`), and contain a baseline/progressive `SOF` frame marker with
//!   dimensions in `1..=16_384`.
//! - SVG files must be UTF-8 XML with a root `<svg>` element and are sanitized
//!   to static vector markup: `<!DOCTYPE` / `<!ENTITY`, `<script>`,
//!   `<foreignObject>`, `<iframe>`, `<object>`, `<embed>`, `<image>`,
//!   `<animate*>`, `<set>`, `on*` event attributes, and external URLs in
//!   `href` / `xlink:href` / `style` are rejected.

use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::Hash;
use crate::ids::ProjectId;

/// Largest figure file accepted by default (5 MiB, `ARC-01`, R3).
pub const MAX_FIGURE_BYTES: usize = 5 * 1024 * 1024;

/// Most figures attached to a single project by default (`ARC-01`, R4).
pub const MAX_FIGURES_PER_PROJECT: usize = 64;

/// Largest combined byte size of all figures in a single project by default
/// (50 MiB, `ARC-01`, R3).
pub const MAX_PROJECT_FIGURES_BYTES: u64 = 50 * 1024 * 1024;

/// Longest figure caption in Unicode scalar values (R3).
pub const MAX_FIGURE_CAPTION_CHARS: usize = 512;

/// Longest figure target element reference in bytes (R3).
pub const MAX_FIGURE_TARGET_BYTES: usize = 128;

/// Maximum width or height of a figure in pixels/units (R3).
pub const MAX_IMAGE_DIMENSION: u32 = 16_384;

/// Default SVG width when neither `width` nor `viewBox` specifies one.
const DEFAULT_SVG_WIDTH: u32 = 800;

/// Default SVG height when neither `height` nor `viewBox` specifies one.
const DEFAULT_SVG_HEIGHT: u32 = 600;

/// Supported figure media types (`ARC-01`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FigureMedia {
    /// `image/png`.
    Png,
    /// `image/jpeg`.
    Jpeg,
    /// `image/svg+xml` (sanitized static SVG).
    Svg,
}

impl FigureMedia {
    /// Canonical MIME type string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            FigureMedia::Png => "image/png",
            FigureMedia::Jpeg => "image/jpeg",
            FigureMedia::Svg => "image/svg+xml",
        }
    }

    /// Parses a MIME type or file extension shorthand.
    pub fn parse(media_type: &str) -> Result<Self, FigureError> {
        match media_type.trim().to_ascii_lowercase().as_str() {
            "image/png" | "png" => Ok(FigureMedia::Png),
            "image/jpeg" | "image/jpg" | "jpeg" | "jpg" => Ok(FigureMedia::Jpeg),
            "image/svg+xml" | "image/svg" | "svg" => Ok(FigureMedia::Svg),
            _ => Err(FigureError::UnsupportedMediaType),
        }
    }
}

/// Metadata of a stored figure attachment (`ARC-01`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FigureMeta {
    /// Identity (UUIDv7, ordered by creation time).
    pub id: Uuid,
    /// Owning project.
    pub project: ProjectId,
    /// Human-readable caption (trimmed, up to [`MAX_FIGURE_CAPTION_CHARS`]).
    pub caption: String,
    /// Optional element binding (`""` for project-level, or `<kind>:<uuid>`).
    pub target: String,
    /// Canonical MIME type (`image/png`, `image/jpeg`, or `image/svg+xml`).
    pub media_type: String,
    /// Intrinsic width in pixels/SVG units (`1..=MAX_IMAGE_DIMENSION`).
    pub width: u32,
    /// Intrinsic height in pixels/SVG units (`1..=MAX_IMAGE_DIMENSION`).
    pub height: u32,
    /// Stored byte length (after SVG sanitization when applicable).
    pub byte_len: u64,
    /// SHA-256 digest of the stored bytes.
    pub sha256: Hash,
    /// Subject that uploaded the figure.
    pub created_by: String,
    /// Upload timestamp in milliseconds since the Unix epoch.
    pub created_ms: u64,
}

/// Validated (and for SVG, sanitized) figure payload ready for storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedFigure {
    /// Canonical media type.
    pub media: FigureMedia,
    /// Intrinsic width.
    pub width: u32,
    /// Intrinsic height.
    pub height: u32,
    /// Stored bytes (sanitized for SVG).
    pub bytes: Vec<u8>,
    /// SHA-256 of `bytes`.
    pub sha256: Hash,
}

/// Why a figure attachment was rejected.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FigureError {
    /// Empty payload.
    #[error("figure file is empty")]
    Empty,
    /// Exceeds the per-figure size cap.
    #[error("figure exceeds maximum size of {max} bytes")]
    TooLarge {
        /// Maximum allowed bytes.
        max: usize,
    },
    /// Media type is not PNG, JPEG, or SVG.
    #[error("figure media type must be image/png, image/jpeg, or image/svg+xml")]
    UnsupportedMediaType,
    /// Caption is too long or contains control characters.
    #[error("figure caption is invalid: {0}")]
    InvalidCaption(&'static str),
    /// Target element reference is malformed.
    #[error("figure target reference is invalid: {0}")]
    InvalidTarget(&'static str),
    /// Image header or structure is malformed.
    #[error("invalid image data: {0}")]
    InvalidImage(&'static str),
    /// Image dimensions are zero or exceed [`MAX_IMAGE_DIMENSION`].
    #[error("image dimensions {width}x{height} are outside 1..={MAX_IMAGE_DIMENSION}")]
    InvalidDimensions {
        /// Parsed width.
        width: u32,
        /// Parsed height.
        height: u32,
    },
    /// SVG contains active, external, or disallowed constructs.
    #[error("unsafe or unsupported SVG construct: {0}")]
    UnsafeSvg(&'static str),
}

/// Trims and validates a figure caption.
pub fn validate_caption(caption: &str) -> Result<String, FigureError> {
    let trimmed = caption.trim();
    if trimmed.chars().count() > MAX_FIGURE_CAPTION_CHARS {
        return Err(FigureError::InvalidCaption(
            "caption exceeds 512 characters",
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(FigureError::InvalidCaption(
            "caption contains control characters",
        ));
    }
    Ok(trimmed.to_owned())
}

/// Validates an optional target element reference (`""` for project-level or
/// `<kind>:<uuid>` where `<uuid>` is a canonical UUIDv7).
pub fn validate_target(target: &str) -> Result<String, FigureError> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.len() > MAX_FIGURE_TARGET_BYTES {
        return Err(FigureError::InvalidTarget("target is too long"));
    }
    if trimmed == "project" {
        return Ok(String::new());
    }
    // The item definition's description and scope (ARC-05) have no ID.
    if matches!(trimmed, "item_description" | "item_scope") {
        return Ok(trimmed.to_owned());
    }
    let Some((kind, id_str)) = trimmed.split_once(':') else {
        return Err(FigureError::InvalidTarget(
            "target must be empty or <kind>:<uuid>",
        ));
    };
    let canonical_kind = match kind {
        "component" | "interface" | "data_flow" | "trust_boundary" | "asset" | "damage"
        | "threat" | "tree" | "control" | "goal" | "claim" => kind,
        // Accept target strings persisted by early 073 builds.
        "damage_scenario" => "damage",
        "threat_scenario" => "threat",
        "attack_tree" => "tree",
        "security_control" => "control",
        "cybersecurity_goal" => "goal",
        "cybersecurity_claim" => "claim",
        _ => return Err(FigureError::InvalidTarget("unknown target element kind")),
    };
    let uuid = Uuid::parse_str(id_str)
        .map_err(|_| FigureError::InvalidTarget("target ID is not a valid UUID"))?;
    if uuid.get_version_num() != 7 || uuid.to_string() != id_str {
        return Err(FigureError::InvalidTarget(
            "target ID must be a canonical lowercase UUIDv7",
        ));
    }
    Ok(format!("{canonical_kind}:{uuid}"))
}

/// Validates (and for SVG, sanitizes) a raw figure byte slice against `max_bytes`.
pub fn validate_and_sanitize_figure(
    media_type: &str,
    raw: &[u8],
    max_bytes: usize,
) -> Result<ValidatedFigure, FigureError> {
    if raw.is_empty() {
        return Err(FigureError::Empty);
    }
    if raw.len() > max_bytes {
        return Err(FigureError::TooLarge { max: max_bytes });
    }
    let media = FigureMedia::parse(media_type)?;
    let (bytes, width, height) = match media {
        FigureMedia::Png => {
            let (w, h) = parse_png_dimensions(raw)?;
            (raw.to_vec(), w, h)
        }
        FigureMedia::Jpeg => {
            let (w, h) = parse_jpeg_dimensions(raw)?;
            (raw.to_vec(), w, h)
        }
        FigureMedia::Svg => sanitize_svg(raw)?,
    };
    if width == 0 || height == 0 || width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return Err(FigureError::InvalidDimensions { width, height });
    }
    if bytes.len() > max_bytes {
        return Err(FigureError::TooLarge { max: max_bytes });
    }
    let sha256: Hash = Sha256::digest(&bytes).into();
    Ok(ValidatedFigure {
        media,
        width,
        height,
        bytes,
        sha256,
    })
}

fn parse_png_dimensions(bytes: &[u8]) -> Result<(u32, u32), FigureError> {
    const PNG_SIG: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    const IEND_TRAILER: &[u8; 12] = b"\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    if bytes.len() < 33 || &bytes[..8] != PNG_SIG {
        return Err(FigureError::InvalidImage("missing PNG signature"));
    }
    let ihdr_len = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if ihdr_len != 13 || &bytes[12..16] != b"IHDR" {
        return Err(FigureError::InvalidImage("missing PNG IHDR chunk"));
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    if !bytes.ends_with(IEND_TRAILER) {
        return Err(FigureError::InvalidImage("missing PNG IEND trailer"));
    }
    Ok((width, height))
}

fn parse_jpeg_dimensions(bytes: &[u8]) -> Result<(u32, u32), FigureError> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Err(FigureError::InvalidImage("missing JPEG SOI marker"));
    }
    if bytes[bytes.len() - 2] != 0xFF || bytes[bytes.len() - 1] != 0xD9 {
        return Err(FigureError::InvalidImage("missing JPEG EOI marker"));
    }
    let mut i = 2usize;
    let mut dims = None;
    while i + 1 < bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        while i < bytes.len() && bytes[i] == 0xFF {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let marker = bytes[i];
        i += 1;
        if marker == 0xD9 || marker == 0xDA {
            break;
        }
        if marker == 0x00 || (0xD0..=0xD8).contains(&marker) || marker == 0x01 {
            continue;
        }
        if i + 2 > bytes.len() {
            return Err(FigureError::InvalidImage("truncated JPEG segment length"));
        }
        let seg_len = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as usize;
        if seg_len < 2 || i + seg_len > bytes.len() {
            return Err(FigureError::InvalidImage("invalid JPEG segment length"));
        }
        let is_sof = matches!(
            marker,
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF
        );
        if is_sof {
            if seg_len < 7 {
                return Err(FigureError::InvalidImage("truncated JPEG SOF segment"));
            }
            let height = u32::from(u16::from_be_bytes([bytes[i + 3], bytes[i + 4]]));
            let width = u32::from(u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]));
            dims = Some((width, height));
            break;
        }
        i += seg_len;
    }
    dims.ok_or(FigureError::InvalidImage("missing JPEG SOF frame header"))
}

const ALLOWED_SVG_ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "text",
    "tspan",
    "defs",
    "linearGradient",
    "radialGradient",
    "stop",
    "clipPath",
    "mask",
    "marker",
    "pattern",
    "symbol",
    "use",
    "title",
    "desc",
];

const ALLOWED_SVG_ATTRS: &[&str] = &[
    "xmlns",
    "version",
    "viewBox",
    "preserveAspectRatio",
    "width",
    "height",
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "dx",
    "dy",
    "d",
    "points",
    "transform",
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-miterlimit",
    "opacity",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "text-anchor",
    "dominant-baseline",
    "letter-spacing",
    "id",
    "class",
    "clip-path",
    "clip-rule",
    "mask",
    "marker-start",
    "marker-mid",
    "marker-end",
    "gradientUnits",
    "gradientTransform",
    "spreadMethod",
    "offset",
    "stop-color",
    "stop-opacity",
    "fx",
    "fy",
    "patternUnits",
    "patternTransform",
    "markerUnits",
    "markerWidth",
    "markerHeight",
    "refX",
    "refY",
    "orient",
    "href",
    "xlink:href",
    "style",
];

/// Sanitizes an SVG document to static, self-contained markup and returns
/// `(sanitized_utf8_bytes, width, height)`.
pub fn sanitize_svg(raw: &[u8]) -> Result<(Vec<u8>, u32, u32), FigureError> {
    let text =
        std::str::from_utf8(raw).map_err(|_| FigureError::InvalidImage("SVG is not UTF-8"))?;
    let mut s = text.trim_start_matches('\u{feff}').trim();

    if let Some(rest) = s.strip_prefix("<?xml") {
        let Some(end) = rest.find("?>") else {
            return Err(FigureError::InvalidImage("unclosed XML declaration"));
        };
        s = rest[end + 2..].trim();
    }

    let lower = s.to_ascii_lowercase();
    if lower.contains("<!doctype") || lower.contains("<!entity") {
        return Err(FigureError::UnsafeSvg(
            "DOCTYPE and ENTITY declarations are forbidden",
        ));
    }
    if lower.contains("<?") {
        return Err(FigureError::UnsafeSvg(
            "XML processing instructions are forbidden",
        ));
    }
    if lower.contains("<![cdata[") {
        return Err(FigureError::UnsafeSvg("CDATA sections are forbidden"));
    }

    let mut out = String::with_capacity(s.len());
    let mut stack: Vec<&str> = Vec::new();
    let mut root_seen = false;
    let mut svg_width: Option<u32> = None;
    let mut svg_height: Option<u32> = None;
    let mut viewbox_dims: Option<(u32, u32)> = None;

    let mut pos = 0usize;
    let bytes = s.as_bytes();
    while pos < bytes.len() {
        if bytes[pos] == b'<' {
            if s[pos..].starts_with("<!--") {
                let Some(end) = s[pos + 4..].find("-->") else {
                    return Err(FigureError::InvalidImage("unclosed SVG comment"));
                };
                pos += 4 + end + 3;
                continue;
            }
            let Some(rel_end) = find_tag_end(&s[pos..]) else {
                return Err(FigureError::InvalidImage("unclosed SVG tag"));
            };
            let tag_content = s[pos + 1..pos + rel_end].trim();
            pos += rel_end + 1;

            if let Some(closing) = tag_content.strip_prefix('/') {
                let name = closing.trim();
                let Some(expected) = stack.pop() else {
                    return Err(FigureError::InvalidImage("unexpected closing SVG tag"));
                };
                if expected != name {
                    return Err(FigureError::InvalidImage("mismatched closing SVG tag"));
                }
                out.push_str("</");
                out.push_str(expected);
                out.push('>');
                if stack.is_empty() {
                    break;
                }
                continue;
            }

            let self_closing = tag_content.ends_with('/');
            let inner = if self_closing {
                tag_content[..tag_content.len() - 1].trim()
            } else {
                tag_content
            };
            let (elem_name, attrs_raw) = split_tag_name(inner)?;
            let Some(&canonical_elem) = ALLOWED_SVG_ELEMENTS.iter().find(|&&e| e == elem_name)
            else {
                return Err(FigureError::UnsafeSvg("disallowed SVG element"));
            };
            if !root_seen {
                if canonical_elem != "svg" {
                    return Err(FigureError::InvalidImage("root element must be <svg>"));
                }
                root_seen = true;
            } else if stack.is_empty() {
                return Err(FigureError::InvalidImage("multiple root elements in SVG"));
            }
            if stack.len() >= 64 {
                return Err(FigureError::UnsafeSvg("SVG nesting depth exceeds 64"));
            }

            let attrs = parse_and_validate_attrs(attrs_raw)?;
            out.push('<');
            out.push_str(canonical_elem);
            let mut has_xmlns = false;
            for (k, v) in &attrs {
                if canonical_elem == "svg" {
                    if *k == "xmlns" {
                        has_xmlns = true;
                    } else if *k == "width" {
                        svg_width = parse_svg_length(v);
                    } else if *k == "height" {
                        svg_height = parse_svg_length(v);
                    } else if *k == "viewBox" {
                        viewbox_dims = parse_viewbox(v);
                    }
                }
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                escape_xml_attr(&mut out, v);
                out.push('"');
            }
            if canonical_elem == "svg" && !has_xmlns {
                out.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
            }
            if self_closing {
                out.push_str("/>");
            } else {
                out.push('>');
                stack.push(canonical_elem);
            }
        } else {
            let next_lt = s[pos..].find('<').unwrap_or(s.len() - pos);
            let text_chunk = &s[pos..pos + next_lt];
            pos += next_lt;
            if stack.is_empty() {
                if !text_chunk.trim().is_empty() {
                    return Err(FigureError::InvalidImage("text outside root <svg> element"));
                }
            } else {
                escape_xml_text(&mut out, text_chunk);
            }
        }
    }

    if !root_seen || !stack.is_empty() {
        return Err(FigureError::InvalidImage(
            "incomplete or unclosed <svg> document",
        ));
    }
    if !s[pos..].trim().is_empty() {
        // Allow trailing comments only.
        let mut tail = s[pos..].trim();
        while let Some(rest) = tail.strip_prefix("<!--") {
            let Some(end) = rest.find("-->") else {
                return Err(FigureError::InvalidImage("unclosed trailing SVG comment"));
            };
            tail = rest[end + 3..].trim();
        }
        if !tail.is_empty() {
            return Err(FigureError::InvalidImage("trailing content after </svg>"));
        }
    }

    let width = svg_width
        .or_else(|| viewbox_dims.map(|(w, _)| w))
        .unwrap_or(DEFAULT_SVG_WIDTH);
    let height = svg_height
        .or_else(|| viewbox_dims.map(|(_, h)| h))
        .unwrap_or(DEFAULT_SVG_HEIGHT);

    Ok((out.into_bytes(), width, height))
}

fn find_tag_end(s: &str) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (idx, b) in s.bytes().enumerate().skip(1) {
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => quote = Some(b),
            None if b == b'<' => return None,
            None if b == b'>' => return Some(idx),
            None => {}
        }
    }
    None
}

fn split_tag_name(s: &str) -> Result<(&str, &str), FigureError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(FigureError::InvalidImage("empty SVG tag"));
    }
    let split = s.find(|c: char| c.is_ascii_whitespace()).unwrap_or(s.len());
    Ok((&s[..split], s[split..].trim()))
}

fn parse_and_validate_attrs(mut s: &str) -> Result<Vec<(&'static str, String)>, FigureError> {
    let mut out = Vec::new();
    while !s.is_empty() {
        s = s.trim_start();
        if s.is_empty() {
            break;
        }
        let Some(eq_idx) = s.find('=') else {
            return Err(FigureError::InvalidImage("malformed SVG attribute"));
        };
        let raw_name = s[..eq_idx].trim();
        if raw_name.is_empty() {
            return Err(FigureError::InvalidImage("empty SVG attribute name"));
        }
        let lower_name = raw_name.to_ascii_lowercase();
        if lower_name.starts_with("on") {
            return Err(FigureError::UnsafeSvg(
                "SVG event handler attributes are forbidden",
            ));
        }
        s = s[eq_idx + 1..].trim_start();
        let Some(quote) = s.as_bytes().first().copied() else {
            return Err(FigureError::InvalidImage("missing SVG attribute value"));
        };
        if quote != b'"' && quote != b'\'' {
            return Err(FigureError::InvalidImage("unquoted SVG attribute value"));
        }
        let rest = &s[1..];
        let Some(end_q) = rest.find(char::from(quote)) else {
            return Err(FigureError::InvalidImage(
                "unterminated SVG attribute value",
            ));
        };
        let raw_val = &rest[..end_q];
        s = &rest[end_q + 1..];

        if lower_name.starts_with("xmlns:") {
            continue;
        }
        let Some(&canonical_attr) = ALLOWED_SVG_ATTRS.iter().find(|&&a| a == raw_name) else {
            return Err(FigureError::UnsafeSvg("disallowed SVG attribute"));
        };
        let val = decode_xml_entities(raw_val)?;
        validate_attr_value(canonical_attr, &val)?;
        out.push((canonical_attr, val));
    }
    Ok(out)
}

fn decode_xml_entities(s: &str) -> Result<String, FigureError> {
    if s.chars()
        .any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
    {
        return Err(FigureError::InvalidImage(
            "control character in SVG attribute",
        ));
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let Some(semi) = after.find(';') else {
            return Err(FigureError::InvalidImage("unterminated XML entity in SVG"));
        };
        let ent = &after[..semi];
        let decoded = match ent {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                let code = u32::from_str_radix(&ent[2..], 16)
                    .map_err(|_| FigureError::InvalidImage("invalid hex character reference"))?;
                char::from_u32(code).ok_or(FigureError::InvalidImage(
                    "invalid Unicode scalar reference",
                ))?
            }
            _ if ent.starts_with('#') => {
                let code = ent[1..].parse::<u32>().map_err(|_| {
                    FigureError::InvalidImage("invalid decimal character reference")
                })?;
                char::from_u32(code).ok_or(FigureError::InvalidImage(
                    "invalid Unicode scalar reference",
                ))?
            }
            _ => return Err(FigureError::UnsafeSvg("custom XML entities are forbidden")),
        };
        if decoded.is_control() && !matches!(decoded, '\t' | '\n' | '\r') {
            return Err(FigureError::UnsafeSvg(
                "control character reference in SVG attribute",
            ));
        }
        out.push(decoded);
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn validate_attr_value(attr: &str, val: &str) -> Result<(), FigureError> {
    let trimmed = val.trim();
    if attr == "xmlns" {
        if trimmed == "http://www.w3.org/2000/svg" {
            return Ok(());
        }
        return Err(FigureError::UnsafeSvg(
            "SVG xmlns must be http://www.w3.org/2000/svg",
        ));
    }
    let lower: String = trimmed
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();

    if lower.contains("javascript:")
        || lower.contains("vbscript:")
        || lower.contains("data:")
        || lower.contains("http:")
        || lower.contains("https:")
        || lower.contains("file:")
        || lower.contains("//")
        || lower.contains("expression(")
        || lower.contains("@import")
    {
        return Err(FigureError::UnsafeSvg(
            "external or script URLs are forbidden in SVG",
        ));
    }

    if matches!(attr, "href" | "xlink:href") && (!trimmed.starts_with('#') || trimmed.len() < 2) {
        return Err(FigureError::UnsafeSvg(
            "SVG href must be a local fragment (#id)",
        ));
    }

    if let Some(idx) = lower.find("url(") {
        let after = &lower[idx + 4..];
        let inner = after.trim_start_matches(['"', '\'']);
        if !inner.starts_with('#') {
            return Err(FigureError::UnsafeSvg(
                "SVG url(...) reference must be a local fragment (#id)",
            ));
        }
    }
    Ok(())
}

fn parse_svg_length(val: &str) -> Option<u32> {
    let trimmed = val.trim();
    let num_str = trimmed
        .strip_suffix("px")
        .or_else(|| trimmed.strip_suffix("pt"))
        .unwrap_or(trimmed)
        .trim();
    if num_str.ends_with('%') {
        return None;
    }
    let parsed: f64 = num_str.parse().ok()?;
    if !parsed.is_finite() || parsed <= 0.0 || parsed > f64::from(MAX_IMAGE_DIMENSION) {
        return None;
    }
    Some(parsed.round() as u32)
}

fn parse_viewbox(val: &str) -> Option<(u32, u32)> {
    let nums: Vec<f64> = val
        .split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<f64>().ok())
        .collect::<Option<Vec<f64>>>()?;
    if nums.len() != 4 {
        return None;
    }
    let w = nums[2];
    let h = nums[3];
    if !w.is_finite()
        || !h.is_finite()
        || w <= 0.0
        || h <= 0.0
        || w > f64::from(MAX_IMAGE_DIMENSION)
        || h > f64::from(MAX_IMAGE_DIMENSION)
    {
        return None;
    }
    Some((w.round() as u32, h.round() as u32))
}

fn escape_xml_attr(out: &mut String, val: &str) {
    for c in val.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
}

fn escape_xml_text(out: &mut String, val: &str) {
    for c in val.chars() {
        if c.is_control() && !matches!(c, '\t' | '\n' | '\r') {
            continue;
        }
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        out.extend_from_slice(&13u32.to_be_bytes());
        out.extend_from_slice(b"IHDR");
        out.extend_from_slice(&width.to_be_bytes());
        out.extend_from_slice(&height.to_be_bytes());
        out.extend_from_slice(&[8, 6, 0, 0, 0]);
        out.extend_from_slice(&0u32.to_be_bytes()); // CRC
        out.extend_from_slice(b"\x00\x00\x00\x00IEND\xae\x42\x60\x82");
        out
    }

    fn sample_jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 0x08];
        out.extend_from_slice(&height.to_be_bytes());
        out.extend_from_slice(&width.to_be_bytes());
        out.extend_from_slice(&[3, 1, 0x11, 0, 2, 0x11, 1, 3, 0x11, 1]);
        out.extend_from_slice(&[0xFF, 0xD9]);
        out
    }

    #[test]
    fn validates_png_and_jpeg_dimensions_and_rejects_corrupt() {
        let png = sample_png(640, 480);
        let v = validate_and_sanitize_figure("image/png", &png, MAX_FIGURE_BYTES).expect("png");
        assert_eq!((v.width, v.height), (640, 480));
        assert_eq!(v.media, FigureMedia::Png);

        let jpg = sample_jpeg(1024, 768);
        let v = validate_and_sanitize_figure("image/jpeg", &jpg, MAX_FIGURE_BYTES).expect("jpeg");
        assert_eq!((v.width, v.height), (1024, 768));
        assert_eq!(v.media, FigureMedia::Jpeg);

        let too_wide = sample_png(MAX_IMAGE_DIMENSION + 1, 100);
        assert!(matches!(
            validate_and_sanitize_figure("image/png", &too_wide, MAX_FIGURE_BYTES),
            Err(FigureError::InvalidDimensions { .. })
        ));
    }

    #[test]
    fn sanitizes_static_svg_and_rejects_active_or_external_constructs() {
        let clean = br##"<svg viewBox="0 0 400 300"><g fill="#1e293b"><rect x="10" y="10" width="100" height="50"/><text x="20" y="35">Gateway ECU</text></g></svg>"##;
        let v =
            validate_and_sanitize_figure("image/svg+xml", clean, MAX_FIGURE_BYTES).expect("svg");
        assert_eq!((v.width, v.height), (400, 300));
        let text = std::str::from_utf8(&v.bytes).expect("utf8");
        assert!(text.contains("xmlns=\"http://www.w3.org/2000/svg\""));

        let bad_cases: &[&[u8]] = &[
            b"<svg><script>alert(1)</script></svg>",
            b"<svg onload=\"alert(1)\"><rect width=\"10\" height=\"10\"/></svg>",
            b"<svg><foreignObject><div>html</div></foreignObject></svg>",
            b"<svg><use href=\"https://evil.example/x.svg#a\"/></svg>",
            b"<svg><rect style=\"fill:url(https://evil.example/a)\"/></svg>",
            b"<!DOCTYPE svg [<!ENTITY xxe \"boom\">]><svg>&xxe;</svg>",
            b"<svg><animate attributeName=\"x\" from=\"0\" to=\"10\"/></svg>",
        ];
        for bad in bad_cases {
            assert!(
                validate_and_sanitize_figure("image/svg+xml", bad, MAX_FIGURE_BYTES).is_err(),
                "expected rejection for: {}",
                String::from_utf8_lossy(bad)
            );
        }
    }

    #[test]
    fn validates_caption_and_target() {
        assert_eq!(
            validate_caption("  Item architecture  ").expect("ok"),
            "Item architecture"
        );
        assert!(validate_caption("bad\ncaption").is_err());

        assert_eq!(validate_target("").expect("ok"), "");
        let id = Uuid::now_v7();
        assert_eq!(
            validate_target(&format!("component:{id}")).expect("ok"),
            format!("component:{id}")
        );
        assert!(validate_target("unknown:123").is_err());
        // Targets persisted by the early 073 builds are normalized to the
        // canonical kind names the client emits and parses.
        assert_eq!(
            validate_target(&format!("damage_scenario:{id}")).expect("ok"),
            format!("damage:{id}")
        );
        assert_eq!(
            validate_target(&format!("threat_scenario:{id}")).expect("ok"),
            format!("threat:{id}")
        );
        assert_eq!(
            validate_target(&format!("attack_tree:{id}")).expect("ok"),
            format!("tree:{id}")
        );
        assert_eq!(
            validate_target(&format!("security_control:{id}")).expect("ok"),
            format!("control:{id}")
        );
    }
}
