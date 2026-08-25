//! **What a running machine IS**, before anything draws it — GUI.md §12 and §7.3, with no toolkit
//! in it.
//!
//! **Phase 7 landed and this module is what the window now asks.** It was written a wave ahead of
//! the wiring, when `Verb::Start` still ended `Kind::Planned` and `main::phase` answered `Off`
//! unconditionally — *"a cradle promising to start would be the window making a claim the program
//! does not keep"*. The claim is kept: `main::start_machine` builds an `emu::Config` from the
//! device the press resolved and spawns `emu::run`, `main::life` reads `Out.phase` through
//! [`Life::read`], and every §7.3 caption on the bench is [`cradle`]'s. What has *not* changed is
//! the rule this file was built on — the states a machine can be in, what each one permits, and
//! the caption §7.3 wants for each, decided with no toolkit in the room.
//!
//! **It decides nothing the machine already decides.** `emu::Phase` is the phase, and [`Life::read`]
//! is the only constructor here — every value of [`Life`] came out of an [`Out`] the emulator thread
//! published, so this cannot become a second opinion about what the machine is doing.
//! [`Config::may_restore`] is asked rather than re-derived, and [`Restore`] is checked against it in
//! this module's own tests. The sentences a device's parts are refused with are
//! `main::blocked_label`'s, called rather than copied, so the bench and the Devices page cannot
//! say two things about one iPod — and that function is now keyed on [`Blocked`], so the arm that
//! words a row and the arm that classifies it cannot drift into two different orders.
//!
//! **Four things §12 asks for that the model cannot answer, named here rather than invented.**
//!
//! - **§12.4's `press ● to resume` is not reachable by any command.** `Cmd::PowerOn` is *"always a
//!   cold boot, never a restore"*, and the only code that restores is `emu::run`'s entry, gated on
//!   `Config::may_restore(first)` with `first` false for every power cycle inside a session. So a
//!   resume needs the machine **thread** built, and a window holding a powered-off machine has no
//!   route back to the parked state. [`Launch::Resume`] answers `None` for its command rather than
//!   handing back `PowerOn`, which would cold-boot a machine whose label promised three seconds.
//! - **§12.4's `parking · 0.7 of 1.6 GB` has no numerator and no denominator.** `Link::saving` is an
//!   `AtomicBool`; nothing anywhere publishes bytes written or bytes to write. [`Cradle`] says
//!   `parking` and stops, which is the whole of what is known.
//! - **§12.2's `24 % of real` has no stated divisor, and §12.8's own worked example matches
//!   neither candidate.** `Config::clock`'s doc says *5 is what every recipe uses; 75 is real*, so
//!   `14.2 M instr/s` against a real 5G is 18.9 %. `Stats::sim_usec` against `Stats::wall_secs` on
//!   §12.8's own numbers — 21.5 s simulated, 34.8 s wall — is 61.8 %. Neither is 24.1 %, and
//!   `487 220 016` instructions in `21.5 s` simulated is 22.7 instructions per simulated
//!   microsecond, which is neither 5 nor 75. [`Pace`] therefore publishes the speed it can measure
//!   and no ratio at all.
//! - **§7.3's `running · wheel 41 queued` names `Stats::queued`, which §12.8 decides does not earn a
//!   row** — *"a refused step is a lie about what you did and a deep queue is only ever the reason
//!   for one"*. Two sections of one document want opposite things from one field. Nothing here
//!   reads it, so §7.3's running row is the bare word — see [`cradle`], where the reason it is not
//!   the speed instead was found by looking at a picture rather than by reading.
//!
//! **And one it asks for that the model can answer and must spell differently, found by a gate
//! rather than by reading: §7.3's own separator is a glyph this program may not type.** Every
//! caption in §7.3's table and §12.2's shelf column is built on `·` —
//! `booting · 62 %`, `running · 14.2 M instr/s`, `parking · 0.7 of 1.6 GB` — and
//! `geometry::GLYPHS` is a closed set of three, `—`, `…` and `§`, with `·` explicitly *off* it:
//! §6.7's answer for a symbol is that it is **drawn as a `Path`**, which `ui/bench.slint` already
//! does for the shelf's MENU list, and Rust has no `Path`. So every caption here uses the em dash
//! and a comma where §7.3 writes a middle dot. That is not a rewording of the design — it is the
//! same sentence in the vocabulary the window can actually render, and the alternative is four
//! `.notdef` squares on the one line the whole bench is built around.
//!
//! **No toolkit type appears on any code line in this file**, which is the same rule `args`,
//! `geometry`, `fit` and `rail` are held to — the model has to be testable on a machine with no
//! display stack, and every test below runs with no window.

use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use eapp_loader::settings::{Absent, Device, Presence};

use crate::emu::{BootTarget, Cmd, Config, Out, Phase, Stats};

/// §7.3's `nothing is mounted`.
///
/// The one row of that table this module types from nothing: every other refusal is about a device,
/// and this is the row for there not being one.
pub const NOTHING_MOUNTED: &str = "nothing is mounted";

/// §7.4's refusal, held while the pointer is down on a drawn control that is not the centre button.
///
/// **It used to be 71 characters against a 48-character row, and `bench-refused.png` is what that
/// looked like**: `the wheel and the buttons belong to the machine, and there is no…`. The note
/// here argued the elision was survivable because *"the first clause is the whole sentence's
/// meaning"* — and the first clause does survive, but what a person actually reads ends on **`and
/// there is no`**, a sentence cut off in the middle of naming the thing that is missing. That is
/// the trade `main::blocked_label`'s `Parts` arm makes for a path, and it was wrong here for the
/// reason that arm is right there: a path has a length this program does not choose, and this
/// sentence is entirely ours.
///
/// **So it is shorter, and it fits.** Same fact, in the register §7.4 is actually about — what the
/// drawn controls are for and when they work — rather than an ownership claim the reader has to
/// finish for themselves. `every_machine_caption_this_module_types_fits_its_own_row` now measures
/// it in the list of captions that fit rather than the list that elides gracefully.
pub const NO_MACHINE: &str = "the wheel and buttons work once the iPod runs";

/// §7.4's other refusal, for the one drawn control that is not a button.
///
/// It was a literal inside `main::refusals` — the only sentence in §7.4 that lived outside this
/// module, which is exactly the arrangement `NO_MACHINE` was moved here to end.
///
/// **Shortened with its sibling, and for the same reason.** At 62 characters it elided to `the hold
/// switch belongs to the machine, and there…`, which is the same sentence-cut-in-half as
/// [`NO_MACHINE`] and was covered by the same note claiming the first clause was enough. It was
/// also the one §7.4 refusal no test measured at all — it is in the budget list now.
pub const NO_MACHINE_HOLD: &str = "the hold switch works once the iPod runs";

/// §7.3's `Running` cradle: **[`NO_MACHINE`] with the tense turned round.**
///
/// The row used to read `running`, which is [`Life::shelf`]'s word for the same phase and drawn
/// fifty pixels from it. See [`cradle`]'s `Running` arm for why that stopped being tenable and why
/// this is the sentence that replaces it — the same reason the speed came off the shelf, reached
/// from the other side.
pub const WHEEL_IS_LIVE: &str = "the wheel and buttons are the iPod's now";

/// **A count of instructions, for a person.** `412 M instr`, `21.5 G instr`, `900 instr`.
///
/// Not `eapp_loader::si`, which is the same arithmetic against a different noun: it renders `412 MB`
/// and this row is not about bytes. Writing `si(n)` here and letting the unit read as bytes is the
/// shape of defect §12.8 is about — a number whose label says something the number is not.
///
/// **`instr` and not `instructions`.** §12.2's own shelf row is already `14.2 M instr/s`, so the
/// abbreviation is the document's; and the cradle's counted boot caption is 45 characters with it
/// against 52 without, over a 48-character row.
pub fn instructions(n: u64) -> String {
    const K: f64 = 1000.0;
    let v = n as f64;
    if v < K {
        return format!("{n} instr");
    }
    for (i, unit) in ["k", "M", "G", "T"].iter().enumerate() {
        let div = K.powi(i as i32 + 1);
        if v < div * K || *unit == "T" {
            let scaled = v / div;
            return if scaled < 10.0 {
                format!("{scaled:.1} {unit} instr")
            } else {
                format!("{scaled:.0} {unit} instr")
            };
        }
    }
    unreachable!()
}

// ── §12.3: progress, honestly ────────────────────────────────────────────────────────────────────

/// §12.3's boot progress, in the two shapes it can honestly take.
///
/// **A bar with no denominator is the state this type exists to make unrepresentable.** §12.3 is
/// exact about it: *"Before a device has ever booted there is no fraction and no bar: the cradle
/// label carries an instruction count that moves."* A single `percent: f32` field would have to
/// answer something for a device that has never booted, and every answer is a lie — `0` reads as *no
/// progress*, and a fraction over an invented denominator reads as progress nobody measured. So the
/// no-denominator case is a **different variant**, and the denominator that does exist is a
/// [`NonZeroU64`], which is what stops a division by zero being reachable at all.
///
/// [`Progress::read`] is the only constructor, and it is what performs the demotion: hand it
/// `None`, or `Some(0)`, and it produces `Counted`. A caller cannot get a `Fraction` by accident.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Progress {
    /// No completed cold boot on this device, so there is a number and no fraction.
    Counted { instructions: u64 },
    /// `Device::cold_boot_instructions` from this device's own last completed cold boot.
    Fraction { done: u64, of: NonZeroU64 },
}

impl Progress {
    /// **The only way to make one.** `denominator` is `Device::cold_boot_instructions`.
    ///
    /// `Some(0)` demotes exactly like `None`: a device whose recorded boot took zero instructions
    /// recorded nothing, and `Settings::expected_boot` already filters that value out for the same
    /// reason. Two places agreeing is cheap; a division by zero reachable from a settings file
    /// somebody hand-edited is not.
    pub fn read(done: u64, denominator: Option<u64>) -> Progress {
        match denominator.and_then(NonZeroU64::new) {
            Some(of) => Progress::Fraction { done, of },
            None => Progress::Counted { instructions: done },
        }
    }

    /// The percentage, or `None` when there is no denominator. **Never clamped.**
    ///
    /// §12.3 names the two ways a bar goes wrong when the recipe changes under a stored denominator
    /// — *"the cradle reads `booting · 6 %` at the moment the machine is finished"* one way, and
    /// *"the bar passes 100 % and keeps going"* the other — and says the remedy is upstream, in
    /// `Settings::set_boot_shape`, which drops the number when the shape moved. Clamping here would
    /// hide the half of that defect the operator can still see, so a reading above 100 is returned
    /// as it is: it means the shape rule did not run.
    pub fn percent(&self) -> Option<u64> {
        match self {
            Progress::Counted { .. } => None,
            Progress::Fraction { done, of } => Some(done.saturating_mul(100) / of.get()),
        }
    }

    /// §7.3's booting caption tail: `62 %`, or `412 M instr`.
    pub fn caption(&self) -> String {
        match self {
            Progress::Counted { instructions: n } => instructions(*n),
            Progress::Fraction { .. } => {
                format!("{} %", self.percent().unwrap_or_default())
            }
        }
    }
}

// ── §12.2: the four phases, and Off is genuinely one of them ─────────────────────────────────────

/// Why a machine stopped, and **never empty**.
///
/// `emu::Phase::Stopped(String)` can hold `""` — nothing in the type says otherwise, and the one
/// producer that fills it formats a `Stop` variant, which is fine until a second producer does not.
/// §7.3 draws this row in `danger` with the reason on it, and a `danger` ring over a blank sentence
/// is the loudest thing on the bench saying nothing at all.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Reason(String);

impl Reason {
    /// What a machine that stopped without saying why is reported as. **A statement about our
    /// instrument, not about the machine** — it does not claim a cause, it says the cause did not
    /// arrive.
    pub const UNSAID: &'static str = "it stopped and did not say why";

    /// Trimmed, and substituted when there is nothing left.
    pub fn new(said: &str) -> Reason {
        let t = said.trim();
        Reason(if t.is_empty() { Reason::UNSAID.to_string() } else { t.to_string() })
    }

    pub fn said(&self) -> &str {
        &self.0
    }
}

/// How fast the machine is going, out of the numbers the run loop actually publishes.
///
/// **`speed` is an `Option` and that is the whole design.** A machine that has been up for no
/// measurable wall time has an unmeasured speed, and §12.8's rule for the Readout — *"a zero and an
/// unmeasured are different facts and this repository has been burned by conflating them"* — is a
/// rule about the model long before it is a rule about a Gauge. `0.0` here would draw as `0 instr/s`
/// on a machine that is running perfectly well and has been for four milliseconds.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Pace {
    /// Everything this machine has ever executed, including whatever a restored snapshot carried.
    pub executed: u64,
    /// Executed **in this process** — the numerator of the only honest speed.
    pub here: u64,
    /// The emulator's own clock. `u32` microseconds, so it wraps after about 71 minutes of
    /// simulated time; carried as published rather than widened, because widening it here would be
    /// this file inventing precision the machine does not have.
    pub sim_usec: u32,
    /// Wall seconds since the machine started running in this process.
    pub wall_secs: f64,
}

impl Pace {
    pub fn of(s: &Stats) -> Pace {
        Pace { executed: s.executed, here: s.executed_here, sim_usec: s.sim_usec, wall_secs: s.wall_secs }
    }

    /// Instructions per second, or `None` while no wall time has passed.
    pub fn speed(&self) -> Option<f64> {
        (self.wall_secs > 0.0).then(|| self.here as f64 / self.wall_secs)
    }

    /// §12.2's `14.2 M instr/s`, or `None` when there is nothing measured to say.
    pub fn caption(&self) -> Option<String> {
        self.speed().map(|s| format!("{}/s", instructions(s as u64)))
    }
}

/// §12.2's four phases **with the evidence each one is entitled to**, and no phase holding evidence
/// it cannot have.
///
/// This is not a second `emu::Phase`. It is that phase joined to the numbers §12 draws beside it,
/// and [`Life::read`] is the only constructor — so a `Life` is always something the emulator thread
/// said, never something this file decided.
///
/// **What the joining buys, stated as the states it deletes:**
///
/// - `Off` carries nothing. §12.2: *"no machine exists, nothing is executing, and the panel is
///   dark"*. A phase that could carry a pace would let the bench draw a speed for a machine that is
///   not running, which is the frozen-last-frame defect §12.2 forbids on the glass, one column left.
/// - `Booting` cannot exist without a [`Progress`], and `Progress` cannot exist as a fraction over
///   nothing. That is the *"a `Working` row with no progress and nothing running is a lying
///   instrument"* rule, made structural rather than remembered.
/// - `Stopped` cannot exist without a [`Reason`], and a `Reason` is never empty.
///
/// **`Phase::Booting { target }` is deliberately not read for the denominator.** Its `target` is
/// `cfg.snap_at`, the instruction count the *snapshot* will be taken at — `emu.rs:1792` is where it
/// becomes the phase, and the run loop compares the phase against that same value again to decide
/// the boot has ended. §12.3 is explicit that the progress denominator is a different number,
/// `Device::cold_boot_instructions`, *"this device's own last completed cold boot"*. Two numbers for two
/// questions, and one is a plausible-looking substitute for the other, which is what would make
/// reading the wrong one a defect nobody sees.
#[derive(Clone, PartialEq, Debug)]
pub enum Life {
    Off,
    Booting { target: BootTarget, progress: Progress },
    Running { pace: Pace, stalled_secs: f32 },
    Stopped { reason: Reason, pace: Pace },
}

impl Life {
    /// **The only constructor**, from what the emulator thread published.
    ///
    /// `boot_target` is `Config::boot` — the phase does not carry it and §12.5's rows need it.
    /// `denominator` is `Settings::expected_boot`, which has already dropped a zero.
    pub fn read(out: &Out, boot_target: &BootTarget, denominator: Option<u64>) -> Life {
        match &out.phase {
            Phase::Off => Life::Off,
            Phase::Booting { .. } => Life::Booting {
                target: boot_target.clone(),
                progress: Progress::read(out.stats.executed, denominator),
            },
            Phase::Running => {
                Life::Running { pace: Pace::of(&out.stats), stalled_secs: out.stalled_secs }
            }
            Phase::Stopped(why) => {
                Life::Stopped { reason: Reason::new(why), pace: Pace::of(&out.stats) }
            }
        }
    }

    /// Whether a machine exists at all — which is the question §7.4 asks of every drawn control.
    ///
    /// `Stopped` answers **false**: §12.5 says power off *"is real — the machine is dropped and
    /// re-entered at the reset vector"*, and a stopped machine is one nothing is executing on. The
    /// last frame stays on the glass because it is evidence, not because anything is behind it.
    pub fn alive(&self) -> bool {
        matches!(self, Life::Booting { .. } | Life::Running { .. })
    }

    /// §12.2's fifth thing, which is not a phase: the instruction count has not moved.
    ///
    /// `Some(secs)` only past the threshold, and only while `Running` — a machine that is off has
    /// not moved either, and reporting that as a stall would be the instrument shouting about the
    /// one state where nothing moving is correct.
    pub fn stalled(&self) -> Option<f32> {
        match self {
            Life::Running { stalled_secs, .. } if *stalled_secs > STALL_SECS => Some(*stalled_secs),
            _ => None,
        }
    }

    /// §12.2's fourth column — the shelf's row 1 trailing slot. **A state, and never a meter.**
    ///
    /// Every other row here is what the machine *is*: `off`, `stopped`, `off, parked 4 min ago`.
    /// `Booting` carries a fraction because a person waiting on a 75-second cold boot is owed how
    /// far along it is, and that is the same fact the progress bar under it draws.
    ///
    /// **`Running` used to carry the speed and no longer does.** §12.2's table asks for `running ·
    /// 14.2 M instr/s · 24 % of real`, and the wider half was never built for a reason `readout.rs`
    /// and this module's own header both record — *"`14.2 M instr/s` against a real 5G is 18.9 %"*,
    /// a comparison nothing here can make honestly. The narrower half went with it: instructions
    /// per second is a fact about how fast the *host* is emulating, not about the iPod, and the
    /// shelf is the one band a person cannot navigate away from. §12.9 draws that line — the ~90
    /// trace instruments are *"terminal instruments for a person already holding a hypothesis"* —
    /// and a speed you would have to compare against a remembered number to learn anything from is
    /// exactly that shape. The Readout's `MACHINE` block still has it, next to `instructions`,
    /// `simulated`, `wall` and `stalled`, opt-in behind the Menu, which is where a person holding
    /// that hypothesis goes. Nothing is hidden: `Pace::caption` is unchanged and `readout.rs`
    /// is its caller.
    pub fn shelf(&self) -> String {
        match self {
            Life::Off => "off".into(),
            Life::Booting { progress, .. } => format!("booting — {}", progress.caption()),
            Life::Running { .. } => "running".into(),
            Life::Stopped { .. } => "stopped".into(),
        }
    }

    /// §7.3's ring colour **for the machine's own rows**.
    ///
    /// `Off` is `accent` here because §12.2's table says so — *"`accent`, or a broken ring"* — and
    /// the broken half is a fact about the device's parts, not about the phase. [`cradle`] is where
    /// the two meet, and it is the only place that may answer [`Ring::Dim`] for an `Off` machine.
    pub fn ring(&self) -> Ring {
        match self {
            Life::Off => Ring::Accent,
            Life::Booting { .. } | Life::Running { .. } => Ring::Dim,
            Life::Stopped { .. } => Ring::Danger,
        }
    }
}

/// §12.2's stall threshold, in wall seconds.
///
/// The number is §12.2's: *"`stalled_secs > 2.0` turns the Readout's stalled Gauge `warn`"*. Named
/// so the Readout and the cradle cannot end up with two thresholds, which is how one session sat
/// dead at 2 791 999 952 instructions and was noticed only because two `state` replies happened to
/// be compared by hand.
pub const STALL_SECS: f32 = 2.0;

/// §12.8's Gauge freshness, as a fact about the model rather than a discipline the drawing keeps.
///
/// Four states and not three: `Unmeasured` renders `—` and never `0`, and `Final` is *the machine
/// ended there* against `Stale`'s *we stopped looking*. §12.8 is explicit that those are different,
/// and a two-state `bool fresh` is what makes them the same.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Freshness {
    /// Sampled within [`SAMPLE_FRESH_MS`].
    Live,
    /// Older than that, or the machine is off.
    Stale,
    /// The machine stopped here.
    Final,
    /// Never sampled. `—`, never `0`.
    Unmeasured,
}

/// §12.8: *"live — sampled within 500 ms"*.
pub const SAMPLE_FRESH_MS: u64 = 500;

impl Freshness {
    /// `sampled_ms_ago` is `None` when nothing has ever sampled this machine.
    pub fn of(life: &Life, sampled_ms_ago: Option<u64>) -> Freshness {
        let Some(ms) = sampled_ms_ago else {
            return Freshness::Unmeasured;
        };
        match life {
            Life::Stopped { .. } => Freshness::Final,
            Life::Off => Freshness::Stale,
            _ if ms <= SAMPLE_FRESH_MS => Freshness::Live,
            _ => Freshness::Stale,
        }
    }
}

// ── §12.1, §12.2 and §12.4: what is on the glass ─────────────────────────────────────────────────

/// What the 320 × 240 panel shows.
///
/// **`Dark` and `Held` are the two that carry the argument.** §12.2: a powered-off iPod's glass is
/// empty *"never a frozen last frame, because a frozen frame is a paused machine pretending"*; a
/// stopped one keeps its last frame because *"the last frame is evidence"*. One enum with both
/// spellings is what stops the drawing choosing.
///
/// `Parked` carries a path that **existed when it was asked for** — see [`parked_frame`] — so
/// §12.4's *"if the PNG is absent the glass is dark"* is a branch nobody has to remember to write.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Glass {
    /// `#08080a` and empty.
    Dark,
    /// The ROM's boot screen.
    Boot,
    /// `Out.fb`, at `fb_seq`.
    Live,
    /// The frame the machine stopped on, kept.
    Held,
    /// §12.4's `<device>.parked.png`.
    Parked(PathBuf),
}

impl Glass {
    /// `parked` is [`parked_frame`]'s answer, and is only consulted when nothing is running.
    ///
    /// `drawn` is `Out.fb_nonzero != 0` — whether the machine has put anything on its own panel.
    ///
    /// **The ROM's boot screen stands in for a machine that has not drawn yet, and it used to cover
    /// one that had.** `Booting` returned [`Glass::Boot`] unconditionally, so a synthesised ROM's
    /// mark was painted over a framebuffer holding **71 695 lit pixels of 76 800** for as long as
    /// the phase lasted — and when a boot stalls, that is for ever. The operator reported the
    /// program *"stuck on the synthesised logo"* twice; the second time the machine behind the logo
    /// had booted far enough to fill its screen, and nothing on the bench said so.
    ///
    /// A real 5G draws its own logo from ROM into the framebuffer, so on a retail dump the two
    /// pictures agree and this changes nothing. On a synthesised ROM there is no such draw, and the
    /// substitute is the only thing standing between the operator and what the machine is doing.
    pub fn of(life: &Life, parked: Option<&Path>, drawn: bool) -> Glass {
        match life {
            Life::Off => match parked {
                Some(p) => Glass::Parked(p.to_path_buf()),
                None => Glass::Dark,
            },
            Life::Booting { .. } if drawn => Glass::Live,
            Life::Booting { .. } => Glass::Boot,
            Life::Running { .. } => Glass::Live,
            Life::Stopped { .. } => Glass::Held,
        }
    }
}

/// §12.4's parked frame, if park wrote one and it is still there.
///
/// **The path is `Config::parked_frame`'s and is not spelled again here.** It used to be, and the
/// writer did not exist to disagree with it; `emu::write_parked_frame` is the writer now, and one
/// stem in two files is the shape this repository keeps deleting.
///
/// `seen` is the caller's stat cache, so a bench drawing this every frame reads the filesystem once
/// per pass rather than once per repaint.
pub fn parked_frame(cfg: &Config, seen: &mut Presence) -> Option<PathBuf> {
    let png = cfg.parked_frame()?;
    seen.exists(&png).then_some(png)
}

// ── §12.4: what a park costs, and whether there is room for it ───────────────────────────────────

/// **Free space against the restore point's own size, asked before `save_on_quit` is set.**
///
/// §12.4 is exact about the order — *"free space is checked against the snapshot size before
/// `save_on_quit` is set, and if it is short the window closes without parking"* — and about why
/// it has to be that way round: a park that fails for want of space fails at window close, where
/// there is no window left to show §9.3's `space` class in.
///
/// **`free` is an `Option` and that is the honest half.** [`free_bytes`] answers only where this
/// program can ask, and a platform it cannot ask on gets `None` — which parks, because refusing a
/// park on a measurement nobody took would lose a restore point over an unanswered question.
/// `Unmeasured` and `zero` are different facts here for exactly the reason §12.8 gives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Park {
    /// `Link::snapshot_bytes` — this machine's own memory, summed off the format that writes it.
    /// Zero before a machine has been built, which cannot be short of anything.
    pub needed: u64,
    /// What the volume the snapshot goes to reports, or `None` where nothing can ask.
    pub free: Option<u64>,
}

impl Park {
    /// `dir` is where the snapshot will land. It is stat'd, so it has to exist — the window creates
    /// it when it builds the `Config`, which is the same instant it decides the path.
    pub fn of(needed: u64, dir: &Path) -> Park {
        Park { needed, free: free_bytes(dir) }
    }

    /// Whether parking would run the volume out. **Unmeasured is not short.**
    pub fn short(&self) -> bool {
        matches!(self.free, Some(f) if f < self.needed)
    }

    /// §12.4's own sentence, for the Rail: *"1.6 GB needed, 0.9 GB free."*
    ///
    /// `eapp_loader::si` and not [`instructions`]: this row **is** about bytes, which is the one
    /// case that function's own doc carves out.
    pub fn sentence(&self) -> String {
        format!(
            "{} needed, {} free",
            eapp_loader::si(self.needed),
            self.free.map_or_else(|| "an unmeasured amount".into(), eapp_loader::si)
        )
    }
}

/// Bytes available to an unprivileged writer on the volume `dir` is on.
///
/// **`statvfs`, and `f_bavail` rather than `f_bfree`** — the second counts blocks the filesystem
/// has reserved for root, which a window writing a restore point cannot have. Over-reporting free
/// space is the one direction this must not fail in.
///
/// Unix only. Windows has `GetDiskFreeSpaceExW` and `windows-sys` is already in this crate's tree,
/// but the feature that carries it is not enabled and nobody here can run the result — and an
/// untested `unsafe` call into a platform nobody can run is worse than a gap somebody can read
/// about, which is the same rule `windows_subsystem` is annotated with in `main.rs`.
#[cfg(unix)]
fn free_bytes(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: `c` is a NUL-terminated path this call only reads, and `s` is a `statvfs` this
    // thread owns and the call only writes. Nothing is retained past the call.
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0;
    ok.then(|| s.f_bavail as u64 * s.f_frsize as u64)
}

#[cfg(not(unix))]
fn free_bytes(_dir: &Path) -> Option<u64> {
    None
}

// ── §12.4: parking, and what a snapshot is worth ─────────────────────────────────────────────────

/// The half of the restore pair that is **not** RAM, as the cradle needs to say it.
///
/// Three states because §7.3 has three rows for them, and the middle one is the one that was
/// missing: a snapshot that is on disk and no longer describes this drive promised *about 3 s* and
/// delivered the intermittent "connect to computer" screen §12.4 was written about.
///
/// **`Config::pair_is_whole` is asked, never re-derived.** Every way that can be wrong resolves to
/// `Broken`, which costs a cold boot; the opposite mistake is restored RAM against a drive that has
/// moved, *"which is a machine that looks fine and is not"*.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Restore {
    /// No snapshot on disk. A cold boot is the only thing pressing can mean.
    Never,
    /// Snapshot present and the pair still agrees.
    Whole,
    /// Snapshot present and the drive moved under it. §7.3's `parked, pair broken`.
    Broken,
}

impl Restore {
    /// One `stat` and, off a copy, one line compare — which is what §12.4 says it costs, and why
    /// the cradle may ask it before it draws.
    pub fn of(cfg: &Config) -> Restore {
        if !cfg.snapshot.as_ref().is_some_and(|p| p.exists()) {
            return Restore::Never;
        }
        if cfg.pair_is_whole() {
            Restore::Whole
        } else {
            Restore::Broken
        }
    }
}

/// What pressing actually starts.
///
/// **`Resume` has no `Cmd`, and that is a fact about the program rather than an omission here.**
/// `Cmd::PowerOn`'s own doc says *"always a cold boot, never a restore"*, and the only code that
/// restores is `emu::run`'s entry, gated on `Config::may_restore(first)` with `first` false for
/// every power cycle inside a session. So a resume is reachable **only by building the machine
/// thread**, and a window that has already built one and powered it off cannot get back to the
/// snapshot without dropping the thread. Named in GUI.md §12.4 and in this module's header.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Launch {
    /// Enter at the reset vector. About `compose::COLD_BOOT_SECONDS`.
    Cold,
    /// Put the machine back where it was. About 3 s.
    Resume,
}

impl Launch {
    /// The command that reaches a machine thread that is **already running** and merely `Off`.
    ///
    /// `None` for `Resume`: see the type's own doc. A caller that unwraps this into a command is
    /// the window quietly cold-booting a machine whose label promised three seconds.
    pub fn cmd(self) -> Option<Cmd> {
        match self {
            Launch::Cold => Some(Cmd::PowerOn),
            Launch::Resume => None,
        }
    }
}

// ── §7.3: the cradle, as a function of state ─────────────────────────────────────────────────────

/// §7.3's ring. **Three colours and one shape** — `tests/bench.rs` asserts `CradleRing` is exactly
/// three values, and this is the Rust half of that same closed set.
///
/// The broken ring is **not** a fourth colour: §7.3 draws it `fg-dim` with gaps, so continuity is a
/// separate boolean on [`Cradle`] and the markup binds the two independently. A fourth variant here
/// would be this file disagreeing with `bench.slint` about what a ring is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ring {
    Accent,
    Dim,
    Danger,
}

/// One row of §7.3's table, drawn.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cradle {
    pub ring: Ring,
    /// Gaps in the outline. §7.3's three `cannot start` rows, and nothing else.
    pub broken: bool,
    /// The one line on the bench that says **what pressing will cost, before you press**.
    pub label: String,
}

/// Why `Start` cannot run, classified — the sentence itself is not written here.
///
/// **`main::blocked_label` words these and this module calls it**, which is the point:
/// `devices.rs`'s `start_row` already refuses a device with the same words on the Devices page, and
/// a second wording on the bench is how one iPod comes to be described two ways. It takes a
/// [`Blocked`] rather than re-testing the device, so there is one classification and one sentence
/// per row rather than two sequences of `if`s that have to be kept in the same order.
///
/// **There is no `Running` variant.** The cradle draws the device in the well, which is by
/// definition the live one; *another* device's `Start` being refused because this one is running is
/// §7.2's refusal and `devices::start_row` owns it. Adding it here would be the third copy of a
/// sentence whose own doc already records being written twice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Blocked {
    /// §7.3's `nothing is mounted`.
    Nothing,
    /// §3.3: one or more of its parts have left the library.
    Parts,
    /// Composed here, and this build cannot make its drive.
    Unbuilt,
    /// §10.3: a first run that stopped part-way. Unfinished, not broken.
    Unfinished,
}

impl Blocked {
    /// `None` when nothing is in the way.
    ///
    /// The order is the table's: a composed device is unfinished in the same way as a half-made
    /// one and the remedy is not the same, so the composed test comes first. **This is now the
    /// only place that order exists** — `main::blocked_label` is a `match` on what this returns,
    /// which is what makes two orders unwritable rather than merely discouraged.
    pub fn of(device: Option<&Device>, absent: &[Absent]) -> Option<Blocked> {
        let Some(d) = device else {
            return Some(Blocked::Nothing);
        };
        if !absent.is_empty() {
            return Some(Blocked::Parts);
        }
        if crate::composed_and_unbuilt(d) {
            return Some(Blocked::Unbuilt);
        }
        if !d.names_a_disk() {
            return Some(Blocked::Unfinished);
        }
        None
    }

    /// §9.4's two kinds. A file that is not there cannot be read by anything; everything else here
    /// is a thing this program has not written yet, which carries a command rather than a `Fix`.
    ///
    /// The same answer `devices::start_row` computes — `row.machine_rule = !gone.is_empty()`.
    pub fn machine_rule(self) -> bool {
        matches!(self, Blocked::Parts)
    }

    /// Whether the ring is drawn with gaps. §7.3 gives a broken ring to the three `cannot start`
    /// rows and to nothing else — an empty bench is *empty*, not broken, and a half-made device is
    /// unfinished.
    pub fn breaks_the_ring(self) -> bool {
        matches!(self, Blocked::Parts)
    }
}

/// Everything §7.3's caption is a function of.
///
/// **`cfg` is the thing whose absence deferred this whole table.** `main::cradle_label`'s
/// retirement condition says it in as many words — *"every §7.3 row but three needs a `Config` this
/// window does not hold"* — so it is an `Option` here, and `None` means the window has not resolved
/// a machine yet and every snapshot question answers [`Restore::Never`].
pub struct Stand<'a> {
    /// The device in the well.
    pub device: Option<&'a Device>,
    /// `Settings::missing_with`'s answer for that device, computed by the caller so one pass's
    /// stats are shared with everything else the page asks.
    pub absent: &'a [Absent],
    pub life: &'a Life,
    pub cfg: Option<&'a Config>,
    /// §12.4's ~1.6 GB write is under way. `Link::saving`.
    pub parking: bool,
}

impl Stand<'_> {
    /// §12.4's half of the pair, or `Never` when there is no machine configured.
    pub fn restore(&self) -> Restore {
        self.cfg.map_or(Restore::Never, Restore::of)
    }

    /// What pressing starts — **asked of `Config::may_restore`, not decided here**.
    ///
    /// `may_restore` is the one place that knows, and it knows three things this file would have to
    /// re-derive to answer: the `--cold` flag, the snapshot's existence, and which half of the pair
    /// this mode even has. `a_launch_is_exactly_what_may_restore_says` is the proof that asking and
    /// re-deriving still agree.
    pub fn launch(&self) -> Launch {
        match self.cfg {
            Some(c) if c.may_restore(true) => Launch::Resume,
            _ => Launch::Cold,
        }
    }
}

/// §7.3's table, evaluated.
///
/// **Rows this function owns**: parking, booting, running, stopped, nothing-mounted, the three
/// `cannot start` rows, startable-never-booted, startable-parked, and parked-pair-broken.
///
/// **Rows it deliberately does not**: `first run`, `first run, partly done` and the empty bench are
/// `main::empty_cradle_label` and `main::blocked_label`'s — this calls the latter rather than
/// re-typing it — `working` is `work::Queue`'s, and `a title` is §13, which has no boot and no 0.5
/// surface. The set is closed between the two files and
/// `every_row_of_the_seven_three_table_has_exactly_one_owner` is what keeps it closed.
///
/// `press` selects §7.3's prefix or §9.5's, exactly as `main::blocked_label` does, so a machine
/// caption drawn on the short-window pane does not point at a centre button that is not on screen.
pub fn cradle(press: crate::Press, st: &Stand) -> Cradle {
    // **Parking outranks everything**, including a stopped machine. It is a ~1.6 GB write the person
    // is waiting on, and §12.4's whole argument is that the bench must say what it is waiting for
    // rather than appearing to hang.
    if st.parking {
        return Cradle { ring: Ring::Dim, broken: false, label: PARKING.into() };
    }
    match st.life {
        Life::Booting { progress, .. } => Cradle {
            // **Asked of [`Life::ring`] rather than typed here**, and that is a defect this pass
            // found rather than a tidiness point: this `match` carried a second copy of §12.2's
            // ring column — `Dim`, `Dim`, `Danger` — beside the one `Life::ring` already answered,
            // and two tables for one column is how a `Stopped` machine comes to be drawn in the
            // colour that means *a file moved*. The `Off` row is the exception the type's own doc
            // names: `off_cradle` is the only thing that may answer `Dim` for a machine that is off,
            // because the broken half is a fact about the device's parts.
            ring: st.life.ring(),
            broken: false,
            // **The stop comes first and the progress second.** §7.3 writes it the other way —
            // `booting · 62 % · press ● to stop` — and that sentence is 48 characters at two digits
            // and 55 at the counted form, so the half that elides is the half §7.3 added the row
            // for: *"a user two minutes into a 21.5 G iPodLinux boot had an inert object in front
            // of them for the next twenty-one minutes"*. Reversed, the stop survives every width,
            // and `every_machine_caption_this_module_types_fits_its_own_row` measures that it does
            // rather than asserting it here in prose.
            label: format!("{} to stop — booting, {}", press.verb(), progress.caption()),
        },
        Life::Running { .. } => Cradle {
            ring: st.life.ring(),
            broken: false,
            // §7.3's table, first alternative: the bare word.
            //
            // **It carried the speed, and the first picture of a running bench is what found that
            // out.** [`Life::shelf`] is §12.2's fourth column and puts `running — 14 M instr/s` in
            // the shelf's row-1 trailing slot; this row was building the same string, and the two
            // are drawn about fifty pixels apart on one screen — so `bench-running.png` came out
            // with one sentence printed twice, one line above itself, and no test could see it
            // because each producer was right on its own.
            //
            // §7.3's own row offers three forms — `running`, `running · wheel 41 queued`, and the
            // fullscreen hint — and this module's header already records why the middle one is
            // unavailable: it names `Stats::queued`, which §12.8 decides does not earn a row. The
            // third needs `Window.full-screen`, which is §12.6 and not wired.
            //
            // **That left the bare word, and the bare word has stopped working.** It was chosen as
            // *the only one of the three that does not repeat what is already on screen*, which was
            // true while [`Life::shelf`] read `running — 14 M instr/s`; the shelf is a state slot
            // now and reads `running`, so the two are one word printed twice, fifty pixels apart —
            // which is the defect the speed was taken off this row for in the first place, arrived
            // at from the other direction.
            //
            // **So the cradle does its own job instead**, which §7.3 states as *what pressing will
            // cost, or why it cannot be pressed*. While a machine runs there is no press for this
            // caption to promise — §7.4 hands every drawn control to the machine — and that IS the
            // answer, said plainly rather than by omission. It is [`NO_MACHINE`]'s sentence with
            // the tense turned round: off, the wheel and buttons *work once the iPod runs*; running,
            // they are the iPod's. One fact, two faces, and neither is the shelf's word.
            label: WHEEL_IS_LIVE.into(),
        },
        Life::Stopped { reason, .. } => Cradle {
            ring: st.life.ring(),
            broken: false,
            // Exempt from the 48-character budget for the same reason `main::gone_sentence` is: it
            // carries a machine fact whose length is not ours, and the first words survive.
            label: format!("stopped — {}", reason.said()),
        },
        Life::Off => off_cradle(press, st),
    }
}

/// §12.4's caption while the restore point is being written.
///
/// **`parking` and nothing else.** §12.4 asks for `parking · 0.7 of 1.6 GB`, and `Link::saving` is
/// an `AtomicBool` — no bytes written, no bytes to write, nothing anywhere to divide. A fraction
/// invented from the snapshot's nominal size would be a bar that moves at a rate nobody measured,
/// which is the one thing §12.3 is written about.
const PARKING: &str = "parking";

/// The `Off` half of [`cradle`] — every row that is about a device rather than about a machine.
fn off_cradle(press: crate::Press, st: &Stand) -> Cradle {
    if let Some(b) = Blocked::of(st.device, st.absent) {
        let label = match st.device {
            Some(d) => crate::blocked_label(press, d, st.absent, b),
            None => NOTHING_MOUNTED.to_string(),
        };
        return Cradle { ring: Ring::Dim, broken: b.breaks_the_ring(), label };
    }
    let tail = match st.restore() {
        // `Whole` is the only row that may promise three seconds, and `Stand::launch` is what the
        // press will actually do — the two read the same `Config` and are checked against each
        // other in this module's tests.
        Restore::Whole => " — resume, about 3 s",
        // §7.3's own wording is `press ● to cold boot · the parked snapshot no longer matches this
        // drive`: 71 characters against a 48-character row. `no resume` is the half a person needs
        // before pressing — the snapshot is not being used — and *why* it is not is a paragraph,
        // which §7.3 already puts on the device's drawer page beside `Discard the snapshot`.
        Restore::Broken => " — no resume, about 75 s",
        Restore::Never => " — cold boot, about 75 s",
    };
    Cradle { ring: Ring::Accent, broken: false, label: format!("{}{tail}", press.verb()) }
}

// ── §7.4 and §12.5: what a press does, and what a phase permits ──────────────────────────────────

/// What the centre button means right now.
///
/// One value, consulted by the ring, the label and the press — the same rule
/// `main::composed_and_unbuilt` states for its own boolean: *three answers to "can this be pressed"
/// is how two of them come to disagree silently*.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Act {
    /// Build the machine, or power it back on.
    Start(Launch),
    /// §7.3 and §12.5: the centre button is live during `Booting` and sends `Cmd::PowerOff`,
    /// *"because a twenty-one-minute boot with no stop control is not a design, it is a hostage
    /// situation"*.
    Stop,
    /// §7.4: the press belongs to the machine and to nothing else.
    ToMachine,
    /// §14.1: drawn, refused, and says why.
    Refuse(Blocked),
}

/// [`Act`] for this stand.
///
/// **A `Stopped` machine starts cold, never resumed.** §7.3's stopped row offers `Cold boot` and
/// nothing else, and `Cmd::PowerOn` is a cold boot by construction — so this cannot answer `Resume`
/// there even if a perfectly good snapshot is on disk. Pressing after a `Lost(0xe19b0000)` restores
/// nothing; it starts again.
pub fn centre(st: &Stand) -> Act {
    match st.life {
        Life::Booting { .. } => Act::Stop,
        Life::Running { .. } => Act::ToMachine,
        Life::Stopped { .. } => match Blocked::of(st.device, st.absent) {
            Some(b) => Act::Refuse(b),
            None => Act::Start(Launch::Cold),
        },
        Life::Off => match Blocked::of(st.device, st.absent) {
            Some(b) => Act::Refuse(b),
            None => Act::Start(st.launch()),
        },
    }
}

/// §7.4: whether the wheel, MENU, Prev, Next, Play and the hold switch reach anything.
///
/// `None` when they do. `Some` is the sentence §7.3 holds while the pointer is down — and the
/// control stays **live** either way: §7.3's own note is that `accessible-enabled` is the
/// announcement and never the gate, because gating the keyboard half and not the pointer half made
/// `Return` a dead key on exactly the device that most needed to say something.
pub fn no_machine(life: &Life) -> Option<&'static str> {
    (!life.alive()).then_some(NO_MACHINE)
}

/// §12.5: whether this phase permits this command.
///
/// **Every refusal is a physical statement, not a policy.** You cannot power off a machine that is
/// off; you cannot power on one that is already running; a power *cycle* needs something to cycle.
/// `Boot(target)` is permitted everywhere because §12.5 makes it a power cycle in every case —
/// *"that is how the hardware reaches them"* — so from `Off` it is a power-on into a target and
/// from `Running` it is a drop and a re-entry.
pub fn permits(life: &Life, c: &Cmd) -> bool {
    match c {
        Cmd::Boot(_) => true,
        Cmd::PowerOn => !life.alive(),
        Cmd::PowerOff | Cmd::PowerCycle => life.alive(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use eapp_loader::nor;
    use eapp_loader::settings::{Disk, Item, Presence, Resource, Settings};
    use std::path::PathBuf;

    /// A scratch directory of this test's own. Never inside the operator's data directory.
    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ipod-machine-{}-{tag}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    fn stats(here: u64, wall: f64) -> Stats {
        Stats { executed: here, executed_here: here, wall_secs: wall, ..Stats::default() }
    }

    /// An `Out` in one phase. Everything else is the default the run loop starts from.
    fn out(phase: Phase, s: Stats) -> Out {
        Out {
            phase,
            fb: Vec::new(),
            fb_nonzero: 0,
            fb_addr: 0,
            fb_seq: 0,
            backlight: 0,
            backlight_steps: (0, 0),
            stalled_secs: 0.0,
            unmapped_pages: Vec::new(),
            pc_trace: Vec::new(),
            pmu_written: Vec::new(),
            watched_writes: Vec::new(),
            bus_log: Vec::new(),
            fb_other_nonzero: 0,
            fb_other_moved: false,
            fb_shown_moved: false,
            booted_at: None,
            stats: s,
        }
    }

    /// Every phase the emulator publishes, so no test can walk three and believe it walked the set.
    fn all_phases() -> Vec<Phase> {
        vec![
            Phase::Off,
            Phase::Booting { target: 1_600_000_000 },
            Phase::Running,
            Phase::Stopped("Lost(0xe19b0000) at 128000 instructions".into()),
        ]
    }

    fn device(name: &str, disk: Option<&str>) -> Device {
        Device {
            name: name.into(),
            firmware: "an iPod".into(),
            disk: disk.map(str::to_string),
            ..Device::default()
        }
    }

    /// A settings file holding one iPod and one drive **that is on disk**, so `missing_with`
    /// answers `[]` and every refusal below has to be caused rather than inherited.
    ///
    /// The boot ROM is synthesised, which has no file anywhere and never goes missing — so the one
    /// part a test can take away is the drive, and `a_part_that_has_left` is the only way this
    /// fixture reaches §7.3's `cannot start` rows.
    fn library(dir: &Path) -> (Settings, Device) {
        let img = dir.join("mine.img");
        std::fs::write(&img, b"not a drive, but it is there").expect("a scratch drive");
        let s = Settings {
            resources: vec![Item {
                name: "an iPod".into(),
                what: Resource::Firmware(nor::Source::Synthetic {
                    model: "MA146".into(),
                    seed: 20_266,
                    serial: None,
                    guid: None,
                    splash: None,
                }),
                from: None,
            }],
            disks: vec![Disk { name: "mine".into(), path: img, ..Disk::default() }],
            ..Settings::default()
        };
        (s, device("My 5.5G", Some("mine")))
    }

    /// A config whose snapshot and drive are both under `dir`.
    fn config(dir: &Path, snapshot: bool) -> Config {
        let snap = dir.join("m.snap");
        if snapshot {
            std::fs::write(&snap, b"ram").expect("a scratch snapshot");
        } else {
            let _ = std::fs::remove_file(&snap);
        }
        let workdisk = dir.join("work.img");
        std::fs::write(&workdisk, b"drive").expect("a scratch work disk");
        Config { snapshot: Some(snap), workdisk, ..Config::default() }
    }

    fn stand<'a>(
        d: Option<&'a Device>,
        absent: &'a [Absent],
        life: &'a Life,
        cfg: Option<&'a Config>,
    ) -> Stand<'a> {
        Stand { device: d, absent, life, cfg, parking: false }
    }

    // ── §12.2: the four phases ───────────────────────────────────────────────────────────────────

    /// **Every phase the machine publishes becomes exactly one [`Life`], and each carries what
    /// §12.2's table draws beside it.**
    ///
    /// The set is walked from [`all_phases`] rather than typed here, so a fifth phase added to
    /// `emu::Phase` fails this rather than being silently unmodelled — which is the shape of gap
    /// this repository keeps finding, an absence no green test could observe.
    #[test]
    fn every_phase_the_machine_publishes_becomes_exactly_one_life() {
        let target = BootTarget::Os;
        let mut seen: Vec<&str> = Vec::new();
        for p in all_phases() {
            let o = out(p.clone(), stats(487_220_016, 34.8));
            let life = Life::read(&o, &target, Some(1_600_000_000));
            let name = match (&p, &life) {
                (Phase::Off, Life::Off) => "off",
                (Phase::Booting { .. }, Life::Booting { progress, .. }) => {
                    assert_eq!(progress.percent(), Some(30), "the bar is over the wrong number");
                    "booting"
                }
                (Phase::Running, Life::Running { pace, .. }) => {
                    assert!(pace.speed().is_some(), "a running machine reported no speed");
                    "running"
                }
                (Phase::Stopped(why), Life::Stopped { reason, .. }) => {
                    assert_eq!(reason.said(), why.trim(), "the reason was reworded");
                    "stopped"
                }
                _ => panic!("{p:?} became {life:?}, which is a different phase"),
            };
            seen.push(name);
        }
        assert_eq!(seen, ["off", "booting", "running", "stopped"]);
    }

    /// **`Off` is genuinely one of them**, and it carries nothing to lie with.
    ///
    /// §12.2: *"no machine exists, nothing is executing, and the panel is dark"*. The machine that
    /// produces this `Out` has 1.6 G instructions on its counters — a real power-off leaves the
    /// stats where they were — and `Life::Off` still has no pace, no progress and no reason.
    #[test]
    fn an_off_machine_carries_no_evidence_that_it_is_doing_anything() {
        let o = out(Phase::Off, stats(1_612_004_992, 34.8));
        let life = Life::read(&o, &BootTarget::Os, Some(1_600_000_000));
        assert_eq!(life, Life::Off);
        assert!(!life.alive());
        assert_eq!(Glass::of(&life, None, false), Glass::Dark, "an off panel kept a frame");
        assert_eq!(life.shelf(), "off");
        assert_eq!(no_machine(&life), Some(NO_MACHINE));
    }

    /// **A stopped machine keeps its last frame and its numbers; an off one keeps neither.**
    ///
    /// The two rows §12.2 puts side by side for opposite reasons, measured against each other so a
    /// future edit cannot make them the same by making one of them "safe".
    #[test]
    fn the_glass_keeps_a_stopped_frame_and_never_an_off_one() {
        let s = stats(1_612_004_992, 34.8);
        let stopped = Life::read(&out(Phase::Stopped("Lost".into()), s), &BootTarget::Os, None);
        let off = Life::read(&out(Phase::Off, s), &BootTarget::Os, None);
        assert_eq!(Glass::of(&stopped, None, false), Glass::Held);
        assert_eq!(Glass::of(&off, None, false), Glass::Dark);
        assert_eq!(stopped.ring(), Ring::Danger);
        assert_eq!(off.ring(), Ring::Accent);
    }

    /// **A stopped machine that said nothing still says something.**
    #[test]
    fn a_stopped_machine_with_no_reason_reports_that_it_gave_none() {
        for said in ["", "   ", "\n"] {
            let life = Life::read(&out(Phase::Stopped(said.into()), stats(1, 1.0)), &BootTarget::Os, None);
            let Life::Stopped { reason, .. } = &life else { panic!("not stopped") };
            assert_eq!(reason.said(), Reason::UNSAID);
            let c = cradle(crate::Press::Centre, &stand(None, &[], &life, None));
            assert_eq!(c.label, format!("stopped — {}", Reason::UNSAID));
            assert!(!c.label.ends_with("— "), "a danger ring over a blank sentence");
        }
    }

    // ── §12.3: progress, honestly ────────────────────────────────────────────────────────────────

    /// **A bar over nothing is not representable**, and the two ways of asking for one both demote.
    ///
    /// §12.3: *"Before a device has ever booted there is no fraction and no bar."* `None` is the
    /// device that never booted; `Some(0)` is the settings file that says it booted in no
    /// instructions, which is the same claim spelled differently.
    #[test]
    fn a_boot_with_no_denominator_has_a_count_and_never_a_fraction() {
        for denominator in [None, Some(0)] {
            let p = Progress::read(412_000_000, denominator);
            assert_eq!(p, Progress::Counted { instructions: 412_000_000 });
            assert_eq!(p.percent(), None, "a fraction was produced over {denominator:?}");
            assert_eq!(p.caption(), "412 M instr");
        }
        let p = Progress::read(992_000_000, Some(1_600_000_000));
        assert_eq!(p.percent(), Some(62));
        assert_eq!(p.caption(), "62 %");
    }

    /// **The denominator is the device's own last boot, never the snapshot instant.**
    ///
    /// `Phase::Booting { target }` is `Config::snap_at`. Feeding it to the bar draws a plausible
    /// number against the wrong question, which is the failure nobody would see: here the two
    /// differ by a factor of eight and the bar would read 240 %.
    #[test]
    fn the_boot_bar_divides_by_the_last_cold_boot_and_not_by_the_snapshot_instant() {
        let snap_at = 200_000_000;
        let o = out(Phase::Booting { target: snap_at }, stats(480_000_000, 4.0));
        let life = Life::read(&o, &BootTarget::Os, Some(1_600_000_000));
        let Life::Booting { progress, .. } = &life else { panic!("not booting") };
        assert_eq!(progress.percent(), Some(30));
        assert_eq!(
            Progress::read(480_000_000, Some(snap_at)).percent(),
            Some(240),
            "the two numbers are not distinguishable, so this test proves nothing"
        );
    }

    /// **A reading past 100 % is reported, not clamped** — §12.3's recipe-changed defect stays
    /// visible, because the remedy is `Settings::set_boot_shape` upstream and hiding it here would
    /// take the symptom away and leave the cause.
    #[test]
    fn a_bar_past_one_hundred_per_cent_is_not_quietly_clamped() {
        assert_eq!(Progress::read(1_600_000_000, Some(100_000_000)).percent(), Some(1600));
    }

    /// **An unmeasured speed is `None` and never `0`** — §12.8's rule, one level below the Gauge.
    #[test]
    fn a_machine_with_no_wall_time_has_no_speed_rather_than_a_speed_of_zero() {
        assert_eq!(Pace::of(&stats(0, 0.0)).speed(), None);
        assert_eq!(Pace::of(&stats(0, 0.0)).caption(), None);
        // And a genuine zero is a zero: wall time passed and nothing ran.
        assert_eq!(Pace::of(&stats(0, 4.0)).speed(), Some(0.0));
        let fast = Pace::of(&stats(487_220_016, 34.8));
        assert_eq!(fast.caption().as_deref(), Some("14 M instr/s"));
    }

    /// §12.2's fifth thing, and it only fires where standing still is wrong.
    #[test]
    fn a_stall_is_only_reported_of_a_machine_that_is_supposed_to_be_moving() {
        let mut o = out(Phase::Running, stats(2_791_999_952, 400.0));
        o.stalled_secs = 6.4;
        assert_eq!(Life::read(&o, &BootTarget::Os, None).stalled(), Some(6.4));
        o.stalled_secs = 1.9;
        assert_eq!(Life::read(&o, &BootTarget::Os, None).stalled(), None, "under the threshold");
        let mut off = out(Phase::Off, stats(2_791_999_952, 400.0));
        off.stalled_secs = 6.4;
        assert_eq!(Life::read(&off, &BootTarget::Os, None).stalled(), None, "an off machine");
    }

    /// §12.8's four freshness states, all reachable, and `Final` is not `Stale`.
    #[test]
    fn freshness_tells_final_from_stale_and_unmeasured_from_zero() {
        let running = Life::read(&out(Phase::Running, stats(1, 1.0)), &BootTarget::Os, None);
        let stopped = Life::read(&out(Phase::Stopped("Lost".into()), stats(1, 1.0)), &BootTarget::Os, None);
        let off = Life::read(&out(Phase::Off, stats(1, 1.0)), &BootTarget::Os, None);
        assert_eq!(Freshness::of(&running, Some(120)), Freshness::Live);
        assert_eq!(Freshness::of(&running, Some(SAMPLE_FRESH_MS + 1)), Freshness::Stale);
        assert_eq!(Freshness::of(&stopped, Some(120)), Freshness::Final);
        assert_eq!(Freshness::of(&off, Some(120)), Freshness::Stale);
        assert_eq!(Freshness::of(&running, None), Freshness::Unmeasured);
    }

    // ── §12.4: parking and restoring ─────────────────────────────────────────────────────────────

    /// **A launch is exactly what `Config::may_restore` says**, and this is the test that says the
    /// model asks rather than deciding.
    ///
    /// Four configs across the two axes `may_restore` actually reads — a snapshot on disk or not,
    /// and `--cold` or not — with `work_on_copy` false, which is the shipped default and the mode
    /// where `pair_is_whole` reads a stamp file rather than testing for a frozen clone. The stamp
    /// is written by `Config::pair_with_drive`, so the whole pair is built the way the machine
    /// builds it and not by hand.
    #[test]
    fn a_launch_is_exactly_what_may_restore_says() {
        let dir = scratch("launch");
        let mut seen: Vec<(bool, bool, Launch, Restore)> = Vec::new();
        for snapshot in [false, true] {
            for cold in [false, true] {
                let mut cfg = config(&dir, snapshot);
                cfg.cold = cold;
                if snapshot {
                    cfg.pair_with_drive().expect("the drive stamp");
                }
                let life = Life::Off;
                let st = stand(None, &[], &life, Some(&cfg));
                let launch = st.launch();
                assert_eq!(
                    launch == Launch::Resume,
                    cfg.may_restore(true),
                    "snapshot={snapshot} cold={cold}: the model and `may_restore` disagree"
                );
                seen.push((snapshot, cold, launch, st.restore()));
            }
        }
        // The control: all four cells are not the same answer, so the assertion above is comparing
        // something. Exactly one of the four resumes.
        assert_eq!(
            seen.iter().filter(|(_, _, l, _)| *l == Launch::Resume).count(),
            1,
            "{seen:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A snapshot whose drive moved is `Broken`, not `Whole`** — and the cradle stops promising
    /// three seconds the moment it is.
    #[test]
    fn a_drive_that_moved_under_its_snapshot_breaks_the_pair_and_the_promise() {
        let dir = scratch("pair");
        let cfg = config(&dir, true);
        cfg.pair_with_drive().expect("the drive stamp");
        assert_eq!(Restore::of(&cfg), Restore::Whole);

        let life = Life::Off;
        let (s, d) = library(&dir);
        let absent = s.missing_with(&d, &mut Presence::new());
        assert!(absent.is_empty(), "the fixture device is not intact: {absent:?}");
        let whole = cradle(crate::Press::Centre, &stand(Some(&d), &absent, &life, Some(&cfg)));
        assert_eq!(whole.label, "Press the centre button — resume, about 3 s");
        assert_eq!(whole.ring, Ring::Accent);

        // Something else wrote to the drive — `ipod-boot put-files`, iTunes, a second window.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&cfg.workdisk, b"drive and more").expect("the drive moving on");
        assert_eq!(Restore::of(&cfg), Restore::Broken);
        let broken = cradle(crate::Press::Centre, &stand(Some(&d), &absent, &life, Some(&cfg)));
        assert_eq!(broken.label, "Press the centre button — no resume, about 75 s");
        assert!(!broken.label.contains("3 s"), "a broken pair still promised a resume");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A parked glass shows a frame that is there, and is dark when it is not** — §12.4's honest
    /// fallback, and `Glass::Parked` cannot be built without a path that existed when asked.
    #[test]
    fn a_parked_glass_needs_a_png_that_is_actually_on_disk() {
        let dir = scratch("parked");
        let cfg = config(&dir, true);
        let mut seen = Presence::new();
        assert_eq!(parked_frame(&cfg, &mut seen), None);
        assert_eq!(Glass::of(&Life::Off, None, false), Glass::Dark);

        let png = dir.join("m.parked.png");
        std::fs::write(&png, b"\x89PNG").expect("a scratch frame");
        let mut fresh = Presence::new();
        let found = parked_frame(&cfg, &mut fresh).expect("the frame beside the snapshot");
        assert_eq!(found, png, "the frame is not beside the snapshot under the same stem");
        assert_eq!(Glass::of(&Life::Off, Some(&found), false), Glass::Parked(png));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A machine that has drawn is shown, not covered.**
    ///
    /// `Booting` returned [`Glass::Boot`] whatever the panel held, so a synthesised ROM's mark was
    /// painted over a framebuffer with 71 695 lit pixels of 76 800 in it — and because the boot it
    /// was covering had stalled, for ever. The operator reported the program *"stuck on the
    /// synthesised logo"* twice; the second time the machine behind that logo had filled its screen.
    ///
    /// The substitute is for a machine that has not drawn. Once it has, it is the machine's picture.
    #[test]
    fn a_booting_machine_that_has_drawn_shows_its_own_panel() {
        let booting = Life::Booting {
            target: BootTarget::default(),
            progress: Progress::read(1_000, Some(800_000_000)),
        };
        assert_eq!(
            Glass::of(&booting, None, false),
            Glass::Boot,
            "a machine that has drawn nothing has nothing of its own to show"
        );
        assert_eq!(
            Glass::of(&booting, None, true),
            Glass::Live,
            "the ROM's boot screen is still being painted over a panel with something on it"
        );
        // A drawn frame does not overrule the states that are not about drawing.
        assert_eq!(Glass::of(&Life::Off, None, true), Glass::Dark);
    }


    /// **Parking says what it is waiting for and outranks every other row** — §12.4, and it says
    /// only what is known.
    #[test]
    fn parking_outranks_the_phase_and_claims_no_fraction() {
        let life = Life::read(&out(Phase::Running, stats(487_220_016, 34.8)), &BootTarget::Os, None);
        let mut st = stand(None, &[], &life, None);
        // The control: this is the `Running` row, so `parking` below is the rank rather than the
        // only caption this stand could produce.
        assert_eq!(cradle(crate::Press::Centre, &st).label, WHEEL_IS_LIVE);
        st.parking = true;
        let c = cradle(crate::Press::Centre, &st);
        assert_eq!(c.label, "parking");
        assert!(!c.label.contains("GB"), "a byte count nothing publishes");
    }

    // ── §7.3: the cradle table ───────────────────────────────────────────────────────────────────

    /// **Every row of §7.3 this module owns, drawn once, with its ring and its continuity.**
    #[test]
    fn the_seven_three_rows_this_module_owns_are_all_reachable() {
        let dir = scratch("rows");
        let (s, d) = library(&dir);
        let intact = s.missing_with(&d, &mut Presence::new());
        let gone = vec![Absent::Gone(dir.join("not-here.img"))];
        let half = device("My 5.5G", None);
        let cfg = config(&dir, false);

        let booting = Life::read(
            &out(Phase::Booting { target: 0 }, stats(992_000_000, 8.0)),
            &BootTarget::Os,
            Some(1_600_000_000),
        );
        let running = Life::read(&out(Phase::Running, stats(487_220_016, 34.8)), &BootTarget::Os, None);
        let stopped = Life::read(
            &out(Phase::Stopped("Lost(0xe19b0000)".into()), stats(1, 1.0)),
            &BootTarget::Os,
            None,
        );
        let off = Life::Off;

        let rows: Vec<(&str, Cradle)> = vec![
            ("booting", cradle(crate::Press::Centre, &stand(Some(&d), &intact, &booting, None))),
            ("running", cradle(crate::Press::Centre, &stand(Some(&d), &intact, &running, None))),
            ("stopped", cradle(crate::Press::Centre, &stand(Some(&d), &intact, &stopped, None))),
            ("cold", cradle(crate::Press::Centre, &stand(Some(&d), &intact, &off, Some(&cfg)))),
            ("gone", cradle(crate::Press::Centre, &stand(Some(&d), &gone, &off, Some(&cfg)))),
            ("half", cradle(crate::Press::Centre, &stand(Some(&half), &[], &off, Some(&cfg)))),
            ("nothing", cradle(crate::Press::Centre, &stand(None, &[], &off, None))),
        ];

        for (name, c) in &rows {
            assert!(!c.label.is_empty(), "{name} drew an empty caption");
        }
        let by = |n: &str| rows.iter().find(|(k, _)| *k == n).map(|(_, c)| c.clone()).unwrap();
        assert_eq!(by("booting").label, "Press the centre button to stop — booting, 62 %");
        assert_eq!(by("running").ring, Ring::Dim);
        assert_eq!(by("stopped").ring, Ring::Danger);
        assert_eq!(by("cold").label, "Press the centre button — cold boot, about 75 s");
        assert_eq!(by("cold").ring, Ring::Accent);
        assert_eq!(by("nothing").label, NOTHING_MOUNTED);

        // **The broken ring belongs to the three `cannot start` rows and to nothing else.**
        let broken: Vec<&str> = rows.iter().filter(|(_, c)| c.broken).map(|(n, _)| *n).collect();
        assert_eq!(broken, ["gone"], "the ring is broken on the wrong rows");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **Every refusal on the bench is the sentence the Devices page already uses.**
    ///
    /// Not "is similar to" — is the same call. `main::blocked_label` is the producer for all
    /// three device refusals, and this compares what [`cradle`] drew against what that function
    /// returns for the same inputs, so a reworded arm moves both surfaces or fails here.
    ///
    /// **It asks the producer directly rather than through `main::cradle_label_at`**, which since
    /// Phase 7 routes through [`cradle`] itself — comparing this function's output with its own
    /// would be an equality that cannot fail.
    #[test]
    fn the_bench_refuses_a_device_in_the_devices_pages_own_words() {
        let dir = scratch("words");
        let (_, d) = library(&dir);
        let half = device("My 5.5G", None);
        let composed = Device { composed: true, ..device("Composed", None) };
        let gone = vec![Absent::Gone(dir.join("mine.img"))];
        let off = Life::Off;

        for (what, dev, absent) in [
            ("gone", &d, gone.as_slice()),
            ("half-made", &half, &[]),
            ("composed", &composed, &[]),
        ] {
            let drew = cradle(crate::Press::Centre, &stand(Some(dev), absent, &off, None));
            let b = Blocked::of(Some(dev), absent).expect("every row here is a refusal");
            assert_eq!(
                drew.label,
                crate::blocked_label(crate::Press::Centre, dev, absent, b),
                "the bench and the Devices page word `{what}` differently"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **Both press surfaces share every machine caption's tail** — §9.5's rule, applied to the
    /// rows this module added rather than restated for them.
    #[test]
    fn a_machine_caption_names_the_press_the_surface_actually_draws() {
        let dir = scratch("press");
        let (s, d) = library(&dir);
        let absent = s.missing_with(&d, &mut Presence::new());
        let cfg = config(&dir, false);
        let off = Life::Off;
        let here = cradle(crate::Press::Here, &stand(Some(&d), &absent, &off, Some(&cfg)));
        let centre = cradle(crate::Press::Centre, &stand(Some(&d), &absent, &off, Some(&cfg)));
        assert_eq!(here.label, "Press here — cold boot, about 75 s");
        assert!(!here.label.contains("centre button"), "§9.5's pane names a control it does not draw");
        assert_eq!(
            here.label.trim_start_matches("Press here"),
            centre.label.trim_start_matches("Press the centre button"),
            "the two surfaces disagree about what pressing costs"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **The captions this module types fit the row they are drawn on** — and the three that cannot
    /// are held to what survives the elision instead of being skipped.
    ///
    /// `geometry::CRADLE_LABEL_MAX_CHARS` is 48 at the smallest window this program draws a device
    /// on. Skipping a long sentence is what produced the defect §7.3's own budget exists for — *Press
    /// the centre button — running is not wired* at 63 characters, eliding unmeasured at every
    /// window size this program allows. So the over-budget class is measured too, on the property
    /// that actually matters: **the words a person needs are inside the first 48**.
    ///
    /// Three are over, all for the same reason — they end with a number or a sentence whose length
    /// is the machine's, not ours. `counted` is 55 and loses only the tail of an instruction count;
    /// `stopped` is however long a `Stop` variant is; §7.4's `NO_MACHINE` is 71 and is the design's
    /// own wording, recorded in GUI.md §7.3 rather than quietly reworded here.
    #[test]
    fn every_machine_caption_this_module_types_fits_its_own_row() {
        let budget = crate::geometry::CRADLE_LABEL_MAX_CHARS;
        let verb = crate::Press::Centre.verb();
        // The control first: the budget is a real number and not a `usize::MAX` anything passes.
        assert!((24..=120).contains(&budget), "{budget} is not a sentence-sized budget");

        for (what, said) in [
            ("cold", format!("{verb} — cold boot, about 75 s")),
            ("resume", format!("{verb} — resume, about 3 s")),
            ("no resume", format!("{verb} — no resume, about 75 s")),
            ("booting", format!("{verb} to stop — booting, 62 %")),
            ("running", WHEEL_IS_LIVE.to_string()),
            ("parking", PARKING.to_string()),
            ("nothing", NOTHING_MOUNTED.to_string()),
            // §7.4's two refusals, which used to be two of the three below. `bench-refused.png`
            // is why they moved: both cut off mid-clause on the row they are drawn on.
            ("no machine", NO_MACHINE.to_string()),
            ("no machine, hold", NO_MACHINE_HOLD.to_string()),
        ] {
            assert!(
                said.chars().count() <= budget,
                "the `{what}` caption is {} characters against a {budget}-character row: {said}",
                said.chars().count()
            );
        }

        // **And the two that do not fit put the load-bearing words first.** `survives` is what a
        // person reads at the smallest window; every clause named beside it has to be in there.
        //
        // It was three. Both of §7.4's refusals were down here on the argument that their first
        // clause carries the meaning — true, and not the whole question: what a person read ended
        // on `and there is no`, which is not a graceful elision but a sentence stopping in the
        // middle of the noun it came to say. The two that are left carry a length this program does
        // not choose — an instruction count and an emulator's own stop reason — which is the
        // difference that earns the exemption.
        for (what, said, needs) in [
            (
                "counted",
                format!("{verb} to stop — booting, 412 M instr"),
                vec!["centre button", "stop", "booting"],
            ),
            (
                "stopped",
                "stopped — Lost(0xe19b0000) at 128000 instructions".to_string(),
                vec!["stopped", "Lost"],
            ),
        ] {
            let survives: String = said.chars().take(budget).collect();
            assert!(
                said.chars().count() > budget,
                "the `{what}` caption fits after all — move it up into the list above: {said}"
            );
            for clause in needs {
                assert!(
                    survives.contains(clause),
                    "`{what}` elides to `{survives}…`, which has lost `{clause}`"
                );
            }
        }
    }

    // ── §7.4 and §12.5: presses and permissions ──────────────────────────────────────────────────

    /// **What the centre button means in each phase**, including the stop §7.3 added because there
    /// was no stop control on the bench at all.
    #[test]
    fn the_centre_button_stops_a_boot_and_reaches_the_machine_while_it_runs() {
        let dir = scratch("centre");
        let (s, d) = library(&dir);
        let absent = s.missing_with(&d, &mut Presence::new());
        let cfg = config(&dir, false);

        let booting = Life::read(&out(Phase::Booting { target: 0 }, stats(1, 1.0)), &BootTarget::Os, None);
        let running = Life::read(&out(Phase::Running, stats(1, 1.0)), &BootTarget::Os, None);
        let stopped = Life::read(&out(Phase::Stopped("Lost".into()), stats(1, 1.0)), &BootTarget::Os, None);

        assert_eq!(centre(&stand(Some(&d), &absent, &booting, Some(&cfg))), Act::Stop);
        assert_eq!(centre(&stand(Some(&d), &absent, &running, Some(&cfg))), Act::ToMachine);
        // A stopped machine starts cold even with a perfectly good snapshot beside it.
        assert_eq!(
            centre(&stand(Some(&d), &absent, &stopped, Some(&cfg))),
            Act::Start(Launch::Cold)
        );
        assert_eq!(
            centre(&stand(None, &[], &Life::Off, None)),
            Act::Refuse(Blocked::Nothing)
        );
        assert_eq!(Launch::Cold.cmd(), Some(Cmd::PowerOn));
        assert_eq!(Launch::Resume.cmd(), None, "a resume is not reachable by command");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **The drawn controls belong to the machine, and say so when there is none** — §7.4, both
    /// halves, so a phase cannot be added on one side only.
    #[test]
    fn the_wheel_and_the_buttons_reach_a_machine_only_while_one_exists() {
        let target = BootTarget::Os;
        for p in all_phases() {
            let life = Life::read(&out(p.clone(), stats(1, 1.0)), &target, None);
            let reaches = no_machine(&life).is_none();
            assert_eq!(
                reaches,
                matches!(p, Phase::Booting { .. } | Phase::Running),
                "{p:?}: §7.4 and `Life::alive` disagree about whether there is a machine"
            );
        }
    }

    /// **§12.5's permission table refuses something in every phase and permits something in every
    /// phase** — a table that answered `true` everywhere would pass any per-case assertion.
    #[test]
    fn every_power_command_is_refused_somewhere_and_permitted_somewhere() {
        let target = BootTarget::Os;
        let lives: Vec<Life> =
            all_phases().iter().map(|p| Life::read(&out(p.clone(), stats(1, 1.0)), &target, None)).collect();
        let cmds = [
            Cmd::PowerOff,
            Cmd::PowerOn,
            Cmd::PowerCycle,
            Cmd::Boot(BootTarget::Nor("diag".into())),
        ];
        for c in &cmds {
            let yes = lives.iter().filter(|l| permits(l, c)).count();
            // `Boot` is the one §12.5 permits everywhere, because it is a power cycle in every case.
            let want_all = matches!(c, Cmd::Boot(_));
            assert!(yes > 0, "{c:?} is permitted in no phase at all");
            assert_eq!(
                yes == lives.len(),
                want_all,
                "{c:?} is permitted in {yes} of {} phases",
                lives.len()
            );
        }
        // And the one that matters: a boot can be stopped.
        let booting = Life::read(&out(Phase::Booting { target: 0 }, stats(1, 1.0)), &target, None);
        assert!(permits(&booting, &Cmd::PowerOff));
        assert!(!permits(&Life::Off, &Cmd::PowerOff), "an off machine can be powered off");
    }

    /// **A park with no room does not happen, and an unmeasured free space is not "no room."**
    ///
    /// §12.4 checks before `save_on_quit` is set because a park that fails for want of space fails
    /// at window close, where there is nothing left to report it in. The half worth a test is the
    /// *other* direction: `free_bytes` answers `None` where this program cannot ask, and treating
    /// that as short would lose a restore point over a question nobody put. §12.8's rule about the
    /// model long before it is a rule about a Gauge — a zero and an unmeasured are different facts.
    #[test]
    fn a_park_refuses_only_on_a_free_space_it_actually_measured() {
        let dir = scratch("park-room");

        // The real reading, on a directory that exists. Asserted as a range rather than a number
        // because it is a fact about the volume this test ran on: any answer at all is what is
        // under test, and a `Some(0)` on a temp directory would mean `f_bavail` was read as
        // `f_bfree`'s root reserve or as blocks rather than bytes.
        let real = Park::of(0, &dir);
        assert!(
            real.free.is_some_and(|f| f > 1 << 20),
            "statvfs reported {:?} free on a writable scratch directory",
            real.free
        );
        assert!(!real.short(), "nothing needed cannot be short of anything");

        // A path nothing answers for. **Not short**: it is unmeasured.
        let nowhere = Park::of(u64::MAX, &dir.join("no-such-directory"));
        assert_eq!(nowhere.free, None);
        assert!(
            !nowhere.short(),
            "a park was refused on a free space nobody measured, which loses a restore point over \
             an unanswered question"
        );
        assert!(
            nowhere.sentence().contains("unmeasured"),
            "the sentence claims a figure it does not have: {}",
            nowhere.sentence()
        );

        // And the case the check exists for: more wanted than there is.
        let short = Park { needed: u64::MAX, free: Some(900_000_000) };
        assert!(short.short());
        assert!(
            short.sentence().contains("needed") && short.sentence().contains("free"),
            "§12.4's sentence is two figures and a person has to be able to check the arithmetic: {}",
            short.sentence()
        );
        // Exactly enough is enough. `<` and not `<=`, because refusing a park that fits is the
        // same lost restore point by a different route.
        assert!(!Park { needed: 100, free: Some(100) }.short());
        assert!(Park { needed: 101, free: Some(100) }.short());

        std::fs::remove_dir_all(&dir).ok();
    }
}
