//! The second core, and which of the two is executing.

/// `CPU_CTRL` from Rockbox `pp5020.h`. Bit 31 is SLEEP; `0x60007004` is the COP's counterpart.
/// Which of the PP5021's two ARM cores is executing.
///
/// **The silicon answers one address differently depending on who asks.** `PROC_ID` at
/// `0x60000000` reads `0x55` on the CPU and `0xAA` on the COP, and that single byte is how every
/// firmware for this part decides which half of its startup to run — Rockbox's `crt0-pp.S` branches
/// on it in its first four instructions. So a second core is not only a second register file: the
/// bus has to know who is asking.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Core {
    #[default]
    Cpu,
    Cop,
}

impl Core {
    /// What `PROC_ID` reads as. From Rockbox's `pp5020.h`: `PROC_ID_CPU 0x55`, `PROC_ID_COP 0xaa`.
    pub fn proc_id(self) -> u8 {
        match self {
            Core::Cpu => 0x55,
            Core::Cop => 0xaa,
        }
    }
    /// This core's sleep/wake control register — `CPU_CTL` / `COP_CTL`.
    pub fn ctrl(self) -> u32 {
        match self {
            Core::Cpu => 0x6000_7000,
            Core::Cop => 0x6000_7004,
        }
    }
    /// `(status, enable_state, enable_set, enable_clear)` for the low bank, then the same 0x100
    /// higher for the high bank. The CPU's are at `0x60004000`/`0x60004020..28`; the COP's mirror
    /// sits at `0x60004004`/`0x60004030..38`, which is what `pp5020.h` documents and what makes
    /// per-core interrupt masking possible at all.
    pub fn int_regs(self) -> (u32, u32, u32, u32) {
        match self {
            Core::Cpu => (0x6000_4000, 0x6000_4020, 0x6000_4024, 0x6000_4028),
            Core::Cop => (0x6000_4004, 0x6000_4030, 0x6000_4034, 0x6000_4038),
        }
    }
}
