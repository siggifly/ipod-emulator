//! A title's `Manifest.plist`: its real name, the cover art the iPod itself shows, and which iPods
//! it runs on.
//!
//! # Why this is read rather than designed
//!
//! Every `.ipg` title ships a `Manifest.plist` beside its `Executables/` directory, and Apple put
//! in it exactly what a list of titles needs. `research/13` reads the key table out of RetailOS's
//! own image; what the **titles** carry, measured across the twenty decrypted ones on hand, is:
//!
//! ```text
//! Name  GUID  Version  DRM  BuildID  BuildIdentifier  ExecutablePath  Files  Verify
//! Platforms[ { PlatformID  PlatformVersion  BuildID  ExecutablePath  LaunchingArtwork  Size } ]
//! ```
//!
//! **`LocalizedNames` is in RetailOS's table and in none of the twenty manifests.** It is in the
//! union of keys the *reader* knows about, not in what the titles ship — measured 2026-09-07,
//! `grep -l '<key>LocalizedNames</key>'` over all twenty returns nothing, while `LaunchingArtwork`
//! and `PlatformID` return all twenty. So the display name is the top-level `Name`, which is what
//! [`crate::manifest_name`] has always read, and this module does not invent a second answer.
//!
//! # `PlatformID`, and what makes it actionable
//!
//! A title lists one entry per iPod generation it was built for, each with its own executable and
//! its own artwork. `research/01` settles the mapping by a natural experiment rather than by
//! assumption: the seven titles independently labelled *"5G and 5.5G only"* are exactly the seven
//! whose only platform is **1**, and every one of the fifty-six archives ships a `PlatformID 1`
//! build. So [`PLATFORM_5G`] is 1, this emulator has one target, and a title that does not list it
//! is one this program cannot run — which is a sentence the window can say **before** loading
//! anything, rather than a failure inside the loader.
//!
//! # This is a scan, not a plist parser
//!
//! It looks for a known shape in a document Apple generated, and says so rather than pretending to
//! be general: no nested-array recursion, no `<data>`, no type checking beyond the two tags it
//! reads. A real plist parser would be a dependency and a much larger surface for one file whose
//! shape has not changed across twenty titles and two years of build ids. Everything it cannot
//! answer it answers `None` to, and every caller has a fallback.

use std::path::{Path, PathBuf};

/// The iPod 5G / 5.5G, and the only platform this emulator targets.
///
/// **Measured, not assigned** — `research/01`'s inventory of the `Platforms` array across all 56
/// archives. The seven titles a third party independently labels *5G and 5.5G only* are exactly
/// the seven whose only entry is this number.
pub const PLATFORM_5G: u32 = 1;

/// One build of a title, for one iPod generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Platform {
    pub id: u32,
    pub version: u32,
    /// Relative to the title's directory — `Executables/<name>.bin`.
    pub exe: String,
    /// The cover art's filename, in the title's directory. Empty where the entry has none.
    pub artwork: String,
}

/// What a title's manifest says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// The top-level `Name`: *Mini Golf*, *Texas Hold'em*, *Ms. PAC-MAN*.
    pub name: String,
    /// One entry per iPod generation, in the order the manifest lists them.
    pub platforms: Vec<Platform>,
}

impl Manifest {
    /// The build for this iPod, or `None` — which is *this title does not run here*.
    pub fn build_for(&self, platform: u32) -> Option<&Platform> {
        self.platforms.iter().find(|p| p.id == platform)
    }

    /// The platforms it lists, for a sentence naming them.
    pub fn platform_ids(&self) -> Vec<u32> {
        let mut out: Vec<u32> = self.platforms.iter().map(|p| p.id).collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// Read `<dir>/Manifest.plist`.
///
/// `None` where the file is absent or unreadable, which is an ordinary state: a title directory a
/// person assembled by hand has an `Executables/` and no manifest, and [`crate::titles_under`]
/// already falls back to the directory's own name for exactly that case.
pub fn manifest(dir: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(dir.join("Manifest.plist")).ok()?;
    let name = crate::manifest_name(dir)?;
    Some(Manifest {
        name,
        platforms: platforms(&text),
    })
}

/// The `Platforms` array, as one [`Platform`] per `<dict>` inside it.
///
/// **Bounded to the array**, which is the whole care this needs: `ExecutablePath` appears at the
/// top level as well, and a scan of the whole document would read the first one and hand every
/// platform the same executable.
fn platforms(text: &str) -> Vec<Platform> {
    let Some(at) = text.find("<key>Platforms</key>") else {
        return Vec::new();
    };
    let rest = &text[at..];
    let Some(open) = rest.find("<array>") else {
        return Vec::new();
    };
    let Some(close) = rest[open..].find("</array>") else {
        return Vec::new();
    };
    let array = &rest[open + "<array>".len()..open + close];

    let mut out = Vec::new();
    for chunk in array.split("<dict>").skip(1) {
        // Each `<dict>` ends at its own `</dict>`; anything after it belongs to the next one.
        let body = chunk.split("</dict>").next().unwrap_or(chunk);
        let Some(id) = integer(body, "PlatformID") else {
            continue;
        };
        out.push(Platform {
            id,
            version: integer(body, "PlatformVersion").unwrap_or(0),
            exe: string(body, "ExecutablePath").unwrap_or_default(),
            artwork: string(body, "LaunchingArtwork").unwrap_or_default(),
        });
    }
    out
}

/// `<key>K</key><integer>N</integer>`, within one dict body.
fn integer(body: &str, key: &str) -> Option<u32> {
    value(body, key, "<integer>", "</integer>")?.trim().parse().ok()
}

/// `<key>K</key><string>S</string>`, within one dict body.
fn string(body: &str, key: &str) -> Option<String> {
    let raw = value(body, key, "<string>", "</string>")?;
    Some(unescape(raw.trim()))
}

fn value<'a>(body: &'a str, key: &str, open: &str, close: &str) -> Option<&'a str> {
    let k = format!("<key>{key}</key>");
    let at = body.find(&k)?;
    let rest = &body[at + k.len()..];
    let a = rest.find(open)?;
    // **The tag has to be the next one**, or a key whose value is missing would take the following
    // key's. Apple's generator writes them adjacent, so anything between the two is a key boundary.
    if rest[..a].contains("<key>") {
        return None;
    }
    let b = rest[a + open.len()..].find(close)?;
    Some(&rest[a + open.len()..a + open.len() + b])
}

/// The five XML entities, of which these manifests use one — `&apos;`, in *Texas Hold'em*.
fn unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

// ── The cover art ───────────────────────────────────────────────────────────────────────────────

/// The header every `.raw.lcd5` opens with, measured rather than documented.
///
/// ```text
/// 00: 40 01 00 00   width   = 320
/// 04: d8 00 00 00   height  = 216
/// 08: 80 02 00 00   stride  = 640  = width × 2
/// 0c: 35 36 35 4c   "565L"  = RGB565, little-endian
/// 10: …            width × height × 2 bytes of pixels
/// ```
///
/// `320 x 216 x 2 + 16` is **138 256**, which is the size of all twenty of the artwork files on
/// hand, to the byte. That is the whole of the format, and it is why this needs no image decoder:
/// [`crate::expand16`] already turns an RGB565 word into RGBA, because the titles' own `.pix`
/// textures are the same two encodings.
pub const LCD5_HEADER: usize = 16;

/// A decoded cover, ready to hand to a window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artwork {
    pub w: u32,
    pub h: u32,
    /// `w × h × 4` bytes, RGBA8.
    pub rgba: Vec<u8>,
}

/// Decode `<dir>/<LaunchingArtwork>`.
///
/// **Refuses rather than guessing**, and each refusal names the number that was wrong. A cover that
/// decoded to noise would be worse than none: §13.2's rule for this slot is *never a blank
/// rectangle, never a stock icon*, and a wrong one is both at once.
pub fn artwork(path: &Path) -> Result<Artwork, String> {
    let raw = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if raw.len() < LCD5_HEADER {
        return Err(format!(
            "{}: {} bytes, which is shorter than the {LCD5_HEADER}-byte header",
            path.display(),
            raw.len()
        ));
    }
    let u32_at = |i: usize| {
        u32::from_le_bytes([raw[i], raw[i + 1], raw[i + 2], raw[i + 3]])
    };
    let (w, h, stride) = (u32_at(0), u32_at(4), u32_at(8));
    let tag = &raw[12..16];
    // `565L` is the only tag on any of the twenty. A second one is a format nobody here has seen,
    // and drawing it as RGB565 would be inventing a fact about somebody else's file.
    if tag != b"565L" {
        return Err(format!(
            "{}: the format tag is {:?} and this reader knows only 565L",
            path.display(),
            String::from_utf8_lossy(tag)
        ));
    }
    if stride != w * 2 {
        return Err(format!(
            "{}: {w} pixels a row at 2 bytes each is {} and the header says the stride is {stride}",
            path.display(),
            w * 2
        ));
    }
    let want = LCD5_HEADER + (stride as usize) * (h as usize);
    if raw.len() < want {
        return Err(format!(
            "{}: {w} by {h} needs {want} bytes and the file is {}",
            path.display(),
            raw.len()
        ));
    }
    let mut rgba = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for y in 0..h as usize {
        let row = LCD5_HEADER + y * stride as usize;
        for x in 0..w as usize {
            let i = row + x * 2;
            let v = u16::from_le_bytes([raw[i], raw[i + 1]]);
            rgba.extend_from_slice(&crate::expand16(v, true));
        }
    }
    Ok(Artwork { w, h, rgba })
}

/// Where this title's cover lives, for the build that runs here — or `None`.
///
/// **The artwork is per-platform**, which is why this takes one: a title built for four generations
/// carries four covers, sized for four screens, and the 5G's is the one this window draws.
pub fn artwork_path(dir: &Path, p: &Platform) -> Option<PathBuf> {
    if p.artwork.is_empty() {
        return None;
    }
    let at = dir.join(&p.artwork);
    at.is_file().then_some(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A manifest of the shape Apple ships, written here rather than copied out of `resources/`.
    ///
    /// **Nothing real is quoted**, per AGENTS.md §2: `resources/` holds shipping game binaries and
    /// real people's identifiers, and a fixture carrying a genuine GUID or build id would put one
    /// in git for ever. The *shape* is what is under test and the shape is public.
    fn a_manifest(platforms: &str) -> String {
        format!(
            "<?xml version=\"1.0\"?>\n<plist version=\"1.0\"><dict>\n\
             <key>Name</key><string>Widget &amp; Sprocket</string>\n\
             <key>ExecutablePath</key><string>Executables/top_level_decoy.bin</string>\n\
             <key>Platforms</key><array>{platforms}</array>\n\
             </dict></plist>\n"
        )
    }

    fn a_platform(id: u32, version: u32, exe: &str, art: &str) -> String {
        format!(
            "<dict><key>BuildID</key><integer>1234</integer>\
             <key>ExecutablePath</key><string>{exe}</string>\
             <key>LaunchingArtwork</key><string>{art}</string>\
             <key>PlatformID</key><integer>{id}</integer>\
             <key>PlatformVersion</key><integer>{version}</integer></dict>"
        )
    }

    fn write(dir: &Path, manifest: &str) {
        std::fs::create_dir_all(dir.join("Executables")).unwrap();
        std::fs::write(dir.join("Manifest.plist"), manifest).unwrap();
    }

    fn scratch(tag: &str) -> PathBuf {
        let at = std::env::temp_dir().join(format!("ipod-title-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&at);
        std::fs::create_dir_all(&at).unwrap();
        at
    }

    /// **The name, every platform, and the executable that belongs to each.**
    ///
    /// The decoy is the point: `ExecutablePath` is a top-level key as well as a per-platform one,
    /// and a scan of the whole document reads the top-level one and hands every platform the same
    /// binary. Red by dropping the `Platforms`-array bound in [`platforms`] — measured, every
    /// entry comes back with `top_level_decoy.bin`.
    #[test]
    fn a_manifest_answers_its_name_and_one_build_per_platform() {
        let dir = scratch("read");
        write(
            &dir,
            &a_manifest(&format!(
                "{}{}",
                a_platform(1, 1, "Executables/five_g.bin", "cover.raw.lcd5"),
                a_platform(3, 2, "Executables/nano.bin", "nano.raw.lcd5")
            )),
        );

        let m = manifest(&dir).expect("a manifest with a Name and a Platforms array");
        assert_eq!(m.name, "Widget & Sprocket", "the entity is not decoded");
        assert_eq!(m.platform_ids(), vec![1, 3]);

        let five = m.build_for(PLATFORM_5G).expect("a 5G build");
        assert_eq!(five.exe, "Executables/five_g.bin", "the top-level decoy won");
        assert_eq!(five.artwork, "cover.raw.lcd5");
        assert_eq!(five.version, 1);
        assert_eq!(m.build_for(3).unwrap().exe, "Executables/nano.bin");

        // …and a platform it does not list is `None` rather than a default. That answer is what the
        // window turns into *this title does not run on this iPod*, before anything is loaded.
        assert!(m.build_for(9).is_none(), "a platform it does not ship reads as available");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A title with no 5G build is refusable before it is loaded**, which is the whole point of
    /// reading `PlatformID` at all.
    ///
    /// Every one of the twenty titles on hand ships a `PlatformID 1` build — `research/01`'s
    /// inventory — so nothing in the corpus exercises this arm. A fixture is the only way to reach
    /// it, and reaching it is what stops the refusal being a sentence nobody has run.
    #[test]
    fn a_title_with_no_five_g_build_says_so_from_its_manifest() {
        let dir = scratch("nano-only");
        write(&dir, &a_manifest(&a_platform(3, 1, "Executables/nano.bin", "")));
        let m = manifest(&dir).expect("a manifest");
        assert!(
            m.build_for(PLATFORM_5G).is_none(),
            "a title built only for platform 3 claims a 5G build"
        );
        assert_eq!(m.platform_ids(), vec![3], "the sentence has nothing to name");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A manifest with no `Platforms` array is a title with no builds, not a panic.**
    ///
    /// `titles_under` accepts a directory holding `Executables/<name>.bin` as a title whether or
    /// not it has a manifest at all, so a hand-assembled one reaching here is ordinary.
    #[test]
    fn a_manifest_without_platforms_is_empty_rather_than_absent() {
        let dir = scratch("bare");
        write(
            &dir,
            "<plist><dict><key>Name</key><string>Bare</string></dict></plist>",
        );
        let m = manifest(&dir).expect("a Name is enough to be a manifest");
        assert_eq!(m.name, "Bare");
        assert!(m.platforms.is_empty());
        assert!(m.build_for(PLATFORM_5G).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **The cover decodes from its own header**, and every refusal names the number that was wrong.
    ///
    /// The bytes are built here rather than read from `resources/`: the format is `320 × 216`,
    /// stride `640`, tag `565L`, which is 138 256 bytes for all twenty of the real ones — measured
    /// — and a fixture proves the reader without putting a shipping asset in git.
    #[test]
    fn a_cover_decodes_to_rgba_and_a_wrong_header_is_refused() {
        let dir = scratch("art");
        let (w, h) = (4u32, 2u32);
        let mut good = Vec::new();
        good.extend_from_slice(&w.to_le_bytes());
        good.extend_from_slice(&h.to_le_bytes());
        good.extend_from_slice(&(w * 2).to_le_bytes());
        good.extend_from_slice(b"565L");
        // Pure red in RGB565 is 0xF800; pure blue is 0x001F. Two rows of four.
        for _ in 0..(w * h) {
            good.extend_from_slice(&0xF800u16.to_le_bytes());
        }
        let at = dir.join("cover.raw.lcd5");
        std::fs::write(&at, &good).unwrap();

        let art = artwork(&at).expect("a 565L cover decodes");
        assert_eq!((art.w, art.h), (w, h));
        assert_eq!(art.rgba.len() as u32, w * h * 4);
        // Red, opaque — the same `expand16` the titles' own textures go through.
        assert_eq!(&art.rgba[0..4], &[255, 0, 0, 255], "0xF800 is not red");

        // …and the three refusals, each naming its own number rather than answering with noise.
        let mut wrong_tag = good.clone();
        wrong_tag[12..16].copy_from_slice(b"JPEG");
        let at2 = dir.join("wrong-tag.raw.lcd5");
        std::fs::write(&at2, &wrong_tag).unwrap();
        let said = artwork(&at2).expect_err("an unknown tag is refused");
        assert!(said.contains("565L"), "{said}");

        let mut short = good.clone();
        short.truncate(LCD5_HEADER + 4);
        let at3 = dir.join("short.raw.lcd5");
        std::fs::write(&at3, &short).unwrap();
        let said = artwork(&at3).expect_err("a truncated cover is refused");
        assert!(said.contains("needs"), "{said}");

        let mut bad_stride = good.clone();
        bad_stride[8..12].copy_from_slice(&99u32.to_le_bytes());
        let at4 = dir.join("stride.raw.lcd5");
        std::fs::write(&at4, &bad_stride).unwrap();
        let said = artwork(&at4).expect_err("a stride that is not width×2 is refused");
        assert!(said.contains("stride"), "{said}");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// **The twenty real titles, where they are on this machine.**
    ///
    /// Skipped rather than failed where `resources/` is not there: it is gitignored, so this is the
    /// ordinary state on any machine but the operator's, and a test that failed for its absence
    /// would fail for every contributor. **Nothing it reads is printed** — the assertion is a count
    /// and a shape, and AGENTS.md §2 is why: those directories hold shipping binaries and the names
    /// stay in `resources/`.
    ///
    /// It is the control for every fixture above. A reader that passed those and could not read a
    /// single real manifest would be a reader for a format nobody ships.
    #[test]
    fn every_real_title_on_this_machine_answers_a_name_and_a_five_g_build() {
        let shelf = std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../resources/games/plaintext/Cracked Games"
        ));
        if !shelf.is_dir() {
            println!("SKIPPED: {} is not here (gitignored)", shelf.display());
            return;
        }
        let titles = crate::titles_under(&shelf);
        assert!(titles.len() >= 20, "the shelf holds {} titles", titles.len());

        let mut with_art = 0;
        for (_, dir) in &titles {
            let m = manifest(dir).expect("every shipped title carries a manifest");
            assert!(!m.name.is_empty(), "a title with an empty name");
            // `research/01`: every one of the fifty-six archives ships a PlatformID 1 build.
            let five = m
                .build_for(PLATFORM_5G)
                .expect("research/01 says every title ships a 5G build");
            assert!(five.exe.ends_with(".bin"), "the 5G build names no executable");
            if let Some(at) = artwork_path(dir, five) {
                let art = artwork(&at).expect("a shipped cover decodes");
                // 320 × 216 across all twenty, measured 2026-09-07.
                assert_eq!((art.w, art.h), (320, 216));
                with_art += 1;
            }
        }
        assert!(
            with_art >= 20,
            "only {with_art} of {} titles decoded a cover; the format is 320×216 565L on every one \
             measured",
            titles.len()
        );
    }
}
