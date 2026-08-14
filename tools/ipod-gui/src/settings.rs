//! What the window remembers between launches, and the three directories it remembers it in.
//!
//! Deliberately a hand-written `key = value` file rather than eframe's `persistence` feature. That
//! feature pulls `serde` and `ron` into a crate whose whole dependency argument is "eframe and
//! nothing else", and it persists egui's own window geometry as a side effect, which is state this
//! project has no opinion about. Four keys do not need a serialiser.
//!
//! # Where the directories are, and why not `$TMPDIR` for all of them
//!
//! The snapshot cache used to live under [`std::env::temp_dir`]. That is correct on macOS, where
//! `$TMPDIR` is `/var/folders/…` on a real disk, and wrong on Linux, where `/tmp` is `tmpfs` on
//! most distributions — RAM-backed and typically capped at half of it. The cache holds a snapshot
//! **and an 8 GB working disk image**, so on a Linux box the old default either filled `/tmp` or
//! ate half the machine's memory. It is a cache directory now, on all three platforms.

use std::path::PathBuf;

/// The two ways the window can be looked at.
///
/// One toggle, not a pile of checkboxes: the choice is between "this is an iPod" and "this is an
/// instrument", and every counter on screen belongs to the second.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    /// The iPod and nothing else. The default, and what a stranger who cloned this should meet.
    #[default]
    User,
    /// Instruction counts, both clocks, the task watches, the ATA census, the framebuffer
    /// inspector, the screenshot button.
    Debug,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::User => "user",
            Mode::Debug => "debug",
        }
    }

    fn parse(s: &str) -> Option<Mode> {
        match s.trim() {
            "user" => Some(Mode::User),
            "debug" => Some(Mode::Debug),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub mode: Mode,
    /// The NOR dump and the drive image the setup screen was pointed at. Absent until somebody
    /// picks them, which is the state a fresh clone is in.
    pub flash: Option<PathBuf>,
    pub disk: Option<PathBuf>,
    /// **Off unless asked for.** An emulator that phones home on launch is a bad first impression
    /// for no benefit, and this audience notices. The menu item works either way — this only
    /// decides whether the check happens on its own.
    pub check_updates_on_start: bool,
}

impl Settings {
    /// Read the settings file, tolerating everything: a missing file, a missing directory, a
    /// truncated write, a key from a future version. A settings file is not a place to fail.
    pub fn load() -> Settings {
        match config_dir().map(|d| d.join(FILE)).and_then(|p| std::fs::read_to_string(p).ok()) {
            Some(text) => Settings::parse(&text),
            None => Settings::default(),
        }
    }

    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "mode" => s.mode = Mode::parse(v).unwrap_or_default(),
                // Empty is "not set", which is what an editor that blanked a line means.
                "flash" if !v.is_empty() => s.flash = Some(PathBuf::from(v)),
                "disk" if !v.is_empty() => s.disk = Some(PathBuf::from(v)),
                "check_updates_on_start" => s.check_updates_on_start = v == "true",
                _ => {}
            }
        }
        s
    }

    pub fn render(&self) -> String {
        let p = |o: &Option<PathBuf>| {
            o.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
        };
        format!(
            "# ipod-gui settings. Hand-editable; unknown keys are ignored.\n\
             mode = {}\n\
             flash = {}\n\
             disk = {}\n\
             # An HTTPS GET of the GitHub releases API and a version comparison, on launch.\n\
             # Off by default on purpose. The menu item works whatever this says.\n\
             check_updates_on_start = {}\n",
            self.mode.as_str(),
            p(&self.flash),
            p(&self.disk),
            self.check_updates_on_start,
        )
    }

    /// Best-effort. A window that could not write its preferences is a window that opens in user
    /// mode next time, not a window that refuses to run.
    pub fn save(&self) {
        let Some(dir) = config_dir() else { return };
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let _ = std::fs::write(dir.join(FILE), self.render());
    }

    /// Where the settings live, for the UI to print. A preference nobody can find is a preference
    /// nobody can reset.
    pub fn path() -> Option<PathBuf> {
        config_dir().map(|d| d.join(FILE))
    }
}

const FILE: &str = "settings.txt";

/// Per-platform config directory, resolved from the environment rather than from a crate.
///
/// - Windows: `%APPDATA%\ipod-gui`
/// - macOS: `~/Library/Application Support/ipod-gui`
/// - everything else: `$XDG_CONFIG_HOME/ipod-gui`, else `~/.config/ipod-gui`
pub fn config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return env_dir("APPDATA").map(|d| d.join(APP));
    }
    if cfg!(target_os = "macos") {
        return home().map(|h| h.join("Library/Application Support").join(APP));
    }
    env_dir("XDG_CONFIG_HOME")
        .or_else(|| home().map(|h| h.join(".config")))
        .map(|d| d.join(APP))
}

/// Per-platform cache directory: the snapshot, and the 8 GB working disk beside it.
///
/// - Windows: `%LOCALAPPDATA%\ipod-gui`
/// - macOS: `~/Library/Caches/ipod-gui`
/// - everything else: `$XDG_CACHE_HOME/ipod-gui`, else `~/.cache/ipod-gui`
///
/// Falls back to the temp directory when there is no home to hang it off — a CI runner, a daemon,
/// a container with no `HOME`. Regenerable either way: the whole directory can be deleted and the
/// only cost is one 75-second cold boot.
pub fn cache_dir() -> PathBuf {
    let d = if cfg!(windows) {
        env_dir("LOCALAPPDATA").map(|d| d.join(APP))
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Caches").join(APP))
    } else {
        env_dir("XDG_CACHE_HOME")
            .or_else(|| home().map(|h| h.join(".cache")))
            .map(|d| d.join(APP))
    };
    d.unwrap_or_else(|| std::env::temp_dir().join(APP))
}

const APP: &str = "ipod-gui";

fn env_dir(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn home() -> Option<PathBuf> {
    env_dir("HOME").or_else(|| env_dir("USERPROFILE"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_user_mode_with_nothing_configured() {
        let s = Settings::parse("");
        assert_eq!(s.mode, Mode::User);
        assert_eq!(s.flash, None);
        assert_eq!(s.disk, None);
        assert!(!s.check_updates_on_start);
    }

    /// The round trip is the contract: whatever the window writes, the next launch reads back.
    #[test]
    fn settings_round_trip_through_the_file_format() {
        let s = Settings {
            mode: Mode::Debug,
            flash: Some(PathBuf::from("/a/b/rom.bin")),
            disk: Some(PathBuf::from("/a/b/disk.img")),
            check_updates_on_start: true,
        };
        assert_eq!(Settings::parse(&s.render()), s);
    }

    /// A path with a space in it must survive the format, which is why the value is the rest of
    /// the line and not a token. Spaces in a chosen path are ordinary, not exotic.
    #[test]
    fn a_path_with_spaces_survives() {
        let p = PathBuf::from("/some where/My iPod Backups/x.img");
        let s = Settings { disk: Some(p.clone()), ..Default::default() };
        assert_eq!(Settings::parse(&s.render()).disk, Some(p));
    }

    /// Garbage in a settings file must not stop the program. Every one of these lines is something
    /// a hand-edit produces.
    #[test]
    fn junk_is_ignored_rather_than_fatal() {
        let s = Settings::parse(
            "# comment\n\
             \n\
             mode = sideways\n\
             nonsense\n\
             future_key = 7\n\
             disk =\n\
             check_updates_on_start = maybe\n",
        );
        assert_eq!(s.mode, Mode::User, "an unknown mode falls back to the default");
        assert_eq!(s.disk, None, "an empty value is `not set`, not `the empty path`");
        assert!(!s.check_updates_on_start, "anything but `true` is off");
    }

    #[test]
    fn the_cache_directory_is_absolute_and_named() {
        let d = cache_dir();
        assert!(d.is_absolute(), "{}", d.display());
        assert!(d.ends_with(APP), "{}", d.display());
    }
}
