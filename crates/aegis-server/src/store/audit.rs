//! The hash chain over a project's op history.
//!
//! Every stored op carries `hash = SHA-256(domain || prev || seq || time ||
//! actor || op)`, where `prev` is the hash of the op before it (or a
//! per-project genesis value). Anyone with read access to the store can
//! recompute the chain and see whether a row was changed, removed, or
//! inserted after the fact. It does not stop an attacker who can rewrite
//! the whole table and recompute every hash; for that, export the head
//! hash (`ChainReport::head`) to a system the database admin cannot write.
//!
//! The encoding is length-prefixed and fixed-width, so no two different ops
//! hash the same input (no ambiguity between actor and op bytes).

use sha2::{Digest, Sha256};

use super::StoredOp;
use crate::ids::ProjectId;

/// A SHA-256 value.
pub type Hash = [u8; 32];

const GENESIS_DOMAIN: &[u8] = b"aegis-audit-genesis-v1\0";
const LINK_DOMAIN: &[u8] = b"aegis-audit-link-v1\0";

/// The `prev` of the first op of a project.
#[must_use]
pub fn genesis(project: ProjectId) -> Hash {
    let mut h = Sha256::new();
    h.update(GENESIS_DOMAIN);
    h.update(project.as_uuid().as_bytes());
    h.finalize().into()
}

/// The hash of `op` chained after `prev`. Time O(op bytes).
#[must_use]
pub fn link(prev: &Hash, op: &StoredOp) -> Hash {
    let mut h = Sha256::new();
    h.update(LINK_DOMAIN);
    h.update(prev);
    h.update(op.seq.to_be_bytes());
    h.update(op.timestamp_ms.to_be_bytes());
    // usize -> u64 never truncates on supported targets.
    h.update((op.actor.len() as u64).to_be_bytes());
    h.update(op.actor.as_bytes());
    h.update((op.op.len() as u64).to_be_bytes());
    h.update(&op.op);
    h.finalize().into()
}

/// Result of checking a project's chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainReport {
    /// Ops whose links checked out, counting from sequence 1.
    pub verified: u64,
    /// The first sequence number whose stored hash or position is wrong.
    pub first_bad: Option<u64>,
    /// The hash of the last verified op (the genesis value if none).
    pub head: Hash,
}

impl ChainReport {
    /// Whether the whole history checked out.
    #[must_use]
    pub fn is_intact(&self) -> bool {
        self.first_bad.is_none()
    }
}

/// Walks a project's history in order and checks every link. Streaming:
/// holds one op at a time.
pub struct ChainVerifier {
    prev: Hash,
    next_seq: u64,
    verified: u64,
    first_bad: Option<u64>,
}

impl ChainVerifier {
    /// Starts at the project's genesis.
    #[must_use]
    pub fn new(project: ProjectId) -> Self {
        ChainVerifier {
            prev: genesis(project),
            next_seq: 1,
            verified: 0,
            first_bad: None,
        }
    }

    /// Checks the next stored op against its stored hash. Once one link is
    /// bad, later ops are not counted.
    pub fn push(&mut self, op: &StoredOp, stored_prev: &Hash, stored_hash: &Hash) {
        if self.first_bad.is_some() {
            return;
        }
        let expected = link(&self.prev, op);
        if op.seq != self.next_seq || *stored_prev != self.prev || *stored_hash != expected {
            self.first_bad = Some(self.next_seq);
            return;
        }
        self.prev = expected;
        self.next_seq += 1;
        self.verified += 1;
    }

    /// Finishes with the hash the project row claims as its head. A head
    /// that does not match means ops were cut off the end.
    #[must_use]
    pub fn finish(mut self, claimed_head: &Hash) -> ChainReport {
        if self.first_bad.is_none() && self.prev != *claimed_head {
            self.first_bad = Some(self.next_seq);
        }
        ChainReport {
            verified: self.verified,
            first_bad: self.first_bad,
            head: self.prev,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(seq: u64, actor: &str, bytes: &[u8]) -> StoredOp {
        StoredOp {
            seq,
            actor: actor.into(),
            timestamp_ms: seq * 10,
            op: bytes.to_vec(),
        }
    }

    fn chain(id: ProjectId, ops: &[StoredOp]) -> Vec<(Hash, Hash)> {
        let mut prev = genesis(id);
        ops.iter()
            .map(|o| {
                let h = link(&prev, o);
                let pair = (prev, h);
                prev = h;
                pair
            })
            .collect()
    }

    fn verify(id: ProjectId, ops: &[StoredOp], links: &[(Hash, Hash)], head: &Hash) -> ChainReport {
        let mut v = ChainVerifier::new(id);
        for (o, (p, h)) in ops.iter().zip(links) {
            v.push(o, p, h);
        }
        v.finish(head)
    }

    #[test]
    fn an_untouched_chain_verifies() {
        let id = ProjectId::new_v7();
        let ops = [op(1, "a", b"x"), op(2, "b", b"y"), op(3, "a", b"z")];
        let links = chain(id, &ops);
        let report = verify(id, &ops, &links, &links[2].1);
        assert!(report.is_intact());
        assert_eq!(report.verified, 3);
        assert_eq!(report.head, links[2].1);
    }

    #[test]
    fn every_field_is_covered() {
        let id = ProjectId::new_v7();
        let ops = [op(1, "a", b"x"), op(2, "b", b"y"), op(3, "a", b"z")];
        let links = chain(id, &ops);
        let head = links[2].1;
        let mut changed = Vec::new();
        for i in 0..3 {
            let mut c = ops.clone();
            c[i].op.push(0);
            changed.push((i, c));
            let mut c = ops.clone();
            c[i].actor.push('x');
            changed.push((i, c));
            let mut c = ops.clone();
            c[i].timestamp_ms += 1;
            changed.push((i, c));
        }
        for (i, c) in changed {
            let report = verify(id, &c, &links, &head);
            assert_eq!(report.first_bad, Some(i as u64 + 1), "change at {i}");
            assert_eq!(report.verified, i as u64);
        }
    }

    #[test]
    fn removed_reordered_and_truncated_histories_are_caught() {
        let id = ProjectId::new_v7();
        let ops = [op(1, "a", b"x"), op(2, "b", b"y"), op(3, "a", b"z")];
        let links = chain(id, &ops);
        let head = links[2].1;
        // A missing middle op: seq 3 where 2 is expected.
        let gap = [ops[0].clone(), ops[2].clone()];
        let gap_links = [links[0], links[2]];
        assert_eq!(verify(id, &gap, &gap_links, &head).first_bad, Some(2));
        // Cut off the end: the project row still claims the old head.
        let cut = verify(id, &ops[..2], &links[..2], &head);
        assert_eq!(cut.first_bad, Some(3));
        assert_eq!(cut.verified, 2);
        // The same ops under another project's genesis.
        let other = ProjectId::new_v7();
        assert_eq!(verify(other, &ops, &links, &head).first_bad, Some(1));
    }

    #[test]
    fn fields_cannot_trade_bytes() {
        // ("ab", "c") and ("a", "bc") must not collide.
        let prev = [0u8; 32];
        let one = link(&prev, &op(1, "ab", b"c"));
        let two = link(&prev, &op(1, "a", b"bc"));
        assert_ne!(one, two);
    }
}
