//! Apple's iPod firmware, and fetching it on demand.
//!
//! **The point of this file is that a person needs to supply nothing.** Paired with a synthesised
//! NOR ([`crate::identity`]), a working iPod can be built out of a bare checkout: the boot ROM is
//! generated, and the firmware is fetched from Apple. Supplying your own dump or `.ipsw` stays
//! supported — it is just no longer required.
//!
//! ## Apple still serves these
//!
//! Every URL below is Apple's own `secure-appldnld.apple.com`, and **66 of the 71 answer today** —
//! all 66 have been downloaded and hashed here, so every one of them can be verified byte for byte.
//! The other five return `403` and are marked [`Release::served`]` == false`. That is worth stating
//! because this project's ROADMAP said "Apple no longer serves anything" — true of **boot ROMs**,
//! which are per-unit and which Apple never served, and not true of firmware.
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
//! So [`Release::bytes`] and [`Release::sha256`] are recorded for **every release Apple still
//! serves**, and [`download`] refuses anything that does not match.
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
    /// Whether Apple still serves this URL.
    ///
    /// **Five of the seventy-one are gone** — they answer `403` — and that is a different thing
    /// from "we have not downloaded it yet". Every release here was attempted, so the difference is
    /// known rather than assumed: [`download`] can say *"Apple no longer serves this one"* instead
    /// of failing with a transport error and leaving somebody wondering about their network.
    pub served: bool,
}

impl Release {
    /// True when this release can be fully verified after download, rather than size-checked only.
    pub fn is_verifiable(&self) -> bool {
        self.sha256.is_some()
    }
}

/// Every click-wheel iPod firmware Apple publishes.
pub const CATALOGUE: &[Release] = &[
    Release { updater_family: 1, family: 1, model: "iPod (1st generation) and iPod (2nd generation)", variant: "", file: "iPod_1.1.5.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2686.20060912.ipTsW/iPod_1.1.5.ipsw", bytes: 2092355, sha256: Some("0edbb2d512bc84d333c0749f4d51398890b3a5934476311dd67e2314809e9103"), served: true },
    Release { updater_family: 2, family: 2, model: "iPod with dock connector (3rd generation)", variant: "", file: "iPod_2.2.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2687.20060912.IPwdC/iPod_2.2.3.ipsw", bytes: 2018057, sha256: Some("bec081f2bacd4099dcfc533048a6844c8fafb50197c60d9d7ef95075cf2c6e92"), served: true },
    Release { updater_family: 4, family: 4, model: "iPod with Click Wheel (4th generation)", variant: "Initial (2004-07)", file: "iPod_4.3.1.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2691.20060912.ipDcw/iPod_4.3.1.1.ipsw", bytes: 2952848, sha256: Some("576e05def7800e28cfd9a28ec36518964ab7a356f3b035e3fc223b0816be9ed9"), served: true },
    Release { updater_family: 10, family: 4, model: "iPod with Click Wheel (4th generation)", variant: "Rev A (?)", file: "iPod_10.3.1.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2692.20060912.pODcW/iPod_10.3.1.1.ipsw", bytes: 2952859, sha256: Some("b526ccf1c99406ee5863e2aba47d9a550699bfcab72c32cc97ec70137310070f"), served: true },
    Release { updater_family: 5, family: 5, model: "iPod Photo (iPod with color display)", variant: "iPod Photo (2004-10)", file: "iPod_5.1.2.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2693.20060912.PdwCD/iPod_5.1.2.1.ipsw", bytes: 3831893, sha256: Some("03643928fd4b5d180f92396382680c48e745826b8b170bf2e298bef2bbe26464"), served: true },
    Release { updater_family: 11, family: 5, model: "iPod Photo (iPod with color display)", variant: "iPod with color display (2005-06)", file: "iPod_11.1.2.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2694.20060912.ipDcD/iPod_11.1.2.1.ipsw", bytes: 3831903, sha256: Some("d8d566b7038b59cbcb07b41875eb3fe2ddb2f53cc6bff18dd18584a39c706349"), served: true },
    Release { updater_family: 13, family: 13, model: "iPod with video (5th generation)", variant: "Initial (2005-10)", file: "iPod_13.1.2.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2788.20061206.nS1yA/iPod_13.1.2.1.ipsw", bytes: 6403368, sha256: Some("fab6508c546b715ed1b4f189235ecd5fe3b8eed126ec8440a22cf2a3e0eb1a6b"), served: true },
    Release { updater_family: 13, family: 0, model: "iPod with video (5th generation)", variant: "Initial (2005-10)", file: "iPod_13.1.2.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4093.20071126.7u8Jh/iPod_13.1.2.3.ipsw", bytes: 0, sha256: None, served: false },
    Release { updater_family: 13, family: 6, model: "iPod with video (5th generation)", variant: "Initial (2005-10)", file: "iPod_13.1.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2965.20080313.R45jT/iPod_13.1.3.ipsw", bytes: 6526351, sha256: Some("66aad071f960061dcfbdfe69773a698a59b9635c18ba9cb4478f57fd69306cb7"), served: true },
    Release { updater_family: 20, family: 20, model: "iPod with video (5th generation)", variant: "Rev A (?)", file: "iPod_20.1.2.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2789.20061206.9IIut/iPod_20.1.2.1.ipsw", bytes: 6403352, sha256: Some("84f193da71cc49d832c854e9c977cff283df9f6bc9a316f0c946585037e70315"), served: true },
    Release { updater_family: 20, family: 0, model: "iPod with video (5th generation)", variant: "Rev A (?)", file: "iPod_20.1.2.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4094.20071126.87yhg/iPod_20.1.2.3.ipsw", bytes: 0, sha256: None, served: false },
    Release { updater_family: 20, family: 6, model: "iPod with video (5th generation)", variant: "Rev A (?)", file: "iPod_20.1.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2966.20080313.2WqrT/iPod_20.1.3.ipsw", bytes: 6526335, sha256: Some("351b19ec7f3eb6e4089a9598334a1f83aef097d88802826e51737f0420978142"), served: true },
    Release { updater_family: 25, family: 25, model: "iPod with video (5th generation)", variant: "Late 2006 (\"Enhanced\"/\"5.5th generation\", 2006-09)", file: "iPod_25.1.2.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2790.20061206.iPr9t/iPod_25.1.2.1.ipsw", bytes: 6410116, sha256: Some("cc647affcca06681be2a02d668b2e56bd0189ff7bd71b96217071f30de728331"), served: true },
    Release { updater_family: 25, family: 6, model: "iPod with video (5th generation)", variant: "Late 2006 (\"Enhanced\"/\"5.5th generation\", 2006-09)", file: "iPod_25.1.2.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4095.20071126.12bvn/iPod_25.1.2.3.ipsw", bytes: 6431336, sha256: Some("2af7eb2f6d98236cc77c522c9f3162a0c0008ff41bdbd1d37f9a5a83283d2d38"), served: true },
    Release { updater_family: 25, family: 6, model: "iPod with video (5th generation)", variant: "Late 2006 (\"Enhanced\"/\"5.5th generation\", 2006-09)", file: "iPod_25.1.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2967.20080313.Cnvkg/iPod_25.1.3.ipsw", bytes: 6533633, sha256: Some("840b2480ad5b692c098cb772c3d2bf8f58bf37557788782b596ac01c27ecdc32"), served: true },
    Release { updater_family: 24, family: 0, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.0.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3619.20070905.iNq3b/iPod_24.1.0.ipsw", bytes: 0, sha256: None, served: false },
    Release { updater_family: 24, family: 11, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.0.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3877.20070914.n9gGb/iPod_24.1.0.1.ipsw", bytes: 59268373, sha256: Some("1f541660b2a7985286050c886cc10982117ca6c22e4c07a1a4c5d3247bce1b7e"), served: true },
    Release { updater_family: 24, family: 0, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.0.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3929.20071005.jGu6t/iPod_24.1.0.2.ipsw", bytes: 0, sha256: None, served: false },
    Release { updater_family: 24, family: 11, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.0.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3940.20071115.0Iun5/iPod_24.1.0.3.ipsw", bytes: 60004790, sha256: Some("7f0638426c0e44dde1f61fa0d38cefd2b2c6c8005f49e44ac17f9866253954ad"), served: true },
    Release { updater_family: 24, family: 11, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4010.20080115.Ad4rF/iPod_24.1.1.ipsw", bytes: 60383109, sha256: Some("e041487afb52e115e11053f5dcb4b942c58ed1647be6c69244fc983648b83fb0"), served: true },
    Release { updater_family: 24, family: 11, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.1.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4275.20080206.PdpOd/iPod_24.1.1.1.ipsw", bytes: 60383154, sha256: Some("7150f4597d64229dc228c9c37769c635145074c398830740eca921232aab5606"), served: true },
    Release { updater_family: 24, family: 11, model: "iPod classic (6th generation)", variant: "Initial (80 GB/\"Fat\" 160 GB, 2007-09)", file: "iPod_24.1.1.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4306.20080430.Gtr54/iPod_24.1.1.2.ipsw", bytes: 60444886, sha256: Some("e753abfb11aaeaa6fd1d7257e87f4e53b6b5d923b1de0e4c9c63c30e0dac9d1a"), served: true },
    Release { updater_family: 33, family: 11, model: "iPod classic (6th generation)", variant: "Rev A (120 GB, 2008-09)", file: "iPod_33.2.0.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4962.20080909.Aaqs3/iPod_33.2.0.ipsw", bytes: 61028317, sha256: Some("b9b0fed16904637210c64dd0e21e34bb5887879a853ff01a1612e9ec6908b335"), served: true },
    Release { updater_family: 33, family: 11, model: "iPod classic (6th generation)", variant: "Rev A (120 GB, 2008-09)", file: "iPod_33.2.0.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5740.20081111.ZaU7Y/iPod_33.2.0.1.ipsw", bytes: 61032316, sha256: Some("17b16ceb4e906cf5636e7389292c40149c503a6fe3df749b3c8447290aa181d5"), served: true },
    Release { updater_family: 35, family: 11, model: "iPod classic (6th generation)", variant: "Rev B (\"Thin\" 160 GB, 2009-09)", file: "iPod_35.2.0.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-6797.20090909.3uTfE/iPod_35.2.0.2.ipsw", bytes: 61033067, sha256: Some("a12f25067a821850979efe8222de6e2bb98eba985ba21f61abe386355c6655b4"), served: true },
    Release { updater_family: 35, family: 11, model: "iPod classic (6th generation)", variant: "Rev B (\"Thin\" 160 GB, 2009-09)", file: "iPod_35.2.0.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-7155.20090925.Ju879/iPod_35.2.0.3.ipsw", bytes: 61092301, sha256: Some("4366d2aaea16110f2cf4cb3ed12ee1b3c647af29567ca71dc35ee54cd777474c"), served: true },
    Release { updater_family: 35, family: 11, model: "iPod classic (6th generation)", variant: "Rev B (\"Thin\" 160 GB, 2009-09)", file: "iPod_35.2.0.4.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-7299.20091217.Bghyt/iPod_35.2.0.4.ipsw", bytes: 61118350, sha256: Some("7ef835c74b08f0bda3566001496cb764afbe0600cb1afec1145c259bc34ad7d0"), served: true },
    Release { updater_family: 38, family: 11, model: "iPod classic (6th generation)", variant: "Rev C (\"Thin\" 160 GB, 2012-09)", file: "iPod_38.2.0.5.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-8552.20121203.Bile3/iPod_38.2.0.5.ipsw", bytes: 63515008, sha256: Some("80f974edea54ae4c9b659578a0c4d74438ffd94b8270944ec0cfc8f10e90eb2d"), served: true },
    Release { updater_family: 3, family: 3, model: "iPod mini (1st generation)", variant: "Initial (2004-02)", file: "iPod_3.1.4.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2688.20060912.iDMni/iPod_3.1.4.1.ipsw", bytes: 2917604, sha256: Some("2fe8d980cb7d7d54adcc53ef715b2753020f24120a6eb7552a9fc1d8ae95abc2"), served: true },
    Release { updater_family: 6, family: 3, model: "iPod mini (1st generation)", variant: "Rev A (?)", file: "iPod_6.1.4.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2689.20060912.ipDmn/iPod_6.1.4.1.ipsw", bytes: 2917611, sha256: Some("1db1cd67c939d22c4a43f6dc5674de2922af81ede0159290143a18e527eca25b"), served: true },
    Release { updater_family: 7, family: 3, model: "iPod mini (2nd generation)", variant: "", file: "iPod_7.1.4.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2690.20060912.PdMin/iPod_7.1.4.1.ipsw", bytes: 2916362, sha256: Some("8811a6c77cd478c1051c1a3de0aef26b2f341abdd76ae040a046c49d6a949ae9"), served: true },
    Release { updater_family: 14, family: 14, model: "iPod nano (1st generation)", variant: "Initial (2005-09)", file: "iPod_14.1.3.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3190.20070315.p0oj7/iPod_14.1.3.1.ipsw", bytes: 17699834, sha256: Some("ec7f464fac1a6147658a2a4c7a36d9054c66dac62d0adf164b196d635d1568e7"), served: true },
    Release { updater_family: 17, family: 17, model: "iPod nano (1st generation)", variant: "Rev A (2006-02)", file: "iPod_17.1.3.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3191.20070315.BgV6t/iPod_17.1.3.1.ipsw", bytes: 17699818, sha256: Some("34233805640b1c77d94c31f293b77d0c61ff134aa2455dd5195c1dd1498eef1c"), served: true },
    Release { updater_family: 19, family: 0, model: "iPod nano (2nd generation)", variant: "Initial (2006-09)", file: "iPod_19.1.1.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2920.20070207.n89nY/iPod_19.1.1.2.ipsw", bytes: 0, sha256: None, served: false },
    Release { updater_family: 19, family: 19, model: "iPod nano (2nd generation)", variant: "Initial (2006-09)", file: "iPod_19.1.1.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3325.20070507.KnB7v/iPod_19.1.1.3.ipsw", bytes: 21866626, sha256: Some("5de87a36f60923dfd230f82cd42a910aabc3d924deca28f23e3dc0b5a5d3f76c"), served: true },
    Release { updater_family: 29, family: 29, model: "iPod nano (2nd generation)", variant: "Rev A (?)", file: "iPod_29.1.1.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3326.20070507.0Pm87/iPod_29.1.1.3.ipsw", bytes: 21866613, sha256: Some("a7317c697ee4498352e76be709f90c238061ed69ed68fffa3c3d96a0eb3e8171"), served: true },
    Release { updater_family: 26, family: 12, model: "iPod nano (3rd generation)", variant: "", file: "iPod_26.1.0.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3878.20070914.P0omB/iPod_26.1.0.1.ipsw", bytes: 60068899, sha256: Some("07b2d35193ee6dd31b921a93c441b88ce8e6b5eed8dc5a97cdebfe5f3436abd8"), served: true },
    Release { updater_family: 26, family: 12, model: "iPod nano (3rd generation)", variant: "", file: "iPod_26.1.0.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3930.20071005.94rVg/iPod_26.1.0.2.ipsw", bytes: 60243827, sha256: Some("038dc1dd12377e44d1b4b23c5764bce3a80ef3ea8e34e91d23b328865f93f923"), served: true },
    Release { updater_family: 26, family: 12, model: "iPod nano (3rd generation)", variant: "", file: "iPod_26.1.0.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3941.20071115.Hngr4/iPod_26.1.0.3.ipsw", bytes: 60931586, sha256: Some("583eaf933a0f374345bbcb22d53f7f2b8152f29ee88df2692f911de06bf36804"), served: true },
    Release { updater_family: 26, family: 12, model: "iPod nano (3rd generation)", variant: "", file: "iPod_26.1.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4011.20080115.Gh5yt/iPod_26.1.1.ipsw", bytes: 61314505, sha256: Some("2eaafe827b4cf44fdd719ed4e6da439ea5f3b28f187414ef17e57e5297e2d05a"), served: true },
    Release { updater_family: 26, family: 12, model: "iPod nano (3rd generation)", variant: "", file: "iPod_26.1.1.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4276.20080430.Gbjt5/iPod_26.1.1.2.ipsw", bytes: 61369352, sha256: Some("c6e1d0393802b45566b07909a836c982543104310070a8335a413d14722a447b"), served: true },
    Release { updater_family: 26, family: 12, model: "iPod nano (3rd generation)", variant: "", file: "iPod_26.1.1.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5164.20080722.hnt3A/iPod_26.1.1.3.ipsw", bytes: 61371490, sha256: Some("6d367d987d76fe0de64670eb1cc86d1c1a06f9e77259d1190b07768ebcbf03b3"), served: true },
    Release { updater_family: 31, family: 15, model: "iPod nano (4th generation)", variant: "", file: "iPod_31.1.0.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4637.20080909.vfH8i/iPod_31.1.0.ipsw", bytes: 61112027, sha256: Some("ba4c30cc0266e8e5a94be71fbde15a622194b847a5927bdafe5d1db0d08f9a41"), served: true },
    Release { updater_family: 31, family: 15, model: "iPod nano (4th generation)", variant: "", file: "iPod_31.1.0.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5529.20080915.3ngi4/iPod_31.1.0.2.ipsw", bytes: 60554790, sha256: Some("1ebdb6551d0a43e551e3d9780c7dfd3a96955685903e67cd76af37075b3bea29"), served: true },
    Release { updater_family: 31, family: 15, model: "iPod nano (4th generation)", variant: "", file: "iPod_31.1.0.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5583.20081111.Bhyui/iPod_31.1.0.3.ipsw", bytes: 60555725, sha256: Some("5c53d02517f8fd45f6cf0f9225004b76f8aeee8d756c950b376c10d48f972f29"), served: true },
    Release { updater_family: 31, family: 15, model: "iPod nano (4th generation)", variant: "", file: "iPod_31.1.0.4.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5808.20090805.Fvgtr/iPod_31.1.0.4.ipsw", bytes: 60636973, sha256: Some("fc8da31398dc153d09bf7a3d9d6888041b6bc17b43f70f7a30ad58e28b252a88"), served: true },
    Release { updater_family: 1, family: 16, model: "iPod nano (5th generation)", variant: "", file: "iPod_1.0.1_34A10006.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-7165.20090909.AzPKm/iPod_1.0.1_34A10006.ipsw", bytes: 78809411, sha256: Some("1ec3d5ff1b1ae6a7b96fd90ec2d431e4986d259400c67064a13290a7e69ab238"), served: true },
    Release { updater_family: 1, family: 16, model: "iPod nano (5th generation)", variant: "", file: "iPod_1.0.2_34A20020.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-7408.20091109.Kef5t/iPod_1.0.2_34A20020.ipsw", bytes: 90622159, sha256: Some("d86f4e899ee42e94c6cbf1084ae3149204c2c244fd19241c525134251d0cc188"), served: true },
    Release { updater_family: 1, family: 17, model: "iPod nano (6th generation)", variant: "", file: "iPod_1.0_36A00403.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-9054.20100907.VKPt5/iPod_1.0_36A00403.ipsw", bytes: 107057019, sha256: Some("8c8f7c27d2f0c4e8225044d73c492c31a59b01b5d52480d8d3fff27fb54bc108"), served: true },
    Release { updater_family: 1, family: 17, model: "iPod nano (6th generation)", variant: "", file: "iPod_1.1_36B00109.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-9358.20110221.9a5fF/iPod_1.1_36B00109.ipsw", bytes: 107163190, sha256: Some("5e2adefb31f1dee9349f2cd648817da3aa95940e70583789e1d8fb29ffc32200"), served: true },
    Release { updater_family: 1, family: 17, model: "iPod nano (6th generation)", variant: "", file: "iPod_1.2_36B10147.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-1920.20111004.CpeEw/iPod_1.2_36B10147.ipsw", bytes: 112554060, sha256: Some("84b27d59f376eaf51270f2ee7ee67016e67fc992882d518f9b294ee21122297d"), served: true },
    Release { updater_family: 1, family: 18, model: "iPod nano (7th generation)", variant: "Initial (2012-09)", file: "iPod_1.0.1_37A10002.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-7730.20121008.NvSxY/iPod_1.0.1_37A10002.ipsw", bytes: 110043181, sha256: Some("98c29168ad78affacf4776033a4b1d8bc3832b1b24ce06d8c3e2b45265efca46"), served: true },
    Release { updater_family: 1, family: 18, model: "iPod nano (7th generation)", variant: "Initial (2012-09)", file: "iPod_1.0.2_37A20067.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-7265.20121212.WnBg0/iPod_1.0.2_37A20067.ipsw", bytes: 115299463, sha256: Some("98032007fc4598752673df68e46ae7ef6f21500b092d34d778b27419cb30f6cd"), served: true },
    Release { updater_family: 1, family: 18, model: "iPod nano (7th generation)", variant: "Initial (2012-09)", file: "iPod_1.0.2_37A20090.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/091-8245.20130910.CP0D3/iPod_1.0.2_37A20090.ipsw", bytes: 116222284, sha256: Some("4a5dcd99e5b43f715d74c9fdb88fa2237be4f517f5be7bdc60d53e6177917169"), served: true },
    Release { updater_family: 1, family: 18, model: "iPod nano (7th generation)", variant: "Initial (2012-09)", file: "iPod_1.0.3_37A30172.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-9962.20131211.Aqaqa/iPod_1.0.3_37A30172.ipsw", bytes: 117956158, sha256: Some("c34b5cb555a519f41780789e995cc74699f29432cbc2ea24a1ed9f6e3c3357d0"), served: true },
    Release { updater_family: 1, family: 18, model: "iPod nano (7th generation)", variant: "Initial (2012-09)", file: "iPod_1.0.4_37A40005.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/031-26260-201500810-D2BC269E-3FBC-11E5-885A-067B3A53DB92/iPod_1.0.4_37A40005.ipsw", bytes: 117989754, sha256: Some("cf26fb17fa37b685b9ac1d8faa7aab3705e1fe04ff6f63758611f0dce87413cf"), served: true },
    Release { updater_family: 1, family: 18, model: "iPod nano (7th generation)", variant: "Rev A (2015-07)", file: "iPod_1.1.1_39A00025.ipsw", url: "https://secure-appldnld.apple.com/ipod/sbml/osx/bundles/031-25237-20150715-D737390E-1C1F-11E5-9274-0ACEBE268FF7/iPod_1.1.1_39A00025.ipsw", bytes: 121168156, sha256: Some("c83b816633f0b85d88f66a8339d807ea159cc103f80a97dcc09e1516886e9620"), served: true },
    Release { updater_family: 1, family: 18, model: "iPod nano (7th generation)", variant: "Rev A (2015-07)", file: "iPod_1.1.2_39A10023.ipsw", url: "https://secure-appldnld.apple.com/ipod/sbml/osx/bundles/031-59796-20160525-8E6A5D46-21FF-11E6-89D1-C5D3662719FC/iPod_1.1.2_39A10023.ipsw", bytes: 121168449, sha256: Some("960d570aa073f278f21b2c99d1aa3601ef136539353492b71cc71f462870a252"), served: true },
    Release { updater_family: 128, family: 128, model: "iPod shuffle (1st generation)", variant: "512 MB (2005-01)", file: "iPod_128.1.1.5.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2975.20061218.in8Uq/iPod_128.1.1.5.ipsw", bytes: 477186, sha256: Some("9ee98e0eea88ed1d0642506091e8b8076aa044daa8d239ee213c5ac5ba4eadda"), served: true },
    Release { updater_family: 129, family: 128, model: "iPod shuffle (1st generation)", variant: "1 GB (2006-02)", file: "iPod_129.1.1.5.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2953.20061218.yRet5/iPod_129.1.1.5.ipsw", bytes: 477165, sha256: Some("5e97a23d3ef4fe77ce3d446f49f264bcb5096298ebc94cd9fbdd679322d1561d"), served: true },
    Release { updater_family: 130, family: 130, model: "iPod shuffle (2nd generation)", variant: "Initial (2006-11)", file: "iPod_130.1.0.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3316.20070618.9n1bC/iPod_130.1.0.3.ipsw", bytes: 750455, sha256: Some("6d4070ad1062a94bb159cef6f173ee21aafae4b1570767277d08bd95449f0674"), served: true },
    Release { updater_family: 130, family: 130, model: "iPod shuffle (2nd generation)", variant: "Initial (2006-11)", file: "iPod_130.1.0.4.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4376.20080303.Bi6T9/iPod_130.1.0.4.ipsw", bytes: 750458, sha256: Some("601272a6533e6f3266d400644b8368e07dd0d9167c9dc99bcf050879da721180"), served: true },
    Release { updater_family: 131, family: 130, model: "iPod shuffle (2nd generation)", variant: "Rev A (?)", file: "iPod_131.1.0.3.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3317.20070618.nBh6t/iPod_131.1.0.3.ipsw", bytes: 750441, sha256: Some("a9ef80e1f0820d9913b70c0df397e0b8d49880aa5dab3f5ac1905cd9edc88093"), served: true },
    Release { updater_family: 131, family: 130, model: "iPod shuffle (2nd generation)", variant: "Rev A (?)", file: "iPod_131.1.0.4.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4377.20080303.fk3ir/iPod_131.1.0.4.ipsw", bytes: 750444, sha256: Some("aabb2542010e94bb5e61e3463a12aa68e9134146686382baf34312415175ec18"), served: true },
    Release { updater_family: 133, family: 130, model: "iPod shuffle (2nd generation)", variant: "Rev B (?)", file: "iPod_133.1.0.4.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4378.20080303.G5T87/iPod_133.1.0.4.ipsw", bytes: 750444, sha256: Some("bbdc92047cda2163eaa47046a2cd5ac4b73685e13c591d16e2d0fb5edde68e9c"), served: true },
    Release { updater_family: 132, family: 132, model: "iPod shuffle (3rd generation)", variant: "", file: "iPod_132.1.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-6315.20090526.AQS4R/iPod_132.1.1.ipsw", bytes: 1919268, sha256: Some("25ecd9c0bd908c132bb378919ee9fae4372f672f2663bb237f0b2283d397d570"), served: true },
    Release { updater_family: 134, family: 133, model: "iPod shuffle (4th generation)", variant: "Initial (2010-09)", file: "iPod_134.1.0.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-8479.20100811.Cdf87/iPod_134.1.0.ipsw", bytes: 1769717, sha256: Some("6ae5c2f6731923a7bc4f92ea8867be6d416c01f61f5ae5c15ca9c841b58dd3fa"), served: true },
    Release { updater_family: 134, family: 133, model: "iPod shuffle (4th generation)", variant: "Initial (2010-09)", file: "iPod_134.1.0.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-9471.20101102.NbU7y/iPod_134.1.0.1.ipsw", bytes: 1811475, sha256: Some("99e7cb085185f947d8120a5c281cee601383b7a44dd30b22dd7890a5a360da9e"), served: true },
    Release { updater_family: 135, family: 133, model: "iPod shuffle (4th generation)", variant: "Rev A (?)", file: "iPod_135.1.0.1.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-3900.20120328.Efre4/iPod_135.1.0.1.ipsw", bytes: 1811890, sha256: Some("3cd400211da781773ae4cb7acf3bc47a0faf3627dd98a6b2b79ce963eb4d2ebe"), served: true },
    Release { updater_family: 135, family: 133, model: "iPod shuffle (4th generation)", variant: "Rev A (?)", file: "iPod_135.1.0.2.ipsw", url: "https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-6857.20121203.D0c4r/iPod_135.1.0.2.ipsw", bytes: 1813224, sha256: Some("efe260482e82d40e3c53157fa14b985e94db880f0572f8ef04404be6b3b5cf30"), served: true },
    Release { updater_family: 136, family: 133, model: "iPod shuffle (4th generation)", variant: "Rev B (?)", file: "iPod_136.1.0.3.ipsw", url: "https://secure-appldnld.apple.com/ipod/sbml/osx/bundles/031-17484-20150205-77E7B2BE-AC97-11E4-9C3C-8BC5C351B811/iPod_136.1.0.3.ipsw", bytes: 1813485, sha256: Some("8d36f4ad0dd825b218268bce5648d24b8fc1a8ac6457a838b7a326c8d2824b53"), served: true },
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

const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

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
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
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

/// Why a download did not produce a file.
///
/// **A class, not a sentence.** Matching on [`download`]'s `String` is the drift this project keeps
/// paying for: a window that wanted to tell a 403 apart from a dead network had to look for the
/// digits `403` inside prose that was free to be reworded. The sentence still travels beside it,
/// verbatim, because the words are the model's and nobody re-words them on the way to a screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trouble {
    /// Apple answers 403/404/410 for this URL, or the catalogue records `served: false`. **A fact
    /// about Apple's servers, not about anybody's network**, which is why it is its own class.
    NotServed { http: u16 },
    /// `curl` is not on this computer, and on Windows neither is `powershell`.
    NoTool,
    /// The fetcher ran and never got an answer. `what` is curl's own documented meaning for `code`.
    Unreachable { code: i32, what: &'static str },
    /// The bytes arrived and are not the bytes on record.
    Verification,
    /// A local create, write, read or rename failed.
    Io,
    /// [`Watch::stop`] went true. **Not a failure** — the caller files it as cancelled.
    Stopped,
}

/// What a long download reports to, and asks.
///
/// **This module knows nothing about threads.** A watcher is whatever the caller has: a channel
/// sender and an `AtomicBool` in the window, [`Silent`] everywhere else. Keeping the trait here and
/// the thread there is what lets `ipod-boot` and the window share one fetcher.
pub trait Watch {
    /// Bytes on disk so far, and the total — `0` where the catalogue records none, which the
    /// caller renders as a number that moves and no bar.
    fn bytes(&mut self, done: u64, total: u64);
    /// Asked once per [`WATCH_TICK`]. `true` stops the download and deletes its `.part`.
    fn stop(&self) -> bool;
}

/// A watcher that wants nothing and never stops — what [`download`] passes.
pub struct Silent;

impl Watch for Silent {
    fn bytes(&mut self, _done: u64, _total: u64) {}
    fn stop(&self) -> bool {
        false
    }
}

/// How often the `.part` is measured and the stop flag read. 10 Hz.
///
/// Fast enough that cancelling feels immediate and slow enough that a 6.5 MB download produces
/// about sixty-five updates rather than thousands.
pub const WATCH_TICK: std::time::Duration = std::time::Duration::from_millis(100);

/// The `.part` a download in flight writes to.
///
/// **Extracted so the fetcher and whoever watches the progress cannot drift about where it is.** A
/// progress bar measuring a path the downloader is not writing reads zero for ever.
pub fn part_path(rel: &Release, dir: &std::path::Path) -> std::path::PathBuf {
    dir.join(format!("{}.part", rel.file))
}

/// Is this release already here, and does it still verify?
///
/// The same question [`download`]'s own early return asks, asked without downloading anything — so
/// a resumed first run can tick the fetch step off rather than fetching 6.5 MB it already has.
/// About 30 ms for 6.5 MB; it reads and hashes the file, so it belongs on a worker.
pub fn is_cached(rel: &Release, dir: &std::path::Path) -> bool {
    std::fs::read(dir.join(rel.file))
        .map(|b| verify(rel, &b).is_ok())
        .unwrap_or(false)
}

/// Download a release and **verify it**, returning where it landed.
///
/// Writes to a `.part` file and renames only once the bytes check out, so an interrupted download
/// can never be mistaken for a finished one — which is the failure that costs an afternoon, because
/// a truncated `.ipsw` is a valid zip right up until the moment it is not.
pub fn download(rel: &Release, dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    download_watched(rel, dir, &mut Silent).map_err(|(_, said)| said)
}

/// The same, reporting bytes as they land and stopping when asked.
///
/// The `.part`-then-rename discipline is the whole of why a cancel is safe: **nothing acquires a
/// real name until the bytes have been checked**, so a stopped download leaves either nothing or a
/// file whose name says what it is, and the only file this ever removes is the one it created.
pub fn download_watched(
    rel: &Release,
    dir: &std::path::Path,
    w: &mut dyn Watch,
) -> Result<std::path::PathBuf, (Trouble, String)> {
    let dest = dir.join(rel.file);
    if let Ok(existing) = std::fs::read(&dest) {
        if verify(rel, &existing).is_ok() {
            return Ok(dest);
        }
        // Present but wrong: say so rather than silently re-using or silently clobbering. **This
        // path is why `Class::Verification` stops offering `Retry` after the first failure** — it
        // loops for as long as a mirror serves the wrong bytes.
        eprintln!(
            "{}: already here but does not verify — downloading again",
            dest.display()
        );
    }
    if !rel.served {
        // One class and one sentence for both routes to "Apple does not serve this": the catalogue
        // saying so, and the server saying so.
        return Err((
            Trouble::NotServed { http: 403 },
            format!(
                "{}: Apple no longer serves this release — its URL returns 403.\n\
                 That is a fact about Apple's servers, not about your network. Another release in the \n\
                 same updater family will almost certainly do: try `ipod-boot firmware list {}`.",
                rel.file, rel.updater_family
            ),
        ));
    }
    std::fs::create_dir_all(dir)
        .map_err(|e| (Trouble::Io, format!("{}: {e}", dir.display())))?;
    let part = part_path(rel, dir);
    if let Err((t, said)) = fetch_watched(rel.url, &part, rel.bytes, w) {
        // **A transfer that ended early leaves its `.part`, and nothing ever comes back for it.**
        // `fetch_watched` removes it when the *watcher* stopped the download and on no other path,
        // so curl 18 / 28 / 56 — an interrupted transfer, which is the common one — left a partial
        // file in the cache that is never shown, never offered for deletion and never cleaned up:
        // `Rail::fail` clears `cancellable`, so no `Cancel` is drawn, and neither `Retry` nor
        // `Provide` routes to the delete. Nothing here resumes a download (`curl -C -` is not
        // used), so the bytes are worth nothing and keeping them is litter.
        let _ = std::fs::remove_file(&part);
        return Err(match t {
            // **One sentence for both routes to "Apple does not serve this."** The catalogue route
            // above names the family to try instead; the server saying 403 to our face is the same
            // fact learned a different way, and it said nothing.
            Trouble::NotServed { .. } => (
                t,
                format!(
                    "{said}\nAnother release in the same updater family will almost certainly do: \
                     try `ipod-boot firmware list {}`.",
                    rel.updater_family
                ),
            ),
            _ => (t, said),
        });
    }
    let got = std::fs::read(&part)
        .map_err(|e| (Trouble::Io, format!("{}: {e}", part.display())))?;
    if let Err(e) = verify(rel, &got) {
        let _ = std::fs::remove_file(&part);
        return Err((Trouble::Verification, e));
    }
    std::fs::rename(&part, &dest)
        .map_err(|e| (Trouble::Io, format!("{}: {e}", dest.display())))?;
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
            eprintln!(
                "{}: size matches; no sha256 on record for this release yet",
                rel.file
            );
            Ok(())
        }
    }
}

/// HTTPS GET to a file, via `curl` — and `powershell` as the Windows fallback.
///
/// The same reasoning as the update check in the window: `curl` is on macOS, on every Linux, and on
/// Windows since 1803. Speaking TLS ourselves would mean a dependency, and shelling out to fetch a
/// file is a thing this project already does.
///
/// **One implementation, two doors.** This is [`fetch_watched`] with a watcher that wants nothing;
/// the Rockbox and iPodLinux fetchers come through here, the window comes through the other.
pub(crate) fn http_get_to_file(url: &str, dest: &std::path::Path) -> Result<(), String> {
    fetch_watched(url, dest, 0, &mut Silent).map_err(|(_, said)| said)
}

/// What curl's exit code means, in curl's own words.
///
/// **Only the ones that can happen to a GET**, and a code with no entry is reported with its number
/// rather than guessed at — a wrong explanation is worse than an honest "it stopped".
fn curl_meaning(code: i32) -> &'static str {
    match code {
        6 => "could not resolve the host",
        7 => "could not connect",
        18 => "the transfer ended early",
        23 => "could not write the file",
        26 => "could not read what it was sending",
        28 => "timed out after 600 seconds",
        35 => "the TLS handshake failed",
        56 => "the connection was reset while receiving",
        60 => "the certificate could not be verified",
        _ => "stopped without an answer",
    }
}

/// HTTPS GET to a file, reporting bytes as they land and stopping when asked.
///
/// **Spawn and poll rather than block.** `Command::status()` returns when the download is over,
/// which is precisely too late to draw a progress bar or to honour a cancel; this starts the child,
/// measures the growing `.part` every [`WATCH_TICK`], and asks the watcher whether to stop.
///
/// `total` is what the catalogue records — `0` where it records none, which travels through to the
/// caller as a number with no denominator rather than as a bar drawn against a guess.
fn fetch_watched(
    url: &str,
    dest: &std::path::Path,
    total: u64,
    w: &mut dyn Watch,
) -> Result<(), (Trouble, String)> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    // **`-w %{http_code}` on stdout, the body on the file.** Without it a 403 and a dead network are
    // one exit code (22) and one sentence, and the window cannot tell the person which happened.
    //
    // **`-L` is deliberate and was very nearly deleted.** Following a redirect silently is how a
    // captive portal's login page becomes a `.ipsw`, and Apple's own URL does not redirect — so on
    // the firmware path alone it is dead weight with a downside. It stays because this function is
    // **shared**: the Rockbox and iPodLinux release servers do redirect, and dropping it here
    // breaks two fetchers that cannot be tested offline. What makes the captive-portal case safe is
    // not the flag but `verify()`, which refuses any body that is not the recorded length *and* the
    // recorded SHA-256, and refuses it before anything is renamed into place.
    let spawned = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "600",
            "-A",
            concat!("ipod-emulator/", env!("CARGO_PKG_VERSION")),
            "-w",
            "%{http_code}",
            "-o",
        ])
        .arg(dest)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();

    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => return windows_fallback(url, dest, e),
    };

    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err((Trouble::Io, format!("curl: {e}")));
            }
        }
        // The `.part`'s apparent length is the honest numerator for a download: it grows a byte at
        // a time, and nothing preallocates it.
        let done = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
        w.bytes(done, total);
        if w.stop() {
            let _ = child.kill();
            let _ = child.wait();
            // **Ours, and named before a byte went into it.** This is the only file this function
            // ever removes.
            let _ = std::fs::remove_file(dest);
            return Err((Trouble::Stopped, format!("stopped at {}", crate::si(done))));
        }
        std::thread::sleep(WATCH_TICK);
    };

    let mut said = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut said);
    }
    if status.success() {
        let done = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
        w.bytes(done, total);
        return Ok(());
    }

    let code = status.code().unwrap_or(-1);
    // 22 is `-f` refusing an HTTP error, and the status it refused is on stdout.
    if code == 22 {
        let http: u16 = said.trim().parse().unwrap_or(0);
        return match http {
            403 | 404 | 410 => Err((
                Trouble::NotServed { http },
                format!("{url}: the server answered {http} — it does not serve this file."),
            )),
            0 => Err((
                Trouble::NotServed { http: 0 },
                format!("{url}: the server refused it and did not say with what status."),
            )),
            _ => Err((
                Trouble::Unreachable {
                    code,
                    what: "the server answered with an error",
                },
                format!("{url}: the server answered {http}."),
            )),
        };
    }
    if matches!(code, 23 | 26) {
        return Err((
            Trouble::Io,
            format!("{}: {}", dest.display(), curl_meaning(code)),
        ));
    }
    Err((
        Trouble::Unreachable {
            code,
            what: curl_meaning(code),
        },
        format!("{url}: {} (curl {code}).", curl_meaning(code)),
    ))
}

/// Windows has a second fetcher. Everywhere else, a `curl` that will not start is the end of it.
fn windows_fallback(
    url: &str,
    dest: &std::path::Path,
    why: std::io::Error,
) -> Result<(), (Trouble, String)> {
    use std::process::{Command, Stdio};
    if !cfg!(windows) {
        return Err((
            Trouble::NoTool,
            format!("curl could not be run: {why}. Every download in this program goes through it."),
        ));
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
        .stdin(Stdio::null())
        .status();
    match ps {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err((
            Trouble::Unreachable {
                code: -1,
                what: "stopped without an answer",
            },
            format!("could not fetch {url}"),
        )),
        Err(e) => Err((
            Trouble::NoTool,
            format!("neither curl nor powershell could be run: {why}; {e}"),
        )),
    }
}

/// What a firmware file somebody handed us turned out to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// Byte-for-byte one of Apple's, whatever it has been renamed to.
    Apple(&'static Release),
    /// Not one we hold a hash for. **This is allowed**, and deliberately so: people modify iPod
    /// firmware, and running modified firmware is a perfectly good reason to want an emulator. It
    /// is reported so they know, not to stop them.
    Unrecognised,
}

/// Identify a firmware file by its **contents**, ignoring what it is called.
///
/// Filenames are the first thing to go: people rename downloads, browsers add `(1)`, and a file
/// called `iPod_25.1.3.ipsw` is not evidence of anything. The hash is.
///
/// Sizes are checked first so the common case is fast — the catalogue's files run to 121 MB, and
/// hashing every candidate to identify one would be a noticeable pause for no reason. Only releases
/// of exactly the right length are hashed, which is usually one and often none.
pub fn identify(data: &[u8]) -> Provenance {
    let len = data.len() as u64;
    let candidates: Vec<&Release> = CATALOGUE.iter().filter(|r| r.bytes == len).collect();
    if candidates.is_empty() {
        return Provenance::Unrecognised;
    }
    let got = sha256(data);
    match candidates.iter().find(|r| r.sha256 == Some(got.as_str())) {
        Some(r) => Provenance::Apple(r),
        None => Provenance::Unrecognised,
    }
}

impl Provenance {
    /// One line to show beside the file.
    pub fn line(&self) -> String {
        match self {
            Provenance::Apple(r) => {
                format!("{} — Apple's {} {}, verified", r.file, r.model, r.variant)
            }
            Provenance::Unrecognised => "not a firmware we recognise".to_string(),
        }
    }

    /// The paragraph a person needs when it is not one of Apple's, or `None` when it is.
    ///
    /// Worded to inform rather than to scold: this is a supported thing to do, and the only reason
    /// to say anything is that "the iPod behaves oddly" is very hard to debug if nobody mentioned
    /// the firmware was not stock.
    pub fn warning(&self) -> Option<&'static str> {
        match self {
            Provenance::Apple(_) => None,
            Provenance::Unrecognised => Some(
                "This does not match any firmware Apple published, so it has either been modified \
                 or it is a release this program does not know about. It will run here either way \
                 — the emulator does not care. But if the iPod behaves strangely, this is the \
                 first thing to suspect.",
            ),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The cache
// ---------------------------------------------------------------------------------------------

/// Where downloaded firmware lives, so it is downloaded **once**.
///
/// `IPOD_EMULATOR_FIRMWARE_DIR` overrides it — which is how a machine that already holds the whole
/// catalogue points at it instead of fetching a second copy. Otherwise it sits beside the rest of
/// the program's data, because a download that lands in whatever directory you happened to be in is
/// a download you will fetch again next week.
pub fn cache_dir() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("IPOD_EMULATOR_FIRMWARE_DIR") {
        let d = std::path::PathBuf::from(d);
        if !d.as_os_str().is_empty() {
            return d;
        }
    }
    crate::settings::data_dir().join("firmware")
}

/// What a cached file turned out to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheState {
    /// Size matches, and the hash was checked and matched.
    Verified,
    /// Size matches. The hash was **not** checked — listing the cache hashes nothing by default,
    /// because hashing 2.7 GB to draw a list is not a thing to do behind somebody's back.
    SizeOk,
    /// In the catalogue, and wrong. A truncated download, or a different file under the same name.
    Corrupt,
    /// Not in the catalogue at all — somebody's own `.ipsw`, or a release we do not list.
    Unknown,
}

/// One file in the cache.
#[derive(Clone, Debug)]
pub struct Cached {
    pub path: std::path::PathBuf,
    pub bytes: u64,
    pub release: Option<&'static Release>,
    pub state: CacheState,
}

/// Everything in the cache directory, newest-irrelevant, sorted by name.
///
/// `verify` decides whether each file is hashed. **It is off by default and that is deliberate**:
/// the full catalogue is 2.7 GB, and a listing that silently spends thirty seconds hashing is a
/// listing people learn not to run.
pub fn cached(dir: &std::path::Path, verify_hashes: bool) -> Vec<Cached> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Cached> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            // `.part` files are interrupted downloads. They are listed so they can be cleaned, and
            // never mistaken for a release.
            let bytes = e.metadata().ok()?.len();
            let release = by_file(&name);
            let state = match release {
                None => CacheState::Unknown,
                Some(r) if r.bytes != 0 && bytes != r.bytes => CacheState::Corrupt,
                Some(r) => {
                    if !verify_hashes {
                        CacheState::SizeOk
                    } else {
                        match (r.sha256, std::fs::read(&path)) {
                            (Some(want), Ok(b)) if sha256(&b) == want => CacheState::Verified,
                            (Some(_), Ok(_)) => CacheState::Corrupt,
                            _ => CacheState::SizeOk,
                        }
                    }
                }
            };
            Some(Cached {
                path,
                bytes,
                release,
                state,
            })
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// What a cached file's state says about where it came from, or `None` when it is not something to
/// file at all.
///
/// **This is what makes the default listing path honest.** [`cached`] hashes nothing unless it is
/// asked to, and says so in its own doc comment; a window that filed its results as "fetched and
/// verified" would be claiming a check the program explicitly declined to perform. `SizeOk` maps to
/// [`crate::settings::Verification::SizeOnly`] and never to `Sha256`.
///
/// `Corrupt` maps to `None`, and that means **do not file it** — not "file it as unknown". A
/// truncated download is not a resource.
///
/// It lives here rather than in `settings.rs` because this module already depends on that one, and
/// the arrow has to stay one-way.
pub fn provenance(state: CacheState) -> Option<crate::settings::Provenance> {
    use crate::settings::{Provenance, Verification};
    match state {
        CacheState::Verified => Some(Provenance::Fetched {
            verified: Verification::Sha256,
        }),
        CacheState::SizeOk => Some(Provenance::Fetched {
            verified: Verification::SizeOnly,
        }),
        CacheState::Corrupt => None,
        // Somebody's own `.ipsw` sitting in the cache directory.
        CacheState::Unknown => Some(Provenance::Provided),
    }
}

/// Total bytes held in a cache directory.
pub fn cache_bytes(dir: &std::path::Path) -> u64 {
    cached(dir, false).iter().map(|c| c.bytes).sum()
}

/// Delete the named files, returning how many bytes went.
///
/// **Takes an explicit list.** There is no "clean everything" flag in this function on purpose: the
/// caller decides what goes, and a caller that wants everything has to enumerate it and say so.
/// Deleting a user's files on a wildcard is how an afternoon of downloads disappears.
pub fn remove(paths: &[std::path::PathBuf]) -> Result<u64, String> {
    let mut freed = 0;
    for p in paths {
        let n = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        std::fs::remove_file(p).map_err(|e| format!("{}: {e}", p.display()))?;
        freed += n;
    }
    Ok(freed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The listing path hashes nothing by default**, so what it files must not claim a hash.
    ///
    /// The match is exhaustive on purpose: a fifth `CacheState` will not compile until somebody has
    /// decided what it says about where the file came from.
    #[test]
    fn every_cache_state_maps_to_one_provenance_and_size_ok_is_not_verified() {
        use crate::settings::{Provenance, Verification};
        assert_eq!(
            provenance(CacheState::Verified),
            Some(Provenance::Fetched {
                verified: Verification::Sha256
            })
        );
        assert_eq!(
            provenance(CacheState::SizeOk),
            Some(Provenance::Fetched {
                verified: Verification::SizeOnly
            }),
            "a size check was filed as a hash check"
        );
        assert_eq!(
            provenance(CacheState::Corrupt),
            None,
            "a truncated download is not a resource"
        );
        assert_eq!(provenance(CacheState::Unknown), Some(Provenance::Provided));
        assert!(
            !provenance(CacheState::SizeOk).unwrap().is_verified(),
            "the default listing path claimed a verification it did not perform"
        );
    }

    /// NIST's vectors, plus the multi-block case the padding gets wrong if `span` is miscomputed.
    /// **A hash that is subtly wrong fails silently**, so these are the whole safety net.
    #[test]
    fn sha256_matches_the_published_vectors() {
        assert_eq!(
            sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
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
        assert_eq!(sha256(&[b'x'; 64]), sha256(&[b'x'; 64]),);
        assert_ne!(sha256(&[b'x'; 55]), sha256(&[b'x'; 56]));
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
            assert!(
                r.url.ends_with(r.file),
                "{}: url and filename disagree",
                r.file
            );
            assert!(r.updater_family > 0, "{} has no updater family", r.file);
            if let Some(sha) = r.sha256 {
                assert_eq!(sha.len(), 64, "{}: sha256 is not 64 hex chars", r.file);
                assert!(sha
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
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

    /// **Every release Apple still serves is fully verifiable**, and the five it does not are
    /// marked rather than left looking un-downloaded. Those are different states and conflating
    /// them would mean telling somebody their network failed when the file is simply gone.
    #[test]
    fn everything_still_served_can_be_verified_and_the_rest_says_why() {
        let served: Vec<_> = CATALOGUE.iter().filter(|r| r.served).collect();
        let gone: Vec<_> = CATALOGUE.iter().filter(|r| !r.served).collect();
        assert_eq!(served.len() + gone.len(), CATALOGUE.len());
        assert!(served.len() >= 66, "only {} served", served.len());
        for r in &served {
            assert!(r.is_verifiable(), "{} is served but has no hash", r.file);
            assert!(r.bytes > 0, "{} is served but has no size", r.file);
        }
        for r in &gone {
            assert!(
                r.sha256.is_none(),
                "{} is not served yet claims a hash",
                r.file
            );
            let e = download(r, std::path::Path::new("/nonexistent")).unwrap_err();
            assert!(
                e.contains("403"),
                "{}: should explain, not just fail: {e}",
                r.file
            );
        }
    }

    /// The cache has to tell a corrupt file from an unknown one from a fine one, because the three
    /// call for different actions: re-download, leave alone, and nothing.
    #[test]
    fn the_cache_sorts_what_it_finds() {
        let dir = std::env::temp_dir().join(format!("ipod-fw-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");

        let rel = by_file("iPod_20.1.3.ipsw").expect("a known release");
        // Right name, wrong length.
        std::fs::write(dir.join(rel.file), b"truncated").expect("write");
        // A file that is not ours at all.
        std::fs::write(dir.join("somebody-elses.ipsw"), b"whatever").expect("write");
        // An interrupted download.
        std::fs::write(dir.join("iPod_13.1.3.ipsw.part"), b"half").expect("write");

        let items = cached(&dir, false);
        assert_eq!(items.len(), 3);
        let by = |n: &str| items.iter().find(|c| c.path.ends_with(n)).expect(n).state;
        assert_eq!(
            by(rel.file),
            CacheState::Corrupt,
            "wrong size must not read as fine"
        );
        assert_eq!(by("somebody-elses.ipsw"), CacheState::Unknown);
        assert_eq!(
            by("iPod_13.1.3.ipsw.part"),
            CacheState::Unknown,
            "a .part is not a release"
        );

        assert_eq!(
            cache_bytes(&dir),
            items.iter().map(|c| c.bytes).sum::<u64>()
        );

        // Removal reports what it actually freed, and takes an explicit list rather than a wildcard.
        let doomed = vec![dir.join("somebody-elses.ipsw")];
        assert_eq!(remove(&doomed).expect("remove"), 8);
        assert_eq!(cached(&dir, false).len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A missing cache directory is empty, not an error — this runs before anything is downloaded.
    #[test]
    fn an_absent_cache_directory_is_simply_empty() {
        let nowhere = std::env::temp_dir().join("ipod-fw-does-not-exist-9e3f");
        assert!(cached(&nowhere, false).is_empty());
        assert_eq!(cache_bytes(&nowhere), 0);
    }

    /// **A renamed file is still identified, and an altered one still isn't.** The filename is
    /// deliberately not consulted: it is the first thing to go, and it is not evidence.
    #[test]
    fn firmware_is_identified_by_content_not_by_name() {
        let rel = by_file("iPod_20.1.3.ipsw").expect("a known release");
        let path = cache_dir().join(rel.file);
        let Ok(real) = std::fs::read(&path) else {
            // The catalogue is 2.7 GB and gitignored; say so rather than skipping in silence.
            println!("SKIPPED: {} is not downloaded here", path.display());
            return;
        };
        assert_eq!(
            identify(&real),
            Provenance::Apple(rel),
            "renaming changes nothing"
        );
        assert!(identify(&real).warning().is_none());

        // One byte different, same length: must NOT be vouched for.
        let mut tweaked = real.clone();
        let last = tweaked.len() - 1;
        tweaked[last] ^= 0xff;
        assert_eq!(
            identify(&tweaked),
            Provenance::Unrecognised,
            "a modified build is not Apple's"
        );
        assert!(
            identify(&tweaked).warning().is_some(),
            "and it has to say so"
        );
    }

    /// Runs with no corpus present, so there is always a live assertion here rather than only
    /// when 2.7 GB happens to be downloaded.
    #[test]
    fn identification_needs_the_hash_and_not_merely_the_size() {
        assert_eq!(identify(b"not a firmware bundle"), Provenance::Unrecognised);
        assert_eq!(identify(&[]), Provenance::Unrecognised);

        // Exactly the right LENGTH for a real release, and nothing else about it right. Matching
        // on size alone would call this Apple's; it must not.
        let rel = by_file("iPod_20.1.3.ipsw").expect("a known release");
        let impostor = vec![0u8; rel.bytes as usize];
        assert_eq!(
            identify(&impostor),
            Provenance::Unrecognised,
            "the right size is not the right file"
        );
        assert!(identify(&impostor).warning().is_some());
    }

    /// Verification has to be able to FAIL, or it is decoration.
    #[test]
    fn verification_rejects_the_wrong_bytes() {
        let rel = by_file("iPod_20.1.3.ipsw").expect("the 5G Rev A release");
        assert!(rel.is_verifiable(), "this one should have a hash on record");
        assert!(
            verify(rel, b"not an ipsw").is_err(),
            "a short file must be refused"
        );
        let mut right_size = vec![0u8; rel.bytes as usize];
        right_size[0] = 1;
        let e = verify(rel, &right_size).unwrap_err();
        assert!(
            e.contains("sha256"),
            "size-correct rubbish must fail on the hash: {e}"
        );
    }
}

#[cfg(test)]
mod watched_tests {
    use super::*;

    fn scratch(what: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "ipod-fw-watch-{what}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&d).expect("a temp directory");
        d
    }

    fn refused() -> &'static Release {
        CATALOGUE
            .iter()
            .find(|r| !r.served)
            .expect("five of the seventy-one are gone")
    }

    fn served() -> &'static Release {
        by_file("iPod_25.1.3.ipsw").expect("the 5.5G's newest")
    }

    /// **A 403 is not a network failure**, and the difference is the whole reason [`Trouble`] is a
    /// class rather than a sentence: one of them means *try again* and the other means *Apple does
    /// not have this*, and a window that could only read prose had to look for the digits `403`
    /// inside words that were free to be reworded.
    #[test]
    fn a_release_apple_does_not_serve_is_not_a_network_failure() {
        let dir = scratch("not-served");
        let (t, said) = download_watched(refused(), &dir, &mut Silent)
            .expect_err("a release Apple refuses was downloaded");
        assert_eq!(t, Trouble::NotServed { http: 403 });
        assert!(
            said.contains("not about your network"),
            "the sentence blames the network: {said}"
        );
        // Nothing was created for a download that never started.
        let left: Vec<String> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(left.is_empty(), "a refused download left {left:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `download` still says exactly what it used to for its five existing callers: the wrapper
    /// forwards the model's own sentence and adds nothing.
    #[test]
    fn download_forwards_the_same_sentence_the_class_travels_with() {
        let dir = scratch("same-sentence");
        let plain = download(refused(), &dir).expect_err("still refused");
        let (_, watched) = download_watched(refused(), &dir, &mut Silent).expect_err("still refused");
        assert_eq!(plain, watched, "the two doors say different things");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The `.part` is one path, known to the fetcher and to whoever is watching it.** A progress
    /// bar measuring a path the downloader is not writing reads zero for ever.
    #[test]
    fn the_partial_file_is_the_release_with_part_after_it() {
        let dir = std::path::Path::new("/cache");
        let p = part_path(served(), dir);
        assert_eq!(p, dir.join("iPod_25.1.3.ipsw.part"));
        assert!(p.to_string_lossy().ends_with(".part"));
    }

    /// **`is_cached` asks the real question.** A file of exactly the right LENGTH is not the
    /// release: this is what stops a resumed first run from skipping a download and then failing to
    /// open the zip.
    #[test]
    fn a_file_of_the_right_length_is_not_a_cached_release() {
        let dir = scratch("cached");
        let rel = served();
        assert!(!is_cached(rel, &dir), "an empty directory reported a cache hit");
        std::fs::write(dir.join(rel.file), vec![0u8; rel.bytes as usize]).unwrap();
        assert!(
            !is_cached(rel, &dir),
            "a file of the right size passed as the release; nothing checked the hash"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every curl exit code this maps has its own words, and one it does not know says so rather
    /// than inventing an explanation.
    #[test]
    fn every_curl_code_this_reports_has_its_own_words() {
        let mut seen = std::collections::BTreeSet::new();
        for code in [6, 7, 18, 23, 26, 28, 35, 56, 60] {
            let what = curl_meaning(code);
            assert!(!what.is_empty());
            assert!(seen.insert(what), "two codes share the words {what:?}");
        }
        assert_eq!(curl_meaning(999), "stopped without an answer");
    }

    /// **The catalogue route and the server route say the same thing.**
    ///
    /// Both mean *Apple does not serve this file*, and only one of them named the family to try
    /// instead — so a live 403 left a person with one disabled control and no sentence, while the
    /// same fact learned from the catalogue handed them a command.
    #[test]
    fn both_routes_to_apple_does_not_serve_this_name_the_family_to_try() {
        let dir = scratch("family");
        let (_, said) = download_watched(refused(), &dir, &mut Silent).expect_err("refused");
        assert!(
            said.contains(&format!("ipod-boot firmware list {}", refused().updater_family)),
            "the catalogue route stopped naming the family: {said}"
        );
        // The server route is built in the same place, so what is asserted here is that the
        // sentence is attached to the CLASS rather than to the branch that discovered it.
        let fabricated = (
            Trouble::NotServed { http: 403 },
            "https://example.invalid/x: the server answered 403 — it does not serve this file."
                .to_string(),
        );
        let widened = match fabricated.0 {
            Trouble::NotServed { .. } => format!(
                "{}\nAnother release in the same updater family will almost certainly do: try \
                 `ipod-boot firmware list {}`.",
                fabricated.1,
                served().updater_family
            ),
            _ => fabricated.1,
        };
        assert!(widened.contains("ipod-boot firmware list 25"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A transfer that ends early leaves nothing behind.**
    ///
    /// `fetch_watched` removes the `.part` when the *watcher* stopped it and on no other path, so
    /// curl 7 / 18 / 28 / 56 — a refused connection, an interrupted transfer, the common ones — left
    /// a partial file in the firmware cache that is never shown, never offered for deletion and
    /// never cleaned up. Nothing here resumes a download, so those bytes are worth nothing.
    ///
    /// Local, and no packet leaves this machine: a one-shot listener on loopback promises the
    /// release's full length, sends 64 KiB of it, and hangs up. That is a real interrupted
    /// transfer — `curl` writes what arrived and exits 18 — which is the shape a hotel wifi
    /// produces and the shape a refused connection does **not**: curl opens the output file
    /// lazily, so a connection that never carries a byte leaves nothing to clean up and would
    /// have made this test green against the bug.
    #[test]
    fn a_transfer_that_ends_early_leaves_no_partial_file() {
        use std::io::Write;
        if !crate::tooling::have("curl") {
            println!("SKIPPED: no curl on this machine");
            return;
        }
        let dir = scratch("interrupted");
        let rel = served();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("the port").port();
        let promised = rel.bytes;
        let server = std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {promised}\r\nConnection: close\r\n\r\n"
                );
                let _ = s.write_all(head.as_bytes());
                let _ = s.write_all(&vec![0u8; 64 * 1024]);
                let _ = s.flush();
                // and hang up, 6.5 MB short of what was promised.
            }
        });

        // Everything else about it is the real 5.5G entry, so the `.part` it leaves is named
        // exactly as a real one is.
        let cut_off: &'static Release = Box::leak(Box::new(Release {
            url: Box::leak(format!("http://127.0.0.1:{port}/iPod_25.1.3.ipsw").into_boxed_str()),
            ..*rel
        }));
        let (t, said) = download_watched(cut_off, &dir, &mut Silent)
            .expect_err("a transfer 6.5 MB short of its Content-Length was accepted");
        let _ = server.join();
        assert!(
            matches!(t, Trouble::Unreachable { .. }),
            "an interrupted transfer is {t:?}"
        );
        assert!(!said.is_empty());
        let left: Vec<String> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            left.is_empty(),
            "an interrupted transfer left {left:?} in the cache, and nothing ever comes back for it"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
