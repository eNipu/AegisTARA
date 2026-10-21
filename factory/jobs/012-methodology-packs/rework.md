## Answers

The owner answered the spec's open question on 2026-10-22:

1. **Pack identity and trust:** confirmed. A pack is identified by `key` + `version` only. No signature and no content hash in this job, so no new dependency and the project file format stays v1. ADR 0011 records the risk that two different pack files with the same key and version could change a project's meaning, and states that signing or hashing belongs to the later server pack-registry job.

All other decisions stay as the spec settled them, including the one public API change (`Methodology::iso21434_r155()` now returns `Result<Methodology, PackError>`).
