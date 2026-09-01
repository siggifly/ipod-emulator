//! The drive: LBA addressing, DMA, and the interrupt latch.

use crate::*;

pub struct Ata {
    file: std::fs::File,
    pub sectors: u64,
    features: u8,
    nsector: u8,
    sector: u8,
    lcyl: u8,
    hcyl: u8,
    select: u8,
    status: u8,
    error: u8,
    buf: Vec<u8>,
    pos: usize,
    remaining: u32,
    next_lba: u64,
    /// `(command, features, nsector, lba)` per command — the whole request, not just its opcode.
    ///
    /// **Capped at 256 entries, and [`Capped`] makes the cap say so.** This is a sample, not a
    /// census: `commands.seen()` is how many commands actually issued. The two were conflated for
    /// months — `trace.rs` printed `commands.len()` under the label "ata commands", so every run
    /// past the cap reported exactly 256 and the number was quoted as a measurement in research/ and
    /// used as this project's baseline verification. The real figure at 600 M is 671. It also
    /// manufactured a false absence: the truncation audit concluded LBA 22169 is never read, when it
    /// is read at command #342 — past the cap, invisible to the log. See research/10 Addendum 15 §3.
    ///
    /// This was the first instrument taught to announce its own saturation, and its report line is
    /// the wording every other one now copies.
    pub commands: Capped<(u8, u8, u8, u64)>,
    /// `command byte -> count`, **uncapped**. The list above is a sample and says so; this is the
    /// census. Without it "did the guest ever issue IDENTIFY?" is answerable only by grepping the
    /// first 256 entries, which is how this project briefly published that a Linux kernel never
    /// issues one — from a log that announces itself as a sample on the line above.
    pub cmd_census: BTreeMap<u8, u64>,
    /// Bitmask of the multiword / ultra DMA mode SET FEATURES last selected, for IDENTIFY words
    /// 63 and 88 to report back.
    mwdma_selected: u8,
    udma_selected: u8,
    /// The PortalPlayer IDE controller's own registers at `IDE_BASE + 0x00..0xff` — timings and
    /// `IDE0_CFG` at `+0x28`. These are the *controller's*, distinct from the ATA taskfile at
    /// `+0x1e0`, and the firmware round-trips them. Returning zero for the block left the boot
    /// polling `IDE0_CFG` bit 3 forever after issuing a read.
    cfg: [u8; 0x100],
    /// `(offset, value)` for controller-register writes, capped. Apple's bootloader transfers by
    /// DMA and Rockbox's PP driver is PIO-only, so there is no published description of how the
    /// descriptor is programmed — it has to be read off the firmware doing it.
    ///
    /// The ordered head is what that reading needs; the per-register totals come from
    /// `cfg_writes_by_reg`, which is uncapped. They used to be a tally of this log.
    pub cfg_writes: Capped<(u32, u8)>,
    /// 32-bit register -> byte-writes, **uncapped**.
    pub cfg_writes_by_reg: BTreeMap<u32, u64>,
    /// Reads per offset in the controller window. Counted rather than logged: a capped log
    /// reported "50 reads of ATA_DATA" when it had simply filled up, which is the same saturation
    /// trap the unmapped log once had.
    pub reads_log: BTreeMap<u32, u64>,
    /// `(offset, byte)` for the first reads of the data window after the most recent IDENTIFY.
    ///
    /// The question this answers cannot be answered any other way: a guest that gets its IDENTIFY
    /// six bytes out of step looks identical, from every other instrument, to one that got it
    /// right — same command count, same byte total, same buffer. What differs is *which* byte came
    /// back first, and nothing was recording that.
    /// A completion raised from inside a data-port READ, which the write path cannot see.
    ///
    /// ATA asserts INTRQ at the start of every data block of a PIO-in transfer, not only the first.
    /// A multi-sector read loads its second and later sectors while the guest is *reading* the data
    /// port, and this machine only ever armed the interrupt controller on a *write* to the ATA
    /// window — so every block after the first completed silently. Apple's bootloader never noticed
    /// because it polls the latch with interrupts masked; Linux waits on the interrupt, and reports
    /// exactly what happened: `hda: lost interrupt`.
    pub(crate) pio_block_ready: bool,
    /// What INITIALIZE DEVICE PARAMETERS last set, as (heads, sectors per track).
    current_geometry: Option<(u16, u16)>,
    pub id_handover: Vec<(u32, u8)>,
    id_watch: bool,
    /// Bytes actually handed over through the data register.
    pub bytes_read: u64,
    /// The controller's own interrupt-pending latch, reported in `IDE0_CFG`.
    ///
    /// iPodLinux's `ipodloader2/ata2.c` clears it by writing `0x20`/`0x30` to `0xc3000028`
    /// ("this hopefully clears all pending intrs"), which is what identifies the register as
    /// interrupt status rather than a readiness flag. Apple's bootloader polls it instead of
    /// taking the IRQ — it runs with interrupts masked.
    pub(crate) irq_pending: bool,
    /// The bus-master DMA engine at `IDE_BASE + 0x400..0x410`, as bytes so 32-bit stores assemble
    /// naturally. Read off Apple's own bootloader programming it at `0x4000bb04`, because Rockbox's
    /// PP driver is PIO-only and no published map describes this block:
    ///
    /// ```text
    /// +0x400  CONTROL   bit 0 = GO, bit 1 = arm, bit 3 = direction (set = read into memory)
    /// +0x408  LENGTH    transfer size in bytes, minus 4
    /// +0x40c  ADDRESS   destination
    /// ```
    ///
    /// `IDE0_CFG` bit 15 enables the completion interrupt; the driver sets it alongside.
    dma: [u8; 0x10],
    /// GO has been written and the engine is waiting for data. Kept as state rather than acted on
    /// inline because **the two events arrive in either order**: the ROM writes GO after the ATA
    /// command, RetailOS writes it ~130 instructions before. Treating GO as the sole trigger
    /// modelled only the ROM's order and silently dropped every transfer armed the other way.
    dma_armed: bool,
    /// Sectors fetched by a READ DMA command, waiting for the engine to be armed. Real hardware has
    /// the drive stream them to the engine; staging is the same thing observed from outside.
    dma_staged: Vec<u8>,
    /// The LBA the staged sectors came from.
    dma_lba: u64,
    /// Handed to `Memory` to commit, because the device cannot reach the regions from in here.
    pub dma_ready: Option<(u32, Vec<u8>)>,
    /// Whether the backing image was opened for writing. A write command to a read-only image
    /// aborts, which is what a write-protected drive does and what lets the driver's own error
    /// path run rather than silently succeeding.
    pub writable: bool,
    /// LBA a WRITE command is staging for, set by `command()` and consumed by the DMA kick.
    write_lba: Option<u64>,
    /// The mirror of `dma_ready` for the outbound direction: `(source address, length, lba)`.
    /// `Ata` cannot reach `Memory`, so the bus picks this up, fetches the bytes and hands them back.
    pub dma_fetch: Option<(u32, u32, u64)>,
    pub sectors_written: u64,
    /// A PIO WRITE is in progress and the data register is inbound.
    write_pio: bool,
    /// `(source LBA, destination, bytes)` per completed transfer. The LBA is in the log because
    /// destination alone cannot show whether the driver is walking the image contiguously.
    pub dma_transfers: Vec<(u64, u32, u32)>,
}

/// One PP502x DMA controller: a master block, a channel array 0x1000 above it, and one
/// completion line into the interrupt controller's first bank.
///
/// The **second** row is the one Rockbox names — `DMA_MASTER_CONTROL 0x6000a000`,
/// `DMA0_BASE_ADDR 0x6000b000` stepping by 0x20, `DMA_IRQ 26`. The **first** row is not in any
/// published map; it is read off RetailOS's own driver, which constructs both from one object at
/// `0x001da160`: `[this+0x20] = 0x60008000` with a two-iteration channel loop (`cmp r5, #2` at
/// `0x001da214`) and `[this+0x30] = 0x6000a000` with a four-iteration one (`cmp r5, #4` at
/// `0x001da308`). Both loops compute the channel base identically — `base + 0x1000 + n*0x20`
/// (`ldr r1,[r4,#0x20]; add r1,r1,r5,lsl #5; add r9,r1,#0x1000`) — and then clear bit 31 of
/// `+0x00`, which is Rockbox's `DMA_CMD_START`. Same registers, same bits, two instances.
pub struct PpDmaCtl {
    pub master: u32,
    pub chans: u32,
    pub n: u32,
    pub irq: u32,
}

impl Ata {
    /// Scalar state for a snapshot. The backing file is deliberately excluded — it is reopened by
    /// path, so a snapshot is only valid against the same disk image.
    pub fn save(&self) -> Vec<u32> {
        let mut v = vec![
            self.features as u32,
            self.nsector as u32,
            self.sector as u32,
            self.lcyl as u32,
            self.hcyl as u32,
            self.select as u32,
            self.status as u32,
            self.error as u32,
            self.pos as u32,
            self.remaining,
            self.next_lba as u32,
            (self.next_lba >> 32) as u32,
            self.irq_pending as u32,
            self.buf.len() as u32,
        ];
        v.extend(self.buf.iter().map(|b| *b as u32));
        v.extend(self.cfg.iter().map(|b| *b as u32));
        v
    }

    pub fn load(&mut self, v: &[u32]) -> bool {
        if v.len() < 14 {
            return false;
        }
        let g = |i: usize| v[i];
        self.features = g(0) as u8;
        self.nsector = g(1) as u8;
        self.sector = g(2) as u8;
        self.lcyl = g(3) as u8;
        self.hcyl = g(4) as u8;
        self.select = g(5) as u8;
        self.status = g(6) as u8;
        self.error = g(7) as u8;
        self.pos = g(8) as usize;
        self.remaining = g(9);
        self.next_lba = g(10) as u64 | ((g(11) as u64) << 32);
        self.irq_pending = g(12) != 0;
        let bl = g(13) as usize;
        if v.len() < 14 + bl + 0x100 {
            return false;
        }
        self.buf = v[14..14 + bl].iter().map(|x| *x as u8).collect();
        for (i, x) in v[14 + bl..14 + bl + 0x100].iter().enumerate() {
            self.cfg[i] = *x as u8;
        }
        true
    }

    /// Open the backing image. `writable` is opt-in and defaults off at every call site, because
    /// the alternative is an emulator bug quietly rewriting the one disk image this project has.
    pub fn open(path: &std::path::Path, writable: bool) -> std::io::Result<Self> {
        let file = if writable {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)?
        } else {
            std::fs::File::open(path)?
        };
        let sectors = file.metadata()?.len() / 512;
        Ok(Ata {
            file,
            sectors,
            features: 0,
            nsector: 0,
            sector: 0,
            lcyl: 0,
            hcyl: 0,
            select: 0,
            // Idle and ready, with no medium error — what a spun-up drive reports.
            status: ATA_DRDY | ATA_DSC,
            error: 0,
            buf: Vec::new(),
            pos: 0,
            remaining: 0,
            next_lba: 0,
            commands: Capped::new(256),
            cmd_census: BTreeMap::new(),
            mwdma_selected: 0,
            udma_selected: 0,
            pio_block_ready: false,
            current_geometry: None,
            cfg: [0; 0x100],
            cfg_writes: Capped::new(512),
            cfg_writes_by_reg: BTreeMap::new(),
            reads_log: BTreeMap::new(),
            id_handover: Vec::new(),
            id_watch: false,
            bytes_read: 0,
            irq_pending: false,
            dma: [0; 0x10],
            dma_armed: false,
            dma_staged: Vec::new(),
            dma_lba: 0,
            dma_ready: None,
            dma_transfers: Vec::new(),
            writable,
            write_lba: None,
            dma_fetch: None,
            sectors_written: 0,
            write_pio: false,
        })
    }

    /// The 512-byte IDENTIFY DEVICE response for this drive.
    fn identify(&self) -> Vec<u8> {
        Ata::identify_sector_with(
            self.sectors,
            self.mwdma_selected,
            self.udma_selected,
            self.current_geometry,
        )
    }

    /// The 512-byte IDENTIFY DEVICE response. Only the fields a driver actually consults are
    /// filled; everything else stays zero, which is legal and keeps the intent readable.
    ///
    /// Takes its three inputs rather than `self` because that is all it depends on, and a method on
    /// a struct that owns an open file cannot be asserted about without conjuring a disk. This is
    /// the first thing every driver reads and the last thing anyone thinks to check.
    pub fn identify_sector(sectors: u64, mwdma_selected: u8, udma_selected: u8) -> Vec<u8> {
        Ata::identify_sector_with(sectors, mwdma_selected, udma_selected, None)
    }

    /// `current` is the geometry INITIALIZE DEVICE PARAMETERS last set, as (heads, sectors).
    pub fn identify_sector_with(
        sectors: u64,
        mwdma_selected: u8,
        udma_selected: u8,
        current: Option<(u16, u16)>,
    ) -> Vec<u8> {
        let mut w = [0u16; 256];
        w[0] = 0x0040; // non-removable, fixed device
        w[1] = 16383; // logical cylinders (legacy CHS, ignored once LBA is on)
        w[3] = 16; // heads
        w[6] = 63; // sectors per track
        put_ata_str(&mut w[10..20], "IPODEMU0000000000001"); // serial
        put_ata_str(&mut w[23..27], "1.00");
        put_ata_str(&mut w[27..47], "Emulated iPod Disk");
        w[47] = 0x8001; // max sectors per READ MULTIPLE
        w[49] = 0x0200; // LBA supported
        w[51] = 0x0200;
        w[53] = 0x0007; // words 54-58, 64-70, 88 are valid
                        // **And they have to actually be there, because bit 0 of word 53 says they are.**
                        //
                        // These were left zero while word 53 advertised them, which is the same defect shape as a
                        // config option with no mechanism behind it: a driver that believes the validity bit reads
                        // a geometry of nothing. Linux's `ide` driver is the one that does — 2.4.32-ipod2 prints
                        // `INVALID GEOMETRY: 63 PHYSICAL HEADS?` and then fails every read of sectors 0, 2, 4 and 6,
                        // which is the MBR, which is why it cannot find a partition table on a disk whose partition
                        // table three other firmwares here read without complaint.
                        //
                        // **The current geometry is whatever INITIALIZE DEVICE PARAMETERS last set.**
                        //
                        // This used to read "nothing ever issues INITIALIZE DEVICE PARAMETERS to this drive, so
                        // there is no second answer to keep in step" — true only for as long as the one firmware
                        // that sends it could not get far enough to send it. iPodLinux issues `0x91` as soon as it
                        // can read its own IDENTIFY, and a drive that answers the command and then reports the old
                        // geometry back is contradicting itself.
                        // With no such command issued the current geometry IS the default one, cylinder count
                        // included — 16383 is the legacy ceiling word 1 reports, not a figure derived from capacity.
        let (cur_heads, cur_sectors) = current.unwrap_or((w[3], w[6]));
        // Once the host has chosen heads and sectors, cylinders follow from them: the drive divides
        // its capacity by the translation it was given. Keeping the default cylinder count against
        // host-chosen heads and sectors would describe a disk of a different size.
        let cur_cyls = match current {
            None => w[1],
            Some(_) => {
                (sectors / (cur_heads.max(1) as u64 * cur_sectors.max(1) as u64)).min(65535) as u16
            }
        };
        w[54] = cur_cyls; // current cylinders
        w[55] = cur_heads; // current heads
        w[56] = cur_sectors; // current sectors per track
                             // Current capacity in sectors, and it is NOT the disk's size: CHS addressing tops out at
                             // 16383*16*63, so a drive larger than that reports the ceiling here and the true figure in
                             // words 60/61. Reporting the LBA size in a CHS field is how you get a geometry that
                             // multiplies out to more sectors than the heads/sectors fields can reach.
        let chs_capacity = w[54] as u32 * w[55] as u32 * w[56] as u32;
        w[57] = (chs_capacity & 0xffff) as u16;
        w[58] = (chs_capacity >> 16) as u16;
        w[60] = (sectors & 0xffff) as u16; // LBA28 capacity, low
        w[61] = ((sectors >> 16) & 0xffff) as u16; // ...and high
                                                   // Transfer modes. Word 53 above claims words 64-70 and 88 are valid, so leaving them zero
                                                   // was a drive that advertises no DMA capability at all while answering SET FEATURES
                                                   // "transfer mode = Multiword DMA 2" with success — which is not a drive that exists.
                                                   //
                                                   // Low byte = modes supported, high byte = mode currently selected. The selected bits are
                                                   // the standard way a driver confirms the mode it just asked for actually took.
        w[62] = 0x0000; // single-word DMA: obsolete since ATA-3, correctly absent
        w[63] = 0x0007 | ((mwdma_selected as u16) << 8); // multiword DMA 0-2 supported
        w[64] = 0x0003; // PIO modes 3 and 4
        w[65] = 120; // minimum multiword DMA cycle time, ns
        w[66] = 120; // recommended
        w[67] = 120; // minimum PIO cycle time without IORDY
        w[68] = 120; // ...with IORDY
        w[88] = 0x001f | ((udma_selected as u16) << 8); // ultra DMA 0-4 supported
        w[80] = 0x0070; // ATA/ATAPI-4,5,6
        w[82] = 0x0000;
        w[83] = 0x4000; // word 83 valid
        w[84] = 0x4000;
        w[86] = 0x0000;
        w[87] = 0x4000;
        let mut out = Vec::with_capacity(512);
        for x in w {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out
    }

    fn lba(&self) -> u64 {
        (((self.select & 0x0f) as u64) << 24)
            | ((self.hcyl as u64) << 16)
            | ((self.lcyl as u64) << 8)
            | self.sector as u64
    }

    /// `count` sectors from `lba`, or empty if any of them is past the end of the image. Partial
    /// success would be worse than failure: the driver would checksum a half-filled buffer.
    fn read_sectors(&mut self, lba: u64, count: u32) -> Vec<u8> {
        use std::io::{Read, Seek, SeekFrom};
        let mut b = vec![0u8; count as usize * 512];
        match self
            .file
            .seek(SeekFrom::Start(lba.saturating_mul(512)))
            .and_then(|_| self.file.read_exact(&mut b))
        {
            Ok(()) => b,
            Err(_) => Vec::new(),
        }
    }

    /// A transfer needs two things: the engine armed (GO) and data to move (an ATA command). They
    /// arrive in either order, so this runs on both edges and does nothing until both have landed.
    ///
    /// The single-trigger version fired only on GO. That dropped RetailOS's 32 KB read outright,
    /// and — worse, because it looked like success — left its 32 KB staged so the *next* arm
    /// committed the stale buffer to the next transfer's address, truncated to the next transfer's
    /// length. That cross-commit read as a working transfer for months: both of RetailOS's reads
    /// are LBA 0, so the wrong buffer held the right bytes by coincidence.
    fn dma_try_start(&mut self) {
        if !self.dma_armed {
            return;
        }
        let word = |o: usize| u32::from_le_bytes(self.dma[o..o + 4].try_into().unwrap());
        if let Some(lba) = self.write_lba.take() {
            let src = word(0x0c);
            let len = word(0x08).wrapping_add(4);
            self.dma_fetch = Some((src, len, lba));
            self.dma_armed = false;
            return;
        }
        if self.dma_staged.is_empty() {
            return;
        }
        self.dma_armed = false;
        let dest = word(0x0c);
        // The engine is programmed with length-minus-four; undo that rather than transferring four
        // bytes short of every image.
        let len = word(0x08).wrapping_add(4) as usize;
        let mut data = std::mem::take(&mut self.dma_staged);
        data.truncate(len.min(data.len()));
        self.dma_transfers
            .push((self.dma_lba, dest, data.len() as u32));
        self.dma_ready = Some((dest, data));
        self.status = ATA_DRDY | ATA_DSC;
        self.irq_pending = true;
    }

    /// Write bytes fetched from memory to the backing image, and complete the command.
    pub fn commit_write(&mut self, lba: u64, data: &[u8]) {
        use std::io::{Seek, SeekFrom, Write};
        let ok = self
            .file
            .seek(SeekFrom::Start(lba.saturating_mul(512)))
            .and_then(|_| self.file.write_all(data))
            .is_ok();
        if ok {
            self.sectors_written += (data.len() / 512) as u64;
            self.status = ATA_DRDY | ATA_DSC;
        } else {
            self.status = ATA_DRDY | ATA_DSC | ATA_ERR;
            self.error = 0x40;
        }
        self.irq_pending = true;
    }

    fn load_sector(&mut self) {
        use std::io::{Read, Seek, SeekFrom};
        let mut b = vec![0u8; 512];
        let off = self.next_lba.saturating_mul(512);
        let ok = self
            .file
            .seek(SeekFrom::Start(off))
            .and_then(|_| self.file.read_exact(&mut b))
            .is_ok();
        if ok {
            self.buf = b;
            self.pos = 0;
            self.status = ATA_DRDY | ATA_DSC | ATA_DRQ;
            self.irq_pending = true;
            self.pio_block_ready = true;
            self.next_lba += 1;
        } else {
            // A read past the end of the image is a real error, and reporting it as one is what
            // lets the driver's own error path run instead of silently consuming zeroes.
            self.buf.clear();
            self.status = ATA_DRDY | ATA_DSC | ATA_ERR;
            self.error = 0x40; // uncorrectable data error
            self.remaining = 0;
        }
    }

    fn command(&mut self, cmd: u8) {
        {
            self.commands
                .push((cmd, self.features, self.nsector, self.lba()));
        }
        // Uncapped, because the sample above is capped at 256 and a capped log is how this project
        // once published "LBA 22169 is never read" about a sector read at command #342. Whether the
        // firmware ever WRITES is exactly the kind of question a truncated sample answers wrongly
        // and confidently.
        // Device 1 is absent: nothing latches the command, nothing completes, nothing interrupts.
        if self.select & 0x10 != 0 {
            return;
        }
        *self.cmd_census.entry(cmd).or_default() += 1;
        self.error = 0;
        match cmd {
            0xec => {
                // IDENTIFY DEVICE
                self.buf = self.identify();
                self.pos = 0;
                // Watch the hand-over of THIS response; the last one issued is the one that matters.
                self.id_handover.clear();
                self.id_watch = true;
                self.remaining = 0;
                self.status = ATA_DRDY | ATA_DSC | ATA_DRQ;
                self.irq_pending = true;
            }
            // READ DMA. The bootloader reads the firmware directory by PIO, re-initialises the
            // controller, then switches to DMA for the image load itself — so this is the command
            // that actually matters for the handoff.
            //
            // The data goes to memory, never through the data register, so DRQ must stay CLEAR.
            // Asserting it (as the earlier PIO-shaped stand-in did) left the drive looking
            // permanently mid-transfer: the driver's next ready-check at `0x4000b700` requires
            // DRDY or ERR, saw DRQ instead, and returned error `0x58` — which is what stopped the
            // `osos` load. Nothing is committed until the GO bit arrives.
            0xc8 | 0xc9 | 0x25 => {
                let n = if self.nsector == 0 {
                    256
                } else {
                    self.nsector as u32
                };
                self.next_lba = self.lba();
                self.dma_staged = self.read_sectors(self.next_lba, n);
                self.dma_lba = self.next_lba;
                self.remaining = 0;
                self.status = if self.dma_staged.is_empty() {
                    self.error = 0x40; // uncorrectable data error
                    ATA_DRDY | ATA_DSC | ATA_ERR
                } else {
                    ATA_DRDY | ATA_DSC
                };
                // The other half of the pair. If the driver armed the engine before issuing the
                // command, this is the edge that starts the transfer.
                self.dma_try_start();
            }
            0x20 | 0x21 | 0xc4 => {
                // READ SECTOR(S) / READ MULTIPLE
                self.remaining = if self.nsector == 0 {
                    256
                } else {
                    self.nsector as u32
                };
                self.next_lba = self.lba();
                self.load_sector();
                self.remaining = self.remaining.saturating_sub(1);
            }
            // WRITE DMA. Stages the LBA; the bytes are fetched once the engine is armed, because
            // only the bus can read them out of memory. Either-order applies here too.
            0xca | 0x35 => {
                if self.writable {
                    self.write_lba = Some(self.lba());
                    self.dma_lba = self.lba();
                    self.status = ATA_DRDY | ATA_DSC;
                    self.dma_try_start();
                } else {
                    self.status = ATA_DRDY | ATA_DSC | ATA_ERR;
                    self.error = 0x04; // ABRT — a write-protected drive
                                       // A real drive asserts INTRQ when it clears BSY, and it does that whether the
                                       // command succeeded or aborted — refusing is a *completion*, not a silence.
                                       // Without this the driver is told nothing at all: RetailOS blocked on RTXC
                                       // semaphore 0xd1 waiting for this exact command (a 1-sector WRITE DMA to
                                       // LBA 32894, the first sector of FAT #1) and only its own 3.9 s timeout ever
                                       // ended the wait, 21 times over. See research/10 Addendum 15.
                    self.irq_pending = true;
                }
            }
            // WRITE SECTOR(S) / WRITE MULTIPLE — PIO, the driver feeds the data register.
            0x30 | 0x31 | 0xc5 => {
                if self.writable {
                    self.remaining = if self.nsector == 0 {
                        256
                    } else {
                        self.nsector as u32
                    };
                    self.next_lba = self.lba();
                    self.buf = vec![0u8; 512];
                    self.pos = 0;
                    self.write_pio = true;
                    self.status = ATA_DRDY | ATA_DSC | ATA_DRQ;
                } else {
                    self.status = ATA_DRDY | ATA_DSC | ATA_ERR;
                    self.error = 0x04;
                    self.irq_pending = true; // same as WRITE DMA above — an abort still interrupts
                }
            }
            // INITIALIZE DEVICE PARAMETERS. The host picks a CHS translation: heads come from
            // the low nibble of the drive/head register as a zero-based count, sectors per track
            // from the sector-count register. Accepting it and then reporting the old geometry in
            // IDENTIFY is a drive disagreeing with itself, so the answer moves with the command.
            0x91 => {
                self.current_geometry =
                    Some(((self.select & 0x0f) as u16 + 1, self.nsector as u16));
                self.status = ATA_DRDY | ATA_DSC;
                self.irq_pending = true;
            }
            // RECALIBRATE — obsolete since ATA-4 and still what Linux issues when it is trying to
            // recover a drive after a timeout. Aborting it turns a recoverable stall into
            // `DriveStatusError`, which is a worse report than the one the drive should have given.
            // **The power-management family, which a real drive answers and we were aborting.**
            //
            // `0xe0` STANDBY IMMEDIATE was already handled; its siblings were not, so a driver that
            // spins the disk down or asks what mode it is in got ABRT — and Linux, which issues
            // `0xe3` IDLE once its root filesystem is up, reported `DriveStatusError` for a command
            // every ATA drive since ATA-1 has accepted.
            //
            // `0xe5` CHECK POWER MODE answers in the sector-count register: `0xff` is "active or
            // idle", which is what a drive with no spin-down modelled is.
            0xe1 | 0xe2 | 0xe3 | 0xe6 => {
                self.status = ATA_DRDY | ATA_DSC;
                self.irq_pending = true;
            }
            0xe5 => {
                self.nsector = 0xff;
                self.status = ATA_DRDY | ATA_DSC;
                self.irq_pending = true;
            }
            0x10 => {
                self.status = ATA_DRDY | ATA_DSC;
                self.irq_pending = true;
            }
            0xe7 | 0xea | 0xef | 0x00 => {
                // SET FEATURES subcommand 0x03 is "set transfer mode", and the mode is in the
                // sector-count register: bits 7:3 select the family, bits 2:0 the mode number.
                // Remembering it is what lets IDENTIFY report back which mode is actually in
                // effect, instead of answering "none selected" to a driver that just selected one.
                if cmd == 0xef && self.features == 0x03 {
                    let (family, mode) = (self.nsector >> 3, self.nsector & 0x07);
                    match family {
                        0b00001 if mode <= 2 => self.mwdma_selected = 1 << mode, // multiword DMA
                        0b01000 if mode <= 4 => self.udma_selected = 1 << mode,  // ultra DMA
                        _ => {}
                    }
                }
                // FLUSH CACHE / INIT PARAMS / SET FEATURES / NOP — nothing to do, report ready.
                self.status = ATA_DRDY | ATA_DSC;
                self.irq_pending = true;
            }
            _ => {
                // Unknown commands must abort rather than appear to succeed, or the driver waits
                // forever for data that is never coming. The interrupt is half of saying so: this
                // comment described the intent for months while the code still left the driver
                // waiting, because the abort was never announced.
                self.status = ATA_DRDY | ATA_DSC | ATA_ERR;
                self.error = 0x04; // ABRT
                self.irq_pending = true;
            }
        }
    }

    pub(crate) fn read(&mut self, off: u32) -> u8 {
        *self.reads_log.entry(off).or_insert(0) += 1;
        // Controller registers: round-trip what was written, and report data-ready in IDE0_CFG.
        if off < 0x100 {
            let mut v = self.cfg[off as usize];
            if off == 0x28 && self.irq_pending {
                v |= 0x08;
            }
            return v;
        }
        // **There is one drive on this bus, and device 1 is not it.**
        //
        // A 5G iPod has a single ATA device. This model answered every taskfile register whatever
        // the DEV bit said, so a driver that probes the slave found a second drive sharing one set
        // of registers with the first — iPodLinux attached *two* disks of the same size and then
        // interleaved their commands through one state machine, which reads as `hda: lost
        // interrupt`. Apple's firmware and Rockbox never probe for a slave, which is why an empty
        // bus position stayed convincing for so long.
        //
        // An absent device drives nothing onto the bus and the host reads zero — that all-zero
        // status is exactly the signature `ide_probe` uses to conclude there is no drive there.
        if self.select & 0x10 != 0 && off >= 0x1e0 {
            return 0;
        }
        match off {
            // **The data register is SIXTEEN bits wide, sitting in a four-byte slot.**
            //
            // Every register in this block is four bytes apart, so a 32-bit access to `+0x1e0`
            // touches lanes 0..3 — but only lanes 0 and 1 are the register. Lanes 2 and 3 are the
            // upper half of a slot whose hardware is 16 bits, and they carry nothing.
            //
            // Serving them as *more sector data* was a real defect, and iPodLinux is the firmware
            // that showed it. Its identify path reads the port with 32-bit loads and keeps the low
            // halfword — correct for a 16-bit register — so with two words per access it kept our
            // words 0, 2, 4, 6 … and dropped every other one. Its `struct hd_driveid` then read
            // `cyls` out of our word 2 and `heads` out of our word 6, which is how a drive
            // reporting 16 heads was diagnosed as having 63 and every read of the partition table
            // failed before a single command reached the bus. Rockbox and Apple's firmware never
            // saw it: both read this port 16 bits at a time and never touch lanes 2 and 3.
            //
            // The arithmetic is the proof. iPodLinux does 256 32-bit reads per IDENTIFY. One word
            // each is 512 bytes — exactly one sector. Two words each is 1024, which is twice the
            // response it asked for.
            0x1e2..=0x1e3 => 0,
            0x1e0..=0x1e1 if self.pos < self.buf.len() => {
                let b = self.buf[self.pos];
                if self.id_watch && self.id_handover.len() < 24 {
                    self.id_handover.push((self.pos as u32, b));
                }
                self.pos += 1;
                self.bytes_read += 1;
                if self.pos == self.buf.len() {
                    if self.remaining > 0 {
                        self.remaining -= 1;
                        self.load_sector();
                    } else {
                        self.status = ATA_DRDY | ATA_DSC;
                    }
                }
                b
            }
            0x1e4..=0x1e7 => self.error,
            0x1e8..=0x1eb => self.nsector,
            0x1ec..=0x1ef => self.sector,
            0x1f0..=0x1f3 => self.lcyl,
            0x1f4..=0x1f7 => self.hcyl,
            0x1f8..=0x1fb => self.select | 0xa0,
            0x1fc..=0x1ff => self.status,
            0x3f8..=0x3fb => self.status, // alternate status: same value, no interrupt ack
            // The driver read-modify-writes CONTROL (`ldr / orr #1 / str`), so these have to read
            // back what was written or the arm and direction bits are lost on the way to GO.
            0x400..=0x40f => self.dma[(off - 0x400) as usize],
            _ => 0,
        }
    }

    pub(crate) fn write(&mut self, off: u32, val: u8) {
        if off < 0x100 {
            self.cfg[off as usize] = val;
            // Writing the clear bits acknowledges the controller interrupt.
            if off == 0x28 && val & 0x30 != 0 {
                self.irq_pending = false;
            }
            *self.cfg_writes_by_reg.entry(off & !3).or_insert(0) += 1;
            self.cfg_writes.push((off, val));
            return;
        }
        match off {
            // The data register, inbound. A PIO write command sets DRQ and the driver then feeds
            // the sector through here a byte at a time; each full 512 bytes is committed and the
            // LBA advances. Without this the drive raised DRQ and then never finished, which
            // Rockbox reports as `wait_for_end_of_transfer` failing.
            //
            // The iPod Video matters here specifically: its target defines MAX_PHYS_SECTOR_SIZE
            // 1024, so Rockbox does read-modify-write through PIO rather than DMA.
            // The write side of the same 16-bit register: lanes 2 and 3 are not a second word,
            // so a 32-bit store puts one word in and discards its upper half.
            0x1e2..=0x1e3 if self.status & ATA_DRQ != 0 && self.write_pio => {}
            0x1e0..=0x1e1 if self.status & ATA_DRQ != 0 && self.write_pio => {
                self.buf[self.pos] = val;
                self.pos += 1;
                if self.pos == self.buf.len() {
                    let lba = self.next_lba;
                    let data = std::mem::take(&mut self.buf);
                    self.commit_write(lba, &data);
                    self.buf = data;
                    self.pos = 0;
                    self.next_lba += 1;
                    self.remaining = self.remaining.saturating_sub(1);
                    if self.remaining == 0 {
                        self.status = ATA_DRDY | ATA_DSC; // DRQ down: transfer complete
                        self.write_pio = false;
                    } else {
                        self.status = ATA_DRDY | ATA_DSC | ATA_DRQ;
                    }
                    self.irq_pending = true;
                }
            }
            // FEATURES. Dropping this on the floor was a real bug: SET FEATURES carries its whole
            // meaning in this register, so without it every subcommand looked identical.
            0x1e4..=0x1e7 => self.features = val,
            0x1e8..=0x1eb => self.nsector = val,
            0x1ec..=0x1ef => self.sector = val,
            0x1f0..=0x1f3 => self.lcyl = val,
            0x1f4..=0x1f7 => self.hcyl = val,
            0x1f8..=0x1fb => self.select = val,
            0x1fc..=0x1ff => self.command(val),
            // The bus-master DMA engine. GO is bit 0 of CONTROL. The ROM sets it *after* writing
            // the taskfile command; RetailOS sets it before. Arming is therefore recorded, not
            // acted on, and the transfer starts from whichever of the two edges lands second.
            0x400..=0x40f => {
                self.dma[(off - 0x400) as usize] = val;
                if off == 0x400 && val & 1 != 0 {
                    self.dma_armed = true;
                    self.dma_try_start();
                }
            }
            // Anything else in the window was being swallowed silently.
            other => {
                *self.cfg_writes_by_reg.entry(other & !3).or_insert(0) += 1;
                self.cfg_writes.push((other, val));
            }
        }
    }
}
