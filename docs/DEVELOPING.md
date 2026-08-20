# Developing

For working on the emulator, or driving it from a terminal. If you just want to run an iPod, the
[README](../README.md) is the whole of it.

## Building

```sh
cargo build --release
./target/release/ipod-emulator      # the window
```

Or without a clone. The packages have to be named — the workspace root is a virtual manifest and
`cargo install` will not guess:

```sh
cargo install --git https://github.com/siggifly/ipod-emulator ipod-gui eapp-loader eapp-inspect
```

`ipod-gui` is the crate; the binary it installs is `ipod-emulator`.

Nothing you build yourself is quarantined, so none of the unsigned-app friction in the README
applies.

## The crates

| | |
|---|---|
| `tools/arm7tdmi` | the CPU |
| `tools/eapp-loader` | the machine — memory map, peripherals, ATA, flash, the co-processor. Ships `ipod-boot`, `trace` and `ipod-film`. |
| `tools/ipod-gui` | the window. Its peripheral map comes from `eapp_loader::map_hardware` rather than a copy of it. |
| `tools/eapp-inspect` | reading Apple's binaries |
| `tools/ghidra` | the headless decompiler path |

## Running it from a terminal

The recipes use whatever the window was last pointed at, so once that is set they need no arguments:

```sh
ipod-boot retail             # the recipe every number in research/ is measured on
ipod-boot retail --print     # compose the argv, run nothing
```

`--print` also says where each path came from — environment, the window, or repository default —
because a recipe with an input you cannot see in its command line is one you cannot check. `FLASH=`,
`DISK=` and `BUDGET=` override.

| recipe | what it boots |
|---|---|
| `retail` | Apple's bootloader and the image it accepts, cold from the reset vector |
| `warm` | RetailOS entered directly, handoff faked |
| `rockbox` | Rockbox as a source-available oracle |
| `loader` (`ipodlinux`) | `ipodloader2` out of the drive's firmware partition |
| `flsh` | one of the NOR's own images — `IMG=diag\|disk` |
| `flash-update` | Apple's `aupd` updater, then the boot that proves it took |
| `from-idle` | restore a cached snapshot: 3 s instead of 110 s |

`tools/ipod-boot/README.md` has the full set.

**Pin your inputs when you compare two builds.** `FLASH=`, `DISK=`, `BUDGET=` and `WORKDISK=`
explicitly, in both arms. A comparison that lets the recipe resolve its own paths is comparing two
machines as well as two builds, and this project has drawn a wrong conclusion from exactly that.

## Building drives

```sh
ipod-boot make-disk iPod_20.1.3.ipsw disk.img     # a drive from an .ipsw
ipod-boot rockbox-install  [DISK.img [OUT.img]]   # Rockbox, downloaded and verified
ipod-boot install-linux    [SRC.img [OUT.img]]    # ipodloader2 + ZeroSlackr
ipod-boot install-os SRC.img OS.ipod OUT.img      # any other OS into the firmware partition
ipod-boot put-files DISK.img SRC_DIR [DEST]       # files onto the FAT32 volume
ipod-boot fat DISK.img                            # list the volume
```

Installs never write to the source image.

### Installing another operating system

`install-os` puts an image where Apple's bootloader will find it and cold-boots exactly as the
hardware does. `install-linux` does the whole iPodLinux install rather than half of it — the
distribution is **five** directories and a drive carrying only `boot/vmlinux` boots the kernel
completely and then has nothing to execute.

The boot menu it writes names only what is actually on the volume, so a drive with Apple's software
and Rockbox already on it comes out triple-boot.

**`ipodloader2` reads FAT32 partition type `0x0B` and no other.** Every drive image taken off real
hardware here is `0x0C` — the LBA form, equally legitimate — and `vfs.c` has no case for it.
`install-linux` refuses those drives rather than producing one that cannot boot. See
[research/17](../research/17-the-boot-matrix.md).

## Filming

```sh
RECIPE=rockbox ipod-film run --out=_film
```

Writes one PNG per distinct screen, a `frames.tsv` saying when each appeared and how long it held,
and a GIF if `ffmpeg` is present. `RECIPE` is passed straight through as an `ipod-boot` subcommand.

Filming iPodLinux is expensive: the kernel's own output starts around 8 G instructions and its
userland around 20 G.

## Instruments

`trace` carries the measurement flags. The ones that have earned their keep:

| | |
|---|---|
| `--norlog` | which flash pages the firmware read, by 256-byte page |
| `--enterlog=PC[,PC…]` | r0–r3 at each arrival — this is how the kernel's `printk` becomes a dmesg |
| `--readlog=ADDR` + `--readlog-dump=` | the PC, value and instruction count of every read of an address |
| `--regs-at=ADDR[:N]` | registers at a PC |
| `--dump=ADDR:LEN` | memory |
| `--bcm-ppm=` / `--bcm-film=` | the panel |
| `--cop-trace` | the second core's park/wake ledger |
| `--stop-when-idle=N` | end the run once nothing new is reached |
| `--restore=FILE` | resume a saved machine — and say so if nothing is mapped at its program counter |

`IPOD_LAYOUT=1` makes the **window** print the measurement its size constants are derived from:

```
layout: window 1100x830 @2 ppp · device area 1100x714 · chrome 116 px · scale x2
```

**[NEXT.md](../NEXT.md) lists every instrument with a note on how each one lies.** That section is
not decoration: eight of them have reported an absence they could not have observed. Before
believing a zero, run the control that makes the instrument produce a non-zero.

### Anchor an injected wheel script in simulated time

`--wheel=@24s:touch` and not `--wheel=@2200M:touch`. A machine spends most of a budget **halted**,
and one resumed from a snapshot is halted from its first step: a 3 G budget executed **495 M**, so a
script anchored at `@2200M` fired **0 of 12** steps and read as a wheel nobody was listening to.
Anchors that did land were worse — they fired in a bunch during a disk scan and the run reported
`1 word read of DATA, 11 frames dropped unread`, which is what a hung driver looks like. Anchored in
simulated time on the same snapshot: **16 posted, 0 dropped, 16 read, 16 interrupts.**

`trace --restore=` prints the clock it resumed at. Anchor past it, and read the `script: N of M
steps fired` line before believing anything the run says about input.

## The research

`research/` is the larger half of this project. What was believed and why it was wrong is preserved
rather than tidied away, and retractions are made in place next to the claim.

| | |
|---|---|
| `research/04` | the bypass ledger — what is faked, with a retirement condition for each |
| `research/06` | Rockbox as an oracle |
| `research/11` | the co-processor's runtime |
| `research/12` | how RetailOS draws |
| `research/16` | the third bootloader, and iPodLinux |
| `research/17` | the boot matrix — every OS on every ROM |

`grep` over the directory rather than reading an index; there isn't a current one.

`docs/HOW-IT-WAS-BUILT.md` is the day-by-day, taken from the commit log rather than from memory.

## Ghidra

`GHIDRA.md` covers the headless path — `ipod-boot ghidra serve`, then queries against the
decompiler without opening a window.
