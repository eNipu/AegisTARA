# Build 208a

Commit: 2387edb Move the login providers into the client core and add the sign-in view model (208a)

Files:
- crates/aegis-shell/src/provider.rs -> crates/aegis-client/src/provider.rs (git mv, 32+/9-)
- crates/aegis-client/src/signin.rs (new)
- crates/aegis-client/src/lib.rs (`pub mod provider;`, `pub mod signin;`)
- crates/aegis-client/Cargo.toml (`[features] os-keychain = ["aegis-auth/os-keychain"]`)
- crates/aegis-client/locales/{en,ja,pseudo}/main.ftl (15 `signin-*` ids after `workbench-list-empty`)
- crates/aegis-client/tests/signin_view.rs (new, 7 tests)
- crates/aegis-shell/src/lib.rs (removed `pub mod provider;`)
- crates/aegis-shell/src/launcher.rs (import only: `aegis_client::provider::`)
- crates/aegis-shell/Cargo.toml (`aegis-client = { workspace = true, features = ["os-keychain"] }`)

Done when:
1. `git log --follow --oneline crates/aegis-client/src/provider.rs | head -3` -> 2387edb, fb28de6, d123e64 (older history kept).
2. `cargo build --locked -p aegis-shell` -> Finished; `scripts/ci.sh crates/aegis-client crates/aegis-shell` -> "all steps passed in 2 crate(s)" (fmt, clippy, test for both).
3. `cargo test --locked -p aegis-client --test signin_view` -> 7 passed.
4. `cargo test --locked -p aegis-client i18n` -> 3 passed.
Also: `cargo clippy --locked -p aegis-client --features os-keychain --all-targets -- -D warnings` clean; Cargo.lock unchanged; `cargo fmt --all` run.
Source lines (no tests, no locales): about 216 of the 320 cap.

Notes:
- Deviation from "change nothing else in the providers": `OsKeychain` exists only under
  aegis-auth's `os-keychain` feature, and `OidcProvider::session()` also calls
  `OsKeychain::open` directly. Without the feature aegis-client would not compile, so
  that block got the same `#[cfg(feature = "os-keychain")]` / memory-store fallback as
  `secrets()`, keeping its own warning text. The `OsKeychain` import is gated too. The
  `use aegis_client::connection::...` import became `use crate::connection::...`
  (required by the move). Behaviour with the feature on (the Slint app) is unchanged.
- `Secrets` got a doc line because it is now public (missing_docs).
- Pseudo texts follow the file's style: lowercase vowels accented, placeholders kept,
  wrapped in `⟦...␣⟧`. Japanese texts are my own translations.
- Tests go a little past the prompt: Offline with no Offline connection, a non-listed
  server code returns the server's message, a non-"not implemented" Other code is not
  the too-old text, plus `grpc`/`auto` transport names. `assert_translated` also checks
  the text is not `[id]`, which is what `Catalog::tr` returns for a missing id.
- `SignInView` derives `Default`; `Stage` uses `#[default]` on `ChooseServer`.

NEXT: reviewer
