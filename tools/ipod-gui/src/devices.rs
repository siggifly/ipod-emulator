// Devices — docs/GUI.md §7.2, §7.5.
//
// **A stub, and the one thing in it that is not a stub is the import.** The page's producer — the
// thing that expands one device into its `Made of` lines and its acts — is next wave. What is
// settled here is that `Detail` comes from `parts.rs` and is not declared a second time: the two
// pages draw the same `primitives.slint` `DetailRow` through the same flattener in `main.rs`, so a
// second `Detail` here would be two vocabularies for one struct and the two would drift on the
// first field either page gained.
//
// **This page pins no ordinal.** `ui/devices.slint` fires `root.act(root.d.action, root.index)` and
// nothing else — every number it sends is one Rust put on the row. The one that matters,
// `RowAction::Remove`, is pinned by `ui/parts.slint` and is written down there.
//
// The state is the open device's **name** and never its index. A device inserted or removed above
// the open one moves every index below it, and an `Expand` that followed an index would then be
// showing somebody else's identity — which is the lesson `Composer::device_vanished` already
// learned once, about a name held across a run that replaced it.

use crate::parts::Detail;

/// What the page draws that `refresh_devices` does not already push.
///
/// **Two fields, because `push_devices_detail` writes two properties.** `devices-empty-line` and
/// `devices-new` stay where they are: a second writer for a property one function already pushes
/// is how two producers come to disagree about one page.
#[allow(dead_code)] // retired when: `Devices::view` returns one and `push_devices_detail` flattens it
pub struct View {
    pub detail: Vec<Detail>,
    /// The index of the open device, or `-1`, which is the markup's own default.
    pub detail_of: i32,
}

/// The Devices page's whole state: which device is expanded, by name.
///
/// Not an `Option<Devices>`: the page exists from startup. `open` is the `Option` — there is
/// genuinely no device expanded most of the time.
#[allow(dead_code)] // retired when: `Devices::view` reads it and `Devices::expand` writes it — the producer, next wave
pub struct Devices {
    open: Option<String>,
}

#[allow(dead_code)] // retired when: `wire` constructs one beside the Composer's cell — the integrator's step, after the producer lands
impl Devices {
    pub fn new() -> Devices {
        Devices { open: None }
    }
}
