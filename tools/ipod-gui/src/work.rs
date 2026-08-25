//! The five steps a first run performs, and the threads this window does its waiting on.
//!
//! `docs/GUI.md` §10. **Press the button**: this synthesises a boot ROM, downloads Apple's firmware,
//! builds a drive from it, installs Apple's software onto it, and hands the machine off to be
//! started. Nobody has to have an iPod, a NOR dump, or any file at all.
//!
//! [`Worker`] is that run and it is one thread. [`Reads`] is the other kind of waiting this crate
//! does — §11.2's Composer asking a chosen drive what its data partition is — and it is here for
//! the same reason: **the window's thread draws, and everything that can block belongs off it.**
//!
//! **There is no toolkit in this file**, and none may enter it. It names no generated type, takes
//! no window, and every sentence in it is testable with no display — the same rule `rail.rs` and
//! `nav.rs` keep, and the reason the window is replaceable (`AGENTS.md` §9).
//!
//! ## The two things that must be right
//!
//! **1. The identity is minted once.** [`nor::mint_seed`] is the one irreversible call in this
//! program: `Identity::generate` is a pure function of a model and that number, and the 8-byte
//! FireWire GUID it produces is what `sysinfo_t` carries and what iTunes binds DRM to. So the seed
//! *is* the iPod. A retry **resumes**; it does not re-mint. Three failed first runs leave one iPod
//! with one GUID, and [`Queue::press`] stores the ROM in `self.rom` **before anything that can
//! fail**, so even a settings save that fails leaves the next press with the same machine.
//!
//! **2. Temp, then rename.** Nothing acquires a real name until its bytes have been checked. The
//! download writes `<release>.ipsw.part` and `firmware.rs` renames it only after the SHA-256
//! matches; the build writes `<stem>.img.part` and it is renamed only after Apple's firmware
//! partition is in it and reads back as bootable. A cancelled or failed build therefore leaves no
//! partial file wearing a real name — and **a cancel deletes our own `.part` and nothing else**.
//! Both paths that reach `remove_file` name a file this program created in this run and announced
//! in [`Report::Writing`] before a byte went into it. `AGENTS.md` §3: a drive image is sometimes
//! the only copy of an iPod somebody owns.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use eapp_loader::compose::{self, Cost, Holes, Loader, Os, Recipe, Start, Step, Verb};
use eapp_loader::firmware::{self, Release, Trouble, Watch};
use eapp_loader::settings::{self, Device, Provenance, Resource, Settings, Verification};
use eapp_loader::{identity, ipodlinux, ipsw, nor, rockbox, si, volume};

use crate::rail::{Class, Failure, Kind, Progress, Rail, Tool};

// ── how often, and how long ─────────────────────────────────────────────────────────────────────

/// How often the window looks at the worker. 10 Hz.
///
/// Matched to [`firmware::WATCH_TICK`] so a faster poll cannot re-read the same number: about
/// sixty-five updates over a 6.5 MB download rather than thousands.
pub const TICK: std::time::Duration = std::time::Duration::from_millis(100);

/// How long the way-out path waits for a worker to acknowledge a cancel.
///
/// Everything a cancel does is a `kill`, an `unlink` or a `flush`; past 250 ms it is a hung
/// filesystem and waiting does not help. [`std::thread::JoinHandle::join`] has no timeout, and a
/// window that refuses to close because a network mount is wedged is worse than a stray `.part`.
pub const GRACE: std::time::Duration = std::time::Duration::from_millis(250);

/// Slack above the estimate before the pre-flight gate refuses.
///
/// `Cost::disk` is an estimate, and RetailOS writes to the volume itself on first boot. Refusing
/// with nothing written is cheap; filling somebody's disk half way through a write is not.
pub const HEADROOM: u64 = 64 * 1024 * 1024;

// ── the first run's own plan ────────────────────────────────────────────────────────────────────

/// The release first run fetches: the newest one Apple still serves in
/// [`compose::FIRST_RUN_FAMILY`] that can be verified byte for byte.
///
/// **The filename appears nowhere in this crate.** It is read out of the catalogue, which is the
/// same table `firmware::verify` refuses against — so the plan cannot promise a file the fetcher
/// would reject. The catalogue is oldest-first within a model, so the newest is the last match.
pub fn release() -> Option<&'static Release> {
    firmware::by_updater_family(compose::FIRST_RUN_FAMILY)
        .filter(|r| r.served && r.is_verifiable())
        .last()
}

/// The recipe §10 builds: Apple's firmware, Apple's bootloader, Apple's software, nothing else.
pub fn recipe() -> Option<(Recipe, &'static Release)> {
    let rel = release()?;
    Some((
        Recipe {
            start: Start::FromIpsw(rel.file.to_string()),
            loader: Loader::Apple,
            oses: [Os::Apple].into_iter().collect(),
        },
        rel,
    ))
}

/// §10.1's plan: the recipe's steps with the two book-ends the recipe cannot know about.
///
/// The boot ROM is **first** because a [`Recipe`] carries no ROM — it is about the drive — and the
/// cold boot is **last** because `Recipe::steps()`'s own contract is *everything that has to be
/// fetched, built or installed*, and a boot is none of those.
///
/// **This is what is on screen before anything is pressed.** Five rows, each with its own sub-line,
/// shown before a single byte is downloaded: nobody has ever been given that list before agreeing
/// to a download.
pub fn plan(holes: Holes) -> Vec<Step> {
    let mut v = vec![Step {
        kind: Verb::Synthesise,
        what: "a boot ROM".into(),
        sub: match eapp_loader::identity::Model::lookup(compose::FIRST_RUN_MODEL) {
            Some(m) => format!(
                "{}, {} GB, {}, model {} — instant, nothing downloaded",
                m.generation.label(),
                m.capacity_gb,
                m.colour().label().to_lowercase(),
                m.number
            ),
            // A model this build does not know is still a model. Never a placeholder, never a panic.
            None => format!(
                "model {} — instant, nothing downloaded",
                compose::FIRST_RUN_MODEL
            ),
        },
        cost: Cost::NONE,
    }];
    if let Some((r, _)) = recipe() {
        v.extend(r.steps(holes));
    }
    v.push(Step {
        kind: Verb::Start,
        what: "cold boot".into(),
        sub: format!(
            "about {} s — no percentage until this device has completed one",
            compose::COLD_BOOT_SECONDS
        ),
        cost: Cost::NONE,
    });
    v
}

/// One number per axis, both from [`plan`]. Never cached — a cache is a second source of a number.
pub fn cost(holes: Holes) -> Cost {
    plan(holes)
        .iter()
        .fold(Cost::NONE, |a, s| a.plus(s.cost))
}

/// The first-run device, if one has been minted.
///
/// **Not by name.** A person may rename the device, and a rename must not mint a second iPod. What
/// identifies it is that its boot ROM resolves to a *synthesised* recipe with a seed somebody's
/// press produced — `seed != 0`, because [`nor::Source::default`] is seed 0 and a default is not a
/// mint. That distinction is exactly why [`nor::mint_seed`] never returns 0.
///
/// **And a composed device is excluded, because that test alone answered *yes* for it.**
/// `Composer::make_one` mints the same shape this looks for, so every device the Composer filed
/// read as the first run's — which is what everything downstream then did with it: `press` reused
/// *its* boot ROM and built into *its* name, `resume_from` measured progress against it, and the
/// window offered to *finish making* it by running the fixed first-run plan. That plan reads no
/// `Recipe`, so a device composed as Rockbox-only was pointed at a build of Apple's firmware onto
/// an 8 GiB drive. [`Device::composed`] is the fact that separates them, and it is a fact about
/// where the device came from rather than about how far it got.
///
/// With two devices in the library the old test was wrong in a second way that this closes: it
/// returned the **first** synthesised one in list order, so a composed device sorting ahead of a
/// half-made first run handed its own identity and its own name to `press`.
pub fn minted(s: &Settings) -> Option<&Device> {
    s.devices.iter().find(|d| {
        !d.composed
            && matches!(
                s.nor_of(d),
                Some(nor::Source::Synthetic { seed, .. }) if *seed != 0
            )
    })
}

// ── the thread boundary ─────────────────────────────────────────────────────────────────────────

/// How the numerator of a step's progress is read off the file it is writing.
///
/// **Two, and the difference is load-bearing.** A download's `.part` grows a byte at a time, so its
/// apparent length is the honest numerator. A drive image is `set_len` to 8 GiB in its first
/// millisecond, so reading *its* length would show 8.6 GB against a 21 MB denominator before the
/// first real byte — a bar at 100 % on a build that has not started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Meter {
    /// The file's length. The downloader reports these itself, once per
    /// [`firmware::WATCH_TICK`], because it is the only thing awake while `curl` runs.
    Apparent,
    /// What the file costs on disk. Measured by whoever is polling, because the call writing it is
    /// inside `ipsw::build_volume` and does not come back until it is done.
    OnDisk,
}

/// Somebody has asked for the work to stop.
///
/// One `AtomicBool`, `Relaxed` — the same shape `emu::Link::quit` uses. Ordering buys nothing here:
/// there is one writer, one reader, and nothing else is published through it.
#[derive(Debug, Default)]
pub struct Cancel(AtomicBool);

impl Cancel {
    pub fn new() -> Arc<Cancel> {
        Arc::new(Cancel::default())
    }
    pub fn ask(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn asked(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A failure, in the two parts §9.3 needs and no more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fault {
    pub class: Class,
    /// **The model's own words, verbatim.** Nothing is re-worded on the way up.
    pub said: String,
}

/// Worker → window.
///
/// **Every variant owns its data**, which is what makes *the worker never touches `Settings`*
/// mechanical rather than a comment: there is no way to send one.
#[derive(Debug)]
pub enum Report {
    Started {
        i: usize,
    },
    /// The file this step is writing, so cancelling can say what it costs before it is pressed.
    Writing {
        i: usize,
        path: PathBuf,
        meter: Meter,
    },
    Bytes {
        i: usize,
        done: u64,
        total: u64,
    },
    /// A better sentence than the plan's: the release's real name, what was actually checked.
    Detail {
        i: usize,
        sub: String,
    },
    Done {
        i: usize,
        outcome: Outcome,
    },
    Failed {
        i: usize,
        fault: Fault,
    },
    /// The flag went true. `removed` is the `.part` the worker deleted on its way out — reported so
    /// the window can say what happened, **not** so it can delete it again.
    Cancelled {
        i: usize,
        removed: Option<PathBuf>,
    },
    /// The flag went true **after the last boundary that could act on it**, so the run finished.
    ///
    /// Nothing was undone and nothing is wrong — but somebody pressed `Cancel` and
    /// [`Queue::cancel`] told them it was accepted, and until this existed that was the last they
    /// heard of it. The drive is theirs either way; what they are owed is being told so.
    TooLate,
}

/// What a finished step produced.
///
/// The **only** thing that mutates the library, and it does so on the window's thread in
/// [`Queue::pump`] — so there is exactly one writer of `Settings` and it is never the worker.
#[derive(Debug)]
pub enum Outcome {
    Fetched {
        path: PathBuf,
        verified: Verification,
        /// **Which of §11.4's three shelves it goes on.** It used to be implied — [`Queue::record`]
        /// filed every `Fetched` as a [`Resource::Installer`], which was right for the only fetch
        /// this program could start and wrong for the moment a second one existed. A bootloader
        /// filed as an installer is a row in the wrong group offering the wrong verb, and nothing
        /// downstream could tell.
        filed_as: Filing,
    },
    Installed {
        path: PathBuf,
        /// What the finished drive costs on disk, measured rather than estimated.
        allocated: u64,
    },
    /// Nothing to record: the container half of the build, and a fetch that was already cached.
    Nothing,
}

/// Which shelf a fetched file belongs on.
///
/// **Derived from the catalogue entry, never declared beside it** — see [`Want::filing`]. Rockbox
/// publishes two files and the difference between them is exactly this: one goes in the firmware
/// partition and one goes on the volume, and `rockbox::Where` already says which.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filing {
    /// An `.ipsw`. A drive is built from it.
    Installer,
    /// Goes in the firmware partition, which holds exactly one thing.
    Bootloader,
    /// Installed onto a drive's volume.
    Software,
}

impl Filing {
    /// The library's own type for this shelf.
    fn resource(self, path: PathBuf) -> Resource {
        match self {
            Filing::Installer => Resource::Installer(path),
            Filing::Bootloader => Resource::Bootloader(path),
            Filing::Software => Resource::Software(path),
        }
    }
}

/// One thing this window can fetch on its own.
///
/// **Three catalogues, one verb.** `firmware::CATALOGUE` is Apple's 71 releases,
/// `rockbox::CATALOGUE` is the bootloader and the release zip, and `ipodlinux::LOADER` is
/// `ipodloader2` v2.8.1 — and every entry in all three carries a URL, a byte count and a SHA-256,
/// which is what makes a fetch of any of them verifiable and its progress honest.
///
/// **Borrowed rather than owned, and `'static` rather than cloned.** A catalogue entry is a
/// constant in `eapp-loader`; copying one into a plan would be a second copy of a hash that the
/// verifier does not read, and the first thing to go stale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
    Firmware(&'static Release),
    Rockbox(&'static rockbox::Piece),
    Loader(&'static ipodlinux::Piece),
}

impl Want {
    /// What it is called on disk and in the cache.
    pub fn file(self) -> &'static str {
        match self {
            Want::Firmware(r) => r.file,
            Want::Rockbox(p) => p.file,
            Want::Loader(p) => p.file,
        }
    }

    /// How many bytes, from the catalogue. **Never zero for anything offered** — [`Want::offered`]
    /// refuses an entry without one, because a step whose denominator is zero draws a number that
    /// moves and no bar, and §12.3 would rather have that than a bar over a guess.
    pub fn bytes(self) -> u64 {
        match self {
            Want::Firmware(r) => r.bytes,
            Want::Rockbox(p) => p.bytes,
            Want::Loader(p) => p.bytes,
        }
    }

    /// Whether this entry can be checked byte for byte once it lands, and is worth offering at all.
    ///
    /// **Both halves, and the second is not a formality.** Five of Apple's seventy-one releases
    /// answer 403 and are recorded `served: false`; offering one would be a control that starts a
    /// download this program already knows will fail. And a release with no hash on record can only
    /// be size-checked, which never renders as *verified* — so the window does not offer it either,
    /// and `ipod-boot firmware get` stays the route for anybody who wants one anyway.
    fn offered(self) -> bool {
        match self {
            Want::Firmware(r) => r.served && r.is_verifiable() && r.bytes != 0,
            // Both `Piece` catalogues assert this of every entry in their own tests, so this is
            // the same claim asked at the point of use rather than trusted from over there.
            Want::Rockbox(p) => p.bytes != 0 && p.sha256.len() == 64,
            Want::Loader(p) => p.bytes != 0 && p.sha256.len() == 64,
        }
    }

    /// Which shelf the fetched file is filed on. **Read off the catalogue**, never declared here:
    /// Rockbox's two pieces differ by `rockbox::Where` and that is the whole of the difference.
    pub fn filing(self) -> Filing {
        match self {
            Want::Firmware(_) => Filing::Installer,
            Want::Rockbox(p) => match p.goes {
                rockbox::Where::FirmwarePartition => Filing::Bootloader,
                rockbox::Where::Volume => Filing::Software,
            },
            Want::Loader(_) => Filing::Bootloader,
        }
    }

    /// The plan line. **[`crate::rail::Rail::plan`] takes `Step`s**, so a fetch a person asked for
    /// is filed on the Rail by the same call the first run's plan is — one list, rendered twice,
    /// with the same verb column and the same progress.
    ///
    /// `what` is the **file** rather than a kind, and that is a decision this plan forces: a
    /// library with a 5G and a 5.5G in it offers two Apple bundles at once, and two rows both
    /// reading `fetch Apple's firmware` is a plan nobody can read. `crate::group` and not `si` for
    /// the same reason `compose.rs` gives one line from its own fetch step: this is the number the
    /// verifier refuses against, so it is printed exactly.
    pub fn step(self) -> Step {
        Step {
            kind: Verb::Fetch,
            what: self.file().to_string(),
            sub: format!(
                "{} B — {}, SHA-256 checked",
                eapp_loader::group(self.bytes()),
                self.from()
            ),
            cost: Cost {
                down: self.bytes(),
                disk: self.bytes(),
                apparent: None,
            },
        }
    }

    /// Whose server it comes off, in the words a person would use for it.
    fn from(self) -> &'static str {
        match self {
            Want::Firmware(_) => "from Apple",
            Want::Rockbox(_) => "from rockbox.org",
            Want::Loader(_) => "from the ipodloader2 releases",
        }
    }

    /// Where it lands, once it verifies.
    pub fn lands_at(self, cache: &Path) -> PathBuf {
        cache.join(self.file())
    }

    /// The `.part` it is written as. **Announced before a byte goes into it**, so a cancel can say
    /// what it costs and so the only file a fetch can ever delete has been named first.
    fn part_path(self, cache: &Path) -> PathBuf {
        match self {
            Want::Firmware(r) => firmware::part_path(r, cache),
            Want::Rockbox(p) => rockbox::part_path(p, cache),
            Want::Loader(p) => ipodlinux::part_path(p, cache),
        }
    }

    /// Fetch it, watched. Three catalogues, three fetchers, one `.part`-then-rename discipline.
    fn download(self, cache: &Path, w: &mut dyn Watch) -> Result<PathBuf, (Trouble, String)> {
        match self {
            Want::Firmware(r) => firmware::download_watched(r, cache, w),
            Want::Rockbox(p) => rockbox::download_watched(p, cache, w),
            Want::Loader(p) => ipodlinux::download_watched(p, cache, w),
        }
    }

    /// Is it already here, and does it still verify?
    ///
    /// **Asked before the `.part` is announced**, and that ordering is the whole of why this
    /// exists. `Report::Writing` sets `Rail::writing`, which arms a `Cancel` whose own caption says
    /// *`<file>.part` is 0 B and cancelling deletes it* — and for a file that was already in the
    /// cache no `.part` is ever created, so the caption would be describing a file that does not
    /// exist. It reads and hashes the file, which is why it belongs on the worker.
    fn cached(self, cache: &Path) -> bool {
        match self {
            Want::Firmware(r) => firmware::is_cached(r, cache),
            Want::Rockbox(p) => std::fs::read(self.lands_at(cache))
                .is_ok_and(|b| rockbox::verify(p, &b).is_ok()),
            Want::Loader(p) => std::fs::read(self.lands_at(cache))
                .is_ok_and(|b| ipodlinux::verify(p, &b).is_ok()),
        }
    }
}

/// **The newest Apple release worth offering for one `UpdaterFamilyID`**, or nothing.
///
/// The same choice [`release`] makes for first run and the same one
/// `tests/firmware_fetch.rs` calls *"the exact choice the wizard makes"* — the last served,
/// verifiable entry of the family. The catalogue is oldest-first within a model, so the newest is
/// the last match.
///
/// **One release per family and not all of them.** Family 25 has three entries and family 24 has
/// six; a verb that fetched every one of them would spend 360 MB on a press whose caption said
/// `Fetch…`. Anybody who wants an older one has `ipod-boot firmware get <family>`, which is what
/// the group's escape hatch names.
pub fn newest_for_family(family: u16) -> Option<Want> {
    firmware::by_updater_family(family)
        .map(Want::Firmware)
        .filter(|w| w.offered())
        .last()
}

/// **The bootloaders this window can fetch**, in the order §11.4's Bootloaders group draws them.
///
/// Two, and both are in the sketch that section carries: Rockbox's `bootloader-ipodvideo.ipod`
/// (51 996 B) and `ipodloader2` v2.8.1 (56 912 B) — which that sketch already calls the row where
/// *`Fetch…` is the whole answer*, because `install-linux` stopped reaching into
/// `resources/vendor/` and now resolves the fetched release.
pub fn bootloaders() -> Vec<Want> {
    rockbox::CATALOGUE
        .iter()
        .map(Want::Rockbox)
        .chain(std::iter::once(Want::Loader(&ipodlinux::LOADER)))
        .filter(|w| w.offered() && w.filing() == Filing::Bootloader)
        .collect()
}

/// **The software this window can fetch.** One: Rockbox 4.0 itself, 9 090 335 B.
///
/// **ZeroSlackr is deliberately not in it**, and the reason is a measurement rather than a taste.
/// `ipodlinux::CATALOGUE`'s one entry is 101 146 859 B behind SourceForge's
/// `files/latest/download` redirect, and nothing in this build unpacks a `.7z` —
/// `rail::Tool::SevenZip`'s own retirement condition is *"the iPodLinux install lands"*, and it has
/// not. A verb that spent 101 MB on an archive this program cannot open would be §14.1's defect
/// with a progress bar on it. It stays in the catalogue, where `ipod-boot` reaches it.
pub fn software() -> Vec<Want> {
    rockbox::CATALOGUE
        .iter()
        .map(Want::Rockbox)
        .filter(|w| w.offered() && w.filing() == Filing::Software)
        .collect()
}

/// **Which `UpdaterFamilyID`s an iPod's firmware is published under**, from the model in its ROM.
///
/// `identity::Generation::updater_families` is the one table that knows, and it is the only thing
/// that separates a 5G from a 5.5G in a bundle — they share `FamilyID` 6. **An empty list means
/// *cannot say* rather than *nothing matches***, so a generation this project has not established
/// the mapping for offers nothing rather than offering the wrong bundle.
pub fn families_of(model: &identity::Model) -> Vec<u16> {
    model
        .generation
        .updater_families()
        .iter()
        .filter_map(|f| u16::try_from(*f).ok())
        .collect()
}

// ── the plan, resolved on the window's thread before the thread exists ──────────────────────────

/// Everything one run needs, owned outright.
///
/// **No `Settings` crosses the boundary.** `Settings` is the library; a worker that could edit it
/// would be a second writer, and every save point in §10.2 is on the window's thread after a
/// completed step.
#[derive(Debug)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub release: &'static Release,
    pub cache: PathBuf,
    /// The drive, and the `.part` it is built as. Nothing is called `image` until it is finished.
    pub image: PathBuf,
    pub image_part: PathBuf,
    pub sectors: u64,
    /// The first unticked step. **A retry resumes; it does not restart.**
    pub from: usize,
}

impl Plan {
    /// Build a plan, or refuse it with its reason.
    ///
    /// A step this build cannot run is refused **here**, before anything is minted or written,
    /// rather than drawn live and failing at the press.
    pub fn of(
        stem: &str,
        drives: &Path,
        cache: &Path,
        from: usize,
        holes: Holes,
        // **Which release, passed in rather than read here.** It is the queue's, and the queue's is
        // the catalogue's everywhere but in a test — see `Queue::fetching`. A plan that read the
        // catalogue itself would put the one thing a test cannot supply in the middle of the one
        // path a test most needs to drive.
        release: Option<&'static Release>,
    ) -> Result<Plan, Failure> {
        let Some(release) = release else {
            return Err(Failure::saying(
                Class::NotServed,
                "making an iPod",
                "there is no firmware release in this build's catalogue that Apple still serves \
                 and that can be verified byte for byte, so there is nothing to fetch."
                    .to_string(),
            ));
        };
        let steps = plan(holes);
        // Defensive, and it has to stay: `job_for` is total over `Verb`, so a seventh verb or a
        // recipe that starts from a drive somebody already has is refused with a sentence instead
        // of reaching a worker that has no arm for it.
        for s in &steps {
            if matches!(s.kind, Verb::Copy) {
                return Err(Failure::saying(
                    Class::Incompatible(compose::Fix::BuildFromIpsw),
                    "making an iPod",
                    "this build cannot start from a drive that already exists — first run builds \
                     one from Apple's firmware."
                        .to_string(),
                ));
            }
        }
        let image = settings::free_path(drives, stem, "img");
        let image_part = part_of(&image);
        Ok(Plan {
            steps,
            release,
            cache: cache.to_path_buf(),
            image,
            image_part,
            // §10.1 says 8 GiB three times, so the plan and the worker take it from one call.
            sectors: ipsw::DEFAULT_SECTORS,
            from,
        })
    }

    /// What has to be free, and where. Two entries when the firmware cache is on another volume.
    pub fn needs(&self) -> Vec<(PathBuf, u64)> {
        let down: u64 = self.steps.iter().map(|s| s.cost.down).sum();
        let disk: u64 = self.steps.iter().map(|s| s.cost.disk).sum();
        let drives = self
            .image
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.image.clone());
        if same_volume(&self.cache, &drives) {
            vec![(drives, disk)]
        } else {
            vec![(self.cache.clone(), down), (drives, disk.saturating_sub(down))]
        }
    }
}

/// `<path>.part`, for a path that already has an extension.
///
/// `Path::set_extension` would turn `my-5.5g.img` into `my-5.5g.part`, which is a *different drive*
/// as far as anything reading the directory is concerned.
fn part_of(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(".part");
    PathBuf::from(s)
}

/// Whether two directories are on one volume, as far as `volume::space` can tell.
///
/// **Not a device-number comparison.** `st_dev` is not available on Windows through `std`, and the
/// question here is only *do these two share a free-space figure* — which is exactly what the
/// mount point answers. Two `None`s are treated as one volume: the alternative is billing an
/// unmeasured volume twice.
fn same_volume(a: &Path, b: &Path) -> bool {
    match (volume::space(a), volume::space(b)) {
        (Some(x), Some(y)) => x.mount == y.mount,
        _ => true,
    }
}

/// One step's worth of work.
///
/// Kept as a description rather than a closure so that [`job_for`] can be **total over [`Verb`]**:
/// a seventh verb stops this file compiling until somebody has said what the worker does about it.
#[derive(Debug)]
pub enum Job {
    Fetch,
    /// Lay out the container: MBR, the firmware partition's extent, an empty FAT32 volume.
    Build,
    /// Apple's bytes into the partition, then the rename that gives the drive its real name.
    Install,
}

/// Which job a step needs, or `None` for the ones the window's own thread does.
///
/// `Synthesise` is `None` because it writes `Settings`, and the worker may not.
/// `Start` is `None` because running the machine is not this phase.
/// `Copy` is `None` because it is the Composer's, and [`Plan::of`] refuses a plan containing one.
pub fn job_for(step: &Step) -> Option<Job> {
    match step.kind {
        Verb::Fetch => Some(Job::Fetch),
        Verb::Build => Some(Job::Build),
        Verb::Install => Some(Job::Install),
        Verb::Synthesise | Verb::Start | Verb::Copy => None,
    }
}

// ── the worker ──────────────────────────────────────────────────────────────────────────────────

/// The plan's remaining steps, on one thread.
///
/// **There is never more than one.** A second worker would be a second writer of the same `.part`,
/// and one run is one machine being made.
///
/// One thread for the whole remainder rather than one per step: Apple's firmware partition is
/// 13.9 MB and lives between the build and the install, and handing it to a second thread would be
/// a copy of it across a channel for no reason.
#[derive(Debug)]
pub struct Worker {
    rx: mpsc::Receiver<Report>,
    cancel: Arc<Cancel>,
    handle: Option<std::thread::JoinHandle<()>>,
}

/// How a worker ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Stopped {
    /// There was nothing running.
    Idle,
    /// It stopped and said what it deleted.
    Ended { deleted: Option<PathBuf> },
    /// It did not answer inside [`GRACE`]. `temp` names the partial file it was writing, and
    /// **nothing deletes it here**: a thread that is still inside a write is one that would be
    /// racing an `unlink`, and a stray `.part` is the cheaper of the two.
    Abandoned { temp: Option<PathBuf> },
}

impl Worker {
    /// Start the plan's remaining steps, or say why the thread could not be made.
    ///
    /// **Fallible, and it used to end in `.ok()`.** A spawn that failed became `handle: None`,
    /// which reads as *already finished* — so `press` returned `Press::Running`, the timer started,
    /// the first `pump` found nothing to do and stopped it, and the window sat on one ticked step
    /// and four `Planned` ones with no failure anywhere, nothing on the Rail and nothing on stderr.
    /// Out of file descriptors or out of thread stacks is the one resource failure that is
    /// genuinely plausible here, and it was the one that said nothing.
    pub fn spawn(plan: Plan, cancel: Arc<Cancel>) -> Result<Worker, std::io::Error> {
        Worker::start("ipod-first-run", cancel, move |tx, flag| run(plan, tx, flag))
    }

    /// §11.4's per-part fetch, on a thread of its own.
    ///
    /// **The second body and the last one**, and it goes through [`Worker::start`] rather than
    /// through a second copy of the channel-and-handle dance — the `.ok()` defect that made a
    /// failed spawn read as *already finished* was in that dance, and one copy of it is one place
    /// for that to be true.
    pub fn spawn_fetch(
        wants: Vec<Want>,
        cache: PathBuf,
        cancel: Arc<Cancel>,
    ) -> Result<Worker, std::io::Error> {
        Worker::start("ipod-fetch", cancel, move |tx, flag| {
            fetch_run(wants, cache, tx, flag)
        })
    }

    /// The channel, the thread and the handle — **one implementation for both bodies**.
    fn start(
        name: &str,
        cancel: Arc<Cancel>,
        body: impl FnOnce(&mpsc::Sender<Report>, &Cancel) + Send + 'static,
    ) -> Result<Worker, std::io::Error> {
        let (tx, rx) = mpsc::channel();
        let flag = Arc::clone(&cancel);
        let handle = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || body(&tx, &flag))?;
        Ok(Worker {
            rx,
            cancel,
            handle: Some(handle),
        })
    }

    /// Everything said since the last call. **Never blocks.**
    pub fn drain(&mut self) -> Vec<Report> {
        let mut out = Vec::new();
        while let Ok(r) = self.rx.try_recv() {
            out.push(r);
        }
        out
    }

    pub fn busy(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| !h.is_finished())
    }

    /// Ask it to stop, and wait up to [`GRACE`] for it to say what it did.
    ///
    /// **Everything it said on the way out comes back.** This used to `drain()` into a `find_map`
    /// looking for one `Cancelled` and throw the rest away — so a close landing in the moment
    /// between the install's rename and the next 100 ms tick discarded the `Done` that carries
    /// `Outcome::Installed`, and the settings file written immediately afterwards did not mention
    /// the drive now sitting on disk. It also discarded the `Cancelled` for the *other* reason a
    /// caller wants it: without it the Rail keeps drawing the step as `Working`, with a live
    /// `Cancel` whose file the worker has already deleted.
    pub fn stop(&mut self) -> (Stopped, Vec<Report>) {
        let Some(h) = self.handle.take() else {
            return (Stopped::Idle, Vec::new());
        };
        self.cancel.ask();
        let deadline = std::time::Instant::now() + GRACE;
        while !h.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if !h.is_finished() {
            // Dropping the handle detaches the thread. It will finish, or it will not; either way
            // the window closes. Whatever it said before now is still worth having.
            return (Stopped::Abandoned { temp: None }, self.drain());
        }
        let _ = h.join();
        let said = self.drain();
        let deleted = said.iter().find_map(|r| match r {
            Report::Cancelled { removed, .. } => removed.clone(),
            _ => None,
        });
        (Stopped::Ended { deleted }, said)
    }
}

impl Drop for Worker {
    /// **Set on the way out of scope, always**, and never waited on: a `Drop` that blocks is a
    /// window that will not close. Whoever wants the answer calls [`Worker::stop`] first.
    fn drop(&mut self) {
        self.cancel.ask();
    }
}

/// What the download reports to.
struct Reporter<'a> {
    i: usize,
    tx: &'a mpsc::Sender<Report>,
    cancel: &'a Cancel,
}

impl Watch for Reporter<'_> {
    fn bytes(&mut self, done: u64, total: u64) {
        let _ = self.tx.send(Report::Bytes {
            i: self.i,
            done,
            total,
        });
    }
    fn stop(&self) -> bool {
        self.cancel.asked()
    }
}

/// The worker body.
///
/// `cancel` is observed at exactly four places and nowhere else: the top of each step, inside the
/// download's own watcher once per [`firmware::WATCH_TICK`], after the container is laid out, and
/// before the install's rename. Between those the work is one syscall that has to finish.
fn run(plan: Plan, tx: &mpsc::Sender<Report>, cancel: &Cancel) {
    // Apple's firmware partition, carried from the build to the install.
    let mut fw: Option<Vec<u8>> = None;

    for (i, step) in plan.steps.iter().enumerate().skip(plan.from) {
        let Some(job) = job_for(step) else {
            // `Synthesise` is already done — the window did it — and `Start` is Phase 7. Neither
            // reports `Done`: the run simply ends and the window notices the handoff.
            //
            // **So the boot step ends `Kind::Planned` rather than `Kind::Working`.** That is a
            // deliberate departure from the first sketch of this: a `Working` row with no progress
            // and nothing running is a lying instrument, which is the one thing §12.3 is written
            // about. `Planned` is what it is — a step nobody has run — and the handoff note beside
            // it says why.
            if step.kind == Verb::Start {
                // **A cancel that arrived too late is answered rather than swallowed.** The last
                // boundary is before the install's rename; a flag set after it lets the run finish
                // — correctly — while `Queue::cancel` has already told the person their request was
                // accepted. Two facts, and only one of them reached the window.
                if cancel.asked() {
                    let _ = tx.send(Report::TooLate);
                }
                return;
            }
            continue;
        };
        if cancel.asked() {
            // **Any cancellation removes our `.part`, whichever boundary it is noticed at.**
            // Without this the flag could be set between two steps and leave the build's partial
            // file behind — a file nobody asked for, in a directory nobody looks in.
            let removed = plan.image_part.exists().then(|| {
                let _ = std::fs::remove_file(&plan.image_part);
                plan.image_part.clone()
            });
            let _ = tx.send(Report::Cancelled { i, removed });
            return;
        }
        let _ = tx.send(Report::Started { i });

        let outcome = match job {
            Job::Fetch => fetch(i, &plan, tx, cancel),
            Job::Build => build(i, &plan, tx, cancel, &mut fw),
            Job::Install => install(i, &plan, tx, cancel, fw.take()),
        };
        match outcome {
            Ok(o) => {
                let _ = tx.send(Report::Done { i, outcome: o });
            }
            Err(Stop::Cancelled { removed }) => {
                let _ = tx.send(Report::Cancelled { i, removed });
                return;
            }
            Err(Stop::Failed(fault)) => {
                let _ = tx.send(Report::Failed { i, fault });
                return;
            }
        }
    }
}

/// Why a step did not finish.
#[derive(Debug)]
enum Stop {
    Cancelled { removed: Option<PathBuf> },
    Failed(Fault),
}

fn fetch(
    i: usize,
    plan: &Plan,
    tx: &mpsc::Sender<Report>,
    cancel: &Cancel,
) -> Result<Outcome, Stop> {
    let rel = plan.release;
    if firmware::is_cached(rel, &plan.cache) {
        let _ = tx.send(Report::Detail {
            i,
            sub: format!("{} — already here, SHA-256 checked", rel.file),
        });
        // **Still `Fetched`.** The bytes are here and they verify, which is the whole of what the
        // word means; reporting nothing would leave the library without the bundle the drive was
        // built from, and a resumed run is exactly when that matters.
        return Ok(Outcome::Fetched {
            path: plan.cache.join(rel.file),
            verified: Verification::Sha256,
            filed_as: Filing::Installer,
        });
    }
    // **Named before a byte goes into it**, so cancelling can say what it costs and so the only
    // file this run can ever delete has been announced.
    let _ = tx.send(Report::Writing {
        i,
        path: firmware::part_path(rel, &plan.cache),
        meter: Meter::Apparent,
    });
    let mut w = Reporter { i, tx, cancel };
    match firmware::download_watched(rel, &plan.cache, &mut w) {
        Ok(path) => {
            let _ = tx.send(Report::Detail {
                i,
                sub: format!(
                    "{} — {} B — from Apple, SHA-256 checked",
                    rel.file,
                    eapp_loader::group(rel.bytes)
                ),
            });
            Ok(Outcome::Fetched {
                path,
                verified: if rel.is_verifiable() {
                    Verification::Sha256
                } else {
                    Verification::SizeOnly
                },
                // §10's fetch is an `.ipsw` and the drive is built from it.
                filed_as: Filing::Installer,
            })
        }
        // The fetcher removed its own `.part` on the way out, so reporting a file we did not
        // delete would be a false claim.
        Err((Trouble::Stopped, _)) => Err(Stop::Cancelled { removed: None }),
        Err((t, said)) => Err(Stop::Failed(Fault {
            class: class_of(t),
            said,
        })),
    }
}

/// Which of §9.3's classes a fetch failure is. **The model decided; this only translates.**
fn class_of(t: Trouble) -> Class {
    match t {
        Trouble::NotServed { .. } => Class::NotServed,
        Trouble::NoTool => Class::ToolMissing(Tool::Curl),
        Trouble::Unreachable { .. } => Class::Network,
        Trouble::Verification => Class::Verification,
        Trouble::Io => Class::Permission,
        // Not a failure, and never reaches here: `fetch` takes it as a cancellation.
        Trouble::Stopped => Class::Permission,
    }
}

fn build(
    i: usize,
    plan: &Plan,
    tx: &mpsc::Sender<Report>,
    cancel: &Cancel,
    fw: &mut Option<Vec<u8>>,
) -> Result<Outcome, Stop> {
    let bundle = plan.cache.join(plan.release.file);
    // A stale `.part` from an abandoned run must not be built on top of. It is ours, and it is a
    // `.part`, which is the whole of the rule.
    let _ = std::fs::remove_file(&plan.image_part);

    let mut bytes = match ipsw::inspect(&bundle) {
        ipsw::Ipsw::Good(what, fw) => {
            let _ = tx.send(Report::Detail { i, sub: what });
            fw
        }
        ipsw::Ipsw::Wrong(why) | ipsw::Ipsw::Bad(why) => {
            return Err(Stop::Failed(Fault {
                class: Class::Verification,
                said: why,
            }))
        }
    };
    // One byte, and it is the difference between a drive that boots and one that sits in Apple's
    // flash updater waiting for a power cycle nothing here performs.
    if ipsw::mark_aupd_applied(&mut bytes) {
        let _ = tx.send(Report::Detail {
            i,
            sub: "Apple's updater marked applied, so the first boot runs the OS".into(),
        });
    }

    let _ = tx.send(Report::Writing {
        i,
        path: plan.image_part.clone(),
        meter: Meter::OnDisk,
    });
    if let Err(e) = ipsw::build_volume(&plan.image_part, plan.sectors, bytes.len()) {
        return Err(Stop::Failed(space_or_permission(plan, &e)));
    }
    if cancel.asked() {
        let _ = std::fs::remove_file(&plan.image_part);
        return Err(Stop::Cancelled {
            removed: Some(plan.image_part.clone()),
        });
    }
    *fw = Some(bytes);
    Ok(Outcome::Nothing)
}

fn install(
    i: usize,
    plan: &Plan,
    tx: &mpsc::Sender<Report>,
    cancel: &Cancel,
    fw: Option<Vec<u8>>,
) -> Result<Outcome, Stop> {
    let Some(bytes) = fw else {
        // Reachable only by resuming into an install whose build did not run in this process, and
        // the honest answer is to say so rather than to write Apple's bytes into nothing.
        return Err(Stop::Failed(Fault {
            class: Class::Missing,
            said: "the firmware partition to install is not in hand — the drive has to be built \
                   again first."
                .into(),
        }));
    };
    let _ = tx.send(Report::Writing {
        i,
        path: plan.image_part.clone(),
        meter: Meter::OnDisk,
    });
    if let Err(e) = ipsw::write_firmware_partition(&plan.image_part, &bytes) {
        return Err(Stop::Failed(space_or_permission(plan, &e)));
    }
    // **Read back what was written**, because a drive that will boot Apple's updater instead of
    // the OS looks broken later for a reason nobody recorded.
    match ipsw::firmware_state(&plan.image_part) {
        Ok(st) if st.has_os && !st.aupd_armed => {
            let _ = tx.send(Report::Detail {
                i,
                sub: format!("firmware partition holds {}", st.tags.join(", ")),
            });
        }
        Ok(st) => {
            return Err(Stop::Failed(Fault {
                class: Class::Verification,
                said: format!(
                    "the drive was written and reads back as {} — {}.",
                    if st.tags.is_empty() {
                        "nothing".to_string()
                    } else {
                        st.tags.join(", ")
                    },
                    if st.has_os {
                        "Apple's flash updater is still armed, so the first boot would run it \
                         instead of the OS"
                    } else {
                        "there is no operating system in it to boot"
                    }
                ),
            }))
        }
        Err(why) => {
            return Err(Stop::Failed(Fault {
                class: Class::Verification,
                said: why,
            }))
        }
    }
    if cancel.asked() {
        let _ = std::fs::remove_file(&plan.image_part);
        return Err(Stop::Cancelled {
            removed: Some(plan.image_part.clone()),
        });
    }
    // **The only moment anything acquires a real name.** Everything before this point is a `.part`.
    if let Err(e) = std::fs::rename(&plan.image_part, &plan.image) {
        return Err(Stop::Failed(Fault {
            class: Class::Permission,
            said: format!("{}: {e}", plan.image.display()),
        }));
    }
    let allocated = std::fs::metadata(&plan.image)
        .map(|m| settings::on_disk_size(&m))
        .unwrap_or(0);
    Ok(Outcome::Installed {
        path: plan.image.clone(),
        allocated,
    })
}

// ── the per-part fetch, on the same thread boundary ─────────────────────────────────────────────

/// §11.4's `Fetch…`, as a worker body.
///
/// **It is not a [`Plan`], and that is the point.** A `Plan` is one drive being made: it carries a
/// release, an image, a `.part` and a sector count, and every one of those is meaningless here.
/// What a per-part fetch has is a list of files and a directory to put them in, so that is what it
/// takes — and it reports through the same [`Report`] channel, so [`Queue::apply`] draws it,
/// records it and saves it by the same code that draws the first run.
///
/// `cancel` is observed at two places: the top of each want, and inside each download's own watcher
/// once per [`firmware::WATCH_TICK`]. Between those the work is one `curl` and one `rename`.
fn fetch_run(wants: Vec<Want>, cache: PathBuf, tx: &mpsc::Sender<Report>, cancel: &Cancel) {
    for (i, want) in wants.iter().enumerate() {
        if cancel.asked() {
            // **`removed: None`, always.** A fetch's only file is its `.part`, and the fetcher
            // deletes that itself on the way out — reporting a file this thread did not delete
            // would be a false claim, which is the same rule `fetch` keeps for `Trouble::Stopped`.
            let _ = tx.send(Report::Cancelled { i, removed: None });
            return;
        }
        let _ = tx.send(Report::Started { i });
        if want.cached(&cache) {
            let _ = tx.send(Report::Detail {
                i,
                sub: format!("{} — already here, SHA-256 checked", want.file()),
            });
            // **Still `Fetched`.** The bytes are here and they verify, and filing them is the whole
            // of what this press was for: a bundle sitting in the cache that the library has never
            // heard of is exactly the state `Fetch…` exists to end.
            let _ = tx.send(Report::Done {
                i,
                outcome: Outcome::Fetched {
                    path: want.lands_at(&cache),
                    verified: Verification::Sha256,
                    filed_as: want.filing(),
                },
            });
            continue;
        }
        // Named before a byte goes into it, so cancelling can say what it costs.
        let _ = tx.send(Report::Writing {
            i,
            path: want.part_path(&cache),
            meter: Meter::Apparent,
        });
        let mut w = Reporter { i, tx, cancel };
        match want.download(&cache, &mut w) {
            Ok(path) => {
                let _ = tx.send(Report::Detail {
                    i,
                    sub: format!(
                        "{} — {} B — {}, SHA-256 checked",
                        want.file(),
                        eapp_loader::group(want.bytes()),
                        want.from()
                    ),
                });
                let _ = tx.send(Report::Done {
                    i,
                    outcome: Outcome::Fetched {
                        path,
                        // **`Sha256` and never `SizeOnly`.** `Want::offered` refuses an entry with
                        // no hash on record, so nothing this function can be handed is checked any
                        // more weakly than this claims.
                        verified: Verification::Sha256,
                        filed_as: want.filing(),
                    },
                });
            }
            Err((Trouble::Stopped, _)) => {
                let _ = tx.send(Report::Cancelled { i, removed: None });
                return;
            }
            Err((t, said)) => {
                let _ = tx.send(Report::Failed {
                    i,
                    fault: Fault {
                        class: class_of(t),
                        said,
                    },
                });
                return;
            }
        }
    }
    // **A cancel that arrived after the last boundary is answered rather than swallowed**, exactly
    // as the first run answers one: [`Queue::cancel`] has already told the person their request was
    // accepted, and without this that is the last they hear of it.
    if cancel.asked() {
        let _ = tx.send(Report::TooLate);
    }
}

/// Classify a write failure **by measuring**, never by matching on the message.
///
/// An `io::Error` for a full volume is `ENOSPC` on one platform and a different sentence on
/// another, and a program that greps its own error strings is one that will be wrong in a language
/// it was not tested in. So: ask the volume how much room is left, and let the number decide.
fn space_or_permission(plan: &Plan, said: &str) -> Fault {
    let dir = plan
        .image_part
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| plan.image_part.clone());
    let on_disk = std::fs::metadata(&plan.image_part)
        .map(|m| settings::on_disk_size(&m))
        .unwrap_or(0);
    let need: u64 = plan.steps.iter().map(|s| s.cost.disk).sum();
    match volume::space(&dir) {
        Some(s) if s.free < need => Fault {
            class: Class::SpaceMidWrite,
            said: format!(
                "Stopped at {}. {} is {} and cancelling deletes it. {} has {} free.",
                si(on_disk),
                plan.image_part
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| plan.image_part.display().to_string()),
                si(on_disk),
                s.mount,
                si(s.free)
            ),
        },
        _ => Fault {
            class: Class::Permission,
            said: said.to_string(),
        },
    }
}

// ── the drive nobody has read yet ───────────────────────────────────────────────────────────────

/// Background reads of what a drive's data partition **is**, and the answers that have landed.
///
/// `install::data_partition_type` opens a drive image and reads its first 512 bytes. Both of those
/// are the operating system's work and neither of them is bounded: a library disk can be a 55.9 GB
/// file on an external drive, a network mount or a sleeping spindle, and `File::open` on one of
/// those blocks for as long as it blocks. §11.2's picker is a control a person presses, so the
/// answer cannot be fetched under the press — which is the whole of why this type exists and why
/// [`Reads::start`] hands control straight back.
///
/// **Every answer is tagged with the file it is about**, because it can land after somebody has
/// chosen a different drive. `composer::Composer::took_reading_of` is what compares the two, and
/// an answer about a drive that is no longer chosen is dropped rather than written onto the one
/// that is.
pub struct Reads {
    tx: mpsc::Sender<(PathBuf, Result<u8, String>)>,
    rx: mpsc::Receiver<(PathBuf, Result<u8, String>)>,
    /// One per read still going. **Handles rather than a counter**: a counter that a panicking
    /// thread never decrements is a window that wakes at 10 Hz for the rest of its life, and
    /// nothing would ever say so. A thread that ended without answering stops being outstanding
    /// here whatever it ended by.
    running: Vec<std::thread::JoinHandle<()>>,
}

impl Default for Reads {
    fn default() -> Reads {
        Reads::new()
    }
}

impl Reads {
    pub fn new() -> Reads {
        let (tx, rx) = mpsc::channel();
        Reads {
            tx,
            rx,
            running: Vec::new(),
        }
    }

    /// Read the MBR of `path` on a thread of its own.
    pub fn start(&mut self, path: PathBuf) {
        let file = path.clone();
        self.answer(path, move || eapp_loader::install::data_partition_type(&file));
    }

    /// Run `ask` on a thread of its own and post what it says, tagged with the drive it is about.
    ///
    /// **Infallible, unlike [`Worker::spawn`], and the difference is what there is to do about a
    /// failure.** A first run whose worker could not be spawned has four steps left undone and
    /// nothing to show for it, so `press` has to say so. A volume read that could not be spawned
    /// has one honest rendering already written — [`composer::VolumeRead::Failed`], *a drive nobody
    /// could read is not a drive that fails* — so the spawn error is posted down the same channel
    /// the read would have used. The one outcome that is certainly wrong is the region left saying
    /// *reading …* for ever, and posting the failure is what stops it.
    ///
    /// **`pub(crate)` and not private**, which is one seam and it is the honest one. What has to be
    /// proved about this type is *when* and *where* it answers, and a read of a real MBR finishes
    /// in microseconds — so neither is observable through [`Reads::start`], which is the only thing
    /// that ever calls this in the program. A probe that answers when the test says so is what
    /// makes `pump_once`'s *a drive being read holds the timer open* assertable at all, and
    /// `a_volume_read_answers_with_what_the_drives_mbr_says` is what pins that `start` hands this
    /// the function that reads an MBR.
    ///
    /// [`Worker::spawn`]: crate::work::Worker::spawn
    /// [`composer::VolumeRead::Failed`]: crate::composer::VolumeRead::Failed
    pub(crate) fn answer(
        &mut self,
        about: PathBuf,
        ask: impl FnOnce() -> Result<u8, String> + Send + 'static,
    ) {
        let tx = self.tx.clone();
        let named = about.clone();
        match std::thread::Builder::new()
            .name("ipod-volume-read".into())
            .spawn(move || {
                let answer = ask();
                // The window may have closed between the spawn and here; nobody is owed an error
                // about a receiver that has gone.
                let _ = tx.send((named, answer));
            }) {
            Ok(h) => self.running.push(h),
            Err(e) => {
                let _ = self
                    .tx
                    .send((about, Err(format!("no thread to read the drive on: {e}"))));
            }
        }
    }

    /// Everything that has landed since the last call. **Never blocks.**
    pub fn landed(&mut self) -> Vec<(PathBuf, Result<u8, String>)> {
        let mut out = Vec::new();
        while let Ok(a) = self.rx.try_recv() {
            out.push(a);
        }
        out
    }

    /// Whether any read is still going.
    ///
    /// **Asked before [`Reads::landed`] and never after**, and the order is the whole of why no
    /// answer is lost. A thread sends before it finishes, so a `false` here means every send has
    /// already happened and the drain that follows takes the lot. Asked the other way round, a
    /// thread that sent and finished in the gap between the two calls would leave its answer in the
    /// channel and the tick that would have drained it stopped.
    pub fn outstanding(&mut self) -> bool {
        self.running.retain(|h| !h.is_finished());
        !self.running.is_empty()
    }
}

// ── the press, the resume, the tick ─────────────────────────────────────────────────────────────

/// What a press did.
#[derive(Debug)]
pub enum Press {
    /// Started, or resumed from step `from`. `embodied` is true on the press that minted the ROM.
    Running { from: usize, embodied: bool },
    /// Refused before anything was written. **The identity may still have been minted** — see
    /// [`Queue::press`], and that is deliberate.
    Refused(Failure),
    /// A run is already in flight. §7.4 keeps the drawn centre button live, so this is reachable
    /// and is a note rather than a failure.
    Busy,
    /// Every step but the boot is done. The boot is Phase 7; this is the handoff.
    HandOff(String),
    /// **Nothing was started and nothing is wrong.** [`Queue::fetch`] with an empty offer: every
    /// file that group can fetch is already in the library, which is a good state and not a
    /// failure. The Rail carries the sentence, which is the caller's — see [`Queue::fetch`].
    ///
    /// [`Queue::press`] never answers it: a first run always has five steps.
    Nothing,
}

/// What one tick changed, so the caller knows what to re-push, save and delete.
#[derive(Default, Debug)]
pub struct Tick {
    /// Steps that finished, in the order they did.
    pub completed: Vec<usize>,
    /// The device is finished and wants starting.
    pub ready: Option<String>,
    /// The library changed, so the device rows have to be rebuilt.
    pub library_changed: bool,
    /// The Rail changed, so its rows have to be re-pushed.
    pub changed: bool,
    /// The fraction to push to the bench. **Negative means no denominator and no bar.**
    pub fraction: f32,
}

/// What a queue is doing, for a caller that only has to draw it. See [`Queue::shape`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    /// The first run's plan is on the Rail.
    pub has_plan: bool,
    /// A worker is running. **Not the same as "a step has reported"**.
    pub running: bool,
}

/// Which run this queue is holding.
///
/// **A queue runs one thing at a time, and now there are two things it can run.** Everything below
/// `ids`/`steps`/`done` used to mean *the first run's plan* by construction; with §11.4's per-part
/// fetch filing its own plan through the same three fields, four places had to be told apart —
/// the resumed-plan update in [`Queue::press`], the handoff in [`Queue::pump`], the busy note, and
/// which worker to release. A boolean would have done three of them and got the fourth wrong.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Run {
    /// Nothing has been pressed in this window.
    #[default]
    Idle,
    /// §10's five steps: the mint, the fetch, the build, the install, the boot.
    First,
    /// §11.4's `Fetch…`: N downloads and nothing else. No identity, no drive, no handoff.
    Fetch,
}

/// One first run: its plan, its identity, its thread.
#[derive(Debug)]
pub struct Queue {
    drives: PathBuf,
    cache: PathBuf,
    /// What the plan on the Rail is about. See [`Run`].
    run: Run,
    ids: Vec<u64>,
    steps: Vec<Step>,
    done: Vec<bool>,
    /// **The identity, minted once.** Set before anything that can fail, and never cleared while
    /// this window is open — which is what makes three failed presses one iPod.
    rom: Option<nor::Source>,
    worker: Option<Worker>,
    cancel: Option<Arc<Cancel>>,
    writing: Option<(usize, PathBuf, Meter)>,
    device: Option<String>,
    /// What the target volume turned out to do with holes, **measured at the press**.
    ///
    /// `None` until one has happened. It is kept because the plan on screen was drawn assuming
    /// [`Holes::Sparse`] — the probe writes an 8 GiB file to find out, and nothing may be written
    /// before a person agrees — so on a volume with no sparse files every figure the window is
    /// showing is 300× under, and the corrected one has to come from somewhere.
    holes: Option<Holes>,
    /// What the fetched bundle was filed under, so the drive can say what it was built from.
    installer: Option<String>,
    /// **Which release this queue fetches.** [`release`]'s answer in every build; a fixture's in
    /// the tests that drive a press without a third party.
    ///
    /// `is_cached` verifies against a recorded SHA-256, so nothing but Apple's own 6 533 633 bytes
    /// can stand in for the real one — which put `press`'s whole resume path behind a server being
    /// up. A release is data, and naming which one is a parameter.
    release: Option<&'static Release>,
}

impl Default for Queue {
    fn default() -> Queue {
        Queue::new()
    }
}

impl Queue {
    /// The queue the window uses: drives and firmware under the data directory, so
    /// `IPOD_EMULATOR_DATA` moves both.
    pub fn new() -> Queue {
        Queue::at(settings::drives_dir(), firmware::cache_dir())
    }

    /// A queue that builds somewhere else. What a test uses, and what a second data directory
    /// would use.
    pub fn at(drives: PathBuf, cache: PathBuf) -> Queue {
        Queue {
            drives,
            cache,
            run: Run::Idle,
            ids: Vec::new(),
            steps: Vec::new(),
            done: Vec::new(),
            rom: None,
            worker: None,
            cancel: None,
            writing: None,
            device: None,
            holes: None,
            installer: None,
            release: release(),
        }
    }

    /// A queue that fetches something other than the catalogue's newest verifiable release.
    ///
    /// **Tests only, and it is the seam that lets a press be driven with nothing downloaded.**
    #[cfg(test)]
    fn fetching(drives: PathBuf, cache: PathBuf, release: &'static Release) -> Queue {
        Queue {
            release: Some(release),
            ..Queue::at(drives, cache)
        }
    }

    /// The bill, **as measured** — `None` until a press has probed the volume.
    ///
    /// The plan is drawn against `Holes::Sparse`, which is right almost everywhere and 300× wrong
    /// on a volume without holes: 8.6 GB rather than 28 MB, for the same five steps. The build's
    /// own sub-line says why, in place, but the ledger and the shelf carry the *bill* and they were
    /// left quoting the assumption for the whole run and afterwards.
    pub fn measured_cost(&self) -> Option<Cost> {
        self.holes.map(cost)
    }

    pub fn busy(&self) -> bool {
        self.worker.as_ref().is_some_and(Worker::busy)
    }

    pub fn owns(&self, id: u64) -> bool {
        self.ids.contains(&id)
    }

    /// What the window needs to know about this queue to draw a heading.
    ///
    /// **Two facts, taken together**, because either one alone gets a heading wrong: a plan with
    /// steps left in it and nothing running is a run that is *over*, and a plan with nothing
    /// reported yet and a worker running is a run that has *begun*. The Rail cannot tell them
    /// apart — between the press and the worker's first `Started` there is no `Working` entry at
    /// all — so the heading read *This is what happened.* over a run that had just started, and
    /// *Working.* over one that had finished.
    pub fn shape(&self) -> Shape {
        Shape {
            has_plan: self.has_plan(),
            running: self.busy(),
        }
    }

    /// Whether the first run's plan is on the Rail.
    ///
    /// **This is what the Work page's heading keys on**, and it is not the same question as *is
    /// the welcome copy showing*. §9.1's later-empty bench has the plan and no welcome copy, and
    /// keying the heading on the welcome left five `Planned` rows under *This is what happened.*
    pub fn has_plan(&self) -> bool {
        !self.ids.is_empty()
    }

    /// The first step this queue has not ticked off.
    pub fn first_unticked(&self) -> usize {
        self.done.iter().position(|d| !d).unwrap_or(self.done.len())
    }

    /// §10.1: **the plan, before anything is pressed.** Filed as `Kind::Planned` entries.
    ///
    /// Idempotent, and **nothing is downloaded, minted, probed or written**. It costs one call to
    /// `Recipe::steps()`, which already existed.
    pub fn show(&mut self, rail: &mut Rail, steps: &[Step]) {
        if !self.ids.is_empty() {
            return;
        }
        // **The plan on screen before anything is pressed is the FIRST RUN's**, and saying so here
        // is what stops [`Queue::press`] treating it as somebody else's and filing a second copy
        // beside it. `Rail::plan` collapses only *finished* entries, so five `Planned` rows plus
        // five more `Planned` rows is ten rows and no way back.
        self.run = Run::First;
        self.steps = steps.to_vec();
        self.done = vec![false; steps.len()];
        self.ids = rail.plan(steps);
    }

    /// §10.2: the press.
    ///
    /// **The ordering below is the whole of §10.2 and §10.3 and none of it is rearrangeable.**
    ///
    /// 1. A run already in flight is a note, not a second worker.
    /// 2. No `curl` is a refusal with a command to paste, and **nothing is minted**.
    /// 3. **The identity**, reused or minted, stored before anything that can fail.
    /// 4. The volume is probed — which writes, which is why it is here and not while the plan was
    ///    merely being drawn.
    /// 5. The free-space gate, against the **materialised** estimate.
    /// 6. Resume from the first unticked step, and spawn.
    ///
    /// It takes four `&mut` rather than four `Rc<RefCell<_>>` so the caller can scope all its
    /// borrows in one block and this can re-enter none of them.
    pub fn press(&mut self, settings: &mut Settings, rail: &mut Rail, can_download: bool) -> Press {
        if self.busy() {
            rail.note(&self.already_running());
            return Press::Busy;
        }
        // **A plan that is not this run's is cleared rather than resumed.** `show` is idempotent on
        // `ids` being non-empty, so a finished fetch's rows would have stopped the first run's plan
        // from ever being filed — and every refusal below returns with the Work page showing the
        // wrong five lines. `Rail::plan` folds the finished ones into one note on the way past.
        if self.run != Run::First {
            self.ids.clear();
            self.steps.clear();
            self.done.clear();
        }
        self.run = Run::First;
        if self.ids.is_empty() {
            self.show(rail, &plan(Holes::Sparse));
        }
        if !can_download {
            return Press::Refused(Failure::new(
                Class::ToolMissing(Tool::Curl),
                "making an iPod",
            ));
        }

        // ---- 3. the identity. Idempotent, and the one irreversible act in this program.
        let embodied = self.rom.is_none() && minted(settings).is_none();
        let rom = self.identity(settings);
        // **Before anything that can fail.** A save that fails below leaves this set, so the next
        // press resumes with the same FireWire GUID rather than making a second iPod.
        self.rom = Some(rom.clone());

        let device_name = minted(settings)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| free_device_name(settings, &settings::suggest_device_name(&rom)));
        self.device = Some(device_name.clone());

        // **The live machine is this iPod, on every press and not only on the first.**
        // `remember_as` files the LIVE ROM, and on a resumed run in a fresh process `settings.nor`
        // is `Source::default()` — the never-chosen seed-0 iPod. Setting it only when minting left
        // the install step filing a default synthetic ROM into the library beside the real one.
        settings.nor = rom.clone();
        if minted(settings).is_none() {
            settings.file_away(
                Resource::Firmware(rom.clone()),
                &settings::suggest_ipod_name(&rom),
                // `None`, not `Some(Provenance::Synthesised)`: `file_away` derives the provenance
                // from the recipe and discards a passed one, so passing one is two spellings of
                // one fact.
                None,
            );
            // **And the live machine has no drive, because this one has not been made yet.** The
            // line above says the live iPod is the one being minted; this is the other half of
            // that same statement, and leaving it out did not leave the drive unstated — it left
            // the *previous* machine's under a brand-new iPod. `Settings::as_device` derives a new
            // device's drive from the live path, so an installation carried forward from before
            // the device list — a `disk =` line, no devices, `seed_resources` filing that drive —
            // minted an iPod already naming somebody else's image. `resume_from`'s step 3 is *the
            // minted device names a drive and nothing about it is missing*, which that satisfies
            // the moment it is minted: the press then answered `HandOff`, and the centre button on
            // an empty bench started a machine over a stranger's drive having downloaded nothing,
            // built nothing and installed nothing. `drops::land` writes the same line for the same
            // reason — a drive is built from an `.ipsw` and is not one, so there is nothing to
            // point at yet.
            settings.disk = None;
            settings.remember_as(&device_name);
            if let Err(e) = settings.save() {
                return Press::Refused(Failure::saying(
                    Class::Permission,
                    "remembering the iPod that was just made",
                    format!("{e}"),
                ));
            }
        }
        if let Some(id) = self.ids.first() {
            rail.done(*id);
        }
        if let Some(d) = self.done.first_mut() {
            *d = true;
        }

        // ---- 4. the volume. This writes an 8 GiB file to find out, which is why it is behind the
        // press: nothing is written before a person has agreed to the plan.
        let holes = match volume::probe(&self.drives, ipsw::DEFAULT_SECTORS * 512) {
            volume::Probe::Sparse => Holes::Sparse,
            volume::Probe::Full => Holes::Full,
            volume::Probe::Refused { why } => {
                return Press::Refused(Failure::saying(
                    Class::Permission,
                    "making an iPod",
                    why,
                ))
            }
            volume::Probe::TooBig { at, why } => {
                return Press::Refused(volume_refusal(&self.drives, at, &why))
            }
        };
        // Kept so the window can re-quote the bill against what the volume actually does. Every
        // figure on screen until now assumed holes.
        self.holes = Some(holes);

        // ---- 5. resume, then the gate, then spawn.
        let from = self.resume_from(settings);
        let stem = settings::suggest_disk_stem(&rom);
        let mut plan = match Plan::of(&stem, &self.drives, &self.cache, from, holes, self.release) {
            Ok(p) => p,
            Err(f) => return Press::Refused(f),
        };
        for (dir, need) in plan.needs() {
            if let Some(f) = gate(need, volume::space(&dir).as_ref(), &dir) {
                return Press::Refused(f);
            }
        }
        if from >= plan.steps.len().saturating_sub(1) {
            return Press::HandOff(device_name);
        }

        // **The plan on the Rail is the plan being run**, and a resumed run re-files nothing: a
        // second `plan()` would append five more entries — `Rail::retire_previous` drops the plan it
        // replaces, and re-filing this one would be replacing it with itself. Where the probe changed a sub-line — a volume with no holes says so — the
        // detail is updated in place instead.
        //
        // The length is not enough to know they are the same plan, which is why the block above
        // clears a foreign one: a five-want fetch and a five-step first run agree on the count, and
        // this branch would have written the first run's sub-lines onto the fetch's rows.
        if self.ids.len() == plan.steps.len() {
            for (id, s) in self.ids.iter().zip(plan.steps.iter()) {
                rail.detail(*id, s.sub());
            }
        } else {
            self.ids = rail.plan(&plan.steps);
            self.done = vec![false; plan.steps.len()];
        }
        // **The skipped prefix is ticked on both surfaces, and it was ticked on neither.**
        //
        // The equal-length branch above updates sub-lines and nothing else, so a run relaunched
        // after a failed build — the exact §10.3 case, with the bundle already in the cache — left
        // `fetch Apple's firmware` sitting `Planned` for ever while the build and the install ran
        // past it. Worse, `self.done` kept its hole, so `first_unticked()` answered 1 for the rest
        // of the run and `Tick::ready` — §12.2's handoff, gated on it — could never fire: the drive
        // finished, the timer stopped, and the window said nothing at all.
        //
        // It is out here rather than in the `else` because both branches need it and only one had
        // it. The same-process retry was green because `pump` had already ticked those steps.
        for (id, d) in self.ids.iter().zip(self.done.iter_mut()).take(from) {
            rail.done(*id);
            *d = true;
        }
        self.steps = plan.steps.clone();
        for id in self.ids.iter().skip(from) {
            // A retry increments the count, and that is what stops `Verification` offering `Retry`
            // for ever when a mirror keeps serving the wrong bytes.
            rail.retry(*id);
        }
        plan.from = from;
        let cancel = Cancel::new();
        match Worker::spawn(plan, Arc::clone(&cancel)) {
            Ok(w) => {
                self.cancel = Some(cancel);
                self.worker = Some(w);
                Press::Running { from, embodied }
            }
            // Out of threads. The identity is already minted and filed, so the next press resumes
            // with the same iPod — this refuses the run, not the machine.
            Err(e) => Press::Refused(Failure::saying(
                Class::Permission,
                "starting the work",
                format!("this program could not start a thread to do the work: {e}"),
            )),
        }
    }

    /// **§11.4's per-part fetch.** N downloads, on the one worker thread this queue owns.
    ///
    /// The whole reason it is here rather than under the press is §12.3: *a progress indication
    /// with no measurement behind it is worse than none*, and the measurement is
    /// `firmware::download_watched`'s — which only exists off the window's thread. A blocking
    /// download on the UI thread is a window that stops repainting for as long as somebody's
    /// connection takes, with no bar, no cancel and no way to tell it apart from a hang.
    ///
    /// **`nothing_to_fetch` is the caller's sentence, not this file's.** An empty offer means
    /// *every file this group can fetch is already in the library*, which is a fact about a group —
    /// and `work.rs` has no groups. `parts::Group::nothing_to_fetch` is the one producer, and it
    /// words the disabled verb as well, so the drawn refusal and the pressed one cannot drift.
    ///
    /// **Nothing is minted, probed or gated.** A fetch writes one file per want into the firmware
    /// cache and touches no drive, so `Plan::of`'s volume probe and the free-space gate are not
    /// this act's — the disk cost is the download's own bytes and `Cost::down` on each step carries
    /// it. What a full disk produces here is the fetcher's own `Trouble::Io`, which
    /// `class_of` files as `Class::Permission` with the operating system's sentence.
    pub fn fetch(&mut self, rail: &mut Rail, wants: &[Want], nothing_to_fetch: &str) -> Press {
        if self.busy() {
            rail.note(&self.already_running());
            return Press::Busy;
        }
        if wants.is_empty() {
            rail.note(nothing_to_fetch);
            return Press::Nothing;
        }
        // **The plan is replaced, never appended to.** `Rail::plan` collapses the previous run's
        // finished rows into one note, so pressing `Fetch…` under three groups in a row leaves
        // three notes and one plan rather than nine step rows.
        let steps: Vec<Step> = wants.iter().map(|w| w.step()).collect();
        self.run = Run::Fetch;
        self.device = None;
        self.ids = rail.plan(&steps);
        self.done = vec![false; steps.len()];
        self.steps = steps;
        let cancel = Cancel::new();
        match Worker::spawn_fetch(wants.to_vec(), self.cache.clone(), Arc::clone(&cancel)) {
            Ok(w) => {
                self.cancel = Some(cancel);
                self.worker = Some(w);
                Press::Running {
                    from: 0,
                    embodied: false,
                }
            }
            // Out of threads. Nothing has been written and nothing has been minted, so this refuses
            // the fetch and costs nothing.
            Err(e) => Press::Refused(Failure::saying(
                Class::Permission,
                "starting the work",
                format!("this program could not start a thread to do the work: {e}"),
            )),
        }
    }

    /// What a second press is told while a run is in flight.
    ///
    /// **Two runs, one sentence each, and the first-run one used to be said about both.** *Already
    /// making an iPod* under a `Fetch…` press would name a device this queue is not making.
    fn already_running(&self) -> String {
        match self.run {
            Run::Fetch => "Already fetching. The steps below are what it is doing.".into(),
            _ => format!(
                "Already making {}. The steps below are what it is doing.",
                self.device.clone().unwrap_or_else(|| "an iPod".into())
            ),
        }
    }

    /// The identity, reused if there is one and minted if there is not.
    ///
    /// Three sources in order, and the order is the whole of it: this window's own minted ROM, the
    /// one the library already holds for a half-made device, and — only then — a new one.
    fn identity(&self, settings: &Settings) -> nor::Source {
        if let Some(rom) = &self.rom {
            return rom.clone();
        }
        if let Some(src) = minted(settings).and_then(|d| settings.nor_of(d)) {
            return src.clone();
        }
        nor::Source::Synthetic {
            model: nor::DEFAULT_MODEL.into(),
            seed: nor::mint_seed(),
            serial: None,
            guid: None,
            splash: None,
        }
    }

    /// Which step to resume at, **derived from the artefacts and never from a stored tick**.
    ///
    /// A stored tick can go stale, be half written, or name a file since deleted. Asking the
    /// filesystem is correct after a crash *and* after a refusal, and it is one less thing in the
    /// settings file.
    ///
    /// | step | done when |
    /// |---|---|
    /// | 0 synthesise | the library holds a minted iPod |
    /// | 1 fetch | the release is in the cache **and still verifies** |
    /// | 2 build | never — its only output is a `.part`, which has no real name |
    /// | 3 install | the minted device names a drive and nothing about it is missing |
    /// | 4 start | never — pressing again starts the machine again |
    fn resume_from(&self, settings: &Settings) -> usize {
        let Some(rel) = self.release else { return 0 };
        resume_step(
            minted(settings).is_some(),
            firmware::is_cached(rel, &self.cache),
            minted(settings).is_some_and(|d| d.names_a_disk() && settings.missing(d).is_empty()),
        )
    }

    /// One tick. Called from a 100 ms timer and, in tests, directly.
    ///
    /// **This is the only place the library is written during a run**, and it is on the window's
    /// thread. §10.2 wants a save after every completed step, so a run that is interrupted between
    /// two of them resumes from the one it reached rather than from the beginning.
    pub fn pump(&mut self, settings: &mut Settings, rail: &mut Rail) -> Tick {
        let mut t = Tick {
            fraction: -1.0,
            ..Tick::default()
        };
        let Some(w) = self.worker.as_mut() else {
            return t;
        };
        // **Drained twice, and the second one is what stops a finished drive going unrecorded.**
        //
        // `busy()` reads `JoinHandle::is_finished`, so it goes false the instant the thread exits —
        // which can be *after* the drain above and *before* anything below looks at it. A `Done`
        // sent in that window would sit in the channel for ever: `all_but_the_boot` would be false
        // (the step is unticked), `pump_once` would then see `!busy()` and stop the 10 Hz timer,
        // and nothing would ever drain again. For the install's `Done` that is a finished 8 GiB
        // drive on disk that the library never learns about — `settings.disks` empty, the Rail
        // stuck on `Working`, and the next press building `my-5.5g (2).img` beside the orphan.
        //
        // Observing `!busy()` is an acquire on the thread's exit, and every `send` happened before
        // it, so one more `try_recv` sweep after that observation is guaranteed to see the lot.
        // Both test harnesses papered over this by pumping one extra time after the loop; the
        // window has no such extra pump, which is exactly why it belongs here.
        //
        // **No test can make that interleaving happen**, and saying so is better than implying one
        // does: the window is the few microseconds between the first `try_recv` that answers empty
        // and this line, and nothing in `Worker` lets a test hold the thread there.
        // `a_run_that_finished_before_the_first_pump_is_still_recorded` covers the *consequence* —
        // a finished drive reaching the library from a worker that is already gone — which is what
        // the race produces and what a `pump` that gave up on a dead worker would produce too.
        let mut reports = w.drain();
        if !w.busy() {
            reports.extend(w.drain());
        }
        self.apply(reports, settings, rail, &mut t);

        // A build's numerator cannot come from the worker: it is inside one call that does not
        // return until it is done. So whoever is polling measures the file.
        if let Some((i, path, Meter::OnDisk)) = &self.writing {
            let done = std::fs::metadata(path)
                .map(|m| settings::on_disk_size(&m))
                .unwrap_or(0);
            let total = self.steps.get(*i).map(|s| s.cost.disk).unwrap_or(0);
            if let Some(id) = self.ids.get(*i) {
                rail.progress(*id, Progress::Bytes { done, total });
                t.changed = true;
            }
        }
        // Every step but the boot is done, and nothing is running: the machine is made.
        //
        // **Gated on which run this is, and it was gated on the shape of the plan.** A fetch files
        // its own steps into `self.done`, and a two-want fetch with one want finished satisfies
        // `first_unticked() >= len - 1` — so the handoff fired mid-fetch. `t.ready` would have been
        // `None` (a fetch has no device) and the *visible* damage was the line below it: the worker
        // handle dropped while its thread was still downloading, which asks the `Drop` impl to set
        // the cancel flag. §12.2's handoff belongs to the run that makes a machine.
        let all_but_the_boot = self.run == Run::First
            && self.done.len() >= 2
            && self.first_unticked() >= self.done.len() - 1
            && rail.failures() == 0;
        if !self.busy() && all_but_the_boot {
            t.ready = self.device.clone();
            // Reported once. The handle also goes here, which is what releases the thread.
            self.worker = None;
        }
        // **A fetch has no handoff, so this is where its worker is released.** Without it the
        // handle sits in `self.worker` until the next press: `busy()` reads `is_finished`, so the
        // queue is not *busy*, but the finished thread is never joined and every subsequent `pump`
        // drains a dead channel. It is deliberately not gated on the steps having finished — a
        // failed or cancelled fetch is over too, and `Kind::Failed` stays on the Rail either way.
        if self.run == Run::Fetch && !self.busy() {
            self.worker = None;
        }
        t.fraction = rail
            .entries()
            .iter()
            .rev()
            .find(|e| e.kind == Kind::Working)
            .map_or(-1.0, |e| e.fraction());
        t
    }

    /// Everything the worker said, applied to the Rail and to the library — **in one place**, so
    /// that the way out applies it the same way a tick does.
    ///
    /// It is the only place the library is written during a run, and it is on the window's thread.
    fn apply(&mut self, reports: Vec<Report>, settings: &mut Settings, rail: &mut Rail, t: &mut Tick) {
        let mut recorded: Vec<Outcome> = Vec::new();
        for r in reports {
            t.changed = true;
            match r {
                Report::Started { i } => {
                    if let Some(id) = self.ids.get(i) {
                        rail.progress(*id, Progress::None);
                    }
                }
                Report::Writing { i, path, meter } => {
                    if let Some(id) = self.ids.get(i) {
                        rail.writing(*id, path.clone());
                    }
                    self.writing = Some((i, path, meter));
                }
                Report::Bytes { i, done, total } => {
                    if let Some(id) = self.ids.get(i) {
                        rail.progress(*id, Progress::Bytes { done, total });
                    }
                }
                Report::Detail { i, sub } => {
                    if let Some(id) = self.ids.get(i) {
                        rail.detail(*id, &sub);
                    }
                }
                Report::Done { i, outcome } => {
                    if let Some(id) = self.ids.get(i) {
                        // **What it cost, measured.** The plan said `about 21 MB`; this is what the
                        // drive actually took, and the two being one line apart is what makes the
                        // estimate checkable rather than a claim.
                        if let Outcome::Installed { allocated, .. } = &outcome {
                            rail.detail(*id, &format!("{} on disk", si(*allocated)));
                        }
                        rail.done(*id);
                    }
                    if let Some(d) = self.done.get_mut(i) {
                        *d = true;
                    }
                    self.writing = None;
                    t.completed.push(i);
                    recorded.push(outcome);
                }
                Report::Failed { i, fault } => {
                    if let Some(id) = self.ids.get(i) {
                        let attempted = self
                            .steps
                            .get(i)
                            .map(|s| format!("{} {}", s.verb(), s.what()))
                            .unwrap_or_else(|| "making an iPod".into());
                        rail.fail(*id, Failure::saying(fault.class, attempted, fault.said));
                    }
                    self.writing = None;
                }
                Report::Cancelled { i, removed } => {
                    if let Some(id) = self.ids.get(i) {
                        // **`stopped`, not `cancel`.** `Rail::cancel` is the *person pressed
                        // Cancel* route and is gated on `cancellable`, which only `Rail::writing`
                        // sets — so a stop that arrived before this step's first `Writing` report
                        // did nothing at all, and the entry sat on `Kind::Planned` after the run
                        // was over. Nothing here needs the path back either: the worker deleted its
                        // own file, and a second `unlink` from this thread is a race.
                        rail.stopped(*id);
                    }
                    let _ = removed;
                    self.writing = None;
                }
                Report::TooLate => {
                    // The drive is finished and it is theirs. Say what happened to the request
                    // rather than letting it disappear — `Rail::note` folds a repeat into one.
                    rail.note(
                        "The run finished before the cancel could take effect. Nothing was undone.",
                    );
                }
            }
        }
        // ---- what the finished steps put in the library, applied here and nowhere else.
        for outcome in &recorded {
            if self.record(settings, outcome) {
                t.library_changed = true;
            }
        }
        if t.library_changed {
            if let Err(e) = settings.save() {
                // A save that fails is a real failure with a real remedy, and swallowing it is how
                // a program says "Saved" about a save that did not happen.
                rail.failed(
                    "remember",
                    "what has been made so far",
                    Failure::saying(Class::Permission, "saving the library", format!("{e}")),
                );
                t.changed = true;
            }
        }
    }

    /// Put what a finished step produced into the library. `true` when something changed.
    fn record(&mut self, settings: &mut Settings, outcome: &Outcome) -> bool {
        match outcome {
            Outcome::Nothing => false,
            Outcome::Fetched {
                path,
                verified,
                filed_as,
            } => {
                let name = settings.file_away(
                    filed_as.resource(path.clone()),
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                        .as_str(),
                    Some(Provenance::Fetched {
                        verified: *verified,
                    }),
                );
                // **Only an installer is what a drive was built from**, and this used to be set for
                // every fetch because every fetch was one. A bootloader fetched under §11.4's verb
                // would otherwise become the `built_from` of whatever drive the next build made.
                if *filed_as == Filing::Installer {
                    self.installer = Some(name);
                }
                true
            }
            Outcome::Installed { path, .. } => {
                let Some(device) = self.device.clone() else {
                    return false;
                };
                // `remember_as` reads the LIVE drive, the same way it reads the live boot ROM.
                settings.disk = Some(path.clone());
                let stem = path
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| device.clone());
                let name = settings.file_disk(path.clone(), &stem);
                if let Some(d) = settings.disks.iter_mut().find(|d| d.name == name) {
                    d.built_from = self.installer.clone();
                    if !d.installed.iter().any(|s| s == Os::Apple.label()) {
                        d.installed.push(Os::Apple.label().to_string());
                    }
                }
                settings.remember_as(&device);
                true
            }
        }
    }

    /// §12.7. Sets the flag; the worker stops at its next step boundary and deletes its own
    /// partial file.
    ///
    /// `false` when this is not a step this queue is running — in which case the caller falls
    /// through to its own delete path, because a queue that is not running cannot stop anything.
    pub fn cancel(&mut self, id: u64) -> bool {
        if !self.owns(id) || !self.busy() {
            return false;
        }
        if let Some(c) = &self.cancel {
            c.ask();
        }
        true
    }

    /// For the way out.
    ///
    /// **It applies what the worker said before it went.** This used to throw those reports away,
    /// and a close landing between the install's rename and the next 100 ms tick therefore wrote a
    /// settings file that did not mention the drive sitting on disk — the same orphan the pump's
    /// own race produced, reached a second way. It also left the Rail drawing a step as `Working`
    /// with a live `Cancel` for a file the worker had already deleted.
    ///
    /// It writes the library through the same `apply` a tick uses — including the save, because a
    /// stop that recorded a finished drive and did not persist it is the orphan again.
    pub fn stop(&mut self, settings: &mut Settings, rail: &mut Rail) -> Stopped {
        let Some(w) = self.worker.as_mut() else {
            return Stopped::Idle;
        };
        let (how, said) = w.stop();
        let mut t = Tick {
            fraction: -1.0,
            ..Tick::default()
        };
        // **Applied before `writing` is read**, so an abandoned worker can name the file it is
        // abandoning. It could not: `temp` was sampled first, and a run whose `Writing` report had
        // not been pumped yet — a build that started between two ticks, which is the common case
        // for a window closed early — reported `Abandoned { temp: None }` about a file with an
        // 8 GiB apparent size sitting in the drives directory.
        self.apply(said, settings, rail, &mut t);
        match how {
            Stopped::Abandoned { .. } => Stopped::Abandoned {
                temp: self.writing.as_ref().map(|(_, p, _)| p.clone()),
            },
            other => other,
        }
    }
}

/// [`Queue::resume_from`]'s table, as a function of the three facts it reads.
///
/// **Pure, because two of its four answers were reachable by no test.** `resume_from` needs a
/// bundle that verifies against a recorded SHA-256 to answer 2 or 4, and nothing but Apple's own
/// 6 533 633 bytes will do that — so the two rows the `.part`-then-rename discipline exists for
/// were exercised only by a test that reaches a third party. What the three booleans mean is
/// checked elsewhere: `is_cached` by `a_file_of_the_right_length_is_not_a_cached_release`,
/// `names_a_disk` by `a_device_with_no_drive_is_unfinished_rather_than_broken`, `minted` by the
/// identity tests. This is the decision they feed, and it is the part that had no coverage.
///
/// | step | done when |
/// |---|---|
/// | 0 synthesise | the library holds a minted iPod |
/// | 1 fetch | the release is in the cache **and still verifies** |
/// | 2 build | never — its only output is a `.part`, which has no real name |
/// | 3 install | the minted device names a drive and nothing about it is missing |
/// | 4 start | never — pressing again starts the machine again |
fn resume_step(minted: bool, cached: bool, finished: bool) -> usize {
    if !minted {
        return 0;
    }
    if !cached {
        return 1;
    }
    if !finished {
        // **Step 2 never ticks and step 3 is what ticks the pair.** A build killed at 40 % leaves
        // no drive with a real name for a resume to adopt, so it runs again — without
        // re-downloading the 6.5 MB that is already verified on disk.
        return 2;
    }
    4
}

/// A device name nobody in this library is already using.
///
/// **`Settings::remember_as` replaces a device of the same name.** So a first run that took
/// `My 5.5G` unconditionally would silently overwrite a device the operator had made by hand and
/// happened to call the same thing — `AGENTS.md` §3's rule, applied to a list entry rather than to
/// a file. `Settings::unique_name` does exactly this for resources and disks and is private, so
/// this is the same rule spelled once more for devices.
fn free_device_name(settings: &Settings, base: &str) -> String {
    let taken = |n: &str| settings.devices.iter().any(|d| d.name == n);
    if !taken(base) {
        return base.to_string();
    }
    for n in 2..10_000u32 {
        let candidate = format!("{base} ({n})");
        if !taken(&candidate) {
            return candidate;
        }
    }
    format!("{base} ({})", settings::now_unix())
}

/// A volume that would not take the file, said in the words of what was observed.
///
/// **Not §9.3's FAT32 paragraph.** Nothing here read a filesystem type; it asked for a file of that
/// size and was refused. Asserting FAT32 would be the program stating a fact about somebody's disk
/// it did not observe — and it would be wrong on every other filesystem with a size ceiling.
fn volume_refusal(dir: &Path, at: u64, why: &str) -> Failure {
    Failure::saying(
        Class::Volume,
        "making an iPod",
        // The article is a fact about the rendered figure rather than about the unit — `a 21 MB
        // file`, `an 8.6 GB file` — and one `format!` produces both, so it cannot be written in.
        format!(
            "{} would not take {} {} file — {why}. A drive image is one file that size, so it \
             cannot be built there.",
            dir.display(),
            eapp_loader::article(at),
            si(at)
        ),
    )
}

/// **The gate, as a pure function**, so both arms are testable with no filesystem.
///
/// `need` is the MATERIALISED estimate — `cost(holes).disk` — and never a sparse file's apparent
/// length. That confusion is what refused somebody with 4.1 GB free on a machine with sixteen times
/// the room the build needs, and the refusal was wrong.
///
/// **`space` of `None` returns `None`: an unmeasured volume never refuses.** A permission, a
/// missing tool or an unparseable line is not an observation about somebody's disk.
pub fn gate(need: u64, space: Option<&volume::Space>, dir: &Path) -> Option<Failure> {
    let s = space?;
    let want = need.saturating_add(HEADROOM);
    if s.free >= want {
        return None;
    }
    Some(Failure::saying(
        Class::SpacePreflight,
        "making an iPod",
        // **It says the number it actually applied.** It used to quote `need` alone — so a volume
        // with 51 MB free was refused by a sentence reading *needs 28 MB and has 51 MB free*, which
        // a person can check on its face and find false. A refusal that contradicts itself in its
        // own words is worse than no explanation: it makes the program look broken at the exact
        // moment it is being careful. The threshold is `need + HEADROOM` and both halves are named,
        // because the slack is a real decision and hiding it is what made the sentence a lie.
        format!(
            "{} needs {} to build in — {} for the drive and {} of room to work in — and {} has {} \
             free. Nothing has been written.",
            dir.display(),
            si(want),
            si(need),
            si(HEADROOM),
            s.mount,
            si(s.free)
        ),
    ))
}


#[cfg(test)]
mod tests {
    use super::*;
    use eapp_loader::identity::Identity;

    /// This file's own text, for the two sweeps that are about what is written rather than about
    /// what it does. Cheaper and more honest than a convention nobody enforces.
    const SOURCE: &str = include_str!("work.rs");

    /// Serialises the tests that set `IPOD_EMULATOR_DATA`.
    ///
    /// `set_var` is process-global and cargo runs tests on several threads, so two tests that both
    /// set it interleave and one reads the other's directory. That is a flake nobody can reproduce,
    /// so it is a lock rather than a convention.
    ///
    /// **It is [`crate::data_dir_lock`] and not a lock of this module's own**, which it was until a
    /// reconciliation pass. `main.rs`'s tests redirect the same variable — once, to one fixed
    /// directory — and a private lock here serialised these tests against each other while leaving
    /// them free to interleave with those. Two locks over one variable is the same as no lock: the
    /// flake it produced was a ledger test reporting a firmware bundle nobody had downloaded,
    /// because it read one of this module's scratch caches.
    use crate::data_dir_lock as env_lock;

    /// A fresh directory nobody else is using. **Never the operator's data directory**: every test
    /// here files devices and writes drives, and landing one of those in a real library is the
    /// destructive mistake `AGENTS.md` §3 is about.
    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "ipod-work-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).expect("a scratch directory");
        d
    }

    /// Point the data directory somewhere disposable for the length of one test.
    ///
    /// `pub(super)` because `offline_worker_tests` presses the button over a **data directory**
    /// rather than over a `Settings` built in memory, and a second copy of this would be a second
    /// answer to *where does a test's library live*.
    pub(super) struct DataDir {
        _guard: crate::DataDirLock,
        pub at: PathBuf,
    }

    impl DataDir {
        pub(super) fn new(name: &str) -> DataDir {
            // Taking the lock is also what redirects `IPOD_EMULATOR_DATA` away from the operator's
            // real library, once per binary — so by the time this line returns, the variable is
            // already pointing somewhere disposable and the override below only narrows it further.
            let guard = env_lock();
            let at = scratch(name);
            // SAFETY: `env_lock` serialises every test in this binary that touches this variable,
            // and it is put back when this is dropped.
            unsafe { std::env::set_var("IPOD_EMULATOR_DATA", &at) };
            DataDir { _guard: guard, at }
        }
    }

    impl Drop for DataDir {
        fn drop(&mut self) {
            // **The variable is put back by `DataDirLock`'s own `Drop`, which runs after this.**
            //
            // This used to save and restore the previous value itself, which is the obvious thing
            // and is wrong: on a run where this was the first test to touch the variable, the
            // previous value was *nothing*, so the restore **unset** it — and the next test to call
            // `wire` resolved `data_dir()` to the platform application-support directory and wrote
            // to the operator's real library. `AGENTS.md` §3, reached by a `Drop` impl being tidy.
            let _ = std::fs::remove_dir_all(&self.at);
        }
    }

    /// The FireWire GUID of the one iPod in this library, or `None` if there is no device.
    ///
    /// **The GUID and not the seed**, because the GUID is what `sysinfo_t` carries and what iTunes
    /// binds DRM to. Two devices with different seeds and the same GUID would still be one iPod;
    /// this asks the question the hardware asks.
    fn guid_of(s: &Settings) -> Option<u64> {
        let d = s.devices.first()?;
        s.nor_of(d)?.identity().ok().map(|i: Identity| i.guid)
    }

    /// A directory that cannot be created, because a regular file is in the way. The one refusal
    /// this test suite can produce on demand, on every platform, without asking for a permission
    /// it does not have.
    fn blocked(dir: &Path) -> PathBuf {
        let f = dir.join("in-the-way");
        std::fs::write(&f, b"not a directory").expect("the blocker");
        f.join("drives")
    }

    // ── the identity ────────────────────────────────────────────────────────────────────────────

    /// **THE test.** §10.3, and §19.2's finding verbatim: three failed first runs left three iPods
    /// with three different FireWire GUIDs.
    ///
    /// Three presses, each with a fresh [`Queue`] and a `Settings` re-read from disk — three
    /// launches, as far as the program can tell. Every one of them fails, at the volume probe,
    /// **after** the identity has been minted, which is the corner this is about. One iPod, one
    /// GUID, one device.
    #[test]
    fn three_failed_first_runs_leave_one_ipod_with_one_guid() {
        let data = DataDir::new("three-failures");
        let drives = blocked(&data.at);
        let cache = data.at.join("firmware");

        let mut guids = Vec::new();
        for attempt in 1..=3 {
            let mut settings = Settings::load();
            let mut rail = Rail::new();
            let mut q = Queue::at(drives.clone(), cache.clone());
            match q.press(&mut settings, &mut rail, true) {
                Press::Refused(f) => assert_eq!(
                    f.class,
                    Class::Permission,
                    "attempt {attempt} was refused for the wrong reason: {}",
                    f.said
                ),
                other => panic!("attempt {attempt} was not refused: {other:?}"),
            }
            settings.save().expect("the scratch data directory is writable");
            let in_the_library = guid_of(&settings).expect("a device was made");
            guids.push(in_the_library);
            assert_eq!(
                settings.devices.len(),
                1,
                "attempt {attempt} left {} devices",
                settings.devices.len()
            );
            // **And the iPod the queue is holding is the one in the library.** Without this the
            // test passes on the strength of a second guard — the library is only written when
            // nothing is minted yet — and would stay green with the mint made unconditional.
            let in_hand = q
                .rom
                .as_ref()
                .expect("the press minted or reused one")
                .identity()
                .expect("a synthesised identity")
                .guid;
            assert_eq!(
                in_hand, in_the_library,
                "attempt {attempt} is holding a different iPod from the one in the library"
            );
        }

        let one: std::collections::BTreeSet<u64> = guids.iter().copied().collect();
        assert_eq!(
            one.len(),
            1,
            "three failed first runs left {} iPods with {} FireWire GUIDs: {:x?}",
            one.len(),
            one.len(),
            guids
        );
        assert_ne!(guids[0], 0, "the minted identity is the never-chosen default");
    }

    /// **The corner §10.3's whole argument turns on**: a run that persisted nothing must not
    /// re-mint.
    ///
    /// The queue survives; the library does not. So on the second press `minted()` finds nothing
    /// and the only thing standing between this person and a second iPod is that the ROM was stored
    /// before the save. **A save that fails leaves exactly this state**, and this reproduces it by
    /// handing each press a fresh empty `Settings`.
    ///
    /// **The note here used to say the real thing could not be reached** — *that variable is read
    /// by every test in this binary, and making saving fail for all of them made another test flaky
    /// once in twelve runs* — and `crate::data_dir_lock` is what made that false: one re-entrant
    /// lock over `IPOD_EMULATOR_DATA` for the whole binary, so a test holding it can point the
    /// directory at something that cannot be written.
    /// [`a_press_whose_save_fails_still_holds_the_ipod_it_minted`] does exactly that, and this
    /// stays as the cheap sibling: same corner, no environment, three presses of one queue.
    #[test]
    fn a_press_that_persisted_nothing_does_not_re_mint_the_identity() {
        let data = DataDir::new("persisted-nothing");
        let mut q = Queue::at(blocked(&data.at), data.at.join("firmware"));
        let mut first = None;
        for attempt in 1..=3 {
            let mut settings = Settings::default();
            let mut rail = Rail::new();
            match q.press(&mut settings, &mut rail, true) {
                Press::Refused(f) => assert_eq!(f.class, Class::Permission, "attempt {attempt}: {}", f.said),
                other => panic!("attempt {attempt} was not refused: {other:?}"),
            }
            let guid = q
                .rom
                .as_ref()
                .expect("the identity is minted before anything that can fail")
                .identity()
                .expect("a synthesised identity")
                .guid;
            match first {
                None => first = Some(guid),
                Some(f) => assert_eq!(
                    f, guid,
                    "attempt {attempt} minted a second iPod for a run that persisted nothing"
                ),
            }
        }
    }

    /// **THE ordering, observed.** A save that fails leaves the iPod that was already minted, and
    /// the next press resumes with it rather than making a second one.
    ///
    /// [`a_press_that_persisted_nothing_does_not_re_mint_the_identity`]'s own note said this could
    /// not be reached — *"rather than by pointing the process-wide data directory at something
    /// unwritable: that variable is read by every test in this binary"* — and that was true when it
    /// was written and is not true now. `crate::data_dir_lock` is one lock over that variable for
    /// the whole binary, it is re-entrant, and `DataDir` holds it: while this test runs, **no other
    /// test in this binary can be reading `IPOD_EMULATOR_DATA`**, so it can point at something that
    /// cannot be written and put it back on the way out. The flake that argument was made about was
    /// two locks over one variable, which is the thing that lock exists to have fixed.
    ///
    /// **The refusal is checked by `attempted`, not by class.** `Class::Permission` is what the
    /// volume probe refuses with too, so a press that failed one step earlier than intended would
    /// look identical — and one step earlier is *before* the store, which is the whole question.
    ///
    /// This is what [`press_names_no_way_out_above_the_store_but_the_two_that_mint_nothing`] used
    /// to stand in for. `settings.save()` is the first thing in `press` that can fail, so a store
    /// that happens before it happens before all four of the others as well.
    #[test]
    fn a_press_whose_save_fails_still_holds_the_ipod_it_minted() {
        let data = DataDir::new("save-refused");
        // `Settings::save` opens with `create_dir_all(data_dir())`, so a regular file where the
        // directory should be refuses it on every platform without asking for a permission this
        // suite does not have — `blocked`'s trick, pointed at the data directory instead of at a
        // drives directory.
        let wall = blocked(&data.at);
        // SAFETY: `DataDir` holds `crate::data_dir_lock`, which serialises every test in this
        // binary that touches this variable, and `DataDirLock`'s own `Drop` puts it back.
        unsafe { std::env::set_var("IPOD_EMULATOR_DATA", &wall) };
        assert!(
            Settings::default().save().is_err(),
            "the fixture's data directory is writable, so this test is not about a failed save"
        );

        let mut q = Queue::at(data.at.join("drives"), data.at.join("firmware"));
        let mut first = None;
        for attempt in 1..=3 {
            // A fresh `Settings` each time, because that is what a save that failed leaves: the
            // library on disk never learned about the iPod.
            let mut settings = Settings::default();
            let mut rail = Rail::new();
            match q.press(&mut settings, &mut rail, true) {
                Press::Refused(f) => {
                    assert_eq!(f.class, Class::Permission, "attempt {attempt}: {}", f.said);
                    assert_eq!(
                        f.attempted, "remembering the iPod that was just made",
                        "attempt {attempt} was refused somewhere other than the save, so this \
                         proves nothing about the ordering: {f:?}"
                    );
                }
                other => panic!("attempt {attempt}: a save that cannot happen went through: {other:?}"),
            }
            let guid = q
                .rom
                .as_ref()
                .expect("the identity is stored before the save that just failed")
                .identity()
                .expect("a synthesised identity")
                .guid;
            assert_ne!(guid, 0, "attempt {attempt} is holding the never-chosen default");
            match first {
                None => first = Some(guid),
                Some(f) => assert_eq!(
                    f, guid,
                    "attempt {attempt} minted a second iPod after a save that failed"
                ),
            }
        }
    }

    /// **Nothing leaves `press` above the store except the two that mint nothing** — a source-order
    /// lock, which is the whole of what it claims.
    ///
    /// **It cannot fail on behaviour**, and it names calls by their spelling: rename one and its
    /// clause measures nothing. That was silent until 2026-08-22, because a needle it could not
    /// find was `unwrap_or(usize::MAX)` — *infinitely late*, which compares as fine. Measured:
    /// writing `Worker::spawn (plan, …)` with one extra space, which changes nothing at all, left
    /// this **green** while it had stopped checking anything about the spawn. Each needle is now
    /// required to be there, so a rename goes red saying which one went, rather than quietly
    /// shrinking the sweep.
    ///
    /// [`a_press_whose_save_fails_still_holds_the_ipod_it_minted`] is the behavioural half and is
    /// the one that watches: it makes the save fail and presses again. This is kept beside it for
    /// the half behaviour cannot reach — **a way out that does not exist yet**. A byte-offset
    /// comparison against a hand-written list of calls is blind to the fifth thing somebody adds,
    /// and the thing most likely to be added is an early `return`, which is a way of not reaching
    /// the store rather than a call that can fail. So: nothing that leaves this function may appear
    /// above the line that stores the ROM, except the two that are deliberately there — a run
    /// already in flight, which mints nothing because there is already one, and no `curl`, which
    /// mints nothing because the whole plan is refused before it starts.
    #[test]
    fn press_names_no_way_out_above_the_store_but_the_two_that_mint_nothing() {
        let body = SOURCE
            .split("pub fn press(")
            .nth(1)
            .expect("press is in this file")
            .split("fn identity(")
            .next()
            .expect("identity follows it");
        let stored = body.find("self.rom = Some(").expect("press stores the minted ROM");
        for after in ["settings.save()", "volume::probe(", "Plan::of(", "gate(", "Worker::spawn("] {
            // **Required, not optional.** A needle that is not there used to read as *infinitely
            // late* and pass, so a renamed call silently dropped out of the sweep.
            let at = body.find(after).unwrap_or_else(|| {
                panic!(
                    "`press` no longer names `{after}`, so this sweep had stopped checking it — \
                     rename it here too, or drop it from the list on purpose"
                )
            });
            assert!(
                stored < at,
                "`self.rom` is stored after `{after}` — a failure there would mint a second iPod \
                 on the next press"
            );
        }

        // Every `return` above the store. Read as a statement rather than as a line, because both
        // of the ones that belong there wrap.
        let code: String = body[..stored]
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let ways_out: Vec<&str> = code
            .match_indices("return ")
            .map(|(i, _)| &code[i..(i + 200).min(code.len())])
            .collect();
        let allowed = ["Press::Busy", "Class::ToolMissing"];
        for way in &ways_out {
            assert!(
                allowed.iter().any(|a| way.contains(a)),
                "`press` leaves before the identity is stored, and it is not one of the two ways \
                 out that mint nothing:\n  {}",
                way.lines().next().unwrap_or(way)
            );
        }
        assert_eq!(
            ways_out.len(),
            2,
            "`press` has {} ways out above the store; there are two, and both are deliberate",
            ways_out.len()
        );
    }

    /// A press with no `curl` refuses, names the command, and **mints nothing**.
    #[test]
    fn a_press_this_build_cannot_run_mints_nothing() {
        let data = DataDir::new("no-curl");
        let mut settings = Settings::default();
        let mut rail = Rail::new();
        let mut q = Queue::at(data.at.join("drives"), data.at.join("firmware"));
        match q.press(&mut settings, &mut rail, false) {
            Press::Refused(f) => {
                assert_eq!(f.class, Class::ToolMissing(Tool::Curl));
                assert!(
                    f.class.mono_remedy().contains("install"),
                    "no command to paste: {:?}",
                    f.class.mono_remedy()
                );
            }
            other => panic!("a build with no curl started anyway: {other:?}"),
        }
        assert!(q.rom.is_none(), "an iPod was minted for a run that cannot happen");
        assert!(settings.devices.is_empty(), "a device was filed for it too");
    }

    // ── the plan ────────────────────────────────────────────────────────────────────────────────

    /// §10.1's five rows, and the numbers on them.
    #[test]
    fn the_plan_is_five_steps_with_one_number_per_axis() {
        let steps = plan(Holes::Sparse);
        assert_eq!(
            steps.iter().map(|s| s.verb()).collect::<Vec<_>>(),
            ["synthesise", "fetch", "build", "install", "start"],
            "the five steps are not the five §10 names"
        );
        for s in &steps {
            assert!(!s.what().trim().is_empty(), "{:?} has no subject", s.verb());
            assert!(!s.sub().trim().is_empty(), "{} has no detail line", s.verb());
        }
        let c = cost(Holes::Sparse);
        assert_eq!(si(c.down), "6.5 MB", "the download figure moved: {}", c.down);
        assert_eq!(si(c.disk), "28 MB", "the disk figure moved: {}", c.disk);
        // **8 GiB appears exactly once**, on the drive's own row, where it is a fact about the file
        // rather than a bill.
        let quoted: Vec<&str> = steps
            .iter()
            .filter(|s| s.sub().contains("8 GiB"))
            .map(|s| s.verb())
            .collect();
        assert_eq!(quoted, ["build"], "8 GiB is quoted on {quoted:?}");
    }

    /// **The plan and the Rail are one list.** It fails the moment anybody composes a step's
    /// string in the window instead of reading it off the model.
    #[test]
    fn the_plan_is_the_same_list_before_the_press_and_while_it_runs() {
        let steps = plan(Holes::Sparse);
        let mut rail = Rail::new();
        let mut q = Queue::at(PathBuf::from("/nowhere"), PathBuf::from("/nowhere"));
        q.show(&mut rail, &steps);

        assert_eq!(rail.entries().len(), steps.len());
        for (e, s) in rail.entries().iter().zip(steps.iter()) {
            assert_eq!(e.verb, s.verb(), "verb mismatch");
            assert_eq!(e.what, s.what(), "subject mismatch");
            assert_eq!(e.sub, s.sub(), "sub mismatch on the {} step", s.verb());
            assert_eq!(e.kind, Kind::Planned);
            assert!(
                e.fraction() < 0.0,
                "a planned step has a fraction, so a bar is drawn for work that has not started"
            );
            assert!(!e.cancellable, "a plan nobody agreed to offers a cancel");
            assert!(!e.dismissible, "a plan nobody agreed to can be dismissed");
        }
        // Idempotent: showing it twice does not file ten entries.
        q.show(&mut rail, &steps);
        assert_eq!(rail.entries().len(), steps.len(), "the plan was filed twice");
    }

    /// Which steps the worker owns, and it is **total over every verb** — a seventh stops this
    /// file compiling until somebody has said what happens to it.
    #[test]
    fn the_worker_runs_exactly_three_of_the_six_verbs() {
        let mut runs = Vec::new();
        for v in Verb::ALL {
            let s = Step {
                kind: v,
                what: String::new(),
                sub: String::new(),
                cost: Cost::NONE,
            };
            if job_for(&s).is_some() {
                runs.push(v.as_str());
            }
        }
        assert_eq!(
            runs,
            ["fetch", "build", "install"],
            "the worker's share of the plan changed"
        );
    }

    // ── resume ──────────────────────────────────────────────────────────────────────────────────

    /// **A retry resumes; it does not restart.** With an iPod already minted, the next press starts
    /// at the fetch — the identity is not made a second time.
    #[test]
    fn a_retry_resumes_from_the_first_unticked_step() {
        let data = DataDir::new("resume");
        let cache = data.at.join("firmware");
        let q = Queue::at(data.at.join("drives"), cache.clone());

        let mut settings = Settings::default();
        assert_eq!(
            q.resume_from(&settings),
            0,
            "a library with no iPod in it resumed past the synthesise step"
        );

        // Mint one, the way `press` does.
        let rom = nor::Source::Synthetic {
            model: nor::DEFAULT_MODEL.into(),
            seed: 4242,
            serial: None,
            guid: None,
            splash: None,
        };
        settings.nor = rom.clone();
        settings.file_away(Resource::Firmware(rom), "an iPod", None);
        settings.remember_as("My 5.5G");
        assert!(minted(&settings).is_some(), "the fixture did not mint anything");
        assert_eq!(
            q.resume_from(&settings),
            1,
            "a second press would synthesise a second iPod"
        );

        // With the bundle in the cache and verified, the fetch is ticked too — but the build never
        // is, because its only output is a `.part` with no real name.
        let rel = release().expect("a release in the catalogue");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join(rel.file), vec![0u8; rel.bytes as usize]).unwrap();
        assert_eq!(
            q.resume_from(&settings),
            1,
            "a file of the right LENGTH was accepted as the release; nothing checked the hash"
        );
    }

    /// A drive that is finished takes the run to the boot, and nowhere else.
    #[test]
    fn a_finished_device_resumes_at_the_boot() {
        let data = DataDir::new("finished");
        let cache = data.at.join("firmware");
        let q = Queue::at(data.at.join("drives"), cache.clone());
        let mut settings = Settings::default();
        let rom = nor::Source::Synthetic {
            model: nor::DEFAULT_MODEL.into(),
            seed: 99,
            serial: None,
            guid: None,
            splash: None,
        };
        settings.nor = rom.clone();
        settings.file_away(Resource::Firmware(rom), "an iPod", None);
        let img = data.at.join("my-5.5g.img");
        std::fs::write(&img, b"a drive").unwrap();
        settings.disk = Some(img.clone());
        settings.file_disk(img, "my-5.5g");
        settings.remember_as("My 5.5G");

        // The fetch is not done — no bundle — so the run resumes there rather than at the boot.
        assert_eq!(q.resume_from(&settings), 1);
    }

    // ── the gate ────────────────────────────────────────────────────────────────────────────────

    /// **The gate is against the materialised estimate, never a sparse file's apparent length.**
    ///
    /// Somebody with 4.1 GB free was refused on a machine with sixteen times the room the build
    /// needs, because the figure compared against was 8 GiB — the length of a file that costs
    /// 21 MB.
    #[test]
    fn the_free_space_gate_is_against_the_materialised_estimate() {
        let dir = Path::new("/drives");
        let four_gig = volume::Space {
            free: 4_100_000_000,
            mount: "/".into(),
        };
        let need = cost(Holes::Sparse).disk;
        assert!(
            gate(need, Some(&four_gig), dir).is_none(),
            "somebody with 4.1 GB free was refused a {} build",
            si(need)
        );
        // And the apparent size is what the wrong version compared, so it must still refuse.
        let apparent = cost(Holes::Sparse).apparent.expect("the drive's own length");
        let f = gate(apparent, Some(&four_gig), dir).expect("8.6 GB does not fit in 4.1 GB");
        assert_eq!(f.class, Class::SpacePreflight);
        assert!(
            f.said.contains("Nothing has been written"),
            "a pre-flight refusal did not say that nothing was written: {}",
            f.said
        );
        assert!(f.said.contains("4.1 GB"), "the refusal does not say what is free: {}", f.said);
    }

    /// **A refusal a person can check on its face, and cannot find false.**
    ///
    /// §9.3's whole shape: the failure says what happened in numbers somebody can read. This one
    /// said *needs 28 MB and /Volumes/X has 51 MB free. Nothing has been written.* — and then
    /// refused. Found by filling a 220 MB volume and pressing the button, which is the only way it
    /// was ever going to be found: every test of this gate checked the **verdict**, and the verdict
    /// was right. It is the sentence that was wrong, and a sentence a person can disprove by
    /// reading it is worse than none — it makes the program look broken at the moment it is being
    /// careful.
    ///
    /// The threshold is `need + HEADROOM`, so that is the figure it has to quote.
    #[test]
    fn a_refusal_for_space_is_arithmetic_a_person_can_check() {
        let need = cost(Holes::Sparse).disk;
        // Room for the build itself, and not for the slack — the exact band where the old sentence
        // read as self-contradictory.
        let awkward = volume::Space {
            free: need + HEADROOM / 2,
            mount: "/Volumes/Small".into(),
        };
        let f = gate(need, Some(&awkward), Path::new("/drives")).expect("refused, correctly");

        let want = need + HEADROOM;
        assert!(
            f.said.contains(&si(want)),
            "the refusal never says the figure it actually applied ({}), so a reader is left with \
             two numbers that do not explain it: {}",
            si(want),
            f.said
        );
        assert!(
            f.said.contains(&si(awkward.free)),
            "the refusal does not say what is free: {}",
            f.said
        );
        // **The check itself**: the largest figure in the sentence must be the threshold, not the
        // free space. If the free space is the biggest number quoted, the sentence reads as *it has
        // more than enough, and I refused anyway*.
        assert!(
            want > awkward.free,
            "the fixture is not in the band this is about: {} vs {} free",
            si(want),
            si(awkward.free)
        );
        assert!(
            f.said.contains("Nothing has been written"),
            "a pre-flight refusal did not say that nothing was written: {}",
            f.said
        );
    }

    /// **The gate's producer, not just its arithmetic.**
    ///
    /// `the_free_space_gate_is_against_the_materialised_estimate` exercises `gate` as a pure
    /// function; what supplies `need` in production is `Plan::needs`, and nothing exercised that.
    /// Changing one line of it back to `s.cost.apparent.unwrap_or(s.cost.disk)` — the exact
    /// regression §10.1 is written about — left the whole suite green while a 500 MB volume with
    /// room for the build was refused with *needs 8.7 GB*.
    #[test]
    fn what_has_to_be_free_is_the_materialised_estimate_and_never_the_apparent_one() {
        let dir = std::env::temp_dir().join(format!("ipod-needs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        let p = Plan::of("needs", &dir, &dir, 0, Holes::Sparse, release()).expect("a plan");

        let apparent = ipsw::DEFAULT_SECTORS * 512;
        let materialised = cost(Holes::Sparse).disk;
        assert!(apparent > materialised * 100, "the fixture cannot tell the two apart");

        let needs = p.needs();
        assert!(!needs.is_empty(), "a plan that needs nothing free");
        for (where_, need) in &needs {
            assert!(
                *need <= materialised,
                "{}: the gate is asked for {} — the plan's whole materialised cost is {}, and the \
                 apparent size of a sparse file is {}",
                where_.display(),
                si(*need),
                si(materialised),
                si(apparent)
            );
            assert_ne!(*need, apparent, "{}: billed the sparse file's length", where_.display());
        }
        assert_eq!(
            needs.iter().map(|(_, n)| *n).sum::<u64>(),
            materialised,
            "the split across volumes does not add up to the plan's own cost"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Every row of the resume table**, including the two that no test could reach.
    ///
    /// `resume_from` answers 2 or 4 only when a bundle in the cache verifies against a recorded
    /// SHA-256, and nothing but Apple's own 6 533 633 bytes does — so the `.part`-then-rename
    /// discipline this table exists for was exercised by one `#[ignore]`d test and by nothing else.
    /// What the three booleans mean is checked elsewhere; this is the decision they feed.
    #[test]
    fn the_resume_table_has_all_four_answers() {
        // minted, cached, finished
        assert_eq!(resume_step(false, false, false), 0, "no iPod: synthesise");
        assert_eq!(resume_step(false, true, true), 0, "no iPod outranks everything");
        assert_eq!(resume_step(true, false, false), 1, "no bundle: fetch");
        // **The two that had no coverage.**
        assert_eq!(
            resume_step(true, true, false),
            2,
            "a build killed part way leaves no drive with a real name, so it runs again — and it \
             must NOT re-download the 6.5 MB that is already verified on disk"
        );
        assert_eq!(
            resume_step(true, true, true),
            4,
            "a finished device resumes at the boot and nowhere else"
        );
        // Step 3 is never an answer: the install is what ticks the build/install pair, so a resume
        // that reached the install without the build would have Apple's bytes in nobody's hand.
        for m in [false, true] {
            for c in [false, true] {
                for f in [false, true] {
                    assert_ne!(resume_step(m, c, f), 3, "{m} {c} {f}");
                }
            }
        }
    }

    /// **An unmeasured volume never refuses.** `None` is "not measured", not "none free".
    #[test]
    fn an_unmeasurable_volume_does_not_refuse() {
        assert!(
            gate(u64::MAX, None, Path::new("/drives")).is_none(),
            "a volume nothing could measure refused a build"
        );
    }

    /// A volume that will not take the file is the `volume` class, and **the sentence does not
    /// assert FAT32**, which nothing observed.
    #[test]
    fn a_volume_that_refuses_the_file_says_only_what_was_observed() {
        let f = volume_refusal(
            Path::new("/Volumes/CAMERA"),
            8_589_934_592,
            "File too large (os error 27)",
        );
        assert_eq!(f.class, Class::Volume);
        assert!(f.said.contains("8.6 GB"), "{}", f.said);
        assert!(f.said.contains("File too large"), "the OS's own words are gone: {}", f.said);
        assert!(
            !f.said.contains("FAT32"),
            "the refusal asserts a filesystem type nothing here read: {}",
            f.said
        );
    }

    // ── temp, then rename ───────────────────────────────────────────────────────────────────────

    /// **`.part` is appended, never substituted.** `set_extension` would turn `my-5.5g.img` into
    /// `my-5.5g.part`, which is a different drive as far as anything reading the directory is
    /// concerned — and the rename at the end would then leave both.
    #[test]
    fn the_partial_file_is_the_real_name_with_part_after_it() {
        let p = part_of(Path::new("/drives/my-5.5g.img"));
        assert_eq!(p, Path::new("/drives/my-5.5g.img.part"));
        assert!(p.to_string_lossy().ends_with(".part"));
    }

    /// **Cancelling deletes our own temporary file and nothing else.**
    ///
    /// A source sweep, because the behavioural test cannot reach every branch offline and the
    /// property is about every branch. Every path this module can unlink is one it created in this
    /// run and named in [`Report::Writing`] before a byte went into it.
    #[test]
    fn every_file_this_module_can_delete_is_a_part_file_of_ours() {
        let mut found = 0;
        for (n, line) in code_lines() {
            let Some(rest) = line.split("remove_file(").nth(1) else {
                continue;
            };
            found += 1;
            let arg = rest.split(')').next().unwrap_or("");
            assert!(
                arg.contains("image_part") || arg.contains("part_path"),
                "line {}: this module can delete `{arg}`, which is not a `.part` it created",
                n + 1
            );
        }
        assert!(
            found >= 2,
            "the sweep found {found} deletes, so it is not looking at the code it claims to"
        );
    }

    /// A queue that is not running cannot stop anything, and says so rather than claiming it did.
    #[test]
    fn cancelling_something_this_queue_does_not_own_is_refused() {
        let mut rail = Rail::new();
        let mut q = Queue::at(PathBuf::from("/nowhere"), PathBuf::from("/nowhere"));
        let stray = rail.note("something else entirely");
        assert!(!q.cancel(stray), "the queue claimed a step it does not own");

        q.show(&mut rail, &plan(Holes::Sparse));
        let mine = q.ids[1];
        assert!(
            !q.cancel(mine),
            "an idle queue claimed to have stopped a worker that does not exist"
        );
        let mut settings = Settings::default();
        assert_eq!(q.stop(&mut settings, &mut rail), Stopped::Idle);
    }

    // ── the drive nobody has read yet ───────────────────────────────────────────────────────────

    /// An MBR whose four partition entries say `types`, and nothing else.
    ///
    /// The same fabrication `install.rs`'s own tests use, and for the reason stated there: the
    /// thing that has to be true is *which entry got looked at*, and a real drive image has one
    /// plausible answer in one plausible slot. `[0x00, 0x0c, …]` is the layout an actual iPod has —
    /// Apple's firmware partition first, the data partition second.
    fn a_drive_whose_mbr_says(at: &Path, types: [u8; 4]) -> PathBuf {
        let mut img = vec![0u8; 512];
        for (i, t) in types.iter().enumerate() {
            img[446 + i * 16 + 4] = *t;
        }
        img[510] = 0x55;
        img[511] = 0xAA;
        std::fs::write(at, &img).expect("a fabricated drive");
        at.to_path_buf()
    }

    /// **The read does not happen on the thread that asked for it**, which is the whole reason this
    /// type exists rather than a call to `install::data_partition_type` under the picker.
    ///
    /// A `Reads::start` on a real file finishes in microseconds, so *when* it answered cannot be
    /// observed by looking. **Where** it answered can: the probe below reports the thread it ran on
    /// and the test compares that against its own. `answer` is the seam and `start` is its one
    /// production caller — `a_volume_read_answers_with_what_the_drives_mbr_says` is the other half,
    /// and it is what proves the function `start` hands it is the one that reads an MBR.
    ///
    /// **Deterministic in both worlds, and it terminates in both.** `recv()` blocks until the probe
    /// has run wherever it is going to run: on this thread, `answer` runs it before returning and
    /// the id comes back equal; on its own, the id differs. Measured red by deleting the spawn and
    /// calling `ask()` inline — *the read ran on the thread that asked for it*.
    #[test]
    fn a_volume_read_does_not_run_on_the_thread_that_asked_for_it() {
        let here = std::thread::current().id();
        let (where_tx, where_rx) = mpsc::channel();

        let mut reads = Reads::new();
        reads.answer(PathBuf::from("/drives/mine.img"), move || {
            where_tx.send(std::thread::current().id()).expect("the test is listening");
            Ok(0x0c)
        });

        let there = where_rx.recv().expect("the read never ran at all");
        assert_ne!(
            there, here,
            "the read ran on the thread that asked for it — a drive on a sleeping spindle or a \
             network mount blocks `File::open` for as long as it blocks, and §11.2's picker is a \
             control somebody is holding down"
        );

        // …and the answer comes back afterwards, tagged with the drive it is about.
        let mut landed = Vec::new();
        for _ in 0..600 {
            landed = reads.landed();
            if !landed.is_empty() {
                break;
            }
            std::thread::sleep(TICK);
        }
        assert_eq!(landed, vec![(PathBuf::from("/drives/mine.img"), Ok(0x0c))]);
        assert!(!reads.outstanding(), "a finished read is still holding the timer open");
    }

    /// **`start` reads an MBR**, which is the half a probe cannot prove.
    ///
    /// `0x0C` in the second entry, because that is the drive the whole path exists for: FAT32 in
    /// its LBA form, which is what every drive off a real iPod is and what `ipodloader2` refuses.
    #[test]
    fn a_volume_read_answers_with_what_the_drives_mbr_says() {
        let dir = scratch("volume-read");
        let mut reads = Reads::new();

        let apple = a_drive_whose_mbr_says(&dir.join("off-a-real-ipod.img"), [0x00, 0x0c, 0, 0]);
        let built = a_drive_whose_mbr_says(&dir.join("built-here.img"), [0x00, 0x0b, 0, 0]);
        // A drive that is not there at all. **Not a refusal of the recipe** — the answer is an
        // `Err`, and `Composer::took_reading` leaves the verdict alone rather than failing it.
        let gone = dir.join("never-existed.img");
        for f in [&apple, &built, &gone] {
            reads.start(f.clone());
        }

        let mut said: Vec<(PathBuf, Result<u8, String>)> = Vec::new();
        for _ in 0..600 {
            said.extend(reads.landed());
            if said.len() == 3 {
                break;
            }
            std::thread::sleep(TICK);
        }
        let answer = |p: &PathBuf| {
            said.iter().find(|(f, _)| f == p).map(|(_, a)| a.clone()).expect("an answer")
        };
        assert_eq!(answer(&apple), Ok(0x0c), "a real iPod's drive did not read as 0x0C");
        assert_eq!(answer(&built), Ok(0x0b));
        assert!(
            answer(&gone).is_err(),
            "a drive that is not there answered a partition type: {:?}",
            answer(&gone)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── the boundary ────────────────────────────────────────────────────────────────────────────

    /// **No toolkit in this file.** The window is replaceable exactly as long as this holds.
    #[test]
    fn nothing_in_the_work_module_names_a_toolkit_type() {
        for banned in [
            "slint::",
            "MainWindow",
            "DeviceRow",
            "RailRow",
            "RailKind",
            "DrawerPage",
            "ComponentHandle",
            "SharedString",
        ] {
            for (n, line) in code_lines() {
                assert!(
                    !line.contains(banned),
                    "line {n}: the work module names `{banned}`"
                );
            }
        }
    }

    /// Drive the queue to a stop, or panic saying where it got stuck.
    ///
    /// `pub(super)` for `offline_worker_tests`, which drives the same queue with nothing
    /// downloaded — this is the window's 10 Hz timer and there is only one of it.
    pub(super) fn drain_to_a_stop(q: &mut Queue, settings: &mut Settings, rail: &mut Rail) -> Tick {
        // At [`TICK`], because that is the rate the window's own timer runs at: a test that drove
        // this faster would be testing a program nobody runs.
        for _ in 0..600 {
            let t = q.pump(settings, rail);
            if t.ready.is_some() || rail.failures() > 0 {
                return t;
            }
            if !q.busy() {
                // One more, to take the reports the worker filed on its way out.
                return q.pump(settings, rail);
            }
            std::thread::sleep(TICK);
        }
        panic!("the run never stopped");
    }

    // ── end to end, and it reaches Apple's servers ──────────────────────────────────────────────
    //
    // `--ignored` for the same reason `firmware_fetch.rs` is: a release build must not depend on a
    // third party being up. Run them when touching this file:
    //
    //     cargo test -p ipod-gui --bins -- --ignored --test-threads=1

    /// **The promise, end to end.** Press the button: it synthesises a boot ROM, downloads Apple's
    /// firmware from Apple, builds a drive from it and installs Apple's software onto it — and the
    /// drive that comes out boots the OS rather than Apple's flash updater.
    #[test]
    #[ignore = "reaches Apple's servers"]
    fn a_first_run_makes_an_ipod_end_to_end() {
        let data = DataDir::new("end-to-end");
        let drives = data.at.join("drives");
        let cache = data.at.join("firmware");
        let mut settings = Settings::default();
        let mut rail = Rail::new();
        let mut q = Queue::at(drives.clone(), cache.clone());

        match q.press(&mut settings, &mut rail, true) {
            Press::Running { from, embodied } => {
                assert_eq!(from, 1, "a fresh library did not start at the fetch");
                assert!(embodied, "the press that mints the iPod did not say so");
            }
            other => panic!("the first run would not start: {other:?}"),
        }
        let t = drain_to_a_stop(&mut q, &mut settings, &mut rail);
        assert_eq!(rail.failures(), 0, "{}", rail.announce());
        assert_eq!(t.ready.as_deref(), Some("My 5.5G"), "the machine was never handed off");

        // ---- one iPod, one drive, and the drive is the one the device names.
        assert_eq!(settings.devices.len(), 1);
        let d = &settings.devices[0];
        assert!(d.names_a_disk(), "the device came out with no drive");
        assert!(settings.missing(d).is_empty(), "{:?}", settings.missing(d));
        let img = settings
            .disks
            .iter()
            .find(|x| Some(&x.name) == d.disk.as_ref())
            .map(|x| x.path.clone())
            .expect("the drive is in the library");
        assert!(img.exists(), "{} is not there", img.display());
        assert!(
            img.extension().is_some_and(|e| e == "img"),
            "the drive kept a temporary name: {}",
            img.display()
        );

        // ---- it is a drive, and it will boot the OS rather than Apple's updater.
        let m = std::fs::metadata(&img).unwrap();
        assert_eq!(m.len(), ipsw::DEFAULT_SECTORS * 512, "the drive is not 8 GiB");
        let on_disk = settings::on_disk_size(&m);
        assert!(
            on_disk < compose::DRIVE_ON_DISK * 5 / 4,
            "the plan billed {} and the build cost {}",
            si(compose::DRIVE_ON_DISK),
            si(on_disk)
        );
        let st = ipsw::firmware_state(&img).expect("the firmware partition reads back");
        assert!(st.has_os, "there is no `osos` in it: {:?}", st.tags);
        assert!(!st.aupd_armed, "the first boot would run Apple's flash updater");

        // ---- nothing partial is left anywhere.
        for dir in [&drives, &cache] {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                assert!(!name.ends_with(".part"), "{name} was left behind in {}", dir.display());
            }
        }

        // ---- and the library says where it all came from.
        let disk = settings.disks.iter().find(|x| x.path == img).unwrap();
        assert!(disk.built_from.is_some(), "the drive does not say what built it");
        assert!(
            disk.installed.iter().any(|s| s == Os::Apple.label()),
            "{:?}",
            disk.installed
        );

        // ---- pressing again does not make a second one; it hands the same machine off.
        let mut again = Queue::at(drives, cache);
        match again.press(&mut settings, &mut rail, true) {
            Press::HandOff(name) => assert_eq!(name, "My 5.5G"),
            other => panic!("a second press did not hand off: {other:?}"),
        }
        assert_eq!(settings.devices.len(), 1, "a second press made a second iPod");
    }

    /// **A cancelled download leaves no bundle and no partial file.**
    ///
    /// Deterministic without being a race: the fetch is 6.5 MB from Apple's servers and the cancel
    /// goes in on the first tick, so `curl` is certainly still running. `firmware.rs` kills it
    /// within one `WATCH_TICK` and removes its own `.part` — and **removes nothing else**.
    #[test]
    #[ignore = "reaches Apple's servers"]
    fn a_cancelled_download_leaves_no_bundle_and_no_partial() {
        let data = DataDir::new("cancel-fetch");
        let drives = data.at.join("drives");
        let cache = data.at.join("firmware");
        // A file of ours that is NOT part of this run, to prove the cancel is surgical.
        std::fs::create_dir_all(&cache).unwrap();
        let bystander = cache.join("someone-elses.ipsw");
        std::fs::write(&bystander, b"not ours to delete").unwrap();

        let mut settings = Settings::default();
        let mut rail = Rail::new();
        let mut q = Queue::at(drives.clone(), cache.clone());
        let from = match q.press(&mut settings, &mut rail, true) {
            Press::Running { from, .. } => from,
            other => panic!("the run would not start: {other:?}"),
        };
        assert_eq!(from, 1, "the fixture did not start at the fetch");
        q.pump(&mut settings, &mut rail);
        assert!(q.cancel(q.ids[1]), "the queue would not stop its own download");
        drain_to_a_stop(&mut q, &mut settings, &mut rail);

        let rel = release().expect("a release");
        assert!(
            !cache.join(rel.file).exists(),
            "a cancelled download produced a bundle anyway"
        );
        assert!(
            !firmware::part_path(rel, &cache).exists(),
            "the `.part` of a cancelled download was left behind"
        );
        assert!(bystander.exists(), "cancelling deleted a file that was not ours");
        let left: Vec<String> = std::fs::read_dir(&drives)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(left.is_empty(), "a cancelled fetch wrote {left:?} to the drives directory");
        // **The iPod is kept.** A retry resumes; it does not re-mint.
        assert_eq!(settings.devices.len(), 1);
        assert_eq!(
            rail.entries()[1].kind,
            Kind::Cancelled,
            "the fetch was not filed as cancelled"
        );
    }

    /// **A cancelled build deletes its own `.part` and leaves no drive with a real name.**
    ///
    /// The flag is set before the step is called, so the check that runs after `build_volume` is
    /// reached deterministically — which a queue-level test cannot promise, because with the bundle
    /// already cached the build and the install together take under a tenth of a second.
    #[test]
    #[ignore = "reaches Apple's servers"]
    fn a_cancelled_build_deletes_its_own_partial() {
        let data = DataDir::new("cancel-build");
        let drives = data.at.join("drives");
        let cache = data.at.join("firmware");
        let rel = release().expect("a release");
        firmware::download(rel, &cache).expect("the bundle");
        std::fs::create_dir_all(&drives).unwrap();

        let plan = Plan::of("my-5.5g", &drives, &cache, 2, Holes::Sparse, release()).expect("a plan");
        let (tx, rx) = mpsc::channel();
        let cancel = Cancel::default();
        cancel.ask();
        let mut fw = None;

        match build(2, &plan, &tx, &cancel, &mut fw) {
            Err(Stop::Cancelled { removed }) => assert_eq!(
                removed.as_deref(),
                Some(plan.image_part.as_path()),
                "the build did not say which file it deleted"
            ),
            other => panic!("a build with the flag already set finished anyway: {:?}", other.is_ok()),
        }
        assert!(
            !plan.image_part.exists(),
            "{} was left behind",
            plan.image_part.display()
        );
        assert!(
            !plan.image.exists(),
            "a cancelled build left {} — a partial drive wearing a real name",
            plan.image.display()
        );
        // It said what it was writing **before** it wrote it, which is what lets a person be told
        // what cancelling costs before they press it.
        let announced = rx.try_iter().any(|r| matches!(
            r,
            Report::Writing { path, meter: Meter::OnDisk, .. } if path == plan.image_part
        ));
        assert!(announced, "the build never announced the file it was writing");
        assert!(cache.join(rel.file).exists(), "cancelling deleted the bundle");
    }

    /// The module's own code, without its comments and without this test module.
    ///
    /// **Both sweeps below read the code and not themselves.** A sweep that matches its own
    /// description of what it is looking for is an instrument reporting its own reflection, and
    /// this project has paid for that shape more than once.
    fn code_lines() -> Vec<(usize, &'static str)> {
        let end = SOURCE
            .lines()
            .position(|l| l.trim() == "mod tests {")
            .unwrap_or(usize::MAX);
        SOURCE
            .lines()
            .enumerate()
            .take(end)
            .filter(|(_, l)| !l.trim_start().starts_with("//"))
            .map(|(i, l)| (i + 1, l))
            .collect()
    }
}

/// **The worker's own steps, run for real, with no third party involved.**
///
/// Every test in here drives `Worker::spawn` against a synthetic `.ipsw` this module builds — a
/// zip, a firmware partition with an `!ATA` directory carrying `osos` at `LOAD_ADDR_5G`, `rsrc`
/// and an armed `aupd` — so the build, the install, the `aupd` marking, the rename, the read-back
/// and both cancellation boundaries execute on `cargo test`.
///
/// **They did not.** All six tests that reached a worker were `#[ignore]`d because they reach
/// Apple's servers, so `ipsw::write_firmware_partition`, `ipsw::mark_aupd_applied` on the worker
/// path, the `.part`-to-real-name rename, `firmware_state`'s read-back, `Report::Cancelled`,
/// `Rail::stopped`, `Worker::stop`, `Stopped::Ended` and `Stopped::Abandoned` were reached by
/// nothing a release run executes. That is the shape of every defect this program has shipped: the
/// code computes, and the platform is never asked.
///
/// The one thing still behind `#[ignore]` is the **download**, which genuinely needs a third party.
#[cfg(test)]
mod offline_worker_tests {
    use super::tests::{drain_to_a_stop, DataDir};
    use super::*;
    use eapp_loader::ipsw::{self, DIRECTORY_AT, LOAD_ADDR_5G};

    /// A firmware partition a 5G/5.5G would accept: `osos` at the load address, `rsrc`, and an
    /// **armed** `aupd`, so `mark_aupd_applied` has something to do.
    ///
    /// The layout is `images()`'s, read backwards: `!ATA`, the four tag characters stored in
    /// reverse, then `dev`, `devOffset`, `len`, `addr`, `entry` as little-endian words.
    fn firmware_partition() -> Vec<u8> {
        // Small: 512 sectors is 256 KiB, which is enough to hold the directory and the images and
        // nothing like Apple's 13.9 MB. What is being exercised is the layout, not the size.
        let mut fw = vec![0u8; 512 * 512];
        // An ARM image opens with a branch, which `image_header` looks for. Two of them, so
        // anything that goes looking finds what it expects.
        let body = 0x8000usize;
        fw[body + 3] = 0xEA;
        fw[body + 7] = 0xEA;
        for (i, (tag, dev, off, len, addr)) in [
            ("osos", 0u32, body as u32, 0x4000u32, LOAD_ADDR_5G),
            ("rsrc", 0, 0xC000, 0x1000, LOAD_ADDR_5G),
            // `dev: 0` is *armed*: Apple's flash updater has not run. `mark_aupd_applied` writes 1.
            ("aupd", 0, 0xE000, 0x1000, LOAD_ADDR_5G),
        ]
        .iter()
        .enumerate()
        {
            let at = DIRECTORY_AT + i * 40;
            fw[at..at + 4].copy_from_slice(b"!ATA");
            let t: Vec<u8> = tag.bytes().rev().collect();
            fw[at + 4..at + 8].copy_from_slice(&t);
            fw[at + 8..at + 12].copy_from_slice(&dev.to_le_bytes());
            fw[at + 0x0c..at + 0x10].copy_from_slice(&off.to_le_bytes());
            fw[at + 0x10..at + 0x14].copy_from_slice(&len.to_le_bytes());
            fw[at + 0x14..at + 0x18].copy_from_slice(&addr.to_le_bytes());
        }
        fw
    }

    /// A zip with one **stored** member, which is what `Zip::extract`'s method-0 arm reads.
    ///
    /// Written by hand rather than with a crate: this repository has no zip writer, the reader it
    /// has to satisfy is thirty lines away, and a dependency added for a fixture is a dependency.
    fn zip_of(name: &str, body: &[u8]) -> Vec<u8> {
        let crc = ipsw::crc32(body).to_le_bytes();
        let size = (body.len() as u32).to_le_bytes();
        let n = (name.len() as u16).to_le_bytes();
        let mut out: Vec<u8> = Vec::new();

        let local_at = out.len() as u32;
        out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]); // local file header
        out.extend_from_slice(&[20, 0, 0, 0, 0, 0]); // version, flags, method 0 (stored)
        out.extend_from_slice(&[0, 0, 0, 0]); // time, date
        out.extend_from_slice(&crc);
        out.extend_from_slice(&size); // packed
        out.extend_from_slice(&size); // unpacked
        out.extend_from_slice(&n);
        out.extend_from_slice(&[0, 0]); // extra len
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(body);

        let dir_at = out.len() as u32;
        out.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]); // central directory header
        out.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0]); // made-by, needed, flags, method 0
        out.extend_from_slice(&[0, 0, 0, 0]); // time, date
        out.extend_from_slice(&crc);
        out.extend_from_slice(&size);
        out.extend_from_slice(&size);
        out.extend_from_slice(&n);
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // extra, comment, disk
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // internal + external attrs
        out.extend_from_slice(&local_at.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        let dir_len = out.len() as u32 - dir_at;

        out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]); // end of central directory
        out.extend_from_slice(&[0, 0, 0, 0, 1, 0, 1, 0]); // disks, entries
        out.extend_from_slice(&dir_len.to_le_bytes());
        out.extend_from_slice(&dir_at.to_le_bytes());
        out.extend_from_slice(&[0, 0]); // comment len
        out
    }

    /// The bundle, the release that names it, and the plan that builds from it.
    ///
    /// **The drive is deliberately small.** `Plan::of` takes `ipsw::DEFAULT_SECTORS` — 8 GiB — and
    /// on a filesystem without holes that is 8.6 GB of real writing, which is not a thing a
    /// `cargo test` may do to somebody's machine. `build_volume` refuses anything at or under
    /// `DATA_LBA + 65_536` sectors, so this is the smallest drive this program can make.
    fn a_plan(dir: &Path) -> Plan {
        let bundle = zip_of("Firmware-25.1.3.MnOpQr.ipsw", &firmware_partition());
        let cache = dir.join("firmware");
        let drives = dir.join("drives");
        std::fs::create_dir_all(&cache).expect("a cache");
        std::fs::create_dir_all(&drives).expect("a drives directory");
        let file: &'static str = "offline-fixture.ipsw";
        std::fs::write(cache.join(file), &bundle).expect("the fixture bundle");

        let release: &'static Release = Box::leak(Box::new(Release {
            updater_family: compose::FIRST_RUN_FAMILY,
            family: 0,
            model: "iPod (5th generation)",
            variant: "offline fixture",
            file,
            // Never reached: `is_cached` answers true, so `fetch` returns before the fetcher does.
            url: "http://127.0.0.1:1/offline-fixture.ipsw",
            bytes: bundle.len() as u64,
            sha256: Some(Box::leak(
                eapp_loader::firmware::sha256(&bundle).into_boxed_str(),
            )),
            served: true,
        }));

        let image = drives.join("offline.img");
        Plan {
            steps: plan(Holes::Sparse),
            release,
            cache,
            image_part: PathBuf::from(format!("{}.part", image.display())),
            image,
            // The smallest drive this program can make: `ipsw::MIN_FAT32_SECTORS` of data
            // partition, which is 1.07 GB apparent and about 4 MB on any filesystem with holes.
            // Not `DEFAULT_SECTORS`, because 8.6 GB of real writing is not a thing `cargo test`
            // may do to somebody's machine on a filesystem without them.
            sectors: ipsw::DATA_LBA as u64 + ipsw::MIN_FAT32_SECTORS,
            from: 1,
        }
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "ipod-offline-{}-{name}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).expect("a scratch directory");
        d
    }

    /// Drain a worker to a stop, with a deadline so a wedged one fails rather than hangs.
    fn drain_worker(w: &mut Worker) -> Vec<Report> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let mut all = Vec::new();
        while std::time::Instant::now() < deadline {
            all.extend(w.drain());
            if !w.busy() {
                all.extend(w.drain());
                return all;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the worker did not finish inside two minutes; it said {all:?}");
    }

    // ── what a first run attaches ───────────────────────────────────────────────────────────────
    //
    // These drive `Queue::press` over a **data directory that starts empty**, which is the one
    // thing nothing else here does: every other press test hands `press` a `Settings::default()`
    // built in memory, so the library it works over is the library the test wrote, and the shape a
    // real launch produces — `Settings::load_and_seed` over whatever is on disk — was never once
    // the input. `library_seeded` is what makes that shape different: an installation from before
    // the device list existed has a live drive and no devices, and seeding files that drive.

    /// The settings file an installation that predates the device list leaves behind: a live drive,
    /// no devices, no library. `Settings::load_and_seed` is what carries it forward, and this is
    /// the only shape that reaches `seed_resources`'s drive-gathering arm at all.
    fn a_library_carried_forward(at: &Path, drive: &Path) -> Settings {
        std::fs::write(
            at.join("settings.txt"),
            format!("disk = {}\n", drive.display()),
        )
        .expect("a settings file");
        let s = Settings::load_and_seed();
        assert!(
            s.devices.is_empty(),
            "the fixture is not a library with no devices in it: {:?}",
            s.devices
        );
        assert_eq!(
            s.disks.len(),
            1,
            "seeding did not carry the live drive into the library, so this fixture is not the \
             shape it is meant to be"
        );
        assert_eq!(s.disk.as_deref(), Some(drive), "the live drive is not the fixture's");
        s
    }

    /// **The drive a first run attaches is the drive a first run built.** Compared by path,
    /// because a name is what the defect got right.
    ///
    /// The library here is one carried onto a machine that does not hold the drive it names —
    /// `disk.0` is a real seeded entry pointing at an image that is somewhere else, which is what
    /// every carried-forward library looks like on a second computer. The run therefore has work
    /// to do: it mints an iPod, builds a drive and installs onto it. And then it attached the
    /// **seeded** entry to the device and left the drive it had just made an orphan, because
    /// `Settings::as_device` prefers whatever a device already names and the iPod had been minted
    /// naming the live one.
    ///
    /// Nothing caught it: `a_first_run_makes_an_ipod_end_to_end` asserts the device names a drive
    /// and that the drive resolves and is 8 GiB — all true of the wrong one — and it starts from a
    /// library with nothing in it, where there is no wrong one to attach.
    #[test]
    fn a_first_run_attaches_the_drive_it_built_and_not_the_one_that_was_there() {
        let data = DataDir::new("attaches-what-it-built");
        let fixture = a_plan(&data.at);
        let drives = data.at.join("drives");
        let cache = data.at.join("firmware");

        let elsewhere = data.at.join("not-on-this-machine.img");
        let mut settings = a_library_carried_forward(&data.at, &elsewhere);
        assert!(
            !elsewhere.exists(),
            "the fixture's drive is on this machine, which is not the case this is about"
        );

        let mut rail = Rail::new();
        let mut q = Queue::fetching(drives.clone(), cache, fixture.release);
        match q.press(&mut settings, &mut rail, true) {
            Press::Running { from, embodied } => {
                assert_eq!(from, 2, "the run did not resume at the build");
                assert!(embodied, "the press that mints the iPod did not say so");
            }
            other => panic!("the first run would not start: {other:?}"),
        }
        let t = drain_to_a_stop(&mut q, &mut settings, &mut rail);
        assert_eq!(rail.failures(), 0, "the run failed: {}", rail.announce());
        assert_eq!(t.ready.as_deref(), Some("My 5.5G"), "the machine was never handed off");

        // **The drive that was written, read off the filesystem.** The library is the thing under
        // test, so the answer it is checked against cannot come out of it.
        let mut written: Vec<PathBuf> = std::fs::read_dir(&drives)
            .expect("the drives directory")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "img"))
            .collect();
        written.sort();
        assert_eq!(
            written.len(),
            1,
            "the run did not leave exactly one drive behind: {written:?}"
        );
        let built = written.remove(0);

        let d = settings.devices.first().expect("the run filed a device");
        let attached = settings
            .disk_of(d)
            .expect("the device this run made names no drive at all")
            .expect("the drive it names is not in the library");
        assert_eq!(
            attached,
            built,
            "`{}` runs {} and the run built {} — the drive this program just made is an orphan \
             and the machine boots whatever the library named before it",
            d.name,
            attached.display(),
            built.display()
        );
    }

    /// **A newly minted iPod has no drive**, and a first run that gives it one it did not make is
    /// a first run that makes nothing at all.
    ///
    /// The same carried-forward library, with the drive it names **present** — and that one
    /// difference is the whole of it. `Queue::resume_from`'s step 3 is *the minted device names a
    /// drive and nothing about it is missing*, which a device born naming somebody else's image
    /// satisfies the instant it is minted. The plan then reads as finished, `press` answers
    /// `HandOff`, and the centre button on an empty bench boots a stranger's drive having
    /// downloaded nothing, built nothing and installed nothing.
    ///
    /// `drops::land` already says the sentence this is: *a drive is built from an `.ipsw` and is
    /// not one, so there is nothing to point at yet.*
    #[test]
    fn a_first_run_mints_an_ipod_that_names_no_drive() {
        let data = DataDir::new("mints-no-drive");
        let fixture = a_plan(&data.at);
        let drives = data.at.join("drives");
        let cache = data.at.join("firmware");

        let theirs = data.at.join("somebody-elses.img");
        std::fs::write(&theirs, vec![0u8; 512]).expect("a drive image");
        let mut settings = a_library_carried_forward(&data.at, &theirs);

        let mut rail = Rail::new();
        let mut q = Queue::fetching(drives, cache, fixture.release);
        match q.press(&mut settings, &mut rail, true) {
            Press::Running { from, embodied } => {
                assert_eq!(from, 2, "the run did not resume at the build");
                assert!(embodied, "the press that mints the iPod did not say so");
            }
            Press::HandOff(name) => panic!(
                "the press built nothing and handed `{name}` to the machine: the iPod it had just \
                 minted was given {}, which the plan then read as a finished drive",
                theirs.display()
            ),
            other => panic!("the first run would not start: {other:?}"),
        }
        let born_with = minted(&settings)
            .expect("the press minted an iPod")
            .disk
            .clone();
        assert_eq!(
            born_with, None,
            "the iPod that was just minted already names a drive, and it is one this program did \
             not make"
        );

        let t = drain_to_a_stop(&mut q, &mut settings, &mut rail);
        assert_eq!(rail.failures(), 0, "the run failed: {}", rail.announce());
        assert_eq!(t.ready.as_deref(), Some("My 5.5G"), "the machine was never handed off");
        assert!(
            theirs.is_file(),
            "the run wrote over the drive that was already in the library"
        );
    }

    /// **A device whose drive has gone runs the one that replaces it.**
    ///
    /// The other end of the same preference, reached without a first run: a finished device whose
    /// image was deleted resumes at the build — `Settings::missing` reports it `Gone`, so
    /// `resume_from` answers 2 — and the install then filed the new drive and left the device
    /// naming the deleted one. Somebody presses the button because their iPod says a file is
    /// missing, waits for a drive to be built, and is told the same file is missing.
    #[test]
    fn a_rebuilt_drive_replaces_the_one_the_device_lost() {
        let data = DataDir::new("rebuilt-drive");
        let fixture = a_plan(&data.at);
        let drives = data.at.join("drives");
        let cache = data.at.join("firmware");

        // A finished device, and then its image is not there. Filed under a name of its own, so
        // that the drive the build makes cannot accidentally be filed under the same one.
        let mut settings = Settings::default();
        let rom = nor::Source::Synthetic {
            model: nor::DEFAULT_MODEL.into(),
            seed: 5_150,
            serial: None,
            guid: None,
            splash: None,
        };
        settings.nor = rom.clone();
        settings.file_away(Resource::Firmware(rom), "Black 5.5G", None);
        let lost = drives.join("the-one-that-went.img");
        settings.disk = Some(lost.clone());
        settings.file_disk(lost.clone(), "the-one-that-went");
        settings.remember_as("My 5.5G");
        assert!(!lost.exists(), "the fixture's drive is on disk");

        let mut rail = Rail::new();
        let mut q = Queue::fetching(drives.clone(), cache, fixture.release);
        match q.press(&mut settings, &mut rail, true) {
            Press::Running { from, embodied } => {
                assert_eq!(from, 2, "a device whose drive is gone did not resume at the build");
                assert!(!embodied, "a resume minted a second iPod");
            }
            other => panic!("the run would not start: {other:?}"),
        }
        let t = drain_to_a_stop(&mut q, &mut settings, &mut rail);
        assert_eq!(rail.failures(), 0, "the run failed: {}", rail.announce());
        assert_eq!(t.ready.as_deref(), Some("My 5.5G"), "the machine was never handed off");

        let d = settings.devices.first().expect("the device is still there");
        let attached = settings
            .disk_of(d)
            .expect("the device names no drive at all")
            .expect("the drive it names is not in the library");
        assert!(
            attached.is_file(),
            "`{}` still runs {}, which is the file that was missing before the build — the drive \
             the run made is filed and attached to nothing",
            d.name,
            attached.display()
        );
    }

    /// **The build and the install, for real.**
    ///
    /// A cached bundle, a drive laid out, Apple's bytes written into it, the flash updater marked
    /// applied, the partition read back, and the `.part` renamed to a real name — none of which
    /// executed on `cargo test` until now.
    #[test]
    fn the_worker_builds_a_drive_and_makes_it_bootable() {
        let dir = scratch_dir("build");
        let plan = a_plan(&dir);
        let image = plan.image.clone();
        let part = plan.image_part.clone();
        let bundle = plan.cache.join(plan.release.file);
        let mut w = Worker::spawn(plan, Cancel::new()).expect("a thread");
        let said = drain_worker(&mut w);

        let failures: Vec<&Report> = said
            .iter()
            .filter(|r| matches!(r, Report::Failed { .. }))
            .collect();
        assert!(failures.is_empty(), "the run failed: {failures:?}");

        // Three steps report `Done`: the cached fetch, the build's container, the install.
        let done: Vec<usize> = said
            .iter()
            .filter_map(|r| match r {
                Report::Done { i, .. } => Some(*i),
                _ => None,
            })
            .collect();
        assert_eq!(done, vec![1, 2, 3], "the worker did not run the three steps it owns");

        // **The rename happened, and nothing called `.part` is left.**
        assert!(image.is_file(), "the drive never took its real name");
        assert!(!part.exists(), "the partial file survived the rename");
        // The bundle is untouched — a build does not consume what it was built from.
        assert!(bundle.is_file(), "the build removed the bundle it read");

        // **The drive is bootable**, read back off the file rather than assumed from the write.
        let st = ipsw::firmware_state(&image).expect("a firmware partition");
        assert!(st.has_os, "the drive has no OS in it: {:?}", st.tags);
        assert!(
            !st.aupd_armed,
            "Apple's flash updater is still armed, so the first boot would run it instead of the \
             OS: {:?}",
            st.tags
        );
        assert!(st.tags.contains(&"osos".to_string()), "{:?}", st.tags);

        // …and the aupd sentence was said out loud, because that is what makes it checkable.
        assert!(
            said.iter().any(|r| matches!(r, Report::Detail { sub, .. } if sub.contains("updater"))),
            "nothing said the updater had been marked: {said:?}"
        );
        // **What it cost, measured** — the figure the plan's estimate is checked against.
        let allocated = said.iter().find_map(|r| match r {
            Report::Done { outcome: Outcome::Installed { allocated, .. }, .. } => Some(*allocated),
            _ => None,
        });
        assert!(allocated.is_some_and(|a| a > 0), "the install reported no cost: {allocated:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A cancel between two steps deletes our own partial file and nothing else.**
    ///
    /// The first of the worker's four boundaries: the top of each step in `run`'s loop. A stale
    /// `.part` from an abandoned run is put there deliberately, because otherwise the assertion
    /// that it is gone is true of a file that never existed.
    #[test]
    fn a_cancel_between_steps_deletes_our_partial_and_leaves_the_bundle() {
        let dir = scratch_dir("cancel-between");
        let plan = a_plan(&dir);
        let image = plan.image.clone();
        let part = plan.image_part.clone();
        let bundle = plan.cache.join(plan.release.file);
        std::fs::write(&part, b"a stale partial from a run that was abandoned").expect("the stale");

        let cancel = Cancel::new();
        cancel.ask();
        let mut w = Worker::spawn(plan, Arc::clone(&cancel)).expect("a thread");
        let said = drain_worker(&mut w);

        let removed = said.iter().find_map(|r| match r {
            Report::Cancelled { removed, .. } => Some(removed.clone()),
            _ => None,
        });
        assert_eq!(
            removed,
            Some(Some(part.clone())),
            "a cancelled run did not report deleting the partial it deleted: {said:?}"
        );
        assert!(!image.exists(), "a cancelled build left a drive with a real name");
        assert!(!part.exists(), "a cancelled build left its partial file");
        assert!(bundle.is_file(), "a cancel deleted the bundle, which is not ours to delete");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The other two boundaries, driven directly**, because no test can hit them through a
    /// worker: the build takes a couple of milliseconds and the flag would have to go true inside
    /// it. `build` and `install` are this module's own functions, so the test module calls them.
    ///
    /// Between them they are the whole of what a cancel costs: a container laid out and thrown
    /// away, and Apple's bytes written into a file that never takes a real name.
    #[test]
    fn a_cancel_after_a_write_deletes_what_was_written_and_never_names_it() {
        let dir = scratch_dir("cancel-mid");
        let plan = a_plan(&dir);
        let part = plan.image_part.clone();
        let image = plan.image.clone();
        let (tx, _rx) = mpsc::channel();

        // ---- after the container is laid out, before Apple's bytes.
        let cancel = Cancel::new();
        cancel.ask();
        let mut fw: Option<Vec<u8>> = None;
        let out = build(2, &plan, &tx, &cancel, &mut fw);
        match out {
            Err(Stop::Cancelled { removed }) => assert_eq!(
                removed,
                Some(part.clone()),
                "the build did not name the file it deleted"
            ),
            other => panic!("a build with the flag already set finished anyway: {other:?}"),
        }
        assert!(!part.exists(), "the build left its partial file behind");
        assert!(fw.is_none(), "a cancelled build handed Apple's bytes on to the install");

        // ---- after Apple's bytes are written and read back, before the rename.
        let mut fw: Option<Vec<u8>> = None;
        let quiet = Cancel::new();
        build(2, &plan, &tx, &quiet, &mut fw).expect("a build with nothing asking it to stop");
        let bytes = fw.take().expect("the firmware partition");
        assert!(part.is_file(), "the build produced no partial file to install into");
        let out = install(3, &plan, &tx, &cancel, Some(bytes));
        match out {
            Err(Stop::Cancelled { removed }) => assert_eq!(
                removed,
                Some(part.clone()),
                "the install did not name the file it deleted"
            ),
            other => panic!("an install with the flag already set finished anyway: {other:?}"),
        }
        assert!(!part.exists(), "the install left its partial file behind");
        assert!(
            !image.exists(),
            "a cancel after the write still gave the drive a real name — the rename is the one \
             moment anything stops being a `.part`, and it must be on the far side of the check"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **`Queue::cancel` really stops a worker**, and the Rail ends up saying so.
    ///
    /// The window-side branch — `if !work.cancel(id) { cancel_write(...) }` — had never taken its
    /// `true` arm in any test: the only ids it was ever given were ones the queue did not own.
    #[test]
    fn the_queue_stops_a_run_it_owns_and_the_rail_says_so() {
        // **The data directory is claimed**, because `Queue::pump` saves the library and
        // `Settings::save` writes wherever `IPOD_EMULATOR_DATA` points — which another test may be
        // moving. A save that fails becomes a `Class::Permission` failure on the Rail, and a Rail
        // with a failure on it never reports the handoff, so the flake looked like a missing
        // handoff rather than like a shared variable.
        let _data = crate::data_dir_lock();
        let dir = scratch_dir("queue-cancel");
        let plan = a_plan(&dir);
        let image = plan.image.clone();
        let part = plan.image_part.clone();

        let mut rail = Rail::new();
        let mut settings = Settings::default();
        let mut q = Queue::at(dir.join("drives"), dir.join("firmware"));
        q.show(&mut rail, &plan.steps);
        let ids = q.ids.clone();

        // The queue's own worker, started with a plan whose release is the fixture's — which is
        // exactly what `press` does once it has probed and gated.
        let cancel = Cancel::new();
        q.cancel = Some(Arc::clone(&cancel));
        q.worker = Some(Worker::spawn(plan, Arc::clone(&cancel)).expect("a thread"));
        q.device = Some("Offline".into());

        assert!(!q.cancel(999_999), "the queue claimed a step it does not own");
        assert!(q.cancel(ids[2]), "the queue refused to stop a step it is running");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while q.busy() && std::time::Instant::now() < deadline {
            q.pump(&mut settings, &mut rail);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        q.pump(&mut settings, &mut rail);

        assert!(!image.exists(), "a cancelled run left a drive with a real name");
        assert!(!part.exists(), "a cancelled run left its partial file");
        assert!(
            rail.entries().iter().any(|e| e.kind == Kind::Cancelled),
            "nothing on the Rail says the run was stopped: {:?}",
            rail.entries().iter().map(|e| (e.kind, e.verb.clone())).collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A `Done` that lands between the drain and the `busy()` check is not lost.**
    ///
    /// `busy()` reads `JoinHandle::is_finished`, so it goes false the instant the thread exits —
    /// and `pump_once` then stops the 10 Hz timer. A report stranded in that window would sit there
    /// for ever: for the install's `Done` that is a finished drive on disk the library never learns
    /// about, and the next press building `offline (2).img` beside the orphan.
    ///
    /// The fixture is the race made certain: let the worker finish completely, **then** pump once.
    /// A single-drain `pump` sees an already-dead thread and returns before reading the channel.
    #[test]
    fn a_run_that_finished_before_the_first_pump_is_still_recorded() {
        // **The data directory is claimed**, because `Queue::pump` saves the library and
        // `Settings::save` writes wherever `IPOD_EMULATOR_DATA` points — which another test may be
        // moving. A save that fails becomes a `Class::Permission` failure on the Rail, and a Rail
        // with a failure on it never reports the handoff, so the flake looked like a missing
        // handoff rather than like a shared variable.
        let _data = crate::data_dir_lock();
        let dir = scratch_dir("race");
        let plan = a_plan(&dir);
        let image = plan.image.clone();

        let mut rail = Rail::new();
        let mut settings = Settings::default();
        let mut q = Queue::at(dir.join("drives"), dir.join("firmware"));
        q.show(&mut rail, &plan.steps);
        q.device = Some("Offline".into());
        // Step 0 is the window's own, and `press` ticks it before spawning.
        q.done[0] = true;
        rail.done(q.ids[0]);
        let cancel = Cancel::new();
        q.cancel = Some(Arc::clone(&cancel));
        q.worker = Some(Worker::spawn(plan, cancel).expect("a thread"));

        // Wait for the thread to be completely gone before looking even once.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while q.busy() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!q.busy(), "the worker did not finish inside two minutes");
        assert!(image.is_file(), "the worker did not build a drive");

        let t = q.pump(&mut settings, &mut rail);
        assert_eq!(
            t.completed,
            vec![1, 2, 3],
            "one pump after the thread exited saw {:?} — the reports it sent on its way out were \
             dropped, and nothing ever drains again once the timer stops",
            t.completed
        );
        assert!(t.library_changed, "a finished drive did not reach the library");
        assert_eq!(
            settings.disks.len(),
            1,
            "the drive on disk is not in the library, so the next press builds a second one \
             beside it"
        );
        assert_eq!(t.ready.as_deref(), Some("Offline"), "the handoff was never reported");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A resume ticks the steps it is skipping, on the Rail and in its own bookkeeping.**
    ///
    /// The §10.3 case: a first run that failed at the build, relaunched. The bundle is in the cache
    /// and verifies, so `resume_from` answers 2 and the worker starts at the build — and the
    /// equal-length branch of `press` updated sub-lines and **nothing else**, so `fetch Apple's
    /// firmware` sat `Planned` for the rest of the run and `self.done` kept a hole at index 1.
    /// `first_unticked()` therefore answered 1 for ever and `Tick::ready` — §12.2's handoff, gated
    /// on it — could never fire: the drive finished, the timer stopped, and the window said nothing.
    ///
    /// The same-process retry was green throughout, because `pump` had already ticked those steps.
    /// This is a **fresh queue** on a library that already has the artefacts, which is what a
    /// relaunch is.
    #[test]
    fn a_resumed_run_ticks_what_it_skipped_and_reports_the_handoff() {
        let _data = crate::data_dir_lock();
        let dir = scratch_dir("resume");
        let fixture = a_plan(&dir);
        let release = fixture.release;
        let drives = dir.join("drives");
        let cache = dir.join("firmware");

        // A library in the state a failed build leaves: the iPod is minted and filed, and there is
        // no drive. The bundle is in the cache from `a_plan`, and it verifies.
        let mut settings = Settings::default();
        let rom = nor::Source::Synthetic {
            model: nor::DEFAULT_MODEL.into(),
            seed: 6_060_842,
            serial: None,
            guid: None,
            splash: None,
        };
        settings.nor = rom.clone();
        settings.file_away(Resource::Firmware(rom), "Black 5.5G", None);
        settings.remember_as("My 5.5G");

        let mut rail = Rail::new();
        let mut q = Queue::fetching(drives, cache, release);
        q.show(&mut rail, &plan(Holes::Sparse));
        assert_eq!(
            q.resume_from(&settings),
            2,
            "the fixture is not in the state this is about — a minted iPod, a verified bundle and \
             no drive"
        );

        match q.press(&mut settings, &mut rail, true) {
            Press::Running { from, embodied } => {
                assert_eq!(from, 2, "the run did not resume at the build");
                assert!(!embodied, "a resume minted a second iPod");
            }
            other => panic!("the press did not resume: {other:?}"),
        }

        // **Both surfaces, before a single tick.** This is the state the window draws from.
        for i in 0..2 {
            assert_eq!(
                rail.find(q.ids[i]).expect("the entry").kind,
                Kind::Done,
                "step {i} is being skipped and is still drawn as not done"
            );
            assert!(q.done[i], "step {i} is being skipped and is not ticked");
        }

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let mut ready = None;
        while std::time::Instant::now() < deadline {
            let t = q.pump(&mut settings, &mut rail);
            if t.ready.is_some() {
                ready = t.ready;
                break;
            }
            if !q.busy() {
                let t = q.pump(&mut settings, &mut rail);
                ready = t.ready;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            ready.as_deref(),
            Some("My 5.5G"),
            "every step but the boot is done and the handoff was never reported — `first_unticked` \
             is stuck behind the step the resume skipped"
        );
        assert_eq!(rail.failures(), 0, "the resumed run failed: {}", rail.announce());
        // …and nothing was fetched again: the bundle's modification time is untouched.
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A cancel that arrived too late is answered rather than swallowed.**
    ///
    /// The worker's last boundary is before the install's rename. A flag set after it lets the run
    /// finish — correctly, and the drive is theirs — while `Queue::cancel` has already returned
    /// `true` and told the person their request was accepted. Two facts, and only one of them
    /// reached the window: the drive appeared and the request vanished.
    ///
    /// **The fixture is the run with nothing left to do but the boot**, which is the state the
    /// last boundary hands over in. Waiting for a real run to reach the rename and then asking is
    /// a race the test would lose — the window between the rename and the thread returning is
    /// microseconds — and a test that passes because the scheduler was kind is worse than none.
    #[test]
    fn a_cancel_that_arrives_after_the_last_boundary_says_so() {
        let _data = crate::data_dir_lock();
        let dir = scratch_dir("too-late");
        let mut plan = a_plan(&dir);
        // Everything but the boot is done. In a real run this is where the worker is a few
        // microseconds after the install's rename, with `Queue::cancel` still answering `true`
        // because the thread has not returned yet.
        plan.from = plan.steps.len() - 1;

        let mut rail = Rail::new();
        let mut settings = Settings::default();
        let mut q = Queue::at(dir.join("drives"), dir.join("firmware"));
        q.show(&mut rail, &plan.steps);
        q.device = Some("Offline".into());
        let cancel = Cancel::new();
        cancel.ask();
        q.cancel = Some(Arc::clone(&cancel));
        q.worker = Some(Worker::spawn(plan, Arc::clone(&cancel)).expect("a thread"));

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while q.busy() && std::time::Instant::now() < deadline {
            q.pump(&mut settings, &mut rail);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        q.pump(&mut settings, &mut rail);

        assert!(
            rail.entries()
                .iter()
                .any(|e| e.kind == Kind::Note && e.what.contains("before the cancel")),
            "the request was accepted and then vanished: {:?}",
            rail.entries().iter().map(|e| (e.kind, e.what.clone())).collect::<Vec<_>>()
        );
        // Nothing was undone: a cancel this late has nothing left to stop.
        assert_eq!(rail.failures(), 0, "a late cancel produced a failure");
        assert!(
            !rail.entries().iter().any(|e| e.kind == Kind::Cancelled),
            "a run that finished was marked as stopped"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The way out records what the worker finished.**
    ///
    /// `Queue::stop` used to drain the channel looking for one `Cancelled` and throw the rest away,
    /// so a close landing between the install's rename and the next 100 ms tick wrote a settings
    /// file that did not mention the drive now on disk.
    #[test]
    fn stopping_on_the_way_out_records_a_drive_that_was_finished() {
        // **The data directory is claimed**, because `Queue::pump` saves the library and
        // `Settings::save` writes wherever `IPOD_EMULATOR_DATA` points — which another test may be
        // moving. A save that fails becomes a `Class::Permission` failure on the Rail, and a Rail
        // with a failure on it never reports the handoff, so the flake looked like a missing
        // handoff rather than like a shared variable.
        let _data = crate::data_dir_lock();
        let dir = scratch_dir("close");
        let plan = a_plan(&dir);
        let image = plan.image.clone();

        let mut rail = Rail::new();
        let mut settings = Settings::default();
        let mut q = Queue::at(dir.join("drives"), dir.join("firmware"));
        q.show(&mut rail, &plan.steps);
        q.device = Some("Offline".into());
        let cancel = Cancel::new();
        q.cancel = Some(Arc::clone(&cancel));
        q.worker = Some(Worker::spawn(plan, cancel).expect("a thread"));

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while q.busy() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(image.is_file(), "the worker did not build a drive");

        // **No pump at all**, which is the close path exactly: `on_close_requested` stops the queue
        // and then saves.
        let how = q.stop(&mut settings, &mut rail);
        assert_eq!(how, Stopped::Ended { deleted: None }, "the worker did not end cleanly");
        assert_eq!(
            settings.disks.len(),
            1,
            "the close threw away the `Done` that carries the finished drive, so the settings file \
             it writes next does not mention a drive that is on disk"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── §11.4's per-part fetch ──────────────────────────────────────────────────────────────────

    /// A Rockbox piece this test wrote, hashed here.
    ///
    /// **`Box::leak`, because a [`Want`] borrows its catalogue entry.** In the running program that
    /// entry is a `const` in `eapp-loader`; in a test it is a few bytes this function makes, and
    /// leaking one per test is what lets the offline path drive the real `Want` rather than a
    /// second type standing in for it. `a_plan` does the same thing for a `Release`.
    fn a_piece(
        file: &'static str,
        body: &[u8],
        goes: rockbox::Where,
    ) -> &'static rockbox::Piece {
        Box::leak(Box::new(rockbox::Piece {
            file,
            // Never reached by the tests that put the bytes in the cache first: `Want::cached`
            // answers true and the fetcher is not called at all.
            url: "http://127.0.0.1:1/never",
            bytes: body.len() as u64,
            sha256: Box::leak(eapp_loader::firmware::sha256(body).into_boxed_str()),
            goes,
            about: "a fixture",
        }))
    }

    /// The same, for the catalogue `ipodloader2` lives in.
    fn a_loader(file: &'static str, body: &[u8]) -> &'static ipodlinux::Piece {
        Box::leak(Box::new(ipodlinux::Piece {
            file,
            url: "http://127.0.0.1:1/never",
            bytes: body.len() as u64,
            sha256: Box::leak(eapp_loader::firmware::sha256(body).into_boxed_str()),
            about: "a fixture",
        }))
    }

    /// Pump until the queue stops, or fail rather than hang.
    fn pump_to_a_stop(q: &mut Queue, settings: &mut Settings, rail: &mut Rail) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while q.busy() && std::time::Instant::now() < deadline {
            q.pump(settings, rail);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!q.busy(), "the worker never finished");
        // The one after the loop is not a nicety: `busy()` goes false the instant the thread exits,
        // which can be after the last drain inside it.
        q.pump(settings, rail);
    }

    /// **Each download is filed in the group it belongs to**, and the group is read off the
    /// catalogue rather than assumed.
    ///
    /// The defect this is about had exactly one shape while there was one fetcher and no way to
    /// see it: `Queue::record` filed every `Outcome::Fetched` as a `Resource::Installer`, because
    /// the only thing this program could download was an `.ipsw`. Fetch a bootloader through the
    /// same channel and it lands in Apple firmware — a row in the wrong group, offering the wrong
    /// verb, with nothing downstream able to tell.
    ///
    /// Offline: the verified bytes are put in the cache first, so `Want::cached` answers true and
    /// every step is the already-here path. What that leaves under test is the whole of the window
    /// side — the plan, the worker, the reports, `apply`, `record` and the save.
    #[test]
    fn a_per_part_fetch_files_each_download_in_the_group_it_belongs_to() {
        let _data = crate::data_dir_lock();
        let dir = scratch_dir("per-part-fetch");
        let cache = dir.join("firmware");
        std::fs::create_dir_all(&cache).expect("a cache");

        let bodies: [Vec<u8>; 3] = [
            b"a bootloader, as far as this test is concerned".to_vec(),
            b"nine megabytes of Rockbox, abbreviated".to_vec(),
            b"ipodloader2, abbreviated".to_vec(),
        ];
        let wants = vec![
            Want::Rockbox(a_piece(
                "bootloader-fixture.ipod",
                &bodies[0],
                rockbox::Where::FirmwarePartition,
            )),
            Want::Rockbox(a_piece("rockbox-fixture.zip", &bodies[1], rockbox::Where::Volume)),
            Want::Loader(a_loader("loader-fixture.bin", &bodies[2])),
        ];
        for (w, body) in wants.iter().zip(bodies.iter()) {
            std::fs::write(w.lands_at(&cache), body).expect("the fixture in the cache");
        }

        let mut rail = Rail::new();
        let mut settings = Settings::default();
        let mut q = Queue::at(dir.join("drives"), cache.clone());
        assert!(
            matches!(q.fetch(&mut rail, &wants, "unreachable"), Press::Running { .. }),
            "the fetch did not start"
        );
        pump_to_a_stop(&mut q, &mut settings, &mut rail);

        // **Three shelves, and the middle one is the whole point.** Rockbox publishes two files and
        // the only thing that separates them is `rockbox::Where`.
        let filed: Vec<(&str, &str)> = settings
            .resources
            .iter()
            .map(|it| (it.what.kind(), it.name.as_str()))
            .collect();
        assert_eq!(
            filed,
            vec![
                ("bootloader", "bootloader-fixture.ipod"),
                ("software", "rockbox-fixture.zip"),
                ("bootloader", "loader-fixture.bin"),
            ],
            "a fetched part was filed on the wrong shelf"
        );
        // …and every one of them says how hard it was checked, because `Want::offered` refuses an
        // entry with no hash and this is what carries that promise into the library.
        for it in &settings.resources {
            assert_eq!(
                it.from,
                Some(Provenance::Fetched {
                    verified: Verification::Sha256
                }),
                "{} arrived without saying what was checked",
                it.name
            );
        }
        // **No drive was told it was built from a bootloader.** `record` used to set `installer`
        // for every fetch, and the next build would have written that name into the drive's
        // `built_from`.
        assert_eq!(q.installer, None, "a bootloader became what a drive was built from");
        assert_eq!(
            rail.entries().iter().filter(|e| e.kind == Kind::Done).count(),
            3,
            "the Rail did not tick all three: {:?}",
            rail.entries().iter().map(|e| (e.kind, e.what.clone())).collect::<Vec<_>>()
        );
        // The plan drew a real denominator on every row, which is what §12.3 asks of a bar.
        for (w, s) in wants.iter().zip(q.steps.iter()) {
            assert_eq!(s.cost.down, w.bytes(), "{} planned against no byte count", w.file());
            assert_ne!(s.cost.down, 0);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A fetch asked to stop before it starts downloads nothing at all.**
    ///
    /// The first of the two boundaries `fetch_run` reads the flag at, driven directly so the answer
    /// is not a race: the flag is already true when the worker body is entered.
    #[test]
    fn a_fetch_asked_to_stop_before_it_starts_downloads_nothing() {
        let dir = scratch_dir("fetch-stop-first");
        let cache = dir.join("firmware");
        std::fs::create_dir_all(&cache).expect("a cache");
        let body = b"a cached fixture".to_vec();
        let want = Want::Rockbox(a_piece(
            "cached-fixture.ipod",
            &body,
            rockbox::Where::FirmwarePartition,
        ));
        // In the cache and verifying — so a run that ignored the flag would report `Done`, which is
        // what makes this test able to fail.
        std::fs::write(want.lands_at(&cache), &body).expect("the fixture in the cache");

        let (tx, rx) = mpsc::channel();
        let cancel = Cancel::new();
        cancel.ask();
        fetch_run(vec![want], cache, &tx, &cancel);
        drop(tx);
        let said: Vec<Report> = rx.try_iter().collect();
        assert!(
            matches!(
                said.as_slice(),
                [Report::Cancelled {
                    i: 0,
                    removed: None
                }]
            ),
            "a stopped fetch said {said:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A fetch in flight is stopped through the queue, and the Rail says so.**
    ///
    /// The second boundary — inside the download's own watcher, once per `firmware::WATCH_TICK` —
    /// and it is driven against a **socket that accepts and never answers**. `TcpListener::bind`
    /// with nobody calling `accept` still completes the handshake out of the kernel's backlog, so
    /// `curl` connects, sends its request and waits, which is a download that lasts exactly as long
    /// as this test wants it to and reaches no third party.
    ///
    /// It is not `#[ignore]`d for that reason: the only thing on the far end of the wire is this
    /// process.
    #[test]
    fn a_fetch_in_flight_is_stopped_and_leaves_no_partial_file() {
        assert!(
            eapp_loader::tooling::can_download(),
            "this test drives the fetcher, and the fetcher is curl"
        );
        let _data = crate::data_dir_lock();
        let dir = scratch_dir("fetch-cancel");
        let cache = dir.join("firmware");
        std::fs::create_dir_all(&cache).expect("a cache");

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a socket");
        let port = listener.local_addr().expect("a port").port();
        let piece: &'static rockbox::Piece = Box::leak(Box::new(rockbox::Piece {
            file: "never-arrives.ipod",
            url: Box::leak(format!("http://127.0.0.1:{port}/never").into_boxed_str()),
            // A denominator, so the bar drawn over this would be a real one — and so
            // `Want::offered` would accept it.
            bytes: 4096,
            sha256: Box::leak(eapp_loader::firmware::sha256(b"never").into_boxed_str()),
            goes: rockbox::Where::FirmwarePartition,
            about: "a fixture that never arrives",
        }));
        let want = Want::Rockbox(piece);
        let part = want.part_path(&cache);

        let mut rail = Rail::new();
        let mut settings = Settings::default();
        let mut q = Queue::at(dir.join("drives"), cache.clone());
        assert!(matches!(
            q.fetch(&mut rail, &[want], "unreachable"),
            Press::Running { .. }
        ));
        let id = q.ids[0];

        // **Wait for the file to be announced before cancelling**, which is what makes this the
        // in-flight boundary rather than the one above it. `Rail::writing` is what sets
        // `cancellable`, and `Entry::cancel_cost` is the caption that reads off it.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            q.pump(&mut settings, &mut rail);
            if rail.find(id).is_some_and(|e| e.cancellable) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            rail.find(id).is_some_and(|e| e.cancellable),
            "the fetch never announced the file it is writing, so nothing could be cancelled"
        );
        // **A second fetch while one is in flight is a note, not a second worker.** And it is the
        // fetch's own sentence: `Queue::press`'s is *Already making an iPod*, which would name a
        // device this queue is not making.
        assert!(matches!(q.fetch(&mut rail, &[want], "unreachable"), Press::Busy));
        assert!(
            rail.entries().iter().any(|e| e.what == "Already fetching. The steps below are what it is doing."),
            "a second press was told about a machine nobody is making: {:?}",
            rail.entries().iter().map(|e| e.what.clone()).collect::<Vec<_>>()
        );

        assert!(q.cancel(id), "the queue refused to stop a fetch it is running");
        pump_to_a_stop(&mut q, &mut settings, &mut rail);

        assert!(!part.exists(), "a cancelled fetch left its partial file: {}", part.display());
        assert!(
            !want.lands_at(&cache).exists(),
            "a cancelled fetch left a file wearing a real name"
        );
        assert!(
            settings.resources.is_empty(),
            "a cancelled fetch filed {:?}",
            settings.resources.iter().map(|it| it.name.clone()).collect::<Vec<_>>()
        );
        assert_eq!(
            rail.find(id).map(|e| e.kind),
            Some(Kind::Cancelled),
            "the Rail does not say the fetch was stopped: {:?}",
            rail.entries().iter().map(|e| (e.kind, e.what.clone())).collect::<Vec<_>>()
        );
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **An empty offer is a note and not a failure**, and the sentence is the caller's.
    #[test]
    fn a_fetch_with_nothing_to_do_says_so_in_the_words_the_greyed_verb_wears() {
        let dir = scratch_dir("fetch-nothing");
        let mut rail = Rail::new();
        let mut q = Queue::at(dir.join("drives"), dir.join("firmware"));
        let said = crate::parts::Group::Bootloaders.nothing_to_fetch(&Settings::default());
        assert!(matches!(q.fetch(&mut rail, &[], said), Press::Nothing));
        assert_eq!(rail.failures(), 0, "having everything was reported as something going wrong");
        assert_eq!(
            rail.entries().iter().map(|e| e.what.as_str()).collect::<Vec<_>>(),
            vec![said],
            "the note is not the sentence the disabled verb draws"
        );
        assert!(!q.busy(), "an empty fetch started a thread");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
