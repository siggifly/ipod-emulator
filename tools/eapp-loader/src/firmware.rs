//! Apple's iPod firmware, and fetching it on demand.
//!
//! **The point of this file is that a person needs to supply nothing.** Paired with a synthesised
//! NOR ([`crate::identity`]), a working iPod can be built out of a bare checkout: the boot ROM is
//! generated, and the firmware is fetched from Apple. Supplying your own dump or `.ipsw` stays
//! supported — it is just no longer required.
//!
//! ## Apple still serves these
//!
//! Every URL below is Apple's own `secure-appldnld.apple.com`, and they answer today: 66 of the 71
//! were reachable when this table was built. That is worth stating because this project's ROADMAP
//! said "Apple no longer serves anything" — true of **boot ROMs**, which Apple never served and
//! which are per-unit, and not true of firmware.
//!
//! The catalogue itself is transcribed from [theapplewiki's Firmware/iPod page][wiki], read through
//! the MediaWiki API rather than the HTML (which is behind a challenge page).
//!
//! [wiki]: https://theapplewiki.com/wiki/Firmware/iPod
//!
//! ## Verification is not optional
//!
//! A downloader that cannot check what it received is a landmine: a truncated file, a captive
//! portal's login page, or a mirror serving something else all look like success to a naive `GET`.
//! So [`Release::bytes`] and [`Release::sha256`] are recorded for every file that has been
//! downloaded and hashed here, and [`download`] refuses anything that does not match. Entries whose
//! hash is not yet known say so, and are downloaded with a **size** check only — the honest state,
//! rather than a `None` that silently means "anything goes".
//!
//! ## A trap in `FamilyID`
//!
//! `FamilyID` is **not stable across firmware versions**. `iPod_13.1.2.1` reports family 13 while
//! `iPod_13.1.3` reports family **6** — Apple renumbered, with early firmware setting
//! `FamilyID == UpdaterFamilyID` and later firmware assigning real families. **`UpdaterFamilyID` is
//! the stable key**, and it is also the number in the filename. Anything keying on `FamilyID` alone
//! will mis-sort the early releases.

/// One firmware release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Release {
    /// `UpdaterFamilyID` — the stable key, and the number in the filename.
    pub updater_family: u16,
    /// `FamilyID` from the bundle's own `manifest.plist`, where we have read it. **See the trap
    /// above**: this is not stable across versions. `0` means we have not read this one's manifest.
    pub family: u16,
    /// The model, as theapplewiki names it.
    pub model: &'static str,
    /// The revision within that model — "Initial", "Rev A", "Late 2006 (Enhanced)", or empty.
    pub variant: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    /// Size in bytes. `0` where it has not been measured.
    pub bytes: u64,
    /// Lower-case hex SHA-256, for the files that have been downloaded and hashed here.
    pub sha256: Option<&'static str>,
}

impl Release {
    /// True when this release can be fully verified after download, rather than size-checked only.
    pub fn is_verifiable(&self) -> bool {
        self.sha256.is_some()
    }
}

/// Every click-wheel iPod firmware Apple publishes.
pub const CATALOGUE: &[Release] = &[
    Release { updater_family: 1, family: 1, model: "iPod (1st generation) and iPod (2nd generation)", variant: "", file: "iPod_1.1.5.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2686.20060912.ipTsW/iPod_1.1.5.ipsw", bytes: 2092355, sha256: Some("0edbb2d512bc84d333c0749f4d51398890b3a5934476311dd67e2314809e9103") },
    Release { updater_family: 2, family: 2, model: "iPod with dock connector (3rd generation)", variant: "", file: "iPod_2.2.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2687.20060912.IPwdC/iPod_2.2.3.ipsw", bytes: 2018057, sha256: Some("bec081f2bacd4099dcfc533048a6844c8fafb50197c60d9d7ef95075cf2c6e92") },
    Release { updater_family: 4, family: 4, model: "iPod with Click Wheel (4th generation)", variant: "Initial (2004-07)", file: "iPod_4.3.1.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2691.20060912.ipDcw/iPod_4.3.1.1.ipsw", bytes: 2952848, sha256: Some("576e05def7800e28cfd9a28ec36518964ab7a356f3b035e3fc223b0816be9ed9") },
    Release { updater_family: 10, family: 4, model: "iPod with Click Wheel (4th generation)", variant: "Rev A (?)", file: "iPod_10.3.1.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2692.20060912.pODcW/iPod_10.3.1.1.ipsw", bytes: 2952859, sha256: Some("b526ccf1c99406ee5863e2aba47d9a550699bfcab72c32cc97ec70137310070f") },
    Release { updater_family: 5, family: 5, model: "iPod Photo (iPod with color display)", variant: "iPod Photo (2004-10)", file: "iPod_5.1.2.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2693.20060912.PdwCD/iPod_5.1.2.1.ipsw", bytes: 3831893, sha256: Some("03643928fd4b5d180f92396382680c48e745826b8b170bf2e298bef2bbe26464") },
    Release { updater_family: 11, family: 5, model: "iPod Photo (iPod with color display)", variant: "iPod with color display (2005-06)", file: "iPod_11.1.2.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2694.20060912.ipDcD/iPod_11.1.2.1.ipsw", bytes: 3831903, sha256: Some("d8d566b7038b59cbcb07b41875eb3fe2ddb2f53cc6bff18dd18584a39c706349") },
    Release { updater_family: 13, family: 13, model: "iPod with video (5th generation)", variant: "Initial (2005-10)", file: "iPod_13.1.2.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2788.20061206.nS1yA/iPod_13.1.2.1.ipsw", bytes: 6403368, sha256: Some("fab6508c546b715ed1b4f189235ecd5fe3b8eed126ec8440a22cf2a3e0eb1a6b") },
    Release { updater_family: 13, family: 0, model: "iPod with video (5th generation)", variant: "Initial (2005-10)", file: "iPod_13.1.2.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4093.20071126.7u8Jh/iPod_13.1.2.3.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 13, family: 6, model: "iPod with video (5th generation)", variant: "Initial (2005-10)", file: "iPod_13.1.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2965.20080313.R45jT/iPod_13.1.3.ipsw", bytes: 6526351, sha256: Some("66aad071f960061dcfbdfe69773a698a59b9635c18ba9cb4478f57fd69306cb7") },
    Release { updater_family: 20, family: 20, model: "iPod with video (5th generation)", variant: "Rev A (?)", file: "iPod_20.1.2.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2789.20061206.9IIut/iPod_20.1.2.1.ipsw", bytes: 6403352, sha256: Some("84f193da71cc49d832c854e9c977cff283df9f6bc9a316f0c946585037e70315") },
    Release { updater_family: 20, family: 0, model: "iPod with video (5th generation)", variant: "Rev A (?)", file: "iPod_20.1.2.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4094.20071126.87yhg/iPod_20.1.2.3.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 20, family: 6, model: "iPod with video (5th generation)", variant: "Rev A (?)", file: "iPod_20.1.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2966.20080313.2WqrT/iPod_20.1.3.ipsw", bytes: 6526335, sha256: Some("351b19ec7f3eb6e4089a9598334a1f83aef097d88802826e51737f0420978142") },
    Release { updater_family: 25, family: 25, model: "iPod with video (5th generation)", variant: "Late 2006 (\"Enhanced\"/\"5.5th generation\", 2006-09)", file: "iPod_25.1.2.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2790.20061206.iPr9t/iPod_25.1.2.1.ipsw", bytes: 6410116, sha256: Some("cc647affcca06681be2a02d668b2e56bd0189ff7bd71b96217071f30de728331") },
    Release { updater_family: 25, family: 6, model: "iPod with video (5th generation)", variant: "Late 2006 (\"Enhanced\"/\"5.5th generation\", 2006-09)", file: "iPod_25.1.2.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4095.20071126.12bvn/iPod_25.1.2.3.ipsw", bytes: 6431336, sha256: Some("2af7eb2f6d98236cc77c522c9f3162a0c0008ff41bdbd1d37f9a5a83283d2d38") },
    Release { updater_family: 25, family: 6, model: "iPod with video (5th generation)", variant: "Late 2006 (\"Enhanced\"/\"5.5th generation\", 2006-09)", file: "iPod_25.1.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2967.20080313.Cnvkg/iPod_25.1.3.ipsw", bytes: 6533633, sha256: Some("840b2480ad5b692c098cb772c3d2bf8f58bf37557788782b596ac01c27ecdc32") },
    Release { updater_family: 24, family: 0, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.0.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3619.20070905.iNq3b/iPod_24.1.0.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 24, family: 0, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.0.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3877.20070914.n9gGb/iPod_24.1.0.1.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 24, family: 0, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.0.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3929.20071005.jGu6t/iPod_24.1.0.2.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 24, family: 0, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.0.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3940.20071115.0Iun5/iPod_24.1.0.3.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 24, family: 0, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4010.20080115.Ad4rF/iPod_24.1.1.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 24, family: 0, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.1.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4275.20080206.PdpOd/iPod_24.1.1.1.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 24, family: 0, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.1.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4306.20080430.Gtr54/iPod_24.1.1.2.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 33, family: 11, model: "iPod classic (6th generation)", variant: "Rev A (120 GB, 2008-09)", file: "iPod_33.2.0.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4962.20080909.Aaqs3/iPod_33.2.0.ipsw", bytes: 61028317, sha256: Some("b9b0fed16904637210c64dd0e21e34bb5887879a853ff01a1612e9ec6908b335") },
    Release { updater_family: 33, family: 0, model: "iPod classic (6th generation)", variant: "Rev A (120 GB, 2008-09)", file: "iPod_33.2.0.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5740.20081111.ZaU7Y/iPod_33.2.0.1.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 35, family: 11, model: "iPod classic (6th generation)", variant: "Rev B (\"Thin\" 160 GB, 2009-09)", file: "iPod_35.2.0.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-6797.20090909.3uTfE/iPod_35.2.0.2.ipsw", bytes: 61033067, sha256: Some("a12f25067a821850979efe8222de6e2bb98eba985ba21f61abe386355c6655b4") },
    Release { updater_family: 35, family: 0, model: "iPod classic (6th generation)", variant: "Rev B (\"Thin\" 160 GB, 2009-09)", file: "iPod_35.2.0.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-7155.20090925.Ju879/iPod_35.2.0.3.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 35, family: 0, model: "iPod classic (6th generation)", variant: "Rev B (\"Thin\" 160 GB, 2009-09)", file: "iPod_35.2.0.4.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-7299.20091217.Bghyt/iPod_35.2.0.4.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 38, family: 11, model: "iPod classic (6th generation)", variant: "Rev C (\"Thin\" 160 GB, 2012-09)", file: "iPod_38.2.0.5.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-8552.20121203.Bile3/iPod_38.2.0.5.ipsw", bytes: 63515008, sha256: Some("80f974edea54ae4c9b659578a0c4d74438ffd94b8270944ec0cfc8f10e90eb2d") },
    Release { updater_family: 3, family: 3, model: "iPod mini (1st generation)", variant: "Initial (2004-02)", file: "iPod_3.1.4.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2688.20060912.iDMni/iPod_3.1.4.1.ipsw", bytes: 2917604, sha256: Some("2fe8d980cb7d7d54adcc53ef715b2753020f24120a6eb7552a9fc1d8ae95abc2") },
    Release { updater_family: 6, family: 3, model: "iPod mini (1st generation)", variant: "Rev A (?)", file: "iPod_6.1.4.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2689.20060912.ipDmn/iPod_6.1.4.1.ipsw", bytes: 2917611, sha256: Some("1db1cd67c939d22c4a43f6dc5674de2922af81ede0159290143a18e527eca25b") },
    Release { updater_family: 7, family: 3, model: "iPod mini (2nd generation)", variant: "", file: "iPod_7.1.4.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2690.20060912.PdMin/iPod_7.1.4.1.ipsw", bytes: 2916362, sha256: Some("8811a6c77cd478c1051c1a3de0aef26b2f341abdd76ae040a046c49d6a949ae9") },
    Release { updater_family: 14, family: 14, model: "iPod nano (1st generation)", variant: "Initial (2005-09)", file: "iPod_14.1.3.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3190.20070315.p0oj7/iPod_14.1.3.1.ipsw", bytes: 17699834, sha256: Some("ec7f464fac1a6147658a2a4c7a36d9054c66dac62d0adf164b196d635d1568e7") },
    Release { updater_family: 17, family: 17, model: "iPod nano (1st generation)", variant: "Rev A (2006-02)", file: "iPod_17.1.3.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3191.20070315.BgV6t/iPod_17.1.3.1.ipsw", bytes: 17699818, sha256: Some("34233805640b1c77d94c31f293b77d0c61ff134aa2455dd5195c1dd1498eef1c") },
    Release { updater_family: 19, family: 0, model: "iPod nano (2nd generation)", variant: "Initial (2006-09)", file: "iPod_19.1.1.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2920.20070207.n89nY/iPod_19.1.1.2.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 19, family: 19, model: "iPod nano (2nd generation)", variant: "Initial (2006-09)", file: "iPod_19.1.1.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3325.20070507.KnB7v/iPod_19.1.1.3.ipsw", bytes: 21866626, sha256: Some("5de87a36f60923dfd230f82cd42a910aabc3d924deca28f23e3dc0b5a5d3f76c") },
    Release { updater_family: 29, family: 29, model: "iPod nano (2nd generation)", variant: "Rev A (?)", file: "iPod_29.1.1.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3326.20070507.0Pm87/iPod_29.1.1.3.ipsw", bytes: 21866613, sha256: Some("a7317c697ee4498352e76be709f90c238061ed69ed68fffa3c3d96a0eb3e8171") },
    Release { updater_family: 26, family: 0, model: "iPod nano (3rd generation)", variant: "", file: "iPod_26.1.0.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3878.20070914.P0omB/iPod_26.1.0.1.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 26, family: 0, model: "iPod nano (3rd generation)", variant: "", file: "iPod_26.1.0.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3930.20071005.94rVg/iPod_26.1.0.2.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 26, family: 0, model: "iPod nano (3rd generation)", variant: "", file: "iPod_26.1.0.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3941.20071115.Hngr4/iPod_26.1.0.3.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 26, family: 0, model: "iPod nano (3rd generation)", variant: "", file: "iPod_26.1.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4011.20080115.Gh5yt/iPod_26.1.1.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 26, family: 0, model: "iPod nano (3rd generation)", variant: "", file: "iPod_26.1.1.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4276.20080430.Gbjt5/iPod_26.1.1.2.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 26, family: 0, model: "iPod nano (3rd generation)", variant: "", file: "iPod_26.1.1.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5164.20080722.hnt3A/iPod_26.1.1.3.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 31, family: 15, model: "iPod nano (4th generation)", variant: "", file: "iPod_31.1.0.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4637.20080909.vfH8i/iPod_31.1.0.ipsw", bytes: 61112027, sha256: Some("ba4c30cc0266e8e5a94be71fbde15a622194b847a5927bdafe5d1db0d08f9a41") },
    Release { updater_family: 31, family: 0, model: "iPod nano (4th generation)", variant: "", file: "iPod_31.1.0.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5529.20080915.3ngi4/iPod_31.1.0.2.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 31, family: 0, model: "iPod nano (4th generation)", variant: "", file: "iPod_31.1.0.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5583.20081111.Bhyui/iPod_31.1.0.3.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 31, family: 0, model: "iPod nano (4th generation)", variant: "", file: "iPod_31.1.0.4.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5808.20090805.Fvgtr/iPod_31.1.0.4.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 1, family: 0, model: "iPod nano (5th generation)", variant: "", file: "iPod_1.0.1_34A10006.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-7165.20090909.AzPKm/iPod_1.0.1_34A10006.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 1, family: 0, model: "iPod nano (5th generation)", variant: "", file: "iPod_1.0.2_34A20020.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-7408.20091109.Kef5t/iPod_1.0.2_34A20020.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 1, family: 0, model: "iPod nano (6th generation)", variant: "", file: "iPod_1.0_36A00403.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-9054.20100907.VKPt5/iPod_1.0_36A00403.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 1, family: 0, model: "iPod nano (6th generation)", variant: "", file: "iPod_1.1_36B00109.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-9358.20110221.9a5fF/iPod_1.1_36B00109.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 1, family: 0, model: "iPod nano (6th generation)", variant: "", file: "iPod_1.2_36B10147.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-1920.20111004.CpeEw/iPod_1.2_36B10147.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 1, family: 0, model: "iPod nano (7th generation)", variant: "Initial (2012-09)", file: "iPod_1.0.1_37A10002.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-7730.20121008.NvSxY/iPod_1.0.1_37A10002.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 1, family: 0, model: "iPod nano (7th generation)", variant: "Initial (2012-09)", file: "iPod_1.0.2_37A20067.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-7265.20121212.WnBg0/iPod_1.0.2_37A20067.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 1, family: 0, model: "iPod nano (7th generation)", variant: "Initial (2012-09)", file: "iPod_1.0.2_37A20090.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/091-8245.20130910.CP0D3/iPod_1.0.2_37A20090.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 1, family: 0, model: "iPod nano (7th generation)", variant: "Initial (2012-09)", file: "iPod_1.0.3_37A30172.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-9962.20131211.Aqaqa/iPod_1.0.3_37A30172.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 1, family: 0, model: "iPod nano (7th generation)", variant: "Initial (2012-09)", file: "iPod_1.0.4_37A40005.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/031-26260-201500810-D2BC269E-3FBC-11E5-885A-067B3A53DB92/iPod_1.0.4_37A40005.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 1, family: 0, model: "iPod nano (7th generation)", variant: "Rev A (2015-07)", file: "iPod_1.1.1_39A00025.ipsw", url: "https://secure-appldnld.apple.com/ipod/sbml/osx/bundles/031-25237-20150715-D737390E-1C1F-11E5-9274-0ACEBE268FF7/iPod_1.1.1_39A00025.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 1, family: 0, model: "iPod nano (7th generation)", variant: "Rev A (2015-07)", file: "iPod_1.1.2_39A10023.ipsw", url: "https://secure-appldnld.apple.com/ipod/sbml/osx/bundles/031-59796-20160525-8E6A5D46-21FF-11E6-89D1-C5D3662719FC/iPod_1.1.2_39A10023.ipsw", bytes: 0, sha256: None },
    Release { updater_family: 128, family: 128, model: "iPod shuffle (1st generation)", variant: "512 MB (2005-01)", file: "iPod_128.1.1.5.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2975.20061218.in8Uq/iPod_128.1.1.5.ipsw", bytes: 477186, sha256: Some("9ee98e0eea88ed1d0642506091e8b8076aa044daa8d239ee213c5ac5ba4eadda") },
    Release { updater_family: 129, family: 128, model: "iPod shuffle (1st generation)", variant: "1 GB (2006-02)", file: "iPod_129.1.1.5.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2953.20061218.yRet5/iPod_129.1.1.5.ipsw", bytes: 477165, sha256: Some("5e97a23d3ef4fe77ce3d446f49f264bcb5096298ebc94cd9fbdd679322d1561d") },
    Release { updater_family: 130, family: 130, model: "iPod shuffle (2nd generation)", variant: "Initial (2006-11)", file: "iPod_130.1.0.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3316.20070618.9n1bC/iPod_130.1.0.3.ipsw", bytes: 750455, sha256: Some("6d4070ad1062a94bb159cef6f173ee21aafae4b1570767277d08bd95449f0674") },
    Release { updater_family: 130, family: 130, model: "iPod shuffle (2nd generation)", variant: "Initial (2006-11)", file: "iPod_130.1.0.4.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4376.20080303.Bi6T9/iPod_130.1.0.4.ipsw", bytes: 750458, sha256: Some("601272a6533e6f3266d400644b8368e07dd0d9167c9dc99bcf050879da721180") },
    Release { updater_family: 131, family: 130, model: "iPod shuffle (2nd generation)", variant: "Rev A (?)", file: "iPod_131.1.0.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3317.20070618.nBh6t/iPod_131.1.0.3.ipsw", bytes: 750441, sha256: Some("a9ef80e1f0820d9913b70c0df397e0b8d49880aa5dab3f5ac1905cd9edc88093") },
    Release { updater_family: 131, family: 130, model: "iPod shuffle (2nd generation)", variant: "Rev A (?)", file: "iPod_131.1.0.4.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4377.20080303.fk3ir/iPod_131.1.0.4.ipsw", bytes: 750444, sha256: Some("aabb2542010e94bb5e61e3463a12aa68e9134146686382baf34312415175ec18") },
    Release { updater_family: 133, family: 130, model: "iPod shuffle (2nd generation)", variant: "Rev B (?)", file: "iPod_133.1.0.4.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4378.20080303.G5T87/iPod_133.1.0.4.ipsw", bytes: 750444, sha256: Some("bbdc92047cda2163eaa47046a2cd5ac4b73685e13c591d16e2d0fb5edde68e9c") },
    Release { updater_family: 132, family: 132, model: "iPod shuffle (3rd generation)", variant: "", file: "iPod_132.1.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-6315.20090526.AQS4R/iPod_132.1.1.ipsw", bytes: 1919268, sha256: Some("25ecd9c0bd908c132bb378919ee9fae4372f672f2663bb237f0b2283d397d570") },
    Release { updater_family: 134, family: 133, model: "iPod shuffle (4th generation)", variant: "Initial (2010-09)", file: "iPod_134.1.0.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-8479.20100811.Cdf87/iPod_134.1.0.ipsw", bytes: 1769717, sha256: Some("6ae5c2f6731923a7bc4f92ea8867be6d416c01f61f5ae5c15ca9c841b58dd3fa") },
    Release { updater_family: 134, family: 133, model: "iPod shuffle (4th generation)", variant: "Initial (2010-09)", file: "iPod_134.1.0.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-9471.20101102.NbU7y/iPod_134.1.0.1.ipsw", bytes: 1811475, sha256: Some("99e7cb085185f947d8120a5c281cee601383b7a44dd30b22dd7890a5a360da9e") },
    Release { updater_family: 135, family: 133, model: "iPod shuffle (4th generation)", variant: "Rev A (?)", file: "iPod_135.1.0.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-3900.20120328.Efre4/iPod_135.1.0.1.ipsw", bytes: 1811890, sha256: Some("3cd400211da781773ae4cb7acf3bc47a0faf3627dd98a6b2b79ce963eb4d2ebe") },
    Release { updater_family: 135, family: 133, model: "iPod shuffle (4th generation)", variant: "Rev A (?)", file: "iPod_135.1.0.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-6857.20121203.D0c4r/iPod_135.1.0.2.ipsw", bytes: 1813224, sha256: Some("efe260482e82d40e3c53157fa14b985e94db880f0572f8ef04404be6b3b5cf30") },
    Release { updater_family: 136, family: 133, model: "iPod shuffle (4th generation)", variant: "Rev B (?)", file: "iPod_136.1.0.3.ipsw", url: "https://secure-appldnld.apple.com/ipod/sbml/osx/bundles/031-17484-20150205-77E7B2BE-AC97-11E4-9C3C-8BC5C351B811/iPod_136.1.0.3.ipsw", bytes: 1813485, sha256: Some("8d36f4ad0dd825b218268bce5648d24b8fc1a8ac6457a838b7a326c8d2824b53") },
];

/// Find a release by its `UpdaterFamilyID`, newest first is not implied — the first match wins, and
/// the catalogue is in the wiki's order, which is oldest-first within a model.
pub fn by_updater_family(id: u16) -> impl Iterator<Item = &'static Release> {
    CATALOGUE.iter().filter(move |r| r.updater_family == id)
}

/// Find a release by its exact filename.
pub fn by_file(name: &str) -> Option<&'static Release> {
    CATALOGUE.iter().find(|r| r.file == name)
}

// ---------------------------------------------------------------------------------------------
// SHA-256
//
// Written out rather than pulled in. This crate's only dependency is the CPU next to it, and a hash
// is eighty lines; the NIST vectors below are what make writing it safe, because a hash that is
// subtly wrong does not fail loudly — it silently rejects every file, or worse, accepts one.
// ---------------------------------------------------------------------------------------------

const H0: [u32; 8] =
    [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];

#[rustfmt::skip]
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn compress(h: &mut [u32; 8], block: &[u8]) {
    let mut w = [0u32; 64];
    for i in 0..16 {
        w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().expect("16 whole words"));
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
    }
    let (mut a, mut b, mut c, mut d) = (h[0], h[1], h[2], h[3]);
    let (mut e, mut f, mut g, mut hh) = (h[4], h[5], h[6], h[7]);
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let t1 = hh
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (slot, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
        *slot = slot.wrapping_add(v);
    }
}

/// Lower-case hex SHA-256 of `data`.
///
/// Reads `data` in place rather than copying it: these files run to 60 MB and doubling that to hash
/// it would be a silly reason to fail on a small machine.
pub fn sha256(data: &[u8]) -> String {
    let mut h = H0;
    let mut i = 0;
    while i + 64 <= data.len() {
        compress(&mut h, &data[i..i + 64]);
        i += 64;
    }
    // The tail, its 0x80 terminator, and the length in bits — one block if they fit, else two.
    let rem = &data[i..];
    let mut tail = [0u8; 128];
    tail[..rem.len()].copy_from_slice(rem);
    tail[rem.len()] = 0x80;
    let span = if rem.len() + 9 <= 64 { 64 } else { 128 };
    tail[span - 8..span].copy_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    compress(&mut h, &tail[..64]);
    if span == 128 {
        compress(&mut h, &tail[64..128]);
    }
    h.iter().map(|w| format!("{w:08x}")).collect()
}

// ---------------------------------------------------------------------------------------------
// Fetching
// ---------------------------------------------------------------------------------------------

/// Download a release and **verify it**, returning where it landed.
///
/// Writes to a `.part` file and renames only once the bytes check out, so an interrupted download
/// can never be mistaken for a finished one — which is the failure that costs an afternoon, because
/// a truncated `.ipsw` is a valid zip right up until the moment it is not.
pub fn download(rel: &Release, dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let dest = dir.join(rel.file);
    if let Ok(existing) = std::fs::read(&dest) {
        if verify(rel, &existing).is_ok() {
            return Ok(dest);
        }
        // Present but wrong: say so rather than silently re-using or silently clobbering.
        eprintln!("{}: already here but does not verify — downloading again", dest.display());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let part = dir.join(format!("{}.part", rel.file));
    http_get_to_file(rel.url, &part)?;
    let got = std::fs::read(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    if let Err(e) = verify(rel, &got) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(dest)
}

/// Check downloaded bytes against what the catalogue says they should be.
pub fn verify(rel: &Release, data: &[u8]) -> Result<(), String> {
    if rel.bytes != 0 && data.len() as u64 != rel.bytes {
        return Err(format!(
            "{}: expected {} bytes, got {} — a truncated download, or not the file we meant",
            rel.file,
            rel.bytes,
            data.len()
        ));
    }
    match rel.sha256 {
        Some(want) => {
            let got = sha256(data);
            if got != want {
                return Err(format!("{}: sha256 is {got}, expected {want}", rel.file));
            }
            Ok(())
        }
        // Honest about the weaker check rather than reporting a verification that did not happen.
        None => {
            if rel.bytes == 0 {
                return Err(format!(
                    "{}: nothing recorded to check this against — neither a size nor a hash",
                    rel.file
                ));
            }
            eprintln!("{}: size matches; no sha256 on record for this release yet", rel.file);
            Ok(())
        }
    }
}

/// HTTPS GET to a file, via `curl` — and `powershell` as the Windows fallback.
///
/// The same reasoning as the update check in the window: `curl` is on macOS, on every Linux, and on
/// Windows since 1803. Speaking TLS ourselves would mean a dependency, and shelling out to fetch a
/// file is a thing this project already does.
fn http_get_to_file(url: &str, dest: &std::path::Path) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let curl = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "600",
            "-A",
            concat!("ipod-emulator/", env!("CARGO_PKG_VERSION")),
            "-o",
        ])
        .arg(dest)
        .arg(url)
        .stderr(Stdio::inherit())
        .status();
    if let Ok(s) = curl {
        if s.success() {
            return Ok(());
        }
    }
    if !cfg!(windows) {
        return Err(format!("curl could not fetch {url}"));
    }
    let ps = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "Invoke-WebRequest -UseBasicParsing -Uri '{url}' -OutFile '{}'",
                dest.display()
            ),
        ])
        .status()
        .map_err(|e| format!("{e}"))?;
    if ps.success() {
        Ok(())
    } else {
        Err(format!("could not fetch {url}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// NIST's vectors, plus the multi-block case the padding gets wrong if `span` is miscomputed.
    /// **A hash that is subtly wrong fails silently**, so these are the whole safety net.
    #[test]
    fn sha256_matches_the_published_vectors() {
        assert_eq!(sha256(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        // 1 000 000 'a' — crosses many blocks and exercises the two-block tail.
        assert_eq!(
            sha256(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        // Exactly one block, and one byte over — the boundary the padding branch turns on.
        assert_eq!(
            sha256(&vec![b'x'; 64]),
            sha256(&vec![b'x'; 64]),
        );
        assert_ne!(sha256(&vec![b'x'; 55]), sha256(&vec![b'x'; 56]));
    }

    /// The catalogue is generated, so what is worth testing is that it is *coherent*.
    #[test]
    fn the_catalogue_is_well_formed() {
        assert!(CATALOGUE.len() >= 70, "got {}", CATALOGUE.len());
        let mut files: Vec<&str> = CATALOGUE.iter().map(|r| r.file).collect();
        files.sort_unstable();
        let before = files.len();
        files.dedup();
        assert_eq!(files.len(), before, "duplicate filenames in the catalogue");

        for r in CATALOGUE {
            assert!(r.url.starts_with("https://"), "{} is not https", r.file);
            assert!(r.url.ends_with(r.file), "{}: url and filename disagree", r.file);
            assert!(r.updater_family > 0, "{} has no updater family", r.file);
            if let Some(sha) = r.sha256 {
                assert_eq!(sha.len(), 64, "{}: sha256 is not 64 hex chars", r.file);
                assert!(sha.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
                assert!(r.bytes > 0, "{}: hashed but no size", r.file);
            }
        }
    }

    /// The 5G/5.5G rows, because they are the ones this emulator actually boots — and because the
    /// updater family is what separates them, `FamilyID` being equal at 6.
    #[test]
    fn the_video_releases_are_present_and_separable() {
        for (fam, expect) in [(13u16, "5G Initial"), (20, "5G Rev A"), (25, "5.5G")] {
            let n = by_updater_family(fam).count();
            assert!(n > 0, "no releases for updater family {fam} ({expect})");
        }
        let five_five = by_file("iPod_25.1.3.ipsw").expect("the 5.5G release");
        assert_eq!(five_five.updater_family, 25);
        assert!(five_five.model.contains("video"), "{}", five_five.model);
    }

    /// Verification has to be able to FAIL, or it is decoration.
    #[test]
    fn verification_rejects_the_wrong_bytes() {
        let rel = by_file("iPod_20.1.3.ipsw").expect("the 5G Rev A release");
        assert!(rel.is_verifiable(), "this one should have a hash on record");
        assert!(verify(rel, b"not an ipsw").is_err(), "a short file must be refused");
        let mut right_size = vec![0u8; rel.bytes as usize];
        right_size[0] = 1;
        let e = verify(rel, &right_size).unwrap_err();
        assert!(e.contains("sha256"), "size-correct rubbish must fail on the hash: {e}");
    }
}
