-- AegisTARA server schema, migration 2 (jobs 037 and 039).
--
-- Password accounts (docs/adr/0035-password-login.md), the server event log
-- (logins, exports, admin actions), and project baselines
-- (docs/adr/0037-baselines-and-reports.md).

CREATE TABLE aegis_user (
    org_id         uuid    NOT NULL,
    username       text    NOT NULL CHECK (octet_length(username) BETWEEN 1 AND 64),
    display_name   text    NOT NULL,
    -- Argon2id PHC string. Never a plain password.
    password_hash  text    NOT NULL,
    is_admin       boolean NOT NULL,
    disabled       boolean NOT NULL,
    created_ms     bigint  NOT NULL CHECK (created_ms >= 0),
    last_login_ms  bigint  NOT NULL DEFAULT 0 CHECK (last_login_ms >= 0),
    PRIMARY KEY (org_id, username)
);

-- Only the SHA-256 of a session token is stored: a dump holds no usable
-- token.
CREATE TABLE aegis_session (
    token_hash   bytea  PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    org_id       uuid   NOT NULL,
    username     text   NOT NULL,
    created_ms   bigint NOT NULL CHECK (created_ms >= 0),
    expires_ms   bigint NOT NULL CHECK (expires_ms >= created_ms),
    client_name  text   NOT NULL,
    FOREIGN KEY (org_id, username) REFERENCES aegis_user (org_id, username) ON DELETE CASCADE
);

CREATE INDEX aegis_session_user ON aegis_session (org_id, username);

-- Append-only. Never holds a password or a token.
CREATE TABLE aegis_event (
    event_id  bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    org_id    uuid   NOT NULL,
    at_ms     bigint NOT NULL CHECK (at_ms >= 0),
    actor     text   NOT NULL,
    kind      text   NOT NULL,
    target    text   NOT NULL,
    detail    text   NOT NULL
);

CREATE INDEX aegis_event_org_time ON aegis_event (org_id, event_id DESC);

-- A named, frozen project state: the snapshot after ops 1..=seq.
CREATE TABLE aegis_baseline (
    baseline_id  uuid   PRIMARY KEY,
    project_id   uuid   NOT NULL REFERENCES aegis_project (project_id),
    name         text   NOT NULL,
    seq          bigint NOT NULL CHECK (seq >= 0),
    created_by   text   NOT NULL,
    created_ms   bigint NOT NULL CHECK (created_ms >= 0),
    snapshot     bytea  NOT NULL
);

CREATE INDEX aegis_baseline_project ON aegis_baseline (project_id, baseline_id);

-- Seat rebuild after a restart (backlog 039) reads the actors of recent ops.
CREATE INDEX aegis_project_op_time ON aegis_project_op (timestamp_ms);
