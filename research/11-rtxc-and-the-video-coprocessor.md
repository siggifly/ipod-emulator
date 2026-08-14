# What RetailOS actually is, and why the screen has never lit up

**2026-08-12.** Two findings that reshape the remaining roadmap. Both came from published sources,
neither from experiment.

---

## 1. RetailOS's kernel is RTXC 3.2 — and its syscall ABI is documented

Source: [freemyipod's RetailOS page](https://freemyipod.org/wiki/RetailOS). It is architecture-general
rather than S5L-specific, so it applies to the PortalPlayer 5.5G.

RetailOS is a single flat binary running entirely in ARM system mode with no privilege separation.
Its kernel is **RTXC 3.2**; the UI layer is built on Pixo IP.

**Every kernel service goes through one dispatcher.** The caller's saved **`R0` points at a
serialized request struct** — `{syscall_number, args...}`. Service wrappers build the request, call a
state-saving trampoline that disables interrupts, and enter the dispatcher.

Documented syscall numbers include:

| | | | |
|---|---|---|---|
| `KS_pend` `0x03` | `KS_receive` `0x05` | `KS_enqueue` `0x0c` | `KS_dequeue` `0x0d` |
| `KS_lock` `0x0e` | `KS_unlock` `0x0f` | `KS_alloc_timer` `0x10` | `KS_start_timer` `0x12` |
| `KS_delay` `0x14` | `KS_execute` `0x15` | `KS_deftask` `0x16` | `KS_alloc_task` `0x17` |
| `KS_suspend` `0x19` | `KS_yield` `0x1c` | `KS_waitm` `0x22` | `KS_defqueue` `0x2e` |

And **named semaphores**, several of which bear directly on our symptom — nothing runnable, no
`READ SECTOR`, display untouched:

| Semaphore | | Semaphore | |
|---|---|---|---|
| `S_BLOCKDEVICE` | `0x36` | `S_DISPLAY` | `0x38` |
| `S_BLOCKDEVICEQ` | `0x37` | `S_VBL` | `0x2e` |
| `S_ATAWRKLPRDY` | `0x34` | `S_VSYNC` | `0x3b` |
| `S_HDDSCANCOMP` | `0x24` | `S_GRAPHMGR` | `0x2d` |
| `S_DISKMGRQ` | `0x06` | `S_I2C_DONE` | `0x3a` |

Mailboxes: `M_DISKMGR 0x01`, `M_BLOCKDEVICE 0x04`, `M_DISPLAY 0x05`, `M_GRAPHMGR 0x03`.
Resources: `I2C_MASTER 0x05`, `PMU_LOCK 0x11`, `ADC_LOCK 0x12`, `BACKLIGHT 0x14`.

### The tool this implies

**Hook the dispatcher.** One address, logged, reconstructs the entire live system: task creation
(`KS_deftask`/`KS_execute`), what every task blocks on (`KS_pend` plus the semaphore number),
mailbox traffic, timers. Cross-referenced against the tables above it yields **named** blocking
reasons — "the block-device task is pended on `S_BLOCKDEVICE`" rather than "something is asleep".

This is strictly better than the task-enumerator-by-string-scan plan (§below), and it is one hook
rather than a structure-recovery exercise. Finding it: the function that runs with interrupts
disabled, reached through a state-saving trampoline, that reads a struct pointer from saved `R0`.

**Caveat the wiki gives itself:** the RTXC manuals contain example code but **no struct
definitions**. freemyipod recovered those by cross-referencing publicly available RTXC binaries that
shipped with debug symbols.

RTXC references: [kernel services (archived)](https://web.archive.org/web/20230218212424/https://datasheet.datasheetarchive.com/originals/library/Datasheets-AS2/DSAAXSA0003458.pdf) ·
[training manual](https://archive.org/details/manualzilla-id-5752851)

### A cheaper corroborating trick, already verified

Our OSOS is a **debug build**: 13,762 strings, including RetailOS's own task names —
`DiskReaderTask`, `USBDeviceTask`, `TimerTaskClass`, `RtcTaskClass`, `TrackCacheReadTask`,
`SearchHelperThread`, `VCUpdateTask`. `DiskReaderTask` sits at `0x1777b4`.

A task control block that carries a name points at that string, so scanning RAM for `0x001777b4`
locates TCBs directly. `--findptr=VALUE` does this.

### Measured, and it moves the problem earlier

| String | Address | Pointers found |
|---|---|---|
| `DiskReaderTask` | `0x1777b4` | **none** |
| `TimerTaskClass` | `0x184000` | **none** |
| `USBDeviceTask`  | `0x17107c` | **none** |
| `RtcTaskClass`   | `0x148d44` | **none** |

**Positive control** — the instrument works. Searching for our own `sysinfo` pointer `0x4001fd00`
finds it at `0x4001ff1c` where we wrote it, *plus two copies the firmware made for itself*
(`0x009830ec`, `0x4000608c`). A scanner that finds those and not the task names is reporting a real
absence.

So ~~**no named task is ever created.**~~ RetailOS is idling *before* it spawns its subsystems, not
after — which is a materially different problem from "a task is blocked on a semaphore nobody
posts". Early hardware init does run (ATA `IDENTIFY`, the I²C transactions, the timer tick), so the
kernel is alive; the application-level task creation never happens.

That raises the value of the dispatcher hook again: `KS_deftask` / `KS_execute` calls are exactly
what we would expect to see and do not.

> **WRONG — retracted 2026-08-14 by the research/20 Addendum 14 audit.** This was contradicted by
> §41 of this same file ("eighteen named tasks each start once") within days and never annotated,
> and §13 below promotes it as *"the one negative result in this project that has survived
> scrutiny."* It has not survived. On the retail path, `--enterlog=0x0011c808` — RTXC's task-creation
> entry, `r0` = the name pointer — logs **27 named creations** in a single 600 M boot, from
> `TimeManager` @49 556 197 to `VideoDaisyTask` @250 095 833, and the TCB array at `0x0087198c` holds
> **62** tasks. The dispatcher hook this paragraph asks for is `0x0011c808`, and it has been
> answering all along.
>
> The failure was not the positive control — `--findptr` really did find the three `sysinfo`
> pointers. It was that the control exercised **literal pointers in a data pool**, and a TCB does not
> hold a name pointer at all: the names live in the *creation descriptor*, which is consumed and
> discarded. The scanner was looking for a field that does not exist, and a control made of a field
> that does cannot detect that. Same rule as research/20 Addendum 8 §8: *a control only proves what
> it exercises*.

---

## 2. `0x30000000` is not an LCD controller — and the display firmware is not in OSOS

This is the correction that most changes the roadmap. **We have been calling `0x30000000` "the LCD
controller". It is the bus window for a Broadcom BCM2722 video co-processor.** From Rockbox
`firmware/target/arm/ipod/video/lcd-video.c`:

```
BCM_DATA     0x30000000     BCM_ALT_DATA     0x30040000
BCM_WR_ADDR  0x30010000     BCM_ALT_WR_ADDR  0x30050000
BCM_RD_ADDR  0x30020000     BCM_ALT_RD_ADDR  0x30060000
BCM_CONTROL  0x30030000     BCM_ALT_CONTROL  0x30070000
```

Bring-up needs GPIO and power first — `GPO32_VAL & 0x4000` tests whether the BCM is powered,
`GPO32_VAL |= 0x4000` powers it up, then a 50 ms wait; `GPIOC` bit `0x40` is the BCM interrupt line.
Then a handshake, and then:

> **the `vmcs` firmware blob is uploaded to the BCM's SRAM.**

**`vmcs` is not in OSOS.** It lives in the iPod's **Flash ROM at `0x20000000`**, indexed by a `flsh`
directory at `0x20000000 + 0xffe00` — 10-word entries with `[0]='flsh'`, `[1]` image id, `[3]`
offset, `[4]` length, `[7]` checksum.

### ✅ We have it — no hardware needed after all

**Found by the operator in `resources/`**, from archive.org: *"SA JULY 12 2007 ipod video prototype
firmware dump"* — `internal_rom_000000-0FFFFF.bin`, **exactly 1 048 576 bytes**, the NOR flash size,
from an **iPod Video** (our generation).

It is genuine. Byte 0 is `ea001ffe` = `b 0x8000`, the first-stage bootloader, and the `flsh`
directory at `+0xffe00` parses cleanly:

| id | image | offset | length |
|---|---|---|---|
| `0x6469736b` | **disk** mode | `0x0d3bd0` | 180 784 |
| `0x64696167` | **diag**nostics | `0x0a2cb8` | 200 472 |
| `0x7363616e` | disk **scan** | `0x089fdc` | 101 596 |
| `0x6c6f676f` | **logo** | `0x0879f8` | 9 700 |
| **`0x766d6373`** | **`vmcs`** — the BCM2722 firmware | `0x06ec98` | **101 728** |

`--flash=PATH` maps it at `0x20000000`.

**This removes the project's only hard hardware dependency.** For the record, the alternative routes
were both worse: no public dump exists (clicky's `resources/.gitignore` contains exactly one line,
`internal_rom_000000-0FFFFF.bin` — its author deliberately withholds their own and states Apple's ROM
cannot legally be redistributed), and the `aupd` "Flash ROM update" section inside Apple's own
updaters is **RC4-encrypted**. We confirmed that directly: `Firmware-20.6.3`'s directory holds
`osos` (7 559 680 — byte-identical in length to ours), `rsrc` (5 242 880) and **`aupd` (1 074 176)**,
and the `aupd` payload measures **8.00 bits/byte of entropy**. iPodLinux documents the decryption
(RC4, key derived from 8 markers in a 512-byte security block at word offsets
`{0x5,0x25,0x6f,0x69,0x15,0x4d,0x40,0x34}`), so that route is open if a second image is ever needed —
but it is no longer necessary.

### What it unlocks — and what it did not

RetailOS with the flash mapped: **no change, and it never reads the flash at all.** That is
consistent with §1 — it stalls before any task is created, so the display task that would want
`vmcs` does not yet exist. The dark screen is *downstream* of the real blocker, not the cause.

The far more valuable use is the **first-stage bootloader** sitting at offset 0. Every piece of
pre-boot state we have been reconstructing by hand — the `sysinfo_t` block, the Gestalt ID at
`sysinfo+0x84`, the memory-bank sizes — is state *that bootloader writes*. We can now **run it**
instead of inferring it, which is a genuine cold boot and removes a whole class of guesswork.

The `diag` image is independently interesting: a smaller, self-contained payload that exercises the
hardware, and a much simpler bring-up target than the full OS.

---

## 3. Smaller corrections from the same sweep

| Claim we held | Correction |
|---|---|
| PCF50605 ADC control is `0x2e` (`ADCC1`) | Rockbox's iPod driver writes **`0x2f`** — `(channel << 1) \| 1` — then reads **two** bytes from `0x30`, result `data[0] << 2 \| (data[1] & 3)`. `0x2e` is ADCC1 on the sibling **PCF50606**. Unresolved; model both |
| Hold switch might be PMU-based on the 5G | No — `pmu_holdswitch_locked()` is the **Classic/Nano2G** path. On PP502x it is GPIO A bit 5, and `DEV_INIT1 \|= INIT_BUTTONS` must be set first for hold detection to work at all |
| Diagnostics might be reachable from OSOS | **Diagnostics lives in Flash ROM, not OSOS** — so that decision is made by the flash bootloader before RetailOS loads. Disk mode *is* inside OSOS on PP iPods |
| No public emulator has attempted RetailOS | clicky's README does claim *"can already boot into RetailOS"*, while its Stage-3 checklist leaves `Boot into RetailOS` **unchecked** and the author notes "lots of accesses to undocumented parts of the PP5020 memory space". Read together: it starts executing and the author was triaging unknown registers — the same position as ours, one generation earlier. The operator, who follows its development, reports it does not boot RetailOS today |

**No PortalPlayer emulator other than clicky exists.** No MAME/MESS iPod driver. The QEMU forks
([devos50/qemu-ios](https://github.com/devos50/qemu-ios), [freemyipod/qemu](https://github.com/freemyipod/qemu))
are S5L only, and the latter is explicitly non-functional.

**Worth mining that we have not:** clicky's git history and issue tracker *as a bug-trail* — its log
of undocumented-register accesses is the closest prior art in existence. And its dev tooling is worth
copying: a GDB stub over a Unix socket, per-subsystem tracing, and `--flash-rom` for genuine cold
boot.


---

## 4. Cold boot — Apple's own bootloader runs, and brings up the video co-processor

`--cold-boot` maps the flash at `0` as well as `0x20000000` and enters at `0`, which is where the CPU
fetches out of reset. Apple's first-stage bootloader then runs instead of us reconstructing what it
would have left behind.

It works. The bootloader reads ~694 KB out of flash, relocates itself into IRAM, and proceeds
through hardware bring-up. Each stall was a status bit, and each was answered with `--rdval=ADDR=VALUE`
(a word that always reads as VALUE, whatever is written to it) rather than a baked-in guess:

| Address | What | Value |
|---|---|---|
| `0x70000030` | **undocumented** — absent from every published map; sits between `DEV_INIT2` and `DEV_TIMING1`, polled for bit 27. ~~*Since identified*~~: the external memory bus control register — bit 30 is the NOR write gate, bit 27 the controller's ready flag; **modelled, flag retired** ([research/12](12-bypass-ledger.md) #1) | `0x08000000` |
| `0x7000003c` | `XMB_RAM_CFG` — SDRAM controller, polled for bit 31. ~~*Since identified*~~: bit 24 is the configuration command and bit 31 its completion; **modelled, flag retired** ([research/12](12-bypass-ledger.md) #2) | `0x80000000` |
| `0x30030000`, `0x30070000` | `BCM_CONTROL` / `BCM_ALT_CONTROL` — the documented handshake: `0x80` clear, `0x40` set, `0x02` write-ready, `0x10` read-ready | `0x52` |
| `0x30020000`, `0x30060000` | `BCM_RD_ADDR` / `BCM_ALT_RD_ADDR` — bit 0 read-ready | `0x1` |

**The `0xff`-filled `lcd` region was actively wrong.** It dated from an assumption that `0x30000000`
was a panel controller and that "an emulated panel is always ready". Against a BCM handshake that
waits for bit `0x80` to *clear*, an all-ones fill is a permanent stall — 59 760 548 reads of it.

With the handshake answered:

```
lcd        0x30000000    15 780 076 reads   15 881 846 writes
sdram-low  0x00000000       101 728 reads      101 740 writes
```

Bidirectional traffic in the millions is a *transfer*, not a poll. And **101 728 is exactly the
`vmcs` image length** from the `flsh` directory. **Apple's bootloader is uploading the video
co-processor's firmware**, in our emulator, from the flash dump — the step that was supposed to be
impossible without hardware.

### Status

The bootloader has not yet handed off to OSOS (`ata commands: 0` — it has not read the firmware
partition). Bring-up continues; each step so far has been one documented status bit.

**Every `--rdval` above is a hypothesis, not a model.** A static value cannot satisfy a multi-phase
handshake indefinitely, and the BCM protocol has read and write phases with different ready bits.
These got the boot moving; a real BCM model will be needed before anything is drawn.


---

## 5. A BCM2722 model — the firmware uploads, and the co-processor is asked to answer

`--rdval` got the boot moving but could never finish the job: the BCM protocol has read and write
phases with different ready bits, and a static value cannot satisfy a state machine. `Bcm` in
`lib.rs` models the host protocol properly.

The host latches an internal address into `WR_ADDR` or `RD_ADDR` and streams halfwords through
`DATA`, which auto-increments; `CONTROL` carries the handshake. The internal address space is a
sparse `BTreeMap` — the firmware alone is 101 728 bytes and a framebuffer would dwarf it, so a flat
allocation is the wrong shape. `--bcm` enables it.

**The firmware upload completes:**

```
bcm: 50 872 halfwords written, 50 864 internal words held
```

50 872 halfwords = **101 744 bytes**, against a `vmcs` image of 101 728. Apple's bootloader has
loaded the video co-processor's firmware into the co-processor, in the emulator, from the flash dump.

### Where it stops, precisely

The read histogram — which exists so the polled word can be found without disassembling the loop:

```
internal 0x00000000  read 3 712 955 times
internal 0x00000002  read 3 712 955 times
internal 0x00000004  read 3 712 955 times
internal 0x00000006  read 3 712 955 times
```

**Eight bytes at internal address 0, read in a tight loop.** The host has started the firmware and is
waiting for the BCM to write a **signature or version word** at the base of its address space. Our
model answers zero, because the upload went elsewhere in the internal space.

This is now a specific question with a findable answer: **what does the host compare those eight
bytes against?** The comparison is in the caller of the read loop at `0x4000eac4`, and reading it
gives the value the running firmware is expected to produce. That is a disassembly task, not a
guess.

### The bug the instrument found

The model latched every address as zero, so the firmware went to internal 0 and every read polled the
wrong word. The cause, visible the moment the latches were logged:

```
latch wr off 0x10000 val 0x01f8 lo      -> 0x000001f8   BCMA_COMMAND
latch wr off 0x10000 val 0x0000 HI
latch wr off 0x10000 val 0x0c00 lo      -> 0x10000c00
latch wr off 0x10000 val 0x1000 HI
```

**The host writes both halves to the same offset, low first.** It does not address them by offset,
which is what the model assumed (`off & 2`). The halves are selected by *write order*.

Reads of the polled word fell from **3 712 955 to 8**, and the boot went on to the PMU and then the
disk. The lesson is the familiar one: the model was wrong in a way that produced plausible-looking
behaviour (a firmware upload of exactly the right size), and only logging the decode showed it.

### It reaches the disk

With the latch fixed and the I²C ADC answered:

```
ata commands: 4
  cmd 0xec  features 0x00  nsector 0x00  lba 0     IDENTIFY DEVICE
  cmd 0xef  features 0x02  nsector 0x00  lba 0     SET FEATURES — write cache
  cmd 0xef  features 0x03  nsector 0x0c  lba 0     SET FEATURES — transfer mode
  cmd 0x20  features 0x00  nsector 0x04  lba 0     READ SECTORS — 4 sectors from LBA 0
```

**The first disk read in the project.** Apple's bootloader is reading the partition table on its way
to loading OSOS off the firmware partition.

It runs with **interrupts disabled** (`irqs: 0 asserted`) and polls with timeouts — the loop at
`0x4000bc98` calls an elapsed-vs-timeout comparison at `0x4000e750`. So progress is gated on
*simulated time*, not on instructions: 2 commands by 600M instructions, 4 by 1.5B. Those timed waits
are what the billions of IRAM reads are. Nothing is stuck; it is a real boot running slowly.

### Honest status of this model

It reproduces the *host* side of the protocol. It does not execute the `vmcs` firmware — the BCM is
a second processor and we are synthesising its replies. `on_write` fakes the one acknowledgement the
documented bootstrap sequence waits for. Everything beyond that is unmodelled, and **no pixel has
been produced**. What has been demonstrated is that the transfer path works end to end.


---

## 6. The clock is the bottleneck, not the interpreter — and why JIT is not next

The bootloader runs with **interrupts disabled and polls with timeouts**: the loop at `0x4000bc98`
calls an elapsed-vs-timeout comparison at `0x4000e750`. Its progress is therefore gated on
**simulated time**, not on instructions executed, and it burns billions of cycles in delay loops
doing nothing.

We set that clock. `Machine::instr_per_usec` was a hardcoded 75 — modelling the real ~75 MHz part —
and is now a `--clock=N` knob. Measured at an identical 600M-instruction budget:

| | ATA commands reached |
|---|---|
| `--clock=75` (real time) | 2 |
| `--clock=5` | **5** |

```
cmd 0x20  nsector 0x04  lba 0        MBR / partition table
cmd 0x20  nsector 0x04  lba 252      the firmware partition
```

**This is the argument against building a JIT now.** A JIT would execute the delay loops 10–50×
faster; the clock knob *skips them*. It attacks the actual bottleneck, cost minutes rather than
weeks, and — decisively — a JIT would compile away the instrumentation this project runs on. Every
blocker cracked so far was found with the sampling profiler, breakpoints, `--watch`, per-page
unmapped tracking with originating PC, device counters, or the BCM latch log. All of those depend on
interpreting one instruction at a time with full visibility.

The pattern of the bugs matters here too. The `0xff` LCD fill, the BCM half-selector, the saturated
unmapped log — **every one was wrong in a way that produced plausible behaviour**, and every one was
caught by cheap introspection. A JIT is also a second implementation of the ARM semantics, needing
its own differential-fuzzing campaign, with divergences far harder to attribute.

JIT becomes the right call when there is a booting OS with a display and the goal is to *use* it —
games at 60 fps, audio without underruns. Not before.

### ✅ Resolved: LBA 252 vs LBA 63 is a block-size conversion

Our MBR's partition 0 says `startLBA 63`. The bootloader reads **LBA 252**. The relationship is
arithmetic:

```
63 × 4 = 252
```

The 5.5G uses **2048-byte logical blocks**. The bootloader reads the partition table, takes block 63,
and converts to a 512-byte LBA by multiplying by four. Our image builder placed the firmware
partition at 512-byte LBA 63 — a quarter of the way to where the firmware looks for it.

Relocating the partition to byte offset `252 × 512` moved the boot on immediately:

```
cmd 0x20  lba 252      firmware partition header  ([hi] magic)
cmd 0x20  lba 284      = 252 + 32, the image directory at +0x4000
cmd 0x20  lba 25292    near the aupd image at devOffset 0xc3a200
cmd 0x20  lba 25296    nsector 0x00 = 256 sectors — a 128 KB read
```

It located the directory and began reading the **flash-update image**, which is what a bootloader
does before deciding whether to boot: check for a pending flash update.

**Note for anyone rebuilding a disk image:** the firmware partition belongs at 512-byte LBA **252**,
not 63, and the MBR entry states it in 2048-byte blocks.


---

## 7. The bootloader streams the flash-update image — over DMA we do not model

Two fixes moved the cold boot from 5 ATA commands to 16.

**The PortalPlayer IDE controller has its own registers**, at `IDE_BASE + 0x00..0xff` — timings and
`IDE0_CFG` at `+0x28` — distinct from the ATA taskfile at `+0x1e0`. Our `Ata` model swallowed the
whole `0x400` window and returned zero for anything it did not recognise, so those never
round-tripped. The bootloader issues `READ SECTORS` and then polls **`IDE0_CFG` bit 3** at
`0x4000bc98`; against a permanent zero it waits forever. The controller block now round-trips, and
bit 3 reports data-ready.

With that, it streams:

```
cmd 0x20  nsector 0x00  lba 25296     256 sectors = 128 KB
cmd 0x20  nsector 0x00  lba 25552
cmd 0x20  nsector 0x00  lba 25808
...                     lba 26832
```

Each +256 sectors, sequential, starting at the `aupd` image (`devOffset 0xc3a200` = sector 25042
relative to a partition at 252). **It is reading the whole flash-update image** — what a bootloader
does before deciding whether a flash update is pending.

### ⚠️ It is progressing on a lie, and this needs fixing before anything is trusted

`ide` reports **8 reads** against 227 writes. Reading 1 MB through the taskfile would be a million
byte-reads. **The data is not moving.** The controller is transferring by **DMA**, which we do not
model — so every read "completes" (because we now report data-ready) while delivering nothing.

The bootloader will therefore be checksumming an empty buffer. That we made it progress by reporting
a ready bit we cannot honour is exactly the failure mode this project keeps hitting, recorded here
before it produces a "successful" boot built on nothing.

**Next: model the IDE DMA path.** The PP5020 DMA engine is at `0x6000b000` (Rockbox `pp5020.h`), but
`mmio-6` shows little traffic, so the transfer is more likely programmed through the IDE controller's
own registers in the `0xc3000000` block — which we now round-trip but do not act on. Logging those
writes will show the descriptor.


---

## 8. Reference material now held locally

Cloned into `resources/reference/` (gitignored — third-party code and Apple-copyrighted binaries):

| Source | Why it matters |
|---|---|
| `crozone/ipodloader2` | **iPodLinux's bootloader for real 5G hardware.** `ata2.c` is the only working PP5020 ATA driver we have source for |
| `freemyipod/defs` | `rtxc.py` — RetailOS RTXC object definitions, including the built-in semaphore name table. Reusable when we reach task enumeration |
| `freemyipod/wInd3x` | S5L bootrom exploitation. Not applicable to PortalPlayer; relevant to the 6G/7G roadmap |
| `Anonymous941/ipod-bootrom-archive` | three more 1 MB NOR dumps |

### The dump comparison is worth having

| dump | flash images |
|---|---|
| A1040 (3G) | `disk` 139 068 · `scan` 63 132 · `diag` 84 192 — **no `vmcs`** |
| A1099 (4G Photo) | `disk` 251 552 · `diag` 97 396 · `logo` 15 628 — **no `vmcs`** |
| **A1238** | `disk` **180 784** · `diag` 97 832 · `logo` **9 700** · **`vmcs` 96 384** |
| **ours (prototype)** | `disk` **180 784** · `diag` 200 472 · `scan` 101 596 · `logo` **9 700** · **`vmcs` 101 728** |

A1238's `disk` and `logo` lengths are **identical to ours**, so whatever its model number says, it is
the same generation — and it is a **retail** counterpart to our prototype. Two independent `vmcs`
blobs to cross-check, and a retail-vs-prototype diff available. Only the BCM-equipped generations
carry `vmcs` at all, which independently confirms what that image is for.

## 9. `IDE0_CFG` is interrupt status — and the transfer path is still unexplained

`ipodloader2/ata2.c` settles the register's meaning:

```c
outl(inl(0xc3000028) | 0x20, 0xc3000028);  // clear intr
outl(inl(0xc3000028) | 0x30, 0xc3000028);  // this hopefully clears all pending intrs
```

`0xc3000028` is **interrupt status/clear**, not a readiness flag — and our own log shows Apple's
bootloader writing exactly `0x20` there. So bit 3 is "interrupt pending", and the bootloader *polls*
for the drive interrupt because it runs with interrupts masked. `Ata` now models it that way: set on
command completion and on each sector becoming available, cleared when the firmware writes the
`0x30` bits.

**It did not change the outcome.** Still 16 commands, still `ide: 8 reads` — the data never moves.

Ruled out so far, each measured rather than assumed:

| Hypothesis | Result |
|---|---|
| SoC DMA engine at `0x6000b000` | never touched |
| A DMA descriptor in the IDE window | only timings, `IDE0_CFG` and `ATA_CONTROL` are ever written |
| The second controller at `0xc0003000` | that is the **PP5002** path in `ipodloader2`; zero traffic there |
| PIO through `ATA_DATA` (`+0x1e0`) | 8 reads total — it is not reading |

`ipodloader2` reads sectors with plain PIO (`inw(pio_reg_addrs[REG_DATA])`, 4-byte register stride),
so a working 5G driver *does* use PIO. Apple's bootloader evidently does something else, and we have
not found what. **This remains the one place where the cold boot is advancing on a model we know to
be false.**


---

## 10. 🖼 A picture — Apple's firmware rendering through Apple's video co-processor

`--boot-flash=NAME` loads one of the flash's own self-contained images and enters it. `diag` was the
right first target: 200 472 bytes, no disk, no filesystem, no OSOS, and it draws to the screen.

The BCM model now implements the **command protocol**, not just the transport. Commands are encoded
`BCM_CMD(x) = ((~x << 16) | x)`; the host kicks one by writing `0x31` to `CONTROL`; the co-processor
signals completion by clearing `BCMA_COMMAND`, which the host polls. Command list from Rockbox
`lcd-video.c`: `0` LCD_UPDATE, `5` LCD_UPDATERECT, `8` LCD_SLEEP.

```
bcm: 8 commands kicked, 4 frame updates

internal write runs (largest first):
  0x000e0000..0x0010581e   76 816 halfwords (153 632 bytes)   <- 320 x 240 x 2 = 153 600
  0x00000000..0x00018d5e   50 864 halfwords (101 728 bytes)   <- the vmcs firmware
```

A **320×240 RGB565 framebuffer**, exactly at `BCMA_CMDPARAM` (`0xE0000`) where Rockbox says it is,
with 71 685 non-zero pixels of 76 800. `--bcm-dump=0xE0000:140:F0:out.ppm` reads it out.

> *Corrected 2026-08-14: `BCMA_CMDPARAM` is a **transfer buffer**, not the panel, and Rockbox's own
> gloss on the name is "Parameters/data for commands". The `76 816` above is the giveaway that was
> read past for weeks — it is `16 + 76 800`, and the sixteen are an eight-word rectangle header the
> command interface consumes. The picture in it is still a 320×240 frame, because a full-screen
> update stages one; a partial update stages only its own rectangle behind that header. See
> [research/24](24-the-apple-logo.md).*

It renders legible text in four languages:

> **Connect to your computer. / Use iTunes to restore.**
> *Connectez à votre ordinateur… / Mit dem Computer verbinden… / コンピュータに接続してください。*

### Why this image is evidence rather than decoration

**The message is semantically correct for the state of our machine.** We know the disk transfer is
broken — §9 records that the data never moves — and this is precisely the screen an iPod shows when
it cannot read its drive. The firmware is not confused; it is correctly reporting the fault we
already knew we had, in its own words.

Everything in the chain is real: Apple's flash bootloader path, Apple's diagnostics image, Apple's
video co-processor protocol, the `vmcs` firmware uploaded from a genuine NOR dump, and RGB565 decoded
at the documented address. Nothing here was staged to produce a screenshot.

**What it does not mean.** The co-processor's own firmware is still not executed — we synthesise its
replies. This is the *host's* output, which is what a panel would have displayed. And it is `diag`,
not RetailOS: the OS still creates no tasks, and the disk still does not read.


---

## 11. NOR must be read-only, and the boot still stops after `aupd`

Two fixes and one honest non-result.

**The device counters were blind to every modelled device** (§9's retraction). Fixed: accounting now
happens before any device model can return early. The corrected report immediately showed something
the old one could not:

```
flash-low  0x00000000   1 753 898 reads   1 062 104 writes
```

**A million writes into the NOR image.** `--cold-boot` maps the flash at 0 — correct out of reset —
but never unmaps it, so it permanently shadows the low SDRAM view. The bootloader's own load of the
firmware image was landing in the ROM. NOR does not work that way: a store does not change it, real
flash needs a command sequence. `Memory::readonly` now marks the region, and `locate_write` skips
read-only regions so the store reaches the memory behind them.

**It did not unblock the boot.** Still 16 ATA commands, ending:

```
cmd 0x20  nsector 0x00  lba 27088
cmd 0x20  nsector 0x34  lba 27344      <- 52 sectors, the tail of aupd
```

From LBA 25296 to 27396 is 2 100 sectors = 1 075 200 bytes, against an `aupd` length of 1 074 176.
**The bootloader reads the flash-update image in full and then stops** — at 2B instructions with
`--clock=5`, roughly 400 seconds of simulated time, so it is not merely slow.

The plausible readings, none yet tested: it is verifying a checksum that fails; it has decided a
flash update *is* pending and is trying to write NOR, which we now correctly refuse; or it wants
something else entirely before handing off. The next measurement is what it does with those bytes.


---

## 12. The bootloader is not waiting — it has given up, and we now know why

### A call tracer, validated before it was believed

The toolset could see instructions, devices, registers and memory, but not **control flow** — only a
16-instruction tail, which always shows the innermost loop and never how it was reached. `--calls`
records every `BL` as `(site, target)` in a **ring** (not a capped log — keeping the *first* 4096 is
the saturation trap that has already produced two false conclusions here).

Per the rule below, it was checked before use: it reported `0x40009cf4 -> 0x4000e744`, and
disassembly of that address gives `bl 0x4000e744`. Only then was its output trusted.

### What it showed

```asm
4000159c  stmdb sp!, {r4, lr}
400015a0  bl 0x400015cc
400015ac  bl 0x4000de4c
400015b0  bl 0x40001278
400015b4  b 0x400015b4      <- branch to self
```

**`b 0x400015b4` is an intentional halt.** The bootloader is not waiting on hardware and not slow —
it has *decided to stop*. Which is exactly consistent with the frame we rendered from `diag`:
*"Connect to your computer. Use iTunes to restore."*

### The failed check: an image checksum

Firmware-directory entries carry a checksum at `+0x1c`, a plain byte sum. iPodLinux documents that it
covers the image **after a 512-byte security block**, so the range is `devOffset + 0x200, +len)`.
Against our disk image:

| image | stored | computed (skew `0x200`) | |
|---|---|---|---|
| `osos` | `0x2c7c48f3` | `0x2c7c48f3` | ✅ |
| `rsrc` | `0x18319bab` | `0x18319bab` | ✅ |
| `aupd` | `0x0b19db1c` | `0x08299587` | ❌ |

Two exact matches confirm both the algorithm and the skew. **`aupd` is the one that fails** — and
`aupd` is precisely the image the bootloader reads in full immediately before halting. Note the
arithmetic: `aupd` at `devOffset 0xc3a200` plus the `0x200` skew plus its length runs 512 bytes past
the declared partition size, so the image is truncated in our build.

> ⚠️ **Partly retracted.** `aupd` is **RC4-encrypted**, so a byte sum of the ciphertext was never
> going to match — the stored checksum is over the *plaintext*. Verified against the pristine
> updater: `osos` and `rsrc` match `[devOffset+0x200, +len)` exactly, and `aupd` matches **no**
> extent, in our image or in Apple's own. The `osos`/`rsrc` result stands; the `aupd` conclusion does
> not, and the truncation arithmetic was a coincidence.
>
> What survives: the bootloader reads `aupd` in full and then executes a deliberate halt. Whether it
> failed a checksum, decided an update is pending, or wanted something else is **not yet known**.

## 12a. We can decrypt Apple's flash-update images

Implemented the derivation from iPodLinux's Flash Decryption page: an 8-marker security block, magic
constant `0x54c3a298`, and RC4 over the payload. The published description leaves two things
ambiguous, and a small search settled both:

| Ambiguity | Answer |
|---|---|
| Security block 512 or 2048 bytes? | **512** — even on the 5.5G, whose disk blocks are 2048 |
| The doc says *"the key is big endian and you need to flip it"* | the working key is the **little-endian** byte order, i.e. the opposite of how that reads |

For `Firmware-20.6.3`'s `aupd`: marker index **6**, key **`c9263bdb`**.

The plaintext is self-evidently correct:

```
060000ea  =  0xea000006  =  b +0x20      \
feffffea  =  0xeafffffe  =  b .           }  an ARM exception vector table
3f0000ea  =  0xea00003f  =  b +0x100     /
```

and it contains **`hslf` ×11** — `flsh` byte-reversed, the flash-image directory magic. The whole-image
checksum is close but not exact (`0x0b19b20c` against `0x0b19db1c`), so the precise extent still needs
settling, but the decryption itself is proven.

Note the payload is **not** a raw NOR image: `hslf` first appears at `0x14f8`, where our NOR dump has
it at `0xc748`. It is the flash *updater program* with images embedded, matching iPodLinux's
description — *"if you extract the entire flashupd image and disassemble it you can locate the
individual images that are the boot loader, diagnostic, disk image, and disk scan code."*

### Why this matters beyond the current blocker

**Flash contents can now be recovered from Apple's own updaters, for any generation, with no
hardware.** The NOR dump the operator found removed the hardware dependency for the 5.5G; this
removes it for every model whose updater we can obtain — which is the C-series generation ladder in
[research/10*(moved to the `opod` repository)*.

## 13. Method: the rule these instruments now follow

Three instruments have produced confident wrong answers in this project — a saturating log, a
change-detector read as a write-detector, and an accounting hook placed below the code paths it was
meant to account for. Each cost a retracted conclusion, one of them committed.

**No new instrument's output is believed until it has reproduced something already known.** The
pointer scanner was checked against a pointer we wrote ourselves; the call tracer against a
disassembled `BL`. ~~The one negative result in this project that has survived scrutiny — "no named
task is ever created" — is the one that had a positive control.~~

> **That last sentence is retracted 2026-08-14, and it is the most instructive thing in this
> section.** "No named task is ever created" is wrong — 27 named creations in a 600 M retail boot,
> 62 TCBs (§1's retraction, research/20 Addendum 14 §9). It was singled out here *because* it had a
> positive control, and the control is exactly what made it feel safe. The control found three
> literal pointers in a data pool; the thing being looked for was a name pointer in a TCB, which does
> not exist — RTXC consumes the name in the creation descriptor and never stores it. **A positive
> control that does not exercise the failure mode is not a control, it is a reassurance.** The count
> of instruments that have produced confident wrong answers in this project is no longer three.

## 14. Research: what is documented, and what is not

| Question | Answer |
|---|---|
| What the ROM does with `aupd` | **No public documentation.** Not in iPodLinux, Rockbox or freemyipod |
| The boot sequence, or what triggers the restore screen | **No public documentation** — it must be read off the ROM we hold |
| `ipodloader2` chain-loading `osos` | **Yes**, and it has **zero `aupd` handling** — `grep -rni aupd` over the tree returns nothing. **Prebuilt `loader.bin` exists** (crozone releases v2.8.1/v2.8), so no toolchain is needed |
| The `aupd` encryption | **Fully documented.** RC4; key derived from 8 markers in the 512-byte security block; magic constant `0x54c3a298`; key is big-endian and must be byte-swapped. Live PDF mirror: `cdn.hackaday.io/files/1739857443774240/Flash_Decryption.pdf` |

Two further findings worth keeping:

**Apple's ROM reads a boot-action block from the end of IRAM.** `ipodloader2`'s `set_boot_action`
writes an 8-char command (`"diskmode"`), the literal magic **`"hotstuff"`**, and a flag near
`0x4001ff00`, then resets. So that region is a real ROM-visible interface — and uninitialised garbage
there is an alternative explanation for a stall, worth instrumenting.

**A methodology worth stealing**, from q3k's wInd3x writeup: the **oracle test** — flip one input per
run and classify the outcome (halt / restart / normal) to leak a bit at a time about code you cannot
inspect. That maps directly onto this blocker: vary the checksum, the version word, or the IRAM magic
and classify the stall.


---

## 15. Past the halt: the handoff uses READ DMA

Removing the `aupd` directory entry — a bypass, logged as #12 in
[the ledger](12-bypass-ledger.md) — got the ROM past its halt and revealed the next stage:

```
cmd 0x20  nsector 0x04  lba 0        MBR              (PIO)
cmd 0x20  nsector 0x04  lba 252      partition header (PIO)
cmd 0x20  nsector 0x04  lba 284      image directory  (PIO)
cmd 0xec  IDENTIFY DEVICE                             <- controller re-initialised
cmd 0xef  SET FEATURES  nsector 0x02
cmd 0xef  SET FEATURES  nsector 0x0c
cmd 0xc8  nsector 0x04  lba 284      image directory  (DMA)
```

**`0xc8` is READ DMA.** The ROM reads the firmware directory by PIO, re-initialises the controller,
and then re-reads it by **DMA** — which reads like a controller self-test before the real image load.
Our model aborted unknown commands, so the boot stopped there.

That also explains an earlier dead end: the search for a DMA descriptor was conducted against the
**`aupd`** path, which is PIO throughout. There was nothing to find.

With `0xc8` served, `ide` reads go from 8 to **7,438** — the firmware does pull the data. But it
still stops after that one command.

**The SoC DMA engine is not involved.** `DMA0_BASE_ADDR` is `0x6000b000` (Rockbox `pp5020.h`), and a
`--pagelog` over `0x6000a000..0x6000e000` — rerun *after* the counter fix, since the first attempt
used the broken instrument — shows only GPIO at `0x6000d000` and the cache at `0x6000c000`. Nothing
at `0xb000`.

So on this controller "READ DMA" evidently still moves bytes through the register file, and the open
question is narrower than it was: **what does the ROM compare, or wait for, after that read?**


---

## 16. Speeding up the loop

Two levers found, one taken, one still on the table — and it is worth being explicit that **the
bottleneck has never been the interpreter's raw speed**; it has been simulated time and wrong
questions.

### Taken: `--clock=N`

The ROM polls with timeouts, so its progress is gated on simulated time, not instructions.
Identical 600M budget: `--clock=75` reached 2 ATA commands, `--clock=5` reached 5. A separate
control: **6 000 000 000 instructions at `--clock=75` reached exactly what 600 000 000 reached at
`--clock=5`.** Ten times the work for the same result.

### Taken: accounting is now opt-in

`count()` attributed every byte access to a region, which cost a full scan of the region list — on
top of the scan `locate` already does. Two scans per byte, always on, for a report almost no run
reads. Now behind `--devices`.

| | instructions/sec |
|---|---|
| before | 8.6 M |
| after | **11.6 M** |

1.35×, on every experiment from here.

### Not taken, and the largest remaining: snapshot / restore

Every experiment re-runs the whole boot from reset. Reaching the current frontier — the `0xc8` DMA
read — costs ~1.5 billion instructions, about two minutes. **Every question about late boot pays that
toll again.**

Snapshotting machine state (registers, all regions, device state) at a chosen point and resuming from
it would make late-boot iteration **seconds instead of minutes**. Now that the work is exclusively at
the *end* of a long boot, this is the highest-leverage tool left unbuilt.

It pairs directly with the **oracle test** from q3k's wInd3x writeup: snapshot once, then vary a
single input per run and classify the outcome. That turns a two-minute experiment into a sweep.

### Also unexploited: parallelism

Sweeps are embarrassingly parallel and are currently run one at a time.


---

## 17. Apple's bootloader is now printing its own console output

A bug in our SWI handling was hiding it. `run()` intercepted **the SWI vector itself**
(`if pc == SEMIHOSTING_VECTOR`), so on the cold-boot path every one of the ROM's own supervisor
calls was read as an ARM semihosting operation. One of them landed on `SYS_EXIT`, which is why the
run ended at exactly 90 452 451 instructions with `Stop::Exited` and an **empty** `firmware output`.

ARM semihosting is specifically `SWI 0x123456`. The check is now on the immediate — read from the
instruction at `lr - 4` — rather than on the vector address. That matters both ways: RetailOS *is* a
debug build and does use real semihosting (its panic dumps arrive that way), so the fix could not
simply be "disable semihosting when cold-booting".

With it, Apple's bootloader talks:

```
(C) Copyright 2000-2006
Apple Computer, Inc.
BootLoader running on iPod M25 cpu Unknown
Image size: 86780 ( 7460 bytes free )
Retail mode
Bootloader could not execute target image!
```

**The ROM is now diagnosing itself for us.** Two leads, in its own words:

- **`cpu Unknown`** — it does not recognise our chip. The ROM contains the template `PP502X-?` and
  the fallback `Unknown`, so it formats a CPU name from the revision byte we seed at `0x70000000`
  (currently `0x00360000`, giving `'6'`), and the lookup misses.
- **`Bootloader could not execute target image!`** — the actual failure, and the thing to chase. Note
  it is *not* a checksum complaint: `osos` and `rsrc` both verify exactly
  (`[devOffset+0x200, +len)`).

Also visible in the ROM's string table: `retailOS`, `M25 Diagnostics`, `Retail mode`.

### Why this changes the character of the work

Every blocker until now had to be inferred from register accesses and control flow. **The bootloader
has a console, and we can read it.** That is a qualitatively better instrument than anything we have
built — it reports intent, not just behaviour, and it was there the whole time behind a two-line bug.


## 18. Reading the ROM by its own console

`print_sites` records `(return address, string pointer)` for every semihosted write, which turns each
console line into an address. Every message funnels through one `printf` at `0x40001194`, so the
useful correlation is with the `--calls` ring.

That gave the tail of the boot in the ROM's own structure:

```asm
400088b0  bl 0x4000941c
400088b4  bl 0x40009c28
400088b8  bl 0x4000091c
400088bc  bl 0x4000095c
400088c0  mov r0, #0
400088c4  bl 0x40004bc8     <- set_boot_action(0)
400088c8  bl 0x40008e4c     <- prints "Bootloader could not execute target image!"
```

**`0x40004bc8` is `set_boot_action`**, identified from the data immediately after it:
`746f6f62 21676e69` = `"booting!"` and `73746f68` = `"hotstuff"` — the IRAM boot-action block
iPodLinux documents (`ipodloader2`'s `set_boot_action` writes an 8-char command, the magic
`"hotstuff"`, and a flag near the end of IRAM). It selects one of several strings by argument,
writes the magic, and records the action. It is the *epilogue*, not the loader.

### The boot-mode dispatcher

The decision is made just above, at `0x40008858..0x40008894`:

```asm
40008858  b 0x40008894                  <- straight to the default
4000885c  add r0, pc, #0x130 ; bl puts  \
40008860  ...                            }  a mode: message, then a handler
40008870  bl 0x40005c88                 /
40008878  add r0, pc, #0x124 ; bl puts  \
40008880  bl 0x400060fc                  }  another mode
40008888  add r0, pc, #0x120 ; bl puts  \
40008890  bl 0x4000615c                  }  another
40008894  add r0, pc, #0x120 ; bl puts  <- the failure default, where we land
```

Three real boot modes — almost certainly disk mode, diagnostics and the normal path — each with its
own message and handler, converging on a default that prints the failure and sets a boot action.

**We fall into the default.** So the remaining question is what selects the branch, and the two
candidates are both now known interfaces: the **boot-action block at the end of IRAM** (the
`"hotstuff"` magic — which on real hardware the *previous* boot or the updater writes, and which our
machine leaves as whatever IRAM happens to hold), or a property of the loaded image.

That is a much better-posed question than "why does it halt", and it is the last one before the
handoff.


## 19. ⚠️ RETRACTED — "the ROM cannot identify the hardware" was wrong

**The section below is kept because its disassembly is accurate and useful, but its conclusion is
false.** See §21 for what actually happens. The error: I traced a *possible* path to the failure
message and assumed it was the *taken* path, without ever reading the value. Measuring it took one
breakpoint.

## 19a. The original (incorrect) chain

Following the console message backwards through the ROM gives a complete causal chain — the first
time in this project a blocker has been explained rather than characterised.

```
0x40008808  bl 0x40001818          boot_mode = f(1)
0x4000880c  mov r4, r0
0x40008810  cmp r4, #6
0x40008814  addls pc, pc, r4, lsl #2    jump table, 7 entries
              r4 = 0  ->  0x40008894   "could not execute target image!"
              r4 = 1..6 -> real boot modes, each with its own message and handler
```

`r4` is the **boot mode**, and we get **0**. Inside `0x40001818` it starts at **1** — a valid mode —
and is zeroed by this gate:

```asm
0x40001840  bl 0x4000296c          ; hardware-ID validity
0x40001844  cmp r0, #0
0x40001848  beq 0x40001884         ; invalid -> the path that leaves r4 = 0
```

And `0x4000296c` is:

```asm
bl 0x40001de8       ; the hardware ID
r0 = r0 >> 16       ; high halfword
r0 = r0 - 4
cmp r0, #8
movls r0, #1        ; valid iff the high halfword is 4..12
movhi r0, #0
```

**The same bounds RetailOS uses** — its model selector at `0x2653a4` compares `hwid >> 16` against
`0xc`. Our Gestalt `0x000B0005` has high halfword 11 and would pass; the ROM simply is not getting
it. `0 >> 16` is 0, `0 - 4` underflows, the comparison fails, and the mode collapses to 0.

`0x40001de8` reads the ID with **exactly the shape RetailOS uses** — a `0x7FFFFFFF` "not yet cached"
sentinel, then a fetch from `[obj + 0x84]`:

```asm
ldr r0, [r4, #0x28]        ; cached
cmn r0, #0x80000001        ; == 0x7FFFFFFF ?
bl  0x40009c04             ; else fetch the object
ldrne r0, [r0, #0x84]      ; hwid = obj->+0x84
```

`0x40009c04` is a lazy singleton with its default instance at **`0x40015888`** — and the console
already told us what that object is: the ROM printed `"iPod M25"` from **`0x40015890`**, which is
`+8` into it. That is `BoardHwName` in `sysinfo_t` layout. **The bootloader builds the same
structure it later hands to RetailOS, and `+0x84` is the Gestalt in both.**

So: **the ROM cannot determine which iPod it is running on.** That single failure produces both
symptoms — `cpu Unknown` in the banner and boot mode 0 in the dispatcher.

Poking the cached slot (`0x40014b78`) and the struct field (`0x4001590c`) changes nothing: both are
written during the ROM's own initialisation, so the value has to come from where it actually looks —
hardware.

### The next experiment is an oracle sweep

Identification inputs, all cheap to vary: the chip ID at `0x70000000` (we seed `0x00360000`),
`STRAP_OPT_A`/`STRAP_OPT_B` at `0x70000008`/`0x7000000c`, and GPIO straps. Vary one per run and
classify the banner — `cpu Unknown` versus a real name, and boot mode 0 versus non-zero.

This is exactly the technique q3k describes for wInd3x, and exactly what snapshot/restore was built
for: the identification happens early, so a snapshot just before it makes each probe seconds rather
than minutes.


## 20. Mapping the ROM's hardware descriptor — and a useful negative

### The whole boot decision is ~2 million instructions

The `--calls` ring at a **2M** budget shows the *same tail* as at 600M. Everything after is the
`udelay` loop. So the entire failing boot — banner, disk, dispatcher, failure — happens in about
**0.2 seconds of wall clock**, and iteration on this phase needs no snapshot at all. Snapshot/restore
remains right for late-boot RetailOS work; it is unnecessary here.

That is worth stating plainly because it inverts the assumption this session started with: **we were
never slow at the thing we were actually studying.**

### The ROM keeps a hardware descriptor at `0x40014b50`

Two independent paths reach it:

```asm
0x40001de8   ldr r4, =0x40014b50 ; ldr r0, [r4, #0x28]   cached hardware ID
0x40001b0c   ldr r4, =0x40014b50 ; ldr r1, [r4, #0x1c]   a flags word
```

`0x40001b0c` sets or clears a bit in the flags at `+0x1c` (by argument), then calls the hardware-ID
validity check at `0x4000296c` and, only if valid, acts on GPIO. So the same identification failure
suppresses hardware setup as well as boot-mode selection — one root cause with several downstream
effects.

### Disproven: the chip ID is not an ASCII string at `0x70000000`

The ROM contains the template `PP502X-?`, and OSOS contains the literal `"PP5020AF"`, which together
suggested the chip register holds a name. Three seeds — `"PP5020AF"`, `"PP5022C"`, `"PP5021C"` —
changed nothing: still `cpu Unknown`, still boot mode 0. **The CPU name does not come from
`0x70000000`.**

### Two facts that shape how this ROM can be analysed

- **It is scatter-loaded into several IRAM segments.** `"Unknown"` lives at flash `0x010918` and runs
  at `0x40008b6c` (delta `0x3fff8254`); `"iPod M25"` lives at flash `0x00ad7c` and runs at
  `0x40015890` (delta `0x4000ab14`). There is no single relocation offset, so flash-offset arithmetic
  cannot be used to find code.
- **String addresses are PC-relative** (`add r0, pc, #imm`), so `--findptr` finds nothing for them —
  confirmed against the `"Unknown"` pointer, which appears nowhere in memory.

Together these mean the ROM has to be read from the **running machine**, which is what the console,
`--calls` and `--disasm` already do well.


---

## 21. What actually happens: the retail handler runs, and returns

Two measurements demolished §19.

**The hardware ID is read successfully.** Seeding the cache slot and watching it:

```
0x400007e4  0xdeadbeef -> 0x7fffffff   the ROM's own init writes the "not cached" sentinel
0x40001e08  0x7fffffff -> 0x000b0011   the fetch succeeds — hwid = 0x000B0011
```

High halfword `0x000B` = 11; `11 - 4 = 7 <= 8`; **the validity check passes.** The ROM knows exactly
what it is running on.

**And the boot mode is 1, not 0.** One breakpoint at the dispatcher:

```
at 0x40008810   r4 = 0x00000001
```

So `addls pc, pc, r4, lsl #2` takes entry 1 — `b 0x4000885c`, a **real** boot mode — not the failure
default. The banner's `Retail mode` is that handler announcing itself.

```asm
4000885c  add r0, pc, #0x130 ; bl puts     "Retail mode"
40008864  cmp r6, #0
40008868  moveq r0, #4
40008870  bl 0x40005c88                    <- the retail loader
40008874  b 0x40008894                     <- reached only because it RETURNED
```

**`"Bootloader could not execute target image!"` is the fallback after the handler returns**, not a
dispatcher default. A loader that succeeds never comes back — it jumps to the OS.

### The real blocker: `0x40005c88`

It looks images up by type. Breaking at its lookup call:

```
at 0x40005cc8   r0 = 0x72737263      'rsrc'
```

and the adjacent literal is `'osos'`. So `0x4000536c` is *find image by type*, and the retail path
wants `rsrc` and `osos` — both of which our firmware partition contains, and both of whose checksums
verify exactly over `[devOffset+0x200, +len)`.

**So the question is now: why does the lookup or load fail for images that are present and valid?**
That is one breakpoint away, and it is the last link.

### The method note

§19's chain was disassembled correctly and reasoned wrongly. Every instruction quoted in it is real;
the mistake was tracing a path *to* the failure and assuming it was the path *taken*. The rule this
project already adopted for instruments applies to reasoning too: **a plausible chain is a
hypothesis, and reading one register would have falsified it immediately.**


## 22. The failure has a name: image load returns error `0x58`

The chain from the console message down to the actual fault, every step measured rather than
inferred:

```asm
0x40008870  bl 0x40005c88        retail-mode handler
0x40005cc8  bl 0x4000536c        find image 'rsrc'   -> r0 = 0, SUCCEEDS (r3 = 0xb012, the
                                                        directory's version field)
0x40005cf0  bl 0x40005ef8        boot image 'osos'
0x40005f28    bl 0x40005730      load the image
0x40005f2c    cmp r0, #0         -> r0 = 0x58   FAILS
0x40005f30    bne 0x40005f5c     skip the launch, unwind, return
0x40008874  b 0x40008894         "Bootloader could not execute target image!"
```

At the failure point `r5 = 0x6f736f73` — `'osos'` — confirming which image, and `r2 = 0x001ae6c9`,
which has the shape of a partially-accumulated checksum.

**So the ROM finds the firmware partition, reads the directory, successfully locates `rsrc`, and then
fails to load `osos` with error `0x58`.** That is a far more specific statement than anything this
project has had about this blocker, and it is one lookup away from a cause: find where `0x40005730`
returns `0x58`.

Worth noting what this rules out. The partition is found, the directory parses, image lookup by type
works, and the disk transfers a megabyte by PIO. None of those are suspects any more.

### The running score on this blocker

| Claim | Verdict |
|---|---|
| The ROM halts because `aupd`'s checksum fails | ❌ `aupd` is RC4-encrypted; the sum was never comparable |
| The disk data never moves | ❌ 1 083 904 bytes transfer by PIO; our counter was blind |
| The ROM cannot identify the hardware | ❌ hwid `0x000B0011`, and the check passes |
| Boot mode is 0, the failure default | ❌ boot mode is **1**, a real handler |
| The retail handler's `osos` load fails with `0x58` | ✅ measured |

Four wrong, each one plausible, each killed by a single measurement. The one that survived is the one
that came from a breakpoint rather than a disassembly.


## 23. Narrowed to one step, with a tag in hand

`0x40005730` loads an image in stages, looping (`sub r4, r4, #1` at `0x40005808`) and collecting each
step's result into `r5`. Breaking on both collection points shows exactly where it turns:

```
at 0x4000577c   r0 = 0x00000000                 find 'osos'      OK
at 0x40005800   r0 = 0x00000000  r1 = 0x40015570                 OK
at 0x40005800   r0 = 0x00000000  r1 = 0x40015548                 OK
at 0x40005800   r0 = 0x00000058  r1 = 0xffffffff
                r2 = 0x31656449  r3 = 0x10000000                 FAILS
```

Two things fall out of that last line:

- **`r2 = 0x31656449` is ASCII `"Ide1"`** — a four-character tag, in the same style as `osos`, `rsrc`
  and `flsh`. It is not one we have seen in any directory we have parsed.
- **`r3 = 0x10000000`** is OSOS's load address, so this stage is the one that actually places the
  image.

The error `0x58` comes from one of `0x400053c8` / `0x400053ac`, both called just above the collection
point at `0x40005800`. Whichever it is, it fails on the third stage having succeeded on the first
two — so the image is found and partly processed before something about it is rejected.

**This is where the next session starts**, and it is a genuinely narrow question: disassemble
`0x400053c8` and `0x400053ac`, find the path returning `0x58`, and identify what `"Ide1"` is.


## 24. ⚠️ RETRACTED — `0x58` is a plain immediate, and the search that said otherwise was broken

**This section recorded that `0x58` was a composed error code. It is not.** It is
`movne r0, #0x58` at `0x4000b728`, an ordinary immediate in IRAM. Section 25 has the real answer;
this stays because the *way* it went wrong is the reusable part.

The claim rested on searching the live machine for the encoding of `mov r0, #0x58` and finding
"none in IRAM". That negative was an artifact of the search, not a fact about the ROM. ARM puts the
condition code in the **top nibble**, so `e3a00058` and `13a00058` (`movne`) are different words —
and a bootloader's error paths are overwhelmingly conditional. Searching a handful of hand-picked
condition codes and reading the miss as absence is the same mistake as reading a saturated counter
as a measurement.

The fix was to make the instrument answer the question that was actually being asked. `--findptr`
now takes an optional mask:

```
--findptr=0xe3a00058/0x0fffffff     # all sixteen condition codes at once
```

which found `iram 0x4000b728 movne r0, #0x58` immediately, and disassembles every hit, because a
match that is code is unreadable as a bare hex word.

**The general lesson.** A negative result from a search is only as strong as the search's coverage,
and coverage is exactly what a hand-enumerated list does not have. When the question is "does this
value appear anywhere", the tool has to be able to express *anywhere* — otherwise its silence is
indistinguishable from absence, and the difference is a wrong conclusion committed to a file.

## 25. `0x58` means "the drive was not ready", and the DRQ bit is why

With the masked search, the whole chain fell out in one sitting. Reading down from the loader:

```
0x40005730  the image loader           tries copies newest-first
0x40008c3c    dispatch                 vtable call into the device driver
0x400040d4      the ATA driver's read  destination = entry.loadAddr
0x40003160        sectors -> bytes     tail-call
0x4000b534          the transfer loop
0x4000b578            bl 0x4000b700    <- "wait for the drive to be ready"
```

`0x4000b700` is tagged `"Ide1"` — a four-character **module tag** sitting in the literal pool
directly above the prologue, alongside a `0x1388` (5000 ms) timeout. That is all `"Ide1"` ever was:
not a device, not an image, not a second IDE channel. The device table holds exactly two entries,
`'flsh'` and `'ATA!'`, and `0x400054e0` loops over precisely those two.

The wait it calls, `0x4000bc0c`, is a textbook ATA status poll:

| Status bit | Meaning | Returns |
|---|---|---|
| `0x80` BSY | busy | keep polling |
| `0x01` ERR | error | **2** ✅ accepted |
| `0x08` DRQ | data request | **4** ❌ |
| `0x40` DRDY | ready | **3** ✅ accepted |
| — | timeout | **1** ❌ |

and the caller maps *anything but 2 or 3* to `0x58`. So **`0x58` is "the drive is not ready"** —
and it fired because our own ATA model was leaving **DRQ asserted**.

Measured, not inferred: the status register read `0x58` — `DRDY|DSC|DRQ` — sixteen times, and there
were **zero** timeouts. (The error code equalling the status byte is a coincidence, and a confusing
one.)

**This was our bug, not Apple's**, and the model's own comment had predicted it: READ DMA was
"served through the data register like the PIO commands… if the driver never reads it, that absence
is itself the next measurement." The absence got measured.

### An instrument for "where did this value come from"

Two wrong answers in a row came from reasoning about where a constant *might* be produced. The
third attempt built `--retwatch=V`, which logs the instruction that **puts** V into `r0` — a
transition, not a match, so the one producing instruction is not buried under every caller that
passes it along. It named `0x4000b728` in a single run.

A search finds a value where it is *written down*. `--retwatch` finds it where it is *produced*,
which is the question that was actually being asked all along.

### And a truncating reporter that produced a confidently wrong answer

Between those two, a breakpoint run reported `--- breakpoint hits: 20 ---` and printed **8**. Three
of the eight were the site under test, all showing success, and that was read as "all three
succeeded, so this is not the source". It was a sample, not the set. The reporter now prints a
complete per-site tally before any register dump, so the count and the detail can never disagree.

Twice now — the saturating unmapped counter, and this — a capped log has been read as a
measurement. The rule that keeps falling out: **a display limit must never be able to masquerade as
a result.**

## 26. The DMA engine, read off Apple's bootloader programming it

Rockbox's PP driver is PIO-only and no published map describes this block, so it came from watching
the ROM at `0x4000bb04`:

| Register | Meaning |
|---|---|
| `IDE_BASE+0x400` | control — bit 1 arm, **bit 3 direction** (set = read into memory), **bit 0 = GO** |
| `IDE_BASE+0x408` | transfer length in bytes, **minus 4** |
| `IDE_BASE+0x40c` | destination address |
| `IDE_BASE+0x028` bit 15 | completion-interrupt enable |

The driver programs address and length, issues `0xc8` through the taskfile, and only then sets GO —
from the *other* `"Ide1"` function, `0x4000ad98`, which afterwards clears bits 0 and 31 and waits
for DRDY (returning `0x59` if that wait fails).

One reason this block had never been seen: **the emulator's ATA window stopped at `off < 0x400`**,
so every descriptor write fell through into the backing region, was stored, and was never read.
That silence looked exactly like a driver that does not program DMA at all. The window now runs to
`0x410`, `0xc8` stages sectors without asserting DRQ, and GO commits them into memory through
`locate_write` — so a transfer aimed at NOR is refused rather than quietly corrupting the flash
image.

**Result:** `osos` loads. ATA commands went from 10 to 68, the driver walks the disk in 128 KB
transfers (LBA 288, 544, 800, 1056, …), 7,563,264 bytes reach `0x10000000`, and all three image
loads return 0. Error `0x58` is gone from the boot entirely.

## 27. The next failure is ours too: the firmware partition is packed wrong

The boot now fails later, at the **checksum**, with a new code `0x57`:

| | |
|---|---|
| expected (from the directory entry) | `0x2c7c48f3` |
| computed by the ROM over loaded memory | `0x2ec6f100` |

The expected value is not in doubt — the plain byte-sum of `OSOS_correct.bin` is **exactly**
`0x2c7c48f3`. So the checksum is right and the bytes in memory are wrong.

Logging each DMA transfer's source LBA against its destination shows why:

```
lba 284    -> 0x10000000    2048 bytes     <- a probe, immediately overwritten
lba 288    -> 0x10000000  131072 bytes     <- the body chain starts here
lba 544    -> 0x10020000  131072 bytes
```

The body is read from **LBA 288**; our disk has it at **LBA 287**. One sector early.

The cause is in how the disk was built, and the ROM's own arithmetic names it. Measured at
`0x4000413c`: partition base `0x3f` (63), sectors-per-block 4, and block index
`0x4400 / 2048 = 8` — **the `0x400` remainder is truncated**. The ROM addresses images in
2048-byte blocks and cannot express a devOffset that is not block-aligned.

Our firmware partition is a **verbatim copy of `Firmware-20.6.3`**, so the file's byte-granular
internal offsets became devOffsets. The consistency check is decisive:

| image | devOffset | `/2048` | |
|---|---|---|---|
| `rsrc` | `0x0073a000` | 3688.0 | **aligned** |
| `osos` | `0x00004400` | 8.5 | **not aligned** |

A real iPod's firmware partition is written by the updater, which places bodies at block-aligned
offsets and writes matching devOffsets into the directory. Ours inherited a file layout that the
ROM's loader cannot address.

### ⚠️ The repacker conclusion above is WRONG — the disk is correct

Before building the repacker, the theory got tested arithmetically against the disk, which costs
seconds and no emulator run. Summing `len` bytes from each candidate placement:

| memory would be | sum |
|---|---|
| disk from LBA 288 | `0x2c7c86cf` |
| **disk from LBA 287 — our actual placement** | **`0x2c7c48f3`** ✅ *the expected value* |
| disk from LBA 286 | `0x2c7d4460` |
| ROM actually computed | `0x2ec6f100` |

**Our placement is right.** Had the ROM read our disk contiguously from where the image actually
sits, the checksum would have passed. And `0x2ec6f100` matches *no* contiguous placement, so the
bytes the ROM summed are not a straight copy of any disk range — which means the fault is in the
emulator, not the layout.

Logging all 59 transfers confirms the transfers themselves are fine: source LBA 288 through
7,708,672, destination `0x10000000` through `0x10736000`, **zero contiguity gaps**.

So something *shadows* that memory. The leading suspect is in the recipe rather than the model:
`--boot-osos` requires `--osos=FILE`, and `--osos=` maps `OSOS_correct.bin` as its own region at
`0x10000000` — the exact address, and the exact image, the ROM is being asked to load for itself.
Two regions cover that range, and `locate` / `locate_write` each return the first match.

**A cold boot must not be handed the image it exists to load.** That is a bypass hiding inside the
harness, and it needs retiring on principle regardless of whether it turns out to be this bug.

**The method point is the durable one.** The repacker was a plausible, well-argued theory built on
a chain of inferences about block arithmetic — and it was wrong. It cost nothing because the
theory was cheap to *test* before it was expensive to *build*. Prefer the arithmetic check over the
implementation every time.

**Still true:** every failure since the DMA fix has been in our reconstruction of the device, not in
Apple's code. The ROM has been correct at every step it has been measured against.


## 28. ✅ The handoff completes — Apple's ROM loads, verifies and runs RetailOS

The one-sector gap was a disk-layout error after all, and the retraction in §27 was itself too
hasty. **The repacker theory was right about the layout and wrong about the mechanism**, and it
took the memory-map fix to make the real shape visible.

Logging the transfer function's arguments settles it. Both DMA groups are the `osos` load, and both
target the same address:

```
0x4000b534(dev, lba, count, dest)
  lba 0x000  count 4        dest 0x40016fa8   MBR, into an IRAM buffer
  lba 0x0fc  count 4        dest 0x40016fa8   LBA 252, partition header
  lba 0x11c  count 4        dest 0x40016fa8   LBA 284, the directory
  lba 0x11c  count 4        dest 0x10000000   LBA 284 — read, then discarded
  lba 0x120  count 0x39b0   dest 0x10000000   LBA 288 — the body
```

`0x39b0` is 14,768 sectors, which is **exactly `ceil(len / 2048)` blocks**. The ROM addresses the
image in 2048-byte blocks and starts the body at block 72 = `partition + 0x4800`.

Our partition was a verbatim copy of `Firmware-20.6.3`, so the body sat at `devOffset + 0x200` —
the *file's* 512-byte security-block skew carried onto a disk where it does not belong. The ROM's
block convention wants the next block boundary, `+0x4800`.

Writing the body there makes the checksum pass exactly:

| | |
|---|---|
| computed | `0x2c7c48f3` |
| expected | `0x2c7c48f3` ✅ |

and the console changes from `Bootloader could not execute target image!` to:

```
Running 'osos' … from 0x10000000
```

**The full cold-boot path now runs end to end**: reset out of NOR · SDRAM bring-up · `vmcs` uploaded
to the video co-processor · PMU over I²C · ATA identify and configure · MBR · firmware partition ·
directory parse · 7.5 MB of RetailOS by bus-master DMA · checksum verified · jump.

### Where it stops now

Execution ends spinning at `0x40000004` — the **undefined-instruction vector**. The profile is
69% at `0x40000000`, i.e. the vector table itself. So RetailOS is entered and then faults.

The likely cause is the other half of bypass #11. RetailOS's scatter-load writes its initialised
data through the **low** view of SDRAM and reads it back through the high one; on hardware that
works because the MMAP unit remaps address `0` onto SDRAM after boot. Cold boot now correctly
leaves `0` to NOR — which is right out of reset and wrong once the firmware has remapped.

**Honouring the MMAP remap is the next step**, and it is exactly the retirement condition written
against #11 in the ledger before any of this happened.

### Caveat on the disk

Moving the body to `+0x4800` runs its tail 512 bytes into `rsrc` at `devOffset 0x73a000`. `rsrc` is
not read on this path so the boot is unaffected, but **the partition now needs a real repack** —
every body at its own block-aligned offset with the directory rewritten to match. The original
bytes are saved at `osos_region_backup.bin`.


## 29. The fault is the MMAP remap — measured, not guessed

Breaking at the undefined-instruction vector names the cause outright:

```
at 0x40000004 (undefined instruction)
  r0 =0xf000f000     <- the MMAP unit
  r1 =0x0000023c     <- the address the ROM jumps to
  r4 =0x10000000     r5 =0x6f736f73 ("osos")   r12=0x10000000
  r14=0x00004024     <- faulting PC + 4, so the bad instruction was at 0x4020
```

and the profile contains **zero** samples anywhere in SDRAM. RetailOS is never executed.

The ROM finishes the load, programs the MMAP unit, and then jumps to a **low** address — `0x23c` —
expecting SDRAM to have been remapped over `0`. Cold boot leaves `0` as NOR, which is right out of
reset and wrong once the firmware has remapped, so the jump lands in flash bytes and decodes as an
undefined instruction at `0x4020`.

`0xf000f000` has been written by the time of the fault:

```
f000f000  00003bf0 00003a88 00003a00 10000f84
f000f010  20003800 00003f88 00000000 00000000
```

Note `0x10000f84` — an SDRAM address — in the fourth word.

The warm-boot path already encoded this requirement without naming it: `--boot-osos` builds an
`osos-low` mirror at base `0` with the comment *"the boot code jumps to physical 0x23c, so OSOS must
also appear at address 0."* That mirror was standing in for the remap all along. The cold path needs
the real thing: **address `0` must become SDRAM at the moment the firmware programs the MMAP, and
NOR before it.**

This is the last item on bypass #11, and the first thing to do next.


## 30. ✅ RetailOS executes — the MMAP remap closes bypass #11

The encoding came out of `ipodloader2/interrupts.c`, which programs the unit directly rather than
describing it: pairs at an 8-byte stride, `LOGICAL = 0x3a00 | base`, `PHYSICAL = 0x3f84 | base`,
with the base in the top 16 bits. Reading back what Apple's ROM had written:

```
f000f008 = 0x00003a00   logical  -> 0x00000000
f000f00c = 0x10000f84   physical -> 0x10000000
```

**Logical 0 mapped to SDRAM.** Modelling the unit — eight window pairs, aliases rebuilt whenever a
window is programmed, identity mappings skipped, the page cache invalidated on every change — and
the jump to `0x23c` lands in RetailOS instead of NOR.

| | before | after |
|---|---|---|
| `0x40000000` (undefined-instruction vector) | **69.0%** | gone |
| low addresses (remapped -> RetailOS) | none | `0x00084380`, `0x00084390`, `0x000843a0` |

Those low PCs translate through window 1 to `0x10084xxx` — RetailOS's own code, running.

**The bootloader is finished.** Reset out of NOR · SDRAM bring-up · `vmcs` to the video
co-processor · PMU over I²C · ATA configure · MBR · firmware partition · directory · 7.5 MB by
bus-master DMA · checksum verified · MMAP remap · jump into RetailOS. All of it Apple's code, none
of it reconstructed.

### Where it goes next

Execution eventually wanders into `0x4001ffc0` and runs zeroed IRAM. So RetailOS starts and then
loses its way — expected, since it is now running against a cold-boot machine whose peripherals are
still the ones in the ledger: a fake PMU (#3), synthesised video co-processor replies (#6), one
core (#7). The warm path already reaches a live scheduler, so the work ahead is joining two things
that each run, not building a third.

**Note the pattern.** Bypass #11 was written down with its retirement condition long before it bit,
and then it explained three separate failures in a row: the `osos` checksum reading NOR, the
undefined-instruction fault, and this. The ledger is not bookkeeping — it is the cheapest debugging
tool in the project.


## 31. Where RetailOS derails: the MMAP window *size* is still a guess

RetailOS runs, then reads unmapped memory — 12,792 reads across 6 pages, every one of them from a
single instruction:

```
0x0471619c..0x04716fff   3684 reads   first pc 0x00084390
0x04733000..0x04733c9f   3232 reads   first pc 0x00084390
0x047326c8..0x04732fff   2360 reads   first pc 0x00084390
0x0472292c..0x04722fff   1748 reads   first pc 0x00084390
0x04723000..0x04723593   1428 reads   first pc 0x00084390
0x04717000..0x04717153    340 reads   first pc 0x00084390
```

All of them sit **just past `0x04000000`** — which is exactly the window size the MMAP model
currently assumes. That 64 MB is not read from the hardware; it was hardcoded, because the base
addresses were decodable from `ipodloader2` and the size was not.

The size lives in the flag bits that were dropped:

| | ours (Apple's ROM) | `ipodloader2` |
|---|---|---|
| `LOGICAL` flags | `0x3a00` | `0x3a00` |
| `PHYSICAL` flags | `0x0f84` | `0x3f84` |

The logical halves agree exactly; the physical halves differ in bits 12–13, which is precisely
where a size or mask field would live. **Decoding those two fields is the next step**, and it should
be read off the ROM's own use of them rather than inferred from the one sample we have.

This is a fair trade to have made — a hardcoded size got RetailOS executing and turned an invisible
problem into a numbered one — but it is a bypass, and it belongs in the ledger as such rather than
in the model pretending to be hardware.


## 32. The MMAP is programmed by RetailOS, not the bootloader — and the constants are in hand

`--watch=0xf000f00c` names the writer outright:

```
0x10000220  0x00000000 -> 0x10000f84   str r1, [r0, #0xc]
```

`0x10000220` is **inside the loaded image**, four instructions before its entry at `0x23c`. So the
remap is RetailOS's own first act, not something the bootloader does for it — which is why the jump
goes low *before* anything is mapped there, and why the ROM's own code never touches `0xf000f000`.

The setup at `0x100001f0` is a straight run of stores from a literal pool:

```
10000200  ldr r0, [pc, #0xdc]   ; MMAP base
10000208  str r1, [r0, #0x0]    ; window 0 logical
10000210  str r1, [r0, #0x4]    ; window 0 physical
10000214  mov r1, #0x3a00       ; window 1 logical  -- immediate, base 0
10000220  str r1, [r0, #0xc]    ; window 1 physical
10000228  str r1, [r0, #0x10]   ; window 2 logical
10000230  str r1, [r0, #0x14]   ; window 2 physical
10000238  mov pc, r1            ; and jump
```

Every value is a constant, so the size cannot be recovered from arithmetic — it has to be decoded
from the bit patterns. The pool, dumped at `0x100002e4`:

| word | value | meaning |
|---|---|---|
| `0x100002e4` | `0xf000f000` | MMAP base |
| `0x100002e8` | `0x00003bf0` | win0 logical — base `0` |
| `0x100002ec` | `0x00003a88` | win0 physical — base `0` |
| — | `0x00003a00` | win1 logical (immediate) — base `0` |
| `0x100002f0` | `0x10000f84` | win1 physical — base `0x10000000` ← **the remap** |
| `0x100002f4` | `0x20003800` | win2 logical — base `0x20000000` |
| `0x100002f8` | `0x00003f88` | win2 physical — base `0` |
| `0x100002fc` | `0x0000023c` | the jump target |
| `0x10000300` | `0x40005ff8` | (return into the bootloader) |

The flag halves to decode are `0x3bf0 / 0x3a88 / 0x3a00 / 0x0f84 / 0x3800 / 0x3f88`. Against
`ipodloader2`'s `0x3a00` logical and `0x3f84` physical, the logical forms match its convention
exactly; the physical ones vary in the low bits, which is where the size or mask must live.

**That is the next session's first question**, and it now has all its data in one table rather than
needing another run. RetailOS is the obvious first Ghidra import for answering it — this routine
sits at image offset `0x1f0`, immediately findable.

## 33. ✅ The MMAP window encoding, decoded — and RetailOS runs

The answer did not need Ghidra, and it was not where §32 pointed. **The size is not in the physical
half at all.** Rockbox has carried the encoding in plain sight for years, in
`firmware/target/arm/pp/crt0-pp.S`:

```asm
    .equ    MMAP_LOG,    0xf000f000 /* MMAP0 */
    .equ    MMAP_PHYS,   0xf000f004
#if MEMORYSIZE > 32
    .equ    MMAP_MASK,   0x00003c00
#else
    .equ    MMAP_MASK,   0x00003e00
#endif
    .equ    MMAP_FLAGS,  0x00000f84
```

It names the two halves. The **logical** half carries a `MASK`, the **physical** half carries
`FLAGS`:

```text
LOGICAL  = base<31:16> | mask<13:4>     mask bit m compares address bit m+16
PHYSICAL = base<31:16> | flags<11:8>    READ | WRITE | DATA | CODE
```

Two independent confirmations that this is the real split:

1. **Only the logical half is parameterised by memory size.** 64 MB gets `0x3c00`, 32 MB gets
   `0x3e00` — one more compared bit, half the window. A size field in the physical half would have
   to move too, and it does not.
2. **The physical half differs by *part*, not by size.** `MMAP_FLAGS` is `0x3f84` on PP5002 and
   `0x0f84` on PP502x. RetailOS writes `0x0f84` because a PP5021C is a PP502x. `ipodloader2` writes
   `0x3f84` on both, which is the PP5002 value used compatibly — bits 12–13 are simply ignored here.

So §31's guess (*"bits 12–13 of PHYSICAL are the size"*) was wrong twice over: wrong field, wrong
register. The bits it fixated on are a part-number difference.

Rockbox also settles priority without a datasheet. Its `crt0` copies itself to IRAM at `0x40000000`
and executes there **while** programming a window based at `0`. It survives — which it could not if
that window also claimed `0x40000000`. **Address bits 31:30 are compared unconditionally.**

### What RetailOS's three windows actually say

| window | mask | compares address bits | meaning |
|---|---|---|---|
| 0 | `0x3bf0` | 29,28,27,25,24…20 | base `0` → physical `0`: **identity, i.e. off** |
| 1 | `0x3a00` | 29,28,27,**25** | 32 MB → SDRAM, **bit 26 not compared** |
| 2 | `0x3800` | 29,28,27 | 128 MB at `0x20000000` → flash |

Window 0 is how you turn a window *off*: program it to map onto itself.

And window 1 is the bug. A mask bit left clear **above** the window's own size is a don't-care, so
the window answers for two disjoint ranges — `0x00000000..0x02000000` *and*
`0x04000000..0x06000000`. Modelling it as one flat 64 MB range is precisely why RetailOS read into
nothing at `0x0471xxxx`: those addresses are in the window, via a bit the hardware never checks.

The translation is the standard decoder form, `phys = (addr & ~tested) | (base & tested)`, so bit 26
passes straight through: `0x04710000` → `0x14710000` → the SDRAM mirror → `0x10710000`, which is
image offset `0x710000`, inside the 7.5 MB RetailOS image. It was reading its own data all along.

That mirror is why `translate` now runs **two** levels. The MMAP unit is one hop and does not chain
— a window's output is not fed back through the windows. Downstream of it the memory controller
decodes fewer address bits than the bus carries, which is what makes SDRAM appear twice. A window
whose don't-care bits push an address into a mirror has to land somewhere real.

### Measured

| | before | after |
|---|---|---|
| unmapped accesses at 150M | 12 792 reads over 6 pages | **0** |
| where it ends up | derailed at `0x00084390` | runs to budget, PC in the `0x000dexxx` region |
| IRQs asserted / taken | — | 93 355 / 19 653 |
| BCM | idle | **4 commands kicked, 2 frame updates** |

RetailOS drives the display. **Bypass #15 is retired** — replaced by a decode with a citation, not a
better guess.

### The next frontier, already visible

At a 600M budget a new one appears, small and specific:

```
unmapped: 236 reads, 0 writes across 1 pages
  0xea000078..0xea0000d7   236 reads   first pc 0x000a09b4
```

`0xea0000xx` is not an address — it is an ARM `b` instruction. Something loads a word out of a
vector table and dereferences it as a pointer. That is the next thread, and unlike this one it
probably *does* want Ghidra.

The profile meanwhile is 20% in a two-instruction loop at `0x00084390`/`0x000843a0` — the same PC
that used to fault. It is no longer faulting; it is polling. What it polls for is the question after
that.

## 34. What RetailOS does once it is running

Disassembling that "poll loop" answers the question and confirms the decode at the same time:

```
0008438c  cmp    r1, r2
00084390  ldrcc  r3, [r0], #0x4      <- copy word
00084394  strcc  r3, [r1], #0x4
00084398  bcc    0x0008438c
0008439c  mov    pc, lr
000843a0  mov    r2, #0x0            <- fill word
000843a4  cmp    r0, r1
000843a8  strcc  r2, [r0], #0x4
000843ac  bcc    0x000843a0
```

It is not polling anything. It is **`memcpy` and `memset`** — an OS bringing up its data and clearing
its BSS, which is exactly what 20% of early boot should look like. And in the literal pool four
words above it sits `0x0471715c`: a source pointer squarely inside the range that used to fault.
RetailOS was copying its own data section out of the image, through the uncompared bit 26. §33's
decode predicted that address would resolve to image offset `0x71715c`, and here is the instruction
that reads it.

### It drives the disk itself

The DMA log ends with a transfer the bootloader did not make:

```
lba 14880 -> 0x10720000   90112 bytes     <- last of the bootloader's 60-transfer image load
lba 0     -> 0x17edbea0    2048 bytes     <- RetailOS, reading the partition table itself
```

83 ATA commands, up from the bootloader's 68. RetailOS has brought up its own storage stack.

### It renders — ❌ **RETRACTED, see §39**

> **This section is wrong and is kept for the correction.** The framebuffer below is **Apple's
> bootloader's**, not RetailOS's — it is the boot logo. A run that never reaches RetailOS at all
> produces byte-identical BCM traffic (§39). RetailOS never touches the video co-processor.

| BCM internal write run | bytes | what |
|---|---|---|
| `0x00000000..0x00018d5e` | 101 728 | the bootloader's `vmcs` upload — matches the `flsh` directory's `vmcs` length **to the byte** (§8) |
| `0x000e0000..0x0010581e` | 153 632 | **76 815 halfwords, against 320 × 240 = 76 800** — a QVGA RGB565 framebuffer |

Dumped as an image, the framebuffer is 2 922 non-zero pixels of 76 800: mostly black, with a few
white diagonal lines. That is a screen being cleared and lightly drawn into, not a UI. Worth stating
plainly, because 153 632 ≈ 320 × 240 × 2 was *also* consistent with several duller explanations and
the image is what ruled them out. The `vmcs` run decodes as pixels into pure noise, which is the
control: it is VideoCore code, and it looks like it.

### The next frontier

```
unmapped: 236 reads, 0 writes across 1 pages
  0xea000078..0xea0000d7   236 reads   first pc 0x000a09b4
```

`0x000a09b4` is `ldrne r0, [r0, #0x0]` — the `if (p && *p)` idiom. A breakpoint there hits **21 288**
times against 236 bad reads, so this is a hot, healthy function whose pointer argument is
occasionally garbage. `0xea000078` is not an address at all; it is an ARM `b` instruction, so
something is loading a word out of code and dereferencing it. It is not the first word of NOR
(`0xea001ffe`), so the source is still open.

> **A methodology note, because it cost a measurement.** Three probes in a row "found nothing"
> before it emerged that the shell's working directory had reset and `./cold-boot.sh` was exiting
> 127 — the runs never happened. Grepping combined stdout+stderr for a success pattern turns a
> failed command into a confident negative. **This is the third time in this file that a
> non-measurement has posed as a measurement** (after the saturating `unmapped` counter and the
> truncated breakpoint tally). The rule that keeps earning itself: check that the instrument ran
> before believing what it did not say.

## 35. RetailOS boot-loops — a null virtual call landing on its own reset vector

### `first pc` was lying, and the fix caught it immediately

The unmapped report named `0x000a09b4` as the culprit. It made **8** of 1 036 accesses. Adding a
full per-PC tally to `UnmappedPage` — the same shape of fix as the breakpoint tally in §24 — named
the real one on the first run:

```
0xea000078..0xea0000d7   1036 reads   first pc 0x000a09b4
      pc 0x00183e7c  x1016
      pc 0x000a09b4  x8
      pc 0x000a0bd0  x8
      pc 0x000fb8d4  x4
```

**Four times now** a report that showed a sample has been read as one that showed everything. The
field is called `first_pc` and it is honest about that; the report printed it under a heading that
was not. It now prints every PC, and says how many it omitted.

### The mechanism

`0x00183e7c` is C++ virtual dispatch:

```
00183e64  ldr r0, [r4, #0x440]   ; obj
00183e6c  ldr r0, [r0, #0x1c]    ; obj->field_1c
00183e74  ldr r1, [r0, #0x0]     ; vtable pointer
00183e7c  ldr r12, [r1, #0x5c]   ; vtable slot 23   <- the unmapped read
00183e88  mov lr, pc
00183e8c  bx  r12
```

`0xea000078 - 0x5c` = `0xea00001c`, so the vtable pointer is an **ARM branch instruction**, not an
object. `obj->field_1c` is a garbage pointer aimed at RetailOS's own exception vectors, and reading
`*ptr` yields a `b` instruction. The dereference does not fault, because logical 0 is legitimately
mapped — it is where RetailOS lives.

Slot 23 is unmapped, our bus returns 0, and `bx 0` branches to address 0. The image's first word is
`0xea00007a` — `b 0x1f0` — which is the MMAP setup routine from §32. **The crash lands on the reset
vector**, so it presents as a silent reboot rather than a hang.

### Measured

Breakpoints at the entry, the reset target and the MMAP routine, over 1.2G instructions:

| breakpoint | hits | |
|---|---|---|
| `0x100001f0` | **1** | the genuine first entry, before the remap |
| `0x00000000` | **256** | 1 cold reset (all registers zero) + 255 restarts |
| `0x000001f0` | **255** | the MMAP routine, re-run every restart |

Which explains a set of numbers that had looked like steady progress: between 600M and 1.2G, ATA
commands stayed at 83 and BCM frame updates at 2, while IRQs kept climbing. It was not settling into
an idle loop. **It was rebooting, roughly every 4.7M instructions.**

The first crash returns to `0x000fb8ec`; the 255 that follow are dominated by `0x00183e7c`. So the
first failure and the loop are at different sites, and the first one is where to look.

### What this does and does not say

It does **not** say the emulator is wrong. Returning 0 for an unmapped read is what converts the
crash into a reboot, but the defect is upstream: an object with an uninitialised field, which means
some construction step did not run or silently failed. The suspects are the bypasses that make
hardware answer plausibly rather than correctly — #3 (every I²C read returns all-ones) and #6
(synthesised BCM replies) are both capable of letting an init path "succeed" against a device that
never really answered.

**This is now the blocker**, and it is a far better-defined one than "RetailOS derails."

## 36. Both crash sites are the same bug, and the obvious experiment cannot be run

### The first crash is a pointer-to-member-function call

`0x000fb8d4`, where the *first* restart originates, is not the same code as `0x00183e7c` but it is
the same shape:

```
000fb8c4  tst   r6, #0x1          ; PMF: low bit selects virtual vs non-virtual
000fb8c8  add   r0, r10, r8       ; object + adjustment
000fb8cc  ldrne r1, [r0, #0x0]    ; vtable pointer
000fb8d4  ldrne r2, [r7, r1]      ; vtable[offset]        <- the unmapped read
000fb8e8  bx    r2
```

That is the ARM C++ **pointer-to-member-function** convention — a (pointer, adjustment) pair whose
low bit distinguishes a virtual member from a direct one. So both sites are *a method call on an
object whose vtable pointer is not a vtable*, reached by two different dispatch mechanisms. One bad
object, two ways of calling into it.

The read at `r1 + 0x5c` = `0xea000078` puts `r1` at `0xea00001c` — a `b` instruction, so the word
loaded from the object *is* code. The object pointer lands in the exception-vector region at the
bottom of RetailOS's own image, which is mapped and therefore silent.

### The falsification test cannot run — and that is the finding

The ledger named #3 (all-ones I²C) and #6 (synthesised BCM replies) as the suspects, on the
reasoning that a device answering plausibly-but-falsely lets a constructor "succeed" against
hardware that never replied. The cheap way to test that is to remove each and see whether the loop
changes character. Both were tried at a 1.2G budget:

| variant | outcome |
|---|---|
| baseline (`--bcm --i2c-fill=0xff`) | 255 restarts, 1 036 unmapped reads |
| `--i2c-fill=0x00` | **never leaves the bootloader** — PCs stay in IRAM at `0x4000abcc`, no firmware output, 0 IRQs |
| no `--bcm` | **worse** — no firmware output section at all, 0 IRQs |

Neither bypass can be removed, because Apple's *bootloader* depends on both before RetailOS is ever
reached. **They cannot be A/B tested; they can only be replaced.** A negative result, but a
structural one: it says the path to this bug runs through building a real PCF50605 and executing
`vmcs`, not through toggling a flag and watching what changes.

It also sharpens what #3 and #6 cost us. They were adopted to get *past* the bootloader, and their
wrongness is now inherited by everything downstream, invisibly — which is exactly the failure mode
the ledger exists to prevent.

## 37. A real PCF50605 — built, and it changes which code path the firmware takes

`--pmu` replaces `--i2c-fill=0xff` with an actual chip: the register map from Rockbox's
`pcf5060x.h`, the **iPod Video** power-on defaults its `pcf50605_init()` documents in comments
(`IOREGC 0xf8`, `DCUDC1 0xe3`, `D1REGC1 0xf5`, `D3REGC1 0xf5`, `LPREGC1 0x1f`), read-clearing
INT1..INT3, a register pointer with auto-increment, and an ADC that latches a 10-bit result into
ADCS1/ADCS2.

The transport decoding was verified against Rockbox's `i2c-pp.c` rather than assumed, and it is
exactly right: `I2C_ADDR = (addr << 1) | 1` for a read, `CTRL` bits 1..2 carry `len - 1`, bit `0x20`
selects read, and `i2c_readbytes` sets the register pointer with a **one-byte write** before
reading. That last detail is what makes a pointer model correct rather than a guess.

### Two real bugs found on the way

**SEND is self-clearing.** We left bit `0x80` latched in CTRL forever, so a driver waiting for it to
fall waits forever. Fixed. The fill baseline came out **byte-identical** afterwards (93 355 IRQs
asserted, 19 653 taken), which is the control that says the fix changed no behaviour it should not
have.

**The I²C data registers are real registers, not a shadow.** The firmware *writes* the target
register address into them to set up a read, and what it wrote stays there until a transfer
overwrites it. Modelling them as private state made every data register the firmware had not just
read return zero.

**And the log cap lied again.** `pcf50605: 797095 read transfers` against an `i2c_log` capped at
4096. Every "hottest register" reading earlier in this session was a sample of 0.5% of the traffic.
The device itself is deliberately driven outside the cap — a capped log must never become a capped
*device* — but the report still needs reading as the sample it is.

### The measurement that matters

Decoding CTRL across both runs splits them cleanly:

| | `--i2c-fill=0xff` (boots) | `--pmu` (stuck) |
|---|---|---|
| dominant read | `ctrl 0xa0` — len **1** — ×1778 | `ctrl 0xa2` — len **2** — ×2036 |
| dominant write | `ctrl 0x80` — len 1 — ×1783 | `ctrl 0x80` — len 1 — ×2039 |

**A modelled chip does not merely change what the firmware reads; it changes which code path the
firmware runs.** Two-byte reads from pointer `0x30` are ADCS1+ADCS2 — the 10-bit conversion result
read as a pair. Apple's firmware enters its ADC loop and never leaves it, ~800 000 times in 150M
instructions.

It is **not waiting on a magnitude.** Forcing ADCS1 alone, then the whole `0x30..0x37` block, to
`0xff` (via the new `--pmu-force=REG=VALUE`, which is `--rdval` for I²C registers) leaves the loop
running. A full-scale result does not satisfy it any more than a mid-scale one does.

So the open question is precise: **what terminates Apple's ADC loop?** The shape of the remaining
answer is probably a *transition* rather than a value — a busy bit that must be seen set and then
clear, or a result that must change between samples — because a conversion that is complete before
it is started is the one thing real hardware never does. That is the next thing to model.

### A control that could not have passed, again

Forcing every register to `0xff` was meant to reproduce the fill through the device and prove the
harness. It failed, and for a while that looked like a harness bug. It is not: **`--i2c-fill`
answers all four data registers, while a faithful device latches only the `len` bytes it was asked
for**, so DATA(1..3) legitimately differ and the two can never agree. The control was incapable of
passing.

That is the same mistake the snapshot tooling nearly cost us (see
[research/12](12-bypass-ledger.md) § snapshot/restore), written down again because recognising it
took an hour the second time too.

## 38. ✅ ADCS2 bit 7 is conversion-ready — and the PMU was **not** the boot loop

### The bit

Counting reads *inside the device* rather than inferring them from the I²C log — the log's data
column holds the previous transfer's bytes, so reading a pointer out of it is right only by accident
— named the loop exactly: registers `0x30` and `0x31` read **797 093 times each**. ADCS1 and ADCS2.

Forcing ADCS2 alone settles which bit:

| ADCS2 forced to | boots? |
|---|---|
| `0x80` | ✅ |
| `0xff` | ✅ |
| `0x04` | ❌ |

**Bit 7 of ADCS2 is conversion-ready.** Not the result bits beside it — `0x04` sets a result bit and
fails. Apple's firmware polls the pair and will not proceed without it, and `--i2c-fill=0xff` set it
**by accident**, which is precisely how a bypass hides a fact indefinitely: it was never "the ADC
never reports ready", it was "we never told it ready".

Modelled properly the bit is clear while the conversion is in flight and set when the result lands,
so the firmware observes a conversion *complete* rather than finding one already complete.

### Measured

| | all-ones bypass | modelled PCF50605 |
|---|---|---|
| reaches RetailOS | yes | **yes** |
| PMU transfers to get there | 797 095 | **1 785** |
| IRQs asserted / taken | 93 355 / 19 653 | 93 119 / 19 653 |

A **446×** reduction in bus traffic: the spin is gone, not hidden.

### The negative result, which matters more

| | restarts over 1.2G |
|---|---|
| all-ones bypass | 255 |
| modelled PCF50605 | **253** |

**The PMU was not causing the boot loop.** [research/12](12-bypass-ledger.md) ranked #3 first on the
reasoning that a device answering plausibly-but-falsely lets a constructor succeed against hardware
that never replied. That reasoning was sound and the answer is still no — 253 against 255 is the
same machine.

Which is worth more than a confirmation would have been. §36 established these bypasses could only
be *replaced*, not A/B tested; this is the first one actually replaced, and it buys a real
elimination. The object nobody constructed is not the PMU's doing. **#6 — the synthesised BCM
replies — is now the sole remaining suspect of that class**, and it is a much larger job: it means
executing the `vmcs` firmware.

Two smaller things fixed on the way, both genuine hardware behaviour rather than accommodations: the
ADC result registers are **read-only** (letting writes land on them lets the firmware overwrite the
value it is about to poll for), and a conversion **takes observable time**.

## 39. #6 is eliminated too — and §34's framebuffer was the bootloader's

The BCM did not need a VideoCore emulator to be ruled out. It needed an attribution test, which
costs one run:

| | BCM commands | frames | halfwords written | internal reads |
|---|---|---|---|---|
| never reaches RetailOS (`--i2c-fill=0x00`) | 4 | 2 | 132 548 | 32 |
| boots RetailOS, restarts 253× (`--pmu`) | 4 | 2 | 132 548 | 32 |

**Byte-identical.** A run that dies inside Apple's bootloader produces exactly the traffic of a run
that loads RetailOS and restarts it 253 times. Every BCM interaction on this path belongs to the
**bootloader**. RetailOS never talks to the video co-processor at all.

Two consequences.

**#6 cannot be the boot loop.** You are not broken by a device you never opened. The synthesised
replies remain wrong, and executing `vmcs` remains the right long-term answer for the *display* —
but it is not what leaves an object unconstructed.

**§34 was wrong, and it is worth being blunt about how.** That section reported RetailOS writing a
320×240 RGB565 framebuffer and called it "It renders". The arithmetic was right — 76 815 halfwords
against 320 × 240 = 76 800 — and the conclusion did not follow. It is Apple's **boot logo**. The
error was attributing a cumulative counter to whichever component was interesting at the time,
without asking which component actually produced it. One run answers that, and it was never run.

### Both suspects are now exhausted

| suspect | verdict | how |
|---|---|---|
| #3 PMU | **cleared** | replaced with a real chip; 253 restarts vs 255 (§38) |
| #6 BCM | **cleared** | attribution: RetailOS never touches it |

[research/12](12-bypass-ledger.md) named these two on the reasoning that a device answering
plausibly-but-falsely lets a constructor succeed against hardware that never replied. The reasoning
was sound and **both answers are no**. The object nobody constructed is not a bypass artefact.

So the next move is not another device. It is to find where that object *should* have been built:
the first crash returns to `0x000fb8ec`, and the constructor that did not run is upstream of it.
That is a debugging problem in RetailOS itself, which is what Ghidra was set up for.

## 40. There was no unconstructed object. The disk image was one sector short.

The plan at the end of §39 was to open RetailOS in Ghidra and find the constructor that never ran.
Before doing that I re-measured the boot loop, because §39 had just invalidated both explanations
for it and a symptom whose every candidate cause has been eliminated deserves to be re-observed
rather than re-theorised.

**It is gone.** With the current tree and the current disk, over 400 M instructions:

| configuration | arrivals at `0x00000000` |
|---|---|
| `--i2c-fill=0xff` (the old bypass) | 1 |
| `--pmu` (the modelled chip) | 1 |

One arrival is the cold reset itself. There is no loop, in either configuration.

### Finding what actually changed

Two candidates were in the emulator, both added after the boot-loop commit: SEND became
self-clearing, and the I²C write hook stopped dying after 4096 transfers. Both were tested and both
are innocent — patching the SEND clear back out still gives 1 arrival, and so does **the binary
built from `078615d` itself**, the very commit that reported the loop.

So the emulator was never the cause. The inputs changed. `resources/derived/disk/ipod8g.img` and
`resources/derived/recovery/osos_region_backup.bin` share a timestamp — 13 Aug 02:05 — which is the
firmware-partition surgery, performed *after* the loop was measured and never re-measured against.

| image | sha256 |
|---|---|
| `fw/OSOS_correct.bin` | `f7b767…` |
| **osos region on disk, now** | **`f7b767…`** |
| `recovery/osos_region_backup.bin` — what was there during the loop | `4682d1…` |

And the relationship between the two is exact:

```
backup[i] == correct[i + 512]      for every byte
```

**The osos body on disk was missing its first sector.** Every address in RetailOS was shifted down
by 512 bytes.

### Why that produced precisely the observed symptom

The correct image begins `7a0000ea 670000ea dcf09fe5 6b0000ea` — `b`, `b`, `ldr pc,[pc,#-0xdc]`,
`b`. That is the **ARM exception vector table**: reset, undefined instruction, SWI, prefetch abort.
The broken image began `dc009fe5 dc109fe5 001080e5` instead, which is ordinary code — the correct
image's offset 512.

So RetailOS ran with **no vector table at address 0**, and with every function entry half a
kilobyte off from every pointer to it. §35 described "a vtable read landing in RetailOS's own
exception vectors, an unmapped slot returning 0, `bx 0`, and a jump to the reset vector." That is
what a one-sector shift looks like from the inside. It was never an object nobody constructed.

### What this costs, and what it is worth

Three sections' worth of diagnosis — §35, §36, and the framing that opened §37, §38 and §39 — were
explaining a corrupted input. The eliminations in §38 and §39 are still valid as measurements (the
PMU really is faithful now; RetailOS really does not touch the BCM) but the *question* they were
answering did not exist.

The error was not the diagnosis; a 512-byte shift genuinely looks like a null vtable. The error was
**fixing the disk and not re-running the failing measurement**. The repair was made for a different
reason, in the same session, and the symptom it silently cured stayed in the notes as fact for three
more sections. A symptom is only as current as its last observation.

### Where RetailOS actually gets to

It boots, and then it idles:

```
000af4a4  bl 0x000c1648      ; tick accounting: disable IRQs, bump a 64-bit counter, restore
000af4a8  b  0x000af4a4
```

It reaches that loop at **119.7 M instructions** and stays there, with 3 772 interrupts taken and
1 844 distinct code buckets sampled. The 59 ATA transfers on the trace are all the *bootloader*
loading osos; RetailOS itself issues none. That is the RTXC idle task with every other task blocked
— a booted OS with nothing to run, not a crashed one.

The question is no longer "what crashed" but **"what is every task waiting for"**.

## 41. What RetailOS actually is when it idles: 18 named RTXC tasks, all blocked

§40 left the question "what is every task waiting for". Answering it needed names, and RetailOS
turns out to carry them.

### The RTXC task registry is a symbol table

At `0x0025d63c` there is a run of records, each a NUL-terminated name padded to a 4-byte boundary
and followed immediately by a pointer to that task's entry point:

```
0025d6ac  "DiskMgrTask\0"        0c 4b 28 00   ->  0x00284b0c
0025d6bc  "HoldSwitchTask\0\0"   30 4f 28 00   ->  0x00284f30
```

`extract_symbols()` in the loader now recovers these by scanning for the pattern and accepting the
pointer only when the word it points at is an ARM function prologue. `--symbols` prints them. This
is the first symbol information this project has had for RetailOS at all.

### Every task starts, once

Breakpointing all 20 recovered entry points over a 400 M-instruction boot:

| started (×1 each) | did not start |
|---|---|
| AlarmTask · ATAWorkLoopTask · AsyncPiezo · BacklightTask · CNATask · DiskMgrTask · EventManager · FirewireTask · HPhoneDetTask · HoldSwitchTask · OptoTask · PCFPowerMgr · RTCTimerMgr · SerialOptoTask · TopPlugTask · USBAudioTask · USBPowerSense · WatchdogTask | USBStatusTask · USBTaskTimeTask |

Eighteen of twenty, each entered exactly once and then blocked — which is what a task does when it
runs to its first wait. The two that did not start are the USB pair, and nothing is plugged in.

**Every one of these is a device or housekeeping task.** There is no UI task, no display task, no
graphics manager, no filesystem mount in the set. RetailOS brought up its hardware abstraction layer
and stopped before the application layer.

### The disk works, and RetailOS chose not to use it

Splitting the ATA log at the handover (`--stop-at=0x10000000:1`) separates the bootloader's disk
activity from RetailOS's:

| | bootloader | RetailOS |
|---|---|---|
| DMA transfers | 59 (7 563 264 bytes) | **0** |
| ATA commands | 68 | **2** |

Its two commands are `0xec` IDENTIFY and `0xef` SET FEATURES subcommand `0x03`, transfer mode
`0x0a` — Multiword DMA mode 2. So it identified the drive, negotiated DMA, and issued no reads.

That looked like a driver stalled waiting for a completion interrupt, which would have implicated
bypasses #9/#10. It is not. Instrumenting the drive's interrupt line specifically:

```
ide irq: raised 129 times, DELIVERED to a handler 2 times, ... enabled=1 pending=0
```

The bootloader runs with the IDE interrupt **disabled** (`enabled=0`, 0 interrupts taken in
101 M instructions — it polls). RetailOS enables it, and both of its commands' completions were
delivered to a handler. **The interrupt path works.** The storage stack initialised successfully and
is idle because nothing has asked it for a sector.

### The idle loop is a correct idle loop

```
000af4a4  bl 0x000c1648      ; tick accounting, then CPU_CTL = 0x80000000 (PROC_SLEEP)
000af4a8  b  0x000af4a4
```

`CPU_CTL` at `0x60007000` takes **18.6 M writes and zero reads** across the run — the idle task
sleeping the core, woken by the timer. `USEC_TIMER` at `0x60005000` takes 58.9 M reads. This is a
healthy RTOS with an empty run queue, not a hang.

### Two more measurements that came out clean

**The BCM attribution test was re-run on corrected inputs.** §39's version compared against a
RetailOS that crashed instantly, so it proved nothing. Repeated against a RetailOS that boots and
runs 400 M instructions, the BCM counters are still byte-identical to the bootloader-only run —
4 commands, 2 frame updates, 132 548 halfwords. The conclusion survives: **RetailOS never touches
the video co-processor.** Consistent with there being no display task.

**The PMU is not being waited on.** RetailOS reads exactly one PCF50605 register in bulk — `0x34`,
BVMC, 26 634 times. Forcing it to `0x00`, `0x01`, `0x02`, `0x04`, `0x80` and `0xff` produces
byte-identical runs (same instruction count, same 1 840 profile buckets). It is a periodic monitor
whose value gates nothing here.

### Where this leaves it

RetailOS is up. Its HAL is up. Its disk is initialised and idle. Nothing it is waiting for is a
device this emulator gets wrong, as far as any measurement so far can tell. What is missing is
whatever **starts the application layer** — and that is a question about RetailOS's own boot
sequence, not about hardware.

## 42. RetailOS's boot is paced by simulated time, and it does read its disk

§41 ended with "nothing it is waiting for is a device this emulator gets wrong". That was true and
it was the wrong frame. RetailOS is not waiting for a *device*. It is waiting for **time**.

### The tell

Running to 3 G instructions instead of 400 M — 40 simulated seconds instead of 5.3 — is not a
steady state. Three more ATA commands appear, and they are a continuation of a sequence:

| # | command | meaning |
|---|---|---|
| 69 | `0xef` features `0x03`, nsector `0x0a` | SET FEATURES — transfer mode Multiword DMA 2 |
| 70 | `0xef` features `0x02` | SET FEATURES — enable write cache |
| 71 | `0xef` features `0xaa` | SET FEATURES — enable read look-ahead |
| 72 | `0xef` features `0x05`, nsector `0x01` | SET FEATURES — advanced power management, level 1 |

That is an ordinary, healthy drive-configuration sequence. What is not ordinary is the spacing:
roughly **ten simulated seconds between consecutive steps**. Nothing about setting a feature bit
takes ten seconds. The sequence is being advanced by something periodic.

### The experiment

`--clock=N` sets instructions per simulated microsecond; 75 is the faithful ratio. At `--clock=5`,
simulated time advances 15× faster relative to the instruction budget. If the boot is paced by
simulated time, the *same* instruction budget should reach much further.

At 400 M instructions, holding everything else fixed:

| | `--clock=75` (faithful) | `--clock=5` |
|---|---|---|
| ATA commands | 70 | **82** |
| ATA DMA transfers | 59 — all the bootloader's | **60** |
| IDE interrupts delivered | 2 | **16** |
| profile buckets executed | 1 844 | **5 094** |

And the sixtieth transfer is the one that matters:

```
[ 78] cmd 0xc8  features 0x00  nsector 0x04  lba 0
```

**READ DMA, four sectors, from LBA 0.** RetailOS read its own partition table. Nearly three times
as much code ran, for the same number of executed instructions.

### What this means, stated carefully

It does **not** mean the emulator was broken. It means this interpreter is roughly three orders of
magnitude slower than the hardware in wall-clock terms, so reaching a given point in RetailOS's
*boot* costs an instruction budget proportional to how long that boot takes in *its* seconds. A
5.3-second window was never going to be enough, and every conclusion in §41 that took the form
"RetailOS never does X" is really "RetailOS had not done X within 5.3 simulated seconds."

Three of those need re-testing at reach, not repeating: the 18-task set was complete at 5.3 s and
may not be; "RetailOS never touches the BCM" may simply mean the display comes up later; and
"zero disk transfers" is already falsified.

`--clock=5` is a **fidelity knob, not a fix** — it is already in
[research/12](12-bypass-ledger.md) with the warning that timing-sensitive code can notice it. It
buys reach for exploration. Any milestone found with it has to be confirmed at `--clock=75` with a
budget large enough to hold it, and until it is, it is a lead rather than a result.

### The disk it is reading is real

The partition table it just read describes a populated, Windows-formatted iPod:

- partition 0, type `0x00` — the Apple firmware partition the bootloader took `osos` from
- partition 1, type `0x0c` (FAT32 LBA), LBA 32768, 8 176 MB — containing `IPOD_C`, `ITUNES` and
  **`GAMES`**

So the filesystem RetailOS is about to mount has the games on it. Nothing further along this path is
blocked on missing content.

## 43. At reach: more tasks, a partition-table read, and two-thirds of the time in memcpy

Running 4 G instructions at `--clock=5` — 800 simulated seconds, thirteen minutes of iPod time —
moves several things that were static at 5.3 s.

**More tasks start.** `TrackCacheReadTask` runs once and `TimerEventManager` four times, neither of
which appeared in §41's set. So §41's "eighteen tasks, that is the whole set" was a statement about
a 5.3-second window, exactly as §42 warned.

**The drive sequence repeats.** After the READ DMA of LBA 0, the four SET FEATURES commands are
issued *again* in the same order, and the run ends with the IDE interrupt **disabled**
(`enabled=0`, having been 1 earlier). Re-initialising a drive and then masking its interrupt is
what a driver does when it resets and spins down — either normal iPod power management, or a
recovery path. Which of those it is, is not yet measured.

**The scatter-load IS looping — the claim below is retracted, see §47.** 64.5% of all sampled time
sits in two adjacent buckets:

```
0008438c  cmp r1, r2                    ; memcpy — 18.9%
00084390  ldrcc r3, [r0], #4
00084394  strcc r3, [r1], #4
00084398  bcc 0x0008438c

000843a0  mov r2, #0                    ; bzero — 45.6%
000843a4  cmp r0, r1
000843a8  strcc r2, [r0], #4
000843ac  bcc 0x000843a0
```

Their direct callers are a dense block of alternating `bl` pairs at `0x83860..0x83a78` — the ARM
scatter-load table runner, C startup. That is a natural suspect for "the boot is restarting", and
the README already records one earlier episode where repeated scatter-loading *was* a boot loop. It
is not this time: breakpointing `0x00083860` gives **exactly one hit** at 400 M/`clock=75`,
400 M/`clock=5` and 2 G/`clock=5`. RetailOS relocates itself once.

> **RETRACTED (§47).** Those three runs were the same run. The shell loop used `set -- $cfg` to
> split a "budget clock" pair, and **zsh does not word-split unquoted parameters** — so `$1` held
> the whole string, `$2` was empty, and all three invocations got an unparseable budget and an
> empty `--clock=`, falling back to the defaults. Measured properly, `0x00083860` is reached
> **522 times** over 2 G at `clock=5`. RetailOS relocates itself once per reset, and it resets
> constantly.

So these are the generic `memcpy` and `memset`, called from everywhere, and RetailOS is spending
two thirds of its execution moving and clearing memory — on the order of 1.8 billion instructions
of `bzero` alone. On a 64 MB machine that is not a one-off; something is clearing buffers over and
over. A 320×240×2 framebuffer clear is 153 600 bytes, which at four instructions per word would put
the observed volume at roughly fifteen clears per simulated second — a plausible redraw rate, and a
hypothesis worth testing rather than believing, since §34 already produced one wrong framebuffer
claim from exactly this kind of arithmetic.

The test is not arithmetic. It is where the writes land, which is a measurement.

### One small anomaly worth recording

At `--clock=5` a new unmapped access appears: 16 reads of `0xea000078..0xea00007b`, first from PC
`0x000a09b4`. `0xea000078` is not a plausible address — it is an ARM `b` instruction word being
used as a pointer. Small, but it is a pointer read out of code, and those are worth naming before
they become a mystery.

## 44. The buffers are cleared and stay empty — and there is a real garbage vtable

### The framebuffer hypothesis is dead

§43 proposed that the `bzero` volume might be a redraw loop clearing a 320×240 framebuffer. Mapping
SDRAM writes at 64 KB granularity over 2 G instructions finds the write-mostly regions it predicted:

| bucket | reads | writes |
|---|---|---|
| `0x108d0000` | **418** | 36 624 912 |
| `0x108b0000` | 231 480 | 50 167 632 |
| `0x10a00000` | 216 332 287 | 217 263 593 |

`0x108d0000` is written 36.6 million times and read 418 times — 87 000 : 1, exactly the signature a
framebuffer would have. So the arithmetic and the access pattern both pointed the same way.

**Then I dumped it, and it is all zeros.** So is `0x108b8000`. These buffers are cleared repeatedly
and **nothing is ever painted into them**. Not a framebuffer being redrawn — a buffer being cleared
by a pipeline that produces no output. (`0x10a00000`, with reads and writes balanced to within half
a percent, is ordinary heap or stack.)

This is the third framebuffer claim in this file to be checked and the second to die. The pattern
worth naming: **a ratio that matches a hypothesis is not the hypothesis**, and the dump costs one
command.

### A genuine uninitialised vtable

The unmapped-access report grows with reach — 16 reads at 400 M, **2 100 at 2 G** — and 2 080 of
them come from one PC. Disassembled from the running machine:

```
00183e64  ldr r0, [r4, #0x440]   ; an object
00183e6c  ldr r0, [r0, #0x1c]    ; a sub-object
00183e74  ldr r1, [r0, #0x0]     ; its vtable pointer   -> 0xea00001c
00183e7c  ldr r12, [r1, #0x5c]   ; vtable slot 23       <- the unmapped read
00183e88  mov lr, pc
00183e8c  bx  r12
```

`r1` is the vtable pointer, and it is `0xea00001c`. That is not an address; it is an ARM `b`
instruction word. So the object at `[r0+0x1c]` has a vtable slot holding **code bytes rather than a
vtable** — either its constructor never ran, or the field is not the object pointer this code
believes it is.

Two things this is **not**. It is not §35's crash: that was the one-sector shift, and it is gone.
And it does not reach the reset vector — `--break=0x00000000` still records exactly one arrival,
the cold reset, even at `--clock=5` where the fault fires 2 080 times. Whatever `bx r12` branches
to, it survives.

So RetailOS is making a virtual call through an uninitialised object, thousands of times, and
carrying on. That is a real defect on a live path, it is in the same subsystem as
`TimerTaskClass` (`0x00184010`) and `ATAWorkLoopIRQTask` (`0x00186be4`), and it is the first thing
in this investigation that is unambiguously wrong rather than merely unfinished.

## 45. It flatlines at 4 G, and the drive was advertising nothing

### The steady state is real, not just slow

§42 established the boot is paced by simulated time, which made "it hasn't done X yet" the
explanation for everything. That has a limit, and 12 G instructions at `--clock=5` — **2 400
simulated seconds, forty minutes of iPod time** — finds it:

| | 4 G | 12 G |
|---|---|---|
| profile buckets executed | 6 028 | **6 028** |
| ATA DMA transfers | 60 | **60** |
| tasks newly started | — | `MP3ExampleTask`, `LoadDataTasks` |

**Identical bucket count over eight billion further instructions.** Two more tasks start and then
nothing new executes for twenty-seven more simulated minutes. RetailOS reads LBA 0 once and never
touches the disk again, ending with the IDE interrupt masked. This is a genuine stop, and no amount
of additional reach will move it.

The unmapped-read count keeps climbing with time (15 472 by 12 G, 15 452 of them from the
`0x00183e7c` vtable call in §44), so the machine is still *running* — it is looping, not hung.

### A drive that advertised no DMA at all

Reading our own `identify()` against what RetailOS asks for turned up something plainly wrong. Word
53 was set to `0x0007`, which asserts *"words 64-70 and 88 are valid"* — and words 62, 63, 64, 65-68
and 88 were all left **zero**. That is a drive claiming its transfer-mode fields are meaningful and
then reporting no DMA capability, no PIO mode and no cycle timings, while answering SET FEATURES
"transfer mode = Multiword DMA 2" with success. No such drive exists.

Now modelled properly: multiword DMA 0-2 and ultra DMA 0-4 supported, PIO 3-4, real cycle times,
and — the part that matters — the **high byte of words 63 and 88 reports the mode SET FEATURES
actually selected**, which is how a driver confirms the mode it just asked for took effect.

**RetailOS noticed.** Its transfer-mode negotiation changed:

| | before | after |
|---|---|---|
| mode requested | `0x0a` — multiword DMA 2 | `0x44` — **ultra DMA 4** |
| ATA commands | 82 | 87 |
| IDE interrupts delivered | 16 | 19 |

So RetailOS does read words 63 and 88 and does adapt to them. That is worth having: it retires a
known-wrong device model, and it proves the drive's advertisement is on a live path rather than
being ignored.

**It did not unblock the boot.** Still 60 DMA transfers, still 6 033 buckets against 6 028. The
fix is correct and the wall is somewhere else — recorded here so the next person does not re-run
this experiment hoping for a different answer.

## 46. The boot ends in a dispatch through an object nobody constructed

Chasing the unstarted subsystems statically kept dead-ending. `DiskReaderTask`'s creator is reached
only from `0x162764`, which is **virtual slot 0** of a vtable at `0x669d44`; that vtable *is*
installed, by a constructor at `0x162b08` which runs exactly once — and **none of its four virtual
functions are ever called**. The object is built and never used. The global it is stored into,
`0x10831818`, has exactly two references in the whole image: the constructor that writes it, and one
reader, `FUN_001e0084` — which never runs, and which has **no static callers at all**, being itself
reached only by virtual dispatch.

That is three dead ends in a row, and it is the same dead end each time: RetailOS is C++ and its
control flow is not in the call graph.

### The instrument that was missing

A profile says where the *time* went. It cannot say what the machine did **last**. So `--novelty`
records the instruction count at which each 16-byte code bucket first executes — a bitset on the
fast path so the map is only touched on genuinely new code.

Over 4 G instructions at `--clock=5`, **19 660 buckets** execute (the sampled profile saw 6 028).
The last new code runs at **494 874 178** instructions, and one further bucket at **497 865 658**.
After that, **nothing new executes for three and a half billion instructions.**

The final entries are not arbitrary:

```
494 874 163 …178   0x000fb8a0 .. 0x000fb8e0
497 865 654 …658   0x00183e70 .. 0x00183e80
```

The second is §44's garbage-vtable virtual call. The first is `0x000fb8d4` — **the address §35
originally fingered.**

### What that code is

Disassembled from the running machine, it is the generic ARM **pointer-to-member-function thunk**:

```
000fb8ac  ldr   r6, [sp, #0x40]   ; the PMF's adjustment word
000fb8bc  mov   r10, r6, asr #1   ; adj >> 1 = this-pointer adjustment
000fb8c4  tst   r6, #0x1          ; bit 0 set = the member is VIRTUAL
000fb8c8  add   r0, r10, r8       ; the adjusted `this`
000fb8cc  ldrne r1, [r0, #0x0]    ; virtual: load the vtable pointer
000fb8d4  ldrne r2, [r7, r1]      ; virtual: load the slot        <- the unmapped read
000fb8e0  moveq r2, r5            ; non-virtual: the function directly
000fb8e8  bx    r2
000fb8ec  movs  r4, r0            ; <- the return point
```

`0x000fb8ec` being the return point is exactly what §35 reported. That section's *cause* was wrong —
it was the one-sector shift, and the boot loop it described is gone — but its **mechanism** was
right, and the same mechanism is still firing on the corrected image: a member-function call
dispatched through an object whose vtable pointer is not a vtable.

So the picture is now consistent across three independent observations:

| site | kind | count at 12 G |
|---|---|---|
| `0x00183e7c` | virtual call, vptr `0xea00001c` | 15 452 |
| `0x000fb8d4` | PMF thunk, virtual branch | 4 |

**RetailOS's boot ends in dispatch through uninitialised objects, and then loops forever.** The
subsystems that never start — `DiskReaderTask`, `VCUpdateTask`, `eAppMotor` — are downstream of
exactly this kind of call. "Something constructed nothing" was the right instinct in §35; it took
removing the corrupted input, and building a first-execution timeline, to earn it back.

The next question is narrow and answerable: **which object**, and **which constructor should have
run**. The caller that passes it is one frame above `0x000fb8a4`, and the run reaches that frame at
a known instruction count, which `--stop-at` can halt on.

## 47. There IS a boot loop. It is a member-function call on a null object.

§35 said RetailOS boot-loops through a bad vtable. §40 retracted that, having proved the image on
disk was missing a sector. Both were right about their own evidence and **the retraction was
over-generalised**: fixing the sector shift removed *that* loop, and a second loop — same mechanism,
different cause, ninety-four times further into the boot — was sitting behind it the whole time.

### The fault, exactly

The generic ARM pointer-to-member-function thunk at `0x000fb8a4` is entered **once** in the whole
boot. Halting on `0x000fb8e8`, the instruction that dispatches:

```
r0 = 00000000     the adjusted `this`  — NULL
r2 = 00000000     the vtable slot loaded from it
r6 = 00000001     adj bit 0 set: the member IS virtual, so the load really happened
r3 = 0000001c     the PMF's ptr field: a vtable offset, not a function address
```

`bx r2` with `r2 = 0` branches to address `0`, which post-MMAP is **RetailOS's own exception vector
table**, whose first word is `b reset`. So it resets itself. That is §35's mechanism, verbatim, on
a corrected image.

The object is genuinely null — and the second half of the sentence that used to be here, "genuinely
never initialised", is **wrong**; see the correction under the bullets.

- the caller passes a 2-word handle at `0x13e26f8c`; memory there reads `00 00 00 00`
- ~~`--watch=0x13e26f8c` observes **no writes** across the whole run~~
- the call chain is `0x000cd87c` → `0x000caa80` → `0x0011e16c` → the thunk, and **none of those
  return addresses is ever reached** — the call never comes back

> **Corrected 2026-08-13.** The `--watch` bullet is the same false negative §52 carries a banner for,
> and it is the same word: `0x13e26f8c` *is* `object + 0x20`. `--watch` reports value **changes**, so
> three writes of zero to an already-zero word were invisible. [research/20](20-the-resource-image.md)
> §2 settled it with `--storeaddr` — **95 456 stores** across the 790 objects of this shape, every
> object written, this one written three times (`0x0017f188` allocation zeroing, `0x00210684` the
> initialiser, `0x00211778` the delegate setter). The object is **initialised and then handed a null
> delegate**, which is a different bug from "never initialised" and points at the caller rather than
> at a missing constructor. The null is deliberate at the setter (§3 of that addendum) and originates
> in a **failed font lookup**. The other two bullets stand.

`0x000caa80` sits immediately before `USBStatusTask` (`0x000caab0`), so this is the USB subsystem —
and `USBStatusTask` and `USBTaskTimeTask` are exactly the two tasks §41 recorded as never starting.

### The count

| tree | arrivals at `0x00000000` over 2 G at `clock=5` | scatter-loads |
|---|---|---|
| with the §45 IDENTIFY fix | 505 | 505 |
| with it reverted | **522** | 522 |

So the loop is **not** caused by the IDENTIFY fix — it is present either way, and the fix is
exonerated. It was worth checking, because the fix was the most recent change to the drive model
and "my last change broke it" is the correct first suspicion.

### Why §43 said the opposite, which is the part worth keeping

§43 reported the scatter-load running **exactly once** at three different budget/clock combinations,
and concluded RetailOS relocates itself once. That was produced by this shell loop:

```sh
for cfg in "400000000 75" "2000000000 5"; do set -- $cfg; ... done
```

**zsh does not word-split unquoted parameters.** `$1` received the entire string and `$2` was empty,
so every iteration ran with an unparseable `BUDGET` and an empty `--clock=` — the defaults, three
times over. Three identical runs, reported as three different configurations agreeing with each
other. The agreement was the tell and I read it as corroboration.

This is the same failure this file has already recorded twice — a non-measurement presented as a
measurement — and the specific lesson is narrower than "be careful": **a loop that varies its
parameters must echo the parameters it actually used.** The printf in that loop did print them, as
`budget=400000000 75 clock=:` — the empty `clock=` was visible in the output I read and I did not
look at it.

### Where this leaves the boot

RetailOS gets ~99 simulated seconds in, calls a member function on an object the USB subsystem never
constructed, branches through a zero vtable slot to its own reset vector, and starts over. Every
"RetailOS never does X" in §41–§46 has to be read against that: it is not idling, it is **cycling**.

~~The next question is the same one §46 ended on and it is now much sharper: the handle at
`0x13e26f8c` is never written, so **what should have written it**, and why did that not run?~~
**Corrected 2026-08-13:** it *is* written — three times, all zero, the last by the delegate setter
at `0x00211778`. The question that actually follows is what the setter was *handed*, and
[research/20](20-the-resource-image.md) answers it: a null that arrives pre-formed from a failed font
lookup. "What should have written it" was a question about a write that was already happening.

## 48. A false breakthrough on the GPIO, caught by the follow-up run

The null object in §47 is reached from `0x000caa80`, immediately before `USBStatusTask`, and this
emulator models **no USB at all**. GPIO input `0x6000d13c` is read 53 276 times and our machine
answers `0` — so "an active-low detect line is reading as asserted, RetailOS tries to bring up a USB
stack that is not there, and calls a delegate nobody bound" was an attractive story.

The first measurement appeared to confirm it outright:

| `0x6000d13c` | arrivals at `0x00000000` | PMF fault |
|---|---|---|
| default (`0`) | 37 | fires |
| forced `0xffffffff` | **1** — the cold reset | **never** |

**That is not a fix. The machine never gets there.** The follow-up run says so plainly: with the
GPIO forced high, execution stops making progress at **238 829 instructions**, every PC in
`0x40002xxx` — inside Apple's bootloader, in IRAM. No banner, no `Retail mode`, `osos` never loaded,
0 BCM commands, 0 ATA transfers, 114 profile buckets against 19 660.

This is the **exact** failure mode §36 recorded for the other bypasses — *"removing either one
strands the run inside Apple's bootloader, before RetailOS is reached at all"* — and knowing that,
having written it down, was not enough to stop me reading a zero as a cure. The thing that caught it
was running the second measurement before writing up the first.

### The clean negative

Bisecting the mask, at 600 M instructions and `--clock=5`:

| mask | bootloader completes | resets | fault |
|---|---|---|---|
| `0xffff0000` | yes | 37 | fires |
| `0xff000000` | yes | 37 | fires |
| `0x00ff0000` | yes | 37 | fires |
| `0x0000ff00` | yes | 37 | fires |
| `0x0000f000` | yes | 37 | fires |
| `0x00000f00` | yes | 37 | fires |
| `0x000000ff` | **no** | 1 | never reached |
| `0x0000ffff` | **no** | 1 | never reached |

Bits 8–31 are harmless to the bootloader and **change nothing about the fault**. Bits 0–7 hang the
bootloader. There is no value of this line that both boots and avoids the null dispatch.

**`0x6000d13c` is not the cause.** It is a line the bootloader depends on and RetailOS polls, and
the fault is indifferent to it.

~~What survives is the sharper form of the question: the two-word delegate at object `+0x20`
(`0x13e26f8c`) is never written by anybody, the constructor at `0x00211a70` writes `+0x1c` and
`+0x34..+0x40` and skips it, and the caller invokes it unconditionally. The missing write is the
target, not the hardware around it.~~

**Corrected 2026-08-13 — there is no missing write.** `+0x20` is written three times on this object
and on all 790 of its shape ([research/20](20-the-resource-image.md) §2, `--storeaddr`, 95 456
stores). The `--watch` zero that made it look unwritten is a value-**change** report, and three
stores of zero into a zero word are not a change. What survives is only the last clause: the caller
invokes the delegate unconditionally, and the delegate it invokes is null. The GPIO negative in this
section is unaffected — it was a bisect over behaviour, not a write measurement.

## 49. The null delegate is one entry in a list being walked

The frame above the call is a loop, not a one-shot:

```
000cd868  b   0x000cd880       ; enter the loop at its test
000cd86c  ldr r2, [sp, #0x4]   ; loop body
000cd870  mov r3, r5
000cd874  mov r1, r6           ; r6 = the delegate pointer — 0x13e26f8c
000cd878  bl  0x000caa80
000cd87c  add r4, r0, r4       ; accumulate each call's result
000cd880  add r0, sp, #0xc     ; …test and advance
```

So RetailOS is **iterating a container of registered callbacks and invoking each one**, summing what
they return. One of the entries has a null object bound to it, and the walk does not check.

That reframes the missing write once more, and more usefully: the question is not "which constructor
forgot a field" but **"what registered an entry into this list without binding its target"** — or,
equally, what should have *removed* it. Both are answerable with a watch on the container rather
than on the object, which is where this picks up next.

## 50. USB is not the blocker, and "USB subsystem" was an attribution error

§47 and §48 called the failing code "the USB subsystem" because `USBStatusTask` sits `0x30` bytes
after the failing function. That is **adjacency, not identity** — and the name does not even attach
that way. `USBStatusTask` was recovered by the *task-registry* pattern (a name elsewhere in the
image paired with a pointer), not by a label immediately preceding the code. Searching the 768 bytes
before `0x000caa80`, `0x000cd854`, `0x0011e16c` and `0x00211a70` finds **no label at all**. Every
function on this chain is unnamed. I attached a subsystem to it on the strength of a neighbour.

### The measurement that settles it

Over 2 G instructions at `--clock=5`, the regions RetailOS touches are:

```
osos  sdram  iram  mmio-6  cache  flash-low  mmio-7  lcd  stack  ide
```

There is **no USB region**, and there is no unmapped access that could be one. The only unmapped
page in the entire run is `0xea000078..0xea0000d7` — the null-vtable reads themselves. The PP502x
USB controller lives at `0xc5000000`, which no region covers, so any access to it would be recorded
as unmapped and named by PC. **Zero USB register accesses in two billion instructions.**

So a faithful USB controller model has nothing to talk to on this path. It would change nothing,
because RetailOS is not trying to drive USB hardware — it is calling an unbound callback out of a
list, and the list walk is indifferent to what the callback was for.

`USBStatusTask` and `USBTaskTimeTask` remain the only two RTXC tasks that never start (§41), which
is the *correct* behaviour for a device with nothing plugged into it. RetailOS's handling of "no USB
present" looks right; the fault is elsewhere.

> **Re-checked on the retail path 2026-08-14 (research/20 Addendum 14 §9). The register half
> survives; the task half does not, and the combination is more interesting than either.**
> `0xc5000000` is still covered by no region, and the only unmapped page in a 600 M retail baseline
> is `0xea000078` — so **zero USB register accesses** holds on the configuration this project now
> measures, not just on the prototype path this section was written against. But `USBDeviceTask`
> (@49 705 952) and `USB MSC` (@52 178 917) **are** created, with TCBs, logged by
> `--enterlog=0x0011c808`. A USB driver task that runs for a whole boot and touches not one USB
> register is a different statement from "nothing asks for USB" — it is a driver waiting on
> something it never gets, and the `0x6000d13c` GPIO presence lines we answer as permanently zero
> (research/19) are the obvious candidate. Not chased here; recorded so it is not re-derived.

## 51. Stepping over the wall: the unbound delegate is central, and there is a second bad pointer

The dispatch through a zero vtable slot ends every run, which makes everything behind it invisible.
`--null-dispatch=survive` steps over it exactly once per occurrence: a `BX` to address `0` is
reported to the caller as **a null return** (`r0 = 0`, continue at `lr`) instead of branching to the
reset vector. That is the value the caller's own error path at `0x0011e1d0` already tests for.

**A diagnostic, never a fix.** The fault is real and modelling it away would be lying. The point is
that the only way to see what is behind a wall is to step over it once.

| 4 G instructions, `--clock=5` | faithful | `--null-dispatch=survive` |
|---|---|---|
| arrivals at `0x00000000` | ~500 | **1** (the cold reset) |
| profile buckets | 6 033 | **10 134** |
| distinct code buckets executed | 19 660 | **26 522** |
| IDE interrupts delivered | 19 | 25 |
| last new code at | 497 M | **2 150 M** |

So RetailOS runs **68% more code** and keeps finding new code until well past the halfway point.
It does not reset. `LoadDataTasks` starts. And it still does not start `VCUpdateTask`,
`DiskReaderTask`, `eAppMotor` or `eAppAsyncIO`, still issues no further disk transfers, and still
never touches the BCM.

### What the step-over reveals

**The fault is not incidental — it is the main event.** `0x000fb8d4` fires **55 108 times**, against
4 when the first occurrence killed the run. RetailOS calls the unbound delegate, gets null, takes
its error path, and comes back to do it again. It is limping, not progressing.

**A second garbage pointer appears.** A new unmapped page, `0x83805d62..0x83805d6d`, 9 800 reads,
first from PC `0x000dcf64` (1 400 of them). `0x83805d62` is not an address any more than
`0xea00001c` was — another read through a field that was never initialised.

### What this settles

Surviving the null dispatch is not a route to a working RetailOS; it is a route to a RetailOS that
fails the same way 55 000 times. That is worth knowing, because it rules out the cheap answer —
"absorb it and move on" — and it confirms the delegate is the thing to fix rather than one symptom
among many.

The binding is the target. The function half of the call comes from a global constant at
`0x004f3fb4`; the **object** half is read from `0x13e26f8c` and is null. So the question is narrower
than "what constructs this": it is **what should have stored an object pointer there**, and the
answer is not in the constructor at `0x00211a70`, which writes `+0x1c` and `+0x34..+0x40` and skips
`+0x20`.

## 52. The object is barely constructed at all — one field out of the whole thing

> **Superseded 2026-08-13 — the central measurement in this section is wrong.** The table below
> reports `+0x00`, `+0x10` and `+0x20` as "never written". They are written; `--watch` records value
> *changes*, and a write of zero to a word already zero changes nothing. `+0x20` is written three
> times, the last by the delegate setter at `0x00211778`. The object is not "barely constructed" —
> it is one of **791 identically constructed** objects, and the delegate bound into it is null
> because a **font lookup missed**. See [research/20](20-the-resource-image.md).

Chasing "what binds the delegate" produced four negatives in a row, and together they say something
stronger than any of them alone.

The object lives at `0x13e26f6c`. Read out of the running machine:

```
+0x00  00000000     <- where a vtable pointer would go
+0x08  00000001
+0x0c  00000001
+0x14  ffffffff
+0x18  000000ff
+0x1c  10883634     <- the source object
+0x20  00000000     <- the delegate's object half   } the call that kills the boot
+0x24  00000000     <- the delegate's second word   }
```

Watching individual fields across a whole run:

| field | ever written? |
|---|---|
| `+0x00` | **no** |
| `+0x10` | **no** |
| `+0x1c` | yes — once, by `0x00211a98`, `str r5, [r4, #0x1c]` |
| `+0x20` | **no** |

**One field. That is the entire construction of this object.**

And nothing outside it helps. Of the **20** call sites that reach the constructor at `0x00211a70`:

- **none** stores to `+0x20` or `+0x24` within ±0x60 bytes — nobody binds the delegate
- **none** stores to offset `0` within ±0x80 bytes — nobody installs a vtable pointer

The source object at `+0x1c` (`0x10883634`) *is* properly formed: its word 0 is `0x006670a4`, a real
vtable whose slots point into `0x0013bxxx..0x0013cxxx` — and notably slots 2, 3 and 5 of that vtable
are **null**, so unimplemented virtuals are a shape this codebase already contains.

### What this changes about the question

"Which constructor forgot a field" was the wrong frame, and so was "what should have bound the
delegate". Neither a missing field nor a missing binding explains an object with **one** initialised
word. The frame that fits the evidence is:

**this object was never meant to be used in this state.** Something upstream produced it — or handed
out a slot for it — without running whatever fills it in, and the loop at `0x000cd86c` then walks a
collection and invokes it anyway.

That is consistent with everything measured since §46: the delegate is null, the vtable pointer is
null, the call is unconditional, and stepping over it (§51) just makes the same failure repeat
55 108 times instead of once.

### The honest state of the blocker

RetailOS boots, brings up eighteen RTXC tasks, configures its drive, reads its partition table, and
then invokes a callback on an object that consists of a single assigned word. It is not blocked on a
device: the PMU, the BCM, the IDE interrupt path, DMA delivery, the drive's IDENTIFY, USB and the
GPIO next to the fault have each been measured and each ruled out. It is blocked on its own
construction order, and the next move is to find **who allocated `0x13e26f6c` and what it was
supposed to do next** — a heap-allocation watch rather than another read of the call graph.

## 53. It is an array element, not an allocation — stride 0x74

> **Superseded 2026-08-13.** The stride is real but the framing it supported is not: the objects are
> enumerated directly in [research/20](20-the-resource-image.md) §1 by watching the constructor's own
> store, and the failing one is unremarkable among 791. Nothing downstream depended on the stride.

`--retwatch=0x13e26f6c` asks which instruction *produces* that pointer, and the answer changes the
shape of the problem again:

```
0x00244a44  str r1, [r0], #0x74     lr=0x0021a424     <- post-increment by 0x74
0x0024415c  add r0, r4, #0x74       lr=0x0021ab94     <- advance by 0x74
0x0021ab8c  mov r0, r6              lr=0x0021aafc
0x0021a1e0  mov r0, r8              lr=0x00221e54
0x00211bf8  mov r0, r4              lr=0x00211a94
0x00211be4  mov r0, r4              lr=0x00211be0
```

A **stride of `0x74`**, twice, from two different sites. `0x13e26f6c` is not a heap object at all —
it is **element *n* of an array of 0x74-byte records**, and the code at `0x00244xxx` is walking that
array filling records in.

That reconciles §52's oddity. An object with exactly one assigned word is a strange thing to
allocate; it is an entirely ordinary thing to find in an array whose records are populated by a pass
that did not reach this one, or that skipped it. The stale-looking values at `+0x08`, `+0x0c`,
`+0x14` and `+0x18` fit the same reading — they belong to the record's layout, not to a constructor.

So the question is now the sharpest it has been, and it is about a **table**, not an object:

- who sizes the array at `0x0021a424` / `0x0021ab94`
- how many records it declares versus how many it fills
- and why the walk at `0x000cd86c` visits a record whose delegate half is still zero

A count mismatch between "records declared" and "records populated" would explain the whole thing,
and it is measurable: watch the array's length field, and compare it against the number of times
the fill loop runs.

### The count-mismatch hypothesis does not survive its first measurement

| site | hits |
|---|---|
| `0x00211a98` — the constructor's `str r5, [r4, #0x1c]` | **791** |
| `0x00244a44` — the stride-`0x74` array write | 445 |
| `0x0024415c` — the other stride site | 1 |
| `0x000cd878` — the walk's call to the delegate | **1** |

So 791 records get their `+0x1c` assigned, and the walk visits exactly **one**. This is not a fill
pass that ran short: the record that kills the boot **was** visited by the constructor, like the
other 790.

Which leaves a genuine contradiction to resolve rather than a story to tell. Nothing anywhere in the
image stores to `+0x20`, yet the walk reads a delegate object from there — so either

- `+0x20` is written by something the static scan missed (a wider-range store, an `stmia`, a
  `memcpy` into the record), which the ±0x60 window around 20 call sites would not catch; or
- `0x13e26f8c` is **not** `record+0x20` at all, and the record boundary is elsewhere — the stride is
  `0x74`, and I inferred the base from a single constructor's `this`

The second is the more likely error and it is cheap to settle: watch the whole `0x74`-byte record as
a range rather than three individual words, and the writer will name itself. That is where this
picks up.

## 54. The model ID is authentic — it comes out of the iPod's own flash config

The strategic reframe in §53's aftermath was: *shipping firmware does not contain an unconditional
call through an unbound delegate, so something upstream is telling RetailOS a lie about this
machine.* The most obvious candidate was the identity we hand it — [research/12](12-bypass-ledger.md)
#5, the Gestalt ID, still 🟡 and never validated. It turns out not to be a lie, and finding that out
decoded the whole identity path.

### How RetailOS asks what machine it is on

```
00265164  ldr   r4, [pc, #0x20]
00265168  ldr   r0, [r4, #0x28]      ; cached
0026516c  cmn   r0, #0x80000001      ; is it still the 0x7fffffff sentinel?
00265178  bl    0x002827b8           ; if so, fetch the sysinfo pointer
00265180  ldrne r0, [r0, #0x84]      ; read sysinfo + 0x84
00265184  strne r0, [r4, #0x28]      ; and cache it
```

**`sysinfo + 0x84`** — exactly the field #5 fakes. Apple's bootloader carries the same accessor at
`0x40001df0`/`0x40001e04`, so this is the one canonical way the machine identifies itself.

### Where the value comes from

Watching `sysinfo + 0x84` (`0x4001590c`, the block being at `0x40015888`) catches two writes:

```
40006d54  str   r0, [r4, #0x84]   0x00000000 -> 0x000f0000    ; a default
40006df8  streq r0, [r4, #0x84]   0x000f0000 -> 0x000b0011    ; the real one
```

The second is a **key/value lookup**: `0x400098dc` is called with a key pointer and, on success,
the record's second word is stored. The keys sit in a run at `0x40007150` as 4-character tags in
reversed byte order — `IsyS`, `mNwH`, `dIwH`, `rVwH`, `mNrS`, `dIwF` — that is **SysI, HwNm, HwId,
HwVr, SrNm, FwId**. The one feeding `sysinfo + 0x84` is **`HwVr`**.

### And `HwVr` is in the NOR dump

The flash carries a `SCfg` block at `0x4000` (magic `gfCS`, length `0xa4`, 7 records):

| offset | tag | value |
|---|---|---|
| `0x4018` | `SrNm` | `"U1234567890"` |
| `0x402c` | `FwId` | `00 00 00 01 20 70 70 02 …` |
| `0x4040` | `HwId` | `ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff` |
| `0x4054` | **`HwVr`** | **`0x000b0011`** |
| `0x4068` | `Regn` | `01 00 02 00 01 00 02 00` |
| `0x407c` | `Mod#` | … |

**`0x000b0011` is read straight out of the iPod's own flash.** It is not our invention and not a
bypass — the machine is telling RetailOS the truth about itself, because the truth is in the dump.
**The wrong-model hypothesis is dead.**

Two things follow.

**Bypass #5 is wrong, and now measurable.** It sets `sysinfo + 0x84 = 0x000B0005` on the *warm* path.
The cold path — the authority — produces `0x000B0011`. The warm-boot constant should be corrected,
and #5 stops being a guess: it has a known-correct value.

**The dump's `SCfg` is partially unprogrammed.** `HwId` is all-ones (erased flash) and the serial is
the placeholder `U1234567890`, not an Apple serial. Either this board's config was reinitialised, or
the serial was scrubbed before the dump was shared. ~~`sysinfo + 0x48`, where the bootloader stores
`HwId`, is **never written** across a whole run — the lookup finds nothing to store.~~

> **Corrected 2026-08-13 — it is written, three times, every time with zero.** Another `--watch`
> change-versus-write false negative, the fourth in this project. Measured with `--storeaddr`, which
> hooks the top of `write32` and cannot miss a store. The cold-boot handoff block is not at the warm
> path's synthetic `0x4001fd00` — the real bootloader builds its own and publishes the pointer at
> `0x4001ff1c` (`0x40015888` on the prototype NOR, `0x40015898` on the retail one), so anything
> watching `0x4001fd48` on a cold boot was watching an address the firmware has no reason to touch.
> At the *real* `+0x48`, on the prototype NOR this section is about:
>
> ```
> 0x400007f8 -> [0x400158d0] = 0x00000000  @87952     ; the block being zeroed
> 0x40006d3c -> [0x400158d0] = 0x00000000  @90859     ; the HwId lookup, storing its result
> 0x40006ea4 -> [0x400158d0] = 0x00000000  @91411
> ```
>
> and `+0x84` two instructions later takes `0x000f0000` then `0x000b0011`, so the neighbouring
> `HwVr` lookup on the same code path demonstrably *succeeds*. The retail NOR behaves identically at
> `+0x48` and differs only in the value it finds for `HwVr` (`0x000b0005`) and in carrying a real
> serial, `<SERIAL-ROM>`.
>
> **The conclusion is unchanged and now has a mechanism:** the lookup runs, finds an erased key, and
> stores zero. "Never written" was the wrong description of it, and the wrong description is what
> would have sent a successor looking for a code path that does not exist.

Whether RetailOS cares about a blank `HwId` is **not yet measured**, and is worth measuring before
it is believed: an unprogrammed identity record is exactly the kind of upstream difference that
could produce a device record nothing knows how to service. But it is a hypothesis, and this file
has spent a night learning what those are worth untested.

## 55. The model selector handles our machine cleanly — bypass #5 is validated, not guilty

`install_sysinfo()` in the tracer carries a comment from an earlier session that reads like a
confession of the exact fault we have been chasing:

> *Without it, the model selector at `0x2653a4` falls off its jump table, the object accessor returns
> null, and RetailOS makes a virtual call through a null vtable straight to address 0 — a reset.*

Same mechanism, same address-0 branch. So the obvious question is whether the **cold** path's
`0x000b0011` also falls off, where the warm path's `0x000b0005` did not. It does not:

```
002653a8  bl    0x00265164        ; fetch HwVr
002653ac  mov   r4, r0, lsl #16
002653b0  mov   r4, r4, lsr #16   ; r4 = LOW half  = 0x0011
002653b8  mov   r0, r0, lsr #16   ; r0 = HIGH half = 0x000b = 11
002653bc  cmp   r0, #0xc          ; 11 <= 12
002653c0  addls pc, pc, r0, lsl #2
   …
002653f4  b     0x00265414        ; <- entry 11
00265414  mov   r0, #3            ; returns 3
```

Entry 11 returns **3** unconditionally — it does not even consult the low half. (`r4` is only used
by the `cmp r4, #2` branch at `0x002653fc`, which other indices reach.) So on the cold path the
model is recognised, the selector answers cleanly, and **the Gestalt is not the trigger**.

### And forcing it the other way proves the direction

Overriding `sysinfo + 0x84` to the warm path's `0x000b0005` does not fix anything — it **breaks the
bootloader**, which never prints `Retail mode`, never loads `osos`, and executes 448 profile buckets
against 6 033. Notably it drives the BCM *twice* as hard (8 commands, 4 frame updates, 265 708
halfwords against 4/2/132 548), which is what an error or recovery screen would look like. Apple's
own ROM consults `HwVr` too, and `0x000b0005` is not this machine.

That is the second "it stopped failing because it never got there" of the night, and it was caught
the same way: by checking whether the bootloader still completed.

### Where that leaves #5

[research/12](12-bypass-ledger.md) #5 has been 🟡 — *"confirming the offset against a real
bootloader's handoff"* — since it was written. It is now **confirmed**: the offset is right, the
field is `HwVr`, the mechanism is a key/value lookup in the flash `SCfg` block, and the authoritative
value is `0x000b0011`. The warm-boot constant `0x000B0005` is **wrong** and should become
`0x000b0011`; both share the high half, so the selector returns 3 either way, which is why the warm
path never noticed.

Not changed here, deliberately: the warm path was tuned around the old constant and re-validating it
is its own measurement. Recorded as a known-correct value waiting for one.

## 56. Both small bypasses retired — #5 by measurement, #14 by reading what the flag actually did

§55 left #5 as "a known-correct value waiting for a measurement", and
[research/12](12-bypass-ledger.md) left #14 as "wrong on principle, measured not to be a live bug".
Both are now closed. Neither turned out the way the ledger expected.

### The warm path had no recipe, which is why it had never been re-validated

`cold-boot.sh` existed; the warm path lived as a command line pasted into
[research/09](09-retailos-boot.md). A bypass whose reproduction is prose cannot be re-measured, so
the first change was `tools/ipod-boot/warm-boot.sh` — the same defaults, overrides and instrument
flags as the cold recipe, entering RetailOS at `0x10000000` with `--sysinfo`.

### #5: `0x000b0011` costs nothing and buys one sector

Measured on `warm-boot.sh`, 600 M instructions at `--clock=5`, before and after:

| | `0x000B0005` (published) | `0x000b0011` (this machine's `HwVr`) |
|---|---|---|
| instructions | 600 000 000 | 600 000 000 |
| arrivals at the selector `0x2653a4` | 104, from `lr=0x111c60` ×103 + `lr=0x265e9c` ×1 | **identical** |
| arrivals at address `0` | 51 | **51** |
| ATA commands | 18 | 18 |
| unmapped | 220 reads, 0 writes | 220 reads, 0 writes |
| code buckets executed | 17 968 | **17 972** |
| irqs asserted / taken | 417 538 / 98 725 | 417 647 / 98 729 |
| **first MBR read** | `cmd 0xc8 nsector 0x01` — 512 bytes | **`nsector 0x04` — 2048 bytes** |

One line moves. RetailOS's drive-configuration step asks for four sectors where it used to ask for
one — a model-dependent read size, which is exactly the kind of thing a Gestalt ID is consulted for.
Nothing regresses. §55's prediction that the selector "returns 3 either way" holds, and the reason
the warm path never noticed the wrong constant is confirmed rather than assumed.

The instruments verified themselves before being believed: `--readlog` carried the control address
`0x4001ff1c` (`SYSINFO_PTR`), read 104 times by `0x13e0` and `0x1404` — the documented readers — and
`--dump=0x4001fd84:16` showed `11 00 0b 00` where it had shown `05 00 0b 00`.

### #14: the flag was never structurally required, and it was hiding something

The ledger's stated retirement condition was *"retire when `--boot-osos` stops requiring `--osos=`"*.
Reading the code, `--boot-osos` never required `--osos=`. It requires **an image at the entry**, and
the entry is `0x10000000` only on the warm path. A cold boot enters NOR at `0`; the ROM finds `osos`
in the firmware directory and DMAs it in. The two are not the same requirement, and conflating them
is what kept the flag in the recipe.

Removing it leaves the run identical to the instruction — 599 999 952 executed, the same console
receipt through `Running 'osos' 0 from 0x10000000`, 88 ATA commands, 61 DMA transfers moving
7 598 080 bytes, the same IRQ, BCM and PMU totals.

**What the ROM loads is byte-identical to `OSOS_correct.bin`.** Compared at the handover
(`--stop-at=0x10000000:1 --save-region=sdram:FILE`, halting at 46 449 749 instructions), all
7 559 680 bytes match — with the pre-placed copy and, more to the point, **without** it, where SDRAM
started as zeros and the only thing that could have filled it was the ROM's own disk read. The
positive control is that the dumped window is not a zero file. The 1 536 bytes the DMA writes past
the image end are the `rsrc` volume's FAT boot sector (`MTOOL399`), which is sector rounding, not a
defect.

The one thing the pre-placed copy was still buying was `extract_symbols` — RetailOS's 140
self-carried function labels, used by `--novelty`, `--profile` and `--callgraph`, all of which name
addresses *after* the run. So they are now recovered from the SDRAM the ROM filled, bounded by the
ATA DMA high-water mark **over transfers that land inside SDRAM**. That qualifier is load-bearing: a
plain maximum takes RetailOS's own later reads to `0x17edbea0` and `0x93eea730`, clamps to the region
end, and puts 64 MB of heap in scope — which recovers 141 names instead of 140, the extra one
manufactured out of an allocation. Bounded properly it produces **the same 140 names at the same
addresses** the file did.

### And it was absorbing writes that should have been reported

`--osos=` also installed an `osos-low` mirror at address `0`. Unlike NOR, that mirror was
**writable** — so a write to low memory found it after skipping read-only `flash-low`. With the flag
gone:

```
unmapped: 160 reads,  0 writes across 1 pages      (with --osos=)
unmapped: 160 reads, 40 writes across 4 pages      (without)
  0x00000000  16 writes  first pc 0x40009fd0
  0x0000aaaa  16 writes  first pc 0x40009f9c
  0x00005554   8 writes  first pc 0x40009fa8
```

`0xaaaa` / `0x5554` / `0x0` from four instructions inside one routine is the **JEDEC/CFI unlock
sequence** — the bootloader probing the NOR for a programmable flash device. That is bypass #12's
territory, and it had been landing silently in a mirror of the OS image for as long as the flag was
there. A bypass that hides a measurement is worse than one that only fakes a value.

### Incidental, and it contradicts §40: the cold path reaches address 0 thirty-six times

§40 records *"**one** arrival at `0x00000000` — the cold reset"*. With `--enterlog=0x00000000` on the
current cold recipe at 600 M / `--clock=5`:

```
0x00000000 from lr=0x117ffffc  x1      <- the reset entry
0x00000000 from lr=0x000fb8ec  x1
0x00000000 from lr=0x00183e90  x35
```

Thirty-six arrivals from *inside RetailOS*, against the warm path's 51 from the same two call sites.
So the null-vtable branch is not warm-only and not gone; §40's count was taken with a different
instrument or a different budget, and the discrepancy is recorded here rather than quietly folded
into either bypass. It is **not** the Gestalt: the count is bit-identical either side of #5's
correction. Whoever picks up the missing-font thread (§ledger item 1) should re-measure §40 first.

> **Re-measured 2026-08-13 and settled.** With the disk fixes in, it is **157** arrivals —
> `lr=0x117ffffc` ×1 (the cold reset), `lr=0x000fb8ec` ×1, `lr=0x00183e90` ×155 — and both `lr`s are
> `mov lr,pc` values, so these are `BX` to zero, not vector entries. §35's "null vtable, `bx 0`,
> jump to the reset vector" was the correct reading; §40 retracted it for the wrong reason (the
> one-sector shift was real, but it was not what produced *this* symptom, which survives the repair).
> The whole mechanism, the trigger, and the ablation that removes all 156 self-resets are in
> [research/20 Addendum 5](20-the-resource-image.md#addendum-5-neither-it-is-bx-to-zero-and-the-headline-of-this-file-was-right-all-along).
