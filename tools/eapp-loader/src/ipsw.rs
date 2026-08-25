//! Build a bootable iPod drive image out of an IPSW, so nobody has to supply somebody else's disk.
//!
//! # Why this is the front door
//!
//! The only thing a drive image *must* carry is the firmware partition — `osos` and `rsrc`.
//! Everything else RetailOS builds itself on first boot: measured, it writes 41 sectors, creates
//! `Contacts`, `Calendars`, `Notes` and `iPod_Control/Device/Accessories`, writes two placeholder
//! vCards, and deletes `IC-Info.sid`. It bootstraps its own volume.
//!
//! And the firmware partition is exactly what an IPSW contains: `Firmware-20.6.3` inside
//! `iPod_20.1.3.ipsw` is 13 895 680 bytes, which is 27 140 sectors, which is precisely the size of
//! MBR partition 0 on a real iPod. It fits with nothing left over — that is how the offset was
//! known in the first place, and `tools/ipod-boot/flash-update.sh` has been writing it into a disk
//! with `dd … seek=63 conv=notrunc` since before this module existed.
//!
//! So the supply burden drops from **8 GB of somebody's iPod to 14 MB of Apple's firmware**, and
//! three problems go with it: no third-party serial number or FireWire GUID, no stranger's music
//! library, and no mismatch between a NOR that belonged to one machine and a drive that belonged to
//! another. It is also what iTunes does when it restores a device, so it is not a workaround.
//!
//! `--disk=` is untouched and still the way every number in `research/` was measured.
//!
//! # No dependencies, and what that costs
//!
//! An IPSW is a zip, and its members are deflated, so this file carries an inflate. That is the
//! price of `eapp-loader` having one dependency (`arm7tdmi`, a path) and the README's claim that
//! the core crates build with no third-party code. It is not a guess at correctness either: every
//! member carries a CRC-32 and [`Zip::extract`] checks it, so a wrong bit anywhere in the inflate
//! is a hard error on the very first real file rather than a subtly corrupt firmware partition.

use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

// ---------------------------------------------------------------- CRC-32, and the zip's own check

/// CRC-32/ISO-HDLC — the one zip stores per member, and the one that makes the inflate below
/// self-checking rather than merely plausible.
pub fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, e) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *e = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xff) as usize] ^ (crc >> 8);
    }
    !crc
}

// ---------------------------------------------------------------- DEFLATE

/// Canonical Huffman decoding table, built from a list of code lengths — RFC 1951 §3.2.2.
struct Huffman {
    /// `counts[n]` = how many codes are `n` bits long.
    counts: [u16; 16],
    /// Symbols, ordered by code length then by symbol.
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Huffman, String> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + counts[i - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Huffman { counts, symbols })
    }
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
    acc: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader {
            data,
            pos: 0,
            bit: 0,
            acc: 0,
        }
    }

    fn bits(&mut self, n: u32) -> Result<u32, String> {
        while self.bit < n {
            let b = *self
                .data
                .get(self.pos)
                .ok_or("deflate: ran off the end of the stream")?;
            self.pos += 1;
            self.acc |= (b as u32) << self.bit;
            self.bit += 8;
        }
        // `n == 0` is a real call — a Huffman table with one symbol asks for zero bits —
        // and `1u32 << 0 - 1` is a mask of nothing, so it is spelled out rather than
        // arrived at.
        let v = if n == 0 { 0 } else { self.acc & ((1u32 << n) - 1) };
        self.acc >>= n;
        self.bit -= n;
        Ok(v)
    }

    /// Huffman codes are packed most-significant-bit-first within the code but the stream is read
    /// least-significant-bit-first, so the code is accumulated one bit at a time. RFC 1951 §3.1.1.
    fn decode(&mut self, h: &Huffman) -> Result<u16, String> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= self.bits(1)? as i32;
            let count = h.counts[len] as i32;
            if code - count < first {
                return Ok(h.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("deflate: a code longer than 15 bits".into())
    }

    fn align(&mut self) {
        self.acc = 0;
        self.bit = 0;
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Raw DEFLATE (RFC 1951) — no zlib header, which is what a zip member holds.
///
/// `expect` is the uncompressed size the zip's central directory declares; it is used to size the
/// output up front and as a bound, so a malformed stream cannot make this allocate without limit.
pub fn inflate(data: &[u8], expect: usize) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::with_capacity(expect);
    let mut br = BitReader::new(data);
    loop {
        let last = br.bits(1)?;
        match br.bits(2)? {
            // Stored: byte-aligned, a length and its complement, then the bytes.
            0 => {
                br.align();
                let p = br.pos;
                let len = u16::from_le_bytes([
                    *data.get(p).ok_or("deflate: truncated stored block")?,
                    *data.get(p + 1).ok_or("deflate: truncated stored block")?,
                ]) as usize;
                let nlen = u16::from_le_bytes([
                    *data.get(p + 2).ok_or("deflate: truncated stored block")?,
                    *data.get(p + 3).ok_or("deflate: truncated stored block")?,
                ]);
                if len as u16 != !nlen {
                    return Err("deflate: stored block length does not match its complement".into());
                }
                let s = p + 4;
                let e = s + len;
                out.extend_from_slice(data.get(s..e).ok_or("deflate: truncated stored block")?);
                br.pos = e;
            }
            1 => {
                let (lit, dist) = fixed_tables()?;
                block(&mut br, &lit, &dist, &mut out, expect)?;
            }
            2 => {
                let (lit, dist) = dynamic_tables(&mut br)?;
                block(&mut br, &lit, &dist, &mut out, expect)?;
            }
            _ => return Err("deflate: reserved block type".into()),
        }
        if last == 1 {
            return Ok(out);
        }
    }
}

fn fixed_tables() -> Result<(Huffman, Huffman), String> {
    let mut l = [0u8; 288];
    for (i, e) in l.iter_mut().enumerate() {
        *e = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    Ok((Huffman::new(&l)?, Huffman::new(&[5u8; 30])?))
}

fn dynamic_tables(br: &mut BitReader) -> Result<(Huffman, Huffman), String> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let hlit = br.bits(5)? as usize + 257;
    let hdist = br.bits(5)? as usize + 1;
    let hclen = br.bits(4)? as usize + 4;
    let mut cl = [0u8; 19];
    for i in 0..hclen {
        cl[ORDER[i]] = br.bits(3)? as u8;
    }
    let clh = Huffman::new(&cl)?;

    let mut lengths = vec![0u8; hlit + hdist];
    let mut i = 0;
    while i < lengths.len() {
        let sym = br.decode(&clh)?;
        match sym {
            0..=15 => {
                lengths[i] = sym as u8;
                i += 1;
            }
            16 => {
                let prev = *lengths
                    .get(i.wrapping_sub(1))
                    .ok_or("deflate: repeat with no previous length")?;
                let n = 3 + br.bits(2)? as usize;
                for _ in 0..n {
                    *lengths
                        .get_mut(i)
                        .ok_or("deflate: length repeat overruns")? = prev;
                    i += 1;
                }
            }
            17 => i += 3 + br.bits(3)? as usize,
            18 => i += 11 + br.bits(7)? as usize,
            _ => return Err("deflate: bad code-length symbol".into()),
        }
    }
    if i > lengths.len() {
        return Err("deflate: code lengths overrun their table".into());
    }
    Ok((
        Huffman::new(&lengths[..hlit])?,
        Huffman::new(&lengths[hlit..])?,
    ))
}

fn block(
    br: &mut BitReader,
    lit: &Huffman,
    dist: &Huffman,
    out: &mut Vec<u8>,
    limit: usize,
) -> Result<(), String> {
    loop {
        let sym = br.decode(lit)?;
        match sym {
            0..=255 => out.push(sym as u8),
            256 => return Ok(()),
            257..=285 => {
                let i = sym as usize - 257;
                let len = LEN_BASE[i] as usize + br.bits(LEN_EXTRA[i] as u32)? as usize;
                let d = br.decode(dist)? as usize;
                if d >= 30 {
                    return Err("deflate: distance symbol out of range".into());
                }
                let back = DIST_BASE[d] as usize + br.bits(DIST_EXTRA[d] as u32)? as usize;
                if back > out.len() {
                    return Err("deflate: back-reference before the start of the output".into());
                }
                // Byte by byte on purpose: overlapping copies are legal and are how run-length
                // encoding falls out of LZ77.
                let start = out.len() - back;
                for k in 0..len {
                    let b = out[start + k];
                    out.push(b);
                }
            }
            _ => return Err("deflate: literal/length symbol out of range".into()),
        }
        // `limit` is the declared uncompressed size. A stream that exceeds it is malformed, and
        // without this check a crafted one could exhaust memory.
        if out.len() > limit + 1 {
            return Err("deflate: output is longer than the archive says it should be".into());
        }
    }
}

// ---------------------------------------------------------------- the zip container

/// One member of the archive.
#[derive(Clone, Debug)]
pub struct Member {
    pub name: String,
    pub method: u16,
    pub packed: u64,
    pub size: u64,
    pub crc: u32,
    /// Offset of the local file header.
    header: u64,
}

pub struct Zip {
    data: Vec<u8>,
    pub members: Vec<Member>,
}

fn le16(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}
fn le32(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

impl Zip {
    /// Read the central directory. An IPSW is two members and a few hundred bytes of directory, so
    /// the whole file is read into memory — it is 6.5 MB.
    pub fn open(path: &Path) -> Result<Zip, String> {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if data.len() < 22 {
            return Err("not a zip archive: the file is too short to hold a directory".into());
        }
        // The end-of-central-directory record is last, after a comment of up to 64 KiB.
        let start = data.len().saturating_sub(22 + 0xFFFF);
        let eocd = (start..=data.len() - 22)
            .rev()
            .find(|&i| data[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
            .ok_or("not a zip archive: no end-of-central-directory record")?;
        let count = le16(&data, eocd + 10) as usize;
        let mut at = le32(&data, eocd + 16) as usize;

        let mut members = Vec::with_capacity(count);
        for _ in 0..count {
            if data.get(at..at + 4) != Some(&[0x50, 0x4b, 0x01, 0x02][..]) {
                return Err(
                    "zip: the central directory is not where the archive says it is".into(),
                );
            }
            let nlen = le16(&data, at + 28) as usize;
            let elen = le16(&data, at + 30) as usize;
            let clen = le16(&data, at + 32) as usize;
            let name = String::from_utf8_lossy(&data[at + 46..at + 46 + nlen]).into_owned();
            members.push(Member {
                name,
                method: le16(&data, at + 10),
                crc: le32(&data, at + 16),
                packed: le32(&data, at + 20) as u64,
                size: le32(&data, at + 24) as u64,
                header: le32(&data, at + 42) as u64,
            });
            at += 46 + nlen + elen + clen;
        }
        Ok(Zip { data, members })
    }

    /// Decompress one member and check its CRC-32.
    ///
    /// The check is not decoration: it is what turns "this inflate looks right" into "this inflate
    /// reproduced 13 895 680 bytes that hash to the value Apple's archiver recorded in 2008".
    pub fn extract(&self, m: &Member) -> Result<Vec<u8>, String> {
        let h = m.header as usize;
        if self.data.get(h..h + 4) != Some(&[0x50, 0x4b, 0x03, 0x04][..]) {
            return Err(format!("zip: no local header for {}", m.name));
        }
        // The local header repeats the name and may carry a DIFFERENT extra field from the central
        // one, so both lengths are read from here rather than reused.
        let nlen = le16(&self.data, h + 26) as usize;
        let elen = le16(&self.data, h + 28) as usize;
        let start = h + 30 + nlen + elen;
        let end = start + m.packed as usize;
        let packed = self
            .data
            .get(start..end)
            .ok_or_else(|| format!("zip: {} runs past the end of the archive", m.name))?;

        let out = match m.method {
            0 => packed.to_vec(),
            8 => inflate(packed, m.size as usize)?,
            other => {
                return Err(format!(
                    "zip: {} uses compression method {other}, which this reader does not implement",
                    m.name
                ))
            }
        };
        if out.len() as u64 != m.size {
            return Err(format!(
                "zip: {} unpacked to {} bytes, not the {} the directory declares",
                m.name,
                out.len(),
                m.size
            ));
        }
        if crc32(&out) != m.crc {
            return Err(format!(
                "zip: {} failed its CRC-32 — the archive is damaged, or this reader is wrong. \
                 Either way the firmware would be corrupt and nothing is written.",
                m.name
            ));
        }
        Ok(out)
    }

    /// The firmware partition image, which is the whole point of opening an IPSW.
    ///
    /// Matched by name rather than by position: `Firmware-20.6.3` in `iPod_20.1.3.ipsw`, and
    /// `Firmware-<x>` in every other one. `manifest.plist` is the only other member.
    pub fn firmware(&self) -> Result<(Member, Vec<u8>), String> {
        let m = self
            .members
            .iter()
            .find(|m| m.name.starts_with("Firmware-"))
            .or_else(|| self.members.iter().max_by_key(|m| m.size))
            .ok_or("this archive has no members")?
            .clone();
        if !m.name.starts_with("Firmware-") {
            return Err(format!(
                "no `Firmware-…` member in this archive — its largest is `{}`. \
                 An iPod IPSW contains exactly two files: the firmware partition image and \
                 `manifest.plist`.",
                m.name
            ));
        }
        let body = self.extract(&m)?;
        Ok((m, body))
    }
}

// ---------------------------------------------------------------- what is inside the firmware

/// One image in an iPod firmware directory: `osos`, `rsrc`, `aupd`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub tag: String,
    pub dev: u32,
    pub offset: u32,
    pub len: u32,
    pub addr: u32,
    /// Where execution begins, **relative to `addr`**.
    ///
    /// Zero for a stock `osos` — Apple's own image is entered at its base. It is **not** zero once
    /// a bootloader has been installed: `install-os` appends the loader to the end of `osos` and
    /// records its position here, which is how Apple's bootloader knows to run the loader rather
    /// than the OS it is sitting behind (`Running 'osos' 0 from 0x10735A00`).
    ///
    /// Ignoring it is why the high-level boot ran RetailOS on a drive with `ipodloader2` installed.
    pub entry: u32,
}

/// Where the `!ATA` directory sits inside a firmware partition.
pub const DIRECTORY_AT: usize = 0x4200;
/// Where a 5G/5.5G loads every image it runs.
pub const LOAD_ADDR_5G: u32 = 0x1000_0000;
/// MBR partition 0 starts here on every iPod this project has seen.
pub const FIRMWARE_LBA: u32 = 63;

/// Parse the `!ATA` directory out of a firmware partition image.
pub fn images(fw: &[u8]) -> Vec<Image> {
    let mut out = Vec::new();
    let Some(dir) = fw.get(DIRECTORY_AT..DIRECTORY_AT + 0x200) else {
        return out;
    };
    for e in dir.chunks_exact(40) {
        if &e[0..4] != b"!ATA" {
            break;
        }
        // The tag is a little-endian u32 of four characters, so the bytes read backwards — `osos`
        // appears as `soso` in a byte dump.
        let tag: String = e[4..8].iter().rev().map(|&b| b as char).collect();
        out.push(Image {
            tag,
            dev: le32(e, 8),
            offset: le32(e, 0x0c),
            len: le32(e, 0x10),
            addr: le32(e, 0x14),
            entry: le32(e, 0x18),
        });
    }
    out
}

/// Where an OS image starts inside the bytes at its `devOffset`.
///
/// An ARM image entered at its base opens with the exception vector table, so its first two words
/// are branches (`0xEA……`). That is a property of the thing being looked for rather than a number
/// that has to be right for every bundle Apple shipped — and it has to be, because the header is
/// `0x200` on the 5G's bundle and `0x800` on the 5.5G's.
pub fn image_header(window: &[u8]) -> Option<usize> {
    (0..window.len().saturating_sub(8))
        .step_by(4)
        .find(|&o| window[o + 3] == 0xEA && window[o + 7] == 0xEA)
}

/// Pull the OS image out of a drive's own firmware partition.
///
/// **This is what a high-level boot needs and a warm boot did not.** `--osos=` takes the image as a
/// separate file, which is fine for research and useless to somebody who has a drive and nothing
/// else: the drive already carries the OS, at LBA 63, indexed by the same `!ATA` directory
/// [`images`] reads. Reading it from there is the difference between "supply three files" and
/// "supply one".
///
/// Returns the image and the address it loads at.
pub fn osos_from_drive(path: &std::path::Path) -> Result<(Vec<u8>, u32, u32), String> {
    image_from_drive(path, "osos")
}

/// The same, for any image the firmware directory lists.
///
/// `osos` is the OS and is what a boot normally wants, but the directory also carries `rsrc` and —
/// the interesting one — **`aupd`, Apple's flash updater**, which is the program that writes the
/// NOR. Being able to enter it directly is the only way to ask what it would do on a machine whose
/// ROM cannot launch it, and a firmware image is a firmware image; nothing here is `osos`-specific
/// except which tag was looked up.
pub fn image_from_drive(path: &std::path::Path, tag: &str) -> Result<(Vec<u8>, u32, u32), String> {
    // **The header this project has now got wrong twice, in two sizes.**
    //
    // `devOffset` in the `!ATA` directory is relative to the firmware PARTITION, but what is
    // written at LBA 63 is Apple's extracted `Firmware-…` file, which carries a header on top. So
    // the byte position inside the drive is `devOffset + header`.
    //
    // The header is **not a constant**. Measured, by finding where the ARM vector table actually
    // begins in each bundle:
    //
    // | bundle | devOffset | vector table | header |
    // |---|---|---|---|
    // | `iPod_20.1.3` (5G) | `0x4400` | `0x4600` | `0x200` |
    // | `iPod_25.1.3` (5.5G) | `0x4800` | `0x5000` | **`0x800`** |
    //
    // Taking devOffset literally lands short by a header and the CPU spins in the exception
    // vectors executing data -- the `OSOS.bin` vs `OSOS_correct.bin` mistake in `research/02`
    // §Provenance. Assuming `0x200` lands short by 0x600 on a 5.5G, which is what made the 5.5G
    // fail to boot at all.
    //
    // So it is FOUND rather than assumed: an ARM image entered at its base begins with the
    // exception vector table, whose first two words are branches. That is a property of the thing
    // being looked for, not a number that has to be right for every bundle Apple ever shipped.
    const MAX_HEADER: u64 = 0x4000;
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let base = FIRMWARE_LBA as u64 * 512;

    // The directory alone, first: a drive whose firmware partition is missing or empty should say
    // so rather than have several megabytes read out of it before anyone notices.
    // `DIRECTORY_AT` is already the offset within the FILE, so this one needs no adjustment.
    let mut dir = vec![0u8; DIRECTORY_AT + 0x200];
    f.seek(SeekFrom::Start(base))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    f.read_exact(&mut dir).map_err(|e| {
        format!(
            "{}: cannot read the firmware partition at LBA {FIRMWARE_LBA}: {e}",
            path.display()
        )
    })?;

    let dir_images = images(&dir);
    if dir_images.is_empty() {
        return Err(format!(
            "{}: no `!ATA` firmware directory at LBA {FIRMWARE_LBA}. This drive has no OS in it — \
             build one from an .ipsw, or point at a drive that already has one.",
            path.display()
        ));
    }
    let osos = dir_images.iter().find(|i| i.tag == tag).ok_or_else(|| {
        format!(
            "{}: the firmware directory lists {} but no `{tag}`, so there is nothing to enter.",
            path.display(),
            dir_images
                .iter()
                .map(|i| i.tag.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    if osos.len == 0 || osos.len > 64 * 1024 * 1024 {
        return Err(format!(
            "{}: `{tag}` claims {} bytes, which is not a size a firmware image has",
            path.display(),
            osos.len
        ));
    }

    // Locate the image start: the first 4-byte-aligned position at or after `devOffset` whose two
    // opening words are both ARM branches.
    let mut window = vec![0u8; MAX_HEADER as usize];
    f.seek(SeekFrom::Start(base + osos.offset as u64))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let got = f
        .read(&mut window)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    window.truncate(got);
    let header = image_header(&window).map(|h| h as u64).ok_or_else(|| {
        format!(
            "{}: no ARM vector table within {MAX_HEADER:#x} of `{tag}` at {:#x}. An image \
                 entered at its base opens with two branch instructions and this one does not, so \
                 either it is not a 5G/5.5G OS image or it is not stored in the clear.",
            path.display(),
            osos.offset
        )
    })?;

    let mut image = vec![0u8; osos.len as usize];
    f.seek(SeekFrom::Start(base + osos.offset as u64 + header))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    f.read_exact(&mut image).map_err(|e| {
        format!(
            "{}: `osos` runs past the end of the image: {e}",
            path.display()
        )
    })?;
    Ok((image, osos.addr, osos.entry))
}

// ---------------------------------------------------------------- building the drive

/// Default drive size: 8 GiB, which is `ipod8g-retail.img`'s, so a disk built here and the
/// reference image have the same geometry and a measurement can be compared between them.
pub const DEFAULT_SECTORS: u64 = 16_777_216;
/// Where the data partition starts on a real iPod, and therefore here. 16 MiB in, well clear of a
/// 27 140-sector firmware partition.
pub const DATA_LBA: u32 = 32_768;

/// 32 sectors per cluster — 16 KiB — which is what a drive this size gets and what keeps the FAT to
/// a few megabytes.
pub const FAT32_SPC: u32 = 32;
/// Reserved sectors before the first FAT.
const FAT32_RESERVED: u32 = 32;
/// Two FATs, which is what every formatter writes.
const FAT32_NFATS: u32 = 2;
/// **The number below which a volume is FAT16 and not FAT32.** Microsoft's own boundary, and the
/// reason a small drive is refused rather than quietly written as something no iPod will mount.
pub const FAT32_MIN_CLUSTERS: u32 = 65_525;
/// The smallest data partition [`fat32`] will produce a volume for.
///
/// Derived rather than typed: the clusters, the FAT that describes them, and the reserved sectors,
/// with one cluster of slack so the ceiling division in `fat32` cannot land a sector short.
pub const MIN_FAT32_SECTORS: u64 = (FAT32_MIN_CLUSTERS as u64 + 1) * FAT32_SPC as u64
    + FAT32_RESERVED as u64
    + 2 * FAT32_NFATS as u64 * (FAT32_MIN_CLUSTERS as u64 * FAT32_SPC as u64).div_ceil(4_097);

/// Mark `aupd` as already applied, in place, and say whether there was one.
///
/// **This is the difference between a disk that boots and one that sits in the bootloader**, and it
/// is one byte.
///
/// A pristine IPSW's firmware directory carries three images: `osos`, `rsrc` and `aupd`, Apple's
/// NOR flash updater. Given all three the boot ROM runs the **updater** rather than the OS — which
/// is correct behaviour and is what `tools/ipod-boot/flash-update.sh` exists to reproduce — and the
/// updater's last act is a `WRITE SECTORS` back to the directory it was launched from, setting
/// `aupd`'s field at +0x08 to 1 so the next boot skips it. That is why a real iPod boots its OS on
/// the second power-up after a restore and not the first.
///
/// **The updater's last act is not one byte, and believing it was is why every drive this program
/// built stopped at its own boot logo.** This function used to write only the 1 and say so:
/// *"Nothing else in the partition is touched: `osos` and `rsrc` are Apple's bytes, unaltered."*
/// Measured 2026-08-25 against `ipod8g-retail.PRISTINE.img`, which is a real iPod after a real
/// restore, that claim is false — Apple's updater rewrites `rsrc`'s load address and entry point
/// too:
///
/// ```text
///                    a real iPod, post-restore   an IPSW as shipped
///     rsrc  addr           0x10000000                  0x00000000
///     rsrc  entry          0x0                         0x600
/// ```
///
/// An IPSW's `rsrc` carries its *packaging* values; a drive's carries its *runtime* ones. Writing
/// the partition verbatim and marking `aupd` applied left the packaging values in place, and the
/// window's boot went **22 ATA commands and 2 612 lit pixels, frozen from 356 M instructions to
/// 4.4 G** — the synthesised bootloader's own logo and nothing after it. With `rsrc` corrected the
/// same drive reaches **70 ATA, 71 695 lit pixels and four co-processor frames**, which is where
/// `ipod-boot retail` also stops and is a different, older defect (see `KNOWN-BUGS.md`).
///
/// **`LOAD_ADDR_5G` rather than a literal**, because it is the same constant `osos` is loaded at
/// and the same one the directory already records for it. What is NOT established is whether every
/// IPSW family's post-update `rsrc` takes that address: the reference drive measured is a 5G's and
/// the drive that exposed this was built from a 5.5G bundle. One family, measured; the other,
/// inferred from `osos` sharing the constant.
///
/// `with_aupd` leaves it armed, which is the configuration `flash-update.sh` measures.
pub fn mark_aupd_applied(fw: &mut [u8]) -> bool {
    let dir = images(fw);
    let Some(i) = dir.iter().position(|e| e.tag == "aupd") else {
        return false;
    };
    if dir[i].dev == 1 {
        return false;
    }
    let at = DIRECTORY_AT + i * 40 + 8;
    fw[at..at + 4].copy_from_slice(&1u32.to_le_bytes());
    // The other half of what the updater does, and the half this function spent its whole life
    // claiming it did not need to. `rsrc` is not executed, so `entry` is 0 on a drive; `addr` is
    // where the image is loaded, which is the same place `osos` goes.
    if let Some(r) = dir.iter().position(|e| e.tag == "rsrc") {
        let at = DIRECTORY_AT + r * 40;
        fw[at + 0x14..at + 0x18].copy_from_slice(&LOAD_ADDR_5G.to_le_bytes());
        fw[at + 0x18..at + 0x1c].copy_from_slice(&0u32.to_le_bytes());
    }
    true
}

/// Write a fresh, bootable drive image: MBR, Apple's firmware partition, and an empty FAT32 volume.
///
/// What RetailOS then does to it is measured and is why nothing more is written here: on first boot
/// it creates `Contacts`, `Calendars`, `Notes` and `iPod_Control/Device/Accessories`, writes two
/// placeholder vCards, and touches 41 sectors in all. It bootstraps its own volume; it just needs
/// one that is formatted.
///
/// The file is created **sparse** — `set_len` then seek-and-write — so an 8 GiB image costs about
/// 21 MB on any filesystem that has holes, which is all three of ext4, APFS and NTFS. (Measured
/// 2026-08-21 on APFS: apparent 8 589 934 592, on disk 20 987 904. The `about 14 MB` this comment
/// carried was never measured; `compose::DRIVE_ON_DISK` is now the one place the number lives.)
///
/// **Two halves, and the split is what makes a build cancellable.** The container goes down first
/// and Apple's bytes second, so a run that stops between them leaves a file with no real name and
/// nothing that looks like a drive. Both halves are public and both re-run the refusals below.
pub fn build_disk(fw: &[u8], out: &Path, sectors: u64) -> Result<(), String> {
    build_volume(out, sectors, fw.len())?;
    write_firmware_partition(out, fw)
}

/// The three things that make a drive impossible, checked before anything is created.
///
/// `sectors` of `0` means "not being asked about the drive's size" — [`write_firmware_partition`]
/// is checking the partition alone.
fn refuse(fw_bytes: usize, sectors: u64) -> Result<u32, String> {
    if !fw_bytes.is_multiple_of(512) {
        return Err(format!(
            "the firmware partition is {fw_bytes} bytes, which is not a whole number of 512-byte sectors"
        ));
    }
    let fw_sectors = (fw_bytes / 512) as u32;
    if (FIRMWARE_LBA + fw_sectors) as u64 >= DATA_LBA as u64 {
        return Err(format!(
            "the firmware partition is {fw_sectors} sectors and would run past LBA {DATA_LBA}, \
             where the data partition starts"
        ));
    }
    // **The real floor, and this used to be 32× under it.** It read `DATA_LBA + 65_536`, which is
    // one cluster count rather than one cluster count times [`FAT32_SPC`] sectors each — so a
    // 65 537-sector volume passed the check that exists to catch exactly this and then failed
    // inside `fat32` with *"2046 clusters, which is FAT16 territory"*, after the file had been
    // created and sized. A pre-write refusal that lets the write start is not one.
    if sectors != 0 && sectors < DATA_LBA as u64 + MIN_FAT32_SECTORS {
        return Err(format!(
            "a {sectors}-sector drive is too small for a FAT32 volume: the data partition needs at \
             least {MIN_FAT32_SECTORS} sectors to hold {FAT32_MIN_CLUSTERS} clusters of \
             {FAT32_SPC} sectors"
        ));
    }
    Ok(fw_sectors)
}

/// Lay out the drive's container: the MBR, the firmware partition's **extent**, and an empty FAT32
/// volume. Apple's bytes are not written — [`write_firmware_partition`] does that.
///
/// `fw_bytes` is how long that partition will be, which the MBR has to state before the bytes
/// exist. Splitting it out is what lets the two halves be two steps of a plan, each of which can be
/// cancelled between.
pub fn build_volume(out: &Path, sectors: u64, fw_bytes: usize) -> Result<(), String> {
    let fw_sectors = refuse(fw_bytes, sectors)?;

    let mut f = std::fs::File::create(out).map_err(|e| format!("{}: {e}", out.display()))?;
    f.set_len(sectors * 512)
        .map_err(|e| format!("{}: {e}", out.display()))?;

    // ---- the MBR. Two entries, and the first one's type is 0x00, which is not a mistake: Apple's
    // firmware partition is marked "empty" so that no PC operating system offers to mount it.
    let mut mbr = [0u8; 512];
    // **Sized to Apple's firmware exactly, because that is what a real iPod has.** Measured on the
    // reference drive: partition 0 is 27 140 sectors, which is `Firmware-20.6.3` to the byte.
    //
    // This briefly grew to `DATA_LBA - FIRMWARE_LBA` so that a bootloader would fit, because
    // `install-os` refuses with "no room" on a drive built here. That was the wrong fix: it made
    // our drives differ from real hardware to work around something that is not a defect. A real
    // post-update iPod has **no `aupd`** — the reference drive carries only `osos` and `rsrc` — and
    // the megabyte the updater occupies here is exactly the room a bootloader goes in. So the drive
    // to install onto is one whose updater has been consumed, not one with a wider partition.
    part(&mut mbr, 0, 0x00, FIRMWARE_LBA, fw_sectors);
    part(
        &mut mbr,
        1,
        0x0b,
        DATA_LBA,
        (sectors - DATA_LBA as u64) as u32,
    );
    mbr[510] = 0x55;
    mbr[511] = 0xAA;
    write_at(&mut f, 0, &mbr)?;

    // ---- an empty FAT32 volume for the rest.
    let vol = fat32(sectors - DATA_LBA as u64)?;
    for (rel, block) in vol {
        write_at(&mut f, (DATA_LBA as u64 + rel) * 512, &block)?;
    }
    f.flush().map_err(|e| format!("{}: {e}", out.display()))
}

/// Write Apple's firmware partition into a drive [`build_volume`] has already laid out.
///
/// **Opened for writing without truncating and without creating.** Writing Apple's bytes into a
/// file nobody laid out would produce a "drive" with no MBR and no volume — one that looks finished
/// to a listing and boots nothing — so a caller that got the order wrong gets an error instead of
/// half a drive.
pub fn write_firmware_partition(out: &Path, fw: &[u8]) -> Result<(), String> {
    // Both halves are public doors, and a check on one door is not a check.
    refuse(fw.len(), 0)?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open(out)
        .map_err(|e| format!("{}: {e}", out.display()))?;
    // Byte for byte, exactly where `dd … seek=63` puts it.
    write_at(&mut f, FIRMWARE_LBA as u64 * 512, fw)?;
    f.flush().map_err(|e| format!("{}: {e}", out.display()))
}

fn part(mbr: &mut [u8], i: usize, kind: u8, lba: u32, sectors: u32) {
    let at = 0x1be + i * 16;
    mbr[at] = 0x00; // not bootable; the iPod's ROM does not read this byte
                    // CHS is meaningless past 8 GB and every consumer of these images is LBA-only. 0xfe/0xff/0xff
                    // is the conventional "use LBA" filler, and it is what the reference image carries.
    mbr[at + 1] = 0xfe;
    mbr[at + 2] = 0xff;
    mbr[at + 3] = 0xff;
    mbr[at + 4] = kind;
    mbr[at + 5] = 0xfe;
    mbr[at + 6] = 0xff;
    mbr[at + 7] = 0xff;
    mbr[at + 8..at + 12].copy_from_slice(&lba.to_le_bytes());
    mbr[at + 12..at + 16].copy_from_slice(&sectors.to_le_bytes());
}

fn write_at(f: &mut std::fs::File, at: u64, data: &[u8]) -> Result<(), String> {
    f.seek(SeekFrom::Start(at))
        .map_err(|e| format!("seek to {at}: {e}"))?;
    f.write_all(data).map_err(|e| format!("write at {at}: {e}"))
}

/// An empty FAT32 volume, as a list of `(sector offset within the partition, bytes)`.
///
/// Returned as sparse blocks rather than one buffer because the two FATs of an 8 GB volume are
/// 4 MB each and all but their first twelve bytes are zero — and the file is sparse, so a zero not
/// written is a block not allocated.
fn fat32(sectors: u64) -> Result<Vec<(u64, Vec<u8>)>, String> {
    let reserved: u32 = FAT32_RESERVED;
    let nfats: u32 = FAT32_NFATS;
    let spc: u32 = FAT32_SPC;

    // Microsoft's own sizing formula (FAT32 spec, "FatSz32"): solve for a FAT big enough to
    // describe the clusters that remain once the FAT itself is subtracted.
    let tmp1 = sectors - reserved as u64;
    let tmp2 = (256 * spc as u64 + nfats as u64) / 2;
    let fat_sectors = tmp1.div_ceil(tmp2) as u32;
    let data_sectors = sectors - reserved as u64 - (nfats as u64 * fat_sectors as u64);
    let clusters = data_sectors / spc as u64;
    if clusters < FAT32_MIN_CLUSTERS as u64 {
        return Err(format!(
            "a {sectors}-sector volume with {spc}-sector clusters gives {clusters} clusters, \
             which is FAT16 territory, not FAT32"
        ));
    }

    let mut bs = vec![0u8; 512];
    bs[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]); // jmp short +0x58; nop — the conventional stub
    bs[3..11].copy_from_slice(b"MSWIN4.1"); // the OEM name every formatter writes and none reads
    bs[11..13].copy_from_slice(&512u16.to_le_bytes()); // bytes per sector
    bs[13] = spc as u8;
    bs[14..16].copy_from_slice(&(reserved as u16).to_le_bytes());
    bs[16] = nfats as u8;
    // FAT32 has no fixed root directory, so both of these are zero, and that is how a reader tells
    // FAT32 from FAT16.
    bs[17..19].copy_from_slice(&0u16.to_le_bytes()); // root entries
    bs[19..21].copy_from_slice(&0u16.to_le_bytes()); // total sectors (16-bit) — 0 means "see +0x20"
    bs[21] = 0xF8; // fixed disk
    bs[22..24].copy_from_slice(&0u16.to_le_bytes()); // FAT size (16-bit) — 0 on FAT32
    bs[24..26].copy_from_slice(&63u16.to_le_bytes()); // sectors per track, cosmetic
    bs[26..28].copy_from_slice(&255u16.to_le_bytes()); // heads, cosmetic
    bs[28..32].copy_from_slice(&DATA_LBA.to_le_bytes()); // hidden sectors: where this partition is
    bs[32..36].copy_from_slice(&(sectors as u32).to_le_bytes()); // total sectors (32-bit)
    bs[36..40].copy_from_slice(&fat_sectors.to_le_bytes());
    bs[40..42].copy_from_slice(&0u16.to_le_bytes()); // flags: both FATs live
    bs[42..44].copy_from_slice(&0u16.to_le_bytes()); // version
    bs[44..48].copy_from_slice(&2u32.to_le_bytes()); // root directory's first cluster
    bs[48..50].copy_from_slice(&1u16.to_le_bytes()); // FSInfo sector
    bs[50..52].copy_from_slice(&6u16.to_le_bytes()); // backup boot sector
    bs[64] = 0x80; // drive number
    bs[66] = 0x29; // extended boot signature: the three fields below are present
    bs[67..71].copy_from_slice(&0x1234_5678u32.to_le_bytes()); // volume id
    bs[71..82].copy_from_slice(b"IPOD       ");
    bs[82..90].copy_from_slice(b"FAT32   ");
    bs[510] = 0x55;
    bs[511] = 0xAA;

    let mut fsinfo = vec![0u8; 512];
    fsinfo[0..4].copy_from_slice(b"RRaA");
    fsinfo[484..488].copy_from_slice(b"rrAa");
    // Free count and next-free hint. Cluster 2 is the root directory, so one is in use.
    fsinfo[488..492].copy_from_slice(&((clusters - 1) as u32).to_le_bytes());
    fsinfo[492..496].copy_from_slice(&3u32.to_le_bytes());
    fsinfo[510] = 0x55;
    fsinfo[511] = 0xAA;

    // The head of each FAT: the media descriptor, the end-of-chain marker, and the root directory's
    // one-cluster chain terminating immediately.
    let mut fat = vec![0u8; 512];
    fat[0..4].copy_from_slice(&0x0FFF_FFF8u32.to_le_bytes());
    fat[4..8].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());
    fat[8..12].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());

    // The root directory: one cluster, and one entry in it — the volume label, which is what makes
    // a freshly built volume show a name rather than an empty string.
    let mut root = vec![0u8; 512];
    root[0..11].copy_from_slice(b"IPOD       ");
    root[11] = 0x08; // ATTR_VOLUME_ID

    let root_lba = reserved as u64 + nfats as u64 * fat_sectors as u64;
    let mut out = vec![
        (0u64, bs.clone()),
        (1, fsinfo.clone()),
        // The backup at sector 6 is three sectors of the same three structures.
        (6, bs),
        (7, fsinfo),
        (reserved as u64, fat.clone()),
        (reserved as u64 + fat_sectors as u64, fat.clone()),
        (root_lba, root),
    ];
    // The rest of the root cluster must be zeroed, and on a fresh sparse file it already is.
    out.sort_by_key(|(a, _)| *a);
    fat.clear();
    Ok(out)
}

// ---------------------------------------------------------------- inspecting an IPSW

/// What an IPSW turned out to be, in a sentence a person can act on.
pub enum Ipsw {
    /// Usable. The string describes it; the bytes are the firmware partition.
    Good(String, Vec<u8>),
    /// It parses and it is not this machine.
    Wrong(String),
    /// It is not an IPSW, or it is damaged.
    Bad(String),
}

/// Open an IPSW, extract its firmware partition, and decide whether it is a 5G/5.5G one.
pub fn inspect(path: &Path) -> Ipsw {
    let zip = match Zip::open(path) {
        Ok(z) => z,
        Err(e) => {
            return Ipsw::Bad(format!(
                "{e}\nAn IPSW is a zip archive containing a firmware partition image and \
                 `manifest.plist`. A `.ipsw` that will not open as a zip is a partial download."
            ))
        }
    };
    let (m, fw) = match zip.firmware() {
        Ok(v) => v,
        Err(e) => return Ipsw::Bad(e),
    };
    let dir = images(&fw);
    if dir.is_empty() {
        return Ipsw::Wrong(format!(
            "`{}` is {} bytes but has no `!ATA` directory at {DIRECTORY_AT:#x}. \
             Every iPod firmware partition carries one; this is a bundle for a device that does \
             not use this layout.",
            m.name,
            fw.len()
        ));
    }
    let Some(osos) = dir.iter().find(|i| i.tag == "osos") else {
        let names: Vec<&str> = dir.iter().map(|i| i.tag.as_str()).collect();
        return Ipsw::Wrong(format!(
            "`{}` lists {} but no `osos` — there is no OS image in it to boot.",
            m.name,
            names.join(", ")
        ));
    };
    if osos.addr != LOAD_ADDR_5G {
        return Ipsw::Wrong(format!(
            "`{}` loads `osos` at {:#010x}, not {LOAD_ADDR_5G:#010x}. \
             A 5G/5.5G loads its OS at {LOAD_ADDR_5G:#010x} — this is a bundle for a different \
             iPod, and this emulator models the 5G/5.5G (PP5021C) only.",
            m.name, osos.addr
        ));
    }
    let mut s = format!(
        "{} — {} bytes ({} sectors), and it fits MBR partition 0 exactly. Images: ",
        m.name,
        fw.len(),
        fw.len() / 512
    );
    let list: Vec<String> = dir
        .iter()
        .map(|i| format!("{} {} KiB @ {:#010x}", i.tag, i.len / 1024, i.addr))
        .collect();
    s.push_str(&list.join(", "));
    // The load-address check above rejects bundles for a different *iPod*. It does not catch a
    // bundle for a different *updater family* of the same iPod, because they all load `osos` at the
    // same address — and that mismatch does not fail loudly. RetailOS boots, does not recognise the
    // drive as its own, and shows "Connect to your computer. Use iTunes to restore." after about 70
    // ATA commands, which reads as a broken emulator rather than a mismatched pair.
    //
    // Measured 2026-08-14: family 25 against a family-20 NOR gives exactly that screen; the matching
    // pair reaches the language picker with 618. So say the family out loud, because the user is the
    // only one who knows which iPod their flash dump came off.
    if let Some(fam) = m
        .name
        .strip_prefix("Firmware-")
        .and_then(|r| r.split('.').next())
    {
        s.push_str(&format!(
            "\n  **Updater family {fam}.** This must match the iPod your NOR dump came from. \
             A mismatch does not fail loudly: RetailOS boots and then asks you to restore it \
             from iTunes, after roughly 70 ATA commands instead of 600."
        ));
    }
    Ipsw::Good(s, fw)
}

#[cfg(test)]
mod tests {
    /// **Read the OS out of a real drive**, which is what a high-level boot does. Skips loudly
    /// without one — the drives are gitignored and 8 GB each.
    #[test]
    fn the_os_can_be_read_out_of_a_drives_firmware_partition() {
        let p = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../resources/drives/ipod8g-retail.img"
        ));
        if !p.is_file() {
            println!("SKIPPED: {} is not here (gitignored)", p.display());
            return;
        }
        let (img, addr, entry) = super::osos_from_drive(p).expect("a retail drive has an OS");
        // A stock drive is entered at the image's base; only an installed bootloader moves it.
        assert_eq!(
            entry, 0,
            "a drive with no bootloader installed has no entry offset"
        );
        assert_eq!(
            addr,
            super::LOAD_ADDR_5G,
            "the 5G loads its OS at 0x10000000"
        );
        assert!(
            img.len() > 1_000_000,
            "an OS image is megabytes, got {}",
            img.len()
        );
        // **The check research/02 gives**: an ARM image entered at its base begins with the
        // exception vector table, so word 0 is a branch. Read one sector early it is data, and
        // this assertion is what catches that.
        let w0 = u32::from_le_bytes(img[..4].try_into().unwrap());
        assert_eq!(
            w0 >> 24,
            0xEA,
            "word 0 is {w0:#010x}, which is not an ARM branch"
        );
        // And its length agrees with what the directory claimed, which is the read this could
        // plausibly get wrong.
        let mut dir = vec![0u8; super::DIRECTORY_AT + 0x200];
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(p).expect("open");
        f.seek(SeekFrom::Start(super::FIRMWARE_LBA as u64 * 512))
            .expect("seek");
        f.read_exact(&mut dir).expect("read");
        let entry = super::images(&dir)
            .into_iter()
            .find(|i| i.tag == "osos")
            .expect("osos");
        assert_eq!(img.len() as u32, entry.len);
        assert_eq!(addr, entry.addr);
    }

    /// **The header is found, not assumed** — it is 0x200 on the 5G's bundle and 0x800 on the
    /// 5.5G's, and assuming the smaller one is what stopped the 5.5G booting at all.
    #[test]
    fn the_image_header_is_located_by_the_vector_table() {
        // Two branches is what an ARM image opens with.
        let vectors = [0x7a, 0x00, 0x00, 0xEA, 0x67, 0x00, 0x00, 0xEA];
        for header in [0usize, 0x200, 0x800, 0x1000] {
            let mut w = vec![0u8; header];
            w.extend_from_slice(&vectors);
            w.resize(header + 64, 0);
            assert_eq!(super::image_header(&w), Some(header), "header {header:#x}");
        }
        // A single branch is not a vector table -- one `0xEA` byte turns up in ordinary data.
        let mut lone = vec![0u8; 16];
        lone[3] = 0xEA;
        assert_eq!(
            super::image_header(&lone),
            None,
            "one branch is not a vector table"
        );
        // And nothing at all is None rather than a panic or a zero.
        assert_eq!(super::image_header(&[]), None);
        assert_eq!(super::image_header(&[0u8; 4096]), None);
    }

    /// A drive with no firmware partition says so, rather than reading megabytes of nothing.
    #[test]
    fn a_drive_with_no_firmware_directory_is_refused_with_a_reason() {
        let dir = std::env::temp_dir().join(format!("ipod-osos-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let blank = dir.join("blank.img");
        std::fs::write(&blank, vec![0u8; 64 * 1024]).expect("write");
        let e = super::osos_from_drive(&blank).unwrap_err();
        assert!(e.contains("firmware"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    #[test]
    fn crc32_matches_the_published_check_value() {
        // The CRC-32 check value from the standard: "123456789" -> 0xCBF43926.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    /// A stored (method 0) DEFLATE block, hand-assembled: final block, type 00, LEN/NLEN, payload.
    #[test]
    fn inflate_reads_a_stored_block() {
        let payload = b"the panel is the one surface that must not be prettified";
        let mut z = vec![0x01];
        z.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(payload.len() as u16)).to_le_bytes());
        z.extend_from_slice(payload);
        assert_eq!(inflate(&z, payload.len()).unwrap(), payload);
    }

    /// The fixed-Huffman path, with a back-reference that **overlaps its own output** — a literal
    /// `a` followed by a length-19 distance-1 copy. This is where a `copy_within` gets it wrong,
    /// because the source of the copy is being written as it is read; run-length encoding falls out
    /// of LZ77 exactly here.
    ///
    /// Bytes produced by zlib at level 9 with `wbits=-15`, so this is a real stream and not a
    /// hand-assembled guess at one.
    #[test]
    fn inflate_reads_fixed_huffman_with_an_overlapping_backreference() {
        let want = vec![b'a'; 20];
        let z = [0x4b, 0x4c, 0xc4, 0x04, 0x00];
        assert_eq!(inflate(&z, want.len()).unwrap(), want);
    }

    /// The dynamic-Huffman path — the one an IPSW's members actually use, and the one with the
    /// code-length alphabet and its three repeat codes in it. Also zlib level 9, `wbits=-15`.
    #[test]
    fn inflate_reads_dynamic_huffman() {
        let want: Vec<u8> = b"the quick brown fox jumps over the lazy dog. ".repeat(8);
        let z = [
            0x2b, 0xc9, 0x48, 0x55, 0x28, 0x2c, 0xcd, 0x4c, 0xce, 0x56, 0x48, 0x2a, 0xca, 0x2f,
            0xcf, 0x53, 0x48, 0xcb, 0xaf, 0x50, 0xc8, 0x2a, 0xcd, 0x2d, 0x28, 0x56, 0xc8, 0x2f,
            0x4b, 0x2d, 0x52, 0x28, 0x01, 0x4a, 0xe7, 0x24, 0x56, 0x55, 0x2a, 0xa4, 0xe4, 0xa7,
            0xeb, 0x81, 0x79, 0xa3, 0x8a, 0xc9, 0x52, 0x0c, 0x00,
        ];
        let got = inflate(&z, want.len()).unwrap();
        assert_eq!(got.len(), want.len());
        assert_eq!(got, want);
        assert_eq!(crc32(&got), crc32(&want));
    }

    #[test]
    fn a_truncated_stream_is_an_error_not_a_panic() {
        assert!(inflate(&[0x01, 0x05, 0x00], 5).is_err());
        assert!(inflate(&[], 5).is_err());
        // Block type 3 is reserved.
        assert!(inflate(&[0x07], 5).is_err());
    }

    /// The directory record is the same 40 bytes on the NOR and on the drive, and its tag is a
    /// little-endian word rather than a string — the detail a hand-written parser gets wrong first.
    #[test]
    fn the_firmware_directory_reads_its_tags_backwards() {
        let mut fw = vec![0u8; DIRECTORY_AT + 0x200];
        let e = DIRECTORY_AT;
        fw[e..e + 4].copy_from_slice(b"!ATA");
        fw[e + 4..e + 8].copy_from_slice(b"soso");
        fw[e + 0x0c..e + 0x10].copy_from_slice(&0x4400u32.to_le_bytes());
        fw[e + 0x10..e + 0x14].copy_from_slice(&0x0073_5a00u32.to_le_bytes());
        fw[e + 0x14..e + 0x18].copy_from_slice(&LOAD_ADDR_5G.to_le_bytes());
        let v = images(&fw);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].tag, "osos");
        assert_eq!(v[0].len, 0x0073_5a00);
        assert_eq!(v[0].addr, LOAD_ADDR_5G);
    }

    /// The built disk must parse as the emulator's own loader reads it: MBR signature, partition 0
    /// type 0x00 at LBA 63, and `!ATA` at partition + 0x4200. That is the whole contract.
    #[test]
    fn a_built_disk_has_the_layout_the_bootloader_looks_for() {
        let mut fw = vec![0u8; 13_895_680];
        let e = DIRECTORY_AT;
        fw[e..e + 4].copy_from_slice(b"!ATA");
        fw[e + 4..e + 8].copy_from_slice(b"soso");
        fw[e + 0x14..e + 0x18].copy_from_slice(&LOAD_ADDR_5G.to_le_bytes());

        let p = std::env::temp_dir().join(format!("ipsw-build-{}.img", std::process::id()));
        build_disk(&fw, &p, DEFAULT_SECTORS).unwrap();

        let f = std::fs::File::open(&p).unwrap();
        assert_eq!(f.metadata().unwrap().len(), DEFAULT_SECTORS * 512);
        let read = |at: u64, n: usize| {
            use std::io::Read;
            let mut f = std::fs::File::open(&p).unwrap();
            f.seek(SeekFrom::Start(at)).unwrap();
            let mut b = vec![0u8; n];
            f.read_exact(&mut b).unwrap();
            b
        };
        let mbr = read(0, 512);
        assert_eq!(&mbr[510..512], &[0x55, 0xAA]);
        assert_eq!(
            mbr[0x1be + 4],
            0x00,
            "Apple's firmware partition is type 0x00"
        );
        assert_eq!(le32(&mbr, 0x1be + 8), FIRMWARE_LBA);
        assert_eq!(
            le32(&mbr, 0x1be + 12),
            27_140,
            "13 895 680 bytes is 27 140 sectors"
        );
        assert_eq!(mbr[0x1be + 16 + 4], 0x0b, "and the data partition is FAT32");
        assert_eq!(le32(&mbr, 0x1be + 16 + 8), DATA_LBA);

        let dir = read(FIRMWARE_LBA as u64 * 512 + DIRECTORY_AT as u64, 8);
        assert_eq!(
            &dir[0..4],
            b"!ATA",
            "the directory landed at partition + 0x4200"
        );
        assert_eq!(&dir[4..8], b"soso");

        // The FAT32 volume, read back through its own boot sector.
        let bs = read(DATA_LBA as u64 * 512, 512);
        assert_eq!(&bs[510..512], &[0x55, 0xAA]);
        assert_eq!(&bs[82..90], b"FAT32   ");
        assert_eq!(le16(&bs, 11), 512, "bytes per sector");
        assert_eq!(le16(&bs, 17), 0, "FAT32 has no fixed root directory");
        assert_eq!(le32(&bs, 44), 2, "and its root starts at cluster 2");
        assert_eq!(
            le32(&bs, 28),
            DATA_LBA,
            "hidden sectors names where the partition is"
        );
        let fat_sectors = le32(&bs, 36) as u64;
        let fat = read((DATA_LBA as u64 + 32) * 512, 12);
        assert_eq!(le32(&fat, 0) & 0x0FFF_FFFF, 0x0FFF_FFF8, "media descriptor");
        assert_eq!(
            le32(&fat, 8),
            0x0FFF_FFFF,
            "the root's chain ends at its first cluster"
        );
        // The second FAT is a copy, and a volume whose two FATs disagree is one a checker will
        // condemn on sight.
        let fat2 = read((DATA_LBA as u64 + 32 + fat_sectors) * 512, 12);
        assert_eq!(fat, fat2);

        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn a_firmware_that_is_not_a_whole_number_of_sectors_is_refused() {
        let p = std::env::temp_dir().join("ipsw-build-odd.img");
        assert!(build_disk(&vec![0u8; 1001], &p, DEFAULT_SECTORS).is_err());
        let _ = std::fs::remove_file(p);
    }

    /// A firmware partition big enough to collide with the data partition must be refused rather
    /// than quietly overwritten — that is a corrupt volume that boots far enough to look fine.
    #[test]
    fn a_firmware_that_would_overrun_the_data_partition_is_refused() {
        let p = std::env::temp_dir().join("ipsw-build-huge.img");
        let too_big = vec![0u8; (DATA_LBA as usize) * 512];
        assert!(build_disk(&too_big, &p, DEFAULT_SECTORS).is_err());
        let _ = std::fs::remove_file(p);
    }
}

/// What a drive image's firmware partition says about itself, read without booting anything.
///
/// The point of this is a screen. RetailOS answers several unrelated faults with one picture — the
/// plug-into-a-computer glyph — and a user restarting the emulator cannot tell which they hit. The
/// emulator can read the same partition RetailOS reads and say so plainly.
#[derive(Debug, Clone, PartialEq)]
pub struct FirmwareState {
    /// Every tag in the `!ATA` directory, in order: normally `osos`, `rsrc`, sometimes `aupd`.
    pub tags: Vec<String>,
    /// An `aupd` image is present and **not** marked applied, so the next boot runs the flash
    /// updater instead of the OS. On hardware that is the first of two boots; here it is a dead
    /// end, because nothing power-cycles the machine and runs the second one.
    pub aupd_armed: bool,
    /// There is an operating system to boot at all.
    pub has_os: bool,
}

/// Read the firmware partition of a drive image and report what is in it.
///
/// Cheap by construction — it reads the first 17 KB of the partition, not the 8 GB drive.
pub fn firmware_state(disk: &Path) -> Result<FirmwareState, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(disk).map_err(|e| format!("{}: {e}", disk.display()))?;
    let mut fw = vec![0u8; DIRECTORY_AT + 0x200];
    f.seek(SeekFrom::Start(u64::from(FIRMWARE_LBA) * 512))
        .and_then(|_| f.read_exact(&mut fw))
        .map_err(|e| format!("{}: firmware partition unreadable: {e}", disk.display()))?;

    let dir = images(&fw);
    Ok(FirmwareState {
        // `dev == 1` is Apple's "already applied" mark — the same field `mark_aupd_applied` sets,
        // read here rather than written.
        aupd_armed: dir.iter().any(|e| e.tag == "aupd" && e.dev != 1),
        has_os: dir.iter().any(|e| e.tag == "osos"),
        tags: dir.into_iter().map(|e| e.tag).collect(),
    })
}

#[cfg(test)]
mod firmware_state_tests {
    use super::*;

    /// Write a drive image whose firmware partition carries `entries` as its `!ATA` directory.
    ///
    /// The byte layout is copied from a real 5.5G drive, dumped at absolute offset 0xC000
    /// (`FIRMWARE_LBA` 63 × 512 + `DIRECTORY_AT`): `!ATA`, then the four tag characters stored
    /// backwards, then `dev` as a little-endian u32.
    fn drive(entries: &[(&str, u32)]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "ipod-fw-{}-{}.img",
            std::process::id(),
            entries
                .iter()
                .map(|(t, d)| format!("{t}{d}"))
                .collect::<String>()
        ));
        let mut img = vec![0u8; FIRMWARE_LBA as usize * 512 + DIRECTORY_AT + 0x200];
        let base = FIRMWARE_LBA as usize * 512 + DIRECTORY_AT;
        for (i, (tag, dev)) in entries.iter().enumerate() {
            let at = base + i * 40;
            img[at..at + 4].copy_from_slice(b"!ATA");
            let t: Vec<u8> = tag.bytes().rev().collect();
            img[at + 4..at + 8].copy_from_slice(&t);
            img[at + 8..at + 12].copy_from_slice(&dev.to_le_bytes());
        }
        std::fs::write(&p, &img).unwrap();
        p
    }

    /// The ordinary drive: an OS, no updater. This is what a real one dumps as.
    #[test]
    fn a_normal_drive_has_an_os_and_no_armed_updater() {
        let p = drive(&[("osos", 0), ("rsrc", 0)]);
        let s = firmware_state(&p).unwrap();
        assert_eq!(
            s.tags,
            ["osos", "rsrc"],
            "tags are stored backwards and must be read back"
        );
        assert!(s.has_os);
        assert!(!s.aupd_armed);
        let _ = std::fs::remove_file(p);
    }

    /// An armed updater — the boot that never reaches RetailOS, because nothing here power-cycles
    /// the machine to run the second one.
    #[test]
    fn an_unapplied_aupd_reads_as_armed() {
        let p = drive(&[("osos", 0), ("rsrc", 0), ("aupd", 0)]);
        assert!(firmware_state(&p).unwrap().aupd_armed);
        let _ = std::fs::remove_file(p);
    }

    /// `dev == 1` is Apple's applied mark, and `make-disk` sets it by default. Reading it as armed
    /// would report the fault on every ordinary drive, which is worse than reporting nothing.
    #[test]
    fn an_applied_aupd_is_not_armed() {
        let p = drive(&[("osos", 0), ("rsrc", 0), ("aupd", 1)]);
        let s = firmware_state(&p).unwrap();
        assert!(!s.aupd_armed);
        assert!(
            s.tags.contains(&"aupd".to_string()),
            "still present, just not armed"
        );
        let _ = std::fs::remove_file(p);
    }

    /// **A drive this program builds carries the directory a real iPod carries after a restore.**
    ///
    /// The `aupd` byte was the whole of what `mark_aupd_applied` used to write, and a drive built
    /// that way stopped at its own boot logo: 22 ATA commands and 2 612 lit pixels, unchanged from
    /// 356 M instructions to 4.4 G. `rsrc` still held its packaging values — the address a shipped
    /// IPSW records rather than the one a restored iPod records.
    ///
    /// Both fields are asserted because both were wrong, and because the easy one to believe
    /// harmless is `entry`: `rsrc` is never executed, so a non-zero entry on it reads as a number
    /// nobody looks at.
    #[test]
    fn the_updater_rewrites_rsrc_as_well_as_marking_itself_applied() {
        let mut fw = vec![0u8; DIRECTORY_AT + 0x200];
        for (i, (tag, dev, addr, entry)) in [
            ("osos", 0u32, LOAD_ADDR_5G, 0u32),
            ("rsrc", 0, 0, 0x600),
            ("aupd", 0, LOAD_ADDR_5G, 0),
        ]
        .iter()
        .enumerate()
        {
            let at = DIRECTORY_AT + i * 40;
            fw[at..at + 4].copy_from_slice(b"!ATA");
            let mut t: Vec<u8> = tag.bytes().collect();
            t.reverse();
            fw[at + 4..at + 8].copy_from_slice(&t);
            fw[at + 8..at + 0x0c].copy_from_slice(&dev.to_le_bytes());
            fw[at + 0x14..at + 0x18].copy_from_slice(&addr.to_le_bytes());
            fw[at + 0x18..at + 0x1c].copy_from_slice(&entry.to_le_bytes());
        }

        let r = images(&fw)
            .into_iter()
            .find(|i| i.tag == "rsrc")
            .expect("rsrc");
        assert_eq!(
            (r.addr, r.entry),
            (0, 0x600),
            "the fixture does not carry an IPSW's own rsrc, so this test cannot show the defect"
        );

        assert!(mark_aupd_applied(&mut fw));
        let after = images(&fw);

        let aupd = after.iter().find(|i| i.tag == "aupd").expect("aupd");
        assert_eq!(aupd.dev, 1, "the updater is not marked applied");

        let r = after.iter().find(|i| i.tag == "rsrc").expect("rsrc");
        assert_eq!(
            (r.addr, r.entry),
            (LOAD_ADDR_5G, 0),
            "`rsrc` still carries the IPSW's packaging values, so this drive stops at the boot logo"
        );

        let o = after.iter().find(|i| i.tag == "osos").expect("osos");
        assert_eq!(
            (o.addr, o.entry),
            (LOAD_ADDR_5G, 0),
            "`osos` was altered; it is Apple's bytes and the updater does not move it"
        );
    }


    /// A drive with nothing bootable on it, which is the honest reading of a file that is not an
    /// iPod drive at all.
    #[test]
    fn a_drive_with_no_os_says_so() {
        let p = drive(&[]);
        let s = firmware_state(&p).unwrap();
        assert!(!s.has_os);
        assert!(s.tags.is_empty());
        let _ = std::fs::remove_file(p);
    }

    /// Too small to hold a firmware partition: an error, not a silent "no OS".
    #[test]
    fn a_truncated_image_is_an_error() {
        let p = std::env::temp_dir().join(format!("ipod-fw-short-{}.img", std::process::id()));
        std::fs::write(&p, b"not a drive").unwrap();
        assert!(firmware_state(&p).is_err());
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod build_split_tests {
    use super::*;

    fn scratch(what: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "ipod-build-split-{what}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&d).expect("a temp directory");
        d
    }

    /// A firmware partition the size of a real one, with an `!ATA` directory in it so
    /// `firmware_state` can read it back.
    fn firmware() -> Vec<u8> {
        let mut fw = vec![0u8; 27_140 * 512];
        for (i, (tag, dev)) in [("osos", 0u32), ("rsrc", 0)].iter().enumerate() {
            let at = DIRECTORY_AT + i * 40;
            fw[at..at + 4].copy_from_slice(b"!ATA");
            let t: Vec<u8> = tag.bytes().rev().collect();
            fw[at + 4..at + 8].copy_from_slice(&t);
            fw[at + 8..at + 12].copy_from_slice(&dev.to_le_bytes());
        }
        fw
    }

    /// **The split is transparent.** `build_disk` is the two halves in order, and the bytes it
    /// produces are the bytes it produced before — otherwise every drive this program has ever
    /// built would differ from the next one for a refactor's sake.
    #[test]
    fn build_disk_is_still_the_two_halves_in_order_and_nothing_else() {
        let dir = scratch("halves");
        let fw = firmware();
        let whole = dir.join("whole.img");
        build_disk(&fw, &whole, DEFAULT_SECTORS).expect("the drive");

        let halves = dir.join("halves.img");
        build_volume(&halves, DEFAULT_SECTORS, fw.len()).expect("the container");
        write_firmware_partition(&halves, &fw).expect("Apple's bytes");

        // **The three regions anything writes, plus what the two files cost.**
        //
        // Not `fs::read` of both: two 8 GiB images is a 16 GiB allocation, and `assert_eq!` on the
        // result would try to *format* both of them on failure — a hung test rather than a red one,
        // which is an instrument that cannot report its own failure. Not a streaming compare
        // either: the holes are not free to read, and it took 107 seconds.
        //
        // The allocated-size equality is what covers "and nothing else was written": a byte written
        // anywhere outside these three regions allocates a block, and the two files would differ.
        let apparent = std::fs::metadata(&whole).unwrap().len();
        assert_eq!(apparent, std::fs::metadata(&halves).unwrap().len());
        assert_eq!(
            crate::settings::on_disk_size(&std::fs::metadata(&whole).unwrap()),
            crate::settings::on_disk_size(&std::fs::metadata(&halves).unwrap()),
            "the two drives cost different amounts, so one of them wrote something the other did not"
        );
        for (what, at, len) in [
            ("the MBR", 0u64, 512usize),
            ("Apple's firmware partition", FIRMWARE_LBA as u64 * 512, fw.len()),
            ("the FAT32 volume", DATA_LBA as u64 * 512, 4 << 20),
        ] {
            assert_eq!(
                region(&whole, at, len),
                region(&halves, at, len),
                "{what} differs between `build_disk` and the two halves"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `len` bytes of `p` starting at `at`.
    fn region(p: &std::path::Path, at: u64, len: usize) -> Vec<u8> {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(p).expect("the image");
        f.seek(SeekFrom::Start(at)).expect("the offset");
        let mut buf = vec![0u8; len];
        f.read_exact(&mut buf).expect("the region");
        buf
    }

    /// **A firmware partition written into no container is an error, not a half drive.**
    ///
    /// `write_firmware_partition` opens without creating and without truncating, so a caller that
    /// gets the order wrong gets a refusal — rather than a file with Apple's bytes at LBA 63, no
    /// MBR, no volume, and every appearance of being a finished drive.
    #[test]
    fn a_firmware_partition_written_into_no_container_is_an_error() {
        let dir = scratch("no-container");
        let nowhere = dir.join("nothing-here.img");
        let e = write_firmware_partition(&nowhere, &firmware())
            .expect_err("writing into a file that does not exist succeeded");
        assert!(e.contains("nothing-here.img"), "{e}");
        assert!(
            !nowhere.exists(),
            "a half drive was created by the call that refused"
        );

        // And the refusals are on both doors: a partition that is not a whole number of sectors is
        // refused by the second half as well as by the first.
        let laid_out = dir.join("laid-out.img");
        build_volume(&laid_out, DEFAULT_SECTORS, 27_140 * 512).expect("the container");
        let ragged = vec![0u8; 27_140 * 512 + 1];
        assert!(
            write_firmware_partition(&laid_out, &ragged).is_err(),
            "a ragged partition passed the second door"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The plan's disk estimate is what a build actually costs**, within a quarter.
    ///
    /// The design this implements said `about 240 MB on disk` for a build that costs 21 MB — an
    /// eleven-fold overstatement, in the direction of alarm, about somebody's own disk. This is the
    /// measurement that keeps `compose::DRIVE_ON_DISK` honest.
    ///
    /// On a volume with no holes the apparent size is what it costs, and that is what is asserted
    /// instead — so a CI runner on a filesystem without sparse files tests the other half rather
    /// than passing vacuously.
    #[test]
    fn the_disk_estimate_is_what_a_build_actually_costs() {
        let dir = scratch("estimate");
        let out = dir.join("measure.img");
        build_disk(&firmware(), &out, DEFAULT_SECTORS).expect("the drive");
        let m = std::fs::metadata(&out).unwrap();
        let apparent = m.len();
        let on_disk = crate::settings::on_disk_size(&m);
        assert_eq!(apparent, DEFAULT_SECTORS * 512);

        let sparse = on_disk < apparent / crate::volume::SPARSE_RATIO;
        let want = if sparse {
            crate::compose::DRIVE_ON_DISK
        } else {
            apparent
        };
        let off = on_disk.abs_diff(want);
        assert!(
            off * 4 <= want,
            "the plan bills {} and the build costs {} — {} out",
            crate::si(want),
            crate::si(on_disk),
            crate::si(off)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
