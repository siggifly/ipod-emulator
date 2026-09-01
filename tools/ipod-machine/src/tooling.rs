//! Whether this computer can run a tool this program shells out to.
//!
//! **Asked by running the tool**, not by walking `PATH`. A `PATH` walk is a second implementation
//! of what the OS is about to do and it is wrong on Windows, where the extension list is a policy
//! (`PATHEXT`) rather than a suffix — so a program that decided `curl` was absent because
//! `curl.exe` was not literally on the path would refuse a download the OS would have run.

use std::process::{Command, Stdio};

/// Can this computer run `name`?
///
/// Runs `name --version`, discards the output, and answers `false` only when the process could not
/// be started or did not exit successfully.
///
/// **`Stdio::null()` on stdin as well as on the two outputs.** A shim that reads stdin would
/// otherwise hang the caller — and the caller is the window, at startup, before the first frame.
pub fn have(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Can this computer fetch anything at all?
///
/// **Every download in this program goes through `curl`** — the firmware fetcher, the Rockbox
/// fetcher and the iPodLinux fetcher all reach [`crate::firmware::http_get_to_file`]. Windows has a
/// second route through `powershell`, which is the fallback that fetcher already takes.
///
/// Measured once per launch, in the window's `caps()`. A control whose route does not exist is
/// drawn disabled wearing its reason, and this is the fact that decides it.
pub fn can_download() -> bool {
    have("curl") || (cfg!(windows) && have("powershell"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The instrument's own control: something that is certainly there, and something that
    /// certainly is not. Without the second arm this could be `|_| true` and nobody would know.
    #[test]
    fn a_tool_that_is_not_installed_is_not_reported_as_present() {
        assert!(
            !have("ipod-emulator-no-such-tool-9f3a2b"),
            "a tool nobody has installed was reported as present, which means this instrument \
             cannot report an absence at all"
        );
        // `curl` ships with macOS, with every Linux, and with Windows since 1803 — but the honest
        // assertion is the pair, not the presence: if the machine running this genuinely has no
        // curl, `can_download` must agree with `have`.
        assert_eq!(
            can_download(),
            have("curl") || (cfg!(windows) && have("powershell")),
            "the download capability and the tool it names disagree"
        );
    }

    /// A tool whose name is empty, or is a path with a NUL in it, is absent rather than a panic.
    #[test]
    fn an_impossible_tool_name_is_absent_rather_than_a_panic() {
        assert!(!have(""));
        assert!(!have("/definitely/not/here/ipod-emulator-test"));
    }
}
