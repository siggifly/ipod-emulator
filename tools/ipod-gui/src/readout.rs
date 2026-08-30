//! **§12.8's Readout** — every counter a running machine publishes, as Gauges, with no toolkit in
//! it.
//!
//! The same rule `machine.rs` is built on and for the same reason: what a number *is* and what it
//! is worth saying are decided with no display in the room, and every test below runs on a machine
//! with no display stack.
//!
//! ## What this module refuses to draw, and why each refusal is §12.8's own
//!
//! - **`ratio · 24.1 % of real` has no stated divisor.** §12.8 works the arithmetic three ways and
//!   its own diagram matches none of them: `14.2 M instr/s` against a real 5G is 18.9 %, simulated
//!   against wall on the same diagram is 61.8 %, and `487 220 016` instructions in `21.5 s`
//!   simulated is 22.7 instructions per simulated microsecond, which is neither 5 nor 75. *"The row
//!   comes back when somebody writes down what it divides by."* [`Group::Machine`] draws the four
//!   numbers a ratio would be made of and no ratio.
//! - **`Stats::queued` does not earn a row**, and the field is **deleted** rather than allowed:
//!   *"`input_dropped` is the number that matters, because a refused step is a lie about what you
//!   did and a deep queue is only ever the reason for one."*
//! - **CORES is one column.** See [`Group::Cores`], which carries the whole of §17 Q10's answer.
//!
//! ## The middle dot, again
//!
//! §12.8 marks *"a counter that starts at zero after a restore"* with `·`, and `geometry::GLYPHS`
//! is a closed set of three that does not hold it — §6.7's answer for a symbol is a drawn `Path`,
//! and Rust has no `Path`. So the marker is [`RESTARTS`], one ASCII character, and
//! [`Group::Provenance`] carries the legend that explains it. That is the same substitution
//! `machine.rs` makes for §7.3's captions: the design's sentence in the vocabulary the window can
//! actually render.

use crate::emu::{Out, Stats, WATCHED};
use crate::machine::{Freshness, Life, Pace};

/// §12.8's marker for a counter that **starts at zero after a restore**.
///
/// `·` is what the design writes and `geometry::GLYPHS` will not have it — see this module's
/// header. One character, because it sits inside a 232 px label column beside a word.
pub const RESTARTS: char = '*';

/// §12.8's *"`—`, never `0`"*, in one place so a Gauge with nothing to say cannot be given a zero
/// by a caller who forgot.
pub const UNMEASURED: &str = "—";

/// §12.8's seven groups, in its order, **all present always**.
///
/// A group with nothing in it still draws its heading, which is the difference between *this
/// machine reported nothing here* and *this page forgot about it*.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Machine,
    /// **One column, and §17 Q10 is why.**
    ///
    /// The question was *can an arrival at a `WATCHED` PC be attributed to a core?* The answer is
    /// **no**, and it is a stronger no than *the array has no second dimension*: `Stats::enters` is
    /// filled from `Machine::enter_log`, and the only code that pushes to that log is inside
    /// `Machine::run` — the **CPU's** loop. `Machine::run_cop` is a reduced loop whose own doc says
    /// *"the instruments do not see the COP: a `--calls` ring, a `--profile` or a `--storelog`
    /// describes the CPU alone"*, and it carries no arrival capture at all. So every arrival ever
    /// recorded is a CPU arrival by construction, and a second column could hold only a literal
    /// zero — which §12.8's own Gauge rule forbids in the group whose caption claims the most.
    ///
    /// The previous revision drew `core 0  core 1` with a `0` in every core-1 cell and captioned it
    /// *"this is where what the two cores are doing lives"*.
    Cores,
    Panel,
    Input,
    Bus,
    Memory,
    Provenance,
}

impl Group {
    /// Every group, in §12.8's order. The length is written into the type, so an eighth stops the
    /// crate compiling until somebody decides where it goes.
    pub const ALL: [Group; 7] = [
        Group::Machine,
        Group::Cores,
        Group::Panel,
        Group::Input,
        Group::Bus,
        Group::Memory,
        Group::Provenance,
    ];

    /// The heading, before freshness is appended to it.
    pub fn heading(self) -> &'static str {
        match self {
            Group::Machine => "MACHINE",
            Group::Cores => "CORES",
            Group::Panel => "PANEL",
            Group::Input => "INPUT",
            Group::Bus => "BUS",
            Group::Memory => "MEMORY",
            Group::Provenance => "PROVENANCE",
        }
    }

    /// Its index in [`Group::ALL`], which is the number the markup carries.
    pub fn as_i32(self) -> i32 {
        Group::ALL
            .iter()
            .position(|g| *g == self)
            .expect("ALL holds every variant") as i32
    }

    /// §12.8's *"the group heading gains ` · stale`"* / ` · final`, in the em dash this program can
    /// draw. `Live` and `Unmeasured` add nothing: a heading that said *live* on every group every
    /// half second would be the page shouting its own refresh rate.
    pub fn line(self, fresh: Freshness) -> String {
        match fresh {
            Freshness::Stale => format!("{} — stale", self.heading()),
            Freshness::Final => format!("{} — final", self.heading()),
            Freshness::Live | Freshness::Unmeasured => self.heading().to_string(),
        }
    }
}

/// **One measured number, for a person** — §5's Gauge primitive, as a value rather than a drawing.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Gauge {
    pub group: Group,
    /// Leading. Carries [`RESTARTS`] when this counter starts again after a restore.
    pub label: String,
    /// Trailing, tabular mono. [`UNMEASURED`] when there is nothing — **never `0`**.
    pub value: String,
    /// §12.8's three-state freshness, plus `Final`. A property of the value, not of the drawing.
    pub fresh: Freshness,
    /// §12.8 draws exactly one Gauge `warn`: `steps dropped` above zero, because a refused step is
    /// a lie about what you did. `stalled` joins it at `machine::STALL_SECS`, which is §12.2's
    /// fifth thing.
    pub warn: bool,
}

impl Gauge {
    fn new(group: Group, label: &str, value: String, fresh: Freshness) -> Gauge {
        Gauge { group, label: label.into(), value, fresh, warn: false }
    }

    /// A number with nothing measured behind it. §12.8: *"a zero and an unmeasured are different
    /// facts and this repository has been burned by conflating them."*
    fn nothing(group: Group, label: &str) -> Gauge {
        Gauge::new(group, label, UNMEASURED.into(), Freshness::Unmeasured)
    }
}

/// **Digits a person can read across.** `1 612 004 992`, grouped in threes with an ordinary space.
///
/// §12.8's own diagram uses a thin space, which `geometry::GLYPHS` does not hold; an ASCII space
/// groups the same digits and is a character this program's font is trusted for. Not
/// `eapp_loader::si`, which is the same arithmetic against a different noun — these rows are counts
/// and a `1.6 G` beside a label reading `instructions` loses the four digits somebody is comparing.
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Seconds, to one decimal. `21.5 s`.
fn secs(v: f64) -> String {
    format!("{v:.1} s")
}

fn yes_no(v: bool) -> String {
    if v { "yes".into() } else { "no".into() }
}

/// **The whole page, as values** — §12.8's seven groups, in its order, every one present.
///
/// `sampled_ms_ago` is how long ago the caller last read `Out`; `None` is *never sampled*, which is
/// what a window that has not opened the page yet answers. It decides freshness for every Gauge in
/// one place — [`Freshness::of`] — so a page cannot end up with two ideas of how old it is.
pub fn read(out: &Out, life: &Life, sampled_ms_ago: Option<u64>) -> Vec<Gauge> {
    let fresh = Freshness::of(life, sampled_ms_ago);
    let s = &out.stats;
    let mut g: Vec<Gauge> = Vec::new();

    machine_group(&mut g, out, life, s, fresh);
    cores_group(&mut g, s, fresh);
    panel_group(&mut g, out, fresh);
    input_group(&mut g, s, fresh);
    bus_group(&mut g, s, fresh);
    memory_group(&mut g, out, fresh);
    provenance_group(&mut g, s, fresh);
    g
}

fn machine_group(g: &mut Vec<Gauge>, out: &Out, life: &Life, s: &Stats, fresh: Freshness) {
    let m = Group::Machine;
    // The phase in one word, and it is `Life`'s rather than a second reading of `Out.phase`.
    g.push(Gauge::new(m, "phase", phase_word(life).into(), fresh));
    g.push(Gauge::new(m, "instructions", grouped(s.executed), fresh));
    // ── The two `*` rows, and **they are drawn only when they say something the row above does
    // not** ────────────────────────────────────────────────────────────────────────────────────
    //
    // `executed_here` and `sim_usec_here` are *this process's* halves of the two counters above
    // them, and a restore is the only thing that can make either differ — a cold boot starts both
    // at zero, so on every cold-booted machine these rows print the number one line above
    // themselves, twice, in a group of eight.
    //
    // **Found by looking at `readout.png`**, which is the only way this kind of thing is ever
    // found: `simulated` and `simulated *` came out as two rows reading `0.0 s`, four pixels apart,
    // and no assertion could have seen it because each producer was right on its own. It is the
    // same defect `bench-running.png` found on the cradle one pass ago, in a new place.
    //
    // The PROVENANCE paragraph is the legend for their absence and already says it: *"this machine
    // was cold-booted, so every counter on this page starts where the machine did."*
    if s.executed != s.executed_here {
        g.push(Gauge::new(
            m,
            &format!("this session {RESTARTS}"),
            grouped(s.executed_here),
            fresh,
        ));
    }
    g.push(Gauge::new(m, "simulated", secs(f64::from(s.sim_usec) / 1e6), fresh));
    // **§12.8's decision about `Stats::sim_usec_here`, acted on.** *"It earns a row — it is the
    // honest simulated-versus-wall ratio now that idle costs what running costs."* The **ratio**
    // does not, for want of a stated divisor, so what is drawn is the number itself beside the wall
    // clock it would be divided by, and the reader does the division that this file will not.
    if u64::from(s.sim_usec) != s.sim_usec_here {
        g.push(Gauge::new(
            m,
            &format!("simulated {RESTARTS}"),
            secs(s.sim_usec_here as f64 / 1e6),
            fresh,
        ));
    }
    g.push(Gauge::new(m, "wall", secs(s.wall_secs), fresh));
    // `Pace::caption` is `None` while no wall time has passed, which is an unmeasured speed and not
    // a speed of zero — the rule this whole page is written around.
    match Pace::of(s).caption() {
        Some(c) => g.push(Gauge::new(m, "speed", c, fresh)),
        None => g.push(Gauge::nothing(m, "speed")),
    }
    // §12.2's fifth thing, which is not a phase. `Life::stalled` answers only past the threshold and
    // only while `Running`: a machine that is off has not moved either, and reporting that as a
    // stall would be the instrument shouting about the one state where nothing moving is correct.
    match life.stalled() {
        Some(secs_stalled) => g.push(Gauge {
            warn: true,
            ..Gauge::new(m, "stalled", secs(f64::from(secs_stalled)), fresh)
        }),
        None if matches!(life, Life::Running { .. }) => {
            g.push(Gauge::new(m, "stalled", secs(f64::from(out.stalled_secs)), fresh))
        }
        None => g.push(Gauge::nothing(m, "stalled")),
    }
}

fn cores_group(g: &mut Vec<Gauge>, s: &Stats, fresh: Freshness) {
    // **One column.** See [`Group::Cores`] — the only loop that records an arrival is the CPU's.
    for (i, (_, name)) in WATCHED.iter().enumerate() {
        g.push(Gauge::new(Group::Cores, name, grouped(s.enters[i]), fresh));
    }
}

fn panel_group(g: &mut Vec<Gauge>, out: &Out, fresh: Freshness) {
    let p = Group::Panel;
    let other = if out.fb_addr == crate::emu::FB_FRONT {
        crate::emu::FB_BACK
    } else {
        crate::emu::FB_FRONT
    };
    g.push(Gauge::new(p, "shown surface", format!("{:#010x}", out.fb_addr), fresh));
    g.push(Gauge::new(p, "shown moved", yes_no(out.fb_shown_moved), fresh));
    // §12.8: *"a restored machine can be one page-flip out of phase, and this is the only place
    // that says so."*
    g.push(Gauge::new(p, "other surface", format!("{other:#010x}"), fresh));
    g.push(Gauge::new(p, "other moved", yes_no(out.fb_other_moved), fresh));
    g.push(Gauge::new(p, "lit pixels", grouped(u64::from(out.fb_nonzero)), fresh));
    g.push(Gauge::new(p, "backlight", format!("{} / 32", out.backlight), fresh));
    // §12.8: *"a level that is not moving and a pin that is not pulsing are different diagnoses."*
    let (up, down) = out.backlight_steps;
    g.push(Gauge::new(p, "steps up / down", format!("{up} / {down}"), fresh));
}

fn input_group(g: &mut Vec<Gauge>, s: &Stats, fresh: Freshness) {
    let i = Group::Input;
    g.push(Gauge::new(i, "wheel position", format!("{} / 96", s.position), fresh));
    g.push(Gauge::new(i, "touched", yes_no(s.touched), fresh));
    g.push(Gauge::new(
        i,
        "buttons",
        if s.buttons == 0 { UNMEASURED.into() } else { format!("{:#06b}", s.buttons) },
        fresh,
    ));
    g.push(Gauge::new(i, "hold switch", if s.hold { "on".into() } else { "off".into() }, fresh));
    // Whether the firmware has *asked*, which is a different question from whether the stream is on
    // — and the one that means *this machine has finished starting and wants input*.
    g.push(Gauge::new(i, "frames asked for", yes_no(s.asked_for_frames), fresh));
    g.push(Gauge::new(i, "reporting", yes_no(s.reporting), fresh));
    g.push(Gauge::new(i, &format!("frames posted {RESTARTS}"), grouped(s.frames_posted), fresh));
    g.push(Gauge::new(
        i,
        &format!("dropped / suppressed {RESTARTS}"),
        format!("{} / {}", s.frames_dropped, s.frames_suppressed),
        fresh,
    ));
    // **The one Gauge §12.8 draws `warn`.** A refused step is a lie about what you did.
    g.push(Gauge {
        warn: s.input_dropped > 0,
        ..Gauge::new(i, "steps dropped", grouped(s.input_dropped), fresh)
    });
}

fn bus_group(g: &mut Vec<Gauge>, s: &Stats, fresh: Freshness) {
    let b = Group::Bus;
    // **§12.8's own sketch draws `ata commands` and `ready` as the same number, and that is the
    // tell.** Both rows were `Stats::data_reads` and `Stats::data_reads_ready` — the CLICK WHEEL's
    // DATA register, which `--selftest` prints as *"DATA reads N (M with a frame waiting)"* — so
    // the row that claimed to count commands issued to the drive was counting serial reads off the
    // wheel, and a machine whose drive never answered would still have shown a four-figure number
    // here. research/04 row 9's A/B is 102 ATA commands against 24 for the same boot with the IDE
    // interrupt latch ablated, and 24 is Apple's bootloader painting its own screen and never
    // handing RetailOS the disk: the difference between a boot and a bootloader is exactly this
    // number, and it was not on the page.
    g.push(Gauge::new(b, "ata commands", grouped(s.ata_commands), fresh));
    g.push(Gauge::new(b, "wheel data reads", grouped(s.data_reads), fresh));
    g.push(Gauge::new(b, "of those ready", grouped(s.data_reads_ready), fresh));
    g.push(Gauge::new(b, "wheel irqs", grouped(s.irqs), fresh));
    // Both counters live on `Bcm` and `Machine::restore` builds a fresh one, so after a restore
    // they start at zero even though the surface they filled is right there on the panel.
    g.push(Gauge::new(b, &format!("co-proc frames {RESTARTS}"), grouped(s.bcm_frames), fresh));
    g.push(Gauge::new(
        b,
        &format!("co-proc commands {RESTARTS}"),
        grouped(s.bcm_commands as u64),
        fresh,
    ));
}

fn memory_group(g: &mut Vec<Gauge>, out: &Out, fresh: Freshness) {
    let m = Group::Memory;
    g.push(Gauge::new(
        m,
        "unmapped pages",
        grouped(out.unmapped_pages.len() as u64),
        fresh,
    ));
    // §12.8: *"the addresses, not a count: the question it settles wants them."* The question is
    // *is the DRM failing because hardware is missing*, and a count cannot answer it.
    //
    // **One address per Gauge rather than one row of them all**, which is a measurement rather than
    // a preference: a `Row`'s drawable value column is `ROW_VALUE_W` 140 less its 16 px chevron
    // gutter = 124 px, and three addresses joined by spaces is 32 characters — about 250 px at
    // `Metric.mono-size`. Written as one row it would elide to `0x7000c000 0x7000c…`, which is the
    // half of the answer that is no use.
    for (i, addr) in out.unmapped_pages.iter().take(MEMORY_ROWS).enumerate() {
        g.push(Gauge::new(m, &format!("page {}", i + 1), format!("{addr:#010x}"), fresh));
    }
    match out.unmapped_pages.len().saturating_sub(MEMORY_ROWS) {
        0 => {}
        n => g.push(Gauge::new(m, "and more", format!("+{n}"), fresh)),
    }
}

/// How many unmapped addresses the MEMORY group names before it stops naming them.
///
/// Eight, because the group is a list of *distinct* things the firmware asked for and nothing
/// answered — a boot that touched fifty of those is not a page-by-page problem any more, and fifty
/// rows would push the six groups under it out of reach on a page that already scrolls.
const MEMORY_ROWS: usize = 8;

fn provenance_group(g: &mut Vec<Gauge>, s: &Stats, fresh: Freshness) {
    let p = Group::Provenance;
    // **Inferred from two counters rather than published as a flag, and the inference is exact.**
    // `executed` is everything this machine has ever done and `executed_here` is what it has done
    // in this process; a restore is the only thing that makes them differ, because a cold boot
    // starts both at zero. §12.8 asks for *"restored from a snapshot at N instructions"* and this
    // is N.
    let carried = s.executed.saturating_sub(s.executed_here);
    g.push(Gauge::new(
        p,
        "restored at",
        match carried {
            0 => "nothing — a cold boot".into(),
            n => format!("{} instr", grouped(n)),
        },
        fresh,
    ));
}

/// §12.8's PROVENANCE paragraph — **the label that stops a healthy restored machine reading as
/// "RetailOS has never drawn"**.
///
/// A paragraph rather than a Gauge, and the reason is the same measurement the MEMORY group's rows
/// were re-cut for: a `Row`'s value column draws about 124 px and this sentence is three lines. It
/// is also not a *number*, and §5's Gauge is *"one measured number"* — a sentence in that slot
/// would be the page's own vocabulary breaking on the group whose job is to explain the rest.
pub fn provenance(s: &Stats) -> String {
    let carried = s.executed.saturating_sub(s.executed_here);
    let restored = match carried {
        0 => "This machine was cold-booted, so every counter on this page starts where the machine               did."
            .to_string(),
        n => format!(
            "Restored from a snapshot at {} instructions. The picture on the panel is real.",
            grouped(n)
        ),
    };
    format!("{restored} {RESTARTS} marks a counter that starts at zero after a restore.")
}

/// §12.2's phase column, in one word.
fn phase_word(life: &Life) -> &'static str {
    match life {
        Life::Off => "off",
        Life::Booting { .. } => "booting",
        Life::Running { .. } => "running",
        Life::Stopped { .. } => "stopped",
    }
}

/// **`Copy this readout` — the whole page as text**, which is what a bug report actually needs.
///
/// Group headings carry their freshness exactly as they do on screen, so a pasted readout says
/// whether the machine was still running when it was taken. A reader who cannot tell that is
/// reading numbers with no time on them.
pub fn as_text(gauges: &[Gauge], fresh: Freshness) -> String {
    let mut out = String::new();
    for group in Group::ALL {
        let mine: Vec<&Gauge> = gauges.iter().filter(|g| g.group == group).collect();
        out.push_str(&group.line(fresh));
        out.push('\n');
        for g in mine {
            out.push_str(&format!("  {:<28} {}\n", g.label, g.value));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emu::Phase;
    use crate::machine::STALL_SECS;

    fn out(phase: Phase, stats: Stats) -> Out {
        Out {
            phase,
            fb: Vec::new(),
            fb_nonzero: 0,
            fb_addr: crate::emu::FB_FRONT,
            fb_seq: 0,
            backlight: 16,
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
            stats,
        }
    }

    fn running(stats: Stats) -> (Out, Life) {
        let o = out(Phase::Running, stats);
        let l = Life::read(&o, &crate::emu::BootTarget::Os, None);
        (o, l)
    }

    /// **Every one of §12.8's seven groups is on the page, always** — and the page is not a list
    /// somebody has to remember to extend.
    #[test]
    fn all_seven_groups_are_present_whatever_the_machine_is_doing() {
        for phase in [
            Phase::Off,
            Phase::Booting { target: crate::emu::SNAP_AT },
            Phase::Running,
            Phase::Stopped("Lost(0xe19b0000) at 128000 instructions".into()),
        ] {
            let o = out(phase.clone(), Stats::default());
            let l = Life::read(&o, &crate::emu::BootTarget::Os, None);
            let g = read(&o, &l, Some(10));
            for group in Group::ALL {
                assert!(
                    g.iter().any(|x| x.group == group),
                    "{:?} has no rows in {phase:?}, and §12.8 draws all seven always",
                    group
                );
            }
        }
    }

    /// **A zero and an unmeasured are different facts, and this page is where the difference is
    /// drawn.**
    ///
    /// The rule §12.8 states and this repository has been burned by: *"not measured — `—`, never
    /// `0`."* A machine with no wall time behind it has an unmeasured speed; a machine that has
    /// dropped no steps has dropped **zero** of them, which is a measurement.
    #[test]
    fn an_unmeasured_number_draws_a_dash_and_a_measured_zero_draws_a_zero() {
        let (o, l) = running(Stats { wall_secs: 0.0, ..Stats::default() });
        let g = read(&o, &l, Some(10));
        let find = |label: &str| {
            g.iter()
                .find(|x| x.label == label)
                .unwrap_or_else(|| panic!("no `{label}` row"))
        };
        assert_eq!(find("speed").value, UNMEASURED, "a machine four ms old has no speed yet");
        assert_eq!(find("speed").fresh, Freshness::Unmeasured);
        assert_eq!(
            find("steps dropped").value,
            "0",
            "nothing dropped is a measurement, and drawing it as `—` loses it"
        );
        assert!(!find("steps dropped").warn);

        // …and above zero it warns, which is the one Gauge §12.8 gives that colour to.
        let (o, l) = running(Stats { input_dropped: 12, wall_secs: 1.0, ..Stats::default() });
        let g = read(&o, &l, Some(10));
        let dropped = g.iter().find(|x| x.label == "steps dropped").unwrap();
        assert_eq!(dropped.value, "12");
        assert!(dropped.warn, "a refused step is a lie about what you did and it says nothing");
    }

    /// **§17 Q10, answered by the run loop and drawn as one column.**
    ///
    /// `Stats::enters` is `[u64; WATCHED.len()]` — five numbers, one per watched address, and no
    /// per-core dimension anywhere. The reason it has none is not an omission in `Stats`: the only
    /// code that pushes an arrival is inside `Machine::run`, the CPU's loop, and `Machine::run_cop`
    /// carries no arrival capture at all. So a second column could hold only an invented zero.
    ///
    /// **The control is the shape this replaces**: five labels against ten cells.
    #[test]
    fn the_cores_group_is_one_column_because_that_is_all_the_machine_can_attribute() {
        let (o, l) = running(Stats { enters: [1_041, 12_880, 3_204, 218, 3_201], ..Stats::default() });
        let g = read(&o, &l, Some(10));
        let cores: Vec<&Gauge> = g.iter().filter(|x| x.group == Group::Cores).collect();
        assert_eq!(
            cores.len(),
            WATCHED.len(),
            "the CORES group is one cell per watched address and no more; a second column is a \
             number nothing measured"
        );
        for (i, (_, name)) in WATCHED.iter().enumerate() {
            assert_eq!(cores[i].label, *name);
        }
        assert_eq!(cores[1].value, "12 880", "the digits are not grouped for a reader");
        // No label anywhere on the page claims a core, which is what the previous revision's
        // `core 0  core 1` heading did over a column of literal zeroes.
        assert!(
            !g.iter().any(|x| x.label.contains("core 0") || x.label.contains("core 1")),
            "a row claims to be about one of the two cores"
        );
    }

    /// **No row on this page prints the number one line above it.**
    ///
    /// The `*` rows are *this process's* half of the two counters above them, and on a cold boot
    /// those are one fact — so drawing both is one number twice, four pixels apart, in the group
    /// that claims the most. Found in `_out/gui/readout.png`, which is the only way this kind of
    /// thing is ever found: no assertion could see it, because each producer was right on its own.
    ///
    /// **Red in one line**: push the two `*` rows unconditionally and the cold-boot half fails.
    #[test]
    fn no_gauge_repeats_the_number_on_the_row_above_it() {
        // A cold boot: both halves of both counters are the same number.
        let cold = Stats {
            executed: 412_000_000,
            executed_here: 412_000_000,
            sim_usec: 5_493_333,
            sim_usec_here: 5_493_333,
            wall_secs: 4.0,
            ..Stats::default()
        };
        let (o, l) = running(cold);
        let g = read(&o, &l, Some(10));
        let machine: Vec<&Gauge> = g.iter().filter(|x| x.group == Group::Machine).collect();
        for pair in machine.windows(2) {
            assert_ne!(
                pair[0].value, pair[1].value,
                "`{}` and `{}` are the same number one line apart",
                pair[0].label, pair[1].label
            );
        }
        assert!(
            !machine.iter().any(|x| x.label.contains(RESTARTS)),
            "a cold-booted machine drew a `this session` row, which is the row above it again"
        );

        // …and a restored one, where they are two facts and both are worth a row.
        let warm = Stats { executed: 1_612_000_000, executed_here: 12_000_000, ..cold };
        let (o, l) = running(warm);
        let g = read(&o, &l, Some(10));
        assert!(
            g.iter().any(|x| x.label == format!("this session {RESTARTS}")),
            "a restored machine hides the one counter that says what it did in this process"
        );
    }

    /// **A restore is inferred from two counters, and a cold boot says so rather than reporting a
    /// zero somebody has to interpret.**
    #[test]
    fn provenance_names_what_a_restore_carried_in_and_a_cold_boot_says_it_was_one() {
        let cold = Stats { executed: 400_000_000, executed_here: 400_000_000, ..Stats::default() };
        let (o, l) = running(cold);
        let g = read(&o, &l, Some(10));
        let row = g.iter().find(|x| x.label == "restored at").unwrap();
        assert!(row.value.contains("cold boot"), "{:?}", row.value);
        assert!(provenance(&cold).contains("cold-booted"), "{}", provenance(&cold));

        let restored = Stats { executed: 1_612_000_000, executed_here: 12_000_000, ..Stats::default() };
        let (o, l) = running(restored);
        let g = read(&o, &l, Some(10));
        let row = g.iter().find(|x| x.label == "restored at").unwrap();
        assert_eq!(row.value, "1 600 000 000 instr", "{:?}", row.value);
        let said = provenance(&restored);
        assert!(said.contains("1 600 000 000 instructions"), "{said}");
        // The legend travels with it, because the marker is on labels in five other groups and a
        // page that used a symbol nobody explained would be worse than one that used none.
        assert!(said.contains(RESTARTS), "{said}");
    }

    /// **The heading says how old the page is, and `stale` and `final` are different words.**
    ///
    /// §12.8: *"stale means we stopped looking, final means the machine ended there."* A two-state
    /// `bool fresh` is what makes them the same, which is why `machine::Freshness` has four.
    #[test]
    fn a_group_heading_tells_final_from_stale_and_says_nothing_when_it_is_live() {
        assert_eq!(Group::Machine.line(Freshness::Live), "MACHINE");
        assert_eq!(Group::Machine.line(Freshness::Stale), "MACHINE — stale");
        assert_eq!(Group::Machine.line(Freshness::Final), "MACHINE — final");
        // Unmeasured is the page before anything has ever sampled it. It is not stale — nothing
        // went out of date, because nothing was ever read.
        assert_eq!(Group::Machine.line(Freshness::Unmeasured), "MACHINE");

        // …and the freshness a page is built with is the one `Freshness::of` decides, from the
        // phase and the sample age together.
        let (o, l) = running(Stats { wall_secs: 1.0, ..Stats::default() });
        assert_eq!(read(&o, &l, None)[0].fresh, Freshness::Unmeasured);
        assert_eq!(read(&o, &l, Some(10))[0].fresh, Freshness::Live);
        assert_eq!(read(&o, &l, Some(10_000))[0].fresh, Freshness::Stale);
        let stopped = out(Phase::Stopped("Lost(0xe19b0000)".into()), Stats::default());
        let dead = Life::read(&stopped, &crate::emu::BootTarget::Os, None);
        assert_eq!(read(&stopped, &dead, Some(10))[0].fresh, Freshness::Final);
    }

    /// **A stall is reported only of a machine that is supposed to be moving**, and it warns.
    #[test]
    fn a_stalled_machine_warns_and_an_off_one_reports_nothing_to_stall() {
        let mut o = out(Phase::Running, Stats { wall_secs: 10.0, ..Stats::default() });
        o.stalled_secs = STALL_SECS + 4.4;
        let l = Life::read(&o, &crate::emu::BootTarget::Os, None);
        let g = read(&o, &l, Some(10));
        let stalled = g.iter().find(|x| x.label == "stalled").unwrap();
        assert!(stalled.warn, "a machine dead for 6.4 s is drawn as though nothing were wrong");
        assert_eq!(stalled.value, "6.4 s");

        // A machine that is off has not moved either, and saying so would be the instrument
        // shouting about the one state where nothing moving is correct.
        let off = out(Phase::Off, Stats::default());
        let g = read(&off, &Life::Off, Some(10));
        let stalled = g.iter().find(|x| x.label == "stalled").unwrap();
        assert_eq!(stalled.value, UNMEASURED);
        assert!(!stalled.warn);
    }

    /// **The BUS group's `ata commands` row counts the DRIVE, and it used to count the wheel.**
    ///
    /// It was `Stats::data_reads` — the CLICK WHEEL's DATA register, which `--selftest` prints as
    /// *"DATA reads N (M with a frame waiting)"* — drawn under a label about storage, with
    /// `data_reads_ready` beside it as `ready`. §12.8's own sketch shows the two as **611 and 611**,
    /// which is the shape of the defect visible in the design: two rows claiming different things
    /// and carrying one number. A machine whose drive had never answered a single command would
    /// have shown a healthy four-figure count there, and research/04 row 9's whole A/B — 102 ATA
    /// commands for a boot against 24 for a bootloader that never gets the disk — was unreadable
    /// from this window.
    ///
    /// The fixture is a machine on which the two are not the same number, which is what any real
    /// one is: 706 commands to the drive by idle, against a wheel nobody has touched.
    #[test]
    fn the_ata_row_counts_the_drive_rather_than_the_click_wheel() {
        let (o, l) = running(Stats {
            ata_commands: 706,
            data_reads: 43,
            data_reads_ready: 41,
            irqs: 183_452,
            ..Stats::default()
        });
        let g = read(&o, &l, Some(10));
        let row = |label: &str| {
            g.iter()
                .find(|x| x.label == label)
                .unwrap_or_else(|| panic!("no `{label}` row on the page"))
        };
        assert_eq!(
            row("ata commands").value,
            grouped(706),
            "the row that names the drive is not the drive's census"
        );
        assert_eq!(row("wheel data reads").value, grouped(43));
        assert_eq!(row("of those ready").value, grouped(41));
        assert_eq!(row("wheel irqs").value, grouped(183_452));
        assert!(
            !g.iter().any(|x| x.label == "ready"),
            "`ready` on its own says nothing about what is ready, and it sat under a label about \
             the drive while carrying the wheel's number"
        );
    }

    /// **`Copy this readout` is the page, and it carries the freshness with it.**
    ///
    /// A pasted readout with no time on it is a column of numbers somebody has to guess the age of,
    /// which is the whole failure §12.8's three-state Gauge exists to prevent — one screen over.
    #[test]
    fn the_copied_readout_is_the_whole_page_and_says_how_old_it_is() {
        let (o, l) = running(Stats { executed: 1_612_004_992, wall_secs: 34.8, ..Stats::default() });
        let text = as_text(&read(&o, &l, Some(10_000)), Freshness::Stale);
        for group in Group::ALL {
            assert!(text.contains(group.heading()), "{:?} is not in the copy", group);
        }
        assert!(text.contains("MACHINE — stale"), "the copy does not say how old it is:\n{text}");
        assert!(text.contains("1 612 004 992"), "{text}");
        assert!(
            text.contains(RESTARTS),
            "no row carries the marker the legend explains:\n{text}"
        );
    }

    /// Digits grouped in threes, and the boundaries are the ones a reader checks.
    #[test]
    fn a_long_count_is_grouped_in_threes_from_the_right() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1 000");
        assert_eq!(grouped(1_612_004_992), "1 612 004 992");
        assert_eq!(grouped(u64::MAX), "18 446 744 073 709 551 615");
    }

    /// **No toolkit type appears on any code line in this file** — the rule `machine`, `args`,
    /// `rail` and `work` are all held to, so the model is testable with no display stack.
    #[test]
    fn nothing_in_the_readout_module_names_a_toolkit_type() {
        // **Cut at the test module, exactly as `work.rs` does.** The banned words have to be
        // written down somewhere to be searched for, and a sweep that read its own list found
        // itself — which is a test that can only fail. What ships is everything above `mod tests`.
        let src = include_str!("readout.rs");
        let end = src
            .lines()
            .position(|l| l.trim() == "mod tests {")
            .expect("this file has a test module");
        for banned in ["slint::", "SharedString", "ModelRc", "VecModel", "MainWindow"] {
            for (n, line) in src.lines().take(end).enumerate() {
                assert!(
                    !line.contains(banned),
                    "readout.rs:{}: `{banned}` — this file has to be testable with no display",
                    n + 1
                );
            }
        }
        assert!(end > 300, "the cut took the module as well as the tests");
    }
}
