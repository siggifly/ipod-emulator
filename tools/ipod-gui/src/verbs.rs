//! The drawer's root page — `docs/GUI.md` §21.3 and §21.6.
//!
//! **Every row is a thing to do, phrased as one.** The page this replaces opened on three nouns —
//! `iPods`, `Games`, `Settings` — and behind them were inventory pages: a key/value inspector for
//! a device, a bill of materials for its parts. Firmware, bootloaders, ROM dumps and seeds are the
//! vocabulary of whoever *assembles* a machine, and nobody arrives at this program wanting a
//! bootloader.
//!
//! **A row that cannot run says why, in place, and is never hidden.** §14.1 is already this
//! repository's rule and it survives §21 intact — it is the one part of the old drawer that was
//! right. Every refusal below is a fact about this iPod, this ROM or this build, and the ones that
//! teach the compatibility matrix are the point rather than an apology: *Rockbox is on the drive
//! and Apple's bootloader starts Apple's software* is a true thing about the hardware that a
//! hidden row would have taught nobody.
//!
//! **No toolkit in this file.** It reads `Settings`, `machine::Life` and the model in
//! `ipod-machine`, and returns rows. `main.rs` flattens them into the markup's struct and `verbs.
//! slint` draws them; nothing here knows a `Row` from a `Rectangle`.
//!
//! **It words no sentence twice.** A refusal that another surface already words is fetched from
//! that surface — `crate::blocked_label` for a device that cannot start, `devices::install_row`
//! for Rockbox, `devices::running_rule` for a machine in the way — so this page and the bench
//! cannot describe one iPod two ways. What is written here is only what nothing else says.

use ipod_machine::compose::{Loader, Os};
use ipod_machine::settings::{Device, Presence, Settings};

use crate::devices;
use crate::machine::{self, Life, Restore};
use crate::rail::Caps;
use crate::work;

/// **The one sentence for *there is not an iPod yet*.**
///
/// Five rows on this page reach that state — the four machine controls and `Files on the drive…` —
/// and each of them used to word it for itself. The four controls did not word it at all: they
/// substituted the literal `this iPod` for the device's name and went on to say *this iPod is not
/// running* under a bench captioned `No iPod yet`, four times, on the first screen anybody sees.
/// **There is one thing wrong on that screen and it is not four things.**
const NO_IPOD_YET: &str = "There is no iPod yet.";

/// What §21.4's first run will cost, in the plan's own numbers.
///
/// **`work::cost` and never a literal**, which is the same call `main::empty_shelf` makes for the
/// bench's own row 3 and `push_ledger` makes for the bill — so the drawer, the bench and the ledger
/// cannot print three prices for one press. `Holes::Sparse` because that is what a real build on
/// this machine allocates, and it is the figure the other two quote.
fn first_run_cost() -> String {
    let cost = work::cost(ipod_machine::compose::Holes::Sparse);
    if cost.down == 0 {
        // The catalogue lost the release. `0 B to download` would read as *free* rather than as
        // *nothing to fetch it from*, which is `main::empty_shelf`'s own note on the same branch.
        return "makes an iPod first — nothing to download".into();
    }
    format!(
        "makes an iPod first — {} to download, about {} on disk",
        ipod_machine::si(cost.down),
        ipod_machine::si(cost.disk)
    )
}

/// One row of the page. **Closed, and ordered as the page draws it**, so `main.rs` can turn a
/// press back into a verb by ordinal and nothing has to keep a second list in step.
///
/// The four at the end are the developer switch's, which is why they are last: §21.3's page is the
/// eight rows above them plus §21.6's five, and `Settings::developer` reveals a fifth group rather
/// than re-ordering the first four.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verb {
    // ── §21.3: what to run ──
    Apple,
    Rockbox,
    Doom,
    Diagnostics,
    Games,
    // ── §21.6: the machine, named as emulator controls ──
    Start,
    Suspend,
    Resume,
    Kill,
    Restart,
    // ── §21.3: the rest of the list ──
    Files,
    Panel,
    ThisIpod,
    Settings,
    // ── the developer switch's four ──
    Parts,
    Readout,
    Work,
    Reference,
}

impl Verb {
    /// Every verb, in the order the page draws them. A sweep that walked a subset and believed it
    /// had walked the set is the shape this exists to prevent.
    pub const ALL: [Verb; 18] = [
        Verb::Apple,
        Verb::Rockbox,
        Verb::Doom,
        Verb::Diagnostics,
        Verb::Games,
        Verb::Start,
        Verb::Suspend,
        Verb::Resume,
        Verb::Kill,
        Verb::Restart,
        Verb::Files,
        Verb::Panel,
        Verb::ThisIpod,
        Verb::Settings,
        Verb::Parts,
        Verb::Readout,
        Verb::Work,
        Verb::Reference,
    ];

    /// The ordinal the markup sends back. Derived from [`Verb::ALL`] rather than typed, so adding a
    /// verb cannot leave two numbers meaning one row.
    pub fn ordinal(self) -> i32 {
        Verb::ALL
            .iter()
            .position(|v| *v == self)
            .expect("every verb is in ALL") as i32
    }

    /// The verb an ordinal names, or `None` — which is the same no-op an unknown row gets
    /// everywhere else in this window.
    pub fn from_i32(n: i32) -> Option<Verb> {
        usize::try_from(n).ok().and_then(|i| Verb::ALL.get(i).copied())
    }

    /// The label, and it is a **thing to do** in every case.
    ///
    /// `Apple's software`, not `iPod software (Os)` — the second is what `emu::BootTarget::label`
    /// answers, which is right for a picker naming a target and wrong for a row somebody presses.
    pub fn label(self) -> &'static str {
        match self {
            Verb::Apple => "Apple's software",
            Verb::Rockbox => "Rockbox",
            Verb::Doom => "Doom",
            Verb::Diagnostics => "Diagnostics",
            Verb::Games => "Games…",
            Verb::Start => "Start",
            Verb::Suspend => "Suspend",
            Verb::Resume => "Resume",
            Verb::Kill => "Kill",
            Verb::Restart => "Restart",
            Verb::Files => "Files on the drive…",
            Verb::Panel => "Panel in its own window",
            Verb::ThisIpod => "This iPod",
            Verb::Settings => "Settings",
            Verb::Parts => "Parts",
            Verb::Readout => "Readout",
            Verb::Work => "Work",
            Verb::Reference => "Reference",
        }
    }

    /// Which band of the page this row is in. A rule is drawn where the number changes, which is
    /// how §21.3's one horizontal line gets drawn without anybody counting rows.
    fn group(self) -> i32 {
        match self {
            Verb::Apple | Verb::Rockbox | Verb::Doom | Verb::Diagnostics | Verb::Games => 0,
            Verb::Start | Verb::Suspend | Verb::Resume | Verb::Kill | Verb::Restart => 1,
            Verb::Files | Verb::Panel | Verb::ThisIpod | Verb::Settings => 2,
            Verb::Parts | Verb::Readout | Verb::Work | Verb::Reference => 3,
        }
    }

    /// Whether this row is only drawn with `Settings::developer` on.
    pub fn developer_only(self) -> bool {
        self.group() == 3
    }

    /// **Whether pressing this goes one level deeper**, which is the only thing a chevron means.
    ///
    /// It is a property of the verb rather than a flag each row sets, because it was a flag each
    /// row set and two of them got it wrong the first time: `Doom` and `Diagnostics` drew a `›`
    /// while refused, promising a page that does not exist behind a control that does not run.
    /// A chevron is a claim about where a press goes, and where a press goes is what this enum is.
    pub fn opens_a_page(self) -> bool {
        matches!(
            self,
            Verb::Games
                | Verb::ThisIpod
                | Verb::Settings
                | Verb::Parts
                | Verb::Readout
                | Verb::Work
                // §22.6, and it is on this list for the same reason `Games` had to be: a chevron
                // is a claim that pressing goes one level deeper, and this row now does.
                | Verb::Reference
        )
    }
}

/// One drawn row, flattened for the markup by `main.rs`.
///
/// **`reason` is non-empty whenever `enabled` is false, and that is a rule rather than a habit** —
/// §9.4 reserves the slot under a disabled control and a control that is refused without saying
/// why is the hidden option §14.1 exists to reject. `every_disabled_verb_says_why` asserts it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub verb: Verb,
    pub label: String,
    /// The right-hand column: a count, a key, or what this iPod currently is.
    pub value: String,
    /// A fact on a second line inside the same hit region — §11.4's shape. What pressing costs,
    /// or what the row is about.
    pub sub: String,
    pub enabled: bool,
    pub reason: String,
    /// §9.4's two kinds. True is *this cannot work, ever*; false is *this is not finished, by us*
    /// and carries a command where there is one.
    pub machine_rule: bool,
    pub escape: String,
    /// This row goes one level deeper into the drawer.
    pub chevron: bool,
    /// A 1 px rule above this row, because the group changed.
    pub rule_above: bool,
}

impl Row {
    fn go(verb: Verb, sub: &str) -> Row {
        Row {
            verb,
            label: verb.label().to_string(),
            value: String::new(),
            sub: sub.to_string(),
            enabled: true,
            reason: String::new(),
            machine_rule: true,
            escape: String::new(),
            // A chevron is a claim about where a press goes, and the verb is what knows.
            chevron: verb.opens_a_page(),
            rule_above: false,
        }
    }

    /// §14.1: drawn, refused, and it says why. `machine_rule` picks which of §9.4's two kinds.
    fn no(verb: Verb, why: String, machine_rule: bool) -> Row {
        Row {
            reason: why,
            enabled: false,
            machine_rule,
            ..Row::go(verb, "")
        }
    }

    fn escape(mut self, cmd: &str) -> Row {
        self.escape = cmd.to_string();
        self
    }
}

/// What the page needs to know that it cannot ask the library — the machine, and where the titles
/// are.
///
/// **A struct rather than six arguments**, because every one of them comes from a different owner
/// in `main.rs` and a positional list of four `Option<&str>` and two `bool`s is a call nobody can
/// read. Every field is a fact somebody else already decided; nothing here is re-derived.
#[derive(Clone, Copy)]
pub struct Now<'a> {
    /// The library, for the one question that needs a file on disk rather than a field: §12.4's
    /// restore pair, through `crate::resting_config` — the same config `cradle_of` asks, so the
    /// cradle's *about 3 s* and this page's `Resume` cannot disagree about whether there is one.
    pub settings: &'a Settings,
    /// §12.2's phase, for §21.6's five controls.
    pub life: &'a Life,
    /// The name of the device the machine is running, or `None`. `crate::running_machine`'s answer.
    pub machine: Option<&'a str>,
    /// What a park would cost this machine, in bytes — `Link::snapshot_bytes`, which the run loop
    /// publishes *before* anything can ask for one, summed off the format that writes it.
    ///
    /// **`None` when there is no machine to ask**, and the row then makes no claim about size.
    /// The first draft wrote *About 149 MB* as a literal, taken out of §12.4's own measurement of
    /// a 5.5G — which is the shape the boot caption's `about 75 s` already is: a number that was
    /// true of one machine at one clock and goes on being printed. A figure without the thing that
    /// produced it cannot be rechecked and goes stale silently (AGENTS.md §5).
    pub park_bytes: Option<u64>,
    /// Whether a machine thread exists at all, running or not. `Resume` needs the difference:
    /// §12.4 says a restore happens only as a thread is *built*, so a window that already has one
    /// cannot get back to a snapshot and the row has to say so instead of cold-booting.
    pub thread: bool,
    /// How many titles are on the shelf, and whether the shelf itself has gone.
    pub titles: usize,
    pub games_gone: bool,
    pub developer: bool,
}

/// The whole page.
///
/// The device is the one the window is looking at — `Settings::current`'s, resolved by the caller,
/// because the bench's `←`/`→` and the Devices page write one selection and this page reads it
/// rather than keeping a second.
pub fn view(s: &Settings, d: Option<&Device>, seen: &mut Presence, caps: Caps, now: Now) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();

    for verb in Verb::ALL {
        if verb.developer_only() && !now.developer {
            continue;
        }
        let mut row = match verb {
            Verb::Apple => boot_row(s, d, seen, now, Verb::Apple),
            Verb::Rockbox => rockbox_row(s, d, seen, caps, now),
            Verb::Doom => doom_row(s, d, caps, now),
            Verb::Diagnostics => diagnostics_row(s, d, seen, now),
            Verb::Games => games_row(now),
            Verb::Start | Verb::Suspend | Verb::Resume | Verb::Kill | Verb::Restart => {
                control_row(verb, d, now)
            }
            Verb::Files => files_row(d),
            Verb::Panel => panel_row(now),
            Verb::ThisIpod => this_ipod_row(s, d),
            Verb::Settings => Row::go(Verb::Settings, ""),
            Verb::Parts => Row::go(Verb::Parts, ""),
            Verb::Readout => Row::go(Verb::Readout, ""),
            Verb::Work => Row::go(Verb::Work, ""),
            // **It was the one row with nothing behind it, and §22.6 built the page.** The refusal
            // that stood here — *"The keyboard table and the stated limits have no page yet."* —
            // named its own retirement condition and this is it: `ui/reference.slint` draws
            // `reference::page()`, `nav::Page::Reference` answers `Some(1)` from `slot()`, and
            // `⌘,` and `?` both reach it. All three had to move together, which is what
            // `every_built_page_is_reachable_from_its_row` measures.
            //
            // **The value column carries `⌘,`'s printed form**, for `panel_row`'s reason: a key
            // that appeared only where it works would be discoverable exactly when it is no longer
            // needed. This one is the opposite case — the key is how most people will arrive — so
            // the row is where somebody finds out the key exists at all.
            Verb::Reference => Row {
                value: "Cmd-,".into(),
                ..Row::go(Verb::Reference, "every key this program binds")
            },
        };
        row.rule_above = out
            .last()
            .is_some_and(|prev| prev.verb.group() != verb.group());
        out.push(row);
    }
    out
}

/// The rows that start the machine at the reset vector, and the four things that stop them.
///
/// **The order is the order they bite**, which is the same discipline `devices::install_row`
/// states: the machine in the way first, because that is about this moment; then the device, which
/// is about whether there is anything to start; then the bootloader, which is about what would
/// actually run.
fn boot_row(s: &Settings, d: Option<&Device>, seen: &mut Presence, now: Now, verb: Verb) -> Row {
    // §9.1 and §10.2: an empty library is a state with something to do, not an absent one. The
    // press is the first run's — `on_start_device` routes it — so the row is live and says what it
    // will cost rather than refusing over a device that is about to exist.
    let Some(d) = d else {
        return Row::go(verb, "downloads Apple's firmware and builds an 8 GB drive");
    };
    if let Some(m) = now.machine.filter(|m| *m == d.name) {
        return Row::no(verb, devices::running_rule(m), true);
    }
    let absent = s.missing_with(d, seen);
    let mismatch = s.generation_mismatch(d);
    if let Some(b) = machine::Blocked::of(Some(d), &absent, mismatch.as_deref()) {
        // The bench's own words for this device, so one iPod is not described two ways.
        return Row::no(verb, crate::blocked_label(crate::Press::Centre, d, &absent, b, mismatch.as_deref()), b.machine_rule());
    }
    Row::go(verb, "")
}

/// **What the bootloader on this iPod will actually start**, which is the whole of why
/// `Apple's software` and `Rockbox` are two rows rather than one.
///
/// `compose::Loader`'s own documentation is the matrix: Apple's bootloader starts Apple's
/// software; Rockbox's *"hands back to Apple's software when MENU is held at power-on"*;
/// `ipodloader2` reads `loader.cfg` and draws a menu. So a press on a row is honest only where
/// the loader agrees with it, and where it does not the row says which one is in the way.
///
/// **Nothing here holds a button at power-on.** That is the missing half — the chord is §21.5's
/// work and belongs to the machine, not to this page — so the MENU route is not offered as a
/// control that does not exist.
///
/// **Every sentence below is short because §9.4's slot is one elided line of 372 px**, and
/// `every_reason_this_window_draws_fits_the_slot_it_is_drawn_in` is what measures it. The first
/// draft of the Rockbox-loader arm read *"…Apple's software is reached by holding MENU at
/// power-on, and nothing in this window holds a button at power-on yet"* and drew **940 px**, so
/// it would have been cut off mid-clause exactly where a person reads it. What it explained is in
/// this doc comment and in `docs/GUI.md` §21.3; what is on screen is the half that answers *why
/// not*.
pub(crate) fn loader_refusal(s: &Settings, d: &Device, verb: Verb) -> Option<String> {
    let r = s.recipe_of(d);
    match (verb, r.loader) {
        (Verb::Apple, Loader::Apple) => None,
        (Verb::Apple, Loader::Rockbox) => {
            Some("Rockbox's bootloader is on this iPod and starts Rockbox.".into())
        }
        (Verb::Apple, Loader::IPodLoader2) => {
            Some("ipodloader2 starts whatever loader.cfg names.".into())
        }
        (Verb::Rockbox, Loader::Rockbox) => None,
        (Verb::Rockbox, Loader::Apple) => {
            Some(format!("Apple's bootloader is on {} and starts Apple's software.", d.name))
        }
        (Verb::Rockbox, Loader::IPodLoader2) => {
            Some("ipodloader2 starts whatever loader.cfg names.".into())
        }
        _ => None,
    }
}

/// `Rockbox` is **two verbs wearing one label**, and which one it is depends on whether Rockbox is
/// on the drive already.
///
/// Not installed, this row is `devices::install_row` — the same two downloads and two writes the
/// Devices page offers, with the same refusals worded in the same place. Installed, it is a boot,
/// refused where the bootloader would not reach it.
///
/// **One label either way**, which is §21.3's rule: a person wants Rockbox, and whether that means
/// fetching 9 MB first is the program's problem rather than a choice to put in front of them.
fn rockbox_row(s: &Settings, d: Option<&Device>, seen: &mut Presence, caps: Caps, now: Now) -> Row {
    let Some(d) = d else {
        return Row::no(
            Verb::Rockbox,
            "There is no iPod yet — the row above makes one.".into(),
            true,
        );
    };
    let installed = s.recipe_of(d).oses.contains(&Os::Rockbox);
    if !installed {
        let fix = devices::install_row(s, d, caps, now.machine);
        return Row {
            enabled: fix.enabled,
            reason: fix.reason,
            machine_rule: fix.machine_rule,
            ..Row::go(
                Verb::Rockbox,
                "downloads 9 MB and writes a new drive beside this one",
            )
        };
    }
    let row = boot_row(s, Some(d), seen, now, Verb::Rockbox);
    if !row.enabled {
        return row;
    }
    match loader_refusal(s, d, Verb::Rockbox) {
        Some(why) => Row::no(Verb::Rockbox, why, true),
        None => row,
    }
}

/// **Doom, and it is a press rather than a refusal now.**
///
/// §21.0's table recorded this row as *disabled, with its reason* — `Doom needs rockdoom.wad and an
/// IWAD; nothing fetches them` — and that sentence was **true of the window and false of the
/// program**. `ipod_machine::doom` has held the catalogue, the URLs, both SHA-256s and the shortcut
/// file since 2026-09-01, and `ipod-boot doom-assets` installs all three; nothing in the window
/// called it. A module that knows exactly where three files live and cannot put them on a disk is
/// the same defect class as a flag with no mechanism behind it, and the row was reporting the
/// mechanism's absence rather than its own.
///
/// **The chain is iPod → Rockbox → plugin → game, and the row refuses at whichever link is
/// missing** — naming the row above, which is where that link is made. §21.3's rule: a refusal
/// another surface already says is fetched from that surface rather than re-worded here.
///
/// **What a live press does, in one go**: fetch `rockdoom.wad` (285 KB) and Freedoom 0.13.0
/// (24 MB), verify both, write them plus `shortcuts.txt` onto the volume, and start the machine.
/// The boot is part of the press because Doom is a thing to *play* — `Queue::doom`'s own
/// `Run::Doom` hands over the way §10's first run does.
///
/// **It does not claim to know whether Doom is already on the drive, and that is deliberate.**
/// `doom::missing` opens the FAT32 volume and walks it; this row is rebuilt on every machine tick,
/// so asking would put a filesystem walk behind the frame rate. The press is idempotent instead —
/// `get_watched` skips a download it already has, and the three writes replace themselves — so the
/// sentence is true whether it is the first press or the fifth.
///
/// **Licensing, because this is a public repository.** Doom's own IWADs are commercial and cannot
/// be named, fetched or shipped here. Rockbox's own manual names the substitute — `doom.tex`: *"A
/// free alternative for Doom 2 is FreeDoom … This can be used in place of `doom2.wad`"* — and
/// Freedoom is BSD-licensed. We fetch from source and host nothing.
fn doom_row(s: &Settings, d: Option<&Device>, caps: Caps, now: Now) -> Row {
    let Some(d) = d else {
        return Row::no(
            Verb::Doom,
            "There is no iPod yet — the row above makes one.".into(),
            true,
        );
    };
    if let Some(m) = now.machine.filter(|m| *m == d.name) {
        return Row::no(Verb::Doom, devices::running_rule(m), true);
    }
    // **Rockbox first, and the sentence names the row that installs it.** `doom.rock` ships inside
    // the Rockbox release, so without Rockbox there is no plugin for the WADs to feed — and the
    // row above is one press from having one.
    if !s.recipe_of(d).oses.contains(&Os::Rockbox) {
        return Row::no(
            Verb::Doom,
            format!("Doom is a Rockbox plugin, and Rockbox is not on {} yet — the row above puts it there.", d.name),
            true,
        );
    }
    // …and if the bootloader on this iPod would not reach Rockbox, installing Doom's files onto the
    // volume would put 24 MB somewhere nothing is going to look. Same question the row above asks,
    // asked once.
    if let Some(why) = loader_refusal(s, d, Verb::Rockbox) {
        return Row::no(Verb::Doom, why, true);
    }
    if s.disk_of(d).and_then(|r| r.ok()).is_none() {
        return Row::no(
            Verb::Doom,
            format!("{} has no drive to put Doom on.", d.name),
            true,
        );
    }
    if !caps.download {
        return Row::no(
            Verb::Doom,
            "this build has no `curl` to download Doom's levels with".into(),
            false,
        )
        .escape("ipod-boot doom-assets DISK.img");
    }
    Row::go(
        Verb::Doom,
        "downloads 24 MB, then boots Rockbox — Doom is in Shortcuts",
    )
}

/// §21.7's second view of the panel, in a window of its own.
///
/// **A second view, never a second machine.** §15 rules out *"a second window, tear-off panels,
/// multiple machines"* because there is exactly one machine by design; §21.7 narrows that rather
/// than overturning it — that argument is about a second *machine* and does not touch a second view
/// of one framebuffer. The iPod keeps its screen, both draw the same texture, and closing the
/// popped-out window can therefore strand nothing.
///
/// **Refused when there is nothing on the panel**, which is §12.6's own rule for fullscreen and is
/// the same rule for the same reason: a window that opened onto a dark rectangle would be a control
/// offered and answered in the same breath. `Life::alive` is the question — a machine that is
/// booting has something on the glass and one that is off does not.
///
/// **The key is drawn whether or not the row can be pressed**, which is §21.6's *keep their keys*
/// applied one band along: a key that appeared only in the state where it works would be
/// discoverable exactly when it is no longer needed.
fn panel_row(now: Now) -> Row {
    let row = Row {
        value: "Ctrl-Cmd-P".into(),
        ..Row::go(
            Verb::Panel,
            "the screen on its own — fullscreen it on a second display",
        )
    };
    if now.life.alive() {
        return row;
    }
    Row {
        value: row.value.clone(),
        ..Row::no(
            Verb::Panel,
            "There is nothing on the panel yet."
                .into(),
            true,
        )
    }
}

/// §12.5's boot target, and its refusal when the ROM cannot carry it.
///
/// **A generated ROM has no `diag`**, and that is a limit on two *modes* rather than on the
/// operating system: `osos` is on the drive and comes out of Apple's IPSW, which is why RetailOS
/// boots perfectly well on a synthesised ROM. Apple shipped Diagnostics and Disk Mode inside the
/// part. `devices::facts` says the same thing in the `Modes` row, from the same `nor_of` call.
///
/// **§12.5's own wording does not fit §9.4's slot, and this is where the two collide.** That
/// section mandates *"Diagnostics lives inside the boot ROM's image directory, and a generated ROM
/// has none"* as this row's machine rule; measured, it is **477 px** against a 372 px slot that
/// elides, so drawn verbatim a person reads *"…and a gen…"*. The short form below says the same
/// thing and fits, and the long one survives where it can be read whole: the `Modes` fact wraps,
/// because a fact is not a reason slot. `docs/GUI.md` §21.3 carries the correction.
fn diagnostics_row(s: &Settings, d: Option<&Device>, seen: &mut Presence, now: Now) -> Row {
    let row = boot_row(s, d, seen, now, Verb::Diagnostics);
    let Some(d) = d else {
        return Row::no(
            Verb::Diagnostics,
            "There is no iPod yet, and Diagnostics comes out of one's boot ROM.".into(),
            true,
        );
    };
    if matches!(
        s.nor_of(d),
        Some(ipod_machine::nor::Source::Synthetic { .. })
    ) {
        return Row::no(
            Verb::Diagnostics,
            "A generated boot ROM carries no Diagnostics image.".into(),
            true,
        );
    }
    row
}

/// §13's page, which is built: it files a shelf, lists what is on it, and starts a title on the
/// bench.
///
/// **It is a chevron and it is live**, including with an empty shelf, for the reason `games.slint`
/// already gives about its own first row: *a page whose only control is disabled when the list is
/// empty is a page you cannot get out of*. Choosing where the titles are is the way in, and it is
/// on that page. The count and the shelf's state are on this row so the press is not a surprise.
fn games_row(now: Now) -> Row {
    let sub = if now.games_gone {
        "the folder that held them is not there any more"
    } else if now.titles == 0 {
        "no titles yet — choose where yours are"
    } else {
        ""
    };
    Row {
        value: if now.titles > 0 {
            now.titles.to_string()
        } else {
            String::new()
        },
        ..Row::go(Verb::Games, sub)
    }
}

/// §21.6's five, **named as emulator controls and not after hardware**.
///
/// §21.5 is the measured reason and it is not a preference: MENU + SELECT is delivered to the
/// machine, arrives at Apple's ISR decoder, and does not reset it — the mask `0x110000` appears
/// zero times in 641 479 disassembled instructions of `OSOS_correct.bin`. On real hardware the
/// chord is caught below the firmware, in the wheel's PSoC or the PMU, and this project models
/// neither. So these are things **this program** does to a machine and they say so.
///
/// **Every refusal is a physical statement**, which is `machine::permits`'s own rule: you cannot
/// power off a machine that is off, you cannot start one that is running. The exception is
/// `Resume`, whose refusal is a fact about this window rather than about the machine — see below.
fn control_row(verb: Verb, d: Option<&Device>, now: Now) -> Row {
    let alive = now.life.alive();
    // **`this iPod` is a phrase about an iPod, and with an empty library there is not one.** These
    // four rows read *this iPod is not running* / *this iPod has no restore point* on the first
    // screen anybody sees, directly under a row that says `No iPod yet`. There is one thing wrong
    // on that screen and it is not four things — which is the empty case `files_row` two functions
    // down already words for itself, and this borrows its sentence rather than inventing a fifth.
    let named = d.map(|d| d.name.as_str());
    let no_ipod = || NO_IPOD_YET.to_string();
    // **The key is on the row whether or not the row can be pressed**, and that is §21.6's *keep
    // their keys* taken literally: the operator could not work out how to stop a machine, and a
    // person who reads `Suspend · Esc` on a machine that is off has learnt the key for the moment
    // there is one. A key that appeared only in the state where it works would be discoverable
    // exactly when it was no longer needed.
    let key = match verb {
        // Not a keystroke: §7.3 makes the drawn button the start affordance, and this row is a
        // second way to reach it rather than a different act.
        Verb::Start | Verb::Resume => "the centre button",
        // §16.8 gives `Esc` ONE definition, outwards — and at the end of it, `Park` from `Running`
        // and `PowerOff` from `Booting`. Two rows, one key, and the sentences under them are what
        // tell the two apart.
        Verb::Suspend | Verb::Kill => "Esc",
        _ => "",
    };
    let row = |sub: &str| Row {
        value: key.into(),
        ..Row::go(verb, sub)
    };
    let no = |why: String| Row {
        value: key.into(),
        ..Row::no(verb, why, true)
    };
    match verb {
        // The centre button is the same press, which is why the key column names it rather than a
        // keystroke: §7.3 makes the drawn button the start affordance and this row is the second
        // way to reach it, not a different act.
        Verb::Start if alive => no(format!("{} is already running.", named.unwrap_or("It"))),
        // **With no iPod this press is §21.4's first run, and the row has to cost it as one.** It
        // read *cold boot, from the reset vector*, which is what `Cmd::PowerOn` does and not what
        // this press does: `press_is_first_run` routes an empty library into `work::Queue::press`,
        // which downloads Apple's firmware and builds an 8 GB drive before anything boots. The
        // bench's own cradle two inches away has said so all along — `Press the centre button to
        // make an iPod`, with the bill under it — and this row promised a reset vector.
        //
        // **The numbers are `work::cost`'s**, the same call the empty bench's shelf and the ledger
        // make, so three surfaces cannot print three bills for one press.
        Verb::Start if d.is_none() => row(&first_run_cost()),
        Verb::Start => row("cold boot, from the reset vector"),

        // §12.4. `Esc` from `Running` sets `Link::save_on_quit`, and `nav::Stack::escape` is the
        // one definition of that key — this row is the same act with a label on it, which is the
        // whole of what §21.6 asks for: *none of them is discoverable*.
        Verb::Suspend if !alive => no(match named {
            Some(n) => format!("{n} is not running, so there is nothing to put down."),
            None => no_ipod(),
        }),
        Verb::Suspend => row(&match now.park_bytes.filter(|n| *n > 0) {
            Some(n) => format!("writes the restore point and stops — {}", ipod_machine::si(n)),
            None => "writes the restore point and stops".into(),
        }),

        // **The one refusal here that is about the window rather than the machine**, and §12.4
        // states it: `Cmd::PowerOn`'s own doc is *"always a cold boot, never a restore"*, and the
        // only code that restores is `emu::run`'s entry, gated on `Config::may_restore(first)`
        // with `first` false for every power cycle inside a session. So a resume is reachable only
        // by *building* the machine thread, and a window that has already built one has no route
        // back to the snapshot. Saying so is better than sending `PowerOn` under a label that
        // promised three seconds.
        Verb::Resume if alive => no(format!("{} is running.", named.unwrap_or("It"))),
        Verb::Resume => match d.map(|d| Restore::of(&crate::resting_config(now.settings, d))) {
            // **`about 3 s` was the second literal of the pair, and the file a resume will read
            // replaces it.** Its twin on §7.3's cradle went the same way, for §21.6's own stated
            // reason: a figure true of one machine on one host goes on being printed long after
            // everything that made it true has moved. The restore point's size is a fact about
            // *this* device, it costs one `stat` of a file `Restore::of` has just established the
            // existence of, and it is the currency `Suspend` two rows up already reports in.
            Some(Restore::Whole) if !now.thread => row(&match d
                .and_then(|d| crate::resting_config(now.settings, d).snapshot)
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
            {
                Some(n) => format!(
                    "puts the machine back where it was — {} to read",
                    ipod_machine::si(n)
                ),
                None => "puts the machine back where it was".into(),
            }),
            Some(Restore::Whole) => no(
                "This machine has been built and powered off, and a restore happens only as the \
                 thread is built. Close the window and start it again to resume."
                    .into(),
            ),
            // Reachable only with a device, since `Restore::of` was asked about one — so the name
            // is always there, and `It` is the total-match answer rather than a case that happens.
            Some(Restore::Broken) => no(format!(
                "{}'s restore point no longer matches its drive, so resuming it would pair a \
                 restored memory with a drive that moved.",
                named.unwrap_or("It")
            )),
            _ => no(match named {
                Some(n) => format!("{n} has no restore point."),
                None => no_ipod(),
            }),
        },

        // §12.5: power off is real — the machine is dropped and re-entered at the reset vector,
        // not restored and pretended. `Esc` from `Booting` is this, because §12.4 refuses to park
        // a boot: a 1.6 GB write of a state nobody wants.
        Verb::Kill if !alive => no(match named {
            Some(n) => format!("{n} is not running."),
            None => no_ipod(),
        }),
        Verb::Kill => row("drops the machine. Nothing is written; the next start is cold."),

        Verb::Restart if !alive => no(match named {
            Some(n) => format!("{n} is not running, so there is nothing to cycle."),
            None => no_ipod(),
        }),
        Verb::Restart => row("power off and straight back on, from the reset vector"),

        // Not reachable: the caller matches on the same five. Total rather than `unreachable!`,
        // because a panic here would take the window down over a row.
        _ => no("This is not a machine control.".into()),
    }
}

/// §12.9's `fat` browsing — *what does earn a surface*, and it does not have one yet.
///
/// §9.4's second kind, with the command that does it today. `ipod-boot fat DISK.img tree` is read
/// off that program's own usage line rather than invented, which is the rule every escape hatch in
/// this window follows.
fn files_row(d: Option<&Device>) -> Row {
    let why = match d {
        Some(d) => format!("Nothing in this window reads {}'s volume yet.", d.name),
        None => NO_IPOD_YET.into(),
    };
    Row::no(Verb::Files, why, false).escape("ipod-boot fat DISK.img tree")
}

/// §21.3's `This iPod`, and it is a **chevron rather than two pickers**.
///
/// The design draws it as `This iPod  5.5G ▾  synthetic ▾` — the row where the default is
/// *changed*. What this program already has for changing those two things is §11.2's Composer,
/// reached from the Devices page's `Edit…`, and it changes them with the verdict, the plan and the
/// cost attached. Two Expands here would be a second way to write the same two fields, one of them
/// without the verdict — which is the drift §16.9 exists to delete. So the row states what this
/// iPod is and goes to the page that changes it.
fn this_ipod_row(s: &Settings, d: Option<&Device>) -> Row {
    let Some(d) = d else {
        return Row {
            value: "none yet".into(),
            ..Row::go(Verb::ThisIpod, "")
        };
    };
    let kind = match s.nor_of(d) {
        Some(ipod_machine::nor::Source::Synthetic { .. }) => "synthetic",
        Some(_) => "dumped",
        None => "unresolved",
    };
    Row {
        value: kind.into(),
        ..Row::go(Verb::ThisIpod, &d.name)
    }
}
