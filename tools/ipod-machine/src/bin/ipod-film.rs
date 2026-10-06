//! `ipod-film` — turn a run's panel into a PNG sequence, and that into the shipped assets.
//!
//! The capture, the deduplication and the manifest are the emulator's ([`ipod_machine::film`]); this
//! is everything that used to be shell around it. Three jobs:
//!
//! ```text
//! ipod-film run   --out=DIR [--every=N] [--rate=N] [--fps=N] [--from=N] [-- trace flags…]
//! ipod-film asset boot | gameplay | all        the shipped films and stills, with their calibration
//! ipod-film concat DIR                         rebuild frames.concat from frames.tsv, run nothing
//! ```
//!
//! **`ffmpeg` is optional.** Without it the PNG sequence *is* the deliverable and the concat list is
//! written anyway, ready for a machine that has one.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let cmd = argv.first().map(String::as_str).unwrap_or("");
    let rest: Vec<String> = argv.iter().skip(1).cloned().collect();
    let r = match cmd {
        "run" => run(&rest),
        "asset" => asset(&rest),
        "concat" => {
            let d = rest
                .first()
                .map(PathBuf::from)
                .ok_or("usage: ipod-film concat DIR".to_string());
            d.and_then(|d| concat(&d).map(|_| ()))
        }
        _ => {
            eprintln!("{}", USAGE);
            std::process::exit(2);
        }
    };
    if let Err(e) = r {
        eprintln!("ipod-film: {e}");
        std::process::exit(1);
    }
}

const USAGE: &str = "\
usage:
  ipod-film run   --out=DIR [--every=N] [--fps=N] [--from=N] [--scale=N]
                  [--realtime] [--cap=SECONDS] [-- trace flags…]
  ipod-film asset boot | gameplay | diag | all
  ipod-film concat DIR

  RECIPE=retail|flsh|rockbox|warm|loader
                                    which machine to film (default retail). Any `ipod-boot`
                                    recipe works — RECIPE is passed straight through as the
                                    subcommand — so this list is what is useful, not what is
                                    accepted.
  IMG=diag|disk                     which NOR image, when RECIPE=flsh
  DISK=PATH                         the drive, for the recipes that take one. `loader` needs one
                                    with iPodLinux installed — `ipod-boot install-linux` builds it.
                                    That boot is long: the kernel's own output begins around 8 G
                                    instructions and ZeroSlackr's screen around 20 G, so film it
                                    with BUDGET=25000000000 and --from= set past the bootloader.
  BUDGET=N                          run length for every recipe but retail
  --realtime                        one sample lasts `--every`/75 microseconds, so the film runs
                                    at the machine's own speed instead of one second per sample
  --cap=SECONDS                     longest any one frame may hold. Opt-in, because a cap is a
                                    lie about duration — but the last frame of a run holds until
                                    the budget ends, which is a fact about the budget";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// `ipod-boot`, beside this binary — the same sibling rule the recipes use, so an unpacked archive
/// works with no configuration.
fn ipod_boot() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("ipod-boot")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("ipod-boot"))
}

fn have_ffmpeg() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Suffixed counts: `2M`, `100k`, `2600000000`.
fn count(s: &str) -> Result<u64, String> {
    let t = s.trim();
    let (n, mul) = match t.chars().last() {
        Some('M') | Some('m') => (&t[..t.len() - 1], 1_000_000),
        Some('k') | Some('K') => (&t[..t.len() - 1], 1_000),
        _ => (t, 1),
    };
    n.parse::<u64>()
        .map(|v| v * mul)
        .map_err(|_| format!("{s}: not a count"))
}

fn arg<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.iter().find_map(|a| a.strip_prefix(&format!("{key}=")))
}

/// How long each frame is on screen, and in whose time.
#[derive(Clone, Copy)]
struct Pace {
    /// Play at the **machine's** speed: one sample lasts `every / CLOCK` microseconds, so a screen
    /// that the firmware held for a second is on screen for a second.
    ///
    /// The default is one second per sample, which is not a time at all — it is a count wearing
    /// seconds' clothing, and it is only watchable because the boot films happen to have short
    /// holds. A tour of a menu does not: `diag` sits on its splash for 39 samples and on its last
    /// screen for 461, which at a second each is eight minutes of a still picture.
    realtime: bool,
    /// Longest any single frame may hold, in seconds. `None` for no cap.
    ///
    /// A cap is a **lie about duration**, so it is opt-in and never the default: the last frame of
    /// a film usually holds until the budget runs out, which says how long the run was and nothing
    /// about the machine.
    cap: Option<f64>,
}

impl Pace {
    const NATURAL: Pace = Pace {
        realtime: false,
        cap: None,
    };

    /// Seconds on screen for a frame the firmware held for `held_instr` instructions, in a film
    /// sampled every `rate` instructions.
    fn of(&self, held_instr: f64, rate: f64) -> f64 {
        let d = if self.realtime {
            // `held_instr / CLOCK` is microseconds of simulated time, by the definition of
            // `--clock`: instructions per simulated microsecond.
            held_instr / ipod_machine::CLOCK as f64 / 1_000_000.0
        } else {
            held_instr / rate
        };
        self.cap.map_or(d, |c| d.min(c))
    }
}

/// Build `frames.concat` and `frames.total` from the manifest the emulator wrote.
///
/// Two things about the last entry, both learned by measuring the output rather than trusting it.
/// The concat demuxer **will not honour the final entry's duration** unless something follows it,
/// so the last file is listed twice; the repeat then plays for that duration a second time, which
/// is what the caller's `-t TOTAL` trims back off.
fn concat(dir: &Path) -> Result<f64, String> {
    concat_paced(dir, Pace::NATURAL)
}

fn concat_paced(dir: &Path, pace: Pace) -> Result<f64, String> {
    let tsv = dir.join("frames.tsv");
    let text = std::fs::read_to_string(&tsv).map_err(|e| format!("{}: {e}", tsv.display()))?;
    // `# film of …, sampled every N instructions` — the rate the durations are expressed against.
    let rate: f64 = text
        .lines()
        .find_map(|l| {
            l.strip_prefix("# film of ")
                .and_then(|_| l.split("every ").nth(1))
        })
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or("frames.tsv has no `sampled every N instructions` header to scale durations by")?;

    // The columns are found **by name**, from the `# index\tfile\t…` header. They were a fixed
    // index once, and when the writer gained `first_usec` (4fdec71) the index went on reading the
    // neighbouring column — `samples`, a count of 1 to a few hundred — as a duration in
    // instructions, so every film came out 0.00 s long while this file's own test, still holding
    // the old layout, passed. A manifest without the column is an error, not a guess.
    let header: Vec<&str> = text
        .lines()
        .find_map(|l| l.strip_prefix("# ").filter(|h| h.starts_with("index\t")))
        .ok_or("frames.tsv has no `# index\tfile\t…` column header")?
        .split('\t')
        .map(str::trim)
        .collect();
    let col = |names: &[&str]| {
        names
            .iter()
            .find_map(|n| header.iter().position(|h| h == n))
            .ok_or_else(|| format!("frames.tsv has no `{}` column: {header:?}", names[0]))
    };
    let file_col = col(&["file"])?;
    // `held` is the same column under the name it had before 4fdec71, so an old film still concats.
    let held_col = col(&["held_instr", "held"])?;

    let mut out = String::new();
    let mut total = 0.0f64;
    let mut last: Option<String> = None;
    for l in text.lines() {
        if l.starts_with('#') || l.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = l.split('\t').collect();
        let (Some(file), Some(held)) = (f.get(file_col), f.get(held_col)) else {
            return Err(format!("frames.tsv row is shorter than its header: {l:?}"));
        };
        let held: f64 = held
            .trim()
            .parse()
            .map_err(|_| format!("bad held_instr: {held}"))?;
        let d = pace.of(held, rate);
        out.push_str(&format!("file '{file}'\nduration {d:.4}\n"));
        total += d;
        last = Some(file.to_string());
    }
    if let Some(l) = last {
        out.push_str(&format!("file '{l}'\n"));
    }
    std::fs::write(dir.join("frames.concat"), out).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("frames.total"), format!("{total:.4}\n")).map_err(|e| e.to_string())?;
    Ok(total)
}

/// `ipod-film run` — film a boot.
fn run(args: &[String]) -> Result<(), String> {
    let out = PathBuf::from(arg(args, "--out").ok_or("run needs --out=DIR")?);
    let every = arg(args, "--every").unwrap_or("2M");
    let fps = arg(args, "--fps").unwrap_or("30");
    let from = arg(args, "--from").map(count).transpose()?.unwrap_or(0);
    let scale: u32 = arg(args, "--scale")
        .unwrap_or("1")
        .parse()
        .map_err(|_| "--scale wants a number")?;
    let pace = Pace {
        realtime: args.iter().any(|a| a == "--realtime"),
        cap: arg(args, "--cap")
            .map(|s| s.parse::<f64>())
            .transpose()
            .map_err(|_| "--cap wants seconds")?,
    };
    let pass: Vec<String> = args
        .iter()
        .skip_while(|a| *a != "--")
        .skip(1)
        .cloned()
        .collect();

    let _ = std::fs::create_dir_all(&out);
    for e in std::fs::read_dir(&out).into_iter().flatten().flatten() {
        let n = e.file_name();
        let n = n.to_string_lossy();
        if n.starts_with("frame-") || n.starts_with("frames.") || n.starts_with("film.") {
            let _ = std::fs::remove_file(e.path());
        }
    }

    // RECIPE picks which machine is filmed. `retail` is the default and carries two flags that
    // belong to it and to nothing else:
    //
    //   --stop-when-idle  ends the run once RetailOS stops reaching new code. On a NOR image it
    //                     would end the film during the boot, because `diag` idles in a 150 ms
    //                     delay loop over code it has already run — busy, and novel to nothing.
    //   --bcm-registry    ledger #6, and off by default everywhere else for the reason recorded
    //                     there: it changes every number in the run.
    //
    // Anything else takes its length from BUDGET, which `ipod-boot` already reads.
    let recipe = std::env::var("RECIPE").unwrap_or_else(|_| "retail".into());
    let idle = std::env::var("IDLE").unwrap_or_else(|_| "400000000".into());
    let mut c = Command::new(ipod_boot());
    c.arg(&recipe);
    if recipe == "retail" {
        c.arg(format!("--stop-when-idle={idle}"))
            .arg("--bcm-registry");
    }
    c.arg(format!(
        "--bcm-film=0xE0000:140:F0:{every}:{}",
        out.display()
    ))
    .arg(format!("--bcm-film-from={from}"))
    .args(&pass);
    let st = c
        .status()
        .map_err(|e| format!("{}: {e}", ipod_boot().display()))?;
    if !st.success() {
        return Err("the run failed; no film written".into());
    }

    let total = concat_paced(&out, pace)?;
    let pngs = std::fs::read_dir(&out)
        .map(|d| {
            d.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("frame-"))
                .count()
        })
        .unwrap_or(0);
    println!("assembled a {pngs}-frame concat list ({total:.2}s)");

    if !have_ffmpeg() {
        println!(
            "ffmpeg is not on this machine, so the PNG sequence IS the deliverable:\n  \
             {0}/frame-*.png    exact 320x240 frames, one per distinct screen\n  \
             {0}/frames.tsv     when each appeared, how long it held, its digest\n  \
             {0}/frames.concat  ready for a machine that has one",
            out.display()
        );
        return Ok(());
    }
    // **A GIF, and no video.** Both were written for years and the three MP4s in `docs/media/`
    // were referenced by nothing — 723 KB of files no reader ever reached, because a GIF plays
    // inline in a README and a video does not. The PNG sequence is still the archival form and
    // `frames.concat` is still written, so anyone who wants a video has the inputs.
    let vf = if scale == 1 {
        format!("fps={fps}")
    } else {
        format!("scale=iw*{scale}:ih*{scale}:flags=neighbor,fps={fps}")
    };
    let gif = out.join("film.gif");
    let vf =
        format!("{vf},split[a][b];[a]palettegen=stats_mode=full[p];[b][p]paletteuse=dither=none");
    let concat = out.join("frames.concat").display().to_string();
    let total_s = format!("{total:.4}");
    let args =
        ["-f", "concat", "-safe", "0", "-i", &concat, "-t", &total_s, "-vf", &vf, "-loop", "0"];
    ffmpeg_gif(&args, &gif.display().to_string())?;
    println!("film -> {} ({total:.2}s)", gif.display());
    Ok(())
}

fn ffmpeg(args: &[&str]) -> Result<(), String> {
    let st = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .status()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    if st.success() {
        Ok(())
    } else {
        Err("ffmpeg failed".into())
    }
}

/// Every gif this program writes goes through here, so no encode can forget the flag below.
fn ffmpeg_gif(args: &[&str], gif: &str) -> Result<(), String> {
    ffmpeg(&gif_argv(args, gif))
}

/// **`-gifflags -transdiff`**: ffmpeg's gif muxer defaults to `+transdiff`, which stores each frame
/// as a difference against the previous one and punches a transparent index into every pixel it
/// judges unchanged. With disposal 1 those holes show the *earlier* frame through, and the result
/// is pixels no frame of the machine ever produced: `ipod-15-rockbox-wheel.gif` carried yellow
/// ticks at a 24 px period on row 23 — the keyframe's selection bar showing through 26 holes — in a
/// film whose raw frames read one clean run of 320 px on that row. KNOWN-BUGS, *"The gif encode
/// invents pixels the panel never showed"*. Off, every frame is opaque and is exactly its own
/// picture; `tests/shipped_gifs.rs` holds every gif in `docs/media/` to that.
const GIF_FLAGS: [&str; 2] = ["-gifflags", "-transdiff"];

fn gif_argv<'a>(args: &[&'a str], gif: &'a str) -> Vec<&'a str> {
    let mut v = args.to_vec();
    v.extend(GIF_FLAGS);
    v.push(gif);
    v
}

/// How a film's frames become a gif. The two published films want opposite answers and the reason
/// is measured, so it is spelled at the call site rather than defaulted.
#[derive(Clone, Copy, PartialEq)]
enum Palette {
    /// One palette PER FRAME, frames held at their real durations. Correct, and the default unless
    /// there is a stated reason.
    Held,
    /// One palette for the WHOLE film, frames resampled to a constant rate. Wrong on colour, and
    /// needs a reason every time — see the gameplay recipe.
    Resampled,
}

/// Publish a film directory as a gif, upscaled 2x nearest-neighbour.
///
/// **The palette.** `stats_mode=single` is one palette per frame. This used to be
/// `stats_mode=full` — one palette for the whole film — and that is wrong by a measured margin: the
/// boot film is 24 distinct screens whose colours union to 548, and one 256-entry table cannot hold
/// 548, so it quantised. The main menu reached the gif with 147 of its 211 colours and Brick's
/// playfield with 167 of its 238, which is why the battery's green and Brick's bricks read wrong in
/// the gif while the stills beside them, written from the same PNGs, read right.
///
/// `reserve_transparent=0` spends the 256th slot on a colour rather than a transparency index that
/// opaque panel frames never use. `new=1` is what makes the rest real: without it `paletteuse` takes
/// the first palette and reuses it, so the per-frame palettes are generated and thrown away.
///
/// **`-fps_mode vfr` goes with the palette.** A new palette per frame forces every frame to a full
/// keyframe, so resampling 24 screens up to 1084 constant-rate frames writes 1084 keyframes —
/// 21.5 MB against 617 KB held at real durations.
///
/// **`-final_delay`** is the held half of the `-t TOTAL` problem: held, the trim removes the
/// trailing duplicate that gives the last screen its length, and the film measures short of its own
/// manifest. A film whose length disagrees with its manifest is an instrument that lies.
fn publish(dir: &Path, name: &str, fps: u32, mode: Palette, post: &Path) -> Result<(), String> {
    let concat_path = dir.join("frames.concat");
    if !concat_path.exists() {
        return Err(format!("no film in {}", dir.display()));
    }
    let total = std::fs::read_to_string(dir.join("frames.total"))
        .map_err(|e| e.to_string())?
        .trim()
        .to_string();
    std::fs::create_dir_all(post).map_err(|e| e.to_string())?;
    let cp = concat_path.display().to_string();
    let gif = post.join(format!("{name}.gif")).display().to_string();

    // `dither=none`: a 16-bit UI of flat fills and one-pixel rules; dithering adds noise that was
    // never on the panel.
    match mode {
        Palette::Held => {
            let vf = "scale=iw*2:ih*2:flags=neighbor,split[a][b];\
                      [a]palettegen=stats_mode=single:reserve_transparent=0[p];\
                      [b][p]paletteuse=dither=none:new=1";
            let fd = final_delay(dir)?;
            let args = [
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                &cp,
                "-t",
                &total,
                "-vf",
                vf,
                "-fps_mode",
                "vfr",
                "-final_delay",
                &fd,
                "-loop",
                "0",
            ];
            ffmpeg_gif(&args, &gif)?;
        }
        Palette::Resampled => {
            let vf = format!(
                "scale=iw*2:ih*2:flags=neighbor,fps={fps},split[a][b];\
                 [a]palettegen=stats_mode=full[p];[b][p]paletteuse=dither=none"
            );
            ffmpeg_gif(
                &["-f", "concat", "-safe", "0", "-i", &cp, "-t", &total, "-vf", &vf, "-loop", "0"],
                &gif,
            )?;
        }
    }
    println!("  -> {gif}   (640x480, {total}s, {fps} fps)");
    Ok(())
}

/// The last entry's duration, in centiseconds, for `-final_delay`.
fn final_delay(dir: &Path) -> Result<String, String> {
    let t = std::fs::read_to_string(dir.join("frames.concat")).map_err(|e| e.to_string())?;
    let d = t
        .lines()
        .filter_map(|l| l.strip_prefix("duration "))
        .next_back()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(0.0);
    Ok(format!("{}", (d * 100.0 + 0.5) as i64))
}

/// One frame of a film, upscaled 2x, as a still.
fn still(dir: &Path, frame: &str, name: &str, post: &Path) -> Result<(), String> {
    let src = dir.join(frame);
    if !src.exists() {
        println!("  (no {frame} in {} — skipping {name})", dir.display());
        return Ok(());
    }
    std::fs::create_dir_all(post).map_err(|e| e.to_string())?;
    let out = post.join(format!("{name}.png")).display().to_string();
    ffmpeg(&[
        "-i",
        &src.display().to_string(),
        "-vf",
        "scale=iw*2:ih*2:flags=neighbor",
        &out,
    ])?;
    println!("  -> {out}");
    Ok(())
}

fn asset(args: &[String]) -> Result<(), String> {
    let which = args.first().map(String::as_str).unwrap_or("all");
    let root = repo_root();
    let film = root.join("_out/film");
    let post = root.join("_out/post");
    if which == "boot" || which == "all" {
        do_boot(&film, &post)?;
    }
    if which == "gameplay" || which == "all" {
        do_gameplay(&film, &post)?;
    }
    if which == "diag" || which == "all" {
        do_diag(&film, &post)?;
    }
    Ok(())
}

/// The tour through Apple's service diagnostics, as one wheel script.
///
/// **The holds are the whole calibration, and they are now stated in the unit they mean.**
/// `diag`'s main loop is `read the button byte, sleep 150 ms`, so a press shorter than that
/// falls between two polls and is never seen. `press=` expands to a down/up pair one click
/// apart, and using it here produced a run where the interrupt handler demonstrably recorded
/// MENU and the firmware demonstrably ignored it. Every button below is an explicit
/// `down=`/`up=` pair held **500 ms**, with 700 ms of quiet after it for the screen to settle.
///
/// These were `25M` and `35M` instructions, which is 333 ms and 467 ms **at the real clock and
/// nothing like that at any other** — 5 s and 7 s at `--clock=5`. A duration written in
/// instructions is a duration only for one clock, and every recipe here is run at a different
/// one from the one it was calibrated against. `500ms` is 500 ms to the firmware always, which
/// is the thing the 150 ms poll is being compared against.
///
/// **`@5s`**: diagnostics settles onto its boot screen at **3.2 s simulated**, measured with
/// `--bcm-film`'s `first_usec` column.
fn diag_tour() -> String {
    // One press: hold it across at least two of the firmware's polls, then let the screen settle.
    let press = |b: &str| format!(",+700ms:down={b},+500ms:up={b}");
    let scroll = ",+700ms:rotate=+8";
    let mut w = String::from("@5s:touch,+300ms:down=menu,+500ms:up=menu"); // -> the manual-test menu
    w.push_str(scroll); // Memory -> IO
    w.push_str(&press("select")); // -> Comms / Wheel / Display / HeadphoneDetect / HardDrive
    w.push_str(scroll); // Comms -> Wheel
    w.push_str(&press("select")); // -> KeyTest / WheelTest
    w.push_str(&press("select")); // -> Key Test, which asks for all five keys
    for b in ["play", "left", "right", "menu"] {
        w.push_str(&press(b));
    }
    // `select` last: Key Test takes MENU as "exit" only once the other four are done, so pressing
    // the action key last is what leaves KEY PASS on screen instead of leaving the test.
    w.push_str(&press("select"));
    w.push_str(",+1s:release");
    w
}

fn do_diag(film: &Path, post: &Path) -> Result<(), String> {
    println!("== Apple's diagnostics, driven ==");
    let dir = film.join("diagnostics");
    // 1.5 G is the tour plus a tail: the last screen holds until the budget ends, and `--cap`
    // trims that back to something watchable rather than pretending the run was shorter.
    std::env::set_var("RECIPE", "flsh");
    std::env::set_var("IMG", "diag");
    std::env::set_var("BUDGET", "1500000000");
    run(&[
        format!("--out={}", dir.display()),
        "--every=2M".into(),
        "--fps=30".into(),
        "--realtime".into(),
        "--cap=2.5".into(),
        "--".into(),
        "--clickwheel".into(),
        format!("--wheel={}", diag_tour()),
    ])?;
    publish(&dir, "ipod-22-diagnostics", 30, Palette::Held, post)?;
    // Frame indices, not instruction counts, because the film's dedup assigns them. `frames.tsv`
    // is the check: the non-black counts are 70669 / 68428 / 67959 / 69429, and a still whose
    // count does not match is a still of the wrong screen — which has happened here before.
    for (f, n) in [
        ("frame-00004.png", "ipod-19-diagnostics"),
        ("frame-00007.png", "ipod-20-diagnostics-menu"),
        ("frame-00010.png", "ipod-21-diagnostics-io"),
        ("frame-00021.png", "ipod-23-diagnostics-keytest"),
    ] {
        still(&dir, f, n, post)?;
    }
    Ok(())
}

// The descent, as named pieces — one row per gesture, with quiet either side. NOT eight clicks
// inside a longer burst: a continuous burst accelerates and the same count moves three rows.
fn to_brick() -> String {
    // **Anchored in simulated time, and it has to be.** These were instruction counts — `@1500M`
    // for the first Select — and on 2026-09-01 the whole descent fired **0 of 48 steps**: a 4 G
    // budget retires 895 M instructions and reaches 800 s, because the machine spends its budget
    // halted. How far a budget gets changes with the machine — the interrupt-wake fix alone took a
    // 3 G budget from 395 M retired to 2.69 G — so an instruction anchor is a number calibrated
    // against a machine that then moves out from under it. AGENTS.md §6 names this trap by its
    // symptom, and research/12 records it happening to NEXT.md's recipe before this one.
    //
    // The firmware's own timers run on the simulated clock, so a screen that appears 73 seconds
    // into a boot appears there whatever the host does.
    //
    // **`@80s`**: the language picker draws at **73.2 s simulated** — measured with `--bcm-film`'s
    // `first_usec` column, on the retail ROM and a drive built from `iPod_20.1.3`.
    //
    // ⚠️ **That 73.2 s is stale, and so is every gap below it — measured 2026-09-07.** Same ROM,
    // same IPSW, same `first_usec` column: the picker draws at **4.8 s**, the main menu answers the
    // first Select **58.3 s** later, a one-row gesture lands in **0.6-1 s**, a Select that opens a
    // submenu takes **6-9 s**, and the Select that launches Brick takes **15.7 s**. The 73.2 s was
    // taken on 2026-09-01, before `7e30c1f` stopped a halt from inventing simulated time. So `@80s`
    // now fires seventy-five seconds after the screen it aims at — harmless in itself — and every
    // `+1500ms` after it is a gap calibrated against a machine that answered on a different
    // schedule, which is not.
    //
    // **The unit is right and the calibration is not**, and those are different bugs: re-basing
    // this on simulated time was a real fix and is not being undone. research/18 §2 carries a
    // descent measured at this clock that lands all nine of its gestures, and its §4 the two-arm
    // control that says it played. This is left as it stands rather than being edited to match,
    // because `do_gameplay` below it cannot run at all (its own anchors are still instruction
    // counts, and `parse_wheel_script` refuses the mixed script — confirmed by running it,
    // 2026-09-07) and re-basing the pair means re-reading a rally off its own run at HEAD, which is
    // research/13 §10.4a's rule and a piece of work of its own. NEXT.md 0b tracks all three.
    //
    // **`down=`/`up=` rather than `press=`**, for the reason `diag_tour` gives at length: `press=`
    // expands to a down/up pair one click apart, and a firmware that polls cannot see it.
    let head = "@80s:touch,+150ms:down=select,+300ms:up=select,+150ms:release";
    let row = ",+1500ms:touch,+150ms:rotate=+8,+400ms:release";
    let sel = ",+1500ms:touch,+150ms:down=select,+300ms:up=select,+150ms:release";
    let to_games = format!("{head}{row}{row}{row}{sel}{row}");
    format!(
        "{to_games}{sel},+3s:touch,+150ms:rotate=+8,+400ms:release{row}{row}{row}{row}\
         ,+2s:touch,+150ms:down=select,+300ms:up=select,+150ms:release"
    )
}

fn do_boot(film: &Path, post: &Path) -> Result<(), String> {
    println!("== boot to Brick ==");
    let dir = film.join("boot-to-brick");
    let w = to_brick();
    run(&[
        format!("--out={}", dir.display()),
        "--every=2M".into(),
        "--fps=30".into(),
        "--".into(),
        "--clickwheel".into(),
        format!("--wheel={w}"),
    ])?;
    // `held`: 24 distinct screens, none under the muxer's 4 cs delay floor, and a 548-colour union
    // one palette cannot hold. This is the film the wrong-colour report was about.
    publish(&dir, "ipod-01-boot-to-brick", 30, Palette::Held, post)?;
    // Frame indices, not instruction counts, because the film's dedup assigns them. `frames.tsv` is
    // the check: the non-black counts are 75267 / 75791 / 75565 / 74160 / 76763 / 2916, and three of
    // these indices were stale once — publishing three stills of the wrong screen from a script that
    // looked like it worked.
    for (f, n) in [
        ("frame-00004.png", "ipod-02-language"),
        ("frame-00006.png", "ipod-03-main-menu"),
        ("frame-00011.png", "ipod-04-extras"),
        ("frame-00015.png", "ipod-05-games-list"),
        ("frame-00023.png", "ipod-06-brick"),
        ("frame-00002.png", "ipod-07-apple-logo"),
    ] {
        still(&dir, f, n, post)?;
    }
    Ok(())
}

fn do_gameplay(film: &Path, post: &Path) -> Result<(), String> {
    println!("== Brick, played ==");
    let dir = film.join("brick-gameplay");
    // The centre button serves. The ball moves (+-8,+-10) px per tick and every bounce flips exactly
    // one sign — where it lands on the paddle does not steer it. The paddle is 57 px wide, travels
    // [4,262], and moves in 24 px QUANTA, acted on about 750 k instructions later.
    let mut w = String::from(",@2502340000:touch,+2M:rotate=+8,+5M:release");
    w.push_str(",@2539480000:touch,+2M:rotate=+8,+5M:release");
    w.push_str(",@2576620000:touch,+2M:press=select");
    // Two sweeps, and only two, because the ball tells you where to be. 200 k apart is calibration,
    // not tidiness: the same steps 400 k apart move the paddle 29 px per million instructions and
    // 200 k apart move it 150, because the wheel accelerator is rate-sensitive.
    for (at, n, d) in [(2581600000u64, 10, 8i32), (2601500000, 5, -8)] {
        w.push_str(&format!(",@{at}:rotate={d:+}"));
        for _ in 1..n {
            w.push_str(&format!(",+200k:rotate={d:+}"));
        }
    }
    w.push_str(",+2M:release");
    run(&[
        format!("--out={}", dir.display()),
        "--every=100k".into(),
        "--from=2574M".into(),
        "--fps=50".into(),
        "--".into(),
        "--clickwheel".into(),
        format!("--wheel={}{w}", to_brick()),
    ])?;
    // `resampled`, deliberately, and it is the worse option on colour: 93 of this film's 253 screens
    // are held 0.02 s, under the gif muxer's 4 cs floor, so encoding it held merges 38 of them away
    // — motion is this film's entire subject. It keeps a real defect (238 -> 229, about a tenth of
    // the boot film's) to keep every frame.
    publish(&dir, "ipod-08-brick-gameplay", 50, Palette::Resampled, post)?;
    still(&dir, "frame-00060.png", "ipod-09-brick-rally", post)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gif-palette bug's guard on the encoder side: transdiff is off on every gif, and the flag
    /// sits before the output path, where ffmpeg reads it as an output option.
    #[test]
    fn every_gif_is_encoded_without_transparent_differences() {
        let v = gif_argv(&["-i", "in", "-loop", "0"], "out.gif");
        assert_eq!(v.last(), Some(&"out.gif"));
        let at = v.iter().position(|a| *a == "-gifflags").expect("-gifflags missing");
        assert_eq!(v[at + 1], "-transdiff");
        assert!(at + 1 < v.len() - 1, "the flag must precede the output");
        // And nothing in this file calls the raw `ffmpeg` with a `.gif` output behind its back.
        let src = include_str!("ipod-film.rs");
        let raw_gif_calls = src
            .split("ffmpeg(&[")
            .skip(1)
            .filter(|call| call.split("])").next().unwrap_or("").contains("&gif"))
            .count();
        assert_eq!(raw_gif_calls, 0, "a gif encode bypasses ffmpeg_gif");
    }

    #[test]
    fn suffixed_counts_parse() {
        assert_eq!(count("2M").unwrap(), 2_000_000);
        assert_eq!(count("100k").unwrap(), 100_000);
        assert_eq!(count("2600000000").unwrap(), 2_600_000_000);
        assert!(count("banana").is_err());
    }

    /// The concat list is built from the manifest, and the last file is listed **twice** — the
    /// demuxer will not honour the final entry's duration otherwise. Asserted rather than trusted,
    /// because that repeat is the reason `-t TOTAL` exists at the call site.
    ///
    /// **The manifest is written by the emulator's own writer**, [`Film::finish`], not typed out
    /// here. A hand-written copy is what let this test pass for a month while every film came out
    /// 0.00 s: the writer gained a column, the copy did not, and the reader's fixed index was only
    /// ever checked against the copy.
    #[test]
    fn the_concat_list_repeats_its_last_frame_and_totals_the_durations() {
        use ipod_machine::film::{Film, Frame};
        let d = std::env::temp_dir().join(format!("ipod-film-test-{}", std::process::id()));
        let mut film = Film::parse(&format!("0xE0000:140:F0:1M:{}", d.display())).unwrap();
        // 1 sample then 2: held 1 M and 2 M instructions, so 1 s + 2 s at the natural pace. The
        // sample counts differ from the held counts on purpose — reading the wrong one shows.
        for (index, samples) in [(0usize, 1u64), (1, 2)] {
            film.frames.push(Frame {
                index,
                file: format!("frame-{index:05}.png"),
                repeat_of: None,
                first_at: index as u64 * 1_000_000,
                last_at: (index as u64 + samples - 1) * 1_000_000,
                first_usec: index as u64 * 13_333,
                last_usec: (index as u64 + samples - 1) * 13_333,
                samples,
                nonblack: 5,
                digest: index as u64,
            });
        }
        film.samples = 3;
        film.finish();
        let total = concat(&d).unwrap();
        let c = std::fs::read_to_string(d.join("frames.concat")).unwrap();
        assert_eq!(
            c.matches("file 'frame-00001.png'").count(),
            2,
            "last frame must be listed twice"
        );
        assert!(
            (total - 3.0).abs() < 1e-6,
            "durations total 1s + 2s, got {total}"
        );
        assert!(std::fs::read_to_string(d.join("frames.total"))
            .unwrap()
            .starts_with("3.0"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A manifest without a `held_instr` column is refused, not read at whatever index it used to
    /// be at — a film 0.00 s long is the failure this exists to make loud.
    #[test]
    fn a_manifest_without_the_held_column_is_an_error() {
        let d = std::env::temp_dir().join(format!("ipod-film-nohold-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("frames.tsv"),
            "# film of 0x000e0000, 320x240, sampled every 1000000 instructions\n\
             # index\tfile\trepeat_of\tfirst_instr\tsamples\tnonblack\n\
             0\tframe-00000.png\t-\t0\t1\t5\n",
        )
        .unwrap();
        let e = concat(&d).unwrap_err();
        assert!(e.contains("held_instr"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
