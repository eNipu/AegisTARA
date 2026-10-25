-- AegisTARA server schema, migration 3 (job 045).
--
-- Settings an administrator changes in the app
-- (docs/adr/0039-password-login-switch.md). One row per setting.
-- `password_login` is `on` or `off`; no row means `on`.

CREATE TABLE aegis_setting (
    org_id  uuid NOT NULL,
    key     text NOT NULL CHECK (octet_length(key) BETWEEN 1 AND 64),
    value   text NOT NULL CHECK (octet_length(value) <= 256),
    PRIMARY KEY (org_id, key)
);
