//! Installing somebody else's operating system onto a drive image.
//!
//! **This is Apple's own arrangement, not an invention**, and it is what `ipodpatcher` does on real
//! hardware: the image goes into the firmware partition where the boot ROM already looks, and the
//! machine's own bootloader finds it. Nothing new boots, so a divergence afterwards belongs to the
//! operating system rather than to us — which is the whole reason this exists instead of the window
//! simply entering an image at `0x10000000`. See `docs/ideas/run-any-os.md`.
//!
//! It lives here rather than in `ipod-boot` because **the window needs it too**. Routing a dropped
//! `rockbox.ipod` to a verdict and then having nowhere to send it was the shape of the gap: the
//! file was identified, the message said "ready to install", and nothing could act on it.
//!
//! Both functions return their report as lines rather than printing, so the caller decides whether
//! that is stdout or a log in a window.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// The sector size the firmware directory's offsets are quantised to.
const FW_SECTOR: u32 = 512;
/// The `!ATA` directory, relative to the start of the firmware partition.
const FW_DIRECTORY: u64 = 0x4200;

/// One 40-byte record of the firmware directory, by the offsets the field names sit at.
struct DirEntry {
    tag: String,
    dev_offset: u32,
    len: u32,
    entry_offset: u32,
    chksum: u32,
}

/// A little-endian word, at a byte offset. `pub(crate)` so the tests that read a directory back
/// out of a built image use the same reader the writer used.
pub fn le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Install an operating system into a drive image's firmware partition, into a **new file**.
///
/// This is Apple's own arrangement and `ipodpatcher`'s recipe, not an invention: the image is
/// appended after the existing `osos`, the directory's entry point is moved to it, and the machine's
/// real bootloader finds it at the address it already looks at. Nothing new boots — the existing
/// cold path loads it — so a divergence afterwards belongs to the OS rather than to us.
///
/// **It never writes to the source.** Apple's `osos` is 7.21 MiB of software that can no longer be
/// downloaded, and an installer that edits in place is one mistake away from destroying the only
/// copy somebody has.
pub fn install_os(src: &Path, os: &Path, out: &Path) -> Result<Vec<String>, String> {
    let mut report: Vec<String> = Vec::new();
    if out == src {
        return Err("OUT.img must not be SRC.img — this never edits the source in place".into());
    }

    // The payload. A `.ipod` file is an 8-byte wrapper — big-endian checksum, then a 4-character
    // model id — over a raw ARM image (`tools/scramble.c`). Checking it is the difference between
    // installing a verified image and installing whatever the file happened to contain.
    let raw = std::fs::read(os).map_err(|e| format!("{}: {e}", os.display()))?;
    let payload = if raw.len() > 8 && raw[4..8].iter().all(|c| c.is_ascii_alphanumeric()) {
        let want = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]);
        let model: String = raw[4..8].iter().map(|&c| c as char).collect();
        let body = &raw[8..];
        // `scramble` seeds the sum with the model number; 5 is the Video's, per `modelnum`.
        let sum = body.iter().fold(5u32, |a, &b| a.wrapping_add(b as u32));
        if sum != want {
            return Err(format!(
                "{}: `.ipod` checksum does not match — header says {want:#010x}, the bytes sum to \
                 {sum:#010x}. The file is truncated or is for another model (`{model}`).",
                os.display()
            ));
        }
        report.push(format!("  {} — `{model}`, {} bytes, checksum OK", os.display(), body.len()));
        body.to_vec()
    } else {
        report.push(format!("  {} — {} bytes, raw (no `.ipod` header)", os.display(), raw.len()));
        raw
    };

    // The destination, as a copy. Sparse-aware on APFS and harmless elsewhere.
    std::fs::copy(src, out).map_err(|e| format!("copying {} -> {}: {e}", src.display(), out.display()))?;
    // **`copy` carries the source's mode, and the sources here are deliberately read-only.**
    // `drives/*.PRISTINE.img` is `chmod 444` so a bug cannot damage it, which meant installing FROM
    // it produced an output that could not be written and failed with a bare `Permission denied`
    // naming the destination — a file this command had just created itself. The same defect in
    // `clone_file` is why a fingerprint got measured on a mutable working copy for a day.
    if let Ok(md) = std::fs::metadata(out) {
        let mut perm = md.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perm.set_mode(perm.mode() | 0o600);
        }
        #[cfg(not(unix))]
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        let _ = std::fs::set_permissions(out, perm);
    }
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(out)
        .map_err(|e| format!("{}: {e}", out.display()))?;

    // Where the firmware partition is. Type 0x00 is Apple's, and it is the first entry.
    let mut mbr = [0u8; 512];
    f.seek(SeekFrom::Start(0)).and_then(|_| f.read_exact(&mut mbr)).map_err(|e| e.to_string())?;
    if mbr[510] != 0x55 || mbr[511] != 0xAA {
        return Err(format!("{}: no MBR signature — is this a drive image?", src.display()));
    }
    if mbr[446 + 4] != 0x00 {
        return Err(format!(
            "{}: partition 0 is type {:#04x}, not Apple's firmware partition (0x00)",
            src.display(),
            mbr[446 + 4]
        ));
    }
    let part = le(&mbr, 446 + 8) as u64 * 512;
    let part_sectors = le(&mbr, 446 + 12) as u64;
    // **Image data lives past the directory by a header, and that header is not one sector.**
    //
    // This used to read `part + 512`, citing `ipodpatcher.c:1586` (`fwoffset = start + sector_size`)
    // and the fact that a stock image's checksums reproduce over `[devOffset + 512, +len)`. Both
    // were true — of the 5G's bundles. Measured across all three, `osos` and `rsrc` reproduce at
    // **+0x200 on `iPod_13.1.3` and `iPod_20.1.3`, and +0x800 on `iPod_25.1.3`**, so a fixed sector
    // refused every 5.5G drive outright. It is discovered below, using `osos`'s own checksum as the
    // oracle — see the loop after the directory is read.

    // The directory.
    let mut dir = vec![0u8; FW_SECTOR as usize];
    f.seek(SeekFrom::Start(part + FW_DIRECTORY)).and_then(|_| f.read_exact(&mut dir)).map_err(|e| e.to_string())?;
    let mut images = Vec::new();
    for i in 0..(FW_SECTOR as usize / 40) {
        let r = &dir[i * 40..i * 40 + 40];
        if &r[0..4] != b"ATA!" && &r[0..4] != b"!ATA" {
            break;
        }
        images.push(DirEntry {
            tag: r[4..8].iter().rev().map(|&b| b as char).collect(),
            dev_offset: le(r, 0x0c),
            len: le(r, 0x10),
            entry_offset: le(r, 0x18),
            chksum: le(r, 0x1c),
        });
    }
    let first = images.first().ok_or("no images in the firmware directory")?;
    if first.tag != "osos" {
        return Err(format!("image 0 is `{}`, expected `osos`", first.tag));
    }
    // **Reproduce the checksums that are already there before writing new ones.** If our idea of
    // where an image starts or how it is summed is wrong, this fails here — on a file nobody has
    // modified — instead of producing a plausible image that the bootloader silently rejects
    // seventy ATA commands into a boot.
    //
    // **The header is discovered, not assumed**, and the checksum is the oracle for it. `devOffset`
    // is relative to the firmware PARTITION; the extracted `Firmware-…` file that gets written at
    // LBA 63 carries a header on top, and that header is **0x200 on the 5G's bundles and 0x800 on
    // the 5.5G's** — measured across `iPod_13.1.3`, `iPod_20.1.3` and `iPod_25.1.3` by finding
    // which offset reproduces `osos`'s recorded sum. A fixed 512 here refused every 5.5G drive.
    let header = {
        let mut found = None;
        for candidate in [0x200u64, 0x800, 0, 0x400, 0x1000] {
            let mut sum = 0u32;
            let mut left = first.len as usize;
            let mut buf = vec![0u8; 1 << 20];
            if f.seek(SeekFrom::Start(part + candidate + first.dev_offset as u64)).is_err() {
                continue;
            }
            let mut ok = true;
            while left > 0 {
                let n = left.min(buf.len());
                if f.read_exact(&mut buf[..n]).is_err() {
                    ok = false;
                    break;
                }
                sum = buf[..n].iter().fold(sum, |a, &b| a.wrapping_add(b as u32));
                left -= n;
            }
            if ok && sum == first.chksum {
                found = Some(candidate);
                break;
            }
        }
        found.ok_or_else(|| {
            format!(
                "`osos`'s checksum ({:#010x}) is not reproduced at any header offset this tool \n\
                 knows. Either the drive's firmware partition is damaged, or its bundle has a \n\
                 layout not seen before — and writing to it would make things worse either way.",
                first.chksum
            )
        })?
    };
    let fw = part + header;
    for img in &images {
        // **`aupd` is exempt, and that is measured rather than assumed.** Its recorded checksum
        // reproduces at NO offset in ANY of the three bundles — 13.1.3, 20.1.3 and 25.1.3 — so
        // whatever Apple sums for the updater, it is not the bytes at `devOffset`. Failing on it
        // means refusing every drive `make-disk` builds, which is a worse outcome than the one
        // this check exists to prevent.
        if img.tag == "aupd" {
            continue;
        }
        let mut sum = 0u32;
        let mut left = img.len as usize;
        let mut buf = vec![0u8; 1 << 20];
        f.seek(SeekFrom::Start(fw + img.dev_offset as u64)).map_err(|e| e.to_string())?;
        while left > 0 {
            let n = left.min(buf.len());
            f.read_exact(&mut buf[..n]).map_err(|e| e.to_string())?;
            sum = buf[..n].iter().fold(sum, |a, &b| a.wrapping_add(b as u32));
            left -= n;
        }
        if sum != img.chksum {
            return Err(format!(
                "`{}`: the checksum in the directory is {:#010x} but its bytes sum to {sum:#010x}.                  Either this image is already damaged, or this tool has the firmware layout wrong                  — and in both cases writing to it would make things worse.",
                img.tag, img.chksum
            ));
        }
    }
    report.push("  existing checksums reproduce — the layout is understood".to_string());
    report.push(format!(
        "  firmware partition at {part:#x}, {} image(s): {}",
        images.len(),
        images.iter().map(|i| i.tag.as_str()).collect::<Vec<_>>().join(" · ")
    ));

    // Where the new image goes. Re-installing over a previous one reuses the same slot rather than
    // pushing everything along again, which is what `entryOffset > 0` means.
    let align = |n: u32| (n + FW_SECTOR - 1) & !(FW_SECTOR - 1);
    let entry_offset =
        if first.entry_offset > 0 { first.entry_offset } else { align(first.len) };
    let length = payload.len() as u32;
    let padded = align(length);

    // Anything after `osos` has to move out of the way. The gap here is 512 bytes against a
    // 52 KB bootloader, so this is the normal case and not an edge one.
    let mut delta = 0u32;
    if let Some(next) = images.get(1) {
        let end = first.dev_offset + entry_offset + padded;
        if end > next.dev_offset {
            delta = end - next.dev_offset + FW_SECTOR;
            let last = images.last().unwrap();
            let needed = (last.dev_offset + align(last.len) + delta) as u64;
            if needed > part_sectors * 512 {
                return Err(format!(
                    "no room: moving the later images by {delta} bytes needs {needed} of a \
                     {}-byte partition",
                    part_sectors * 512
                ));
            }
            report.push(format!("  moving {} later image(s) on by {delta} bytes", images.len() - 1));
            // Backwards, so a shift never overwrites the source of a later block.
            for img in images[1..].iter().rev() {
                let n = align(img.len) as usize;
                let mut buf = vec![0u8; n];
                f.seek(SeekFrom::Start(fw + img.dev_offset as u64))
                    .and_then(|_| f.read_exact(&mut buf))
                    .map_err(|e| e.to_string())?;
                f.seek(SeekFrom::Start(fw + (img.dev_offset + delta) as u64))
                    .and_then(|_| f.write_all(&buf))
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    // The combined image, and its checksum over every byte of it.
    f.seek(SeekFrom::Start(fw + first.dev_offset as u64)).map_err(|e| e.to_string())?;
    let mut combined = vec![0u8; entry_offset as usize];
    f.read_exact(&mut combined).map_err(|e| e.to_string())?;
    combined.extend_from_slice(&payload);
    let chksum = combined.iter().fold(0u32, |a, &b| a.wrapping_add(b as u32));
    combined.resize((entry_offset + padded) as usize, 0);
    f.seek(SeekFrom::Start(fw + first.dev_offset as u64))
        .and_then(|_| f.write_all(&combined))
        .map_err(|e| e.to_string())?;

    // And the directory: image 0 gains the payload and points its entry at it; the rest follow
    // their data. `loadAddr` is cleared the way `ipodpatcher` clears it.
    dir[0x10..0x14].copy_from_slice(&(entry_offset + length).to_le_bytes());
    dir[0x18..0x1c].copy_from_slice(&entry_offset.to_le_bytes());
    dir[0x1c..0x20].copy_from_slice(&chksum.to_le_bytes());
    dir[0x24..0x28].copy_from_slice(&0xffff_ffffu32.to_le_bytes());
    if delta > 0 {
        for i in 1..images.len() {
            let at = i * 40 + 0x0c;
            let moved = le(&dir, at) + delta;
            dir[at..at + 4].copy_from_slice(&moved.to_le_bytes());
        }
    }
    f.seek(SeekFrom::Start(part + FW_DIRECTORY))
        .and_then(|_| f.write_all(&dir))
        .map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;

    report.push(format!(
        "  installed at +{entry_offset:#x}, {length} bytes, checksum {chksum:#010x}\n\
         {} — cold boot it and Apple's own bootloader will run it.",
        out.display()
    ));
    Ok(report)
}

/// Copy a local directory tree into the drive image's FAT32 volume.
///
/// **This modifies DISK.img in place**, unlike `install-os`, and says so before it starts. The
/// image it is meant for is one `install-os` just produced — a derived file — and copying 8 GB
/// again for every file added would be its own kind of hostile.
pub fn put_files(disk: &Path, src: &Path, dest: &str) -> Result<Vec<String>, String> {
    let mut report: Vec<String> = Vec::new();
    if !src.is_dir() {
        return Err(format!("{}: not a directory", src.display()));
    }

    let mut vol = crate::fat::Fat32::open(disk)?;
    let root = if dest.is_empty() { vol.root() } else { vol.mkdir_p(dest)? };
    report.push(format!("  {} — writing into {}", disk.display(), if dest.is_empty() { "/" } else { dest }));

    // Breadth-first, so a directory exists before anything is written into it.
    let mut queue = vec![(src.to_path_buf(), root)];
    let (mut files, mut bytes, mut dirs) = (0u64, 0u64, 0u64);
    while let Some((from, into)) = queue.pop() {
        let mut items: Vec<_> = std::fs::read_dir(&from)
            .map_err(|e| format!("{}: {e}", from.display()))?
            .filter_map(|e| e.ok())
            .collect();
        items.sort_by_key(|e| e.file_name());
        for item in items {
            let name = item.file_name().to_string_lossy().to_string();
            let path = item.path();
            let meta = std::fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            if meta.is_dir() {
                let sub = vol.mkdir(into, &name)?;
                dirs += 1;
                queue.push((path, sub));
            } else {
                let body = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                vol.write_file(into, &name, &body)?;
                files += 1;
                bytes += body.len() as u64;
            }
        }
    }
    vol.flush()?;
    report.push(format!("  {files} file(s) in {dirs} directory(ies), {bytes} bytes"));
    Ok(report)
}

/// Copy a zip's contents into the drive image's FAT32 volume.
///
/// This is what a Rockbox release is: `rockbox-ipodvideo-4.0.zip` holds a `.rockbox/` tree, and
/// Rockbox Utility's whole "install" is unpacking it onto the volume. So the archive goes straight
/// in — **no temporary directory** — which matters because the alternative is writing several
/// hundred files to somebody's disk twice, once to unpack and once to copy.
///
/// **Directories come from the paths, not from the archive's directory entries.** A zip may or may
/// not carry them, and Rockbox's does so inconsistently; deriving them from each member's own path
/// is the only version that works on both kinds.
pub fn put_zip(disk: &Path, zip: &Path) -> Result<Vec<String>, String> {
    let mut report: Vec<String> = Vec::new();
    let archive = crate::ipsw::Zip::open(zip)?;
    let mut vol = crate::fat::Fat32::open(disk)?;
    report.push(format!("  {} — into {}", zip.display(), disk.display()));

    let (mut files, mut bytes, mut dirs) = (0u64, 0u64, 0u64);
    // Sorted, so a parent is always made before its children and the run is reproducible.
    let mut members: Vec<&crate::ipsw::Member> = archive.members.iter().collect();
    members.sort_by(|a, b| a.name.cmp(&b.name));
    let mut made: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for m in members {
        // Zip paths are `/`-separated by specification, whatever wrote them.
        let name = m.name.replace('\\', "/");
        if name.ends_with('/') {
            continue; // A directory entry; the paths below create it when something needs it.
        }
        let Some((dir, leaf)) = name.rsplit_once('/') else {
            let body = archive.extract(m)?;
            vol.write_file(vol.root(), &name, &body)?;
            files += 1;
            bytes += body.len() as u64;
            continue;
        };
        let cluster = match made.get(dir) {
            Some(c) => *c,
            None => {
                let c = vol.mkdir_p(dir)?;
                made.insert(dir.to_string(), c);
                dirs += 1;
                c
            }
        };
        let body = archive.extract(m)?;
        vol.write_file(cluster, leaf, &body)?;
        files += 1;
        bytes += body.len() as u64;
    }
    vol.flush()?;
    report.push(format!("  {files} file(s) in {dirs} directory(ies), {bytes} bytes"));
    Ok(report)
}
