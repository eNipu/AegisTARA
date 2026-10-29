-- AegisTARA server schema, migration 4 (job 062).
--
-- Hash chain over the server event log (SEC-06). Each event row carries the
-- previous hash and its own SHA-256 link hash; `aegis_event_chain` holds the
-- per-organization tail anchor (advanced on retention trims) and head hash
-- (advanced on every event append and trim checkpoint).

ALTER TABLE aegis_event
    ADD COLUMN prev_hash bytea NOT NULL DEFAULT '\x0000000000000000000000000000000000000000000000000000000000000000'::bytea CHECK (octet_length(prev_hash) = 32),
    ADD COLUMN hash      bytea NOT NULL DEFAULT '\x0000000000000000000000000000000000000000000000000000000000000000'::bytea CHECK (octet_length(hash) = 32);

CREATE TABLE aegis_event_chain (
    org_id     uuid  PRIMARY KEY,
    tail_hash  bytea NOT NULL CHECK (octet_length(tail_hash) = 32),
    head_hash  bytea NOT NULL CHECK (octet_length(head_hash) = 32)
);
