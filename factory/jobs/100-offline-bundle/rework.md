## Rework 100b

Owner note (2026-11-11): fix the two script issues from `slices/100b/review.md` in `scripts/offline-bundle.sh`.

1. Reject an `--installer` whose file name was already given (exit 2 with a clear message), so one installer can never silently overwrite another. Add a negative check for it to `scripts/offline-bundle-test.sh`.
2. Quote the images path so a path with a space works.
