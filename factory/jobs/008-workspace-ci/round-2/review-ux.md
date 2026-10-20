VERDICT: PASS

Round 2 only swaps the cache action (rust-cache to actions/cache) and updates the ADR and README. `scripts/ci.sh` is unchanged since round 1, so the round 1 review of its output and error messages still holds. No GUI, so keyboard access, accessible labels and the 50 ms budget do not apply. Reviewed from the diff and build.md; I did not re-run the scripts or the workflow.

User-facing surfaces: CI check names and logs on a commit or PR, `scripts/ci.sh` output, and the README section.

Flow and errors
- Check names are still "fmt, clippy, test", "cargo-deny", "MSRV cargo check". A red check says which gate failed.
- `.github/workflows/ci.yml:62-82` — the new cache steps do not hide failures. Restore on a miss is not an error (a cold run is just slower). The save step runs only on master, only after a green `scripts/ci.sh` (implicit `success()`), and skips an exact hit. A cache problem cannot turn a green gate red, and a red gate cannot be masked by the cache.
- `.github/workflows/ci.yml:62-82` — step names ("List cache paths", "Restore cargo cache", "scripts/ci.sh", "Save cargo cache") read well in the log, so a reader can tell cache time from test time.
- `README.md:33-39` — now says "see `RUST_TOOLCHAIN` in `ci.yml`" instead of repeating the version (round 1 suggestion 5 is fixed). The local commands are unchanged and correct.

Non-blocking (carried from round 1, the builder declined them within the "keep other choices" rework, and I agree they are small follow-ups)
1. `scripts/ci.sh` — a bare `FAIL <crate> <step>` has no fix hint (for example `cargo fmt` for fmt, commit `Cargo.lock` for a stale lock).
2. `scripts/ci.sh` — on GitHub, failing output is inside the folded `::group::`; the `::error` annotation names crate and step, so the reader finds it quickly.
3. `.github/workflows/ci.yml` msrv step — add a hint that a dependency needs a newer compiler and that `rust-version` should not be raised without an ADR.
4. `scripts/ci.sh` — a cold local run prints nothing until the first step ends (about 24 s today). An optional `run   <crate> <step>` line would help.
5. `cancel-in-progress: true` also applies to master. The builder notes the consequence for the cache: a cancelled master run skips its save and the next master run saves. Acceptable.

No blocking findings.
