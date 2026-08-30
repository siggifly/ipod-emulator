//! The macOS `.app` bundle — `ipod-emulator --make-app`.
//!
//! **No toolkit here either.** Four directories, one copy, one XML file and, where the tools for it
//! exist, an icon. Every part of that is testable with no display, and the parts that need macOS say
//! so at run time rather than at compile time, so the Linux leg compiles and exercises the same code
//! path the Mac runs.
//!
//! ## What this replaces, and why it had to be re-authored
//!
//! A shell script wrote this bundle until it became a flag; the flag went with the egui `main.rs`
//! and took the bundler with it. `grep -rn make_app --include='*.rs' tools/` was empty when this was
//! written — bundle layout, `Info.plist`, icon, all of it gone — and `release.yml` had been invoking
//! the hole for weeks.
//!
//! ## It bundles itself
//!
//! [`make_app`] takes no path to a binary. The script it originally replaced did, and that is one
//! more thing a release step can get wrong: point it at yesterday's build and it produces an app
//! that looks right and is stale. [`std::env::current_exe`] cannot be the wrong binary.
//!
//! [`write_bundle`] does take one, and only so that a test can hand it a fixture instead of copying
//! a release binary into a temp directory on every run. Nothing that ships calls it with anything
//! but `current_exe()`.
//!
//! ## The version comes from the compiler
//!
//! `CARGO_PKG_VERSION`, through [`crate::update::VERSION`] — the same constant `--check-update`
//! compares against. A bundle cannot report a number the program inside it disagrees with, which is
//! the failure that made a release checklist necessary in the first place.

use std::path::{Path, PathBuf};

use crate::update::VERSION;

/// What the bundle directory is called, inside whatever directory is named.
pub const APP: &str = "ipod-emulator.app";

/// Why a bundle cannot be built anywhere else.
///
/// **A constant so that the sentence is checkable on the platform that never prints it.** A message
/// only Linux can produce and only macOS can test is a message nobody reads until it is wrong.
const NOT_MACOS: &str = "a .app bundle is a macOS format; there is nothing to build here. The \
                         plain binary in this archive is the whole program on every other platform";

/// Whether this platform can be asked for a bundle at all, and what to say if it cannot.
///
/// **An error rather than a silent no-op**, deliberately: a release script that "succeeded" at
/// producing nothing is worse than one that stopped, because the archive ships without the
/// double-clickable app and nobody finds out until somebody downloads it.
pub fn unsupported() -> Option<String> {
    (!cfg!(target_os = "macos")).then(|| NOT_MACOS.to_string())
}

/// Wrap **this** binary in a `.app` under `out`, and return where it went.
pub fn make_app(out: &Path, icon: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(why) = unsupported() {
        return Err(why);
    }
    let me = std::env::current_exe().map_err(|e| format!("cannot find this binary: {e}"))?;
    write_bundle(&me, out, icon)
}

/// The bundle, around a named executable.
///
/// The layout is Apple's and is not negotiable: `Contents/MacOS/<CFBundleExecutable>`,
/// `Contents/Resources/`, `Contents/Info.plist`. Launch Services reads the plist to find the first
/// of those, so a bundle whose executable name and `CFBundleExecutable` disagree opens to an error
/// dialog rather than to a window.
pub fn write_bundle(exe: &Path, out: &Path, icon: Option<&Path>) -> Result<PathBuf, String> {
    let app = out.join(APP);

    // **Rebuilt, not merged.** A second build over a first would otherwise leave whatever the first
    // put there — an icon that has since been removed, a helper that no longer exists — inside an
    // app that reports itself as this version. The path deleted is one this function is about to
    // create and is named by a constant, so there is no argument through which this reaches
    // anything of the operator's; `AGENTS.md` §3 is why that sentence is here rather than assumed.
    let _ = std::fs::remove_dir_all(&app);

    let macos = app.join("Contents/MacOS");
    let resources = app.join("Contents/Resources");
    std::fs::create_dir_all(&macos).map_err(|e| format!("{}: {e}", macos.display()))?;
    std::fs::create_dir_all(&resources).map_err(|e| format!("{}: {e}", resources.display()))?;

    // `copy` and not a read-then-write: on Unix it carries the permission bits over, and an
    // executable that lost its executable bit is a bundle that opens to "you do not have
    // permission" with nothing else wrong with it.
    std::fs::copy(exe, macos.join("ipod-emulator")).map_err(|e| format!("copying self: {e}"))?;

    if let Some(icon) = icon {
        try_icon(icon, &resources);
    }

    std::fs::write(app.join("Contents/Info.plist"), info_plist(VERSION))
        .map_err(|e| format!("Info.plist: {e}"))?;
    Ok(app)
}

/// The property list, as text. Pure, so what a bundle claims is checkable everywhere.
///
/// `CFBundleVersion` and `CFBundleShortVersionString` are the same number on purpose. They are
/// allowed to differ — one is a build counter and the other is what a person sees — and keeping two
/// numbers in step by hand is how they stop being in step.
pub fn info_plist(version: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>                 <string>ipod-emulator</string>
  <key>CFBundleDisplayName</key>          <string>ipod-emulator</string>
  <key>CFBundleExecutable</key>           <string>ipod-emulator</string>
  <key>CFBundleIdentifier</key>           <string>net.siggifly.ipod-emulator</string>
  <key>CFBundleVersion</key>              <string>{version}</string>
  <key>CFBundleShortVersionString</key>   <string>{version}</string>
  <key>CFBundlePackageType</key>          <string>APPL</string>
  <key>CFBundleIconFile</key>             <string>icon</string>
  <key>LSMinimumSystemVersion</key>       <string>11.0</string>
  <!-- The panel is 320x240 upscaled; without this it renders at 1x and looks soft on Retina. -->
  <key>NSHighResolutionCapable</key>      <true/>
</dict>
</plist>
"#
    )
}

/// Turn one PNG into `Resources/icon.icns`. `true` if there is now an icns there.
///
/// **Shelled out to `sips` and `iconutil`, which are macOS's own.** There is no Rust equivalent
/// worth carrying for a program whose only dependency outside the workspace is a UI toolkit, and an
/// `.icns` is a container of five resolutions rather than a picture — writing one by hand means
/// writing a PNG encoder's worth of code for a file that is looked at and never read.
///
/// **Missing either tool is not fatal.** An app with no icon runs; a release that stopped over a
/// picture would be worse. Off macOS this does not even try: neither tool exists there, and ten
/// subprocesses that all fail is a slower way of returning `false`.
fn try_icon(icon: &Path, resources: &Path) -> bool {
    if !cfg!(target_os = "macos") || !icon.is_file() {
        return false;
    }
    let set = std::env::temp_dir()
        .join(format!("ipod-iconset-{}", std::process::id()))
        .join("icon.iconset");
    let _ = std::fs::remove_dir_all(&set);
    if std::fs::create_dir_all(&set).is_err() {
        return false;
    }
    // The five sizes Apple's `iconutil` accepts, each at 1x and 2x. `sips` resamples; anything it
    // cannot read it declines, and the missing member is what makes `iconutil` decline in turn.
    for s in [16u32, 32, 128, 256, 512] {
        for (px, name) in [
            (s, format!("icon_{s}x{s}.png")),
            (s * 2, format!("icon_{s}x{s}@2x.png")),
        ] {
            let _ = std::process::Command::new("sips")
                .args(["-z", &px.to_string(), &px.to_string()])
                .arg(icon)
                .arg("--out")
                .arg(set.join(&name))
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
    let icns = resources.join("icon.icns");
    let _ = std::process::Command::new("iconutil")
        .arg("-c")
        .arg("icns")
        .arg(&set)
        .arg("-o")
        .arg(&icns)
        .stderr(std::process::Stdio::null())
        .status();
    if let Some(parent) = set.parent() {
        let _ = std::fs::remove_dir_all(parent);
    }
    icns.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory of this test's own, under the system temp dir and named for the test.
    ///
    /// No `tempfile` crate: this workspace's core crates have no third-party dependencies and the
    /// window has one. Removed on the way in rather than on the way out, so a failing run leaves
    /// what it built for somebody to look at.
    fn scratch(what: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ipod-bundle-{}-{what}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("a scratch directory");
        d
    }

    /// A stand-in for the binary. Not a real Mach-O — nothing here executes it, and copying a
    /// release binary per test is a hundred megabytes of nothing being proved.
    fn fixture_exe(dir: &Path) -> PathBuf {
        let p = dir.join("pretend-binary");
        std::fs::write(&p, b"\x7fELF not really\n").expect("the fixture");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755))
                .expect("the fixture is executable");
        }
        p
    }

    /// The whole of what Launch Services needs, in the places it looks for them.
    #[test]
    fn a_bundle_has_the_three_paths_macos_opens_it_by() {
        let dir = scratch("layout");
        let exe = fixture_exe(&dir);
        let app = write_bundle(&exe, &dir, None).expect("a bundle");

        assert_eq!(app, dir.join("ipod-emulator.app"));
        assert!(app.join("Contents/Info.plist").is_file(), "no Info.plist");
        assert!(app.join("Contents/Resources").is_dir(), "no Resources");
        let inside = app.join("Contents/MacOS/ipod-emulator");
        assert!(inside.is_file(), "no executable at Contents/MacOS/ipod-emulator");

        // The name inside must be the name the plist claims, or the app opens to an error dialog
        // and nothing else is wrong with it.
        let plist = std::fs::read_to_string(app.join("Contents/Info.plist")).expect("the plist");
        let claimed = plist
            .split("<key>CFBundleExecutable</key>")
            .nth(1)
            .and_then(|s| s.split("<string>").nth(1))
            .and_then(|s| s.split("</string>").next())
            .expect("CFBundleExecutable");
        assert_eq!(claimed, inside.file_name().unwrap().to_string_lossy());

        assert_eq!(
            std::fs::read(&inside).expect("the copy"),
            std::fs::read(&exe).expect("the fixture"),
            "the bundle holds different bytes from the binary it was made of"
        );
    }

    /// **The executable bit survives the copy.** `fs::copy` carries the mode over and a
    /// read-then-write would not; a bundle whose binary is not executable opens to *you do not have
    /// permission*, which reads as a signing problem and is not one.
    #[cfg(unix)]
    #[test]
    fn the_binary_inside_is_still_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("mode");
        let exe = fixture_exe(&dir);
        let app = write_bundle(&exe, &dir, None).expect("a bundle");
        let mode = std::fs::metadata(app.join("Contents/MacOS/ipod-emulator"))
            .expect("the copy")
            .permissions()
            .mode();
        assert!(mode & 0o111 != 0, "mode {mode:o} — the copy is not executable");
    }

    /// **A second build replaces the first rather than merging into it.** Otherwise an app reports
    /// itself as this version while holding a file the last version put there.
    #[test]
    fn building_twice_leaves_nothing_of_the_first() {
        let dir = scratch("rebuild");
        let exe = fixture_exe(&dir);
        let app = write_bundle(&exe, &dir, None).expect("the first");
        let stray = app.join("Contents/leftover-from-an-older-build");
        std::fs::write(&stray, b"x").expect("the stray");
        assert!(stray.is_file(), "the control: the stray was not written");

        write_bundle(&exe, &dir, None).expect("the second");
        assert!(!stray.exists(), "the second build merged into the first");
        assert!(app.join("Contents/Info.plist").is_file(), "and took the plist with it");
    }

    /// The plist says what this build is, in both places, and says the two things about the window
    /// that are not defaults: it wants a Retina backing store, and it does not run on 10.x.
    #[test]
    fn the_plist_reports_this_build_and_nothing_else() {
        let p = info_plist("9.9.9");
        assert_eq!(
            p.matches("<string>9.9.9</string>").count(),
            2,
            "CFBundleVersion and CFBundleShortVersionString must both carry it: {p}"
        );
        assert!(p.contains("<key>NSHighResolutionCapable</key>"), "{p}");
        assert!(p.contains("<string>11.0</string>"), "{p}");
        assert!(p.contains("net.siggifly.ipod-emulator"), "{p}");
        // The version is substituted, not hard-coded — the control for the assertion above.
        assert!(!info_plist("0.0.1").contains("9.9.9"));
    }

    /// The bundle a release actually ships carries **this** crate's version, which is the whole
    /// argument for the number coming from the compiler.
    #[test]
    fn the_shipped_plist_carries_the_compiled_version() {
        let dir = scratch("version");
        let exe = fixture_exe(&dir);
        let app = write_bundle(&exe, &dir, None).expect("a bundle");
        let plist = std::fs::read_to_string(app.join("Contents/Info.plist")).expect("the plist");
        assert!(
            plist.contains(&format!("<string>{VERSION}</string>")),
            "the bundle does not report {VERSION}"
        );
    }

    /// **Where a bundle cannot be built, saying so is the feature.** The refusal is what stops a
    /// release script succeeding at producing nothing, and it must be a refusal on exactly the
    /// platforms that have no such format.
    ///
    /// **What each leg of this actually catches, because the two are not the same, and three
    /// mutations were run on a Mac to find out which.** Deleting the `!` from [`unsupported`] is
    /// caught on **both** — macOS starts refusing, Linux stops — and is red here. The other two are
    /// changes only the Linux leg can observe, and both came back **green on a Mac**: pinning the
    /// condition to a constant `false`, and deleting the guard out of [`make_app`] entirely. Neither
    /// is a behaviour change on a platform where the answer was already *supported*. That is written
    /// down rather than papered over — a platform-scoped assertion is worth having and is not worth
    /// mistaking for one that holds everywhere.
    ///
    /// The last assertion is what stops the macOS leg being a pure tautology: it distinguishes the
    /// platform's refusal from the filesystem's, which is the ordering claim — the guard answers
    /// before anything is written.
    #[test]
    fn only_macos_is_asked_for_a_bundle() {
        assert_eq!(
            unsupported().is_none(),
            cfg!(target_os = "macos"),
            "the platform guard and the platform disagree"
        );
        // The sentence itself, on every platform — including the one that never prints it.
        assert!(NOT_MACOS.contains("macOS"), "{NOT_MACOS:?} does not say why");
        assert!(NOT_MACOS.contains("plain binary"), "{NOT_MACOS:?} does not say what to use instead");

        // A path no platform can create a directory under, so both legs get an `Err` — and the two
        // must be distinguishable. If the guard ran *after* the filesystem work, macOS and Linux
        // would both report the path and the refusal would be unreachable.
        let e = make_app(Path::new("/dev/null/impossible"), None)
            .expect_err("nothing can be written under /dev/null");
        assert_eq!(
            e == NOT_MACOS,
            !cfg!(target_os = "macos"),
            "the refusal should be the platform's off macOS and the filesystem's on it: {e:?}"
        );
    }

    /// The icon step, against the picture the release actually ships.
    ///
    /// Written as an equality with `cfg!` rather than as a macOS-only test so that **both** legs
    /// assert something: on macOS that `sips` and `iconutil` are reachable and produce an `.icns`,
    /// and everywhere else that ten subprocesses are not attempted. A plain `assert!(...)` under
    /// `#[cfg(target_os = "macos")]` would leave the Linux leg with no statement at all about a
    /// function it compiles.
    #[test]
    fn the_icon_is_built_where_its_tools_live_and_skipped_where_they_do_not() {
        let dir = scratch("icon");
        let icon = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/media/icon-1024.png"
        ));
        assert!(icon.is_file(), "{} is not there", icon.display());
        assert_eq!(
            try_icon(&icon, &dir),
            cfg!(target_os = "macos"),
            "the icon step and the platform disagree"
        );
        assert_eq!(dir.join("icon.icns").is_file(), cfg!(target_os = "macos"));
    }

    /// A picture that is not there is not a reason to fail, and is not an icon either.
    #[test]
    fn a_missing_icon_is_declined_rather_than_fatal() {
        let dir = scratch("no-icon");
        let exe = fixture_exe(&dir);
        assert!(!try_icon(&dir.join("nothing.png"), &dir));
        // …and the bundle is still built, with everything but the picture.
        let app = write_bundle(&exe, &dir, Some(&dir.join("nothing.png"))).expect("a bundle");
        assert!(app.join("Contents/Info.plist").is_file());
        assert!(!app.join("Contents/Resources/icon.icns").exists());
    }
}
