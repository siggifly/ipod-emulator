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
use std::sync::atomic::{AtomicBool, Ordering};
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
    pub flash: PathBuf,
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
}

/// UI -> emulator, the commands that are not buttons.
///
/// Deliberately not modelled as click-wheel input: nothing measured in this project shows RetailOS
/// or our model acting on MENU+SELECT (see `Probe::Combo` and research/10 Addendum 31 §5), so a
/// control that claimed to be the hardware combo while actually restarting the emulator would be
/// the UI lying about what the machine does. These restart the *emulator*, and say so.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmd {
    PowerOff,
    /// Power on from off — always a cold boot, never a restore.
    PowerOn,
    /// Power off and straight back on.
    PowerCycle,
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
    pub stats: Stats,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Cold-booting. The number is the instruction count the snapshot will be taken at, so the UI
    /// can show a progress bar that means something.
    Booting { target: u64 },
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
    pub sim_usec_here: u64,
    pub hold: bool,
    pub touched: bool,
    pub position: u8,
    pub buttons: u8,
    /// The `0x052a` gate. **On at reset** — the part streams unless told not to, which is what
    /// lets a driver that never sends the command (Rockbox) receive anything at all. Autonomous
    /// frames also need the receiver armed; both conditions live in `eapp-loader`.
    pub reporting: bool,
    /// Whether the firmware has *sent* `0x052a` at all — a different question from whether the
    /// stream is on, and the one that means "this machine has finished starting and wants input".
    /// After a restore it starts false, because the click wheel is not part of a snapshot.
    pub asked_for_frames: bool,
    pub frames_posted: u64,
    pub frames_dropped: u64,
    pub frames_suppressed: u64,
    pub data_reads: u64,
    pub data_reads_ready: u64,
    pub irqs: u64,
    /// Steps refused because the drain queue was already full — always shown, never silent.
    pub input_dropped: u64,
    pub queued: usize,
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
                stats: Stats::default(),
            }),
            quit: AtomicBool::new(false),
            resnap: AtomicBool::new(false),
            save_on_quit: AtomicBool::new(false),
            saving: AtomicBool::new(false),
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
        let Some(stamp) = self.stamp() else { return false };
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
    eapp_loader::map_hardware(&mut m, true);
    // Hardware revision probe: boot reads 0x70000000, takes bits 16..23 and compares to 0x36.
    {
        use arm7tdmi::Bus as _;
        m.mem.write32(0x7000_0000, 0x0036_0000);
        // `--charger`: GPIOL bit 3 low is "mains charger attached", and it is what decides between
        // the charging screen and the UI. See research/10 Addendum 30 §1 and §6.
        if cfg.charger {
            m.mem.write32(0x6000_d13c, 0x0000_0000);
        }
    }

    let flash = std::fs::read(&cfg.flash)
        .map_err(|e| format!("{}: {e}", cfg.flash.display()))?;
    let size = flash.len() as u32;
    // Cold boot: the flash also answers at 0, where the CPU fetches out of reset. Inserted at the
    // front so it wins the first-match lookup for low addresses.
    m.mem.readonly.push("flash-low");
    m.mem
        .regions
        .insert(0, Region { name: "flash-low", base: 0, data: flash.clone() });
    m.mem.readonly.push("flash");
    m.mem.regions.push(Region { name: "flash", base: 0x2000_0000, data: flash });
    m.mem.nor = Some(Nor::sst39wf800a(
        vec![(0x2000_0000, size), (0, size)],
        vec!["flash", "flash-low"],
    ));

    // The co-processor, with the GENCMD registry published. Without `registry` RetailOS never gets
    // an answer to its service lookup and never draws — the panel would be a black rectangle and
    // the GUI would be showing a true picture of a machine configured not to work.
    let mut bcm = Bcm::new(0x3000_0000);
    bcm.registry = true;
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
    let source = if cfg.work_on_copy && cfg.may_restore(first) { &cfg.frozen } else { &cfg.disk };
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
        m.stop_when_idle = if cfg.no_idle_stop { None } else { Some(400_000_000) };
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
fn clone_disk(from: &Path, to: &Path) -> Result<(), String> {
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
fn drain(m: &mut Machine, inbox: &Mutex<Inbox>, next_at: &mut u64, gap: u64) -> usize {
    let now = m.executed as u64;
    let Some(w) = m.mem.clickwheel.as_mut() else { return 0 };
    let mut inbox = inbox.lock().unwrap();
    while let Some(&ev) = inbox.events.front() {
        let at = (*next_at).max(now);
        // Only schedule what will fire soon. Letting the whole queue onto the script at once is
        // harmless for the model but makes the depth invisible, and the depth is what tells a
        // person their drag is outrunning the emulator.
        if at > now + gap * 8 {
            break;
        }
        inbox.events.pop_front();
        w.script.push(WheelStep { at, event: ev });
        *next_at = at + gap;
    }
    inbox.events.len()
}

fn collect(m: &Machine, started: Instant, base: (u64, u32)) -> Stats {
    let w = m.mem.clickwheel.as_ref();
    let mut s = Stats {
        executed: m.executed as u64,
        sim_usec: m.mem.usec,
        wall_secs: started.elapsed().as_secs_f64(),
        executed_here: m.executed as u64 - base.0,
        sim_usec_here: m.mem.usec.wrapping_sub(base.1) as u64,
        ..Stats::default()
    };
    if let Some(w) = w {
        s.hold = w.hold;
        s.touched = w.touched;
        s.position = w.position;
        s.buttons = w.buttons;
        s.reporting = w.reporting;
        s.asked_for_frames = w.set_commands > 0;
        s.frames_posted = w.frames_posted;
        s.frames_dropped = w.frames_dropped;
        s.frames_suppressed = w.frames_suppressed;
        s.data_reads = w.data_reads;
        s.data_reads_ready = w.data_reads_ready;
        s.irqs = w.irqs;
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
    let mut first = true;
    loop {
        match session(&cfg, &link, first) {
            Outcome::Quit => return,
            Outcome::ColdBoot => first = false,
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
            Some(Cmd::PowerOn) | Some(Cmd::PowerCycle) => return true,
            // Already off. A queued power-off is not an error, and not a second one either.
            Some(Cmd::PowerOff) | None => {}
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// One power cycle: build the machine, run it, and stop when the window closes or power is cut.
fn session(cfg: &Config, link: &Arc<Link>, first: bool) -> Outcome {
    let mut m = match build(cfg, first) {
        Ok(m) => m,
        Err(e) => {
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
                    let cold = Config { cold: true, ..cfg.clone() };
                    match build(&cold, first) {
                        Ok(fresh) => m = fresh,
                        Err(e) => {
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

    {
        let mut out = link.out.lock().unwrap();
        out.phase = if restored { Phase::Running } else { Phase::Booting { target: cfg.snap_at } };
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
    let mut test = SelfTest { shots: cfg.shots.clone(), ..SelfTest::default() };
    let mut probe = Probing::new(cfg);
    // `None` means "not yet sampled", which is distinct from an unmapped address.
    let mut watched: Vec<Option<u32>> = vec![None; cfg.watch.len()];
    let mut last_executed = 0u64;
    let mut last_moved = std::time::Instant::now();

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
                write_restore_point(cfg, &m);
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
                Cmd::PowerOn => {}
            }
        }
        let queued = drain(&mut m, &link.inbox, &mut next_at, cfg.click_gap);

        // A cold boot enters at 0, where the CPU fetches out of reset, with `r0`-`r3` zeroed and
        // `lr` at the sentinel — exactly `trace.rs`'s `call_with(entry, &[0,0,0,0], …)`. A restored
        // machine is already somewhere and is simply resumed, which is `trace.rs`'s `m.run(…)`.
        let stop = if entered {
            m.run(SLICE)
        } else {
            entered = true;
            m.call_with(0, &[0, 0, 0, 0], SLICE)
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
                        m.mem.bcm.as_ref().map(|b| {
                            let mut buf = vec![0u8; FB_W * FB_H * 3];
                            read_framebuffer(b, FB_FRONT, &mut buf)
                        }).unwrap_or(0)
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
        if cfg.selftest && !want_snapshot && test.tick(&m, &link, started, cfg.selftest_control) {
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
                    format!("ok ata {from}..{to}: {hits} command(s) [{}]", first.join(" "))
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
                        out.bus_log = m
                            .mem
                            .watch_range_log
                            .drain()
                            .into_iter()
                            .map(|(pc, addr, v, us)| (pc, addr, v, us))
                            .collect();
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
                            watched[i].map(|w| format!("{w:#010x}")).unwrap_or_else(|| "unmapped".into())
                        ),
                        None => eprintln!("watch {addr:#010x} became unmapped at {executed} instructions"),
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
                write_restore_point(cfg, &m);
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
                    b.mem.range(other..other + (FB_W * FB_H * 2) as u32).filter(|(_, &p)| p != 0).count() as u32,
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
        out.stats = Stats { input_dropped: dropped, queued, ..stats };
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
        // **The boot is over when RetailOS starts listening, not at an instruction count.**
        //
        // This used to flip only at `snap_at`, which is 1.6 G instructions — a point chosen because
        // it is a good place to *resume from*, not because it is where the boot ends. RetailOS
        // reaches the language picker before it, so the bar went on filling over a machine that was
        // already up and taking input, and the "about N s left" beside it counted toward a number
        // nobody could observe. Reported from use, which is the only way this kind of thing is ever
        // found: *"the language screen was long there before it finished."*
        //
        // RetailOS writing `0x8001052a` to say it wants wheel frames. A machine asking for input is
        // a machine that has finished starting, and it is an observation rather than an assumption.
        //
        // The *sending* of the command, not the resulting flag: autonomous reporting is on at
        // reset (the part streams unless told not to, which is how Rockbox gets input without ever
        // sending this), so the flag is true from the first instruction and would end the boot
        // phase immediately.
        //
        // `snap_at` stays as a fallback for the case the signal never comes: a boot that fails
        // before the UI should not leave the window claiming to be booting for ever, and the old
        // behaviour is the honest thing to fall back *to*.
        if out.phase == (Phase::Booting { target: cfg.snap_at })
            && (stats.asked_for_frames || executed >= cfg.snap_at)
        {
            out.phase = Phase::Running;
        }
        if stop != Stop::BudgetExhausted && stop != Stop::Idle {
            out.phase = Phase::Stopped(format!("{stop:?} at {executed} instructions"));
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
fn write_restore_point(cfg: &Config, m: &Machine) {
    let Some(path) = &cfg.snapshot else { return };
    let img = m.snapshot();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(path, &img) {
        eprintln!("snapshot {}: {e}", path.display());
        return;
    }
    eprintln!("snapshot -> {} ({} bytes)", path.display(), img.len());
    match cfg.pair_with_drive() {
        Ok(line) => eprintln!("{line}"),
        // Not fatal, and deliberately not silent: without its other half the snapshot must not be
        // restored, so it goes and the next launch cold-boots.
        Err(e) => {
            eprintln!("{e} — this snapshot will not be restored");
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The machine stopped on its own. Keep the reason on screen and wait for a power command.
fn wait_after_stop(link: &Arc<Link>) -> Outcome {
    loop {
        if link.quit.load(Ordering::Relaxed) {
            return Outcome::Quit;
        }
        match link.inbox.lock().unwrap().cmds.pop_front() {
            Some(Cmd::PowerOff) => return Outcome::PoweredOff,
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
        let _ = std::fs::write(shots.join(format!("selftest-{name}.png")), crate::png::encode(&buf, FB_W, FB_H));
        (n, h)
    }

    /// Returns true when the test is finished and the run should stop.
    fn tick(&mut self, m: &Machine, link: &Arc<Link>, started: Instant, control: bool) -> bool {
        let now = m.executed as u64;
        // Whether RetailOS has ASKED, not whether the stream is on: reporting is on at reset now,
        // so the flag no longer marks the moment the firmware is ready for input.
        let reporting = m.mem.clickwheel.as_ref().is_some_and(|w| w.set_commands > 0);
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
                    if control { "CONTROL — no input" } else { "driving the wheel" }
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
    println!("selftest: {} instructions after the gate opened", m.executed as u64 - gate_at);
    println!(
        "  wheel: position {}, {} frames posted, {} dropped, {} suppressed",
        s.position, s.frames_posted, s.frames_dropped, s.frames_suppressed
    );
    println!(
        "  DATA reads {} ({} with a frame waiting), IRQ 40 asserted {} times",
        s.data_reads, s.data_reads_ready, s.irqs
    );
    println!("  queue drained: {} left, {} dropped", link.inbox.lock().unwrap().events.len(), s.input_dropped);
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
const PROBE_SAMPLES: [u64; 6] = [8_000_000, 40_000_000, 100_000_000, 200_000_000, 400_000_000, 800_000_000];

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
            samples: if cfg.samples.is_empty() { PROBE_SAMPLES.to_vec() } else { cfg.samples.clone() },
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
            self.shots.join(format!("probe-{}-{label}-back.png", self.arm)),
            crate::png::encode(&buf, FB_W, FB_H),
        );
        self.panel.push((label.to_string(), m.executed as u64, m.mem.usec, n, h, back, hb));
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
        println!("probe [{}]: {} instructions after acting", self.arm, m.executed as u64 - self.act_at);
        println!(
            "  wheel: buttons {:#07b}, {} frames posted, {} dropped, {} suppressed",
            s.buttons, s.frames_posted, s.frames_dropped, s.frames_suppressed
        );
        println!("  queue: {} left, {} dropped", link.inbox.lock().unwrap().events.len(), s.input_dropped);
        // The output stage, counted **in this session**: `Machine::restore` builds a fresh `Bcm`, so
        // on a restored run these start at zero even though the surface they filled is on screen.
        // That is exactly what makes them useful here — they say whether the co-processor was asked
        // to present anything *after* the input, which is a different question from whether
        // RetailOS received it.
        println!("  co-processor this session: {} commands kicked, {} frame updates", s.bcm_commands, s.bcm_frames);
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
            let (lo, hi): (Vec<u32>, Vec<u32>) =
                rows.iter().partition(|&&u| u < eapp_loader::BACKLIGHT_STEP_USEC);
            println!(
                "    pulse widths: {} pulses{} — {} under {} µs, {} over{}",
                w.seen(),
                if rows.len() as u64 == w.seen() { "" } else { " (SAMPLE, NOT A CENSUS)" },
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
    println!("  ata commands: {}", m.mem.ata.as_ref().map(|(_, d)| d.commands.seen()).unwrap_or(0));
    if let Some((_, d)) = &m.mem.ata {
        let names = |c: u8| match c {
            0x20 => "READ SECTORS", 0x24 => "READ SECTORS EXT", 0x25 => "READ DMA EXT",
            0xc8 => "READ DMA", 0x30 => "WRITE SECTORS", 0x34 => "WRITE SECTORS EXT",
            0x35 => "WRITE DMA EXT", 0xca => "WRITE DMA", 0xe7 => "FLUSH CACHE",
            0xea => "FLUSH CACHE EXT", 0xec => "IDENTIFY", 0xef => "SET FEATURES",
            0xe0 => "STANDBY IMMEDIATE", 0xe1 => "IDLE IMMEDIATE", _ => "",
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
    println!("  unmapped: {reads} reads, {writes} writes across {} pages", m.mem.unmapped.len());
    // Printed unconditionally when a range was asked for, including when it is empty. "Nothing was
    // recorded" and "nothing was asked" must not look the same, because an instrument that stays
    // silent when it found nothing is indistinguishable from one that is not running.
    if let Some((name, path)) = save {
        match m.mem.region_named(name) {
            Some(r) => match std::fs::write(path, &r.data) {
                Ok(()) => println!("  saved region {name}: {} bytes -> {}", r.data.len(), path.display()),
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
                    let who: Vec<String> =
                        w.pcs.iter().map(|(pc, n)| format!("{pc:#010x} x{n}")).collect();
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
        println!("  registers at {:#010x}:", m.mem.regs_at.map(|(a, _)| a).unwrap_or(0));
        for (at, r) in m.mem.regs_seen.iter() {
            println!("    at {at}:");
            for row in 0..4 {
                let cells: Vec<String> = (0..4).map(|c| {
                    let i = row * 4 + c;
                    format!("r{i:<2}={:#010x}", r[i])
                }).collect();
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
        let mut per: Vec<_> = count.iter().filter(|(_, c)| (8..=20).contains(*c)).collect();
        per.sort_by_key(|((f, _), _)| *f);
        for ((f, to), c) in per.iter().take(24) {
            println!("      {f:#010x} -> {to:#010x}  x{c}  first at {}", first[&(*f, *to)]);
        }
    }
    if let Some((lo, hi)) = m.mem.trace_pc {
        let n = m.mem.pc_trace.len();
        let distinct = m.mem.pc_trace.iter().map(|(pc, _)| *pc).collect::<std::collections::BTreeSet<_>>().len();
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
        let tail: Vec<String> = m.mem.pc_trace.iter().rev().take(24).rev()
            .map(|(pc, _)| format!("{pc:x}")).collect();
        if !tail.is_empty() {
            println!("    last: {}", tail.join(" "));
        }
        // And the distinct addresses in first-seen order, which for a dispatch table is the state
        // sequence rather than the instruction stream.
        let mut seen = std::collections::BTreeSet::new();
        let order: Vec<String> = m.mem.pc_trace.iter()
            .filter(|(pc, _)| seen.insert(*pc))
            .map(|(pc, _)| format!("{pc:x}")).take(60).collect();
        println!("    order: {}", order.join(" "));
    }
    if let Some(n) = &m.novelty {
        println!("  {} code buckets executed", n.len());
    }
    if let Some(b) = &m.mem.bcm {
        let mut buf = vec![0u8; FB_W * FB_H * 3];
        let n = read_framebuffer(b, FB_FRONT, &mut buf);
        println!("  bcm: {} kicked, {} frame updates", b.commands.len(), b.frames);
        println!("  framebuffer 0x000e0000: {n} non-black pixels of {}", FB_W * FB_H);
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
    println!("  {:.1} s wall, {:.2} M instructions/s", secs, m.executed as f64 / secs / 1e6);
}

#[cfg(test)]
mod tests {
    use super::*;

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
            flash: dir.join("f.bin"),
            disk: dir.join("d.img"),
            workdisk: dir.join("w.img"),
            frozen: frozen.clone(),
            clock: 5,
            snapshot: Some(snap.clone()),
            snap_at: 1,
            cold: false,
            work_on_copy: true,
            ..Default::default()
        };

        assert!(!cfg.may_restore(true), "a snapshot with no frozen drive is half a pair");
        std::fs::write(&frozen, b"drive").unwrap();
        assert!(cfg.may_restore(true), "both halves present");
        assert!(!cfg.may_restore(false), "a power cycle inside a session never restores");
        cfg.cold = true;
        assert!(!cfg.may_restore(true), "--cold overrides a complete pair");
        cfg.cold = false;
        std::fs::remove_file(&snap).unwrap();
        assert!(!cfg.may_restore(true), "a frozen drive with no snapshot is the other half");

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
            flash: dir.join("f.bin"),
            disk: drive.clone(),
            workdisk: drive.clone(),
            frozen: dir.join("b.frozen"),
            clock: 5,
            snapshot: Some(snap.clone()),
            snap_at: 1,
            cold: false,
            work_on_copy: false,
            ..Default::default()
        };

        assert!(!cfg.may_restore(true), "no stamp yet, so there is no pair");
        cfg.pair_with_drive().expect("stamping a drive that exists must work");
        assert!(cfg.may_restore(true), "stamped, and nothing has touched the drive since");

        // What iTunes, `make-disk` or a second window does to it.
        std::fs::write(&drive, b"the drive after something else wrote to it").unwrap();
        assert!(!cfg.may_restore(true), "the drive moved, so the snapshot no longer describes it");

        // And the case where the stamp itself is unreadable or from another build.
        cfg.pair_with_drive().unwrap();
        assert!(cfg.may_restore(true), "re-stamped");
        std::fs::write(cfg.stamp().unwrap(), b"not a fingerprint").unwrap();
        assert!(!cfg.may_restore(true), "a stamp that does not parse is not a match");

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
        assert_eq!(std::fs::read(&work).unwrap(), b"PRISTINE", "self-clone must not empty the drive");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
