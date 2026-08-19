//! Opening the drive image on the **host**, so a person can put their own files on it.
//!
//! Rockbox plugins, themes, WADs, music, a `loader.cfg`, a kernel — the list of things somebody
//! might want on the volume is open-ended, and a program that only accepts what it can identify
//! will always be missing something. The escape hatch is the one the device itself has: on a real
//! iPod you hold `SELECT`+`PLAY`, it appears on your computer as a disk, and you drag things onto
//! it.
//!
//! **This is not disk mode, and the difference matters.** Target disk mode is a USB feature; USB is
//! not modelled here, and the boot ROM's own `disk` image faults after 128 K instructions. What
//! this does is mount the *image file* with the host's own tools. Same outcome for a person,
//! completely different mechanism, and the window says which one it is doing rather than letting
//! somebody believe we have emulated the other.
//!
//! ## It is not the same on every system, and one of the three cannot do it at all
//!
//! | | how | notes |
//! |---|---|---|
//! | macOS | `hdiutil attach` | recognises our images natively — `FDisk_partition_scheme`, `Windows_FAT_32` |
//! | Linux | `udisksctl loop-setup` | no root needed where udisks2 is running; `losetup` needs it |
//! | Windows | **nothing built in** | Windows mounts ISO and VHD, not a raw image. Third-party tools exist and this program will not require one |
//!
//! So Windows needs the in-window files view instead, which is why that is not an optional
//! nicety — it is one platform's only route. [`available`] says which case this machine is, and
//! the caller is expected to offer something else when the answer is `None`.

use std::path::Path;
use std::process::Command;

/// How this host can mount a raw drive image, if it can.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mounter {
    /// macOS. `hdiutil attach -imagekey diskimage-class=CRawDiskImage`.
    HdiUtil,
    /// Linux with udisks2 — a loop device and a mount, both without root.
    UDisks,
}

impl Mounter {
    /// What to tell a person this button is going to do.
    pub fn describe(self) -> &'static str {
        match self {
            Mounter::HdiUtil => "attaches the drive image and opens it in the Finder",
            Mounter::UDisks => "attaches the drive image as a loop device and opens it",
        }
    }
}

/// The mounter this machine has, if any.
///
/// Checked by asking whether the tool is actually there rather than by `cfg!(target_os)` alone: a
/// Linux box without udisks2 running would otherwise be told it can do something it cannot, and
/// find out at the moment it matters.
pub fn available() -> Option<Mounter> {
    if cfg!(target_os = "macos") && which("hdiutil") {
        return Some(Mounter::HdiUtil);
    }
    if cfg!(target_os = "linux") && which("udisksctl") {
        return Some(Mounter::UDisks);
    }
    None
}

fn which(tool: &str) -> bool {
    Command::new(tool)
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

/// Attach `image` and reveal it, returning the lines to show the person.
///
/// **The machine must not be running.** Two writers on one filesystem is how a volume gets
/// corrupted, and the corruption would land on the drive somebody keeps their music on. The caller
/// powers the iPod off first; this cannot check for it and says so rather than pretending to.
pub fn open(image: &Path) -> Result<Vec<String>, String> {
    let Some(m) = available() else {
        return Err(
            "this system has no way to mount a raw drive image — Windows mounts ISO and VHD but \
             not this, and requiring a third-party tool is not something this program will do. \
             Use the files view instead."
                .into(),
        );
    };
    let mut out = Vec::new();
    match m {
        Mounter::HdiUtil => {
            let r = Command::new("hdiutil")
                .args(["attach", "-imagekey", "diskimage-class=CRawDiskImage"])
                .arg(image)
                .output()
                .map_err(|e| format!("hdiutil: {e}"))?;
            if !r.status.success() {
                return Err(format!(
                    "hdiutil could not attach it: {}",
                    String::from_utf8_lossy(&r.stderr).trim()
                ));
            }
            let text = String::from_utf8_lossy(&r.stdout).into_owned();
            // The last field of the FAT32 line is where it was mounted.
            let at = text
                .lines()
                .find(|l| l.contains("Windows_FAT_32") || l.contains("/Volumes/"))
                .and_then(|l| l.split_whitespace().last().map(str::to_string));
            match at {
                Some(p) if p.starts_with('/') => {
                    let _ = Command::new("open").arg(&p).status();
                    out.push(format!("mounted at {p}"));
                }
                _ => out.push(
                    "attached, but the volume did not mount — the drive may have no filesystem yet"
                        .into(),
                ),
            }
            out.push("Eject it in the Finder before starting the iPod again.".into());
        }
        Mounter::UDisks => {
            let r = Command::new("udisksctl")
                .args(["loop-setup", "-f"])
                .arg(image)
                .output()
                .map_err(|e| format!("udisksctl: {e}"))?;
            if !r.status.success() {
                return Err(format!(
                    "udisksctl could not attach it: {}",
                    String::from_utf8_lossy(&r.stderr).trim()
                ));
            }
            let text = String::from_utf8_lossy(&r.stdout).into_owned();
            // `Mapped file … as /dev/loop0.`
            let dev = text
                .split_whitespace()
                .last()
                .map(|s| s.trim_end_matches('.').to_string())
                .unwrap_or_default();
            out.push(format!("attached as {dev}"));
            // Partition 1 is the data volume; partition 0 is Apple's firmware and has no filesystem.
            let part = format!("{dev}p1");
            let m = Command::new("udisksctl").args(["mount", "-b", &part]).output();
            match m {
                Ok(r) if r.status.success() => {
                    let text = String::from_utf8_lossy(&r.stdout).into_owned();
                    if let Some(p) = text.split(" at ").nth(1) {
                        let p = p.trim().trim_end_matches('.').to_string();
                        let _ = Command::new("xdg-open").arg(&p).status();
                        out.push(format!("mounted at {p}"));
                    }
                }
                _ => out.push(format!("attached, but {part} did not mount")),
            }
            out.push(format!("Unmount and `udisksctl loop-delete -b {dev}` before starting the iPod again."));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The point of `available` is that it can say **no**, and that the caller has to handle it.
    /// Windows is the case that proves it: there is no built-in way to mount a raw image there, so
    /// a design that assumed a host mount would have shipped one platform with a dead button.
    #[test]
    fn a_host_that_cannot_mount_says_so_rather_than_failing_later() {
        // Whatever this machine is, the answer is a decision and not a panic.
        let m = available();
        if cfg!(target_os = "windows") {
            assert_eq!(m, None, "Windows has no built-in raw-image mount");
        }
        // And every mounter can describe itself, because the button has to say what it will do
        // before somebody presses it.
        for m in [Mounter::HdiUtil, Mounter::UDisks] {
            assert!(!m.describe().is_empty());
        }
    }

    /// A refusal has to name the alternative. "Cannot mount" on its own leaves somebody stuck.
    #[test]
    fn the_refusal_points_somewhere() {
        if available().is_none() {
            let e = open(Path::new("/nonexistent.img")).unwrap_err();
            assert!(e.contains("files view"), "{e}");
        }
    }
}
