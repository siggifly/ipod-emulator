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
//! wheel N [MS]       scroll N detents, MS apart; negative is anticlockwise
//! press NAME         select | menu | play | left/prev | right/next
//! hold NAME MS       hold a button, for the combos that need one
//! holdsw on|off     the HOLD SWITCH, which is not a button and not `hold`
//! snapshot          re-take the idle snapshot here, so this is where launches resume
//! shot PATH          write the current framebuffer as a PNG
//! peek ADDR          read one word, hex in and hex out; unmapped says so
//! ata FROM TO        whether the drive was ever asked for these sectors
//! state              phase, instruction count, framebuffer address and non-black pixels
//! quit               close this connection (the emulator keeps running)
//! ```
//!
//! Read-only where it can be: `peek` goes through `Memory::peek32`, which walks the regions
//! directly rather than through the access counters, so observing costs nothing and changes no
//! number any report has produced.

use crate::emu::{self, Link};
use eapp_loader::{wheel_button, WheelEvent};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;

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

/// Start listening. Returns immediately; each connection is served on its own thread.
pub fn serve(path: &Path, link: Arc<Link>) -> Result<(), String> {
    // A socket left by a previous run would make `bind` fail with EADDRINUSE, and the previous run
    // is gone -- the file is not a lock, it is litter.
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let p = path.to_path_buf();
    std::thread::spawn(move || {
        eprintln!("control socket: {}", p.display());
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let link = Arc::clone(&link);
            std::thread::spawn(move || {
                if let Err(e) = session(stream, &link) {
                    eprintln!("control: {e}");
                }
            });
        }
    });
    Ok(())
}

fn session(stream: UnixStream, link: &Arc<Link>) -> Result<(), String> {
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
        let reply = command(line, link);
        writeln!(out, "{reply}").map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn command(line: &str, link: &Arc<Link>) -> String {
    let mut it = line.split_whitespace();
    let verb = it.next().unwrap_or("");
    let arg = it.next().unwrap_or("");
    let arg2 = it.next().unwrap_or("");

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
            let png = eapp_loader::png::encode(&out.fb, emu::FB_W, emu::FB_H);
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
        "state" => {
            let out = link.out.lock().unwrap();
            format!(
                "ok phase={:?} executed={} fb={:#010x} nonblack={} seq={} backlight={}/32 up={} down={}",
                out.phase,
                out.stats.executed,
                out.fb_addr,
                out.fb_nonzero,
                out.fb_seq,
                out.backlight,
                out.backlight_steps.0,
                out.backlight_steps.1
            )
        }
        other => format!("error: unknown command {other:?}"),
    }
}
