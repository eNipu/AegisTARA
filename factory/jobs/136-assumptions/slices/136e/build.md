Commit: 64bed76 Add Assumptions page view, claim draft flow, and exclusion formatting (136e)
Files:
- crates/aegis-client/src/assumptions.rs
- crates/aegis-client/src/lib.rs
- crates/aegis-client/src/exclusion.rs
- crates/aegis-client/src/view.rs
- crates/aegis-client/src/state.rs
- crates/aegis-client/src/pages.rs
- crates/aegis-client/src/commands.rs
- crates/aegis-client/src/session.rs
- crates/aegis-client/locales/en/main.ftl
- crates/aegis-client/locales/ja/main.ftl
- crates/aegis-client/locales/pseudo/main.ftl
- crates/aegis-shell/ui/app.slint
- crates/aegis-shell/translations/ja/LC_MESSAGES/aegis-shell.po
- crates/aegis-shell/translations/pseudo/LC_MESSAGES/aegis-shell.po
- crates/aegis-client/tests/configuration.rs
- crates/aegis-client/tests/assumptions.rs
Done when:
1. `cargo test --locked -p aegis-client --test assumptions` -> exit 0 (4 passed; 0 failed)
2. `cargo test --locked -p aegis-client --test configuration` -> exit 0 (6 passed; 0 failed)
3. `cargo test --locked -p aegis-shell --test nav_groups` -> exit 0 (1 passed; 0 failed)
Notes:
- Additional verification after final edit: `cargo fmt --all` -> exit 0; `scripts/ci.sh crates/aegis-client crates/aegis-shell crates/aegis-workbench` -> exit 0 (`all steps passed in 3 crate(s)`); `cargo test --locked -p aegis-shell --test standalone` -> exit 0 (5 passed; 0 failed); `cargo test --locked -p aegis-shell --test i18n` -> exit 0 (1 passed; 0 failed).
- Changed `aegis_client::exclusion::excluded_entries` visibility from `pub(crate)` to `pub` so `crates/aegis-client/tests/assumptions.rs` can assert directly on its output.
- Non-test diff size: 394 added lines in `src/` (420 added across all non-test files, within `max_lines: 440`).
NEXT: reviewer

## Rework 1
Commit: f6f5f68 Trim assumption client diff below gate line cap (136e)
Files:
- crates/aegis-client/src/assumptions.rs
- crates/aegis-client/src/exclusion.rs
- crates/aegis-client/src/session.rs
Gate fixes:
- Reduced non-test diff from 461 to 435 counted lines (`git diff --numstat 42f2b35ef46e33b96cf1f1180022740cf6649f10..HEAD`, cap 440) without `#[rustfmt::skip]` or removing functionality/docs: extracted `switch_lead` in `exclusion.rs` and `links` in `assumptions.rs`, removed the redundant `touched` `BTreeSet` and unnecessary imports in `assumptions.rs`, and simplified `CreateClaimFromAssumption`, `set_draft`, `commit_draft`, and `show_page` in `session.rs`.
Done when:
1. `cargo test --locked -p aegis-client --test assumptions` -> exit 0 (4 passed; 0 failed)
2. `cargo test --locked -p aegis-client --test configuration` -> exit 0 (6 passed; 0 failed)
3. `cargo test --locked -p aegis-shell --test nav_groups` -> exit 0 (1 passed; 0 failed)
Notes:
- `cargo fmt --all` -> exit 0; `scripts/ci.sh crates/aegis-client crates/aegis-shell crates/aegis-workbench` -> exit 0 (`all steps passed in 3 crate(s)`).
NEXT: reviewer
