//! The two cores' mailbox — how the CPU and the co-processor talk.

use crate::*;

/// The external memory bus controller — `0x70000030` and `XMB_RAM_CFG` at `0x7000003c`.
///
/// Both registers were `--rdval` hypotheses in the cold-boot recipe (ledger #1 and #2). What they
/// are was read out of the ROM's own use of them, with `--disasm` on the running machine.
///
/// **`0x70000030`** appears in no published map; `pp5020.h` names its neighbours `DEV_TIMING1`
/// (`+0x34`), `XMB_NOR_CFG` (`+0x38`) and `XMB_RAM_CFG` (`+0x3c`), so it heads the memory-controller
/// group. Three instruction sequences in the whole image touch it, and together they say what two
/// of its bits are:
///
/// ```text
/// 40001378  mov  r2, #0x70000000
/// 4000137c  ldr  r1, [r2, #0x30]
/// 40001380  tst  r1, #0x8000000      ; bit 27
/// 40001384  beq  0x4000137c          ; spin until the controller says ready
/// 40001388  ldr  r1, [r2, #0x30]
/// 4000138c  cmp  r0, #0x0            ; the argument
/// 40001390  movne r0, #0x40000000    ; bit 30
/// 40001394  bic  r1, r1, #0x40000000
/// 40001398  orr  r0, r1, r0
/// 4000139c  str  r0, [r2, #0x30]
/// ```
///
/// All six call sites pass 1, run a JEDEC NOR command sequence, then pass 0. `0x40009f88` writes
/// `0xAAAA` to `0xAAAA`, `0x5555` to `0x5554`, autoselect `0x9090`, reads the ID words back and
/// resets with `0xF0F0`; the other two pairs dispatch through a device table at `0x400150e0` whose
/// rows begin with JEDEC ID pairs (`…00ec` Samsung, `…0001` AMD, `…00bf` SST) and whose `+0x8`
/// method is a sector erase in the same command set. The object the driver works from carries the
/// magic `'Cfi!'` at `+0`. So bit 30 is the **NOR write gate**: closed, a store to flash is an
/// ordinary store; open, it is a command.
///
/// Bit 27 is **read-only ready**. Nothing in the image writes it — the three writers all reach it
/// through `bic`/`orr` of other fields — and the enable path waits for it while bit 30 is still
/// *clear*, which rules out its being an echo of bit 30 (an echo would deadlock there). A bus that
/// finishes every access inside the access is never not-ready, so this model holds it set and lets
/// the rest of the word be ordinary storage, which is what makes the ROM's read-modify-writes of
/// bits 30, 16, 11:8 and 7:4 read back.
///
/// **`0x7000003c`** is `XMB_RAM_CFG`, and the SDRAM bring-up at `0x40003590` shows the handshake:
/// write the geometry word, `orr` in bit 24, write again, spin on bit 31; then probe the array's
/// address aliasing (`0x40008ba8` writes `0x10000040`, `+0x800`, `+0x400`, `+0x200` and reads the
/// first back), fold the answer into bits 17:16, and repeat. **Bit 24 is the command and bit 31 is
/// its completion** — a handshake the firmware starts itself, which is why no static value and no
/// alternating value was ever the right shape. Applying a configuration to a modelled array takes
/// no time, so the completion lands on the kick; the point is that it lands *because of* the kick.
pub struct Xmb {
    pub base: u32,
    /// Times bit 30 went 0 -> 1 and 1 -> 0. Printed because they should come in pairs, and an
    /// unpaired open would mean the ROM left the flash writable — a real fault, not a counter.
    pub gate_opens: u64,
    pub gate_closes: u64,
    /// Times bit 24 was written set. Two per boot is the SDRAM bring-up's two configurations.
    pub ram_kicks: u64,
    /// Times `INIT_USB` was written into `DEV_INIT2`. See [`Xmb::usb_clock`].
    pub usb_enables: u64,
}

impl Xmb {
    /// Byte 3 of `+0x30` and of `+0x3c` — the only two bytes whose stored value is not simply what
    /// the firmware wrote. Everything else in the block is plain memory.
    const CTRL_HI: u32 = 0x33;
    const RAM_CFG_HI: u32 = 0x3f;
    /// Within byte 3: bit 27 -> `0x08`, bit 30 -> `0x40`, bit 24 -> `0x01`, bit 31 -> `0x80`.
    const READY: u8 = 0x08;
    const NOR_GATE: u8 = 0x40;
    const RAM_START: u8 = 0x01;
    const RAM_DONE: u8 = 0x80;

    pub fn new(base: u32) -> Self {
        Self {
            base,
            gate_opens: 0,
            gate_closes: 0,
            ram_kicks: 0,
            usb_enables: 0,
        }
    }

    /// Byte 3 of `+0x20` — `DEV_INIT2`'s high byte, holding `INIT_USB` (bit 31).
    const DEV_INIT2_HI: u32 = 0x23;
    const INIT_USB_HI: u8 = 0x80;
    /// `+0x28`, whose bit 7 the USB clock reports itself ready in.
    pub const USB_STATUS: u32 = 0x28;
    pub const USB_CLOCK_READY: u8 = 0x80;

    /// The USB clock reporting ready, once something has switched it on.
    ///
    /// **Rockbox hangs forever without this**, at `usb-fw-pp502x.c:116` — `DEV_INIT2 |= INIT_USB;`
    /// and then `while ((inl(0x70000028) & 0x80) == 0);`, a spin with no timeout on a bit this
    /// emulator had no reason to have ever set. It is the first thing Rockbox does after drawing
    /// its splash, which is why the splash was as far as it got.
    ///
    /// Modelled as a *consequence of the enable* rather than as a bit that is simply always on,
    /// because those differ: a machine that reports its USB clock locked before anyone started it
    /// is answering a question nobody asked, and would hide a driver that forgot to start it.
    ///
    /// **Not a bypass, and measured rather than assumed.** `--read-count=0x70000028,0x70000020`
    /// over a 600 M-instruction RetailOS boot: `0x70000020` is read ten times, from five call
    /// sites, and `0x70000028` is read **zero** times. Apple's firmware never looks at this
    /// address, so nothing in `research/` is measured through it.
    ///
    /// Returned as a side effect for the caller to apply, in keeping with the rest of this model:
    /// the state lives in the region, so a snapshot carries it without knowing this device exists.
    pub fn usb_clock(&mut self, addr: u32, val: u8) -> Option<(u32, u8)> {
        if addr.wrapping_sub(self.base) != Self::DEV_INIT2_HI || val & Self::INIT_USB_HI == 0 {
            return None;
        }
        self.usb_enables += 1;
        Some((self.base + Self::USB_STATUS, Self::USB_CLOCK_READY))
    }

    /// The reset value of byte 3 of `+0x30`: ready, gate closed.
    pub fn ctrl_hi_at_reset() -> u8 {
        Self::READY
    }

    /// Whether `addr` is one of the two bytes this model owns.
    pub fn owns(&self, addr: u32) -> bool {
        let off = addr.wrapping_sub(self.base);
        off == Self::CTRL_HI || off == Self::RAM_CFG_HI
    }

    /// What the register file keeps when the firmware writes `val` at `addr`, and `was` is there.
    ///
    /// Written as a filter on the stored byte rather than as an override on the read, so the state
    /// lives in the region and a snapshot carries it without knowing this device exists.
    pub fn store(&mut self, addr: u32, was: u8, val: u8) -> u8 {
        match addr.wrapping_sub(self.base) {
            Self::CTRL_HI => {
                match (was & Self::NOR_GATE != 0, val & Self::NOR_GATE != 0) {
                    (false, true) => self.gate_opens += 1,
                    (true, false) => self.gate_closes += 1,
                    _ => {}
                }
                val | Self::READY
            }
            Self::RAM_CFG_HI => {
                if val & Self::RAM_START != 0 {
                    self.ram_kicks += 1;
                    val | Self::RAM_DONE
                } else {
                    // Staging a configuration without the command bit retracts the completion:
                    // the controller has been given something new and has not been told to do it.
                    val & !Self::RAM_DONE
                }
            }
            _ => val,
        }
    }
}

/// The CPU<->COP mailbox at `0x60001000`, as Rockbox's `pp5020.h` names it.
///
/// Three registers, and only the first is storage: `MBX_MSG_STAT` at `+0x00` reports the bits,
/// `MBX_MSG_SET` at `+0x04` raises the ones written to it, `MBX_MSG_CLR` at `+0x08` drops them.
/// Modelling it is four lines; not modelling it made a set-then-read return zero.
///
/// `thread-pp.c` is the specification. `core_sleep` writes `0x4 << core` to SET to announce it is
/// going down, tests `0x10 << core` in STAT to see whether anyone is trying to wake it, clears
/// `0x14 << core`, then spins on `while (MBX_MSG_STAT & (0x1 << core))`; `core_wake` sets
/// `0x11 << othercore` and waits on `0x4 << othercore`. All of that is a conversation between two
/// cores, and with one core running it happens to survive a mailbox stuck at zero — which is why
/// this went unseen until something counted the reads.
pub struct Mbx;

impl Mbx {
    pub const BASE: u32 = 0x6000_1000;
    pub const STAT: u32 = 0x00;
    pub const SET: u32 = 0x04;
    pub const CLR: u32 = 0x08;
    /// `CPU_QUEUE` — **the COP posts here and the CPU is interrupted.** Rockbox's `pp5020.h`:
    /// *"COP can set bit 29 — only CPU read clears it"*.
    pub const CPU_QUEUE: u32 = 0x10;
    /// `COP_QUEUE`, the same the other way: *"CPU can set bit 29 — only COP read clears it"*.
    pub const COP_QUEUE: u32 = 0x20;
    /// The bit that means "there is a message", and the one that raises the interrupt.
    pub const MSG: u32 = 1 << 29;
    /// `MAILBOX_IRQ` — Rockbox's `pp5020.h` line 108. Low bank, so `int_pending` bit 4.
    pub const IRQ: u32 = 4;

    /// Which queue an address is, and which core is meant to be woken by it.
    pub fn queue(addr: u32) -> Option<Core> {
        match addr.wrapping_sub(Self::BASE) & !3 {
            Self::CPU_QUEUE => Some(Core::Cpu),
            Self::COP_QUEUE => Some(Core::Cop),
            _ => None,
        }
    }

    /// `Some(true)` for a write to SET, `Some(false)` for CLR, `None` for anything else.
    pub fn strobe(addr: u32) -> Option<bool> {
        match addr.wrapping_sub(Self::BASE) & !3 {
            Self::SET => Some(true),
            Self::CLR => Some(false),
            _ => None,
        }
    }
}
