//! The Rail: where the program narrates what it is doing and where it fails.
//!
//! `docs/GUI.md` §9.2, §9.3 and §20 item 12 — *the Rail exists before the first button is wired*,
//! because every action about to be reconnected needs somewhere to fail and principle 5 forbids the
//! easy answer. Today the easy answer is what is there: `on_start_device` is an `eprintln!`, and a
//! device whose ROM or drive has left the library is refused by the model with nobody to say so.
//!
//! **There is no toolkit in this file**, and none may enter it. It does not name `RailRow`,
//! `RailKind` or any other generated type — `main.rs` converts, and that is what keeps the window
//! replaceable (`AGENTS.md` §9). Everything here is testable with no display.
//!
//! **Nothing expires on a timer.** A failure stays until it is dismissed, individually. §14.4 bans
//! the self-dismissing message by name and this module gives it nowhere to live: there is no clock
//! in it, no time-to-live on an [`Entry`], and no method that drops one without being asked.

use std::path::PathBuf;

use eapp_loader::compose;

/// Bytes, in the units a person reads. **One formatter, and this is a re-export rather than a copy.**
///
/// It used to be a second implementation, byte for byte identical to `eapp_loader::si` and sitting
/// two hundred lines below this one. Nothing had gone wrong yet, which is the only interesting thing
/// about it: `Entry::measure` and `Entry::cancel_cost` render here, `compose::Step::sub` renders in
/// the model — `21 MB` is built inside eapp-loader and cannot reach a formatter in this crate — and
/// `push_ledger` renders in `main.rs`. Four surfaces, one number, and two functions that could drift
/// on any edit to either. Two names for one number is how they come to disagree.
use eapp_loader::si;

/// What an entry is doing. §9.2's states, plus the two an entry can end in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// On the plan, not started. §11.3: the plan is one list rendered twice.
    Planned,
    Working,
    Done,
    Failed,
    Cancelled,
    /// Not a step: something the program wants to say. First-run notes live here.
    Note,
}

/// How far along, and **whether there is a denominator at all**.
///
/// §12.3: a bar with no denominator is a bar that lies. Where there is none there is a number that
/// moves and no bar — which is also why [`Entry::fraction`] hands back a negative rather than a
/// zero, since 0.0 is a legitimate fraction and "no fraction" is not.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Progress {
    None,
    /// Real bytes, both halves. §9.2: the measure is `4.1 MB of 6.5 MB`, never a percentage alone.
    Bytes { done: u64, total: u64 },
    /// A fraction somebody computed, where there are no bytes to count.
    #[allow(dead_code)]  // retired when: a step reports progress that is not a byte count — the install of a directory tree, Phase 6
    Fraction(f32),
}

/// A tool this program shells out to, and which capability it gates.
///
/// **Named individually because the remedy is**: §9.3 gives each of the three its own command, and
/// a bare "install the tool" is not a remedy — it is the shape of one. `Class::ToolMissing` carries
/// which, so [`Class::mono_remedy`] can print something a person can paste.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    /// Every download.
    Curl,
    /// ZeroSlackr only.
    #[allow(dead_code)]  // retired when: the iPodLinux install lands — it is what unpacks a `.7z`
    SevenZip,
    /// GIFs only.
    #[allow(dead_code)]  // retired when: `ipod-film`'s GIF path reaches the window
    Ffmpeg,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Curl => "curl",
            Tool::SevenZip => "7z",
            Tool::Ffmpeg => "ffmpeg",
        }
    }

    /// The named remedy, in `mono`. A real command on both platforms this program is used on.
    ///
    /// ASCII, and no `·` between the two: §6.7 treats a middle dot as a symbol, and the answer for a
    /// symbol is that it is drawn as a `Path`. Rust has no `Path`, so Rust says the words.
    pub fn remedy(self) -> &'static str {
        match self {
            Tool::Curl => "brew install curl on macOS, apt install curl on Debian",
            Tool::SevenZip => "brew install p7zip on macOS, apt install p7zip-full on Debian",
            Tool::Ffmpeg => "brew install ffmpeg on macOS, apt install ffmpeg on Debian",
        }
    }
}

/// §9.3's table. **Ten classes**, and the tenth is the one this phase exists for.
///
/// `Missing` — *a part of this device is no longer on disk* — is §20 item 1's refusal, and §20 item
/// 12 requires the Rail to carry it. None of the nine in the document covers it, and filing it
/// under `permission` would be the program asserting a fact about somebody's filesystem it did not
/// observe.
///
/// The class decides the wording and the next steps. It does **not** decide the sentence a
/// [`Failure`] shows when there are numbers in it — see [`Failure::saying`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Class {
    Network,
    NotServed,
    Verification,
    Incompatible(compose::Fix),
    SpacePreflight,
    SpaceMidWrite,
    Volume,
    Permission,
    ToolMissing(Tool),
    /// A part this device names is no longer where it was. §3.3, §20 items 1 and 12.
    Missing,
}

impl Class {
    /// Every class, by name, in declaration order. **A closed set of ten.**
    ///
    /// The length is written into the type, so an eleventh variant stops the crate compiling until
    /// somebody decides what it is called — and `every_failure_class_carries_a_next_step_and_its_own_words`
    /// then fails until it is swept too.
    #[allow(dead_code)]  // retired when: a caller outside this module needs the closed set by name; the sweep in this file's own tests does not count in a non-test build
    pub const ALL: [&'static str; 10] = [
        "network",
        "not served",
        "verification",
        "incompatible",
        "space, pre-flight",
        "space, mid-write",
        "volume",
        "permission",
        "tool missing",
        "missing",
    ];

    #[allow(dead_code)]  // retired when: a caller outside this module needs a class as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            Class::Network => Class::ALL[0],
            Class::NotServed => Class::ALL[1],
            Class::Verification => Class::ALL[2],
            Class::Incompatible(_) => Class::ALL[3],
            Class::SpacePreflight => Class::ALL[4],
            Class::SpaceMidWrite => Class::ALL[5],
            Class::Volume => Class::ALL[6],
            Class::Permission => Class::ALL[7],
            Class::ToolMissing(_) => Class::ALL[8],
            Class::Missing => Class::ALL[9],
        }
    }

    /// The program's own words for this class, when nothing was measured to put in them.
    ///
    /// §9.3's table, and the ones with numbers in them get their sentence built by whoever has the
    /// numbers — [`Failure::saying`]. This is what a failure says when nobody did, and it is never
    /// empty, because an entry that says nothing is the "it boots to a white screen" non-diagnosis
    /// §20 item 1 exists to delete.
    pub fn wording(&self) -> String {
        match self {
            Class::Network => "Apple's server did not answer.".into(),
            Class::NotServed => "Apple no longer serves this release (403). Five of the 71 are \
                                 refused; that is a fact about Apple's servers, not about your \
                                 network."
                .into(),
            Class::Verification => "The SHA-256 does not match the one on record. That is \
                                    interesting and should not be shrugged off."
                .into(),
            // `Verdict::No.why`, verbatim, is what a caller passes; this is the shape of it when
            // the caller had no verdict to hand.
            Class::Incompatible(fix) => format!(
                "That combination cannot work as it stands. One change resolves it: {}.",
                fix.label()
            ),
            Class::SpacePreflight => "There is not enough room on that volume. Nothing has been \
                                      written."
                .into(),
            Class::SpaceMidWrite => "The volume filled up part way through, and the partial file \
                                     is still there."
                .into(),
            Class::Volume => "That folder is on a FAT32 volume. FAT32 cannot hold a file larger \
                              than 4 GiB and has no sparse files, so an 8 GiB drive image would be \
                              written in full and would stop at exactly 4 294 967 296 bytes."
                .into(),
            Class::Permission => "This program is not allowed to write there.".into(),
            Class::ToolMissing(t) => format!(
                "{} is not on the path, and it is what this step runs.",
                t.name()
            ),
            Class::Missing => "A part this device is made of is no longer where it was.".into(),
        }
    }

    /// The named remedy in `mono`, for the one class that has no next step.
    ///
    /// Empty for every other class, and [`Class::next`] is empty for exactly this one — the two
    /// halves of §9.3's last row, kept in one place so they cannot disagree.
    ///
    /// **It reaches the screen**, through `RailRow::mono`. It used not to: the struct had no field
    /// for it and `to_row` never called this, so §9.3's last row would have rendered as a paragraph,
    /// two invisible controls and `Dismiss` — with the one thing a person could actually do about it
    /// never crossing the boundary. `every_failure_class_carries_a_next_step_and_its_own_words` was
    /// green throughout, because it called this directly.
    pub fn mono_remedy(&self) -> String {
        match self {
            Class::ToolMissing(t) => t.remedy().to_string(),
            _ => String::new(),
        }
    }

    /// §9.3's next-step column, which is the deliverable that table asks for.
    ///
    /// `retries` is how many times this exact step has already been retried, and it is why
    /// `Verification` stops offering `Retry`: `firmware.rs`'s present-but-wrong path prints
    /// *"already here but does not verify — downloading again"* and loops for as long as a mirror
    /// serves the wrong bytes.
    ///
    /// A step whose mechanism this build does not have is still returned — [`Next::available`] is
    /// what makes it a disabled control with a reason, rather than an absent one. §14.1: hiding it
    /// is the convention everywhere else and it is refused here.
    pub fn next(&self, retries: u8, caps: Caps) -> Vec<Next> {
        // **Nothing is filtered out here, and the parameter is kept to say so.** §14.1 refuses the
        // hide-don't-disable convention: a step this build cannot take is still returned, and
        // [`Next::available`] is what turns it into a disabled control wearing its reason. If this
        // ever starts dropping steps, the reason goes here beside the drop.
        let _ = caps;
        match self {
            Class::Network => vec![Next::Retry, Next::Provide],
            Class::NotServed => vec![Next::Provide],
            Class::Verification => {
                if retries == 0 {
                    vec![Next::Retry, Next::CopyDetails]
                } else {
                    vec![Next::Provide, Next::CopyDetails]
                }
            }
            // §11.3: two presses where it detaches a resource. A `Fix` that names a value the
            // picker disables is itself disabled, and that is the caller's business — the value it
            // would set is not known here.
            Class::Incompatible(fix) => vec![Next::Fix {
                label: fix.label(),
                presses: if *fix == compose::Fix::BuildFromIpsw { 2 } else { 1 },
            }],
            Class::SpacePreflight => vec![Next::ChooseElsewhere],
            Class::SpaceMidWrite => vec![Next::ChooseElsewhere, Next::CancelWrite],
            Class::Volume => vec![Next::ChooseElsewhere],
            Class::Permission => vec![Next::Reveal],
            // **None, on purpose.** The remedy is a command, not a control — nothing this window
            // can press installs a tool. `mono_remedy` is the other half.
            Class::ToolMissing(_) => Vec::new(),
            Class::Missing => vec![Next::Provide, Next::Devices],
        }
    }
}

/// One control offered under a failure.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Next {
    Retry,
    Provide,
    ChooseElsewhere,
    CopyDetails,
    Reveal,
    CancelWrite,
    /// Take me to the device this is about.
    Devices,
    Fix {
        label: String,
        presses: u8,
    },
}

impl Next {
    /// **Short enough for half a failure block**, which is a measurement and not a preference.
    /// Six of the ten failure classes offer two next steps and `geometry::RAIL_NEXT_W` makes each
    /// one exactly half the row, so a label is drawn in the same 146 px a reason is. The two that
    /// were sentences — `Provide the file yourself…` and `Choose somewhere else…` — drew as
    /// *Provide the file you…* and lost their own trailing `…`, which is the one character that
    /// says a picker opens. §11.4 already calls the same act `Provide…` on Parts; these are that
    /// name with the object put back.
    pub fn label(&self) -> String {
        match self {
            Next::Retry => "Retry".into(),
            Next::Provide => "Provide a file…".into(),
            Next::ChooseElsewhere => "Choose a folder…".into(),
            // **Never `Report it`.** There is no network reporting path in this program and no
            // issue URL in this design, and a visible control that does nothing is the defect this
            // document indicts twice.
            Next::CopyDetails => "Copy the details".into(),
            Next::Reveal => "Reveal".into(),
            Next::CancelWrite => "Cancel".into(),
            Next::Devices => "Devices".into(),
            Next::Fix { label, .. } => label.clone(),
        }
    }

    /// Whether this build can actually do the thing the control claims.
    ///
    /// **Advertising a mechanism that does not exist is the first of §19.1's fatal findings in
    /// miniature**, so every route out of here is a real one or the control is disabled and says
    /// which. Two of them are false in this phase and both are named in [`Caps`].
    pub fn available(&self, caps: Caps) -> bool {
        match self {
            // A file arrives either through a picker or by being dropped on the window; either one
            // is enough, and in this phase there is neither.
            Next::Provide => caps.file_picker || caps.drop_target,
            Next::ChooseElsewhere => caps.file_picker,
            Next::CopyDetails => caps.clipboard,
            Next::Reveal => caps.reveal,
            Next::Devices => caps.devices_page,
            // **Retrying is downloading.** Every class that offers `Retry` — `Network`, and
            // `Verification` on its first failure — failed while fetching something, so the control
            // is exactly as real as the fetcher is. On a computer with no `curl` it is drawn
            // disabled wearing that sentence, and `Class::ToolMissing`'s `mono_remedy` is the other
            // half.
            Next::Retry => caps.download,
            // A `Fix` rewrites a recipe, so it needs a surface that holds one. This build has one
            // — `Page::Composer` answers `Some(2)` and `main::caps()` derives the flag from that —
            // so the cap is `true` here and the sentence below is drawn only by a fixture.
            Next::Fix { .. } => caps.composer,
            // Cancelling really is this program talking to itself: the file is this program's, it
            // is on this computer, and `cancel_write` is wired. It needs no capability.
            Next::CancelWrite => true,
        }
    }

    /// Why it is not pressable — **non-empty exactly for the steps [`Next::available`] can refuse**.
    ///
    /// §9.4: a project state says *this is not finished, by us*, names what does work, and names the
    /// escape hatch. Every sentence below is the second kind but one; that one says so.
    ///
    /// **One clause each, and that is a rule with a measurement behind it.** These are drawn in
    /// §9.3's next-step pair, which is `geometry::REASON_MEASURE` 146 px wide — the narrowest
    /// column in the program that draws a reason — in a slot that **elides and cannot wrap**. What
    /// shipped here was a sentence: *there is no file picker in this build yet, and nothing here
    /// accepts a dropped file* measures **411 px**, so what a person actually read was
    /// *there is no file picker in this build yet, and not…*, and §9.4's whole argument for
    /// disabling a control rather than hiding it is that the control says why. §17.Q1 refused the
    /// other answer for the shelf and it is refused here for the same reason: 34 px of permanent
    /// chrome will not hold a paragraph, so the paragraph is not what goes in it.
    ///
    /// Every string below was measured with the renderer, not counted:
    /// `every_reason_this_window_draws_fits_the_slot_it_is_drawn_in` fails on a longer one.
    ///
    /// **What the shortening dropped, and where it went.** The clauses that named the *second* half
    /// of an absence — *and nothing here accepts a dropped file*, *every download in this program
    /// goes through curl* — are gone from the screen. There is no `why ›` route to put them behind:
    /// that control is the bench's (§7.6) and opens the drawer page owning a refusal, and these
    /// refusals are already **on** their drawer page. The long form survives in these doc comments
    /// and in §9.4, which is where a person who wants the whole argument reads it, and the drawn
    /// half is the half a person standing at the control needs.
    pub fn reason(&self) -> &'static str {
        match self {
            Next::Provide => "no file picker in this build",
            Next::ChooseElsewhere => "no folder picker yet",
            Next::CopyDetails => "this build has no clipboard",
            Next::Reveal => "no file manager here",
            Next::Devices => "no Devices page yet",
            // **A machine rule, not a project state**, and the only one in this function. The other
            // five say *we have not finished this*; this one says *your computer cannot do it*, and
            // the difference matters because no amount of work on this program fixes it. The remedy
            // is `Class::ToolMissing`'s command, not a control.
            Next::Retry => "no curl on this computer",
            // **This one is unreachable in the shipping build, and it is kept rather than
            // reworded.** `available` asks `caps.composer`, `main::caps()` derives that from
            // `nav::Page::Composer.slot()`, and that answers `Some(2)` unconditionally — so nothing
            // a person can see draws this sentence. The all-off fixture does, which is what keeps
            // it swept by `the_preview_fixture_draws_a_disabled_next_step_with_its_reason`.
            //
            // It is kept because it is **correct whenever it is shown**, and there is exactly one
            // condition that shows it: `Page::Composer` losing its slot, which is a fact about the
            // build and is the one thing `caps.composer` is derived from. A cap derived from a
            // question needs the answer to that question worded for both outcomes, or the day the
            // page is removed the control greys out saying nothing. The alternative — replacing it
            // with a reason a `Fix` can be refused for *today* — has nothing to say: `available` is
            // the only thing that refuses a `Fix`, and this is what `available` refuses on.
            Next::Fix { .. } => "no Composer in this build",
            Next::CancelWrite => "",
        }
    }

    /// The command that does the same job from a terminal, when there is one.
    ///
    /// **Empty unless it is real, and §9.4's rule is that a project state always names one.**
    /// Three of these had none, so `verification` after one retry, a live `403` and a `permission`
    /// failure each resolved to nothing but two greyed controls: the reason said *this is not
    /// finished, by us* and then offered no way round it, which is a dead end wearing an apology.
    /// Every command below exists in `ipod-boot` today and was run to check it.
    ///
    /// The one that stays empty is [`Next::Retry`], and that is a different kind of absence: no
    /// command in this program downloads a `.ipsw` without `curl` — `firmware::download` **is**
    /// `curl` — so naming one would be the phantom route in its original shape, offered on the one
    /// failure where a person is least able to check. `Class::ToolMissing`'s `mono_remedy` is what
    /// that failure carries instead.
    pub fn escape_hatch(&self, caps: Caps) -> String {
        if self.available(caps) {
            return String::new();
        }
        match self {
            Next::ChooseElsewhere => "IPOD_EMULATOR_DATA=<path>".into(),
            Next::Devices => "ipod-boot setup".into(),
            // `ipod-boot setup` composes a recipe from a terminal, which is exactly the job a
            // Composer would do from the window.
            Next::Fix { .. } => "ipod-boot setup".into(),
            // **Providing the file yourself, from a terminal.** `firmware get` takes an
            // `UpdaterFamilyID` or a filename and lands the bundle in the same cache directory this
            // program reads, which is precisely what a picker would have done. `<family>` is a
            // placeholder in the shape `IPOD_EMULATOR_DATA=<path>` already uses; `firmware list`
            // is how you find out which number to put there.
            Next::Provide => "ipod-boot firmware get <family>".into(),
            // **The details, from a terminal.** `Copy the details` puts the release, its sizes and
            // both hashes on a clipboard this build has not got; `firmware cache --verify` hashes
            // every cached bundle and prints exactly that, which is what a report about a SHA-256
            // mismatch needs.
            Next::CopyDetails => "ipod-boot firmware cache --verify".into(),
            // **`Reveal` is offered on `permission` alone**, and what a person does after being
            // shown a folder they cannot write in is put this program's files somewhere else. That
            // is one variable, read by `settings::data_dir`, and it is the same escape
            // `ChooseElsewhere` carries — the two controls are the same remedy seen from two
            // failures.
            Next::Reveal => "IPOD_EMULATOR_DATA=<path>".into(),
            Next::Retry => String::new(),
            Next::CancelWrite => String::new(),
        }
    }

    /// §11.3: two presses where the press detaches a resource, one everywhere else.
    pub fn presses(&self) -> u8 {
        match self {
            Next::Fix { presses, .. } => *presses,
            _ => 1,
        }
    }

    /// What pressing it will do, said **before** the press rather than after.
    ///
    /// **Eight sentences, all of them re-worded to the slot they are drawn in**, which is §9.3's
    /// next-step pair at `geometry::REASON_MEASURE` 146 px — the narrowest in the program. Every
    /// one of them elided: `_out/gui/work-failed.png` drew *Runs this step again, fro…* under a
    /// live blue `Retry`, and nothing in the suite could see it, because
    /// `every_reason_this_window_draws_fits_the_slot_it_is_drawn_in` swept [`Next::reason`] and
    /// not this. One slot, two producers, one of them measured.
    ///
    /// **`Fix` no longer repeats its own label**, and that is the same rule `devices::start_row`
    /// wrote down: putting a fact in this slot that the reader can already see spends the one line
    /// the control has and loses the one they cannot. The button says
    /// *build from Apple's firmware instead*; the sentence under it said
    /// *Changes the recipe to build from Apple's firmware instead* — 310 px of it, in 146 — so it
    /// says what changes and lets the button say to what.
    pub fn consequence(&self) -> String {
        match self {
            Next::Retry => "Runs the step again.".into(),
            Next::Provide => "Uses a file you have.".into(),
            Next::ChooseElsewhere => "Picks another folder.".into(),
            // The long form enumerated the release, the URL, both sizes, both hashes and the
            // platform. Seven nouns is a list, and a list is what the clipboard is for.
            Next::CopyDetails => "URL, sizes and hashes.".into(),
            Next::Reveal => "Opens a file manager.".into(),
            Next::CancelWrite => "Deletes the partial file.".into(),
            Next::Devices => "Opens its device page.".into(),
            Next::Fix { presses, .. } => {
                if *presses == 2 {
                    "Press again; the old goes.".into()
                } else {
                    "Rewrites the recipe.".into()
                }
            }
        }
    }
}

/// What this build can actually do, decided in `main.rs` and passed in.
///
/// **Seven booleans, not the three the design first named**, and every one past the third is the
/// same rule applied again: `Reveal` needs a file manager this build has no way to open, `Devices`
/// needs a drawer page, `Retry` needs a downloader, and `Fix` needs a surface that holds a recipe.
/// A control whose mechanism does not exist is disabled and says so; it is never drawn live and
/// never quietly dropped.
///
/// **Two of the seven have since been earned, and this paragraph is not the place their answer
/// lives.** The Devices page and the Composer both ship now, so `devices_page` and `composer` are
/// `true` in the running program. Neither is typed as `true`: `main::caps()` derives both from
/// `Page::slot()`, so the flag and the markup cannot disagree, and a sentence here saying *not
/// written* would be the §16.9 defect all over again.
///
/// **Four of the seven are literals, two are derived, and one is measured.** `download` is the odd
/// one: every download in this program goes through `curl`, and whether `curl` is on this computer
/// is a fact about the computer rather than about the build — so `main::caps()` asks
/// `eapp_loader::tooling::can_download()` once per launch.
///
/// **There is deliberately no `build` cap.** It would be a lie the moment this phase landed: this
/// build *can* build a drive, and a boolean claiming otherwise would disable the one control §10
/// exists to make pressable.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Caps {
    pub file_picker: bool,
    pub drop_target: bool,
    pub clipboard: bool,
    pub reveal: bool,
    /// The drawer's Devices page. **Derived, never declared** — `main::caps()` asks
    /// `Page::Devices.slot()`, which answers `Some(1)` the day `ui/drawer.slint` gains a child that
    /// draws the page. All four of the depth-1 pages answer `Some(1)` today, so this is `true` in
    /// the running program and `false` only in the all-off fixture the disabled-control sweep needs.
    pub devices_page: bool,
    /// Whether anything on this computer can fetch a file. §10.4: *every download in this program
    /// goes through curl.* **Probed once per launch, never assumed** — the other six are facts
    /// about the build and this one is a fact about the machine it is running on.
    pub download: bool,
    /// The Composer. A [`Next::Fix`] changes a recipe, and this build has a surface that holds one:
    /// `Page::Composer` answers `Some(2)`, `main::caps()` derives this from that answer, and
    /// `take_next_step` has a `Fix` arm that pushes the page. So the control is drawn **live**.
    ///
    /// **Live is not the same as reachable, and the difference is written down rather than
    /// implied.** Nothing this build files produces a `Class::Incompatible`, so no `Fix` is ever
    /// drawn for a person to press; `main.rs` records that beside the arm. What this flag decides
    /// is only whether the control would be pressable if one were drawn — and `false`, in the
    /// all-off fixture, is what keeps the disabled sentence below swept.
    pub composer: bool,
}

/// A failure, in three parts: what was attempted, what happened, and which kind of wrong it is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Failure {
    pub class: Class,
    /// What the program was trying to do. Short — the entry's own `verb` and `what` carry the rest.
    pub attempted: String,
    /// **The program's own words, and nobody re-words them on the way to the screen.** A model
    /// sentence — `Verdict::No.why`, an `io::Error`, `Absent::label` — arrives here verbatim.
    pub said: String,
}

impl Failure {
    /// A failure whose sentence is the class's own.
    pub fn new(class: Class, attempted: impl Into<String>) -> Failure {
        Failure {
            said: class.wording(),
            class,
            attempted: attempted.into(),
        }
    }

    /// A failure whose sentence somebody measured — the two space classes, an incompatibility, a
    /// missing part that can name the file.
    pub fn saying(class: Class, attempted: impl Into<String>, said: impl Into<String>) -> Failure {
        Failure {
            class,
            attempted: attempted.into(),
            said: said.into(),
        }
    }
}

/// One line of the Rail.
#[derive(Clone, PartialEq, Debug)]
pub struct Entry {
    pub id: u64,
    /// `Step::verb()`, or the verb of an action that was never a step.
    pub verb: String,
    /// `Step::what()`.
    pub what: String,
    /// The detail line: the release, the byte count, where it came from.
    pub sub: String,
    pub kind: Kind,
    pub progress: Progress,
    /// How many times this step has been retried. `Class::next` reads it.
    pub retries: u8,
    /// §12.7: the file cancelling deletes. `Rail::cancel` hands it back; **this module never
    /// touches the filesystem** — the caller does, having said which file and how big it is.
    pub temp: Option<PathBuf>,
    pub failure: Option<Failure>,
    pub dismissible: bool,
    pub cancellable: bool,
}

impl Entry {
    /// `0.0..=1.0`, or **negative where there is no denominator** — which is not the same as zero
    /// and must not be drawn as an empty bar.
    pub fn fraction(&self) -> f32 {
        match self.progress {
            Progress::None => -1.0,
            Progress::Bytes { total: 0, .. } => -1.0,
            Progress::Bytes { done, total } => (done as f32 / total as f32).clamp(0.0, 1.0),
            Progress::Fraction(f) => f.clamp(0.0, 1.0),
        }
    }

    /// `4.1 MB of 6.5 MB` — **real bytes, both halves**. Empty unless this entry is working.
    pub fn measure(&self) -> String {
        if self.kind != Kind::Working {
            return String::new();
        }
        match self.progress {
            Progress::None => String::new(),
            Progress::Bytes { done, total: 0 } => si(done),
            Progress::Bytes { done, total } => format!("{} of {}", si(done), si(total)),
            Progress::Fraction(f) => format!("{:.0} %", (f.clamp(0.0, 1.0) * 100.0)),
        }
    }

    /// §12.7: which file cancelling deletes, and how big it is **right now**.
    pub fn cancel_cost(&self) -> String {
        let Some(p) = self.temp.as_ref() else {
            return String::new();
        };
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.display().to_string());
        match self.progress {
            Progress::Bytes { done, .. } if done > 0 => {
                format!("{name} is {} and cancelling deletes it", si(done))
            }
            _ => format!("cancelling deletes {name}"),
        }
    }

    /// What the entry says happened. Empty unless it failed.
    pub fn happened(&self) -> String {
        self.failure.as_ref().map(|f| f.said.clone()).unwrap_or_default()
    }
}

/// Every entry, in the order they were filed.
///
/// **It does not grow without bound**, and that is a mechanism rather than a hope: a new plan
/// collapses the previous one's finished steps into one `Note`. Nothing is dropped on a timer and
/// nothing scrolls itself.
#[derive(Debug, Default)]
pub struct Rail {
    next_id: u64,
    entries: Vec<Entry>,
}

impl Rail {
    // **The producer landed, and the allows came off with it.** `work.rs` is the work queue this
    // module was written ahead of: it files the plan, reports bytes, names the file it is writing,
    // fills the detail line, finishes a step and fails one. Thirteen of the sixteen
    // `#[allow(dead_code)]` markers here named *"when `work.rs` lands"* as their retirement
    // condition; that has happened, so they are gone rather than left standing.
    //
    // Four remain, each on the one item that is still genuinely unreferenced, each carrying what
    // would retire it. That is the difference the original note asked for: anything still
    // unreferenced now is either named or deleted.

    pub fn new() -> Rail {
        Rail::default()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn find(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    fn mint(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn at(&mut self, id: u64) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// **§11.3's plan, and it is the model's own list.** Mapped one to one, with no string built
    /// here: the verb is `Step::verb()`, the subject is `Step::what()` and the detail line is
    /// `Step::sub()`. One list, rendered twice — as what will happen, and as what is happening.
    ///
    /// **It takes the steps rather than a `Recipe`**, because a first run's plan is the recipe's
    /// steps with a boot ROM in front and a cold boot behind, and a `Recipe` knows about neither.
    ///
    /// Every entry is `Planned` with `Progress::None`, so `Entry::fraction()` is negative and no
    /// bar is drawn on any of them: §9.2's *never a spinner* holds by construction rather than by
    /// the caller remembering.
    pub fn plan(&mut self, steps: &[compose::Step]) -> Vec<u64> {
        self.collapse_finished();
        steps
            .iter()
            .map(|s| {
                let id = self.mint();
                self.entries.push(Entry {
                    id,
                    verb: s.verb().to_string(),
                    what: s.what().to_string(),
                    sub: s.sub().to_string(),
                    kind: Kind::Planned,
                    progress: Progress::None,
                    retries: 0,
                    temp: None,
                    failure: None,
                    dismissible: false,
                    cancellable: false,
                });
                id
            })
            .collect()
    }

    /// The previous plan's finished steps become one line, so the Rail cannot grow for ever.
    ///
    /// Only `Done` entries — a failure stays until it is dismissed and a cancellation is a fact
    /// somebody may want to read twice.
    fn collapse_finished(&mut self) {
        let done = self.entries.iter().filter(|e| e.kind == Kind::Done).count();
        if done == 0 {
            return;
        }
        self.entries.retain(|e| e.kind != Kind::Done);
        let id = self.mint();
        self.entries.insert(
            0,
            Entry {
                id,
                verb: String::new(),
                what: format!(
                    "{done} step{} finished",
                    if done == 1 { "" } else { "s" }
                ),
                sub: String::new(),
                kind: Kind::Note,
                progress: Progress::None,
                retries: 0,
                temp: None,
                failure: None,
                dismissible: true,
                cancellable: false,
            },
        );
    }

    /// Something the program wants to say that is not a step.
    ///
    /// **A note that repeats the last one is not filed twice**, and it returns the existing id. The
    /// module's own claim that the Rail *"does not grow without bound, and that is a mechanism
    /// rather than a hope"* was true of no code path: [`Rail::collapse_finished`] is called only
    /// from [`Rail::plan`], which nothing in this build calls, and it collapses only `Done`. So
    /// pressing the centre button N times appended N identical entries, each needing its own
    /// dismissal. Consecutive-identical is the honest bound — two *different* notes are two things
    /// that happened and both stay.
    pub fn note(&mut self, text: &str) -> u64 {
        if let Some(last) = self.entries.last() {
            if last.kind == Kind::Note && last.what == text {
                return last.id;
            }
        }
        let id = self.mint();
        self.entries.push(Entry {
            id,
            verb: String::new(),
            what: text.to_string(),
            sub: String::new(),
            kind: Kind::Note,
            progress: Progress::None,
            retries: 0,
            temp: None,
            failure: None,
            dismissible: true,
            cancellable: false,
        });
        id
    }

    /// A planned or working step failed.
    pub fn fail(&mut self, id: u64, f: Failure) {
        if let Some(e) = self.at(id) {
            e.kind = Kind::Failed;
            e.failure = Some(f);
            e.dismissible = true;
            e.cancellable = false;
        }
    }

    /// A failure with no step in front of it — the centre button refusing, a tool that is not there.
    ///
    /// **The same failure, filed twice in a row, is one entry**, and it returns the existing id. The
    /// centre button and `why ›` both file this, and pressing either twice used to stack two copies
    /// of one sentence, each needing its own dismissal. Same rule as [`Rail::note`], for the same
    /// reason: two *different* failures are two things that went wrong and both stay until they are
    /// dismissed individually (§9.3). Nothing here expires on a timer, ever.
    pub fn failed(&mut self, verb: &str, what: &str, f: Failure) -> u64 {
        if let Some(last) = self.entries.last() {
            if last.kind == Kind::Failed
                && last.verb == verb
                && last.what == what
                && last.failure.as_ref() == Some(&f)
            {
                return last.id;
            }
        }
        let id = self.mint();
        self.entries.push(Entry {
            id,
            verb: verb.to_string(),
            what: what.to_string(),
            sub: String::new(),
            kind: Kind::Failed,
            progress: Progress::None,
            retries: 0,
            temp: None,
            failure: Some(f),
            dismissible: true,
            cancellable: false,
        });
        id
    }

    /// Move a step along. **The only method that does not change what the Rail announces.**
    pub fn progress(&mut self, id: u64, p: Progress) {
        if let Some(e) = self.at(id) {
            e.kind = Kind::Working;
            e.progress = p;
        }
    }

    /// Say which file this step is writing, so cancelling can say what it costs.
    pub fn writing(&mut self, id: u64, temp: PathBuf) {
        if let Some(e) = self.at(id) {
            e.temp = Some(temp);
            e.cancellable = true;
        }
    }

    /// Set the detail line — the release, where it came from, what was checked.
    pub fn detail(&mut self, id: u64, sub: &str) {
        if let Some(e) = self.at(id) {
            e.sub = sub.to_string();
        }
    }

    pub fn done(&mut self, id: u64) {
        if let Some(e) = self.at(id) {
            e.kind = Kind::Done;
            e.cancellable = false;
            e.dismissible = false;
            e.temp = None;
        }
    }

    /// Retry a failed step: back to planned, and **the count goes up**, which is what stops
    /// `Verification` offering `Retry` for ever.
    pub fn retry(&mut self, id: u64) -> bool {
        let Some(e) = self.at(id) else { return false };
        if e.kind != Kind::Failed {
            return false;
        }
        e.retries = e.retries.saturating_add(1);
        e.kind = Kind::Planned;
        e.failure = None;
        e.dismissible = false;
        true
    }

    /// Take one entry off the Rail. Individually — nothing dismisses a neighbour and nothing
    /// dismisses itself.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|e| !(e.id == id && (e.dismissible || e.kind == Kind::Failed)));
        self.entries.len() != before
    }

    /// Stop a write. **Returns the file the caller must delete** — this module does not delete
    /// anything, because `AGENTS.md` §3 makes that the operator's decision and `cancel_cost` is
    /// where they were told which file and how big.
    ///
    /// **Two entries can release a file, and the second one is why this is not just `cancellable`.**
    /// A step that is *writing* is the obvious one. The other is a step that **failed while
    /// writing**: [`Rail::fail`] clears `cancellable` — correctly, since there is no longer a write
    /// to stop — but it does **not** clear `temp`, because the partial file is still on the disk.
    /// `Class::SpaceMidWrite` offers `Next::CancelWrite` on exactly such an entry, and its whole job
    /// is *delete the partial this step left*. Gated on `cancellable` alone it could never do it:
    /// the path was right there in `temp` and unreachable, so the one control offered under *the
    /// disk filled up* was guaranteed to do nothing. That is the visible-control-that-does-nothing
    /// defect, arrived at from the opposite direction — not an unwired arm, but a wired arm the
    /// model refused to serve.
    ///
    /// **A failed entry stays failed.** It does not become `Cancelled`, and the [`Failure`] on it is
    /// not cleared: releasing the file is not a retraction of what went wrong, and a person who
    /// presses `Cancel` under *the disk filled up* must not thereby lose the sentence explaining it,
    /// nor the other next step beside it. Only the file is given up, and `temp.take()` is what makes
    /// pressing twice honest — the second press finds nothing to release and the caller says so.
    pub fn cancel(&mut self, id: u64) -> Option<PathBuf> {
        let e = self.at(id)?;
        if e.cancellable {
            e.kind = Kind::Cancelled;
            e.cancellable = false;
            e.dismissible = true;
            return e.temp.take();
        }
        if e.kind == Kind::Failed {
            return e.temp.take();
        }
        None
    }

    /// **The run stopped before this step finished — record it.** Not the same request as
    /// [`Rail::cancel`], and separating them fixed a step that vanished from the narrative.
    ///
    /// `cancel` answers *somebody pressed Cancel; here is the file to delete*, and it is gated on
    /// `cancellable`, which only [`Rail::writing`] ever sets. This answers *the worker reports that
    /// this step was stopped*, which the queue knows for certain and which is true whether or not
    /// the step got as far as opening a file.
    ///
    /// Routed through `cancel`, a stop that landed before the first `Writing` report left the entry
    /// on `Kind::Planned` for ever: the run was over, nothing was running, and the Rail still said
    /// the step was coming up. It reproduced only under load — with five other tests ahead of it,
    /// the cancel beat the worker's first report — which is the shape of defect that ships.
    ///
    /// A step that already finished or already failed is left alone: those are outcomes, and a
    /// cancellation arriving afterwards does not undo one.
    pub fn stopped(&mut self, id: u64) {
        let Some(e) = self.at(id) else { return };
        if matches!(e.kind, Kind::Done | Kind::Failed | Kind::Cancelled | Kind::Note) {
            return;
        }
        e.kind = Kind::Cancelled;
        e.cancellable = false;
        e.dismissible = true;
        e.progress = Progress::None;
        e.temp = None;
    }

    pub fn failures(&self) -> usize {
        self.entries.iter().filter(|e| e.kind == Kind::Failed).count()
    }

    /// §7.5's shelf row 2: the most recent line worth showing on the bench, or nothing.
    ///
    /// A failure outranks work in progress, because the bench shows one line and never has to hold
    /// two (§9.3).
    pub fn line(&self) -> Option<String> {
        let pick = |k: Kind| self.entries.iter().rev().find(|e| e.kind == k);
        let e = pick(Kind::Failed).or_else(|| pick(Kind::Working))?;
        Some(match e.kind {
            Kind::Failed => format!("{} {} — {}", e.verb, e.what, e.happened()).trim().to_string(),
            _ => {
                let m = e.measure();
                if m.is_empty() {
                    format!("{} {}", e.verb, e.what).trim().to_string()
                } else {
                    format!("{} {} — {m}", e.verb, e.what).trim().to_string()
                }
            }
        })
    }

    /// What an assistive technology is told, and **it must not carry the byte counter**.
    ///
    /// The Rail is a `polite` live region, so a description that tracked `measure` would read 6.5 MB
    /// of progress out loud, one update at a time. This is computed from kinds and names only —
    /// there is no path from [`Progress`] into this string, which is what makes the rule mechanical
    /// rather than a comment somebody deletes.
    pub fn announce(&self) -> String {
        if self.entries.is_empty() {
            return "Nothing is happening.".into();
        }
        let failed = self.failures();
        if failed > 0 {
            let e = self
                .entries
                .iter()
                .rev()
                .find(|e| e.kind == Kind::Failed)
                .expect("failures() counted one");
            return format!(
                "{failed} failed. {} {} — {}",
                e.verb,
                e.what,
                e.happened()
            )
            .trim()
            .to_string();
        }
        if let Some(e) = self.entries.iter().rev().find(|e| e.kind == Kind::Working) {
            return format!("{} {}", e.verb, e.what).trim().to_string();
        }
        let done = self.entries.iter().filter(|e| e.kind == Kind::Done).count();
        let planned = self.entries.iter().filter(|e| e.kind == Kind::Planned).count();
        // **A plan nobody has agreed to is not work at zero per cent.** §10.1's whole argument is
        // that the five steps are on screen *before* anything is downloaded — and this said
        // `0 of 5 done.`, which tells an assistive technology that a job is under way on the one
        // screen written to prove that none is. The heading beside it already made the distinction
        // (*This is what pressing the centre button does*); the announcement did not.
        if planned > 0 && done == 0 {
            return format!("A plan of {planned} steps. Nothing has started.");
        }
        if planned > 0 {
            return format!("{done} of {} done.", done + planned);
        }
        if done > 0 {
            return format!("{done} done.");
        }
        // **A Rail made only of notes, and it had no arm at all.** Every branch above counts steps,
        // so one `Note` and nothing else fell through to `0 done.` — which is what a screen reader
        // was told after the one press in this build that succeeds, and it was the only feedback
        // that press produced anywhere. A note is a sentence somebody wrote to be read; reading it
        // is the whole point of it being here.
        if let Some(e) = self.entries.iter().rev().find(|e| e.kind == Kind::Note) {
            return e.what.clone();
        }
        // Everything left is cancelled. Say so rather than counting to zero.
        let n = self.entries.len();
        format!("{n} cancelled.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eapp_loader::compose::{Loader, Os, Recipe, Start};

    /// One value of every class, and the sweep asserts it is one of each.
    fn every_class() -> Vec<Class> {
        vec![
            Class::Network,
            Class::NotServed,
            Class::Verification,
            Class::Incompatible(compose::Fix::BuildFromIpsw),
            Class::SpacePreflight,
            Class::SpaceMidWrite,
            Class::Volume,
            Class::Permission,
            Class::ToolMissing(Tool::SevenZip),
            Class::Missing,
        ]
    }

    /// Everything this build can do once the deferred halves land, so a sweep can separate
    /// "unavailable because the mechanism is missing" from "unavailable because of the class".
    const ALL_CAPS: Caps = Caps {
        file_picker: true,
        drop_target: true,
        clipboard: true,
        reveal: true,
        devices_page: true,
        download: true,
        composer: true,
    };

    /// **Every cap off**, which is what a sweep of disabled controls needs and no longer what this
    /// phase is: `main::caps()` answers `true` for `devices_page` and `composer` now, both derived
    /// from `Page::slot()`. The name is kept and the claim is not — a fixture is allowed to be a
    /// shape the program has outgrown, but it is not allowed to say it *is* the program.
    ///
    /// `download` is the one cap that is measured rather than declared, so a machine **with**
    /// `curl` still has to leave `Retry` swept as a disabled control somewhere, and this is where.
    const THIS_PHASE: Caps = Caps {
        file_picker: false,
        drop_target: false,
        clipboard: false,
        reveal: false,
        devices_page: false,
        download: false,
        composer: false,
    };

    /// **T-11.** §9.3's table, swept: ten classes, each with its own words, each with a next step —
    /// except the one whose remedy is a command, which has none and names the command.
    #[test]
    fn every_failure_class_carries_a_next_step_and_its_own_words() {
        let all = every_class();
        assert_eq!(
            all.len(),
            Class::ALL.len(),
            "the sweep does not cover every class"
        );
        let mut names: Vec<&str> = all.iter().map(|c| c.as_str()).collect();
        names.sort_unstable();
        let mut want: Vec<&str> = Class::ALL.to_vec();
        want.sort_unstable();
        assert_eq!(names, want, "the sweep is not one value of each class");

        for c in &all {
            let said = Failure::new(c.clone(), "a step").said;
            assert!(
                !said.trim().is_empty(),
                "{} says nothing, which is the 'it boots to a white screen' non-diagnosis",
                c.as_str()
            );

            let steps = c.next(0, ALL_CAPS);
            if matches!(c, Class::ToolMissing(_)) {
                assert!(
                    steps.is_empty(),
                    "{} offers {} controls; nothing this window can press installs a tool",
                    c.as_str(),
                    steps.len()
                );
                let remedy = c.mono_remedy();
                assert!(
                    remedy.contains("install"),
                    "{} has no next step and no named command either: {remedy:?}",
                    c.as_str()
                );
            } else {
                assert!(
                    !steps.is_empty(),
                    "{} offers no next step, and §9.3 wants a real pressable control",
                    c.as_str()
                );
                assert!(
                    c.mono_remedy().is_empty(),
                    "{} has both a control and a command; §9.3 gives it one or the other",
                    c.as_str()
                );
            }
            for s in steps {
                assert!(!s.label().trim().is_empty(), "{} offers an unlabelled control", c.as_str());
                assert_ne!(
                    s.label(),
                    "Report it",
                    "there is no reporting path in this program"
                );
            }
        }
    }

    /// **T-12.** A control whose mechanism this build does not have is disabled, says why, and
    /// **never names a route that does not exist**.
    #[test]
    fn a_next_step_whose_action_does_not_exist_is_disabled_and_says_so() {
        let missing = Class::Missing.next(0, THIS_PHASE);
        let provide = missing
            .iter()
            .find(|n| **n == Next::Provide)
            .expect("Missing offers Provide");
        assert!(
            !provide.available(THIS_PHASE),
            "Provide claims to work with no picker and no drop target"
        );
        assert!(
            !provide.reason().is_empty(),
            "Provide is disabled and says nothing about why"
        );

        let elsewhere = Next::ChooseElsewhere;
        assert!(!elsewhere.available(THIS_PHASE), "there is no folder picker yet");
        assert!(
            elsewhere.escape_hatch(THIS_PHASE).contains("IPOD_EMULATOR_DATA"),
            "the one escape hatch that is real today is not offered: {:?}",
            elsewhere.escape_hatch(THIS_PHASE)
        );

        // **The phantom route, in its exact original shape.** Nothing unavailable may name a
        // mechanism whose own capability is false.
        let banned = [
            ("drop", THIS_PHASE.drop_target),
            ("picker", THIS_PHASE.file_picker),
            ("clipboard", THIS_PHASE.clipboard),
        ];
        for c in every_class() {
            for n in c.next(0, THIS_PHASE) {
                if n.available(THIS_PHASE) {
                    assert!(
                        n.escape_hatch(THIS_PHASE).is_empty(),
                        "{} works and still offers a way round it",
                        n.label()
                    );
                    continue;
                }
                assert!(
                    !n.reason().is_empty(),
                    "{} is disabled with no reason, in class {}",
                    n.label(),
                    c.as_str()
                );
                let hatch = n.escape_hatch(THIS_PHASE).to_ascii_lowercase();
                for (word, cap) in banned {
                    assert!(
                        cap || !hatch.contains(word),
                        "{} points at `{word}`, which this build does not have: {hatch:?}",
                        n.label()
                    );
                }
            }
        }

        // **The two that returned `true` unconditionally, named.** The loop above cannot catch
        // either of them coming ungated: it only ever says *if this is available it must offer no
        // way round it*, and `Retry` and `Fix` both have an empty hatch — so both would sweep clean
        // while drawn live over a mechanism that does not exist. That is the shape they shipped in.
        //
        // This is also why it is asserted against `THIS_PHASE` rather than against `main::caps()`:
        // `download` is measured on the machine the test runs on, and a gate whose verdict depends
        // on whether the developer has `curl` installed reports on the computer instead of on the
        // program.
        assert!(
            !Next::Retry.available(THIS_PHASE),
            "`Retry` is live with no downloader. Every class that offers it failed while fetching \
             something, and `firmware::download` IS curl"
        );
        assert!(
            !Next::Fix { label: "build from Apple's firmware instead".into(), presses: 2 }
                .available(THIS_PHASE),
            "`Fix` is live with every cap off. It rewrites a recipe, so it is gated on a surface \
             that holds one — and a fixture that has none must not draw it pressable"
        );
        // `Cancel` genuinely needs nothing: the file is this program's, in this run, on this
        // computer. It is the control the other two were wrongly grouped with.
        assert!(
            Next::CancelWrite.available(THIS_PHASE),
            "`Cancel` is refused, and it needs no mechanism this build lacks"
        );

        // And with every mechanism present, nothing is disabled and nothing needs a way round it.
        for c in every_class() {
            for n in c.next(0, ALL_CAPS) {
                assert!(n.available(ALL_CAPS), "{} is refused with every cap on", n.label());
                assert!(n.escape_hatch(ALL_CAPS).is_empty());
            }
        }
    }

    /// **No failure in this build is a dead end**, which is §9.4's rule about a project state:
    /// say what does work, and always name the escape hatch.
    ///
    /// Three classes had none. After one retry, `verification` offered `Provide the file
    /// yourself…` and `Copy the details`, both disabled, both with an empty hatch and an empty
    /// `mono`; a live `403` offered `Provide` alone on the same terms; and `permission` offered
    /// `Reveal`. Each of those is a person told *we have not built this* and given nothing else —
    /// a greyed row wearing an apology, on the failures somebody is most likely to hit first.
    ///
    /// What counts as a way out is deliberately narrow: **a live control, or a command**. A
    /// `reason` is not one, which is the whole point — every one of the three had a reason.
    #[test]
    fn no_failure_class_is_a_dead_end_in_this_build() {
        for c in every_class() {
            // `retries` changes what `verification` offers, and the second failure is the one that
            // had nothing left: `Retry` is replaced rather than repeated.
            for retries in [0u8, 1, 2] {
                let steps = c.next(retries, THIS_PHASE);
                let live = steps.iter().any(|n| n.available(THIS_PHASE));
                let hatch = steps
                    .iter()
                    .find(|n| !n.escape_hatch(THIS_PHASE).is_empty())
                    .map(|n| n.escape_hatch(THIS_PHASE));
                let mono = c.mono_remedy();
                assert!(
                    live || hatch.is_some() || !mono.is_empty(),
                    "class `{}` after {retries} retries offers {:?} — every one of them disabled, \
                     no command among them and no `mono` — so a person meeting it has a paragraph, \
                     two greyed controls and nothing to do",
                    c.as_str(),
                    steps.iter().map(Next::label).collect::<Vec<_>>()
                );
            }
        }
    }

    /// **T-13.** The plan is `Recipe::steps()`, one to one, with no string built here.
    #[test]
    fn the_plan_is_the_models_own_list() {
        let r = Recipe {
            start: Start::FromIpsw("iPod_25.1.3".into()),
            loader: Loader::Rockbox,
            oses: [Os::Apple, Os::Rockbox].into_iter().collect(),
        };
        let mut rail = Rail::new();
        let steps = r.steps(compose::Holes::Sparse);
        let ids = rail.plan(&steps);
        assert_eq!(ids.len(), steps.len(), "the plan is not the model's list");
        for (id, s) in ids.iter().zip(steps.iter()) {
            let e = rail.find(*id).expect("the entry just filed");
            assert_eq!(e.verb, s.verb(), "the verb was written here rather than read");
            assert_eq!(e.what, s.what(), "the subject was written here rather than read");
            assert_eq!(e.sub, s.sub(), "the detail line was dropped on the way in");
            assert_eq!(e.kind, Kind::Planned);
        }
        // **The detail line reaches the Rail.** `RailRow.sub` has been drawn since the drawer
        // landed with nothing behind it, because this filed `String::new()`.
        assert!(
            rail.entries().iter().any(|e| !e.sub.is_empty()),
            "every planned step's detail line is empty, so the row that draws it draws nothing"
        );
    }

    /// **T-14.** Two downloads that fail the same check stop offering `Retry`.
    ///
    /// `firmware.rs`'s present-but-wrong path prints *"already here but does not verify —
    /// downloading again"* and will loop for as long as a mirror serves the wrong bytes.
    #[test]
    fn two_downloads_that_fail_the_same_check_stop_offering_retry() {
        let first = Class::Verification.next(0, ALL_CAPS);
        assert!(first.contains(&Next::Retry), "the first mismatch has to offer a retry");

        let second = Class::Verification.next(1, ALL_CAPS);
        assert!(
            !second.contains(&Next::Retry),
            "the second mismatch still offers Retry, which loops for as long as a mirror serves \
             the wrong bytes"
        );
        assert!(
            second.contains(&Next::Provide),
            "and it has to offer the file itself instead: {second:?}"
        );
    }

    /// The count is what `Class::next` reads, so `retry` has to move it.
    #[test]
    fn a_retry_counts() {
        let mut rail = Rail::new();
        let id = rail.failed(
            "fetch",
            "Apple's firmware",
            Failure::new(Class::Verification, "the SHA-256"),
        );
        assert!(rail.retry(id));
        assert_eq!(rail.find(id).unwrap().retries, 1);
        assert_eq!(rail.find(id).unwrap().kind, Kind::Planned);
        // A retry of something that did not fail is not a retry.
        assert!(!rail.retry(id), "a planned step was retried");
    }

    /// **Nothing dismisses itself, and there is no clock in this module.**
    ///
    /// §14.4 bans the four-second self-dismissing message by name. The mechanical form is that a
    /// failure survives any amount of unrelated traffic and only `dismiss` removes it.
    #[test]
    fn a_failure_stays_until_it_is_dismissed() {
        let mut rail = Rail::new();
        let id = rail.failed("start", "iPod 1", Failure::new(Class::Missing, "the drive"));
        for i in 0..200 {
            rail.note(&format!("note {i}"));
        }
        assert_eq!(rail.failures(), 1, "the failure went away on its own");
        assert!(rail.dismiss(id));
        assert_eq!(rail.failures(), 0);
        assert!(!rail.dismiss(id), "dismissing twice removed something else");
    }

    /// **The same thing said twice in a row is one entry, and that is the bound.**
    ///
    /// This module's own header claims the Rail *"does not grow without bound, and that is a
    /// mechanism rather than a hope."* That was true of no code path: `collapse_finished` is called
    /// only from `plan`, which nothing in this build calls, and it collapses only `Done`. So
    /// pressing the centre button N times appended N identical entries, each needing its own
    /// dismissal, and `why ›` — which files the refusal it was pressed for — stacked one per press.
    ///
    /// **Two DIFFERENT things are still two entries.** Failures accumulate and are dismissed
    /// individually (§9.3); this collapses a repeat, not a history.
    #[test]
    fn the_same_thing_said_twice_in_a_row_is_one_entry() {
        let mut rail = Rail::new();
        for _ in 0..10 {
            rail.note("iPod 1 resolves and would start here.");
        }
        assert_eq!(rail.entries().len(), 1, "ten identical presses filed ten notes");

        rail.note("something else happened");
        assert_eq!(rail.entries().len(), 2, "a different note is a different thing");
        // …and the first one is still reachable, rather than having been overwritten.
        assert_eq!(rail.entries()[0].what, "iPod 1 resolves and would start here.");

        let f = || Failure::saying(Class::Missing, "starting iPod 1", "x.img is not where it was");
        let first = rail.failed("start", "iPod 1", f());
        for _ in 0..10 {
            assert_eq!(rail.failed("start", "iPod 1", f()), first, "a repeat minted a new id");
        }
        assert_eq!(rail.failures(), 1, "ten presses of `why` filed ten copies of one sentence");

        // A different device is a different failure, and both stay.
        rail.failed(
            "start",
            "iPod 2",
            Failure::saying(Class::Missing, "starting iPod 2", "its ROM is not where it was"),
        );
        assert_eq!(rail.failures(), 2, "two devices with two problems collapsed into one");
    }

    /// **A Rail made only of notes announces the note, not `0 done.`**
    ///
    /// Every branch of `announce` counted *steps*, so one `Note` and nothing else fell through to
    /// `0 done.` — which is what a screen reader was told after the one press in this build that
    /// succeeds, and it was the only feedback that press produced anywhere in the program.
    #[test]
    fn a_rail_of_notes_announces_the_note() {
        let mut rail = Rail::new();
        assert_eq!(rail.announce(), "Nothing is happening.");

        rail.note("iPod 1 resolves and would start here.");
        assert_eq!(
            rail.announce(),
            "iPod 1 resolves and would start here.",
            "a Rail holding one note announced a count of steps that are not there"
        );
        assert!(
            !rail.announce().contains("0 done"),
            "the live region is counting to zero over a sentence somebody wrote to be read"
        );

        // …and a note does not outrank work in progress or a failure, which are the two branches
        // above it.
        let id = rail.note("fetching");
        rail.progress(id, Progress::Bytes { done: 1, total: 2 });
        assert_eq!(rail.announce(), "fetching");
        rail.fail(id, Failure::new(Class::Network, "a download"));
        assert!(rail.announce().starts_with("1 failed."), "{}", rail.announce());
    }

    /// **A plan nobody has agreed to does not announce progress.**
    ///
    /// §10.1 puts five steps on screen *before* a byte is downloaded, and the whole argument of
    /// that screen is that nothing has happened yet. `announce` had one arm for `planned > 0`, so
    /// what a screen reader was told on it was `0 of 5 done.` — a job under way, on the one page
    /// written to prove there is not one. The visible heading beside it said *This is what
    /// pressing the centre button does*; the two disagreed.
    #[test]
    fn a_plan_nobody_has_agreed_to_does_not_announce_progress() {
        let mut rail = Rail::new();
        let steps = compose::Recipe {
            start: compose::Start::FromIpsw("iPod_25.1.3.ipsw".into()),
            loader: compose::Loader::Apple,
            oses: [compose::Os::Apple].into_iter().collect(),
        }
        .steps(compose::Holes::Sparse);
        let ids = rail.plan(&steps);
        assert!(ids.len() >= 3, "the fixture filed {} steps", ids.len());

        let said = rail.announce();
        assert_eq!(
            said,
            format!("A plan of {} steps. Nothing has started.", ids.len()),
            "a plan nobody has pressed announces {said:?}"
        );
        assert!(
            !said.contains("done"),
            "the live region says work is finished on a plan nothing has started: {said}"
        );

        // One step in, it goes back to counting — because now there is something to count.
        rail.done(ids[0]);
        assert_eq!(rail.announce(), format!("1 of {} done.", ids.len()));
    }

    /// The announcement is a state transition, never a byte counter.
    #[test]
    fn the_announcement_does_not_read_out_the_progress_bar() {
        let mut rail = Rail::new();
        let id = rail.note("fetching");
        rail.progress(id, Progress::Bytes { done: 0, total: 6_500_352 });
        let first = rail.announce();
        for done in (0..6_500_352).step_by(65_003) {
            rail.progress(id, Progress::Bytes { done, total: 6_500_352 });
            assert_eq!(
                rail.announce(),
                first,
                "the live region changed while only the byte count moved"
            );
        }
        rail.done(id);
        assert_ne!(rail.announce(), first, "finishing is a transition and has to be announced");
    }

    /// The measure is real bytes, and a step with no denominator gets a number and no bar.
    #[test]
    fn a_step_with_no_denominator_gets_no_bar() {
        let mut rail = Rail::new();
        let id = rail.note("copy");
        rail.progress(id, Progress::Bytes { done: 4_100_000, total: 6_500_352 });
        let e = rail.find(id).unwrap();
        assert_eq!(e.measure(), "4.1 MB of 6.5 MB");
        assert!((e.fraction() - 0.6307).abs() < 0.001, "{}", e.fraction());

        rail.progress(id, Progress::Bytes { done: 4_100_000, total: 0 });
        let e = rail.find(id).unwrap();
        assert_eq!(e.measure(), "4.1 MB");
        assert!(
            e.fraction() < 0.0,
            "a step with no denominator reported {} — a bar would be drawn empty and would lie",
            e.fraction()
        );
    }

    /// §12.7: cancelling says which file it deletes and how big it is, and hands the file back
    /// rather than deleting it here.
    #[test]
    fn cancelling_names_the_file_it_deletes_and_deletes_nothing_itself() {
        let mut rail = Rail::new();
        let id = rail.note("build");
        rail.writing(id, PathBuf::from("/tmp/my-5.5g.img.part"));
        rail.progress(id, Progress::Bytes { done: 41_200_000_000, total: 0 });
        let cost = rail.find(id).unwrap().cancel_cost();
        assert!(cost.contains("my-5.5g.img.part"), "{cost}");
        assert!(cost.contains("41 GB"), "{cost}");

        let f = rail.cancel(id).expect("the file to delete");
        assert_eq!(f, PathBuf::from("/tmp/my-5.5g.img.part"));
        assert_eq!(rail.find(id).unwrap().kind, Kind::Cancelled);
        assert!(rail.cancel(id).is_none(), "cancelling twice handed the file back twice");
    }

    /// **The one control offered under *the disk filled up* could never do the one thing it named.**
    ///
    /// §12.7 and §9.3's `space, mid-write` row. `Rail::fail` clears `cancellable` — there is no
    /// write left to stop — but the partial file is still on the disk, and `Class::SpaceMidWrite`
    /// answers `Next::CancelWrite`, whose whole job is to delete it. Gated on `cancellable` alone,
    /// `cancel` handed back `None` for exactly that entry: the path sat in `temp`, named on screen
    /// by `cancel_cost`, unreachable.
    ///
    /// This is the same defect as the two live-but-inert controls, reached from the other side. The
    /// arm was wired; the model refused to serve it.
    #[test]
    fn a_step_that_failed_mid_write_can_still_release_the_file_it_left() {
        let part = PathBuf::from("/tmp/my-5.5g.img.part");
        let mut rail = Rail::new();
        let id = rail.note("build");
        rail.writing(id, part.clone());
        rail.progress(id, Progress::Bytes { done: 9_000_000, total: 20_987_904 });
        rail.fail(
            id,
            Failure::saying(
                Class::SpaceMidWrite,
                "building a drive",
                "Stopped at 9 MB. my-5.5g.img.part is 9 MB and cancelling deletes it.",
            ),
        );

        // The control really is offered here, and it really is pressable — otherwise this test
        // would be about a route nobody can take.
        let steps = Class::SpaceMidWrite.next(0, THIS_PHASE);
        assert!(steps.contains(&Next::CancelWrite), "SpaceMidWrite stopped offering Cancel");
        assert!(Next::CancelWrite.available(THIS_PHASE));

        // And it is told what pressing costs BEFORE the press, which is what makes deleting it the
        // operator's decision rather than this program's (`AGENTS.md` §3).
        let cost = rail.find(id).unwrap().cancel_cost();
        assert!(cost.contains("my-5.5g.img.part"), "{cost}");

        assert_eq!(
            rail.cancel(id),
            Some(part),
            "the partial file a failed write left could not be released, so the only control \
             offered under `the disk filled up` was guaranteed to do nothing"
        );

        // **The failure survives the cancel.** Releasing the file is not a retraction of what went
        // wrong: the sentence stays, the class stays, and the other next step beside it stays.
        let e = rail.find(id).unwrap();
        assert_eq!(e.kind, Kind::Failed, "pressing Cancel erased the failure that explained it");
        assert_eq!(e.failure.as_ref().map(|f| f.class.clone()), Some(Class::SpaceMidWrite));
        assert!(e.temp.is_none(), "the file was released and the entry still claims to hold it");
        assert!(rail.cancel(id).is_none(), "a second press handed the same file back again");
    }

    /// **A step stopped before it opened a file still leaves the narrative.**
    ///
    /// The race that found this: `Queue::cancel` sets a flag, and the worker checks it at the top of
    /// each step — so a cancel that lands in the first hundred milliseconds is answered *before* the
    /// step reports `Writing`, which is the only thing that ever sets `cancellable`. Routed through
    /// [`Rail::cancel`] that stop did nothing, and the entry sat on `Kind::Planned` with the run
    /// over: the Rail saying a step was coming up that nothing was ever going to run.
    ///
    /// It reproduced only with five other tests ahead of it — fast machine, warm cache, the cancel
    /// beating the worker — so the deterministic version is here rather than in the ignored test
    /// that found it.
    #[test]
    fn a_step_stopped_before_it_opened_a_file_is_still_filed_as_cancelled() {
        let steps = Recipe {
            start: Start::FromIpsw("iPod_25.1.3".into()),
            loader: Loader::Apple,
            oses: [Os::Apple].into_iter().collect(),
        }
        .steps(compose::Holes::Sparse);
        let mut rail = Rail::new();
        let ids = rail.plan(&steps);

        // Exactly the state a worker stopped at the top of its first step leaves behind: planned,
        // never `writing`, so `cancellable` was never set.
        let id = ids[0];
        assert!(!rail.find(id).unwrap().cancellable, "the fixture is not the state this is about");
        assert_eq!(
            rail.cancel(id),
            None,
            "a planned step has no file to hand back, which is why `cancel` alone cannot file this"
        );
        assert_eq!(
            rail.find(id).unwrap().kind,
            Kind::Planned,
            "the fixture is not the state this is about"
        );

        rail.stopped(id);
        assert_eq!(
            rail.find(id).unwrap().kind,
            Kind::Cancelled,
            "the run stopped and the Rail still says this step is coming up"
        );
        assert!(rail.find(id).unwrap().dismissible, "a cancelled step cannot be taken off the Rail");
        assert!(rail.find(id).unwrap().fraction() < 0.0, "a cancelled step still draws a bar");

        // **An outcome is not undone by a stop arriving afterwards.** A step that finished stays
        // finished, and one that failed keeps the sentence explaining it.
        rail.done(ids[1]);
        rail.stopped(ids[1]);
        assert_eq!(rail.find(ids[1]).unwrap().kind, Kind::Done, "a stop undid a finished step");
        rail.fail(ids[2], Failure::new(Class::Network, "a download"));
        rail.stopped(ids[2]);
        assert_eq!(rail.find(ids[2]).unwrap().kind, Kind::Failed, "a stop erased a failure");
        assert!(rail.find(ids[2]).unwrap().failure.is_some(), "a stop erased the sentence with it");
    }

    /// A new plan folds the last one's finished steps into one line, so the Rail is bounded.
    #[test]
    fn a_new_plan_collapses_the_last_ones_finished_steps() {
        let r = Recipe {
            start: Start::FromIpsw("iPod_25.1.3".into()),
            loader: Loader::Apple,
            oses: [Os::Apple].into_iter().collect(),
        };
        let mut rail = Rail::new();
        let steps = r.steps(compose::Holes::Sparse);
        let first = rail.plan(&steps);
        for id in &first {
            rail.done(*id);
        }
        assert_eq!(rail.entries().len(), first.len());

        let second = rail.plan(&steps);
        assert_eq!(
            rail.entries().len(),
            second.len() + 1,
            "the finished steps were kept as well as the new ones: {:?}",
            rail.entries().iter().map(|e| e.what.clone()).collect::<Vec<_>>()
        );
        assert_eq!(rail.entries()[0].kind, Kind::Note);
        assert!(rail.entries()[0].what.contains("finished"));
    }
}
