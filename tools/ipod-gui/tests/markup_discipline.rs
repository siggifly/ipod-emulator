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
    // **`mono: "…"`, not `mono:`.** §11.4's `DetailRow` carries a `mono` **flag** — is this line a
    // path or a hash? — and the fixture sets it on every detail row it draws. This test is about
    // §9.3's `RailRow.mono`, which is the command itself, so it matches the string form. Counting
    // both made a Parts fixture look like three failures with three remedies.
    let mono: Vec<&String> = lines.iter().filter(|l| l.starts_with("mono: \"")).collect();
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

// ═══ Phase 6 — the Composer, the Devices page, Parts and Settings ═══════════════════════════════
//
// docs/GUI.md §11 in full, plus §7.2. Everything below sweeps **markup**: what these four pages
// draw, what they refuse to word, and what the fixture that is the only way to look at them covers.
//
// `src/geometry.rs` owns the geometry sweeps and the fit arithmetic; this file owns the rules that
// are about the shape of the markup rather than about its numbers.

/// The four pages this phase adds, plus the two files they changed.
const PHASE_SIX: &[&str] = &[
    "composer.slint",
    "devices.slint",
    "parts.slint",
    "settings.slint",
];

/// Every `"…"` literal on one line, with the escapes left as written.
fn strings(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for c in line.chars() {
        if in_string {
            if escaped {
                cur.push(c);
                escaped = false;
            } else if c == '\\' {
                cur.push(c);
                escaped = true;
            } else if c == '"' {
                out.push(std::mem::take(&mut cur));
                in_string = false;
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_string = true;
        }
    }
    out
}

/// **§11.3's verdict region has four renderings and its `Fix` has three shapes, and the fixture is
/// the only place any of them is drawn.**
///
/// Nothing in this build composes a recipe, so `ui/preview.slint` is where a person — or a reviewer
/// — looks at the state before anything is chosen, at the state that is still reading a volume, at
/// a one-press fix, at the two-press `BuildFromIpsw` with its consequence, and at a fix that is
/// itself disabled because it names a value the picker four rows above refuses.
///
/// A rendering with no fixture entry is a state that renders in no window anybody can open, which is
/// exactly how §12.3's *cancelled* Rail row would have been drawn wrong for as long as it took
/// somebody to cancel something.
#[test]
fn the_preview_fixture_covers_every_composer_verdict_state() {
    let preview = code(&ui("preview.slint")).join("\n");
    for case in ["nothing", "reading", "ok", "one-press", "two-press", "disabled-fix"] {
        assert!(
            preview.contains(&format!("name: \"{case}\"")),
            "ui/preview.slint's Composer fixture draws no `{case}` verdict state, so §11.3's \
             rendering for it is in no window anybody can open"
        );
    }

    // …and the two that are about the `Fix` rather than about the verdict carry the thing that
    // makes them different from a plain refusal.
    let two = preview
        .split("name: \"two-press\"")
        .nth(1)
        .expect("the two-press case")
        .split("name: \"")
        .next()
        .unwrap();
    assert!(
        two.contains("presses: 2,"),
        "the two-press fixture case does not carry `presses: 2`; §11.3's whole point about \
         `BuildFromIpsw` is the second press"
    );
    assert!(
        two.contains("consequence:"),
        "the two-press fixture case names what it detaches nowhere — a one-press `Fix` detached a \
         55.9 GB reference with no sentence at all, and that is the finding this case exists for"
    );

    let disabled = preview
        .split("name: \"disabled-fix\"")
        .nth(1)
        .expect("the disabled-fix case");
    assert!(
        disabled.contains("enabled: false,"),
        "the disabled-fix fixture case is not disabled, so §11.3's fourth rule — a `Fix` naming a \
         value the picker refuses is disabled too — is drawn nowhere"
    );
    assert!(
        disabled.contains("escape-hatch:"),
        "the disabled-fix fixture case names no escape hatch; §9.4's rule for a project state is to \
         say what does work and always name a command that does"
    );
}

/// **§11.4: six groups, fixed order, always all six present even when empty.**
///
/// Two of them are the ones the shipped window dropped entirely — `Bootloaders`, which is
/// `Resource::Bootloader` and reached no pixel at all, and `Snapshots`, whose 1.6 GB per park was
/// invisible. A fixture with four groups would look exactly like the window this replaces.
#[test]
fn the_preview_fixture_draws_all_six_parts_groups() {
    let preview = code(&ui("preview.slint")).join("\n");
    for (n, heading) in [
        (0, "iPods"),
        (1, "Apple firmware"),
        (2, "Bootloaders"),
        (3, "Software"),
        (4, "Disks"),
        (5, "Snapshots"),
    ] {
        assert!(
            preview.contains(&format!("group: {n},")) && preview.contains(&format!("heading: \"{heading}\"")),
            "ui/preview.slint's Parts fixture is missing group {n} ({heading}); §11.4 requires all \
             six present even when empty"
        );
    }
    // §9.1: never a bare "nothing here" — every group says what belongs in it.
    // A line that **starts** `empty:`, not one that contains it: `work-empty:` is the Work page's
    // §9.1 sentence and is not a Parts group.
    let empties = code(&ui("preview.slint"))
        .iter()
        .filter(|l| l.starts_with("empty: \""))
        .count();
    assert_eq!(
        empties, 6,
        "ui/preview.slint's Parts fixture carries {empties} empty-group sentences and §9.1 wants \
         one per group"
    );
    // …and one expanded ROM row, which is where §11.4's whole Expand — the verdict, the identity
    // masked, the machine rules and the boot-screen preview — is actually rendered.
    assert!(
        preview.contains("kind: 1,") && preview.contains("parts-detail-of: 2;"),
        "ui/preview.slint draws no expanded `Kind::Rom` row, so §11.4's Expand is in no window \
         anybody can open"
    );
}

/// **§16.5, mechanically: `enabled:` never appears on a `TouchArea` or a `FocusScope`, anywhere.**
///
/// Both traps compile and neither works. A disabled `TouchArea` forcibly clears `has_hover`
/// (`i-slint-core-1.17.1/items/input_items.rs:80-91`), so a disabled control cannot show its reason
/// on hover; a disabled `FocusScope` refuses focus *"not even programmatically"*
/// (`builtins.slint:903`), so it cannot show it on focus either. §9.4's whole rule — *visible,
/// `fg-disabled`, non-interactive, carrying its reason on focus as well as hover* — is unbuildable
/// the obvious way, and this is what stops somebody rebuilding it the obvious way.
#[test]
fn no_touch_area_or_focus_scope_is_ever_disabled() {
    for name in [
        "bench.slint",
        "composer.slint",
        "devices.slint",
        "drawer.slint",
        "ipod.slint",
        "parts.slint",
        "preview.slint",
        "primitives.slint",
        "rail.slint",
        "settings.slint",
        "tokens.slint",
        "window.slint",
        "work.slint",
    ] {
        let lines = code(&ui(name));
        let mut inside: Option<&'static str> = None;
        let mut depth = 0i32;
        for (n, line) in lines.iter().enumerate() {
            if inside.is_some() {
                depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
                if line.starts_with("enabled:") {
                    panic!(
                        "ui/{name}:{}: `{}` inside a {} — §16.5: a disabled TouchArea clears \
                         has_hover and a disabled FocusScope refuses focus even programmatically, \
                         so the control cannot state its reason at all. Gate the ACTION on a plain \
                         boolean instead.",
                        n + 1,
                        line,
                        inside.unwrap()
                    );
                }
                if depth <= 0 {
                    inside = None;
                }
                continue;
            }
            for kind in ["TouchArea", "FocusScope"] {
                if line.contains(&format!("{kind} {{")) {
                    inside = Some(if kind == "TouchArea" { "TouchArea" } else { "FocusScope" });
                    depth = line.matches('{').count() as i32 - line.matches('}').count() as i32;
                }
            }
        }
    }
}

/// **The control that makes the sweep above produce a non-zero.**
///
/// `AGENTS.md` §6: before believing a zero, run the control. Three fragments — a legitimate
/// `Row { enabled: false }`, an `accessible-enabled:`, and an offending `TouchArea { enabled:
/// false }` — and it must catch exactly the third.
#[test]
fn the_disabled_sweep_can_see_a_disabled_touch_area() {
    let caught = |src: &str| -> bool {
        let lines = code(src);
        let mut inside = false;
        let mut depth = 0i32;
        for line in &lines {
            if inside {
                depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
                if line.starts_with("enabled:") {
                    return true;
                }
                if depth <= 0 {
                    inside = false;
                }
                continue;
            }
            if line.contains("TouchArea {") || line.contains("FocusScope {") {
                inside = true;
                depth = line.matches('{').count() as i32 - line.matches('}').count() as i32;
            }
        }
        false
    };

    assert!(
        !caught("Row {\n    label: \"Parts\";\n    enabled: false;\n}\n"),
        "the sweep flags a disabled Row, which is §9.4's whole construction and is correct"
    );
    assert!(
        !caught("Rectangle {\n    accessible-enabled: false;\n}\n"),
        "the sweep flags `accessible-enabled`, which is what a disabled control is supposed to \
         announce"
    );
    assert!(
        caught("touch := TouchArea {\n    enabled: root.on;\n    clicked => { }\n}\n"),
        "the sweep does not see a disabled TouchArea, so its zero means nothing"
    );
}

/// **§16.3: every drawer page is hidden by `visible`, never by `if`.**
///
/// A conditional element destroys and rebuilds its subtree, which is what made the shipped
/// `if tab == 0:` throw away carousel state, hover and focus on every tab change. `visible` keeps
/// the subtree alive, so a page you come back to still has its scroll offset and its focus — which
/// is the whole reason §16.8 promises that neither `Esc` nor `⌘\` ever writes `depth`.
#[test]
fn every_drawer_page_is_hidden_by_visible_and_never_by_if() {
    let lines = code(&ui("drawer.slint"));
    let pages = [
        "WorkPage",
        "DevicesPage",
        "PartsPage",
        "SettingsPage",
        "ComposerRoot",
        "WhichIpodPage",
        "WhatItRunsPage",
        "NameItPage",
    ];
    for page in pages {
        let opens = format!("{page} {{");
        let at = lines
            .iter()
            .position(|l| l.contains(&opens))
            .unwrap_or_else(|| panic!("ui/drawer.slint draws no {page}"));
        assert!(
            !lines[at].starts_with("if "),
            "ui/drawer.slint:{}: {page} is behind an `if`, which destroys its subtree on every \
             page change and takes hover, focus and the scroll offset with it (§16.3)",
            at + 1
        );
        let visible = lines[at..]
            .iter()
            .take(8)
            .any(|l| l.starts_with("visible: root.on-screen("));
        assert!(
            visible,
            "ui/drawer.slint:{}: {page} does not gate itself on `on-screen`, so it is in the tab \
             order and in the accessible tree while the drawer is 420 px off the right edge",
            at + 1
        );
    }
}

/// **A menu row does not state a gap that has been closed.**
///
/// §16.9's rule, and §8 decision 22: the Devices, Parts and Settings rows' disabled reasons and
/// escape hatches are **deleted**, not left standing beside the pages that make them false. A stale
/// reason is worse than no reason — it is the program asserting something about itself that stopped
/// being true, which is §1.1's shape exactly.
#[test]
fn no_menu_row_states_a_gap_that_has_been_closed() {
    let lines = code(&ui("drawer.slint"));
    for label in ["Devices", "Parts", "Settings"] {
        let at = lines
            .iter()
            .position(|l| l == &format!("label: \"{label}\";"))
            .unwrap_or_else(|| panic!("ui/drawer.slint's MenuPage has no {label} row"));
        let row = &lines[at..(at + 8).min(lines.len())];
        for stale in ["reason:", "escape-hatch:", "enabled: false"] {
            assert!(
                !row.iter().any(|l| l.starts_with(stale)),
                "ui/drawer.slint:{}: the {label} menu row still carries `{stale}`, and the page it \
                 says does not exist is drawn one slot along",
                at + 1
            );
        }
        assert!(
            row.iter().any(|l| l.starts_with("activated =>")),
            "ui/drawer.slint:{}: the {label} menu row goes nowhere, which is the live-but-inert \
             control §19.1 calls fatal",
            at + 1
        );
    }
}

/// **The verdict is never computed in a binding, and the window computes no compatibility rule of
/// its own.**
///
/// A binding is re-evaluated on the toolkit's schedule; the one region the design forbids to be
/// stale is written on a schedule the program controls — `Composer::recompute`, which rewrites the
/// region, the plan and the cost **together** before control returns to the event loop, so no frame
/// can render a recipe with another recipe's verdict.
///
/// The second half is the one that catches a well-meaning shortcut: a partition type compared in
/// markup is `compose.rs`'s rule 2 written twice, and the second copy is where the `Start::FromDisk`
/// variant gets forgotten — which is exactly what happened to it in the model.
#[test]
fn the_verdict_is_never_computed_in_a_binding() {
    let text = code(&ui("composer.slint")).join("\n");
    for banned in [
        ".check(",
        ".check_parts(",
        ".steps(",
        ".cost(",
        ".describe(",
        ".best_loader(",
        "0x0b",
        "0x0c",
        "0x0B",
        "0x0C",
        "nothing chosen",
        "Verdict",
    ] {
        assert!(
            !text.contains(banned),
            "ui/composer.slint names `{banned}`, so the window is deciding something \
             `compose.rs` already decides — and two copies of one rule is how the third `Start` \
             variant comes to be forgotten"
        );
    }

    // The control: the matcher has to flag a planted line, or a sweep that sees nothing is
    // indistinguishable from a file that contains nothing.
    let planted = "region-text: root.recipe.check() == Verdict.ok ? \"\" : \"no\";";
    assert!(
        [".check(", "Verdict"].iter().any(|b| planted.contains(b)),
        "the matcher cannot see a verdict computed in a binding, so its silence means nothing"
    );
}

/// **The pages state no policy and word no sentence.**
///
/// Every string these four files draw arrives on a pushed model, built in `main.rs` out of
/// `composer.rs` / `parts.rs` / `devices.rs` / `settings_page.rs`, which are built out of the model
/// in `eapp-loader`. What is left is each surface's own furniture — its page name, its back label,
/// its section captions and its two or three verbs — and that list is written here rather than
/// inferred, so a sentence smuggled into markup fails rather than reads.
#[test]
fn every_composer_sentence_comes_from_the_model_or_composer_rs() {
    // Page names, back labels, section captions, verbs, and the one joining separator. Nothing in
    // this list is a claim about a recipe, a file, a device or a refusal.
    const FURNITURE: &[&str] = &[
        "",
        " ",
        ", ",
        "Devices",
        "Which iPod",
        "What it runs",
        "Name it",
        "Will do, in order",
        "Systems",
        "Menu",
        "Parts",
        "Remove",
        "preview",
        "Settings",
        "Theme",
        "Check for updates on launch",
        "Settings file",
        "Start",
    ];
    for name in PHASE_SIX {
        let mut importing = false;
        for (n, line) in code(&ui(name)).iter().enumerate() {
            // An import path is a file name, not a sentence. Multi-line import lists end on the
            // `} from "…";` line, so the flag spans them rather than the first line alone.
            if line.starts_with("import ") {
                importing = !line.ends_with(';');
                continue;
            }
            if importing {
                importing = !line.ends_with(';');
                continue;
            }
            for s in strings(line) {
                // A literal that is nothing but an interpolation — `"\{g.count}"` in the markup — is the
                // model's own value rendered as text. It states nothing and words nothing; it is
                // Slint's only way to put an `int` in a `Text`.
                if s.starts_with("\\{") && s.ends_with('}') {
                    continue;
                }
                assert!(
                    FURNITURE.contains(&s.as_str()),
                    "ui/{name}:{}: the markup words `{s}`. Every sentence these pages draw is the \
                     model's — if this is furniture rather than a sentence, add it to FURNITURE \
                     with the reason\n  {line}",
                    n + 1
                );
            }
        }
    }
}

/// **One `Fix` control per refusal, and it is the only one.**
///
/// §11.3 item 3: *at most one `Fix` row*, wearing a 60 %-opacity material. Two controls under one
/// refusal is a page with two primary actions, which is a page with none — and the opacity is a
/// design constant, so it comes from `Geometry` and never from a typed 0.6.
#[test]
fn there_is_one_fix_control_per_refusal() {
    let text = code(&ui("composer.slint")).join("\n");
    let view = text
        .split("component RefusalView")
        .nth(1)
        .expect("ui/composer.slint declares RefusalView")
        .split("\ncomponent ")
        .next()
        .unwrap()
        .split("\nexport component ")
        .next()
        .unwrap();
    let controls = view.matches("Pressable {").count();
    assert_eq!(
        controls, 1,
        "RefusalView draws {controls} pressable controls; §11.3 allows exactly one `Fix` under a \
         refusal"
    );
    assert!(
        view.contains("material-opacity: Geometry.fix-opacity;"),
        "the `Fix` does not wear §11.3's 60 %-opacity material from `Geometry` — a typed 0.6 is a \
         second source of truth and `no_opacity_literal_outside_the_cosmetic_set` refuses it"
    );
    assert!(
        view.contains("presses: root.r.fix.presses;") && view.contains("consequence: root.r.fix.consequence;"),
        "the `Fix` does not take its press count and its consequence from the model, so §11.3's \
         two-press rule would have to be decided twice"
    );
}

/// **The verdict region is reserved in all four renderings.**
///
/// 54 px, always, whichever of the four the model answers — nothing chosen, still reading,
/// `Verdict::Ok` and `Verdict::No`. A region that sized itself to its content would move `Create`
/// every time you ticked a box, which is principle 2 violated by the one control the page exists
/// for.
#[test]
fn the_verdict_region_is_reserved_in_all_four_renderings() {
    let lines = code(&ui("composer.slint"));
    let at = lines
        .iter()
        .position(|l| l == "height: Geometry.verdict-h;")
        .expect("ui/composer.slint reserves Geometry.verdict-h for the verdict region");
    let region = &lines[at..(at + 16).min(lines.len())];
    assert!(
        region.iter().any(|l| l.starts_with("clip: true")),
        "the verdict region does not clip, so a four-line refusal would draw over the plan under it"
    );
    assert!(
        region.iter().any(|l| l.contains("text: root.region-text;")),
        "the verdict region draws something other than the one string `composer::Region` answers \
         with, which is how it came to assert a plan for a firmware nobody had chosen"
    );
    assert!(
        region.iter().any(|l| l.contains("root.region-emphatic")),
        "the verdict region does not distinguish `Verdict::No` from the three `fg-dim` renderings, \
         so a refusal reads as a description"
    );
    // …and there is exactly one of it. A second reserved region is a second answer.
    assert_eq!(
        lines.iter().filter(|l| *l == "height: Geometry.verdict-h;").count(),
        1,
        "ui/composer.slint reserves the verdict region more than once"
    );
}

/// **The drawn glass and §11.4's boot-screen preview are one colour.**
///
/// §12.2 gives a powered-off panel `#08080a` and §11.4 draws the preview with *the same glass
/// treatment*. Two literals is how they come to be two colours; one token with two readers cannot
/// drift.
#[test]
fn the_preview_and_the_drawn_glass_are_one_colour() {
    let tokens = code(&ui("tokens.slint")).join("\n");
    assert!(
        tokens.contains("out property <color> glass: #08080a;"),
        "ui/tokens.slint does not declare the glass, so the two surfaces that draw it have two \
         literals"
    );
    for (name, what) in [("ipod.slint", "the drawn device's panel"), ("parts.slint", "§11.4's preview")] {
        let text = code(&ui(name)).join("\n");
        assert!(
            text.contains("Ink.glass"),
            "ui/{name} does not read `Ink.glass`, so {what} is a second copy of one colour"
        );
        assert!(
            !text.contains("#08080a"),
            "ui/{name} still types `#08080a` beside the token that declares it"
        );
    }
}

/// **Every `Scroll` in this phase binds its own `viewport-height`.**
///
/// Slint computes it automatically only from direct children that are layout elements **and are not
/// repeated** (`i-slint-compiler-1.17.1/passes/flickable.rs:181-196`), and every one of these page
/// bodies contains a `for`. Left unset the page reports a viewport of zero, declines to scroll, and
/// positions its tail outside the Flickable and draws it there. It looks fine and it is wrong —
/// which is §16.2's finding in the one place §16.11 exists to close.
#[test]
fn every_new_page_binds_its_own_viewport_height() {
    for name in PHASE_SIX {
        let lines = code(&ui(name));
        let scrolls = lines.iter().filter(|l| l.contains("Scroll {")).count();
        assert!(scrolls > 0, "ui/{name} has no Scroll, so this sweep is reading nothing");
        let bound = lines.iter().filter(|l| l.starts_with("viewport-height:")).count();
        assert_eq!(
            bound, scrolls,
            "ui/{name} declares {scrolls} Scrolls and binds {bound} viewport heights; an unbound \
             one reports zero and silently declines to scroll"
        );
        for l in lines.iter().filter(|l| l.starts_with("viewport-height:")) {
            assert!(
                l.contains(".preferred-height"),
                "ui/{name}: `{l}` — a viewport height that is not the body's own preferred height \
                 is a second measurement of the same layout"
            );
        }
    }
}

/// **The three pages the menu just made live are drawn in the slot their `Page::slot()` names, and
/// the Composer's four are too.**
///
/// `Stack::go` refuses any page whose `slot()` does not equal the resulting depth, and lands on the
/// menu instead — because a page drawn at a depth nothing draws it at is a blank 420 px panel with
/// no header and therefore no visible way out. That guard is only worth having if the markup and the
/// slot agree, and this is where they are compared.
#[test]
fn the_new_pages_are_drawn_at_the_depth_their_slot_names() {
    let lines = code(&ui("drawer.slint"));
    let slot_of = |page: &str| -> usize {
        let at = lines
            .iter()
            .position(|l| l.contains(&format!("{page} {{")))
            .unwrap_or_else(|| panic!("ui/drawer.slint draws no {page}"));
        let gate = lines[at..]
            .iter()
            .take(8)
            .find(|l| l.starts_with("visible: root.on-screen("))
            .unwrap_or_else(|| panic!("{page} is not gated on `on-screen`"));
        gate.trim_start_matches("visible: root.on-screen(")
            .split(')')
            .next()
            .unwrap()
            .parse()
            .expect("the slot index")
    };

    // The strip is blank, depth 0, depth 1, depth 2, depth 3, blank — so slot `n` draws depth
    // `n - 1`.
    for (page, depth) in [
        ("WorkPage", 1),
        ("DevicesPage", 1),
        ("PartsPage", 1),
        ("SettingsPage", 1),
        ("ComposerRoot", 2),
        ("WhichIpodPage", 3),
        ("WhatItRunsPage", 3),
        ("NameItPage", 3),
    ] {
        assert_eq!(
            slot_of(page) - 1,
            depth,
            "ui/drawer.slint draws {page} at depth {}, and `nav::Page::slot()` says {depth}",
            slot_of(page) - 1
        );
    }
}

/// **A `Pressable`'s height is three named terms and no literal.**
///
/// 44 plain, 60 with a fact line, 78 disabled or two-press or carrying a consequence, 94 both —
/// and every one of those four numbers is the sum of constants `src/geometry.rs` owns. It is the
/// arithmetic every fit test in this phase rests on, so a typed 16 here would be a second source of
/// truth for the row height inside the primitive that defines it.
#[test]
fn a_row_with_a_fact_line_costs_one_named_line_box() {
    let text = code(&ui("primitives.slint")).join("\n");
    let height = text
        .split("height: Geometry.row-h\n")
        .nth(1)
        .expect("ui/primitives.slint's Pressable sizes itself from Geometry.row-h")
        .split(';')
        .next()
        .unwrap();
    assert!(
        height.contains("root.sub == \"\" ? 0px : Geometry.line-label"),
        "a `Pressable` with a fact line does not cost exactly one `LINE_LABEL`: {height:?}"
    );
    assert!(
        height.contains("root.tells ? Geometry.field-reason : 0px"),
        "a `Pressable` that has something to tell you does not reserve §5's 34 px slot: {height:?}"
    );

    // And the reason slot is one component rather than two copies of one, so `Field` and
    // `Pressable` cannot reserve two different 34 px.
    assert_eq!(
        text.matches("ReasonSlot {").count(),
        2,
        "ui/primitives.slint instantiates ReasonSlot somewhere other than exactly `Pressable` and \
         `Field`, which are the two things §11.1's *same heights* claim is about"
    );
}

/// The `Field` variants `src/composer.rs` lists in `Field::ALL`, **in order**.
///
/// Read out of the declaration rather than typed here, because the `int` that crosses into the
/// markup *is* a position in that array — `Field::from_i32` indexes it and `Field::as_i32` searches
/// it. A list typed in this file would be a second copy of the ordinals, and a second copy is what
/// the defect below already looked like from inside the markup.
fn field_ordinals() -> Vec<String> {
    let p = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src/composer.rs"));
    let text = std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let body = text
        .split_once("pub const ALL:")
        .expect("src/composer.rs declares `Field::ALL`")
        .1
        .split_once("= [")
        .expect("the array's opening bracket")
        .1
        .split_once("];")
        .expect("the array's closing bracket")
        .0;
    body.split(',')
        .map(|v| v.trim().trim_start_matches("Field::").to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

/// **`Copy the command line` does not mint a new iPod.**
///
/// `Field` crosses the boundary as an `int` so the vocabulary stays in Rust (`src/composer.rs`'s own
/// words), which means the markup carries a bare ordinal and nothing in the compiler checks which
/// control it names. This file is the only thing that can.
///
/// It shipped as `root.act(0)`. Zero is `Field::Ipod`, the first variant of `Field::ALL`, and
/// `main::wire`'s `on_composer_act` answers `Field::Ipod` by calling `Composer::make_one` — so the
/// button labelled *Copy the command line* **minted a new iPod and discarded the seed on screen**,
/// and the `Field::Serial` arm that builds and copies the command was reachable from nothing.
///
/// A wrong ordinal is silent in every other instrument in this tree: it type-checks, it compiles, it
/// draws, and `Field::from_i32` turns an out-of-range one into a no-op rather than a panic. The only
/// place the two halves can be compared is here, against the array the ordinal indexes.
#[test]
fn the_copy_command_control_does_not_mint_a_new_ipod() {
    let fields = field_ordinals();
    // The control: a reader that found nothing would wave any ordinal through, and the two
    // assertions below would both be comparing against `None.position()` — so pin the shape of what
    // was read before trusting a position in it.
    assert!(
        fields.len() >= 10,
        "only {} `Field` variants were read out of `Field::ALL`: {fields:?}",
        fields.len()
    );
    let mint = fields
        .iter()
        .position(|v| v == "Ipod")
        .expect("`Field::ALL` lists `Ipod`");
    let copy = fields
        .iter()
        .position(|v| v == "Serial")
        .expect("`Field::ALL` lists `Serial`");
    assert_eq!(
        mint, 0,
        "`Field::Ipod` is no longer the ordinal a forgotten `act(0)` would send"
    );

    let lines = code(&ui("composer.slint"));
    let at = lines
        .iter()
        .position(|l| l == "label: root.copy-command.label;")
        .expect("ui/composer.slint draws a Pressable off `root.copy-command`");
    // Bounded to the control: `activated` is a handful of lines below the label, and an unbounded
    // search would happily read the *next* control's ordinal if this one had lost its handler.
    let sent = lines[at..]
        .iter()
        .take(20)
        .find_map(|l| l.split_once("root.act("))
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(n, _)| n.trim().to_string())
        .expect("the copy-command Pressable's `activated` handler calls `root.act(…)`");

    assert_ne!(
        sent,
        mint.to_string(),
        "ui/composer.slint's `Copy the command line` sends {sent}, which is `Field::Ipod` — the \
         button mints a new iPod and throws away the seed it was showing"
    );
    assert_eq!(
        sent,
        copy.to_string(),
        "ui/composer.slint's `Copy the command line` sends {sent}; `main::wire`'s \
         `on_composer_act` builds and copies the command line under `Field::Serial`, which is \
         ordinal {copy} in `Field::ALL`"
    );
}
