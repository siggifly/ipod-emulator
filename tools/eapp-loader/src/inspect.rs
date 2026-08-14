//! Look at a file the user picked and say, immediately, whether it will work.
//!
//! The single biggest usability cliff in this project is that a fresh clone does nothing: the NOR
//! dump and the disk image are Apple's, they are gitignored, and you supply your own. The second
//! biggest is what happens next — somebody supplies the wrong ones and finds out ninety seconds
//! into a boot, from a symptom that reads like a bug in the emulator.
//!
//! So both files are parsed before anything is built, and the report says what was *found*, not
//! merely pass or fail. "This is a 2 MiB dump; the 5G/5.5G ROM is 1 MiB" saves an evening.
//!
//! # Everything here is structure, not signature
//!
//! No hashes of Apple's files, no table of known-good dumps. Two reasons, and the second is the
//! load-bearing one: a hash table would only ever recognise the dumps we happen to have, and a
//! **legitimately different** dump — a different capacity, a different firmware version, somebody's
//! own 5.5G — would be rejected for being unfamiliar. The layout is what the emulator actually
//! needs, so the layout is what is checked.
//!
//! Both formats below were read out of the images this project boots, and both agree with the
//! published iPod firmware-directory layout (`ipodlinux`, and `tools/rsrc-extract.py` for the
//! disk side).

use std::fmt::Write as _;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The verdict on one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// It is what the emulator wants. The string is what was found, for the user to read back.
    Good(String),
    /// It parses, and it is not this machine — or it parses with something odd about it. The
    /// emulator will be started only if the user insists.
    Wrong(String),
    /// It does not parse at all, or could not be read.
    Bad(String),
}

impl Verdict {
    pub fn ok(&self) -> bool {
        matches!(self, Verdict::Good(_))
    }
    pub fn text(&self) -> &str {
        match self {
            Verdict::Good(s) | Verdict::Wrong(s) | Verdict::Bad(s) => s,
        }
    }
}

/// Every image in an iPod firmware directory, on the NOR or on the disk.
///
/// One 40-byte record, and the same record in both places — only the magic differs, and the two
/// magics are not spelled the same way in the bytes. On the drive it reads **`!ATA`**, literally,
/// left to right. In the NOR it reads **`hslf`** — `flsh` stored as a little-endian word. Both are
/// u32 constants and one of them happens to be palindromic under the byte order; the first version
/// of this file assumed they behaved alike, compared the drive's against `ATA!`, and reported the
/// project's own reference image as having no firmware directory. Its unit tests passed, because
/// the test helper wrote the same wrong magic.
///
/// Fields, all little-endian:
///
/// ```text
///   +0x00  magic      "!ATA" / "flsh"
///   +0x04  tag        four characters stored as a u32, so `osos` reads as `soso` in a byte dump
///   +0x08  dev        0. `aupd`'s becomes 1 once the updater has run, which is how the second
///                     boot knows to skip it
///   +0x0c  devOffset  where the body is, relative to the partition (or to the NOR base)
///   +0x10  len        how long the body is
///   +0x14  addr       where it is loaded. 0x10000000 on a 5G/5.5G — the discriminator that
///                     matters here, because a later iPod does not load there
///   +0x18  entryOffset
///   +0x1c  checksum
///   +0x20  version
///   +0x24  loadAddr
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub tag: String,
    pub dev: u32,
    pub offset: u32,
    pub len: u32,
    pub addr: u32,
}

/// Where a 5G/5.5G loads every image it runs, from `osos` to the NOR's diagnostics.
pub const LOAD_ADDR_5G: u32 = 0x1000_0000;
/// The retail 5G/5.5G NOR is one megabyte.
pub const NOR_LEN: u64 = 1024 * 1024;
/// The image directory inside it.
const NOR_DIRECTORY: u64 = 0x000f_fe00;
/// The `!ATA` directory, relative to the start of the firmware partition.
const DISK_DIRECTORY: u64 = 0x4200;

fn parse_entries(buf: &[u8], magic: &[u8; 4]) -> Vec<Entry> {
    let mut out = Vec::new();
    for e in buf.chunks_exact(40) {
        if &e[0..4] != magic {
            break;
        }
        let w = |o: usize| u32::from_le_bytes([e[o], e[o + 1], e[o + 2], e[o + 3]]);
        // The tag is a little-endian u32 of four characters, so the bytes read backwards.
        let tag: String = e[4..8].iter().rev().map(|&b| b as char).collect();
        out.push(Entry { tag, dev: w(8), offset: w(0x0c), len: w(0x10), addr: w(0x14) });
    }
    out
}

fn read_at(path: &Path, at: u64, n: usize) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(at))?;
    let mut buf = vec![0u8; n];
    // `read_exact` and not `read`: a short read at the end of a truncated image would otherwise
    // parse as a directory of zeroes and be reported as "no images", which is a different problem
    // from the one the user has.
    f.read_exact(&mut buf)?;
    Ok(buf)
}

// ---------------------------------------------------------------- the NOR dump

/// Parse a NOR flash dump and say what it is.
///
/// Three things are checked, in the order that makes the message most useful:
///
/// 1. **Length.** The 5G/5.5G bootrom is 1 MiB. Rockbox's "Dump ROM contents" names its output
///    after the range it read — `internal_rom_000000-0FFFFF.bin` is this one, and a
///    `…-1FFFFF.bin` is 2 MiB and a different machine.
/// 2. **The reset vector.** Word 0 must be an ARM `B` (`0xEA……`), because that is where a PP502x
///    fetches out of reset. Apple's is `0xea001ffe`.
/// 3. **The image directory at `0xffe00`**, magic `flsh`, whose entries name `disk`, `diag`,
///    `scan`, `logo` and `vmcs` and load every one of them at `0x10000000`.
pub fn flash(path: &Path) -> Verdict {
    let len = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) => return Verdict::Bad(format!("cannot read this file: {e}")),
    };
    let head = match read_at(path, 0, 4) {
        Ok(b) => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        Err(e) => return Verdict::Bad(format!("cannot read this file: {e}")),
    };

    // Size first, because it is the check that produces the most useful sentence. The conventional
    // filename says the range: `internal_rom_000000-0FFFFF.bin` is this one, 0x00000..0xFFFFF.
    if len < NOR_LEN {
        return Verdict::Wrong(format!(
            "{} — too small for a 5G/5.5G NOR, which is exactly 1 MiB (1 048 576 bytes). \
             A 512 KiB dump is a nano-class device. This emulator models the 5G/5.5G (PP5021C) \
             only.",
            bytes(len)
        ));
    }
    if len > NOR_LEN {
        // 2 MiB is the interesting case and deserves its own sentence: it is somebody else's iPod,
        // dumped perfectly correctly. Anything much larger is not a ROM dump at all, and saying
        // "this is a 6G Classic" about a 13 MiB firmware bundle would be a confident wrong answer.
        let what = if len <= 4 * NOR_LEN {
            "A 2 MiB dump is a later model: a 6G Classic or a nano."
        } else {
            "A file this size is not a ROM dump — a firmware bundle, an .ipsw, or a disk image."
        };
        return Verdict::Wrong(format!(
            "{} — larger than a 5G/5.5G NOR, which is exactly 1 MiB (1 048 576 bytes). {what} \
             The 5G/5.5G dump is conventionally named `internal_rom_000000-0FFFFF.bin`, after the \
             offset range it covers; a `…-1FFFFF.bin` is twice the chip and a different machine.",
            bytes(len)
        ));
    }
    if head >> 24 != 0xEA {
        return Verdict::Bad(format!(
            "word 0 is {head:#010x}, which is not an ARM branch. A PP502x fetches its reset vector \
             at address 0, and Apple's is a `B` — {:#010x} on the retail 5G. This file is not a raw \
             NOR dump (an encrypted or wrapped dump, or a different file entirely).",
            0xea00_1ffeu32
        ));
    }

    let dir = match read_at(path, NOR_DIRECTORY, 0x200) {
        Ok(b) => parse_entries(&b, b"hslf"),
        Err(e) => return Verdict::Bad(format!("cannot read the image directory: {e}")),
    };
    if dir.is_empty() {
        return Verdict::Wrong(format!(
            "1 MiB and a plausible reset vector, but no `flsh` image directory at {NOR_DIRECTORY:#x}. \
             The 5G/5.5G NOR carries one, naming the images it can boot — disk, diag, logo, vmcs. This may be a \
             dump of a different model, or of only part of the chip."
        ));
    }
    let stray: Vec<&Entry> = dir.iter().filter(|e| e.addr != LOAD_ADDR_5G).collect();
    if !stray.is_empty() {
        return Verdict::Wrong(format!(
            "the `flsh` directory loads `{}` at {:#010x}, not {LOAD_ADDR_5G:#010x}. \
             Every image on a 5G/5.5G loads at {LOAD_ADDR_5G:#010x}; a different load address is a \
             different machine.",
            stray[0].tag, stray[0].addr
        ));
    }

    let mut s = format!(
        "1 MiB, reset vector {head:#010x} (ARM branch), {} images at {LOAD_ADDR_5G:#010x}: ",
        dir.len()
    );
    let names: Vec<&str> = dir.iter().map(|e| e.tag.as_str()).collect();
    s.push_str(&names.join(" · "));
    if let Some(b) = build_string(path) {
        let _ = write!(s, "\n{b}");
    }
    Verdict::Good(s)
}

/// The bootloader's own build line, if it is there. **Reported, never judged on** — the prototype
/// dump and the retail dump differ here (Nov 28 2006 against Mar 10 2008) and both are 5G NORs, so
/// this identifies which dump you have without deciding anything.
fn build_string(path: &Path) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let needle = b"Bootloader UI Shell";
    let at = data.windows(needle.len()).position(|w| w == needle)?;
    let end = data[at..].iter().position(|&b| b == 0).unwrap_or(0) + at;
    let s = String::from_utf8_lossy(&data[at..end.min(at + 96)]).into_owned();
    Some(s.trim().to_string())
}

// ---------------------------------------------------------------- the drive image

/// Parse a drive image and say what it is.
///
/// 1. **The MBR** at LBA 0 — signature `0x55AA`, partition 0 of type `0x00`, which is Apple's
///    firmware partition. An Apple Partition Map (`ER` at byte 0) and a GPT are both recognised
///    and named, because "this is a Mac-formatted iPod" is a far more useful thing to be told
///    than "no MBR signature".
/// 2. **The `!ATA` directory** at partition start + `0x4200`.
/// 3. **An `osos` entry loading at `0x10000000`**, whose body is inside the file.
pub fn disk(path: &Path) -> Verdict {
    let len = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) => return Verdict::Bad(format!("cannot read this file: {e}")),
    };
    let mbr = match read_at(path, 0, 512) {
        Ok(b) => b,
        Err(e) => return Verdict::Bad(format!("cannot read the first sector: {e}")),
    };

    if &mbr[0..2] == b"ER" {
        return Verdict::Wrong(
            "this is an Apple Partition Map (`ER` at byte 0) — a Mac-formatted iPod. \
             The recipes here read an MBR: partition 0, type 0x00, at LBA 63. \
             A Windows-formatted iPod is the layout this emulator's images use."
                .into(),
        );
    }
    if mbr[510] != 0x55 || mbr[511] != 0xAA {
        return Verdict::Bad(format!(
            "no MBR signature in the first sector ({:#04x}{:02x}, expected 0x55aa). \
             This is not a whole-drive image — a partition image, a compressed file, or a firmware \
             bundle rather than a disk.",
            mbr[510], mbr[511]
        ));
    }
    if read_at(path, 512, 8).map(|b| b == *b"EFI PART").unwrap_or(false) {
        return Verdict::Wrong(
            "the MBR is a GPT protective MBR (`EFI PART` at LBA 1). \
             The iPod's own layout is a plain MBR with the firmware partition first."
                .into(),
        );
    }

    // Partition 0: 16 bytes at 0x1be. +0x04 type, +0x08 first LBA, +0x0c sector count.
    let p = &mbr[0x1be..0x1be + 16];
    let ptype = p[4];
    let lba = u32::from_le_bytes([p[8], p[9], p[10], p[11]]) as u64;
    let sectors = u32::from_le_bytes([p[12], p[13], p[14], p[15]]) as u64;
    if ptype != 0x00 || lba == 0 {
        return Verdict::Wrong(format!(
            "no firmware partition found — MBR partition 1 is type {ptype:#04x} starting at LBA \
             {lba}, where Apple's is type 0x00 at LBA 63. This drive may have been reformatted by \
             a PC, which wipes the firmware partition and leaves the data one. \
             You do not need to find the original: build a fresh disk from an IPSW instead — \
             `ipod-boot make-disk`, or the setup screen's IPSW slot."
        ));
    }

    let dir = match read_at(path, lba * 512 + DISK_DIRECTORY, 0x200) {
        Ok(b) => parse_entries(&b, b"!ATA"),
        Err(e) => return Verdict::Bad(format!("cannot read the firmware directory: {e}")),
    };
    if dir.is_empty() {
        return Verdict::Wrong(format!(
            "no firmware partition found — there is one at LBA {lba} ({sectors} sectors) but no \
             `!ATA` directory at partition + {DISK_DIRECTORY:#x}, so it is empty. \
             This drive may have been reformatted. Build a fresh disk from an IPSW instead — \
             `ipod-boot make-disk`, or the setup screen's IPSW slot."
        ));
    }
    let Some(osos) = dir.iter().find(|e| e.tag == "osos") else {
        let names: Vec<&str> = dir.iter().map(|e| e.tag.as_str()).collect();
        return Verdict::Wrong(format!(
            "the `!ATA` directory lists {} but no `osos`. \
             `osos` is the OS image the bootloader loads; without it there is nothing to boot.",
            names.join(" · ")
        ));
    };
    if osos.addr != LOAD_ADDR_5G {
        return Verdict::Wrong(format!(
            "this is not a 5G/5.5G image: `osos` loads at {:#010x}, not {LOAD_ADDR_5G:#010x}. \
             A 5G/5.5G loads its OS at {LOAD_ADDR_5G:#010x} — a 6G Classic and the nanos do not. \
             This emulator is 5G/5.5G (PP5021C) only.",
            osos.addr
        ));
    }
    let body_end = lba * 512 + osos.offset as u64 + osos.len as u64;
    if body_end > len {
        return Verdict::Bad(format!(
            "`osos` claims {} ending at byte {body_end}, past the end of a {} file. \
             The image is truncated — a partial copy, or a download that stopped.",
            bytes(osos.len as u64),
            bytes(len)
        ));
    }

    let mut s = format!(
        "{}, MBR, firmware partition at LBA {lba} ({} sectors). \
         `osos` {} loading at {:#010x}",
        bytes(len),
        sectors,
        bytes(osos.len as u64),
        osos.addr
    );
    let others: Vec<String> = dir
        .iter()
        .filter(|e| e.tag != "osos")
        .map(|e| format!("{} {}", e.tag, bytes(e.len as u64)))
        .collect();
    if !others.is_empty() {
        let _ = write!(s, ", plus {}", others.join(" · "));
    }
    // `aupd` present and unmarked means the ROM will run Apple's flash updater instead of the OS —
    // correct behaviour, a genuinely different boot, and not what somebody expecting a main menu
    // is about to see. `flash-update.sh` is the recipe for it.
    if let Some(aupd) = dir.iter().find(|e| e.tag == "aupd") {
        if aupd.dev == 0 {
            let _ = write!(
                s,
                "\nNote: `aupd` is present and not yet marked done, so the bootloader will run \
                 Apple's FLASH UPDATER on the first boot rather than the OS. That is correct \
                 behaviour and it takes two boots — `ipod-boot flash-update` is the recipe."
            );
        }
    }
    Verdict::Good(s)
}

// ---------------------------------------------------------------- the IPSW, and the disk it makes

/// Parse an IPSW and say whether its firmware partition is a 5G/5.5G one.
///
/// The same [`Verdict`] the two image checks produce, so the setup screen has one shape to draw.
/// The work is [`eapp_loader::ipsw`]'s: a zip reader, an inflate, a CRC-32 check, and the `!ATA`
/// directory out of the extracted bundle.
pub fn ipsw(path: &Path) -> Verdict {
    match crate::ipsw::inspect(path) {
        crate::ipsw::Ipsw::Good(s, _) => Verdict::Good(s),
        crate::ipsw::Ipsw::Wrong(s) => Verdict::Wrong(s),
        crate::ipsw::Ipsw::Bad(s) => Verdict::Bad(s),
    }
}

/// Build a drive image from an IPSW, at `out`. Returns what to say about it.
pub fn build_from_ipsw(src: &Path, out: &Path) -> Result<String, String> {
    let fw = match crate::ipsw::inspect(src) {
        crate::ipsw::Ipsw::Good(_, fw) => fw,
        crate::ipsw::Ipsw::Wrong(s) | crate::ipsw::Ipsw::Bad(s) => return Err(s),
    };
    if let Some(d) = out.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    crate::ipsw::build_disk(&fw, out, crate::ipsw::DEFAULT_SECTORS)?;
    Ok(format!(
        "built {} — 8 GiB, sparse, about 20 MB on disk. Apple's firmware partition byte for byte, \
         and an empty FAT32 volume that RetailOS populates itself on first boot.",
        out.display()
    ))
}

fn bytes(n: u64) -> String {
    if n >= 1 << 30 {
        format!("{:.2} GiB", n as f64 / (1u64 << 30) as f64)
    } else if n >= 1 << 20 {
        format!("{:.2} MiB", n as f64 / (1u64 << 20) as f64)
    } else {
        format!("{n} bytes")
    }
}

/// `ipod-gui --check-images`: the same two reports, on stdout, with no window.
///
/// So that "does my dump work" is answerable on a machine with no display, in CI, and over SSH —
/// and so the README can tell somebody to run one command rather than describing a dialog.
pub fn report(flash_path: &Path, disk_path: &Path) -> i32 {
    let mut bad = 0;
    for (what, p, v) in [
        ("NOR dump  ", flash_path, flash(flash_path)),
        ("disk image", disk_path, disk(disk_path)),
    ] {
        let mark = match v {
            Verdict::Good(_) => "OK  ",
            Verdict::Wrong(_) => "NOT THIS MACHINE",
            Verdict::Bad(_) => "UNREADABLE",
        };
        println!("{what}  {mark}  {}", p.display());
        for line in v.text().lines() {
            println!("             {line}");
        }
        if !v.ok() {
            bad += 1;
        }
    }
    if bad == 0 {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(magic: &[u8; 4], tag: &str, dev: u32, off: u32, len: u32, addr: u32) -> Vec<u8> {
        let mut e = vec![0u8; 40];
        e[0..4].copy_from_slice(magic);
        // Stored little-endian, so the characters go in backwards — the property that makes `osos`
        // read as `soso` in a byte dump, and the one a hand-written test gets wrong first.
        for (i, c) in tag.bytes().rev().enumerate() {
            e[4 + i] = c;
        }
        e[8..12].copy_from_slice(&dev.to_le_bytes());
        e[0x0c..0x10].copy_from_slice(&off.to_le_bytes());
        e[0x10..0x14].copy_from_slice(&len.to_le_bytes());
        e[0x14..0x18].copy_from_slice(&addr.to_le_bytes());
        e
    }

    /// **The bytes off the real images, copied out of a hex dump**, not synthesised by the helper
    /// below.
    ///
    /// This is the test that would have caught the bug this file shipped for an hour: the drive's
    /// magic reads `!ATA` left to right and the NOR's reads `hslf`, and comparing the drive's
    /// against `ATA!` made the project's own reference image report "no firmware partition found".
    /// Every synthetic test passed throughout, because the helper wrote the same wrong magic. A
    /// parser's fixtures have to come from the format, not from the parser's author.
    #[test]
    fn the_two_magics_are_the_bytes_the_real_images_carry() {
        // resources/derived/disk/ipod8g-retail.img at 63*512 + 0x4200 — the `osos` record.
        let disk: [u8; 40] = [
            0x21, 0x41, 0x54, 0x41, 0x73, 0x6f, 0x73, 0x6f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x44,
            0x00, 0x00, 0x00, 0x5a, 0x73, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00,
            0xf3, 0x48, 0x7c, 0x2c, 0x12, 0xb0, 0x00, 0x00, 0x08, 0x00, 0x7c, 0x18,
        ];
        let e = parse_entries(&disk, b"!ATA");
        assert_eq!(e.len(), 1, "the drive's magic is `!ATA`, left to right");
        assert_eq!(e[0].tag, "osos");
        assert_eq!(e[0].len, 0x0073_5a00, "7 559 680 bytes — the RetailOS image");
        assert_eq!(e[0].addr, LOAD_ADDR_5G);
        assert!(parse_entries(&disk, b"ATA!").is_empty(), "and it is NOT `ATA!`");

        // The retail NOR at 0xffe00 — the `disk` record.
        let nor: [u8; 40] = [
            0x68, 0x73, 0x6c, 0x66, 0x6b, 0x73, 0x69, 0x64, 0x00, 0x00, 0x00, 0x00, 0xd0, 0x3b,
            0x0d, 0x00, 0x30, 0xc2, 0x02, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00,
            0x72, 0x8c, 0x19, 0x01, 0x12, 0xb0, 0x00, 0x00, 0x78, 0x08, 0x6b, 0x00,
        ];
        let e = parse_entries(&nor, b"hslf");
        assert_eq!(e.len(), 1, "the NOR's magic is `flsh` stored little-endian");
        assert_eq!(e[0].tag, "disk");
        assert_eq!(e[0].addr, LOAD_ADDR_5G);
        assert!(parse_entries(&nor, b"flsh").is_empty(), "and it is NOT `flsh`");
    }

    #[test]
    fn the_tag_is_a_little_endian_word_not_a_string() {
        let e = entry(b"!ATA", "osos", 0, 0x4400, 0x735a00, LOAD_ADDR_5G);
        assert_eq!(&e[4..8], b"soso", "the bytes on disk read backwards");
        let parsed = parse_entries(&e, b"!ATA");
        assert_eq!(parsed[0].tag, "osos");
        assert_eq!(parsed[0].len, 0x735a00);
        assert_eq!(parsed[0].addr, LOAD_ADDR_5G);
    }

    /// The directory ends at the first record whose magic is absent — it is not a counted list.
    #[test]
    fn parsing_stops_at_the_first_record_without_the_magic() {
        let mut buf = entry(b"!ATA", "osos", 0, 0x4400, 16, LOAD_ADDR_5G);
        buf.extend(entry(b"!ATA", "rsrc", 0, 0x73a000, 32, LOAD_ADDR_5G));
        buf.extend(vec![0u8; 40]);
        buf.extend(entry(b"!ATA", "aupd", 0, 0x900000, 8, LOAD_ADDR_5G));
        let e = parse_entries(&buf, b"!ATA");
        assert_eq!(e.len(), 2, "the zeroed record terminates the directory");
        assert_eq!(e[1].tag, "rsrc");
    }

    /// `tag` names the file. Named rather than derived from the arguments, because the first
    /// version hashed them into a filename, two tests hashed to the same one, and the parallel
    /// test runner had them deleting each other's images mid-read.
    fn synthetic_disk(tag: &str, dir: &[u8], total: u64, ptype: u8, lba: u32) -> std::path::PathBuf {
        let p = std::env::temp_dir()
            .join(format!("ipod-gui-inspect-{tag}-{}.img", std::process::id()));
        let mut img = vec![0u8; total as usize];
        img[510] = 0x55;
        img[511] = 0xAA;
        img[0x1be + 4] = ptype;
        img[0x1be + 8..0x1be + 12].copy_from_slice(&lba.to_le_bytes());
        img[0x1be + 12..0x1be + 16].copy_from_slice(&27140u32.to_le_bytes());
        let at = lba as usize * 512 + DISK_DIRECTORY as usize;
        img[at..at + dir.len()].copy_from_slice(dir);
        std::fs::write(&p, &img).unwrap();
        p
    }

    #[test]
    fn a_well_formed_disk_is_recognised_and_described() {
        let dir = entry(b"!ATA", "osos", 0, 0x4400, 4096, LOAD_ADDR_5G);
        let p = synthetic_disk("good", &dir, 1 << 21, 0x00, 63);
        let v = disk(&p);
        assert!(v.ok(), "{v:?}");
        assert!(v.text().contains("LBA 63"), "{v:?}");
        assert!(v.text().contains("0x10000000"), "{v:?}");
        let _ = std::fs::remove_file(p);
    }

    /// The message a 6G Classic owner should get: not "invalid", but "this is not that machine".
    #[test]
    fn a_foreign_load_address_says_which_machine_this_is() {
        let dir = entry(b"!ATA", "osos", 0, 0x4400, 4096, 0x0800_0000);
        let p = synthetic_disk("foreign", &dir, 1 << 21, 0x00, 63);
        let v = disk(&p);
        assert!(matches!(v, Verdict::Wrong(_)), "{v:?}");
        assert!(v.text().contains("0x08000000"), "{v:?}");
        assert!(v.text().contains("5G/5.5G"), "{v:?}");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn a_truncated_osos_body_is_reported_as_truncation() {
        let dir = entry(b"!ATA", "osos", 0, 0x4400, 0x0080_0000, LOAD_ADDR_5G);
        let p = synthetic_disk("truncated", &dir, 1 << 21, 0x00, 63);
        let v = disk(&p);
        assert!(matches!(v, Verdict::Bad(_)), "{v:?}");
        assert!(v.text().contains("truncated"), "{v:?}");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn an_apple_partition_map_is_named_rather_than_called_invalid() {
        let p = std::env::temp_dir().join(format!("ipod-gui-apm-{}.img", std::process::id()));
        let mut img = vec![0u8; 4096];
        img[0] = b'E';
        img[1] = b'R';
        std::fs::write(&p, &img).unwrap();
        let v = disk(&p);
        assert!(matches!(v, Verdict::Wrong(_)), "{v:?}");
        assert!(v.text().contains("Apple Partition Map"), "{v:?}");
        let _ = std::fs::remove_file(p);
    }

    fn synthetic_nor(len: u64, word0: u32, dir: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir()
            .join(format!("ipod-gui-nor-{}-{len}-{word0:08x}.bin", std::process::id()));
        let mut rom = vec![0u8; len as usize];
        rom[0..4].copy_from_slice(&word0.to_le_bytes());
        if len as usize > NOR_DIRECTORY as usize + dir.len() {
            let at = NOR_DIRECTORY as usize;
            rom[at..at + dir.len()].copy_from_slice(dir);
        }
        std::fs::write(&p, &rom).unwrap();
        p
    }

    #[test]
    fn a_well_formed_nor_is_recognised() {
        let mut dir = entry(b"hslf", "disk", 0, 0xd3bd0, 0x2c230, LOAD_ADDR_5G);
        dir.extend(entry(b"hslf", "diag", 0, 0xbbda8, 0x17e28, LOAD_ADDR_5G));
        let p = synthetic_nor(NOR_LEN, 0xea00_1ffe, &dir);
        let v = flash(&p);
        assert!(v.ok(), "{v:?}");
        assert!(v.text().contains("disk"), "{v:?}");
        assert!(v.text().contains("diag"), "{v:?}");
        let _ = std::fs::remove_file(p);
    }

    /// The 2 MiB case is the one worth a sentence: it is somebody else's iPod, dumped correctly.
    #[test]
    fn a_two_megabyte_dump_is_told_it_is_a_different_ipod() {
        let p = synthetic_nor(2 * NOR_LEN, 0xea00_1ffe, &[]);
        let v = flash(&p);
        assert!(matches!(v, Verdict::Wrong(_)), "{v:?}");
        assert!(v.text().contains("1 MiB"), "{v:?}");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn a_non_branch_reset_vector_is_refused() {
        let p = synthetic_nor(NOR_LEN, 0x0000_0000, &[]);
        let v = flash(&p);
        assert!(matches!(v, Verdict::Bad(_)), "{v:?}");
        assert!(v.text().contains("ARM branch"), "{v:?}");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn a_missing_file_says_so_instead_of_panicking() {
        let p = std::env::temp_dir().join("ipod-gui-definitely-not-here.bin");
        assert!(matches!(flash(&p), Verdict::Bad(_)));
        assert!(matches!(disk(&p), Verdict::Bad(_)));
    }
}
