//! The USB device controller at `0xC5000000`: the generic ARC (ChipIdea) high-speed core in device
//! mode, with queue heads and transfer descriptors read from and written to guest RAM by DMA.
//!
//! The register file decodes 9 address bits, so the 64 KB window repeats every `0x200`. Every
//! register is reached through [`Usb::read8`] and [`Usb::write8`] one lane at a time, because the
//! bus delivers a 32-bit store as four byte stores; a register applies the lanes it is given.
//!
//! The host end of the bus is a socket (`usbbus`). Frames are taken at the interrupt service tick,
//! one transaction each, and answered at once. Where the register map left a behaviour open the
//! generic core's documented one is used.

use crate::hw::usbbus::{self, Event, Frame, Link};

pub const BASE: u32 = 0xc500_0000;
pub const WINDOW: u32 = 0x1_0000;
/// CPU IRQ number of the controller (`USB_MASK` in `pp5020.h`).
pub const IRQ: u32 = 20;

/// SoC glue words the driver touches, as full-word addresses.
pub const DEV_RS: u32 = 0x6000_6004;
pub const DEV_EN: u32 = 0x6000_600c;
pub const DEV_INIT2: u32 = 0x7000_0020;
pub const R28: u32 = 0x7000_0028;
pub const R2C: u32 = 0x7000_002c;
pub const R60: u32 = 0x7000_0060;
const DEV_USB0: u32 = 1 << 3;
/// The enable the stock firmware sets for the controller (bit 22) when it attaches.
const DEV_USB1: u32 = 1 << 22;
const INIT_USB: u32 = 1 << 31;

/// GPIO L, bit 4: 5 V on the connector.
pub const GPIOL_INPUT_VAL: u32 = 0x6000_d13c;
pub const GPIOL_INT_STAT: u32 = 0x6000_d14c;
pub const GPIOL_INT_EN: u32 = 0x6000_d15c;
pub const GPIOL_INT_LEV: u32 = 0x6000_d16c;
pub const GPIOL_INT_CLR: u32 = 0x6000_d17c;
pub const GPIOL_VBUS: u32 = 0x10;
/// The hi-bank bit of the third GPIO group (ports I to L).
pub const GPIO2_IRQ_HI: u32 = 2;

const ID: u32 = 0x000;
const HWGENERAL: u32 = 0x004;
const HWHOST: u32 = 0x008;
const HWDEVICE: u32 = 0x00c;
const HWTXBUF: u32 = 0x010;
const HWRXBUF: u32 = 0x014;
const CAPLENGTH: u32 = 0x100;
const DCIVERSION: u32 = 0x120;
const DCCPARAMS: u32 = 0x124;
const USBCMD: u32 = 0x140;
const USBSTS: u32 = 0x144;
const USBINTR: u32 = 0x148;
const FRINDEX: u32 = 0x14c;
const DEVICEADDR: u32 = 0x154;
const ENDPOINTLISTADDR: u32 = 0x158;
const BURSTSIZE: u32 = 0x160;
const ULPI: u32 = 0x170;
const ENDPTNAK: u32 = 0x178;
const ENDPTNAKEN: u32 = 0x17c;
const CONFIGFLAG: u32 = 0x180;
const PORTSC1: u32 = 0x184;
const OTGSC: u32 = 0x1a4;
const USBMODE: u32 = 0x1a8;
const ENDPTSETUPSTAT: u32 = 0x1ac;
const ENDPTPRIME: u32 = 0x1b0;
const ENDPTFLUSH: u32 = 0x1b4;
const ENDPTSTAT: u32 = 0x1b8;
const ENDPTCOMPLETE: u32 = 0x1bc;
const ENDPTCTRL: u32 = 0x1c0;

const CMD_RS: u32 = 1;
const CMD_RST: u32 = 2;
const CMD_SUTW: u32 = 1 << 13;
const CMD_ATDTW: u32 = 1 << 14;
const CMD_WRITABLE: u32 = CMD_RS | CMD_SUTW | CMD_ATDTW | 0x00ff_0000;
const CMD_RESET_VALUE: u32 = 0x0008_0000;

const STS_UI: u32 = 1 << 0;
const STS_UEI: u32 = 1 << 1;
const STS_PCI: u32 = 1 << 2;
const STS_SEI: u32 = 1 << 4;
const STS_URI: u32 = 1 << 6;
const STS_SRI: u32 = 1 << 7;
const STS_HCH: u32 = 1 << 12;
const STS_NAKI: u32 = 1 << 16;
const STS_W1C: u32 = STS_UI | STS_UEI | STS_PCI | STS_SEI | STS_URI | STS_SRI | (1 << 8) | STS_NAKI;
const INTR_MASK: u32 = 0x1ff | STS_NAKI;
const INTR_SRE: u32 = STS_SRI;

const PORT_CCS: u32 = 1 << 0;
const PORT_CSC: u32 = 1 << 1;
const PORT_PE: u32 = 1 << 2;
const PORT_PR: u32 = 1 << 8;
const PORT_HSP: u32 = 1 << 9;
const PORT_LS_J: u32 = 1 << 10;
const PORT_PFSC: u32 = 1 << 24;
/// The bits software can write: FPR, PP, PO, PIC, PTC, the wake enables, PHCD, PFSC, PTS2, PTW,
/// STS and PTS.
const PORT_RW: u32 = 0xf1ff_f000 | (1 << 6);
const PORT_RESET_RW: u32 = 1 << 29;

const MODE_CM: u32 = 3;
const MODE_CM_DEVICE: u32 = 2;
const MODE_SLOM: u32 = 1 << 3;
const MODE_WRITABLE: u32 = MODE_CM | (1 << 2) | MODE_SLOM | (1 << 4);

const TD_ACTIVE: u32 = 0x80;
const TD_HALTED: u32 = 0x40;
const TD_BUFERR: u32 = 0x20;
const TD_XACTERR: u32 = 0x08;
const TD_IOC: u32 = 1 << 15;

const QH_IOS: u32 = 1 << 15;
const QH_ZLT_OFF: u32 = 1 << 29;
const QH_SETUP: u32 = 0x28;

const EP_STALL_RX: u32 = 1 << 0;
const EP_STALL_TX: u32 = 1 << 16;
const EP_TOGGLE_INHIBIT: [u32; 2] = [1 << 5, 1 << 21];
const EP_TOGGLE_RESET: [u32; 2] = [1 << 6, 1 << 22];
const EP_ENABLE: [u32; 2] = [1 << 7, 1 << 23];
const EP_CTRL_WRITABLE: u32 = 0x00ef_00ef;
const EP0_RESET: u32 = 0x0080_0080;

/// Simulated microseconds a bus reset takes: `PR` reads 1 for this long.
const RESET_USEC: u32 = 2000;

/// How the controller reaches guest memory. The caller supplies this so that the model holds no
/// reference to the bus.
pub trait Dma {
    /// Fill `out` from `addr`; false if any byte is unmapped.
    fn dma_read(&mut self, addr: u32, out: &mut [u8]) -> bool;
    /// Store `data` at `addr`; false if any byte is unmapped or read-only.
    fn dma_write(&mut self, addr: u32, data: &[u8]) -> bool;
}

#[derive(Clone, Debug)]
pub struct Config {
    /// What `DCCPARAMS.DEN` reports: 3, 4 or 6.
    pub endpoints: u8,
    /// The link only ever runs at full speed.
    pub full_speed: bool,
    /// The port stays disconnected until the PP5022 PHY sequence has been seen.
    pub strict_phy: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            endpoints: 4,
            full_speed: false,
            strict_phy: false,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Pipe {
    td: u32,
    bufs: [u32; 5],
    off0: u32,
    done: u32,
    rem: u32,
    token: u32,
    zlp: bool,
}

#[derive(Default, Debug, Clone)]
pub struct Stats {
    pub gated_accesses: u64,
    pub controller_resets: u64,
    pub bus_resets: u64,
    pub setups: u64,
    pub setups_nak: u64,
    pub out_ok: u64,
    pub out_nak: u64,
    pub out_dup: u64,
    pub in_ok: u64,
    pub in_nak: u64,
    pub stalls: u64,
    pub timeouts: u64,
    pub td_errors: u64,
    pub dma_faults: u64,
    pub primes: u64,
    pub prime_refused: u64,
    pub attaches: u64,
    pub detaches: u64,
}

pub struct Usb {
    cfg: Config,
    link: Option<Link>,
    dev_en: u32,
    dev_init2: u32,
    dev_rs: u32,
    shadow: Vec<(u32, u32)>,
    phy_stage: u8,
    phy_prev60: u32,
    phy_prev28: u32,
    usbcmd: u32,
    usbsts: u32,
    usbintr: u32,
    addr: u8,
    addr_staged: Option<u8>,
    listaddr: u32,
    portsc_rw: u32,
    csc: bool,
    usbmode: u32,
    setupstat: u32,
    stat: u32,
    complete: u32,
    nak: u32,
    nak_en: u32,
    burst: u32,
    ulpi: u32,
    ctrl: [u32; 16],
    tog: [[bool; 2]; 16],
    pipes: [Pipe; 32],
    cable: bool,
    present: bool,
    speed: u8,
    reset_due: Option<u32>,
    reset_hs: bool,
    now: u32,
    last_frame: u32,
    quit: bool,
    /// What happened and when, capped; printed by `trace --usb-log`.
    pub log: Vec<String>,
    pub stats: Stats,
}

fn bit_of(pipe: usize) -> u32 {
    let (ep, dir) = (pipe / 2, pipe % 2);
    1 << (ep + 16 * dir)
}

impl Usb {
    pub fn new(cfg: Config) -> Usb {
        let mut u = Usb {
            cfg,
            link: None,
            dev_en: 0,
            dev_init2: 0,
            dev_rs: 0,
            shadow: Vec::new(),
            phy_stage: 0,
            phy_prev60: 0,
            phy_prev28: 0,
            usbcmd: CMD_RESET_VALUE,
            usbsts: 0,
            usbintr: 0,
            addr: 0,
            addr_staged: None,
            listaddr: 0,
            portsc_rw: PORT_RESET_RW,
            csc: false,
            usbmode: 0,
            setupstat: 0,
            stat: 0,
            complete: 0,
            nak: 0,
            nak_en: 0,
            burst: 0x404,
            ulpi: 0,
            ctrl: [0; 16],
            tog: [[false; 2]; 16],
            pipes: [Pipe::default(); 32],
            cable: false,
            present: false,
            speed: 2,
            reset_due: None,
            reset_hs: true,
            now: 0,
            last_frame: 0,
            quit: false,
            log: Vec::new(),
            stats: Stats::default(),
        };
        u.controller_reset("power-on");
        u.stats.controller_resets = 0;
        u
    }

    pub fn attach_socket(&mut self, link: Link) {
        self.link = Some(link);
    }

    pub fn endpoints(&self) -> u8 {
        self.cfg.endpoints
    }

    /// The host asked the emulator to stop.
    pub fn quit_requested(&self) -> bool {
        self.quit
    }

    pub fn cable(&self) -> bool {
        self.cable
    }

    fn clock_on(&self) -> bool {
        self.dev_en & (DEV_USB0 | DEV_USB1) != 0 && self.dev_init2 & INIT_USB != 0
    }

    fn running(&self) -> bool {
        self.usbcmd & CMD_RS != 0 && self.usbmode & MODE_CM == MODE_CM_DEVICE && self.clock_on()
    }

    fn compute_present(&self) -> bool {
        self.cable && self.running() && (!self.cfg.strict_phy || self.phy_stage >= PHY_STAGES)
    }

    /// Whether the interrupt line is asserted.
    pub fn irq_line(&self) -> bool {
        self.usbsts & self.usbintr & (INTR_MASK | STS_NAKI) != 0
    }

    fn note(&mut self, what: String) {
        if self.log.len() >= 400 {
            self.log.remove(0);
        }
        let t = self.now;
        self.log.push(format!("{t:>10} us  {what}"));
    }

    fn controller_reset(&mut self, cause: &str) {
        self.stats.controller_resets += 1;
        self.note(format!("controller reset by {cause}"));
        self.usbcmd = CMD_RESET_VALUE;
        self.usbsts = 0;
        self.usbintr = 0;
        self.addr = 0;
        self.addr_staged = None;
        self.listaddr = 0;
        self.portsc_rw = PORT_RESET_RW;
        self.usbmode = 0;
        self.setupstat = 0;
        self.stat = 0;
        self.complete = 0;
        self.nak = 0;
        self.nak_en = 0;
        self.ctrl = [0; 16];
        self.ctrl[0] = EP0_RESET;
        self.tog = [[false; 2]; 16];
        self.pipes = [Pipe::default(); 32];
        self.reset_due = None;
        self.phy_stage = 0;
        self.csc = false;
    }

    fn bus_reset_effects(&mut self) {
        self.addr = 0;
        self.addr_staged = None;
        self.setupstat = 0;
        self.stat = 0;
        self.complete = 0;
        self.tog = [[false; 2]; 16];
        self.pipes = [Pipe::default(); 32];
    }

    fn portsc(&self) -> u32 {
        let mut v = self.portsc_rw;
        if self.present {
            v |= PORT_CCS | PORT_PE;
            if self.reset_due.is_some() {
                v |= PORT_PR;
            } else {
                v |= PORT_LS_J;
            }
            if self.speed == 2 && self.reset_due.is_none() {
                v |= PORT_HSP;
            }
            v |= (self.speed as u32 & 3) << 26;
        }
        if self.csc {
            v |= PORT_CSC;
        }
        v
    }

    fn dccparams(&self) -> u32 {
        0x180 | self.cfg.endpoints as u32
    }

    /// Recompute `present` and tell the host if it changed. Returns whether it did.
    fn refresh_present(&mut self) -> bool {
        let p = self.compute_present();
        if p == self.present {
            return false;
        }
        self.present = p;
        self.note(format!(
            "port {}",
            if p { "connected" } else { "disconnected" }
        ));
        self.csc = true;
        self.usbsts |= STS_PCI;
        if p {
            self.stats.attaches += 1;
        } else {
            self.stats.detaches += 1;
            self.reset_due = None;
            self.bus_reset_effects();
            self.speed = 2;
        }
        true
    }

    fn state_frame(&self) -> Frame {
        let mut f = Frame::new(usbbus::STATE);
        f.aux = self.present as u8 | (self.cable as u8) << 1;
        f.flags = self.speed;
        f
    }

    fn send(&self, f: Frame) {
        if let Some(l) = &self.link {
            l.send(&f);
        }
    }

    fn notify_if_changed(&mut self, changed: bool) {
        if changed {
            let f = self.state_frame();
            self.send(f);
        }
    }

    pub fn set_cable(&mut self, on: bool) {
        self.cable = on;
        let c = self.refresh_present();
        let _ = c;
    }

    // ---------------------------------------------------------------- register file

    pub fn read8(&mut self, off: u32, usec: u32) -> u8 {
        if !self.clock_on() {
            self.stats.gated_accesses += 1;
            return 0;
        }
        let w = self.read_reg(off & 0x1fc, usec);
        (w >> ((off & 3) * 8)) as u8
    }

    pub fn write8(&mut self, off: u32, val: u8, usec: u32, dma: &mut dyn Dma) {
        self.now = usec;
        if !self.clock_on() {
            self.stats.gated_accesses += 1;
            return;
        }
        let lane = (off & 3) * 8;
        self.write_reg(off & 0x1fc, (val as u32) << lane, 0xff << lane, dma);
    }

    fn read_reg(&mut self, reg: u32, usec: u32) -> u32 {
        match reg {
            ID => 0xe241_fa05,
            HWGENERAL => 0,
            HWHOST => 0,
            HWDEVICE => 1 | (self.cfg.endpoints as u32) << 1,
            HWTXBUF => 0x8006_0a10,
            HWRXBUF => 0x0000_0808,
            CAPLENGTH => 0x0100_0040,
            DCIVERSION => 0x0001,
            DCCPARAMS => self.dccparams(),
            USBCMD => self.usbcmd,
            USBSTS => {
                let mut v = self.usbsts;
                if self.usbcmd & CMD_RS == 0 || self.usbmode & MODE_CM != MODE_CM_DEVICE {
                    v |= STS_HCH;
                }
                v
            }
            USBINTR => self.usbintr,
            FRINDEX => {
                if self.present {
                    (usec / 125) & 0x3fff
                } else {
                    0
                }
            }
            DEVICEADDR => {
                ((self.addr_staged.unwrap_or(self.addr) as u32) << 25)
                    | if self.addr_staged.is_some() {
                        1 << 24
                    } else {
                        0
                    }
            }
            ENDPOINTLISTADDR => self.listaddr,
            BURSTSIZE => self.burst,
            ULPI => self.ulpi,
            ENDPTNAK => self.nak,
            ENDPTNAKEN => self.nak_en,
            CONFIGFLAG => 1,
            PORTSC1 => self.portsc(),
            OTGSC => 0x20 | if self.cable { 0xa00 } else { 0 },
            USBMODE => self.usbmode,
            ENDPTSETUPSTAT => self.setupstat,
            ENDPTPRIME => 0,
            ENDPTFLUSH => 0,
            ENDPTSTAT => self.stat,
            ENDPTCOMPLETE => self.complete,
            r if (ENDPTCTRL..ENDPTCTRL + 64).contains(&r) => {
                let n = ((r - ENDPTCTRL) / 4) as usize;
                if n < self.cfg.endpoints as usize {
                    self.ctrl[n]
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    fn write_reg(&mut self, reg: u32, v: u32, mask: u32, dma: &mut dyn Dma) {
        let v = v & mask;
        let merge = |old: u32, writable: u32| (old & !(mask & writable)) | (v & writable);
        match reg {
            USBCMD => {
                if v & CMD_RST != 0 {
                    self.controller_reset("USBCMD.RST");
                    let changed = self.refresh_present();
                    self.notify_if_changed(changed);
                    return;
                }
                self.usbcmd = merge(self.usbcmd, CMD_WRITABLE);
                let changed = self.refresh_present();
                self.notify_if_changed(changed);
            }
            USBSTS => self.usbsts &= !(v & STS_W1C),
            USBINTR => self.usbintr = merge(self.usbintr, INTR_MASK),
            DEVICEADDR => {
                // Lane 3 carries the whole field, so the address is taken from the lane written.
                if mask & 0xff00_0000 != 0 {
                    let a = (v >> 25) as u8 & 0x7f;
                    if v & (1 << 24) != 0 {
                        self.addr_staged = Some(a);
                    } else {
                        self.addr_staged = None;
                        self.addr = a;
                    }
                }
            }
            ENDPOINTLISTADDR => self.listaddr = merge(self.listaddr, 0xffff_f800),
            BURSTSIZE => self.burst = merge(self.burst, 0xffff),
            ULPI => self.ulpi = merge(self.ulpi, 0xffff_ffff),
            ENDPTNAK => {
                self.nak &= !v;
                self.update_naki();
            }
            ENDPTNAKEN => {
                self.nak_en = merge(self.nak_en, 0xffff_ffff);
                self.update_naki();
            }
            PORTSC1 => {
                if v & PORT_CSC != 0 {
                    self.csc = false;
                }
                let was = self.portsc_rw;
                self.portsc_rw = merge(self.portsc_rw, PORT_RW);
                let _ = was;
            }
            OTGSC => {}
            USBMODE => {
                let mut m = self.usbmode;
                if m & MODE_CM == 0 {
                    m = (m & !MODE_CM) | (v & MODE_CM);
                }
                m = (m & !(mask & (MODE_WRITABLE & !MODE_CM))) | (v & MODE_WRITABLE & !MODE_CM);
                self.usbmode = m;
                let changed = self.refresh_present();
                self.notify_if_changed(changed);
            }
            ENDPTSETUPSTAT => self.setupstat &= !v,
            ENDPTPRIME => {
                for pipe in 0..(2 * self.cfg.endpoints as usize) {
                    if v & bit_of(pipe) != 0 {
                        self.stats.primes += 1;
                        if !self.prime_pipe(dma, pipe) {
                            self.stats.prime_refused += 1;
                        }
                    }
                }
            }
            ENDPTFLUSH => {
                self.note(format!("flush {v:#x}"));
                for pipe in 0..(2 * self.cfg.endpoints as usize) {
                    if v & bit_of(pipe) != 0 {
                        self.stat &= !bit_of(pipe);
                    }
                }
            }
            ENDPTCOMPLETE => self.complete &= !v,
            r if (ENDPTCTRL..ENDPTCTRL + 64).contains(&r) => {
                let n = ((r - ENDPTCTRL) / 4) as usize;
                if n < self.cfg.endpoints as usize {
                    for (dir, reset) in EP_TOGGLE_RESET.iter().enumerate() {
                        if v & reset != 0 {
                            self.tog[n][dir] = false;
                        }
                    }
                    let keep = !(mask & EP_CTRL_WRITABLE);
                    self.ctrl[n] = (self.ctrl[n] & keep) | (v & EP_CTRL_WRITABLE);
                    self.ctrl[n] &= !(EP_TOGGLE_RESET[0] | EP_TOGGLE_RESET[1]);
                }
            }
            _ => {}
        }
    }

    fn update_naki(&mut self) {
        if self.nak & self.nak_en != 0 {
            self.usbsts |= STS_NAKI;
        }
    }

    // ---------------------------------------------------------------- SoC glue

    /// One byte lane written to a SoC word; `backing` is the word as stored. The word the driver
    /// wrote is tracked here because the external memory bus model filters some stored bits (the
    /// pulses of `0x70000028` never stay), and the sequence checks need the writes, not the store.
    pub fn glue_lane(&mut self, addr: u32, lane: u32, val: u8, backing: u32) {
        let i = match self.shadow.iter().position(|e| e.0 == addr) {
            Some(i) => i,
            None => {
                self.shadow.push((addr, backing));
                self.shadow.len() - 1
            }
        };
        let sh = 8 * lane;
        let w = (self.shadow[i].1 & !(0xff << sh)) | (val as u32) << sh;
        self.shadow[i].1 = w;
        self.glue(addr, w);
    }

    /// A write to one of the SoC words the driver touches; `val` is the whole word afterwards.
    pub fn glue(&mut self, addr: u32, val: u32) {
        match addr {
            DEV_EN => {
                self.dev_en = val;
                let changed = self.refresh_present();
                self.notify_if_changed(changed);
            }
            DEV_RS => {
                let rose = val & !self.dev_rs;
                self.dev_rs = val;
                if rose & (DEV_USB0 | DEV_USB1) != 0 {
                    self.controller_reset("DEV_RS");
                    let changed = self.refresh_present();
                    self.notify_if_changed(changed);
                }
            }
            DEV_INIT2 => {
                self.dev_init2 = val;
                let changed = self.refresh_present();
                self.notify_if_changed(changed);
            }
            R28 => {
                let rose = val & !self.phy_prev28;
                self.phy_prev28 = val;
                if rose & (1 << 16) != 0 {
                    self.phy_event(PhyEvent::Pulse16);
                }
                if rose & (1 << 11) != 0 {
                    self.phy_event(PhyEvent::Pulse11);
                }
            }
            R60 if val != self.phy_prev60 => {
                self.phy_prev60 = val;
                self.phy_event(PhyEvent::Nibble(val & 0xf));
            }
            _ => {}
        }
    }

    fn phy_event(&mut self, ev: PhyEvent) {
        let want = PHY_SEQUENCE.get(self.phy_stage as usize).copied();
        if want == Some(ev) {
            self.phy_stage += 1;
            self.note(format!(
                "phy sequence step {} of {}",
                self.phy_stage, PHY_STAGES
            ));
            let changed = self.refresh_present();
            self.notify_if_changed(changed);
        }
    }

    pub fn phy_complete(&self) -> bool {
        self.phy_stage >= PHY_STAGES
    }

    // ---------------------------------------------------------------- queue heads, descriptors

    fn qh(&self, pipe: usize) -> u32 {
        (self.listaddr & 0xffff_f800).wrapping_add(64 * pipe as u32)
    }

    fn rd32(&mut self, dma: &mut dyn Dma, addr: u32) -> Option<u32> {
        let mut b = [0u8; 4];
        if dma.dma_read(addr, &mut b) {
            Some(u32::from_le_bytes(b))
        } else {
            self.dma_fault();
            None
        }
    }

    fn wr32(&mut self, dma: &mut dyn Dma, addr: u32, v: u32) -> bool {
        if dma.dma_write(addr, &v.to_le_bytes()) {
            true
        } else {
            self.dma_fault();
            false
        }
    }

    /// A queue head or descriptor the controller cannot reach is a bus error: it halts.
    fn dma_fault(&mut self) {
        self.stats.dma_faults += 1;
        self.usbsts |= STS_SEI;
        self.usbcmd &= !CMD_RS;
        let changed = self.refresh_present();
        self.notify_if_changed(changed);
    }

    fn ep_enabled(&self, ep: usize, dir: usize) -> bool {
        ep < self.cfg.endpoints as usize && self.ctrl[ep] & EP_ENABLE[dir] != 0
    }

    fn load_td(&mut self, dma: &mut dyn Dma, pipe: usize, td: u32) -> bool {
        let mut raw = [0u8; 28];
        if !dma.dma_read(td, &mut raw) {
            self.dma_fault();
            return false;
        }
        let w = |i: usize| u32::from_le_bytes(raw[4 * i..4 * i + 4].try_into().unwrap());
        let token = w(1);
        if token & TD_ACTIVE == 0 {
            return false;
        }
        let qh = self.qh(pipe);
        if !self.wr32(dma, qh + 4, td) || !dma.dma_write(qh + 8, &raw) {
            return false;
        }
        let bufs = [w(2), w(3), w(4), w(5), w(6)];
        self.pipes[pipe] = Pipe {
            td,
            bufs,
            off0: bufs[0] & 0xfff,
            done: 0,
            rem: (token >> 16) & 0x7fff,
            token,
            zlp: false,
        };
        self.stat |= bit_of(pipe);
        self.usbcmd &= !CMD_ATDTW;
        true
    }

    fn prime_pipe(&mut self, dma: &mut dyn Dma, pipe: usize) -> bool {
        let r = self.prime_pipe_inner(dma, pipe);
        self.note(format!(
            "prime pipe {pipe}: {}",
            if r { "ok" } else { "refused" }
        ));
        r
    }

    fn prime_pipe_inner(&mut self, dma: &mut dyn Dma, pipe: usize) -> bool {
        let (ep, dir) = (pipe / 2, pipe % 2);
        if !self.ep_enabled(ep, dir) {
            return false;
        }
        if self.stat & bit_of(pipe) != 0 {
            return true;
        }
        let qh = self.qh(pipe);
        let (Some(next), Some(token)) = (self.rd32(dma, qh + 8), self.rd32(dma, qh + 12)) else {
            return false;
        };
        if token & TD_HALTED != 0 || next & 1 != 0 {
            return false;
        }
        self.load_td(dma, pipe, next & !0x1f)
    }

    fn page_addr(&self, pipe: usize, at: u32) -> u32 {
        let p = &self.pipes[pipe];
        let abs = p.off0 + at;
        let page = ((abs >> 12) as usize).min(4);
        (p.bufs[page] & !0xfff) + (abs & 0xfff)
    }

    fn span_read(&mut self, dma: &mut dyn Dma, pipe: usize, n: u32) -> Option<Vec<u8>> {
        let mut out = vec![0u8; n as usize];
        let mut i = 0usize;
        let done = self.pipes[pipe].done;
        while i < out.len() {
            let a = self.page_addr(pipe, done + i as u32);
            let chunk = (out.len() - i).min(0x1000 - (a & 0xfff) as usize);
            if !dma.dma_read(a, &mut out[i..i + chunk]) {
                self.dma_fault_td();
                return None;
            }
            i += chunk;
        }
        Some(out)
    }

    fn span_write(&mut self, dma: &mut dyn Dma, pipe: usize, data: &[u8]) -> bool {
        let mut i = 0usize;
        let done = self.pipes[pipe].done;
        while i < data.len() {
            let a = self.page_addr(pipe, done + i as u32);
            let chunk = (data.len() - i).min(0x1000 - (a & 0xfff) as usize);
            if !dma.dma_write(a, &data[i..i + chunk]) {
                self.dma_fault_td();
                return false;
            }
            i += chunk;
        }
        true
    }

    /// A data buffer the controller cannot reach is a descriptor error, not a bus error.
    fn dma_fault_td(&mut self) {
        self.stats.dma_faults += 1;
    }

    /// Retire the descriptor in flight on `pipe`, with `err` status bits (0 for success).
    fn retire(&mut self, dma: &mut dyn Dma, pipe: usize, err: u32) {
        let p = self.pipes[pipe];
        let qh = self.qh(pipe);
        self.note(format!("retire pipe {pipe} err {err:#x}"));
        let token = (p.token & !(0x7fff_0000 | 0xff)) | (p.rem << 16) | err;
        self.wr32(dma, p.td + 4, token);
        self.wr32(dma, qh + 12, token);
        self.complete |= bit_of(pipe);
        if p.token & TD_IOC != 0 {
            self.usbsts |= STS_UI;
        }
        if err != 0 {
            self.stats.td_errors += 1;
            self.usbsts |= STS_UEI;
            self.stat &= !bit_of(pipe);
            return;
        }
        let next = self.rd32(dma, p.td).unwrap_or(1);
        if next & 1 == 0 && self.load_td(dma, pipe, next & !0x1f) {
            return;
        }
        self.stat &= !bit_of(pipe);
        self.usbcmd &= !CMD_ATDTW;
    }

    // ---------------------------------------------------------------- the bus

    fn note_nak(&mut self, ep: usize, dir: usize) {
        let st = self.stat;
        self.note(format!("nak ep {ep} dir {dir} stat {st:#x}"));
        self.nak |= 1 << (ep + 16 * dir);
        self.update_naki();
    }

    fn result(&self, ep: u8, aux: u8) -> Frame {
        let mut f = Frame::new(usbbus::RESULT);
        f.ep = ep;
        f.aux = aux;
        f
    }

    fn addressed(&self, f: &Frame) -> bool {
        self.present && self.reset_due.is_none() && f.addr == self.addr
    }

    fn do_setup(&mut self, dma: &mut dyn Dma, f: &Frame) -> Frame {
        let ep = (f.ep & 0xf) as usize;
        if !self.addressed(f)
            || ep >= self.cfg.endpoints as usize
            || self.ctrl[ep] & EP_ENABLE[0] == 0
            || f.data.len() != 8
        {
            self.stats.timeouts += 1;
            return self.result(f.ep, usbbus::TIMEOUT);
        }
        if self.usbmode & MODE_SLOM == 0 && self.setupstat & (1 << ep) != 0 {
            self.stats.setups_nak += 1;
            return self.result(f.ep, usbbus::NAK);
        }
        self.stat &= !((1 << ep) | (1 << (ep + 16)));
        self.ctrl[ep] &= !(EP_STALL_RX | EP_STALL_TX);
        let qh = self.qh(2 * ep);
        if !dma.dma_write(qh + QH_SETUP, &f.data) {
            self.dma_fault();
            return self.result(f.ep, usbbus::TIMEOUT);
        }
        self.setupstat |= 1 << ep;
        self.usbcmd &= !CMD_SUTW;
        self.tog[ep] = [true, true];
        self.stats.setups += 1;
        let cap = self.rd32(dma, qh).unwrap_or(0);
        if cap & QH_IOS != 0 {
            self.usbsts |= STS_UI;
        }
        self.result(f.ep, usbbus::ACK)
    }

    fn do_out(&mut self, dma: &mut dyn Dma, f: &Frame) -> Frame {
        let ep = (f.ep & 0xf) as usize;
        if !self.addressed(f) || !self.ep_enabled(ep, 0) {
            self.stats.timeouts += 1;
            return self.result(f.ep, usbbus::TIMEOUT);
        }
        if self.ctrl[ep] & EP_STALL_RX != 0 {
            self.stats.stalls += 1;
            return self.result(f.ep, usbbus::STALL);
        }
        let pipe = 2 * ep;
        if self.stat & bit_of(pipe) == 0 {
            self.stats.out_nak += 1;
            self.note_nak(ep, 0);
            return self.result(f.ep, usbbus::NAK);
        }
        if f.flags & 1 != 0 {
            self.retire(dma, pipe, TD_HALTED | TD_XACTERR);
            self.stats.timeouts += 1;
            return self.result(f.ep, usbbus::TIMEOUT);
        }
        let want = f.aux & 1 != 0;
        if self.ctrl[ep] & EP_TOGGLE_INHIBIT[0] == 0 && want != self.tog[ep][0] {
            self.stats.out_dup += 1;
            return self.result(f.ep, usbbus::ACK);
        }
        let qh = self.qh(pipe);
        let mps = (self.rd32(dma, qh).unwrap_or(0) >> 16) & 0x7ff;
        let len = f.data.len() as u32;
        let p = self.pipes[pipe];
        if len > mps.max(1) || len > p.rem {
            self.retire(dma, pipe, TD_HALTED | TD_BUFERR);
            return self.result(f.ep, usbbus::ACK);
        }
        if len > 0 && !self.span_write(dma, pipe, &f.data) {
            self.retire(dma, pipe, TD_HALTED | TD_BUFERR);
            return self.result(f.ep, usbbus::ACK);
        }
        self.tog[ep][0] = !self.tog[ep][0];
        let pp = &mut self.pipes[pipe];
        pp.done += len;
        pp.rem -= len;
        self.stats.out_ok += 1;
        if self.pipes[pipe].rem == 0 || len < mps {
            self.retire(dma, pipe, 0);
        } else {
            let t = (p.token & !(0x7fff_0000)) | (self.pipes[pipe].rem << 16);
            self.wr32(dma, qh + 12, t);
        }
        self.result(f.ep, usbbus::ACK)
    }

    fn do_in(&mut self, dma: &mut dyn Dma, f: &Frame) -> Frame {
        let ep = (f.ep & 0xf) as usize;
        if !self.addressed(f) || !self.ep_enabled(ep, 1) {
            self.stats.timeouts += 1;
            return self.result(f.ep, usbbus::TIMEOUT);
        }
        if self.ctrl[ep] & EP_STALL_TX != 0 {
            self.stats.stalls += 1;
            return self.result(f.ep, usbbus::STALL);
        }
        let pipe = 2 * ep + 1;
        if self.stat & bit_of(pipe) == 0 {
            self.stats.in_nak += 1;
            self.note_nak(ep, 1);
            return self.result(f.ep, usbbus::NAK);
        }
        if f.flags & 1 != 0 {
            self.retire(dma, pipe, TD_HALTED | TD_XACTERR);
            self.stats.timeouts += 1;
            return self.result(f.ep, usbbus::TIMEOUT);
        }
        let qh = self.qh(pipe);
        let cap = self.rd32(dma, qh).unwrap_or(0);
        let mps = (cap >> 16) & 0x7ff;
        let p = self.pipes[pipe];
        let n = p.rem.min(mps);
        let data = if n > 0 {
            match self.span_read(dma, pipe, n) {
                Some(d) => d,
                None => {
                    self.retire(dma, pipe, TD_HALTED | TD_BUFERR);
                    self.stats.timeouts += 1;
                    return self.result(f.ep, usbbus::TIMEOUT);
                }
            }
        } else {
            Vec::new()
        };
        let toggle = self.tog[ep][1];
        self.tog[ep][1] = !toggle;
        let pp = &mut self.pipes[pipe];
        pp.done += n;
        pp.rem -= n;
        let (rem, was_zlp) = (pp.rem, pp.zlp);
        self.stats.in_ok += 1;
        let needs_zlp = rem == 0 && n == mps && n > 0 && cap & QH_ZLT_OFF == 0 && !was_zlp;
        if needs_zlp {
            self.pipes[pipe].zlp = true;
            let t = p.token & !0x7fff_0000;
            self.wr32(dma, qh + 12, t);
        } else if rem == 0 {
            self.retire(dma, pipe, 0);
        } else {
            let t = (p.token & !0x7fff_0000) | (rem << 16);
            self.wr32(dma, qh + 12, t);
        }
        if ep == 0 {
            if let Some(a) = self.addr_staged.take() {
                self.addr = a;
            }
        }
        let mut r = self.result(f.ep, usbbus::ACK);
        r.flags = toggle as u8;
        r.data = data;
        r
    }

    fn begin_reset(&mut self, hs: bool) {
        self.stats.bus_resets += 1;
        self.note("bus reset".into());
        self.bus_reset_effects();
        self.usbsts |= STS_URI;
        self.reset_due = Some(self.now.wrapping_add(RESET_USEC));
        self.reset_hs = hs;
    }

    fn finish_reset(&mut self) {
        self.reset_due = None;
        let hs = self.reset_hs && !self.cfg.full_speed && self.portsc_rw & PORT_PFSC == 0;
        self.speed = if hs { 2 } else { 0 };
        self.usbsts |= STS_PCI;
        let mut f = self.result(0xff, usbbus::ACK);
        f.flags = self.speed;
        self.send(f);
    }

    /// Answer one request from the host. `None` for a request answered later (a bus reset).
    pub fn handle(&mut self, dma: &mut dyn Dma, f: Frame) -> Option<Frame> {
        match f.kind {
            usbbus::ATTACH => {
                self.set_cable(true);
                Some(self.state_frame())
            }
            usbbus::DETACH => {
                self.set_cable(false);
                Some(self.state_frame())
            }
            usbbus::GET_STATE => Some(self.state_frame()),
            usbbus::QUIT => {
                self.quit = true;
                Some(self.state_frame())
            }
            usbbus::RESET => {
                if !self.present || self.reset_due.is_some() {
                    self.stats.timeouts += 1;
                    let mut r = self.result(0xff, usbbus::TIMEOUT);
                    r.flags = 0;
                    return Some(r);
                }
                self.begin_reset(f.aux & 1 != 0);
                None
            }
            usbbus::SETUP => Some(self.do_setup(dma, &f)),
            usbbus::OUT => Some(self.do_out(dma, &f)),
            usbbus::IN => Some(self.do_in(dma, &f)),
            _ => None,
        }
    }

    /// One service tick: finish a reset that is due and take the frames the host has sent.
    pub fn service(&mut self, usec: u32, dma: &mut dyn Dma) {
        self.now = usec;
        if let Some(due) = self.reset_due {
            if usec.wrapping_sub(due) < 0x8000_0000 {
                self.finish_reset();
            }
        }
        if self.usbintr & INTR_SRE != 0 && self.present {
            let frame = usec / if self.speed == 2 { 125 } else { 1000 };
            if frame != self.last_frame {
                self.last_frame = frame;
                self.usbsts |= STS_SRI;
            }
        }
        loop {
            let Some(link) = &self.link else { return };
            if !link.pending() {
                return;
            }
            match link.pop() {
                None => return,
                Some(Event::Connected) => {
                    let f = self.state_frame();
                    self.send(f);
                }
                Some(Event::Disconnected) => {
                    self.set_cable(false);
                }
                Some(Event::Frame(f)) => {
                    if let Some(r) = self.handle(dma, f) {
                        self.send(r);
                    }
                    if self.quit {
                        return;
                    }
                }
            }
        }
    }

    pub fn report(&self) -> Vec<String> {
        let s = &self.stats;
        vec![
            format!(
                "usb: {} endpoints, {} speed, cable {}, port {}, phy sequence {}",
                self.cfg.endpoints,
                if self.speed == 2 { "high" } else { "full" },
                if self.cable { "in" } else { "out" },
                if self.present { "connected" } else { "disconnected" },
                if self.phy_complete() {
                    "complete".to_string()
                } else {
                    format!("incomplete ({} of {} steps)", self.phy_stage, PHY_STAGES)
                }
            ),
            format!(
                "usb: {} accesses with the clock off, {} controller resets, {} bus resets, {} attach, {} detach",
                s.gated_accesses, s.controller_resets, s.bus_resets, s.attaches, s.detaches
            ),
            format!(
                "usb: setup {} ({} nak), out {} ({} nak, {} dup), in {} ({} nak), {} stall, {} timeout",
                s.setups, s.setups_nak, s.out_ok, s.out_nak, s.out_dup, s.in_ok, s.in_nak, s.stalls, s.timeouts
            ),
            format!(
                "usb: {} primes ({} refused), {} descriptor errors, {} dma faults",
                s.primes, s.prime_refused, s.td_errors, s.dma_faults
            ),
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PhyEvent {
    Nibble(u32),
    Pulse16,
    Pulse11,
}

/// The PP5022 sequence of `pp502x-usb-arc.md` section 2.3, reduced to its distinguishing steps.
const PHY_SEQUENCE: [PhyEvent; 5] = [
    PhyEvent::Nibble(0xf),
    PhyEvent::Pulse16,
    PhyEvent::Nibble(0x4),
    PhyEvent::Nibble(0x0),
    PhyEvent::Pulse11,
];
const PHY_STAGES: u8 = PHY_SEQUENCE.len() as u8;

#[cfg(test)]
mod tests {
    use super::*;

    struct Ram(Vec<u8>);

    impl Dma for Ram {
        fn dma_read(&mut self, addr: u32, out: &mut [u8]) -> bool {
            let a = addr as usize;
            match self.0.get(a..a + out.len()) {
                Some(s) => {
                    out.copy_from_slice(s);
                    true
                }
                None => false,
            }
        }
        fn dma_write(&mut self, addr: u32, data: &[u8]) -> bool {
            let a = addr as usize;
            match self.0.get_mut(a..a + data.len()) {
                Some(s) => {
                    s.copy_from_slice(data);
                    true
                }
                None => false,
            }
        }
    }

    fn w32(u: &mut Usb, ram: &mut Ram, reg: u32, v: u32) {
        for i in 0..4 {
            u.write8(reg + i, (v >> (8 * i)) as u8, 0, ram);
        }
    }

    fn r32(u: &mut Usb, reg: u32) -> u32 {
        (0..4).fold(0, |a, i| a | (u.read8(reg + i, 0) as u32) << (8 * i))
    }

    fn up(eps: u8) -> (Usb, Ram) {
        let mut u = Usb::new(Config {
            endpoints: eps,
            ..Config::default()
        });
        let mut ram = Ram(vec![0; 0x10000]);
        u.glue(DEV_EN, DEV_USB0);
        u.glue(DEV_INIT2, INIT_USB);
        w32(&mut u, &mut ram, USBCMD, CMD_RST);
        w32(&mut u, &mut ram, USBMODE, 2);
        w32(&mut u, &mut ram, ENDPOINTLISTADDR, 0x800);
        w32(&mut u, &mut ram, USBCMD, CMD_RS);
        u.set_cable(true);
        (u, ram)
    }

    fn reset_host(u: &mut Usb, ram: &mut Ram) {
        let mut f = Frame::new(usbbus::RESET);
        f.aux = 1;
        assert!(u.handle(ram, f).is_none());
        u.service(10_000, ram);
    }

    #[test]
    fn capability_registers_report_the_configured_endpoint_count() {
        let (mut u, _) = up(3);
        assert_eq!(r32(&mut u, DCCPARAMS) & 0x1f, 3);
        assert_eq!(r32(&mut u, HWDEVICE) >> 1 & 0x1f, 3);
    }

    #[test]
    fn registers_are_gated_without_the_clock() {
        let mut u = Usb::new(Config::default());
        assert_eq!(r32(&mut u, DCCPARAMS), 0);
        assert!(u.stats.gated_accesses > 0);
    }

    #[test]
    fn the_port_connects_only_with_cable_run_and_device_mode() {
        let (mut u, mut ram) = up(4);
        assert!(u.present);
        assert_eq!(r32(&mut u, PORTSC1) & PORT_CCS, PORT_CCS);
        w32(&mut u, &mut ram, USBCMD, 0);
        assert!(!u.present);
        assert_eq!(r32(&mut u, PORTSC1) & PORT_CCS, 0);
    }

    #[test]
    fn run_has_no_effect_before_the_mode_is_written() {
        let mut u = Usb::new(Config::default());
        let mut ram = Ram(vec![0; 0x1000]);
        u.glue(DEV_EN, DEV_USB0);
        u.glue(DEV_INIT2, INIT_USB);
        w32(&mut u, &mut ram, USBCMD, CMD_RS);
        u.set_cable(true);
        assert!(!u.present);
    }

    #[test]
    fn a_bus_reset_reports_speed_and_clears_the_address() {
        let (mut u, mut ram) = up(4);
        w32(&mut u, &mut ram, DEVICEADDR, 5 << 25);
        reset_host(&mut u, &mut ram);
        assert_eq!(r32(&mut u, DEVICEADDR), 0);
        assert_eq!(r32(&mut u, USBSTS) & STS_URI, STS_URI);
        assert_eq!(r32(&mut u, PORTSC1) >> 26 & 3, 2);
    }

    #[test]
    fn full_speed_is_forced_by_the_option() {
        let mut u = Usb::new(Config {
            full_speed: true,
            ..Config::default()
        });
        let mut ram = Ram(vec![0; 0x1000]);
        u.glue(DEV_EN, DEV_USB0);
        u.glue(DEV_INIT2, INIT_USB);
        w32(&mut u, &mut ram, USBMODE, 2);
        w32(&mut u, &mut ram, USBCMD, CMD_RS);
        u.set_cable(true);
        reset_host(&mut u, &mut ram);
        assert_eq!(r32(&mut u, PORTSC1) >> 26 & 3, 0);
    }

    fn enable(u: &mut Usb, ram: &mut Ram, ep: u32, rx: bool, tx: bool) {
        let v = if rx { 0x80 | 0x08 } else { 0 } | if tx { 0x80_0000 | 0x08_0000 } else { 0 };
        w32(u, ram, ENDPTCTRL + 4 * ep, v);
    }

    fn put_td(ram: &mut Ram, at: u32, next: u32, bytes: u32, ioc: bool, buf: u32) {
        let tok = (bytes << 16) | if ioc { TD_IOC } else { 0 } | TD_ACTIVE;
        for (i, w) in [next, tok, buf, 0, 0, 0, 0, 0].iter().enumerate() {
            ram.0[(at + 4 * i as u32) as usize..][..4].copy_from_slice(&w.to_le_bytes());
        }
    }

    fn set_qh(ram: &mut Ram, qh: u32, mps: u32, next: u32) {
        ram.0[qh as usize..][..4].copy_from_slice(&(mps << 16 | QH_ZLT_OFF).to_le_bytes());
        ram.0[(qh + 8) as usize..][..4].copy_from_slice(&next.to_le_bytes());
    }

    #[test]
    fn setup_lands_in_the_queue_head_and_in_with_data_completes() {
        let (mut u, mut ram) = up(4);
        reset_host(&mut u, &mut ram);
        set_qh(&mut ram, 0x800, 64, 1);
        set_qh(&mut ram, 0x840, 64, 1);
        let mut f = Frame::new(usbbus::SETUP);
        f.data = vec![0x80, 6, 0, 1, 0, 0, 18, 0];
        let r = u.handle(&mut ram, f.clone()).unwrap();
        assert_eq!(r.aux, usbbus::ACK);
        assert_eq!(&ram.0[0x828..0x830], &f.data[..]);
        assert_eq!(r32(&mut u, ENDPTSETUPSTAT), 1);
        let again = u.handle(&mut ram, f).unwrap();
        assert_eq!(again.aux, usbbus::NAK, "setup lockout");
        w32(&mut u, &mut ram, ENDPTSETUPSTAT, 1);

        for (i, b) in ram.0[0x2000..0x2012].iter_mut().enumerate() {
            *b = i as u8;
        }
        put_td(&mut ram, 0x1000, 1, 18, true, 0x2000);
        ram.0[0x848..0x84c].copy_from_slice(&0x1000u32.to_le_bytes());
        w32(&mut u, &mut ram, ENDPTPRIME, 1 << 16);
        assert_eq!(r32(&mut u, ENDPTSTAT), 1 << 16);
        let mut inf = Frame::new(usbbus::IN);
        let r = u.handle(&mut ram, inf.clone()).unwrap();
        assert_eq!(r.aux, usbbus::ACK);
        assert_eq!(r.data.len(), 18);
        assert_eq!(r.flags, 1, "data stage starts with DATA1");
        assert_eq!(r32(&mut u, ENDPTSTAT), 0);
        assert_eq!(r32(&mut u, ENDPTCOMPLETE), 1 << 16);
        assert_eq!(r32(&mut u, USBSTS) & STS_UI, STS_UI);
        let tok = u32::from_le_bytes(ram.0[0x1004..0x1008].try_into().unwrap());
        assert_eq!(tok & 0xff, 0);
        assert_eq!(tok >> 16 & 0x7fff, 0);
        inf.flags = 0;
        assert_eq!(u.handle(&mut ram, inf).unwrap().aux, usbbus::NAK);
    }

    #[test]
    fn out_with_a_short_packet_leaves_the_remainder_in_the_descriptor() {
        let (mut u, mut ram) = up(4);
        reset_host(&mut u, &mut ram);
        enable(&mut u, &mut ram, 1, true, true);
        set_qh(&mut ram, 0x880, 512, 0x1000);
        put_td(&mut ram, 0x1000, 1, 512, true, 0x3000);
        w32(&mut u, &mut ram, ENDPTPRIME, 1 << 1);
        let mut f = Frame::new(usbbus::OUT);
        f.ep = 1;
        f.data = vec![7; 100];
        assert_eq!(u.handle(&mut ram, f.clone()).unwrap().aux, usbbus::ACK);
        assert_eq!(&ram.0[0x3000..0x3064], &[7u8; 100][..]);
        let tok = u32::from_le_bytes(ram.0[0x1004..0x1008].try_into().unwrap());
        assert_eq!(tok >> 16 & 0x7fff, 412);
        assert_eq!(tok & TD_ACTIVE, 0);
        assert_eq!(r32(&mut u, ENDPTSTAT), 0);
    }

    #[test]
    fn a_wrong_toggle_is_acknowledged_and_dropped() {
        let (mut u, mut ram) = up(4);
        reset_host(&mut u, &mut ram);
        enable(&mut u, &mut ram, 1, true, true);
        set_qh(&mut ram, 0x880, 64, 0x1000);
        put_td(&mut ram, 0x1000, 1, 64, true, 0x3000);
        w32(&mut u, &mut ram, ENDPTPRIME, 1 << 1);
        let mut f = Frame::new(usbbus::OUT);
        f.ep = 1;
        f.aux = 1;
        f.data = vec![1; 64];
        assert_eq!(u.handle(&mut ram, f.clone()).unwrap().aux, usbbus::ACK);
        assert_eq!(r32(&mut u, ENDPTSTAT), 1 << 1, "still primed");
        assert_eq!(u.stats.out_dup, 1);
        f.aux = 0;
        assert_eq!(u.handle(&mut ram, f).unwrap().aux, usbbus::ACK);
        assert_eq!(r32(&mut u, ENDPTSTAT), 0);
    }

    #[test]
    fn an_injected_error_halts_the_descriptor() {
        let (mut u, mut ram) = up(4);
        reset_host(&mut u, &mut ram);
        enable(&mut u, &mut ram, 1, true, true);
        set_qh(&mut ram, 0x880, 64, 0x1000);
        put_td(&mut ram, 0x1000, 1, 64, true, 0x3000);
        w32(&mut u, &mut ram, ENDPTPRIME, 1 << 1);
        let mut f = Frame::new(usbbus::OUT);
        f.ep = 1;
        f.flags = 1;
        f.data = vec![1; 64];
        assert_eq!(u.handle(&mut ram, f).unwrap().aux, usbbus::TIMEOUT);
        let tok = u32::from_le_bytes(ram.0[0x1004..0x1008].try_into().unwrap());
        assert_eq!(tok & 0xff, TD_HALTED | TD_XACTERR);
        assert_eq!(r32(&mut u, USBSTS) & STS_UEI, STS_UEI);
        assert_eq!(r32(&mut u, ENDPTSTAT), 0);
        assert_eq!(r32(&mut u, ENDPTCOMPLETE), 1 << 1, "the pipe completes");
    }

    #[test]
    fn a_stalled_endpoint_answers_stall_and_setup_clears_it() {
        let (mut u, mut ram) = up(4);
        reset_host(&mut u, &mut ram);
        set_qh(&mut ram, 0x800, 64, 1);
        w32(
            &mut u,
            &mut ram,
            ENDPTCTRL,
            EP0_RESET | EP_STALL_TX | EP_STALL_RX,
        );
        assert_eq!(
            u.handle(&mut ram, Frame::new(usbbus::IN)).unwrap().aux,
            usbbus::STALL
        );
        let mut s = Frame::new(usbbus::SETUP);
        s.data = vec![0; 8];
        u.handle(&mut ram, s).unwrap();
        assert_eq!(r32(&mut u, ENDPTCTRL) & (EP_STALL_TX | EP_STALL_RX), 0);
    }

    #[test]
    fn the_address_takes_effect_for_the_next_transaction() {
        let (mut u, mut ram) = up(4);
        reset_host(&mut u, &mut ram);
        w32(&mut u, &mut ram, DEVICEADDR, 9 << 25);
        let mut f = Frame::new(usbbus::SETUP);
        f.data = vec![0; 8];
        set_qh(&mut ram, 0x800, 64, 1);
        assert_eq!(u.handle(&mut ram, f.clone()).unwrap().aux, usbbus::TIMEOUT);
        f.addr = 9;
        assert_eq!(u.handle(&mut ram, f).unwrap().aux, usbbus::ACK);
    }

    #[test]
    fn the_phy_sequence_is_recognised_in_order() {
        let mut u = Usb::new(Config::default());
        u.glue(R60, 0xf);
        u.glue(R28, 1 << 16);
        u.glue(R28, 0);
        u.glue(R60, 0x4);
        assert!(!u.phy_complete());
        u.glue(R60, 0);
        u.glue(R28, 1 << 11);
        assert!(u.phy_complete());
    }

    #[test]
    fn an_unreachable_queue_head_halts_the_controller() {
        let (mut u, mut ram) = up(4);
        w32(&mut u, &mut ram, ENDPOINTLISTADDR, 0xf000_0000);
        enable(&mut u, &mut ram, 1, true, true);
        w32(&mut u, &mut ram, ENDPTPRIME, 1 << 1);
        assert_eq!(r32(&mut u, USBSTS) & STS_SEI, STS_SEI);
        assert_eq!(r32(&mut u, USBCMD) & CMD_RS, 0);
    }
}
