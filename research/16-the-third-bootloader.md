# ipodloader2 — the third bootloader, and the fourth model shaped around one driver

**Built and cold-booted for the first time on 2026-08-18.** Apple's own bootloader finds it in the
firmware partition and enters it, exactly as it enters RetailOS and the Rockbox bootloader. What
happened next is the point of having a third stack.

## Building it

No ARM cross-compiler was installed and nothing recorded that this was the blocker.
`arm-none-eabi-gcc` is a bottled Homebrew formula and `ipodloader2`'s Makefile already defaults
`CROSS ?= arm-none-eabi-`, so a plain `make` builds it: 16 translation units, one `.s`, one `.cc`,
its own linker script, and `loader.bin` at **57 676 bytes**.

`install-os` takes a raw image, but its `.ipod`-header detection is a heuristic — *"bytes 4..8 are
all alphanumeric"* — and ARM code can satisfy that by accident, which would send a raw binary down
the checksum path and fail. So the loader is wrapped properly first: big-endian sum of the body
seeded with `5` (the Video's `modelnum`), then `ipvd`. `install-os` then verifies it as it would
Apple's own.

## It runs, and immediately addresses a 1G iPod

```
Running 'osos' 0 from 0x10735A00        <- Apple's bootloader entered our loader
unmapped: 91 499 510 reads, 99 755 writes across 14 pages
  0xcf00101c..0xcf001113   91 417 368 reads   first pc 0x4000aadc
        pc 0x40002228  x91 417 360
```

**`0xcf00xxxx` is the PP5002 register base** — the 1G/2G/3G iPods. This is a 5.5G, a PP5022, whose
registers are at `0x6000xxxx`/`0x7000xxxx`. The loader relocated itself to IRAM correctly (every PC
above is `0x4000xxxx`, which is where `startup.s` says it copies itself) and then spent 91 million
reads talking to hardware that has not existed since 2003.

### The discriminator is one line

`ipodhw.c:27`:

```c
static int ipod_is_pp5022(void) {
  return (inl(0x70000000) << 8) >> 24 == '2';
}
```

It reads `PP_VER1` and asks whether **bits 23:16 are ASCII `'2'`**. That single bit of information
chooses which of two IRAM addresses holds the `sysinfo_t` pointer Apple's bootloader left behind —
`0x4001ff1c` for PP5022, `0x40017f18` for PP5002. Answer it wrong and the loader dereferences the
wrong pointer, fails the `IsyS` magic check, leaves `ipod.hw_rev` at **0**, computes
`hw_ver = 0 >> 16 = 0`, and selects the oldest hardware it knows.

### ~~Confirmed by forcing it, one variable~~ — RETRACTED 2026-08-18, it was not one variable

`--rdval=0x70000000=0x32323035` — a value whose bits 23:16 are `0x32`, used as this project's
documented bisect instrument and **not** as a model:

| | unmapped reads |
|---|---|
| as-is | **91 499 510** across 14 pages |
| chip id forced | **8** across 1 page |

The `0xcf00xxxx` traffic disappears completely.

> **"The detection was the whole of it" is wrong, and the control was confounded.** Forcing
> `0x70000000` does not touch only `ipodloader2`: **Apple's bootloader reads that register 23 times
> a boot**, as the table at the top of this file says in the very next section. With the value
> forced, Apple's bootloader itself hangs — in IRAM at `0x400038cc`, before `ipodloader2` runs at
> all:
>
> ```
> 400038c0  ldr r0, [r4, #0x28]     ; r4 = 0x70000000
> 400038c4  bic r0, r0, #0x800
> 400038c8  str r0, [r4, #0x28]
> 400038cc  ldr r0, [r4, #0x28]
> 400038d0  tst r0, #0x80
> 400038d4  beq 400038cc            ; wait for bit 7, forever
> ```
>
> The bytes in memory there do not match `loader.bin` at the same offset — checked, and they differ
> completely — which is what proves whose code it is. **So the unmapped reads went to 8 because the
> machine stopped getting that far, not because the model was fixed.** A number improving is not the
> same as the thing improving, and this is R5: a control only proves what it exercises, and this one
> exercised two drivers while claiming to isolate one.
>
> What survives: `ipod_is_pp5022()` really does test byte 16 for `'2'`, and our answer really does
> fail it. What does not survive is any claim about what happens *after* that test, because no run
> has yet got there with Apple's bootloader still working.
>
> **The fix is therefore narrower and harder than "set the byte":** the value has to satisfy Apple's
> bootloader *and* `ipodloader2` at once, which means the **real** `PP_VER1`, sourced. An invented
> word that pleases one and hangs the other is not progress, it is a different bug.

## Why no earlier stack found this

The same shape as the three Rockbox found, and [ROADMAP](../ROADMAP.md) §"What this project is"
predicts it: **a model shaped around the drivers that have run against it.** Apple's bootloader
reads `0x70000000` 23 times in a cold boot and RetailOS once, and neither of them cares what byte
16 says — they already know what chip they are. `ipodloader2` is the first code here that has to
*ask*, and it is the first code that could be wrong about the answer.

### The right value, derived from a second implementation

**Rockbox does decode it**, in `firmware/target/arm/pp/debug-pp.c:224`, and that is the source the
earlier draft of this file said we did not have:

```c
char pp_version[] = { (PP_VER2 >> 24) & 0xff, (PP_VER2 >> 16) & 0xff,
                      (PP_VER2 >>  8) & 0xff, (PP_VER2)       & 0xff,
                      (PP_VER1 >> 24) & 0xff, (PP_VER1 >> 16) & 0xff,
                      (PP_VER1 >>  8) & 0xff, (PP_VER1)       & 0xff, '\0' };
```

So the two registers are an **eight-character ASCII string**, most-significant byte first, `PP_VER2`
then `PP_VER1`. Line that up against `ipodloader2`'s test — `(inl(0x70000000) << 8) >> 24`, which is
bits 23:16 of `PP_VER1`:

```
 "P  P  5  0  2  2  C  -"
  0  1  2  3  4  5  6  7
              ^ index 5 = (PP_VER1 >> 16) & 0xff
```

**The test is not arbitrary: index 5 is the digit that separates PP502*2* from PP502*0*.** That
makes the loader's one-line check a sensible thing to write, and it makes the value derivable rather
than invented:

| | |
|---|---|
| `PP_VER2` (`0x70000004`) | `0x50503530` — `'P','P','5','0'` |
| `PP_VER1` (`0x70000000`) | `0x3232432D` — `'2','2','C','-'` |

### The wrinkle, and why this is still not a model

**Our reference hardware may not be a PP5022 at all.** The real drive's own
`iPod_Control/Device/SysInfo` says:

```
BoardHwName: PP5021C-2
boardHwRev:  0x00050000
ModelNumStr: xMA146
```

`PP5021C`, and a board revision of 5 — which is the **5G**, not the 5.5G, and `MA146` is the 30 GB
5G. If that is what this machine is, then character 5 is `'1'`, `ipod_is_pp5022()` correctly returns
false, and **`ipodloader2` taking the PP5002 path is the loader's own bug on a 5G rather than our
model's**. Choosing `'2'` to make the loader happy would then be inventing a different iPod.

**So the open question is not "what value" but "which chip are we".** It is answerable — the NOR's
`SCfg`, the `sysinfo_t` Apple's bootloader leaves at `0x4001ff1c`, and this `SysInfo` file are three
independent statements of identity, and they should agree. Until they are compared, nothing goes in
the model.

## Where it stops now

**Nowhere useful, and the honest statement is that we do not know.** With the chip id forced,
`ata commands: 0` and the panel is blank — but that is Apple's bootloader hanging, per the
retraction above, so it says nothing about `ipodloader2`. Without the chip id forced, the loader
runs and addresses a 1G iPod. **There is no run yet in which Apple's bootloader completes AND the
loader detects the right chip**, so the loader's own behaviour past detection has never been
observed.

Everything needed to observe it is now in place except one number:

| | |
|---|---|
| the loader | builds, wraps, installs, and is entered by Apple's bootloader |
| the kernel | **found and sourced** — ZeroSlackr `boot/vmlinux`, 1 531 200 bytes, sha256 `9c7b66e2…` |
| the drive | built: loader in the firmware partition, `/boot/vmlinux` across 374 clusters, `loader.cfg` at the root |
| the blocker | the **real** `PP_VER1`, which must satisfy Apple's bootloader and `ipodloader2` at once |

**Settled when** the loader draws its own menu. That is [ROADMAP](../ROADMAP.md) M4's first
checkpoint, and it proves the whole chain except the kernel.

## The handoff block, captured — the specification M5 has to meet

Taken at `--stop-at=0x10000000:1`, the instant Apple's bootloader jumps to the OS. **This is the
thing a synthesised ROM has to reproduce**, and holding a real ROM is what makes that checkable
rather than hopeful.

Two levels. At the top of IRAM, a tag and a pointer:

```
0x4001ff18   "IsyS"  <ptr>        <- 128 KB machine; a 96 KB one keeps this at 0x40017f18
```

And the block it points at, whose layout `ipodloader2`'s `struct sysinfo_t` documents and which the
capture confirms field by field:

| offset | field | observed |
|---|---|---|
| `+0x00` | `IsyS` magic | present |
| `+0x04` | `len` | **`0xf8`** — and this is load-bearing: `ipodhw.c` reads `hw_rev` from `sdram_zero2` when `len == 0xf8` and from `boardHwSwInterfaceRev` otherwise |
| `+0x08` | `BoardHwName[16]` | a model string, NUL-padded |
| `+0x18` | `pszSerialNumber[32]` | **the unit's real serial** |
| `+0x38` | `pu8FirewireGuid[16]` | **the unit's real FireWire GUID** |
| `+0x48` | `boardHwRev` | … |

**The last two are why M5's identity tiers exist, and why the values are not written down here.**
They belong to whoever owned this iPod. `research/07` already carries a flagged privacy issue about
exactly this class of data, and a synthesised ROM must take them from the user — generated,
provided, or read out of their own `iPod_Control/Device/SysInfo` — rather than inheriting a
stranger's from a dump that circulated.

**What this gives M5 immediately:** the block is small, its layout is known, `len == 0xf8` selects
which field the OS believes, and the whole thing is reproducible by construction. The remaining
work is not this block but everything around it — SDRAM up, PMU talked to, `vmcs` uploaded, the
drive spun and its partition table read — each of which is a diff against a real boot rather than a
guess.

## Identity formats, for M5's generator

Both fields decompose from data already held, and neither needs a datasheet.

### FireWire GUID — fully specified

The observed GUID begins `0x000A27`, which is **Apple's registered OUI**. The remaining 40 bits are
the device's own. So generation is exactly: Apple's OUI in the high 24 bits, 40 bits of uniqueness
below, and nothing else is required for software to accept it.

**This is the field with teeth.** Apple's DRM binds a purchased title to it, so a generated GUID
means those titles can never authorise — which is a fact for the UI, not a footnote. It is also the
reason the "read it from your own iPod" tier exists.

### Serial — Apple's pre-2010 eleven-character format

Two real examples, from two different sources, agree on the shape:

```
4J 6 08 2Y7 TXK        from the NOR's handoff block
JQ 5 51 Y5H TXM        from the drive's iPod_Control/Device/SysInfo
│  │ │  │   └── model code (3)
│  │ │  └────── unique (3)
│  │ └───────── week of year (2)
│  └─────────── year, last digit
└────────────── manufacturing location (2)
```

**The last three characters are not the model, and chasing them was the wrong thread.** Apple's
published 5G endings are `V9K V9P V9M V9R V9L V9N V9Q V9S WU9 WUA WUB WUC X3N`, plus `W9G` for the U2
edition — and **neither `TXK` nor `TXM` is on that list**, while both serials' date fields sit
squarely in the 5G period (week 51 of 2005, week 08 of 2006). So the published tables are incomplete,
and no mapping from these codes to a capacity is available or inferable.

It does not matter, because **the model is a different field entirely** and we have it — see
§"What the NOR says it is" below. Nothing validates the serial; RetailOS displays it. So a generated
serial needs the right *shape*, and this project's generator deliberately ends its serials `ZZ?`,
which is on no published table: a generated identity should be recognisable as generated, and can
then never collide with a real device's code by accident.

### And a thing the two examples reveal

**Those are two different serials, so the NOR and the drive came from different iPods.** The README
warns that the two files "must be for the same iPod", and the check that enforces it compares
*family*, not serial — so this pair passes, correctly, because family is what actually has to match.
Worth knowing before anyone reads a serial out of one and expects it to appear in the other.

## What the NOR says it is — model, colour, and the generation, all sourced

`ipod-boot syscfg` read two of `SysCfg`'s seven records. The other five were sitting there the whole
time, and one of them answers a question that had otherwise been headed for somebody's memory:

```
records SrNm, FwId, HwId, HwVr, Regn, Mod#, DrmV
```

**`Mod#` is the model number**, and on this dump it is `MA146` — written bare, with no `x` prefix.
(The drive's `SysInfo` writes the same value as `xMA146`; the NOR does not. A lookup has to take
both.) **`HwVr` is `0x000B0005`** — the same Gestalt ID `research/02` found RetailOS switching on at
`sysinfo + 0x84`.

Each tag carries its payload differently, observed on the one real dump held here: `SrNm` and `Mod#`
are NUL-terminated text from byte 0, while `FwId`, `HwVr` and `DrmV` leave the first word zero and
put their value at byte 4. There is no general rule to apply to an unfamiliar tag, so unfamiliar tags
are listed and not decoded.

### The table, from libgpod

`libgpod/src/itdb_device.c` carries `ipod_model_table` — the table iTunes-compatible software has
used for twenty years. Its 5G/5.5G rows:

| `Mod#` | capacity | colour | generation |
|---|---|---|---|
| `A002` / **`A146`** | 30 GB | white / **black** | **`VIDEO_1` — 5G** |
| `A003` / `A147` | 60 GB | white / black | `VIDEO_1` — 5G |
| `A452` | 30 GB | U2 | `VIDEO_1` — 5G |
| `A444` / `A446` | 30 GB | white / black | **`VIDEO_2` — 5.5G** |
| `A448` / `A450` | 80 GB | white / black | `VIDEO_2` — 5.5G |
| `A664` | 30 GB | U2 | `VIDEO_2` — 5.5G |

Lookup strips a leading alphabetic character, so `MA146` → `A146`. Our drives' `xMA146` needs *two*
strips, which libgpod does in two places — its `SysInfo` reader drops the `x`, then the table lookup
drops the `M`. Reproducing that as two conditional strips is fragile, so `Model::lookup` takes the
**last four characters** and requires the final three to be digits: that accepts `xMA146`, `MA146`
and `A146` alike, and rejects strings that are not model numbers.

### So: this is a 30 GB **black** 5G, and the logo confirms it

`A146` = 30 GB, black, 5G. **Three independent fields agree**, which is what makes it a fact rather
than a reading:

| source | says |
|---|---|
| the NOR's `Mod#` | `MA146` → 30 GB, black, 5G |
| the NOR's `HwVr` | `0x000B0005` → 5G |
| the drive's `SysInfo` | `xMA146`, `boardHwRev: 0x00050000`, `BoardHwName: PP5021C-2` → 5G |

And a fourth, from a completely different direction: **`research/14` recorded that this NOR draws a
white Apple logo on a black background.** A black iPod has a white-on-black boot logo and a white one
has black-on-white — which was offered here as a technician's twenty-year-old recollection, and is
now corroborated by a model number that independently says *black*. It is one unit, so this confirms
the pair rather than proving the general rule; but the general rule now has evidence rather than
memory behind it.

**The default iPod colour therefore comes from the hardware**, not from a constant somebody picked:
read `Mod#`, look it up, use what it says. A synthesised NOR sets `Mod#` from the model the user
chose, and the colour follows.

`generation_agrees()` cross-checks `Mod#` against `HwVr` and returns `Option<bool>`, so "not checked"
stays distinguishable from "agrees" — a bare `false` for absent data is how a check ends up quietly
never running.

### And it settles §"The wrinkle" above

That section asked *"which chip are we"* and said nothing goes in the model until the NOR's `SCfg`,
the handoff block and the drive's `SysInfo` are compared. **They have now been compared, and they
agree: this is a 5G.**

Which means `ipod_is_pp5022()` returning false is **correct on this hardware**, not a symptom of our
model answering wrong — a PP5021C has `'1'` where the test looks for `'2'`. `ipodloader2` selecting
the PP5002 path from that is **its own gap on a 5G**, not a value we need to invent. The remaining
question is narrower than it was: not *what should `PP_VER1` be*, but *does `ipodloader2` have a
PP5021C path at all* — and the honest answer from reading `ipodhw.c` is that it does not.

### Checked: iTunes does **not** carry a model table

The obvious objection to leaning on libgpod is that it is a community reconstruction, and Apple's own
software is right there. So it was checked, against the Windows VM in `resources/vm/` — iTunes
12.8.0, `iPodService.exe`, `iPodUpdaterExt.dll`.

**There is no model table in any of them.** Not one of `A146`, `A446`, `A002`, `A448`, `A452` occurs
as a string in `iTunes.exe`, `iPodUpdaterExt.dll` or `iPodService.exe`. A lookup keyed by the
`ModelNumStr` a device reports would have to hold those codes as text; none does.

What iTunes actually has:

| where | what |
|---|---|
| `iPodService.exe` | a **family-name** table — `iPod`, `iPod Mini`, `iPod Nano`, `iPod Photo`, `iPod Shuffle`, `iPod Touch Wheel`, `iPod with Video` — keyed by `FamilyID` |
| `iPodDevices.xml` | a per-device **connection log**: GUID, serial, `Family ID`, `Updater Family ID`, firmware version, use count |
| the updater `manifest.plist` | `FamilyID`, `UpdaterFamilyID`, and a `DefaultColor` that is the placeholder string `XXX` for family 6 |

**iTunes never needed a model table**, which is why it has not got one: the device reports its family
and its GUID, and that is enough to pick an updater and remember the device. The colour/capacity
mapping lives in Apple's published "Identify your iPod model" pages, which is where libgpod's table
came from — so libgpod remains the best available source, and the reason is now known rather than
assumed.

**This was checked with `strings` and `grep`, not with Ghidra**, and the limit of that is worth
stating: string-scanning finds names but not the arrays that index them. It is conclusive *here* only
because the keys would themselves have to be strings — a table keyed on `ModelNumStr` cannot store
its keys as anything else. A question about *structure* rather than presence would need the
disassembler.

### A 5.5G sighting, not yet a fact

The same scan turned up a third device's `SysInfo` on that disk, with `ModelNum` **`MA446`** — 30 GB
black **5.5G**, a model we do not hold. It also appeared to read `BoardHwName: iPod M25` rather than
the `PP5021C-2` our own two report.

**That is recorded as a sighting, not as data.** The region it came from was fragmented — neighbouring
fields read as `ModelNumS0`, `Mode`, `Firew`, `Guid` — so `iPod M25` may be a torn read rather than a
string. It wants confirming from an intact copy before anything is built on it. What survives without
qualification is that `MA446` exists on that disk at all, which is one more corroboration of the
libgpod row.

**Also recorded, because it is the receipt for a claim made elsewhere:** `iPodDevices.xml` holds our
own NOR's serial and GUID, `Family ID 6`, with a use count and a last-connected date. **iTunes
accepted the identity a virtual iPod presented.** `research/02` said the Gestalt ID work happened "in
the USB research (not published)"; this is that work's footprint, and it is why
[`Identity::title_auth`](../tools/eapp-loader/src/identity.rs) has three states rather than two.

That file also contains **several other people's serials and GUIDs**. It is not copied into this
repository, and no value from it is written down here — the same rule `research/07` already carries.

### Settled with Ghidra: how Apple's own software identifies an iPod

The `strings` result above — that no model code appears in iTunes — was a negative, and a negative
from a text scan is weak evidence. Two things were done about that.

**First, the scan was redone in UTF-16.** The original searched ASCII bytes only, and iTunes on
Windows is a Unicode program, so a literal `MA146` would very plausibly be stored as
`4D 00 41 00 31 00 34 00 36 00` — which an ASCII `grep` cannot match. **The conclusion survived**:
none of `MA146`/`MA446`/`A146`/`A446`/`A002`/`A448`/`A452`/`A664` occurs in `iTunes.exe`,
`iPodUpdaterExt.dll` or `iPodService.exe` in *either* encoding.

**Second, `iPodService.exe` was disassembled** (Ghidra 12.1.2, headless, x86-64 PE). That turns the
negative into a positive: we no longer merely fail to find a model table, we can see what Apple
uses *instead*. Its own C++ symbol names survive in assert strings:

| symbol | what it does |
|---|---|
| `CCheckpointData::Initialize` | reads a **Checkpoint** off the device via `SCSIGetVitalPages`, paged, `MinPageIndex..MaxPageIndex` |
| `CAppleUsbControl::GetCheckpointData` | the same over USB — `AppleUsbCheckpointRequest` |
| `CCheckpointData::ParseCheckpoint` | parses it as **XML** |
| `CIpodDevice::GetSysInfoData` | the fallback: reads `:\iPod_Control\Device\SysInfo` off the disk |
| `CiPod::CalculateBuildAndFamily` | derives build + family when there is no checkpoint |
| `CIpodDevice::ParseInterfaceStringForSerialNumber` | takes the serial from the USB interface string |

And the fields it parses are spelled out as literal plist paths:

```
plist/dict/SerialNumber:string/
plist/dict/UpdaterFamilyID:integer/
plist/dict/Versions:dict/VersionsIndex:dict/iPodFamily:integer/
plist/dict/Versions:dict/VersionsIndex:dict/updaterFamily:integer/
```

with the failure message naming the complete set it needs:

```
XML_Parse for device succeeded, but no family (%d), BuildID (%d), BuildVersion (%d), or GUID (%d)
```

**Serial, family, updaterFamily, BuildID, BuildVersion, GUID. No model number. No colour. No
capacity.** That is not an inference from absent strings any more — it is the field list Apple's own
device-management code asks for. **iTunes has no model table because iTunes never identifies a model**;
it identifies a *family* and a *device*, and family is all it needs to choose an updater.

So `Mod#` → colour/capacity/generation genuinely has no Apple-software source to recover, and
libgpod's table — sourced from Apple's published "Identify your iPod model" pages — is not a
second-best. It is the only place that mapping has ever existed outside Apple's website.

**A side finding that belongs to M9.** The Checkpoint is a real mechanism this project has not
modelled: an XML plist the device serves over SCSI vital pages *and* over USB. Anything presenting
itself to iTunes has to answer it, and `CIpodDevice::GetSysInfoData` reading
`iPod_Control/Device/SysInfo` is the documented fallback when it cannot.

The decompilation is kept at `resources/derived/ghidra/` — outside git, like everything else under
`resources/`.

### How iTunes knows the colour: **the iPod tells it**

The colour question kept coming back because iTunes visibly showed the right one, and no model table
could be found in any Apple software. Both facts are true, and the resolution is that the second was
never needed. `AMPDevicesAgent` (macOS 27) carries the Checkpoint field dispatcher, and among its
typed setters:

```
fieldSize == sizeof(IPodColor)
SetCheckpointDeviceColorValue((CFStringRef) value, (IPodColor *) fieldPtr)
```

**The device colour arrives as a string in the Checkpoint plist** and is parsed into an `IPodColor`.
There is no model-number lookup because the iPod reports its own colour. That also explains the
artwork: `iPod6-White` / `iPod6-Black` / `iPod6-BlackRed` are selected by family plus the colour the
device stated.

The same dispatcher shows what else the Checkpoint carries — `SetCheckpointFairPlayGUIDStringValue`,
`SetCheckpointRGBColorValue`, `SetCheckpointNumVersionValue`, `SetCheckpointMediaKindsValue`,
rotation, gamma, capacity. **This is the specification M9 has to satisfy and M5 has to generate**,
and it is Apple's own field list rather than a reconstruction.

### The Apple-authored variant discriminator

Three iPod restore images were recovered from the Windows VM, with Apple's own `manifest.plist`:

| variant | firmware | `FamilyID` | `UpdaterFamilyID` |
|---|---|---|---|
| 5G Initial | Firmware-13.6.3 | 6 | **13** |
| 5G RevA | Firmware-20.6.3 | 6 | **20** |
| **5.5G** | Firmware-25.6.3 | 6 | **25** |

**`FamilyID` does not separate the 5G from the 5.5G — `UpdaterFamilyID` does.** Our own drive's
`SysInfo` says `updaterFamily: 13`, which is 5G Initial, agreeing with `Mod#`, `HwVr` and
`boardHwRev`. And the 5.5G firmware payload now exists here (`Firmware-25.6.3`, 13 905 920 bytes
against the 5G's 13 895 680) — the thing an earlier session went looking for and did not find.

### `.ipod` model numbers for every model, from ipodpatcher

`getmodel()` in `rbutil/ipodpatcher/ipodpatcher.c` maps the firmware partition's version word to the
`.ipod` header fields this project already writes for the Video:

| `ipod_version` | model | `modelnum` | magic |
|---|---|---|---|
| `0x01` | 1st/2nd Generation | 19 | `1g2g` |
| `0x02` | 3rd Generation | 7 | `ip3g` |
| `0x40` | Mini 1G | 9 | `mini` |
| `0x50` | 4th Generation | 8 | `ip4g` |
| `0x60` | Photo/Color | 3 | `ipco` |
| `0x70` | Mini 2G | 11 | `mn2g` |
| `0xb0` | **Video (5G)** | **5** | **`ipvd`** |
| `0xc0` | Nano 1G | 4 | `nano` |
| `0x100` | Nano 2G | 62 | `nn2x` |

That is the checksum seed and magic for nine model families, from the code that writes them.

## The experiment the retraction said was blocked — run, 2026-08-19

§"Confirmed by forcing it" was retracted because forcing `PP_VER1` does not touch only
`ipodloader2`: **Apple's bootloader reads that register 23 times a boot**, and with the value forced
it hangs at `0x400038cc` before `ipodloader2` runs at all. The retraction's closing line was that the
fix "has to satisfy Apple's bootloader *and* `ipodloader2` at once".

**The high-level boot removes that constraint entirely.** It never runs Apple's bootloader — it maps
the OS out of the drive's firmware partition and enters it directly — so `PP_VER1` can be answered
for `ipodloader2` without anything else reading it.

Measured, with `ipodloader2` installed via `install-os` and entered at the directory's own entry
offset (`0x10736000`), both runs identical but for the one register:

| | ATA commands | unmapped |
|---|---|---|
| as-is | **0** | **30 437 746 reads, 99 755 writes across 14 pages** |
| `--rdval=0x70000000=0x3232432D` | **1** — `IDENTIFY DEVICE` | **none at all** |

`0x3232432D` is `'2','2','C','-'` — bytes 4..8 of the eight-character `PP5022C-` that `PP_VER2` and
`PP_VER1` spell together, and its byte 16 is the `'2'` that `ipod_is_pp5022()` tests for.

**So the loader was never broken, and neither was our model.** With the chip identified it stops
addressing a 1G iPod's registers at `0xcf00xxxx`, relocates into IRAM, and issues `IDENTIFY DEVICE`
to the drive. Thirty million wrong reads become none.

What this does **not** settle: whether a real 5.5G answers `'2'` there. Our reference hardware is a
5G whose `BoardHwName` is `PP5021C-2`, and a PP5021C answering `'1'` is why `ipod_is_pp5022()`
correctly returns false on it. The register is being *supplied* here, as the emulator supplies every
register — the value is sourced from Rockbox's `debug-pp.c` decoding, not measured off a 5.5G.

---

## The two bugs were upstream's, and fixing them boots a Linux kernel

**2026-08-19.** This note ends with the loader misidentifying the chip and stalling before the
drive. With `0x70000000` bits 23:16 answered `'2'` it gets past that — and then prints its own
console, which this note records as never having been observed:

```
iPL 2.9.0d
iPod: 000b0005
HDD identify OK (no checksum)
HDD Model: Emulated iPod Di
Detected WinPod MBR
[0]: Bad iPod FW entry
[1]: Unknown 0xC2
No valid paritions found!
```

Both of those lines are **ipodloader2's own bugs**, read out of the source it is built from, and
neither belongs to this emulator.

**1 — the firmware-partition test is inverted.** `vfs.c:193`:

```c
if( mlc_strncmp((void*)(fs_header->fwfsmagic),"]ih[", 4) ) { validoffset = 1; }
```

`mlc_strncmp` returns **0 on a match** — `minilibc.c:452`, `if(!length || (*s1 == 0 && *s2 == 0)) return(0);`.
So the partition is accepted only when the magic does *not* match. The bytes at `0x100` of the
partition's first sector are `5d 69 68 5b` — `]ih[` — which is correct, and the loader rejects it.

**2 — there is no case for FAT32-LBA.** `vfs.c` handles `case 0x00`, `case 0x83` and `case 0xB`.
The MBR here says partition 1 is type **`0x0C`**, FAT32 with LBA addressing, which is what iTunes
on Windows produces and what this project's own `make-disk` writes. It falls to `default:`.
(`0xC2` in the output is the format string: `"Unknown 0x%X2"`, with a literal `2`.)

`tools/patches/ipodloader2-vfs.patch` fixes all three. Rebuilt and installed with
`ipod-boot install-os`, into a drive carrying the ZeroSlackr `vmlinux` and a `loader.cfg` pointing
at it:

| | before | after |
|---|---|---|
| ATA commands | **3** — `IDENTIFY`, the MBR, one probe | **3 194** |
| frame updates | 8 | **97** |
| non-black pixels | 7 538 | 74 419 |

![ipodloader2 loading a Linux kernel](../docs/media/ipod-26-ipodlinux-loaded.png)

> `Load succeeded` · `Jmp to 10000000`

**And the kernel executes.** Not "is loaded" — runs: the machine ends inside
`ldmia sp, {r0-pc}^`, an ARM exception return restoring user-mode registers, having taken
interrupts. That is a Linux kernel servicing its own traps.

**Where it stops is named.** It polls an address nothing here models:

```
unmapped: 8 385 336 reads across 1 page
  0x64004000..0x64004103   8 385 336 reads   from 0x000177b4 and 0x000177d4
```

`0x64004000` appears in **no** register map available to this project — not Rockbox's `pp5020.h`,
not ipodloader2's own headers. Two PCs a few instructions apart, inside the interrupt path, each
reading it four million times. That is the next question, and it is a much better one than the one
this note started with.

### `--rdval=0x70000000` is an ABLATION, not a fake — and I had to break the machine to be sure

**2026-08-19.** The `loader` recipe's `--rdval=0x70000000=0x3232432D` looks exactly like the thing
this project is trying to delete: a per-guest value that one operating system needs. So it was
attacked on the theory that the register should simply be truthful for everybody, and the attack
**failed in a way worth keeping**.

The reasoning that started it was: Apple's own boot ROM writes its `IsyS` tag and pointer to
`0x4001ff18`/`0x4001ff1c` and **never touches `0x40017f18`** — measured — and those are precisely
the two addresses `ipodhw.c`'s `ipod_set_sysinfo` picks between on the strength of this register. So
the ROM appeared to have already answered the question about its own part, and `PP5022C-` looked
like the honest reading.

It is not. `0x70000000`/`0x70000004` were made read-only and seeded with `PP5022C-`, and **the
retail boot went from 599 ATA commands to 0.** Apple's bootloader reads bits 16..23 and wants
`0x36`; handed `0x32` it takes a path it cannot finish. That is the outcome this file predicted
before the experiment, and `lib.rs` already carried the measurement and the decision in a comment
nobody re-read:

> *Apple's bootloader reads bits 16..23 and compares against `0x36`, taking its PP5021C path when it
> matches… `ipodloader2` reads the same bits and compares against `0x32`… Two drivers want different
> answers from one register, and Apple's is the one measured off real firmware, so it wins.*

**So the machine was already right, and the sysinfo-pointer inference was wrong** — where the ROM
parks that pointer is a statement about IRAM size, not about the part's name. Reverted; 599 ATA and
2 916 non-black pixels restored.

**The distinction this buys is the useful part.** Two different things wear the same `--rdval`
costume:

| | what it covers | what to do |
|---|---|---|
| a **fake** | our own failure to model a register | delete it by modelling the register |
| an **ablation** | a defect in the *guest*, on hardware we emulate correctly | keep it, label it, never put it in a recipe that measures the machine |

This one is the second kind. **This is a PP5021C, its byte is `'6'`, and `ipod_is_pp5022()`
correctly answers false** — the bug is that `ipodloader2` only distinguishes PP5022 from
everything-else and has no PP5021C path, so it falls back to 1G addressing. Forcing `'2'` is
telling the loader a lie that happens to route it past its own missing branch, which is a legitimate
thing to do on purpose and an illegitimate thing to do silently. It belongs in the ledger beside
`--force-sem`, and the `loader` recipe should say so on the line that passes it.

### The recipe, which was the actual blocker — 2026-08-19

This file recorded the numbers and not the command that produced them, and the cost of that was a
day spent chasing a regression that did not exist. Cold-booting the same image gives **71 ATA
commands** and 184 283 902 reads of `0xcf00101c`; the binary built from the very commit that
recorded 3 194 gives the identical figures, to the read. Nothing had broken. The invocation was
simply not written down, and it is now `ipod-boot loader` (aliased `ipodlinux`).

Three flags are load-bearing, and each fails differently:

| flag | why | without it |
|---|---|---|
| `--osos-from-disk` | the loader is **appended after** RetailOS in the same `osos` image, at entry offset `0x735a00` | boots the OS sitting behind the loader |
| `--rdval=0x70000000=0x3232432D` | `ipod_is_pp5022()` is `(inl(0x70000000) << 8) >> 24 == '2'` — `ipodhw.c:27`, byte 2 of `PP_VER1`, spelling `-C22` | takes the not-PP5022 branch |
| `--sysinfo` | **the one that produces the symptom.** With the chip identified, `ipod_set_sysinfo` dereferences the *PP5022* pointer at `0x4001ff1c` | no `IsyS` there → `hw_rev` 0 → `hw_ver` 0 → every register access goes to a **1G iPod's** `0xcf00xxxx`, forever |

`--sysinfo` alone is the difference between **184 283 902 unmapped reads and none at all**, and
between **0 ATA commands and 3 196**. A loader spinning on a 1G's registers looks exactly like a
broken emulator and is in fact a correctly-emulated loader that was never told what it is running on.

It stays a high-level boot deliberately: it never runs Apple's bootloader, which is what allows
`PP_VER1` to be answered for `ipodloader2` without anything else reading it. Apple's own bootloader
reads that register 23 times and hangs at `0x400038cc` when it is forced.

### The kernel boots, and it says so — its own console, read out of `printk`

With the mirror in place the unmapped report is **empty** on a 12 G run, where this file recorded
8 385 336 reads of `0x64004000`. The kernel no longer spins on anything. It ends at

```
00024cb8  b  0x00024cb8
```

which is not a hang on hardware: `0x00024c78` loads `"<0>In idle task - not syncing"` and calls
`0x000255e4`, and the three instructions before the self-branch are `mrs r3, cpsr` /
`bic r3, r3, #0x80` / `msr cpsr_c, r3`. That is the tail of Linux's **`panic()`**.

`0x000255e4` is therefore `printk`, and `--enterlog` on it turns `r0` into a console, because
`vmlinux` is a raw image loaded at 0 and the format string's address *is* its file offset. 163 calls,
decoded:

```
Linux version 2.4.32-ipod2 (Keripo@tehhomebase) (gcc version 3.4.3) #1 Sun Apr 27 11:44:04 EDT 2008
Architecture: iPod
Memory: 30128KB available
Calibrating delay loop... 74.xx BogoMIPS
ipodaudio: (c) Copyright 2003-2005 Bernard Leach   ·  codec WM8758
devfs: v… Richard Gooch          ·  fb0: … frame buffer device
pty: 256 Unix98 ptys configured  ·  loop: loaded (max 8 devices)
Uniform Multi-Platform E-IDE driver Revision: 7.00beta4-2.4
…
end_request: I/O error, dev …, sector 0     (and 2, 4, 6 — twice each)
 unable to read partition table
…: INVALID GEOMETRY: 63 PHYSICAL HEADS?
NET4: Linux TCP/IP 1.0 for NET4.0
VFS: Cannot open root device … 
Please append a correct "root=" boot option
Kernel panic: …
```

**The whole kernel comes up.** Memory, the framebuffer, ptys, the E-IDE driver, TCP/IP. What it
cannot do is mount a root filesystem, and there are two separate reasons, one of which is ours and
one of which is not:

1. **Not ours.** `loader.cfg` on this image says `console=ttyS0 quiet` and **contains no `root=` at
   all**, so `Please append a correct "root=" boot option` is a literally accurate report about a
   file this project wrote. There is also no Linux root filesystem on the drive to name.
2. **Ours, and open.** The eight `I/O error`s are on **sectors 0, 2, 4 and 6** — the MBR, in 1 KB
   blocks — which Apple's bootloader, Rockbox and `ipodloader2` all read off the same model without
   complaint. Linux's `ide` driver is being refused something they do not ask for.

**One IDENTIFY defect was found and fixed on the way, and it did not turn out to be the cause.**
Word 53 bit 0 advertised words 54-58 as valid while they were left zero — a validity bit with
nothing behind it. They now carry the current geometry (`w[54..56] = w[1]/w[3]/w[6]`) and the CHS
capacity ceiling in `w[57..58]`, with `identify_does_not_advertise_fields_it_leaves_empty` as the
regression, confirmed failable. **The kernel log is byte-identical either way** — same 163 printks,
same eight I/O errors, same `INVALID GEOMETRY: 63` — and the retail boot is unmoved at 599 ATA
commands and 2 916 non-black pixels. So it is a real defect corrected on its own terms, and it is
**not** the thing Linux is tripping on. Said plainly rather than filed as a fix for a bug it did
not fix.

**63 is the number to chase.** Nothing in our IDENTIFY reports 63 heads: `w[3]` is 16, `w[6]` is 63,
and `w[55]` is now 16. Linux ending up with `drive->head == 63` means it is reading a field one slot
from where we put it — and `ipodloader2`, reading the same sector through its own PIO path, prints
`CHS: 16383/16/63` correctly. Two readers of one buffer disagreeing is the next measurement.

### `0x64004000` is the interrupt controller, and the kernel's own code says so — 2026-08-19

The address is not absent from the map; it is the map, plus one bit. `vmlinux` is a raw ARM image,
so file offsets are addresses, and the constant pool for those two PCs sits at `0x17b50`:

```
0x000177b0  ldr r5, [pc,#920]      ; = 0x64004000   (pool 0x17b50)
0x000177b4  ldr r6, [r5,#0]
0x000177b8  tst r6, #0x1           movne r0, #0
0x00017794  tst r6, #0x10          movne r0, #4
0x000177a0  tst r6, #0x800         movne r0, #11
0x000177d0  ldr r5, [pc,#892]      ; = 0x64004100   (pool 0x17b54)
0x000177d4  ldr r6, [r5,#0]
0x000177c4  tst r6, #0x800000      movne r0, #23
0x000177fc  tst r6, #0x100         movne r0, #40
```

Load, dereference once, then a bit-by-bit source scan. **Bit 8 maps to IRQ 40** — the click wheel
this emulator already delivers at `0x60004000` — and the second load is the high bank at `+0x100`,
which is where this project's own notes put it. Two firmwares written by different people, the same
register, the same bit assignments, `0x04000000` apart.

That offset is not new here either: SDRAM's uncached view is `0x14000000` over `0x10000000`, and the
ROM computes it itself (`bic #0xfc000000` / `orr #0x14000000`). Bit 26 is the uncached mirror on this
part, and a kernel reaching MMIO through the uncached alias is a kernel doing the ordinary thing.
`map_hardware` now registers `0x64000000..0x64100000` as a view of the device window, with
`the_device_window_is_mirrored_where_the_kernel_reads_it` as the regression — confirmed to fail
against the un-aliased build before it was kept.

**It changes nothing yet, and that is stated rather than buried.** A/B'd on the retail boot to 1.8 G
it is inert to the pixel: 599 ATA commands and **2 916 non-black pixels in both arms**. And on the
iPodLinux path it cannot be evaluated, because *this file's own measurement no longer reproduces*:
`ipl-patched.img` cold-booted today reaches **71 ATA commands**, not the 3 194 recorded above, and
spins 891 499 514 times on `0xcf00101c` — the **PP5002** I/O base, which is the wrong-chip-family
branch — with the arms identical. So there are now two open questions where there was one, and the
second is the more urgent: **what regressed, or which image the 3 194 was measured on.** The mirror
is kept because its evidence is the kernel's own instructions, not because it was seen to help.

### It is not a missing `root=`, and the kernel never asks the drive who it is — 2026-08-19

The panic reads like a configuration problem — `loader.cfg` on this image really does say only
`console=ttyS0 quiet` — but that is not what stops it. Two lines above the panic the kernel says

```
 unable to read partition table
…: INVALID GEOMETRY: 63 PHYSICAL HEADS?
```

so there is no `/dev/hda2` for a `root=` to name even if one were supplied. And the kernel has
`ext2`, `ext3`, `vfat` and `msdos` all compiled in — 21, 88, 2 and 43 string hits in the image — so
it is not short of a filesystem either.

> ~~**The uncapped ATA census is the finding: `cmd 0xec` appears exactly once in the whole run.** The
> kernel never issues IDENTIFY DEVICE at all~~ — **wrong, and wrong in the way this project keeps
> warning about.** That count came from grepping the command *list*, whose own heading says
> `SAMPLE, NOT A CENSUS` on the line above it. There was an uncapped `cmd_census` in `Ata` the whole
> time with **no printer**, so the honest number was collected and never shown. Printed, it reads:
>
> ```
> ata commands: 3196  (log below shows the first 256 — SAMPLE, NOT A CENSUS)
>   by command (uncapped census): 0x20 READ SECTORS x3190 · 0xe0 STANDBY IMMEDIATE x3 · 0xec IDENTIFY x3
> ```
>
> **Three IDENTIFYs** — the loader's, and two more. The kernel does ask the drive who it is, and gets
> an answer; what it does with that answer is the open question, not whether it asked.

What is established: the run reports **zero unmapped accesses**, so the kernel is not driving an
address we fail to model, and the image holds **no pool reference** to `0xc30001e0`, `0xc3000000` or
`0xc0003000`, so its IDE base is computed rather than a literal.

**And the root device is named in the kernel, not missing from it.** `root=/dev/hda3` sits at
`0x12efa` as the compiled-in default command line — so `Please append a correct "root=" boot option`
is the generic message for *a named device that could not be opened*, and what iPodLinux wants is the
classic **third partition** with an ext2/ext3 filesystem. This drive has two. Fixing the partition
read is necessary but not sufficient; a working iPodLinux drive also needs an `hda3` to mount.

So this is the same family as the two bugs that turned out to be ours — a device the guest cannot
get into a usable state — and **not** the content problem it was written up as. The next measurement
is where the kernel's `ide` probe stops: its own `probe_for_drive` never reaches a command register
we own, and finding out what it drives instead is one `--watch-range` over the window it should be
using once the base is known.

### The patch was compensating for our test disk, and is deleted

**The sharpest question asked of this work: why are we patching software that runs on real hardware?**
The answer is that we were not fixing `ipodloader2`; we were papering over the drive we handed it.

`vfs.c` handles partition types `0x00`, `0x83` and `0xB` — and `0x0C` falls to `default:` and is not
counted, so `foundpartcount` reaches zero and it prints `No valid paritions found!`. Every loader
disk in `_out/` is `0x0C`, because they descend from `ipod8g-retail.img`. **This project's own
`make-disk` writes `0x0B`** — `ipsw.rs:674`, with a test asserting it — which is the type upstream
expects.

Built clean from `crozone/ipodloader2` at `a41ec49` with **no patch at all**, and pointed at a drive
whose data partition is `0x0B`:

```
Detected WinPod MBR
[0]: Bad iPod FW entry
[1]: FAT
FAT32 detected.
…
Load succeeded
Jmp to 10000000
```

**3 209 ATA commands**, the kernel loaded and entered. `[0]: Bad iPod FW entry` is the inverted
`mlc_strncmp` this file describes — `mlc_strncmp` really does return 0 on a match
(`minilibc.c`), so the firmware partition really is rejected — and it is **cosmetic**: the loader
boots from the FAT32 volume and never needed partition 0. A real defect, upstream's to fix, and not
one that stops anything.

So `tools/patches/ipodloader2-vfs.patch` is **deleted**. What it bought was a `0x0C` drive booting;
what it cost was a fork of a guest, and a claim in this file that two upstream bugs were blocking us
when one was cosmetic and the other was our fixture. **Nothing in this project now patches any
operating system it runs.**

### `'6'` is not a preference — it steers the bootloader away from hardware we do not model

**Measured 2026-08-20, with only the value changed** (the register left writable, so this is one
variable rather than the two the first attempt moved). Seed `PP5022C-` — `'2'` where Apple looks —
and the retail boot goes from 599 ATA commands to **0**, ending:

```
last instructions:
  400038cc  ldr r0, [r4, #0x28]
  400038d0  tst r0, #0x80
  400038d4  beq 0x400038cc          <- forever

unmapped: 8 reads, 4 writes across 1 pages
  0xc5000140..0xc5000143   8 reads  4 writes   first pc 0x400011dc
```

**`0xc5000140` is a controller this project does not model**, and Apple's bootloader touches it
*only* on the `'2'` path — our ATA is at `0xc3000000` and nothing else has ever gone near
`0xc5000000`. So the byte does not express a preference between two readers. It selects between two
hardware paths inside Apple's bootloader, and `'6'` is the one whose hardware we have.

That makes the value a **bypass in the ledger's sense** — steering firmware away from a device we
have not built — with a retirement condition that is now a specific address rather than an argument:
**model the controller at `0xc5000140`.**

**And it inverts the likely reading of which part this is.** Apple's own ROM writes its `IsyS`
pointer to `0x4001ff18`/`0x4001ff1c` — the slot `ipod_set_sysinfo` selects **when
`ipod_is_pp5022()` is true** — and never to `0x40017f18`. A machine whose ROM parks sysinfo in the
PP5022 slot is most likely a PP5022, in which case `'2'` is the true answer, the `'2'` path is what
a real 5G takes, and what stops us is the missing controller rather than the chip id. The `'6'` this
project reports is then the workaround and `--rdval` is a second workaround layered on the first.
Neither is established, and both now have one experiment between them and being settled.

### The two guests are not asking about different chips — they are testing the same byte

**Read out of Apple's own ROM at `0x9258`, which is better evidence than the comment that stood
here:**

```
9258:  mov r0, #0x70000000
925c:  ldr r0, [r0]
9260:  mov r0, r0, lsl #8
9264:  mov r0, r0, lsr #24        <- bits 16..23
9268:  cmp r0, #0x36              <- '6'
926c:  movne r0, #0
9270:  moveq r0, #1
```

`(v << 8) >> 24` is **character for character `ipod_is_pp5022()`** (`ipodhw.c:27`). One predicate,
one byte, two different letters wanted: Apple's ROM asks *is it `'6'`*, `ipodloader2` asks *is it
`'2'`*. There is one chip in an iPod and both are asking it the same question.

**And we were answering with something that is not a chip id at all.** `trace.rs` seeded
`0x00360000` — one byte in the position Apple reads and zeroes everywhere else. It passes Apple's
test and spells nothing: `debug-pp.c` renders an eight-character part name out of `PP_VER2` then
`PP_VER1`, and out of that value it renders four NULs, a `6`, and two more NULs. So the register had
no answer for the second reader to get right, and the `--rdval` in the `loader` recipe is what was
standing in for one.

It now carries a whole string — `PP_VER1 = 0x3236432d`, `PP_VER2 = 0x50503530`, spelling
`PP5026C-` — which keeps `'6'` exactly where Apple looks (**retail unmoved at 599 ATA commands and
2 916 non-black pixels**) and gives every other reader something coherent.

**What is still not established, and should not be dressed up:** whether a real 5G reports `'6'`.
This project has never measured that register on hardware. What is known is that *our* ROM contains a
predicate for `'6'`, that the ROM's name table at `0x10ae4` lists `PP5020`, `PP5022` and `PP5026`
and no `PP5021`, and that the community documentation calls this part a **PP5021C** — whose byte
would be `'1'` and would make **both** predicates false. The `26` in the string above is therefore
**constructed around Apple's comparison, not read off a part**, and the honest reading is that
`0x70000000` is one of the few registers here whose true value nobody in this project has seen.

### The kernel reads our IDENTIFY six bytes out of step — measured, not inferred

`drive->head` is the byte at `drive + 0x107`; the check at `0x000ebfc4` accepts 1..16 and complains
otherwise, and it holds **63**. 63 is our `sectors per track`, which is IDENTIFY **word 6**, sitting
where the kernel expects **word 3**.

That could be a coincidence of two identical numbers, so it was tested rather than argued. Four
fields were given distinct probe values in one build — `w[3] = 13` (heads), `w[6] = 61` (sectors),
`w[55] = 9` (current heads), `w[56] = 7` (current sectors) — and the kernel came back with

```
r3 = 0x0000003d      = 61
```

**`w[6]`.** Not 13, not 9, not 7. The kernel's `id->heads` is our `id->sectors`, so its copy of the
response is our copy **shifted left by three 16-bit words — six bytes.** Everything downstream
follows from that one displacement: a geometry that fails its own sanity check, a drive the driver
will not accept, and `unable to read partition table` from a disk whose MBR reads back byte-perfect
for every other guest.

**What that rules out.** It is not the `field_valid` words — `w[53]`'s bit 0 is set and `w[54..56]`
carry the current geometry, and the probe shows the kernel reading none of them. It is not a stale
buffer: `0xec` sets `buf = identify()` and `pos = 0` together. It is not a missing IDENTIFY —
**three** are issued, the loader's and two of the kernel's, per the uncapped census.

**What it leaves.** Six bytes is three halfword reads of the data port that our model serves and
hardware would not, or that the kernel makes before its transfer loop and our model answers from the
buffer instead of from a register. `Ata::read` maps `0x1e0..=0x1e3` to the buffer and pops one byte
per byte-read, so any read in that window advances it — including one the real controller would
answer from somewhere else. That is the next thing to instrument: log the first byte-reads of the
data port after each `0xec`, with their values, and see whether the kernel's first read is handed
byte 0 or byte 6.

**And the command line is not the problem, which the doc settles.** `ipodloader2`'s own
`IPOD_LINUX_INSTALL.md` gives ZeroSlackr's line as
`root=/dev/hda2 rootfstype=vfat rw quiet` — the root is the **FAT32 data partition** and the ext2
tree is a file loop-mounted out of it, so **no third partition is wanted** and this project's
two-partition drive is the right shape. Supplied as `/ipodloader.conf` (which `config.c:41` checks
*before* `loader.cfg`, so nothing had to be overwritten) the boot is unchanged: same panic, same
3 196 ATA commands. The kernel's compiled-in `root=/dev/hda3` at `0x12efa` is a default that
ZeroSlackr overrides and is not what it is failing on.

**What is NOT established:** that this kernel is right for this iPod. It is the ZeroSlackr
`vmlinux` dated 2008, and a kernel built for a different generation would also load and then poll a
register that generation has. The polled address being absent from the 5G's map is consistent with
either.
