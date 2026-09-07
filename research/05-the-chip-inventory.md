# The 5.5G logic board — what is on it, what we emulate, and what is left

**Model numbers, because a wrong one has been circulating in our own notes.** The 5.5G is
**A1136, EMC 2065 — the same A-number and EMC as the original 5G**
([EveryMac](https://everymac.com/systems/apple/ipod/specs/ipod-5th-generation-enhanced-specs.html)).
**A1238 (EMC 2173) is the iPod classic 6G/7G**, a different platform (Samsung S5L8702)
([EveryMac](https://everymac.com/systems/apple/ipod/specs/ipod-classic-6th-generation-specs.html)).
"5.5G" is community shorthand; Apple's name is *iPod (5th generation Late 2006)*. Since 5G and 5.5G
share A1136/EMC 2065 and differ only in HDD and SDRAM density, **treat them as one hardware target**
— Apple, Rockbox and iPodLinux all do.

**Question this file answers:** which physical chips does an iPod 5.5G (A1136) have, what does each
do, which are on the critical path for booting RetailOS and running games, and how much of that have
we built?

Primary evidence is preferred throughout: **our own NOR dump and RetailOS image beat any secondary
source**, and where they disagree with a wiki, they win. This project has twice been misled by
confident recall, so claims here carry their evidence inline.

---

## 1. FireWire — charging yes, data no, and the firmware says so itself

**Answer: the 5.5G charges over FireWire and cannot sync over it.** There is no FireWire data stack
in the shipping software, and RetailOS ships a localized user-facing message saying so.

This did not need a teardown. It is in the binaries we already have.

**From the NOR flash** (Apple's bootloader and its diagnostics):

```
FirewireCharge          FirewireNoCharge         FirewireTest
Firewire Charge Test    Firewire No Charge       USBPLL
```

The ROM detects FireWire *power* and has a factory diagnostic for it. Searching the same 1 MB dump
for `1394`, `sbp`, `ohci` — the strings any FireWire *data* stack would carry — returns **nothing**.

**From RetailOS**, in English and shipped to users:

> **"FireWire connections are not supported. To transfer songs, connect the USB cable provided."**

Localized into at least Italian, Polish, Spanish, Dutch, Hungarian, German and Swedish — Apple
translated this message for every market, which is what you do for a message users will actually
hit. The Italian is unambiguous: *"Connessioni di dati via FireWire non sono supportate."*

So the dock connector's FireWire pins are wired for power only. **No FireWire PHY/link chip needs
emulating**, because there is no FireWire data hardware to emulate and no firmware that would talk
to it.

### The GUID is not evidence of a FireWire port

A trap worth naming, because we walked past it. The iPod reports a **FireWire GUID**
(`0x000A2700195D4E9C`, OUI `000A27` = Apple) and a **`FireWireVersion` of 1.62**, and that GUID *is*
the USB serial number — see the USB research *(not published)* § "The FireWire GUID *is* the USB
serial number." None of that implies a FireWire port. Apple kept the GUID as the device's stable
64-bit identity after the bus it was named for was gone.

Nor is iTunes' `sbp2` string evidence: it lives in iTunes' own Windows binary
(`DeviceManagement\PnpDiskUtil.cpp`), where `sbp2` is how Windows names FireWire storage in a device
instance ID. That is iTunes being able to talk to *older* FireWire iPods, not a statement about this
one. It is quoted in the USB research *(not published)* and was briefly misread here as flash
content before the surrounding context was checked.

---

## 2. What we emulate today — measured, not claimed

Per-region access counts from a real cold boot (`cold-boot.sh --devices`, 150M instructions), which
is also the best available answer to *which hardware actually matters*:

| region | base | reads | writes | modelled? |
|---|---|---|---|---|
| `osos` (RetailOS image) | `0x10000000` | 866 M | 9 740 | storage |
| `iram` | `0x40000000` | 376 M | 2.0 M | storage, 128 KB |
| `sdram` | `0x10000000` | 104 M | 99 M | storage, 64 MB + uncached alias |
| `mmio-6` | `0x60000000` | **13.0 M** | **4.9 M** | **partly** — the system controller |
| `flash-low` | `0x00000000` | 703 k | 40 | NOR, read-only at reset |
| `lcd` / BCM | `0x30000000` | 133 k | 429 k | transport only; replies synthesised |
| `stack` | `0x11000000` | 102 k | 102 k | storage |
| `mmio-7` | `0x70000000` | 26 k | 16 k | partly — memory controller + I²C at `0x7000c000` |
| `ide` | `0xc3000000` | 10 k | 3 k | **yes** — taskfile + bus-master DMA |
| `cache` | `0xf0000000` | 36 | 2 144 | MMAP unit yes; cache behaviour no |

**The shape of the work is in that table.** `0x60000000` takes four times the MMIO traffic of
everything else combined, and it is the one block we have built piecemeal — timers and the interrupt
controller are real, the rest is read-overrides.

Built and trustworthy:

- **ARM7TDMI core** — fuzz-verified against a reference implementation.
- **Memory model** — SDRAM sizing, the uncached alias, and the **MMAP unit** (8 windows, encoding
  decoded from Rockbox — see [research/03](03-rtxc-and-the-video-coprocessor.md) §33).
- **Timers + interrupt controller** — the firmware programs its own ~1 kHz tick and we deliver it.
- **ATA** — `IDENTIFY`, `SET FEATURES`, PIO reads, and **bus-master DMA**, which is what loads the
  7.5 MB image.
- **I²C transport** at `0x7000c000` — the bus is real; the device on the far end is not.
- **BCM transport** — address latching, data window and command encoding are real.

Not built at all: **click wheel**, **audio codec**, **USB device controller**, **the second CPU
core**, **cache behaviour**, **the PMU as a device**, **the VideoCore as a processor**.

---

## 3. The chip inventory

Board is **820-1975-A** for every 5.5G (Late 2006); the 2005 5G is **820-1763-A**
([Elite Obsolete Electronics board table](https://eoe.works/pages/logic-board-infomation)).

Evidence tiers below: **[P]** silkscreen read off an archived teardown photo · **[T]** named by a text
source (teardown article, wiki) · **[U]** unverified.

| # | Chip | Marking | Function | Boot-critical? |
|---|---|---|---|---|
| 1 | PortalPlayer SoC | **PP5021C-TDF** [T] | Dual ARM7TDMI, ≤80 MHz, 128 KB IRAM; hosts ATA, USB, I²C, I²S, click-wheel, piezo, timers, IRQ | **Yes — it is the machine** |
| 2 | Broadcom | **`BCM2722MB1KFBG`** [P] | VideoCore II multimedia coprocessor; **is the display controller** and TV-out | **Yes for display** |
| 3 | Wolfson | **WM8758** (`WM8758BG` [T]) | Stereo codec: DAC/ADC, **integrated headphone amp**, lineout, hardware EQ | No — audio only |
| 4 | NXP/Philips | **PCF50605** [T] | PMU + RTC: rails, charging, ADC, wake flags | **Yes in hardware** |
| 5 | SDRAM | Samsung **`K4M51163PC`** (64 MB) [P] · **`K4M56163PG`** (32 MB) [U] | 32 MB (30 GB) / 64 MB (80 GB) at `0x10000000` | **Yes** |
| 6 | NOR flash | SST **`39WF800A`** [T×2] | 1 MB: bootloader, disk mode, diagnostics, **BCM `vmcs` blob** | **Yes** |
| 7 | Cypress PSoC | **`CY8C214…`** [T] (`CY8C21434` [U]) | Click-wheel + capacitive sensing, on the wheel flex | No |
| 8 | LCD panel | Toshiba-Matsushita `1WX510015194` [P] | 320×240 panel. **Not a controller** — see below | **Yes for display** |
| 9 | HDD (ATA/ZIF) | Toshiba, varies | Holds RetailOS in the boot partition | **Yes** |
| 10 | National Semi | **`LM34910B`** [T] | Step-down switching regulator | Rail — yes in hardware |
| 11 | NXP | **`T1211`** [T] | **Second** power-management chip, function unknown | Unknown |
| 12 | Linear Tech | `LTC4066` [T, medium] | USB power manager / charger | Rail — yes in hardware |

**There is a published BOM for the 5.5G**, which is the single best secondary source we have. EE Times
on the Wedbush Morgan teardown, verbatim —
[teardown-finds-few-changes-to-new-video-ipod](https://www.eetimes.com/teardown-finds-few-changes-to-new-video-ipod/):

> "PortalPlayer provided the dual-core ARM audio and applications processor (part #PP5021C-TDF)…
> Broadcom supplied the video processor (#BCM2722-MB1KFBG)… Wolfson supplied the audio driver
> (WM8758BG), SST supplied the NOR boot flash part (#39WF800A), Cypress supplied the scroll wheel
> controller (CY8C214), National supplied the switching regulator (L34910B), and NXP supplied **two**
> power management chips (T1211 and PCF50607)"

This settles the NOR letter (`WF`, not `VF`), confirms a Cypress wheel part is genuinely on *this*
board, and puts Linear on the vendor roll-call — but it also says **`PCF50607`**, which is the
opposite of what our driver evidence says. See the gaps.

**There is no separate LCD controller IC.** EDN's *"a separate LCD driver/controller from Toshiba"*
misreads the board: Toshiba-Matsushita made the **panel**, and the BCM2722 is the controller. That
reconciles EDN against iPodLinux's "Unknown LCD Controller" — there was never a third chip.

### What the SoC absorbs

Most things you would expect to be separate chips are **inside the PP5021C**, which is why the board
looks so sparse. From Rockbox's [`pp5020.h`](https://git.rockbox.org/cgit/rockbox.git/tree/firmware/export/pp5020.h)
and the reverse-engineered [PortalPlayer502x register map](https://web.archive.org/web/20250319065733/https://www.rockbox.org/wiki/PortalPlayer502x):
EIDE `0xC3000000`, USB `0xC5000000`, **FireWire `0xC6000000`**, I²C `0x7000C000`, I²S `0x70002800`,
click wheel `0x7000C100`/`0x7000C140`, piezo `0x7000A000`, cache/MMAP `0xF0000000`.

**The FireWire block is on the die.** `DEV_EN` bit `0x800000` and interrupt bit `0x2000000` are
FireWire; the controller sits at `0xC6000000`. This does not contradict §1 — the *link* is SoC
silicon Apple simply stopped wiring to a PHY. Rockbox never touches it, and our access-count table
shows zero traffic there, which is the emulator-relevant fact: **nothing to model**.

Corroborating §1 from an independent angle: Rockbox enables FireWire detection **only** for the 4G,
Color, mini and mini 2G —
[`usb.h`](https://git.rockbox.org/cgit/rockbox.git/tree/firmware/export/usb.h) guards
`USB_FIREWIRE_HANDLING` with `IPOD_COLOR || IPOD_4G || IPOD_MINI || IPOD_MINI2G`. The Video is
absent. And [`power-ipod.c`](https://git.rockbox.org/cgit/rockbox.git/tree/firmware/target/arm/ipod/power-ipod.c)
comments GPIO C2 as *"C2 is firewire power"* for those models while the Video reads its charger
state from GPIO L instead. And Rockbox's
[IpodStatus](https://web.archive.org/web/20150219135252id_/http://www.rockbox.org/wiki/IpodStatus?raw=on)
states it outright: *"All iPods apart from the Nano and 5g can act as external firewire hard
drives."* And Apple's own service documentation is the hardware-level proof: the *iPod 5th
Generation* testing procedure (19 Oct 2005) has **no FireWire Disk Mode Test and no `FIREWIRE`
comms test** — both present in the 4G, photo and mini documents — while retaining a `FWPWR` presence
check and a VCC rise of 3669 → 4562 mV on "Plug In FW Power". The rail is live; the data path is not.

**The retreat was two steps, not one** — from iPodLinux's per-generation `I/O` rows
([Generations](http://web.archive.org/web/20260730084039/http://www.ipodlinux.org/Generations/)):

| Generation | FireWire silicon |
|---|---|
| 1G/2G/3G | TI **TSB43AA82** — link **+** PHY (the "iceLynx-Micro") |
| 4G / photo / color / mini | TI **TSB41AB1** — **PHY only**; the PP502x supplies the link on-die |
| **5G / 5.5G / nano 1G** | **none** |

So the part you might go looking for was already gone a generation before the link was: 4G-era boards
dropped to a bare PHY because the SoC had the link, and the 5G dropped the PHY too. A useful negative
control: TI's TSB41AB1 ships only in gull-wing TQFP/PowerPAD packages, and **no TQFP exists on either
side of either board** — so the unreadable leadless parts in the photos cannot be it.

**Dock-connector numbering — pin the convention in code comments.** Two mutually-mirrored schemes
are in circulation and neither matches the "pins 1–2 power / 3–6 data" framing we started from. In
the dominant convention ([pinoutguide](https://pinoutguide.com/PortableDevices/ipod_pinout.shtml),
[irq5](https://irq5.io/2012/06/25/the-apple-30-pin-dock-connector/)): FireWire **ground** 1–2 and
29–30, FireWire **+12 V** 19–20, TPA± 24/22, TPB± 28/26, USB VBUS/D+/D− 23/27/25. **iPodLinux
numbers pins in reverse** (*n* ↔ 31−*n*), so the same +12 V rail appears there as pins 11/12 — and
its own page is internally inconsistent across revisions, saying "pin 19" in one line while the note
below still says "Pins 11 and 12 connected on motherboard." TPA/TPB assignments also swap between
sources.

### The BCM2722 owns the panel

The LCD is **not** wired to the SoC. Rockbox drives the panel entirely through a window at
`0x30000000` — data `0x30000000`, write-address `…10000`, read-address `…20000`, control `…30000`
— per [`lcd-video.c`](https://git.rockbox.org/cgit/rockbox.git/tree/firmware/target/arm/ipod/video/lcd-video.c).
Consequences for us:

- **No panel model is needed.** Panel timing lives in the BCM's own firmware, invisible to the ARM.
- **The NOR flash is a display dependency.** `bcm_init()` uploads the `vmcs` section — found via a
  `flsh` directory at `ROM_BASE + 0xFFE00` — into BCM SRAM before the first update. No `vmcs`, no
  display; Rockbox sets `flash_vmcs_length = 0` and disables LCD sleep outright.
- Commands are `~x<<16 | x`: `0` update, `1` self-test ("M25 Diagnostics"), `2`/`3` TV PAL/NTSC,
  `5` update-rect, `8` sleep, `14` Macrovision-off.
- Rockbox **never** uses it as a video *decoder* — the iPod port is listed as *"lacking support for
  the video decoder chip"* ([IpodPort](https://web.archive.org/web/20260211134952/https://www.rockbox.org/wiki/IpodPort)).

### 30 GB vs 80 GB — the two real deltas

1. **RAM.** 30 GB = 32 MB, 80 GB = 64 MB. Apple's own model split
   ([board table](https://eoe.works/pages/logic-board-infomation)): MA446/MA444 (30 GB) 32 MB;
   MA450/MA448 (80 GB) 64 MB. Wikipedia agrees: *"the 60 GB and 80 GB fifth generation … have
   64 MB"* ([iPod Classic](https://en.wikipedia.org/wiki/IPod_Classic)). Rockbox detects it at
   runtime in [`crt0-pp.S`](https://git.rockbox.org/cgit/rockbox.git/tree/firmware/target/arm/pp/crt0-pp.S)
   by writing `64` to the last byte of the first bank and `32` to the last byte of the second — on a
   32 MB part both writes alias to the same cell. **This is the same aliasing we already model.**
2. **Sector size — two distinct numbers, don't conflate them.** `ipodvideo.h` sets
   `MAX_PHYS_SECTOR_SIZE 1024` (the 80 GB drive reports 1024-byte *physical* sectors and does not
   handle them in drive firmware, forcing read-modify-write) **and** `MAX_VIRT_SECTOR_SIZE 2048` /
   `DEFAULT_VIRT_SECTOR_SIZE 2048` (the larger sector the device advertises *over USB*). Secondary
   summaries collapse these into "2048-byte sectors". The 1024 is what held up the Rockbox 80 GB
   port. Our ATA model should be explicit about which it presents.

Everything else is common: same board, same SoC, same BCM, same codec, same PMU — and **no
documented LCD difference**. Rockbox builds a single `ipodvideo` target for all variants. The
documented 5.5G-vs-5G deltas are storage-side only.

### Nobody ever enumerated the boot requirements — including Rockbox

The honest answer to *"which chips must respond for firmware to boot"* is that **no published source
knows**, and there is a specific reason. From Rockbox's
[IpodStatus](https://web.archive.org/web/20150219135252id_/http://www.rockbox.org/wiki/IpodStatus?raw=on)
(r71, 24 Oct 2010), "Flash support / Not started":

> "All iPods have a 1MB flash ROM containing the Apple bootloader, the emergency disk mode
> application, and the diagnostics mode application. Replacing this code would allow far faster
> booting time into Rockbox… **However, doing so would require Rockbox to fully initialise the
> hardware on boot (it currently relies on some initialisations performed by Apple's bootloader)**
> and failed flashing attempts would result in a bricked ipod."

**Rockbox has never done cold bring-up on this platform.** It inherits an already-initialised
machine, so its driver set is a *lower* bound on what the hardware needs, not an enumeration of it.
A concrete instance is visible in `lcd-video.c`, where the normal path simply assumes Apple's
bootloader has already powered and initialised the video chip:

```c
if (GPO32_VAL & 0x4000) {
    /* BCM is powered.  Assume it is initialized. */
```

For us this is good news rather than bad: we boot the *real* NOR bootloader, so we inherit the same
initialisation RetailOS expects. It does mean **our access-count table is the best enumeration that
exists** — better than any wiki — and worth treating as a project output.

Two entry points worth having, both recovered from iPodLinux/Rockbox and both directly testable:

- **RetailOS is re-entered** by jumping to `DRAM_START` after checking for the literal string
  `"portalplayer"` at `DRAM_START + 0x20`.
- **Disk mode is entered** by writing `"diskmode\0\0hotstuff\0\0\1"` to IRAM at `0x4001ff00` on
  PP5022-class parts (`0x40017f00` on PP5020) and resetting. Note this is the same IRAM region as
  the `sysinfo_t` handoff block we already model at `0x4001ff18`/`0x4001ff1c`.

A caveat that cuts our way: *"RoLo does not currently restart the coprocessor properly. Separately to
this, !RoLoing the original firmware does not work"* — handing control back to RetailOS from a
running third-party OS fails on real hardware. **The second core is implicated in boot** in a way
nobody has characterised.

### Click wheel — the frame format, since we will need it

Software never sees the Cypress part. It reads `CLICKWHEEL_DATA` at `0x7000C140`, and a frame is
valid when `(status & 0x800000ff) == 0x8000001a`. Bit 31 is set unless Hold is engaged; bit 30 is set
while the wheel is touched; bits 16–22 carry absolute position over **96 clicks per rotation**
([button-clickwheel.c](https://git.rockbox.org/cgit/rockbox.git/tree/firmware/target/arm/ipod/button-clickwheel.c)).
Enable is `DEV_OPTO` (`0x10000`) plus `INIT_BUTTONS`, with init writes to `0x7000C100`/`0x7000C104`.

**Extended and second-sourced 2026-08-14 against Apple's own driver, and modelled.** Rockbox gives
the data register and the streaming frame; RetailOS and the boot ROM give the *transceiver* — a
transmit register at `0x7000C120`, a start bit, a busy bit, a write-1-to-clear receive-ready bit, and
a second frame shape (`0x8000023A`) that is the **reply to a command** rather than an autonomous
report. Both Apple stages ship the same routine byte-for-byte (`0x4000E540` in the ROM,
`0x00283EA0` in RetailOS). The full register table, the interrupt line (IRQ 40, high bank bit 8), and
the measured effect of modelling it are in
[research/10 Addendum 16](10-the-resource-image.md#addendum-16-the-click-wheel-modelled-and-the-only-thing-reading-it-is-apples-bootloader).

**The third command is a write, and it is settled.** `0x8001052A` is opcode `0x052A` — *set
reporting* — with a payload byte at bits 23..16; `0x8000052A` is the same command turning it off.
The hardware sends **no reply**: all five senders in the two Apple stages return or tail-branch
without reading `0x7000C140`, the boot ROM's copy writes TX, spins 10 000 iterations and returns,
and no instruction in either image compares anything against the opcode. So the frame vocabulary is
three shapes and only one of them is a question. See
[research/10 Addendum 21](10-the-resource-image.md#addendum-21-0x8001052a-is-a-write-and-the-answer-is-silence--the-wheel-reaches-the-event-queue).

### The click — `PWM0_CTRL` at `0x7000A000`, and it is a PWM channel, not a piezo — 2026-09-06

**The address in §3's list is right and the name beside it is not.** The reverse-engineered
PortalPlayer502x map calls `0x7000A000` *"piezo"*, which is what the iPod does with it rather than
what it is. Rockbox's
[`pp5020.h`](https://git.rockbox.org/cgit/rockbox.git/tree/firmware/export/pp5020.h) defines it as

```c
#define PWM0_CTRL (*(volatile unsigned long*)(0x7000a000))
```

and the driver that writes it is
[`piezo.c`](https://git.rockbox.org/cgit/rockbox.git/tree/firmware/target/arm/ipod/piezo.c). (Only
those two files were read; no claim is made here about what else in that tree might touch it.) The
distinction earns its place: it is **why there is one register here and not a block of them**, and
it predicts — correctly, see below — that nothing else in the page is ever addressed. §3's line is
left as it stands because it accurately quotes its source; this section is the refinement.

**One register, and two independent firmwares write it the same way.**

```text
+0x00  PWM0_CTRL   bit 31 enable · bits 30..0 the wave · a write of plain 0 stops it
```

Apple's side, out of `OSOS_correct.bin`. `dis --wordref=0x7000a000` finds the address in **four**
literal pools and nowhere else (control: `--wordref=0x7000c000`, the click wheel, finds 16). Two of
the four are the whole driver:

```text
0011c750  ldr  r1, =0x7000a000     ; --- stop ---
0011c754  mov  r0, #0x0
0011c758  str  r0, [r1, #0x0]      ; PWM0_CTRL = 0
0011c75c  ldr  r0, =0x60005000
0011c760  ldr  r1, [r0, #0x8]
0011c764  bic  r1, r1, #0x80000000
0011c768  str  r1, [r0, #0x8]      ; ...and TIMER2_CFG loses its enable

000c7204  ldr  r1, =0x7000a000     ; --- start ---
000c7208  orr  r0, r6, #0x80000000 ; the queued wave...
000c720c  orr  r0, r0, #0x800000   ; ...with bit 23, always
000c7210  str  r0, [r1, #0x0]
```

Rockbox's side is two lines against the same register — `PWM0_CTRL = 0x80000000 | form_and_period`
to start, `PWM0_CTRL = 0` to stop — and the same header puts `TIMER2_CFG` and `TIMER2_VAL` at
`0x60005008`/`0x6000500C`, which is the pair Apple's code reaches for in the same two functions.
Neither source was consulted for code; both were read for register semantics.

**`AsyncPiezo` is a sequencer over a 16-entry ring, not a beeper.** Task 35 in
[research/10](10-the-resource-image.md)'s table — entry `0x00285060`, blocked on semaphore `0x95` —
initialises by disabling `TIMER2_CFG`, enabling interrupt `0x15` through `0x60004024`, clearing bits
3..2 of `0x70000010`, and setting **bit 17 of `0x6000600C`**, the device-enable gate. Its loop pends
on the semaphore, takes a message, and writes two of its fields into parallel rings at `0x10882314`
(the wave) and `0x10882354` (the duration). The step at `0x000C719C` then stops the register, writes
the next wave to it, and programs that entry's duration into `TIMER2_CFG`. So **one tone is two
writes to this register** — a zero and a start — and the *duration* lives in the timer, never here.

The API beneath it is `0x000CD430(wave, duration)`: it builds a message with `op = 0` at `+0x10`,
the wave at `+0x14` and the duration at `+0x18`, takes resource `0x12C`, and sends to queue `0x15`.
Sixteen call sites. `op = 1` is a flush — `head = tail`, playback off.

**The tone vocabulary, read off those call sites.** Durations are microseconds; `TIMER2_CFG` takes
the low 29 bits.

| site | wave | duration | what it sounds like |
|---|---|---|---|
| `0x001B91FC` | `0x55` | `0xBB8` — 3 ms | one short tick, **behind a settings bit** (`[r4+0x3E] & 1`) |
| `0x00091438` | `0xB4` / `0x90` ×4 | 200 ms / 400 ms | an eight-tone alternating melody — `AlarmTask` also calls the API directly |
| `0x001DFF10` | `0x70`, `0`, `0x70` | 80 / 150 / 80 ms | beep–silence–beep |
| `0x0011E4CC` | caller's | `r4 × 1000` | a generic beep of N ms |

Run through Rockbox's own independently-derived frequency relation — `piezo.c` returns `91225/hz` —
those waves are **1073 Hz, 507 Hz, 633 Hz and 815 Hz**. Four constants from Apple's image landing in
the audible beeper band under a formula from a different codebase is a coincidence worth recording;
it is not proof, and no run has yet confirmed a frequency.

There is a **`"Clicker"`** string in this image (five copies), so the settings bit at `0x001B91FC`
having a user-facing switch is consistent — but nothing here has traced the bit to that menu.

**One disagreement, recorded rather than resolved.** Rockbox's `pp5020.h` defines
`DEV_PIEZO 0x00010000` — bit 16 — and gives `DEV_OPTO` the *identical* value. Apple sets **bit 17**
of the same register. One of the two labels is wrong, and **the run shows both bits set**: the
RetailOS descent ends with `DEV_EN 0x408318C0`, which has bit 16 (the wheel's `DEV_OPTO`, and
Rockbox's `DEV_PIEZO` too) *and* bit 17 — so the two are genuinely different gates and Rockbox's
`DEV_PIEZO` is the suspect name. Nothing rests on it here: the model gates nothing on the
device-enable bit, so a wrong guess cannot silently suppress a click.

#### It is modelled, and the fast path was the whole difficulty

`tools/ipod-machine/src/hw/piezo.rs`. The device answers reads with the bytes last written, so it is
byte-for-byte what the plain `mmio-7` backing store did — **attaching it cannot change what the
firmware sees**, which is what makes it a recorder and lets it be on by default.

The one thing that had to be got right is that `0x7000A000` sits *inside* the `mmio-7` region and
the firmware drives it with `str`. Without the page named in `Memory::page_is_plain`, `write32`'s
hoist copies the word straight into the region and returns, and the device is never reached. That
list has now been the bug **seven** times. The failure mode here is the quiet one: a correct
recorder changes nothing observable, so a bypassed one reports zero for ever and reads as an answer.
`load_and_trace.rs::a_word_store_to_pwm0_ctrl_reaches_the_device_and_not_the_region` is the guard,
and it was checked in both directions — with the line removed it fails with `fires: 0`.

#### Measured: Rockbox drives it, and the two instruments agree

Rockbox is the oracle here, and it is a **positive control the earlier zero never had**.

```sh
trace 6000000000 --osos=resources/vendor/rockbox/bin/rb-main.raw --boot-osos --until=170s \
  --flash=resources/roms/retail_5g_MA146_HwVr000B0005_internal_rom_000000-0FFFFF.bin \
  --disk=<writable clone of ipod8g-rockbox.img> --disk-writable \
  --sysinfo --bcm --pmu --clock=5 --storeaddr=0x7000a000 \
  --wheel='@60s:touch,+500ms:rotate=+1 ×8,+3s:release,@80s:touch,+60ms:rotate=+1 ×8,+3s:release,
           @100s:touch,+1s:down=select,+200ms:up=select,+2s:release, …menu@110s, play@120s,
           next@130s, prev@140s'
```

| arm | clicks | word writes | `--storeaddr` stores |
|---|---|---|---|
| stock volume, no `config.cfg` | **0** — no piezo section printed at all | 0 | — |
| `hardware keyclick: on` in `/.rockbox/config.cfg` | **8** | 16 | **16** |

The second arm is *configuration*, not modification: `hardware keyclick` is a stock Rockbox setting
and `/.rockbox/config.cfg` is where Rockbox itself writes it. The binary is untouched, and the file
was written onto a copy-on-write clone. Rockbox's hardware click is gated by
`global_settings.keyclick_hardware` in `apps/misc.c`, so the first arm's zero is a setting being off
and **not** a disagreement about the register.

Two things this settles. **The device's count and `--storeaddr`'s agree exactly** — 16 and 16 — and
`--storeaddr` records at the top of `write32`, ahead of the fast path, so it cannot share the
device's blind spot. And every Rockbox write is `0x8080005B` to start and `0x00000000` to stop, from
`0x0007DEB8` and `0x0007DEFC`, separated by a constant ~20 000 instructions:

```text
0x0007deb8 -> [0x7000a000] = 0x8080005b   @113940218
0x0007defc -> [0x7000a000] = 0x00000000   @113960217
```

**Rockbox sets bit 23 too.** Its `form_and_period` is `0x0080005B`; Apple's constant half is
`0x800000`. Two firmwares that share no authorship both set bit 31 as the enable, both set bit 23,
both stop with a plain zero, and pick low bytes of `0x5B` and `0x55` — 1002 Hz and 1073 Hz under the
`91225/hz` relation. That agreement is the reason this is a model and not a hypothesis.

**When Rockbox clicks**, from the fire log (simulated seconds):

| gesture | clicks |
|---|---|
| 8 detents 500 ms apart (60.0–63.5 s) | **2** — at 62.072 s and 64.072 s |
| 8 detents 60 ms apart (80.0–80.4 s) | **2** — at 80.312 s and 80.551 s |
| SELECT · MENU · PLAY · PREV | **1 each** — 101.026 · 111.036 · 121.025 · 141.078 s |
| NEXT | **0** |
| touch and release alone | **0** |

So in Rockbox: **one click per four detents, at both speeds** — the count follows scroll *events*,
not elapsed time, and there is **no acceleration in the click policy**. Buttons click on the press.
The single NEXT that did not click is unexplained and is left that way; each press navigates, so the
five presses were not made in the same UI context, and `keyclick_click` in `apps/misc.c` is reached
from the action layer rather than from the button driver.

**A methodological trap that cost a run.** `--wheel='…press=select…'` releases the button one
`click_instr` after pressing it — under 4 ms of simulated time at `--clock=5`. Rockbox never saw
four of five such presses. Written as `down=select,+200ms:up=select` the same script clicks on four
of them. **Any button measurement must state the press duration in simulated time.**

#### Measured: RetailOS clicks, and it clicks on the buttons

Same instrument, Apple's own firmware, on a machine that navigates. **`--disk-writable` against a
clone of a mode-444 image is the trap here**: `cp -c` inherits the mode, the open fails with one
`Permission denied` line in the header, and the run goes on to report 12.25 G instructions, a
plausible `usec`, and `54 of 54 steps fired` while stuck in Apple's bootloader with **0 interrupts
taken**. `chmod u+w` on the clone is the whole fix. The healthy run is the one with
`57 frames posted, 0 dropped unread, 57 word reads of DATA, 57 acknowledged`.

```sh
trace 14000000000 --boot-osos --cold-boot --until=2450s \
  --flash=resources/roms/retail_5g_MA146_HwVr000B0005_internal_rom_000000-0FFFFF.bin \
  --disk=<writable clone of ipod8g-retail.PRISTINE.img> --disk-writable \
  --bcm --pmu --nor --clock=5 --storeaddr=0x7000a000 \
  --wheel='@210s:touch,+500ms:rotate=+1 ×8,+3s:release,
           @260s:touch,+60ms:rotate=+1 ×8,+3s:release,
           @300s:touch,+1s:down=select,+200ms:up=select,+2s:release,   … menu@330s, play@360s,
           next@390s, prev@420s,  @1500s: eight more detents,  @1600s: select again'
```

**9 clicks, 27 word writes, 9 stops, 0 reads.** Every start is `0x80800055`, every one from
`0x000C7210` — `AsyncPiezo`'s sequencer step, exactly as the static reading predicted — and
`--storeaddr` independently reports **27** stores against the model's 27.

The 27 are 9 groups of three, and the shape is the sequencer's:

```text
0x0011c758 -> [0x7000a000] = 0x00000000   @1057648543     stop before start
0x000c7210 -> [0x7000a000] = 0x80800055   @1057652230     the tone
0x0011c758 -> [0x7000a000] = 0x00000000   @1057678171     the queue drained
```

A tone runs about **26 000 instructions — 5.2 ms of simulated time**, against the 3 ms that
`0x001B91FC` programs into `TIMER2_CFG`; the remainder is the task's wake-up. **The wave is `0x55`,
which is that call site's constant and no other's.** So the click RetailOS plays is the 3 ms tick
behind the settings bit, and the static identification and the run agree.

**When it clicks.** Nine fires against the script, in simulated seconds:

| fire | at | scripted event | delay |
|---|---|---|---|
| 1 | 214.096 s | the last of eight detents, 214.0 s | +0.10 s |
| 2 | 301.171 s | **`down=select`** 301.0 s | +0.17 s |
| 3 | 324.836 s | *nothing within 5 s* | — |
| 4 | 331.137 s | `down=menu` 331.0 s | +0.14 s |
| 5 | 361.119 s | `down=play` 361.0 s | +0.12 s |
| 6 | 391.124 s | `down=next` 391.0 s | +0.12 s |
| 7 | 421.120 s | `down=prev` 421.0 s | +0.12 s |
| 8 | 1601.119 s | **`down=select`** 1601.0 s | +0.12 s |
| 9 | 1765.890 s | *nothing within 100 s* | — |

**SELECT clicks.** That was asked directly and had been guessed at rather than measured; it is
settled, twice, 1 300 s apart, at +0.17 s and +0.12 s. **MENU, PLAY, NEXT and PREV click too** — one
each, every one inside 0.14 s of the press. Six of the nine fires are a button going down, and the
delay is so nearly constant that the attribution needs no argument.

**Scrolling is the unsettled half, and the number is small.** Eight detents at 210–214 s produced
**one** click; eight more at ~257 s and eight more at 1500–1504 s produced **none**. Two things stop
that from being an answer about a real iPod:

- **The "slow" scroll was not uniformly slow.** The frame log shows detents 0–2 arriving 500 ms
  apart as written and detents 3–8 bunching to 70–130 ms. A halted machine advances `usec` in jumps,
  so several time-anchored steps come due at once and fire together. Any statement about *rate* here
  needs a pacing method that survives an idle machine.
- **Nothing yet confirms the list moved per detent.** The panel redrew 27 times over the run, so the
  descent navigated, but no measurement here ties one detent to one row of movement. Until it does,
  "one click per eight detents" is a statement about our wheel as much as about Apple's click policy.

So: **on buttons, RetailOS's click policy is measured and clean. On scrolling it is not, and the
honest answer is that this run cannot separate the firmware's policy from our wheel's delivery.**
Rockbox, on the same machine and the same day, clicked once per four detents at both speeds and
showed no acceleration — which is a datum about Rockbox and a hypothesis about the hardware, nothing
more.

**Two fires are unattributed to a particular gesture** — 324.836 s and 1765.890 s, both the same
wave from the same site, neither within reach of a scripted one. They are left that way rather than
assigned to the nearest press.

**The control says every one of the nine is input.** Same machine, same drive, same
`--until=2450s`, the wheel device still modelled — only the injected script removed:

| | with the script | control, no input |
|---|---|---|
| piezo report | 9 clicks, 27 word writes | **printed nothing at all** |
| `--storeaddr=0x7000a000` | 27 stores | **no stores section** |
| wheel frames posted / read | 57 / 57 | 3 / 3 — the boot's own queries |
| panel frame updates | 27 | 80 |

Two instruments, both silent, on a machine that booted RetailOS and redrew the panel eighty times.
So **RetailOS never clicks on its own**, the nine fires are all consequences of the wheel, and the
two unattributed ones are late effects of a gesture rather than housekeeping. It also retires the
last worry about the model: a recorder that printed nine clicks and could not print zero would be
telling you nothing, and this is the run where it prints zero.

#### Scrolling settled: there is no acceleration, and the fold is ours — 2026-09-07

The section above left one thing open — *"on scrolling it is not [settled], and the honest answer is
that this run cannot separate the firmware's policy from our wheel's delivery"* — and the operator
asked the same question from the other end: a real iPod clicks constantly as a list moves, ours
clicks about five times.

**It separates.** Every arm below is `ipod-boot retail` on
`roms/retail_5g_MA146_HwVr000B0005_internal_rom_000000-0FFFFF.bin` and `drives/ipod8g-retail.img`,
at the part's own clock, with the PMU clock pinned so the arms are comparable to the instruction:

```sh
BUDGET=20000000000 ipod-boot retail --clock=75 --until=32s --rtc=2026-09-07T12:00:00 \
  --wheel='@20s:touch,+1s:rotate=+60,+3s:release' --wheel-click-instr=1950000 \
  --enterlog=0x000dd018,0x000cd6a0,0x001181f8,0x000cd430
```

**`--wheel-click-instr` is the knob, and the obvious one is not.** `+26ms:rotate=+60` sets when the
rotate *starts*; the spacing between its sixty steps is `click_instr` and nothing else. Three arms
written that way came back byte-identical — 6 events, 2 clicks, three times — which reads as *the
rate does not matter* and is instead an instrument that was never varied. In a time-anchored script
the flag is instructions and `parse_wheel_script` divides by the clock, so 26 ms at clock 75 is
`--wheel-click-instr=1950000`.

##### RetailOS's wheel path has no acceleration in it. It has a threshold, and a queue of one

Read out of the image, and then watched running.

`0x000dd018`, the scroll accumulator, works on `r5 = 0x1081d998`:

```text
000dd028  tst r4, #0x40000000     ; touched now?
000dd02c  sub r0, r4, r1          ; delta against [+0x1c], the previous frame
000dd03c  cmp r0, #0x48           ; ...wrapped into +-72 by +-0x60
000dd050  ldr r1, [r5, #0x4]      ; T, the threshold
000dd058  cmp r0, r1              ;   |delta| >= T -> emit; else accumulate into [+0xc]
000dd084  ldr r1, [r5, #0x10]     ; emit: the running TOTAL...
000dd088  add r0, r1, r0          ;   ...gains the delta, every time, 1:1
000dd098  ldrb r0, [r5, #0x0]     ; and the event is posted ONLY if none is pending
000dd0a0  bleq 0x000cd6a0
000dd0a4  ldr r0, [r5, #0xc]      ; once the accumulator passes +-3...
000dd0b0  strcs r6, [r5, #0x4]    ;   ...T := 0, and every later detent takes the emit path
```

So **T is a dead zone that opens after four detents and never comes back while the finger is down**,
and past it the firmware is 1:1. `--storeaddr=0x1081d99c` watches T do exactly that: written **3** by
`0x00084394` during the boot, dropped to **0** by `0x000dd0b0` four detents into the gesture. Nothing
in this file multiplies, divides or squares a delta. There is no acceleration to find.

What folds is the **post**, not the motion. `0x000cd6a0` is four instructions — `strb #1,
[0x1081d998]` then `b 0x000adb54`, which posts a `'Weel'` (`0x5765656c`) event — and `0x001181f8` is
the consumer: it returns `[+0x10] - [+0x14]`, the whole accumulated delta, and clears that byte. One
event may be outstanding at a time. Every arm below has `0x000cd6a0` and `0x001181f8` at **exactly
equal counts**, which is that queue of one, seen from both ends.

**The click is requested once per view transition.** `--enterlog=0x000cd430` — RetailOS's piezo API —
puts **every** fire of every scroll arm at `lr = 0x001b9200` — the call at `0x001b91fc`, with
`r0 = 0x55` and `r1 = 0xbb8` (3 ms), inside the screen-transition function that begins at
`0x001b9168` and clicks when `[r4+0x3e] & 1`. **The scroll path itself never clicks**: `0x000adb54`,
where `0x000cd6a0` tail-branches to post the event, reaches nothing that calls `0x000cd430`. The
redraw the event causes is what clicks.

*(The button path reaches the same API by a different road — `0x000ada4c` tails into `0x000a69dc`,
which reads the Clicker setting at `0x1081d9c4` and then either clicks directly or delegates to
`0x001b9168`. No arm here pressed a button, so nothing above is evidence about buttons; the button
measurement is the 2026-09-06 section and stands unchanged.)*

##### Measured: the same gesture, three speeds

62 steps every time — one touch, sixty detents, one release. Only the spacing changes.

| detent spacing | detents/s | `'Weel'` events | clicks | panel frames | rows the list moved |
|---|---|---|---|---|---|
| **4 ms** — what `ipod-gui` sends | 250 | 6 | **2** | 8 | 2 *(inferred)* |
| 26 ms — a thumb | 38 | 8 | **4** | 10 | 4 — English → Deutsch |
| 100 ms | 10 | 14 | **10** | 16 | 10 — English → Nederlands |

The two named rows are read off `--bcm-dump=e0000:140:f0:` at the end of those runs: the machine is
on the first-run **Language** picker, which is long, so nothing here is a list running out. **Clicks
equal panel frames minus the boot's own six in all three arms**, and equal the rows actually
photographed in the two that were photographed — so the 4 ms row count is that identity carried one
step, not a picture anybody looked at.

**The ceiling is not the wheel.** The fires are **555 ms apart in every arm** — 548, 554, 558 ms at
26 ms spacing; 531 … 616 ms at 100 ms; 555 ms at 4 ms — and in *instructions* they are 41.6 M apart
in all three. That is a fixed amount of work per list move, not a timer and not a rate we set. The
count of clicks is simply how many 555 ms redraws fit inside the gesture, which is why spreading the
same sixty detents over six seconds instead of a quarter of one buys five times the clicks **and five
times the scrolling**.

`--profile --profile-window=` over one such interval puts **58.6%** of it in the four-instruction
zero-fill loop at `0x0007cce8`, entered at `0x0007ccd0`; `--enterlog=0x0007ccd0` counts **387 076**
calls across the run, with small lengths (5, 0x30, 0x200, 0x1e …). So it is very many small fills
rather than one large one, with `ImagePresentationEngine` next at 6.3%. **Read the profiler's split
between `0x0007cce0` and `0x0007ccf0` — 56.7% against 1.9% — as one number and not two**: the sampler
fires every 64th instruction and the loop is four long, so it lands on the same phase of the loop
every time and the bucket boundary falls inside it. Their sum is the measurement; their ratio is an
artefact of a systematic sampler aliasing against a short loop.

##### The oracle: Rockbox does not fold, and it is the control for the machine

`hardware keyclick` is off in stock Rockbox, so the first arm is a **zero that had to be made
non-zero before it meant anything** — `/.rockbox/config.cfg` with `hardware keyclick: on` written
onto a writable clone of `drives/ipod8g-rockbox.img`. That is configuration, not modification: the
name is `settings_list.c:2275`'s own, the binary is untouched.

| arm, same 62 detents | clicks | panel frames |
|---|---|---|
| stock, no `config.cfg` | **0** — no piezo section printed at all | — |
| keyclick on, 4 ms spacing | **8** | 65 |
| keyclick on, 26 ms spacing | **8** | 69 |

Two things fall out. **Rockbox is rate-independent where RetailOS is not** — 8 either way — which is
`button-clickwheel.c` posting one event per frame with a non-zero delta, gated by
`button_queue_empty()`: the same queue-of-one shape as Apple's, against a consumer that keeps up.

And the like-for-like number, which is the interval and not the total: **Rockbox's eight fires are
104 ms apart** (21.083 · 21.187 · 21.291 · 21.395 · 21.499 · 21.603 · 21.707 · 21.811 s — a
metronome) against RetailOS's 555 ms, on the same emulated machine, in the same session, through the
same `--wheel` script. So the 555 ms is RetailOS's own cost and not our model refusing to go faster.
*(The panel-frame totals in that table are for the whole 32 s run and are not comparable across the
two firmwares — Rockbox boots in a fraction of the time and sits at its menu redrawing a clock. The
interval is the comparison; the totals are context.)* Whether a real 5G pays 41.6 M instructions for
a list row is a different question and is **not** answered here.

##### `Pad::on_ring`'s missing hysteresis is implicated, and it is the sharpest of the three

The operator's words were *"i only get haptics when i go over the edge on the trackpad"*, and this is
why. The decoder at `0x00281350` does this on **every frame with the touch bit clear**:

```text
00281390  moveq r0, #0x3
00281394  streq r0, [r3, #0x4]    ; T := 3      -- the dead zone is re-armed
002813a8  streq r2, [r3, #0xc]    ; accumulator := 0
```

and `0x000dd018`'s release arm posts a `'Weel'` event of its own. So a lift is not neutral: it costs
the four detents needed to reopen the gate, and it rings the bell on the way out.

`ipod-gui`'s `Pad::on_ring` had no hysteresis — a contact was on the wheel iff `WheelRing::hit` said
so, per frame, on a pad that samples at ~124 Hz. A finger circling near `CENTRE` therefore lifted and
landed repeatedly. Scripted, with a release-and-touch pair inserted into the same 26 ms gesture:

| chatter | `'Weel'` events | clicks | panel frames |
|---|---|---|---|
| none — one unbroken contact | 8 | **4** | 10 |
| a pair every 5 detents | 13 | **3** | 9 |
| a pair after **every** detent | 60 | **0** | **6** — the boot's own |

The last row is the whole finding: sixty detents, every one delivered and decoded, and the list
**does not move at all** and RetailOS **never clicks**. The gate never opens, so no detent ever
reaches the total. More events, less motion.

Fixed in `tools/ipod-gui/src/trackpad.rs`: the contact is now held across `ARM_MM` (1.5 mm — the band
already measured for the edge marks) before it leaves the wheel. The *mark* still fires on the true
line, because a boundary announced late is reported in the wrong place; only the contact is
hysteretic. **This overturns a decision that was recorded rather than assumed** — `ARM_MM`'s note
used to end *"a Schmitt trigger would move the boundary, and the boundary is not this feature's to
move … and the wheel behaves exactly as it did before"* — and it is overturned by a fact its author
did not have, which is that an untouched frame reaches into Apple's firmware and re-arms a dead zone.
One geometric guarantee is genuinely lost with it: a centre mark and a detent could not previously
occur in the same frame, and now can. What that was protecting is untouched, because it never rested
on the geometry — `Ticks::due` reads `self.click` and no field a mark can write, so marks still
cannot starve a click.

**The chatter is the trackpad's and not the part's.** A 5G has a moulded bezel between the ring and
the centre button; a rectangle of glass has nothing there to feel. Holding the contact is the adapter
compensating for a physical edge the input device lacks, and it stops at the adapter — nothing in
`ipod-machine` moved.

##### What is left, stated as ours

- **The 4 ms delivery floor is a real hazard that is not currently firing, and the difference is the
  clock.** `emu::drain` spaces appended steps by `click_gap` — 4 ms of the iPod's own time, 250
  detents/s — and that figure is `--wheel-click-instr`'s default, whose own comment calls it *"a
  brisk but human scroll"*. It was chosen for **scripts**, where it is the spacing you asked for, and
  nothing ever checked it against hardware. Used as the delivery rate for **live** input it would be
  a category error, because live input already has a rate: the hand's — and `Inbox` is a
  `VecDeque<WheelEvent>` with no arrival time on it, so `drain` has nothing to pace against and falls
  back to the floor **whenever a backlog exists**.

  A backlog exists whenever the machine is slower than life, which is what `drain`'s own note
  records from clock 75: *"1.2 s of wall time and 120 ms of the iPod's own"*, a tenfold compression.
  The table above prices that at half the clicks and half the scrolling.

  **But at the clock the window actually ships, there is no backlog.** Measured this session, same
  host, `a_scroll_at_a_human_rate_is_timed_end_to_end_and_this_needs_resources` at clock 16 — sixty
  detents pushed one every 16 ms of wall time:

  ```text
  60 detents, one every 16 ms — the finger was on the wheel for 1.30 s
  reached the wheel after       1.31 s
  frames posted 62 (0 dropped unread, 0 suppressed)
  ```

  Ten milliseconds of lag across a 1.3 s drag: the queue never fills, so the floor never becomes the
  rate. **The clock calibration of 2026-09-07 already closed this**, and a change to `drain` today
  would buy nothing on the machine it ships on. It is recorded here because the hazard is still in
  the code and returns the moment the machine falls behind — `--clock=75` is exactly that machine,
  and the compression is fully present there. The fix, if it is ever wanted, is an arrival `Instant`
  on the queued event, not a different constant.
- **The 555 ms redraw is the dominant term and is not this section's.** It is Addenda 20–25's
  output-stage wall. Nothing about the wheel can raise a click rate that RetailOS caps at 1.8 Hz.

##### The operator's six, arithmetic rather than impression

His log was `released — 4736 frames, 6 detents felt, 61 edges felt (centre)`, on a machine the clock
calibration had settled at **16**. 41.6 M instructions is 555 ms of the iPod's time at clock 75 and
**2.6 s at clock 16** — and clock 16 is chosen precisely so that a simulated second costs a wall
second, so it is 2.6 s of *his* time per list row. 4736 frames at the pad's ~124 Hz is about 38 s of
contact, which has room for roughly **fifteen** rows. He got six, and the 61 edge crossings are where
the other nine went: each one re-armed the four-detent gate.

So none of the three terms is a mystery and only one of them is large. **Six was the machine
answering correctly**, and the pulse he is missing is a pulse RetailOS did not ask for — which is why
the window must not manufacture it, and why the way to the sound he remembers runs through the
redraw.

**So `Piezo::fires` is faithful and the window must not invent a pulse.** Five or six clicks across a
long drag is the guest asking five or six times, because the guest moved the list five or six rows.
The way to more clicks is more rows — deliver the gesture at the rate it was made, keep the contact
whole, and make the redraw cheaper — and every one of those is a change to the emulator rather than
to the actuator.

### TV-out is behind the BCM too

Unlike the Photo/Color, which used a separate Analog Devices ADV7179 encoder, the 5G's TV-out hangs
off the Broadcom chip — *"this is likely to be different to the Photo/Color and connected directly to
the Broadcom chip"*. Framebuffer at `BCMA_TV_FB` (`0xC0000000`), and note `BCMCMD_TV_MVOFF`:
*"Macrovision analog copy prevention is on by default on TV output."* No encoder chip to model.

### Honest gaps

- **The SDRAM is `K4M`, not `K4S`.** The widely-copied `K4S56163PF` has the wrong family letter and
  appears to be one wiki error that propagated. `K4M` is Samsung *mobile* SDRAM, consistent across
  both capacities and with what iPodLinux lists for the nano 1G. **Second-sourced:** one 820-1975-A
  board photographs with a non-Samsung part whose marking begins `HY…18L` (lot `WVV46056`) — vendor
  attribution muddled in our sources (an `HY` prefix reads as Hynix, though the 2006 teardown
  reported Qimonda multi-sourcing), exact part unverified. **Assume multi-sourcing; never key
  behaviour off a specific SDRAM part.**
- **NOR: `WF` wins.** `SST39WF800A` (1.65–1.95 V) is given by iPodLinux *and* the EE Times 5.5G BOM.
  The Rockbox wiki's `SST39VF800A` (3.0–3.6 V) cites iPodLinux as its source, so it is a downstream
  typo. 1 MB either way — immaterial to us, material if sourcing a replacement.
- **The Cypress part is half-resolved.** The *family* is confirmed on this board — EE Times names
  `CY8C214` — but the truncated digits leave the exact part open, and `CY8C21434` still rests on one
  iPodLinux mention with no photo. Rockbox has **no Cypress reference for any PortalPlayer iPod**,
  which is expected: the wheel is read through the **SoC's** `opto` block, so we emulate
  `0x7000C140` and never the PSoC.
- **The nano-2G contamination warning was half wrong — I over-corrected.** LTC4066 and LM34910B *are*
  documented for the nano 2G ([InformationWeek](https://www.informationweek.com/it-leadership/report-mystery-chips-in-ipod-nano)),
  but the EE Times 5.5G BOM independently names `L34910B` and puts **Linear** on the vendor
  roll-call, so both plausibly sit on this board too. The reused parts-listing photo is still weak
  evidence; the BOM is not. Shared parts across contemporaneous Apple designs is the ordinary case.
- **`WM87588G` is bogus** — a misread of `WM8758BG`, traceable to one component list that also
  misspells "Wolfsom". Discard it wherever it appears. The `BG` suffix is itself text-only, never
  photographed.
- **IRAM size is inferred, not stated.** 128 KB on PP5022-class parts vs 96 KB on PP5020, derived
  from the disk-mode magic-address delta (`0x4001ff00` vs `0x40017f00`). No source states it
  outright. We already run 128 KB and the `sysinfo` pointers corroborate it, but it is a derivation.
- **PMU: `PCF50605` vs `PCF50607` is a genuine standoff, and I called it too early.** I first wrote
  this off as a search artifact. It is not. **Two** independent sources say `PCF50607` — iPodLinux's
  *Generations* page and the EE Times 5.5G BOM — against Rockbox's
  [`pcf50605.c`](https://git.rockbox.org/cgit/rockbox.git/tree/firmware/drivers/pcf50605.c) at I²C
  `0x08` with `CONFIG_RTC RTC_PCF50605` in `ipodvideo.h`. The likeliest reconciliation is that these
  are register-compatible NXP siblings and Rockbox named its driver after the earlier part it was
  first written for — but **nobody has photographed the marking**, so this is unresolved.
  **Immaterial to emulation**: what we model is the register interface at I²C `0x08`, which Rockbox
  documents against real 5G behaviour regardless of the number silkscreened on the package.
- **A second NXP PMIC (`T1211`) is unaccounted for.** The EE Times BOM says NXP supplied *two* power
  chips. Nothing in Rockbox, iPodLinux or any wiki mentions a second one. Function unknown; it never
  appears on I²C `0x08`. Worth watching for if unexplained I²C traffic ever shows up in a boot trace.
- **Whether the FireWire DATA pins are true no-connects — UNVERIFIED.** No continuity test,
  boardview or schematic has ever been published for 820-1763-A or 820-1975-A. "No PHY, no data
  test, no driver" settles the *functional* question, but it does not prove the pins are NC rather
  than unpopulated footprints or stub traces. Settling it needs a multimeter on a donor board.
  Immaterial to emulation; relevant only if we ever build dock hardware.
- **Provenance of the [P] tier.** Those are silkscreen reads off archived teardown photos, made by a
  verification pass rather than re-examined by hand. Stronger than the text sources, weaker than
  reading our own board under a loupe — which is the cheapest way to close every remaining [U].
- **Re-fetching the photos.** rockbox.org, theapplewiki and EDN/EE Times all block automated access,
  and Ars Technica's teardown images now 410. The Wayback raw modifier `id_` defeats that:
  `https://web.archive.org/web/2013id_/http://origin.arstechnica.com/reviews/hardware/video-ipod.media/ipodvideo-mainchips.jpg`
- **PP5021C vs PP5022.** The package is marked **PP5021C-TDF**
  ([Rockbox PortalPlayer](https://web.archive.org/web/20251113153059/https://www.rockbox.org/wiki/PortalPlayer),
  [IpodHardwareInfo](https://web.archive.org/web/20250905133836/https://www.rockbox.org/wiki/IpodHardwareInfo)).
  Rockbox's `CONFIG_CPU PP5022` is a **software family grouping** — it has no `PP5021` constant —
  not a contradiction.
