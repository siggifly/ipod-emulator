// The two ways a file gets into this program from outside — docs/GUI.md §11.4, §16.4, §17 Q3.
//
// Until this file existed there were none. Every `Provide…`, every `Add a dump…` and every
// `Choose a folder…` in the window was drawn disabled saying there was no file picker, which was
// true, and `main::caps()` typed `file_picker`, `drop_target` and `reveal` as `false` literals. So
// the only way to give this program a file was the command line.
//
// **Two routes, and the operator chose both in this order: the drop first.** §17 Q3's own words for
// why the picker is still wanted — *"drag-and-drop is the better route and it is window-wide, but
// 'the only way to give this program a file is to drag it' is not a program"*.
//
// ── What this file is allowed to depend on ───────────────────────────────────────────────────────
//
// `AGENTS.md` §9: `main.rs` is the only file in this crate that touches the **toolkit**, and every
// other file is toolkit-free. `rfd` is not the toolkit — it is the platform's own dialog
// (`NSOpenPanel` on macOS, `IFileOpenDialog` on Windows, the xdg-desktop-portal on Linux) reached
// without Slint, and nothing in this file names a Slint type. Replacing the window would keep every
// line of it.
//
// **`rfd` is here rather than in `main.rs` for a second reason, and it is the one that makes
// `main::caps().file_picker` honest**: the flag is derived from [`PICKER`], which sits a few lines
// from the only `rfd` call in the program. Delete the dependency and this file stops compiling, so
// the claim cannot outlive its mechanism. That is the same guarantee `nav::Page::Devices.slot()`
// gives the `devices_page` cap — ask the thing that would know, never write the answer down twice.
//
// ── What `rfd` cost, measured rather than assumed (§17 Q3) ───────────────────────────────────────
//
// `cargo tree -p ipod-gui --target <t> --prefix none -e normal,build | sed 's/ (\*)//' | sort -u |
// wc -l`, before and after, on rfd 0.17.2:
//
// | target | before | after | delta |
// |---|---|---|---|
// | `aarch64-apple-darwin` | 352 | 353 | **+1** — `rfd` itself |
// | `x86_64-pc-windows-msvc` | 347 | 348 | **+1** — `rfd` itself |
// | `x86_64-unknown-linux-gnu` | 430 | 432 | **+2** — `rfd`, `pollster` |
//
// Everything else rfd asks for is already compiled here: `block2`, `dispatch2`, `objc2 0.6`,
// `objc2-app-kit 0.3`, `objc2-foundation`, `objc2-core-foundation`, `raw-window-handle 0.6` and
// `log` on macOS; `windows-sys` on Windows; `wayland-client`, `wayland-protocols` and
// `wayland-backend` on Linux, through winit.
//
// **And §17 Q3's premise is out of date, which the measurement is what shows.** It says *"`rfd`
// pulls GTK or xdg-desktop-portal on Linux"* and recommends the portal. In rfd 0.17 the portal is
// the **default** and GTK3 is opt-in: `default = ["xdg-portal", "wayland"]`. The alternative was
// measured too — `features = ["gtk3"]` is **449**, or **+19**, dragging in `atk-sys`,
// `cairo-sys-rs`, `gdk-sys`, `gdk-pixbuf-sys`, `gio-sys`, `glib-sys`, `gobject-sys`, `gtk-sys`,
// `pango-sys` and the whole `system-deps` / `toml` build-script stack behind them. So the manifest
// names `xdg-portal` and `wayland` explicitly rather than inheriting them, and the arithmetic that
// chose them is above rather than in somebody's head.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eapp_loader::inspect::{self, Kind, Verdict};
use eapp_loader::settings::{Provenance, Resource, Settings};
use eapp_loader::{firmware, group, nor, si};

use crate::parts::{Group, Wrote};

// ── What this build can do, answered beside the mechanism ────────────────────────────────────────

/// Whether this build can open a file dialog. **Read by `main::caps()`; never typed there.**
///
/// A `const` and not a probe, and the difference from `tooling::can_download` is the question being
/// asked. Whether `curl` is on the computer is a fact **about the computer** and changes per
/// machine; whether a dialog is compiled in is a fact **about the build**, and the one thing that
/// could make it false — the dependency going away — takes [`ask_native`] with it and this file with
/// that.
///
/// It is deliberately not a probe on Linux either. rfd's portal backend asks
/// `org.freedesktop.portal.FileChooser` at open time and falls back to nothing if the portal is
/// absent; a D-Bus round trip at launch to decide whether to grey a control would be a second
/// implementation of what the dialog is about to do, and it would be wrong on any desktop that
/// starts the portal on demand. A portal that refuses is reported through the Rail as a refusal,
/// where a failure belongs.
pub const PICKER: bool = true;

/// Whether this build accepts a dropped file. **Read by `main::caps()`; never typed there.**
///
/// **The tie that keeps this honest is the compiler, not a comment.** [`Landing`] is fed by exactly
/// one caller — `main.rs`'s single `on_winit_window_event` registration, per §16.4 — and it is
/// `pub` to nothing outside this crate. Delete the three winit arms and every method on `Landing`
/// is dead code, which this tree treats as an error (`-D warnings`). So the flag cannot go on
/// claiming a route that has been removed.
///
/// `the_winit_hook_handles_all_three_of_winits_file_events` is the other half: it reads `main.rs`'s
/// own source for `DroppedFile`, `HoveredFile` and `HoveredFileCancelled`, the way T-7 counts the
/// registrations.
pub const DROPS: bool = true;

/// The command that shows a file in the platform's file manager, or `None` where there is none.
///
/// **Three different tools and three different arguments**, which is why this is a `match` and not
/// a name: macOS reveals with `open -R <file>` (`-R, --reveal  Selects in the Finder instead of
/// opening`), Windows with `explorer /select,<file>`, and Linux has no select at all — `xdg-open`
/// opens the **containing directory**, which is the closest honest thing and is what
/// [`reveal`] does there.
///
/// `None` on every other platform rather than a guess.
const fn reveal_tool() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("open")
    } else if cfg!(target_os = "windows") {
        Some("explorer")
    } else if cfg!(target_os = "linux") {
        Some("xdg-open")
    } else {
        None
    }
}

/// Whether this **computer** can show a file in a file manager. Read by `main::caps()`.
///
/// **A machine rule, not a project state** — the distinction §9.4 draws, and the reason this is a
/// function where [`PICKER`] is a `const`. A headless Linux box with no `xdg-open` genuinely cannot
/// do this and no amount of work on this program fixes it, which is exactly what `Next::Retry`
/// says about `curl`.
///
/// The two always-present tools are asked for by **path** rather than by running them, and that is
/// not a shortcut: `open --version` on macOS exits **1** with `unrecognized option`, so
/// `tooling::have("open")` — which is `--version` and an exit status — reports the tool that ships
/// with every macOS since NeXT as absent. `explorer.exe` has the same shape. `xdg-open` does
/// support `--version`, and on Linux it is genuinely optional, so that one is measured the way
/// `curl` is.
///
/// Measured once per launch, in `main::caps()`, for the same reason `can_download` is: this spawns
/// a process on the Linux arm and a display refresh must not.
pub fn can_reveal() -> bool {
    match reveal_tool() {
        Some("open") => Path::new("/usr/bin/open").is_file(),
        Some("explorer") => true,
        Some(t) => eapp_loader::tooling::have(t),
        None => false,
    }
}

/// Show `p` in the platform's file manager.
///
/// **Spawned and not waited on.** `open -R` returns immediately, `explorer` does not report through
/// its exit status at all, and `xdg-open` may block for as long as the file manager takes to start
/// — which on a cold GNOME session is seconds, on the UI thread, inside a callback. So a success
/// here means the request was made, not that a window appeared.
///
/// **Silent when it works, and that is deliberate rather than an omission of `AGENTS.md` §7.** The
/// thing that happened is a file manager coming to the front, which is the loudest feedback
/// anything in this program produces; a Rail line saying so would be a note per press for an act
/// whose whole output is visible. A failure is not silent — this program cannot show what the OS
/// refused to.
///
/// **`Wrote::Nothing`, always.** Nothing about the library moved, and a save on this press would
/// rewrite the operator's settings file — which `Settings::render` regenerates whole, taking any
/// comment they added with it — for having looked at a folder.
pub fn reveal(p: &Path) -> Result<Wrote, String> {
    let Some(tool) = reveal_tool().filter(|_| can_reveal()) else {
        return Err("this computer has no file manager to show it in".into());
    };
    let mut cmd = std::process::Command::new(tool);
    match tool {
        "open" => {
            cmd.arg("-R").arg(p);
        }
        "explorer" => {
            // One argument, comma-attached, no space: `explorer /select,C:\dir\file`. A space makes
            // Explorer open the user's Documents folder instead, silently.
            let mut arg = std::ffi::OsString::from("/select,");
            arg.push(p);
            cmd.arg(arg);
        }
        // No select on Linux. The directory is the honest answer, and naming it in the sentence
        // below is what stops it reading as a failure to highlight the file.
        _ => {
            cmd.arg(p.parent().unwrap_or(p));
        }
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    match cmd.spawn() {
        Ok(_) => Ok(Wrote::Nothing),
        Err(e) => Err(format!("{tool} could not be started: {e}")),
    }
}

// ── The picker ───────────────────────────────────────────────────────────────────────────────────

/// What the program is asking the operator for.
///
/// **There are deliberately no extension filters on any of these**, and that is §11.4's rule rather
/// than laziness: *identification is by content, never by extension*. A dump named `rom.bin`, one
/// named `internal_rom_000000-0FFFFF`, and an `.ipsw` a browser renamed to `iPod_25.1.3(1).zip` are
/// all files this program identifies correctly and a filter would hide. The dialog offers
/// everything and [`inspect::classify`] decides, which is the same order the drop route uses.
///
/// **And there is deliberately no `Folder`**, which is the one variant §17 Q3 asks for by name:
/// *"`Provide…`, `Add a dump…` and `Choose a folder…` all need one."* `rfd::FileDialog::pick_folder`
/// exists and would have been one line — and the answer would have had nowhere to go.
/// `Choose a folder…` means *put this program's files somewhere else*, which is
/// `settings::data_dir()` reading `IPOD_EMULATOR_DATA` at launch; `rail::Next::unwired` carries the
/// whole argument. A variant nothing can ask for is a control nobody wanted, which is why
/// `parts::RowAction::Rename` was deleted rather than built, and this is the same deletion made
/// before the variant had a chance to sit here unconstructed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ask {
    /// §11.4's `Add a dump…` and `Provide…`, for the group whose verb was pressed.
    Part(Group),
    /// §9.3's `Provide a file…`, which hangs off a **failure** and so knows no group. What arrives
    /// is filed by its contents.
    Any,
}

impl Ask {
    /// The dialog's own title. It says what will happen to the file, because a dialog titled
    /// `Open` is the one thing on screen that is not about this program.
    pub fn title(self) -> String {
        match self {
            Ask::Part(Group::Ipods) => "Add a boot ROM dump".into(),
            Ask::Part(g) => format!("Provide {}", g.heading().to_lowercase()),
            Ask::Any => "Provide a file".into(),
        }
    }
}

/// **The platform's own file UI** — the dialog that answers with a path, and the file manager that
/// shows one. *Not* a command interpreter; nothing here runs a shell.
///
/// **A value rather than two bare calls**, so the two functions in this crate that put something on
/// the operator's screen are reached through something the suite can stand in for. That is not
/// tidiness: `every_next_step_this_build_offers_is_wired_to_something` presses every live control
/// this build draws, `Next::Reveal` is one of them now, and a suite that pressed it for real would
/// open a Finder window per run on the machine somebody is working at. The picker is worse — an
/// `NSOpenPanel` is modal, so it would hang the run.
///
/// [`Shell::Answering`] is `#[cfg(test)]`: a release build cannot construct one, so there is no
/// path by which the shipping program asks anything but the platform.
pub enum Shell {
    /// The platform's own — `rfd` for the dialog, `open` / `explorer` / `xdg-open` for the reveal.
    Native,
    /// A queue of canned answers and a log of what was revealed. An empty queue is a cancelled
    /// dialog, which is the state most presses are made in.
    #[cfg(test)]
    Answering {
        answers: std::cell::RefCell<std::collections::VecDeque<PathBuf>>,
        revealed: std::cell::RefCell<Vec<PathBuf>>,
    },
}

impl Shell {
    /// Ask for a path, and hand back what the operator chose. `None` is a cancel, which is not a
    /// failure and says nothing.
    pub fn pick(&self, ask: Ask) -> Option<PathBuf> {
        match self {
            Shell::Native => ask_native(ask),
            #[cfg(test)]
            Shell::Answering { answers, .. } => answers.borrow_mut().pop_front(),
        }
    }

    /// Show `p` in the platform's file manager. See [`reveal`] for what that means per platform and
    /// for why a success here is silent.
    pub fn reveal(&self, p: &Path) -> Result<Wrote, String> {
        match self {
            Shell::Native => reveal(p),
            #[cfg(test)]
            Shell::Answering { revealed, .. } => {
                revealed.borrow_mut().push(p.to_path_buf());
                Ok(Wrote::Nothing)
            }
        }
    }

    /// A shell holding these answers, in order, and revealing onto a list instead of a screen.
    #[cfg(test)]
    pub fn answering<I: IntoIterator<Item = PathBuf>>(paths: I) -> Shell {
        Shell::Answering {
            answers: std::cell::RefCell::new(paths.into_iter().collect()),
            revealed: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// What [`Shell::reveal`] was asked to show, for a test that pressed a control.
    #[cfg(test)]
    pub fn revealed(&self) -> Vec<PathBuf> {
        match self {
            Shell::Native => Vec::new(),
            Shell::Answering { revealed, .. } => revealed.borrow().clone(),
        }
    }
}

/// **The only `rfd` call in this program**, and the only line of it that opens anything.
///
/// It is separated from [`Shell::pick`] so that everything above and below it is testable and this
/// is not: there is no logic here to get wrong, which is the point of keeping it this thin.
fn ask_native(ask: Ask) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(ask.title()).pick_file()
}

// ── Filing, which is what both routes end in ─────────────────────────────────────────────────────

/// Where a part of this kind belongs, as the model's own type.
///
/// `None` for the two groups that hold no [`Resource`]: a disk is what ingredients are combined
/// *into* and goes through `Settings::file_disk`, and a snapshot is a device's paused state that
/// nothing outside this program can hand us.
fn resource_for(g: Group, p: &Path) -> Option<Resource> {
    match g {
        Group::Ipods => Some(Resource::Firmware(nor::Source::File(p.to_path_buf()))),
        Group::Firmware => Some(Resource::Installer(p.to_path_buf())),
        Group::Bootloaders => Some(Resource::Bootloader(p.to_path_buf())),
        Group::Software => Some(Resource::Software(p.to_path_buf())),
        Group::Disks | Group::Snapshots => None,
    }
}

/// Which group a file's **contents** put it in. `None` when nothing recognises it.
///
/// The one mapping from [`inspect::Kind`] to §11.4's six groups, so the drop route and the
/// contents-disagree note under the picker cannot answer it two ways.
pub fn group_of(k: Kind) -> Option<Group> {
    match k {
        Kind::Rom => Some(Group::Ipods),
        Kind::Ipsw => Some(Group::Firmware),
        // §11.4: a bootloader *goes in the firmware partition, which holds exactly one thing*, and
        // that is what a verified `.ipod` image is — `rockbox.ipod`, `ipodloader2`. It is the kind
        // whose own doc names both.
        Kind::Os => Some(Group::Bootloaders),
        Kind::OsBundle => Some(Group::Software),
        Kind::Disk => Some(Group::Disks),
        Kind::Unknown => None,
    }
}

/// The name to file `p` under: its stem, which is what every other filing route in this program
/// uses.
fn suggest(p: &Path) -> String {
    p.file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Put `p` in `g`, and say what happened.
///
/// **The group is the operator's own statement and it wins**, which is the one place this program
/// files by something other than contents. Pressing `Provide…` under **Bootloaders** is a person
/// saying *this is a bootloader*; overruling that from a size class would be the window telling
/// somebody they are wrong about their own file. What the contents say is **reported** when the two
/// disagree — the sentence names both — so a misfiled part is visible immediately rather than at
/// the next boot.
///
/// **Nothing is copied.** `file_away` and `file_disk` record the path; §11.4's `used by N` is the
/// reference-not-copy property, and `Remove` never deletes the file behind a part.
pub fn provide(s: &mut Settings, g: Group, p: &Path) -> Result<(Wrote, String), String> {
    let meta = std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is a folder, not a file", p.display()));
    }
    let said = match group_of(inspect::classify(p)) {
        Some(other) if other != g => format!(
            " — its contents read as {}, not {}",
            other.heading().to_lowercase(),
            g.heading().to_lowercase()
        ),
        // Nothing recognises it, which §11.4 allows and reports: *a 4.2 MB unknown reads `no
        // recognisable header — named, not filed`*. Here it IS filed, because somebody said which
        // group it belongs to, so the sentence says only that nothing corroborated them.
        None => " — nothing in it identifies what it is".into(),
        Some(_) => String::new(),
    };
    if g == Group::Snapshots {
        return Err("a snapshot is a machine this program paused; nothing outside it makes one".into());
    }
    let name = file_into(s, g, p);
    Ok((
        Wrote::Library,
        format!(
            "filed {} as {name} in {}, referenced in place{said}",
            si(meta.len()),
            g.heading()
        ),
    ))
}

/// Put `p` in `g` and hand back the name it got. **The one filing call**, so the picker's route
/// and the drop's route cannot record two different provenances for one act.
///
/// `Provided`: *the operator handed us this file; nothing is known about it beyond that*. Never
/// `Dumped` — a 1 MiB file that parses as a NOR may be one this program synthesised and somebody
/// exported, and claiming it was read off a real iPod is a fact we do not have.
fn file_into(s: &mut Settings, g: Group, p: &Path) -> String {
    match resource_for(g, p) {
        Some(r) => s.file_away(r, &suggest(p), Some(Provenance::Provided)),
        None => s.file_disk(p.to_path_buf(), &suggest(p)),
    }
}

/// File one file by its **contents**, composing nothing.
///
/// §9.3's `Provide a file…`, which hangs off a **failure** — a part that is missing, a bundle that
/// would not verify — and so has no group to file into. Nobody said what this is, so
/// [`inspect::classify`] decides, exactly as it does for a drop.
///
/// **It composes no device even when the file is a ROM**, and that is the difference from [`land`]:
/// this press is a person answering *the thing you could not find is here*, and answering it by
/// minting a second iPod on their bench would be the window doing something nobody asked for.
pub fn file_one(s: &mut Settings, p: &Path) -> Result<(Wrote, String), String> {
    let meta = std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is a folder, not a file", p.display()));
    }
    let Some(g) = group_of(inspect::classify(p)) else {
        // §11.4's own words for it, and it is not a failure: the file is named, on the Rail, and
        // not in the library.
        return Ok((
            Wrote::Nothing,
            format!("{} — no recognisable header, so it was named and not filed", p.display()),
        ));
    };
    let name = file_into(s, g, p);
    Ok((
        Wrote::Library,
        format!("filed {} as {name} in {}", si(meta.len()), g.heading()),
    ))
}

// ── The drop ─────────────────────────────────────────────────────────────────────────────────────

/// §11.4 rule 1: **all `DroppedFile` events within this window are one drop.**
///
/// winit delivers one event per file, carrying no cursor position and **with no event that says
/// the drop is over** (§16.4). So nothing in the stream marks the boundary between one drop of
/// eight files and eight drops of one, and the program has to draw it. 150 ms is far longer than
/// the microseconds eight events take to arrive together and far shorter than the pause between
/// two deliberate drags.
pub const WINDOW: Duration = Duration::from_millis(150);

/// One of winit's three file events, decoded off the platform type.
///
/// **The whole of what `main.rs`'s winit hook does with a drag**, so that everything from here on
/// is reachable from a test: the testing backend has no winit window, `WinitWindowAccessor::
/// with_winit_window` answers `None` there, and a route that started at a `WindowEvent` would be
/// drivable by nothing without a display. `Wiring::files` hands this closure back for the same
/// reason `Wiring::tick` is handed back.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Event {
    /// `WindowEvent::HoveredFile` — one per file, before any of them lands.
    Hovered(PathBuf),
    /// `WindowEvent::HoveredFileCancelled` — the drag left the window.
    Cancelled,
    /// `WindowEvent::DroppedFile` — one per file, and nothing after it says the drop is over.
    Dropped(PathBuf),
}

/// A drag over the window, and the drop it becomes.
///
/// **Window-wide, because winit's file events carry no cursor position** — which makes §11.4's
/// *"there is no wrong target"* the only implementable design rather than merely a good one
/// (§16.4).
#[derive(Default)]
pub struct Landing {
    /// The files winit has said are over the window, from `HoveredFile`.
    hovering: Vec<PathBuf>,
    /// When the open coalescing window started, or `None` when no drop is in flight.
    opened: Option<Instant>,
    /// The files that have landed inside it.
    batch: Vec<PathBuf>,
}

impl Landing {
    pub fn new() -> Landing {
        Landing::default()
    }

    /// winit's `HoveredFile`. One per file, before any of them lands.
    pub fn hovering(&mut self, p: PathBuf) {
        self.hovering.push(p);
    }

    /// winit's `HoveredFileCancelled` — the drag left the window.
    pub fn hover_cancelled(&mut self) {
        self.hovering.clear();
    }

    /// What the shelf draws while a drag is over the window. `None` when there is no drag.
    pub fn band(&self) -> Option<Band> {
        (!self.hovering.is_empty()).then(|| band(&self.hovering))
    }

    /// winit's `DroppedFile`.
    ///
    /// Hands back a batch **only when this arrival fell outside the open window**, which closes it:
    /// the eight events of one drop return `None` eight times and the batch comes out of
    /// [`Landing::settled`] 150 ms later. A ninth file arriving a second after the first eight is a
    /// second drop, and closing the first here is what stops it joining one.
    #[must_use]
    pub fn dropped(&mut self, p: PathBuf, at: Instant) -> Option<Vec<PathBuf>> {
        // The hover is over the moment anything lands: winit sends no `HoveredFileCancelled` after
        // a drop, so a band left standing here would be drawn over the shelf for ever.
        self.hovering.clear();
        let closed = match self.opened {
            Some(t) if at.duration_since(t) >= WINDOW => Some(std::mem::take(&mut self.batch)),
            _ => None,
        };
        if closed.is_some() || self.opened.is_none() {
            self.opened = Some(at);
        }
        self.batch.push(p);
        closed
    }

    /// The timer's half. `Some` once the open window has run out.
    ///
    /// A timer rather than a second event, because there is no second event: §16.4 measured that
    /// winit has nothing that says a drop is over.
    #[must_use]
    pub fn settled(&mut self, now: Instant) -> Option<Vec<PathBuf>> {
        let t = self.opened?;
        if now.duration_since(t) < WINDOW {
            return None;
        }
        self.opened = None;
        Some(std::mem::take(&mut self.batch))
    }

}

// ── The band (§11.4, and it is drawn over the shelf's rows 1 and 2) ──────────────────────────────

/// What the shelf says while a drag is over the window.
///
/// Rows 1 and 2, and row 3 — `write_target()`, the one line standing between an afternoon and
/// somebody's only image of an iPod they own — is left exactly where it was. §11.4's picture has it
/// still reading `works on a copy of my-5.5g.img` under the band.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Band {
    /// Row 1's leading slot: the **count**, which is the fact that survives any elide.
    pub count: String,
    /// Row 2: at most two identifications, then `+N more`. **A list rather than a sentence**, and
    /// that is what makes rule 2 assertable: an identification may itself contain the separator —
    /// `no recognisable header — named, not filed` does — so a test counting separators in a joined
    /// string would have been counting the punctuation inside the parts as well as between them,
    /// and it read four where the rule says two. [`Band::what`] joins; nothing else does.
    pub parts: Vec<String>,
}

impl Band {
    /// The parts as row 2 draws them: one eliding `Text`, one string.
    pub fn what(&self) -> String {
        self.parts.join(SEPARATOR)
    }
}

/// Row 1's trailing slot, which does not vary.
pub const LET_GO: &str = "let go to file these";

/// What goes between two identifications.
///
/// **§11.4's picture writes `·` and this program cannot.** `geometry`'s closed glyph set is the
/// mechanism: Slint takes one `font-family` per element with no fallback list and a missing glyph
/// falls to `.notdef`, so the set is short and `·` is not in it — §6.7's answer for a symbol is
/// that it is *drawn as a `Path`*, which is what `ui/bench.slint`'s own `Dot` is and what the shelf
/// MENU list uses. A drawn separator is unavailable here for a reason of layout rather than of
/// taste: row 2's leading slot is ONE eliding `Text` at `horizontal-stretch: 1`, and a row of five
/// elements with two Paths in it does not elide — it shrinks by stretch, which is the defect
/// `PARTS_VERB_W` was raised for. So the em dash separates the identifications and a comma
/// separates the clauses inside one, and both are glyphs the set already carries.
const SEPARATOR: &str = " — ";

/// §11.4 rule 2: **a count and at most two identifications, then `+N more`.**
///
/// The rule is a measurement rather than a preference. The band was written for `Three files` with
/// three identifications on one 20 px line; eight do not fit, **and a band that reflows is a band
/// that moves** — principle 1's prohibition, on the one surface that is drawn while somebody is
/// holding a mouse button down.
pub fn band(files: &[PathBuf]) -> Band {
    /// **Two, and it is the number the row holds** — §11.4 measured three on one 20 px line and
    /// found eight do not fit.
    const AT_MOST: usize = 2;
    let mut parts: Vec<String> = files.iter().take(AT_MOST).map(|p| identify(p)).collect();
    // Off `parts.len()` rather than off `AT_MOST`, so the count and the list cannot disagree: with
    // the two written out separately, widening the list left `+6 more` under eight identifications
    // of which three were drawn.
    if files.len() > parts.len() {
        parts.push(format!("+{} more", files.len() - parts.len()));
    }
    Band {
        count: count_of(files.len()),
        parts,
    }
}

/// `One file`, `Eight files`, `11 files`.
///
/// **Words to ten and digits after**, which is the same line every style guide draws and the same
/// one §11.4's own picture draws: it writes `Eight files`, not `8 files`. Above ten a word is
/// longer to read than the number it spells, and this slot is one elided line.
fn count_of(n: usize) -> String {
    const WORDS: [&str; 11] = [
        "No", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten",
    ];
    let count = match WORDS.get(n) {
        Some(w) => (*w).to_string(),
        None => n.to_string(),
    };
    format!("{count} file{}", if n == 1 { "" } else { "s" })
}

/// What one file is, judged by its contents — **§11.4 rule 4, and it is the dangerous one**.
///
/// `inspect::Kind::Rom` is *"exactly 1 MiB"* and nothing else. A 1 048 576-byte JPEG classifies as
/// `Rom`, and a band that rendered the class drew `boot ROM · 5.5G · 1 048 576 B` — an affirmative
/// claim, with a **generation attached**, for a photograph. The real verdicts (`word 0 is not an
/// ARM branch`, `no flsh directory at 0xffe00`) existed only inside the Parts row's `Expand`, which
/// somebody has to go and open.
///
/// **A size class is a hypothesis and the band is where it gets tested**, because the band is the
/// last moment before the file acquires a name. So a ROM-sized file is put through
/// [`inspect::flash`] here — the same verdict the expanded row shows — and only a `Verdict::Good`
/// is allowed to say *boot ROM*.
///
/// Hashing is deliberately **not** done: §11.4 size-gates identification during a hover, because a
/// SHA-256 of the 101 MB ZeroSlackr archive on the UI thread while a drag is in progress is a
/// frozen window. `firmware::identify` is the one exception and it is bounded by the catalogue's
/// own lengths — it hashes only a file whose size matches a release exactly, which is usually one
/// candidate and often none.
pub fn identify(p: &Path) -> String {
    let len = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    match inspect::classify(p) {
        Kind::Rom => match inspect::flash(p) {
            Verdict::Good(_) => format!("boot ROM, {} B", group(len)),
            // The verdict's own first sentence, which is the clause that says what is wrong. The
            // rest of it is a paragraph and this is a 20 px line.
            v => format!("{} B, not a boot ROM, {}", group(len), first_clause(v.text())),
        },
        Kind::Ipsw => match std::fs::read(p).map(|d| firmware::identify(&d)) {
            Ok(firmware::Provenance::Apple(r)) => format!("Apple firmware, {}", r.model),
            // Allowed, and deliberately so — §11.4: *modified firmware is a legitimate reason to
            // want an emulator, and it is reported so you know, not to stop you.*
            _ => format!("Apple firmware, unrecognised, {}", si(len)),
        },
        Kind::Os => match inspect::os_checksum(p) {
            Some((model, body)) => format!("bootloader, {model}, {} B", group(body as u64)),
            None => format!("bootloader, {}", si(len)),
        },
        Kind::OsBundle => format!("software, {}", si(len)),
        Kind::Disk => match inspect::disk(p) {
            Verdict::Good(_) => format!("drive image, {}", si(len)),
            v => format!("{}, not a drive, {}", si(len), first_clause(v.text())),
        },
        // §11.4's contrast case, worded as it words it. `named, not filed` is the whole of what
        // happens to it: the Rail records that a file arrived and where it is, and the library does
        // not grow a row for something nothing can identify.
        Kind::Unknown => format!("{}, no recognisable header — named, not filed", si(len)),
    }
}

/// The **band's bound on one identification**, in characters, with the ellipsis included.
///
/// **A character count is not a width and this file knows it** — `the_group_verb_column_holds_every_
/// verb_this_page_draws` found `Add a dump…` wider than `Synthesise…` at equal length. So this is a
/// bound and not a fit: row 2 is one eliding `Text` and will elide whatever it is given. What the
/// bound buys is *where* it elides. Unbounded, one long verdict swallowed the second identification
/// and `+N more` with it, and rule 2's whole structure — count, two, then a number — was invisible
/// on exactly the drop it was written for.
const CLAUSE: usize = 64;

/// The half of a verdict the band draws, with its full stop taken off.
///
/// A verdict is written to be read in an expanded row and runs to a paragraph; the band has one
/// line. Two constructions, both of them `inspect::flash`'s own:
///
///   - `word 0 is {x}, which is not an ARM branch. A PP502x fetches its reset vector at …` — the
///     **head** is the evidence and the tail is the verdict, and the band has already said the
///     verdict: *not a boot ROM* leads the identification. So `, which ` ends it, and what is drawn
///     is `word 0 is 0xe0ffd8ff` — the number a person would quote in a report.
///   - `1 MiB and a plausible reset vector, but no `flsh` image directory at 0xffe00. The 5G/5.5G
///     NOR carries one, naming …` — no `, which `, so the first sentence stands and [`CLAUSE`]
///     bounds it.
///
/// **§11.4's own picture compresses this by hand** — it writes `word 0 is not an ARM branch`, which
/// is the sentence with its subject and its predicate joined across the comma. That is a rewrite no
/// rule produces, and inventing one would be a program guessing at English. Taking the evidence is
/// the compression that follows from where the split already is.
fn first_clause(text: &str) -> String {
    let one = text.lines().next().unwrap_or(text);
    let sentence = one.find(". ").map(|i| i + 1).unwrap_or(one.len());
    let evidence = one.find(", which ").unwrap_or(one.len());
    let end = sentence.min(evidence);
    clipped(one[..end].trim_end_matches('.').trim())
}

/// `text`, bounded at [`CLAUSE`] with the ellipsis the closed glyph set carries.
fn clipped(text: &str) -> String {
    if text.chars().count() <= CLAUSE {
        return text.to_string();
    }
    let mut out: String = text.chars().take(CLAUSE - 1).collect();
    out.push('…');
    out
}

// ── What a settled drop does ─────────────────────────────────────────────────────────────────────

/// The result of filing one drop.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Landed {
    /// Whether the library moved, so `main.rs` knows whether to save. A drop of one unidentifiable
    /// file moves nothing and must not rewrite the operator's settings file.
    pub wrote: Wrote,
    /// The Rail's own line: what happened, and where to go next when nothing composed.
    pub note: String,
    /// The device this drop composed, by name. `None` when it composed none — which is every
    /// ambiguous drop, and every drop with no ROM in it.
    pub device: Option<String>,
}

/// §11.4 rule 3: **ambiguity files and does not compose.**
///
/// A drop carrying more than one ROM, or more than one `.ipsw`, files everything into Parts and
/// creates **no** device. The old promise — *"a ROM and an `.ipsw` dropped together, in either
/// order, produce one device"* — is undefined with two of each: there is no answer to *which* iPod
/// the device is made of that is not a guess, and guessing here writes somebody's serial number
/// into a machine they did not ask for.
///
/// So the ambiguous drop gets the Rail line §11.4 wrote for it — `eight files filed; make a device
/// from them in Devices` — and the parts are all there, named, for the Composer to pick from. The
/// `›` §11.4 puts on the end of that sentence is not typed: `geometry`'s closed glyph set holds
/// three characters and a chevron is not one of them, because §6.7 draws a chevron as a `Path` and
/// the Rail's line is a `Text`.
fn ambiguous(kinds: &[Kind]) -> bool {
    let n = |k: Kind| kinds.iter().filter(|x| **x == k).count();
    n(Kind::Rom) > 1 || n(Kind::Ipsw) > 1
}

/// File a settled drop, and compose a device from it when it is unambiguous.
///
/// **One ROM composes; two do not.** The device is made of the one iPod in the drop, and the
/// `.ipsw` beside it is filed for the drive that has still to be built — which is the *unfinished*
/// state `Composer::commit` already writes when a recipe names a drive nobody has built yet, not a
/// broken one.
///
/// A drop with no ROM in it composes nothing either, and that is not a refusal: a device cannot be
/// made without one (§11.4's *"a device names one and cannot be made without it"*), so the parts
/// are filed and the Rail says where they went.
pub fn land(s: &mut Settings, files: &[PathBuf]) -> Landed {
    let kinds: Vec<Kind> = files.iter().map(|p| inspect::classify(p)).collect();
    let mut wrote = Wrote::Nothing;
    let mut named: Vec<String> = Vec::new();
    let mut unfiled = 0usize;

    for (p, k) in files.iter().zip(&kinds) {
        let Some(g) = group_of(*k) else {
            // §11.4: *named, not filed*. It is on the Rail and it is not in the library.
            unfiled += 1;
            named.push(format!("{} — nothing identifies it", p.display()));
            continue;
        };
        let name = file_into(s, g, p);
        wrote = Wrote::Library;
        named.push(format!("{name} in {}", g.heading()));
    }

    let filed = files.len() - unfiled;
    if ambiguous(&kinds) {
        return Landed {
            wrote,
            // §11.4's own sentence, and the names go after it so a long list elides where the point
            // does not.
            note: format!(
                "{filed} filed; make a device from them in Devices — {}",
                named.join(", ")
            ),
            device: None,
        };
    }

    let rom = files
        .iter()
        .zip(&kinds)
        .find(|(_, k)| **k == Kind::Rom)
        .map(|(p, _)| p.clone());
    let Some(rom) = rom else {
        return Landed {
            wrote,
            note: match filed {
                0 => format!("nothing was filed — {}", named.join(", ")),
                _ => format!("{filed} filed — {}", named.join(", ")),
            },
            device: None,
        };
    };

    // **The device, and every field of it comes off the ROM.** `remember_as` files the live source
    // by value first thing, so the resource this points at is the one the loop above filed rather
    // than a second entry for one iPod.
    let src = nor::Source::File(rom.clone());
    let name = device_name(s, &src, &rom);
    s.chassis = src.model().map(|m| m.colour());
    s.nor = src;
    // A drive is **built** from an `.ipsw` and is not one, so there is nothing to point at yet.
    // `Absent`'s own vocabulary calls this unfinished rather than broken.
    s.disk = None;
    s.remember_as(&name);
    Landed {
        wrote: Wrote::Library,
        note: format!("{filed} filed; composed {name} — {}", named.join(", ")),
        device: Some(name),
    }
}

/// A name for a device nobody typed one for.
///
/// **The iPod's own description when the ROM resolves one** — `Black 5.5G`, which is the row
/// §11.4's own picture draws — and the file's stem when it does not, because inventing a
/// generation for a ROM whose `Mod#` is not in the table is the exact thing §11.4 forbids two
/// paragraphs later.
///
/// Uniquified against the devices, never against the resources: `remember_as` **replaces** a device
/// of the same name outright, so two ROMs of one model dropped an hour apart would have the second
/// silently eat the first.
fn device_name(s: &Settings, src: &nor::Source, p: &Path) -> String {
    let base = match src.model() {
        Some(m) => format!("{} {}", m.colour().label(), m.generation.label()),
        None => {
            let stem = suggest(p);
            if stem.is_empty() {
                "Dropped iPod".into()
            } else {
                stem
            }
        }
    };
    let mut name = base.clone();
    let mut n = 2;
    while s.devices.iter().any(|d| d.name == name) {
        name = format!("{base} ({n})");
        n += 1;
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of exactly `len` bytes whose first four are `head`, in a directory of this test's own.
    fn a_file(dir: &Path, name: &str, len: usize, head: &[u8]) -> PathBuf {
        std::fs::create_dir_all(dir).expect("the directory");
        let mut body = vec![0u8; len];
        body[..head.len().min(len)].copy_from_slice(&head[..head.len().min(len)]);
        let at = dir.join(name);
        std::fs::write(&at, &body).expect("the file");
        at
    }

    fn scratch(name: &str) -> PathBuf {
        let at = std::env::temp_dir().join(format!("ipod-drops-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&at);
        std::fs::create_dir_all(&at).expect("the scratch directory");
        at
    }

    /// A 1 MiB file that is **not** a ROM: the size class is right and word 0 is not an ARM branch.
    fn a_rom_sized_photograph(dir: &Path, name: &str) -> PathBuf {
        a_file(dir, name, inspect::NOR_LEN as usize, &[0xff, 0xd8, 0xff, 0xe0])
    }

    /// A 1 MiB file whose word 0 **is** an ARM branch — far enough into `flash`'s checks to be a
    /// different verdict from the one above, and still not a real dump.
    fn a_rom_sized_branch(dir: &Path, name: &str) -> PathBuf {
        a_file(dir, name, inspect::NOR_LEN as usize, &[0xfe, 0x1f, 0x00, 0xea])
    }

    fn an_ipsw(dir: &Path, name: &str) -> PathBuf {
        a_file(dir, name, 4096, b"PK\x03\x04")
    }

    /// **§11.4 rule 1.** All `DroppedFile` events within 150 ms are one drop; one that arrives
    /// after it is a second.
    ///
    /// **Proved red before it was believed**: with `WINDOW` set to `Duration::ZERO` the first
    /// assertion below fails with `eight events became 8 drops`, which is the defect the rule is
    /// for — winit delivers one event per file with nothing marking the boundary, so a program with
    /// no window of its own cannot tell one drop of eight from eight drops of one (§16.4).
    #[test]
    fn eight_files_dropped_together_are_one_drop_and_a_ninth_later_is_another() {
        let t0 = Instant::now();
        let mut l = Landing::new();
        let mut closed: Vec<Vec<PathBuf>> = Vec::new();

        for i in 0..8 {
            // Microseconds apart, which is how eight events off one drag actually arrive.
            let at = t0 + Duration::from_micros(i * 40);
            if let Some(b) = l.dropped(PathBuf::from(format!("/tmp/f{i}")), at) {
                closed.push(b);
            }
        }
        assert!(
            closed.is_empty(),
            "eight events became {} drops before the window ran out; nothing in winit's stream \
             marks the boundary, so this window is the only thing that can",
            closed.len() + 1
        );
        assert_eq!(l.settled(t0 + Duration::from_millis(149)), None, "149 ms is inside the window");
        let one = l.settled(t0 + WINDOW).expect("the window has run out");
        assert_eq!(one.len(), 8, "the drop of eight came out as {} files", one.len());
        assert_eq!(
            l.settled(t0 + Duration::from_secs(5)),
            None,
            "the batch came out and the window is still open, so the timer's next call would file \
             an empty drop"
        );

        // …and a ninth file a second later is its own drop, closing nothing because nothing is open.
        let mut l = Landing::new();
        assert_eq!(l.dropped(PathBuf::from("/tmp/a"), t0), None);
        let second = l
            .dropped(PathBuf::from("/tmp/b"), t0 + Duration::from_secs(1))
            .expect("the first drop closed when the second arrived");
        assert_eq!(second, vec![PathBuf::from("/tmp/a")], "the two drops were coalesced into one");
        assert_eq!(
            l.settled(t0 + Duration::from_secs(2)).as_deref(),
            Some(&[PathBuf::from("/tmp/b")][..]),
            "the second drop did not come out on its own"
        );
    }

    /// **§11.4 rule 2.** The count, at most two identifications, then `+N more`.
    #[test]
    fn the_band_shows_a_count_two_identifications_and_the_rest_as_a_number() {
        let dir = scratch("band");
        let files: Vec<PathBuf> = (0..8)
            .map(|i| a_file(&dir, &format!("f{i}.bin"), 4096, b"\0\0\0\0"))
            .collect();
        let b = band(&files);
        assert_eq!(b.count, "Eight files");
        assert_eq!(
            b.parts.len(),
            3,
            "the band drew {:?}, which is not two identifications and a count",
            b.parts
        );
        assert_eq!(b.parts[2], "+6 more", "the rest is not drawn as a number");
        assert!(b.what().ends_with("+6 more"), "{:?} does not end in it", b.what());

        // The two ends of the rule, because "at most two" is a rule about a maximum and a minimum.
        assert_eq!(band(&files[..1]).count, "One file");
        assert_eq!(band(&files[..1]).parts.len(), 1, "one file drew more than one part");
        assert_eq!(band(&files[..2]).parts.len(), 2, "two files drew a `+N more`");
        assert_eq!(band(&files[..3]).parts[2], "+1 more");
        assert_eq!(count_of(11), "11 files", "eleven is drawn as a word");
    }

    /// **§11.4 rule 4.** A ROM-sized file shows `inspect::flash`'s verdict, never `inspect::Kind`.
    ///
    /// The defect this is for, in its exact original shape: a 1 048 576-byte JPEG classifies as
    /// `Kind::Rom`, and a band drawn off the class rendered `boot ROM · 5.5G · 1 048 576 B` — an
    /// affirmative claim, with a generation attached, for a photograph.
    #[test]
    fn a_rom_sized_file_that_is_not_a_rom_is_not_called_one() {
        let dir = scratch("verdict");
        let photo = a_rom_sized_photograph(&dir, "holiday.jpg");
        assert_eq!(
            inspect::classify(&photo),
            Kind::Rom,
            "the fixture is not exercising the trap: the size class has to say Rom for the verdict \
             to be the thing that saves it"
        );
        let said = identify(&photo);
        assert!(
            said.starts_with("1 048 576 B, not a boot ROM, "),
            "a 1 MiB photograph was identified as {said:?}"
        );
        // **The verdict's own evidence, and it is the number a report would quote.** `flash`'s
        // sentence is `word 0 is 0xe0ffd8ff, which is not an ARM branch`; the band has already
        // said *not a boot ROM*, so what it draws is the half that is not already on the line.
        assert!(
            said.contains("word 0 is 0xe0ffd8ff"),
            "the verdict's own evidence is missing from {said:?}"
        );
        assert!(
            !said.contains(", which "),
            "{said:?} carries the verdict twice — the band leads with `not a boot ROM` already"
        );
        assert!(!said.contains("5.5G"), "{said:?} attaches a generation to a photograph");

        // A second verdict, so this is testing `flash` rather than one string: word 0 IS a branch
        // here, so the refusal comes from the missing image directory instead.
        let deeper = identify(&a_rom_sized_branch(&dir, "half-a-dump.bin"));
        assert!(
            deeper.contains("flsh") || deeper.contains("image directory"),
            "a 1 MiB file with a plausible reset vector and no directory reads {deeper:?}"
        );
        assert_ne!(deeper, said, "both 1 MiB files got the same sentence, so nothing read them");
    }

    /// **§11.4 rule 3.** Two ROMs file everything and compose nothing; one ROM and an `.ipsw`
    /// compose one device, in either order.
    #[test]
    fn an_ambiguous_drop_files_a_note_and_composes_nothing() {
        let dir = scratch("ambiguous");
        let a = a_rom_sized_branch(&dir, "one.bin");
        let b = a_rom_sized_branch(&dir, "two.bin");
        let ipsw = an_ipsw(&dir, "iPod_25.1.3.ipsw");

        let mut s = Settings::default();
        let out = land(&mut s, &[a.clone(), b.clone(), ipsw.clone()]);
        assert_eq!(out.device, None, "two ROMs composed {:?}", out.device);
        assert!(s.devices.is_empty(), "an ambiguous drop made {} device(s)", s.devices.len());
        assert_eq!(out.wrote, Wrote::Library, "nothing was filed either");
        assert!(
            out.note.contains("make a device from them in Devices"),
            "the ambiguous drop's note is {:?}, which names no way forward",
            out.note
        );
        assert_eq!(s.resources.len(), 3, "the three parts are not all in the library");

        // …and the two-file case keeps its two-file scope, in either order.
        for pair in [[a.clone(), ipsw.clone()], [ipsw.clone(), a.clone()]] {
            let mut s = Settings::default();
            let out = land(&mut s, &pair);
            assert!(out.device.is_some(), "{pair:?} composed nothing");
            assert_eq!(s.devices.len(), 1, "{pair:?} made {} devices", s.devices.len());
            assert_eq!(s.nor, nor::Source::File(a.clone()), "the device is made of the wrong ROM");
            assert_eq!(s.resources.len(), 2, "the pair is not both in the library");
        }

        // A drop with no ROM in it composes nothing, and that is not a refusal.
        let mut s = Settings::default();
        let out = land(&mut s, &[ipsw]);
        assert_eq!(out.device, None);
        assert_eq!(out.wrote, Wrote::Library);
        assert!(s.devices.is_empty());
    }

    /// A drop of something nothing recognises is **named, not filed** — it reaches the Rail and it
    /// does not grow a row, and the library must not be rewritten for it.
    #[test]
    fn a_file_nothing_recognises_is_named_and_not_filed() {
        let dir = scratch("unknown");
        let odd = a_file(&dir, "notes.txt", 12, b"hell");
        assert_eq!(inspect::classify(&odd), Kind::Unknown);
        let mut s = Settings::default();
        let out = land(&mut s, &[odd]);
        assert_eq!(out.wrote, Wrote::Nothing, "an unfiled drop asked for a save");
        assert!(s.resources.is_empty() && s.disks.is_empty(), "it was filed after all");
        assert!(out.note.contains("nothing identifies it"), "{:?}", out.note);
        assert!(identify(Path::new("/definitely/not/here")).contains("named, not filed"));
    }

    /// The group's verb files into **the group whose verb was pressed**, and says so when the
    /// contents disagree.
    #[test]
    fn provide_files_where_the_operator_said_and_reports_a_disagreement() {
        let dir = scratch("provide");
        let ipsw = an_ipsw(&dir, "iPod_25.1.3.ipsw");
        let mut s = Settings::default();

        let (wrote, said) = provide(&mut s, Group::Firmware, &ipsw).expect("an .ipsw is filable");
        assert_eq!(wrote, Wrote::Library);
        assert!(said.contains("Apple firmware"), "{said:?}");
        assert!(!said.contains("its contents read as"), "a matching file reported a disagreement");
        assert!(matches!(s.resources[0].what, Resource::Installer(_)));

        // The same file under the wrong verb is still filed — the press is a statement — and the
        // sentence names what the bytes say.
        let mut s = Settings::default();
        let (_, said) = provide(&mut s, Group::Bootloaders, &ipsw).expect("filed where told");
        assert!(matches!(s.resources[0].what, Resource::Bootloader(_)), "the press was overruled");
        assert!(
            said.contains("its contents read as apple firmware"),
            "the disagreement is not reported: {said:?}"
        );

        // A folder is refused rather than filed as a part of unknown size.
        let out = provide(&mut s, Group::Firmware, &dir);
        assert!(out.is_err(), "a directory was filed as a part");
    }

    /// **A file reaching the library through the picker**, from the answer to the settings entry.
    #[test]
    fn a_file_reaching_the_library_through_the_picker() {
        let dir = scratch("picker");
        let ipsw = an_ipsw(&dir, "iPod_20.1.3.ipsw");
        let sh = Shell::answering([ipsw.clone()]);

        let chosen = sh.pick(Ask::Part(Group::Firmware)).expect("the picker answered");
        let mut s = Settings::default();
        provide(&mut s, Group::Firmware, &chosen).expect("filed");
        assert_eq!(s.resources.len(), 1, "the picker's answer never reached the library");
        assert_eq!(s.resources[0].from, Some(Provenance::Provided));

        // A cancelled dialog is `None`, which is not a failure and files nothing.
        assert_eq!(sh.pick(Ask::Any), None, "an empty picker answered twice");

        // And the other half of the same seam: a reveal goes onto a list rather than onto the
        // operator's screen, which is what lets the suite press a live `Reveal` at all.
        assert_eq!(sh.reveal(&ipsw), Ok(Wrote::Nothing));
        assert_eq!(sh.revealed(), vec![ipsw], "the reveal was not recorded");
    }

    /// Every `Ask` has a title, and none of them is the word `Open`.
    #[test]
    fn every_ask_names_what_will_happen_to_the_file() {
        let asks: Vec<Ask> = Group::ALL
            .iter()
            .map(|g| Ask::Part(*g))
            .chain([Ask::Any])
            .collect();
        for a in asks {
            let t = a.title();
            assert!(!t.trim().is_empty(), "{a:?} opens an untitled dialog");
            assert_ne!(t, "Open", "{a:?} uses the system's own word for it");
        }
    }

    /// The reveal capability and the tool it names agree, and neither claims a platform this
    /// program has no command for.
    #[test]
    fn the_reveal_capability_and_its_tool_agree() {
        assert_eq!(
            can_reveal(),
            reveal_tool().is_some()
                && match reveal_tool() {
                    Some("open") => Path::new("/usr/bin/open").is_file(),
                    Some("explorer") => true,
                    Some(t) => eapp_loader::tooling::have(t),
                    None => false,
                },
            "the capability and the tool disagree"
        );
        // The instrument's own control, which is `tooling::have`'s point: `open --version` exits 1
        // with `unrecognized option`, so the probe that works for `curl` reports the tool that
        // ships with every macOS as absent. That is why the macOS arm asks the filesystem.
        if cfg!(target_os = "macos") {
            assert!(can_reveal(), "/usr/bin/open is missing, which macOS does not do");
            assert!(
                !eapp_loader::tooling::have("open"),
                "`open --version` succeeded, so the reason the macOS arm does not use `have` has \
                 gone away and this arm should go with it"
            );
        }
    }
}
