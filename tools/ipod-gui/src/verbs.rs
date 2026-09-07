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

/// §22.4's two labels and the key that reaches the on position.
///
/// **Named rather than typed at four sites**, because they are one control's two states and a
/// fifth spelling is how a switch comes to say `Turn Off` in one phase and `Turn off` in another.
/// `Esc` is not among them: it is on the row in both live phases and is written where it is read,
/// beside the sentence that says which of `Park` and `PowerOff` it will be.
const TURN_OFF: &str = "Turn off";
const TURN_ON: &str = "Turn on";
/// §7.3 makes the drawn centre button the start affordance, so the on position names it rather
/// than a keystroke — this row is a second way to reach that press, not a different act.
///
/// **Two words and not three.** §21.6 wrote `the centre button` into a 124 px column; §22.4's
/// switch takes 40 px of that column, and the article was the first thing to go over the edge —
/// `_out/gui/menu.png` drew `the centre …`. A value that elides is a key nobody can read, which is
/// the whole of what *keep their keys* was for.
const CENTRE: &str = "centre button";

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
    // ── §22.4: the machine, as one switch ──
    Power,
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
    pub const ALL: [Verb; 14] = [
        Verb::Apple,
        Verb::Rockbox,
        Verb::Doom,
        Verb::Diagnostics,
        Verb::Games,
        Verb::Power,
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
            // **The label is the act, and the act depends on which way the switch is
            // thrown** — so [`power_row`] overwrites it. This is the off position's, because a
            // window that has not yet been told about a machine has not got one.
            Verb::Power => "Turn on",
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
            Verb::Power => 1,
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
    /// §22.4: this row is a **switch**, so it is drawn with a switch's affordance and announced as
    /// one. Exactly one row is, and [`power_row`] is the only thing that sets it.
    pub switch: bool,
    /// Which way that switch is thrown. Meaningless on every row where `switch` is false, and it
    /// is a plain `bool` beside a plain `bool` rather than an `Option` for that reason: the markup
    /// reads it only under `switch`, and an `Option<bool>` in a Slint struct is a second nullable
    /// field for a distinction one boolean already makes.
    pub on: bool,
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
            switch: false,
            on: false,
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
            Verb::Power => power_row(d, now),
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
    let absent = s.missing_with(d, seen);
    let mismatch = s.generation_mismatch(d);
    if let Some(b) = machine::Blocked::of(Some(d), &absent, mismatch.as_deref()) {
        // The bench's own words for this device, so one iPod is not described two ways.
        return Row::no(verb, crate::blocked_label(crate::Press::Centre, d, &absent, b, mismatch.as_deref()), b.machine_rule());
    }
    // **§22.2: a machine in the way is not a reason to refuse a boot — it is the thing to power
    // cycle.** This arm used to be `devices::running_rule` — *"My 5.5G is running. Stop it
    // first."* — and it was the largest single source of grey on the page: four rows telling a
    // person to go and perform an act the row itself could perform. `emu::Cmd::Boot` is that act
    // and it already existed: §12.5 makes it a power cycle in every phase, *"that is how the
    // hardware reaches them"*, and `machine::permits` answers true for it everywhere for exactly
    // that reason.
    //
    // **It costs a cold boot and the row says so**, which is the whole of what the person needs to
    // know before pressing: the machine is dropped and re-entered at the reset vector, nothing is
    // written, and this device's own last measured boot is what it will take again.
    if now.machine.is_some_and(|m| m == d.name) {
        return Row::go(verb, "restarts it — power off and straight back on, from the reset vector");
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
        // **§22.2: the machine in the way is not asked about.** `install_row` is handed `None`
        // where it used to be handed `now.machine`, and that one argument is the operator's own
        // sentence — *"i dont want to turn off the ipod to install rockbox"* — made mechanical.
        // The refusal existed because writing a drive an ARM7 is executing from is the one failure
        // here that damages something a person cannot rebuild; stopping the machine first removes
        // that, and stopping it is an act this program performs rather than an errand it sets.
        //
        // **The Devices page still asks**, and that is not an inconsistency: its `Install…` sits
        // inside a row about a device's parts, beside `Edit` and `Remove`, which are edits to a
        // library rather than a thing to run. This row is a thing to run, and §22.2 is about rows
        // that are.
        let fix = devices::install_row(s, d, caps, None);
        return Row {
            enabled: fix.enabled,
            reason: fix.reason,
            machine_rule: fix.machine_rule,
            escape: fix.escape,
            ..Row::go(
                Verb::Rockbox,
                match now.machine.filter(|m| *m == d.name) {
                    // What it costs, and every clause of it is true: the machine is dropped
                    // without a park (the drive is about to be replaced, so a restore point
                    // stamped against the old one could not be honoured — `Config::pair_is_whole`
                    // reads the drive's size and mtime), 9 MB is fetched, a new drive is written
                    // beside the old one, and the machine comes back at the reset vector.
                    Some(_) => "stops this iPod, downloads 9 MB, and starts it again on Rockbox — a cold boot",
                    None => "downloads 9 MB, writes a new drive beside this one, and starts it",
                },
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
    // **Rockbox first, and the sentence names the row that installs it.** `doom.rock` ships inside
    // the Rockbox release, so without Rockbox there is no plugin for the WADs to feed — and the
    // row above is one press from having one.
    //
    // **§22.2 does not reach this one, and the reason is the 24 MB rather than the principle.**
    // Chaining it is mechanically available — `work::Then` already carries one run into the next —
    // but the chain would be *install Rockbox, then fetch Freedoom, then boot*, which is 33 MB and
    // two writes behind a row whose sub-line promised 24. §10.1's rule is that a person agrees to
    // the whole plan before any of it runs, and a press that quietly grew by a third of its bill
    // is that rule broken from the inside. The row above states the same act, priced correctly.
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
    // **§22.2, and the running machine is the reason the sentence changes rather than the row.**
    // Writing 24 MB onto the volume an ARM7 is executing from is the hazard `devices::running_rule`
    // was refusing over; stopping the machine first removes the hazard, so the press stops it,
    // writes, and starts it again. What the person is owed is the cost of that, which is the boot
    // they were watching.
    Row::go(
        Verb::Doom,
        // **Short because §9.4's slot is 372 px and this one measured 411**, which is
        // `every_reason_this_window_draws_fits_the_slot_it_is_drawn_in` doing its job: the first
        // draft ended `— Doom is in Shortcuts` and would have been cut off mid-clause. Where Doom
        // is is on the other arm, which has the room; what this arm has to say is what happens to
        // the machine somebody is watching.
        match now.machine.filter(|m| *m == d.name) {
            Some(_) => "stops this iPod, downloads 24 MB, and starts it again",
            None => "downloads 24 MB, then boots Rockbox — Doom is in Shortcuts",
        },
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

/// §22.4's **one switch**, where §21.6 had five controls.
///
/// `Start · Suspend · Resume · Kill · Restart` was one control wearing five hats, four of them
/// greyed at any moment — and the operator, looking for a way to turn the iPod off, found
/// `Suspend` and `Kill` and concluded there was not one. `Kill` is what a process manager does. A
/// person's model of an iPod is **off** and **on**, and the drawn centre button already expresses
/// both, so the menu carries one row and it is a switch.
///
/// **Nothing is deleted but the rows.** Every act is still here and still reachable:
///
/// | §21.6 | where it went |
/// |---|---|
/// | `Start` | the switch, thrown on, over a machine with no restore point |
/// | `Resume` | the switch, thrown on, over one with a whole pair — `Restore::of` decides, not the person |
/// | `Suspend` | the switch, thrown off, while `Running` — `nav::Escape::Park` |
/// | `Kill` | the switch, thrown off, while `Booting` — `nav::Escape::PowerOff`, because parking a boot is a 1.6 GB write of a state nobody wants |
/// | `Restart` | the switch twice |
///
/// **The two keys keep their meanings and their column.** §16.8 gives `Esc` one definition
/// outwards and `nav::Stack::escape` ends it in `Park` from `Running` and `PowerOff` from
/// `Booting` — which is exactly this row's off position in its two live phases, so one row now
/// carries the key that used to be printed on two. The on position's is the drawn centre button,
/// as §7.3 has always had it.
///
/// **What the person is not asked.** Whether to suspend or to kill, and whether to start cold or
/// to resume, are both answered by facts the program already holds — the phase, and
/// `Config::pair_is_whole` by way of [`Restore::of`] — so the switch does the cheaper one and the
/// sub-line says which. That is §22.2's rule pointed at a control rather than at a refusal: a
/// question with a knowable answer is not a choice, it is a form.
///
/// **The one refusal left is `crate::blocked_label`'s**, and it is about files rather than about
/// the machine: an iPod whose boot ROM or drive is not on disk cannot be turned on by any act of
/// this program, because the bytes are not there to run.
fn power_row(d: Option<&Device>, now: Now) -> Row {
    let switch = |on: bool, label: &str, value: &str, sub: &str| Row {
        label: label.to_string(),
        value: value.to_string(),
        switch: true,
        on,
        ..Row::go(Verb::Power, sub)
    };
    // ── on ──────────────────────────────────────────────────────────────────────────────────────
    //
    // **`Esc`, in both live phases, because that is what `Esc` already does.** The sub-line is what
    // tells the two apart, which is what §21.6 said about printing the key on two rows and is now
    // true of one.
    match now.life {
        Life::Booting { .. } => {
            return switch(
                true,
                TURN_OFF,
                "Esc",
                "stops the boot. Nothing is written, so the next start is cold.",
            )
        }
        Life::Running { .. } => {
            // The size is the machine's own — `Link::snapshot_bytes`, published by the run loop
            // before anything can ask for a park — and never §12.4's measured literal, which was
            // true of one machine at one clock.
            let sub = match now.park_bytes.filter(|n| *n > 0) {
                Some(n) => format!(
                    "writes the restore point and stops — {}, and the next start reads it back",
                    ipod_machine::si(n)
                ),
                None => "writes the restore point and stops".into(),
            };
            return switch(true, TURN_OFF, "Esc", &sub);
        }
        Life::Off | Life::Stopped { .. } => {}
    }

    // ── off ─────────────────────────────────────────────────────────────────────────────────────
    //
    // §21.4: with no iPod, throwing the switch on is the first run, and it costs what `work::cost`
    // says rather than what a sentence here says — the same call the bench's shelf and the ledger
    // make, so three surfaces cannot print three bills for one press.
    let Some(d) = d else {
        return switch(false, TURN_ON, CENTRE, &first_run_cost());
    };
    // The bench's own words for a device whose parts are not on disk, fetched rather than
    // re-worded, so one iPod is not described two ways.
    let absent = now.settings.missing_with(d, &mut Presence::new());
    let mismatch = now.settings.generation_mismatch(d);
    if let Some(b) = machine::Blocked::of(Some(d), &absent, mismatch.as_deref()) {
        return Row {
            value: CENTRE.into(),
            switch: true,
            on: false,
            ..Row::no(
                Verb::Power,
                crate::blocked_label(crate::Press::Centre, d, &absent, b, mismatch.as_deref()),
                b.machine_rule(),
            )
        };
    }
    // **A stopped machine starts cold, and that is `machine::centre`'s rule rather than a second
    // one here**: its `Stopped` arm answers `Launch::Cold` even over a perfectly good snapshot,
    // because pressing after a `Lost(0xe19b0000)` starts again rather than restoring the state
    // that died. `Life::Off` is the other half — the thread is parked at `wait_for_power`, the
    // press falls through `on_start_device` to `start_machine`, and a machine built afresh is
    // `Config::may_restore`'s `first`.
    let cfg = crate::resting_config(now.settings, d);
    let sub = match (now.life, Restore::of(&cfg)) {
        (Life::Stopped { .. }, _) => "cold boot, from the reset vector".into(),
        (_, Restore::Whole) => {
            // The file a resume will read, `stat`ed — the currency the on position reports in, and
            // never §7.3's retired `about 3 s`, which was a figure about one host at one clock.
            match cfg.snapshot.and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()) {
                Some(n) => format!(
                    "puts the machine back where it was — {} to read",
                    ipod_machine::si(n)
                ),
                None => "puts the machine back where it was".into(),
            }
        }
        // **A broken pair is a cold boot and says so, rather than refusing.** §7.3's own reading of
        // the same fact: the snapshot is on disk and no longer describes this drive, so it is not
        // used. Nothing about that stops the iPod turning on, which is why it is a sub-line and not
        // a reason.
        (_, Restore::Broken) => "cold boot — the restore point no longer matches this drive".into(),
        (_, Restore::Never) => "cold boot, from the reset vector".into(),
    };
    switch(false, TURN_ON, CENTRE, &sub)
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
