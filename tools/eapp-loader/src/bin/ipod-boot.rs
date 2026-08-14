//! `ipod-boot` — the boot recipes, as a program rather than as seven shell scripts.
//!
//! ```text
//! ipod-boot retail   [trace flags…]     # the recipe every number in research/ is measured on
//! ipod-boot cold     [trace flags…]
//! ipod-boot warm     [trace flags…]
//! ipod-boot flsh     [trace flags…]
//! ipod-boot rockbox  [trace flags…]
//! ipod-boot flash-update [trace flags…]
//! ipod-boot from-idle    [trace flags…]
//! ```
//!
//! # Why this exists, and what it is not
//!
//! `tools/ipod-boot/*.sh` are bash, and bash is not on a stock Windows. They are also the recipes
//! **every measurement in `research/` was taken through**, which makes rewriting them the one
//! change here with no upside and a large downside: a recipe that has drifted from the one a number
//! was measured on turns that number into a claim about a machine nobody can rebuild.
//!
//! So the scripts are untouched and this is a **second front end that composes the same argv**. Not
//! a port, not a replacement, not a wrapper that shells out to `bash` — the same list of arguments,
//! in the same order, handed to the same `trace` binary. `--print` shows the list without running
//! it, and `recipe_flags_match_the_shell_scripts` in this file's tests reads the `.sh` files off
//! disk and asserts that the flags each recipe passes are the flags the script passes, in order. If
//! somebody edits one and not the other, `cargo test` says so.
//!
//! # Two deliberate differences from the scripts, both documented rather than hidden
//!
//! - **`TRACE` defaults to the `trace` binary beside this one**, not to
//!   `$HOME/dev/.cargo-target/release/trace`. The scripts' default is one machine's target
//!   directory; a released binary cannot have that. `cargo build --release` puts `ipod-boot` and
//!   `trace` in the same directory, so the sibling is right by construction. `TRACE=` still wins.
//! - **The disk clone tries three copies, not one.** `cp -c` is an APFS clone and Linux `cp` does
//!   not have the flag; `cp --reflink=auto` is the btrfs/XFS equivalent and macOS `cp` does not
//!   have *that*. Both are attempted, in that order, before a full 8 GB byte copy. See
//!   [`clone_file`].
//!
//! Everything else — the defaults, the environment variables, the flags, the order — is the
//! scripts'. `research/` numbers are reproducible through either.

use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(name) = argv.first() else {
        usage();
        std::process::exit(2);
    };
    if name == "-h" || name == "--help" {
        usage();
        return;
    }
    // `--print` anywhere means "compose the argv and show it, run nothing". The recipes that build
    // something first (a disk clone, a snapshot) do not build it either.
    let dry = argv.iter().any(|a| a == "--print");
    let rest: Vec<String> = argv[1..].iter().filter(|a| *a != "--print").cloned().collect();

    // `make-disk` is not a recipe — it builds the thing a recipe needs, and it is the front door
    // for anyone who does not already have an 8 GB image of an iPod.
    if name == "make-disk" {
        match make_disk(&rest) {
            Ok(()) => return,
            Err(e) => {
                eprintln!("ipod-boot make-disk: {e}");
                std::process::exit(1);
            }
        }
    }

    let Some(recipe) = Recipe::parse(name) else {
        eprintln!("unknown recipe `{name}`\n");
        usage();
        std::process::exit(2);
    };

    match run(recipe, &rest, dry) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("ipod-boot {name}: {e}");
            std::process::exit(1);
        }
    }
}

fn usage() {
    eprintln!(
        "\
ipod-boot — Apple's firmware, booted under the emulator

  ipod-boot retail        [flags…]   Apple's shipping 5G bootloader + the image it accepts.
                                     Every current number in research/ is measured on this one.
  ipod-boot cold          [flags…]   the PROTOTYPE NOR. Numbers measured on it do not transfer.
  ipod-boot warm          [flags…]   RetailOS entered directly at 0x10000000, handoff faked
  ipod-boot flsh          [flags…]   one of the NOR's own images: IMG=diag|disk|scan|logo|vmcs
  ipod-boot rockbox       [flags…]   Rockbox as a source-available oracle
  ipod-boot flash-update  [flags…]   Apple's `aupd` updater, then the boot that proves it took
  ipod-boot from-idle     [flags…]   restore a cached 1.6 G snapshot: 3 s instead of 110 s

  ipod-boot make-disk IPSW OUT.img [SECTORS]
                                     build a bootable drive image from an IPSW. This is the way
                                     to get a disk if you do not already have one: an IPSW is
                                     ~14 MB against 8 GB, it is Apple's own firmware rather than
                                     an image of somebody's iPod, and RetailOS builds the rest of
                                     the volume itself on first boot. SECTORS defaults to
                                     16777216 (8 GiB); the file is sparse.

  --print                            compose the argv, print it, run nothing
  -h, --help                         this

Everything else is passed through to `trace` unchanged.

Environment, all optional and all the same as the shell recipes in tools/ipod-boot/:
  TRACE     the emulator binary            (default: the `trace` beside this program)
  BUDGET    instruction budget             (per recipe: 150M cold, 4G is the usual for retail)
  FLASH     the NOR dump
  DISK      the drive image
  WORKDISK  keep the writable clone across runs instead of a per-run temporary
  OSOS      warm only — the RetailOS image
  IMG       flsh / rockbox — which image
  GAME      the eApp handed to `trace`; a boot never executes it
  SNAP_AT   from-idle — where the snapshot is taken       (default 1600000000)
  CACHE     from-idle — where snapshots live              (default <tempdir>/ipod-from-idle)

The shell recipes in tools/ipod-boot/ remain the reference. This program composes the same argv;
`--print` shows it, and a test asserts the two agree."
    );
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Recipe {
    Retail,
    Cold,
    Warm,
    Flsh,
    Rockbox,
    FlashUpdate,
    FromIdle,
}

impl Recipe {
    fn parse(s: &str) -> Option<Recipe> {
        Some(match s {
            "retail" | "retail-boot" => Recipe::Retail,
            "cold" | "cold-boot" => Recipe::Cold,
            "warm" | "warm-boot" => Recipe::Warm,
            "flsh" => Recipe::Flsh,
            "rockbox" => Recipe::Rockbox,
            "flash-update" => Recipe::FlashUpdate,
            "from-idle" => Recipe::FromIdle,
            _ => return None,
        })
    }

    /// The `.sh` this recipe is the second front end for. Used by the drift test, and only by it —
    /// the mapping is the test's premise, so it lives beside the recipes rather than inside
    /// `#[cfg(test)]` where a reader would have to go looking for it.
    #[cfg_attr(not(test), allow(dead_code))]
    fn script(self) -> &'static str {
        match self {
            Recipe::Retail => "retail-boot.sh",
            Recipe::Cold => "cold-boot.sh",
            Recipe::Warm => "warm-boot.sh",
            Recipe::Flsh => "flsh.sh",
            Recipe::Rockbox => "rockbox.sh",
            Recipe::FlashUpdate => "flash-update.sh",
            Recipe::FromIdle => "from-idle.sh",
        }
    }
}

// ---------------------------------------------------------------- where things are

/// The repository root, found from the executable rather than the working directory, so the
/// program can be launched from anywhere. Same walk `ipod-gui` does, for the same reason: a shared
/// `CARGO_TARGET_DIR` puts the binary a long way from the source.
fn repo_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let mut p = exe.as_path();
        while let Some(dir) = p.parent() {
            if dir.join("tools/ipod-boot").is_dir() {
                return dir.to_path_buf();
            }
            p = dir;
        }
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf()).unwrap_or(here)
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(default)
}

/// The `trace` beside this binary. `cargo build --release` puts both in the same directory, and a
/// release archive ships them side by side, so "the sibling" is correct by construction — where
/// `$HOME/dev/.cargo-target/release/trace`, the shell default, is correct on exactly one machine.
fn default_trace() -> PathBuf {
    let exe_name = if cfg!(windows) { "trace.exe" } else { "trace" };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(exe_name);
            if sibling.is_file() {
                return sibling;
            }
        }
    }
    PathBuf::from(exe_name)
}

/// A path that must exist before anything is spawned, so the failure names the file rather than
/// arriving 40 lines into `trace`'s output as a read error.
fn require(p: &Path, what: &str) -> Result<(), String> {
    if p.exists() {
        return Ok(());
    }
    Err(format!(
        "no {what} at {}\n\
         resources/ is gitignored — it holds Apple's firmware and multi-gigabyte disk images, and \
         you supply your own. See the README's \"What you have to supply\" and \
         tools/ipod-boot/README.md.",
        p.display()
    ))
}

// ---------------------------------------------------------------- copying an 8 GB disk image

/// Clone `from` to `to`, using the filesystem's copy-on-write if it has one.
///
/// Three rungs, and the order is the point:
///
/// 1. **`cp -c`** — APFS `clonefile(2)`. ~3 ms for 8 GB, and the reason a fresh disk per run is
///    free on a Mac. Not a GNU flag; on Linux `cp -c` is an error, so it falls through.
/// 2. **`cp --reflink=auto`** — the btrfs / XFS / bcachefs equivalent, and the rung that was
///    missing: without it a Linux run paid a full 8 GB byte copy *per boot*. GNU `cp` with
///    `--reflink=auto` never fails for want of reflink support; it silently does a full copy. So
///    reaching rung 3 means neither `cp` exists at all.
/// 3. **`std::fs::copy`** — Windows, and any Unix without a usable `cp`.
///
/// Windows skips straight to rung 3: `cp` is not there, and ReFS block cloning is an `FSCTL` this
/// program is not going to reach for. `WORKDISK=` is the answer on a filesystem with no clone —
/// pay the copy once and keep the image.
fn clone_file(from: &Path, to: &Path) -> Result<(), String> {
    if to.exists() {
        return Ok(());
    }
    if let Some(dir) = to.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if cfg!(unix) {
        for flag in ["-c", "--reflink=auto"] {
            let ok = Command::new("cp")
                .arg(flag)
                .arg(from)
                .arg(to)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return Ok(());
            }
            // A failed `cp` can still have created a truncated destination; a partial 8 GB disk
            // image that later reads as a valid file is exactly the silent failure this project
            // keeps paying for.
            let _ = std::fs::remove_file(to);
        }
    }
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("copying {} -> {}: {e}", from.display(), to.display()))
}

// ---------------------------------------------------------------- the recipes

/// The argv a recipe hands `trace`, and the temporary it wants removed afterwards.
struct Plan {
    trace: PathBuf,
    /// One entry per `trace` invocation. `flash-update` boots twice, on purpose.
    runs: Vec<Vec<String>>,
    /// Deleted after the last run. The per-run disk clone, when it is not a `WORKDISK`.
    cleanup: Option<PathBuf>,
}

fn run(recipe: Recipe, user: &[String], dry: bool) -> Result<i32, String> {
    let plan = plan(recipe, user, dry)?;
    if dry {
        for r in &plan.runs {
            println!("{}", shell_quote(&plan.trace, r));
        }
        return Ok(0);
    }
    require(&plan.trace, "trace binary")?;

    let mut code = 0;
    for (i, args) in plan.runs.iter().enumerate() {
        if plan.runs.len() > 1 {
            println!("===== boot {} =====", i + 1);
        }
        let status = Command::new(&plan.trace)
            .args(args)
            .status()
            .map_err(|e| format!("{}: {e}", plan.trace.display()))?;
        code = status.code().unwrap_or(1);
        if code != 0 {
            break;
        }
    }
    if let Some(p) = plan.cleanup {
        let _ = std::fs::remove_file(p);
    }
    Ok(code)
}

fn plan(recipe: Recipe, user: &[String], dry: bool) -> Result<Plan, String> {
    let root = repo_root();
    let res = root.join("resources");
    let trace = env_path("TRACE").unwrap_or_else(default_trace);

    // The eApp every `trace` invocation is handed. A boot never executes it — RetailOS is entered
    // from the reset vector and never looks at 0x18000000 — but `trace`'s first positional is the
    // image, so the recipes all name the same one.
    let game = env_path("GAME").unwrap_or_else(|| {
        res.join("Cracked Click Wheel Games/Cracked Games/Tetris/Executables/Tetris_1_1_2563292.bin")
    });

    // The prototype NOR (archive.org, "SA JULY 12 2007 ipod video prototype firmware dump"), which
    // is `cold-boot.sh`'s default and, deliberately, nothing else's.
    let proto_flash =
        || res.join("internal_rom_000000-0FFFFF/internal_rom_000000-0FFFFF.bin");
    let retail_flash =
        || res.join("reference/ipod-bootrom-archive/A1238/internal_rom_000000-0FFFFF.bin");

    match recipe {
        // trace GAME BUDGET --boot-osos --cold-boot --flash= --disk= --bcm --pmu --nor "$@"
        Recipe::Cold => {
            let flash = env_path("FLASH").unwrap_or_else(proto_flash);
            let disk = env_path("DISK").unwrap_or_else(|| res.join("derived/disk/ipod8g.img"));
            let budget = env_u64("BUDGET", 150_000_000);
            if !dry {
                require(&game, "game image (GAME=)")?;
                require(&flash, "NOR dump (FLASH=)")?;
                require(&disk, "disk image (DISK=)")?;
            }
            Ok(Plan {
                trace,
                runs: vec![cold_argv(&game, budget, &flash, &disk, &[], user)],
                cleanup: None,
            })
        }

        // retail-boot.sh: the retail defaults, a writable per-run clone, then cold-boot.sh
        // --disk-writable "$@" — so --disk-writable lands ahead of the caller's own flags.
        Recipe::Retail => {
            let flash = env_path("FLASH").unwrap_or_else(retail_flash);
            let src = env_path("DISK").unwrap_or_else(|| res.join("derived/disk/ipod8g-retail.img"));
            let budget = env_u64("BUDGET", 150_000_000);
            let (work, cleanup) = match env_path("WORKDISK") {
                Some(w) => (w, None),
                None => {
                    let w = std::env::temp_dir()
                        .join(format!("ipod-retail-boot-{}.img", std::process::id()));
                    (w.clone(), Some(w))
                }
            };
            if !dry {
                require(&game, "game image (GAME=)")?;
                require(&flash, "NOR dump (FLASH=)")?;
                require(&src, "disk image (DISK=)")?;
                clone_file(&src, &work)?;
            }
            Ok(Plan {
                trace,
                runs: vec![cold_argv(
                    &game,
                    budget,
                    &flash,
                    &work,
                    &["--disk-writable".into()],
                    user,
                )],
                cleanup,
            })
        }

        // trace GAME BUDGET --osos= --boot-osos --osos-at=0x04000000 --sysinfo --flash= --disk=
        //       --bcm --pmu "$@"
        Recipe::Warm => {
            let flash = env_path("FLASH").unwrap_or_else(proto_flash);
            let disk = env_path("DISK").unwrap_or_else(|| res.join("derived/disk/ipod8g.img"));
            let osos = env_path("OSOS").unwrap_or_else(|| res.join("derived/fw/OSOS_correct.bin"));
            let budget = env_u64("BUDGET", 600_000_000);
            if !dry {
                require(&game, "game image (GAME=)")?;
                require(&flash, "NOR dump (FLASH=)")?;
                require(&disk, "disk image (DISK=)")?;
                require(&osos, "RetailOS image (OSOS=)")?;
            }
            let mut a = head(&game, budget);
            a.push(opt("--osos=", &osos));
            a.push("--boot-osos".into());
            a.push("--osos-at=0x04000000".into());
            a.push("--sysinfo".into());
            a.push(opt("--flash=", &flash));
            a.push(opt("--disk=", &disk));
            a.push("--bcm".into());
            a.push("--pmu".into());
            a.extend(user.iter().cloned());
            Ok(Plan { trace, runs: vec![a], cleanup: None })
        }

        // trace GAME BUDGET --osos=.../flsh/$IMG.bin --boot-osos --flash= --disk= --sysinfo
        //       --bcm --pmu --nor "$@"
        Recipe::Flsh => {
            let img = std::env::var("IMG").unwrap_or_else(|_| "diag".into());
            let flash = env_path("FLASH").unwrap_or_else(proto_flash);
            let disk = env_path("DISK").unwrap_or_else(|| res.join("derived/disk/ipod8g.img"));
            let osos = res.join(format!("derived/fw/flsh/{img}.bin"));
            let budget = env_u64("BUDGET", 200_000_000);
            if !dry {
                require(&game, "game image (GAME=)")?;
                require(&flash, "NOR dump (FLASH=)")?;
                require(&disk, "disk image (DISK=)")?;
                require(&osos, "flash image (IMG=)")?;
            }
            let mut a = head(&game, budget);
            a.push(opt("--osos=", &osos));
            a.push("--boot-osos".into());
            a.push(opt("--flash=", &flash));
            a.push(opt("--disk=", &disk));
            a.push("--sysinfo".into());
            a.push("--bcm".into());
            a.push("--pmu".into());
            a.push("--nor".into());
            a.extend(user.iter().cloned());
            Ok(Plan { trace, runs: vec![a], cleanup: None })
        }

        // trace GAME BUDGET --osos=$RB/$IMG --boot-osos --flash= --disk= --sysinfo --bcm --pmu "$@"
        Recipe::Rockbox => {
            let img = std::env::var("IMG").unwrap_or_else(|_| "rb-main.raw".into());
            let flash = env_path("FLASH").unwrap_or_else(proto_flash);
            let disk = env_path("DISK").unwrap_or_else(|| res.join("derived/disk/ipod8g.img"));
            let osos = res.join("reference/rockbox/bin").join(&img);
            let budget = env_u64("BUDGET", 200_000_000);
            if !dry {
                require(&game, "game image (GAME=)")?;
                require(&flash, "NOR dump (FLASH=)")?;
                require(&disk, "disk image (DISK=)")?;
                require(&osos, "Rockbox image (IMG=)")?;
            }
            let mut a = head(&game, budget);
            a.push(opt("--osos=", &osos));
            a.push("--boot-osos".into());
            a.push(opt("--flash=", &flash));
            a.push(opt("--disk=", &disk));
            a.push("--sysinfo".into());
            a.push("--bcm".into());
            a.push("--pmu".into());
            a.extend(user.iter().cloned());
            Ok(Plan { trace, runs: vec![a], cleanup: None })
        }

        // Two boots of the same argv, against a disk whose firmware partition was written from the
        // pristine bundle. The first is the update; the second is the proof that the update took.
        Recipe::FlashUpdate => {
            let flash = env_path("FLASH").unwrap_or_else(retail_flash);
            let srcdisk =
                env_path("SRCDISK").unwrap_or_else(|| res.join("derived/disk/ipod8g.img"));
            let fw = env_path("FW").unwrap_or_else(|| res.join("derived/fw/Firmware-20.6.3"));
            let work = env_path("WORK").unwrap_or_else(|| std::env::temp_dir().join("ipod-flash-update"));
            let budget = env_u64("BUDGET", 600_000_000);
            let disk = work.join("disk.img");
            if !dry {
                require(&game, "game image (GAME=)")?;
                require(&flash, "NOR dump (FLASH=)")?;
                require(&srcdisk, "disk image (SRCDISK=)")?;
                require(&fw, "firmware bundle (FW=)")?;
                if !disk.exists() {
                    clone_file(&srcdisk, &disk)?;
                    write_firmware_partition(&disk, &fw)?;
                    println!("built {} — firmware partition written from {}", disk.display(), fw.display());
                }
            }
            // NOT `cold_argv` + an extra. `flash-update.sh` puts `--disk-writable` immediately
            // after `--disk=`, where `retail-boot.sh` — which reaches `trace` through
            // `cold-boot.sh`'s `"$@"` — puts it after `--nor`. Same flags, different order, and
            // the drift test caught the difference the first time this was written the tidy way.
            let mut argv = head(&game, budget);
            argv.push("--boot-osos".into());
            argv.push("--cold-boot".into());
            argv.push(opt("--flash=", &flash));
            argv.push(opt("--disk=", &disk));
            argv.push("--disk-writable".into());
            argv.push("--bcm".into());
            argv.push("--pmu".into());
            argv.push("--nor".into());
            argv.extend(user.iter().cloned());
            Ok(Plan { trace, runs: vec![argv.clone(), argv], cleanup: None })
        }

        // Restore a cached snapshot instead of replaying 1.6 G instructions of boot.
        Recipe::FromIdle => {
            let snap_at = env_u64("SNAP_AT", 1_600_000_000);
            let budget = env_u64("BUDGET", 60_000_000);
            let cache = env_path("CACHE").unwrap_or_else(|| std::env::temp_dir().join("ipod-from-idle"));
            let flash = env_path("FLASH").unwrap_or_else(retail_flash);
            let src = env_path("DISK").unwrap_or_else(|| res.join("derived/disk/ipod8g-retail.img"));

            // Keyed on the emulator binary, and this is the load-bearing line in the file: a
            // snapshot restored under a different build is a hybrid machine whose numbers stay
            // plausible and stop meaning anything. `shasum -a 256 "$TRACE" | cut -c1-16` is what
            // from-idle.sh computes, so the two front ends share one cache rather than each
            // paying its own 80-second cold boot.
            // Computed under `--print` too, whenever the binary is there to read: a printed
            // command line with a placeholder in it is not a command line, and the whole point of
            // `--print` is that it can be pasted.
            let key = match std::fs::read(&trace) {
                Ok(bytes) => sha256_hex(&bytes)[..16].to_string(),
                Err(e) if dry => {
                    eprintln!("(no trace binary at {}: {e})", trace.display());
                    "<sha256 of the trace binary, first 16 hex>".to_string()
                }
                Err(e) => return Err(format!("{}: {e}", trace.display())),
            };
            let snap = cache.join(format!("idle-{key}-{snap_at}.snap"));
            let disk = cache.join(format!("idle-{key}-{snap_at}.img"));

            if !dry && !snap.exists() {
                std::fs::create_dir_all(&cache).map_err(|e| format!("{}: {e}", cache.display()))?;
                eprintln!("building snapshot at {snap_at} for trace {key} (one-off, ~80 s) …");
                let _ = std::fs::remove_file(&disk);
                require(&game, "game image (GAME=)")?;
                require(&flash, "NOR dump (FLASH=)")?;
                require(&src, "disk image (DISK=)")?;
                clone_file(&src, &disk)?;
                let build = cold_argv(
                    &game,
                    snap_at + 1_000_000,
                    &flash,
                    &disk,
                    &["--disk-writable".into()],
                    &[
                        "--clock=5".to_string(),
                        format!("--snapshot={}:{}", snap_at, snap.display()),
                    ],
                );
                let status = Command::new(&trace)
                    .args(&build)
                    .status()
                    .map_err(|e| format!("{}: {e}", trace.display()))?;
                if !status.success() {
                    return Err("the snapshot build did not finish".into());
                }
                // A partial snapshot is worse than none — it would restore and quietly under-run.
                let ok = std::fs::metadata(&snap).map(|m| m.len() > 0).unwrap_or(false);
                if !ok {
                    return Err("snapshot was not written; refusing to continue".into());
                }
            }

            let mut tail = vec!["--clock=5".to_string(), format!("--restore={}", snap.display())];
            tail.extend(user.iter().cloned());
            Ok(Plan {
                trace,
                runs: vec![cold_argv(
                    &game,
                    budget,
                    &flash,
                    &disk,
                    &["--disk-writable".into()],
                    &tail,
                )],
                cleanup: None,
            })
        }
    }
}

/// `ipod-boot make-disk IPSW OUT.img [SECTORS]` — the front door for somebody with no disk image.
///
/// Prints what the IPSW turned out to be before writing anything, so a wrong bundle is a sentence
/// rather than a boot that fails ninety seconds later for a reason nobody can read.
fn make_disk(args: &[String]) -> Result<(), String> {
    let src = args.first().ok_or(
        "usage: ipod-boot make-disk IPSW OUT.img [SECTORS]\n\
         IPSW is an iPod software-update bundle — a zip holding `Firmware-<version>` and \
         `manifest.plist`. It is not distributed with this project.",
    )?;
    let out = args.get(1).ok_or("usage: ipod-boot make-disk IPSW OUT.img [SECTORS]")?;
    let with_aupd = args.iter().any(|a| a == "--with-aupd");
    let sectors = args
        .get(2)
        .filter(|s| !s.starts_with("--"))
        .map(|s| s.replace('_', "").parse::<u64>().map_err(|e| format!("SECTORS: {e}")))
        .transpose()?
        .unwrap_or(eapp_loader::ipsw::DEFAULT_SECTORS);

    let mut fw = match eapp_loader::ipsw::inspect(Path::new(src)) {
        eapp_loader::ipsw::Ipsw::Good(what, fw) => {
            println!("{src}\n  {what}");
            fw
        }
        eapp_loader::ipsw::Ipsw::Wrong(why) | eapp_loader::ipsw::Ipsw::Bad(why) => {
            return Err(why)
        }
    };
    if with_aupd {
        println!(
            "  `aupd` left armed: the FIRST boot will run Apple's flash updater and the second \
             will run the OS, which is what a real iPod does after a restore. \
             `ipod-boot flash-update` is the recipe that measures both."
        );
    } else if eapp_loader::ipsw::mark_aupd_applied(&mut fw) {
        println!(
            "  `aupd` marked applied (+0x08 = 1), so the first boot runs the OS. That is the state \
             a real iPod is in after its post-restore firmware update; `--with-aupd` leaves it \
             armed and reproduces the two-boot sequence."
        );
    }
    eapp_loader::ipsw::build_disk(&fw, Path::new(out), sectors)?;
    println!(
        "wrote {out} — {sectors} sectors ({} MiB), sparse. \
         The firmware partition is Apple's, byte for byte; the FAT32 volume is empty and RetailOS \
         populates it on first boot.",
        sectors / 2048
    );
    println!("  ipod-boot retail --clock=5     # DISK={out}");
    Ok(())
}

/// The positional head every recipe shares: the eApp, then the budget.
fn head(game: &Path, budget: u64) -> Vec<String> {
    vec![game.to_string_lossy().into_owned(), budget.to_string()]
}

fn opt(flag: &str, p: &Path) -> String {
    format!("{flag}{}", p.display())
}

/// `cold-boot.sh`'s argv, which `retail-boot.sh`, `flash-update.sh` and `from-idle.sh` all reach
/// through. `extra` is what the calling recipe inserts ahead of the caller's own flags, exactly
/// where `"$@"` puts it when one script `exec`s another.
fn cold_argv(
    game: &Path,
    budget: u64,
    flash: &Path,
    disk: &Path,
    extra: &[String],
    user: &[String],
) -> Vec<String> {
    let mut a = head(game, budget);
    a.push("--boot-osos".into());
    a.push("--cold-boot".into());
    a.push(opt("--flash=", flash));
    a.push(opt("--disk=", disk));
    a.push("--bcm".into());
    a.push("--pmu".into());
    a.push("--nor".into());
    a.extend(extra.iter().cloned());
    a.extend(user.iter().cloned());
    a
}

/// `dd if=$FW of=$DISK bs=512 seek=63 conv=notrunc`.
///
/// MBR partition 0 starts at LBA 63 and is 27 140 sectors — 13 895 680 bytes, exactly the size of
/// the pristine firmware. It fits with nothing left over, which is how the offset is known.
/// `conv=notrunc` is the whole point: the 8 GB image keeps its length.
fn write_firmware_partition(disk: &Path, fw: &Path) -> Result<(), String> {
    let bytes = std::fs::read(fw).map_err(|e| format!("{}: {e}", fw.display()))?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open(disk)
        .map_err(|e| format!("{}: {e}", disk.display()))?;
    f.seek(SeekFrom::Start(63 * 512)).map_err(|e| format!("{}: {e}", disk.display()))?;
    f.write_all(&bytes).map_err(|e| format!("{}: {e}", disk.display()))?;
    f.flush().map_err(|e| format!("{}: {e}", disk.display()))
}

/// Render an argv the way a shell would need it written, for `--print`. Paths in this project have
/// spaces in them (`Cracked Click Wheel Games`), and an unquoted `--print` line that cannot be
/// pasted back is a documentation bug waiting to be reported as a code one.
fn shell_quote(trace: &Path, args: &[String]) -> String {
    let mut out = quote_one(&trace.to_string_lossy());
    for a in args {
        out.push(' ');
        out.push_str(&quote_one(a));
    }
    out
}

fn quote_one(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_=./:+,@".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

// ---------------------------------------------------------------- SHA-256

/// SHA-256, ~60 lines of it, so this program and `from-idle.sh` compute the **same** cache key and
/// share one snapshot instead of each paying its own 80-second cold boot.
///
/// Written out rather than depended on: `eapp-loader` has one dependency (`arm7tdmi`, a path) and
/// the README's claim that the core crates build with no third-party code is worth more than the
/// sixty lines. There is a NIST known-answer test below.
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());

    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, c) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (i, v) in [a, b, c, d, e, f, g, hh].into_iter().enumerate() {
            h[i] = h[i].wrapping_add(v);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// NIST FIPS 180-2 test vectors, plus the empty string. If this passes, `from-idle.sh` and
    /// `ipod-boot from-idle` name the same cache file.
    #[test]
    fn sha256_matches_the_published_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        // A multi-block input, to exercise the padding boundary rather than only the first block.
        assert_eq!(
            sha256_hex(&vec![b'a'; 1_000_000])[..16],
            *"cdc76e5c9914fb92"
        );
    }

    fn flags_of(argv: &[String]) -> Vec<String> {
        argv.iter()
            .filter(|a| a.starts_with("--"))
            .map(|a| match a.split_once('=') {
                Some((k, _)) => format!("{k}="),
                None => a.clone(),
            })
            .collect()
    }

    /// The drift guard this file exists to make possible.
    ///
    /// Reads each `.sh` off disk, pulls the flags out of its `trace` invocation, and asserts they
    /// are the flags this program passes, in order. Two front ends composing "the same" argv is a
    /// claim; this is the test that keeps it one.
    #[test]
    fn recipe_flags_match_the_shell_scripts() {
        let scripts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("ipod-boot");
        for recipe in [
            Recipe::Cold,
            Recipe::Warm,
            Recipe::Flsh,
            Recipe::Rockbox,
            Recipe::FlashUpdate,
        ] {
            let text = std::fs::read_to_string(scripts.join(recipe.script()))
                .unwrap_or_else(|e| panic!("{}: {e}", recipe.script()));
            let from_script = script_flags(&text);
            let plan = plan(recipe, &[], true).unwrap();
            let mine = flags_of(&plan.runs[0]);
            assert_eq!(
                mine,
                from_script,
                "{} and ipod-boot {:?} disagree about the flags",
                recipe.script(),
                recipe
            );
        }
    }

    /// `retail-boot.sh` reaches `trace` through `cold-boot.sh`, inserting `--disk-writable` where
    /// the `exec` puts it. Checked against `cold-boot.sh`'s flags plus that one, because the
    /// script itself has no `trace` line to read.
    #[test]
    fn retail_is_cold_plus_disk_writable() {
        let scripts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("ipod-boot");
        let cold = script_flags(&std::fs::read_to_string(scripts.join("cold-boot.sh")).unwrap());
        let retail = flags_of(&plan(Recipe::Retail, &[], true).unwrap().runs[0]);
        let mut want = cold;
        want.push("--disk-writable".into());
        assert_eq!(retail, want);

        let text = std::fs::read_to_string(scripts.join("retail-boot.sh")).unwrap();
        assert!(
            text.contains("--disk-writable"),
            "retail-boot.sh stopped passing --disk-writable; this test's premise is gone"
        );
    }

    /// Extract the flags from a script's `trace` invocation: everything from the line that runs
    /// `$TRACE` to the end of its backslash continuations.
    fn script_flags(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut in_call = false;
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with('#') {
                continue;
            }
            if !in_call && t.contains("\"$TRACE\"") {
                in_call = true;
            }
            if in_call {
                for tok in t.split_whitespace() {
                    if let Some(f) = tok.strip_prefix("--") {
                        let name = f.split('=').next().unwrap_or(f);
                        let has_value = f.contains('=');
                        // `"$@"` is the caller's flags, not the recipe's.
                        out.push(if has_value {
                            format!("--{name}=")
                        } else {
                            format!("--{name}")
                        });
                    }
                }
                if !t.ends_with('\\') {
                    break;
                }
            }
        }
        out
    }

    /// `flash-update` boots twice with an identical argv. That is the measurement — the second boot
    /// proves the first one's write took, and it only proves it if nothing differs between them.
    #[test]
    fn flash_update_boots_the_same_argv_twice() {
        let p = plan(Recipe::FlashUpdate, &[], true).unwrap();
        assert_eq!(p.runs.len(), 2);
        assert_eq!(p.runs[0], p.runs[1]);
    }

    /// The caller's flags go last, so `--clock=5` on the command line reaches `trace` after the
    /// recipe's own — which is what lets a caller override one.
    #[test]
    fn user_flags_are_appended_last() {
        let user = vec!["--clock=5".to_string(), "--profile".to_string()];
        let p = plan(Recipe::Cold, &user, true).unwrap();
        let n = p.runs[0].len();
        assert_eq!(&p.runs[0][n - 2..], &user[..]);
    }

    /// A per-run temporary belongs under the platform's temp directory, wherever that is.
    /// Hardcoding `/tmp` is the bug this is here to prevent coming back.
    #[test]
    fn the_per_run_disk_lives_under_the_platform_temp_dir() {
        let p = plan(Recipe::Retail, &[], true).unwrap();
        let disk = p
            .runs[0]
            .iter()
            .find_map(|a| a.strip_prefix("--disk="))
            .expect("retail passes --disk=");
        assert!(
            Path::new(disk).starts_with(std::env::temp_dir()),
            "{disk} is not under {}",
            std::env::temp_dir().display()
        );
        assert_eq!(p.cleanup.as_deref(), Some(Path::new(disk)));
    }

    /// Paths in this project contain spaces. A `--print` line that cannot be pasted into a shell is
    /// worse than no `--print` line, because it looks like it can.
    #[test]
    fn print_quotes_paths_with_spaces() {
        let q = shell_quote(
            Path::new("/x/trace"),
            &["/res/Cracked Click Wheel Games/t.bin".to_string(), "--clock=5".to_string()],
        );
        assert_eq!(q, "/x/trace '/res/Cracked Click Wheel Games/t.bin' --clock=5");
    }

    #[test]
    fn every_recipe_name_round_trips() {
        for (name, want) in [
            ("retail", Recipe::Retail),
            ("cold", Recipe::Cold),
            ("warm", Recipe::Warm),
            ("flsh", Recipe::Flsh),
            ("rockbox", Recipe::Rockbox),
            ("flash-update", Recipe::FlashUpdate),
            ("from-idle", Recipe::FromIdle),
        ] {
            assert_eq!(Recipe::parse(name), Some(want));
            assert!(scripts_dir().join(want.script()).is_file(), "{}", want.script());
        }
        assert_eq!(Recipe::parse("nonsense"), None);
    }

    fn scripts_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("ipod-boot")
    }
}
