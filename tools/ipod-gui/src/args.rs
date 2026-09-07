//! The command line: what this binary answers without opening a window, and what it refuses.
//!
//! **There is no toolkit in this file**, and none may enter it — the same rule `rail.rs`, `nav.rs`
//! and `work.rs` keep. It names no generated type and takes no window; [`parse`] is a pure function
//! of a `&[String]`, and [`run`] writes into two writers a test can hand it. That is what makes
//! every sentence below checkable with no display.
//!
//! ## Why this file exists at all
//!
//! `fn main` went straight to the window and read `argv` **nowhere**. Every flag the program used
//! to take died with the egui `main.rs`, and two workflow steps did not notice: CI ran
//! `--check-update` and `--help` against a binary that recognised neither — the first was ignored
//! and the second opened a window, which on a runner is a job that hangs or a backend that fails.
//! An unrecognised flag opening a window is the worst of the three possible answers, because it
//! looks like the flag worked.
//!
//! ## The rule this file is built around
//!
//! **A flag that is listed is a flag that works, and a flag that is named is a flag that answers.**
//! The old `--help` broke the second half of that in both directions at once: it described 38 flags
//! and the program honoured 45, so seven working flags — `--boot=`, `--press=`, `--second-core`,
//! `--shot-after=`, `--window-shot=`, `--make-app` and `--help` itself — were undocumented. Counted
//! off `65fecaf^`'s `main.rs`, not remembered.
//!
//! Here, [`FLAGS`] is the closed set the parser accepts and [`HELP`] is the text a person reads, and
//! three tests hold them against each other: every entry in `FLAGS` appears in `HELP`, every flag
//! token in `HELP` is in `FLAGS`, and every entry in `FLAGS` parses to something that is not a
//! refusal. Deleting a `parse` arm, a `FLAGS` entry, or a `HELP` line each turns a **different** one
//! of the three red, which is the property a single table generating both halves would not have.
//!
//! ## And the thirty-nine that are not here
//!
//! [`RETIRED`] is the rest of that count. They are not silently ignored and they are not quietly
//! accepted: naming one prints what it used to do and why it is absent, and exits 2. A flag restored
//! into a program that cannot honour it is worse than one left out — but so is a flag that
//! disappears without saying it ever existed, because the person typing it has a script that used
//! to work.

use std::io::Write;
use std::path::PathBuf;

use ipod_machine::settings::Settings;

use crate::{bundle, update};

/// What one launch was asked to do.
///
/// Every variant but [`Cli::Window`] runs with **no window at all**, so all of them work over SSH
/// and on a CI runner with no display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cli {
    /// Open the window — the only path that reaches the toolkit. Carries the four flags that
    /// configure the machine it will start (all-`false` is a plain launch), and the one flag that
    /// configures no machine at all: whether this launch can be reached from outside.
    Window {
        launch: Machine,
        /// `--control=PATH`: bind a control socket there before the window opens.
        ///
        /// **Not a [`Machine`] field, and the reason is the test that struct's own doc applies.**
        /// `--cold`, `--clock=`, `--second-core` and `--charger` each write onto an
        /// `emu::Config`; this one writes onto nothing, because it is not a property of the
        /// machine but of the *process* — a second way in, for whoever is not sitting in front of
        /// the window. `None` is a launch nothing outside can reach, which stays the default.
        control: Option<PathBuf>,
    },
    /// Run N instructions with no window and print the fingerprint, then exit.
    ///
    /// **The self-check that this front end and `retail-boot.sh` are running the same machine.**
    /// The disagreement between the two is what `emu.rs`'s own header says this flag exists to make
    /// *"a number rather than an impression"*, and it was refused for as long as nothing called
    /// `emu::run`.
    Headless {
        machine: Machine,
        budget: u64,
        /// `--control=PATH`, the same socket. A headless run is the only launch an automated
        /// test can make with no display at all, so it is the one that most needs driving —
        /// and a driven run's fingerprint is not a boot's, which [`run`] says out loud.
        control: Option<PathBuf>,
    },
    /// Print [`HELP`] and exit 0.
    Help,
    /// Ask GitHub for the latest release. Exit 0 whether or not the network answers.
    CheckUpdate,
    /// Parse a NOR dump and a drive image, say what they are, exit 0 if both are usable.
    ///
    /// `None` on either means *whatever the window last recorded*, resolved in [`run`] rather than
    /// here so that parsing stays a pure function of the arguments.
    CheckImages {
        flash: Option<PathBuf>,
        disk: Option<PathBuf>,
    },
    /// Wrap **this** binary in a macOS `.app` bundle under `out`.
    MakeApp {
        out: PathBuf,
        icon: Option<PathBuf>,
    },
    /// The command line was not one this build can answer. The string is the whole of what to print;
    /// it names the offending word and says what to do instead.
    Refused(String),
}

/// **The four flags that configure a machine this window starts**, and nothing else.
///
/// Every one of them is a property of a **launch**, not of an iPod, which is the whole test this
/// struct applies: a flag that describes the *device* belongs in the library beside the device, and
/// a second place to say it is a second answer that can disagree. See [`RETIRED`]'s
/// [`Gone::Device`] arm for the five that failed that test and stay out because of it.
///
/// `--headless=N` is the fifth flag of the five and is not here: it is not a property of the
/// machine but a decision not to open a window at all, so it is a [`Cli`] verb.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Machine {
    /// `--cold`: ignore any restore point on disk, for **this launch**.
    ///
    /// §12.4's park writes one per device now, so a session that wants a cold boot has to be able
    /// to say so — and it is per launch rather than per device because *"give me the machine from
    /// scratch this time"* is not a fact about the iPod. §7.3's `Cold boot` row is the in-window
    /// equivalent and is per press.
    pub cold: bool,
    /// `--clock=N`: interpreter instructions per simulated microsecond. `None` is
    /// `ipod_machine::CLOCK`.
    ///
    /// **A property of the experiment, not of the iPod.** Every recipe in `research/` states its
    /// clock, and a device that stored one would make two runs of the same iPod incomparable for a
    /// reason nobody could see on either.
    pub clock: Option<usize>,
    /// How many cores were **asked for**: `Some(true)` for `--second-core`, `Some(false)` for
    /// `--no-second-core`, `None` for neither.
    ///
    /// An `Option` rather than a `bool` because both flags exist and a `bool` can only carry one
    /// of them. The default is two cores — the part has two — so `--second-core` now asks for what
    /// it would get anyway and `--no-second-core` is the one that changes the machine. Keeping
    /// `--second-core` spelled out is deliberate: it appears in `research/` recipes, and a flag
    /// that a recipe names must not become an argument this program shrugs at.
    pub cores: Option<bool>,
    /// `--charger`: hold `GPIOL` bit 3 low, which is what RetailOS's charger sense reads.
    ///
    /// A property of what the iPod is plugged into, which is neither the device nor the experiment
    /// — and it is the only configuration in which there is a charging screen to return to.
    pub charger: bool,
}

impl Machine {
    /// Write these four onto a config the window has already built from the library.
    ///
    /// **One place, and it only ever writes what was asked for.** `clock` is an `Option` precisely
    /// so that *not saying* leaves `machine_config`'s measured clock standing rather than
    /// overwriting it with a zero — which `emu::build` clamps to 1, a machine running at one
    /// seventy-fifth of the part and reported as though it were the part.
    ///
    /// **`--clock=` moves the wheel's click spacing with it, and that is not optional.** The gap is
    /// 4 ms of the *iPod's* time, so it is `4000 × clock` and nothing else; a launch that changed
    /// one and left the other is the defect `--wheel-click-instr` shipped with for three weeks,
    /// arriving through a different door. `ipod_machine::pace::wheel_click_gap` is the rule, in one
    /// place, and this is the third caller of it.
    pub fn apply(&self, cfg: &mut crate::emu::Config) {
        cfg.cold |= self.cold;
        if let Some(two) = self.cores {
            cfg.one_core = !two;
        }
        cfg.charger |= self.charger;
        if let Some(n) = self.clock {
            cfg.clock = n;
            cfg.click_gap = ipod_machine::pace::wheel_click_gap(n);
        }
    }
}

/// Why a flag the old window took is not here.
///
/// **`Gone::Machine` retired with this pass and its sentence went with it.** It read *"it configured
/// the emulator, and this build starts no machine yet — `emu.rs` is compiled and tested, and
/// nothing calls it"*, which was true of thirty-one flags for exactly as long as `Verb::Start`
/// ended `Kind::Planned`. `emu::run` has a caller. A refusal whose stated reason has become false
/// is worse than the flag being absent, because a person reading it goes looking for the thing that
/// is supposedly missing.
///
/// So the thirty-one are re-sorted into the three answers that are actually true of them, and five
/// of them stop being refusals at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gone {
    /// It drove the window that was deleted: its modes, its drop path, its screenshot key.
    Window,
    /// **It says something about a device, and a device is where the library says it.**
    ///
    /// The five: `--snapshot=` and `--workdisk=` are paths the model derives from a device's name
    /// (`Settings::restore_point`); `--copy` / `--no-copy` are `Device::work_on_copy`, which §11.2
    /// writes and §11.4 draws; `--boot=` is §12.5's `Start into` rows. Restoring any of them would
    /// be a second place to say one fact, and the two would disagree the first time somebody used
    /// both.
    Device,
    /// **It is a terminal instrument, and §12.9 keeps those out of the window on purpose.**
    ///
    /// *"These are terminal instruments for a person already holding a hypothesis, and putting them
    /// in a window would make it a debugger — which is the one thing this thesis must not become."*
    /// `trace` and `ipod-boot` take them, this binary does not, and that is a boundary rather than
    /// a gap.
    ///
    /// **`--control=` was in here and is not any more.** Its sentence read *"a socket that appears
    /// without being asked for is an interface nobody audited"*, which is an argument for the
    /// socket being **opt-in** and was doing duty as an argument for it being **unreachable**:
    /// `control.rs` shipped a working protocol that nothing could open, under a refusal saying it
    /// belonged to `trace`. It does not — `trace --drive` drives `trace`'s own machine from stdin,
    /// and cannot see the window's. Opt-in it stays; refused it no longer is.
    Instrument,
}

impl Gone {
    /// The sentence a refusal prints after naming the flag.
    fn because(self) -> &'static str {
        match self {
            Gone::Window => "it drove the window that was replaced, and that window is gone",
            Gone::Device => {
                "it says something about a device, and this build keeps that in the library beside \
                 the device rather than on a command line — open the drawer at Devices"
            }
            Gone::Instrument => {
                "it is a measurement instrument, and this window is deliberately not a debugger — \
                 `trace` and `ipod-boot` take it"
            }
        }
    }
}

/// The flags this build honours. **Closed set**, and the parser accepts nothing outside it.
///
/// A trailing `=` is part of the spelling: `--flash=` takes its value attached, and `--flash` on its
/// own is not this flag.
pub const FLAGS: &[&str] = &[
    "-h",
    "--help",
    "--check-update",
    "--check-images",
    "--flash=",
    "--disk=",
    "--make-app",
    // **The five that came back with `emu::run`'s first caller.** Four of them configure the
    // machine a window starts ([`Machine`]) and the fifth is a run with no window at all. Each was
    // refused with *"this build starts no machine yet"* until this pass, and each is here rather
    // than in [`RETIRED`] because that sentence stopped being true.
    "--headless=",
    "--cold",
    "--clock=",
    "--second-core",
    "--no-second-core",
    "--charger",
    // **The sixth, and it configures no machine.** It says this launch may be driven by something
    // that is not a person — which is a property of the process, so it rides beside [`Machine`]
    // on both verbs that start one rather than inside it.
    "--control=",
];

/// The flags the window before this one took, and why each is absent.
///
/// Enumerated off `65fecaf^:tools/ipod-gui/src/main.rs` — the union of what `print_help` described
/// and what `config` and `main` actually read. The zenity and kdialog arguments that file also
/// spelled with two dashes are not flags of this program and are not here.
///
/// **It was thirty-nine and is thirty-three**, and the six that left are the ones whose stated
/// reason expired: `--headless=`, `--cold`, `--clock=`, `--second-core` and `--charger` were
/// refused with *"this build starts no machine yet"*, and this build starts machines; `--control=`
/// was refused as an instrument belonging to `trace`, and `trace` has never had it. The rest are
/// re-sorted onto [`Gone::Device`] and [`Gone::Instrument`] — two answers that are true of them
/// rather than one that stopped being.
const RETIRED: &[(&str, Gone)] = &[
    ("--ablate=", Gone::Instrument),
    // §12.5's `Start into` rows, on the device's drawer page.
    ("--boot=", Gone::Device),
    ("--clock-v3", Gone::Instrument),
    ("--cop-awake", Gone::Instrument),
    // `Device::work_on_copy`. §11.2 writes it, §11.4 draws it, and `machine_config` reads it.
    ("--copy", Gone::Device),
    ("--debug", Gone::Window),
    ("--input-regs=", Gone::Instrument),
    ("--ipsw=", Gone::Window),
    ("--no-copy", Gone::Device),
    ("--no-ide-irq-latch", Gone::Instrument),
    ("--no-idle-stop", Gone::Instrument),
    ("--power-cycle-at=", Gone::Instrument),
    ("--press=", Gone::Window),
    ("--probe=", Gone::Instrument),
    ("--probe-at=", Gone::Instrument),
    ("--profile", Gone::Instrument),
    ("--read-count=", Gone::Instrument),
    ("--regs-at=", Gone::Instrument),
    ("--samples=", Gone::Instrument),
    ("--save-region=", Gone::Instrument),
    ("--selftest", Gone::Instrument),
    ("--selftest-control", Gone::Instrument),
    ("--shot-after=", Gone::Window),
    // **Neither a device's nor a launch's**, which is why it is `Instrument` and not `Device`: it
    // is `emu::SNAP_AT`, the fallback that ends `Phase::Booting`, and §12.4 parks at the quit
    // instead of at an instruction count. A flag that moved it would move where the boot phase
    // ends, which is a measurement decision.
    ("--snap-at=", Gone::Instrument),
    // `Settings::restore_point(name)` — one per device, under its own name. A flag pointing two
    // devices at one file is the stale-pair defect `Config::frozen` documents, with a new entrance.
    ("--snapshot=", Gone::Device),
    ("--trace-calls-from=", Gone::Instrument),
    ("--trace-pc=", Gone::Instrument),
    ("--user", Gone::Window),
    ("--watch=", Gone::Instrument),
    ("--watch-writes=", Gone::Instrument),
    ("--wheel-click-instr=", Gone::Instrument),
    ("--window-shot=", Gone::Window),
    // Derived from `Device::work_on_copy` and the drive it names; there is no third thing to say.
    ("--workdisk=", Gone::Device),
];

/// What `--help` prints, verbatim.
///
/// **Hand-written rather than generated from [`FLAGS`]**, and that is the point rather than a
/// shortcut not taken: a table that produced both halves could lose a flag from both at once and
/// stay green through it. Two texts held against each other by two tests can each catch the other
/// losing something. The version is filled in at run time by [`help`].
const HELP: &str = "\
ipod-emulator {v} — an interactive iPod 5G over the ipod-machine emulator

With no arguments it opens the window. These run and exit instead, with no window at all,
so they work over SSH and on a runner with no display:

  -h, --help              print this and exit
  --check-update          ask GitHub for the latest release, and print what it said. Silent
                          when offline; exit 0 either way, so it cannot fail a build
  --check-images          parse a NOR dump and a drive image, say what each is, and exit 0
                          only if both are usable. Takes no window and no machine
    --flash=FILE          the NOR dump to check. Default: the one the window last recorded
    --disk=FILE           the drive image to check. Default: as above
  --make-app OUT [ICON]   macOS only: write OUT/ipod-emulator.app around THIS binary, so the
                          bundle cannot be built around a stale one. ICON is a PNG and is
                          optional — without it the bundle simply has no icon. Nothing is
                          signed and nothing is notarised
  --headless=N            run the device the window last used for N instructions with no
                          window, and print the fingerprint. Always a cold boot: a restore
                          point is not a boot, and a fingerprint of one is not comparable

And these open the window as usual, and configure the machine it starts:

  --cold                  ignore this device's restore point for this launch. The bench's own
                          Cold boot is the same decision, one press at a time
  --clock=N               interpreter instructions per simulated microsecond. 5 is what every
                          recipe in research/ uses; 75 is the real part, and is the default
  --second-core           run the PP5021's coprocessor. The default since 2026-09-05 — the
                          part has two cores, so the machine has two
  --no-second-core        run one core instead of two. The ablation, not a setting: one core
                          drops five wheel frames in six and puts bypass #7 back
  --charger               plug the mains in — the only configuration in which there is a
                          charging screen to return to

And one that configures no machine at all:

  --control=PATH          bind a control socket at PATH, before the window opens and for as
                          long as the process lives. A drawn window publishes no accessibility
                          elements, so this is the only way anything that is not a person can
                          scroll the wheel, press a button, read a word of memory or take a
                          screenshot. Absent unless asked for. Connect to the Unix socket, one
                          command per line, and type `help` for the vocabulary. Legal beside
                          --headless=N too, which is the one launch that needs no display —
                          and a run somebody drove is a run whose fingerprint is not a boot's

Every other flag the window before this one took either drove the window that was replaced,
says something about a device (which the library holds, beside the device), or is a
measurement instrument that belongs to trace and ipod-boot. Naming one says which and exits
2, rather than being ignored while a window opens.
";

/// [`HELP`] with this build's version in it.
pub fn help() -> String {
    HELP.replace("{v}", update::VERSION)
}

/// Turn `argv` — **without** the program name — into the one thing this launch was asked to do.
///
/// Pure: no filesystem, no network, no environment. Everything that has to ask the machine a
/// question happens in [`run`].
///
/// `--help` wins over everything, including a malformed rest of the line, because somebody who has
/// got the line wrong is exactly who is asking.
pub fn parse(args: &[String]) -> Cli {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        return Cli::Help;
    }

    let mut verb: Option<Cli> = None;
    let mut flash: Option<PathBuf> = None;
    let mut disk: Option<PathBuf> = None;
    // §12's four, accumulated across the line and attached to whichever of the two verbs that
    // start a machine came out of it. Anything else on the line leaves them unread, which is
    // refused below for the same reason `--flash=` beside `--make-app` is.
    let mut machine = Machine::default();
    let mut asked_for_machine = false;
    // The socket is neither a verb nor a machine flag: it rides on whichever of the two verbs that
    // start a machine came out of the line, and beside anything else it would be a flag read by
    // nothing — which is refused below, exactly as `--cold` beside `--check-update` is.
    let mut control: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let taken = match a {
            "--check-update" => Some(Cli::CheckUpdate),
            _ if a.starts_with("--headless=") => {
                let n = a.trim_start_matches("--headless=");
                match n.parse::<u64>() {
                    // Zero is not a run: `emu::run`'s headless arm stops when `executed >= limit`,
                    // which is true before the first instruction, so this would print a
                    // fingerprint of a machine that had done nothing and exit 0.
                    Ok(0) | Err(_) => {
                        return Cli::Refused(format!(
                            "--headless={n}: how many instructions to run has to be a whole number \
                             above zero — `--headless=200000000` is the usual self-check."
                        ))
                    }
                    Ok(budget) => Some(Cli::Headless {
                        machine: Machine::default(),
                        budget,
                        control: None,
                    }),
                }
            }
            "--check-images" => Some(Cli::CheckImages {
                flash: None,
                disk: None,
            }),
            "--make-app" => {
                // The operands are positional and follow immediately. A word starting with `-` is
                // the next flag, not a path: `--make-app --check-update` must refuse for having two
                // verbs, not silently write a bundle into a directory called `--check-update`.
                let mut rest = args[i + 1..].iter().take_while(|s| !s.starts_with('-'));
                let Some(out) = rest.next() else {
                    return Cli::Refused(
                        "--make-app: where to write it is not optional. \
                         `--make-app OUT [ICON]`, and OUT is a directory that will hold \
                         `ipod-emulator.app`."
                            .into(),
                    );
                };
                let icon = rest.next();
                i += 1 + usize::from(icon.is_some());
                Some(Cli::MakeApp {
                    out: PathBuf::from(out),
                    icon: icon.map(PathBuf::from),
                })
            }
            _ => None,
        };
        if let Some(v) = taken {
            if let Some(had) = &verb {
                return Cli::Refused(format!(
                    "{}: this build does one thing per run, and `{}` was already asked for.",
                    a,
                    name_of(had)
                ));
            }
            verb = Some(v);
            i += 1;
            continue;
        }
        if let Some(p) = a.strip_prefix("--control=") {
            // **Not a verb**: it opens nothing on its own and it does not decide what this launch
            // is. It sits here with the operands and the machine flags because it is one — a
            // modifier on whichever verb the rest of the line names.
            //
            // **An empty path is refused rather than bound.** `UnixListener::bind("")` fails with
            // a message about a file nobody named, and a launch that typed `--control=` meaning
            // `--control=/tmp/ipod.sock` would read that as the socket being unavailable rather
            // than as the line being wrong.
            if p.is_empty() {
                return Cli::Refused(
                    "--control=: where to bind the socket is not optional. \
                     `--control=/tmp/ipod.sock`, and whatever is already at that path is replaced \
                     — a stale socket is litter, not a lock."
                        .into(),
                );
            }
            control = Some(PathBuf::from(p));
        } else if let Some(p) = a.strip_prefix("--flash=") {
            flash = Some(PathBuf::from(p));
        } else if let Some(p) = a.strip_prefix("--disk=") {
            disk = Some(PathBuf::from(p));
        } else if a == "--cold" {
            machine.cold = true;
            asked_for_machine = true;
        } else if a == "--second-core" {
            machine.cores = Some(true);
            asked_for_machine = true;
        } else if a == "--no-second-core" {
            machine.cores = Some(false);
            asked_for_machine = true;
        } else if a == "--charger" {
            machine.charger = true;
            asked_for_machine = true;
        } else if let Some(n) = a.strip_prefix("--clock=") {
            // **Zero is refused rather than clamped.** `emu::build` does `cfg.clock.max(1)`, so
            // `--clock=0` would silently run the part at one instruction per simulated microsecond
            // — a machine seventy-five times slow, reported as though it were the part, with the
            // flag on the command line saying it had been set to nothing.
            match n.parse::<usize>() {
                Ok(0) | Err(_) => {
                    return Cli::Refused(format!(
                        "--clock={n}: instructions per simulated microsecond has to be a whole \
                         number above zero. 5 is what every recipe in research/ uses; {} is the \
                         real part and is the default.",
                        ipod_machine::CLOCK
                    ))
                }
                Ok(v) => {
                    machine.clock = Some(v);
                    asked_for_machine = true;
                }
            }
        } else {
            return Cli::Refused(refusal(a));
        }
        i += 1;
    }

    match verb {
        // **First, because the two arms below it would swallow it.** A socket asked for beside a
        // verb that starts no machine would answer `state` with *nothing on the bench* for as long
        // as the process lived, and `--check-images` lives for milliseconds: an interface that is
        // technically there and can never be used. Same rule as the machine flags one arm down,
        // and it has to be tested first because `--check-images` has an arm of its own that
        // returns before any general guard could see it.
        Some(ref v) if control.is_some() && !matches!(v, Cli::Headless { .. }) => {
            Cli::Refused(format!(
                "--control= opens a socket onto a running machine, and `{}` starts none. It goes \
                 on a plain launch — the window — or beside --headless=N.",
                name_of(v)
            ))
        }
        Some(Cli::CheckImages { .. }) if asked_for_machine => Cli::Refused(
            "--cold, --clock=, --second-core and --charger configure a machine, and \
             --check-images starts none: it parses two files and exits."
                .into(),
        ),
        Some(Cli::CheckImages { .. }) => Cli::CheckImages { flash, disk },
        // The four ride on whichever verb starts a machine. `--headless=` is the only one that is
        // not the window, and it takes the same four for the same reason: a fingerprint taken at a
        // different clock is a different number. `--control=` rides with them, for the reason its
        // own field says: it is the process's, and both of these verbs make a process with a
        // machine in it.
        Some(Cli::Headless { budget, .. }) => Cli::Headless { machine, budget, control },
        // …and a machine flag beside a verb that starts nothing is an unread flag, which is the
        // whole defect this file exists to remove.
        Some(v) if asked_for_machine => Cli::Refused(format!(
            "--cold, --clock=, --second-core and --charger configure the machine the window \
             starts, and `{}` opens no window.",
            name_of(&v)
        )),
        // **`--flash=` and `--disk=` are operands of `--check-images` and of nothing else here**,
        // so anything else on the line leaves them unread — and an unread flag is the whole defect
        // this file exists to remove. The old window took them as *launch* configuration for a
        // machine it then booted; accepting them beside another verb, or beside no verb at all,
        // would be those flags surviving in spelling only.
        v if flash.is_some() || disk.is_some() => Cli::Refused(format!(
            "--flash= and --disk= are read by --check-images and by nothing else in this build. {}",
            match &v {
                Some(had) => format!("`{}` does not take them.", name_of(had)),
                None => "Add --check-images, which does.".to_string(),
            }
        )),
        Some(v) => v,
        None => Cli::Window { launch: machine, control },
    }
}

/// The flag a verb was spelled with, for a message that has to name two of them.
fn name_of(v: &Cli) -> &'static str {
    match v {
        Cli::Window { .. } => "(none)",
        Cli::Headless { .. } => "--headless=",
        Cli::Help => "--help",
        Cli::CheckUpdate => "--check-update",
        Cli::CheckImages { .. } => "--check-images",
        Cli::MakeApp { .. } => "--make-app",
        Cli::Refused(_) => "(a refusal)",
    }
}

/// What to print about a word this build does not take.
///
/// Three answers, in order of how much the person already had right.
///
/// A word that was a flag of the old window is answered **as one**, because whoever typed it has a
/// script that used to work and is owed the reason rather than "unknown option". A live flag spelled
/// the other way round is answered as a spelling, because `--flash FILE` and `--flash=FILE` are one
/// keystroke apart and "not a flag of this build" is a wrong answer to the first of them. Everything
/// else is named and pointed at `--help`.
///
/// **This is the only thing that ships reading [`FLAGS`]**, and it is why that table is not
/// `#[cfg(test)]`: the sweeps hold the table against the help text, and this holds it against what
/// somebody actually typed.
fn refusal(word: &str) -> String {
    // `--flash=x` and `--flash` share a head. The tables spell an attached value with a trailing
    // `=`, so comparing heads is what lets one lookup answer both spellings.
    let head = word.split_once('=').map_or(word, |(h, _)| h);
    let attached = word.contains('=');

    if !attached && FLAGS.contains(&format!("{head}=").as_str()) {
        return format!(
            "{head} takes its value attached, with no space: `{head}=FILE`. \
             `--help` shows both of them in place."
        );
    }
    if attached && FLAGS.contains(&head) {
        return format!(
            "{head} takes what follows it as separate words, not attached with `=`. \
             `--help` shows the shape."
        );
    }
    if let Some((f, gone)) = RETIRED
        .iter()
        .find(|(f, _)| *f == word || f.trim_end_matches('=') == head)
    {
        return format!(
            "{f} is not a flag of this build: {}. `--help` lists the ones that are.",
            gone.because()
        );
    }
    if word.starts_with('-') {
        return format!("{word} is not a flag of this build. `--help` lists the ones that are.");
    }
    // A bare word. The old window took files by being dropped on, which is a gesture and not an
    // argument; it never took one from the command line either, so this is not a regression being
    // announced — it is a mistake being named rather than ignored.
    format!(
        "{word}: this build takes no file arguments. `--check-images --flash=FILE --disk=FILE` \
         inspects a pair without opening anything."
    )
}

/// Do it, and return the process's exit code.
///
/// Two writers rather than `println!`, so a test reads exactly what a person reads. `out` is the
/// answer and `err` is the complaint, which is the split a shell script depends on.
///
/// **One thing does not go through `out`**, and it is worth saying rather than hiding: the two
/// reports `--check-images` prints are `ipod_machine::inspect::report`'s own, written to stdout by
/// the crate that knows how to read a NOR dump. Routing them through here would mean a second
/// implementation of the same paragraph, and the two would drift. Its exit code is this function's.
pub fn run(cli: &Cli, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    match cli {
        // The one path that reaches the toolkit does not come through here at all — `main` matches
        // it off before calling this, so nothing in this file can open a window by accident.
        Cli::Window { .. } => 0,
        // ── `--headless=N`: the self-check, and it is the reason this flag came back ────────────
        //
        // `emu.rs`'s own header: *"a GUI that runs the machine differently from the recipes is a
        // GUI that measures a different iPod, and `--headless` exists so the disagreement is a
        // number rather than an impression."* It has been unanswerable for as long as nothing in
        // this build called `emu::run`.
        //
        // **The device is the one the window last used**, not a pair of paths: `machine_config` is
        // the same function the press goes through, so a headless run and a press build the same
        // machine out of the same library. `--flash=` and `--disk=` deliberately do not reach it —
        // they are `--check-images`'s operands, and a second way to name a machine's parts is the
        // thing this file exists to stop.
        Cli::Headless { machine, budget, control } => {
            let saved = Settings::load();
            let Some(name) = saved.current.clone() else {
                let _ = writeln!(
                    err,
                    "--headless: no device has been used yet, so there is nothing to run. Open \
                     the window once and start one."
                );
                return 2;
            };
            let Some(mut cfg) = crate::machine_config(&saved, &name) else {
                let _ = writeln!(
                    err,
                    "--headless: `{name}` names no drive this build can open. Open the window and \
                     check it under Devices."
                );
                return 2;
            };
            cfg.headless = Some(*budget);
            // **Always a cold boot, and this is the line that makes it one.** §12.4's park writes
            // a restore point per device, so without this a self-check taken after a park would
            // resume a machine at 1.6 G instructions and print a fingerprint of the *restore* —
            // a number that looks like a boot's and is not comparable to `retail-boot.sh`'s.
            cfg.snapshot = None;
            machine.apply(&mut cfg);
            let _ = writeln!(
                out,
                "headless: {name} for {budget} instructions, {} per simulated microsecond{}",
                cfg.clock,
                if cfg.one_core { ", ONE core" } else { ", two cores" }
            );
            // **The socket, if this launch asked to be driven.** It goes up before the machine
            // runs and it is fatal if it cannot: a headless run that was asked for a socket and
            // silently produced none would look, from the outside, exactly like a socket nobody
            // could connect to — which is the defect this flag exists to close.
            let link = crate::emu::Link::new();
            if let Some(path) = control {
                // **A driven run must not stop itself for being idle, and this line was measured
                // rather than reasoned.** `emu::build` arms `stop_when_idle = 400_000_000` for
                // every headless run, because the self-check has to be comparable to
                // `retail-boot.sh --stop-when-idle=400000000`. A driver is idle by nature — it
                // waits for a person or a script to decide the next command — so the first
                // driven run ended itself with `Idle after 528079424 instructions` mid-session
                // and took the socket with it: a connection dropped, from the outside, exactly
                // like a socket that had never worked. Comparability is already forfeit here (the
                // sentence below says so), so the heuristic buys nothing and costs the session.
                cfg.no_idle_stop = true;
                crate::control::attach(&link);
                if let Err(why) = crate::control::serve(path) {
                    let _ = writeln!(err, "--control: {why}");
                    return 2;
                }
                let _ = writeln!(out, "{}", crate::control::listening(path));
                // **Said out loud, because the number below stops being what it says it is.**
                // `--headless=N` is the self-check whose fingerprint is compared against
                // `retail-boot.sh` line for line; a wheel click delivered through the socket makes
                // the machine execute code that a boot does not. A driven run may still be
                // measured — it just cannot be compared to one nobody touched.
                let _ = writeln!(
                    out,
                    "headless: this run can be driven from outside, so its fingerprint is \
                     comparable only to another driven run — and it will not stop itself for \
                     being idle, because a driver waiting for its next command is not a machine \
                     that has finished."
                );
            }
            // The report goes to `println!` inside the run loop rather than into `out`, because it
            // is the same `report_headless` every other front end prints and reformatting it here
            // would make two spellings of one measurement.
            crate::emu::run(cfg, link);
            0
        }
        Cli::Help => {
            let _ = write!(out, "{}", help());
            0
        }
        Cli::CheckUpdate => {
            match update::check() {
                Some(f) => {
                    let _ = writeln!(out, "{}", f.line());
                }
                // Offline is the expected answer, not a failure, and this is the only place it is
                // ever said out loud — the window says nothing at all when a check does not answer.
                None => {
                    let _ = writeln!(
                        out,
                        "No answer from GitHub. That is the expected result offline, and it is not \
                         an error: nothing was downloaded, nothing was changed, and this build is \
                         {}.",
                        update::VERSION
                    );
                }
            }
            0
        }
        Cli::CheckImages { flash, disk } => {
            // Settings are read **only** when something is missing from the command line. A pair
            // given in full is answered without touching the operator's file at all, which is what
            // makes this path identical in CI and on a machine that has been used.
            let (flash, disk) = match (flash, disk) {
                (Some(f), Some(d)) => (Some(f.clone()), Some(d.clone())),
                _ => {
                    let saved = Settings::load();
                    (
                        flash.clone().or_else(|| saved.flash()),
                        disk.clone().or_else(|| saved.disk.clone()),
                    )
                }
            };
            match (flash, disk) {
                (Some(f), Some(d)) => ipod_machine::inspect::report(&f, &d),
                (f, d) => {
                    // Naming the half that is missing, rather than reporting `UNREADABLE ` against
                    // an empty path — which would be this program inventing a verdict about a file
                    // nobody named.
                    let _ = writeln!(
                        err,
                        "--check-images: nothing to check{}. Name them: --check-images \
                         --flash=FILE --disk=FILE.",
                        match (f.is_none(), d.is_none()) {
                            (true, true) => " — no NOR dump and no drive image",
                            (true, false) => " — no NOR dump",
                            _ => " — no drive image",
                        }
                    );
                    2
                }
            }
        }
        Cli::MakeApp { out: dir, icon } => match bundle::make_app(dir, icon.as_deref()) {
            Ok(app) => {
                let _ = writeln!(out, "{}", app.display());
                0
            }
            Err(e) => {
                let _ = writeln!(err, "--make-app: {e}");
                1
            }
        },
        Cli::Refused(why) => {
            let _ = writeln!(err, "{why}");
            2
        }
    }
}

/// Every flag-shaped word in a text, normalised to the spelling [`FLAGS`] uses.
///
/// `--flash=FILE` is the flag `--flash=`; `--check-images,` at the end of a sentence is
/// `--check-images`. Used by the two sweeps below and by nothing that ships.
#[cfg(test)]
fn flag_tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let word = word.trim_matches(|c: char| c == ',' || c == '.' || c == '`' || c == ';');
        if !word.starts_with('-') || word.len() < 2 {
            continue;
        }
        let token = match word.find('=') {
            Some(i) => &word[..=i],
            None => word,
        };
        if !out.iter().any(|s| s == token) {
            out.push(token.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    /// `run`'s two writers, as strings.
    fn ran(cli: &Cli) -> (i32, String, String) {
        let (mut o, mut e) = (Vec::new(), Vec::new());
        let code = run(cli, &mut o, &mut e);
        (
            code,
            String::from_utf8(o).expect("stdout is text"),
            String::from_utf8(e).expect("stderr is text"),
        )
    }

    // ── The three sweeps that keep `--help` true ────────────────────────────────────────────────
    //
    // Each catches a different one of the three ways one flag can go missing, and that is the whole
    // design: a single table feeding both the parser and the help text would lose a flag from both
    // in one edit and report nothing.

    /// **Delete a line from [`HELP`] and this is what goes red.**
    #[test]
    fn every_flag_this_build_takes_is_in_the_help_text() {
        let help = help();
        let missing: Vec<&&str> = FLAGS.iter().filter(|f| !help.contains(**f)).collect();
        assert!(
            missing.is_empty(),
            "{missing:?} are flags the parser accepts and `--help` does not mention. A flag \
             nobody can find is a flag nobody has"
        );
        assert!(FLAGS.len() >= 7, "the table holds {}", FLAGS.len());
    }

    /// **Delete an entry from [`FLAGS`] and this is what goes red** — the help text still names it,
    /// and the sweep asks the table whether it is real.
    ///
    /// This is the direction that matters most, because it is the defect the old `--help` shipped:
    /// a listed flag the program does not honour is indistinguishable, to the person reading, from
    /// one it does.
    #[test]
    fn every_flag_the_help_text_names_is_one_this_build_takes() {
        let help = help();
        let found = flag_tokens(&help);
        assert!(
            found.len() >= 7,
            "the sweep read {} flag-shaped words out of `--help`, which is fewer than the table \
             holds — it is reading nothing rather than agreeing",
            found.len()
        );
        let strays: Vec<&String> = found.iter().filter(|t| !FLAGS.contains(&t.as_str())).collect();
        assert!(
            strays.is_empty(),
            "`--help` lists {strays:?}, and the parser does not take them"
        );
    }

    /// **Delete an arm from [`parse`] and this is what goes red.**
    ///
    /// The table and the help text can both still name a flag whose handler is gone; only asking
    /// the parser catches that. `--flash=` and `--disk=` are operands rather than verbs, so they
    /// are asked together with the verb that reads them.
    #[test]
    fn every_flag_in_the_table_is_one_the_parser_answers() {
        let mut refused: Vec<String> = Vec::new();
        for flag in FLAGS {
            let line = match *flag {
                "--make-app" => "--make-app somewhere".to_string(),
                "--flash=" => "--check-images --flash=a.bin".to_string(),
                "--disk=" => "--check-images --disk=a.img".to_string(),
                // **The two that take a number are given one.** A sweep that fed every `=` flag
                // the letter `x` would report `--clock=x`'s refusal — which is the parser working
                // — as a missing arm, and the two failures read identically from here.
                "--headless=" => "--headless=200000000".to_string(),
                "--clock=" => "--clock=5".to_string(),
                f if f.ends_with('=') => format!("{f}x"),
                f => f.to_string(),
            };
            match parse(&argv(&line)) {
                Cli::Refused(why) => refused.push(format!("`{line}` -> {why}")),
                // **A machine flag's answer IS a window** — and it has to be one that carries the
                // flag. A `Machine::default()` here means the word was accepted and dropped, which
                // is the shape this whole file exists to delete: it looks exactly like working.
                //
                // `control` is held to the same standard and by the same arm: `--control=x` parses
                // to a window too, and a window with `control: None` is the flag having been read
                // into nothing. That is precisely how `--control=` shipped — accepted by nobody,
                // refused by `RETIRED`, and served by a `serve` with no caller.
                Cli::Window { launch, control }
                    if launch == Machine::default() && control.is_none() =>
                {
                    refused.push(format!("`{line}` -> opened a window and dropped the flag"))
                }
                _ => {}
            }
        }
        assert!(
            refused.is_empty(),
            "{refused:?}: the flag is in the table and in the help, and the parser has no arm for it"
        );
    }

    // ── What each flag parses to ────────────────────────────────────────────────────────────────

    #[test]
    fn no_arguments_opens_the_window() {
        assert_eq!(
            parse(&[]),
            Cli::Window { launch: Machine::default(), control: None }
        );
    }

    /// **The four that configure a machine reach one, and each reaches exactly its own field.**
    ///
    /// The half worth asserting is the second: a flag that parses and lands on the wrong field is
    /// a flag that works and does something else, which is the quietest way a command line lies.
    /// [`Machine::apply`] is the only route from a word to a `Config`, so this is that route with
    /// each word taken one at a time.
    #[test]
    fn the_four_machine_flags_each_reach_their_own_field_and_no_other() {
        let win = |line: &str| match parse(&argv(line)) {
            Cli::Window { launch, .. } => launch,
            other => panic!("`{line}` -> {other:?}"),
        };
        assert_eq!(win("--cold"), Machine { cold: true, ..Machine::default() });
        assert_eq!(win("--second-core"), Machine { cores: Some(true), ..Machine::default() });
        assert_eq!(
            win("--no-second-core"),
            Machine { cores: Some(false), ..Machine::default() }
        );
        assert_eq!(win("--charger"), Machine { charger: true, ..Machine::default() });
        assert_eq!(win("--clock=5"), Machine { clock: Some(5), ..Machine::default() });
        // …and together, because they are not exclusive and a line with two on it is the normal
        // shape of a measurement.
        assert_eq!(
            win("--cold --clock=5 --second-core --charger"),
            Machine { cold: true, clock: Some(5), cores: Some(true), charger: true }
        );

        // **On the config, and nothing else on it moves.** `apply` writes four axes; a fifth would
        // be the command line reaching past the boundary this struct is. The clock's axis is two
        // fields, because the wheel's click spacing is a duration in the machine's own time and
        // therefore a function of the clock rather than a second knob.
        let mut cfg = crate::emu::Config {
            clock: ipod_machine::CLOCK,
            click_gap: ipod_machine::pace::wheel_click_gap(ipod_machine::CLOCK),
            snapshot: Some(PathBuf::from("/somewhere/m.snap")),
            ..Default::default()
        };
        Machine::default().apply(&mut cfg);
        assert_eq!(cfg.clock, ipod_machine::CLOCK, "a flag nobody typed overwrote the default");
        assert_eq!(cfg.click_gap, 300_000, "nor the gap that goes with it");
        // Two cores is the default, so "nothing asked for" is `!one_core`.
        assert!(!cfg.cold && !cfg.one_core && !cfg.charger);
        win("--cold --clock=5 --second-core --charger").apply(&mut cfg);
        assert!(cfg.cold && !cfg.one_core && cfg.charger);
        assert_eq!(cfg.clock, 5);
        // **The half that would have been silent.** A `--clock=5` leaving 300 000 behind delivers
        // one detent per 60 ms of the iPod's own time — fifteen times slower than a thumb — and
        // nothing on any surface would have said so.
        //
        // **How to make it go red:** drop the `click_gap` line from `Machine::apply`.
        assert_eq!(cfg.click_gap, 20_000, "--clock= moved the clock and left the wheel behind");
        assert_eq!(cfg.click_gap / cfg.clock as u64, 4_000, "4 ms of the machine's own time");
        // **The cores axis needs the flag that can move it.** Two cores is the default, so
        // `--second-core` sets `one_core` to the value it already had and an assertion about it
        // would hold with `apply` deleted. `--no-second-core` is the only input that changes the
        // machine, so it is the only one worth asserting on — and it must go back, because these
        // flags write onto a config the window has already built and a one-way switch would make
        // the second run of a session a different machine from the first.
        win("--no-second-core").apply(&mut cfg);
        assert!(cfg.one_core, "--no-second-core did not reach the machine");
        win("--second-core").apply(&mut cfg);
        assert!(!cfg.one_core, "--second-core did not put the coprocessor back");
        assert_eq!(
            cfg.snapshot,
            Some(PathBuf::from("/somewhere/m.snap")),
            "`apply` reached a field that is the library's, not the command line's"
        );
    }

    /// **A number that is not one, and a zero, are refused rather than clamped.**
    ///
    /// `emu::build` does `cfg.clock.max(1)`, so `--clock=0` would run the part at one instruction
    /// per simulated microsecond — seventy-five times slow, reported as though it were the part,
    /// with the flag on the command line saying it had been set to nothing. `--headless=0` is the
    /// same shape one flag over: the run loop stops when `executed >= limit`, which is true before
    /// the first instruction, so it would print a fingerprint of a machine that had done nothing
    /// and exit 0.
    #[test]
    fn a_machine_flag_with_a_meaningless_value_is_refused_rather_than_clamped() {
        for line in ["--clock=0", "--clock=x", "--clock=", "--clock=-5"] {
            assert!(
                matches!(parse(&argv(line)), Cli::Refused(_)),
                "`{line}` was accepted"
            );
        }
        for line in ["--headless=0", "--headless=x", "--headless="] {
            assert!(
                matches!(parse(&argv(line)), Cli::Refused(_)),
                "`{line}` was accepted"
            );
        }
        assert_eq!(
            parse(&argv("--headless=200000000")),
            Cli::Headless {
                machine: Machine::default(),
                budget: 200_000_000,
                control: None,
            }
        );
        // The four ride on `--headless=` too: a fingerprint taken at a different clock is a
        // different number, and a self-check that silently used the default would be comparing two
        // machines while claiming to compare two builds.
        assert_eq!(
            parse(&argv("--headless=200000000 --clock=5 --second-core")),
            Cli::Headless {
                machine: Machine { clock: Some(5), cores: Some(true), ..Machine::default() },
                budget: 200_000_000,
                control: None,
            }
        );
    }

    /// **The socket reaches the two verbs that start a machine, and lands on neither's fields.**
    ///
    /// It is the one flag here that configures nothing: `Machine::apply` must not learn about it,
    /// or the process's business would be written onto the machine's config — which is exactly the
    /// boundary [`Machine`]'s own doc draws to decide what may be a flag at all.
    #[test]
    fn the_control_socket_rides_on_both_verbs_that_start_a_machine() {
        assert_eq!(
            parse(&argv("--control=/tmp/ipod.sock")),
            Cli::Window {
                launch: Machine::default(),
                control: Some(PathBuf::from("/tmp/ipod.sock")),
            }
        );
        // Beside the machine flags, which is the normal shape: a cold boot somebody wants to watch.
        assert_eq!(
            parse(&argv("--cold --clock=5 --control=/tmp/ipod.sock")),
            Cli::Window {
                launch: Machine { cold: true, clock: Some(5), ..Machine::default() },
                control: Some(PathBuf::from("/tmp/ipod.sock")),
            }
        );
        assert_eq!(
            parse(&argv("--headless=200000000 --control=/tmp/ipod.sock")),
            Cli::Headless {
                machine: Machine::default(),
                budget: 200_000_000,
                control: Some(PathBuf::from("/tmp/ipod.sock")),
            }
        );
        // And it reaches no field of the machine, which is the half a test can get wrong by
        // agreeing with itself: `apply` writes four fields and a socket is not one of them.
        let mut cfg = crate::emu::Config::default();
        let before = (cfg.cold, cfg.one_core, cfg.charger, cfg.clock);
        let Cli::Window { launch, .. } = parse(&argv("--control=/tmp/ipod.sock")) else {
            panic!("a socket on its own is still a window")
        };
        launch.apply(&mut cfg);
        assert_eq!((cfg.cold, cfg.one_core, cfg.charger, cfg.clock), before);
    }

    /// **`--control=` was refused as an instrument belonging to `trace`, and `trace` has never had
    /// one.** The refusal is gone; the two spellings that are still wrong are still answered.
    #[test]
    fn the_socket_is_no_longer_refused_and_its_two_wrong_spellings_still_are() {
        assert!(
            !RETIRED.iter().any(|(f, _)| *f == "--control="),
            "a flag cannot be live and retired"
        );
        // The empty path: bound, it would fail with a message about a file nobody named.
        let r = parse(&argv("--control="));
        assert!(
            matches!(&r, Cli::Refused(w) if w.contains("/tmp/ipod.sock")),
            "{r:?} should name the shape that works"
        );
        // Detached, it is the spelling correction every other `=` flag gets.
        let r = parse(&argv("--control /tmp/ipod.sock"));
        assert!(
            matches!(&r, Cli::Refused(w) if w.contains("--control=FILE")),
            "{r:?}"
        );
    }

    /// **A socket beside a verb that starts no machine is refused rather than bound.**
    ///
    /// `--check-images` has its own arm in the tail match and returns before any general guard
    /// could see it, so this is not the same assertion as the machine flags' one below — it is the
    /// one that catches the socket being silently dropped by that early return.
    #[test]
    fn the_socket_refuses_beside_a_verb_that_runs_no_machine() {
        for (line, verb) in [
            ("--check-update --control=/tmp/s", "--check-update"),
            ("--check-images --control=/tmp/s", "--check-images"),
            ("--make-app dist --control=/tmp/s", "--make-app"),
        ] {
            let r = parse(&argv(line));
            assert!(
                matches!(&r, Cli::Refused(w) if w.contains("--control=") && w.contains(verb)),
                "`{line}` -> {r:?}"
            );
        }
    }

    /// **A machine flag beside a verb that starts no machine is refused, not read into nothing.**
    ///
    /// The same rule `--flash=` beside `--make-app` already keeps, and for the same reason: an
    /// unread flag is the whole defect this file exists to remove, and it is invisible because the
    /// command still does something.
    #[test]
    fn a_machine_flag_beside_a_verb_that_starts_nothing_is_refused() {
        for line in [
            "--check-update --cold",
            "--check-images --clock=5",
            "--make-app dist --second-core",
        ] {
            let r = parse(&argv(line));
            assert!(
                matches!(&r, Cli::Refused(w) if w.contains("--cold")),
                "`{line}` -> {r:?}"
            );
        }
    }

    #[test]
    fn help_is_spelled_both_ways_and_wins_over_the_rest_of_the_line() {
        assert_eq!(parse(&argv("-h")), Cli::Help);
        assert_eq!(parse(&argv("--help")), Cli::Help);
        // Somebody who has got the line wrong is exactly who is asking for the help.
        assert_eq!(parse(&argv("--nonsense --help")), Cli::Help);
        assert_eq!(parse(&argv("--check-images -h")), Cli::Help);
    }

    #[test]
    fn the_two_no_window_reports_parse_to_themselves() {
        assert_eq!(parse(&argv("--check-update")), Cli::CheckUpdate);
        assert_eq!(
            parse(&argv("--check-images --flash=a.bin --disk=b.img")),
            Cli::CheckImages {
                flash: Some("a.bin".into()),
                disk: Some("b.img".into()),
            }
        );
        // Order is not part of the grammar, and neither operand is required.
        assert_eq!(
            parse(&argv("--disk=b.img --check-images")),
            Cli::CheckImages {
                flash: None,
                disk: Some("b.img".into()),
            }
        );
    }

    #[test]
    fn the_bundler_takes_a_directory_and_an_optional_icon() {
        assert_eq!(
            parse(&argv("--make-app dist")),
            Cli::MakeApp {
                out: "dist".into(),
                icon: None
            }
        );
        assert_eq!(
            parse(&argv("--make-app dist icon.png")),
            Cli::MakeApp {
                out: "dist".into(),
                icon: Some("icon.png".into())
            }
        );
        // A flag after it is a flag, not an icon — otherwise `--make-app dist --check-update`
        // writes a bundle and swallows the second verb without saying so.
        let both = parse(&argv("--make-app dist --check-update"));
        assert!(
            matches!(&both, Cli::Refused(w) if w.contains("--make-app")),
            "{both:?} should refuse two verbs"
        );
    }

    #[test]
    fn the_bundler_refuses_rather_than_choosing_a_directory_for_you() {
        let r = parse(&argv("--make-app"));
        assert!(
            matches!(&r, Cli::Refused(w) if w.contains("--make-app OUT")),
            "{r:?} should name the signature"
        );
    }

    // ── And what it refuses ─────────────────────────────────────────────────────────────────────

    /// **The defect this whole file closes.** Every flag in [`RETIRED`] must be answered rather
    /// than ignored, and — the half that actually matters — none of them may reach
    /// [`Cli::Window`], because a window opening is how an ignored flag looks like a working one.
    ///
    /// **The floor is 33 and was 39.** The six that left are `--headless=`, `--cold`, `--clock=`,
    /// `--second-core`, `--charger` and `--control=`, which this build now honours: they moved to
    /// [`FLAGS`], where `every_flag_in_the_table_is_one_the_parser_answers` holds them to the
    /// opposite standard. A flag cannot be in both tables — `a_flag_is_either_live_or_retired`
    /// is what says so — so the two floors together are the whole count.
    #[test]
    fn every_flag_the_old_window_took_is_answered_rather_than_ignored() {
        assert!(RETIRED.len() >= 33, "the table holds {}", RETIRED.len());
        let mut wrong: Vec<String> = Vec::new();
        for (flag, _) in RETIRED {
            let line = if flag.ends_with('=') {
                format!("{flag}7")
            } else {
                (*flag).to_string()
            };
            match parse(&argv(&line)) {
                Cli::Refused(why) => {
                    if !why.contains(flag) {
                        wrong.push(format!("`{line}` refused without naming itself: {why}"));
                    }
                    if !why.contains("--help") {
                        wrong.push(format!("`{line}` refused without saying where to look: {why}"));
                    }
                }
                other => wrong.push(format!("`{line}` -> {other:?}")),
            }
        }
        assert!(wrong.is_empty(), "{wrong:?}");
    }

    /// The two tables may not overlap. A flag in both would be honoured **and** declared dead, and
    /// which of the two a person saw would depend on the order of two branches in [`refusal`].
    ///
    /// Compared by **head**, not by spelling: `--flash` and `--flash=` are one flag typed two ways,
    /// and a table holding one of each would collide through the spelling branch rather than
    /// through the exact match, which is the harder of the two to see.
    #[test]
    fn a_flag_is_either_live_or_retired_and_never_both() {
        let head = |f: &str| f.trim_end_matches('=').to_string();
        let live: Vec<String> = FLAGS.iter().map(|f| head(f)).collect();
        let both: Vec<&str> = RETIRED
            .iter()
            .map(|(f, _)| *f)
            .filter(|f| live.contains(&head(f)))
            .collect();
        assert!(both.is_empty(), "{both:?} are in both tables");
    }

    /// A live flag typed the other way round is a **spelling**, and is answered as one.
    ///
    /// `--flash FILE` is one keystroke from `--flash=FILE`, and answering it with *not a flag of
    /// this build* is a wrong answer that sends somebody to look for a flag they already have.
    #[test]
    fn a_live_flag_spelled_the_other_way_is_corrected_rather_than_denied() {
        let r = parse(&argv("--check-images --flash rom.bin"));
        assert!(
            matches!(&r, Cli::Refused(w) if w.contains("--flash=FILE")),
            "{r:?} should name the spelling that works"
        );
        let r = parse(&argv("--make-app=dist"));
        assert!(
            matches!(&r, Cli::Refused(w) if w.contains("separate words")),
            "{r:?} should say the operands are not attached"
        );
        // And a retired flag typed without its `=` is still answered as retired, not as a typo of
        // something live. `--snapshot` and not `--headless`: the second is live now, and a fixture
        // that quietly became a working flag is a test asserting nothing.
        let r = parse(&argv("--snapshot"));
        assert!(
            matches!(&r, Cli::Refused(w) if w.contains("beside the device")),
            "{r:?}"
        );
    }

    /// A retired flag is answered with the reason it is retired, and the three reasons are not
    /// interchangeable.
    ///
    /// **There were two and one of them expired.** `Gone::Machine` said *"this build starts no
    /// machine yet"* about thirty-one flags, and this build starts machines — so five of them came
    /// back and the rest are sorted onto the two answers that are true of them: a device says its
    /// own configuration in the library, and a measurement instrument belongs to `trace`.
    #[test]
    fn a_retired_flag_says_which_half_of_the_program_it_belonged_to() {
        let device = parse(&argv("--snapshot=x"));
        assert!(
            matches!(&device, Cli::Refused(w) if w.contains("beside the device")),
            "{device:?}"
        );
        let instrument = parse(&argv("--profile"));
        assert!(
            matches!(&instrument, Cli::Refused(w) if w.contains("not a debugger")),
            "{instrument:?}"
        );
        let window = parse(&argv("--user"));
        assert!(
            matches!(&window, Cli::Refused(w) if w.contains("window that was replaced")),
            "{window:?}"
        );
        // **And the sentence that expired is gone rather than reworded.** A refusal saying this
        // build starts no machine, in a build that starts machines, sends a person looking for a
        // thing that is right there.
        for (flag, _) in RETIRED {
            let line = if flag.ends_with('=') { format!("{flag}7") } else { (*flag).to_string() };
            let Cli::Refused(why) = parse(&argv(&line)) else { continue };
            assert!(
                !why.contains("starts no machine"),
                "`{flag}` is refused with a reason that stopped being true: {why}"
            );
        }
    }

    #[test]
    fn a_word_this_build_has_never_had_is_named_rather_than_swallowed() {
        let r = parse(&argv("--nonsense"));
        assert!(matches!(&r, Cli::Refused(w) if w.contains("--nonsense")), "{r:?}");
        // A bare word is not a flag and not a file this build opens.
        let r = parse(&argv("rom.bin"));
        assert!(
            matches!(&r, Cli::Refused(w) if w.contains("rom.bin") && w.contains("--check-images")),
            "{r:?}"
        );
    }

    /// **The operands cannot be given on their own, and they cannot be given to somebody else.**
    /// `--flash=` used to configure a machine at launch; here it only says what to check, so it
    /// needs the thing that checks — and beside `--check-update` or `--make-app` it would be read
    /// by nothing, which is the shape this whole file exists to stop.
    #[test]
    fn the_operands_refuse_without_the_verb_that_reads_them() {
        for line in ["--flash=a.bin", "--disk=b.img", "--flash=a.bin --disk=b.img"] {
            let r = parse(&argv(line));
            assert!(
                matches!(&r, Cli::Refused(w) if w.contains("Add --check-images")),
                "`{line}` -> {r:?}"
            );
        }
        for (line, verb) in [
            ("--check-update --flash=a.bin", "--check-update"),
            ("--make-app dist --disk=b.img", "--make-app"),
        ] {
            let r = parse(&argv(line));
            assert!(
                matches!(&r, Cli::Refused(w) if w.contains(verb) && w.contains("--check-images")),
                "`{line}` -> {r:?}: the operand would have been read by nothing"
            );
        }
    }

    // ── Exit codes and what reaches which writer ────────────────────────────────────────────────

    /// Help goes to **stdout** and exits 0, which is what `--help > /dev/null` in CI depends on,
    /// and it says the version, which is the one thing about this build a bundle can disagree with.
    #[test]
    fn help_is_an_answer_on_stdout_and_not_an_error() {
        let (code, out, err) = ran(&Cli::Help);
        assert_eq!(code, 0);
        assert!(err.is_empty(), "{err:?} reached stderr");
        assert!(out.contains(update::VERSION), "the help does not say what build this is");
        assert!(out.contains("--check-update"), "{out}");
        assert!(out.lines().count() > 10, "the help is {} lines", out.lines().count());
    }

    /// A refusal goes to **stderr** and exits 2. Both halves matter: a message on stdout is one a
    /// pipeline reads as the answer, and an exit code of 0 is one `set -e` does not see.
    #[test]
    fn a_refusal_is_a_complaint_on_stderr_with_a_non_zero_code() {
        let (code, out, err) = ran(&parse(&argv("--profile")));
        assert_eq!(code, 2);
        assert!(out.is_empty(), "{out:?} reached stdout");
        assert!(err.contains("--profile"), "{err:?}");
    }

    /// `--check-images` with nothing to check names what is missing rather than reporting a verdict
    /// about a file nobody gave it.
    ///
    /// **It claims the data directory first**, and that is the same rule `main.rs`'s own sweep
    /// enforces on its side of the crate: this is the one path in this file that falls back to
    /// `Settings::load`, which without the redirect resolves to the operator's real library —
    /// `AGENTS.md` §3 — and would also make the assertion depend on whose machine it ran on.
    #[test]
    fn nothing_to_check_is_a_refusal_and_not_a_verdict() {
        let _data = crate::data_dir_lock();
        let (code, out, err) = ran(&Cli::CheckImages {
            flash: None,
            disk: Some("b.img".into()),
        });
        assert_eq!(code, 2, "exit {code}: {out}{err}");
        assert!(err.contains("no NOR dump"), "{err:?}");
        assert!(out.is_empty(), "{out:?}");
    }

    /// A pair given in full is answered with a verdict and a non-zero code, and — the part CI reads
    /// — the word `UNREADABLE` for a file that is not there.
    #[test]
    fn a_pair_that_does_not_exist_is_unreadable_rather_than_a_crash() {
        let dir = std::env::temp_dir();
        let (code, _, err) = ran(&Cli::CheckImages {
            flash: Some(dir.join("no-such-ipod-rom.bin")),
            disk: Some(dir.join("no-such-ipod-drive.img")),
        });
        assert_eq!(code, 1, "a missing pair is a failure, not a pass");
        assert!(err.is_empty(), "{err:?}: the verdict is not a complaint");
    }

    /// The tokeniser the two help sweeps stand on, proved to read and proved to refuse — otherwise
    /// `every_flag_the_help_text_names_is_one_this_build_takes` is a sweep that finds nothing and
    /// reports agreement.
    #[test]
    fn the_help_tokeniser_reads_flags_and_only_flags() {
        assert_eq!(
            flag_tokens("  --flash=FILE  the NOR dump, or `--disk=X`."),
            ["--flash=", "--disk="]
        );
        assert_eq!(flag_tokens("-h, --help"), ["-h", "--help"]);
        // Hyphens inside a word are not flags, and neither is an em dash.
        assert!(flag_tokens("a double-clickable app — signed by nobody").is_empty());
    }
}
