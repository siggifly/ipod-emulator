# Rockbox as an oracle

**RetailOS is stripped C++ with no symbols. Rockbox drives the same PP5022 hardware, is open
source, and ships an ELF with 5 808 symbols.** Running it on this emulator converts every
divergence from *"a hex address in a binary we cannot read"* into *"a function name and a line of
source"*.

This was the strategic answer to a night spent generating and killing hypotheses one at a time. The
bottleneck was never measurement — the instruments are good — it was that **every measurement landed
in a binary with no ground truth**.

## It works, first attempt

`tools/ipod-boot/rockbox.sh` warm-enters `rockbox.ipod` at `0x10000000`. Rockbox runs **913 611
instructions** and executes **93 854 distinct code buckets** — against RetailOS's 19 660 in a run
sixteen times longer.

And when it stops, the profile says this:

```
0x00000270   47.0%   cpu_init+0x88
0x00000240    8.0%   cpu_init+0x58
0x00000250    1.5%   cpu_init+0x68
```

**That is the entire point.** `cpu_init` is in `firmware/target/arm/pp/crt0-pp.S`, we have the file,
and `+0x88` lands in the BSS-zeroing loop with `+0x58` in the `.init` copy — ordinary startup work,
which is what 913 K instructions of a booting OS should look like.

## What it has already validated about our emulator

Each of these was previously an assumption:

| thing | evidence |
|---|---|
| **The MMAP model is right** | Rockbox programs the remap itself (`remap_start` in `crt0-pp.S`, `MMAP_LOG`/`MMAP_MASK`/`MMAP_PHYS`) and then keeps executing. §33's decode holds up against a second, independent implementation |
| **Bypass #7 (COP asleep) is adequate** | `cpu_init` opens with `ldr r3,[r4]; tst r3,#COPSLEEPING; beq 1b` — a hard spin until the coprocessor reports sleeping. Rockbox passes it, and the spin does **not** appear in the profile |
| **The relocation/alias layout is right** | Memory at `0x03e80520` after the run is byte-identical to the image file at `0xa932c+0x520`. The `.init` copy to the codec buffer landed exactly |
| **BSS reclaim behaves as documented** | `0x000a932c` reads zeros afterwards — the `.init` source region is reclaimed, exactly as `crt0-pp.S` says it is |

## Where Rockbox currently stops

Execution runs off the end of the `.init` section — `_initstart` = `0x03e80000`, size `0x11bb8`, so
`_initend` = `0x03e91bb8`, and the last new code is at `0x03e92bb0`, past it, executing zeros until
it falls out of DRAM at `0x04000000`. `r4 = 0xdeadbeef` at the halt is Rockbox's **stack munge
value**, loaded in `crt0-pp.S` right after the copies — so the copies completed and it got past
them.

`main` itself is fine: `0x03e80520` holds `stmdb sp!,{r7,lr} / sub sp,sp,#0x30 / bl …`, matching the
file. So this is not a bad copy. Something inside `.init` falls through instead of branching, and
**that is now a debuggable question**, because every frame has a name.

## The load contract, verified

Both `.ipod` files are an 8-byte wrapper — big-endian checksum + ASCII `ipvd` — over a raw ARM
image (`tools/scramble.c`, `modelnum = 5`; stripped by `firmware/common/rb-loader.c`). Both
checksums were **recomputed and matched**, so the downloads are intact and the format is confirmed
rather than assumed.

- `rockbox.ipod` — linked for address 0 (`app-pp.lds`: `DRAMORIG 0x00000000`, ELF `e_entry = 0`),
  loaded to `DRAM_START` = `0x10000000` and entered there; it remaps SDRAM to 0 itself, detecting
  where it is running with `and r6, pc, #0xff000000`.
- `bootloader-ipodvideo.ipod` — linked at `0x40000000` but **position-independent**: it copies
  itself to IRAM and jumps. Installed by appending to the `osos` image.

## What this confirmed about the identity path

Independently of [research/11](11-rtxc-and-the-video-coprocessor.md) §54, Rockbox names the field:

```c
/* firmware/export/hwcompat.h */
#ifdef IPOD_VIDEO
#ifdef BOOTLOADER
#define IPOD_HW_REVISION (*((unsigned long*)(0x0000405c)))
#else  /* ROM is remapped */
#define IPOD_HW_REVISION (*((unsigned long*)(0x2000405c)))
#endif
```

`0x405c` is `SCfg + 0x5c` — the `HwVr` record's value word, exactly where we found `0x000b0011`.
Generation is `hw_rev >> 16`, confirmed against `ipodloader2`'s `hw_ver` and Rockbox's own
`(IPOD_HW_REVISION >> 16)` comparisons.

**Two honest caveats, carried forward from the acquisition rather than smoothed over:**

- **`0x000B0011` appears in no published table.** The archived iPodLinux "Generations" table lists 5G
  as `0x000B0005` or `0x000B0010`, and never recorded a 5.5G value — its wiki states *"5.5G iPods
  never had a SysInfo file"*. That `0x11` is "the 5.5G" is a **reasonable inference from the next
  board revision, not a sourced fact**. The high half (`0x000B` = iPod Video family) is solid.
- **Our ROM dump is labelled a prototype**, with a dummy serial (`U1234567890`) and a blank `HwId`,
  so the low half may be prototype-specific.

## The one that matters for bypass #6

`lcd-video.c` uploads a **`vmcs` blob to the Broadcom chip**, located through a directory at
`ROM_BASE + 0xffe00` of 10-word entries tagged `flsh`. That directory was parsed in our own dump: 5
sections — `disk`, `diag`, `scan`, `logo`, `vmcs` — **all checksums valid**, with `vmcs` being
101 728 bytes at ROM offset `0x6ec98`.

[research/12](12-bypass-ledger.md) #6's retirement condition has always read *"executing the `vmcs`
firmware"*, and it has always looked like the largest job in the file. **The blob is in the flash we
already have, at a known offset, with a valid checksum, and Rockbox contains readable source for how
it is uploaded.** That is a materially different proposition from writing a VideoCore II emulator
from nothing.

## The first divergence the oracle found

Rockbox falls through the **linker veneer table** at the end of `.init` — `__i2c_readbyte_veneer`,
`__pcf50605_init_veneer`, `__memset_veneer` and the rest, executed one after another in address
order at exactly four instructions per 16-byte bucket. A veneer is `ldr pc, [pc, #-4]` + a target
word; it branches immediately. Walking *through* them means they are zeros.

They are. And this is our bug, not Rockbox's.

### Bounded precisely

| | |
|---|---|
| `.init` runs at | `_initstart = 0x03e80000`, `_initend = 0x03e91bb8` (`0x11bb8` bytes) |
| copied from | `_initcopy = 0x000a932c` |
| **last address with correct data** | `0x03e90c00` (source `0x000b9f2c`) |
| **first address left zero** | `0x03e90d00` (source `0x000ba02c`) |

Everything below the boundary matches the image **byte for byte**, including regions that are
legitimately zero in the file. Everything above is zeros where the file has code.

### What has been ruled out

- **The loop did not exit early.** Breaking at `0x0000024c`, immediately after it, gives
  `r2 = 0x03e91bb8` — exactly `_initend`. Every iteration executed, so every store executed.
- **The source is intact.** `0x000ba02c` and `0x000ba32c` match the image file both *before* the
  copy (`--stop-at=0x23c`) and *after* it. The copy did not clobber its own source.
- **Nothing zeroed the destination afterwards.** The dump is taken at `0x24c`, the instruction after
  the loop. It is already zero there. The BSS (`0x000a6cf0..0x00132960`) and IBSS (IRAM) zeroing
  loops both run *after*, and neither covers `0x03e9xxxx`.
- **Nothing is unmapped.** The access report is empty: `osos-low` and `sdram-low` both at
  `0x00000000`, and `sdram-low` is a full `0x04000000`. The address is mapped and writable.
- **Translation is not splitting it.** MMAP0 reads back `LOGICAL = 0x00003c00`,
  `PHYSICAL = 0x10000f84` — exactly the values `crt0-pp.S` writes (`MMAP_MASK` for the 64 MB part,
  `MMAP_FLAGS | (pc & 0xff000000)`). The `0x13e90c00`/`0x13e90d00` aliases behave identically to the
  low addresses, so both sides of the boundary resolve the same way.

So: the loop ran, the source was good, the destination was mapped and writable, nothing overwrote
it, and roughly the last `0xfb8` bytes of an `0x11bb8`-byte copy did not land. **That is
contradictory, which means one of those five statements is measuring the wrong thing** — and finding
which is the next job.

### Why this is the point of the exercise

This took about an hour from "Rockbox boots" to a divergence bounded to a **256-byte window**, with
the failing mechanism named (`.init` veneers), the correct data available for comparison (the image
file), and the authoritative constants in hand (`crt0-pp.S`).

The equivalent question in RetailOS consumed a night and produced "an unbound delegate at
`record+0x20` that nothing writes", ~~which is still unresolved~~ **which was wrong in its premise —
`+0x20` is written, on all 790 objects of its shape ([research/20](20-the-resource-image.md) §2), and
the delegate is null because a font lookup missed.** Same class of bug; the difference is
entirely that one target has source and symbols.

**Note for whoever picks this up:** `--watch` records *changes*, so it cannot distinguish "wrote 0
over 0" from "never wrote". It reported no writes here and that is not evidence either way. A
write-attempt counter — the shape of the DMA-drop counter added in §51 — is the instrument this
needs.

> **This note aged into the thing it warned about.** The instrument built in answer to it,
> `--watch-range`, carried a *different* blind spot — word-sized writes into a mapped region — and
> went on to produce two more false absences of its own before anyone re-ran the standing claims
> through it (research/19, retracted 2026-08-13; mechanism in research/20 Addendum 8). Writing the
> warning down was not enough, twice. The rule that would have caught it is in `NEXT.md`: a new
> instrument's first job is to re-run the conclusions the old one produced, as a deliberate pass.

## Resolved: a silent data-corruption bug in the emulator's page cache

The five contradictory statements above were contradictory because one of them was measuring the
wrong thing — and `--watch` was not the culprit. `--writelog=BASE:SIZE` was: it records where a store
**lands**, with the PC, the value, and the region that answered.

```
write log: 256 stores recorded, 0 dropped
  -> sdram-low    256
    pc 0x00000244  0x03e90b00 = 0xe3540004  sdram-low
    pc 0x00000244  0x03e90ef0 = 0x00000000  sdram-low
```

**Nothing was dropped. The copy stored zeros because it was handed zeros.** The fault was in the
*reads*, and comparing each stored word against the image file pinned it exactly:

| destination | source | stored | file |
|---|---|---|---|
| `0x03e90c00` | `0x0b9f2c` | `0xe58d3000` | `0xe58d3000` ✓ |
| `0x03e90ef0` | `0x0ba21c` | `0x00000000` | `0xe5d430e8` ✗ |

### The bug

`fast_region` caches a page→region mapping, and required **a whole page to fit inside a region**:

```rust
off < r.data.len() && r.data.len() - off >= PAGE as usize
```

…then used `position(fits)` — so when the correct region could not hold a full page, the search
**continued to the next region that could**. Two regions share base `0`: the firmware image
(`osos-low`, `0xbaee4` bytes) and SDRAM. The image's last partial page, `0xba000..0xbaee4`, cannot
hold 4 KB — so every read there silently fell through to **SDRAM and returned zeros instead of
firmware**.

Predicted boundary `0xba000`; measured last-good source `0x0b9f38`, first-bad `0x0ba21c`. Exact.

Nothing was ever reported unmapped, because **the wrong region answered rather than no region at
all** — which is precisely why five separate checks all came back clean.

The fix makes the fast path agree with the slow one: `locate_idx` takes the *first* region
containing an address, so the cache now picks that region and then decides whether it is cacheable,
never searching past it.

### What it bought

| | before | after |
|---|---|---|
| Rockbox outcome | `Lost(0x04000000)` after 913 K instructions | runs to budget |
| profile buckets | 256 | 2 140 |
| where | `.init` veneers, executing zeros | **`switch_thread`, `corelock_lock/unlock`, `__bitarray_ffs`** |

**Rockbox now boots to its own scheduler.** `lcd_init_device` runs. The kernel is scheduling.

One more thing was needed to get there, and Rockbox named it in C:

```c
/* firmware/target/arm/pp/usb-fw-pp502x.c */
while ((inl(0x70000028) & 0x80) == 0);
```

Bit 7 of `0x70000028` is the USB module's reset-complete signal, polled forever on a machine that
does not model it — 98.2% of the profile sat in `usb_reset_controller+0x6c`. Supplying it is
currently `--rdval=0x70000028=0x80`, which is a bypass of exactly the #1/#2 family, but for the first
time **with the semantics documented in source** rather than inferred from a spin.

### And it names two of our long-standing mysteries

`firmware/export/pp5020.h` is the real register map we have been reconstructing:

```c
#define DEV_INIT2        (*(volatile unsigned long *)(0x70000020))
#define XMB_RAM_CFG      (*(volatile unsigned long *)(0x7000003c))
#define DEV_RS           (*(volatile unsigned long *)(0x60006004))
#define DEV_EN           (*(volatile unsigned long *)(0x6000600c))
```

`0x7000003c` is [research/12](12-bypass-ledger.md) **#2's register**, now sourced rather than
guessed. `0x70000030` — **#1** — is still absent from the header, so that one remains genuinely
undocumented.

### The RetailOS control

Re-run with the fix: **unchanged.** 505 resets, 6 033 profile buckets, same ATA and BCM totals. The
page-cache bug was real and is fixed, and it is **not** RetailOS's blocker. Worth stating plainly,
because a fix this suggestive invites the assumption that it explains everything.

## Rockbox boots to its menu

![the menu] — themed background, icons, sidebar logo, status bar, and Files/Database/Resume
Playback/Settings/Recording/Playlists/Plugins/System/Shortcuts.

Getting there took three fixes, each named by Rockbox itself.

**1. Install its files.** The first boot drew `Installation incomplete` — correct, since the disk had
`iPod_Control` but no `.rockbox`. An APFS clone of the 8 GB image (`cp -c`, instant, zero extra
space), `Virtual: Yes` verified before attaching, `.rockbox` copied in, unmounted.

**2. ATA writes did not exist.** The next boot panicked, and printed why:

```
*PANIC* (4.0)
dc_writeback_callback() - Could not write sector 8908074 (error -53)
```

The model implemented `IDENTIFY`, `READ DMA`, `READ SECTORS` and a few no-ops, and **aborted every
write**. Adding `WRITE DMA` (with the outbound mirror of the `dma_ready` hand-off, since `Ata`
cannot reach `Memory`) moved the error to `-43`.

**3. The 5.5G writes by PIO, not DMA.** `ata.c` composes these as `ret * 10 - n`, so `-53 → -43` is
the inner code moving `-5 → -4`: `wait_for_end_of_transfer`. The iPod Video defines
`MAX_PHYS_SECTOR_SIZE 1024`, so Rockbox does read-modify-write through **PIO** — and the WRITE
command had been added without the data-register path to feed it. With that, the drive finishes its
transfers and Rockbox reaches its menu.

Writes are **opt-in** (`--disk-writable`, default off) so an emulator bug cannot rewrite the one
disk image this project has; `ipod8g.img` hashes identically before and after, and every write went
to the clone.

### What is proven working end to end

CPU · MMAP · timers · interrupts · threading and scheduler · **ATA reads and writes** · **FAT32
read/write** · LCD init · **BCM2722 command protocol and framebuffer** · fonts · icons · theming.

### And what it says about bypass #6

[research/12](12-bypass-ledger.md) #6's retirement condition reads *"executing the `vmcs` firmware"*,
and it has always been described as the largest job in the file. **Rockbox drives this display to a
complete themed UI without `vmcs` ever being executed.** The synthesised replies are evidently
sufficient for real rendering, so #6 is a smaller job than it has been billed as — and the display
is no longer a capability gap.

RetailOS, re-run after every one of these fixes, is **unchanged**: 505 resets, 6 033 profile
buckets. We now have a machine that provably renders, so RetailOS not drawing is RetailOS's own
state rather than a device we lack.

## A self-check for the whole class of bug

The `fast_region` bug was invisible to unmapped-access reporting, to `--watch`, and to four other
checks, because **the wrong region answered rather than no region**. That failure mode is
indistinguishable from *"a field nobody ever wrote"* — which is exactly what RetailOS's blocker
looks like. So the question worth answering is not "is there another one" but "would we know".

`--verify-memory` cross-checks the page cache against the slow path on every access and reports any
disagreement with the PC and both region names.

**Validated against the bug it was built for.** Reintroducing the old `position(fits)` search:

```
verify-memory: 64 DISAGREEMENTS (fast path answered a different region)
  pc 0x00000240  addr 0x000ba000  fast=sdram-low slow=osos-low
```

`0x000ba000` is the predicted boundary to the byte, and `pc 0x00000240` is the `ldrhi r5, [r4], #4`
in Rockbox's `.init` copy. A control that can fire, firing.

**And the result on the real tree:**

| | verdict |
|---|---|
| Rockbox to its menu | **no fast/slow disagreements** |
| RetailOS, 800 M instructions at `--clock=5` | **no fast/slow disagreements** |

So the memory model is self-consistent on both paths, and **RetailOS's blocker is not a
memory-model lie of this class.** That closes off the hypothesis that the unbound delegate at
`record+0x20` is our emulator silently answering from the wrong region — which, after the
`fast_region` bug, was the single most plausible remaining explanation.

It is a negative, and it is worth the run: it removes the last explanation that would have made the
blocker *our* fault rather than RetailOS's state.
