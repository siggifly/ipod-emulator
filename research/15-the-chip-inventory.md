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
the USB serial number — see research/07*(moved to the `ipod-usb` repository)* § "The FireWire GUID *is* the USB
serial number." None of that implies a FireWire port. Apple kept the GUID as the device's stable
64-bit identity after the bus it was named for was gone.

Nor is iTunes' `sbp2` string evidence: it lives in iTunes' own Windows binary
(`DeviceManagement\PnpDiskUtil.cpp`), where `sbp2` is how Windows names FireWire storage in a device
instance ID. That is iTunes being able to talk to *older* FireWire iPods, not a statement about this
one. It is quoted in research/07*(moved to the `ipod-usb` repository)* and was briefly misread here as flash
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
  decoded from Rockbox — see [research/11](11-rtxc-and-the-video-coprocessor.md) §33).
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
[research/20 Addendum 16](20-the-resource-image.md#addendum-16-the-click-wheel-modelled-and-the-only-thing-reading-it-is-apples-bootloader).

**The third command is a write, and it is settled.** `0x8001052A` is opcode `0x052A` — *set
reporting* — with a payload byte at bits 23..16; `0x8000052A` is the same command turning it off.
The hardware sends **no reply**: all five senders in the two Apple stages return or tail-branch
without reading `0x7000C140`, the boot ROM's copy writes TX, spins 10 000 iterations and returns,
and no instruction in either image compares anything against the opcode. So the frame vocabulary is
three shapes and only one of them is a question. See
[research/20 Addendum 21](20-the-resource-image.md#addendum-21-0x8001052a-is-a-write-and-the-answer-is-silence--the-wheel-reaches-the-event-queue).

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
