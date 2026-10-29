fn main() {
    // The Slint compiler turns ui/app.slint into Rust at build time, with
    // the bundled gettext catalogs from translations/ (L10N-01, L10N-02).
    // `DefaultTranslationContext::None` keeps the .po files a plain
    // msgid/msgstr mapping (no per-component context).
    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new()
            .with_bundled_translations("translations/")
            .with_default_translation_context(slint_build::DefaultTranslationContext::None),
    )
    .expect("the Slint UI compiles");
}
