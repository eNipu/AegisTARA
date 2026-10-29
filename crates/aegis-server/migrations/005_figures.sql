-- AegisTARA server schema, migration 5 (job 073: figure attachments, ARC-01).

CREATE TABLE aegis_figure (
    figure_id   uuid    PRIMARY KEY,
    project_id  uuid    NOT NULL REFERENCES aegis_project (project_id) ON DELETE CASCADE,
    caption     text    NOT NULL,
    target      text    NOT NULL,
    media_type  text    NOT NULL,
    width       integer NOT NULL CHECK (width > 0),
    height      integer NOT NULL CHECK (height > 0),
    byte_len    bigint  NOT NULL CHECK (byte_len > 0),
    sha256      bytea   NOT NULL CHECK (octet_length(sha256) = 32),
    created_by  text    NOT NULL,
    created_ms  bigint  NOT NULL CHECK (created_ms >= 0),
    data        bytea   NOT NULL
);

CREATE INDEX aegis_figure_project ON aegis_figure (project_id, figure_id);
