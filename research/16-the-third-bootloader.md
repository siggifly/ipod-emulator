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
