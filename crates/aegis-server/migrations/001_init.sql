-- AegisTARA server schema, migration 1.
--
-- org_id is in the schema from the start (direction section 2): v1 runs one
-- org per deployment, but a hosted offering later needs no schema change.
--
-- Source of truth: project_op (append-only). project.snapshot is a cache
-- that bounds replay time (standards D10); it may lag last_seq.

CREATE TABLE aegis_project (
    project_id          uuid        PRIMARY KEY,
    org_id              uuid        NOT NULL,
    name                text        NOT NULL,
    methodology_key     text        NOT NULL,
    methodology_version text        NOT NULL,
    created_ms          bigint      NOT NULL CHECK (created_ms >= 0),
    -- Sequence number of the last stored op, and the hash chain head.
    last_seq            bigint      NOT NULL DEFAULT 0 CHECK (last_seq >= 0),
    last_hash           bytea       NOT NULL CHECK (octet_length(last_hash) = 32),
    -- The snapshot reflects ops 1..=snapshot_seq.
    snapshot_seq        bigint      NOT NULL DEFAULT 0,
    snapshot            bytea       NOT NULL,
    CHECK (snapshot_seq >= 0 AND snapshot_seq <= last_seq)
);

CREATE INDEX aegis_project_org ON aegis_project (org_id, project_id);

CREATE TABLE aegis_project_op (
    project_id   uuid   NOT NULL REFERENCES aegis_project (project_id),
    seq          bigint NOT NULL CHECK (seq >= 1),
    actor        text   NOT NULL,
    timestamp_ms bigint NOT NULL CHECK (timestamp_ms >= 0),
    -- Canonical op JSON (OpCodec).
    op           bytea  NOT NULL,
    -- Audit hash chain (store/audit.rs).
    prev_hash    bytea  NOT NULL CHECK (octet_length(prev_hash) = 32),
    hash         bytea  NOT NULL CHECK (octet_length(hash) = 32),
    PRIMARY KEY (project_id, seq)
);
