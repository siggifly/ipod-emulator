//! What a volume will do with an 8 GiB file, and how much room is left on it.
//!
//! **Both are measured rather than named.** The alternative is reading a filesystem type and
//! deciding from a table what it can do — which is how a program comes to say *"that folder is on a
//! FAT32 volume"* about a volume it never looked at. Here the sparse question is answered by
//! creating a file, sizing it, measuring it and removing it, and the free-space question by asking
//! the OS the one question it will answer.
//!
//! **Nothing here refuses anything.** [`space`] returning `None` means *not measured*, never *none
//! free*: a permission, a missing tool or an unparseable line is not an observation about somebody's
//! disk, and the caller's gate treats `None` as "do not stand in the way". `crate::settings::Presence`
//! states the same rule for `stat(2)` and for the same reason.

use std::path::{Path, PathBuf};

/// What a target volume can do, measured by [`probe`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Probe {
    /// The file cost far less than its length: this volume has holes.
    Sparse,
    /// The file cost what it says it is. An 8 GiB drive image here costs 8 GiB.
    Full,
    /// The volume would not take a file that size at all — FAT32's 4 GiB ceiling is the one that
    /// happens. `at` is the size refused; `why` is the OS's own sentence.
    TooBig { at: u64, why: String },
    /// The program is not allowed to write there, or the directory could not be made.
    Refused { why: String },
}

impl Probe {
    /// `None` for the two refusals — there is no plan to cost.
    pub fn holes(&self) -> Option<crate::compose::Holes> {
        match self {
            Probe::Sparse => Some(crate::compose::Holes::Sparse),
            Probe::Full => Some(crate::compose::Holes::Full),
            Probe::TooBig { .. } | Probe::Refused { .. } => None,
        }
    }
}

/// Free bytes on a volume, and what it is called.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Space {
    pub free: u64,
    /// The mount point, for a sentence that says *where* — `39 GB free on /`.
    pub mount: String,
}

/// The prefix every probe file wears, so a stray one is identifiable as ours.
pub const PROBE_PREFIX: &str = ".ipod-probe-";

/// A file whose materialised size is under `apparent / SPARSE_RATIO` is sparse.
///
/// A threshold rather than an equality, so a filesystem that preallocates a little — or a metadata
/// scheme that charges for the extent map — does not tip the wrong way. At 8 GiB the line is at
/// 512 MiB, and the measured real answer on APFS is 20 987 904 bytes.
pub const SPARSE_RATIO: u64 = 16;

/// Ask the volume `dir` is on what it will do with a file of `apparent` bytes — **by doing it**.
///
/// One file created, sized, measured and removed. **It may block, and it writes**, so it runs at
/// the press and never while a plan is merely being drawn: nothing is written before a person has
/// agreed to the plan.
///
/// The probe file is removed on **every** path, including both error paths; its own removal failure
/// is ignored, because a program that failed to write a file and then reported failing to delete it
/// would be naming the wrong problem.
///
/// On Windows this always answers [`Probe::Full`], and honestly: `on_disk_size` there is `len()`,
/// and nothing in this program issues `FSCTL_SET_SPARSE`.
pub fn probe(dir: &Path, apparent: u64) -> Probe {
    if let Err(e) = std::fs::create_dir_all(dir) {
        return Probe::Refused {
            why: format!("{}: {e}", dir.display()),
        };
    }
    let at = dir.join(format!("{PROBE_PREFIX}{}", std::process::id()));
    let f = match std::fs::File::create(&at) {
        Ok(f) => f,
        // **The directory, never the probe file.** This said `…/drives/.ipod-probe-95411:
        // Permission denied` — naming a file that exists for a few milliseconds and is gone before
        // anybody reads the sentence, about a directory that is the actual subject. A refusal has
        // to name something a person can go and look at.
        Err(e) => {
            return Probe::Refused {
                why: format!("{}: {e}", dir.display()),
            }
        }
    };
    if let Err(e) = f.set_len(apparent) {
        drop(f);
        let _ = std::fs::remove_file(&at);
        return Probe::TooBig {
            at: apparent,
            why: format!("{e}"),
        };
    }
    let answer = match f.metadata() {
        Ok(m) => {
            if crate::settings::on_disk_size(&m) < apparent / SPARSE_RATIO {
                Probe::Sparse
            } else {
                Probe::Full
            }
        }
        // A file we just created whose metadata will not read is not a statement about holes.
        Err(e) => Probe::Refused {
            why: format!("{}: {e}", at.display()),
        },
    };
    drop(f);
    let _ = std::fs::remove_file(&at);
    answer
}

/// Free bytes on the volume `dir` is on, or `None` when nothing here could say.
///
/// **`None` is "not measured", never "none free", and it never refuses anything.**
///
/// A path that does not exist yet walks up to its nearest existing ancestor, because the directory
/// this program is about to create is on the volume its parent is on.
///
/// `df` is deliberately **not** a [`crate::firmware`]-style named tool with a remedy attached:
/// `brew install df` is not a sentence, so a missing `df` produces `None`, `None` never refuses, and
/// there is no control to disable and no command to print.
pub fn space(dir: &Path) -> Option<Space> {
    let at = nearest_existing(dir)?;
    #[cfg(unix)]
    {
        unix_df(&at)
    }
    #[cfg(not(unix))]
    {
        windows_space(&at)
    }
}

/// The nearest ancestor of `p` that exists, `p` included. `None` when even the root does not.
fn nearest_existing(p: &Path) -> Option<PathBuf> {
    p.ancestors()
        .find(|a| !a.as_os_str().is_empty() && a.exists())
        .map(|a| a.to_path_buf())
}

/// `df -Pk`, parsed defensively: every field is optional and an unparseable one is `None` rather
/// than `0`. A zero invented here would be the eighth instrument in this project to report an
/// absence it could not have observed.
///
/// **No shell.** The path is one `Command::arg`, so a directory with a quote or a space in its name
/// is safe by construction rather than by escaping.
#[cfg(unix)]
fn unix_df(at: &Path) -> Option<Space> {
    use std::process::{Command, Stdio};
    let out = Command::new("df")
        .arg("-Pk")
        .arg(at)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // POSIX `-P` promises one line per filesystem after the header. Some implementations still wrap
    // a long device name, so the fields are taken from the LAST line with enough of them rather
    // than from line two positionally.
    let line = text
        .lines()
        .skip(1)
        .filter(|l| l.split_whitespace().count() >= 6)
        .last()?;
    let f: Vec<&str> = line.split_whitespace().collect();
    let free_kb: u64 = f.get(3)?.parse().ok()?;
    let mount = f[5..].join(" ");
    Some(Space {
        free: free_kb.saturating_mul(1024),
        mount: if mount.is_empty() {
            at.display().to_string()
        } else {
            mount
        },
    })
}

/// One `powershell` call returning the drive's free bytes and its name.
#[cfg(not(unix))]
fn windows_space(at: &Path) -> Option<Space> {
    use std::process::{Command, Stdio};
    let script = format!(
        "$d=(Get-Item -LiteralPath $env:IPOD_PROBE_DIR).PSDrive; \
         Write-Output $d.Free; Write-Output $d.Name"
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .env("IPOD_PROBE_DIR", at)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let free: u64 = lines.next()?.parse().ok()?;
    let mount = lines.next().unwrap_or("").to_string();
    Some(Space {
        free,
        mount: if mount.is_empty() {
            at.display().to_string()
        } else {
            mount
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "ipod-volume-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).expect("a scratch directory");
        d
    }

    /// **The probe leaves nothing behind, on every outcome it can reach here.**
    ///
    /// A probe that left an 8 GiB file in somebody's data directory every launch would be worse
    /// than not measuring at all.
    #[test]
    fn the_probe_leaves_nothing_behind() {
        let d = scratch("clean");
        let answer = probe(&d, 8 * 1024 * 1024 * 1024);
        let left: Vec<String> = std::fs::read_dir(&d)
            .expect("the directory the probe just used")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            left.is_empty(),
            "the probe answered {answer:?} and left {left:?} behind"
        );
        assert!(
            matches!(answer, Probe::Sparse | Probe::Full),
            "a writable scratch directory answered {answer:?}"
        );
        // And a directory that does not exist yet is created rather than refused — the drives
        // directory does not exist before the first build.
        let fresh = d.join("not").join("yet");
        let answer = probe(&fresh, 1024);
        assert!(matches!(answer, Probe::Sparse | Probe::Full), "{answer:?}");
        assert!(fresh.is_dir());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **The volume that will not take the file at all** — §9.3's `volume` class, executed rather
    /// than constructed.
    ///
    /// FAT32's 4 GiB ceiling is the one that happens in the wild and there is no FAT32 volume in
    /// this test run, so the size is what makes it refuse: every filesystem has a maximum file
    /// size, and `u64::MAX` is above all of them. What is being checked is that a `set_len` the
    /// filesystem declines becomes `TooBig` with the OS's own sentence and the size it refused —
    /// and that the probe file goes even on that path.
    #[test]
    fn a_size_no_filesystem_will_take_is_the_volume_class_and_leaves_nothing() {
        let d = scratch("too-big");
        let answer = probe(&d, u64::MAX);
        let left: Vec<String> = std::fs::read_dir(&d)
            .expect("the directory")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(left.is_empty(), "the refused probe left {left:?} behind");
        match &answer {
            Probe::TooBig { at, why } => {
                assert_eq!(*at, u64::MAX, "the refusal does not name the size it refused");
                assert!(!why.is_empty(), "the refusal carries no sentence from the OS");
                assert_eq!(answer.holes(), None, "a refusal reported a verdict about holes");
            }
            // A filesystem that accepts a `u64::MAX` `set_len` is answering honestly about a
            // sparse file, and there is nothing to check here. Say so rather than pass silently.
            other => println!("SKIPPED: this filesystem took a u64::MAX file: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A refusal is a refusal and not a verdict about holes: `holes()` is `None` for both, so a
    /// caller cannot cost a plan against a volume it could not write to.
    #[test]
    fn a_refused_volume_has_no_holes_to_report() {
        assert_eq!(Probe::Sparse.holes(), Some(crate::compose::Holes::Sparse));
        assert_eq!(Probe::Full.holes(), Some(crate::compose::Holes::Full));
        assert_eq!(
            Probe::TooBig {
                at: 8,
                why: "File too large".into()
            }
            .holes(),
            None
        );
        assert_eq!(
            Probe::Refused {
                why: "read-only".into()
            }
            .holes(),
            None
        );
    }

    /// **A path with a quote in it does not reach a shell**, because there is no shell.
    ///
    /// The whole of `df`'s argument is one `Command::arg`. This asserts the behaviour rather than
    /// the absence of a `sh -c`: a directory whose name would end a quoted string still measures.
    #[test]
    fn a_path_with_a_quote_in_it_measures_rather_than_breaking() {
        if cfg!(windows) {
            return; // `"` is not a legal filename character there.
        }
        let d = scratch("a'weird \"name\"; echo hi");
        let s = space(&d);
        assert!(
            s.as_ref().is_none_or(|s| s.free > 0),
            "a quoted path measured zero free: {s:?}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A path nothing could measure is `None` — and `None` is not zero.
    #[test]
    fn an_unmeasurable_path_is_not_measured_rather_than_empty() {
        // `/` always exists, so the walk up finds something and the measurement is real.
        let here = space(Path::new("/definitely/not/here/ipod-emulator-test"));
        if let Some(s) = here {
            assert!(s.free > 0, "the root volume reported {} free", s.free);
        }
        // An empty path has no existing ancestor and cannot be measured at all.
        assert_eq!(nearest_existing(Path::new("")), None);
    }

    /// **The refusal names the directory, not the probe file.**
    ///
    /// The probe file is `.ipod-probe-<pid>`, it exists for a few milliseconds, and it is gone
    /// before anybody reads the sentence. A person handed its path has nothing to go and look at.
    #[test]
    fn a_refusal_names_something_a_person_can_go_and_look_at() {
        let d = scratch("refused");
        // A regular file where the directory should be. `create_dir_all` refuses this on every
        // platform, which is what makes it a fixture rather than a permission trick.
        let blocked = d.join("drives");
        std::fs::write(&blocked, b"not a directory").expect("the blocking file");
        let answer = probe(&blocked, 1024);
        let Probe::Refused { why } = &answer else {
            panic!("a file where the directory should be answered {answer:?}");
        };
        assert!(
            why.contains(&blocked.display().to_string()),
            "the refusal does not name the directory it is about: {why}"
        );

        // **The other refusal, and it is the one that shipped wrong**: a directory that exists and
        // cannot be written in. This is the read-only-`drives/` case, where the probe file's path
        // was what a person was handed — a file called `.ipod-probe-95411` that lives for a few
        // milliseconds and is gone before the sentence is read.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let ro = d.join("read-only");
            std::fs::create_dir_all(&ro).expect("a directory to lock");
            std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).expect("chmod");
            if std::fs::File::create(ro.join("canary")).is_ok() {
                // Running as root, or on a filesystem that ignores the mode. Not a failure of the
                // program, and pretending it measured something would be the eighth lying
                // instrument in this project.
                println!("SKIPPED: this process can write into a 0555 directory");
            } else {
                let answer = probe(&ro, 1024);
                let Probe::Refused { why } = &answer else {
                    panic!("a read-only directory answered {answer:?}");
                };
                assert!(
                    why.contains(&ro.display().to_string()),
                    "the refusal does not name the directory it is about: {why}"
                );
                assert!(
                    !why.contains(PROBE_PREFIX),
                    "the refusal names the probe file, which is gone by the time anybody reads \
                     it: {why}"
                );
            }
            let _ = std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755));
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **`space` is checked against a second measurement of the same volume**, because until now
    /// every test of the free-space gate handed it a `Space` somebody typed.
    ///
    /// The failure this catches is a unit or a column slip — `df -Pk` reports **kibibytes** in
    /// **field 3**, and dropping the `× 1024` or reading field 2 leaves every arithmetic test in
    /// the program green while the gate is wrong by 1024× or by the used-space figure. So this
    /// writes a file of a known size and asserts the reading moves by about that much, which no
    /// mis-scaled or mis-indexed reading can do.
    #[test]
    fn the_free_space_reading_moves_by_what_was_written() {
        let d = scratch("df");
        let Some(before) = space(&d) else {
            println!("SKIPPED: nothing on this machine could measure {}", d.display());
            let _ = std::fs::remove_dir_all(&d);
            return;
        };
        assert!(before.free > 0, "a writable scratch volume reported 0 free");
        assert!(!before.mount.is_empty(), "the reading named no mount point");

        // 64 MiB: large enough to clear the noise of a live machine writing elsewhere, small
        // enough to be polite. Written, not `set_len`, so a sparse filesystem still charges for it.
        const BLOCK: u64 = 64 * 1024 * 1024;
        let big = d.join("ballast");
        std::fs::write(&big, vec![0u8; BLOCK as usize]).expect("64 MiB of ballast");
        let after = space(&d).expect("the same volume, a moment later");
        assert_eq!(after.mount, before.mount, "the reading moved to another volume");

        let dropped = before.free.saturating_sub(after.free);
        // Half to four times: other processes are writing too, and a filesystem may not account for
        // a fresh write immediately. What this refuses is a reading in the wrong UNIT — a `× 1024`
        // that is missing puts the drop at 64 KiB, and one applied twice puts it at 64 GiB.
        assert!(
            (BLOCK / 2..=BLOCK * 4).contains(&dropped),
            "writing {} moved the free-space reading by {} — the figure is not in bytes, or it is \
             not the available column ({} then {})",
            crate::si(BLOCK),
            crate::si(dropped),
            crate::si(before.free),
            crate::si(after.free)
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
