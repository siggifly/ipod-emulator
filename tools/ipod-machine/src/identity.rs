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

impl TitleAuth {
    /// The sentence, **on the enum that computes it**.
    ///
    /// It is the one decision on the identity page with an irreversible consequence, and it is said
    /// where the decision is made rather than in a footnote: choose to synthesise an iPod and you
    /// have chosen a machine that can never authorise a purchased title, because invented values
    /// match no purchase ever made on any machine.
    ///
    /// Exhaustive, with no `_` arm, so a fourth source cannot inherit a third's sentence.
    pub fn line(self) -> &'static str {
        match self {
            TitleAuth::Never => {
                "It can never authorise a purchased title — invented values match no purchase ever \
                 made, on any machine."
            }
            TitleAuth::IfGenuine => {
                "Only if these are really this device's; we cannot tell by looking."
            }
            TitleAuth::Yes => "Yes, for the titles bought for this device.",
        }
    }

    /// Every variant, in declaration order — the length is written into the type.
    pub const ALL: [TitleAuth; 3] = [TitleAuth::Never, TitleAuth::IfGenuine, TitleAuth::Yes];
}

/// One refusal, worded twice: **once verbatim, and once by POSITION for use while the field that
/// produced it is masked.**
///
/// The masking on the Composer's identity rows is defeated the moment a validation sentence quotes
/// the offending character back — `identity.rs` renders *"the 5.5G was made in 2006 or 2007, so its
/// serial's third character is one of those — not `3`"*, and the third character is one of the six
/// the mask hides. So every arm that can quote the input carries a twin that names the *position*
/// instead, and the surface picks between them on one question: is the field revealed?
///
/// Where an arm quotes nothing the user cannot already see — a count, a fixed string, or the last
/// three characters, which the mask shows — `masked` is **identical to `why`**, deliberately, so
/// the boundary is derived from what the mask hides rather than chosen per sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The model's own sentence, verbatim. Drawn when the field is revealed.
    pub why: String,
    /// The same rule with every hidden position named rather than quoted. Drawn while it is masked.
    pub masked: String,
}

impl Refusal {
    /// One sentence that is safe against a masked field, so both wordings are it.
    ///
    /// **A constructor rather than a default**, because `masked` defaulting to `why` is precisely
    /// the shape that lets a new arm leak by omission: this way every arm states which of the two
    /// kinds it is.
    pub fn same(why: String) -> Refusal {
        Refusal {
            masked: why.clone(),
            why,
        }
    }

    /// The wording for a field in this state. **The one reader**, so no surface picks by hand.
    pub fn text(&self, revealed: bool) -> &str {
        if revealed {
            &self.why
        } else {
            &self.masked
        }
    }
}

/// What a hidden character is replaced by.
///
/// **ASCII `*`, not U+2022.** The window's glyph rule is a closed set — `{'—', '…', '§'}` — whose
/// own doc forbids widening it to make a test pass, and nothing in Slint can ask whether a glyph
/// exists. A bullet would ship `.notdef` squares over somebody's serial.
pub const MASK_CHAR: char = '*';

/// How much of a serial stays visible at the front: the two-character manufacturing location.
///
/// **Two and not three.** Head 3 would leave the year digit — byte index 2 — on screen, and the
/// year digit is exactly what [`Identity::check_serial_for`]'s own refusal quotes back. The mask
/// has to hide every character a refusal can name, or the refusal is the leak.
pub const MASK_SERIAL_HEAD: usize = 2;
/// How much stays visible at the end: the three-character model code, which the Model row two lines
/// above states outright. Hiding it would be theatre.
pub const MASK_SERIAL_TAIL: usize = 3;
/// The six hex digits of Apple's OUI — a published constant this program declares as
/// [`APPLE_OUI`] and quotes in its own refusal.
pub const MASK_GUID_HEAD: usize = 6;
/// Nothing. The low ten digits are the device's own and are the whole of what is worth hiding.
pub const MASK_GUID_TAIL: usize = 0;

/// `7B******X3N` — the same width as what it hides, so pressing `Show` moves nothing.
///
/// **The mask count is computed from the value's own length and never typed.** A serial that is not
/// eleven characters is not a serial, so it is masked whole rather than sliced at positions that
/// mean nothing in it.
pub fn mask_serial(s: &str) -> String {
    if s.chars().count() != 11 {
        return mask_whole(s);
    }
    mask_between(s, MASK_SERIAL_HEAD, MASK_SERIAL_TAIL)
}

/// `000A27**********` — sixteen characters, ten of them hidden.
pub fn mask_guid_hex(hex: &str) -> String {
    if hex.chars().count() != 16 {
        return mask_whole(hex);
    }
    mask_between(hex, MASK_GUID_HEAD, MASK_GUID_TAIL)
}

fn mask_whole(s: &str) -> String {
    s.chars().map(|_| MASK_CHAR).collect()
}

fn mask_between(s: &str, head: usize, tail: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= head + tail {
        return mask_whole(s);
    }
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if i < head || i >= chars.len() - tail {
                *c
            } else {
                MASK_CHAR
            }
        })
        .collect()
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
            format!(
                "{}: SysCfg has no FwId record, so there is no GUID",
                path.display()
            )
        })?;
        // **The warning is not printed here.** It used to go to stderr, where a window cannot see
        // it — so the one surface that draws this identity said nothing about the strongest
        // evidence there is that the parse is wrong. [`Identity::oui_warning`] is the sentence, on
        // the value, for whoever is drawing it. It warns rather than refusing: this is evidence of
        // a bad parse, not a permission decision.
        Ok(Identity {
            serial: c.serial,
            guid,
            source: Source::RealDevice,
        })
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
            let Ok(entries) = std::fs::read_dir(root) else {
                continue;
            };
            for e in entries.flatten() {
                let vol = e.path();
                // One level on macOS (`/Volumes/<volume>`), two on Linux (`/media/<user>/<volume>`).
                // Checking the entry and then its children covers both without knowing which we are.
                let nested = std::fs::read_dir(&vol)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|c| c.path());
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
        // See `from_nor`: the warning is [`Identity::oui_warning`], not a line on stderr.
        Ok(Identity {
            serial,
            guid,
            source: Source::RealDevice,
        })
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
                if !s
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                {
                    return Err(format!("a serial is letters and digits only: {s}"));
                }
                Some(s)
            }
        };
        if guid >> 40 != APPLE_OUI {
            // **This one stays on stderr, and its two siblings did not.**
            //
            // [`Identity::oui_warning`] answers for a `RealDevice`, where the reading is *this dump
            // may not have parsed* and a window has an identity in hand to draw it beside. A
            // `Provided` value is a claim rather than evidence, and the window refuses a foreign
            // OUI at [`Identity::check_guid`] before anything reaches here — so the only caller
            // that can still get here is `ipod-boot --guid`, which has no surface and no other
            // channel. Deleting it would have removed the warning rather than moved it.
            //
            // **Retirement condition**: `ipod-boot`'s `--guid` parsing routes through `check_guid`,
            // at which point this arm is unreachable and comes out.
            eprintln!(
                "warning: {guid:016X} does not start with Apple's FireWire OUI ({APPLE_OUI:06X})."
            );
        }
        Ok(Identity {
            serial,
            guid,
            source: Source::Provided,
        })
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

        // **The model is part of the seed, not merely a source of digits.**
        //
        // Only `year_digits`, `serial_codes` and `colour` were read from the model, and all three
        // are properties of the *generation* — so every 5G produced the same serial and the same
        // GUID for a given seed, and choosing a 60 GB instead of a 30 GB changed the `Mod#` and
        // nothing a person could see. "I picked a different iPod and got the same one" is not a
        // subtle failure; it is the screen saying the choice did not take.
        //
        // Mixing the model number in makes the identity a function of the whole choice while
        // keeping the property that matters: same model and same seed, same iPod, every launch.
        let mut st = seed;
        for b in model.number.bytes() {
            st = st.wrapping_mul(0x0100_0000_01B3) ^ b as u64;
        }
        let pick = |st: &mut u64, n: usize| -> String {
            (0..n)
                .map(|_| A[(mix(st) % A.len() as u64) as usize] as char)
                .collect()
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
        Identity {
            serial: Some(serial),
            guid,
            source: Source::Generated,
        }
    }

    /// Whether a string is shaped like an Apple iPod serial, and what is wrong if not.
    ///
    /// **Eleven characters: `LLYWWUUUCCC`.** Two for the manufacturing location, one for the year,
    /// two for the week, three unique, three for the model. This is the same layout
    /// [`Identity::generate`] assembles, read backwards — so a serial somebody types is held to the
    /// shape the program itself produces, rather than accepted and quietly written into a `SysCfg`
    /// where nothing will ever check it.
    ///
    /// What is deliberately **not** checked: whether the location prefix or the model code is one
    /// Apple used. Those sets are short and observed rather than published, and refusing a real
    /// iPod's serial because its factory is not in a list of three would be worse than accepting an
    /// invented one.
    pub fn check_serial(s: &str) -> Result<(), String> {
        Self::check_serial_for(s, None)
    }

    /// The same check, **against a particular iPod** when one is known.
    ///
    /// **What this can and cannot say.** A real serial's last three characters are its model code,
    /// and its year digit is the year it was made — both are checkable against a chosen model, and
    /// neither was checked at all. What is *not* checkable is which code means which capacity:
    /// `serial_codes` says so itself — "which capacity each denotes is not known and is not
    /// claimed" — because the published list does not separate them and a real `MA446`, a 5.5G, was
    /// observed here ending `V9M`, which is on the 5th-generation list.
    ///
    /// So this refuses an ending no iPod of that generation is known to carry, and a year that
    /// generation was not made in, and stops there. Inventing a code-to-capacity mapping would put
    /// a fabricated fact into a `SysCfg` that nothing downstream would ever question.
    pub fn check_serial_for(s: &str, model: Option<&'static Model>) -> Result<(), String> {
        Self::check_serial_at(s, model).map_err(|r| r.why)
    }

    /// The same check, **worded twice** — see [`Refusal`].
    ///
    /// **Five arms quote the input and one did not have a twin**: the `O`, the lower-case letter,
    /// the character outside the alphabet, the year digit and the model code. Only the model code
    /// falls outside the mask, so four of them needed one.
    ///
    /// The lengths are still counted in **bytes**, exactly as this has always done, and that is not
    /// an oversight to tidy: the ASCII scan below runs before any slicing, so every character is one
    /// byte by the time `s[3..5]` and `s.as_bytes()[2]` are reached. [`Identity::provided`] counts
    /// in `chars()`, which is safe for its own input; unifying the two the careless way puts a panic
    /// on multibyte text into a field a person types into.
    pub fn check_serial_at(s: &str, model: Option<&'static Model>) -> Result<(), Refusal> {
        const A: &str = "0123456789ABCDEFGHIJKLMNPQRSTUVWXYZ";
        // A count, not a character: nothing here is hidden, so the two wordings are one.
        if s.len() != 11 {
            return Err(Refusal::same(format!(
                "a serial is 11 characters — this is {}. The shape is LLYWWUUUCCC: location, \
                 year, week, three unique, three for the model.",
                s.len()
            )));
        }
        if let Some(c) = s.chars().find(|c| !A.contains(*c)) {
            return Err(if c == 'O' {
                Refusal {
                    why: "`O` does not appear in Apple serials — it would be read as a zero."
                        .into(),
                    masked: "one of the characters is `O`, which Apple does not use — it would be \
                             read as a zero."
                        .into(),
                }
            } else if c.is_ascii_lowercase() {
                Refusal {
                    why: format!("serials are upper case; `{c}` is not."),
                    masked: "one of the characters is lower case; serials are upper case.".into(),
                }
            } else {
                Refusal {
                    why: format!("`{c}` is not a character Apple serials use."),
                    masked: "one of the characters is not one Apple serials use — they are digits \
                             and upper-case letters, and never O."
                        .into(),
                }
            });
        }
        let week: u32 = s[3..5]
            .parse()
            .map_err(|_| Refusal::same("the week is not a number".to_string()))?;
        if !(1..=52).contains(&week) {
            return Err(Refusal {
                // `week` is parsed out of two hidden positions, so the number IS the characters.
                why: format!("week {week} is not a week — positions 4 and 5 are 01 to 52."),
                masked: "the fourth and fifth characters are the week, and a week is 01 to 52."
                    .into(),
            });
        }
        let Some(m) = model else { return Ok(()) };

        // The year digit, position 3, against the years that generation was made in.
        let years = m.generation.year_digits();
        if !years.is_empty() {
            let y = s.as_bytes()[2] - b'0';
            if !years.contains(&y) {
                let made_in = years
                    .iter()
                    .map(|y| format!("200{y}"))
                    .collect::<Vec<_>>()
                    .join(" or ");
                return Err(Refusal {
                    why: format!(
                        "the {} was made in {made_in}, so its serial's third character is one of \
                         those — not `{}`.",
                        m.generation.label(),
                        s.as_bytes()[2] as char
                    ),
                    // `made_in` comes from `year_digits`, never from the input — which is the whole
                    // difference between the two sentences.
                    masked: format!(
                        "the third character is the year, and {made_in} are the only ones this \
                         generation was made in."
                    ),
                });
            }
        }

        // The last three, against the codes real iPods of that generation carry.
        let codes = m.generation.serial_codes();
        if !codes.is_empty() && !codes.contains(&&s[8..]) {
            // It quotes `s[8..]`, which is the model code — the three characters `MASK_SERIAL_TAIL`
            // leaves on screen. Nothing is hidden, so the two wordings are one.
            return Err(Refusal::same(format!(
                "`{}` is not an ending seen on a {} — the ones this project has records for are {}. \
                 Which capacity each denotes is not known, so that part is not checked.",
                &s[8..],
                m.generation.label(),
                codes.join(", ")
            )));
        }
        Ok(())
    }

    /// Whether a string is a FireWire GUID an iPod could have, and what is wrong if not.
    ///
    /// **Sixteen hex digits whose top 24 bits are Apple's OUI.** Every iPod's GUID begins
    /// `00:0A:27`; a number that does not is not one, whoever typed it. That is the whole of the
    /// check, because the 40 bits below it are a serial number Apple allocated and nothing about
    /// them is derivable.
    ///
    /// It is worth being able to type at all — the argument for locking it was that it is not
    /// printed on the case, which is an argument about *convenience*, not possibility. It is
    /// readable from a real iPod through `SysInfo` and iTunes, and this program parses it out of
    /// dumps itself.
    pub fn check_guid(s: &str) -> Result<(), String> {
        Self::check_guid_at(s).map_err(|r| r.why)
    }

    /// The same check, worded twice — see [`Refusal`].
    ///
    /// **All three arms set `masked == why`, and there must not be a fourth.** The length sentence
    /// quotes a count; *that is not hexadecimal* quotes nothing; and the OUI sentence quotes the top
    /// six hex digits, which [`MASK_GUID_HEAD`] leaves on screen. Every one of them is already safe
    /// against a masked field, so a twin would be a second string to keep in step with no rule
    /// asking for it.
    pub fn check_guid_at(s: &str) -> Result<(), Refusal> {
        let t = s.trim().trim_start_matches("0x");
        if t.len() != 16 {
            return Err(Refusal::same(format!(
                "a GUID is 16 hex digits — this is {}. `000A270014EFE726` is the shape.",
                t.len()
            )));
        }
        let v = u64::from_str_radix(t, 16)
            .map_err(|_| Refusal::same("that is not hexadecimal — 0-9 and A-F only.".to_string()))?;
        if v >> 40 != APPLE_OUI {
            return Err(Refusal::same(format!(
                "every iPod's GUID starts {APPLE_OUI:06X} — Apple's FireWire OUI. This one starts \
                 {:06X}, so it belongs to some other maker's hardware.",
                v >> 40
            )));
        }
        Ok(())
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

    /// The strongest evidence there is that a dump did not parse, **as a sentence a surface can
    /// draw** rather than a line on stderr nothing reads.
    ///
    /// `Some` only for a [`Source::RealDevice`] whose top 24 bits are not [`APPLE_OUI`]. Every real
    /// iPod's GUID begins `00:0A:27`, observed independently in the NOR's `SysCfg`, in the handoff
    /// block and in the drive's own `SysInfo` — so a read value that does not is a parse that went
    /// wrong, and saying so beside the value is the point.
    ///
    /// **`None` for [`Source::Generated`] and [`Source::Provided`], and neither is an oversight.**
    /// A generated GUID is built with the OUI in it and cannot fail this. A provided one is a claim
    /// rather than evidence, and the window refuses it at [`Identity::check_guid`] before it is
    /// ever an `Identity` — a warning after a refusal is a warning about something that did not
    /// happen.
    pub fn oui_warning(&self) -> Option<String> {
        if self.source != Source::RealDevice || self.guid >> 40 == APPLE_OUI {
            return None;
        }
        Some(format!(
            "This one starts {:06X} rather than {APPLE_OUI:06X}, Apple's FireWire OUI — so either \
             it is not an iPod's, or it did not parse.",
            self.guid >> 40
        ))
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
            Colour::White,
            Colour::Black,
            Colour::U2,
            Colour::Silver,
            Colour::Blue,
            Colour::Gold,
            Colour::Green,
            Colour::Pink,
            Colour::Orange,
            Colour::Purple,
            Colour::Red,
            Colour::Yellow,
            Colour::Stainless,
            Colour::Unspecified,
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

    /// The last three characters of serials real devices of this generation carry.
    ///
    /// **The two Video revisions do not share a set, and believing they did was a misreading.**
    /// This used to hand the 5.5G's published list to both, reasoning that "the published list does
    /// not separate the 5G from the 5.5G". It does: Apple gives that list under *iPod (5th
    /// generation Late 2006)*, and the plain *iPod (5th generation)* entry carries no list at all.
    ///
    /// The two codes this project observed on hardware settle which is which, and they resolve a
    /// loose end in research/16 rather than adding one. Their date fields are:
    ///
    /// ```text
    ///   4J 6 08 2Y7 TXK    week 08 of 2006    from the NOR's handoff block
    ///   JQ 5 51 Y5H TXM    week 51 of 2005    from the drive's SysInfo
    /// ```
    ///
    /// The Late 2006 model was introduced in **September 2006**, so a device built in week 51 of
    /// 2005 or week 08 of 2006 cannot be one. Both are original 5Gs. research/16 concluded from
    /// their absence from Apple's list that "the published tables are incomplete"; they are not —
    /// the table is for the *other* revision.
    ///
    /// And the corroboration runs the other way too: a real `MA446`, which this table puts in
    /// `Video2`, was observed here ending `V9M`, which is on the Late 2006 list.
    ///
    /// **Which capacity each code denotes is still not known and is not claimed.** Apple publishes
    /// thirteen endings for one revision and no mapping from them to 30 GB or 80 GB, and nothing
    /// here invents one.
    pub fn serial_codes(self) -> &'static [&'static str] {
        // Apple's published endings for the iPod (5th generation Late 2006).
        // <https://support.apple.com/en-us/103823>
        const LATE_2006: &[&str] = &[
            "V9K", "V9P", "V9M", "V9R", "V9L", "V9N", "V9Q", "V9S", "WU9", "WUA", "WUB", "WUC",
            "X3N",
        ];
        // The original 5G, for which Apple publishes none. These are the two this project has
        // seen, and a pool of two is small — but a generated serial ending in one of them ends
        // where a real iPod of that revision ended, which is the whole point of the list.
        const INITIAL: &[&str] = &["TXK", "TXM"];
        match self {
            Generation::Video1 => INITIAL,
            Generation::Video2 => LATE_2006,
            _ => &[],
        }
    }

    /// The `UpdaterFamilyID`s Apple shipped firmware under for this generation.
    ///
    /// **This is the only thing that separates a 5G from a 5.5G in a firmware bundle**, because
    /// they share `FamilyID` 6. Read out of Apple's own restore manifests: `Firmware-13.6.3` and
    /// `Firmware-20.6.3` are the 5G's Initial and Rev A, `Firmware-25.6.3` is the 5.5G's. The
    /// number is also the one in the filename.
    ///
    /// Empty where the mapping has not been established, and an empty list means "cannot say"
    /// rather than "nothing matches" — a check on it has to treat the two differently.
    pub fn updater_families(self) -> &'static [u32] {
        match self {
            Generation::Video1 => &[13, 20],
            Generation::Video2 => &[25],
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
        let key: String = s
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if key.len() != 4 || !key[1..].chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        MODELS.iter().find(|m| m.number == key)
    }

    /// Look one up from the text of a `SysInfo`.
    pub fn from_sysinfo(text: &str) -> Option<&'static Model> {
        Model::lookup(&sysinfo_field(text, "ModelNumStr")?)
    }

    /// The model number **in the form the hardware writes it** — `MA146`, not the table key.
    ///
    /// [`Model::number`] is libgpod's key, which has had one leading letter stripped for lookup.
    /// Apple's flash writes the full form in its `Mod#` record and in the handoff block, and the
    /// drive's `SysInfo` writes that with a further `x` in front again. Writing the key where the
    /// hardware writes the full form produces a NOR that differs from a real one in a field
    /// software actually reads.
    pub fn apple_number(&self) -> String {
        format!("M{}", self.number)
    }

    /// The drive size this model shipped with, in 512-byte sectors.
    ///
    /// Decimal GB, the way drives are labelled and the way Apple advertised them — a "30 GB" iPod
    /// is 30 000 000 000 bytes, not 30 GiB, and reports about 27.9 GiB formatted.
    ///
    /// **This is cheap to honour.** `build_disk` creates the image with `set_len`, so it is sparse:
    /// an 80 GB drive occupies the ~14 MB actually written on APFS, ext4, btrfs and NTFS. And the
    /// 5.5G's notorious 80 GB problem — 1024-byte physical sectors — does not reach us, because in
    /// an emulator we supply the drive.
    pub fn sectors(&self) -> u64 {
        (self.capacity_gb as u64) * 1_000_000_000 / 512
    }

    /// SDRAM, in bytes.
    ///
    /// **Memory follows the CAPACITY, not the generation** — a 60 GB 5G has 64 MB while a 30 GB
    /// 5.5G has 32 MB, so "the 5.5G has double the RAM" is wrong in both directions. The two
    /// generations are revisions of one platform: same PP5021C, same BCM2722, same WM8758, same
    /// 320x240 panel; what changed is the board (820-1763-A to 820-1975-A), a brighter screen, and
    /// Search in the firmware.
    ///
    /// **Not measured here, and it is contradicted by our own hardware.** Our unit is a 30 GB 5G,
    /// which this rule puts at 32 MB — and the word at `+0xe0` of its real handoff block reads
    /// `0x04000000`. Either that field is not the memory size (the likelier reading, and it is
    /// already flagged as not understood) or the rule is wrong. Until one of those is settled this
    /// value is not written into the handoff; it is here to be tested against.
    pub fn sdram_bytes(&self) -> u32 {
        if self.capacity_gb >= 60 {
            64 * 1024 * 1024
        } else {
            32 * 1024 * 1024
        }
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
        assert_ne!(
            Identity::generate(m, 42).guid,
            Identity::generate(m, 43).guid
        );
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
            assert!(
                s.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
                "{s}"
            );

            assert!(
                LOCATIONS.contains(&&s[0..2]),
                "seed {seed}: {s} — not a real factory prefix"
            );
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

    /// Capacity is decimal GB, and the sector count follows the model rather than a constant.
    ///
    /// This existed as a hardcoded 8 GiB while the reference model was a 30 GB `MA146`, so the
    /// About screen and the model name had never agreed. RetailOS reads the size from ATA, so
    /// making them agree is something we do, not something that happens.
    #[test]
    fn the_drive_size_follows_the_model() {
        let thirty = Model::lookup("MA146").expect("MA146");
        assert_eq!(thirty.capacity_gb, 30);
        assert_eq!(thirty.sectors(), 30_000_000_000 / 512);
        // 80 GB is 5.5G-only, and is the one that would have been a real file without sparseness.
        let eighty = Model::lookup("MA448").expect("MA448");
        assert_eq!(eighty.capacity_gb, 80);
        assert_eq!(eighty.sectors(), 80_000_000_000 / 512);
        // Bigger model, bigger drive — and both larger than the 8 GiB constant they replaced.
        assert!(eighty.sectors() > thirty.sectors());
        assert!(thirty.sectors() > 16_777_216);
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
        assert!(
            s.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
            "{s}"
        );
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
            Identity::provided(Some("AB1234XYZQR"), 0x000A_2700_1122_3344)
                .unwrap()
                .title_auth(),
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
        assert_eq!(
            Identity::provided(Some("   "), 0x000A_2700_0000_0001)
                .unwrap()
                .serial,
            None
        );
        assert_eq!(
            Identity::provided(None, 0x000A_2700_0000_0001)
                .unwrap()
                .serial,
            None
        );
    }

    /// Malformed serials are refused; a non-Apple OUI is *not*, per the rule adopted from
    /// `ipod-usb-new` — it is evidence of a bad parse, not a permission decision.
    #[test]
    fn malformed_serials_are_refused_but_a_foreign_oui_is_allowed() {
        for bad in ["TOOSHORT", "WAYTOOLONGSERIAL", "AB1234-YZQR"] {
            assert!(
                Identity::provided(Some(bad), 0x000A_2700_0000_0001).is_err(),
                "must reject {bad:?}"
            );
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
        assert_eq!(
            Model::lookup("MA448").unwrap().generation,
            Generation::Video2
        );
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

#[cfg(test)]
mod serial_tests {
    use super::*;

    /// **A different iPod has to produce a different iPod.** Only the generation and the colour
    /// were read from the model, so every 5G shared a serial for a given seed and picking a 60 GB
    /// instead of a 30 GB changed nothing anybody could see.
    #[test]
    fn two_models_of_one_generation_get_different_identities() {
        let mut seen = std::collections::BTreeSet::new();
        let mut n = 0;
        for m in crate::models::MODELS
            .iter()
            .filter(|m| m.model == crate::models::IpodModel::VideoWhite)
        {
            let id = Identity::generate(m, 5);
            seen.insert((id.serial.clone(), id.guid));
            n += 1;
        }
        assert!(
            n >= 2,
            "the test needs at least two Video models, found {n}"
        );
        assert_eq!(
            seen.len(),
            n,
            "{n} models produced {} identities",
            seen.len()
        );
    }

    /// And the property that made it a *seed* rather than a random number still holds.
    #[test]
    fn the_same_model_and_seed_is_the_same_ipod_every_time() {
        let m = crate::models::MODELS
            .iter()
            .find(|m| m.model == crate::models::IpodModel::VideoWhite)
            .unwrap();
        let a = Identity::generate(m, 7);
        let b = Identity::generate(m, 7);
        assert_eq!(a.serial, b.serial);
        assert_eq!(a.guid, b.guid);
        assert_ne!(
            Identity::generate(m, 8).serial,
            a.serial,
            "the seed stopped mattering"
        );
    }

    /// **Everything this program generates must pass its own validator.** A checker stricter than
    /// the generator rejects the program's own output, which is the one thing it must never do.
    #[test]
    fn every_generated_serial_is_a_valid_serial() {
        for m in crate::models::MODELS.iter().take(60) {
            for seed in 0..8 {
                let id = Identity::generate(m, seed);
                let s = id.serial.clone().unwrap();
                Identity::check_serial(&s).unwrap_or_else(|e| {
                    panic!(
                        "{} seed {seed} generated {s}, which it rejects: {e}",
                        m.number
                    )
                });
            }
        }
    }

    /// The GUID check: Apple's OUI, sixteen hex digits, and every generated one passes it.
    #[test]
    fn a_guid_has_to_be_one_an_ipod_could_have() {
        for m in crate::models::MODELS.iter().take(24) {
            for seed in 0..4 {
                let g = Identity::generate(m, seed).guid_hex();
                Identity::check_guid(&g)
                    .unwrap_or_else(|e| panic!("{} generated {g} and rejects it: {e}", m.number));
            }
        }
        assert!(Identity::check_guid("000A270014EFE726").is_ok());
        assert!(
            Identity::check_guid("0x000A270014EFE726").is_ok(),
            "an 0x prefix is ordinary"
        );
        for (bad, want) in [
            ("000A270014EFE72", "16 hex digits"),
            ("000A270014EFE7266", "16 hex digits"),
            ("000A270014EFE72Z", "hexadecimal"),
            ("001B6300ABCDEF01", "Apple's FireWire OUI"),
        ] {
            let e = Identity::check_guid(bad).unwrap_err();
            assert!(e.contains(want), "{bad}: expected {want:?}, got {e:?}");
        }
    }

    /// **Model-aware checking, and the generator must still pass it.** A validator stricter than
    /// the generator rejects the program's own output; a validator that ignores the model accepts a
    /// 5.5G's serial on a 5G, which is what it used to do.
    #[test]
    fn a_serial_is_checked_against_the_ipod_it_claims_to_be() {
        let video = |g| {
            crate::models::MODELS
                .iter()
                .find(|m| m.generation == g && m.model == crate::models::IpodModel::VideoWhite)
                .unwrap()
        };
        let g5 = video(crate::models::Generation::Video1);
        let g55 = video(crate::models::Generation::Video2);

        // Everything generated for a model passes that model's own check.
        for m in [g5, g55] {
            for seed in 0..16 {
                let s = Identity::generate(m, seed).serial.unwrap();
                Identity::check_serial_for(&s, Some(m))
                    .unwrap_or_else(|e| panic!("{} generated {s} and rejects it: {e}", m.number));
            }
        }

        // A 5G serial has a 2005 or 2006 digit; 2007 is the 5.5G's and is refused for a 5G.
        let bad_year = "4J7011K2V9K";
        assert!(
            Identity::check_serial_for(bad_year, Some(g5))
                .unwrap_err()
                .contains("made in"),
            "a 2007 digit was accepted on a 5G"
        );

        // An ending no iPod carries is refused, with the list.
        let bad_code = "4J5011K2AAA";
        let e = Identity::check_serial_for(bad_code, Some(g5)).unwrap_err();
        assert!(
            e.contains("AAA"),
            "the reason does not name the ending: {e}"
        );
        assert!(
            e.contains("TXK"),
            "the reason does not offer the real ones: {e}"
        );

        // **And the two revisions do not share a set.** Apple publishes the V9-and-WU list under
        // *iPod (5th generation Late 2006)* only; the two codes seen on hardware here date to week
        // 51 of 2005 and week 08 of 2006, before that model existed. So each list belongs to one
        // revision, and each is refused on the other.
        let late_ending = "4J6011K2V9K"; // a Late 2006 code on a 2006-built 5G
        assert!(
            Identity::check_serial_for(late_ending, Some(g5)).is_err(),
            "a Late 2006 ending was accepted on the original 5G"
        );
        let initial_ending = "4J6011K2TXK";
        assert!(
            Identity::check_serial_for(initial_ending, Some(g55)).is_err(),
            "an original-5G ending was accepted on a Late 2006"
        );
        assert!(Identity::check_serial_for(late_ending, Some(g55)).is_ok());
        assert!(Identity::check_serial_for(initial_ending, Some(g5)).is_ok());

        // **And the capacity is deliberately not checked.** The same ending is accepted on a 30 GB
        // and a 60 GB, because which code means which capacity is not known — `serial_codes` says
        // so, and a real 5.5G was observed carrying an ending from the 5th-generation list.
        let g5_60 = crate::models::MODELS
            .iter()
            .find(|m| {
                m.generation == crate::models::Generation::Video1
                    && m.capacity_gb == 60
                    && m.model == crate::models::IpodModel::VideoWhite
            })
            .unwrap();
        let s = Identity::generate(g5, 3).serial.unwrap();
        assert!(
            Identity::check_serial_for(&s, Some(g5_60)).is_ok(),
            "a capacity claim crept into the check"
        );

        // With no model in hand it is the old, shape-only check — which is what a dump gets.
        assert!(Identity::check_serial_for(bad_code, None).is_ok());
    }

    // ── masking, and the refusals that would defeat it — GUI.md §11.2 ─────────────────────────

    /// A serial that is valid for the 5.5G, so a substitution at one position is the only thing
    /// wrong with it.
    const GOOD: &str = "4J6011K2V9K";

    fn g55() -> &'static Model {
        crate::models::MODELS
            .iter()
            .find(|m| {
                m.generation == crate::models::Generation::Video2
                    && m.model == crate::models::IpodModel::VideoWhite
            })
            .expect("a 5.5G")
    }

    fn substitute(s: &str, i: usize, c: char) -> String {
        let mut b: Vec<char> = s.chars().collect();
        b[i] = c;
        b.into_iter().collect()
    }

    /// **The whole of the masking rule, and it is red the moment any arm renders `why` verbatim.**
    ///
    /// The mask hides byte indices 2..=7 — the year digit, the two week digits and the three unique
    /// characters. So a validation sentence that quotes any of them back puts on screen exactly
    /// what the row above is hiding, and it does it at the moment somebody is typing.
    ///
    /// The statement is a closed set: **however a hidden character is changed, the masked wording
    /// comes from a short fixed list**, none of whose members carries the character. The `why`
    /// column is swept in the same loop and has to be *larger*, or the test is comparing a constant
    /// with itself.
    #[test]
    fn the_masked_reason_does_not_change_when_a_masked_character_changes() {
        assert!(Identity::check_serial_for(GOOD, Some(g55())).is_ok(), "the fixture is not valid");

        let mut masked = std::collections::BTreeSet::new();
        let mut whys = std::collections::BTreeSet::new();
        let mut arms = 0;
        // Every hidden position, against characters that trip each arm in turn.
        for i in MASK_SERIAL_HEAD..(GOOD.len() - MASK_SERIAL_TAIL) {
            for c in "0123456789ABKMOZabkmz!# -".chars() {
                let s = substitute(GOOD, i, c);
                let Err(r) = Identity::check_serial_at(&s, Some(g55())) else {
                    continue;
                };
                arms += 1;
                assert!(!r.masked.is_empty(), "{s} refused with no masked wording");
                masked.insert(r.masked.clone());
                whys.insert(r.why.clone());
            }
        }
        assert!(arms > 40, "the sweep tripped almost nothing: {arms}");

        let expected: std::collections::BTreeSet<String> = [
            "one of the characters is `O`, which Apple does not use — it would be read as a zero.",
            "one of the characters is lower case; serials are upper case.",
            "one of the characters is not one Apple serials use — they are digits and upper-case \
             letters, and never O.",
            "the fourth and fifth characters are the week, and a week is 01 to 52.",
            "the third character is the year, and 2006 or 2007 are the only ones this generation \
             was made in.",
            "the week is not a number",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(
            masked, expected,
            "a masked refusal is worded from the input rather than from the position"
        );
        assert!(
            whys.len() > masked.len(),
            "the unmasked column did not vary, so this test compared a constant with itself: \
             {} whys against {} masked",
            whys.len(),
            masked.len()
        );

        // And nothing in the masked column ever carries a character out of a hidden position.
        for m in &masked {
            for i in MASK_SERIAL_HEAD..(GOOD.len() - MASK_SERIAL_TAIL) {
                for c in "123489ZMzmk!#".chars() {
                    let s = substitute(GOOD, i, c);
                    if let Err(r) = Identity::check_serial_at(&s, Some(g55())) {
                        if r.masked == *m {
                            assert!(
                                !m.contains(&format!("`{c}`")),
                                "{m:?} quotes the hidden character {c:?} back"
                            );
                        }
                    }
                }
            }
        }
    }

    /// **Two wordings, one rule.** A masked twin that fired where the verbatim one did not — or
    /// that came back empty — would be a field refusing for one reason and explaining another.
    #[test]
    fn the_masked_and_unmasked_refusals_are_the_same_rule() {
        let inputs = [
            "", "SHORT", "4J6011K2V9K", "4J6011K2V9KX", "4JO011K2V9K", "4j6011k2v9k",
            "4J6011K2V9!", "4J6991K2V9K", "4J6001K2V9K", "4J6XX1K2V9K", "4J1011K2V9K",
            "4J6011K2AAA", "ÿÿÿÿÿÿÿÿÿÿÿ",
        ];
        for s in inputs {
            for model in [None, Some(g55())] {
                let at = Identity::check_serial_at(s, model);
                let old = Identity::check_serial_for(s, model);
                assert_eq!(at.is_err(), old.is_err(), "{s:?} refuses one way and not the other");
                if let Err(r) = at {
                    assert!(!r.why.is_empty(), "{s:?} refused with no reason");
                    assert!(!r.masked.is_empty(), "{s:?} refused with no masked reason");
                    // The one reader, and it picks by the field's state and nothing else.
                    assert_eq!(r.text(true), r.why);
                    assert_eq!(r.text(false), r.masked);
                }
            }
        }
    }

    /// The old signature is the new one with the twin dropped, and every caller that had it keeps
    /// exactly the sentence it had.
    #[test]
    fn check_serial_for_still_returns_the_sentence_it_always_did() {
        for (bad, want) in [
            ("7Q7411K2VQ", "11 characters"),
            ("7QO411K2VQK", "read as a zero"),
            ("7q7411k2vqk", "upper case"),
            ("7Q7411K2VQ!", "not a character"),
            ("7Q7991K2VQK", "not a week"),
        ] {
            let e = Identity::check_serial(bad).unwrap_err();
            assert!(e.contains(want), "{bad}: expected {want:?}, got {e:?}");
            assert_eq!(e, Identity::check_serial_at(bad, None).unwrap_err().why);
        }
        for bad in ["000A270014EFE72", "000A270014EFE72Z", "001B6300ABCDEF01"] {
            assert_eq!(
                Identity::check_guid(bad).unwrap_err(),
                Identity::check_guid_at(bad).unwrap_err().why
            );
        }
    }

    /// **The boundary is derived, not chosen**: the mask hides exactly the positions a refusal can
    /// quote, and shows exactly the ones it may.
    ///
    /// The model code — the last three — is quoted verbatim by the ending rule and is *shown*,
    /// deliberately: the Model row two lines above states it outright, so hiding it would be
    /// theatre. The GUID's top six are Apple's published OUI, which this program declares as a
    /// constant and quotes in its own refusal.
    #[test]
    fn the_mask_hides_every_character_a_refusal_can_quote() {
        let m = mask_serial(GOOD);
        assert_eq!(m, "4J******V9K", "the mask moved");
        for (i, c) in m.chars().enumerate() {
            let hidden = i >= MASK_SERIAL_HEAD && i < GOOD.chars().count() - MASK_SERIAL_TAIL;
            assert_eq!(c == MASK_CHAR, hidden, "position {i} of {m}");
        }

        // The ending rule quotes the model code, and the model code is on screen.
        let e = Identity::check_serial_at("4J6011K2AAA", Some(g55())).unwrap_err();
        assert!(e.why.contains("AAA"));
        assert_eq!(e.masked, e.why, "a visible quote was given a twin it does not need");
        assert!(mask_serial("4J6011K2AAA").ends_with("AAA"));

        // The GUID's own refusal quotes the top six, which the mask shows.
        let g = Identity::check_guid_at("001B6300ABCDEF01").unwrap_err();
        assert!(g.why.contains("001B63"));
        assert_eq!(g.masked, g.why);
        assert_eq!(mask_guid_hex("001B6300ABCDEF01"), "001B63**********");
    }

    /// Pressing `Show` must move nothing, so a mask is the width of what it hides — computed from
    /// the value's own length and never typed.
    #[test]
    fn the_mask_is_the_same_width_as_the_value() {
        for s in ["4J6011K2V9K", "", "A", "SHORT", "WAYTOOLONGSERIALINDEED", "ÿÿÿ"] {
            assert_eq!(
                mask_serial(s).chars().count(),
                s.chars().count(),
                "mask_serial({s:?})"
            );
        }
        for g in ["000A270014EFE726", "", "000A27", "000A270014EFE7266"] {
            assert_eq!(
                mask_guid_hex(g).chars().count(),
                g.chars().count(),
                "mask_guid_hex({g:?})"
            );
        }
        // Anything that is not the shape is masked whole rather than sliced at positions that mean
        // nothing in it.
        assert_eq!(mask_serial("SHORT"), "*****");
        assert_eq!(mask_guid_hex("000A27"), "******");
    }

    /// The window's glyph set is closed and nothing in `.slint` can ask whether a glyph exists, so
    /// a mask character outside the proven set ships `.notdef` squares over somebody's serial.
    #[test]
    fn a_masked_identifier_is_ascii_and_in_the_closed_glyph_set() {
        assert!(MASK_CHAR.is_ascii(), "the mask character is not ASCII");
        assert_eq!(MASK_CHAR, '*');
        for s in ["4J6011K2V9K", "SHORT", ""] {
            assert!(mask_serial(s).is_ascii(), "{s:?}");
        }
        assert!(mask_guid_hex("000A270014EFE726").is_ascii());
    }

    /// **The one UI decision with an irreversible consequence, said where the decision is made.**
    /// Three sources, three answers, and no two of them the same.
    #[test]
    fn every_title_auth_variant_says_something_and_never_the_same_thing() {
        let mut seen = std::collections::BTreeSet::new();
        for t in TitleAuth::ALL {
            let l = t.line();
            assert!(!l.is_empty(), "{t:?} says nothing");
            assert!(l.ends_with('.'), "{t:?} is not a sentence: {l}");
            assert!(seen.insert(l), "two sources share a sentence: {l}");
        }
        assert_eq!(seen.len(), 3);

        // And they are attached to the sources that actually produce them.
        let m = Model::lookup("MA146").expect("MA146");
        assert_eq!(
            Identity::generate(m, 1).title_auth().line(),
            TitleAuth::Never.line()
        );
        assert!(TitleAuth::Never.line().contains("never authorise"));
    }

    /// **GUI.md §11.2's three behaviours, and the UI must not flatten them.** A typed non-Apple OUI
    /// is refused, because a typed field is a claim; one read out of a real file warns, because a
    /// dump is evidence.
    #[test]
    fn a_typed_non_apple_oui_is_refused_and_a_read_one_warns() {
        // Typed: refused, with the OUI named.
        let e = Identity::check_guid("001B6300ABCDEF01").unwrap_err();
        assert!(e.contains("001B63"), "{e}");
        assert!(e.contains("Apple's FireWire OUI"), "{e}");

        // Read: accepted, and carrying its own warning as a value.
        let read = Identity::from_sysinfo(
            "pszSerialNumber: AB1234XYZQR\nFirewireGuid: 0x001B6300ABCDEF01\n",
        )
        .expect("a foreign OUI is evidence, not a refusal");
        assert_eq!(read.source, Source::RealDevice);
        let w = read.oui_warning().expect("a read foreign OUI must warn");
        assert!(w.contains("001B63"), "{w}");
        assert!(w.contains("did not parse"), "{w}");

        // An Apple one says nothing, so the warning is a signal rather than decoration.
        let fine = Identity::from_sysinfo(
            "pszSerialNumber: AB1234XYZQR\nFirewireGuid: 0x000A270011223344\n",
        )
        .unwrap();
        assert_eq!(fine.oui_warning(), None);

        // Generated identities carry the OUI by construction and cannot trip it.
        let m = Model::lookup("MA146").expect("MA146");
        assert_eq!(Identity::generate(m, 9).oui_warning(), None);
    }

    /// **A warning a window cannot see is a warning nobody reads.** Both of the identity readers
    /// printed the strongest evidence there is that a dump did not parse to stderr, where the one
    /// surface that draws that identity has no access to it.
    ///
    /// The source sweep is the half that catches a re-introduction: one `eprintln!` is left in this
    /// file, in [`Identity::provided`], and its doc says why and when it goes.
    #[test]
    fn no_model_warning_reaches_only_stderr() {
        // The body only: this test names the macro three times itself, and a sweep that counted
        // its own mentions would report four and be measuring nothing.
        let src = include_str!("identity.rs");
        let body = src.split("#[cfg(test)]").next().expect("a body");
        let printed = body.matches("eprintln!(").count();
        assert_eq!(
            printed, 1,
            "{printed} stderr warnings in identity.rs; the reader-facing ones are `oui_warning`"
        );
        // The one that is left is inside `provided`, which no window path reaches — nothing else
        // declared after it comes between the two.
        let provided_at = body.find("pub fn provided").expect("provided is still here");
        let printed_at = body.find("eprintln!(").expect("one is left");
        assert!(printed_at > provided_at, "the warning moved above `provided`");
        let after = provided_at + "pub fn provided".len();
        assert!(
            !body[after..printed_at].contains("pub fn "),
            "the surviving stderr warning moved out of `provided`"
        );

        // And both readers now answer with a value instead.
        let read = Identity::from_sysinfo("FirewireGuid: 0x001B6300ABCDEF01\n").unwrap();
        assert!(read.oui_warning().is_some());
    }

    /// And it has to refuse the things it exists to refuse, each with its own reason.
    #[test]
    fn the_validator_says_what_is_wrong() {
        for (bad, want) in [
            ("7Q7411K2VQ", "11 characters"),
            ("7Q7411K2VQKX", "11 characters"),
            ("7QO411K2VQK", "read as a zero"),
            ("7q7411k2vqk", "upper case"),
            ("7Q7411K2VQ!", "not a character"),
            ("7Q7991K2VQK", "not a week"),
            ("7Q7001K2VQK", "not a week"),
        ] {
            let e = Identity::check_serial(bad).unwrap_err();
            assert!(e.contains(want), "{bad}: expected {want:?}, got {e:?}");
        }
        assert!(
            Identity::check_serial("7Q7411K2VQK").is_ok(),
            "a real serial was refused"
        );
    }
}
