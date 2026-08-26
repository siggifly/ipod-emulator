//! Synthesising a boot ROM, so nobody has to find one.
//!
//! A real 5G NOR dump is per-unit and Apple never served one; the only way to get yours is to read
//! it off your own iPod, and [issue #2] is somebody stuck on exactly that. This module builds one
//! instead — from a model, a colour and an identity, all of which are settings rather than files.
//!
//! ## What "synthetic" means here, and what it does not
//!
//! **It does not mean a megabyte of generated ARM code.** Apple's boot ROM's observable effect is a
//! handful of things — SDRAM brought up, a `sysinfo_t` handoff block written into IRAM, the OS
//! copied to `0x10000000`, and a jump — and those are done in Rust, at a higher level, rather than
//! by executing instructions out of a synthesised image. See [ROADMAP] M5.
//!
//! What this module produces is the **data** half: a 1 MiB image carrying a real `SysCfg` block, so
//! that everything which later reads the flash for an identity — RetailOS's About screen, anything
//! binding to the FireWire GUID — finds one.
//!
//! ## It says that it is synthetic, on purpose
//!
//! [`SYNTH_MARK`] sits at [`SYNTH_MARK_AT`], and [`is_synthetic`] finds it. A generated ROM that
//! could pass as a dump is a generated ROM that will eventually circulate as one — somebody's
//! "5.5G bootrom" that is actually this program's invention, muddying a pool of dumps that is
//! already small and already mislabelled (`research/16` records the Video's dump filed under the
//! Classic's model number).
//!
//! **The mark goes here and not in the serial.** The serial is meant to pass for real — that is
//! most of why [`crate::identity::Identity::generate`] exists — so the image carries the
//! disclosure instead, where it costs nothing.
//!
//! [issue #2]: https://github.com/siggifly/ipod-emulator/issues/2
//! [ROADMAP]: ../../../ROADMAP.md

use crate::identity::{Identity, Model};
use crate::inspect::{SysCfgBuilder, NOR_LEN, SYSCFG_AT};

/// Where [`SYNTH_MARK`] is written.
///
/// `0x40` is the first byte after the ARM exception vector table, which occupies `0x00..0x40` and
/// is the only part of the low image a PP502x cares about. Apple's ROM has code here; ours has a
/// sentence saying what the file is.
pub const SYNTH_MARK_AT: usize = 0x40;

/// The mark that says this image was generated rather than read off an iPod.
pub const SYNTH_MARK: &[u8] = b"ipod-emulator synthetic NOR v1\n";

/// The reset vector Apple's retail 5G carries: `B +0x8000`.
///
/// Written so the image is a well-formed ARM boot image rather than a file that merely happens to
/// be a megabyte — [`crate::inspect::flash`] checks word 0 for a branch, and a synthetic ROM should
/// pass the same check a real one does. **Nothing executes from it**: the boot is emulated at a
/// higher level, and this word is there to make the file honest, not to be run.
pub const RESET_VECTOR: u32 = 0xea00_1ffe;

/// What to put in a synthesised ROM.
#[derive(Clone, Debug)]
pub struct Spec {
    /// Serial and FireWire GUID. Generated, provided, or read off the user's own hardware.
    pub identity: Identity,
    /// Which iPod this claims to be. Supplies `Mod#`, and its generation supplies `HwVr`.
    pub model: &'static Model,
    /// `HwId`. **Meaning unknown** — the reference unit reads `0x8201763A` and nothing we have
    /// documents what it is. Carried forward when a source is available and left `0` otherwise,
    /// because inventing a value for a field nobody understands is how a wrong fact gets a number.
    pub hw_id: u32,
    /// `Regn`, raw. The reference unit reads `01 00 02 00  01 00 02 00`, which is a region code of
    /// some kind. Copied rather than understood; it is not identity, so copying it is safe.
    pub region: [u8; 16],
    /// `DrmV`. The reference unit reads `6`.
    pub drm_version: u32,
    /// Force `HwVr` instead of taking it from the generation.
    ///
    /// **A bisect control, not a setting.** The 5G's Gestalt is measured; the 5.5G's `0x000B0010`
    /// is the one value in this whole system that came from a comment rather than from hardware.
    /// Being able to vary exactly that, with nothing else moving, is how it gets isolated.
    ///
    /// **It has now been isolated, and it is not the reason a 5.5G drive does not boot.** This
    /// doc used to say "and the 5.5G does not boot" in the same breath, which read as a link
    /// there was never any evidence for. Two measurements, 2026-08-26, in research/17:
    ///
    /// - Over a drive built from `iPod_25.1.3`, `0x000B0010` and `0x000B0011` give the same run
    ///   to the code bucket. `cmp -l` proves the two ROMs differ in exactly one byte.
    /// - Apple's own retail 5G dump, with `HwVr` patched from `0x000B0005` to `0x000B0010` —
    ///   one byte at flash `0x405c`, `cmp -l` against the untouched dump proving it — boots that
    ///   drive to **the same 70 ATA commands and the same 71 695 lit pixels** as unpatched, and
    ///   still boots a 20.1.3 drive to 617. The Gestalt moved and nothing else did.
    ///
    /// Whatever RetailOS pairs a drive against, it is not this word.
    pub hw_vr: Option<u32>,
}

impl Spec {
    /// A spec for `model` with `identity`, carrying the reference unit's values for the three
    /// records whose meaning we have not established.
    pub fn new(model: &'static Model, identity: Identity) -> Spec {
        let mut region = [0u8; 16];
        region[..8].copy_from_slice(&[0x01, 0x00, 0x02, 0x00, 0x01, 0x00, 0x02, 0x00]);
        Spec {
            identity,
            model,
            hw_id: 0,
            region,
            drm_version: 6,
            hw_vr: None,
        }
    }

    /// Take the unexplained records from a real dump, so a ROM synthesised alongside real hardware
    /// carries that hardware's values rather than a default.
    ///
    /// This is what "generate one matching my iPod" does: the identity comes from the user, and
    /// everything nobody understands comes from the machine it is standing in for.
    pub fn carry_from(mut self, source: &crate::inspect::SysCfg) -> Spec {
        for (tag, payload) in &source.records {
            match tag.as_str() {
                "HwId" => {
                    self.hw_id = u32::from_le_bytes(payload[..4].try_into().unwrap_or([0; 4]))
                }
                "Regn" => self.region = *payload,
                "DrmV" => {
                    self.drm_version =
                        u32::from_le_bytes(payload[4..8].try_into().unwrap_or([0; 4]))
                }
                _ => {}
            }
        }
        self
    }
}

/// Build the image.
///
/// The record order matches the reference dump's — `SrNm FwId HwId HwVr Regn Mod# DrmV` — because
/// there is no reason to differ and every reason for a diff against a real dump to be short.
pub fn synthesise(spec: &Spec) -> Vec<u8> {
    let mut nor = vec![0u8; NOR_LEN as usize];
    nor[..4].copy_from_slice(&RESET_VECTOR.to_le_bytes());
    nor[SYNTH_MARK_AT..SYNTH_MARK_AT + SYNTH_MARK.len()].copy_from_slice(SYNTH_MARK);

    let mut b = SysCfgBuilder::new();
    if let Some(sn) = spec.identity.serial.as_deref() {
        b = b.text("SrNm", sn);
    }
    b = b.guid(spec.identity.guid);
    b = b.word_at0("HwId", spec.hw_id);
    // `HwVr` is the generation's Gestalt ID. `None` for a generation whose constant we have not
    // sourced, and in that case the record is left out rather than filled with a guess — an absent
    // record is readable as "unknown", a wrong one is not.
    if let Some(hw_vr) = spec.hw_vr.or_else(|| spec.model.generation.gestalt()) {
        b = b.word_at4("HwVr", hw_vr);
    }
    b = b.raw("Regn", spec.region);
    // `Mod#` in the form the flash writes it — `MA146`, the full model number. The table key has
    // a letter stripped for lookup and is not what the hardware stores; the drive's `SysInfo` adds
    // a further `x` on top again. Measured from the real dump.
    b = b.text("Mod#", &spec.model.apple_number());
    b = b.word_at4("DrmV", spec.drm_version);

    let block = b.build();
    nor[SYSCFG_AT..SYSCFG_AT + block.len()].copy_from_slice(&block);

    // **The `flsh` image directory, and one image in it.**
    //
    // Without this the file has a plausible reset vector and no directory at
    // [`crate::inspect::NOR_DIRECTORY`], which is the one thing [`crate::inspect::flash`] refuses
    // outright — so this program's own generated ROM came back from this program's own inspector
    // as `Wrong`, and `ipod-boot flsh` said it *"has no `flsh` image directory at all"*.
    //
    // **One record, not four.** A retail NOR indexes `disk`, `diag`, `logo` and `vmcs`; three of
    // those are Apple's programs and a generated ROM cannot carry them. Naming an image that is
    // not in the file would move the failure rather than fix it — `emu.rs` would find a `diag`
    // record, cut zeros out of it, and report *"`diag` is data, not a program"* about a machine
    // that never had one. So the directory says what the file contains: a boot logo, which we can
    // draw ourselves, at the address every 5G/5.5G image loads at.
    let logo = logo_image(&mark_tile());
    nor[LOGO_AT..LOGO_AT + logo.len()].copy_from_slice(&logo);
    write_image_record(&mut nor, 0, "logo", LOGO_AT as u32, &logo);
    nor
}

/// One 40-byte `flsh` record, written into the directory at slot `slot`.
///
/// The layout is [`crate::inspect::Entry`]'s, and the two fields left zero are left zero on
/// purpose. `version` reads `0x0000b012` on both dumps in this repository and `loadAddr` reads two
/// different values on the two of them — so one is a constant nobody here has explained and the
/// other is per-build, and inventing either for our own image would be writing a fact we do not
/// have. Nothing in this program reads either field.
///
/// `checksum` **is** written, because it is the one field whose correct value is derivable: all
/// nine images across both dumps verify as a plain byte sum of the body (`research/07`), and
/// `install.rs` reproduces that sum before it will touch a firmware partition.
fn write_image_record(nor: &mut [u8], slot: usize, tag: &str, at: u32, body: &[u8]) {
    let rec = crate::inspect::NOR_DIRECTORY as usize + slot * crate::inspect::IMAGE_RECORD;
    // The magic and the tag are both stored as little-endian u32s of four characters, so both go
    // in backwards: `flsh` is `hslf` on disk and `logo` is `ogol`.
    nor[rec..rec + 4].copy_from_slice(b"hslf");
    let backwards: Vec<u8> = tag.bytes().rev().collect();
    nor[rec + 4..rec + 8].copy_from_slice(&backwards);
    let mut put = |off: usize, v: u32| nor[rec + off..rec + off + 4].copy_from_slice(&v.to_le_bytes());
    put(0x08, 0); // dev — 0 until Apple's flash updater marks `aupd` done, and we have no `aupd`
    put(0x0c, at);
    put(0x10, body.len() as u32);
    put(0x14, crate::inspect::LOAD_ADDR_5G);
    put(0x18, 0); // entryOffset — 0 on all nine images of both dumps
    put(0x1c, body.iter().fold(0u32, |a, &b| a.wrapping_add(b as u32)));
}

/// The `sysinfo_t` handoff block Apple's boot ROM leaves in IRAM for the OS, **as measured**.
///
/// Captured from a real cold boot at the instant of handoff (`--stop-at=0x10000000:1`), not
/// reconstructed from documentation. Apple puts the block at `0x40015898` and writes a tag and a
/// pointer to it at the top of IRAM:
///
/// ```text
/// 0x4001ff18  "IsyS"
/// 0x4001ff1c  -> the block
/// ```
///
/// ## The layout, and which parts are understood
///
/// | offset | what | source |
/// |---|---|---|
/// | `+0x00` | `IsyS` | measured |
/// | `+0x04` | `len` = **`0xf8`** | measured — and load-bearing, see below |
/// | `+0x08` | `BoardHwName[16]` = `"iPod M25"` | measured |
/// | `+0x18` | `pszSerialNumber[32]` | measured |
/// | `+0x38` | GUID: low word then high, as `FwId` stores it | measured |
/// | `+0x84` | the Gestalt ID RetailOS switches on | measured; equals the NOR's `HwVr` |
/// | `+0x88` | `"1.00    "` | measured, meaning unknown |
/// | `+0x98` | the model number, e.g. `MA146` | measured |
/// | `+0xe0`… | four words that look like bases and sizes | measured, **not** understood |
/// | `+0xf8` | the whole `SysCfg` block, copied verbatim | measured |
///
/// **`len` is `0xf8` and that matters.** `research/16` records that `ipodloader2` reads `hw_rev`
/// from one field when `len == 0xf8` and from another otherwise, so a wrong length there is not
/// cosmetic — it sends a third-party bootloader to the wrong offset.
///
/// Everything not understood is reproduced byte for byte from the capture rather than zeroed or
/// invented. A field nobody has explained is still a field the firmware may read.
pub const HANDOFF_AT: u32 = 0x4001_5898;
/// Where the tag and pointer live, at the top of IRAM.
pub const HANDOFF_TAG_AT: u32 = 0x4001_ff18;
/// `sizeof(sysinfo_t)` as Apple's own boot ROM reports it.
pub const HANDOFF_LEN: usize = 0xf8;
/// The board name the Video's boot ROM writes. Observed on the retail 5G, and independently in a
/// 5.5G's `SysInfo` recovered from a Windows install — so it is the family's name, not one unit's.
pub const BOARD_HW_NAME: &str = "iPod M25";

/// Build the handoff block for an identity, ready to be written at [`HANDOFF_AT`].
///
/// `syscfg` is the block as it appears in the NOR; Apple copies it in directly after the struct,
/// so a synthesised boot passes through the same bytes a real one would.
pub fn handoff(identity: &Identity, model: &Model, syscfg: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; HANDOFF_LEN];
    let put = |b: &mut [u8], off: usize, v: u32| {
        b[off..off + 4].copy_from_slice(&v.to_le_bytes());
    };
    let text = |b: &mut [u8], off: usize, s: &str, max: usize| {
        let n = s.len().min(max);
        b[off..off + n].copy_from_slice(&s.as_bytes()[..n]);
    };

    b[..4].copy_from_slice(b"IsyS");
    put(&mut b, 0x04, HANDOFF_LEN as u32);
    text(&mut b, 0x08, BOARD_HW_NAME, 16);
    if let Some(sn) = identity.serial.as_deref() {
        text(&mut b, 0x18, sn, 32);
    }
    // Low word then high, the same order `FwId` uses in the flash.
    put(&mut b, 0x38, (identity.guid & 0xffff_ffff) as u32);
    put(&mut b, 0x3c, (identity.guid >> 32) as u32);

    // Measured constants. Their meaning is not established, and reproducing what the hardware does
    // beats leaving a field the firmware might read at zero.
    put(&mut b, 0x80, 0xfff9_f3b6);
    if let Some(hw_vr) = model.generation.gestalt() {
        put(&mut b, 0x84, hw_vr);
    }
    text(&mut b, 0x88, "1.00    ", 8);
    put(&mut b, 0x90, 0x0001_0000);
    put(&mut b, 0x94, 0x0000_0002);
    text(&mut b, 0x98, &model.apple_number(), 16);
    put(&mut b, 0xd0, 0x0006_0000);
    put(&mut b, 0xe0, 0x0400_0000);
    put(&mut b, 0xe4, 0x1000_0000);
    put(&mut b, 0xe8, 0x0002_0000);
    put(&mut b, 0xec, 0x4000_0000);
    put(&mut b, 0xf0, 0x0010_0000);

    // Apple copies the SysCfg in immediately after the struct; so do we.
    b.extend_from_slice(syscfg);
    b
}

// ── The boot logo ───────────────────────────────────────────────────────────────────────────────

/// The rectangle Apple's own boot logo occupies, blitted dead centre of the 320×240 panel —
/// `(129,81)`..`(190,158)` inclusive, measured in `research/14`.
pub const LOGO_W: usize = 62;
/// The tall side of that rectangle.
pub const LOGO_H: usize = 78;

/// The header on a `logo` image, ahead of `LOGO_W × LOGO_H` RGB565 halfwords.
///
/// **Measured, and measured twice.** The retail 5G's `flsh` `logo` record and the prototype's are
/// byte-identical images at different offsets — same 9 700 bytes, same checksum — so this is the
/// family's container and not one build's:
///
/// ```sh
/// # retail, from the offset research/07's directory table gives for `logo`
/// xxd -s $((0xb97c4)) -l 32 resources/roms/retail_5g_*_internal_rom_000000-0FFFFF.bin
/// # the prototype, same image, different address
/// xxd -s $((0x879f8)) -l 32 resources/archive-downloads/internal_rom_000000-0FFFFF.bin
/// ```
///
/// | off | value | meaning |
/// |---|---|---|
/// | `+0x00` | `6f 47 6f 4c` | `LoGo`, stored as a little-endian u32 of four characters — the same backwards spelling the `flsh` directory uses for its tags |
/// | `+0x08` | `0x004e` | height, 78 |
/// | `+0x0a` | `0x003e` | width, 62 |
/// | `+0x0c` | `0x007c` | the row stride in bytes, 62 × 2 |
/// | `+0x0e` | `0x0034` | **unidentified**, and reproduced rather than understood |
/// | `+0x14` | `0x25c8` | 9 672 = 62 × 78 × 2, the payload that follows the header |
///
/// Everything else in the 28 bytes is zero on both dumps.
///
/// `+0x0e` is worth one more sentence, because it turns up somewhere else. `research/14` reads the
/// eight-word rect header Apple's bootloader stages at `BCMA_CMDPARAM` and calls its word 0 —
/// `0x00000034` — *"unidentified. Constant across both commands of a retail boot."* It is the same
/// number, and it is in the image's own header one step upstream. That is an agreement between two
/// measurements, not an explanation of either: neither says what the field means.
pub const LOGO_HEADER: usize = 28;

/// Where [`synthesise`] puts the `logo` body it writes.
///
/// Chosen, not measured — the retail dump has its at `0xb97c4` and the prototype at `0x879f8`, both
/// of which are where somebody's linker happened to put it. `0xf0000` is the last 64 KiB before the
/// directory at [`crate::inspect::NOR_DIRECTORY`], clear of the reset vector, [`SYNTH_MARK`] and the
/// `SysCfg` block by three quarters of the chip.
pub const LOGO_AT: usize = 0x000f_0000;

/// The project's own mark, as a `LOGO_W × LOGO_H` tile of RGB565.
///
/// **A filled ring with a sheen, not an outline.**
///
/// Apple's logo — extracted from a real boot and kept at `resources/derived/logo/` — is a solid,
/// shaded shape. A two-pixel stroke next to it reads as a wireframe: the right silhouette with
/// none of the weight. So the wheel is filled between its two radii and lit from above, which is
/// also what the physical part looks like.
///
/// Both edges are anti-aliased by coverage. A hard `d <= r` test stair-steps, and at 62 pixels
/// across those steps are a third of the mark's apparent line weight.
///
/// **The colours are the hardware's, not a choice — and they do not follow the case.** Every iPod
/// with video boots a white logo on black, whatever colour its case is. This took the case colour
/// as an argument until 2026-08-19 and inverted for a white one, on the belief that a white 5G
/// booted a dark logo on a white screen. It does not — corrected by the operator, who owned one.
/// The boot screen belongs to the *firmware*, and Apple shipped one firmware for both cases; the
/// U2's red is the wheel, not the case and not this.
pub fn mark_tile() -> Vec<u16> {
    // RGB565. The panel is 16-bit, and 0 is black, so a white mark has to be written.
    const WHITE: u16 = 0xffff;
    const BLACK: u16 = 0x0000;
    let (bg, fg) = (BLACK, WHITE);
    let (w, h) = (LOGO_W, LOGO_H);

    let mut tile = vec![bg; w * h];
    let cx = (w as f32) / 2.0;
    let cy = (h as f32) / 2.0;
    // The wheel is round, so the short side bounds it.
    let outer = (w.min(h) as f32) / 2.0 - 1.0;
    // The same proportion the window draws the real wheel at: the centre button is 0.34 of the
    // wheel's radius.
    let inner = outer * 0.34;

    let blend = |a: u16, b: u16, t: f32| -> u16 {
        let t = t.clamp(0.0, 1.0);
        let ch = |v: u16, sh: u16, m: u16| ((v >> sh) & m) as f32;
        let (ar, ag, ab) = (ch(a, 11, 0x1f), ch(a, 5, 0x3f), ch(a, 0, 0x1f));
        let (br, bg2, bb) = (ch(b, 11, 0x1f), ch(b, 5, 0x3f), ch(b, 0, 0x1f));
        let mix = |x: f32, y: f32| (x + (y - x) * t).round();
        ((mix(ar, br) as u16) << 11) | ((mix(ag, bg2) as u16) << 5) | (mix(ab, bb) as u16)
    };
    // Dim the foreground towards the background, for the gradient.
    let shade = |c: u16, k: f32| blend(bg, c, k);

    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            // Inside the outer edge, outside the inner one — each with a one-pixel soft band.
            let cover = ((outer + 0.5 - d).clamp(0.0, 1.0)) * ((d - inner + 0.5).clamp(0.0, 1.0));
            if cover <= 0.0 {
                continue;
            }
            // Lit from above: full strength at the top of the wheel, easing to about half at the
            // bottom. The same direction Apple's logo is lit.
            let t = ((dy + outer) / (2.0 * outer)).clamp(0.0, 1.0);
            let lit = shade(fg, 1.0 - 0.45 * t);
            tile[y * w + x] = blend(tile[y * w + x], lit, cover);
        }
    }
    tile
}

/// A `LOGO_W × LOGO_H` tile wrapped in the container [`LOGO_HEADER`] describes, ready to be
/// indexed by a `flsh` record.
pub fn logo_image(tile: &[u16]) -> Vec<u8> {
    assert_eq!(
        tile.len(),
        LOGO_W * LOGO_H,
        "a `logo` image is exactly {LOGO_W}x{LOGO_H}; this tile is {} pixels",
        tile.len()
    );
    let mut out = vec![0u8; LOGO_HEADER + tile.len() * 2];
    out[..4].copy_from_slice(b"oGoL");
    out[8..10].copy_from_slice(&(LOGO_H as u16).to_le_bytes());
    out[10..12].copy_from_slice(&(LOGO_W as u16).to_le_bytes());
    out[12..14].copy_from_slice(&((LOGO_W * 2) as u16).to_le_bytes());
    out[14..16].copy_from_slice(&0x0034u16.to_le_bytes());
    out[0x14..0x18].copy_from_slice(&((tile.len() * 2) as u32).to_le_bytes());
    for (i, px) in tile.iter().enumerate() {
        let at = LOGO_HEADER + i * 2;
        out[at..at + 2].copy_from_slice(&px.to_le_bytes());
    }
    out
}

/// The tile back out of a `logo` image — width, height, pixels.
///
/// `None` for anything that is not one: a different tag, a header whose stated payload disagrees
/// with its own dimensions, or an image shorter than it claims. **The length word is what makes
/// this a parse rather than a guess** — a wrong four bytes would have to coincidentally satisfy a
/// byte count the same header states in a fifth field.
pub fn logo_tile(image: &[u8]) -> Option<(usize, usize, Vec<u16>)> {
    if image.get(..4)? != b"oGoL" {
        return None;
    }
    let at16 = |at: usize| -> Option<usize> {
        image
            .get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
    };
    let h = at16(8)?;
    let w = at16(10)?;
    let len = u32::from_le_bytes(image.get(0x14..0x18)?.try_into().ok()?) as usize;
    if w == 0 || h == 0 || len != w * h * 2 {
        return None;
    }
    let px = image.get(LOGO_HEADER..LOGO_HEADER + len)?;
    Some((
        w,
        h,
        px.chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect(),
    ))
}

/// A black `w`×`h` panel with `tile` centred on it.
///
/// The centring **is** the placement the hardware uses: 320 and 240 against 62 and 78 give
/// `(129, 81)`, which is the `x0`/`y0` `research/14` read out of the bootloader's own rect header.
/// Anything of the tile that would fall off the panel is dropped rather than wrapped, so a panel
/// smaller than the tile draws the middle of it instead of panicking.
fn panel_with(w: usize, h: usize, tw: usize, th: usize, tile: &[u16]) -> Vec<u16> {
    let mut fb = vec![0u16; w * h];
    let ox = (w as isize - tw as isize) / 2;
    let oy = (h as isize - th as isize) / 2;
    for y in 0..th {
        for x in 0..tw {
            let (px, py) = (ox + x as isize, oy + y as isize);
            if px < 0 || py < 0 || px >= w as isize || py >= h as isize {
                continue;
            }
            fb[py as usize * w + px as usize] = tile[y * tw + x];
        }
    }
    fb
}

/// The boot screen a synthesised iPod shows, as RGB565 pixels for a `w`×`h` panel.
///
/// A real NOR carries a `logo` image and Apple's bootloader blits it. A synthesised one **now
/// carries one too** — [`synthesise`] writes [`mark_tile`] into it — and it could not carry
/// Apple's if it wanted to: that artwork is Apple's, and a generated ROM handing it out is a
/// generated ROM redistributing it. So the mark goes in instead, a click wheel outline, which is
/// the iPod's most recognisable shape and is not a trademark.
///
/// This draws the same tile straight onto the panel rather than reading it back out of a megabyte
/// it would have to build first. The two routes are held to each other by
/// `the_screen_a_synthesised_ipod_shows_is_the_image_in_its_own_rom`.
pub fn boot_screen(w: usize, h: usize) -> Vec<u16> {
    panel_with(w, h, LOGO_W, LOGO_H, &mark_tile())
}

/// The same screen, with a supplied image in place of the click wheel.
///
/// The image is fitted to the 62×78 rectangle Apple's own logo occupies — aspect preserved, box
/// filtered — and then **painted as a mask rather than pasted as a picture**.
///
/// ## Why a mask
///
/// The boot logo on this hardware is monochrome — white ink on black, on every case, per
/// [`mark_tile`]. The tile extracted from a real boot is therefore white artwork, and a picture
/// somebody supplies may be either polarity: a black-on-white drawing exported from anything is
/// the same shape with the ink and the paper swapped.
///
/// So the image's **luminance becomes coverage** and the firmware's own foreground supplies the
/// colour. One source image is then correct whichever way round it was drawn, which is what a
/// person supplying "a logo" means. It also means the extracted Apple tile works without anyone
/// having to invert it first.
///
/// The cost is that a colour image renders monochrome. That is the panel's own behaviour for this
/// image and not a limitation worth working around — the boot logo was never in colour.
///
/// **What somebody supplies is their business.** If they have extracted Apple's logo from a dump
/// they own and want to use it, that is a decision about their own files.
pub fn boot_screen_with(w: usize, h: usize, img: &crate::splash::Image) -> Vec<u16> {
    panel_with(w, h, LOGO_W, LOGO_H, &mask_tile(img))
}

/// A supplied image as a `LOGO_W × LOGO_H` tile, its luminance taken as ink coverage.
fn mask_tile(img: &crate::splash::Image) -> Vec<u16> {
    const WHITE: u16 = 0xffff;
    const BLACK: u16 = 0x0000;
    let (bg, fg) = (BLACK, WHITE);

    let mut tile = vec![bg; LOGO_W * LOGO_H];
    let (px, mask) = crate::splash::fit(img, LOGO_W, LOGO_H);
    for i in 0..LOGO_W * LOGO_H {
        if !mask[i] {
            continue;
        }
        let p = px[i];
        // Luminance of the RGB565 sample, 0..1. Rec. 601 weights, which is what a person reads
        // as "how bright is this pixel".
        let r = ((p >> 11) & 0x1f) as f32 / 31.0;
        let g = ((p >> 5) & 0x3f) as f32 / 63.0;
        let b = (p & 0x1f) as f32 / 31.0;
        let cover = (0.299 * r + 0.587 * g + 0.114 * b).clamp(0.0, 1.0);

        let ch = |v: u16, sh: u16, m: u16| ((v >> sh) & m) as f32;
        let mix = |x: f32, y: f32| (x + (y - x) * cover).round();
        tile[i] = ((mix(ch(bg, 11, 0x1f), ch(fg, 11, 0x1f)) as u16) << 11)
            | ((mix(ch(bg, 5, 0x3f), ch(fg, 5, 0x3f)) as u16) << 5)
            | (mix(ch(bg, 0, 0x1f), ch(fg, 0, 0x1f)) as u16);
    }
    tile
}

/// Whether this image was made by [`synthesise`] rather than read off an iPod.
pub fn is_synthetic(nor: &[u8]) -> bool {
    nor.get(SYNTH_MARK_AT..SYNTH_MARK_AT + SYNTH_MARK.len()) == Some(SYNTH_MARK)
}

/// The iPod this program makes when nobody has said which — a 30 GB **white** 5.5G.
///
/// **The only model number written in this program.** [`Source::default`] reads it, and so does
/// `compose::FIRST_RUN_MODEL`, so the three cannot come to describe different machines.
///
/// **`A444` is not a number anybody here made up.** It is a row of
/// [`crate::models::MODELS`] — the table transcribed from libgpod's `ipod_model_table`
/// (`src/itdb_device.c`) and corroborated against Apple's published *Identify your iPod model*
/// pages — where it reads 30 GB, `IpodModel::VideoWhite`, `Generation::Video2`. It is the white
/// peer of `A446` in the same row of that table's 5.5G block: same capacity, same generation, same
/// board, different case. `research/16` §"The table, from libgpod" prints the block, and its
/// 5.5G line reads `A444 / A446 | 30 GB | white / black | VIDEO_2` — one row, two colours.
///
/// **The generation is the decision the operator made; the colour is not.** `ROADMAP.md`
/// §"5G, 5.5G, and which is the default" settles 5.5G and says nothing about the case, so moving
/// from `A446` to `A444` moves the case alone — and the case is the half a person sees first.
///
/// **The model number is part of the identity, so this is not a cosmetic constant.**
/// [`crate::identity::Identity::generate`] mixes `model.number` into the seed, so `A444` and `A446`
/// at one seed are two different iPods with two different serials and two different FireWire GUIDs.
/// An iPod already minted under the old default keeps the one it was minted with; nothing here
/// reaches back.
pub const DEFAULT_MODEL: &str = "A444";

/// A seed nobody chose, for an identity that is minted **once** and is then permanent.
///
/// **The one irreversible call in this program.** [`crate::identity::Identity::generate`] is a pure
/// function of a model and this number, and the 8-byte FireWire GUID it produces is what `sysinfo_t`
/// carries and what iTunes binds DRM to — so the seed *is* the iPod. Three failed first runs must
/// leave one iPod with one GUID, which means this is called once, at the first press, and never
/// again while a synthesised ROM for that device exists.
///
/// **No dependency**: [`std::collections::hash_map::RandomState`] is std's own OS-seeded hasher key.
/// The process id and the clock are mixed in because two `RandomState`s in one process share a
/// thread-local base key that increments by one — hashing the same value twice would otherwise
/// produce two numbers that differ by a known amount, which is not the property wanted here.
///
/// **Never made deterministic.** [`crate::identity::Identity::generate`]'s contract is *same seed,
/// same iPod, every launch*; a test that pinned this would be asserting that two people who each
/// press the button get the same iPod.
///
/// `0` comes back as `1`: [`Source::default`] is `seed: 0`, and a minted identity indistinguishable
/// from the never-chosen default is the one value that must not come out of a mint.
pub fn mint_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u32(std::process::id());
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    // A second RandomState, because the first one's key is per-thread and increments by one between
    // two instances — so on its own it is a counter with a random start, not a random number.
    h.write_u64(std::collections::hash_map::RandomState::new().hash_one("ipod"));
    match h.finish() {
        0 => 1,
        n => n,
    }
}

/// Where a boot ROM comes from.
///
/// **Synthetic is not a file.** The image is a pure function of a model, a seed and any overrides,
/// so what gets persisted is that recipe and not the megabyte it produces. Storing the artifact
/// would buy a cache to manage, files to clean up, and a stale image every time this module's
/// output changes; storing the recipe costs a few bytes in the settings file and regenerates in
/// microseconds.
///
/// It also removes the only way a generated ROM could be mistaken for a dump: there is nothing on
/// disk to mistake. [`SYNTH_MARK`] still goes into any image that *is* written out, via
/// `ipod-boot make-nor`, which exists for exporting one — to inspect, to hand to another tool, or
/// to attach to a bug report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A dump the user supplied. Read as-is.
    File(std::path::PathBuf),
    /// Built on demand.
    Synthetic {
        /// A model number in any written form — `A146`, `MA146`, `xMA146`.
        model: String,
        /// The seed. Persisted, so the same machine comes back on the next launch.
        seed: u64,
        /// An identity typed or edited by the user, overriding what the seed would produce.
        serial: Option<String>,
        guid: Option<u64>,
        /// An image to show while booting, in place of the click wheel.
        ///
        /// A path rather than the pixels, for the same reason the identity is a recipe: it is a few
        /// bytes in the settings file, it re-reads if the user edits the picture, and there is
        /// nothing to go stale. Absent means the built-in mark.
        splash: Option<std::path::PathBuf>,
    },
}

impl Default for Source {
    /// A 30 GB white **5.5G** — the newest generation, per the operator decision recorded in
    /// [ROADMAP] §"5G, 5.5G, and which is the default", in the case [`DEFAULT_MODEL`] names.
    ///
    /// Synthesis is what makes that default honest. It was previously blocked on this project
    /// owning exactly one dump, which is a 5G: defaulting to a machine we could not produce would
    /// have been a setting that did not work. A generated ROM is whichever model is asked for.
    ///
    /// The reference hardware every measurement in `research/` was taken on is still the 5G
    /// (`A146`), and a checkout that has the dump still boots it — see the source resolution in
    /// the window's argument parsing.
    fn default() -> Source {
        Source::Synthetic {
            model: DEFAULT_MODEL.into(),
            seed: 0,
            serial: None,
            guid: None,
            splash: None,
        }
    }
}

impl Source {
    /// The bytes, read or built.
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        match self {
            Source::File(p) => std::fs::read(p).map_err(|e| format!("{}: {e}", p.display())),
            Source::Synthetic {
                model,
                seed,
                serial,
                guid,
                ..
            } => {
                let m = Model::lookup(model)
                    .ok_or_else(|| format!("{model} is not a model number this program knows"))?;
                let identity = match guid {
                    // A typed identity wins over the seed's, and is marked `Provided` because we
                    // cannot tell by looking whether it is a real device's.
                    Some(g) => Identity::provided(serial.as_deref(), *g)?,
                    None => Identity::generate(m, *seed),
                };
                Ok(synthesise(&Spec::new(m, identity)))
            }
        }
    }

    /// The identity this source will present, without building the image.
    pub fn identity(&self) -> Result<Identity, String> {
        match self {
            Source::File(p) => Identity::from_nor(p),
            Source::Synthetic {
                model,
                seed,
                serial,
                guid,
                ..
            } => {
                let m = Model::lookup(model)
                    .ok_or_else(|| format!("{model} is not a model number this program knows"))?;
                match guid {
                    Some(g) => Identity::provided(serial.as_deref(), *g),
                    None => Ok(Identity::generate(m, *seed)),
                }
            }
        }
    }

    /// The model this source describes — **and for a dump that means reading it.**
    ///
    /// The doc here used to say *where it is known without reading anything*, which was false of
    /// the arm below it from the day it was written: a [`Source::File`] is opened, all
    /// [`crate::inspect::NOR_LEN`] of it, and its SysCfg parsed. Naming the cost matters now that
    /// `settings::model_of` delegates here rather than answering `None` for every file — a picker
    /// listing N filed iPods asks this N times per push, which is N × 1 MiB off the page cache.
    /// It is worth it: the alternative was the same iPod wearing its model on one row and its file
    /// stem on the row above.
    ///
    /// `None` for a path that is not there, is not a NOR image, or carries no model record.
    pub fn model(&self) -> Option<&'static Model> {
        match self {
            Source::File(p) => {
                let nor = std::fs::read(p).ok()?;
                crate::inspect::syscfg(&nor)?.model_info()
            }
            Source::Synthetic { model, .. } => Model::lookup(model),
        }
    }

    /// A stable string identifying this source, for cache keys.
    ///
    /// A synthesised ROM has no path, so keying a cache on one would give every generated machine
    /// the same key — a 5.5G would restore a 5G's snapshot. The recipe is the identity, so the
    /// recipe is the key.
    pub fn cache_tag(&self) -> String {
        match self {
            Source::File(p) => p.to_string_lossy().into_owned(),
            Source::Synthetic {
                model,
                seed,
                serial,
                guid,
                ..
            } => format!(
                "synthetic:{model}:{seed}:{}:{}",
                serial.as_deref().unwrap_or("-"),
                guid.map(|g| format!("{g:016X}"))
                    .unwrap_or_else(|| "-".into())
            ),
        }
    }

    /// The boot screen this source shows.
    ///
    /// **A dump shows its own `logo` image**, which is the picture that iPod boots to and which
    /// [`crate::inspect::nor_images`] has been listing on the row above this one all along. It used
    /// to draw the project's mark for a dump as well — the arm below read the splash field, and a
    /// [`Source::File`] has no splash field, so every real ROM fell through to the `None` arm and
    /// the preview showed our click wheel over somebody else's boot ROM.
    ///
    /// Showing it back is not redistribution: it is the operator's own file, read off their own
    /// disk, and it never leaves the panel. What a **generated** ROM carries is a separate
    /// question and [`mark_tile`] is its answer.
    ///
    /// A synthesised source shows the user's image if they chose one, else the mark. A splash that
    /// cannot be read **falls back to the mark and says why** rather than refusing to boot; a
    /// picture is decoration, and failing to start an iPod over it would be the wrong trade. A dump
    /// with no readable `logo` falls back the same way and says nothing, because a partial dump not
    /// carrying one is not an error.
    pub fn boot_screen(&self, w: usize, h: usize) -> Vec<u16> {
        match self {
            Source::File(p) => std::fs::read(p)
                .ok()
                .and_then(|nor| crate::inspect::nor_image(&nor, "logo"))
                .and_then(|img| logo_tile(&img))
                .map_or_else(
                    || boot_screen(w, h),
                    |(tw, th, tile)| panel_with(w, h, tw, th, &tile),
                ),
            Source::Synthetic {
                splash: Some(p), ..
            } => match std::fs::read(p)
                .map_err(|e| e.to_string())
                .and_then(|b| crate::splash::decode(&b))
            {
                Ok(img) => boot_screen_with(w, h, &img),
                Err(e) => {
                    eprintln!("{}: {e} — using the built-in mark", p.display());
                    boot_screen(w, h)
                }
            },
            Source::Synthetic { .. } => boot_screen(w, h),
        }
    }

    /// One line for a person.
    pub fn describe(&self) -> String {
        match self {
            Source::File(p) => {
                let name = p.file_name().unwrap_or_default().to_string_lossy();
                match crate::inspect::describe_rom(p, "iPod") {
                    Some(d) => format!("{d} — from {name}"),
                    None => format!("from {name}"),
                }
            }
            Source::Synthetic { .. } => match (self.model(), self.identity()) {
                (Some(m), Ok(id)) => format!(
                    "generated — {} GB {}, {}, {}",
                    m.capacity_gb,
                    m.colour().label().to_lowercase(),
                    m.generation.label(),
                    id.serial.as_deref().unwrap_or("no serial")
                ),
                _ => "generated".to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Colour;

    fn model(num: &str) -> &'static Model {
        Model::lookup(num).expect("a known model")
    }

    /// **Rebuild the handoff from the real dump and compare it to what the boot ROM actually
    /// leaves.** The reference bytes were captured from a cold boot at `--stop-at=0x10000000:1`;
    /// the identity-bearing fields are checked against the dump's own `SysCfg` rather than against
    /// a literal, so nobody's serial ends up in this repository.
    ///
    /// Skips loudly without the dump, which is gitignored.
    #[test]
    fn the_handoff_matches_what_the_boot_rom_leaves() {
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../resources/roms/retail_5g_MA146_HwVr000B0005_internal_rom_000000-0FFFFF.bin"
        ));
        let Ok(rom) = std::fs::read(path) else {
            println!("SKIPPED: {} is not here (gitignored)", path.display());
            return;
        };
        let cfg = crate::inspect::syscfg(&rom).expect("the dump has a SysCfg");
        let m = cfg.model_info().expect("MA146 resolves");
        let id = Identity {
            serial: cfg.serial.clone(),
            guid: cfg.guid.expect("a GUID"),
            source: crate::identity::Source::RealDevice,
        };
        let syscfg_bytes = &rom[crate::inspect::SYSCFG_AT
            ..crate::inspect::SYSCFG_AT
                + crate::inspect::SYSCFG_HEADER
                + cfg.records.len() * crate::inspect::SYSCFG_RECORD];
        let h = handoff(&id, m, syscfg_bytes);

        // The measured constants, exactly as the capture shows them.
        assert_eq!(&h[..4], b"IsyS");
        assert_eq!(
            u32::from_le_bytes(h[4..8].try_into().unwrap()),
            0xf8,
            "len must be 0xf8"
        );
        assert_eq!(&h[8..16], b"iPod M25");
        assert_eq!(
            u32::from_le_bytes(h[0x84..0x88].try_into().unwrap()),
            0x000B_0005
        );
        assert_eq!(&h[0x88..0x90], b"1.00    ");
        assert_eq!(&h[0x98..0x9d], b"MA146");

        // The identity, checked against the dump rather than against a literal.
        let serial = cfg.serial.clone().expect("the dump has a serial");
        assert_eq!(&h[0x18..0x18 + serial.len()], serial.as_bytes());
        let guid = cfg.guid.expect("a GUID");
        assert_eq!(
            u32::from_le_bytes(h[0x38..0x3c].try_into().unwrap()),
            guid as u32
        );
        assert_eq!(
            u32::from_le_bytes(h[0x3c..0x40].try_into().unwrap()),
            (guid >> 32) as u32
        );

        // And the SysCfg copied in directly after the struct, byte for byte.
        assert_eq!(&h[HANDOFF_LEN..HANDOFF_LEN + 4], b"gfCS");
        assert_eq!(&h[HANDOFF_LEN..], syscfg_bytes);
    }

    /// The whole point: what goes in comes back out through the ordinary reader, with no special
    /// case for having been generated.
    #[test]
    fn a_synthesised_rom_reads_back_through_the_normal_parser() {
        let id = Identity::generate(model("MA146"), 1234);
        let spec = Spec::new(model("MA146"), id.clone());
        let nor = synthesise(&spec);

        assert_eq!(nor.len(), NOR_LEN as usize);
        assert_eq!(
            u32::from_le_bytes(nor[..4].try_into().unwrap()),
            RESET_VECTOR
        );
        assert!(is_synthetic(&nor));

        let c = crate::inspect::syscfg(&nor).expect("a synthesised ROM must parse");
        assert_eq!(c.serial, id.serial);
        assert_eq!(c.guid, Some(id.guid));
        assert!(c.guid_looks_apple());
        // The form the hardware writes, not the lookup key — which is what a real dump has.
        assert_eq!(c.model.as_deref(), Some("MA146"));
        assert_eq!(c.model_info().expect("still resolves").number, "A146");
        assert_eq!(c.hw_vr, Some(0x000B_0005));
        assert_eq!(
            c.tags,
            ["SrNm", "FwId", "HwId", "HwVr", "Regn", "Mod#", "DrmV"]
        );

        // And it identifies as what it claims to be.
        let info = c.model_info().expect("A146 resolves");
        assert_eq!(info.colour(), Colour::Black);
        assert_eq!(info.capacity_gb, 30);
        assert_eq!(c.generation_agrees(), Some(true));
    }

    /// Colour is chosen by choosing a model number. There is no colour record in any documented
    /// `SysCfg`, so this is the only lever there is — and it works.
    #[test]
    fn choosing_the_model_number_chooses_the_colour() {
        for (num, want) in [
            ("A146", Colour::Black),
            ("A002", Colour::White),
            ("A452", Colour::U2),
        ] {
            let nor = synthesise(&Spec::new(model(num), Identity::generate(model(num), 7)));
            let got = crate::inspect::syscfg(&nor)
                .and_then(|c| c.model_info())
                .expect("resolves")
                .colour();
            assert_eq!(got, want, "{num}");
        }
    }

    /// **The boot screen is the firmware's, not the case's.** Every iPod with video boots a white
    /// logo on black — a white 5G included.
    ///
    /// This test used to assert the opposite, and both it and the code it checked were written from
    /// the same wrong belief, which is why the test passed. Corrected 2026-08-19 by the operator,
    /// who owned a white one. A test agreeing with its subject is not evidence about hardware.
    #[test]
    fn every_case_boots_the_same_screen() {
        let (w, h) = (320usize, 240usize);
        let fb = boot_screen(w, h);
        assert_eq!(fb.len(), w * h);

        // The background is black, at a corner.
        assert_eq!(fb[0], 0x0000, "an iPod with video boots to a black screen");

        // The mark is drawn, in white.
        let ink = fb.iter().filter(|&&p| p != 0x0000).count();
        assert!(ink > 200, "nothing was drawn: {ink}");

        // It is a RING, not a disc: the centre is background.
        assert_eq!(fb[(h / 2) * w + w / 2], 0x0000, "the middle must be empty");
    }

    /// **A supplied image is painted as a mask, not pasted.**
    ///
    /// Its brightness is coverage and the firmware's own colours supply the ink, so the extracted
    /// Apple tile — white artwork on black — lands as white artwork on black, and a dark-on-light
    /// picture of the same shape lands identically. That is what makes "supply any logo" work at
    /// all: nobody has to know which polarity their file is in.
    ///
    /// *(This test used to assert that the same image inverted itself for a white case. That was
    /// the wrong belief about the hardware — see `every_case_boots_the_same_screen`.)*
    #[test]
    fn a_supplied_logo_is_painted_as_a_mask() {
        // A white blob on black, which is the shape of the real tile.
        let (iw, ih) = (62usize, 78usize);
        let mut rgb = vec![0u8; iw * ih * 3];
        for y in 20..60 {
            for x in 15..45 {
                for c in 0..3 {
                    rgb[(y * iw + x) * 3 + c] = 255;
                }
            }
        }
        let img = crate::splash::Image {
            w: iw,
            h: ih,
            rgb: rgb.clone(),
        };
        let (w, h) = (320usize, 240usize);
        let fb = boot_screen_with(w, h, &img);

        assert_eq!(fb[0], 0x0000, "the background is the firmware's black");
        let c = (h / 2) * w + w / 2;
        let lum = |p: u16| ((p >> 11) & 0x1f) as u32 + ((p >> 5) & 0x3f) as u32 + (p & 0x1f) as u32;
        assert!(lum(fb[c]) > 80, "the mark should be bright");
        assert_ne!(
            fb[c], fb[0],
            "the mark is invisible against its own background"
        );

        // The polarity of the source does not matter, because it is a mask: inverting the image
        // inverts which pixels are ink, and the blob's centre goes dark while the corner lights up.
        let inverted: Vec<u8> = rgb.iter().map(|v| 255 - v).collect();
        let img2 = crate::splash::Image {
            w: iw,
            h: ih,
            rgb: inverted,
        };
        let fb2 = boot_screen_with(w, h, &img2);
        assert!(
            lum(fb2[c]) < 30,
            "an inverted source should leave the centre dark"
        );
    }

    /// A generated ROM must be recognisable as generated, and a real one must not trip the check.
    #[test]
    fn the_synthetic_mark_is_present_and_specific() {
        let nor = synthesise(&Spec::new(
            model("A146"),
            Identity::generate(model("A146"), 1),
        ));
        assert!(is_synthetic(&nor));
        // A real dump has code at 0x40, not our sentence.
        let mut real_ish = nor.clone();
        real_ish[SYNTH_MARK_AT..SYNTH_MARK_AT + SYNTH_MARK.len()].fill(0xEE);
        assert!(!is_synthetic(&real_ish), "the check must be able to say no");
        assert!(!is_synthetic(&[]), "and must not panic on a short buffer");
    }

    /// An unsourced Gestalt ID leaves the record out rather than writing a guess. A missing `HwVr`
    /// reads as "we do not know"; a wrong one reads as a fact.
    #[test]
    fn a_generation_with_no_known_gestalt_id_omits_the_record() {
        // Nano 1G — in the table, and its Gestalt ID has never been sourced here.
        let nano = model("A004");
        assert!(
            nano.generation.gestalt().is_none(),
            "precondition: still unsourced"
        );
        let nor = synthesise(&Spec::new(nano, Identity::generate(nano, 2)));
        let c = crate::inspect::syscfg(&nor).expect("parses");
        assert_eq!(c.hw_vr, None);
        assert!(
            !c.tags.contains(&"HwVr".to_string()),
            "no HwVr record at all"
        );
        // And the cross-check reports "not checked" rather than a false agreement.
        assert_eq!(c.generation_agrees(), None);
    }

    /// `carry_from` takes the three records nobody understands off real hardware, so a ROM
    /// generated beside a real iPod carries that iPod's values instead of our defaults.
    #[test]
    fn the_unexplained_records_can_be_carried_from_a_real_dump() {
        let source = synthesise(&Spec {
            hw_id: 0x8201_763A,
            region: [9; 16],
            drm_version: 42,
            ..Spec::new(model("A146"), Identity::generate(model("A146"), 3))
        });
        let parsed = crate::inspect::syscfg(&source).expect("parses");

        let spec =
            Spec::new(model("A002"), Identity::generate(model("A002"), 99)).carry_from(&parsed);
        assert_eq!(spec.hw_id, 0x8201_763A);
        assert_eq!(spec.region, [9; 16]);
        assert_eq!(spec.drm_version, 42);
        // The identity and model are the caller's, not the source's.
        assert_eq!(spec.model.number, "A002");
        assert_eq!(spec.identity, Identity::generate(model("A002"), 99));
    }

    /// A scratch path of this module's own. Never inside the operator's data directory.
    fn scratch(what: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "ipod-emulator-nor-{what}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&d).expect("a temp directory");
        d
    }

    /// **This program's own generated ROM passes this program's own inspector.**
    ///
    /// It did not. `synthesise` wrote a reset vector, a mark and a `SysCfg` and no `flsh` directory
    /// at all, so [`crate::inspect::flash`] — the verdict the setup screen and `--check-images`
    /// both render — called it `Wrong`: *"1 MiB and a plausible reset vector, but no `flsh` image
    /// directory at 0xffe00"*. A generated ROM our own inspector calls wrong is a bad thing to hand
    /// out, and it reads as a defect to anybody who points the checker at one.
    ///
    /// The control is the second half: zero the directory back out and the same file is `Wrong`
    /// again, with that same sentence. Without it this test would pass over any ROM at all that
    /// happened to be a megabyte long.
    #[test]
    fn a_synthesised_rom_passes_the_check_this_program_judges_a_dump_by() {
        let dir = scratch("verdict");
        let at = dir.join("rom.bin");
        let nor = synthesise(&Spec::new(model("MA146"), Identity::generate(model("MA146"), 0x4f2a)));
        assert_eq!(nor.len() as u64, crate::inspect::NOR_LEN);
        std::fs::write(&at, &nor).expect("writing the ROM");

        let verdict = crate::inspect::flash(&at);
        assert!(
            verdict.ok(),
            "the inspector refuses this program's own output: {}",
            verdict.text()
        );
        // And it says what is in there, which is one image and not four — in the singular, which
        // no dump had ever made this sentence reach for.
        assert!(
            verdict.text().contains("1 image at 0x10000000: logo"),
            "the Good verdict does not name the image the directory indexes: {}",
            verdict.text()
        );

        // The control: take the directory away and the old refusal comes back, word for word.
        let mut without = nor.clone();
        let d = crate::inspect::NOR_DIRECTORY as usize;
        without[d..d + crate::inspect::IMAGE_RECORD].fill(0);
        std::fs::write(&at, &without).expect("writing the ROM");
        let refused = crate::inspect::flash(&at);
        assert!(
            !refused.ok() && refused.text().contains("no `flsh` image directory"),
            "the check cannot see a missing directory, so passing it means nothing: {}",
            refused.text()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The picture on the panel and the picture in the ROM are one drawing, and it lands where
    /// the hardware puts it.**
    ///
    /// [`boot_screen`] draws the tile straight rather than building a megabyte and reading it back,
    /// which is two routes to one image and therefore two things that can drift.
    ///
    /// **The placement is asserted against `research/14`'s own numbers and not against
    /// [`panel_with`]**, which is the difference between a test and a tautology: written the
    /// obvious way — compare `panel_with(…)` to `boot_screen(…)` — both sides go through the same
    /// paste, so moving the tile to the corner of the panel moved *both* and the test stayed green.
    /// Measured: it did. `(129, 81)` is the `x0`/`y0` the bootloader states in its own rect header.
    #[test]
    fn the_screen_a_synthesised_ipod_shows_is_the_image_in_its_own_rom() {
        let nor = synthesise(&Spec::new(model("A146"), Identity::generate(model("A146"), 11)));
        let img = crate::inspect::nor_image(&nor, "logo").expect("a synthesised ROM carries a logo");
        let (w, h, tile) = logo_tile(&img).expect("and it is in the container the dumps use");
        assert_eq!((w, h), (LOGO_W, LOGO_H));
        assert_eq!(tile, mark_tile(), "the ROM carries a picture the panel never shows");

        let (pw, ph) = (320usize, 240usize);
        let fb = boot_screen(pw, ph);
        for y in 0..h {
            for x in 0..w {
                assert_eq!(
                    fb[(81 + y) * pw + 129 + x],
                    tile[y * w + x],
                    "the logo is not at (129,81): pixel ({x},{y}) of the tile is somewhere else"
                );
            }
        }
        // …and nothing is drawn outside that rectangle, which is what makes the loop above a
        // placement rather than a coincidence of two black regions overlapping.
        let outside = (0..ph)
            .flat_map(|y| (0..pw).map(move |x| (x, y)))
            .filter(|&(x, y)| !(129..129 + w).contains(&x) || !(81..81 + h).contains(&y))
            .filter(|&(x, y)| fb[y * pw + x] != 0)
            .count();
        assert_eq!(outside, 0, "{outside} lit pixels fall outside the logo's rectangle");
    }

    /// **The `logo` container is Apple's, read off the hardware, and both dumps agree on it.**
    ///
    /// Skips loudly without them — both are gitignored. The retail dump and the prototype carry
    /// byte-identical logo images at different offsets, so what this checks is the *format*, not
    /// one build's bytes: 62×78, a stated payload of exactly `w × h × 2`, and a directory checksum
    /// that reproduces as a plain byte sum.
    ///
    /// 2 916 lit pixels is `research/14`'s own figure for the placed logo, arrived at there by
    /// refolding a framebuffer and here by parsing the file it came out of.
    #[test]
    fn the_logo_container_is_what_both_dumps_carry() {
        let roms = [
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../resources/roms/retail_5g_MA146_HwVr000B0005_internal_rom_000000-0FFFFF.bin"
            ),
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../resources/archive-downloads/internal_rom_000000-0FFFFF.bin"
            ),
        ];
        let mut read = 0;
        for path in roms {
            let Ok(rom) = std::fs::read(path) else {
                println!("SKIPPED: {path} is not here (gitignored)");
                continue;
            };
            read += 1;
            let e = crate::inspect::nor_images(&rom)
                .into_iter()
                .find(|e| e.tag == "logo")
                .expect("a 5G NOR indexes a logo");
            let img = crate::inspect::nor_image(&rom, "logo").expect("and its body is in the file");
            assert_eq!(img.len(), LOGO_HEADER + LOGO_W * LOGO_H * 2);
            assert_eq!(e.addr, crate::inspect::LOAD_ADDR_5G);

            let (w, h, tile) = logo_tile(&img).expect("the container parses");
            assert_eq!((w, h), (LOGO_W, LOGO_H));
            let lit = tile.iter().filter(|&&p| p != 0).count();
            assert_eq!(lit, 2916, "research/14 counts 2 916 pixels in the placed logo");

            // The header this program writes is the header those files carry, everywhere it makes
            // a claim: the tag, the two dimensions, the stride and the payload length.
            let ours = logo_image(&tile);
            assert_eq!(
                ours[..LOGO_HEADER],
                img[..LOGO_HEADER],
                "our container and Apple's disagree in the header"
            );
            assert_eq!(ours, img, "…and the payload must round trip untouched");
        }
        assert!(read > 0, "neither dump is here, so this test measured nothing");
    }

    /// **A dump's boot screen is the dump's own logo, not ours.**
    ///
    /// `Source::boot_screen` read the splash field, and a [`Source::File`] has none — so every real
    /// ROM fell past that arm onto the built-in mark, and §11.4's *Show its boot screen* drew this
    /// project's click wheel over somebody else's iPod. The row above it was listing that dump's
    /// `logo` at the time.
    ///
    /// The fixture inverts the mark rather than inventing a picture, so the two images are the same
    /// shape and differ only in which of them the code chose.
    #[test]
    fn a_dump_shows_its_own_boot_logo_and_not_the_built_in_mark() {
        let dir = scratch("dumplogo");
        let at = dir.join("rom.bin");
        let mut nor = synthesise(&Spec::new(model("A146"), Identity::generate(model("A146"), 5)));

        // Somebody else's logo: the mark, inverted. Written through the same writer, so the record
        // and the checksum stay honest.
        let theirs: Vec<u16> = mark_tile().iter().map(|p| !p).collect();
        let img = logo_image(&theirs);
        nor[LOGO_AT..LOGO_AT + img.len()].copy_from_slice(&img);
        write_image_record(&mut nor, 0, "logo", LOGO_AT as u32, &img);
        std::fs::write(&at, &nor).expect("writing the ROM");

        let (w, h) = (320usize, 240usize);
        let shown = Source::File(at.clone()).boot_screen(w, h);
        assert_eq!(
            shown,
            panel_with(w, h, LOGO_W, LOGO_H, &theirs),
            "the preview is not this dump's own logo"
        );
        assert_ne!(
            shown,
            boot_screen(w, h),
            "the preview fell back to the built-in mark, which is the defect this test is about"
        );

        // The control, and the other half of the contract: a dump with no readable logo falls back
        // to the mark rather than to a black screen or a panic.
        let bare = dir.join("nologo.bin");
        let mut stripped = nor.clone();
        let d = crate::inspect::NOR_DIRECTORY as usize;
        stripped[d..d + crate::inspect::IMAGE_RECORD].fill(0);
        std::fs::write(&bare, &stripped).expect("writing the ROM");
        assert_eq!(
            Source::File(bare).boot_screen(w, h),
            boot_screen(w, h),
            "a dump carrying no logo must still show something"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `logo` image that is not one is refused rather than half-read.
    ///
    /// The length word is the discriminator: a header whose stated payload disagrees with its own
    /// two dimensions is not a container this program will believe, because believing it means
    /// reading `w × h` halfwords out of a buffer that never held them.
    #[test]
    fn a_container_that_disagrees_with_itself_is_refused() {
        let good = logo_image(&mark_tile());
        assert!(logo_tile(&good).is_some(), "precondition: a real one parses");

        assert!(logo_tile(b"").is_none(), "and must not panic on a short buffer");
        let mut wrong_tag = good.clone();
        wrong_tag[..4].copy_from_slice(b"junk");
        assert!(logo_tile(&wrong_tag).is_none(), "a different tag is a different image");

        let mut lying = good.clone();
        lying[0x14..0x18].copy_from_slice(&99u32.to_le_bytes());
        assert!(
            logo_tile(&lying).is_none(),
            "a payload length that does not equal w x h x 2 was believed"
        );

        let mut truncated = good.clone();
        truncated.truncate(LOGO_HEADER + 8);
        assert!(logo_tile(&truncated).is_none(), "an image shorter than it claims was read");
    }
}

#[cfg(test)]
mod mint_tests {
    use super::*;

    /// **The identity is minted once and is then permanent**, so the mint itself must not be a
    /// counter with a random start: two presses in one process have to produce two different iPods,
    /// or the "same seed, same iPod" contract would be quietly making everybody the same one.
    ///
    /// `RandomState`'s key increments by one between two instances in one thread, which is why
    /// [`mint_seed`] mixes in the clock and the process id as well.
    #[test]
    fn two_seeds_drawn_in_one_process_are_different() {
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..64 {
            seen.insert(mint_seed());
        }
        assert_eq!(
            seen.len(),
            64,
            "64 mints produced {} distinct seeds — two people would get one iPod",
            seen.len()
        );
    }

    /// `0` is [`Source::default`]'s seed, and a minted identity indistinguishable from the
    /// never-chosen default is the one value that must not come out of a mint: it is what
    /// `work::minted` tells a made iPod from an unmade one by.
    #[test]
    fn a_minted_seed_is_never_the_default_seed() {
        for _ in 0..256 {
            assert_ne!(mint_seed(), 0, "a mint produced the never-chosen default");
        }
        let Source::Synthetic { seed, model, .. } = Source::default() else {
            panic!("the default source is not synthetic");
        };
        assert_eq!(seed, 0, "the default seed moved, so 0 is no longer the marker");
        assert_eq!(model, DEFAULT_MODEL, "two spellings of which iPod this program makes");
    }

    /// **The iPod one press makes is a WHITE 30 GB 5.5G**, and every figure is the model table's.
    ///
    /// Operator, having watched a first run: *"it made me synthesise bootrom 5.5g 30gb black
    /// (should default to white instead imo)"*. The generation and the capacity are settled
    /// elsewhere — `ROADMAP.md` §"5G, 5.5G, and which is the default" for the one, `A444`'s own row
    /// for the other — so this asserts all three together: a change that got the colour by picking
    /// a number outside the 5.5G block would move one of the other two and be caught here.
    ///
    /// It reads them out of [`Model`] rather than restating them, which is the point: a model
    /// number this program invented would resolve to nothing and fail at the first line.
    #[test]
    fn the_ipod_one_press_makes_is_a_white_thirty_gigabyte_five_and_a_half() {
        use crate::identity::Colour;
        let m = Model::lookup(DEFAULT_MODEL)
            .expect("the default is a model number this build's table holds");
        assert_eq!(m.colour(), Colour::White, "the default iPod is not white");
        assert_eq!(m.capacity_gb, 30, "the default iPod is not a 30 GB one");
        assert_eq!(m.generation.label(), "5.5G", "the default iPod is not a 5.5G");
        // And the black one it replaced is still in the table, still black, still a 5.5G — this
        // moved which one is preselected and removed nothing.
        let black = Model::lookup("A446").expect("the black 5.5G is still a model");
        assert_eq!(black.colour(), Colour::Black);
        assert_eq!(black.capacity_gb, m.capacity_gb);
        assert_eq!(black.generation, m.generation);
    }
}
