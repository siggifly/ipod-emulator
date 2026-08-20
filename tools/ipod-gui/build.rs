// The window's markup is compiled to Rust at build time rather than interpreted at run time, so a
// typo in the UI is a compile error and the shipped binary carries no parser.
fn main() {
    slint_build::compile("ui/window.slint").expect("ui/window.slint failed to compile");
}
