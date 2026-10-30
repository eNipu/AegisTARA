//! Builds the Slint window with the widget style of the *target*, not the
//! host (UI-07.4, D25).
//!
//! `CARGO_CFG_TARGET_OS` is the target triple's OS, so cross-compiling a
//! Windows build on a Mac still selects `fluent`. The chosen style is also
//! passed to the crate as `AEGIS_SLINT_STYLE` so a cfg-gated test can prove
//! the selection matches the platform.

fn main() {
    // The Slint compiler turns ui/app.slint into Rust at build time, with
    // the bundled gettext catalogs from translations/ (L10N-01, L10N-02).
    // `DefaultTranslationContext::None` keeps the .po files a plain
    // msgid/msgstr mapping (no per-component context).
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let style = match target_os.as_str() {
        // Cupertino is Slint's macOS widget style; Fluent is its Windows
        // style and the closest fit for Linux and other targets.
        "macos" => "cupertino",
        _ => "fluent",
    };
    println!("cargo:rustc-env=AEGIS_SLINT_STYLE={style}");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");
    println!("cargo:rerun-if-changed=build.rs");

    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new()
            .with_style(style.to_owned())
            .with_bundled_translations("translations/")
            .with_default_translation_context(slint_build::DefaultTranslationContext::None),
    )
    .expect("the Slint UI compiles");
}
