// Devices — docs/GUI.md §7.2, §7.5, §9.1, §11.2.
//
// **The producer for the one page whose rows could not open.** `ui/devices.slint` declares
// `devices-detail` and `devices-detail-of`; nothing set either, and `detail-of` defaults to `-1`,
// so `Expand.open: root.detail-of == i` was false for every row for ever. The five `Made of` lines
// were undrawn — and so was the `Start` button, which lives **inside** that Expand
// (`devices.slint:266`). A page whose only control was unreachable is what this file ends.
//
// **What this file does not produce.** `refresh_devices` in `main.rs` already pushes `devices`,
// `devices-empty-line` and `devices-new`, and every field of `DeviceRow` with them — the row's
// name, its summary and its state. So this file writes what is inside the open row and nothing
// else. A second writer for a property one function already pushes is two producers for one
// pixel, which is the defect the whole of this wave exists to stop. The one thing inside that
// row which `DeviceRow` also carries is `Start`, and note 3 below is why that is not an exception
// to this paragraph but the sharpest instance of it.
//
// **The cursor is the device's NAME and never its index.** `devices.slint:223` fires
// `expand(i, ...)` with the repeater's index and `detail-of` is compared against that same index,
// so the number crossing the boundary is an index in both directions — but a device inserted or
// removed above the open one moves every index below it, and a cursor that held one would then be
// showing somebody else's identity. So the index is resolved to a name on the way in and back to
// an index on the way out, which is one lookup per push and the only shape that survives the list
// changing. `Composer::device_vanished` learned the same lesson about a name held across a run
// that replaced it.
//
// **And `open_row` is the only place in this file that reads one.** The draft this grew out of
// took an index in `row_action` and in `editor` as well, and that is where an index costs more
// than a body drawn about the wrong device: every act is rendered *inside* the open row —
// `devices.slint:248` hands a `MadeOfLine` the detail only where `detail-of == i` — so the number
// a press carries is always the open device's, computed at the last push. Resolve it again
// against a list that gained a device in between and `Remove` forgets the row above the one that
// was pressed, silently, having been handed a valid index for a device nobody asked about. The
// name is already held and it is the answer, so the acts read it and take no index at all.
//
// **This page pins no ordinal.** `devices.slint:254` fires `root.row-action(a, n)` where `a` is
// `root.d.action` — a number Rust put on the line — and nothing else. The one ordinal that is
// pinned anywhere, `RowAction::Remove == 2`, is pinned by `ui/parts.slint`'s own `Remove` control
// and is written down in `parts.rs` beside the enum.
//
// ─── Three things measured while writing this ────────────────────────────────────────────────────
//
// **1. The two files drew one struct in two colours, and that is repaired.** `devices.slint` drew
// every act in `Ink.danger` and `parts.slint` drew the same struct — one `DetailRow`, one
// `to_detail`, one flattener — in `Ink.accent`, and the disagreement only became visible when this
// page gained a line that is not a removal: `Edit…`, in the destructive colour. The repair is the
// one this note asked for: the colour is a fact about the **act**, so
// [`crate::parts::RowAction::destructive`] answers it, `to_detail` carries the answer across as
// `DetailRow.destructive`, and both files bind it. `Remove` is the only true one — §12.4 parks a
// machine rather than discarding it, so `PowerOff` destroys nothing.
//
// **The second half of the same disagreement went with it.** `devices.slint` had **no** paragraph
// branch: a `Detail` with an empty label — which is what [`device_rule`] produces, and §9.4's
// machine rule is the only line on this page that has one — fell into the two-column fact
// rendering and was drawn as a value, indented past a blank label column at `label-size`.
// `parts.slint` had split the two since it was written. One struct, one rendering, on both pages.
//
// **2. `Settings::rename_device`, `set_boot_shape` and `restate_firmware` could not get a
// production caller here — and all three have one now, in `Composer::commit`, which is where this
// note said they belonged.** It is kept because the reasoning is what put them there:
//
//   - a rename needs typed text, and `devices.slint`'s only route into a row is
//     `row-action(int, int)`, which carries no string. §11.2's level ③ is where a device is named,
//     so `commit` is the rename — and it calls `rename_device` rather than assigning `d.name`, so
//     the refusals are asked, `current` moves with the device, and `Commit.renamed` reports
//     nothing when the device went out from under the page instead of a rename that did not
//     happen. **There is no `Rename` control anywhere in this window**: `RowAction::Rename` has a
//     label and two *exhaustive* arms that refuse it, and the two contradict each other —
//     `parts.rs:678` calls it *a device's control, not a part's* and `row_action` below calls it
//     *not one of a device's controls*. Nothing builds the row on either page.
//   - `Device::boot_shape` is written by `commit` now, through `set_boot_shape`, which owns the
//     same-shape-keep-the-number rule whole rather than half of it. The consequence lands **on
//     this page**: `Settings::recipe_of` treats `boot_shape` as the authority on what a device
//     boots, so the Edit route below opens a device on what it was composed as instead of
//     re-deriving it from the drive's install list. A device composed as Rockbox-only whose drive
//     records no install used to re-open as Apple-by-default.
//     `the_edit_route_carries_the_recipe_the_model_resolves` measures both arms.
//   - `commit` **restates** an edited iPod through `restate_firmware`, which replaces the entry and
//     repoints every device naming it, rather than minting a second resource with `file_away` and
//     repointing one. The operator settled that shared iPod edits restate rather than fork; what
//     this page owes that decision is the sentence before the press, and `Edit…` wears it — `N
//     devices are made of this iPod`, counted with `Settings::devices_using_resource`, two presses
//     to arm.
//
// **3. §7.2's `Start` rule has no producer anywhere, and it cannot be given one in `DeviceRow`.**
// *"`Start` — disabled while a machine exists, with the machine-rule reason `My 5.5G is
// running.`"* The button reads `d.startable` and `d.cradle-label`, both built by
// `device_rows`, which is handed no machine and asks no such question — so today a page opened
// beside a running ARM7 draws a live `Start` on every other device in the library.
//
// The obvious repair is to teach `device_rows` the machine, and it is wrong: **those two fields
// are the bench's cradle as well.** `window.slint:486` reads `root.current.cradle-label` and
// `window.slint:515` reads `root.current.startable`, so the sentence that refuses this page's
// `Start` would be printed under the drawn iPod — the machine's own cradle telling the operator
// that the machine is running and to stop it first. One field, two surfaces, and only one of them
// is asking §7.2's question.
//
// So this page answers it for itself, in [`start_row`], and it is not a second writer for one
// pixel: `DeviceRow.cradle_label` stays §7.3's cradle caption and this is §7.2's refusal. Nothing
// is re-worded — `crate::cradle_label` is called for every arm that is not the machine rule, so
// the shelf and this page cannot say two things about one device. **The bindings are now this
// row's**, counted off the markup rather than off the two fields this note opened with:
// `devices.slint:267` `label`, `:268` `enabled`, `:269` `reason`, `:270` `escape-hatch`, `:276`
// `machine-rule` — which was a literal `true`, and is wrong for the composed-and-unbuilt arm —
// `:277` `presses`, `:278` `consequence` and `:279` `primary`, which read `d.startable` a second
// time. `main.rs`'s `devices-start` carries one row, for the one open device, because a closed
// `Expand` is `visible: false` and no other row's control is on screen.
// And `on_start_device` asks the machine rule again on the press, for the reason `row_action` asks
// it twice below.

use eapp_loader::settings::{self, Device, Presence, Settings};

use crate::composer::{Composer, FixRow};
use crate::parts::{Detail, RowAction, Wrote};
use crate::rail::{Caps, Next};

/// What the page draws that `refresh_devices` does not already push.
///
/// `devices-empty-line` and `devices-new` stay where they are: a second writer for a property one
/// function already pushes is how two producers come to disagree about one page.
///
/// **`start` is an `Option` because the control it describes is drawn in exactly one place** —
/// inside the open row's `Expand` — so with nothing open there is no `Start` on screen and no
/// honest `FixRow` to describe it with. A default one would be a disabled control with an empty
/// reason, which is the shape `primitives.slint` forbids and the shape this file's own sweep
/// looks for; `None` says *there is no such control right now* instead of lying quietly. The
/// flattener writes `unwrap_or_default()` into a struct property that nothing is drawing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub detail: Vec<Detail>,
    /// The index of the open device, or `-1`, which is the markup's own default.
    pub detail_of: i32,
    /// §7.2's `Start`, for the open device. See [`start_row`].
    pub start: Option<FixRow>,
}

/// The Devices page's whole state: which device is expanded, by name.
///
/// Not an `Option<Devices>`: the page exists from startup. `open` is the `Option` — there is
/// genuinely no device expanded most of the time.
///
/// **A cursor, not a copy.** Everything drawn is recomputed from `Settings` on every push, which
/// is what stops it going stale — the same discipline `Composer` holds one `Recipe` under.
pub struct Devices {
    open: Option<String>,
}

impl Devices {
    pub fn new() -> Devices {
        Devices { open: None }
    }

    /// The open device's body, recomputed.
    ///
    /// **`seen` is the pass's shared stat cache and the only filesystem work this does.**
    /// `Settings::missing_with` stats every path a device resolves to and `device_rows` has
    /// already made one `Presence` for the pass it shares across every row; handing it in here
    /// means the open device's two paths are read out of that cache rather than statted a second
    /// time. `Settings::missing` — which mints its own — is the call that would undo it.
    ///
    /// **There is no `busy`.** Nothing on this page is gated on a build: `Edit…` opens a page,
    /// `Remove` touches no file, and `Composer::lock` already answers `Lock::Building` for every
    /// picker inside the Composer this route opens. A gate wired to a question this page does not
    /// ask must not pretend to fire, which is `settings_page.rs`'s rule for the same absence.
    ///
    /// `machine` is the device the emulator is running, by name, or `None`. An argument rather
    /// than a question this file asks, for the reason `Composer::lock` states about `building`:
    /// `main::phase` answers `Off` unconditionally today, so today this is always `None` and the
    /// machine rule below is drawn by this file's own tests and by nothing else.
    pub fn view(
        &mut self,
        s: &Settings,
        seen: &mut Presence,
        caps: Caps,
        machine: Option<&str>,
    ) -> View {
        let at = self
            .open
            .as_deref()
            .and_then(|n| s.devices.iter().position(|d| d.name == n));
        // A cursor that no longer names a device closes itself. Without this, forgetting the open
        // device leaves `detail-of` pointing at an index the list has re-used, and the Expand
        // reopens under whoever moved up into it.
        let Some(i) = at else {
            self.open = None;
            return View {
                detail: Vec::new(),
                detail_of: -1,
                start: None,
            };
        };
        let d = &s.devices[i];
        View {
            detail: made_of(s, d, seen, caps, machine),
            detail_of: i32::try_from(i).unwrap_or(-1),
            start: Some(start_row(s, d, seen, machine)),
        }
    }

    /// Open or close one device's body. `device-expand(i, on)` is the callback this answers.
    ///
    /// **`i` is an index and what is kept is a name.** That conversion is the whole of this
    /// function, and `s` is an argument because it is what performs it.
    ///
    /// It is not called `expand`, for the reason `parts.rs` records: `nav::Stack::expand` already
    /// is, and `no_dead_code_allow_sits_on_a_function_the_program_already_calls` decides a call by
    /// text, so a second `expand(` in this crate makes that sweep report `nav.rs`'s as reconnected
    /// when it is not.
    ///
    /// An index nothing answers to closes whatever was open rather than opening nothing, which is
    /// the same no-op an unknown ordinal gets.
    pub fn open_row(&mut self, s: &Settings, index: i32, open: bool) {
        self.open = if open { name_at(s, index) } else { None };
    }

    /// One control inside one device's body.
    ///
    /// **A refusal mutates nothing and answers `Nothing`**, so `main.rs` does not rewrite the
    /// settings file — which `Settings::render` regenerates whole, taking any comment the operator
    /// added with it.
    ///
    /// `RowAction::Edit` is the one act here that does not touch the library at all, and it is
    /// deliberately a refusal rather than a silent success: [`Devices::editor`] is its route, and
    /// a handler that forwarded it here instead would be a live control that does nothing, which
    /// is §19.1's first fatal finding.
    pub fn row_action(
        &mut self,
        s: &mut Settings,
        a: RowAction,
        machine: Option<&str>,
    ) -> Result<Wrote, String> {
        // **The open device is the subject, and the press carries no index.** See the header: the
        // acts are drawn inside the open row, so the index `row-action(int, int)` sends is the one
        // this file put in `detail_of` at the last push — and resolving it a second time against a
        // list that has moved is how `Remove` comes to forget the neighbour of the row somebody
        // pressed.
        //
        // A cursor naming nothing is a no-op, in the same way an unknown ordinal is: the row went
        // away between the push and the press. The existence check is not decoration — `forget`
        // on a name the library no longer holds writes nothing, and answering `Library` for it
        // would have `main.rs` regenerate the settings file over a press that changed nothing.
        let Some(name) = self
            .open
            .clone()
            .filter(|n| s.devices.iter().any(|d| d.name == *n))
        else {
            return Ok(Wrote::Nothing);
        };
        match a {
            RowAction::Remove => {
                // Asked again here rather than trusted from the control, because the control was
                // drawn at the last push and the machine may have started since.
                if let Some(m) = machine.filter(|m| *m == name) {
                    return Err(running_rule(m));
                }
                s.forget(&name);
                if self.open.as_deref() == Some(name.as_str()) {
                    self.open = None;
                }
                Ok(Wrote::Library)
            }
            RowAction::Edit => Err(format!(
                "{} opens the Composer rather than changing the library, so arriving here means \
                 the press is not wired — a defect in the window rather than in the library",
                a.name()
            )),
            // §7.2's `Start` is `start-device(i)` — the same callback the bench's own centre
            // button presses, which is what `devices.slint:16` means by REUSED, not duplicated —
            // so it does not arrive here and this arm is exhaustiveness rather than a route.
            RowAction::Start => {
                Err("Start is the bench's own control and does not act on the library".into())
            }
            RowAction::Reveal
            | RowAction::CopyPath
            | RowAction::PowerOff
            | RowAction::Rename
            | RowAction::ShowBootScreen
            | RowAction::ShowIdentity => {
                Err(format!("{} is not one of a device's controls", a.name()))
            }
        }
    }

    /// §11.2's *existing and new look identical*, given a surface at last.
    ///
    /// **`Composer::editing` is the only constructor that opens the Composer on a device that
    /// exists, and until this it had no caller outside its own file** — so the Mode::Editing title
    /// `push_composer` already draws was drawn by nothing, and every entrance the window had
    /// constructed `Composer::new()`.
    ///
    /// The recipe is `Settings::recipe_of`'s and is not assembled here: which disk a name resolves
    /// to and what the device recorded about what it boots are the model's rules, and a second
    /// copy of them in the window is the drift that file exists to prevent.
    ///
    /// It is separate from [`Devices::row_action`] because it changes no library and returns
    /// something that is not a `Wrote`: the caller drops it into the cell `push_composer` reads
    /// and pushes `Page::Composer`. `None` when nothing is open, or when the open cursor names a
    /// device the library no longer holds — the same subject, and the same reading of it, as
    /// every other act on this page.
    pub fn editor(&self, s: &Settings) -> Option<Composer> {
        let name = self.open.as_deref()?;
        let d = s.devices.iter().find(|d| d.name == name)?;
        Composer::editing(s, &d.name, s.recipe_of(d))
    }
}

// ─── The body ───────────────────────────────────────────────────────────────────────────────────

/// §7.2's `Made of` lines, its machine rules, and the two acts.
///
/// **It must not call `nor::Source::describe`.** That interpolates `id.serial` into its sentence,
/// and this page is drawn beside a device somebody may be screenshotting; `devices.slint:38` names
/// the function by name for that reason. `settings::suggest_ipod_name` is the answer — the colour
/// and the generation, which is what a person calls an iPod — and it reads no identity at all.
///
/// The five facts are always all five, whatever the device is missing, because a body that grows
/// and shrinks its own rows is a page that jumps under the finger while it is being read (§16.3).
/// What is absent says so in its own value.
fn made_of(
    s: &Settings,
    d: &Device,
    seen: &mut Presence,
    caps: Caps,
    machine: Option<&str>,
) -> Vec<Detail> {
    let mut out: Vec<Detail> = Vec::new();

    // **Which iPod, in the words a person would use, plus the name it is filed under.** The two
    // are usually the same string — `Composer::commit` files a ROM under `suggest_ipod_name` of it
    // — and saying it twice would read as two facts, so the second half appears only when the
    // library knows it by something else.
    let ipod = match s.nor_of(d) {
        Some(src) => {
            let called = settings::suggest_ipod_name(src);
            if called == d.firmware {
                called
            } else {
                format!("{called}, filed as {}", d.firmware)
            }
        }
        // What it *names*, which is still what it is made of; the rule below says the library no
        // longer holds it. Inventing a description here would be the window claiming to know an
        // iPod it cannot resolve.
        None => d.firmware.clone(),
    };
    out.push(device_fact("iPod", ipod));

    let drive = s
        .disks
        .iter()
        .find(|k| Some(k.name.as_str()) == d.disk.as_deref());
    out.push(device_fact(
        "Drive",
        match (&d.disk, &d.disk_path) {
            (Some(n), _) => n.clone(),
            // A device migrated from the old shape carries a resolved path and no name. The file
            // name rather than the path: the path is what the machine rule below prints when it
            // has gone, and a full one does not fit a value column.
            (None, Some(p)) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string()),
            // §10.3's half-made device. Not broken — `Settings::missing` sees nothing wrong with
            // it — so this says unfinished rather than gone.
            (None, None) => "no drive yet".into(),
        },
    ));

    out.push(device_fact(
        "Built from",
        match (drive, d.names_a_disk()) {
            (Some(k), _) => match &k.built_from {
                Some(b) => b.clone(),
                None => "provided whole; nothing here built it".into(),
            },
            (None, true) => "the library has no entry for this drive".into(),
            (None, false) => "nothing yet".into(),
        },
    ));

    out.push(device_fact(
        "Installed",
        match drive {
            Some(k) if !k.installed.is_empty() => k.installed.join(", "),
            // A drive this program built records `Apple's software` as it installs it, so an empty
            // list on a built drive is a real absence rather than a shorthand — and on a provided
            // one it is the truth that nothing here has looked.
            Some(_) | None => "nothing recorded".into(),
        },
    ));

    // §7.5's row 3, which the shelf draws for the live device and nowhere else. **`write_target`
    // is `main.rs`'s and is called rather than re-worded**: the shelf and this page saying two
    // things about whose file is about to be written to is the defect that line exists to prevent.
    //
    // **The finding written down here has been repaired**, and by the model, as it said it had to
    // be. `write_target` read `d.disk_path` directly and answered *no drive yet — nothing will be
    // written* for every device that had been through the settings file once — which is every
    // device from its second launch on, because `render_devices` writes `device.N.disk = <name>`
    // and `parse` reads it back as a name. Meanwhile `writes_to_your_own_image` resolved the same
    // drive the other way, through `built_from`, and painted the warn colour underneath those
    // words. `Settings::disk_of` is public now and is the only resolver; the two functions are one
    // that returns the sentence and the colour together, so this page and the shelf take the same
    // value out of the same `match`. `.line` is the sentence — a fact row has no colour of its own.
    out.push(device_fact("Writes to", crate::write_target(s, d).line));

    // §9.4's machine rule, one per part that has gone, worded by the function the cradle and the
    // Rail both call. A one-element slice, so each line names one part — `gone_sentence` joins a
    // pair with `and`, which is right for the cradle's single row and wrong for a list.
    for a in &s.missing_with(d, seen) {
        out.push(device_rule(crate::gone_sentence(d, std::slice::from_ref(a))));
    }

    out.push(device_act(RowAction::Edit, edit_row(s, d, caps, machine)));
    out.push(device_act(RowAction::Remove, remove_row(s, d, machine)));
    out
}

/// A labelled fact — `devices.slint:92`'s two-column rendering.
fn device_fact(label: &str, value: String) -> Detail {
    Detail {
        label: label.to_string(),
        value,
        mono: false,
        machine_rule: false,
        action: None,
    }
}

/// §9.4's machine rule: prose, in `fg` rather than `fg-dim`, because its teaching is the point.
/// `devices.slint:122` is the branch that draws it — the label is empty, which is what tells the
/// two renderings apart, and `:126` is the colour that draws the difference.
fn device_rule(value: String) -> Detail {
    Detail {
        label: String::new(),
        value,
        mono: false,
        machine_rule: true,
        action: None,
    }
}

/// A `Detail` that is nothing but an act.
///
/// **One property, one producer, and the value is MOVED rather than copied.** `DetailRow` has
/// exactly one `machine-rule`; `devices.slint:56` binds it to the `Pressable` when there is an act
/// and `devices.slint:126` to the paragraph when there is not, and `main.rs`'s `to_detail` reads it
/// off the `Detail`. Leaving the `FixRow`'s copy set as well would be two fields holding one fact
/// on their way to one pixel. `parts::act` is the same three lines for the same reason.
fn device_act(a: RowAction, mut fix: FixRow) -> Detail {
    let machine_rule = std::mem::take(&mut fix.machine_rule);
    Detail {
        label: String::new(),
        value: String::new(),
        mono: false,
        machine_rule,
        action: Some((a, fix)),
    }
}

/// §11.2's Edit route, drawn.
///
/// **Its availability is `rail::Next`'s question, not this file's.** `Next::Fix` is what the Rail
/// asks when a refusal offers to change a recipe, and the Composer is the surface that holds one —
/// so the two cannot word one absent capability two ways, and the escape hatch is `Next`'s own
/// `ipod-boot setup`, which composes a recipe from a terminal and is real.
///
/// **The consequence is `Settings::restate_firmware`'s own argument, said before the press.** That
/// function's doc settles it: the named resource wins, editing one iPod changes every device made
/// of it, and what the window owes is a sentence naming how many — never a refusal, which would be
/// the window contradicting the model. §11.3's arming is the mitigation the design chose.
fn edit_row(s: &Settings, d: &Device, caps: Caps, machine: Option<&str>) -> FixRow {
    let needs = Next::Fix {
        label: "Edit…".into(),
        presses: 1,
    };
    let enabled = needs.available(caps);
    let mut row = FixRow {
        label: needs.label(),
        enabled,
        reason: if enabled {
            String::new()
        } else {
            needs.reason().to_string()
        },
        escape: needs.escape_hatch(caps),
        machine_rule: false,
        presses: 1,
        consequence: String::new(),
    };
    let sharing = s.devices_using_resource(&d.firmware);
    if sharing.len() > 1 {
        row.presses = 2;
        row.consequence = format!(
            "{} devices are made of this iPod — {} — and editing it changes it for all of them. \
             Its drive and its name are this device's alone.",
            sharing.len(),
            sharing.join(", ")
        );
    }
    // **What a running machine is made of must not change under it.** `run_device` resolves the
    // named resource and holds it live, so restating that iPod mid-run would be an edit landing on
    // an ARM7 that is executing. A machine rule: no amount of work on this program makes it safe.
    if let Some(m) = machine.filter(|m| *m == d.name) {
        row.enabled = false;
        row.reason = running_rule(m);
        row.escape = String::new();
        row.machine_rule = true;
        row.presses = 1;
        row.consequence = String::new();
    }
    row
}

/// `Remove` — **the entry, never the files.**
///
/// `Settings::forget` deletes nothing on disk and rewrites no other device, which is the same rule
/// `remove_resource` obeys one page over: a drive image is sometimes the only copy of an iPod
/// somebody owns, and *stop showing me this* and *delete it* must not be one press.
///
/// Two presses, always, because §11.3 arms anything that detaches a reference — and the
/// consequence is never empty, because `primitives.slint` reserves the slot for what the second
/// press will do and a blank there is a control that arms and says nothing.
fn remove_row(s: &Settings, d: &Device, machine: Option<&str>) -> FixRow {
    let mut row = FixRow {
        label: RowAction::Remove.name().to_string(),
        enabled: true,
        reason: String::new(),
        escape: String::new(),
        machine_rule: false,
        presses: 2,
        consequence: removal_consequence(s, d),
    };
    if let Some(m) = machine.filter(|m| *m == d.name) {
        row.enabled = false;
        row.reason = running_rule(m);
        row.machine_rule = true;
        row.presses = 1;
        row.consequence = String::new();
    }
    row
}

/// What goes with the removal, named **before** the press.
///
/// **Not `remove_consequence`**, which is what it wants to be called and which `parts.rs` already
/// is. `no_dead_code_allow_sits_on_a_function_the_program_already_calls` decides a call by text
/// across files, so a second function of that name in this crate makes it report `parts.rs`'s as
/// reconnected the moment this one is called from here — which it did, and which is the same
/// collision that turned `expand` / `boot_screen` / `refusal` into `open_row` / `preview_of` /
/// `refused_because` one file over.
fn removal_consequence(s: &Settings, d: &Device) -> String {
    let mut said = match (s.nor_of(d).is_some(), d.names_a_disk()) {
        (true, true) => format!(
            "The entry goes. Its iPod {} and its drive stay in the library, and neither file is \
             deleted.",
            d.firmware
        ),
        (true, false) => format!(
            "The entry goes. Its iPod {} stays in the library, and no file is deleted.",
            d.firmware
        ),
        // Nothing resolves, so naming what stays would name something that is not there.
        (false, _) => "The entry goes. Nothing is deleted; the library keeps whatever it holds."
            .to_string(),
    };
    if d.parked_at.is_some() {
        said.push_str(
            " The park goes with it — the RAM and the frozen drive behind it are not deleted by \
             this, and nothing in the library records where they are.",
        );
    }
    said
}

/// §7.2's `Start`, for the open device — **the one control on this page the bench also has, and
/// the one question the bench must not be asked.**
///
/// Three arms, in this order, and the order is the argument:
///
/// 1. **A machine exists, so nothing else starts.** §7.2 settles it — *while there is a machine
///    the bench draws it and only it* — and the reason names which one, because an operator with
///    four devices needs to know what to stop, not that something somewhere is running. This is
///    the arm no code in the program had, and it is first rather than last on purpose: the parts
///    a device is missing are already drawn as their own machine-rule lines a few pixels above
///    (see `made_of`), so putting them in this slot as well would spend the one line `Start` has
///    on a fact the reader can already see and lose the one they cannot.
///
///    Note the rule is `machine.is_some()` and not `machine == d.name`, which is what `edit_row`
///    and `remove_row` ask. Those two refuse *this* device because it is executing; this one
///    refuses *every* device because the bench has only one place to draw a machine in.
///
/// 2. **Otherwise `crate::cradle_label`, whatever it says.** Not re-worded here and not
///    paraphrased: it is the sentence the shelf and the cradle are already wearing for this
///    device, and two surfaces disagreeing about why one iPod will not start is precisely the
///    divergence `resolve_for_start`'s own doc records being fixed once already.
///
/// 3. **And `reason` is empty when the control is live**, which `cradle_label` is not — its
///    enabled arm is §7.3's caption, *press the centre button*, and `Pressable.reason` is the
///    refusal slot: `primitives.slint:507` is `text: root.enabled ? root.consequence : root.reason`,
///    so a live control draws its consequence there and its reason nowhere. (Not `:418`, which this
///    used to cite — that is `tells`, and it reserves the slot for **three** reasons: disabled, two
///    presses, or a consequence. The reservation is not the binding.) Handing a live control a
///    reason it will never draw is the kind of field that is true for a while and then quietly
///    becomes a second producer.
///
/// `machine_rule` is computed rather than assumed, and `devices.slint:276` binds what this
/// computes. It used to be a literal `machine-rule: true` in the markup, which is wrong for the
/// composed-and-unbuilt arm — *building a composed device is not wired yet* is §9.4's other kind,
/// a project state, and drawing it in `fg` as a law of physics tells the reader this program will
/// never do it.
fn start_row(s: &Settings, d: &Device, seen: &mut Presence, machine: Option<&str>) -> FixRow {
    let mut row = FixRow {
        label: "Start".into(),
        enabled: true,
        reason: String::new(),
        escape: String::new(),
        machine_rule: false,
        presses: 1,
        consequence: String::new(),
    };
    if let Some(m) = machine {
        row.enabled = false;
        row.reason = running_rule(m);
        row.machine_rule = true;
        return row;
    }
    // `missing_with` rather than `Settings::missing`: `seen` is the pass's shared stat cache and
    // `made_of` has already read this device's paths through it, so the two answers on one row
    // cost one `stat` between them and cannot disagree.
    let gone = s.missing_with(d, seen);
    row.enabled = gone.is_empty() && !crate::composed_and_unbuilt(d);
    if !row.enabled {
        row.reason = crate::cradle_label(d, &gone);
        // A file that is not there cannot be read by anything; a thing this program has not
        // written yet is a project state and carries a command instead (§9.4).
        row.machine_rule = !gone.is_empty();
    }
    row
}

/// §7.2's own sentence for a device the machine is made of.
///
/// **Written twice in this crate**, here and as `parts::inventory`'s `held`, because the two pages
/// refuse different objects for one reason and neither the model nor `rail::Next` words it.
/// Retirement condition, in the shape `research/04` uses for a bypass: it comes off when the model
/// owns the sentence — `Settings::run_device` is what knows a device is live — and both callers
/// read it from there.
///
/// **It lost `Stop it first.`, and that is the reason budget, not brevity for its own sake.** A
/// reason is one eliding line and §9.4's rule is that it be legible; the sentence measured 168 px
/// against `geometry::REASON_MEASURE` 146 for a device called `My 5.5G`, so what a person read
/// was *My 5.5G is running. Stop it f…* — the imperative cut off, which is the only half that was
/// an instruction. What is left names the machine, and stopping it is what the bench's own control
/// does.
///
/// **The name is the operator's and this program does not shorten it.** ` is running` is 64 px, so
/// a name has about 82 px — fifteen characters or so — before the line elides. That is a budget on
/// *this program's* words and not on theirs: a device the operator called
/// `Rockbox on a 5G, second try` is what they called it, and truncating it here would be the window
/// deciding a person's own name for their own iPod is too long.
fn running_rule(machine: &str) -> String {
    format!("{machine} is running")
}

/// The name at a repeater index, or `None`.
///
/// **The one place an index becomes a name**, which is what makes the cursor survive the list
/// changing under it. `refresh_devices` builds the model out of `Settings::devices` in order and
/// removes the tail from the end, so index `i` on the way in is `devices[i]` and nothing else.
fn name_at(s: &Settings, index: i32) -> Option<String> {
    let i = usize::try_from(index).ok()?;
    s.devices.get(i).map(|d| d.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    use eapp_loader::nor;
    use eapp_loader::settings::{Disk, Item, Provenance, Resource};
    use std::path::PathBuf;

    /// Every capability on. The running build has **none** of the first four — see `main::caps` —
    /// so this is the arm that proves a control goes live rather than being disabled for ever.
    fn all_on() -> Caps {
        Caps {
            file_picker: true,
            drop_target: true,
            clipboard: true,
            reveal: true,
            devices_page: true,
            download: true,
            composer: true,
        }
    }

    /// A scratch directory of this test's own. Never inside the operator's data directory.
    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ipod-devices-{}-{tag}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    fn synthetic(model: &str, seed: u64) -> nor::Source {
        nor::Source::Synthetic {
            model: model.into(),
            seed,
            // **A serial the sweep can look for.** `Source::describe` interpolates it; nothing this
            // page calls may, and `no_line_names_the_identity_the_rom_carries` is what says so.
            serial: Some("7B4XX00X3NX".into()),
            guid: Some(0x000A_2700_14EF_E726),
            splash: None,
        }
    }

    fn device(name: &str, firmware: &str, disk: Option<&str>) -> Device {
        Device {
            name: name.into(),
            firmware: firmware.into(),
            disk: disk.map(str::to_string),
            ..Device::default()
        }
    }

    /// Three devices, two of which are made of one iPod, and every file on disk.
    ///
    /// **The images are written rather than named**, so `missing_with` reports nothing and the
    /// machine rules are absent by default — which is what leaves
    /// `a_part_that_has_left_is_named_in_the_body` able to go red by moving one away.
    fn library(dir: &std::path::Path) -> Settings {
        let built = dir.join("my-5.5g.img");
        let spare = dir.join("spare.img");
        std::fs::write(&built, b"not a drive, but it is there").expect("the built image");
        std::fs::write(&spare, b"nor is this").expect("the spare image");
        Settings {
            resources: vec![
                Item {
                    name: "MA146, seed 4f2a".into(),
                    what: Resource::Firmware(synthetic("MA146", 0x4f2a)),
                    from: None,
                },
                Item {
                    name: "the other one".into(),
                    what: Resource::Firmware(synthetic("MA002", 0x11)),
                    from: None,
                },
                Item {
                    name: "iPod_25.1.3.ipsw".into(),
                    what: Resource::Installer(dir.join("a.ipsw")),
                    from: Some(Provenance::Built),
                },
            ],
            disks: vec![
                Disk {
                    name: "my-5.5g.img".into(),
                    path: built,
                    built_from: Some("iPod_25.1.3.ipsw".into()),
                    installed: vec!["Apple's software".into(), "Rockbox".into()],
                },
                Disk {
                    name: "spare.img".into(),
                    path: spare,
                    built_from: None,
                    installed: Vec::new(),
                },
            ],
            devices: vec![
                device("My 5.5G", "MA146, seed 4f2a", Some("my-5.5g.img")),
                device("Second", "MA146, seed 4f2a", Some("spare.img")),
                device("Third", "the other one", None),
            ],
            ..Settings::default()
        }
    }

    fn view_of(p: &mut Devices, s: &Settings, caps: Caps) -> View {
        let mut seen = Presence::new();
        p.view(s, &mut seen, caps, None)
    }

    /// Every string the page would draw, acts included.
    fn every_string(v: &View) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for d in &v.detail {
            out.push(d.label.clone());
            out.push(d.value.clone());
            if let Some((_, f)) = &d.action {
                out.extend([
                    f.label.clone(),
                    f.reason.clone(),
                    f.escape.clone(),
                    f.consequence.clone(),
                ]);
            }
        }
        // §7.2's `Start` is drawn inside the same body and carries a sentence of its own, so a
        // sweep over what this page says has to read it too.
        if let Some(f) = &v.start {
            out.extend([
                f.label.clone(),
                f.reason.clone(),
                f.escape.clone(),
                f.consequence.clone(),
            ]);
        }
        out
    }

    fn fact<'a>(v: &'a View, label: &str) -> &'a str {
        v.detail
            .iter()
            .find(|d| d.label == label)
            .map(|d| d.value.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "no `{label}` line; the body drew {:?}",
                    v.detail.iter().map(|d| d.label.as_str()).collect::<Vec<_>>()
                )
            })
    }

    fn act_of(v: &View, a: RowAction) -> &FixRow {
        v.detail
            .iter()
            .find_map(|d| match &d.action {
                Some((x, f)) if *x == a => Some(f),
                _ => None,
            })
            .unwrap_or_else(|| panic!("the body draws no {:?}", a))
    }

    // ─── The cursor ─────────────────────────────────────────────────────────────────────────────

    /// **The open device is kept by name, so a device arriving above it moves the index and not
    /// the body.**
    ///
    /// The whole reason the cursor is not the number the markup sends. Proved by mutation: hold
    /// the index instead of the name and the second half reads `Second`'s Expand open over
    /// `Nought`'s row — `left: 1  right: 2` on the index, and the body is the wrong device's.
    #[test]
    fn the_open_device_survives_a_device_arriving_above_it() {
        let dir = scratch("cursor");
        let mut s = library(&dir);
        let mut p = Devices::new();
        p.open_row(&s, 1, true);
        let before = view_of(&mut p, &s, all_on());
        assert_eq!(before.detail_of, 1, "the second device is the open one");
        assert_eq!(fact(&before, "Drive"), "spare.img");

        s.devices.insert(0, device("Nought", "the other one", None));
        let after = view_of(&mut p, &s, all_on());
        assert_eq!(
            after.detail_of, 2,
            "the open device moved down one and the cursor did not follow it"
        );
        assert_eq!(
            fact(&after, "Drive"),
            "spare.img",
            "the body is somebody else's now"
        );

        // And the other direction, which is the one a removal takes.
        s.devices.remove(0);
        s.devices.remove(0);
        let last = view_of(&mut p, &s, all_on());
        assert_eq!(last.detail_of, 0);
        assert_eq!(fact(&last, "Drive"), "spare.img");
    }

    /// A cursor whose device has left the list closes itself rather than pointing at an index the
    /// list has re-used.
    #[test]
    fn removing_the_open_device_closes_the_body() {
        let dir = scratch("closes");
        let mut s = library(&dir);
        let mut p = Devices::new();
        p.open_row(&s, 1, true);
        assert_eq!(
            p.row_action(&mut s, RowAction::Remove, None),
            Ok(Wrote::Library)
        );
        let v = view_of(&mut p, &s, all_on());
        assert_eq!(v.detail_of, -1, "the markup's own nothing-open");
        assert!(v.detail.is_empty(), "a body with no device under it");
        assert!(v.start.is_none(), "a `Start` for a device that is not on screen");
    }

    /// An index nothing answers to opens nothing, and a press with nothing open acts on nothing.
    ///
    /// Two halves because the boundary has two shapes now: an index enters at `open_row` and
    /// nowhere else, so that is the one place a number out of range has to be survivable, and
    /// every act reads the cursor instead — where the thing that can be absent is the cursor.
    #[test]
    fn an_index_nothing_answers_to_acts_on_nothing() {
        let dir = scratch("no-index");
        let mut s = library(&dir);
        let was = s.devices.clone();
        let mut p = Devices::new();

        p.open_row(&s, 1, true);
        p.open_row(&s, 99, true);
        assert_eq!(view_of(&mut p, &s, all_on()).detail_of, -1);

        p.open_row(&s, 1, true);
        p.open_row(&s, -1, true);
        assert_eq!(view_of(&mut p, &s, all_on()).detail_of, -1);

        // Nothing is open now, so there is no control on screen and no subject for one.
        assert_eq!(
            p.row_action(&mut s, RowAction::Remove, None),
            Ok(Wrote::Nothing)
        );
        assert!(p.editor(&s).is_none());

        // And a cursor whose device left the library between the push and the press: `forget`
        // would write nothing, so answering `Library` for it would have `main.rs` regenerate the
        // settings file — and `Settings::render` takes any comment the operator added with it.
        p.open_row(&s, 1, true);
        s.devices.retain(|d| d.name != "Second");
        assert_eq!(
            p.row_action(&mut s, RowAction::Remove, None),
            Ok(Wrote::Nothing)
        );
        assert!(p.editor(&s).is_none());

        s.devices = was.clone();
        assert_eq!(s.devices, was, "a press on nothing changed the library");
    }

    /// **A press acts on the device that is open, not on the row the last push put it in.**
    ///
    /// The whole reason `row_action` and `editor` take no index. Every act this page draws lives
    /// inside the open row, so the number `row-action(int, int)` carries is `detail-of` from the
    /// last push — and one insertion later it names the device above. Proved red by resolving that
    /// index instead: `Remove` then forgets `My 5.5G`, which nobody pressed, and the Composer
    /// opens on it.
    #[test]
    fn a_press_acts_on_the_open_device_after_the_list_moved_under_it() {
        let dir = scratch("moved");
        let mut s = library(&dir);
        let mut p = Devices::new();

        p.open_row(&s, 1, true);
        assert_eq!(
            view_of(&mut p, &s, all_on()).detail_of,
            1,
            "`Second` is the open row, and 1 is what the push wrote into `detail-of`"
        );

        // The library gains a device from somewhere that is not this page — a build finishing, a
        // re-read of the file — between the push and the press. `refresh_devices` will re-push,
        // but the press in flight was drawn against the old list.
        s.devices.insert(0, device("Nought", "the other one", None));
        assert_eq!(s.devices[1].name, "My 5.5G", "the fixture no longer moves anything");

        let c = p.editor(&s).expect("the open device");
        assert_eq!(
            c.mode(),
            &crate::composer::Mode::Editing {
                device: "Second".into()
            },
            "the Composer opened on the device now sitting at the pressed index"
        );

        assert_eq!(
            p.row_action(&mut s, RowAction::Remove, None),
            Ok(Wrote::Library)
        );
        assert_eq!(
            s.devices.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            ["Nought", "My 5.5G", "Third"],
            "the press forgot the device at the index rather than the one that was open"
        );
    }

    // ─── The body ───────────────────────────────────────────────────────────────────────────────

    /// **Every device fills all five lines, whatever it is missing.**
    ///
    /// §16.3's rule about a surface that changes shape while it is being read: the half-made
    /// device in the fixture has no drive at all, and its body is the same five rows as the
    /// finished one's with different words in them.
    #[test]
    fn every_device_draws_all_five_facts_including_the_one_that_is_half_made() {
        let dir = scratch("five");
        let s = library(&dir);
        let mut p = Devices::new();
        for i in 0..3 {
            p.open_row(&s, i, true);
            let v = view_of(&mut p, &s, all_on());
            let labels: Vec<&str> = v
                .detail
                .iter()
                .filter(|d| !d.label.is_empty())
                .map(|d| d.label.as_str())
                .collect();
            assert_eq!(
                labels,
                ["iPod", "Drive", "Built from", "Installed", "Writes to"],
                "device {i} draws a different set of facts"
            );
            for d in &v.detail {
                assert!(
                    d.action.is_some() || !d.value.is_empty(),
                    "device {i}'s `{}` line is blank",
                    d.label
                );
            }
        }

        p.open_row(&s, 2, true);
        let half = view_of(&mut p, &s, all_on());
        assert_eq!(fact(&half, "Drive"), "no drive yet");
        assert_eq!(fact(&half, "Built from"), "nothing yet");
        assert_eq!(fact(&half, "Installed"), "nothing recorded");
    }

    /// **The drive's own facts are the disk's, and they are read rather than inferred.**
    ///
    /// A drive somebody supplied and a drive this program built say different things, and the
    /// difference is `Disk::built_from`. Proved red by answering `built_from` for both.
    #[test]
    fn a_provided_drive_and_a_built_one_say_which_they_are() {
        let dir = scratch("built");
        let s = library(&dir);
        let mut p = Devices::new();

        p.open_row(&s, 0, true);
        let built = view_of(&mut p, &s, all_on());
        assert_eq!(fact(&built, "Built from"), "iPod_25.1.3.ipsw");
        assert_eq!(fact(&built, "Installed"), "Apple's software, Rockbox");

        p.open_row(&s, 1, true);
        let provided = view_of(&mut p, &s, all_on());
        assert_eq!(
            fact(&provided, "Built from"),
            "provided whole; nothing here built it"
        );
        assert_eq!(fact(&provided, "Installed"), "nothing recorded");
    }

    /// **A part that has left is named, in the sentence the cradle and the Rail already use.**
    ///
    /// §9: a failure names *what* is wrong. Proved red by dropping the `missing_with` call — the
    /// body then draws five cheerful facts about a device that cannot start, which is exactly the
    /// state the shipped bench used to draw as fine.
    #[test]
    fn a_part_that_has_left_is_named_in_the_body() {
        let dir = scratch("gone");
        let mut s = library(&dir);
        let mut p = Devices::new();

        p.open_row(&s, 0, true);
        assert!(
            view_of(&mut p, &s, all_on())
                .detail
                .iter()
                .all(|d| !d.machine_rule),
            "the fixture already has something missing, so this test measures nothing"
        );

        // The drive leaves the disk, and the iPod leaves the library.
        let path = s.disks[0].path.clone();
        std::fs::remove_file(&path).expect("removing the image this test wrote");
        s.resources.retain(|it| it.name != "MA146, seed 4f2a");

        let v = view_of(&mut p, &s, all_on());
        let rules: Vec<&str> = v
            .detail
            .iter()
            .filter(|d| d.machine_rule && d.action.is_none())
            .map(|d| d.value.as_str())
            .collect();
        assert_eq!(rules.len(), 2, "both absences were expected, got {rules:?}");
        assert!(
            rules[0].contains("MA146, seed 4f2a") && rules[0].contains("not in the library"),
            "the iPod's absence is not named first: {rules:?}"
        );
        assert!(
            rules[1].contains("my-5.5g.img") && rules[1].contains(&path.display().to_string()),
            "the drive's absence does not name the path: {rules:?}"
        );
        // And the fact lines still say what the device names, rather than going blank.
        assert_eq!(fact(&v, "iPod"), "MA146, seed 4f2a");
        assert_eq!(fact(&v, "Drive"), "my-5.5g.img");
    }

    /// **No line names the identity the ROM carries.**
    ///
    /// `nor::Source::describe` interpolates `id.serial` into its sentence and is one call away from
    /// every line here; `devices.slint:38` names it as the thing this file must not reach for.
    /// Proved red by wording the iPod line with `describe()` instead — the serial then appears in
    /// a body a screenshot would carry.
    #[test]
    fn no_line_names_the_identity_the_rom_carries() {
        let dir = scratch("identity");
        let s = library(&dir);
        let mut p = Devices::new();

        // The control: the source this fixture is built on really does carry the identity, so an
        // absence below is this file's discipline rather than an empty fixture.
        let src = s.nor_of(&s.devices[0]).expect("the fixture's iPod");
        assert!(
            src.describe().contains("7B4XX00X3NX"),
            "the model's own description does not carry the serial: {}",
            src.describe()
        );

        for i in 0..3 {
            p.open_row(&s, i, true);
            let v = view_of(&mut p, &s, all_on());
            for text in every_string(&v) {
                for secret in ["7B4XX00X3NX", "000A270014EFE726"] {
                    assert!(
                        !text.contains(secret),
                        "device {i}'s body carries `{secret}`: {text}"
                    );
                }
            }
        }
    }

    // ─── The controls ───────────────────────────────────────────────────────────────────────────

    /// **Every disabled control states its reason, and every live one wears none.**
    ///
    /// The invariant `primitives.slint` states about a `Pressable`, asked of everything this
    /// producer can emit, across both capability arms and both machine states.
    #[test]
    fn every_disabled_control_states_its_reason() {
        let dir = scratch("reasons");
        let mut s = library(&dir);
        // **A device that cannot start for a reason that is not the machine**, because without
        // one the only refused `Start` in this sweep is the machine arm and the whole of
        // [`start_row`]'s second half is swept while it happens to be answering `enabled`.
        s.devices.push(Device {
            composed: true,
            ..device("Composed", "the other one", None)
        });
        let mut checked = 0usize;
        let mut refused = 0usize;
        for caps in [Caps::default(), all_on()] {
            for machine in [None, Some("Second")] {
                let mut p = Devices::new();
                let mut seen = Presence::new();
                for i in 0..4 {
                    p.open_row(&s, i, true);
                    let v = p.view(&s, &mut seen, caps, machine);
                    let acts = v.detail.iter().filter(|d| d.action.is_some()).count();
                    assert_eq!(acts, 2, "device {i} draws {acts} acts");
                    // **`Start` is swept with the other two.** It is the third control in the same
                    // body and the only one whose refusal `primitives.slint` would have drawn from
                    // a field this page does not own — see [`start_row`].
                    let start = v.start.clone().expect("an open row draws a `Start`");
                    let all: Vec<&FixRow> = v
                        .detail
                        .iter()
                        .filter_map(|d| d.action.as_ref().map(|(_, f)| f))
                        .chain(std::iter::once(&start))
                        .collect();
                    for f in all {
                        checked += 1;
                        assert!(!f.label.is_empty(), "an act with no label");
                        assert!(
                            f.enabled || !f.reason.is_empty(),
                            "`{}` is disabled and says nothing",
                            f.label
                        );
                        assert!(
                            !f.enabled || f.reason.is_empty(),
                            "`{}` is pressable and still wears a refusal: {}",
                            f.label,
                            f.reason
                        );
                        assert!(
                            f.presses < 2 || !f.consequence.is_empty(),
                            "`{}` arms and says nothing about what the second press does",
                            f.label
                        );
                        refused += usize::from(!f.enabled);
                    }
                }
            }
        }
        // Two capability arms, two machine states, four devices, three controls. The floor sits
        // **on** the population rather than under it, so a control that stops being emitted turns
        // this red instead of quietly shrinking what the sweep reads.
        assert_eq!(checked, 48, "the sweep read {checked} controls");
        assert!(
            refused > 0,
            "nothing was ever refused, so the disabled half of the sweep read nothing"
        );
    }

    /// **`Edit…` is refused in the words `rail::Next` already uses for an absent Composer.**
    ///
    /// One refusal, worded once: the Rail draws the same sentence and the same escape hatch for
    /// the same absent capability. Proved red by writing the sentence here instead.
    #[test]
    fn the_edit_act_wears_rails_own_sentence_when_there_is_no_composer() {
        let dir = scratch("no-composer");
        let s = library(&dir);
        let mut p = Devices::new();
        p.open_row(&s, 0, true);

        let off = view_of(&mut p, &s, Caps::default());
        let e = act_of(&off, RowAction::Edit);
        let needs = Next::Fix {
            label: "Edit…".into(),
            presses: 1,
        };
        assert!(!e.enabled);
        assert_eq!(e.reason, needs.reason());
        assert_eq!(e.escape, needs.escape_hatch(Caps::default()));
        assert!(!e.escape.is_empty(), "a project state names no escape hatch");

        let on = view_of(&mut p, &s, all_on());
        let e = act_of(&on, RowAction::Edit);
        assert!(e.enabled, "the Composer exists and the control is still dead");
        assert!(e.reason.is_empty() && e.escape.is_empty());
    }

    /// **Editing an iPod two devices are made of arms first, and names them.**
    ///
    /// `Settings::restate_firmware`'s own argument, said before the press: the named resource
    /// wins, so an edit lands on every device made of it. Proved red by arming unconditionally —
    /// the half-made device shares its iPod with nobody and must stay a single press.
    #[test]
    fn editing_a_shared_ipod_arms_and_names_the_devices_it_changes() {
        let dir = scratch("shared");
        let s = library(&dir);
        let mut p = Devices::new();

        p.open_row(&s, 0, true);
        let shared = act_of(&view_of(&mut p, &s, all_on()), RowAction::Edit).clone();
        assert_eq!(shared.presses, 2, "a shared iPod's edit does not arm");
        assert!(
            shared.consequence.contains("My 5.5G") && shared.consequence.contains("Second"),
            "the consequence does not name both devices: {}",
            shared.consequence
        );

        p.open_row(&s, 2, true);
        let alone = act_of(&view_of(&mut p, &s, all_on()), RowAction::Edit).clone();
        assert_eq!(alone.presses, 1, "an unshared iPod's edit arms for nothing");
        assert!(alone.consequence.is_empty());
    }

    /// **The Edit route opens the Composer on the device, with the model's own recipe.**
    ///
    /// §11.2's *existing and new look identical* had no surface: `Composer::editing` had no caller
    /// outside its own file, so nothing ever constructed `Mode::Editing`. Proved red by returning
    /// `Composer::new()` — the mode is then `New` and the recipe is `nothing chosen`.
    ///
    /// **And the second half is the measurement that matters more.** `Settings::recipe_of` treats
    /// `Device::boot_shape` as the authority on what a device boots, and `Composer::commit` records
    /// it — so both arms are reachable in production and both are asserted here. The fixture's
    /// device takes the fallback because `library` builds it by hand and records no shape, which is
    /// also every device written before that writer existed;
    /// `composer::tests::a_composed_device_records_what_it_boots_and_reopens_on_it` is the same
    /// pair measured from the writing end.
    #[test]
    fn the_edit_route_carries_the_recipe_the_model_resolves() {
        use eapp_loader::compose::{BootShape, Loader, Os, Start};

        let dir = scratch("edit");
        let mut s = library(&dir);
        let mut p = Devices::new();
        p.open_row(&s, 0, true);

        let c = p.editor(&s).expect("the first device");
        assert_eq!(
            c.mode(),
            &crate::composer::Mode::Editing {
                device: "My 5.5G".into()
            },
            "the Composer opened as a new device rather than on this one"
        );
        assert_eq!(c.recipe(), &s.recipe_of(&s.devices[0]));
        // Derived from the drive, because no shape is recorded: the install list holds Apple's
        // software and Rockbox, so `best_loader` reaches Rockbox's bootloader.
        assert_eq!(
            c.recipe().start,
            Start::FromDisk {
                name: "my-5.5g.img".into(),
                fat_type: None
            }
        );
        assert_eq!(c.recipe().oses, [Os::Apple, Os::Rockbox].into_iter().collect());

        // The other branch, armed the way `Composer::commit` arms it — through `set_boot_shape`,
        // which is the only writer there is.
        let shape = BootShape {
            loader: Loader::Apple,
            oses: [Os::Apple].into_iter().collect(),
        };
        assert!(s.set_boot_shape("My 5.5G", &shape));
        let c = p.editor(&s).expect("the first device");
        assert_eq!(
            c.recipe().loader,
            Loader::Apple,
            "the recorded shape did not win over the drive's install list"
        );
        assert_eq!(c.recipe().oses, [Os::Apple].into_iter().collect());
    }

    /// **§7.2, verbatim: while there is a machine, every other device's `Start` is refused, and
    /// the sentence names the machine.**
    ///
    /// `My 5.5G is running` is the doc's own example, shortened to §9.4's reason budget by
    /// [`running_rule`], and naming the machine is why a person with four devices can act on it. Nothing in this program produced it before: `device_rows` is
    /// handed no machine, so a page opened beside a running ARM7 drew a live `Start` on every row.
    /// Proved red by asking `machine == d.name` — the rule the two acts below it ask — which
    /// leaves the two devices that are *not* the machine offering to start a second one.
    #[test]
    fn start_is_refused_while_a_machine_exists_and_says_which() {
        let dir = scratch("start-machine");
        let s = library(&dir);
        let mut p = Devices::new();
        let mut seen = Presence::new();

        // The control: with no machine, every device in this fixture starts, so an absence below
        // is the rule firing rather than a fixture that could never start anything.
        for i in 0..3 {
            p.open_row(&s, i, true);
            let start = p
                .view(&s, &mut seen, all_on(), None)
                .start
                .expect("an open row draws a `Start`");
            assert!(
                start.enabled,
                "device {i} is refused before there is a machine: {}",
                start.reason
            );
            assert!(start.reason.is_empty(), "a live control wears a refusal");
        }

        for i in 0..3 {
            p.open_row(&s, i, true);
            let start = p
                .view(&s, &mut seen, all_on(), Some("My 5.5G"))
                .start
                .expect("an open row draws a `Start`");
            assert!(
                !start.enabled,
                "device {i} offers to start a second machine while one is running"
            );
            assert_eq!(start.reason, "My 5.5G is running");
            assert!(start.machine_rule, "§7.2 calls this a machine rule");
            assert!(start.escape.is_empty(), "no command gets round a running machine");
        }
    }

    /// **A device whose parts have moved is refused by name, in the cradle's own words — and the
    /// two kinds of refusal are told apart.**
    ///
    /// §9.4: a file that is not there is a machine rule; a thing this program has not written yet
    /// is a project state and is drawn differently. Proved red twice — by dropping the
    /// `missing_with` call, which offers to start a device whose drive was deleted an hour ago,
    /// and by hard-coding `machine_rule` true the way `devices.slint` used to, which tells the
    /// reader that building a composed device is a law of physics.
    #[test]
    fn start_names_the_part_that_has_gone_in_the_cradles_own_words() {
        let dir = scratch("start-gone");
        let mut s = library(&dir);
        let mut p = Devices::new();
        p.open_row(&s, 0, true);

        assert!(
            view_of(&mut p, &s, all_on())
                .start
                .expect("a `Start`")
                .enabled,
            "the fixture's first device cannot start with its drive still on disk"
        );

        let path = s.disks[0].path.clone();
        std::fs::remove_file(&path).expect("removing the image this test wrote");

        let start = view_of(&mut p, &s, all_on()).start.expect("a `Start`");
        assert!(!start.enabled, "a device whose drive has gone still offers to start");
        assert!(start.machine_rule, "a file that is not there is a machine rule");
        assert!(
            start.reason.contains(&path.display().to_string()),
            "the refusal does not name the path: {}",
            start.reason
        );
        // Worded once. The shelf, the cradle and this page cannot say three things about one
        // device, so this is `crate::cradle_label`'s sentence rather than a second one beside it.
        let mut fresh = Presence::new();
        assert_eq!(
            start.reason,
            crate::cradle_label(&s.devices[0], &s.missing_with(&s.devices[0], &mut fresh))
        );

        // §9.4's other kind, on the same control.
        s.devices.push(Device {
            composed: true,
            ..device("Composed", "the other one", None)
        });
        p.open_row(&s, 3, true);
        let start = view_of(&mut p, &s, all_on()).start.expect("a `Start`");
        assert!(!start.enabled, "a composed device with no drive offers to start");
        assert!(
            !start.machine_rule,
            "a thing this program has not built yet is drawn as a law of physics: {}",
            start.reason
        );
        assert!(start.reason.contains("not wired"), "{}", start.reason);
    }

    // ─── Acting ─────────────────────────────────────────────────────────────────────────────────

    /// **`Remove` forgets the entry and deletes nothing.**
    ///
    /// Both halves, because the second is the one that cannot be undone: the drive image and the
    /// iPod stay in the library and both files stay on disk. Proved red by having the arm delete
    /// the disk entry as well.
    #[test]
    fn removing_a_device_forgets_the_entry_and_deletes_nothing() {
        let dir = scratch("remove");
        let mut s = library(&dir);
        let images: Vec<PathBuf> = s.disks.iter().map(|k| k.path.clone()).collect();
        let mut p = Devices::new();
        p.open_row(&s, 1, true);

        assert_eq!(
            p.row_action(&mut s, RowAction::Remove, None),
            Ok(Wrote::Library)
        );
        assert_eq!(
            s.devices.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            ["My 5.5G", "Third"]
        );
        assert_eq!(s.disks.len(), 2, "a drive left the library with the device");
        assert_eq!(s.resources.len(), 3, "a part left the library with the device");
        for p in &images {
            assert!(p.exists(), "{} was deleted", p.display());
        }
    }

    /// **`Remove` arms, and the second press is named before the first is taken.**
    ///
    /// §11.3 arms anything that detaches a reference, and [`removal_consequence`] is what the
    /// arming says. **Neither was measured by anything**: dropping `presses` to 1 was **GREEN**
    /// across all seventeen tests, because `every_disabled_control_states_its_reason` only asks for
    /// a consequence *when* a control arms — a control that stops arming stops being asked. So a
    /// single-press `Remove` on somebody's only iPod would have shipped under a green suite.
    ///
    /// And the sentence has three arms plus a park, chosen by what the device still resolves to,
    /// and **none of the four was reached by the fixture above**: dropping the park half entirely
    /// was green too. What each one exists to say is what the operator is about to lose, so a
    /// wrong one is worse than a missing one — hence the values rather than a non-emptiness check.
    #[test]
    fn removing_a_device_arms_and_names_what_it_leaves_behind() {
        let dir = scratch("arming");
        let mut s = library(&dir);
        // A device migrated from the old shape — `disk_path` and no name — that has been parked.
        // Both are shapes `library` has none of, and the park is the fourth arm.
        s.devices.push(Device {
            disk_path: Some(dir.join("migrated.img")),
            parked_at: Some(1_700_000_000),
            ..device("Migrated", "iPod_25.1.3.ipsw", None)
        });
        let mut p = Devices::new();

        // ── (resolves, names a drive) ──
        p.open_row(&s, 0, true);
        let r = act_of(&view_of(&mut p, &s, all_on()), RowAction::Remove).clone();
        assert_eq!(r.presses, 2, "§11.3: detaching a reference does not arm");
        assert_eq!(
            r.consequence,
            "The entry goes. Its iPod MA146, seed 4f2a and its drive stay in the library, and \
             neither file is deleted."
        );

        // ── (resolves, no drive) — the half-made device, which must not promise a drive stays ──
        p.open_row(&s, 2, true);
        let r = act_of(&view_of(&mut p, &s, all_on()), RowAction::Remove).clone();
        assert_eq!(r.presses, 2);
        assert_eq!(
            r.consequence,
            "The entry goes. Its iPod the other one stays in the library, and no file is deleted."
        );

        // ── (nothing resolves) plus the park, which is appended rather than substituted ──
        p.open_row(&s, 3, true);
        let r = act_of(&view_of(&mut p, &s, all_on()), RowAction::Remove).clone();
        assert_eq!(r.presses, 2);
        assert_eq!(
            r.consequence,
            "The entry goes. Nothing is deleted; the library keeps whatever it holds. The park \
             goes with it — the RAM and the frozen drive behind it are not deleted by this, and \
             nothing in the library records where they are."
        );

        // Same device without the park: the first sentence is unchanged and the second is gone, so
        // the park half is proved to be the thing that added it rather than the arm that was taken.
        s.devices[3].parked_at = None;
        p.open_row(&s, 3, true);
        let r = act_of(&view_of(&mut p, &s, all_on()), RowAction::Remove).clone();
        assert_eq!(
            r.consequence,
            "The entry goes. Nothing is deleted; the library keeps whatever it holds."
        );
    }

    /// **The iPod line is the words a person would use, and says what the library files it under
    /// only when the two differ.**
    ///
    /// Both arms, because saying it twice reads as two facts and saying it once loses the name
    /// `Remove`'s own sentence uses. Neither was measured — collapsing the line to
    /// `suggest_ipod_name` alone was **GREEN**, because the only test that read this value was
    /// looking for a serial in it.
    ///
    /// **And the migrated device's drive is named by file name rather than by path**, which was
    /// green to break as well: a full path does not fit the value column and the path is what the
    /// machine rule underneath already prints.
    #[test]
    fn the_ipod_line_says_what_it_is_and_what_it_is_filed_as() {
        let dir = scratch("filed-as");
        let mut s = library(&dir);
        let mut p = Devices::new();

        // The fixture files its ROM under the recipe — `<model>, seed <n>` — and a person calls
        // that iPod `Black 5G`, so this is the two-name arm.
        p.open_row(&s, 0, true);
        assert_eq!(
            fact(&view_of(&mut p, &s, all_on()), "iPod"),
            "Black 5G, filed as MA146, seed 4f2a"
        );

        // Filed under the name a person uses, which is what `Composer::commit` does. One name, said
        // once.
        s.resources[0].name = "Black 5G".into();
        for d in &mut s.devices {
            if d.firmware == "MA146, seed 4f2a" {
                d.firmware = "Black 5G".into();
            }
        }
        p.open_row(&s, 0, true);
        assert_eq!(fact(&view_of(&mut p, &s, all_on()), "iPod"), "Black 5G");

        // A resource of the wrong kind resolves to nothing, so the line says what the device
        // *names* and the machine rule below says the library no longer holds it. Inventing a
        // description here would be the window claiming to know an iPod it cannot resolve.
        s.devices.push(Device {
            disk_path: Some(dir.join("migrated.img")),
            ..device("Migrated", "iPod_25.1.3.ipsw", None)
        });
        p.open_row(&s, 3, true);
        let v = view_of(&mut p, &s, all_on());
        assert_eq!(fact(&v, "iPod"), "iPod_25.1.3.ipsw");
        assert_eq!(
            fact(&v, "Drive"),
            "migrated.img",
            "the drive is named by its path rather than by its file name"
        );
        assert_eq!(fact(&v, "Built from"), "the library has no entry for this drive");
    }

    /// **Nothing the machine is made of can be removed or edited while it is running**, and the
    /// rule is asked twice — once when the control is drawn and again inside `row_action`, because
    /// the control was drawn at the last push and the machine may have started since.
    ///
    /// Proved red by dropping the guard from `row_action`: the drawn control still refuses, and the
    /// press files the removal anyway.
    #[test]
    fn nothing_the_machine_is_made_of_can_be_removed_while_it_is_running() {
        let dir = scratch("running");
        let mut s = library(&dir);
        let mut p = Devices::new();
        let mut seen = Presence::new();
        p.open_row(&s, 1, true);

        let v = p.view(&s, &mut seen, all_on(), Some("Second"));
        for a in [RowAction::Edit, RowAction::Remove] {
            let f = act_of(&v, a);
            assert!(!f.enabled, "{a:?} is pressable on the running device");
            assert_eq!(f.reason, "Second is running");
            // **And the refusal disarms it.** `Second` shares its iPod with `My 5.5G`, so `Edit…`
            // was two presses and carried the sentence naming both, and `Remove` is two presses
            // always — a control that cannot be pressed at all still reserving §11.3's *press
            // again to* slot is a row telling the reader to do something it will not accept.
            // Green to break before this line: nothing read `presses` on a refused control.
            assert_eq!(f.presses, 1, "{a:?} is refused and still arms");
            assert!(
                f.consequence.is_empty(),
                "{a:?} is refused and still names what a second press would do: {}",
                f.consequence
            );
        }
        // §9.4's two kinds: this one is a machine rule, and it is the `Detail`'s field rather than
        // the `FixRow`'s — `to_detail` reads it there and `DetailRow` has one of it.
        for d in v.detail.iter().filter(|d| d.action.is_some()) {
            assert!(d.machine_rule, "the refusal is not drawn as a machine rule");
            let (_, f) = d.action.as_ref().expect("the act");
            assert!(
                !f.machine_rule,
                "the `FixRow` kept a second copy of the machine rule"
            );
        }

        assert_eq!(
            p.row_action(&mut s, RowAction::Remove, Some("Second")),
            Err("Second is running".into())
        );
        assert_eq!(s.devices.len(), 3, "the running device was removed anyway");

        // A different device is not the machine, and is not refused — the two acts ask
        // `machine == this device`, which is not the question §7.2's `Start` asks. See
        // [`start_row`].
        p.open_row(&s, 2, true);
        assert_eq!(
            p.row_action(&mut s, RowAction::Remove, Some("Second")),
            Ok(Wrote::Library)
        );
    }

    /// **An act this page does not draw acts on nothing and says so**, which is the exhaustive arm
    /// rather than a route: the six below are `parts.rs`'s controls, the bench's own `Start`, and
    /// `Edit`, which changes no library at all.
    ///
    /// **The `|| a == RowAction::Start` escape this shipped with is deleted.** It exempted the one
    /// arm whose sentence is written by hand rather than by `format!`, so that arm's message could
    /// be emptied and this stayed green — measured: `Err(String::new())` for `Start` was **GREEN**
    /// across all seventeen. `RowAction::Start.name()` is `"Start"` and the sentence opens with it,
    /// so the escape was never carrying anything; it was an exemption that would only ever be
    /// noticed by whoever broke the thing it exempted.
    #[test]
    fn an_act_this_page_does_not_draw_changes_nothing() {
        let dir = scratch("not-ours");
        let mut s = library(&dir);
        let was = s.clone();
        let mut p = Devices::new();
        p.open_row(&s, 0, true);
        for a in RowAction::ALL {
            if a == RowAction::Remove {
                continue;
            }
            let said = p
                .row_action(&mut s, a, None)
                .expect_err(&format!("{a:?} was accepted by a page that does not draw it"));
            assert!(
                said.contains(a.name()),
                "the refusal does not name the control: {said}"
            );
        }
        assert_eq!(s.devices, was.devices, "a refusal changed the library");
        assert_eq!(s.disks, was.disks);
        assert_eq!(s.resources.len(), was.resources.len());
    }
}
