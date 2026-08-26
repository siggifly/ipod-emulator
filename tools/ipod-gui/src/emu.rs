//! The machine, and the thread that runs it.
//!
//! This is a **front end over the existing model**, not a second model. Every device here is built
//! with the same call the `trace` recipes make, in the same order, with the same arguments; the
//! peripheral map comes from [`eapp_loader::map_hardware`] rather than from a copy of it. If this
//! file and `tools/ipod-boot/retail-boot.sh` ever disagree about what the machine is, that is a bug
//! in this file, and `--headless` exists so the disagreement is a number rather than an impression.
//!
//! # How input reaches the wheel, and why it is scheduled rather than poked
//!
//! [`eapp_loader::ClickWheel`] posts a frame only from its script — `service_clickwheel` walks
//! `script[next..]` and fires every step whose instruction count has arrived. Writing `w.position`
//! directly would move the device's state and report nothing, which is precisely the failure this
//! project keeps calling "an instrument that lies". So the GUI *appends steps*, at the current
//! instruction count, and the model's own path posts the frames.
//!
//! Appending also has to be **spaced**. A frame posted while the previous one is still unread is a
//! real overrun (`frames_dropped`), and that is what a whole rotation delivered in one tick would
//! be: research/10 Addendum 21's arm D posted 39 frames and had 35 of them overwritten unread. So
//! events drain one per `click_gap` instructions — default 300 000, the same figure
//! `--wheel-click-instr` uses, which at the default `--clock=75` is 4 ms per click. Both numbers
//! moved together when the clock went from the research accelerant to the real part; what the
//! firmware's wheel poll sees is the *simulated* interval, and that is unchanged.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eapp_loader::{
    Ata, Bcm, ClickWheel, EApp, Machine, Nor, Pcf50605, Region, Stop, WheelEvent, WheelStep,
};

/// The scratch stack and its size, verbatim from `trace.rs`. Not a free choice: in a cold boot the
/// 8 MB at `0x11000000` shadows the front of SDRAM, and every measurement in `research/` was taken
/// through that shadow. A GUI that "fixed" it would be emulating a different iPod.
const RAM_BASE: u32 = 0x1100_0000;
const RAM_SIZE: usize = 0x0080_0000;

/// The 5G panel. 320x240 RGB565, and the co-processor surface Apple's bootloader fills is
/// `0xe0000..0x10581e` — exactly one frame of it.
pub const FB_W: usize = 320;
pub const FB_H: usize = 240;
/// Front buffer: `BCMA_CMDPARAM`, where `Bcm::REG_SURFACE_BASE` allocates the first surface.
pub const FB_FRONT: u32 = 0x000e_0000;
/// The second surface the same allocator hands out — one frame further on.
pub const FB_BACK: u32 = 0x0010_6000;

/// The instruction count `--snap-at` defaulted to, and what `Config::snap_at` still means when
/// nothing writes a snapshot: **the fallback that ends `Phase::Booting`.**
///
/// The boot phase normally ends when the machine goes quiet, which is an observation — see
/// [`Quiet`]. This is the answer for the case that never happens: firmware that spins for ever, a
/// drive that never answers, an operating system with no idle loop. It matters that it is not
/// zero: `Booting { target: 0 }` ends the boot phase on the first slice, so a window built on
/// `Config::default()` would say *running* over a machine that had executed 250 000 instructions,
/// which is the shape of instrument this project keeps deleting.
pub const SNAP_AT: u64 = 1_600_000_000;

/// **How much of a window the core has to spend halted before the machine counts as quiet**, in
/// hundredths, and how wide that window is in [`Machine::steps`] — instructions executed plus
/// cycles spent halted.
///
/// Both are readings off one boot rather than tunings, and the run that produced them is
/// `the_bench_boots_apples_software_and_this_needs_resources` on Apple's own NOR and a real 5.5G's
/// drive. Over an 8 M-step trailing window, the **highest** halted fraction anywhere in RetailOS's
/// 871 M-instruction cold boot is **61.7 %** (at 164.7 M, waiting on the drive); the first window
/// that crosses 95 % is at **823.6 M**, and from there the machine holds **99.7 %** for the rest of
/// its life. Thirty-three points of clear air, so every threshold from 70 to 95 answers the same,
/// and the width is what buys it: at 2 M the boot's own worst window is 91.7 % and there is no gap
/// left to read.
pub const QUIET_WINDOW_STEPS: u64 = 8_000_000;
pub const QUIET_HALTED_PERCENT: u64 = 95;

/// Apple's ISR frame decoder. Entering it is the evidence that a frame this GUI caused was read
/// and parsed by RetailOS rather than merely posted into a register; see research/10 Addendum 21 §6.
pub const PC_DECODER: u32 = 0x0028_1350;
/// The button-edge dispatcher `SerialOptoTask` calls on every wake.
pub const PC_EDGE: u32 = 0x000c_953c;
/// The scroll accumulator — wraps at 0x60, folds the delta into `[state+0x10]`.
pub const PC_SCROLL: u32 = 0x000d_d018;
/// Post a button event / post a wheel event, the two ways input enters RetailOS's event system.
pub const PC_BUTTON_EVENT: u32 = 0x000a_da4c;
pub const PC_WHEEL_EVENT: u32 = 0x000c_d6a0;

/// The addresses `--enterlog` is armed on, in the order the UI shows them.
pub const WATCHED: [(u32, &str); 5] = [
    (PC_DECODER, "decoder 0x00281350"),
    (PC_EDGE, "edge 0x000c953c"),
    (PC_SCROLL, "scroll 0x000dd018"),
    (PC_BUTTON_EVENT, "button event 0x000ada4c"),
    (PC_WHEEL_EVENT, "wheel event 0x000cd6a0"),
];

/// `Clone` because the setup screen builds the machine *after* the window is up: the config is
/// edited in the UI thread and a copy is moved into the emulator thread, rather than the whole
/// program deciding its paths before `main` knows whether the files exist.
///
/// `Default` exists for tests that exercise one decision and should not have to spell out the
/// other eighteen fields to do it.
#[derive(Clone, Default)]
pub struct Config {
    // **`flash` was here and is deleted.** It held *the path a supplied dump came from*, which is
    // the same path `Source::File` already carries — so it was one fact in two fields, and nothing
    // in this file ever read the second. Setting it was the shape of defect a module blanket hides:
    // a value computed on every launch and dropped.
    /// Where the boot ROM comes from. A synthesised one is built here, in memory, from a recipe:
    /// there is no file, nothing to cache and nothing to go stale.
    pub nor: eapp_loader::nor::Source,
    /// The pristine image. Never written — a per-run clone is.
    pub disk: PathBuf,
    /// The writable clone the machine actually runs against. Re-made every launch.
    pub workdisk: PathBuf,
    /// The drive as it stood at the instant the snapshot was taken, and the reason a restore is
    /// coherent.
    ///
    /// A snapshot is RAM and CPU state; the drive is not in it. Pairing restored RAM with a drive
    /// that has kept moving is what produced the intermittent "connect to computer" screen: the
    /// working disk was created once and then reused for ever (`clone_disk` returned early if the
    /// destination existed, despite the comment calling it per-run), so a restore put RAM from the
    /// first boot against a volume RetailOS had rewritten a dozen times since. Its cached view of
    /// the FAT and the music database disagreed with the platter, it concluded the disk did not
    /// verify, and it said so the only way it can.
    ///
    /// Freezing the drive alongside the snapshot makes the pair coherent by construction.
    ///
    /// **What it costs, measured, not assumed.** `cp -c` on APFS is a copy-on-write clone: cloning
    /// the 8 GiB reference drive moved the volume's free space by 12 KB, against the 3.1 GB a real
    /// copy would have taken. btrfs and XFS reflink the same way through `--reflink=auto`. On ext4,
    /// which has no reflink, that flag falls back to a full copy and the cache holds one extra
    /// drive — bounded, because the prune keeps exactly one set, and worth it: the alternative is
    /// an emulator that intermittently boots to a screen nobody can explain.
    pub frozen: PathBuf,
    /// Interpreter instructions per simulated microsecond. 5 is what every recipe uses; 75 is real.
    pub clock: usize,
    /// Where the idle snapshot lives. Restored if present, written after the cold boot if not.
    pub snapshot: Option<PathBuf>,
    /// Instruction count the snapshot is taken at.
    pub snap_at: u64,
    /// Keep the user's drive image pristine by running on a copy of it.
    ///
    /// **Off by default: the emulator runs on the image it is given, and the iPod writes to it.**
    /// That is what the hardware does — settings, the language you picked, RetailOS's own
    /// bookkeeping — and it is why a real iPod remembers things. The copy existed to protect an
    /// image that took twelve iTunes sync rounds to build from an emulator bug writing to the wrong
    /// sector, which has happened here before; it is kept as an option for exactly that, and it
    /// costs 8 GB on any filesystem without reflinks, which is most of Linux and all of NTFS.
    ///
    /// It also decides what the snapshot pairs with — see [`Config::pair_is_whole`].
    pub work_on_copy: bool,
    /// Ignore any existing snapshot and boot from the reset vector.
    pub cold: bool,
    pub click_gap: u64,
    /// Run N instructions with no window and print the fingerprint. The self-check that says this
    /// front end and `retail-boot.sh` are running the same machine.
    pub headless: Option<u64>,
    /// No window: push a scripted gesture through the *GUI's own* input path and print what
    /// reached RetailOS. See [`SelfTest`].
    pub selftest: bool,
    /// The matched control for the above: identical run, identical sampling, **no gesture**. The
    /// panel-changed line is worthless without it — "RetailOS redrew because time moved" and
    /// "RetailOS redrew because the wheel moved" are the same picture unless one arm holds the
    /// input at zero.
    pub selftest_control: bool,
    /// Where screenshots go — the self-test's panel samples, and the window's `S` key.
    pub shots: PathBuf,
    /// **Run the PP5021's second ARM core.**
    ///
    /// Off by default, and the default is a measurement decision rather than a doubt about the
    /// feature: every number in `research/` was taken on a single-core machine, and turning this on
    /// changes some of them — a retail cold boot goes from 102 ATA commands to 99, because the
    /// coprocessor is now doing part of the work. RetailOS genuinely uses it (it parks and
    /// dispatches to it twice during a boot, then runs 111 M instructions there), so with this on
    /// the machine is *more* faithful and *less* comparable to what is already written down.
    pub second_core: bool,
    /// What this machine boots. [`BootTarget::Os`] is the ordinary one.
    pub boot: BootTarget,
    /// `--press=BUTTON@SECONDS`, repeatable — press a button through the window's own input path,
    /// at a moment measured from when the window opened.
    #[allow(dead_code)]  // retired when: `args::FLAGS` accepts `--press=` again — it refuses it as `Gone::Window` (it drove the window that was replaced), and nothing else in this program schedules a press
    ///
    /// It exists to make the window's input testable from a command line: a screenshot of Apple's
    /// diagnostics with its menu open is otherwise something only a person with a mouse can take,
    /// and "the wheel does not reach diagnostics" is otherwise something only a person with a
    /// mouse can discover.
    pub presses: Vec<(String, f32)>,
    #[allow(dead_code)]  // retired when: `args::FLAGS` accepts `--window-shot=` again; the window has no other route to a picture of itself
    /// `--window-shot=FILE` — write a PNG of **the whole window** and quit.
    ///
    /// Not the same picture as the `S` key, which captures the 320x240 panel. The two shipped
    /// window pictures in `docs/media/` were taken with the operating system's screen grabber, so
    /// they could not be regenerated without a person at the machine, and they went stale the
    /// moment the window's colours changed. This makes them a command.
    pub window_shot: Option<PathBuf>,
    /// Seconds to let the machine run before the shot is taken. The window is drawing from the
    /// first frame, but the iPod on it is not: a shot at zero is a picture of a black panel.
    #[allow(dead_code)]  // retired when: `window_shot` has a reader — this is the delay before it, and it has no second use
    pub shot_after: f32,
    /// No window: drive to the main menu at a fixed instruction anchor and watch the panel while
    /// the machine idles. See [`Probe`].
    pub probe: Option<Probe>,
    /// The instruction count the probe acts at. 1 500 000 000 is research/10 Addendum 30's own
    /// script anchor (`--wheel=@1500M:…`), so a cold arm here and that recipe press Select at the
    /// same point in the same boot; a restored machine is already past it and acts as soon as the
    /// wheel's reporting gate opens.
    pub probe_at: u64,
    /// **Reproduce the version-3 restore**: zero `Memory::slept_usec` after restoring, which is
    /// what the snapshot format did before this session — see `Machine::snapshot`. The two arms of
    /// research/10 Addendum 31 differ in this flag and nothing else, including the snapshot file
    /// itself, which is why it is a run-time ablation rather than a second build.
    pub clock_v3: bool,
    /// Plug the mains charger in: hold `GPIOL` bit 3 low, which is what RetailOS's charger sense
    /// reads (research/10 Addendum 30 §1 — the pin is active low, and a region default of zero was
    /// the accidental lie that made every early boot draw the charging screen). `map_hardware`
    /// seeds the bare-iPod value `0x08`; this puts the machine back on a wall socket **on purpose**,
    /// which is the only configuration in which there is a charging screen to return to.
    pub charger: bool,
    /// `--samples=8,12,16` — where the probe samples the panel, in **millions of instructions**
    /// past the moment it acts. Empty means [`PROBE_SAMPLES`].
    pub samples: Vec<u64>,
    /// `--power-cycle-at=N` — cut power at instruction N of the first session and boot again, with
    /// no window and no hand on the button.
    ///
    /// The self-check for the power controls, and it exists for the same reason `--headless` does:
    /// "the GUI can cold boot" is a claim until the second session prints a boot fingerprint of its
    /// own, counted from zero. A restore-that-calls-itself-a-cold-boot would be caught here by the
    /// instruction count alone.
    pub power_cycle_at: Option<u64>,
    /// `--ablate=pmu` — at the moment the probe acts, replace the PMU with a factory-fresh one.
    ///
    /// This is the isolation for §5 of research/10 Addendum 31. A **restored** machine differs from
    /// a cold one at the same instruction count in exactly the state a snapshot does not carry, and
    /// `Memory::pmu` is the largest piece of it: `Machine::restore` never touches the device, so a
    /// restored machine runs with the `Pcf50605` `build()` made — power-on register defaults, an RTC
    /// block of zeroes — having forgotten everything the bootloader and RetailOS wrote to it. Doing
    /// that deliberately to a *cold* machine is the only way to ask whether it is what matters,
    /// short of putting the chip in the snapshot and changing every restored run to find out.
    pub ablate_pmu: bool,
    #[allow(dead_code)]  // retired when: `args::FLAGS` accepts `--control=` again — it refuses it as `Gone::Instrument`, and §12.9 keeps the socket out on purpose: a socket that appears without being asked for is an interface nobody audited
    /// Where to open the control socket, if anywhere.
    ///
    /// Absent by default. A socket that appears without being asked for is an interface nobody
    /// audited, on a program that reads a NOR dump and a drive image.
    pub control: Option<PathBuf>,
    /// Record execution inside this address range, for code that resists being read.
    pub trace_pc: Option<(u32, u32)>,
    /// Stop forcing the second core to report itself asleep (ledger #7).
    pub cop_awake: bool,
    /// Stop reporting the IDE0_CFG interrupt latch in bit 3 (ledger #9).
    pub ide_irq_latch_off: bool,
    /// Addresses to count reads of, with the PC that made each one.
    ///
    /// `input_regs` cannot answer this: it counts reads *before the first write*, so any register
    /// this model seeds at startup -- GPIOA's hold line among them -- reports zero reads forever
    /// after. "Does the firmware ever look at this?" is a different question from "do we invent it",
    /// and only one of them had an instrument.
    pub read_count: Vec<u32>,
    /// `BASE:SIZE` — enumerate the addresses the firmware reads before it has ever written them.
    ///
    /// These are hardware *inputs*: values firmware expects silicon to supply and we answer with
    /// whatever the region holds, which is almost always zero. `trace.rs` has had this since the
    /// `fast_region` bug; the binary that boots the retail path never did, so the one machine whose
    /// DRM actually runs could not be asked the question.
    pub input_regs: Option<(u32, u32)>,
    /// `BASE:LEN` — log writes into this range with the PC that made them.
    ///
    /// The step that turns "these are RSA operands" into "and this is where they came from": a
    /// buffer's first writer names its source, and the source is the whole question — NOR bytes we
    /// hold, or a value this model invented.
    pub watch_writes: Option<(u32, u32)>,
    /// `ADDR:N` — dump the register file the first N times ADDR executes.
    pub regs_at: Option<(u32, usize)>,
    /// Count executed instructions per 64-byte bucket and report the hottest.
    pub profile: bool,
    /// Disable the headless idle heuristic.
    pub no_idle_stop: bool,
    /// Write a named memory region out when the run ends, as `NAME:FILE`.
    ///
    /// Exists so `tcb` can be pointed at a real boot. That tool reads the whole RTXC scheduler out
    /// of an SDRAM image -- every task's state, its saved resume PC, and therefore which kernel
    /// primitive it is waiting in -- which is the question a call that never returns raises.
    pub save_region: Option<(String, PathBuf)>,
    /// Record call edges from this instruction count onward.
    pub trace_calls_from: Option<u64>,
    /// Addresses to watch, reported whenever one changes.
    ///
    /// Built for a single question: `06-game-drm.md` establishes that `[0x14937194]` is the DRM
    /// context pointer `FUN_000103d4` fills in, that it stayed `0x00000000` in every arm measured
    /// so far, and that every later content-key unwrap therefore ran against a null. Whether a
    /// keybag minted against the identity this machine actually presents changes that is one word.
    pub watch: Vec<u32>,
}

/// The scripted measurements this front end can make with no window and no hand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)]  // retired when: `args::FLAGS` accepts `--probe=` again — it refuses it as `Gone::Instrument`, and `Menu` and `ComboControl` are two arms of one measurement, so a probe with half its arms is not a controlled one
pub enum Probe {
    /// Press Select at the anchor — from the first-run Language list that is the main menu — and
    /// then sample the panel six times over the next 800 M instructions while nothing touches it.
    /// The question is whether the menu stays up.
    Menu,
    /// The matched control: same anchor, same sampling instants, **no press**.
    MenuControl,
    /// Hold MENU+SELECT together for `COMBO_HOLD` instructions. On real hardware that is the hard
    /// reset; whether anything in our model acts on it is the measurement.
    Combo,
    /// The matched control: hold MENU alone for the same span, so "the firmware saw the buttons"
    /// and "the firmware saw *that pair* of buttons" are distinguishable.
    ComboControl,
}

/// What a probe run does after the machine is powered up. Kept out of [`Config`] because the UI
/// never issues these — they are the emulator thread's own lifecycle.
enum Outcome {
    /// The window is closing, or a headless run finished.
    Quit,
    /// Power was cut. Publish a dark panel and wait.
    PoweredOff,
    /// Start again from the reset vector, with a machine built from nothing.
    ColdBoot,
    /// The same, but into a different boot target. Carried out here rather than set on the config
    /// directly because the emulator thread owns the config and the UI thread does not.
    ColdBootInto(BootTarget),
}

/// What the machine is asked to boot.
///
/// **They differ only in where the first instruction comes from.** Everything else — the drive, the
/// co-processor, the wheel, the identity out of the NOR — is the same machine, which is the point:
/// Rockbox and Apple's diagnostics are not modes of this program, they are programs this iPod runs.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
#[allow(dead_code)]  // retired when: §12.5's `Start into` rows are drawn — the drawer's device page is where `Nor("diag")` and `Image(path)` are chosen, and the window boots `Os` until it exists
pub enum BootTarget {
    /// The operating system on the drive, entered the way the machine enters it — from the reset
    /// vector on a real dump, or through the high-level boot on a synthesised one.
    #[default]
    Os,
    /// One of the boot ROM's own self-contained images, by tag: `diag` for Apple's service
    /// diagnostics, `disk` for target disk mode. Cut out of the dump in use.
    ///
    /// On real hardware these are reached by a key chord held at power-on, and the boot ROM does
    /// the loading — see research/07, where that is measured and works. It is done here instead
    /// because releasing the chord afterwards currently storms the interrupt controller, and a
    /// diagnostics screen you cannot press a button on is not much of one.
    Nor(String),
    /// Any raw ARM image that expects to be loaded at 0x10000000 and entered there — Rockbox's
    /// `rb-main.raw`, its bootloader, `ipodloader2`. The same contract Apple's own `flsh` images
    /// have, which is why one code path serves both.
    Image(PathBuf),
}

impl BootTarget {
    /// Whether this is the ordinary boot. The machine is built differently for everything else:
    /// no low mirror, the non-cold memory map, and the CPU placed where the image is.
    pub fn is_os(&self) -> bool {
        *self == BootTarget::Os
    }

    /// One line for a person, and for the window's picker.
    #[allow(dead_code)]  // retired when: §12.5's boot-target picker is drawn; this is the label on its rows
    pub fn label(&self) -> String {
        match self {
            BootTarget::Os => "iPod software".into(),
            BootTarget::Nor(t) if t == "diag" => "Diagnostics".into(),
            BootTarget::Nor(t) if t == "disk" => "Disk mode".into(),
            BootTarget::Nor(t) => t.clone(),
            BootTarget::Image(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }

    /// `os`, a NOR tag, or a path — the one spelling the command line and the settings file share.
    #[allow(dead_code)]  // retired when: something reads a boot target back — `args::FLAGS` refuses `--boot=` as `Gone::Device` (§12.5 puts it on the device's drawer page), and no settings key holds one
    pub fn parse(s: &str) -> BootTarget {
        match s.trim() {
            "" | "os" => BootTarget::Os,
            t if t.contains('/') || t.contains('\\') => BootTarget::Image(PathBuf::from(t)),
            t => BootTarget::Nor(t.to_string()),
        }
    }
}

/// UI -> emulator, the commands that are not buttons.
///
/// Deliberately not modelled as click-wheel input: nothing measured in this project shows RetailOS
/// or our model acting on MENU+SELECT (see `Probe::Combo` and research/10 Addendum 31 §5), so a
/// control that claimed to be the hardware combo while actually restarting the emulator would be
/// the UI lying about what the machine does. These restart the *emulator*, and say so.
#[derive(Clone, PartialEq, Eq, Debug)]
#[allow(dead_code)]  // retired when: §12.5's power rows are drawn on the drawer's device page — `PowerCycle` and `Boot` are two of the four it offers, and the bench's centre button is only ever `PowerOff` (§7.3's booting row) or `PowerOn`
pub enum Cmd {
    PowerOff,
    /// Power on from off — always a cold boot, never a restore.
    PowerOn,
    /// Power off and straight back on.
    PowerCycle,
    /// Power-cycle into a different boot target. The real device reaches diagnostics by a key
    /// chord held at power-on, so this is a power cycle and not a mode switch.
    Boot(BootTarget),
}

/// What the emulator thread publishes. Read by the UI once per repaint; never held across a frame.
#[derive(Clone)]
pub struct Out {
    pub phase: Phase,
    /// 320x240 RGB888, converted from the co-processor's RGB565 surface.
    pub fb: Vec<u8>,
    pub fb_nonzero: u32,
    pub fb_addr: u32,
    /// Bumped whenever `fb` changed, so the UI can skip a texture upload.
    pub fb_seq: u64,
    /// The panel's dimmer, 1..32, counted off the firmware's own pulses. Nothing reads this back
    /// from hardware, so it exists only where it is counted.
    pub backlight: u8,
    /// Steps up and down the dimmer has taken. A level that is not moving and a pin that is not
    /// pulsing are different diagnoses, and the level alone cannot tell them apart.
    pub backlight_steps: (u64, u64),
    /// Wall seconds the instruction count has not moved while the phase says `Running`.
    ///
    /// A halted machine and a still screen look identical from the outside — the window keeps
    /// repainting, the panel keeps its last frame, and nothing says anything. One session stopped
    /// dead at 2 791 999 952 instructions and was only noticed because two `state` replies happened
    /// to be compared by hand. The core halts waiting for an interrupt; if nothing has a deadline
    /// armed, none ever comes, and the clock cannot advance because the clock is made of
    /// instructions.
    pub stalled_secs: f32,
    /// Page addresses the machine has touched that nothing answers for.
    ///
    /// Carried out here rather than asked for, because the question it settles -- *is the DRM
    /// failing because hardware is missing?* -- wants the addresses, not a count, and a count that
    /// moved with no way to see what moved it would be a worse instrument than none.
    pub unmapped_pages: Vec<u32>,
    /// The most recent PC trace handed over by the run loop.
    pub pc_trace: Vec<(u32, u64)>,
    /// The PMU's write census, `(register, writes, last value)`, handed over on request.
    ///
    /// Carried through `Out` rather than read directly because `Pcf50605` lives on the emulator
    /// thread; this is the same request/answer route `peek` and `trace` use, for the same reason.
    pub pmu_written: Vec<(u8, u64, u8)>,
    /// The `--watch-writes` census, `(word address, writes, 0)`, handed over on request. The
    /// third field is a placeholder: `WatchWord` keeps the writers, not the value.
    pub watched_writes: Vec<(u32, u64, u8)>,
    /// The `--watch-writes` value log, `(pc, address, byte, usec)`, drained on request.
    pub bus_log: Vec<(u32, u32, u32, u32)>,
    /// The surface the window is **not** showing, and whether its content has moved since this
    /// session began.
    ///
    /// Not a curiosity. A restored machine can be one page-flip out of phase with a cold one: fed
    /// the identical input it draws the identical picture — same digest, to the pixel — into the
    /// *other* buffer, and a window hard-wired to `0x000e0000` shows a frozen screen and no reason
    /// for it. Measured in research/10 Addendum 31 §5. Nothing here models which surface the panel
    /// is actually scanning out, so the honest thing is to report both and say which one is moving.
    pub fb_other_nonzero: u32,
    pub fb_other_moved: bool,
    pub fb_shown_moved: bool,
    /// **What this cold boot cost, and only when it was OBSERVED to end.** GUI.md §12.3's
    /// denominator, published from the one place that can tell the two endings apart.
    ///
    /// The boot phase ends two ways and they are not the same fact: the machine going quiet is an
    /// **observation** ([`Quiet`]), and `executed >= snap_at` is a **fallback** for the case that
    /// never happens. Both set [`Phase::Running`] one line apart, so a reader watching for the
    /// phase change and taking `Stats::executed` at that moment records `snap_at` — a constant —
    /// on every machine that never settled, and files it as *this device's own last completed
    /// cold boot*. That is exactly the substitution `Device::cold_boot_instructions` replaced
    /// `snap_at` to fix, re-entered through the back door. [`boot_end`] is the one function that
    /// decides, and this field is `Some` only for its observed arm.
    ///
    /// `None` also for a **restored** machine, which never enters `Booting` at all: a resume is not
    /// a cold boot and must not teach the denominator what one costs.
    pub booted_at: Option<u64>,
    pub stats: Stats,
}

/// **Has the cold boot ended, and was its end observed?** GUI.md §12.2 and §12.3, in one place.
///
/// Three answers, and the middle one is the whole reason this is a function rather than a
/// condition in the run loop:
///
/// - `None` — still booting.
/// - `Some(None)` — the boot phase ends, and **nothing was measured**. `snap_at` is the fallback,
///   *"a point chosen because it is a good place to resume from, not because it is where the boot
///   ends"*, so the instruction count at this instant is a constant wearing a measurement's name.
/// - `Some(Some(n))` — the machine went quiet at `n` instructions with its drive answered. It has
///   finished starting, and `n` is what §12.3 divides by next time.
///
/// **`settled` is `Quiet::read`'s answer and this function does not recompute it**, deliberately:
/// the observation needs a trailing window and therefore state, and a pure decision that owns no
/// state cannot be tricked into disagreeing with the loop that feeds it.
///
/// The two arms are one line apart in the loop and produce the same phase, which is what makes the
/// wrong one so easy to read: `a_boot_that_ended_on_the_fallback_teaches_the_denominator_nothing`
/// carries the substitution as its own control.
pub fn boot_end(settled: Option<u64>, executed: u64, snap_at: u64) -> Option<Option<u64>> {
    if let Some(n) = settled {
        return Some(Some(n));
    }
    (executed >= snap_at).then_some(None)
}

/// **Has the machine stopped starting?** A trailing window of its own steps, and how much of that
/// window the core spent halted.
///
/// # Why this and not the thing it replaced
///
/// The observed arm used to be *"RetailOS wrote `0x8001052a` to ask the click wheel for frames"*,
/// reasoned about rather than measured: *a machine asking for input is a machine that has finished
/// starting*. Booted from Apple's own NOR it declared the cold boot over at **2 250 000**
/// instructions of **872 043 218** — 0.26 % — with the drive not yet answered and the panel black,
/// and taught `Device::cold_boot_instructions` a denominator that draws a bar full at 0.12 %.
///
/// **Because the first one is not RetailOS's command, and RetailOS's own is not the end of a boot
/// either.** Two measurements, and the second is the one that matters:
///
/// - **Whose it is.** `ipod-boot retail --storeaddr=0x7000c120 --storelog-dump=`, on the same NOR
///   dump and the same reference drive: the first `0x8001052a` is written by **`pc = 0x4000e654`**
///   at **@2 211 983** — the boot ROM's own opto bring-up, running out of IRAM **55 M instructions
///   before the drive answers at all**. `eapp-loader`'s snapshot note had said as much all along:
///   *"the firmware turns it on once with opcode `0x052a` early in the boot"*.
/// - **Whether RetailOS sends one later.** It does — and the window's own boot is the only run that
///   can say so, because `ipod-boot retail` diverges from it (see below). Over the bench boot's
///   872 M instructions there are **five**, all payload 1: `@2 205 089` (the ROM's),
///   `@111 545 868`, two within one sample at `@823 611 625`, and `@823 719 014`. RetailOS's
///   earliest is **50x further on than the ROM's and still only 12.8 % of the boot**; the other
///   three arrive *after* the machine has already settled. So no arrival of this command is the end
///   of a cold boot, and the fix is not "watch a later one".
///
/// research/10 Addendum 32 is the write-up and the retraction.
///
/// # What replaces it, and why it is not another guess
///
/// **A booted machine halts and a booting one does not.** It is the one thing every operating
/// system this program runs has in common — it needs no detection of which one is on the drive,
/// which is the same property `Device::cold_boot_instructions` exists for. RetailOS reaches its
/// language picker and sleeps; Rockbox reaches its menu and sleeps (a 400 M-step budget of
/// `ipod-boot rockbox` executes **77 264 434** instructions — 80.7 % of the whole run halted, boot
/// included); iPodLinux takes 21.5 G and then sleeps.
///
/// The numbers that make it a reading rather than a threshold somebody liked are on
/// [`QUIET_WINDOW_STEPS`]. `idle_steps` costs an addition in the halt arm of `Machine::run` and
/// nothing per instruction, which is what makes it affordable in a window nobody is measuring —
/// `--stop-when-idle`'s novelty bitset is 512 KB and a probe per instruction, and `emu::build` arms
/// it for `--headless` alone.
///
/// # The drive has to have answered
///
/// Halted is not booted on its own: a machine waiting for an interrupt that is never coming is
/// halted too, and it would teach the denominator whatever count it hung at. So [`Self::read`] also
/// requires at least one ATA command, which is the number the bug report led with — *the window
/// leaves `Booting` … 0 ata* — and the failure mode of requiring it is the safe one: a boot that
/// never touches its drive ends on the `snap_at` fallback and teaches nothing, rather than teaching
/// something wrong.
///
/// # `ipod-boot retail` is not this boot, and that is not fixed here
///
/// Pinned to the same NOR dump and the same `PRISTINE` drive, `ipod-boot retail` reaches **70 ATA
/// commands** and stops at Apple's own logo — after 1.2 G instructions, with `--clickwheel` and
/// with `--bcm-registry`, both tried. The window reaches **768** and the language picker in 872 M.
/// So the trace front end's `--enterlog` reporting **0 arrivals** at all five of RetailOS's
/// documented `0x052a` senders is a fact about a machine that never got that far, not about
/// RetailOS — and reading it as one is exactly the mistake this whole entry is about. `KNOWN-BUGS.md`
/// carries the divergence.
#[derive(Default)]
pub struct Quiet {
    /// `(executed, idle_steps)` at the start of the open window, and `None` before the first read.
    mark: Option<(u64, u64)>,
    /// The answer, once given. A boot ends once.
    settled: Option<u64>,
}

impl Quiet {
    /// Feed the machine's own counters. Answers `Some(n)` from the first full window that was
    /// [`QUIET_HALTED_PERCENT`] halted, where `n` is the instruction count at the **start** of that
    /// window — the last moment the machine was doing work, which is what the boot cost.
    ///
    /// The window tumbles rather than slides: it is re-armed at every evaluation, so the cost is
    /// two subtractions per slice and the answer can be at most one window late. Eight million
    /// steps is nine thousandths of the boot it is measuring.
    pub fn read(&mut self, executed: u64, idle_steps: u64, ata_commands: u64) -> Option<u64> {
        if self.settled.is_some() {
            return self.settled;
        }
        let steps = executed + idle_steps;
        let Some((was_executed, was_idle)) = self.mark else {
            self.mark = Some((executed, idle_steps));
            return None;
        };
        let window = steps - (was_executed + was_idle);
        if window < QUIET_WINDOW_STEPS {
            return None;
        }
        let halted = idle_steps - was_idle;
        if ata_commands > 0 && halted * 100 >= window * QUIET_HALTED_PERCENT {
            self.settled = Some(was_executed);
        } else {
            self.mark = Some((executed, idle_steps));
        }
        self.settled
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Cold-booting. The number is the instruction count the snapshot will be taken at, so the UI
    /// can show a progress bar that means something.
    Booting {
        target: u64,
    },
    Running,
    /// Power is off: no machine exists, nothing is executing, and the panel is dark. The state a
    /// 5G is in with a flat battery and no charger, and the one an emulator that "paused" would be
    /// pretending to be while keeping every register alive.
    Off,
    /// The machine stopped and will not resume — `Stop::Lost` or a failed setup.
    Stopped(String),
}

#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub executed: u64,
    /// The emulator's own clock, in simulated microseconds.
    pub sim_usec: u32,
    /// Wall-clock seconds since the machine started running in this process.
    pub wall_secs: f64,
    /// Instructions executed in this process — the numerator of the honest speed ratio. Distinct
    /// from `executed`, which includes everything a restored snapshot had already done.
    pub executed_here: u64,
    /// Simulated microseconds elapsed in this process. **Computed and not yet shown** — and it
    /// became worth showing the day the clock stopped inventing time: against `wall_secs` it is the
    /// honest "how fast is this iPod running compared to a real one", now that idle costs what
    /// running costs.
    ///
    /// **§12.8 decided this one the other way and it is drawn**: it *"earns a row — it is the
    /// honest simulated-versus-wall ratio now that idle costs what running costs"*. What is drawn
    /// is the number, beside the wall clock it would be divided by; the **ratio** is not, because
    /// §12.8 could not state a divisor its own worked example agreed with. See `readout.rs`.
    pub sim_usec_here: u64,
    /// **Loop iterations the core spent HALTED**, straight off `Machine::idle_steps`.
    ///
    /// The counter a booting machine barely moves and a booted one moves almost exclusively: a core
    /// that has finished starting sits in `CPU_CTRL`'s sleep bit waiting for an interrupt, and one
    /// that is still starting is executing. It costs an addition in the halt arm of `Machine::run`
    /// and nothing at all anywhere else — no bitset, no per-instruction probe — which is what makes
    /// it affordable in a window that is not being measured. `eapp-loader`'s own note on
    /// `last_novel_sleeps` is where this was first written down: *"a machine that is genuinely
    /// waiting asks the core to sleep, so a window with zero sleeps in it is a busy machine"*.
    pub idle_steps: u64,
    pub hold: bool,
    pub touched: bool,
    pub position: u8,
    pub buttons: u8,
    /// The `0x052a` gate. **On at reset** — the part streams unless told not to, which is what
    /// lets a driver that never sends the command (Rockbox) receive anything at all. Autonomous
    /// frames also need the receiver armed; both conditions live in `eapp-loader`.
    pub reporting: bool,
    /// Whether the firmware has *sent* `0x052a` at all — a different question from whether the
    /// stream is on. After a restore it starts false, because the click wheel is not part of a
    /// snapshot.
    ///
    /// **It used to mean "this machine has finished starting and wants input", and it does not.**
    /// The command is the boot ROM's, sent 2.2 M instructions into a cold boot; see [`Quiet`].
    /// It is a readout gauge and nothing decides anything on it.
    pub asked_for_frames: bool,
    /// **The census behind that bool** — how many `0x052a` commands the firmware has sent, and the
    /// instruction count and payload of the last one.
    ///
    /// A count where there was a flag, because the flag is what hid the defect: `set_commands > 0`
    /// reads the same at one command and at thirty, so nothing in the window could say that a
    /// whole 871 M cold boot contains exactly **one** of them and that it arrives at 2.2 M.
    pub wheel_sets: u64,
    pub last_wheel_set: Option<(u64, u8)>,
    pub frames_posted: u64,
    pub frames_dropped: u64,
    pub frames_suppressed: u64,
    /// **The CLICK WHEEL's DATA register**, read as words, and how many of those found a frame
    /// waiting. `--selftest` prints them as *"DATA reads N (M with a frame waiting)"*, which is
    /// what they are.
    ///
    /// **They are not the drive, and §12.8's BUS group drew the first of them under the label
    /// `ata commands`.** One field, filled from `m.mem.clickwheel`, rendered as the count of
    /// commands issued to a disk — so every Readout ever taken reported the wheel's serial traffic
    /// as storage traffic, and a machine whose drive had never answered at all would have shown a
    /// healthy four-figure number there. [`Stats::ata_commands`] is the drive's own census, and
    /// this pair keeps the name of the register it comes off.
    pub data_reads: u64,
    pub data_reads_ready: u64,
    /// IRQ 40 assertions — the click wheel's line, and the third of that group.
    pub irqs: u64,
    /// **Commands issued to the drive, as a census.**
    ///
    /// `Ata::commands` is a `Capped<_>` holding 256 rows, and a retail cold boot issues about seven
    /// hundred — so `commands.sample().len()` is a cap wearing a census's clothes, which is
    /// research/12's whole subject. `seen()` is the number, and it is the number `report_headless`
    /// already prints beside `retail-boot.sh`'s.
    ///
    /// **It is what tells a boot from a bootloader.** research/04 row 9's A/B is 102 ATA commands
    /// with the IDE interrupt latch modelled against **24** without it — and the 24 is Apple's
    /// bootloader painting its own screen and never handing RetailOS the disk. A window that could
    /// not publish this number could not tell those two apart from the outside.
    pub ata_commands: u64,
    /// Steps refused because the drain queue was already full — always shown, never silent.
    pub input_dropped: u64,
    // **`queued` was here and is deleted.** It was the depth of the input drain queue, computed
    // every slice and read by nothing, under an allow whose condition was *the Readout draws queue
    // depth*. §12.8 built the Readout and decided the opposite: *"`input_dropped` is the number
    // that matters, because a refused step is a lie about what you did and a deep queue is only
    // ever the reason for one."* A field whose stated retirement condition has been met with a
    // *no* is dead rather than deferred, so it goes rather than getting a new allow.
    /// Arrivals at each of [`WATCHED`], in that order.
    pub enters: [u64; WATCHED.len()],
    /// Co-processor activity **since the machine started running in this process**. Both counters
    /// live on `Bcm` and `Machine::restore` builds a fresh one, so after a restore they start at
    /// zero even though the surface they filled is right there on the panel. Labelled accordingly
    /// in the UI rather than left to read as "RetailOS has never drawn".
    pub bcm_frames: u64,
    pub bcm_commands: usize,
}

/// UI -> emulator: the physical events, in the order the pointer caused them, and the power
/// commands, which are not physical events and are kept apart from them.
#[derive(Default)]
pub struct Inbox {
    pub events: VecDeque<WheelEvent>,
    pub cmds: VecDeque<Cmd>,
}

pub struct Link {
    pub inbox: Mutex<Inbox>,
    pub out: Mutex<Out>,
    pub quit: AtomicBool,
    /// Re-take the idle snapshot from wherever the machine currently is.
    ///
    /// The restore point was fixed at `--snap-at` instructions into a cold boot, which on this
    /// machine lands *before* first-run setup — so every launch resumed a first-run iPod and asked
    /// for a language, a state no synced iPod is ever in. The snapshot is a pair (RAM plus the
    /// drive beside it), so re-taking it has to write both, which is why this is a request to the
    /// run loop rather than something the socket thread can do itself.
    pub resnap: AtomicBool,
    /// Park the machine on the way out: write the restore point before honouring `quit`.
    ///
    /// **This is what makes working directly on the drive worth doing.** A restore point taken at
    /// `--snap-at` pairs with a drive the machine then goes on writing to, so it is stale before
    /// anything can use it; the only instant at which RAM and the user's own drive provably agree
    /// is the one after which nothing runs. Set by the window as it closes, and by nothing else —
    /// `--headless`, `--selftest` and `--probe` all end through `quit` too, and none of them should
    /// leave a restore point behind.
    pub save_on_quit: AtomicBool,
    /// Set while the restore point is being written, so the window can say what it is waiting for
    /// rather than appearing to hang for the second or two a 1.6 GB write takes.
    pub saving: AtomicBool,
    /// **The Unix second at which a COMPLETE restore point was written**, or 0 for never.
    ///
    /// A pair, not a file: `write_restore_point` sets this only after the snapshot and the half
    /// that pairs it with the drive are both on disk, so a park whose companion could not be
    /// written — the case that deletes the snapshot again — leaves it at 0 and `Device::parked_at`
    /// is never claimed for a restore point that is not there.
    ///
    /// `AtomicU64` rather than a field of [`Out`]: the window reads it **after** the machine thread
    /// has finished, which is exactly when nothing is publishing an `Out` any more.
    pub parked: AtomicU64,
    /// **What a restore point for this machine would cost, in bytes**, or 0 before one is built.
    ///
    /// GUI.md §12.4 wants free space checked *before* `save_on_quit` is set, and the only honest
    /// denominator for that check is this machine's own memory. Published once per session by
    /// [`snapshot_bytes`], which sums the regions the snapshot format actually writes rather than
    /// quoting §12.4's "~1.6 GB" — a figure that is about the frozen **drive** and not about the
    /// RAM half at all.
    pub snapshot_bytes: AtomicU64,
    /// Addresses the control socket has asked about, and what they held when the run loop next
    /// looked.
    ///
    /// A request/answer pair rather than a callback, because `Memory` lives on the emulator thread
    /// and cannot be borrowed from another one. The run loop drains requests between slices, which
    /// is also the only moment a read is guaranteed to see a coherent machine.
    pub peek_req: Mutex<Vec<u32>>,
    pub peek_ans: Mutex<Vec<(u32, Option<u32>)>>,
    /// An LBA range the control socket wants to know about, and the answer.
    ///
    /// The question this exists for is the one that splits the DRM problem in half: **does RetailOS
    /// ever read the key files at all?** Never reading them means the refusal happens before the
    /// keystore is consulted and no keybag, however perfect, would change it. Reading and rejecting
    /// them means the opposite. The drive already logs every command with its LBA; this only asks.
    pub ata_query: Mutex<Option<(u64, u64)>>,
    pub ata_answer: Mutex<Option<String>>,
}

impl Link {
    pub fn new() -> Arc<Self> {
        Arc::new(Link {
            inbox: Mutex::new(Inbox::default()),
            peek_req: Mutex::new(Vec::new()),
            peek_ans: Mutex::new(Vec::new()),
            ata_query: Mutex::new(None),
            ata_answer: Mutex::new(None),
            out: Mutex::new(Out {
                phase: Phase::Booting { target: 0 },
                fb: vec![0; FB_W * FB_H * 3],
                fb_nonzero: 0,
                fb_addr: FB_FRONT,
                unmapped_pages: Vec::new(),
                pc_trace: Vec::new(),
                pmu_written: Vec::new(),
                watched_writes: Vec::new(),
                bus_log: Vec::new(),
                fb_seq: 0,
                backlight: 16,
                backlight_steps: (0, 0),
                stalled_secs: 0.0,
                fb_other_nonzero: 0,
                fb_other_moved: false,
                fb_shown_moved: false,
                booted_at: None,
                stats: Stats::default(),
            }),
            quit: AtomicBool::new(false),
            resnap: AtomicBool::new(false),
            save_on_quit: AtomicBool::new(false),
            saving: AtomicBool::new(false),
            parked: AtomicU64::new(0),
            snapshot_bytes: AtomicU64::new(0),
        })
    }

    /// Queue a physical event. Rotation steps are dropped once the queue is deep enough that they
    /// would arrive visibly late; touch, release, hold and buttons never are, because they are rare
    /// and because a dropped release is a stuck finger.
    pub fn push(&self, ev: WheelEvent) {
        let mut inbox = self.inbox.lock().unwrap();
        if matches!(ev, WheelEvent::Step(_)) && inbox.events.len() >= MAX_QUEUE {
            drop(inbox);
            let mut out = self.out.lock().unwrap();
            out.stats.input_dropped += 1;
            return;
        }
        inbox.events.push_back(ev);
    }

    /// Cut power, restore it, or both. Queued rather than acted on here: only the emulator thread
    /// owns the machine, and a UI thread that dropped a `Machine` would be racing the interpreter.
    pub fn command(&self, c: Cmd) {
        self.inbox.lock().unwrap().cmds.push_back(c);
    }
}

/// One full rotation of backlog. Deeper than this and a drag is being reported minutes after it
/// happened, which is worse than admitting the drop.
const MAX_QUEUE: usize = 96;

/// Instructions per `Machine::run` call. Small enough that queued input is picked up within about
/// half a millisecond of wall time, large enough that the per-call `invalidate_fast()` is noise.
const SLICE: usize = 250_000;

// ---------------------------------------------------------------- building the machine

/// A stand-in for the game image every `trace` invocation is handed.
///
/// `Machine::new` wants an `EApp` because that is how the loader was born, and it uses it for
/// exactly three things: the load base, the image bytes, and the import thunks to trap. A boot run
/// executes none of them — RetailOS is entered from the reset vector and never looks at
/// `0x18000000`. So the GUI hands it an empty one at the address a real title links to, which
/// reproduces the region *layout* every measurement was taken through (image, stack, heap, in that
/// order, ahead of everything `map_hardware` adds) without requiring a copyrighted binary on disk
/// to start a window.
fn placeholder_app() -> EApp {
    EApp {
        load_base: 0x1800_0000,
        entry: 0,
        vectors: Vec::new(),
        image: Vec::new(),
        frameworks: Vec::new(),
    }
}

impl Config {
    /// Whether this launch may restore — asked in exactly one place because two places would
    /// eventually disagree, and disagreeing is the bug.
    ///
    /// A snapshot **and** the drive it was taken against must both be present and must still
    /// agree. Either alone is an incomplete pair, and using half of one is what produced the
    /// intermittent "connect to computer" screen; see [`Config::frozen`]. A snapshot written
    /// before its other half existed is therefore ignored once, cold-booted past, and replaced by
    /// a complete set.
    ///
    /// What "the drive it was taken against" *is* differs by mode, and [`Config::pair_is_whole`]
    /// is the single place that knows which.
    ///
    /// `first` is false for a power cycle inside a running session, which never restores.
    pub fn may_restore(&self, first: bool) -> bool {
        !self.cold
            && first
            && self.pair_is_whole()
            && self.snapshot.as_ref().is_some_and(|p| p.exists())
    }

    /// Where the drive stamp lives: beside the snapshot, under the same stem.
    ///
    /// The same rule `--snapshot=` already follows for the frozen drive, and for the same reason —
    /// a hand-given snapshot must bring its own other half rather than pairing with whatever the
    /// cache happens to hold.
    pub fn stamp(&self) -> Option<PathBuf> {
        self.snapshot.as_ref().map(|s| s.with_extension("drive"))
    }

    /// §12.4's parked frame, beside the snapshot under the same stem — **the same rule as
    /// [`Config::stamp`], written once.**
    ///
    /// The writer (`write_parked_frame`) and the reader (`machine::parked_frame`) both ask this,
    /// so a park that wrote `x.parked.png` and a bench that looked for `x.png` is not a state this
    /// program can reach. It was two `with_extension` calls in two files for exactly as long as
    /// this method did not exist.
    pub fn parked_frame(&self) -> Option<PathBuf> {
        self.snapshot
            .as_ref()
            .map(|s| s.with_extension("parked.png"))
    }

    /// The drive as it stands, in the two numbers that change when anything writes to it.
    ///
    /// Running on the image directly means the snapshot's other half is the user's own file, and
    /// nothing stops iTunes, `make-disk` or another emulator session touching it in between. Size
    /// and modification time are what a filesystem will tell us for free; they are not a hash, and
    /// they are not meant to be. What they catch is the case that matters — the drive moved on
    /// without this RAM — and a stale pair is the bug that produced "connect to computer" on every
    /// third start before the frozen drive existed.
    ///
    /// **Nanoseconds, not seconds.** A one-second mtime is coarser than the gap between stamping
    /// the drive and the machine writing to it again, which would let a pair that has already
    /// diverged still compare equal. Filesystems vary in what they actually store, so this narrows
    /// the window rather than closing it — the reason the stamp is written at a moment when
    /// nothing can write next, rather than relying on the resolution.
    pub fn drive_fingerprint(&self) -> (u64, u128) {
        match std::fs::metadata(&self.workdisk) {
            Ok(m) => {
                let nanos = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                (m.len(), nanos)
            }
            Err(_) => (0, 0),
        }
    }

    /// What a stamp file holds. One line, so a person who finds one can read it.
    fn stamp_line(&self) -> String {
        let (len, nanos) = self.drive_fingerprint();
        format!("{len} {nanos}")
    }

    /// Is the half of the pair that is *not* RAM present, and does it still describe this drive?
    ///
    /// **Two modes, two answers, one question.**
    ///
    /// - Working **on a copy**, the other half is a frozen clone taken at the snapshot instant, and
    ///   the machine then runs on a throwaway. The pair is coherent by construction and existence
    ///   is the whole test.
    /// - Working **directly**, there is no clone: the drive under the snapshot is the user's own
    ///   file, and it keeps moving for as long as the machine runs. So the pair is only whole if
    ///   the drive has not been touched since the stamp was written — by this emulator, by iTunes,
    ///   by `make-disk`, or by a second window.
    ///
    /// Every way this can be wrong resolves to `false`, which costs a cold boot. That direction is
    /// deliberate: the opposite mistake is a restored RAM against a drive that has moved, which is
    /// a machine that looks fine and is not.
    pub fn pair_is_whole(&self) -> bool {
        if self.work_on_copy {
            return self.frozen.exists();
        }
        let Some(stamp) = self.stamp() else {
            return false;
        };
        match std::fs::read_to_string(&stamp) {
            Ok(text) => text.trim() == self.stamp_line(),
            Err(_) => false,
        }
    }

    /// Write the half of the pair the snapshot does not carry, for whichever mode this is.
    ///
    /// Called at the instant the snapshot is written and at no other time, because the two are one
    /// act: a snapshot whose companion was written a moment later describes a drive that had
    /// already moved.
    pub fn pair_with_drive(&self) -> Result<String, String> {
        if self.work_on_copy {
            clone_disk(&self.workdisk, &self.frozen)?;
            return Ok(format!("frozen drive -> {}", self.frozen.display()));
        }
        let Some(stamp) = self.stamp() else {
            return Err("no snapshot path, so there is nothing to pair a drive with".into());
        };
        std::fs::write(&stamp, self.stamp_line())
            .map_err(|e| format!("{}: {e}", stamp.display()))?;
        Ok(format!("drive stamp -> {}", stamp.display()))
    }
}

pub fn build(cfg: &Config, first: bool) -> Result<Machine, String> {
    let app = placeholder_app();
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);

    // Set before the peripheral map, which is where the COP_STATUS override is seeded.
    m.mem.cop_awake = cfg.cop_awake;
    m.mem.ide_irq_latch_off = cfg.ide_irq_latch_off;
    m.mem.input_probe = cfg.input_regs;
    if !cfg.read_count.is_empty() {
        m.mem.read_addrs = cfg.read_count.clone();
        m.mem.read_addrs.sort_unstable();
        m.mem.read_addrs.dedup();
        m.mem.set_store_addr_bounds();
    }
    // **`cold_boot` decides where SDRAM's storage lives, and a boot image is not a cold boot.**
    //
    // With it true, SDRAM is a region at 0x10000000 and address 0 belongs to the NOR — the map a
    // machine has out of reset. With it false, the storage is at 0 and 0x10000000 is an alias, and
    // low memory is ordinary writable RAM. A `flsh` image needs the second: it is entered at
    // 0x10000000 with the CPU already running, and the first thing an interrupt does is vector to
    // 0x18, which has to be memory the image can install a handler into.
    //
    // Without this the wheel's IRQ 40 fetched from an unmapped 0x18 and the machine reported
    // `Lost(24)` the instant a button was pressed. `ipod-boot flsh` never passes `--cold-boot`,
    // which is why the same image driven from the command line always worked.
    m.mem.second_core = cfg.second_core;
    eapp_loader::map_hardware(&mut m, cfg.boot.is_os());
    // The part's own name at `PP_VER1`/`PP_VER2`, from the one place that decides it.
    //
    // This wrote `0x00360000` until 2026-08-26 — `research/16`'s chip lie, one byte shaped to pass
    // Apple's bootloader's single test and nothing else — while `trace.rs` had already been moved
    // to the eight characters that spell a part number. So the same machine answered two different
    // chips depending on which program started it, which is one half of the `ipod-boot`-versus-
    // `ipod-gui` divergence in `KNOWN-BUGS.md`. Measured across the change on the PRISTINE drive,
    // one core: **768 ATA / 872 147 649 instructions / 38 313 buckets** before and **769 /
    // 872 236 211 / 38 307** after — one extra `READ DMA`, and the same 75 267 lit pixels and the
    // same `Idle` at the language picker. Not identical, so it is a real if small change, and
    // saying "identical" here before measuring it is the mistake this comment is now the record
    // of. See [`eapp_loader::seed_chip_id`] for which byte and why.
    eapp_loader::seed_chip_id(&mut m);
    {
        use arm7tdmi::Bus as _;
        // `--charger`: GPIOL bit 3 low is "mains charger attached", and it is what decides between
        // the charging screen and the UI. See research/10 Addendum 30 §1 and §6.
        if cfg.charger {
            m.mem.write32(0x6000_d13c, 0x0000_0000);
        }
    }

    let flash = cfg.nor.bytes()?;
    // **The high-level boot.**
    //
    // A synthesised ROM carries the identity block and no code — executing it would branch to
    // 0x8000, find zeros and hang. So the boot ROM's *effects* are produced here instead of its
    // instructions being run: the OS is copied out of the drive's own firmware partition to
    // 0x10000000, the `sysinfo_t` handoff block is written where Apple writes it, and the CPU
    // starts at the OS's entry. That is what "HLE" means here — see [`eapp_loader::nor`].
    //
    // Everything about *which iPod this is* comes from the synthesised flash, so RetailOS reads the
    // same identity it would read off a real one.
    use arm7tdmi::Bus as _;
    let synthetic = eapp_loader::nor::is_synthetic(&flash);
    // **Booting one of the ROM's own images instead of the OS.**
    //
    // `diag` is Apple's service diagnostic — on real hardware you reach it by holding SELECT+REW
    // at power-on. It is a self-contained program in the flash, loaded at 0x10000000 and entered
    // there, needing no drive and no filesystem, so it takes exactly the same high-level path a
    // synthesised ROM takes: place the image, leave the handoff block a real boot ROM would have
    // left, and start. See `ipod-boot flsh`, which is the same boot on the command line.
    let boot_image = match &cfg.boot {
        BootTarget::Os => None,
        BootTarget::Nor(tag) => {
            let img = eapp_loader::inspect::nor_image(&flash, tag).ok_or_else(|| {
                let have: Vec<String> = eapp_loader::inspect::nor_images(&flash)
                    .iter()
                    .map(|e| e.tag.clone())
                    .collect();
                if have.is_empty() {
                    "this boot ROM carries no images at all".to_string()
                } else {
                    format!(
                        "this boot ROM has no `{tag}` image. It has: {}",
                        // ASCII: this becomes a `Failure::said` and therefore UI text, and §6.7
                        // does not trust the window's font with a middle dot — `ui/bench.slint`
                        // draws that one as a `Path`.
                        have.join(", ")
                    )
                }
            })?;
            Some((tag.clone(), img))
        }
        BootTarget::Image(p) => {
            let img = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
            Some((p.display().to_string(), img))
        }
    };
    // `logo` and `vmcs` are data, and entering data does not fail visibly — the interpreter decodes
    // whatever is there and runs out of budget looking busy. The same is true of any file somebody
    // points this at, which is why the check is here and not only on the NOR's own images.
    if let Some((name, img)) = &boot_image {
        if !eapp_loader::inspect::is_bootable(img) {
            return Err(format!(
                "`{name}` is data, not a program: word 0 is not an ARM branch, so there is no \
                 reset vector to enter."
            ));
        }
    }
    if synthetic || boot_image.is_some() {
        let (osos, load_at, entry) = match boot_image {
            Some((name, img)) => {
                println!("  booting `{name}`: {} bytes -> 0x10000000", img.len());
                (img, 0x1000_0000u32, 0u32)
            }
            None => {
                let (osos, load_at, entry) = eapp_loader::ipsw::osos_from_drive(&cfg.disk)?;
                println!(
                    "  high-level boot: {} bytes of OS from {} -> {load_at:#010x}",
                    osos.len(),
                    cfg.disk.display()
                );
                (osos, load_at, entry)
            }
        };
        // **The image is WRITTEN INTO SDRAM, never registered as a region beside it**, and the
        // machine is entered where the bootloader enters it. Both halves are what Apple's own
        // bootloader is measured doing, and this HLE exists to leave what it leaves.
        //
        // The measurement, off `ipod-boot retail` on a drive that boots: 58 READ DMA commands land
        // the firmware partition at `0x10000000` and the last of them ends at `0x10736000` — a
        // 7 561 216-byte image, exactly what `osos_from_drive` hands back — and then the console
        // says `Running 'osos' 0 from 0x10000000`. The OS is in SDRAM and the CPU goes to the top
        // of it.
        //
        // **What was here instead, and how it failed.** The OS was pushed as a region named `osos`
        // at `0x10000000` and mirrored as a live region named `osos-low` at 0, and the CPU entered
        // at 0. Region lookup is first-match and `map_hardware` has already registered 64 MB of
        // `sdram` at `0x10000000`, so the `osos` region was never read by anything: SDRAM was 64 MB
        // of zeros with a copy of the OS filed behind it. That held up only until RetailOS did what
        // RetailOS does about 0x220 bytes into its own entry — program the PP's remap windows at
        // `0xf000f000`, one of which is `0x00000000..0x01ffffff -> 0x10000000`. `Memory::translate`
        // runs before the region list, so from that instruction on every low address resolved into
        // the zeroed SDRAM and the code the CPU was executing went out from under it. It then
        // NOP-slid — `0x00000000` decodes as `andeq r0, r0, r0` — from `0x1ec` to the top of the
        // window and left every mapped region: **`Lost(0x02000000)` after 8 388 485 instructions**,
        // which is `(0x02000000 - 0x1ec) / 4` to the instruction.
        //
        // With the bytes in SDRAM there is one storage and the remap points at it, which is the
        // arrangement the hardware has. Nothing is mirrored at 0: before RetailOS programs that
        // window it is running from `0x1000xxxx`, and after it, address 0 *is* SDRAM.
        eapp_loader::place_image(&mut m, load_at, &osos);
        // **Where the machine starts, decided here and read back by the run loop.** `Machine::new`
        // puts the PC at the placeholder app's entry, which is 0; `session` used to re-decide the
        // address with a second `if cfg.boot.is_os()` of its own, and two places deciding one thing
        // is how the OS came to be entered somewhere its bootloader never enters it.
        //
        // `entry` is non-zero once a bootloader has been appended to `osos` — `ipodloader2` sits at
        // `0x735a00` — and Apple's bootloader honours the directory's offset rather than the load
        // address, printing `Running 'osos' 0 from 0x10735A00` when it does. Starting at `load_at`
        // regardless would run the OS sitting behind the loader instead of the loader.
        m.cpu.regs[15] = load_at + entry;
        if entry != 0 {
            println!(
                "  entry offset {entry:#x} — starting at {:#010x}",
                load_at + entry
            );
        }

        // The handoff, byte for byte as a cold boot leaves it.
        let cfg_block = eapp_loader::inspect::syscfg(&flash);
        let identity = cfg.nor.identity()?;
        let model = cfg
            .nor
            .model()
            .ok_or("the synthesised ROM names a model this program does not know")?;
        let syscfg_bytes = match &cfg_block {
            Some(c) => {
                let at = eapp_loader::inspect::SYSCFG_AT;
                let len = eapp_loader::inspect::SYSCFG_HEADER
                    + c.records.len() * eapp_loader::inspect::SYSCFG_RECORD;
                flash.get(at..at + len).unwrap_or(&[]).to_vec()
            }
            None => Vec::new(),
        };
        let block = eapp_loader::nor::handoff(&identity, model, &syscfg_bytes);
        for (i, chunk) in block.chunks(4).enumerate() {
            let mut w = [0u8; 4];
            w[..chunk.len()].copy_from_slice(chunk);
            m.mem.write32(
                eapp_loader::nor::HANDOFF_AT + (i as u32) * 4,
                u32::from_le_bytes(w),
            );
        }
        // The scaffolding a real ROM would already have done — see `install_sysinfo` in trace.rs,
        // where the same reasoning is spelled out and bisected.
        let hw = |m: &mut Machine, off: u32, v: u32| {
            m.mem.write32(eapp_loader::nor::HANDOFF_AT + off, v)
        };
        hw(&mut m, 0x60, u32::from_le_bytes(*b"Flsh"));
        hw(&mut m, 0x68, 0x2000_0000);
        hw(&mut m, 0x6c, 0x0010_0000);
        hw(&mut m, 0x74, u32::from_le_bytes(*b"Sdrm"));
        hw(&mut m, 0x7c, 0x1000_0000);
        // **Not `RAM_SIZE`** — that is this loader's 8 MB scratch region, and writing it here told
        // RetailOS the iPod had 8 MB of SDRAM. The value a real cold boot leaves in these fields is
        // 0x04000000, measured; `trace`'s `--sysinfo` default is the same number.
        //
        // Whether it varies by model is NOT established. The 5.5G 80 GB is widely reported to carry
        // 64 MB against everything else's 32 MB, and Rockbox builds the Video for 64 "always. This
        // is reduced at runtime if needed" — but our own hardware is a 30 GB 5G, which should then
        // be 32 MB, and its handoff reads 0x04000000 here anyway. So either this field is not the
        // memory size or the reporting is wrong, and until that is measured the hardware's own
        // value is used rather than a per-model guess.
        const MEASURED_SDRAM_WORD: u32 = 0x0400_0000;
        hw(&mut m, 0x80, MEASURED_SDRAM_WORD);
        hw(&mut m, 0x88, u32::from_le_bytes(*b"Frwr"));
        hw(&mut m, 0x9c, u32::from_le_bytes(*b"Iram"));
        hw(&mut m, 0xa4, 0x4000_0000);
        hw(&mut m, 0xa8, 0x0002_0000);
        hw(&mut m, 0xe0, MEASURED_SDRAM_WORD);
        hw(&mut m, 0x128, 0x0005_0014);
        m.mem.write32(
            eapp_loader::nor::HANDOFF_TAG_AT,
            u32::from_le_bytes(*b"IsyS"),
        );
        m.mem.write32(
            eapp_loader::nor::HANDOFF_TAG_AT + 4,
            eapp_loader::nor::HANDOFF_AT,
        );
        println!(
            "  identity: {} · {}",
            identity.serial.as_deref().unwrap_or("(no serial)"),
            identity.guid_hex()
        );
    }
    let size = flash.len() as u32;
    // **Whether the flash also answers at 0**, which is true only of a cold boot from the reset
    // vector. A synthesised ROM has no code to run there, and a boot image is entered where it is
    // loaded — `ipod-boot flsh` maps the flash only at 0x20000000, and that is the configuration
    // every measurement of `diag` was taken on.
    //
    // Deliberately a second name rather than a re-`let` on `synthetic`: shadowing it made one word
    // mean two things twelve lines apart, and the boot-screen paint below — which asks "is there
    // an Apple logo in this ROM?" and not "does the flash answer at 0?" — read the wrong one and
    // put this project's mark over Apple's own boot screen.
    let flash_answers_low = !synthetic && cfg.boot.is_os();
    // Cold boot: the flash also answers at 0, where the CPU fetches out of reset. Inserted at the
    // front so it wins the first-match lookup for low addresses.
    if flash_answers_low {
        m.mem.readonly.push("flash-low");
        m.mem.regions.insert(
            0,
            Region {
                name: "flash-low",
                base: 0,
                data: flash.clone(),
            },
        );
    }
    m.mem.readonly.push("flash");
    m.mem.regions.push(Region {
        name: "flash",
        base: 0x2000_0000,
        data: flash,
    });
    m.mem.nor = Some(if !flash_answers_low {
        // No low mirror on a high-level boot: address 0 is the OS, or the image's own memory.
        Nor::sst39wf800a(vec![(0x2000_0000, size)], vec!["flash"])
    } else {
        Nor::sst39wf800a(
            vec![(0x2000_0000, size), (0, size)],
            vec!["flash", "flash-low"],
        )
    });

    // The co-processor, with the GENCMD registry published.
    //
    // **What this line used to claim, and what a measurement of it says.** The claim was: *"without
    // `registry` RetailOS never gets an answer to its service lookup and never draws — the panel
    // would be a black rectangle."* Ablated on 2026-08-24 as a red proof for
    // `the_bench_boots_apples_software_and_this_needs_resources` — one flag, same ROM, same drive,
    // same start path — the boot is **indistinguishable**: 75 267 non-black pixels, 4
    // co-processor commands, 2 frame updates, RetailOS's language picker on the glass. So the
    // black-rectangle half is not true of *this* boot, and it is left here as what was believed
    // rather than deleted.
    //
    // The line stays on, because what the flag does is separately measured and is not nothing:
    // research/04 records `--bcm-registry` turning 4 DMA transfers into 104. That is a different
    // claim from *the panel goes black*, and the two had been written down as one.
    let mut bcm = Bcm::new(0x3000_0000);
    bcm.registry = true;
    // The boot screen. A real NOR carries a `logo` image and Apple's bootloader blits it; a
    // synthesised one carries the project's own mark, because it could not carry Apple's artwork
    // if it wanted to. Either way it is white on black — every iPod with video boots the same
    // screen whatever colour its case is, corrected 2026-08-19 by the operator, who owned a white
    // one. `Source::boot_screen` is the one place that decides which picture it is.
    if synthetic {
        // Whatever the source says to show — the built-in mark, or an image somebody chose.
        let px = cfg.nor.boot_screen(FB_W, FB_H);
        for (i, v) in px.iter().enumerate() {
            bcm.mem.insert(FB_FRONT + (i as u32) * 2, *v);
        }
    }
    m.mem.bcm = Some(bcm);

    let mut w = ClickWheel::new(0x7000_c000);
    w.irq_enabled = true;
    m.mem.clickwheel = Some(w);

    // The drive must accept writes: RetailOS bootstraps its own volume during boot and blocks on a
    // WRITE DMA that a read-only drive aborts. Cloned per run so the reference image is never
    // touched — `cp -c` on APFS is a copy-on-write clone, so this costs milliseconds for 8 GB.
    //
    // *Which* image it is cloned from is the whole of the coherence fix — see `Config::frozen`. A
    // run that is about to restore takes the drive that snapshot was taken against; a run that is
    // about to cold-boot takes the pristine one. Either way the clone is remade, never reused.
    //
    // **Direct is the default, and then there is no clone at all**: `workdisk` IS the user's image,
    // so the iPod's writes land where the user can see them, which is what the hardware does.
    //
    // `work_on_copy` is tested here rather than left to the paths, and that is not belt-and-braces.
    // The frozen drive is the only input to this function that gets *written over* the working one,
    // and in direct mode the working one is the user's own image — the file that took twelve iTunes
    // sync rounds to build. A mis-wired `frozen` would then restore a stale drive over it, silently,
    // before the machine even starts. Reaching that line requires copy mode to be on, so the
    // destructive branch cannot be entered by a path bug alone.
    let source = if cfg.work_on_copy && cfg.may_restore(first) {
        &cfg.frozen
    } else {
        &cfg.disk
    };
    clone_disk(source, &cfg.workdisk)?;
    let d = Ata::open(&cfg.workdisk, true).map_err(|e| format!("disk: {e}"))?;
    m.mem.ata = Some((0xc300_0000, d));

    m.mem.i2c_base = Some(0x7000_c000);
    m.mem.pmu = Some(Pcf50605::new());

    m.mem.trace_pc = cfg.trace_pc;
    m.mem.trace_calls_from = cfg.trace_calls_from;
    // 8 MB of low address space at 64 bytes a bucket.
    m.mem.regs_at = cfg.regs_at;
    m.mem.watch_range = cfg.watch_writes;
    if cfg.profile {
        m.mem.pc_hist = Some(vec![0u64; (8 << 20) >> 6]);
    }
    m.instr_per_usec = cfg.clock.max(1);

    // The five addresses whose arrival counts are the measurement this GUI exists to make.
    for (pc, _) in WATCHED {
        m.enter_pcs.push(pc);
        m.enter_bloom |= 1u64 << ((pc >> 2) & 63);
    }

    // `--headless` is the self-check, and it has to be comparable to `retail-boot.sh --clock=5
    // --stop-when-idle=400000000` line for line. That flag needs the novelty bitset armed, and the
    // bitset costs 512 KB and a probe per instruction — which is exactly the sort of thing that
    // should not be paid for by a window nobody is measuring. So it is on only here.
    // …and overridable, because "idle" here means "executed no *new* code in the window", which a
    // long-running computation through already-seen code satisfies perfectly. A headless run
    // investigating exactly that will stop early and report an instruction count that is the
    // heuristic's, not the budget's -- which reads as "still running at N" when N never happened.
    if cfg.headless.is_some() {
        m.stop_when_idle = if cfg.no_idle_stop {
            None
        } else {
            Some(400_000_000)
        };
        m.novelty = Some(Default::default());
        m.arm_novelty();
    }
    Ok(m)
}

/// Copy-on-write where the filesystem has it, a plain copy where it does not.
///
/// Three rungs, and the order is the whole content of the function. It is the same ladder
/// `retail-boot.sh` and `ipod-boot` climb, and it exists in three places because each of the three
/// front ends has to make a writable disk before it has anything to share code with:
///
/// 1. **`cp -c`** — Apple's `clonefile(2)`. ~3 ms for 8 GB. Not a GNU flag: on Linux it is an
///    invalid option, so it falls through.
/// 2. **`cp --reflink=auto`** — the btrfs / XFS / bcachefs equivalent, and the rung that was
///    missing. Without it a Linux run paid a **full 8 GB byte copy on every first launch**, and
///    printed `cp: invalid option -- 'c'` on the way past. GNU `cp --reflink=auto` never fails for
///    want of reflink support; it silently does a full copy.
/// 3. **`std::fs::copy`** — Windows (there is no `cp.exe`, and ReFS block cloning is an `FSCTL`
///    this is not going to reach for), and any Unix without a usable `cp`.
///
/// Both subprocesses are skipped outright on Windows: spawning something that cannot exist to
/// discover it does not exist is two failed `CreateProcess` calls per launch.
/// Copy `from` over `to`, replacing whatever was there.
///
/// **The destination is removed first, deliberately.** This used to return early when the
/// destination existed, which quietly turned the per-run clone into a permanent one and is the
/// whole cause of the stale-pair failure documented on `Config::frozen`. Reusing a drive is only
/// ever correct when the RAM that goes with it is reused too, and that decision belongs to the
/// caller, which knows whether it is restoring.
pub fn clone_disk(from: &Path, to: &Path) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    let _ = std::fs::remove_file(to);
    if let Some(dir) = to.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if cfg!(unix) {
        for flag in ["-c", "--reflink=auto"] {
            let ok = std::process::Command::new("cp")
                .arg(flag)
                .arg(from)
                .arg(to)
                // The failed rung prints `cp: invalid option` on the platform where it is the
                // wrong one, and that is noise about an implementation detail, not a diagnostic.
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return Ok(());
            }
            // A failed `cp` can still have left a truncated destination behind, and a partial 8 GB
            // disk image that later opens as a valid file is exactly the silent failure this
            // project keeps paying for.
            let _ = std::fs::remove_file(to);
        }
    }
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("cloning {} -> {}: {e}", from.display(), to.display()))
}

// ---------------------------------------------------------------- the framebuffer readout

/// Convert one 320x240 RGB565 surface out of the co-processor's internal memory.
///
/// Iterated as a *range* rather than 76 800 individual lookups: `Bcm::mem` is a sparse `BTreeMap`
/// and a range walk is one descent plus a linear scan, which is the difference between this costing
/// nothing and it costing more than the emulation it is watching.
pub fn read_framebuffer(bcm: &Bcm, addr: u32, out: &mut [u8]) -> u32 {
    out.fill(0);
    let mut nonzero = 0u32;
    let end = addr + (FB_W * FB_H * 2) as u32;
    for (&a, &px) in bcm.mem.range(addr..end) {
        if px == 0 {
            continue;
        }
        let i = ((a - addr) / 2) as usize;
        if i >= FB_W * FB_H {
            continue;
        }
        nonzero += 1;
        let (r, g, b) = ((px >> 11) & 0x1f, (px >> 5) & 0x3f, px & 0x1f);
        // 5- and 6-bit channels widened by bit replication, the same expansion `--bcm-dump` uses,
        // so a PPM from `trace` and a pixel in this window are the same number.
        out[i * 3] = ((r << 3) | (r >> 2)) as u8;
        out[i * 3 + 1] = ((g << 2) | (g >> 4)) as u8;
        out[i * 3 + 2] = ((b << 3) | (b >> 2)) as u8;
    }
    nonzero
}

// ---------------------------------------------------------------- the thread

/// Drain queued events onto the wheel's script, one per `gap` instructions.
///
/// `next_at` is carried across calls: it is the instruction count at which the next appended step
/// is allowed to fire. Steps are appended with non-decreasing `at`, which is what keeps the script
/// sorted — `service_clickwheel` walks it in order and would otherwise fire them out of sequence.
/// The shortest a button may be held, in **instructions**, which is what makes it a duration in
/// the machine's own time rather than in the operator's.
///
/// A click of a mouse lasts about a tenth of a second of *your* time. This emulator runs at about
/// a third of the real part, so that tenth is thirty milliseconds to the firmware — and firmware
/// that reads its buttons on a timer can miss it entirely. **Apple's `diag` polls once per 150 ms**
/// (`0x10009e7c`: read the button byte, sleep `0x249f0` microseconds, repeat), so every press the
/// window sent it landed between two polls and diagnostics looked like it ignored the wheel. It
/// did not: the interrupt handler recorded each press perfectly and the poll read the release that
/// had already overwritten it.
///
/// 300 ms, which clears two of `diag`'s polls and is still shorter than a deliberate human press.
/// It bounds only the *release*: the press is delivered immediately, so nothing feels slower, and
/// RetailOS — which polls fast enough that this never mattered — sees the same press it always
/// did, held a little longer.
///
/// **The unit is instructions**, and `CLOCK` is instructions per simulated *microsecond*, so a
/// duration in milliseconds is `ms * 1_000 * CLOCK`. The first version of this divided by 1 000
/// instead of multiplying and produced a 0.3 ms hold — and the test beside it computed `diag`'s
/// poll rate with the same mistake, so the two agreed and the assertion passed. Hence
/// [`instructions_for_ms`], which both now use, and which is checked against a figure worked out
/// by hand.
const MIN_BUTTON_HOLD: u64 = instructions_for_ms(300);

/// Instructions in `ms` milliseconds of simulated time.
///
/// `CLOCK` is instructions per simulated microsecond — the definition of `--clock` — so this is
/// `ms * 1_000 * CLOCK`. At the real part's 75 that is 75 000 instructions per millisecond.
pub const fn instructions_for_ms(ms: u64) -> u64 {
    ms * 1_000 * eapp_loader::CLOCK as u64
}

/// When an event may fire, given what is already scheduled and the earliest slot free.
///
/// Everything fires at `earliest` except a button's **release**, which waits until the button has
/// been held for [`MIN_BUTTON_HOLD`]. The hold is measured from the button's own down event in the
/// script, so the two cannot disagree the way a separate ledger could.
fn schedule_at(script: &[WheelStep], ev: eapp_loader::WheelEvent, earliest: u64) -> u64 {
    let eapp_loader::WheelEvent::Button(mask, false) = ev else {
        return earliest;
    };
    match script
        .iter()
        .rev()
        .find(|s| matches!(s.event, eapp_loader::WheelEvent::Button(m2, true) if m2 == mask))
    {
        Some(down) => earliest.max(down.at + MIN_BUTTON_HOLD),
        None => earliest,
    }
}

fn drain(m: &mut Machine, inbox: &Mutex<Inbox>, next_at: &mut u64, gap: u64) {
    let now = m.executed as u64;
    let Some(w) = m.mem.clickwheel.as_mut() else {
        return;
    };
    let mut inbox = inbox.lock().unwrap();
    while let Some(&ev) = inbox.events.front() {
        let at = schedule_at(&w.script, ev, (*next_at).max(now));
        // Only schedule what will fire soon. Letting the whole queue onto the script at once is
        // harmless for the model but makes the depth invisible, and the depth is what tells a
        // person their drag is outrunning the emulator.
        //
        // A held button is exempt: its release is deliberately far in the future, and deferring it
        // here would stall every event behind it — including the release itself, for ever.
        if at > now + gap * 8 && !matches!(ev, eapp_loader::WheelEvent::Button(_, false)) {
            break;
        }
        inbox.events.pop_front();
        // The window always anchors in instructions: `next_at` and `MIN_BUTTON_HOLD` are both
        // instruction counts, and mixing a clock in here would make the hold mean two things.
        w.script.push(WheelStep::instr(at, ev));
        *next_at = at.max(*next_at) + gap;
    }
}

fn collect(m: &Machine, started: Instant, base: (u64, u32)) -> Stats {
    let w = m.mem.clickwheel.as_ref();
    let mut s = Stats {
        executed: m.executed as u64,
        sim_usec: m.mem.usec,
        wall_secs: started.elapsed().as_secs_f64(),
        executed_here: m.executed as u64 - base.0,
        sim_usec_here: m.mem.usec.wrapping_sub(base.1) as u64,
        idle_steps: m.idle_steps,
        ..Stats::default()
    };
    if let Some(w) = w {
        s.hold = w.hold;
        s.touched = w.touched;
        s.position = w.position;
        s.buttons = w.buttons;
        s.reporting = w.reporting;
        s.asked_for_frames = w.set_commands > 0;
        s.wheel_sets = w.set_commands;
        s.last_wheel_set = w.last_set;
        s.frames_posted = w.frames_posted;
        s.frames_dropped = w.frames_dropped;
        s.frames_suppressed = w.frames_suppressed;
        s.data_reads = w.data_reads;
        s.data_reads_ready = w.data_reads_ready;
        s.irqs = w.irqs;
    }
    // The drive's own census, and `seen()` rather than `sample().len()` for the reason the field
    // carries: the log holds 256 rows and a retail cold boot issues about seven hundred.
    if let Some((_, d)) = &m.mem.ata {
        s.ata_commands = d.commands.seen();
    }
    for (i, (pc, _)) in WATCHED.iter().enumerate() {
        s.enters[i] = m.enter_log.iter().filter(|e| e.0 == *pc).count() as u64;
    }
    if let Some(b) = &m.mem.bcm {
        s.bcm_frames = b.frames;
        s.bcm_commands = b.commands.len();
    }
    s
}

/// **What the log says when a machine dies, and what it says when the same one dies again.**
///
/// A window run leaves a log, and the log is the part a person keeps. The one this type came out
/// of held **twenty-five identical high-level boots and not one word about why any of them
/// ended** — because [`build`] prints two lines per machine and a machine that stops printed none.
/// Read back afterwards that is indistinguishable from a program restarting itself in a loop, and
/// it was read that way: the same failure, twenty-five times, filed as a retry loop.
///
/// **There is no retry loop, and that is the fact this replaces the guess with.** `session` parks
/// on `wait_after_stop` when a machine stops and only a `Cmd::PowerOff` / `PowerOn` / `PowerCycle`
/// / `Boot` moves it, every one of which is queued from one place — `on_start_device`, reached from
/// `pressed-centre` and from the Devices page's `activated`. Both are people. Twenty-five boots is
/// twenty-five presses, which is exactly what somebody does when a press appears to do nothing:
/// each one died in about a third of a second and left the bench saying the same thing.
///
/// So the run is not bounded here — a person may start an iPod as often as they like — but the
/// *log* now says what happened each time, and says when it has said it before.
#[derive(Default)]
struct Deaths {
    last: Option<String>,
    run: u32,
}

impl Deaths {
    /// The line for this stop. Never empty: a death that printed nothing is the whole defect.
    fn note(&mut self, why: &str) -> String {
        if self.last.as_deref() == Some(why) {
            self.run += 1;
        } else {
            self.last = Some(why.to_string());
            self.run = 1;
        }
        match self.run {
            1 => format!("stopped: {why}"),
            n => format!(
                "stopped: {why} — {n} starts in a row have ended in the same place, so \
                 this is something about the iPod rather than about the start"
            ),
        }
    }
}

/// The emulator thread. Owns the machine outright; the UI never touches it.
///
/// **One iteration of this loop is one power cycle.** Powering off drops the `Machine` — every
/// register, all 64 MB of SDRAM, the co-processor's surface — and powering on builds a new one and
/// enters at the reset vector, which is what makes the GUI's cold boot the same event
/// `retail-boot.sh` performs rather than a restore wearing its name. The drive is the one thing
/// that survives, because it is the one thing that survives a real power cycle.
///
/// Only the **first** session may restore a snapshot or write one. A session reached by powering
/// the machine back on has been asked for a cold boot explicitly; and a snapshot written from it
/// would be taken against a drive the previous session had already written to, which would quietly
/// change what every later restore restores.
pub fn run(cfg: Config, link: Arc<Link>) {
    let mut cfg = cfg;
    let mut first = true;
    // Across sessions, because "again" is a fact about the sequence and a session cannot see one.
    let mut deaths = Deaths::default();
    loop {
        match session(&cfg, &link, first, &mut deaths) {
            Outcome::Quit => return,
            Outcome::ColdBoot => first = false,
            // Changing the boot target is a power cycle, and a session reached by one never
            // restores or writes a snapshot — the machine it would restore is a different program.
            Outcome::ColdBootInto(t) => {
                cfg.boot = t;
                first = false;
            }
            Outcome::PoweredOff => {
                if !wait_for_power(&link) {
                    return;
                }
                first = false;
            }
        }
    }
}

/// Publish a dark panel and block until power comes back. False if the window closed instead.
fn wait_for_power(link: &Arc<Link>) -> bool {
    {
        let mut out = link.out.lock().unwrap();
        out.phase = Phase::Off;
        out.fb.fill(0);
        out.fb_nonzero = 0;
        out.fb_seq += 1;
        // Not merely stale but *meaningless*: there is no machine to have a wheel position or an
        // instruction count. Zeroed rather than frozen, so the panel cannot be read as a running
        // machine that has stopped moving.
        out.stats = Stats::default();
    }
    loop {
        if link.quit.load(Ordering::Relaxed) {
            return false;
        }
        let cmd = link.inbox.lock().unwrap().cmds.pop_front();
        match cmd {
            Some(Cmd::PowerOn) | Some(Cmd::PowerCycle) | Some(Cmd::Boot(_)) => return true,
            // Already off. A queued power-off is not an error, and not a second one either.
            Some(Cmd::PowerOff) | None => {}
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// One power cycle: build the machine, run it, and stop when the window closes or power is cut.
fn session(cfg: &Config, link: &Arc<Link>, first: bool, deaths: &mut Deaths) -> Outcome {
    let mut m = match build(cfg, first) {
        Ok(m) => m,
        Err(e) => {
            // Said, not merely shown. The window puts this on the bench; a machine that could not
            // be built and printed nothing leaves a log with a boot line missing and no reason.
            println!("{}", deaths.note(&e));
            link.out.lock().unwrap().phase = Phase::Stopped(e);
            return Outcome::Quit;
        }
    };
    // Buttons pressed at the old machine were pressed at a machine that no longer exists.
    {
        let mut inbox = link.inbox.lock().unwrap();
        inbox.events.clear();
    }

    // Restore, if the complete pair is there and a cold boot was not demanded. This asks the same
    // question `build` asked when it chose which drive to clone, through the same method, because
    // the two answering differently is precisely the failure being fixed.
    let mut restored = false;
    if cfg.may_restore(first) {
        if let Some(path) = &cfg.snapshot {
            if let Ok(b) = std::fs::read(path) {
                if m.restore(&b) {
                    restored = true;
                } else {
                    // Two things are wrong at once here, and patching either alone leaves the
                    // other. The drive under this machine was cloned from the frozen one, on the
                    // strength of the snapshot file existing, so it is part-way through somebody
                    // else's boot — cold-booting from it is the stale pair with the halves swapped.
                    // And `restore` can reject a snapshot *after* loading part of the CPU from it,
                    // so the machine itself is no longer trustworthy either.
                    //
                    // So build a new one. `cold` forces the pristine drive through the same path
                    // that chose the frozen one, which is the point of routing both through
                    // `may_restore`: there is no second place that has to be kept in step.
                    eprintln!("{}: not a valid snapshot; cold-booting", path.display());
                    let cold = Config {
                        cold: true,
                        ..cfg.clone()
                    };
                    match build(&cold, first) {
                        Ok(fresh) => m = fresh,
                        Err(e) => {
                            println!("{}", deaths.note(&e));
                            link.out.lock().unwrap().phase = Phase::Stopped(e);
                            return Outcome::Quit;
                        }
                    }
                }
            }
        }
    }
    // `--clock-v3`: put the restored machine back into the state the old snapshot format left it
    // in. The ablation is applied *here*, after a restore that carried the field correctly, so the
    // two arms of the experiment share one snapshot file and differ in one assignment.
    if restored && cfg.clock_v3 {
        m.mem.slept_usec = 0;
        eprintln!("--clock-v3: the sleep accumulator has been zeroed — this run reproduces the pre-2026-08-14 snapshot format");
    }
    // Does the restored clock survive the next instruction? Checked as the exact identity the run
    // loop maintains — `usec == executed / instr_per_usec + slept_usec` — rather than by watching
    // for a suspicious jump, because the jump is *not* recognisable by size or by sign. The one
    // this project shipped for weeks reads, in the u32 arithmetic firmware actually does, as
    // 1 580 473 981 µs of ordinary forward progress. Only the identity tells the two apart.
    if restored {
        let want = (m.executed / m.instr_per_usec.max(1)) as u32 + m.mem.slept_usec;
        if want == m.mem.usec {
            eprintln!(
                "restore: the simulated clock round-trips — {} µs, and `executed / {} + slept_usec` \
                 agrees, so the next instruction will not move it.",
                m.mem.usec, m.instr_per_usec
            );
        } else {
            eprintln!(
                "restore: THE SIMULATED CLOCK DOES NOT ROUND-TRIP — the snapshot says {} µs and the \
                 next instruction will recompute it as {} µs, a step of {} µs. `slept_usec` is not \
                 being carried.",
                m.mem.usec,
                want,
                want as i64 - m.mem.usec as i64
            );
        }
    }
    // `restore` rebuilds the co-processor from the image and `Bcm::new` defaults `registry` to
    // false — it is not one of the four scalars a snapshot carries. Re-assert it, or a restored
    // machine would answer no service lookups and the panel would go dark the moment RetailOS asked
    // one. This is also why the snapshot has to have been *taken* with the registry on: see
    // `snapshot_path`, which keys the cache on that.
    if let Some(b) = m.mem.bcm.as_mut() {
        b.registry = true;
    }

    // What a park would cost, published before anything can ask for one — see `Link::snapshot_bytes`.
    link.snapshot_bytes
        .store(snapshot_bytes(&m), Ordering::Relaxed);

    {
        let mut out = link.out.lock().unwrap();
        out.phase = if restored {
            Phase::Running
        } else {
            Phase::Booting {
                target: cfg.snap_at,
            }
        };
        // Per session, not per process. A machine reached by a power cycle cold-boots again, and
        // what the *previous* session's boot cost is not a measurement of this one — the window
        // records the number once per boot and this is what stops the second one arriving with the
        // first one's answer already in it.
        out.booted_at = None;
    }

    let started = Instant::now();
    let usec_from_snapshot = m.mem.usec;
    // The origin the "this session" figures are measured from, taken after the first slice.
    //
    // The clock is recomputed every instruction as `executed / instr_per_usec + slept_usec`, so it
    // round-trips a snapshot only if the snapshot carries `slept_usec` — and until 2026-08-14 it did
    // not, which put a 2.62-billion-µs step backwards into the first instruction after every
    // restore. It is carried now (`Machine::snapshot`, format `IPODSNP4`), and the check below is
    // kept, and kept *loud*, because the failure was silent for as long as it existed: this is the
    // one place that can see it, and a regression here would look exactly like a machine behaving
    // oddly for reasons nobody could name. `--clock-v3` reproduces the old behaviour on purpose.
    let mut base: Option<(u64, u32)> = None;
    let mut next_at = m.executed as u64;
    let mut last_fb = Instant::now();
    let mut fb = vec![0u8; FB_W * FB_H * 3];
    let mut fb_seq = 0u64;
    // The two surfaces' first observed pixel counts, and whether either has moved since.
    let (mut seen_shown, mut seen_other) = (None::<u32>, None::<u32>);
    let (mut shown_moved, mut other_moved) = (false, false);
    let mut want_snapshot = first && !restored && cfg.snapshot.is_some();
    let mut entered = restored;
    let mut test = SelfTest {
        shots: cfg.shots.clone(),
        ..SelfTest::default()
    };
    let mut probe = Probing::new(cfg);
    // `None` means "not yet sampled", which is distinct from an unmapped address.
    let mut watched: Vec<Option<u32>> = vec![None; cfg.watch.len()];
    let mut last_executed = 0u64;
    let mut last_moved = std::time::Instant::now();
    // The trailing window that ends the boot phase. Per session, because a boot ends once and a
    // power cycle is a new one — `run`'s outer loop builds a fresh `Machine` and gets a fresh one
    // of these with it.
    let mut quiet = Quiet::default();

    loop {
        if link.quit.load(Ordering::Relaxed) {
            // Park it, if the window asked and there is a machine worth parking. `Phase::Running`
            // is the test rather than any instruction count: it is set by the restore and by the
            // end of the cold boot, which are exactly the two ways to arrive at a machine that has
            // finished starting. A restore point taken mid-boot would resume into a window that
            // says "running" over an iPod still drawing its logo.
            let running = matches!(link.out.lock().unwrap().phase, Phase::Running);
            if running && link.save_on_quit.load(Ordering::Relaxed) {
                link.saving.store(true, Ordering::Relaxed);
                // The frame the machine stops on is the one it last published, which is what
                // `fb_seq` moving off zero means — see `write_parked_frame`, and `Link::parked`
                // for why the answer is an atomic rather than another field of `Out`.
                let at = write_restore_point(cfg, &m, (fb_seq > 0).then_some(&fb[..]));
                link.parked.store(at.unwrap_or(0), Ordering::Relaxed);
                link.saving.store(false, Ordering::Relaxed);
            }
            break;
        }
        // Power commands are read before input, so a click queued behind a power-off is discarded
        // with the machine rather than delivered to its successor.
        if let Some(c) = link.inbox.lock().unwrap().cmds.pop_front() {
            match c {
                Cmd::PowerOff => return Outcome::PoweredOff,
                Cmd::PowerCycle => return Outcome::ColdBoot,
                Cmd::Boot(t) => return Outcome::ColdBootInto(t),
                Cmd::PowerOn => {}
            }
        }
        drain(&mut m, &link.inbox, &mut next_at, cfg.click_gap);

        // A cold boot enters at 0, where the CPU fetches out of reset, with `r0`-`r3` zeroed and
        // `lr` at the sentinel — exactly `trace.rs`'s `call_with(entry, &[0,0,0,0], …)`. A restored
        // machine is already somewhere and is simply resumed, which is `trace.rs`'s `m.run(…)`.
        let stop = if entered {
            m.run(SLICE)
        } else {
            entered = true;
            // **Where `build` left the PC**, and it is read rather than re-decided. A cold boot off
            // a real dump leaves it at 0, where the CPU fetches out of reset; a high-level boot
            // leaves it at the entry the bootloader would have jumped to. This line used to be its
            // own `if cfg.boot.is_os() { 0 } else { 0x1000_0000 }`, which is the same decision
            // taken twice in two files — and the OS's arm of it was the wrong answer.
            let pc = m.cpu.regs[15];
            // The coprocessor comes out of reset running the same code and decides for itself, on
            // `PROC_ID`, that it is not the CPU — Apple's bootloader branches on it at 0x8738 and
            // parks it three instructions later.
            if cfg.second_core {
                m.cop.regs[15] = pc;
            }
            m.call_with(pc, &[0, 0, 0, 0], SLICE)
        };
        let executed = m.executed as u64;

        // The scripted power cycle, in the first session only. Announced with the machine's own
        // numbers so the second session's are visibly a different machine's.
        if first {
            if let Some(at) = cfg.power_cycle_at {
                if executed >= at {
                    println!(
                        "power cycle: cutting power at {executed} instructions, {} µs simulated, \
                         {} non-black on the panel. The machine is dropped here.",
                        m.mem.usec,
                        m.mem
                            .bcm
                            .as_ref()
                            .map(|b| {
                                let mut buf = vec![0u8; FB_W * FB_H * 3];
                                read_framebuffer(b, FB_FRONT, &mut buf)
                            })
                            .unwrap_or(0)
                    );
                    return Outcome::ColdBoot;
                }
            }
        }
        if let Some(limit) = cfg.headless {
            if executed >= limit || stop != Stop::BudgetExhausted {
                report_headless(&m, stop, started, cfg.save_region.as_ref());
                link.quit.store(true, Ordering::Relaxed);
                break;
            }
        }
        if cfg.selftest && !want_snapshot && test.tick(&m, link, started, cfg.selftest_control) {
            link.quit.store(true, Ordering::Relaxed);
            break;
        }
        // Answer an LBA-range question, if one is pending.
        {
            let q = *link.ata_query.lock().unwrap();
            if let Some((from, to)) = q {
                let mut hits = 0usize;
                let mut first = Vec::new();
                if let Some((_, ata)) = m.mem.ata.as_ref() {
                    for (cmd, _, n, lba) in ata.commands.sample() {
                        // A command covers `n` sectors from `lba`; overlap, not containment, or a
                        // read that starts just below the range would be missed.
                        let end = lba + (if *n == 0 { 256 } else { *n as u64 });
                        if *lba <= to && end > from {
                            hits += 1;
                            if first.len() < 6 {
                                first.push(format!("cmd={cmd:#04x}@{lba}+{n}"));
                            }
                        }
                    }
                }
                *link.ata_answer.lock().unwrap() = Some(if hits == 0 {
                    format!("ok ata {from}..{to}: NEVER READ")
                } else {
                    format!(
                        "ok ata {from}..{to}: {hits} command(s) [{}]",
                        first.join(" ")
                    )
                });
                *link.ata_query.lock().unwrap() = None;
            }
        }

        // Answer anything the control socket asked for. Bounded per slice so a client that
        // floods the queue cannot stall the emulator.
        {
            let mut req = link.peek_req.lock().unwrap();
            if !req.is_empty() {
                let batch: Vec<u32> = req.drain(..).take(64).collect();
                drop(req);
                let mut ans = link.peek_ans.lock().unwrap();
                {
                    let mut out = link.out.lock().unwrap();
                    out.unmapped_pages = m.mem.unmapped.keys().copied().collect();
                    out.unmapped_pages.sort_unstable();
                }
                for a in batch {
                    let v = if a == crate::control::UNMAPPED_SENTINEL {
                        Some(m.mem.unmapped.len() as u32)
                    } else if a == crate::control::BUS_SENTINEL {
                        let mut out = link.out.lock().unwrap();
                        out.bus_log = m.mem.watch_range_log.drain().into_iter().collect();
                        Some(out.bus_log.len() as u32)
                    } else if a == crate::control::WRITES_SENTINEL {
                        let mut out = link.out.lock().unwrap();
                        out.watched_writes = m
                            .mem
                            .watch_range_words
                            .iter()
                            .map(|(addr, w)| (*addr, w.writes, 0u8))
                            .collect();
                        Some(out.watched_writes.len() as u32)
                    } else if a == crate::control::PMU_SENTINEL {
                        let mut out = link.out.lock().unwrap();
                        out.pmu_written = m
                            .mem
                            .pmu
                            .as_ref()
                            .map(|p| p.written.iter().map(|(r, (n, v))| (*r, *n, *v)).collect())
                            .unwrap_or_default();
                        Some(out.pmu_written.len() as u32)
                    } else if a == crate::control::TRACE_SENTINEL {
                        // Hand the trace over and clear it, so a second dump shows what happened
                        // since the first rather than repeating it.
                        let mut out = link.out.lock().unwrap();
                        out.pc_trace = std::mem::take(&mut m.mem.pc_trace);
                        Some(out.pc_trace.len() as u32)
                    } else {
                        m.mem.peek32(a)
                    };
                    ans.push((a, v));
                }
            }
        }

        // Sampled once per slice, and only reported on change: a value printed every slice would
        // be 40 lines a second saying nothing, and the thing worth seeing is the transition.
        if !cfg.watch.is_empty() {
            for (i, &addr) in cfg.watch.iter().enumerate() {
                let now = m.mem.peek32(addr);
                if watched[i] != now {
                    match now {
                        Some(v) => eprintln!(
                            "watch {addr:#010x} = {v:#010x}  (was {})  at {executed} instructions",
                            watched[i]
                                .map(|w| format!("{w:#010x}"))
                                .unwrap_or_else(|| "unmapped".into())
                        ),
                        None => eprintln!(
                            "watch {addr:#010x} became unmapped at {executed} instructions"
                        ),
                    }
                    watched[i] = now;
                }
            }
        }

        if probe.tick(&mut m, link, cfg) {
            link.quit.store(true, Ordering::Relaxed);
            break;
        }

        // The cold boot's finish line. Written once, then the machine keeps running — the snapshot
        // is a side effect of getting here, not a reason to stop.
        //
        // **Working directly, this instant is the wrong one to write at.** The machine goes on
        // running and goes on writing to the user's own drive, so a restore point taken here
        // describes a drive that has already moved by the time anything could use it: 1.6 GB
        // written to produce a pair that `pair_is_whole` will correctly refuse. Direct mode's
        // restore point is written when the machine stops, where nothing can write next. What this
        // instant still means in both modes is that the cold boot is over, which is the phase
        // change below.
        let asked = link.resnap.swap(false, Ordering::Relaxed);
        let reached_snap_at = want_snapshot && executed >= cfg.snap_at;
        if reached_snap_at || asked {
            want_snapshot = false;
            if cfg.work_on_copy || asked {
                let at = write_restore_point(cfg, &m, (fb_seq > 0).then_some(&fb[..]));
                link.parked.store(at.unwrap_or(0), Ordering::Relaxed);
            }
            link.out.lock().unwrap().phase = Phase::Running;
        }

        if base.is_none() {
            base = Some((executed, m.mem.usec));
            // What one slice actually did to the clock, which is a different number from the
            // identity checked at restore: 250 000 instructions at the idle point advance it by a
            // second or two of simulated time through the idle task's sleeps.
            if restored {
                eprintln!(
                    "restore: {usec_from_snapshot} µs at the restore point, {} µs one slice later.",
                    m.mem.usec
                );
            }
        }
        let stats = collect(&m, started, base.unwrap());
        let refresh = last_fb.elapsed().as_secs_f32() > 1.0 / 60.0;
        let addr = link.out.lock().unwrap().fb_addr;
        let other = if addr == FB_FRONT { FB_BACK } else { FB_FRONT };
        let (nonzero, other_nonzero) = if refresh {
            last_fb = Instant::now();
            fb_seq += 1;
            match m.mem.bcm.as_ref() {
                Some(b) => (
                    read_framebuffer(b, addr, &mut fb),
                    // Counted, not converted: the other surface needs a number, not a picture, and
                    // a second full conversion per frame would cost more than the emulation.
                    b.mem
                        .range(other..other + (FB_W * FB_H * 2) as u32)
                        .filter(|(_, &p)| p != 0)
                        .count() as u32,
                ),
                None => (0, 0),
            }
        } else {
            (0, 0)
        };
        if refresh {
            // "Has it moved at all this session" — the cheapest honest signal that the picture is
            // being drawn somewhere the window is not looking.
            match (seen_shown, seen_other) {
                (None, _) => {
                    seen_shown = Some(nonzero);
                    seen_other = Some(other_nonzero);
                }
                (Some(s), Some(o)) => {
                    if nonzero != s {
                        shown_moved = true;
                    }
                    if other_nonzero != o {
                        other_moved = true;
                    }
                }
                _ => {}
            }
        }

        let mut out = link.out.lock().unwrap();
        let dropped = out.stats.input_dropped;
        out.stats = Stats {
            input_dropped: dropped,
            ..stats
        };
        if refresh {
            out.fb.copy_from_slice(&fb);
            out.fb_nonzero = nonzero;
            out.fb_other_nonzero = other_nonzero;
            out.fb_other_moved = other_moved;
            out.fb_shown_moved = shown_moved;
            out.fb_seq = fb_seq;
        }
        // Outside the `refresh` gate: the level moves without a single pixel changing, and a panel
        // that only redraws when the pixels move would show the old brightness until something else
        // happened to repaint.
        {
            // Measured against the wall, not against the slice count: a slice that executes
            // nothing still goes round.
            let now = std::time::Instant::now();
            if executed == last_executed {
                out.stalled_secs = now.duration_since(last_moved).as_secs_f32();
            } else {
                last_executed = executed;
                last_moved = now;
                out.stalled_secs = 0.0;
            }
        }
        out.backlight = m.mem.backlight.level;
        out.backlight_steps = (m.mem.backlight.steps_up, m.mem.backlight.steps_down);
        // **The boot is over when the machine stops working, not at an instruction count.**
        //
        // This used to flip only at `snap_at`, which is 1.6 G instructions — a point chosen because
        // it is a good place to *resume from*, not because it is where the boot ends. RetailOS
        // reaches the language picker before it, so the bar went on filling over a machine that was
        // already up and taking input, and the "about N s left" beside it counted toward a number
        // nobody could observe. Reported from use, which is the only way this kind of thing is ever
        // found: *"the language screen was long there before it finished."*
        //
        // It was then flipped on RetailOS asking the click wheel for frames, and that reading was
        // **wrong by 387x**: the command it watched is the boot ROM's, written 2.2 M instructions
        // in, and RetailOS never sends one at all. See [`Quiet`], which carries the measurement
        // that replaced it and the one that falsified it.
        //
        // `snap_at` stays as a fallback for the case the machine never settles: a boot that fails
        // before the UI should not leave the window claiming to be booting for ever, and the old
        // behaviour is the honest thing to fall back *to*.
        //
        // **And the two endings are told apart rather than merged**, which is `boot_end`'s whole
        // job: the observed one is what §12.3 divides by next time, the fallback measured nothing,
        // and reading `executed` at this instant without asking which happened files `snap_at` as
        // a measurement. See `Out::booted_at`.
        if out.phase
            == (Phase::Booting {
                target: cfg.snap_at,
            })
        {
            let settled = quiet.read(executed, stats.idle_steps, stats.ata_commands);
            if let Some(measured) = boot_end(settled, executed, cfg.snap_at) {
                out.phase = Phase::Running;
                out.booted_at = measured;
            }
        }
        if stop != Stop::BudgetExhausted && stop != Stop::Idle {
            let why = format!("{stop:?} at {executed} instructions");
            // **On the same stream as the boot line it ends**, so a log reads as boot/death rather
            // than as a list of boots. See [`Deaths`], which is also what makes the second one
            // say it is a second one.
            println!("{}", deaths.note(&why));
            out.phase = Phase::Stopped(why);
            drop(out);
            // A machine that has run off the rails is exactly when someone wants to power-cycle it,
            // so the thread stays alive holding the failure on screen rather than exiting and
            // leaving the power controls inert.
            return wait_after_stop(link);
        }
    }
    Outcome::Quit
}

/// Write both halves of the restore point: RAM, and whatever pairs with the drive in this mode.
///
/// **One function because they are one act.** A snapshot whose companion was written at a
/// different instant describes a drive that had already moved — the stale pair that produced
/// "connect to computer" on every third start. When the companion cannot be written the snapshot
/// is deleted rather than left behind, so half a pair is never on disk to be found later.
///
/// Safe to run while the machine holds the drive open: `Ata` seeks and `write_all`s each sector
/// straight through, keeping no dirty buffer of its own, so what is on disk now is exactly what
/// this RAM believes.
///
/// **`frame` is the third thing on disk and it is not part of the pair** — GUI.md §12.4 and §17 Q7,
/// answered *"do it"*: `<snapshot>.parked.png`, 320 × 240, so a parked device's glass shows the
/// frame it stopped on instead of reading as off. It is written last and its failure is not the
/// pair's failure: a restore point whose picture could not be written still restores, and §12.4's
/// own fallback for a missing PNG — *"the glass is dark"* — is the honest one. `None` means the
/// machine never published a frame, which is a different thing from a black one.
///
/// Returns the Unix second the **complete** pair reached the disk, or `None` — see `Link::parked`.
fn write_restore_point(cfg: &Config, m: &Machine, frame: Option<&[u8]>) -> Option<u64> {
    let path = cfg.snapshot.as_ref()?;
    let img = m.snapshot();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(path, &img) {
        eprintln!("snapshot {}: {e}", path.display());
        return None;
    }
    eprintln!("snapshot -> {} ({} bytes)", path.display(), img.len());
    match cfg.pair_with_drive() {
        Ok(line) => eprintln!("{line}"),
        // Not fatal, and deliberately not silent: without its other half the snapshot must not be
        // restored, so it goes and the next launch cold-boots.
        Err(e) => {
            eprintln!("{e} — this snapshot will not be restored");
            let _ = std::fs::remove_file(path);
            return None;
        }
    }
    write_parked_frame(cfg, frame);
    Some(eapp_loader::settings::now_unix())
}

/// §12.4's parked frame: `<snapshot>.parked.png`, at exactly 320 × 240.
///
/// **`png::encode` gets its first production caller here**, which is what retires that module's
/// dead-code allow — the condition §17 Q7 named. Beside the snapshot and under the same stem, the
/// rule `Config::stamp` already follows for the drive's fingerprint and for the same reason: a
/// hand-given `--snapshot=` brings its own companions rather than pairing with whatever the cache
/// happens to hold.
fn write_parked_frame(cfg: &Config, frame: Option<&[u8]>) {
    let Some(path) = cfg.parked_frame() else { return };
    let Some(rgb) = frame else {
        // Nothing was ever read out of the co-processor, so there is no frame this machine stopped
        // on. A stale PNG from an earlier park would be a picture of a different session, so it
        // goes with the snapshot it no longer belongs to.
        let _ = std::fs::remove_file(&path);
        return;
    };
    if rgb.len() != FB_W * FB_H * 3 {
        eprintln!(
            "parked frame: {} bytes is not {}x{} RGB, so no picture was written",
            rgb.len(),
            FB_W,
            FB_H
        );
        return;
    }
    match std::fs::write(&path, crate::png::encode(rgb, FB_W, FB_H)) {
        Ok(()) => eprintln!("parked frame -> {}", path.display()),
        Err(e) => eprintln!("parked frame {}: {e}", path.display()),
    }
}

/// **What a restore point for this machine would cost**, summed off the format that writes it.
///
/// `Machine::snapshot` walks `mem.regions` and then the co-processor's sparse map as
/// address/value pairs, and those two are the whole of the size — everything else it writes is
/// scalars. So this is the same walk without the allocation, which is what makes it answerable
/// *before* a park rather than by doing one.
///
/// **[`SNAPSHOT_SLACK`] is measured, not guessed** — see
/// `a_park_and_a_restore_are_a_round_trip_and_the_frame_comes_back`, which builds a real machine,
/// takes a real snapshot and asserts this over-estimates it and by how little. An estimate that
/// came out *under* would be a free-space check that passed and a write that filled the volume.
fn snapshot_bytes(m: &Machine) -> u64 {
    let regions: u64 = m
        .mem
        .regions
        .iter()
        .map(|r| (r.data.len() + r.name.len() + 12) as u64)
        .sum();
    let bcm = m.mem.bcm.as_ref().map_or(0, |b| b.mem.len() as u64 * 8);
    regions + bcm + SNAPSHOT_SLACK
}

/// Everything `Machine::snapshot` writes that is not a region or a co-processor pair: the register
/// file, the clock, the alias table, the MMAP window registers, the read overrides, the drive's
/// saved state, the click wheel and the backlight.
const SNAPSHOT_SLACK: u64 = 256 * 1024;

/// The machine stopped on its own. Keep the reason on screen and wait for a power command.
fn wait_after_stop(link: &Arc<Link>) -> Outcome {
    loop {
        if link.quit.load(Ordering::Relaxed) {
            return Outcome::Quit;
        }
        match link.inbox.lock().unwrap().cmds.pop_front() {
            Some(Cmd::PowerOff) => return Outcome::PoweredOff,
            Some(Cmd::Boot(t)) => return Outcome::ColdBootInto(t),
            Some(Cmd::PowerOn) | Some(Cmd::PowerCycle) => return Outcome::ColdBoot,
            None => {}
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// `--selftest`: drive a gesture through the GUI's own input path and print what reached RetailOS.
///
/// This is the measurement the GUI has to be able to make, run without a window so it is a number
/// in a log rather than an impression from a screenshot. It pushes through [`Link::push`] — the
/// same call a mouse drag makes — so what it exercises is the whole chain the window uses: the
/// inbox, the gap-spaced drain, the appended script, `service_clickwheel`, the posted frame, IRQ 40
/// and Apple's ISR. The comparison is research/10 Addendum 21 §6, which measured the same counts
/// from a `--wheel` script: 36 clicks give 36 arrivals at the decoder, 32 at the scroll
/// accumulator, 15 wheel events and 4 button events.
///
/// Three stages, because the middle one is not optional: after a restore the wheel's `reporting`
/// gate is closed (a snapshot does not carry the click wheel), so the test **waits for RetailOS's
/// own `0x8001052a`** rather than forcing the flag. Forcing it would be a bypass, and it would hide
/// exactly the thing a user of the window will hit — that input in the first second is refused.
#[derive(Default)]
struct SelfTest {
    stage: u8,
    /// The instruction count each stage is waiting for.
    until: u64,
    gate_at: u64,
    /// When this test first looked for the gate, so waiting for it can be bounded.
    start: Option<u64>,
    /// The panel before the gesture, and at two points after it. Reaching Apple's decoder is what
    /// the brief asks for, but the *screen changing* is the thing a person actually wants to know
    /// happened, and it is one `read_framebuffer` to say so instead of implying it.
    panel: Vec<(&'static str, u64, u32, u64)>,
    /// Which arm this is, for naming the PNGs, and where they go.
    arm: &'static str,
    shots: PathBuf,
}

impl SelfTest {
    /// A cheap content digest of the surface, plus its non-black count — and the picture itself,
    /// written to `_out/`.
    ///
    /// The digest is what the control arm is compared on; the PNG is what makes "the panel changed"
    /// checkable by somebody who was not here. FNV over the bytes: it only has to distinguish "the
    /// same picture" from "a different picture", and a hash that told them apart wrongly would show
    /// up immediately as a control arm that also "changed".
    fn sample(m: &Machine, tag: &str, shots: &Path) -> (u32, u64) {
        let Some(b) = &m.mem.bcm else { return (0, 0) };
        let mut buf = vec![0u8; FB_W * FB_H * 3];
        let n = read_framebuffer(b, FB_FRONT, &mut buf);
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for x in &buf {
            h ^= *x as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        let _ = std::fs::create_dir_all(shots);
        let name = tag.replace(' ', "-");
        let _ = std::fs::write(
            shots.join(format!("selftest-{name}.png")),
            crate::png::encode(&buf, FB_W, FB_H),
        );
        (n, h)
    }

    /// Returns true when the test is finished and the run should stop.
    fn tick(&mut self, m: &Machine, link: &Arc<Link>, started: Instant, control: bool) -> bool {
        let now = m.executed as u64;
        // Whether RetailOS has ASKED, not whether the stream is on: reporting is on at reset now,
        // so the flag no longer marks the moment the firmware is ready for input.
        let reporting = m
            .mem
            .clickwheel
            .as_ref()
            .is_some_and(|w| w.set_commands > 0);
        match self.stage {
            0 => {
                // Wait for the firmware to open the gate itself.
                if !reporting {
                    // But not forever. On a **restored** machine RetailOS re-sends `0x8001052a`
                    // only when its power state changes, and on the current model — a bare iPod
                    // sitting on the first-run Language list — nothing changes: 0 commands in 500 M
                    // instructions, measured. This test used to wait for that gate with no bound,
                    // on a machine where it had always opened within 3.5 M instructions, and it
                    // would now hang and look like a slow run. See research/10 Addendum 31 §3.
                    if self.start.is_none() {
                        self.start = Some(now);
                    }
                    if now - self.start.unwrap() > 400_000_000 {
                        println!(
                            "selftest: ABANDONED — the wheel's `0x052a` reporting gate never opened \
                             in 400 M instructions, so no input could have been delivered. A \
                             snapshot does not carry the click wheel, and on this machine nothing \
                             re-arms it."
                        );
                        return true;
                    }
                    return false;
                }
                self.gate_at = now;
                println!(
                    "selftest [{}]: reporting enabled by the firmware at {now} instructions",
                    if control {
                        "CONTROL — no input"
                    } else {
                        "driving the wheel"
                    }
                );
                self.arm = if control { "control" } else { "driven" };
                let arm = self.arm;
                let (n, h) = Self::sample(m, &format!("{arm}-before"), &self.shots);
                self.panel.push(("before the gesture", now, n, h));
                // One touch, thirty-six clicks clockwise, one release; then a Select press and
                // release. Thirty-six is Addendum 21 arm B's script, so the counts below are
                // directly comparable to the ones it published.
                if !control {
                    link.push(WheelEvent::Touch);
                    for _ in 0..36 {
                        link.push(WheelEvent::Step(1));
                    }
                    link.push(WheelEvent::Release);
                    link.push(WheelEvent::Button(eapp_loader::WHEEL_SELECT, true));
                    link.push(WheelEvent::Button(eapp_loader::WHEEL_SELECT, false));
                }
                self.stage = 1;
                // 40 events at 20 000 instructions apart is 800 k; give the firmware ten times
                // that to act on the last of them.
                self.until = now + 8_000_000;
                false
            }
            1 => {
                if now < self.until {
                    return false;
                }
                let (n, h) = Self::sample(m, &format!("{}-plus8M", self.arm), &self.shots);
                self.panel.push(("8 M after", now, n, h));
                self.stage = 2;
                self.until = self.gate_at + 60_000_000;
                false
            }
            2 => {
                if now < self.until {
                    return false;
                }
                let (n, h) = Self::sample(m, &format!("{}-plus60M", self.arm), &self.shots);
                self.panel.push(("60 M after", now, n, h));
                report_selftest(m, link, started, self.gate_at, &self.panel);
                true
            }
            _ => true,
        }
    }
}

fn report_selftest(
    m: &Machine,
    link: &Arc<Link>,
    started: Instant,
    gate_at: u64,
    panel: &[(&'static str, u64, u32, u64)],
) {
    let s = collect(m, started, (gate_at, 0));
    println!(
        "selftest: {} instructions after the gate opened",
        m.executed as u64 - gate_at
    );
    println!(
        "  wheel: position {}, {} frames posted, {} dropped, {} suppressed",
        s.position, s.frames_posted, s.frames_dropped, s.frames_suppressed
    );
    println!(
        "  DATA reads {} ({} with a frame waiting), IRQ 40 asserted {} times",
        s.data_reads, s.data_reads_ready, s.irqs
    );
    println!(
        "  queue drained: {} left, {} dropped",
        link.inbox.lock().unwrap().events.len(),
        s.input_dropped
    );
    println!("  arrivals in RetailOS:");
    for (i, (pc, name)) in WATCHED.iter().enumerate() {
        println!("    {pc:#010x}  {:<24} {}", name, s.enters[i]);
    }
    println!("  the panel at 0x000e0000:");
    let first = panel.first().map(|p| p.3);
    for (when, at, n, h) in panel {
        let changed = match first {
            Some(f) if *h != f => "  CHANGED",
            _ => "",
        };
        println!("    {when:<20} @{at:<12} {n} non-black, digest {h:016x}{changed}");
    }
}

// ---------------------------------------------------------------- the probes

/// How long the button combo is held, in instructions. At `--clock=5` and with the idle task's
/// sleeps on top, 400 M instructions is well over a minute of simulated time — the hardware combo
/// wants six to ten seconds, so a machine that ignores this held it for at least six times as long
/// as it would ever need.
const COMBO_HOLD: u64 = 400_000_000;

/// Where the panel is sampled after the probe acts, in instructions past the anchor. `--samples=`
/// replaces this, which is how an interval that falls between two of these is narrowed without
/// rebuilding.
const PROBE_SAMPLES: [u64; 6] = [
    8_000_000,
    40_000_000,
    100_000_000,
    200_000_000,
    400_000_000,
    800_000_000,
];

/// A scripted measurement with no window: act at a fixed instruction anchor, then watch the panel.
///
/// The anchor is the whole point. `SelfTest` waits for the wheel's reporting gate, and that is a
/// different instruction count in a cold machine (RetailOS's opto init, early in the boot, long
/// before there is a UI) than in a restored one (a few million instructions past a 1.6 G idle) — so
/// its two arms were never delivering input at the same moment in the boot, and "restored and cold
/// answer the same input differently" was measured across that confound. This one presses at
/// `cfg.probe_at` in both arms, which is the anchor research/10 Addendum 30's `--wheel=@1500M:…`
/// script uses.
struct Probing {
    mode: Option<Probe>,
    stage: u8,
    act_at: u64,
    next: usize,
    /// `(label, instructions, simulated µs, front non-black, front digest, back non-black, back
    /// digest)`.
    panel: Vec<(String, u64, u32, u32, u64, u32, u64)>,
    shots: PathBuf,
    arm: &'static str,
    combo_down: bool,
    combo_up: bool,
    samples: Vec<u64>,
}

impl Probing {
    fn new(cfg: &Config) -> Self {
        Probing {
            mode: cfg.probe,
            stage: 0,
            act_at: 0,
            next: 0,
            panel: Vec::new(),
            combo_down: false,
            combo_up: false,
            samples: if cfg.samples.is_empty() {
                PROBE_SAMPLES.to_vec()
            } else {
                cfg.samples.clone()
            },
            shots: cfg.shots.clone(),
            arm: match cfg.probe {
                Some(Probe::Menu) => "menu",
                Some(Probe::MenuControl) => "menu-control",
                Some(Probe::Combo) => "combo",
                Some(Probe::ComboControl) => "combo-control",
                None => "",
            },
        }
    }

    fn sample(&mut self, m: &Machine, label: &str) {
        let Some(b) = &m.mem.bcm else { return };
        let mut buf = vec![0u8; FB_W * FB_H * 3];
        let n = read_framebuffer(b, FB_FRONT, &mut buf);
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for x in &buf {
            h ^= *x as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        let _ = std::fs::create_dir_all(&self.shots);
        let _ = std::fs::write(
            self.shots.join(format!("probe-{}-{label}.png", self.arm)),
            crate::png::encode(&buf, FB_W, FB_H),
        );
        // The **back** buffer as well, because "RetailOS did not draw" and "RetailOS drew somewhere
        // this window is not looking" are different findings and the front surface alone cannot
        // tell them apart. `halfwords_written` says pixels moved; this says where they landed.
        let back = read_framebuffer(b, FB_BACK, &mut buf);
        let mut hb: u64 = 0xcbf2_9ce4_8422_2325;
        for x in &buf {
            hb ^= *x as u64;
            hb = hb.wrapping_mul(0x100_0000_01b3);
        }
        let _ = std::fs::write(
            self.shots
                .join(format!("probe-{}-{label}-back.png", self.arm)),
            crate::png::encode(&buf, FB_W, FB_H),
        );
        self.panel.push((
            label.to_string(),
            m.executed as u64,
            m.mem.usec,
            n,
            h,
            back,
            hb,
        ));
    }

    /// Returns true when the probe is finished and the run should stop.
    fn tick(&mut self, m: &mut Machine, link: &Arc<Link>, cfg: &Config) -> bool {
        let Some(mode) = self.mode else { return false };
        let now = m.executed as u64;
        let reporting = m.mem.clickwheel.as_ref().is_some_and(|w| w.reporting);
        match self.stage {
            // Wait for the anchor AND for the firmware to have opened the wheel's gate itself.
            // Forcing the gate would be a bypass; waiting past the anchor is honest and is reported.
            0 => {
                // Only an arm that pushes input needs the wheel armed. `MenuControl` pushes none,
                // and gating it on the same flag would make the one arm that can always run wait
                // for a condition it does not use.
                let needs_gate = !matches!(mode, Probe::MenuControl);
                // A probe that waits forever is an instrument that reports nothing and looks like a
                // slow run. If the firmware has not opened the wheel's reporting gate within 400 M
                // instructions of the anchor it is not going to, and *that* is the measurement.
                if needs_gate && now > cfg.probe_at + 400_000_000 && !reporting {
                    let w = m.mem.clickwheel.as_ref();
                    println!(
                        "probe [{}]: ABANDONED at {now} — the wheel's `0x052a` reporting gate never \
                         opened within 400 M instructions of the anchor {}. {} `0x052a` commands \
                         seen, last {:?}. Every event would have been suppressed, so no input was \
                         pushed.",
                        self.arm,
                        cfg.probe_at,
                        w.map(|w| w.set_commands).unwrap_or(0),
                        w.and_then(|w| w.last_set),
                    );
                    self.sample(m, "gate-shut");
                    self.report(m, link);
                    return true;
                }
                if now < cfg.probe_at || (needs_gate && !reporting) {
                    return false;
                }
                self.act_at = now;
                println!(
                    "probe [{}]: acting at {now} instructions, {} µs simulated (anchor {}, gate {})",
                    self.arm,
                    m.mem.usec,
                    cfg.probe_at,
                    if reporting { "open" } else { "shut" }
                );
                if cfg.ablate_pmu {
                    let reads = m.mem.pmu.as_ref().map(|p| p.reads).unwrap_or(0);
                    m.mem.pmu = Some(Pcf50605::new());
                    println!(
                        "probe [{}]: --ablate=pmu — the PCF50605 has been replaced with a \
                         factory-fresh one at {now}, discarding the state {reads} reads' worth of \
                         traffic had put in it. This is the state a RESTORED machine runs with.",
                        self.arm
                    );
                }
                self.sample(m, "before");
                // Every arm but the control presses Select once. From the first-run Language list
                // that is the main menu — research/10 Addendum 30 §6, same event, same anchor — and
                // it is what puts the combo arms in front of the same screen as the menu arms.
                if mode != Probe::MenuControl {
                    link.push(WheelEvent::Touch);
                    link.push(WheelEvent::Button(eapp_loader::WHEEL_SELECT, true));
                    link.push(WheelEvent::Button(eapp_loader::WHEEL_SELECT, false));
                    link.push(WheelEvent::Release);
                }
                self.stage = 1;
                false
            }
            1 => {
                // The combo, once the menu has had 40 M instructions to come up. MENU and SELECT go
                // down as two separate events rather than one mask with both bits set: that is what
                // a thumb does, and `ClickWheel::buttons` ORs them into the same frame either way,
                // so the wheel reports the pair held exactly as the hardware would.
                let combo_at = self.act_at + 40_000_000;
                if matches!(mode, Probe::Combo | Probe::ComboControl) {
                    if now >= combo_at && !self.combo_down {
                        self.combo_down = true;
                        if mode == Probe::Combo {
                            link.push(WheelEvent::Button(eapp_loader::WHEEL_MENU, true));
                        }
                        // The control holds SELECT alone, so "the firmware saw a held button" and
                        // "the firmware saw *that pair*" are different arms rather than one claim.
                        link.push(WheelEvent::Button(eapp_loader::WHEEL_SELECT, true));
                        self.sample(m, "combo-down");
                    }
                    if self.combo_down && !self.combo_up && now >= combo_at + COMBO_HOLD {
                        self.combo_up = true;
                        link.push(WheelEvent::Button(eapp_loader::WHEEL_MENU, false));
                        link.push(WheelEvent::Button(eapp_loader::WHEEL_SELECT, false));
                        self.sample(m, "combo-up");
                    }
                }
                if self.next >= self.samples.len() {
                    self.report(m, link);
                    return true;
                }
                if now < self.act_at + self.samples[self.next] {
                    return false;
                }
                let label = format!("plus{}M", self.samples[self.next] / 1_000_000);
                self.sample(m, &label);
                self.next += 1;
                false
            }
            _ => true,
        }
    }

    fn report(&self, m: &Machine, link: &Arc<Link>) {
        let s = collect(m, Instant::now(), (self.act_at, 0));
        println!(
            "probe [{}]: {} instructions after acting",
            self.arm,
            m.executed as u64 - self.act_at
        );
        println!(
            "  wheel: buttons {:#07b}, {} frames posted, {} dropped, {} suppressed",
            s.buttons, s.frames_posted, s.frames_dropped, s.frames_suppressed
        );
        println!(
            "  queue: {} left, {} dropped",
            link.inbox.lock().unwrap().events.len(),
            s.input_dropped
        );
        // The output stage, counted **in this session**: `Machine::restore` builds a fresh `Bcm`, so
        // on a restored run these start at zero even though the surface they filled is on screen.
        // That is exactly what makes them useful here — they say whether the co-processor was asked
        // to present anything *after* the input, which is a different question from whether
        // RetailOS received it.
        println!(
            "  co-processor this session: {} commands kicked, {} frame updates",
            s.bcm_commands, s.bcm_frames
        );
        // Pixels, not commands. The UI is drawn by writing halfwords into the co-processor's
        // internal memory, not by a GENCMD — so `commands`/`frames` cannot tell "RetailOS drew
        // nothing" from "RetailOS drew and did not present". This can.
        if let Some(b) = &m.mem.bcm {
            println!(
                "  co-processor halfwords written this session: {} ({} read)",
                b.halfwords_written, b.halfwords_read
            );
        }
        println!("  arrivals in RetailOS:");
        for (i, (pc, name)) in WATCHED.iter().enumerate() {
            println!("    {pc:#010x}  {:<24} {}", name, s.enters[i]);
        }
        println!("  the panel at 0x000e0000 (front) and 0x00106000 (back), and the clock at each:");
        let first = self.panel.first().map(|p| p.4);
        let first_back = self.panel.first().map(|p| p.6);
        for (when, at, usec, n, h, bn, bh) in &self.panel {
            let changed = match first {
                Some(f) if *h != f => "  FRONT CHANGED",
                _ => "",
            };
            let back_changed = match first_back {
                Some(f) if *bh != f => "  BACK CHANGED",
                _ => "",
            };
            println!(
                "    {when:<12} @{at:<12} {:>10.3} s  front {n:>6} {h:016x}  back {bn:>6} \
                 {bh:016x}{changed}{back_changed}",
                *usec as f64 / 1e6
            );
        }
    }
}

/// The self-check. Prints the three numbers `retail-boot.sh` prints, so "the GUI runs the same
/// machine as the recipe" is a comparison rather than a claim.
fn report_headless(m: &Machine, stop: Stop, started: Instant, save: Option<&(String, PathBuf)>) {
    let secs = started.elapsed().as_secs_f64();
    println!("headless: {stop:?} after {} instructions", m.executed);
    // `commands.seen()`, never `commands.sample().len()`: the log is a `Capped<T>` and its length
    // is a cap wearing a census's clothes. That conflation is research/12's whole subject, and this
    // line was written as `d.command_count` against a field that no longer exists — which is how it
    // was found that this crate had not been compiled since the `Capped<T>` merge.
    println!(
        "  backlight: {} / 32 ({} up, {} down)",
        m.mem.backlight.level, m.mem.backlight.steps_up, m.mem.backlight.steps_down
    );
    // The widths, not just the verdict they produced. `BACKLIGHT_STEP_USEC` is inferred from
    // Rockbox's 10 µs / 200 µs delays, and Apple's firmware is not Rockbox — if its two delays
    // land on the same side of the threshold, every pulse steps the same way and the dimmer walks
    // to a rail. That is invisible from the level and obvious from this line.
    {
        let w = &m.mem.backlight.widths;
        if w.seen() > 0 {
            let rows = w.sample();
            let (lo, hi): (Vec<u32>, Vec<u32>) = rows
                .iter()
                .partition(|&&u| u < eapp_loader::BACKLIGHT_STEP_USEC);
            println!(
                "    pulse widths: {} pulses{} — {} under {} µs, {} over{}",
                w.seen(),
                if rows.len() as u64 == w.seen() {
                    ""
                } else {
                    " (SAMPLE, NOT A CENSUS)"
                },
                lo.len(),
                eapp_loader::BACKLIGHT_STEP_USEC,
                hi.len(),
                if rows.is_empty() {
                    String::new()
                } else {
                    format!(
                        ". min {} µs, max {} µs, first few {:?}",
                        rows.iter().min().unwrap(),
                        rows.iter().max().unwrap(),
                        &rows[..rows.len().min(8)]
                    )
                }
            );
        }
    }
    println!(
        "  ata commands: {}",
        m.mem
            .ata
            .as_ref()
            .map(|(_, d)| d.commands.seen())
            .unwrap_or(0)
    );
    if let Some((_, d)) = &m.mem.ata {
        let names = |c: u8| match c {
            0x20 => "READ SECTORS",
            0x24 => "READ SECTORS EXT",
            0x25 => "READ DMA EXT",
            0xc8 => "READ DMA",
            0x30 => "WRITE SECTORS",
            0x34 => "WRITE SECTORS EXT",
            0x35 => "WRITE DMA EXT",
            0xca => "WRITE DMA",
            0xe7 => "FLUSH CACHE",
            0xea => "FLUSH CACHE EXT",
            0xec => "IDENTIFY",
            0xef => "SET FEATURES",
            0xe0 => "STANDBY IMMEDIATE",
            0xe1 => "IDLE IMMEDIATE",
            _ => "",
        };
        let census: Vec<String> = d
            .cmd_census
            .iter()
            .map(|(c, n)| format!("{c:#04x} {} x{n}", names(*c)))
            .collect();
        if !census.is_empty() {
            println!("    by opcode: {}", census.join(" · "));
        }
    }
    let (reads, writes) = {
        let mut mm = 0u64;
        let mut mw = 0u64;
        for p in m.mem.unmapped.values() {
            mm += p.reads;
            mw += p.writes;
        }
        (mm, mw)
    };
    println!(
        "  unmapped: {reads} reads, {writes} writes across {} pages",
        m.mem.unmapped.len()
    );
    // A total with no address is a number nobody can act on. The window's readout has always named
    // the pages; the headless summary printed the count alone, so the one run you can do over SSH
    // was the one that could not say *what* the firmware reached for. Same report `trace` prints.
    for line in m.mem.unmapped_report() {
        println!("    {line}");
    }
    // Printed unconditionally when a range was asked for, including when it is empty. "Nothing was
    // recorded" and "nothing was asked" must not look the same, because an instrument that stays
    // silent when it found nothing is indistinguishable from one that is not running.
    if let Some((name, path)) = save {
        match m.mem.region_named(name) {
            Some(r) => match std::fs::write(path, &r.data) {
                Ok(()) => println!(
                    "  saved region {name}: {} bytes -> {}",
                    r.data.len(),
                    path.display()
                ),
                Err(e) => println!("  saved region {name}: {e}"),
            },
            None => println!(
                "  no region {name:?}; have {:?}",
                m.mem.regions.iter().map(|r| r.name).collect::<Vec<_>>()
            ),
        }
    }
    if !m.mem.read_sites.is_empty() {
        println!("  reads of the watched addresses:");
        for ((addr, pc), (n, first)) in m.mem.read_sites.iter() {
            println!("    [{addr:#010x}] read by pc {pc:#010x}  x{n}  first @{first}");
        }
    } else if !m.mem.read_addrs.is_empty() {
        println!("  reads of the watched addresses: NONE -- the firmware never looked");
    }
    if !m.mem.input_regs.is_empty() {
        let mut rows: Vec<_> = m.mem.input_regs.iter().filter(|(_, v)| v.0 > 0).collect();
        rows.sort_by_key(|(_, v)| std::cmp::Reverse(v.0));
        println!(
            "  input registers: {} of {} were read before they were ever written",
            rows.len(),
            m.mem.input_regs.len()
        );
        println!("    values the firmware expects hardware to supply, and we invent:");
        for (a, (r, w, pc)) in rows.iter().take(24) {
            println!("      {a:#010x}  {r:>10} reads before write, {w:>8} writes after, first pc {pc:#010x}");
        }
    }
    {
        // (addr, value, pc, width) per the Capped's element type.
        let log = m.mem.watch_range_log.sample();
        if !log.is_empty() {
            println!("  writes into the watched range: {} logged", log.len());
            // Every writer, zeros included. Zeroing a buffer is memset noise and says nothing about
            // provenance, which is what this instrument was built for -- but zero written into a
            // *register* is an instruction, and dropping it is how a control-register census comes
            // back empty.
            let mut pcs: std::collections::BTreeMap<u32, (usize, u32, usize)> = Default::default();
            for (pc, _addr, val, _w) in log.iter() {
                let e = pcs.entry(*pc).or_insert((0, *val, 0));
                e.0 += 1;
                if *val != 0 {
                    e.1 = *val;
                    e.2 += 1;
                }
            }
            // Per WORD as well as per PC. A range census grouped only by writer answers "who"
            // and not "where", and for a block of adjacent device registers -- a GPIO port's four
            // OUTPUT_VALs, say -- "where" is the entire question. Chasing which pin the backlight
            // hangs off took three runs for want of this line.
            if m.mem.watch_range_words.len() > 1 {
                println!("    by word:");
                for (addr, w) in m.mem.watch_range_words.iter() {
                    let who: Vec<String> = w
                        .pcs
                        .iter()
                        .map(|(pc, n)| format!("{pc:#010x} x{n}"))
                        .collect();
                    println!(
                        "      {addr:#010x}  {} byte-writes, first @{}  [{}]",
                        w.writes,
                        w.first_at,
                        who.join(" ")
                    );
                }
            }
            println!("    writers ({} distinct pc):", pcs.len());
            for (pc, (n, sample, nonzero)) in pcs.iter().take(12) {
                let what = if *nonzero == 0 {
                    "all zero".to_string()
                } else {
                    format!("e.g. {sample:#04x}, {nonzero} non-zero")
                };
                println!("      pc {pc:#010x}  x{n}  {what}");
            }
        }
    }
    if !m.mem.regs_seen.is_empty() {
        println!(
            "  registers at {:#010x}:",
            m.mem.regs_at.map(|(a, _)| a).unwrap_or(0)
        );
        for (at, r) in m.mem.regs_seen.iter() {
            println!("    at {at}:");
            for row in 0..4 {
                let cells: Vec<String> = (0..4)
                    .map(|c| {
                        let i = row * 4 + c;
                        format!("r{i:<2}={:#010x}", r[i])
                    })
                    .collect();
                println!("      {}", cells.join("  "));
            }
        }
    }
    if let Some(h) = &m.mem.pc_hist {
        let total: u64 = h.iter().sum();
        println!("  profile: {total} instructions counted");
        let mut idx: Vec<usize> = (0..h.len()).filter(|i| h[*i] > 0).collect();
        idx.sort_by_key(|i| std::cmp::Reverse(h[*i]));
        for i in idx.iter().take(16) {
            let pc = (*i as u32) << 6;
            let pct = h[*i] as f64 * 100.0 / total.max(1) as f64;
            println!("    {pc:#010x}  {:>12}  {pct:5.2}%", h[*i]);
        }
    }
    if m.mem.trace_calls_from.is_some() {
        let n = m.mem.call_trace.len();
        let mut count: std::collections::BTreeMap<(u32, u32), usize> = Default::default();
        let mut first: std::collections::BTreeMap<(u32, u32), u64> = Default::default();
        for (f, to, at) in m.mem.call_trace.iter() {
            *count.entry((*f, *to)).or_default() += 1;
            first.entry((*f, *to)).or_insert(*at);
        }
        println!("  calls: {n} recorded, {} distinct edges", count.len());
        let mut by_count: Vec<_> = count.iter().collect();
        by_count.sort_by(|a, b| b.1.cmp(a.1));
        println!("    busiest:");
        for ((f, to), c) in by_count.iter().take(12) {
            println!("      {f:#010x} -> {to:#010x}  x{c}");
        }
        // An edge taken once per account is the one worth finding, and twelve accounts is the
        // number this drive carries. Widened either side so an off-by-a-few still shows.
        println!("    taken 8..20 times (candidates for once-per-account):");
        let mut per: Vec<_> = count
            .iter()
            .filter(|(_, c)| (8..=20).contains(*c))
            .collect();
        per.sort_by_key(|((f, _), _)| *f);
        for ((f, to), c) in per.iter().take(24) {
            println!(
                "      {f:#010x} -> {to:#010x}  x{c}  first at {}",
                first[&(*f, *to)]
            );
        }
    }
    if let Some((lo, hi)) = m.mem.trace_pc {
        let n = m.mem.pc_trace.len();
        let distinct = m
            .mem
            .pc_trace
            .iter()
            .map(|(pc, _)| *pc)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        println!("  trace {lo:#010x}..{hi:#010x}: {n} executed, {distinct} distinct addresses");
        if let Some((first, at)) = m.mem.pc_trace.first() {
            println!("    first {first:#010x} at {at} instructions");
        }
        if let Some((lastpc, at)) = m.mem.pc_trace.last() {
            println!("    last  {lastpc:#010x} at {at} instructions");
        }
        // How many separate visits: a function entered once and a function entered forty times are
        // different situations, and the entry address is what distinguishes them.
        let entries = m.mem.pc_trace.iter().filter(|(pc, _)| *pc == lo).count();
        println!("    entered {entries} time(s)");
        // The tail is the interesting end. A flattened function is entered and left many times;
        // where it stops going round is where it decided.
        let tail: Vec<String> = m
            .mem
            .pc_trace
            .iter()
            .rev()
            .take(24)
            .rev()
            .map(|(pc, _)| format!("{pc:x}"))
            .collect();
        if !tail.is_empty() {
            println!("    last: {}", tail.join(" "));
        }
        // And the distinct addresses in first-seen order, which for a dispatch table is the state
        // sequence rather than the instruction stream.
        let mut seen = std::collections::BTreeSet::new();
        let order: Vec<String> = m
            .mem
            .pc_trace
            .iter()
            .filter(|(pc, _)| seen.insert(*pc))
            .map(|(pc, _)| format!("{pc:x}"))
            .take(60)
            .collect();
        println!("    order: {}", order.join(" "));
    }
    if let Some(n) = &m.novelty {
        println!("  {} code buckets executed", n.len());
    }
    if let Some(b) = &m.mem.bcm {
        let mut buf = vec![0u8; FB_W * FB_H * 3];
        let n = read_framebuffer(b, FB_FRONT, &mut buf);
        println!(
            "  bcm: {} kicked, {} frame updates",
            b.commands.len(),
            b.frames
        );
        println!(
            "  framebuffer 0x000e0000: {n} non-black pixels of {}",
            FB_W * FB_H
        );
    }
    // The wheel's own gate, because a run where no input could have been delivered looks exactly
    // like a run where input was delivered and ignored.
    if let Some(w) = &m.mem.clickwheel {
        println!(
            "  wheel: reporting {}, {} `0x052a` commands, last {:?}, {} frames posted, {} suppressed",
            if w.reporting { "ON" } else { "OFF" },
            w.set_commands,
            w.last_set,
            w.frames_posted,
            w.frames_suppressed
        );
    }
    println!(
        "  {:.1} s wall, {:.2} M instructions/s",
        secs,
        m.executed as f64 / secs / 1e6
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use eapp_loader::{WheelEvent, WheelStep};

    /// **A press has a duration, and the duration is the machine's, not the operator's.**
    ///
    /// The window used to schedule a release 4 ms of simulated time after its press, which is what
    /// RetailOS's own poll rate made feel right. Apple's `diag` reads its buttons once per 150 ms
    /// and saw none of them. Nothing failed: the interrupt handler recorded every press and the
    /// poll read the release that had overwritten it.
    #[test]
    fn a_button_is_held_long_enough_for_a_polling_reader() {
        // Worked out by hand, so that the conversion is checked against something other than
        // itself: 75 instructions per simulated microsecond is 75 000 per millisecond.
        assert_eq!(
            instructions_for_ms(1),
            75_000,
            "CLOCK is {} — has the part changed?",
            eapp_loader::CLOCK
        );
        assert_eq!(
            MIN_BUTTON_HOLD, 22_500_000,
            "300 ms at 75 000 instructions per ms"
        );
        // Apple's diagnostics polls at 150 ms; the hold has to clear that with room. Asserted in a
        // `const` block so it is the *compiler* that refuses, not this test: both sides are
        // constants, so a regression here should not be able to wait for someone to run the suite.
        const DIAG_POLL: u64 = instructions_for_ms(150);
        const {
            assert!(
                MIN_BUTTON_HOLD > DIAG_POLL,
                "a button hold shorter than Apple's 150 ms diag poll cannot be seen at all"
            )
        };

        let down = WheelStep::instr(1_000, WheelEvent::Button(eapp_loader::WHEEL_MENU, true));
        let script = vec![down];
        let up = WheelEvent::Button(eapp_loader::WHEEL_MENU, false);

        // The release is pushed back to the end of the hold, however soon it was asked for.
        assert_eq!(schedule_at(&script, up, 1_300), 1_000 + MIN_BUTTON_HOLD);
        // And never pulled forward: a release asked for later than that happens later.
        let late = 1_000 + MIN_BUTTON_HOLD * 3;
        assert_eq!(schedule_at(&script, up, late), late);

        // Only the release. A press, a rotation and a touch all fire when asked.
        for ev in [
            WheelEvent::Button(eapp_loader::WHEEL_MENU, true),
            WheelEvent::Step(1),
            WheelEvent::Touch,
            WheelEvent::Release,
        ] {
            assert_eq!(
                schedule_at(&script, ev, 1_300),
                1_300,
                "{ev:?} should not be delayed"
            );
        }

        // A release with no press in the script is not delayed either — there is nothing to hold.
        assert_eq!(schedule_at(&[], up, 1_300), 1_300);
        // And it is *this* button's press that counts, not any press.
        let other = vec![WheelStep::instr(
            1_000,
            WheelEvent::Button(eapp_loader::WHEEL_PLAY, true),
        )];
        assert_eq!(schedule_at(&other, up, 1_300), 1_300);
    }

    /// A snapshot without its frozen drive is **not** restorable — the copy-mode half of the rule.
    ///
    /// This is the migration case, and it is the one that would have re-introduced the bug
    /// inverted: every user upgrading into this change has a snapshot on disk and no frozen drive
    /// beside it. Restoring that RAM onto a freshly cloned pristine drive is the same mismatch with
    /// the halves swapped, so the incomplete pair has to be refused and cold-booted past.
    #[test]
    fn a_snapshot_without_its_drive_is_refused() {
        let dir = std::env::temp_dir().join(format!("ipod-pair-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let snap = dir.join("a.snap");
        let frozen = dir.join("a.frozen");
        std::fs::write(&snap, b"snapshot").unwrap();

        let mut cfg = Config {
            disk: dir.join("d.img"),
            workdisk: dir.join("w.img"),
            frozen: frozen.clone(),
            clock: eapp_loader::CLOCK,
            snapshot: Some(snap.clone()),
            snap_at: 1,
            cold: false,
            work_on_copy: true,
            ..Default::default()
        };

        assert!(
            !cfg.may_restore(true),
            "a snapshot with no frozen drive is half a pair"
        );
        std::fs::write(&frozen, b"drive").unwrap();
        assert!(cfg.may_restore(true), "both halves present");
        assert!(
            !cfg.may_restore(false),
            "a power cycle inside a session never restores"
        );
        cfg.cold = true;
        assert!(!cfg.may_restore(true), "--cold overrides a complete pair");
        cfg.cold = false;
        std::fs::remove_file(&snap).unwrap();
        assert!(
            !cfg.may_restore(true),
            "a frozen drive with no snapshot is the other half"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Working directly, a drive that has moved since the stamp is **not** restorable.
    ///
    /// The same rule as the frozen drive above, asked of the mode that has no frozen drive to ask
    /// about. There the pair is coherent by construction; here it is coherent only while nothing
    /// writes, and the things that write are not all ours — iTunes, `make-disk`, a second window,
    /// and the emulator itself for as long as it runs. Every one of those has to land on "cold
    /// boot", because the alternative is restored RAM over a drive that moved, which is the machine
    /// that looks fine and is not.
    ///
    /// The size half of the fingerprint is what is asserted against a real edit. The mtime half is
    /// asserted against a stamp written by hand, because how finely a filesystem records mtime is a
    /// property of the filesystem and asserting it here would be testing the volume the test ran on.
    #[test]
    fn a_drive_that_moved_since_the_stamp_is_refused() {
        let dir = std::env::temp_dir().join(format!("ipod-stamp-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let snap = dir.join("b.snap");
        let drive = dir.join("b.img");
        std::fs::write(&snap, b"snapshot").unwrap();
        std::fs::write(&drive, b"the drive as it was").unwrap();

        // Direct mode: the working drive IS the user's image, which is the condition that makes
        // the stamp necessary in the first place.
        let cfg = Config {
            disk: drive.clone(),
            workdisk: drive.clone(),
            frozen: dir.join("b.frozen"),
            clock: eapp_loader::CLOCK,
            snapshot: Some(snap.clone()),
            snap_at: 1,
            cold: false,
            work_on_copy: false,
            ..Default::default()
        };

        assert!(!cfg.may_restore(true), "no stamp yet, so there is no pair");
        cfg.pair_with_drive()
            .expect("stamping a drive that exists must work");
        assert!(
            cfg.may_restore(true),
            "stamped, and nothing has touched the drive since"
        );

        // What iTunes, `make-disk` or a second window does to it.
        std::fs::write(&drive, b"the drive after something else wrote to it").unwrap();
        assert!(
            !cfg.may_restore(true),
            "the drive moved, so the snapshot no longer describes it"
        );

        // And the case where the stamp itself is unreadable or from another build.
        cfg.pair_with_drive().unwrap();
        assert!(cfg.may_restore(true), "re-stamped");
        std::fs::write(cfg.stamp().unwrap(), b"not a fingerprint").unwrap();
        assert!(
            !cfg.may_restore(true),
            "a stamp that does not parse is not a match"
        );

        // The stamp lives beside the snapshot, not beside the drive: a hand-given `--snapshot=`
        // has to bring its own, exactly as the frozen drive does.
        assert_eq!(cfg.stamp().unwrap(), dir.join("b.drive"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The working drive is **replaced** on every launch, never reused.
    ///
    /// This is a regression test with a specific bug behind it. `clone_disk` used to open with
    /// `if to.exists() { return Ok(()) }`, which made the "per-run clone" a one-time clone: the
    /// machine wrote to the same drive for ever while its snapshot stayed frozen at the first
    /// boot. Restoring then paired old RAM with a drive that had moved on, RetailOS found its
    /// cached view of the volume contradicted, and it showed "connect to computer" — sometimes, on
    /// restart, with nothing in the UI to say why.
    ///
    /// The failure it guards is silent by nature, so the assertion is on the bytes: after a clone,
    /// the destination must be the source, not what the destination used to be.
    #[test]
    fn the_working_drive_is_replaced_not_reused() {
        let dir = std::env::temp_dir().join(format!("ipod-clone-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let pristine = dir.join("pristine.img");
        let work = dir.join("work.img");
        std::fs::write(&pristine, b"PRISTINE").unwrap();

        clone_disk(&pristine, &work).unwrap();
        assert_eq!(std::fs::read(&work).unwrap(), b"PRISTINE");

        // RetailOS writes to the drive during a boot. The next launch must not inherit it.
        std::fs::write(&work, b"MUTATED!").unwrap();
        clone_disk(&pristine, &work).unwrap();
        assert_eq!(
            std::fs::read(&work).unwrap(),
            b"PRISTINE",
            "a reused working drive is the stale-pair bug returning"
        );

        // Cloning a path onto itself is the one case that must not delete anything: it is what
        // `--workdisk=` pointed straight at the frozen drive would do.
        clone_disk(&work, &work).unwrap();
        assert_eq!(
            std::fs::read(&work).unwrap(),
            b"PRISTINE",
            "self-clone must not empty the drive"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A boot that ended on the fallback teaches §12.3's denominator nothing** — and the
    /// substitution is this test's own control.
    ///
    /// The two endings sit one line apart in the run loop and produce the same phase. A reader that
    /// watched for the phase change and took `Stats::executed` would file `snap_at` — a constant
    /// tuned to RetailOS's 1.6 G — as *this device's own last completed cold boot*, on every
    /// machine that never settled. That is the exact defect `Device::cold_boot_instructions`
    /// replaced `snap_at` to fix, re-entered from the other end.
    ///
    /// The control is the substitution written out: `Some(Some(executed))` for both arms is what a
    /// phase-watcher computes, and the assertion below is what tells it apart from the truth.
    #[test]
    fn a_boot_that_ended_on_the_fallback_teaches_the_denominator_nothing() {
        const SNAP: u64 = SNAP_AT;

        // Still booting: the machine has not settled and the fallback is not due.
        assert_eq!(boot_end(None, 1_000, SNAP), None);
        assert_eq!(boot_end(None, SNAP - 1, SNAP), None);

        // The fallback. The boot phase ends — a window that says *booting* for ever over a machine
        // that failed before the UI is the twenty-one-minute hostage §7.3 added a stop control for
        // — and **nothing is learned**.
        assert_eq!(
            boot_end(None, SNAP, SNAP),
            Some(None),
            "the fallback ended the boot and this reported a measurement"
        );
        assert_eq!(boot_end(None, SNAP * 3, SNAP), Some(None));

        // The observation. The machine went quiet with its drive answered, and the count at the
        // moment it stopped working is what the next boot divides by.
        assert_eq!(
            boot_end(Some(871_685_000), 871_712_000, SNAP),
            Some(Some(871_685_000)),
            "the observed arm published the count it noticed at, not the one it settled at"
        );
        // …at any count, including one before the fallback would ever have been due, which is the
        // case Rockbox is: it reaches its menu at about 100 M.
        assert_eq!(
            boot_end(Some(100_000_000), 108_000_000, SNAP),
            Some(Some(100_000_000))
        );

        // **The control.** A reader keyed on the phase change alone cannot tell the two apart: it
        // computes the same non-`None` answer for both, and the number it takes on the fallback arm
        // is `snap_at` itself.
        let phase_watcher =
            |settled: Option<u64>, executed: u64| (settled.is_some() || executed >= SNAP).then_some(executed);
        assert_eq!(phase_watcher(None, SNAP), Some(SNAP));
        assert_eq!(
            boot_end(None, SNAP, SNAP).flatten(),
            None,
            "and this is the whole difference between the two readings"
        );
        assert!(
            phase_watcher(Some(871_685_000), 871_712_000)
                != boot_end(Some(871_685_000), 871_712_000, SNAP).flatten(),
            "the phase-watcher reads the count at the moment of NOTICING; the observation is the \
             count at the moment the machine stopped working, and one window separates them"
        );
    }

    /// **The end of a cold boot is the machine going quiet, and this is the run that measured it —
    /// beside the reading it replaced, which was wrong by 387x.**
    ///
    /// # What was believed
    ///
    /// [`boot_end`]'s observed arm was *"RetailOS asking for wheel frames … a machine asking for
    /// input is a machine that has finished starting"*. Driven against Apple's own NOR dump and a
    /// real 5.5G's drive through the window's own start path — `ipod-gui`'s
    /// `the_bench_boots_apples_software_and_this_needs_resources`, which is `#[ignore]`d because it
    /// needs `resources/` — that is false, and not marginally:
    ///
    /// ```text
    /// the window USED to leave `Booting`    2 250 000 instr    0 ata      0 lit   0 co-proc frames
    /// the first pixel lights               42 999 970 instr    0 ata
    /// the drive first answers              57 499 970 instr
    /// the machine goes quiet              823 593 896 instr  765 ata  75 267 lit   2 co-proc frames
    /// the last work finishes              872 147 527 instr  768 ata
    /// ```
    ///
    /// At the instant the window declared the boot over the drive had answered **nothing** and the
    /// panel was **black** — the first of each is nineteen and twenty-five times further on.
    ///
    /// The last two rows carry a few hundred thousand instructions of run-to-run jitter, because
    /// they are read off `Out` on a tick and the ticks do not land in the same places twice. The
    /// first three do not: they are the machine's own arithmetic and reproduce exactly.
    ///
    /// # Which sender it actually was, measured rather than reasoned
    ///
    /// `ipod-boot retail --storeaddr=0x7000c120 --storelog-dump=`, pinned to the same NOR dump and
    /// the same `PRISTINE` drive, over a 1.2 G budget. Every write to the click wheel's TX register
    /// in the whole run, and there are four:
    ///
    /// ```text
    /// 0x4000e654 -> [0x7000c120] = 0x8001052a   @2211983
    /// 0x4000e654 -> [0x7000c120] = 0x8000023a   @2833953
    /// 0x4000e654 -> [0x7000c120] = 0x8000023a   @18107448
    /// 0x4000e654 -> [0x7000c120] = 0x8000023a   @57422301
    /// ```
    ///
    /// One `0x052a`, at **@2 211 983**, from **`0x4000e654`** — the boot ROM's own opto bring-up
    /// running out of IRAM, 55 M instructions before the drive answers and 41 M before the first
    /// pixel. `--enterlog` on all five of RetailOS's documented senders — `0x00283e20`,
    /// `0x00283e10`, `0x000b2ce0`, `0x000bbdb0`, `0x000b4638` — records **0 arrivals** in that run,
    /// against **944 984** at a control address armed in the same report, which is what makes the
    /// zero a reading.
    ///
    /// # And a later one exists after all, which is the half a control caught
    ///
    /// That `0 arrivals` was first written up here as *RetailOS never sends the command on a cold
    /// boot*, and it is not true: it is true of the machine `ipod-boot retail` builds, which stops
    /// at Apple's logo with 70 ATA commands and never reaches the code. The window's own boot is
    /// the run that can answer, and the bench prints its whole census — **five** `0x052a`
    /// commands, all payload 1:
    ///
    /// ```text
    /// @2 205 089     the boot ROM's, `pc = 0x4000e654`
    /// @111 545 868   RetailOS's first — 50x further on, and still 12.8 % of the boot
    /// @823 611 625   two within one sample, at the instant the machine settles
    /// @823 719 014   after it has settled
    /// ```
    ///
    /// So the answer to *does RetailOS send one of its own later* is **yes**, and it makes no
    /// difference: its earliest is a ninth of the way through a boot, and its other three arrive
    /// after the machine has already stopped working. No arrival of this command is the end of a
    /// cold boot. research/10 Addendum 32 is the write-up and the retraction.
    ///
    /// # What replaces it
    ///
    /// [`Quiet`]: a trailing window of the machine's own steps that is [`QUIET_HALTED_PERCENT`]
    /// halted, with the drive answered. Over the 8 M-step window the whole 872 M boot never exceeds
    /// **61.7 %**, and the machine holds **99.7 %** from 823.6 M onward.
    ///
    /// # How to make it go red, both measured
    ///
    /// - **The observation**: set [`QUIET_HALTED_PERCENT`] to `100` — a bar no window with a single
    ///   executed instruction in it can clear — and nothing ever settles: *the machine went quiet
    ///   and this never noticed*. (`101` does not compile, because `slack` below derives itself
    ///   from the same constant and the subtraction underflows. That is the constant's range
    ///   asserted at build time, and it is worth more than the ablation would have been.)
    /// - **The drive half**: delete `ata_commands > 0 &&` from [`Quiet::read`] and the hung machine
    ///   at the end is called booted at 6 000 instructions.
    #[test]
    fn the_first_ask_for_frames_is_not_the_end_of_a_cold_boot() {
        /// The boot ROM's `0x8001052a`, which the window used to read as the end of the boot.
        /// `@2 211 983` at the bus; 2 250 000 is where the window's own sampling notices it.
        const OLD_SIGNAL_AT: u64 = 2_250_000;
        /// The same machine, when it stopped executing new work.
        const QUIET_AT: u64 = 872_147_527;

        // **What the old signal was worth as a denominator**, kept as arithmetic rather than as
        // prose. `Progress::read` divides by it, so a bar drawn against this reads full while the
        // machine still has 99.7 % of its boot to do.
        let fraction = OLD_SIGNAL_AT as f64 / QUIET_AT as f64;
        assert!(
            fraction < 0.01,
            "the measurement this test was written to pin has moved: the boot ROM's command is now \
             {:.1} % of the boot rather than 0.26 %. Re-run \
             `the_bench_boots_apples_software_and_this_needs_resources` and re-state it",
            fraction * 100.0
        );
        // And the control that made the point: the fallback — a constant nobody claims is a
        // measurement — lands within 1.9x of where the machine actually finished, and the arm that
        // called itself an observation was 387x out. The signal was worse than the thing it was
        // introduced to replace.
        assert!(
            SNAP_AT.abs_diff(QUIET_AT) < QUIET_AT.abs_diff(OLD_SIGNAL_AT),
            "the fallback is now further from the truth than the old observation, which would make \
             this test's own point backwards"
        );

        // ── And what the machine that replaced it answers, driven on the measured shape ─────────
        //
        // A boot: 75 instructions per simulated microsecond with the core barely halting, the
        // drive answering from 57.5 M. Fed a step at a time, `Quiet` must stay silent through all
        // of it — including the worst window the real boot has, which is 61.7 % halted.
        let mut q = Quiet::default();
        let mut executed = 0u64;
        let mut idle = 0u64;
        for slice in 0..600u64 {
            executed += 2_000_000;
            // 61.7 % halted for one 8 M-step stretch in the middle, as measured at 164.7 M.
            idle += if (80..84).contains(&slice) { 3_220_000 } else { 40_000 };
            let ata = u64::from(executed > 57_500_000) * 700;
            assert_eq!(
                q.read(executed, idle, ata),
                None,
                "the boot settled at {executed}, and the real boot's own worst window is 61.7 % \
                 halted with nothing finished"
            );
        }
        // Then it stops working: 99.7 % of every window halted. The count it publishes is where
        // the machine last did work, not where the window noticed — and the window that straddles
        // the transition is mixed, so it re-arms once and the answer lands one window later. That
        // costs nothing readable, and the bound is derived rather than picked: the most a window
        // this program will accept as quiet can have executed is the fraction of it that was not
        // halted.
        let stopped_at = executed;
        let mut answer = None;
        for _ in 0..16 {
            executed += 6_000;
            idle += 2_000_000;
            answer = answer.or(q.read(executed, idle, 768));
        }
        let settled = answer.expect("the machine went quiet and this never noticed");
        let slack = QUIET_WINDOW_STEPS * (100 - QUIET_HALTED_PERCENT) / 100;
        assert!(
            settled >= stopped_at && settled - stopped_at <= slack,
            "the boot cost was published as {settled}; the machine stopped working at \
             {stopped_at}, and one quiet window can account for at most {slack} instructions"
        );
        // **The drive is half the condition.** The identical trace with a drive that never answered
        // is a machine halted on an interrupt that is not coming, and it must teach nothing.
        let mut hung = Quiet::default();
        let (mut n, mut i) = (0u64, 0u64);
        for _ in 0..64 {
            n += 6_000;
            i += 2_000_000;
            assert_eq!(
                hung.read(n, i, 0),
                None,
                "a machine halted with a drive that never answered was called booted"
            );
        }
    }

    /// **A park and a restore are a round trip, and §12.4's frame comes back with them.**
    ///
    /// It builds a real machine off a synthesised ROM and a scratch drive, parks it through the
    /// shipped writer, and then asks the questions the *next launch* asks: is the pair whole, may
    /// this restore, and is there a picture of the frame it stopped on.
    ///
    /// **How to make it go red**, in one line each:
    ///
    /// - Drop the frame write — delete the `write_parked_frame(cfg, frame)` call in
    ///   `write_restore_point` — and the three assertions about the PNG fail: `Config::parked_frame`
    ///   is not on disk, `machine::parked_frame` answers `None`, and the glass §12.4 promises has
    ///   nothing on it. Nothing else in the suite notices, which is the point of asserting it here.
    /// - Return `Some(now)` before `pair_with_drive` and `may_restore` goes false on the next
    ///   launch while `Device::parked_at` says the machine was parked four minutes ago.
    /// - Take [`SNAPSHOT_SLACK`] out of `snapshot_bytes` and the estimate goes **under** the real
    ///   snapshot, which is a free-space check that passes and a write that fills the volume.
    #[test]
    fn a_park_and_a_restore_are_a_round_trip_and_the_frame_comes_back() {
        let dir = std::env::temp_dir().join(format!("ipod-park-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let disk = dir.join("d.img");
        std::fs::write(&disk, b"not really a drive").unwrap();

        // **A boot image rather than a drive with an OS on it**, and the difference is what makes
        // this test runnable at all: a synthesised ROM does a high-level boot, which reads the
        // `!ATA` firmware directory out of the drive — several megabytes of RetailOS that no test
        // fixture can carry. `BootTarget::Image` enters a raw ARM image at `0x10000000` instead,
        // which is `rb-main.raw`'s route (§12.5), and sixteen words of `b .` is a program by every
        // test `build` applies to one. What is under test is the **park**, and a parked machine is
        // a `Machine` with regions in it whatever it was executing.
        let boot = dir.join("tiny.raw");
        std::fs::write(&boot, [0xfeu8, 0xff, 0xff, 0xea].repeat(16)).unwrap();

        let cfg = Config {
            nor: eapp_loader::nor::Source::Synthetic {
                model: eapp_loader::nor::DEFAULT_MODEL.into(),
                seed: 1,
                serial: None,
                guid: None,
                splash: None,
            },
            disk: disk.clone(),
            // Direct mode, which is the window's default: the working drive IS the user's image
            // and the pair's other half is the stamp beside the snapshot.
            workdisk: disk.clone(),
            frozen: dir.join("m.frozen"),
            snapshot: Some(dir.join("m.snap")),
            snap_at: SNAP_AT,
            clock: eapp_loader::CLOCK,
            boot: BootTarget::Image(boot),
            ..Default::default()
        };
        let m = build(&cfg, true).expect("a machine off a synthesised ROM and a raw boot image");

        // Before: nothing on disk, so nothing to resume. This is what every device in a fresh
        // library answers, and it is the control for every assertion below it.
        assert!(!cfg.may_restore(true), "there is no snapshot yet");
        assert_eq!(crate::machine::Restore::of(&cfg), crate::machine::Restore::Never);

        // §12.4's frame: a diagnostic picture rather than a pretty one, so a channel swap or a
        // stride error shows up in the comparison rather than passing as "some bytes came back".
        let mut frame = vec![0u8; FB_W * FB_H * 3];
        for y in 0..FB_H {
            for x in 0..FB_W {
                let p = (y * FB_W + x) * 3;
                frame[p] = (x % 256) as u8;
                frame[p + 1] = (y % 256) as u8;
                frame[p + 2] = if x < y { 0xff } else { 0x11 };
            }
        }

        let estimate = snapshot_bytes(&m);
        let at = write_restore_point(&cfg, &m, Some(&frame)).expect("the park writes a pair");
        assert!(at > 1_700_000_000, "the park time is not a Unix second: {at}");

        // ── The next launch's questions ─────────────────────────────────────────────────────────
        let snap = cfg.snapshot.clone().unwrap();
        let actual = std::fs::metadata(&snap).expect("the snapshot").len();
        assert!(
            estimate >= actual,
            "`snapshot_bytes` predicted {estimate} and the snapshot is {actual}: a free-space \
             check against an under-estimate passes and then fills the volume"
        );
        assert!(
            estimate <= actual + SNAPSHOT_SLACK * 4,
            "the estimate is {estimate} against {actual}, which is loose enough to refuse a park \
             that would have fitted"
        );
        assert!(
            cfg.stamp().is_some_and(|p| p.exists()),
            "the half that pairs the RAM with the drive is not on disk"
        );
        assert!(cfg.may_restore(true), "the pair is whole and this refuses to restore it");
        assert_eq!(crate::machine::Restore::of(&cfg), crate::machine::Restore::Whole);

        // ── §12.4 and §17 Q7: the frame it stopped on ───────────────────────────────────────────
        let png = cfg.parked_frame().expect("a snapshot has a parked frame path");
        assert_eq!(png, dir.join("m.parked.png"), "beside the snapshot, under its own stem");
        assert!(png.exists(), "the park wrote no picture, so a parked glass is dark");
        let mut seen = eapp_loader::settings::Presence::new();
        assert_eq!(
            crate::machine::parked_frame(&cfg, &mut seen),
            Some(png.clone()),
            "the writer and the reader disagree about where the frame is"
        );
        // It is a PNG a decoder reads — asserted against the shipped header rather than against
        // `png::encode` calling itself correct.
        let bytes = std::fs::read(&png).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "that is not a PNG");
        assert!(
            bytes.len() > FB_W * FB_H * 3,
            "a stored-deflate PNG of a full frame cannot be {} bytes",
            bytes.len()
        );

        // ── And a machine that had never drawn leaves no picture at all ─────────────────────────
        //
        // A black PNG and no PNG are different claims: §12.4's fallback for the second is *the
        // glass is dark*, and writing a black frame instead would be the bench asserting that this
        // is what the machine had on screen.
        assert!(write_restore_point(&cfg, &m, None).is_some(), "the pair is still written");
        assert!(
            !png.exists(),
            "a park with no frame left the PREVIOUS park's picture beside a new snapshot"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A synthesised ROM boots Apple's software as far as a real dump does — measured, on the
    /// reference drive.**
    ///
    /// `#[ignore]`, and the name says which kind: it needs `resources/drives/`, which is not in git.
    /// Run it by name from a release build, because a debug interpreter takes about twenty times as
    /// long to do the same 400 M instructions:
    ///
    /// ```text
    /// cargo test --release -p ipod-gui --bin ipod-emulator \
    ///     a_synthesised_rom_boots_the_os_and_this_needs_resources -- --ignored --nocapture
    /// ```
    ///
    /// The floor is `research/04` row 9's discriminator rather than a round number: **102** ATA
    /// commands for a retail cold boot against **24** for the same boot with the IDE interrupt latch
    /// ablated, where the 24 is Apple's bootloader painting its own screen and never handing
    /// RetailOS the disk. Anything in the hundreds is RetailOS working the volume.
    ///
    /// **What it measured, 2026-08-25**, `--headless=400000000` at the real clock on
    /// `ipod8g-retail.PRISTINE.img` with a synthesised `A146`:
    ///
    /// ```text
    /// BudgetExhausted after 400 044 725 instructions
    /// ata commands: 484   —  387 READ DMA · 91 WRITE DMA · 1 IDENTIFY · 5 SET FEATURES
    /// 28 105 code buckets      unmapped: 0 reads, 0 writes
    /// ```
    ///
    /// **How to make it go red**: put the OS back in a region beside SDRAM instead of writing it
    /// into SDRAM — see `a_high_level_boot_survives_the_os_remapping_low_memory_onto_sdram`, which
    /// is the same defect in twenty-one instructions and no `resources/` — and this run ends
    /// `Lost(0x02000000)` after 8 388 485 instructions with **0** ATA commands and 2 097 122 code
    /// buckets, every one of them a NOP slide.
    #[test]
    #[ignore = "needs resources/: a real drive image, which is not in git"]
    fn a_synthesised_rom_boots_the_os_and_this_needs_resources() {
        let pristine = eapp_loader::settings::repo_root()
            .join("resources/drives/ipod8g-retail.PRISTINE.img");
        assert!(
            pristine.is_file(),
            "this test was asked for by name and {} is not on this machine. See \
             tools/ipod-boot/DISK-IMAGES.md.",
            pristine.display()
        );

        let dir = std::env::temp_dir().join(format!("ipod-synth-boot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Never the source: RetailOS bootstraps its own volume during boot, and the reference image
        // is `chmod 444` on purpose. `cp -c` carries the mode across, so the clone is made writable.
        let work = dir.join("work.img");
        clone_disk(&pristine, &work).expect("a writable clone of the reference drive");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o644)).unwrap();
        }

        let cfg = Config {
            // A synthesised ROM, which is the whole point: no Apple code anywhere in this machine
            // except what comes off the drive.
            nor: eapp_loader::nor::Source::Synthetic {
                model: "A146".into(),
                seed: 1,
                serial: None,
                guid: None,
                splash: None,
            },
            disk: work.clone(),
            workdisk: work.clone(),
            frozen: dir.join("m.frozen"),
            clock: eapp_loader::CLOCK,
            boot: BootTarget::Os,
            ..Default::default()
        };
        let mut m = build(&cfg, true).expect("a machine off a synthesised ROM and the reference drive");

        // The run loop's own two calls: `call_with` for the first slice, `run` for the rest, and
        // the entry is the PC `build` left rather than a second decision about where an OS starts.
        const BUDGET: u64 = 400_000_000;
        let pc = m.cpu.regs[15];
        let mut stop = m.call_with(pc, &[0, 0, 0, 0], SLICE);
        while stop == eapp_loader::Stop::BudgetExhausted && (m.executed as u64) < BUDGET {
            stop = m.run(SLICE);
        }

        let ata = m.mem.ata.as_ref().map_or(0, |(_, a)| a.commands.seen());
        assert!(
            !matches!(stop, eapp_loader::Stop::Lost(_)),
            "the machine left every mapped region after {} instructions with {ata} ATA commands: \
             {stop:?}",
            m.executed
        );
        assert!(
            ata > 100,
            "{ata} ATA commands in {} instructions. research/04 row 9: a retail cold boot is 102 \
             and a bootloader that never hands over the disk is 24, so this is not a boot",
            m.executed
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A machine that dies says so, and a machine that dies the same way again says that.**
    ///
    /// The log this came out of — a real first run, 2026-08-25 — is fifty lines of
    ///
    /// ```text
    ///   high-level boot: 7561216 bytes of OS from …/my-5.5g.img -> 0x10000000
    ///   identity: JQ652FQUX3N · 000A27CB851F81B9
    /// ```
    ///
    /// twenty-five times over, and **nothing else**: no reason, no ending, no difference between
    /// the first boot and the twenty-fifth. It was read as a retry loop, which is the only thing
    /// that log can be read as, and it was twenty-five presses on a bench whose iPod died in a
    /// third of a second each time.
    ///
    /// **How to make it go red**: delete the `println!("{}", deaths.note(&why))` from `session` and
    /// the program is back to printing two lines per machine and none per death.
    #[test]
    fn a_machine_that_dies_the_same_way_twice_says_it_is_the_same_way() {
        let mut d = Deaths::default();
        let why = "Lost(33554432) at 8388485 instructions";
        assert_eq!(d.note(why), format!("stopped: {why}"));
        // The second one is not the first one repeated: it says *again*, with a count, because a
        // person looking at two identical lines cannot tell a second failure from a second copy.
        let again = d.note(why);
        assert!(again.contains(why), "the reason went missing on the second death: {again}");
        assert!(again.contains('2'), "the second death does not say it is the second: {again}");
        assert!(d.note(why).contains('3'), "the count is not advancing past two");
        // A different ending starts the count over — two failures are not one failure twice.
        let other = d.note("Returned at 12 instructions");
        assert_eq!(other, "stopped: Returned at 12 instructions");
    }

    /// **An OS that remaps low memory onto SDRAM and carries on running there** — the thing
    /// RetailOS does 0x220 bytes into its own entry, in twenty-one instructions.
    ///
    /// This is what a high-level boot has to survive, and it is the whole of the failure the
    /// window shipped with. Apple's bootloader is measured — `ipod-boot retail`, 58 READ DMA
    /// commands ending at `0x10736000`, then `Running 'osos' 0 from 0x10000000` — putting the OS
    /// **into SDRAM** and entering it there. RetailOS then programs the PP's remap windows at
    /// `0xf000f000`, one of which is `0x00000000..0x01ffffff -> 0x10000000`, and from that
    /// instruction on it executes from low addresses. `Memory::translate` runs ahead of the region
    /// list, so after the remap *only what is really in SDRAM answers*.
    ///
    /// **How to make it go red**, and this is the bug it was written for: put the image back in a
    /// region beside SDRAM —
    ///
    /// ```text
    ///     m.mem.regions.push(Region { name: "osos-low", base: 0, data: osos.clone() });
    ///     m.mem.regions.push(Region { name: "osos", base: load_at, data: osos });
    ///     …and enter at 0 instead of `load_at + entry`
    /// ```
    ///
    /// — and the sentinel below is never written. Region lookup is first-match and `map_hardware`
    /// has already registered 64 MB of `sdram` at `0x10000000`, so the `osos` region is read by
    /// nothing and SDRAM is zeros with a copy of the OS filed behind it. The remap then points the
    /// low window at those zeros, the code goes out from under the PC, and the CPU NOP-slides
    /// (`0x00000000` decodes as `andeq r0, r0, r0`) to the top of the window: on the real thing,
    /// `Lost(0x02000000)` after 8 388 485 instructions, which is `(0x02000000 - 0x1ec) / 4`.
    ///
    /// The image here is twenty-one words rather than 7.5 MB of RetailOS, so this runs in the
    /// ordinary suite with nothing out of `resources/` — but it is the same twenty-one
    /// instructions and the same window.
    #[test]
    fn a_high_level_boot_survives_the_os_remapping_low_memory_onto_sdram() {
        use arm7tdmi::Bus as _;

        // Hand-assembled, because what is under test is where the bytes are rather than what they
        // say, and a 21-word program that can be read in place beats a fixture nobody can check.
        //
        //   0x00  b 0x20                     the two branches `ipsw::image_header` looks for
        //   0x04  b 0x20
        //   0x08  b .                        the rest of the vector table
        //   0x20  mov r0, #0xf0000000        MMAP window 0
        //         orr r0, r0, #0xf000
        //         mov r1, #0x3e00            logical: mask 0x3e000000 -> a 32 MB window at 0
        //         mov r2, #0x10000000        physical: SDRAM
        //         str r1, [r0]
        //         str r2, [r0, #4]           the window is live from here
        //         mov pc, #0x40              …so jump into the LOW view of this same image
        //   0x40  mov r3, #0x10000000        the sentinel address, 0x10010000
        //         orr r3, r3, #0x10000
        //         mov r4, #0xa5
        //         str r4, [r3]               "the low half ran"
        //   0x50  b .
        const OS: [u32; 21] = [
            0xea00_0006, 0xea00_0005, 0xeaff_fffe, 0xeaff_fffe, 0xeaff_fffe, 0xeaff_fffe,
            0xeaff_fffe, 0xeaff_fffe, 0xe3a0_04f0, 0xe380_0cf0, 0xe3a0_1c3e, 0xe3a0_2410,
            0xe580_1000, 0xe580_2004, 0xe3a0_f040, 0xeaff_fffe, 0xe3a0_3410, 0xe383_3801,
            0xe3a0_40a5, 0xe583_4000, 0xeaff_fffe,
        ];
        const SENTINEL_AT: u32 = 0x1001_0000;
        const SPINNING_AT: u32 = 0x50;

        let dir = std::env::temp_dir().join(format!("ipod-remap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let disk = dir.join("d.img");

        // A drive is a file with a firmware partition at LBA 63 and an `!ATA` directory in it.
        // Nothing else here reads the volume, so nothing else is written.
        let image: Vec<u8> = OS.iter().flat_map(|w| w.to_le_bytes()).collect();
        let base = eapp_loader::ipsw::FIRMWARE_LBA as u64 * 512;
        const DEV_OFFSET: u32 = 0x4400;
        let mut entry = [0u8; 40];
        entry[..4].copy_from_slice(b"!ATA");
        // The tag is a little-endian u32 of four characters, so `osos` is stored backwards.
        entry[4..8].copy_from_slice(b"soso");
        entry[8..12].copy_from_slice(&0u32.to_le_bytes());
        entry[0x0c..0x10].copy_from_slice(&DEV_OFFSET.to_le_bytes());
        entry[0x10..0x14].copy_from_slice(&(image.len() as u32).to_le_bytes());
        entry[0x14..0x18].copy_from_slice(&eapp_loader::ipsw::LOAD_ADDR_5G.to_le_bytes());
        entry[0x18..0x1c].copy_from_slice(&0u32.to_le_bytes());
        {
            use std::io::{Seek, SeekFrom, Write};
            let mut f = std::fs::File::create(&disk).unwrap();
            f.set_len(1 << 20).unwrap();
            f.seek(SeekFrom::Start(base + eapp_loader::ipsw::DIRECTORY_AT as u64))
                .unwrap();
            f.write_all(&entry).unwrap();
            f.seek(SeekFrom::Start(base + DEV_OFFSET as u64)).unwrap();
            f.write_all(&image).unwrap();
        }

        let cfg = Config {
            nor: eapp_loader::nor::Source::Synthetic {
                model: eapp_loader::nor::DEFAULT_MODEL.into(),
                seed: 1,
                serial: None,
                guid: None,
                splash: None,
            },
            disk: disk.clone(),
            // Direct mode: the working drive IS this file, so `clone_disk` is a no-op and the
            // machine reads what was just written.
            workdisk: disk.clone(),
            frozen: dir.join("m.frozen"),
            clock: eapp_loader::CLOCK,
            boot: BootTarget::Os,
            ..Default::default()
        };
        let mut m = build(&cfg, true).expect("a machine off a synthesised ROM and a drive with an OS");

        // **The OS is in SDRAM, before a single instruction runs.** This is the state Apple's
        // bootloader leaves and the reason the rest of the test can pass: a region filed behind
        // `sdram` reads as zero here.
        assert_eq!(
            m.mem.read32(eapp_loader::ipsw::LOAD_ADDR_5G),
            OS[0],
            "SDRAM does not hold the OS at its load address, so the remap below has nothing to \
             point at"
        );
        // And the machine starts where the bootloader's console says it starts.
        assert_eq!(
            m.cpu.regs[15],
            eapp_loader::ipsw::LOAD_ADDR_5G,
            "the CPU was not left at the entry `Running 'osos' 0 from 0x10000000` names"
        );

        // The run loop's own entry: `session` reads the PC `build` left rather than deciding again.
        let pc = m.cpu.regs[15];
        let stop = m.call_with(pc, &[0, 0, 0, 0], 100_000);

        assert_eq!(
            stop,
            eapp_loader::Stop::BudgetExhausted,
            "the machine left every mapped region after remapping low memory onto SDRAM"
        );
        assert_eq!(
            m.mem.read32(SENTINEL_AT),
            0xa5,
            "the half of the OS that runs from the LOW view never executed: after the remap, \
             address 0 is SDRAM and SDRAM did not have the OS in it"
        );
        assert_eq!(
            m.cpu.regs[15], SPINNING_AT,
            "the CPU is not spinning where the image says it should be, so it reached the \
             sentinel by some route this test does not describe"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

}
