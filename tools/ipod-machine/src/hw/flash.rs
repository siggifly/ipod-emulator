//! The NOR flash controller.
//!
//! Named `flash` rather than `nor` because `crate::nor` already exists and is a different
//! thing: that one BUILDS a NOR image from a model and a seed, this one is the part that
//! answers reads and writes while a machine is running.

use crate::*;

/// A completed operation, handed back for the bus to apply because the chip cannot reach the
/// regions that hold its bytes — the same split as the ATA DMA engine's `dma_ready`.
pub enum NorOp {
    Erase { start: u32, len: u32 },
    Program { off: u32, val: u16 },
}

impl NorOp {
    /// A program can only *clear* bits — that is what makes a NOR need an erase at all — so it ANDs
    /// rather than assigns. Modelling it as a plain store would let an update that forgot to erase
    /// appear to succeed here and fail on hardware.
    pub fn apply(&self, data: &mut [u8]) {
        match *self {
            NorOp::Erase { start, len } => {
                let end = (start + len) as usize;
                if end <= data.len() {
                    data[start as usize..end].fill(0xff);
                }
            }
            NorOp::Program { off, val } => {
                let i = off as usize;
                if i + 2 <= data.len() {
                    let cur = u16::from_le_bytes([data[i], data[i + 1]]);
                    data[i..i + 2].copy_from_slice(&(cur & val).to_le_bytes());
                }
            }
        }
    }
}

/// The NOR as a JEDEC/CFI device rather than a read-only region.
///
/// Apple's bootloader will not touch the flash until it has *identified* it. `0x40009f88` writes
/// `0xAA`/`0x55`/`0x90` to the unlock addresses and reads a manufacturer/device pair back, then
/// looks that pair up in an 8-row table at flash+`0x1d0e0`. Against a plain memory region the
/// "reply" is whatever the dump holds at offset 0 — `0x1ffe`/`0xea00`, which is the reset branch
/// `b 0x8000` read as two IDs — so no row matches and every path that writes flash is dead. The
/// `aupd` updater carries a byte-identical copy of that driver at `0x100051fc` and fails the same
/// way, which is ledger bypass #12.
///
/// The eight rows the ROM accepts, decoded from the image (identical in the prototype and retail
/// dumps), with the geometry read the way `0x40009eb8` reads it — triples of
/// `(start, end, sector)` in **2 KiB units** from row+`0x18`, terminated by `0xffff`:
///
/// ```text
///   mfr    dev     size    sectors
///   0x00ec 0x22b2  1 MiB   uniform 64 KiB          Samsung
///   0x0001 0x226b  1 MiB   16/8/8/32K then 64 KiB  AMD      (AM29LV800B bottom boot)
///   0x0004 0x226b  1 MiB   16/8/8/32K then 64 KiB  Fujitsu
///   0x00bf 0x273f  1 MiB   uniform 4 KiB           SST
///   0x00bf 0x2781  1 MiB   uniform 4 KiB           SST      (SST39VF800A)
///   0x00b0 0x0000  1 MiB   8 KiB then 64 KiB       Sharp    — Intel command set
///   0x0020 0x0000  1 MiB   8 KiB then 64 KiB       Intel/ST — Intel command set
///   0x00bf 0x272f  512 KiB uniform 4 KiB           SST
/// ```
///
/// We answer as the SST39VF800A. The dump we have is 1 MiB, which rules out the last row; of the
/// six that remain it is the only **uniform** geometry whose sector the driver computes and the
/// one we erase can never disagree about, and it drives the AMD command set the ROM's probe
/// already speaks. The two Intel-command-set rows would need a second command set implemented for
/// no gain. Nothing in either dump records which part the hardware actually carried — this is a
/// choice among the eight the ROM accepts, not a measurement.
pub struct Nor {
    /// `(base, size)` per address window. Cold boot has two: the reset-time alias at 0, which is
    /// where the ROM's driver addresses the chip, and the PP502x NOR window at `0x20000000`.
    pub windows: Vec<(u32, u32)>,
    /// Regions holding the chip's bytes. Two windows means two copies, and an erase that updated
    /// only one would leave the aliases disagreeing about the same cell.
    pub regions: Vec<&'static str>,
    pub mfr: u16,
    pub dev: u16,
    pub sector: u32,
    mode: NorMode,
    /// How far through an unlock sequence the chip is. Reset to 0 by anything unexpected, which is
    /// what a real part does — a mistyped cycle aborts the command rather than corrupting it.
    seq: u8,
    /// The even half of a halfword store, waiting for its odd half. `Bus::write16`'s default
    /// splits a `strh` into two byte writes, so the chip would otherwise see each 16-bit command
    /// twice — and a program's two data bytes as two separate commands.
    pending_lo: Option<(u32, u8)>,
    /// Command cycles seen, by the command byte. A tally rather than a log: the update writes a
    /// megabyte, and a capped log would saturate and read as a constant.
    pub cmds: BTreeMap<u16, u64>,
    pub erases: u64,
    pub programs: u64,
    /// Cycles that did not decode, with the address. Empty is the passing result; anything here is
    /// a command set we are not modelling — so the count must be uncapped even though the list is.
    pub unknown: Capped<(u32, u16)>,
    mode_changed: bool,
}

impl Nor {
    /// SST39WF800A: 8 Mbit, x16, uniform 4 KiB sectors, JEDEC `0xbf`/`0x273f`.
    ///
    /// The ROM's accept-table holds **two** SST rows with identical uniform 4 KiB geometry —
    /// `0x273f` and `0x2781` — so either boots. We drove `0x2781` first, named for `SST39VF800A`,
    /// which our own [`research/05`] calls a downstream typo: iPodLinux and the EE Times 5.5G BOM
    /// both name the part **`39WF800A`**, and the Rockbox wiki's `VF` spelling cites iPodLinux as
    /// its source. `daniel5151/clicky` independently picked `0x273f` and labels it `SST39WF800A`.
    /// Two lines of evidence for `WF`, none for `VF`, so this follows them.
    ///
    /// A/B'd over a full `flash-update.sh` run before switching: console output, ATA command count
    /// and flash behaviour identical. The runs are not bit-identical — after ~600 M instructions
    /// the resting state has diverged by a handful of interrupts — but nothing that decides the
    /// boot differs. **This still does not establish what the hardware carried**; only a board
    /// photograph does, and neither NOR dump records it.
    pub fn sst39wf800a(windows: Vec<(u32, u32)>, regions: Vec<&'static str>) -> Self {
        Nor {
            windows,
            regions,
            mfr: 0x00bf,
            dev: 0x273f,
            sector: 0x1000,
            mode: NorMode::Array,
            seq: 0,
            pending_lo: None,
            cmds: BTreeMap::new(),
            erases: 0,
            programs: 0,
            unknown: Capped::new(64),
            mode_changed: false,
        }
    }

    /// Byte offset into the chip, if `addr` falls in any of its windows.
    pub fn hit(&self, addr: u32) -> Option<u32> {
        self.windows.iter().find_map(|&(b, n)| {
            let off = addr.wrapping_sub(b);
            (off < n).then_some(off)
        })
    }

    /// Whether reads currently need the chip rather than the backing store. False for the whole
    /// boot except the few thousand instructions around an identify or an update, which is what
    /// keeps the page cache — and instruction fetch out of NOR at address 0 — on the fast path.
    pub fn intercepts(&self) -> bool {
        self.mode != NorMode::Array
    }

    pub fn take_mode_change(&mut self) -> bool {
        std::mem::take(&mut self.mode_changed)
    }

    fn set_mode(&mut self, m: NorMode) {
        if self.mode != m {
            self.mode = m;
            self.mode_changed = true;
        }
    }

    /// `None` means "answer from memory" — the chip is in read-array mode.
    pub fn read(&self, off: u32) -> Option<u8> {
        let word = match self.mode {
            NorMode::Array => return None,
            // A0/A1 are the only address lines an autoselect read decodes. `0x02` is the sector
            // protect bit, and zero is "not protected" — the driver refuses to erase otherwise.
            NorMode::Autoselect => match (off >> 1) & 3 {
                0 => self.mfr,
                1 => self.dev,
                _ => 0x0000,
            },
            NorMode::Cfi => self.cfi(off >> 1),
        };
        Some(word.to_le_bytes()[(off & 1) as usize])
    }

    /// The CFI query table, JEDEC JESD68. Only the fields a driver reads are filled; the rest are
    /// zero, which is what an absent optional field means in this format.
    ///
    /// **Nothing in Apple's ROM or in `aupd` ever reads it.** Both identify the part by autoselect
    /// and dispatch off the ROM's own device table; a full 1 MiB reflash issues `0x98` exactly
    /// zero times. It is here because the driver object calls itself `Cfi!` and because a part
    /// that answers autoselect but not a query is not a part — not because anything measured needs
    /// it. Delete it the day something is shown to want it and still gets this wrong.
    fn cfi(&self, wa: u32) -> u16 {
        let blocks = (0x10_0000u32 / self.sector) as u16 - 1;
        match wa & 0x7f {
            0x10 => 0x51, // 'Q'
            0x11 => 0x52, // 'R'
            0x12 => 0x59, // 'Y'
            0x13 => 0x02, // primary algorithm: AMD/Fujitsu standard command set
            0x15 => 0x40, // primary extended table at word 0x40
            0x1b => 0x27, // Vcc min 2.7 V
            0x1c => 0x36, // Vcc max 3.6 V
            0x1f => 0x04, // typical single-word program: 2^4 us
            0x21 => 0x0a, // typical block erase: 2^10 ms
            0x23 => 0x05, // max program: 2^5 x typical
            0x25 => 0x04, // max erase: 2^4 x typical
            0x27 => 0x14, // device size 2^20
            0x28 => 0x01, // x16 asynchronous interface
            0x2c => 0x01, // one erase-block region
            0x2d => blocks & 0xff,
            0x2e => blocks >> 8,
            0x2f => ((self.sector >> 8) & 0xff) as u16,
            0x40 => 0x50, // 'P'
            0x41 => 0x52, // 'R'
            0x42 => 0x49, // 'I'
            0x43 => 0x31, // major version '1'
            0x44 => 0x30, // minor version '0'
            _ => 0x0000,
        }
    }

    /// One byte of a store into a window. Returns the operation to apply once a full 16-bit cycle
    /// has arrived, because this device is 16 bits wide and a byte on its own is half a command.
    pub fn write(&mut self, off: u32, val: u8) -> Option<NorOp> {
        if off & 1 == 0 {
            self.pending_lo = Some((off, val));
            return None;
        }
        let (lo_off, lo) = self.pending_lo.take()?;
        if lo_off != off - 1 {
            return None;
        }
        self.cycle(off & !1, u16::from_le_bytes([lo, val]))
    }

    /// One 16-bit bus cycle. The AMD command set as the ROM drives it, at `0x40009e54`
    /// (`AA 55 80 / AA 55 30` sector erase), `0x40009f88` (`AA 55 90` autoselect) and
    /// `0x4000a1d8` (`F0` reset).
    ///
    /// Only address bits A10:A0 take part in command decoding, which is why the ROM's `0xaaaa` and
    /// `0x5554` byte addresses — word `0x5555`/`0x2aaa` — unlock a part whose datasheet says
    /// `0x555`/`0x2aa`.
    fn cycle(&mut self, off: u32, val: u16) -> Option<NorOp> {
        // The cycle after a program setup carries DATA, and a real part latches whatever is on the
        // bus rather than decoding it. Deciding this after the reset check instead swallowed every
        // word of the payload whose low byte was 0xff or 0xf0 — 281 612 of the 507 904 words of a
        // full reflash, which the report showed as "reset (Intel) x281612" beside a program count
        // that was less than half the size of the transfer.
        if self.seq == 6 {
            self.seq = 0;
            self.programs += 1;
            self.set_mode(NorMode::Array);
            return Some(NorOp::Program { off, val });
        }
        let cmd = val & 0xff;
        let wa = (off >> 1) & 0x7ff;
        *self.cmds.entry(cmd).or_default() += 1;
        // Reset is accepted in any state and from any address. `0xff` is Intel's; the ROM sends
        // both back to back at 0x40009fd0 so that one probe identifies either family.
        if cmd == 0xf0 || cmd == 0xff {
            self.seq = 0;
            self.set_mode(NorMode::Array);
            return None;
        }
        match (self.seq, wa, cmd) {
            (0, 0x555, 0xaa) | (3, 0x555, 0xaa) => self.seq += 1,
            (1, 0x2aa, 0x55) | (4, 0x2aa, 0x55) => self.seq += 1,
            (2, 0x555, 0x90) => {
                self.seq = 0;
                self.set_mode(NorMode::Autoselect);
            }
            (2, 0x555, 0x98) => {
                self.seq = 0;
                self.set_mode(NorMode::Cfi);
            }
            (2, 0x555, 0x80) => self.seq = 3,
            (2, 0x555, 0xa0) => self.seq = 6,
            (5, 0x555, 0x10) => {
                self.seq = 0;
                self.erases += 1;
                self.set_mode(NorMode::Array);
                return Some(NorOp::Erase {
                    start: 0,
                    len: 0x10_0000,
                });
            }
            (5, _, 0x30) => {
                self.seq = 0;
                self.erases += 1;
                self.set_mode(NorMode::Array);
                return Some(NorOp::Erase {
                    start: off & !(self.sector - 1),
                    len: self.sector,
                });
            }
            _ => {
                self.unknown.push((off, val));
                self.seq = 0;
            }
        }
        None
    }
}
