//! The preview root is the only way to look at the Rail, and this is what stops it rotting.
//!
//! `build.rs` hands the slint compiler exactly one root, `ui/window.slint`, so `ui/preview.slint` is
//! compiled by no `cargo` command in this tree — it exists for `slint-viewer`, and the invocation is
//! in its own header. Nothing about it can fail a build.
//!
//! What it carries is a **fixture**: six Rail entries covering every `RailKind`, including a failed
//! one whose two next steps are the disabled-with-a-reason shape this build is actually in
//! (`Caps { file_picker: false, drop_target: false, clipboard: false, … }`). That fixture is the
//! only place any of those states is ever drawn, because nothing in this build composes a recipe.
//!
//! `ui/preview.slint`'s own header claimed this file existed and checked exactly that. **It did not
//! exist**, and in the meantime the fixture had drifted: the entry the header described as *"the
//! second is disabled with a reason and no escape hatch"* carried `next-b-enabled: true`. A comment
//! describing a mechanism that is not there is the next drift, so the claim is made true here rather
//! than deleted.

fn ui(name: &str) -> String {
    let p = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/ui")).join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Strip `//` comments, so the prose above a fixture cannot stand in for the fixture.
fn code(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| {
            let mut out = String::new();
            let mut in_string = false;
            let mut chars = line.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '"' => {
                        in_string = !in_string;
                        out.push(c);
                    }
                    '/' if !in_string && chars.peek() == Some(&'/') => break,
                    _ => out.push(c),
                }
            }
            out.trim().to_string()
        })
        .collect()
}

/// The variants `ui/rail.slint` declares, read out of the declaration rather than typed here.
fn rail_kinds() -> Vec<String> {
    let text = ui("rail.slint");
    let body = text
        .split_once("export enum RailKind {")
        .expect("ui/rail.slint declares RailKind")
        .1
        .split_once('}')
        .expect("the enum's closing brace")
        .0;
    body.split(',')
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

/// **Every `RailKind` is drawn by the preview fixture.**
///
/// A seventh kind added to `rail.slint` with no fixture entry is a state that renders in no window
/// anybody can open, which is how §12.3's *cancelled* row would have been drawn wrong for as long as
/// it took somebody to cancel something.
#[test]
fn the_preview_fixture_covers_every_rail_kind() {
    let kinds = rail_kinds();
    // The control: an enum reader that found nothing would wave every fixture through.
    assert!(
        kinds.len() >= 6,
        "only {} RailKind variants were read out of ui/rail.slint: {kinds:?}",
        kinds.len()
    );

    let preview = code(&ui("preview.slint")).join("\n");
    let missing: Vec<&String> = kinds
        .iter()
        .filter(|k| !preview.contains(&format!("kind: RailKind.{k},")))
        .collect();
    assert!(
        missing.is_empty(),
        "ui/preview.slint's fixture draws no entry of kind {missing:?}, so those states are \
         rendered in no window anybody can open"
    );
}

/// **The fixture carries the case this build is actually in: a next step that is disabled, says
/// why, and points at no mechanism that does not exist.**
///
/// §14.1 refuses hide-don't-disable, so an unavailable step is drawn with its reason. §19.1's first
/// fatal finding was the opposite of that — a control advertising a route the program does not have
/// — so a disabled step is allowed an escape hatch only where the escape hatch is real. `Copy the
/// details` has none: `caps.clipboard` is false because nothing in this dependency graph reaches a
/// pasteboard, and there is no command that would.
#[test]
fn the_preview_fixture_draws_a_disabled_next_step_with_its_reason() {
    let lines = code(&ui("preview.slint"));
    let value = |key: &str| -> Option<String> {
        lines
            .iter()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.trim().trim_end_matches(',').trim().to_string())
    };

    for slot in ["next-a", "next-b"] {
        assert_eq!(
            value(&format!("{slot}-enabled:")).as_deref(),
            Some("false"),
            "ui/preview.slint's `{slot}` is not the disabled case, so the fixture stops covering \
             the Caps this build is in"
        );
        let reason = value(&format!("{slot}-reason:")).unwrap_or_default();
        assert!(
            reason.len() > 2,
            "ui/preview.slint's `{slot}` is disabled and says nothing about why: {reason:?}"
        );
    }

    // …and only the one whose escape hatch is real has one. `IPOD_EMULATOR_DATA` is read by
    // `settings.rs`; there is no command that puts text on a clipboard this build does not have.
    let a = value("next-a-escape:").unwrap_or_default();
    assert!(
        a.contains("IPOD_EMULATOR_DATA"),
        "the one disabled step with a real escape hatch no longer names it: {a:?}"
    );
    assert!(
        value("next-b-escape:").is_none(),
        "`Copy the details` is offering an escape hatch, and there is no command that copies to a \
         clipboard this build does not have — that is §19.1's phantom route in miniature"
    );
}

/// **§9.3's last row is drawn too: a failure with NO next step and a named command instead.**
///
/// `Class::ToolMissing` returns an empty `Vec` from `Class::next` by design — there is no control
/// this program could draw that installs 7-Zip — and what it carries is `Class::mono_remedy()`.
/// `RailRow` had no field for that, so the remedy reached no pixel; `every_failure_class_carries_a_
/// next_step_and_its_own_words` was green throughout because it asked the model directly. The
/// fixture is where that row is actually rendered, so it has to be in it.
#[test]
fn the_preview_fixture_draws_the_failure_that_has_a_command_instead_of_a_control() {
    let lines = code(&ui("preview.slint"));
    let mono: Vec<&String> = lines.iter().filter(|l| l.starts_with("mono:")).collect();
    assert_eq!(
        mono.len(),
        1,
        "ui/preview.slint draws {} entries with a `mono` remedy; §9.3 has exactly one class that \
         carries one, and it is the only class with no next step at all",
        mono.len()
    );
    let remedy = mono[0].trim_start_matches("mono:").trim().trim_end_matches(',');
    assert!(
        remedy.contains("install"),
        "the one command a person could paste is not a command: {remedy:?}"
    );

    // …and that entry offers no next step, because that is the whole shape of the row.
    let after: Vec<&String> = lines
        .iter()
        .skip_while(|l| !l.starts_with("mono:"))
        .take_while(|l| !l.starts_with("},"))
        .collect();
    assert!(
        !after.iter().any(|l| l.starts_with("next-a-label:")),
        "the `ToolMissing` fixture entry carries a next step; §9.3's last row has none, which is \
         why it has a command"
    );
}
