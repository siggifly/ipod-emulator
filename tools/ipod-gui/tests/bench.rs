//! What has to be true about the bench's markup, checked against the markup rather than believed.
//!
//! `src/geometry.rs` carries the sweeps, and they read the whole `ui/` directory. This file guards
//! the things about the bench that **no sweep of literals could ever see**: that the cradle's
//! colours are visible on the one surface a cradle is ever drawn on, that each cradle state binds
//! the role §7.3's table gives it, that the drawn controls are never disabled, that the keyboard and
//! the pointer mean the same thing by a press, and that the wheel's hit test is `wheel.rs`'s and is
//! not re-derived in markup.
//!
//! One test here is a literal sweep, and it survives its own retirement condition because it is
//! **stricter** than the general one rather than a duplicate of it — see its own note.
//!
//! Every test here has been made to fail before being trusted (`AGENTS.md` §6, §7): an instrument
//! that reports an absence it could not have observed has cost this project more time than every
//! real bug in the emulator combined.

use std::collections::BTreeMap;

/// The markup, and the generated constants the markup itself reads. Reading the *generated* file
/// rather than re-typing `src/geometry.rs`'s values is what makes this "the test reads what the
/// markup reads" rather than a second copy waiting to drift.
const GEOMETRY: &str = include_str!(concat!(env!("OUT_DIR"), "/geometry.slint"));

fn ui(name: &str) -> String {
    let p = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/ui")).join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn bench() -> String {
    ui("bench.slint")
}

fn ipod() -> String {
    ui("ipod.slint")
}

/// Strip `//` comments and string literals, so neither can hide or invent a token.
fn strip(line: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => in_string = !in_string,
            '/' if !in_string && chars.peek() == Some(&'/') => break,
            _ if !in_string => out.push(c),
            _ => {}
        }
    }
    out
}

/// Every code line of a file, comments and string bodies removed, trimmed.
fn code(text: &str) -> Vec<String> {
    text.lines().map(|l| strip(l).trim().to_string()).collect()
}

/// The whole statement beginning at the line whose stripped form starts with `head`, joined to its
/// `;`. A binding that wraps across lines is one statement and has to be swept as one — that is the
/// hole a per-line sweep left, and `body-y` below is three lines long.
fn statement(text: &str, head: &str) -> String {
    let lines = code(text);
    let n = lines
        .iter()
        .position(|l| l.starts_with(head))
        .unwrap_or_else(|| panic!("no statement starting `{head}`"));
    let mut stmt = lines[n].clone();
    let mut at = n;
    while !stmt.contains(';') && at + 1 < lines.len() {
        at += 1;
        stmt.push(' ');
        stmt.push_str(&lines[at]);
    }
    stmt
}

// ── Colour ──────────────────────────────────────────────────────────────────────────────────────

/// `out property <color> bg-sunken: #c6cfd6;` → `("bg-sunken", (198, 207, 214))`.
fn palette() -> BTreeMap<String, (u8, u8, u8)> {
    let mut out = BTreeMap::new();
    for line in ui("tokens.slint").lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("out property <color> ") else {
            continue;
        };
        let Some((name, value)) = rest.split_once(": ") else {
            continue;
        };
        let hex = value.trim().trim_end_matches(';').trim_start_matches('#');
        if hex.len() != 6 {
            continue;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("a colour is hex");
        out.insert(name.to_string(), (byte(0), byte(2), byte(4)));
    }
    assert!(
        out.contains_key("bg-sunken"),
        "ui/tokens.slint no longer declares bg-sunken; the well has no colour to be measured against"
    );
    out
}

/// WCAG 2.x relative luminance. Written out rather than pulled in, because a contrast claim in this
/// design is a **measurement** and a measurement whose arithmetic nobody can read is a claim.
fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let channel = |v: u8| {
        let c = f64::from(v) / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn contrast(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

// ── A very small expression evaluator, for the derivations the shelf is built from ──────────────

/// `Metric.title-size + Metric.s3 / 2` → 26.0, with the names resolved out of `ui/tokens.slint` and
/// the generated `geometry.slint`.
///
/// It exists so `the_shelf_rows_and_its_padding_fit_the_declared_shelf` reads the file rather than
/// re-typing what the file is supposed to say — which is the difference between a test and a
/// mirror.
///
/// It is arithmetic over a closed table of declared lengths and nothing else: `+ - * /`, a name
/// that must already be in `names`, or a decimal. There is no interpreter here and no way to reach
/// one — an unknown name panics rather than being looked up anywhere.
///
/// **Operators are whitespace-delimited tokens, and that is not laziness.** A hyphen is an
/// identifier character in Slint, so `Metric.title-size` is one name; splitting on the character
/// gives `Metric.title` and `size` and the arithmetic quietly becomes about something else. It did.
fn eval(expr: &str, names: &BTreeMap<String, f64>) -> f64 {
    let mut total = 0.0;
    let mut sign = 1.0;
    let mut acc: Option<f64> = None;
    let mut pending: Option<char> = None;
    for token in expr.split_whitespace() {
        match token {
            "+" | "-" => {
                total += sign * acc.take().unwrap_or_else(|| panic!("`{expr}`: no operand"));
                sign = if token == "+" { 1.0 } else { -1.0 };
            }
            "*" | "/" => pending = token.chars().next(),
            _ => {
                let v = atom(token, names);
                acc = Some(match (acc, pending.take()) {
                    (None, _) => v,
                    (Some(a), Some('*')) => a * v,
                    (Some(a), Some('/')) => a / v,
                    (Some(_), _) => panic!("`{expr}`: two operands with no operator between them"),
                });
            }
        }
    }
    total + sign * acc.unwrap_or_else(|| panic!("`{expr}`: no operand"))
}

fn atom(text: &str, names: &BTreeMap<String, f64>) -> f64 {
    if let Some(v) = names.get(text) {
        return *v;
    }
    let plain = text.trim_end_matches("px");
    plain
        .parse()
        .unwrap_or_else(|_| panic!("`{text}` is neither a declared length nor a number"))
}

/// Every `<length>` the markup can name, as `Metric.s5` / `Geometry.shelf` → its value in px.
fn lengths() -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    for (file, prefix, text) in [
        ("tokens", "Metric.", ui("tokens.slint")),
        ("geometry", "Geometry.", GEOMETRY.to_string()),
    ] {
        let mut seen = 0;
        for line in text.lines() {
            let t = line.trim();
            let Some(rest) = t
                .strip_prefix("out property <length> ")
                .or_else(|| t.strip_prefix("in property <length> "))
            else {
                continue;
            };
            let Some((name, value)) = rest.split_once(": ") else {
                continue;
            };
            let px = value.trim().trim_end_matches(';').trim_end_matches("px");
            if let Ok(v) = px.parse::<f64>() {
                out.insert(format!("{prefix}{name}"), v);
                seen += 1;
            }
        }
        assert!(seen > 0, "{file} declared no lengths this test could read");
    }
    out
}

// ── The tests ───────────────────────────────────────────────────────────────────────────────────

/// **The bench carries no length literal but a device-pixel hairline.**
///
/// `src/geometry.rs`'s own sweep reads the whole `ui/` directory now, so this is no longer the only
/// thing looking at these two files. What it still is, is **stricter**: that sweep allows `0` and
/// the `/ 2` centring divisor as bare numbers, and this one allows neither — a token with a length
/// unit on it must be `0px`, `1px` — the hairline the drawn device's borders and the shelf's top
/// rule are — or a percentage of the parent, which is what percentages are for.
///
/// *(Its own header used to say the geometry sweep "names `ui/ipod.slint` and `ui/window.slint` in
/// a literal array", which was the retirement condition and is no longer true. A comment describing
/// a mechanism that is not there is the next drift, so it is corrected rather than left standing.)*
///
/// Stricter on purpose. `420px` is on the real sweep's allowlist because it is `IPod`'s
/// `body-height` **default**, and that default lives in `ui/ipod.slint`; nothing else may have one.
#[test]
fn no_length_literal_but_a_hairline_survives_in_the_bench() {
    // `420px` is on the list because it is `IPod`'s `body-height` **default**, which exists so
    // `slint-viewer` and the live preview draw a plausible device with no Rust running; every real
    // use site passes a size.
    const ALLOWED: &[&str] = &["0px", "1px", "100%", "50%", "420px"];
    // Only bindings whose value **is** a position or a size. Anything else is not a length: a
    // gradient stop (`chassis 42%`) is a position along a gradient, a viewbox is a coordinate
    // space, a weight is a weight. `border-width` is on this list and is not on
    // `src/geometry.rs`'s — a `border-width: 2px` on a new focus ring would go straight through the
    // sweep that exists to stop exactly that.
    const LENGTH_PROPS: &[&str] = &[
        "x",
        "y",
        "width",
        "height",
        "min-width",
        "min-height",
        "preferred-width",
        "preferred-height",
        "border-radius",
        "border-width",
        "font-size",
        "letter-spacing",
        "padding",
        "padding-left",
        "padding-right",
        "padding-top",
        "padding-bottom",
        "spacing",
        "body-height",
    ];
    let is_length_line = |t: &str| {
        t.starts_with("property <length>")
            || t.starts_with("in property <length>")
            || LENGTH_PROPS
                .iter()
                .any(|p| t.starts_with(p) && t[p.len()..].starts_with(':'))
    };

    for (name, text) in [("ui/bench.slint", bench()), ("ui/ipod.slint", ipod())] {
        let lines = code(&text);
        for (n, first) in lines.iter().enumerate() {
            if !is_length_line(first) {
                continue;
            }
            // Join to the end of the statement: a binding that wraps puts its literal on a line
            // that does not begin with a length property, and a per-line sweep never looks at it.
            // `body-y` in `ui/bench.slint` is three lines long.
            let mut line = first.clone();
            let mut at = n;
            while !line.contains(';') && !line.contains('{') && at + 1 < lines.len() {
                at += 1;
                line.push(' ');
                line.push_str(&lines[at]);
            }
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                // Consume identifiers whole: `hero-phys-1x` ends in something that reads exactly
                // like a length, and a hyphen is an identifier character in Slint.
                if chars[i].is_alphabetic() || chars[i] == '_' {
                    while i < chars.len()
                        && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-')
                    {
                        i += 1;
                    }
                    continue;
                }
                // …and colours whole, because `#ffffff14` is a colour and not a 14. The `#` is
                // stepped over **before** the loop rather than tested inside it: a `#` is not an
                // alphanumeric, so a shared branch never advances and spins for ever. It did.
                if chars[i] == '#' {
                    i += 1;
                    while i < chars.len() && chars[i].is_alphanumeric() {
                        i += 1;
                    }
                    continue;
                }
                if !chars[i].is_ascii_digit() {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let digits = i;
                while i < chars.len() && (chars[i].is_ascii_alphabetic() || chars[i] == '%') {
                    i += 1;
                }
                let unit: String = chars[digits..i].iter().collect();
                if unit != "px" && unit != "%" {
                    continue; // not a length: a viewbox, a weight, an opacity, a duration
                }
                let token: String = chars[start..i].iter().collect();
                assert!(
                    ALLOWED.contains(&token.as_str()),
                    "{name}:{}: `{token}` is a length literal; it belongs in src/geometry.rs\n  {line}",
                    n + 1
                );
            }
        }
    }
}

/// **The cradle's colours are visible on the one surface a cradle is ever drawn on.**
///
/// This is the finding §6.4 was rewritten around, and it is the reason this test computes rather
/// than trusts: the previous revision measured the accent against `#ffffff` and against `#121212`
/// and **never once against `bg-sunken`**, which is the only surface the cradle has. The inactive
/// ring was `line` at 30 % — **1.23 : 1**, i.e. invisible, across five of the cradle's twelve
/// states, and the one state whose whole job is to teach (`cannot start`) was `fg-disabled` at
/// **1.67 : 1**.
///
/// So the test reads the ring's own colour statement out of the markup, pulls the `Ink` roles out
/// of it, and recomputes every one against the well. Changing which role the cradle binds moves
/// this number; nothing else has to be remembered.
#[test]
fn the_cradle_colours_clear_three_to_one_against_the_well() {
    let ink = palette();
    let well = ink["bg-sunken"];
    let stmt = statement(&bench(), "property <color> ring-ink:");

    assert!(
        !stmt.contains("with-alpha"),
        "the cradle's ring is drawn at partial alpha: {stmt}\n  `line` at 30 % is 1.23 : 1 on the \
         well, which is the defect §6.4 was rewritten around"
    );

    let mut roles: Vec<String> = Vec::new();
    for piece in stmt.split("Ink.").skip(1) {
        let role: String = piece
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-')
            .collect();
        if !roles.contains(&role) {
            roles.push(role);
        }
    }
    assert_eq!(
        roles.len(),
        3,
        "§6.4: the cradle uses three colours and one shape, not {}: {roles:?}",
        roles.len()
    );

    for role in &roles {
        let c = *ink
            .get(role)
            .unwrap_or_else(|| panic!("the cradle binds Ink.{role}, which tokens.slint does not have"));
        let ratio = contrast(c, well);
        assert!(
            ratio >= 3.0,
            "the cradle's `{role}` is {ratio:.2} : 1 on the well (#{:02x}{:02x}{:02x}); \
             3 : 1 is the floor a 2 px outline has to clear",
            well.0,
            well.1,
            well.2
        );
    }

    // §6.4's one exception to *the focus ring is `accent`*: on the cradle it is `fg`, because a
    // 2 px accent ring 4 px outside a 2 px accent state ring is a ring around a ring in one colour.
    assert!(
        bench().contains("border-color: Ink.fg;"),
        "the cradle's focus ring is no longer `fg`; §6.4's exception is the whole reason it is not \
         `accent` there"
    );
    let ratio = contrast(ink["fg"], well);
    assert!(ratio >= 3.0, "the cradle's focus ring is {ratio:.2} : 1 on the well");
}

/// **No drawn control is ever disabled** — §16.5, §9.4.
///
/// A `TouchArea` with `enabled: false` forcibly clears `has_hover` and forwards the event to
/// whatever is underneath; a `FocusScope` with `enabled: false` refuses focus *even
/// programmatically*. So a control that carries its own reason is unbuildable that way, and the
/// construction is an always-enabled outer pair wrapping an action gated by a plain boolean.
///
/// `ui/ipod.slint` walked into this: `centre-touch` was `enabled: pressable`, and the same flag was
/// read back through `centre-touch.has-hover` — a hover highlight on a control that had just been
/// told it can never be hovered.
#[test]
fn no_drawn_control_is_ever_disabled() {
    // **Every markup file, not the two the bench owns.** It was scoped to `bench.slint` and
    // `ipod.slint` because those were the only two a builder owned at the time, and §16.5 is not a
    // rule about drawn controls: `TouchArea { enabled: false }` forcibly sets `has_hover = false`
    // and returns `ForwardAndIgnore` (`i-slint-core-1.17.1/items/input_items.rs:80-91`), and
    // `FocusScope { enabled: false }` refuses focus **even programmatically** (`:643-645`) — so a
    // `Pressable` gated that way is a control nobody without a mouse can reach, and it would say
    // nothing about why. That is T-17's structural half, and it belongs over the whole tree.
    //
    // `focus-on-tab-navigation:` and `focus-on-click:` are the *supported* way to take an inert
    // line out of the tab order, and `Pressable` uses those; neither starts with `enabled:`.
    // **Scoped to the two element types the rule is about**, because `enabled:` on our own
    // `Pressable` is the correct way to say *disabled, and here is why* — §9.4's whole second kind
    // is built on it, and the drawer's six unfinished rows each set it. A sweep that could not tell
    // the two apart fired on `ui/drawer.slint:72`, which is a `Row` doing exactly what it should.
    let mut swept = 0;
    let mut seen_a_touch_area = false;
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/ui")).expect("ui/") {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_none_or(|x| x != "slint") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        swept += 1;

        // A stack of open blocks: each frame says whether it is a TouchArea or a FocusScope.
        let lines = code(&ui(&name));
        let mut stack: Vec<bool> = Vec::new();
        for (n, line) in lines.iter().enumerate() {
            if line.ends_with('{') {
                let head = line.trim_end_matches('{').trim();
                let head = head.rsplit(":=").next().unwrap_or(head).trim();
                let kind = head.split_whitespace().next_back().unwrap_or("");
                let input = kind == "TouchArea" || kind == "FocusScope";
                seen_a_touch_area |= input;
                stack.push(input);
                continue;
            }
            if line.starts_with('}') {
                stack.pop();
                continue;
            }
            assert!(
                !(line.starts_with("enabled:") && *stack.last().unwrap_or(&false)),
                "ui/{name}:{}: `{line}` — §16.5: never `enabled:` on a TouchArea or a FocusScope. \
                 `TouchArea {{ enabled: false }}` forcibly sets `has_hover = false` and \
                 `FocusScope {{ enabled: false }}` refuses focus even programmatically, so the \
                 control cannot be reached without a mouse and cannot say why. Gate the action on a \
                 plain boolean inside the handler instead",
                n + 1
            );
        }
    }
    // Two controls, because a sweep that read no files and a sweep that recognised no elements both
    // look exactly like a sweep that found nothing.
    assert!(swept >= 9, "only {swept} markup files were swept; there are more than that");
    assert!(
        seen_a_touch_area,
        "the scanner recognised no TouchArea or FocusScope in any markup file, so it was never in a \
         position to catch one"
    );
}

/// **The wheel's hit test is `wheel.rs`'s, and there is not a second one in the markup.**
///
/// §7.4 is explicit that this had two rules that disagreed with each other and with the model. The
/// ring's radii are `outer × 0.52` and `outer × 0.465` and they live in `wheel::WheelRing::new`;
/// the drawn centre button is `CENTRE_D × h`, which is **39 % narrower** than the region `hit()`
/// already treats as Select. So the markup reports *where* the pointer is and never decides *what*
/// it means, and the one size it does need — the centre button's target — is pushed in.
#[test]
fn the_wheel_hit_test_is_wheel_rs_and_is_not_re_derived_in_the_markup() {
    for (name, text) in [("ui/bench.slint", bench()), ("ui/ipod.slint", ipod())] {
        for ratio in ["0.52", "0.465"] {
            for (n, line) in code(&text).iter().enumerate() {
                assert!(
                    !line.contains(ratio),
                    "{name}:{}: `{ratio}` is one of `WheelRing::new`'s radii and it is now written \
                     in markup too — that is the second rule §7.4 records as having disagreed with \
                     the model\n  {line}",
                    n + 1
                );
            }
        }
    }
    assert!(
        code(&ipod())
            .iter()
            .any(|l| l.starts_with("in property <length> select-d:")),
        "the centre button's target is no longer pushed in; the only other way to size it is to \
         write `WheelRing`'s ratio in the markup"
    );
    for want in ["callback wheel-down(length, length);", "callback wheel-up();"] {
        assert!(
            code(&ipod()).iter().any(|l| l == want),
            "ui/ipod.slint no longer declares `{want}`, so the ring has no way to report a \
             position to the one thing that knows what one means"
        );
    }
}

/// **No string the bench draws carries a glyph the font is not proven to have** — §6.7, §16.6.
///
/// Slint takes one `font-family` per element with no fallback list, runtime font registration is
/// behind an unstable feature, and **nothing in `.slint` can ask whether a glyph exists**. A
/// missing one falls to `.notdef`. Twelve of those shipped to the operator as empty squares, which
/// is why §6.7's icon set is closed and drawn — and why `MENU ›  Devices · Parts · …` is a
/// `HorizontalLayout` of words, one drawn chevron and four drawn dots rather than one string.
///
/// **Do not widen this to make it pass.** That is exactly how the twelve shipped.
#[test]
fn no_string_the_bench_draws_carries_a_glyph_the_font_is_not_proven_to_have() {
    for (name, text) in [("ui/bench.slint", bench()), ("ui/ipod.slint", ipod())] {
        for (n, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            // Everything the stripper removed *is* the string bodies, so take the difference.
            let mut in_string = false;
            let mut chars = line.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '"' => in_string = !in_string,
                    '/' if !in_string && chars.peek() == Some(&'/') => break,
                    _ if in_string => assert!(
                        c.is_ascii_graphic() || c == ' ',
                        "{name}:{}: the string literal on this line carries `{c}` (U+{:04X}), \
                         which nothing has proved this font has. §6.7: draw it.\n  {}",
                        n + 1,
                        c as u32,
                        line.trim()
                    ),
                    _ => {}
                }
            }
        }
    }
}

/// **U-1, measured rather than argued.**
///
/// §7.5 declares an 88 px shelf with 12 px of padding top and bottom and rows of 26, 20 and 16 —
/// which sums to **86**, or 87 with the 1 px top rule. The design's own decomposition does not sum
/// to its own total, and `SHELF` is load-bearing: `CHROME_MIN` (154) and `CHROME_PREF` (190) are
/// both built on it.
///
/// So this measures the leftover from the markup's own derivations rather than asserting it away,
/// and it fails in **both** directions — an overflow, which would draw a row past the bottom edge,
/// and a leftover big enough to be a missing term rather than a rounding, which is the shape a
/// `SHELF_SPARE` invented to absorb it would have.
#[test]
fn the_shelf_rows_and_its_padding_fit_the_declared_shelf() {
    let text = bench();
    let names = lengths();

    // **All three line boxes now come from `src/geometry.rs`.** They were derived locally here —
    // `Metric.title-size + Metric.s3 / 2` — while that file had a different owner, with a stated
    // retirement condition of *when `geometry.rs` declares `LINE_TITLE` / `LINE_BODY`*. It does, so
    // the rows are read out of the generated file the markup itself reads.
    let row = |what: &str, constant: &str| {
        assert!(
            code(&text).iter().any(|l| *l == format!("height: {constant};")),
            "the shelf's {what} row is no longer the line box `{constant}`"
        );
        names[constant]
    };

    let row1 = row("first", "Geometry.line-title");
    let row2 = row("second", "Geometry.line-body");
    let row3 = row("third", "Geometry.line-label");

    let pad = |head: &str| {
        let stmt = statement(&text, head);
        eval(stmt.split_once(':').unwrap().1.trim().trim_end_matches(';'), &names)
    };
    let padding = pad("padding-top: Metric.") + pad("padding-bottom: Metric.");

    let shelf = names["Geometry.shelf"];
    let used = padding + row1 + row2 + row3;
    let spare = shelf - used;

    assert!(
        spare >= 0.0,
        "the shelf's own parts need {used:.0} px and it declares {shelf:.0}: \
         padding {padding:.0} + rows {row1:.0} / {row2:.0} / {row3:.0}. \
         A row is being drawn past the bottom edge of the window."
    );
    assert!(
        spare < names["Metric.s1"],
        "{spare:.0} px of the {shelf:.0} px shelf is unaccounted for — padding {padding:.0} + \
         rows {row1:.0} / {row2:.0} / {row3:.0} = {used:.0}. That is a missing term rather than a \
         rounding, and U-1 says do NOT invent a spare one to absorb it: the decomposition and the \
         total have to be made to agree in src/geometry.rs."
    );
}

/// **The device is pinned by its distance above the shelf, not centred in the well** — §7.2.
///
/// The consequence is the reason for the choice: growing the window moves nothing, shrinking it
/// eats the top margin first, and anything that pushes from the top costs the well its air rather
/// than the device its place. A `y` of `(parent.height - self.height) / 2` — which is what the
/// shipped window does, and what anybody tidying this would reach for — throws all three away and
/// nothing on screen says so.
///
/// The terms are §9.6's column read bottom-up, and they are the terms `CHROME_PREF` is summed from,
/// so a change to the column that missed this file would be caught here.
#[test]
fn the_device_is_pinned_by_its_distance_above_the_shelf() {
    let stmt = statement(&bench(), "property <length> body-y:");
    for term in [
        "well.height",
        "Geometry.gap-2",
        "Geometry.cradle-label",
        "Geometry.gap-1",
        "Geometry.cradle-band",
        "root.hero",
    ] {
        assert!(
            stmt.contains(term),
            "the device's y no longer subtracts `{term}` from the well's own height, so it is not \
             pinned to the shelf any more:\n  {stmt}"
        );
    }
    assert!(
        !stmt.contains("/ 2"),
        "the device is centred in the well rather than pinned above the shelf:\n  {stmt}"
    );
}

/// **Nothing in the bench reads the window's own height** — §16.1.
///
/// The window's height comes from its layout, the layout from its content, the content from
/// `hero`. Reading it back closes the loop, and Slint reports the inherited case as a
/// *deprecation warning* saying it may panic at run time — the weaker signal this upgrades. The
/// horizontal axis has no such loop, which is why the drawer is allowed to push the well narrower.
#[test]
fn nothing_in_the_bench_reads_the_windows_own_height() {
    for (name, text) in [("ui/bench.slint", bench()), ("ui/ipod.slint", ipod())] {
        for (n, line) in code(&text).iter().enumerate() {
            assert!(
                !line.contains("root.height"),
                "{name}:{}: `{line}` — the window's height is an output of the layout the bench \
                 is in, and reading it here closes the loop",
                n + 1
            );
        }
    }
}

// `the_bench_is_not_yet_reachable_from_the_compiled_root` **was here, went red on schedule, and has
// been deleted — which is what closing that gap looks like.**
//
// It asserted that `ui/window.slint` did not contain the string `bench.slint`, because `build.rs`
// hands the slint compiler exactly one root and a `.slint` file that root does not import is
// compiled by no `cargo` command in this tree: a syntax error in it, a binding loop in it, a
// property that does not exist — none of them could fail a build. It failed with *"ui/window.slint
// imports the bench now, so build.rs compiles it and a syntax error in it can fail a build at last.
// Delete this test."* the first time the composed window was built.
//
// The guarantee it was standing in for is now the compiler's: `ui/window.slint` imports `Bench`, so
// `cargo build -p ipod-gui` compiles `ui/bench.slint` and `ui/ipod.slint` with it. The eight tests
// above are not literal sweeps of a file nothing compiles; they are checks on what it says, and they
// stay.

// ── Two things the colour test could not see ────────────────────────────────────────────────────

/// **Each cradle state binds the role §7.3's table gives it, and `danger` means one thing.**
///
/// `the_cradle_colours_clear_three_to_one_against_the_well` measures whether the roles the cradle
/// binds are *visible*. It says nothing about which state binds which — so a refusal painted
/// `danger` reads as a perfectly legible 4.14 : 1 and is the wrong colour anyway.
///
/// §6.4 states the rule in one sentence: *`accent` when startable, `fg-dim` otherwise, `danger` when
/// stopped, and a broken ring … when the device cannot start.* §7.3's table gives all three
/// `cannot start` rows `fg-dim` + a broken ring, and its only `danger` row is
/// `stopped — Lost(0xe19b0000)`. The first cut of `ui/window.slint` bound `danger` to *not
/// startable*, which spends the one colour that means *a machine died* on *a file moved* — and
/// leaves nothing to tell them apart the day §12.2's `Stopped` lands.
#[test]
fn the_cradle_ring_means_what_the_table_says_it_means() {
    let stmt = statement(&ui("window.slint"), "cradle-ring:");
    assert!(
        stmt.contains("CradleRing.accent") && stmt.contains("CradleRing.dim"),
        "the cradle no longer distinguishes startable from not:\n  {stmt}"
    );
    assert!(
        !stmt.contains("CradleRing.danger"),
        "the cradle is painted `danger` for something other than a stopped machine. §6.4 gives \
         a refusal `fg-dim` plus a BROKEN RING, and `danger` to §12.2's `Stopped` alone — a phase \
         this build cannot be in.\n  {stmt}"
    );

    // …and the shape that carries the refusal is still there, or the state has no tell at all.
    let broken = statement(&ui("window.slint"), "cradle-broken:");
    assert!(
        broken.contains("startable"),
        "the broken ring is no longer bound to startability, so a refused device is drawn exactly \
         like a startable one:\n  {broken}"
    );

    // The enum is closed at three, which is what stops a fourth colour arriving by accident.
    let decl = statement(&bench(), "export enum CradleRing");
    assert_eq!(
        decl.matches(',').count(),
        2,
        "`CradleRing` is no longer three values: {decl}. §6.4: three colours and one shape."
    );
}

/// **The keyboard and the pointer mean the same thing by a press.**
///
/// §7.4 keeps the drawn centre button live at all times — *a control that goes dead is a control
/// that teaches nothing* — and Rust is the only thing that can say which part of a device is gone.
/// So neither route may gate on `startable`: the pointer's did not and the cradle's did, which made
/// `Return` a silent no-op on the one device §20 item 12 exists for while a click on the same device
/// filed the refusal. On an empty bench it was starker still: a click produced *"there are no
/// devices in the library yet"* and `Return` produced nothing at all.
///
/// The file's own comment says the keyboard route exists *"so the two cannot disagree about what
/// pressing means"*, which is the sentence this makes true.
#[test]
fn the_keyboard_and_the_pointer_agree_about_what_pressing_means() {
    let text = bench();
    let lines = code(&text);

    // The cradle's key handler, from `key-pressed` to its closing brace.
    let from = lines
        .iter()
        .position(|l| l.starts_with("cradle-focus := FocusScope"))
        .expect("ui/bench.slint declares the cradle's FocusScope");
    let handler: String = lines[from..from + 40].join("\n");
    assert!(
        handler.contains("root.pressed-centre();"),
        "the cradle's keyboard route no longer reaches `pressed-centre`, so Return does nothing \
         on the Button the whole program is built around:\n{handler}"
    );
    assert!(
        !handler.contains("if (root.startable) { root.pressed-centre(); }"),
        "the cradle's keyboard route is gated on `startable` and the drawn centre button's is not, \
         so Return is a dead key on exactly the device §20 item 12 exists for:\n{handler}"
    );

    // The pointer route, in the drawing, is ungated too — and it is the control.
    let ip = code(&ipod());
    let centre = ip
        .iter()
        .position(|l| l.starts_with("centre-touch := TouchArea"))
        .expect("ui/ipod.slint declares the centre button's target");
    let touch: String = ip[centre..centre + 16].join("\n");
    assert!(
        touch.contains("root.pressed-device();"),
        "the drawn centre button no longer reports a press:\n{touch}"
    );
    assert!(
        !touch.contains("root.startable"),
        "the drawn centre button has grown a `startable` gate; the refusal is Rust's to make, and \
         it is the only thing that can name which part is gone:\n{touch}"
    );
}
