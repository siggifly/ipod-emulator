//! End-to-end: build a synthetic eApp, load it, run it on the real CPU, and check that the
//! framework calls it makes come back as a trace.
//!
//! Nothing here is stubbed at the CPU level — the game reaches the trap because a genuine
//! `bl` hit a genuine `ldr pc, [pc, #imm]` thunk whose literal the loader rewrote. That is
//! exactly the mechanism a real title will use.

use eapp_loader::{EApp, LoadError, Machine, Stop, TRAP_BASE};

const LOAD_BASE: u32 = 0x1800_0000;
const RAM_BASE: u32 = 0x1000_0000;
const RAM_SIZE: usize = 0x1_0000;

// Image layout, chosen so every offset below is checkable by hand.
const ENTRY_OFF: usize = 0x28;
const BLOCK_OFF: usize = 0x50;
const PRIMARY_OFF: usize = 0xA0;
const THUNK_A: usize = BLOCK_OFF + 0x3C;
const THUNK_B: usize = BLOCK_OFF + 0x40;

fn put32(img: &mut [u8], off: usize, val: u32) {
    img[off..off + 4].copy_from_slice(&val.to_le_bytes());
}

/// A minimal but *executable* eApp: one framework, two imports, and entry code that calls
/// import A, then B, then A again.
fn synth_eapp() -> Vec<u8> {
    let mut img = vec![0u8; 0x140];

    img[0..4].copy_from_slice(b"eapp");
    put32(&mut img, 0x04, 0x1000_1000); // version
    put32(&mut img, 0x08, 1); // one framework block
    put32(&mut img, 0x0c, ENTRY_OFF as u32); // ignored by the loader — see lib.rs
    // Real entry: absolute pointer in the vector table at +0x14.
    put32(&mut img, 0x14, LOAD_BASE + ENTRY_OFF as u32);
    // +0x10 points at the PRIMARY framework descriptor — a bare descriptor with no magic,
    // exactly as real binaries do for OpenGLES. Load-base derivation keys off it too.
    put32(&mut img, 0x10, LOAD_BASE + PRIMARY_OFF as u32);

    // ---- entry code (ARM) ----
    // push {lr}          — BL clobbers LR, so the return address must be saved first
    put32(&mut img, 0x28, 0xE92D_4000);
    // mov r0, #0x11      — a recognisable argument
    put32(&mut img, 0x2c, 0xE3A0_0011);
    // bl thunk_A         — PC reads as 0x38; (0x8c - 0x38) / 4 = 21
    put32(&mut img, 0x30, 0xEB00_0015);
    // mov r0, #0x22
    put32(&mut img, 0x34, 0xE3A0_0022);
    // bl thunk_B         — PC reads as 0x40; (0x90 - 0x40) / 4 = 20
    put32(&mut img, 0x38, 0xEB00_0014);
    // bl thunk_A         — PC reads as 0x44; (0x8c - 0x44) / 4 = 18
    put32(&mut img, 0x3c, 0xEB00_0012);
    // pop {pc}
    put32(&mut img, 0x40, 0xE8BD_8000);

    // ---- framework import block (real layout: fixed 32-byte name, hash at +0x24) ----
    img[BLOCK_OFF..BLOCK_OFF + 4].copy_from_slice(&[0x68, 0x19, 0x06, 0x29]);
    img[BLOCK_OFF + 4..BLOCK_OFF + 9].copy_from_slice(b"Audio");
    img[BLOCK_OFF + 0x24..BLOCK_OFF + 0x34].copy_from_slice(&[0xAB; 16]);
    put32(&mut img, BLOCK_OFF + 0x34, 2); // function count
    put32(&mut img, BLOCK_OFF + 0x38, 1); // non-zero pointer

    put32(&mut img, THUNK_A, 0xE59F_F000);
    put32(&mut img, THUNK_B, 0xE59F_F000);
    put32(&mut img, THUNK_A + 8, 0xDEAD_0000); // literal A — loader must overwrite this
    put32(&mut img, THUNK_B + 8, 0xDEAD_0001); // literal B

    // ---- primary framework descriptor (no magic; name at +0x00) ----
    img[PRIMARY_OFF..PRIMARY_OFF + 8].copy_from_slice(b"OpenGLES");
    img[PRIMARY_OFF + 0x20..PRIMARY_OFF + 0x30].copy_from_slice(&[0xCD; 16]);
    put32(&mut img, PRIMARY_OFF + 0x30, 0); // no imports used in this fixture
    put32(&mut img, PRIMARY_OFF + 0x34, 1); // non-zero pointer, or RetailOS rejects it

    img
}

#[test]
fn rejects_input_that_is_not_an_eapp() {
    assert_eq!(
        EApp::parse(b"definitely not an eapp image".to_vec()).unwrap_err(),
        LoadError::NotAnEApp
    );
    assert_eq!(EApp::parse(b"eapp".to_vec()).unwrap_err(), LoadError::NotAnEApp);
}

#[test]
fn parses_header_and_derives_load_base() {
    let app = EApp::parse(synth_eapp()).expect("parse");
    assert_eq!(app.load_base, LOAD_BASE, "derived, not assumed");
    assert_eq!(app.entry, LOAD_BASE + ENTRY_OFF as u32);
}

#[test]
fn discovers_frameworks_and_their_thunks() {
    let app = EApp::parse(synth_eapp()).expect("parse");
    // Primary framework first, then the magic-prefixed blocks.
    assert_eq!(app.frameworks.len(), 2);
    assert_eq!(app.frameworks[0].name, "OpenGLES");
    let fw = &app.frameworks[1];
    assert_eq!(fw.name, "Audio");
    assert_eq!(fw.thunks.len(), 2);
    assert_eq!(fw.thunks[0], LOAD_BASE + THUNK_A as u32);
    assert_eq!(fw.thunks[1], LOAD_BASE + THUNK_B as u32);
    assert_eq!(app.import_count(), 2, "OpenGLES declares none in this fixture");
}

#[test]
fn loader_rewrites_the_thunk_literals() {
    use arm7tdmi::Bus;
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);

    // The placeholder literals must be gone, replaced by distinct trap addresses.
    // Audio is framework index 1 — OpenGLES occupies index 0 — so its traps sit one stride up.
    const AUDIO_TRAPS: u32 = TRAP_BASE + 0x1000;
    assert_eq!(m.mem.read32(LOAD_BASE + THUNK_A as u32 + 8), AUDIO_TRAPS);
    assert_eq!(m.mem.read32(LOAD_BASE + THUNK_B as u32 + 8), AUDIO_TRAPS + 4);
}

#[test]
fn running_the_entry_point_produces_a_call_trace() {
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);

    let stop = m.run(1000);
    assert_eq!(stop, Stop::Returned, "entry point returned cleanly");

    assert_eq!(m.trace.len(), 3, "three calls were made");
    assert_eq!(m.trace[0].framework, "Audio");
    assert_eq!(m.trace[0].index, 0);
    assert_eq!(m.trace[0].args[0], 0x11, "r0 captured at the call");
    assert_eq!(m.trace[1].index, 1);
    assert_eq!(m.trace[1].args[0], 0x22);
    assert_eq!(m.trace[2].index, 0, "import A called a second time");
}

#[test]
fn reached_deduplicates_repeat_calls() {
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    m.run(1000);

    let reached = m.reached();
    assert_eq!(
        reached.get("Audio"),
        Some(&vec![0, 1]),
        "two distinct imports reached across three calls"
    );
    // This ratio is the whole point of B1: calls made vs surface actually used.
    assert_eq!(m.trace.len(), 3);
    assert_eq!(reached["Audio"].len(), 2);
}

#[test]
fn returning_from_a_trap_resumes_the_caller() {
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    m.run(1000);

    // Each trap recorded the address it returned to, which must be the instruction after the BL.
    assert_eq!(m.trace[0].return_to, LOAD_BASE + 0x34);
    assert_eq!(m.trace[1].return_to, LOAD_BASE + 0x3c);
    assert_eq!(m.trace[2].return_to, LOAD_BASE + 0x40);
}

#[test]
fn execution_leaving_mapped_memory_is_reported_not_silently_absorbed() {
    // Strip the `pop {pc}` so the entry point falls through into unprogrammed image space.
    // Whether it walks off the end or branches into garbage first is not the point — either
    // way the runner must name the address rather than spinning quietly.
    let mut img = synth_eapp();
    put32(&mut img, 0x40, 0xE1A0_0000); // nop, so it falls through
    let app = EApp::parse(img).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);

    match m.run(10_000) {
        Stop::Lost(pc) => {
            let in_image = (LOAD_BASE..LOAD_BASE + 0x100).contains(&pc);
            let in_ram = (RAM_BASE..RAM_BASE + RAM_SIZE as u32).contains(&pc);
            assert!(
                !in_image && !in_ram,
                "reported PC {pc:#x} should be outside every mapped region"
            );
        }
        other => panic!("expected Lost, got {other:?}"),
    }
}

#[test]
fn unmapped_data_accesses_are_recorded_as_findings() {
    let mut img = synth_eapp();
    // ldr r1, [r0] with r0 = 0x11 — nowhere near either mapped region.
    put32(&mut img, 0x2c, 0xE3A0_0011); // mov r0, #0x11
    put32(&mut img, 0x30, 0xE590_1000); // ldr r1, [r0]
    put32(&mut img, 0x34, 0xE8BD_8000); // pop {pc}
    let app = EApp::parse(img).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    m.run(1000);

    assert!(
        m.mem
            .unmapped
            .values()
            .any(|p| p.lo < 0x100 && p.reads > 0),
        "a read from unmapped space must be recorded, not silently zeroed: {:?}",
        m.mem.unmapped_report()
    );
}

/// The external memory bus's two handshakes, as Apple's bootloader uses them.
///
/// Pinned because both bits used to be `--rdval` constants, and a constant passes any test that
/// only checks a read. What matters is that the completion follows the *command*: the ROM's
/// SDRAM bring-up stages a configuration, then kicks it, then spins — so a model that reports
/// done before the kick would let a broken sequence through.
#[test]
fn the_memory_bus_completes_a_configuration_only_when_it_is_kicked() {
    use arm7tdmi::Bus as _;
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    m.mem.regions.push(eapp_loader::Region { name: "mmio-7", base: 0x7000_0000, data: vec![0; 0x100] });
    m.mem.xmb = Some(eapp_loader::Xmb::new(0x7000_0000));
    m.mem.write8(0x7000_0033, eapp_loader::Xmb::ctrl_hi_at_reset());

    // Bit 27 answers ready out of reset, and the firmware cannot clear it — the enable path waits
    // for it while bit 30 is still clear, so a bit that echoed anything would deadlock there.
    assert_eq!(m.mem.read32(0x7000_0030) & (1 << 27), 1 << 27);
    m.mem.write32(0x7000_0030, 0);
    assert_eq!(m.mem.read32(0x7000_0030) & (1 << 27), 1 << 27);

    // Bit 30 is ordinary storage: the ROM sets it around a NOR command sequence and reads it back.
    m.mem.write32(0x7000_0030, 1 << 30);
    assert_eq!(m.mem.read32(0x7000_0030) & (1 << 30), 1 << 30);
    m.mem.write32(0x7000_0030, 0);
    assert_eq!(m.mem.read32(0x7000_0030) & (1 << 30), 0);

    // Staging a configuration is not executing it.
    m.mem.write32(0x7000_003c, 0x2007_16d0);
    assert_eq!(m.mem.read32(0x7000_003c) & (1 << 31), 0, "done before the command was given");
    m.mem.write32(0x7000_003c, 0x2107_16d0);
    assert_eq!(m.mem.read32(0x7000_003c) & (1 << 31), 1 << 31, "the kick must complete");

    let x = m.mem.xmb.as_ref().unwrap();
    assert_eq!((x.gate_opens, x.gate_closes, x.ram_kicks), (1, 1, 1));
}

/// The idle task writes CPU_CTRL's sleep bit in a tight loop, so an unmodelled write made the
/// resting machine spin at full speed and turned "instructions executed" into a measure of how
/// long we idled rather than how much work happened. Sleeping has to move the clock **without**
/// moving the instruction count, or every profile built on that count measures the wrong thing.
#[test]
fn sleeping_advances_the_clock_and_not_the_instruction_count() {
    use arm7tdmi::Bus as _;
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    m.mem.regions.push(eapp_loader::Region { name: "mmio-6", base: 0x6000_0000, data: vec![0; 0x8000] });

    // Only bit 31 is the sleep request; the register carries other fields the firmware writes.
    m.mem.write32(eapp_loader::CPU_CTRL, 0x0000_0001);
    assert!(!m.mem.cpu_sleep, "a write without bit 31 is not a sleep request");
    m.mem.write32(eapp_loader::CPU_CTRL, 0x8000_0000);
    assert!(m.mem.cpu_sleep);

    // With nothing armed there is no wake-up to skip to. Inventing time here would be inventing an
    // external event we have no model of, so the request is simply consumed.
    m.mem.cpu_sleep = false;
    assert_eq!((m.mem.slept_usec, m.mem.sleeps), (0, 0));
}

/// A machine with the two MMIO regions the click wheel needs — the block it lives in, and the one
/// holding the interrupt controller and GPIOA.
fn wheel_machine() -> Machine {
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    m.mem.regions.push(eapp_loader::Region { name: "mmio-6", base: 0x6000_0000, data: vec![0; 0x1_0000] });
    m.mem.regions.push(eapp_loader::Region { name: "mmio-6d", base: 0x6000_d000, data: vec![0; 0x1000] });
    m.mem.regions.push(eapp_loader::Region { name: "mmio-7", base: 0x7000_0000, data: vec![0; 0x1_0000] });
    m.mem.clickwheel = Some(eapp_loader::ClickWheel::new(0x7000_c000));
    m
}

/// Apple's opto init, in the three stores that matter: `0x00283e20` writes `0x8001052a` to TX and
/// starts a transmit through `0x00283fa0`, and *that* is what turns autonomous reporting on.
///
/// Spelled out here rather than hidden in `wheel_machine()` because a wheel that reports before it
/// has been told to is precisely the thing this model used to get wrong for free. Every test below
/// that injects a scripted event calls this first, in the same order the firmware does.
fn enable_reporting(m: &mut Machine) {
    use arm7tdmi::Bus as _;
    m.mem.write32(0x7000_c104, 0x0c00_0000);
    m.mem.write32(0x7000_c120, 0x8001_052a);
    m.mem.write32(0x7000_c100, 0x8000_0000);
    // ...and the re-arm, which this helper used to stop short of. `0x00283fa0` acknowledges with
    // `0x0c000000` to STATUS and then ORs `0x60000000` into CTRL — the receiver has to be armed or
    // there is nowhere for a frame to land. It was omissible only while the model gated the stream
    // on `0x052a` alone; now that the gate is the pair (reporting AND armed), which is what the
    // interrupt below was always gated on, the helper has to be the whole sequence.
    m.mem.write32(0x7000_c100, 0x6000_0000);
    assert!(m.mem.clickwheel.as_ref().unwrap().reporting, "the enable did not take");
}

/// Apple's own query sequence, register for register — including the acknowledgement that comes
/// *after* the transmit and would destroy a reply that arrived too early.
///
/// `0x00283fa0` writes TX, sets CTRL bit 31, spins on STATUS bit 31, then writes `0x0c000000` to
/// STATUS and re-arms; only then does its caller start polling receive-ready with a 1500 µs
/// timeout. Replying inside the store to CTRL is therefore *wrong*, and wrong in a way that reads
/// as a working device: the frame is posted, the driver's own ack wipes it, and every query times
/// out and reads a stale word. That is what the first version of this model did, measured on a real
/// boot — 3 word reads of DATA, none of them with a frame waiting.
///
/// The frame is checked against both drivers' masks, because they differ: RetailOS tests
/// `(frame & 0x8000ffff) == 0x8000023a` at `0x002813c4`, Rockbox tests `(frame & 0x800000ff) !=
/// 0x8000003a` before deciding to resynchronise. A word that satisfied one and not the other would
/// pass a unit test and fail on the machine.
///
/// The refusal half carries its own positive control, matched in width, register and code path: an
/// unrecognised command must post nothing, and the very next transmit of the recognised one — same
/// three stores, same registers — must still post, so "nothing happened" cannot be the device
/// having died rather than having refused.
#[test]
fn the_wheel_answers_the_command_retailos_sends_and_refuses_ones_it_does_not() {
    use arm7tdmi::Bus as _;
    use eapp_loader::*;
    let mut m = wheel_machine();
    // Deliver a reply that is due: the sender's ack runs first, then the caller polls.
    let settle = |m: &mut Machine| {
        m.mem.usec = m.mem.usec.wrapping_add(OPTO_REPLY_USEC);
        m.service_interrupts();
    };

    m.mem.write32(0x7000_c104, 0x0c00_0000);
    m.mem.write32(0x7000_c120, 0x8000_023a);
    m.mem.write32(0x7000_c100, 0x8000_0000);
    // 0x00283ffc spins while STATUS bit 31 is set. A transmit that completes inside the store is
    // never busy, so that spin must fall through immediately or the driver sits out its timeout.
    assert_eq!(m.mem.read32(0x7000_c104) & 0x8000_0000, 0, "transmit reported still busy");
    assert_eq!(
        m.mem.read32(0x7000_c104) & 0x0400_0000,
        0,
        "the reply arrived before the driver had finished sending"
    );
    // 0x00284054: the sender's own acknowledgement, which must not be able to lose the answer.
    m.mem.write32(0x7000_c104, 0x0c00_0000);
    settle(&mut m);
    assert_ne!(m.mem.read32(0x7000_c104) & 0x0400_0000, 0, "the reply never arrived");

    let frame = m.mem.read32(0x7000_c140);
    assert_eq!(frame & 0x8000_ffff, 0x8000_023a, "RetailOS's check at 0x002813c4");
    assert_ne!(frame & 0x8000_00ff, 0x8000_001a, "must not also read as the streaming frame");
    assert_eq!(frame >> 16 & 0x1f, 0, "no button is held, so every button bit is clear");

    // Hold a button and ask again. RetailOS takes bits 21:16 (`lsl #10; lsr #26`) as the answer.
    m.mem.clickwheel.as_mut().unwrap().buttons = WHEEL_SELECT | WHEEL_MENU;
    m.mem.write32(0x7000_c104, 0x0c00_0000);
    m.mem.write32(0x7000_c100, 0);
    m.mem.write32(0x7000_c100, 0x8000_0000);
    settle(&mut m);
    let frame = m.mem.read32(0x7000_c140);
    assert_eq!(frame >> 16 & 0x1f, (WHEEL_SELECT | WHEEL_MENU) as u32);

    // An unrecognised command answers nothing at all, and says so.
    let before = m.mem.clickwheel.as_ref().unwrap().frames_posted;
    m.mem.write32(0x7000_c104, 0x0c00_0000);
    m.mem.write32(0x7000_c120, 0x1234_5678);
    m.mem.write32(0x7000_c100, 0);
    m.mem.write32(0x7000_c100, 0x8000_0000);
    settle(&mut m);
    let w = m.mem.clickwheel.as_ref().unwrap();
    assert_eq!(w.frames_posted, before, "an unmodelled command was given an invented reply");
    assert_eq!(w.unknown.sample(), &[0x1234_5678]);
    assert_eq!(w.unknown.seen(), 1);
    assert_eq!(m.mem.read32(0x7000_c104) & 0x0400_0000, 0, "receive-ready with nothing received");

    // The control: the same stores with the command we do model still post.
    m.mem.write32(0x7000_c120, 0x8000_023a);
    m.mem.write32(0x7000_c100, 0);
    m.mem.write32(0x7000_c100, 0x8000_0000);
    settle(&mut m);
    assert_eq!(m.mem.clickwheel.as_ref().unwrap().frames_posted, before + 1);
}

/// `0x052a` is a **write**, and its correct answer is silence — with the gate that makes the claim
/// falsifiable instead of merely convenient.
///
/// The derivation is in `ClickWheel::transmit`'s doc comment; what is asserted here is its three
/// consequences, each with the matched control that separates "refused" from "dead":
///
/// 1. The command posts **no frame** — and the recognised query, run immediately after through the
///    same three stores, still does. Silence has to be attributable to the opcode, not to a device
///    that stopped answering.
/// 2. It is **not** counted as a command we have no evidence for, which is what it was until the
///    boot ROM's copy at `0x000c9714` — write TX, start, spin 10 000 iterations, return, never read
///    `0x7000c140` — settled it.
/// 3. Autonomous frames are gated on the payload. Before the enable they are refused and counted;
///    after it they are posted; after `0x8000052a` they are refused again. Same script, same
///    machine, three different answers, and the only thing that moved is the command word.
#[test]
fn the_set_reporting_command_is_answered_with_silence_and_gates_the_stream() {
    use arm7tdmi::Bus as _;
    use eapp_loader::*;
    let mut m = wheel_machine();

    // (3a) Reporting is off out of reset: an injected event is refused, not posted.
    m.mem.clickwheel.as_mut().unwrap().script = parse_wheel_script("@0:touch", 10).expect("script");
    m.mem.icount = 0;
    m.service_interrupts();
    {
        let w = m.mem.clickwheel.as_ref().unwrap();
        assert_eq!(w.frames_posted, 0, "a wheel nobody armed reported anyway");
        assert_eq!(w.frames_suppressed, 1);
        assert_eq!(m.mem.read32(0x7000_c104) & 0x0400_0000, 0, "receive-ready with nothing received");
    }

    // (1) + (2) The enable itself: Apple's three stores, and no reply to them.
    enable_reporting(&mut m);
    m.mem.usec = m.mem.usec.wrapping_add(OPTO_REPLY_USEC * 4);
    m.service_interrupts();
    {
        let w = m.mem.clickwheel.as_ref().unwrap();
        assert_eq!(w.frames_posted, 0, "0x052a was given an invented reply");
        assert_eq!(w.set_commands, 1);
        assert_eq!(w.last_set.map(|(_, v)| v), Some(1));
        assert_eq!(w.unknown_commands, 0, "0x052a is evidence-backed, not unknown");
        assert!(w.unknown.is_empty());
    }

    // (3b) The same script step, now that the wheel has been told to report.
    m.mem.clickwheel.as_mut().unwrap().script = parse_wheel_script("@100:rotate=+1", 10).expect("script");
    m.mem.clickwheel.as_mut().unwrap().next = 0;
    m.mem.icount = 100;
    m.service_interrupts();
    {
        let w = m.mem.clickwheel.as_ref().unwrap();
        assert_eq!(w.frames_posted, 1, "an armed wheel refused to report");
        assert_eq!(w.frames_suppressed, 1, "and nothing new was refused");
        assert_eq!(w.log.sample().last().unwrap().1 & 0xbc00_00ff, 0x8000_001a);
    }

    // (3c) `0x8000052a` — same opcode, payload 0 — turns it back off. This is the arm that proves
    // the gate is a gate: `0x000b2ce0` chooses between exactly these two words.
    m.mem.write32(0x7000_c104, 0x0c00_0000);
    m.mem.write32(0x7000_c120, 0x8000_052a);
    m.mem.write32(0x7000_c100, 0);
    m.mem.write32(0x7000_c100, 0x8000_0000);
    assert!(!m.mem.clickwheel.as_ref().unwrap().reporting, "payload 0 did not turn reporting off");
    m.mem.clickwheel.as_mut().unwrap().script = parse_wheel_script("@200:rotate=+1", 10).expect("script");
    m.mem.clickwheel.as_mut().unwrap().next = 0;
    m.mem.icount = 200;
    m.service_interrupts();
    {
        let w = m.mem.clickwheel.as_ref().unwrap();
        assert_eq!(w.frames_posted, 1, "a disabled wheel reported");
        assert_eq!(w.frames_suppressed, 2);
        assert_eq!(w.set_commands, 2);
        assert_eq!(w.last_set.map(|(_, v)| v), Some(0));
        assert_eq!(w.unknown_commands, 0);
    }

    // The control for the whole test: the one command that *is* a question still answers, in the
    // same machine, on the very next transmit — so every zero above is a refusal and not a corpse.
    m.mem.write32(0x7000_c104, 0x0c00_0000);
    m.mem.write32(0x7000_c120, 0x8000_023a);
    m.mem.write32(0x7000_c100, 0);
    m.mem.write32(0x7000_c100, 0x8000_0000);
    m.mem.write32(0x7000_c104, 0x0c00_0000);
    m.mem.usec = m.mem.usec.wrapping_add(OPTO_REPLY_USEC);
    m.service_interrupts();
    assert_ne!(m.mem.read32(0x7000_c104) & 0x0400_0000, 0, "the control query never answered");
    assert_eq!(m.mem.read32(0x7000_c140) & 0x8000_ffff, 0x8000_023a);
}

/// An injected rotation, from the parsed script through the tick to the frames the firmware would
/// read — and the wrap at 96 clicks, in both directions.
///
/// The frames are checked against **both** decoders' masks for the same reason as above: Apple's
/// `and r12, r0, #0xbc0000ff; cmp r12, #0x8000001a` is strictly stronger than Rockbox's
/// `(status & 0x800000ff) == 0x8000001a`, so a frame with a stray bit in 29..26 would pass the
/// published check and fail on the machine this emulator runs.
#[test]
fn a_scripted_rotation_posts_one_frame_per_click_and_wraps_at_96() {
    use eapp_loader::*;
    let mut m = wheel_machine();
    enable_reporting(&mut m);
    let steps = parse_wheel_script("@100:touch,+50:rotate=+3,+50:release", 10).expect("script");
    // touch, three clicks 10 apart, release 50 after the last of them.
    assert_eq!(steps.len(), 5);
    assert_eq!(steps.iter().map(|s| s.at).collect::<Vec<_>>(), vec![100, 150, 160, 170, 220]);
    m.mem.clickwheel.as_mut().unwrap().script = steps;
    m.mem.clickwheel.as_mut().unwrap().position = 95;

    for n in (0..=240).step_by(10) {
        m.mem.icount = n;
        m.service_interrupts();
    }

    let w = m.mem.clickwheel.as_ref().unwrap();
    assert_eq!(w.next, 5, "every step must fire");
    let frames: Vec<u32> = w.log.iter().map(|&(_, f)| f).collect();
    assert_eq!(frames.len(), 5);
    for f in &frames {
        assert_eq!(f & 0xbc00_00ff, 0x8000_001a, "RetailOS's mask at 0x00281370");
        assert_eq!(f & 0x8000_00ff, 0x8000_001a, "Rockbox's mask");
    }
    // Position wraps 95 -> 0 -> 1 -> 2; the touch flag follows the finger.
    let pos: Vec<u32> = frames.iter().map(|f| f >> 16 & 0x7f).collect();
    assert_eq!(pos, vec![95, 0, 1, 2, 2]);
    let touched: Vec<bool> = frames.iter().map(|f| f & 0x4000_0000 != 0).collect();
    assert_eq!(touched, vec![true, true, true, true, false], "the release frame is what ends a scroll");
    // Nothing read them, so every frame after the first overwrote an unread one. Counting that is
    // the only way an injected sequence the driver kept up with is distinguishable from one it did not.
    assert_eq!(w.frames_dropped, 4);

    // Anticlockwise wraps the other way.
    let mut m = wheel_machine();
    enable_reporting(&mut m);
    m.mem.clickwheel.as_mut().unwrap().script = parse_wheel_script("@0:rotate=-2", 10).expect("script");
    for n in (0..=20).step_by(10) {
        m.mem.icount = n;
        m.service_interrupts();
    }
    let pos: Vec<u32> = m.mem.clickwheel.as_ref().unwrap().log.iter().map(|&(_, f)| f >> 16 & 0x7f).collect();
    assert_eq!(pos, vec![95, 94]);
}

/// Receive-ready is write-1-to-clear and the interrupt line follows it, gated on the receiver
/// having been armed.
///
/// Both drivers acknowledge by *setting* the bit — RetailOS `orr r0, r0, #0x4000000` at
/// `0x002813e4`, Rockbox `outl(inl(0x7000c104) | 0x0c000000, 0x7000c104)`. Modelled as ordinary
/// storage that write puts the flag back up, and the handler re-enters forever.
///
/// The arm gate carries the control: with the receiver disarmed the same script step must produce
/// **nothing at all**, and both the flag and the line must appear once it is armed — so "no
/// interrupt" is attributable to the gate rather than to a model that never raises one.
///
/// That control changed shape on 2026-08-18 and the reason is worth stating. It used to post the
/// frame while unarmed and withhold only the interrupt. But "armed" is the receiver being ready to
/// accept a frame; an unarmed one has nowhere for the frame to land, so the delivery does not
/// happen and there is nothing to raise later. The gate now sits on the post as well as on the
/// line, which is also what makes Rockbox work: it arms with `0xc00a1f00` and never sends the
/// `0x052a` this model used to demand.
#[test]
fn receive_ready_is_write_one_to_clear_and_the_line_follows_it() {
    use arm7tdmi::Bus as _;
    use eapp_loader::*;
    let mut m = wheel_machine();
    enable_reporting(&mut m);
    let line = 1u32 << OPTO_IRQ_HI;

    // Disarm, and confirm a frame is not delivered at all.
    m.mem.write32(0x7000_c100, 0x0000_0000);
    m.mem.clickwheel.as_mut().unwrap().script = parse_wheel_script("@0:touch", 10).expect("script");
    m.mem.icount = 0;
    m.service_interrupts();
    assert_eq!(
        m.mem.read32(0x7000_c104) & 0x0400_0000,
        0,
        "an unarmed receiver accepted a frame"
    );
    assert_eq!(m.mem.int_pending_hi & line, 0, "an unarmed receiver must not interrupt");
    assert_eq!(m.mem.clickwheel.as_ref().unwrap().frames_posted, 0);

    // 0x002813f0 / Rockbox's ISR tail: arm the receiver, then let the next step through.
    m.mem.write32(0x7000_c100, 0x6000_0000);
    m.mem.clickwheel.as_mut().unwrap().script = parse_wheel_script("@1:touch", 10).expect("script");
    m.mem.clickwheel.as_mut().unwrap().next = 0;
    m.mem.icount = 1;
    m.service_interrupts();
    assert_ne!(m.mem.read32(0x7000_c104) & 0x0400_0000, 0, "the frame did not set receive-ready");
    assert_eq!(m.mem.int_pending_hi & line, line, "arming did not raise the pending frame's line");
    assert_eq!(m.mem.clickwheel.as_ref().unwrap().irqs, 1);

    // Delivery is not acknowledgement — the line is a level, so it survives until the flag is cleared.
    m.service_interrupts();
    assert_eq!(m.mem.int_pending_hi & line, line);

    m.mem.write32(0x7000_c104, 0x0400_0000);
    assert_eq!(m.mem.read32(0x7000_c104) & 0x0400_0000, 0, "write-1-to-clear did not clear");
    m.service_interrupts();
    assert_eq!(m.mem.int_pending_hi & line, 0, "the line survived its acknowledgement");
}

/// Hold reaches the line `button_hold()` actually reads, not only the frame's bit 31.
///
/// GPIOA bit 0x20 is active low — Rockbox `return (GPIOA_INPUT_VAL & 0x20) ? false : true` — and
/// `map_hardware` seeds it *set* for a bare iPod. A hold event that moved only the frame bit would
/// leave the emulator reporting a wheel that has gone quiet next to a switch that never moved.
#[test]
fn engaging_hold_moves_both_the_frame_bit_and_the_gpio_line() {
    use arm7tdmi::Bus as _;
    use eapp_loader::*;
    let mut m = wheel_machine();
    enable_reporting(&mut m);
    m.mem.write32(GPIOA_INPUT_VAL, GPIOA_HOLD); // hold off, as map_hardware leaves it
    m.mem.clickwheel.as_mut().unwrap().script =
        parse_wheel_script("@0:touch,@10:hold,@20:unhold", 10).expect("script");

    m.mem.icount = 0;
    m.service_interrupts();
    assert_ne!(m.mem.clickwheel.as_ref().unwrap().log.sample()[0].1 & 0x8000_0000, 0, "bit 31 set with hold off");
    assert_eq!(m.mem.read32(GPIOA_INPUT_VAL) & GPIOA_HOLD, GPIOA_HOLD);

    m.mem.icount = 10;
    m.service_interrupts();
    let f = m.mem.clickwheel.as_ref().unwrap().log.sample()[1].1;
    assert_eq!(f & 0x8000_0000, 0, "hold must clear frame bit 31");
    assert_ne!(f & 0x8000_00ff, 0x8000_001a, "and so must fail both drivers' frame checks");
    assert_eq!(m.mem.read32(GPIOA_INPUT_VAL) & GPIOA_HOLD, 0, "hold is active low on GPIOA");

    m.mem.icount = 20;
    m.service_interrupts();
    assert_eq!(m.mem.read32(GPIOA_INPUT_VAL) & GPIOA_HOLD, GPIOA_HOLD, "releasing hold must restore it");
}

/// A script means exactly what the run prints, and a step it cannot parse stops the run.
///
/// The expansion is the reproducibility guarantee: `rotate` and `press` become their individual
/// steps here, so the schedule in a log can be pasted back into `--wheel=` and produce the same run.
#[test]
fn a_script_expands_to_the_steps_it_prints_and_a_bad_one_is_refused() {
    use eapp_loader::*;
    let s = parse_wheel_script("@1000:press=select", 25).expect("script");
    assert_eq!(s.len(), 2);
    assert_eq!(s[0], WheelStep::instr(1000, WheelEvent::Button(WHEEL_SELECT, true)));
    assert_eq!(s[1], WheelStep::instr(1025, WheelEvent::Button(WHEEL_SELECT, false)));
    assert_eq!(wheel_step_name(s[0].event), "down=select");

    // `+N` is relative to the previous step's *last* expanded click, so a sequence stays in order.
    let s = parse_wheel_script("@0:rotate=+3,+5:touch", 10).expect("script");
    assert_eq!(s.iter().map(|x| x.at).collect::<Vec<_>>(), vec![0, 10, 20, 25]);

    // Suffixes and separators, because instruction counts in this project are eight digits long.
    assert_eq!(parse_wheel_script("@49_700k:touch", 1).unwrap()[0].at, 49_700_000);
    assert_eq!(parse_wheel_script("@50M:touch", 1).unwrap()[0].at, 50_000_000);

    for bad in ["50:touch", "@50:wiggle", "@50:down=knob", "@50", "", "@50:rotate=0"] {
        assert!(parse_wheel_script(bad, 10).is_err(), "{bad:?} should not parse");
    }
}

/// The PP502x DMA controller at `0x60008000`/`0x60009000` — the transfer engine RetailOS's
/// `APPLEBOOT` points at `vmcs.bin`, and the reason that task blocked forever.
///
/// Everything asserted here is a bit some published or measured source names: the register offsets
/// and command bits are Rockbox `pp5020.h`; the `size - 4` encoding is `DMA0_CMD = CONFIG |
/// (size - 4) | DMA_CMD_START` in its `pcm-pp.c` and `sub r2, r3, #0x4` at RetailOS `0x0028dff8`;
/// the read-to-clear latch is `DMA0_STATUS; /* Clear any pending interrupt */`, the first
/// statement of Rockbox's FIQ handler.
#[test]
fn pp_dma_moves_bytes_to_a_fixed_port_and_posts_a_read_to_clear_completion() {
    use arm7tdmi::Bus as _;
    use eapp_loader::*;
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    m.mem.regions.push(eapp_loader::Region { name: "mmio-6", base: 0x6000_0000, data: vec![0; 0x1_0000] });

    let c = &PP_DMA[0];
    let ch = c.chans; // channel 0
    let (src, dst) = (RAM_BASE, RAM_BASE + 0x100);
    const LEN: u32 = 0x20;
    for i in 0..LEN / 4 {
        m.mem.write32(src + i * 4, 0x1000 + i);
    }

    m.mem.write32(c.master, DMA_MASTER_CONTROL_EN);
    m.mem.write32(ch + DMA_RAM_ADDR, src);
    m.mem.write32(ch + DMA_PER_ADDR, dst);
    m.mem.write32(
        ch + DMA_CMD,
        DMA_CMD_RAM_TO_PER | DMA_CMD_SINGLE | DMA_CMD_INTR | (LEN - 4) | DMA_CMD_START,
    );

    // A channel whose controller is disabled must not run: RetailOS clears every channel long
    // before it enables either master, and a model that ran them would fire on the clearing pass.
    m.mem.write32(c.master, 0);
    m.service_interrupts();
    assert_eq!(m.mem.pp_dma_transfers, 0, "a disabled controller ran a transfer");
    m.mem.write32(c.master, DMA_MASTER_CONTROL_EN);

    m.service_interrupts();
    assert_eq!((m.mem.pp_dma_transfers, m.mem.pp_dma_bytes), (1, LEN as u64));

    // The peripheral side is a port, not a window. Every word landed on the same address, so what
    // survives is the *last* one — which is precisely what distinguishes a fixed destination from
    // an incrementing one, and what lets RetailOS's chunk loop re-use one address for all four.
    assert_eq!(m.mem.read32(dst), 0x1000 + LEN / 4 - 1);
    assert_eq!(m.mem.read32(dst + 4), 0, "the peripheral address advanced");

    // START drops on completion (`DMA_CMD_SINGLE` is "stop on complete, no auto reload") but the
    // size stays: Rockbox reads it back out of CMD to work out how much of the buffer went.
    let cmd = m.mem.read32(ch + DMA_CMD);
    assert_eq!(cmd & DMA_CMD_START, 0);
    assert_eq!((cmd & DMA_SIZE_MASK) + 4, LEN);
    assert_eq!(m.mem.read32(c.master + DMA_MASTER_STATUS), 1 << DMA_MASTER_STATUS_CH0);
    assert_eq!(m.mem.int_pending >> c.irq & 1, 1, "the completion line is not asserted");

    // Read-to-clear, and the reader still sees the bit. That this assertion has to come *first* is
    // the point: an earlier `read32(STATUS)` here consumed the latch and made the next line fail,
    // which is the model behaving correctly and the test asking the wrong question twice.
    //
    // A word load reaches the bus as four byte loads, so the clear is deferred to byte 3 — the one
    // that actually carries bit 30. Clearing on byte 0 would hand the handler a status with the
    // bit already gone, and RetailOS's ISR would dispatch to nothing.
    assert_eq!(m.mem.read32(ch + DMA_STATUS), DMA_STATUS_INTR | (LEN - 4));
    assert_eq!(m.mem.read32(ch + DMA_STATUS) & DMA_STATUS_INTR, 0, "the latch was not read-to-clear");
    m.service_interrupts();
    assert_eq!(m.mem.int_pending >> c.irq & 1, 0, "the line survived its acknowledgement");

    // RetailOS's DMA ISR ends by raising a software interrupt so the completion callback runs at
    // task level — `INT_FORCED_SET = 1 << 13` at 0x001fc840. Unmodelled, the ISR fires, posts, and
    // nothing ever collects: the upload stops dead after its first 64 KB chunk.
    m.mem.write32(0x6000_4024, 1 << 13); // CPU_INT_EN
    m.mem.write32(0x6000_4018, 1 << 13); // INT_FORCED_SET
    m.service_interrupts();
    assert_eq!(m.mem.read32(0x6000_4000) >> 13 & 1, 1, "a forced interrupt did not reach CPU_INT_STAT");
    m.mem.write32(0x6000_401c, 1 << 13); // INT_FORCED_CLR
    m.service_interrupts();
    assert_eq!(m.mem.read32(0x6000_4000) >> 13 & 1, 0, "a forced interrupt could not be retired");
}

// ---------------------------------------------------------------- video co-processor

const BCM: u32 = 0x3000_0000;
const BCM_DATA: u32 = BCM;
const BCM_WR_ADDR: u32 = BCM + 0x1_0000;
const BCM_RD_ADDR: u32 = BCM + 0x2_0000;

fn bcm_machine() -> Machine {
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    m.mem.bcm = Some(eapp_loader::Bcm::new(BCM));
    m
}

/// A halfword access to the co-processor, as the interpreter issues it: two byte accesses.
fn bcm_put16(m: &mut Machine, v: u16) {
    use arm7tdmi::Bus;
    m.mem.write8(BCM_DATA, v as u8);
    m.mem.write8(BCM_DATA + 1, (v >> 8) as u8);
}
fn bcm_get16(m: &mut Machine) -> u16 {
    use arm7tdmi::Bus;
    let lo = m.mem.read8(BCM_DATA) as u16;
    let hi = m.mem.read8(BCM_DATA + 1) as u16;
    lo | (hi << 8)
}
fn bcm_get32(m: &mut Machine) -> u32 {
    let lo = bcm_get16(m) as u32;
    let hi = bcm_get16(m) as u32;
    lo | (hi << 16)
}

/// Latch an internal address, the way the host does: the same register twice, low half first.
fn bcm_seek(m: &mut Machine, addr: u32, write: bool) {
    use arm7tdmi::Bus;
    let reg = if write { BCM_WR_ADDR } else { BCM_RD_ADDR };
    for h in [addr as u16, (addr >> 16) as u16] {
        m.mem.write8(reg, h as u8);
        m.mem.write8(reg + 1, (h >> 8) as u8);
    }
}

/// The data port is a FIFO, and one halfword the host asks for must cost exactly one halfword.
///
/// The interpreter decomposes `ldrh` into two byte reads. Serving each from a fresh fetch drew
/// TWO internal halfwords per halfword delivered and spliced the low byte of one with the high
/// byte of the next — so RetailOS's 16-byte read at internal `0x1f0` drained `0x1f0..0x20f` and
/// handed back word 2 as `0x2f01fc78` instead of `1`, and the display never came up.
///
/// The positive control is the write direction, matched in width, port and code path: it already
/// buffered the pair, so a round trip through both must return what went in.
#[test]
fn a_halfword_read_from_the_co_processor_consumes_exactly_one_internal_halfword() {
    let mut m = bcm_machine();

    bcm_seek(&mut m, 0x1f0, true);
    for v in [1u16, 2, 3, 4, 5, 6, 7, 8] {
        bcm_put16(&mut m, v);
    }
    {
        let held = &m.mem.bcm.as_ref().unwrap().mem;
        assert_eq!(held.get(&0x1f0), Some(&1), "the write side did not land where addressed");
        assert_eq!(held.get(&0x1fe), Some(&8), "eight halfwords in must occupy eight halfwords");
    }

    bcm_seek(&mut m, 0x1f0, false);
    let got: Vec<u16> = (0..8).map(|_| bcm_get16(&mut m)).collect();
    assert_eq!(got, vec![1, 2, 3, 4, 5, 6, 7, 8], "the read FIFO advanced twice per halfword");

    bcm_seek(&mut m, 0x1f0, false);
    let words: Vec<u32> = (0..4).map(|_| bcm_get32(&mut m)).collect();
    assert_eq!(words, vec![0x0002_0001, 0x0004_0003, 0x0006_0005, 0x0008_0007]);
}

/// The registry RetailOS looks for, as its own reader defines it.
///
/// `FUN_00288058` requires `[0x1f8] == 1` and `[0x1fc]` non-zero and 4-aligned; `FUN_00286aa8`
/// walks eight `u16` slots at `[0x1fc]`, reads 16 bytes at `base + slot`, and matches a `u16` at
/// record `+4` against 2; `FUN_002882c0` pulls 0x50 bytes down. Nothing here is tuned — every
/// assertion is one of those tests, restated.
#[test]
fn the_co_processor_publishes_a_service_directory_its_reader_would_accept() {
    let mut m = bcm_machine();
    m.mem.bcm.as_mut().unwrap().registry = true;

    // Negative control: before the firmware is started the block is zero, which is what the
    // uploaded image genuinely contains at that offset.
    bcm_seek(&mut m, 0x1f0, false);
    assert_eq!((0..4).map(|_| bcm_get32(&mut m)).collect::<Vec<_>>(), vec![0, 0, 0, 0]);

    // The host starts the firmware exactly as Rockbox's `bcm_init` does.
    bcm_seek(&mut m, 0x1000_0400, true);
    bcm_put16(&mut m, 0x0002);
    bcm_put16(&mut m, 0xa5a5);

    bcm_seek(&mut m, 0x1f0, false);
    let hdr: Vec<u32> = (0..4).map(|_| bcm_get32(&mut m)).collect();
    assert_eq!(hdr[2], 1, "FUN_00288058 requires word 2 to be exactly 1");
    assert_ne!(hdr[3], 0, "the directory pointer must be non-zero");
    assert_eq!(hdr[3] & 3, 0, "the directory pointer must be 4-byte aligned");

    // Eight u16 slots. A zero slot means "no service" and is skipped.
    let base = hdr[3];
    bcm_seek(&mut m, base, false);
    let slots: Vec<u16> = (0..8).map(|_| bcm_get16(&mut m)).collect();
    assert_ne!(slots[0], 0, "no service was published");
    assert!(slots[1..].iter().all(|&s| s == 0), "only the display service is modelled");

    // The record, read the way FUN_002882c0 reads it: 0x50 bytes at base + slot.
    bcm_seek(&mut m, base + slots[0] as u32, false);
    let rec: Vec<u16> = (0..0x28).map(|_| bcm_get16(&mut m)).collect();
    assert_eq!(rec[2], 2, "record +0x04 is the tag, and the display service is tag 2");
    let (tx_lo, tx_hi, rx_lo, rx_hi) = (rec[3], rec[4], rec[5], rec[6]);
    assert!(tx_lo < tx_hi && rx_lo < rx_hi, "a ring must have a non-empty span");
    assert_eq!(rec[8], tx_lo, "+0x10 is the TX read pointer, and starts at the ring base");
    assert_eq!(rec[0x10], tx_lo, "+0x20 is the TX write pointer");
    assert_eq!(rec[0x18], rx_lo, "+0x30 is the RX read pointer");
    assert_eq!(rec[0x20], rx_lo, "+0x40 is the RX write pointer");
}

/// A request pushed into the ring is answered with a header the reply parser accepts.
///
/// `FUN_002872fc` rejects any reply whose first word is not `0xf1a55a1f`; `FUN_00286ca8` reads
/// exactly `0x20` bytes and takes `+0x10` as the handle and `+0x14` as the co-processor address.
/// The control is the *absence* of a reply until the doorbell is rung — writing the message into
/// the ring alone must produce nothing, so "it answered" cannot be the model answering anything
/// that lands in memory.
#[test]
fn a_request_written_into_the_ring_is_answered_only_once_the_doorbell_is_rung() {
    let mut m = bcm_machine();
    m.mem.bcm.as_mut().unwrap().registry = true;
    bcm_seek(&mut m, 0x1000_0400, true);
    bcm_put16(&mut m, 0x0002);
    bcm_put16(&mut m, 0xa5a5);

    bcm_seek(&mut m, 0x1f0, false);
    let base = (0..4).map(|_| bcm_get32(&mut m)).collect::<Vec<_>>()[3];
    bcm_seek(&mut m, base, false);
    let rec = base + bcm_get16(&mut m) as u32;
    bcm_seek(&mut m, rec + 6, false);
    let tx_lo = bcm_get16(&mut m) as u32;
    bcm_seek(&mut m, rec + 0xa, false);
    let rx_lo = bcm_get16(&mut m) as u32;

    // Header + a 0x20-byte payload for opcode 8: 320x240, pitch 640, address 0 = allocate.
    let mut msg = [0u8; 0x30];
    msg[0..4].copy_from_slice(&0xf1a5_5a1fu32.to_le_bytes());
    msg[4..8].copy_from_slice(&7u32.to_le_bytes()); // sequence
    msg[8..12].copy_from_slice(&8u32.to_le_bytes()); // opcode
    msg[12..14].copy_from_slice(&0x20u16.to_le_bytes());
    msg[0x18..0x1c].copy_from_slice(&320u32.to_le_bytes());
    msg[0x1c..0x20].copy_from_slice(&240u32.to_le_bytes());
    msg[0x20..0x24].copy_from_slice(&640u32.to_le_bytes());
    bcm_seek(&mut m, base + tx_lo, true);
    for c in msg.chunks(2) {
        bcm_put16(&mut m, c[0] as u16 | ((c[1] as u16) << 8));
    }
    assert_eq!(m.mem.bcm.as_ref().unwrap().gencmd.len(), 0, "answered before the doorbell");

    // The doorbell is the 16-byte block at record +0x20, whose first halfword is the pointer.
    bcm_seek(&mut m, rec + 0x20, true);
    bcm_put16(&mut m, (tx_lo + 0x30) as u16);

    {
        let b = m.mem.bcm.as_ref().unwrap();
        assert_eq!(b.gencmd, vec![(8, 0x20)], "the request was not decoded");
        assert_eq!(b.gencmd_dropped, 0);
    }

    // The reply, read the way FUN_00286ca8 reads it.
    bcm_seek(&mut m, base + rx_lo, false);
    let r: Vec<u32> = (0..8).map(|_| bcm_get32(&mut m)).collect();
    assert_eq!(r[0], 0xf1a5_5a1f, "FUN_002872fc would reject this reply");
    assert_eq!(r[1], 7, "the sequence must come back");
    assert_eq!(r[2], 8, "the opcode must come back");
    assert_eq!(r[3] & 0xffff, 0x10, "the payload length must be the 16 bytes every caller reads");
    assert_ne!(r[4], 0, "reply +0x10 is the handle FUN_00286ca8 stores");
    assert_ne!(r[5], 0, "reply +0x14 is the address FUN_00164450 refuses to proceed without");

    // And the co-processor's read pointer moved past exactly one message.
    bcm_seek(&mut m, rec + 0x10, false);
    assert_eq!(bcm_get16(&mut m) as u32, tx_lo + 0x30);
}


// ---------------------------------------------------------------- §10 saturation
//
// Nine published conclusions in this project were lost to a capped log printed as a count. These
// tests are the mechanical guard: they assert that a saturated instrument SAYS it is saturated, and
// that the count beside it keeps rising after the log has stopped.
//
// Each carries a matched control — an unsaturated instrument, whose report must NOT carry the
// warning. A test that only checks the loud case cannot tell "always warns" from "warns correctly",
// and "always warns" is its own way of going unread.

#[test]
fn a_saturated_log_reports_the_census_and_says_the_rows_are_a_sample() {
    let mut c: eapp_loader::Capped<u32> = eapp_loader::Capped::new(3);
    for i in 0..10 {
        c.push(i);
    }
    assert_eq!(c.seen(), 10, "the count must not stop at the cap");
    assert_eq!(c.sample(), &[0, 1, 2], "the kept rows are the first ones, not the last");
    assert!(c.truncated());
    let line = c.census();
    assert!(line.starts_with("10"), "the census is the headline: {line}");
    assert!(line.contains("SAMPLE, NOT A CENSUS"), "saturation must announce itself: {line}");

    // The control: below the cap, the same call is a bare number with no warning at all.
    let mut quiet: eapp_loader::Capped<u32> = eapp_loader::Capped::new(3);
    quiet.push(1);
    quiet.push(2);
    assert!(!quiet.truncated());
    assert_eq!(quiet.census(), "2", "an unsaturated instrument must not cry wolf");
    assert_eq!(quiet.more_line(2), None, "nothing is hidden, so nothing is claimed to be");
}

#[test]
fn a_lazy_row_that_is_never_built_is_still_counted() {
    // `push_with` exists so an expensive row can stay lazy. If laziness cost the count, the cheap
    // instrument would be the lying one — which is exactly the trade that produced `ata commands`.
    let mut built = 0;
    let mut c: eapp_loader::Capped<String> = eapp_loader::Capped::new(2);
    for i in 0..7 {
        c.push_with(|| {
            built += 1;
            format!("row {i}")
        });
    }
    assert_eq!(built, 2, "the closure ran past the cap");
    assert_eq!(c.seen(), 7, "the count stopped with the closure");
}

#[test]
fn the_i2c_census_keeps_counting_after_its_ordered_log_has_filled() {
    // The instrument that named this class most recently: the 4 G baseline prints exactly 4 096
    // transfers, and every per-device figure under it — including the WM8758's 52 — was a tally of
    // that saturated log rather than of the bus.
    use arm7tdmi::Bus as _;
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    m.mem.regions.push(eapp_loader::Region { name: "mmio-7", base: 0x7000_0000, data: vec![0; 0x1_0000] });
    m.mem.i2c_base = Some(0x7000_c000);

    // 5 000 transfers to one device, which is past the 4 096-entry log cap. Device 0x34 is the
    // WM8758's address as the controller carries it (0x1a << 1).
    m.mem.write32(0x7000_c004, 0x34);
    m.mem.write32(0x7000_c00c, 0x6f);
    for _ in 0..5000 {
        m.mem.write32(0x7000_c000, 0x80);
    }

    assert!(m.mem.i2c_log.truncated(), "the log did not saturate, so this proves nothing");
    assert_eq!(m.mem.i2c_log.sample().len(), 4096, "the cap moved; update this test's premise");
    assert_eq!(m.mem.i2c_log.seen(), 5000, "the log's own count stopped at the cap");

    let per_dev: u64 = m.mem.i2c_tally.iter().filter(|((d, _, _), _)| *d == 0x34).map(|(_, n)| n).sum();
    assert_eq!(per_dev, 5000, "the per-device census is still a tally of the capped log");
    assert_eq!(
        m.mem.i2c_tally.get(&(0x34, 0x80, 0x6f)).copied(),
        Some(5000),
        "the per-register census is still a tally of the capped log"
    );
}

#[test]
fn watch_range_names_every_writer_of_a_word_not_just_the_first() {
    // `--watch-range` printed the FIRST pc per word, from a log capped at 4 096. On a span Apple's
    // bootloader fills before RetailOS runs, that reported the bootloader as the sole author — which
    // is how "RetailOS never touches the VideoCore" was published.
    use arm7tdmi::Bus as _;
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    let span = RAM_BASE + 0x100;
    m.mem.watch_range = Some((span, 0x10));

    // An early writer that fills the log on its own, then a late one at the same word.
    m.mem.pc = 0x4000_ec14;
    for _ in 0..5000 {
        m.mem.write32(span, 1);
    }
    m.mem.pc = 0x0016_4f44;
    m.mem.write32(span, 2);

    assert!(m.mem.watch_range_log.truncated(), "the log did not saturate, so this proves nothing");
    let w = m.mem.watch_range_words.get(&span).expect("the word was not accounted for");
    assert_eq!(w.writes, 5001 * 4, "byte-granular writes are undercounted");
    assert_eq!(w.pcs.len(), 2, "the late writer is invisible — the original defect");
    assert_eq!(w.pcs.get(&0x0016_4f44).copied(), Some(4), "the late writer's own count is wrong");

    // The control: a word nobody touched must not appear at all, or "every writer" would be
    // satisfied by a table that lists everything.
    assert!(m.mem.watch_range_words.get(&(span + 8)).is_none());
}
// ---------------------------------------------------------------- the PMU's ADC

const I2C: u32 = 0x7000_c000;

fn pmu_machine() -> Machine {
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    // The I²C block is MMIO, so it needs storage behind it before the controller can be driven.
    m.mem.regions.push(eapp_loader::Region {
        name: "mmio-7",
        base: 0x7000_0000,
        data: vec![0; 0x10_000],
    });
    m.mem.i2c_base = Some(I2C);
    m.mem.pmu = Some(eapp_loader::Pcf50605::new());
    m
}

/// One I²C write transfer to the PMU: `reg` then `vals`, exactly as `0x4000acac` issues it.
fn pmu_write(m: &mut Machine, reg: u8, vals: &[u8]) {
    use arm7tdmi::Bus;
    m.mem.write8(I2C + 4, 0x10); // PCF50605 at 0x08, write
    m.mem.write8(I2C + 0x0c, reg);
    for (i, v) in vals.iter().enumerate() {
        m.mem.write8(I2C + 0x10 + 4 * i as u32, *v);
    }
    let len = 1 + vals.len() as u8;
    m.mem.write8(I2C, 0x80 | ((len - 1) << 1));
}

/// One I²C read transfer of `len` bytes from `reg`, and the bytes it returns.
fn pmu_read(m: &mut Machine, reg: u8, len: usize) -> Vec<u8> {
    use arm7tdmi::Bus;
    m.mem.write8(I2C + 4, 0x10);
    m.mem.write8(I2C + 0x0c, reg);
    m.mem.write8(I2C, 0x80); // one-byte write: move the pointer
    m.mem.write8(I2C + 4, 0x11); // and now read
    m.mem.write8(I2C, 0xa0 | ((len as u8 - 1) << 1));
    (0..len).map(|i| m.mem.read8(I2C + 0x0c + 4 * i as u32)).collect()
}

/// A conversion that reports itself complete must report its VALUE, not a zero.
///
/// Apple's bootloader starts a conversion and then reads `ADCS1`+`ADCS2` as a **single two-byte
/// transfer**, twice. The model used to spend the busy countdown inside `read_reg(0x30)`, so on
/// the second poll the ADCS1 byte was served from the in-flight state (a synthetic `0`) and the
/// ADCS2 byte from the completed one (ready set) — one transfer straddling both states, and the
/// only value the firmware ever accepted was zero. With no charger attached the bootloader checks
/// the battery, read a flat cell every time, and halted at `0x400015b4` without touching the disk.
///
/// Positive control, matched in width, device and code path: the RTC block, written and read back
/// through the same two-byte transfers. If the harness could not move bytes at all the assertions
/// below would be vacuous.
#[test]
fn a_completed_adc_conversion_reports_the_value_it_converted() {
    let mut m = pmu_machine();

    // Control first: a register pair that has nothing to do with the converter.
    pmu_write(&mut m, 0x0a, &[0x21, 0x43]);
    assert_eq!(pmu_read(&mut m, 0x0a, 2), vec![0x21, 0x43], "the harness cannot move bytes");

    // ADCC2: channel 0 (battery volts) in bits 4:1. ADCC1 bit 0 starts it.
    pmu_write(&mut m, 0x2f, &[0x00]);
    pmu_write(&mut m, 0x2e, &[0x01]);

    // Poll it exactly the way the bootloader does, and keep what each poll saw.
    let mut seen = Vec::new();
    for _ in 0..4 {
        let r = pmu_read(&mut m, 0x30, 2);
        seen.push((r[0], r[1]));
        if r[1] & 0x80 != 0 {
            break;
        }
    }
    let (hi, lo) = *seen.last().unwrap();
    assert_ne!(lo & 0x80, 0, "the conversion never reported ready: {seen:?}");
    assert_eq!(
        (hi as u16) << 2 | (lo & 3) as u16,
        0x2c0,
        "ready was set but the value was not the one converted: {seen:?}"
    );
    // There used to be an assertion here that the FIRST poll saw the ready bit clear — "or there
    // is nothing for a poll loop to wait on". It was removed on 2026-08-18, deliberately, and the
    // reasoning is recorded because deleting an assertion to make a test pass is usually wrong.
    //
    // It encoded an unsourced belief. We have no datasheet for this part. What the assertion
    // actually described was the old implementation's shape: a countdown of two READ TRANSFERS,
    // which made the converter's completion depend on how many times the driver happened to ask.
    // Rockbox's `_adc_read` asks exactly once per conversion and never polls, so under that model
    // it never completed a single one of 27 000 conversions, read 0 mV, and powered the machine
    // off as a flat battery.
    //
    // A device cannot take longer because the driver is terse. The model now settles a conversion
    // before the host's next transfer, which is what the real part does — a 10-bit conversion is
    // microseconds and one I²C transaction at 400 kHz is ~70 — and under it there is no observable
    // not-ready window to assert on. Apple's boot is unchanged across the switch, to the digit:
    // 27 510 code buckets, 76 800 non-black pixels, same ATA opcode census, wheel reporting on.
    // That Apple polls is evidence its author was careful, not evidence the hardware was slow.
    //
    // What is still guarded, above: ready is set, and the value read back is the value converted.
    assert!(!seen.is_empty(), "the poll loop did not run at all");
}

/// `--pmu-adc=CH=VALUE` must survive the same round trip, on the channel it names and no other.
#[test]
fn a_per_channel_adc_override_reaches_the_firmware_intact() {
    let mut m = pmu_machine();
    m.mem.pmu.as_mut().unwrap().adc_values.push((3, 0x37f));

    for (channel, want) in [(3u8, 0x37fu16), (4, 0x200)] {
        pmu_write(&mut m, 0x2f, &[channel << 1]);
        pmu_write(&mut m, 0x2e, &[0x01]);
        let mut got = None;
        for _ in 0..4 {
            let r = pmu_read(&mut m, 0x30, 2);
            if r[1] & 0x80 != 0 {
                got = Some((r[0] as u16) << 2 | (r[1] & 3) as u16);
                break;
            }
        }
        assert_eq!(got, Some(want), "channel {channel:#x}");
    }
}

// ---------------------------------------------------------------- the snapshot's clock

/// A machine whose simulated clock is genuinely made of BOTH its halves.
///
/// `usec` is recomputed every instruction as `executed / instr_per_usec + slept_usec`, so a fixture
/// that has never slept cannot tell a snapshot that carries `slept_usec` from one that does not —
/// both restore the same number, and a test written on such a fixture passes whatever the format
/// does. The machine built here runs a two-instruction loop that writes CPU_CTRL's sleep bit with a
/// repeating 10 ms timer armed, which is the shape of RetailOS's idle task: the core asks to be
/// switched off, the run loop jumps the clock to the next deadline, and the accumulator ends up
/// carrying far more of the clock than the instruction count does.
fn sleeping_machine() -> Machine {
    use arm7tdmi::Bus as _;
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    // Storage behind the on-chip register block: the interrupt controller at 0x60004000, the two
    // timers at 0x60005000, the free-running counter at 0x60005010, CPU_CTRL at 0x60007000.
    m.mem.regions.push(eapp_loader::Region {
        name: "mmio-6",
        base: 0x6000_0000,
        data: vec![0; 0x1_0000],
    });
    m.mem.usec_timer = Some(0x6000_5010);
    m.instr_per_usec = 5;

    // TIMER1_CFG: enabled (bit 31), repeating (bit 30), period 10 000 µs — `period = (cfg & mask)+1`.
    m.mem.write32(0x6000_5000, 0xc000_0000 | 9_999);

    // str r1, [r0] ; b .-8      — an idle task, in two instructions.
    let code = RAM_BASE + 0x200;
    m.mem.write32(code, 0xe580_1000);
    m.mem.write32(code + 4, 0xeaff_fffd);
    m.call_with(code, &[eapp_loader::CPU_CTRL, 0x8000_0000, 0, 0], 5_000);
    m
}

/// A snapshot must restore the simulated clock the machine had when it was taken.
///
/// Version 3 of the format saved `usec` and not `slept_usec`. `usec` is derived, so the saved value
/// survived exactly zero instructions: the first one after a restore recomputed it against a zero
/// accumulator and the clock fell backwards — 2 940 704 453 µs to 321 777 002 µs on the standard
/// idle snapshot, 44 minutes of simulated time, on every restored run, silently. Firmware measuring
/// `now - start` in unsigned microseconds does not see a negative number — that pair wraps to
/// +1 676 039 845 µs, twenty-eight minutes of *elapsed* time — so every timeout in RetailOS is
/// expired the instant it is restored.
///
/// The two premise assertions are the point of the fixture: if the machine had never slept, or if
/// the clock happened to equal what the instruction count alone implies, this test would pass on
/// the broken format too. See research/20 Addendum 31.
#[test]
fn a_snapshot_round_trips_the_simulated_clock() {
    let m = sleeping_machine();
    let (executed, usec, slept) = (m.executed, m.mem.usec, m.mem.slept_usec);

    // The fixture must be unsaturated in both directions, or what follows is vacuous.
    assert!(slept > 0, "the fixture never slept, so it cannot tell the two formats apart");
    assert_ne!(
        usec,
        (executed / m.instr_per_usec) as u32,
        "the clock is exactly what the instruction count implies, so dropping the accumulator \
         would be invisible here"
    );

    let img = m.snapshot();
    let mut restored = sleeping_machine();
    assert!(restored.restore(&img), "the snapshot was refused");
    assert_eq!(restored.mem.usec, usec, "the restore itself did not carry the clock");
    assert_eq!(restored.mem.slept_usec, slept, "the sleep accumulator was not carried");
    assert_eq!(restored.executed, executed, "the instruction count was not carried");

    // And it must survive the recomputation, which is what actually broke. One instruction was all
    // it took to lose it.
    restored.run(1);
    assert!(
        restored.mem.usec >= usec && restored.mem.usec - usec <= 1,
        "the clock moved from {usec} to {} on the first instruction after a restore",
        restored.mem.usec
    );
}

/// The negative control for the test above: with the accumulator dropped — which is exactly what
/// version 3 of the format did — the clock must visibly fall backwards, by the amount dropped.
/// Without this, "the clock round-trips" is a claim about an instrument nobody has shown can fail.
#[test]
fn dropping_the_sleep_accumulator_moves_the_clock_backwards() {
    let m = sleeping_machine();
    let (usec, slept) = (m.mem.usec, m.mem.slept_usec);
    let img = m.snapshot();

    let mut restored = sleeping_machine();
    assert!(restored.restore(&img));
    // Reproduce the version-3 restore exactly: everything else carried, this one field zeroed.
    restored.mem.slept_usec = 0;
    restored.run(1);

    assert!(restored.mem.usec < usec, "the control did not reproduce the defect");
    assert_eq!(
        usec - restored.mem.usec,
        slept,
        "the clock should fall back by exactly the accumulator that was dropped"
    );
}

/// A snapshot from an older format must be refused rather than read with zeros in the new fields.
///
/// The cached snapshots this project keeps are keyed on a hash of the emulator's own source, so in
/// normal use a format change mints a new file. This is the belt: an older image reaching `restore`
/// by any other route would restore the exact machine each fix exists to abolish, and it would do
/// it silently.
///
/// v4 -> v5 added the click wheel: a v4 image read as v5 comes back with `reporting` false, which
/// is the state where the wheel is dead and says nothing about it. v5 -> v6 added the backlight
/// dimmer, whose absence shows up only as a screen at the wrong brightness.
#[test]
fn an_older_snapshot_is_refused() {
    let m = sleeping_machine();
    let mut img = m.snapshot();
    assert_eq!(&img[..8], b"IPODSNP6", "the format moved past v6; update this test");

    for old in [b"IPODSNP3", b"IPODSNP4", b"IPODSNP5"] {
        img[..8].copy_from_slice(old);
        let mut into = sleeping_machine();
        assert!(
            !into.restore(&img),
            "a {} snapshot was accepted",
            std::str::from_utf8(old).unwrap()
        );
    }

    // Positive control: the same bytes with the current magic still restore, so what is refused is
    // the version and not the image.
    img[..8].copy_from_slice(b"IPODSNP6");
    let mut into = sleeping_machine();
    assert!(into.restore(&img), "the harness cannot restore a valid snapshot at all");
}

/// The wheel's `reporting` flag must survive a snapshot round trip.
///
/// It starts false, the firmware sets it once with opcode 0x052a early in a boot, and before v5 the
/// snapshot did not carry it -- so every restored session came back with autonomous reporting off
/// and suppressed every frame the wheel posted. Scrolling produced a byte-identical panel, no
/// error, and no counter moving anywhere a person would look.
#[test]
fn a_restored_wheel_is_still_reporting() {
    let mut m = sleeping_machine();
    m.mem.clickwheel = Some(eapp_loader::ClickWheel::new(0x7000_c000));
    {
        let w = m.mem.clickwheel.as_mut().unwrap();
        w.reporting = true;
        w.position = 42;
        w.touched = true;
    }
    let img = m.snapshot();

    let mut into = sleeping_machine();
    into.mem.clickwheel = Some(eapp_loader::ClickWheel::new(0x7000_c000));
    assert!(into.restore(&img), "the snapshot did not restore");
    let w = into.mem.clickwheel.as_ref().expect("wheel");
    assert!(w.reporting, "reporting was lost -- the wheel comes back dead");
    assert_eq!(w.position, 42, "the wheel came back at a different position");
    assert!(w.touched, "the finger was lost");
}

// ---------------------------------------------------------------- the OR-masked registers

/// A read OR-mask must reach the firmware, which means the page it sits on must be off the fast
/// path.
///
/// `Memory` serves whole pages of plain storage without consulting any of the per-address tables,
/// and `page_is_plain` is the list of tables that disqualify a page. `read_or_masks` was added to
/// the read path and left off that list, so the mask was never consulted: reads of PLL_STATUS
/// returned the region's zero, the bootrom's lock-bit poll at `0x8780` never came out, and the
/// machine spun at instruction 23 for as long as it was given. The mechanism existed and did
/// nothing, which is the failure mode that looks most like success in a diff.
#[test]
fn a_read_or_mask_is_observed_through_the_ordinary_read_path() {
    use arm7tdmi::Bus;
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    m.mem.regions.push(eapp_loader::Region {
        name: "mmio-6",
        base: 0x6000_0000,
        data: vec![0; 0x10_000],
    });

    // Nothing else claims this page, so it is plain unless the mask itself disqualifies it.
    m.mem.read_or_masks.push((0x6000_603c, 0x8000_0000));
    assert_eq!(
        m.mem.read32(0x6000_603c),
        0x8000_0000,
        "the OR-mask was not consulted -- the page is still being served from the fast path"
    );

    // And it is a mask, not a replacement: every other bit is whatever the register holds. This is
    // the whole reason ledger #8 stopped being a whole-word override.
    m.mem.write32(0x6000_603c, 0x0000_1234);
    assert_eq!(
        m.mem.read32(0x6000_603c),
        0x8000_1234,
        "the mask overwrote bits it does not claim"
    );

    // Neighbouring words are untouched: the window is four bytes wide, not a page.
    assert_eq!(m.mem.read32(0x6000_6038), 0, "the mask leaked into the word below");
    assert_eq!(m.mem.read32(0x6000_6040), 0, "the mask leaked into the word above");
}

// ---------------------------------------------------------------- the backlight dimmer

/// The dimmer counts pulses, and the WIDTH of the low decides the direction.
///
/// Nothing reads the level back from hardware — the counter lives in the panel's circuit and the
/// firmware tracks its own idea of where it is. So if this classifier is wrong, nothing anywhere
/// disagrees with it; the screen is just the wrong brightness and stays that way. That is the whole
/// reason it is tested against the two delays Rockbox's driver actually uses, 10 us and 200 us,
/// rather than against a round number of its own.
#[test]
fn the_dimmer_counts_short_pulses_up_and_long_pulses_down() {
    use eapp_loader::{Backlight, BACKLIGHT_PIN};
    let mut b = Backlight::default();
    assert_eq!(b.level, 16, "the circuit is assumed to wake at the driver's midpoint");

    // One short pulse: low at t, high 10 us later.
    let mut t = 1_000u32;
    let pulse = |b: &mut Backlight, low_for: u32, t: &mut u32| {
        b.port_written(0, *t);
        *t += low_for;
        b.port_written(BACKLIGHT_PIN, *t);
        *t += 10;
    };
    pulse(&mut b, 10, &mut t);
    assert_eq!(b.level, 17, "a 10 us low is a step up");
    pulse(&mut b, 200, &mut t);
    assert_eq!(b.level, 16, "a 200 us low is a step down");

    // The range is 1..32 and neither end wraps — a wrap would take the panel from brightest to
    // black on one step, which is the sort of thing that looks like a rendering bug for a week.
    for _ in 0..40 {
        pulse(&mut b, 10, &mut t);
    }
    assert_eq!(b.level, 32, "the top of the range holds");
    for _ in 0..40 {
        pulse(&mut b, 200, &mut t);
    }
    assert_eq!(b.level, 1, "the bottom of the range holds, and is not zero");

    // Writes that do not move the pin are not edges. The firmware writes the whole port, so every
    // unrelated pin on it lands here too.
    let before = b.level;
    b.port_written(BACKLIGHT_PIN, t);
    b.port_written(BACKLIGHT_PIN | 0x21, t + 5);
    assert_eq!(b.level, before, "a write with the pin already high is not a pulse");
    assert_eq!(b.steps_up + b.steps_down, 82, "and it was not counted as one");
}

/// `thread-pp.c`'s `core_sleep` sets a mailbox bit and reads it back. Ours returned zero for ever,
/// which one running core happens to survive and two would not — found by counting reads rather
/// than by anything failing (research/15: 52 868 892 reads by Rockbox, none by RetailOS).
#[test]
fn setting_a_mailbox_bit_makes_it_readable_and_clearing_it_removes_it() {
    use arm7tdmi::Bus as _;
    use eapp_loader::Mbx;
    let mut m = wheel_machine();
    assert_eq!(m.mem.read32(Mbx::BASE), 0, "the mailbox does not start with bits raised");

    // core_wake(0): MBX_MSG_SET = 0x11 << 0
    m.mem.write32(Mbx::BASE + Mbx::SET, 0x11);
    assert_eq!(m.mem.read32(Mbx::BASE), 0x11, "a set bit did not appear in STAT");

    // core_sleep(0): MBX_MSG_SET = 0x4 << 0, on top of what is already raised.
    m.mem.write32(Mbx::BASE + Mbx::SET, 0x4);
    assert_eq!(m.mem.read32(Mbx::BASE), 0x15, "SET must OR, not replace");

    // core_sleep(0): MBX_MSG_CLR = 0x14 << 0 — drops two of the three.
    m.mem.write32(Mbx::BASE + Mbx::CLR, 0x14);
    assert_eq!(m.mem.read32(Mbx::BASE), 0x01, "CLR must clear only the bits written");

    // The loop `core_sleep` ends on: `while (MBX_MSG_STAT & (0x1 << core));`
    m.mem.write32(Mbx::BASE + Mbx::CLR, 0x1);
    assert_eq!(m.mem.read32(Mbx::BASE) & 0x1, 0, "the wait loop would never terminate");
}

/// Bits above the low byte must survive, because a 32-bit store arrives as four byte writes and
/// the lane arithmetic is the part that can be wrong.
#[test]
fn the_mailbox_is_thirty_two_bits_wide_not_eight() {
    use arm7tdmi::Bus as _;
    use eapp_loader::Mbx;
    let mut m = wheel_machine();
    m.mem.write32(Mbx::BASE + Mbx::SET, 0x8040_2010);
    assert_eq!(m.mem.read32(Mbx::BASE), 0x8040_2010);
    m.mem.write32(Mbx::BASE + Mbx::CLR, 0x0040_0010);
    assert_eq!(m.mem.read32(Mbx::BASE), 0x8000_2000);
}

/// **The two registers that belong to a core rather than to memory must answer at both widths.**
///
/// This is a regression test for a bug that hid behind a plausible-looking implementation. The
/// hook answering `PROC_ID` lived only on the byte path, because Rockbox's `crt0-pp.S` reads it
/// with `ldrb`. Apple's bootloader reads it with `ldr` and masks — `0x873c` in the retail NOR — and
/// `Memory::read32` has a fast path that never reaches `read8`.
///
/// So the coprocessor read `0x55`, concluded it was the CPU, and ran Apple's bootloader
/// concurrently with the real one. From outside that looked like six `Running 'osos'` lines in one
/// cold boot and no ATA commands at all — a symptom nobody would trace back to an access width.
#[test]
fn the_core_registers_answer_at_both_widths() {
    use arm7tdmi::Bus;
    use eapp_loader::{Core, PROC_ID};
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    eapp_loader::map_hardware(&mut m, false);

    // Single-core, which is what every measurement in research/ was taken on: the CPU's value,
    // whoever asks, and no second-core register at all.
    assert_eq!(m.mem.read8(PROC_ID), Core::Cpu.proc_id());
    assert_eq!(m.mem.read32(PROC_ID) & 0xff, Core::Cpu.proc_id() as u32);

    m.mem.second_core = true;

    // The CPU still reads what it always did — the byte AND the word.
    m.mem.asking = Core::Cpu;
    assert_eq!(m.mem.read8(PROC_ID), 0x55);
    assert_eq!(m.mem.read32(PROC_ID) & 0xff, 0x55, "Apple reads this as a word");

    // And the coprocessor reads its own id, the byte AND the word. The word is the one that broke.
    m.mem.asking = Core::Cop;
    assert_eq!(m.mem.read8(PROC_ID), 0xaa, "Rockbox reads this as a byte");
    assert_eq!(m.mem.read32(PROC_ID) & 0xff, 0xaa, "Apple reads this as a word");
    m.mem.asking = Core::Cpu;

    // `COP_CTRL` and `COP_STATUS` are one address: written to park the core, read to see it parked.
    // `crt0-pp.S` polls it with `ldr`, so the word path matters here too.
    let ctl = Core::Cop.ctrl();
    m.mem.cop_asleep = false;
    assert_eq!(m.mem.read32(ctl), 0, "awake");
    m.mem.write32(ctl, 0x8000_0000); // PROC_SLEEP, as Apple's COP path does at 0x805c
    assert!(m.mem.cop_asleep);
    assert_eq!(m.mem.read32(ctl), 0x8000_0000, "the CPU waits for exactly this");
    assert_eq!(m.mem.read8(ctl + 3), 0x80, "and the same value one byte at a time");
    m.mem.write32(ctl, 0); // PROC_WAKE, as Rockbox's `wake_core` does
    assert!(!m.mem.cop_asleep);
    assert_eq!(m.mem.read32(ctl), 0);

    // Both transitions are counted, because "the COP ran N instructions" says nothing on its own.
    assert_eq!(m.mem.cop_sleeps, 1);
    assert_eq!(m.mem.cop_wakes, 1);
}

/// **The device window's uncached mirror answers, at both widths and through both paths.**
///
/// iPodLinux drives the interrupt controller at `0x64004000` — 8 385 336 reads that landed in the
/// unmapped report for days — and its own constant pool is what identifies the address: the word is
/// loaded, dereferenced, and bit-tested against a source scan whose bit 8 is IRQ 40, the click wheel
/// this emulator already delivers at `0x60004000`.
///
/// Two paths have to agree, and they are reached differently. `read32` takes the `fast_region`
/// route, which translates a whole page; `read8` goes through `locate`. A mirror wired into one and
/// not the other is this project's most-repeated bug — `read_or_masks` shipped exactly that way and
/// left the bootrom spinning at instruction 23 — so both are asserted here.
#[test]
fn the_device_window_is_mirrored_where_the_kernel_reads_it() {
    use arm7tdmi::Bus;
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    eapp_loader::map_hardware(&mut m, false);

    // CPU_INT_STAT and its high bank — the two the kernel actually holds in its pool.
    for (real, mirror) in [(0x6000_4000u32, 0x6400_4000u32), (0x6000_4100, 0x6400_4100)] {
        m.mem.write32(real, 0xdead_beef);
        assert_eq!(m.mem.read32(mirror), 0xdead_beef, "{mirror:#x} is not a view of {real:#x}");
        assert_eq!(m.mem.read8(mirror), 0xef, "{mirror:#x} disagrees on the byte path");
        // A mirror is the same storage, not a copy: a write through it is visible at the original.
        m.mem.write32(mirror, 0x0000_0100);
        assert_eq!(m.mem.read32(real), 0x0000_0100, "the mirror kept its own buffer");
    }
    // And it is bounded — the mirror is the 1 MB device window, not the whole 64 MB above it.
    assert_eq!(m.mem.translate(0x6400_0000), 0x6000_0000);
    assert_eq!(m.mem.translate(0x6410_0000), 0x6410_0000, "past the window, no translation");
}

/// **A validity bit has to be backed by the fields it validates.**
///
/// Word 53 bit 0 of IDENTIFY DEVICE says "words 54-58 are valid". Those words held zero, which is
/// the same defect shape as a config option with no mechanism behind it: a driver that believes the
/// bit reads a geometry of nothing. This asserts the two halves agree, and that the CHS capacity is
/// the CHS *ceiling* rather than the disk's real size — reporting an LBA48-sized figure in a field
/// three CHS fields cannot address is how a geometry ends up multiplying out to more sectors than
/// its own heads and sectors can reach.
#[test]
fn identify_does_not_advertise_fields_it_leaves_empty() {
    let id = eapp_loader::Ata::identify_sector(16_777_216, 0, 0);
    let w = |n: usize| u16::from_le_bytes([id[n * 2], id[n * 2 + 1]]);

    assert_eq!(id.len(), 512, "IDENTIFY DEVICE is one sector");
    assert_eq!(w(53) & 1, 1, "this test is about the bit being set");
    assert_eq!(w(54), w(1), "current cylinders");
    assert_eq!(w(55), w(3), "current heads");
    assert_eq!(w(56), w(6), "current sectors per track");
    assert!(w(55) <= 16, "more than 16 heads is what Linux rejects outright");

    let chs = w(54) as u32 * w(55) as u32 * w(56) as u32;
    assert_eq!(w(57) as u32 | ((w(58) as u32) << 16), chs, "current capacity is C*H*S");
    let lba = w(60) as u32 | ((w(61) as u32) << 16);
    assert_eq!(lba, 16_777_216, "the true size still lives in words 60/61");
    assert!(chs < lba, "an 8 GiB disk is larger than CHS can address; that is the point");
}

/// **A wake ends the running core's turn — because two cores are concurrent, not alternating.**
///
/// Apple's bootloader writes the coprocessor's entry vector at `0x40000050` and wakes it two
/// instructions later, and both cores are then meant to enter the OS and let its own crt0 branch on
/// `PROC_ID`. With a plain 1000-instruction quantum the CPU ran on into that OS first, and on the
/// Rockbox path its startup overwrote the vector 90 instructions after the wake — so the
/// coprocessor read an instruction word, jumped to it as an address, and wandered 27 660 256 code
/// buckets. The quantum was asserting that nothing observable happens between turns, and across
/// this edge that is false.
#[test]
fn waking_the_coprocessor_yields_to_it_immediately() {
    use arm7tdmi::Bus;
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    eapp_loader::map_hardware(&mut m, false);
    m.mem.second_core = true;

    // Park it, the way the coprocessor parks itself: PROC_SLEEP into COP_CTL.
    m.mem.write32(eapp_loader::Core::Cop.ctrl(), 0x8000_0000);
    assert!(m.mem.cop_asleep, "PROC_SLEEP should park it");
    assert!(!m.mem.yield_to_cop, "parking is not a reason to yield");

    // And wake it, the way Apple's bootloader does: PROC_WAKE is zero.
    m.mem.write32(eapp_loader::Core::Cop.ctrl(), 0);
    assert!(!m.mem.cop_asleep, "PROC_WAKE should start it");
    assert!(m.mem.yield_to_cop, "a wake must end the running core's turn at once");

    // Re-writing the same state is not an edge and must not yield: firmware polls this register,
    // and a yield per poll would hand the coprocessor a turn thousands of times for nothing.
    m.mem.yield_to_cop = false;
    m.mem.write32(eapp_loader::Core::Cop.ctrl(), 0);
    assert!(!m.mem.yield_to_cop, "only the transition is an edge");

    // A single-core machine must be untouched by any of this.
    let mut solo = Machine::new(&EApp::parse(synth_eapp()).unwrap(), RAM_BASE, RAM_SIZE);
    eapp_loader::map_hardware(&mut solo, false);
    solo.mem.write32(eapp_loader::Core::Cop.ctrl(), 0);
    assert!(!solo.mem.yield_to_cop, "no second core, no yield");
}

/// **A byte store and a word store must reach the same memory.**
///
/// The NOR is aliased over address 0 out of reset and the boot ROM fetches from it, but firmware
/// then programs the MMAP unit to put SDRAM there. `write8_inner` tested the flash windows on the
/// **raw** address, so after that remap it kept handing low stores to the chip, which swallowed
/// them and returned — while `write32`'s fast path resolved through `translate` and landed in RAM.
///
/// Cold-booted Rockbox is what found it: `disk_init` writes `partinfo[].start` with an `stm` and
/// `.type` with a `strb`, so the start survived and the type did not, every partition read as type
/// 0, `disk_mount_all()` returned 0, and it sat on "No partition found (0)." forever.
#[test]
fn a_byte_store_lands_where_a_word_store_does() {
    use arm7tdmi::Bus;
    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    eapp_loader::map_hardware(&mut m, true); // cold: NOR over 0, SDRAM at 0x10000000

    // **The flash model has to be present or this test proves nothing** — with `nor` unset the
    // branch that swallowed the store is never entered and the assertion passes against the bug.
    // Confirmed: without this line the test is green either way.
    m.mem.nor = Some(eapp_loader::Nor::sst39wf800a(vec![(0, 0x10_0000)], vec!["flash-low"]));

    // Stand in for the MMAP remap the firmware performs: low addresses become SDRAM.
    m.mem.aliases.push((0x0000_0000, 0x0400_0000, 0x1000_0000));
    m.mem.invalidate_fast();

    // Two adjacent fields of one struct, exactly as `disk_init` writes them.
    let (start, ty) = (0x000e_72f8u32, 0x000e_7308u32);
    m.mem.write32(start, 0x0000_8000);
    m.mem.write8(ty, 0x0c);

    assert_eq!(m.mem.read32(start), 0x0000_8000, "the word store");
    assert_eq!(m.mem.read8(ty), 0x0c, "the byte store was swallowed by the flash");

    // And the flash must still be reachable where it really is, or this trades one bug for another:
    // the updater writes the chip at 0x20000000 and the reset-time view at 0 must still fetch.
    assert_eq!(
        m.mem.translate(0x2000_0000),
        0x2000_0000,
        "the flash aperture is not remapped and must still reach the chip"
    );
}

/// **The IDE data register is 16 bits wide, and a 32-bit read of it must not swallow two words.**
///
/// Every register in the PP502x IDE block is four bytes apart, so a 32-bit access to `IDE_BASE+0x1e0`
/// touches four byte lanes — but the register underneath is 16 bits, and lanes 2 and 3 are its empty
/// upper half. Serving them as more sector data cost us iPodLinux for a long time: its identify path
/// reads this port with 32-bit loads and keeps the low halfword, which is correct for a 16-bit
/// register, so a model handing over two words per access gave it words 0, 2, 4, 6 … and dropped
/// every other one. `struct hd_driveid` then read `cyls` out of our word 2 and `heads` out of our
/// word 6, and a drive reporting 16 heads was diagnosed as having 63.
///
/// Rockbox and Apple's firmware never saw it — both read this port 16 bits at a time.
#[test]
fn a_32_bit_read_of_the_data_register_yields_one_ata_word() {
    use arm7tdmi::Bus;
    let dir = std::env::temp_dir().join("ipod-emu-ata-width");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let img = dir.join("disk.img");
    std::fs::write(&img, vec![0u8; 512 * 64]).expect("write image");

    let app = EApp::parse(synth_eapp()).expect("parse");
    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);
    eapp_loader::map_hardware(&mut m, false);
    let drive = eapp_loader::Ata::open(&img, false).expect("open image");
    let sectors = drive.sectors;
    m.mem.ata = Some((0xc300_0000, drive));

    const DATA: u32 = 0xc300_01e0;
    m.mem.write8(0xc300_01fc, 0xec); // IDENTIFY DEVICE

    // Read it the way iPodLinux does: 256 32-bit loads, keeping the low halfword of each. One word
    // per access is exactly one 512-byte sector; two words per access would be twice what a drive
    // has to give.
    let mut got = Vec::with_capacity(512);
    for _ in 0..256 {
        let w = m.mem.read32(DATA);
        got.extend_from_slice(&(w as u16).to_le_bytes());
    }

    let want = eapp_loader::Ata::identify_sector(sectors, 0, 0);
    assert_eq!(got.len(), want.len(), "256 32-bit reads have to cover one sector");
    assert_eq!(
        &got[..16],
        &want[..16],
        "the first eight words the guest keeps must be our first eight words, in order"
    );
    assert_eq!(got, want, "the whole IDENTIFY response, not every second word of it");
}
