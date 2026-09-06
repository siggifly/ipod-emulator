//! A control socket, so the emulator can be driven and observed by something that is not a person.
//!
//! **Why this exists.** Every measurement so far has needed somebody sitting in front of the window
//! to scroll a wheel and press a button, and then to describe what happened. That does not compose
//! with anything: a question like *"does `[0x14937194]` change when this title launches"* is one
//! memory read and forty wheel clicks, and the wheel clicks were the hard part.
//!
//! Line protocol on a Unix socket. One command per line, one reply per line, no framing beyond
//! that, because the whole point is to be drivable from a shell.
//!
//! ```text
//! help               the vocabulary, so a connection can discover it without this file
//! wheel N [MS]       scroll N detents, MS apart; negative is anticlockwise
//! press NAME         select | menu | play | left/prev | right/next
//! hold NAME MS       hold a button, for the combos that need one
//! holdsw on|off      the HOLD SWITCH, which is not a button and not `hold`
//! snapshot           re-take the idle snapshot here, so this is where launches resume
//! shot PATH          write the current framebuffer as a PNG
//! peek ADDR          read one word, hex in and hex out; unmapped says so
//! ata FROM TO        whether the drive was ever asked for these sectors
//! state              the phase, the instruction count, the panel and the wheel as the MACHINE
//!                    has it — see the verb for why the last of those is in there
//! quit               close this connection (the emulator keeps running)
//! ```
//!
//! Read-only where it can be: `peek` goes through `Memory::peek32`, which walks the regions
//! directly rather than through the access counters, so observing costs nothing and changes no
//! number any report has produced.
//!
//! ## How anything reaches it — and why that sentence had to be written
//!
//! **This module shipped complete and unreachable.** The protocol below worked, `emu.rs`'s run
//! loop answered five of its sentinels, and `serve` had no caller anywhere in the program;
//! `--control=` was refused by `args.rs` as a flag belonging to `trace`, and a module-wide
//! `#[allow(dead_code)]` meant nothing warned. A socket nobody can open is the same defect, one
//! level up, as a control drawn live that does nothing.
//!
//! So: `--control=PATH` binds it, before the window opens and for as long as the process lives,
//! and it stays absent unless asked for — *a socket that appears without being asked for is an
//! interface nobody audited* was always the right argument for opt-in and never an argument for
//! unreachable.
//!
//! ## The bench, and why the socket does not hold a `Link`
//!
//! A [`Link`] is per-**machine**: `start_machine` makes one, and the window may go a whole session
//! without starting anything. A socket that took a `Link` at launch would have nothing to take, and
//! a socket opened *when a machine starts* would be absent exactly when somebody wants to ask what
//! is running.
//!
//! So the socket looks at a [`BENCH`] rather than holding a machine: one process-wide slot that
//! `start_machine` and `start_title` fill and `Live`'s drop empties. `state` on an empty bench is
//! an **answer** — *nothing is running* — and every verb that needs a machine says so rather than
//! timing out, which is the difference between an instrument that reports an absence and one that
//! is absent.

use crate::emu::{self, Link};
use ipod_machine::{wheel_button, WheelEvent};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Not an address. Asking for a count through the same request channel keeps the run loop
/// with one place that answers questions, rather than two that can drift apart.
pub const UNMAPPED_SENTINEL: u32 = 0xFFFF_FFFF;
pub const TRACE_SENTINEL: u32 = 0xFFFF_FFFE;
/// Ask the run loop for the PMU's per-register write census. Same request/answer route as the
/// others, because `Pcf50605` lives on the emulator thread and cannot be borrowed from this one.
pub const PMU_SENTINEL: u32 = 0xFFFF_FFFD;
/// Ask for the `--watch-writes` census: which words in the watched range have been written, how
/// often, and by whom. Live, so a control can be moved and the answer asked for immediately.
pub const WRITES_SENTINEL: u32 = 0xFFFF_FFFC;
/// Take the `--watch-writes` **value** log and clear it, so the next dump covers only what happened
/// since this one. Counting writes cannot tell two transaction types on one bus apart; their bytes
/// can, and this is how one click of a control gets read as a byte sequence.
pub const BUS_SENTINEL: u32 = 0xFFFF_FFFB;

/// **What is on the bench**, which is not the same thing as what the socket is.
///
/// Process-wide because the socket is: one listener, opened at launch, outliving every machine the
/// session starts. A `static` rather than a value threaded through `wire` because the two ends live
/// on different threads and in different ownership worlds — the window holds its `Live` in an `Rc`
/// on the UI thread, and no `Rc` can be handed to a connection thread.
static BENCH: Mutex<Option<Arc<Link>>> = Mutex::new(None);

/// A machine has started: this is what the socket now talks to.
///
/// Called by `start_machine` and `start_title`, **after** the previous `Live` has been dropped —
/// which is the order that matters, because that drop is what calls [`detach`]. Reversed, a new
/// machine would be attached and then immediately unattached by its predecessor's funeral.
pub fn attach(link: &Arc<Link>) {
    *BENCH.lock().unwrap() = Some(Arc::clone(link));
}

/// What is on the bench right now, cloned so the lock is not held across a command.
///
/// A command can sleep for a third of a second (`wheel`) or wait three (`peek`); holding the
/// bench's lock across one would stop the window starting a machine while a driver scrolled.
fn on_the_bench() -> Option<Arc<Link>> {
    BENCH.lock().unwrap().clone()
}

/// The machine is gone. `state` goes back to answering *nothing is running*.
///
/// Deliberately not "detach this particular link": there is one bench, whoever is leaving it is
/// leaving it, and a version that compared identities would need the caller to still hold the
/// `Arc` it is in the middle of dropping.
pub fn detach() {
    *BENCH.lock().unwrap() = None;
}

/// The sentence a launch prints when the socket is up.
///
/// **One wording, two destinations.** `fn main` writes it to stderr beside the window and
/// `args::run` writes it into its own `out` writer; two `format!`s would be two spellings of one
/// fact, and the one a person read would depend on which launch they used.
pub fn listening(path: &Path) -> String {
    format!(
        "control: listening on {}. One command per line; `help` lists them, `state` says what is \
         on the bench.",
        path.display()
    )
}

/// Start listening. Returns immediately; each connection is served on its own thread.
///
/// **Binding is the caller's to fail on.** An error here is returned rather than logged, because a
/// launch that asked for a socket and got a window without one is indistinguishable, from the
/// outside, from a socket nobody could connect to.
pub fn serve(path: &Path) -> Result<(), String> {
    // A socket left by a previous run would make `bind` fail with EADDRINUSE, and the previous run
    // is gone -- the file is not a lock, it is litter.
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).map_err(|e| format!("{}: {e}", path.display()))?;
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            std::thread::spawn(move || {
                if let Err(e) = session(stream) {
                    eprintln!("control: {e}");
                }
            });
        }
    });
    Ok(())
}

fn session(stream: UnixStream) -> Result<(), String> {
    let mut out = stream.try_clone().map_err(|e| e.to_string())?;
    for line in BufReader::new(stream).lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "quit" {
            return Ok(());
        }
        // **Read per command, not per connection.** A connection outlives the machine it first
        // saw: an agent holds one socket open across a power cycle, and a `Link` captured when the
        // session opened would go on answering for a machine that had stopped.
        let held = on_the_bench();
        let reply = command(line, held.as_ref());
        writeln!(out, "{reply}").map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// One line in, one line out.
///
/// **`link` is an `Option` and that is the whole of the empty-bench design.** Every verb but two
/// needs a machine; rather than each of them discovering that for itself — `peek` by timing out
/// after three seconds, `wheel` by sleeping through a scroll nothing received — there is one gate
/// here, and it answers in a sentence that says what to do next.
fn command(line: &str, link: Option<&Arc<Link>>) -> String {
    let mut it = line.split_whitespace();
    let verb = it.next().unwrap_or("");
    let arg = it.next().unwrap_or("");
    let arg2 = it.next().unwrap_or("");

    // The two that answer with no machine: what the vocabulary is, and what is running. Everything
    // else is a question about a machine, and gets the same answer when there is not one.
    if verb == "help" {
        // One reply per line is the whole framing this protocol has, so the vocabulary is folded
        // onto one. The separator is an ASCII bar rather than a middle dot because `geometry.rs`
        // holds a closed set of non-ASCII glyphs for strings this program emits, and `·` is not in
        // it — it is a symbol, and §6.7's answer for a symbol is that it is drawn, not typed.
        return HELP.replace('\n', " | ");
    }
    if verb == "state" && link.is_none() {
        return "ok bench=empty — no machine is running. Start one in the window (the centre \
                button on the bench), or launch with --headless=N to run one with no window."
            .into();
    }
    let Some(link) = link else {
        return format!(
            "error: {verb:?} needs a machine and the bench is empty. `state` says so without \
             asking for one; start a device in the window, or launch with --headless=N."
        );
    };

    match verb {
        "wheel" => match arg.parse::<i32>() {
            Ok(n) => {
                // **Timed to match what the window does**, because the firmware's wheel driver is
                // sampling and anything faster is invisible to it. The GUI emits one click per
                // repaint -- about 60 a second -- and then holds the finger on for 300 ms after the
                // last one before releasing, synthesising the lift a keyboard scroll never gives.
                //
                // A first attempt queued Touch/Step/Step/Release back to back. Every button still
                // worked and the menu never moved, which reads like the steps being dropped; they
                // were delivered, and were simply faster than the thing meant to observe them.
                let gap = arg2.parse::<u64>().unwrap_or(16);
                link.push(WheelEvent::Touch);
                for _ in 0..n.abs() {
                    link.push(WheelEvent::Step(if n > 0 { 1 } else { -1 }));
                    std::thread::sleep(std::time::Duration::from_millis(gap));
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
                link.push(WheelEvent::Release);
                format!("ok wheel {n} ({gap}ms apart, 300ms hold)")
            }
            Err(_) => "error: wheel wants a number of detents".into(),
        },
        // The switch, not a button. `hold` was already taken by "hold a button down for N ms",
        // which is a different physical act on a different piece of plastic.
        // Park the machine: whatever is on screen now becomes what the next launch resumes to.
        // Both halves are written, RAM and the drive beside it, because a snapshot without its
        // drive is the stale pair that used to produce "connect to computer" on every third start.
        "snapshot" => {
            link.resnap
                .store(true, std::sync::atomic::Ordering::Relaxed);
            "ok snapshot requested — written at the next slice".into()
        }
        "holdsw" => {
            let on = match arg {
                "on" | "1" | "engage" => true,
                "off" | "0" | "release" => false,
                _ => return "error: holdsw wants on or off".into(),
            };
            link.push(WheelEvent::Hold(on));
            format!("ok holdsw {}", if on { "on" } else { "off" })
        }
        "press" | "hold" => {
            let Some(mask) = wheel_button(arg) else {
                return format!("error: unknown button {arg:?} (select menu play left right)");
            };
            // A zero-length press is not a thing a finger can do, and RetailOS agreed: `press
            // select` on the Language screen did nothing at all, while `hold select 400` opened
            // the main menu. The button has to be down long enough for the firmware's own scan to
            // see it, so the default press is a short press rather than an instantaneous one.
            let ms: u64 = if verb == "hold" {
                arg2.parse().unwrap_or(400)
            } else {
                120
            };
            link.push(WheelEvent::Button(mask, true));
            if ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
            link.push(WheelEvent::Button(mask, false));
            format!("ok {verb} {arg}")
        }
        "shot" => {
            if arg.is_empty() {
                return "error: shot wants a path".into();
            }
            let out = link.out.lock().unwrap();
            let png = ipod_machine::png::encode(&out.fb, emu::FB_W, emu::FB_H);
            match std::fs::write(arg, &png) {
                Ok(()) => format!(
                    "ok shot {arg} ({} bytes, {} non-black)",
                    png.len(),
                    out.fb_nonzero
                ),
                Err(e) => format!("error: {arg}: {e}"),
            }
        }
        "peek" => match u32::from_str_radix(arg.trim_start_matches("0x"), 16) {
            Ok(addr) => {
                link.peek_req.lock().unwrap().push(addr);
                // The run loop answers between slices. A slice is milliseconds, so this is a short
                // wait -- but it is bounded, because a paused or wedged emulator must return an
                // answer rather than hang whoever asked.
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                loop {
                    {
                        let mut ans = link.peek_ans.lock().unwrap();
                        if let Some(i) = ans.iter().position(|(a, _)| *a == addr) {
                            let (_, v) = ans.remove(i);
                            return match v {
                                Some(v) => format!("ok {addr:#010x} = {v:#010x}"),
                                None => format!("ok {addr:#010x} = unmapped"),
                            };
                        }
                    }
                    if std::time::Instant::now() > deadline {
                        return format!("error: {addr:#010x} timed out — is the emulator running?");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
            Err(_) => "error: peek wants a hex address".into(),
        },
        // The operator's hypothesis, made answerable: if RetailOS reaches for a crypto block or any
        // other peripheral this model does not implement, those accesses land in unmapped space and
        // are counted. A DRM that fails because the hardware doing it is absent looks exactly like a
        // DRM that fails because the keys are wrong, and this is what tells them apart.
        "unmapped" => {
            // Ask through the peek channel so the list is refreshed between slices, then read it.
            link.peek_req.lock().unwrap().push(UNMAPPED_SENTINEL);
            std::thread::sleep(std::time::Duration::from_millis(120));
            let _ = link.peek_ans.lock().unwrap().pop();
            let out = link.out.lock().unwrap();
            if out.unmapped_pages.is_empty() {
                return "ok unmapped none".into();
            }
            let list: Vec<String> = out
                .unmapped_pages
                .iter()
                .map(|p| format!("{p:#010x}"))
                .collect();
            format!("ok unmapped {} page(s): {}", list.len(), list.join(" "))
        }
        // Did the machine ever ask the drive for these sectors?
        "ata" => {
            let (from, to) = match (arg.parse::<u64>(), arg2.parse::<u64>()) {
                (Ok(a), Ok(b)) => (a, b),
                _ => return "error: ata wants FROM TO as LBAs".into(),
            };
            *link.ata_query.lock().unwrap() = Some((from, to));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if let Some(a) = link.ata_answer.lock().unwrap().take() {
                    return a;
                }
                if std::time::Instant::now() > deadline {
                    return "error: timed out".into();
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        // Hand over the execution trace, newest run first, and clear it.
        "trace" => {
            link.peek_req.lock().unwrap().push(TRACE_SENTINEL);
            std::thread::sleep(std::time::Duration::from_millis(150));
            let _ = link.peek_ans.lock().unwrap().pop();
            let out = link.out.lock().unwrap();
            if out.pc_trace.is_empty() {
                return "ok trace empty".into();
            }
            // The sequence of distinct addresses matters, not the repetition: a flattened function
            // revisits its dispatcher constantly and a raw list would be mostly that.
            let mut seq: Vec<String> = Vec::new();
            let mut last = None;
            for (pc, _) in out.pc_trace.iter() {
                if Some(*pc) != last {
                    if seq.len() < 400 {
                        seq.push(format!("{pc:x}"));
                    }
                    last = Some(*pc);
                }
            }
            format!(
                "ok trace {} entries, {} transitions: {}",
                out.pc_trace.len(),
                seq.len(),
                seq.join(" ")
            )
        }
        "pmu" => {
            link.peek_req.lock().unwrap().push(PMU_SENTINEL);
            std::thread::sleep(std::time::Duration::from_millis(150));
            let _ = link.peek_ans.lock().unwrap().pop();
            let rows = link.out.lock().unwrap().pmu_written.clone();
            if rows.is_empty() {
                return "ok pmu no writes".into();
            }
            let mut s = String::from("ok pmu");
            for (reg, n, last) in rows {
                s.push_str(&format!(" {reg:#04x}={last:#04x}(x{n})"));
            }
            s
        }
        "writes" => {
            link.peek_req.lock().unwrap().push(WRITES_SENTINEL);
            std::thread::sleep(std::time::Duration::from_millis(200));
            let _ = link.peek_ans.lock().unwrap().pop();
            let rows = link.out.lock().unwrap().watched_writes.clone();
            if rows.is_empty() {
                return "ok writes none".into();
            }
            let mut s = String::from("ok writes");
            for (addr, n, _) in rows {
                s.push_str(&format!(" {addr:#010x}(x{n})"));
            }
            s
        }
        "bus" => {
            link.peek_req.lock().unwrap().push(BUS_SENTINEL);
            std::thread::sleep(std::time::Duration::from_millis(250));
            let _ = link.peek_ans.lock().unwrap().pop();
            let rows = link.out.lock().unwrap().bus_log.clone();
            if rows.is_empty() {
                return "ok bus empty".into();
            }
            let shown = rows.len().min(600);
            let mut s = format!("ok bus {} writes", rows.len());
            if shown < rows.len() {
                s.push_str(" (TRUNCATED)");
            }
            for (pc, addr, val, us) in rows.iter().take(shown) {
                s.push_str(&format!(" {addr:#x}={val:#04x}@{pc:#x}t{us}"));
            }
            s
        }
        // **The wheel census is in here, and it is the reason this verb was worth widening.**
        //
        // `AGENTS.md` §6's first shape is a wheel script that fired 0 of 12 steps and read as *the
        // wheel is not being listened to*. A driver that pushes `wheel 5` and then looks at the
        // panel is exactly that instrument: nothing on the screen distinguishes *the click never
        // arrived* from *it arrived and the firmware ignored it*. `frames_posted` and `position`
        // are the machine's own answer to the first half, and they are cheap — the run loop
        // already publishes them for §12.8's Readout — so the reply carries them rather than
        // leaving every caller to infer input from pixels.
        "state" => {
            let out = link.out.lock().unwrap();
            let s = &out.stats;
            format!(
                "ok phase={:?} executed={} fb={:#010x} nonblack={} seq={} backlight={}/32 up={} \
                 down={} wheel=pos{} touched={} buttons={:#04x} hold={} posted={} dropped={} \
                 refused={}",
                out.phase,
                s.executed,
                out.fb_addr,
                out.fb_nonzero,
                out.fb_seq,
                out.backlight,
                out.backlight_steps.0,
                out.backlight_steps.1,
                s.position,
                s.touched,
                s.buttons,
                s.hold,
                s.frames_posted,
                s.frames_dropped,
                s.input_dropped
            )
        }
        other => format!("error: unknown command {other:?} — `help` lists them"),
    }
}

/// The vocabulary, for a connection that has no copy of this file.
///
/// **Held against the `match` by a test**, in the shape `args.rs` uses for `--help`: a verb the
/// dispatcher answers and this text does not name is a verb nobody can find, and a verb named here
/// that the dispatcher refuses is worse — it reads exactly like a working one.
const HELP: &str = "ok help
help — this
state — phase, instructions, panel, and the wheel as the machine has it
wheel N [MS] — scroll N detents MS apart; negative is anticlockwise
press NAME — select | menu | play | left | right (and prev | next)
hold NAME [MS] — the same buttons, held; default 400 ms
holdsw on|off — the hold SWITCH, which is not a button
snapshot — re-take the restore point here
shot PATH — write the panel as a PNG
peek ADDR — read one word; hex in, hex out
ata FROM TO — was the drive ever asked for these LBAs
unmapped — pages touched that nothing answers for
trace — the PC trace, newest first, and clear it
pmu — the PMU write census
writes — the --watch-writes census
bus — the --watch-writes value log, and clear it
quit — close this connection; the machine keeps running";

#[cfg(test)]
mod tests {
    use super::*;

    /// The two tests that touch [`BENCH`] take this first. It is one process-wide slot, and two
    /// tests writing it at once would produce a failure that belongs to neither.
    static SERIAL: Mutex<()> = Mutex::new(());

    /// Every verb the dispatcher answers, read out of this file.
    ///
    /// **Held against [`HELP`] by the sweep below**, which is `args.rs`'s rule one surface over: a
    /// verb the socket answers and `help` does not name is a verb nobody connecting can find, and
    /// that is the same defect as the socket itself having been unreachable — undiscoverable is
    /// unreachable with extra steps.
    ///
    /// Read by indentation, not by parsing Rust: the arms of `match verb` sit at exactly eight
    /// spaces, and the nested `match arg` inside `holdsw` — whose arms are `"on"`, `"off"` and are
    /// not verbs — sits at sixteen. A scan that ignored the depth would report six words as
    /// vocabulary.
    fn verbs_the_dispatcher_answers() -> Vec<String> {
        let src = include_str!("control.rs");
        let body = src
            .split_once("fn command(")
            .expect("this file defines `command`")
            .1;
        let body = body.split_once("\nconst HELP").expect("HELP follows it").0;
        let mut out: Vec<String> = Vec::new();
        for line in body.lines() {
            // `if verb == "help"` and the empty-bench arm answer before the match, at four spaces.
            let head = if let Some(rest) = line.strip_prefix("    if verb == \"") {
                rest.split('"').next().unwrap_or("").to_string()
            } else if line.starts_with("        \"") {
                let Some((h, _)) = line.split_once("=>") else { continue };
                h.split('|')
                    .filter_map(|p| p.trim().strip_prefix('"'))
                    .filter_map(|p| p.split('"').next())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                continue;
            };
            for v in head.split_whitespace() {
                if !v.is_empty() && !out.iter().any(|s| s == v) {
                    out.push(v.to_string());
                }
            }
        }
        out
    }

    /// **The sweep that makes the vocabulary discoverable**, in both directions.
    ///
    /// Delete an arm from `command` and the first half goes red; add one without a `help` line and
    /// the second does. `quit` is the exception and is named as one: it is answered by [`session`],
    /// which closes the connection rather than replying, so it never reaches the dispatcher.
    #[test]
    fn help_names_every_verb_the_socket_answers_and_no_others() {
        let found = verbs_the_dispatcher_answers();
        assert!(
            found.len() >= 12,
            "the scan read {found:?} out of this file, which is fewer than the dispatcher has — \
             it is reading nothing rather than agreeing"
        );
        let named: Vec<&str> = HELP
            .lines()
            .skip(1)
            .filter_map(|l| l.split_whitespace().next())
            .collect();
        let undocumented: Vec<&String> = found.iter().filter(|v| !named.contains(&v.as_str())).collect();
        assert!(
            undocumented.is_empty(),
            "{undocumented:?} are answered by the socket and `help` does not name them"
        );
        let unanswered: Vec<&&str> = named
            .iter()
            .filter(|v| **v != "quit" && !found.iter().any(|f| f == *v))
            .collect();
        assert!(
            unanswered.is_empty(),
            "`help` names {unanswered:?} and the dispatcher has no arm for them"
        );
    }

    /// **An empty bench is an answer, not an absence.**
    ///
    /// This is the half of the design that made the socket worth opening at launch rather than at
    /// the first machine: whoever connects in order to decide whether to start something gets a
    /// sentence, and every other verb says which one it needed rather than sleeping through a
    /// scroll or timing out after three seconds.
    #[test]
    fn an_empty_bench_answers_rather_than_hanging() {
        let state = command("state", None);
        assert!(state.starts_with("ok bench=empty"), "{state:?}");
        // The verbs that would otherwise wait: `wheel` sleeps 300 ms plus a gap per detent and
        // `peek` waits three seconds for a run loop that is not there. Timed, because "it returned"
        // and "it returned promptly" are different claims and only the second is the point.
        let began = std::time::Instant::now();
        for line in ["wheel 40", "press select", "peek 0x14937194", "ata 0 10", "shot /dev/null"] {
            let r = command(line, None);
            assert!(r.starts_with("error:") && r.contains("bench is empty"), "`{line}` -> {r:?}");
        }
        assert!(
            began.elapsed() < std::time::Duration::from_millis(500),
            "the empty-bench answers took {:?} — something waited for a machine",
            began.elapsed()
        );
    }

    /// `help` answers with no machine, on one line, because the protocol is one reply per line.
    #[test]
    fn help_is_one_line_and_needs_no_machine() {
        let h = command("help", None);
        assert_eq!(h.lines().count(), 1, "{h:?} is more than one reply");
        assert!(h.starts_with("ok help"), "{h:?}");
        assert!(h.contains("wheel N"), "{h:?}");
    }

    #[test]
    fn a_word_the_socket_does_not_know_is_named_and_pointed_somewhere() {
        let r = command("scroll 3", None);
        assert!(r.contains("bench is empty"), "{r:?}");
        let link = Link::new();
        let r = command("scroll 3", Some(&link));
        assert!(r.contains("\"scroll\"") && r.contains("help"), "{r:?}");
    }

    /// **What the bench is for**, in three lines: empty, filled, emptied.
    #[test]
    fn attaching_a_machine_is_what_the_socket_starts_answering_about() {
        let _serial = SERIAL.lock().unwrap();
        detach();
        assert!(command("state", on_the_bench().as_ref()).starts_with("ok bench=empty"));
        let link = Link::new();
        attach(&link);
        let live = command("state", on_the_bench().as_ref());
        assert!(live.starts_with("ok phase="), "{live:?}");
        // The wheel census the widened `state` carries — the numbers that tell "the click never
        // arrived" from "it arrived and the firmware ignored it". A fresh `Link` has them all at
        // rest, which is the control that proves the fields are being read rather than invented.
        assert!(live.contains("wheel=pos0 touched=false"), "{live:?}");
        assert!(live.contains("posted=0 dropped=0 refused=0"), "{live:?}");
        detach();
        assert!(command("state", on_the_bench().as_ref()).starts_with("ok bench=empty"));
    }

    /// **The regression test for the whole of this: a socket that can be reached.**
    ///
    /// `serve` had no caller for its entire life and nothing said so. This binds one, connects to
    /// it the way a shell would, and reads two replies off it — so a future edit that leaves
    /// `serve` uncalled still cannot leave it unreachable.
    #[test]
    fn the_socket_binds_and_answers_a_connection() {
        let _serial = SERIAL.lock().unwrap();
        let path = std::env::temp_dir().join(format!(
            "ipod-control-{}-{:?}.sock",
            std::process::id(),
            std::thread::current().id()
        ));
        serve(&path).unwrap_or_else(|e| panic!("bind {}: {e}", path.display()));
        assert!(listening(&path).contains(&path.display().to_string()));

        let stream = UnixStream::connect(&path).expect("nothing accepted a connection");
        let mut w = stream.try_clone().expect("clone");
        let mut r = BufReader::new(stream);
        let ask = |w: &mut UnixStream, r: &mut BufReader<UnixStream>, line: &str| {
            writeln!(w, "{line}").expect("write");
            let mut got = String::new();
            r.read_line(&mut got).expect("read");
            got.trim_end().to_string()
        };
        let state = ask(&mut w, &mut r, "state");
        assert!(state.starts_with("ok "), "{state:?}");
        let help = ask(&mut w, &mut r, "help");
        assert!(help.starts_with("ok help") && help.contains("peek ADDR"), "{help:?}");
        // A blank line is not a command and must not consume a reply, or every driver that echoes
        // an empty line reads the answer to its next question as the answer to this one.
        writeln!(w).expect("write");
        let after = ask(&mut w, &mut r, "state");
        assert!(after.starts_with("ok "), "{after:?}");

        let _ = std::fs::remove_file(&path);
    }
}
