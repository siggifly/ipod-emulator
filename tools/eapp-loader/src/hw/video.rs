//! The Broadcom BCM2722 — the video co-processor that owns the panel.

use crate::*;

/// The Broadcom BCM2722 video co-processor, at the level of its host protocol.
///
/// `0x30000000` is not a panel controller — it is a bus window onto a second processor. Register
/// map from Rockbox `firmware/target/arm/ipod/video/lcd-video.c`:
///
/// ```text
/// +0x00000  DATA (16-bit)   +0x40000  ALT_DATA
/// +0x10000  WR_ADDR         +0x50000  ALT_WR_ADDR
/// +0x20000  RD_ADDR         +0x60000  ALT_RD_ADDR
/// +0x30000  CONTROL         +0x70000  ALT_CONTROL
/// ```
///
/// The host latches an internal address into `WR_ADDR` or `RD_ADDR` and then streams halfwords
/// through `DATA`, which auto-increments. `CONTROL` carries the handshake bits.
///
/// This models the *protocol and its internal address space*, not the video hardware: enough for
/// Apple's bootloader to upload the `vmcs` firmware and get the acknowledgement it waits for.
pub struct Bcm {
    pub base: u32,
    /// A second address the **same chip** answers at.
    ///
    /// The VideoCore is one part with one register file, and it appears in this machine's memory
    /// map twice. RetailOS, Rockbox and disk mode drive it at `0x30000000`, which is the address
    /// Rockbox's `lcd-video.c` documents. **Apple's diagnostics drives it at `0xb0000000`** — the
    /// same seven registers at the same `0x10000` stride, the same `BCM_CMD(x) = (~x << 16) | x`
    /// encoding, and the same eight bootstrap bytes `a1 81 91 02 12 22 72 62`. Its own assert
    /// strings name the driver: `…\service diag\drivers\vchost.c`, and the file it uploads
    /// through the port is `VMCS    BIN`.
    ///
    /// So this is an **alias, not a second device**: a write through either window lands in the
    /// same co-processor. Whether the hardware decodes both permanently or `diag` switches the
    /// decode (it writes `0x98016460` to `0x70000030`, in the external-bus block, immediately
    /// before its first access) is **not measured**. It makes no difference to anything that runs
    /// here, because `diag` is the only program in Apple's software that touches the second
    /// window and it never touches the first.
    pub alias: Option<u32>,
    /// Internal address space, halfword-granular and sparse — the firmware upload alone is 101 728
    /// bytes and the framebuffer would be far larger, so a flat allocation is the wrong shape.
    pub mem: BTreeMap<u32, u16>,
    wr_addr: u32,
    rd_addr: u32,
    pub halfwords_written: u64,
    pub halfwords_read: u64,
    /// Low byte of a halfword write, held until its high byte arrives.
    pending: u8,
    /// The halfword the last even-address byte read fetched, held so the odd-address read that
    /// follows takes its high byte instead of drawing a second word out of the FIFO.
    rd_pending: u16,
    /// Publish a GENCMD service directory when the host starts the firmware it uploaded, and
    /// answer the RPC that follows. Off by default: with it off the co-processor is a memory and
    /// a protocol, which is what every published measurement was taken against.
    pub registry: bool,
    /// Every GENCMD request the host sent, as `(opcode, payload length)`.
    pub gencmd: Vec<(u32, u32)>,
    /// Requests dropped because the header did not carry the magic, or the reply ring was full.
    pub gencmd_dropped: u64,
    /// Next handle to hand out, and the bump pointer surfaces are allocated from.
    next_handle: u32,
    next_surface: u32,
    /// Where that bump pointer started, for `--surface-base=`.
    ///
    /// **This exists to run one experiment**, the first step research/04 names for retiring
    /// ledger #6: *"move the allocator's base and see whether the drawn frame follows it. If
    /// it does, the pixels are ours and the address is arbitrary; if the panel goes blank,
    /// the address is load-bearing and must be sourced."* `0xE0000` is a **known-wrong**
    /// choice still in place — it is the co-processor's command-parameter buffer, and a real
    /// co-processor would not hand that out as a free resource.
    pub surface_base: u32,
    /// Internal addresses the host reads, and how often — which is how you find the word it is
    /// waiting on without disassembling the poll loop.
    pub read_hist: BTreeMap<u32, u64>,
    /// First few address latches, to check the host's writes are being decoded as intended.
    pub latch_log: Capped<(&'static str, u32, u16, bool)>,
    /// **The co-processor's traffic in the order it happened** — data runs, commands, and the
    /// image operations the commands turned into.
    ///
    /// The latch log answers "was the address decoded", and it answered yes while the panel was
    /// still wrong, because it is a log of *halves* and 81 718 of them scroll past a 24-row cap.
    /// This is the shape of the traffic instead: every time the write pointer moves anywhere other
    /// than the next halfword, the run that just ended is recorded. **A picture written at one
    /// stride where the panel wants another shows up here as one long run**, which is how the Apple
    /// boot logo was found — 4 852 halfwords in a single run at `BCMA_CMDPARAM`, not 78 rows of 62.
    pub timeline: Capped<BcmOp>,
    /// The run in progress: where it started, and where its next halfword would land.
    run_base: u32,
    run_next: u32,
    run_len: u64,
    /// **The frame store — what the panel actually shows.** 320x240 RGB565.
    ///
    /// On the real part this is internal to the co-processor and the host cannot address it: the
    /// host stages an image at `BCMA_CMDPARAM` and issues a command, and the command is what moves
    /// pixels into the store. Rockbox never has to know that, because it stages a whole 320x240
    /// frame and issues `LCD_UPDATE` — for which "the transfer buffer" and "the panel" are the same
    /// picture. Apple's bootloader does know: it stages an 8-word header plus a 62x78 tile and
    /// issues `LCD_UPDATERECT`, and reading `BCMA_CMDPARAM` as the panel then shows the tile lying
    /// at the top-left corner in 62-halfword rows, which is exactly what this model did until the
    /// operation was implemented.
    ///
    /// **A snapshot does not carry this**, in the same way it does not carry `registry`: `restore`
    /// rebuilds the co-processor and `Bcm::new` zeroes the store. What a restored machine *does*
    /// carry is the published copy in `mem`, which is what every instrument reads — so a restored
    /// panel looks right. It would stop looking right if a restored machine ever issued another
    /// `LCD_UPDATERECT`, because that rectangle would land on a black store. Nothing does: all four
    /// commands of a retail boot are the bootloader's, and RetailOS reaches the panel through the
    /// RPC ring instead. Named rather than fixed, because a fix here would have nothing to test
    /// against.
    pub panel: Vec<u16>,
    /// Commands that named a rectangle this model would not honour, with the header that named it.
    pub blits_rejected: Capped<[u32; 8]>,
    /// Commands the host has kicked, and how many were a frame update.
    pub commands: Vec<u16>,
    pub frames: u64,
    /// Which half of the address register the next write fills.
    ///
    /// The host does **not** address the two halves by offset — it writes the same register twice,
    /// low half first. Decoding it by `off & 2` instead left every latched address at zero, so the
    /// firmware upload landed at internal 0 and every read polled the wrong word.
    wr_phase_high: bool,
    rd_phase_high: bool,
}

/// One thing the co-processor was asked to do, in the order it was asked.
#[derive(Clone, Copy, Debug)]
pub enum BcmOp {
    /// A contiguous run of host data writes: where it started and how many halfwords it carried.
    Write { base: u32, halfwords: u64 },
    /// `CONTROL = 0x31` with a well-formed command word in `BCMA_COMMAND`.
    Command { cmd: u16 },
    /// A command that moved pixels into the frame store. `x1`/`y1` are inclusive, matching the
    /// header; `src` is where the tile was read from.
    Blit {
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
        src: u32,
    },
}

/// Bytes between `rd` and `wr` in a ring `[lo, hi)` — RetailOS's own `FUN_000f5834`.
impl Bcm {
    /// Move the surface allocator, for the ledger #6 step-1 ablation.
    ///
    /// Sets both the recorded base and the live bump pointer. Setting only one would make the
    /// flag report a move it had not performed — the shape of a switch that is not wired up,
    /// which this repository has already shipped once with `--cop-awake`.
    pub fn set_surface_base(&mut self, base: u32) {
        self.surface_base = base;
        self.next_surface = base;
    }
}

impl Bcm {
    /// How wide each of the chip's two windows is: seven registers at a `0x10000` stride, so the
    /// eighth slot closes it.
    pub const WINDOW: u32 = 0x8_0000;
    /// Where RetailOS, Rockbox and disk mode drive the co-processor — the address Rockbox's
    /// `lcd-video.c` documents as `BCM_DATA`.
    pub const HOST_BASE: u32 = 0x3000_0000;
    /// Where Apple's diagnostics drives the same chip. See [`Bcm::alias`].
    pub const DIAG_BASE: u32 = 0xb000_0000;

    /// The offset into the register window, if this address falls in either window the chip is
    /// decoded at.
    pub fn window(&self, addr: u32) -> Option<u32> {
        let hit = |base: u32| {
            let off = addr.wrapping_sub(base);
            (off < Self::WINDOW).then_some(off)
        };
        hit(self.base).or_else(|| self.alias.and_then(hit))
    }

    pub fn new(base: u32) -> Self {
        Bcm {
            base,
            // Not a flag, because it is not a choice: the iPod's co-processor is decoded at both
            // addresses, and a `Bcm` at `HOST_BASE` *is* the iPod's co-processor. Doing it here
            // rather than at each construction site is also what keeps a restored snapshot right —
            // the saved state carries one base, and the pair is written down in exactly one place.
            alias: (base == Self::HOST_BASE).then_some(Self::DIAG_BASE),
            mem: BTreeMap::new(),
            wr_addr: 0,
            rd_addr: 0,
            halfwords_written: 0,
            halfwords_read: 0,
            pending: 0,
            rd_pending: 0,
            registry: false,
            gencmd: Vec::new(),
            gencmd_dropped: 0,
            next_handle: 1,
            next_surface: REG_SURFACE_BASE,
            surface_base: REG_SURFACE_BASE,
            read_hist: BTreeMap::new(),
            latch_log: Capped::new(24),
            timeline: Capped::new(4096),
            run_base: 0,
            run_next: u32::MAX,
            run_len: 0,
            panel: vec![0; PANEL_W * PANEL_H],
            blits_rejected: Capped::new(8),
            commands: Vec::new(),
            frames: 0,
            wr_phase_high: false,
            rd_phase_high: false,
        }
    }

    /// Close the run in progress. Idempotent, and safe to call at the end of a run — the last run
    /// is never terminated by a discontinuity, so a report that did not flush would be one run
    /// short and the missing one would be the most recent, which is usually the interesting one.
    pub fn flush_run(&mut self) {
        if self.run_len > 0 {
            let (base, halfwords) = (self.run_base, self.run_len);
            self.timeline.push(BcmOp::Write { base, halfwords });
            self.run_len = 0;
        }
    }

    fn get32(&self, addr: u32) -> u32 {
        let lo = self.mem.get(&addr).copied().unwrap_or(0) as u32;
        let hi = self.mem.get(&(addr + 2)).copied().unwrap_or(0) as u32;
        lo | (hi << 16)
    }

    fn set32(&mut self, addr: u32, v: u32) {
        self.mem.insert(addr, v as u16);
        self.mem.insert(addr + 2, (v >> 16) as u16);
    }

    /// Execute the pending command. The host writes `0x31` to `CONTROL` to kick one.
    ///
    /// Commands are encoded `BCM_CMD(x) = ((~x << 16) | x)` and the host treats the co-processor as
    /// busy while `BCMA_COMMAND` still reads back the command (or `0xFFFF`), so completion is
    /// signalled by clearing it. Command list from Rockbox `lcd-video.c`: 0 LCD_UPDATE,
    /// 1 SELFTEST, 2 TV_PALBMP, 3 TV_NTSCBMP, 5 LCD_UPDATERECT, 8 LCD_SLEEP, 14 TV_MVOFF.
    fn kick(&mut self) {
        let raw = self.get32(BCMA_COMMAND);
        let cmd = (raw & 0xffff) as u16;
        // A well-formed command has the complement in the high half.
        if raw != 0 && (raw >> 16) as u16 == !cmd {
            self.flush_run();
            self.timeline.push(BcmOp::Command { cmd });
            self.commands.push(cmd);
            match cmd {
                0 => {
                    self.frames += 1;
                    self.lcd_update();
                }
                5 => {
                    self.frames += 1;
                    self.lcd_update_rect();
                }
                _ => {}
            }
        }
        // Report the command consumed.
        self.set32(BCMA_COMMAND, 0);
        self.set32(BCMA_STATUS, 0);
    }

    /// `LCD_UPDATE` — take the whole staged frame into the frame store.
    ///
    /// **Rockbox's authority, not measured here.** Rockbox stages 320x240 halfwords at
    /// `BCMA_CMDPARAM` with no header and issues this command, for both whole-screen and partial
    /// updates (its partial path writes the rows in place and still sends command 0). Apple's
    /// bootloader never sends it — every command in a retail boot is `0x13`, `0xa`, `5`, `5` — so
    /// nothing in this project exercises this arm. It is here because leaving it out would make the
    /// model answer "the panel never changed" to a Rockbox-shaped host, which is a worse lie than a
    /// second-sourced implementation.
    fn lcd_update(&mut self) {
        for i in 0..PANEL_W * PANEL_H {
            self.panel[i] = self
                .mem
                .get(&(BCMA_CMDPARAM + i as u32 * 2))
                .copied()
                .unwrap_or(0);
        }
        self.publish_panel();
    }

    /// `LCD_UPDATERECT` — **the image operation**, derived from what Apple's bootloader stages.
    ///
    /// The host writes eight words at `BCMA_CMDPARAM` and then the rectangle's pixels, in one
    /// contiguous run, and issues command 5. Measured on the retail boot, the second of the two:
    ///
    /// ```text
    /// +0x00 = 0x00000034     unidentified — constant across both commands of a retail boot
    /// +0x04 = 0x00000081     x0 = 129     +0x0c = 0x000000be   x1 = 190   -> 62 wide
    /// +0x08 = 0x00000051     y0 =  81     +0x10 = 0x0000009e   y1 = 158   -> 78 tall
    /// +0x14 = 0, +0x18 = 0
    /// +0x1c = 0x000025c8     9 672 bytes  = 62 * 78 * 2, so the rect and the length agree
    /// ```
    ///
    /// and the 4 836 halfwords that follow are the Apple logo, centred: `(129+190)/2 = 159.5` and
    /// `(81+158)/2 = 119.5`, against a panel centre of `(159.5, 119.5)`.
    ///
    /// The length word is what makes this derived rather than fitted: **`len == w * h * 2` is
    /// checked, not assumed**, so a rect read out of the wrong words would have to agree with a
    /// byte count written by the same firmware, and a rect this model cannot honour is recorded and
    /// skipped rather than smeared across the panel.
    fn lcd_update_rect(&mut self) {
        let hdr: [u32; 8] = std::array::from_fn(|i| self.get32(BCMA_CMDPARAM + i as u32 * 4));
        let (x0, y0, x1, y1, len) = (hdr[1], hdr[2], hdr[3], hdr[4], hdr[7]);
        let (w, h) = (
            x1.wrapping_sub(x0).wrapping_add(1),
            y1.wrapping_sub(y0).wrapping_add(1),
        );
        let sane = x0 <= x1
            && y0 <= y1
            && (x1 as usize) < PANEL_W
            && (y1 as usize) < PANEL_H
            && len == w * h * 2;
        if !sane {
            self.blits_rejected.push(hdr);
            return;
        }
        let src = BCMA_CMDPARAM + 0x20;
        for row in 0..h {
            for col in 0..w {
                let px = self
                    .mem
                    .get(&(src + (row * w + col) * 2))
                    .copied()
                    .unwrap_or(0);
                self.panel[(y0 + row) as usize * PANEL_W + (x0 + col) as usize] = px;
            }
        }
        self.timeline.push(BcmOp::Blit {
            x0,
            y0,
            x1,
            y1,
            src,
        });
        self.publish_panel();
    }

    /// Write the frame store back over `BCMA_CMDPARAM`.
    ///
    /// **This step is the model's, not the co-processor's, and it is the one thing in this file
    /// that a reader should not mistake for hardware.** On the real part the frame store is not
    /// host-addressable at all; the host stages into `BCMA_CMDPARAM` and never reads it back (this
    /// boot reads ten distinct internal offsets and none of them is in the buffer). So there is no
    /// address for `--bcm-dump`, `--bcm-ppm`, `--bcm-film` or the GUI to point at — and rather than
    /// invent one and move every recipe onto it, the model publishes the store at the address every
    /// instrument already reads. The two disagree only between a stage and its command, which is
    /// tens of thousands of instructions, and a sample landing in that window sees the tile
    /// mid-flight rather than a wrong picture.
    ///
    /// With `--bcm-registry` on, RetailOS's own compositor writes straight into this region and
    /// never sends a command, so nothing here runs after the bootloader hands over and every frame
    /// that file measures is untouched by it.
    fn publish_panel(&mut self) {
        for i in 0..PANEL_W * PANEL_H {
            self.mem.insert(BCMA_CMDPARAM + i as u32 * 2, self.panel[i]);
        }
    }

    /// `CONTROL`: not-busy, alive, write-ready, read-ready.
    ///
    /// `0x80` must read *clear* — the host waits for it to drop. An all-ones fill (which this
    /// region used to have, from an era when it was believed to be a panel) is a permanent stall.
    fn control(&self) -> u16 {
        0x52
    }

    fn read16(&mut self, off: u32) -> u16 {
        match off & 0x7_0000 {
            0x0_0000 | 0x4_0000 => {
                let v = self.mem.get(&self.rd_addr).copied().unwrap_or(0);
                *self.read_hist.entry(self.rd_addr).or_insert(0) += 1;
                self.rd_addr = self.rd_addr.wrapping_add(2);
                self.halfwords_read += 1;
                v
            }
            0x1_0000 | 0x5_0000 => 1, // WR_ADDR: bit 0 = ready
            0x2_0000 | 0x6_0000 => 1, // RD_ADDR: bit 0 = data ready
            0x3_0000 | 0x7_0000 => self.control(),
            _ => 0,
        }
    }

    pub(crate) fn write16(&mut self, off: u32, val: u16, _high: bool) {
        match off & 0x7_0000 {
            0x0_0000 | 0x4_0000 => {
                self.mem.insert(self.wr_addr, val);
                let addr = self.wr_addr;
                if addr != self.run_next {
                    self.flush_run();
                    self.run_base = addr;
                    self.run_len = 0;
                }
                self.run_len += 1;
                self.run_next = addr.wrapping_add(2);
                self.wr_addr = self.wr_addr.wrapping_add(2);
                self.halfwords_written += 1;
                self.on_write(addr);
            }
            0x1_0000 | 0x5_0000 => {
                let hi = self.wr_phase_high;
                self.latch_log.push(("wr", off, val, hi));
                self.wr_addr = if hi {
                    (self.wr_addr & 0xffff) | ((val as u32) << 16)
                } else {
                    (self.wr_addr & 0xffff_0000) | val as u32
                };
                self.wr_phase_high = !hi;
            }
            0x2_0000 | 0x6_0000 => {
                let hi = self.rd_phase_high;
                self.latch_log.push(("rd", off, val, hi));
                self.rd_addr = if hi {
                    (self.rd_addr & 0xffff) | ((val as u32) << 16)
                } else {
                    (self.rd_addr & 0xffff_0000) | val as u32
                };
                self.rd_phase_high = !hi;
            }
            0x3_0000 | 0x7_0000 if val == 0x31 => {
                self.kick();
            }
            _ => {}
        }
    }

    /// Stand in for the co-processor's own firmware reacting to a host write.
    ///
    /// The bootstrap sequence ends `0x10000400 = 0xA5A50002`, then **waits for `BCMA_COMMAND` to
    /// become non-zero** — that is the BCM's running firmware acknowledging. Nothing in a passive
    /// memory model ever sets it, so the host waits forever. Here the acknowledgement is
    /// synthesised at the point the trigger word is written.
    fn on_write(&mut self, addr: u32) {
        // 0x10000c00 |= 1 : the host polls bit 0 after writing 0xC0000000.
        if addr & !2 == 0x1000_0c00 {
            let cur = self.mem.get(&0x1000_0c00).copied().unwrap_or(0);
            self.mem.insert(0x1000_0c00, cur | 1);
        }
        if addr & !2 == 0x1000_0400 {
            self.mem.insert(BCMA_COMMAND, 1);
            self.mem.insert(BCMA_STATUS, 1);
            if self.registry {
                self.publish_registry();
            }
        }
        // The host pushing its ring write pointer is the doorbell — `FUN_00288800` writes the
        // 16-byte block at record `+0x20`, and the first halfword of it IS the pointer.
        if self.registry && addr == REG_BASE + REG_REC2 + 0x20 {
            self.gencmd_pump();
        }
    }

    /// Publish the service directory the firmware would have published once it was running.
    ///
    /// Layout is `FUN_00288058`'s + `FUN_00286aa8`'s + `FUN_002882c0`'s, in that order: the
    /// `0x1f0` header, the eight `u16` slots, and one 0x50-byte record for the tag-2 service.
    /// Stand in for the whole bootstrap a warm entry skips, not merely its directory.
    ///
    /// `on_write(0x10000400)` does **three** things: acknowledges the co-processor's firmware by
    /// setting `BCMA_COMMAND` and `BCMA_STATUS`, and publishes the channel directory. Replicating
    /// only the third is what the first attempt at this did, and it left RetailOS with a
    /// directory it could find and a co-processor that had never said it was running.
    ///
    /// Named for what it stands in for rather than what it writes, because the next thing found
    /// missing belongs in here too.
    pub fn bootstrap_for_warm_entry(&mut self) {
        self.mem.insert(BCMA_COMMAND, 1);
        self.mem.insert(BCMA_STATUS, 1);
        if self.registry {
            self.publish_registry();
        }
    }

    /// Publish the channel directory the host reads at internal `0x1f0`.
    ///
    /// `pub` because a **warm entry has to call it directly**. It normally runs from
    /// `on_write(0x10000400)`, the last write of the bootstrap sequence — Apple's bootloader
    /// bringing the co-processor up. A boot that starts at `0x10000000` is after that sequence,
    /// so the trigger never fires, the directory is never published, and RetailOS finds no
    /// channel to send a display RPC over. That is why a warm boot draws nothing.
    pub fn publish_registry(&mut self) {
        self.set32(BCMA_COMMAND, 1); // "firmware up" — FUN_00288058 requires exactly 1
        self.set32(BCMA_STATUS, REG_BASE); // the directory pointer; non-zero, 4-aligned
        for i in 0..8u32 {
            self.mem
                .insert(REG_BASE + i * 2, if i == 0 { REG_REC2 as u16 } else { 0 });
        }
        let r = REG_BASE + REG_REC2;
        self.set32(r, 0); //        +0x00  read by every scanner, examined by none
        self.mem.insert(r + 4, 2); //      +0x04  the tag — 2 is the display service
        self.mem.insert(r + 6, REG_TX_LO as u16); //   +0x06  TX ring start
        self.mem.insert(r + 8, REG_TX_HI as u16); //   +0x08  TX ring end
        self.mem.insert(r + 0xa, REG_RX_LO as u16); // +0x0a  RX ring start
        self.mem.insert(r + 0xc, REG_RX_HI as u16); // +0x0c  RX ring end
        self.mem.insert(r + 0xe, 0); //                +0x0e  unread
        self.mem.insert(r + 0x10, REG_TX_LO as u16); // TX read  — ours, host polls it
        self.mem.insert(r + 0x20, REG_TX_LO as u16); // TX write — host's, it pushes it to us
        self.mem.insert(r + 0x30, REG_RX_LO as u16); // RX read  — host's
        self.mem.insert(r + 0x40, REG_RX_LO as u16); // RX write — ours
    }

    /// Read `n` bytes out of a ring, wrapping at `hi` back to `lo`. `p` is an offset from
    /// `REG_BASE`, which is how RetailOS stores it (`FUN_0028871c` writes to `wr + *base`).
    fn ring_read(&self, mut p: u32, lo: u32, hi: u32, n: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(n);
        while v.len() < n {
            if p >= hi {
                p = lo;
            }
            let h = self.mem.get(&(REG_BASE + p)).copied().unwrap_or(0);
            v.push(h as u8);
            v.push((h >> 8) as u8);
            p += 2;
        }
        v.truncate(n);
        v
    }

    /// Drain every complete request the host has pushed, and answer each one.
    fn gencmd_pump(&mut self) {
        let r = REG_BASE + REG_REC2;
        let g = |m: &BTreeMap<u32, u16>, a: u32| m.get(&a).copied().unwrap_or(0) as u32;
        loop {
            let rd = g(&self.mem, r + 0x10);
            let wr = g(&self.mem, r + 0x20);
            let used = ring_used(REG_TX_LO, REG_TX_HI, rd, wr);
            if used < 0x10 {
                break;
            }
            let hdr = self.ring_read(rd, REG_TX_LO, REG_TX_HI, 0x10);
            let w = |b: &[u8], i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
            if w(&hdr, 0) != GENCMD_MAGIC {
                // Desynchronised. Stop rather than invent a framing that was never derived.
                self.gencmd_dropped += 1;
                break;
            }
            let (seq, op) = (w(&hdr, 4), w(&hdr, 8));
            let len = u16::from_le_bytes([hdr[12], hdr[13]]) as u32;
            let plen = (len + 0xf) & !0xf; // FUN_0028861c pads the payload to 16
            if used < 0x10 + plen {
                break; // the rest has not arrived yet
            }
            let mut p = rd + 0x10;
            if p >= REG_TX_HI {
                p = REG_TX_LO + (p - REG_TX_HI);
            }
            let pay = self.ring_read(p, REG_TX_LO, REG_TX_HI, plen as usize);
            let mut nrd = rd + 0x10 + plen;
            while nrd >= REG_TX_HI {
                nrd -= REG_TX_HI - REG_TX_LO;
            }
            self.mem.insert(r + 0x10, nrd as u16);
            self.gencmd.push((op, len));
            self.reply(op, seq, &pay);
        }
    }

    /// Build the 16-byte header + 16-byte payload every caller reads back.
    ///
    /// Six independent call sites — opcodes 1, 2, 3, 4, 9 and 0x10 — read exactly `0x20` bytes and
    /// take the word at `+0x10`. Opcode 8 additionally takes `+0x14`. So the reply is one header
    /// plus one 16-byte payload whose first word is the result; the rest is never examined.
    fn reply(&mut self, op: u32, seq: u32, pay: &[u8]) {
        let mut m = [0u8; 0x20];
        m[0..4].copy_from_slice(&GENCMD_MAGIC.to_le_bytes());
        m[4..8].copy_from_slice(&seq.to_le_bytes());
        m[8..12].copy_from_slice(&op.to_le_bytes());
        m[12..14].copy_from_slice(&0x10u16.to_le_bytes());
        let h = self.next_handle;
        self.next_handle += 1;
        m[0x10..0x14].copy_from_slice(&h.to_le_bytes());
        if op == 8 && pay.len() >= 0x20 {
            // FUN_00286ca8's payload, from FUN_00286a1c's descriptor: +0x04 type (u8),
            // +0x08 width, +0x0c height, +0x10 pitch, +0x18 co-processor address (0 = allocate).
            let w = |i: usize| u32::from_le_bytes(pay[i..i + 4].try_into().unwrap());
            let (height, pitch) = (w(0x0c), w(0x10));
            let mut addr = w(0x18);
            if addr == 0 {
                addr = self.next_surface;
                self.next_surface += (height.saturating_mul(pitch) + 0xfff) & !0xfff;
            }
            m[0x14..0x18].copy_from_slice(&addr.to_le_bytes());
        }
        // Append to the reply ring, if it fits. Dropping is visible; corrupting is not.
        let r = REG_BASE + REG_REC2;
        let g = |mm: &BTreeMap<u32, u16>, a: u32| mm.get(&a).copied().unwrap_or(0) as u32;
        let (rd, wr) = (g(&self.mem, r + 0x30), g(&self.mem, r + 0x40));
        let free = (REG_RX_HI - REG_RX_LO) - 0x10 - ring_used(REG_RX_LO, REG_RX_HI, rd, wr);
        if (m.len() as u32) > free {
            self.gencmd_dropped += 1;
            return;
        }
        let mut p = wr;
        for pair in m.chunks(2) {
            if p >= REG_RX_HI {
                p = REG_RX_LO;
            }
            self.mem
                .insert(REG_BASE + p, pair[0] as u16 | ((pair[1] as u16) << 8));
            p += 2;
        }
        if p >= REG_RX_HI {
            p = REG_RX_LO;
        }
        self.mem.insert(r + 0x40, p as u16);
    }

    /// Byte-level bus access, since the interpreter decomposes `ldrh`/`strh` into byte accesses.
    /// Latches and phase, for a snapshot. `mem` is saved separately because it is sparse.
    pub fn save_scalars(&self) -> [u32; 4] {
        [
            self.wr_addr,
            self.rd_addr,
            self.wr_phase_high as u32,
            self.rd_phase_high as u32,
        ]
    }

    pub fn load_scalars(&mut self, v: [u32; 4]) {
        self.wr_addr = v[0];
        self.rd_addr = v[1];
        self.wr_phase_high = v[2] != 0;
        self.rd_phase_high = v[3] != 0;
    }

    pub fn read8(&mut self, off: u32) -> u8 {
        // The data port is a FIFO: every access advances `rd_addr`. The interpreter decomposes one
        // `ldrh` into two byte reads, so calling `read16` for both halves consumed TWO internal
        // halfwords per halfword the host asked for, and spliced the low byte of one with the high
        // byte of the next. Measured: RetailOS's 16-byte read at internal `0x1f0` drained
        // `0x1f0..0x20f` and handed word 2 back as `0x2f01fc78` — byte-exactly
        // `(mem[0x206]>>8)<<24 | (mem[0x204]&0xff)<<16 | (mem[0x202]>>8)<<8 | (mem[0x200]&0xff)`.
        // Buffer the pair, exactly as `write8` already does for the other direction.
        if off & 1 == 0 {
            let v = self.read16(off & !1);
            self.rd_pending = v;
            v as u8
        } else {
            (self.rd_pending >> 8) as u8
        }
    }

    pub fn write8(&mut self, off: u32, val: u8) {
        // Halfword writes arrive low byte first; buffer the pair rather than acting on each byte.
        if off & 1 == 0 {
            self.pending = val;
        } else {
            let w = ((val as u16) << 8) | self.pending as u16;
            self.write16(off & !1, w, off & 2 != 0);
        }
    }
}
