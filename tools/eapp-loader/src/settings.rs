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

/// The two ways the window can be looked at.
///
/// One toggle, not a pile of checkboxes: the choice is between "this is an iPod" and "this is an
/// instrument", and every counter on screen belongs to the second.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    /// The iPod and nothing else. The default, and what a stranger who cloned this should meet.
    #[default]
    User,
    /// Instruction counts, both clocks, the task watches, the ATA census, the framebuffer
    /// inspector, the screenshot button.
    Debug,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::User => "user",
            Mode::Debug => "debug",
        }
    }

    fn parse(s: &str) -> Option<Mode> {
        match s.trim() {
            "user" => Some(Mode::User),
            "debug" => Some(Mode::Debug),
            _ => None,
        }
    }
}

/// One thing an iPod can be built out of: a boot ROM, an Apple firmware bundle, or a drive.
///
/// **Named, so a machine can refer to it instead of repeating it.** Before this, a machine held a
/// path; two machines that shared a boot ROM held the same path twice, and there was nowhere to
/// keep a file you had but were not currently running. A library entry is that place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ingredient {
    /// A 1 MB dump, or the recipe for synthesising one — the same [`crate::nor::Source`] a machine
    /// has always carried, so a synthesised ROM is a library entry like any other.
    Rom(crate::nor::Source),
    /// An `.ipsw`. Not bootable itself: a drive is **built** from it, and the drive is what runs.
    Ipsw(PathBuf),
    /// A drive image.
    Disk(PathBuf),
}

impl Ingredient {
    /// The word the settings file and the window both use.
    pub fn kind(&self) -> &'static str {
        match self {
            Ingredient::Rom(_) => "rom",
            Ingredient::Ipsw(_) => "ipsw",
            Ingredient::Disk(_) => "disk",
        }
    }

    /// Where it is on disk, when it is a file at all. A synthesised ROM is a recipe and has none —
    /// which is the whole reason this returns an `Option` rather than a `PathBuf`.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Ingredient::Rom(crate::nor::Source::File(p)) => Some(p),
            Ingredient::Rom(_) => None,
            Ingredient::Ipsw(p) | Ingredient::Disk(p) => Some(p),
        }
    }
}

/// A named entry in the library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// What the person calls it, and the key a machine refers to it by. Unique within the library.
    pub name: String,
    pub what: Ingredient,
}

/// A saved machine: a boot ROM, a drive, and how to dress and run them.
///
/// **This is not a new kind of state — it is the state this program has always had, named.** The
/// fields are exactly the ones [`Settings`] already carried for the one machine it could hold, so
/// a machine is what you get by giving that set a name and being allowed more than one.
///
/// The live machine stays in `Settings`' own fields, and this list is what you can switch *to*.
/// Keeping it that way means every existing reader of `settings.nor` and `settings.disk` is still
/// reading the machine that is running, which is what it meant before and still means.
///
/// **A machine composed of library entries stores their names, not their contents** — see
/// [`Machine::rom`] and [`Machine::drive`]. The `nor` and `disk` fields below stay as the resolved
/// answer, so nothing downstream of here had to learn about the library at all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Machine {
    /// What the person calls it. The key, so it is unique and renaming is a delete plus an add.
    pub name: String,
    pub nor: crate::nor::Source,
    pub disk: Option<PathBuf>,
    pub chassis: Option<crate::identity::Colour>,
    pub work_on_copy: Option<bool>,
    /// Instructions the last **completed** cold boot of this machine took.
    ///
    /// The progress bar's denominator, and the reason it can be honest across operating systems.
    /// It used to be `snap_at` — a constant tuned to RetailOS's 1.6 G — which made the bar
    /// meaningless for anything else: Rockbox reaches its menu in about 100 M and barely moved it,
    /// iPodLinux takes 21.5 G and pinned it at 100 % for twenty billion instructions. A machine's
    /// own last boot is a better predictor of its next one than any constant, and it needs no
    /// detection of which operating system is on the drive.
    ///
    /// `None` until it has booted once, and the bar says "booting" without a fraction rather than
    /// inventing one.
    pub boot_instructions: Option<u64>,
    /// The library entry this machine's boot ROM comes from, by name.
    ///
    /// `None` means the `nor` field above is the answer directly — which is every machine saved
    /// before the library existed, and is why nothing had to be migrated destructively. A name that
    /// is not in the library is **reported, not ignored**: see [`Settings::missing`].
    pub rom: Option<String>,
    /// The library entry this machine's drive comes from, by name. `None` means `disk` is the
    /// answer directly, as above.
    pub drive: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub mode: Mode,
    /// Saved machines, in the order they were made. **Does not include the live one's edits** —
    /// `Settings`' own fields are the machine that is running.
    pub machines: Vec<Machine>,
    /// The name of the machine the live fields came from, if any.
    pub current: Option<String>,
    /// Every boot ROM, `.ipsw` and drive this program has been told about.
    ///
    /// **The library exists because a file is worth keeping when it is not the one running.** Every
    /// path this program learned used to be attached to the single live machine, so choosing a
    /// second drive lost the first one and there was no way to say "I have these four ROMs". A
    /// machine now names entries from here, and one entry can back any number of machines.
    ///
    /// Entries are added rather than replaced: dropping a file on the window files it here as well
    /// as using it, so the thing you just used is still here next time.
    pub library: Vec<Item>,
    /// Whether [`Settings::seed_library`] has run.
    ///
    /// **A marker, so that removing an entry sticks.** Seeding whenever the library is empty would
    /// be simpler and would be wrong: empty is also what you get after removing the last entry, and
    /// a list that puts back what you took out is a list you cannot edit.
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
    pub fn load() -> Settings {
        let mut s = match std::fs::read_to_string(data_dir().join(FILE)).ok() {
            Some(text) => Settings::parse(&text),
            None => Settings::default(),
        };
        s.seed_library();
        s
    }

    /// Fill an empty library from what this program already has: the live machine's boot ROM and
    /// drive, and every saved machine's.
    ///
    /// **Once.** A setup that predates the library would otherwise open the page and be told it has
    /// nothing, while running a boot ROM and a drive — which is not a list of what you have, it is
    /// a list of what you have added since Tuesday. Seeding makes the page true on the first look.
    ///
    /// Guarded by [`Settings::library_seeded`] rather than by emptiness, so that removing every
    /// entry is a thing you can do rather than a thing that undoes itself at the next launch.
    pub fn seed_library(&mut self) {
        if self.library_seeded {
            return;
        }
        self.library_seeded = true;
        let stem = |p: &Path| {
            p.file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let describe = |src: &crate::nor::Source| match src {
            crate::nor::Source::File(p) => stem(p),
            crate::nor::Source::Synthetic { model, seed, .. } => format!("{model}, seed {seed}"),
        };
        // Gathered before anything is filed, because filing borrows the library mutably and the
        // names are read out of the very fields being filed.
        //
        // The live machine comes first, so its entries get the unsuffixed names — it is the one on
        // screen, and the one whose name a person will recognise.
        let mut want: Vec<(Ingredient, String)> = Vec::new();
        want.push((Ingredient::Rom(self.nor.clone()), describe(&self.nor)));
        if let Some(d) = &self.disk {
            want.push((Ingredient::Disk(d.clone()), stem(d)));
        }
        for m in &self.machines {
            want.push((Ingredient::Rom(m.nor.clone()), describe(&m.nor)));
            if let Some(d) = &m.disk {
                want.push((Ingredient::Disk(d.clone()), stem(d)));
            }
        }
        for (what, name) in want {
            let name = if name.is_empty() {
                "unnamed".to_string()
            } else {
                name
            };
            self.file_away(what, &name);
        }
    }

    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
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
                "mode" => s.mode = Mode::parse(v).unwrap_or_default(),
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
                "work_on_copy" => s.work_on_copy = Some(v == "true"),
                "current" if !v.is_empty() => s.current = Some(v.to_string()),
                "library_seeded" => s.library_seeded = v == "true",
                // `machine.N.field = value`. Flat, because the file is flat and a nested format
                // would mean a parser that can fail — and a settings file is not a place to fail.
                // Indices are dense on write and tolerated sparse on read.
                _ if k.starts_with("machine.") => {
                    let mut it = k.splitn(3, '.');
                    let (_, idx, field) = (it.next(), it.next(), it.next());
                    let (Some(idx), Some(field)) = (idx, field) else {
                        continue;
                    };
                    let Ok(i) = idx.parse::<usize>() else {
                        continue;
                    };
                    while s.machines.len() <= i {
                        s.machines.push(Machine::default());
                    }
                    let m = &mut s.machines[i];
                    match field {
                        "name" => m.name = v.to_string(),
                        "flash" if !v.is_empty() => {
                            m.nor = crate::nor::Source::File(PathBuf::from(v))
                        }
                        "nor_model" if !v.is_empty() => m.nor = with_model(m.nor.clone(), v),
                        "nor_seed" => {
                            if let Ok(n) = v.parse::<u64>() {
                                m.nor = with_seed(m.nor.clone(), n);
                            }
                        }
                        "nor_serial" => m.nor = with_serial(m.nor.clone(), v),
                        "nor_splash" => m.nor = with_splash(m.nor.clone(), v),
                        "nor_guid" => {
                            if let Ok(g) = u64::from_str_radix(v.trim_start_matches("0x"), 16) {
                                m.nor = with_guid(m.nor.clone(), g);
                            }
                        }
                        "disk" if !v.is_empty() => m.disk = Some(PathBuf::from(v)),
                        "chassis" => m.chassis = crate::identity::Colour::parse(v),
                        "work_on_copy" => m.work_on_copy = Some(v == "true"),
                        "boot_instructions" => m.boot_instructions = v.parse::<u64>().ok(),
                        "rom" if !v.is_empty() => m.rom = Some(v.to_string()),
                        "drive" if !v.is_empty() => m.drive = Some(v.to_string()),
                        _ => {}
                    }
                }
                // `item.N.field = value` — the library, in the same flat shape as the machines
                // above and for the same reason.
                _ if k.starts_with("item.") => {
                    let mut it = k.splitn(3, '.');
                    let (_, idx, field) = (it.next(), it.next(), it.next());
                    let (Some(idx), Some(field)) = (idx, field) else {
                        continue;
                    };
                    let Ok(i) = idx.parse::<usize>() else {
                        continue;
                    };
                    while s.library.len() <= i {
                        s.library.push(Item {
                            name: String::new(),
                            what: Ingredient::Rom(crate::nor::Source::default()),
                        });
                    }
                    let item = &mut s.library[i];
                    match field {
                        "name" => item.name = v.to_string(),
                        // `kind` before `path` on write, so this has always seen it first. A kind
                        // it does not know leaves the entry as it was rather than dropping it —
                        // the same tolerance every other key here has.
                        "kind" => {
                            item.what = match v {
                                "ipsw" => Ingredient::Ipsw(PathBuf::new()),
                                "disk" => Ingredient::Disk(PathBuf::new()),
                                _ => Ingredient::Rom(crate::nor::Source::default()),
                            }
                        }
                        "path" if !v.is_empty() => {
                            let p = PathBuf::from(v);
                            item.what = match &item.what {
                                Ingredient::Ipsw(_) => Ingredient::Ipsw(p),
                                Ingredient::Disk(_) => Ingredient::Disk(p),
                                Ingredient::Rom(_) => Ingredient::Rom(crate::nor::Source::File(p)),
                            }
                        }
                        _ => {
                            if let Ingredient::Rom(src) = &item.what {
                                if let Some(next) = nor_field(src.clone(), field, v) {
                                    item.what = Ingredient::Rom(next);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        s
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
    pub fn render(&self) -> String {
        let p = |o: &Option<PathBuf>| {
            o.as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        format!(
            "# ipod-gui settings. Hand-editable; unknown keys are ignored.\n\
             mode = {}\n\
             # auto, white, black, or u2. `auto` reads it out of the NOR's Mod#, which is\n\
             # what the dump says the iPod was; the rest overrule that. Cosmetic either way —\n\
             # the firmware is handed the same identity whatever this says.\n\
             chassis = {}\n\
{}\
             disk = {}\n\
             # An HTTPS GET of the GitHub releases API and a version comparison, on launch.\n\
             # Off by default on purpose. The menu item works whatever this says.\n\
             check_updates_on_start = {}\n\
             # Run on a COPY of the drive, leaving the original untouched. Absent means \"decide\n\
             # from where the drive came from\": a drive this program built is written to directly,\n\
             # one you supplied is copied. Set it to true or false to answer for both.\n\
             {}",
            self.mode.as_str(),
            self.chassis.map(|c| c.as_str()).unwrap_or("auto"),
            self.render_nor(),
            p(&self.disk),
            self.check_updates_on_start,
            match self.work_on_copy {
                Some(v) => format!("work_on_copy = {v}\n"),
                None => String::new(),
            },
        ) + &self.render_library()
            + &self.render_machines()
    }

    /// The library, as `item.N.field` lines.
    ///
    /// `kind` is written **before** `path`, and the parser depends on that order: it uses the kind
    /// to decide which variant a path belongs to. Reordering these two lines by hand in the file
    /// would file an `.ipsw` as a boot ROM, so the order is load-bearing rather than tidy.
    fn render_library(&self) -> String {
        if self.library.is_empty() && !self.library_seeded {
            return String::new();
        }
        let mut out = String::from(
            "\n# The library: every boot ROM, .ipsw and drive this program knows about.\n\
             # A machine refers to these by name — see `machine.N.rom` and `machine.N.drive`.\n",
        );
        out.push_str(&format!("library_seeded = {}\n", self.library_seeded));
        for (i, item) in self.library.iter().enumerate() {
            out.push_str(&format!("\nitem.{i}.name = {}\n", item.name));
            out.push_str(&format!("item.{i}.kind = {}\n", item.what.kind()));
            match &item.what {
                // A synthesised ROM is a recipe, so it is written the way every other recipe in
                // this file is written — and `flash = …` becomes `path = …` because at this level
                // the three kinds share one key.
                Ingredient::Rom(src) => {
                    for line in render_nor_of(src).lines() {
                        let line = line
                            .strip_prefix("flash = ")
                            .map_or_else(|| line.to_string(), |p| format!("path = {p}"));
                        out.push_str(&format!("item.{i}.{line}\n"));
                    }
                }
                Ingredient::Ipsw(p) | Ingredient::Disk(p) => {
                    out.push_str(&format!("item.{i}.path = {}\n", p.display()));
                }
            }
        }
        out
    }

    /// The saved machines, as `machine.N.field` lines.
    ///
    /// Written after everything else so that the top of the file is still the live machine and the
    /// program's own preferences — which is what a person opening this file to hand-edit is looking
    /// for. Nothing here is required: a file with no machine lines is a program with one machine,
    /// which is what it was before.
    fn render_machines(&self) -> String {
        if self.machines.is_empty() {
            return String::new();
        }
        let mut out = String::from(
            "\n# Saved machines. `current` names the one the settings above came from.\n",
        );
        if let Some(c) = &self.current {
            out.push_str(&format!("current = {c}\n"));
        }
        for (i, m) in self.machines.iter().enumerate() {
            out.push_str(&format!("\nmachine.{i}.name = {}\n", m.name));
            for line in render_nor_of(&m.nor).lines() {
                out.push_str(&format!("machine.{i}.{line}\n"));
            }
            if let Some(d) = &m.disk {
                out.push_str(&format!("machine.{i}.disk = {}\n", d.display()));
            }
            if let Some(c) = m.chassis {
                out.push_str(&format!("machine.{i}.chassis = {}\n", c.as_str()));
            }
            if let Some(w) = m.work_on_copy {
                out.push_str(&format!("machine.{i}.work_on_copy = {w}\n"));
            }
            if let Some(r) = &m.rom {
                out.push_str(&format!("machine.{i}.rom = {r}\n"));
            }
            if let Some(d) = &m.drive {
                out.push_str(&format!("machine.{i}.drive = {d}\n"));
            }
            if let Some(b) = m.boot_instructions {
                out.push_str(&format!("machine.{i}.boot_instructions = {b}\n"));
            }
        }
        out
    }

    /// Best-effort. A window that could not write its preferences is a window that opens in user
    /// mode next time, not a window that refuses to run.
    /// The live machine, as a [`Machine`] under `name`.
    pub fn as_machine(&self, name: &str) -> Machine {
        let existing = self.machines.iter().find(|m| m.name == name);
        Machine {
            name: name.to_string(),
            nor: self.nor.clone(),
            disk: self.disk.clone(),
            chassis: self.chassis,
            work_on_copy: self.work_on_copy,
            // Carried across, so naming a machine you have already booted does not throw away the
            // one measurement that makes its progress bar honest.
            boot_instructions: self
                .current
                .as_deref()
                .and_then(|c| self.machines.iter().find(|m| m.name == c))
                .and_then(|m| m.boot_instructions),
            // **The composition is kept, not re-derived.** Deriving it from the live values was
            // tried and is wrong: switching machines writes back the one you were editing, so the
            // moment a library entry changed, the write-back looked for an entry matching the old
            // value, found none, and quietly cut the machine loose from the entry it was made of.
            // The test `editing_a_library_entry_changes_the_machines_composed_of_it` is that bug.
            //
            // So an existing machine keeps what it was composed of, exactly as it keeps its
            // measured boot length, and the derivation below is only for a machine being named for
            // the first time — where the files it is running are the only evidence of what it is
            // made of.
            rom: existing.and_then(|m| m.rom.clone()).or_else(|| {
                self.library
                    .iter()
                    .find(|it| matches!(&it.what, Ingredient::Rom(src) if *src == self.nor))
                    .map(|it| it.name.clone())
            }),
            drive: existing.and_then(|m| m.drive.clone()).or_else(|| {
                self.disk.as_ref().and_then(|d| {
                    self.library
                        .iter()
                        .find(|it| matches!(&it.what, Ingredient::Disk(p) if p == d))
                        .map(|it| it.name.clone())
                })
            }),
        }
    }

    /// Put something in the library, or return the name it already has.
    ///
    /// **Adding the same file twice is not an error and does not make a second entry** — dropping a
    /// drive on the window that you dropped last week should leave one drive in the list, named the
    /// way you named it. Identity is the value, not the name: two entries pointing at one path
    /// would be two names for one thing, and nothing could tell you which you were running.
    pub fn file_away(&mut self, what: Ingredient, suggested: &str) -> String {
        if let Some(existing) = self.library.iter().find(|it| it.what == what) {
            return existing.name.clone();
        }
        // A name collision with a *different* thing gets a suffix rather than overwriting it.
        let mut name = suggested.to_string();
        let mut n = 2;
        while self.library.iter().any(|it| it.name == name) {
            name = format!("{suggested} ({n})");
            n += 1;
        }
        self.library.push(Item {
            name: name.clone(),
            what,
        });
        name
    }

    /// Library entries a saved machine names but the library does not have.
    ///
    /// **Reported rather than silently ignored.** A machine whose ROM went missing should say which
    /// one, because "it boots to a white screen" is not a diagnosis and the file that went is the
    /// whole of the answer.
    pub fn missing(&self, m: &Machine) -> Vec<String> {
        let mut out = Vec::new();
        for want in [m.rom.as_deref(), m.drive.as_deref()].into_iter().flatten() {
            if !self.library.iter().any(|it| it.name == want) {
                out.push(want.to_string());
            }
        }
        out
    }

    /// Use a library entry now: a ROM or a drive becomes the live machine's, an `.ipsw` is not
    /// something that can be *run* and is refused rather than half-applied.
    ///
    /// `false` when there is no entry of that name, so a caller can say so.
    pub fn use_item(&mut self, name: &str) -> bool {
        let Some(item) = self.library.iter().find(|it| it.name == name) else {
            return false;
        };
        match item.what.clone() {
            Ingredient::Rom(src) => self.nor = src,
            Ingredient::Disk(p) => self.disk = Some(p),
            // Building a drive from a bundle is a job with a progress bar and a destination, not a
            // field assignment. The window runs it; this refuses rather than pretending.
            Ingredient::Ipsw(_) => return false,
        }
        true
    }

    /// Save the live machine under `name`, replacing any machine of that name.
    ///
    /// **Name is the key.** Two machines called the same thing is a list nobody can act on — you
    /// cannot say which one you meant, and neither can the program.
    pub fn remember_as(&mut self, name: &str) {
        let m = self.as_machine(name);
        match self.machines.iter().position(|x| x.name == name) {
            Some(i) => self.machines[i] = m,
            None => self.machines.push(m),
        }
        self.current = Some(name.to_string());
    }

    /// Make a saved machine the live one. `false` if there is no machine of that name.
    ///
    /// **The machine being replaced is written back first**, so switching away from something you
    /// have been editing does not discard the edits — which is what every person switching between
    /// two of anything expects, and what they never say out loud.
    pub fn switch_to(&mut self, name: &str) -> bool {
        let Some(i) = self.machines.iter().position(|m| m.name == name) else {
            return false;
        };
        if let Some(c) = self.current.clone() {
            if c != name && self.machines.iter().any(|m| m.name == c) {
                let live = self.as_machine(&c);
                if let Some(j) = self.machines.iter().position(|m| m.name == c) {
                    self.machines[j] = live;
                }
            }
        }
        let m = self.machines[i].clone();
        // **The library entry wins when the machine names one**, so editing an entry changes every
        // machine composed of it — which is the point of composing rather than copying. A name that
        // is not in the library falls back to the machine's own stored value rather than leaving it
        // with no ROM at all; `missing` is what reports that, at a moment when it can be read.
        self.nor = m
            .rom
            .as_deref()
            .and_then(|n| self.library.iter().find(|it| it.name == n))
            .and_then(|it| match &it.what {
                Ingredient::Rom(src) => Some(src.clone()),
                _ => None,
            })
            .unwrap_or(m.nor);
        self.disk = m
            .drive
            .as_deref()
            .and_then(|n| self.library.iter().find(|it| it.name == n))
            .and_then(|it| match &it.what {
                Ingredient::Disk(p) => Some(p.clone()),
                _ => None,
            })
            .or(m.disk);
        self.chassis = m.chassis;
        self.work_on_copy = m.work_on_copy;
        self.current = Some(name.to_string());
        true
    }

    /// Remove a saved machine. The live fields are untouched — forgetting the machine you are
    /// running stops it being in the list, it does not stop it running.
    pub fn forget(&mut self, name: &str) {
        self.machines.retain(|m| m.name != name);
        if self.current.as_deref() == Some(name) {
            self.current = None;
        }
    }

    /// Record how long this machine's cold boot took, for the next one's progress bar.
    pub fn record_boot(&mut self, instructions: u64) {
        let Some(c) = self.current.clone() else {
            return;
        };
        if let Some(m) = self.machines.iter_mut().find(|m| m.name == c) {
            m.boot_instructions = Some(instructions);
        }
    }

    /// What the progress bar should divide by, if anything is known.
    pub fn expected_boot(&self) -> Option<u64> {
        self.current
            .as_deref()
            .and_then(|c| self.machines.iter().find(|m| m.name == c))
            .and_then(|m| m.boot_instructions)
            .filter(|n| *n > 0)
    }

    pub fn save(&self) {
        let dir = data_dir();
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let _ = std::fs::write(dir.join(FILE), self.render());
    }

    /// Where the settings live, for the UI to print. A preference nobody can find is a preference
    /// nobody can reset.
    pub fn path() -> Option<PathBuf> {
        Some(data_dir().join(FILE))
    }
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

    #[test]
    fn a_missing_file_is_user_mode_with_nothing_configured() {
        let s = Settings::parse("");
        assert_eq!(s.mode, Mode::User);
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
            mode: Mode::Debug,
            chassis: Some(crate::identity::Colour::Black),
            nor: crate::nor::Source::File(PathBuf::from("/a/b/rom.bin")),
            disk: Some(PathBuf::from("/a/b/disk.img")),
            check_updates_on_start: true,
            work_on_copy: Some(true),
            machines: Vec::new(),
            current: None,
            library: Vec::new(),
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
        assert_eq!(
            s.mode,
            Mode::User,
            "an unknown mode falls back to the default"
        );
        assert_eq!(
            s.disk, None,
            "an empty value is `not set`, not `the empty path`"
        );
        assert!(!s.check_updates_on_start, "anything but `true` is off");
    }

    #[test]
    fn the_data_directory_is_absolute() {
        let d = data_dir();
        assert!(d.is_absolute(), "{}", d.display());
    }

    /// `IPOD_EMULATOR_DATA` is what the setup screen's "change" button sets, so it has to win over
    /// both the beside-the-executable default and the platform directory.
    #[test]
    fn the_override_wins() {
        // SAFETY: single-threaded test, and the value is restored before it returns.
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
mod machine_tests {
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

    /// **A machine list has to survive the round trip, or it is not storage.**
    #[test]
    fn machines_survive_render_and_parse() {
        let mut s = Settings {
            nor: synth("A146", 5),
            ..Default::default()
        };
        s.disk = Some(PathBuf::from("/drives/one.img"));
        s.remember_as("Video 5G");
        s.nor = crate::nor::Source::File(PathBuf::from("/roms/real.bin"));
        s.disk = Some(PathBuf::from("/drives/two.img"));
        s.remember_as("my own iPod");

        let back = Settings::parse(&s.render());
        assert_eq!(back.machines.len(), 2, "both machines came back");
        assert_eq!(back.machines[0].name, "Video 5G");
        assert_eq!(
            back.machines[0].nor,
            synth("A146", 5),
            "a synthesised ROM is a recipe"
        );
        assert_eq!(
            back.machines[1].disk,
            Some(PathBuf::from("/drives/two.img"))
        );
        assert_eq!(back.current.as_deref(), Some("my own iPod"));
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

        assert!(s.switch_to("generated"));
        assert_eq!(s.nor, synth("A146", 7), "back to the recipe, not to a path");
        assert!(s.switch_to("real dump"));
        assert!(matches!(s.nor, crate::nor::Source::File(_)));
        assert!(!s.switch_to("nothing of that name"));
    }

    /// Switching away from a machine you have edited keeps the edits.
    #[test]
    fn switching_writes_back_what_you_were_editing() {
        let mut s = Settings {
            nor: synth("A146", 1),
            ..Default::default()
        };
        s.remember_as("a");
        s.remember_as("b");
        s.switch_to("a");
        s.disk = Some(PathBuf::from("/drives/edited.img"));
        s.switch_to("b");
        s.switch_to("a");
        assert_eq!(
            s.disk,
            Some(PathBuf::from("/drives/edited.img")),
            "the edit made while `a` was live came back with it"
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
        s.switch_to("one");
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
        assert!(s.machines.is_empty(), "no list, and that is not an error");
        assert_eq!(s.current, None);
    }

    // ------------------------------------------------------------------------- the library

    /// **The library has to survive the round trip, or it is not storage** — the same bar the
    /// machine list is held to, for the same reason.
    #[test]
    fn the_library_survives_render_and_parse() {
        let mut s = Settings::default();
        s.file_away(Ingredient::Rom(synth("A146", 5)), "a synthesised 30 GB");
        s.file_away(
            Ingredient::Rom(crate::nor::Source::File(PathBuf::from("/roms/retail.bin"))),
            "my own dump",
        );
        s.file_away(
            Ingredient::Ipsw(PathBuf::from("/fw/iPod_20.1.3.ipsw")),
            "20.1.3",
        );
        s.file_away(
            Ingredient::Disk(PathBuf::from("/drives/rockbox.img")),
            "with Rockbox",
        );

        let back = Settings::parse(&s.render());
        assert_eq!(
            back.library, s.library,
            "the library did not come back as it went in"
        );
    }

    /// **Filing the same thing twice makes one entry, not two.** Identity is the value, not the
    /// name: two entries for one path would be two names for one thing, and nothing could say
    /// which one you were running.
    #[test]
    fn filing_the_same_file_twice_returns_the_name_it_already_has() {
        let mut s = Settings::default();
        let first = s.file_away(Ingredient::Disk(PathBuf::from("/drives/one.img")), "mine");
        let again = s.file_away(
            Ingredient::Disk(PathBuf::from("/drives/one.img")),
            "something else",
        );
        assert_eq!(first, again, "the same drive was filed under a second name");
        assert_eq!(s.library.len(), 1);

        // A different thing that wants a taken name gets a suffix rather than overwriting it.
        let other = s.file_away(Ingredient::Disk(PathBuf::from("/drives/two.img")), "mine");
        assert_ne!(
            other, "mine",
            "a second drive overwrote the first one's name"
        );
        assert_eq!(s.library.len(), 2);
    }

    /// **The point of composing rather than copying**: edit the entry, and every machine made of it
    /// changes. If this passes with the resolution removed, machines are copies and the library is
    /// decoration.
    #[test]
    fn editing_a_library_entry_changes_the_machines_composed_of_it() {
        let mut s = Settings::default();
        s.file_away(Ingredient::Rom(synth("A146", 5)), "the shared ROM");
        s.nor = synth("A146", 5);
        s.disk = Some(PathBuf::from("/drives/one.img"));
        s.remember_as("first");
        s.remember_as("second");
        assert_eq!(
            s.machines[0].rom.as_deref(),
            Some("the shared ROM"),
            "not composed of it"
        );

        // Re-seed the entry. Both machines are made of it, so both should now boot the new one.
        s.library[0].what = Ingredient::Rom(synth("A146", 99));
        for name in ["first", "second"] {
            assert!(s.switch_to(name));
            assert_eq!(
                s.nor,
                synth("A146", 99),
                "{name} did not follow the entry it is made of"
            );
        }
    }

    /// A machine saved before the library existed has no reference, and must keep working.
    #[test]
    fn a_machine_with_no_reference_still_boots_its_own_files() {
        let mut s = Settings::parse(
            "machine.0.name = old\nmachine.0.nor_model = A146\nmachine.0.nor_seed = 7\n\
             machine.0.disk = /drives/old.img\n",
        );
        assert!(s.switch_to("old"));
        assert_eq!(s.nor, synth("A146", 7));
        assert_eq!(s.disk, Some(PathBuf::from("/drives/old.img")));
        assert!(
            s.missing(&s.machines[0].clone()).is_empty(),
            "nothing was named, so nothing is missing"
        );
    }

    /// **A reference to an entry that is gone is reported, not swallowed.** "It boots to a white
    /// screen" is not a diagnosis; the name of the file that went is.
    #[test]
    fn a_dangling_reference_is_named() {
        let s = Settings::parse(
            "machine.0.name = broken\nmachine.0.rom = a ROM that was deleted\n\
             machine.0.drive = a drive that was deleted\n",
        );
        let missing = s.missing(&s.machines[0]);
        assert_eq!(
            missing.len(),
            2,
            "both dangling references should be named: {missing:?}"
        );
        assert!(missing.iter().any(|m| m.contains("ROM")));
    }

    /// An `.ipsw` is not something that can be run — a drive is *built* from it. Applying one as if
    /// it were a drive would produce a machine that boots nothing, silently.
    #[test]
    fn an_ipsw_cannot_be_used_as_a_drive() {
        let mut s = Settings::default();
        s.file_away(
            Ingredient::Ipsw(PathBuf::from("/fw/iPod_20.1.3.ipsw")),
            "20.1.3",
        );
        assert!(
            !s.use_item("20.1.3"),
            "an .ipsw was accepted as something to run"
        );
        assert_eq!(s.disk, None, "and it set the drive anyway");
        assert!(!s.use_item("not in the library"));

        s.file_away(
            Ingredient::Disk(PathBuf::from("/drives/one.img")),
            "a drive",
        );
        assert!(s.use_item("a drive"));
        assert_eq!(s.disk, Some(PathBuf::from("/drives/one.img")));
    }

    /// **A setup that predates the library is not an empty library.** Someone who has been running
    /// a boot ROM and a drive for months should open the page and see them, not "nothing yet".
    #[test]
    fn the_library_seeds_itself_from_what_is_already_configured() {
        let mut s = Settings {
            nor: synth("A146", 5),
            disk: Some(PathBuf::from("/drives/mine.img")),
            ..Default::default()
        };
        s.remember_as("the one I use");
        s.nor = crate::nor::Source::File(PathBuf::from("/roms/retail.bin"));
        s.remember_as("with my own dump");

        s.seed_library();
        assert_eq!(
            s.library.len(),
            3,
            "two ROMs and one drive: {:?}",
            s.library
        );
        assert!(s
            .library
            .iter()
            .any(|i| i.what == Ingredient::Rom(synth("A146", 5))));
        assert!(s
            .library
            .iter()
            .any(|i| matches!(&i.what, Ingredient::Disk(p) if p.ends_with("mine.img"))));
    }

    /// **Seeding happens once, so removing an entry sticks.** A list that puts back what you took
    /// out at the next launch is a list you cannot edit.
    #[test]
    fn removing_the_last_entry_is_not_undone_by_the_next_launch() {
        let mut s = Settings {
            nor: synth("A146", 5),
            ..Default::default()
        };
        s.seed_library();
        assert_eq!(s.library.len(), 1);

        s.library.clear();
        let back = Settings::parse(&s.render());
        assert!(back.library_seeded, "the marker did not survive the file");
        let mut back = back;
        back.seed_library();
        assert!(
            back.library.is_empty(),
            "an entry that was removed came back"
        );
    }
}
