//! The Philips PCF50605 — power, the real-time clock, and the battery.

use crate::*;

/// A minimal ATA device, enough for RetailOS to identify a disk and read sectors off it.
///
/// Register layout from Rockbox `firmware/target/arm/pp/ata-target.h` — a 4-byte stride from
/// `IDE_BASE + 0x1e0`:
///
/// ```text
/// +0x1e0  DATA (16-bit)   +0x1f0  LCYL
/// +0x1e4  ERROR/FEATURES  +0x1f4  HCYL
/// +0x1e8  NSECTOR         +0x1f8  SELECT
/// +0x1ec  SECTOR          +0x1fc  STATUS (read) / COMMAND (write)
/// +0x3f8  CONTROL
/// ```
///
/// The PCF50605 power-management chip, on I²C address `0x08`.
///
/// Register map from Rockbox's `firmware/export/pcf5060x.h`; the power-on values below are the
/// per-model defaults its `pcf50605_init()` documents in comments for an **iPod Video**
/// specifically, which is what this part should read as before firmware touches it.
///
/// This replaces `--i2c-fill=0xff`, which answered every read with all-ones. That was never a
/// device — it was a probe for "is the firmware stuck on a bit that never asserts", and it made
/// every status bit read as set, so any init path that checked a result got a plausible lie. Per
/// [research/03](../../../research/03-rtxc-and-the-video-coprocessor.md) §36 the bypass cannot be
/// removed to test its effect — the bootloader needs *an* answer — so the only way to find out what
/// it was hiding is to put a real chip behind it.
///
/// **What is honest here and what is not.** The register file, the read-clearing interrupt
/// registers, the pointer/auto-increment behaviour and the transfer decoding are the documented
/// part. The *analog* values are invented: nothing in the dumps says what voltage this battery
/// reports. They are marked at each site.
pub struct Pcf50605 {
    regs: [u8; 0x40],
    /// Register pointer. A one-byte write sets it; that is how the driver sets up a read.
    ptr: u8,
    /// The four data registers the controller latches a read into.
    data: [u8; 4],
    /// Read transfers still owed before a conversion reports complete.
    ///
    /// A conversion that is finished before it is started is the one thing real hardware never
    /// does, and a driver written against real hardware is entitled to notice. Everything else in
    /// this model resolves instantly — the PLL, the ADC's arithmetic — but the *observability* of
    /// this one has to change over time or a poll loop has nothing to wait for.
    ///
    /// **Counted in simulated microseconds, not in transfers** — and the difference is a whole
    /// operating system.
    ///
    /// This was a countdown of two *read transfers*, which is right for a driver that polls the
    /// ready bit: Apple's does, so its poll loop supplied the transfers and the conversion landed.
    /// Rockbox's `_adc_read` does not poll. It writes `ADCC1` and reads `ADCS1`/`ADCS2`
    /// immediately, one read per conversion, then starts the next — so the countdown went 2 → 1,
    /// was reset to 2, and `latch` **never ran once in a 27 000-conversion boot**. The result
    /// registers held their reset value for the entire run, Rockbox read 0 mV, and
    /// `query_force_shutdown()` powered the machine off.
    ///
    /// A conversion completes because time passes — never because of how the driver is written,
    /// which is what a transfer countdown made it. That is the same mistake as `OPTO_REPLY_USEC`
    /// and `IDE_COMPLETION_USEC`, for the third time in this file, and the previous two both carry
    /// a comment saying it must not happen again.
    ///
    /// **The unit here is "before the host looks again", and that is a statement about hardware,
    /// not a shortcut.** A 10-bit conversion on this part takes microseconds; one I²C transaction
    /// at 400 kHz takes on the order of 70. The conversion is therefore always finished by the
    /// time the host can next address the chip — so `settle` runs at the top of every transfer,
    /// and the transfer that *starts* a conversion cannot also finish it. A µs deadline was tried
    /// first and is wrong here for a specific reason: this model's bus costs **no simulated time**,
    /// so a deadline measured in µs is compared against a clock that never advanced for the
    /// transaction it was supposed to outlast.
    ///
    /// The earlier lesson survives and is still load-bearing: the two halves of one result,
    /// fetched in a single I²C transfer, must describe the *same* state of the converter.
    /// Settling inside `read_reg(0x30)` broke that and answered every completed conversion with
    /// zero (research/10 Addendum 30). Settling happens once per transfer, before any byte.
    settling: bool,
    /// The conversion in flight, latched into `ADCS1`/`ADCS2` when its deadline passes.
    ///
    /// Result registers are result registers: while a conversion runs they hold the *previous*
    /// one, with the ready bit clear. The model used to answer `ADCS1` with a synthetic `0` while
    /// in flight, which is not something the part does and is what destroyed the value.
    pending: Option<u16>,
    /// `(register, value)` overrides applied on read, from `--pmu-force`.
    ///
    /// The point of these is bisection. When the firmware sits polling one register block, the
    /// question is *which byte in it* is the one being waited on, and the cheap way to answer that
    /// is to pin one candidate at a time and see which one lets the boot proceed. Same role
    /// `--rdval` plays for memory-mapped status bits, and the same caveat: **each one is a
    /// hypothesis**, not a model.
    pub force: Vec<(u8, u8)>,
    /// `--pmu-adc=CH=VALUE` — per-channel ADC results, so a channel can be answered on its own
    /// scale. The PCF50605 mux has resistive and *subtractor* modes for the same input, and they
    /// do not share a scale; one catch-all number cannot be right for both.
    pub adc_values: Vec<(u8, u16)>,
    /// Reads per register, counted where the register is actually known.
    ///
    /// The I²C log cannot answer this. Its data column is the controller's data registers, which on
    /// a *read* transfer still hold whatever was last written there — so reading the pointer out of
    /// it reports the register of the preceding write, which is right only by accident. Counting
    /// here, inside the device, is the difference between knowing which register a poll loop is
    /// hammering and inferring it.
    pub polled: BTreeMap<u8, u64>,
    /// `register -> (writes, last value)`, **uncapped**, counted inside the device.
    ///
    /// The mirror of [`polled`](Self::polled), and it exists for the same reason: the I²C log's data
    /// column cannot tell you which register a byte was destined for. Written because a whole class
    /// of question — *where does the firmware put this setting* — is answerable by moving a control
    /// and seeing which register moved with it, and there was no way to ask it.
    pub written: BTreeMap<u8, (u64, u8)>,
    /// Every ADC conversion: (channel, value returned), so the channel map can be read off a run
    /// rather than guessed. `ADCC2` bits 4:1 select the channel. An ordered **sample**; the
    /// per-channel census is `adc_by_channel`.
    pub adc_log: Capped<(u8, u16)>,
    /// `channel -> (conversions, last value)`, **uncapped**. The run report's by-channel table was a
    /// tally of the capped log, so on a poll-heavy boot it reported the first 4 096 conversions'
    /// distribution under a header that read as a total.
    pub adc_by_channel: BTreeMap<u8, (u64, u16)>,
    pub reads: u64,
    pub writes: u64,
}

impl Default for Pcf50605 {
    fn default() -> Self {
        Self::new()
    }
}

impl Pcf50605 {
    /// I²C address, from Rockbox's `pcf50605_read`/`_write`, which pass `0x8`.
    pub const ADDR: u8 = 0x08;

    pub fn new() -> Self {
        let mut regs = [0u8; 0x40];
        // iPod Video power-on defaults, quoted from Rockbox `pcf50605_init()`.
        regs[0x1b] = 0xec; // DCDC1   core supply, 1.2 V on
        regs[0x21] = 0xe3; // DCUDC1  1.8 V on
        regs[0x23] = 0xf8; // IOREGC  I/O + GPO supply, 3.3 V on
        regs[0x24] = 0xf5; // D1REGC1 codec supply, 3.0 V on
        regs[0x26] = 0xf5; // D3REGC1 LCD supply, 3.0 V on
        regs[0x27] = 0x1f; // LPREGC1 off
        Self {
            regs,
            ptr: 0,
            data: [0; 4],
            settling: false,
            pending: None,
            force: Vec::new(),
            adc_values: Vec::new(),
            polled: BTreeMap::new(),
            written: BTreeMap::new(),
            adc_log: Capped::new(4096),
            adc_by_channel: BTreeMap::new(),
            reads: 0,
            writes: 0,
        }
    }

    /// Answer the battery channel from the **host machine's** charge instead of a fixed number.
    ///
    /// `pct` is 0..=100. Rockbox's `powermgmt-ipod-pcf.c` fixes the scale in one line —
    /// `mV = (adc * 6000) >> 10` — so the only invented part is percent-to-millivolts, and that
    /// is the usual Li-ion working range: 3400 mV (Rockbox's danger threshold, where it prints
    /// "Battery empty! RECHARGE!") up to 4200 mV at full. A desktop with no battery is reported
    /// as 100, not as flat, because "no battery" and "dead battery" are opposite facts and only
    /// one of them should stop a boot.
    ///
    /// This pushes onto `adc_values`, so an explicit `--pmu-adc=2=…` set afterwards still wins.
    pub fn set_battery_percent(&mut self, pct: u8) {
        let mv = 3400 + u32::from(pct.min(100)) * 8;
        let code = ((mv << 10) / 6000) as u16;
        self.adc_values.push((0x2, code));
    }

    /// Seed the real-time clock registers from the host's local time.
    ///
    /// **The register numbers are from the PCF5060x datasheet family, not from a measurement of
    /// this firmware.** `RTCSC`/`RTCMN`/`RTCHR`/`RTCWD`/`RTCDT`/`RTCMT`/`RTCYR` sit at 0x0a..0x10
    /// in BCD, seconds first. Nothing in a captured run has been seen reading them — booting the
    /// firmware needs a NOR dump — so if the iPod turns out to keep its clock somewhere else,
    /// this is where that will show up, and `polled` will say so as soon as a boot runs.
    pub fn set_clock(&mut self, tm: [u8; 7]) {
        let bcd = |v: u8| ((v / 10) << 4) | (v % 10);
        for (i, &v) in tm.iter().enumerate() {
            self.regs[0x0a + i] = bcd(v);
        }
    }

    /// One I²C transfer. `ctrl` is the PP controller's CTRL word — bit `0x20` selects a read, bits
    /// 1..2 carry `len - 1`.
    pub fn transfer(&mut self, ctrl: u8, d: [u8; 4]) {
        let len = (((ctrl >> 1) & 3) as usize + 1).min(4);
        // Settle any finished conversion **before this transfer is looked at at all** — before a
        // byte is served and before a write can start the next one. Both halves matter:
        //
        // - before the bytes, so every byte of one read describes one state of the converter
        //   (research/10 Addendum 30);
        // - before the write, because Rockbox's next contact with this chip after reading is the
        //   *write* that starts the following conversion, 400 ms later. Settling only on reads
        //   left the result of every conversion un-latched at the moment the next one replaced
        //   it, which is the transfer-countdown bug again wearing a clock.
        //
        // The host reading or writing a register is when it finds out; it is not what makes it
        // happen. Time is.
        if self.settling {
            self.settling = false;
            self.latch();
        }
        if ctrl & 0x20 != 0 {
            for i in 0..len {
                self.data[i] = self.read_reg(self.ptr.wrapping_add(i as u8));
            }
            // The pointer auto-increments across the bytes read, and this is load-bearing rather
            // than a detail: `i2c_readbytes` splits a request longer than 4 bytes into several
            // transfers and re-sends the address **only once**, relying on the device to carry on
            // where it left off. A pointer that did not advance would answer an 8-byte block read
            // with the same 4 bytes twice, which reads as a device stuck rather than a bus bug.
            self.ptr = self.ptr.wrapping_add(len as u8);
            self.reads += 1;
        } else {
            // The first byte of a write is always the register address, so a one-byte write only
            // moves the pointer. Longer writes carry values for consecutive registers.
            self.ptr = d[0];
            for (i, &value) in d.iter().enumerate().take(len).skip(1) {
                self.write_reg(self.ptr.wrapping_add(i as u8 - 1), value);
            }
            self.ptr = self.ptr.wrapping_add(len as u8 - 1);
            self.writes += 1;
        }
    }

    /// The byte the controller would present at data register `i`.
    pub fn data_byte(&self, i: usize) -> u8 {
        self.data[i.min(3)]
    }

    fn read_reg(&mut self, reg: u8) -> u8 {
        let r = (reg & 0x3f) as usize;
        *self.polled.entry(r as u8).or_insert(0) += 1;
        if let Some(&(_, v)) = self.force.iter().find(|&&(f, _)| f as usize == r) {
            return v;
        }
        match r {
            // INT1..INT3 clear on read. A driver that polls them depends on this: leaving a source
            // latched would make one event look like an endless stream of them.
            0x02..=0x04 => std::mem::take(&mut self.regs[r]),
            // ADCS1/ADCS2/ADCS3 are plain result registers here. Whether a conversion is in flight
            // is carried by ADCS2 bit 7 alone — cleared when the conversion starts, set by `latch`
            // when it finishes — so nothing in this function needs to know about `busy`, and a
            // multi-byte read of the pair cannot straddle the transition.
            _ => self.regs[r],
        }
    }

    fn write_reg(&mut self, reg: u8, val: u8) {
        let r = (reg & 0x3f) as usize;
        // Before the read-only guard below, because a write the part ignores is still a write the
        // firmware made, and "which register did it aim at" is the question this answers.
        let e = self.written.entry(r as u8).or_insert((0, 0));
        e.0 += 1;
        e.1 = val;
        // The ADC result registers are read-only on the part. Letting a write land on them lets the
        // firmware overwrite the very value it is about to poll for, which presents as a converter
        // that never produces a result rather than as a bad write.
        if (0x30..=0x32).contains(&r) {
            return;
        }
        self.regs[r] = val;
        // ADCC1/ADCC2 carry the start bit and the channel select. A real conversion takes far less
        // time than the firmware's polling loop, so it resolves immediately rather than being
        // modelled as taking time — the same call made for the PLL.
        if r == 0x2e || r == 0x2f {
            self.convert();
        }
    }

    /// Latch a conversion result into ADCS1/ADCS2 as a 10-bit value.
    ///
    /// Split per Rockbox: `ADCS1` holds bits 9:2 and `ADCS2` the low two, so a driver recombines
    /// them as `ADCS1 << 2 | (ADCS2 & 3)`.
    ///
    /// **The numbers are invented.** Nothing we have says what this battery reads. They are chosen
    /// to sit mid-to-high scale so nothing looks flat, empty or disconnected; if the firmware turns
    /// out to care about the exact scaling, that will show up as a decision it makes differently.
    /// Record one conversion in both instruments — the uncapped per-channel tally and the ordered
    /// sample. One helper so a future branch cannot update only the log, which is how the by-channel
    /// table came to be a tally of a capped sample in the first place.
    fn note_conversion(&mut self, channel: u8, value: u16) {
        let e = self.adc_by_channel.entry(channel).or_insert((0, value));
        e.0 += 1;
        e.1 = value;
        self.adc_log.push((channel, value));
    }

    fn convert(&mut self) {
        let channel = (self.regs[0x2f] >> 1) & 0xf;
        // Rockbox's `powermgmt-ipod-pcf.c` gives the scale: `mV = (adc * 6000) >> 10`, so 0x2c0 is
        // 4125 mV and the 0x200 catch-all is 3000 mV. The catch-all for **unknown** channels is
        // left as-is deliberately: raising it did NOT let the bootloader boot with no charger
        // present (research/09), so the threshold is not simply "a healthy cell" and inventing a
        // higher number would be guessing. Channel 2 has since stopped being an unknown channel —
        // Rockbox names it — so it is answered from a source rather than from that guess.
        let value: u16 = match self.adc_values.iter().find(|&&(c, _)| c == channel) {
            Some(&(_, v)) => v,
            None => match channel {
                // Channel 2 is the battery **on this board**, and Rockbox says so in one line:
                // `adc_battery->channelnum = 0x2; /* ADCVIN1, resistive divider */`
                // (`firmware/target/arm/ipod/adc-ipod-pcf.c`, `adc_init`). The 0/1/0xc below are
                // the PCF50605's own battery inputs from the datasheet; the iPod does not use
                // them for this. Answering 2 with the 3000 mV catch-all is what made Rockbox
                // print **"Battery empty! RECHARGE! Shutting down…"** and power off after a
                // complete, disk-mounting boot — its danger threshold is 3400 mV.
                0x0 | 0x1 | 0x2 | 0xc => 0x2c0, // 704 -> 4125 mV, a charged, not-full cell
                0x4 => 0x200,                   // battery temperature — mid-scale, i.e. not hot
                _ => 0x200,                     // unknown channels
            },
        };
        self.note_conversion(channel, value);
        // Starting a conversion clears the ready bit and leaves the result registers holding the
        // PREVIOUS result. It does not publish the new one — `latch` does that, `busy` transfers
        // later. Overwriting them here and answering `ADCS1` with a synthetic zero "while busy"
        // was the defect: the countdown was consumed by the ADCS1 read of a two-byte poll, so the
        // poll that finally saw ready set had already been handed a zero for the value.
        //
        // Bit 7 of ADCS2 is **conversion-ready**, and it is the whole reason the all-ones bypass
        // was ever needed. Found by forcing the register: 0x80 and 0xff boot, 0x04 does not — so it
        // is bit 7 specifically and not the result bits beside it. Apple's firmware polls this pair
        // and will not proceed without it; with `--i2c-fill=0xff` the bit was set by accident,
        // which is exactly how a bypass hides a fact for months.
        self.regs[0x31] &= !0x80;
        self.regs[0x32] &= !0x01;
        self.pending = Some(value);
        self.settling = true;
        // The start bit is self-clearing, as it is on every converter that has one: the driver
        // writes it to begin and the hardware drops it when the result is latched.
        self.regs[0x2f] &= !0x01;
    }

    /// Publish the conversion that was in flight. Called when its deadline passes, from `transfer`.
    ///
    /// Split per Rockbox: `ADCS1` holds bits 9:2 and `ADCS2` the low two plus the ready bit in
    /// bit 7, so a driver recombines them as `ADCS1 << 2 | (ADCS2 & 3)`.
    fn latch(&mut self) {
        let Some(value) = self.pending.take() else {
            return;
        };
        self.regs[0x30] = (value >> 2) as u8;
        self.regs[0x31] = (value & 3) as u8 | 0x80;
        // ADCS3 bit 0 read as a conversion-ready flag. **Unverified** — the bit that the firmware
        // actually polls is not documented in anything we have, and this is the first candidate.
        // If it is wrong the I²C log will show a register being read over and over, which is
        // exactly the signature that made the old all-ones fill necessary in the first place.
        self.regs[0x32] |= 0x01;
    }
}
