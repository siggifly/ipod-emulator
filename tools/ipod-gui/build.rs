// The window's markup is compiled to Rust at build time rather than interpreted at run time, so a
// typo in the UI is a compile error and the shipped binary carries no parser.
//
// And, since docs/GUI.md §20 item 9: **every geometry constant reaches the markup from one Rust
// source**. `src/geometry.rs` declares them once; this script renders them into a `.slint` global
// and hands it to the compiler as a library. The tests read the same module, so a ratio cannot be
// edited in the markup and left green in a test — which is what happened, twice, to a screen ratio
// that shipped stretched.
//
// `src/geometry.rs` is compiled by this script as well as by the crate. It must therefore `use`
// nothing outside `core`/`std`: a build script sees only its own build-dependencies, and the error
// from a stray import points here rather than at the line that caused it.
#[allow(dead_code)]
#[path = "src/geometry.rs"]
mod geometry;

fn main() {
    let out = std::path::PathBuf::from(
        std::env::var_os("OUT_DIR").expect("build scripts are run by cargo"),
    );
    let generated = out.join("geometry.slint");

    // **Write only if the bytes changed, and it is load-bearing.** `slint-build` prints a
    // `cargo:rerun-if-changed` for every file the slint compiler loaded, which now includes this
    // one. Rewriting it unconditionally bumps its mtime on every run, cargo sees the dependency
    // newer than the fingerprint, and the crate recompiles on every single `cargo build` — for
    // ever, and it reads as Slint being slow rather than as a bug.
    let source = geometry::slint_source();
    let stale = std::fs::read(&generated)
        .map(|old| old != source.as_bytes())
        .unwrap_or(true);
    if stale {
        std::fs::write(&generated, &source)
            .unwrap_or_else(|e| panic!("could not write {}: {e}", generated.display()));
    }

    // `slint-build` only knows about files the slint compiler read. It has never heard of
    // `src/geometry.rs`, so without these two lines editing a ratio would not rebuild the markup.
    println!("cargo:rerun-if-changed=src/geometry.rs");
    println!("cargo:rerun-if-changed=build.rs");

    // **`with_library_paths`, not `with_include_paths`.** An import beginning `@` is resolved only
    // through the library paths (`i-slint-compiler-1.17.1/typeloader.rs:1259`); everything else
    // goes through the include path, whose first candidate directory is the importing file's own
    // (`typeloader.rs:1690-1693`). Under an include-path arrangement a stray `ui/geometry.slint`
    // would silently win over the generated one. `@geometry` cannot be shadowed by any file on
    // disk.
    let config = slint_build::CompilerConfiguration::new().with_library_paths(
        std::collections::HashMap::from([("geometry".to_string(), generated)]),
    );

    // **Exactly one `compile*` call in this script, and that is a constraint rather than a style.**
    // `compile_with_config` ends by printing `cargo:rustc-env=SLINT_INCLUDE_GENERATED=<path>`, and
    // `slint::include_modules!()` expands to `include!(env!("SLINT_INCLUDE_GENERATED"))`. A second
    // call clobbers it and the window component simply stops existing — the failure is
    // `cannot find type MainWindow in this scope`, which looks like a markup error and is not. So
    // the generated `.slint` is never compiled as a root; it is only ever imported.
    slint_build::compile_with_config("ui/window.slint", config)
        .expect("ui/window.slint failed to compile");
}
