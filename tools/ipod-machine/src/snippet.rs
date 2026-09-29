//! The emulator half of the hardware/emulator differential, and the comparison that joins the two.
//!
//! A **snippet** is a few hundred bytes of position-independent ARM that a real iPod and this
//! emulator both run. Each side prints the same `key=value` report; [`compare`] lines them up and
//! says which facts agree, which differ, and which are timing — reported, never asserted.
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
//!   it sees its own previous results. The block is initialised once, before the first call.
//!
//! # The report
//!
//! `key=value` lines, one fact per line, in a fixed order, so a line-by-line diff of the device's
//! report and this one is the comparison. Keys only one side can produce — [`EMULATOR_ONLY`] here,
//! [`DEVICE_ONLY`] there — come after the shared ones so they never line up against something else.
//!
//! **Timing is reported and not asserted.** `elapsed_usec` here is the emulator's simulated clock,
//! which runs at `clock` instructions per microsecond and knows nothing about pipeline stalls or
//! memory wait states. The difference between it and the device's `USEC_TIMER` is the point of
//! measuring, not a failure — and so is any `io` word a snippet fills from the timer, which the
//! caller names as timing.
//!
//! # Getting a snippet in
//!
//! Either the raw assembled bytes (`.bin`), or a `.words` text file: one 32-bit word per
//! whitespace-separated token, hex, `@`/`#`/`;` comments. The text form exists because this
//! repository keeps no binaries outside `docs/media/` (the pre-commit hook enforces it), and a
//! snippet the tests run has to be checked in.

use std::collections::BTreeSet;

use arm7tdmi::Mode;

use crate::{map_hardware, EApp, Machine, Stop};

/// Where the snippet is loaded. Inside the low SDRAM alias `map_hardware` sets up for a warm
/// machine — the window Rockbox itself runs from on the device — and far from both the vectors and
/// the `io` block.
pub const LOAD: u32 = 0x0010_0000;
/// The sixteen `io` words.
pub const IO: u32 = 0x0020_0000;
/// Top of a full-descending stack, with room below it.
pub const STACK: u32 = 0x0030_0000;
/// Nothing is mapped here, so a snippet that returns anywhere but through `lr` is `Lost`, not
/// silently mistaken for a return.
pub const EXIT: u32 = 0xDEAD_0000;
/// Largest snippet accepted. The device's buffer is the same size, so a snippet that fits one fits
/// the other.
pub const MAX_SNIPPET: usize = 4096;

/// Keys only this side can produce: the instruction count and the clock it was converted at.
pub const EMULATOR_ONLY: [&str; 2] = ["instructions", "clock_ipu"];
/// Keys only the device can produce.
pub const DEVICE_ONLY: [&str; 1] = ["cpu_hz"];
/// Keys that are timing on every snippet.
pub const ALWAYS_TIMING: [&str; 1] = ["elapsed_usec"];

/// How a snippet is run.
#[derive(Debug, Clone)]
pub struct Options {
    /// Calls, back to back, against the same `io` block. At least one.
    pub iterations: u64,
    /// Instructions per simulated microsecond.
    pub clock: usize,
    /// Instruction budget **per call**; a call that exhausts it is a fault.
    pub budget: usize,
    /// The `io` block's initial contents.
    pub io: [u32; 16],
}

impl Default for Options {
    fn default() -> Self {
        Options {
            iterations: 1,
            clock: 75,
            budget: 100_000_000,
            io: [0; 16],
        }
    }
}

/// `0x`-prefixed hex or decimal.
pub fn parse_u(s: &str) -> Option<u64> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// A `.words` file as the bytes it describes, little-endian. Every token is a hex word, with or
/// without `0x`; a token that is not one is an error rather than a silently shorter snippet.
pub fn parse_words(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let code = line.split(['@', '#', ';']).next().unwrap_or("");
        for tok in code.split_whitespace() {
            let h = tok.strip_prefix("0x").unwrap_or(tok);
            let w = u32::from_str_radix(h, 16)
                .map_err(|_| format!("line {}: {tok:?} is not a hex word", n + 1))?;
            out.extend_from_slice(&w.to_le_bytes());
        }
    }
    Ok(out)
}

/// Read a snippet from `path`: `.words` text, anything else raw bytes. Checked against the size
/// the device accepts, so a snippet refused there is refused here first.
pub fn load(path: &std::path::Path) -> Result<Vec<u8>, String> {
    let name = path.display();
    let code = if path.extension().is_some_and(|e| e == "words") {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{name}: {e}"))?;
        parse_words(&text).map_err(|e| format!("{name}: {e}"))?
    } else {
        std::fs::read(path).map_err(|e| format!("{name}: {e}"))?
    };
    if code.is_empty() || code.len() > MAX_SNIPPET || code.len() % 4 != 0 {
        return Err(format!(
            "{name}: {} bytes; a snippet is 4..={MAX_SNIPPET} bytes of whole ARM words",
            code.len()
        ));
    }
    Ok(code)
}

/// One report: ordered `key=value` facts.
pub type Report = Vec<(String, String)>;

fn fact(r: &mut Report, k: &str, v: impl std::fmt::Display) {
    r.push((k.to_string(), v.to_string()));
}

/// Run `code` on a warm `map_hardware` machine and report what it did.
pub fn run(code: &[u8], o: &Options) -> Report {
    let mut m = Machine::new(&EApp::none(), 0x1100_0000, 0x0100_0000);
    map_hardware(&mut m, false);
    m.set_clock(o.clock);
    for (i, b) in code.iter().enumerate() {
        m.mem.poke8(LOAD + i as u32, *b);
    }
    for (i, w) in o.io.iter().enumerate() {
        m.mem.poke32(IO + 4 * i as u32, *w);
    }
    m.set_exit(EXIT);
    // `Machine::new` leaves the core in System mode, which is where an eApp runs. The contract
    // says Supervisor with both interrupt lines masked, because that is how the probe calls it.
    m.cpu.set_mode(Mode::Supervisor);
    m.cpu.cpsr.set_irq_disabled(true);
    m.cpu.cpsr.set_fiq_disabled(true);

    let mut r = Report::new();
    let iterations = o.iterations.max(1);
    let (usec0, exec0) = (m.mem.usec, m.executed);
    for n in 0..iterations {
        m.cpu.cpsr.set_thumb(false);
        m.cpu.regs[0] = IO;
        m.cpu.regs[13] = STACK;
        m.cpu.regs[14] = EXIT;
        m.cpu.regs[15] = LOAD;
        match m.run(o.budget) {
            Stop::Returned => {}
            other => {
                fact(&mut r, "status", "fault");
                fact(&mut r, "fault", format!("{other:?}"));
                fact(&mut r, "fault_iteration", n);
                fact(&mut r, "pc", format!("{:#010x}", m.cpu.regs[15]));
                return r;
            }
        }
    }
    fact(&mut r, "status", "ok");
    fact(&mut r, "iterations", iterations);
    for i in 0..16 {
        let w = m.mem.peek32(IO + 4 * i as u32).unwrap_or(0);
        fact(&mut r, &format!("io[{i}]"), format!("{w:#010x}"));
    }
    fact(&mut r, "elapsed_usec", m.mem.usec.wrapping_sub(usec0));
    fact(&mut r, "instructions", m.executed - exec0);
    fact(&mut r, "clock_ipu", o.clock);
    r
}

/// The report as it is printed: one `key=value` per line.
pub fn render(r: &Report) -> String {
    r.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
}

/// Every `key=value` line of a captured report. Anything else — a console prompt, the command
/// echoed back, a blank line — is not a fact and is skipped.
pub fn parse_report(text: &str) -> Report {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.trim().split_once('=')?;
            let k = k.trim();
            let ok = !k.is_empty()
                && k.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_[]".contains(c));
            ok.then(|| (k.to_string(), v.trim().to_string()))
        })
        .collect()
}

/// Two values that should be the same fact. Numbers compare by value, so `0x0000002a` on one
/// side and `42` on the other agree; anything else compares as text.
fn same(a: &str, b: &str) -> bool {
    match (parse_u(a), parse_u(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// How one report stands against the other.
#[derive(Debug, Default, PartialEq)]
pub struct Comparison {
    /// Shared facts that agree.
    pub agree: Vec<String>,
    /// Shared facts that differ: `(key, emulator, device)`. Any of these is a disagreement.
    pub differ: Vec<(String, String, String)>,
    /// Timing, both values, never a disagreement.
    pub timing: Vec<(String, String, String)>,
    /// Facts the emulator reported and the device should have but did not. A disagreement: a
    /// capture that stops short must not read as a match.
    pub missing: Vec<String>,
    /// Facts only one side can produce, `(key, value)`, listed for the record.
    pub emulator_only: Vec<(String, String)>,
    pub device_only: Vec<(String, String)>,
}

impl Comparison {
    /// No shared fact differs and none is missing. Timing never counts against it.
    pub fn agrees(&self) -> bool {
        self.differ.is_empty() && self.missing.is_empty()
    }

    /// The comparison as a reader wants it: disagreements first, then timing, then agreement,
    /// and a last line that states the verdict with its counts.
    pub fn render(&self) -> String {
        let mut s = String::new();
        for (k, e, d) in &self.differ {
            s += &format!("DIFFER   {k}: emulator {e}, device {d}\n");
        }
        for k in &self.missing {
            s += &format!("MISSING  {k}: the device report has no such line\n");
        }
        for (k, e, d) in &self.timing {
            let ratio = match (parse_u(e), parse_u(d)) {
                (Some(x), Some(y)) if x != 0 => {
                    format!("  (device/emulator {:.3})", y as f64 / x as f64)
                }
                _ => String::new(),
            };
            s += &format!("timing   {k}: emulator {e}, device {d}{ratio}\n");
        }
        for k in &self.agree {
            s += &format!("same     {k}\n");
        }
        for (k, v) in &self.emulator_only {
            s += &format!("emulator {k}={v}\n");
        }
        for (k, v) in &self.device_only {
            s += &format!("device   {k}={v}\n");
        }
        s += &format!(
            "{}: {} same, {} differ, {} missing, {} timing\n",
            if self.agrees() { "AGREE" } else { "DISAGREE" },
            self.agree.len(),
            self.differ.len(),
            self.missing.len(),
            self.timing.len()
        );
        s
    }
}

/// Line the emulator's report up against the device's. `timing` names the extra keys — usually
/// `io[n]` words a snippet filled from the timer — that are reported and not compared.
pub fn compare(emu: &Report, dev: &Report, timing: &BTreeSet<String>) -> Comparison {
    let find = |r: &Report, k: &str| r.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
    let is_timing = |k: &str| ALWAYS_TIMING.contains(&k) || timing.contains(k);
    let mut c = Comparison::default();
    for (k, e) in emu {
        if EMULATOR_ONLY.contains(&k.as_str()) {
            c.emulator_only.push((k.clone(), e.clone()));
            continue;
        }
        match find(dev, k) {
            None => c.missing.push(k.clone()),
            Some(d) if is_timing(k) => c.timing.push((k.clone(), e.clone(), d)),
            Some(d) if same(e, &d) => c.agree.push(k.clone()),
            Some(d) => c.differ.push((k.clone(), e.clone(), d)),
        }
    }
    for (k, d) in dev {
        if find(emu, k).is_some() {
            continue;
        }
        // Listed, never counted: `cpu_hz` is expected here, and a device that faulted reports
        // `fault` lines a clean run does not — a disagreement `status` already carries.
        c.device_only.push((k.clone(), d.clone()));
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `snippets/timer-loop.words`, the checked-in form of `timer-loop.s`.
    const TIMER_LOOP: &str = include_str!("../snippets/timer-loop.words");

    fn timer_loop() -> Vec<u8> {
        parse_words(TIMER_LOOP).expect("timer-loop.words parses")
    }

    fn get<'a>(r: &'a Report, k: &str) -> &'a str {
        &r.iter()
            .find(|(x, _)| x == k)
            .unwrap_or_else(|| panic!("no {k} in {r:?}"))
            .1
    }

    fn word(r: &Report, k: &str) -> u64 {
        parse_u(get(r, k)).unwrap()
    }

    /// The hand-assembled words are the program `timer-loop.s` says they are: each instruction
    /// word decodes, with the interpreter's own disassembler, to the mnemonic on the matching
    /// source line, and each `.word` is the literal the source gives. Edit one file without the
    /// other and this fails, rather than the report turning puzzling. What it cannot see — an
    /// operand wrong under the right mnemonic — `the_timer_loop_runs_and_reports` catches by
    /// running it.
    #[test]
    fn the_checked_in_words_are_the_assembly() {
        let source = include_str!("../snippets/timer-loop.s");
        let lines: Vec<(String, Option<u32>)> = source
            .lines()
            .filter_map(|l| {
                let l = l.split('@').next().unwrap().trim();
                // Drop a leading label, `snippet:` or `1:`.
                let l = l.split_once(':').map_or(l, |(_, rest)| rest).trim();
                let mut t = l.split_whitespace();
                let op = t.next()?;
                if op == ".word" {
                    return Some((op.to_string(), t.next().and_then(parse_u).map(|v| v as u32)));
                }
                (!op.starts_with('.')).then(|| (op.to_string(), None))
            })
            .collect();
        let code = timer_loop();
        assert_eq!(code.len(), lines.len() * 4, "one word per source line");
        for (i, (w, (op, literal))) in code.chunks(4).zip(&lines).enumerate() {
            let w = u32::from_le_bytes(w.try_into().unwrap());
            if op == ".word" {
                assert_eq!(Some(w), *literal, "word {i}: .word");
                continue;
            }
            let text = arm7tdmi::disasm::arm(w, LOAD + 4 * i as u32, None);
            assert_eq!(
                text.split_whitespace().next(),
                Some(op.as_str()),
                "word {i}: decoded {text:?}"
            );
        }
        // `bne 1b` lands on the `subs`, the loop's first instruction.
        let bne = u32::from_le_bytes(code[0x28..0x2c].try_into().unwrap());
        let text = arm7tdmi::disasm::arm(bne, LOAD + 0x28, None);
        assert!(text.contains(&format!("{:#010x}", LOAD + 0x24)), "{text}");
    }

    /// The first snippet, run the way the probe runs it. Every value here is one the device can be
    /// held to except the timer words, which is why the caller names them as timing.
    #[test]
    fn the_timer_loop_runs_and_reports() {
        let r = run(&timer_loop(), &Options::default());
        assert_eq!(get(&r, "status"), "ok", "{r:?}");
        assert_eq!(
            word(&r, "io[0]"),
            0x5555_5555,
            "PROC_ID read as a word, CPU"
        );
        assert_eq!(
            word(&r, "io[4]"),
            10_000,
            "the default iteration count was written back"
        );
        assert_eq!(word(&r, "io[5]"), 1, "one call");
        let (t0, t1, dt) = (word(&r, "io[1]"), word(&r, "io[2]"), word(&r, "io[3]"));
        assert_eq!(t1.wrapping_sub(t0) & 0xffff_ffff, dt);
        // Twenty thousand loop instructions at 75 per microsecond: the timer moved, by about that.
        assert!((200..400).contains(&dt), "loop took {dt} us");
        assert!(word(&r, "instructions") > 20_000);
        // The same fixed order the device prints in, so a line diff is the comparison.
        let keys: Vec<&str> = r.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys[..2], ["status", "iterations"]);
        assert_eq!(
            keys[keys.len() - 3..],
            ["elapsed_usec", "instructions", "clock_ipu"]
        );
    }

    /// `--iterations` calls against one `io` block, so a snippet sees its own previous results.
    #[test]
    fn iterations_share_one_io_block() {
        let o = Options {
            iterations: 3,
            io: {
                let mut io = [0; 16];
                io[4] = 100;
                io
            },
            ..Default::default()
        };
        let r = run(&timer_loop(), &o);
        assert_eq!(get(&r, "status"), "ok");
        assert_eq!(word(&r, "io[5]"), 3);
        assert_eq!(
            word(&r, "io[4]"),
            100,
            "an input the caller gave is not overwritten"
        );
        assert_eq!(get(&r, "iterations"), "3");
    }

    /// A snippet that never returns is a fault with the iteration it died in, not a hang and not a
    /// report of zeros. `b .` spins until the budget runs out; a jump to nowhere is `Lost`.
    #[test]
    fn a_snippet_that_does_not_return_is_a_fault() {
        let spin = parse_words("eafffffe").unwrap();
        let r = run(
            &spin,
            &Options {
                budget: 1000,
                ..Default::default()
            },
        );
        assert_eq!(get(&r, "status"), "fault");
        assert_eq!(get(&r, "fault"), "BudgetExhausted");
        assert_eq!(get(&r, "fault_iteration"), "0");
        // mov pc, #0x50000000 — unmapped.
        let lost = parse_words("e3a0f205").unwrap();
        let r = run(&lost, &Options::default());
        assert_eq!(get(&r, "status"), "fault");
        assert!(get(&r, "fault").starts_with("Lost"), "{r:?}");
    }

    #[test]
    fn words_files_parse_comments_and_refuse_junk() {
        assert_eq!(
            parse_words("@ c\n0x01 2 # x\n; y\n").unwrap(),
            [1, 0, 0, 0, 2, 0, 0, 0]
        );
        assert!(parse_words("e5912000 ldr").unwrap_err().contains("\"ldr\""));
    }

    /// A report round-trips through its printed form, and console noise around it is ignored.
    #[test]
    fn a_captured_report_parses_back() {
        let r = run(&timer_loop(), &Options::default());
        let text = format!("> X timer-loop\n{}lab> \n", render(&r));
        assert_eq!(parse_report(&text), r);
    }

    fn device_from(emu: &Report, edit: &[(&str, &str)]) -> Report {
        let mut d: Report = emu
            .iter()
            .filter(|(k, _)| !EMULATOR_ONLY.contains(&k.as_str()))
            .cloned()
            .collect();
        for &(k, v) in edit {
            match d.iter_mut().find(|(x, _)| x == k) {
                Some(e) => e.1 = v.to_string(),
                None => d.push((k.to_string(), v.to_string())),
            }
        }
        d
    }

    /// The comparison's four outcomes, each from one controlled edit of the emulator's own report:
    /// agreement, a real difference, timing that differs and is only reported, and a capture that
    /// stops short.
    #[test]
    fn the_comparison_separates_facts_from_timing() {
        let emu = run(&timer_loop(), &Options::default());
        let timing: BTreeSet<String> = ["io[1]", "io[2]", "io[3]"].map(String::from).into();

        let dev = device_from(
            &emu,
            &[
                ("io[1]", "0x12345678"),
                ("elapsed_usec", "999"),
                ("cpu_hz", "80000000"),
            ],
        );
        let c = compare(&emu, &dev, &timing);
        assert!(c.agrees(), "{}", c.render());
        assert!(c
            .timing
            .iter()
            .any(|(k, _, d)| k == "io[1]" && d == "0x12345678"));
        assert_eq!(
            c.device_only,
            [("cpu_hz".to_string(), "80000000".to_string())]
        );
        assert_eq!(c.emulator_only.len(), 2);
        assert!(
            c.render()
                .ends_with("AGREE: 15 same, 0 differ, 0 missing, 4 timing\n"),
            "{}",
            c.render()
        );

        // PROC_ID answering the byte-only value the model used to: a real disagreement.
        let dev = device_from(&emu, &[("io[0]", "0x00000055")]);
        let c = compare(&emu, &dev, &timing);
        assert!(!c.agrees());
        assert_eq!(
            c.differ,
            [("io[0]".into(), "0x55555555".into(), "0x00000055".into())]
        );
        assert!(c.render().starts_with("DIFFER   io[0]"));

        // The same timer words, NOT named as timing, are differences.
        let dev = device_from(&emu, &[("io[1]", "0x12345678")]);
        assert!(!compare(&emu, &dev, &BTreeSet::new()).agrees());

        // Numbers compare by value, not spelling.
        let dev = device_from(&emu, &[("io[5]", "1")]);
        assert!(compare(&emu, &dev, &timing).agrees());

        // A truncated capture is not a match.
        let mut dev = device_from(&emu, &[]);
        dev.retain(|(k, _)| k != "io[15]");
        let c = compare(&emu, &dev, &timing);
        assert_eq!(c.missing, ["io[15]"]);
        assert!(!c.agrees());
    }
}
