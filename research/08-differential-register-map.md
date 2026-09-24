# What only RetailOS touches

Two operating systems now boot on this machine. Rockbox reaches a working UI; RetailOS resets. That
makes a comparison possible that was not before: **which registers does RetailOS depend on that no
working OS has ever exercised?** Those are exactly the ones our model could be wrong about with
nothing to catch it.

Both were run to 300 M instructions with `--pagelog` at 4-byte granularity over
`0x60000000`, `0x70000000` and `0xc3000000`:

| | distinct registers touched |
|---|---|
| RetailOS | **206** |
| Rockbox | 109 |
| **RetailOS only** | **112** |

The 112 are listed in full at the end. Two clusters are named by Rockbox's own `pp5020.h`, which is
a register *map* rather than merely a list of what Rockbox uses:

## The DMA controller — and a hypothesis it kills

`0x6000b000` is **`DMA0_BASE_ADDR`** and `0x6000b020` is `DMA1_BASE_ADDR`, so
`0x6000b000..0x6000b0e4` is the PP5020's DMA controller. RetailOS's driver builds it with **four**
channels (`cmp r5, #4` at `0x001da308`); the boot ROM clears eight slots, which is what made the
touched set look eight wide. Rockbox never touches one.

**Both DMA controllers are modelled as of 2026-08-13** — this one *and* the undocumented second
instance at `0x60008000`/`0x60009000`, which is what uploads `vmcs.bin` to the co-processor. The
"unnamed four channels at stride `0x20`" flagged below are that second controller. See
[research/10](10-the-resource-image.md) Addendum 9.

That looked like the answer. An unmodelled DMA engine silently does nothing, so whatever it was
meant to fill **stays zero** — which is precisely the shape of
[research/03](03-rtxc-and-the-video-coprocessor.md) §52's "a field nobody ever wrote".

`--writelog` over the block settles it, and the answer is no:

```
write log: 828 stores recorded, 0 dropped
  pc 0x00001294  0x6000b000 = 0x00000000
  pc 0x000012a4  0x6000b020 = 0x00000000
  …
```

**Every one of the 828 stores writes zero**, from a tight PC sequence at stride `0x10`. RetailOS is
*disabling* all eight channels in an init loop, not programming transfers — 828 stores over 8
channels is ~103 rounds, matching the 103 resets in that run. It reads `+0x04` (a status register we
always answer as 0) 412 times, about four per round, as part of the same sequence.

So the DMA controller is unmodelled, RetailOS only ever clears it, and it is **not** the source of
the unwritten delegate.

## The serial port

`0x70006040` is **`SER1_BASE`** / `SER1_RBR`. RetailOS touches UART 1; Rockbox does not. Worth
knowing because RetailOS may emit diagnostics there that we are currently discarding — the flash's
`diag` image waits on a *different* port (`0xb0020000`), but the same idea applies.

## And it confirms one long-standing bypass

`0x70000030` — [research/04](04-bypass-ledger.md) **#1**, the register "absent from every published
map", where we feed a made-up bit 27 — is in the RetailOS-only set. Rockbox never reads it, and
`pp5020.h` does not name it. So there is **no working OS and no published source that validates our
guess**, which is exactly why it has stayed amber. Now that is measured rather than assumed.

## The full RetailOS-only set

```
0x60003000 0x60003004 0x60003008 0x6000300c 0x60004100 0x6000412c 0x60004144 0x6000414c 0x600050
08 0x6000500c 0x60005014 0x6000602c 0x60006048 0x600060a0 0x600060c8 0x60008000 0x60009000 0x600
09004 0x60009020 0x60009024 0x60009040 0x60009044 0x60009060 0x60009064 0x6000b000 0x6000b020 0x
6000b024 0x6000b040 0x6000b044 0x6000b060 0x6000b064 0x6000b080 0x6000b084 0x6000b0a0 0x6000b0a4
 0x6000b0c0 0x6000b0c4 0x6000b0e0 0x6000b0e4 0x6000c010 0x6000c034 0x6000d004 0x6000d00c 0x6000d
014 0x6000d01c 0x6000d020 0x6000d024 0x6000d028 0x6000d02c 0x6000d03c 0x6000d060 0x6000d064 0x60
00d068 0x6000d06c 0x6000d070 0x6000d080 0x6000d084 0x6000d088 0x6000d08c 0x6000d090 0x6000d098 0
x6000d09c 0x6000d0a0 0x6000d0a8 0x6000d0ac 0x6000d0e0 0x6000d0e8 0x6000d0ec 0x6000d100 0x6000d10
4 0x6000d108 0x6000d10c 0x6000d110 0x6000d114 0x6000d118 0x6000d11c 0x6000d120 0x6000d124 0x6000
d128 0x6000d12c 0x6000d160 0x6000d164 0x6000d168 0x6000d16c 0x6000d170 0x6000d174 0x6000d800 0x6
000d810 0x6000d850 0x6000d860 0x6000d904 0x6000d914 0x6000d924 0x6000d950 0x6000d954 0x6000d960 
0x6000d964 0x70000000 0x70000004 0x70000014 0x7000001c 0x70000024 0x70000030 0x70003800 0x700060
40 0x70006044 0x70006048 0x7000604c 0x7000c120 0xc3000410 
```

Clusters worth a second look: ~~`0x60009000..0x60009064` (four channels at stride `0x20`, unnamed)~~
*— identified 2026-08-13: the second DMA controller's channel array, two channels in use; Addendum 9 —*
`0x6000d0xx`/`0x6000d1xx` (GPIO banks Rockbox never uses), `0x6000d8xx`/`0x6000d9xx` (unnamed),
`0x60003000..0x6000300c`, `0x60008000`, `0x70003800`, and `0xc3000410` — one past the `0x410` IDE
window this emulator models.

## The registers, read off the hardware at last — 2026-09-24

Every address in this document until now was inferred: from literals in the firmware, from what
the machine did when a region was absent, from what Rockbox's headers name. **A serial console on
a real 5.5G (`ipod-toolchain/written/34-the-lab-console.md`) makes the device answer directly.**
`machine()` maps these regions as backing store full of zeros — mapped, in its own words,
"because an unmapped write is a write that never happened" — so the emulator's answer for all of
them is known in advance, and every word below is a measured disagreement.

| region | emulator | hardware | verdict |
|---|---|---|---|
| `0x60000000` mmio-6 | zeros | **`55555555` across the whole 4 KB** | 1024/1024 differ |
| `0x70000000` mmio-7 | zeros | chip id + config | 320/1024 differ |
| `0xc0000000` mmio-c | zeros | zeros | **0/1024 — the emulator is right** |
| `0xc3000000` IDE | zeros | real registers | 658/1024 differ |
| `0xc5000000` USB | zeros | real registers | 207/1024 differ |
| `0xf0000000` cache | zeros | **ARM instructions** | 995/1024 differ |
| `0x30020000`, `0x30060000` LCD | `0xFF` | **`00000000`** | 1024/1024 differ |
| `0x30030000`, `0x30070000` LCD | `0xFF` | **`b280b280`** | 1024/1024 differ |

### Four things worth acting on

**1. `PROC_ID` is `55555555`, not `0x00000055`.** `lib.rs` does
`m.mem.write32(0x6000_0000, 0x0000_0055)`. The firmware reads it with `ldrb`, so the low byte
matches and nothing has ever broken — but **the entire 4 KB window reads `55555555` on hardware**,
so any word-width read of anything in mmio-6 disagrees. The comment calls it "PROC_ID (read as a
byte; 0x55 = CPU)"; the hardware says the value is on every byte of every word in the window.

**2. The LCD `0xFF` fill is wrong, and it was a documented guess.** The source says the region is
"Filled with `0xFF` rather than zeros because the driver spins on a ready bit". Hardware says two
of the four windows read `00000000` and the other two read **`b280b280`**. A driver that spins on
a ready bit is satisfied by `b280b280`, not by `0xFF` — and `0xFF` was chosen to make the spin
stop, not because anything measured it.

**3. `0xf0000000` contains ARM code.** `ebfff8d8` (`bl`), `e3a00004` (`mov r0, #4`), `e1a00810`
(`lsl`), `e58a0004` (`str`). It is the cache data array holding recently-executed instructions,
which is what `ipod-toolchain/written/30` §19 concluded from it containing the dumper's own
filename. Backing store cannot model this at any initialisation value.

**4. `0xc0000000` is genuinely zeros.** Worth recording because it is the one region where the
emulator's assumption survives contact, and a differential that only ever finds faults is not
being read carefully.

### What the instrument does NOT show, and why

The `v` command reads a range twice back to back and reports what moved. It found **almost nothing
volatile** — and that is a limitation, not a result. Two reads microseconds apart cannot see a
slowly-changing register. The proof is in this document's own data: the third chip-id word at
`0x70000008` read `003e5082` earlier in the session and `003f008e` here. It changes; `v` cannot
see it. A volatility pass worth trusting needs a delay between reads, and does not exist yet.

### The full 64 KB, and a live register nobody has named — 2026-09-24

The first pass above read 4 KB per region. Asking for 64 KB produced **the same numbers**, because
the console clamps a single read to 4096 bytes and the harness believed the short reply — it
reported "1024 of 1024 words differ" for a window it never read. Chunked and count-asserted, the
picture changes:

| region | differ | of | live |
|---|---:|---:|---:|
| `0x60000000` mmio-6 | 8 450 | 16 384 | **16** |
| `0x70000000` mmio-7 | 674 | 16 384 | 0 |
| `0xc0000000` mmio-c | **0** | 16 384 | 0 |
| `0xc3000000` IDE | 16 384 | 16 384 | 0 |
| `0xc5000000` USB | 3 275 | 16 384 | **5** |
| `0xf0000000` cache | 15 465 | 16 384 | 1 |
| `0x30020000`–`0x30070000` BCM ×4 | 16 384 each | 16 384 | 0 |

#### `0x60006038` — a free-running counter, undocumented, and aliased

Sixteen addresses moved between back-to-back reads, at a perfect `0x100` stride:
`0x60006038 + n * 0x100`. **They are one register, not sixteen.** The control is a static
neighbour: `PLL_CONTROL` (`0x…34`) reads `8a121403` and `PLL_STATUS` (`0x…3c`) reads `80000034`
at *every* alias, so bits 8–11 are not decoded in this block — the same incomplete decoding that
governs the rest of this part.

It sits **between `PLL_CONTROL` (`0x60006034`) and `PLL_STATUS` (`0x6000603c`)**, and Rockbox's
`pp5020.h` names neither it nor anything else at `0x…38`. Measured: 16-bit, changes on every read
(`00007648` → `000013ce` within one round trip), values spread across the whole range — a counter
wrapping far faster than a serial round trip, which is why consecutive samples look unordered.

**What it is remains open; what it does is not.** It is live, and `machine()` returns a static
zero for it. Firmware that polls it — for a PLL lock, a delay, or entropy — sees a constant in the
model and a moving value on the device, and would spin forever in one and not the other.

#### The other live ones

- **USB, 5 addresses** at `0xc500?f84`/`0xc500?384`, toggling `28000605` ↔ `28000205` — one status
  bit (`0x400`), also aliased.
- **Cache, `0xf0000000`** — the DATA register, changing as code executes. Expected, and it is the
  one place backing store is obviously wrong.

#### IDE is state-dependent across runs, and `v` cannot see that

`0xc3000000` read `00003131`/`80003371` in one pass and a flat `00000050` in another minutes later,
with `v` reporting **0 volatile both times**. Two reads microseconds apart cannot see a register
that changes with disk state. `v` finds fast registers; it is blind to slow ones, and a region it
calls stable is only stable *at that timescale*.

### The IIS registers `PAUSED.md` blamed are real — 2026-09-24

The work stopped on 2026-09-08 left a suspected fault it could not confirm: `IISFIFO_WR` running
at a hardcoded 44 100 frames/s "because `IISCONFIG` (`0x70002800`) and `IISFIFO_CFG`
(`0x7000280c`) are unmodelled", costing a claimed **628x**. Doom was the arm that would have
priced it and was never run.

Read off the hardware:

```text
  70002800  20000070     <- IISCONFIG
  70002804  a000000a
  70002808  0000001f
  7000280c  00100031     <- IISFIFO_CFG
  70002810..7000283c     all zero
```

**Both hold real configuration.** The emulator maps neither, so both read zero, and four words of
audio-clock configuration are simply absent from the model.

**What this settles and what it does not.** It settles the *premise*: the registers exist, carry
non-zero configuration, and are unmodelled — that is no longer a suspicion. It does **not** confirm
the 628x figure, which is a claim about cost and still needs the arm that was never run. A premise
measured is not a conclusion earned.
