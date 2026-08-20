//! A snapshot has to restore a machine that still runs.
//!
//! **This test exists because one did not.** `Machine::snapshot` carried every region, every alias,
//! both interrupt banks and the whole CPU — and left out `mmap_regs`, the sixteen words that define
//! the MMAP unit's address windows. RetailOS executes entirely through the low window (a PC of
//! `0x002079dc` is SDRAM at `0x102079dc` seen through it), so a restored machine read **zeros** at
//! its own program counter, executed `andeq r0, r0, r0` — a NOP — through address space, and was
//! declared `Lost` a few hundred instructions later.
//!
//! Nothing reported it. The co-processor still held the last picture it had been given, so the
//! window showed a normal iPod that would not respond to the wheel or the hold switch, and the bug
//! arrived as "the restored machine will not take input".
//!
//! The check that catches it is one line — **execute after restoring, and compare** — and it is
//! worth more than any assertion about the format, because it cannot pass while the machine is
//! dead. The failure mode is specifically *silent*: a snapshot that round-trips byte-for-byte can
//! still produce a machine that cannot fetch an instruction.

use arm7tdmi::Bus as _;
use eapp_loader::{EApp, Machine, Stop};

const RAM_BASE: u32 = 0x1000_0000;
const RAM_SIZE: usize = 0x10_0000;
/// The MMAP unit, where `map_hardware` puts it. Window `w` is `[w*8]` logical, `[w*8+4]` physical.
const MMAP_BASE: u32 = 0xf000_f000;
/// Where the code really lives, and where the window makes it appear.
const PHYS: u32 = RAM_BASE + 0x2000;
const LOGICAL: u32 = 0x0000_2000;

fn put32(m: &mut Machine, addr: u32, val: u32) {
    for (i, b) in val.to_le_bytes().iter().enumerate() {
        m.mem.write8(addr + i as u32, *b);
    }
}

/// A machine as the *restoring* process builds one: hardware mapped, and **no MMAP window
/// programmed**, because programming those is the firmware's job and the firmware has not run yet.
///
/// **This is the load-bearing half of the fixture.** The first version of this file restored into a
/// machine that had already programmed the window itself, so the window was present whether or not
/// the snapshot carried it — and all four tests passed with the fix reverted. A test that cannot
/// fail is worse than no test, because it is also a claim.
fn fresh_machine() -> Machine {
    let app = EApp::none();
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    eapp_loader::map_hardware(&mut m, true);
    m
}

/// A machine executing a two-instruction loop *through an MMAP window*, which is the only thing
/// about this that matters: the same loop at its physical address would survive a broken restore.
fn machine_running_through_a_window() -> Machine {
    let mut m = fresh_machine();

    // `add r0, r0, #1` then `b .-4` — a counter, so "did it run" has an observable answer rather
    // than only "did it not crash".
    put32(&mut m, PHYS, 0xe280_0001);
    put32(&mut m, PHYS + 4, 0xeaff_fffd);

    // One window: logical 0x00000000 (64 MB) -> physical 0x10000000. The encoding is the one
    // `rebuild_mmap_aliases` decodes — LOGICAL is base<31:16> | mask<13:4>, PHYSICAL is
    // base<31:16> | flags. 0x3c00 is the 64 MB mask Rockbox's `MMAP_MASK` gives.
    put32(&mut m, MMAP_BASE, 0x0000_3c00);
    put32(&mut m, MMAP_BASE + 4, 0x1000_0f84);

    m.cpu.regs[0] = 0;
    m.cpu.regs[15] = LOGICAL;
    m
}

/// The window has to work *before* the snapshot, or this test proves nothing about restoring.
#[test]
fn the_window_is_what_makes_the_low_address_executable() {
    let mut m = machine_running_through_a_window();
    assert_eq!(
        m.mem.read32(LOGICAL),
        0xe280_0001,
        "the MMAP window is not mapping {LOGICAL:#010x} onto {PHYS:#010x}, so the rest of this \
         file is testing nothing"
    );
    m.run(20);
    assert!(
        m.cpu.regs[0] >= 8,
        "the loop did not run: r0 = {}",
        m.cpu.regs[0]
    );
}

/// **The regression.** Snapshot a machine mid-loop, restore into a fresh one, and require it to
/// keep counting.
#[test]
fn a_restored_machine_can_still_fetch_an_instruction() {
    let mut a = machine_running_through_a_window();
    a.run(40);
    let counted = a.cpu.regs[0];
    assert!(counted > 0, "the source machine never ran");
    let image = a.snapshot();

    let mut b = fresh_machine();
    assert!(b.restore(&image), "the snapshot did not load");

    // The exact symptom, named where it happens: zeros at the PC.
    assert_eq!(
        b.mem.read32(b.cpu.regs[15]),
        0xe280_0001,
        "nothing is mapped at the restored PC {:#010x} — it reads {:#010x}. The memory is in the \
         image; the address map is not. This is the `mmap_regs` defect.",
        b.cpu.regs[15],
        b.mem.read32(b.cpu.regs[15])
    );

    let stop = b.run(40);
    assert!(
        b.cpu.regs[0] > counted,
        "the restored machine executed nothing: r0 stayed at {counted} ({stop:?})"
    );
    assert!(
        !matches!(stop, Stop::Lost(_)),
        "the restored machine ran off into unmapped memory: {stop:?}"
    );
}

/// The two machines must agree, not merely both be alive. A restore that lands one instruction out
/// is a different bug with the same shape, and only a comparison finds it.
#[test]
fn a_restored_machine_runs_the_same_instructions_as_the_one_it_came_from() {
    let mut a = machine_running_through_a_window();
    a.run(40);
    let image = a.snapshot();

    let mut b = fresh_machine();
    assert!(b.restore(&image));

    a.run(100);
    b.run(100);
    assert_eq!(
        b.cpu.regs[0], a.cpu.regs[0],
        "the two machines counted differently"
    );
    assert_eq!(
        b.cpu.regs[15], a.cpu.regs[15],
        "the two machines are at different PCs"
    );
    assert_eq!(
        b.executed, a.executed,
        "the two machines executed different amounts"
    );
}

/// An image from an older format must be **refused**, not misread.
///
/// The wheel block and now `mmap_regs` were both added without the reader being able to tell. A
/// field appended in the middle shifts everything after it, so an old image parsed by a new reader
/// produces a machine assembled out of the wrong words — which is worse than cold-booting, because
/// it looks like it worked.
#[test]
fn a_snapshot_from_an_older_format_is_refused() {
    let mut a = machine_running_through_a_window();
    a.run(10);
    let mut image = a.snapshot();
    assert_eq!(
        &image[..8],
        b"IPODSNP7",
        "the format tag moved; update this test with it"
    );

    image[7] = b'6';
    let mut b = fresh_machine();
    assert!(
        !b.restore(&image),
        "a v6 image was accepted by the v7 reader"
    );

    let mut truncated = a.snapshot();
    truncated.truncate(64);
    let mut c = fresh_machine();
    assert!(!c.restore(&truncated), "a truncated image was accepted");
}
