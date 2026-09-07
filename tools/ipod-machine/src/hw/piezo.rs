//! The PWM channel at `0x7000a000` — the click.
//!
//! **The block is a PWM controller, and "piezo" is what the board wires it to.**
//! [research/05](../../../research/05-the-chip-inventory.md) §3 lists this address as *"piezo
//! `0x7000A000`"*, which is what it does on an iPod and not what it is. Rockbox's `pp5020.h` names
//! it `PWM0_CTRL`, and the driver that writes it is `piezo.c` (no claim is made here about what
//! else in that tree might; only those two files were read). The distinction is not pedantry — it
//! is why there is exactly **one**
//! register here rather than a block of them, and why nothing in the window has ever been observed
//! to answer a read.
//!
//! ```text
//! +0x00  PWM0_CTRL   bit 31 enable · bits 30..0 the wave — Apple always sets bit 23
//! ```
//!
//! ## Two independent drivers write the same two words, which is why this is a model
//!
//! **Apple, read out of `OSOS_correct.bin`.** `dis --wordref=0x7000a000` finds the address in four
//! literal pools and no more. Two of them are the whole of the driver:
//!
//! ```text
//! 0011c750  ldr  r1, =0x7000a000     ; --- stop ---
//! 0011c754  mov  r0, #0x0
//! 0011c758  str  r0, [r1, #0x0]      ; PWM0_CTRL = 0
//! 0011c75c  ldr  r0, =0x60005000
//! 0011c760  ldr  r1, [r0, #0x8]
//! 0011c764  bic  r1, r1, #0x80000000
//! 0011c768  str  r1, [r0, #0x8]      ; ...and TIMER2_CFG loses its enable
//!
//! 000c7204  ldr  r1, =0x7000a000     ; --- start ---
//! 000c7208  orr  r0, r6, #0x80000000 ; the queued wave...
//! 000c720c  orr  r0, r0, #0x800000   ; ...with bit 23, always
//! 000c7210  str  r0, [r1, #0x0]
//! ```
//!
//! **Rockbox, from the other side.** `firmware/target/arm/ipod/piezo.c` is two lines against this
//! register — `PWM0_CTRL = 0x80000000 | form_and_period` to start and `PWM0_CTRL = 0` to stop —
//! and `firmware/export/pp5020.h` puts `PWM0_CTRL` at `0x7000a000` and `TIMER2_CFG`/`TIMER2_VAL`
//! at `0x60005008`/`0x6000500c`. Apple pairs the register with the same timer, at the same two
//! offsets, in the same function. Neither source was consulted for code; both were read for
//! register semantics, which are facts about silicon.
//!
//! So bit 31 is the enable and a write of plain zero is the stop, from two drivers that share no
//! authorship. **Everything below bit 31 is called "the wave" here and is not decoded**, because
//! neither source explains it: Rockbox passes an opaque `form_and_period` computed from a
//! frequency, and Apple reads it out of a queue. [`Piezo::waves`] is uncapped so the next run does
//! not have to infer the vocabulary from a top-N list.
//!
//! ## What the register is attached to, and why that is not modelled here
//!
//! `AsyncPiezo` — task 35 in [research/10](../../../research/10-the-resource-image.md)'s table,
//! entry `0x00285060`, blocked on semaphore `0x95` — is a **sequencer over a 16-entry ring**, not a
//! beeper. Its initialisation (`0x00285060`..`0x002850c4`) disables `TIMER2_CFG`, enables interrupt
//! `0x15` through `0x60004024`, clears bits 3..2 of `0x70000010`, and sets **bit 17 of
//! `0x6000600c`** — the device-enable gate. Its loop pends on the semaphore, takes a message,
//! writes `msg->wave` and `msg->duration` into rings at `0x10882314` and `0x10882354`, and calls
//! the step at `0x000c719c`. The step stops the register, writes the next wave to it, and programs
//! that entry's duration into `TIMER2_CFG`. One tone is therefore **two writes here** — a zero and
//! a start — and the duration lives in the timer, never in this register.
//!
//! Bit 17 is worth recording because Rockbox disagrees: its `pp5020.h` defines `DEV_PIEZO` as
//! `0x00010000`, **bit 16**, and gives `DEV_OPTO` the identical value. Apple sets bit 17. One of
//! the two labels is wrong and this model does not need to know which — it gates nothing on the
//! device-enable bit, so a wrong guess here cannot silently suppress a click.
//!
//! The timer, the semaphore and the ring are all outside this file. This models one register.
//!
//! ## It observes and never owns, and that is deliberate
//!
//! **This device answers nothing and absorbs nothing.** Every access falls through to the ordinary
//! `mmio-7` backing store that served this address before the model existed; the model only counts
//! what goes past. So **attaching it cannot change what the firmware sees** — not approximately,
//! but by construction, because the code that produces the value is the same code as before.
//!
//! That is a deliberate departure from [`crate::ClickWheel`], which *does* absorb writes to its
//! `DATA` register. The wheel has to: it answers reads from its own state, so a store that reached
//! the region as well would leave two sources disagreeing. This part answers no reads, so it has no
//! such state to protect — and taking the weaker power buys two things. A run recorded before this
//! device existed stays comparable. And **`Machine::snapshot` keeps working unchanged**: the
//! snapshot format serialises regions and not peripherals, so a device that swallowed the register
//! would restore it as zero, and only the fact that nothing reads it back would hide that.
//!
//! [`Piezo::reg`] is the model's own mirror of the word, kept because detecting an *edge* on bit 31
//! needs the previous value. It is not a second source of truth: the region holds the same bytes.
//! Nothing in this repository has ever observed a read of the register — [`Piezo::reads`] exists to
//! find out if that stops being true.
//!
//! ## The neighbourhood is counted rather than assumed empty
//!
//! Naming this window in `page_is_plain` takes the whole 4 KB page off the fast path, so observing
//! the rest of it is free. [`Piezo::neighbours`] is that census. It is here because "the block is
//! one register" is a claim from two reverse-engineered sources and not from a datasheet, and the
//! cheapest way to be wrong about it is to look only where you expect an answer.

use crate::Capped;
use std::collections::BTreeMap;

/// One write that started the wave: when, from where, and what.
///
/// The times are both clocks, because they answer different questions. `icount` is the coordinate
/// every measurement in `research/` uses; `usec` is the one a person means, and the one anything
/// downstream that wants to *render* a click has to have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fire {
    pub icount: u64,
    pub usec: u32,
    /// The instruction that stored it. `0x000c7210` is Apple's sequencer, `0x00264d5c` its
    /// synchronous beep; anything else is a driver this file has not read.
    pub pc: u32,
    /// The whole register, enable bit included.
    pub word: u32,
}

/// **A tone that finished**: the wave it ran, and how long the enable bit was set.
///
/// The *duration* is not in this register — [`Piezo`]'s own note says so: `AsyncPiezo` programs it
/// into `TIMER2_CFG` and this block is only turned on and off. So the length is **measured**, as
/// the simulated microseconds between the write that set bit 31 and the write that cleared it,
/// rather than decoded from anything. research/05 predicts what that should come to on RetailOS —
/// *"a tone runs about 26 000 instructions — 5.2 ms of simulated time, against the 3 ms that
/// `0x001B91FC` programs into `TIMER2_CFG`; the remainder is the task's wake-up"* — so this is a
/// number that can disagree with the static reading and say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tone {
    /// The register with the enable bit masked off, as it was while it ran.
    pub wave: u32,
    /// How long bit 31 stayed set, in simulated microseconds.
    pub usec: u32,
}

/// `PWM0_CTRL`, and a census of the page it sits in.
pub struct Piezo {
    /// Base of the window — `0x7000a000`.
    pub base: u32,
    /// The model's mirror of the register — exactly what was last written. **Not** what answers a
    /// read; the backing region does that, as it always did. Kept because an edge on bit 31 needs
    /// the previous value.
    pub reg: u32,
    /// Byte writes that landed on the register, so a `str` counts four. [`Self::fires`] is the
    /// number of *tones*; this is the number of stores, and the two differing is how a driver that
    /// writes bytes instead of words would show up.
    pub byte_writes: u64,
    /// Word stores completed — counted on byte 3, so one `str` counts once.
    pub writes: u64,
    /// **The click.** Writes that took the enable bit from clear to set.
    pub fires: u64,
    /// Writes that cleared the enable bit.
    pub stops: u64,
    /// Writes that changed the wave while the enable bit was *already* set. Apple's sequencer stops
    /// before every start, so this is expected to stay at zero on RetailOS; a driver that retunes
    /// on the fly would show up here rather than being counted as a second click.
    pub retriggers: u64,
    /// Reads of the register. Expected to stay zero — see the module note.
    pub reads: u64,
    /// `wave -> times started`, **uncapped**. The wave is the register with the enable bit masked
    /// off. Uncapped because the question this answers is *what does a click sound like*, and a
    /// capped tally answers it with a floor.
    pub waves: BTreeMap<u32, u64>,
    /// Every fire in order — a **sample**. [`Self::fires`] is the census.
    pub log: Capped<Fire>,
    /// The storing instruction for each fire, uncapped: `pc -> count`. Which driver clicked is a
    /// different question from how often, and the log's cap must not be able to lose it.
    pub sites: BTreeMap<u32, u64>,
    /// `offset -> (reads, writes)` for everything in the page that is **not** the register. The
    /// falsification hook for "the block is one register wide".
    pub neighbours: BTreeMap<u32, (u64, u64)>,
    /// **The last tone that finished.** `None` until one has, which is the honest answer for a run
    /// that has clicked once and not yet stopped — and the state anything rendering a click has to
    /// have a policy for rather than a default hidden in a zero.
    pub tone: Option<Tone>,
    /// The tone that is running: the wave, and the microsecond bit 31 was set. Private because it
    /// is the intermediate of one measurement and reading it as a fact about the part would be
    /// reading a stopwatch that is still going.
    running: Option<(u32, u32)>,
}

impl Piezo {
    /// Rockbox `pp5020.h`: `PWM0_CTRL`. research/05 §3 reaches the same address from the
    /// PortalPlayer502x map.
    pub const BASE: u32 = 0x7000_a000;
    /// What this device owns: the one register. Everything else in the page falls through to
    /// ordinary backing memory and is merely counted.
    pub const WINDOW: u32 = 4;
    /// What it *watches*. `page_is_plain` works a page at a time, so the census costs nothing.
    pub const PAGE: u32 = 0x1000;
    /// Bit 31 — the enable, agreed on by both drivers.
    pub const ENABLE: u32 = 0x8000_0000;
    /// Bit 23, which Apple sets on every start and neither source explains.
    pub const FORM: u32 = 0x0080_0000;

    pub fn new(base: u32) -> Self {
        Piezo {
            base,
            reg: 0,
            byte_writes: 0,
            writes: 0,
            fires: 0,
            stops: 0,
            retriggers: 0,
            reads: 0,
            waves: BTreeMap::new(),
            log: Capped::new(4096),
            sites: BTreeMap::new(),
            neighbours: BTreeMap::new(),
            tone: None,
            running: None,
        }
    }

    /// Whether the wave is running right now.
    pub fn on(&self) -> bool {
        self.reg & Self::ENABLE != 0
    }

    /// The register with the enable bit masked off.
    pub fn wave(&self) -> u32 {
        self.reg & !Self::ENABLE
    }

    /// Note a byte read. **Answers nothing** — the caller goes on to the backing region, which is
    /// where the value came from before this model existed and still does.
    pub(crate) fn observe_read8(&mut self, off: u32) {
        if off >= Self::WINDOW {
            if off < Self::PAGE {
                self.neighbours.entry(off & !3).or_default().0 += 1;
            }
            return;
        }
        // Counted on byte 3 for the same reason writes are: one `ldr` should count once.
        if off & 3 == 3 {
            self.reads += 1;
        }
    }

    /// Note a byte write. **Consumes nothing** — the store goes on to the backing region.
    ///
    /// **The transition is judged on byte 3, not on every byte.** A word store arrives here as four
    /// byte writes, low byte first, so the register is only complete on the last of them; judging
    /// each byte would report a start and a stop for one `str` whose top byte crosses `0x80`. This
    /// is the same convention [`crate::ClickWheel::read8`] uses on `DATA`, and for the same reason.
    pub(crate) fn write8(&mut self, off: u32, val: u8, pc: u32, icount: u64, usec: u32) {
        if off >= Self::WINDOW {
            if off < Self::PAGE {
                self.neighbours.entry(off & !3).or_default().1 += 1;
            }
            return;
        }
        self.byte_writes += 1;
        let was_on = self.on();
        let mut b = self.reg.to_le_bytes();
        b[(off & 3) as usize] = val;
        self.reg = u32::from_le_bytes(b);
        if off & 3 != 3 {
            return;
        }
        self.writes += 1;
        let now_on = self.on();
        match (was_on, now_on) {
            (false, true) => {
                self.fires += 1;
                *self.waves.entry(self.wave()).or_insert(0) += 1;
                *self.sites.entry(pc).or_insert(0) += 1;
                self.running = Some((self.wave(), usec));
                self.log.push(Fire {
                    icount,
                    usec,
                    pc,
                    word: self.reg,
                });
            }
            (true, false) => {
                self.stops += 1;
                // **`wrapping_sub`, because `Memory::usec` is a `u32` and wraps every ~71 minutes
                // of simulated time.** A tone straddling the wrap is milliseconds long either way,
                // so the wrapped difference is the right answer rather than a guarded one; what a
                // subtraction would give is a 71-minute click.
                if let Some((wave, at)) = self.running.take() {
                    self.tone = Some(Tone { wave, usec: usec.wrapping_sub(at) });
                }
            }
            // A wave changed under a running enable bit. Not a click by this model's definition,
            // and counted separately rather than folded into one or the other. The clock keeps
            // running: the enable bit never fell, so this is one tone whose pitch moved, and
            // restarting it here would report the tail as the whole.
            (true, true) => {
                self.retriggers += 1;
                let wave = self.wave();
                if let Some(r) = self.running.as_mut() {
                    r.0 = wave;
                }
            }
            (false, false) => {}
        }
    }

    /// The run report. Empty when the register was never touched, so a caller can print it
    /// unconditionally and a run with no clicks says nothing.
    pub fn report(&self) -> Vec<String> {
        let touched = self.byte_writes + self.reads + self.neighbours.len() as u64;
        if touched == 0 {
            return Vec::new();
        }
        let mut out = vec![format!(
            "piezo (PWM0_CTRL {:#010x}): {} clicks — {} word writes, {} stops, {} retriggers, \
             {} reads; now {}",
            self.base,
            self.fires,
            self.writes,
            self.stops,
            self.retriggers,
            self.reads,
            if self.on() { "ON" } else { "off" },
        )];
        if let Some(t) = self.tone {
            out.push(format!(
                "  last tone: wave {:#010x} for {} us ({:.2} ms) — MEASURED off the enable bit, \
                 not decoded; the duration lives in TIMER2_CFG",
                t.wave,
                t.usec,
                t.usec as f64 / 1000.0,
            ));
        }
        if self.reads > 0 {
            out.push(
                "  READS — nothing has ever been observed to read this register; the write-only \
                 premise needs re-checking"
                    .into(),
            );
        }
        if !self.waves.is_empty() {
            let mut rows: Vec<_> = self.waves.iter().collect();
            rows.sort_by_key(|(w, n)| (std::cmp::Reverse(*n), **w));
            out.push(format!(
                "  waves started, commonest first ({} distinct, all shown):",
                rows.len()
            ));
            for (w, n) in rows {
                out.push(format!(
                    "    wave {w:#010x} x{n}{}",
                    if w & Self::FORM != 0 {
                        "  bit 23 set"
                    } else {
                        "  bit 23 CLEAR — Apple always sets it"
                    }
                ));
            }
        }
        for (pc, n) in &self.sites {
            out.push(format!(
                "    started from {pc:#010x} x{n}{}",
                match *pc {
                    0x000c_7210 => "  (AsyncPiezo's sequencer step)",
                    0x0026_4d5c => "  (the synchronous beep at 0x00264d30)",
                    _ => "",
                }
            ));
        }
        // The timeline, in the clock a person means. This is the half of the report that anything
        // wanting to *render* a click has to have; a histogram cannot be replayed.
        let shown = 32.min(self.log.sample().len());
        if shown > 0 {
            out.push(format!("  in order: {} fires", self.log.seen()));
            for f in self.log.iter().take(shown) {
                out.push(format!(
                    "    @{:>10}  {:>9.3} s  wave {:#010x}  from {:#010x}",
                    f.icount,
                    f.usec as f64 / 1e6,
                    f.word & !Self::ENABLE,
                    f.pc
                ));
            }
            if self.log.seen() > shown as u64 {
                out.push(format!(
                    "    (+{} further fires not shown — SAMPLE)",
                    self.log.seen() - shown as u64
                ));
            }
        }
        // Zero rows here is the expected answer and is stated rather than left as an absence.
        if self.neighbours.is_empty() {
            out.push("  nothing else in the page was touched — the block is one register".into());
        } else {
            out.push(format!(
                "  {} OTHER word(s) in the page were touched — the block is wider than one \
                 register:",
                self.neighbours.len()
            ));
            for (off, (r, w)) in &self.neighbours {
                out.push(format!(
                    "    {:#010x}  {r} reads  {w} writes",
                    self.base.wrapping_add(*off)
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One word store, as the CPU delivers it once the page is off the fast path: four byte writes,
    /// low byte first.
    fn store(p: &mut Piezo, off: u32, val: u32, pc: u32, icount: u64, usec: u32) {
        for (i, b) in val.to_le_bytes().iter().enumerate() {
            p.write8(off + i as u32, *b, pc, icount, usec);
        }
    }

    fn piezo() -> Piezo {
        Piezo::new(Piezo::BASE)
    }

    /// Apple's own two words, in the order its sequencer writes them.
    ///
    /// `0x000c7200` stops before every start, so one queued tone is a zero followed by
    /// `0x80800000 | wave`. This is the sequence the model has to read as **one** click.
    #[test]
    fn apples_stop_then_start_is_one_click() {
        let mut p = piezo();
        store(&mut p, 0, 0, 0x0011_c758, 100, 1_000_000);
        store(&mut p, 0, 0x8080_012c, 0x000c_7210, 200, 1_000_100);
        assert_eq!(p.fires, 1, "one tone, one click");
        assert_eq!(p.writes, 2, "two word stores");
        // The leading zero was already off, so it is not a stop either — nothing to stop.
        assert_eq!(p.stops, 0);
        assert!(p.on());
        assert_eq!(p.wave(), 0x0080_012c);
        assert_eq!(p.waves[&0x0080_012c], 1);
        assert_eq!(p.sites[&0x000c_7210], 1);
        // And the ending zero closes it.
        store(&mut p, 0, 0, 0x0011_c758, 300, 1_002_000);
        assert_eq!(p.stops, 1);
        assert!(!p.on());
        assert_eq!(p.fires, 1, "a stop is not a second click");
    }

    /// The control for the test above: prove the byte-3 rule is load-bearing.
    ///
    /// A word store's top byte carries the enable bit, and it arrives **last**. A model that judged
    /// the transition on every byte would see `0x8080012c` as a start on byte 3 and also count the
    /// three bytes below it, and — worse — would read the *stop* word `0x00000000` written over a
    /// running `0x8080012c` as a stop on byte 3 having already reported nothing on bytes 0..2. This
    /// asserts the count is per-word, which is the property that makes `fires` a number of clicks.
    #[test]
    fn a_word_store_counts_once_not_four_times() {
        let mut p = piezo();
        store(&mut p, 0, 0x8080_012c, 0x000c_7210, 1, 0);
        assert_eq!(p.byte_writes, 4, "the CPU delivered four bytes");
        assert_eq!(p.writes, 1, "and they are one register write");
        assert_eq!(p.fires, 1);
        assert_eq!(p.log.seen(), 1, "one row in the timeline, not four");
    }

    /// The low byte of a running register may move without that being a new click.
    #[test]
    fn changing_the_wave_while_running_is_a_retrigger_not_a_click() {
        let mut p = piezo();
        store(&mut p, 0, 0x8080_012c, 0x000c_7210, 1, 0);
        store(&mut p, 0, 0x8080_0258, 0x000c_7210, 2, 10);
        assert_eq!(p.fires, 1);
        assert_eq!(p.retriggers, 1);
        assert_eq!(p.stops, 0);
        // The second wave is not credited as a start, so the tally stays a tally of clicks.
        assert_eq!(p.waves.len(), 1);
        assert!(p.report().iter().any(|l| l.contains("1 retriggers")));
    }

    /// Rockbox's two lines produce the same two transitions as Apple's, which is the whole reason
    /// this decode is believed rather than merely self-consistent.
    ///
    /// `piezo_hw_tick` is `PWM0_CTRL = 0x80000000 | form_and_period` and `piezo_hw_stop` is
    /// `PWM0_CTRL = 0`. Neither sets bit 23 by construction — it is inside `form_and_period` if it
    /// is there at all — so this also covers the case the report calls out.
    #[test]
    fn rockboxs_two_lines_read_as_a_click_and_a_stop() {
        let mut p = piezo();
        store(&mut p, 0, 0x8000_0000 | 0x0000_0100, 0x4000_0000, 1, 0);
        assert_eq!(p.fires, 1);
        assert_eq!(p.wave(), 0x0000_0100);
        store(&mut p, 0, 0, 0x4000_0000, 2, 500);
        assert_eq!(p.stops, 1);
        assert!(!p.on());
        // Bit 23 clear is called out rather than silently normalised to Apple's shape.
        assert!(p
            .report()
            .iter()
            .any(|l| l.contains("bit 23 CLEAR — Apple always sets it")));
    }

    /// A read is counted and answers nothing.
    ///
    /// This is the property the whole "it observes and never owns" claim rests on, and it is
    /// asserted by the *signature*: [`Piezo::observe_read8`] returns `()`, so there is no value it
    /// could substitute for the region's. What is left to check is that it still counts.
    #[test]
    fn a_read_is_counted_and_answered_by_nothing() {
        let mut p = piezo();
        store(&mut p, 0, 0x8080_012c, 0x000c_7210, 1, 0);
        for i in 0..4 {
            p.observe_read8(i);
        }
        assert_eq!(p.reads, 1, "one word read, counted once");
        assert_eq!(p.reg, 0x8080_012c, "and reading did not disturb the mirror");
        assert!(p
            .report()
            .iter()
            .any(|l| l.contains("write-only premise needs re-checking")));
    }

    /// Everything past the register is ignored except for the tally, because "the block is one
    /// register" is a claim from two reverse-engineered sources and not from a datasheet.
    #[test]
    fn the_rest_of_the_page_is_counted_and_not_owned() {
        let mut p = piezo();
        p.write8(0x10, 0xff, 0, 1, 0);
        p.observe_read8(0x20);
        assert_eq!(p.writes, 0, "and it is not a register write");
        assert_eq!(p.reg, 0, "a neighbour must not reach the register");
        assert_eq!(p.neighbours[&0x10], (0, 1));
        assert_eq!(p.neighbours[&0x20], (1, 0));
        let text = p.report().join("\n");
        assert!(text.contains("the block is wider than one register"));
        assert!(text.contains("0x7000a010"));
        // A page nobody else touched says so out loud, rather than leaving an absence.
        let mut q = piezo();
        store(&mut q, 0, 0x8080_012c, 0x000c_7210, 1, 0);
        assert!(q
            .report()
            .iter()
            .any(|l| l.contains("the block is one register")));
    }

    /// A run that never touched the register prints nothing at all.
    #[test]
    fn a_silent_run_reports_nothing() {
        assert!(piezo().report().is_empty());
    }

    /// **How long the tone ran, measured off the enable bit rather than decoded.**
    ///
    /// The duration is not in this register — `AsyncPiezo` programs it into `TIMER2_CFG` — so the
    /// only honest source for it is the interval between the write that set bit 31 and the write
    /// that cleared it. research/05's RetailOS run puts that at 5.2 ms of simulated time against
    /// the 3 ms programmed, which is the shape asserted here.
    ///
    /// **It carries its own control**, and that is the half that matters: while the tone is still
    /// running the answer is `None` and not a zero. A renderer handed `0` would draw a click of no
    /// length and could not tell that from a click nobody has measured yet, which is `AGENTS.md`
    /// §6's shape — an instrument that cannot report *not yet* reports the wrong thing instead.
    ///
    /// **How to make it go red:** move the `self.tone = ...` line into the `(false, true)` arm, so
    /// a tone is dated when it starts. The `None` assertion fails first.
    #[test]
    fn a_tone_is_measured_from_the_enable_bit_and_is_none_until_it_stops() {
        let mut p = piezo();
        store(&mut p, 0, 0, 0x0011_c758, 100, 1_000_000);
        assert_eq!(p.tone, None, "a stop before any start dated a tone that never ran");
        store(&mut p, 0, 0x8080_0055, 0x000c_7210, 200, 1_000_100);
        assert_eq!(
            p.tone, None,
            "the tone was dated while it was still running — a stopwatch read before it stopped"
        );
        store(&mut p, 0, 0, 0x0011_c758, 300, 1_005_300);
        assert_eq!(p.tone, Some(Tone { wave: 0x0080_0055, usec: 5_200 }));
        assert!(p.report().iter().any(|l| l.contains("5200 us (5.20 ms)")));
    }

    /// `Memory::usec` is a `u32` and wraps every ~71 minutes of simulated time. A tone is
    /// milliseconds long on either side of that, so the wrapped difference is the answer.
    ///
    /// **How to make it go red:** use `usec - at` instead of `wrapping_sub`. In release it reports
    /// 4 294 964 096 us — a 71-minute click; in debug it panics.
    #[test]
    fn a_tone_across_the_clocks_wrap_is_milliseconds_not_seventy_one_minutes() {
        let mut p = piezo();
        store(&mut p, 0, 0x8080_0055, 0x000c_7210, 1, u32::MAX - 999);
        store(&mut p, 0, 0, 0x0011_c758, 2, 2_200);
        assert_eq!(p.tone.expect("a tone").usec, 3_200);
    }

    /// A wave changing under a running enable bit is one tone whose pitch moved, so the clock is
    /// not restarted — restarting it would report the tail as the whole tone. The *pitch* recorded
    /// is the one it ended on, which is the only one this model can claim to know it stopped at.
    ///
    /// Apple stops before every start, so `retriggers` is expected to stay zero on RetailOS; this
    /// is what the model does if that stops being true.
    ///
    /// **How to make it go red:** set `self.running = Some((self.wave(), usec))` in the
    /// `(true, true)` arm — the length becomes 400 us, which is the last leg rather than the tone.
    #[test]
    fn a_retrigger_moves_the_pitch_without_restarting_the_clock() {
        let mut p = piezo();
        store(&mut p, 0, 0x8080_0055, 0x000c_7210, 1, 1_000);
        store(&mut p, 0, 0x8080_005b, 0x000c_7210, 2, 1_600);
        store(&mut p, 0, 0, 0x0011_c758, 3, 2_000);
        assert_eq!(p.tone, Some(Tone { wave: 0x0080_005b, usec: 1_000 }));
    }
}

