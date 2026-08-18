//! Read the `rsrc` volume out of an iPod firmware partition, and files out of it.
//!
//! **Why this exists.** The emulator mounts `rsrc` the way RetailOS does, but for most of this
//! project there was no way to get a file out of it onto the host and look at it — which is how
//! `vmcs.bin` went un-analysed while sitting at the centre of the last open bypass.
//!
//! **Deliberately pure parsing.** No `hdiutil`, no `diskutil`, no mounting. This project's disk
//! images are irreplaceable reference material and a partitioning command aimed at the wrong device
//! is the one mistake here with no undo. Read-only by construction: there is no write path in this
//! file at all, which is a stronger guarantee than remembering to open the file read-only.
//!
//! Layout, all measured rather than assumed (research/10, research/11):
//!
//! * MBR partition 0, type `0x00`, starts at LBA 63.
//! * The `!ATA` directory sits at `partition + 0x4200`, in 40-byte entries.
//! * Each entry is `!ATA` magic, then a four-character tag stored as a **u32** — so `rsrc` reads as
//!   `crsr` in a byte dump. It is a little-endian constant, not a string, and reading it as one is
//!   a mistake this comment exists to stop somebody repeating.
//! * `+0x0c` is the device offset relative to the partition, `+0x10` the length.
//! * `rsrc` itself carries a `0x200` header in front of its FAT12 boot sector.

/// One image in the firmware partition's directory: its tag, its absolute byte offset in the disk
/// image, and its length.
pub struct Image {
    pub tag: String,
    pub offset: u64,
    pub len: u32,
}

/// Every image in the firmware partition's `!ATA` directory, in directory order.
pub fn read_directory(disk: &[u8], part_lba: u64) -> Result<Vec<Image>, String> {
    let part = (part_lba * 512) as usize;
    let at = part + 0x4200;
    if disk.len() < at + 0x200 {
        return Err(format!("image is {} bytes, too short to hold a directory", disk.len()));
    }
    let d = &disk[at..at + 0x200];
    if &d[..4] != b"!ATA" {
        return Err("no `!ATA` directory at partition+0x4200 — wrong image or wrong partition".into());
    }
    let mut out = Vec::new();
    let mut off = 0usize;
    while off + 40 <= d.len() {
        let e = &d[off..off + 40];
        if &e[..4] != b"!ATA" {
            break;
        }
        // The tag is a u32, so its bytes run backwards relative to the name.
        let tag: String = e[4..8].iter().rev().map(|&c| c as char).collect();
        let devoff = u32::from_le_bytes(e[0x0c..0x10].try_into().unwrap()) as u64;
        let len = u32::from_le_bytes(e[0x10..0x14].try_into().unwrap());
        out.push(Image { tag, offset: part as u64 + devoff, len });
        off += 40;
    }
    Ok(out)
}

/// Just enough FAT12 to walk a read-only volume.
pub struct Fat12<'a> {
    d: &'a [u8],
    bps: usize,
    spc: usize,
    nroot: usize,
    fat: usize,
    root: usize,
    data: usize,
}

/// One entry from a walk: full path, first cluster, size, and whether it is a directory.
pub struct Entry {
    pub path: String,
    pub cluster: u16,
    pub size: u32,
    pub is_dir: bool,
}

impl<'a> Fat12<'a> {
    pub fn new(d: &'a [u8]) -> Result<Fat12<'a>, String> {
        if d.len() < 512 {
            return Err("volume is shorter than one boot sector".into());
        }
        let u16at = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]) as usize;
        let bps = u16at(11);
        let spc = d[13] as usize;
        let rsvd = u16at(14);
        let nfat = d[16] as usize;
        let nroot = u16at(17);
        let spf = u16at(22);
        if bps == 0 || spc == 0 {
            return Err("boot sector declares a zero sector or cluster size".into());
        }
        let fat = rsvd * bps;
        let root = fat + nfat * spf * bps;
        let data = root + nroot * 32;
        Ok(Fat12 { d, bps, spc, nroot, fat, root, data })
    }

    fn next_cluster(&self, n: u16) -> u16 {
        let o = self.fat + (n as usize * 3) / 2;
        if o + 2 > self.d.len() {
            return 0xfff;
        }
        let v = u16::from_le_bytes([self.d[o], self.d[o + 1]]);
        if n & 1 == 1 { v >> 4 } else { v & 0xfff }
    }

    /// Follow a cluster chain, bounded by the recorded size as well as by the FAT terminator.
    ///
    /// The size bound is load-bearing: a cross-linked or truncated chain would otherwise walk until
    /// it hit a value that happened to look like end-of-chain.
    pub fn read_chain(&self, first: u16, size: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(size as usize);
        let mut c = first;
        let step = self.spc * self.bps;
        while (2..0xff0).contains(&c) && out.len() < size as usize {
            let o = self.data + (c as usize - 2) * step;
            if o >= self.d.len() {
                break;
            }
            out.extend_from_slice(&self.d[o..(o + step).min(self.d.len())]);
            c = self.next_cluster(c);
        }
        out.truncate(size as usize);
        out
    }

    /// Every file and directory, depth first, with `/`-joined paths.
    pub fn walk(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        self.walk_at(self.root, self.nroot, "", &mut out, 0);
        out
    }

    fn walk_at(&self, off: usize, nents: usize, path: &str, out: &mut Vec<Entry>, depth: usize) {
        // A directory whose `..` is wrong, or a cycle, would otherwise recurse until the stack ran
        // out. Nothing in a firmware `rsrc` is anywhere near this deep.
        if depth > 16 {
            return;
        }
        for i in 0..nents {
            let s = off + i * 32;
            if s + 32 > self.d.len() {
                return;
            }
            let e = &self.d[s..s + 32];
            // Free, deleted, or the second half of a long-name entry.
            if e[0] == 0 || e[0] == 0xe5 || e[11] & 0x0f == 0x0f {
                continue;
            }
            let base = String::from_utf8_lossy(&e[0..8]).trim_end().to_string();
            let ext = String::from_utf8_lossy(&e[8..11]).trim_end().to_string();
            let name = if ext.is_empty() { base } else { format!("{base}.{ext}") };
            if name == "." || name == ".." {
                continue;
            }
            let cluster = u16::from_le_bytes([e[26], e[27]]);
            let size = u32::from_le_bytes(e[28..32].try_into().unwrap());
            let full = format!("{path}{name}");
            if e[11] & 0x10 != 0 {
                out.push(Entry { path: format!("{full}/"), cluster, size: 0, is_dir: true });
                if cluster >= 2 {
                    let o = self.data + (cluster as usize - 2) * self.spc * self.bps;
                    let n = self.spc * self.bps / 32;
                    self.walk_at(o, n, &format!("{full}/"), out, depth + 1);
                }
            } else {
                out.push(Entry { path: full, cluster, size, is_dir: false });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic firmware directory, so the tag's byte order is asserted rather than assumed.
    #[test]
    fn the_directory_tag_is_a_u32_and_reads_back_as_its_name() {
        let mut disk = vec![0u8; 63 * 512 + 0x4400];
        let at = 63 * 512 + 0x4200;
        disk[at..at + 4].copy_from_slice(b"!ATA");
        // `rsrc` stored as a little-endian u32 is the bytes reversed.
        disk[at + 4..at + 8].copy_from_slice(b"crsr");
        disk[at + 0x0c..at + 0x10].copy_from_slice(&4096u32.to_le_bytes());
        disk[at + 0x10..at + 0x14].copy_from_slice(&1234u32.to_le_bytes());
        let d = read_directory(&disk, 63).expect("directory");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].tag, "rsrc", "the tag is a u32, so its bytes run backwards");
        assert_eq!(d[0].offset, 63 * 512 + 4096);
        assert_eq!(d[0].len, 1234);
    }

    /// The negative control: without the magic there is no directory, and saying so beats
    /// returning an empty list that reads as "this image has no images in it".
    #[test]
    fn a_disk_with_no_ata_magic_is_an_error_not_an_empty_list() {
        let disk = vec![0u8; 63 * 512 + 0x4400];
        assert!(read_directory(&disk, 63).is_err());
    }

    /// One file, one cluster, walked and read back — so the geometry arithmetic has a witness.
    #[test]
    fn a_one_file_volume_walks_and_reads_back() {
        let (bps, spc, rsvd, nfat, nroot, spf) = (512usize, 1usize, 1usize, 1usize, 16usize, 1usize);
        let mut v = vec![0u8; 64 * 512];
        v[11..13].copy_from_slice(&(bps as u16).to_le_bytes());
        v[13] = spc as u8;
        v[14..16].copy_from_slice(&(rsvd as u16).to_le_bytes());
        v[16] = nfat as u8;
        v[17..19].copy_from_slice(&(nroot as u16).to_le_bytes());
        v[22..24].copy_from_slice(&(spf as u16).to_le_bytes());
        let root = rsvd * bps + nfat * spf * bps;
        let data = root + nroot * 32;
        // FAT: cluster 2 terminates.
        let fat = rsvd * bps;
        v[fat + 3] = 0xff;
        v[fat + 4] = 0xff;
        // Root entry: VMCS.BIN, cluster 2, 4 bytes.
        v[root..root + 11].copy_from_slice(b"VMCS    BIN");
        v[root + 26..root + 28].copy_from_slice(&2u16.to_le_bytes());
        v[root + 28..root + 32].copy_from_slice(&4u32.to_le_bytes());
        v[data..data + 4].copy_from_slice(b"\xde\xad\xbe\xef");

        let f = Fat12::new(&v).expect("boot sector");
        let w = f.walk();
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].path, "VMCS.BIN");
        assert!(!w[0].is_dir);
        assert_eq!(f.read_chain(w[0].cluster, w[0].size), b"\xde\xad\xbe\xef");
    }
}
