# The boot matrix — real, synthetic 5G, synthetic 5.5G

**Measured 2026-08-19.** Every row is a run, not a claim. The point of the table is that a
synthesised boot ROM now reaches the same place a real one does, and that where it does not is
named rather than left to be discovered.

## RetailOS

| boot ROM | firmware | outcome |
|---|---|---|
| real 5G dump, cold | `iPod_20.1.3` (updater 20) | boots — copyright banner, 597 M instructions |
| real 5G dump, cold | `iPod_25.1.3` (updater 25) | **70 ATA commands** — the wrong-family signature |
| **synthetic 5G** (`MA146`) | `iPod_20.1.3` | **boots** — 597 M instructions, 7 ATA commands |
| **synthetic 5.5G** (`MA446`) | `iPod_25.1.3` | **boots** — 597 M instructions, 7 ATA commands |

The second row is not a failure: `inspect::family_mismatch` documents exactly that shape — *"a
bundle from the wrong updater family boots, is not recognised as this iPod's own software, and
shows the plug-into-a-computer screen after about 70 ATA commands"*. A 5G ROM with 5.5G firmware
**should** land there, and it does.

## Rockbox

Warm-booted from `rb-main.raw` against a drive carrying `.rockbox`, with `--sysinfo` so the handoff
comes from the ROM under test.

| boot ROM | instructions | ATA |
|---|---|---|
| real 5G dump | 16 980 585 | 72 |
| synthetic 5G | 16 483 392 | 90 |
| synthetic 5.5G | 16 483 392 | 90 |

**The two synthetic runs are identical**, which is informative rather than suspicious: Rockbox is
built for a fixed target and does not read the generation out of the handoff, so a `MA146` and a
`MA446` present it the same machine.

**The real-versus-synthetic difference is not explained.** 72 ATA commands against 90, and about
500 000 fewer instructions. Both reach the same budget without faulting. It is recorded here
because an unexplained difference that nobody wrote down is one that gets rediscovered.

## What a synthesised ROM cannot do

**The four NOR modes are not in it, and cannot be.**

```
synthetic       flsh entries: 0
real 5G dump    flsh entries: 4
```

`diag`, `disk`, `scan` and `logo` are self-contained payloads Apple ships inside the flash, indexed
by the `flsh` directory at `0xffe00`. Synthesising the identity block is one thing; synthesising
Apple's diagnostics is not the same kind of task, and this project does not have those images except
inside a dump somebody already owns.

So: **diagnostics mode, disk mode, the disk scanner and the boot logo image require a real dump.**
A synthesised ROM boots an operating system and nothing else. That is worth saying out loud next to
any claim that "all the NOR modes draw", because it is true of a real dump and false of a generated
one.

## What this does *not* establish

**The 5.5G's `HwVr` is still unsourced.** `0x000B0010` came from a code comment. The 5.5G boots with
it, and boots byte-identically with the 5G's `0x000B0005` forced in its place — same instruction
count, same ATA count, same unmapped count. So the boot says nothing about which value is right, and
anyone reading this table should not take "the 5.5G boots" as "the constant is confirmed".

Every other constant in the handoff is measured: `len = 0xf8`, `BoardHwName = "iPod M25"`, the 5G's
Gestalt, the model number, and the `SysCfg` copied in after the struct.
