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
