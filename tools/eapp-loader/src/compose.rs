//! What can be put on a drive together, and what happens if you try the rest.
//!
//! **The alternative was a short list of blessed combinations, and it was the wrong shape.** A
//! fixed menu — *Apple's*, *Apple's + Rockbox*, *all three* — cannot express "Rockbox and iPodLinux
//! without Apple's software", which is a perfectly good drive, and it teaches nothing about why the
//! combinations are what they are. Free choice plus a live verdict says the same thing and answers
//! the question underneath it.
//!
//! **Nothing here is disabled.** A checkbox you cannot tick is a question you cannot ask: it tells
//! you *that* something is impossible and never *why*, and the why is the whole of what somebody
//! learning this hardware wants. So every box ticks, and an impossible combination produces a
//! sentence naming the constraint and — where there is one — a fix that can be applied with a
//! click. There is no dead end, only a state with an explanation attached.
//!
//! Every rule below is measured, and cited where it came from.

use std::collections::BTreeSet;

/// What the verdict says before anybody has said where the drive comes from.
///
/// **A constant because two surfaces render it.** [`Recipe::check`] returns it as the `why`, and
/// the window draws that same string in `fg-dim` rather than `fg`. A literal in one match arm and
/// a quoted string in the design document is exactly the drift this project keeps paying for.
pub const NOTHING_CHOSEN: &str = "nothing chosen yet";

/// An operating system that can live on the drive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Os {
    /// Apple's own software, out of an `.ipsw`.
    Apple,
    Rockbox,
    /// iPodLinux, as the ZeroSlackr distribution.
    IPodLinux,
}

impl Os {
    pub fn label(self) -> &'static str {
        match self {
            Os::Apple => "Apple's software",
            Os::Rockbox => "Rockbox",
            Os::IPodLinux => "iPodLinux",
        }
    }

    /// The one lower-case word this is written as in a settings file.
    ///
    /// **Deliberately not [`Os::label`].** That is prose — `Apple's software` — and prose is free
    /// to change when the window's wording does. A key in a file somebody can hand-edit is not.
    pub fn as_str(self) -> &'static str {
        match self {
            Os::Apple => "apple",
            Os::Rockbox => "rockbox",
            Os::IPodLinux => "ipodlinux",
        }
    }

    /// Back again. `None` for anything this build does not know, which is what makes
    /// [`BootShape::parse`] able to refuse a half-read line rather than accept a wrong one.
    pub fn parse(s: &str) -> Option<Os> {
        match s.trim() {
            "apple" => Some(Os::Apple),
            "rockbox" => Some(Os::Rockbox),
            "ipodlinux" => Some(Os::IPodLinux),
            _ => None,
        }
    }

    pub const ALL: [Os; 3] = [Os::Apple, Os::Rockbox, Os::IPodLinux];

    /// What the window offers.
    ///
    /// **iPodLinux is not on it, and the rules for it are kept anyway.** Its kernel boot is clean —
    /// both partitions found, the root mounted, `/bin/init` run, no ATA error anywhere — and then
    /// ZeroLauncher reaches "Finishing Up…" and stalls, which is a real open bug (KNOWN-BUGS.md).
    /// Offering a path that ends there, after a 101 MB download, is offering a disappointment.
    ///
    /// The engine keeps knowing about it because the knowledge is measured and the tests that hold
    /// the free-choice model honest run over `ALL` — and because the way back is deleting a line
    /// here, not rebuilding what was thrown away. `ipod-boot install-linux` still does the whole
    /// install for anybody who wants to look at it.
    pub const OFFERED: [Os; 2] = [Os::Apple, Os::Rockbox];
}

/// What goes in the **firmware partition**, which holds exactly one thing.
///
/// That is the constraint the whole of this module is downstream of: the boot ROM starts whatever
/// is in that partition, and there is one of it. Everything called "dual boot" or "triple boot" is
/// therefore a *bootloader* in that partition offering the rest from the data volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loader {
    /// None. Apple's software goes in the firmware partition and the ROM starts it directly.
    Apple,
    /// Rockbox's bootloader. Hands back to Apple's software when MENU is held at power-on.
    Rockbox,
    /// `ipodloader2`, which reads `loader.cfg` from the volume and draws a menu.
    IPodLoader2,
}

impl Loader {
    pub fn label(self) -> &'static str {
        match self {
            Loader::Apple => "Apple's own",
            Loader::Rockbox => "Rockbox's bootloader",
            Loader::IPodLoader2 => "ipodloader2",
        }
    }

    /// The one lower-case word this is written as in a settings file — see [`Os::as_str`] for why
    /// it is not [`Loader::label`].
    ///
    /// `Loader::Apple` and `Os::Apple` are both `apple`, and that is safe because they never share
    /// a position: [`BootShape::render`] writes the bootloader first, always, and everything after
    /// it is a system.
    pub fn as_str(self) -> &'static str {
        match self {
            Loader::Apple => "apple",
            Loader::Rockbox => "rockbox",
            Loader::IPodLoader2 => "ipodloader2",
        }
    }

    /// Back again. `None` for anything this build does not know.
    pub fn parse(s: &str) -> Option<Loader> {
        match s.trim() {
            "apple" => Some(Loader::Apple),
            "rockbox" => Some(Loader::Rockbox),
            "ipodloader2" => Some(Loader::IPodLoader2),
            _ => None,
        }
    }

    pub const ALL: [Loader; 3] = [Loader::Apple, Loader::Rockbox, Loader::IPodLoader2];

    /// What the window offers — see [`Os::OFFERED`]. `ipodloader2` goes with iPodLinux: it is the
    /// only thing that needs it, and a bootloader with nothing to boot is a menu with one entry.
    pub const OFFERED: [Loader; 2] = [Loader::Apple, Loader::Rockbox];
}

/// Where the drive comes from before anything is installed onto it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Start {
    /// Built here from an Apple bundle, by name in the resources. Volumes built this way are FAT32
    /// type `0x0B`, which matters — see [`Recipe::check`].
    FromIpsw(String),
    /// An image the person already has. `fat_type` is what its data partition actually says, when
    /// it has been read; `None` when it has not been looked at yet.
    FromImage { path: String, fat_type: Option<u8> },
    /// A disk already in the library, by name.
    ///
    /// **Distinct from a file somebody picks**, because the library knows what is on it — an
    /// existing disk arrives with its `built_from` and its install list, so a device made from one
    /// can say what it will boot without opening anything.
    FromDisk { name: String, fat_type: Option<u8> },
}

/// A drive somebody is describing: where it starts, what bootloader, and which systems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recipe {
    pub start: Start,
    pub loader: Loader,
    pub oses: BTreeSet<Os>,
}

impl Default for Recipe {
    /// **Apple's software from a fetched bundle, on Apple's own bootloader.** What somebody who has
    /// not chosen anything means by "an iPod".
    ///
    /// The bundle has no name yet, so this state is **refused** — [`Recipe::check`] rule (0). The
    /// default says what the boxes are ticked to; it does not claim a plan.
    fn default() -> Self {
        Recipe {
            start: Start::FromIpsw(String::new()),
            loader: Loader::Apple,
            oses: [Os::Apple].into_iter().collect(),
        }
    }
}

/// The half of a [`Recipe`] that decides how long a cold boot takes.
///
/// **A device's progress bar has one denominator — its own last completed cold boot — and that is
/// honest only while the device keeps booting the same thing.** Install Rockbox onto a device that
/// learned ~1.6 G on RetailOS and it reaches its menu at ~100 M, so the bar reads 6 % at the moment
/// the machine is finished; go the other way and it passes 100 % and keeps going. So the number is
/// stored with the shape that produced it, and a shape that no longer matches is a number that
/// gets dropped rather than trusted.
///
/// **The drive it starts from is deliberately not in here**, for three reasons in order of weight.
/// [`Start`] carries `fat_type`, which goes from `None` to `Some(_)` when a background read of the
/// volume finishes — a discovery, not an edit — so a whole-`Recipe` comparison would throw away a
/// good denominator because a read completed, which is a number changing for a reason the user did
/// not cause. Renaming the `.ipsw` a drive was built from moves RetailOS's cold boot by a few per
/// cent, not by the order of magnitude this exists to catch. And the next completed boot overwrites
/// the number anyway.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootShape {
    pub loader: Loader,
    pub oses: BTreeSet<Os>,
}

impl BootShape {
    /// The bootloader, then the systems, comma separated — one settings line.
    ///
    /// The bootloader is first **positionally**, which is what makes `rockbox, apple, rockbox`
    /// unambiguous. The systems follow in the `BTreeSet`'s order, which is the enum's declaration
    /// order, so the file is reproducible.
    pub fn render(&self) -> String {
        std::iter::once(self.loader.as_str())
            .chain(self.oses.iter().map(|o| o.as_str()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Back again, or `None` if any token is not one this build knows.
    ///
    /// **All or nothing.** Half a shape is a *wrong* shape that compares equal to a real one, and
    /// the whole point of storing it is that a mismatch drops the denominator; no shape is honest
    /// and costs one boot without a bar. An empty system set is a shape — an empty drive — and not
    /// an absence, so `"apple"` parses and `""` does not.
    pub fn parse(s: &str) -> Option<BootShape> {
        let mut tokens = s.split(',');
        let loader = Loader::parse(tokens.next()?)?;
        let mut oses = BTreeSet::new();
        for t in tokens {
            oses.insert(Os::parse(t)?);
        }
        Some(BootShape { loader, oses })
    }
}

/// A change that would make an impossible recipe possible, offered as a button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fix {
    UseLoader(Loader),
    AddOs(Os),
    RemoveOs(Os),
    /// Start from an Apple bundle instead of the supplied image — the only answer to a `0x0C`
    /// volume that does not involve rewriting somebody's partition table.
    BuildFromIpsw,
}

impl Fix {
    pub fn label(&self) -> String {
        match self {
            Fix::UseLoader(l) => format!("use {}", l.label()),
            Fix::AddOs(o) => format!("add {}", o.label()),
            Fix::RemoveOs(o) => format!("remove {}", o.label()),
            Fix::BuildFromIpsw => "build from Apple's firmware instead".into(),
        }
    }
}

/// What a recipe will do, or why it will not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// It works. The string is what the person gets — the boot menu, or how to reach each system.
    Ok(String),
    /// It does not. `why` names the constraint; `fix` is one click that resolves it, when one
    /// exists that does not involve the person going and finding a file.
    No { why: String, fix: Option<Fix> },
}

impl Verdict {
    pub fn ok(&self) -> bool {
        matches!(self, Verdict::Ok(_))
    }
    pub fn text(&self) -> &str {
        match self {
            Verdict::Ok(s) => s,
            Verdict::No { why, .. } => why,
        }
    }
}

impl Recipe {
    /// Is this drive buildable, and what will it do?
    ///
    /// The rules, each with where it was measured:
    ///
    /// 0. **Nobody has said where the drive comes from yet.** The verdict region is always
    ///    reserved, so before this arm existed it read `Starts Apple's software, the way the iPod
    ///    shipped.` for a firmware nobody had chosen — a plan asserted for nothing. It carries no
    ///    [`Fix`], and does not need one: the picker one row above is what resolves it.
    /// 1. **iPodLinux requires `ipodloader2`.** `install::install_linux` puts the loader in the
    ///    firmware partition and writes `loader.cfg` beside the kernel; there is no path in this
    ///    project, or upstream, that starts a ZeroSlackr kernel from anything else.
    /// 2. **`ipodloader2` reads FAT32 type `0x0B` and no other.** `vfs.c` has `case 0x83` and
    ///    `case 0xB` and nothing else; a `0x0C` volume prints `Unknown 0xC` then `No valid
    ///    paritions found!` — the loader's own spelling. Both types are legitimate FAT32, and every
    ///    image in this project taken off real hardware is `0x0C` while `make-disk`'s own volumes
    ///    are `0x0B`. `install::install_linux` refuses those drives rather than producing one that
    ///    cannot boot.
    ///
    ///    **The rule fires whenever `ipodloader2` is *required*, not only when it is showing.**
    ///    With iPodLinux ticked on some other bootloader, rule (1) offers `use ipodloader2` — and
    ///    applying that used to land on this refusal, which carries a fix of its own. A fix leading
    ///    to a refusal carrying a fix is exactly what `every_fix_resolves_the_thing_it_is_offered_for`
    ///    promises cannot happen. On a `0x0C` volume iPodLinux is impossible whatever bootloader is
    ///    selected, and the reason is the volume, so that is where the rule is stated.
    /// 3. **Rockbox's bootloader can hand back to Apple's software, and cannot start a kernel.**
    ///    Holding MENU at power-on is the documented hand-back; there is no third entry.
    /// 4. **Apple's own bootloader starts exactly what is in the firmware partition.** So with no
    ///    third-party loader there is room for one system, and it is Apple's.
    /// 5. **An empty drive is a drive.** It boots nothing, and saying so is more useful than
    ///    refusing to build it — it is how you get a volume to put music on.
    pub fn check(&self) -> Verdict {
        // (0)
        if self.nothing_chosen() {
            return Verdict::No {
                why: NOTHING_CHOSEN.into(),
                fix: None,
            };
        }
        self.check_parts()
    }

    /// Has anybody said where the drive comes from?
    ///
    /// All three [`Start`] variants, not only the one the wizard opens on: the window asks the same
    /// question to decide whether to draw the verdict dimmed, and a second copy of this match is
    /// where the third variant gets forgotten. `fat_type` is deliberately not consulted — an image
    /// nobody has read yet has still been *chosen*.
    pub fn nothing_chosen(&self) -> bool {
        match &self.start {
            Start::FromIpsw(name) => name.is_empty(),
            Start::FromImage { path, .. } => path.is_empty(),
            Start::FromDisk { name, .. } => name.is_empty(),
        }
    }

    /// What the data partition of the drive this starts from says it is, when that has been read.
    ///
    /// **Both variants that carry a volume, not only the one somebody picks in a file dialog.** A
    /// disk out of the library is a drive image like any other — and the library's are the ones
    /// most likely to be `0x0C`, because they come off real iPods, which is what rule (2)'s own
    /// refusal text says. Read through one function rather than matched a second time inside the
    /// rule, because a second copy of this match is where the third variant gets forgotten — which
    /// is exactly what happened to [`Recipe::nothing_chosen`]'s twin.
    ///
    /// `FromIpsw` is `None` and not `Some(0x0b)`: nothing has been built yet, so there is no
    /// volume to have a type. That the builder writes `0x0B` is [`Fix::BuildFromIpsw`]'s reason,
    /// not a fact about a drive that exists.
    pub fn volume_type(&self) -> Option<u8> {
        match &self.start {
            Start::FromIpsw(_) => None,
            Start::FromImage { fat_type, .. } | Start::FromDisk { fat_type, .. } => *fat_type,
        }
    }

    /// Whether these parts go together, **whatever the drive starts from**.
    ///
    /// Split out of [`Recipe::check`] so that [`Recipe::loader_works`] and [`Recipe::why_not`] keep
    /// answering *about the bootloader*. Without the split, every bootloader's tooltip reads
    /// `nothing chosen yet` before a firmware is picked — a non-sequitur in a bootloader tooltip,
    /// and a whole picker greyed out for the wrong reason.
    fn check_parts(&self) -> Verdict {
        let has = |o: Os| self.oses.contains(&o);

        // (2) first, because it invalidates a whole loader regardless of what is selected.
        if (self.loader == Loader::IPodLoader2 || has(Os::IPodLinux))
            && self.volume_type() == Some(0x0c)
        {
            return Verdict::No {
                why: "That drive's data partition is FAT32 type 0x0C, the LBA form, and \
                      ipodloader2 reads only 0x0B — it will report `No valid paritions found!`. \
                      Both are legitimate FAT32; drives off real iPods are 0x0C and drives built \
                      here are 0x0B."
                    .into(),
                fix: Some(Fix::BuildFromIpsw),
            };
        }

        // (1)
        if has(Os::IPodLinux) && self.loader != Loader::IPodLoader2 {
            return Verdict::No {
                why: format!(
                    "iPodLinux needs ipodloader2 to start its kernel; {} cannot.",
                    self.loader.label()
                ),
                fix: Some(Fix::UseLoader(Loader::IPodLoader2)),
            };
        }

        // (3)
        if has(Os::Rockbox) && self.loader == Loader::Apple {
            return Verdict::No {
                why: "Apple's bootloader starts whatever is in the firmware partition and nothing \
                      else, so Rockbox needs a bootloader of its own."
                    .into(),
                fix: Some(Fix::UseLoader(Loader::Rockbox)),
            };
        }

        // (4)
        if self.loader == Loader::Rockbox && !has(Os::Rockbox) {
            return Verdict::No {
                why: "Rockbox's bootloader with no Rockbox on the volume starts nothing.".into(),
                fix: Some(if has(Os::Apple) {
                    Fix::UseLoader(Loader::Apple)
                } else {
                    Fix::AddOs(Os::Rockbox)
                }),
            };
        }
        if self.loader == Loader::IPodLoader2 && self.oses.is_empty() {
            return Verdict::No {
                why: "ipodloader2 with nothing to offer draws an empty menu.".into(),
                fix: Some(Fix::AddOs(Os::Apple)),
            };
        }

        // (5)
        if self.oses.is_empty() {
            return Verdict::Ok(
                "An empty drive: a volume you can put files on, and nothing that boots.".into(),
            );
        }

        Verdict::Ok(self.describe())
    }

    /// What booting this drive will be like, in the words the boot menu will actually use.
    fn describe(&self) -> String {
        let mut entries: Vec<&str> = Vec::new();
        match self.loader {
            // `install::loader_menu` writes ZeroSlackr first, then Apple OS, then Rockbox, then
            // Disk Mode and Sleep. This lists them in that order because that is the order they
            // will appear in, and a preview that disagrees with the screen is worse than none.
            Loader::IPodLoader2 => {
                if self.oses.contains(&Os::IPodLinux) {
                    entries.push("ZeroSlackr");
                }
                if self.oses.contains(&Os::Apple) {
                    entries.push("Apple OS");
                }
                if self.oses.contains(&Os::Rockbox) {
                    entries.push("Rockbox");
                }
                entries.push("Disk Mode");
                entries.push("Sleep");
                format!("A boot menu: {}.", entries.join(", "))
            }
            Loader::Rockbox if self.oses.contains(&Os::Apple) => {
                "Starts Rockbox. Hold MENU at power-on for Apple's software.".into()
            }
            Loader::Rockbox => "Starts Rockbox.".into(),
            Loader::Apple => "Starts Apple's software, the way the iPod shipped.".into(),
        }
    }

    /// The bootloader this set of systems wants, if there is exactly one sensible answer.
    ///
    /// **So the wizard defaults instead of complaining.** Ticking iPodLinux and then being told
    /// that the bootloader you had is wrong is a correction; ticking iPodLinux and having the
    /// bootloader follow is the same knowledge applied before it becomes a mistake. The verdict is
    /// still there for the cases somebody drives into deliberately.
    ///
    /// - iPodLinux at all -> `ipodloader2`; nothing else starts a kernel.
    /// - Rockbox without it -> Rockbox's own, which hands back to Apple's software.
    /// - Apple's alone -> none; the ROM starts the firmware partition directly.
    pub fn best_loader(&self) -> Loader {
        if self.oses.contains(&Os::IPodLinux) {
            Loader::IPodLoader2
        } else if self.oses.contains(&Os::Rockbox) {
            Loader::Rockbox
        } else {
            Loader::Apple
        }
    }

    /// Whether a bootloader can carry this set of systems at all — what the wizard greys out.
    ///
    /// A disabled control says *that* something is impossible and never *why*, so each one keeps
    /// its reason: [`Recipe::why_not`] is what the tooltip shows.
    /// Goes through the private `check_parts` and not [`Recipe::check`], so that a bootloader is
    /// greyed out for a reason about bootloaders — see the note on `check_parts` itself.
    pub fn loader_works(&self, l: Loader) -> bool {
        let mut trial = self.clone();
        trial.loader = l;
        trial.check_parts().ok()
    }

    /// Why a bootloader cannot carry this set, for the tooltip on the control that is greyed out.
    pub fn why_not(&self, l: Loader) -> String {
        let mut trial = self.clone();
        trial.loader = l;
        match trial.check_parts() {
            Verdict::No { why, .. } => why,
            Verdict::Ok(_) => String::new(),
        }
    }

    /// What this recipe will boot, without the drive it starts from — see [`BootShape`].
    pub fn shape(&self) -> BootShape {
        BootShape {
            loader: self.loader,
            oses: self.oses.clone(),
        }
    }

    /// The number of systems, for the word a person uses: dual boot, triple boot.
    pub fn boot_word(&self) -> Option<&'static str> {
        match self.oses.len() {
            2 => Some("dual boot"),
            3 => Some("triple boot"),
            _ => None,
        }
    }

    /// Everything that has to be fetched, built or installed, in order.
    ///
    /// **Shown before it happens and again while it happens.** One list, rendered twice: as "this
    /// is what will be downloaded" on the way in, and as a checklist with a spinner on the way
    /// through. A plan a person cannot see before agreeing to it is a download they did not agree
    /// to.
    pub fn steps(&self) -> Vec<Step> {
        let mut v = Vec::new();
        match &self.start {
            Start::FromIpsw(name) => {
                v.push(Step::Fetch(format!(
                    "Apple's firmware{}",
                    if name.is_empty() {
                        String::new()
                    } else {
                        format!(" — {name}")
                    }
                )));
                v.push(Step::Build("a drive, 8 GB sparse".into()));
            }
            Start::FromImage { path, .. } => v.push(Step::Copy(path.clone())),
            Start::FromDisk { name, .. } => v.push(Step::Copy(format!("{name}, from the library"))),
        }
        if self.loader == Loader::IPodLoader2 {
            v.push(Step::Install(
                "ipodloader2, into the firmware partition".into(),
            ));
        }
        if self.oses.contains(&Os::Rockbox) {
            v.push(Step::Fetch("Rockbox 4.0".into()));
            v.push(Step::Install(if self.loader == Loader::Rockbox {
                "Rockbox and its bootloader".into()
            } else {
                "Rockbox, onto the volume".into()
            }));
        }
        if self.oses.contains(&Os::IPodLinux) {
            v.push(Step::Fetch("ZeroSlackr".into()));
            v.push(Step::Install(
                "iPodLinux — five directories onto the volume".into(),
            ));
        }
        v
    }
}

/// One line of the plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Fetch(String),
    Build(String),
    Copy(String),
    Install(String),
}

impl Step {
    pub fn verb(&self) -> &'static str {
        match self {
            Step::Fetch(_) => "fetch",
            Step::Build(_) => "build",
            Step::Copy(_) => "copy",
            Step::Install(_) => "install",
        }
    }
    pub fn what(&self) -> &str {
        match self {
            Step::Fetch(s) | Step::Build(s) | Step::Copy(s) | Step::Install(s) => s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe(loader: Loader, oses: &[Os]) -> Recipe {
        Recipe {
            start: Start::FromIpsw("iPod_20.1.3".into()),
            loader,
            oses: oses.iter().copied().collect(),
        }
    }

    /// **The wizard's opening state asserts nothing.** This replaces `the_default_recipe_works`,
    /// which asserted the opposite and was wrong: `Recipe::default()` names no firmware, and
    /// `check()` had no arm for an empty name, so the always-reserved verdict region opened on
    /// `Starts Apple's software, the way the iPod shipped.` — a plan for a bundle nobody had
    /// chosen. The two assertions cannot both hold, which is why the old one is deleted rather
    /// than adjusted.
    #[test]
    fn the_default_recipe_says_nothing_is_chosen_yet() {
        let v = Recipe::default().check();
        assert!(!v.ok(), "the default claims to be buildable: {}", v.text());
        assert_eq!(v.text(), NOTHING_CHOSEN);
        assert!(
            !v.text().contains("Starts Apple's software"),
            "the opening state still describes a plan: {}",
            v.text()
        );
        assert!(
            matches!(v, Verdict::No { fix: None, .. }),
            "a fix was offered for a state the picker above resolves"
        );
    }

    /// Rule (0) covers all three [`Start`] variants, not only the one the wizard opens on. A second
    /// copy of the match is where the third variant gets forgotten.
    #[test]
    fn an_empty_image_path_or_disk_name_is_also_nothing_chosen() {
        for start in [
            Start::FromIpsw(String::new()),
            Start::FromImage {
                path: String::new(),
                fat_type: Some(0x0b),
            },
            Start::FromDisk {
                name: String::new(),
                fat_type: None,
            },
        ] {
            let r = Recipe {
                start: start.clone(),
                ..Recipe::default()
            };
            assert!(r.nothing_chosen(), "{start:?} was read as chosen");
            let v = r.check();
            assert_eq!(v.text(), NOTHING_CHOSEN, "for {start:?}");
        }

        // And a named one of each is chosen — an unread volume has still been picked.
        for start in [
            Start::FromIpsw("iPod_20.1.3".into()),
            Start::FromImage {
                path: "/drives/mine.img".into(),
                fat_type: None,
            },
            Start::FromDisk {
                name: "rockbox-test".into(),
                fat_type: None,
            },
        ] {
            let r = Recipe {
                start: start.clone(),
                ..Recipe::default()
            };
            assert!(!r.nothing_chosen(), "{start:?} was read as unchosen");
        }
    }

    /// The refusal is about the empty name and nothing else: one edit and the same recipe describes
    /// itself again.
    #[test]
    fn choosing_a_firmware_turns_the_refusal_into_a_plan() {
        let mut r = Recipe::default();
        assert_eq!(r.check().text(), NOTHING_CHOSEN);
        r.start = Start::FromIpsw("iPod_20.1.3".into());
        assert!(r.check().ok(), "still refused: {}", r.check().text());
        assert_eq!(
            r.check().text(),
            "Starts Apple's software, the way the iPod shipped."
        );
    }

    /// **A bootloader's tooltip is about the bootloader.** `loader_works` and `why_not` go through
    /// `check_parts`, so the picker is not greyed out wholesale — with its own non-sequitur reason —
    /// merely because no firmware has been chosen yet.
    #[test]
    fn the_bootloader_tooltip_is_about_the_bootloader_even_before_a_firmware_is_chosen() {
        let r = Recipe {
            start: Start::FromIpsw(String::new()),
            loader: Loader::Apple,
            oses: [Os::IPodLinux].into_iter().collect(),
        };
        assert_eq!(r.check().text(), NOTHING_CHOSEN, "the fixture is wrong");

        let why = r.why_not(Loader::Apple);
        assert!(
            why.contains("ipodloader2"),
            "the tooltip is not about the loader: {why}"
        );
        assert!(
            r.loader_works(Loader::IPodLoader2),
            "ipodloader2 was greyed out because no firmware is chosen yet"
        );
    }

    /// Rule (2) is a fact about the **volume**, so it holds whatever bootloader is showing. With the
    /// guard on `loader == IPodLoader2` alone, rule (1) offered `use ipodloader2` and applying it
    /// landed on this refusal — a fix chaining into a fix.
    #[test]
    fn a_zero_c_volume_refuses_ipodlinux_whatever_bootloader_is_showing() {
        // **And whichever way the drive was chosen.** A disk from the library carries the same
        // `fat_type`, and the library's are the drives most likely to be `0x0C` — they come off
        // real iPods, which is what the refusal itself says. Rule (2) matched `FromImage` alone, so
        // the one case the text is about was the one case that passed.
        let volumes = [
            Start::FromImage {
                path: "/drives/mine.img".into(),
                fat_type: Some(0x0c),
            },
            Start::FromDisk {
                name: "off my 5.5G".into(),
                fat_type: Some(0x0c),
            },
        ];
        for start in volumes {
            for loader in Loader::ALL {
                let mut r = recipe(loader, &[Os::IPodLinux]);
                r.start = start.clone();
                let v = r.check();
                assert!(!v.ok(), "{loader:?} was allowed onto a 0x0C {start:?}");
                assert!(
                    v.text().contains("0x0C"),
                    "the reason blames the bootloader rather than the volume, on {loader:?} from \
                     {start:?}: {}",
                    v.text()
                );
                assert_eq!(
                    match &v {
                        Verdict::No { fix, .. } => fix.clone(),
                        _ => None,
                    },
                    Some(Fix::BuildFromIpsw),
                    "no way out of a 0x0C volume on {loader:?} from {start:?}"
                );
            }
        }

        // The other half: `0x0B` from the library is fine, so the widening did not become a
        // blanket refusal of every drive somebody already has.
        let mut ok = recipe(Loader::IPodLoader2, &[Os::IPodLinux]);
        ok.start = Start::FromDisk {
            name: "built here".into(),
            fat_type: Some(0x0b),
        };
        assert!(ok.check().ok(), "a 0x0B library disk was refused: {}", ok.check().text());
    }

    /// Rule 1, and the reason it exists: `install_linux` writes `ipodloader2` into the firmware
    /// partition because nothing else starts a ZeroSlackr kernel.
    #[test]
    fn ipodlinux_without_ipodloader2_is_refused_with_a_fix() {
        for loader in [Loader::Apple, Loader::Rockbox] {
            let v = recipe(loader, &[Os::IPodLinux]).check();
            assert!(!v.ok(), "{loader:?} was allowed to start a kernel");
            assert!(
                v.text().contains("ipodloader2"),
                "the reason does not name it: {}",
                v.text()
            );
            assert_eq!(
                match &v {
                    Verdict::No { fix, .. } => fix.clone(),
                    _ => None,
                },
                Some(Fix::UseLoader(Loader::IPodLoader2)),
                "no one-click fix, so this is a dead end"
            );
        }
        assert!(recipe(Loader::IPodLoader2, &[Os::IPodLinux]).check().ok());
    }

    /// Rule 2 — and it is about the *drive*, not the software, which is why an imported image can
    /// be refused for a combination that a built one accepts.
    #[test]
    fn an_lba_volume_cannot_take_ipodloader2() {
        let mut r = recipe(Loader::IPodLoader2, &[Os::Apple, Os::IPodLinux]);
        r.start = Start::FromImage {
            path: "/drives/mine.img".into(),
            fat_type: Some(0x0c),
        };
        let v = r.check();
        assert!(!v.ok());
        assert!(
            v.text().contains("0x0C"),
            "the reason does not name the type: {}",
            v.text()
        );

        // The same drive at 0x0B is fine, which is what makes this a fact about the volume.
        r.start = Start::FromImage {
            path: "/drives/mine.img".into(),
            fat_type: Some(0x0b),
        };
        assert!(r.check().ok(), "a 0x0B volume was refused");

        // And unknown is not refused: a drive nobody has read yet is not a drive that fails.
        r.start = Start::FromImage {
            path: "/drives/mine.img".into(),
            fat_type: None,
        };
        assert!(r.check().ok(), "an unread volume was refused on a guess");
    }

    /// **Every combination is reachable through some loader.** If one is not, the free checkboxes
    /// are a lie and the fixed list would have been more honest.
    #[test]
    fn every_set_of_systems_can_be_built_by_some_loader() {
        for n in 0..8u8 {
            let oses: Vec<Os> = Os::ALL
                .iter()
                .enumerate()
                .filter(|(i, _)| n & (1 << i) != 0)
                .map(|(_, o)| *o)
                .collect();
            let any = Loader::ALL.iter().any(|l| recipe(*l, &oses).check().ok());
            assert!(any, "no bootloader can build {oses:?}");
        }
    }

    /// A refusal must carry a fix, and **applying the fix must actually work** — otherwise the
    /// button moves you from one dead end to another.
    ///
    /// **Widened to sweep the three starts**, because with only `FromIpsw` in the loop
    /// `Fix::BuildFromIpsw` was never produced and its arm below never executed — so the one fix
    /// that could chain into another refusal was the one fix this test could not see.
    ///
    /// `BuildFromIpsw` lands on `Start::FromIpsw(String::new())`, which is rule (0)'s
    /// nothing-chosen state: a refusal with no fix, resolved by the picker one row above rather
    /// than by a button. That is the second arm of the assertion, and it is not a loophole — a fix
    /// that ended anywhere else with a fix attached would still fail.
    #[test]
    fn every_fix_resolves_the_thing_it_is_offered_for() {
        for start in [
            Start::FromIpsw("iPod_20.1.3".into()),
            Start::FromImage {
                path: "/drives/mine.img".into(),
                fat_type: Some(0x0b),
            },
            Start::FromImage {
                path: "/drives/mine.img".into(),
                fat_type: Some(0x0c),
            },
            // **The library's own drives, both volume types.** A disk out of the library carries
            // the same `fat_type` field, and rule (2) used to match `FromImage` alone — so a drive
            // off real hardware, which compose's own refusal text says is always `0x0C`, passed
            // with iPodLinux ticked while `install::install_linux` refused it.
            //
            // These two rows are **coverage, not the guard**: narrowing `volume_type` back leaves
            // this sweep green, because without rule (2) firing, rule (1)'s fix simply resolves.
            // `a_zero_c_volume_refuses_ipodlinux_whatever_bootloader_is_showing` is what goes red,
            // and it is where the FromDisk case is actually asserted.
            Start::FromDisk {
                name: "off my 5.5G".into(),
                fat_type: Some(0x0b),
            },
            Start::FromDisk {
                name: "off my 5.5G".into(),
                fat_type: Some(0x0c),
            },
        ] {
            for n in 0..8u8 {
                for loader in Loader::ALL {
                    let oses: Vec<Os> = Os::ALL
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| n & (1 << i) != 0)
                        .map(|(_, o)| *o)
                        .collect();
                    let mut r = recipe(loader, &oses);
                    r.start = start.clone();
                    let Verdict::No { why, fix } = r.check() else {
                        continue;
                    };
                    let fix =
                        fix.unwrap_or_else(|| panic!("no fix offered for {start:?}: {why}"));
                    let mut fixed = r.clone();
                    match &fix {
                        Fix::UseLoader(l) => fixed.loader = *l,
                        Fix::AddOs(o) => {
                            fixed.oses.insert(*o);
                        }
                        Fix::RemoveOs(o) => {
                            fixed.oses.remove(o);
                        }
                        Fix::BuildFromIpsw => fixed.start = Start::FromIpsw(String::new()),
                    }
                    let after = fixed.check();
                    assert!(
                        after.ok()
                            || (fixed.nothing_chosen()
                                && matches!(after, Verdict::No { fix: None, .. })),
                        "the fix {:?} for {oses:?} on {loader:?} from {start:?} led to another \
                         refusal: {}",
                        fix.label(),
                        after.text()
                    );
                }
            }
        }
    }

    /// The preview has to match `install::loader_menu`, which writes ZeroSlackr, Apple OS, Rockbox,
    /// Disk Mode, Sleep — in that order. A preview that disagrees with the screen is worse than
    /// none, because it is the only thing a person has to check their choice against.
    #[test]
    fn the_menu_preview_is_in_the_order_the_loader_writes() {
        let v = recipe(
            Loader::IPodLoader2,
            &[Os::Apple, Os::Rockbox, Os::IPodLinux],
        )
        .check();
        assert_eq!(
            v.text(),
            "A boot menu: ZeroSlackr, Apple OS, Rockbox, Disk Mode, Sleep."
        );
        let real = crate::install::loader_menu_for_tests(true, true);
        for entry in ["ZeroSlackr", "Apple OS", "Rockbox", "Disk Mode", "Sleep"] {
            assert!(real.contains(entry), "loader_menu no longer writes {entry}");
        }
    }

    /// **What the window offers must be closed under its own rules.** Every set of offered systems
    /// has to be buildable by an offered bootloader — otherwise removing iPodLinux from the list
    /// would have left a combination that can be ticked and never built.
    #[test]
    fn every_offered_combination_is_buildable_with_an_offered_loader() {
        for n in 0..(1 << Os::OFFERED.len()) {
            let oses: Vec<Os> = Os::OFFERED
                .iter()
                .enumerate()
                .filter(|(i, _)| n & (1 << i) != 0)
                .map(|(_, o)| *o)
                .collect();
            let r = recipe(Loader::Apple, &oses);
            let best = r.best_loader();
            assert!(
                Loader::OFFERED.contains(&best),
                "{oses:?} defaults to {best:?}, which the window does not offer"
            );
            let mut fixed = r.clone();
            fixed.loader = best;
            assert!(fixed.check().ok(), "{oses:?} on {best:?} is refused");
        }
    }

    /// **The default must always be a working default.** A wizard that picks a bootloader and then
    /// refuses the result is worse than one that never picked.
    #[test]
    fn the_best_loader_for_any_set_of_systems_actually_works() {
        for n in 0..8u8 {
            let oses: Vec<Os> = Os::ALL
                .iter()
                .enumerate()
                .filter(|(i, _)| n & (1 << i) != 0)
                .map(|(_, o)| *o)
                .collect();
            let mut r = recipe(Loader::Apple, &oses);
            r.loader = r.best_loader();
            assert!(
                r.check().ok(),
                "the default for {oses:?} is {:?}, which is refused: {}",
                r.loader,
                r.check().text()
            );
        }
    }

    /// And every bootloader that is greyed out has to say why, or the grey is just a wall.
    #[test]
    fn a_bootloader_that_cannot_be_used_says_why() {
        let r = recipe(Loader::IPodLoader2, &[Os::IPodLinux]);
        for l in Loader::ALL {
            if r.loader_works(l) {
                continue;
            }
            let why = r.why_not(l);
            assert!(!why.is_empty(), "{l:?} is disabled with no reason given");
            assert!(
                why.contains("ipodloader2"),
                "the reason is not about the loader: {why}"
            );
        }
    }

    /// Three systems is the word people use for it, and the plan says what it will cost.
    #[test]
    fn a_triple_boot_plan_names_everything_it_will_fetch() {
        let r = recipe(
            Loader::IPodLoader2,
            &[Os::Apple, Os::Rockbox, Os::IPodLinux],
        );
        assert_eq!(r.boot_word(), Some("triple boot"));
        let steps = r.steps();
        let fetched: Vec<&str> = steps
            .iter()
            .filter(|s| matches!(s, Step::Fetch(_)))
            .map(|s| s.what())
            .collect();
        assert_eq!(
            fetched.len(),
            3,
            "not everything that downloads is listed: {fetched:?}"
        );
        assert!(steps.iter().any(|s| s.what().contains("ipodloader2")));
    }

    /// **A discovery is not an edit.** `fat_type` goes from `None` to `Some(_)` when a background
    /// read of the volume finishes, and the user did nothing. If that counted as a change to what
    /// the device boots, a good progress-bar denominator would be thrown away because a read
    /// completed — so the drive is deliberately not part of the shape.
    #[test]
    fn a_volume_type_discovered_later_is_not_a_change_to_what_a_device_boots() {
        let mut a = recipe(Loader::Rockbox, &[Os::Apple, Os::Rockbox]);
        a.start = Start::FromImage {
            path: "/drives/mine.img".into(),
            fat_type: None,
        };
        let mut b = a.clone();
        b.start = Start::FromImage {
            path: "/drives/mine.img".into(),
            fat_type: Some(0x0b),
        };
        assert_ne!(a, b, "the fixture does not actually differ");
        assert_eq!(
            a.shape(),
            b.shape(),
            "reading the volume counted as a change to what the device boots"
        );
    }

    /// **All or nothing.** Half a shape is a wrong shape that compares equal to a real one, and a
    /// wrong shape keeps a denominator it cannot vouch for.
    #[test]
    fn an_unreadable_boot_shape_is_no_shape_rather_than_a_wrong_one() {
        assert_eq!(
            BootShape::parse("rockbox, apple, rockbox"),
            Some(BootShape {
                loader: Loader::Rockbox,
                oses: [Os::Apple, Os::Rockbox].into_iter().collect(),
            })
        );
        // An empty system set is a shape — an empty drive — and not an absence.
        assert_eq!(
            BootShape::parse("apple"),
            Some(BootShape {
                loader: Loader::Apple,
                oses: BTreeSet::new(),
            })
        );
        for junk in ["rockbox, apple, banana", "banana", "", "apple,", "  "] {
            assert_eq!(
                BootShape::parse(junk),
                None,
                "{junk:?} was read as a boot shape"
            );
        }
    }

    /// Writer and reader cannot drift, and no two shapes share a line — otherwise a device could
    /// be told its recipe was unchanged when it was not.
    #[test]
    fn every_pair_of_bootloader_and_systems_is_its_own_line() {
        let mut seen: Vec<(String, BootShape)> = Vec::new();
        for loader in Loader::ALL {
            for n in 0..8u8 {
                let oses: BTreeSet<Os> = Os::ALL
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| n & (1 << i) != 0)
                    .map(|(_, o)| *o)
                    .collect();
                let sh = BootShape { loader, oses };
                let line = sh.render();
                assert_eq!(
                    BootShape::parse(&line),
                    Some(sh.clone()),
                    "{line:?} did not come back as itself"
                );
                if let Some((_, other)) = seen.iter().find(|(l, _)| *l == line) {
                    panic!("{:?} and {:?} both render as {line:?}", other, sh);
                }
                seen.push((line, sh));
            }
        }
        assert_eq!(seen.len(), 24);
    }
}
