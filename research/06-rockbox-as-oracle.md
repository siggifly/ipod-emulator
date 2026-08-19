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

Independently of [research/03](03-rtxc-and-the-video-coprocessor.md) §54, Rockbox names the field:

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

[research/04](04-bypass-ledger.md) #6's retirement condition has always read *"executing the `vmcs`
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
`+0x20` is written, on all 790 objects of its shape ([research/10](10-the-resource-image.md) §2), and
the delegate is null because a font lookup missed.** Same class of bug; the difference is
entirely that one target has source and symbols.

**Note for whoever picks this up:** `--watch` records *changes*, so it cannot distinguish "wrote 0
over 0" from "never wrote". It reported no writes here and that is not evidence either way. A
write-attempt counter — the shape of the DMA-drop counter added in §51 — is the instrument this
needs.

> **This note aged into the thing it warned about.** The instrument built in answer to it,
> `--watch-range`, carried a *different* blind spot — word-sized writes into a mapped region — and
> went on to produce two more false absences of its own before anyone re-ran the standing claims
> through it (research/09, retracted 2026-08-13; mechanism in research/10 Addendum 8). Writing the
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

`0x7000003c` is [research/04](04-bypass-ledger.md) **#2's register**, now sourced rather than
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

[research/04](04-bypass-ledger.md) #6's retirement condition reads *"executing the `vmcs` firmware"*,
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


## 2026-08-18: Rockbox boots, and two device models were missing

Re-run on the current tree after the resource reorganisation, which had left this script's `FLASH`
default — and `retail-boot.sh`'s `TRACE` default — pointing at paths that no longer existed.

### First result: it draws, and then stops dead

| | |
|---|---|
| budget | 200 M, then 3 000 M |
| result | `BudgetExhausted` both times — no fall out of DRAM, so the `.init` copy bug above stays fixed |
| panel | **22 179 non-black pixels**: the Rockbox logo and `Ver. 4.0` |
| ATA commands | **0** |
| halt | `r15 = 0x0007e888`, and the splash is unchanged from 200 M to 3 000 M |

The map file names it: `0x0007e888` is inside `wmcodec_write` (`0x7e7cc..0x7e900`) with `LR` in
`usb_init_device`. **The map was misleading and the disassembly settled it** — `--dis=0x7e858:0x58`
gives

```
0007e870  ldr r3, [r2, #0x20]     ; r2 = 0x70000000
0007e874  orr r3, r3, #0x80000000 ; INIT_USB
0007e878  str r3, [r2, #0x20]     ; DEV_INIT2 |= INIT_USB
0007e87c  ldr r3, [r2, #0x28]
0007e880  tst r3, #0x80
0007e888  beq 0x0007e87c          ; spin, no timeout
```

which is `usb-fw-pp502x.c:114-116` word for word, including the `|= 0x2`, the `udelay` through
`0x60005000` and `XMB_RAM_CFG |= 0x47A` that follow it. Rockbox was waiting for a **USB clock-ready
bit this emulator had never had a reason to set**, and the previous section's *"Rockbox to its
menu"* could not have been produced by this recipe.

### The fix, and why it is not a bypass

`Xmb::usb_clock` sets bit 7 of `0x70000028` when `INIT_USB` is written to `DEV_INIT2` — modelled as
a *consequence of the enable*, not as a bit that is simply always on, so a driver that forgot to
start the clock would still hang.

**Measured before it was written**, with `--read-count=0x70000028,0x70000020` over a 600 M RetailOS
boot: `0x70000020` is read ten times from five call sites, and **`0x70000028` is read zero times**.
Apple's firmware never looks at this address.

### Second result: it boots

| | before | after |
|---|---|---|
| ATA commands | 0 | **2 393** |
| distinct pictures | 2 | **4** |
| what it draws | splash | splash → `Scanning disk…` → *"Battery empty! RECHARGE! Shutting down…"* |

![Rockbox 4.0 booting on this emulator](../docs/media/ipod-13-rockbox-boot.gif)

### The battery message is not the battery

Rockbox's `adc_init` says `adc_battery->channelnum = 0x2; /* ADCVIN1, resistive divider */`, and
the census showed **every one of 25 677 conversions on channel 2** — which this emulator was
answering with the 3 000 mV catch-all for unknown channels, below Rockbox's 3 400 mV danger
threshold. Channel 2 now answers `0x2c0` (4 125 mV) like the other battery inputs, sourced from
that line rather than guessed.

**It did not fix the shutdown, and the control is why we know.** Forcing `--pmu-adc=2=0x3ff` —
5 994 mV, full scale — still prints the same message and still powers off. So the voltage is not
the trigger, the ADC correction stands on its own evidence rather than on having fixed anything,
and the real cause is elsewhere on `shutdown_screen()`'s path. `battery_level_safe()` reads
`voltage_now`, a filtered value the power thread maintains; a shutdown *requested* for some other
reason would print exactly this while that filter is still empty.

Both changes were re-validated against the recipe every number here is measured on. RetailOS at
600 M is **unchanged to the digit**: 27 510 code buckets, 76 800 non-black pixels, `0xc8 READ DMA
x475 · 0x20 READ SECTORS x4 · 0xca WRITE DMA x4 · 0xec IDENTIFY x3 · 0xef SET FEATURES x11`, wheel
reporting ON with 2 `0x052a` commands.

### What is next, and it is bounded

Why a shutdown is requested at all. Rockbox has 5 808 symbols and this is a two-frame window
between `Scanning disk…` and the message — which is the entire argument this file was written to
make.

## 2026-08-18, later: the menu, and a third model shaped around Apple

### It reaches the menu

Fixing the ADC (below) took Rockbox from a splash to its **main menu** — Files, Database, Resume
Playback, Settings, Recording, Playlists, Plugins, System, Shortcuts — drawn through the same
co-processor transport RetailOS uses.

![Rockbox 4.0's main menu](../docs/media/ipod-14-rockbox-menu.png)

### The ADC completed on transfers, not on time

`_adc_read` (`adc-ipod-pcf.c:50-55`) writes `ADCC1` and reads `ADCS1`/`ADCS2` **immediately**, one
read per conversion, no poll of the ready bit, once per 400 ms. This model completed a conversion
after a countdown of **two read transfers** — which Apple's polling driver satisfies out of its own
poll loop, and Rockbox never does. The countdown went 2 → 1, the next conversion reset it to 2, and
`latch` did not run once in 27 000 conversions. Rockbox read 0 mV and `query_force_shutdown()`
powered the machine off.

A conversion now settles before the host's next transfer. A microsecond deadline was tried first
and is wrong here for a specific reason: **this model's bus costs no simulated time**, so a deadline
in µs is compared against a clock that never advanced for the transaction it was meant to outlast.

RetailOS across the change: **27 510 code buckets, 76 800 non-black pixels, wheel reporting on with
2 commands and 3 frames, same ATA opcode census** — unchanged.

### The shutdown that remains is not a bug

With the ADC fixed, every `sys_poweroff` call comes from **one** site — `handle_auto_poweroff+0xb8`,
the idle-timeout branch, not the battery branch that produced the earlier message.

And the idle timeout is expiring honestly:

```
cpu sleep: 7 526 995 halts, 1 332 504 ms of simulated time skipped
irqs: 7 051 288 asserted, 1 034 209 taken; usec 1 343 170 682
```

**22 simulated minutes, of which 1 332 s was skipped sleep**, inside about 11 s of executed time.
Rockbox reaches its menu, idles, and this emulator fast-forwards the idling — so its 10-minute
idle poweroff arrives on schedule. Rockbox is correct and the emulator is correct; what is missing
is a thumb on the wheel.

### And a thumb on the wheel does not reach it

Driving `--wheel` from just after the menu appears changes nothing: **0 frames posted, 0 word reads
of `CLICKWHEEL_DATA`**, and Rockbox writes `0x7000c104` exactly **once** in a run where RetailOS
re-arms it continuously.

`lib.rs:4801` is why:

```rust
if !w.reporting { w.frames_suppressed += 1; continue; }
```

`reporting` is set by opcode **`0x052a`** — *RetailOS's* way of asking for autonomous frames. Our
click wheel delivers input only to a firmware that speaks that protocol. Rockbox's
`button-clickwheel.c` arms the receiver its own way and is handed silence.

**That is the third model in two days shaped around Apple's driver rather than around the part** —
after the USB clock-ready bit and the ADC. The pattern is now the most reliable bug-finder this
project has: *anywhere a device's behaviour was derived from what RetailOS does, a second stack
finds the seam.*

## 2026-08-18, later still: input, and the file browser on a real volume

Removing the `0x052a` gate (below) let wheel frames reach Rockbox: **78 frames posted, 78 reads of
`CLICKWHEEL_DATA`, 78 with a frame waiting**, and the menu selection moves. `press=select` on
*Files* opens the file browser.

### The empty listing was correct, and here is the control

The browser opened on **nothing**, which looked like a failed mount. It is not. Every entry at the
root of `ipod8g.img` carries the hidden attribute:

| entry | attr | |
|---|---|---|
| `IPOD` | `0x08` | volume label, not a file |
| `SYSTEM~1` | `0x16` | HIDDEN · SYSTEM · DIR |
| `IPOD_C~1` | `0x12` | HIDDEN · DIR |
| `$RECYCLEBIN` | `0x16` | HIDDEN · SYSTEM · DIR |

and `apps/filetree.c:352-356` skips them:

```c
if (*c->dirfilter != SHOW_ALL &&
    ((entry->d_name[0]=='.') || (info.attribute & ATTR_HIDDEN))) {
    continue;
}
```

**The positive control.** On a *copy* of the image, one byte: clear the hidden bit on `IPOD_C~1`
(`0x12` → `0x10`). Same build, same script, same everything else.

![Rockbox listing iPod_Control](../docs/media/ipod-16-rockbox-files.png)

The frame goes from 265 non-black pixels to 2 670 and `iPod_Control` is listed and highlighted. So
Rockbox mounts this emulator's FAT32 volume through the emulated ATA controller, walks its
directory, and applies its own filter correctly. An absence that a one-byte change turns into a
presence is a measurement; an absence on its own is not.

*(Partition type was the suspect and is not the cause: our images are FAT32 type `0x0C`, and
`firmware/common/disk.c` normalises to `PARTITION_TYPE_FAT32_LBA` whenever it finds a valid FAT.
`ipodloader2` is the one that only accepts `0x0B`, which matters for M4 and not here.)*

## 2026-08-18: installed, and cold-booted by Apple's own bootloader

`ipod-boot install-os SRC.img OS.ipod OUT.img` writes an operating system into a **new** drive
image's firmware partition, the way `ipodpatcher` does on hardware: append after `osos`, point the
directory's `entryOffset` at it, fix the checksum, shift the later images out of the way. Nothing
new boots — the machine's real cold path finds it at the address it already looks at, so this is
not a seventh bypass.

**And it works:**

![The Rockbox bootloader, cold-booted](../docs/media/ipod-17-rockbox-bootloader.png)

```
Rockbox boot loader          Version: v4.0
IPOD version: 0x000B0005
Emulated iPod Disk
Partition 1: 0x0C 16744448 sectors
Loading Rockbox...
Error!  Can't load rockbox.ipod: File not found
```

Read that as a report on *us*. It found the ROM's hardware revision (`0x000B0005`, the value in our
NOR dump), read our ATA IDENTIFY string back (`Emulated iPod Disk`), and parsed our partition table
— **type `0x0C`, accepted**. Then it looked for `/.rockbox/rockbox.ipod` on the FAT32 volume and
correctly reported that it is not there, because it is not.

### The first attempt produced an image the bootloader rejected, and the fix is worth recording

It booted to *"Connect to your computer. Use iTunes to restore."* after **71 ATA commands** —
against 637 for the unmodified image through the same recipe, which is the control that made it
attributable.

The cause: **image data begins one sector past the partition; the directory does not.**
`ipodpatcher.c:1586` sets `fwoffset = start + sector_size`, and `diroffset` is relative to `start`.
Writing both at the partition base put every byte 512 short.

It is measurable rather than a matter of reading the C: in a stock image, **both** recorded
checksums match a byte sum over `[devOffset + 512, +len)` and **neither** matches one over
`[devOffset, +len)`.

```
osos: recorded 0x2c7c48f3 · sum[dev,+len) = 0x2c7d4460 · sum[dev+512,+len) = 0x2c7c48f3
rsrc: recorded 0x18319bab · sum[dev,+len) = 0x1832856f · sum[dev+512,+len) = 0x18319bab
```

So the installer now **reproduces the checksums that are already there before writing new ones**,
and refuses if it cannot. That check fails on a file nobody has modified, in a second, instead of
producing a plausible image that dies seventy ATA commands into a boot. Both behaviours are tested.

## 2026-08-18: the whole chain, from Apple's reset vector to Rockbox's own binary

`ipod-boot put-files DISK.img SRC_DIR` writes a directory tree into the drive image's FAT32
volume — the other half of an install, and the half the Rockbox bootloader was asking for when it
said *"Can't load rockbox.ipod: File not found"*. 381 files, 19.3 MB, in 1.7 s; an independent
reader (`ipod-boot fat`) sees all 404 entries with their long names intact.

**Every link now runs:**

```
Apple's boot ROM  →  Apple's bootloader  →  the Rockbox bootloader we installed into the
firmware partition  →  rockbox.ipod, loaded from the FAT32 volume we wrote  →  Rockbox
```

![Rockbox, cold-booted from its own binary on disk](../docs/media/ipod-18-rockbox-cold.png)

The splash appears at ~110 M instructions and holds for 15 M. Nothing here is warm-entered and no
step is skipped.

### And then it powers off — from a *different* branch than the warm path does

Bounded precisely, which is the useful part:

| | warm-entered (`rockbox.sh`) | cold-booted from disk |
|---|---|---|
| draws | splash → `Scanning disk…` → **menu** | splash, then black |
| `sys_poweroff` callers | all `handle_auto_poweroff+0xb8` — the **idle** branch | all `+0x64` — the **`query_force_shutdown`** branch |
| `power_off()` | — | **230 calls** (it cannot actually power off, so it loops) |

So the cold path fails a battery check that the warm path passes, with the same ADC model and the
same `--pmu` flag in both recipes.

**What has been ruled out**, each by measurement rather than reasoning:

- *The firmware partition was damaged by the FAT writes.* No — both directory checksums still
  reproduce exactly after `put-files`.
- *Wheel input would keep it alive.* No — 124 frames delivered from during the splash, 117 reads of
  `CLICKWHEEL_DATA`, and it still blanks at the same instruction count.
- *A `battery_levels.cfg` in the installed tree moved the thresholds.* No — the Rockbox 4.0 build
  contains no such file.
- *The ADC answers badly.* Channel 2 returns `0x2c0` (4 125 mV) here as everywhere.

The ADC census is itself the next clue: **9 237 conversions on channel 0 and exactly one on
channel 2**. Channel 0 is Apple's bootloader polling before the handoff; one conversion on Rockbox's
own channel means it shut down before its power thread ever ran a second time — so `voltage_now` is
being decided by a single early reading, or by nothing at all.

### Narrowing the cold-boot shutdown: the reading is zero, not low

Both addresses read straight out of the code rather than guessed, by disassembling
`query_force_shutdown` at `0x00068358`:

```
0006835c  bl 0x00068340          ; power_input_present() -- early out if true
0006836c  ldr r3, =0x000a6ba8    ; &battery_level_shutoff (u16)
00068374  ldr r3, =0x000e23b8
00068378  ldr r3, [r3, #0xfc]    ; voltage_now  ->  0x000e24b4
0006837c  cmp r3, r0
00068384  movlt r0, #1           ; voltage_now < shutoff  =>  force shutdown
```

Watching `voltage_now` separates the two paths immediately. Same store, same code
(`0x000687e4`), 300 M instructions:

| | `r0` (filter accumulator) | `r3` (new `voltage_now`) | decay per step |
|---|---|---|---|
| **warm** | `0x00080f7e` | `0x101d` = **4 125 mV** | −1 |
| **cold** | `0x000000fe` | **0** | −0x20 = −32 mV |

So the cold path's battery reading is **zero, not merely low**, and the exponential filter walks
4 160 → 3 300 in about twenty-seven power-thread iterations. That is the shutdown.

**Two more explanations eliminated, both by measurement:**

- *The ADC result registers.* `--pmu-force=0x30=0xb0 --pmu-force=0x31=0x80` rescues the **warm**
  path (it is what first produced the menu) and does **nothing** in the cold one — 315
  `sys_poweroff` calls with the force applied. So the failure is upstream of those registers.
- *The I²C bus never going idle.* `pp_i2c_wait_not_busy` (`i2c-pp.c:55`) gives up after `HZ` ticks
  and `_adc_read` does not check the result, which would leave `data[2]` as stack garbage — a very
  good story. It is wrong: `--watch=0x7000c01c` reports **no writes at all** in either path, so
  `I2C_STATUS` reads 0, `I2C_BUSY` is clear, and the wait returns on its first look. (Consistent
  with that address sitting in the *both* column of [research/15](15-the-register-agreement-table.md)
  — a pure input we invent.)

**Where it stands.** The census records exactly **one** channel-2 conversion in the cold path
against 9 237 on channel 0 (Apple's bootloader, before the handoff). One conversion means
`adc->data` is populated once by `adc_init`'s own `_adc_read` and cached from then on — so a single
early read returning zero would explain every later number. The next measurement is to catch that
one transaction and record the bytes it actually received.

### The cold-boot shutdown, narrowed to the I²C response path

`adc_read(ADC_BATTERY)` returns **`0x2c0` warm and `0` cold** — caught at `0x0007e228`, the
instruction after `_battery_voltage`'s call to `adc_read` at `0x000836e8`. Everything below follows
from separating that one value.

**The disk is not the variable.** Same instrument, three runs:

| | raw ADC value |
|---|---|
| warm + stock drive | `0x2c0` |
| warm + the drive with Rockbox installed | `0x2c0` |
| **cold** + the same installed drive | **`0`** |

So it is the boot path — state Apple's bootloader leaves behind — and nothing to do with the files
we wrote.

**Four more explanations eliminated, each by measurement:**

- *`adc_init` never ran, so `channelnum` stayed at its BSS zero.* No — `--break=0x03e9131c` hits
  **exactly once in both paths**.
- *Rockbox converts the wrong channel.* **It does — but not by asking for the wrong one.** The
  original text read this off `order (first 12 of N kept)`, which is a 4 096-capped sample of the
  *whole run* and cannot attribute anything to the second stack; that reasoning was unsound and is
  replaced below. The measured answer needed a control run, and it is that **9 235 of the cold
  path's conversions are Rockbox's and land on channel 0** — while the guest asks for channel 2
  every single time. See §"The channel is right in the guest and wrong at the device".
- *The PMU model ends up holding the wrong bytes.* No — **both** paths finish with
  `data registers now [0xb0 0x80 0x00 0x00]`, which is 704 with the ready bit set.
- *The I²C bus never goes idle, so `pp_i2c_read_bytes` returns `-2` and leaves `data[2]`
  uninitialised.* No — `--watch=0x7000c01c` reports **no writes at all** in either path, so
  `I2C_STATUS` reads 0 and `pp_i2c_wait_not_busy` returns on its first look.

**What is left, and it is narrow.** `--pmu-force=0x30=0xb0 --pmu-force=0x31=0x80` forces those
registers on every read. It rescues the **warm** path — it is what first produced the menu — and
changes nothing cold (315 `sys_poweroff` calls with the force applied). A forced register that does
not reach the guest means the failure is **not in the conversion and not in the register file**,
but in the step that copies a read's answer into the controller's data registers at
`i2c_base + 0x0c + 4i`.

That is four lines of this emulator, and the next measurement is to log what they copy — and
whether they run at all — on a cold boot.

### The ordered census is not a window onto Rockbox

> **Superseded within the hour it was written, and the way it failed is the lesson.** This section
> replaced one unsourced attribution with another: it argued — correctly — that the ordered print
> cannot attribute a conversion to a stack, and then attributed that `(2,704)` to Apple anyway, on
> the same evidence it had just declared insufficient. **It is Rockbox's.** A control run settles it
> in one command and nothing else does: cold-boot Apple's own image, where no Rockbox exists.
>
> ```
> retail-boot.sh                                  ->  20 conversions: ch0 x2, ch3 x9, ch4 x9
> retail-boot.sh DISK=rockbox-cold.img            ->  9 246:  ch0 x9237, ch2 x1, ch3 x5, ch4 x3
> ```
>
> **Apple's bootloader converts channel 2 zero times.** So the single channel-2 conversion is
> Rockbox's, and the 9 235 extra channel-0 conversions are Rockbox's too. The reasoning below about
> the instrument stands and is why the ADC now has a row in `NEXT.md`'s table; the conclusion it was
> used to reach did not, and the corrected finding is the section after this one.

**The `(2,704)` above is Apple's bootloader's, and reading it as Rockbox's inverted the finding.**
`trace.rs` prints `order (first 12 of N kept)` from `Pcf50605::adc_log`, which is
`Capped::new(4096)` — so those twelve are the first twelve conversions of the **entire run**. A cold
boot opens with Apple's bootloader doing **9 237** conversions on channel 0 before Rockbox executes
an instruction, so nothing in that window can be Rockbox's. This is R6 arriving in a place the
instrument table does not cover: the ADC has no row in it, and the by-channel tally beside this
print *is* uncapped (`adc_by_channel`), which makes the two numbers on screen look like one
instrument when they are two.

Put the uncapped tally back against the corrected ordering and the conclusion reverses:

| | cold |
|---|---|
| channel-0 conversions (uncapped tally) | 9 237 — Apple's |
| channel-2 conversions (uncapped tally) | **exactly 1** |
| does that one appear in the first twelve of the run? | **yes** |

One channel-2 conversion in the whole run, and it happens before Rockbox runs. **So Rockbox issued
no ADC conversion at all on a cold boot** — not a wrong channel, not a wrong value, none. Its first
`_adc_read` returned `0x2c0` by reading result registers that Apple's bootloader had already
latched, which is why the first read looked correct and made the second look like a regression. It
was never a regression; the first read was the accident.

### What the driver source says, and one more elimination that was too narrow

`resources/vendor/rockbox/src/firmware/target/arm/ipod/adc-ipod-pcf.c` is the driver, and it
settles the struct that was inferred from the store addresses:

```c
struct adc_struct {
    long timeout;                              /* +0  */
    void (*conversion)(unsigned short *data);  /* +4  */
    short channelnum;                          /* +8  */
    unsigned short data;                       /* +10 */
};                                             /* 12 bytes, IDATA_ATTR -> IRAM */
```

With `adcdata[]` at `0x40008e9c`, that puts `channelnum`/`data` in the word at `0x40008ea4` — which
is what was watched, and it matches to the bit (`0x02c00002` is data `0x2c0` beside channelnum 2).
It also puts **`conversion` at `0x40008ea0`**, one word below, and that word has never been looked
at. `adc_init` never assigns it; the field is only ever zero because the IRAM init copy is supposed
to make it zero. The cold path is precisely the path where IRAM was seen carrying instruction words
before being cleared.

**And the fourth elimination above was sound but too narrow.** It reads as though
`pp_i2c_wait_not_busy` were the one thing standing between the transfer and the store. Two things
the source shows instead:

- `_adc_read` has **no early return inside the branch at all**. Once `TIME_AFTER` passes, the write
  to `ADCC1`, the read of `ADCS1`/`ADCS2`, and the store `adc->data = value` all happen
  unconditionally. So a store IS evidence the branch was entered — and therefore evidence that
  `pcf50605_write(0x2f, …)` was executed.
- `pp_i2c_read_bytes` (`firmware/target/arm/pp/i2c-pp.c`) calls `pp_i2c_wait_not_busy` **twice** —
  once before it touches the controller and again *after* `I2C_SEND`. Returning `-2` from the
  second one skips the `*data++ = I2C_DATA(i)` copy entirely and leaves `data[2]` as uninitialised
  stack, and `_adc_read` ignores the return value. The `--watch=0x7000c01c` measurement eliminates
  both sites, since `I2C_STATUS` reads 0 throughout — but it was written as though there were one.

### The contradiction that names the next measurement

Two measured facts that cannot both be innocent:

1. The store at `0x000836ac` executed **twice**, and it sits after the register write with nothing
   between them that can skip it. Two stores therefore mean two writes to `ADCC1`.
2. The uncapped per-channel tally records **one** channel-2 conversion in the whole run, and that
   one is Apple's.

So a write to `ADCC1` was executed by the CPU and did not become a conversion in the device. The
cheapest explanation covering both symptoms at once is that **the controller's data registers at
`i2c_base + 0x0c + 4i` are not backed on the cold path**: `pp_i2c_send_bytes` stages the register
number and the value there before raising `I2C_SEND`, so if the model reads back zeros it performs
a write to PMU register `0x00` instead of `0x2f` — no conversion — and the read that follows copies
its answer into the same dead registers, so `data[0]`/`data[1]` come back zero and
`value = data[0] << 2 | (data[1] & 3)` is `0`. One broken mapping, both symptoms, no second bug
required.

That is a hypothesis, and it has a competitor that must not be assumed away: a non-zero
`adc->conversion` at `0x40008ea0` would also zero `value`, via `adc->conversion(&value)` — but it
would leave the conversion count at two, so **it cannot explain the missing conversion** and can
only ever be half the story.

Three measurements settle it, none costing more than a run:

- `--watch=0x40008ea0` on both paths. Zero cold ⇒ the function pointer is innocent and the whole
  question is the I²C mapping.
- Count the read replies the model actually delivers into `i2c_base + 0x0c + 4i`, and the bytes
  that find no region to land in.
- Count the same for the **write** direction — what the model reads back out of those registers
  when `I2C_SEND` goes up. This is the half that discriminates, because it is the half that decides
  whether a conversion starts, and no instrument reports it today.

### The channel is right in the guest and wrong at the device

**The value byte of a two-byte PMU write is lost inside this emulator on the cold path.** Not the
register byte beside it, not the read direction, and not on the warm path. Everything else in this
investigation was downstream of that.

The instrument that was missing: `Pcf50605::written` has carried `register -> (writes, last value)`
uncapped for weeks and **was never printed**, so "which register is the firmware hammering" was
answerable and "what is it putting in it" was not. The ADC's channel select *is* the value —
`ADCC1` bits 4:1 — so the read-side table saying `reg 0x2f x9236` says a conversion was requested
and says nothing at all about which channel. Printed, both arms of the standard recipe:

| | `reg 0x2f` writes | last value | meaning |
|---|---|---|---|
| **warm** (`rockbox.sh`) | 8 895 | **`0x05`** | `(2 << 1) \| 1` — channel 2, correct |
| **cold** (`retail-boot.sh DISK=rockbox-cold.img`) | 9 240 | **`0x00`** | channel 0 |

`0x00` is not a value that expression can produce: `(channelnum << 1) | 0x1` has bit 0 set
unconditionally, for every possible `channelnum`. So the byte reaching the device was never
computed by Rockbox, and the question stops being "what does the guest believe" and becomes "where
did the byte go".

**The guest is innocent, measured at the call.** `--enterlog=0x0007e144` on `pcf50605_write(reg,
val)` — address from Rockbox's own ELF, so this is a named function and not an inferred one:

```
0x0007e144 lr=0x00083670  r0=0x0000002f r1=0x00000005   @109269868
0x0007e144 lr=0x00083670  r0=0x0000002f r1=0x00000005   @124934640
…
386 of 386 calls with r0=0x2f carry r1=0x05, and all 386 come from lr=0x00083670 — inside _adc_read.
```

**Every call asks for channel 2.** The loss is between that call and the bus.

**It is lost at the store.** `--storeaddr=0x7000c010` (`I2C_DATA(1)`) on the cold path shows
`pp_i2c_send_bytes`'s copy loop writing the byte it was handed:

```
0x0007e328 -> [0x7000c010] = 0x00000005   @109269952     <- adc_init's read: the one good conversion
0x0007e328 -> [0x7000c010] = 0x00000000   @124934724     <- and every one after it
0x0007e328 -> [0x7000c010] = 0x00000000   @124965954
```

So `I2C_DATA(1) = *data++` stored **zero**, which means `data[1]` read as zero. `pcf50605_write`
takes `val` by value and passes `&val` down, so `data` is a one-byte stack local: **it is written
with 5 and read back as 0.**

`--enterlog=0x0007e2c0` gives the buffer address, and it is the same on both paths —
`r2 = 0x4000af5c`, IRAM. `--watch-range=0x4000af5c:8`:

| | byte-writes | distinct writing PCs | hottest |
|---|---|---|---|
| warm | 2 186 | 90 / 91 | `0x00084564` x292 |
| **cold** | **10 670** | 41 / 49 | **`0x00084f70` x2808**, `0x0007eb28` x1856, `0x0007eb40` x1856 |

That span is a heavily reused stack frame, and the cold path has hot writers on it that the warm
path does not have at all.

**What this is not** — each ruled out by measurement, not by reasoning:

- *Apple's bootloader still writing the bus underneath Rockbox.* It does write `I2C_DATA(0)`
  **26 517** times from `0x4000acac` — and identically often on Apple's own cold boot, which is the
  control. Its last write is `@63 401 686`; Rockbox's first is `@108 443 429`. **They never
  overlap.**
- *The controller's data registers being unbacked.* `replies delivered 44 978 (0 byte(s) with
  nowhere to land)` cold, `19 060 (0)` warm. Both directions of that mapping are fine, which kills
  the hypothesis the previous section was built on.
- *`adc_init` not running, or not setting the field.* `--watch-range=0x40008e9c:48` shows all three
  entries initialised by `.init` — `0x03e91328` writes `channelnum` **once, two bytes**, and no
  other PC ever writes that halfword.
- *`adc->conversion` being a live garbage pointer.* `0x40008ea0` takes **12 byte-writes in the whole
  run**, all from the three init/copy PCs, none later. The field is written once and never again.
- *Exception modes running on the interrupted stack.* `arm7tdmi`'s `cpu.rs` banks `r13`/`r14` per
  mode (`bank_irq`, `bank_svc`, `bank_abt`, `bank_und`, `bank_fiq`) as the architecture requires.

**Still open, and now one question instead of a family:** what writes over a one-byte stack local
between its spill and the copy four instructions later, on the cold path only.

Resolved against Rockbox's ELF, the cold-only writers into that span are not anonymous:

| PC | symbol | cold | warm |
|---|---|---|---|
| `0x00084f70` | **`queue_wait_w_tmo`** | 2 808 | — |
| `0x0007eb28` | **`set_cpu_frequency__lock`** | 1 856 | — |
| `0x0007eb40` | **`set_cpu_frequency__unlock`** | 1 856 | — |
| `0x00084564` | `corelock_init` | 280 | 292 |

`set_cpu_frequency__lock` / `__unlock` are **corelock** primitives — Rockbox's CPU↔COP mutual
exclusion — and the cold path runs frequency scaling that the warm path never reaches. That lands
this squarely on ground the project already knows is faked: **ledger #7** (`COP_STATUS` sticky,
pushed unconditionally by `trace.rs`) and [ROADMAP](../ROADMAP.md) M2's first oracle finding, that
`MBX_MSG_STAT` (`0x60001000`) is read **52 868 892** times by Rockbox and never once by RetailOS,
first from `switch_thread` — *a CPU↔COP mailbox this emulator does not model at all*.

**So the working hypothesis is that this is a second face of the unmodelled co-processor**, not a
new bug: a corelock whose other half never answers, on a boot path that actually exercises it. It
is a hypothesis and is written down as one — the discriminator is whether the clobbering write and
the spill interleave, which one `--storeaddr` on the buffer address, ordered, will show. Do not
model the mailbox on the strength of this paragraph.

**Settled when** the cold path writes `0x05` to `ADCC1` and the boot survives its own battery
reading.

> ### The first half of that is now true, and the second half is not — 2026-08-19
>
> **The cold path writes `0x05`.** Same recipe, `retail DISK=rockbox-cold.img`, 1.2 G:
>
> ```
> pcf50605 ADC conversions by channel (11 total):
>     reg 0x2f  x5  last value 0x05
> ```
>
> `0x05` is `(2 << 1) | 1` — channel 2, correct, where this section recorded 9 240 writes of `0x00`.
> The stack-local clobber is gone; the I²C and PMU work since then closed it. **Every measurement in
> the sections above was taken through that bug and should be re-read with that in mind.**
>
> **The boot still does not survive.** 113 ATA commands, and the panel holds Apple's logo — 4 frames
> in a 2 G film, none of them Rockbox's. So the ADC was one cause and not the only one, and the
> conditional this section closes with has to be split: the channel byte is settled, the cold boot
> is not.
>
> ### And the second core makes it worse, which is a finding rather than a setback
>
> The hypothesis this section ends on — *"a second face of the unmodelled co-processor"*, because
> the cold-only clobbering PCs are `set_cpu_frequency__lock` / `__unlock`, Rockbox's CPU↔COP
> corelock — is now testable, and the answer is not the one it predicted:
>
> | | ADC `reg 0x2f` last value | non-black pixels |
> |---|---|---|
> | cold, single core | **`0x05`** — correct | 0 |
> | cold, `--second-core` | **`0x89`** | 2 916 |
>
> `0x89` is `(0x44 << 1) | 1` — channel 68, which is not a channel. So running a real coprocessor
> **reintroduces** a clobber of the same byte, with a different value, on the same path; and the
> panel keeps Apple's logo instead of being blanked. The corelock reading was pointing at the right
> subsystem and the wrong direction: it is not that the COP's silence corrupts the byte, it is that
> our COP's *presence* does. The coprocessor entered here at the same reset vector as the CPU and its
> stack is not something this project has yet established — that is the next measurement, and it is
> the same open question as ledger #7's default.
>
> ### Resolved the same day: the quantum was the bug, and it was never a Rockbox bug
>
> Giving the coprocessor the instruments the CPU has had all along (`--cop-trace`: a novelty map over
> its own instruction count, and a park/wake ledger carrying both clocks and the PC at each edge)
> made this legible in one run. Apple's coprocessor path, read out of the ROM at `0x8054`:
>
> ```
> 0x8744  cmp r1, #0x55        ; am I the CPU?
> 0x8748  bne 0x8054           ; no -> the coprocessor path
> 0x8054  ldr r4, =0x60007004  ; COP_CTL
> 0x8058  mov r3, #0x80000000  ; PROC_SLEEP
> 0x805c  str r3, [r4]         ; park
> 0x8060  nop / nop
> 0x8068  ldr r0, =0x40000050
> 0x806c  ldr pc, [r0]         ; jump through the entry vector
> ```
>
> And the bootloader's side of it, `--storeaddr=0x40000050` on both paths:
>
> | | writes the vector | value | then |
> |---|---|---|---|
> | retail | `0x400089f0` @101 260 697 | **`0x10000000`** | wakes at `0x400089f8` |
> | cold Rockbox | `0x400089f0` @101 520 797 | **`0x10735a00`** | wakes at `0x400089f8` |
>
> **Both cores are meant to enter the OS at its own entry** and let its crt0 branch on `PROC_ID` —
> which is exactly what `crt0-pp.S` is written to expect (`moveq r3, #WAKE` / `movne r3, #SLEEP`,
> then `ldrne pc, =cop_init`). Two instructions separate the vector write from the wake.
>
> **Our quantum was 1000 instructions**, so the CPU ran on into the OS first, and Rockbox's own
> startup overwrote `0x40000050` **90 instructions after the wake** — `0x10735a18` storing
> `0xe59f027c`, a word of its own code. The coprocessor finally got a turn, read that instruction
> word, jumped to it as an address, and wandered **27 660 256 code buckets**. The retail path has
> the identical race (`0x00084394` clobbers the vector at +1182) and was simply landing on the
> lucky side of it.
>
> The fix is one line of concurrency and no per-guest special case: **a wake ends the running core's
> turn.** A quantum is a claim that nothing observable happens between turns, and across this edge
> that claim is false. With it, the coprocessor parks in **Rockbox's own crt0 at `0x40000040`**, then
> works through `0x10000130`, `0x000001d8` and `0x0008632c` — the core-lock — over **107 park/wake
> edges** where there were 5.
>
> | | ADC `reg 0x2f` last value |
> |---|---|
> | cold Rockbox, `--second-core`, 1000-instruction quantum | `0x89` — channel 68, not a channel |
> | cold Rockbox, `--second-core`, yield on wake | **`0x05`** — channel 2, correct |
>
> So the second core no longer corrupts anything, and the retail path is unmoved: **599 ATA commands
> and 2 916 non-black pixels in both arms.** What cold Rockbox still does not do is draw — 113 ATA
> commands, which is far too few to have loaded `rockbox.ipod`, and that is now the whole of the
> remaining question.
>
> ### Where it stops now: both cores inside the core-lock
>
> It is no longer the battery shutdown. The run ends with the CPU at **`0x00086300`** —
>
> ```
> 00086300  cmp  r4, #0x0
> 00086304  bne  0x00086374
> 00086308  mov  r0, r7
> 0008630c  bl   0x000845dc      ; ldrb r1, [0x60000000]  -- PROC_ID
> ```
>
> — which is Rockbox's **corelock**, the CPU↔COP mutual exclusion, indexing a two-entry array by
> the core id. And the coprocessor is in the same function: `pc 0x00086340`, parking and being woken
> **182 times** at `0x0008632c`, with its last *new* code bucket at cop-instruction 54 895 out of
> 240 620 — so the final 185 000 instructions are all ground it has already covered.
>
> **It is not our interleave.** Three quanta, same wall:
>
> | `--quantum` | ATA | non-black pixels |
> |---|---|---|
> | 1 | 113 | 0 |
> | 16 | 113 | 0 |
> | 1000 | 113 | 0 |
>
> That is worth having: the wake edge *was* a granularity bug and this is not one, so the whole
> class is ruled out rather than suspected.
>
> ### And then the livelock turned out not to exist — 2026-08-19
>
> **Three claims in the paragraph above are wrong, and the controls are what killed them.**
>
> **It is not the corelock.** `0x86300` symbolises against Rockbox's own link map as
> `switch_thread+0x6c`, and `0x845dc` — the call from it — is `corelock_unlock`, not a lock
> acquisition. `objdump` on the ELF shows `86310..86348` is `core_sleep()` inlined: write
> `PROC_SLEEP` to `CPU_CTL` (`r9` = `0x60007000`, from the pool at `+0x180`), then spin on
> `[0x60001000]` — the mailbox — until this core's bit clears. Both cores idling in the scheduler is
> what a booted Rockbox *does*.
>
> **It is not a starved tick, and the control says so.** The cold run reports 13 153 408 interrupts
> asserted against 332 taken, 1 593 halts, 14 336 ms halted in 16 s — which looks alarming until the
> **warm boot that reaches the menu** is measured the same way: 6 683 180 asserted, 228 taken, 771
> halts, 7 036 ms halted in 8 s. Normalised, the two are the same machine — 100 halts/s against 96,
> 21 interrupts/s against 28. The IDE interrupt is `DELIVERED to a handler 0 times, enabled=0,
> pending=1` in **both**, because Rockbox polls the status register. An alarming ratio with no
> control beside it is not a finding.
>
> **And it is not stuck.** Symbolising the whole novelty map, cold enters 347 distinct functions and
> ends at `usb_start_monitoring` → `button_get_w_tmo` → `ata_event` → `ata_spindown` →
> `query_force_shutdown`. `ata_init`, `disk_mount_all`, `disk_mount` and `fat_mount` all run, in
> both arms. It boots, mounts the volume, and sits in the main input wait.
>
> **The panel was the instrument, not the machine.** `--bcm-dump=0xE0000` is where *RetailOS* keeps
> its surface; reading it after a Rockbox boot and finding black says nothing about whether Rockbox
> drew. The write census says it did: **283 480 halfwords and 4 frame updates**.
>
> ### What is actually different, measured
>
> | | address latches | the HI half is written to |
> |---|---|---|
> | warm — reaches the menu | **156** | `0x10002` |
> | cold | **81 718** | `0x10000` |
>
> **That comparison was also bad, and the uncapped instrument says so.** `--watch-range` over the
> whole `0x30010000` window gives the census `--storeaddr` did not: **81 686 writes each from
> `0x4000eb80` and `0x4000eb88`, which are Apple's bootloader**, and 16 byte-writes from Rockbox's
> `0x00083d74`. The 81 718-against-156 gap is a full cold boot compared against a warm entry — it
> is Apple's bootloader existing on one path and not the other, and attributing it to a defect was
> comparing two arms that differ in more than the variable. Twice in one day.
>
> The controlled version of that measurement, same PC in both arms: **`0x00083d74` writes the latch
> 220 bytes warm and 16 cold.** Rockbox's display driver runs 55 times on the path that reaches the
> menu and 4 times on the one that does not.
>
> ### The fork, found by set difference: `lcd_awake` is never called
>
> Symbolising every executed bucket and walking warm's novelty in order, the two runs agree
> instruction for instruction through `switch_thread`, `semaphore_release` and `sleep`, and then:
>
> ```
>  shared     @  24542935  0x00086d60  sleep
>  WARM-ONLY  @  24542941  0x00084290  lcd_awake+0x68
> ```
>
> `lcd_awake` (`0x84228`) is what takes the panel out of sleep, and cold never calls it. Its three
> call sites are `0x668d0`, `0x66a34` and a conditional at `0x837f0`; the first is in
> `backlight_update_state`, and the guard is not a display test at all:
>
> ```
> 668b4:  bl   backlight_get_current_timeout
> 668b8:  cmp  r0, #0
> 668c4:  bge  668d8          ; timeout >= 0 -> skip
> 668cc:  bl   backlight_setup_fade_down
> 668d0:  bl   lcd_awake      ; only on a NEGATIVE timeout
> ```
>
> A negative timeout is Rockbox's "backlight always on". `backlight_get_current_timeout` chooses
> between the plugged and unplugged settings on `power_input_present()` — and **both arms measure
> `r0 = 0` there**, no charger, so that is not the difference either, and neither disk carries a
> `config.cfg` to differ in.
>
> ### Closed: Rockbox inherits a co-processor we do not hand over correctly
>
> **`lcd_awake` IS called on the cold path — it returns early**, which is a correction to the line
> above. It reaches 4 of its code buckets against warm's 34, and the exit is its first guard.
> `lcd_state` is at `0x40005d20` and, with the enum packed to a byte, offset 9 is `display_on` —
> which `lcd-video.c:602` confirms is the first thing tested:
>
> ```c
> if (!lcd_state.display_on && flash_vmcs_length != 0)
> ```
>
> | at the first `lcd_awake` | warm | cold |
> |---|---|---|
> | `lcd_state.display_on` | **0** — proceeds | **1** — bails |
> | `flash_vmcs_length` | `0xbc40` | `0xbc40` — identical, not the gate |
>
> And `lcd_init_device` says in its own comment why:
>
> ```c
> if (GPO32_VAL & 0x4000) {
>     /* BCM is powered.  Assume it is initialized. */
>     lcd_state.display_on = true;
>     tick_add_task(&lcd_tick);
> } else {
>     /* BCM is not powered, so it needs to be initialized. */
>     lcd_state.display_on = false;
>     lcd_awake();
> }
> ```
>
> Measured at `0x70000080`, at the first `lcd_awake` in each arm:
>
> | | `GPO32_VAL` bit 14 |
> |---|---|
> | cold | **set** — Apple's bootloader powered the BCM |
> | warm | **clear** — nothing powered it, so Rockbox does |
>
> **The guest is correct in both arms, and that is the whole point.** On a real cold boot Apple's
> bootloader has powered *and initialised* the co-processor, and Rockbox is entitled to inherit it
> rather than redo the work. Ours reports the power bit honestly and then does not honour the
> inheritance: Rockbox skips its own initialisation, draws 283 480 halfwords and asks for 4 frame
> updates, and nothing reaches the panel. Its display driver touches the address latch 4 times
> where the warm boot — which had to set the co-processor up itself — touches it 55.
>
> **That is a real difference and it is not the cause — corrected an hour later, and this is the
> second wrong root cause in a day.** The co-processor timeline shows cold Rockbox writing
> `0x000e0000  76800 halfwords` followed by `command 0x0`, twice: the same surface, the same command,
> the same full-frame shape the warm boot uses. Frames *do* reach the panel. They are **blank**,
> because Rockbox has nothing to draw. Skipping `lcd_awake` on an inherited co-processor turns out
> to cost nothing, which is what Rockbox's comment says it should cost.
>
> ### The actual cause: Rockbox reads an all-zero partition table
>
> Exact addresses out of the link map, no nearest-symbol tolerance. `init()` calls `disk_mount_all`
> and the return lands at `0x03e80628`:
>
> | | `disk_mount_all()` |
> |---|---|
> | warm | **1** — one volume |
> | cold | **0** — *"No partition found (0)."* |
>
> Rockbox then stops in `apps/main.c`'s error branch: it never calls `settings_load`,
> `settings_apply`, `make_volume_root` or `open_stream_internal` — **0 arrivals at all four against
> warm's 47** — so it never opens a file, and the frames it pushes are the cleared screen.
>
> **And the disk is not the variable, which is the control that had been missing all along.** Both
> arms were being run on *different images*. Swapped:
>
> | | `disk_mount_all()` |
> |---|---|
> | warm entry, on the cold arm's disk | **1** |
> | cold boot, on the warm arm's disk | **0** |
>
> It is the boot path. On the **same** image, at `disk_mount`'s partition loop (`0x6c6b0`, where
> `r2` is the type byte):
>
> | | partition types seen |
> |---|---|
> | warm | `0x00`, **`0x0c`** — FAT32 with LBA, accepted |
> | cold | `0x00`, `0x00`, `0x00`, `0x00` |
>
> After Apple's bootloader has finished with the drive, **Rockbox's own read of the partition table
> comes back as zeros.** `disk_init` still returns 1 in both and `fat_mount` still returns -42 in
> both, so nothing reports an error — the table is simply empty. That is state the bootloader leaves
> in the storage path, and it is the same shape of bug as the co-processor one without being that
> bug: a device our model hands over in a condition the next operating system cannot use.
>
> ### It draws its own splash, and the storage path is right up to one contradiction
>
> **Cold Rockbox is not blank** — that reading came from an end-of-run dump. Filmed, it puts up the
> Rockbox logo. The display path is entirely fine, which retires the co-processor theory for good.
> What it does after the splash is Rockbox's documented dead end, `apps/main.c`:
>
> ```c
> rc = disk_mount_all();
> if (rc <= 0) {
>     lcd_putsf(0, line++, "No partition found (%d).", rc);
>     lcd_puts(0, line++, "Insert USB cable");
>     ...
>     disk_set_sector_multiplier(DEFAULT_VIRT_SECTOR_SIZE/SECTOR_SIZE);
>     usb_start_monitoring();
>     while(button_get(true) != SYS_USB_CONNECTED) {};      /* forever */
> }
> ```
>
> Every function in cold's tail — `usb_start_monitoring`, `button_get`, `button_get_w_tmo`,
> `usb_inserted`, and `disk_set_sector_multiplier(4)` at `@124 621 553`, *after* the mount — is on
> that branch. It is waiting for a USB cable that will never arrive.
>
> ### The contradiction, and it is one line wide
>
> Run against the **same disk image** on both paths, every step of the mount agrees:
>
> | | cold | warm |
> |---|---|---|
> | `ata_read_sectors(0, 0, 1, 0x127d10)` | same args | same args |
> | MBR buffer sum after the read | `0xe5f` | `0xe5f` |
> | partition-table sum (`+0x1be`, 64 bytes) | `0xcc0` | `0xcc0` |
> | boot signature at `disk_init+0x64` | `0xaa55` | `0xaa55` |
> | value written to `partinfo[1].type` by `0x6be18` | **`0x0c`** | **`0x0c`** |
> | type byte read at `disk_mount+0x100` (`0x6c6b0`) | **`0x00` × 4** | `0x00`, then **`0x0c`** |
> | `disk_mount_all()` | **0** | **1** |
>
> **The parse writes `0x0c` and the loop that reads it back sees `0x00`, on the cold path only** —
> and `--storeaddr` over `0x000e7308` records exactly two writes in the whole run, the BSS memset at
> `@108 197 001` and the parse at `@124 542 344`, with the loop reading at `@124 544 932`. Nothing
> overwrote it. So the loop is not reading `0x000e7308`: `disk_mount`'s pool at `0x6c7b8` holds
> `0x000e72e0`, the same array `disk_init` fills, and `r5` starts at `+16` — so the question is what
> `r5` advances by, and why four reads on the cold path never land on entry 1 when two on the warm
> path do.
>
> That is the next thing to read, and it is a register rather than a hypothesis.
>
> The four things ruled out on the way, each by a control rather than by argument: `disk_init`
> returns **1** in both · `fat_mount` returns **-42** in **both**, so it fails even on the path that
> reaches the menu · `disk_mount_all` and `disk_mount` are entered once each with identical
> registers · `power_input_present()` is **0** in both.
>
> **One measurement toward it, taken the same day.** On a cold boot with `--second-core`, the
> coprocessor ends 400 M instructions at **`pc 0x00000734`**, having run 291 612 061 of its own and
> slept and woken four times. `0x734` is in **Apple's boot ROM**, not in Rockbox — so on the cold
> path our COP spends the whole run executing Apple's startup code beside the CPU, and it is
> executing it *as the CPU would*: the same instructions set the same stack pointer, because they
> are the same instructions. Two cores writing one stack is a mechanism that would produce exactly
> the byte-shaped corruption above without anything else being wrong. What Apple's ROM does with a
> core that reads `PROC_ID` as `0xAA` — and where it expects that core's stack to be — is the thing
> to read next, and this project has the disassembly to read it.
 Everything measured on the cold path before this is fixed is measured through it — R4.
## 2026-08-18, later still: the menu's font was never on the volume the recipe mounts

The shipped screenshot of Rockbox's menu renders in the compiled-in 8 px sysfont, not in the
`15-Adobe-Helvetica` that `apps/settings_list.c:328-368` asks for at `LCD_HEIGHT <= 240`. Two
explanations were open: the font never reached the volume, or it is on the volume and our FAT32 /
ATA path hands back the wrong bytes. The second would have been the valuable one — at 231 928 bytes
it is by far the largest file Rockbox opens at boot, it spans 57 clusters, and a cluster-chain bug
would surface there and nowhere else.

**It is neither. The FAT32 path is correct and the recipe mounts the wrong disk.**

### `rockbox.sh` defaults to a volume with no `.rockbox` on it at all

`DISK` defaults to `resources/drives/ipod8g.img`, which is a stock Apple volume:

```
$ IMG=resources/drives/ipod8g.img ipod-boot fat DISK.img find rockbox
# ipod8g.img: FAT32 type 0xc at LBA 32768, 8 sectors/cluster, data starts at LBA 65536
                                                              (nothing)
$ IMG=resources/drives/ipod8g.img ipod-boot fat DISK.img find ipod_control
/iPod_Control                                DIR  lba 65560..65567
/iPod_Control/Device                         DIR  lba 65568..65575
…9 entries
```

The second command is the control, and R13 is why it is there: an absence reported by a tool nobody
has watched succeed on that data is not a measurement. The tool finds things in this image. It finds
no `.rockbox` because there is none.

With no `/.rockbox/fonts/`, the font load fails and Rockbox falls back to the sysfont it carries.
Nothing reports it — a themeless install is an ordinary condition for Rockbox, not an error — so the
failure is silent by design and not by defect.

**And the shipped still is exactly that run.** `rockbox.sh`'s own flags, its own default disk,
nothing added, filmed at a 2 M cadence:

```
BUDGET=1200000000 trace --osos=rb-main.raw --boot-osos --flash=… \
  --disk=resources/drives/ipod8g.img --sysinfo --bcm --pmu \
  --bcm-film=0xE0000:140:F0:2M:_out/film/asshipped
```

Frame 5 of that film — up from 48 M to 166 M instructions — differs from
`docs/media/ipod-14-rockbox-menu.png` in **0 pixels of 76 800**.

### The row pitch, measured

Not eyeballed. The ink profile of the label column, with the gap rule stated as a number: a row with
**0** lit pixels above the background is a gap, and the pitch is the spacing between gaps.

| run | volume | gap rows | pitch |
|---|---|---|---|
| the shipped still | `ipod8g.img`, no `.rockbox` | 31, 39, 47, 55, 63, 71, 79, 87 | **8 px, 7 of 7 gaps** |
| `put-files` | our own installer's volume | 38, 50, 68, 83, 98, 113, 128, 143, 155 | **15 px, 5 consecutive gaps** |
| a mounted copy | `ipod8g-rockbox.img` | 38, 48, 64, 79, 94, 109, 124, 139, 153 | **15 px, 5 consecutive gaps** |

15 is `DEFAULT_FONT_HEIGHT`. The instrument's control is printed beside every answer: the busiest
row in each strip carries 33–102 lit pixels, 33× to 102× the gap rule, so a gap and a glyph row are
nowhere near each other on this measure. The naive version of this — "a band of lit rows is a
row" — reports the shipped still's pitch as **55**, because two rows of an 8 px font touch and merge
into one band. That is R5: the measure has to be matched to what it is measuring.

### The FAT32 writer is exonerated, with the bytes to prove it

`put-files` puts the font on the volume, and it reads back correct:

```
$ ipod-boot put-files disk.img <the 4.0 zip, unpacked>
  381 file(s) in 23 directory(ies), 19298918 bytes
$ IMG=disk.img ipod-boot fat DISK.img find helvetica
/.rockbox/fonts/15-Adobe-Helvetica.fnt   size=231928  lba 6598080..6598535 (57 clusters)
$ IMG=disk.img ipod-boot fat DISK.img cat /.rockbox/fonts/15-Adobe-Helvetica.fnt out.bin
$ shasum -a 256 out.bin  <the zip's own copy>
8a7ff4b0…  out.bin
8a7ff4b0…  .rockbox/fonts/15-Adobe-Helvetica.fnt
```

Identical across all 57 clusters, read back by the independent reader — and Rockbox agrees, because
booted on that volume it draws the themed background, the colour icons and 15 px rows. The 232 KB
multi-cluster read this was supposed to break on does not break. `rockbox.ipod` on the same volume
is more demanding still and also correct: 187 clusters spread from LBA 1 660 696 to 6 565 959, so
the chain is followed across a fragmented file and not merely across a contiguous one.

*(`ipod8g-rockbox.img` holds the same font at `/.rockbox/FONTS/` — uppercase, because macOS's FAT
driver wrote a bare 8.3 name with no long-name entry. It renders identically, so the case is not a
factor.)*

### One condition, and it is not optional

Rockbox **writes** to a volume that has `.rockbox` on it, and this emulator's ATA is read-only
unless asked. Without `--disk-writable` the boot panics long before it loads a font:

```
*PANIC* (4.0)
dc_writeback_callback() - Could not write sector 8908074 (error -53)
```

That is at ~20 M instructions. So a run against an installed volume needs `--disk-writable`, and
`rockbox.sh` does not pass it. On the *stock* volume the flag makes no difference at all — the two
runs are identical frame for frame and digest for digest, 2 253 ATA commands each — because there is
nothing there for Rockbox to write to. Which is why the flag's absence never showed up until a
volume with files on it was put under it.

**The reproduction, end to end:**

```
cp -c resources/drives/ipod8g.img /tmp/rb.img
unzip -q resources/vendor/rockbox/bin/rockbox-ipodvideo-4.0.zip -d /tmp/rbzip
ipod-boot put-files /tmp/rb.img /tmp/rbzip
DISK=/tmp/rb.img tools/ipod-boot/rockbox.sh --disk-writable      # + --bcm-film to record it
```

`docs/media/ipod-14-rockbox-menu.png` is now a frame from that run. The re-encoded still is
pixel-identical to the frame the machine produced — 0 of 76 800 — because what has to be checked is
what ships, not what was on disk before ffmpeg touched it.

## 2026-08-18: the yellow dashes are the gif encoder, and the raw frames say so

`docs/media/ipod-15-rockbox-wheel.gif` shows yellow tick marks at a 24 px period along the rows of
the selection bar. Two explanations were open, and the still could not separate them, because a
still is a settled screen and the dashes are on frames captured mid-scroll: either ffmpeg's
partial-frame differencing in the gif encode, or our own `lcd_update_rect` going wrong during a
redraw.

**It is the encoder, and it is not a judgement call — the gif convicts itself.**

### The raw frames are solid, including the ones caught mid-redraw

The panel was filmed under wheel input at a 200 k cadence, then again at **5 000** instructions per
sample across a single redraw. That is fine enough that the film's dedup keeps transient pictures,
held for one or two samples where a settled screen is held for thousands — `frame-00001` is up for
5 000 instructions and `frame-00004` for 10 000. Those are redraws in flight; one of them has row 21
drawn from x=56 rightwards and no further, which is what a partial update looks like when you catch
one.

Row 23, on every raw frame that carries the bar:

```
frame-00000  row 23:  1 yellow run, 320 px   0..319(320)
frame-00001  row 23:  1 yellow run, 320 px   0..319(320)      <- mid-redraw
frame-00006  row 23:  1 yellow run, 320 px   0..319(320)      <- mid-redraw
frame-00007  row 23:  1 yellow run, 320 px   0..319(320)      <- mid-redraw
```

Never dashed, at either cadence, settled or in flight. Two controls sit beside that, because an
instrument which has only ever answered "one run of 320" has not been shown able to answer anything
else: rows 20–22 of the same frames report **7–8 runs totalling ~305 px** — the black label glyphs
punched through the yellow bar, which is correct rendering and a genuinely interrupted run — and
frames where the selection has moved away report **0 runs, 0 px**. The measure can say solid, can
say broken, and can say absent.

### The gif was made from these exact frames

The gif's keyframe differs from the emulator's raw frame in **0 pixels of 76 800**. So the input to
the encode was clean, and anything the gif shows that the frame does not is the encode's.

### What the gif actually stores

```
logical screen 320x240, global palette 32 entries
frame 0  rect 0,0   320x240  disposal 1  transparent no
frame 1  rect 0,16  320x24   disposal 1  transparent yes idx 0
frame 2  rect 0,32  320x24   disposal 1  transparent yes idx 0
frame 3  rect 0,48  320x32   disposal 1  transparent yes idx 0
```

A keyframe and three difference bands, each with a transparent index and disposal 1 — *leave what is
underneath*. That is ffmpeg's `-gifflags +transdiff`, which is on by default.

Decoded, stored frame 1 writes exactly one colour on row 23 — `#000000` — and leaves **26 pixels
transparent**:

```
transparent pixels on row 23: 22 23  46 47  70 71  94 95  118 119  142 143  166 167
                              190 191  214 215  238 239  262 263  286 287  310 311
spacing:                      24
the keyframe's colour under every one of them: (189,154,16)     <- the bar's yellow
```

So the composited frame reads **13 yellow runs totalling 26 px** where the raw frame reads one run
of 320, and every one of those 26 pixels is the keyframe showing through a hole the encoder punched.
The emulator drew the row black. The gif kept it yellow, at a 24 px comb.

### What was not settled, and it is worth naming

**Which ffmpeg invocation produced it.** Six were run against the same proven-solid frames —
default, no palette filter, `max_colors=32`, `dither=bayer` at 32 and at 256 colours, and
`diff_mode=rectangle` — and every one writes a 256-entry palette and a clean row 23. The shipped
file has a **32-entry** palette, which none of them produce, so the command line is not recoverable
from the artifact. That is a gap in the account of *how*, not in the account of *who*: the input
frames are byte-identical to ours and the artifact is made of transparency the input does not have.

**The remedy is verified rather than assumed.** `-gifflags -transdiff` turns `transparent yes` into
`transparent no` on every frame — measured across that sweep, not read off a manual — and an
artifact made of transparency cannot survive a file that has none.
`ipod-film asset`'s `publish()` does not set it. Its own films happen not to show
this because they are 256-colour with `dither=none`, which is a mitigation and not a defence.


## 2026-08-18, later still: the cold boot stops powering off, and starts stalling

**The ADC bug is fixed, and not by anything aimed at it.** Retiring the clock's sleep teleport —
a halted core now costs a loop iteration instead of jumping to the next deadline — fixed it as a
side effect:

| | `reg 0x2f` writes | last value |
|---|---|---|
| before the clock change | 9 240 | `0x00` |
| after | 5 | **`0x05`** |

`0x05` is `(2 << 1) | 1`, channel 2, which is what the guest asked for all along. So the byte that
was "written 0x05 and read back 0x00 seventy-two instructions later" was **a consequence of the
teleport**: jumping time in the middle of `pp_i2c_send` let something land between the spill of
`data[1]` and the copy into `I2C_DATA(1)` in a way that cannot happen on hardware, where time does
not move in steps of ten milliseconds between two adjacent instructions. Every hypothesis about a
lost byte, an alias, or a clobbered stack was chasing a symptom of the clock.

**`sys_poweroff` is now never called.** `--enterlog=0x00068450` reports **0 arrivals** across a
600 M cold boot, against the 315 calls that opened this whole investigation, and `_battery_voltage`
runs exactly once, at `@124 522 783`.

### What replaces it, and it is a different question

The panel draws the splash and then goes to **zero non-black pixels** at `@124.6 M` and stays there.
It is not a shutdown and not a hang: `--novelty` finds new code still appearing at `@353 699 369`,
and the machine is executing rather than halted. Where it executes, on an 800 M cold boot:

| share | address | symbol |
|---|---|---|
| 18.3 % + 12.2 % | `0x40005520` / `0x40005510` | **not in Rockbox's ELF** |
| 13.3 % + 7.4 % | `0x4000e750` / `0x4000e740` | **not in Rockbox's ELF** |
| 6.0 % + 6.0 % | `0x0007e8b0` / `0x0007e8a0` | **`usb_reset_controller +0x9c` / `+0x8c`** |

Two things to take from it. **Around 70 % of the time is spent in `0x4000xxxx` code that has no
symbol in `rockbox.elf`** — the nearest preceding symbol is megabytes away, so these are not
Rockbox's IRAM functions however much the address range looks like it. And **12 % is a two-address
spin inside `usb_reset_controller`**, on a machine where [ROADMAP](../ROADMAP.md) records USB as
*"nothing modelled beyond a clock-ready bit"*.

### Whose code `0x40005510` is: nobody's

**It is not code.** `rockbox.elf`'s section map answers it in one command, and the answer is that
the addresses holding 70 % of the run are data:

```
.ibss         vma 40000000  size 68c4     ->  40000000..400068c4
.iram         vma 400068c4  size 25fc     ->  400068c4..40008ec0
.idle_stacks  vma 40008ec0  size 0100
.stack        vma 40008fc0  size 2000     ->  40008fc0..4000afc0
```

| address | share | what it is |
|---|---|---|
| `0x40005510` / `0x40005520` | 30.5 % | **`.ibss`** — `downmix_buf +0x5f0`, an audio buffer |
| `0x4000e740` / `0x4000e750` | 20.7 % | **past the end of every section**, `stackend +0x3780` |
| `0x40009cf0` | 7.0 % | `.stack` / past it |

**The cold-booted machine is executing BSS and running off the end of IRAM.** The earlier framing —
"70 % in code with no symbol" — was too generous: the addresses are inside the ELF's address space
and they are not code at all. My first symbol lookup asked only for *function* symbols, which is why
they came back unresolved and looked like somebody else's binary.

It does not fault because that memory is **mapped**, and an ARM word of `0x00000000` decodes as
`andeq r0, r0, r0` — a no-op. So the PC walks forward through zeroes indefinitely. That is also why
`--novelty` kept reporting new code as late as `@353 699 369` and why nothing draws: the novelty
counter is watching a program counter stroll through an empty buffer.

**This is a `Lost` that the machine cannot detect**, because `Lost` fires on an *unmapped* fetch and
IRAM is mapped for its whole length. Worth considering as a real instrument gap: a fetch from a
region the loaded image declares as `.bss` or `.stack` is a fault in every sense that matters, and
nothing reports it.

**Settled when** the cold path reaches the menu the warm path reaches. The next question is no
longer "whose code" but **where the PC leaves real code**, which is one `--callgraph` or a watch on
the last `.iram` address before the walk begins.

---

## Cold-booted, Rockbox spins in its core-lock — and it is not the disk

**Measured 2026-08-19.** Rockbox warm-entered through `--osos=rb-main.raw` reaches its main menu,
takes wheel input and navigates four levels deep. Cold-booted — Apple's boot ROM running Rockbox's
own bootloader out of the firmware partition — it draws **nothing at all**:

```
Running 'osos' 0 from 0x10735A00
bcm: 6 commands kicked, 4 frame updates
320x240 from 0x000e0000, 0 non-black pixels
ata commands: 99
```

**The disk was the suspicion, and it is ruled out.** That run is on a drive this project's own
installer had just built from a plain Apple image — `ipod-boot rockbox-install`, bootloader into the
firmware partition and 381 verified files onto the volume — and it behaves exactly like the older
`rockbox-cold.img`. Two independently built drives, same result. The difference is **cold versus
warm**, not the volume.

**Where it stops is specific.** The halt is a two-instruction spin at `0x00086300`, reached from
`0x000845dc`:

```asm
000845dc  mov  r1, #0x60000000
000845e0  ldrb r1, [r1, #0x0]        ; PROC_ID — 0x55 on the CPU, 0xAA on the COP
000845e4  mov  r2, #0x0
000845e8  strb r2, [r0, r1, lsr #7]  ; a per-core slot: 0x55>>7 = 0, 0xAA>>7 = 1
...
00086300  cmp  r4, #0x0
00086304  bne  0x00086374            ; spin
```

Indexing a two-entry array by `PROC_ID >> 7` is Rockbox's **core-lock**, and this note already
records `corelock` as one of the two cold-only stack writers. So cold-booted Rockbox is waiting on
the second core, and 99 ATA commands is the bootloader's reading and almost none of Rockbox's own —
it never gets as far as scanning the volume.

**What this does not say.** Whether the COP is under-modelled, whether the bootloader hands over in
a state the warm path fakes, or whether `--sysinfo`'s handoff is doing the work in the warm case, is
not established. All three are consistent with what is above, and the same spin address appears on
every cold run tried, which makes it a good place to put a `--enterlog` next.

**And the aperture theory is dead.** The obvious guess after diagnostics — that Rockbox drives the
co-processor at a second address nobody had mapped, as `diag` does at `0xb0000000` — is wrong:
`rb-bootloader.raw` references `0x30000000` twelve times and `rb-main.raw` nine, and **neither
mentions `0xb0000000` at all**. Cheap to check, and it saved chasing the wrong thing.

### Doom stops in the same place, and the two problems are one

**Measured 2026-08-19.** Rockbox's Doom launches: the wheel navigates four levels down —
`Plugins` → `Games` → `doom` — the plugin starts, and it reads the drive hard. **4 023 ATA
commands, 82 frame updates, and `Loading…` on screen.** Then, at 7.83 G executed instructions and
**320 seconds of simulated time**, it is sitting on:

```asm
000845dc  mov  r1, #0x60000000
000845e0  ldrb r1, [r1, #0x0]        ; PROC_ID
000845e8  strb r2, [r0, r1, lsr #7]  ; the two-entry per-core array
00086300  cmp  r4, #0x0
00086304  bne  0x00086374            ; spin
```

Byte for byte the address a cold-booted Rockbox halts at. **So the cold-boot black panel and Doom's
unfinished load are not two problems.** They are one, and it is named in this emulator's own source:

> *"The PP5021 is dual-core (CPU + COP) … Report CPU, and report the COP already asleep."*

`map_hardware` seeds `PROC_ID` with `0x55` and reports the coprocessor permanently asleep, because
there is one interpreter. That is exactly right for RetailOS, which uses the mailbox and never
dispatches to a second core here — and it is a dead end for **any firmware that hands work to the
COP and waits for it**. Rockbox's stock build is a dual-core build. Its core-lock is waiting for a
processor that does not exist, and 320 seconds of simulated time is long enough to be certain it is
spinning rather than slow.

**What that costs, stated plainly:** cold-booted Rockbox, and Doom past its load. Warm-entered
Rockbox is unaffected — it reaches its menu, takes wheel input and navigates — so this is a specific
path rather than a general failure.

**What it would take:** a second interpreter over the same bus, with `CPU_CTRL`/`COP_CTRL` sleep and
wake and the `0x60001000` mailbox already modelled (research/15) doing real work. That is a feature,
not a fix, and it belongs with the other 1.0 items rather than being attempted in passing.

### The second core, and what it is actually built from

**2026-08-19.** `--second-core` runs the PP5021's coprocessor: a second register file over the same
bus, interleaved with the CPU in fixed quanta. Off by default, and that default is the control — a
retail boot with it off is **146 733 702 instructions and 102 ATA commands**, byte for byte what it
was before any of this existed.

**Almost all of it is read, not guessed.** The handshake is in `crt0-pp.S`, vendored here:

```asm
    ldr    r0, =PROC_ID
    ldrb   r0, [r0]
    cmp    r0, #0x55
    ldrne  r2, =COP_CTRL        ; the COP puts ITSELF to sleep
    movne  r1, #SLEEP
    strne  r1, [r2]
    ldreq  r2, =COP_STATUS      ; the CPU waits for it, at the SAME address
1:  ldreq  r1, [r2]
    tsteq  r1, #COPSLEEPING
    beq    1b
```

So `COP_CTRL` and `COP_STATUS` are one register, `sleep_core` writes `0x80000000` and `wake_core`
writes `0` (`system-target.h`), and both cores enter at the reset vector and sort themselves out on
`PROC_ID`. Values, addresses and per-core interrupt banks all come from `pp5020.h`.

**Two things are modelled and one is deliberately not.** `PROC_SLEEP` and wake-on-interrupt are;
`PROC_WAIT_CNT` and the counter-source bits are not, and that is a scoped omission rather than an
oversight — Rockbox uses them only for timed stalls around frequency changes
(`system-pp502x.c:338`, `:664`), which this emulator does not model either.

**The one invented number is the quantum, and it was measured rather than asserted.** Nothing on
hardware corresponds to it: the two cores run at once. So `--quantum=N`, and a sweep:

| quantum | COP instructions | COP ends | panel | ATA |
|---|---|---|---|---|
| 50 | 26 539 | asleep at `0x8632c` | 74 057 | 3 953 |
| 1 000 | 112 499 | asleep at `0x8632c` | 74 057 | 3 953 |
| 20 000 | 72 480 069 | **awake at `0x2e0`** | 74 057 | 3 953 |

**The visible outcome is identical across a 400× range** — same panel, same ATA count — which is
what the default's doc comment claimed and had not shown. The COP's own trajectory is *not*
identical: at 20 000 it runs 640 times as far and ends awake in the exception vectors, so a large
quantum lets it past the point it should have parked at. Small is safer, and the reason is now
recorded instead of assumed.

### What it unblocks, and what it does not

**Warm Rockbox: the coprocessor is a participant.** 149 323 instructions inside Rockbox's own code
at `0x8633c`, menu intact, 3 953 ATA. Doom launched, ran, and returned to the menu with **259 frame
updates** against 82 single-core — so the core-lock that swallowed both is no longer swallowing them.

**Cold boot is worse, not better, and it is out of scope until Apple's bootloader is read.** With
the second core on, a cold boot resets six times and the COP ends in the vector table; the CPU
stops issuing ATA commands entirely. Apple's bootloader has its own `PROC_ID` branch and this
project has never disassembled it. Rockbox's COP path is open source and was read; Apple's is not,
and inventing it is exactly what this note exists to avoid.

### Apple's own coprocessor handshake, disassembled

**The second core was half-guessed until this was read.** Apple's bootloader does branch on
`PROC_ID`, at file offset `0x8738` in the retail NOR — found by scanning for `cmp rN, #0x55` rather
than for the address, because the address is built with `mov` rather than loaded from a pool:

```asm
00008738  mov  r0, #0x60000000      ; PROC_ID
0000873c  ldr  r1, [r0, #0x0]       ; a WORD read, masked — not `ldrb`
00008740  and  r1, r1, #0xff
00008744  cmp  r1, #0x55
00008748  bne  0x00008054           ; not the CPU: the coprocessor's path
```

and the coprocessor's path is seven instructions:

```asm
00008054  ldr  r4, =0x60007004      ; COP_CTRL
00008058  mov  r3, #0x80000000      ; PROC_SLEEP
0000805c  str  r3, [r4, #0x0]       ; park myself
00008060  nop
00008064  nop
00008068  ldr  r0, =0x40000050      ; on wake, take my entry point from IRAM
0000806c  ldr  pc, [r0, #0x0]
```

So **both cores run from the reset vector**, the COP parks itself three instructions after the
branch, and when the CPU wakes it, it jumps to whatever address the CPU has left at **`0x40000050`**.
That is the mechanism, and none of it had to be guessed.

**It also found a real bug in this emulator, which the guess had hidden.** Apple reads `PROC_ID`
with `ldr` and masks; Rockbox reads it with `ldrb`. The hook answering it lived only on the byte
path — and `Memory::read32` has a fast path that never reaches `read8`. So the coprocessor read
`0x55`, concluded it was the CPU, and ran Apple's bootloader concurrently with the real one.

That is exactly what the first cold-boot attempt looked like from outside: **six `Running 'osos'`
lines in one boot** and the CPU issuing no ATA commands at all. `Memory::core_register` now answers
both registers at both widths, and the same cold boot is a single clean boot:

```
Retail mode
Running 'osos' 0 from 0x10000000
second core: 111 501 639 instructions, pc 0x4001d8f0, awake — slept 2x, woken 2x
ata commands: 99            (102 single-core)
```

**Slept twice, woken twice** is the number that matters: not a wake storm from a mask we got wrong,
but RetailOS parking and dispatching to a coprocessor deliberately, twice, and running 111 M
instructions of real work on it. **RetailOS uses the second core, and now it has one.**

### Doom runs, and stops on a file rather than a feature

With the coprocessor, Rockbox's Doom gets past the core-lock and starts:

```
Z_Init: Init zone memory allocation daemon...
Z_Init: Allocated 61254kb zone memory.
M_LoadDefaults: Load system defaults.
              Missing Base WAD!
```

That is not an emulator limit. `rockdoom.c:294` needs `/.rockbox/doom/rockdoom.wad` beside the game
WAD, and `fileexists` there returns **0 on success**, so a missing file makes `Dbuild_base` return 0
and the caller splash. Our drive has `doomf.wad`, which *is* one of the seven the plugin accepts —
it is Freedoom, `wads_builtin[4]`. Only the base WAD is absent, and it is a prebuilt asset rather
than something the source tree generates: the URL in `i_video.c`'s comment
(`alamode.mines.edu/~kkurbjun/rockdoom.wad`) is long dead and `download.rockbox.org/useful_files/`
answers 404.

So Doom is blocked on **content this project does not have**, having been blocked on a missing
processor an hour earlier. Those are very different kinds of blocked, and the difference is the
whole point of writing it down.

### The base WAD — and a retraction, because the first attempt was a picture of a failure

`rockdoom.wad` is a **PWAD**, and `Dbuild_base` only asks whether the file opens
(`rockdoom.c:294`, `fileexists` returning 0 on success). A twelve-byte empty PWAD satisfies that
check and gets as far as Doom's own menu — `Game · Addons · Demos · Options · Play Game · Quit`.

**That was written up here as "and it plays". It does not, and the correction is the finding.**
Pressing `Play Game` on the empty PWAD produces:

```
W_GetNumForName: TANGTABL not found
R_LoadTrigTables: Invalid TANGTABL
W_ReadLump: only read 12 of 3143200 on lump -1
```

A menu is not a game. `rockdoom.wad` is not a formality that the loader merely stats — it carries
lumps the renderer reads at level start, and `tables.c:2150-2170` names three of them with their
exact sizes:

| lump | bytes | what it is |
|---|---|---|
| `SINETABL` | 40 960 | `finesine`, 10 240 × `fixed_t` |
| `TANGTABL` | 16 384 | `finetangent`, 4 096 × `fixed_t` |
| `TANTOANG` | 8 196 | `tantoangle`, 2 049 × `angle_t` |

Each is checked with `W_LumpLength(lump) != N` and `I_Error`s on mismatch, so nothing approximate
gets past it.

**And "both published copies are gone" was also wrong.** The URL in Rockbox's own source
(`i_video.c:70`, `m_menu.c:31` — `alamode.mines.edu/~kkurbjun/rockdoom.wad`) is dead, and so is
`download.rockbox.org/useful/`. The **wiki attachment is live**:
`rockbox.org/wiki/pub/Main/PluginDoom/rockdoom.wad` — 285 048 bytes, `PWAD`, **186 lumps**, all
three tables present at exactly the sizes above. Two dead links were reported as a census of the
internet; a third URL was all it took.

### What Doom actually needs, from Rockbox's own manual

Two files in `/.rockbox/doom/`, and they do different jobs:

| file | what it is | where it came from here |
|---|---|---|
| `rockdoom.wad` | the base PWAD, *"based on `prboom.wad` from prboom-2.2.6"* | the Rockbox wiki attachment, sha256 `303f5ea5…` |
| a game IWAD | one of `doom1` · `doom` · `doomu` · `doom2` · `doomf` · `plutonia` · `tnt` (`rockdoom.c:207-213`) | **Freedoom 0.13.0** `freedoom2.wad` installed as `doom2.wad` |

Freedoom is not a substitute anybody here invented: `manual/plugins/doom.tex` says *"A free
alternative for Doom 2 is FreeDoom … This can be used in place of `doom2.wad`"*. It is a real
`IWAD` (28 787 748 bytes, magic checked at `d_main.c:546`) under a BSD licence, which is what makes
it the one that can be named in a public repository's documentation.

**The coprocessor is doing the work.** On the run that reaches the menu it parked and was woken
**18 554 times** — Doom hands it work constantly, which is exactly the traffic the core-lock spin
was standing in for when there was no second core to hand anything to.
