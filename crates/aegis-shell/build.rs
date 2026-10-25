fn main() {
    // The Slint compiler turns ui/app.slint into Rust at build time.
    slint_build::compile("ui/app.slint").expect("the Slint UI compiles");
}
