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
