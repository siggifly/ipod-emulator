# `ipod-boot` — cold-booting Apple's firmware

> **There are two front ends to these recipes and they compose the same argv.** The `.sh` files here
> are the original spelling and are what every number in `research/` was measured through; they are
> unchanged and stay that way. The **`ipod-boot` binary** (`tools/eapp-loader/src/bin/ipod-boot.rs`,
> built by `cargo build --release` in that crate) is the same recipes as a program, which is what
> makes them runnable on Windows:
>
> ```
> ipod-boot retail | cold | warm | flsh | rockbox | flash-update | from-idle   [trace flags…]
> ipod-boot <recipe> --print          # show the command line it composes, run nothing
> ipod-boot make-disk IPSW OUT.img    # build a bootable drive from an iPod software-update bundle
> ```
>
> Same environment variables, same defaults, same flags in the same order — and a test
> (`recipe_flags_match_the_shell_scripts`) reads these `.sh` files off disk and asserts it, so the
> two cannot drift apart quietly. Measured on 2026-08-14: `BUDGET=4000000000 ipod-boot retail
> --clock=5 --stop-when-idle=400000000` produces `Idle after 1562789429`, 38 220 buckets, 770 ata
> commands and 4 unmapped reads — the baseline, through either front end.
>
> Two deliberate differences, both documented in that file's header: `TRACE` defaults to the `trace`
> binary **beside** `ipod-boot` rather than to one machine's `~/dev/.cargo-target`, and the disk
> clone tries `cp -c`, then `cp --reflink=auto`, then a byte copy. macOS still succeeds on the first
> rung, so nothing measured changed; Linux stopped paying a full 8 GB copy per run.
>
> **`make-disk` is the answer to "I do not have an 8 GB disk image."** An IPSW is about 14 MB and
> contains exactly the firmware partition an iPod needs — `Firmware-20.6.3` is 13 895 680 bytes,
> which is 27 140 sectors, which is the size of MBR partition 0 with nothing left over. RetailOS
> builds the rest of the volume itself on first boot. See the main README's "Reproducing it" for the
> measured result, including the one byte (`aupd`'s `+0x08`) that decides whether the first boot
> runs the OS or Apple's flash updater.

`cold-boot.sh` runs the iPod's **own NOR flash bootloader** under the emulator, entering at `0x0`
where the CPU fetches out of reset. `warm-boot.sh` is its counterpart — it skips the bootloader,
enters RetailOS at `0x10000000`, and installs the handoff state by hand (bypass #4).

It matters because everything the bootloader leaves behind — the `sysinfo_t` block, the Gestalt ID at
`sysinfo+0x84`, the SDRAM bank sizes — was previously reconstructed by hand from its consumers. This
runs the code that writes it.

What it gets through, in order: SDRAM bring-up · the Broadcom BCM2722 video co-processor (uploading
its `vmcs` firmware straight out of flash) · the PCF50605 PMU over I²C · then the ATA disk, reading
the partition table and the firmware partition.

## Inputs

| Variable | Default | Notes |
|---|---|---|
| `TRACE` | `~/dev/.cargo-target/release/trace` | build with `cargo build --release --bin trace` |
| `FLASH` | `resources/internal_rom_000000-0FFFFF/…bin` | 1 MB NOR dump — **required**, and the thing that makes cold boot possible |
| `DISK` | `resources/derived/disk/ipod8g.img` | a disk image with a firmware partition. **Cold boot's only source for the OS image** |
| `OSOS` | `resources/derived/fw/OSOS_correct.bin` | `warm-boot.sh` only. A cold boot is not handed it — see below |
| `BUDGET` | `150000000` | instructions. The retail boot decision completes and fails by ~150M; 600M was four times more work than the question needed |

## `cold-boot.sh` is not handed an OS image (ledger #14, retired)

It used to pass `--osos=$OSOS`, pre-placing a known-good `OSOS_correct.bin` at `0x10000000` — the
same address, holding the same bytes, that the whole point of a cold boot is to have the ROM fetch
off the disk itself. The two runs are identical to the instruction (599 999 952 executed, 88 ATA
commands, 61 DMA transfers moving 7 598 080 bytes, the same console receipt), and what the ROM
lands in SDRAM is **byte-identical to `OSOS_correct.bin` across all 7 559 680 bytes**, compared at
the handover with `--stop-at=0x10000000:1 --save-region=sdram:FILE`.

Two things the removal bought, beyond principle:

- **40 unmapped writes stopped being swallowed.** `--osos=` also installed an `osos-low` mirror at
  address 0, and unlike NOR that mirror was writable — so the bootloader's JEDEC unlock sequence
  (`0xaaaa` / `0x5554` / `0x0`, from `0x40009f9c`..`0x40009fd8`) landed in it silently. Without the
  mirror those writes are reported, which is bypass #12's flash-update path asking to be modelled.
- **Symbols now come out of the machine.** RetailOS's 140 self-carried function labels are
  recovered from the SDRAM the ROM filled, bounded by the ATA DMA high-water mark — the same 140
  names at the same addresses the file produced.

`warm-boot.sh` still needs `--osos=`, and that is not a bypass: it enters at `0x10000000` with
nothing but zeroed SDRAM there. `--boot-osos` now says so instead of running zeros.

Large binaries live outside git on purpose: the flash and firmware images are Apple's, and the disk
images are multi-gigabyte. `resources/` is gitignored.

## Where the big files live

Everything the boot path needs is under `resources/derived/`. It is **not** scratch — a scratch
directory is a place whose contents may vanish between sessions, and two of these cannot be
regenerated from anything else in the tree.

| Path | What | Regenerable? |
|---|---|---|
| `derived/disk/ipod8g.img` | 8 GB disk image with the firmware partition the cold boot reads | rebuildable, slowly |
| `derived/fw/OSOS_correct.bin` | the 7.5 MB RetailOS image, correctly placed | yes, from `Firmware-20.6.3` |
| `derived/fw/Firmware-20.6.3` | Apple's firmware bundle, the source of the above | **no** — re-download |
| `derived/re/iram.bin` | the bootloader as it *executes*, scatter-loaded into IRAM | yes, via `--save-region` |
| `derived/re/flash.bin` | NOR as read out of the running machine | yes, via `--save-region` |
| `derived/recovery/osos_region_backup.bin` | **the bytes `ipod8g.img` had before we moved the `osos` body over them** | **no** |
| `derived/recovery/aupd_entry.bak` | the 40-byte directory entry removed for bypass #12 | **no** |

The two marked **no** are recovery artifacts for edits already made to the disk image. Losing them
means losing the ability to put `ipod8g.img` back. They live beside the image they undo, rather than
in a scratch directory that gets swept.

## Speed

A cold boot to the `osos` load decision runs in **~3.6 s**, down from ~55 s, with byte-identical
output. Three changes, all measured against a host profile rather than guessed at:

| Change | Why |
|---|---|
| default budget 600M -> 150M | the boot decision completes by ~150M; the rest was dead time |
| 32-bit fast path | `Bus::read32` defaulted to four `read8` calls, each walking the alias list, every device window and the region list. Every instruction fetch is one of those |
| hoisted accounting test | `count()` is a no-op unless `--devices`-style accounting is on, so the four calls per access are skipped rather than made-and-returned |

`--devices` still costs ~10.6 s, because accounting genuinely has to run per byte to keep its
numbers comparable with every measurement taken before this. **Leave it off unless you want the
access reports.**

## `--clock=N` is the flag that matters

The bootloader runs with **interrupts disabled and polls with timeouts**, so its progress is gated on
*simulated time*, not on instructions — it spends billions of cycles in delay loops. `N` is
interpreter instructions per simulated microsecond; 75 models the real ~75 MHz part, and **lowering
it makes time run faster than the code executing it**, collapsing those waits.

Measured, at an identical 600M-instruction budget:

| | ATA commands reached |
|---|---|
| `--clock=75` | 2 |
| `--clock=5` | **5**, including `READ SECTORS` of the firmware partition |

This is why a JIT is not the next thing to build: it would execute the delay loops faster, where the
clock knob skips them.

Timing-sensitive code can notice, so it is a knob and not a new default.

**RetailOS is paced the same way, and more severely.** Its boot advances one drive-configuration
step per ~10 simulated seconds, so a 400M-instruction run at `--clock=75` sees only 5.3 seconds of
iPod time and concludes, wrongly, that RetailOS never touches its disk. At `--clock=5`, the same
budget:

| | `--clock=75` | `--clock=5` |
|---|---|---|
| ATA commands | 70 | **82** |
| DMA transfers | 59 (all the bootloader's) | **60** — RetailOS reads LBA 0 |
| profile buckets executed | 1 844 | **5 094** |

Anything found this way is a lead until it is confirmed at `--clock=75`, and the budget needed to
confirm it grows 15×. See [research/11](../../research/11-rtxc-and-the-video-coprocessor.md) §42.

## Useful flags

| Flag | What |
|---|---|
| `--bcm` | model the video co-processor's host protocol rather than leaving it as passive memory |
| `--i2c-fill=0xff` | answer every I²C data read with all-ones. **Crude on purpose** — the PMU's ADC status never reports "conversion ready" otherwise, and this is an experiment, not a device model |
| `--rdval=ADDR=VALUE` | a word that always reads as VALUE. For status bits of hardware we do not model; **each one is a hypothesis**. The recipe no longer passes any: the two it carried were the external memory bus, now modelled ([research/12](../../research/12-bypass-ledger.md) #1, #2). Note that supplying *any* `--rdval` suppresses the built-in `COP_STATUS`/`PLL_STATUS` pair |
| `--profile` | sampled PC histogram |
| `--pagelog=BASE:SIZE[:GRAN]` | account for a range, bucketed by `GRAN` bytes (default 256). 256 answers "which register block", 4 answers "which register" |
| `--pmu` | model a real PCF50605 instead of `--i2c-fill`. Retires that bypass |
| `--stop-at=ADDR[:N]` | halt on the Nth arrival at ADDR. What makes `--history` describe a *first* fault rather than a later echo, and what splits a measurement at the bootloader→RetailOS handover (`--stop-at=0x10000000:1`) |
| `--symbols` | print the function names recovered from RetailOS's own labels — 140 of them, including every RTXC task entry. There is no symbol table; these are read out of the image |
| `--disasm=ADDR:COUNT` | disassemble the **running** machine. Necessary rather than convenient: much of RetailOS is scatter-loaded, and the file holds zeros where the code will be |
| `--history=N` | how many instructions of tail to print (also honoured on the cold-boot path, alongside the register file at the halt) |
| `--bcm-film=ADDR:W:H:EVERY:DIR` | `--bcm-dump`'s timelapse: sample the panel every `EVERY` instructions for the whole run and keep every frame that differs from the one before. Same address/size spelling (hex), same converter, exact 320x240 PNGs. `tools/ipod-film/film.sh` is the recipe that wraps it and assembles a video |

Two counters print on every boot run and are worth reading: `dma: every staged byte landed` (a
transfer log reports what was *staged*, so a destination no region answers would otherwise vanish
silently) and `ide irq: ... DELIVERED to a handler N times` ("interrupts are being taken" is a
statement about the timers, not about the disk).
