//! A harness, and nothing else: it mounts `src/composer.rs` so that its own tests compile and run.
//!
//! **Why this exists.** `ipod-gui` is a binary crate whose module list lives in `src/main.rs`, and a
//! module nobody declares is a module nobody compiles — so `composer.rs` and every test in it would
//! be text on disk until `main.rs` gains a `mod composer;`. That line belongs to whoever owns
//! `main.rs`; this file is what lets the rules be proved in the meantime, rather than asserted.
//!
//! It states no rule, holds no fixture and duplicates nothing: every test is inside `composer.rs`,
//! where the code it is about is, and `include_str!` in there resolves against that file either way.
//!
//! **Retirement condition**: `src/main.rs` declares `mod composer;`. At that point the module is
//! compiled and tested as part of the binary, this file is a second copy of that, and it is deleted.
//! (Until then the tests here and the ones there would simply run twice, which is wasteful and not
//! wrong.)

// No `#[allow(dead_code)]` here: `composer.rs` carries its own, with its own retirement condition,
// and a second one is a duplicated attribute that clippy reports.
#[path = "../src/composer.rs"]
mod composer;

/// The harness has to have a test of its own, or a cargo run of it reports nothing at all and an
/// empty run is indistinguishable from a run that did not happen.
#[test]
fn the_composer_module_is_mounted() {
    let c = composer::Composer::new();
    assert!(
        c.plan().is_empty(),
        "a Composer that has just been made already has a plan"
    );
}
