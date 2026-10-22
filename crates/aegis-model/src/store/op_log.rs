//! The append-only op log: `aegis-oplog` v1, JSON Lines.

use std::io::{BufRead, ErrorKind, Write};
use std::sync::Arc;

use serde::Serialize;

use super::wire::{
    self, CatalogRefView, CatalogRefsInOrder, FORMAT_VERSION, HeaderView, HeaderWire, Ids,
    ImpactsInOrder, KeyOrder, MethodologyView, OpWire, PropertyKeys, RecordWire, Required, Text,
};
use super::{Limit, MAX_NAME_BYTES, MAX_OP_RECORD_BYTES};
use crate::error::StoreError;
use crate::id::{AssetId, DamageId, ThreatId};
use crate::methodology::Methodology;
use crate::op::Op;
use crate::provenance::Provenance;
use crate::threat::{MAX_CATALOG_REFS, MAX_DAMAGE_LINKS};

/// One record of an op log: its sequence number and the op.
///
/// ```
/// use aegis_model::{AssetId, LoggedOp, Op};
///
/// let record = LoggedOp { seq: 1, op: Op::RemoveAsset { id: AssetId::new_v7() } };
/// assert_eq!(record.seq, 1);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggedOp {
    /// Position in the log: 1 for the first record, then up by exactly 1.
    pub seq: u64,
    /// The op, with indices of the reader's methodology.
    pub op: Op,
}

// ---------------------------------------------------------------------------
// Encoding.
// ---------------------------------------------------------------------------

/// One record line, borrowed from the op.
#[derive(Serialize)]
struct RecordView<'a> {
    seq: u64,
    op: OpView<'a>,
}

/// An op with every index resolved to a key while it is written.
#[derive(Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum OpView<'a> {
    AddAsset {
        id: Text<AssetId>,
        name: &'a str,
        properties: PropertyKeys<'a>,
    },
    RenameAsset {
        id: Text<AssetId>,
        name: &'a str,
    },
    SetAssetProperties {
        id: Text<AssetId>,
        properties: PropertyKeys<'a>,
    },
    RemoveAsset {
        id: Text<AssetId>,
    },
    AddDamageScenario {
        id: Text<DamageId>,
        name: &'a str,
        impacts: ImpactsInOrder<'a>,
    },
    RenameDamageScenario {
        id: Text<DamageId>,
        name: &'a str,
    },
    SetImpact {
        id: Text<DamageId>,
        category: Required<&'a str>,
        level: Option<Required<&'a str>>,
    },
    RemoveDamageScenario {
        id: Text<DamageId>,
    },
    AddThreatScenario {
        id: Text<ThreatId>,
        name: &'a str,
        asset: Text<AssetId>,
        category: Required<&'a str>,
        catalog_refs: CatalogRefsInOrder<'a>,
        damages: Ids<'a, DamageId>,
    },
    RenameThreatScenario {
        id: Text<ThreatId>,
        name: &'a str,
    },
    SetThreatCategory {
        id: Text<ThreatId>,
        category: Required<&'a str>,
    },
    AddCatalogRef {
        threat: Text<ThreatId>,
        catalog_ref: CatalogRefView<'a>,
    },
    RemoveCatalogRef {
        threat: Text<ThreatId>,
        catalog_ref: CatalogRefView<'a>,
    },
    LinkDamage {
        threat: Text<ThreatId>,
        damage: Text<DamageId>,
    },
    UnlinkDamage {
        threat: Text<ThreatId>,
        damage: Text<DamageId>,
    },
    RemoveThreatScenario {
        id: Text<ThreatId>,
    },
}

/// Refuses a name or list the reader would refuse, or non-default
/// provenance, so the writer never writes a record it cannot read back.
/// Time O(1).
fn within_caps(op: &Op) -> bool {
    if op.provenance().is_some_and(|p| !p.is_default()) {
        return false;
    }
    let name_ok = |name: &str| name.len() <= MAX_NAME_BYTES;
    match op {
        Op::AddAsset { name, .. }
        | Op::RenameAsset { name, .. }
        | Op::AddDamageScenario { name, .. }
        | Op::RenameDamageScenario { name, .. }
        | Op::RenameThreatScenario { name, .. } => name_ok(name),
        Op::AddThreatScenario {
            name,
            catalog_refs,
            damages,
            ..
        } => {
            name_ok(name)
                && catalog_refs.len() <= MAX_CATALOG_REFS
                && damages.len() <= MAX_DAMAGE_LINKS
        }
        Op::AddAttackTree { .. }
        | Op::SetStepFeasibility { .. }
        | Op::SetThreatAttackTree { .. }
        | Op::RemoveAttackTree { .. } => false,
        _ => true,
    }
}

/// Builds the borrowed view of `op`. Index-to-key lookups happen while it
/// is serialized. Returns `None` for ops not in the v1 op log format.
/// Time O(1).
fn op_view<'a>(m: &'a Methodology, order: &'a KeyOrder, op: &'a Op) -> Option<OpView<'a>> {
    let props = |set| PropertyKeys { m, order, set };
    Some(match op {
        Op::AddAsset {
            id,
            name,
            properties,
            ..
        } => OpView::AddAsset {
            id: Text(*id),
            name,
            properties: props(*properties),
        },
        Op::RenameAsset { id, name, .. } => OpView::RenameAsset {
            id: Text(*id),
            name,
        },
        Op::SetAssetProperties { id, properties, .. } => OpView::SetAssetProperties {
            id: Text(*id),
            properties: props(*properties),
        },
        Op::RemoveAsset { id } => OpView::RemoveAsset { id: Text(*id) },
        Op::AddDamageScenario {
            id, name, impacts, ..
        } => OpView::AddDamageScenario {
            id: Text(*id),
            name,
            impacts: ImpactsInOrder { m, impacts },
        },
        Op::RenameDamageScenario { id, name, .. } => OpView::RenameDamageScenario {
            id: Text(*id),
            name,
        },
        Op::SetImpact {
            id,
            category,
            level,
            ..
        } => OpView::SetImpact {
            id: Text(*id),
            category: Required(m.impact_category_key(*category)),
            level: level.map(|l| Required(m.level_key(*category, l))),
        },
        Op::RemoveDamageScenario { id } => OpView::RemoveDamageScenario { id: Text(*id) },
        Op::AddThreatScenario {
            id,
            name,
            asset,
            category,
            catalog_refs,
            damages,
            ..
        } => OpView::AddThreatScenario {
            id: Text(*id),
            name,
            asset: Text(*asset),
            category: Required(m.threat_category_key(*category)),
            catalog_refs: CatalogRefsInOrder {
                m,
                refs: catalog_refs,
            },
            damages: Ids(damages),
        },
        Op::RenameThreatScenario { id, name, .. } => OpView::RenameThreatScenario {
            id: Text(*id),
            name,
        },
        Op::SetThreatCategory { id, category, .. } => OpView::SetThreatCategory {
            id: Text(*id),
            category: Required(m.threat_category_key(*category)),
        },
        Op::AddCatalogRef {
            threat,
            catalog_ref,
            ..
        } => OpView::AddCatalogRef {
            threat: Text(*threat),
            catalog_ref: CatalogRefView { m, r: *catalog_ref },
        },
        Op::RemoveCatalogRef {
            threat,
            catalog_ref,
            ..
        } => OpView::RemoveCatalogRef {
            threat: Text(*threat),
            catalog_ref: CatalogRefView { m, r: *catalog_ref },
        },
        Op::LinkDamage { threat, damage, .. } => OpView::LinkDamage {
            threat: Text(*threat),
            damage: Text(*damage),
        },
        Op::UnlinkDamage { threat, damage, .. } => OpView::UnlinkDamage {
            threat: Text(*threat),
            damage: Text(*damage),
        },
        Op::RemoveThreatScenario { id } => OpView::RemoveThreatScenario { id: Text(*id) },
        Op::AddAttackTree { .. }
        | Op::SetStepFeasibility { .. }
        | Op::SetThreatAttackTree { .. }
        | Op::RemoveAttackTree { .. } => return None,
    })
}

/// Encodes one record line, `\n` included, into `buf` (cleared first). On
/// error `buf` holds garbage and must not be written. Time O(k log k).
fn encode_record(
    buf: &mut Vec<u8>,
    m: &Methodology,
    order: &KeyOrder,
    seq: u64,
    op: &Op,
) -> Result<(), StoreError> {
    buf.clear();
    if !within_caps(op) {
        return Err(StoreError::UnencodableOp);
    }
    let record = RecordView {
        seq,
        op: op_view(m, order, op).ok_or(StoreError::UnencodableOp)?,
    };
    serde_json::to_writer(&mut *buf, &record).map_err(wire::encode_error)?;
    if buf.len() > MAX_OP_RECORD_BYTES {
        return Err(StoreError::UnencodableOp);
    }
    buf.push(b'\n');
    Ok(())
}

// ---------------------------------------------------------------------------
// Decoding.
// ---------------------------------------------------------------------------

/// Turns a decoded op into an `Op` of `m`. Vocabulary was resolved while
/// decoding; only a `SetImpact` level is resolved here, against its
/// category. Lists keep their order. Time O(k).
#[allow(clippy::too_many_lines)]
fn op_from_wire(m: &Methodology, w: OpWire) -> Result<Op, StoreError> {
    let provenance = Provenance::default();
    Ok(match w {
        OpWire::AddAsset {
            id,
            name,
            properties,
        } => Op::AddAsset {
            id: id.0,
            name: name.0,
            properties: properties.0,
            provenance,
        },
        OpWire::RenameAsset { id, name } => Op::RenameAsset {
            id: id.0,
            name: name.0,
            provenance,
        },
        OpWire::SetAssetProperties { id, properties } => Op::SetAssetProperties {
            id: id.0,
            properties: properties.0,
            provenance,
        },
        OpWire::RemoveAsset { id } => Op::RemoveAsset { id: id.0 },
        OpWire::AddDamageScenario { id, name, impacts } => Op::AddDamageScenario {
            id: id.0,
            name: name.0,
            impacts: impacts.0,
            provenance,
        },
        OpWire::RenameDamageScenario { id, name } => Op::RenameDamageScenario {
            id: id.0,
            name: name.0,
            provenance,
        },
        OpWire::SetImpact {
            id,
            category,
            level,
        } => Op::SetImpact {
            id: id.0,
            category: category.0,
            level: level
                .as_ref()
                .map(|l| wire::resolve_level(m, category.0, l))
                .transpose()?,
            provenance,
        },
        OpWire::RemoveDamageScenario { id } => Op::RemoveDamageScenario { id: id.0 },
        OpWire::AddThreatScenario {
            id,
            name,
            asset,
            category,
            catalog_refs,
            damages,
        } => Op::AddThreatScenario {
            id: id.0,
            name: name.0,
            asset: asset.0,
            category: category.0,
            catalog_refs: catalog_refs.0.into_iter().map(|r| r.0).collect(),
            damages: damages.0.into_iter().map(|d| d.0).collect(),
            provenance,
        },
        OpWire::RenameThreatScenario { id, name } => Op::RenameThreatScenario {
            id: id.0,
            name: name.0,
            provenance,
        },
        OpWire::SetThreatCategory { id, category } => Op::SetThreatCategory {
            id: id.0,
            category: category.0,
            provenance,
        },
        OpWire::AddCatalogRef {
            threat,
            catalog_ref,
        } => Op::AddCatalogRef {
            threat: threat.0,
            catalog_ref: catalog_ref.0,
            provenance,
        },
        OpWire::RemoveCatalogRef {
            threat,
            catalog_ref,
        } => Op::RemoveCatalogRef {
            threat: threat.0,
            catalog_ref: catalog_ref.0,
            provenance,
        },
        OpWire::LinkDamage { threat, damage } => Op::LinkDamage {
            threat: threat.0,
            damage: damage.0,
            provenance,
        },
        OpWire::UnlinkDamage { threat, damage } => Op::UnlinkDamage {
            threat: threat.0,
            damage: damage.0,
            provenance,
        },
        OpWire::RemoveThreatScenario { id } => Op::RemoveThreatScenario { id: id.0 },
    })
}

/// Reads one line, without its `\n`, into `buf` (cleared first). Returns
/// `Ok(false)` at a clean end of input. Never holds more than
/// [`MAX_OP_RECORD_BYTES`] bytes. Time O(line bytes).
///
/// # Errors
///
/// [`StoreError::TruncatedRecord`] if the input ends inside the line,
/// [`StoreError::LimitExceeded`] if the line is too long, or I/O errors.
fn read_line<R: BufRead>(r: &mut R, buf: &mut Vec<u8>, line: u64) -> Result<bool, StoreError> {
    buf.clear();
    loop {
        let available = match r.fill_buf() {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        if available.is_empty() {
            return if buf.is_empty() {
                Ok(false)
            } else {
                Err(StoreError::TruncatedRecord { line })
            };
        }
        let newline = available.iter().position(|&b| b == b'\n');
        let chunk = match newline {
            Some(i) => available.get(..i).unwrap_or(available),
            None => available,
        };
        if buf.len().saturating_add(chunk.len()) > MAX_OP_RECORD_BYTES {
            return Err(StoreError::LimitExceeded(Limit::OP_RECORD));
        }
        buf.extend_from_slice(chunk);
        let used = chunk.len() + usize::from(newline.is_some());
        r.consume(used);
        if newline.is_some() {
            return Ok(true);
        }
    }
}

/// Rejects a `\r` anywhere in a line: lines end in `\n` only, and a raw
/// `\r` is never valid inside a JSON string. Time O(line bytes).
fn reject_cr(buf: &[u8], line: u64) -> Result<(), StoreError> {
    match buf.iter().position(|&b| b == b'\r') {
        Some(pos) => Err(StoreError::Decode {
            line,
            column: wire::to_u64(pos).saturating_add(1),
        }),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Writer.
// ---------------------------------------------------------------------------

/// Appends ops to an `aegis-oplog` v1 log.
///
/// The writer only appends: it has no API that rewrites or truncates.
/// Each record is encoded into a reused internal buffer first and written
/// with one `write_all` only when encoding succeeded, so a refused op
/// writes nothing.
///
/// Keeping a project and its log in step is the caller's job: apply the op
/// to the project, then [`append`](Self::append) it; if the append fails,
/// apply the inverse to undo it.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{AssetId, Methodology, Op, OpLogReader, OpLogWriter, Project, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let mut log = OpLogWriter::create(Vec::new(), Arc::clone(&m))?;
///
/// let op = Op::AddAsset {
///     id: AssetId::new_v7(),
///     name: "ECU".into(),
///     properties: m.all_properties(),
///     provenance: Provenance::default(),
/// };
/// let undo = project.apply(op.clone())?;
/// if let Err(e) = log.append(&op) {
///     project.apply(undo)?;
///     return Err(e.into());
/// }
/// let bytes = log.into_inner()?;
///
/// // Replaying the log onto an empty project rebuilds it.
/// let mut replayed = Project::new(Arc::clone(&m));
/// for record in OpLogReader::open(bytes.as_slice(), m)? {
///     replayed.apply(record?.op)?;
/// }
/// assert_eq!(replayed, project);
/// # Ok(())
/// # }
/// ```
pub struct OpLogWriter<W: Write> {
    w: W,
    m: Arc<Methodology>,
    order: KeyOrder,
    last_seq: u64,
    buf: Vec<u8>,
}

impl<W: Write> OpLogWriter<W> {
    /// Starts a new log on `w`: writes the header line with the
    /// methodology's key and version. The first [`append`](Self::append)
    /// gets `seq` 1.
    ///
    /// Time O(m log m) for the key order, m = methodology size.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] if writing the header fails.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let bytes = OpLogWriter::create(Vec::new(), m)?.into_inner()?;
    /// assert_eq!(
    ///     String::from_utf8(bytes)?,
    ///     "{\"format\":\"aegis-oplog\",\"version\":1,\
    ///      \"methodology\":{\"key\":\"iso21434-r155\",\"version\":\"0.1.0\"}}\n",
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn create(w: W, m: Arc<Methodology>) -> Result<Self, StoreError> {
        let mut writer = Self::resume(w, m, 0);
        let header = HeaderView {
            format: "aegis-oplog",
            version: FORMAT_VERSION,
            methodology: MethodologyView::of(&writer.m),
        };
        serde_json::to_writer(&mut writer.buf, &header).map_err(wire::encode_error)?;
        writer.buf.push(b'\n');
        writer.w.write_all(&writer.buf)?;
        Ok(writer)
    }

    /// Continues an existing log on `w`, positioned after its last record
    /// (for example a file opened for appending). `last_seq` is the
    /// sequence number of that record, as reported by
    /// [`OpLogReader::last_seq`]; the next [`append`](Self::append) gets
    /// `last_seq + 1`. Writes nothing.
    ///
    /// Time O(m log m) for the key order.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, OpLogWriter, Op};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut log = OpLogWriter::resume(Vec::new(), m, 41);
    /// assert_eq!(log.append(&Op::RemoveAsset { id: AssetId::new_v7() })?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn resume(w: W, m: Arc<Methodology>, last_seq: u64) -> Self {
        let order = KeyOrder::new(&m);
        OpLogWriter {
            w,
            m,
            order,
            last_seq,
            buf: Vec::new(),
        }
    }

    /// Appends `op` as the next record and returns its `seq`.
    ///
    /// The op is not checked against a project; log ops after
    /// [`Project::apply`](crate::Project::apply) accepted them. The record
    /// is encoded first and written with one `write_all`; nothing is
    /// written if encoding fails. Writes are not flushed; call
    /// [`into_inner`](Self::into_inner) (or flush the underlying writer)
    /// for durability. After an I/O error the log may end in a partial
    /// line, which a reader reports as [`StoreError::TruncatedRecord`];
    /// stop appending to it.
    ///
    /// Time O(k log k), extra space O(k) in a reused buffer, k = size of
    /// the op.
    ///
    /// # Errors
    ///
    /// [`StoreError::UnencodableOp`] if an index in the op is not in the
    /// writer's methodology, an impact category is repeated, a name or list
    /// is over its decode cap, the record would exceed
    /// [`MAX_OP_RECORD_BYTES`](crate::MAX_OP_RECORD_BYTES), or `seq` would
    /// overflow; [`StoreError::Io`] if writing fails.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, Op, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut log = OpLogWriter::create(Vec::new(), m)?;
    /// let id = AssetId::new_v7();
    /// assert_eq!(log.append(&Op::RemoveAsset { id })?, 1);
    /// assert_eq!(log.append(&Op::RemoveAsset { id })?, 2);
    /// assert_eq!(log.last_seq(), 2);
    /// # Ok(())
    /// # }
    /// ```
    pub fn append(&mut self, op: &Op) -> Result<u64, StoreError> {
        let seq = self
            .last_seq
            .checked_add(1)
            .ok_or(StoreError::UnencodableOp)?;
        encode_record(&mut self.buf, &self.m, &self.order, seq, op)?;
        self.w.write_all(&self.buf)?;
        self.last_seq = seq;
        Ok(seq)
    }

    /// Returns the `seq` of the last record written, or the `last_seq`
    /// given to [`resume`](Self::resume) (0 for a new log). Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let log = OpLogWriter::create(Vec::new(), Arc::new(Methodology::iso21434_r155()?))?;
    /// assert_eq!(log.last_seq(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    /// Flushes and returns the underlying writer. Time O(1) besides the
    /// flush.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] if the flush fails.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let log = OpLogWriter::create(Vec::new(), Arc::new(Methodology::iso21434_r155()?))?;
    /// let bytes = log.into_inner()?;
    /// assert_eq!(bytes.iter().filter(|&&b| b == b'\n').count(), 1);
    /// # Ok(())
    /// # }
    /// ```
    pub fn into_inner(mut self) -> Result<W, StoreError> {
        self.w.flush()?;
        Ok(self.w)
    }
}

// ---------------------------------------------------------------------------
// Reader.
// ---------------------------------------------------------------------------

/// Reads an `aegis-oplog` v1 log: the header on [`open`](Self::open), then
/// one [`LoggedOp`] per line as an iterator.
///
/// Each line is read through a reused buffer of at most
/// [`MAX_OP_RECORD_BYTES`](crate::MAX_OP_RECORD_BYTES), so memory stays
/// bounded whatever the input. Records must count 1, 2, 3, ... The
/// iterator is fused after the first error: it yields that error once and
/// then `None`.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{DamageId, Methodology, Op, OpLogReader, OpLogWriter, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut log = OpLogWriter::create(Vec::new(), Arc::clone(&m))?;
/// let op = Op::AddDamageScenario {
///     id: DamageId::new_v7(),
///     name: "Leak".into(),
///     impacts: vec![],
///     provenance: Provenance::default(),
/// };
/// log.append(&op)?;
/// let bytes = log.into_inner()?;
///
/// let mut reader = OpLogReader::open(bytes.as_slice(), m)?;
/// let record = reader.next().ok_or("empty")??;
/// assert_eq!((record.seq, record.op), (1, op));
/// assert!(reader.next().is_none());
/// # Ok(())
/// # }
/// ```
pub struct OpLogReader<R: BufRead> {
    r: R,
    m: Arc<Methodology>,
    version: Arc<str>,
    last_seq: u64,
    line: u64,
    buf: Vec<u8>,
    done: bool,
}

impl<R: BufRead> OpLogReader<R> {
    /// Reads and checks the header line: the format must be `aegis-oplog`
    /// version 1 and the methodology key must equal `m.key()`. A different
    /// methodology version is accepted; see
    /// [`methodology_version`](Self::methodology_version).
    ///
    /// Time O(header bytes + m) at most; space O(64 KiB) for the line
    /// buffer.
    ///
    /// # Errors
    ///
    /// [`StoreError::MissingHeader`] for empty input;
    /// [`StoreError::TruncatedRecord`] if the header has no `\n`;
    /// [`StoreError::LimitExceeded`], [`StoreError::Decode`],
    /// [`StoreError::UnsupportedFormat`],
    /// [`StoreError::MethodologyMismatch`], or [`StoreError::Io`].
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, OpLogReader, StoreError};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let err = OpLogReader::open(&b""[..], m).err();
    /// assert!(matches!(err, Some(StoreError::MissingHeader)));
    /// # Ok(())
    /// # }
    /// ```
    pub fn open(mut r: R, m: Arc<Methodology>) -> Result<Self, StoreError> {
        let mut buf = Vec::new();
        if !read_line(&mut r, &mut buf, 1)? {
            return Err(StoreError::MissingHeader);
        }
        reject_cr(&buf, 1)?;
        let header: HeaderWire = wire::from_slice(&m, &buf, Some(1))?;
        Ok(OpLogReader {
            r,
            m,
            version: header.methodology.version,
            last_seq: 0,
            line: 1,
            buf,
            done: false,
        })
    }

    /// Returns the methodology version in the header, which may differ
    /// from the supplied methodology's version. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, OpLogReader, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let bytes = OpLogWriter::create(Vec::new(), Arc::clone(&m))?.into_inner()?;
    /// let reader = OpLogReader::open(bytes.as_slice(), m)?;
    /// assert_eq!(reader.methodology_version(), "0.1.0");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn methodology_version(&self) -> &str {
        &self.version
    }

    /// Returns the `seq` of the last record read successfully, 0 before
    /// the first. Pass it to [`OpLogWriter::resume`] to continue the log.
    /// Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, Op, OpLogReader, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut log = OpLogWriter::create(Vec::new(), Arc::clone(&m))?;
    /// log.append(&Op::RemoveAsset { id: AssetId::new_v7() })?;
    /// let bytes = log.into_inner()?;
    /// let mut reader = OpLogReader::open(bytes.as_slice(), m)?;
    /// assert_eq!(reader.last_seq(), 0);
    /// reader.by_ref().count();
    /// assert_eq!(reader.last_seq(), 1);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    /// Returns the 1-based line of the last line read: 1 after
    /// [`open`](Self::open) (the header), 2 after the first record. Time
    /// O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, OpLogReader, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let bytes = OpLogWriter::create(Vec::new(), Arc::clone(&m))?.into_inner()?;
    /// assert_eq!(OpLogReader::open(bytes.as_slice(), m)?.line(), 1);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn line(&self) -> u64 {
        self.line
    }

    /// Reads, decodes, and checks the next record. Time O(record bytes +
    /// k log k).
    fn read_record(&mut self) -> Result<Option<LoggedOp>, StoreError> {
        let line = self.line.saturating_add(1);
        if !read_line(&mut self.r, &mut self.buf, line)? {
            return Ok(None);
        }
        self.line = line;
        reject_cr(&self.buf, line)?;
        let record: RecordWire = wire::from_slice(&self.m, &self.buf, Some(line))?;
        let expected = self.last_seq.saturating_add(1);
        if record.seq != expected || self.last_seq == u64::MAX {
            return Err(StoreError::Sequence {
                expected,
                found: record.seq,
            });
        }
        let op = op_from_wire(&self.m, record.op)?;
        self.last_seq = record.seq;
        Ok(Some(LoggedOp {
            seq: record.seq,
            op,
        }))
    }
}

impl<R: BufRead> Iterator for OpLogReader<R> {
    type Item = Result<LoggedOp, StoreError>;

    /// Yields the next record, `None` at the end of the log, or one error
    /// and then `None` forever. Time O(record bytes + k log k) per record.
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        match self.read_record() {
            Ok(Some(record)) => Some(Ok(record)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}

impl<R: BufRead> std::iter::FusedIterator for OpLogReader<R> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit;

    #[test]
    fn read_line_frames_lines() {
        let mut r: &[u8] = b"ab\n\ncd";
        let mut buf = Vec::new();
        assert!(matches!(read_line(&mut r, &mut buf, 1), Ok(true)));
        assert_eq!(buf, b"ab");
        assert!(matches!(read_line(&mut r, &mut buf, 2), Ok(true)));
        assert_eq!(buf, b"");
        assert!(matches!(
            read_line(&mut r, &mut buf, 3),
            Err(StoreError::TruncatedRecord { line: 3 })
        ));
        let mut empty: &[u8] = b"";
        assert!(matches!(read_line(&mut empty, &mut buf, 1), Ok(false)));
    }

    #[test]
    fn read_line_caps_across_buffer_refills() {
        let long = vec![b'x'; MAX_OP_RECORD_BYTES + 1];
        // A 7-byte BufReader forces many partial chunks.
        let mut r = std::io::BufReader::with_capacity(7, long.as_slice());
        let mut buf = Vec::new();
        assert!(matches!(
            read_line(&mut r, &mut buf, 1),
            Err(StoreError::LimitExceeded(Limit::OpRecordBytes { .. }))
        ));
        assert!(buf.len() <= MAX_OP_RECORD_BYTES);
        let mut exact = vec![b'x'; MAX_OP_RECORD_BYTES];
        exact.push(b'\n');
        let mut r = std::io::BufReader::with_capacity(7, exact.as_slice());
        assert!(matches!(read_line(&mut r, &mut buf, 1), Ok(true)));
        assert_eq!(buf.len(), MAX_OP_RECORD_BYTES);
    }

    #[test]
    fn reject_cr_reports_its_column() {
        assert!(reject_cr(b"abc", 4).is_ok());
        assert!(matches!(
            reject_cr(b"ab\rc", 4),
            Err(StoreError::Decode { line: 4, column: 3 })
        ));
    }

    #[test]
    fn within_caps_matches_the_decoder() {
        let id = testkit::asset_id(1);
        let ok = Op::RenameAsset {
            id,
            name: "x".repeat(MAX_NAME_BYTES).into(),
            provenance: Provenance::default(),
        };
        let long = Op::RenameAsset {
            id,
            name: "x".repeat(MAX_NAME_BYTES + 1).into(),
            provenance: Provenance::default(),
        };
        assert!(within_caps(&ok));
        assert!(!within_caps(&long));
        assert!(within_caps(&Op::RemoveAsset { id }));
    }

    #[test]
    fn encode_refuses_foreign_indices_and_leaves_no_output() {
        let m = testkit::second();
        let wide = testkit::wide();
        let order = KeyOrder::new(&m);
        let mut buf = Vec::new();
        let foreign = Op::SetAssetProperties {
            id: testkit::asset_id(1),
            properties: wide.all_properties(),
            provenance: Provenance::default(),
        };
        assert!(matches!(
            encode_record(&mut buf, &m, &order, 1, &foreign),
            Err(StoreError::UnencodableOp)
        ));
        let mut log = OpLogWriter::resume(Vec::new(), m, 0);
        assert!(log.append(&foreign).is_err());
        assert_eq!(log.last_seq(), 0);
        assert!(log.into_inner().is_ok_and(|w| w.is_empty()));
    }

    #[test]
    fn append_refuses_seq_overflow() {
        let mut log = OpLogWriter::resume(Vec::new(), testkit::second(), u64::MAX);
        let op = Op::RemoveAsset {
            id: testkit::asset_id(1),
        };
        assert!(matches!(log.append(&op), Err(StoreError::UnencodableOp)));
    }
}
