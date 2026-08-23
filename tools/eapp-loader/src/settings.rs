//! What the window remembers between launches, and the three directories it remembers it in.
//!
//! Deliberately a hand-written `key = value` file rather than eframe's `persistence` feature. That
//! feature pulls `serde` and `ron` into a crate whose whole dependency argument is "eframe and
//! nothing else", and it persists egui's own window geometry as a side effect, which is state this
//! project has no opinion about. Four keys do not need a serialiser.
//!
//! # Where the directories are, and why not `$TMPDIR` for all of them
//!
//! The snapshot cache used to live under [`std::env::temp_dir`]. That is correct on macOS, where
//! `$TMPDIR` is `/var/folders/…` on a real disk, and wrong on Linux, where `/tmp` is `tmpfs` on
//! most distributions — RAM-backed and typically capped at half of it. The cache holds a snapshot
//! **and an 8 GB working disk image**, so on a Linux box the old default either filled `/tmp` or
//! ate half the machine's memory. It is a cache directory now, on all three platforms.

use std::path::{Path, PathBuf};

// **There is no `Mode` here, and its deletion is the decision rather than an omission.**
//
// It was a two-value user/debug toggle: `Mode::Debug` was to show instruction counts, both clocks,
// the task watches, the ATA census, the framebuffer inspector and a screenshot button. Every one of
// those surfaces is now a *page* — §12.8's Readout — reached by navigating to it, and a page you
// can navigate to does not need a session-wide boolean deciding whether it exists.
//
// What it had become was a stored state, set once and forgotten, that silently changed what the
// window contained on a later launch. The one job left for it — hiding the Readout row — is a
// second navigation model layered over the first, and a menu row that is present or absent
// depending on a flag nobody remembers setting is worse than one that is always present.
//
// **`parse` ignores keys it does not know**, so a settings file carrying `mode = debug` is read
// exactly as it was before this went: the line is skipped, nothing complains, and the next `save`
// drops it. That is what makes this a deletion rather than a migration.

/// One thing in the resources list, and **the verb is what distinguishes them**.
///
/// The three kinds are not three flavours of the same thing — they attach to different places, and
/// that is the whole reason this is an enum rather than a `PathBuf` with a label:
///
/// ```text
///   Firmware   ->  chosen by a DEVICE      a boot ROM, real or synthesised
///   Installer  ->  makes a DISK            an Apple .ipsw
///   Software   ->  installs onto a DISK    Rockbox, ZeroSlackr, ipodloader2
/// ```
///
/// A flat list with no roles was tried first, and it is what made the settings page read as three
/// unrelated screens: a list of things with no verbs, beside a page of verbs with no target. Each
/// kind now appears only where its verb makes sense, so aiming the wrong one at the wrong place is
/// not a mistake you can make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resource {
    /// A 1 MB dump, or the recipe for synthesising one. **Chosen by a device**; never installed.
    Firmware(crate::nor::Source),
    /// An `.ipsw`. Not bootable itself and never installed onto anything: a drive is **built** from
    /// it, and the drive is what runs.
    Installer(PathBuf),
    /// A bootloader: `ipodloader2`, or Rockbox's. **Goes in the firmware partition**, which holds
    /// exactly one thing — which is why it is not filed with the software that goes on the volume.
    /// Everything called dual or triple boot is one of these offering the rest.
    Bootloader(PathBuf),
    /// An operating system, or a bundle of files for the volume. **Installed onto a disk** — it is
    /// not a machine part and cannot be run on its own.
    Software(PathBuf),
}

impl Resource {
    /// The word the settings file and the window both use.
    pub fn kind(&self) -> &'static str {
        match self {
            Resource::Firmware(_) => "firmware",
            Resource::Installer(_) => "installer",
            Resource::Bootloader(_) => "bootloader",
            Resource::Software(_) => "software",
        }
    }

    /// What this kind is *for*, in the words the window uses for it.
    pub fn verb(&self) -> &'static str {
        match self {
            Resource::Firmware(_) => "chosen by a device",
            Resource::Installer(_) => "makes a disk",
            Resource::Bootloader(_) => "goes in the firmware partition",
            Resource::Software(_) => "installs onto a disk",
        }
    }

    /// Where it is on disk, when it is a file at all. A synthesised ROM is a recipe and has none —
    /// which is the whole reason this returns an `Option`.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Resource::Firmware(crate::nor::Source::File(p)) => Some(p),
            Resource::Firmware(_) => None,
            Resource::Installer(p) | Resource::Software(p) | Resource::Bootloader(p) => Some(p),
        }
    }
}

/// How hard a fetched file was checked, and it is never rounded up.
///
/// Only ever appears inside [`Provenance::Fetched`]. `SizeOnly` is not a lesser spelling of
/// `Sha256`: `firmware.rs` keeps the two apart on purpose — the catalogue holds no hash for some
/// releases, and the cache listing hashes nothing by default — and a window that collapses them is
/// a window claiming a check that did not happen.
///
/// **Never `use Verification::*`.** [`Verification::None`] and [`Option::None`] would then be two
/// meanings of one word in the same match, and the compiler's complaint about it points somewhere
/// unhelpful. Spell it `Verification::None`, always.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verification {
    /// A SHA-256 was on record for this exact file and the bytes matched it.
    Sha256,
    /// A size was on record and matched; **no hash was**. Never renders as "verified".
    SizeOnly,
    /// Neither was on record, or the settings file did not say which.
    None,
}

/// Where a resource came from, so a row can say it rather than a string literal claiming it.
///
/// The shipped window printed `fetched and verified` for every installer and every piece of
/// software, and `dumped from a real iPod` for every ROM file, regardless — because [`Resource`]
/// carried a path and nothing else, so the model could not support the claim. This is the field
/// that supports it.
///
/// **Five variants, not the four `docs/GUI.md` §3.2 lists.** `Built` is the fifth: a file this
/// program produced from a vendored tree is neither fetched nor provided nor dumped, and a row that
/// has to say something about it would otherwise have to pick one of those and be wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// Read off somebody's own iPod.
    Dumped,
    /// Generated here from this seed. Derived from the recipe, never passed in — see [`normalised`].
    Synthesised { seed: u64 },
    /// Downloaded, and checked as hard as [`Verification`] says.
    Fetched { verified: Verification },
    /// The operator handed us this file. Nothing is known about it beyond that.
    Provided,
    /// Built here, out of a tree in this checkout.
    ///
    /// **Retirement condition**, in the shape `research/04` uses for bypasses: `docs/GUI.md` §20
    /// item 7 replaces the vendored `ipodloader2` with the fetched release. If, once the Composer's
    /// Build action exists, nothing files a resource with this, delete the variant rather than
    /// leave it standing — a variant with no producer is the claim-without-a-check §16.9 forbids.
    Built,
}

impl Provenance {
    /// The single word the settings file carries.
    ///
    /// **One token, not a `provenance` plus a `verified` pair.** This file already has one
    /// order-dependent pair of keys (`kind` before `path`) and a second is how a hand-edit files an
    /// unverified download as a verified one.
    pub fn token(&self) -> &'static str {
        match self {
            Provenance::Dumped => "dumped",
            Provenance::Synthesised { .. } => "synthesised",
            Provenance::Fetched {
                verified: Verification::Sha256,
            } => "fetched-sha256",
            Provenance::Fetched {
                verified: Verification::SizeOnly,
            } => "fetched-size",
            Provenance::Fetched {
                verified: Verification::None,
            } => "fetched",
            Provenance::Provided => "provided",
            Provenance::Built => "built",
        }
    }

    /// Back from the token. **Anything unrecognised is `None`** — an unreadable value is not a
    /// claim, and it must never fall through to a verified one.
    pub fn parse(s: &str) -> Option<Provenance> {
        match s.trim() {
            "dumped" => Some(Provenance::Dumped),
            // A placeholder seed. `normalised` overwrites it from the recipe, unconditionally.
            "synthesised" => Some(Provenance::Synthesised { seed: 0 }),
            "fetched-sha256" => Some(Provenance::Fetched {
                verified: Verification::Sha256,
            }),
            "fetched-size" => Some(Provenance::Fetched {
                verified: Verification::SizeOnly,
            }),
            "fetched" => Some(Provenance::Fetched {
                verified: Verification::None,
            }),
            "provided" => Some(Provenance::Provided),
            "built" => Some(Provenance::Built),
            _ => None,
        }
    }

    /// The trailing line a parts row draws.
    ///
    /// The `Verification::None` wording deliberately avoids the substring `verified`, so that
    /// `line().contains("verified")` is an exact test of the claim rather than a string that also
    /// matches "not verified".
    ///
    /// **The separator is `,` inside a list of facts and `—` between two clauses, and never `·`.**
    /// `·` is U+00B7, which is not in the window's closed glyph set (§6.7: a symbol is *drawn* as a
    /// `Path`, and `ui/bench.slint` draws this one) — so a `Text` asked for it falls to `.notdef`,
    /// an empty square, and nothing in `.slint` can ask whether a glyph exists. It went unnoticed
    /// because the window's sweep read `tools/ipod-gui/src` only, and this is a model sentence the
    /// window renders verbatim; that sweep now reads this crate's library too.
    /// `every_provenance_line_is_ascii_or_an_em_dash` is the local half of the same rule.
    ///
    /// **`Sha256` says *when* it was verified, and the tense is the point.** This is a record of
    /// how a file arrived, not a measurement of the bytes on disk now: an entry is keyed on its
    /// path, so replacing the file under it — a re-download with `curl -o`, a restore from a
    /// backup — leaves the record standing. `fetched — SHA-256 verified` read as a live claim
    /// about a file nobody had re-opened, which is the same shape as the `fetched and verified`
    /// string literal this field exists to delete, one level down. See [`Provenance::is_verified`]
    /// for how a surface re-establishes it.
    pub fn line(&self) -> String {
        match self {
            Provenance::Dumped => "dumped from a real iPod".to_string(),
            Provenance::Synthesised { seed } => format!("synthesised, seed {seed:x}"),
            Provenance::Fetched {
                verified: Verification::Sha256,
            } => "fetched — SHA-256 verified when it arrived".to_string(),
            Provenance::Fetched {
                verified: Verification::SizeOnly,
            } => "fetched — size only, no hash on record for this release yet".to_string(),
            Provenance::Fetched {
                verified: Verification::None,
            } => "fetched — nothing on record to check it against".to_string(),
            Provenance::Provided => "provided".to_string(),
            Provenance::Built => "built here".to_string(),
        }
    }

    /// **The only way to ask whether a verification badge is warranted.**
    ///
    /// There is deliberately no other predicate, so `SizeOnly` cannot be silently upgraded by a
    /// surface that felt like rounding up.
    ///
    /// **It answers about the filing, not about the file.** A [`Provenance`] is stored against a
    /// [`Resource`] — a path — and no digest, size or mtime is stored beside it, so this cannot
    /// tell you the bytes are still the bytes that matched. Nothing here is a substitute for
    /// looking: the path that re-establishes or refutes it is `firmware::cached(dir, true)`, which
    /// re-hashes, through `firmware::provenance`, filed with [`Settings::record_provenance`] —
    /// **never** [`Settings::file_away`], whose whole rule is that it does not overwrite a stated
    /// value. A surface that wants a live badge runs that pass; a surface that draws this alone is
    /// drawing a record, and [`Provenance::line`] words it as one.
    pub fn is_verified(&self) -> bool {
        matches!(
            self,
            Provenance::Fetched {
                verified: Verification::Sha256
            }
        )
    }
}

/// The one place a [`Provenance`] is reconciled with the [`Resource`] it describes.
///
/// A synthesised ROM's seed lives in the recipe already. Storing it a second time in the provenance
/// is two spellings of one fact, which is the drift this whole model change exists to delete — so
/// the relation is made total and mechanical instead: a `Firmware(Synthetic)` item's provenance is
/// **always** `Synthesised` with the recipe's own seed, whatever the caller or the file said, and
/// `Synthesised` on anything that is not one is meaningless and says nothing.
fn normalised(what: &Resource, from: Option<Provenance>) -> Option<Provenance> {
    match (what, from) {
        (Resource::Firmware(crate::nor::Source::Synthetic { seed, .. }), _) => {
            Some(Provenance::Synthesised { seed: *seed })
        }
        (_, Some(Provenance::Synthesised { .. })) => None,
        (_, other) => other,
    }
}

/// A named entry in the resources list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// What the person calls it, and the key a device refers to it by. Unique within the list.
    pub name: String,
    pub what: Resource,
    /// Where this came from. **`None` is "nobody recorded it"**, and it renders as nothing.
    ///
    /// An `Option` rather than a bare [`Provenance`] because none of the five variants is a
    /// says-nothing state, and every settings file written before this field has one for every
    /// entry it holds. When every write path states a provenance this becomes unreachable and the
    /// `Option` comes off.
    pub from: Option<Provenance>,
}

/// A drive image, and **what is on it**.
///
/// A disk used to be a `PathBuf`, which is why the list of them was a list of filenames: nothing
/// recorded that this one was built from `iPod_20.1.3.ipsw` and has Rockbox on it, so choosing
/// between two of them meant remembering. All of this is already detectable; it simply was not kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Disk {
    /// What the person calls it, and the key a device refers to it by.
    pub name: String,
    pub path: PathBuf,
    /// The `.ipsw` resource this was built from, by name. `None` for an image that was imported.
    pub built_from: Option<String>,
    /// What has been installed onto it, in the order it went on — so a triple-boot drive reads as
    /// one, rather than as a filename somebody has to remember the history of.
    pub installed: Vec<String>,
}

/// A part a device refers to that is not there, and **which of the two kinds of gone it is**.
///
/// Two named cases rather than one boolean, because the sentences differ and only one of them can
/// name a path — which, when there is one, is the whole of the answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Absent {
    /// The device names a part the lists no longer hold.
    Unlisted(String),
    /// The lists hold it and the file it points at is gone.
    Gone(PathBuf),
}

impl Absent {
    /// The shortest thing that names it: the entry's name, or the file's name — **not its path**.
    ///
    /// The cradle's one-part row is 24 px of centred body text (`cannot start — my-5.5g.img is not
    /// where it was`), and a full path does not fit in it. Whoever wants the path asks for it.
    pub fn label(&self) -> String {
        match self {
            Absent::Unlisted(s) => s.clone(),
            Absent::Gone(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                // A path ending in `/` or `..` has no file name, and an empty label names nothing.
                .unwrap_or_else(|| p.display().to_string()),
        }
    }

    /// The full path, when there is one. `Copy path` and the device's drawer page want it.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Absent::Unlisted(_) => None,
            Absent::Gone(p) => Some(p),
        }
    }
}

/// Whether paths are on disk, remembered for the length of one pass.
///
/// **The caller rule, and it is what the memoization buys:** [`Settings::missing`] performs a
/// `stat(2)` per resolved path, and a path under a stale network mount blocks until the mount times
/// out — tens of seconds on SMB, indefinitely on a hard NFS mount. So it must not be called from a
/// UI binding or a callback body. One `Presence` is made at the top of the pass that rebuilds the
/// rows, shared across every device in it, and dropped at the end of it. Nothing invalidates the
/// cache because the cache does not outlive the pass.
#[derive(Clone, Debug, Default)]
pub struct Presence {
    seen: std::collections::HashMap<PathBuf, bool>,
}

impl Presence {
    pub fn new() -> Presence {
        Presence::default()
    }

    /// `false` **only** when the path was looked for and was not there.
    ///
    /// [`std::fs::metadata`] by hand rather than either of the two shorter spellings, and the third
    /// match arm is the whole reason:
    ///
    /// - `Path::exists()` is `metadata().is_ok()`, which folds **every** error into `false`. A
    ///   parent directory the user cannot traverse would be reported as "the disk is not where it
    ///   was" — the program asserting a fact about somebody's filesystem it did not observe.
    /// - `symlink_metadata()` does not follow links, so a symlink pointing at a deleted image would
    ///   read as present. `metadata()` follows it, gets `NotFound`, and reports the truth the device
    ///   cares about.
    ///
    /// **The `false` arm names the errors that are statements of absence**, and there is more than
    /// one of them. `NotFound` is the obvious one. `NotADirectory` is a path whose parent component
    /// is a regular file. `InvalidInput` and `InvalidFilename` are paths the OS will not even
    /// accept — a NUL byte inside one on Unix, a reserved name on Windows; `metadata` produces
    /// neither from anything but the path itself. Nothing can exist at any of them, so calling them
    /// present would swallow a device that cannot start and leave the cradle saying nothing.
    /// Everything else — a permission, a timeout, a device error — stays in the `true` arm, which
    /// is what this function exists for.
    ///
    /// The path is keyed exactly as given. `canonicalize` would stat every component and multiply
    /// the cost this exists to bound.
    pub fn exists(&mut self, p: &Path) -> bool {
        if let Some(known) = self.seen.get(p) {
            return *known;
        }
        use std::io::ErrorKind;
        let there = match std::fs::metadata(p) {
            Ok(_) => true,
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::NotFound
                        | ErrorKind::NotADirectory
                        | ErrorKind::InvalidInput
                        | ErrorKind::InvalidFilename
                ) =>
            {
                false
            }
            // Something else went wrong — a permission, a timeout, a device error. That is not an
            // observation of absence, and reporting it as one is the lie the third arm exists for.
            Err(_) => true,
        };
        self.seen.insert(p.to_path_buf(), there);
        there
    }

    /// Forget one answer, for a caller that changed the model mid-pass.
    pub fn forget(&mut self, p: &Path) {
        self.seen.remove(p);
    }

    /// Forget every answer.
    pub fn clear(&mut self) {
        self.seen.clear();
    }
}

/// **A device: a firmware and a disk, under a name.** The only thing that can be run.
///
/// This replaced `Machine`, which was the same idea with the seams showing: the machine you were
/// *running* lived in [`Settings`]' own fields and the saved list was something you switched
/// between, so every operation had to reconcile the two and "the live one" was a special case in
/// each. A device is just a device; the one that is running is the one `current` names.
///
/// **It refers to its parts by name** — [`Device::firmware`] into the resources, [`Device::disk`]
/// into the disks — so editing a resource changes every device made of it. `disk_path` below is the
/// *resolved* value, kept because everything downstream of here reads it and none of it should have
/// to know a disk list exists.
///
/// There is deliberately no resolved copy of the boot ROM beside the name. There used to be, with
/// a migration case in its own doc comment, and two spellings of one fact is how the two came to
/// disagree: a device whose dump had moved silently booted a **generated** 5.5G instead, because
/// the resolution fell back to the inline copy. [`Settings::nor_of`] is the one resolution point
/// now, and it has no fallback.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Device {
    /// What the person calls it. The key, so it is unique and renaming is a delete plus an add.
    pub name: String,
    /// The iPod this device boots, by name into [`Settings::resources`]. Resolved through
    /// [`Settings::nor_of`]; there is no second, inline copy.
    ///
    /// A `String` rather than an `Option<String>` because with the inline copy gone, `None` and
    /// `""` would both mean "this device names no iPod" — one bad state, so it is encoded once.
    /// **Every device in [`Settings::devices`] names a resource**: [`Settings::parse`] guarantees
    /// it for anything read from a file and [`Settings::remember_as`] for anything this program
    /// makes. `Device::default()`'s empty one is a scratch value, never a list member.
    pub firmware: String,
    /// The disk this device runs, by name. `None` means [`Device::disk_path`] answers directly.
    pub disk: Option<String>,
    /// The resolved drive image.
    pub disk_path: Option<PathBuf>,
    pub chassis: Option<crate::identity::Colour>,
    pub work_on_copy: Option<bool>,
    /// Instructions the last **completed** cold boot of this device took.
    ///
    /// The progress bar's denominator, and the reason it can be honest across operating systems.
    /// It used to be `snap_at` — a constant tuned to RetailOS's 1.6 G — which made the bar
    /// meaningless for anything else: Rockbox reaches its menu in about 100 M and barely moved it,
    /// iPodLinux takes 21.5 G and pinned it at 100 % for twenty billion instructions. A device's
    /// own last boot is a better predictor of its next one than any constant, and it needs no
    /// detection of which operating system is on the drive.
    ///
    /// `None` until it has booted once, and the bar says "booting" without a fraction rather than
    /// inventing one.
    pub boot_instructions: Option<u64>,
    /// **What [`Device::boot_instructions`] was measured on** — `crate::compose::BootShape::render`,
    /// e.g. `rockbox, apple, rockbox`.
    ///
    /// The denominator above is honest only while the device goes on booting the same thing.
    /// Install Rockbox onto a device that learned ~1.6 G on RetailOS and it reaches its menu at
    /// ~100 M, so the bar reads 6 % at the moment the machine is finished; go the other way and it
    /// passes 100 % and keeps going. So the number is stored **with the shape that produced it**,
    /// and [`Settings::set_boot_shape`] drops the number exactly when the shape moves.
    ///
    /// A `String` rather than a `BootShape`, and that is the field's whole design: it keeps
    /// [`Device`] `PartialEq + Default` with no `impl` borrowed from `compose`, and the field and
    /// the settings-file line are **one spelling** — `render()` on the way out, `BootShape::parse`
    /// on the way in, and nothing in between to disagree.
    ///
    /// `None` means nobody recorded one, which is what every device written before this field has.
    /// A `None` shape beside a `Some` number is a denominator that cannot be checked, so
    /// [`Settings::set_boot_shape`] treats it as a mismatch and drops the number once.
    pub boot_shape: Option<String>,
    /// Seconds since the Unix epoch at which a complete restore point was last written for this
    /// device. `None` means no restore point has ever been written by this program.
    ///
    /// **It answers *when*, never *whether*.** Whether there is a restore point to resume is a
    /// question about two files on disk and the drive they pair with; this is the time to put
    /// beside the answer. So it is not cleared by a cold boot, by a power-off, or by a pair that
    /// has broken — a device whose drive was touched by something else keeps its park time and is
    /// offered `Discard the snapshot`, which is what makes that offer explicable.
    pub parked_at: Option<u64>,
    /// Whether this device was described in the Composer rather than made by the first run.
    ///
    /// **It exists because nothing else could tell the two apart.** The first-run device is
    /// identified by its boot ROM — a synthesised recipe with a seed somebody's press produced —
    /// and the Composer's `make one` mints exactly that shape, so a composed device answered *yes*
    /// to the only question the window was asking. What follows from that answer is the fixed
    /// first-run plan, which reads no recipe at all: a device composed as Rockbox-only was routed
    /// into a build of Apple's firmware onto an 8 GiB drive.
    ///
    /// **A fact about where the device came from, so it does not expire.** It is not *unbuilt* —
    /// [`Device::names_a_disk`] answers that, and it changes — and it is not the recipe, which is
    /// deliberately not stored under a device (see [`Settings::render_devices`]). A device the
    /// Composer filed goes on being one after its drive exists, because what may be done to it is
    /// still the recipe's and not the first run's.
    ///
    /// `false` for everything written before this field, which is the truth about those devices:
    /// there was no route to an existing device through the Composer at all.
    pub composed: bool,
}

impl Device {
    /// Whether this device names a drive at all.
    ///
    /// **`false` is *unfinished*, not *broken*.** `Settings::disk_of` already draws that line —
    /// `missing()` returns nothing for such a device and `run_device` accepts it — and a first run
    /// that failed at the fetch leaves exactly this shape. The cradle must then say *press the
    /// centre button to finish making My 5.5G* rather than promise a start.
    ///
    /// A name that resolves to nothing is `true` here: the device *names* a disk, and whether the
    /// name resolves is [`Settings::missing`]'s question, answered as `Absent::Unlisted`.
    pub fn names_a_disk(&self) -> bool {
        self.disk.is_some() || self.disk_path.is_some()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// The devices, in the order they were made.
    pub devices: Vec<Device>,
    /// The name of the device that is running, if the live fields came from one.
    pub current: Option<String>,
    /// Firmware, installers and software — everything that is **not** a disk. See [`Resource`].
    ///
    /// It exists because a file is worth keeping when it is not the one running. Every path this
    /// program learned used to be attached to the single live machine, so choosing a second boot
    /// ROM lost the first and there was no way to say "I have these four".
    pub resources: Vec<Item>,
    /// The drive images, and what is on each — see [`Disk`].
    ///
    /// **Separate from [`Settings::resources`] because a disk is not an ingredient.** It is the
    /// thing ingredients are combined *into*: an installer makes one, software goes onto one, and a
    /// device runs one. Filing it beside the `.ipsw` that built it is what made the old list
    /// unreadable.
    pub disks: Vec<Disk>,
    /// Whether [`Settings::seed_resources`] has run.
    ///
    /// **A marker, so that removing an entry sticks.** Seeding whenever the list is empty would be
    /// simpler and would be wrong: empty is also what you get after removing the last entry, and a
    /// list that puts back what you took out is a list you cannot edit.
    pub library_seeded: bool,
    /// Where the boot ROM comes from — a dump, or a recipe for synthesising one.
    ///
    /// **Stored as a recipe rather than as a megabyte.** A synthesised ROM is a pure function of a
    /// model, a seed and any typed-in identity, so persisting those regenerates it exactly and
    /// leaves nothing on disk to go stale, to clean up, or to be mistaken for a real dump.
    ///
    /// The file case is still a path, and [`Settings::flash`] returns it — which is what the
    /// recipes want, because they pass a path to a subprocess.
    pub nor: crate::nor::Source,
    pub disk: Option<PathBuf>,
    /// Which iPod this is, cosmetically. Not an instrument — it is which iPod you had, so it lives
    /// in user mode and is remembered like the rest of the setup.
    ///
    /// **`None` means "whatever the ROM says", and that is the default.** `SysCfg`'s `Mod#`
    /// resolves through [`crate::identity::Model`] to a colour, so a dump states which iPod it came
    /// off and the window shows that one without being told.
    ///
    /// `Some` is a person overruling it, and is obeyed. They know what was in their hand — a case
    /// swapped at some point in twenty years, a model table that disagrees, or simply a preference
    /// — and this is cosmetic. It is the *window's* iPod, not the machine's identity: nothing the
    /// firmware reads changes with it.
    pub chassis: Option<crate::identity::Colour>,
    /// **Off unless asked for.** An emulator that phones home on launch is a bad first impression
    /// for no benefit, and this audience notices. The menu item works either way — this only
    /// decides whether the check happens on its own.
    pub check_updates_on_start: bool,
    /// Whether the first-run screen has ever been drawn for this installation.
    ///
    /// **The old window inferred *offer me* from *the device list is empty*, and a cancelled or
    /// failed build empties the list** — so it re-opened its wizard for ever, returning a person to
    /// step one with no error shown and no way past. That shipped. Emptiness is a state of the
    /// library; this is a fact about the person, and the two are not the same question.
    ///
    /// It sits in the **main** block of [`Settings::render`] rather than in `render_resources`,
    /// which returns early on `resources.is_empty() && !library_seeded` — every first launch — so a
    /// key written there would not survive the one file that matters.
    ///
    /// Written once, by the window. **Nothing in this program ever clears it**: not a cancel, not a
    /// failure, not `forget`, not an empty library. A person who wants the welcome back sets it to
    /// false by hand, which is what the comment above the key says.
    pub welcomed: bool,
    /// Run on a copy of the drive image rather than on the image itself.
    ///
    /// **`None` means "nobody has said", and that is not the same as "no".** With no answer the
    /// choice is made from where the drive came from: one this program built from a bundle is
    /// regenerable byte for byte, so writing to it costs nothing; one the user supplied might be
    /// the only image of an iPod they own, and defaulting to writing on it is how an afternoon
    /// disappears. `Some` is a person's explicit choice and is obeyed for either kind.
    pub work_on_copy: Option<bool>,
}

impl Settings {
    /// Read the settings file, tolerating everything: a missing file, a missing directory, a
    /// truncated write, a key from a future version. A settings file is not a place to fail.
    ///
    /// **A read, and only a read.** It creates no file and writes none. That is not timidity: this
    /// is called from five places in `ipod-boot` that only want to know the default drive, and
    /// `ipod-boot <recipe> --print` is documented as *showing the command line and running
    /// nothing*. A save from here made `--print` rewrite the operator's settings — and
    /// [`Settings::render`] is generated from the model, so every comment they had added went with
    /// it. Whoever means to persist what seeding produced says so: [`Settings::load_and_seed`].
    pub fn load() -> Settings {
        let (mut s, _) = Settings::read();
        s.seed_resources();
        s
    }

    /// The file, and whether there was one. Split out so [`Settings::load_and_seed`] can tell a
    /// file it read from a file it would be creating.
    fn read() -> (Settings, bool) {
        let text = std::fs::read_to_string(data_dir().join(FILE)).ok();
        match &text {
            Some(text) => (Settings::parse(text), true),
            None => (Settings::default(), false),
        }
    }

    /// The same read, **and what seeding produced is written back**.
    ///
    /// Seeding mutates, and for as long as nothing wrote the answer down it was re-derived on every
    /// launch: the marker that makes removing an entry stick existed only in memory, so the list a
    /// person had edited came back the next time the program opened. Once per installation — the
    /// next launch reads the marker and seeding returns before it changes anything.
    ///
    /// **Only for a caller that owns the library**, which today is the window and nothing else. It
    /// rewrites the file from the model, so anything the format cannot hold — an operator's own
    /// comments, a key from a version that is not this one — does not survive it.
    ///
    /// **Only a file that already exists is written.** Reading is not a reason to create one:
    /// `migrate_legacy` carries a previous installation's settings forward and declines the moment
    /// a file exists here, so a load that minted one on a fresh machine would permanently block the
    /// carry-forward.
    pub fn load_and_seed() -> Settings {
        let (mut s, existed) = Settings::read();
        if s.seed_resources() && existed {
            let _ = s.save();
        }
        s
    }

    /// Fill empty resource and disk lists from what this program already has: the live boot ROM
    /// and drive, and every device's.
    ///
    /// **Once.** A setup that predates the library would otherwise open the page and be told it has
    /// nothing, while running a boot ROM and a drive — which is not a list of what you have, it is
    /// a list of what you have added since Tuesday. Seeding makes the page true on the first look.
    ///
    /// Guarded by [`Settings::library_seeded`] rather than by emptiness, so that removing every
    /// entry is a thing you can do rather than a thing that undoes itself at the next launch.
    ///
    /// **Returns whether it changed anything**, so [`Settings::load`] can write the answer back.
    /// Seeding that is never persisted is seeding that runs again next launch, and a marker that is
    /// never written is a removal that does not stick.
    pub fn seed_resources(&mut self) -> bool {
        if self.library_seeded {
            return false;
        }
        self.library_seeded = true;
        let stem = |p: &Path| {
            p.file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        // The live machine's ROM. Devices no longer carry one to gather — they name a resource, and
        // `Settings::parse` has already filed whatever an older file had inline.
        //
        // Bound to locals first: `file_away` takes `&mut self` and would otherwise be handed a
        // borrow of the very field it is filing.
        let live = self.nor.clone();
        let name = suggest_nor_name(&live);
        // `None`, deliberately: a dump seeded out of a pre-library `flash =` line is one we never
        // observed being made, and `Dumped` for it would be exactly the assertion this field exists
        // to delete. A synthetic needs nothing — `normalised` derives it from the recipe.
        self.file_away(Resource::Firmware(live), &name, None);

        let drives: Vec<(PathBuf, String)> = self
            .disk
            .iter()
            .map(|d| (d.clone(), stem(d)))
            .chain(
                self.devices
                    .iter()
                    .filter_map(|d| d.disk_path.as_ref().map(|p| (p.clone(), stem(p)))),
            )
            .collect();
        for (path, name) in drives {
            self.file_disk(path, &name);
        }
        // A device migrated from the old shape carries a path and no name. Now that every path is
        // a named disk, point it at the name — otherwise the Devices page shows "(no disk)" beside
        // a device that plainly has one.
        for i in 0..self.devices.len() {
            if self.devices[i].disk.is_none() {
                if let Some(p) = self.devices[i].disk_path.clone() {
                    self.devices[i].disk = self
                        .disks
                        .iter()
                        .find(|d| d.path == p)
                        .map(|d| d.name.clone());
                }
            }
        }
        true
    }

    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        // Resource entries an older file filed as `kind = disk`. Moved into `disks` once the whole
        // file is read, because an entry's `path` line arrives after its `kind` line and moving it
        // early would move an empty one.
        let mut was_a_disk: Vec<usize> = Vec::new();
        // The recipe each device carried in `device.N.flash` / `device.N.nor_*`, which is what a
        // settings file written before a device named its iPod looks like. Kept beside the devices
        // rather than on them, because a device has nowhere to put one any more; `adopt_inline_roms`
        // files each as a resource once the whole file has been read.
        let mut inline_rom: Vec<Option<crate::nor::Source>> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim(), v.trim());
            match k {
                // Empty is "not set", which is what an editor that blanked a line means.
                // The key an older settings file has. Still read, so an existing setup keeps its
                // dump instead of silently switching to a generated one.
                "flash" if !v.is_empty() => s.nor = crate::nor::Source::File(PathBuf::from(v)),
                "nor_model" if !v.is_empty() => s.nor = with_model(s.nor.clone(), v),
                "nor_seed" => {
                    if let Ok(n) = v.parse::<u64>() {
                        s.nor = with_seed(s.nor.clone(), n);
                    }
                }
                "nor_serial" => s.nor = with_serial(s.nor.clone(), v),
                "nor_splash" => s.nor = with_splash(s.nor.clone(), v),
                "nor_guid" => {
                    if let Ok(g) = u64::from_str_radix(v.trim_start_matches("0x"), 16) {
                        s.nor = with_guid(s.nor.clone(), g);
                    }
                }
                "disk" if !v.is_empty() => s.disk = Some(PathBuf::from(v)),
                // `auto` — and anything unrecognised — leaves it `None`, which is "ask the ROM".
                "chassis" => s.chassis = crate::identity::Colour::parse(v),
                // The key this replaced. Honoured so that anyone who had already chosen black does
                // not silently get a white iPod back on the next launch.
                "black_device" if v == "true" => s.chassis = Some(crate::identity::Colour::Black),
                "check_updates_on_start" => s.check_updates_on_start = v == "true",
                // Anything but the literal `true` is false, matching `check_updates_on_start` — a
                // half-written file must not suppress the one screen that explains the program.
                "welcomed" => s.welcomed = v == "true",
                "work_on_copy" => s.work_on_copy = Some(v == "true"),
                "current" if !v.is_empty() => s.current = Some(v.to_string()),
                "library_seeded" => s.library_seeded = v == "true",
                // `device.N.field`, `disk.N.field`, `res.N.field`. Flat, because the file is flat
                // and a nested format would mean a parser that can fail — and a settings file is
                // not a place to fail. Indices are dense on write and tolerated sparse on read.
                //
                // **`machine.` and `item.` are still read**, into devices and resources. They are
                // what a settings file written before this shape looks like, and the alternative to
                // reading them is a person's setup silently emptying itself.
                _ if k.starts_with("device.") || k.starts_with("machine.") => {
                    let Some((i, field)) = indexed(k) else {
                        continue;
                    };
                    while s.devices.len() <= i {
                        s.devices.push(Device::default());
                    }
                    while inline_rom.len() < s.devices.len() {
                        inline_rom.push(None);
                    }
                    // `s.devices[i]` is indexed per arm rather than bound once, because the arms
                    // that write `inline_rom[i]` must not be holding a borrow of `s.devices`.
                    match field {
                        "name" => s.devices[i].name = v.to_string(),
                        "flash" if !v.is_empty() => {
                            inline_rom[i] = Some(crate::nor::Source::File(PathBuf::from(v)))
                        }
                        // `disk` was a path in the old shape and is the disk's *name* in this one.
                        // Told apart by what it looks like: a path has a separator in it.
                        "disk" if !v.is_empty() => {
                            if v.contains('/') || v.contains('\\') {
                                s.devices[i].disk_path = Some(PathBuf::from(v));
                            } else {
                                s.devices[i].disk = Some(v.to_string());
                            }
                        }
                        // An empty value leaves it empty, which the post-pass then fills.
                        "firmware" => s.devices[i].firmware = v.to_string(),
                        // The old shape's names for the two references.
                        "rom" if !v.is_empty() => s.devices[i].firmware = v.to_string(),
                        "drive" if !v.is_empty() => s.devices[i].disk = Some(v.to_string()),
                        "chassis" => s.devices[i].chassis = crate::identity::Colour::parse(v),
                        "work_on_copy" => s.devices[i].work_on_copy = Some(v == "true"),
                        "boot_instructions" => {
                            s.devices[i].boot_instructions = v.parse::<u64>().ok()
                        }
                        // **Read as text and not through `BootShape::parse`.** A hand-edited or
                        // future token this build does not know must survive the round trip rather
                        // than being silently blanked — and a shape that does not parse compares
                        // unequal to every shape that does, which is exactly the answer
                        // `set_boot_shape` wants from it: drop the denominator, keep going.
                        "boot_shape" if !v.is_empty() => {
                            s.devices[i].boot_shape = Some(v.to_string())
                        }
                        "parked_at" => s.devices[i].parked_at = v.parse::<u64>().ok(),
                        // Written only when true, so anything but the word is `false` — which is
                        // what a hand-edited file that says nothing means as well.
                        "composed" => s.devices[i].composed = v == "true",
                        _ => {
                            // `unwrap_or_default` reproduces the old start point exactly: the
                            // resolved ROM defaulted to a synthetic A446 seed 0, so a lone
                            // `nor_seed = 5` still yields `Synthetic { model: "A446", seed: 5 }`.
                            let base = inline_rom[i].clone().unwrap_or_default();
                            if let Some(next) = nor_field(base, field, v) {
                                inline_rom[i] = Some(next);
                            }
                        }
                    }
                }
                _ if k.starts_with("disk.") => {
                    let Some((i, field)) = indexed(k) else {
                        continue;
                    };
                    while s.disks.len() <= i {
                        s.disks.push(Disk::default());
                    }
                    let d = &mut s.disks[i];
                    match field {
                        "name" => d.name = v.to_string(),
                        "path" if !v.is_empty() => d.path = PathBuf::from(v),
                        "built_from" if !v.is_empty() => d.built_from = Some(v.to_string()),
                        // One line, comma separated, because the order is the install order and a
                        // key per entry would make that order an accident of the file.
                        "installed" if !v.is_empty() => {
                            d.installed = v
                                .split(',')
                                .map(|x| x.trim().to_string())
                                .filter(|x| !x.is_empty())
                                .collect()
                        }
                        _ => {}
                    }
                }
                _ if k.starts_with("res.") || k.starts_with("item.") => {
                    let Some((i, field)) = indexed(k) else {
                        continue;
                    };
                    while s.resources.len() <= i {
                        s.resources.push(Item {
                            name: String::new(),
                            what: Resource::Firmware(crate::nor::Source::default()),
                            from: None,
                        });
                    }
                    let item = &mut s.resources[i];
                    match field {
                        "name" => item.name = v.to_string(),
                        // `kind` before `path` on write, so this has always seen it first. An
                        // unknown kind leaves the entry as it was rather than dropping it.
                        //
                        // `disk` is the old shape's third kind. A disk is not a resource any more,
                        // so it is parked here and moved by `migrate_disks_out_of_resources` —
                        // dropping it would lose the drive somebody is running.
                        "kind" => {
                            item.what = match v {
                                "installer" | "ipsw" => Resource::Installer(PathBuf::new()),
                                "bootloader" => Resource::Bootloader(PathBuf::new()),
                                "software" | "disk" => Resource::Software(PathBuf::new()),
                                _ => Resource::Firmware(crate::nor::Source::default()),
                            };
                            if v == "disk" {
                                was_a_disk.push(i);
                            }
                        }
                        "path" if !v.is_empty() => {
                            let p = PathBuf::from(v);
                            item.what = match &item.what {
                                Resource::Installer(_) => Resource::Installer(p),
                                Resource::Bootloader(_) => Resource::Bootloader(p),
                                Resource::Software(_) => Resource::Software(p),
                                Resource::Firmware(_) => {
                                    Resource::Firmware(crate::nor::Source::File(p))
                                }
                            }
                        }
                        // Written explicitly rather than left to fall through `nor_field`, which
                        // returns `None` for it and would make a missing arm silently harmless —
                        // which is exactly why it has to be here to be read.
                        "provenance" => item.from = Provenance::parse(v),
                        _ => {
                            if let Resource::Firmware(src) = &item.what {
                                if let Some(next) = nor_field(src.clone(), field, v) {
                                    item.what = Resource::Firmware(next);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        // Order is load-bearing. `migrate_disks_out_of_resources` removes by the **file's** own
        // indices, so it has to run before anything else adds or removes a resource; swapping these
        // two lines silently removes the wrong one.
        s.migrate_disks_out_of_resources(&was_a_disk);
        // **A device with no name is not a device.** Indices are tolerated sparse on read, and the
        // gaps are filled with `Device::default()` so that `device.2.name` lands on index 2 — but a
        // hand-edit that deletes one `device.1.*` block leaves that placeholder behind, and
        // `adopt_inline_roms` would then mint a generated iPod for a machine nobody made. Dropped
        // here, before the migration, for the same reason `filed_under` refuses to hand out an
        // unnamed resource: nothing can name it, so nothing can run it or remove it.
        //
        // `inline_rom` is indexed by the **file's** device index, so it is filtered in the same
        // pass — `Vec::retain` visits every element once, in order, which is what makes this safe.
        let mut kept: Vec<Option<crate::nor::Source>> = Vec::with_capacity(s.devices.len());
        let mut at = 0usize;
        s.devices.retain(|d| {
            let keep = !d.name.is_empty();
            if keep {
                kept.push(inline_rom.get(at).cloned().flatten());
            }
            at += 1;
            keep
        });
        s.adopt_inline_roms(&kept);
        s.normalise_provenance();
        s
    }

    /// Give every device that came out of the file a named iPod.
    ///
    /// `inline[i]` is the recipe device `i` carried in `device.N.flash` / `device.N.nor_*`. It is
    /// filed as a resource and the device is pointed at its name — the old keys are read for ever
    /// and written never.
    fn adopt_inline_roms(&mut self, inline: &[Option<crate::nor::Source>]) {
        for i in 0..self.devices.len() {
            if !self.devices[i].firmware.is_empty() {
                continue;
            }
            // A device that said nothing at all — `device.5.chassis = black` and no ROM keys —
            // silently booted a generated 5.5G. It still boots exactly that; the difference is that
            // it now says which one, in a list, where it can be changed.
            let src = inline.get(i).cloned().flatten().unwrap_or_default();
            let suggested = suggest_nor_name(&src);
            // `None`: a dump named by a `flash =` line is one this program never watched being
            // made. `file_away` dedupes by value, so however many devices lack a recipe, at most
            // one extra resource comes out of this.
            let name = self.file_away(Resource::Firmware(src), &suggested, None);
            self.devices[i].firmware = name;
        }
    }

    /// Reconcile every resource's provenance with the resource itself, once the whole file is read.
    ///
    /// Running it here rather than per line is what keeps `res.N.provenance` and `res.N.nor_seed`
    /// order-independent: the file gains no new order dependency from this key.
    fn normalise_provenance(&mut self) {
        for it in &mut self.resources {
            it.from = normalised(&it.what, it.from);
        }
    }

    /// Move entries an older settings file filed as resources of `kind = disk` into [`Self::disks`].
    ///
    /// **The old shape had three kinds in one list and a disk was one of them.** It is not: a disk
    /// is what resources are combined *into*. Leaving them there would show drives on the resources
    /// page under a verb that does not apply to them, and dropping them would lose the drive
    /// somebody is running — so they move, keeping their names, and a device that pointed at one by
    /// name still resolves.
    fn migrate_disks_out_of_resources(&mut self, was_a_disk: &[usize]) {
        for &i in was_a_disk.iter().rev() {
            if i >= self.resources.len() {
                continue;
            }
            let item = self.resources.remove(i);
            // The entry's provenance is dropped in the move, and that is right: a `Disk` has no
            // such field because a drive's origin is already `built_from`.
            if let Some(p) = item.what.path() {
                let path = p.to_path_buf();
                if !self.disks.iter().any(|d| d.path == path) {
                    self.disks.push(Disk {
                        name: item.name,
                        path,
                        built_from: None,
                        installed: Vec::new(),
                    });
                }
            }
        }
    }

    /// The path of a supplied dump, or `None` when the ROM is synthesised.
    ///
    /// The recipes hand a path to a subprocess, so they need this; a synthesised ROM has no path
    /// and they say so rather than inventing one.
    pub fn flash(&self) -> Option<PathBuf> {
        match &self.nor {
            crate::nor::Source::File(p) => Some(p.clone()),
            crate::nor::Source::Synthetic { .. } => None,
        }
    }

    /// The `nor` half of the settings file.
    fn render_nor(&self) -> String {
        render_nor_of(&self.nor)
    }
}

/// A string that can be written into this file and read back as itself.
///
/// **Names in this program come from filenames**, through `p.file_stem()`, and every filesystem
/// this runs on lets a filename hold a newline. Written straight out, one splits the record in
/// two and everything after it is read as a fresh key — `res.0.name = mine⏎res.0.kind = software`
/// files somebody's boot ROM as software. Replaced with a space rather than rejected, because a
/// name is a label and the label they meant is still in there.
///
/// **Paths are not put through this and must not be**: a mangled path is a path that names the
/// wrong file, which is worse than the record it would repair. A path holding a line break is a
/// limit of this file format, stated in [`Settings::render`], and closing it means quoting values
/// rather than trimming them.
fn one_line(s: &str) -> String {
    // `\r\n` first, so a Windows line ending becomes one space rather than two.
    s.replace("\r\n", " ").replace(['\r', '\n'], " ").trim().to_string()
}

/// The name to file a boot ROM under when nobody has given it one.
///
/// One place, because four callers need the same spelling: seeding, [`Settings::remember_as`], the
/// migration of a device that carried its recipe inline, and the Composer, which files an iPod on
/// `Create`. Four copies of a format string is four chances for one of them to mint a duplicate
/// entry under a slightly different name.
///
/// **Public for the Composer, and it is the fourth caller that made it so.** §11.2 says what the
/// name is — `<model>, seed <n>` — and this string is it. A window that formatted its own would be
/// a fifth spelling, and [`Settings::restate_firmware`] re-derives through this function when an
/// identity is tuned, so the name a filing produces and the name a restatement produces cannot be
/// two names.
///
/// **It does not file at the mint, and the doc used to say it did.** §11.2 asked for the iPod to
/// become a filed resource the moment it was made; nothing ever implemented that, and two doc
/// comments in this file asserted it as fact. `Composer::make_one` mints into the page and touches
/// no library; `Composer::commit` is the filing. GUI.md §11.2 is corrected to match, because the
/// program the design describes has to be the program.
pub fn suggest_nor_name(src: &crate::nor::Source) -> String {
    match src {
        crate::nor::Source::File(p) => p
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        crate::nor::Source::Synthetic { model, seed, .. } => format!("{model}, seed {seed}"),
    }
}

/// What to call the iPod a source describes, in the words a person would use: `Black 5.5G`.
///
/// **Not [`suggest_nor_name`]**, which is `A446, seed 12873491` — a recipe, and the right name for
/// a row in a list of recipes. This is what first run puts on the shelf, and `seed 12873491` is not
/// something anybody would say out loud.
///
/// An unknown model falls back to [`suggest_nor_name`] rather than to a placeholder — the model
/// number and seed for a synthesised iPod, the file stem for a dump this build cannot read. A name
/// nobody recognises is still a name, and `unnamed` is not. A dump it CAN read is named by what it
/// is; see [`model_of`], which is where that used to go wrong.
pub fn suggest_ipod_name(src: &crate::nor::Source) -> String {
    match model_of(src) {
        Some(m) => format!("{} {}", m.colour().label(), m.generation.label()),
        None => suggest_nor_name(src),
    }
}

/// What to call the device made out of that iPod: `My 5.5G`.
pub fn suggest_device_name(src: &crate::nor::Source) -> String {
    match model_of(src) {
        Some(m) => format!("My {}", m.generation.label()),
        None => "My iPod".into(),
    }
}

/// What to call its drive image, as a filename stem: `my-5.5g`.
///
/// **It must produce a filename on every platform this program runs on**, so `\ / : * ? " < > |`,
/// control characters and newlines cannot survive it, and it is never empty — an empty stem
/// produces `.img`, a hidden file nobody can find.
pub fn suggest_disk_stem(src: &crate::nor::Source) -> String {
    file_stem_of(&suggest_device_name(src))
}

/// Any name at all, as a filename stem.
///
/// **A separate function because it is the part with a hostile input.** `suggest_disk_stem` can
/// only ever hand it `My 5.5G`-shaped text, so a test that went through that door could never see
/// a `/` and would pass whatever this did. A person renaming a device can type anything.
///
/// Everything outside `[a-z0-9.]` becomes one `-`, runs collapse, and leading or trailing `-` and
/// `.` go — so `\ / : * ? " < > |`, control characters and newlines cannot survive, and the result
/// is never a hidden file. Never empty: an empty stem produces `.img`, which nobody can find.
pub fn file_stem_of(name: &str) -> String {
    let name = name.to_lowercase();
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches(['-', '.']).to_string();
    if trimmed.is_empty() {
        "ipod".into()
    } else {
        trimmed
    }
}

/// The model a source describes, when this build knows it — **[`crate::nor::Source::model`]'s
/// answer, asked rather than re-derived.**
///
/// It used to answer `None` for every [`crate::nor::Source::File`], which made this a *second*
/// answer to a question `nor.rs` already had one for, and the two disagreed on the surface that
/// draws both. §11.2's level ① reads the model at the `Model` row and the name at the `iPod` row
/// two above it, so one readable dump came out as `5G, 30 GB` on one line and `foreign-oui` — its
/// **file stem**, by way of [`suggest_nor_name`]'s fallback — on the other. One iPod, two names,
/// and the devices page's `Made of` line had the same pair for the same reason.
///
/// A dump this program cannot read still falls back: `Source::model` reads the file and parses its
/// SysCfg, and a path that is not there, not a NOR image, or carries no model block answers `None`
/// exactly as before — which is why `a_disk_stem_is_a_filename_on_every_platform`'s
/// `/roms/My Dump (2).bin` is still `my-ipod`.
fn model_of(src: &crate::nor::Source) -> Option<&'static crate::identity::Model> {
    src.model()
}

/// Where drives this program builds land.
///
/// Under [`data_dir`], so `IPOD_EMULATOR_DATA` moves them — which is what makes it safe for a test
/// or an agent to run a build without landing an 8 GiB file in somebody's real library.
pub fn drives_dir() -> PathBuf {
    data_dir().join("drives")
}

/// A path in `dir` named `<stem>.<ext>` that **nothing already occupies**, suffixing ` (2)`,
/// ` (3)` … the way [`Settings::unique_name`] does for names in a list.
///
/// `fs::rename` overwrites silently, so without this a first run could destroy a drive the operator
/// already had and named the same thing. `AGENTS.md` §3: never overwrite a disk image.
pub fn free_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let stem = one_line(stem);
    let stem = if stem.is_empty() { "ipod" } else { stem.as_str() };
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    // Bounded rather than unbounded: a directory holding four billion `my-5.5g (n).img` is a
    // different problem, and a loop with no end is how a UI thread stops answering.
    for n in 2..10_000u32 {
        let p = dir.join(format!("{stem} ({n}).{ext}"));
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!("{stem} ({}).{ext}", now_unix()))
}

/// One [`crate::nor::Source`] as settings lines. Shared by the live machine and every saved one, so
/// the two can never drift into different spellings of the same recipe.
fn render_nor_of(nor: &crate::nor::Source) -> String {
    {
        match nor {
            crate::nor::Source::File(p) => format!("flash = {}\n", p.display()),
            crate::nor::Source::Synthetic {
                model,
                seed,
                serial,
                guid,
                splash,
            } => {
                let mut out = format!("nor_model = {model}\nnor_seed = {seed}\n");
                if let Some(p) = splash {
                    out.push_str(&format!("nor_splash = {}\n", p.display()));
                }
                if let Some(s) = serial {
                    out.push_str(&format!("nor_serial = {s}\n"));
                }
                if let Some(g) = guid {
                    out.push_str(&format!("nor_guid = {g:016X}\n"));
                }
                out
            }
        }
    }
}

impl Settings {
    /// The whole settings file, as text.
    ///
    /// **One known limit, stated rather than papered over**: a `key = value` line format cannot
    /// hold a value with a line break in it. Names go through [`one_line`] before they are filed,
    /// so the only way to reach it is a *path* — a drive image or a dump whose own filename holds
    /// a newline. Such a record is written and read back short. Mangling the path would name the
    /// wrong file and dropping it would lose a drive somebody is running, so the fix is quoting,
    /// which is a change to the format and not to this function.
    pub fn render(&self) -> String {
        let p = |o: &Option<PathBuf>| {
            o.as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        format!(
            "# ipod-gui settings. Hand-editable; keys this version does not know are ignored.\n\
             # When the program saves, it writes this file out from its own model — so anything\n\
             # not in the list below, including comments you add, is not carried over.\n\
             # auto, white, black, or u2. `auto` reads it out of the NOR's Mod#, which is\n\
             # what the dump says the iPod was; the rest overrule that. Cosmetic either way —\n\
             # the firmware is handed the same identity whatever this says.\n\
             chassis = {}\n\
{}\
             disk = {}\n\
             # An HTTPS GET of the GitHub releases API and a version comparison, on launch.\n\
             # Off by default on purpose. The menu item works whatever this says.\n\
             check_updates_on_start = {}\n\
             # Whether the first-run screen has been shown. Once true it never goes back: a\n\
             # cancelled or failed build empties the device list, and a program that read\n\
             # emptiness as \"offer the welcome again\" returns you to step one for ever. Set\n\
             # it to false to see it again.\n\
             welcomed = {}\n\
             # Run on a COPY of the drive, leaving the original untouched. Absent means \"decide\n\
             # from where the drive came from\": a drive this program built is written to directly,\n\
             # one you supplied is copied. Set it to true or false to answer for both.\n\
             {}",
            self.chassis.map(|c| c.as_str()).unwrap_or("auto"),
            self.render_nor(),
            p(&self.disk),
            self.check_updates_on_start,
            self.welcomed,
            match self.work_on_copy {
                Some(v) => format!("work_on_copy = {v}\n"),
                None => String::new(),
            },
        ) + &self.render_resources()
            + &self.render_disks()
            + &self.render_devices()
    }

    /// The resources, as `res.N.field` lines.
    ///
    /// `kind` is written **before** `path`, and the parser depends on that order: it uses the kind
    /// to decide which variant a path belongs to. Reordering these two by hand would file an
    /// `.ipsw` as a boot ROM, so the order is load-bearing rather than tidy.
    ///
    /// `provenance` is one self-contained token and carries **no** order dependency. It is omitted
    /// entirely for a synthesised ROM, whose provenance is derived from the recipe already in the
    /// file, and omitted when nobody recorded one — because "nobody recorded it" is the absence of
    /// a line, not a line saying so.
    fn render_resources(&self) -> String {
        if self.resources.is_empty() && !self.library_seeded {
            return String::new();
        }
        let mut out = String::from(
            "\n# Resources: firmware a device can boot, installers that make a disk, and\n\
             # software that installs onto one. A device names firmware from here.\n\
             # `provenance` says where a file came from: dumped, provided, built,\n\
             # fetched, fetched-size or fetched-sha256. A generated iPod has none —\n\
             # its provenance is the recipe below it, so writing one here does nothing.\n",
        );
        out.push_str(&format!("library_seeded = {}\n", self.library_seeded));
        for (i, item) in self.resources.iter().enumerate() {
            out.push_str(&format!("\nres.{i}.name = {}\n", item.name));
            out.push_str(&format!("res.{i}.kind = {}\n", item.what.kind()));
            if !matches!(
                &item.what,
                Resource::Firmware(crate::nor::Source::Synthetic { .. })
            ) {
                if let Some(f) = item.from {
                    out.push_str(&format!("res.{i}.provenance = {}\n", f.token()));
                }
            }
            match &item.what {
                // A synthesised ROM is a recipe, written the way every other recipe in this file
                // is. `flash = …` becomes `path = …` because at this level the kinds share one key.
                Resource::Firmware(src) => {
                    for line in render_nor_of(src).lines() {
                        let line = line
                            .strip_prefix("flash = ")
                            .map_or_else(|| line.to_string(), |p| format!("path = {p}"));
                        out.push_str(&format!("res.{i}.{line}\n"));
                    }
                }
                Resource::Installer(p) | Resource::Software(p) | Resource::Bootloader(p) => {
                    out.push_str(&format!("res.{i}.path = {}\n", p.display()));
                }
            }
        }
        out
    }

    /// The disks, as `disk.N.field` lines.
    fn render_disks(&self) -> String {
        if self.disks.is_empty() {
            return String::new();
        }
        let mut out = String::from("\n# Drive images, and what is on each.\n");
        for (i, d) in self.disks.iter().enumerate() {
            out.push_str(&format!("\ndisk.{i}.name = {}\n", d.name));
            out.push_str(&format!("disk.{i}.path = {}\n", d.path.display()));
            if let Some(b) = &d.built_from {
                out.push_str(&format!("disk.{i}.built_from = {b}\n"));
            }
            if !d.installed.is_empty() {
                out.push_str(&format!(
                    "disk.{i}.installed = {}\n",
                    d.installed.join(", ")
                ));
            }
        }
        out
    }

    /// The devices, as `device.N.field` lines.
    fn render_devices(&self) -> String {
        if self.devices.is_empty() {
            return String::new();
        }
        let mut out =
            String::from("\n# Devices. `current` names the one the settings above came from.\n");
        if let Some(c) = &self.current {
            out.push_str(&format!("current = {c}\n"));
        }
        for (i, d) in self.devices.iter().enumerate() {
            out.push_str(&format!("\ndevice.{i}.name = {}\n", d.name));
            // **The name is the whole of it.** A recipe is never written under a device: writing
            // both a reference and a resolved copy is how the two came to disagree, and the copy is
            // what used to win. Written even when empty, so the block is uniform and hand-editable;
            // `parse`'s post-pass fills an empty one back in.
            out.push_str(&format!("device.{i}.firmware = {}\n", d.firmware));
            match (&d.disk, &d.disk_path) {
                (Some(name), _) => out.push_str(&format!("device.{i}.disk = {name}\n")),
                (None, Some(p)) => out.push_str(&format!("device.{i}.disk = {}\n", p.display())),
                (None, None) => {}
            }
            if let Some(c) = d.chassis {
                out.push_str(&format!("device.{i}.chassis = {}\n", c.as_str()));
            }
            if let Some(w) = d.work_on_copy {
                out.push_str(&format!("device.{i}.work_on_copy = {w}\n"));
            }
            if let Some(b) = d.boot_instructions {
                out.push_str(&format!("device.{i}.boot_instructions = {b}\n"));
            }
            // Written **after** the number it qualifies, so a person reading the file meets the
            // denominator and then what it was measured on. Order is not load-bearing on read —
            // unlike `res.N.kind` before `res.N.path` — because neither line decides how the other
            // is parsed.
            if let Some(b) = &d.boot_shape {
                out.push_str(&format!("device.{i}.boot_shape = {b}\n"));
            }
            if let Some(t) = d.parked_at {
                out.push_str(&format!("device.{i}.parked_at = {t}\n"));
            }
            // **Written only when true.** `device.N.composed = false` on every device this program
            // has ever made would be a line per device stating the default, and the file is meant
            // to be read and hand-edited.
            if d.composed {
                out.push_str(&format!("device.{i}.composed = true\n"));
            }
        }
        out
    }

    /// The live fields as a [`Device`] under `name`.
    ///
    /// The name goes through [`one_line`], for the same reason a resource's does — it ends up as
    /// `device.N.name` in a `key = value` file, and `current` names a device by it.
    pub fn as_device(&self, name: &str) -> Device {
        let name = one_line(name);
        let existing = self.devices.iter().find(|d| d.name == name);
        Device {
            name: name.clone(),
            // **The composition is kept, not re-derived.** Deriving it from the live values was
            // tried and is wrong: switching devices writes back the one you were editing, so the
            // moment a resource changed, the write-back looked for one matching the old value,
            // found none, and quietly cut the device loose from what it was made of.
            //
            // It can still come back empty — nothing existing, and nothing in the resources
            // matching the live ROM. `remember_as` is what closes that; this stays `&self` and
            // mints nothing.
            firmware: existing
                .map(|d| d.firmware.clone())
                .filter(|f| !f.is_empty())
                .or_else(|| {
                    // The same lookup `file_away` does, through the same function, so the name a
                    // save hands the device and the name filing hands back cannot be two names.
                    self.filed_under(&Resource::Firmware(self.nor.clone()))
                        .map(|i| self.resources[i].name.clone())
                })
                .unwrap_or_default(),
            disk: existing.and_then(|d| d.disk.clone()).or_else(|| {
                self.disk
                    .as_ref()
                    .and_then(|p| self.disks.iter().find(|d| d.path == *p))
                    .map(|d| d.name.clone())
            }),
            disk_path: self.disk.clone(),
            chassis: self.chassis,
            work_on_copy: self.work_on_copy,
            boot_instructions: existing.and_then(|d| d.boot_instructions),
            // **The named trap of §20 item 6, closed.** Without this line every `run_device` /
            // `remember_as` round trip loses the shape, `set_boot_shape` then sees `None` beside a
            // good number, reads it as a mismatch, and the next `Create` throws away a denominator
            // that was correct. The number and the shape it was measured on travel together or
            // neither of them is worth storing.
            boot_shape: existing.and_then(|d| d.boot_shape.clone()),
            // The park time belongs to the saved device and is not derivable from the live fields.
            parked_at: existing.and_then(|d| d.parked_at),
            // **Kept, for the same reason the boot shape is.** Where a device came from is not
            // derivable from the live fields, and `remember_as` runs on routes that are not the
            // Composer — a save from anywhere else would otherwise quietly re-file a composed
            // device as the first run's and hand it back to the fixed plan.
            composed: existing.is_some_and(|d| d.composed),
        }
    }

    /// Put something in the resources, or return the name it already has.
    ///
    /// **Adding the same file twice is not an error and does not make a second entry.** Identity is
    /// the value, not the name: two entries pointing at one path would be two names for one thing,
    /// and nothing could tell you which you were running. A name collision with a *different* thing
    /// gets a suffix rather than overwriting it.
    ///
    /// `from` is a required argument, including when it is `None` — because `None` is a statement,
    /// and a resource filed without one is a row that has to invent what to say about it. On the
    /// duplicate path there is one rule and one only: **`None` may become stated; a stated value is
    /// never changed.** So a ROM seeded before anyone knew where it came from stops saying nothing
    /// the first time a real acquisition files it, and a fetch followed by a Provide cannot flip a
    /// recorded fact. [`Settings::record_provenance`] is the deliberate second verb for the caller
    /// that has re-checked the bytes and is entitled to overwrite.
    ///
    /// An entry with **no name** is never handed out as one: `parse` fills sparse `res.N.` indices
    /// with unnamed placeholders, and without that guard the first file of a default synthetic ROM
    /// matched placeholder 0 and returned `""` — a reference nothing can resolve.
    pub fn file_away(&mut self, what: Resource, suggested: &str, from: Option<Provenance>) -> String {
        if let Some(i) = self.filed_under(&what) {
            let existing = &mut self.resources[i];
            if existing.from.is_none() && from.is_some() {
                existing.from = normalised(&existing.what, from);
            }
            return existing.name.clone();
        }
        let name = self.unique_name(suggested, |s| s.resources.iter().map(|i| i.name.as_str()));
        let from = normalised(&what, from);
        self.resources.push(Item {
            name: name.clone(),
            what,
            from,
        });
        name
    }

    /// Where this exact value is already filed, if it is. **One lookup, two callers**, so the name
    /// [`Settings::file_away`] hands back and the name [`Settings::as_device`] writes on a device
    /// cannot be two different names for one thing.
    ///
    /// **Never an unnamed entry.** [`Settings::parse`] fills sparse `res.N.` indices with unnamed
    /// placeholders holding a default synthetic ROM, and handing one of those back as a name is a
    /// reference nothing can resolve — which is what a device made on a hand-edited file used to
    /// get.
    fn filed_under(&self, what: &Resource) -> Option<usize> {
        self.resources
            .iter()
            .position(|it| it.what == *what && !it.name.is_empty())
    }

    /// State where a resource came from, overwriting whatever was there.
    ///
    /// The second verb, because "fill in what nobody said" and "I have just re-checked this" are
    /// different acts and one function that did both would decide silently which had happened.
    /// `false` if there is no resource of that name.
    ///
    /// **This is the only way a stale verification comes down.** A recorded
    /// [`Provenance::Fetched`] is a record of how the file arrived, and the entry is keyed on its
    /// path — so a file replaced underneath it keeps the badge until somebody re-reads the bytes
    /// and files the answer here. A re-fetch must come through this and not through
    /// [`Settings::file_away`], which by design leaves a stated value alone: routed the wrong way,
    /// a re-download that could only be size-checked would keep the SHA-256 badge from the first.
    pub fn record_provenance(&mut self, name: &str, from: Provenance) -> bool {
        match self.resources.iter_mut().find(|it| it.name == name) {
            Some(it) => {
                it.from = normalised(&it.what, Some(from));
                true
            }
            None => false,
        }
    }

    /// Put a drive image in the disks, or return the name the same path already has.
    pub fn file_disk(&mut self, path: PathBuf, suggested: &str) -> String {
        if let Some(existing) = self.disks.iter().find(|d| d.path == path) {
            return existing.name.clone();
        }
        let name = self.unique_name(suggested, |s| s.disks.iter().map(|d| d.name.as_str()));
        self.disks.push(Disk {
            name: name.clone(),
            path,
            built_from: None,
            installed: Vec::new(),
        });
        name
    }

    /// A name not already taken in the list `taken` names.
    ///
    /// Put through [`one_line`] first — every name in this program is derived from a filename, and
    /// a filename may hold a newline.
    fn unique_name<'a, F, I>(&'a self, suggested: &str, taken: F) -> String
    where
        F: Fn(&'a Settings) -> I,
        I: Iterator<Item = &'a str>,
    {
        let suggested = one_line(suggested);
        let base = if suggested.is_empty() {
            "unnamed"
        } else {
            suggested.as_str()
        };
        let mut name = base.to_string();
        let mut n = 2;
        while taken(self).any(|t| t == name) {
            name = format!("{base} ({n})");
            n += 1;
        }
        name
    }

    /// The boot ROM this device runs, resolved through the resources by name.
    ///
    /// `None` when the name is empty, names nothing, or names something that is not a
    /// [`Resource::Firmware`]. **There is deliberately no fallback**: substituting a generated 5.5G
    /// for a dump that has gone is the window telling a lie about which iPod is running, and it is
    /// precisely what the deleted inline copy used to do.
    ///
    /// Callers that need ownership write `.cloned()`.
    pub fn nor_of(&self, d: &Device) -> Option<&crate::nor::Source> {
        self.firmware_of(d).ok()
    }

    /// The boot ROM a device actually boots, or why it cannot be found.
    ///
    /// The wrong-kind case is a real class rather than a curiosity: resource names are unique within
    /// one list holding all four kinds, so an `.ipsw` called `my dump` shadows nothing and resolves
    /// to nothing. It is [`Absent::Unlisted`] for the same reason a name nobody holds is — the
    /// device names a part the lists do not hold, whatever else happens to wear that name.
    ///
    /// **A device that names nothing gets a label rather than the empty string.** `parse` fills
    /// every empty `firmware` and `remember_as` files before it saves, so this arrives only from a
    /// hand-built [`Device`] — but `missing` renders `missing {label}`, and `missing ` followed by
    /// nothing is the caption saying something is wrong and refusing to say what.
    fn firmware_of(&self, d: &Device) -> Result<&crate::nor::Source, Absent> {
        self.resources
            .iter()
            .find(|it| it.name == d.firmware && !it.name.is_empty())
            .and_then(|it| match &it.what {
                Resource::Firmware(src) => Some(src),
                _ => None,
            })
            .ok_or_else(|| {
                Absent::Unlisted(if d.firmware.is_empty() {
                    "an iPod".into()
                } else {
                    d.firmware.clone()
                })
            })
    }

    /// The drive image a device actually runs. `None` when it names no disk at all — which is an
    /// **unfinished** device, not a broken one, and belongs to first run rather than to a refusal.
    ///
    /// **Public because it is the only function that knows how a device becomes an image**, and
    /// every caller that answered the question for itself got it wrong. The window's §7.5 row 3 read
    /// `d.disk_path` directly and so said *no drive yet — nothing will be written* about every saved
    /// device from its second launch on: `render_devices` writes the name and [`Settings::parse`]
    /// reads it back as a name, and `disk_path` is the pre-name fallback that a modern save does not
    /// produce. The two-field `match` below is the whole of that knowledge and it is not to be
    /// copied — [`Settings::run_device`], [`Settings::missing_with`] and the window's row 3 all come
    /// here, so a device that resolves for the machine resolves for the sentence describing it.
    pub fn disk_of(&self, d: &Device) -> Option<Result<PathBuf, Absent>> {
        match (&d.disk, &d.disk_path) {
            (Some(name), _) => Some(
                self.disks
                    .iter()
                    .find(|x| x.name == *name)
                    .map(|x| x.path.clone())
                    .ok_or_else(|| Absent::Unlisted(name.clone())),
            ),
            (None, Some(p)) => Some(Ok(p.clone())),
            (None, None) => None,
        }
    }

    /// Names a device refers to that no longer resolve, and files that are no longer there.
    ///
    /// **Reported rather than swallowed.** A device whose firmware went missing should say which
    /// one, because "it boots to a white screen" is not a diagnosis and the name of the file that
    /// went is the whole of the answer.
    ///
    /// This performs a `stat(2)` per resolved path and **may block** — see [`Presence`] for the
    /// caller rule. Use [`Settings::missing_with`] to share one pass's answers across devices.
    pub fn missing(&self, d: &Device) -> Vec<Absent> {
        self.missing_with(d, &mut Presence::new())
    }

    /// The same, sharing one pass's worth of `stat` answers across several devices.
    ///
    /// The firmware absence comes first and the disk second, always, so the cradle's one-part
    /// sentence is stable. At most two elements.
    pub fn missing_with(&self, d: &Device, seen: &mut Presence) -> Vec<Absent> {
        let mut out = Vec::new();
        match self.firmware_of(d) {
            Err(a) => out.push(a),
            // A recipe has no file anywhere, and never goes missing.
            Ok(crate::nor::Source::Synthetic { .. }) => {}
            Ok(crate::nor::Source::File(p)) => {
                if !seen.exists(p) {
                    out.push(Absent::Gone(p.clone()));
                }
            }
        }
        match self.disk_of(d) {
            Some(Err(a)) => out.push(a),
            Some(Ok(p)) if !seen.exists(&p) => out.push(Absent::Gone(p)),
            // The disk is there, or the device names none — which is unfinished, not broken.
            Some(Ok(_)) | None => {}
        }
        out
    }

    /// Resolve a device's parts and make it the live one.
    ///
    /// `false`, and **nothing is mutated**, when there is no device of that name or when a name it
    /// carries does not resolve — its `firmware` to a [`Resource::Firmware`], or its `disk` to a
    /// drive in the list. The window asks [`Settings::missing`] which it was; this only refuses.
    /// There used to be a fallback here — the device's own inline copy of the recipe — and it is
    /// what made a moved dump boot a silently substituted generated 5.5G rather than say anything
    /// at all.
    ///
    /// **The disk obeys the same rule as the boot ROM**, and used not to. `disk_path` was kept as a
    /// last resort, but by the time a device has been through the settings file once there is no
    /// `disk_path` to fall back to — `render_devices` writes the name and `parse` reads it back as
    /// one — so an unresolvable name started a machine with no drive at all and said nothing.
    /// Before that round trip it was worse: the stale resolved path was still there and the machine
    /// booted the wrong drive, which is the `unwrap_or(d.nor)` substitution wearing a different
    /// hat. Naming **no** disk is still fine: that is an unfinished device, not a broken one.
    ///
    /// **The device being replaced is written back first**, so switching away from something you
    /// have been editing does not discard the edits — which is what every person switching between
    /// two of anything expects, and what they never say out loud. Both resolutions happen before
    /// that write-back, so a refusal leaves everything exactly as it was.
    pub fn run_device(&mut self, name: &str) -> bool {
        let Some(i) = self.devices.iter().position(|d| d.name == name) else {
            return false;
        };
        let Some(nor) = self.nor_of(&self.devices[i]).cloned() else {
            return false;
        };
        let disk = match self.disk_of(&self.devices[i]) {
            Some(Ok(p)) => Some(p),
            Some(Err(_)) => return false,
            None => None,
        };
        if let Some(c) = self.current.clone() {
            if c != name && self.devices.iter().any(|d| d.name == c) {
                let live = self.as_device(&c);
                if let Some(j) = self.devices.iter().position(|d| d.name == c) {
                    self.devices[j] = live;
                }
            }
        }
        // The write-back only ever touches an index other than `i` — it is guarded by `c != name` —
        // so the position resolved above is still this device.
        let d = self.devices[i].clone();
        // **The named resource wins**, so editing one changes every device made of it — the point
        // of composing rather than copying.
        self.nor = nor;
        self.disk = disk;
        self.chassis = d.chassis;
        self.work_on_copy = d.work_on_copy;
        self.current = Some(name.to_string());
        true
    }

    /// Save the live fields as a device under `name`, replacing any device of that name.
    ///
    /// **The live boot ROM is filed first**, because a device names its iPod and cannot name one
    /// that is not in the list. Filing is by value and idempotent, so a ROM already in the resources
    /// keeps the name it has and no second entry appears per save. `as_device` still prefers an
    /// existing device's reference, so re-saving a device whose stored ROM differs from the live one
    /// does not silently re-point it.
    pub fn remember_as(&mut self, name: &str) {
        // Sanitised here as well as in `as_device`, so the name this looks the device up by and the
        // name it saves under cannot be two different strings — which would push a duplicate.
        let name = one_line(name);
        let live = self.nor.clone();
        let suggested = suggest_nor_name(&live);
        self.file_away(Resource::Firmware(live), &suggested, None);
        let d = self.as_device(&name);
        match self.devices.iter().position(|x| x.name == name) {
            Some(i) => self.devices[i] = d,
            None => self.devices.push(d),
        }
        self.current = Some(name);
    }

    /// Remove a device. The live fields are untouched — forgetting the device you are running
    /// stops it being in the list, it does not stop it running.
    pub fn forget(&mut self, name: &str) {
        self.devices.retain(|d| d.name != name);
        if self.current.as_deref() == Some(name) {
            self.current = None;
        }
    }

    /// Record how long this device's cold boot took, for the next one's progress bar.
    pub fn record_boot(&mut self, instructions: u64) {
        let Some(c) = self.current.clone() else {
            return;
        };
        if let Some(d) = self.devices.iter_mut().find(|d| d.name == c) {
            d.boot_instructions = Some(instructions);
        }
    }

    /// Record that a complete restore point was written for this device at `at`.
    ///
    /// The time is reported by whoever wrote the snapshot — which has a machine and deliberately no
    /// [`Settings`] — and recorded here by whoever owns the device's name.
    /// `false` if there is no device of that name.
    pub fn record_park(&mut self, name: &str, at: u64) -> bool {
        match self.devices.iter_mut().find(|d| d.name == name) {
            Some(d) => {
                d.parked_at = Some(at);
                true
            }
            None => false,
        }
    }

    /// Forget this device's park time. **Deletes no files** — the caller does that, after saying
    /// what it is about to delete and how big it is.
    /// `false` if there is no device of that name.
    pub fn discard_park(&mut self, name: &str) -> bool {
        match self.devices.iter_mut().find(|d| d.name == name) {
            Some(d) => {
                d.parked_at = None;
                true
            }
            None => false,
        }
    }

    /// Record what a device boots, and **drop the denominator exactly when the shape moved**.
    ///
    /// §12.3 / §20 item 6. [`Device::boot_instructions`] is a measurement of one cold boot and is a
    /// prediction of the next one only while the device goes on booting the same thing — so this is
    /// the one function that owns the comparison, and it is called from wherever a recipe is
    /// committed rather than reimplemented there.
    ///
    /// **It keeps the number when the shape did not move**, which is the half that is easy to lose:
    /// re-saving a device you did not change must not cost it a boot without a bar. A `None` shape
    /// beside a `Some` number compares unequal to every real shape and drops it once, which is the
    /// right answer for a device written before the field existed — the number was measured on
    /// something nobody recorded, so nothing can vouch for it.
    ///
    /// `false` when there is no device of that name.
    pub fn set_boot_shape(&mut self, name: &str, shape: &crate::compose::BootShape) -> bool {
        let rendered = shape.render();
        let Some(d) = self.devices.iter_mut().find(|d| d.name == name) else {
            return false;
        };
        if d.boot_shape.as_deref() != Some(rendered.as_str()) {
            d.boot_instructions = None;
        }
        d.boot_shape = Some(rendered);
        true
    }

    /// **Replace what one filed iPod IS, keeping every reference pointing at it.**
    ///
    /// §11.2's level ① tunes an identity — a model, a colour, a serial, a GUID — and on the Edit
    /// route the iPod it is tuning is **already** a filed resource: `Composer::editing` opens on a
    /// device and carries the name that device's firmware is filed under. So saving is a
    /// *restatement* of one entry, not the filing of a second: without this, five saves over one
    /// seed leave five near-identical entries in the resources and four devices pointing at the one
    /// nobody is looking at.
    ///
    /// **Not because the iPod was filed at the mint.** It is not — `Composer::make_one` mints into
    /// the page and files nothing, and `Composer::commit` is the only filing. This doc asserted the
    /// mint-filing as fact for as long as GUI.md §11.2 asked for it and nothing built it; both are
    /// corrected. An unfiled iPod reaches `commit` with an empty `filed_as`, which this function
    /// refuses, and the caller files it.
    ///
    /// Four things happen together, and they are one act because doing three of them is a broken
    /// library:
    ///
    /// 1. the entry's [`Resource::Firmware`] becomes `what`;
    /// 2. its provenance is re-run through `normalised`, so a synthesised ROM's seed and its
    ///    recorded provenance cannot disagree after the seed changes;
    /// 3. its name is re-derived through [`suggest_nor_name`] — the same spelling the mint used —
    ///    made unique against **every other** entry, so restating an iPod to what it already was
    ///    keeps its own name rather than colliding with itself and becoming `… (2)`;
    /// 4. **every** device naming the old name is repointed at the new one.
    ///
    /// Step 4 is the decision worth defending. [`Settings::run_device`]'s own model is that the
    /// named resource wins — *editing one changes every device made of it*, which is the point of
    /// composing rather than copying — so refusing a shared edit would be the window contradicting
    /// the model. What the window owes instead is a sentence **before** it acts: *N devices are made
    /// of this iPod and will change with it.* [`Settings::devices_using_resource`] is what counts
    /// them.
    ///
    /// `None` for a name that resolves to nothing, or to an entry that is not a boot ROM.
    ///
    /// **An empty name resolves to nothing, and that is a guard rather than a formality.**
    /// [`Settings::parse`] fills sparse `res.N.` indices with *unnamed* placeholders, so
    /// `position(|it| it.name == "")` finds one and restates it — and the caller that hands an
    /// empty name is [`crate::compose`]'s window saying *this iPod has not been filed yet*, which
    /// is precisely the case where a placeholder must not be adopted. [`Settings::filed_under`]
    /// has refused unnamed entries since it was written, for the same reason and against the same
    /// list; this is the second lookup in this file and it needed the same rule.
    pub fn restate_firmware(
        &mut self,
        filed_as: &str,
        what: crate::nor::Source,
    ) -> Option<String> {
        if filed_as.is_empty() {
            return None;
        }
        let i = self.resources.iter().position(|it| it.name == filed_as)?;
        if !matches!(self.resources[i].what, Resource::Firmware(_)) {
            return None;
        }
        let suggested = suggest_nor_name(&what);
        // Unique against every entry **but this one**, through the same helper every other name in
        // this file goes through. A second copy of the suffix loop here is a second answer to
        // "what is this called", and the two would drift the first time either changed.
        let name = self.unique_name(&suggested, move |s| {
            s.resources
                .iter()
                .enumerate()
                .filter(move |(j, _)| *j != i)
                .map(|(_, it)| it.name.as_str())
        });
        let old = std::mem::replace(&mut self.resources[i].name, name.clone());
        self.resources[i].what = Resource::Firmware(what);
        self.resources[i].from = normalised(&self.resources[i].what, self.resources[i].from);
        for d in self.devices.iter_mut().filter(|d| d.firmware == old) {
            d.firmware = name.clone();
        }
        Some(name)
    }

    /// Rename a device, keeping `current` pointing at it.
    ///
    /// Refuses an empty name and a name a **different** device already holds — the name is the key,
    /// so two devices wearing one name is a list where `run_device`, `forget` and `remember_as` each
    /// pick whichever they find first. Renaming a device to the name it already has is `true` and
    /// writes nothing.
    ///
    /// **It does not move a snapshot set, and that is safe rather than overlooked.** A park is
    /// keyed on the device's name at the time it was written and carries its own `name.txt`, so a
    /// renamed device leaves a set that can still say whose it was — which is a set somebody can
    /// find and discard, rather than one silently adopted by whoever takes the old name next.
    pub fn rename_device(&mut self, old: &str, new: &str) -> bool {
        let new = one_line(new);
        if new.is_empty() {
            return false;
        }
        let Some(i) = self.devices.iter().position(|d| d.name == old) else {
            return false;
        };
        if self.devices[i].name == new {
            return true;
        }
        if self.devices.iter().any(|d| d.name == new) {
            return false;
        }
        self.devices[i].name = new.clone();
        if self.current.as_deref() == Some(old) {
            self.current = Some(new);
        }
        true
    }

    /// §11.4's `used by N`, for a resource: every device that names it as its iPod, in list order.
    ///
    /// An empty name matches nothing rather than every device that names nothing — `firmware` is a
    /// `String` and `""` is *this device names no iPod*, which is a device with a gap, not a device
    /// made of the entry somebody is about to remove.
    pub fn devices_using_resource(&self, name: &str) -> Vec<String> {
        if name.is_empty() {
            return Vec::new();
        }
        self.devices
            .iter()
            .filter(|d| d.firmware == name)
            .map(|d| d.name.clone())
            .collect()
    }

    /// The same, for a disk — **and the second arm is not optional.**
    ///
    /// A device migrated from the old shape carries a resolved `disk_path` and no name, so matching
    /// on the name alone reports *used by 0* for a drive something is running.
    ///
    /// The two arms are read in [`Settings::disk_of`]'s own order rather than as a plain `or`: a
    /// device that names a disk is using **that** disk, whatever its resolved path happens to hold,
    /// because that is what `disk_of` starts and `missing` reports. Reading them as a plain `or`
    /// would name a device in the consequence sentence that removing this disk would not touch,
    /// which is the same class of wrong as missing one.
    pub fn devices_using_disk(&self, name: &str) -> Vec<String> {
        if name.is_empty() {
            return Vec::new();
        }
        let path = self.disks.iter().find(|d| d.name == name).map(|d| &d.path);
        self.devices
            .iter()
            .filter(|d| match (&d.disk, &d.disk_path) {
                (Some(n), _) => n == name,
                (None, Some(p)) => path == Some(p),
                (None, None) => false,
            })
            .map(|d| d.name.clone())
            .collect()
    }

    /// Which drives record this resource — built from it, or with it installed on them.
    ///
    /// The other half of §11.4's `used by`: an `.ipsw` is named by no device and by every drive
    /// built from it, so a removal that counted devices alone would report *used by 0* about the
    /// bundle three drives came out of.
    pub fn disks_recording_resource(&self, name: &str) -> Vec<String> {
        if name.is_empty() {
            return Vec::new();
        }
        self.disks
            .iter()
            .filter(|d| {
                d.built_from.as_deref() == Some(name) || d.installed.iter().any(|x| x == name)
            })
            .map(|d| d.name.clone())
            .collect()
    }

    /// Remove one entry from the resources. **Touches no file.**
    ///
    /// §11.4's own rule, and both halves are deliberate:
    ///
    /// - **No file is deleted.** A boot ROM is sometimes the only dump of an iPod somebody owns.
    ///   Removing it from a list is *"stop showing me this"*; deleting it is not recoverable, and
    ///   the two must not be one press. Whoever wants the file gone is told where it is.
    /// - **No device is rewritten.** A device that named it goes on naming it, so
    ///   [`Settings::missing`] reports [`Absent::Unlisted`] and every surface — the cradle, the
    ///   Rail, the Devices page — says *which name* is gone. Silently blanking the reference would
    ///   turn a device somebody can repair into a device that is quietly incomplete.
    ///
    /// `false` when no entry of that name exists.
    pub fn remove_resource(&mut self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        let before = self.resources.len();
        self.resources.retain(|it| it.name != name);
        before != self.resources.len()
    }

    /// The same for a drive image — see [`Settings::remove_resource`] for both halves of the rule.
    ///
    /// The image on disk is a **separate** press with its own consequence and its own size, because
    /// it is the one thing here that can be gigabytes and the one thing that cannot be undone.
    pub fn remove_disk(&mut self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        let before = self.disks.len();
        self.disks.retain(|d| d.name != name);
        before != self.disks.len()
    }

    /// **What a device is, as a recipe** — the bridge §11.2's Edit mode stands on.
    ///
    /// In the model, because every resolution rule it needs is here: which disk a name resolves to,
    /// and what the device recorded about what it boots. A window that assembled this itself would
    /// be a second resolver beside [`Settings::disk_of`], and the two would disagree the first time
    /// either changed.
    ///
    /// **It never fails.** A device that names nothing produces `Start::FromIpsw("")`, which is
    /// `Recipe::nothing_chosen` — the verdict region's own opening state — rather than an error the
    /// caller has to invent a page for. An unfinished device opens the Composer on *nothing chosen*,
    /// which is exactly what it is.
    ///
    /// **A device that names a disk starts `FromDisk` and not `FromIpsw`.** The drive exists; a
    /// recipe that re-describes it references it rather than proposing to build a second one — which
    /// is also why `Start::FromDisk` costs nothing. `fat_type` is `None`: what the volume actually
    /// says is a read of the file, and reads do not happen here.
    ///
    /// [`Device::boot_shape`] is the authority on what it boots when there is one, because that is
    /// the thing recorded at the moment it was composed. The fallback reads the drive's own install
    /// list through `Os::from_label` and follows the systems to a bootloader with
    /// `Recipe::best_loader`, so a drive filed before shapes existed still opens as what is on it
    /// rather than as Apple-by-default.
    pub fn recipe_of(&self, d: &Device) -> crate::compose::Recipe {
        use crate::compose::{BootShape, Os, Recipe, Start};
        let start = match (&d.disk, &d.disk_path) {
            (Some(name), _) => Start::FromDisk {
                name: name.clone(),
                fat_type: None,
            },
            (None, Some(p)) => Start::FromImage {
                path: p.to_string_lossy().into_owned(),
                fat_type: None,
            },
            (None, None) => Start::FromIpsw(String::new()),
        };
        if let Some(shape) = d.boot_shape.as_deref().and_then(BootShape::parse) {
            return Recipe {
                start,
                loader: shape.loader,
                oses: shape.oses,
            };
        }
        let oses = d
            .disk
            .as_deref()
            .and_then(|n| self.disks.iter().find(|x| x.name == n))
            .map(|x| x.installed.iter().filter_map(|s| Os::from_label(s)).collect())
            .unwrap_or_default();
        let mut r = Recipe {
            start,
            loader: crate::compose::Loader::Apple,
            oses,
        };
        r.loader = r.best_loader();
        r
    }

    /// What the progress bar should divide by, if anything is known.
    pub fn expected_boot(&self) -> Option<u64> {
        self.current
            .as_deref()
            .and_then(|c| self.devices.iter().find(|d| d.name == c))
            .and_then(|d| d.boot_instructions)
            .filter(|n| *n > 0)
    }

    /// Write the settings file.
    ///
    /// **Through a `.part` and a rename**, the same shape `firmware::download` uses, because
    /// `fs::write` truncates before it writes: a process that died between the two left a device
    /// list that was half a file, and this is called on paths where nobody is watching. A rename
    /// within one directory is atomic, so the file on disk is either the old one or the new one.
    ///
    /// **It reports failure.** A read-only home, a full disk or a second process holding the file
    /// used to be swallowed, and the caller went on to say "Saved to …". A save nobody can see fail
    /// is a save that silently did not happen.
    pub fn save(&self) -> std::io::Result<()> {
        let dir = data_dir();
        std::fs::create_dir_all(&dir)?;
        let part = dir.join(format!("{FILE}.part"));
        std::fs::write(&part, self.render())?;
        match std::fs::rename(&part, dir.join(FILE)) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                Err(e)
            }
        }
    }

    /// Where the settings live, for the UI to print. A preference nobody can find is a preference
    /// nobody can reset.
    pub fn path() -> Option<PathBuf> {
        Some(data_dir().join(FILE))
    }
}

/// Seconds since the Unix epoch, now.
///
/// One place, so a park time and anything that compares against it cannot end up in different
/// units. Seconds because that is the coarsest unit a `parked · 4 min ago` can be rendered from,
/// because it is what [`std::time::SystemTime`] gives directly, and because it is readable in a
/// hand-editable settings file. A clock set before 1970 reads `0` rather than failing — a settings
/// file is not a place to fail.
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// How long ago this device was parked, in seconds, or `None` if it never was.
///
/// Saturating, because the two clocks are the same one read at different times: a machine whose
/// clock went backwards — an NTP step, a dual boot, a VM resumed — must read as `0` seconds ago
/// rather than as 584 942 417 355 years.
///
/// The phrase itself is the window's to choose. This is the last step the model takes.
pub fn parked_for(d: &Device, now: u64) -> Option<u64> {
    d.parked_at.map(|at| now.saturating_sub(at))
}

const FILE: &str = "settings.txt";

const APP: &str = "ipod-emulator";
/// What the directory was called before 2026-08-17. Read once, to move a user's settings forward
/// rather than silently starting them over.
const APP_WAS: &str = "ipod-gui";

fn env_dir(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// **One directory for everything this program writes**, and where it is depends on how the program
/// was delivered.
///
/// It used to be two — settings in the config directory, an 8 GB working disk and a 1.6 GB snapshot
/// in the cache directory — which on Windows meant `AppData\Roaming` *and* `AppData\Local`. A
/// reverse-engineering tool shipped as a zip is expected to stay where you put it, and that
/// expectation is right: somebody trying four firmware versions filled a drive they were not
/// watching, on a volume the program was not installed on.
///
/// - **Beside the executable**, in `data/`, when that directory can be written. This is the case for
///   a zip a user unpacked, which is how Windows and Linux get it, and it means deleting the folder
///   deletes everything.
/// - **The platform's application-support directory** otherwise — which is what a macOS `.app`
///   dragged to `/Applications` gets, because writing inside a bundle breaks its signature and the
///   volume may be read-only.
///
/// `IPOD_EMULATOR_DATA` overrides both, and is what the setup screen's "change" button sets.
pub fn data_dir() -> PathBuf {
    if let Some(d) = env_dir("IPOD_EMULATOR_DATA") {
        return d;
    }
    if let Some(beside) = beside_executable() {
        return beside;
    }
    platform_dir().unwrap_or_else(|| std::env::temp_dir().join(APP))
}

/// Is this executable sitting in a cargo build tree?
///
/// Data must not go beside it if so. A build tree is disposable by definition — `cargo clean`
/// deletes it without asking — and on this workspace one target directory is shared by every crate,
/// so it is also the last place that should quietly gain 8 GB. Measured, not imagined: running the
/// binary from `.cargo-target/release` put 6.4 GB in `.cargo-target/release/data`.
///
/// The test is a `debug`/`release` leaf under a `target`-ish ancestor at any depth, because
/// cross-compiling inserts the triple: `target/x86_64-pc-windows-gnu/release/`. A *released* build
/// unpacked into a directory the user happens to have called `release` does not match, since
/// nothing above it is a target directory — and that case is the whole point of beside-the-
/// executable, so it has to keep working.
fn in_build_tree(exe: &std::path::Path) -> bool {
    let Some(parent) = exe.parent() else {
        return false;
    };
    let leaf = parent.file_name().and_then(|n| n.to_str());
    if !matches!(leaf, Some("debug" | "release")) {
        return false;
    }
    parent
        .ancestors()
        .skip(1)
        .filter_map(|a| a.file_name().and_then(|n| n.to_str()))
        .any(|n| n == "target" || n.ends_with("-target"))
}

/// `data/` next to the running binary, if it is a directory we may write to.
///
/// Probed rather than assumed: a bundle's `Contents/MacOS`, `/usr/local/bin` and a read-only mount
/// all look like ordinary paths until the write fails.
fn beside_executable() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Inside a macOS bundle this would put user data in `Contents/MacOS/data`, which is both wrong
    // and signature-breaking. Recognise it and decline.
    if exe
        .components()
        .any(|c| c.as_os_str().to_string_lossy().ends_with(".app"))
    {
        return None;
    }
    if in_build_tree(&exe) {
        return None;
    }
    let dir = exe.parent()?.join("data");
    std::fs::create_dir_all(&dir).ok()?;
    // create_dir_all succeeds on a directory that already exists and is read-only, so prove it.
    let probe = dir.join(".writable");
    std::fs::write(&probe, b"").ok()?;
    let _ = std::fs::remove_file(&probe);
    Some(dir)
}

fn platform_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return env_dir("APPDATA").map(|d| d.join(APP));
    }
    if cfg!(target_os = "macos") {
        return home().map(|h| h.join("Library/Application Support").join(APP));
    }
    env_dir("XDG_CONFIG_HOME")
        .or_else(|| home().map(|h| h.join(".config")))
        .map(|d| d.join(APP))
}

/// The two directories the old build used, so their contents can be carried forward once.
fn legacy_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if cfg!(windows) {
        v.extend(env_dir("APPDATA").map(|d| d.join(APP_WAS)));
        v.extend(env_dir("LOCALAPPDATA").map(|d| d.join(APP_WAS)));
    } else if cfg!(target_os = "macos") {
        if let Some(h) = home() {
            v.push(h.join("Library/Application Support").join(APP_WAS));
            v.push(h.join("Library/Caches").join(APP_WAS));
        }
    } else {
        v.extend(
            env_dir("XDG_CONFIG_HOME")
                .or_else(|| home().map(|h| h.join(".config")))
                .map(|d| d.join(APP_WAS)),
        );
        v.extend(
            env_dir("XDG_CACHE_HOME")
                .or_else(|| home().map(|h| h.join(".cache")))
                .map(|d| d.join(APP_WAS)),
        );
    }
    v
}

/// Move a previous installation's settings file into the new directory, once.
///
/// Only the settings file. The old snapshots and working disks are gigabytes and keyed on paths
/// that may no longer exist; they are reported to the user for deletion rather than copied.
///
/// **This has no callers**, and it must run before the first [`Settings::load`] when it acquires
/// one: it declines the moment a file exists in the new directory, and `load` writes one back
/// whenever seeding changed something. `load` deliberately does not create a file that was not
/// already there, which is what keeps that order from mattering today.
pub fn migrate_legacy() {
    let dir = data_dir();
    if dir.join(FILE).exists() {
        return;
    }
    for old in legacy_dirs() {
        let f = old.join(FILE);
        if f.is_file() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::copy(&f, dir.join(FILE));
            return;
        }
    }
}

/// Old directories that still exist and still hold bytes, for the setup screen to offer to delete.
/// Returns each with its total size.
pub fn legacy_leftovers() -> Vec<(PathBuf, u64)> {
    legacy_dirs()
        .into_iter()
        .filter(|d| d.is_dir())
        .map(|d| {
            let n = dir_size(&d);
            (d, n)
        })
        .filter(|(_, n)| *n > 0)
        .collect()
}

/// Total bytes in a directory, including what is nested inside it.
///
/// **It used to stop at the first level**, with a comment saying that was all these directories
/// ever held — which was true until built drives moved into `drives/`. A figure that skipped the
/// largest files in the folder, on the screen whose whole job is telling somebody what this program
/// is costing them, is the kind of wrong that reads as reassuring.
pub fn dir_size(d: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(d) else {
        return 0;
    };
    rd.flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => on_disk_size(&m),
            Err(_) => 0,
        })
        .sum()
}

/// What a directory **claims** to hold, as against what [`dir_size`] says it costs.
///
/// The sibling of [`dir_size`], and it exists for one sentence rather than for arithmetic. §11.4's
/// Snapshots group has to say how much disk a parked machine is using, and the two figures differ
/// by more than a rounding here: `clone_disk` copies with `cp -c`, so on APFS a 1.6 GB park can be
/// **153 MB** of real disk. Neither number alone is the truth — the apparent one is what would be
/// written if every shared block were touched, and the materialised one is what deleting it gives
/// back today.
///
/// So the group renders [`dir_size`] and names this one **beside it where the two differ**, rather
/// than picking whichever reads better. A group whose whole argument is *this is where every byte
/// this program spends is visible* cannot open with a figure wrong by a factor of forty in the
/// direction of alarm — and it cannot open with one wrong in the direction of reassurance either.
pub fn dir_size_apparent(d: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(d) else {
        return 0;
    };
    rd.flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size_apparent(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// What a file actually costs, rather than how long it claims to be.
///
/// The drive images here are **sparse** and, on APFS, **clones**: `clone_disk` copies with `cp -c`,
/// so every block the emulator has not written is shared with the source rather than duplicated.
/// `len()` reports the logical 8 GB regardless, and summing that told the operator the cache had
/// reached 32 GB. It had not. Deleting one whole set — two 3.1 GB drive files and a snapshot, 6.4 GB
/// by that reckoning — returned **153 MB** of real disk, which is the snapshot and almost nothing
/// else.
///
/// A number that is wrong by a factor of forty, in the direction of alarm, about the user's own
/// disk, is not a cosmetic defect. `st_blocks` is in 512-byte units by POSIX definition, whatever
/// the filesystem's own block size is.
pub fn on_disk_size(m: &std::fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        m.blocks() * 512
    }
    #[cfg(not(unix))]
    {
        m.len()
    }
}

fn home() -> Option<PathBuf> {
    env_dir("HOME").or_else(|| env_dir("USERPROFILE"))
}

/// This repository's root, or the working directory when there is no repository — which is what a
/// released binary is always in.
///
/// Two walks, and deliberately no compile-time path. The obvious third fallback is
/// `env!("CARGO_MANIFEST_DIR")`, and it was one here: it is the absolute path of the machine that
/// did the build, so it is baked into every published binary (naming a stranger's home directory)
/// and it is wrong on every machine but that one. `--remap-path-prefix` cannot reach it, because it
/// is a cargo variable rather than a path `rustc` embeds.
///
/// - **Up from the executable**, which is right for `target/release/ipod-boot` in a checkout.
/// - **Up from the working directory**, which is what catches a shared `CARGO_TARGET_DIR` — the
///   binary is then nowhere near the source, and this is the case the compile-time path used to
///   cover.
/// - Otherwise the working directory, so the paths built from it are somewhere the user can see,
///   and the "no NOR dump at …" message names a plausible place rather than a build machine.
pub fn repo_root() -> PathBuf {
    /// The directory that holds the recipes — present in a checkout, absent in a release archive.
    const MARKER: &str = "tools/ipod-boot";

    if let Ok(exe) = std::env::current_exe() {
        let mut p = exe.as_path();
        while let Some(dir) = p.parent() {
            if dir.join(MARKER).is_dir() {
                return dir.to_path_buf();
            }
            p = dir;
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut p = cwd.as_path();
        loop {
            if p.join(MARKER).is_dir() {
                return p.to_path_buf();
            }
            match p.parent() {
                Some(up) => p = up,
                None => break,
            }
        }
        return cwd;
    }
    PathBuf::from(".")
}

// Small helpers so a settings file can set the synthetic fields in any order, and so a `flash =`
// line followed by `nor_model =` does the obvious thing rather than half of each.
type Synth = (String, u64, Option<String>, Option<u64>, Option<PathBuf>);

fn as_synth(src: crate::nor::Source) -> Synth {
    match src {
        crate::nor::Source::Synthetic {
            model,
            seed,
            serial,
            guid,
            splash,
        } => (model, seed, serial, guid, splash),
        crate::nor::Source::File(_) => match crate::nor::Source::default() {
            crate::nor::Source::Synthetic {
                model,
                seed,
                serial,
                guid,
                splash,
            } => (model, seed, serial, guid, splash),
            crate::nor::Source::File(_) => unreachable!("the default is synthetic"),
        },
    }
}

/// Split `prefix.N.field` into `(N, field)`. `None` for anything that is not that shape.
///
/// One place, because three sections use it and the fourth copy is where the off-by-one goes.
fn indexed(key: &str) -> Option<(usize, &str)> {
    let mut it = key.splitn(3, '.');
    let (_, idx, field) = (it.next()?, it.next()?, it.next()?);
    Some((idx.parse().ok()?, field))
}

/// Apply one `nor_*` settings key to a ROM source, or return `None` if it is not one.
///
/// **One place, because there are now three callers**: the live machine's keys, a saved machine's
/// `machine.N.nor_*`, and a library entry's `item.N.nor_*`. Two of those were written by copying
/// the first, which is exactly how a fourth key gets added to two of the three.
pub fn nor_field(src: crate::nor::Source, field: &str, v: &str) -> Option<crate::nor::Source> {
    match field {
        "nor_model" if !v.is_empty() => Some(with_model(src, v)),
        "nor_seed" => v.parse::<u64>().ok().map(|n| with_seed(src, n)),
        "nor_serial" => Some(with_serial(src, v)),
        "nor_splash" => Some(with_splash(src, v)),
        "nor_guid" => u64::from_str_radix(v.trim_start_matches("0x"), 16)
            .ok()
            .map(|g| with_guid(src, g)),
        _ => None,
    }
}

fn with_model(src: crate::nor::Source, v: &str) -> crate::nor::Source {
    let (_, seed, serial, guid, splash) = as_synth(src);
    crate::nor::Source::Synthetic {
        model: v.to_string(),
        seed,
        serial,
        guid,
        splash,
    }
}
fn with_seed(src: crate::nor::Source, n: u64) -> crate::nor::Source {
    let (model, _, serial, guid, splash) = as_synth(src);
    crate::nor::Source::Synthetic {
        model,
        seed: n,
        serial,
        guid,
        splash,
    }
}
fn with_serial(src: crate::nor::Source, v: &str) -> crate::nor::Source {
    let (model, seed, _, guid, splash) = as_synth(src);
    let serial = (!v.trim().is_empty()).then(|| v.trim().to_string());
    crate::nor::Source::Synthetic {
        model,
        seed,
        serial,
        guid,
        splash,
    }
}
fn with_guid(src: crate::nor::Source, g: u64) -> crate::nor::Source {
    let (model, seed, serial, _, splash) = as_synth(src);
    crate::nor::Source::Synthetic {
        model,
        seed,
        serial,
        guid: Some(g),
        splash,
    }
}
fn with_splash(src: crate::nor::Source, v: &str) -> crate::nor::Source {
    let (model, seed, serial, guid, _) = as_synth(src);
    let splash = (!v.trim().is_empty()).then(|| PathBuf::from(v.trim()));
    crate::nor::Source::Synthetic {
        model,
        seed,
        serial,
        guid,
        splash,
    }
}

/// Serialises the tests that set `IPOD_EMULATOR_DATA`.
///
/// [`std::env::set_var`] is process-global and cargo runs tests on several threads, so two tests
/// that both set it interleave and one reads the other's directory. That is a flake nobody can
/// reproduce, so it is a lock rather than a convention. A test that panicked holding it must not
/// poison every later one, hence the `into_inner`.
#[cfg(test)]
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    /// A build tree is refused; a real install beside the executable is not.
    ///
    /// The second half matters as much as the first. Declining too eagerly would send a user who
    /// unpacked the release tarball into `~/Downloads/ipod-emulator/` off to Application Support
    /// instead, which is the split that produced two data folders and a bug report.
    #[test]
    fn data_never_lands_in_a_build_tree() {
        use std::path::Path;
        for p in [
            "/Users/x/dev/target/release/ipod-emulator",
            "/Users/x/dev/target/debug/ipod-emulator",
            // The shared workspace target, which is what this was found in.
            "/Users/x/dev/.cargo-target/release/ipod-emulator",
            // Cross-compiling inserts the triple, so the ancestor walk has to go deeper than one.
            "/Users/x/dev/target/x86_64-pc-windows-gnu/release/ipod-emulator.exe",
            "/Users/x/dev/.cargo-target/aarch64-apple-darwin/debug/ipod-emulator",
        ] {
            assert!(in_build_tree(Path::new(p)), "must refuse: {p}");
        }
        for p in [
            // An unpacked release. The directory is called `release`, but nothing above it is a
            // target directory, and putting data here is the entire feature.
            "/Users/x/Downloads/ipod-emulator-0.4.0-release/ipod-emulator",
            "/Users/x/Downloads/ipod-emulator/ipod-emulator",
            "/Applications/ipod-emulator.app/Contents/MacOS/ipod-emulator",
            "/usr/local/bin/ipod-emulator",
        ] {
            assert!(!in_build_tree(Path::new(p)), "must allow: {p}");
        }
    }

    use super::*;

    /// **Renamed rather than deleted when `Mode` went**, and the three assertions under it are why:
    /// the name was the only part of it that was about a mode.
    #[test]
    fn a_missing_file_configures_nothing() {
        let s = Settings::parse("");
        assert_eq!(s.flash(), None);
        assert_eq!(s.disk, None);
        assert!(!s.check_updates_on_start);
    }

    /// All three colours survive the file, **`auto` is the default and round-trips as itself**,
    /// and the key this replaced is still honoured — a person who had already chosen black must
    /// not meet a white iPod after an update.
    #[test]
    fn the_chassis_colour_round_trips_and_the_old_key_still_works() {
        use crate::identity::Colour;
        for c in [Colour::White, Colour::Black, Colour::U2] {
            let s = Settings {
                chassis: Some(c),
                ..Settings::default()
            };
            assert_eq!(Settings::parse(&s.render()).chassis, Some(c), "{c:?}");
        }
        // The default is "ask the ROM", and it survives a write-then-read rather than collapsing
        // into whichever colour happens to be first in the enum.
        assert_eq!(Settings::default().chassis, None);
        assert!(Settings::default().render().contains("chassis = auto"));
        assert_eq!(Settings::parse(&Settings::default().render()).chassis, None);
        assert_eq!(Settings::parse("chassis = auto").chassis, None);

        assert_eq!(
            Settings::parse("black_device = true").chassis,
            Some(Colour::Black)
        );
        // An unreadable value falls back to the ROM rather than picking a colour at random.
        assert_eq!(Settings::parse("chassis = chartreuse").chassis, None);
    }

    /// A synthesised ROM is stored as its recipe, and the recipe has to survive the file.
    #[test]
    fn a_synthetic_nor_round_trips_as_a_recipe_not_as_bytes() {
        use crate::nor::Source;
        let s = Settings {
            nor: Source::Synthetic {
                model: "A446".into(),
                seed: 987654321,
                serial: Some("AB1234XYZQR".into()),
                guid: Some(0x000A_2700_1122_3344),
                splash: None,
            },
            ..Settings::default()
        };
        let text = s.render();
        // Stored as a few lines, not a megabyte.
        assert!(text.contains("nor_model = A446"), "{text}");
        assert!(text.contains("nor_seed = 987654321"), "{text}");
        assert!(text.contains("nor_guid = 000A270011223344"), "{text}");
        assert!(
            !text.contains("flash ="),
            "a synthetic ROM has no path: {text}"
        );
        assert_eq!(Settings::parse(&text).nor, s.nor);
    }

    /// **The key an older settings file uses still works.** Somebody who had pointed this at their
    /// own dump must not silently get a generated iPod after an update.
    #[test]
    fn an_older_settings_file_keeps_its_dump() {
        use crate::nor::Source;
        let s = Settings::parse("flash = /somewhere/internal_rom.bin\ndisk = /somewhere/d.img\n");
        assert_eq!(
            s.nor,
            Source::File(PathBuf::from("/somewhere/internal_rom.bin"))
        );
        assert_eq!(
            s.flash(),
            Some(PathBuf::from("/somewhere/internal_rom.bin"))
        );
    }

    /// With nothing configured the ROM is generated rather than missing — which is the whole point
    /// of the synthetic path, and it means a fresh clone has a working iPod to offer.
    #[test]
    fn a_fresh_install_defaults_to_a_generated_rom() {
        let s = Settings::default();
        assert_eq!(s.flash(), None, "nothing to load from disk");
        let id = s.nor.identity().expect("a generated identity");
        assert_eq!(id.guid >> 40, crate::identity::APPLE_OUI);
        assert!(s.nor.bytes().expect("builds").len() == crate::inspect::NOR_LEN as usize);
        // And it is a real, described machine rather than a blank.
        assert!(s.nor.describe().contains("30 GB"), "{}", s.nor.describe());
    }

    /// The round trip is the contract: whatever the window writes, the next launch reads back.
    #[test]
    fn settings_round_trip_through_the_file_format() {
        let s = Settings {
            chassis: Some(crate::identity::Colour::Black),
            nor: crate::nor::Source::File(PathBuf::from("/a/b/rom.bin")),
            disk: Some(PathBuf::from("/a/b/disk.img")),
            check_updates_on_start: true,
            welcomed: true,
            work_on_copy: Some(true),
            devices: Vec::new(),
            current: None,
            resources: Vec::new(),
            disks: Vec::new(),
            library_seeded: false,
        };
        assert_eq!(Settings::parse(&s.render()), s);
    }

    /// A path with a space in it must survive the format, which is why the value is the rest of
    /// the line and not a token. Spaces in a chosen path are ordinary, not exotic.
    #[test]
    fn a_path_with_spaces_survives() {
        let p = PathBuf::from("/some where/My iPod Backups/x.img");
        let s = Settings {
            disk: Some(p.clone()),
            ..Default::default()
        };
        assert_eq!(Settings::parse(&s.render()).disk, Some(p));
    }

    /// Garbage in a settings file must not stop the program. Every one of these lines is something
    /// a hand-edit produces.
    #[test]
    fn junk_is_ignored_rather_than_fatal() {
        let s = Settings::parse(
            "# comment\n\
             \n\
             mode = sideways\n\
             nonsense\n\
             future_key = 7\n\
             disk =\n\
             check_updates_on_start = maybe\n",
        );
        // `mode = sideways` above is now a key this program has never heard of, which is exactly
        // what it has to be: an existing settings file carrying `mode = debug` must be read as it
        // always was and lose the line on the next save, rather than complain about it.
        assert_eq!(
            s.disk, None,
            "an empty value is `not set`, not `the empty path`"
        );
        assert!(!s.check_updates_on_start, "anything but `true` is off");
    }

    #[test]
    fn the_data_directory_is_absolute() {
        let _guard = env_lock();
        let d = data_dir();
        assert!(d.is_absolute(), "{}", d.display());
    }

    /// `IPOD_EMULATOR_DATA` is what the setup screen's "change" button sets, so it has to win over
    /// both the beside-the-executable default and the platform directory.
    #[test]
    fn the_override_wins() {
        let _guard = env_lock();
        // SAFETY: `env_lock` serialises every test in this crate that touches this variable, and
        // the value is restored before it returns.
        let before = std::env::var_os("IPOD_EMULATOR_DATA");
        unsafe { std::env::set_var("IPOD_EMULATOR_DATA", "/tmp/ipod-emulator-test-dir") };
        assert_eq!(data_dir(), PathBuf::from("/tmp/ipod-emulator-test-dir"));
        match before {
            Some(v) => unsafe { std::env::set_var("IPOD_EMULATOR_DATA", v) },
            None => unsafe { std::env::remove_var("IPOD_EMULATOR_DATA") },
        }
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;

    fn synth(model: &str, seed: u64) -> crate::nor::Source {
        crate::nor::Source::Synthetic {
            model: model.into(),
            seed,
            serial: None,
            guid: None,
            splash: None,
        }
    }

    /// A directory of our own, named after the test, so two running at once cannot collide.
    ///
    /// The counter is there because a test may want two, and `SystemTime` has a resolution these
    /// calls can outrun.
    fn temp_dir(what: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "ipod-emulator-test-{what}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&d).expect("a temp directory");
        d
    }

    /// **A machine list has to survive the round trip, or it is not storage.**
    #[test]
    fn devices_survive_render_and_parse() {
        let mut s = Settings {
            nor: synth("A146", 5),
            ..Default::default()
        };
        s.disk = Some(PathBuf::from("/drives/one.img"));
        s.remember_as("Video 5G");
        s.nor = crate::nor::Source::File(PathBuf::from("/roms/real.bin"));
        s.disk = Some(PathBuf::from("/drives/two.img"));
        s.remember_as("my own iPod");
        s.record_park("Video 5G", 1_755_738_000);

        let back = Settings::parse(&s.render());
        assert_eq!(back.devices.len(), 2, "both devices came back");
        assert_eq!(back.devices[0].name, "Video 5G");
        assert_eq!(
            back.nor_of(&back.devices[0]),
            Some(&synth("A146", 5)),
            "a synthesised ROM is a recipe, resolved through the resource it names"
        );
        assert_eq!(
            back.devices[0].parked_at,
            Some(1_755_738_000),
            "the park time did not survive the file"
        );
        // The disk arrives as a *name*, and it resolves to the path it was seeded from.
        assert_eq!(
            back.devices[1].disk_path,
            Some(PathBuf::from("/drives/two.img"))
        );
        assert_eq!(back.current.as_deref(), Some("my own iPod"));
        // The whole device, field for field — which is what makes this storage rather than a
        // hopeful subset of it.
        assert_eq!(back.devices, s.devices);
    }

    /// **A device holds exactly one reference to its iPod, and it resolves.**
    ///
    /// The pair this replaced was a name *and* a resolved recipe, and the resolved one won when
    /// they disagreed — so a device whose dump had moved silently booted a generated 5.5G.
    #[test]
    fn a_device_holds_one_reference_to_its_ipod() {
        let mut s = Settings {
            nor: synth("A146", 5),
            ..Default::default()
        };
        s.remember_as("mine");
        let d = s.devices[0].clone();
        assert!(
            !d.firmware.is_empty(),
            "a device this program made names no iPod"
        );
        assert!(
            s.resources
                .iter()
                .any(|it| it.name == d.firmware
                    && matches!(&it.what, Resource::Firmware(src) if *src == synth("A146", 5))),
            "the name does not reach a firmware resource: {:?}",
            s.resources
        );
        assert_eq!(s.nor_of(&d), Some(&synth("A146", 5)));

        // A second, differently seeded ROM ahead of it in the list must not be what the name
        // resolves to — there is no "first firmware wins" anywhere.
        s.resources.insert(
            0,
            Item {
                name: "a decoy".into(),
                what: Resource::Firmware(synth("A146", 99)),
                from: None,
            },
        );
        assert_eq!(
            s.nor_of(&d),
            Some(&synth("A146", 5)),
            "the resolution ignored the name"
        );
    }

    /// A name that reaches an entry of the **wrong kind** resolves to nothing, and the two functions
    /// that ask cannot disagree about it.
    #[test]
    fn a_name_that_resolves_to_the_wrong_kind_is_not_a_boot_rom() {
        let mut s = Settings::default();
        s.file_away(
            Resource::Installer(PathBuf::from("/fw/iPod_20.1.3.ipsw")),
            "my dump",
            Some(Provenance::Provided),
        );
        let d = Device {
            name: "confused".into(),
            firmware: "my dump".into(),
            ..Device::default()
        };
        assert_eq!(s.nor_of(&d), None, "an .ipsw was accepted as a boot ROM");
        assert_eq!(
            s.missing(&d),
            vec![Absent::Unlisted("my dump".into())],
            "the name resolves to nothing and nothing said so"
        );
        assert!(!s.run_device("confused"), "there is no device of that name");
    }

    /// The two lookups are one lookup, so they cannot drift.
    #[test]
    fn missing_and_nor_of_never_disagree() {
        let mut s = Settings::default();
        s.file_away(Resource::Firmware(synth("A146", 5)), "a real one", None);
        s.file_away(
            Resource::Installer(PathBuf::from("/fw/x.ipsw")),
            "wrong kind",
            None,
        );
        for name in ["a real one", "wrong kind", "nothing of that name", ""] {
            let d = Device {
                firmware: name.into(),
                ..Device::default()
            };
            assert_eq!(
                s.missing(&d).is_empty(),
                s.nor_of(&d).is_some(),
                "`missing` and `nor_of` disagree about {name:?}"
            );
        }
    }

    /// The three callers that need a ROM's default name have to spell it the same way, or reading a
    /// settings file back mints duplicates under new names.
    #[test]
    fn a_synthesised_rom_and_a_dump_are_named_the_way_they_always_were() {
        assert_eq!(suggest_nor_name(&synth("A146", 5)), "A146, seed 5");
        assert_eq!(
            suggest_nor_name(&crate::nor::Source::File(PathBuf::from("/roms/retail.bin"))),
            "retail"
        );
        // And the round trip: file the same value twice, get one entry.
        let mut s = Settings::default();
        let a = s.file_away(Resource::Firmware(synth("A146", 5)), "A146, seed 5", None);
        let b = s.file_away(
            Resource::Firmware(synth("A146", 5)),
            &suggest_nor_name(&synth("A146", 5)),
            None,
        );
        assert_eq!(a, b);
        assert_eq!(s.resources.len(), 1);
    }

    /// **Switching must be able to go back to a synthesised ROM**, which is the whole complaint
    /// that started this: one boot from a dump used to make "generate one" unreachable.
    #[test]
    fn switching_restores_a_synthesised_rom() {
        let mut s = Settings {
            nor: synth("A146", 7),
            ..Default::default()
        };
        s.remember_as("generated");
        s.nor = crate::nor::Source::File(PathBuf::from("/roms/real.bin"));
        s.remember_as("real dump");

        assert!(s.run_device("generated"));
        assert_eq!(s.nor, synth("A146", 7), "back to the recipe, not to a path");
        assert!(s.run_device("real dump"));
        assert!(matches!(s.nor, crate::nor::Source::File(_)));
        assert!(!s.run_device("nothing of that name"));
    }

    /// **A device whose iPod is gone refuses, and changes nothing on the way out.**
    ///
    /// It used to boot a silently substituted generated 5.5G — the resolution fell back to the
    /// device's own inline copy of the recipe, which is the pair this model change deleted.
    #[test]
    fn a_device_whose_ipod_is_gone_refuses_and_changes_nothing() {
        let mut s = Settings {
            nor: synth("A146", 7),
            ..Default::default()
        };
        s.remember_as("mine");
        s.nor = crate::nor::Source::File(PathBuf::from("/roms/still here.bin"));
        s.current = Some("something else".into());
        s.resources.clear();

        let before = s.nor.clone();
        assert!(!s.run_device("mine"), "it started without an iPod");
        assert_eq!(s.nor, before, "the live ROM was changed by a refusal");
        assert_eq!(
            s.current.as_deref(),
            Some("something else"),
            "the live device was changed by a refusal"
        );
        assert_eq!(
            s.missing(&s.devices[0]),
            vec![Absent::Unlisted("A146, seed 7".into())],
            "the refusal did not name what is gone"
        );
    }

    /// **A device whose drive is gone from the list refuses too**, and for the same reason the
    /// boot ROM does.
    ///
    /// It used to start anyway. After one round trip through the settings file there is no
    /// `disk_path` left to fall back on — `render_devices` writes the name and `parse` reads it
    /// back as one — so the machine started with **no drive at all** and nothing said so, while
    /// `missing` was reporting the very same name as absent. Three of the four `disk_of` outcomes
    /// had the two functions disagreeing.
    #[test]
    fn a_device_whose_drive_is_gone_refuses_and_missing_agrees() {
        let mut s = Settings {
            nor: synth("A146", 3),
            disk: Some(PathBuf::from("/drives/mine.img")),
            ..Default::default()
        };
        s.file_disk(PathBuf::from("/drives/mine.img"), "mine");
        s.remember_as("mine");
        assert!(s.run_device("mine"), "it refused a device that is complete");

        // The drive leaves the library — a Parts remove, or a hand-edit. The name on the device
        // now resolves to nothing.
        let saved = Settings::parse(&s.render());
        let mut s = saved;
        s.disks.clear();
        s.disk = Some(PathBuf::from("/drives/something else.img"));
        let before = s.disk.clone();

        assert_eq!(
            s.devices[0].disk_path, None,
            "the fixture must have been through the file, which is where the fallback vanishes"
        );
        assert!(!s.run_device("mine"), "it started with no drive at all");
        assert_eq!(s.disk, before, "the live drive was changed by a refusal");
        assert!(
            s.missing(&s.devices[0])
                .contains(&Absent::Unlisted("mine".into())),
            "`missing` and `run_device` disagree about the drive: {:?}",
            s.missing(&s.devices[0])
        );
    }

    /// A device that names **no** drive is unfinished, not broken: it still starts, and `missing`
    /// still says nothing about it. The refusal above must not swallow first run.
    #[test]
    fn a_device_with_no_drive_at_all_still_starts() {
        let mut s = Settings {
            nor: synth("A146", 4),
            ..Default::default()
        };
        s.remember_as("bare");
        s.disk = None;
        s.devices[0].disk = None;
        s.devices[0].disk_path = None;
        assert!(s.run_device("bare"), "an unfinished device was refused");
        assert!(s.missing(&s.devices[0]).is_empty());
    }

    /// **A device that names nothing says so, rather than saying `missing` and stopping.**
    ///
    /// `summary` renders `missing {label}`, and `Absent::Unlisted("")` made that `missing ` with
    /// nothing after it — the caption saying something is wrong and refusing to say what, which is
    /// the one thing §9 forbids.
    #[test]
    fn a_device_that_names_no_ipod_is_named_as_that() {
        let s = Settings::default();
        let d = Device::default();
        assert_eq!(
            s.missing(&d),
            vec![Absent::Unlisted("an iPod".into())],
            "a device with no iPod produced a nameless absence"
        );
        assert!(
            !s.missing(&d)[0].label().is_empty(),
            "an absence with an empty label names nothing"
        );
    }

    /// **A hand-edit that deletes one device block does not leave a phantom behind.**
    ///
    /// Indices are tolerated sparse on read and the gaps are filled with `Device::default()`, so
    /// deleting `device.1.*` used to leave a nameless placeholder in the list — and once devices
    /// carried a named iPod, `adopt_inline_roms` minted a generated one for it. A machine nobody
    /// made, pointing at an iPod nobody added.
    #[test]
    fn a_gap_in_the_device_numbering_is_not_a_device() {
        let s = Settings::parse(
            "device.0.name = a\ndevice.0.firmware = mine\n\
             device.2.name = c\ndevice.2.nor_model = A146\ndevice.2.nor_seed = 9\n",
        );
        assert_eq!(
            s.devices.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            vec!["a", "c"],
            "a nameless placeholder survived: {:?}",
            s.devices
        );
        // And the surviving device kept **its own** recipe: dropping the gap must not shift the
        // side table the migration reads.
        assert_eq!(s.devices[1].firmware, "A146, seed 9", "{:?}", s.resources);
        assert!(
            !s.resources.iter().any(|it| it.name == "A446, seed 0"),
            "the phantom was given a generated iPod: {:?}",
            s.resources
        );
    }

    /// **A filename with a line break in it cannot inject keys into the settings file.**
    ///
    /// Every name here is derived from a filename, and every filesystem this runs on allows a
    /// newline in one. Written straight out, `res.0.name = mine⏎res.0.kind = software` files
    /// somebody's boot ROM as software on the next read.
    #[test]
    fn a_name_out_of_a_hostile_filename_cannot_split_the_record() {
        let mut s = Settings::default();
        let hostile = PathBuf::from("/roms/mine\nres.0.kind = software.bin");
        let name = s.file_away(
            Resource::Firmware(crate::nor::Source::File(hostile)),
            &suggest_nor_name(&crate::nor::Source::File(PathBuf::from(
                "/roms/mine\nres.0.kind = software.bin",
            ))),
            None,
        );
        assert!(
            !name.contains('\n'),
            "the filed name still holds a line break: {name:?}"
        );
        s.remember_as("a\nname");
        assert_eq!(s.devices[0].name, "a name", "the device name was not repaired");
        assert_eq!(s.devices.len(), 1, "the lookup and the save used two names");
        // `as_device` is `pub` and repairs the name itself, not only because `remember_as` does.
        // Both, deliberately: a caller that reaches `as_device` directly must not be able to mint
        // a device whose name splits the record, and `remember_as` must look the device up under
        // the same name it saves it under or it pushes a duplicate on every save.
        assert_eq!(s.as_device("b\r\nname").name, "b name");

        let text = s.render();
        let back = Settings::parse(&text);
        assert!(
            matches!(back.resources[0].what, Resource::Firmware(_)),
            "the record split and the kind was overwritten:\n{text}"
        );
        assert_eq!(back.devices[0].name, "a name");
        assert_eq!(back.current.as_deref(), Some("a name"));
    }

    /// Switching away from a machine you have edited keeps the edits — including which iPod it is
    /// made of, which a re-derivation from the live fields would quietly re-point.
    #[test]
    fn switching_writes_back_what_you_were_editing() {
        let mut s = Settings {
            nor: synth("A146", 1),
            ..Default::default()
        };
        s.remember_as("a");
        s.nor = synth("A146", 2);
        s.remember_as("b");
        let a_boots = s.devices[0].firmware.clone();
        s.run_device("a");
        s.disk = Some(PathBuf::from("/drives/edited.img"));

        // Re-seed the entry `a` is made of, the way the Parts page will. **This is what the
        // write-back has to survive**: a re-derivation from the live values goes looking for a
        // resource matching what the ROM used to be, finds none, and quietly cuts the device loose
        // from what it is made of.
        let i = s
            .resources
            .iter()
            .position(|it| it.name == a_boots)
            .expect("the entry `a` is made of");
        s.resources[i].what = Resource::Firmware(synth("A146", 42));

        s.run_device("b");
        assert!(s.run_device("a"));
        assert_eq!(
            s.disk,
            Some(PathBuf::from("/drives/edited.img")),
            "the edit made while `a` was live came back with it"
        );
        assert_eq!(
            s.devices[0].firmware, a_boots,
            "the write-back re-pointed `a` at whatever was live"
        );
        assert_eq!(
            s.nor,
            synth("A146", 42),
            "`a` did not follow the entry it is made of"
        );
    }

    /// The progress bar's denominator is per machine, and absent until one boot has finished.
    #[test]
    fn the_expected_boot_is_learned_not_assumed() {
        let mut s = Settings::default();
        assert_eq!(
            s.expected_boot(),
            None,
            "nothing is known before the first boot"
        );
        s.remember_as("one");
        assert_eq!(s.expected_boot(), None);
        s.record_boot(1_600_000_000);
        assert_eq!(s.expected_boot(), Some(1_600_000_000));
        s.remember_as("two");
        s.record_boot(21_500_000_000);
        assert_eq!(
            s.expected_boot(),
            Some(21_500_000_000),
            "each machine learns its own"
        );
        s.run_device("one");
        assert_eq!(s.expected_boot(), Some(1_600_000_000), "and keeps it");
    }

    /// **An old settings file must still describe the machine it described.** Anyone updating has
    /// one machine in the old keys and no machine list at all.
    #[test]
    fn a_settings_file_from_before_machines_still_loads() {
        let old = "mode = user\nflash = /roms/mine.bin\ndisk = /drives/mine.img\nchassis = black\n";
        let s = Settings::parse(old);
        assert_eq!(
            s.nor,
            crate::nor::Source::File(PathBuf::from("/roms/mine.bin"))
        );
        assert_eq!(s.disk, Some(PathBuf::from("/drives/mine.img")));
        assert!(s.devices.is_empty(), "no list, and that is not an error");
        assert_eq!(s.current, None);
    }

    // ------------------------------------------------------------------------- the library

    /// **The library has to survive the round trip, or it is not storage** — the same bar the
    /// machine list is held to, for the same reason.
    #[test]
    fn the_resources_survive_render_and_parse() {
        let mut s = Settings::default();
        s.file_away(
            Resource::Firmware(synth("A146", 5)),
            "a synthesised 30 GB",
            None,
        );
        s.file_away(
            Resource::Firmware(crate::nor::Source::File(PathBuf::from("/roms/retail.bin"))),
            "my own dump",
            Some(Provenance::Dumped),
        );
        // Size-only on purpose: the round trip has to cover the case §11.4 forbids upgrading.
        s.file_away(
            Resource::Installer(PathBuf::from("/fw/iPod_20.1.3.ipsw")),
            "20.1.3",
            Some(Provenance::Fetched {
                verified: Verification::SizeOnly,
            }),
        );
        s.file_away(
            Resource::Software(PathBuf::from("/software/rockbox.ipod")),
            "Rockbox 4.0",
            Some(Provenance::Fetched {
                verified: Verification::Sha256,
            }),
        );
        s.disks.push(Disk {
            name: "Music 30GB".into(),
            path: PathBuf::from("/drives/music.img"),
            built_from: Some("20.1.3".into()),
            installed: vec!["Rockbox 4.0".into(), "ipodloader2".into()],
        });

        let back = Settings::parse(&s.render());
        assert_eq!(
            back.resources, s.resources,
            "the resources did not come back as they went in"
        );
        assert_eq!(
            back.disks, s.disks,
            "the disks did not come back as they went in"
        );
    }

    /// **Filing the same thing twice makes one entry, not two.** Identity is the value, not the
    /// name: two entries for one path would be two names for one thing, and nothing could say
    /// which one you were running.
    #[test]
    fn filing_the_same_file_twice_returns_the_name_it_already_has() {
        let mut s = Settings::default();
        let first = s.file_away(
            Resource::Software(PathBuf::from("/sw/one.ipod")),
            "mine",
            None,
        );
        let again = s.file_away(
            Resource::Software(PathBuf::from("/sw/one.ipod")),
            "something else",
            None,
        );
        assert_eq!(first, again, "the same file was filed under a second name");
        assert_eq!(s.resources.len(), 1);

        // A different thing that wants a taken name gets a suffix rather than overwriting it.
        let other = s.file_away(
            Resource::Software(PathBuf::from("/sw/two.ipod")),
            "mine",
            None,
        );
        assert_ne!(
            other, "mine",
            "a second file overwrote the first one's name"
        );
        assert_eq!(s.resources.len(), 2);

        // Disks are a separate list with the same rule, and the two do not collide: a disk called
        // "mine" and a resource called "mine" are different things in different lists.
        assert_eq!(s.file_disk(PathBuf::from("/drives/a.img"), "mine"), "mine");
        assert_eq!(s.file_disk(PathBuf::from("/drives/a.img"), "other"), "mine");
        assert_eq!(s.disks.len(), 1);
    }

    /// **The point of composing rather than copying**: edit the entry, and every machine made of it
    /// changes. If this passes with the resolution removed, machines are copies and the library is
    /// decoration.
    #[test]
    fn editing_a_library_entry_changes_the_machines_composed_of_it() {
        let mut s = Settings::default();
        // A decoy ahead of the shared one, so a resolution that ignored the name would be caught.
        s.file_away(Resource::Firmware(synth("A146", 1)), "a decoy", None);
        s.file_away(Resource::Firmware(synth("A146", 5)), "the shared ROM", None);
        s.nor = synth("A146", 5);
        s.disk = Some(PathBuf::from("/drives/one.img"));
        s.remember_as("first");
        s.remember_as("second");
        assert_eq!(s.devices[0].firmware, "the shared ROM", "not composed of it");

        // Re-seed the entry. Both machines are made of it, so both should now boot the new one.
        s.resources[1].what = Resource::Firmware(synth("A146", 99));
        for name in ["first", "second"] {
            assert!(s.run_device(name));
            assert_eq!(
                s.nor,
                synth("A146", 99),
                "{name} did not follow the entry it is made of"
            );
        }
    }

    /// A machine saved before the library existed carried its recipe inline. **Reading the file is
    /// what gives it a named iPod**, and it must boot exactly what it booted before.
    #[test]
    fn a_machine_with_no_reference_gets_one_when_the_file_is_read() {
        let mut s = Settings::parse(
            "machine.0.name = old\nmachine.0.nor_model = A146\nmachine.0.nor_seed = 7\n\
             machine.0.disk = /drives/old.img\n",
        );
        assert_eq!(s.resources.len(), 1, "the recipe was not filed: {:?}", s.resources);
        assert_eq!(
            s.devices[0].firmware, "A146, seed 7",
            "the device does not name what it boots"
        );
        assert_eq!(s.nor_of(&s.devices[0]), Some(&synth("A146", 7)));
        assert!(s.run_device("old"));
        assert_eq!(s.nor, synth("A146", 7));
        assert_eq!(s.disk, Some(PathBuf::from("/drives/old.img")));
        assert!(
            !s.missing(&s.devices[0].clone())
                .iter()
                .any(|a| matches!(a, Absent::Unlisted(_))),
            "nothing was named, so nothing is unlisted"
        );
    }

    /// The other spelling of the same file: a path rather than a recipe.
    #[test]
    fn a_device_that_carried_its_recipe_inline_becomes_a_named_ipod() {
        let s = Settings::parse("machine.0.name = old\nmachine.0.flash = /roms/mine.bin\n");
        assert_eq!(s.resources.len(), 1, "{:?}", s.resources);
        assert_eq!(s.devices[0].firmware, "mine");
        assert_eq!(
            s.nor_of(&s.devices[0]),
            Some(&crate::nor::Source::File(PathBuf::from("/roms/mine.bin")))
        );
        // And the recipe is never written back under the device.
        let text = s.render();
        assert!(text.contains("device.0.firmware = mine"), "{text}");
        assert!(!text.contains("device.0.flash"), "{text}");
        assert!(!text.contains("device.0.nor_model"), "{text}");
    }

    /// A device that was **already** composed gains nothing from the migration.
    #[test]
    fn a_device_already_pointing_at_a_named_ipod_gains_nothing() {
        let s = Settings::parse(
            "res.0.name = my dump\nres.0.kind = firmware\nres.0.path = /roms/retail.bin\n\
             device.0.name = mine\ndevice.0.firmware = my dump\n",
        );
        assert_eq!(
            s.resources.len(),
            1,
            "the migration minted a duplicate: {:?}",
            s.resources
        );
        assert_eq!(s.devices[0].firmware, "my dump");
    }

    /// A device that says nothing at all still gets the iPod it silently booted, **named**.
    #[test]
    fn a_device_that_says_nothing_still_gets_the_ipod_it_used_to_boot() {
        let s = Settings::parse("device.0.name = old\ndevice.0.chassis = black\n");
        assert_eq!(s.resources.len(), 1, "{:?}", s.resources);
        assert!(!s.devices[0].firmware.is_empty());
        assert_eq!(
            s.nor_of(&s.devices[0]),
            Some(&crate::nor::Source::default()),
            "it used to boot a generated 5.5G; it still does, and now it says so"
        );
    }

    /// An entry `parse` left unnamed — a sparse `res.N.` block — is **never handed out as a
    /// reference**, because nothing can resolve one.
    #[test]
    fn an_unnamed_leftover_is_never_handed_out_as_a_reference() {
        let mut s = Settings::parse("res.3.name = mine\nres.3.kind = firmware\n");
        assert_eq!(s.resources.len(), 4, "the sparse index did not leave fillers");
        s.remember_as("x");
        let d = s.devices[0].clone();
        assert!(!d.firmware.is_empty(), "a device was given an unusable name");
        assert!(s.nor_of(&d).is_some(), "the name resolves to nothing");
    }

    /// **And an unnamed leftover is never *restated* either**, which is the same rule at the second
    /// lookup in this file.
    ///
    /// [`Settings::restate_firmware`]'s caller is [`crate::compose`]'s window, which hands it the
    /// name the iPod on screen is filed under — and hands it `""` to mean *this one has not been
    /// filed yet, it was just minted*. A bare `position(|it| it.name == filed_as)` answers that
    /// with placeholder 0 and restates somebody's filler into the minted iPod, under a name a
    /// device is about to be pointed at. `None` is the answer, and the caller files instead.
    #[test]
    fn an_unnamed_leftover_is_never_restated_either() {
        let mut s = Settings::parse("res.3.name = mine\nres.3.kind = firmware\n");
        assert_eq!(s.resources.len(), 4, "the sparse index did not leave fillers");
        assert!(
            s.resources.iter().any(|it| it.name.is_empty()),
            "the fixture holds no unnamed entry, so what follows proves nothing"
        );
        let before = s.resources.clone();

        let minted = crate::nor::Source::Synthetic {
            model: "A446".into(),
            seed: 7,
            serial: None,
            guid: None,
            splash: None,
        };
        assert_eq!(
            s.restate_firmware("", minted),
            None,
            "an empty name adopted a placeholder"
        );
        assert_eq!(s.resources, before, "it wrote into the list anyway");
    }

    /// **A reference to an entry that is gone is reported, not swallowed.** "It boots to a white
    /// screen" is not a diagnosis; the name of the file that went is.
    #[test]
    fn a_dangling_reference_is_named() {
        let s = Settings::parse(
            "machine.0.name = broken\nmachine.0.rom = a ROM that was deleted\n\
             machine.0.drive = a drive that was deleted\n",
        );
        let missing = s.missing(&s.devices[0]);
        assert_eq!(
            missing.len(),
            2,
            "both dangling references should be named: {missing:?}"
        );
        assert!(missing.iter().any(|a| a.label().contains("ROM")));
    }

    /// The two kinds of gone are different, they come in a fixed order, and the second one names a
    /// path the first cannot.
    #[test]
    fn a_delisted_name_and_a_deleted_file_are_different_absences() {
        let dir = temp_dir("absences");
        let img = dir.join("mine.img");
        std::fs::write(&img, b"not really a drive").unwrap();

        let mut s = Settings::default();
        s.file_disk(img.clone(), "the drive");
        let d = Device {
            name: "half gone".into(),
            firmware: "a ROM that was deleted".into(),
            disk: Some("the drive".into()),
            ..Device::default()
        };
        assert_eq!(
            s.missing(&d),
            vec![Absent::Unlisted("a ROM that was deleted".into())],
            "the drive is on disk, so only the name is absent"
        );

        std::fs::remove_file(&img).unwrap();
        assert_eq!(
            s.missing(&d),
            vec![
                Absent::Unlisted("a ROM that was deleted".into()),
                Absent::Gone(img.clone()),
            ],
            "the firmware comes first and the two are different kinds of gone"
        );
        assert_eq!(s.missing(&d)[1].label(), "mine.img");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A deleted image is `Gone`, and it names the file. **The check that did not exist**: the old
    /// `missing` looked only at names, so an image deleted in Finder left the device startable and
    /// the cradle promising `about 3 s`.
    #[test]
    fn a_deleted_image_is_gone_and_names_its_path() {
        let dir = temp_dir("deleted-image");
        let img = dir.join("my-5.5g.img");
        std::fs::write(&img, b"x").unwrap();

        let mut s = Settings::default();
        s.file_away(Resource::Firmware(synth("A146", 5)), "an iPod", None);
        s.file_disk(img.clone(), "my drive");
        let d = Device {
            name: "mine".into(),
            firmware: "an iPod".into(),
            disk: Some("my drive".into()),
            ..Device::default()
        };
        assert!(s.missing(&d).is_empty(), "{:?}", s.missing(&d));

        std::fs::remove_file(&img).unwrap();
        assert_eq!(s.missing(&d), vec![Absent::Gone(img.clone())]);
        assert_eq!(s.missing(&d)[0].label(), "my-5.5g.img");
        assert_eq!(s.missing(&d)[0].path(), Some(img.as_path()));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A recipe is not a path, so it is never absent — however hard anybody stats.
    #[test]
    fn a_synthesised_rom_is_never_absent() {
        let mut s = Settings::default();
        s.file_away(Resource::Firmware(synth("A146", 5)), "a recipe", None);
        let d = Device {
            firmware: "a recipe".into(),
            ..Device::default()
        };
        assert!(s.missing(&d).is_empty(), "{:?}", s.missing(&d));
    }

    /// A device that names no disk at all is **unfinished**, not broken.
    #[test]
    fn a_device_with_no_disk_is_not_missing_one() {
        let mut s = Settings::default();
        s.file_away(Resource::Firmware(synth("A146", 5)), "a recipe", None);
        let d = Device {
            firmware: "a recipe".into(),
            disk: None,
            disk_path: None,
            ..Device::default()
        };
        assert!(s.missing(&d).is_empty(), "{:?}", s.missing(&d));
    }

    /// The absence label names the **file**, not the path — the cradle has 24 px of centred body
    /// text for it — and a path with no file name falls back to something rather than nothing.
    #[test]
    fn an_absence_names_the_file_not_the_path() {
        let p = PathBuf::from("/some where/My iPod Backups/my-5.5g.img");
        let gone = Absent::Gone(p.clone());
        assert_eq!(gone.label(), "my-5.5g.img");
        assert_eq!(gone.path(), Some(p.as_path()));

        let unlisted = Absent::Unlisted("a ROM that was deleted".into());
        assert_eq!(unlisted.label(), "a ROM that was deleted");
        assert_eq!(unlisted.path(), None);

        assert!(
            !Absent::Gone(PathBuf::from("/")).label().is_empty(),
            "a path with no file name produced an empty label"
        );
    }

    /// The cache answers from memory for the length of one pass, and a fresh one sees the world.
    #[test]
    fn the_presence_cache_answers_from_memory_within_one_pass() {
        let dir = temp_dir("presence");
        let f = dir.join("here.img");
        std::fs::write(&f, b"x").unwrap();

        let mut seen = Presence::new();
        assert!(seen.exists(&f));
        std::fs::remove_file(&f).unwrap();
        assert!(
            seen.exists(&f),
            "the pass re-stat'ed a path it had already answered"
        );
        assert!(
            !Presence::new().exists(&f),
            "a fresh pass did not see the deletion"
        );
        seen.forget(&f);
        assert!(!seen.exists(&f), "`forget` did not forget");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A path we cannot stat is not a path we observed to be absent.**
    ///
    /// `Path::exists()` folds every error into `false`, so a directory the user cannot traverse
    /// would be reported as "the disk is not where it was" — the program asserting a fact about
    /// somebody's filesystem that it did not observe.
    #[cfg(unix)]
    #[test]
    fn a_path_we_cannot_stat_is_not_reported_as_gone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("eacces");
        let locked = dir.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        let f = locked.join("mine.img");
        std::fs::write(&f, b"x").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

        // Running as root ignores the mode bits, and a test that cannot create the condition it
        // measures must say so rather than assert on it.
        let enforced = std::fs::read_dir(&locked).is_err();
        let answer = Presence::new().exists(&f);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        if !enforced {
            return;
        }
        assert!(
            answer,
            "a permission error was reported as an observation of absence"
        );
    }

    /// **The other half of the same rule: an error that *is* a statement of absence reads as one.**
    ///
    /// The permission arm above is right, and a catch-all `Err(_) => true` beside it swallows the
    /// errors that are definite negatives. A path whose parent component is a regular file gives
    /// `ENOTDIR`; a path with an interior NUL gives `InvalidFilename`. Nothing can exist at either,
    /// so calling them present hides a device that cannot start and leaves the cradle saying
    /// nothing at all.
    #[test]
    fn an_impossible_path_is_gone_rather_than_unreadable() {
        let dir = temp_dir("enotdir");
        let file = dir.join("not-a-directory");
        std::fs::write(&file, b"x").unwrap();

        let mut seen = Presence::new();
        assert!(seen.exists(&file), "the file itself is there");
        assert!(
            !seen.exists(&file.join("mine.img")),
            "a path under a regular file was reported as present"
        );
        // A filesystem cannot hold this name at all, on any platform.
        assert!(
            !seen.exists(Path::new("no\0such.img")),
            "a path no filesystem can hold was reported as present"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A symlink pointing at nothing is gone, and one pointing at something is not.
    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_is_gone_not_present() {
        let dir = temp_dir("symlink");
        let real = dir.join("real.img");
        std::fs::write(&real, b"x").unwrap();
        let good = dir.join("good.img");
        let bad = dir.join("bad.img");
        std::os::unix::fs::symlink(&real, &good).unwrap();
        std::os::unix::fs::symlink(dir.join("never existed"), &bad).unwrap();

        assert!(Presence::new().exists(&good));
        assert!(
            !Presence::new().exists(&bad),
            "a symlink to a deleted image read as present"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// An `.ipsw` is not something that can be run — a drive is *built* from it. Applying one as if
    /// it were a drive would produce a machine that boots nothing, silently.
    #[test]
    fn a_disk_is_not_a_resource_and_an_installer_is_not_software() {
        let mut s = Settings::default();
        s.file_away(
            Resource::Installer(PathBuf::from("/fw/iPod_20.1.3.ipsw")),
            "20.1.3",
            None,
        );
        s.file_away(
            Resource::Software(PathBuf::from("/sw/rockbox.ipod")),
            "Rockbox 4.0",
            None,
        );
        s.file_disk(PathBuf::from("/drives/one.img"), "a drive");

        // **The lists are what stop the wrong thing being aimed at the wrong place.** An `.ipsw`
        // and a Rockbox build are both files in `resources`, but they carry different verbs; a
        // drive is not in that list at all, because it is what the other two are combined into.
        assert_eq!(s.resources.len(), 2);
        assert_eq!(s.disks.len(), 1);
        let verbs: Vec<&str> = s.resources.iter().map(|i| i.what.verb()).collect();
        assert_eq!(verbs, ["makes a disk", "installs onto a disk"]);
        assert!(
            !s.resources
                .iter()
                .any(|i| i.what.path().unwrap().ends_with("one.img")),
            "a drive was filed as a resource"
        );
    }

    /// A settings file from the shape where a disk *was* a resource has to come back with the disk
    /// in the disks — otherwise updating empties the list of drives somebody is running.
    #[test]
    fn an_older_file_that_filed_disks_as_resources_migrates_them() {
        let s = Settings::parse(
            "item.0.name = my dump\nitem.0.kind = rom\nitem.0.path = /roms/retail.bin\n\
             item.1.name = ipod8g\nitem.1.kind = disk\nitem.1.path = /drives/ipod8g.img\n\
             machine.0.name = old\nmachine.0.drive = ipod8g\n",
        );
        assert!(
            !s.resources
                .iter()
                .any(|i| i.what.path().is_some_and(|p| p.ends_with("ipod8g.img"))),
            "the disk stayed in the resources: {:?}",
            s.resources
        );
        // **An exact count, not just an absence.** The dump, plus the generated 5.5G the migration
        // mints for `machine.0`, which names a drive and no iPod — §20 item 1's stated behaviour:
        // a device that said nothing at all is given, in a list, the ROM it used to boot silently.
        // Asserting only that the drive left would let a third entry appear unnoticed.
        assert_eq!(
            s.resources
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>(),
            vec!["my dump", "A446, seed 0"],
            "{:?}",
            s.resources
        );
        assert_eq!(s.devices[0].firmware, "A446, seed 0");
        assert_eq!(s.disks.len(), 1, "the disk did not arrive in the disks");
        assert_eq!(s.disks[0].name, "ipod8g");
        assert_eq!(s.disks[0].path, PathBuf::from("/drives/ipod8g.img"));
        // And the device that referred to it by name still resolves, which is the whole point of
        // moving it rather than dropping it.
        assert_eq!(s.devices[0].disk.as_deref(), Some("ipod8g"));
        // The drive image itself is fictional, so it *is* `Gone`. The subject here is that the
        // **name** survived the move, which is the `Unlisted` half.
        assert!(
            !s.missing(&s.devices[0])
                .iter()
                .any(|a| matches!(a, Absent::Unlisted(_))),
            "the reference broke in the move: {:?}",
            s.missing(&s.devices[0])
        );
    }

    /// **A setup that predates this is not an empty list.** Someone who has been running a boot ROM
    /// and a drive for months should open the page and see them, not "nothing yet".
    ///
    /// The per-device half is proved through `parse` now — a device names its iPod and has no
    /// recipe of its own to gather. What is left here is the live machine and the drives.
    #[test]
    fn the_lists_seed_themselves_from_what_is_already_configured() {
        let mut s = Settings::parse(
            "flash = /roms/retail.bin\ndisk = /drives/mine.img\n\
             device.0.name = the one I use\ndevice.0.nor_model = A146\ndevice.0.nor_seed = 5\n\
             device.0.disk = /drives/mine.img\n",
        );
        assert!(s.seed_resources(), "seeding said it changed nothing");
        assert_eq!(s.resources.len(), 2, "two boot ROMs: {:?}", s.resources);
        assert_eq!(s.disks.len(), 1, "one drive: {:?}", s.disks);
        assert!(s
            .resources
            .iter()
            .any(|i| i.what == Resource::Firmware(synth("A146", 5))));
        assert!(s.resources.iter().any(|i| i.what
            == Resource::Firmware(crate::nor::Source::File(PathBuf::from(
                "/roms/retail.bin"
            )))));
        assert!(s.disks[0].path.ends_with("mine.img"));
        assert!(
            !s.seed_resources(),
            "seeding ran twice and said it had changed something"
        );
    }

    /// **Seeding happens once, so removing an entry sticks.** A list that puts back what you took
    /// out at the next launch is a list you cannot edit.
    ///
    /// The fixture carries a device on purpose: it is what makes this the guard against `parse`'s
    /// own post-pass re-filing a ROM somebody removed.
    #[test]
    fn removing_the_last_entry_is_not_undone_by_the_next_launch() {
        let mut s = Settings {
            nor: synth("A146", 5),
            ..Default::default()
        };
        s.remember_as("mine");
        s.seed_resources();
        assert_eq!(s.resources.len(), 1);

        s.resources.clear();
        let mut back = Settings::parse(&s.render());
        assert!(back.library_seeded, "the marker did not survive the file");
        assert!(
            back.resources.is_empty(),
            "reading the file put back an entry that was removed: {:?}",
            back.resources
        );
        back.seed_resources();
        assert!(
            back.resources.is_empty(),
            "an entry that was removed came back"
        );
    }

    /// **What seeding produced is written back — by the caller that asked for it, and by nothing
    /// else.** Two halves, and both of them are the point:
    ///
    /// - [`Settings::load_and_seed`] persists it, or a removal cannot stick: the marker that says
    ///   "this has been seeded" lived only in memory, so the next launch re-derived the list a
    ///   person had just edited.
    /// - [`Settings::load`] does not, because `ipod-boot` calls it from five places that only want
    ///   to know the default drive, and one of them is `--print`, documented as running nothing.
    ///   A save from there rewrote the operator's file — and `render` is generated from the model,
    ///   so every comment they had added went with it.
    ///
    /// The fixture is an existing installation's file — one written before the library existed, so
    /// it has a `flash =` line and no `library_seeded` marker. That is the only shape this can
    /// happen to, and it is the shape every current installation has. The comment in it is the
    /// thing a plain read must not eat.
    #[test]
    fn what_seeding_produced_is_written_back_so_a_removal_can_stick() {
        let dir = temp_dir("seed-writeback");
        let _guard = env_lock();
        let before = std::env::var_os("IPOD_EMULATOR_DATA");
        // SAFETY: `env_lock` serialises every test in this crate that touches this variable, and
        // the previous value is restored before the guard is dropped.
        unsafe { std::env::set_var("IPOD_EMULATOR_DATA", &dir) };

        let fixture = "# MY OWN NOTE: this is the dump off the 5.5G in the drawer.\n\
                       mode = user\nflash = /roms/mine.bin\n";
        std::fs::write(dir.join(FILE), fixture).unwrap();

        // A plain read seeds in memory and touches nothing on disk.
        let read_only = Settings::load();
        assert!(read_only.library_seeded, "a plain read still seeds in memory");
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE)).unwrap(),
            fixture,
            "reading the settings rewrote them"
        );

        let first = Settings::load_and_seed();
        assert!(first.library_seeded);
        assert_eq!(first.resources.len(), 1, "{:?}", first.resources);
        let written = std::fs::read_to_string(dir.join(FILE))
            .expect("the settings file went missing");
        assert!(
            written.contains("library_seeded = true"),
            "the write-back did not persist what it seeded: {written}"
        );
        assert!(written.contains("res.0.kind = firmware"), "{written}");

        // Remove it and save, the way a Parts remove will. The next load must leave it removed.
        let mut edited = first;
        edited.resources.clear();
        edited.save().expect("save");
        let again = Settings::load_and_seed();
        assert!(
            again.resources.is_empty(),
            "an entry that was removed came back on the next launch: {:?}",
            again.resources
        );

        // And reading is never a reason to create a file: `migrate_legacy` declines as soon as one
        // exists here, so a `load` that minted one would block a carry-forward for ever.
        std::fs::remove_file(dir.join(FILE)).unwrap();
        Settings::load_and_seed();
        assert!(
            !dir.join(FILE).exists(),
            "reading settings that do not exist created a settings file"
        );
        // Nor a half-written one: `save` goes through a `.part` and a rename, so an interrupted
        // write leaves the old file rather than a truncated device list.
        assert!(
            !dir.join(format!("{FILE}.part")).exists(),
            "a .part file survived a completed save"
        );

        match before {
            Some(v) => unsafe { std::env::set_var("IPOD_EMULATOR_DATA", v) },
            None => unsafe { std::env::remove_var("IPOD_EMULATOR_DATA") },
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    // ------------------------------------------------------------------- parking

    /// A park time survives the file, and a file written before the field has none rather than a
    /// device that claims to have been parked at the epoch.
    #[test]
    fn a_park_time_survives_the_file_and_an_older_file_has_none() {
        let mut s = Settings {
            nor: synth("A146", 5),
            ..Default::default()
        };
        s.remember_as("mine");
        assert!(s.record_park("mine", 1_755_738_000));
        let back = Settings::parse(&s.render());
        assert_eq!(back.devices[0].parked_at, Some(1_755_738_000));

        assert_eq!(
            Settings::parse("device.0.name = old\n").devices[0].parked_at,
            None,
            "a file that never said reads as never parked"
        );
        assert_eq!(
            Settings::parse("device.0.name = old\ndevice.0.parked_at = nonsense\n").devices[0]
                .parked_at,
            None,
            "an unreadable value is `nobody said`, not `parked at the epoch`"
        );
    }

    /// Parking stamps the time, discarding clears it, and neither invents a device.
    #[test]
    fn parking_stamps_the_time_and_discarding_clears_it() {
        let mut s = Settings {
            nor: synth("A146", 5),
            ..Default::default()
        };
        s.remember_as("a");
        assert!(s.record_park("a", 1_000));
        assert_eq!(s.devices[0].parked_at, Some(1_000));
        assert!(s.discard_park("a"));
        assert_eq!(s.devices[0].parked_at, None);

        assert!(!s.record_park("nope", 1));
        assert!(!s.discard_park("nope"));
        assert_eq!(s.devices.len(), 1, "a park invented a device");
    }

    /// A clock that went backwards reads as just now, not as 584 942 417 355 years ago.
    #[test]
    fn a_clock_that_went_backwards_reads_as_just_now() {
        let parked = Device {
            parked_at: Some(1_000),
            ..Device::default()
        };
        assert_eq!(parked_for(&parked, 1_240), Some(240));
        assert_eq!(parked_for(&parked, 500), Some(0));
        assert_eq!(
            parked_for(&Device::default(), 500),
            None,
            "never parked is not the same as parked a moment ago"
        );
        assert!(now_unix() > 1_700_000_000, "the clock reads before 2023");
    }

    // ------------------------------------------------------------------- provenance

    /// **§11.4's rule, mechanically**: only a SHA-256 match ever renders as verified.
    #[test]
    fn a_size_only_row_never_says_verified() {
        for p in [
            Provenance::Dumped,
            Provenance::Synthesised { seed: 5 },
            Provenance::Fetched {
                verified: Verification::Sha256,
            },
            Provenance::Fetched {
                verified: Verification::SizeOnly,
            },
            Provenance::Fetched {
                verified: Verification::None,
            },
            Provenance::Provided,
            Provenance::Built,
        ] {
            assert_eq!(
                p.line().contains("verified"),
                p.is_verified(),
                "{p:?} renders {:?} but is_verified() is {}",
                p.line(),
                p.is_verified()
            );
        }
        // **And the one that does say `verified` says WHEN.** A provenance is stored against a
        // path with no digest, size or mtime beside it, so replacing the file underneath it leaves
        // the claim standing and nothing can refute it. A present-tense `fetched — SHA-256
        // verified` is then the `fetched and verified` string literal again, one level down.
        let sha = Provenance::Fetched {
            verified: Verification::Sha256,
        };
        assert!(
            sha.line().contains("arrived"),
            "the verified row reads as a live fact about the bytes on disk: {:?}",
            sha.line()
        );
        assert!(Provenance::Fetched {
            verified: Verification::Sha256
        }
        .is_verified());
        assert!(!Provenance::Fetched {
            verified: Verification::SizeOnly
        }
        .is_verified());
    }

    /// An unreadable token is not a claim, and it degrades toward saying nothing.
    #[test]
    fn an_unknown_provenance_token_is_not_a_claim() {
        for junk in [
            "",
            "verified",
            "fetched-md5",
            "FETCHED-SHA256",
            "sha256",
            "  ",
            "dumped from a real iPod",
        ] {
            assert_eq!(Provenance::parse(junk), None, "{junk:?} was read as a claim");
        }
        assert_eq!(
            Provenance::parse("fetched"),
            Some(Provenance::Fetched {
                verified: Verification::None
            })
        );
    }

    /// The writer and the reader cannot drift, because the round trip is a test.
    #[test]
    fn every_token_round_trips_through_parse() {
        for p in [
            Provenance::Dumped,
            Provenance::Synthesised { seed: 0 },
            Provenance::Fetched {
                verified: Verification::Sha256,
            },
            Provenance::Fetched {
                verified: Verification::SizeOnly,
            },
            Provenance::Fetched {
                verified: Verification::None,
            },
            Provenance::Provided,
            Provenance::Built,
        ] {
            assert_eq!(Provenance::parse(p.token()), Some(p), "{p:?}");
        }
    }

    /// **The invariant is total and unforgeable**: a synthesised ROM's provenance is its recipe's
    /// seed, whatever the caller or the file said.
    #[test]
    fn a_synthesised_rom_is_always_synthesised_provenance() {
        let mut s = Settings::default();
        let name = s.file_away(
            Resource::Firmware(synth("A146", 5)),
            "x",
            Some(Provenance::Dumped),
        );
        assert_eq!(
            s.resources[0].from,
            Some(Provenance::Synthesised { seed: 5 }),
            "a caller was allowed to state a synthetic ROM's provenance"
        );
        assert!(s.record_provenance(&name, Provenance::Provided));
        assert_eq!(
            s.resources[0].from,
            Some(Provenance::Synthesised { seed: 5 }),
            "`record_provenance` was allowed to overrule the recipe"
        );

        // And through the file, with the key on either side of the recipe.
        for text in [
            "res.0.name = x\nres.0.kind = firmware\nres.0.provenance = provided\n\
             res.0.nor_model = A146\nres.0.nor_seed = 7\n",
            "res.0.name = x\nres.0.kind = firmware\nres.0.nor_model = A146\n\
             res.0.nor_seed = 7\nres.0.provenance = provided\n",
        ] {
            assert_eq!(
                Settings::parse(text).resources[0].from,
                Some(Provenance::Synthesised { seed: 7 }),
                "a hand-written provenance survived on a synthetic recipe: {text}"
            );
        }
    }

    /// `synthesised` on something that is not a recipe is nonsense, and nonsense says nothing
    /// rather than inventing a seed of zero.
    #[test]
    fn a_synthesised_token_on_a_file_is_not_a_claim() {
        let s = Settings::parse(
            "res.0.name = x\nres.0.kind = installer\nres.0.path = /fw/x.ipsw\n\
             res.0.provenance = synthesised\n",
        );
        assert_eq!(s.resources[0].from, None);
    }

    /// **The file shape every existing installation has**: no `provenance` key at all, and the row
    /// says nothing rather than lying.
    #[test]
    fn an_older_settings_file_says_nothing_rather_than_lying() {
        let s = Settings::parse(
            "res.0.name = my dump\nres.0.kind = firmware\nres.0.path = /roms/retail.bin\n",
        );
        assert_eq!(s.resources[0].from, None);
        let text = s.render();
        assert!(
            !text.contains("res.0.provenance"),
            "`nobody said` was written as a line: {text}"
        );
    }

    /// `None` may become stated; a stated value is never changed by filing. `record_provenance` is
    /// the second verb, for the caller that has just re-checked the bytes.
    #[test]
    fn filing_a_file_twice_fills_an_unknown_and_never_overwrites_a_stated_one() {
        let mut s = Settings::default();
        let p = Resource::Software(PathBuf::from("/sw/one.ipod"));
        let name = s.file_away(p.clone(), "mine", None);
        assert_eq!(s.resources[0].from, None);

        s.file_away(
            p.clone(),
            "mine",
            Some(Provenance::Fetched {
                verified: Verification::Sha256,
            }),
        );
        assert_eq!(
            s.resources[0].from,
            Some(Provenance::Fetched {
                verified: Verification::Sha256
            }),
            "`nobody said` was not filled in by a caller that knew"
        );

        s.file_away(p.clone(), "mine", Some(Provenance::Provided));
        assert_eq!(
            s.resources[0].from,
            Some(Provenance::Fetched {
                verified: Verification::Sha256
            }),
            "filing overwrote a recorded fact"
        );
        assert_eq!(s.resources.len(), 1);

        assert!(s.record_provenance(&name, Provenance::Provided));
        assert_eq!(s.resources[0].from, Some(Provenance::Provided));
        assert!(!s.record_provenance("no such name", Provenance::Provided));
    }

    /// Seeding states nothing about a dump it never watched being made, and everything about a
    /// recipe, because the recipe says it.
    #[test]
    fn a_seeded_dump_states_nothing_and_a_seeded_recipe_states_the_seed() {
        let mut s = Settings::parse(
            "flash = /roms/retail.bin\n\
             device.0.name = mine\ndevice.0.nor_model = A146\ndevice.0.nor_seed = 5\n",
        );
        s.seed_resources();
        let dump = s
            .resources
            .iter()
            .find(|i| matches!(&i.what, Resource::Firmware(crate::nor::Source::File(_))))
            .expect("the dump");
        assert_eq!(dump.from, None, "a dump was claimed as `Dumped`");
        let recipe = s
            .resources
            .iter()
            .find(|i| i.what == Resource::Firmware(synth("A146", 5)))
            .expect("the recipe");
        assert_eq!(recipe.from, Some(Provenance::Synthesised { seed: 5 }));
    }

    /// Every provenance survives the file, and the one that is derived is written nowhere.
    #[test]
    fn provenance_survives_render_and_parse() {
        let mut s = Settings::default();
        for (i, p) in [
            Some(Provenance::Dumped),
            Some(Provenance::Fetched {
                verified: Verification::Sha256,
            }),
            Some(Provenance::Fetched {
                verified: Verification::SizeOnly,
            }),
            Some(Provenance::Fetched {
                verified: Verification::None,
            }),
            Some(Provenance::Provided),
            Some(Provenance::Built),
            None,
        ]
        .into_iter()
        .enumerate()
        {
            s.file_away(
                Resource::Software(PathBuf::from(format!("/sw/{i}.ipod"))),
                &format!("sw {i}"),
                p,
            );
        }
        s.file_away(Resource::Firmware(synth("A146", 5)), "a recipe", None);

        let text = s.render();
        // The literal lines, not only the round trip: a render that drops the key for everything
        // and a parse that ignores it for everything would cancel out.
        assert!(text.contains("res.0.provenance = dumped"), "{text}");
        assert!(text.contains("res.1.provenance = fetched-sha256"), "{text}");
        assert!(text.contains("res.2.provenance = fetched-size"), "{text}");
        assert!(text.contains("res.3.provenance = fetched\n"), "{text}");
        assert!(text.contains("res.4.provenance = provided"), "{text}");
        assert!(text.contains("res.5.provenance = built"), "{text}");
        assert!(!text.contains("res.6.provenance"), "{text}");
        assert!(
            !text.contains("res.7.provenance"),
            "a synthetic ROM's seed was written twice: {text}"
        );

        let back = Settings::parse(&text);
        assert_eq!(back.resources, s.resources);
        assert_eq!(
            back.resources[7].from,
            Some(Provenance::Synthesised { seed: 5 }),
            "the derived provenance did not come back"
        );
    }

    /// The new key adds no order dependency to a file that already has one.
    #[test]
    fn a_reordered_hand_edit_still_reads_the_same() {
        let want = Item {
            name: "20.1.3".into(),
            what: Resource::Installer(PathBuf::from("/fw/iPod_20.1.3.ipsw")),
            from: Some(Provenance::Fetched {
                verified: Verification::SizeOnly,
            }),
        };
        for text in [
            "res.0.name = 20.1.3\nres.0.kind = installer\nres.0.path = /fw/iPod_20.1.3.ipsw\n\
             res.0.provenance = fetched-size\n",
            "res.0.provenance = fetched-size\nres.0.name = 20.1.3\nres.0.kind = installer\n\
             res.0.path = /fw/iPod_20.1.3.ipsw\n",
            "res.0.name = 20.1.3\nres.0.kind = installer\nres.0.provenance = fetched-size\n\
             res.0.path = /fw/iPod_20.1.3.ipsw\n",
        ] {
            assert_eq!(Settings::parse(text).resources[0], want, "{text}");
        }
    }

    /// A settings file **never** writes a recipe under a device — writing both a reference and a
    /// resolved copy is how the two came to disagree.
    #[test]
    fn a_settings_file_never_writes_a_recipe_under_a_device() {
        let mut s = Settings {
            nor: synth("A146", 5),
            ..Default::default()
        };
        s.remember_as("mine");
        let text = s.render();
        assert!(text.contains("device.0.firmware = A146, seed 5"), "{text}");
        assert!(!text.contains("device.0.flash"), "{text}");
        assert!(!text.contains("device.0.nor_model"), "{text}");
        assert!(!text.contains("device.0.nor_seed"), "{text}");
    }

    /// **Where a device came from survives the file, and survives `as_device`.**
    ///
    /// It is the one thing separating a device the Composer filed from the first run's own — both
    /// carry a synthesised boot ROM with a minted seed — and the consequence of losing it is not
    /// cosmetic: the window reads a composed device as a half-made first run and offers to finish
    /// it by running the fixed first-run plan, which builds Apple's firmware onto an 8 GiB drive
    /// whatever the recipe said. A field that is written and not read back is that lie with a
    /// delay on it.
    ///
    /// Both halves are checked, because they fail differently: the file, and the
    /// `run_device`/`remember_as` round trip through [`Settings::as_device`] — which is the trap
    /// §20 item 6 names for `boot_shape`, and this field is reachable by the same route.
    #[test]
    fn where_a_device_came_from_survives_a_round_trip() {
        let mut s = a_device_called("Rockbox only");
        s.devices[0].composed = true;

        let text = s.render();
        assert!(
            text.contains("device.0.composed = true"),
            "the file does not say where the device came from:\n{text}"
        );
        let back = Settings::parse(&text);
        assert!(back.devices[0].composed, "it did not come back");

        // A device nobody composed says nothing rather than saying `false`, and reads back `false`.
        let plain = a_device_called("My 5.5G");
        let plain_text = plain.render();
        assert!(
            !plain_text.contains("composed"),
            "every device states the default:\n{plain_text}"
        );
        assert!(!Settings::parse(&plain_text).devices[0].composed);

        // And the in-memory round trip: a save from anywhere but the Composer must not re-file a
        // composed device as the first run's.
        let mut live = back;
        assert!(live.run_device("Rockbox only"), "the fixture does not resolve");
        live.remember_as("Rockbox only");
        assert!(
            live.devices[0].composed,
            "`remember_as` handed a composed device back to the first run"
        );
    }

    // ── §12.3 / §20 item 6: what a device boots, and the number measured on it ──────────────────

    fn shape(loader: crate::compose::Loader, oses: &[crate::compose::Os]) -> crate::compose::BootShape {
        crate::compose::BootShape {
            loader,
            oses: oses.iter().copied().collect(),
        }
    }

    fn a_device_called(name: &str) -> Settings {
        let mut s = Settings {
            nor: synth("A446", 3),
            ..Default::default()
        };
        s.remember_as(name);
        s
    }

    /// **§20 item 6's named trap, at the file.** The number and the shape it was measured on travel
    /// together or neither is worth storing — a round trip that keeps one and loses the other
    /// leaves `set_boot_shape` comparing a good denominator against nothing and dropping it.
    #[test]
    fn a_devices_boot_shape_survives_a_round_trip_through_the_settings_file() {
        let mut s = a_device_called("My 5.5G");
        let sh = shape(
            crate::compose::Loader::Rockbox,
            &[crate::compose::Os::Apple, crate::compose::Os::Rockbox],
        );
        assert!(s.set_boot_shape("My 5.5G", &sh));
        s.devices[0].boot_instructions = Some(1_600_000_000);

        let text = s.render();
        assert!(
            text.contains("device.0.boot_shape = rockbox, apple, rockbox"),
            "the shape is not in the file at all:\n{text}"
        );
        let back = Settings::parse(&text);
        assert_eq!(back.devices[0].boot_shape.as_deref(), Some("rockbox, apple, rockbox"));
        assert_eq!(back.devices[0].boot_instructions, Some(1_600_000_000));
        // The file is the only spelling: what came back parses to the shape that went in.
        assert_eq!(
            back.devices[0]
                .boot_shape
                .as_deref()
                .and_then(crate::compose::BootShape::parse),
            Some(sh)
        );
    }

    /// **The same trap one layer in, and this is the layer it was named at.** `as_device` rebuilds
    /// a device from the live fields on every `run_device` and every `remember_as`; without the
    /// carry-forward line the shape is dropped there, silently, in memory, and never reaches the
    /// file for the test above to catch.
    #[test]
    fn as_device_carries_the_boot_shape_it_was_given() {
        let mut s = a_device_called("My 5.5G");
        assert!(s.set_boot_shape("My 5.5G", &shape(crate::compose::Loader::Apple, &[crate::compose::Os::Apple])));
        let rebuilt = s.as_device("My 5.5G");
        assert_eq!(
            rebuilt.boot_shape.as_deref(),
            Some("apple, apple"),
            "a round trip through `as_device` lost the shape"
        );
        // And through the two functions that call it.
        s.remember_as("My 5.5G");
        assert_eq!(s.devices[0].boot_shape.as_deref(), Some("apple, apple"));
        assert!(s.run_device("My 5.5G"));
        s.remember_as("My 5.5G");
        assert_eq!(s.devices[0].boot_shape.as_deref(), Some("apple, apple"));
    }

    /// **A denominator is honest only about the thing it was measured on.**
    ///
    /// Three cases in order, and the middle one is the half that is easy to lose: the first shape a
    /// device ever gets drops the number (it was measured on something nobody recorded), the same
    /// shape twice keeps it, and a real change drops it.
    #[test]
    fn installing_rockbox_over_apple_drops_the_boot_denominator() {
        let mut s = a_device_called("My 5.5G");
        let apple = shape(crate::compose::Loader::Apple, &[crate::compose::Os::Apple]);
        let rockbox = shape(
            crate::compose::Loader::Rockbox,
            &[crate::compose::Os::Apple, crate::compose::Os::Rockbox],
        );

        s.devices[0].boot_instructions = Some(1_600_000_000);
        assert!(s.set_boot_shape("My 5.5G", &apple));
        assert_eq!(
            s.devices[0].boot_instructions, None,
            "a number measured on a shape nobody recorded cannot be vouched for"
        );

        s.devices[0].boot_instructions = Some(1_600_000_000);
        assert!(s.set_boot_shape("My 5.5G", &rockbox));
        assert_eq!(
            s.devices[0].boot_instructions, None,
            "RetailOS's 1.6 G is not Rockbox's 100 M, and a bar built on it reads 6 % when the \
             machine is finished"
        );
        assert!(!s.set_boot_shape("nobody", &rockbox), "a device that does not exist");
    }

    /// The control for the test above: **re-saving a device you did not change must not cost it a
    /// boot without a bar.**
    #[test]
    fn re_saving_an_unchanged_recipe_keeps_the_denominator() {
        let mut s = a_device_called("My 5.5G");
        let rockbox = shape(
            crate::compose::Loader::Rockbox,
            &[crate::compose::Os::Apple, crate::compose::Os::Rockbox],
        );
        assert!(s.set_boot_shape("My 5.5G", &rockbox));
        s.devices[0].boot_instructions = Some(101_000_000);
        assert!(s.set_boot_shape("My 5.5G", &rockbox));
        assert_eq!(s.devices[0].boot_instructions, Some(101_000_000));
    }

    // ── §11.2's Edit mode: a device, as a recipe ────────────────────────────────────────────────

    /// **What a device is, read back as what would compose it.**
    #[test]
    fn a_device_round_trips_through_a_recipe_and_back() {
        let mut s = Settings {
            nor: synth("A446", 3),
            ..Default::default()
        };
        s.file_disk(PathBuf::from("/drives/mine.img"), "mine");
        s.disk = Some(PathBuf::from("/drives/mine.img"));
        s.remember_as("My 5.5G");
        let sh = shape(
            crate::compose::Loader::Rockbox,
            &[crate::compose::Os::Apple, crate::compose::Os::Rockbox],
        );
        assert!(s.set_boot_shape("My 5.5G", &sh));

        let r = s.recipe_of(&s.devices[0]);
        assert_eq!(
            r.start,
            crate::compose::Start::FromDisk {
                name: "mine".into(),
                fat_type: None
            },
            "a device that names a library drive references it — it does not propose to build a \
             second one"
        );
        assert_eq!(r.shape(), sh, "the recipe boots something else than the device does");
    }

    /// **A device that names nothing is unfinished, not broken** — so it opens on `nothing chosen`
    /// rather than on an error the caller has to invent a page for.
    #[test]
    fn an_unfinished_device_opens_the_composer_on_nothing_chosen() {
        let d = Device {
            name: "half made".into(),
            firmware: "A446, seed 3".into(),
            ..Default::default()
        };
        let r = Settings::default().recipe_of(&d);
        assert!(r.nothing_chosen(), "{r:?}");
        assert!(!r.check().ok(), "a recipe with nothing chosen must not claim a plan");
    }

    /// **A drive filed before shapes existed opens as what is on it**, not as Apple-by-default.
    #[test]
    fn a_device_with_no_recorded_shape_reads_the_drives_own_install_list() {
        let mut s = Settings::default();
        s.file_disk(PathBuf::from("/drives/mine.img"), "mine");
        s.disks[0].installed = vec![
            crate::compose::Os::Apple.label().to_string(),
            crate::compose::Os::Rockbox.label().to_string(),
        ];
        let d = Device {
            name: "old".into(),
            firmware: "whatever".into(),
            disk: Some("mine".into()),
            ..Default::default()
        };
        let r = s.recipe_of(&d);
        assert!(r.oses.contains(&crate::compose::Os::Rockbox), "{r:?}");
        assert_eq!(
            r.loader,
            crate::compose::Loader::Rockbox,
            "the bootloader follows the systems when nobody recorded one"
        );
    }

    // ── §11.4: renaming, restating, and what `used by N` counts ─────────────────────────────────

    /// The name is the key, so **`current` moves with it** or the running device stops being in the
    /// list the moment it is renamed.
    #[test]
    fn a_rename_moves_current_with_it() {
        let mut s = a_device_called("My 5.5G");
        assert_eq!(s.current.as_deref(), Some("My 5.5G"));
        assert!(s.rename_device("My 5.5G", "The black one"));
        assert_eq!(s.devices[0].name, "The black one");
        assert_eq!(s.current.as_deref(), Some("The black one"));
        assert!(
            s.rename_device("The black one", "The black one"),
            "renaming a device to the name it has is not a failure"
        );
        assert!(!s.rename_device("The black one", "   "), "an empty name is refused");
        assert!(!s.rename_device("nobody", "anything"), "a device that does not exist");
    }

    /// Two devices wearing one name is a list where `run_device`, `forget` and `remember_as` each
    /// pick whichever they find first.
    #[test]
    fn a_rename_onto_a_taken_name_is_refused() {
        let mut s = a_device_called("one");
        s.remember_as("two");
        assert_eq!(s.devices.len(), 2);
        assert!(!s.rename_device("one", "two"));
        assert_eq!(s.devices[0].name, "one", "the refusal renamed it anyway");
    }

    /// **Editing one iPod changes every device made of it**, which is the point of composing rather
    /// than copying — and `restate_firmware` is what keeps the references pointing at it.
    #[test]
    fn restating_an_ipod_repoints_every_device_made_of_it() {
        let mut s = a_device_called("one");
        s.remember_as("two");
        assert_eq!(s.resources.len(), 1, "both devices are made of one iPod");
        let filed = s.devices[0].firmware.clone();
        assert_eq!(s.devices_using_resource(&filed), vec!["one", "two"]);

        let now = s
            .restate_firmware(&filed, synth("A446", 9))
            .expect("a filed boot ROM restates");
        assert_eq!(now, "A446, seed 9");
        assert_eq!(s.resources.len(), 1, "restating filed a second iPod");
        assert!(
            s.devices.iter().all(|d| d.firmware == now),
            "a device was cut loose from the iPod it is made of: {:?}",
            s.devices.iter().map(|d| d.firmware.clone()).collect::<Vec<_>>()
        );
        assert_eq!(
            s.resources[0].from,
            Some(Provenance::Synthesised { seed: 9 }),
            "the provenance still states the old seed"
        );
        assert!(s.restate_firmware("nobody", synth("A446", 1)).is_none());
    }

    /// **Five edits to one seed leave one entry, not five** — §11.2 files an iPod at the mint, so
    /// every subsequent tune is a restatement of that entry.
    #[test]
    fn tuning_an_identity_files_one_ipod_and_not_five() {
        let mut s = a_device_called("mine");
        let mut name = s.devices[0].firmware.clone();
        for seed in 1..=5 {
            name = s
                .restate_firmware(&name, synth("A446", seed))
                .expect("each tune restates the same entry");
        }
        assert_eq!(s.resources.len(), 1, "{:?}", s.resources);
        assert_eq!(name, "A446, seed 5");
        assert_eq!(s.devices[0].firmware, name);
        // And restating it to what it already is keeps its own name rather than colliding with
        // itself and becoming `A446, seed 5 (2)`.
        let again = s.restate_firmware(&name, synth("A446", 5)).expect("idempotent");
        assert_eq!(again, name);
        assert_eq!(s.resources.len(), 1);
    }

    /// **Removing an entry from a list is not deleting somebody's file**, and it does not rewrite
    /// the device that named it — so `missing` can say *which name* is gone.
    #[test]
    fn removing_a_resource_never_deletes_the_file_and_the_device_still_names_it() {
        let dir = temp_dir("remove-resource");
        let rom = dir.join("real.bin");
        std::fs::write(&rom, [0u8; 16]).expect("a scratch dump");
        let mut s = Settings {
            nor: crate::nor::Source::File(rom.clone()),
            ..Default::default()
        };
        s.remember_as("mine");
        let filed = s.devices[0].firmware.clone();

        assert!(s.remove_resource(&filed));
        assert!(!s.remove_resource(&filed), "removing it twice is not a second removal");
        assert!(rom.is_file(), "removing an entry deleted somebody's only dump of an iPod");
        assert_eq!(
            s.devices[0].firmware, filed,
            "the device stopped naming what is gone, so nothing can say which name it was"
        );
        assert_eq!(s.missing(&s.devices[0]), vec![Absent::Unlisted(filed)]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// §11.4's `used by N`, both halves — **a device that names only a resolved path still counts.**
    #[test]
    fn removal_names_every_device_and_every_disk_that_refers_to_it() {
        let mut s = Settings::default();
        s.file_disk(PathBuf::from("/drives/mine.img"), "mine");
        s.disks[0].built_from = Some("iPod_25.1.3".into());
        s.disks[0].installed = vec!["Rockbox 4.0".into()];
        s.file_away(
            Resource::Installer(PathBuf::from("/x/iPod_25.1.3.ipsw")),
            "iPod_25.1.3",
            None,
        );
        s.devices.push(Device {
            name: "by name".into(),
            firmware: "rom".into(),
            disk: Some("mine".into()),
            ..Default::default()
        });
        // A device migrated from the old shape: a resolved path and no name.
        s.devices.push(Device {
            name: "by path".into(),
            firmware: "rom".into(),
            disk_path: Some(PathBuf::from("/drives/mine.img")),
            ..Default::default()
        });

        assert_eq!(s.devices_using_disk("mine"), vec!["by name", "by path"]);
        assert_eq!(s.disks_recording_resource("iPod_25.1.3"), vec!["mine"]);
        assert_eq!(s.disks_recording_resource("Rockbox 4.0"), vec!["mine"]);
        assert!(s.disks_recording_resource("nothing").is_empty());
        // An empty name matches nothing, rather than every device that names nothing.
        assert!(s.devices_using_disk("").is_empty());
        assert!(s.devices_using_resource("").is_empty());

        assert!(s.remove_disk("mine"));
        assert!(!s.remove_disk("mine"));
        assert_eq!(
            s.devices[0].disk.as_deref(),
            Some("mine"),
            "removing a disk rewrote the device that named it"
        );
    }

    /// A resource two devices share says so **before** it is edited or removed.
    #[test]
    fn a_rom_two_devices_share_is_used_by_two() {
        let mut s = a_device_called("one");
        s.remember_as("two");
        s.nor = synth("A146", 77);
        s.remember_as("three");
        let shared = s.devices[0].firmware.clone();
        assert_eq!(s.devices_using_resource(&shared), vec!["one", "two"]);
        assert_eq!(s.devices_using_resource(&s.devices[2].firmware.clone()), vec!["three"]);
    }

    // ── The two sweeps this file owns ───────────────────────────────────────────────────────────

    /// **Nothing in this program reads a settings mode any more**, and a file that carries one is
    /// read exactly as it was before the field went.
    #[test]
    fn no_settings_key_called_mode_survives_a_render() {
        let text = Settings::default().render();
        assert!(
            !text.contains("mode = "),
            "the renderer still writes a key nothing reads:\n{text}"
        );
        // The compatibility half: `parse` ignores keys it does not know, so an existing file is not
        // a migration and not a complaint. It is the same Settings, minus a line, on the next save.
        assert_eq!(
            Settings::parse("mode = debug\nwelcomed = true\n"),
            Settings {
                welcomed: true,
                ..Default::default()
            }
        );
    }

    /// **Every sentence this model hands the window is ASCII or an em dash.**
    ///
    /// §6.7 / §16.6: Slint takes one `font-family` per element with no fallback list and nothing in
    /// `.slint` can ask whether a glyph exists, so a character outside the closed set the window
    /// trusts falls to `.notdef` — an empty square — in the middle of a row. `·` (U+00B7) is not in
    /// that set: it is a *symbol*, and §6.7's answer for a symbol is a drawn `Path`.
    ///
    /// The window's own sweep now reads this crate's library as well, so the rule is enforced over
    /// every sentence the model words and not only over this one. This stays because it names the
    /// set a `Provenance` line is allowed to draw from, which is narrower than the sweep's.
    #[test]
    fn every_provenance_line_is_ascii_or_an_em_dash() {
        let all = [
            Provenance::Dumped,
            Provenance::Synthesised { seed: 0x4f2a },
            Provenance::Fetched {
                verified: Verification::Sha256,
            },
            Provenance::Fetched {
                verified: Verification::SizeOnly,
            },
            Provenance::Fetched {
                verified: Verification::None,
            },
            Provenance::Provided,
            Provenance::Built,
        ];
        for p in all {
            let line = p.line();
            for c in line.chars() {
                assert!(
                    c.is_ascii() || c == '—',
                    "`{c}` (U+{:04X}) is in {line:?}, and the window's font is not trusted for it",
                    c as u32
                );
            }
        }
        // The control, in the shape `AGENTS.md` §6 asks for: the predicate above has to be able to
        // say no, or a sweep that found nothing looks exactly like a set that carries nothing.
        let planted = "synthesised · seed 4f2a";
        assert!(
            planted.chars().any(|c| !c.is_ascii() && c != '—'),
            "the check cannot see the character it exists to refuse"
        );
    }

    /// **A parked machine is measured by what deleting it gives back, not by what it claims.**
    ///
    /// `clone_disk` copies with `cp -c`, so on APFS a drive image shares every block the emulator
    /// has not written. Summing `len()` once told the operator a cache had reached 32 GB; deleting
    /// one whole set — 6.4 GB by that arithmetic — returned 153 MB. §11.4's whole argument is that
    /// this is where every byte the program spends is visible, so it cannot open with a figure
    /// wrong by a factor of forty in either direction.
    #[cfg(unix)]
    #[test]
    fn a_directory_is_measured_materialised_and_the_apparent_figure_is_a_different_number() {
        const GIB: u64 = 1024 * 1024 * 1024;
        let dir = temp_dir("sparse");
        let f = std::fs::File::create(dir.join("frozen.img")).expect("a scratch image");
        // A hole, which is what `make-disk` produces and what `cp -c` preserves. Nothing is
        // written, so nothing is allocated.
        f.set_len(GIB).expect("a sparse file");
        drop(f);

        let apparent = dir_size_apparent(&dir);
        let real = dir_size(&dir);
        assert_eq!(apparent, GIB, "the apparent figure is the length it claims");
        assert!(
            real < GIB / 100,
            "a 1 GiB hole was measured as {real} bytes of real disk; `dir_size` is summing \
             `len()` again and every size this program shows is wrong in the direction of alarm"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod first_run_naming_tests {
    use super::*;

    /// A scratch directory of this module's own. `tests::temp_dir` is private to that module, and a
    /// sibling cannot reach it — copying four lines is cheaper than widening a test helper.
    fn temp_dir(what: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "ipod-emulator-first-run-{what}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&d).expect("a temp directory");
        d
    }

    fn an_ipod() -> crate::nor::Source {
        crate::nor::Source::Synthetic {
            model: crate::nor::DEFAULT_MODEL.into(),
            seed: 7,
            serial: None,
            guid: None,
            splash: None,
        }
    }

    /// The names first run puts on the shelf are the words a person would use, not the recipe.
    ///
    /// `suggest_nor_name` is `A446, seed 7` — right for a row in a list of recipes, and not
    /// something anybody would say out loud about the iPod on the bench.
    #[test]
    fn the_suggested_names_are_the_words_a_person_would_use() {
        let src = an_ipod();
        assert_eq!(suggest_ipod_name(&src), "Black 5.5G");
        assert_eq!(suggest_device_name(&src), "My 5.5G");
        assert_eq!(suggest_disk_stem(&src), "my-5.5g");
        assert!(
            !suggest_ipod_name(&src).contains("seed"),
            "the shelf is being shown a recipe: {}",
            suggest_ipod_name(&src)
        );
        // The device name and the design's own word for it are one string.
        assert_eq!(suggest_device_name(&src), crate::compose::FIRST_RUN_DEVICE);
    }

    /// **A dump this program can read is named by what it IS, not by what the file is called.**
    ///
    /// `suggest_ipod_name` asks [`model_of`], which used to refuse every
    /// [`crate::nor::Source::File`] outright and fall through to [`suggest_nor_name`] — the file
    /// **stem**. So one readable dump had two names on one page: §11.2's level ① drew
    /// `foreign-oui` at the `iPod` row and `5G, 30 GB` at the `Model` row two below it, off the
    /// same bytes, because `nor::Source::model` reads the file and this did not.
    ///
    /// Both arms, because the fallback is still the answer for a dump nothing can read and
    /// deleting it would be worse than the defect: a name nobody recognises is still a name.
    #[test]
    fn a_dump_this_program_can_read_is_named_by_what_it_is() {
        use crate::identity::{Identity, Model, Source};

        let dir = temp_dir("dump-name");
        let m = Model::lookup("MA146").expect("the reference 5G");
        let spec = crate::nor::Spec::new(
            m,
            Identity {
                serial: Some("AB1234XYZQR".into()),
                guid: 0x001B_6300_ABCD_EF01,
                source: Source::RealDevice,
            },
        );
        let path = dir.join("some-dump-i-was-sent.rom");
        std::fs::write(&path, crate::nor::synthesise(&spec)).expect("a fabricated dump");
        let readable = crate::nor::Source::File(path);

        assert_eq!(
            suggest_ipod_name(&readable),
            format!("{} {}", m.colour().label(), m.generation.label()),
            "the iPod is named after the file rather than after the iPod"
        );
        assert_eq!(suggest_device_name(&readable), format!("My {}", m.generation.label()));
        assert_ne!(
            suggest_ipod_name(&readable),
            suggest_nor_name(&readable),
            "the two spellings collapsed; the stem is back"
        );

        // And a path that is not a NOR image at all keeps the fallback, which is the half that
        // must not be lost: `model_of` reads the file, so an unreadable one answers `None` and the
        // stem is still a name.
        let unreadable = crate::nor::Source::File(dir.join("nothing-is-here.bin"));
        assert_eq!(suggest_ipod_name(&unreadable), "nothing-is-here");
        assert_eq!(suggest_device_name(&unreadable), "My iPod");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A stem has to be a filename on every platform this program runs on**, and it is never
    /// empty: an empty stem produces `.img`, a hidden file nobody can find.
    #[test]
    fn a_disk_stem_is_a_filename_on_every_platform() {
        let banned = ['\\', '/', ':', '*', '?', '"', '<', '>', '|', '\n', '\r', '\0'];
        // **The hostile input goes through the sanitiser directly.** A person can rename a device
        // to anything; going in through `suggest_disk_stem` would only ever offer it `My 5.5G`, and
        // a sweep that cannot see a `/` cannot tell you whether `/` is handled.
        for name in [
            "My / iPod: \"the\" <one>|\\ *?",
            "  ..  ",
            "\n\r\0",
            "....",
            "-----",
            "My 5.5G",
            "",
        ] {
            let stem = file_stem_of(name);
            assert!(!stem.is_empty(), "{name:?} produced an empty stem");
            for c in banned {
                assert!(
                    !stem.contains(c),
                    "{name:?} produced {stem:?}, which carries {c:?}"
                );
            }
            assert!(!stem.starts_with('.'), "{name:?} produced the hidden file {stem:?}");
            assert!(!stem.ends_with('.'), "{name:?} produced {stem:?}, which Windows will not take");
        }
        assert_eq!(file_stem_of("My / iPod"), "my-ipod");
        assert_eq!(file_stem_of("\n\r\0"), "ipod");

        for model in ["A446", "A146", "not-a-model", "", "  ", "8513"] {
            let src = crate::nor::Source::Synthetic {
                model: model.into(),
                seed: 1,
                serial: None,
                guid: None,
                splash: None,
            };
            let stem = suggest_disk_stem(&src);
            assert!(!stem.is_empty(), "{model:?} produced an empty stem");
            for c in banned {
                assert!(
                    !stem.contains(c),
                    "{model:?} produced {stem:?}, which carries {c:?}"
                );
            }
            assert!(!stem.starts_with('.'), "{stem:?} is a hidden file");
        }
        // A dump has no model to read, and still gets a name.
        let file = crate::nor::Source::File(PathBuf::from("/roms/My Dump (2).bin"));
        assert_eq!(suggest_disk_stem(&file), "my-ipod");
    }

    /// **A build never takes a name something already occupies.** `fs::rename` overwrites silently,
    /// so without this a first run could destroy a drive the operator already had.
    #[test]
    fn a_build_never_takes_a_name_something_already_occupies() {
        let dir = temp_dir("free-path");
        let first = free_path(&dir, "my-5.5g", "img");
        assert_eq!(first.file_name().unwrap(), "my-5.5g.img");
        std::fs::write(&first, b"somebody's only copy of an iPod").unwrap();

        let second = free_path(&dir, "my-5.5g", "img");
        assert_ne!(second, first, "the second build would have overwritten the first");
        assert_eq!(second.file_name().unwrap(), "my-5.5g (2).img");
        std::fs::write(&second, b"and another").unwrap();
        assert_eq!(
            free_path(&dir, "my-5.5g", "img").file_name().unwrap(),
            "my-5.5g (3).img"
        );
        // The one that was already there is untouched.
        assert_eq!(
            std::fs::read(&first).unwrap(),
            b"somebody's only copy of an iPod"
        );
        // An empty stem is a name, not a hidden file.
        assert_eq!(free_path(&dir, "", "img").file_name().unwrap(), "ipod.img");
    }

    /// Drives land under the data directory, so `IPOD_EMULATOR_DATA` moves them — which is what
    /// makes it safe to run a build without landing 8 GiB in somebody's real library.
    #[test]
    fn drives_live_under_the_data_directory() {
        let _guard = env_lock();
        let dir = temp_dir("drives-dir");
        let before = std::env::var_os("IPOD_EMULATOR_DATA");
        // SAFETY: `env_lock` serialises every test in this crate that touches this variable.
        unsafe { std::env::set_var("IPOD_EMULATOR_DATA", &dir) };
        assert_eq!(drives_dir(), dir.join("drives"));
        // SAFETY: still holding the lock.
        unsafe {
            match before {
                Some(v) => std::env::set_var("IPOD_EMULATOR_DATA", v),
                None => std::env::remove_var("IPOD_EMULATOR_DATA"),
            }
        }
    }

    /// A device that names no drive is **unfinished**, not broken — which is what tells a first run
    /// that failed at the fetch from a device whose drive has gone.
    #[test]
    fn a_device_with_no_drive_is_unfinished_rather_than_broken() {
        let mut d = Device {
            name: "My 5.5G".into(),
            firmware: "an iPod".into(),
            ..Device::default()
        };
        assert!(!d.names_a_disk());
        d.disk = Some("my-5.5g".into());
        assert!(d.names_a_disk(), "a device naming a disk read as unfinished");
        d.disk = None;
        d.disk_path = Some(PathBuf::from("/drives/my-5.5g.img"));
        assert!(
            d.names_a_disk(),
            "a device naming a drive by path read as unfinished"
        );
    }
}
