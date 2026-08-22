//! The command line: what this binary answers without opening a window, and what it refuses.
//!
//! **There is no toolkit in this file**, and none may enter it — the same rule `rail.rs`, `nav.rs`
//! and `work.rs` keep. It names no generated type and takes no window; [`parse`] is a pure function
//! of a `&[String]`, and [`run`] writes into two writers a test can hand it. That is what makes
//! every sentence below checkable with no display.
//!
//! ## Why this file exists at all
//!
//! `fn main` went straight to the window and read `argv` **nowhere**. Every flag the program used
//! to take died with the egui `main.rs`, and two workflow steps did not notice: CI ran
//! `--check-update` and `--help` against a binary that recognised neither — the first was ignored
//! and the second opened a window, which on a runner is a job that hangs or a backend that fails.
//! An unrecognised flag opening a window is the worst of the three possible answers, because it
//! looks like the flag worked.
//!
//! ## The rule this file is built around
//!
//! **A flag that is listed is a flag that works, and a flag that is named is a flag that answers.**
//! The old `--help` broke the second half of that in both directions at once: it described 38 flags
//! and the program honoured 45, so seven working flags — `--boot=`, `--press=`, `--second-core`,
//! `--shot-after=`, `--window-shot=`, `--make-app` and `--help` itself — were undocumented. Counted
//! off `65fecaf^`'s `main.rs`, not remembered.
//!
//! Here, [`FLAGS`] is the closed set the parser accepts and [`HELP`] is the text a person reads, and
//! three tests hold them against each other: every entry in `FLAGS` appears in `HELP`, every flag
//! token in `HELP` is in `FLAGS`, and every entry in `FLAGS` parses to something that is not a
//! refusal. Deleting a `parse` arm, a `FLAGS` entry, or a `HELP` line each turns a **different** one
//! of the three red, which is the property a single table generating both halves would not have.
//!
//! ## And the thirty-nine that are not here
//!
//! [`RETIRED`] is the rest of that count. They are not silently ignored and they are not quietly
//! accepted: naming one prints what it used to do and why it is absent, and exits 2. A flag restored
//! into a program that cannot honour it is worse than one left out — but so is a flag that
//! disappears without saying it ever existed, because the person typing it has a script that used
//! to work.

use std::io::Write;
use std::path::PathBuf;

use eapp_loader::settings::Settings;

use crate::{bundle, update};

/// What one launch was asked to do.
///
/// Every variant but [`Cli::Window`] runs with **no window at all**, so all of them work over SSH
/// and on a CI runner with no display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cli {
    /// No flags. Open the window — the only path that reaches the toolkit.
    Window,
    /// Print [`HELP`] and exit 0.
    Help,
    /// Ask GitHub for the latest release. Exit 0 whether or not the network answers.
    CheckUpdate,
    /// Parse a NOR dump and a drive image, say what they are, exit 0 if both are usable.
    ///
    /// `None` on either means *whatever the window last recorded*, resolved in [`run`] rather than
    /// here so that parsing stays a pure function of the arguments.
    CheckImages {
        flash: Option<PathBuf>,
        disk: Option<PathBuf>,
    },
    /// Wrap **this** binary in a macOS `.app` bundle under `out`.
    MakeApp {
        out: PathBuf,
        icon: Option<PathBuf>,
    },
    /// The command line was not one this build can answer. The string is the whole of what to print;
    /// it names the offending word and says what to do instead.
    Refused(String),
}

/// Why a flag the old window took is not here.
///
/// Two answers, and the difference is not cosmetic: one of these becomes false the moment a machine
/// starts, and the other stays true for as long as this window exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gone {
    /// It configured the emulator. `emu.rs` is compiled and its own tests pass, but nothing in this
    /// build calls `emu::run` — so the flag has nowhere to land.
    Machine,
    /// It drove the window that was deleted: its modes, its drop path, its screenshot key.
    Window,
}

impl Gone {
    /// The sentence a refusal prints after naming the flag.
    fn because(self) -> &'static str {
        match self {
            Gone::Machine => {
                "it configured the emulator, and this build starts no machine yet — `emu.rs` is \
                 compiled and tested, and nothing calls it"
            }
            Gone::Window => {
                "it drove the window that was replaced, and that window is gone"
            }
        }
    }
}

/// The flags this build honours. **Closed set**, and the parser accepts nothing outside it.
///
/// A trailing `=` is part of the spelling: `--flash=` takes its value attached, and `--flash` on its
/// own is not this flag.
pub const FLAGS: &[&str] = &[
    "-h",
    "--help",
    "--check-update",
    "--check-images",
    "--flash=",
    "--disk=",
    "--make-app",
];

/// The flags the window before this one took, and why each is absent.
///
/// Enumerated off `65fecaf^:tools/ipod-gui/src/main.rs` — the union of what `print_help` described
/// and what `config` and `main` actually read — minus the seven that survive above. The zenity and
/// kdialog arguments that file also spelled with two dashes are not flags of this program and are
/// not here.
const RETIRED: &[(&str, Gone)] = &[
    ("--ablate=", Gone::Machine),
    ("--boot=", Gone::Machine),
    ("--charger", Gone::Machine),
    ("--clock=", Gone::Machine),
    ("--clock-v3", Gone::Machine),
    ("--cold", Gone::Machine),
    ("--control=", Gone::Machine),
    ("--cop-awake", Gone::Machine),
    ("--copy", Gone::Machine),
    ("--debug", Gone::Window),
    ("--headless=", Gone::Machine),
    ("--input-regs=", Gone::Machine),
    ("--ipsw=", Gone::Window),
    ("--no-copy", Gone::Machine),
    ("--no-ide-irq-latch", Gone::Machine),
    ("--no-idle-stop", Gone::Machine),
    ("--power-cycle-at=", Gone::Machine),
    ("--press=", Gone::Window),
    ("--probe=", Gone::Machine),
    ("--probe-at=", Gone::Machine),
    ("--profile", Gone::Machine),
    ("--read-count=", Gone::Machine),
    ("--regs-at=", Gone::Machine),
    ("--samples=", Gone::Machine),
    ("--save-region=", Gone::Machine),
    ("--second-core", Gone::Machine),
    ("--selftest", Gone::Machine),
    ("--selftest-control", Gone::Machine),
    ("--shot-after=", Gone::Window),
    ("--snap-at=", Gone::Machine),
    ("--snapshot=", Gone::Machine),
    ("--trace-calls-from=", Gone::Machine),
    ("--trace-pc=", Gone::Machine),
    ("--user", Gone::Window),
    ("--watch=", Gone::Machine),
    ("--watch-writes=", Gone::Machine),
    ("--wheel-click-instr=", Gone::Machine),
    ("--window-shot=", Gone::Window),
    ("--workdisk=", Gone::Machine),
];

/// What `--help` prints, verbatim.
///
/// **Hand-written rather than generated from [`FLAGS`]**, and that is the point rather than a
/// shortcut not taken: a table that produced both halves could lose a flag from both at once and
/// stay green through it. Two texts held against each other by two tests can each catch the other
/// losing something. The version is filled in at run time by [`help`].
const HELP: &str = "\
ipod-emulator {v} — an interactive iPod 5G over the eapp-loader emulator

With no arguments it opens the window. These run and exit instead, with no window at all,
so they work over SSH and on a runner with no display:

  -h, --help              print this and exit
  --check-update          ask GitHub for the latest release, and print what it said. Silent
                          when offline; exit 0 either way, so it cannot fail a build
  --check-images          parse a NOR dump and a drive image, say what each is, and exit 0
                          only if both are usable. Takes no window and no machine
    --flash=FILE          the NOR dump to check. Default: the one the window last recorded
    --disk=FILE           the drive image to check. Default: as above
  --make-app OUT [ICON]   macOS only: write OUT/ipod-emulator.app around THIS binary, so the
                          bundle cannot be built around a stale one. ICON is a PNG and is
                          optional — without it the bundle simply has no icon. Nothing is
                          signed and nothing is notarised

Every other flag the window before this one took drove the emulator or that window itself.
Naming one says so and exits 2, rather than being ignored while a window opens.
";

/// [`HELP`] with this build's version in it.
pub fn help() -> String {
    HELP.replace("{v}", update::VERSION)
}

/// Turn `argv` — **without** the program name — into the one thing this launch was asked to do.
///
/// Pure: no filesystem, no network, no environment. Everything that has to ask the machine a
/// question happens in [`run`].
///
/// `--help` wins over everything, including a malformed rest of the line, because somebody who has
/// got the line wrong is exactly who is asking.
pub fn parse(args: &[String]) -> Cli {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        return Cli::Help;
    }

    let mut verb: Option<Cli> = None;
    let mut flash: Option<PathBuf> = None;
    let mut disk: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let taken = match a {
            "--check-update" => Some(Cli::CheckUpdate),
            "--check-images" => Some(Cli::CheckImages {
                flash: None,
                disk: None,
            }),
            "--make-app" => {
                // The operands are positional and follow immediately. A word starting with `-` is
                // the next flag, not a path: `--make-app --check-update` must refuse for having two
                // verbs, not silently write a bundle into a directory called `--check-update`.
                let mut rest = args[i + 1..].iter().take_while(|s| !s.starts_with('-'));
                let Some(out) = rest.next() else {
                    return Cli::Refused(
                        "--make-app: where to write it is not optional. \
                         `--make-app OUT [ICON]`, and OUT is a directory that will hold \
                         `ipod-emulator.app`."
                            .into(),
                    );
                };
                let icon = rest.next();
                i += 1 + usize::from(icon.is_some());
                Some(Cli::MakeApp {
                    out: PathBuf::from(out),
                    icon: icon.map(PathBuf::from),
                })
            }
            _ => None,
        };
        if let Some(v) = taken {
            if let Some(had) = &verb {
                return Cli::Refused(format!(
                    "{}: this build does one thing per run, and `{}` was already asked for.",
                    a,
                    name_of(had)
                ));
            }
            verb = Some(v);
            i += 1;
            continue;
        }
        if let Some(p) = a.strip_prefix("--flash=") {
            flash = Some(PathBuf::from(p));
        } else if let Some(p) = a.strip_prefix("--disk=") {
            disk = Some(PathBuf::from(p));
        } else {
            return Cli::Refused(refusal(a));
        }
        i += 1;
    }

    match verb {
        Some(Cli::CheckImages { .. }) => Cli::CheckImages { flash, disk },
        // **`--flash=` and `--disk=` are operands of `--check-images` and of nothing else here**,
        // so anything else on the line leaves them unread — and an unread flag is the whole defect
        // this file exists to remove. The old window took them as *launch* configuration for a
        // machine it then booted; accepting them beside another verb, or beside no verb at all,
        // would be those flags surviving in spelling only.
        v if flash.is_some() || disk.is_some() => Cli::Refused(format!(
            "--flash= and --disk= are read by --check-images and by nothing else in this build. {}",
            match &v {
                Some(had) => format!("`{}` does not take them.", name_of(had)),
                None => "Add --check-images, which does.".to_string(),
            }
        )),
        Some(v) => v,
        None => Cli::Window,
    }
}

/// The flag a verb was spelled with, for a message that has to name two of them.
fn name_of(v: &Cli) -> &'static str {
    match v {
        Cli::Window => "(none)",
        Cli::Help => "--help",
        Cli::CheckUpdate => "--check-update",
        Cli::CheckImages { .. } => "--check-images",
        Cli::MakeApp { .. } => "--make-app",
        Cli::Refused(_) => "(a refusal)",
    }
}

/// What to print about a word this build does not take.
///
/// Three answers, in order of how much the person already had right.
///
/// A word that was a flag of the old window is answered **as one**, because whoever typed it has a
/// script that used to work and is owed the reason rather than "unknown option". A live flag spelled
/// the other way round is answered as a spelling, because `--flash FILE` and `--flash=FILE` are one
/// keystroke apart and "not a flag of this build" is a wrong answer to the first of them. Everything
/// else is named and pointed at `--help`.
///
/// **This is the only thing that ships reading [`FLAGS`]**, and it is why that table is not
/// `#[cfg(test)]`: the sweeps hold the table against the help text, and this holds it against what
/// somebody actually typed.
fn refusal(word: &str) -> String {
    // `--flash=x` and `--flash` share a head. The tables spell an attached value with a trailing
    // `=`, so comparing heads is what lets one lookup answer both spellings.
    let head = word.split_once('=').map_or(word, |(h, _)| h);
    let attached = word.contains('=');

    if !attached && FLAGS.contains(&format!("{head}=").as_str()) {
        return format!(
            "{head} takes its value attached, with no space: `{head}=FILE`. \
             `--help` shows both of them in place."
        );
    }
    if attached && FLAGS.contains(&head) {
        return format!(
            "{head} takes what follows it as separate words, not attached with `=`. \
             `--help` shows the shape."
        );
    }
    if let Some((f, gone)) = RETIRED
        .iter()
        .find(|(f, _)| *f == word || f.trim_end_matches('=') == head)
    {
        return format!(
            "{f} is not a flag of this build: {}. `--help` lists the ones that are.",
            gone.because()
        );
    }
    if word.starts_with('-') {
        return format!("{word} is not a flag of this build. `--help` lists the ones that are.");
    }
    // A bare word. The old window took files by being dropped on, which is a gesture and not an
    // argument; it never took one from the command line either, so this is not a regression being
    // announced — it is a mistake being named rather than ignored.
    format!(
        "{word}: this build takes no file arguments. `--check-images --flash=FILE --disk=FILE` \
         inspects a pair without opening anything."
    )
}

/// Do it, and return the process's exit code.
///
/// Two writers rather than `println!`, so a test reads exactly what a person reads. `out` is the
/// answer and `err` is the complaint, which is the split a shell script depends on.
///
/// **One thing does not go through `out`**, and it is worth saying rather than hiding: the two
/// reports `--check-images` prints are `eapp_loader::inspect::report`'s own, written to stdout by
/// the crate that knows how to read a NOR dump. Routing them through here would mean a second
/// implementation of the same paragraph, and the two would drift. Its exit code is this function's.
pub fn run(cli: &Cli, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    match cli {
        // The one path that reaches the toolkit does not come through here at all — `main` matches
        // it off before calling this, so nothing in this file can open a window by accident.
        Cli::Window => 0,
        Cli::Help => {
            let _ = write!(out, "{}", help());
            0
        }
        Cli::CheckUpdate => {
            match update::check() {
                Some(f) => {
                    let _ = writeln!(out, "{}", f.line());
                }
                // Offline is the expected answer, not a failure, and this is the only place it is
                // ever said out loud — the window says nothing at all when a check does not answer.
                None => {
                    let _ = writeln!(
                        out,
                        "No answer from GitHub. That is the expected result offline, and it is not \
                         an error: nothing was downloaded, nothing was changed, and this build is \
                         {}.",
                        update::VERSION
                    );
                }
            }
            0
        }
        Cli::CheckImages { flash, disk } => {
            // Settings are read **only** when something is missing from the command line. A pair
            // given in full is answered without touching the operator's file at all, which is what
            // makes this path identical in CI and on a machine that has been used.
            let (flash, disk) = match (flash, disk) {
                (Some(f), Some(d)) => (Some(f.clone()), Some(d.clone())),
                _ => {
                    let saved = Settings::load();
                    (
                        flash.clone().or_else(|| saved.flash()),
                        disk.clone().or_else(|| saved.disk.clone()),
                    )
                }
            };
            match (flash, disk) {
                (Some(f), Some(d)) => eapp_loader::inspect::report(&f, &d),
                (f, d) => {
                    // Naming the half that is missing, rather than reporting `UNREADABLE ` against
                    // an empty path — which would be this program inventing a verdict about a file
                    // nobody named.
                    let _ = writeln!(
                        err,
                        "--check-images: nothing to check{}. Name them: --check-images \
                         --flash=FILE --disk=FILE.",
                        match (f.is_none(), d.is_none()) {
                            (true, true) => " — no NOR dump and no drive image",
                            (true, false) => " — no NOR dump",
                            _ => " — no drive image",
                        }
                    );
                    2
                }
            }
        }
        Cli::MakeApp { out: dir, icon } => match bundle::make_app(dir, icon.as_deref()) {
            Ok(app) => {
                let _ = writeln!(out, "{}", app.display());
                0
            }
            Err(e) => {
                let _ = writeln!(err, "--make-app: {e}");
                1
            }
        },
        Cli::Refused(why) => {
            let _ = writeln!(err, "{why}");
            2
        }
    }
}

/// Every flag-shaped word in a text, normalised to the spelling [`FLAGS`] uses.
///
/// `--flash=FILE` is the flag `--flash=`; `--check-images,` at the end of a sentence is
/// `--check-images`. Used by the two sweeps below and by nothing that ships.
#[cfg(test)]
fn flag_tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let word = word.trim_matches(|c: char| c == ',' || c == '.' || c == '`' || c == ';');
        if !word.starts_with('-') || word.len() < 2 {
            continue;
        }
        let token = match word.find('=') {
            Some(i) => &word[..=i],
            None => word,
        };
        if !out.iter().any(|s| s == token) {
            out.push(token.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    /// `run`'s two writers, as strings.
    fn ran(cli: &Cli) -> (i32, String, String) {
        let (mut o, mut e) = (Vec::new(), Vec::new());
        let code = run(cli, &mut o, &mut e);
        (
            code,
            String::from_utf8(o).expect("stdout is text"),
            String::from_utf8(e).expect("stderr is text"),
        )
    }

    // ── The three sweeps that keep `--help` true ────────────────────────────────────────────────
    //
    // Each catches a different one of the three ways one flag can go missing, and that is the whole
    // design: a single table feeding both the parser and the help text would lose a flag from both
    // in one edit and report nothing.

    /// **Delete a line from [`HELP`] and this is what goes red.**
    #[test]
    fn every_flag_this_build_takes_is_in_the_help_text() {
        let help = help();
        let missing: Vec<&&str> = FLAGS.iter().filter(|f| !help.contains(**f)).collect();
        assert!(
            missing.is_empty(),
            "{missing:?} are flags the parser accepts and `--help` does not mention. A flag \
             nobody can find is a flag nobody has"
        );
        assert!(FLAGS.len() >= 7, "the table holds {}", FLAGS.len());
    }

    /// **Delete an entry from [`FLAGS`] and this is what goes red** — the help text still names it,
    /// and the sweep asks the table whether it is real.
    ///
    /// This is the direction that matters most, because it is the defect the old `--help` shipped:
    /// a listed flag the program does not honour is indistinguishable, to the person reading, from
    /// one it does.
    #[test]
    fn every_flag_the_help_text_names_is_one_this_build_takes() {
        let help = help();
        let found = flag_tokens(&help);
        assert!(
            found.len() >= 7,
            "the sweep read {} flag-shaped words out of `--help`, which is fewer than the table \
             holds — it is reading nothing rather than agreeing",
            found.len()
        );
        let strays: Vec<&String> = found.iter().filter(|t| !FLAGS.contains(&t.as_str())).collect();
        assert!(
            strays.is_empty(),
            "`--help` lists {strays:?}, and the parser does not take them"
        );
    }

    /// **Delete an arm from [`parse`] and this is what goes red.**
    ///
    /// The table and the help text can both still name a flag whose handler is gone; only asking
    /// the parser catches that. `--flash=` and `--disk=` are operands rather than verbs, so they
    /// are asked together with the verb that reads them.
    #[test]
    fn every_flag_in_the_table_is_one_the_parser_answers() {
        let mut refused: Vec<String> = Vec::new();
        for flag in FLAGS {
            let line = match *flag {
                "--make-app" => "--make-app somewhere".to_string(),
                "--flash=" => "--check-images --flash=a.bin".to_string(),
                "--disk=" => "--check-images --disk=a.img".to_string(),
                f if f.ends_with('=') => format!("{f}x"),
                f => f.to_string(),
            };
            match parse(&argv(&line)) {
                Cli::Refused(why) => refused.push(format!("`{line}` -> {why}")),
                Cli::Window => refused.push(format!("`{line}` -> opened a window")),
                _ => {}
            }
        }
        assert!(
            refused.is_empty(),
            "{refused:?}: the flag is in the table and in the help, and the parser has no arm for it"
        );
    }

    // ── What each flag parses to ────────────────────────────────────────────────────────────────

    #[test]
    fn no_arguments_opens_the_window() {
        assert_eq!(parse(&[]), Cli::Window);
    }

    #[test]
    fn help_is_spelled_both_ways_and_wins_over_the_rest_of_the_line() {
        assert_eq!(parse(&argv("-h")), Cli::Help);
        assert_eq!(parse(&argv("--help")), Cli::Help);
        // Somebody who has got the line wrong is exactly who is asking for the help.
        assert_eq!(parse(&argv("--nonsense --help")), Cli::Help);
        assert_eq!(parse(&argv("--check-images -h")), Cli::Help);
    }

    #[test]
    fn the_two_no_window_reports_parse_to_themselves() {
        assert_eq!(parse(&argv("--check-update")), Cli::CheckUpdate);
        assert_eq!(
            parse(&argv("--check-images --flash=a.bin --disk=b.img")),
            Cli::CheckImages {
                flash: Some("a.bin".into()),
                disk: Some("b.img".into()),
            }
        );
        // Order is not part of the grammar, and neither operand is required.
        assert_eq!(
            parse(&argv("--disk=b.img --check-images")),
            Cli::CheckImages {
                flash: None,
                disk: Some("b.img".into()),
            }
        );
    }

    #[test]
    fn the_bundler_takes_a_directory_and_an_optional_icon() {
        assert_eq!(
            parse(&argv("--make-app dist")),
            Cli::MakeApp {
                out: "dist".into(),
                icon: None
            }
        );
        assert_eq!(
            parse(&argv("--make-app dist icon.png")),
            Cli::MakeApp {
                out: "dist".into(),
                icon: Some("icon.png".into())
            }
        );
        // A flag after it is a flag, not an icon — otherwise `--make-app dist --check-update`
        // writes a bundle and swallows the second verb without saying so.
        let both = parse(&argv("--make-app dist --check-update"));
        assert!(
            matches!(&both, Cli::Refused(w) if w.contains("--make-app")),
            "{both:?} should refuse two verbs"
        );
    }

    #[test]
    fn the_bundler_refuses_rather_than_choosing_a_directory_for_you() {
        let r = parse(&argv("--make-app"));
        assert!(
            matches!(&r, Cli::Refused(w) if w.contains("--make-app OUT")),
            "{r:?} should name the signature"
        );
    }

    // ── And what it refuses ─────────────────────────────────────────────────────────────────────

    /// **The defect this whole file closes.** Every one of the thirty-nine flags the old window took
    /// must be answered rather than ignored, and — the half that actually matters — none of them may
    /// reach [`Cli::Window`], because a window opening is how an ignored flag looks like a working
    /// one.
    #[test]
    fn every_flag_the_old_window_took_is_answered_rather_than_ignored() {
        assert!(RETIRED.len() >= 39, "the table holds {}", RETIRED.len());
        let mut wrong: Vec<String> = Vec::new();
        for (flag, _) in RETIRED {
            let line = if flag.ends_with('=') {
                format!("{flag}7")
            } else {
                (*flag).to_string()
            };
            match parse(&argv(&line)) {
                Cli::Refused(why) => {
                    if !why.contains(flag) {
                        wrong.push(format!("`{line}` refused without naming itself: {why}"));
                    }
                    if !why.contains("--help") {
                        wrong.push(format!("`{line}` refused without saying where to look: {why}"));
                    }
                }
                other => wrong.push(format!("`{line}` -> {other:?}")),
            }
        }
        assert!(wrong.is_empty(), "{wrong:?}");
    }

    /// The two tables may not overlap. A flag in both would be honoured **and** declared dead, and
    /// which of the two a person saw would depend on the order of two branches in [`refusal`].
    ///
    /// Compared by **head**, not by spelling: `--flash` and `--flash=` are one flag typed two ways,
    /// and a table holding one of each would collide through the spelling branch rather than
    /// through the exact match, which is the harder of the two to see.
    #[test]
    fn a_flag_is_either_live_or_retired_and_never_both() {
        let head = |f: &str| f.trim_end_matches('=').to_string();
        let live: Vec<String> = FLAGS.iter().map(|f| head(f)).collect();
        let both: Vec<&str> = RETIRED
            .iter()
            .map(|(f, _)| *f)
            .filter(|f| live.contains(&head(f)))
            .collect();
        assert!(both.is_empty(), "{both:?} are in both tables");
    }

    /// A live flag typed the other way round is a **spelling**, and is answered as one.
    ///
    /// `--flash FILE` is one keystroke from `--flash=FILE`, and answering it with *not a flag of
    /// this build* is a wrong answer that sends somebody to look for a flag they already have.
    #[test]
    fn a_live_flag_spelled_the_other_way_is_corrected_rather_than_denied() {
        let r = parse(&argv("--check-images --flash rom.bin"));
        assert!(
            matches!(&r, Cli::Refused(w) if w.contains("--flash=FILE")),
            "{r:?} should name the spelling that works"
        );
        let r = parse(&argv("--make-app=dist"));
        assert!(
            matches!(&r, Cli::Refused(w) if w.contains("separate words")),
            "{r:?} should say the operands are not attached"
        );
        // And a retired flag typed without its `=` is still answered as retired, not as a typo of
        // something live.
        let r = parse(&argv("--headless"));
        assert!(
            matches!(&r, Cli::Refused(w) if w.contains("starts no machine")),
            "{r:?}"
        );
    }

    /// A retired flag is answered with the reason it is retired, and the two reasons are not
    /// interchangeable: one of them stops being true the day a machine starts.
    #[test]
    fn a_retired_flag_says_which_half_of_the_program_it_belonged_to() {
        let machine = parse(&argv("--headless=200"));
        assert!(
            matches!(&machine, Cli::Refused(w) if w.contains("starts no machine")),
            "{machine:?}"
        );
        let window = parse(&argv("--user"));
        assert!(
            matches!(&window, Cli::Refused(w) if w.contains("window that was replaced")),
            "{window:?}"
        );
    }

    #[test]
    fn a_word_this_build_has_never_had_is_named_rather_than_swallowed() {
        let r = parse(&argv("--nonsense"));
        assert!(matches!(&r, Cli::Refused(w) if w.contains("--nonsense")), "{r:?}");
        // A bare word is not a flag and not a file this build opens.
        let r = parse(&argv("rom.bin"));
        assert!(
            matches!(&r, Cli::Refused(w) if w.contains("rom.bin") && w.contains("--check-images")),
            "{r:?}"
        );
    }

    /// **The operands cannot be given on their own, and they cannot be given to somebody else.**
    /// `--flash=` used to configure a machine at launch; here it only says what to check, so it
    /// needs the thing that checks — and beside `--check-update` or `--make-app` it would be read
    /// by nothing, which is the shape this whole file exists to stop.
    #[test]
    fn the_operands_refuse_without_the_verb_that_reads_them() {
        for line in ["--flash=a.bin", "--disk=b.img", "--flash=a.bin --disk=b.img"] {
            let r = parse(&argv(line));
            assert!(
                matches!(&r, Cli::Refused(w) if w.contains("Add --check-images")),
                "`{line}` -> {r:?}"
            );
        }
        for (line, verb) in [
            ("--check-update --flash=a.bin", "--check-update"),
            ("--make-app dist --disk=b.img", "--make-app"),
        ] {
            let r = parse(&argv(line));
            assert!(
                matches!(&r, Cli::Refused(w) if w.contains(verb) && w.contains("--check-images")),
                "`{line}` -> {r:?}: the operand would have been read by nothing"
            );
        }
    }

    // ── Exit codes and what reaches which writer ────────────────────────────────────────────────

    /// Help goes to **stdout** and exits 0, which is what `--help > /dev/null` in CI depends on,
    /// and it says the version, which is the one thing about this build a bundle can disagree with.
    #[test]
    fn help_is_an_answer_on_stdout_and_not_an_error() {
        let (code, out, err) = ran(&Cli::Help);
        assert_eq!(code, 0);
        assert!(err.is_empty(), "{err:?} reached stderr");
        assert!(out.contains(update::VERSION), "the help does not say what build this is");
        assert!(out.contains("--check-update"), "{out}");
        assert!(out.lines().count() > 10, "the help is {} lines", out.lines().count());
    }

    /// A refusal goes to **stderr** and exits 2. Both halves matter: a message on stdout is one a
    /// pipeline reads as the answer, and an exit code of 0 is one `set -e` does not see.
    #[test]
    fn a_refusal_is_a_complaint_on_stderr_with_a_non_zero_code() {
        let (code, out, err) = ran(&parse(&argv("--headless=200")));
        assert_eq!(code, 2);
        assert!(out.is_empty(), "{out:?} reached stdout");
        assert!(err.contains("--headless="), "{err:?}");
    }

    /// `--check-images` with nothing to check names what is missing rather than reporting a verdict
    /// about a file nobody gave it.
    ///
    /// **It claims the data directory first**, and that is the same rule `main.rs`'s own sweep
    /// enforces on its side of the crate: this is the one path in this file that falls back to
    /// `Settings::load`, which without the redirect resolves to the operator's real library —
    /// `AGENTS.md` §3 — and would also make the assertion depend on whose machine it ran on.
    #[test]
    fn nothing_to_check_is_a_refusal_and_not_a_verdict() {
        let _data = crate::data_dir_lock();
        let (code, out, err) = ran(&Cli::CheckImages {
            flash: None,
            disk: Some("b.img".into()),
        });
        assert_eq!(code, 2, "exit {code}: {out}{err}");
        assert!(err.contains("no NOR dump"), "{err:?}");
        assert!(out.is_empty(), "{out:?}");
    }

    /// A pair given in full is answered with a verdict and a non-zero code, and — the part CI reads
    /// — the word `UNREADABLE` for a file that is not there.
    #[test]
    fn a_pair_that_does_not_exist_is_unreadable_rather_than_a_crash() {
        let dir = std::env::temp_dir();
        let (code, _, err) = ran(&Cli::CheckImages {
            flash: Some(dir.join("no-such-ipod-rom.bin")),
            disk: Some(dir.join("no-such-ipod-drive.img")),
        });
        assert_eq!(code, 1, "a missing pair is a failure, not a pass");
        assert!(err.is_empty(), "{err:?}: the verdict is not a complaint");
    }

    /// The tokeniser the two help sweeps stand on, proved to read and proved to refuse — otherwise
    /// `every_flag_the_help_text_names_is_one_this_build_takes` is a sweep that finds nothing and
    /// reports agreement.
    #[test]
    fn the_help_tokeniser_reads_flags_and_only_flags() {
        assert_eq!(
            flag_tokens("  --flash=FILE  the NOR dump, or `--disk=X`."),
            ["--flash=", "--disk="]
        );
        assert_eq!(flag_tokens("-h, --help"), ["-h", "--help"]);
        // Hyphens inside a word are not flags, and neither is an em dash.
        assert!(flag_tokens("a double-clickable app — signed by nobody").is_empty());
    }
}
