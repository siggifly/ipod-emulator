//! Doom's assets, and what it takes to make Rockbox's plugin actually play.
//!
//! # Why this is a module and not two `curl` lines
//!
//! `doom.rock` ships inside the Rockbox release, so a stock install already has the plugin — and it
//! will not play. Running it produces a splash and nothing else, because the plugin needs two files
//! that Rockbox deliberately does not distribute: a **base PWAD** carrying the renderer's lookup
//! tables, and a **game IWAD** carrying the levels.
//!
//! The first attempt here got as far as Doom's own menu with a twelve-byte empty PWAD, because
//! `Dbuild_base` only asks whether the file opens (`rockdoom.c:294`, `fileexists` returning 0 on
//! success). **That was written up as "and it plays". It does not.** Pressing `Play Game` gives:
//!
//! ```text
//! W_GetNumForName: TANGTABL not found
//! R_LoadTrigTables: Invalid TANGTABL
//! ```
//!
//! `rockdoom.wad` carries lumps the renderer reads at level start, and `tables.c:2150-2170` checks
//! three of them against exact sizes — `SINETABL` 40 960, `TANGTABL` 16 384, `TANTOANG` 8 196 —
//! with `W_LumpLength(lump) != N` and an `I_Error` on mismatch. Nothing approximate gets past it.
//! research/06 has the full account, including the retraction.
//!
//! # Why Freedoom, and why that choice is not ours to be clever about
//!
//! Doom's own IWADs are commercial and cannot be named, fetched or shipped by a public repository.
//! Rockbox's own manual settles it: `manual/plugins/doom.tex` says *"A free alternative for Doom 2
//! is FreeDoom … This can be used in place of `doom2.wad`"*. Freedoom is BSD-licensed and is a real
//! `IWAD` — the magic is checked at `d_main.c:546` — so it is the one that can be written down.
//!
//! **Nothing here fetches anything the user did not ask for**, and both files are hashed. A game
//! asset that arrives unverified is a 28 MB blob nobody can tell from a corrupted one.

use std::path::Path;

/// One downloadable asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wad {
    /// What it is called in the cache.
    pub file: &'static str,
    pub url: &'static str,
    /// Size in bytes, measured from the fetched file rather than taken from a page.
    pub bytes: u64,
    /// Lower-case hex SHA-256 of the bytes.
    pub sha256: &'static str,
    /// What it has to be called in `/.rockbox/doom/` for the plugin to find it.
    pub install_as: &'static str,
    /// One line for a person choosing.
    pub about: &'static str,
}

/// The two files, both measured on 2026-09-01 by fetching them.
///
/// The `zip` case is real and is why `install_as` exists separately from `file`: Freedoom ships one
/// archive holding both of its IWADs, and the plugin wants the Doom 2 one under Doom 2's name.
pub const CATALOGUE: &[Wad] = &[
    Wad {
        file: "rockdoom.wad",
        url: "https://www.rockbox.org/wiki/pub/Main/PluginDoom/rockdoom.wad",
        bytes: 285_048,
        sha256: "303f5ea5c23df6ef412fdd3b8aa85febd1748ac170024de381fa8eae1bca34c9",
        install_as: "rockdoom.wad",
        about: "the base PWAD, 186 lumps — the renderer's sine/tangent tables live in it",
    },
    Wad {
        file: "freedoom-0.13.0.zip",
        url: "https://github.com/freedoom/freedoom/releases/download/v0.13.0/freedoom-0.13.0.zip",
        bytes: 24_143_781,
        sha256: "3f9b264f3e3ce503b4fb7f6bdcb1f419d93c7b546f4df3e874dd878db9688f59",
        install_as: "doom2.wad",
        about: "Freedoom 0.13.0 — BSD-licensed levels, named by Rockbox's own manual",
    },
];

/// `freedoom2.wad` inside the archive, checked after extraction.
///
/// **Its own hash, separate from the archive's**, because the thing the plugin reads is the WAD and
/// not the zip. An archive that verifies can still be unpacked wrongly, and 28 MB of the wrong bytes
/// fails inside Doom's renderer rather than at the door.
pub const FREEDOOM2: (u64, &str) = (
    28_787_748,
    "a8772e088847032510d97ba2312406a6998f21cbab44d4ff10696faa9c0ecd4b",
);

/// The shortcut that makes Doom reachable without a counted descent.
///
/// **Rockbox accelerates the wheel**, and research/06 measured what that costs: two runs of the same
/// forward descent, 24 clicks and 18 clicks, landed on `Shortcuts` and on `Settings` — half the
/// clicks moved less than half as far. A fixed click count cannot target a menu item.
///
/// `Shortcuts` is the **last** item of the main menu, so one small backward step from `Files` wraps
/// onto it: too short to accelerate, and where it lands does not depend on how far it travelled.
pub const SHORTCUTS_TXT: &str = "[shortcut]\ntype: file\ndata: /.rockbox/rocks/games/doom.rock\nname: DOOM\n";

/// Where the plugin looks. `rockdoom.c` builds both paths from this.
pub const DOOM_DIR: &str = "/.rockbox/doom";

/// Is this drive ready to play?
///
/// Returns what is missing, so a caller can say which of three files to go and get rather than
/// "Doom does not work".
pub fn missing(disk: &Path) -> Vec<&'static str> {
    // Read-only, and an unreadable volume reports everything missing rather than erroring: a drive
    // Apple's firmware has never booted has no FAT32 on it yet, which is an ordinary state and not
    // a fault. `volume_software` takes the same line for the same reason.
    let entries = crate::fat::Fat32::open_ro(disk)
        .ok()
        .and_then(|mut v| v.walk().ok())
        .unwrap_or_default();
    let has = |needle: &str| {
        entries
            .iter()
            .any(|e| e.path.to_ascii_lowercase().contains(needle))
    };
    let mut out = Vec::new();
    if !has("rockdoom.wad") {
        out.push("rockdoom.wad");
    }
    if !has("doom2.wad") {
        out.push("doom2.wad");
    }
    if !has("shortcuts.txt") {
        out.push("shortcuts.txt");
    }
    if !has("doom.rock") {
        out.push("doom.rock (install Rockbox first)");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Every hash is a hash and every URL is https**, which is the same gate `rockbox.rs` keeps.
    ///
    /// A catalogue entry with a truncated hash is worse than none: it looks verified and checks
    /// nothing. research/06 recorded this one as `303f5ea5…` for two weeks, and the full 64 here
    /// was measured by fetching the file, not by copying the page.
    #[test]
    fn the_catalogue_is_verifiable() {
        for w in CATALOGUE {
            assert_eq!(w.sha256.len(), 64, "{}: not a sha256", w.file);
            assert!(
                w.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
                "{}: not lower-case hex",
                w.file
            );
            assert!(w.url.starts_with("https://"), "{}: not https", w.file);
            assert!(w.bytes > 0, "{}: no size", w.file);
        }
        assert_eq!(FREEDOOM2.1.len(), 64, "the extracted WAD needs its own hash");
        assert_ne!(
            FREEDOOM2.1, CATALOGUE[1].sha256,
            "the archive and the WAD inside it cannot have the same hash"
        );
    }

    /// The shortcut has to name the path Rockbox actually loads from.
    #[test]
    fn the_shortcut_points_at_the_plugin() {
        assert!(SHORTCUTS_TXT.contains("doom.rock"));
        assert!(SHORTCUTS_TXT.starts_with("[shortcut]"));
        assert!(SHORTCUTS_TXT.ends_with('\n'), "a trailing newline, or the last line is dropped");
    }
}
