//! Who this iPod says it is: model, serial number and FireWire GUID.
//!
//! A synthesised boot ROM has to answer *which iPod is this*, and the answer is a **setting** rather
//! than an accident of which dump somebody found. Four tiers, per [ROADMAP] M5:
//!
//! * **read the NOR** — the identity of the dump this machine actually boots ([`Identity::from_nor`])
//! * **read the drive** — out of `iPod_Control/Device/SysInfo` on a volume the user's own computer
//!   has already mounted, which needs no dump and no disk-mode driver
//! * **provide** — the user's own values, typed or edited
//! * **generate** — deterministic from a seed, for anyone who has neither
//!
//! ## Prior art this is reconciled against
//!
//! `siggifly/ipod-usb-new` solved the same problem first and solved it against a **real iTunes**,
//! which is the only authority that can say an identity was got right. Its rules are adopted here
//! rather than re-derived:
//!
//! * **Nothing is hardwired.** No default GUID, no default serial. A plausible-looking constant is
//!   somebody's real device the moment it is used unthinkingly.
//! * **The serial is optional; the GUID is not.** The DRM binds to the GUID. A serial is a string
//!   RetailOS prints on the About screen.
//! * **A non-Apple OUI warns, it does not refuse.** It is a strong hint of a bad parse, not a fact
//!   about what a user is permitted to present.
//! * **Show what was read before using it.** An identity presented without being seen is one nobody
//!   can catch being wrong.
//!
//! ## The one thing that is easy to get wrong
//!
//! **A generated identity must be STABLE.** If it were random per launch the machine would be a
//! different device every time — settings keyed to it, and anything bound to the GUID, would see a
//! new iPod on every boot. So generation is a pure function of a seed, and persisting that seed is
//! the caller's job.
//!
//! [ROADMAP]: ../../../ROADMAP.md

use std::path::Path;

/// Apple's registered OUI, and the top 24 bits of every real iPod's FireWire GUID.
///
/// Observed directly in the NOR's `SysCfg` block, in the handoff block Apple's bootloader leaves,
/// and in the drive's own `SysInfo` — three independent sources. Software that checks a GUID at all
/// checks this.
pub const APPLE_OUI: u64 = 0x00_0A_27;

/// A machine's identity, as the boot ROM reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// Eleven characters, Apple's pre-2010 format. **Optional**: the DRM binds to the GUID, and a
    /// dump whose `SrNm` record is absent is still a usable identity.
    pub serial: Option<String>,
    /// 64 bits: Apple's OUI in the top 24, the device's own in the low 40. **This is the field with
    /// teeth** — it is also the USB serial number a host sees.
    pub guid: u64,
    /// Where these came from, which decides what may be claimed about them.
    pub source: Source,
}

/// Where an identity came from. **Kept with the values** because the difference is not cosmetic —
/// see [`Identity::title_auth`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Made up, deterministically, from a seed.
    Generated,
    /// Typed in or edited by the user. We cannot tell by looking whether these are a real device's.
    Provided,
    /// Read out of a real device — its NOR's `SysCfg`, or its drive's `SysInfo`.
    RealDevice,
}

/// Whether iTunes could authorise DRM-bound titles against this identity.
///
/// **This is not a guess.** `siggifly/ipod-usb` presented a virtual iPod to a real iTunes, iTunes
/// accepted it, and the titles it authorised are the ones `research/13` then loaded. So the
/// mechanism is known: iTunes mints keys against the identity a device presents, and the emulator
/// presents whatever is in the NOR it boots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleAuth {
    /// **Never.** Invented values match no purchase that has ever been made, on any machine.
    Never,
    /// **Only if they are really yours.** Real values authorise the titles bought for *that*
    /// device — which is the user's own iPod, or is somebody else's.
    IfGenuine,
    /// **Yes**, for the titles bought for this device.
    Yes,
}

impl Identity {
    /// Read the identity out of the NOR dump this machine boots.
    ///
    /// **The most relevant tier, because it is the only one that is self-consistent.** Keys
    /// authorised against any other identity are keys this machine cannot present.
    pub fn from_nor(path: &Path) -> Result<Identity, String> {
        let nor = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let c = crate::inspect::syscfg(&nor).ok_or_else(|| {
            format!(
                "{}: no SysCfg block at 0x4000.\n\
                 A 5G/5.5G NOR dump is 1 MiB and starts with the boot ROM; a file that is neither \
                 will land here.",
                path.display()
            )
        })?;
        let guid = c.guid.ok_or_else(|| {
            format!("{}: SysCfg has no FwId record, so there is no GUID", path.display())
        })?;
        if !c.guid_looks_apple() {
            // Warn, do not refuse: this is evidence of a bad parse, not a permission decision.
            eprintln!(
                "warning: {guid:016X} does not carry Apple's FireWire OUI ({APPLE_OUI:06X}).\n\
                 \x20        Either this is not an iPod NOR, or it did not parse."
            );
        }
        Ok(Identity { serial: c.serial, guid, source: Source::RealDevice })
    }

    /// Read a real device's `iPod_Control/Device/SysInfo`.
    ///
    /// `root` is the volume — `/Volumes/IPOD` on macOS, wherever the desktop mounted it elsewhere.
    /// **This tier needs no NOR dump, no disk-mode driver and no elevated privileges**: it is an
    /// ordinary read of a file the user's own computer has already mounted, which is the whole
    /// reason it exists.
    pub fn from_volume(root: &Path) -> Result<Identity, String> {
        let p = root.join("iPod_Control/Device/SysInfo");
        let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        Identity::from_sysinfo(&text).map_err(|e| format!("{}: {e}", p.display()))
    }

    /// Every mounted volume that looks like an iPod, with what each reports.
    ///
    /// The test is a `SysInfo` that parses — **not** the volume's name, which the owner may have
    /// changed to anything. A volume that has the file but fails to parse is returned with its
    /// reason, because "I can see your iPod but its SysInfo is not what I expected" is worth saying
    /// out loud rather than presenting as no-iPod-found.
    pub fn detect_mounted() -> Vec<(std::path::PathBuf, Result<Identity, String>)> {
        // Where a desktop mounts removable media. A missing directory is skipped, so listing all
        // three costs nothing on a machine that has one of them.
        const ROOTS: &[&str] = &["/Volumes", "/media", "/run/media"];
        let mut out = Vec::new();
        for root in ROOTS {
            let Ok(entries) = std::fs::read_dir(root) else { continue };
            for e in entries.flatten() {
                let vol = e.path();
                // One level on macOS (`/Volumes/<volume>`), two on Linux (`/media/<user>/<volume>`).
                // Checking the entry and then its children covers both without knowing which we are.
                let nested = std::fs::read_dir(&vol).into_iter().flatten().flatten().map(|c| c.path());
                for c in std::iter::once(vol.clone()).chain(nested) {
                    if c.join("iPod_Control/Device/SysInfo").is_file() {
                        let id = Identity::from_volume(&c);
                        out.push((c, id));
                    }
                }
            }
        }
        out
    }

    /// Parse the text of an `iPod_Control/Device/SysInfo`.
    ///
    /// ~349 bytes of `Key: value` lines on the data partition.
    pub fn from_sysinfo(text: &str) -> Result<Identity, String> {
        let serial = sysinfo_field(text, "pszSerialNumber");
        let raw = sysinfo_field(text, "FirewireGuid").ok_or("no FirewireGuid line")?;
        let guid = u64::from_str_radix(raw.trim_start_matches("0x"), 16)
            .map_err(|_| format!("FirewireGuid is not hex: {raw}"))?;
        if guid >> 40 != APPLE_OUI {
            eprintln!(
                "warning: {guid:016X} does not carry Apple's FireWire OUI ({APPLE_OUI:06X})."
            );
        }
        Ok(Identity { serial, guid, source: Source::RealDevice })
    }

    /// The user's own values, typed in or edited from a generated pair.
    ///
    /// Checked rather than trusted, because both failure modes here are silent: a serial of the
    /// wrong length renders as garbage on RetailOS's About screen, and a malformed GUID surfaces as
    /// iTunes quietly declining to identify the device — the least debuggable failure in this
    /// family of projects.
    ///
    /// **The source is `Provided`, not `RealDevice`, even when the user typed a real device's
    /// values**, because we cannot tell the difference by looking.
    pub fn provided(serial: Option<&str>, guid: u64) -> Result<Identity, String> {
        let serial = match serial.map(str::trim).filter(|s| !s.is_empty()) {
            None => None,
            Some(s) => {
                let s = s.to_ascii_uppercase();
                if s.chars().count() != 11 {
                    return Err(format!(
                        "a serial is 11 characters; got {}: {s}",
                        s.chars().count()
                    ));
                }
                if !s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
                    return Err(format!("a serial is letters and digits only: {s}"));
                }
                Some(s)
            }
        };
        if guid >> 40 != APPLE_OUI {
            eprintln!(
                "warning: {guid:016X} does not start with Apple's FireWire OUI ({APPLE_OUI:06X})."
            );
        }
        Ok(Identity { serial, guid, source: Source::Provided })
    }

    /// Deterministic from `seed` — the same seed always yields the same iPod.
    ///
    /// **The serial has to look like a serial.** That is most of the reason this module exists: a
    /// synthesised iPod whose About screen reads something no Apple factory ever stamped is a
    /// synthesised iPod that announces itself. So every field is drawn from what real hardware
    /// carries, for the generation being generated:
    ///
    /// | field | where the values come from |
    /// |---|---|
    /// | location (2) | factory codes observed on real iPods here |
    /// | year (1) | [`Generation::year_digits`] — a 5G reads `5` or `6`, not `3` |
    /// | week (2) | `01`..=`52` |
    /// | unique (3) | free |
    /// | model (3) | [`Generation::serial_codes`] — Apple's published endings, plus observed ones |
    ///
    /// **The image says it is synthetic, not the serial.** An earlier version ended every generated
    /// serial `ZZ?` so it could never be mistaken for a real code — which defeated the point of
    /// generating one. [`crate::nor::SYNTH_MARK`] carries that job, in the ROM image, where it costs
    /// nothing.
    ///
    /// Nothing validates a serial, and a collision with a real device's is harmless: the GUID is
    /// the field with teeth, and its low 40 bits are drawn from the same seed.
    pub fn generate(model: &'static Model, seed: u64) -> Identity {
        // SplitMix64 — a few lines, no dependency, and good enough for picking characters. The
        // requirement here is "same seed, same iPod", not statistical quality.
        fn mix(s: &mut u64) -> u64 {
            *s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = *s;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        // Apple's serials use digits and upper-case letters. `O` is absent because it is not used
        // in them — it would be read as a zero.
        const A: &[u8] = b"0123456789ABCDEFGHIJKLMNPQRSTUVWXYZ";
        // Manufacturing-location prefixes seen on real iPods here. A short, real set beats a long
        // invented one: two random letters produce prefixes no factory ever had.
        const LOCATIONS: &[&str] = &["4J", "JQ", "9C"];

        let mut st = seed;
        let pick = |st: &mut u64, n: usize| -> String {
            (0..n).map(|_| A[(mix(st) % A.len() as u64) as usize] as char).collect()
        };

        let loc = LOCATIONS[(mix(&mut st) % LOCATIONS.len() as u64) as usize];
        let years = model.generation.year_digits();
        let year = if years.is_empty() {
            // No production years established for this generation, so the digit is left free
            // rather than guessed at.
            format!("{}", mix(&mut st) % 10)
        } else {
            format!("{}", years[(mix(&mut st) % years.len() as u64) as usize])
        };
        // A real week is 01..=52, and a serial claiming week 99 is the sort of detail that makes
        // somebody doubt everything else on the screen.
        let week = format!("{:02}", mix(&mut st) % 52 + 1);
        let uniq = pick(&mut st, 3);
        // The U2 edition has its own published ending, so a U2 gets it rather than a code from the
        // general pool.
        let codes = model.generation.serial_codes();
        let code = if model.colour() == Colour::U2 {
            "W9G".to_string()
        } else if codes.is_empty() {
            pick(&mut st, 3)
        } else {
            codes[(mix(&mut st) % codes.len() as u64) as usize].to_string()
        };
        let serial = format!("{loc}{year}{week}{uniq}{code}");

        // Apple's OUI in the top 24 bits, 40 bits of uniqueness below — the structure every real
        // GUID has.
        let guid = (APPLE_OUI << 40) | (mix(&mut st) & 0x00_FF_FF_FF_FF_FF);
        Identity { serial: Some(serial), guid, source: Source::Generated }
    }

    /// `000A270014EFE726` — the form iTunes, `SysInfo` and a USB descriptor all use.
    pub fn guid_hex(&self) -> String {
        format!("{:016X}", self.guid)
    }

    /// Whether iTunes could authorise DRM-bound titles against this identity. See [`TitleAuth`].
    pub fn title_auth(&self) -> TitleAuth {
        match self.source {
            Source::Generated => TitleAuth::Never,
            Source::Provided => TitleAuth::IfGenuine,
            Source::RealDevice => TitleAuth::Yes,
        }
    }
}

/// One `Key: value` line out of a `SysInfo`.
fn sysinfo_field(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.split_once(':').filter(|(n, _)| n.trim() == key))
        .map(|(_, v)| v.trim().to_string())
}

/// A case colour — **a fact stated by the model number**, not inferred.
///
/// The full set across every iPod, because the table covers every iPod. `Unspecified` is its own
/// answer and matters: many early models came in exactly one colour and say nothing about it, and
/// recording that as white would be inventing a fact about hardware we have never seen.
///
/// The default is black, and applies only before a NOR has been chosen: once there is a dump the
/// colour comes from its `Mod#`. Black because the reference hardware here is an `MA146`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Colour {
    White,
    #[default]
    Black,
    /// Black case, **red** wheel. Apple's own asset for it is named `iPod6-BlackRed`.
    U2,
    Silver,
    Blue,
    Gold,
    Green,
    Pink,
    Orange,
    Purple,
    Red,
    Yellow,
    Stainless,
    /// The model constant names no colour.
    Unspecified,
}

impl Colour {
    /// The settings-file spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Colour::White => "white",
            Colour::Black => "black",
            Colour::U2 => "u2",
            Colour::Silver => "silver",
            Colour::Blue => "blue",
            Colour::Gold => "gold",
            Colour::Green => "green",
            Colour::Pink => "pink",
            Colour::Orange => "orange",
            Colour::Purple => "purple",
            Colour::Red => "red",
            Colour::Yellow => "yellow",
            Colour::Stainless => "stainless",
            Colour::Unspecified => "unspecified",
        }
    }

    /// Parse the settings-file spelling. `None` for anything else, so an unreadable value falls
    /// back to the default rather than picking a colour at random.
    pub fn parse(s: &str) -> Option<Colour> {
        let s = s.trim().to_ascii_lowercase();
        [
            Colour::White, Colour::Black, Colour::U2, Colour::Silver, Colour::Blue, Colour::Gold,
            Colour::Green, Colour::Pink, Colour::Orange, Colour::Purple, Colour::Red,
            Colour::Yellow, Colour::Stainless, Colour::Unspecified,
        ]
        .into_iter()
        .find(|c| c.as_str() == s)
    }

    /// What to call it on screen.
    pub fn label(self) -> &'static str {
        match self {
            Colour::U2 => "U2 Special Edition",
            Colour::Unspecified => "Standard",
            Colour::White => "White",
            Colour::Black => "Black",
            Colour::Silver => "Silver",
            Colour::Blue => "Blue",
            Colour::Gold => "Gold",
            Colour::Green => "Green",
            Colour::Pink => "Pink",
            Colour::Orange => "Orange",
            Colour::Purple => "Purple",
            Colour::Red => "Red",
            Colour::Yellow => "Yellow",
            Colour::Stainless => "Stainless steel",
        }
    }
}

pub use crate::models::{Generation, IpodModel, Row as Model, MODELS};

impl Generation {
    /// The Gestalt ID RetailOS switches on, at `sysinfo + 0x84`.
    ///
    /// `research/02` establishes that its jump table at `0x2653a4` accepts both Video generations,
    /// because it switches on the high halfword `0x000B` = 11. **`None` for every other
    /// generation**, because we have not sourced their constants and a plausible guess here would
    /// be indistinguishable from a fact.
    /// The year digits a real serial from this generation plausibly carries.
    ///
    /// Apple's pre-2010 serial holds a **single digit** for the year, so a 5G — on sale from
    /// October 2005 into 2006 — reads `5` or `6`. Getting this right is most of what makes a
    /// generated serial look like a serial: a 30 GB Video claiming to be built in 2003 is the same
    /// class of tell as week 99.
    ///
    /// Empty where the generation's production years have not been established here, in which case
    /// the digit is left free rather than guessed.
    pub fn year_digits(self) -> &'static [u8] {
        match self {
            // On sale October 2005; the Late 2006 revision replaced it in September 2006.
            Generation::Video1 => &[5, 6],
            // Late 2006, sold into 2007.
            Generation::Video2 => &[6, 7],
            _ => &[],
        }
    }

    /// The last three characters of serials **observed on real devices** of this generation.
    ///
    /// For the Video these are Apple's published 5th-generation endings plus two this project has
    /// seen on hardware and which appear on no published list. **Which capacity each denotes is not
    /// known and is not claimed** — the point is only that these are codes real iPods carry, so a
    /// generated serial ends in one rather than in something no iPod ever did.
    ///
    /// The published list does not separate the 5G from the 5.5G, and a real `MA446` — a 5.5G —
    /// was observed here ending `V9M`, which is on it. So both Video generations share the set.
    pub fn serial_codes(self) -> &'static [&'static str] {
        const VIDEO: &[&str] = &[
            // Apple's published 5th-generation endings.
            "V9K", "V9P", "V9M", "V9R", "V9L", "V9N", "V9Q", "V9S", "WU9", "WUA", "WUB", "WUC",
            "X3N",
            // Observed here on real hardware, on no published list.
            "TXK", "TXM",
        ];
        match self {
            Generation::Video1 | Generation::Video2 => VIDEO,
            _ => &[],
        }
    }

    /// A short human name. The two Video generations get the names people actually use; the rest
    /// fall back to libgpod's own constant name rather than to an invented marketing string.
    pub fn label(self) -> String {
        match self {
            Generation::Video1 => "5G".into(),
            Generation::Video2 => "5.5G".into(),
            other => format!("{other:?}"),
        }
    }

    pub fn gestalt(self) -> Option<u32> {
        match self {
            Generation::Video1 => Some(0x000B_0005),
            Generation::Video2 => Some(0x000B_0010),
            _ => None,
        }
    }
}

impl Model {
    /// Look up a `ModelNumStr`, in any of the forms it is written in.
    ///
    /// **The normalisation is the whole difficulty.** Our own drives say `xMA146`; the NOR's `Mod#`
    /// says `MA146`; the table key is `A146`. libgpod gets there in two strips — its `SysInfo`
    /// reader drops the leading `x`, then the table lookup drops one further alphabetic character.
    /// Reproducing that as two conditional strips is fragile, so this takes the **last four
    /// characters** and requires the final three to be digits, which accepts every observed form
    /// and rejects strings that are not model numbers.
    pub fn lookup(model_num_str: &str) -> Option<&'static Model> {
        let s = model_num_str.trim().to_ascii_uppercase();
        let key: String = s.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
        if key.len() != 4 || !key[1..].chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        MODELS.iter().find(|m| m.number == key)
    }

    /// Look one up from the text of a `SysInfo`.
    pub fn from_sysinfo(text: &str) -> Option<&'static Model> {
        Model::lookup(&sysinfo_field(text, "ModelNumStr")?)
    }

    /// The case colour this row implies.
    pub fn colour(&self) -> Colour {
        self.model.colour()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the whole design rests on: same seed, same iPod. Without it, anything bound to
    /// the GUID sees a new device on every launch.
    #[test]
    fn generation_is_stable_for_a_seed_and_different_across_seeds() {
        let m = Model::lookup("MA146").expect("MA146");
        assert_eq!(Identity::generate(m, 42), Identity::generate(m, 42));
        assert_ne!(Identity::generate(m, 42).guid, Identity::generate(m, 43).guid);
    }

    /// **A generated serial has to look like one a factory stamped.** Every field is checked
    /// against what real hardware carries, because the whole reason for generating an identity is
    /// that the result passes for one — an About screen reading something impossible is a
    /// synthesised iPod announcing itself.
    #[test]
    fn a_generated_serial_looks_like_a_real_one() {
        const LOCATIONS: [&str; 3] = ["4J", "JQ", "9C"];
        let video = Model::lookup("MA146").expect("MA146");
        for seed in [0u64, 1, 7, 1234, u64::MAX] {
            let id = Identity::generate(video, seed);
            assert_eq!(id.guid >> 40, APPLE_OUI, "seed {seed}");
            let s = id.serial.clone().expect("generate always makes one");
            assert_eq!(s.len(), 11, "seed {seed}: {s}");
            assert!(s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()), "{s}");

            assert!(LOCATIONS.contains(&&s[0..2]), "seed {seed}: {s} — not a real factory prefix");
            let year: u8 = s[2..3].parse().expect("the year is a digit");
            assert!(
                video.generation.year_digits().contains(&year),
                "seed {seed}: {s} — a 5G was not built in 200{year}"
            );
            let week: u32 = s[3..5].parse().expect("week digits");
            assert!((1..=52).contains(&week), "seed {seed}: week {week}");
            assert!(
                video.generation.serial_codes().contains(&&s[8..11]),
                "seed {seed}: {s} — ends in a code no iPod carries"
            );
        }
    }

    /// The U2 has its own published ending, and gets it.
    #[test]
    fn a_u2_serial_carries_the_u2_code() {
        let u2 = Model::lookup("MA452").expect("the 30 GB U2");
        assert_eq!(u2.colour(), Colour::U2);
        for seed in [0u64, 5, 99] {
            let s = Identity::generate(u2, seed).serial.expect("a serial");
            assert!(s.ends_with("W9G"), "{s} should end W9G");
        }
    }

    /// A generation whose production years and codes are not established gets a free-form serial
    /// rather than a Video's — borrowing the Video's codes for a Nano would be inventing a fact.
    #[test]
    fn a_generation_with_no_recorded_codes_still_produces_a_shaped_serial() {
        let nano = Model::lookup("A004").expect("a nano");
        assert!(nano.generation.serial_codes().is_empty(), "precondition");
        let s = Identity::generate(nano, 3).serial.expect("a serial");
        assert_eq!(s.len(), 11);
        assert!(s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()), "{s}");
        let week: u32 = s[3..5].parse().expect("week digits");
        assert!((1..=52).contains(&week));
    }

    /// The distinction the DRM actually turns on, and it is three-valued rather than two. Getting
    /// this wrong in a UI means either promising something impossible or refusing something that
    /// `ipod-usb` demonstrated works.
    #[test]
    fn only_real_values_can_authorise_titles() {
        let m = Model::lookup("MA146").expect("MA146");
        assert_eq!(Identity::generate(m, 1).title_auth(), TitleAuth::Never);
        assert_eq!(
            Identity::provided(Some("AB1234XYZQR"), 0x000A_2700_1122_3344).unwrap().title_auth(),
            TitleAuth::IfGenuine
        );
        let real = Identity::from_sysinfo(
            "BoardHwName: PP5021C-2\npszSerialNumber: AB1234XYZQR\nFirewireGuid: 0x000A270011223344\n",
        )
        .unwrap();
        assert_eq!(real.title_auth(), TitleAuth::Yes);
        assert_eq!(real.serial.as_deref(), Some("AB1234XYZQR"));
        assert_eq!(real.guid, 0x000A_2700_1122_3344);
        assert_eq!(real.guid_hex(), "000A270011223344");
    }

    /// The serial is optional because the DRM binds to the GUID — a dump with no `SrNm` is still a
    /// usable identity, and an empty string is not a serial.
    #[test]
    fn a_blank_serial_is_none_not_an_empty_string() {
        assert_eq!(Identity::provided(Some("   "), 0x000A_2700_0000_0001).unwrap().serial, None);
        assert_eq!(Identity::provided(None, 0x000A_2700_0000_0001).unwrap().serial, None);
    }

    /// Malformed serials are refused; a non-Apple OUI is *not*, per the rule adopted from
    /// `ipod-usb-new` — it is evidence of a bad parse, not a permission decision.
    #[test]
    fn malformed_serials_are_refused_but_a_foreign_oui_is_allowed() {
        for bad in ["TOOSHORT", "WAYTOOLONGSERIAL", "AB1234-YZQR"] {
            assert!(Identity::provided(Some(bad), 0x000A_2700_0000_0001).is_err(), "must reject {bad:?}");
        }
        assert!(Identity::provided(None, 0xDEAD_BEEF_DEAD_BEEF).is_ok());
        assert!(Identity::from_sysinfo("nothing useful here").is_err());
    }

    /// **The normalisation, against the form our own hardware actually writes.** `xMA146` is what is
    /// on every drive image here; if only `A146` resolved, the lookup would silently never fire on
    /// real data — which is the failure this test exists to prevent.
    #[test]
    fn a_model_number_resolves_in_every_form_it_is_written_in() {
        for form in ["xMA146", "MA146", "A146", "xma146"] {
            let m = Model::lookup(form).unwrap_or_else(|| panic!("{form} must resolve"));
            assert_eq!(m.colour(), Colour::Black, "{form}");
            assert_eq!(m.capacity_gb, 30, "{form}");
            assert_eq!(m.generation, Generation::Video1, "{form}");
        }
        // The 80 GB models exist only as 5.5G, which is the table's own consistency check.
        assert_eq!(Model::lookup("MA448").unwrap().generation, Generation::Video2);
        assert_eq!(Model::lookup("MA448").unwrap().colour(), Colour::White);
        assert_eq!(Model::lookup("MA450").unwrap().colour(), Colour::Black);
        // Negative controls: things that are not model numbers must not resolve to one.
        for bad in ["", "A14", "ABCD", "nonsense", "A1466"] {
            assert!(Model::lookup(bad).is_none(), "must not resolve {bad:?}");
        }
    }

    /// Reading it the way it actually arrives — a whole `SysInfo`, exactly as our drives write it.
    #[test]
    fn the_model_is_read_out_of_a_real_shaped_sysinfo() {
        let text = "BoardHwName: PP5021C-2\nModelNumStr: xMA146\nboardHwRev: 0x00050000\n";
        let m = Model::from_sysinfo(text).expect("must resolve");
        assert_eq!((m.colour(), m.capacity_gb), (Colour::Black, 30));
        // The model says 5G and `boardHwRev` says 5. Two independent fields, one answer — which is
        // the check worth having, because either alone could be a misparse.
        assert_eq!(m.generation, Generation::Video1);
        assert_eq!(m.generation.gestalt(), Some(0x000B_0005));
        // Every other generation is honest about not knowing.
        assert_eq!(Generation::Nano1.gestalt(), None);
    }
}
