# How this was built, and in what order

Four days, 11–14 August 2026. Taken from the commit log rather than memory, because memory was wrong
about several of these — Ghidra especially, which I would have sworn arrived on day two.

| day | commits | what happened |
|---|---|---|
| 1 (from 23:34) | 4 | the ARMv4T core, and iTunes accepting a virtual iPod |
| 2 | 72 | the disk, and snapshot/restore |
| 3 | 113 | **RetailOS boots at 02:06.** Ghidra at 02:16 |
| 4 | 53+ | the display, the menu, a game |

## Day 1 — no iPod in the room

The first problem was not the emulator, it was having anything for iTunes to talk to. That landed
the same night: a virtual iPod that iTunes accepts as a real device, with the USB identity rebuilt
out of real firmware artifacts.

The ARM7TDMI interpreter core was written the same day. About 1 900 lines, zero dependencies, 59
tests including a differential fuzz harness.

## Day 2 — the disk, and the tool that changed the pace

Most of it went on ATA. The DMA engine arms in either order depending on who is driving it, the
interrupt is a level rather than a pulse, and RetailOS will not finish booting until it can *write* —
it blocks on a one-sector write to the first sector of FAT #1, times out after 3.9 simulated seconds,
and retries forever. A read-only disk looks exactly like a deadlock and is not one.

Snapshot and restore were built here, and mattered more than any single discovery: a 110-second
experiment became a 3-second one, which changes which questions are worth asking.

## Day 3 — RetailOS boots, and the tooling changes shape

At **02:06** Apple's bootloader loaded RetailOS, verified it and handed over. At **02:16** a headless
Ghidra server went up, and the ten-minute gap is the whole story of that day. Until then most
questions could be answered by running the machine and watching. Once RetailOS was booting *and then
halting somewhere inside itself*, the questions became "what calls this" and "what is this structure",
and those do not yield to watching.

Rockbox was brought up around here too, as an oracle. It is open, so when something breaks you can
read why — and it is not the same problem as RetailOS: Rockbox treats the co-processor as a
framebuffer, RetailOS treats it as a display server.

## Day 4 — the display, and then a game

The resource volume turned out to hold the answer: the `.vll` codec plugins are ordinary ELF shared
objects for `EM_VIDEOCORE`, and their undefined symbols name the co-processor's entire runtime. That
identified the display stack as DispmanX, which is publicly documented two chip generations later.

The transport itself was derived from RetailOS's own parser, on the principle that code which reads a
structure is a specification of that structure. Then the language picker, the main menu, and Brick.

## Day 5 — a second operating system, a third bootloader, and an honest clock

The strategy changed rather than the pace. **Rockbox was adopted as an oracle**: RetailOS is
stripped C++ with no symbols, Rockbox drives the same PP5022 and ships an ELF with 5 808 of them, so
a divergence stops being a hex address and becomes a function and a line. It paid immediately —
three device models turned out to be shaped around Apple's drivers rather than around the parts, and
none of the three is findable with one operating system.

Then the chain got real. `install-os` writes an OS into a drive image's firmware partition the way
`ipodpatcher` does, and `put-files` writes the FAT32 volume beside it, so Apple's own bootloader
cold-boots somebody else's operating system with nothing warm-entered and no step skipped. The first
installer attempt produced an image the bootloader rejected after 71 ATA commands; the fix — image
data begins one sector past the partition and the directory does not — is measurable rather than a
matter of reading the C, and the installer now reproduces the checksums already present before it
writes new ones.

**`ipodloader2` built and cold-booted**, making three bootloaders. It immediately addressed a 1G
iPod's registers, because one line asks whether byte 16 of a chip-id register is ASCII `'2'` and
nothing in this emulator had ever had to be right about that answer. That is the fourth model shaped
around one driver, and the first found by a stack other than Rockbox.

The day's last change was subtraction. A halted core used to teleport the clock to whichever
interrupt was due next, which made idle time free — an untouched iPod aged thousands of times too
fast and powered itself off about thirteen seconds after you stopped touching the wheel. Now a halt
costs one loop iteration, exactly as running does, so the machine keeps one honest ratio to the real
part whether it is busy or idle. Nothing was added to pace it. The teleport was simply deleted.

## What the work actually looked like

Not a straight line. Several walls turned out to be the emulator's own instruments rather than the
machine: a log that filled its cap and printed the cap as a count, an "is it idle" test that was
really a novelty test, a bus read that popped a FIFO twice per halfword and produced zeros that were
attributed to hardware for a full session. Each one is written down where it happened, because the
record of what was believed and why it was wrong is the part that is hard to reconstruct later.

The Apple boot logo is the neatest example. It was in the co-processor's buffer from the first run,
at 62-halfword pitch, waiting for a rectangle placement that was never executed — so a 62-pixel-wide
logo sat in a buffer being read 320 pixels wide, and looked like diagonal noise. It was dismissed
twice as fragments before anyone thought to look at it in address order instead of screen order.
