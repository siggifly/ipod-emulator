//! The Wolfson WM8758 — the audio codec, on I²C address `0x1a`.
//!
//! **This part is write-only over this bus, and that is measured rather than assumed.** In
//! `_debug/wheel40/clk5.log` — a captured RetailOS boot on this tree — the I²C census reads
//!
//! ```text
//! i2c CTRL values seen (all 3773 transfers):
//!   ctrl 0x80  write  len 1  x1823      ctrl 0xa0  read   len 1  x1775
//!   ctrl 0x82  write  len 2  x78        ctrl 0xa2  read   len 2  x13
//!   ctrl 0x84  write  len 3  x13        ctrl 0xa4  read   len 3  x35
//!   ctrl 0x86  write  len 4  x2         ctrl 0xa6  read   len 4  x34
//! i2c: 3773 transfers, by device address (uncapped census):
//!   dev 0x10  1864 transfers    dev 0x11  1857 transfers    dev 0x34  52 transfers
//! ```
//!
//! The four read CTRLs sum to 1 857, which is `dev 0x11` exactly — the PCF50605's read address —
//! so **every read on the bus went to the PMU and none went here.** The writes sum to 1 916, which
//! is `dev 0x10` plus `dev 0x34`. So this model answers nothing, and adding it cannot change what
//! the firmware sees. That is deliberate: it is a recorder, and its only output is its report.
//!
//! ## The wire format is from this repository's own disassembly, not from a datasheet
//!
//! `research/17` §"What it is retrying" quotes Apple's own I²C register-write helper at
//! `0x0015c1cc`:
//!
//! ```text
//! 0015c1d4  mov  r0, r1, lsl #1        ; register address << 1
//! 0015c1dc  and  r1, r2, #0x100
//! 0015c1e0  orr  r0, r0, r1, lsr #8    ; …with bit 8 of the value as its LSB
//! 0015c1e4  strb r0, [sp, #0x0]
//! 0015c1e8  strb r2, [sp, #0x1]        ; value bits 7..0
//! ```
//!
//! so a transfer is two bytes carrying a **7-bit register address and a 9-bit value**:
//!
//! ```text
//! byte 0 = (register << 1) | ((value >> 8) & 1)
//! byte 1 =  value & 0xff
//! ```
//!
//! The device address is likewise measured: the bus log prints the 8-bit form, and `0x34 >> 1` is
//! `0x1a` (research/09 §"And it found an unmodelled chip", corrected in research/10 Addendum 13 §2).
//!
//! ## What the firmware actually writes, re-derived rather than quoted
//!
//! `NEXT.md` §5 summarises the baseline as `reg 0x54 ×5 · reg 0x6f ×4 · reg 0x06 ×3 · reg 0x6b ×3`.
//! **Those are first bytes, not register numbers** — the run report labels the tally's third key
//! "reg" because for the PCF50605 the first byte *is* the register pointer, and for this part it is
//! not. Re-derived from the captured logs under `_debug/`, taking the fullest of them
//! (`_debug/syn/hl.log`, which names five codec rows where most name three) and applying the
//! packing above:
//!
//! | first byte | register | value bit 8 | count |
//! |---|---|---|---|
//! | `0x54` | `0x2a` | 0 | 5 |
//! | `0x6f` | `0x37` | 1 | 4 |
//! | `0x06` | `0x03` | 0 | 3 |
//! | `0x6b` | `0x35` | 1 | 3 |
//! | `0x6c` | `0x36` | 0 | 3 |
//!
//! **Fifty captured runs agree on the total and disagree only about how much of it they show.**
//! Every one reports `dev 0x34  52 transfers`; 46 name three of the first bytes, four name `0x6b`
//! as well, and two of those also name `0x6c`. The difference is where each report's top-twelve
//! `(device, register)` cut falls — the cut is global across the bus and the PMU's rows move — not
//! a difference in what the codec was told. The logs do not record the command line that produced
//! them, so nothing is claimed here about which recipe or flags each used.
//!
//! The five rows above are 18 of the 52 transfers. The other 34 are below the cut in every captured
//! run, and **twelve distinct first bytes is likewise a floor and not a census**. Runs whose codec
//! traffic is *not* 52 name seven more — `0x02 0x0d 0x16 0x19 0x56 0x69 0x6d` → registers
//! `0x01 0x06 0x0b 0x0c 0x2b 0x34 0x36` — from the 42-transfer runs under `_out/drm/` and the
//! 55- and 59-transfer ones under `_debug/`. The whole point of [`Wm8758::written`] being uncapped
//! is that the next run does not have to infer the rest from a top-N list.
//!
//! **The decode checks itself, and that is the reason to believe it.** Six distinct registers were
//! written with bit 8 set — `0x06 0x0c 0x34 0x35 0x36 0x37` — and in the published WM897x-family
//! map every one of those six has a defined bit-8 function: `CLKSEL` on the clock-generator
//! register, and the *volume-update* latch on the DAC and output volume registers. The registers
//! written with bit 8 clear are ones with no bit-8 meaning. The left/right pairs also appear as
//! pairs — `0x0b`/`0x0c`, `0x34`/`0x35`, `0x36`/`0x37` — with the latch bit on the right-hand
//! write, which is how a driver sets a stereo volume atomically. Under any other split of the first
//! byte those coincidences vanish: without the shift the "registers" would be `0x54 0x56 0x69 0x6b
//! 0x6c 0x6d 0x6f` — seven of the twelve first bytes named, every one past the end of a 58-register
//! part. [`Wm8758::out_of_map`] counts that failure rather than hiding it, and the tests below
//! assert it in both directions.
//!
//! ## What is *not* established here
//!
//! - **The register names are labels, not measurements.** They come from the published
//!   WM8758/WM897x register map, which this repository does not hold a copy of; nothing in a
//!   captured run confirms any of them. They are here so a report reads as audio rather than as
//!   hex, and [`Wm8758::name`] returns `None` for every number the family map does not define.
//! - **`0x2a` is the busiest register on the boot and its family label does not explain that.**
//!   The map puts "OUT4 to ADC" there, which is not something an initialisation writes five times.
//!   Either the label is wrong for this part or the register does something else on it. Recorded as
//!   an open question; no behaviour is attached to it.
//! - **Power-on register values are unknown**, so the file starts at zero. That is a placeholder
//!   and not a claim — the datasheet defaults are not all zero. It costs nothing today because
//!   nothing reads the file back over the bus (measured above), and it is the first thing to fix if
//!   that ever stops being true.
//! - **No audio comes out of this.** It is the register half of M6. The I²S transport and the DMA
//!   that feeds it are not modelled, and this part does not pretend to be a source of samples.
//! - **Nothing prints [`report`](Wm8758::report) yet.** The device is wired to the bus and counts,
//!   but no binary renders it, so as of this writing the numbers above are the only thing anyone
//!   has read out of it. One line in `trace`'s run report — the doc on `report` has it — is what
//!   turns this from a recorder into an instrument.

use crate::Capped;
use std::collections::BTreeMap;

/// The codec's control interface: a 58-entry register file written two bytes at a time.
pub struct Wm8758 {
    /// `R0..R57`, nine bits each. Zero at power-on — see the module note; this is a placeholder.
    regs: [u16; Self::N_REGS],
    /// `register -> (writes, last value)`, **uncapped**.
    ///
    /// Uncapped for the same reason the PMU's is: the question this instrument exists to answer is
    /// *which register is the firmware driving*, and a capped tally answers it with a floor. The
    /// bus's own `i2c_tally` cannot answer it at all — it keys on the raw first byte, which on this
    /// part is a register number and half a value welded together.
    pub written: BTreeMap<u8, (u64, u16)>,
    /// `(register, value)` in order — a **sample**, so an initialisation sequence can be read as a
    /// sequence. The census is [`writes`](Self::writes).
    pub log: Capped<(u8, u16)>,
    /// Two-byte writes decoded, including any that fell outside the part's map.
    pub writes: u64,
    /// Read transfers addressed at this part.
    ///
    /// **Expected to stay at zero, and worth counting for exactly that reason.** The captured
    /// census has every read on the bus going to the PMU. If this ever moves, the "write-only"
    /// premise this model is built on is wrong, and the number is how that gets found instead of
    /// being absorbed by a bus fill that answers anything.
    pub reads: u64,
    /// Decoded registers past the end of the part's map. A non-zero here means the decode above is
    /// wrong — it is the falsification hook, not an error path.
    pub out_of_map: u64,
    /// `(ctrl, bytes)` of transfers that were not two bytes long. The packing above has no meaning
    /// for any other length, so they are kept verbatim rather than decoded into a guess.
    pub malformed: Capped<(u8, [u8; 4])>,
    /// Writes to `R0`, which the part defines as a software reset.
    pub resets: u64,
}

impl Default for Wm8758 {
    fn default() -> Self {
        Self::new()
    }
}

impl Wm8758 {
    /// 7-bit I²C address. The bus log prints the 8-bit form, `0x34` for a write and `0x35` for a
    /// read, so a dispatcher compares `dev >> 1` against this.
    pub const ADDR: u8 = 0x1a;

    /// `R0..R57` plus `R61`, the bias control register Rockbox writes at start-up. The array spans
    /// `R0..R61`; [`Self::in_map`] says which of those the part has.
    ///
    /// Nothing rests on the exact bound: the highest register any Apple boot touches is `0x37`,
    /// so this only ever functions as the decode's sanity check.
    pub const N_REGS: usize = 0x3e;

    /// `R58..R60` are not registers on the part.
    pub fn in_map(reg: u8) -> bool {
        reg <= 0x39 || reg == 0x3d
    }

    /// `R0` — writing it resets the part.
    pub const RESET: u8 = 0x00;

    pub fn new() -> Self {
        Self {
            regs: [0; Self::N_REGS],
            written: BTreeMap::new(),
            log: Capped::new(512),
            writes: 0,
            reads: 0,
            out_of_map: 0,
            malformed: Capped::new(32),
            resets: 0,
        }
    }

    /// One I²C transfer. `ctrl` is the PP controller's CTRL word — bit `0x20` selects a read, bits
    /// 1..2 carry `len - 1` — the same encoding [`crate::Pcf50605::transfer`] takes.
    ///
    /// A read returns nothing and consumes nothing: the caller must leave the controller's data
    /// registers exactly as it found them. That is what makes attaching this model a no-op for the
    /// guest, and it is why it can be on by default without an A/B.
    pub fn transfer(&mut self, ctrl: u8, d: [u8; 4]) {
        if ctrl & 0x20 != 0 {
            self.reads += 1;
            return;
        }
        let len = (((ctrl >> 1) & 3) as usize + 1).min(4);
        if len != 2 {
            self.malformed.push((ctrl, d));
            return;
        }
        let reg = d[0] >> 1;
        let val = (u16::from(d[0] & 1) << 8) | u16::from(d[1]);
        self.write_reg(reg, val);
    }

    fn write_reg(&mut self, reg: u8, val: u16) {
        self.writes += 1;
        let e = self.written.entry(reg).or_insert((0, val));
        e.0 += 1;
        e.1 = val;
        self.log.push((reg, val));
        // Counted before it is dropped. A register the part does not have is a statement about the
        // decode, and the one thing it must never do is disappear.
        if !Self::in_map(reg) {
            self.out_of_map += 1;
            return;
        }
        self.regs[reg as usize] = val;
        if reg == Self::RESET {
            self.resets += 1;
            self.regs = [0; Self::N_REGS];
        }
    }

    /// The nine bits currently held in `reg`, or `None` for a register the part does not have.
    pub fn reg(&self, reg: u8) -> Option<u16> {
        self.regs.get(reg as usize).copied()
    }

    /// The family map's name for a register, or `None` where it defines none.
    ///
    /// **Labels, not measurements** — see the module note. Nothing in this emulator behaves
    /// differently because of a name; they exist so a report reads as audio.
    /// The codec's master clock pin, Hz. The PP5022 drives it at 24 MHz once the external device
    /// clock is switched, and every PLL setting the firmware uses assumes that.
    pub const DEFAULT_MCLK_HZ: f64 = 24_000_000.0;

    /// The nine rates the iPod's sound layer offers.
    pub const RATES: [u32; 9] = [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000];

    const MCLKDIV: [f64; 8] = [1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0];

    /// `SYSCLK`, the clock the converters run from, or `None` while the codec is not producing
    /// one: it is the I2S slave (`MS` clear), it has been told to use the PLL with `PLLEN` clear,
    /// or the PLL's integer part is outside what the part locks to.
    ///
    /// Registers that were never written read as zero here, not as the part's reset values, so a
    /// guest that selects the PLL without programming `N` gets `None` rather than a default.
    pub fn sysclk_hz(&self, mclk_hz: f64) -> Option<f64> {
        let r = |n: usize| u32::from(self.regs[n]);
        let (r1, r6) = (r(0x01), r(0x06));
        if r6 & 1 == 0 {
            return None;
        }
        let src = if r6 & 0x100 != 0 {
            if r1 & 0x20 == 0 {
                return None;
            }
            let r36 = r(0x24);
            let n = r36 & 0xf;
            if !(5..=13).contains(&n) {
                return None;
            }
            let k = ((r(0x25) & 0x3f) << 18) | ((r(0x26) & 0x1ff) << 9) | (r(0x27) & 0x1ff);
            let prescale = if r36 & 0x10 != 0 { 0.5 } else { 1.0 };
            mclk_hz * prescale * (f64::from(n) + f64::from(k) / 16_777_216.0) / 4.0
        } else {
            mclk_hz
        };
        Some(src / Self::MCLKDIV[((r6 >> 5) & 7) as usize])
    }

    /// The frame rate on LRC: `SYSCLK / 256`, snapped to a standard rate when within 0.5% of one
    /// (the PLL constants Rockbox uses land within 0.01%), otherwise rounded. `None` outside
    /// 7 to 50 kHz, where the part is not specified to run.
    pub fn sample_rate(&self, mclk_hz: f64) -> Option<u32> {
        let fs = self.sysclk_hz(mclk_hz)? / 256.0;
        if !(7_000.0..=50_000.0).contains(&fs) {
            return None;
        }
        Self::RATES
            .iter()
            .copied()
            .find(|&s| (fs - f64::from(s)).abs() <= f64::from(s) * 0.005)
            .or(Some(fs.round() as u32))
    }

    /// `(DACENL, DACENR)` from `R3`.
    pub fn dac_enabled(&self) -> (bool, bool) {
        let r3 = self.regs[0x03];
        (r3 & 1 != 0, r3 & 2 != 0)
    }

    pub fn name(reg: u8) -> Option<&'static str> {
        Some(match reg {
            0x00 => "RESET",
            0x01 => "PWRMGMT1",
            0x02 => "PWRMGMT2",
            0x03 => "PWRMGMT3",
            0x04 => "AINTFCE",
            0x05 => "COMPANDING",
            0x06 => "CLKGEN",
            0x07 => "SRATECTRL",
            0x08 => "GPIOCTL",
            0x09 => "JACKDETECT1",
            0x0a => "DACCTRL",
            0x0b => "LDACVOL",
            0x0c => "RDACVOL",
            0x0d => "JACKDETECT2",
            0x0e => "ADCCTRL",
            0x0f => "LADCVOL",
            0x10 => "RADCVOL",
            0x12 => "EQ1",
            0x13 => "EQ2",
            0x14 => "EQ3",
            0x15 => "EQ4",
            0x16 => "EQ5",
            0x18 => "DACLIMIT1",
            0x19 => "DACLIMIT2",
            0x1b => "NOTCH1",
            0x1c => "NOTCH2",
            0x1d => "NOTCH3",
            0x1e => "NOTCH4",
            0x20 => "ALC1",
            0x21 => "ALC2",
            0x22 => "ALC3",
            0x23 => "NOISEGATE",
            0x24 => "PLLN",
            0x25 => "PLLK1",
            0x26 => "PLLK2",
            0x27 => "PLLK3",
            0x29 => "THREEDCTRL",
            // The family map calls this "OUT4 to ADC" and it is the busiest register on the boot,
            // which the label does not explain. Left unnamed rather than named wrongly.
            0x2b => "BEEPCTRL",
            0x2c => "INCTRL",
            0x2d => "LINPGAGAIN",
            0x2e => "RINPGAGAIN",
            0x2f => "LADCBOOST",
            0x30 => "RADCBOOST",
            0x31 => "OUTCTRL",
            0x32 => "LOUTMIX",
            0x33 => "ROUTMIX",
            0x34 => "LOUT1VOL",
            0x35 => "ROUT1VOL",
            0x36 => "LOUT2VOL",
            0x37 => "ROUT2VOL",
            0x38 => "OUT3MIX",
            0x39 => "OUT4MIX",
            0x3d => "BIASCTRL",
            _ => return None,
        })
    }

    /// Whether bit 8 of this register is the family's *volume-update* latch.
    ///
    /// Load-bearing for reading a log rather than for behaviour: a driver writes the left channel
    /// with the bit clear and the right channel with it set, so the pair lands at once. It is also
    /// the coherence check that says the register decode is right — see the module note.
    pub fn has_volume_update(reg: u8) -> bool {
        matches!(reg, 0x0b | 0x0c | 0x0f | 0x10 | 0x2d | 0x2e | 0x34..=0x37)
    }

    /// The run report. Empty when the part was never addressed, so a caller can print it
    /// unconditionally and a run that has no audio traffic says nothing.
    ///
    /// The counts here are the device's own, not the bus's. `i2c_tally` keys on the raw first byte
    /// and so reports a register number welded to a value bit; this reports registers.
    ///
    /// Lines carry no leading blank; a report that prints a section separator adds its own, the
    /// way `trace`'s other device sections do:
    ///
    /// ```no_run
    /// # let m: ipod_machine::Machine = unimplemented!();
    /// for line in m.mem.wm8758.report() {
    ///     println!("{line}");
    /// }
    /// ```
    pub fn report(&self) -> Vec<String> {
        let total = self.writes + self.reads + self.malformed.seen();
        if total == 0 {
            return Vec::new();
        }
        let mut out = vec![format!(
            "wm8758 (i2c {:#04x}): {total} transfers — {} register writes across {} registers, \
             {} reads, {} not two bytes, {} outside the part's map",
            Self::ADDR,
            self.writes,
            self.written.len(),
            self.reads,
            self.malformed.seen(),
            self.out_of_map,
        )];
        if self.reads > 0 {
            out.push(
                "  READS — this part answers none over two-wire, so the model's write-only \
                 premise needs re-checking"
                    .into(),
            );
        }
        if self.out_of_map > 0 {
            out.push(format!(
                "  {} write(s) decoded outside R0..R57 and R61 — the register decode is wrong",
                self.out_of_map
            ));
        }
        if self.resets > 0 {
            out.push(format!("  software resets: {}", self.resets));
        }
        let mut rows: Vec<_> = self.written.iter().collect();
        rows.sort_by_key(|(reg, (n, _))| (std::cmp::Reverse(*n), **reg));
        out.push(format!(
            "  registers written, busiest first ({} distinct, all shown):",
            rows.len()
        ));
        for (reg, (n, val)) in rows {
            out.push(format!(
                "    reg {reg:#04x} {:<12} x{n:<5} last {val:#05x}{}",
                Self::name(*reg).unwrap_or("?"),
                if Self::has_volume_update(*reg) && val & 0x100 != 0 {
                    "  volume-update latch set"
                } else {
                    ""
                },
            ));
        }
        // The order is the other half of the answer: an initialisation is a sequence, and a
        // histogram of it is not.
        out.push(format!("  in order: {}", self.log.census()));
        let shown = 24.min(self.log.sample().len());
        for (reg, val) in self.log.iter().take(shown) {
            out.push(format!(
                "    reg {reg:#04x} {:<12} = {val:#05x}",
                Self::name(*reg).unwrap_or("?")
            ));
        }
        if let Some(line) = self.log.more_line(shown) {
            out.push(format!("    {line}"));
        }
        for (ctrl, d) in self.malformed.iter() {
            out.push(format!("    not two bytes: ctrl {ctrl:#04x} data {d:02x?}"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One two-byte write as the firmware sends it: CTRL `0x82`, first byte then value byte.
    fn send(c: &mut Wm8758, byte0: u8, byte1: u8) {
        c.transfer(0x82, [byte0, byte1, 0, 0]);
    }

    /// The first bytes a 52-transfer boot sends, with the counts the captured runs recorded.
    ///
    /// From `_debug/syn/hl.log`, the fullest of the fifty logs that report `dev 0x34  52
    /// transfers` — most show only the first three rows, because the report cuts at twelve
    /// `(device, register)` pairs across the whole bus. **Eighteen of the fifty-two**; the rest are
    /// under the cut in every captured run, so this is a floor on what the codec was told.
    const BASELINE: [(u8, u64); 5] = [(0x54, 5), (0x6f, 4), (0x06, 3), (0x6b, 3), (0x6c, 3)];

    /// Every first byte *named* against `dev 0x34` anywhere under `_debug/`, including runs whose
    /// codec traffic is not 52 transfers. A floor: each report shows twelve pairs and no more.
    const ALL_SEEN: [u8; 12] = [
        0x02, 0x06, 0x0d, 0x16, 0x19, 0x54, 0x56, 0x69, 0x6b, 0x6c, 0x6d, 0x6f,
    ];

    fn replay_baseline() -> Wm8758 {
        let mut c = Wm8758::new();
        for (byte0, n) in BASELINE {
            for i in 0..n {
                send(&mut c, byte0, i as u8);
            }
        }
        c
    }

    /// The captured traffic decodes onto registers the part actually has.
    ///
    /// This is the test the decode has to survive. `0x54 >> 1` is `0x2a`, not `0x54` — the run
    /// report's "reg 0x54" is a first byte, and `NEXT.md` §5 reads it as a register number. If the
    /// shift were dropped, four of these five would land past R57 and `out_of_map` would say so.
    #[test]
    fn the_captured_first_bytes_decode_onto_registers_the_part_has() {
        let c = replay_baseline();
        assert_eq!(c.writes, 18);
        assert_eq!(c.out_of_map, 0, "every decoded register is inside the map");
        let regs: Vec<u8> = c.written.keys().copied().collect();
        assert_eq!(regs, vec![0x03, 0x2a, 0x35, 0x36, 0x37]);
        assert_eq!(c.written[&0x2a].0, 5, "the busiest register on the boot");
        assert_eq!(c.written[&0x37].0, 4, "ROUT2VOL");
        assert_eq!(c.written[&0x03].0, 3, "PWRMGMT3");
        assert_eq!(c.written[&0x35].0, 3, "ROUT1VOL");
        assert_eq!(c.written[&0x36].0, 3, "LOUT2VOL");
        // The pair that makes the decode believable: LOUT2VOL is written with bit 8 clear and
        // ROUT2VOL with it set, which is a stereo volume being latched in one go.
        assert_eq!(c.written[&0x36].1 & 0x100, 0);
        assert_eq!(c.written[&0x37].1 & 0x100, 0x100);
    }

    /// The control for the test above: prove it can fail.
    ///
    /// Reading the first byte as the register — the reading `NEXT.md` §5 invites and the one the
    /// bus report's label suggests — puts most of this traffic past the end of a 58-register part.
    /// So the assertion above is not satisfiable by accident, and `out_of_map` is an instrument
    /// that moves.
    #[test]
    fn reading_the_first_byte_as_the_register_falls_off_the_part() {
        let mut c = Wm8758::new();
        for byte0 in ALL_SEEN {
            // Feed the raw first byte as if it were the register number, which is what a decode
            // without the shift would produce.
            c.write_reg(byte0, 0);
        }
        assert_eq!(
            c.out_of_map, 7,
            "0x54 0x56 0x69 0x6b 0x6c 0x6d 0x6f are all past R57"
        );
        assert!(c.out_of_map > 0, "the wrong decode is detectable");
        // And the model says so out loud rather than only in a field.
        assert!(c
            .report()
            .iter()
            .any(|l| l.contains("the register decode is wrong")));
    }

    /// The low bit of the first byte is the value's ninth bit. It is not part of the register.
    ///
    /// Two transfers one bit apart must reach the *same* register with *different* values. A decode
    /// that folded the bit into the register number would produce two registers and one value.
    #[test]
    fn the_first_bytes_low_bit_is_the_values_ninth_bit() {
        let mut c = Wm8758::new();
        send(&mut c, 0x6a, 0xff);
        send(&mut c, 0x6b, 0xff);
        assert_eq!(c.written.len(), 1, "one register, written twice");
        assert_eq!(c.written[&0x35].0, 2);
        assert_eq!(c.reg(0x35), Some(0x1ff), "the second write set bit 8");
        let seq: Vec<_> = c.log.iter().copied().collect();
        assert_eq!(seq, vec![(0x35, 0x0ff), (0x35, 0x1ff)]);
    }

    /// Why the decode is believable and not merely self-consistent.
    ///
    /// Every first byte with its low bit set — six distinct ones across the captured runs — decodes
    /// onto a register that has a defined bit-8 function in the family map: the volume-update latch,
    /// or `CLKSEL` on the clock generator. That is six coincidences under the right decode and none
    /// under any other, which is the whole of the evidence that the shift is where it belongs.
    #[test]
    fn every_odd_first_byte_lands_on_a_register_with_a_ninth_bit() {
        // Driven through the model rather than through the arithmetic, so a wrong decode fails
        // here too instead of leaving this as a restatement of the table.
        let mut c = Wm8758::new();
        for byte0 in ALL_SEEN {
            send(&mut c, byte0, 0xff);
        }
        assert_eq!(c.out_of_map, 0);
        // Read off the ordered log rather than the per-register tally: the tally keeps the LAST
        // value, so a register written both ways — `0x36`, from first bytes `0x6c` and `0x6d` —
        // would be classified by the order the bytes happen to be listed in.
        let latched: BTreeMap<u8, ()> = c
            .log
            .iter()
            .filter(|(_, v)| v & 0x100 != 0)
            .map(|(reg, _)| (*reg, ()))
            .collect();
        let latched: Vec<u8> = latched.into_keys().collect();
        assert_eq!(
            latched,
            vec![0x06, 0x0c, 0x34, 0x35, 0x36, 0x37],
            "the six registers ever written with bit 8 set"
        );
        for reg in &latched {
            assert!(
                Wm8758::has_volume_update(*reg) || *reg == 0x06,
                "reg {reg:#04x} was written with bit 8 and has no bit-8 function"
            );
        }
        // Every register reached at all is one the part has; nothing needed a bit 8 it lacks.
        let all: Vec<u8> = c.written.keys().copied().collect();
        assert_eq!(
            all,
            vec![0x01, 0x03, 0x06, 0x0b, 0x0c, 0x2a, 0x2b, 0x34, 0x35, 0x36, 0x37]
        );
    }

    /// A read is counted and answers nothing. The model must not become a bus device by accident:
    /// if it ever started supplying bytes, attaching it would change what the firmware sees, and
    /// every measurement taken before it was attached would stop comparing.
    #[test]
    fn a_read_is_counted_and_answered_by_nothing() {
        let mut c = Wm8758::new();
        c.transfer(0xa2, [0x54, 0, 0, 0]);
        assert_eq!(c.reads, 1);
        assert_eq!(c.writes, 0);
        assert!(c.written.is_empty(), "a read must not touch the file");
        assert!(c
            .report()
            .iter()
            .any(|l| l.contains("write-only premise needs re-checking")));
    }

    /// A transfer of any other length has no meaning under this packing, so it is kept rather than
    /// decoded into a plausible register.
    #[test]
    fn a_transfer_that_is_not_two_bytes_is_kept_rather_than_decoded() {
        let mut c = Wm8758::new();
        c.transfer(0x80, [0x54, 0, 0, 0]); // one byte
        c.transfer(0x84, [0x54, 0x01, 0x02, 0]); // three bytes
        assert_eq!(c.writes, 0, "nothing was decoded");
        assert_eq!(c.malformed.seen(), 2);
        assert!(c
            .report()
            .iter()
            .any(|l| l.contains("not two bytes: ctrl 0x80")));
    }

    /// `R0` is a software reset, and it clears the file rather than becoming a value in it.
    #[test]
    fn writing_r0_resets_the_register_file() {
        let mut c = Wm8758::new();
        send(&mut c, 0x6b, 0xff);
        assert_eq!(c.reg(0x35), Some(0x1ff));
        send(&mut c, 0x00, 0x00);
        assert_eq!(c.resets, 1);
        assert_eq!(c.reg(0x35), Some(0), "the reset cleared it");
        // The write is still on the record: a reset is a thing the firmware did.
        assert_eq!(c.written[&0x00].0, 1);
        assert_eq!(c.writes, 2);
    }

    /// The report says what the codec was told — the registers, the counts and the values.
    ///
    /// An unobservable model is indistinguishable from no model, so the report is part of the
    /// device rather than a debugging aid bolted to it.
    #[test]
    fn the_report_says_what_the_codec_was_told() {
        let c = replay_baseline();
        let text = c.report().join("\n");
        assert!(text.contains("18 register writes across 5 registers"));
        assert!(text.contains("reg 0x37 ROUT2VOL"));
        assert!(text.contains("reg 0x03 PWRMGMT3"));
        assert!(
            text.contains("volume-update latch set"),
            "bit 8 is called out"
        );
        // The busiest register is unnamed on purpose; the report must print the number anyway.
        assert!(text.contains("reg 0x2a ?"));
        // Order, not only census.
        assert!(text.contains("in order: 18"));
        // A silent run prints nothing at all.
        assert!(Wm8758::new().report().is_empty());
    }
    /// Write a register the way the firmware's helper does.
    fn put(c: &mut Wm8758, reg: u8, val: u16) {
        send(c, (reg << 1) | (val >> 8) as u8, val as u8);
    }

    /// Rockbox's per-rate PLL constants and R6 value, from `docs/hw/wm8758-i2s.md` section 4.
    const RATE_TABLE: [(u32, [u16; 4], u16); 9] = {
        const A: [u16; 4] = [0x017, 0x021, 0x161, 0x026];
        const B: [u16; 4] = [0x018, 0x00c, 0x093, 0x0e9];
        [
            (48000, B, 0x145),
            (44100, A, 0x145),
            (32000, B, 0x165),
            (24000, B, 0x185),
            (22050, A, 0x185),
            (16000, B, 0x1a5),
            (12000, B, 0x1c5),
            (11025, A, 0x1c5),
            (8000, B, 0x1e5),
        ]
    };

    fn bring_up(rate_regs: &[u16; 4], r6: u16) -> Wm8758 {
        let mut c = Wm8758::new();
        put(&mut c, 0x01, 0x02f);
        put(&mut c, 0x06, 0x001);
        for (i, v) in rate_regs.iter().enumerate() {
            put(&mut c, 0x24 + i as u8, *v);
        }
        put(&mut c, 0x06, r6);
        c
    }

    #[test]
    fn every_rockbox_rate_decodes_from_its_pll_and_divider() {
        for (rate, pll, r6) in RATE_TABLE {
            let c = bring_up(&pll, r6);
            assert_eq!(
                c.sample_rate(Wm8758::DEFAULT_MCLK_HZ),
                Some(rate),
                "R6 {r6:#x}"
            );
        }
    }

    #[test]
    fn the_pll_constants_are_exact_to_a_hertz() {
        let c = bring_up(&RATE_TABLE[1].1, 0x145);
        let fs = c.sysclk_hz(Wm8758::DEFAULT_MCLK_HZ).unwrap() / 256.0;
        assert!((fs - 44100.0).abs() < 1.0, "{fs}");
        let c = bring_up(&RATE_TABLE[0].1, 0x145);
        let fs = c.sysclk_hz(Wm8758::DEFAULT_MCLK_HZ).unwrap() / 256.0;
        assert!((fs - 48000.0).abs() < 0.01, "{fs}");
    }

    #[test]
    fn no_rate_until_the_codec_is_master_and_its_clock_is_running() {
        let mut c = Wm8758::new();
        assert_eq!(c.sample_rate(Wm8758::DEFAULT_MCLK_HZ), None, "reset");
        // CLKSEL = PLL, MS clear.
        put(&mut c, 0x06, 0x144);
        assert_eq!(c.sample_rate(Wm8758::DEFAULT_MCLK_HZ), None, "slave");
        put(&mut c, 0x06, 0x145);
        assert_eq!(c.sample_rate(Wm8758::DEFAULT_MCLK_HZ), None, "PLLEN clear");
        put(&mut c, 0x01, 0x020);
        assert_eq!(
            c.sample_rate(Wm8758::DEFAULT_MCLK_HZ),
            None,
            "PLL N is zero"
        );
        for (i, v) in RATE_TABLE[1].1.iter().enumerate() {
            put(&mut c, 0x24 + i as u8, *v);
        }
        assert_eq!(c.sample_rate(Wm8758::DEFAULT_MCLK_HZ), Some(44100));
        put(&mut c, 0x01, 0x000);
        assert_eq!(
            c.sample_rate(Wm8758::DEFAULT_MCLK_HZ),
            None,
            "PLLEN dropped"
        );
    }

    #[test]
    fn mclk_direct_decodes_without_the_pll() {
        // 12.288 MHz on the pin, /1: SYSCLK 12.288 MHz, 48 kHz.
        let mut c = Wm8758::new();
        put(&mut c, 0x06, 0x001);
        assert_eq!(c.sample_rate(12_288_000.0), Some(48000));
        assert_eq!(c.sample_rate(11_289_600.0), Some(44100));
        // Off the standard list: rounded, not snapped.
        assert_eq!(c.sample_rate(10_240_000.0), Some(40000));
        // Out of the part's range.
        assert_eq!(c.sample_rate(24_000_000.0), None);
    }

    #[test]
    fn dac_enables_come_from_r3() {
        let mut c = Wm8758::new();
        assert_eq!(c.dac_enabled(), (false, false));
        put(&mut c, 0x03, 0x00f);
        assert_eq!(c.dac_enabled(), (true, true));
        put(&mut c, 0x03, 0x002);
        assert_eq!(c.dac_enabled(), (false, true));
    }
    #[test]
    fn r61_is_a_register_and_r58_to_r60_are_not() {
        let mut c = Wm8758::new();
        put(&mut c, 0x3d, 0x100);
        assert_eq!((c.out_of_map, c.reg(0x3d)), (0, Some(0x100)));
        for r in 0x3a..=0x3c {
            put(&mut c, r, 1);
        }
        assert_eq!(c.out_of_map, 3);
        assert!(c.report().iter().any(|l| l.contains("R0..R57 and R61")));
    }
}
