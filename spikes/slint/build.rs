fn main() {
    // Light theme as required by the SPEC (the plain "fluent" style follows the OS dark mode).
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-light".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint build failed");
}
