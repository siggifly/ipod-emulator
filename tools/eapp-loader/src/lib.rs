//! B1 — load an eApp, resolve its framework imports, and trace every call it makes.
//!
//! The whole point of this stage is the **call trace**: the eApp header advertises ~98 imports,
//! but nobody knows how many a real game actually touches. Wiring every import to a distinct trap
//! address and logging arrivals answers that empirically instead of by guesswork, and the answer
//! is what sizes the rest of the project.
//!
//! How import resolution works, and why traps are faithful rather than a hack: each import is a
//! `ldr pc, [pc, #imm]` thunk whose literal slot RetailOS patches at load time. We patch the same
//! slot — with an address in a region that holds no code. When the game calls the import, the
//! thunk loads our address into `PC` exactly as it would load the real one, and the runner
//! notices `PC` landed in trap space. Nothing is special-cased in the CPU.

use std::collections::{BTreeMap, HashMap};

use arm7tdmi::{Bus, Cpu, Mode};

/// Recording the co-processor's panel over a run, and the PNG writer that lands the frames.
/// Neither touches the machine: `film` reads `Bcm::mem` between chunks of the run and costs the
/// emulated CPU nothing. See `tools/ipod-film/README.md`.
pub mod film;
pub mod png;

/// Building a drive image from an IPSW, so nobody has to be handed 8 GB of somebody else's iPod.
/// A zip reader, an inflate, and an MBR + FAT32 writer — none of which touches the machine.
pub mod ipsw;
pub mod inspect;
pub mod settings;

pub const EAPP_MAGIC: &[u8; 4] = b"eapp";
/// Corrected 2026-08-11 against RetailOS 1.3's own loader — see `eapp-inspect` for the evidence.
/// The reversed order is still accepted, because the prior public report had it that way and only
/// a real game binary settles it.
pub const BLOCK_MAGIC: [u8; 4] = [0x68, 0x19, 0x06, 0x29];
pub const BLOCK_MAGIC_REVERSED: [u8; 4] = [0x29, 0x06, 0x19, 0x68];
const EMPTY_MD5: [u8; 16] = [
    0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04, 0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8, 0x42, 0x7e,
];

const LDR_PC_PC: u32 = 0xE59F_F000;
const LDR_PC_PC_MASK: u32 = 0xFFFF_F000;

/// Trap space. Chosen far above any plausible load address so a stray branch into it is
/// unambiguous — if `PC` lands here, an import was called and nothing else can explain it.
pub const TRAP_BASE: u32 = 0xF000_0000;
const TRAP_STRIDE: u32 = 0x1000;

// ---------------------------------------------------------------- capped logs
//
/// A bounded log that also carries an **unbounded count**, and refuses to let the two be confused.
///
/// Nine published conclusions in this project have been lost to instruments that failed silently,
/// and most were the same shape: a `Vec` behind a `len() < N` guard, printed as `log.len()`. Once
/// the log filled, that number stopped being a measurement and became the cap — a constant, printed
/// with the confidence of a count. `ata commands: 256` served as this project's baseline
/// fingerprint for months while being the cap; the true figure is 770. `--watch-range`'s `4096`
/// produced *"RetailOS never touches the VideoCore"*, which was wrong and steered the strategy
/// toward emulating a co-processor that was never in the way.
///
/// The type exists so the trap cannot be re-entered by accident:
///
/// - [`push`](Self::push) increments [`seen`](Self::seen) **always**, and stores only while under
///   the cap. `seen` is the census and cannot saturate.
/// - There is deliberately **no `len()`**. `len()` on a capped log is the trap itself: it reads
///   like a count everywhere it is printed. Call sites must ask for either the census
///   ([`seen`](Self::seen)) or the sample ([`sample`](Self::sample)), and say which they printed.
/// - [`census`](Self::census) renders the headline — the true total, plus an unmissable notice when
///   the rows below it are a sample. Every report line built from one of these uses it.
///
/// [`push_with`](Self::push_with) is the same contract for entries that cost something to build:
/// the count still rises when the closure is not run, so laziness never costs a number.
#[derive(Debug, Clone)]
pub struct Capped<T> {
    cap: usize,
    kept: Vec<T>,
    seen: u64,
}

impl<T> Capped<T> {
    pub const fn new(cap: usize) -> Self {
        Self { cap, kept: Vec::new(), seen: 0 }
    }

    /// Record an entry. The count always rises; the row is kept only while there is room.
    pub fn push(&mut self, v: T) {
        self.seen += 1;
        if self.kept.len() < self.cap {
            self.kept.push(v);
        }
    }

    /// The same, for a row that is expensive to construct — a `format!`, a memory peek. The closure
    /// runs only when the row will be kept, but `seen` rises either way, so a cheap instrument never
    /// buys itself a wrong number.
    pub fn push_with(&mut self, f: impl FnOnce() -> T) {
        self.seen += 1;
        if self.kept.len() < self.cap {
            self.kept.push(f());
        }
    }

    /// How many entries actually happened. This is the number to print.
    pub fn seen(&self) -> u64 {
        self.seen
    }

    /// The retained rows. A **sample** whenever [`truncated`](Self::truncated) is true.
    pub fn sample(&self) -> &[T] {
        &self.kept
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.kept.iter()
    }

    /// The most recently **kept** row, for amending a value that only becomes known after the push.
    /// Guard it with `sample().len()` around the push: once the cap is reached this still points at
    /// an old row, and writing through it would rewrite somebody else's measurement.
    pub fn last_mut(&mut self) -> Option<&mut T> {
        self.kept.last_mut()
    }

    /// True when nothing was ever recorded — not "nothing was kept". A cap of zero would make those
    /// differ, and reports gate their whole section on this.
    pub fn is_empty(&self) -> bool {
        self.seen == 0
    }

    pub fn cap(&self) -> usize {
        self.cap
    }

    pub fn truncated(&self) -> bool {
        self.seen > self.kept.len() as u64
    }

    /// The headline: the census, and — when the log below it is truncated — a notice that cannot be
    /// skimmed past. The wording is the one the ATA fix established, because a reader who has learned
    /// to look for `SAMPLE, NOT A CENSUS` should find the same words on every instrument.
    pub fn census(&self) -> String {
        if self.truncated() {
            format!(
                "{}  (log below shows the first {} — SAMPLE, NOT A CENSUS)",
                self.seen,
                self.kept.len()
            )
        } else {
            format!("{}", self.seen)
        }
    }

    /// The tail line under a report that printed only `shown` of the retained rows. Returns `None`
    /// when everything recorded is on screen — so a caller that always prints it never lies in
    /// either direction.
    pub fn more_line(&self, shown: usize) -> Option<String> {
        let kept = self.kept.len();
        if self.seen <= shown as u64 {
            return None;
        }
        let dropped = self.seen - kept as u64;
        Some(match (self.truncated(), kept.saturating_sub(shown)) {
            // Everything kept is on screen, and the rest never made it into the log at all.
            (true, 0) => format!(
                "  … and {dropped} more that were dropped past the {}-entry cap — SAMPLE, NOT A CENSUS",
                self.cap
            ),
            (true, more) => format!(
                "  … {more} more of the {kept} kept, and {dropped} dropped past the {}-entry cap — SAMPLE, NOT A CENSUS",
                self.cap
            ),
            (false, more) => format!("  … and {more} more"),
        })
    }
}

// Deliberately **no** `impl Default for Capped`. A default would have to invent a cap, and a
// struct that picked one up through `#[derive(Default)]` would get a silent bound nobody chose —
// which is the whole failure this type exists to prevent. Every cap in this crate is written at its
// construction site, where the number can be justified, and adding a `Capped` to a `Default`-derived
// struct is meant to fail the build.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Framework {
    pub name: String,
    pub hash: [u8; 16],
    /// Address of each import thunk, in declaration order.
    pub thunks: Vec<u32>,
}

#[derive(Debug)]
pub struct EApp {
    pub load_base: u32,
    pub entry: u32,
    /// The vector table at header +0x14 — entry point first, then further hooks.
    pub vectors: Vec<u32>,
    pub image: Vec<u8>,
    pub frameworks: Vec<Framework>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    NotAnEApp,
    Truncated,
    NoLoadBase,
}

/// Where a title is linked to load, and where the region lives when there is no title.
pub const EAPP_LOAD_BASE: u32 = 0x1800_0000;

impl EApp {
    /// No eApp at all.
    ///
    /// A boot never executes one: RetailOS is entered from the reset vector and never looks at
    /// `EAPP_LOAD_BASE`. The region is still created, so a stray access there is mapped exactly as
    /// it is with a title loaded — it just reads as zeros. Every recipe that boots uses this, which
    /// is why booting needs nothing but a NOR dump and a drive image.
    pub fn none() -> Self {
        EApp {
            load_base: EAPP_LOAD_BASE,
            entry: 0,
            vectors: Vec::new(),
            image: Vec::new(),
            frameworks: Vec::new(),
        }
    }

    /// Whether anything was loaded — false for [`EApp::none`].
    pub fn is_loaded(&self) -> bool {
        !self.image.is_empty()
    }

    /// Parse an eApp image. `load_base` is derived from the header pointer rather than assumed,
    /// so a title linked somewhere other than `0x1800_0000` still loads.
    pub fn parse(image: Vec<u8>) -> Result<Self, LoadError> {
        if image.len() < 0x28 || image[..4] != *EAPP_MAGIC {
            return Err(LoadError::NotAnEApp);
        }
        let load_base = derive_load_base(&image).ok_or(LoadError::NoLoadBase)?;

        // The entry point is the FIRST WORD OF THE VECTOR TABLE at +0x14 — an absolute address,
        // not the offset at +0x0C.
        //
        // Determined empirically 2026-08-11: `load_base + [+0x0C]` lands on `b .` (a self-branch
        // trap) in Pac-Man, so the loader ran two million instructions and made zero calls. The
        // word at +0x14 lands on a real `stmdb sp!, {r4-r6, lr}` prologue. RetailOS corroborates:
        // its validator `memcpy`s `count * 4` bytes starting at +0x14, i.e. it treats that region
        // as the table of entry points and never reads +0x0C at all.
        let vectors: Vec<u32> = (0x14..0x28).step_by(4).map(|o| u32le(&image, o)).collect();
        let entry = vectors[0];

        let mut frameworks = Vec::new();

        // The primary framework (OpenGLES in every title seen so far) hangs off header +0x10 and
        // carries no magic — it is invisible to a scan and was missed entirely until an
        // unpatched thunk at the image start branched to zero.
        let primary_off = u32le(&image, 0x10).wrapping_sub(load_base) as usize;
        if let Some(fw) = parse_descriptor(&image, primary_off, load_base) {
            frameworks.push(fw);
        }

        let mut off = 0usize;
        while off + 4 <= image.len() {
            if image[off..off + 4] == BLOCK_MAGIC || image[off..off + 4] == BLOCK_MAGIC_REVERSED {
                if let Some(fw) = parse_descriptor(&image, off + 4, load_base) {
                    if fw.hash != EMPTY_MD5 {
                        frameworks.push(fw);
                    }
                }
            }
            off += 4;
        }

        Ok(EApp {
            load_base,
            entry,
            vectors,
            image,
            frameworks,
        })
    }

    pub fn import_count(&self) -> usize {
        self.frameworks.iter().map(|f| f.thunks.len()).sum()
    }
}

/// Framework descriptor layout, verified against real binaries *and* RetailOS's validator
/// (the eApp loader research). Offsets are relative to the **name**, which is a fixed
/// 32-byte buffer:
///
/// ```text
/// +0x00  name (32 bytes, NUL-padded)
/// +0x20  16-byte interface hash — what frameworks actually bind by
/// +0x30  function count
/// +0x34  pointer (RetailOS rejects the descriptor if zero)
/// +0x38  `count` thunks, then `count` literal slots
/// ```
///
/// A block in the import list is exactly this with a 4-byte magic prepended. The **primary**
/// framework — pointed at by header `+0x10` — is the same structure with *no* magic, which is
/// why RetailOS resolves it separately from the block loop.
const DESC_HASH_OFF: usize = 0x20;
const DESC_COUNT_OFF: usize = 0x30;
const DESC_PTR_OFF: usize = 0x34;
const DESC_THUNKS_OFF: usize = 0x38;

/// Parse a descriptor whose name begins at `name_off`.
fn parse_descriptor(image: &[u8], name_off: usize, load_base: u32) -> Option<Framework> {
    if name_off + DESC_THUNKS_OFF > image.len() {
        return None;
    }
    let name = cstr_at(image, name_off)?;
    if name.is_empty() {
        return None;
    }
    let mut hash = [0u8; 16];
    hash.copy_from_slice(&image[name_off + DESC_HASH_OFF..name_off + DESC_HASH_OFF + 16]);
    if u32le(image, name_off + DESC_PTR_OFF) == 0 {
        return None; // RetailOS rejects these outright
    }
    let count = u32le(image, name_off + DESC_COUNT_OFF) as usize;
    let mut p = name_off + DESC_THUNKS_OFF;

    // Trust the thunks actually present over the declared count: a disagreement means the
    // layout assumption is wrong, and walking off the end of a real image is worse than
    // under-reporting.
    let mut thunks = Vec::new();
    while thunks.len() < count && p + 4 <= image.len() {
        if u32le(image, p) & LDR_PC_PC_MASK != LDR_PC_PC {
            break;
        }
        thunks.push(load_base + p as u32);
        p += 4;
    }

    Some(Framework { name, hash, thunks })
}

fn derive_load_base(image: &[u8]) -> Option<u32> {
    let ptr = u32le(image, 0x10);
    for base in [0x1800_0000, ptr & 0xFF00_0000, ptr & 0xFFF0_0000, 0] {
        let off = ptr.wrapping_sub(base) as usize;
        if off < image.len() && cstr_at(image, off).is_some_and(|s| !s.is_empty()) {
            return Some(base);
        }
    }
    None
}

// ---------------------------------------------------------------- memory

/// Flat address space: the loaded image, plus one RAM region for stack and heap.
///
/// Accesses that hit neither are recorded rather than faulted. An unmapped access is a *finding*
/// at this stage — it means the game expects something we have not modelled, and silently
/// returning zero would hide exactly the information B1 exists to collect.
pub struct Memory {
    pub regions: Vec<Region>,
    /// Direct-mapped page-resolution cache for the 32-bit fast path. Boxed because it is large and
    /// only ever touched through indices.
    fast: Box<[FastPage]>,
    /// Unmapped accesses, aggregated per 4 KB page rather than logged one by one.
    ///
    /// The flat log this replaces was capped at 4096 entries to survive a runaway loop, with the
    /// result that every busy run reported the same saturated "4032 reads, 64 writes" no matter
    /// what it had actually done — a constant read as a measurement. Counting per page is bounded
    /// by the number of distinct pages touched, so the totals stay true however long the run.
    pub unmapped: BTreeMap<u32, UnmappedPage>,
    /// `(base, size, target)` — address windows that are another view of memory elsewhere.
    pub aliases: Vec<(u32, u32, u32)>,
    /// `(address, value_a, value_b)` words that **alternate** between two values on each read.
    ///
    /// For busy flags. Named for `0x7000003c`, which turned out not to need it — that register's
    /// two waits are both on bit 31, in the same direction, and bit 24 written beside it is what
    /// makes bit 31 true (see [`Xmb`]). The mechanism is kept because the reasoning behind it holds
    /// for any handshake we have not yet read: a value that alternates satisfies both edges of a
    /// wait, which is how you find out what is on the other side of one.
    pub read_toggle: Vec<(u32, u32, u32)>,
    /// Which side of each toggle comes next.
    toggle_state: Vec<bool>,
    /// `(address, value)` words that always read as `value`, whatever was written to them.
    ///
    /// For registers whose real value comes from hardware we do not model. `COP_STATUS` is the
    /// case that motivated it: firmware *wakes* the coprocessor by writing `WAKE` to the same
    /// address it later polls for `COPSLEEPING`, so on a single-core machine the bit must be
    /// reasserted by the model or the wait never ends.
    pub read_overrides: Vec<(u32, u32)>,
    /// Bits forced **on** in a register, leaving every other bit as the machine actually holds it.
    ///
    /// Ledger #8 was a whole-word override: reads of `PLL_STATUS` returned exactly `0x80000000`, so
    /// the lock bit was asserted and *every other bit read as zero*. Reporting an emulated PLL as
    /// locked is defensible — it has no physical lock delay — but blanking the rest of the register
    /// is a second, undocumented lie riding along with the first.
    ///
    /// An OR-mask asserts only what is being claimed.
    pub read_or_masks: Vec<(u32, u32)>,
    /// Address of the free-running microsecond counter, if the machine has one.
    ///
    /// A zeroed MMIO region is a **stopped clock**, and firmware written against a real one waits
    /// forever: every `elapsed = now - start; if elapsed >= timeout` check stays false when `now`
    /// never moves. It reads as a hang and looks nothing like a missing register.
    pub usec_timer: Option<u32>,
    /// Current value of that counter, advanced by [`Machine::run`].
    pub usec: u32,
    /// Ablation switch for the IDE0_CFG acknowledgement above, set by `--no-cfg-ack`. The driver
    /// writes those clear bits twelve instructions after issuing a command, which is inside the
    /// window before the next `service_interrupts`; turning the ack off is how that ordering is
    /// distinguished from a genuinely undelivered interrupt.
    pub ide_cfg_ack_off: bool,
    /// Ablation switch for the IDE0_CFG **latch** itself (ledger #9), set by `--no-ide-irq-latch`.
    /// Bit 3 stops being reported and nothing else changes, so what the firmware does without it is
    /// the measurement of what it was buying. The bit had been ORed in unconditionally, with no
    /// arm B, since before there was a ledger.
    pub ide_irq_latch_off: bool,
    /// When the drive's next completion is due, as a value of `usec`. See `arm_ide_irq`.
    pub ide_irq_due: Option<u32>,
    /// The core wrote CPU_CTRL's sleep bit and is waiting for an interrupt. Consumed by
    /// [`Machine::run`], which is the only place that knows when the next one is due.
    pub cpu_sleep: bool,
    /// Microseconds skipped by those sleeps. Kept apart from `executed` deliberately: the clock has
    /// to advance while the core is halted, but the instruction count must stay a count of
    /// instructions actually run, or every profile and novelty measurement built on it becomes a
    /// measurement of how long we idled.
    pub slept_usec: u32,
    /// How many times the core was put to sleep, and how long it stayed there.
    pub sleeps: u64,
    /// Interrupt sources currently asserted, one bit per IRQ number.
    pub int_pending: u32,
    /// The same, for the controller's second bank — IRQs 32..63, whose registers sit at
    /// `0x60004100` and mirror the first bank's layout exactly. RetailOS's kernel init writes both
    /// banks in six consecutive stores (`0x1604`..`0x1618`), and its ATA driver enables bit 23 in
    /// *each* — IRQ 23 for the drive's INTRQ, IRQ 55 for the DMA engine's own completion.
    pub int_pending_hi: u32,
    /// `(base, device)` of an attached ATA controller.
    pub ata: Option<(u32, Ata)>,
    /// The video co-processor, when modelled rather than left as passive memory.
    pub bcm: Option<Bcm>,
    /// Base of the PP I²C controller, if transactions should be logged.
    ///
    /// Register map from Rockbox `i2c-pp.c`: `CTRL` at `+0x00` (bit `0x80` starts a transfer, bit
    /// `0x20` selects read, bits 1..2 carry `len-1`), `ADDR` at `+0x04`, data bytes at `+0x0c`
    /// stepping by 4, `STATUS` at `+0x1c` with bit 6 = BUSY.
    pub i2c_base: Option<u32>,
    /// `(addr, ctrl, data)` per started transfer — an ordered **sample**, so the first traffic to a
    /// chip can be read in sequence. The census lives in `i2c_tally` beside it.
    pub i2c_log: Capped<(u8, u8, [u8; 4])>,
    /// `(device, ctrl, register) -> count`, **uncapped**. One map, three views: sum by device, by
    /// `(device, register)`, or by ctrl.
    ///
    /// It exists because the three histograms in the run report were all built from `i2c_log`, and
    /// that log fills at 4 096 on the standard baseline — so every one of them was a floor wearing a
    /// count's clothes. `NEXT.md` §5 was about to fit a WM8758 model to "52 transfers" taken from it.
    pub i2c_tally: BTreeMap<(u8, u8, u8), u64>,
    /// Byte returned for every I²C data-register read, when set.
    ///
    /// Crude on purpose: it is an experiment, not a device model. Every peripheral status bit the
    /// firmware polls reads as *set*, which answers "is it waiting on a bit that never asserts?"
    /// in one run without having to guess which bit.
    pub i2c_fill: Option<u8>,
    /// The PCF50605 itself, when modelled rather than answered with a fixed byte.
    pub pmu: Option<Pcf50605>,
    /// The external memory bus controller, when modelled rather than answered with `--rdval`.
    pub xmb: Option<Xmb>,
    /// The click wheel, when modelled rather than answered with zero.
    ///
    /// Answering zero was never "nothing is happening": `0x00000000` fails both of RetailOS's frame
    /// checks, so the driver takes its *error* path five times per poll and the emulator was
    /// reporting a broken transceiver rather than an idle one.
    pub clickwheel: Option<ClickWheel>,
    /// The panel's brightness, counted off the pulses the firmware sends. See [`Backlight`].
    pub backlight: Backlight,
    /// The NOR flash as a device rather than a read-only region, when modelled.
    pub nor: Option<Nor>,
    /// `(base, size)` of a range to account for at 256-byte granularity, and the counts.
    ///
    /// "Which region" is too coarse to name a device: `0x70000000` holds the chip-ID registers, the
    /// GPIO block, I²S and I²C. This resolves an access down to the register block.
    /// `--pagelog=BASE:SIZE[:GRAN]` — access counts across a range, bucketed by `GRAN` bytes.
    ///
    /// Granularity is a parameter because the two questions are different sizes: "which register
    /// block is busy" wants 256 bytes, and "which register in it" wants 4.
    pub page_log: Option<(u32, u32)>,
    pub page_gran: u32,
    /// `--writelog=BASE:SIZE` — record where stores in a range actually LAND.
    ///
    /// `--watch` reports value *changes*, so it cannot distinguish "wrote 0 over 0" from "never
    /// wrote", and leaning on it produced a contradiction in research/06. This records the attempt:
    /// the PC, the value, and the region that answered — or `DROPPED` if none did.
    /// `--verify-memory` — cross-check the page cache against the slow path on every access.
    ///
    /// The `fast_region` bug (research/06) returned data from the WRONG region rather than no
    /// region, so it was invisible to unmapped-access reporting, to `--watch`, and to four other
    /// checks. That failure mode is indistinguishable from "a field nobody ever wrote" — which is
    /// precisely what RetailOS's blocker looks like. This makes the whole class loud.
    pub verify_memory: bool,
    pub verify_mismatches: Capped<(u32, u32, &'static str, &'static str)>,
    /// `--input-regs=BASE:SIZE` — addresses the firmware READS before ever writing them.
    ///
    /// These are the pure hardware *inputs*: values the firmware expects silicon to supply. We have
    /// no silicon, so every one of them is answered with whatever the region happens to hold —
    /// usually zero. That makes this list, not the raw register set, the honest enumeration of
    /// where this emulator is inventing answers.
    pub input_probe: Option<(u32, u32)>,
    /// addr -> (reads-before-first-write, writes, first reading PC)
    pub input_regs: BTreeMap<u32, (u64, u64, u32)>,
    /// `--watch-range=BASE:LEN` — every write into a range, with the PC and the value.
    ///
    /// `--watch` records value *changes* to one word, which cannot distinguish "wrote 0 over 0"
    /// from "never wrote" — that ambiguity produced a whole contradiction in research/06 and was
    /// leaned on twice more. This records the write itself, over a whole structure.
    pub watch_range: Option<(u32, u32)>,
    /// An ordered **sample** of those writes. The report is driven by `watch_range_words` below.
    pub watch_range_log: Capped<(u32, u32, u32, u8)>,
    /// `word -> (byte-writes, PC -> count)`, **uncapped**.
    ///
    /// The two failures this replaces were one instrument and two bugs. The log capped at 4 096, and
    /// the report attributed each word to whichever PC touched it *first* — so a span that Apple's
    /// bootloader writes before RetailOS executes an instruction reported the bootloader as its only
    /// author, which is how *"RetailOS never touches the VideoCore"* got published. Counting per word
    /// and per PC at capture time is bounded by distinct `(word, PC)` pairs, not by run length, and
    /// names every writer instead of the earliest one.
    pub watch_range_words: BTreeMap<u32, WatchWord>,
    pub write_log: Option<(u32, u32)>,
    pub write_log_entries: Capped<(u32, u32, u32, &'static str)>,
    /// Stores in the `--writelog` span by answering region, **uncapped** — including `DROPPED`.
    /// The report's per-region breakdown used to be a tally of the capped log.
    pub write_log_regions: BTreeMap<&'static str, u64>,
    /// Times the drive's IRQ line was raised, and times it was cleared by a status read.
    pub ide_irq_raised: u64,
    pub ide_irq_acked: u64,
    /// Times an IRQ was actually delivered to the CPU with the drive's bit set and enabled.
    pub ide_irq_delivered: u64,
    /// Bytes a DMA transfer could not place because no region answered the destination.
    pub dma_dropped: u64,
    pub dma_drop_sites: Vec<(u32, u64)>,
    /// Transfers the PP502x DMA controllers ran, and bytes moved. Separate from `dma_dropped`
    /// above, which belongs to the ATA controller's own bus-master engine at `IDE_BASE+0x400`.
    pub pp_dma_transfers: u64,
    pub pp_dma_bytes: u64,
    /// `(channel base, source, destination, bytes)` per transfer, capped. The interesting property
    /// of this engine is *which* addresses it was pointed at, and there are only a handful — but
    /// `--bcm-registry` turns 4 transfers into 104, so "a handful" is a property of a configuration
    /// rather than of the engine, and the cap has to announce itself like every other.
    pub pp_dma_log: Capped<(u32, u32, u32, u32)>,
    /// Completion line for the `0x60008000` controller, overriding `PP_DMA[0].irq`.
    ///
    /// A flag rather than a constant because that line is the one part of this device that is
    /// inferred rather than read: nothing published names it, and RetailOS's driver holds four
    /// candidate masks. `--pp-dma-irq=N` is how the candidates get tested against the machine
    /// instead of against an argument.
    pub pp_dma_irq: Option<u32>,
    pub page_counts: BTreeMap<u32, (u64, u64)>,
    /// Whether to attribute each access to a region. Off by default: it costs a scan of the region
    /// list on **every byte access**, on top of the scan `locate` already does. Enabled by
    /// `--devices`, which is the only thing that reads the result.
    pub accounting: bool,
    /// Regions that ordinary stores cannot modify, by name.
    ///
    /// NOR flash is one: it answers reads at address 0 out of reset, but a store does not change it
    /// — real flash needs a command sequence. Without this the cold-boot mapping shadows low SDRAM
    /// permanently, so the bootloader's own load of the firmware image lands in the ROM instead of
    /// in RAM, and a megabyte of data quietly goes nowhere.
    pub readonly: Vec<&'static str>,
    /// Set while the emulator services its own devices, to keep that traffic out of the counters.
    pub internal: bool,
    /// Per-region access counters, parallel to `regions`.
    pub region_reads: Vec<u64>,
    pub region_writes: Vec<u64>,
    /// `(address, bit)` — reading this word acknowledges that interrupt source.
    ///
    /// The PP interrupt controller has no central acknowledge register; a timer interrupt is
    /// cleared at the timer, by reading its `VAL`. Clearing on delivery instead would be simpler
    /// and wrong: the handler reads `CPU_INT_STAT` to decide *which* source fired, so a bit that
    /// has already been cleared dispatches to nothing.
    pub int_ack_on_read: Vec<(u32, u32)>,
    /// The PP5022 MMAP unit — 8 window pairs at an 8-byte stride, `LOGICAL` then `PHYSICAL`.
    ///
    /// Encoding from `ipodloader2/interrupts.c`, which programs it directly:
    /// `LOGICAL = 0x3a00 | base`, `PHYSICAL = 0x3f84 | base` — the base is the top 16 bits.
    ///
    /// This is what ends the boot: Apple's ROM loads `osos` to 0x10000000, programs window 1 as
    /// logical 0 -> physical 0x10000000, and then jumps LOW (to 0x23c). Without the remap that
    /// jump lands in NOR and decodes as an undefined instruction.
    pub mmap_base: Option<u32>,
    mmap_regs: [u32; 16],
    /// How many aliases were installed before any MMAP window; those survive a rebuild.
    pub mmap_alias_floor: usize,
    /// PC of the instruction being executed, so an unmapped access can name its own culprit.
    /// Set by [`Machine::run`]; zero when memory is driven directly by a test.
    pub pc: u32,
    /// Instructions retired, mirrored from the machine so a store can be placed in time. `usec` is
    /// already here but is paced by `--clock`, so it cannot order two stores inside one microsecond.
    pub icount: u64,
    /// `--storelog=PC[,PC…]` — every store those instructions perform: `(pc, addr, value, icount)`.
    ///
    /// `--watch-range` is keyed by *address*, which is only usable once the address is known — and
    /// heap addresses are not stable across runs. Keying by the *storing instruction* inverts that:
    /// one known writer inside a constructor enumerates every object it ever built, in creation
    /// order, however the heap happened to lay them out.
    pub store_pcs: Vec<u32>,
    /// `--storeaddr=ADDR[,ADDR…]` — the inverse query: every store that *lands* on one of these
    /// addresses, whatever instruction made it. `--watch-range` answers this for one contiguous
    /// span; a question like "does anything ever write `+0x20` of any of these 791 objects" is 791
    /// disjoint words, and asking it one span at a time is 791 runs.
    pub store_addrs: Vec<u32>,
    store_addr_lo: u32,
    store_addr_hi: u32,
    /// `--readlog=ADDR[,ADDR…]|FILE` — the mirror of `--storeaddr`. "Who consumed this buffer" is as
    /// load-bearing a question as "who filled it", and until now nothing could answer it: a value
    /// that arrives by DMA has no storing instruction to key on at all.
    pub read_addrs: Vec<u32>,
    read_addr_lo: u32,
    read_addr_hi: u32,
    /// `(pc, addr, value, icount)` per watched read. Capped, and the cap is the one that turned a
    /// control read 9 588 012 times into a clean zero — see [`Capped`].
    pub read_log: Capped<(u32, u32, u8, u64)>,
    /// An address range to record execution within, and what was recorded.
    ///
    /// `None` costs one compare per instruction, which is what makes it acceptable to leave in the
    /// hot loop of an interpreter.
    pub trace_pc: Option<(u32, u32)>,
    pub pc_trace: Vec<(u32, u64)>,
    /// Record `bl` edges from this instruction count onward.
    ///
    /// A PC trace of a flattened function is mostly its dispatcher going round; the *calls* are the
    /// shape worth having, because they are what the obfuscation does not hide -- a call still has
    /// to name its target. Gated on an instruction count so the window can be aimed at the moment
    /// the subsystem runs rather than recording a whole boot.
    pub trace_calls_from: Option<u64>,
    pub call_trace: Vec<(u32, u32, u64)>,
    /// Execution counts per 64-byte bucket of the low 8 MB, when profiling.
    ///
    /// A *call* histogram answers "who calls whom", which is the wrong question for obfuscated
    /// mixed-arithmetic code: it computes inline and loops without calling, so its work does not
    /// appear as edges at all. Counting where instructions actually retire does answer it.
    ///
    /// 64 bytes is sixteen instructions -- fine enough to name a loop body, coarse enough that the
    /// table is 128 K entries and stays in cache.
    pub pc_hist: Option<Vec<u64>>,
    /// Report the register file the first `n` times this address executes.
    ///
    /// The profiler names *where* the time goes; this names *what it is working on*. At the head of
    /// a bignum loop the registers are the operands -- the limb pointers, the multiplier, the
    /// length -- and those pointers are what makes the data traceable back to whatever produced it.
    /// Do not force `COP_STATUS` to report the second core asleep.
    ///
    /// This does **not** emulate the core — it only stops lying about it. What RetailOS does when
    /// its wake finally appears to succeed is the measurement; it may proceed, or it may wait for a
    /// partner that will never answer, and both are worth knowing before building one.
    pub cop_awake: bool,
    pub regs_at: Option<(u32, usize)>,
    pub regs_seen: Vec<(u64, [u32; 16])>,
    /// `(addr, pc) -> (reads, first icount)`, **uncapped**. The report's per-reader breakdown; the
    /// log above is the ordered sample it sits under.
    pub read_sites: BTreeMap<(u32, u32), (u64, u64)>,
    pub store_pc_log: Capped<(u32, u32, u32, u64)>,
    store_split: u8,
    /// Bumped by every unmapped access, so [`Machine::run`] can tell that the instruction it just
    /// stepped made one. The unmapped report names a PC and an address; it cannot say which
    /// *register* carried the bad address, and for a value that arrives through a chain of
    /// dereferences that is the only question worth asking — `--retwatch` on the value found
    /// nothing, which is either a real answer or a broken instrument, and only the register file at
    /// the instant of the access distinguishes those.
    pub unmapped_seq: u64,
}

/// What happened in one 4 KB page of unmapped space.
#[derive(Default, Clone)]
pub struct UnmappedPage {
    pub reads: u64,
    pub writes: u64,
    /// Address range actually touched within the page — usually far narrower than the page.
    pub lo: u32,
    pub hi: u32,
    /// PC of the first access, which is the one worth disassembling.
    pub first_pc: u32,
    /// Every PC that touched the page, with a count.
    ///
    /// `first_pc` alone attributes a whole page to whichever instruction happened to arrive first,
    /// which reads as "this one instruction is the culprit" even when it made one access out of
    /// thousands. Twice in this project a report that showed a sample has been mistaken for one
    /// that showed everything; a tally costs a map per page and removes the trap.
    pub pcs: BTreeMap<u32, u64>,
}

/// What happened to one 32-bit word inside a `--watch-range` span.
///
/// The same shape as [`UnmappedPage`], and for the same reason: attributing a word to its *first*
/// writer is what let the bootloader's framebuffer fill hide RetailOS's later writes to the same
/// addresses, and publish that RetailOS never touched the co-processor.
#[derive(Default, Clone)]
pub struct WatchWord {
    /// Byte-granular writes landing anywhere in the word.
    pub writes: u64,
    /// Every storing PC, with a count. Not just the first.
    pub pcs: BTreeMap<u32, u64>,
    /// Instruction count of the first write, so a span can be split into eras by hand.
    pub first_at: u64,
}

/// Pull `Files[].Path` out of an XML `Manifest.plist`, in document order.
///
/// A full plist parser is unnecessary: the file is XML, and the only thing needed is the sequence
/// of `<key>Path</key><string>…</string>` pairs.
fn manifest_paths(path: &std::path::Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut out = Vec::new();
    let mut rest = text.as_str();
    while let Some(i) = rest.find("<key>Path</key>") {
        rest = &rest[i + 15..];
        let Some(a) = rest.find("<string>") else { break };
        let Some(b) = rest[a + 8..].find("</string>") else { break };
        out.push(rest[a + 8..a + 8 + b].to_string());
        rest = &rest[a + 8 + b..];
    }
    (!out.is_empty()).then_some(out)
}

/// Expand a 16-bit A1R5G5B5 or RGB565 pixel to RGBA8, colour-keying magenta.
fn expand16(v: u16, rgb565: bool) -> [u8; 4] {
    let (r, g, b) = if rgb565 {
        (((v >> 11) & 0x1F) as u8, ((v >> 5) & 0x3F) as u8, (v & 0x1F) as u8)
    } else {
        (((v >> 10) & 0x1F) as u8, ((v >> 5) & 0x1F) as u8, (v & 0x1F) as u8)
    };
    let g8 = if rgb565 { (g << 2) | (g >> 4) } else { (g << 3) | (g >> 2) };
    // 0xF83E is the colour key in both encodings the games use.
    let a = if v == 0xF83E { 0 } else { 255 };
    [(r << 3) | (r >> 2), g8, (b << 3) | (b >> 2), a]
}

/// Uncompressed 16-bit TGA. Rows stay in file order; the framebuffer's Y flip handles orientation.
fn decode_tga(d: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    if d.len() < 18 || d[2] != 2 || d[16] != 16 {
        return None;
    }
    let w = u16::from_le_bytes([d[12], d[13]]) as usize;
    let h = u16::from_le_bytes([d[14], d[15]]) as usize;
    let top = d[17] & 0x20 != 0;
    if w == 0 || h == 0 || d.len() < 18 + w * h * 2 {
        return None;
    }
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        let src = if top { h - 1 - y } else { y };
        for x in 0..w {
            let o = 18 + (src * w + x) * 2;
            let px = expand16(u16::from_le_bytes([d[o], d[o + 1]]), false);
            rgba[(y * w + x) * 4..][..4].copy_from_slice(&px);
        }
    }
    Some((w, h, rgba))
}

/// `.ipd` — width(4) height(4) type(4) rgbformat(4), then RGB565.
fn decode_ipd(d: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    if d.len() < 16 {
        return None;
    }
    let w = u32::from_le_bytes(d[0..4].try_into().ok()?) as usize;
    let h = u32::from_le_bytes(d[4..8].try_into().ok()?) as usize;
    // Trust the header only when it accounts for the file exactly — these extensions are also
    // used for non-image data, and a wrong guess would produce convincing garbage.
    if w == 0 || h == 0 || w > 4096 || h > 4096 || d.len() < 16 + w * h * 2 {
        return None;
    }
    let mut rgba = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let o = 16 + i * 2;
        rgba[i * 4..][..4].copy_from_slice(&expand16(u16::from_le_bytes([d[o], d[o + 1]]), true));
    }
    Some((w, h, rgba))
}

/// Headerless RGB565, dimensions inferred from the file size (square, or 2:1).
fn decode_raw_rgb565(d: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let px = d.len() / 2;
    if d.len() % 2 != 0 || px == 0 {
        return None;
    }
    let side = (px as f64).sqrt() as usize;
    let (w, h) = if side * side == px {
        (side, side)
    } else if (px / 2) > 0 && (px / 2) * 2 == px && side > 0 && (side * 2) * (side / 2) == px {
        (side * 2, side / 2)
    } else {
        return None; // not a shape we can infer with confidence
    };
    let mut rgba = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let o = i * 2;
        rgba[i * 4..][..4].copy_from_slice(&expand16(u16::from_le_bytes([d[o], d[o + 1]]), true));
    }
    Some((w, h, rgba))
}

/// One vertex: screen position and texture coordinate, both already in pixels/texels.
struct Vertex {
    x: f32,
    y: f32,
    u: f32,
    w: f32,
}

/// A decoded texture, RGBA8.
struct Texture {
    w: usize,
    h: usize,
    rgba: Vec<u8>,
}

/// One registered vertex attribute array.
#[derive(Debug, Clone, Copy)]
struct VertexArray {
    size: usize,
    stride: usize,
    ptr: u32,
}

pub struct Region {
    pub name: &'static str,
    pub base: u32,
    pub data: Vec<u8>,
}

/// The region walk behind [`Memory::peek32`], as a free function so it can be tested against a
/// plain list of regions rather than a fully built machine.
pub fn peek_regions(regions: &[Region], addr: u32) -> Option<u32> {
    let a = addr & !3;
    // SDRAM has an uncached alias 0x04000000 above its cached view and the firmware uses both;
    // 0x14937194 and 0x10937194 are the same word, and an observer that resolved only one spelling
    // would report "unmapped" for the address this facility exists to watch.
    for candidate in [a, a ^ 0x0400_0000] {
        for r in regions {
            let Some(off) = candidate.checked_sub(r.base) else { continue };
            let off = off as usize;
            if off + 4 <= r.data.len() {
                return Some(u32::from_le_bytes(r.data[off..off + 4].try_into().ok()?));
            }
        }
    }
    None
}

/// How many executed addresses a PC trace keeps. Two hundred thousand is several full passes
/// through a 593-instruction flattened function -- enough to see the state sequence and where it
/// stops -- while costing a few megabytes rather than growing without bound over a long boot.
pub const PC_TRACE_CAP: usize = 200_000;

/// Resolution-cache granularity, and the number of slots in the direct-mapped cache. 65536 slots
/// covers 256 MB of distinct pages — comfortably more than the 64 MB of SDRAM plus IRAM, NOR and
/// the device windows, so the working set does not thrash.
const PAGE: u32 = 4096;
const FAST_SLOTS: usize = 1 << 16;

/// One cached page resolution: which region answers reads, and which answers writes.
///
/// They differ because `locate_write` skips read-only regions, so a page backed by NOR resolves for
/// reads and not for writes. `u32::MAX` means "no fast path" — either a device window overlaps the
/// page, or no single region covers it.
#[derive(Clone, Copy)]
struct FastPage {
    tag: u32,
    r: u32,
    w: u32,
}

impl FastPage {
    // Tag 1 is never a page base (pages are 4 KiB-aligned), so an empty slot can never match.
    const EMPTY: FastPage = FastPage { tag: 1, r: u32::MAX, w: u32::MAX };
}

impl Memory {
    /// Cheap reject bounds for `--storeaddr`, recomputed after the set is loaded.
    pub fn set_store_addr_bounds(&mut self) {
        self.store_addr_lo = self.store_addrs.first().copied().unwrap_or(u32::MAX);
        self.store_addr_hi = self.store_addrs.last().copied().unwrap_or(0);
        self.read_addr_lo = self.read_addrs.first().copied().unwrap_or(u32::MAX);
        self.read_addr_hi = self.read_addrs.last().copied().unwrap_or(0);
    }

    /// The `--storelog` hook. Placed where the *value* is still in hand: `count()` runs before the
    /// bytes land, so peeking the target there reports the value being overwritten, not the one
    /// being written.
    fn note_store_pc(&mut self, addr: u32, val: u32) {
        if (self.store_pcs.is_empty() && self.store_addrs.is_empty()) || self.internal {
            return;
        }
        // A word store that misses the fast path is re-issued as four byte stores. Logging those
        // too would report one `str` as five writes.
        if self.store_split > 0 {
            self.store_split -= 1;
            return;
        }
        let pc = self.pc;
        let by_addr = addr >= self.store_addr_lo
            && addr <= self.store_addr_hi
            && self.store_addrs.binary_search(&addr).is_ok();
        if by_addr || self.store_pcs.contains(&pc) {
            let n = self.icount;
            self.store_pc_log.push((pc, addr, val, n));
        }
    }

    pub fn region_named(&self, name: &str) -> Option<&Region> {
        self.regions.iter().find(|r| r.name == name)
    }

    /// Read a word out of backing memory **without touching anything**.
    ///
    /// Deliberately not `read32`: that one resolves through the fast-region cache, feeds the access
    /// counters and consults the device windows, so using it to observe a value would change the
    /// numbers every access report has ever produced — and would answer from a device rather than
    /// from memory. This walks the regions directly and answers only from plain backing storage.
    ///
    /// `None` when the address is not in a region, which includes every MMIO window. An observer
    /// that cannot see a value must say so rather than return zero, because zero is a meaningful
    /// value at the address this exists to watch.
    ///
    /// SDRAM has an uncached alias 0x40000000 above its cached view, and the firmware uses both —
    /// `0x14937194` and `0x10937194` are the same word. Both spellings resolve here.
    pub fn peek32(&self, addr: u32) -> Option<u32> {
        peek_regions(&self.regions, addr)
    }

    /// Whether every address in the 4 KiB page at `page` is answered by plain backing memory —
    /// no device window, no override, no instrumentation that needs the raw address.
    ///
    /// Checked per *page* rather than per access, so the cost is paid once and amortised over
    /// every access to that page. Every window `read8`/`write8` consults must appear here; a
    /// missing one would route a device access to memory and silently change behaviour, so the
    /// list is deliberately conservative and errs towards marking a page slow.
    fn page_is_plain(&self, page: u32) -> bool {
        let end = page.wrapping_add(PAGE - 1);
        let hits = |base: u32, size: u32| {
            // Overlap, not containment: a window clipping either edge disqualifies the page.
            base.wrapping_sub(page) < PAGE || page.wrapping_sub(base) < size
        };
        if self.read_toggle.iter().any(|&(at, _, _)| hits(at, 4))
            || self.read_overrides.iter().any(|&(at, _)| hits(at, 4))
            || self.read_or_masks.iter().any(|&(at, _)| hits(at, 4))
            || self.int_ack_on_read.iter().any(|&(at, _)| hits(at, 4))
            // The GPIO block. `write8_inner` counts the backlight dimmer's pulses on GPIOB and
            // retires port-A interrupts on a write of INT_CLR, and neither hook is reached if this
            // page is served from plain memory. Both were written and both did nothing until this
            // line existed -- which is the third time in one day that a mechanism present in the
            // source turned out never to be consulted, and the reason this list is the first place
            // to look when a device model has no effect.
            || hits(0x6000_d000, 0x200)
            // The DMA channel blocks, for the read-to-clear latch in `read8_inner`. Two pages
            // total, and the firmware touches them a few dozen times in a whole boot, so taking
            // them off the fast path costs nothing measurable.
            || PP_DMA.iter().any(|c| hits(c.chans, 0x1000))
            || self.usec_timer.is_some_and(|b| hits(b, 4))
            || self.bcm.as_ref().is_some_and(|b| hits(b.base, 0x8_0000))
            || self.ata.as_ref().is_some_and(|(b, _)| hits(*b, 0x410))
            || self.i2c_base.is_some_and(|b| hits(b, 0x40))
            // The click wheel's four registers sit at +0x100..+0x140 of the same block. With
            // `--pmu` in the recipe the I²C window above already disqualifies this page, so this
            // line changes nothing today — which is exactly why it has to be here: without it the
            // device would be silently bypassed the first time the wheel is run without I²C.
            || self.clickwheel.as_ref().is_some_and(|w| hits(w.base + ClickWheel::CTRL, ClickWheel::WINDOW - ClickWheel::CTRL))
            || self.mmap_base.is_some_and(|b| hits(b, 0x40))
            || self.xmb.as_ref().is_some_and(|x| hits(x.base, 0x40))
            // Only while the chip is answering something other than its own bytes. The NOR is a
            // megabyte the CPU fetches instructions out of at address 0, so disqualifying its pages
            // unconditionally would take the whole cold boot off the fast path to model a handful
            // of cycles; `take_mode_change` drops the cache on each transition instead.
            || self
                .nor
                .as_ref()
                .is_some_and(|n| n.intercepts() && n.windows.iter().any(|&(b, s)| hits(b, s)))
        {
            return false;
        }
        // A page that straddles an alias boundary would translate non-uniformly, so the fast path
        // cannot describe it with a single region and offset.
        self.translate(end) == self.translate(page).wrapping_add(PAGE - 1)
    }

    /// Resolve `addr`'s page to a region index once, then answer from a direct-mapped cache.
    ///
    /// The linear region scan and the alias scan were running on every *byte* — four times per
    /// 32-bit access, and every instruction fetch is one of those. This turns the common case into
    /// a tag compare.
    ///
    /// **It must agree with the slow path.** `locate_idx` takes the *first* region containing an
    /// address; if this cache picks a different one, reads and writes silently disagree.
    fn fast_region_doc_anchor(&self) {}

    /// Apply a finished flash operation to every region that holds the chip's bytes. Two windows
    /// means two copies, and an erase that updated only one would leave the aliases disagreeing.
    fn nor_commit(&mut self, op: NorOp) {
        let names = match &self.nor {
            Some(n) => n.regions.clone(),
            None => return,
        };
        for r in self.regions.iter_mut().filter(|r| names.contains(&r.name)) {
            op.apply(&mut r.data);
        }
    }

    /// Record a completed store for `--writelog`. `region` is the region that took it, or None.
    fn note_write(&mut self, addr: u32, val: u32, region: Option<usize>) {
        if let Some((base, size)) = self.write_log {
            if addr.wrapping_sub(base) < size {
                let name = region.map_or("DROPPED", |i| self.regions[i].name);
                let pc = self.pc;
                // The tally first, and outside the cap: "how many stores were DROPPED" is the whole
                // question this instrument is asked, and a capped answer to it is a floor.
                *self.write_log_regions.entry(name).or_insert(0) += 1;
                self.write_log_entries.push((pc, addr, val, name));
            }
        }
    }

    fn fast_region(&mut self, addr: u32, write: bool) -> Option<(usize, usize)> {
        let page = addr & !(PAGE - 1);
        let slot = ((page >> 12) as usize) & (FAST_SLOTS - 1);
        let e = self.fast[slot];
        let idx = if e.tag == page {
            if write { e.w } else { e.r }
        } else {
            let (r, w) = if self.page_is_plain(page) {
                let t = self.translate(page);
                let contains = |r: &Region| (t.wrapping_sub(r.base) as usize) < r.data.len();
                // Whichever region the SLOW path would choose is the only correct answer, so pick
                // that one and then decide whether it is cacheable — never search past it for a
                // region that happens to hold a whole page.
                //
                // Searching on was a silent data-corruption bug. Two regions share base 0 (the
                // firmware image and SDRAM), and the image's last partial page — 0xba000..0xbaee4 —
                // cannot hold a full 4 KB, so every read there fell through to SDRAM and returned
                // **zeros instead of firmware**. Rockbox's `.init` copy is what exposed it: the copy
                // stored faithfully, but the last 0xfb8 bytes it was handed were zeros, so the
                // linker veneers at the end of `.init` never arrived and it executed into them.
                // Nothing was ever reported unmapped, because the wrong region answered rather than
                // no region at all.
                let cacheable = |i: usize| {
                    let r = &self.regions[i];
                    let off = t.wrapping_sub(r.base) as usize;
                    if r.data.len() - off >= PAGE as usize { i as u32 } else { u32::MAX }
                };
                let rd = self.regions.iter().position(contains).map_or(u32::MAX, cacheable);
                let wr = self
                    .regions
                    .iter()
                    .position(|r| contains(r) && !self.readonly.contains(&r.name))
                    .map_or(u32::MAX, cacheable);
                (rd, wr)
            } else {
                (u32::MAX, u32::MAX)
            };
            self.fast[slot] = FastPage { tag: page, r, w };
            if write { w } else { r }
        };
        // Agreement with the slow path is the invariant; check it rather than trust it.
        if self.verify_memory && idx != u32::MAX {
            let t = self.translate(addr);
            let slow = if write {
                self.regions.iter().position(|r| {
                    (t.wrapping_sub(r.base) as usize) < r.data.len() && !self.readonly.contains(&r.name)
                })
            } else {
                self.regions.iter().position(|r| (t.wrapping_sub(r.base) as usize) < r.data.len())
            };
            if slow != Some(idx as usize) {
                let fast_name = self.regions[idx as usize].name;
                let slow_name = slow.map_or("(none)", |i| self.regions[i].name);
                let pc = self.pc;
                self.verify_mismatches.push((pc, addr, fast_name, slow_name));
            }
        }
        if idx == u32::MAX {
            return None;
        }
        let base = self.regions[idx as usize].base;
        let off = self.translate(addr).wrapping_sub(base) as usize;
        Some((idx as usize, off))
    }

    /// Recompute aliases from the MMAP windows. Called when the firmware programs one.
    ///
    /// The encoding is Rockbox's, from `firmware/target/arm/pp/crt0-pp.S`:
    ///
    /// ```text
    /// LOGICAL  = base<31:16> | mask<13:4>    mask bit m compares address bit m+16
    /// PHYSICAL = base<31:16> | flags<11:8>   READ | WRITE | DATA | CODE
    /// ```
    ///
    /// Two independent confirmations that this is the real split. Rockbox parameterises only the
    /// LOGICAL half by memory size (`MMAP_MASK` is `0x3c00` for 64 MB, `0x3e00` for 32 MB — one
    /// more compared bit, half the window), so the size lives there and nowhere else. And its
    /// PHYSICAL half differs by *part*, not by size: `MMAP_FLAGS` is `0x3f84` on PP5002 and
    /// `0x0f84` on PP502x. RetailOS writes `0x0f84`, which is simply what a PP5021C takes.
    ///
    /// Address bits 31:30 are compared unconditionally. Rockbox's `crt0` copies itself to IRAM at
    /// 0x40000000 and runs there *while* programming a window based at 0; it survives, which it
    /// could not if that window also claimed 0x40000000.
    ///
    /// A mask bit left clear above the window's own size is a **don't care**, so one window can
    /// answer for several disjoint ranges. RetailOS's SDRAM window leaves address bit 26 clear,
    /// which is why it covers 0x04000000..0x06000000 as well as 0x00000000..0x02000000 — and why
    /// modelling it as one flat 64 MB range left RetailOS reading into nothing. Each such range
    /// becomes its own alias, an alias being a contiguous remap.
    ///
    /// Base aliases (the uncached window) are kept; MMAP windows are appended. Identity mappings
    /// are skipped — that is how a window is turned *off*, and installing one would be a no-op
    /// that only costs a scan.
    fn rebuild_mmap_aliases(&mut self) {
        self.aliases.truncate(self.mmap_alias_floor);
        for w in 0..8 {
            let logical = self.mmap_regs[w * 2];
            let physical = self.mmap_regs[w * 2 + 1];
            // A window is only live once both halves have been written.
            if logical == 0 && physical == 0 {
                continue;
            }
            let mask = (logical & 0x3ff0) << 16;
            let tested = 0xc000_0000 | mask;
            let (lb, pb) = (logical & tested, physical & tested);
            if lb == pb {
                continue;
            }
            // The window spans everything below its lowest compared bit.
            let size = if mask == 0 { 0x4000_0000 } else { mask & mask.wrapping_neg() };
            // Above that, compared bits are fixed and uncompared ones are free; every combination
            // of the free ones is another range this window answers for.
            let free = !tested & 0x3fff_ffff & !(size - 1);
            let mut sub = free;
            loop {
                self.aliases.push((lb | sub, size, pb | sub));
                if sub == 0 {
                    break;
                }
                sub = (sub - 1) & free;
            }
        }
        self.invalidate_fast();
    }

    /// Drop every cached page resolution. Must be called whenever regions, aliases, overrides or
    /// device mappings change — otherwise a stale entry answers for memory that has moved.
    pub fn invalidate_fast(&mut self) {
        for e in self.fast.iter_mut() {
            *e = FastPage::EMPTY;
        }
    }

    fn locate(&mut self, addr: u32) -> Option<(&mut [u8], usize)> {
        let addr = self.translate(addr);
        for r in &mut self.regions {
            let off = addr.wrapping_sub(r.base) as usize;
            if off < r.data.len() {
                return Some((&mut r.data, off));
            }
        }
        None
    }

    /// Locate for a store: skips read-only regions so the write reaches the memory behind them.
    fn locate_write(&mut self, addr: u32) -> Option<(&mut [u8], usize)> {
        let addr = self.translate(addr);
        let ro = std::mem::take(&mut self.readonly);
        let mut found = None;
        for (i, r) in self.regions.iter().enumerate() {
            let off = addr.wrapping_sub(r.base) as usize;
            if off < r.data.len() && !ro.contains(&r.name) {
                found = Some((i, off));
                break;
            }
        }
        self.readonly = ro;
        found.map(move |(i, off)| (&mut self.regions[i].data[..], off))
    }

    /// Index of the region answering `addr`, for access accounting.
    fn locate_idx(&self, addr: u32) -> Option<usize> {
        let addr = self.translate(addr);
        self.regions
            .iter()
            .position(|r| (addr.wrapping_sub(r.base) as usize) < r.data.len())
    }

    /// Per-region access counts, busiest first — which devices the firmware is driving.
    ///
    /// A PC profile cannot answer "is it talking to the LCD yet?", because a device is identified by
    /// the address it answers on, not by the code that touches it. This can.
    pub fn device_report(&self) -> Vec<String> {
        let mut rows: Vec<(usize, u64, u64)> = (0..self.regions.len())
            .map(|i| {
                (
                    i,
                    self.region_reads.get(i).copied().unwrap_or(0),
                    self.region_writes.get(i).copied().unwrap_or(0),
                )
            })
            .filter(|&(_, r, w)| r + w > 0)
            .collect();
        rows.sort_by_key(|&(_, r, w)| std::cmp::Reverse(r + w));
        rows.iter()
            .map(|&(i, r, w)| {
                let reg = &self.regions[i];
                format!("{:<12} {:#010x}  {r:>12} reads {w:>12} writes", reg.name, reg.base)
            })
            .collect()
    }

    /// Account for an access whose region index is already known — the scan is not repeated.
    fn bump(&mut self, idx: usize, write: bool) {
        let v = if write { &mut self.region_writes } else { &mut self.region_reads };
        if v.len() <= idx {
            v.resize(idx + 1, 0);
        }
        v[idx] += 1;
    }

    /// `wval` is the byte **being written**, and is meaningless on a read.
    ///
    /// It has to be passed in because `count` runs ahead of both the device models and the region
    /// copy, so peeking the address here answers with the value that is about to be replaced. On
    /// plain storage that is merely off by one write; on MMIO it is a fabrication, because the
    /// device consumes the store and the backing bytes stay zero forever. That is what
    /// `--watch-writes=0xc3000028:4` reported as "3 824 logged, 0 distinct pc" — every entry read
    /// back as zero and the report drops zeros as memset noise.
    fn count(&mut self, addr: u32, write: bool, wval: u8) {
        if !write
            && !self.internal
            && addr >= self.read_addr_lo
            && addr <= self.read_addr_hi
            && self.read_addrs.binary_search(&addr).is_ok()
        {
            // The byte is peeked rather than taken from the read in flight: `count` runs ahead of
            // the device models, and a log without the value answers "who looked" but not "at
            // what", which is the half that matters.
            let (pc, n) = (self.pc, self.icount);
            // The per-reader tally is outside the cap. This is the instrument whose 2 000 000-entry
            // log turned a control read 9 588 012 times into `--- reads of watched addresses: 0 ---`
            // for four fifths of a run; the count must not be able to stop.
            let e = self.read_sites.entry((addr, pc)).or_insert((0, n));
            e.0 += 1;
            self.read_log.push((pc, addr, 0, n));
        }
        if write {
            if let Some((base, len)) = self.watch_range {
                if !self.internal && addr.wrapping_sub(base) < len {
                    let (pc, v, n) = (self.pc, wval, self.icount);
                    let e = self.watch_range_words.entry(addr & !3).or_insert(WatchWord {
                        first_at: n,
                        ..Default::default()
                    });
                    e.writes += 1;
                    *e.pcs.entry(pc).or_insert(0) += 1;
                    self.watch_range_log.push((pc, addr, v as u32, 0));
                }
            }
        }
        if let Some((base, size)) = self.input_probe {
            if !self.internal && addr.wrapping_sub(base) < size {
                let pc = self.pc;
                let e = self.input_regs.entry(addr & !3).or_insert((0, 0, pc));
                if write { e.1 += 1 } else if e.1 == 0 { e.0 += 1 }
            }
        }
        // The emulator's own device servicing must not appear in a report about what the
        // *firmware* is driving. Left unguarded, `service_interrupts` alone accounts for roughly
        // half of every access to the 0x60000000 region and the report reads as a firmware hot
        // loop — a tool measuring itself.
        if self.internal {
            return;
        }
        // The page log is the only part that needs the raw address; the region attribution below
        // costs a full scan of the region list, so it is skipped unless something asked for it.
        if !self.accounting {
            if let Some((base, size)) = self.page_log {
                if addr.wrapping_sub(base) < size {
                    let e = self.page_counts.entry(addr & !(self.page_gran - 1)).or_insert((0, 0));
                    if write { e.1 += 1 } else { e.0 += 1 }
                }
            }
            return;
        }
        if let Some((base, size)) = self.page_log {
            if addr.wrapping_sub(base) < size {
                let e = self.page_counts.entry(addr & !(self.page_gran - 1)).or_insert((0, 0));
                if write { e.1 += 1 } else { e.0 += 1 }
            }
        }
        if let Some(i) = self.locate_idx(addr) {
            let v = if write { &mut self.region_writes } else { &mut self.region_reads };
            if v.len() <= i {
                v.resize(i + 1, 0);
            }
            v[i] += 1;
        }
    }

    /// Resolve an alias to the memory it is a view of.
    ///
    /// An alias is not a second buffer — it is the *same* storage seen at another address. Modelling
    /// one as a separate zeroed region appears to work for as long as the firmware writes and reads
    /// through a single view, then silently diverges the first time it crosses between them, and the
    /// symptom looks like memory corruption rather than like a mapping bug.
    /// Translation is two levels, because the hardware has two. The MMAP unit resolves a logical
    /// address to a physical one and does **not** chain — a window's output is not fed back through
    /// the windows, so 0x20000000 -> 0 -> 0x10000000 stops at the first hop. Downstream of it the
    /// memory controller decodes fewer address bits than the bus carries, which is what makes SDRAM
    /// appear more than once; a window whose don't-care bits push an address into one of those
    /// mirrors has to land somewhere real.
    pub fn translate(&self, addr: u32) -> u32 {
        let mut a = addr;
        for &(base, size, target) in &self.aliases[self.mmap_alias_floor..] {
            let off = a.wrapping_sub(base);
            if off < size {
                a = target.wrapping_add(off);
                break;
            }
        }
        for &(base, size, target) in &self.aliases[..self.mmap_alias_floor] {
            let off = a.wrapping_sub(base);
            if off < size {
                return target.wrapping_add(off);
            }
        }
        a
    }
}

impl Memory {
    /// Record one unmapped access against its page.
    fn note_unmapped(&mut self, addr: u32, write: bool) {
        let pc = self.pc;
        self.unmapped_seq += 1;
        let e = self.unmapped.entry(addr & !0xfff).or_insert(UnmappedPage {
            lo: addr,
            hi: addr,
            first_pc: pc,
            ..Default::default()
        });
        if write {
            e.writes += 1;
        } else {
            e.reads += 1;
        }
        e.lo = e.lo.min(addr);
        e.hi = e.hi.max(addr);
        *e.pcs.entry(pc).or_insert(0) += 1;
    }

    /// Read a byte without counting it or recording it as a finding — for the emulator's own use.
    fn peek(&mut self, addr: u32) -> u8 {
        match self.locate(addr) {
            Some((buf, i)) => buf[i],
            None => 0,
        }
    }

    /// Total unmapped reads and writes across every page.
    pub fn unmapped_totals(&self) -> (u64, u64) {
        self.unmapped
            .values()
            .fold((0, 0), |(r, w), p| (r + p.reads, w + p.writes))
    }

    /// One line per page touched, busiest first — the form worth printing at the end of a run.
    pub fn unmapped_report(&self) -> Vec<String> {
        let mut pages: Vec<_> = self.unmapped.values().collect();
        pages.sort_by_key(|p| std::cmp::Reverse(p.reads + p.writes));
        let mut out = Vec::new();
        for p in pages {
            out.push(format!(
                "{:#010x}..{:#010x}  {:>8} reads {:>8} writes   first pc {:#010x}",
                p.lo, p.hi, p.reads, p.writes, p.first_pc
            ));
            let mut by_pc: Vec<_> = p.pcs.iter().collect();
            by_pc.sort_by_key(|&(pc, n)| (std::cmp::Reverse(*n), *pc));
            for (pc, n) in by_pc.iter().take(8) {
                out.push(format!("      pc {pc:#010x}  x{n}"));
            }
            // Say what was left out rather than trimming in silence.
            if by_pc.len() > 8 {
                out.push(format!("      … and {} more PCs", by_pc.len() - 8));
            }
        }
        out
    }
}

impl Bus for Memory {
    /// A 32-bit load resolved once instead of four times.
    ///
    /// The default `Bus::read32` is four `read8` calls, and each of those walked the alias list,
    /// every device window and the region list. Every instruction fetch is one of these, so that
    /// was the interpreter's dominant cost — a host profile put every sample in this path and none
    /// in instruction decode.
    ///
    /// `count()` is still called per byte, deliberately: it feeds the access reports, and folding
    /// four byte-accesses into one word-access would quietly change every number those reports have
    /// ever produced. The saving here is address resolution, not accounting.
    fn read32(&mut self, addr: u32) -> u32 {
        let a = addr & !3;
        if let Some((idx, off)) = self.fast_region(a, false) {
            // `count` is a no-op unless something asked for accounting, so hoist that test out of
            // the four calls rather than making them and returning immediately from each.
            // `input_probe` is in the list because it consumes `count` too — the hoist has to name
            // every consumer or it silences one. See the write path below for what that cost.
            //
            // This half cost its own wrong conclusion, separately from the write half. `--input-regs`
            // answers "which addresses does the firmware read that nothing ever wrote", and with
            // `input_probe` missing here it counted only byte reads: on the retail boot it saw
            // 44 510 reads of PROCESSOR_ID against the true 128 150, and missed CPU_INT_STAT's
            // 167 264 outright. research/09's register table was built on that.
            if self.accounting
                || self.page_log.is_some()
                || !self.read_addrs.is_empty()
                || self.input_probe.is_some()
            {
                for i in 0..4 {
                    self.count(a.wrapping_add(i), false, 0);
                }
            }
            let d = &self.regions[idx].data[off..off + 4];
            return u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
        }
        u32::from_le_bytes([
            self.read8(a),
            self.read8(a.wrapping_add(1)),
            self.read8(a.wrapping_add(2)),
            self.read8(a.wrapping_add(3)),
        ])
    }

    fn write32(&mut self, addr: u32, val: u32) {
        self.note_store_pc(addr, val);
        // CPU_CTRL. Bit 31 is the core asking to be switched off until an interrupt, and RetailOS's
        // idle task writes it in a tight loop — it is the second-hottest instruction in the resting
        // state. Unmodelled, the idle task spun at full speed, so a 2 G-instruction budget bought
        // ~400 s of simulated time and `--clock` silently changed simulated-seconds-per-unit-work.
        // Recorded rather than acted on here because only `Machine` knows the timer deadlines.
        if addr & !3 == CPU_CTRL && val & 0x8000_0000 != 0 {
            self.cpu_sleep = true;
        }
        let a = addr & !3;
        if let Some((idx, off)) = self.fast_region(a, true) {
            // `watch_range` and `input_probe` were missing from this hoist, and `count` is the only
            // thing that feeds them — so `--watch-range` saw *byte* writes (write8_inner calls
            // `count` unconditionally) and no word writes at all, unless some other flag happened to
            // arm the path. That is not a hypothetical: it is why research/10 Addendum 7 §5
            // concluded the transfer engine at 0x60009000 "is never programmed". It is programmed —
            // 208 byte-writes' worth — and the whole-run watch that reported 222 GPIO writes and
            // nothing else had been blind to every one of them.
            //
            // The blindness applied only to addresses a *region* answers; a device window goes
            // through write8, which calls `count` unconditionally. That asymmetry is why it survived
            // so long — most MMIO looked fine, and SDRAM was where it hid. Measured on two heap
            // records over a whole retail boot, pre-fix against post-fix on the same machine:
            // 212 byte-writes reported against 3 040 actual on one, and on the other 22 against 670,
            // with 23 of its 29 words reported as never written when every one of them was.
            // research/09's two retractions are that measurement.
            if self.accounting
                || self.page_log.is_some()
                || !self.read_addrs.is_empty()
                || self.watch_range.is_some()
                || self.input_probe.is_some()
            {
                for i in 0..4 {
                    self.count(a.wrapping_add(i), true, val.to_le_bytes()[i as usize]);
                }
            }
            self.regions[idx].data[off..off + 4].copy_from_slice(&val.to_le_bytes());
            if self.write_log.is_some() {
                self.note_write(a, val, Some(idx));
            }
            return;
        }
        let b = val.to_le_bytes();
        self.store_split = 4;
        for (i, v) in b.iter().enumerate() {
            self.write8(a.wrapping_add(i as u32), *v);
        }
        self.store_split = 0;
    }

    fn read8(&mut self, addr: u32) -> u8 {
        // Account for the access before any device model can return early. Doing this further down
        // meant every modelled device — ATA, the BCM, the timers — was invisible to the device
        // report: `ide` showed 8 reads while the drive was handing over a megabyte, and the whole
        // "the data never moves" conclusion rested on it.
        //
        // The *value*, though, has to be recorded on the way out: `count` runs ahead of every
        // device model, so peeking the target here reports the backing region rather than what the
        // drive answered. Reading it there made STATUS look permanently 0x00 — an artefact of the
        // instrument, caught only because "every register always reads zero" is too tidy to be true.
        // `sample()`, not the census: once the log saturates the push is dropped and the last kept
        // row belongs to a different read, so amending it would overwrite a real measurement.
        let logged = self.read_log.sample().len();
        self.count(addr, false, 0);
        let v = self.read8_inner(addr);
        if self.read_log.sample().len() > logged {
            if let Some(e) = self.read_log.last_mut() {
                e.2 = v;
            }
        }
        v
    }

    fn write8(&mut self, addr: u32, val: u8) {
        self.write8_inner(addr, val)
    }
}

impl Memory {
    fn read8_inner(&mut self, addr: u32) -> u8 {
        // Ahead of `locate`, or the zeroed MMIO region would answer first and the clock would
        // still read as stopped.
        for i in 0..self.read_toggle.len() {
            let (at, a, b) = self.read_toggle[i];
            let off = addr.wrapping_sub(at);
            if off < 4 {
                if self.toggle_state.len() <= i {
                    self.toggle_state.resize(i + 1, false);
                }
                let v = if self.toggle_state[i] { b } else { a };
                // Flip once the whole word has been read, so a 32-bit load sees one consistent value.
                if off == 3 {
                    self.toggle_state[i] = !self.toggle_state[i];
                }
                return v.to_le_bytes()[off as usize];
            }
        }
        for &(at, v) in &self.read_overrides {
            let off = addr.wrapping_sub(at);
            if off < 4 {
                return v.to_le_bytes()[off as usize];
            }
        }
        for &(at, mask) in &self.read_or_masks {
            let off = addr.wrapping_sub(at);
            if off < 4 {
                // Whatever the register holds, with the claimed bits asserted -- and nothing else
                // disturbed.
                let base = peek_regions(&self.regions, at).unwrap_or(0);
                return (base | mask).to_le_bytes()[off as usize];
            }
        }
        if let Some(base) = self.usec_timer {
            let off = addr.wrapping_sub(base);
            if off < 4 {
                return self.usec.to_le_bytes()[off as usize];
            }
        }
        if let Some(b) = &mut self.bcm {
            let off = addr.wrapping_sub(b.base);
            if off < 0x8_0000 {
                return b.read8(off);
            }
        }
        // In read-array mode this answers `None` and the backing region serves the byte, which is
        // the case for all but a few thousand instructions of a boot.
        if let Some(n) = &self.nor {
            if let Some(v) = n.hit(addr).and_then(|off| n.read(off)) {
                return v;
            }
        }
        if let Some((base, dev)) = &mut self.ata {
            let off = addr.wrapping_sub(*base);
            if off < 0x410 {
                let mut v = dev.read(off);
                // Ledger #9's arm B. `Ata::read` reports the pending latch in IDE0_CFG bit 3; this
                // takes it back out again, at the one place every read of the controller passes.
                if off == 0x28 && self.ide_irq_latch_off {
                    v &= !0x08;
                }
                // Reading the primary status register acknowledges the drive's interrupt — that is
                // ATA's own convention, not ours. The alternate status at +0x3f8 deliberately does
                // not, which is the whole reason it exists.
                if (0x1fc..0x200).contains(&off) {
                    if self.int_pending & (1 << IDE_IRQ) != 0 {
                        self.ide_irq_acked += 1;
                    }
                    self.int_pending &= !(1 << IDE_IRQ);
                }
                return v;
            }
        }
        // With a modelled PMU the data registers are backing memory, written when a transfer
        // completes — so there is nothing to intercept here and the fixed fill must not apply.
        if self.pmu.is_some() {
            if let Some(base) = self.i2c_base {
                if addr.wrapping_sub(base + 0x0c) < 0x10 {
                    return self.peek(addr);
                }
            }
        }
        if let (Some(base), Some(fill)) = (self.i2c_base, self.i2c_fill) {
            if addr.wrapping_sub(base + 0x0c) < 0x10 {
                return fill;
            }
        }
        // Ahead of `--i2c-fill` deliberately: that flag answers the I²C *data* registers, and the
        // wheel is a different device that happens to share the block. Behind the PMU for the same
        // reason — neither can claim the other's addresses.
        if let Some(w) = &mut self.clickwheel {
            let off = addr.wrapping_sub(w.base);
            if (ClickWheel::CTRL..ClickWheel::WINDOW).contains(&off) {
                if let Some(v) = w.read8(off) {
                    return v;
                }
            }
        }
        if let Some(base) = self.mmap_base {
            let off = addr.wrapping_sub(base);
            if off < 0x40 {
                return self.mmap_regs[(off / 4) as usize].to_le_bytes()[(off % 4) as usize];
            }
        }
        for &(at, bit) in &self.int_ack_on_read {
            if addr.wrapping_sub(at) < 4 {
                self.int_pending &= !bit;
                return 0;
            }
        }
        // A DMA channel's `STATUS` interrupt latch is read-to-clear, and Rockbox's FIQ handler is
        // the whole specification: its first statement is a bare `DMA0_STATUS;` carrying the
        // comment "Clear any pending interrupt". RetailOS agrees by omission — its ISR at
        // `0x001d9be0` loads `[chan+0x04]`, tests bit 30, dispatches, and never writes the
        // register back, so a latch that needed an explicit acknowledgement would leave that ISR
        // re-entering itself forever. It measurably did: 132 725 reads from `0x001d9bf4` in one
        // run, all of them the same completion.
        //
        // Only byte 3 clears it. A word read arrives here as four independent byte reads, so
        // clearing on the first would hand the caller a status with bit 30 already gone — and
        // bit 30 lives in byte 3 anyway, which is the last of the four.
        if !self.internal && addr & 3 == 3 {
            let hit = PP_DMA.iter().any(|c| {
                let off = addr.wrapping_sub(c.chans);
                off < 0x20 * c.n && off % 0x20 == DMA_STATUS + 3
            });
            if hit {
                if let Some((buf, i)) = self.locate(addr) {
                    let b = buf[i];
                    buf[i] = b & !((DMA_STATUS_INTR >> 24) as u8);
                    return b;
                }
            }
        }
        match self.locate(addr) {
            Some((buf, i)) => buf[i],
            None => {
                self.note_unmapped(addr, false);
                0
            }
        }
    }

    /// Move a GPIO port-A input pin, and raise the port's interrupt if the firmware asked for it.
    ///
    /// The level test is *match*, not edge: `INT_LEV` says which state a pin has to be in for the
    /// enabled bit to assert, so a switch that moves to the armed level interrupts and one that
    /// moves away from it does not. That is what `ipodloader2`'s `outb(~state, ..._INT_LEV)` is
    /// doing — arming for the opposite of what it just read, so the next movement fires.
    pub fn set_gpioa_input(&mut self, v: u32) {
        let old = self.read32(GPIOA_INPUT_VAL);
        if old == v {
            return;
        }
        self.write32(GPIOA_INPUT_VAL, v);
        let fire = (old ^ v) & self.read32(GPIOA_INT_EN) & !(v ^ self.read32(GPIOA_INT_LEV));
        if fire != 0 {
            let stat = self.read32(GPIOA_INT_STAT) | fire;
            self.write32(GPIOA_INT_STAT, stat);
            self.int_pending_hi |= 1 << GPIO_IRQ_HI;
        }
    }

    /// Arm the drive's completion for `IDE_COMPLETION_USEC` from now, rather than raising it here.
    ///
    /// The delay is load-bearing, not cosmetic. This model finishes a transfer inside the store to
    /// the command register, so a synchronously-raised completion is already asserted when the
    /// driver, twelve instructions later, writes IDE0_CFG's clear bits as part of arming its wait —
    /// and the acknowledgement lands on an interrupt whose handler has not run. Measured: RetailOS
    /// then sits out a full 10.24 s timeout on every `READ DMA`, while `SET FEATURES` — whose
    /// acknowledgement happens 208 instructions later, far enough for a service tick to slip in —
    /// completes immediately. Real drives take milliseconds; the driver arms first because it
    /// always has time to.
    ///
    /// Only the interrupt-controller line is delayed. `Ata::irq_pending`, the IDE0_CFG bit 3 latch,
    /// is still set inside the command so Apple's bootloader — which polls that bit with interrupts
    /// masked — sees exactly what it saw before, and is re-set here so a driver that clears it
    /// while arming still finds it set when its handler looks.
    fn arm_ide_irq(&mut self) {
        // A second completion arriving before the first is due must not swallow it — Apple's
        // bootloader issues back-to-back commands closer together than the delay, and a dropped
        // completion there costs a whole transfer.
        if self.ide_irq_due.is_some() {
            self.fire_ide_irq();
        }
        self.ide_irq_due = Some(self.usec.wrapping_add(IDE_COMPLETION_USEC));
        self.ide_irq_raised += 1;
    }

    /// Assert the drive's completion: the controller latch the driver reads back, plus both
    /// interrupt-controller lines it enabled for the drive.
    pub fn fire_ide_irq(&mut self) {
        self.ide_irq_due = None;
        self.int_pending |= 1 << IDE_IRQ;
        self.int_pending_hi |= 1 << IDE_DMA_IRQ_HI;
        if let Some((_, dev)) = &mut self.ata {
            dev.irq_pending = true;
        }
    }

    fn write8_inner(&mut self, addr: u32, val: u8) {
        self.note_store_pc(addr, val as u32);
        self.count(addr, true, val);
        if let Some(b) = &mut self.bcm {
            let off = addr.wrapping_sub(b.base);
            if off < 0x8_0000 {
                b.write8(off, val);
                return;
            }
        }
        // Ahead of `locate_write`, which would otherwise report the store as unmapped — the flash
        // regions are read-only and nothing else answers at address 0.
        let nor = match &mut self.nor {
            Some(n) => n
                .hit(addr)
                .map(|off| (n.write(off, val), n.take_mode_change())),
            None => None,
        };
        if let Some((op, mode_changed)) = nor {
            if mode_changed {
                self.invalidate_fast();
            }
            if let Some(op) = op {
                self.nor_commit(op);
            }
            return;
        }
        // 0x410, not 0x400: the DMA engine sits at +0x400..+0x410, and a window that stopped short
        // of it let every descriptor write fall through into the backing region, where it was
        // stored and never read. That silence was indistinguishable from a driver that does not
        // program DMA at all.
        let ata_hit = match &mut self.ata {
            Some((base, dev)) if addr.wrapping_sub(*base) < 0x410 => {
                let off = addr.wrapping_sub(*base);
                dev.write(off, val);
                // The drive latches completion inside this very store. Take the latch here and let
                // `arm_ide_irq` put it back when the completion is due, so IDE0_CFG bit 3 and the
                // interrupt line move together and neither can report a transfer that has not
                // plausibly finished yet. The PIO read path in `read8_inner` is left alone: Apple's
                // bootloader drives it by polling with interrupts masked, and nothing there races.
                let latched = std::mem::replace(&mut dev.irq_pending, false);
                Some((dev.dma_ready.take(), latched))
            }
            _ => None,
        };
        // IDE0_CFG's clear bits acknowledge the controller's interrupt latch, and that latch is
        // what drives IRQ 23 — so the write has to drop the interrupt-controller line as well as
        // the bit the driver reads back. Apple's bootloader polls the latch with IRQs masked, so
        // it never needed this; RetailOS's handler writes the same bits with the line live, and
        // without it a held line would survive the very ack that was meant to clear it.
        // Writing a bit to GPIOA_INT_CLR retires that pin's interrupt, and the shared port line
        // drops only when no pin is left asserting. Ports A..D are one interrupt, so a handler
        // clearing hold must not silence a wheel edge that arrived while it ran.
        // The backlight dimmer counts pulses on this pin; nothing reads the level back, so if the
        // emulator does not count them with the firmware, the level exists nowhere at all.
        if addr & !3 == BACKLIGHT_PORT {
            let shift = (addr & 3) * 8;
            let word =
                (self.read32(BACKLIGHT_PORT) & !(0xffu32 << shift)) | ((val as u32) << shift);
            let usec = self.usec;
            self.backlight.port_written(word, usec);
        }
        if addr & !3 == GPIOA_INT_CLR {
            let shift = (addr & 3) * 8;
            let stat = self.read32(GPIOA_INT_STAT) & !((val as u32) << shift);
            self.write32(GPIOA_INT_STAT, stat);
            if stat == 0 {
                self.int_pending_hi &= !(1 << GPIO_IRQ_HI);
            }
        }
        if let Some((base, _)) = &self.ata {
            if addr.wrapping_sub(*base) == 0x28 && val & 0x30 != 0 && !self.ide_cfg_ack_off {
                self.int_pending &= !(1 << IDE_IRQ);
                self.int_pending_hi &= !(1 << IDE_DMA_IRQ_HI);
            }
        }
        // The outbound half of DMA: the drive staged a WRITE and needs the bytes, which only the
        // bus can read out of memory.
        if let Some((base, dev)) = &mut self.ata {
            if let Some((src, len, lba)) = dev.dma_fetch.take() {
                let _ = base;
                let mut data = Vec::with_capacity(len as usize);
                for i in 0..len {
                    data.push(self.read8(src.wrapping_add(i)));
                }
                if let Some((_, dev)) = &mut self.ata {
                    dev.commit_write(lba, &data);
                }
                self.arm_ide_irq();
            }
        }
        if let Some((pending, latched)) = ata_hit {
            // The engine writes straight into SDRAM; going through locate_write means a transfer
            // aimed at NOR is refused rather than silently corrupting the flash image.
            if let Some((dest, data)) = pending {
                // A byte the DMA engine cannot place is a byte the driver will never see, and the
                // transfer log reports what was staged rather than what landed. Counting the
                // difference is the only way a silently-dropped transfer is distinguishable from a
                // successful one.
                let mut dropped = 0u64;
                for (i, b) in data.iter().enumerate() {
                    if let Some((buf, j)) = self.locate_write(dest.wrapping_add(i as u32)) {
                        buf[j] = *b;
                    } else {
                        dropped += 1;
                    }
                }
                if dropped > 0 {
                    self.dma_dropped += dropped;
                    self.dma_drop_sites.push((dest, dropped));
                }
                // The bus-master engine's completion is modelled as a *second* line as well as the
                // drive's INTRQ: RetailOS's ATA driver enables bit 23 in both interrupt banks
                // twenty instructions apart (0x001fc9a4 -> CPU_INT_EN, 0x00233768 -> CPU_HI_INT_EN).
                // Asserting only the first changed nothing measurable, so the second is asserted
                // too rather than left as an untested guess about which line the driver waits on.
                self.arm_ide_irq();
            }
            // A drive raises IRQ 23 when a command completes. Without it the driver issues
            // IDENTIFY, gets a correct answer, and then waits forever for a completion that
            // never arrives — which looks exactly like a drive that is not responding. The latch
            // the command set is the signal that it completed, so that is what is keyed on rather
            // than the address written.
            if latched {
                self.arm_ide_irq();
            }
            return;
        }
        // Before the I²C transfer trigger below, which only claims `base + 0x00`; these four
        // registers are 0x100 higher and belong to the wheel, not to the bus controller.
        if let Some(mut w) = self.clickwheel.take() {
            let off = addr.wrapping_sub(w.base);
            let (n, t) = (self.icount, self.usec);
            let owned =
                (ClickWheel::CTRL..ClickWheel::WINDOW).contains(&off) && w.write8(off, val, n, t);
            self.clickwheel = Some(w);
            if owned {
                return;
            }
        }
        if let Some(base) = self.mmap_base {
            let off = addr.wrapping_sub(base);
            if off < 0x40 {
                let (w, byte) = ((off / 4) as usize, off % 4);
                let mut b = self.mmap_regs[w].to_le_bytes();
                b[byte as usize] = val;
                self.mmap_regs[w] = u32::from_le_bytes(b);
                self.rebuild_mmap_aliases();
                return;
            }
        }
        // A transfer starts when CTRL's SEND bit is written; sample the request as it goes out.
        if let Some(base) = self.i2c_base {
            if addr == base && val & 0x80 != 0 {
                let dev = self.peek(base + 4);
                let d = [
                    self.peek(base + 0x0c),
                    self.peek(base + 0x10),
                    self.peek(base + 0x14),
                    self.peek(base + 0x18),
                ];
                // The tally is the census and the log is the sample. Both, always — the report's
                // by-device and by-register breakdowns were built from the log until 2026-08-14,
                // and the standard baseline fills that log at exactly 4 096, so "52 WM8758
                // transfers" was a floor that `NEXT.md` §5 was about to fit a model to.
                *self.i2c_tally.entry((dev, val, d[0])).or_insert(0) += 1;
                self.i2c_log.push((dev, val, d));
                // Outside the log cap deliberately. The log is a sample; the device is not, and a
                // chip that stops answering after 4096 transfers would be a bug that looks exactly
                // like firmware hanging on real hardware.
                if self.pmu.is_some() && dev >> 1 == Pcf50605::ADDR {
                    let len = (((val >> 1) & 3) as usize + 1).min(4);
                    let read = val & 0x20 != 0;
                    let pmu = self.pmu.as_mut().unwrap();
                    pmu.transfer(val, d);
                    if read {
                        // Latch the received bytes into the real data registers rather than
                        // keeping them in a shadow. These are ordinary read/write registers: the
                        // firmware *writes* the target register address into them to set up a
                        // read, and whatever it wrote stays there until a transfer overwrites it.
                        // Shadowing them made every data register the firmware had not just read
                        // return zero, which is why a modelled chip answering all-ones still
                        // behaved differently from a bus that answered all-ones.
                        let bytes: Vec<u8> = (0..len).map(|i| pmu.data_byte(i)).collect();
                        for (i, b) in bytes.iter().enumerate() {
                            if let Some((buf, j)) = self.locate_write(base + 0x0c + 4 * i as u32) {
                                buf[j] = *b;
                            }
                        }
                    }
                }
                // SEND is self-clearing: the controller raises it to start a transfer and drops it
                // when the transfer finishes. Leaving it latched means a driver that waits for it
                // to fall waits forever — and Apple's does, which is why its I²C routine sat at
                // 0x4000aa48 reading CTRL until it timed out. Recorded here rather than in the
                // generic write below, because that would put the bit straight back.
                if let Some((buf, i)) = self.locate_write(addr) {
                    buf[i] = val & !0x80;
                }
                return;
            }
        }
        // Logged before the bus filters it, so the log records what the firmware wrote rather than
        // what survived — same convention as `note_store_pc` at the top of this function.
        if self.write_log.is_some() {
            let idx = self.locate_idx(addr);
            self.note_write(addr, val as u32, idx);
        }
        // The external memory bus filters what it keeps: a ready bit the firmware cannot clear, and
        // a completion bit that follows the command bit written beside it.
        let val = match self.xmb.as_ref().map(|x| x.owns(addr)) {
            Some(true) => {
                let was = self.peek(addr);
                let mut x = self.xmb.take().unwrap();
                let v = x.store(addr, was, val);
                self.xmb = Some(x);
                v
            }
            _ => val,
        };
        match self.locate_write(addr) {
            Some((buf, i)) => buf[i] = val,
            None => {
                let _ = val;
                self.note_unmapped(addr, true);
            }
        }
    }
}

// ---------------------------------------------------------------- machine

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub framework: String,
    pub index: usize,
    /// `r0`–`r3` at the call — the first four arguments under the EABI.
    pub args: [u32; 4],
    /// The first four words at `sp`. Arguments beyond the fourth are passed on the stack, so
    /// without these any call taking more than four is silently truncated — which is exactly
    /// how `#167` was mistaken for a three-argument function when it takes seven.
    pub stack: [u32; 4],
    pub return_to: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Stop {
    /// Ran the instruction budget without otherwise stopping.
    BudgetExhausted,
    /// The game returned from its entry point.
    Returned,
    /// `PC` left every mapped region — almost always a missing stub rather than a CPU bug.
    Lost(u32),
    /// Reached the instruction count requested for a snapshot.
    SnapshotPoint,
    /// `--stop-when-idle` — no code executed for the first time within the window. The machine is
    /// still running; it has simply stopped doing anything it had not done already.
    Idle,
    /// The game called the semihosting exit call.
    Exited,
    /// `--stop-at` fired: the PC reached a requested address for the requested time.
    StopPoint(u32),
}

/// Extra space mapped above the image for the game's BSS.
///
/// A game's zero-initialised data is not in the file — the image ends and BSS begins immediately
/// after it. Without this, Pac-Man's first act is 57 741 writes into nothing while it clears its
/// own globals, and it gives up before reaching any real work.
pub const IMAGE_SPAN: usize = 0x0080_0000;

/// ARM semihosting: `svc #0x123456` in ARM state, operation in `r0`, parameters via `r1`.
///
/// The games ship with debug logging built on it. Implementing the output calls means each
/// title narrates its own startup — which is a far better guide to what the frameworks must do
/// than reading disassembly.
const SEMIHOSTING_VECTOR: u32 = 0x08;
const SYS_WRITEC: u32 = 0x03;
const SYS_WRITE0: u32 = 0x04;
const SYS_WRITE: u32 = 0x05;
const SYS_EXIT: u32 = 0x18;

/// The 5G panel: 320x240. Games render into this and present it via the swap import.
pub const FB_WIDTH: usize = 320;
pub const FB_HEIGHT: usize = 240;

pub const HEAP_BASE: u32 = 0x1900_0000;
pub const HEAP_SIZE: usize = 0x0400_0000; // 64 MB — see the note on miscTBD #0 in the README

/// What a trapped import does when called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stub {
    /// Record the call and return 0. The default, and honest: a wrong non-zero value would
    /// send the game down a path the trace could not explain.
    ReturnZero,
    /// Allocate `r0` bytes and return the pointer.
    Alloc,
    /// Release the block whose pointer is in argument register `arg`.
    ///
    /// Without this the games exhaust an 8 MB heap in about 200 frames — Pac-Man alone asks for
    /// nine blocks per frame. A bump allocator is enough to reach a first frame and nowhere near
    /// enough to keep running.
    Free { arg: usize },
    /// Return a fixed value.
    Value(u32),
    /// A monotonic microsecond clock, reporting through the out-pointer in `arg`.
    ///
    /// Identified as `miscTBD #9`: the games wrap it as `getTime(&out)` and compute
    /// `1_000_000 / (now - last)` — a frames-per-second counter. A stub returning any *constant*
    /// makes that delta zero and the title dies on a divide by zero, which is why a sweep over
    /// fixed return values could never find it. The value has to move.
    Clock { arg: usize, step: u32 },
    /// `glClearColor(r, g, b, a)` — IEEE-754 bit patterns arrive in `r0`–`r3`, since the
    /// EABI passes softfloat arguments in the core registers.
    GlClearColor,
    /// `glClear(mask)` — fills the framebuffer when `GL_COLOR_BUFFER_BIT` is set.
    GlClear,
    /// Present the framebuffer. Identified by bracketing every frame, first call and last.
    GlSwap,
    /// `glVertexAttribPointer(index, size, type, normalized, stride, pointer)` — six arguments,
    /// the last two on the stack. Records the array; drawing reads it back.
    GlVertexAttribPointer,
    /// Open a file: the path is a NUL-terminated string in register `path`, and the resulting
    /// handle is written to the address in register `out`.
    ///
    /// Games load their artwork and audio from the directory beside the executable — Pac-Man
    /// ships `tex_ig.tga`, `tex_menu1.tga`, `PM_Logo.raw.lcd5` and a tree of WAVs. Returning
    /// zero here is why sixteen of twenty titles never draw anything.
    FileOpen { path: usize, out: usize },
    /// `read(handle, buffer, length, &bytesRead)`.
    ///
    /// Identified from the sequence a game runs verbatim: open the path, allocate exactly N
    /// bytes, then call this with that handle, that buffer and that N.
    FileRead {
        handle: usize,
        buffer: usize,
        length: usize,
        out: usize,
    },
    /// Poll for input: writes the next queued event word to `arg + offset`, or zero when the
    /// queue is empty.
    ///
    /// The games treat input as **edge-triggered** — Pac-Man's handler tests bit 30 as an
    /// "event present" flag and consumes the low byte as a code. Writing a constant looks like a
    /// single stuck event and is ignored after the first poll, which is why pinning a value
    /// produced no response at all.
    InputPoll { arg: usize, offset: u32 },
    /// `glBindTexture(target, texture)` — two arguments.
    GlBindTexture,
    /// `glCompressedTexImage2D(target, level, format, width, height, border, imageSize, data)`
    /// — four registers plus four stack words.
    ///
    /// The games use **`GL_PALETTE8_RGBA8_OES` (0x8B96)**: a 1024-byte RGBA palette followed by
    /// `width * height` single-byte indices. Confirmed by arithmetic on three textures —
    /// `1024 + w*h` equals the declared `imageSize` exactly in every case.
    GlCompressedTexImage2D,
    /// `glDrawArrays(mode, first, count)`. Modes seen: 7 (`GL_QUADS`, Apple kept the desktop
    /// value) and 5 (`GL_TRIANGLE_STRIP`), always with `count = 4` — one quad per call.
    GlDrawArrays,
    /// Write `value` to the address held in argument register `arg`, then return `ret`.
    ///
    /// Many framework calls are queries that report through an out-parameter rather than a
    /// return value — `Settings #0(name, out)` and the `glGet`-shaped OpenGLES calls both do
    /// this. Returning zero from those leaves the caller reading whatever was already in the
    /// slot, which is how a divide-by-zero appears several hundred instructions later.
    WriteOut {
        arg: usize,
        /// Byte offset from the pointer in `arg`. Structs are filled in place, so the field the
        /// caller reads is often not at offset 0 — `InputEvents #0` fills a buffer whose caller
        /// reads `[buf + 4]`.
        offset: u32,
        value: u32,
        ret: u32,
    },
}

pub struct Machine {
    pub cpu: Cpu,
    pub mem: Memory,
    pub trace: Vec<Call>,
    /// Trap address -> (framework index, function index).
    traps: HashMap<u32, (usize, usize)>,
    /// Address span of `traps`, so the per-instruction lookup can be skipped by two compares.
    /// The traps are the game's import thunks, all in the image at `0x18000000+`; a cold boot runs
    /// entirely in IRAM and would otherwise pay a hash probe per instruction that can never hit.
    trap_lo: u32,
    trap_hi: u32,
    names: Vec<String>,
    /// Per-import behaviour, keyed by (framework, index). Missing means `ReturnZero`.
    stubs: HashMap<(String, usize), Stub>,
    heap_next: u32,
    /// First texture index assigned by `preload_textures`.
    ///
    /// **Unresolved.** Titles disagree: Pac-Man binds `tex#1` and renders best when numbering
    /// starts at 1; Ms. Pac-Man, Bejeweled and Zuma bind `tex#0`. Neither base is right for all,
    /// so the real assignment is not a directory-order sequence at all — see the README.
    pub tex_base: u32,
    /// How many times the input poll stub ran.
    pub polls: usize,
    /// Pending input events, consumed one per poll.
    pub input_queue: Vec<u32>,
    /// Record every indirect branch as (site, target).
    pub log_indirect: bool,
    /// An ordered **sample**; `indirect_edges` beside it is the census the report is built from.
    pub indirect_log: Capped<(u32, u32)>,
    /// `(site, target) -> count`, **uncapped**. The `--- indirect branches: N distinct edges ---`
    /// line was a tally of the capped log, so a busy run reported the first 4 096 branches' worth
    /// of distinct edges and called it the set.
    pub indirect_edges: BTreeMap<(u32, u32), u64>,
    /// `--callgraph` — every branch edge actually TAKEN, deduplicated with a count.
    ///
    /// RetailOS dispatches virtually, so a static scan of `BL` targets cannot answer "who calls
    /// this" — that dead end has stopped this investigation four separate times (research/03 §46,
    /// §52, research/08). At runtime the question is trivial: record `(site, target)` for both
    /// direct and indirect branches. Deduplicated, so the map is bounded by distinct edges rather
    /// than by executed instructions.
    pub edges: Option<BTreeMap<(u32, u32), u64>>,
    /// Capture the full register file whenever the PC reaches one of these addresses.
    ///
    /// "What is in `r4` at the bind site" is not answerable statically — the value is whatever the
    /// game put there — so the question has to be asked of a running machine.
    /// Function names recovered from the image by `extract_symbols`, keyed by entry address.
    pub symbols: BTreeMap<u32, String>,
    pub breakpoints: Vec<u32>,
    /// `(pc, regs)` for each breakpoint hit.
    pub break_log: Vec<(u32, [u32; 16])>,
    /// `--stop-at=ADDR[:N]` — halt the machine the Nth time the PC reaches ADDR.
    ///
    /// A breakpoint records and continues, which is the wrong shape for a fault that repeats: the
    /// boot loop restarts RetailOS hundreds of times, so the end of a run — and therefore the
    /// instruction history — belongs to whichever restart the budget happened to land in. Halting
    /// on the *first* arrival is what makes `--history` describe the original crash instead of a
    /// later echo of it.
    pub stop_at: Vec<(u32, u64)>,
    /// `--retwatch=V` — the instruction that *puts* V into r0, for tracing an error code back to
    /// the one place that produced it. Searching memory for the constant only finds it when it is
    /// an immediate; a code composed, loaded from a table, or propagated through a call chain is
    /// invisible to a search and obvious here.
    pub retwatch: Option<u32>,
    pub retwatch_log: Capped<(u32, u32)>,
    /// `pc -> (times, lr)`, **uncapped** — the producing instructions, which is what the report
    /// prints. Built from the capped log until 2026-08-14.
    pub retwatch_sites: BTreeMap<u32, (u64, u32)>,
    /// Register file immediately before an unmapped access. Capped at 64 — the count of unmapped
    /// accesses itself is `Memory::unmapped`, which is per-page and cannot saturate.
    pub unmapped_regs: Capped<(u32, [u32; 16])>,
    /// `--enterlog=PC[,PC…]` — arguments on every arrival at those addresses:
    /// `(pc, lr, r0, r1, r2, r3, icount)`.
    ///
    /// Deliberately keyed on *arrival*, not on `BL`: a plain `B` is a tail call (the edge-recording
    /// bug this project already paid for once), and virtual dispatch arrives by `BX` from a vtable
    /// slot. Anything that hooks the call instruction misses both.
    pub enter_pcs: Vec<u32>,
    pub enter_bloom: u64,
    pub enter_log: Capped<(u32, u32, [u32; 4], u64)>,
    /// `(pc, lr) -> count`, **uncapped**. `NEXT.md` has described this histogram as "the honest
    /// census" to read when the detail rows are truncated — which was only true below the log's own
    /// 65 536-entry cap, because it was tallied from the log. It is now tallied on arrival, so the
    /// claim in that row is true unconditionally.
    pub enter_callers: BTreeMap<(u32, u32), u64>,
    /// `--force-sem=ID[,ID…]` (and its alias `--force-vc-upload`) — RTXC semaphore ids whose
    /// `KS_pend` returns success instead of blocking. **An ablation, not a fix** — ledger bypass
    /// #17. It exists because `APPLEBOOT` waits on `0xe0` for a 64 KB VideoCore-firmware chunk that
    /// nothing in the machine can deliver: RetailOS programs the transfer engine at `0x60009000`
    /// and no completion ever comes back, because nothing is executing on the other side. Off by
    /// default; retire it when `0x60009000` moves bytes and raises its completion.
    ///
    /// On its own it is **not** sufficient, and that is the measured result rather than a caveat —
    /// see `force_vc_retire` and research/10 Addendum 8.
    ///
    /// Keyed on the pend itself rather than on a memory value because the wait is reached by a
    /// *tail* branch from the counting acquire at `0x000a0ebc` — there is no call frame to patch,
    /// and the transfer object's address is heap-dependent while the instruction is not.
    pub force_sems: Vec<u32>,
    /// The `KS_pend` service wrapper. `0x01` in this image's dispatch table, not freemyipod's
    /// `0x03`; the address is RetailOS-specific and belongs with the flag that uses it.
    pub force_sem_pend_pc: u32,
    /// `(lr, sem, icount)` per satisfied pend. The count is the flag's own positive control: the
    /// unablated run pends on `0xe0` exactly once, at `@51_764_626`.
    pub force_sem_log: Vec<(u32, u32, u64)>,
    /// `--force-vc-retire` — the second half of bypass #17. Satisfying `0xe0` is not enough: the
    /// buffer allocator at `0x00159b88` then busy-waits at `0x00159ba0` until all four in-use
    /// bytes at `channel+0x18` read zero, and only the engine's completion path clears them. This
    /// zeroes them on the retry edge (`0x00159bc8` with `r2 != 0` — the branch that would spin),
    /// which is the narrowest possible statement of "the engine retired what was outstanding".
    ///
    /// Deliberately keyed on the retry rather than the loop head: on the fast path — a genuinely
    /// free ring — it changes nothing, so a run that never spins is bit-identical to one without
    /// the flag.
    pub force_vc_retire: bool,
    /// `(channel, icount)` per modelled retire.
    pub force_retire_log: Capped<(u32, u64)>,
    /// `--sum-at=PC:ADDR:LEN` — byte-sum a memory range the moment execution reaches PC.
    ///
    /// A post-run `--dump` shows memory as it is when the budget runs out, which is a *later*
    /// state: the boot carries on after a failed load and overwrites what failed. Comparing an
    /// end-of-run dump against what the firmware checksummed mid-run is comparing two different
    /// moments, and that is how an image load looked corrupt when the question was never asked at
    /// the right time.
    pub sum_at: Vec<(u32, u32, u32)>,
    pub sum_at_log: Vec<(u32, u32, u32, [u8; 16])>,
    /// A single 32-bit word to watch. Every change is recorded together with the PC responsible.
    ///
    /// Watching one word rather than a range keeps this to a compare per instruction, which is
    /// cheap enough to leave on for a whole run.
    pub watch: Option<u32>,
    /// `(pc, old, new)` for each observed change to the watched word.
    pub watch_log: Capped<(u32, u32, u32)>,
    /// Directory holding the game's resources, if one was supplied.
    pub game_dir: Option<std::path::PathBuf>,
    /// Contents and read position of each opened file, indexed by handle - 1.
    open_files: Vec<(Vec<u8>, usize)>,
    /// Diagnostic log of file activity.
    pub file_log: Capped<String>,
    /// Diagnostic log of uploads and draws.
    pub tex_log: Capped<String>,
    /// Decoded textures, keyed by the name passed to `glBindTexture`.
    textures: std::collections::HashMap<u32, Texture>,
    bound_texture: u32,
    /// Vertex arrays registered by `glVertexAttribPointer`, indexed by attribute number.
    arrays: [Option<VertexArray>; 8],
    /// Quads actually rasterised.
    pub quads_drawn: usize,
    /// 320x240 RGB, the panel the games believe they are drawing to.
    pub framebuffer: Vec<u8>,
    clear_color: [f32; 4],
    /// How many times the game asked for the framebuffer to be presented.
    pub frames_presented: usize,
    /// How many `glClear` calls actually reached the framebuffer.
    pub clears: usize,
    /// Microsecond counter backing `Stub::Clock`.
    clock: u32,
    /// Released blocks available for reuse, as (block, total_size).
    free_list: Vec<(u32, u32)>,
    /// Everything the game wrote via semihosting.
    pub output: String,
    /// Total instructions executed.
    pub executed: usize,
    /// `(return address, string pointer)` for each semihosted string written.
    pub print_sites: Capped<(u32, u32)>,
    /// Stop cleanly once this many instructions have run, so state can be captured.
    pub snap_at: Option<usize>,
    /// `(call site, target)` for every `BL` executed, capped. Enabled by `--calls`.
    pub call_log: Vec<(u32, u32)>,
    pub call_at: usize,
    pub call_log_on: bool,
    /// Interpreter instructions per simulated microsecond.
    ///
    /// The PP5021C runs at roughly 75 MHz, so 75 models real time. **Lowering it makes simulated
    /// time run faster than the code executing it**, which is the cheap answer to firmware that
    /// polls with timeouts: the bootloader spends billions of instructions in delay loops, and
    /// those collapse when the clock advances quicker. Timing-sensitive code can notice, so it is a
    /// knob rather than a new default.
    pub instr_per_usec: usize,
    /// Next fire time in microseconds for TIMER1 and TIMER2; 0 means "not yet armed".
    pub timer_next: [u32; 2],
    /// Times an enabled interrupt was asserted, and times the CPU actually took it. A gap between
    /// them means the firmware is running with IRQs masked -- a different problem from the
    /// controller never asserting at all.
    pub irqs_asserted: u64,
    pub irqs_taken: u64,
    /// Sampled PC histogram, bucketed by 16 bytes. `None` disables sampling entirely.
    pub profile: Option<HashMap<u32, u64>>,
    /// Restrict sampling to `[from, to)` instructions. A whole-run profile of this boot is 57%
    /// one restart loop, which drowns every phase that ran before it — the 83 M window between the
    /// last disk command and the first self-reset contributes under a fifth of the samples and no
    /// row of the top-15 belongs to it. Windowing is what makes that stretch legible at all.
    pub profile_window: Option<(u64, u64)>,
    /// `--novelty` — the instruction count at which each 16-byte code bucket FIRST executed.
    ///
    /// A profile says where the time went; it cannot say what the machine did *last*. When a boot
    /// settles into a steady state, the question that matters is which code ran once, late, just
    /// before everything stopped changing — and that is a first-seen timestamp, not a histogram.
    /// The bitset is the fast path: one bit test per instruction, so the map is only touched on
    /// genuinely new code.
    pub novelty: Option<HashMap<u32, u64>>,
    /// `--stop-when-idle=N` — end the run once N instructions have passed with no code bucket
    /// executing for the first time. A booted RetailOS spends most of a run in its idle loop (the
    /// last new code on the retail path runs at @108 M of a 600 M budget), so without this four
    /// fifths of every measurement is confirmed idling. Requires `--novelty`, which is what knows
    /// whether a bucket is new.
    pub stop_when_idle: Option<u64>,
    /// Instruction count at which a bucket last executed for the first time.
    pub last_novel: u64,
    /// `mem.sleeps` as it stood when `last_novel` was set, so the trailing window's sleep count is
    /// `mem.sleeps - last_novel_sleeps`.
    ///
    /// Without this, `Stop::Idle` is indistinguishable from a block. `-> Idle after N instructions`
    /// was read as "the machine went quiet" for a whole day, and a 24-entry module registry that
    /// stalls at entry 11 was published on the strength of it; what the machine was actually doing
    /// was a bounded 65 536-iteration scan over code it had already run once, which by construction
    /// records no novelty and reads as idle. A machine that is genuinely waiting asks the core to
    /// sleep, so a window with **zero** sleeps in it is a busy machine and never a blocked one —
    /// that one number is the whole discriminator, and it costs a subtraction.
    pub last_novel_sleeps: u64,
    seen_bits: Vec<u64>,
    /// Ring buffer of recently executed addresses. When the game branches somewhere
    /// impossible, "how did it get here" is the only question worth asking, and the answer
    /// is the previous dozen instructions.
    history: [u32; HISTORY],
    history_at: usize,
    /// Where the entry point should return to, so we can tell "finished" from "lost".
    exit_addr: u32,
}

/// Deep enough to see past a fault handler to the code that actually went wrong. 32 entries
/// were entirely consumed by the semihosting print routine, hiding the real caller.
const CALL_LOG: usize = 4096;
const HISTORY: usize = 512;

impl Machine {
    /// Map the image, allocate RAM, and rewrite every import thunk's literal to a trap address.
    pub fn new(app: &EApp, ram_base: u32, ram_size: usize) -> Self {
        // The image region is extended with zeros so the game's BSS lands somewhere real.
        let mut image = app.image.clone();
        image.resize(image.len().max(IMAGE_SPAN), 0);

        let mut mem = Memory {
            icount: 0,
            store_pcs: Vec::new(),
            store_addrs: Vec::new(),
            store_addr_lo: u32::MAX,
            store_addr_hi: 0,
            read_addrs: Vec::new(),
            read_addr_lo: u32::MAX,
            read_addr_hi: 0,
            read_log: Capped::new(2_000_000),
            trace_pc: None,
            pc_trace: Vec::new(),
            trace_calls_from: None,
            call_trace: Vec::new(),
            pc_hist: None,
            cop_awake: false,
            regs_at: None,
            regs_seen: Vec::new(),
            read_sites: BTreeMap::new(),
            store_pc_log: Capped::new(2_000_000),
            store_split: 0,
            unmapped_seq: 0,
            regions: vec![
                Region {
                    name: "image",
                    base: app.load_base,
                    data: image,
                },
                Region {
                    name: "stack",
                    base: ram_base,
                    data: vec![0; ram_size],
                },
                Region {
                    name: "heap",
                    base: HEAP_BASE,
                    data: vec![0; HEAP_SIZE],
                },
            ],
            unmapped: BTreeMap::new(),
            aliases: Vec::new(),
            read_toggle: Vec::new(),
            toggle_state: Vec::new(),
            backlight: Backlight::default(),
            read_overrides: Vec::new(),
            read_or_masks: Vec::new(),
            usec_timer: None,
            usec: 0,
            ide_cfg_ack_off: false,
            ide_irq_latch_off: false,
            ide_irq_due: None,
            cpu_sleep: false,
            slept_usec: 0,
            sleeps: 0,
            int_pending: 0,
            int_pending_hi: 0,
            ata: None,
            bcm: None,
            i2c_base: None,
            i2c_log: Capped::new(4096),
            i2c_tally: BTreeMap::new(),
            i2c_fill: None,
            pmu: None,
            xmb: None,
            clickwheel: None,
            nor: None,
            page_log: None,
            page_gran: 0x100,
            verify_memory: false,
            verify_mismatches: Capped::new(64),
            input_probe: None,
            input_regs: BTreeMap::new(),
            watch_range: None,
            watch_range_log: Capped::new(4096),
            watch_range_words: BTreeMap::new(),
            write_log: None,
            write_log_entries: Capped::new(8192),
            write_log_regions: BTreeMap::new(),
            ide_irq_raised: 0,
            ide_irq_acked: 0,
            ide_irq_delivered: 0,
            dma_dropped: 0,
            dma_drop_sites: Vec::new(),
            pp_dma_transfers: 0,
            pp_dma_bytes: 0,
            pp_dma_log: Capped::new(64),
            pp_dma_irq: None,
            page_counts: BTreeMap::new(),
            accounting: false,
            fast: vec![FastPage::EMPTY; FAST_SLOTS].into_boxed_slice(),
            readonly: Vec::new(),
            internal: false,
            region_reads: Vec::new(),
            region_writes: Vec::new(),
            int_ack_on_read: Vec::new(),
            mmap_base: None,
            mmap_regs: [0; 16],
            mmap_alias_floor: 0,
            pc: 0,
        };

        let mut traps = HashMap::new();
        let mut names = Vec::new();
        for (fi, fw) in app.frameworks.iter().enumerate() {
            names.push(fw.name.clone());
            for (gi, &thunk) in fw.thunks.iter().enumerate() {
                let trap = TRAP_BASE + fi as u32 * TRAP_STRIDE + gi as u32 * 4;
                // `ldr pc, [pc, #imm]` reads from thunk+8+imm. Patch that slot, exactly as
                // RetailOS's own loader would.
                let instr = mem.read32(thunk);
                let literal = thunk.wrapping_add(8).wrapping_add(instr & 0xFFF);
                mem.write32(literal, trap);
                traps.insert(trap, (fi, gi));
            }
        }

        let exit_addr = ram_base.wrapping_add(ram_size as u32).wrapping_sub(4);

        let mut cpu = Cpu::new();
        cpu.set_mode(Mode::System); // games run unprivileged-but-unseparated, per freemyipod
        cpu.regs[13] = ram_base + ram_size as u32 - 0x100; // stack near the top of RAM
        cpu.regs[14] = exit_addr;
        cpu.regs[15] = app.entry;

        Machine {
            cpu,
            mem,
            trace: Vec::new(),
            trap_lo: traps.keys().copied().min().unwrap_or(u32::MAX),
            trap_hi: traps.keys().copied().max().unwrap_or(0),
            traps,
            names,
            stubs: HashMap::new(),
            heap_next: HEAP_BASE,
            // Magenta, not black: if a rendered frame comes back black we need that to be
            // evidence the clear actually ran, not the initial state coinciding with it.
            tex_base: 1,
            polls: 0,
            input_queue: Vec::new(),
            log_indirect: false,
            indirect_log: Capped::new(4096),
            indirect_edges: BTreeMap::new(),
            edges: None,
            symbols: BTreeMap::new(),
            breakpoints: Vec::new(),
            stop_at: Vec::new(),
            retwatch: None,
            retwatch_log: Capped::new(4096),
            retwatch_sites: BTreeMap::new(),
            unmapped_regs: Capped::new(64),
            enter_pcs: Vec::new(),
            enter_bloom: 0,
            enter_log: Capped::new(65536),
            enter_callers: BTreeMap::new(),
            force_sems: Vec::new(),
            force_sem_pend_pc: 0x000a_6924,
            force_sem_log: Vec::new(),
            force_vc_retire: false,
            force_retire_log: Capped::new(4096),
            sum_at: Vec::new(),
            sum_at_log: Vec::new(),
            break_log: Vec::new(),
            watch: None,
            watch_log: Capped::new(4096),
            game_dir: None,
            open_files: Vec::new(),
            file_log: Capped::new(4096),
            tex_log: Capped::new(64),
            textures: std::collections::HashMap::new(),
            bound_texture: 0,
            arrays: Default::default(),
            quads_drawn: 0,
            framebuffer: [255u8, 0, 255]
                .iter()
                .cycle()
                .take(FB_WIDTH * FB_HEIGHT * 3)
                .copied()
                .collect(),
            clear_color: [0.0; 4],
            frames_presented: 0,
            clears: 0,
            clock: 0,
            free_list: Vec::new(),
            executed: 0,
            print_sites: Capped::new(64),
            snap_at: None,
            call_log: Vec::new(),
            call_at: 0,
            call_log_on: false,
            instr_per_usec: 75,
            timer_next: [0; 2],
            irqs_asserted: 0,
            irqs_taken: 0,
            profile: None,
            profile_window: None,
            novelty: None,
            stop_when_idle: None,
            last_novel: 0,
            last_novel_sleeps: 0,
            seen_bits: Vec::new(),
            output: String::new(),
            history: [0; HISTORY],
            history_at: 0,
            exit_addr,
        }
    }

    /// Read a NUL-terminated string out of guest memory.
    fn read_cstr(&mut self, addr: u32, max: usize) -> String {
        let mut out = String::new();
        for i in 0..max {
            let c = self.mem.read8(addr.wrapping_add(i as u32));
            if c == 0 {
                break;
            }
            out.push(c as char);
        }
        out
    }

    /// Pre-load every `.tga` in the resource directory as a texture, numbered from 1.
    ///
    /// Hypothesis under test: RetailOS loads a title's artwork from its manifest at launch, and
    /// the games simply `glBindTexture` it by index. Pac-Man binds `tex#1` while never creating
    /// or uploading a texture, and every title ships `.tga` files it never opens — both of which
    /// this would explain.
    ///
    /// The files are 16-bit A1R5G5B5 with magenta (`0xF83E`) as the colour key.
    pub fn preload_textures(&mut self) -> Vec<String> {
        let Some(root) = self.game_dir.clone() else { return Vec::new() };

        // Texture order comes from `Manifest.plist`, not the filesystem.
        //
        // The manifest is the only per-title ordering RetailOS is known to consume, and it is
        // demonstrably NOT alphabetical — Ms. Pac-Man lists `tex_tutorial_*` after
        // `tex_ui_display`, which a directory sort can never produce. Sorting by filename put
        // every index after that point on the wrong texture.
        let files: Vec<std::path::PathBuf> = match manifest_paths(&root.join("Manifest.plist")) {
            Some(paths) => paths
                .into_iter()
                .filter(|p| !p.contains("Executables"))
                .map(|p| root.join(p.replace('\\', "/")))
                .collect(),
            None => {
                // No manifest: fall back to a sorted scan, which is at least deterministic.
                let mut v = Vec::new();
                let mut stack = vec![root.clone()];
                while let Some(d) = stack.pop() {
                    let Ok(entries) = std::fs::read_dir(&d) else { continue };
                    for e in entries.flatten() {
                        let p = e.path();
                        if p.is_dir() {
                            stack.push(p);
                        } else {
                            v.push(p);
                        }
                    }
                }
                v.sort();
                v
            }
        };

        // Numbering starts at 0 — the games bind `tex#0` — and advances only for textures that
        // actually decode. Using the file's position in the directory listing left gaps wherever
        // a file was skipped, so every index after the first failure was wrong.
        let mut next = self.tex_base;
        let mut loaded = Vec::new();
        for path in files.iter() {
            let Ok(d) = std::fs::read(path) else { continue };
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let decoded = match ext.as_str() {
                "tga" => decode_tga(&d),
                // Documented in research/01: 16-byte header of width, height, type id and RGB
                // format, then RGB565 pixels. Cubis 2 ships these.
                "ipd" => decode_ipd(&d),
                // Ms. Pac-Man ships headerless RGB565 — dimensions come from the file size.
                "bin" => decode_raw_rgb565(&d),
                _ => None,
            };
            let Some((w, h, rgba)) = decoded else { continue };
            let name = next;
            next += 1;
            loaded.push(format!(
                "tex#{name} <- {} ({w}x{h}, .{ext})",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
            self.textures.insert(name, Texture { w, h, rgba });
        }
        loaded
    }

    /// Resolve a game-relative path against the resource directory and open it.
    ///
    /// Lookup is case-insensitive and tries the basename as a fallback, because the games ask
    /// for paths as they appeared on a FAT32 volume and the layout on disk here is not
    /// guaranteed to match byte for byte.
    fn open_file(&mut self, name: &str) -> u32 {
        let Some(root) = self.game_dir.clone() else { return 0 };
        let rel = name.replace('\\', "/");
        let target = rel.trim_start_matches('/');
        let base = target.rsplit('/').next().unwrap_or(target).to_ascii_lowercase();

        let mut found = None;
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.to_ascii_lowercase() == base)
                {
                    found = Some(p);
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }

        let Some(path) = found else { return 0 };
        let Ok(data) = std::fs::read(&path) else { return 0 };
        self.open_files.push((data, 0));
        self.open_files.len() as u32 // handles are 1-based; 0 means failure
    }

    /// Copy up to `len` bytes from the open file into guest memory, advancing its position.
    fn read_file(&mut self, handle: usize, buf: u32, len: u32) -> u32 {
        if handle == 0 || handle > self.open_files.len() {
            return 0;
        }
        let (data, pos) = &self.open_files[handle - 1];
        let start = (*pos).min(data.len());
        let n = (len as usize).min(data.len() - start);
        let bytes: Vec<u8> = data[start..start + n].to_vec();
        self.open_files[handle - 1].1 = start + n;
        for (i, b) in bytes.iter().enumerate() {
            self.mem.write8(buf.wrapping_add(i as u32), *b);
        }
        n as u32
    }

    /// Decode a `GL_PALETTE8_RGBA8_OES` image into RGBA for the currently bound texture.
    fn upload_paletted(&mut self, w: usize, h: usize, data: u32) {
        if w == 0 || h == 0 || w > 2048 || h > 2048 {
            return;
        }
        let palette: Vec<[u8; 4]> = (0..256)
            .map(|i| {
                let a = data + (i * 4) as u32;
                [
                    self.mem.read8(a),
                    self.mem.read8(a + 1),
                    self.mem.read8(a + 2),
                    self.mem.read8(a + 3),
                ]
            })
            .collect();
        let base = data + 1024;
        let mut rgba = Vec::with_capacity(w * h * 4);
        for i in 0..w * h {
            rgba.extend_from_slice(&palette[self.mem.read8(base + i as u32) as usize]);
        }
        let opaque = rgba.chunks_exact(4).filter(|p| p[3] >= 8).count();
        self.tex_log.push(format!(
            "upload tex#{} {}x{} opaque={}/{}",
            self.bound_texture, w, h, opaque, w * h
        ));
        self.textures.insert(self.bound_texture, Texture { w, h, rgba });
    }

    /// Read one attribute component as 16.16 fixed point, in pixels.
    fn attr(&mut self, index: usize, vertex: u32, comp: usize) -> f32 {
        let Some(a) = self.arrays[index].as_ref() else { return 0.0 };
        if comp >= a.size {
            return 0.0;
        }
        // A stride of 0 means tightly packed, per GL. Taking it literally made every vertex
        // read the same address — Pac-Man registers its arrays this way and drew degenerate
        // quads because of it.
        let stride = if a.stride == 0 { a.size * 4 } else { a.stride };
        let addr = a.ptr + vertex * stride as u32 + (comp * 4) as u32;
        self.mem.read32(addr) as i32 as f32 / 65536.0
    }

    /// Whether a vertex attribute array is registered.
    fn has_attr(&self, index: usize) -> bool {
        self.arrays[index].is_some()
    }

    /// Rasterise a draw as triangles, with per-vertex texture-coordinate interpolation.
    ///
    /// The projection is `glOrthof(0, 320, 0, 240, -1, 1)`, so positions are already in pixels —
    /// no transform is needed. Y is flipped because GL's origin is bottom-left and the
    /// framebuffer's is top-left.
    ///
    /// Barycentric interpolation replaces an earlier bounding-box approximation. That shortcut is
    /// only correct for a single screen-aligned sprite filling its own quad; the games pack 8x8
    /// tiles out of a 512x512 atlas and flip them freely, which the approximation smeared.
    fn draw_arrays(&mut self, mode: u32, first: u32, count: u32) {
        if count < 3 || count > 64 {
            return;
        }
        let v: Vec<Vertex> = (0..count)
            .map(|i| {
                let n = first + i;
                Vertex {
                    x: self.attr(0, n, 0),
                    y: self.attr(0, n, 1),
                    u: self.attr(1, n, 0),
                    w: self.attr(1, n, 1),
                }
            })
            .collect();

        let colour = if self.has_attr(2) {
            [
                (self.attr(2, first, 0).clamp(0.0, 1.0) * 255.0) as u8,
                (self.attr(2, first, 1).clamp(0.0, 1.0) * 255.0) as u8,
                (self.attr(2, first, 2).clamp(0.0, 1.0) * 255.0) as u8,
            ]
        } else {
            // GL's default vertex colour is opaque white; defaulting to black hides geometry.
            [255, 255, 255]
        };
        let textured = self.arrays[1].is_some() && self.textures.contains_key(&self.bound_texture);

        // `push_with`, not a `len()` guard: the row costs eight folds over the vertex list plus a
        // `format!`, so it must stay lazy — but the draw still has to be counted, or the report's
        // total becomes the cap the moment a title draws more than a handful of times.
        let (bound, known) = (self.bound_texture, textured);
        let log = &mut self.tex_log;
        log.push_with(|| {
            let rng = |f: fn(&Vertex) -> f32| {
                v.iter().map(f).fold(f32::MAX, f32::min)..v.iter().map(f).fold(f32::MIN, f32::max)
            };
            let (px, py, pu, pv) = (rng(|p| p.x), rng(|p| p.y), rng(|p| p.u), rng(|p| p.w));
            format!(
                "n={count} mode={mode} pos=[{:.1}..{:.1} , {:.1}..{:.1}] uv=[{:.1}..{:.1} , {:.1}..{:.1}] tex#{bound} known={known}",
                px.start, px.end, py.start, py.end, pu.start, pu.end, pv.start, pv.end
            )
        });

        // GL_QUADS fans from vertex 0; GL_TRIANGLE_STRIP walks in pairs.
        const GL_TRIANGLE_STRIP: u32 = 5;
        let tris: Vec<[usize; 3]> = if mode == GL_TRIANGLE_STRIP {
            (0..count as usize - 2).map(|i| [i, i + 1, i + 2]).collect()
        } else {
            (1..count as usize - 1).map(|i| [0, i, i + 1]).collect()
        };
        for t in tris {
            self.fill_triangle(&v[t[0]], &v[t[1]], &v[t[2]], colour, textured);
        }
        self.quads_drawn += 1;
    }

    fn fill_triangle(&mut self, a: &Vertex, b: &Vertex, c: &Vertex, colour: [u8; 3], textured: bool) {
        let area = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
        if area.abs() < 1e-6 {
            return; // degenerate
        }
        let min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as usize;
        let max_x = (a.x.max(b.x).max(c.x).ceil()).min(FB_WIDTH as f32 - 1.0);
        let min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as usize;
        let max_y = (a.y.max(b.y).max(c.y).ceil()).min(FB_HEIGHT as f32 - 1.0);
        if max_x < 0.0 || max_y < 0.0 {
            return;
        }

        for py in min_y..=(max_y as usize) {
            for px in min_x..=(max_x as usize) {
                let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                // Barycentric weights, sign-normalised so winding order does not matter.
                let w0 = ((b.x - fx) * (c.y - fy) - (c.x - fx) * (b.y - fy)) / area;
                let w1 = ((c.x - fx) * (a.y - fy) - (a.x - fx) * (c.y - fy)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                if px >= FB_WIDTH || py >= FB_HEIGHT {
                    continue;
                }

                let mut rgb = colour;
                if textured {
                    let fu = w0 * a.u + w1 * b.u + w2 * c.u;
                    let fv = w0 * a.w + w1 * b.w + w2 * c.w;
                    let t = &self.textures[&self.bound_texture];
                    let tx = (fu.max(0.0) as usize).min(t.w - 1);
                    let ty = (fv.max(0.0) as usize).min(t.h - 1);
                    let ti = (ty * t.w + tx) * 4;
                    if t.rgba[ti + 3] < 8 {
                        continue; // colour-keyed transparent texel
                    }
                    rgb = [t.rgba[ti], t.rgba[ti + 1], t.rgba[ti + 2]];
                }
                // Flip Y: GL origin is bottom-left, framebuffer origin is top-left.
                let row = FB_HEIGHT - 1 - py;
                let o = (row * FB_WIDTH + px) * 3;
                self.framebuffer[o..o + 3].copy_from_slice(&rgb);
            }
        }
    }

    /// The framebuffer as a binary PPM (P6) — the simplest format that any viewer opens.
    pub fn framebuffer_ppm(&self) -> Vec<u8> {
        let mut out = format!("P6\n{FB_WIDTH} {FB_HEIGHT}\n255\n").into_bytes();
        out.extend_from_slice(&self.framebuffer);
        out
    }

    /// First-fit allocator over the heap region.
    ///
    /// Each block carries an 8-byte header holding its size, so `free` knows how much to
    /// return. First-fit reuse matters more than efficiency here: the games allocate and
    /// release the same shapes every frame, so the free list settles quickly.
    fn alloc(&mut self, want: u32) -> u32 {
        let size = (want + 7) & !7;
        let total = size + 8;

        if let Some(i) = self.free_list.iter().position(|(_, sz)| *sz >= total) {
            let (block, block_size) = self.free_list.remove(i);
            self.mem.write32(block, block_size);
            return block + 8;
        }

        if (self.heap_next - HEAP_BASE) as usize + total as usize > HEAP_SIZE {
            return 0; // refuse rather than hand back a pointer into nothing
        }
        let block = self.heap_next;
        self.heap_next += total;
        self.mem.write32(block, total);
        block + 8
    }

    fn free(&mut self, ptr: u32) {
        if ptr < HEAP_BASE + 8 || ptr >= HEAP_BASE + HEAP_SIZE as u32 {
            return; // not ours; silently ignoring is safer than corrupting the list
        }
        let block = ptr - 8;
        let size = self.mem.read32(block);
        if size >= 8 && !self.free_list.iter().any(|(b, _)| *b == block) {
            self.free_list.push((block, size));
        }
    }

    /// Queue an input event. Bit 30 marks "event present"; the low byte carries the code.
    pub fn queue_input(&mut self, code: u8) {
        self.input_queue.push(0x4000_0000 | code as u32);
    }

    /// Give an import a behaviour other than "return 0".
    ///
    /// Identification is evidential, not assumed: `miscTBD #0` is called with `0x10`, `0x30`,
    /// `0xbf0`, `0x10` and its result is immediately dereferenced, which is an allocator and
    /// very little else. Each stub added here should be justified by what the trace showed.
    pub fn set_stub(&mut self, framework: &str, index: usize, stub: Stub) {
        self.stubs.insert((framework.to_string(), index), stub);
    }

    /// Service one semihosting call. Returns true if the game asked to exit.
    ///
    /// On `SWI` the CPU has already banked `CPSR` into `SPSR_svc` and put the return address in
    /// `LR`, so returning is a `restore_cpsr` plus a jump — exactly what a real handler does.
    fn semihost(&mut self) -> bool {
        let op = self.cpu.regs[0];
        let param = self.cpu.regs[1];
        let mut result: u32 = 0;

        match op {
            SYS_WRITEC => {
                let c = self.mem.read8(param);
                self.output.push(c as char);
            }
            SYS_WRITE0 => {
                // Who printed this? The ROM's console is now our best instrument, and knowing the
                // call site turns a message into an address we can disassemble.
                self.print_sites.push((self.cpu.regs[14], param));
                let mut a = param;
                // Bound the walk: a runaway pointer must not hang the run.
                for _ in 0..4096 {
                    let c = self.mem.read8(a);
                    if c == 0 {
                        break;
                    }
                    self.output.push(c as char);
                    a = a.wrapping_add(1);
                }
            }
            SYS_WRITE => {
                // param -> [handle, address, length]
                let addr = self.mem.read32(param.wrapping_add(4));
                let len = self.mem.read32(param.wrapping_add(8)).min(4096);
                for i in 0..len {
                    let c = self.mem.read8(addr.wrapping_add(i));
                    self.output.push(c as char);
                }
            }
            SYS_EXIT => return true,
            // Everything else reports failure rather than pretending to succeed.
            _ => result = u32::MAX,
        }

        let ret = self.cpu.regs[14];
        self.cpu.restore_cpsr();
        self.cpu.regs[0] = result;
        self.cpu.regs[15] = ret;
        false
    }

    /// The most recently executed addresses, oldest first.
    pub fn recent(&self) -> Vec<u32> {
        let n = self.history_at.min(HISTORY);
        (0..n)
            .map(|i| self.history[(self.history_at - n + i) % HISTORY])
            .collect()
    }

    /// Bytes handed out by the bump allocator so far.
    pub fn heap_used(&self) -> u32 {
        self.heap_next - HEAP_BASE
    }

    /// Point the CPU at `addr` and run it as a fresh call, preserving memory and heap state.
    ///
    /// The vector table holds several entry points and RetailOS calls them at different
    /// lifecycle moments; this lets each be driven in turn without discarding what the
    /// previous one set up.
    pub fn call(&mut self, addr: u32, budget: usize) -> Stop {
        self.call_with(addr, &[], budget)
    }

    /// Call `addr` with explicit arguments in `r0`–`r3`.
    ///
    /// RetailOS passes a context to these vectors — Pac-Man's frame vector does
    /// `mov r4, r0; mov r5, r1; ldrb r0, [r5]`, dereferencing its second argument. Calling with
    /// zeros makes that a null read, which is indistinguishable from the game deciding it has
    /// nothing to do.
    pub fn call_with(&mut self, addr: u32, args: &[u32], budget: usize) -> Stop {
        for (i, a) in args.iter().take(4).enumerate() {
            self.cpu.regs[i] = *a;
        }
        self.cpu.regs[14] = self.exit_addr;
        self.cpu.regs[15] = addr;
        self.run(budget)
    }

    /// Map a RetailOS `OSOS` image so Apple's own code can be executed.
    ///
    /// OSOS is position-dependent and loads at `0x10000000`; the firmware's image directory
    /// records that address explicitly.
    pub fn map_osos(&mut self, data: Vec<u8>) -> Result<(), String> {
        const OSOS_BASE: u32 = 0x1000_0000;
        let end = OSOS_BASE + data.len() as u32;
        // Regions are resolved first-match, so an overlap silently shadows one of them. That is
        // exactly what happened the first time OSOS was mapped: the scratch stack also sat at
        // 0x10000000, so Apple's loader executed 1.8M instructions of zeroed stack instead.
        for r in &self.mem.regions {
            let (rs, re) = (r.base, r.base + r.data.len() as u32);
            if rs < end && OSOS_BASE < re {
                return Err(format!(
                    "OSOS [{OSOS_BASE:#x}..{end:#x}) overlaps region {:?} [{rs:#x}..{re:#x})",
                    r.name
                ));
            }
        }
        self.mem.regions.push(Region {
            name: "osos",
            base: OSOS_BASE,
            data,
        });
        Ok(())
    }

    /// Locate a framework's export table in the mapped OSOS by searching for its interface hash.
    ///
    /// This is the static equivalent of what RetailOS's loader does at runtime: it `memcmp`s the
    /// game's 16-byte hash against a system table and takes the pointer it finds. Because the hash
    /// travels *inside the game binary*, the same table can be found without executing anything —
    /// which is what makes booting the firmware unnecessary to reach the implementations.
    ///
    /// Entry layout, with the thunk array growing **backwards** from the hash:
    ///
    /// ```text
    /// [ count x 4-byte function pointers ]   <- returned
    /// +0x00  16-byte interface hash              <- found by search
    /// +0x10  count
    /// +0x14  magic 0x13061973
    /// ```
    ///
    /// The count at `+0x10` is checked against the game's own declaration; a mismatch means the
    /// match was coincidental and is rejected rather than trusted.
    /// The returned addresses are **low-mirror** addresses (e.g. `0x26c534`), matching where the
    /// firmware's own boot executes — it branched to `0x23c`, not `0x1000023c`. So `osos-low`
    /// must be mapped for these to resolve; rebasing them onto `0x10000000` would point at the
    /// wrong copy.
    pub fn find_exports(&self, hash: &[u8; 16], declared: usize) -> Option<Vec<u32>> {
        let osos = self.mem.regions.iter().find(|r| r.name == "osos")?;
        let d = &osos.data;

        let at = d.windows(16).position(|w| w == hash)?;
        let count = u32::from_le_bytes(d.get(at + 0x10..at + 0x14)?.try_into().ok()?) as usize;
        if count != declared {
            return None;
        }
        let magic = u32::from_le_bytes(d.get(at + 0x14..at + 0x18)?.try_into().ok()?);
        if magic != 0x1306_1973 {
            return None;
        }

        let start = at.checked_sub(count * 4)?;
        Some(
            (0..count)
                .map(|i| {
                    let o = start + i * 4;
                    u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
                })
                .collect(),
        )
    }

    /// Bind imports straight to RetailOS's own implementations instead of to traps.
    ///
    /// Same mechanism as trap binding — the thunk's literal slot is patched — so nothing in the
    /// CPU is special-cased. The only difference is the address written. Returns per-framework
    /// `(name, bound, declared)` so a partial bind is visible rather than silent.
    pub fn bind_native(&mut self, app: &EApp, only: Option<&str>) -> Vec<(String, usize, usize)> {
        let mut report = Vec::new();
        for (fi, fw) in app.frameworks.iter().enumerate() {
            let mut bound = 0;
            if only.is_none_or(|o| o == fw.name) {
                if let Some(exports) = self.find_exports(&fw.hash, fw.thunks.len()) {
                    for (i, &thunk) in fw.thunks.iter().enumerate() {
                        let Some(&target) = exports.get(i) else { continue };
                        let instr = self.mem.read32(thunk);
                        let literal = thunk.wrapping_add(8).wrapping_add(instr & 0xFFF);
                        self.mem.write32(literal, target);
                        // The trap is now unreachable for this import; drop it so a later call
                        // that *does* land in trap space is unambiguous.
                        self.traps.retain(|_, &mut (f, g)| !(f == fi && g == i));
                        bound += 1;
                    }
                }
            }
            report.push((fw.name.clone(), bound, fw.thunks.len()));
        }
        report
    }

    /// Read back every import thunk's literal slot.
    ///
    /// This is the payoff of running Apple's loader: whatever it wrote here is the real address
    /// of each framework function, resolved by RetailOS's own binding logic rather than inferred
    /// by us.
    pub fn thunk_targets(&mut self, app: &EApp) -> Vec<(String, usize, u32)> {
        let mut out = Vec::new();
        for fw in &app.frameworks {
            for (i, &thunk) in fw.thunks.iter().enumerate() {
                let instr = self.mem.read32(thunk);
                let literal = thunk.wrapping_add(8).wrapping_add(instr & 0xFFF);
                out.push((fw.name.clone(), i, self.mem.read32(literal)));
            }
        }
        out
    }

    /// Allocate a zeroed scratch block for use as a synthetic context argument.
    pub fn scratch(&mut self, size: u32) -> u32 {
        self.alloc(size)
    }

    /// Run until the budget is exhausted, the entry point returns, or `PC` goes somewhere
    /// we cannot explain.
    /// Drive the PP502x timers and interrupt controller, and deliver an IRQ if one is due.
    ///
    /// Register map from Rockbox `pp5020.h`; timer programming from its `timer-pp.c`, which writes
    /// `TIMERn_CFG = 0xc0000000 | (cycles - 1)` — bit 31 enable, bit 30 repeat, low bits a period
    /// in microseconds (`TIMER_FREQ` is 1 MHz on PortalPlayer).
    ///
    /// `CPU_INT_EN` and `CPU_INT_DIS` are write-to-set and write-to-clear, with the real state in
    /// `CPU_INT_EN_STAT`. Rather than intercept those writes in the bus, the writes land in the
    /// MMIO region as ordinary stores and are consumed here — which keeps the byte-level `Bus`
    /// free of device knowledge.
    /// `pub` so a test can drive exactly one tick. Everything this touches — the DMA engines, the
    /// forced-interrupt latches, the timers — is edge-sensitive to how often it runs, and a test
    /// that had to reach it through `run` would be measuring the scheduler instead of the device.
    pub fn service_interrupts(&mut self) {
        self.mem.internal = true;
        let r = self.service_interrupts_inner();
        self.mem.internal = false;
        r
    }

    /// Run whatever the two PP502x DMA controllers have been armed to do, and hold their
    /// completion lines.
    ///
    /// Driven from `service_interrupts` rather than from a bus intercept for the same reason the
    /// interrupt controller is: these are ordinary backing-store registers, the firmware programs
    /// them with a read-modify-write sequence, and only the *last* store — the one that sets
    /// `DMA_CMD_START` — means anything. Polling for that bit costs two reads per channel per
    /// service tick and keeps device knowledge out of the byte-level `Bus`.
    ///
    /// The transfer is issued through `Memory` itself, so a destination that is a modelled device
    /// sees it as bus traffic. That is not incidental: the one transfer this exists for pushes
    /// `vmcs.bin` at `0x30000000`, which is the video co-processor's host **port**, not a window.
    fn service_pp_dma(&mut self) {
        let (mut set, mut touched) = (0u32, 0u32);
        for (i, c) in PP_DMA.iter().enumerate() {
            let irq = if i == 0 { self.mem.pp_dma_irq.unwrap_or(c.irq) } else { c.irq };
            if self.mem.read32(c.master) & DMA_MASTER_CONTROL_EN == 0 {
                continue;
            }
            let mut line = false;
            let mut master = 0u32;
            for ch in 0..c.n {
                let base = c.chans + 0x20 * ch;
                if self.mem.read32(base + DMA_CMD) & DMA_CMD_START != 0 {
                    self.run_pp_dma(base);
                }
                let cmd = self.mem.read32(base + DMA_CMD);
                let status = self.mem.read32(base + DMA_STATUS);
                if status & DMA_STATUS_INTR == 0 {
                    continue;
                }
                // Rockbox's `dma_tx_stop` acknowledges by clearing `DMA_CMD_INTR` and then
                // spinning until `DMA0_STATUS & (BUSY|INTR)` reads clear, so dropping CMD's
                // interrupt bit has to drop the status latch too or that spin never ends.
                if cmd & DMA_CMD_INTR == 0 {
                    self.mem.write32(base + DMA_STATUS, status & !DMA_STATUS_INTR);
                } else {
                    line = true;
                    master |= 1 << (DMA_MASTER_STATUS_CH0 + ch);
                }
            }
            self.mem.write32(c.master + DMA_MASTER_STATUS, master);
            // Level, not pulse: the line follows the status latch, so a handler that returns
            // without acknowledging is re-entered rather than losing the completion.
            //
            // Accumulated across both controllers before being applied, because they can share a
            // line. Clearing per controller looked right and was not: with both pointed at IRQ 26
            // the second one's "nothing pending" wiped the first one's completion every service
            // tick, and the run came out byte-identical to one with no DMA model at all — which
            // reads exactly like "the interrupt was acknowledged".
            if line {
                set |= 1 << irq;
            }
            touched |= 1 << irq;
        }
        self.mem.int_pending = (self.mem.int_pending & !touched) | set;
    }

    /// Move one channel's bytes and post its completion.
    ///
    /// The peripheral address does **not** advance. RetailOS's chunking loop at `0x00286ff4`
    /// walks the source forward 64 KB per iteration (`add r5, r5, r8, lsl #2`, r8 = 0x4000) and
    /// re-uses the *same* destination word every time — it loads it from `[sp, #0xc]` and never
    /// updates it — so an incrementing peripheral address would put the second chunk 64 KB past
    /// the port. The co-processor keeps its own auto-incrementing write pointer (`Bcm::wr_addr`),
    /// which is what makes a fixed port correct rather than merely convenient.
    fn run_pp_dma(&mut self, base: u32) {
        let cmd = self.mem.read32(base + DMA_CMD);
        let len = (cmd & DMA_SIZE_MASK) + 4;
        let ram = self.mem.read32(base + DMA_RAM_ADDR);
        let per = self.mem.read32(base + DMA_PER_ADDR);
        let to_per = cmd & DMA_CMD_RAM_TO_PER != 0;
        let (src, dst) = if to_per { (ram, per) } else { (per, ram) };
        for off in (0..len).step_by(4) {
            let w = self.mem.read32(if to_per { src + off } else { src });
            self.mem.write32(if to_per { dst } else { dst + off }, w);
        }
        self.mem.pp_dma_transfers += 1;
        self.mem.pp_dma_bytes += len as u64;
        self.mem.pp_dma_log.push((base, src, dst, len));
        // `DMA_CMD_SINGLE` is "stop on complete, no auto reload", and every transfer RetailOS
        // arms sets it. Only START drops; the size field stays, because `dma_tx_stop` reads it
        // back out of CMD to work out how much of the buffer was consumed.
        self.mem.write32(base + DMA_CMD, cmd & !DMA_CMD_START);
        // "DMA0_STATUS will have been reloaded automatically with size in DMA0_CMD" — Rockbox
        // `pcm-pp.c`. BUSY is never observable here: the copy is instantaneous, so the channel is
        // already idle by the time any instruction can look.
        self.mem.write32(base + DMA_STATUS, DMA_STATUS_INTR | (cmd & DMA_SIZE_MASK));
    }

    fn service_interrupts_inner(&mut self) {
        const CPU_INT_STAT: u32 = 0x6000_4000;
        const INT_STAT: u32 = 0x6000_4010;
        const CPU_INT_EN_STAT: u32 = 0x6000_4020;
        const CPU_INT_EN: u32 = 0x6000_4024;
        const CPU_INT_DIS: u32 = 0x6000_4028;
        // The second bank, IRQs 32..63. Same layout, 0x100 higher. RetailOS's kernel init clears
        // both banks in six consecutive stores at 0x1604..0x1618, which is what identifies these
        // as real registers rather than a guess from the header file.
        const CPU_HI_INT_STAT: u32 = 0x6000_4100;
        const HI_INT_STAT: u32 = 0x6000_4110;
        const CPU_HI_INT_EN_STAT: u32 = 0x6000_4120;
        const CPU_HI_INT_EN: u32 = 0x6000_4124;
        const CPU_HI_INT_DIS: u32 = 0x6000_4128;
        // Software-raised interrupts. Rockbox names all six registers and uses none of them;
        // RetailOS uses them as its deferred-work mechanism, which is why they matter here. Its
        // DMA ISR finishes by writing `INT_FORCED_SET = 1 << 13` at 0x001fc840 — the completion
        // callback runs at task level on line 13, not in the ISR.
        const INT_FORCED_STAT: u32 = 0x6000_4014;
        const INT_FORCED_SET: u32 = 0x6000_4018;
        const INT_FORCED_CLR: u32 = 0x6000_401c;
        const HI_INT_FORCED_STAT: u32 = 0x6000_4114;
        const HI_INT_FORCED_SET: u32 = 0x6000_4118;
        const HI_INT_FORCED_CLR: u32 = 0x6000_411c;
        const TIMER_CFG: [u32; 2] = [0x6000_5000, 0x6000_5008];

        // Write-to-set / write-to-clear, with the real state in EN_STAT — see the doc comment.
        let consume = |m: &mut Memory, en_stat: u32, en: u32, dis: u32| {
            let mut v = m.read32(en_stat);
            let set = m.read32(en);
            if set != 0 {
                v |= set;
                m.write32(en, 0);
            }
            let clr = m.read32(dis);
            if clr != 0 {
                v &= !clr;
                m.write32(dis, 0);
            }
            m.write32(en_stat, v);
            v
        };
        let enabled = consume(&mut self.mem, CPU_INT_EN_STAT, CPU_INT_EN, CPU_INT_DIS);
        let enabled_hi = consume(&mut self.mem, CPU_HI_INT_EN_STAT, CPU_HI_INT_EN, CPU_HI_INT_DIS);
        // Identical set/clear/state shape to the enable trio, so the same consumer serves. The
        // kernel's own init writes `0xffffffff` to both CLR registers at 0x1618 and 0x160c, which
        // is what says these are write-to-clear rather than plain words.
        let forced = consume(&mut self.mem, INT_FORCED_STAT, INT_FORCED_SET, INT_FORCED_CLR);
        let forced_hi =
            consume(&mut self.mem, HI_INT_FORCED_STAT, HI_INT_FORCED_SET, HI_INT_FORCED_CLR);

        self.service_pp_dma();
        // Before the `pending_hi` snapshot below, or a packet posted this tick would not reach
        // CPU_HI_INT_STAT until the next one.
        self.mem.service_clickwheel();

        let now = self.mem.usec;
        for i in 0..2 {
            let cfg = self.mem.read32(TIMER_CFG[i]);
            if cfg & 0x8000_0000 == 0 {
                self.timer_next[i] = 0;
                continue;
            }
            let period = (cfg & 0x1fff_ffff).wrapping_add(1).max(1);
            if self.timer_next[i] == 0 {
                self.timer_next[i] = now.wrapping_add(period);
            } else if now.wrapping_sub(self.timer_next[i]) < 0x8000_0000 {
                self.mem.int_pending |= 1 << i;
                // Repeat bit clear means one-shot: disarm rather than re-arming.
                self.timer_next[i] =
                    if cfg & 0x4000_0000 != 0 { now.wrapping_add(period) } else { u32::MAX };
            }
        }

        // The drive's completion, armed by `Memory::arm_ide_irq` and due now.
        if let Some(due) = self.mem.ide_irq_due {
            if now.wrapping_sub(due) < 0x8000_0000 {
                self.mem.fire_ide_irq();
            }
        }

        // A forced bit is a real pending source; it is kept out of `int_pending` so that the
        // firmware's own `INT_FORCED_CLR` stays the only thing that can retire it.
        let pending = self.mem.int_pending | forced;
        let pending_hi = self.mem.int_pending_hi | forced_hi;
        self.mem.write32(INT_STAT, pending);
        // Bit 30 of the *low* status is the second bank's aggregate — Rockbox calls it `HI_IRQ`.
        // It is not a source of its own; it says "something in the hi bank is asserting", and
        // Apple's object demux masks it out of the low status precisely because it is not one.
        //
        // We never raised it, and that single omission was the whole of Wall B. Apple's installed
        // ISR at `0x00277128` gates its entire hi-bank arm on `tst r4, #0x40000000`, so the wheel
        // decoder at `0x00281350` — which returns semaphore `0x7f`, exactly what `SerialOptoTask`
        // has been pended on since tick 66 — was unreachable no matter how many frames the wheel
        // posted. See research/10 Addendum 17 §8.
        let hi_aggregate = if pending_hi & enabled_hi != 0 { 1 << 30 } else { 0 };
        self.mem.write32(CPU_INT_STAT, (pending & enabled) | hi_aggregate);
        self.mem.write32(HI_INT_STAT, pending_hi);
        self.mem.write32(CPU_HI_INT_STAT, pending_hi & enabled_hi);
        if pending & enabled != 0 || pending_hi & enabled_hi != 0 {
            self.irqs_asserted += 1;
            if self.cpu.irq() {
                self.irqs_taken += 1;
                // The drive's interrupt is a level, not a pulse: ATA asserts INTRQ at command
                // completion and holds it until the host acknowledges. Delivering it is not an
                // acknowledgement, so nothing is cleared here — the two real acks are a read of
                // the primary status register (`read8_inner`) and a write of IDE0_CFG's clear bits
                // (`write8_inner`), and both drop the line.
                if pending & enabled & (1 << IDE_IRQ) != 0 {
                    self.mem.ide_irq_delivered += 1;
                }
            }
        }
    }

    pub fn run(&mut self, budget: usize) -> Stop {
        // Regions, aliases and device mappings are all configured before a run; clearing here means
        // a cached resolution can never outlive the layout it was computed against.
        self.mem.invalidate_fast();
        let mut prev_pc = self.cpu.regs[15];
        for _ in 0..budget {
            // Before the fetch, so a taken IRQ lands on the vector rather than one instruction
            // past it. Rate-limited because it costs several memory accesses.
            if self.mem.usec_timer.is_some() && self.executed & 0x3f == 0 {
                self.service_interrupts();
            }
            let pc = self.cpu.regs[15];
            // So an unmapped access can name the instruction that made it.
            self.mem.pc = pc;
            self.mem.icount = self.executed as u64;

            if pc == self.exit_addr {
                return Stop::Returned;
            }

            // A trace of *which code ran*, for functions that resist being read.
            //
            // FUN_000103d4 -- the one that turns the key files into a DRM context -- is control-flow
            // flattened: a 64-way computed dispatch whose blocks all branch back to a dispatcher, with
            // mixed-arithmetic obfuscation inside them. There is no top-to-bottom path to follow, so
            // reading it statically is expensive and uncertain. Watching it execute is neither: the
            // machine is ours to the instruction, and a state machine that resists reading does not
            // resist being observed.
            //
            // Two bounds keep this from being a liability. The range test is one compare against a
            // `None` in the overwhelming case, and the log is capped -- an instrument that fills
            // memory during a long boot would be worse than no instrument.
            // **A call is identified by the link register, not by reading the instruction.**
            //
            // The first version peeked the word at the previous PC and tested the `bl` encoding.
            // That is wrong here: RetailOS executes through the low alias at 0x00000000, and
            // `peek32` resolves low addresses against NOR rather than SDRAM -- so it was decoding
            // the boot ROM's bytes as if they were the instruction that had just run, and reporting
            // the coincidences as calls. The genuine edges were missing and the noise was not.
            //
            // `bl` writes the return address into r14. So: a discontinuity whose new r14 points one
            // instruction past where we just were is a call, whatever alias it executed through and
            // whatever the memory map says.
            if let Some(from) = self.mem.trace_calls_from {
                if self.executed as u64 >= from
                    && pc != prev_pc.wrapping_add(4)
                    && self.cpu.regs[14] == prev_pc.wrapping_add(4)
                    && self.mem.call_trace.len() < PC_TRACE_CAP
                {
                    self.mem.call_trace.push((prev_pc, pc, self.executed as u64));
                }
            }
            prev_pc = pc;

            if let Some((addr, n)) = self.mem.regs_at {
                if pc == addr && self.mem.regs_seen.len() < n {
                    let mut r = [0u32; 16];
                    r.copy_from_slice(&self.cpu.regs);
                    let at = self.executed as u64;
                    self.mem.regs_seen.push((at, r));
                }
            }

            if let Some(h) = self.mem.pc_hist.as_mut() {
                let b = (pc >> 6) as usize;
                if b < h.len() {
                    h[b] += 1;
                }
            }

            if let Some((lo, hi)) = self.mem.trace_pc {
                if pc >= lo && pc <= hi && self.mem.pc_trace.len() < PC_TRACE_CAP {
                    self.mem.pc_trace.push((pc, self.executed as u64));
                }
            }

            if !self.breakpoints.is_empty() && self.breakpoints.contains(&pc) {
                self.break_log.push((pc, self.cpu.regs));
            }
            if !self.stop_at.is_empty() {
                for i in 0..self.stop_at.len() {
                    if self.stop_at[i].0 != pc {
                        continue;
                    }
                    self.stop_at[i].1 = self.stop_at[i].1.saturating_sub(1);
                    if self.stop_at[i].1 == 0 {
                        return Stop::StopPoint(pc);
                    }
                }
            }
            if !self.sum_at.is_empty() {
                for i in 0..self.sum_at.len() {
                    let (at, addr, len) = self.sum_at[i];
                    if at != pc {
                        continue;
                    }
                    let mut sum = 0u32;
                    for k in 0..len {
                        sum = sum.wrapping_add(self.mem.read8(addr.wrapping_add(k)) as u32);
                    }
                    let mut head = [0u8; 16];
                    for (k, b) in head.iter_mut().enumerate() {
                        *b = self.mem.read8(addr.wrapping_add(k as u32));
                    }
                    self.sum_at_log.push((pc, addr, sum, head));
                }
            }
            // Sampled before the instruction runs so the PC recorded alongside a change is the
            // instruction that *caused* it, not the one after.
            let watched = self.watch.map(|a| (a, self.mem.read32(a)));

            let trapped = if (self.trap_lo..=self.trap_hi).contains(&pc) {
                self.traps.get(&pc).copied()
            } else {
                None
            };
            if let Some((fi, gi)) = trapped {
                let return_to = self.cpu.regs[14];
                self.trace.push(Call {
                    framework: self.names[fi].clone(),
                    index: gi,
                    args: [
                        self.cpu.regs[0],
                        self.cpu.regs[1],
                        self.cpu.regs[2],
                        self.cpu.regs[3],
                    ],
                    stack: {
                        let sp = self.cpu.regs[13];
                        [
                            self.mem.read32(sp),
                            self.mem.read32(sp.wrapping_add(4)),
                            self.mem.read32(sp.wrapping_add(8)),
                            self.mem.read32(sp.wrapping_add(12)),
                        ]
                    },
                    return_to,
                });
                let name = &self.names[fi];
                let result = match self.stubs.get(&(name.clone(), gi)) {
                    Some(Stub::Alloc) => self.alloc(self.cpu.regs[0]),
                    Some(Stub::Free { arg }) => {
                        let p = self.cpu.regs[*arg];
                        self.free(p);
                        0
                    }
                    Some(Stub::Value(v)) => *v,
                    Some(Stub::Clock { arg, step }) => {
                        let (dst, step) = (self.cpu.regs[*arg], *step);
                        self.clock = self.clock.wrapping_add(step);
                        let now = self.clock;
                        self.mem.write32(dst, now);
                        now
                    }
                    Some(Stub::GlClearColor) => {
                        for i in 0..4 {
                            self.clear_color[i] = f32::from_bits(self.cpu.regs[i]);
                        }
                        0
                    }
                    Some(Stub::GlClear) => {
                        self.clears += 1;
                        const GL_COLOR_BUFFER_BIT: u32 = 0x4000;
                        if self.cpu.regs[0] & GL_COLOR_BUFFER_BIT != 0 {
                            let px = [
                                (self.clear_color[0].clamp(0.0, 1.0) * 255.0) as u8,
                                (self.clear_color[1].clamp(0.0, 1.0) * 255.0) as u8,
                                (self.clear_color[2].clamp(0.0, 1.0) * 255.0) as u8,
                            ];
                            for chunk in self.framebuffer.chunks_exact_mut(3) {
                                chunk.copy_from_slice(&px);
                            }
                        }
                        0
                    }
                    Some(Stub::GlVertexAttribPointer) => {
                        let sp = self.cpu.regs[13];
                        let idx = self.cpu.regs[0] as usize;
                        if idx < 8 {
                            self.arrays[idx] = Some(VertexArray {
                                size: self.cpu.regs[1] as usize,
                                stride: self.mem.read32(sp) as usize,
                                ptr: self.mem.read32(sp.wrapping_add(4)),
                            });
                        }
                        0
                    }
                    Some(Stub::FileOpen { path, out }) => {
                        let (pa, oa) = (self.cpu.regs[*path], self.cpu.regs[*out]);
                        let name = self.read_cstr(pa, 256);
                        let handle = self.open_file(&name);
                        if oa != 0 {
                            self.mem.write32(oa, handle);
                        }
                        self.file_log.push(format!(
                            "open {:?} -> handle {} ({})",
                            name,
                            handle,
                            if handle == 0 { "MISS" } else { "ok" }
                        ));
                        // Zero is success in the calling convention seen so far.
                        0
                    }
                    Some(Stub::FileRead { handle, buffer, length, out }) => {
                        let h = self.cpu.regs[*handle] as usize;
                        let (buf, len, oa) =
                            (self.cpu.regs[*buffer], self.cpu.regs[*length], self.cpu.regs[*out]);
                        let got = self.read_file(h, buf, len);
                        if oa != 0 {
                            self.mem.write32(oa, got);
                        }
                        self.file_log
                            .push(format!("read handle {h} len {len} -> {got} bytes"));
                        0
                    }
                    Some(Stub::InputPoll { arg, offset }) => {
                        let dst = self.cpu.regs[*arg].wrapping_add(*offset);
                        let ev = if self.input_queue.is_empty() {
                            0
                        } else {
                            self.input_queue.remove(0)
                        };
                        self.mem.write32(dst, ev);
                        self.polls += 1;
                        if ev != 0 {
                            self.file_log.push(format!("input event {ev:#010x} -> {dst:#010x}"));
                        }
                        0
                    }
                    Some(Stub::GlBindTexture) => {
                        self.bound_texture = self.cpu.regs[1];
                        0
                    }
                    Some(Stub::GlCompressedTexImage2D) => {
                        let sp = self.cpu.regs[13];
                        let w = self.cpu.regs[3] as usize;
                        let h = self.mem.read32(sp) as usize;
                        let data = self.mem.read32(sp.wrapping_add(12));
                        self.upload_paletted(w, h, data);
                        0
                    }
                    Some(Stub::GlDrawArrays) => {
                        let (mode, first, count) =
                            (self.cpu.regs[0], self.cpu.regs[1], self.cpu.regs[2]);
                        self.draw_arrays(mode, first, count);
                        0
                    }
                    Some(Stub::GlSwap) => {
                        self.frames_presented += 1;
                        0
                    }
                    Some(Stub::WriteOut { arg, offset, value, ret }) => {
                        let (dst, off, value, ret) = (self.cpu.regs[*arg], *offset, *value, *ret);
                        self.mem.write32(dst.wrapping_add(off), value);
                        ret
                    }
                    _ => 0,
                };
                self.cpu.regs[0] = result;
                self.cpu.regs[15] = return_to;
                // Stubs write guest memory too (FileRead fills a buffer, InputPoll stores an
                // event). Without this the trap path would `continue` past the watch and a handle
                // handed over by RetailOS -- the single most likely source -- would be invisible.
                if let Some((addr, old)) = watched {
                    let new = self.mem.read32(addr);
                    if new != old {
                        self.watch_log.push((pc, old, new));
                    }
                }
                continue;
            }

            // Only a *semihosting* SWI, not every SWI. ARM semihosting is `SWI 0x123456`; anything
            // else belongs to the firmware's own handler. Intercepting the whole vector hijacked
            // Apple's ROM — its syscalls were being read as semihosting ops, and one of them landed
            // on SYS_EXIT, ending the run at 90M instructions with an empty "firmware output".
            //
            // RetailOS *is* a debug build and does use real semihosting (its panic dumps arrive
            // this way), so the check has to be on the immediate rather than on the boot path.
            if pc == SEMIHOSTING_VECTOR && {
                let caller = self.cpu.regs[14].wrapping_sub(4);
                self.mem.read32(caller) & 0x00ff_ffff == 0x0012_3456
            } {
                if self.semihost() {
                    return Stop::Exited;
                }
                continue;
            }

            if !self.is_mapped(pc) {
                return Stop::Lost(pc);
            }
            if let Some(at) = self.snap_at {
                if self.executed >= at {
                    return Stop::SnapshotPoint;
                }
            }

            self.executed += 1;
            // The core asked to be switched off. Rather than halting the interpreter — which would
            // stall a machine whose only other core we do not run — jump the clock to whichever
            // interrupt is due first, which is what the core would have woken on. Nothing is due
            // when no timer is armed and no drive completion is pending, and then the write is a
            // no-op: a real core would wait for an external event we have no model of, and
            // pretending otherwise would invent time out of nothing.
            if self.mem.cpu_sleep {
                self.mem.cpu_sleep = false;
                let now = self.mem.usec;
                let due = self
                    .timer_next
                    .iter()
                    .copied()
                    .chain(self.mem.ide_irq_due)
                    .filter(|d| *d != u32::MAX && *d != 0)
                    .map(|d| d.wrapping_sub(now))
                    // Already due, or so far past that it wrapped: nothing to skip.
                    .filter(|delta| *delta < 0x8000_0000)
                    .min();
                if let Some(delta) = due {
                    self.mem.slept_usec = self.mem.slept_usec.wrapping_add(delta);
                    self.mem.sleeps += 1;
                }
            }
            // Advance the microsecond clock. The PP5021C runs at roughly 75 MHz and this
            // interpreter is one instruction per step, so ~75 instructions is ~1 µs. The ratio only
            // has to be plausible: firmware compares elapsed against its own timeouts, so what
            // matters is that time moves forward at a sane rate, not that it matches real silicon.
            if self.mem.usec_timer.is_some() {
                self.mem.usec =
                    (self.executed / self.instr_per_usec.max(1)) as u32 + self.mem.slept_usec;
            }
            // Sampling profiler. A run that ends in "BudgetExhausted" says nothing about where the
            // time went, and the 16-entry tail of `last instructions` only ever shows the innermost
            // loop — never which caller keeps re-entering it. Sampled rather than exact so it costs
            // nothing when idle, and bucketed by 16 bytes to keep the map small.
            if let Some(p) = &mut self.profile {
                let n = self.executed as u64;
                let inside = self.profile_window.is_none_or(|(a, b)| n >= a && n < b);
                if inside && self.executed & 0x3f == 0 {
                    *p.entry(pc & !0xf).or_insert(0) += 1;
                }
            }
            if self.novelty.is_some() {
                // Bucket index: RetailOS's low mirror and IRAM are the only code regions, folded
                // into one 22-bit space. A collision costs a missed novelty record, never a wrong
                // one, because the map still keys on the real address.
                let b = ((pc >> 4) ^ (pc >> 26)) as usize & 0x3f_ffff;
                let (word, bit) = (b >> 6, 1u64 << (b & 63));
                if self.seen_bits[word] & bit == 0 {
                    self.seen_bits[word] |= bit;
                    let n = self.executed as u64;
                    self.last_novel = n;
                    self.last_novel_sleeps = self.mem.sleeps;
                    self.novelty.as_mut().unwrap().insert(pc & !0xf, n);
                }
                // Checked here rather than every instruction: this block already costs a bitset
                // probe, and idleness cannot begin on an instruction that just found new code.
                if let Some(win) = self.stop_when_idle {
                    if self.executed as u64 - self.last_novel >= win {
                        return Stop::Idle;
                    }
                }
            }
            // Argument capture on arrival. The bloom is what keeps this off the hot path: one
            // 64-bit test rejects every PC that is not watched, and the linear scan runs only on a
            // hit or a collision.
            if self.enter_bloom & (1u64 << ((pc >> 2) & 63)) != 0 && self.enter_pcs.contains(&pc) {
                let r = &self.cpu.regs;
                let (args, lr, n) = ([r[0], r[1], r[2], r[3]], r[14], self.executed as u64);
                // The caller histogram is tallied here, not derived from the log below it — the log
                // caps at 65 536 and the histogram is what the instrument table tells readers to
                // trust when the detail rows are truncated.
                *self.enter_callers.entry((pc, lr)).or_insert(0) += 1;
                self.enter_log.push((pc, lr, args, n));
            }
            // Bypass #17, off unless asked for: return from `KS_pend` as though the semaphore had
            // already been signalled. Placed after the arrival capture so `--enterlog` on the same
            // address still sees — and counts — every pend the ablation then eats.
            //
            // Returning is the whole of it. The wrapper's own frame is not built until the next
            // instruction (`str lr, [sp,#-4]!`), so nothing has to be unwound; `r0` is forced to 0
            // because the wrapper would have read its result out of the request struct's `+0x04`
            // slot, which `0x000a6938` pre-zeroes and only the kernel ever fills.
            if !self.force_sems.is_empty()
                && pc == self.force_sem_pend_pc
                && self.force_sems.contains(&self.cpu.regs[0])
            {
                let (sem, lr, n) = (self.cpu.regs[0], self.cpu.regs[14], self.executed as u64);
                self.force_sem_log.push((lr, sem, n));
                self.cpu.regs[0] = 0;
                self.cpu.regs[15] = lr;
                continue;
            }
            // Bypass #17 stage 2. `r2 != 0` is the loop's own "some buffer is still in use" flag,
            // so this fires only where the code was about to spin — never on a free ring.
            if self.force_vc_retire && pc == 0x0015_9bc8 && self.cpu.regs[2] != 0 {
                let ch = self.cpu.regs[0];
                for i in 0..4 {
                    self.mem.write8(ch.wrapping_add(0x18 + i), 0);
                }
                self.force_retire_log.push((ch, self.executed as u64));
            }
            // Call history. The instruction ring shows only the innermost loop, so "what did the
            // firmware *do* before it got stuck" has been unanswerable — the single biggest gap in
            // this toolset. A BL is `cond 101 1 imm24`, sign-extended, offset from pc+8.
            if self.call_log_on {
                let w = self.mem.read32(pc);
                if (w >> 25) & 0x7 == 0b101 && (w >> 24) & 1 == 1 {
                    let imm = ((w & 0x00ff_ffff) << 8) as i32 >> 6; // sign-extend, then <<2
                    let target = pc.wrapping_add(8).wrapping_add(imm as u32);
                    // A ring, not a capped log. Keeping the *first* 4096 is the saturation trap
                    // that has already produced two false conclusions in this project; what matters
                    // is what happened most recently.
                    if self.call_log.len() < CALL_LOG {
                        self.call_log.push((pc, target));
                    } else {
                        self.call_log[self.call_at % CALL_LOG] = (pc, target);
                    }
                    self.call_at += 1;
                }
            }
            self.history[self.history_at % HISTORY] = pc;
            self.history_at += 1;

            // Indirect branches are the edges static analysis cannot resolve: the games are C++
            // with virtual dispatch, so every interesting call goes through a vtable slot.
            // Recording (site, target) turns those unknown edges into observed ones.
            let indirect = if self.log_indirect || self.edges.is_some() {
                let w = self.mem.read32(pc);
                let cond_ok = (w >> 28) != 0xF;
                let bx = w & 0x0FFF_FFF0 == 0x012F_FF10;
                // `mov pc, rX` and friends: data-processing writing R15, register operand.
                let dp_pc = (w >> 26) & 3 == 0 && (w >> 12) & 0xF == 0xF && (w >> 25) & 1 == 0;
                // `ldr pc, [...]`, excluding the PC-relative import thunks already traced.
                let ldr_pc =
                    (w >> 26) & 3 == 1 && (w >> 20) & 1 == 1 && (w >> 12) & 0xF == 0xF
                        && (w >> 16) & 0xF != 15;
                cond_ok && (bx || dp_pc || ldr_pc)
            } else {
                false
            };

            let r0_before = self.cpu.regs[0];
            let unmapped_before = self.mem.unmapped_seq;
            // Snapshotted unconditionally rather than behind the flag: 16 word copies is cheaper
            // than the branch-predictor cost of a second test on this path, and it has to be the
            // *pre*-step file — the load that faults overwrites its own address register.
            let regs_before = self.cpu.regs;

            self.cpu.step(&mut self.mem);

            if self.mem.unmapped_seq != unmapped_before {
                self.unmapped_regs.push((pc, regs_before));
            }

            // A transition, not a match: r0 sits at the value for the whole return path, so
            // logging equality would bury the one instruction that caused it under its callers.
            if let Some(v) = self.retwatch {
                if self.cpu.regs[0] == v && r0_before != v {
                    let lr = self.cpu.regs[14];
                    // Per-instruction tally outside the cap: the report groups by producing PC, and
                    // "one site repeated in a loop is one answer" is only true if the count is real.
                    let e = self.retwatch_sites.entry(pc).or_insert((0, lr));
                    e.0 += 1;
                    self.retwatch_log.push((pc, lr));
                }
            }

            if let Some((addr, old)) = watched {
                let new = self.mem.read32(addr);
                if new != old {
                    self.watch_log.push((pc, old, new));
                }
            }

            if self.edges.is_some() {
                let target = self.cpu.regs[15];
                // A branch is any control transfer that did not simply fall through. Both the
                // direct BL (already visible statically) and the indirect ones (not) are recorded,
                // so the graph is complete rather than complementary.
                // EVERY non-fall-through transfer, not just BL and indirect. A plain `B` is how
                // ARM compilers emit a tail call, so restricting to BL reported *zero* runtime
                // callers for a function that demonstrably runs. Deduplication keeps this bounded
                // by distinct edges — loop back-edges included — rather than by executed
                // instructions.
                if target != pc.wrapping_add(4) {
                    *self.edges.as_mut().unwrap().entry((pc, target)).or_insert(0) += 1;
                }
            }
            if indirect {
                let target = self.cpu.regs[15];
                // Ignore returns into the caller's own frame — the interesting edges are the
                // ones that reach code the static call graph could not connect.
                if target.abs_diff(pc) > 0x40 {
                    // Distinct edges are counted here rather than tallied from the log, which caps.
                    *self.indirect_edges.entry((pc, target)).or_insert(0) += 1;
                    self.indirect_log.push((pc, target));
                }
            }
        }
        Stop::BudgetExhausted
    }

    /// Whether `addr` can be fetched from. Goes through the same alias resolution as data access —
    /// firmware jumps into the uncached view, so an alias that only worked for loads and stores
    /// would report code as unmapped and stop the run at the first such branch.
    fn is_mapped(&self, addr: u32) -> bool {
        let addr = self.mem.translate(addr);
        self.mem
            .regions
            .iter()
            .any(|r| (addr.wrapping_sub(r.base) as usize) < r.data.len())
    }

    /// Distinct imports reached, as `framework -> sorted indices`. This is the number that
    /// matters: how much of the advertised ABI a real game actually exercises.
    pub fn reached(&self) -> HashMap<&str, Vec<usize>> {
        let mut out: HashMap<&str, Vec<usize>> = HashMap::new();
        for c in &self.trace {
            let e = out.entry(c.framework.as_str()).or_default();
            if !e.contains(&c.index) {
                e.push(c.index);
            }
        }
        for v in out.values_mut() {
            v.sort_unstable();
        }
        out
    }
}

// ---------------------------------------------------------------- helpers

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn cstr_at(buf: &[u8], off: usize) -> Option<String> {
    let end = buf.get(off..)?.iter().position(|&c| c == 0)? + off;
    let s = &buf[off..end];
    if s.len() > 64 || !s.iter().all(|&c| (0x20..0x7f).contains(&c)) {
        return None;
    }
    Some(String::from_utf8_lossy(s).into_owned())
}

// ------------------------------------------------- external memory bus

/// The external memory bus controller — `0x70000030` and `XMB_RAM_CFG` at `0x7000003c`.
///
/// Both registers were `--rdval` hypotheses in the cold-boot recipe (ledger #1 and #2). What they
/// are was read out of the ROM's own use of them, with `--disasm` on the running machine.
///
/// **`0x70000030`** appears in no published map; `pp5020.h` names its neighbours `DEV_TIMING1`
/// (`+0x34`), `XMB_NOR_CFG` (`+0x38`) and `XMB_RAM_CFG` (`+0x3c`), so it heads the memory-controller
/// group. Three instruction sequences in the whole image touch it, and together they say what two
/// of its bits are:
///
/// ```text
/// 40001378  mov  r2, #0x70000000
/// 4000137c  ldr  r1, [r2, #0x30]
/// 40001380  tst  r1, #0x8000000      ; bit 27
/// 40001384  beq  0x4000137c          ; spin until the controller says ready
/// 40001388  ldr  r1, [r2, #0x30]
/// 4000138c  cmp  r0, #0x0            ; the argument
/// 40001390  movne r0, #0x40000000    ; bit 30
/// 40001394  bic  r1, r1, #0x40000000
/// 40001398  orr  r0, r1, r0
/// 4000139c  str  r0, [r2, #0x30]
/// ```
///
/// All six call sites pass 1, run a JEDEC NOR command sequence, then pass 0. `0x40009f88` writes
/// `0xAAAA` to `0xAAAA`, `0x5555` to `0x5554`, autoselect `0x9090`, reads the ID words back and
/// resets with `0xF0F0`; the other two pairs dispatch through a device table at `0x400150e0` whose
/// rows begin with JEDEC ID pairs (`…00ec` Samsung, `…0001` AMD, `…00bf` SST) and whose `+0x8`
/// method is a sector erase in the same command set. The object the driver works from carries the
/// magic `'Cfi!'` at `+0`. So bit 30 is the **NOR write gate**: closed, a store to flash is an
/// ordinary store; open, it is a command.
///
/// Bit 27 is **read-only ready**. Nothing in the image writes it — the three writers all reach it
/// through `bic`/`orr` of other fields — and the enable path waits for it while bit 30 is still
/// *clear*, which rules out its being an echo of bit 30 (an echo would deadlock there). A bus that
/// finishes every access inside the access is never not-ready, so this model holds it set and lets
/// the rest of the word be ordinary storage, which is what makes the ROM's read-modify-writes of
/// bits 30, 16, 11:8 and 7:4 read back.
///
/// **`0x7000003c`** is `XMB_RAM_CFG`, and the SDRAM bring-up at `0x40003590` shows the handshake:
/// write the geometry word, `orr` in bit 24, write again, spin on bit 31; then probe the array's
/// address aliasing (`0x40008ba8` writes `0x10000040`, `+0x800`, `+0x400`, `+0x200` and reads the
/// first back), fold the answer into bits 17:16, and repeat. **Bit 24 is the command and bit 31 is
/// its completion** — a handshake the firmware starts itself, which is why no static value and no
/// alternating value was ever the right shape. Applying a configuration to a modelled array takes
/// no time, so the completion lands on the kick; the point is that it lands *because of* the kick.
pub struct Xmb {
    pub base: u32,
    /// Times bit 30 went 0 -> 1 and 1 -> 0. Printed because they should come in pairs, and an
    /// unpaired open would mean the ROM left the flash writable — a real fault, not a counter.
    pub gate_opens: u64,
    pub gate_closes: u64,
    /// Times bit 24 was written set. Two per boot is the SDRAM bring-up's two configurations.
    pub ram_kicks: u64,
}

impl Xmb {
    /// Byte 3 of `+0x30` and of `+0x3c` — the only two bytes whose stored value is not simply what
    /// the firmware wrote. Everything else in the block is plain memory.
    const CTRL_HI: u32 = 0x33;
    const RAM_CFG_HI: u32 = 0x3f;
    /// Within byte 3: bit 27 -> `0x08`, bit 30 -> `0x40`, bit 24 -> `0x01`, bit 31 -> `0x80`.
    const READY: u8 = 0x08;
    const NOR_GATE: u8 = 0x40;
    const RAM_START: u8 = 0x01;
    const RAM_DONE: u8 = 0x80;

    pub fn new(base: u32) -> Self {
        Self { base, gate_opens: 0, gate_closes: 0, ram_kicks: 0 }
    }

    /// The reset value of byte 3 of `+0x30`: ready, gate closed.
    pub fn ctrl_hi_at_reset() -> u8 {
        Self::READY
    }

    /// Whether `addr` is one of the two bytes this model owns.
    pub fn owns(&self, addr: u32) -> bool {
        let off = addr.wrapping_sub(self.base);
        off == Self::CTRL_HI || off == Self::RAM_CFG_HI
    }

    /// What the register file keeps when the firmware writes `val` at `addr`, and `was` is there.
    ///
    /// Written as a filter on the stored byte rather than as an override on the read, so the state
    /// lives in the region and a snapshot carries it without knowing this device exists.
    pub fn store(&mut self, addr: u32, was: u8, val: u8) -> u8 {
        match addr.wrapping_sub(self.base) {
            Self::CTRL_HI => {
                match (was & Self::NOR_GATE != 0, val & Self::NOR_GATE != 0) {
                    (false, true) => self.gate_opens += 1,
                    (true, false) => self.gate_closes += 1,
                    _ => {}
                }
                val | Self::READY
            }
            Self::RAM_CFG_HI => {
                if val & Self::RAM_START != 0 {
                    self.ram_kicks += 1;
                    val | Self::RAM_DONE
                } else {
                    // Staging a configuration without the command bit retracts the completion:
                    // the controller has been given something new and has not been told to do it.
                    val & !Self::RAM_DONE
                }
            }
            _ => val,
        }
    }
}

// ---------------------------------------------------------------- click wheel

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
    /// Transmits started, and the commands we had no evidence for.
    pub commands: u64,
    pub unknown_commands: u64,
    /// The distinct unknown command words, capped. `unknown_commands` is the uncapped count of
    /// occurrences; this is the set, and it can be truncated — so the report says when it was.
    pub unknown: Capped<u32>,
    /// Autonomous reporting, as the firmware last set it with opcode `0x052a` — see
    /// [`ClickWheel::transmit`]. **Starts off**, because a wheel nobody has spoken to has not been
    /// told to report, and because that is the one thing about this command that is falsifiable:
    /// events injected before the firmware's own enable are suppressed instead of being silently
    /// consumed by a driver that is not listening yet.
    pub reporting: bool,
    /// `0x052a` commands seen, and the payload of the last one.
    pub set_commands: u64,
    pub last_set: Option<(u64, u8)>,
    /// Autonomous frames that were not posted because reporting was off. The only number that can
    /// distinguish "the script did nothing" from "the script was refused".
    pub frames_suppressed: u64,
    /// Times the line went from clear to asserted.
    pub irqs: u64,
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
    /// Instruction count at which this step fires.
    pub at: u64,
    pub event: WheelEvent,
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

/// Button masks, in the streaming frame's bit order relative to bit 8. Rockbox names them
/// SELECT/RIGHT/LEFT/PLAY/MENU at `0x100`/`0x200`/`0x400`/`0x800`/`0x1000`.
pub const WHEEL_SELECT: u8 = 0x01;
pub const WHEEL_RIGHT: u8 = 0x02;
pub const WHEEL_LEFT: u8 = 0x04;
pub const WHEEL_PLAY: u8 = 0x08;
pub const WHEEL_MENU: u8 = 0x10;

/// Name a button the way a script writes it. `next`/`prev` are aliases for right/left, because
/// that is what the labels on the wheel mean.
pub fn wheel_button(name: &str) -> Option<u8> {
    Some(match name {
        "select" | "center" => WHEEL_SELECT,
        "right" | "next" | "ffwd" => WHEEL_RIGHT,
        "left" | "prev" | "rew" => WHEEL_LEFT,
        "play" | "pause" => WHEEL_PLAY,
        "menu" => WHEEL_MENU,
        _ => return None,
    })
}

/// The interrupt the wheel arrives on, in the controller's **second** bank.
///
/// Not a guess. Rockbox's `button_init_device` enables exactly this line — `CPU_INT_EN = HI_MASK;
/// CPU_HI_INT_EN = I2C_MASK`, and `I2C_MASK` is `1 << (I2C_IRQ - 32)` with `I2C_IRQ` 32+8. RetailOS
/// masks the same bit around its polled button read: `mov r9, #0x100` then `str r9, [r10, #0x128]`
/// with `r10 = 0x60004000`, which is `CPU_HI_INT_DIS = 1 << 8`. The wheel shares the I²C block's
/// address space and its interrupt line.
pub const OPTO_IRQ_HI: u32 = 8;

/// How long after a transmit the wheel's reply arrives, in microseconds.
///
/// **Not cosmetic — the model is wrong without it, and the firmware says so.** Apple's sender at
/// `0x00283fa0` starts the transmit, waits for the busy bit, and *then* writes `0x0c000000` to
/// STATUS and re-arms, and only after all of that does its caller begin polling receive-ready. A
/// device that answered inside the store to CTRL would have its answer wiped by the driver's own
/// acknowledgement thirty instructions later, and every query would time out — on the shipping
/// firmware, which demonstrably works on real hardware. So the reply must land after the ack, which
/// means the round trip to the PSoC takes real time.
///
/// Measured against the driver's own patience: it gives up after `0x5dc` = 1500 µs. 100 µs is
/// unambiguously later than its arming sequence and well inside that window. Exactly the same
/// argument, and the same failure, as `IDE_COMPLETION_USEC` — this emulator has now made the
/// synchronous-completion mistake twice.
pub const OPTO_REPLY_USEC: u32 = 100;

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
    const ARM: u32 = 0x4000_0000;
    /// STATUS bit 26: a packet is waiting. Write-1-to-clear.
    const RX_READY: u32 = 0x0400_0000;
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
            commands: 0,
            unknown_commands: 0,
            unknown: Capped::new(16),
            reporting: false,
            set_commands: 0,
            last_set: None,
            frames_suppressed: 0,
            irqs: 0,
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
    fn post(&mut self, frame: u32, icount: u64) {
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
    fn apply(&mut self, ev: WheelEvent) -> Option<bool> {
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
    fn read8(&mut self, off: u32) -> Option<u8> {
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
    fn write8(&mut self, off: u32, val: u8, icount: u64, usec: u32) -> bool {
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
                w[b] = (w[b] & !(val & mask)) | (val & !mask);
                self.status = u32::from_le_bytes(w);
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

/// 96 clicks per full rotation — Rockbox `WHEELCLICKS_PER_ROTATION`, and the reason the position
/// field is seven bits with 0x5F as its top value.
pub const WHEEL_CLICKS_PER_ROTATION: u8 = 96;

/// Parse an injected sequence into the exact list of steps the run will execute.
///
/// The grammar, which `--wheel=` takes verbatim:
///
/// ```text
/// SCRIPT := STEP[,STEP…]
/// STEP   := ('@' N | '+' N) ':' ACTION       @N = at instruction N; +N = N after the previous step
/// ACTION := touch | release | hold | unhold
///         | rotate=[+-]N                      N clicks, one frame each, spaced `click_instr` apart
///         | down=BTN | up=BTN | press=BTN     press = down, then up one `click_instr` later
/// BTN    := select | menu | play | prev | next        (also center/left/right/ffwd/rew/pause)
/// N      := digits, `_` ignored, optional `k` or `M` suffix
/// ```
///
/// **Expansion happens here, not at run time.** `rotate` and `press` become their individual steps
/// before anything runs, so the schedule the trace prints is byte-for-byte the schedule that
/// executes — a script cannot mean one thing on paper and another on the machine. That is the whole
/// reason this is a parser returning a list rather than an interpreter running beside the CPU.
pub fn parse_wheel_script(spec: &str, click_instr: u64) -> Result<Vec<WheelStep>, String> {
    /// Enough for a hundred full rotations; beyond that a script is a stress test, not a sequence,
    /// and would push the printed schedule past anything a run report can carry.
    const MAX_STEPS: usize = 16384;
    let number = |t: &str| -> Result<u64, String> {
        let t = t.replace('_', "");
        let (digits, mul) = match t.strip_suffix(['k', 'K']) {
            Some(d) => (d.to_string(), 1_000u64),
            None => match t.strip_suffix(['m', 'M']) {
                Some(d) => (d.to_string(), 1_000_000),
                None => (t.clone(), 1),
            },
        };
        digits
            .parse::<u64>()
            .map(|v| v * mul)
            .map_err(|_| format!("not a number: {t:?}"))
    };
    let mut out: Vec<WheelStep> = Vec::new();
    let mut prev = 0u64;
    for raw in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (when, action) = raw
            .split_once(':')
            .ok_or_else(|| format!("step {raw:?} has no ':' — expected @N:ACTION or +N:ACTION"))?;
        let at = match when.chars().next() {
            Some('@') => number(&when[1..])?,
            Some('+') => prev + number(&when[1..])?,
            _ => return Err(format!("step {raw:?}: time must start with '@' or '+'")),
        };
        prev = at;
        let mut push = |at: u64, event: WheelEvent, out: &mut Vec<WheelStep>| {
            out.push(WheelStep { at, event });
        };
        match action {
            "touch" => push(at, WheelEvent::Touch, &mut out),
            "release" => push(at, WheelEvent::Release, &mut out),
            "hold" => push(at, WheelEvent::Hold(true), &mut out),
            "unhold" => push(at, WheelEvent::Hold(false), &mut out),
            _ => {
                let (verb, arg) = action
                    .split_once('=')
                    .ok_or_else(|| format!("step {raw:?}: unknown action {action:?}"))?;
                match verb {
                    "rotate" => {
                        let (sign, mag) = match arg.strip_prefix('-') {
                            Some(m) => (-1i8, m),
                            None => (1i8, arg.strip_prefix('+').unwrap_or(arg)),
                        };
                        let n = number(mag)?;
                        if n == 0 {
                            return Err(format!("step {raw:?}: rotate=0 does nothing"));
                        }
                        for k in 0..n {
                            push(at + k * click_instr, WheelEvent::Step(sign), &mut out);
                        }
                        // So a following `+N` is relative to the *last* click, not the first.
                        prev = at + (n - 1) * click_instr;
                    }
                    "down" | "up" | "press" => {
                        let mask = wheel_button(arg)
                            .ok_or_else(|| format!("step {raw:?}: unknown button {arg:?}"))?;
                        match verb {
                            "down" => push(at, WheelEvent::Button(mask, true), &mut out),
                            "up" => push(at, WheelEvent::Button(mask, false), &mut out),
                            _ => {
                                push(at, WheelEvent::Button(mask, true), &mut out);
                                push(at + click_instr, WheelEvent::Button(mask, false), &mut out);
                                prev = at + click_instr;
                            }
                        }
                    }
                    _ => return Err(format!("step {raw:?}: unknown action {action:?}")),
                }
            }
        }
        if out.len() > MAX_STEPS {
            return Err(format!("script expands past {MAX_STEPS} steps"));
        }
    }
    if out.is_empty() {
        return Err("empty script".into());
    }
    Ok(out)
}

/// One step, printed the way the run report prints it — so a schedule read out of a log can be
/// pasted straight back into `--wheel=`.
pub fn wheel_step_name(ev: WheelEvent) -> String {
    match ev {
        WheelEvent::Touch => "touch".into(),
        WheelEvent::Release => "release".into(),
        WheelEvent::Hold(true) => "hold".into(),
        WheelEvent::Hold(false) => "unhold".into(),
        WheelEvent::Step(1) => "rotate=+1".into(),
        WheelEvent::Step(_) => "rotate=-1".into(),
        WheelEvent::Button(mask, down) => {
            let name = match mask {
                WHEEL_SELECT => "select",
                WHEEL_RIGHT => "next",
                WHEEL_LEFT => "prev",
                WHEEL_PLAY => "play",
                WHEEL_MENU => "menu",
                _ => "?",
            };
            format!("{}={name}", if down { "down" } else { "up" })
        }
    }
}

/// `GPIOB_OUTPUT_VAL`, and the backlight dimmer's pin in it.
///
/// The brightness on this machine is **pulse-counted**, not a level in a register. Rockbox's
/// `backlight-nano_video.c` documents the protocol: drive the pin low, wait, drive it high, and the
/// dimmer moves one step — **short low (~10 us) steps up, long low (~200 us) steps down** — over a
/// range of 1..32. Nothing ever reads the level back; the counter lives in the panel's own
/// circuit, and the firmware tracks its own idea of where it is.
///
/// Rockbox pulses **GPIOD** bit 7 and uses GPIOB bit 3 only to enable the circuit. On this machine
/// it is measurably **GPIOB bit 4**: `0x6000d024` takes 42 byte-writes in the first 300 M where
/// GPIOD takes 2 and GPIOL takes 2, both of those pairs being initialisation. Apple's bootloader
/// writes 0x00 and 0x10 alternately from `0x4000e66c` and `0x4000e6d0`, and RetailOS pulses the
/// same pin once more.
///
/// Those write counts are **byte**-granular, and the stores are words — so the bootloader's
/// "sixteen writes from each of two PCs" is four pulses, not sixteen. It was briefly read as
/// sixteen, which agreed beautifully with the midpoint Rockbox's driver assumes and meant nothing.
/// The model's own count is the check: a boot ends at 19 with four steps up and one down, which is
/// the same five pulses seen from the other side.
pub const GPIOB_OUTPUT_VAL: u32 = 0x6000_d024;
pub const GPIOB_BACKLIGHT: u32 = 0x10;

/// A pin that is **not** the dimmer, kept named because finding that out cost a day.
///
/// **Measured, on a running machine, 2026-08-18.** With the whole GPIO window watched and a person
/// dragging RetailOS's own brightness slider from full to minimum, `0x6000d024` — the pin this
/// model had counted since the dimmer was written — took **zero** writes, and so did every other
/// register in the A–D bank. What moved was a second bank at `0x6000d800`, which appears neither in
/// this project's notes nor in Rockbox's `pp5020.h`:
///
/// ```text
///   0x6000d80c  +236     enable, toggled around each pulse
///   0x6000d81c  +236     direction, likewise
///   0x6000d82c  +112     the pin
/// ```
///
/// The bit is `0x80`, read straight out of the register. Rockbox's `backlight_hw_brightness` for
/// this model bit-bangs `0x80` too — on `GPIOD_OUTPUT_VAL`, one bank lower. It had the bit right
/// and the port wrong, and this emulator inherited the port from it and then guessed a different
/// bit as well, which is how a dimmer came to count an enable line.
pub const WHEEL_BITBANG_PORT: u32 = 0x6000_d82c;
pub const WHEEL_BITBANG_PIN: u32 = 0x80;

/// Where the dimmer is counted today. **Also not the brightness control** — see `KNOWN-BUGS.md`.
/// Restored after the pin above turned out to be the wheel: this one at least does not dim the
/// panel while somebody scrolls a menu.
pub const BACKLIGHT_PORT: u32 = 0x6000_d024;
pub const BACKLIGHT_PIN: u32 = 0x10;

/// A pulse low for less than this many microseconds steps the dimmer UP; longer steps it down.
/// Rockbox's two delays are 10 and 200, so anything in the middle separates them.
pub const BACKLIGHT_STEP_USEC: u32 = 100;

/// The panel's dimmer, as the pulses on [`GPIOB_OUTPUT_VAL`] leave it.
#[derive(Clone, Debug)]
pub struct Backlight {
    /// 1..32. Starts at 16, which is where Rockbox's driver says the circuit wakes up, and is the
    /// only value in this model that is assumed rather than derived.
    pub level: u8,
    /// `usec` at which the pin went low, while it is low.
    low_since: Option<u32>,
    pub steps_up: u64,
    pub steps_down: u64,
    /// Every pulse's low width, in microseconds, in order.
    ///
    /// **[`BACKLIGHT_STEP_USEC`] is inferred, not measured.** It comes from Rockbox's driver, whose
    /// two delays are 10 µs and 200 µs — and Rockbox is not the firmware this emulator runs. If
    /// Apple's own delays fall on the same side of the threshold, every pulse steps the same way,
    /// the level walks to a rail, and the dimmer looks like it does nothing. That failure is
    /// invisible from the level alone, which is why the widths are kept rather than just the
    /// verdict they produced.
    pub widths: Capped<u32>,
}

impl Default for Backlight {
    fn default() -> Self {
        Self {
            level: 16,
            low_since: None,
            steps_up: 0,
            steps_down: 0,
            widths: Capped::new(256),
        }
    }
}

impl Backlight {
    /// One write of the port. Returns true if the level moved.
    pub fn port_written(&mut self, val: u32, usec: u32) -> bool {
        let high = val & BACKLIGHT_PIN != 0;
        match (self.low_since, high) {
            // Falling edge: start timing.
            (None, false) => {
                self.low_since = Some(usec);
                false
            }
            // Rising edge: the width of the low decides the direction.
            (Some(at), true) => {
                self.low_since = None;
                let width = usec.wrapping_sub(at);
                self.widths.push(width);
                if width < BACKLIGHT_STEP_USEC {
                    self.steps_up += 1;
                    self.level = (self.level + 1).min(32);
                } else {
                    self.steps_down += 1;
                    self.level = self.level.saturating_sub(1).max(1);
                }
                true
            }
            _ => false,
        }
    }
}

/// The GPIO port-A interrupt block, from `ipodloader2/ipodhw.h` and Rockbox's `pp5020.h`, which
/// agree address for address.
///
/// A port pin raises when it is **enabled** and its level **matches `INT_LEV`**; the handler clears
/// it by writing the bit to `INT_CLR`. Ports A..D share one line, `GPIO0_IRQ = 32 + 0`.
///
/// Modelling this is what makes the hold switch exist. RetailOS reads `GPIOA_INPUT_VAL` **four
/// times in a 1.7 G boot**, from one PC, first at 111 551 914 — that is initialisation, not a poll,
/// and `HoldSwitchTask` learns about every later movement from this interrupt. Without it the
/// emulator moved the pin and told nobody, so the switch was sampled once at boot and never again.
pub const GPIOA_INT_STAT: u32 = 0x6000_d040;
pub const GPIOA_INT_EN: u32 = 0x6000_d050;
pub const GPIOA_INT_LEV: u32 = 0x6000_d060;
pub const GPIOA_INT_CLR: u32 = 0x6000_d070;
pub const GPIO_IRQ_HI: u32 = 0;

/// `GPIOA_INPUT_VAL`, and the hold switch's bit in it: active low, so **clear means engaged**.
/// `map_hardware` seeds it set (hold off) for a bare iPod; a scripted hold has to clear it, or the
/// switch is modelled in the frame and not in the line `button_hold()` reads.
pub const GPIOA_INPUT_VAL: u32 = 0x6000_d030;
pub const GPIOA_HOLD: u32 = 0x20;

impl Memory {
    /// Fire whatever the script is due to fire, and hold the interrupt line.
    ///
    /// Driven from `service_interrupts` for the same reason the DMA controllers are: the tick is
    /// already there, it runs every 64 instructions, and it is the only place that knows the clock.
    fn service_clickwheel(&mut self) {
        let Some(mut w) = self.clickwheel.take() else { return };
        let now = self.icount;
        let mut hold_moved = None;
        // The reply to a transmit, back from the wheel. Ahead of the script so that a command and a
        // scripted event landing in the same tick arrive in the order they were caused.
        if let Some((frame, due)) = w.reply {
            if self.usec.wrapping_sub(due) < 0x8000_0000 {
                w.reply = None;
                w.post(frame, now);
            }
        }
        // Strictly in order. A script is a *sequence* — a step whose instant has already gone by
        // fires at the first opportunity rather than being skipped, so a schedule can never be
        // silently half-executed.
        while w.next < w.script.len() && w.script[w.next].at <= now {
            let ev = w.script[w.next].event;
            w.next += 1;
            if let Some(on) = w.apply(ev) {
                hold_moved = Some(on);
            }
            // The physical state moves whether or not anybody is listening — a finger on a wheel
            // that has not been armed is still a finger on a wheel — but the *report* is gated on
            // the firmware having sent `0x052a` with a non-zero payload. That is what the setter
            // means, and it is the one falsifiable consequence of reading it: an event injected
            // before RetailOS's own opto init is refused by the device rather than posted into a
            // driver that is not listening. Hold still reaches GPIOA below; it is a switch, not a
            // report.
            if !w.reporting {
                w.frames_suppressed += 1;
                continue;
            }
            let f = w.stream_frame();
            w.post(f, now);
        }
        // Level, not pulse: the line follows the receive-ready flag, so a handler that returns
        // without acknowledging is re-entered rather than losing the packet — which is what the
        // drive's INTRQ does here and what the write-1-to-clear acknowledgement implies.
        let assert = w.irq_enabled && w.status & ClickWheel::RX_READY != 0 && w.ctrl & ClickWheel::ARM != 0;
        if assert {
            if self.int_pending_hi & (1 << OPTO_IRQ_HI) == 0 {
                w.irqs += 1;
            }
            self.int_pending_hi |= 1 << OPTO_IRQ_HI;
        } else {
            self.int_pending_hi &= !(1 << OPTO_IRQ_HI);
        }
        self.clickwheel = Some(w);
        if let Some(on) = hold_moved {
            let v = self.read32(GPIOA_INPUT_VAL);
            let v = if on { v & !GPIOA_HOLD } else { v | GPIOA_HOLD };
            self.set_gpioa_input(v);
        }
    }
}

// ---------------------------------------------------------------- NOR flash

/// What the chip answers reads with. A JEDEC part is a memory until a command puts it into one of
/// these, and a memory again as soon as it is reset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NorMode {
    Array,
    Autoselect,
    Cfi,
}

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
        let cmd = (val & 0xff) as u16;
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
                return Some(NorOp::Erase { start: 0, len: 0x10_0000 });
            }
            (5, _, 0x30) => {
                self.seq = 0;
                self.erases += 1;
                self.set_mode(NorMode::Array);
                return Some(NorOp::Erase { start: off & !(self.sector - 1), len: self.sector });
            }
            _ => {
                self.unknown.push((off, val));
                self.seq = 0;
            }
        }
        None
    }
}

// ---------------------------------------------------------------- ATA

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
    /// **Counted per read transfer, not per read of `ADCS1`** — and that distinction is the whole
    /// of research/10 Addendum 30. A conversion does not advance because the host happened to read
    /// one particular register of the chip; and the two halves of one result, fetched in a single
    /// I²C transfer, cannot describe different states of the converter. Counting inside
    /// `read_reg(0x30)` did both: it advanced the countdown on the first byte of a two-byte read
    /// and then answered the second byte from the *new* state, so every conversion Apple's
    /// bootloader ever observed as complete carried the value **zero**.
    busy: u8,
    /// The conversion in flight, latched into `ADCS1`/`ADCS2` only when `busy` reaches zero.
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
            busy: 0,
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

    /// One I²C transfer. `ctrl` is the PP controller's CTRL word — bit `0x20` selects a read, bits
    /// 1..2 carry `len - 1`.
    pub fn transfer(&mut self, ctrl: u8, d: [u8; 4]) {
        let len = (((ctrl >> 1) & 3) as usize + 1).min(4);
        if ctrl & 0x20 != 0 {
            // Advance the conversion **before** any byte of this transfer is served, so every byte
            // of one read describes one state of the converter. The countdown lives here rather
            // than in `read_reg` because a conversion progresses on its own; the host reading a
            // register is when it finds out, not what makes it happen.
            if self.busy > 0 {
                self.busy -= 1;
                if self.busy == 0 {
                    self.latch();
                }
            }
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
            for i in 1..len {
                self.write_reg(self.ptr.wrapping_add(i as u8 - 1), d[i]);
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
        // 4125 mV and the 0x200 catch-all is 3000 mV. The catch-all is left as-is deliberately:
        // raising it did NOT let the bootloader boot with no charger present (research/09), so the
        // threshold is not simply "a healthy cell" and inventing a higher number would be guessing.
        let value: u16 = match self.adc_values.iter().find(|&&(c, _)| c == channel) {
            Some(&(_, v)) => v,
            None => match channel {
                0x0 | 0x1 | 0xc => 0x2c0, // battery volts — 704 -> 4125 mV, a charged, not-full cell
                0x4 => 0x200,             // battery temperature — mid-scale, i.e. not hot
                _ => 0x200,               // unknown channels
            },
        };
        self.note_conversion(channel as u8, value);
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
        self.busy = 2;
        // The start bit is self-clearing, as it is on every converter that has one: the driver
        // writes it to begin and the hardware drops it when the result is latched.
        self.regs[0x2f] &= !0x01;
    }

    /// Publish the conversion that was in flight. Called when `busy` runs out, from `transfer`.
    ///
    /// Split per Rockbox: `ADCS1` holds bits 9:2 and `ADCS2` the low two plus the ready bit in
    /// bit 7, so a driver recombines them as `ADCS1 << 2 | (ADCS2 & 3)`.
    fn latch(&mut self) {
        let Some(value) = self.pending.take() else { return };
        self.regs[0x30] = (value >> 2) as u8;
        self.regs[0x31] = (value & 3) as u8 | 0x80;
        // ADCS3 bit 0 read as a conversion-ready flag. **Unverified** — the bit that the firmware
        // actually polls is not documented in anything we have, and this is the first candidate.
        // If it is wrong the I²C log will show a register being read over and over, which is
        // exactly the signature that made the old all-ones fill necessary in the first place.
        self.regs[0x32] |= 0x01;
    }
}

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
    /// Every opcode issued, with its count. See the note on `commands` for why this is separate.
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
    /// Bytes actually handed over through the data register.
    pub bytes_read: u64,
    /// The controller's own interrupt-pending latch, reported in `IDE0_CFG`.
    ///
    /// iPodLinux's `ipodloader2/ata2.c` clears it by writing `0x20`/`0x30` to `0xc3000028`
    /// ("this hopefully clears all pending intrs"), which is what identifies the register as
    /// interrupt status rather than a readiness flag. Apple's bootloader polls it instead of
    /// taking the IRQ — it runs with interrupts masked.
    irq_pending: bool,
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

/// How long after a command the drive's completion interrupt arrives, in microseconds. A 1.8"
/// drive takes milliseconds; this is the smallest delay that is still unambiguously "later than
/// the driver's own arming sequence", which is the only property the model needs.
pub const IDE_COMPLETION_USEC: u32 = 50;

/// The DMA engine's completion line, as a bit in the controller's *second* bank — IRQ 55.
/// Read off RetailOS rather than a header: its ATA driver enables this bit at 0x00233768.
pub const IDE_DMA_IRQ_HI: u32 = 23;

/// `IDE_IRQ` from Rockbox `pp5020.h`.
pub const IDE_IRQ: u32 = 23;

/// `CPU_CTRL` from Rockbox `pp5020.h`. Bit 31 is SLEEP; `0x60007004` is the COP's counterpart.
pub const CPU_CTRL: u32 = 0x6000_7000;

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

/// `irq: 26` is Rockbox's `DMA_IRQ`, whose handler demuxes on `DMA_MASTER_STATUS` bits 24..27 —
/// so the line is per *controller*, not per channel. `irq: 27` for the 0x60008000 controller is
/// inference, not a published fact: RetailOS's driver object holds four interrupt masks at
/// `+0x10..+0x1c` (`1<<24`, `1<<13`, `1<<26`, `1<<27`), and of those the run enables 26 and 27
/// back to back at @51 762 895 / @51 763 063, immediately before it configures this controller's
/// two channels. See `research/10` Addendum 8.
pub const PP_DMA: [PpDmaCtl; 2] = [
    PpDmaCtl { master: 0x6000_8000, chans: 0x6000_9000, n: 2, irq: 27 },
    PpDmaCtl { master: 0x6000_a000, chans: 0x6000_b000, n: 4, irq: 26 },
];

/// Channel register offsets and command bits, all from Rockbox `pp5020.h`.
pub const DMA_CMD: u32 = 0x00;
pub const DMA_STATUS: u32 = 0x04;
pub const DMA_RAM_ADDR: u32 = 0x10;
pub const DMA_PER_ADDR: u32 = 0x18;
pub const DMA_MASTER_STATUS: u32 = 0x04;
pub const DMA_MASTER_CONTROL_EN: u32 = 1 << 31;
/// `DMA_MASTER_STATUS_CH0` is `1 << 24` and the channels run upward from there. This register is
/// the reason one interrupt line can serve a whole controller: it is how a shared handler learns
/// which channel it was called for.
pub const DMA_MASTER_STATUS_CH0: u32 = 24;
pub const DMA_CMD_SINGLE: u32 = 1 << 26;
pub const DMA_CMD_RAM_TO_PER: u32 = 1 << 27;
pub const DMA_CMD_INTR: u32 = 1 << 30;
pub const DMA_CMD_START: u32 = 1 << 31;
pub const DMA_STATUS_INTR: u32 = 1 << 30;
/// The byte count lives in the low half of both CMD and STATUS, biased by 4: Rockbox writes
/// `DMA0_CMD = CONFIG | (size - 4) | DMA_CMD_START`, and RetailOS's own submit does the same
/// arithmetic in registers — `sub r2, r3, #0x4` at `0x0028dff8`, where r3 is the chunk length.
pub const DMA_SIZE_MASK: u32 = 0xfffc;

const ATA_BSY: u8 = 0x80;
const ATA_DRDY: u8 = 0x40;
const ATA_DSC: u8 = 0x10;
const ATA_DRQ: u8 = 0x08;
const ATA_ERR: u8 = 0x01;

impl Ata {
    /// Scalar state for a snapshot. The backing file is deliberately excluded — it is reopened by
    /// path, so a snapshot is only valid against the same disk image.
    pub fn save(&self) -> Vec<u32> {
        let mut v = vec![
            self.features as u32, self.nsector as u32, self.sector as u32, self.lcyl as u32,
            self.hcyl as u32, self.select as u32, self.status as u32, self.error as u32,
            self.pos as u32, self.remaining, self.next_lba as u32, (self.next_lba >> 32) as u32,
            self.irq_pending as u32, self.buf.len() as u32,
        ];
        v.extend(self.buf.iter().map(|b| *b as u32));
        v.extend(self.cfg.iter().map(|b| *b as u32));
        v
    }

    pub fn load(&mut self, v: &[u32]) -> bool {
        if v.len() < 14 { return false; }
        let g = |i: usize| v[i];
        self.features = g(0) as u8; self.nsector = g(1) as u8; self.sector = g(2) as u8;
        self.lcyl = g(3) as u8; self.hcyl = g(4) as u8; self.select = g(5) as u8;
        self.status = g(6) as u8; self.error = g(7) as u8;
        self.pos = g(8) as usize; self.remaining = g(9);
        self.next_lba = g(10) as u64 | ((g(11) as u64) << 32);
        self.irq_pending = g(12) != 0;
        let bl = g(13) as usize;
        if v.len() < 14 + bl + 0x100 { return false; }
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
            std::fs::OpenOptions::new().read(true).write(true).open(path)?
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
            cfg: [0; 0x100],
            cfg_writes: Capped::new(512),
            cfg_writes_by_reg: BTreeMap::new(),
            reads_log: BTreeMap::new(),
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

    /// The 512-byte IDENTIFY DEVICE response. Only the fields a driver actually consults are
    /// filled; everything else stays zero, which is legal and keeps the intent readable.
    fn identify(&self) -> Vec<u8> {
        let mut w = [0u16; 256];
        w[0] = 0x0040; // non-removable, fixed device
        w[1] = 16383; // logical cylinders (legacy CHS, ignored once LBA is on)
        w[3] = 16; // heads
        w[6] = 63; // sectors per track
        put_ata_str(&mut w[10..20], "VELDI0000000000000001"); // serial
        put_ata_str(&mut w[23..27], "1.00");
        put_ata_str(&mut w[27..47], "Emulated iPod Disk");
        w[47] = 0x8001; // max sectors per READ MULTIPLE
        w[49] = 0x0200; // LBA supported
        w[51] = 0x0200;
        w[53] = 0x0007; // words 54-58, 64-70, 88 are valid
        w[60] = (self.sectors & 0xffff) as u16; // LBA28 capacity, low
        w[61] = ((self.sectors >> 16) & 0xffff) as u16; // ...and high
        // Transfer modes. Word 53 above claims words 64-70 and 88 are valid, so leaving them zero
        // was a drive that advertises no DMA capability at all while answering SET FEATURES
        // "transfer mode = Multiword DMA 2" with success — which is not a drive that exists.
        //
        // Low byte = modes supported, high byte = mode currently selected. The selected bits are
        // the standard way a driver confirms the mode it just asked for actually took.
        w[62] = 0x0000; // single-word DMA: obsolete since ATA-3, correctly absent
        w[63] = 0x0007 | ((self.mwdma_selected as u16) << 8); // multiword DMA 0-2 supported
        w[64] = 0x0003; // PIO modes 3 and 4
        w[65] = 120; // minimum multiword DMA cycle time, ns
        w[66] = 120; // recommended
        w[67] = 120; // minimum PIO cycle time without IORDY
        w[68] = 120; // ...with IORDY
        w[88] = 0x001f | ((self.udma_selected as u16) << 8); // ultra DMA 0-4 supported
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
        self.dma_transfers.push((self.dma_lba, dest, data.len() as u32));
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
            self.commands.push((cmd, self.features, self.nsector, self.lba()));
        }
        // Uncapped, because the sample above is capped at 256 and a capped log is how this project
        // once published "LBA 22169 is never read" about a sector read at command #342. Whether the
        // firmware ever WRITES is exactly the kind of question a truncated sample answers wrongly
        // and confidently.
        *self.cmd_census.entry(cmd).or_default() += 1;
        self.error = 0;
        match cmd {
            0xec => {
                // IDENTIFY DEVICE
                self.buf = self.identify();
                self.pos = 0;
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
                let n = if self.nsector == 0 { 256 } else { self.nsector as u32 };
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
                self.remaining = if self.nsector == 0 { 256 } else { self.nsector as u32 };
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
                    self.remaining = if self.nsector == 0 { 256 } else { self.nsector as u32 };
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
            0xe7 | 0xea | 0x91 | 0xef | 0x00 => {
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

    fn read(&mut self, off: u32) -> u8 {
        *self.reads_log.entry(off).or_insert(0) += 1;
        // Controller registers: round-trip what was written, and report data-ready in IDE0_CFG.
        if off < 0x100 {
            let mut v = self.cfg[off as usize];
            if off == 0x28 && self.irq_pending {
                v |= 0x08;
            }
            return v;
        }
        match off {
            0x1e0..=0x1e3 => {
                if self.pos < self.buf.len() {
                    let b = self.buf[self.pos];
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
                } else {
                    0
                }
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

    fn write(&mut self, off: u32, val: u8) {
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
            0x1e0..=0x1e3 if self.status & ATA_DRQ != 0 && self.write_pio => {
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

/// ATA strings are ASCII, space-padded, with each 16-bit word byte-swapped.
fn put_ata_str(dst: &mut [u16], s: &str) {
    let b = s.as_bytes();
    for (i, w) in dst.iter_mut().enumerate() {
        let hi = *b.get(i * 2).unwrap_or(&b' ');
        let lo = *b.get(i * 2 + 1).unwrap_or(&b' ');
        *w = ((hi as u16) << 8) | lo as u16;
    }
}

// ---------------------------------------------------------------- BCM2722

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

/// Internal BCM addresses, from the same source.
const BCMA_COMMAND: u32 = 0x1f8;
const BCMA_STATUS: u32 = 0x1fc;
/// `BCMA_CMDPARAM` — Rockbox's own name for it, and its own gloss: *"Parameters/data for
/// commands"*. Both roles are real. `LCD_UPDATE` reads it as a bare 320x240 frame; `LCD_UPDATERECT`
/// reads it as an 8-word header followed by the rectangle the header describes.
const BCMA_CMDPARAM: u32 = 0x000e_0000;
const PANEL_W: usize = 320;
const PANEL_H: usize = 240;

/// One thing the co-processor was asked to do, in the order it was asked.
#[derive(Clone, Copy, Debug)]
pub enum BcmOp {
    /// A contiguous run of host data writes: where it started and how many halfwords it carried.
    Write { base: u32, halfwords: u64 },
    /// `CONTROL = 0x31` with a well-formed command word in `BCMA_COMMAND`.
    Command { cmd: u16 },
    /// A command that moved pixels into the frame store. `x1`/`y1` are inclusive, matching the
    /// header; `src` is where the tile was read from.
    Blit { x0: u32, y0: u32, x1: u32, y1: u32, src: u32 },
}

// ---- the GENCMD service registry, derived from RetailOS's reader ------------------------------
//
// `FUN_00288058` reads 16 bytes at internal `0x1f0` and accepts them only when the third word is
// exactly `1` and the fourth is non-zero and 4-byte aligned. The fourth word is the address of an
// **8-entry table of `u16` offsets** (`FUN_00287a6c(0x108d3bd4, w3, 0x10, 0)`). Three scanners —
// `FUN_00286aa8` (tag 2), `FUN_00287194` (tag 1), `FUN_00288978` (tag 7) — walk those eight slots,
// skip zeros, read the record at `w3 + offset`, and match a `u16` at record `+4`. The matching
// slot INDEX is the channel id; `FUN_002882c0` then pulls **0x50 bytes** of that record down.
//
// Everything below except the four placement constants is that structure. The placements are ours:
// the reader constrains the base only to be non-zero and 4-aligned, and says nothing about where
// the rings live.
const REG_BASE: u32 = 0x0004_0000; // directory base — past the 0x312a0-byte firmware upload
const REG_REC2: u32 = 0x0000_0100; // slot-0 record, as an offset from REG_BASE
const REG_TX_LO: u32 = 0x1000; // host -> co-processor ring, offsets from REG_BASE
const REG_TX_HI: u32 = 0x3000;
const REG_RX_LO: u32 = 0x3000; // co-processor -> host ring
const REG_RX_HI: u32 = 0x5000;
/// Header word 0 of every message in both directions. `FUN_0028861c` writes it; `FUN_002872fc`
/// rejects any reply whose first word is not it (`if (local_28 != DAT_002874ec) return -1`).
const GENCMD_MAGIC: u32 = 0xf1a5_5a1f;
/// Where surfaces get allocated when the host asks for address 0. Rockbox's `lcd-video.c` calls
/// `0xE0000` `BCMA_CMDPARAM` and puts the panel image there; Apple's bootloader fills exactly
/// `0xe0000..0x10581e` — one 320x240 RGB565 frame. **Chosen, not derived**: the reply format says
/// the co-processor returns an address, not which one.
const REG_SURFACE_BASE: u32 = 0x000e_0000;

/// Bytes between `rd` and `wr` in a ring `[lo, hi)` — RetailOS's own `FUN_000f5834`.
fn ring_used(lo: u32, hi: u32, rd: u32, wr: u32) -> u32 {
    if wr < rd { (hi - rd) + (wr - lo) } else { wr - rd }
}

impl Bcm {
    pub fn new(base: u32) -> Self {
        Bcm {
            base,
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
            self.panel[i] = self.mem.get(&(BCMA_CMDPARAM + i as u32 * 2)).copied().unwrap_or(0);
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
        let (w, h) = (x1.wrapping_sub(x0).wrapping_add(1), y1.wrapping_sub(y0).wrapping_add(1));
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
        self.timeline.push(BcmOp::Blit { x0, y0, x1, y1, src });
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

    fn write16(&mut self, off: u32, val: u16, _high: bool) {
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
            0x3_0000 | 0x7_0000 => {
                if val == 0x31 {
                    self.kick();
                }
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
    fn publish_registry(&mut self) {
        self.set32(BCMA_COMMAND, 1); // "firmware up" — FUN_00288058 requires exactly 1
        self.set32(BCMA_STATUS, REG_BASE); // the directory pointer; non-zero, 4-aligned
        for i in 0..8u32 {
            self.mem.insert(REG_BASE + i * 2, if i == 0 { REG_REC2 as u16 } else { 0 });
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
            self.mem.insert(REG_BASE + p, pair[0] as u16 | ((pair[1] as u16) << 8));
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
        [self.wr_addr, self.rd_addr, self.wr_phase_high as u32, self.rd_phase_high as u32]
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

// ---------------------------------------------------------------- snapshot

/// Save and restore a whole running machine.
///
/// The point is iteration cost. Every experiment re-runs the boot from reset, and reaching the
/// interesting part of a RetailOS or bootloader run costs a billion-plus instructions — two minutes,
/// paid again for every question. Snapshotting once and resuming turns that into seconds, which is
/// what makes an oracle-style sweep (vary one input, classify the outcome) practical at all.
///
/// **What is saved:** CPU including banked registers, every memory region, the alias and
/// read-override tables, the microsecond clock **and the sleep accumulator behind it**, interrupt
/// state, timer deadlines, and the ATA and BCM device models.
///
/// # The clock is two numbers, and saving one of them was a 44-minute jump backwards
///
/// `usec` is not stored state — it is *recomputed* every instruction as
/// `executed / instr_per_usec + slept_usec`. Version 3 of this format saved `usec` and not
/// `slept_usec`, so a restored machine kept the right clock for exactly zero instructions: the first
/// one recomputed it against a `slept_usec` of 0 and the clock fell back to whatever the instruction
/// count alone implied. On the standard idle snapshot that was **2 940 704 453 µs → 321 777 002 µs,
/// a backwards jump of 44 minutes of simulated time**, and firmware that measures an interval as
/// `now - start` in unsigned microseconds does not see a negative number: it sees that pair wrap to
/// **+1 676 039 845 µs, twenty-eight minutes of elapsed time**, arriving in one instruction. Every
/// timeout in RetailOS is therefore expired at the moment of restore. See research/10 Addendum 31.
///
/// Saving `slept_usec` makes the recomputation reproduce the saved `usec` exactly, because the
/// identity above is the same one the run loop maintains. The magic is `IPODSNP4`: a version-3
/// snapshot is **refused**, not read with a zero in the new field, because reading it would restore
/// precisely the machine this paragraph exists to stop existing.
///
/// **What is deliberately not saved**, because a restored run should not inherit a half-written
/// measurement: the profile, call log, unmapped map, region counters and every device log. A
/// restored run measures itself. The ATA *backing file* is not saved either — it is reopened by
/// path, so a snapshot is only valid against the same disk image.
///
/// **The click wheel is not saved**, and that has a consequence worth stating rather than
/// discovering: a restored run replays its injected script from the first step, against a machine
/// that is already past the instruction counts those steps are anchored to — so every step fires at
/// once, on the first tick. `--restore` and `--wheel` are not usefully combined until the script is
/// re-anchored; the injection is meant for a run from reset.
impl Machine {
    pub fn snapshot(&self) -> Vec<u8> {
        let mut o = Vec::new();
        let w32 = |o: &mut Vec<u8>, v: u32| o.extend_from_slice(&v.to_le_bytes());
        let w64 = |o: &mut Vec<u8>, v: u64| o.extend_from_slice(&v.to_le_bytes());

        o.extend_from_slice(b"IPODSNP6");
        let cpu = self.cpu.save();
        w32(&mut o, cpu.len() as u32);
        for x in &cpu {
            w32(&mut o, *x);
        }
        w64(&mut o, self.executed as u64);
        w64(&mut o, self.instr_per_usec as u64);
        w32(&mut o, self.timer_next[0]);
        w32(&mut o, self.timer_next[1]);
        w32(&mut o, self.mem.usec);
        // The other half of the clock. `usec` above is a derived value and is written for the
        // reader's benefit; this is the one the machine cannot recompute.
        w32(&mut o, self.mem.slept_usec);
        w32(&mut o, self.mem.int_pending);
        w32(&mut o, self.mem.int_pending_hi);
        w32(&mut o, self.mem.ide_irq_due.unwrap_or(u32::MAX));
        w32(&mut o, self.mem.usec_timer.unwrap_or(u32::MAX));

        w32(&mut o, self.mem.regions.len() as u32);
        for r in &self.mem.regions {
            w32(&mut o, r.name.len() as u32);
            o.extend_from_slice(r.name.as_bytes());
            w32(&mut o, r.base);
            w32(&mut o, r.data.len() as u32);
            o.extend_from_slice(&r.data);
        }
        w32(&mut o, self.mem.aliases.len() as u32);
        for (b, s, t) in &self.mem.aliases {
            w32(&mut o, *b);
            w32(&mut o, *s);
            w32(&mut o, *t);
        }
        w32(&mut o, self.mem.read_overrides.len() as u32);
        for (a, v) in &self.mem.read_overrides {
            w32(&mut o, *a);
            w32(&mut o, *v);
        }
        // BCM internal memory — sparse, so store as pairs.
        match &self.mem.bcm {
            Some(b) => {
                w32(&mut o, 1);
                w32(&mut o, b.base);
                w32(&mut o, b.mem.len() as u32);
                for (a, v) in b.mem.iter() {
                    w32(&mut o, *a);
                    w32(&mut o, *v as u32);
                }
                for x in b.save_scalars() {
                    w32(&mut o, x);
                }
            }
            None => w32(&mut o, 0),
        }
        match &self.mem.ata {
            Some((base, d)) => {
                let st = d.save();
                w32(&mut o, 1);
                w32(&mut o, *base);
                w32(&mut o, st.len() as u32);
                for x in &st {
                    w32(&mut o, *x);
                }
            }
            None => w32(&mut o, 0),
        }
        // The click wheel. Omitting it was not free: `reporting` starts off, the firmware turns it
        // on once with opcode 0x052a early in the boot, and a restored machine came back with it
        // off -- so every autonomous frame was suppressed and the wheel was dead in every session
        // that resumed rather than cold-booted. Scrolling produced a byte-identical panel and no
        // error, which is the quietest way a machine can be broken.
        //
        // The counters and logs are deliberately NOT saved: they are instrumentation, and the run
        // loop already reports "this session" separately from the total. The script is not saved
        // either, for the reason in this block's own doc comment -- a replayed script fires every
        // step at once against a machine already past their anchors.
        match &self.mem.clickwheel {
            Some(w) => {
                w32(&mut o, 1);
                w32(&mut o, w.base);
                w32(&mut o, w.hold as u32);
                w32(&mut o, w.touched as u32);
                w32(&mut o, w.position as u32);
                w32(&mut o, w.buttons as u32);
                w32(&mut o, w.ctrl);
                w32(&mut o, w.status);
                w32(&mut o, w.tx);
                w32(&mut o, w.rx);
                w32(&mut o, w.reporting as u32);
                w32(&mut o, w.irq_enabled as u32);
                match w.reply {
                    Some((frame, due)) => {
                        w32(&mut o, 1);
                        w32(&mut o, frame);
                        w32(&mut o, due);
                    }
                    None => w32(&mut o, 0),
                }
            }
            None => w32(&mut o, 0),
        }
        // The backlight. Same reasoning as the wheel one line above: the dimmer's counter lives in
        // the panel's circuit and nothing reads it back, so a restored machine that does not carry
        // it comes back at the default while the firmware still believes it is wherever the user
        // left it. The two then disagree for the rest of the session, and the only symptom is a
        // screen at the wrong brightness.
        w32(&mut o, self.mem.backlight.level as u32);
        o
    }

    /// Restore a snapshot over this machine. Returns false on a bad or truncated image.
    ///
    /// Regions are replaced wholesale rather than merged: a partial restore would leave the machine
    /// in a state that never existed, which is worse than refusing.
    pub fn restore(&mut self, b: &[u8]) -> bool {
        if b.len() < 8 || &b[..8] != b"IPODSNP6" {
            return false;
        }
        let mut p = 8usize;
        let mut r32 = |p: &mut usize| -> u32 {
            let v = u32::from_le_bytes(b[*p..*p + 4].try_into().unwrap());
            *p += 4;
            v
        };
        let n = r32(&mut p) as usize;
        let cpu: Vec<u32> = (0..n).map(|_| r32(&mut p)).collect();
        if !self.cpu.load(&cpu) {
            return false;
        }
        let r64 = |p: &mut usize| -> u64 {
            let v = u64::from_le_bytes(b[*p..*p + 8].try_into().unwrap());
            *p += 8;
            v
        };
        self.executed = r64(&mut p) as usize;
        self.instr_per_usec = r64(&mut p) as usize;
        self.timer_next[0] = r32(&mut p);
        self.timer_next[1] = r32(&mut p);
        self.mem.usec = r32(&mut p);
        self.mem.slept_usec = r32(&mut p);
        self.mem.int_pending = r32(&mut p);
        self.mem.int_pending_hi = r32(&mut p);
        let due = r32(&mut p);
        self.mem.ide_irq_due = if due == u32::MAX { None } else { Some(due) };
        let ut = r32(&mut p);
        self.mem.usec_timer = if ut == u32::MAX { None } else { Some(ut) };

        let nregions = r32(&mut p) as usize;
        self.mem.regions.clear();
        for _ in 0..nregions {
            let nl = r32(&mut p) as usize;
            let name = String::from_utf8_lossy(&b[p..p + nl]).into_owned();
            p += nl;
            let base = r32(&mut p);
            let dl = r32(&mut p) as usize;
            let data = b[p..p + dl].to_vec();
            p += dl;
            // Region names are &'static str; a restored name is leaked deliberately — there are a
            // dozen of them per run and they must outlive the machine.
            self.mem.regions.push(Region { name: Box::leak(name.into_boxed_str()), base, data });
        }
        self.mem.aliases.clear();
        for _ in 0..r32(&mut p) as usize {
            let (x, y, z) = (r32(&mut p), r32(&mut p), r32(&mut p));
            self.mem.aliases.push((x, y, z));
        }
        self.mem.read_overrides.clear();
        for _ in 0..r32(&mut p) as usize {
            let (x, y) = (r32(&mut p), r32(&mut p));
            self.mem.read_overrides.push((x, y));
        }
        if r32(&mut p) == 1 {
            let base = r32(&mut p);
            let count = r32(&mut p) as usize;
            let mut bcm = Bcm::new(base);
            for _ in 0..count {
                let a = r32(&mut p);
                let v = r32(&mut p) as u16;
                bcm.mem.insert(a, v);
            }
            let sc = [r32(&mut p), r32(&mut p), r32(&mut p), r32(&mut p)];
            bcm.load_scalars(sc);
            self.mem.bcm = Some(bcm);
        }
        if r32(&mut p) == 1 {
            let _base = r32(&mut p);
            let n = r32(&mut p) as usize;
            let st: Vec<u32> = (0..n).map(|_| r32(&mut p)).collect();
            // The device itself is reattached by --disk; only its state comes from the snapshot.
            if let Some((_, d)) = &mut self.mem.ata {
                if !d.load(&st) {
                    return false;
                }
            }
        }
        if r32(&mut p) == 1 {
            let base = r32(&mut p);
            let (hold, touched) = (r32(&mut p) != 0, r32(&mut p) != 0);
            let (position, buttons) = (r32(&mut p) as u8, r32(&mut p) as u8);
            let (ctrl, status, tx, rx) = (r32(&mut p), r32(&mut p), r32(&mut p), r32(&mut p));
            let (reporting, irq_enabled) = (r32(&mut p) != 0, r32(&mut p) != 0);
            let reply = if r32(&mut p) == 1 { Some((r32(&mut p), r32(&mut p))) } else { None };
            if let Some(w) = &mut self.mem.clickwheel {
                w.base = base;
                w.hold = hold;
                w.touched = touched;
                w.position = position;
                w.buttons = buttons;
                w.ctrl = ctrl;
                w.status = status;
                w.tx = tx;
                w.rx = rx;
                w.reporting = reporting;
                w.irq_enabled = irq_enabled;
                w.reply = reply;
            }
        }
        // The dimmer, if this image carries one. Older v6 images stop at the wheel.
        let level = r32(&mut p);
        if (1..=32).contains(&level) {
            self.mem.backlight.level = level as u8;
        }
        true
    }
}

/// Recover function names from a RetailOS image.
///
/// RetailOS ships no symbol table, but it does ship the RTXC task registry: a run of records, each
/// a NUL-terminated name padded to a 4-byte boundary and followed immediately by a pointer to that
/// task's entry point. `DiskMgrTask`, `HoldSwitchTask`, `WatchdogTask` and the rest are all named
/// this way, and reading the table is the difference between a profile of bare addresses and one
/// that says which subsystem is running.
///
/// The pointer is only accepted when the word it points at looks like an ARM function prologue,
/// which is what keeps ordinary strings that happen to precede a pointer-shaped word out of the
/// result. `base` is the address the image is loaded at, so the returned keys match trace PCs.
///
/// **Pattern A is wrong for the six boot tasks, and knowingly left wrong.** They are declared in a
/// compiler literal pool that runs *pointer then name*, so each is reported one record late:
/// `0x00284ea0` comes out "APPLEBOOT" when it is `t_graphicsManager`, and `0x002844e0` comes out
/// "t_power" when it is `APPLEBOOT` — which sent a whole session at the wrong task. The true
/// mapping is read off the creation code at `0x000d3b60` and recorded in research/10 Addendum 7 §2;
/// every priority and stack size it gives matches the resulting TCB. Reversing the pattern is not
/// the fix: in the device registry at `0x0025d63c` the word before each name is the *previous*
/// entry's pointer, so a reversed scan renames `OptoTask` to `SerialOptoTask`. Distinguishing the
/// two layouts needs the creation code, not the table.
pub fn extract_symbols(image: &[u8], base: u32) -> BTreeMap<u32, String> {
    let n = image.len();
    let word = |o: usize| -> u32 {
        u32::from_le_bytes([image[o], image[o + 1], image[o + 2], image[o + 3]])
    };
    let prologue = |p: usize| -> bool {
        if p + 4 > n || p % 4 != 0 || p < 0x1000 {
            return false;
        }
        let w = word(p);
        // stmdb sp!, {...}  |  sub sp, sp, #imm  |  mov r12, sp  |  sub rd, rn, #imm
        w & 0x0fff_0000 == 0x092d_0000
            || w & 0x0fff_f000 == 0x024d_d000
            || w & 0x0fff_ffff == 0x01a0_c00d
            || w & 0x0ff0_0000 == 0x0240_0000
    };
    let mut out = BTreeMap::new();
    let mut i = 0usize;
    while i < n {
        // A candidate name: an ASCII run starting with a letter, ending at a NUL.
        if !(image[i] as char).is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && (0x20..0x7f).contains(&image[i]) {
            i += 1;
        }
        let len = i - start;
        if i >= n || image[i] != 0 || !(4..=40).contains(&len) {
            i += 1;
            continue;
        }
        let after = i + 1;
        let p = after + (4 - after % 4) % 4;
        let clean = std::str::from_utf8(&image[start..start + len])
            .ok()
            .filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ' '));
        if let Some(name) = clean {
            if p + 4 <= n {
                // Pattern A — the RTXC task registry: the name is followed by a POINTER to the
                // entry point.
                let target = word(p) as usize;
                if prologue(target) {
                    out.insert(base.wrapping_add(target as u32), name.to_string());
                }
                // Pattern B — an inline label: the function's own code follows the name directly,
                // so the entry point IS the aligned address after the string. This is how
                // `VCUpdateTask`, `DiskReaderTask` and `eAppMotor` are named, and it recovers
                // several times as many functions as pattern A.
                if prologue(p) {
                    out.insert(base.wrapping_add(p as u32), name.to_string());
                }
            }
        }
        i += 1;
    }
    out
}

impl Machine {
    /// Allocate the seen-bitset that `--novelty` needs. Separate from setting the map so the cost
    /// is only paid when the measurement is asked for.
    pub fn arm_novelty(&mut self) {
        self.seen_bits = vec![0u64; (1 << 22) / 64];
    }

    /// The name of the function containing `addr`, if one is known, as `name+offset`.
    ///
    /// Nearest-at-or-below rather than exact-match: a profile samples PCs inside a function, not
    /// its entry, so exact lookup would name almost nothing. Capped at 4 KB so an address in an
    /// unnamed region is reported as unknown rather than attributed to a distant neighbour.
    pub fn symbolise(&self, addr: u32) -> Option<String> {
        let (&a, n) = self.symbols.range(..=addr).next_back()?;
        let off = addr - a;
        if off > 0x1000 {
            return None;
        }
        Some(if off == 0 { n.clone() } else { format!("{n}+{off:#x}") })
    }
}

/// Map the memory RetailOS code expects: its BSS, SDRAM, and the PP5021C peripheral windows.
///
/// Shared by `--native` and `--boot-osos` — firmware functions reach for the same globals whether
/// they were entered from the reset vector or called directly from a game.
///
/// Mapped generously on purpose: the trace reports what was actually touched, which is better
/// evidence than a guess at which windows matter. Deliberately **not** mapping `0xf0000000`,
/// which is `TRAP_BASE` — a region there would shadow unbound imports and turn a missing binding
/// into silently-executed zeros.
///
/// **Lives here rather than in `trace.rs` because there is now a second front end.** `tools/ipod-gui`
/// stands the same machine up in a window, and a machine whose peripheral map is a *copy* of this
/// one would be a different machine the first time either copy was corrected — with nothing in
/// either front end's output saying which of the two it was running. The move was byte-for-byte;
/// `trace.rs` keeps a one-line delegate so its call sites read unchanged.
///
/// Note also what it does **not** do: it never removes or reorders a region that is already there.
/// `Machine::new` has already installed the game's image, stack and heap, and in a cold boot the
/// 8 MB scratch stack at `0x11000000` therefore sits *in front of* the SDRAM region installed
/// below. That shadowing is load-bearing history rather than an oversight — every measurement in
/// `research/` was taken through it — so a caller that wants the boot this project has measured
/// must build its machine with the same `ram_base`/`ram_size` and call this at the same point.
pub fn map_hardware(m: &mut Machine, cold_boot: bool) {
    for (name, base, size) in [
        // SDRAM, as the hardware actually has it: one contiguous 64 MB at 0x10000000, plus the
        // remapped view at 0. Rockbox's `crt0-pp.S` settles both the size and the layout — it
        // detects 32-vs-64 MB by writing to 0x12000000-1 and 0x14000000-1 and seeing whether the
        // two alias, which is only meaningful if SDRAM runs 0x10000000..0x14000000 unbroken.
        //
        // These replace a patchwork of three part-regions with gaps between them. The firmware's
        // own heap pointer is around 0x00a0ffa0, past the end of what the old low mirror covered,
        // so anything that got that far was writing into unmapped space.
        //
        // There is exactly ONE storage region, at 0. The native window at 0x10000000 and the
        // uncached window at 0x14000000 are *aliases* of it, registered below — not regions. They
        // were regions once, and that was a real bug: RetailOS's statics are written by the
        // scatter-load through the low view and read back through the high one (`0x1081de40` at
        // `0x265164`, for instance), so two buffers meant the initialised value never arrived.
        // Cold boot puts NOR at 0, so SDRAM's storage cannot also live there — `locate` is
        // first-match and NOR wins, which made every read of 0x10000000 return flash while the
        // writes went elsewhere. `osos` checksummed the NOR reset vectors and failed. Bypass #11
        // in the ledger predicted exactly this. So in cold boot SDRAM is a region where the
        // hardware actually has it, and address 0 belongs to NOR until the MMAP remap is honoured.
        (if cold_boot { "sdram" } else { "sdram-low" }, if cold_boot { 0x1000_0000 } else { 0x0000_0000u32 }, 0x0400_0000usize),
        ("iram", 0x4000_0000, 0x0002_0000),
        ("mmio-7", 0x7000_0000, 0x0010_0000),
        ("mmio-6", 0x6000_0000, 0x0010_0000),
        ("mmio-c", 0xc000_0000, 0x0010_0000),
        // The ATA controller. Rockbox `pp5020.h`: `IDE_BASE 0xc3000000`. This sat *outside* the
        // `mmio-c` region above, which covers only 0xc0000000..0xc00fffff — so every disk access
        // the firmware made landed in unmapped space and silently read back zero.
        ("ide", 0xc300_0000, 0x0001_0000),
        // The cache controller aperture. Rockbox `pp5020.h`: the data and status arrays live at
        // 0xf0000000..0xf0005fff, and the control registers just past them — CACHE_MASK 0xf000f040,
        // CACHE_OPERATION 0xf000f044, CACHE_FLUSH_MASK 0xf000f048.
        ("cache", 0xf000_0000, 0x0001_0000),
    ] {
        if m.mem.regions.iter().any(|r| r.name == name) {
            continue;
        }
        m.mem.regions.push(Region { name, base, data: vec![0; size] });
    }
    // The LCD controller, at 0x30020000/0x30030000/0x30060000/0x30070000. Filled with 0xFF
    // rather than zeros because the driver spins on a ready bit:
    //
    //     ldrh r0, [r6]      ; status
    //     tst  r0, #1        ; FIFO ready?
    //     beq  <back>        ; ...spin
    //
    // Zeroed MMIO means that bit is never set and every draw call hangs forever — which is
    // exactly what `glClearColor` did. An emulated panel is always ready, so reads report ready.
    if !m.mem.regions.iter().any(|r| r.name == "lcd") {
        m.mem.regions.push(Region {
            name: "lcd",
            base: 0x3000_0000,
            data: vec![0xff; 0x0008_0000],
        });
    }
    // GPIO input ports: the values silicon supplies, which nothing in firmware ever writes.
    //
    // Left at zero — the region default — this emulator was not saying "nothing is connected", it
    // was asserting every active-low line at once: hold engaged, charger present, mid-charge. The
    // per-bit meanings below are read off Rockbox's source for THIS target, so they are sourced
    // rather than guessed:
    //
    //   button-clickwheel.c  GPIOA 0x20 hold switch, active low   `(GPIOA_INPUT_VAL & 0x20) ? false : true`
    //                        GPIOA 0x80 headphones, active high
    //   power-ipod.c         GPIOB 0x01 charging, active low      `(GPIOB_INPUT_VAL & 0x01) ? false : true`
    //                        GPIOL 0x08 main/FireWire charger, active low   (IPOD_VIDEO)
    //                        GPIOL 0x10 USB charger, active high            (IPOD_VIDEO)
    //
    // The state modelled is a bare iPod: nothing plugged in, hold off, not charging.
    for (addr, val) in [
        (0x6000_d030u32, 0x0000_0020u32), // GPIOA: hold OFF, no headphones
        (0x6000_d034, 0x0000_0001),       // GPIOB: not charging
        // GPIOL: bit 3 SET — no main/FireWire charger; bit 4 clear — no USB charger.
        //
        // This was deliberately left at zero ("charger present") until 2026-08-14, because with
        // no charger Apple's bootloader checks the battery and our PMU could not answer that
        // check: the ADC reported a completed conversion whose value was always 0, so the cell
        // read flat and the bootloader halted at `0x400015b4` without touching the disk. That was
        // a defect in this model's converter, not a missing threshold — research/10 Addendum 30.
        // With it fixed the honest value boots, and it is load-bearing all the way to the screen:
        // RetailOS polls this exact bit (pin `0x63` through `FUN_00282b70`, 130 times a boot) and
        // a zero here made it draw the "Charged" screen instead of its menu.
        (0x6000_d13c, 0x0000_0008),
    ] {
        m.mem.write32(addr, val);
    }
    // The PP5021 is dual-core (CPU + COP), and the firmware's first act is to ask the
    // silicon which of the two it is running on. From Rockbox `firmware/target/arm/pp/
    // crt0-pp.S` — the same hardware, documented:
    //
    //     ldr    r0, =PROC_ID    ; 0x60000000
    //     ldrb   r0, [r0]
    //     cmp    r0, #0x55       ; 0x55 = CPU, anything else = COP
    //     ldrne  r2, =COP_CTRL   ; not the CPU -> put self to sleep
    //     movne  r1, #SLEEP
    //  1: ldreq  r1, [r2]        ; the CPU, meanwhile, spins on COP_STATUS
    //     tsteq  r1, #COPSLEEPING
    //     beq    1b
    //
    // Zeroed MMIO makes *both* branches dead ends. The read returns 0, so firmware decides
    // it is the coprocessor and sleeps within three instructions; and had it decided
    // otherwise, it would spin forever waiting for a COP that never reports sleeping. Every
    // boot before this was a coprocessor boot. Report CPU, and report the COP already asleep.
    m.mem.write32(0x6000_0000, 0x0000_0055); // PROC_ID (read as a byte; 0x55 = CPU)
    // COP_STATUS must *stay* COPSLEEPING. `COP_CTRL` is the same address, and firmware wakes the
    // coprocessor by writing WAKE (0) to it before waiting for the sleep bit to come back — so a
    // plain seeded value gets cleared by the very code that then waits for it. We do not emulate
    // the second core, so on this machine the COP is always asleep.
    if m.mem.read_overrides.is_empty() {
        // Ledger #7, and until now unconditional — which meant nothing that depends on the second
        // core could be A/B'd, because there was no arm B. RetailOS reads this address 5 470 times
        // and runs Rockbox's `wake_cop` 10 198 times; being able to answer differently is the first
        // step to knowing whether any of that matters.
        if !m.mem.cop_awake {
            m.mem.read_overrides.push((0x6000_7004, 0x8000_0000));
        } else {
            // Said out loud. A switch whose effect cannot be observed is indistinguishable from a
            // switch that is not wired up, and this session has produced enough of those.
            eprintln!("ledger #7: COP_STATUS override NOT installed (--cop-awake)");
        }
    }
    // Ledger #8, retired as a whole-word override. Firmware programs PLL_CONTROL at 0x60006034 and
    // spins until bit 31 of PLL_STATUS says the PLL has locked; an emulated PLL locks instantly, so
    // asserting that bit is honest. Forcing the other 31 bits to zero was not, and was never the
    // claim being made -- so it is an OR-mask now, and the rest of the register reads as it is.
    if m.mem.read_or_masks.is_empty() {
        m.mem.read_or_masks.push((0x6000_603c, 0x8000_0000));
    }

    // The uncached view of SDRAM. The firmware computes this address itself, at 0xdb4:
    //
    //     ldr r2, [r4, #0x24]
    //     bic r0, r2, #0xfc000000     ; keep the low 26 bits — a 64 MB space
    //     orr r0, r0, #0x14000000     ; ...and OR in the uncached base
    //
    // so the rule is `0x14000000 | (addr & 0x03FFFFFF)`, over the remapped low view. An alias, not
    // a region: give it its own storage and a value written through one view is invisible in the
    // other.
    // Rockbox `pp5020.h`: `USEC_TIMER (*(volatile unsigned long *)(0x60005010))` — free-running.
    m.mem.usec_timer = Some(0x6000_5010);
    // A PP timer interrupt is acknowledged at the timer, by reading its VAL register — there is no
    // The external memory bus controller. Not behind a flag, because unlike the PMU or the video
    // co-processor there is nothing optional about it: the two bits it owns are the ones Apple's
    // bootloader stops on, and the alternative was a pair of `--rdval` guesses in the recipe.
    // Reset state is "ready, NOR write gate closed", which is also what the ROM assumes — its
    // first act on the gate is to wait for ready and then open it.
    if m.mem.xmb.is_none() {
        m.mem.xmb = Some(Xmb::new(0x7000_0000));
        m.mem.write8(0x7000_0033, Xmb::ctrl_hi_at_reset());
    }

    // central acknowledge in the interrupt controller.
    if m.mem.int_ack_on_read.is_empty() {
        m.mem.int_ack_on_read.push((0x6000_5004, 1 << 0)); // TIMER1_VAL -> TIMER1_IRQ
        m.mem.int_ack_on_read.push((0x6000_500c, 1 << 1)); // TIMER2_VAL -> TIMER2_IRQ
    }

    if m.mem.aliases.is_empty() {
        if cold_boot {
            // Storage is at 0x10000000; only the uncached window aliases onto it. Address 0 is
            // NOR, so it must NOT be aliased here — `translate` runs before the region lookup, and
            // aliasing 0 would make the reset vectors unreachable.
            m.mem.aliases.push((0x1400_0000, 0x0400_0000, 0x1000_0000));
            // A third window onto the same 64 MB, at 0x90000000. RetailOS's ATA driver points the
            // DMA engine at 0x93eea730 and then reads the transfer back from that same address
            // (2048 word reads from 0x000000fc); masking the top bit off made the bytes land at
            // 0x13eea730 while the read-back still went to 0x93eea730 and answered zero. Same
            // "OR a window base over the low 26 bits" shape as the uncached view above, so it is
            // registered the same way rather than special-cased in the DMA engine.
            m.mem.aliases.push((0x9000_0000, 0x0400_0000, 0x1000_0000));
            // Everything above is unconditional; MMAP windows are appended past this point and
            // rebuilt each time the firmware programs one.
            m.mem.mmap_alias_floor = m.mem.aliases.len();
            m.mem.mmap_base = Some(0xf000_f000);
        } else {
            m.mem.aliases.push((0x1400_0000, 0x0400_0000, 0x0000_0000));
            // The native SDRAM window. Same 64 MB, seen where the hardware puts it before the remap.
            m.mem.aliases.push((0x1000_0000, 0x0400_0000, 0x0000_0000));
        }
    }
}

#[cfg(test)]
mod nor_tests {
    use super::*;

    /// A 64 KiB stand-in for the chip's bytes, plus the device driving them. The bus wiring is
    /// exercised by the boot runs; what needs a test is the protocol decode and the cell semantics.
    struct Chip {
        nor: Nor,
        data: Vec<u8>,
    }

    impl Chip {
        fn new() -> Self {
            Chip {
                nor: Nor::sst39wf800a(vec![(0, 0x1_0000)], vec!["flash-low"]),
                data: vec![0x5a; 0x1_0000],
            }
        }

        /// One 16-bit bus cycle as it actually reaches the device: `Bus::write16`'s default splits
        /// a `strh` into two byte stores, low half first.
        fn cycle(&mut self, addr: u32, val: u16) {
            let b = val.to_le_bytes();
            for (i, v) in b.iter().enumerate() {
                if let Some(op) = self.nor.write(addr + i as u32, *v) {
                    op.apply(&mut self.data);
                }
            }
        }

        fn unlock(&mut self, cmd: u16) {
            self.cycle(0xaaaa, 0xaaaa);
            self.cycle(0x5554, 0x5555);
            self.cycle(0xaaaa, cmd);
        }

        fn read16(&self, off: u32) -> u16 {
            let byte = |o: u32| {
                self.nor.read(o).unwrap_or_else(|| self.data[o as usize])
            };
            u16::from_le_bytes([byte(off), byte(off + 1)])
        }
    }

    #[test]
    fn autoselect_answers_the_jedec_pair_and_reset_restores_the_array() {
        let mut c = Chip::new();
        assert_eq!(c.read16(0), 0x5a5a, "read-array must answer the image before any command");
        c.unlock(0x9090);
        assert_eq!(c.read16(0), 0x00bf, "manufacturer");
        assert_eq!(c.read16(2), 0x273f, "device");
        assert_eq!(c.read16(4), 0x0000, "sector protect: the driver refuses to erase otherwise");
        c.cycle(0, 0xf0f0);
        assert_eq!(c.read16(0), 0x5a5a, "reset must put the chip back into read-array");
    }

    #[test]
    fn sector_erase_clears_its_own_sector_and_nothing_else() {
        let mut c = Chip::new();
        c.unlock(0x8080);
        c.cycle(0xaaaa, 0xaaaa);
        c.cycle(0x5554, 0x5555);
        c.cycle(0x1800, 0x3030);
        assert_eq!(c.data[0x1000], 0xff, "the addressed 4 KiB sector is erased");
        assert_eq!(c.data[0x1fff], 0xff);
        assert_eq!(c.data[0x0fff], 0x5a, "the sector below is untouched");
        assert_eq!(c.data[0x2000], 0x5a, "the sector above is untouched");
        assert_eq!(c.nor.erases, 1);
    }

    /// The regression a full reflash exposed: a program's data cycle is data. Decoding it as a
    /// command swallowed every payload word whose low byte was `0xff` — 281 612 of 507 904, which
    /// the report showed as a reset count larger than the whole rest of the transfer.
    #[test]
    fn programming_clears_bits_only_and_all_ones_data_is_not_a_reset() {
        let mut c = Chip::new();
        c.unlock(0x8080);
        c.cycle(0xaaaa, 0xaaaa);
        c.cycle(0x5554, 0x5555);
        c.cycle(0x1800, 0x3030);

        c.unlock(0xa0a0);
        c.cycle(0x1000, 0xffff);
        c.unlock(0xa0a0);
        c.cycle(0x1002, 0x1234);
        assert_eq!(c.read16(0x1000), 0xffff, "0xffff data must be programmed, not read as a reset");
        assert_eq!(c.read16(0x1002), 0x1234);

        c.unlock(0xa0a0);
        c.cycle(0x1002, 0xffff);
        assert_eq!(c.read16(0x1002), 0x1234, "a program can only clear bits, never set them");

        assert_eq!(c.nor.programs, 3);
        assert!(c.nor.unknown.is_empty(), "undecoded cycles: {:?}", c.nor.unknown);
    }
}

#[cfg(test)]
mod bcm_command_tests {
    use super::*;

    /// Drive the co-processor the way the host does: latch a write address as two halves of one
    /// 32-bit store, then push halfwords at the data port.
    struct Host {
        bcm: Bcm,
    }

    impl Host {
        fn new() -> Self {
            Host { bcm: Bcm::new(0x3000_0000) }
        }
        fn addr(&mut self, a: u32) {
            self.bcm.write16(0x1_0000, a as u16, false);
            self.bcm.write16(0x1_0000, (a >> 16) as u16, true);
        }
        fn data(&mut self, v: u16) {
            self.bcm.write16(0x0_0000, v, false);
        }
        fn word(&mut self, v: u32) {
            self.data(v as u16);
            self.data((v >> 16) as u16);
        }
        /// `BCM_CMD(x) = ((~x << 16) | x)`, then `CONTROL = 0x31`.
        fn command(&mut self, cmd: u32) {
            self.addr(BCMA_COMMAND);
            self.word((!cmd << 16) | cmd);
            self.bcm.write16(0x3_0000, 0x31, false);
        }
        fn panel(&self, x: usize, y: usize) -> u16 {
            self.bcm.panel[y * PANEL_W + x]
        }
    }

    /// Stage the 8-word header plus a solid tile, exactly as Apple's bootloader does.
    fn stage_rect(h: &mut Host, x0: u32, y0: u32, x1: u32, y1: u32, len: u32, fill: u16) {
        h.addr(BCMA_CMDPARAM);
        for w in [0x34, x0, y0, x1, y1, 0, 0, len] {
            h.word(w);
        }
        for _ in 0..(x1 - x0 + 1) * (y1 - y0 + 1) {
            h.data(fill);
        }
    }

    /// The measured shape of a retail boot: a full-screen fill by rectangle, then the logo tile at
    /// (129,81)-(190,158). The point of the test is that the rectangle is *placed* — a model that
    /// stored the pixels and ignored the command would leave the tile at the panel origin, which is
    /// exactly what this one did until 2026-08-14.
    #[test]
    fn lcd_updaterect_places_the_rectangle_the_header_describes() {
        let mut h = Host::new();

        stage_rect(&mut h, 0, 0, 319, 239, 320 * 240 * 2, 0x0000);
        h.command(5);

        stage_rect(&mut h, 129, 81, 190, 158, 62 * 78 * 2, 0xffff);
        h.command(5);

        assert_eq!(h.panel(129, 81), 0xffff, "the rectangle's top-left corner");
        assert_eq!(h.panel(190, 158), 0xffff, "and its bottom-right, inclusive");
        assert_eq!(h.panel(128, 81), 0x0000, "one pixel left of it is untouched");
        assert_eq!(h.panel(191, 158), 0x0000, "and one pixel right of it");
        assert_eq!(h.panel(129, 80), 0x0000, "and the row above");
        assert_eq!(h.panel(0, 0), 0x0000, "and above all, NOT at the panel origin");

        let lit = h.bcm.panel.iter().filter(|&&p| p == 0xffff).count();
        assert_eq!(lit, 62 * 78, "exactly the rectangle, no more and no less");

        // The frame store is published back over the transfer buffer, so the header that described
        // the blit is no longer readable as a pixel.
        assert_eq!(h.bcm.mem.get(&BCMA_CMDPARAM).copied().unwrap_or(0), 0x0000);
        assert_eq!(h.bcm.frames, 2);
    }

    /// The control the placement test needs: a header this model will not honour must change
    /// nothing at all, and must say so. Without this arm, "the rectangle landed where the header
    /// said" is indistinguishable from "every rectangle lands there".
    #[test]
    fn a_rectangle_whose_length_disagrees_with_its_own_corners_is_refused() {
        let mut h = Host::new();
        stage_rect(&mut h, 0, 0, 319, 239, 320 * 240 * 2, 0x1234);
        h.command(5);
        assert_eq!(h.panel(10, 10), 0x1234);

        // Same corners, a length word that does not match them.
        stage_rect(&mut h, 129, 81, 190, 158, 62 * 78 * 2 - 4, 0xffff);
        h.command(5);
        assert_eq!(h.bcm.blits_rejected.seen(), 1, "the refusal is counted");
        assert_eq!(h.panel(129, 81), 0x1234, "and nothing moved");

        // And a rectangle that runs off the panel.
        stage_rect(&mut h, 300, 200, 400, 300, 101 * 101 * 2, 0xffff);
        h.command(5);
        assert_eq!(h.bcm.blits_rejected.seen(), 2);
        assert_eq!(h.panel(300, 200), 0x1234);
    }

    /// `LCD_UPDATE` is the other arm of the command interface — Rockbox's, not Apple's — and it
    /// reads the buffer as a bare frame with no header. Nothing in this project sends it; the test
    /// is what stops that arm rotting unnoticed.
    #[test]
    fn lcd_update_takes_the_whole_staged_frame_with_no_header() {
        let mut h = Host::new();
        h.addr(BCMA_CMDPARAM);
        for i in 0..PANEL_W * PANEL_H {
            h.data(i as u16);
        }
        h.command(0);
        assert_eq!(h.panel(0, 0), 0);
        assert_eq!(h.panel(1, 0), 1);
        assert_eq!(h.panel(0, 1), 320);
        assert_eq!(h.bcm.frames, 1);
        assert_eq!(h.bcm.blits_rejected.seen(), 0);
    }

    /// A command that is not an image operation must move no pixels. A retail boot sends two of
    /// them — `0x13` and `0xa` — before either of the rectangles, and if they composited anything
    /// the logo would be drawn over.
    #[test]
    fn commands_that_are_not_image_operations_leave_the_panel_alone() {
        let mut h = Host::new();
        stage_rect(&mut h, 0, 0, 319, 239, 320 * 240 * 2, 0x4321);
        h.command(5);

        h.addr(BCMA_CMDPARAM);
        for _ in 0..64 {
            h.data(0xdead);
        }
        h.command(0x13);
        h.command(0xa);
        assert_eq!(h.panel(0, 0), 0x4321, "the frame store is unchanged");
        assert_eq!(h.bcm.frames, 1, "and neither counted as a frame update");
        assert_eq!(h.bcm.commands, vec![5, 0x13, 0xa]);
    }
}

#[cfg(test)]
mod peek_tests {
    use super::*;

    fn regions(base: u32, bytes: &[u8]) -> Vec<Region> {
        vec![Region { name: "sdram", base, data: bytes.to_vec() }]
    }

    #[test]
    fn a_word_is_read_from_its_region() {
        let r = regions(0x1000_0000, &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);
        assert_eq!(peek_regions(&r, 0x1000_0000), Some(0x4433_2211));
        assert_eq!(peek_regions(&r, 0x1000_0004), Some(0x8877_6655));
    }

    /// The firmware writes through the cached view and reads through the uncached alias, so both
    /// spellings of the same word must resolve. Getting this wrong reports the DRM context address
    /// as unmapped, which reads like a broken tool rather than a wrong answer.
    #[test]
    fn both_sdram_aliases_resolve_to_the_same_word() {
        let mut data = vec![0u8; 0x100];
        data[0x40..0x44].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        let r = regions(0x1000_0000, &data);
        assert_eq!(peek_regions(&r, 0x1000_0040), Some(0xDEAD_BEEF), "cached view");
        assert_eq!(peek_regions(&r, 0x1400_0040), Some(0xDEAD_BEEF), "uncached alias");
    }

    #[test]
    fn an_unaligned_address_reads_its_containing_word() {
        let r = regions(0x1000_0000, &[0x11, 0x22, 0x33, 0x44]);
        for a in 0..4 {
            assert_eq!(peek_regions(&r, 0x1000_0000 + a), Some(0x4433_2211));
        }
    }

    /// **`None`, not zero.** Zero is a meaningful value at `0x14937194` -- a null DRM context is
    /// exactly what the research recorded -- so "I cannot see this" must never be reported as
    /// "this contains zero".
    #[test]
    fn an_address_outside_every_region_is_none() {
        let r = regions(0x1000_0000, &[0u8; 16]);
        assert_eq!(peek_regions(&r, 0x7000_0000), None, "an MMIO window is not backing store");
        assert_eq!(peek_regions(&r, 0x1000_0020), None, "past the end of the region");
    }

    #[test]
    fn a_word_that_would_run_past_the_end_is_none() {
        let r = regions(0x1000_0000, &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        assert_eq!(peek_regions(&r, 0x1000_0004), None, "only two bytes remain");
    }
}
