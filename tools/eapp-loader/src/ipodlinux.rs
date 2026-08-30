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
//!
//! [`resolve_loader`] is the answer, and nothing reaches for `resources/vendor/ipodloader2` any
//! more. `IPOD_LOADER=/path/to/loader.bin` is the override for anybody who wants their own build —
//! including 2.9.0d — and the install report says which of the two ran.

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

/// Where a `loader.bin` came from, so the report can say which `ipodloader2` ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoaderFrom {
    /// The verified v2.8.1 release — [`LOADER`] — out of the download cache, fetched if it was not
    /// already there.
    Release,
    /// Whatever `IPOD_LOADER=` named. **Not hashed**: this project holds no hash for a build
    /// somebody made.
    Provided,
}

/// The `loader.bin` to install, and where it came from.
///
/// Two sources, and the order is the whole of the decision:
///
/// 1. **`IPOD_LOADER=` wins and is never second-guessed.** If it names a file, that file is used;
///    if it names something that is not a file, that is an error and *not* a quiet fall-through to
///    the download. An override that silently does something else is worse than no override — it
///    is an instrument that lies.
/// 2. Otherwise the v2.8.1 release, fetched and verified, or already in the cache.
///
/// **`resources/vendor/ipodloader2/loader.bin` is not consulted at all**, not even as a fallback.
/// That directory is gitignored, so reaching for it made iPodLinux installable only by somebody
/// working inside this checkout — and the failure arrived after a 101 MB download.
/// `rockbox-install` already sets the precedent of fetching rather than looking at
/// `resources/vendor/rockbox/bin/`. And the vendored build is `iPL 2.9.0d` from `master` while the
/// release is v2.8.1, so preferring it silently would give two people running one command two
/// different bootloaders with only one of them able to find out which. Whoever wants 2.9.0d has
/// `IPOD_LOADER=`, which says so in the report.
///
/// The asymmetry with the ZeroSlackr tree is deliberate: that tree is one hashed archive and is
/// content-identical however you get it, so preferring a local unpack is a bandwidth decision. The
/// loader is two different versions, so it is a correctness decision.
pub fn resolve_loader(dir: &Path) -> Result<(PathBuf, LoaderFrom), String> {
    if let Some(v) = std::env::var_os("IPOD_LOADER") {
        let p = PathBuf::from(&v);
        if !p.as_os_str().is_empty() {
            // **The diagnosis is the observation, not a guess.** `Path::is_file()` is
            // `metadata().map(…).unwrap_or(false)`, which folds a permission problem, an
            // unmounted volume and a directory all into "no such file" — the program asserting a
            // fact about somebody's filesystem it never observed, which is the same defect
            // `settings::Presence::exists` spells out three arms to avoid. Here it also ends the
            // command, so a wrong diagnosis is the last thing the user is told.
            let unset = format!(
                "Unset it to fetch {} ({} bytes, sha256 on record) instead.",
                LOADER.file, LOADER.bytes
            );
            return match std::fs::metadata(&p) {
                Ok(m) if m.is_file() => Ok((p, LoaderFrom::Provided)),
                Ok(_) => Err(format!(
                    "IPOD_LOADER={} — that is not a file. {unset}",
                    p.display()
                )),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!(
                    "IPOD_LOADER={} — no such file. {unset}",
                    p.display()
                )),
                Err(e) => Err(format!(
                    "IPOD_LOADER={} — cannot read it: {e}. {unset}",
                    p.display()
                )),
            };
        }
    }
    match download(&LOADER, dir) {
        Ok(p) => Ok((p, LoaderFrom::Release)),
        Err(e) => Err(no_loader(&e, dir)),
    }
}

/// What to say when the loader can be neither fetched nor found.
///
/// Separate from [`resolve_loader`] so it can be read without a network. It names the file, its
/// size, its hash, where to get it and where to put it — and it deliberately says nothing about
/// `make` or `resources/vendor`, because that project state is being deleted rather than
/// relocated.
fn no_loader(e: &str, dir: &Path) -> String {
    format!(
        "{e}\n\
         `ipodloader2` could not be fetched and there is no other copy this knows about.\n\
         {file} is {bytes} bytes, sha256 {sha}, at\n\
         \x20 {url}\n\
         Fetch it by hand and put it at {dest}, or point at one you already\n\
         have with IPOD_LOADER=/path/to/loader.bin.",
        file = LOADER.file,
        bytes = LOADER.bytes,
        sha = LOADER.sha256,
        url = LOADER.url,
        dest = dir.join(LOADER.file).display(),
    )
}

/// Where downloads are kept — beside Apple's and Rockbox's, because they are the same kind of
/// thing and somebody clearing one expects to have cleared all of them.
pub fn cache_dir() -> PathBuf {
    crate::firmware::cache_dir()
}

/// Check bytes against what the catalogue says they should be.
///
/// [`crate::firmware::checked`]'s wording, shared with [`crate::rockbox::verify`] — see that one.
pub fn verify(p: &Piece, data: &[u8]) -> Result<(), String> {
    crate::firmware::checked(p.file, p.bytes, p.sha256, data)
}

/// The `.part` a download of this piece writes to.
pub fn part_path(p: &Piece, dir: &Path) -> PathBuf {
    crate::firmware::part_named(p.file, dir)
}

/// Fetch a piece into `dir`, or return the copy already there if it verifies.
pub fn download(p: &Piece, dir: &Path) -> Result<PathBuf, String> {
    download_watched(p, dir, &mut crate::firmware::Silent).map_err(|(_, said)| said)
}

/// The same, reporting bytes as they land and stopping when asked.
///
/// **[`LOADER`] is 56 912 bytes and [`CATALOGUE`]'s ZeroSlackr is 101 MB**, which is the whole
/// reason this one needs a watcher and `resolve_loader`'s blocking call did not: a fetch a person
/// can watch is a fetch a person can stop.
pub fn download_watched(
    p: &Piece,
    dir: &Path,
    w: &mut dyn crate::firmware::Watch,
) -> Result<PathBuf, (crate::firmware::Trouble, String)> {
    crate::firmware::get_watched(p.file, p.url, p.bytes, p.sha256, dir, w)
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

    /// **Both `IPOD_LOADER` arms, in one function**, because they mutate process-global state and
    /// cargo runs tests on several threads — split in two they could interleave with each other.
    ///
    /// The `dir` passed is one that does not exist, so if the override branch is ever deleted this
    /// stops being a fast assertion failure and becomes a real network fetch. Watch the clock as
    /// well as the result.
    #[test]
    fn the_loader_override_wins_and_an_override_that_points_at_nothing_is_an_error() {
        let tmp = std::env::temp_dir().join(format!(
            "ipod-loader-override-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let mine = tmp.join("my-loader.bin");
        std::fs::write(&mine, b"not really a loader, and never hashed").unwrap();
        let absent = tmp.join("nothing-here.bin");
        let no_cache = tmp.join("no-such-cache-dir");

        // SAFETY: single-threaded within this test, and the value is restored before it returns.
        let before = std::env::var_os("IPOD_LOADER");

        unsafe { std::env::set_var("IPOD_LOADER", &mine) };
        assert_eq!(
            resolve_loader(&no_cache),
            Ok((mine.clone(), LoaderFrom::Provided)),
            "the override did not win, so something reached the network or the cache"
        );

        unsafe { std::env::set_var("IPOD_LOADER", &absent) };
        let e = resolve_loader(&no_cache).unwrap_err();
        assert!(
            e.contains("IPOD_LOADER") && e.contains(&absent.display().to_string()),
            "an override pointing at nothing did not name itself: {e}"
        );
        assert!(
            e.contains("no such file"),
            "a path that really is not there should say so: {e}"
        );

        // **And a path that is there but is not a file gets its own diagnosis.** `is_file()` folds
        // every `stat` outcome into "no such file", so a directory — or a volume that is not
        // mounted, or a parent nobody can traverse — was reported as an absence nobody observed,
        // and it ends the command.
        unsafe { std::env::set_var("IPOD_LOADER", &tmp) };
        let e = resolve_loader(&no_cache).unwrap_err();
        assert!(
            e.contains("not a file"),
            "a directory was diagnosed as a missing file: {e}"
        );

        match before {
            Some(v) => unsafe { std::env::set_var("IPOD_LOADER", v) },
            None => unsafe { std::env::remove_var("IPOD_LOADER") },
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A failure somebody cannot act on is a failure that costs a session. This one names the file,
    /// its size, its hash, where it is, where to put it, and the override — and it names **neither**
    /// `make` nor `resources/vendor`, because that project state no longer exists.
    #[test]
    fn the_failure_names_the_file_the_hash_and_the_way_to_supply_it_by_hand() {
        let dir = Path::new("/tmp/ipod-emulator-cache");
        let msg = no_loader("curl could not fetch it: connection refused", dir);
        for needle in [
            LOADER.url,
            LOADER.sha256,
            LOADER.file,
            "IPOD_LOADER",
            "connection refused",
        ] {
            assert!(msg.contains(needle), "the failure omits {needle}:\n{msg}");
        }
        assert!(
            msg.contains(&LOADER.bytes.to_string()),
            "the failure does not say how big it is:\n{msg}"
        );
        assert!(
            msg.contains(&dir.display().to_string()),
            "the failure does not say where to put it:\n{msg}"
        );
        assert!(
            !msg.contains("resources/vendor"),
            "the deleted project state came back:\n{msg}"
        );
        assert!(
            !msg.contains("make"),
            "the failure still tells people to build it:\n{msg}"
        );
    }

    /// **The vendored path is gone from the code and stays gone.**
    ///
    /// The needle is the bare path on any line that is not a comment, not one spelling of one call.
    /// It was `join("resources/vendor/ipodloader2`, which `PathBuf::from(…)`,
    /// `root.join("resources/vendor").join("ipodloader2")` and a `const` holding the path all walk
    /// straight past — a guard that only catches the exact line somebody already deleted. Comments
    /// are stripped first rather than the search being narrowed, so the doc comments and research
    /// prose that record *why* it was wrong are untouched: a wrong answer preserved in place is the
    /// point of those.
    ///
    /// **And the zero is proved to be a zero.** A run outside a checkout has no sources, and an
    /// empty walk reports exactly what a clean tree does — `AGENTS.md` §6's own shape. The count of
    /// files scanned is asserted, so "nothing found" cannot come from "nothing looked at".
    ///
    /// **What it still cannot see**, said rather than left to be discovered: a path assembled in
    /// pieces — `join("resources/vendor").join("ipodloader2")`. Widening to `resources/vendor`
    /// alone would fire on `resources/vendor/zeroslackr`, which is a live and deliberate local
    /// path, and on `ipodloader2` alone it would fire on the release's own filename. A text search
    /// is the wrong instrument for that case; the thing that would actually catch it is
    /// `resolve_loader` staying the only route to a loader.
    #[test]
    fn nothing_reaches_for_the_vendored_loader_any_more() {
        fn walk(dir: &Path, out: &mut Vec<String>, scanned: &mut usize, needle: &str) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out, scanned, needle);
                } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                    let Ok(text) = std::fs::read_to_string(&p) else {
                        continue;
                    };
                    *scanned += 1;
                    for (i, line) in text.lines().enumerate() {
                        // Everything from a `//` on is prose. `///` and `//!` start with it too.
                        let code = line.split("//").next().unwrap_or("");
                        if code.contains(needle) {
                            out.push(format!("{}:{}", p.display(), i + 1));
                        }
                    }
                }
            }
        }

        let tools = crate::settings::repo_root().join("tools");
        if !tools.is_dir() {
            return; // a release build has no sources to check
        }
        // Assembled rather than written, so this line cannot match itself.
        let needle = format!("resources/vendor/{}", "ipodloader2");
        let mut found = Vec::new();
        let mut scanned = 0usize;
        walk(&tools, &mut found, &mut scanned, &needle);
        assert!(
            scanned > 10,
            "only {scanned} .rs files were read under {}; a zero from an empty walk is not a zero",
            tools.display()
        );
        assert!(
            found.is_empty(),
            "the vendored ipodloader2 is reached for again at {} — \
             `resources/` is gitignored, so that path works only inside this checkout",
            found.join(", ")
        );
    }
}
