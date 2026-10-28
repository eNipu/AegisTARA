# Security Review — 107-standalone-mode (Round 1)

## Verdict
PASS

## Blocking findings
None.

## Advisory findings
None.

## Security Evaluation
- Loopback standalone server binds strictly to `127.0.0.1:0` (never `0.0.0.0`) and shuts down cleanly when the desktop window exits.
- `FolderStore` refuses oversized manifests (`64 KiB`), oversized op-log records (`MAX_OP_RECORD_BYTES`), oversized actor strings (`64 KiB`), and oversized snapshots (`MAX_SNAPSHOT_BYTES`), preventing memory exhaustion from untrusted project folders.
- `Registry::import_folder` verifies the full SHA-256 hash chain (`verify_chain`) and replays every operation through `Project::apply` before writing to the target server store.
