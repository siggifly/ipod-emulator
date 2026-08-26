# The boot matrix — real, synthetic 5G, synthetic 5.5G

**Measured 2026-08-19.** Every row is a run, not a claim. The point of the table is that a
synthesised boot ROM now reaches the same place a real one does, and that where it does not is
named rather than left to be discovered.

## RetailOS

| boot ROM | firmware | outcome |
|---|---|---|
| real 5G dump, cold | `iPod_20.1.3` (updater 20) | boots — copyright banner, 597 M instructions |
| real 5G dump, cold | `iPod_25.1.3` (updater 25) | **70 ATA commands** — the wrong-family signature |
| **synthetic 5G** (`MA146`) | `iPod_20.1.3` | **boots** — 597 M instructions, 7 ATA commands |
| **synthetic 5.5G** (`MA446`) | `iPod_25.1.3` | **boots** — 597 M instructions, 7 ATA commands |

The second row is not a failure: `inspect::family_mismatch` documents exactly that shape — *"a
bundle from the wrong updater family boots, is not recognised as this iPod's own software, and
shows the plug-into-a-computer screen after about 70 ATA commands"*. A 5G ROM with 5.5G firmware
**should** land there, and it does.

## Rockbox

Warm-booted from `rb-main.raw` against a drive carrying `.rockbox`, with `--sysinfo` so the handoff
comes from the ROM under test.

| boot ROM | instructions | ATA |
|---|---|---|
| real 5G dump | 16 980 585 | 72 |
| synthetic 5G | 16 483 392 | 90 |
| synthetic 5.5G | 16 483 392 | 90 |

**The two synthetic runs are identical**, which is informative rather than suspicious: Rockbox is
built for a fixed target and does not read the generation out of the handoff, so a `MA146` and a
`MA446` present it the same machine.

**The real-versus-synthetic difference is not explained.** 72 ATA commands against 90, and about
500 000 fewer instructions. Both reach the same budget without faulting. It is recorded here
because an unexplained difference that nobody wrote down is one that gets rediscovered.

## `ipodloader2` — the row this file did not have

**Measured 2026-08-19**, `ipod-boot loader` against the same drive on all three ROMs.

**Which `ipodloader2`, recorded late.** These numbers are **`iPL 2.9.0d`**, built from upstream
`master` at `a41ec49` into `resources/vendor/ipodloader2/loader.bin` (57 676 B) — which is what
`ipod-boot install-linux` used to put in the firmware partition. It no longer does: since
2026-08-21 the command fetches the **v2.8.1 release** (56 912 B, SHA-256 on record), because the
vendored path is inside a gitignored directory and so worked only in this checkout. Nobody has run
this table against 2.8.1.

**Reproducing the rows is two steps, not a flag.** `IPOD_LOADER=` is **not** a pinned input in the
shape `FLASH=` and `DISK=` are — the run that produced these numbers was `ipod-boot loader`, which
is `--osos-from-disk`: it boots whatever bootloader is already in the drive's firmware partition and
never reads the variable. Only `install-linux` does. So the loader is pinned by rebuilding the
drive with it and then booting that drive:

```sh
IPOD_LOADER=resources/vendor/ipodloader2/loader.bin ipod-boot install-linux   # writes the drive
ipod-boot loader                                                              # boots what is on it
```

Setting the variable and re-running `ipod-boot loader` alone changes nothing, silently — which is
the shape §*The instruments lie* is about. (`install-linux` currently refuses on every drive tried;
see `KNOWN-BUGS.md`.)

| boot ROM | ATA commands | unmapped |
|---|---|---|
| real 5G dump (`MA146`) | **3 196** | none |
| synthetic 5G (`MA146`) | **3 196** | none |
| synthetic 5.5G (`MA446`) | **3 196** | none |

**Identical, to the command.** The loader reads the drive, walks the FAT32 volume, loads
`/boot/vmlinux` and jumps, and it does not care which of the three it booted through — which is the
useful result, because it says the generation fields the synthesiser writes are not something this
bootloader consults. The Linux kernel then boots in full and panics; that is
[research/16](16-the-third-bootloader.md) and is not a property of the ROM.

**One real dump exists.** `ipod-resources/roms/` holds exactly one file — `retail_5g_MA146`, a real
5G — and it is what every recipe here uses by default. The 5G and 5.5G rows above are built by
`ipod-boot make-nor --model MA146|MA446`, so "we are testing a 5.5G" is true only of the synthetic
row: **the default machine is a real 5G.**

## Re-measured 2026-08-20, after six ATA fixes — and the old table below is superseded

R4 applies in full: the machine now gets further with two of these firmwares than it ever has, so
every cell of the 2026-08-19 table was measured on a machine that no longer exists. This is what a
re-run says.

| | real 5G dump | synthetic 5G | synthetic 5.5G |
|---|---|---|---|
| **RetailOS**, cold | ✅ boots — 611 READ DMA, 4 WRITE DMA | — *needs Apple's bootloader* | — |
| **RetailOS**, high-level | ✅ | ✅ | ✅ *(the command line's high-level boot — see §2026-08-25)* |
| **Rockbox**, warm | ✅ **main menu, 74 057 lit pixels** | ✅ **74 057** | ✅ **74 057** |
| **ipodloader2** | ✅ its own console, `Found 1 valid partitions` | ✅ *(reached, via the loader recipe)* | ✅ |
| **iPodLinux** | ✅ **boots to ZeroSlackr's userland** | ✅ **same dmesg** | ✅ **same dmesg** |
| **diag** (service diagnostics) | ✅ **draws, and is drivable** | ⛔ **impossible** | ⛔ |
| **disk** (target disk mode) | ⚠️ faults — USB unmodelled | ⛔ | ⛔ |
| **aupd** (`flash-update`) | ✅ | ⛔ | ⛔ |

> **Corrected 2026-08-25, and the correction is about the word "high-level" rather than about the
> three ticks.** There are **two** high-level boots in this project and this row measures one of
> them. The command line's — `ipod-boot warm`, `trace --boot-osos` without `--cold-boot` — and the
> window's, `ipod-gui`'s `emu::build` when the ROM is synthetic. They are different machines, and
> the window's was ❌ on every column of this row until the day of this note. See
> §*2026-08-25: the high-level row was two boots, and only one of them was ever measured* below.

**Read the three columns as two questions, not one.** A synthesised NOR carries an identity block
and a reset vector and **none of Apple's code**. So:

- Anything **entered directly** — RetailOS high-level, Rockbox, `ipodloader2`, iPodLinux — does not
  care which column it is in, and after 2026-08-20 the numbers are identical across all three.
- Anything that **is** Apple's ROM cannot be synthesised. `diag`, `disk` and the `aupd` updater are
  images inside the NOR; a synthesised one has no `flsh` directory at all, and `ipod-boot flsh` says
  exactly that rather than failing obscurely:

  ```
  ipod-boot flsh: synth-5g.bin has no `flsh` image directory at all
  ```

  ⛔ in this table means *cannot, by construction* — not *broken*, and not *unimplemented*. Apple's
  service diagnostics needs a real dump and always will.
- **Cold RetailOS** is the third case: the image is on the drive, but reaching it means running
  Apple's bootloader, which is in the ROM.

**Rockbox's pixel count depends on the drive, not the ROM.** 74 057 is Rockbox with its own
`.rockbox/` resources on the volume — icons, backdrop, fonts. Against a drive without them it falls
back to the built-in 8 px font and draws 3 858, which is a complete and correct boot that merely
looks bare. An earlier version of this table recorded 3 858 as the number without saying which drive
it came from, which made a resource question look like a ROM question.

### FIXED the same day: the pin that says the co-processor is powered

`GPO32_VAL` bit 14 is a general-purpose output Apple's bootloader drives when it brings the BCM up,
and a warm entry skips that bootloader. Rockbox's `lcd_init_device` keys on it directly:

```c
if (GPO32_VAL & 0x4000) { display_on = true;  tick_add_task(&lcd_tick); }
else                    { display_on = false; lcd_awake(); }   /* only reached via ROLO */
```

`lcd_update_rect` returns immediately while `display_on` is false. So **every warm boot this project
has ever run took the recovery branch meant for ROLO** — and got away with it only because that
branch re-uploads the co-processor firmware out of `flash_get_section('vmcs')`, which a real dump
carries and a synthesised NOR does not.

| Rockbox warm, non-black pixels | before | after |
|---|---|---|
| retail 5G | 3 858 | **3 858** |
| synthetic 5G | 0 | **3 858** |
| synthetic 5.5G | 0 | **3 858** |

Retail unmoved; both synthetic cells now match it exactly. Rockbox's main menu, on a NOR carrying no
Apple code at all.

**The instrument nearly lied first, and that is the part worth keeping.** `--norlog` counts through
the `Nor` model, and the warm recipes install no such model — so it reported `0 flash reads` for
*both* arms, and "Rockbox never reads the NOR" was believed on the strength of it. A control on the
cold recipe returned **107 622**, which is how the blindness surfaced. It now prints `NOT MEASURED`
rather than `0` when there is no model to count through. Eighth of its kind in this project.

The paragraph below is what the failure looked like before the cause was known, and is kept because
the reasoning in it was sound and still led nowhere until the source was read.

**Rockbox on a synthetic NOR is a display failure, not a boot failure — and that is measured, not
inferred.** Both arms issue the identical `0xc6 ·  0xc8 x2065 ·  0xec x2 ·  0xef x2`, so Rockbox
loads its whole binary off the disk either way, and both end at **the same instruction** —
`0x00086300`, inside `switch_thread`. It is running its scheduler on both. What differs is that
nothing reaches the co-processor surface at `0xE0000`. Ledger #6 is the neighbourhood.

~~**iPodLinux on a synthetic NOR ends at `Lost(0x40020000)`**~~ — **fixed by the same one line.**
The `GPO32_VAL` bit-14 handover repaired this cell too: the kernel now runs to
`EXT3-fs: mounted filesystem with ordered data mode.` on a synthetic NOR, the same dmesg it produces
on the real dump, and the panel carries ZeroSlackr's startup screen. **Three cells, one line** —
Rockbox on both synthetic NORs and iPodLinux on the 5G one.

**And one cell is not the emulator at all.** `ipodloader2` reads FAT32 partition type `0x0B` and no
other — `vfs.c` has `case 0x83` for ext2 and `case 0xB` for FAT32, and nothing else. Every drive
image here taken off real hardware is `0x0C`, the LBA form, and the loader's own console says so:

```
Detected WinPod MBR
[0]: Bad iPod FW entry
[1]: Unknown 0xC2          <- mlc_printf("0x%X2"), so the trailing 2 is a literal
No valid paritions found!
```

A real 5G with a `0x0C` volume would fail identically on real hardware. `install-linux` refuses
those drives now rather than writing 1 776 files onto a disk that cannot boot.

## 2026-08-25: the high-level row was two boots, and only one of them was ever measured

**A first run on a synthesised 5.5G, pressed Start, died in a third of a second.**

```
stopped: lost 33554432 at 8388485 instructions
```

`33554432` is `0x02000000` and it is a **program counter**, not a size — `Stop::Lost(pc)`, the CPU
having left every mapped region. Reproduced from the command line, deterministically, on the
identity the window minted:

```sh
ipod-boot make-nor --model A446 --seed 1904983579699507775 repro.bin   # JQ652FQUX3N
IPOD_EMULATOR_DATA=<scratch> ipod-emulator --headless=10000000
```
```
  high-level boot: 7561216 bytes of OS from …/my-5.5g.img -> 0x10000000
  identity: JQ652FQUX3N · 000A27CB851F81B9
headless: Lost(33554432) after 8388485 instructions
  ata commands: 0        2097122 code buckets executed        bcm: 0 kicked, 0 frame updates
```

**The arithmetic names the failure before anything is opened.** `8 388 485 × 4 = 33 553 940`, and
`0x02000000 − 33 553 940 = 0x1ec`. The CPU executed a straight line from `0x1ec` to the top of a
32 MB window without taking one branch — a NOP slide, because `0x00000000` decodes as
`andeq r0, r0, r0`. `2 097 122` code buckets at 16 bytes each is the same 32 MB counted the other
way. Nothing was faulting; the code had gone out from under the program counter.

### What the oracle does, and what we were doing instead

`ipod-boot retail` on a drive that boots — Apple's own bootloader, in the emulator, doing the thing
the high-level boot exists to imitate:

```
  lba 14692  -> 0x10720000  90112 bytes
  last: lba 14692 -> 0x10720000 + 90112 = 0x10736000
…
Running 'osos' 0 from 0x10000000
```

58 `READ DMA` commands put the firmware partition **into SDRAM** at `0x10000000..0x10736000` — a
7 561 216-byte image, exactly what `ipsw::osos_from_drive` hands back — and the console then names
`0x10000000` as where it goes. Instrumented, RetailOS's first act at `pc 0x10000220` is to program
the PP's remap windows at `0xf000f000`, one of which is `0x00000000..0x01ffffff -> 0x10000000`;
after that it runs from low addresses (`0x000a601c`, `0x00289e98`, …), which are that window.

`emu::build` did neither. It **pushed** the image as a region at `0x10000000` and mirrored it as a
second live region at 0, and entered the CPU at 0. Region lookup is first-match and `map_hardware`
had already registered 64 MB of `sdram` at `0x10000000`, so the `osos` region was read by nothing:
SDRAM was 64 MB of zeros with a copy of the OS filed behind it. That held up exactly as long as it
took RetailOS to program its own window — `Memory::translate` runs ahead of the region list — and
then every low address resolved into the zeros.

**Fixed by doing what the bootloader is measured doing**: write the bytes into SDRAM, and start the
CPU at `load_at + entry`. One storage, and the remap points at it. Same drive, same ROM, same seed:

| `ipod-emulator --headless=200000000` | before | after |
|---|---|---|
| ending | **`Lost(33554432)` @ 8 388 485** | `BudgetExhausted` @ 200 149 075 |
| ata commands | **0** | **290** — 20 READ DMA, 264 WRITE DMA, IDENTIFY, 5 SET FEATURES |
| code buckets | 2 097 122 *(all of them the slide)* | 20 196 |
| backlight | 16 / 32, never moved | 17 / 32, **1 step up** |
| wheel | 0 `0x052a` commands | **1** |

The 264 `WRITE DMA` is RetailOS bootstrapping its own volume, which is what it does on a drive it
has not seen before.

### Why no recipe in `ipod-boot` could have caught it

**The remap is modelled only on the cold map.** `map_hardware` sets `mmap_base = Some(0xf000f000)`
inside `if cold_boot`; `trace` passes `--cold-boot` on the cold recipe alone, and `emu::build`
passes `cfg.boot.is_os()`, which is true for the window's OS boot. So a warm run's writes to
`0xf000f000` land in the `cache` region as plain memory and nothing happens.

Counted, with a control, by printing one line per `rebuild_mmap_aliases` — all four on the same
`PRISTINE` drive at `--clock=5`:

| run | map | MMAP window rebuilds |
|---|---|---|
| `ipod-emulator --headless=200000000` (the window's high-level boot) | cold | **408** |
| `ipod-boot retail`, 600 M | cold | **56** |
| `ipod-boot warm --flash=<synthetic>`, 600 M | warm | **0** |
| `ipod-boot rockbox`, 200 M — **the control**, because Rockbox demonstrably runs on this path | warm | **0** |

The Rockbox row is what makes the two zeros mean something: a zero from a run where nothing
executed would prove nothing at all.

So the ✅ in this file's *RetailOS, high-level* row is a true statement about `ipod-boot warm`, and
`ipod-boot warm` is a different machine from the one the window builds — **different memory map**
(warm has SDRAM's storage at 0 and no remap at all), **different entry** (`0x10000000` either way,
but the window's used to be 0), **different source for the OS** (`--osos=` reads a file;
`emu::build` reads the drive's own firmware partition), and one extra mirror at `0x04000000` that
the window does not make. The row was never a measurement of the window's boot, and no flag on the
command line would have made it one.

**The corrected cell**, measured today:

| **RetailOS**, high-level — *the window's* (`emu::build`) | real 5G dump | synthetic 5G | synthetic 5.5G |
|---|---|---|---|
| before 2026-08-25 | n/a — a real dump cold-boots | ❌ `Lost(0x02000000)`, 0 ATA | ❌ `Lost(0x02000000)`, 0 ATA |
| after | n/a | ✅ | ✅ |

### And the instrument that hid the ROM under test

`ipod-boot warm --flash=synth.bin` **ignored the flag**. Every recipe appends the caller's
passthrough after its own flags, and `trace` reads a single-valued flag with
`args.iter().find_map(|a| a.strip_prefix("--flash="))` — the *first* match. So the composed argv
carried two `--flash=` and the recipe's won:

```
trace 600000000 --osos=… --boot-osos --osos-at=0x04000000 --sysinfo \
      --flash=…/retail_5g_MA146_… --disk=… --bcm --pmu --flash=…/repro.bin
  sysinfo at 0x40015898, sdram_size 0x4000000, from MA146      <- the RETAIL dump
```

The run named the retail dump in its own output while the operator watched, so this is the ninth
instrument in this project to report something it could not have observed. Fixed twice over: the
caller's `--flash=`/`--disk=` is now an *input* (`resolve` reads it ahead of `FLASH=`, and `--print`
says `command line`), and `passthrough_wins` drops the recipe's copy of any single-valued `--key=`
the caller spelled. `--osos-at=` is exempt by name — `trace` collects it with `filter_map` and
`ipod-boot warm` writes one on purpose — and a test reads `trace.rs` to keep that list honest.

**`sdram_size 0x4000000` is not a fact about the ROM.** It is `trace`'s `--sysinfo` default
(`trace.rs`, `spec.and_then(parse_addr).unwrap_or(0x0400_0000)`) and the same constant
`emu::build` writes as `MEASURED_SDRAM_WORD` on both the synthetic and the real path. A synthesised
ROM does not tell RetailOS it has 32 MB — it says nothing about memory at all — and the 32 MB in
`0x02000000` is the size of the remap window, not of anybody's SDRAM. This paragraph exists because
that was the first hypothesis and it was wrong.

## Superseded: the whole matrix, measured 2026-08-19

Every cell is a run. `—` is not "untested", it is "cannot, and the row below says why".

| | real 5G dump | synthetic 5G | synthetic 5.5G |
|---|---|---|---|
| **RetailOS**, cold | boots — 599 ATA, full framebuffer | — | — |
| **RetailOS**, high-level | boots — 597 M instructions | **boots** — 7 ATA | **boots** — 7 ATA |
| **Rockbox**, warm | menu — 72 ATA | 90 ATA | 90 ATA |
| **Rockbox**, cold | **menu — 10 304 ATA, 74 057 lit pixels** | **0 ATA, 0 pixels** | **0 ATA, 0 pixels** |
| **ipodloader2** → Linux | 3 196 ATA, no unmapped | **3 196** | **3 196** |
| `diag` | **draws — 70 669 lit pixels** | — | — |
| `disk` | faults after 128 K instructions (USB unmodelled) | — | — |
| `logo`, `vmcs` | not bootable images — payloads, refused by `is_bootable` | — | — |

**Two rows are worth reading twice.**

`ipodloader2` is **identical on all three** — 3 196 ATA commands and no unmapped accesses — so it
consults nothing the synthesiser writes, and the Linux failure downstream of it is not a
generation mismatch. That was a live hypothesis and this kills it.

**Cold Rockbox is 10 304 against 0.** Anything that has to run Apple's bootloader needs a real dump,
because a synthetic ROM has none — see below. The synthetic rows exist only through the high-level
boot, which enters an operating system directly.

## Where the four NOR modes actually ship — and why a synthetic ROM still cannot run them

**Asked and answered 2026-08-19, because the obvious guess is that `diag` and friends come down in
the IPSW and could therefore be given to any ROM.** They do come down in the IPSW. It does not help.

**The IPSW's firmware directory holds three images and none of them is a mode.** Parsed out of
`iPod_20.1.3`'s `Firmware-20.6.3` at `0x4200`:

| image | devOffset | length |
|---|---|---|
| `osos` | `0x00004400` | 7 559 680 |
| `rsrc` | `0x0073a000` | 5 242 880 |
| `aupd` | `0x00c3a200` | 1 074 176 |

The `diag`, `disk`, `logo` and `scan` strings that appear elsewhere in the file are constants inside
code — including an `hslf` at `0x71c754` whose neighbours disassemble to `push {r2-r6,lr}` and
`ldr r0,[r1,#0x10]`, i.e. the routine that *searches* for a flash directory rather than one.

**But `aupd` is 1 074 176 bytes, which is a 1 MB flash image plus a header, and it is encrypted.**
Entropy is **8.00 bits/byte** over the whole image and 7.99 in every 16 KB window sampled, with the
byte histogram flat (4 266 `0xff`, 4 309 `0x00`, against 4 196 expected for uniform). So the flash
content almost certainly *does* ship in the IPSW — inside the updater, which is Apple's own code and
would decrypt it itself. That is a better answer than "the modes are only in the NOR", and it is the
answer to the question people will actually ask.

**It still cannot be used, and the reason is not the payloads.** A synthesised ROM is an identity
card, not a ROM:

| | non-zero bytes |
|---|---|
| real 5G dump | **908 246** — 86.6 % of a megabyte: a bootloader and four self-contained payloads |
| synthetic 5G | **101** — 0.0 % |

Word 0 is a branch in both (`0xea001ffe`, to `0x8000`) because `inspect::flash` checks for one, but
on the synthetic image there is nothing at `0x8000` to branch to. Booted cold it fetches straight
off the end of the chip — 188 reads at `0x00100000`, one megabyte in, from 43 consecutive PCs. **It
has no bootloader, so it cannot run `aupd`, so it cannot be given the modes by the updater either.**

Which is why the synthetic rows in this file are all high-level boots: they enter an operating
system directly and skip the bootloader that does not exist. **Diagnostics, disk mode, the scanner
and the boot logo require a real dump** — not because the bytes are unobtainable, but because the
program that would install them needs a ROM to run in.

### Could Apple's own updater install them? No, and the refusal is circular

The obvious follow-up: `aupd` is the program that writes the NOR, so **enter it directly** and let
Apple's code decrypt its own payload onto a synthetic chip. `--osos-from-disk=TAG` exists now for
exactly this — the firmware directory's images are all firmware images and nothing about the entry
path was `osos`-specific.

It refuses, and the refusal is the confirmation:

```
upd-armed.img: no ARM vector table within 0x4000 of `aupd` at 0xc3a200.
An image entered at its base opens with two branch instructions and this one does not,
so either it is not a 5G/5.5G OS image or it is not stored in the clear.
```

**It is not stored in the clear** — which is what 8.00 bits/byte already said, now confirmed by a
reader that was not looking for encryption. So:

1. the four modes ship in the IPSW, inside `aupd`;
2. `aupd` is encrypted;
3. the thing that decrypts it is Apple's bootloader;
4. Apple's bootloader is in the NOR we are trying to synthesise.

**The requirement is circular by construction**, and no amount of work on this side breaks it. A
synthetic ROM can boot an operating system, and that is the whole of what it can do. `diag`, `disk`,
`scan` and `logo` need a dump from an iPod somebody owns.

## What a synthesised ROM cannot do

**The four NOR modes are not in it, and cannot be.**

```
synthetic       flsh entries: 0
real 5G dump    flsh entries: 4
```

`diag`, `disk`, `scan` and `logo` are self-contained payloads Apple ships inside the flash, indexed
by the `flsh` directory at `0xffe00`. Synthesising the identity block is one thing; synthesising
Apple's diagnostics is not the same kind of task, and this project does not have those images except
inside a dump somebody already owns.

So: **diagnostics mode, disk mode, the disk scanner and the boot logo image require a real dump.**
A synthesised ROM boots an operating system and nothing else. That is worth saying out loud next to
any claim that "all the NOR modes draw", because it is true of a real dump and false of a generated
one.

## What this does *not* establish

**The 5.5G's `HwVr` is still unsourced.** `0x000B0010` came from a code comment. The 5.5G boots with
it, and boots byte-identically with the 5G's `0x000B0005` forced in its place — same instruction
count, same ATA count, same unmapped count. So the boot says nothing about which value is right, and
anyone reading this table should not take "the 5.5G boots" as "the constant is confirmed".

Every other constant in the handoff is measured: `len = 0xf8`, `BoardHwName = "iPod M25"`, the 5G's
Gestalt, the model number, and the `SysCfg` copied in after the struct.

## All three revisions

There are **three** firmware revisions of this iPod, not two, and the middle one had been skipped:

| updater family | revision | RetailOS, high-level boot |
|---|---|---|
| **13** | 5G Initial (Oct 2005) | **boots** — 597 M instructions, 7 ATA |
| **20** | 5G Rev A | **boots** — 597 M instructions, 7 ATA |
| **25** | 5.5G, Late 2006 | **boots** — 597 M instructions, 7 ATA |

13 and 20 are both 5G and share Gestalt `0x000B0005`, so both take the synthetic `MA146`. Our own
reference drive reports `updaterFamily: 13`, so it is an Initial.

## `HwVr` for the 5.5G — upgraded from "a comment" to "published, uncited"

Searched for deliberately. What exists:

- **theapplewiki's *iPod with video* page** assigns `0x000B0010` to Rev B / Late 2006, update family
  25 — which matches `iPod_25.1.3.ipsw`'s own manifest. Published, and **uncited**.
- **Apple's own binary contains the constant.** `ipod-usb`'s reverse engineering of
  `CIpodDevice::GetDeviceType()` records a switch with `0x000B0005 → type 17` and
  `0x000B0010 → type 23`, both inside the iPod-with-Video family. So the value is Apple-recognised
  rather than invented — though that does not by itself say which revision it belongs to.
- **The iPodLinux wiki lists both values as "5G"** without splitting them by revision, and is the
  likeliest origin of the comment this project carried.
- **No measurement exists anywhere.** No retail 5.5G NOR `HwVr`, no 5.5G `SysInfo`. Rockbox and
  `ipodloader2` both compare only the high halfword (`0xB`), so neither can distinguish, and no
  aligned occurrence of any of these words appears in any Apple firmware image — consistent with
  RetailOS switching on the high half only.

**Keeping `0x000B0010` is defensible. Calling it measured is not.**

> *Strengthened 2026-08-19.* A third occurrence, and the first in code that runs **on the iPod**:
> Apple's `diag` image dispatches on the hardware version with exactly three cases —
> `0x000B0005`, `0x000B0010`, `0x000B0011` — each with its own handler, at prototype `diag`
> `0x10003c28`. See [research/07](07-the-flash-images.md). The value is Apple's, not a wiki's.
> Which revision it names is still unmeasured, because no retail 5.5G NOR has been read.

## Two blockers found while testing the bootloaders

**`install-os` refuses every drive `make-disk` builds**, and for a different image on each revision:

```
5G   (20.1.3)   `aupd`: directory says 0x0b19db1c, bytes sum to 0x08299587
5.5G (25.1.3)   `osos`: directory says 0x2c7c48f3, bytes sum to 0x2c7f4045
```

That check is deliberate — it reproduces the existing checksums before writing new ones, so a wrong
idea of the layout fails on an unmodified file rather than producing an image the bootloader rejects
seventy ATA commands in. It is working; what it is telling us is that we have two layout problems.

Both are now settled, and by a **second independent method**: which offset reproduces each image's
recorded checksum. That agrees with the vector-table method and is stronger, because a checksum over
seven megabytes cannot match by coincidence.

| bundle | `osos` / `rsrc` | `aupd` |
|---|---|---|
| `iPod_13.1.3` (5G Initial) | reproduce at **`+0x200`** | matches at **no** offset |
| `iPod_20.1.3` (5G Rev A) | reproduce at **`+0x200`** | matches at **no** offset |
| `iPod_25.1.3` (5.5G) | reproduce at **`+0x800`** | matches at **no** offset |

1. **The header is per-bundle**, and `FW_SECTOR`'s fixed 512 was right for the 5G and wrong for the
   5.5G. It is now *discovered* — `osos`'s own checksum is the oracle — so the tool no longer has to
   be told, and a bundle with a header nobody has seen resolves itself.
2. **`aupd`'s checksum reproduces at no offset in any bundle.** My earlier note here said its extent
   ran past the end of the firmware file; **that was wrong arithmetic** — the file is `0xD40800`,
   not `0xD40000`, and `aupd` fits inside it. The real finding is that whatever Apple sums for the
   updater image, it is not the bytes at `devOffset`. It is systematic across all three bundles, so
   it is a property of the format rather than damage, and `install-os` now exempts it by name and
   says why. Failing on it meant refusing every drive `make-disk` builds.

**And "no room" is not a defect — it is a faithful drive with an armed updater.** `install-os`
refuses to install a bootloader on a drive `make-disk` builds, because Apple's three images fill the
partition. The obvious fix was to widen partition 0 to `DATA_LBA - FIRMWARE_LBA`, and that was
**wrong and has been reverted**: measured on the reference drive, a real iPod's firmware partition is
**27 140 sectors — Apple's firmware to the byte, with no slack at all**. Widening it made our drives
differ from real hardware to work around something that is not a fault. The layout test said so, and
I had been about to update the test to match the code.

What a real post-update iPod has instead is **no `aupd`**: the reference drive carries only `osos`
and `rsrc`, and the megabyte the updater occupies is exactly the room a bootloader goes into. So the
drive to install onto is one whose updater has been consumed — which is what the reference drive is,
and `install-os` works on it first time.

**The Rockbox bootloader test was run the wrong way.** `bootloader-ipodvideo.ipod` was warm-booted
through `--osos=`, and burned its whole 200 M budget with **zero** ATA commands on all three ROMs.
That is not evidence about the bootloader: it expects to be *installed in the firmware partition and
entered by Apple's bootloader*, which is a cold boot. The run says nothing until it is redone that
way — and redoing it needs `install-os`, which is blocked above.


---

## Addendum — the 5.5G does not boot, and it is the FIRMWARE, not the ROM (2026-08-26)

Measured with `ipod-emulator --headless=400000000`, one data directory per arm, every path pinned.
**The control is the identity line**: each synthesised arm prints its own serial, so an arm that did
not change is visible rather than assumed.

| ROM | drive | ata | lit pixels | |
|---|---|---|---|---|
| Apple's 5G dump | built from `iPod_20.1.3` | **617** | **75 267** | boots |
| synthesised **5G** (`MA146`) | built from `iPod_20.1.3` | **264** | **75 267** | boots |
| synthesised **5.5G** (`A444`) | built from `iPod_20.1.3` | **280** | **75 267** | boots |
| synthesised **5G** (`MA146`) | built from `iPod_25.1.3` | **22** | **2 612** | **stops** |
| synthesised **5.5G** (`A444`) | built from `iPod_25.1.3` | **22** | **2 612** | **stops** |

75 267 is Addendum 10 §8's own fingerprint, to the pixel. **The ROM is not the variable and the
generation of the ROM is not the variable.** Every ROM boots a 5G drive; no ROM boots a 5.5G one.
What does not boot is **RetailOS 25.1.3**.

### The prime suspect is dead, with a control

`nor.rs`'s `Spec::hw_vr` exists for exactly this — *"the 5.5G's `0x000B0010` is the one value in this
whole system that came from a comment rather than from hardware, and the 5.5G does not boot"* — and
line 436 above says the same. `ipod-boot make-nor --hwvr` reaches it and is absent from the usage
text, which is why it had never been turned.

Turned, over the failing drive, nothing else moving:

```
HwVr 0x000B0010   22 ata   2 612 lit   19 754 code buckets
HwVr 0x000B0011   22 ata   2 612 lit   19 754 code buckets
```

Identical, including the bucket count — which is §6's signature of a change that did not take
effect, so the control was run: `cmp -l` reports the two ROMs differ in **exactly one byte**, at
`0x405D`, `0x10` -> `0x11`, which is the `HwVr` record's value word. The arms were real. **`HwVr` is
not the cause.**

### And it is not the header, which this file has blamed before

`image_from_drive` finds the header rather than assuming it, and both drives come back correct:

```
5G   7 559 680 bytes  addr=0x10000000 entry=0x0  first8=[7a 00 00 ea 67 00 00 ea]
5.5G 7 561 216 bytes  addr=0x10000000 entry=0x0  first8=[7a 00 00 ea 67 00 00 ea]
```

Same vector table at the base of both. The 0x200-versus-0x800 defect recorded in `ipsw.rs` is fixed
and is not this.

### Also ruled out

- **`aupd`.** Removing the directory entry entirely leaves the boot byte-for-byte identical.
- **A regression from the window rebuild.** `ipod-boot retail`, pinned identically, at `e127849`
  (before the first Slint commit) and at HEAD 53 commits later: **599 999 952 instructions, second
  core 315 681 at pc `0x000dee20`, 70 ata** in both. The machine did not move.

### What a mismatch is supposed to look like, and does not

`make-disk` warns that a family mismatch *"does not fail loudly: RetailOS boots and then asks you to
restore it from iTunes, after roughly 70 ATA commands instead of 600."* A synthesised **5.5G** ROM on
a **5G** drive is that mismatch, and it reached **280 ata and a full panel** instead. So either the
synthesised NOR does not carry what RetailOS tests the family against, or the test is somewhere this
machine does not reach. That is a second open question and it is not the same as this one.

**Next:** trace `iPod_25.1.3` against `iPod_20.1.3` from the handoff and find the first divergence.
Bisection by file-swapping is finished — it has taken this as far as it goes.

---

## Addendum, same day: RetailOS 25.1.3 is not broken, and half of what is above is now wrong

Written after the section above, from the handoff-comparison it said to do next. Two of its
conclusions survive, two do not, and the one that matters most reverses.

### The instrument, corrected first

Everything above was measured through `ipod-boot retail`, which runs **Apple's shipping 5G
bootloader out of a real NOR dump**. A synthesised ROM has no bootloader in it — `ipod-boot facts`
reports `Images: logo` and nothing else — so `retail` on a synthesised ROM is not a boot at all.
Run anyway, three different synthesised ROM/drive pairs give:

```
second core: 238000 instructions, pc 0xffcc2fe4, awake
ata commands: 0
```

The *same numbers for all three*, which is §6's signature of an arm that did not vary. The
synthesised paths above were measuring the harness. They are re-run here through
`ipod-emulator --headless`, which is what the window uses and what a synthesised ROM needs: one
`IPOD_EMULATOR_DATA` per arm, its own clone of the drive, `work_on_copy = false`, and the printed
`identity:` line as the control that the arms differ.

### The matrix, on the harness that fits

```
ROM                          drive        ata   lit      buckets  stopped
Apple's retail 5G dump       20.1.3       617   75 267    31 480  budget
Apple's retail 5G dump       25.1.3        70   71 695     1 812  budget
synthesised 5G   (MA146)     25.1.3       290    2 612    20 196  budget
synthesised 5.5G (A444)      25.1.3       290    2 612    20 196  budget
synthesised 5.5G (A444)      20.1.3       547   75 267    29 964  idle
```

**Read the second row.** 71 695 lit pixels of 76 800, four blits ending in a `320x176` band at
`(0,32)-(319,207)`, and 70 ATA commands after 58 DMA transfers have landed all 7 561 216 bytes of
the image. That is `make-disk`'s own documented mismatch screen — *"RetailOS boots and then asks
you to restore it from iTunes, after roughly 70 ATA commands instead of 600."*

Where it goes afterwards is worth naming, because "stuck" and "waiting" look identical from
outside. The profile is 100 % in IRAM, and the hot address disassembles to a delay loop:

```
40009cec  mov r1, r3            ; a duration
40009cf0  mov r0, r4            ; r4 = [0x60005010], captured on entry
40009cf4  bl  0x4000e744        ; (now - start) >= duration ?
40009cf8  cmp r0, #0
40009cfc  beq 0x40009cec
```

`0x60005010` is the PP502x microsecond timer. It drew the screen and is waiting. **RetailOS 25.1.3
loads, executes, drives the display and reaches its own idle. It is not broken, and a drive built
from `iPod_25.1.3` is a working drive.** The section above concluded *"what does not boot is
RetailOS 25.1.3"* and that is withdrawn.

### `HwVr` is dead twice over now

The bisect above varied the *synthesised* ROM's Gestalt. This varies **Apple's**, which is the
stronger arm — same real bootloader, same everything, one byte:

```
$ printf '\x10' | dd of=<copy of the retail dump> bs=1 seek=$((0x405C)) count=1 conv=notrunc
$ cmp -l <Apple's dump> <the copy>
16477 5 20            # one byte, offset 0x405c, 0x05 -> 0x10
```

`0x4054` is the `rVwH` record — the tag stored byte-reversed, value word at `+0x08`. Exactly one of
the three `rVwH` occurrences in the dump is the SysCfg record; the other two are at `0xf160` and
`0xc90c8` and were left alone. Booted:

```
Apple's ROM, HwVr 0x000B0005 (untouched)  x 25.1.3   70 ata  71 695 lit   1 812 buckets
Apple's ROM, HwVr 0x000B0010 (patched)    x 25.1.3   70 ata  71 695 lit   1 812 buckets
Apple's ROM, HwVr 0x000B0010 (patched)    x 20.1.3  617 ata  75 267 lit  31 480 buckets
```

The Gestalt moved and nothing else did. **Whatever RetailOS pairs a drive against, it is not
`HwVr`** — so the doc on `Spec::hw_vr` that put *"came from a comment"* and *"and the 5.5G does not
boot"* in one sentence was asserting a link nothing ever supported. Corrected in place.

### And it is not `rsrc` either, which is a retraction of this session's other fix

`mark_aupd_applied` was changed earlier today to rewrite `rsrc`'s load address and entry as well as
marking the updater applied, and `KNOWN-BUGS.md` recorded that as taking a boot from 22 ATA and
2 612 lit pixels to 70 ATA and 71 695. Three drives built here — Apple's shipped values, the
forced values, and the armed updater — differ in exactly the three bytes the diff names and in
nothing else:

```
abs 49215   0x10 -> 0x00     rsrc addr, high byte
abs 49217   0x00 -> 0x06     rsrc entry
abs 49240   0x01 -> 0x00     aupd applied
```

```
ROM                 rsrc                        ata   lit      buckets
synthesised 5.5G    forced 0x10000000 / 0       290   2 612    20 196
synthesised 5.5G    Apple's 0x00000000 / 0x600  290   2 612    20 196
Apple's retail 5G   forced 0x10000000 / 0        70   71 695    1 812
Apple's retail 5G   Apple's 0x00000000 / 0x600   70   71 695    1 812
```

`70 / 71 695` is Apple's ROM either way; `2 612` is a synthesised ROM either way. **The original
comparison moved the ROM as well as the drive** and read the difference as the drive's — §6's fifth
shape, in a session that had already quoted §6 twice. The write stays, because a built drive
carrying a real drive's values is still the drive to build; the boot-effect claim does not.

### What is actually left, stated narrowly

Only the **high-level boot** — the path a synthesised ROM takes, where the ROM's *effects* are
produced instead of its instructions being run. On it, 25.1.3 stops after 20 `READ DMA` where
20.1.3 does 277, and `2 612` lit pixels is the synthesised bootloader's own logo and nothing after.

The strongest single clue is that the two synthesised arms are **identical to the code bucket**:
`MA146` and `A444`, different serials, different GUIDs, different Gestalts, same 290 / 2 612 /
20 196. On this path the OS's fate does not depend on the NOR's contents at all, so the divergence
is machine state at OS entry, not identity — which is the same conclusion the ROM arms reach from
the other side.

Ruled out inside that, each with its control:

- **The warm-path tag scaffolding.** `emu.rs` copies `trace`'s `install_sysinfo` records —
  `Flsh` / `Sdrm` / `Frwr` / `Iram` at `+0x60` / `+0x74` / `+0x88` / `+0x9c` — onto this path.
  Apple's real cold boot leaves all four **zero** (dumped at `0x40015898`), and `Frwr` at `+0x88`
  additionally overwrites the measured `"1.00    "`. Removed: the 25.1.3 arm is unchanged to the
  instruction, 290 / 2 612 / 20 196; the 20.1.3 arm shifts by 26 786 instructions and still idles
  at 547 / 75 267, so the edit was live and the control holds. Not it.
- **`0xea000078`.** The failing arm's only unmapped page, 144 reads, and it is a **known-benign
  artifact** — research/07 §"`0xea000078` was never a real address", research/02 §372, and
  research/10's own healthy 600 M baseline where it is the *only* unmapped page. It is RetailOS
  dereferencing a null object and reading its own reset vector. Nearly filed as the cause; the
  grep that NEXT.md R3 asks for is what stopped it.

`trace` cannot currently reproduce this arm: `--osos-from-disk` uses `Machine::map_osos`, which
pushes `osos` as a **region** at `0x10000000`, and the window's HLE deliberately does not — its own
comment records that arrangement producing `Lost(0x02000000)` after 8 388 485 instructions, which
is the failure the operator reported at the start of this session. So the CLI instrument and the
shipped machine disagree about how a high-level boot is arranged, and the profile, register-file
and watch instruments are all on the side that does not match. **That is the next thing to fix**,
and it is a prerequisite for the trace this section keeps saying to run.

Meanwhile `--headless` names its unmapped pages now, with PCs, the way the window's readout and
`trace` always have. A total with no address is a number nobody can act on, and the one run that
works over SSH was the one that could not say what the firmware reached for.

### Two more things, and one of them changes the shape of the question

**It is not running out of budget. It goes to sleep.** Fresh drive, five times the budget:

```
25.1.3   Idle after 494 749 605   290 ata (20 READ DMA, 264 WRITE DMA)   2 612 lit
20.1.3   Idle after 302 191 833   547 ata (277 READ DMA, 264 WRITE DMA)  75 267 lit
```

Both reach `Idle`, which is `emu::Quiet` — a trailing window that is 95 % halted. So RetailOS 25.1.3
is not spinning on anything and is not being cut off: it initialises, **writes the same 264 sectors
the working boot writes** — it bootstraps the volume, so it is doing real work and the drive is not
being rejected — reads 20 sectors where the other reads 277, and then halts waiting for an interrupt
that never arrives. The 2 612 lit pixels are the synthesised bootloader's logo, never overwritten.

That is a different question from the one this file has been asking. Not *"why does it die"* —
**"which interrupt does 25.1.3 wait for that 20.1.3 does not?"**

**And the 22-versus-290 discrepancy between this file's two matrices is the drive, not the run.**
`work_on_copy = false` means a second boot in the same data directory starts on the drive the first
boot wrote. First boot: 290 ata, 264 of them writes. Second boot on that same drive: 22 ata, 4
writes — it finds the volume already bootstrapped. The earlier matrix was measuring second boots.
Every number in this addendum is a fresh clone per arm, and an arm re-run for a second measurement
gets a fresh clone first.

Worth stating plainly because it is the same trap in a new place: an emulator arm is not fresh
because the *settings* are fresh. The drive is state, and it is 8 GiB of it.

### The harness split, which is the finding under all of the others

Same ROM, same drive, both pinned, both built by `ipod-boot make-disk` from `iPod_20.1.3`:

```
ipod-emulator --headless   617 ata   75 267 lit   31 480 buckets   full panel
ipod-boot retail            70 ata    2 blits                      Apple logo, then nothing
```

`retail` does not get further with more budget. At `BUDGET=2000000000`, and again through
`from-idle`'s 1.6 G snapshot — the recipe that exists *because* a run needs 1.6 G to reach the menu
— it is still 70 ata, still the two blits, still `usec 21333333` of a machine sitting at the Apple
logo. The `70` that `ipsw.rs` records as *"where `ipod-boot retail` also stops and is a different,
older defect"* is real, and it is **not** on the drive: the same drive boots to the language picker
under the window's own machine.

So the CLI recipe and the shipped emulator disagree about the same two files, and the instruments
are all on the losing side. `--profile`, `--disasm`, `--watch`, `--break`, the register file at a
fault and the wheel script are `trace` flags; `--headless` has none of them. Everything this file
concluded from `ipod-boot retail` was measured on a machine that stops at the Apple logo.

**The wheel bears this out.** Driven on `retail`, anchored in simulated time the way
`parse_wheel_script`'s own doc insists:

```
--wheel='@8s:touch,@8500ms:rotate=+6,@10s:rotate=+6,@11s:press=select,@13s:release'
  wheel script: 16 steps        script: 16 of 16 steps fired
  clickwheel: 19 frames posted (15 dropped unread), 3 word reads of DATA
```

16 of 16 is the control §6 asks for, so the injector works and the anchor is right. But 15 frames
dropped unread and 3 reads of DATA is a firmware that is not listening — because it is at the Apple
logo, not at a menu. **There is currently no way to drive the wheel on a configuration that boots.**

That is the next thing to fix, ahead of everything else in this file. `Machine::map_osos` pushing
`osos` as a region at `0x10000000` is one half of it and is named above; the `70` is the other half
and is not yet explained. Until the two harnesses agree, a trace of 25.1.3 against 20.1.3 would be
a trace of the wrong machine.
