//! Regenerate `research/FINDINGS.md`.
//!
//! ```sh
//! cargo run -p docgen                  # rewrite research/FINDINGS.md
//! cargo run -p docgen -- --check       # say whether it is current; write nothing
//! ```
//!
//! `cargo test -p docgen` does the same thing and fails on drift, which is what keeps this honest
//! without anybody remembering to run it.

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root");
    let dir = root.join("research");
    let out = dir.join("FINDINGS.md");

    let docs = docgen::read_dir(&dir);
    if docs.is_empty() {
        eprintln!("no documents under {} — nothing to index", dir.display());
        std::process::exit(2);
    }
    let want = docgen::render(&docs);

    if args.iter().any(|a| a == "--check") {
        let have = std::fs::read_to_string(&out).unwrap_or_default();
        if have == want {
            println!("{} is current", out.display());
        } else {
            println!("{} is OUT OF DATE — run `cargo run -p docgen`", out.display());
            std::process::exit(1);
        }
        return;
    }

    std::fs::write(&out, &want).expect("write the index");
    println!(
        "{} — {} documents, {} headings",
        out.display(),
        docs.len(),
        docs.iter().map(|d| d.headings.len()).sum::<usize>()
    );
}
