//! A resource-free microbenchmark of `Memory::read32`, by the path each address takes.
//!
//!   cargo run --release -p ipod-machine --example read32-bench [-- ROUNDS]
//!
//! Builds the warm machine `map_hardware` describes — no firmware, no disk image, nothing from
//! `resources/` — and times word reads of three kinds of address:
//!
//! - **page**: a word on a page the page cache serves (SDRAM, IRAM). The fast path, and the
//!   control: a change to the device-page path must not move it.
//! - **word**: a word on a *device* page that nothing on the byte path claims — `TIMER1_CFG`
//!   beside the free-running `USEC_TIMER`, the GPIO input ports, `CPU_CTRL`, the chip id. These are
//!   the reads the word path exists for.
//! - **byte**: a word a device answers (`USEC_TIMER`, `TIMER1_VAL`, `PLL_STATUS`, a DMA status).
//!   These must still take four `read8` calls, so their cost should not move either.
//!
//! Prints nanoseconds per `read32` for each group. Timing is wall-clock on a shared host, so read
//! it as a ratio between runs of the same build pair, never as an absolute.

use std::hint::black_box;
use std::time::Instant;

use arm7tdmi::Bus;
use ipod_machine::{map_hardware, EApp, Machine};

const GROUPS: [(&str, &[u32]); 3] = [
    ("page", &[0x0000_1000, 0x0010_0000, 0x1020_0000, 0x4000_0100]),
    (
        "word",
        &[0x6000_5000, 0x6000_5008, 0x6000_d030, 0x6000_d034, 0x6000_7000, 0x7000_0000],
    ),
    ("byte", &[0x6000_5010, 0x6000_5004, 0x6000_603c, 0x6000_b004]),
];

fn main() {
    let rounds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_000_000);
    let mut m = Machine::new(&EApp::none(), 0x1100_0000, 0x0100_0000);
    map_hardware(&mut m, false);
    println!("read32-bench: {rounds} rounds per address");
    for (name, addrs) in GROUPS {
        // Warm the page cache so the first round is not a resolution.
        for &a in addrs {
            black_box(m.mem.read32(a));
        }
        let t = Instant::now();
        let mut acc = 0u32;
        for _ in 0..rounds {
            for &a in addrs {
                acc = acc.wrapping_add(m.mem.read32(black_box(a)));
            }
        }
        black_box(acc);
        let ns = t.elapsed().as_nanos() as f64 / (rounds * addrs.len() as u64) as f64;
        println!("{name:<5} {ns:>8.2} ns/read32  ({} addresses)", addrs.len());
    }
}
