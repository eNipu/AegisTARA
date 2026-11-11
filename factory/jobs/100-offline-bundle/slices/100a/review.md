VERDICT: PASS
## For the owner
What changed: the license crate can now write and check an offline bundle folder for air-gapped sites.
It lists every file with its SHA-256 in `SHA256SUMS`, signs that list with the update key (a raw Ed25519 signature that OpenSSL 3 can check), and checks that every installer is in the signed update manifest.
It can also print the update public key as a PEM for customers. No CLI, script or docs yet; later slices add those.
Look at:
- crates/aegis-license/src/bundle.rs:309-358 (`verify_bundle_dir`): the order of checks is the security core. The signature is checked before anything in the list is trusted, the file set must match the list exactly, then the hashes, the manifest signature and the installer-to-artifact match.
- crates/aegis-license/src/issuer.rs:89-96 (`sign_bundle_sums`): the update key now signs a second kind of message with no domain prefix. It is safe only because `parse_sums` runs first, and a license or manifest message starts with `aegis-...`, where the first 64 bytes of a sums line must be lowercase hex.
- crates/aegis-license/src/bundle.rs:210-235 (`walk`): the folder walk never follows links, has a depth limit and an entry cap, and checks every name before it is used.
Risk: low. A malformed or tampered bundle is refused with an exact error. Tests check each error variant. `parse_sums` is proptested for panics. Two small limits:
(1) `read_regular` and `sha256_file` check `symlink_metadata` and then open by path. If someone swaps a file for a symlink during the check, the open follows it. That needs write access to the folder while the check runs, which an offline USB bundle does not give.
(2) The entry cap counts the `installers` folder and the two signature files. So a bundle with an `installers/` folder holds at most 63 files on disk, while `parse_sums` allows 64. `sums_for_dir` refuses the same folder, so the writer and the checker agree.
View: git -C factory/worktrees/100-offline-bundle diff 3764940a9f1adf1cb1e23c4ca64c58d6a43ef45d..HEAD
## Findings
- No defects found. Every "Done when" item is met, and every test in tests/bundle.rs would fail without the new code.
- Optional, not blocking: tests/bundle.rs:194-195 check the `aegis-update-v1\0...` and `aegis-license-v1\0...` refusals without a final `\n`, so they fail on the "no final newline" rule, not on the domain prefix. The code still refuses them with a newline (`g`, `l` and `-` are not hex), but adding `\n` to both strings would test the stated safety argument directly.
## Lesson
[reviewer] When a test proves "X can never be signed", check that the sample input fails for the reason the safety claim gives, not for an earlier unrelated rule (job 100a domain-prefix samples had no final newline).
NEXT: human
