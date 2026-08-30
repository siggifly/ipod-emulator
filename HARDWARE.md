# Lending the project an iPod

**The scarcest thing here is a real device.** This emulator is built by comparing what it does
against what hardware does, and every question of the form *"is this right?"* eventually needs
one. If you own a click-wheel iPod, you can answer questions nobody else can — usually in an
afternoon, usually without opening it.

This page is what to do. You do not need to write code, and you do not need to give anything
away permanently.

## Which iPods help, and why

Two families, and they share nothing that matters. A device from the wrong family cannot
answer a question about the other one.

| Model | Chip | Family | Priority |
|---|---|---|---|
| **iPod 5G / 5.5G (Video)** | PP5022 | PortalPlayer | 🔴 highest — the generation everything targets |
| **iPod nano 1G** | PP5022 | PortalPlayer | 🔴 highest — *the same chip designation as the 5G* |
| iPod mini 2G | PP5022 | PortalPlayer | 🟠 high |
| iPod 4G · iPod Photo | PP5020 | PortalPlayer | 🟠 high — the previous revision |
| iPod mini 1G | PP5020 | PortalPlayer | 🟡 useful |
| **iPod classic 6G** | S5L8702 | Samsung | 🟠 high, later work |
| **iPod nano 3G** | S5L8702 | Samsung | 🟠 high, later work — *same chip as the classic 6G* |
| iPod classic 7G | S5L8702-class | Samsung | 🟡 later |
| iPod nano 2G | S5L8701 | Samsung | 🟡 later |
| iPod nano 4G | S5L8720 | Samsung | 🟢 completeness |
| iPod 1G / 2G / 3G | PP5002 | PortalPlayer | 🟢 completeness |

Chips are read from Rockbox's own per-model configuration, not from a wiki.

**iPod touch and anything running iOS is out of scope**, and always will be. This project stops
at the click wheel.

## What actually helps, easiest first

**A ROM dump.** Install Rockbox, open *System → Debug → Save ROM contents*, keep the file. Five
minutes, no disassembly, no soldering. Sometimes a patched build is wanted to read an address
the stock dumper does not — that patch is usually about five lines and we will supply it.

**A `SysInfoExtended` capture** — what your iPod tells iTunes about itself when it connects.

**Recorded gameplay.** A phone camera pointed at the screen, plus a written note of which
buttons got you to each screen, is **ground truth an emulator can be checked against**. This is
genuinely valuable and needs nothing but patience.

**A photograph of the board.** Some questions can only be settled by seeing which physical part
was fitted — one flash-chip question in this project's notes says exactly that.

**Lending a device for a while**, if you are comfortable with it. Most questions do not need
this; a few do.

## What we will never ask you for

- **Your Apple ID, password, or anything you bought.** Not needed for any of the above.
- **To modify anything permanently.** Rockbox installs alongside Apple's software and uninstalls.
- **To open the case**, unless you want to and the question genuinely requires it.

## The one thing that makes a contribution usable

**Say where it came from.** Which model, which firmware version, which address range, and what
was on screen. A dump with no provenance is a number nobody can re-derive, and it will
eventually be thrown away by someone who cannot tell whether to trust it — which wastes your
afternoon, not ours.

A single line is enough:

```
iPod 5.5G (A1136), 80 GB, firmware 1.3, Rockbox 4.0 stock build,
Save ROM contents, 2026-08-30, dumped by <name or handle>
```

## How to offer

Say hello in Discord — the link is in the [README](README.md) — and mention what you have.
There is a channel for hardware questions, kept as a standing list precisely so that when
somebody has a device open on the desk, it can answer several at once instead of one.

If you would rather not use Discord, open an issue using the **Hardware question** template.

**Answering a question is worth more than owning the device**, and a null result counts. "I ran
it and the file was all zeros" is a real finding that redirects work — several times here, the
absence of something has been more useful than its presence.
