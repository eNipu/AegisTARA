VERDICT: PASS

User-facing surfaces: the CLI output of `scripts/ci.sh`, the CI log and annotations, the check names on a commit or PR, and the README section. No GUI. Reviewed from the diff and build.md; I did not re-run the scripts.

Flow against the spec goal
- One local command (`scripts/ci.sh`, optionally with a crate dir) runs the same gates as the `check` job. README lists it, plus the deny scripts. That is the fewest steps.
- Check names on the commit are readable: "fmt, clippy, test", "cargo-deny", "MSRV cargo check". A red check says which gate failed.

Errors
- `scripts/ci.sh:55-56` — each failing step prints `FAIL  <crate> <step>` and the full tool output, indented. rustfmt prints its diff, cargo test prints the panic, and `--locked` prints cargo's own "lock file needs to be updated" message. The tool says what is wrong, so this is enough.
- `scripts/ci.sh:70-75, 97-103` — a missing `Cargo.toml` and "no crates found" both name the path and exit 1. No silent success.
- `scripts/ci.sh:106-108` — the summary line `N step(s) failed in M crate(s): a/fmt b/test` names crate and step. Good.
- Actions: `::error title=ci.sh::FAIL <crate> <step>` shows the crate and step in the run summary without opening the folded group.

Non-blocking suggestions (none block the merge)
1. `scripts/ci.sh:55` — a bare `FAIL a fmt` gives no next step. Add a one-line hint per step on failure, for example `fix: run 'cargo fmt' in <dir>` for fmt and `run 'cargo update -w' / commit Cargo.lock` for a stale lock. Not required, because the tool output already says what is wrong.
2. `scripts/ci.sh:77-81` — on GitHub the failure output is inside `::group::`, so it is folded. The annotation points to the crate, but the reader must expand the group. Consider emitting `::endgroup::` before printing failing output, or printing the failing output after the group closes.
3. `.github/workflows/ci.yml:21-23` — `cancel-in-progress: true` also applies to `master`. A quick second push to master cancels the first commit's run, which then shows as cancelled, not green or red, and its cache is not saved. Consider `cancel-in-progress: ${{ github.ref != 'refs/heads/master' }}`. The spec asked for plain `true`, so this is optional.
4. `.github/workflows/ci.yml:117-121` — the MSRV failure prints only `FAIL <crate> msrv`. Add a hint such as "a dependency or code needs a newer compiler than rust-version; do not raise rust-version without an ADR".
5. `README.md:38-39` — the README repeats the pinned version `1.99.1`. It will drift on the next bump. Say "see `RUST_TOOLCHAIN` in ci.yml" instead.
6. `scripts/ci.sh` — a cold local run prints nothing until a step ends (about 24 s today, longer with more crates). Optional: print a `run   <crate> <step>` line to stderr first so the terminal does not look hung.

Accessibility and keyboard: not applicable (no GUI). Output uses plain-text `ok`/`FAIL` words, not colour alone. The 50 ms UI budget does not apply.
