# Known bugs

Things that are **wrong**, as opposed to things that are **absent**. Three lists, deliberately kept
apart, because merging them is how a project starts describing its gaps as choices:

| | |
|---|---|
| **This file** | defects. It behaves differently from the hardware and that difference is not intended |
| `README.md` § *What does not* | features that were never built. No audio, no USB — absent, not broken |
| `research/12-bypass-ledger.md` | deliberate fakes, each with a written condition for retiring it. Known, chosen, and load-bearing |

A bug that turns out to be a bypass belongs in the ledger. A bypass nobody can justify any more
belongs here.

---

## The cold boot takes far longer in simulated time than hardware does

**~300 seconds of simulated time to reach the menu, against five or ten on a real 5G.** The wall
clock is a separate matter — the interpreter runs at roughly 30 % of hardware speed, which would
account for a factor of three, not thirty.

So something waits far longer than it should. A timer that is counted rather than elapsed, a poll
loop spinning on a register that only changes on an interrupt this emulator does not deliver, or a
delay calibrated against a clock that is running at the wrong rate. It has not been chased down.

**What you see:** the Apple logo, then a white screen for most of a minute. The white screen is
faithful — the hardware shows one too — but not for that long. The footer now carries a progress
bar during the boot, so at least it is visibly working rather than apparently hung.

**How you would know it is fixed:** a cold boot that reaches the language picker in a simulated time
of the same order as the hardware's, without the instruction count falling.

## The hold switch does not reach RetailOS after boot

The GPIO line is right and RetailOS reads it — four times, all before instruction 49 689 152, and
never again. What is missing is a **GPIO interrupt**, which this emulator does not model at all. The
OS latched the value once during startup and has no reason to look a second time.

`H` in the window therefore moves the switch, and nothing downstream notices.

**How you would know it is fixed:** toggling hold at the main menu stops the wheel from scrolling.

## `MENU`+`SELECT` and `PLAY` are delivered and ignored

Held for 400 M instructions at the main menu, the machine keeps running (`research/20` Addendum 31
§5). On a real 5G that pair is caught by the wheel controller or the PMU before the OS is involved,
and neither is modelled. The window says so on screen rather than pretending the buttons work.

## Four values in the co-processor transport are chosen, not measured

The display path was derived from RetailOS's own parser rather than from a datasheet, and four
values in it were picked because they worked. There is also **no timing model** in that transport at
all — every reply is instant. A bug that only appears when a reply is late is therefore invisible
here, and would be a difference from hardware nobody would see until real timing arrived.

Listed in `research/12` with retirement conditions. Repeated here because "it works" and "it is
right" are not the same claim.

## Purchased titles do not launch

Apple's DRM refuses them. The identity it binds to is understood — the 8-byte FireWire GUID in
`sysinfo_t` — but supplying the right one gets past identity and into a keystore that returns a null
context, and the wall after that is a control-flow-flattened function.

Arguably not a bug: the emulator is doing what the firmware tells it to. It is here because someone
will reasonably expect a game they bought to run, and it will not.

---

## Not bugs, though they look like ones

- **A white screen during a cold boot.** The hardware does this too. Its *length* is the bug above.
- **The charging screen after a while.** Authentic 5G behaviour with a charger attached.
- **"Restore from iTunes" after `make-disk`.** The IPSW's updater family does not match the iPod
  your NOR dump came from. `make-disk` prints the family for exactly this reason.
- **`aupd` running the flash updater on first boot.** Correct — it takes two boots.
  `ipod-boot flash-update` is the recipe.
