# The register-agreement table

**Two operating systems, one instrument, and a list of every address this emulator invents — sorted
by who depends on it.** Built 2026-08-18, the day Rockbox first ran far enough to be compared with.

## Why this is the instrument, and running two stacks is not

Running Rockbox is not the measurement. It is the *sample*. The measurement is the diff.

One stack can only ever tell you that this emulator satisfies *that stack* — and worse, a model
shaped around a driver is a model that driver can never falsify. Three of those were found in two
days ([research/06](06-rockbox-as-oracle.md)): a USB clock-ready bit, an ADC that completed on
transfer counts, a click wheel that answered Apple's opcode. Each looked correct for years of
RetailOS boots because RetailOS is what it was shaped from.

`--input-regs=BASE:SIZE` already enumerates **addresses read before they were ever written** — which
is, precisely, the list of places where the firmware expects silicon to supply a value and this
emulator supplies whatever the region happened to hold. Run it per stack and the four-way split
falls out of the intersection:

| in | means |
|---|---|
| **both** | a genuine hardware input. Two independent implementations, written years apart by people who never spoke, both expect the part to answer here |
| **one only** | either OS-specific behaviour, or a corner the other stack never reaches |
| **neither** | not exercised — and *unmeasured ground is not green ground* |

## How to regenerate it

Same instrument on both sides, which is the whole point:

```sh
for R in 0x60000000:0x10000 0x70000000:0x10000; do
  BUDGET=600000000 ./tools/ipod-boot/retail-boot.sh --input-regs=$R
  BUDGET=600000000 ./tools/ipod-boot/rockbox.sh     --input-regs=$R
done
```

Names below are Rockbox's own, from `firmware/export/pp5020.h`. An em dash means Rockbox does not
name it either — those are the least-understood addresses in the machine.

## Result — 93 addresses across the two MMIO regions

**18 both · 56 RetailOS-only · 19 Rockbox-only.**

### Both — hardware inputs, on two independent authorities

| address | name | RetailOS reads | Rockbox reads |
|---|---|---|---|
| `0x60005010` | `USEC_TIMER` | 21 691 592 | 15 274 744 |
| `0x60000000` | `PROCESSOR_ID` | 29 096 | 16 014 218 |
| `0x60004000` | `CPU_INT_STAT` | 54 408 | 2 985 564 |
| `0x60005004` | `TIMER1_VAL` | 26 072 | 2 978 224 |
| `0x7000c01c` | *(I²C status)* | 159 398 | 105 344 |
| `0x6000d13c` | `GPIOL_INPUT_VAL` | 52 983 | 116 800 |
| `0x6000d030` | `GPIOA_INPUT_VAL` | 7 | 6 017 096 |
| `0x6000d034` | `GPIOB_INPUT_VAL` | 1 | 174 792 |
| `0x6000500c` | `TIMER2_VAL` | 4 | 3 672 |
| `0x6000603c` | `PLL_STATUS` | 24 | 132 |
| `0x60004020` | `CPU_INT_EN_STAT` | 12 | 12 |
| `0x6000402c` | `CPU_INT_PRIORITY` | 4 | 4 |
| `0x60008000` | *(DMA controller)* | 4 | 4 |
| `0x6000a000` | `DMA_MASTER_CONTROL` | 4 | 4 |
| `0x6000b004` | — | 4 | 4 |
| `0x70000008` | `STRAP_OPT_A` | 4 | 4 |
| `0x7000280c` | `IISFIFO_CFG` | 4 | 4 |
| `0x70006000` | `SER0_BASE` | 4 | 4 |

Every one of these is a value we invent that **two** implementations expect hardware to supply.
That is a materially stronger claim than anything a single-stack run can make, and it is the honest
priority order for anything that wants to model this machine properly.

### Rockbox-only — where we invent for a stack that has source

The cheapest rows in the project: each has a name, a first PC, and 5 808 symbols behind it.

| address | name | reads before write | first pc | note |
|---|---|---|---|---|
| `0x60001000` | **`MBX_MSG_STAT`** | **52 868 892** | `0x0008631c` = **`switch_thread`** | see below |
| `0x7000c014` | — | 1 246 | `0x0007e42c` | I²C data |
| `0x7000c018` | — | 622 | `0x0007e42c` | I²C data |
| `0x70000028` | — | 8 | `0x0007e87c` | the USB clock-ready bit, now modelled |
| `0x60007004` | **`COP_CTL`** | 4 | `0x10000140` | **ledger #7** |
| `0x6000a008` | `DMA_REQ_STATUS` | 4 | `0x03e912e4` | |
| `0x60006034` | `PLL_CONTROL` | 4 | `0x40008714` | |
| `0x70002800` | `IISCONFIG` | 4 | `0x00088018` | audio transport — M6 |
| `0x7000003c` | `XMB_RAM_CFG` | 4 | `0x0007e8bc` | |
| `0x70000080` / `0x70000084` | `GPO32_VAL` / `GPO32_ENABLE` | 4 / 4 | | |
| `0x6000d000`/`08`/`10`/`18` | GPIO base + enables | 4 each | | |
| `0x70000018` · `0x7000002c` · `0x7000008c` · `0x7000c000` | — | 4 / 4 / 4 / 1 | | unnamed by Rockbox too |

## The one that matters: `MBX_MSG_STAT`, 52.8 million times

`0x60001000` is read **52 868 892 times** in a 600 M-instruction run — roughly one read every
eleven instructions — and the first one comes from **`switch_thread`**, Rockbox's scheduler.
RetailOS never touches it at all.

This emulator has no mailbox. The address is ordinary backing store, so it answers whatever is
there, and Rockbox's thread scheduler is leaning on that answer at the rate of a spin loop. That is
not a cosmetic gap: it sits directly underneath the 7 526 995 halts and 1 332 s of skipped sleep
recorded in [research/06](06-rockbox-as-oracle.md), which is to say underneath every timing claim
that could be made about Rockbox here.

**It is also the first thing this table found that no amount of RetailOS work could have.**

## What this does to the bypass ledger

**Ledger #7 — "COP_STATUS says the second core is asleep."** Its retirement condition has always
been about whether anything depends on the lie. The table now separates the two questions properly:
`COP_CTL` (`0x60007004`) is read by **Rockbox and not by RetailOS**, and `MBX_MSG_STAT` — the
mailbox the two cores talk over — is Rockbox-only and enormously hot. So the second core's *state*
is not merely something RetailOS is being told; it is a thing an independent OS actively works
with, through a register we do not model at all.

That does not retire #7 and it does not widen it. It does what the ledger is for: it says the arm
that would test it (`--cop-awake`) has, for the first time, a second consumer to test against.

## The standing warning this table inherits

`--input-regs` reports *reads before writes*. An address the firmware writes first and reads later
does not appear, however load-bearing it is; nor does one neither stack reached in 600 M
instructions. So this is a floor on where we invent, never a ceiling — the same shape of caveat
that `--watch` carried in [research/06](06-rockbox-as-oracle.md), and that went on to produce two
false absences before anyone re-ran the conclusions through the repaired instrument.

**Do not read an empty row as a clean one.** Read it as unmeasured.
