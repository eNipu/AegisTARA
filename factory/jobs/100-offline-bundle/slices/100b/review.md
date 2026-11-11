VERDICT: PASS
## For the owner
What changed: the vendor tool `aegis-license` gains `sign-bundle`, `verify-bundle` and `public-pem`. `scripts/offline-bundle.sh` builds the signed offline archive and its `.sha256`, and `scripts/offline-bundle-test.sh` checks it as a customer would (OpenSSL 3 plus sha256sum). CI's `check` job runs that test.
Rework (67ce712): the bundle script now refuses two `--installer` files with the same file name (exit 2, "installer file name given twice"), and an `--images` path with a space in it now works. The test script has a check for each.
No library code, file format or dependency changed.
Look at:
- scripts/offline-bundle.sh:79-95: the duplicate-name check. It compares base names line by line. Base names cannot contain a newline because the name pattern check runs first (lines 70-77), so a match cannot be faked.
- scripts/offline-bundle.sh:63-66: `"$images"` is now quoted, and an empty value is skipped. The copy at line 125 was already quoted.
- scripts/offline-bundle-test.sh:142-159: the two new checks. Both would fail without the fix. Without the duplicate check, the two identical installers would build and verify a bundle (exit 0, not 2). Without the quoting, "img dir/images.tar.gz" would be split into two words and rejected as "not a regular file".
Risk: low. The rest is unchanged from the first review: the release in the output path comes from a manifest whose signature is checked and is limited to `major.minor.patch`, and the private key is never copied into the bundle. A run that fails halfway still leaves a partial `$out/aegistara-<release>-offline/` folder, and the next run refuses until someone deletes it. That is safe.
View: git -C factory/worktrees/100-offline-bundle diff 7251c9a9050f1c442e57ef95ab885959a711a05d..HEAD
## Findings
None. Both owner notes from the rework are fixed and tested. The gate passed (362 counted lines, all three checks ok). Only the two script files changed in the rework, and both are inside the slice's file list.
## Lesson
none
NEXT: human
