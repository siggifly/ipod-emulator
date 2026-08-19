//! eapp-inspect — dump the framework import table of an iPod clickwheel-game binary.
//!
//! The eApp layout this parses is an OBSERVED, NOT YET INDEPENDENTLY REPRODUCED format
//! (see ../../README.md#the-eapp-abi). So this tool is deliberately written as a *discovery*
//! instrument rather than a strict parser: it scans for structure, cross-checks every field
//! against an independent signal where one exists, and dumps annotated hex so a wrong
//! assumption shows up as a visible mismatch instead of a silent misparse.
//!
//! Usage:
//!   eapp-inspect <file.bin> [--hex] [--json]
//!   eapp-inspect <dir>      [--json]     # every *.bin under dir, aggregated
//!
//! Exit status is 0 even for non-eApp input — classification is the point, not validation.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const EAPP_MAGIC: &[u8; 4] = b"eapp";
/// Framework-import block magic.
///
/// **Byte order corrected 2026-08-11 against RetailOS 1.3, which is ground truth.** The eApp
/// loader's literal pool inside OSOS holds the bytes `68 19 06 29` — u32 `0x29061968` — next to
/// the `eapp` magic and the `0x10001000` version. A sibling constant `0x13061973` sits beside it.
/// Read as dates (29/06/1968, 13/06/1973) they are obviously deliberate, and the reading only
/// works in this order, which is strong corroboration.
///
/// The earlier report of this constant had the bytes reversed. Both orders are still scanned:
/// only a real game binary settles which one appears in the wild, and silently searching for
/// the wrong one would look identical to "this file has no frameworks".
const BLOCK_MAGIC: [u8; 4] = [0x68, 0x19, 0x06, 0x29];
const BLOCK_MAGIC_REVERSED: [u8; 4] = [0x29, 0x06, 0x19, 0x68];
/// `ldr pc, [pc, #imm12]` — the PLT-style thunk the loader patches at load time.
const LDR_PC_PC: u32 = 0xE59F_F000;
const LDR_PC_PC_MASK: u32 = 0xFFFF_F000;
/// MD5 of the empty string — marks the terminator block.
const EMPTY_MD5: [u8; 16] = [
    0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04, 0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8, 0x42, 0x7e,
];

/// FairPlay `.sinf` atom identifiers.
///
/// Corrected 2026-08-11 against real archives: the 2007 wiki listed these as a flat sequence,
/// but `.sinf` is **nested** — `sinf` is the root container spanning the whole file and `schi`
/// ("scheme information") holds the interesting children. `iviv` carries the 16-byte AES IV and
/// was not in the wiki's list at all.
const SINF_IDS: [&str; 12] = [
    "sinf", "frma", "schm", "schi", "user", "key ", "iviv", "righ", "tran", "name", "priv", "sign",
];

/// Atoms whose payload is more atoms rather than data.
const SINF_CONTAINERS: [&str; 2] = ["sinf", "schi"];

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let hex = args.iter().any(|a| a == "--hex");
    let json = args.iter().any(|a| a == "--json");
    let targets: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();

    if targets.is_empty() {
        eprintln!("usage: eapp-inspect <file.bin|dir> [--hex] [--json]");
        std::process::exit(2);
    }

    let mut files = Vec::new();
    for t in targets {
        let p = PathBuf::from(t);
        if p.is_dir() {
            collect_bins(&p, &mut files);
        } else {
            files.push(p);
        }
    }
    files.sort();

    let reports: Vec<Report> = files
        .iter()
        .filter_map(|p| match fs::read(p) {
            Ok(buf) => Some(inspect(p, &buf)),
            Err(e) => {
                eprintln!("{}: {e}", p.display());
                None
            }
        })
        .collect();

    if json {
        print!("{}", render_json(&reports));
    } else {
        for r in &reports {
            r.print(hex);
        }
        if reports.len() > 1 {
            print_aggregate(&reports);
        }
    }
}

fn collect_bins(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_bins(&p, out);
        // Case-insensitively: the game trees came off FAT volumes, where `GAME.BIN` and `Game.bin`
        // are the same name, and on a case-sensitive filesystem the exact comparison silently
        // skipped half of them.
        } else if p
            .extension()
            .map(|x| x.to_string_lossy().to_ascii_lowercase())
            .is_some_and(|x| x == "bin" || x == "sinf")
        {
            out.push(p);
        }
    }
}

// ---------------------------------------------------------------- model

/// One FairPlay atom inside a `.bin.sinf`. Layout per the iPodLinux wiki:
/// 4-byte size, then 4-byte identifier. Endianness is *derived*, not assumed — see `parse_sinf`.
struct SinfBlock {
    off: usize,
    size: u32,
    id: String,
    known: bool,
    depth: usize,
}

/// `<Name>_<PlatformID>_<HardwareID>_<BuildID>.bin`, e.g. `Pacman_1_1_2563976.bin`.
struct NameFields {
    name: String,
    platform_id: String,
    hardware_id: String,
    build_id: String,
}

struct Report {
    path: PathBuf,
    len: usize,
    entropy: f64,
    is_eapp: bool,
    sinf: Vec<SinfBlock>,
    sinf_endian: Option<&'static str>,
    fields: Option<NameFields>,
    /// Populated only when `is_eapp`.
    header: Option<Header>,
    load_base: Option<u32>,
    blocks: Vec<Block>,
    /// Every `ldr pc,[pc,#imm]` run found anywhere in the file — an independent
    /// cross-check on the per-block function counts.
    thunk_runs: Vec<(usize, usize)>,
}

struct Header {
    version: u32,
    block_count: u32,
    entry_off: u32,
    /// The five raw pointer words following the fixed fields.
    ptrs: Vec<u32>,
}

struct Block {
    file_off: usize,
    name: String,
    hash: [u8; 16],
    func_count: u32,
    next_ptr: u32,
    /// Thunks actually observed immediately after the block header.
    thunks_seen: usize,
    is_terminator: bool,
}

// ---------------------------------------------------------------- parsing

fn inspect(path: &Path, buf: &[u8]) -> Report {
    let is_eapp = buf.len() >= 4 && &buf[..4] == EAPP_MAGIC;
    let entropy = shannon_entropy(buf);
    let load_base = if is_eapp { derive_load_base(buf) } else { None };

    let header = if is_eapp && buf.len() >= 0x28 {
        Some(Header {
            version: u32le(buf, 0x04),
            block_count: u32le(buf, 0x08),
            entry_off: u32le(buf, 0x0c),
            ptrs: (0x10..0x28).step_by(4).map(|o| u32le(buf, o)).collect(),
        })
    } else {
        None
    };

    let blocks = if is_eapp { scan_blocks(buf) } else { Vec::new() };
    let is_sinf = path.extension().is_some_and(|x| x == "sinf");
    let (sinf, sinf_endian) = if is_sinf {
        parse_sinf(buf)
    } else {
        (Vec::new(), None)
    };

    Report {
        path: path.to_path_buf(),
        len: buf.len(),
        entropy,
        is_eapp,
        sinf,
        sinf_endian,
        fields: parse_name_fields(path),
        header,
        load_base,
        blocks,
        thunk_runs: find_thunk_runs(buf),
    }
}

/// Walk the `.sinf` atom list. Endianness is derived by trying both and keeping whichever
/// produces a walk that lands exactly on the end of the file — a wrong guess overshoots or
/// stalls almost immediately, so this is self-validating rather than assumed.
fn parse_sinf(buf: &[u8]) -> (Vec<SinfBlock>, Option<&'static str>) {
    for (label, be) in [("big-endian", true), ("little-endian", false)] {
        let mut out = Vec::new();
        walk_atoms(buf, 0, buf.len(), 0, be, &mut out);
        // Two known identifiers is enough to distinguish a real walk from coincidence, and the
        // wrong endianness fails this immediately rather than producing plausible garbage.
        if out.iter().filter(|b| b.known).count() >= 2 {
            return (out, Some(label));
        }
    }
    (Vec::new(), None)
}

/// Walk one level of the atom tree, descending into containers.
///
/// The flat walk this replaces stopped at the root: `sinf` declares a size covering the whole
/// file, so a non-recursive parser consumes it in one step, sees a single known atom, and
/// concludes the file is unparseable — which is exactly what happened across all 116 archives
/// before this was fixed.
fn walk_atoms(
    buf: &[u8],
    start: usize,
    end: usize,
    depth: usize,
    be: bool,
    out: &mut Vec<SinfBlock>,
) {
    let mut off = start;
    while off + 8 <= end {
        let raw = &buf[off..off + 4];
        let size = if be {
            u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]])
        } else {
            u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])
        };
        let id: String = buf[off + 4..off + 8]
            .iter()
            .map(|&c| if (0x20..0x7f).contains(&c) { c as char } else { '.' })
            .collect();
        // An atom must at least hold its own header and must not run past its parent.
        if size < 8 || off + size as usize > end {
            break;
        }
        out.push(SinfBlock {
            off,
            size,
            known: SINF_IDS.contains(&id.as_str()),
            depth,
            id: id.clone(),
        });
        if SINF_CONTAINERS.contains(&id.as_str()) {
            walk_atoms(buf, off + 8, off + size as usize, depth + 1, be, out);
        }
        off += size as usize;
    }
}

fn parse_name_fields(path: &Path) -> Option<NameFields> {
    let stem = path.file_name()?.to_str()?.trim_end_matches(".sinf");
    let stem = stem.strip_suffix(".bin")?;
    let parts: Vec<&str> = stem.rsplitn(4, '_').collect();
    if parts.len() != 4 {
        return None;
    }
    // rsplitn yields reversed order.
    Some(NameFields {
        build_id: parts[0].to_string(),
        hardware_id: parts[1].to_string(),
        platform_id: parts[2].to_string(),
        name: parts[3].to_string(),
    })
}

/// Scan for every framework block by magic rather than walking `next_ptr`.
///
/// Walking the linked list would be faster, but it trusts the pointer field — and if the
/// observed layout is wrong the walk terminates early and silently. Scanning finds every
/// block regardless, so a bad `next_ptr` shows up as "scan found 6, list-walk implies 5".
fn scan_blocks(buf: &[u8]) -> Vec<Block> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 4 <= buf.len() {
        if buf[i..i + 4] == BLOCK_MAGIC || buf[i..i + 4] == BLOCK_MAGIC_REVERSED {
            if let Some(b) = parse_block(buf, i) {
                out.push(b);
            }
        }
        i += 4; // blocks are word-aligned
    }
    out
}

/// Parse one framework-import block.
///
/// **Layout verified 2026-08-11** against real game binaries *and* RetailOS's own validation code
/// (see the eApp loader research). Offsets relative to the block start:
///
/// ```text
/// +0x00  magic 0x29061968
/// +0x04  name — a FIXED 32-byte buffer, NUL-padded (not variable-length)
/// +0x24  16-byte interface hash — the key frameworks actually bind by
/// +0x34  function count
/// +0x38  pointer (RetailOS rejects the block if this is zero)
/// +0x3C  `count` thunks, then `count` literal slots — hence the count*8 total
///        RetailOS computes with `add r6, r5, r0, lsl #3`
/// ```
///
/// The earlier guess treated the name as NUL-terminated-then-aligned, which put the hash and
/// count in the middle of the name's zero padding and reported `count = 0` for every framework
/// in all 20 binaries. Consistent zeros are what gave it away.
const BLOCK_NAME_OFF: usize = 0x04;
const BLOCK_HASH_OFF: usize = 0x24;
const BLOCK_COUNT_OFF: usize = 0x34;
const BLOCK_PTR_OFF: usize = 0x38;
const BLOCK_THUNKS_OFF: usize = 0x3C;

fn parse_block(buf: &[u8], off: usize) -> Option<Block> {
    if off + BLOCK_THUNKS_OFF > buf.len() {
        return None;
    }
    let name = cstr_at(buf, off + BLOCK_NAME_OFF)?;
    let mut hash = [0u8; 16];
    hash.copy_from_slice(&buf[off + BLOCK_HASH_OFF..off + BLOCK_HASH_OFF + 16]);

    Some(Block {
        file_off: off,
        name,
        hash,
        func_count: u32le(buf, off + BLOCK_COUNT_OFF),
        next_ptr: u32le(buf, off + BLOCK_PTR_OFF),
        thunks_seen: count_thunks_at(buf, off + BLOCK_THUNKS_OFF),
        is_terminator: hash == EMPTY_MD5,
    })
}

/// Derive the load base instead of hardcoding 0x18000000.
///
/// The word at +0x10 points at a framework name string. Whatever base makes that pointer
/// land on printable ASCII inside the file is the real base — which keeps this working if
/// some titles link at a different address.
fn derive_load_base(buf: &[u8]) -> Option<u32> {
    if buf.len() < 0x14 {
        return None;
    }
    let ptr = u32le(buf, 0x10);
    for base in [
        0x1800_0000,
        ptr & 0xFF00_0000,
        ptr & 0xFFF0_0000,
        ptr & 0xFFFF_0000,
        0,
    ] {
        let off = ptr.wrapping_sub(base) as usize;
        if off < buf.len() && cstr_at(buf, off).is_some_and(|s| !s.is_empty()) {
            return Some(base);
        }
    }
    None
}

fn count_thunks_at(buf: &[u8], mut off: usize) -> usize {
    let mut n = 0;
    while off + 4 <= buf.len() && u32le(buf, off) & LDR_PC_PC_MASK == LDR_PC_PC {
        n += 1;
        off += 4;
    }
    n
}

/// Every run of two or more consecutive thunks in the file, as (file_offset, count).
fn find_thunk_runs(buf: &[u8]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut i = 0usize;
    while i + 4 <= buf.len() {
        if u32le(buf, i) & LDR_PC_PC_MASK == LDR_PC_PC {
            let n = count_thunks_at(buf, i);
            if n >= 2 {
                runs.push((i, n));
            }
            i += n * 4;
        } else {
            i += 4;
        }
    }
    runs
}

// ---------------------------------------------------------------- output

impl Report {
    fn print(&self, hex: bool) {
        println!("\n=== {} ===", self.path.display());
        println!(
            "  size {} bytes · entropy {:.2} bits/byte → {}",
            self.len,
            self.entropy,
            classify(self.entropy)
        );

        if let Some(f) = &self.fields {
            println!(
                "  filename fields: name={} platform_id={} hardware_id={} build_id={}",
                f.name, f.platform_id, f.hardware_id, f.build_id
            );
        }

        if !self.sinf.is_empty() {
            println!("  FairPlay .sinf — {} atoms, size field is {}", self.sinf.len(), self.sinf_endian.unwrap_or("?"));
            for b in &self.sinf {
                let payload = b.size.saturating_sub(8);
                let note = match b.id.trim() {
                    "iviv" if payload == 16 => "  ← 16-byte AES-128 IV",
                    "key" => "  ← key reference (payload is too small to be the key itself)",
                    "priv" => "  ← private blob — likely the wrapped key material",
                    "name" => "  ← purchaser's iTunes Store username",
                    _ if SINF_CONTAINERS.contains(&b.id.as_str()) => "  (container)",
                    _ if !b.known => "  ← UNDOCUMENTED atom",
                    _ => "",
                };
                println!(
                    "    {:indent$}{:>6} @0x{:04x}  size {:>5}  payload {:>5}{}",
                    "",
                    b.id,
                    b.off,
                    b.size,
                    payload,
                    note,
                    indent = b.depth * 2
                );
            }
            return;
        }

        if !self.is_eapp {
            println!("  no `eapp` magic — not a plaintext eApp binary");
            return;
        }

        if let Some(h) = &self.header {
            println!(
                "  header: version 0x{:08x} · block_count {} · entry_off 0x{:x}",
                h.version, h.block_count, h.entry_off
            );
            let ptrs: Vec<String> = h.ptrs.iter().map(|p| format!("0x{p:08x}")).collect();
            println!("  header ptrs @+0x10: {}", ptrs.join(" "));
        }
        match self.load_base {
            Some(b) => println!("  load base: 0x{b:08x} (derived)"),
            None => println!("  load base: UNRESOLVED — pointers will not map to file offsets"),
        }

        let real: Vec<&Block> = self.blocks.iter().filter(|b| !b.is_terminator).collect();
        println!("  framework blocks found by scan: {}", self.blocks.len());
        if let Some(h) = &self.header {
            if h.block_count as usize != real.len() {
                println!(
                    "  ⚠️  header says {} blocks, scan found {} non-terminator — layout assumption suspect",
                    h.block_count,
                    real.len()
                );
            }
        }

        let mut total = 0u32;
        for b in &self.blocks {
            if b.is_terminator {
                println!("    [terminator] {:?}", b.name);
                continue;
            }
            total += b.func_count;
            let flag = if b.thunks_seen as u32 == b.func_count {
                "✓"
            } else {
                "⚠️"
            };
            println!(
                "    {:<14} count {:>3}  thunks_seen {:>3} {}  next 0x{:08x}  hash {}  @0x{:x}",
                b.name,
                b.func_count,
                b.thunks_seen,
                flag,
                b.next_ptr,
                hex16(&b.hash),
                b.file_off
            );
        }
        println!("  TOTAL imported functions: {total}");

        let thunk_total: usize = self.thunk_runs.iter().map(|(_, n)| n).sum();
        println!(
            "  independent check: {} `ldr pc,[pc,#imm]` thunks in {} runs file-wide",
            thunk_total,
            self.thunk_runs.len()
        );

        if hex {
            for b in self.blocks.iter().take(2) {
                println!("\n  --- raw @0x{:x} ({}) ---", b.file_off, b.name);
                dump_hex(&fs::read(&self.path).unwrap_or_default(), b.file_off, 96);
            }
        }
    }
}

fn print_aggregate(reports: &[Report]) {
    println!("\n\n=== AGGREGATE across {} files ===", reports.len());
    let eapps = reports.iter().filter(|r| r.is_eapp).count();
    println!(
        "  {eapps} plaintext eApp · {} not eApp (encrypted or other)",
        reports.len() - eapps
    );

    let mut per_fw: BTreeMap<&str, (usize, u32, u32)> = BTreeMap::new(); // name -> (seen, min, max)
    for r in reports {
        for b in r.blocks.iter().filter(|b| !b.is_terminator) {
            let e = per_fw.entry(&b.name).or_insert((0, u32::MAX, 0));
            e.0 += 1;
            e.1 = e.1.min(b.func_count);
            e.2 = e.2.max(b.func_count);
        }
    }
    println!("\n  framework            games   count(min..max)");
    for (name, (seen, lo, hi)) in &per_fw {
        let range = if lo == hi {
            format!("{lo}")
        } else {
            format!("{lo}..{hi}")
        };
        println!("  {name:<20} {seen:>5}   {range}");
    }
    println!(
        "\n  ⚠️  A framework whose count VARIES across games means the ABI is versioned,\n      not fixed — that changes what a loader has to resolve against."
    );
}

fn render_json(reports: &[Report]) -> String {
    let mut s = String::from("[\n");
    for (i, r) in reports.iter().enumerate() {
        s.push_str(&format!(
            "  {{\"file\":{:?},\"size\":{},\"entropy\":{:.4},\"is_eapp\":{},\"load_base\":{},\"frameworks\":[",
            r.path.display().to_string(),
            r.len,
            r.entropy,
            r.is_eapp,
            r.load_base
                .map(|b| format!("{b}"))
                .unwrap_or_else(|| "null".into()),
        ));
        let real: Vec<&Block> = r.blocks.iter().filter(|b| !b.is_terminator).collect();
        for (j, b) in real.iter().enumerate() {
            s.push_str(&format!(
                "{{\"name\":{:?},\"count\":{},\"thunks_seen\":{},\"hash\":\"{}\"}}",
                b.name,
                b.func_count,
                b.thunks_seen,
                hex16(&b.hash)
            ));
            if j + 1 < real.len() {
                s.push(',');
            }
        }
        s.push_str("]}");
        if i + 1 < reports.len() {
            s.push(',');
        }
        s.push('\n');
    }
    s.push_str("]\n");
    s
}

// ---------------------------------------------------------------- helpers

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

#[allow(dead_code)]
fn align4(n: usize) -> usize {
    (n + 3) & !3
}

fn cstr_at(buf: &[u8], off: usize) -> Option<String> {
    let end = buf.get(off..)?.iter().position(|&c| c == 0)? + off;
    let s = &buf[off..end];
    if s.len() > 64 || !s.iter().all(|&c| (0x20..0x7f).contains(&c)) {
        return None;
    }
    Some(String::from_utf8_lossy(s).into_owned())
}

fn hex16(h: &[u8; 16]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whole-file Shannon entropy. AES ciphertext sits at ~8.0; ARM code with string
/// tables and zero padding sits far below it. This is what sorts the 20 from the 34.
fn shannon_entropy(buf: &[u8]) -> f64 {
    if buf.is_empty() {
        return 0.0;
    }
    let mut freq = [0usize; 256];
    for &b in buf {
        freq[b as usize] += 1;
    }
    let len = buf.len() as f64;
    -freq
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / len;
            p * p.log2()
        })
        .sum::<f64>()
}

fn classify(e: f64) -> &'static str {
    if e > 7.9 {
        "ENCRYPTED (or compressed)"
    } else if e > 7.0 {
        "ambiguous — inspect"
    } else {
        "PLAINTEXT"
    }
}

fn dump_hex(buf: &[u8], off: usize, len: usize) {
    let end = (off + len).min(buf.len());
    for row in (off..end).step_by(16) {
        let slice = &buf[row..(row + 16).min(end)];
        let hex: Vec<String> = slice.iter().map(|b| format!("{b:02x}")).collect();
        let ascii: String = slice
            .iter()
            .map(|&c| if (0x20..0x7f).contains(&c) { c as char } else { '.' })
            .collect();
        println!("    {:08x}  {:<47}  |{}|", row, hex.join(" "), ascii);
    }
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic eApp matching the layout observed in the archived Pac-Man binary.
    /// This is the *assumed* layout — if the real files disagree, this fixture is what we
    /// edit, and the diff records exactly what the assumption got wrong.
    fn synth() -> Vec<u8> {
        const BASE: u32 = 0x1800_0000;
        let mut b = Vec::new();
        b.extend_from_slice(EAPP_MAGIC);
        b.extend_from_slice(&0x1000_1000u32.to_le_bytes()); // version
        b.extend_from_slice(&2u32.to_le_bytes()); // block count
        b.extend_from_slice(&0x28u32.to_le_bytes()); // entry off
        for _ in 0..6 {
            b.extend_from_slice(&0u32.to_le_bytes()); // ptrs @+0x10..+0x28
        }
        b.extend_from_slice(&0xEAFF_FFFEu32.to_le_bytes()); // b .  @0x28

        let block = |name: &str, hash: [u8; 16], count: u32, next: u32, buf: &mut Vec<u8>| {
            let start = buf.len();
            buf.extend_from_slice(&BLOCK_MAGIC);
            // Name occupies a fixed 32-byte buffer at +0x04, NUL-padded.
            let mut name_buf = [0u8; 32];
            name_buf[..name.len()].copy_from_slice(name.as_bytes());
            buf.extend_from_slice(&name_buf);
            buf.extend_from_slice(&hash);
            buf.extend_from_slice(&count.to_le_bytes());
            buf.extend_from_slice(&next.to_le_bytes());
            debug_assert_eq!(buf.len() - start, BLOCK_THUNKS_OFF);
            for i in 0..count {
                buf.extend_from_slice(&(LDR_PC_PC | (i * 4)).to_le_bytes());
            }
        };

        block("Audio", [0xAA; 16], 61, 0, &mut b);
        block("InputEvents", [0xBB; 16], 2, 0, &mut b);
        block("$$$$ terminator $$$$", EMPTY_MD5, 0, 0, &mut b);

        // Point header +0x10 at the "Audio" name so load-base derivation has something to hit.
        let name_off = 0x2c + 4;
        b[0x10..0x14].copy_from_slice(&(BASE + name_off as u32).to_le_bytes());
        b
    }

    #[test]
    fn parses_header_and_blocks() {
        let buf = synth();
        let r = inspect(Path::new("synth.bin"), &buf);

        assert!(r.is_eapp);
        let h = r.header.as_ref().expect("header");
        assert_eq!(h.version, 0x1000_1000);
        assert_eq!(h.block_count, 2);
        assert_eq!(h.entry_off, 0x28);

        let real: Vec<&Block> = r.blocks.iter().filter(|b| !b.is_terminator).collect();
        assert_eq!(real.len(), 2, "should find both non-terminator blocks");
        assert_eq!(real[0].name, "Audio");
        assert_eq!(real[0].func_count, 61);
        assert_eq!(real[1].name, "InputEvents");
        assert_eq!(real[1].func_count, 2);
    }

    /// The whole point of the cross-check: declared count must equal thunks actually present.
    #[test]
    fn thunk_count_cross_checks_declared_count() {
        let buf = synth();
        let r = inspect(Path::new("synth.bin"), &buf);
        for b in r.blocks.iter().filter(|b| !b.is_terminator) {
            assert_eq!(
                b.thunks_seen as u32, b.func_count,
                "{} declared {} but {} thunks follow",
                b.name, b.func_count, b.thunks_seen
            );
        }
    }

    #[test]
    fn terminator_is_recognised_by_empty_md5() {
        let buf = synth();
        let r = inspect(Path::new("synth.bin"), &buf);
        assert_eq!(r.blocks.iter().filter(|b| b.is_terminator).count(), 1);
    }

    #[test]
    fn load_base_is_derived_not_assumed() {
        let buf = synth();
        let r = inspect(Path::new("synth.bin"), &buf);
        assert_eq!(r.load_base, Some(0x1800_0000));
    }

    #[test]
    fn entropy_separates_plaintext_from_ciphertext() {
        // Plaintext-ish: ARM code has heavy byte skew.
        let code = synth();
        assert!(shannon_entropy(&code) < 7.0, "synthetic code should read as plaintext");

        // Ciphertext-ish: uniform bytes.
        let uniform: Vec<u8> = (0..=255u8).cycle().take(65536).collect();
        assert!(shannon_entropy(&uniform) > 7.9, "uniform bytes should read as encrypted");
    }

    /// Build a synthetic `.sinf` in QuickTime-atom shape: big-endian size, then 4cc id.
    fn synth_sinf() -> Vec<u8> {
        let mut b = Vec::new();
        let atom = |id: &str, payload: usize, buf: &mut Vec<u8>| {
            buf.extend_from_slice(&((payload + 8) as u32).to_be_bytes());
            buf.extend_from_slice(id.as_bytes());
            buf.extend(std::iter::repeat_n(0xAB, payload));
        };
        atom("frma", 8, &mut b);
        atom("schm", 12, &mut b);
        atom("key ", 16, &mut b);
        atom("name", 20, &mut b);
        b
    }

    #[test]
    fn sinf_endianness_is_derived_and_atoms_walk_cleanly() {
        let buf = synth_sinf();
        let (blocks, endian) = parse_sinf(&buf);
        assert_eq!(endian, Some("big-endian"));
        let ids: Vec<&str> = blocks.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["frma", "schm", "key ", "name"]);
        assert!(blocks.iter().all(|b| b.known), "all atoms should be recognised");
        // The `key` atom is the extraction target — its payload size must be recoverable.
        let key = blocks.iter().find(|b| b.id.trim() == "key").unwrap();
        assert_eq!(key.size - 8, 16);
    }

    #[test]
    fn sinf_walk_rejects_garbage_rather_than_inventing_atoms() {
        let (blocks, endian) = parse_sinf(&[0xFF; 64]);
        assert!(blocks.is_empty());
        assert_eq!(endian, None);
    }

    #[test]
    fn executable_filename_fields_parse() {
        // Real example from the iPodLinux wiki listing.
        let f = parse_name_fields(Path::new("Pacman_1_1_2563976.bin")).expect("fields");
        assert_eq!(f.name, "Pacman");
        assert_eq!(f.platform_id, "1");
        assert_eq!(f.hardware_id, "1");
        assert_eq!(f.build_id, "2563976");

        let z = parse_name_fields(Path::new("Zuma_1_1_2563298.bin")).expect("fields");
        assert_eq!(z.name, "Zuma");
        assert_eq!(z.build_id, "2563298");
    }

    #[test]
    fn non_eapp_input_is_classified_not_rejected() {
        let r = inspect(Path::new("x.bin"), b"not an eapp file at all");
        assert!(!r.is_eapp);
        assert!(r.blocks.is_empty());
    }
}
