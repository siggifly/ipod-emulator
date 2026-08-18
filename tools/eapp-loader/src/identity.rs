//! Who this iPod says it is: serial number and FireWire GUID.
//!
//! A synthesised boot ROM has to answer *which iPod is this*, and the answer is a **setting** rather
//! than an accident of which dump somebody found. Three tiers, per [ROADMAP] M5:
//!
//! * **generate** — deterministic from a seed the caller persists
//! * **provide** — the user's own values, parsed and checked
//! * **read** — out of the user's own `iPod_Control/Device/SysInfo`, which is a text file on the
//!   partition their computer already mounts
//!
//! ## Two things that are easy to get wrong
//!
//! **The GUID has teeth and the serial does not.** Apple's DRM binds a purchased title to the
//! FireWire GUID, so a *generated* one can never authorise those titles — on any machine, ever.
//! Nothing validates the serial; RetailOS displays it. So the serial owes the right shape and the
//! GUID owes real structure.
//!
//! **A generated identity must be STABLE.** If it were random per launch the machine would be a
//! different device every time — settings keyed to it, and anything bound to the GUID, would see a
//! new iPod on every boot. So generation is a pure function of a seed, and persisting that seed is
//! the caller's job.
//!
//! [ROADMAP]: ../../../ROADMAP.md

/// Apple's registered OUI, and the top 24 bits of every real iPod's FireWire GUID.
///
/// Observed directly in the handoff block Apple's bootloader leaves, and in the drive's own
/// `SysInfo`. Software that checks a GUID at all checks this.
pub const APPLE_OUI: u64 = 0x00_0A_27;

/// A machine's identity, as the boot ROM reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// Eleven characters, Apple's pre-2010 format.
    pub serial: String,
    /// 64 bits: Apple's OUI in the top 24, the device's own in the low 40.
    pub guid: u64,
    /// Whether these came from real hardware, which decides what may be claimed about them.
    pub source: Source,
}

/// Where an identity came from. **Kept with the values** because the difference is not cosmetic:
/// only a real GUID can authorise a purchased title, and a UI that cannot tell the two apart cannot
/// warn anybody.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Made up, deterministically, from a seed. Boots; cannot authorise DRM-bound titles.
    Generated,
    /// Typed in by the user.
    Provided,
    /// Read out of a real device's `SysInfo`.
    RealDevice,
}

impl Identity {
    /// Deterministic from `seed` — the same seed always yields the same iPod.
    ///
    /// The serial follows Apple's shape: location(2) · year(1) · week(2) · unique(3) · model(3).
    /// **The model code is not derived from the model**, because the table that maps one to the
    /// other is Apple-internal and we do not have it; guessing it would be inventing a fact. It is
    /// generated like the rest, and it is cosmetic until somebody demonstrates otherwise.
    pub fn generate(seed: u64) -> Identity {
        // SplitMix64 — a few lines, no dependency, and good enough for picking characters. The
        // requirement here is "same seed, same iPod", not statistical quality.
        fn mix(s: &mut u64) -> u64 {
            *s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = *s;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        // Apple's serials use digits and upper-case letters.
        const A: &[u8] = b"0123456789ABCDEFGHIJKLMNPQRSTUVWXYZ";
        let mut st = seed;
        let pick = |st: &mut u64, n: usize| -> String {
            (0..n).map(|_| A[(mix(st) % A.len() as u64) as usize] as char).collect()
        };
        let loc = pick(&mut st, 2);
        // The year is a DIGIT — the last digit of the manufacturing year — and both real examples
        // have one. Picking it from the full alphanumeric set produced serials like `PCL29…`, which
        // is the same class of tell as an impossible week number.
        let year = format!("{}", mix(&mut st) % 10);
        // A real week is 01..=52, and a serial claiming week 99 is the sort of detail that makes
        // somebody doubt everything else on the screen.
        let week = format!("{:02}", mix(&mut st) % 52 + 1);
        let uniq = pick(&mut st, 3);
        let model = pick(&mut st, 3);
        let serial = format!("{loc}{year}{week}{uniq}{model}");

        // Apple's OUI in the top 24 bits, 40 bits of uniqueness below — the structure every real
        // GUID has.
        let guid = (APPLE_OUI << 40) | (mix(&mut st) & 0x00_FF_FF_FF_FF_FF);
        Identity { serial, guid, source: Source::Generated }
    }

    /// True when this identity can authorise DRM-bound titles — i.e. when it is a real device's.
    ///
    /// A generated GUID is not merely unlikely to work: Apple bound those titles to a *specific*
    /// device, so nothing invented can ever match.
    pub fn can_authorise_titles(&self) -> bool {
        self.source == Source::RealDevice
    }

    /// Parse a real device's `iPod_Control/Device/SysInfo`.
    ///
    /// The file is ~349 bytes of `Key: value` lines on the data partition — the one a computer
    /// already mounts when an iPod is plugged in. **This is the tier that needs no NOR dump and no
    /// disk-mode driver**, which is the whole reason it exists.
    pub fn from_sysinfo(text: &str) -> Result<Identity, String> {
        let field = |k: &str| -> Option<String> {
            text.lines()
                .find_map(|l| l.split_once(':').filter(|(n, _)| n.trim() == k))
                .map(|(_, v)| v.trim().to_string())
        };
        let serial = field("pszSerialNumber").ok_or("no pszSerialNumber line")?;
        let raw = field("FirewireGuid").ok_or("no FirewireGuid line")?;
        let guid = u64::from_str_radix(raw.trim_start_matches("0x"), 16)
            .map_err(|_| format!("FirewireGuid is not hex: {raw}"))?;
        if guid >> 40 != APPLE_OUI {
            return Err(format!(
                "FirewireGuid {guid:#018x} does not begin with Apple's OUI {APPLE_OUI:#08x} — \
                 this is not an iPod's GUID"
            ));
        }
        Ok(Identity { serial, guid, source: Source::RealDevice })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the whole design rests on: same seed, same iPod. Without it, anything bound to
    /// the GUID sees a new device on every launch.
    #[test]
    fn generation_is_stable_for_a_seed_and_different_across_seeds() {
        assert_eq!(Identity::generate(42), Identity::generate(42));
        assert_ne!(Identity::generate(42).guid, Identity::generate(43).guid);
    }

    /// Structure, not decoration: software that inspects a GUID checks the OUI.
    #[test]
    fn a_generated_guid_carries_apples_oui_and_a_shaped_serial() {
        for seed in [0u64, 1, 7, 1234, u64::MAX] {
            let id = Identity::generate(seed);
            assert_eq!(id.guid >> 40, APPLE_OUI, "seed {seed}");
            assert_eq!(id.serial.len(), 11, "seed {seed}: {}", id.serial);
            assert!(id.serial.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
            // The date fields are dates: a digit year and a real week. Both are the sort of tell
            // that makes a reader doubt everything else on the screen.
            assert!(id.serial[2..3].chars().all(|c| c.is_ascii_digit()), "seed {seed}: {}", id.serial);
            let week: u32 = id.serial[3..5].parse().expect("week digits");
            assert!((1..=52).contains(&week), "seed {seed}: week {week}");
        }
    }

    /// A generated identity must never claim it can authorise titles — the DRM binds to a specific
    /// real device, so nothing invented can match, and a UI that got this wrong would promise
    /// something impossible.
    #[test]
    fn only_a_real_device_can_authorise_titles() {
        assert!(!Identity::generate(1).can_authorise_titles());
        let real = Identity::from_sysinfo(
            "BoardHwName: PP5021C-2\npszSerialNumber: AB1234XYZQR\nFirewireGuid: 0x000A270011223344\n",
        )
        .unwrap();
        assert!(real.can_authorise_titles());
        assert_eq!(real.serial, "AB1234XYZQR");
        assert_eq!(real.guid, 0x000A2700_11223344);
    }

    /// The negative control, and it is the useful half: a file that is not an iPod's must be
    /// refused with a reason, not accepted with a wrong GUID.
    #[test]
    fn a_guid_without_apples_oui_is_refused() {
        let e = Identity::from_sysinfo(
            "pszSerialNumber: AB1234XYZQR\nFirewireGuid: 0xDEADBEEFCAFEBABE\n",
        )
        .unwrap_err();
        assert!(e.contains("Apple's OUI"), "{e}");
        assert!(Identity::from_sysinfo("nothing useful here").is_err());
    }
}
