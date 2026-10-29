//! Excel (`.xlsx`) TARA import with column mapping, dry-run preview, unmapped
//! row reporting, and atomic batch application with `ProvenanceSource::Import`
//! (job 080, INT-02).
//!
//! Reads Office Open XML SpreadsheetML archives (both Stored and Deflate ZIP
//! entries) under strict size, entry, and cell caps so an untrusted file
//! cannot exhaust memory or CPU (rule P2).

use std::collections::HashMap;
use std::io::Read as _;

use aegis_model::{
    AssetId, ClaimId, ComponentId, ControlId, DamageId, GoalId, Methodology, Op, Project,
    PropertySet, Provenance, RiskTreatment, ThreatId,
};

/// Maximum `.xlsx` archive size in bytes (16 MiB).
pub const MAX_XLSX_BYTES: usize = 16 * 1024 * 1024;
/// Maximum number of ZIP entries in a `.xlsx` archive.
pub const MAX_XLSX_ENTRIES: usize = 64;
/// Maximum uncompressed size of any single XML entry (16 MiB).
pub const MAX_XLSX_ENTRY_BYTES: usize = 16 * 1024 * 1024;
/// Maximum total uncompressed XML retained from a workbook (32 MiB).
pub const MAX_XLSX_TOTAL_XML_BYTES: usize = 32 * 1024 * 1024;
/// Maximum number of shared strings in `xl/sharedStrings.xml`.
pub const MAX_SHARED_STRINGS: usize = 65_536;
/// Maximum number of rows per worksheet.
pub const MAX_XLSX_ROWS: usize = 10_000;
/// Maximum number of columns per row.
pub const MAX_XLSX_COLS: usize = 64;

/// Errors returned while reading or previewing an `.xlsx` workbook.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ExcelError {
    /// Archive exceeds [`MAX_XLSX_BYTES`] or an entry exceeds [`MAX_XLSX_ENTRY_BYTES`].
    #[error("the workbook exceeds the size limit ({0})")]
    TooLarge(&'static str),
    /// The file is not a valid `.xlsx` ZIP archive.
    #[error("not a valid .xlsx archive: {0}")]
    InvalidArchive(&'static str),
    /// The workbook contains no worksheets.
    #[error("the workbook contains no worksheets")]
    NoSheets,
    /// The requested worksheet name does not exist in the workbook.
    #[error("worksheet \"{0}\" was not found in the workbook")]
    SheetNotFound(String),
    /// A workbook exceeds a supported row, column, or shared-string count.
    #[error("the workbook exceeds a supported worksheet limit ({0})")]
    WorksheetLimit(&'static str),
    /// A row could not be rolled back while building the dry-run preview.
    #[error("could not restore the dry-run project after a rejected row")]
    SimulationRollback,
    /// The import would exceed the one-edit operation limit.
    #[error("the import produces more than {0} changes and cannot be applied as one step")]
    TooManyOperations(usize),
}

/// One parsed worksheet from an `.xlsx` file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetData {
    /// Worksheet title from `xl/workbook.xml`.
    pub name: String,
    /// Header cells from the first non-empty row.
    pub headers: Vec<String>,
    /// Data rows after the header row: `(1-based row number, cell values)`.
    pub rows: Vec<(usize, Vec<String>)>,
}

/// All worksheets parsed from an `.xlsx` file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkbookData {
    /// Worksheets in workbook order.
    pub sheets: Vec<SheetData>,
}

/// Column-to-element mapping for importing a TARA worksheet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExcelMapping {
    /// Worksheet name to import, or `None` to use the first sheet.
    pub sheet: Option<String>,
    /// Column index for Component name.
    pub component_col: Option<usize>,
    /// Column index for Asset name.
    pub asset_col: Option<usize>,
    /// Column index for comma-separated security properties.
    pub properties_col: Option<usize>,
    /// Column index for Damage scenario name.
    pub damage_col: Option<usize>,
    /// Column index for Impact rating (`Category: Level` or level label).
    pub impact_col: Option<usize>,
    /// Column index for Threat scenario name.
    pub threat_col: Option<usize>,
    /// Column index for Threat category label.
    pub category_col: Option<usize>,
    /// Column index for Direct feasibility / likelihood label.
    pub feasibility_col: Option<usize>,
    /// Column index for Treatment decision (`Avoid`, `Reduce`, `Share`, `Retain`).
    pub treatment_col: Option<usize>,
    /// Column index for Security control name.
    pub control_col: Option<usize>,
    /// Column index for Cybersecurity goal name (or `Goal: ...` / `Claim: ...`).
    pub goal_col: Option<usize>,
    /// Column index for Cybersecurity claim statement.
    pub claim_col: Option<usize>,
}

impl ExcelMapping {
    /// Automatically detects column positions from a header row (supports
    /// English and Japanese column headings).
    #[must_use]
    pub fn auto_detect(headers: &[String]) -> Self {
        let mut m = ExcelMapping::default();
        for (idx, raw) in headers.iter().enumerate() {
            let h = raw.trim().to_lowercase();
            match h.as_str() {
                "component" | "components" | "コンポーネント" => {
                    m.component_col.get_or_insert(idx);
                }
                "asset" | "assets" | "資産" => {
                    m.asset_col.get_or_insert(idx);
                }
                "properties" | "security properties" | "property" | "セキュリティ特性" | "特性" =>
                {
                    m.properties_col.get_or_insert(idx);
                }
                "damage"
                | "damage scenario"
                | "damage scenarios"
                | "deciding damage"
                | "deciding damage scenario"
                | "deciding harm"
                | "harm"
                | "harms"
                | "損害"
                | "損害シナリオ"
                | "ダメージシナリオ"
                | "決定する損害"
                | "決定するダメージシナリオ" => {
                    m.damage_col.get_or_insert(idx);
                }
                "impact" | "impacts" | "影響" | "影響度" => {
                    m.impact_col.get_or_insert(idx);
                }
                "threat" | "threat scenario" | "threat scenarios" | "脅威" | "脅威シナリオ" =>
                {
                    m.threat_col.get_or_insert(idx);
                }
                "category" | "threat category" | "stride" | "カテゴリ" | "脅威カテゴリ" =>
                {
                    m.category_col.get_or_insert(idx);
                }
                "feasibility" | "attack feasibility" | "likelihood" | "attack likelihood"
                | "攻撃可能性" | "発生可能性" => {
                    m.feasibility_col.get_or_insert(idx);
                }
                "treatment" | "risk treatment" | "対応" | "リスク対応" => {
                    m.treatment_col.get_or_insert(idx);
                }
                "control"
                | "controls"
                | "security control"
                | "security controls"
                | "対策"
                | "セキュリティ対策" => {
                    m.control_col.get_or_insert(idx);
                }
                "goal"
                | "goals"
                | "cybersecurity goal"
                | "cybersecurity goals"
                | "security requirement"
                | "security requirements"
                | "goal or claim"
                | "目標"
                | "サイバーセキュリティ目標"
                | "目標または主張" => {
                    m.goal_col.get_or_insert(idx);
                }
                "claim"
                | "claims"
                | "cybersecurity claim"
                | "cybersecurity claims"
                | "主張"
                | "サイバーセキュリティ主張" => {
                    m.claim_col.get_or_insert(idx);
                }
                _ => {}
            }
        }
        m
    }

    /// Whether any column is mapped.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.component_col.is_none()
            && self.asset_col.is_none()
            && self.damage_col.is_none()
            && self.threat_col.is_none()
            && self.control_col.is_none()
            && self.goal_col.is_none()
            && self.claim_col.is_none()
    }
}

/// A worksheet row that could not be mapped to any TARA element during the
/// dry-run preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnmappedRow {
    /// Worksheet name.
    pub sheet: String,
    /// 1-based row number in the worksheet.
    pub row_number: usize,
    /// Human-readable reason why the row was skipped or partially unmapped.
    pub reason: String,
    /// Raw cell values of the row.
    pub values: Vec<String>,
}

/// Result of a dry-run Excel import preview (INT-02).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExcelPreview {
    /// Available worksheet names in the workbook.
    pub sheet_names: Vec<String>,
    /// Active worksheet name.
    pub active_sheet: String,
    /// Header names of the active worksheet.
    pub headers: Vec<String>,
    /// Effective column mapping used for the preview.
    pub mapping: ExcelMapping,
    /// Number of rows that produced at least one element or link.
    pub mapped_rows: usize,
    /// Number of new components in the batch.
    pub components_to_add: usize,
    /// Number of new assets in the batch.
    pub assets_to_add: usize,
    /// Number of new damage scenarios in the batch.
    pub damages_to_add: usize,
    /// Number of new threat scenarios in the batch.
    pub threats_to_add: usize,
    /// Number of new security controls in the batch.
    pub controls_to_add: usize,
    /// Number of new cybersecurity goals in the batch.
    pub goals_to_add: usize,
    /// Number of new cybersecurity claims in the batch.
    pub claims_to_add: usize,
    /// Number of additional operations that link or update project entities.
    pub updates_and_links: usize,
    /// Rows that could not be mapped, with their row number and reason.
    pub unmapped_rows: Vec<UnmappedRow>,
    /// Validated operations ready to be applied as a single batch with
    /// `ProvenanceSource::Import`.
    pub ops: Vec<Op>,
}

impl ExcelPreview {
    /// Summary sentence for the UI and notices.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} row(s) mapped ({} component(s), {} asset(s), {} damage(s), {} threat(s), {} control(s), {} goal(s), {} claim(s), {} link/update operation(s)); {} unmapped row(s).",
            self.mapped_rows,
            self.components_to_add,
            self.assets_to_add,
            self.damages_to_add,
            self.threats_to_add,
            self.controls_to_add,
            self.goals_to_add,
            self.claims_to_add,
            self.updates_and_links,
            self.unmapped_rows.len()
        )
    }
}

// ---------------------------------------------------------------------------
// Minimal ZIP + SpreadsheetML reader.
// ---------------------------------------------------------------------------

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Extracts named UTF-8 XML entries from a ZIP archive (supports Stored and
/// Deflate entries, reading the central directory at the end of the archive).
fn read_zip_entries(archive: &[u8]) -> Result<HashMap<String, String>, ExcelError> {
    if archive.len() > MAX_XLSX_BYTES {
        return Err(ExcelError::TooLarge("archive > 16 MiB"));
    }
    if archive.len() < 22 || &archive[0..4] != b"PK\x03\x04" {
        return Err(ExcelError::InvalidArchive("missing ZIP signature"));
    }
    // Locate End of Central Directory (EOCD) signature PK\x05\x06.
    let min_eocd = archive.len().saturating_sub(65_557);
    let mut eocd_pos = None;
    for pos in (min_eocd..=archive.len() - 22).rev() {
        if &archive[pos..pos + 4] == b"PK\x05\x06" {
            eocd_pos = Some(pos);
            break;
        }
    }
    let eocd = eocd_pos.ok_or(ExcelError::InvalidArchive("missing central directory"))?;
    let total_entries = read_u16_le(archive, eocd + 10)
        .ok_or(ExcelError::InvalidArchive("truncated EOCD"))? as usize;
    if total_entries > MAX_XLSX_ENTRIES {
        return Err(ExcelError::TooLarge("too many ZIP entries"));
    }
    let cd_size = read_u32_le(archive, eocd + 12)
        .ok_or(ExcelError::InvalidArchive("truncated EOCD"))? as usize;
    let cd_offset = read_u32_le(archive, eocd + 16)
        .ok_or(ExcelError::InvalidArchive("truncated EOCD"))? as usize;
    let cd_end = cd_offset
        .checked_add(cd_size)
        .filter(|&end| end <= eocd)
        .ok_or(ExcelError::InvalidArchive(
            "central directory out of bounds",
        ))?;

    let mut entries = HashMap::with_capacity(total_entries);
    let mut total_xml_bytes = 0usize;
    let mut pos = cd_offset;
    for _ in 0..total_entries {
        if pos + 46 > cd_end || &archive[pos..pos + 4] != b"PK\x01\x02" {
            return Err(ExcelError::InvalidArchive("corrupt central directory"));
        }
        let method = read_u16_le(archive, pos + 10).unwrap_or(u16::MAX);
        let comp_size = read_u32_le(archive, pos + 20).unwrap_or(0) as usize;
        let uncomp_size = read_u32_le(archive, pos + 24).unwrap_or(0) as usize;
        let name_len = read_u16_le(archive, pos + 28).unwrap_or(0) as usize;
        let extra_len = read_u16_le(archive, pos + 30).unwrap_or(0) as usize;
        let comment_len = read_u16_le(archive, pos + 32).unwrap_or(0) as usize;
        let local_offset = read_u32_le(archive, pos + 42).unwrap_or(0) as usize;

        if uncomp_size > MAX_XLSX_ENTRY_BYTES || comp_size > MAX_XLSX_ENTRY_BYTES {
            return Err(ExcelError::TooLarge("ZIP entry > 16 MiB"));
        }
        let name_end = pos
            .checked_add(46)
            .and_then(|n| n.checked_add(name_len))
            .filter(|&n| n <= cd_end)
            .ok_or(ExcelError::InvalidArchive("entry name out of bounds"))?;
        let name = std::str::from_utf8(&archive[pos + 46..name_end])
            .map_err(|_| ExcelError::InvalidArchive("non-UTF-8 entry name"))?
            .to_owned();
        pos = name_end
            .checked_add(extra_len)
            .and_then(|n| n.checked_add(comment_len))
            .filter(|&n| n <= cd_end)
            .ok_or(ExcelError::InvalidArchive("central directory overflow"))?;

        // Only decode XML and relationship entries we care about.
        if !std::path::Path::new(&name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("xml") || ext.eq_ignore_ascii_case("rels"))
        {
            continue;
        }
        total_xml_bytes = total_xml_bytes
            .checked_add(uncomp_size)
            .filter(|&size| size <= MAX_XLSX_TOTAL_XML_BYTES)
            .ok_or(ExcelError::TooLarge("total XML > 32 MiB"))?;
        if local_offset + 30 > archive.len()
            || &archive[local_offset..local_offset + 4] != b"PK\x03\x04"
        {
            return Err(ExcelError::InvalidArchive("corrupt local header"));
        }
        let l_name_len = read_u16_le(archive, local_offset + 26).unwrap_or(0) as usize;
        let l_extra_len = read_u16_le(archive, local_offset + 28).unwrap_or(0) as usize;
        let data_start = local_offset
            .checked_add(30)
            .and_then(|n| n.checked_add(l_name_len))
            .and_then(|n| n.checked_add(l_extra_len))
            .ok_or(ExcelError::InvalidArchive("local header overflow"))?;
        let data_end = data_start
            .checked_add(comp_size)
            .filter(|&n| n <= archive.len())
            .ok_or(ExcelError::InvalidArchive("entry data out of bounds"))?;
        let payload = &archive[data_start..data_end];

        let raw = match method {
            0 => payload.to_vec(),
            8 => {
                let mut decoder = flate2::read::DeflateDecoder::new(payload)
                    .take((MAX_XLSX_ENTRY_BYTES as u64) + 1);
                let mut out = Vec::with_capacity(uncomp_size.min(65_536));
                decoder
                    .read_to_end(&mut out)
                    .map_err(|_| ExcelError::InvalidArchive("corrupt deflate stream"))?;
                if out.len() > MAX_XLSX_ENTRY_BYTES {
                    return Err(ExcelError::TooLarge("decompressed entry > 16 MiB"));
                }
                out
            }
            _ => return Err(ExcelError::InvalidArchive("unsupported ZIP compression")),
        };
        if raw.len() != uncomp_size {
            return Err(ExcelError::InvalidArchive(
                "ZIP entry size does not match its directory",
            ));
        }
        let xml = String::from_utf8(raw)
            .map_err(|_| ExcelError::InvalidArchive("XML entry is not UTF-8"))?;
        entries.insert(name, xml);
    }
    Ok(entries)
}

fn xml_unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp + 1..];
        let Some(semi) = rest.find(';') else {
            out.push('&');
            break;
        };
        let entity = &rest[..semi];
        rest = &rest[semi + 1..];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                if let Ok(code) = u32::from_str_radix(&entity[2..], 16)
                    && let Some(ch) = char::from_u32(code)
                {
                    out.push(ch);
                }
            }
            _ if entity.starts_with('#') => {
                if let Ok(code) = entity[1..].parse::<u32>()
                    && let Some(ch) = char::from_u32(code)
                {
                    out.push(ch);
                }
            }
            _ => {
                out.push('&');
                out.push_str(entity);
                out.push(';');
            }
        }
    }
    out.push_str(rest);
    out
}

fn attr_value<'a>(tag: &'a str, attr: &str) -> Option<&'a str> {
    let pattern = format!("{attr}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// Collects all `<t>` text runs inside an XML fragment (`<si>` or `<is>`).
fn extract_t_texts(fragment: &str) -> String {
    let mut out = String::new();
    let mut rest = fragment;
    while let Some(open) = rest.find("<t") {
        let after = &rest[open + 2..];
        let Some(first_char) = after.chars().next() else {
            break;
        };
        if first_char != '>' && !first_char.is_ascii_whitespace() {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        if after[..gt].ends_with('/') {
            rest = &after[gt + 1..];
            continue;
        }
        let content_start = &after[gt + 1..];
        let Some(close) = content_start.find("</t>") else {
            break;
        };
        out.push_str(&xml_unescape(&content_start[..close]));
        rest = &content_start[close + 4..];
    }
    out
}

fn strip_xml_comments(xml: &str) -> Result<String, ExcelError> {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        let end = after
            .find("-->")
            .ok_or(ExcelError::InvalidArchive("unterminated XML comment"))?;
        rest = &after[end + 3..];
    }
    out.push_str(rest);
    Ok(out)
}

fn parse_shared_strings(xml: &str) -> Result<Vec<String>, ExcelError> {
    let mut strings = Vec::new();
    let mut rest = xml;
    while let Some(si_start) = rest.find("<si") {
        let after = &rest[si_start..];
        if !after[3..]
            .chars()
            .next()
            .is_some_and(|c| c == '>' || c.is_ascii_whitespace())
        {
            rest = &after[3..];
            continue;
        }
        if strings.len() >= MAX_SHARED_STRINGS {
            return Err(ExcelError::WorksheetLimit("too many shared strings"));
        }
        let Some(si_end) = after.find("</si>") else {
            return Err(ExcelError::InvalidArchive("unterminated shared string"));
        };
        strings.push(extract_t_texts(&after[..si_end]));
        rest = &after[si_end + 5..];
    }
    Ok(strings)
}

fn parse_workbook_sheets(xml: &str) -> Result<Vec<(String, String)>, ExcelError> {
    let mut sheets = Vec::new();
    let mut rest = xml;
    while let Some(pos) = rest.find("<sheet") {
        let after = &rest[pos..];
        if !after[6..]
            .chars()
            .next()
            .is_some_and(|c| c == '>' || c.is_ascii_whitespace())
        {
            rest = &after[6..];
            continue;
        }
        let Some(end) = after.find('>') else {
            return Err(ExcelError::InvalidArchive(
                "unterminated worksheet declaration",
            ));
        };
        let tag = &after[..end];
        let name =
            attr_value(tag, "name").ok_or(ExcelError::InvalidArchive("worksheet has no name"))?;
        let rel_id = attr_value(tag, "r:id").ok_or(ExcelError::InvalidArchive(
            "worksheet has no relationship ID",
        ))?;
        sheets.push((xml_unescape(name), xml_unescape(rel_id)));
        rest = &after[end + 1..];
    }
    Ok(sheets)
}

fn normalize_worksheet_target(target: &str) -> Result<String, ExcelError> {
    let path = target.trim_start_matches('/');
    let path = if path.starts_with("xl/") {
        path.to_owned()
    } else {
        format!("xl/{path}")
    };
    if !path.starts_with("xl/worksheets/")
        || !path.ends_with(".xml")
        || path.split('/').any(|part| part == "." || part == "..")
    {
        return Err(ExcelError::InvalidArchive(
            "invalid worksheet relationship target",
        ));
    }
    Ok(path)
}

fn parse_worksheet_relationships(xml: &str) -> Result<HashMap<String, String>, ExcelError> {
    let mut relationships = HashMap::new();
    let mut rest = xml;
    while let Some(pos) = rest.find("<Relationship") {
        let after = &rest[pos..];
        if !after[13..]
            .chars()
            .next()
            .is_some_and(|c| c == '>' || c.is_ascii_whitespace() || c == '/')
        {
            rest = &after[13..];
            continue;
        }
        let Some(end) = after.find('>') else {
            return Err(ExcelError::InvalidArchive(
                "unterminated worksheet relationship",
            ));
        };
        let tag = &after[..end];
        let kind = attr_value(tag, "Type").unwrap_or("");
        if !kind.contains("/worksheet") {
            rest = &after[end + 1..];
            continue;
        }
        let id = attr_value(tag, "Id").ok_or(ExcelError::InvalidArchive(
            "worksheet relationship has no ID",
        ))?;
        let target = attr_value(tag, "Target").ok_or(ExcelError::InvalidArchive(
            "worksheet relationship has no target",
        ))?;
        if attr_value(tag, "TargetMode") == Some("External") {
            return Err(ExcelError::InvalidArchive(
                "external worksheet relationship",
            ));
        }
        let path = normalize_worksheet_target(target)?;
        if relationships.insert(xml_unescape(id), path).is_some() {
            return Err(ExcelError::InvalidArchive(
                "duplicate worksheet relationship ID",
            ));
        }
        rest = &after[end + 1..];
    }
    Ok(relationships)
}

/// Converts an Excel cell reference like `"C12"` or `"AA5"` to a 0-based
/// column index.
fn col_index_from_ref(cell_ref: &str) -> Option<usize> {
    let mut col = 0usize;
    let mut has_letters = false;
    for b in cell_ref.bytes() {
        if b.is_ascii_alphabetic() {
            has_letters = true;
            let digit = (b.to_ascii_uppercase() - b'A') as usize + 1;
            col = col.checked_mul(26)?.checked_add(digit)?;
        } else {
            break;
        }
    }
    if has_letters && col > 0 {
        Some(col - 1)
    } else {
        None
    }
}

fn parse_sheet(name: String, xml: &str, shared: &[String]) -> Result<SheetData, ExcelError> {
    let cleaned_xml = strip_xml_comments(xml)?;
    let mut headers: Vec<String> = Vec::new();
    let mut rows: Vec<(usize, Vec<String>)> = Vec::new();
    let mut rest = cleaned_xml.as_str();
    let mut row_counter = 0usize;

    while let Some(row_start) = rest.find("<row") {
        let after_row = &rest[row_start..];
        let Some(row_gt) = after_row.find('>') else {
            break;
        };
        let row_tag = &after_row[..row_gt];
        row_counter += 1;
        let excel_row_num = attr_value(row_tag, "r")
            .and_then(|r| r.parse::<usize>().ok())
            .unwrap_or(row_counter);

        if row_tag.ends_with('/') {
            rest = &after_row[row_gt + 1..];
            continue;
        }
        let body_start = &after_row[row_gt + 1..];
        let Some(row_end) = body_start.find("</row>") else {
            break;
        };
        let row_body = &body_start[..row_end];
        rest = &body_start[row_end + 6..];

        let mut cells: Vec<String> = Vec::new();
        let mut c_rest = row_body;
        let mut next_col = 0usize;

        while let Some(c_pos) = c_rest.find("<c") {
            let after_c = &c_rest[c_pos..];
            let Some(after_ch) = after_c[2..].chars().next() else {
                break;
            };
            if after_ch != '>' && after_ch != '/' && !after_ch.is_ascii_whitespace() {
                c_rest = &after_c[2..];
                continue;
            }
            let Some(c_gt) = after_c.find('>') else { break };
            let c_tag = &after_c[..c_gt];
            let col_idx = attr_value(c_tag, "r")
                .and_then(col_index_from_ref)
                .unwrap_or(next_col);
            if col_idx >= MAX_XLSX_COLS {
                return Err(ExcelError::WorksheetLimit("too many columns"));
            }
            next_col = col_idx + 1;
            let cell_type = attr_value(c_tag, "t").unwrap_or("");

            let value = if c_tag.ends_with('/') {
                c_rest = &after_c[c_gt + 1..];
                String::new()
            } else {
                let c_body_start = &after_c[c_gt + 1..];
                let Some(c_end) = c_body_start.find("</c>") else {
                    break;
                };
                let c_body = &c_body_start[..c_end];
                c_rest = &c_body_start[c_end + 4..];

                if cell_type == "inlineStr" {
                    extract_t_texts(c_body)
                } else if let Some(v_start) = c_body.find("<v>")
                    && let Some(v_end) = c_body[v_start + 3..].find("</v>")
                {
                    let raw_v = &c_body[v_start + 3..v_start + 3 + v_end];
                    if cell_type == "s" {
                        raw_v
                            .trim()
                            .parse::<usize>()
                            .ok()
                            .and_then(|idx| shared.get(idx).cloned())
                            .unwrap_or_default()
                    } else {
                        xml_unescape(raw_v)
                    }
                } else {
                    extract_t_texts(c_body)
                }
            };

            if cells.len() <= col_idx {
                cells.resize(col_idx + 1, String::new());
            }
            cells[col_idx] = value.trim().to_owned();
        }

        if cells.iter().all(|c| c.is_empty()) {
            continue;
        }
        if headers.is_empty() {
            headers = cells;
        } else {
            if rows.len() >= MAX_XLSX_ROWS {
                return Err(ExcelError::WorksheetLimit("too many rows"));
            }
            rows.push((excel_row_num, cells));
        }
    }

    Ok(SheetData {
        name,
        headers,
        rows,
    })
}

/// Reads and parses all worksheets from an `.xlsx` byte slice.
///
/// # Errors
///
/// Returns [`ExcelError`] if the archive is too large, corrupt, or has no
/// worksheets.
pub fn read_workbook(bytes: &[u8]) -> Result<WorkbookData, ExcelError> {
    let entries = read_zip_entries(bytes)?;
    let shared = match entries.get("xl/sharedStrings.xml") {
        Some(xml) => parse_shared_strings(&strip_xml_comments(xml)?)?,
        None => Vec::new(),
    };
    let workbook = entries
        .get("xl/workbook.xml")
        .ok_or(ExcelError::InvalidArchive("missing workbook.xml"))?;
    let declarations = parse_workbook_sheets(&strip_xml_comments(workbook)?)?;
    if declarations.is_empty() {
        return Err(ExcelError::NoSheets);
    }
    let rels = entries
        .get("xl/_rels/workbook.xml.rels")
        .ok_or(ExcelError::InvalidArchive("missing workbook relationships"))?;
    let relationships = parse_worksheet_relationships(&strip_xml_comments(rels)?)?;

    let mut sheets = Vec::with_capacity(declarations.len());
    for (sheet_name, relationship_id) in declarations {
        let key = relationships
            .get(&relationship_id)
            .ok_or(ExcelError::InvalidArchive(
                "worksheet relationship is missing",
            ))?;
        let xml = entries
            .get(key)
            .ok_or(ExcelError::InvalidArchive("worksheet XML is missing"))?;
        sheets.push(parse_sheet(sheet_name, xml, &shared)?);
    }
    Ok(WorkbookData { sheets })
}

// ---------------------------------------------------------------------------
// Dry-run preview and batch op builder with `Provenance::import`.
// ---------------------------------------------------------------------------

fn cell_at(row: &[String], col: Option<usize>) -> &str {
    col.and_then(|i| row.get(i)).map_or("", |s| s.trim())
}

fn properties_are_known(m: &Methodology, text: &str) -> bool {
    text.split([',', ';', '/']).all(|part| {
        let token = part.trim();
        token.is_empty()
            || m.properties().any(|property| {
                let en = m.property_label(property).unwrap_or("");
                let ja = m.property_label_for_lang(property, "ja").unwrap_or("");
                en.eq_ignore_ascii_case(token) || ja == token
            })
    })
}

fn parse_properties(
    m: &Methodology,
    text: &str,
    threat_cat: Option<aegis_model::ThreatCategory>,
) -> PropertySet {
    let mut set = PropertySet::default();
    if !text.is_empty() {
        for part in text.split([',', ';', '/']) {
            let token = part.trim();
            if token.is_empty() {
                continue;
            }
            for prop in m.properties() {
                let en = m.property_label(prop).unwrap_or("");
                let ja = m.property_label_for_lang(prop, "ja").unwrap_or("");
                if en.eq_ignore_ascii_case(token) || ja == token {
                    set = set.with(prop);
                }
            }
        }
    }
    if let Some(cat) = threat_cat {
        for prop in m.violates(cat).iter() {
            set = set.with(prop);
        }
    }
    set
}

fn parse_impact(
    m: &Methodology,
    text: &str,
) -> Option<(aegis_model::ImpactCategory, aegis_model::Level)> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if let Some((cat_str, lvl_str)) = t.split_once(':') {
        let (cat_str, lvl_str) = (cat_str.trim(), lvl_str.trim());
        for cat in m.impact_categories() {
            let c_en = m.impact_category_label(cat).unwrap_or("");
            let c_ja = m.impact_category_label_for_lang(cat, "ja").unwrap_or("");
            if c_en.eq_ignore_ascii_case(cat_str) || c_ja == cat_str {
                for lvl in m.levels(cat) {
                    let l_en = m.level_label(cat, lvl).unwrap_or("");
                    let l_ja = m.level_label_for_lang(cat, lvl, "ja").unwrap_or("");
                    if l_en.eq_ignore_ascii_case(lvl_str) || l_ja == lvl_str {
                        return Some((cat, lvl));
                    }
                }
            }
        }
    }
    // Fallback: match level label on the first impact category.
    let cat = m.impact_categories().next()?;
    for lvl in m.levels(cat) {
        let l_en = m.level_label(cat, lvl).unwrap_or("");
        let l_ja = m.level_label_for_lang(cat, lvl, "ja").unwrap_or("");
        if l_en.eq_ignore_ascii_case(t) || l_ja == t {
            return Some((cat, lvl));
        }
    }
    None
}

fn parse_threat_category(m: &Methodology, text: &str) -> Option<aegis_model::ThreatCategory> {
    let t = text.trim();
    for cat in m.threat_categories() {
        let en = m.threat_category_label(cat).unwrap_or("");
        let ja = m.threat_category_label_for_lang(cat, "ja").unwrap_or("");
        if en.eq_ignore_ascii_case(t) || ja == t {
            return Some(cat);
        }
    }
    // A blank category column falls back to the methodology's first category,
    // as the risk-register export has no category column. A non-blank value
    // that did not match is reported as unmapped by the caller.
    if t.is_empty() {
        m.threat_categories().next()
    } else {
        None
    }
}

fn parse_feasibility(m: &Methodology, text: &str) -> Option<aegis_model::FeasibilityLevel> {
    let t = text.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("not rated") || t == "未評価" {
        return None;
    }
    for (lvl, _, en) in m.feasibility_levels() {
        let ja = m.feasibility_level_label_for_lang(lvl, "ja").unwrap_or("");
        if en.eq_ignore_ascii_case(t) || ja == t {
            return Some(lvl);
        }
    }
    None
}

fn parse_treatment(text: &str) -> Option<RiskTreatment> {
    match text.trim().to_lowercase().as_str() {
        "avoid" | "回避" => Some(RiskTreatment::Avoid),
        "reduce" | "低減" => Some(RiskTreatment::Reduce),
        "share" | "移転" | "共有" => Some(RiskTreatment::Share),
        "retain" | "保有" | "受容" => Some(RiskTreatment::Retain),
        _ => None,
    }
}

/// Runs a dry-run preview of importing `workbook` into `project` with `mapping`
/// (auto-detecting columns when `mapping.is_empty()`), building and validating
/// all operations with `Provenance::import(actor, now_ms)`.
///
/// # Errors
///
/// Returns [`ExcelError`] if the workbook has no sheets or the requested sheet
/// does not exist.
#[allow(clippy::too_many_lines)]
pub fn preview_import(
    project: &Project,
    workbook: &WorkbookData,
    mapping: &ExcelMapping,
    actor: &str,
    now_ms: u64,
) -> Result<ExcelPreview, ExcelError> {
    if workbook.sheets.is_empty() {
        return Err(ExcelError::NoSheets);
    }
    let sheet_names: Vec<String> = workbook.sheets.iter().map(|s| s.name.clone()).collect();
    let sheet = match &mapping.sheet {
        Some(wanted) if !wanted.trim().is_empty() => workbook
            .sheets
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(wanted.trim()))
            .ok_or_else(|| ExcelError::SheetNotFound(wanted.clone()))?,
        _ => {
            // Prefer the first sheet that has data rows and auto-detected columns.
            workbook
                .sheets
                .iter()
                .find(|s| !s.rows.is_empty() && !ExcelMapping::auto_detect(&s.headers).is_empty())
                .unwrap_or(&workbook.sheets[0])
        }
    };

    let effective_mapping = if mapping.is_empty() {
        let mut auto = ExcelMapping::auto_detect(&sheet.headers);
        auto.sheet = Some(sheet.name.clone());
        auto
    } else {
        let mut m = mapping.clone();
        if m.sheet.is_none() {
            m.sheet = Some(sheet.name.clone());
        }
        m
    };

    let m = project.methodology();
    let prov = Provenance::import(actor, now_ms);
    let mut sim = project.clone();

    // Pre-populate name -> ID maps from existing project entities so duplicate
    // names across rows reuse the existing or newly created entity.
    let mut components: HashMap<String, ComponentId> = project
        .components()
        .map(|(_, id, c)| (c.name().to_lowercase(), id))
        .collect();
    let mut assets: HashMap<String, AssetId> = project
        .assets()
        .map(|(_, id, a)| (a.name().to_lowercase(), id))
        .collect();
    let mut damages: HashMap<String, DamageId> = project
        .damage_scenarios()
        .map(|(_, id, d)| (d.name().to_lowercase(), id))
        .collect();
    let mut threats: HashMap<(AssetId, String), ThreatId> = project
        .threat_scenarios()
        .filter_map(|(_, id, t)| {
            Some(((project.asset_id(t.asset())?, t.name().to_lowercase()), id))
        })
        .collect();
    let mut controls: HashMap<String, ControlId> = project
        .security_controls()
        .map(|(_, id, c)| (c.name().to_lowercase(), id))
        .collect();
    let mut goals: HashMap<String, GoalId> = project
        .cybersecurity_goals()
        .map(|(_, id, g)| (g.name().to_lowercase(), id))
        .collect();
    let mut claims: HashMap<String, ClaimId> = project
        .cybersecurity_claims()
        .map(|(_, id, c)| (c.name().to_lowercase(), id))
        .collect();

    let mut preview = ExcelPreview {
        sheet_names,
        active_sheet: sheet.name.clone(),
        headers: sheet.headers.clone(),
        mapping: effective_mapping.clone(),
        ..ExcelPreview::default()
    };

    for (row_num, row) in &sheet.rows {
        let comp_name = cell_at(row, effective_mapping.component_col);
        let asset_name = cell_at(row, effective_mapping.asset_col);
        let props_text = cell_at(row, effective_mapping.properties_col);
        let damage_name = cell_at(row, effective_mapping.damage_col);
        let impact_text = cell_at(row, effective_mapping.impact_col);
        let threat_name = cell_at(row, effective_mapping.threat_col);
        let cat_text = cell_at(row, effective_mapping.category_col);
        let feas_text = cell_at(row, effective_mapping.feasibility_col);
        let treat_text = cell_at(row, effective_mapping.treatment_col);
        let control_name = cell_at(row, effective_mapping.control_col);
        let raw_goal = cell_at(row, effective_mapping.goal_col);
        let raw_claim = cell_at(row, effective_mapping.claim_col);

        // Allow `Goal: ...` or `Claim: ...` in the goal column (as exported by
        // the Risk register sheet).
        let (goal_name, claim_name) = if let Some(rest) = raw_goal.strip_prefix("Claim:") {
            ("", rest.trim())
        } else if let Some(rest) = raw_goal.strip_prefix("Goal:") {
            (rest.trim(), raw_claim)
        } else {
            (raw_goal, raw_claim)
        };

        if comp_name.is_empty()
            && asset_name.is_empty()
            && damage_name.is_empty()
            && threat_name.is_empty()
            && control_name.is_empty()
            && goal_name.is_empty()
            && claim_name.is_empty()
            && props_text.is_empty()
            && impact_text.is_empty()
            && cat_text.is_empty()
            && feas_text.is_empty()
            && treat_text.is_empty()
        {
            preview.unmapped_rows.push(UnmappedRow {
                sheet: sheet.name.clone(),
                row_number: *row_num,
                reason: "no mapped element columns have values in this row".to_owned(),
                values: row.clone(),
            });
            continue;
        }

        if !threat_name.is_empty() && asset_name.is_empty() {
            preview.unmapped_rows.push(UnmappedRow {
                sheet: sheet.name.clone(),
                row_number: *row_num,
                reason: format!(
                    "threat \"{threat_name}\" requires an asset name in the mapped asset column"
                ),
                values: row.clone(),
            });
            continue;
        }

        let mut row_ops = Vec::new();
        let mut comp_added = 0usize;
        let mut asset_added = 0usize;
        let mut damage_added = 0usize;
        let mut threat_added = 0usize;
        let mut control_added = 0usize;
        let mut goal_added = 0usize;
        let mut claim_added = 0usize;

        let comp_id = if !comp_name.is_empty() {
            if let Some(&existing) = components.get(&comp_name.to_lowercase()) {
                Some(existing)
            } else {
                let id = ComponentId::new_v7();
                row_ops.push(Op::AddComponent {
                    id,
                    name: comp_name.into(),
                    parent: None,
                    provenance: prov.clone(),
                });
                comp_added += 1;
                Some(id)
            }
        } else {
            None
        };

        let row_threat_cat = if !threat_name.is_empty() {
            parse_threat_category(m, cat_text)
        } else {
            None
        };
        let parsed_impact = (!impact_text.is_empty())
            .then(|| parse_impact(m, impact_text))
            .flatten();
        let parsed_feasibility = (!feas_text.is_empty())
            .then(|| parse_feasibility(m, feas_text))
            .flatten();
        let parsed_treatment = (!treat_text.is_empty())
            .then(|| parse_treatment(treat_text))
            .flatten();
        let invalid_value = if !threat_name.is_empty() && row_threat_cat.is_none() {
            Some("threat category is missing or not recognized")
        } else if !impact_text.is_empty() && damage_name.is_empty() {
            Some("impact rating requires a damage scenario")
        } else if (!cat_text.is_empty() || !feas_text.is_empty() || !treat_text.is_empty())
            && threat_name.is_empty()
        {
            Some("threat category, feasibility, and treatment require a threat scenario")
        } else if !impact_text.is_empty() && parsed_impact.is_none() {
            Some("impact rating is not recognized")
        } else if !feas_text.is_empty()
            && parsed_feasibility.is_none()
            && !feas_text.eq_ignore_ascii_case("not rated")
            && feas_text != "未評価"
        {
            Some("feasibility rating is not recognized")
        } else if !treat_text.is_empty() && parsed_treatment.is_none() {
            Some("risk treatment is not recognized")
        } else if !props_text.is_empty() && !properties_are_known(m, props_text) {
            Some("one or more security properties are not recognized")
        } else {
            None
        };
        if let Some(reason) = invalid_value {
            preview.unmapped_rows.push(UnmappedRow {
                sheet: sheet.name.clone(),
                row_number: *row_num,
                reason: reason.to_owned(),
                values: row.clone(),
            });
            continue;
        }

        let asset_id = if !asset_name.is_empty() {
            let id = if let Some(&existing) = assets.get(&asset_name.to_lowercase()) {
                if let Some(cat) = row_threat_cat
                    && let Some(a_idx) = sim.asset_idx(&existing)
                    && let Some(a) = sim.asset(a_idx)
                {
                    let mut props = a.properties();
                    let mut changed = false;
                    for prop in m.violates(cat).iter() {
                        if !props.contains(prop) {
                            props = props.with(prop);
                            changed = true;
                        }
                    }
                    if changed {
                        row_ops.push(Op::SetAssetProperties {
                            id: existing,
                            properties: props,
                            provenance: prov.clone(),
                        });
                    }
                }
                existing
            } else {
                let id = AssetId::new_v7();
                row_ops.push(Op::AddAsset {
                    id,
                    name: asset_name.into(),
                    properties: parse_properties(m, props_text, row_threat_cat),
                    provenance: prov.clone(),
                });
                asset_added += 1;
                id
            };
            if let Some(cid) = comp_id {
                row_ops.push(Op::SetAssetComponent {
                    asset: id,
                    component: Some(cid),
                    provenance: prov.clone(),
                });
            }
            Some(id)
        } else {
            None
        };

        let damage_id = if !damage_name.is_empty() {
            let id = if let Some(&existing) = damages.get(&damage_name.to_lowercase()) {
                existing
            } else {
                let id = DamageId::new_v7();
                let impacts = parsed_impact.into_iter().collect();
                row_ops.push(Op::AddDamageScenario {
                    id,
                    name: damage_name.into(),
                    impacts,
                    provenance: prov.clone(),
                });
                damage_added += 1;
                id
            };
            if damages
                .get(&damage_name.to_lowercase())
                .is_some_and(|existing| *existing == id)
                && let Some((category, level)) = parsed_impact
            {
                row_ops.push(Op::SetImpact {
                    id,
                    category,
                    level: Some(level),
                    provenance: prov.clone(),
                });
            }
            Some(id)
        } else {
            None
        };

        let control_id = if !control_name.is_empty() {
            if let Some(&existing) = controls.get(&control_name.to_lowercase()) {
                Some(existing)
            } else {
                let id = ControlId::new_v7();
                row_ops.push(Op::AddSecurityControl {
                    id,
                    name: control_name.into(),
                    provenance: prov.clone(),
                });
                control_added += 1;
                Some(id)
            }
        } else {
            None
        };

        let goal_id = if !goal_name.is_empty() {
            let id = if let Some(&existing) = goals.get(&goal_name.to_lowercase()) {
                existing
            } else {
                let id = GoalId::new_v7();
                row_ops.push(Op::AddCybersecurityGoal {
                    id,
                    name: goal_name.into(),
                    controls: Vec::new(),
                    provenance: prov.clone(),
                });
                goal_added += 1;
                id
            };
            if let Some(cid) = control_id {
                row_ops.push(Op::LinkGoalControl {
                    goal: id,
                    control: cid,
                    provenance: prov.clone(),
                });
            }
            Some(id)
        } else {
            None
        };

        let claim_id = if !claim_name.is_empty() {
            if let Some(&existing) = claims.get(&claim_name.to_lowercase()) {
                Some(existing)
            } else {
                let id = ClaimId::new_v7();
                row_ops.push(Op::AddCybersecurityClaim {
                    id,
                    name: claim_name.into(),
                    provenance: prov.clone(),
                });
                claim_added += 1;
                Some(id)
            }
        } else {
            None
        };

        if !threat_name.is_empty()
            && let Some(aid) = asset_id
        {
            let key = (aid, threat_name.to_lowercase());
            let tid = if let Some(&existing) = threats.get(&key) {
                if let Some(did) = damage_id {
                    row_ops.push(Op::LinkDamage {
                        threat: existing,
                        damage: did,
                        provenance: prov.clone(),
                    });
                }
                existing
            } else if let Some(cat) = parse_threat_category(m, cat_text) {
                let tid = ThreatId::new_v7();
                row_ops.push(Op::AddThreatScenario {
                    id: tid,
                    name: threat_name.into(),
                    asset: aid,
                    category: cat,
                    catalog_refs: Vec::new(),
                    damages: damage_id.into_iter().collect(),
                    provenance: prov.clone(),
                });
                threat_added += 1;
                tid
            } else {
                preview.unmapped_rows.push(UnmappedRow {
                    sheet: sheet.name.clone(),
                    row_number: *row_num,
                    reason: "methodology has no threat categories".to_owned(),
                    values: row.clone(),
                });
                continue;
            };

            if let Some(lvl) = parsed_feasibility {
                row_ops.push(Op::SetThreatFeasibility {
                    threat: tid,
                    level: Some(lvl),
                    provenance: prov.clone(),
                });
            }
            if let Some(tr) = parsed_treatment {
                row_ops.push(Op::SetRiskTreatment {
                    threat: tid,
                    treatment: Some(tr),
                    provenance: prov.clone(),
                });
            }
            if let Some(gid) = goal_id {
                row_ops.push(Op::SetThreatGoal {
                    threat: tid,
                    goal: Some(gid),
                    provenance: prov.clone(),
                });
            }
            if let Some(cid) = claim_id {
                row_ops.push(Op::SetThreatClaim {
                    threat: tid,
                    claim: Some(cid),
                    provenance: prov.clone(),
                });
            }
        }

        // Apply against one simulation project; inverses roll back a rejected
        // row without cloning the entire project for every worksheet row.
        if preview.ops.len().saturating_add(row_ops.len()) > crate::workspace::MAX_SUBMIT_OPS {
            return Err(ExcelError::TooManyOperations(
                crate::workspace::MAX_SUBMIT_OPS,
            ));
        }
        let mut inverses = Vec::with_capacity(row_ops.len());
        let mut failed = None;
        for op in &row_ops {
            match sim.apply(op.clone()) {
                Ok(inverse) => inverses.push(inverse),
                Err(error) => {
                    failed = Some(error.to_string());
                    break;
                }
            }
        }
        if let Some(err) = failed {
            for inverse in inverses.into_iter().rev() {
                if sim.apply(inverse).is_err() {
                    return Err(ExcelError::SimulationRollback);
                }
            }
            preview.unmapped_rows.push(UnmappedRow {
                sheet: sheet.name.clone(),
                row_number: *row_num,
                reason: err,
                values: row.clone(),
            });
            continue;
        }
        if let Some(cid) = comp_id {
            components.insert(comp_name.to_lowercase(), cid);
        }
        if let Some(aid) = asset_id {
            assets.insert(asset_name.to_lowercase(), aid);
            if !threat_name.is_empty() {
                // Find the threat ID in sim.
                if let Some((_, tid, _)) = sim.threat_scenarios().find(|(_, _, t)| {
                    sim.asset_id(t.asset()) == Some(aid)
                        && t.name().eq_ignore_ascii_case(threat_name)
                }) {
                    threats.insert((aid, threat_name.to_lowercase()), tid);
                }
            }
        }
        if let Some(did) = damage_id {
            damages.insert(damage_name.to_lowercase(), did);
        }
        if let Some(cid) = control_id {
            controls.insert(control_name.to_lowercase(), cid);
        }
        if let Some(gid) = goal_id {
            goals.insert(goal_name.to_lowercase(), gid);
        }
        if let Some(cid) = claim_id {
            claims.insert(claim_name.to_lowercase(), cid);
        }

        preview.mapped_rows += 1;
        preview.components_to_add += comp_added;
        preview.assets_to_add += asset_added;
        preview.damages_to_add += damage_added;
        preview.threats_to_add += threat_added;
        preview.controls_to_add += control_added;
        preview.goals_to_add += goal_added;
        preview.claims_to_add += claim_added;
        preview.updates_and_links += row_ops
            .iter()
            .filter(|op| {
                matches!(
                    op,
                    Op::SetAssetProperties { .. }
                        | Op::SetAssetComponent { .. }
                        | Op::SetImpact { .. }
                        | Op::LinkDamage { .. }
                        | Op::SetThreatFeasibility { .. }
                        | Op::SetRiskTreatment { .. }
                        | Op::LinkGoalControl { .. }
                        | Op::SetThreatGoal { .. }
                        | Op::SetThreatClaim { .. }
                )
            })
            .count();
        preview.ops.extend(row_ops);
    }

    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_comments_are_stripped_not_parsed_as_rows() {
        let xml = "<worksheet><sheetData><!--<row r=\"9\"><c><v>fake</v></c></row>-->\
                    <row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>real</t></is></c></row></sheetData></worksheet>";
        let sheet = parse_sheet("s".into(), xml, &[]).expect("parses");
        assert_eq!(sheet.headers, vec!["real"]);
        assert!(sheet.rows.is_empty());
    }

    #[test]
    fn too_many_columns_or_rows_are_rejected_not_truncated() {
        let mut cols = String::new();
        for i in 0..MAX_XLSX_COLS {
            cols.push_str(&format!("<c r=\"{}{}\"/>", col_ref(i), 1));
        }
        let overflow = "<c r=\"AAAA1\"/>";
        let xml = format!(
            "<worksheet><sheetData><row r=\"1\">{cols}{overflow}</row></sheetData></worksheet>"
        );
        assert!(matches!(
            parse_sheet("s".into(), &xml, &[]),
            Err(ExcelError::WorksheetLimit("too many columns"))
        ));
    }

    #[test]
    fn the_formula_guard_apostrophe_round_trips_without_losing_text() {
        // The importer must preserve a literal cell that starts with an
        // apostrophe before a formula marker; the exporter's own guard is
        // written as inlineStr and does not need stripping here.
        let value = xml_unescape("'=text");
        assert_eq!(value, "'=text");
        let value = xml_unescape("=SUM(A1)");
        assert_eq!(value, "=SUM(A1)");
    }

    #[test]
    fn worksheet_relationships_resolve_reordered_targets() {
        let rels = "<Relationships><Relationship Id=\"rId2\" \
                    Type=\".../worksheet\" Target=\"worksheets/sheet2.xml\"/>\
                    <Relationship Id=\"rId1\" Type=\".../worksheet\" Target=\"worksheets/sheet1.xml\"/>\
                    </Relationships>";
        let parsed = parse_worksheet_relationships(rels).expect("parses");
        assert_eq!(
            parsed.get("rId1").map(String::as_str),
            Some("xl/worksheets/sheet1.xml")
        );
        assert_eq!(
            parsed.get("rId2").map(String::as_str),
            Some("xl/worksheets/sheet2.xml")
        );
    }

    #[test]
    fn worksheet_relationships_reject_external_or_escaping_targets() {
        assert!(parse_worksheet_relationships(
            "<Relationships><Relationship Id=\"rId1\" Type=\".../worksheet\" Target=\"https://evil/x.xml\"/></Relationships>"
        )
        .is_err());
        assert!(
            parse_worksheet_relationships(
                "<Relationships><Relationship Id=\"rId1\" Type=\".../worksheet\" Target=\"../x.xml\"/></Relationships>"
            )
            .is_err()
        );
    }

    fn col_ref(mut n: usize) -> String {
        let mut s = String::new();
        loop {
            s.push(char::from(b'A' + u8::try_from(n % 26).unwrap_or(0)));
            if n < 26 {
                break;
            }
            n = n / 26 - 1;
        }
        s.chars().rev().collect()
    }
}
