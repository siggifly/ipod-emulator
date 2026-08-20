//! Rockbox, fetched from inside the program — the same shape as [`crate::firmware`].
//!
//! Somebody who wants a second operating system on their iPod should not have to go and find one
//! first. Apple's firmware is already fetched this way, verified by SHA-256, cached, deduplicated
//! and clearable; there is no reason Rockbox should be different, and every reason it should not
//! be — it is the one alternative here that reaches its own main menu.
//!
//! ## Two files, and they are not interchangeable
//!
//! Installing Rockbox is two separate acts, and confusing them is the classic way to end up with a
//! device that boots to `Can't load rockbox.ipod: File not found`:
//!
//! | file | goes | with |
//! |---|---|---|
//! | `bootloader-ipodvideo.ipod` | the **firmware partition**, appended after Apple's `osos` | [`crate::install::install_os`] |
//! | `rockbox-ipodvideo-N.zip` | the **data volume**, as `.rockbox/` | [`crate::install::put_zip`] |
//!
//! The bootloader is what Apple's boot ROM runs; the zip is what the bootloader then looks for.
//! Neither is any use without the other, so [`FULL_INSTALL`] names both.
//!
//! ## Where the hashes came from
//!
//! Both entries were verified before being written down, and not by downloading them here and
//! trusting the result: the bytes already in `resources/vendor/rockbox/bin/` were hashed locally,
//! and `download.rockbox.org` was asked for the same two files' `Content-Length`. They match
//! exactly — 9 090 335 and 51 996 — so the files this catalogue points at are the files this
//! project has actually booted.

/// One downloadable piece of Rockbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    /// What it is called on disk, and in the cache.
    pub file: &'static str,
    pub url: &'static str,
    /// Size in bytes, from the server's own `Content-Length`.
    pub bytes: u64,
    /// Lower-case hex SHA-256 of the bytes.
    pub sha256: &'static str,
    /// Where it belongs once it is here. The whole reason both are in one catalogue.
    pub goes: Where,
    /// One line for a person choosing.
    pub about: &'static str,
}

/// Which half of an install a piece is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Where {
    /// The firmware partition, appended after Apple's `osos` — what the boot ROM runs.
    FirmwarePartition,
    /// The data volume, unpacked as `.rockbox/` — what the bootloader then loads.
    Volume,
}

/// The pieces of a Rockbox install for the iPod Video.
///
/// **Deliberately not "every Rockbox build".** Rockbox publishes a daily and a current build for
/// every target it supports, and a catalogue of those would be a list this project cannot verify
/// and would go stale the day after it was written. This is the stable release and the bootloader
/// that goes with it, both hashed.
pub const CATALOGUE: &[Piece] = &[
    Piece {
        file: "bootloader-ipodvideo.ipod",
        url: "https://download.rockbox.org/bootloader/ipod/bootloader-ipodvideo.ipod",
        bytes: 51_996,
        sha256: "19dfa0e930689f5afdeaae18f4c56b472cbed9c6c3a7039bb32b646d8040298f",
        goes: Where::FirmwarePartition,
        about:
            "Rockbox's bootloader — what Apple's boot ROM runs. Chains back to Apple's own \
                software when you hold MENU, so installing it does not take the iPod away from you.",
    },
    Piece {
        file: "rockbox-ipodvideo-4.0.zip",
        url: "https://download.rockbox.org/release/4.0/rockbox-ipodvideo-4.0.zip",
        bytes: 9_090_335,
        sha256: "010334b02a89f43cd64f069807cb52228c0cfd55a8ee084d6a05aab829f42732",
        goes: Where::Volume,
        about: "Rockbox 4.0 itself — 381 files of `.rockbox/`, including its plugins and games.",
    },
];

/// The two pieces a working install needs, in the order they should be applied.
///
/// The bootloader first, because [`crate::install::install_os`] writes a **new** drive and the zip
/// then goes onto that one. The other order would unpack 19 MB onto a drive that is about to be
/// superseded.
pub const FULL_INSTALL: [&Piece; 2] = [&CATALOGUE[0], &CATALOGUE[1]];

/// Where downloads are kept — beside Apple's, because they are the same kind of thing and a person
/// clearing one expects to have cleared both.
pub fn cache_dir() -> std::path::PathBuf {
    crate::firmware::cache_dir()
}

/// Check bytes against what the catalogue says they should be.
pub fn verify(p: &Piece, data: &[u8]) -> Result<(), String> {
    if data.len() as u64 != p.bytes {
        return Err(format!(
            "{}: expected {} bytes, got {} — a truncated download, or not the file we meant",
            p.file,
            p.bytes,
            data.len()
        ));
    }
    let got = crate::firmware::sha256(data);
    if got != p.sha256 {
        return Err(format!(
            "{}: sha256 is {got}, expected {}",
            p.file, p.sha256
        ));
    }
    Ok(())
}

/// Fetch a piece into `dir`, or return the copy already there if it verifies.
pub fn download(p: &Piece, dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let dest = dir.join(p.file);
    if let Ok(existing) = std::fs::read(&dest) {
        if verify(p, &existing).is_ok() {
            return Ok(dest);
        }
        // Present but wrong: say so rather than silently re-using or silently clobbering.
        eprintln!(
            "{}: already here but does not verify — downloading again",
            dest.display()
        );
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let part = dir.join(format!("{}.part", p.file));
    crate::firmware::http_get_to_file(p.url, &part)?;
    let got = std::fs::read(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    if let Err(e) = verify(p, &got) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Every entry is fully verifiable**, which is the difference between this catalogue and a
    /// list of links. A size alone would accept a truncated file that happened to be the right
    /// length, and no hash at all would accept anything the network handed back.
    #[test]
    fn every_piece_can_be_checked_before_it_is_installed() {
        for p in CATALOGUE {
            assert_eq!(p.sha256.len(), 64, "{}: not a sha256", p.file);
            assert!(
                p.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{}",
                p.file
            );
            assert!(p.bytes > 0, "{}: no size", p.file);
            assert!(p.url.starts_with("https://"), "{}: not https", p.file);
            assert!(
                p.url.ends_with(p.file),
                "{}: url and filename disagree",
                p.file
            );
            assert!(!p.about.is_empty());
        }
    }

    /// The two halves are different acts and go to different places. A catalogue that lost track of
    /// which was which would install a 9 MB zip into the firmware partition.
    #[test]
    fn a_full_install_is_the_bootloader_then_the_volume() {
        assert_eq!(FULL_INSTALL.len(), 2);
        assert_eq!(FULL_INSTALL[0].goes, Where::FirmwarePartition);
        assert_eq!(FULL_INSTALL[1].goes, Where::Volume);
        // And the order matters: the bootloader install makes the new drive the zip lands on.
        assert!(FULL_INSTALL[0].file.ends_with(".ipod"));
        assert!(FULL_INSTALL[1].file.ends_with(".zip"));
    }

    /// The hashes are of the files this project has actually booted, not of whatever the network
    /// last returned — so if the vendored copies are present, they must match.
    #[test]
    fn the_catalogue_matches_the_files_in_this_repository() {
        let root = crate::settings::repo_root().join("resources/vendor/rockbox/bin");
        for p in CATALOGUE {
            let path = root.join(p.file);
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            }; // resources/ is not shipped.
            assert!(
                verify(p, &bytes).is_ok(),
                "{} in the repository does not match the catalogue: {:?}",
                path.display(),
                verify(p, &bytes)
            );
        }
    }
}
