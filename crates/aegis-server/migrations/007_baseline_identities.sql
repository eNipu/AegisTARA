-- Record engine and methodology pack identities on baselines (ADR 0075).
ALTER TABLE aegis_baseline
    ADD COLUMN IF NOT EXISTS engine_version TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS engine_semantics INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS methodology_key TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS methodology_version TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS methodology_digest TEXT NOT NULL DEFAULT '';
