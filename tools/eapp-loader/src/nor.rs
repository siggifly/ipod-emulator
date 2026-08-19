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
    /// is the one value in this whole system that came from a comment rather than from hardware,
    /// and the 5.5G does not boot. Being able to vary exactly that, with nothing else moving, is
    /// how it gets isolated.
    pub hw_vr: Option<u32>,
}

impl Spec {
    /// A spec for `model` with `identity`, carrying the reference unit's values for the three
    /// records whose meaning we have not established.
    pub fn new(model: &'static Model, identity: Identity) -> Spec {
        let mut region = [0u8; 16];
        region[..8].copy_from_slice(&[0x01, 0x00, 0x02, 0x00, 0x01, 0x00, 0x02, 0x00]);
        Spec { identity, model, hw_id: 0, region, drm_version: 6, hw_vr: None }
    }

    /// Take the unexplained records from a real dump, so a ROM synthesised alongside real hardware
    /// carries that hardware's values rather than a default.
    ///
    /// This is what "generate one matching my iPod" does: the identity comes from the user, and
    /// everything nobody understands comes from the machine it is standing in for.
    pub fn carry_from(mut self, source: &crate::inspect::SysCfg) -> Spec {
        for (tag, payload) in &source.records {
            match tag.as_str() {
                "HwId" => self.hw_id = u32::from_le_bytes(payload[..4].try_into().unwrap_or([0; 4])),
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
    nor
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

/// The boot screen a synthesised iPod shows, as RGB565 pixels for a `w`×`h` panel.
///
/// ## Why this exists at all
///
/// A real NOR carries a `logo` image and Apple's bootloader blits it — 62×78 pixels to
/// `(129,81)`, measured in `research/14`. A **synthesised** NOR has no such image, and it could not
/// carry Apple's if it did: that artwork is Apple's, and a generated ROM handing it out is a
/// generated ROM redistributing it. So this draws the project's own mark instead — a click wheel
/// outline, which is the iPod's most recognisable shape and is not a trademark.
///
/// ## The colours are the hardware's, not a choice
///
/// A white iPod boots to a **dark logo on white**; a black one and the U2 boot to a **white logo on
/// black**. So the screen follows the model number, like everything else about the case — the same
/// `Mod#` that decides the colour of the plastic decides the colour of this.
pub fn boot_screen(colour: crate::identity::Colour, w: usize, h: usize) -> Vec<u16> {
    use crate::identity::Colour;
    // RGB565. The panel is 16-bit, and 0 is black, so a white background has to be written.
    const WHITE: u16 = 0xffff;
    const BLACK: u16 = 0x0000;
    // Not pure black on white: the real logo is a dark grey-black shape, and pure 0 on 0xffff
    // rings harshly on an LCD this small.
    const INK_DARK: u16 = 0x2104;

    let (bg, fg) = match colour {
        Colour::White => (WHITE, INK_DARK),
        // Black and the U2 both boot white-on-black. The U2's red is the WHEEL, not the case, and
        // not the boot screen.
        _ => (BLACK, WHITE),
    };

    let mut fb = vec![bg; w * h];

    // The rectangle Apple's own logo occupies, so ours sits exactly where a real one would.
    const LOGO_W: f32 = 62.0;
    const LOGO_H: f32 = 78.0;
    let cx = (w as f32) / 2.0;
    let cy = (h as f32) / 2.0;
    // The wheel is round, so the short side bounds it.
    let outer = LOGO_W.min(LOGO_H) / 2.0 - 1.0;
    // The same proportion the window draws the real wheel at: the centre button is 0.34 of the
    // wheel's radius.
    let inner = outer * 0.34;
    let stroke = 2.0;

    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            let on_outer = (d - outer).abs() <= stroke / 2.0;
            let on_inner = (d - inner).abs() <= stroke / 2.0;
            if on_outer || on_inner {
                fb[y * w + x] = fg;
            }
        }
    }
    fb
}

/// Whether this image was made by [`synthesise`] rather than read off an iPod.
pub fn is_synthetic(nor: &[u8]) -> bool {
    nor.get(SYNTH_MARK_AT..SYNTH_MARK_AT + SYNTH_MARK.len()) == Some(SYNTH_MARK)
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
    },
}

impl Default for Source {
    /// A 30 GB black **5.5G** — the newest generation, per the operator decision recorded in
    /// [ROADMAP] §"5G, 5.5G, and which is the default".
    ///
    /// Synthesis is what makes that default honest. It was previously blocked on this project
    /// owning exactly one dump, which is a 5G: defaulting to a machine we could not produce would
    /// have been a setting that did not work. A generated ROM is whichever model is asked for.
    ///
    /// The reference hardware every measurement in `research/` was taken on is still the 5G
    /// (`A146`), and a checkout that has the dump still boots it — see the source resolution in
    /// the window's argument parsing.
    fn default() -> Source {
        Source::Synthetic { model: "A446".into(), seed: 0, serial: None, guid: None }
    }
}

impl Source {
    /// The bytes, read or built.
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        match self {
            Source::File(p) => std::fs::read(p).map_err(|e| format!("{}: {e}", p.display())),
            Source::Synthetic { model, seed, serial, guid } => {
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
            Source::Synthetic { model, seed, serial, guid } => {
                let m = Model::lookup(model)
                    .ok_or_else(|| format!("{model} is not a model number this program knows"))?;
                match guid {
                    Some(g) => Identity::provided(serial.as_deref(), *g),
                    None => Ok(Identity::generate(m, *seed)),
                }
            }
        }
    }

    /// The model, where it is known without reading anything.
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
            Source::Synthetic { model, seed, serial, guid } => format!(
                "synthetic:{model}:{seed}:{}:{}",
                serial.as_deref().unwrap_or("-"),
                guid.map(|g| format!("{g:016X}")).unwrap_or_else(|| "-".into())
            ),
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
                    "generated — {} GB {}, {} · {}",
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
        assert_eq!(u32::from_le_bytes(h[4..8].try_into().unwrap()), 0xf8, "len must be 0xf8");
        assert_eq!(&h[8..16], b"iPod M25");
        assert_eq!(u32::from_le_bytes(h[0x84..0x88].try_into().unwrap()), 0x000B_0005);
        assert_eq!(&h[0x88..0x90], b"1.00    ");
        assert_eq!(&h[0x98..0x9d], b"MA146");

        // The identity, checked against the dump rather than against a literal.
        let serial = cfg.serial.clone().expect("the dump has a serial");
        assert_eq!(&h[0x18..0x18 + serial.len()], serial.as_bytes());
        let guid = cfg.guid.expect("a GUID");
        assert_eq!(u32::from_le_bytes(h[0x38..0x3c].try_into().unwrap()), guid as u32);
        assert_eq!(u32::from_le_bytes(h[0x3c..0x40].try_into().unwrap()), (guid >> 32) as u32);

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
        assert_eq!(u32::from_le_bytes(nor[..4].try_into().unwrap()), RESET_VECTOR);
        assert!(is_synthetic(&nor));

        let c = crate::inspect::syscfg(&nor).expect("a synthesised ROM must parse");
        assert_eq!(c.serial, id.serial);
        assert_eq!(c.guid, Some(id.guid));
        assert!(c.guid_looks_apple());
        // The form the hardware writes, not the lookup key — which is what a real dump has.
        assert_eq!(c.model.as_deref(), Some("MA146"));
        assert_eq!(c.model_info().expect("still resolves").number, "A146");
        assert_eq!(c.hw_vr, Some(0x000B_0005));
        assert_eq!(c.tags, ["SrNm", "FwId", "HwId", "HwVr", "Regn", "Mod#", "DrmV"]);

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
        for (num, want) in
            [("A146", Colour::Black), ("A002", Colour::White), ("A452", Colour::U2)]
        {
            let nor = synthesise(&Spec::new(model(num), Identity::generate(model(num), 7)));
            let got = crate::inspect::syscfg(&nor)
                .and_then(|c| c.model_info())
                .expect("resolves")
                .colour();
            assert_eq!(got, want, "{num}");
        }
    }

    /// **The colour rule is the hardware's.** A white iPod boots dark-on-white; a black one and
    /// the U2 boot white-on-black. Getting this backwards would be visible in the first frame.
    #[test]
    fn the_boot_screen_follows_the_case_colour() {
        use crate::identity::Colour;
        let (w, h) = (320usize, 240usize);

        let white = boot_screen(Colour::White, w, h);
        let black = boot_screen(Colour::Black, w, h);
        let u2 = boot_screen(Colour::U2, w, h);
        assert_eq!(white.len(), w * h);

        // A corner is background, and the two backgrounds are opposites.
        assert_eq!(white[0], 0xffff, "a white iPod boots to a white screen");
        assert_eq!(black[0], 0x0000, "a black iPod boots to a black screen");
        // The U2 is a black case with a red WHEEL -- its boot screen is the black one, not a red
        // one, and not the white one.
        assert_eq!(u2, black, "the U2 boots exactly as the black iPod does");

        // The mark is drawn, and in the foreground colour.
        let ink_white_case = white.iter().filter(|&&p| p != 0xffff).count();
        let ink_black_case = black.iter().filter(|&&p| p != 0x0000).count();
        assert!(ink_white_case > 200, "nothing was drawn on the white case: {ink_white_case}");
        assert!(ink_black_case > 200, "nothing was drawn on the black case: {ink_black_case}");
        // Two rings of the same geometry, so the same number of pixels either way.
        assert_eq!(ink_white_case, ink_black_case);

        // It is a RING, not a disc: the centre is background on both.
        assert_eq!(white[(h / 2) * w + w / 2], 0xffff, "the middle must be empty");
        assert_eq!(black[(h / 2) * w + w / 2], 0x0000, "the middle must be empty");
    }

    /// A generated ROM must be recognisable as generated, and a real one must not trip the check.
    #[test]
    fn the_synthetic_mark_is_present_and_specific() {
        let nor = synthesise(&Spec::new(model("A146"), Identity::generate(model("A146"), 1)));
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
        assert!(nano.generation.gestalt().is_none(), "precondition: still unsourced");
        let nor = synthesise(&Spec::new(nano, Identity::generate(nano, 2)));
        let c = crate::inspect::syscfg(&nor).expect("parses");
        assert_eq!(c.hw_vr, None);
        assert!(!c.tags.contains(&"HwVr".to_string()), "no HwVr record at all");
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

        let spec = Spec::new(model("A002"), Identity::generate(model("A002"), 99)).carry_from(&parsed);
        assert_eq!(spec.hw_id, 0x8201_763A);
        assert_eq!(spec.region, [9; 16]);
        assert_eq!(spec.drm_version, 42);
        // The identity and model are the caller's, not the source's.
        assert_eq!(spec.model.number, "A002");
        assert_eq!(spec.identity, Identity::generate(model("A002"), 99));
    }
}
