# B7 — booting RetailOS: what the published hardware documentation bought us

> **Status: RetailOS boots and reaches its idle loop.** It completes its scatter-load, programs the
> cache/remap unit, brings up its device model, probes the ATA controller, runs **without a single
> fault**, **without a single reset**, and ends up writing `CPU_CTRL = SLEEP` — asking the core to
> halt until an interrupt arrives. Before this session it managed a few hundred instructions.
>
> **Interrupts work** (§5f): the firmware programs its own ~1 kHz tick, and a 600M-instruction run
> takes 7,705 timer interrupts across **8 seconds of simulated uptime** with a live scheduler, no
> faults and no resets.
>
> **The I²C wait is understood** (§5h): RetailOS starts a battery-voltage conversion on the PCF50605
> PMU and polls `ADCS1` for conversion-ready — a register that always read zero. With
> `--i2c-fill=0xff` it moves on, and I²C traffic drops from 30 149 646 accesses to 1 694.
>
> **The ATA controller was never mapped** (§5i) — `IDE_BASE 0xc3000000` sat outside our `mmio-c`
> region, so every disk access read back zero. Mapped now; the firmware probes a controller that
> reports no drive. **A real ATA model is the next piece**, and behind it the disk image that
> already holds the 54 games.
>
> One command, no pokes:
>
> ```sh
> trace <game.bin> --osos=fw/OSOS_correct.bin --boot-osos --osos-at=0x04000000 --sysinfo
> ```

## The short version

Every blocker in this document was solved by reading **Rockbox**, not by experiment. Rockbox is a
GPL firmware for the same PortalPlayer silicon, and it contains a documented register map and boot
sequence for the exact SoC in the 5G iPod. The specific files:

| File | What it gave us |
|---|---|
| `firmware/export/pp5020.h` | the register map — `PROC_ID`, `COP_CTRL`/`COP_STATUS`, the cache aperture, the MMAP window table, `IDE_BASE` |
| `firmware/target/arm/pp/crt0-pp.S` | the boot sequence — core arbitration, the remap dance, the order things must happen in |

Fetched from `https://raw.githubusercontent.com/Rockbox/rockbox/master/…`. Both are public.

**iPodLinux** supplied the second half — the flash bootloader's IRAM handoff block (§5b):

| File | What it gave us |
|---|---|
| `ipodloader2/ipodhw.c` | `SYSINFO_TAG_PP5022 0x4001ff18`, `SYSINFO_PTR_PP5022 0x4001ff1c` |
| kernel `include/asm-armnommu/arch-ipod/hardware.h` | the `struct sysinfo_t` field order |

Two further sources are worth knowing about even though we have not needed them yet:
**clicky** (`github.com/daniel5151/clicky`), an ARM7TDMI iPod **4G** emulator whose HLE bootloader is
the closest thing to a written spec of the handoff state, and which mirrors MrH's *"PortalPlayer
PP5024 memory controller & cache(s)"* doc — the original `daniel.haxx.se` URL is dead. Its README
describes booting RetailOS; per the operator, who follows its development, **that is not its current
state** — it boots Rockbox, iPodLinux does not yet boot, and the author's order is Rockbox →
iPodLinux → diagnostics → RetailOS. Read its *code and docs* as reference, not its status claims. And **freemyipod**,
which turns out to be a **negative result** for this work: their bootrom and boot-process pages are
S5L87xx (Nano 3G+ / Classic 6G+) only, with no PortalPlayer memory map at all.

## 1. The PP5021 is dual-core, and we were always booting the wrong core

This one invalidated every prior boot run.

`crt0-pp.S` opens by asking the silicon which core it is:

```asm
ldr    r0, =PROC_ID     /* 0x60000000 */
ldrb   r0, [r0]
cmp    r0, #0x55        /* 0x55 = CPU, anything else = COP */
ldrne  r2, =COP_CTRL    /* not the CPU -> put self to sleep */
movne  r1, #SLEEP
1:  ldreq  r1, [r2]     /* the CPU meanwhile spins on COP_STATUS */
    tsteq  r1, #COPSLEEPING
    beq    1b
```

Our MMIO region at `0x60000000` was zero-filled. Zero is not `0x55`, so **every run we had ever
done was a coprocessor boot**: the firmware concluded it was the COP and went to sleep within three
instructions. And the other branch is no better — the CPU spins forever waiting for a COP that never
reports sleeping, so a zeroed register kills both paths.

The fix is two words, now baked into `map_hardware()`:

```rust
m.mem.write32(0x6000_0000, 0x0000_0055); // PROC_ID: we are the CPU
m.mem.write32(0x6000_7004, 0x8000_0000); // COP_STATUS: COP already asleep
```

Instructions executed went from a few hundred to **2,243,616**, and the firmware started printing
its own diagnostics.

There is no amount of black-box experiment that finds `0x55`. It is a magic number in a register we
did not know existed, on a core we did not know was there.

## 2. `0xf000f000` is the remap unit, not scratch memory

With the core fixed, the run wrote 64 bytes to `0xf000f000..0xf000f03f` and we had nothing mapped
there. `pp5020.h` names it exactly:

```c
#define MMAP0_LOGICAL   (*(volatile unsigned long*)(0xf000f000))
#define MMAP0_PHYSICAL  (*(volatile unsigned long*)(0xf000f004))
...                                     /* 8 windows, 8 bytes each = the 64 bytes */
#define MMAP_PHYS_READ_MASK   0x0100
#define MMAP_PHYS_WRITE_MASK  0x0200
#define MMAP_PHYS_DATA_MASK   0x0400
#define MMAP_PHYS_CODE_MASK   0x0800
```

Register bits [31:16] are the address; bits [13:9] are a size mask (`0x3c00` = 64 MB, `0x3e00` =
32 MB, per `crt0-pp.S`'s `MEMORYSIZE` switch); the physical register carries the four access-enable
flags. The surrounding aperture `0xf0000000..0xf000c000` is the cache controller's data, status,
flush and invalidate arrays.

Mapping `0xf0000000` (64 KB) let the firmware's own programming land, and it can be read back with
`--dump=0xf000f000:64` rather than guessed at.

## 3. `0x149xxxxx` is a computed uncached alias

The run then wrote 883,307 words across 224 contiguous pages at `0x149xxxxx` and jumped into the
region it had just filled. This looked like an unknown RAM bank. It is not — the firmware *computes*
the address, at `0xdb4`:

```asm
0db4  ldr r2, [r4, #0x24]
0db8  bic r0, r2, #0xfc000000   ; strip the top 6 bits
0dbc  orr r0, r0, #0x14000000   ; ...and OR in 0x14000000
```

So the rule is `alias = 0x14000000 | (addr & 0x03FFFFFF)` — the uncached view of a 64 MB space.

This was first modelled as blank RAM via `--map=0x14000000:0x1000000`, which works only for as long
as the firmware writes and reads through a single view and silently diverges the moment it crosses
between them. **Now fixed properly**: `Memory` carries an `aliases: Vec<(base, size, target)>` and
`translate()` resolves them, so the uncached window is genuinely the same storage as the low view.

`is_mapped()` has to translate too. It did not at first, and the run stopped at `Lost(0x14936F50)` —
the firmware *jumps* into the uncached view, so an alias honoured for loads and stores but not for
instruction fetch reports the code as unmapped and ends the run at the first such branch.

## 4. The scatter-load reads the image at `0x04000000`

After the relocation target was mapped, the remaining unmapped accesses were all *reads*, from
`pc 0x84390` — the load side of a word-copy loop:

```asm
0008438c  cmp   r1, r2
00084390  ldr   r3, [r0], #0x4      ; source
00084394  str   r3, [r1], #0x4      ; destination
00084398  bcc   0x0008438c
```

Sources ran `0x04716000..0x04733c9f`. The OSOS image is `0x735E00` bytes, and
`0x04733ca0 − 0x04000000 = 0x733ca0` — just inside it. This is a standard ARM RVCT scatter-load
reading initialised-data sections straight out of the firmware image, with pointers relative to an
image base of `0x04000000`. Mirroring the image there (`--osos-at=0x04000000`) dropped unmapped
accesses from 224 pages to 2.

The neighbouring literal pool at `0x84378` is the region table, and the thunks just past the copy
loops (`ldr pc, [pc, #-4]` with literals `0x4000013c`, `0x40008bd0`, `0x400003c8`) are calls into
IRAM-resident code the same mechanism has copied into place.

## 5. Where it stops now: an assertion in RetailOS's own code

The firmware's fault path is fully mapped out:

- vectors at `0x0` are real — reset `0x1f0`, undef `0x1a8`, SWI `ldr pc,[pc,#0xdc]`, prefetch abort
  `0x1c0`, data abort `0x1d8`, IRQ `0x180`, FIQ `0x15c`
- the three fault vectors all funnel into `0x90494` with a reason code in `r1` — undef 1, prefetch
  abort 3, data abort 4
- `0x13bc` is the reporter: `mov r0, #0x18` is the ARM semihosting `ReportException` code, which is
  why the dump arrives as semihosted output
- the image identifies itself in cleartext at `0x20`: `"portalplayer"`, and `"PP5020AF"` at `0x40`

Breaking on `0x90494` shows `r1 = 8` and `lr = 0xdb4` — **not an abort at all**, but a direct
assertion failure at `0xda0`:

```asm
0d98  ldr r0, [r4, #0x24]     ; r4 = 0x40006000
0d9c  ldr r1, [r4, #0x20]
0da0  cmp r0, r1
0da8  movhi r1, #0xe7         ; assertion identifier 231
0db0  blhi 0x144cc8           ; assert(r0 <= r1) — fires
```

`r4 = 0x40006000` is the IRAM structure we spent earlier sessions poking blind. Offsets `+0x20` and
`+0x24` are a limit and a pointer, and `+0x24` is what gets converted to the uncached alias at
`0xdb4` — so this is a memory-region descriptor whose contents the **flash bootloader** would
normally have filled in. We do not have the flash bootloader; that is the actual remaining gap, and
it is now a specific one: two words at a known address with a known ordering invariant.

The `Task: 74726F70` in the panic dump was a red herring — `0x74726F70` is `"port"`, the first four
bytes of the `"portalplayer"` string at `0x20`. It means the task pointer was near-null, not that
there is a task called `port`.

## 5a. The assertion, traced to its cause: RAM size resolves to zero

`[0x40006020]` is not bootloader state — the firmware writes it itself, at `0xe60`, and it is a
**sum**:

```asm
0e3c  mov r1, #0
0e40  and r3, r1, #0xff
0e44  ldr r3, [r0, r3, lsl #2]   ; r3 = struct[i]
0e50  add r2, r3, r2             ; r2 += struct[i]
0e4c  cmp r1, #4                 ; ...over i in 0..4
0e60  stmia r3, {r2, r4}         ; [+0x20] = total, [+0x24] = pointer
```

The four words at `0x40006000..0x4000600f` are **SDRAM bank sizes**, and each is a table lookup
(`0xdf8`) indexed by a nibble of a per-bank config word. The table lives at `0x16d8`:

| index | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |
|---|---|---|---|---|---|---|---|---|---|---|
| size | 0 | 512 KB | 1 MB | 2 MB | 4 MB | 8 MB | 16 MB | **32 MB** | **64 MB** | 128 MB |

All four banks index 0, so the total is 0, so every pointer is above the limit and the assertion
fires. The size is written into the config word by `0xe6c` (`set_bank_size(cfg, bank, size)`, a
reverse lookup into the same table), called from `0x13f4` with the size read from `[r0+0xe0]` — and
at that point `r0 = 0`. The object is null. `r4 = 0x4001ff18` is a registry near the top of IRAM
whose slot 4 was never filled.

So the chain terminates in genuinely absent pre-boot state, and **`--poke` cannot substitute for
it** — both `0x40006000` and `0x40006020` are overwritten during the run by the firmware's own
(correct, but zero-valued) computation. Two pokes were tried and had literally no effect;
instruction counts were identical to the digit.

### The registry itself is understood; it is simply empty

`0x1084` is `slot(id) = base + id - 0x100`, where `base` is chosen by chip revision — `0x5b8` and
`0x598` both read `0x70000000`, take bits [23:16], and compare against `0x36` and `0x32`. Either
match selects `0x40020000`; anything else selects `0x40018000`. Those are the **tops of IRAM** for a
128 KB and a 96 KB part. We seed `0x36` in `map_hardware()`, so we get `0x40020000`, correct for the
PP5021C's 128 KB — the per-CPU block is the last 256 bytes of IRAM, `0x4001ff00..0x40020000`, and
slot `0x18` is `0x4001ff18`.

That path is right. Seeding `0x4001ff1c` with a distinctive value and watching it reports **no writes
at all** — so the memory device is never registered, rather than registered wrongly. This is real
missing pre-boot state, and it is a *structure*, not a field: the flash bootloader's handoff block.

Note also, from `crt0-pp.S`: **there is no register that reports SDRAM size on this hardware.**
Rockbox detects 32-vs-64 MB by writing `64` to `0x12000000-1` and `32` to `0x14000000-1` and seeing
whether the two writes alias. Whatever fills `[r0+0xe0]` on a real iPod, it is not a size register.

### A measurement trap worth remembering

`--watch` reports **changes, not writes**. Watching `0x40006020` alone said *"no writes observed"*,
which reads as "nothing touches this" — but the firmware stores `0` over an already-`0` word, which
is not a change. Poking a distinctive value first and watching *that* is what exposed the real
store:

```
--- watch: 1 changes ---
  0x00000e60  0x04000000 -> 0x00000000   stmia r3, {r2, r4}
```

Two runs, opposite conclusions, same instrument. When a watch reports nothing, seed the address with
a value that would be a change.

## 5b. Solved — the missing structure is `sysinfo_t`, and it is publicly documented

The empty registry slot has a name. From iPodLinux — `ipodloader2/ipodhw.c` and the kernel's
`include/asm-armnommu/arch-ipod/hardware.h`:

```c
#define SYSINFO_TAG_PP5022  (unsigned char *)0x4001ff18
#define SYSINFO_PTR_PP5022  (struct sysinfo_t **)0x4001ff1c
```

Those are **exactly** the two addresses we had found empty. The last 256 bytes of IRAM are the Apple
flash bootloader's handoff area; ipodloader2 comments it as *"last 256 bytes appear to be used for
special things by the Flash ROM"*. The 5G is a PP5022-class part (128 KB IRAM), so the pointer lives
at `0x4001ff1c`, not the `0x40017f1c` used by the 96 KB parts.

It also explains a literal pool we had already dumped and not understood — `0x1430` holds
`0x40006000` followed by the four bytes `I s y S`.

`struct sysinfo_t` field order is documented identically by the iPodLinux kernel, ipodloader2, and
clicky. Computed offsets:

| offset | field | offset | field |
|---|---|---|---|
| `+0x00` | `IsyS` tag | `+0x60` | `Flsh` block |
| `+0x04` | `len` (`0x184`) | `+0x74` | `Sdrm` block |
| `+0x08` | `BoardHwName[16]` | `+0x7c` | `sdram_base` |
| `+0x18` | `pszSerialNumber[32]` | `+0x80` | `sdram_size` |
| `+0x38` | `pu8FirewireGuid[16]` | `+0x88` | `Frwr` block |
| `+0x48` | `boardHwRev` | `+0x9c` | `Iram` block |
| `+0x5c` | `iram_perhaps` | `+0xb0` | `pad7[120]` |

`install_sysinfo()` in `trace.rs` builds this, and `--sysinfo[=SIZE]` installs it.

**The assertion is gone.** With the block installed, `0x90494` — the fault dispatcher every abort and
assertion funnels through — **never fires at all**, and the run reaches a full 200,000,000-instruction
budget with the firmware printing no diagnostics whatsoever.

### One field is inference, and it is labelled

RetailOS reads its RAM size from `[sysinfo + 0xe0]` (measured: breakpoint at `0x13e8` shows the
load, with `r0` = the sysinfo pointer). Offset `0xe0` falls inside what iPodLinux calls `pad7[120]`
— bytes they never identified. So `install_sysinfo` writes the size there **by inference from the
consumer**, not from documentation.

The inference is testable, and it behaves like a size:

| declared size | result |
|---|---|
| 8 MB | assertion fires, `_sys_exit` panic |
| 32 MB | clean, no fault |
| 64 MB | clean, no fault |

A wrong-but-arbitrary value would not produce that pattern.

## 5c. Where it is now: booting, but doing too much work

With `--profile` (a new sampling PC histogram — see the tooling table), 600M instructions distribute as:

```
profile: 9375000 samples over 2999 buckets
  0x000843a0   66.2%      <- the zero-fill loop
  0x00084390   20.6%      <- the word-copy loop
  0x00084380    6.9%
  0x0027ecb0    0.2%      <- a table initialiser, 0x20-byte entries
  ...
```

**2,999 distinct buckets** — roughly 48 KB of distinct code — so this is real OS activity, not a
two-instruction spin. But 94% is memcpy/memset, driven from a table initialiser at `0x27ec90` whose
element count comes from `[r3]`:

> **Retracted.** The "oversized table init" reading below was wrong. Profiling at 32 MB and 64 MB
> gives **the same** memset sample count — 3,107,177 vs 3,107,163 — so the work does not scale with
> declared RAM at all, and nothing is sized wrong. See §5d: it is a **boot loop**, and each pass
> re-runs the same scatter-load. The correct lesson is that "94% in memset" was a symptom of
> repetition, not of magnitude, and a single profile could not tell the two apart. The comparison
> across two inputs could.

```asm
0027ec94  ldr r12, [r3, #0x0]     ; count
0027ec98  cmp r2, r12
0027ec9c  bxgt lr
0027eca0  str r1, [r0, #0x0]      ; ...zero a 0x20-byte entry
0027ecc4  add r2, r2, #0x1
0027ecc8  b 0x0027ec94
```

At 1.5 billion instructions it has still not finished. On an 80 MHz PP5021 that would be ~18 seconds
of real time, and a RetailOS boot does not take 18 seconds — so something is sized wrong, probably
downstream of a field we are supplying. **This is a sizing question inside a booting OS, not a
crash**, which is a materially different kind of problem from everything above it in this document.

## 5d. It is a boot loop, and the cause is a null virtual call

Breaking on the reset vector settles what the profile could not:

```
--break=0x1f0   -> 129 hits     (reset vector)
--break=0x0     -> 129 hits     (address 0)
```

**129 restarts in 300M instructions.** RetailOS boots, gets a fixed distance, resets, and starts
over — which is why the memcpy/memset totals are identical regardless of declared RAM. Each pass
re-runs the same scatter-load.

Address 0 is reached with `lr = 0x002668ec`, and the site is an ordinary C++ virtual dispatch:

```asm
002668d8  bl 0x00111c58       ; r0 = object
002668dc  ldr r1, [r0, #0x0]  ; r1 = vtable
002668e0  add lr, pc, #0x4
002668e4  ldr r1, [r1, #0x0]  ; r1 = vtable[0]
002668e8  bx r1               ; -> 0 -> reset
```

`r0` is **0** at `0x2668dc`, so `[r0]` reads the vector table at address 0 and gets `0xea000078` —
a branch instruction, used as a vtable pointer. (That also explains the stray `0xea000078` unmapped
read we had been ignoring.)

`0x111c58` is a three-way selector that returns null by default:

```asm
00111c5c  bl 0x002653a4       ; model index
00111c60  cmp r0, #1 -> object A
00111c6c  cmp r0, #2 -> object B
00111c78  cmp r0, #3 -> object C
00111c80  movne r0, #0        ; ...otherwise null
```

and `0x2653a4` is a jump table on `hwid >> 16`, bounds-checked against `0xc`. The **Gestalt ID** we
already know from the USB work fits exactly — `0x000B0005` (iPod with Video) has high halfword
`0x000B` = 11, inside the table. So the chain wants a hardware ID and is not getting one.

### The hardware ID: fetch path correct, source empty

`0x265164` caches the ID at `0x1081de68`, using `0x7FFFFFFF` as its "not yet fetched" sentinel:

```asm
0026516c  ldr r0, [r4, #0x28]
00265170  cmn r0, #0x80000001   ; == 0x7FFFFFFF ?
00265174  bne 0x00265188        ; already cached -> return it
00265178  bl 0x002827b8         ; else fetch
00265180  ldrne r0, [r0, #0x84] ; ...from obj->field_0x84
00265184  strne r0, [r4, #0x28] ; ...and cache
```

Seeding the sentinel and watching shows the machinery is **working correctly**:

```
0x00084394  0x00000000 -> 0x7fffffff   strcc r3, [r1], #0x4   <- scatter-load writes the sentinel
0x00265184  0x7fffffff -> 0x00000000   strne r0, [r4, #0x28]  <- fetch runs, caches 0
```

So the scatter-load initialises the static properly and the fetch executes. The problem is one level
further out: the object at `0x108710dc` is **144 bytes of zeros**, an uninitialised BSS singleton
whose `+0x84` should hold the hardware ID. Nothing ever populates it.

**Pokes cannot substitute here either** — `0x843a8`, the scatter-load's zero-fill, wipes any seeded
value on every boot pass (verified by watch, not assumed). Whatever fills this object has to run
*inside* the boot, so the next question is what should call into it.

## 5e. SOLVED — RetailOS boots, and reaches its idle loop

The unpopulated singleton was traced one more level. `0x281224` does not *construct* the device
object — it **memcpy's** it:

```asm
0028122c  ldr r1, [r0, #0x8c]   ; source
00281230  cmp r1, #0
00281234  popeq {r4,pc}         ; null -> don't register at all
00281240  cmp r4, #0xf8         ; clamp length to 248
0028124c  bl 0x0007cae0         ; memcpy(0x108710dc, src, len)
00281260  b 0x00282800          ; register the copy
```

Breaking at `0x281238` shows `r1 = 0x4001fd00` — **our own `sysinfo_t` block.** The device object is
a 248-byte copy of it, so RetailOS reads its model from `sysinfo + 0x84`.

Setting that to the **Gestalt ID `0x000B0005`** — "iPod with Video", the same constant the USB work
feeds iTunes in the INQUIRY vendor bytes — ends the boot loop:

| | before | after |
|---|---|---|
| resets (`--break=0x0`) | **129** | **0** |
| scatter-load share of runtime | 94% | 0.7% (runs once) |
| unmapped pages | 2 | 1, touched 8 times |

`0x000B0010` (Late 2006) works equally well; the jump table at `0x2653a4` accepts both, since it
switches on the high halfword `0x000B` = 11.

**The two halves of this project met here.** The Gestalt ID was reverse-engineered months of work
away, in research/07*(moved to the `ipod-usb` repository)*, to make iTunes accept a virtual iPod. It turns out
to be the same number RetailOS needs to boot.

### And then: the idle loop

With the reset gone, 99% of runtime moved to a tight three-bucket loop, which is a **timeout check**:

```asm
002840b8  ldr r2, [pc, #0x14]   ; 0x60005010
002840bc  ldr r2, [r2, #0x10]
002840c0  sub r0, r2, r0        ; elapsed = now - start
002840c4  cmp r0, r1            ; ...>= timeout?
```

`0x60005010` is `USEC_TIMER` (Rockbox `pp5020.h`) — a free-running microsecond counter. **A zeroed
MMIO region is a stopped clock**, so `elapsed` was always 0 and every timeout waited forever. It
reads as a hang and looks nothing like a missing register.

`Memory::usec_timer` now answers that address with a counter advanced by `Machine::run` at ~75
instructions per microsecond (the PP5021C runs at roughly 75 MHz and this interpreter is one
instruction per step). The ratio only needs to be plausible — firmware compares elapsed against its
own timeouts, so what matters is that time moves at a sane rate.

With the clock running, the profile spreads across many buckets and two of them identify the state:

| bucket | what it is |
|---|---|
| `0x11c4d0` | `PROC_ID == 0x55` — the "am I the CPU?" check |
| `0xa0590` | `mov r0, #0x80000000` / `str r0, [r1]`, literal `0x60007000` — **`CPU_CTRL = SLEEP`** |

`CPU_CTRL` and `SLEEP` are both from `crt0-pp.S`. **RetailOS has finished booting and is asking the
core to sleep until an interrupt arrives.** On real silicon that halts the CPU; here the write does
nothing and the idle loop spins.

**So the next thing RetailOS needs is interrupts** — the controller at `0x60004000`
(`CPU_INT_STAT`/`CPU_INT_EN`, documented in `pp5020.h`), a periodic timer raising IRQs, and IRQ
delivery through the vector at `0x18` that we have already identified. Nothing is faulting; the OS is
idle because nothing ever wakes it.

## 5f. Interrupts — RetailOS now has a running scheduler

Built from Rockbox's `pp5020.h` (register map) and `timer-pp.c` (programming model), which writes
`TIMERn_CFG = 0xc0000000 | (cycles - 1)`: bit 31 enable, bit 30 repeat, low bits a period in
microseconds, `TIMER_FREQ` being 1 MHz.

The firmware's own programming, read back from the emulated registers:

```
CPU_INT_EN_STAT (0x60004020) = 0xc0000017    TIMER1, TIMER2 and others enabled
TIMER1_CFG      (0x60005000) = 0xc00003fc    enable | repeat | period 1021 µs  -> ~1 kHz tick
```

`Machine::service_interrupts` models the controller. Two details worth keeping:

- **`CPU_INT_EN` / `CPU_INT_DIS` are write-to-set and write-to-clear**, with the real state in
  `CPU_INT_EN_STAT`. Rather than intercept those stores in the byte-level `Bus`, they land in the
  MMIO region as ordinary writes and are consumed here — which keeps device knowledge out of the
  memory layer.
- **A PP timer interrupt is acknowledged at the timer**, by reading its `VAL` register; there is no
  central acknowledge. Clearing on delivery instead would be simpler and wrong — the handler reads
  `CPU_INT_STAT` to decide *which* source fired, so a bit already cleared dispatches to nothing.
  `Memory::int_ack_on_read` maps `TIMER1_VAL`/`TIMER2_VAL` to their pending bits.

Result, over a 600M-instruction run — **8 seconds of simulated uptime**:

```
irqs: 15411 asserted, 7705 taken; usec 8000000
unmapped: 8 reads, 8 writes across 1 pages
```

Breakpoints confirm the path: `0x18` (the IRQ vector) and `0x180` (its handler) are each hit once
per taken interrupt. The ~50% gap between asserted and taken is interrupts arriving while the CPU is
already inside the handler with IRQs masked — expected, and the reason the counter reports both.

### Two false negatives, one of them mine

- **`--watch` cannot see writes made by `service_interrupts`.** The watch snapshot is taken inside
  the run loop *after* that call, so a value it writes is already the "old" value by the time the
  comparison happens. Watching `CPU_INT_STAT` reported "no writes observed" while interrupts were
  in fact firing thousands of times. An instrument's blind spot is a property of *where* it samples,
  not just of what it samples.
- **A breakpoint report that appeared to show zero hits was my own `grep ... | head -14`** cutting
  the output off. The break report prints last, after the profile, and the profile filled the
  window. Not a tool defect — but the same failure shape, and worth the same suspicion.

The counters (`irqs_asserted` / `irqs_taken`) exist because of the first one: they measure the thing
directly instead of inferring it from a side effect.

## 5g. Where it is now: the I²C bus

With the scheduler running, the profile settles on a poll loop:

```asm
000f9140  ldr  r4, [pc, #0x4c]   ; 0x7000c000
000f9148  ldrb r0, [r4, #0x1c]   ; status
000f9150  tst  r0, #0x40
```

`0x7000c000` is `I2C_BASE` (Rockbox `pp5020.h`). RetailOS is waiting on an **I²C transfer**, with a
byte-shifting routine at `0x282e00` feeding it. On the iPod 5G the I²C bus is how the SoC talks to
the **PCF50605 power-management chip** — which the OS cannot get far without.

Rockbox has a driver for exactly this (`firmware/target/arm/pp/i2c-pp.c`), so the next device follows
the same pattern as every blocker so far: the documentation already exists.

## 5h. The I²C wait is a stopped ADC — and a tool that measured itself

Three new instruments were needed to see this, and the first one lied.

**`device_report()`** counts accesses per mapped region — "which devices is the firmware actually
driving?", which a PC profile cannot answer, because a device is identified by the address it
answers on and not by the code that touches it.

Its first output was **wrong, and wrong in this project's established way**: `mmio-6` showed 150M
accesses, reading as a firmware hot loop. Roughly all of it was **`service_interrupts`, the
emulator's own device servicing** — ~8 word accesses every 64 instructions. A tool counting its own
traffic. With `Memory::internal` suppressing that, `mmio-6` fell from **150M to 77K**, and the real
picture appeared:

```
mmio-7   0x70000000    18 089 459 reads   12 060 187 writes     <- one access per 10 instructions
lcd      0x30000000             (never touched)
```

*Those numbers are the warm path as it stood when this section was written, and the `lcd` line is no
longer true of any current run: on `retail-boot.sh` the same region records **130 040 reads,
423 450 writes**, because the co-processor's `vmcs` firmware now arrives through it
([research/20](20-the-resource-image.md) Addendum 9). Kept as-is because the point here is what
`device_report()` does, not what these particular registers do.*

**`--pagelog=BASE:SIZE`** then resolves a region to a register block at 256-byte granularity, because
`0x70000000` holds chip ID, GPIO, I²S *and* I²C:

```
0x7000c000    18 089 231 reads   12 059 987 writes      <- I2C_BASE. Everything else is noise.
```

**`--i2c`** logs each started transfer — sampled when `CTRL`'s `SEND` bit is written — giving the
device address and register index. Register map from Rockbox `i2c-pp.c`: `CTRL` `+0x00`
(`0x80` = send, `0x20` = read, bits 1..2 = `len-1`), `ADDR` `+0x04`, data from `+0x0c` by 4,
`STATUS` `+0x1c` bit 6 = BUSY.

```
dev 0x10  1352 transfers        \ 7-bit address 0x08 = PCF50605 PMU
dev 0x11  2694 transfers        /  (write / read pair)
dev 0x34    50 transfers           Wolfson audio codec

hottest (device, register):
  dev 0x11 reg 0x30   2692
  dev 0x10 reg 0x30   1347
  dev 0x10 reg 0x2e      2
```

`0x30` is **`PCF5060X_ADCS1`** — ADC Status 1 — and `0x2e` is `ADCC1`, ADC Control. **RetailOS starts
a battery-voltage conversion and polls for "conversion ready" on a register that always reads zero.**
Exactly the stopped-clock shape from §5e, one device along.

`--i2c-fill=0xff` makes every I²C data read return all-ones — crude on purpose, an experiment rather
than a device model, answering "is it waiting on a bit that never asserts?" in one run without having
to guess *which* bit. It is:

| | before | after |
|---|---|---|
| `mmio-7` accesses | 30 149 646 | **1 694** |

The firmware moves on. `--pagelog` over `0x60000000` then shows where to:

```
0x60007000    0 reads   18 764 504 writes    <- CPU_CTRL = SLEEP
0x60000000    4 706 370 reads                <- PROC_ID, "am I the CPU?"
```

Which is **the idle loop** — what a booted OS does with nothing to do.

## 5i. The ATA controller was never mapped

`mmio-c` covers `0xc0000000..0xc00fffff`. `IDE_BASE` is **`0xc3000000`** — outside it. Every disk
access the firmware made landed in unmapped space and read back zero, which is why the unmapped
report's one surviving entry was always `0xc3000400`.

With an `ide` region mapped there, the firmware talks to it: **379 reads, 16 writes** where there
were 8 accesses into nowhere. It is probing a controller that reports no drive, so the next piece is
a real ATA model — status with DRDY/DRQ, `IDENTIFY`, and sector reads backed by the disk image that
already holds the 54 games.

## 5j. An ATA model — RetailOS identifies the disk

Register layout from Rockbox `firmware/target/arm/pp/ata-target.h`, a 4-byte stride from
`IDE_BASE + 0x1e0`:

```text
+0x1e0  DATA (16-bit)   +0x1f0  LCYL      +0x1fc  STATUS (read) / COMMAND (write)
+0x1e4  ERROR           +0x1f4  HCYL      +0x3f8  CONTROL (alternate status)
+0x1e8  NSECTOR         +0x1f8  SELECT
+0x1ec  SECTOR
```

`Ata` in `lib.rs` implements `IDENTIFY DEVICE` (`0xec`), `READ SECTOR(S)` (`0x20`/`0x21`/`0xc4`) with
LBA28 addressing, and the no-op commands, backed by a real image file via `--disk=PATH`. Unknown
commands **abort** rather than appear to succeed — a driver waiting for data that is never coming is
indistinguishable from a hang.

With the 8 GB image iTunes synced the games onto:

```
disk ipod8g.img — 16777216 sectors (8192 MB)
ata commands: 0xec ×1, 0xef ×1
```

**RetailOS issues `IDENTIFY DEVICE`, accepts the answer, and issues `SET FEATURES`.** It has found
the disk and negotiated with it.

### The IDE interrupt, and a storm

A drive raises `IDE_IRQ` (23, per `pp5020.h`) on command completion. Without it the driver identifies
the disk and then waits forever — which looks exactly like a drive that is not responding.

Modelled level-triggered first, acknowledged by reading the primary status register at `+0x1fc` as
ATA specifies (the alternate status at `+0x3f8` deliberately does not acknowledge — that is what it
is *for*). That produced an **interrupt storm**: 9,078,058 assertions against 15,411 for the timers.
This handler evidently acknowledges some other way.

It is now **edge-triggered** — cleared once delivered. Assertions fall back to 36,457, timer-dominated.
This is a **deliberate simplification and a known divergence from real hardware**, recorded here
because it is the first place to look if disk behaviour turns strange.

### What the driver actually asks for

Logging the whole request rather than the opcode — which required fixing a bug in our own model,
where `Ata::write` had **no case for `0x1e4`, the FEATURES register**, so every SET FEATURES
subcommand looked identical:

```
cmd 0xec  features 0x00  nsector 0x00  lba 0     IDENTIFY DEVICE
cmd 0xef  features 0x03  nsector 0x0a  lba 0     SET FEATURES: set transfer mode, PIO flow-control mode 2
```

`0x03` is *set transfer mode* and `0x0a` is `0x08 | 2` — PIO mode 2. The model accepts it. Over 40
seconds of simulated time the driver reissues SET FEATURES **four times**, so something above it is
retrying, but it never issues a single `READ SECTOR`.

### Where it stops: idle, not blocked

The interesting part is what did *not* happen: **no `READ SECTOR` was ever issued**, and the machine
is not stuck on the drive. `mmio-6` shows ~38M writes to `CPU_CTRL`, which is `SLEEP` — RetailOS
identified its disk and went back to **idling**.

So the next question is not "why won't the disk work" but **"what is the OS waiting for before it
mounts anything?"** — a scheduler/task question rather than a peripheral one, and a different kind of
investigation from every step above.

The idle task itself is identified, at `0xded04`:

```asm
000ded1c  bl 0x0011c4d0        ; am I the CPU?
000ded24  mrs r10, cpsr        ; ...mask interrupts
000ded50  ldmia r1, {r0, r1}   ; 64-bit idle counter at [r5+0x10]
000ded54  adds r0, r0, #0x1
000ded60  stmia r2, {r0, r1}
000ded64  bl 0x000a058c        ; CPU_CTRL = SLEEP
```

It increments an idle tick counter and sleeps. Nothing is runnable.

**Things ruled out, so they are not retried:**

| Hypothesis | Result |
|---|---|
| Waiting on a long timeout | No — 40 s of simulated time changes nothing but the SET FEATURES retry count |
| Hold switch held (GPIO A bit 5, `GPIOA_INPUT_VAL` `0x6000d030`) — clicky's HLE bootloader sets it | No — `0x20` and `0xffffffff` both change nothing |
| Blocked on the disk | No — the drive answers, and the machine is in its idle task, not a driver wait |

**The next instrument needed is a task enumerator**: RetailOS's scheduler has a task list and a
ready queue, and the question "which tasks exist and what is each blocked on" is not answerable from
a PC profile or a device-access count. That is a different class of tool from everything built so
far, and building it is the honest next step rather than another peripheral guess.

**Also still true: the LCD at `0x30000000` has never been touched.** Whatever gates display init is
the same thing gating the mount.

## 5k. Rockbox as a validation harness — and two hardware bits it found

**The problem it solves:** when something does not work, is it our hardware model or RetailOS's
expectations? That was undecidable, and undecidable questions are where projects burn months.
Rockbox is open source, targets this exact SoC, and we have its source — so it makes the question
decidable.

`rockbox-ipodvideo-3.15.zip` → `.rockbox/rockbox.ipod`, an 8-byte header (checksum + the model tag
`ipvd`) then the raw image, which loads at `0x10000000` exactly like OSOS. Its first instruction is
`msr cpsr_c, #0xd3` — `crt0-pp.S`'s opening.

It ran **100M instructions with zero unmapped accesses and zero faults** first time, which is itself
a strong statement about the memory model. Then it stalled, twice, and each stall was a real bug in
our machine:

**`COP_STATUS` must be sticky.** Rockbox spun 266M reads on `0x60007004`. From `crt0-pp.S`, the CPU
*wakes* the coprocessor by writing `WAKE` (0) to `COP_CTRL` — **the same address** it then polls for
`COPSLEEPING`. So a seeded value is cleared by the very code that waits for it. We do not emulate the
second core, so on this machine the COP is permanently asleep: `Memory::read_overrides` now answers
that address with `0x80000000` regardless of writes.

**The PLL must lock.** Next stall: 26M reads of `0x6000603c` = `PLL_STATUS`, immediately after a
write to `PLL_CONTROL` at `+0x34`, waiting for bit 31. An emulated PLL locks instantly.

With both, Rockbox relocates itself into IRAM, initialises 932 843 bytes of BSS, and reaches **its
own kernel scheduler** — 211 distinct profile buckets, driving the inter-core mailbox at
`0x60001000`. That is deep into boot.

**Neither fix changed RetailOS's behaviour** — a clean negative, and worth having: RetailOS was not
silently depending on either.

## 6. Also now reached: the ATA controller

`0xc3000400` and `0xc3000410` get a read-modify-write from `pc 0x1310`:

```asm
0000130c  mov r0, #0xc3000000
00001310  ldr r1, [r0, #0x400]
00001314  bic r1, r1, #0x1
00001318  str r1, [r0, #0x400]
```

`pp5020.h` gives `IDE_BASE 0xc3000000`. These are config bits being cleared; harmless with reads
returning zero, and not currently a blocker. It is noted because it marks how far the boot gets: far
enough to start bringing up storage.

## Reproducing

```sh
trace <game.bin> --osos=fw/OSOS_correct.bin --boot-osos --osos-at=0x04000000 --sysinfo
```

This command line is now `tools/ipod-boot/warm-boot.sh`. It was moved there when bypass #5 needed
re-validating and it turned out the warm path had no recipe to re-run — see
[research/11](11-rtxc-and-the-video-coprocessor.md) §56.

`--map=0x14000000:0x1000000` is no longer needed — that window is a real alias now, and SDRAM is one
contiguous 64 MB region at `0x10000000` with its remapped view at `0`, replacing a patchwork of
three part-regions with gaps. The firmware's own heap pointer sits around `0x00a0ffa0`, past the end
of the old 7.5 MB low mirror, so anything reaching that far had been writing into unmapped space.

The four `--poke=0x400060xx` stack-slot seeds from the previous session are **no longer needed** —
they were compensating for the coprocessor-boot bug. Reaching the "main OS entry" that way was real
but incidental; the boot now gets there on its own and continues well past it.

## Tooling changes that made this session work

| Change | Why it mattered |
|---|---|
| `Memory::unmapped` is now a per-page `BTreeMap` with counts and the first PC | the old flat log was **capped at 4096 entries**, so every busy run reported the same saturated "4032 reads, 64 writes" — a constant that we had been reading as a measurement |
| `--disasm=ADDR:COUNT` | reads code out of the running machine; the alternative was dumping hex and decoding ARM by eye, which is where guesses come from |
| `--osos-at=ADDR` | finds which image base makes the scatter-load pointers land inside the image, instead of guessing the base |
| `--dump` now runs on the boot path | it had the same early-`return` bug that made `--break`/`--watch` look broken |
| `--profile` — sampled PC histogram, bucketed by 16 bytes | "BudgetExhausted" says nothing about where the time went, and the 16-entry `last instructions` tail only ever shows the innermost loop, never the caller re-entering it |
| `--sysinfo[=SIZE]` — builds the bootloader's IRAM handoff block | the alternative was a pile of `--poke`s substituting for a structure |
| `Memory::aliases` + `translate()`, honoured by `is_mapped()` too | an alias is another view of the same storage, not a second buffer |
| SDRAM is one contiguous 64 MB region in both views | it was three part-regions with gaps, and the firmware's heap lives in one of the gaps |

The unmapped-log cap is the one worth remembering: an instrument that silently saturates does not
report that it has stopped measuring. It reports a number.

## Provenance: where `OSOS_correct.bin` comes from, and what was wrong with `OSOS.bin`

Asked directly, and it was not written down anywhere. It is worth being precise, because **two
separate 512-byte errors** have been made with this image and they pull in opposite directions.

### The source

`resources/derived/fw/Firmware-20.6.3` is Apple's own firmware bundle — 13 895 680 bytes, opening
with the `{{~~ … S T O P` copyright block that every iPod firmware image carries. Its **firmware
directory** sits at offset `0x4200` and holds three `!ATA` records:

| tag | devOffset | length | load address |
|---|---|---|---|
| `osos` | `0x4400` | `0x735a00` (7 559 680) | `0x10000000` |
| `rsrc` | `0x73a000` | `0x500000` | `0x10000000` |
| `aupd` | `0xc3a200` | `0x106400` | `0x10000000` |

The `osos` length and load address match exactly what the emulator reports — `mapped OSOS: 7559680
bytes at 0x10000000`, and Apple's bootloader printing `Running 'osos' 0 from 0x10000000`.

### The extraction, and the off-by-one-sector

`devOffset` is relative to the start of the firmware **partition**; the extracted `Firmware-…`
file carries an additional `0x200` header, so the byte position inside *this file* is
`devOffset + 0x200`.

- **`OSOS.bin`** — extracted at `0x4400`, the devOffset taken literally. **Wrong.**
- **`OSOS_correct.bin`** — extracted at `0x4600`. **Right**, and provably so.

The proof needs no spec. An ARM image entered at its base must begin with the exception vector
table, and `OSOS_correct.bin` does:

```
ea00007a  b   reset          ea000067  b   undefined
e59ff0dc  ldr pc, [pc,#-0xdc]   (SWI)   ea00006b  b   prefetch abort
ea000070  b   data abort     eafffffe  b   .        (reserved — branch to self)
ea000058  b   IRQ            ea00004e  b   FIQ
```

`OSOS.bin` begins `00000000 00000000 68199f6b 6b495415 …` — the tail of the firmware header. Not a
vector table, not executable at `0x10000000`. And the relationship between the two files is exactly
`OSOS.bin[i + 512] == OSOS_correct.bin[i]`: one extra sector of header on the front.

### The other 512 bytes, which is a different bug

The `osos` region written into `resources/derived/disk/ipod8g.img` was for a time the correct image
with its **first sector removed** — `backup[i] == OSOS_correct[i + 512]`. That is the opposite
direction to the extraction error, and it is the one that produced the phantom boot loop diagnosed
and retracted in [research/11](11-rtxc-and-the-video-coprocessor.md) §40.

So: `OSOS.bin` had 512 bytes too many at the front, the disk had 512 bytes too few, and only
`OSOS_correct.bin` — and the disk region as it stands today, which hashes identically to it — is
the real thing.

### Which bootloader we run

**Apple's retail bootloader, out of the iPod's own NOR flash.** `cold-boot.sh` passes
`--flash=resources/internal_rom_000000-0FFFFF/…` and `--cold-boot`, entering at `0x00000000` where
the CPU fetches out of reset. Its own console output is the receipt:

```
(C) Copyright 2000-2006 Apple Computer, Inc.
BootLoader running on iPod M25 cpu Unknown
Retail mode
Running 'osos' 0 from 0x10000000
```

**`ipodloader2` is not in the boot path and never has been on this recipe.** It is used only as a
*documentation source* — its MMAP encoding and register names — and the copy under
`resources/reference/` is there to be read, not run. [research/12](12-bypass-ledger.md) #13 lists
its `loader.bin` as a bypass "*(if used)*"; it is not used, and running Apple's own ROM is both the
higher-fidelity choice and the one already in place.

### Which firmware version this actually is

`Firmware-20.6.3` looks like version 6.3, and no iPod Video firmware ever shipped as 6.3 — published
versions stop at 1.3. That is a trap. The accompanying `manifest.plist` decodes it:

| key | value | decoded |
|---|---|---|
| `BuildID` | `103841792` = `0x06308000` | **6.3.0** — internal, and where the filename comes from |
| `VisibleBuildID` | `19955712` = `0x01308000` | **1.3.0** — the shipping version |
| `FamilyID` | `6` | iPod with Video — the same family iTunes records |
| `UpdaterFamilyID` | `20` | the 5th-generation Video updater family |

**So this is retail RetailOS 1.3**, the last iPod Video firmware, dated 11 Mar 2008. Independently
corroborated by [giek2000/ipod-classic-firmware-research](https://github.com/giek2000/ipod-classic-firmware-research),
whose `specs/iPod_5th_Gen_Video_Late_20_1_3.md` describes `iPod_20.1.3.ipsw` as RetailOS 1.3,
UpdaterFamilyID 20, PP5021C.

This matters because the **NOR dump is a different story**: it carries a placeholder serial
(`U1234567890`) and a blank `HwId`, where the same project's `NOR_FLASH.md` shows a real Apple
serial (`<SERIAL-EXAMPLE>`) in its example. So the two halves of our machine have different provenance —
**retail firmware, prototype/scrubbed flash config** — and only the flash half is suspect.
