//! Run one position-independent ARM snippet on the emulated machine, and hold it against the part.
//!
//!   snippet FILE [--iterations=N] [--io=W0,W1,…] [--clock=N] [--budget=N]
//!   snippet FILE … --device=REPORT [--timing=1,2,3]
//!
//! `FILE` is the assembled snippet (`.bin`) or its text form (`.words`). Without `--device` this
//! prints the emulator's `key=value` report — the same lines the lab console's `X` command prints
//! on a real iPod. With `--device=REPORT`, a capture of that command's output, it runs the snippet
//! here and prints the comparison instead: what differs, what is timing, what agrees.
//!
//! `--timing` names `io` words a snippet filled from the timer — reported with both values and a
//! ratio, never counted as a difference. `elapsed_usec` is always timing.
//!
//! Exit status: 0 for a report, or a comparison that agrees; 1 for a fault or a disagreement; 2
//! for a usage error. The contract a snippet is written against is in `ipod_machine::snippet`.

use std::collections::BTreeSet;
use std::process::ExitCode;

use ipod_machine::snippet::{self, parse_u, Options};

const USAGE: &str = "usage: snippet FILE [--iterations=N] [--io=W0,W1,…] [--clock=N] [--budget=N] \
                     [--device=REPORT [--timing=I,J,…]]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let flag = |k: &str| args.iter().find_map(|a| a.strip_prefix(k));
    let known = [
        "--iterations=",
        "--io=",
        "--clock=",
        "--budget=",
        "--device=",
        "--timing=",
    ];
    if let Some(a) = args
        .iter()
        .find(|a| a.starts_with("--") && !known.iter().any(|k| a.starts_with(k)))
    {
        eprintln!("snippet: unknown flag {a}\n{USAGE}");
        return ExitCode::from(2);
    }

    let mut o = Options::default();
    let number = |k: &str| -> Result<Option<u64>, String> {
        flag(k)
            .map(|v| parse_u(v).ok_or_else(|| format!("{k}{v} is not a number")))
            .transpose()
    };
    let parsed = (|| -> Result<(), String> {
        if let Some(n) = number("--iterations=")? {
            o.iterations = n.max(1);
        }
        if let Some(n) = number("--clock=")? {
            o.clock = n.max(1) as usize;
        }
        if let Some(n) = number("--budget=")? {
            o.budget = n as usize;
        }
        if let Some(list) = flag("--io=") {
            for (i, w) in list.split(',').enumerate() {
                if i >= 16 {
                    return Err("--io: more than 16 words".into());
                }
                o.io[i] = parse_u(w)
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| format!("--io: word {i} ({w:?}) is not a 32-bit number"))?;
            }
        }
        Ok(())
    })();
    if let Err(e) = parsed {
        eprintln!("snippet: {e}");
        return ExitCode::from(2);
    }

    let code = match snippet::load(std::path::Path::new(path)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let report = snippet::run(&code, &o);
    let ok = report.iter().any(|(k, v)| k == "status" && v == "ok");

    let Some(device) = flag("--device=") else {
        print!("{}", snippet::render(&report));
        return if ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        };
    };
    let text = match std::fs::read_to_string(device) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{device}: {e}");
            return ExitCode::from(2);
        }
    };
    let dev = snippet::parse_report(&text);
    if dev.is_empty() {
        eprintln!("{device}: no key=value lines — not a snippet report");
        return ExitCode::from(2);
    }
    let timing: BTreeSet<String> = flag("--timing=")
        .map(|l| {
            l.split(',')
                .filter(|s| !s.is_empty())
                .map(|i| format!("io[{}]", i.trim()))
                .collect()
        })
        .unwrap_or_default();
    let c = snippet::compare(&report, &dev, &timing);
    print!("{}", c.render());
    if c.agrees() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
