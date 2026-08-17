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
//! wheel N            scroll N detents; negative is anticlockwise
//! press NAME         select | menu | play | left/prev | right/next
//! hold NAME MS       hold a button, for the combos that need one
//! shot PATH          write the current framebuffer as a PNG
//! peek ADDR          read one word, hex in and hex out; unmapped says so
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
                // A real finger touches the wheel, moves, and lifts. Posting bare steps leaves the
                // driver believing a finger is still down, and the next press behaves oddly.
                link.push(WheelEvent::Touch);
                for _ in 0..n.abs() {
                    link.push(WheelEvent::Step(if n > 0 { 1 } else { -1 }));
                }
                link.push(WheelEvent::Release);
                format!("ok wheel {n}")
            }
            Err(_) => "error: wheel wants a number of detents".into(),
        },
        "press" | "hold" => {
            let Some(mask) = wheel_button(arg) else {
                return format!("error: unknown button {arg:?} (select menu play left right)");
            };
            let ms: u64 = if verb == "hold" { arg2.parse().unwrap_or(400) } else { 0 };
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
                Ok(()) => format!("ok shot {arg} ({} bytes, {} non-black)", png.len(), out.fb_nonzero),
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
        "state" => {
            let out = link.out.lock().unwrap();
            format!(
                "ok phase={:?} executed={} fb={:#010x} nonblack={} seq={}",
                out.phase, out.stats.executed, out.fb_addr, out.fb_nonzero, out.fb_seq
            )
        }
        other => format!("error: unknown command {other:?}"),
    }
}
