//! The click wheel: contact, rotation, the five buttons and the streaming frame.

use crate::*;

/// The click wheel, at the level the SoC presents it — four registers in the `0x7000c000` block.
///
/// **Software never sees the Cypress part.** The wheel hangs off a PSoC that talks to the SoC's
/// `opto` transceiver, and firmware drives the transceiver. So what is modelled here is that
/// transceiver and the packet format it hands over; the PSoC is not modelled and does not need to
/// be ([research/05](../../../research/05-the-chip-inventory.md) §"Click wheel").
///
/// ```text
/// +0x100  CTRL     bit 31 transmit start · bits 30..29 receiver/interrupt arm
/// +0x104  STATUS   bit 31 transmit busy · bit 26 receive ready (write 1 to clear) · bit 27 likewise
/// +0x120  TX       the command word a transmit sends
/// +0x140  DATA     the received packet
/// ```
///
/// **Two independent sources agree on every bit above**, which is why this is a model rather than a
/// hypothesis. Rockbox's `button-clickwheel.c` gives `CLICKWHEEL_DATA` at `0x7000c140`, the
/// `0x7000c100`/`0x7000c104` init pair, the `0x60000000`/`0x0c000000` re-arm, the `0x800000ff ==
/// 0x8000001a` frame check, 96 clicks per rotation and the bit-30 touch flag. **Apple's own driver,
/// read out of `OSOS_correct.bin`, says the same thing from the other side** — and it is the
/// stronger source, because it is the code this emulator actually runs:
///
/// ```text
/// 00281358  ldr r0,[r1,#0x104]      ; r1 = 0x7000c000
/// 0028135c  tst r0, #0x4000000      ; receive ready?
/// 00281360  beq 0x002813e0          ;   no -> re-arm and return
/// 00281364  ldr r0,[r1,#0x140]      ; the packet
/// 00281370  and r12, r0, #0xbc0000ff
/// 00281374  cmp r12, #0x8000001a    ; the streaming frame
/// 00281380  mov r12, r0, lsl #18    ; buttons = bits 13..8
/// 00281384  tst r0, #0x40000000     ; touched?
/// 00281388  mov r12, r12, lsr #26
/// 0028139c  and r0, r12, r0, lsr #16 ; position = bits 25..16, masked
/// 002813b8  ldr lr, =0x8000023a     ; else the queried button frame
/// 002813bc  bic r12, r0, #0x7f000000
/// 002813c0  bic r12, r12, #0xff0000  ; r0 & 0x8000ffff
/// 002813e4  orr r0, r0, #0x4000000   ; acknowledge: write 1 to bit 26
/// 002813f0  orr r0, r0, #0x60000000  ; re-arm the receiver
/// ```
///
/// Apple's mask is `0xbc0000ff` where Rockbox's is `0x800000ff` — a *stricter* test of the same
/// frame (it additionally requires bits 29..26 clear), satisfied by exactly the packets Rockbox
/// accepts. Neither source was consulted for code; both were read for register semantics, which are
/// facts about silicon and not anyone's expression of them.
///
/// **Two packet shapes, and RetailOS decodes both.** The streaming frame `0x8000001a` carries the
/// wheel: buttons in bits 13..8, absolute position in bits 22..16 over 96 clicks, bit 30 set while a
/// finger is on the wheel. The queried frame `0x8000023a` is the *reply to a command*: RetailOS
/// writes `0x8000023a` to TX at `0x00283fc8`, starts a transmit, waits for receive-ready and reads
/// the buttons back in bits 20..16. Bit 31 is clear only when Hold is engaged.
///
/// **Three commands, and only one of them is a question.** The low 16 bits of a transmitted word are
/// the opcode; bits 30..16 are its payload; bit 31 is framing.
///
/// - `0x023a` — *read the buttons.* The one command with a reply. `0x00283ea0` sends it, polls
///   receive-ready and reads the answer back with the buttons in bits 20..16.
/// - `0x052a` — *set reporting on or off*, payload byte at bits 23..16. **A write, not a question**,
///   and [`ClickWheel::transmit`] answers it with silence for reasons derived from Apple's own code
///   rather than assumed — see there.
///
/// **What is not modelled**, said plainly: the transmit is instantaneous, so STATUS bit 31 (busy) is
/// never observably set — the same convention every other device here uses. Any opcode that is
/// neither of the two above is counted and listed rather than given an invented reply. Nothing here
/// knows the wheel's *physical* geometry — position is whatever the injected script says it is.
pub struct ClickWheel {
    /// Base of the block the registers are offsets from — `0x7000c000`, shared with I²C.
    pub base: u32,
    /// Hold engaged. Clears frame bit 31 *and* drives GPIOA's active-low hold line, which is the
    /// bit `button_hold()` actually reads; the frame bit alone would be a half-modelled switch.
    pub hold: bool,
    /// A finger is on the wheel — frame bit 30.
    pub touched: bool,
    /// Absolute position, 0..95. Rockbox: "Highest wheel = 0x5F, clockwise increases."
    pub position: u8,
    /// Buttons held, in the streaming frame's bit order relative to bit 8: select, right, left,
    /// play, menu. The queried frame puts the same five bits at 16.
    pub buttons: u8,
    pub ctrl: u32,
    pub status: u32,
    pub tx: u32,
    pub rx: u32,
    /// A reply that has been composed and is not back from the wheel yet: the frame, and the value
    /// of `usec` at which it lands. See [`OPTO_REPLY_USEC`] — the delay is load-bearing.
    pub reply: Option<(u32, u32)>,
    /// The scripted sequence, fired strictly in the order written — see [`WheelStep`].
    pub script: Vec<WheelStep>,
    /// How far through the script this run is.
    pub next: usize,
    /// Whether a posted frame may raise IRQ 40. `--wheel-no-irq` clears it, which is the ablation
    /// that separates "the firmware read a frame" from "the firmware was interrupted".
    pub irq_enabled: bool,
    pub frames_posted: u64,
    /// Frames overwritten before the firmware had read the previous one — a real overrun, and the
    /// only way an injected sequence that outruns the driver is distinguishable from one it consumed.
    pub frames_dropped: u64,
    /// Word reads of DATA, and how many of those found a frame waiting.
    pub data_reads: u64,
    pub data_reads_ready: u64,
    /// How many of `data_reads` came from the **coprocessor** rather than the CPU.
    ///
    /// Written because two cores read every frame the wheel posts and the menu still does not
    /// move, and "the firmware consumed the event" does not say *which* firmware did. If the COP
    /// is taking the frames the CPU's UI is waiting for, that is a different defect from the UI
    /// ignoring them, and no existing counter separates the two.
    pub data_reads_cop: u64,
    /// Transmits started, and the commands we had no evidence for.
    pub commands: u64,
    pub unknown_commands: u64,
    /// The distinct unknown command words, capped. `unknown_commands` is the uncapped count of
    /// occurrences; this is the set, and it can be truncated — so the report says when it was.
    pub unknown: Capped<u32>,
    /// Autonomous reporting, as the firmware last set it with opcode `0x052a`.
    ///
    /// **On at reset**, corrected 2026-08-18. It defaulted to off, so a driver that never sent
    /// `0x052a` was handed silence for ever — and Rockbox is exactly that driver. Its
    /// `opto_i2c_init` writes `0xc00a1f00` to `CTRL` and nothing else (`button-clickwheel.c:97`;
    /// the extra `0x7000c104` poke beside it is `#if IPOD_4G || IPOD_COLOR` and does not compile
    /// for the Video), then its ISR expects the same `0x1a`-tagged autonomous frames RetailOS
    /// does. It works on hardware, so the part cannot require the command in order to stream.
    /// Measured before the correction: Rockbox reached its menu with **0 frames posted and 0 reads
    /// of `CLICKWHEEL_DATA`**.
    ///
    /// The command is still real and still does what the old model said it did — `0x000b2ce0`
    /// picks between `0x8001052a` and `0x8000052a`, so RetailOS can turn the stream *off* — it
    /// simply is not what turns it on. See
    /// [`ClickWheel::transmit`]. **Starts off**, because a wheel nobody has spoken to has not been
    /// told to report, and because that is the one thing about this command that is falsifiable:
    /// events injected before the firmware's own enable are suppressed instead of being silently
    /// consumed by a driver that is not listening yet.
    pub reporting: bool,
    /// `0x052a` commands seen, and the payload of the last one.
    pub set_commands: u64,
    pub last_set: Option<(u64, u8)>,
    /// Autonomous frames not posted because the firmware had switched reporting **off**. The only
    /// number that can distinguish "the script did nothing" from "the script was refused".
    pub frames_suppressed: u64,
    /// Autonomous frames not posted because the receiver was **not armed** — `CTRL`'s bit 30 clear.
    ///
    /// **Separate from [`Self::frames_suppressed`] because a run reported the wrong cause.** Both
    /// halves of the gate fed one counter and the report described it as "suppressed while off",
    /// so a machine with reporting ON and `CTRL` at zero printed `reporting ON` and
    /// `12 frames suppressed while off` on consecutive lines and pointed at the half that was
    /// innocent. Two counters cannot do that.
    pub frames_unarmed: u64,
    /// Times the line went from clear to asserted.
    pub irqs: u64,
    /// Times the firmware **acknowledged** a packet — a write that cleared `RX_READY`.
    ///
    /// **Reading `DATA` does not clear it; only this write does.** So this, not `data_reads`, is
    /// the number that says whether the firmware is still in the loop: the line stays asserted
    /// until the acknowledgement lands, and while it stays asserted no later frame can raise it,
    /// so every frame after an un-acknowledged one is dropped. A run whose posts keep climbing
    /// while this stops is a firmware that stopped acknowledging — which is a different failure
    /// from one that never received anything, and the two used to be indistinguishable here.
    pub acks: u64,
    /// When the last acknowledgement landed, in executed instructions.
    pub last_ack: Option<u64>,
    /// Times the interrupt line was **lowered while a frame was still waiting**.
    ///
    /// The line is level, and the level is `irq_enabled && RX_READY && ARM`. So a frame can sit
    /// unread with the line down, if the receiver was disarmed in between — and then nothing
    /// raises it again, because a later post finds `RX_READY` already set and produces no edge.
    /// The firmware never learns about the frame it was sent.
    ///
    /// **This separates two explanations that look identical from outside.** A run where the
    /// firmware stops reading, and a run where the model withdrew the interrupt underneath it,
    /// both end with frames posted and unread. Measured on RetailOS's language picker: 27 posted,
    /// 6 assertions, 2 seen by Apple's ISR — this says which story the missing four belong to.
    pub line_dropped_waiting: u64,
    /// Every frame posted, capped — the sequence is short by construction and its *order* is the
    /// thing worth reading back. `frames_posted` above is the census; this is the sample.
    pub log: Capped<(u64, u32)>,
}

/// One step of an injected sequence: when it fires, and what it does.
///
/// **Anchored in instructions, not microseconds.** Simulated time in this emulator is dominated by
/// the idle task's sleeps — a 600 M-instruction boot reaches 950 s of `usec` — so a microsecond
/// anchor is not a stable coordinate across runs that idle differently. Every measurement in
/// `research/` is instruction-anchored (`OptoTask` enters `@49678867`), and so is this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WheelStep {
    /// When this step fires — an executed-instruction count, or a moment in simulated
    /// microseconds when [`WheelStep::in_usec`] is set.
    pub at: u64,
    /// Read `at` as simulated microseconds rather than as executed instructions.
    ///
    /// **The two diverge on a machine that idles, and the failure is silent.** Instructions are the
    /// right anchor for a measurement — reproducible, and unmoved by how much the machine slept —
    /// which is why they are the default and why every calibrated recipe here uses them. But a
    /// machine sitting at a menu spends most of its budget halted: measured on Rockbox, a 200 M
    /// budget executed under 90 M, so a script anchored at `@1600M` over a 2 G budget fired
    /// **0 of 20 steps** and read as "Rockbox ignores the wheel". It does not. The press never
    /// happened.
    ///
    /// So driving a user interface wants the unit a person means and the firmware's own timers
    /// use: `@20s`, not `@1500M`.
    pub in_usec: bool,
    pub event: WheelEvent,
}

impl WheelStep {
    /// A step at an executed-instruction count.
    pub fn instr(at: u64, event: WheelEvent) -> WheelStep {
        WheelStep {
            at,
            in_usec: false,
            event,
        }
    }
    /// A step at a moment in simulated microseconds.
    pub fn usec(at: u64, event: WheelEvent) -> WheelStep {
        WheelStep {
            at,
            in_usec: true,
            event,
        }
    }
    /// Whether this step is due, given both clocks.
    pub fn due(&self, icount: u64, usec: u64) -> bool {
        self.at <= if self.in_usec { usec } else { icount }
    }
    /// `@2 000 000 us` or `@150M`, for the schedule a run prints so a log reproduces itself.
    pub fn when(&self) -> String {
        if self.in_usec {
            format!("{} us", self.at)
        } else {
            format!("{}", self.at)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelEvent {
    /// Finger down / finger up. A release posts a frame with bit 30 *clear*, which is how the
    /// driver learns the finger left — a release that posted nothing would look like a stuck touch.
    Touch,
    Release,
    Hold(bool),
    /// One click, clockwise (+1) or anticlockwise (-1).
    Step(i8),
    /// A button going down or coming up, by streaming-frame mask.
    Button(u8, bool),
}

impl ClickWheel {
    pub const CTRL: u32 = 0x100;
    pub const STATUS: u32 = 0x104;
    pub const TX: u32 = 0x120;
    pub const DATA: u32 = 0x140;
    /// The window this device answers for. `0x100..0x144` — everything between the four registers
    /// stays ordinary backing memory, so a register we have not identified is reported by
    /// `--input-regs` rather than swallowed.
    pub const WINDOW: u32 = 0x144;

    /// CTRL bit 31: writing it 0 -> 1 starts a transmit.
    const START: u32 = 0x8000_0000;
    /// CTRL bit 30: the receiver is armed. Both drivers set it — Rockbox's init word `0xc00a1f00`
    /// and its ISR tail `0x400a1f00`, RetailOS's `orr r0, r0, #0x60000000` — and it is the only bit
    /// common to every arming write, so it is what gates the interrupt.
    pub(crate) const ARM: u32 = 0x4000_0000;
    /// STATUS bit 26: a packet is waiting. Write-1-to-clear.
    pub(crate) const RX_READY: u32 = 0x0400_0000;
    /// STATUS bits 27..26 are both write-1-to-clear; the rest of the register is storage, which is
    /// what lets Rockbox's `outl(0x01000000, 0x7000c104)` configuration write survive.
    const W1C: u32 = 0x0c00_0000;

    /// The command RetailOS sends to read the buttons, and the tag its reply carries.
    const QUERY: u32 = 0x8000_023a;
    /// The tag of the autonomous frame that carries the wheel.
    const STREAM: u32 = 0x0000_001a;
    /// Opcode `0x052a` — set autonomous reporting. The payload is the byte at bits 23..16.
    const SET_REPORT: u32 = 0x0000_052a;

    pub fn new(base: u32) -> Self {
        ClickWheel {
            base,
            hold: false,
            touched: false,
            position: 0,
            buttons: 0,
            ctrl: 0,
            status: 0,
            tx: 0,
            rx: 0,
            reply: None,
            script: Vec::new(),
            next: 0,
            irq_enabled: true,
            frames_posted: 0,
            frames_dropped: 0,
            data_reads: 0,
            data_reads_ready: 0,
            data_reads_cop: 0,
            commands: 0,
            unknown_commands: 0,
            unknown: Capped::new(16),
            reporting: true,
            set_commands: 0,
            last_set: None,
            frames_suppressed: 0,
            frames_unarmed: 0,
            irqs: 0,
            acks: 0,
            last_ack: None,
            line_dropped_waiting: 0,
            log: Capped::new(256),
        }
    }

    /// The autonomous frame: what the wheel sends when it has something to report.
    pub fn stream_frame(&self) -> u32 {
        let mut f = Self::STREAM;
        if !self.hold {
            f |= 1 << 31;
        }
        if self.touched {
            f |= 1 << 30;
        }
        f |= (self.buttons as u32 & 0x1f) << 8;
        f |= (self.position as u32 & 0x7f) << 16;
        f
    }

    /// The reply to a `0x8000023a` command: the same five buttons, sixteen bits higher.
    pub fn query_frame(&self) -> u32 {
        let mut f = Self::QUERY;
        if !self.hold {
            f |= 1 << 31;
        }
        f |= (self.buttons as u32 & 0x1f) << 16;
        f
    }

    /// Hand a packet to the receiver.
    pub(crate) fn post(&mut self, frame: u32, icount: u64) {
        if self.status & Self::RX_READY != 0 {
            self.frames_dropped += 1;
        }
        self.rx = frame;
        self.status |= Self::RX_READY;
        self.frames_posted += 1;
        self.log.push((icount, frame));
    }

    /// Run one transmit. Two opcodes are known; anything else is recorded as unanswered rather than
    /// replied to, because a plausible invented reply is exactly the kind of thing that reads as a
    /// working device for a whole session.
    ///
    /// The reply is *composed* here and *delivered* later — see [`OPTO_REPLY_USEC`].
    ///
    /// # `0x052a` is a write, and the silence is derived rather than assumed
    ///
    /// `0x8001052a` went unanswered for two addenda on the grounds that we had no evidence for what
    /// it replies. The evidence was in Apple's own code, and it says the question was wrong: it is a
    /// **setter with a byte payload**, not a query, and the hardware's correct answer is nothing.
    ///
    /// - `0x00283e10` is the whole API: `orr r0, #0x8000052a, r0 lsl #16` then `b 0x00283fa0`. Three
    ///   instructions, a tail branch, no frame — it *cannot* read a reply. Its two callers are
    ///   one-liners `mov r0,#1; b` (`0x000bbdb0`) and `mov r0,#0; b` (`0x000b4638`), and a third
    ///   caller `0x000b2ce0` picks between the two assembled constants `0x8001052a` and
    ///   `0x8000052a`. So the payload is a boolean at bits 23..16 and nothing more.
    /// - The other two senders do not read either. `0x00283e20` (the opto init both Apple stages
    ///   ship) sends it and returns 0. The **boot ROM's** copy at `0x000c9714` in the NOR image
    ///   writes TX, starts the transmit, spins a fixed 10 000-iteration delay and returns — it never
    ///   touches `0x7000c140` at all. Its byte-identical twin at `0x000c9634` differs in exactly one
    ///   word, `0x8000052a`, and the two are called from a power-down and a power-up sequence
    ///   respectively.
    /// - **Nothing in the image could parse such a reply.** There are two frame parsers: the ISR
    ///   decoder `0x00281350` and the polled query `0x00283ea0`. Both accept only
    ///   `(f & 0xbc0000ff) == 0x8000001a` or `(f & 0x8000ffff) == 0x8000023a`. `--wordref=0x0000052a`
    ///   over 7.5 MB is **0**. A `0x052a`-shaped reply would take the decoder's third arm, set the
    ///   bad-frame flag at `[0x1081d998+1]`, and make `SerialOptoTask` run its receiver-reset path at
    ///   `0x00285608` — about seventy times per boot, on shipping firmware. That is the reductio.
    ///
    /// What the payload *means* is second-sourced the same way. `0x00266b18` writes an accessory-mode
    /// byte at `[0x1081de40+1]` and sends payload 1 for mode 0, payload 0 for modes 1–2 — and
    /// `SerialOptoTask` runs the scroll accumulator `0x000dd018` only while that byte is 0. So
    /// payload 1 is exactly the state in which RetailOS bothers to decode wheel positions. The
    /// RetailOS power state machine agrees from the other side (`0x001d8198` sends 1 on the arm with
    /// the 10 s/120 s timers, `0x001d8418` sends 0 on the arm with the 500 ms one), and so does the
    /// ROM pair above.
    fn transmit(&mut self, icount: u64, usec: u32) {
        self.commands += 1;
        // A second command before the first reply is due must not swallow it.
        if let Some((f, _)) = self.reply.take() {
            self.post(f, icount);
        }
        if self.tx & 0x8000_ffff == Self::QUERY {
            let f = self.query_frame();
            self.reply = Some((f, usec.wrapping_add(OPTO_REPLY_USEC)));
        } else if self.tx & 0x0000_ffff == Self::SET_REPORT {
            let payload = ((self.tx >> 16) & 0xff) as u8;
            self.reporting = payload != 0;
            self.set_commands += 1;
            self.last_set = Some((icount, payload));
            // Deliberately no reply. See the derivation above.
        } else {
            self.unknown_commands += 1;
            if !self.unknown.sample().contains(&self.tx) {
                self.unknown.push(self.tx);
            }
        }
    }

    /// Apply one scripted event to the physical state. Returns whether the hold switch moved, which
    /// the caller has to push out to GPIOA — this device cannot reach it.
    pub(crate) fn apply(&mut self, ev: WheelEvent) -> Option<bool> {
        match ev {
            WheelEvent::Touch => self.touched = true,
            WheelEvent::Release => self.touched = false,
            WheelEvent::Hold(on) => {
                self.hold = on;
                return Some(on);
            }
            WheelEvent::Step(d) => {
                let p = self.position as i32 + d as i32;
                self.position = p.rem_euclid(WHEEL_CLICKS_PER_ROTATION as i32) as u8;
            }
            WheelEvent::Button(mask, down) => {
                if down {
                    self.buttons |= mask;
                } else {
                    self.buttons &= !mask;
                }
            }
        }
        None
    }

    /// A byte of one of the four registers, or `None` for everything else in the window — which
    /// then falls through to ordinary memory.
    pub(crate) fn read8(&mut self, off: u32, who: Core) -> Option<u8> {
        let w = match off & !3 {
            Self::CTRL => self.ctrl,
            Self::STATUS => self.status,
            Self::TX => self.tx,
            Self::DATA => {
                // Counted on byte 3 so one `ldr` counts once: a word read arrives here as four
                // byte reads and byte 3 is the last of them. A bare `ldrb` of the low byte would
                // go uncounted, and no driver does that.
                if off & 3 == 3 {
                    self.data_reads += 1;
                    if who == Core::Cop {
                        self.data_reads_cop += 1;
                    }
                    if self.status & Self::RX_READY != 0 {
                        self.data_reads_ready += 1;
                    }
                }
                self.rx
            }
            _ => return None,
        };
        Some(w.to_le_bytes()[(off & 3) as usize])
    }

    /// Take a byte of one of the four registers. Returns whether this device owned the store.
    pub(crate) fn write8(&mut self, off: u32, val: u8, icount: u64, usec: u32) -> bool {
        let b = (off & 3) as usize;
        let put = |reg: u32, val: u8| {
            let mut w = reg.to_le_bytes();
            w[b] = val;
            u32::from_le_bytes(w)
        };
        match off & !3 {
            Self::CTRL => {
                let new = put(self.ctrl, val);
                // On the transition, not on the level: RetailOS clears this bit itself after the
                // busy wait and then writes the register again to re-arm the receiver, so a model
                // that fired on any store with the bit set would transmit twice per command.
                let started = new & Self::START != 0 && self.ctrl & Self::START == 0;
                self.ctrl = new;
                if started {
                    self.transmit(icount, usec);
                }
                true
            }
            Self::STATUS => {
                let mask = (Self::W1C >> (8 * b)) as u8;
                let mut w = self.status.to_le_bytes();
                let before = self.status;
                w[b] = (w[b] & !(val & mask)) | (val & !mask);
                self.status = u32::from_le_bytes(w);
                if before & Self::RX_READY != 0 && self.status & Self::RX_READY == 0 {
                    self.acks += 1;
                    self.last_ack = Some(icount);
                }
                true
            }
            Self::TX => {
                self.tx = put(self.tx, val);
                true
            }
            // The receiver's output register. Absorbed rather than let through, so a stray store
            // cannot leave the backing region disagreeing with what this device answers.
            Self::DATA => true,
            _ => false,
        }
    }
}
