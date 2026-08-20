//! Where iPodLinux comes from, and how it is checked.
//!
//! Rockbox and Apple's firmware are both fetched and verified from inside this program — a URL, a
//! size and a SHA-256 per entry, and nothing renamed into place until it verifies. iPodLinux was
//! the odd one out: its kernel arrived here as a file somebody downloaded once by hand, and the
//! archive it came out of was thrown away. That cost a session — with one file of 1 805 on the
//! drive, the kernel booted completely and then had nothing to execute, and the panic read exactly
//! like an emulator defect.
//!
//! So it is a catalogue like the others.
//!
//! **`ipodloader2` is in it now, and the reason it was not is a mistake worth recording.** This
//! file used to say upstream published no binary this project could hash — which was wrong.
//! `crozone/ipodloader2` publishes GitHub releases with a `loader.bin` asset, and v2.8.1 is one.
//! The claim was never checked; it was inferred from the repository having a Makefile.
//!
//! The cost of that was not academic: the window built its loader from
//! `resources/vendor/ipodloader2/loader.bin`, a path inside this checkout, so **iPodLinux could not
//! be installed by anybody who was not working in the repository** — and the failure arrived after
//! a 101 MB download.
//!
//! Two loaders were considered and rejected, which is worth saying because both are defensible:
//!
//! - **ZeroSlackr carries one**, at `patch-files/loader.bin` — inside an archive already fetched
//!   and already hashed, so it costs nothing. But it is `iPL Loader 2.5 rxported`, from 2008, and
//!   using it would be a downgrade from what this project has measured.
//! - **Our own build is `iPL 2.9.0d`**, from `master` at `a41ec49` — newer than any release, and
//!   the one every number in research/17 was measured on. It cannot be fetched, because it is a
//!   build somebody made rather than a thing upstream published.
//!
//! So v2.8.1 it is: the newest thing with a URL and a hash. **The numbers in research/17 are
//! 2.9.0d's**, and until the same run is made against 2.8.1 they describe a loader most people
//! will not be running.

use std::path::{Path, PathBuf};

/// One downloadable piece of an iPodLinux install.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub file: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    /// Lower-case hex SHA-256 of the bytes.
    pub sha256: &'static str,
    /// One line for a person choosing.
    pub about: &'static str,
}

/// **Project ZeroSlackr**, the distribution the kernel here comes out of.
///
/// Keripo's drag-and-drop iPodLinux build. It avoids repartitioning the iPod by keeping the real
/// root filesystem in an ext3 image on the FAT32 volume and loop-mounting it at boot, which is why
/// installing it is a file copy rather than a partition editor.
///
/// One entry, deliberately. SourceForge carries several snapshots and a pre-release directory; a
/// catalogue of all of them would be a list this project has not booted. This is the one it has.
pub const CATALOGUE: &[Piece] = &[Piece {
    file: "ZeroSlackr-SVN-snapshot-2008-08-11.7z",
    url: "https://sourceforge.net/projects/zeroslackr/files/latest/download",
    bytes: 101_146_859,
    sha256: "a6871b5b40dbc85e6f7a3facf02cbba327011149d7652c80beac1119389abd70",
    about: "Project ZeroSlackr — the iPodLinux distribution: kernel, busybox userland, and an \
            ext3 root image the kernel loop-mounts off the FAT32 volume.",
}];

/// The five directories `IPOD_LINUX_INSTALL.md` says to copy to the iPod's volume.
///
/// Re-exported from [`crate::install`] so a caller checking an unpacked tree does not have to know
/// which module owns the list.
pub use crate::install::ZEROSLACKR_DIRS;

/// **`ipodloader2` v2.8.1**, the newest release upstream publishes a binary for.
///
/// `master` is further along — this project builds `2.9.0d` from it — but a build is not a
/// release, and a catalogue row naming one would be a row nobody else could reproduce.
pub const LOADER: Piece = Piece {
    file: "ipodloader2-v2.8.1-loader.bin",
    url: "https://github.com/crozone/ipodloader2/releases/download/v2.8.1/loader.bin",
    bytes: 56_912,
    sha256: "28eb4b805580b959cee73566999912bf0a9f54a581eb32d0b72df56669b427e9",
    about: "ipodloader2 v2.8.1 — the bootloader iPodLinux needs. Reads `loader.cfg` from the \
            volume and offers everything installed on it.",
};

/// Fetch the loader, or return the copy already here if it verifies.
pub fn loader(dir: &Path) -> Result<PathBuf, String> {
    download(&LOADER, dir)
}

/// Where downloads are kept — beside Apple's and Rockbox's, because they are the same kind of
/// thing and somebody clearing one expects to have cleared all of them.
pub fn cache_dir() -> PathBuf {
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
pub fn download(p: &Piece, dir: &Path) -> Result<PathBuf, String> {
    let dest = dir.join(p.file);
    if let Ok(existing) = std::fs::read(&dest) {
        if verify(p, &existing).is_ok() {
            return Ok(dest);
        }
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

/// Unpack the archive into `dest/tree`, returning that directory.
///
/// **This shells out to `7z`**, and says so rather than failing obscurely when it is absent. The
/// alternative is an LZMA decoder in-tree for one 101 MB file that most people will never fetch,
/// and this project already asks for `ffmpeg` to make a GIF and `curl` to download — a tool that
/// does one thing well, named in the error when it is missing, is the established shape here.
pub fn unpack(archive: &Path, dest: &Path) -> Result<PathBuf, String> {
    let tree = dest.join("tree");
    if ZEROSLACKR_DIRS.iter().all(|d| tree.join(d).is_dir()) {
        return Ok(tree);
    }
    let seven = ["7z", "7za", "7zz"]
        .iter()
        .find(|c| {
            std::process::Command::new(c)
                .arg("--help")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok()
        })
        .ok_or_else(|| {
            format!(
                "{} is a 7-zip archive and no `7z` was found on PATH.\n\
                 Install p7zip (`brew install p7zip`, `apt install p7zip-full`) or unpack it \n\
                 yourself into {} — the five directories `bin`, `boot`, `dev`, `etc` and \n\
                 `ZeroSlackr` have to end up directly inside it.",
                archive.display(),
                tree.display()
            )
        })?;
    std::fs::create_dir_all(&tree).map_err(|e| format!("{}: {e}", tree.display()))?;
    let st = std::process::Command::new(seven)
        .arg("x")
        .arg("-y")
        .arg(format!("-o{}", tree.display()))
        .arg(archive)
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("{seven}: {e}"))?;
    if !st.success() {
        return Err(format!("{seven} failed to unpack {}", archive.display()));
    }
    let missing: Vec<&str> = ZEROSLACKR_DIRS
        .iter()
        .copied()
        .filter(|d| !tree.join(d).is_dir())
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "unpacked {} but {} is/are not in it — this is not the archive the catalogue names",
            archive.display(),
            missing.join(", ")
        ));
    }
    Ok(tree)
}

/// Fetch and unpack in one step, returning the tree `install_linux` wants.
pub fn fetch(dir: &Path) -> Result<PathBuf, String> {
    let archive = download(&CATALOGUE[0], dir)?;
    unpack(&archive, dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Every entry is fully verifiable.** A size alone would accept a truncated file that
    /// happened to be the right length, and no hash would accept whatever the network handed back.
    /// The same test guards the Rockbox catalogue, for the same reason.
    #[test]
    fn every_piece_can_be_checked_before_it_is_installed() {
        assert!(!CATALOGUE.is_empty());
        for p in CATALOGUE {
            assert!(p.bytes > 0, "{}: no size", p.file);
            assert_eq!(p.sha256.len(), 64, "{}: sha256 is not 64 hex chars", p.file);
            assert!(
                p.sha256
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "{}: sha256 must be lower-case hex",
                p.file
            );
            assert!(p.url.starts_with("https://"), "{}: not https", p.file);
            assert!(!p.about.is_empty(), "{}: no description", p.file);
        }
    }

    /// A wrong hash has to be rejected, or `verify` is decoration.
    #[test]
    fn verify_rejects_the_wrong_bytes() {
        let p = &CATALOGUE[0];
        assert!(
            verify(p, b"not the archive").is_err(),
            "a short file must be refused"
        );
        let right_length = vec![0u8; p.bytes as usize];
        assert!(
            verify(p, &right_length).is_err(),
            "the right length with the wrong contents must still be refused"
        );
    }
}
