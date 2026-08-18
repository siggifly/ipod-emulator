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

### Confirmed by forcing it, one variable

`--rdval=0x70000000=0x32323035` — a value whose bits 23:16 are `0x32`, used as this project's
documented bisect instrument and **not** as a model:

| | unmapped reads |
|---|---|
| as-is | **91 499 510** across 14 pages |
| chip id forced | **8** across 1 page |

The `0xcf00xxxx` traffic disappears completely. The detection was the whole of it.

## Why no earlier stack found this

The same shape as the three Rockbox found, and [ROADMAP](../ROADMAP.md) §"What this project is"
predicts it: **a model shaped around the drivers that have run against it.** Apple's bootloader
reads `0x70000000` 23 times in a cold boot and RetailOS once, and neither of them cares what byte
16 says — they already know what chip they are. `ipodloader2` is the first code here that has to
*ask*, and it is the first code that could be wrong about the answer.

**What it does not yet tell us is the right value.** Rockbox names the register (`PP_VER1`,
`pp5020.h:374`) and never decodes it, so nothing we hold sources the actual bytes a PP5022 returns.
The forced value satisfies the loader's test and is otherwise invented. **Do not promote it to a
model on the strength of this file** — the honest options are a datasheet, a second implementation
that decodes rather than names it, or a reading off real hardware.

## Where it stops now

With the chip id forced, the loader gets past detection and then does **nothing visible**: 400 M
instructions, `ata commands: 0`, no console output, a blank panel, 8 unmapped reads at
`0xc5000140`. So it is stalled before it ever reaches the drive — a different and much smaller
question than the one above, and the next one to work.

**Settled when** the loader draws its own menu. That is [ROADMAP](../ROADMAP.md) M4's first
checkpoint, and it proves the whole chain except the kernel.
