//! Run one position-independent ARM snippet on the emulated machine and report what it did.
//!
//!   snippet FILE.bin [--iterations=N] [--io=W0,W1,…] [--clock=N] [--budget=N]
//!
//! This is the emulator half of a hardware/emulator differential. The other half is a probe on a
//! real iPod that loads the **same bytes**, calls them the same way, and prints the same report —
//! so the two outputs can be compared line by line, and every line that differs is either a model
//! defect or a measurement of how the emulator's timing departs from the part's.
//!
//! # The contract a snippet is written against
//!
//! - ARM state, **position-independent**: it is loaded at a different address on each side, so it
//!   may address hardware absolutely but must reach its own code and data PC-relatively.
//! - Called as `void snippet(u32 io[16])` under AAPCS: `r0` points at sixteen words, `lr` is the
//!   return address, `sp` is a valid full-descending stack. It returns with `bx lr` (or `mov pc,
//!   lr`, or a pop of `pc`).
//! - Supervisor mode with IRQ and FIQ masked. The probe masks interrupts around the call for the
//!   same reason: a timed snippet that can be pre-empted measures the pre-emption.
//! - Called `N` times in a row against **the same** `io` block, so a snippet that accumulates into
//!   it sees its own previous results. The block is initialised once, from `--io=`, before the
//!   first call.
//!
//! # The report
//!
//! `key=value` lines, one fact per line, in a fixed order, so a line-by-line diff of the device's
//! report and this one is the comparison. The keys only one side can produce — `instructions` here,
//! `cpu_hz` there — are listed after the shared ones so they never line up against something else.
//!
//! **Timing is reported and not asserted.** `elapsed_usec` here is the emulator's simulated clock,
//! which runs at `--clock` instructions per microsecond and knows nothing about pipeline stalls or
//! memory wait states. The difference between it and the device's `USEC_TIMER` is the point of
//! measuring, not a failure.

use std::process::ExitCode;

use ipod_machine::{map_hardware, EApp, Machine, Stop};

/// Where the snippet is loaded. Inside the low SDRAM alias `map_hardware` sets up for a warm
/// machine — the window Rockbox itself runs from on the device — and far from both the vectors and
/// the `io` block.
const LOAD: u32 = 0x0010_0000;
/// The sixteen `io` words.
const IO: u32 = 0x0020_0000;
/// Top of a full-descending stack, with room below it.
const STACK: u32 = 0x0030_0000;
/// Nothing is mapped here, so a snippet that returns anywhere but through `lr` is `Lost`, not
/// silently mistaken for a return.
const EXIT: u32 = 0xDEAD_0000;
/// Largest snippet accepted. The device's buffer is the same size, so a snippet that fits one fits
/// the other.
pub const MAX_SNIPPET: usize = 4096;

fn parse_u(s: &str) -> Option<u64> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: snippet FILE.bin [--iterations=N] [--io=W0,W1,…] [--clock=N] [--budget=N]");
        return ExitCode::from(2);
    };
    let flag = |k: &str| args.iter().find_map(|a| a.strip_prefix(k));
    let iterations = flag("--iterations=").and_then(parse_u).unwrap_or(1).max(1);
    let clock = flag("--clock=").and_then(parse_u).unwrap_or(75) as usize;
    let budget = flag("--budget=").and_then(parse_u).unwrap_or(100_000_000) as usize;

    let code = match std::fs::read(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };
    if code.is_empty() || code.len() > MAX_SNIPPET || code.len() % 4 != 0 {
        eprintln!(
            "{path}: {} bytes; a snippet is 4..={MAX_SNIPPET} bytes of whole ARM words",
            code.len()
        );
        return ExitCode::from(2);
    }

    let mut io = [0u32; 16];
    if let Some(list) = flag("--io=") {
        for (i, w) in list.split(',').enumerate().take(16) {
            match parse_u(w) {
                Some(v) => io[i] = v as u32,
                None => {
                    eprintln!("--io: word {i} ({w:?}) is not a number");
                    return ExitCode::from(2);
                }
            }
        }
    }

    let mut m = Machine::new(&EApp::none(), 0x1100_0000, 0x0100_0000);
    map_hardware(&mut m, false);
    m.set_clock(clock);
    for (i, b) in code.iter().enumerate() {
        m.mem.poke8(LOAD + i as u32, *b);
    }
    for (i, w) in io.iter().enumerate() {
        m.mem.poke32(IO + 4 * i as u32, *w);
    }
    m.set_exit(EXIT);

    let (usec0, exec0) = (m.mem.usec, m.executed);
    for n in 0..iterations {
        m.cpu.regs[0] = IO;
        m.cpu.regs[13] = STACK;
        m.cpu.regs[14] = EXIT;
        m.cpu.regs[15] = LOAD;
        match m.run(budget) {
            Stop::Returned => {}
            other => {
                println!("status=fault");
                println!("fault={other:?}");
                println!("fault_iteration={n}");
                println!("pc={:#010x}", m.cpu.regs[15]);
                return ExitCode::from(1);
            }
        }
    }

    println!("status=ok");
    println!("iterations={iterations}");
    for i in 0..16 {
        println!("io[{i}]={:#010x}", m.mem.peek32(IO + 4 * i as u32).unwrap_or(0));
    }
    println!("elapsed_usec={}", m.mem.usec.wrapping_sub(usec0));
    println!("instructions={}", m.executed - exec0);
    println!("clock_ipu={clock}");
    ExitCode::SUCCESS
}
