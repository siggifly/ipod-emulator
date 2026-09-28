//! **The hardware conformance gate.** The emulator's peripheral registers, compared word by word with
//! a capture read off a real iPod.
//!
//! `fixtures/hw-registers-5.5g-rockbox.txt` is peripheral register state read off a retail 5.5G
//! (PP5022C) running Rockbox, at its menu, through the lab serial console — three passes seconds
//! apart, so a word that moved is marked `live` and never compared. `fixtures/hw-divergences.txt`
//! is every word where this emulator currently disagrees with it.
//!
//! The gate fails in **both** directions:
//!
//! - a word that disagrees and is **not** in the divergence list is a new departure from the
//!   hardware — a regression, or a change that needs its entry written and justified;
//! - a word **in** the list that now agrees is a divergence retired, and the list must shrink with
//!   it. A list that is allowed to keep stale entries stops being a count of what is wrong.
//!
//! So `cargo test` states, as a number, how far the model is from the part, and that number can
//! only move on purpose. The emulator side runs the same firmware to the same point (Rockbox, its
//! menu, 30 simulated seconds) and reads the same words through the firmware's own bus path
//! (`trace --dump-words`), so what is compared is what Rockbox would see on each machine.
//!
//! Skips, and says so, when the resources the Rockbox run needs are absent — the fixture itself is
//! checked in and always present.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const FIXTURE: &str = include_str!("fixtures/hw-registers-5.5g-rockbox.txt");
const DIVERGENCES: &str = include_str!("fixtures/hw-divergences.txt");

fn resources() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources")
}

/// `word ADDR VALUE` and `live ADDR` lines; everything else is commentary.
fn parse_fixture(text: &str) -> (BTreeMap<u32, u32>, BTreeSet<u32>) {
    let (mut words, mut live) = (BTreeMap::new(), BTreeSet::new());
    for l in text.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        match f.as_slice() {
            ["word", a, v] => {
                words.insert(
                    u32::from_str_radix(a, 16).expect("fixture address"),
                    u32::from_str_radix(v, 16).expect("fixture value"),
                );
            }
            ["live", a] => {
                live.insert(u32::from_str_radix(a, 16).expect("fixture address"));
            }
            _ => {}
        }
    }
    (words, live)
}

/// `ADDR` at the start of each non-comment line.
fn parse_divergences(text: &str) -> BTreeSet<u32> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let a = l.split_whitespace().next().unwrap();
            u32::from_str_radix(a, 16).unwrap_or_else(|_| panic!("divergence line {l:?}"))
        })
        .collect()
}

/// The windows the fixture covers, as contiguous `(base, len)` runs over its addresses.
fn windows(addrs: impl Iterator<Item = u32>) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for a in addrs {
        match out.last_mut() {
            Some((b, l)) if *b + *l == a => *l += 4,
            _ => out.push((a, 4)),
        }
    }
    out
}

#[test]
fn the_fixture_is_well_formed() {
    let (words, live) = parse_fixture(FIXTURE);
    assert!(words.len() > 1000, "fixture has {} words; a truncated capture is not a fixture", words.len());
    assert!(words.keys().all(|a| a % 4 == 0), "unaligned address in the fixture");
    assert!(words.keys().all(|a| !live.contains(a)), "an address is both stable and live");
    // Nothing from the apertures the capture script excludes on purpose.
    for (lo, hi, what) in [
        (0xc300_0000u32, 0xc3ff_ffffu32, "IDE"),
        (0xc500_0000, 0xc5ff_ffff, "USB"),
        (0xe000_0000, 0xffff_ffff, "cache / mask ROM aliases"),
    ] {
        assert!(
            !words.keys().chain(live.iter()).any(|a| (lo..=hi).contains(a)),
            "the fixture carries {what} words, which the capture excludes"
        );
    }
    let div = parse_divergences(DIVERGENCES);
    let stray: Vec<_> = div.iter().filter(|a| !words.contains_key(a)).collect();
    assert!(stray.is_empty(), "divergences name words the fixture does not hold: {stray:x?}");
}

#[test]
fn the_emulator_agrees_with_the_hardware_except_where_it_says_it_does_not() {
    let res = resources();
    let rb = res.join("vendor/rockbox/bin/rb-main.raw");
    let flash = res.join("roms/retail_5g_MA146_HwVr000B0005_internal_rom_000000-0FFFFF.bin");
    let drive = res.join("drives/ipod8g.img");
    if !(rb.exists() && flash.exists() && drive.exists()) {
        eprintln!("SKIPPED: the Rockbox run needs resources/ (rb-main.raw, the retail NOR, ipod8g.img)");
        return;
    }
    let (words, _live) = parse_fixture(FIXTURE);
    let expected_div = parse_divergences(DIVERGENCES);

    // A private copy of the drive: the run writes to it.
    let scratch = std::env::temp_dir().join(format!("conformance-{}.img", std::process::id()));
    std::fs::copy(&drive, &scratch).expect("copy the drive");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_trace"));
    cmd.arg("20000000000")
        .arg(format!("--osos={}", rb.display()))
        .arg("--boot-osos")
        .arg("--sysinfo")
        .arg(format!("--flash={}", flash.display()))
        .arg(format!("--disk={}", scratch.display()))
        .args(["--disk-writable", "--bcm", "--pmu", "--clock=75", "--until=30s"]);
    for (b, l) in windows(words.keys().copied()) {
        cmd.arg(format!("--dump-words={b:#x}:{l:#x}"));
    }
    let out = cmd.output().expect("run trace");
    let _ = std::fs::remove_file(&scratch);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let (emu, _) = parse_fixture(&stdout);
    assert_eq!(
        emu.len(),
        words.len(),
        "trace returned {} words for {} asked — the run did not reach its dump",
        emu.len(),
        words.len()
    );

    let actual_div: BTreeSet<u32> =
        words.iter().filter(|(a, v)| emu.get(a) != Some(v)).map(|(a, _)| *a).collect();

    // `CONFORMANCE_BLESS=1` rewrites the divergence list from this run — and then FAILS, so a
    // blessed list is never also the run that passed. Bless, read the diff, commit, rerun.
    if std::env::var_os("CONFORMANCE_BLESS").is_some() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hw-divergences.txt");
        let mut body = String::from(
            "# Every word where the emulator disagrees with fixtures/hw-registers-5.5g-rockbox.txt.\n\
             # ADDR  hardware  emulator. Generated by CONFORMANCE_BLESS=1; each entry is a known\n\
             # departure from the part, and removing one is the only way the count goes down.\n",
        );
        for a in &actual_div {
            body.push_str(&format!("{a:08x}  {:08x}  {:08x}\n", words[a], emu[a]));
        }
        std::fs::write(&path, body).expect("write the divergence list");
        panic!("blessed {} divergences into {}; rerun without CONFORMANCE_BLESS", actual_div.len(), path.display());
    }
    let new: Vec<String> = actual_div
        .difference(&expected_div)
        .map(|a| format!("{a:08x} hw {:08x} emu {:08x}", words[a], emu[a]))
        .collect();
    let retired: Vec<String> = expected_div
        .difference(&actual_div)
        .map(|a| format!("{a:08x} {:08x}", words[a]))
        .collect();
    eprintln!(
        "conformance: {} of {} words agree, {} diverge ({} listed)",
        words.len() - actual_div.len(),
        words.len(),
        actual_div.len(),
        expected_div.len()
    );
    assert!(
        new.is_empty() && retired.is_empty(),
        "\n{} NEW divergence(s) — fix the model or list them in fixtures/hw-divergences.txt:\n  {}\n\
         {} divergence(s) RETIRED — delete them from the list:\n  {}\n",
        new.len(),
        new.iter().take(40).cloned().collect::<Vec<_>>().join("\n  "),
        retired.len(),
        retired.iter().take(40).cloned().collect::<Vec<_>>().join("\n  ")
    );
}
