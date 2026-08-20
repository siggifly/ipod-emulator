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
    pub const ALL: [Os; 3] = [Os::Apple, Os::Rockbox, Os::IPodLinux];
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
    pub const ALL: [Loader; 3] = [Loader::Apple, Loader::Rockbox, Loader::IPodLoader2];
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
    /// A partitioned drive with nothing on it.
    Empty,
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
    fn default() -> Self {
        Recipe {
            start: Start::FromIpsw(String::new()),
            loader: Loader::Apple,
            oses: [Os::Apple].into_iter().collect(),
        }
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
    /// 1. **iPodLinux requires `ipodloader2`.** `install::install_linux` puts the loader in the
    ///    firmware partition and writes `loader.cfg` beside the kernel; there is no path in this
    ///    project, or upstream, that starts a ZeroSlackr kernel from anything else.
    /// 2. **`ipodloader2` reads FAT32 type `0x0B` and no other.** `vfs.c` has `case 0x83` and
    ///    `case 0xB` and nothing else; a `0x0C` volume prints `Unknown 0xC` then `No valid
    ///    paritions found!` — the loader's own spelling. Both types are legitimate FAT32, and every
    ///    image in this project taken off real hardware is `0x0C` while `make-disk`'s own volumes
    ///    are `0x0B`. `install::install_linux` refuses those drives rather than producing one that
    ///    cannot boot.
    /// 3. **Rockbox's bootloader can hand back to Apple's software, and cannot start a kernel.**
    ///    Holding MENU at power-on is the documented hand-back; there is no third entry.
    /// 4. **Apple's own bootloader starts exactly what is in the firmware partition.** So with no
    ///    third-party loader there is room for one system, and it is Apple's.
    /// 5. **An empty drive is a drive.** It boots nothing, and saying so is more useful than
    ///    refusing to build it — it is how you get a volume to put music on.
    pub fn check(&self) -> Verdict {
        let has = |o: Os| self.oses.contains(&o);

        // (2) first, because it invalidates a whole loader regardless of what is selected.
        if self.loader == Loader::IPodLoader2 {
            if let Start::FromImage {
                fat_type: Some(0x0c),
                ..
            } = &self.start
            {
                return Verdict::No {
                    why: "That image's data partition is FAT32 type 0x0C, the LBA form, and \
                          ipodloader2 reads only 0x0B — it will report `No valid paritions found!`. \
                          Both are legitimate FAT32; drives off real iPods are 0x0C and drives built \
                          here are 0x0B."
                        .into(),
                    fix: Some(Fix::BuildFromIpsw),
                };
            }
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

        // Apple's software has to come from somewhere.
        if has(Os::Apple) && self.start == Start::Empty {
            return Verdict::No {
                why: "Apple's software comes out of an .ipsw — an empty drive has none.".into(),
                fix: Some(Fix::RemoveOs(Os::Apple)),
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
            Start::Empty => v.push(Step::Build("an empty drive, 8 GB sparse".into())),
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

    /// **The default has to be buildable**, or the wizard opens on an error.
    #[test]
    fn the_default_recipe_works() {
        let v = Recipe::default().check();
        assert!(v.ok(), "the default is refused: {}", v.text());
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
    #[test]
    fn every_fix_resolves_the_thing_it_is_offered_for() {
        for n in 0..8u8 {
            for loader in Loader::ALL {
                let oses: Vec<Os> = Os::ALL
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| n & (1 << i) != 0)
                    .map(|(_, o)| *o)
                    .collect();
                let r = recipe(loader, &oses);
                let Verdict::No { why, fix } = r.check() else {
                    continue;
                };
                let fix = fix.unwrap_or_else(|| panic!("no fix offered for: {why}"));
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
                assert!(
                    fixed.check().ok(),
                    "the fix {:?} for {oses:?} on {loader:?} led to another refusal: {}",
                    fix.label(),
                    fixed.check().text()
                );
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
}
