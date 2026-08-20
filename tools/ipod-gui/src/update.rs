//! An update **check**. Not an updater.
//!
//! One HTTPS GET of GitHub's releases API, one version comparison, and — if there is something
//! newer — one line of text with a link the user follows themselves. That is the whole feature, and
//! the restraint is the feature:
//!
//! - **Nothing is downloaded.** No archive, no binary, no signature, no manifest.
//! - **Nothing is executed.** The response is parsed for one string and thrown away.
//! - **Nothing is modified.** This program never writes to its own installation.
//! - **It is off unless asked for.** Not a silent call on launch. A reverse-engineering tool that
//!   phones home the moment it opens is a bad first impression for no benefit, and this audience
//!   notices. [`Settings::check_updates_on_start`](eapp_loader::settings::Settings) defaults to false;
//!   the menu item works whatever it says.
//! - **It fails silently and completely.** Offline, DNS down, GitHub unreachable, rate-limited,
//!   proxied, a response in a shape nobody expected — every one of those returns [`None`] and the
//!   UI says nothing. An emulator that shows a network error on launch is worse than one that
//!   never checks.
//!
//! # Why a subprocess and not an HTTP crate
//!
//! An HTTPS GET in Rust means a TLS stack, and every one of them is a large dependency with a
//! build-time story on at least one of the three platforms — `ring`'s assembler on Windows,
//! `aws-lc-rs`'s cmake, or OpenSSL headers on Linux. This project's core crates have **no**
//! third-party dependencies at all and the window has exactly one (`eframe`); paying a TLS stack
//! and a CI toolchain to fetch forty bytes of JSON, once, when asked, is the wrong trade.
//!
//! `curl` is on all three: macOS ships it, every Linux has it, and Windows has carried `curl.exe`
//! in System32 since Windows 10 1803. PowerShell's `Invoke-WebRequest` is the fallback where it is
//! somehow absent. The command line is fixed and visible below — a URL, a five-second timeout, and
//! `-f` so an HTTP error is an exit code rather than a body that parses as garbage.

use std::process::{Command, Stdio};
use std::time::Duration;

/// The repository the releases live in.
///
/// A constant rather than a lookup: there is exactly one, and an emulator that took its update URL
/// from a file would be an emulator you could point at anything. `IPOD_EMULATOR_UPDATE_REPO` overrides
/// it for a fork, which is a different thing from configuring it at rest.
pub const REPO: &str = "siggifly/ipod-emulator";

/// This build's version — `ipod-emulator`'s `Cargo.toml`, at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn repo() -> String {
    std::env::var("IPOD_EMULATOR_UPDATE_REPO").unwrap_or_else(|_| REPO.to_string())
}

/// What a check found. `Current` and `Newer` are both successes; there is no error variant,
/// deliberately — see the module note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// Nothing newer. `String` is the latest tag, so the UI can say what it compared against.
    Current(String),
    /// A newer release: the tag, and the page to get it from.
    Newer { tag: String, url: String },
}

impl Found {
    pub fn line(&self) -> String {
        match self {
            Found::Current(tag) => {
                format!("Up to date — this is {VERSION}, latest release is {tag}.")
            }
            Found::Newer { tag, url } => format!("{tag} is available. You have {VERSION}. {url}"),
        }
    }
}

/// Ask GitHub for the latest release. [`None`] on **any** failure, including no network.
///
/// Blocking, and expected to be called off the UI thread. The five-second cap is `curl`'s, not a
/// timer here, so a hung connection cannot outlive the call.
pub fn check() -> Option<Found> {
    let body = fetch(&format!(
        "https://api.github.com/repos/{}/releases/latest",
        repo()
    ))?;
    let tag = json_string(&body, "tag_name")?;
    let url = json_string(&body, "html_url")
        .unwrap_or_else(|| format!("https://github.com/{}/releases/latest", repo()));
    let mine = parse_version(VERSION)?;
    let theirs = parse_version(&tag)?;
    Some(if theirs > mine {
        Found::Newer { tag, url }
    } else {
        Found::Current(tag)
    })
}

/// The GET. Two attempts, both of them a system tool that already exists.
fn fetch(url: &str) -> Option<String> {
    // `-f` fail on HTTP >= 400 · `-s` no progress meter · `-S` still print the error to stderr,
    // which is discarded below · `-L` follow the one redirect the API can answer with · a hard
    // total timeout so nothing here can hang a menu.
    let curl = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "5",
            "-H",
            "Accept: application/vnd.github+json",
            "-A",
            &format!("ipod-emulator/{VERSION}"),
            url,
        ])
        .stderr(Stdio::null())
        .output();
    if let Ok(o) = curl {
        if o.status.success() && !o.stdout.is_empty() {
            return Some(String::from_utf8_lossy(&o.stdout).into_owned());
        }
    }
    if !cfg!(windows) {
        return None;
    }
    // Windows without curl.exe — pre-1803, or a stripped image. `-UseBasicParsing` keeps it off
    // Internet Explorer's engine, which is not present on Server Core.
    let ps = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "try {{ (Invoke-WebRequest -UseBasicParsing -TimeoutSec 5 -Uri '{url}').Content }} \
                 catch {{ }}"
            ),
        ])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let out = String::from_utf8_lossy(&ps.stdout).into_owned();
    if ps.status.success() && !out.trim().is_empty() {
        Some(out)
    } else {
        None
    }
}

/// Pull one string field out of a JSON object, without a JSON parser.
///
/// The response is one object of flat scalars and one nested array, and this needs two of the
/// scalars. A parser would be several hundred lines or another dependency to read forty bytes.
/// It handles the escapes JSON can put in a tag or a URL (`\"`, `\\`, `\/`, `\n`, `\uXXXX` left
/// as-is) and returns [`None`] on anything it does not understand, which the caller treats as a
/// failed check — the same as being offline.
fn json_string(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let mut from = 0usize;
    loop {
        let at = body[from..].find(&needle)? + from;
        let after = &body[at + needle.len()..];
        let rest = after.trim_start();
        if !rest.starts_with(':') {
            from = at + needle.len();
            continue;
        }
        let rest = rest[1..].trim_start();
        if !rest.starts_with('"') {
            return None;
        }
        let mut out = String::new();
        let mut chars = rest[1..].chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(out),
                '\\' => match chars.next()? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    other => out.push(other),
                },
                other => out.push(other),
            }
        }
        return None;
    }
}

/// `v0.2.1`, `0.2.1`, `clickwheel-v0.2.1` -> `(0, 2, 1)`.
///
/// Tolerant on the prefix because a repository that holds more than one thing tags them apart, and
/// strict after it: three numbers, and anything trailing (`-rc1`, `+build`) is dropped. A tag that
/// does not parse returns [`None`] and the check reports nothing — which is right, because "I could
/// not understand the latest tag" is not the same as "you are out of date" and must never be shown
/// as if it were.
pub fn parse_version(tag: &str) -> Option<(u32, u32, u32)> {
    let s = tag.trim();
    // Take everything after the last `v` that is followed by a digit, so `clickwheel-v1.2.3` and
    // `v1.2.3` and `1.2.3` all land on the same three numbers.
    let s = match s.rfind(['v', 'V']) {
        Some(i) if s[i + 1..].starts_with(|c: char| c.is_ascii_digit()) => &s[i + 1..],
        _ => s,
    };
    let core: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().unwrap_or("0").parse().unwrap_or(0);
    let patch = it.next().unwrap_or("0").parse().unwrap_or(0);
    Some((major, minor, patch))
}

/// The check, on its own thread, with the result posted back through a shared slot.
///
/// The window must never block on a socket: a five-second `curl` on the UI thread is five seconds
/// of a frozen iPod, and the whole point of failing silently is that nobody notices the check
/// happened at all.
pub fn spawn(slot: std::sync::Arc<std::sync::Mutex<Option<Option<Found>>>>) {
    std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            let found = check();
            *slot.lock().unwrap() = Some(found);
        })
        .ok();
    // A thread that cannot be spawned leaves the slot at `None`, which reads as "still checking"
    // forever — harmless, silent, and the same as every other failure here.
    let _ = Duration::from_secs(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_with_or_without_a_prefix() {
        assert_eq!(parse_version("0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_version("v0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_version("clickwheel-v1.20.3"), Some((1, 20, 3)));
        assert_eq!(parse_version(" v2.0 "), Some((2, 0, 0)));
        assert_eq!(parse_version("v1.2.3-rc1"), Some((1, 2, 3)));
    }

    /// A tag nobody can read is not a newer version. This is the case that decides whether a
    /// mangled response can produce a false "update available", and it must not.
    #[test]
    fn an_unparseable_tag_is_not_an_update() {
        assert_eq!(parse_version("nightly"), None);
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("v"), None);
    }

    #[test]
    fn ordering_is_numeric_not_lexicographic() {
        assert!(
            parse_version("v0.10.0") > parse_version("v0.9.0"),
            "10 > 9, not '1' < '9'"
        );
        assert!(parse_version("v1.0.0") > parse_version("v0.99.99"));
        assert_eq!(parse_version("v1.2.3"), parse_version("1.2.3"));
    }

    #[test]
    fn the_field_extractor_reads_the_real_response_shape() {
        let body = r#"{"url":"https://api.github.com/x","assets_url":"y","tag_name":"v0.4.0",
                       "name":"iPod 5G emulator 0.4.0","draft":false,
                       "html_url":"https://github.com/o/r/releases/tag/v0.4.0","assets":[]}"#;
        assert_eq!(json_string(body, "tag_name").as_deref(), Some("v0.4.0"));
        assert_eq!(
            json_string(body, "html_url").as_deref(),
            Some("https://github.com/o/r/releases/tag/v0.4.0")
        );
        assert_eq!(json_string(body, "nothing_like_it"), None);
    }

    /// A key name that appears inside a *value* must not be mistaken for the key. GitHub's own
    /// response contains `.../releases/tag_name`-shaped URLs often enough to matter.
    #[test]
    fn a_key_appearing_inside_a_value_is_skipped() {
        let body = r#"{"body":"see \"tag_name\" below","tag_name":"v9.9.9"}"#;
        assert_eq!(json_string(body, "tag_name").as_deref(), Some("v9.9.9"));
    }

    #[test]
    fn escapes_in_a_value_are_unescaped() {
        let body = r#"{"html_url":"https:\/\/example.com\/a\"b"}"#;
        assert_eq!(
            json_string(body, "html_url").as_deref(),
            Some("https://example.com/a\"b")
        );
    }

    /// Truncated JSON — the shape a half-received response has — must be `None`, never a partial
    /// string that then compares as a version.
    #[test]
    fn a_truncated_response_yields_nothing() {
        assert_eq!(json_string(r#"{"tag_name":"v1.0"#, "tag_name"), None);
        assert_eq!(json_string(r#"{"tag_name":"#, "tag_name"), None);
        assert_eq!(json_string("", "tag_name"), None);
        assert_eq!(json_string("<html>404</html>", "tag_name"), None);
    }

    /// The offline contract, exercised without a network: point the check at a host that cannot
    /// resolve and require silence rather than an error. This is the behaviour the brief calls out
    /// — an emulator that shows a network error on launch is worse than one that never checks.
    #[test]
    fn an_unreachable_host_returns_none_rather_than_failing() {
        // `.invalid` is reserved by RFC 2606 and guaranteed never to resolve, so this test is
        // deterministic on a machine WITH a network as well as on one without.
        assert_eq!(fetch("https://ipod-emulator-update-check.invalid/x"), None);
    }

    #[test]
    fn the_lines_shown_to_a_person_name_both_versions() {
        let n = Found::Newer {
            tag: "v9.9.9".into(),
            url: "https://example.com/r".into(),
        };
        assert!(n.line().contains("v9.9.9"));
        assert!(n.line().contains(VERSION));
        assert!(n.line().contains("https://example.com/r"));
        assert!(Found::Current("v0.1.0".into())
            .line()
            .contains("Up to date"));
    }
}
