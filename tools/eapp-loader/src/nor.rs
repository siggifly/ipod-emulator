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
}

impl Spec {
    /// A spec for `model` with `identity`, carrying the reference unit's values for the three
    /// records whose meaning we have not established.
    pub fn new(model: &'static Model, identity: Identity) -> Spec {
        let mut region = [0u8; 16];
        region[..8].copy_from_slice(&[0x01, 0x00, 0x02, 0x00, 0x01, 0x00, 0x02, 0x00]);
        Spec { identity, model, hw_id: 0, region, drm_version: 6 }
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
    if let Some(hw_vr) = spec.model.generation.gestalt() {
        b = b.word_at4("HwVr", hw_vr);
    }
    b = b.raw("Regn", spec.region);
    // `Mod#` in the form the NOR writes it: bare, no `x` prefix. That is the drive's `SysInfo`
    // convention, not the flash's.
    b = b.text("Mod#", spec.model.number);
    b = b.word_at4("DrmV", spec.drm_version);

    let block = b.build();
    nor[SYSCFG_AT..SYSCFG_AT + block.len()].copy_from_slice(&block);
    nor
}

/// Whether this image was made by [`synthesise`] rather than read off an iPod.
pub fn is_synthetic(nor: &[u8]) -> bool {
    nor.get(SYNTH_MARK_AT..SYNTH_MARK_AT + SYNTH_MARK.len()) == Some(SYNTH_MARK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Colour;

    fn model(num: &str) -> &'static Model {
        Model::lookup(num).expect("a known model")
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
        assert_eq!(c.model.as_deref(), Some("A146"));
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
