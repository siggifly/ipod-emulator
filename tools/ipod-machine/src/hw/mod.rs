//! **The peripherals, one module each.**
//!
//! Every type in here answers memory-mapped registers and, where the part has an interrupt line,
//! raises one. That is the whole of what a peripheral does — and the rule that keeps it true is
//! that none of them may reach a front end, read a setting, or know which operating system is
//! booting. It is what lets stock Rockbox and stock iPodLinux be oracles: they cannot be
//! special-cased by something that cannot see them.
//!
//! **These types were never new; they were never separate either.** All of them lived in
//! `lib.rs` together — 14 246 lines holding a machine, nine peripherals, the eApp game runtime
//! and a texture rasteriser behind one name that describes none of them. Each already had its own
//! state and its own register decoding, so giving it a file is a move rather than a rewrite, and
//! the line count is the check: nothing is rephrased on the way across.
//!
//! **The reason to do it is the second SoC.** The 6G and nano 3G are Samsung S5L87xx, a different
//! part with a different map. With the peripherals named, that generation is new modules against
//! the same machine; without them it is a fork of one enormous file.
//!
//! Everything here is re-exported from the crate root, so nothing outside had to change.

pub mod backlight;
pub mod wheel;
pub mod mailbox;
pub mod flash;
pub mod pmu;
pub mod ata;
pub mod cop;
pub mod video;
