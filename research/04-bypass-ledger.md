# The bypass ledger

**Every shortcut currently in the boot path, what it fakes, and what retiring it requires.**

Per the doctrine in the open-player research *(not published)*: *borrow freely to learn, never to depend.*
The purpose of this file is that no bypass can quietly become permanent. **A bypass with no
retirement condition written down is a bug we have agreed not to see.**

Status key — 🔴 known-wrong, kept because it reveals what is behind it · 🟡 plausible but unverified ·
🟢 correct model, listed only because it substitutes for hardware we do not emulate.

**The `Live in` column is the answer to "is this still switched on, and where?"** It was added
2026-08-14 after an audit found two recipes still passing three bypasses this file had marked
RETIRED — `flsh.sh` and `flash-update.sh` carried #1 and #2, and `flash-update.sh` carried #3 as
well, which meant the run that *proved* #12's retirement was obtained with three retired bypasses
switched on. A ledger that records a retirement but not where the flag still lives cannot catch
that. Every cell in the column is checked against the `exec` lines in `tools/ipod-boot/*.sh` and
against `trace.rs`/`lib.rs`, because **some bypasses are not flags at all** — #7, #8 and #9 are
installed by code with no way to turn them off from a recipe, and the column says so.

The six recipes: `cold-boot.sh` · `retail-boot.sh` (which `exec`s cold-boot.sh with the retail
ROM and disk, so it inherits every flag) · `flash-update.sh` · `flsh.sh` · `warm-boot.sh` ·
`rockbox.sh`.

---

## Active ledger entries

**Still faking something, on some path.** **One** is live on the retail path every number in
`research/` is measured on — #6, the synthesised co-processor replies. #4 and #11 are retired on the
cold path and survive only in the three warm-entry recipes, which by construction have no
bootloader. **Two rows left this table on 2026-08-19**: #17, whose flags were deleted rather than
switched off, and **#7, retired by running the second core and making it the default**.

| # | Bypass | Live in (2026-08-14) | What it fakes | Why it exists | Status | Retiring it requires |
|---|---|---|---|---|---|---|
| 4 | `--sysinfo` | `flsh.sh` · `warm-boot.sh` · `rockbox.sh` — **and no cold path.** `cold-boot.sh`, `retail-boot.sh` and `flash-update.sh` pass no `--sysinfo` and print no `sysinfo at …` line; the ROM writes the block itself, which is this row's own retirement condition already met on the retail path | the flash bootloader's IRAM handoff block | warm boot has no bootloader to write it | 🟢 | cold boot writing it, as hardware does — **done, for the three cold recipes.** It survives only in the three warm-entry recipes, which by construction have no bootloader |
| 6 | **BCM replies synthesised** | **ALL SIX recipes** — every one passes `--bcm`. 🔴 **the only known-wrong bypass live on the retail path** | `BCMA_COMMAND` acknowledgement; `CONTROL` returns a fixed `0x52` | renders frames without a VideoCore emulator | 🔴 | **executing the `vmcs` firmware — and we now know exactly where it is.** `vmcs.bin` (201 376 bytes) sits in `/Resources/VideoCore/Boot/` inside the **`rsrc`** image on the firmware partition, beside `render.bin` and six codec libraries (`aacdec`, `h264dec`, `mpg4dec`, `mplayer`, `passthrough`, `slideshow`). The same 5 MB volume holds the font whose absence causes the boot loop — see [research/10](10-the-resource-image.md). The display bypass and the boot loop are the same missing image. ~~The attribution measurement stands — RetailOS never touches the co-processor at all, byte-identical BCM traffic~~ *(**wrong since Addendum 9 and marked 2026-08-14**: RetailOS writes **100 696 halfwords** into `0x30000000` — the whole of `vmcs.bin` — and reads 24. What **is** byte-identical, measured at the ROM→RetailOS handoff and at 600 M, is **the framebuffer**: same 76 816 halfwords at `0x000e0000`, same 2 922 non-black pixels, identical PPM files. The display half of the attribution stands; the "touches the co-processor" half does not. See [research/10](10-the-resource-image.md) Addendum 14 §2)* — but the boot loop it was clearing does not exist (§40) ([research/03](03-rtxc-and-the-video-coprocessor.md) §39). **2026-08-13, retail path: the retirement condition is now a specific address, not a firmware image.** RetailOS *does* start the upload — `APPLEBOOT` arms a transfer channel whose fields are literally `{src = 0x13eaf188 (the vmcs.bin buffer), dst = 0x30000000, chunk = 0x10000, engine regs = 0x60009000}` and then blocks in `KS_pend` on semaphore `0xe0` waiting for the first 64 KB chunk. ~~Nothing ever writes `0x60009000`~~ *(instrument bug — `--watch-range` could not see word stores; retracted, see Addendum 7 §5)*. **2026-08-13, resolved: `0x60009000` is a second PP502x DMA controller and is now modelled.** It moves all 201 216 bytes of `vmcs.bin` into the co-processor in 4 chunks, and RetailOS's own ISR retires each one — so this bypass's retirement condition has narrowed again, from "upload the firmware" to "answer the request the co-processor is now asked for" (`KS_pend` on `0xea`). BCM halfwords written 129 876 → 230 572. See [research/10](10-the-resource-image.md) Addendum 9. **2026-08-14 — this bypass is now the named cause of the blank panel, and the retirement condition is four concrete artefacts.** RetailOS runs the whole bring-up itself (`FUN_00287698` @51 753 290, `FUN_00287998`, the 201 376-byte upload) and then asks the co-processor who it is: `FUN_00288058` reads 16 bytes at BCM internal `0x1f0` and requires word 2 == 1 and word 3 a valid pointer to an 8-entry service directory. Nothing has ever been written near `0x1f0` — by the bootloader or by RetailOS — so the read returns zeros, the directory at `0x108d3bd4` stays zeroed, `FUN_00286aa8` finds no tag-2 service, the RPC channel index at `0x1082359c` keeps the `-1` its BSS initialiser gave it, `FUN_00286ca8` early-returns, no display layer is ever created, and `FUN_00164f44` — the function that uploads a frame — has **0 arrivals in 1.61 G instructions** while its caller `FUN_001650f8` is asked to flush 42 times. Retiring #6 now means: (1) the `0x1f0` block, (2) the service directory, (3) per-service descriptors tagged 1/2/7, (4) the request/reply ring behind `FUN_0028861c`/`FUN_00288434`. See [research/10](10-the-resource-image.md) Addendum 26. **2026-08-14, later the same day — two of those four are built, and one sentence above is retracted.** ~~the read returns zeros~~ *(**wrong**: the model's `Bcm::read8` served each half of an `ldrh` from a fresh FIFO pop, so a 16-byte read drained 32 bytes and returned bytes spliced from alternate words. `0x1f8` held `1` throughout; the CPU was handed `0x2f01fc78`. Fixed, A/B'd both ways — the pre-fix path reproduces this file's baseline to the instruction — see [research/10](10-the-resource-image.md) Addendum 29 §1)*. With the read honest the gate that actually fails is `(w3 & 3) == 0`, and `--bcm-registry` now publishes (1) the `0x1f0` header, (2) an 8-slot channel directory, (3) one 0x50-byte record tagged 2, and (4) a responder for the ring — **all derived from RetailOS's reader, none of it tuned**, layout and evidence in Addendum 29 §2–§3. **The flag is OFF by default and is in no recipe**, because it is still a bypass: four things in it are *chosen* rather than derived (Addendum 29 §6), the largest being that surfaces are allocated from `0xE0000` on Rockbox's authority rather than the co-processor's. With it on, `FUN_00164f44` runs **41 times** and `--bcm-dump=0xE0000` differs from the handoff dump — **76 607 non-zero pixels against 2 922**, RetailOS's own "Charged" screen. So #6's colour does not change and its severity does: it is no longer *the* blocker on seeing what RetailOS draws, and what remains of it is a real co-processor for the parts §6 lists as chosen. **The current state of this bypass — what the registry supplies, what in it is still assumed, and what retiring it now means — is the section **"#6 today"** below.** Everything in this cell above that link is the chronological record and is kept as written |
| 11 | ~~NOR mapped at `0` **forever**~~ **RETIRED 2026-08-13** | **the three warm-entry recipes** — `warm-boot.sh`, `flsh.sh`, `rockbox.sh` keep the old layout, which RetailOS's scatter-load depends on. Retired on the cold path: `cold-boot.sh`, `retail-boot.sh`, `flash-update.sh` | the reset-time aliasing | correct out of reset; hardware remaps SDRAM over it | 🟢 **cold boot fixed** · 🟡 warm | **2026-08-19: the same row had a second half nobody had looked at — the WRITE path.** Retiring #11 fixed which region *reads* resolve to; `write8_inner` was still testing the flash windows on the **raw** address, so once firmware programs the MMAP unit to put SDRAM over `0`, every **byte** store below 1 MB was handed to the JEDEC chip, swallowed, and returned — while `write32`'s fast path resolved through `translate` and landed in RAM. A width-dependent memory system, which is not a thing any part is. Cold-booted Rockbox is what found it: `disk_init` writes `partinfo[].start` with an `stm` and `.type` with a `strb`, the start survived and the type did not, every partition read as type 0, `disk_mount_all()` returned 0, and it sat on *"No partition found (0). Insert USB cable"* forever. Fixed by testing `n.hit(translate(addr))` — before the remap `translate` is the identity, so the reset-time aliasing and the updater's writes at `0x20000000` are untouched. **`disk_mount_all()` 0 → 1, ATA 113 → 10 304, 0 → 74 057 lit pixels, and Rockbox reaches its main menu from a cold boot.** Retail unmoved at 599 ATA and 2 916 pixels; `flash-update` byte-identical to a reverted baseline. Regression: `a_byte_store_lands_where_a_word_store_does`, confirmed failable — **after a first version of it passed against the bug**, because with no `nor` installed the swallowing branch is never entered and the assertion proved nothing. **This one bit, exactly as written.** SDRAM's storage was a single region at `0`, with `0x10000000` an *alias* onto it — so NOR at `0` won first-match and every read of `0x10000000` returned flash while writes went elsewhere. `osos` checksummed the NOR reset vectors. Cold boot now puts SDRAM where the hardware has it (`0x10000000`) and leaves `0` to NOR. Warm boot keeps the old layout, which RetailOS's scatter-load depends on; the MMAP remap is still unmodelled |

## Retired ledger entries

**Nothing here fakes anything any more.** Each row keeps its full evidence: what it faked, why it
existed, and what retiring it actually required — because the retirement is the finding, and a
ledger that deleted its own history would be worth less than one that never had it.

| # | Bypass | Live in (2026-08-14) | What it fakes | Why it exists | Status | Retiring it requires |
|---|---|---|---|---|---|---|
| 1 | `--rdval=0x70000030=0x08000000` **RETIRED 2026-08-13** | **no recipe** — modelled unconditionally as `Xmb` in `lib.rs`, installed by `map_hardware`. Left in `flsh.sh` + `flash-update.sh` after retirement; removed 2026-08-14 after the A/B below | bit 27 of a register **absent from every published map**, between `DEV_INIT2` and `DEV_TIMING1` | the ROM polls it during SDRAM bring-up | 🟢 | **Done, from the ROM's own use of it.** It is the external memory bus controller: **bit 30 is the NOR write gate** and **bit 27 is the controller's ready flag**. Not SDRAM bring-up — the polling helper's six call sites all bracket a JEDEC flash command sequence. Modelled as `Xmb` in `lib.rs`; the boot is byte-identical without the flag. See §Retiring #1 below |
| 2 | `--rdtoggle=0x7000003c=0xc0000000:0x80000000`, latterly `--rdval=0x7000003c=0x80000000` — **RETIRED 2026-08-13** | **no recipe** — same `Xmb`, same story: removed from `flsh.sh` + `flash-update.sh` 2026-08-14 | `XMB_RAM_CFG` bit 30 **alternating** | the ROM waits for bit 30 to go *set*, then waits for it to go *clear* — a busy-flag handshake no static value can satisfy | 🟢 | **Done. It is a handshake, and the firmware is the thing that starts it**: **bit 24 is the SDRAM configuration command, bit 31 its completion**. The row's own description was wrong in two ways — the bit is 31, not 30, and there is no clear-phase to wait for. What is real is that no *value* is ever the answer, because the completion has to follow the command. See §Retiring #2 below |
| 3 | `--i2c-fill=0xff` **RETIRED 2026-08-13 by `--pmu`** | **no recipe** — `flash-update.sh` still carried it for a day after retirement; swapped for `--pmu` 2026-08-14 | **every** I²C data read returned all-ones | the PCF50605's ADC never reported conversion-ready otherwise | 🟢 | **Done.** A real PCF50605 — register map from Rockbox, iPod Video power-on defaults, read-clearing INTs, pointer auto-increment, read-only ADC status, and a conversion that takes observable time. The missing fact was **ADCS2 bit 7 = conversion-ready**, which all-ones had been setting by accident. Boots on 1 785 PMU transfers against 797 095. The "253 restarts against 255" comparison recorded here was measured against a corrupted disk image and describes a boot loop that does not exist (§40). See [research/03](03-rtxc-and-the-video-coprocessor.md) §38. **2026-08-14: the replacement device carried a defect of its own for a day, and it was load-bearing.** "a conversion that takes observable time" was implemented by counting the busy state down *inside the read of `ADCS1`*, which also answered `0` while counting — so Apple's single two-byte `ADCS1`+`ADCS2` poll took one byte from the in-flight state and the next from the completed one, and **every conversion the firmware ever accepted carried the value zero**. Fixed by latching the result when the countdown expires and advancing it once per read transfer; `busy = 2` is unchanged, so nothing is tuned. It kept `GPIOL` lying about a charger for a day and the screen showing "Charged" instead of the menu — [research/10](10-the-resource-image.md) Addendum 30, and note that this is the fourth model defect this project has first attributed to missing hardware |
| 5 | `sysinfo + 0x84 = 0x000B0005` **RETIRED 2026-08-13** | inside `--sysinfo`, so wherever #4 is: `flsh.sh` · `warm-boot.sh` · `rockbox.sh`. Not a bypass any more — the value installed is the machine's own | the Gestalt ID, in a field iPodLinux labels only as `pad7[120]` | RetailOS reads its model from there | 🟢 | **Corrected to the machine's own value and re-validated.** The field is the flash `SCfg` record **`HwVr`** at flash `0x4054`, fetched by key lookup at `0x400098dc`; the cold path reads **`0x000b0011`** straight out of the NOR dump, so the warm path now installs that. Not a bypass any more — it is the same number the hardware supplies, installed by hand because no bootloader ran. Re-validated on `warm-boot.sh`, 600M at `--clock=5`: **104 arrivals at the selector** `0x2653a4` from the same two call sites, 18 ATA commands, 220 unmapped reads and **0 unmapped writes**, 17 972 code buckets against 17 968. The one visible change is that RetailOS's first MBR read becomes `nsector 4` / 2048 bytes where it was `nsector 1` / 512. See [research/03](03-rtxc-and-the-video-coprocessor.md) §54–55 and §56 |
| 8 | `PLL_STATUS` always locked | **ALL SIX recipes**, by the same unconditional push at `trace.rs:2106` and under the same `read_overrides.is_empty()` guard as #7. Its one measured reader outside the retail path is `flsh.sh`'s `disk` image, which reads `0x6000603c` exactly once at instruction 71 043 from `0x100099c0` — and reads it as `0x80000000` now where it read the register file before, with no change to that run's outcome | `0x6000603c` bit 31 | an emulated PLL locks instantly | 🟢 **RETIRED as a whole-word override 2026-08-17** | **Done, and the row was understating what it did.** "nothing — this is arguably correct" was true of the *claim* and false of the *mechanism*: the entry lived in `read_overrides`, which replaces the whole word, so every read of `0x6000603c` returned exactly `0x80000000` and the other **31 bits were forced to zero** — a second fake, never written down, on a register whose remaining bits nobody had checked. It is now an **OR-mask** (`read_or_masks`): the register reads as whatever it holds, with bit 31 asserted. That is the whole claim and nothing else. **Equivalent today, and measured to be**: nothing writes `0x6000603c` in a boot (`--watch-writes=0x6000603c:4`, 300 M, no writers), so the base is zero and the two mechanisms agree — **25 506 code buckets at 1.7 G, the same number this file's baseline carries**, 305 ATA commands, full framebuffer. The value of the change is that it stops being *accidentally* equivalent: if anything ever does write that register, the mask preserves it. **The switch-over first broke the boot, and the reason is the reusable lesson.** `Memory` serves whole pages of plain storage without consulting the per-address tables, and `page_is_plain` is the list of tables that take a page off that fast path — `read_toggle`, `read_overrides`, `int_ack_on_read`, the DMA blocks, `usec_timer`, `bcm`. `read_or_masks` was added to the read path and *not* to that list, so the mask was never consulted: the bootrom's lock-bit poll at `0x8780` (`r0 = 0x60005000`, offset `0x103c`) read the region's zero and spun, **the machine never left instruction 23**, and a 1.7 G run reported 887 code buckets and 0 ATA commands. A mechanism that is present, correct in isolation, and never reached is the failure mode this project keeps meeting; `a_read_or_mask_is_observed_through_the_ordinary_read_path` in `tests/load_and_trace.rs` is the regression, and it was confirmed to fail against the un-fixed `page_is_plain` before being kept |
| 9 | `IDE0_CFG` bit 3 = interrupt pending | **ALL SIX recipes — and it is not a flag either.** `Ata::read` in `lib.rs` ORs `0x08` into offset `0x28` whenever `irq_pending`, unconditionally; there is no switch and no `--rdval` interaction. The only related flag is `--no-cfg-ack`, which ablates the *acknowledgement* (#10), and no recipe passes it | the controller's interrupt latch | the ROM polls it with IRQs masked | 🟢 **confirmed 2026-08-13** | **Confirmed from RetailOS, which is better evidence than `ipodloader2`.** Its ISR at `0x000fc6c8` writes `0x20400020` to `0xc3000028` to acknowledge a completion, and its ATA driver at `0x00232c64` writes `0x20408028` while arming a wait — both setting the `0x30` clear bits. A register written to *acknowledge* is an interrupt latch; "data ready" cannot explain an ISR clearing it. The latch now moves with the interrupt line rather than independently. **RETIRED 2026-08-17 — RetailOS names the bit itself, and the arm that was missing now exists.** `--regs-at=0x000fc6c8:2` catches the ISR at the store with `r0 = 0xc3000028`, **`r1 = 0x20400028`** and **`r2 = 0x20400020`**: it reads IDE0_CFG, finds bit 3 set, and writes the same word back with **exactly that bit cleared**. The bit assignment is therefore not ours and not iPodLinux's comment — it is read straight off Apple's interrupt handler, twice, at 50 503 739 and 50 511 875. The bypass had no arm B and had not had one since before this file existed; `--no-ide-irq-latch` is it, and the A/B at 600 M is not subtle: **102 ATA commands / 22 904 code buckets / 76 800 non-black pixels with the latch, against 24 / 1 733 / 2 916 without** — the second is the bootloader's own screen, RetailOS never gets its disk. A modelled register that firmware demonstrably reads, whose bit position the firmware itself identifies, and whose removal stops the boot, is emulation and not a bypass. **The census that produced this nearly did not.** `--watch-writes` logged the value by peeking the address, and `count()` runs ahead of both the device models and the region copy — so on MMIO it recorded the byte *about to be replaced*, which for a device register is always zero, and the report drops zeros as memset noise. `--watch-writes=0xc3000028:4` therefore said "**3 824 logged, 0 distinct pc**" for a register with fifteen writers. Fixed by passing the written byte in; the same run then names the ISR at `0x000fc6c8` (944 byte-writes) and the ATA driver at `0x00232c64` (916) without being told where to look |
| 10 | IDE IRQ **edge**-triggered **RETIRED 2026-08-13** | **no recipe.** Level-triggered with both acks is now the default in code; `--no-cfg-ack` reproduces the historic storm on demand and is in no recipe | real ATA holds it until the status register is read | level-triggered produced an interrupt storm — 9,078,058 assertions against 15,411 for the timers | 🟢 | **Done — and the storm that justified this bypass was an instrument ceiling, not a measurement.** 9,078,058 is 97 % of 600 M ÷ 64, the most a per-64-instruction sampler can emit; it means *saturated*, not *this many*. The line is now held until acknowledged, and there are **two** acks, not one: a read of the primary status register **and** a write of IDE0_CFG's clear bits (`0x30`), which RetailOS's ISR uses and Apple's bootloader never needed because it polls with IRQs masked. Modelling only the first is what stormed. With both, a correct level model asserts *fewer* interrupts than the edge model did — 183 452 against 402 864. Ablating the second ack (`--no-cfg-ack`) reproduces the historic storm exactly: 8 585 404 asserted, 968 014 IDE deliveries. See [research/10](10-the-resource-image.md) Addendum 4 |
| 12 | `aupd` directory entry removed **RETIRED 2026-08-13 on the retail ROM** | **no recipe** — it was a disk edit, not a flag, and no disk carries it now. Still open on `cold-boot.sh`'s prototype ROM. **Its retirement proof was re-run 2026-08-14 without #1/#2/#3 and is unchanged** — see below | a firmware partition with no flash-update image | the ROM runs the flash UPDATER instead of the OS | 🟢 retail · ⚪ prototype (out of scope) | **Done, and the disk edit turns out to have been the updater's own output.** With Apple's directory intact — `osos`, `rsrc`, `aupd` — the retail ROM runs `aupd`, the updater completes, and it finishes by issuing **`WRITE SECTORS` to LBA 96**, setting `aupd`'s `+0x08` to 1 so the next boot skips it. The second boot prints `Retail mode` / `Running 'osos'`. Two things were needed: a **JEDEC NOR model** (`--nor`), because the ROM identifies the chip before it will touch it and against bytes the reply was `0x1ffe`/`0xea00`; and **`--disk-writable`**, because read-only the updater's own bookkeeping write aborts and the machine updates forever. `Device Flash Version: FFFFFFFF` was a red herring — it is a `vers` lookup at flash `0x4040`, not a CFI query, and the updater proceeds regardless. Recipe: `tools/ipod-boot/flash-update.sh`. **CLOSED as out of scope 2026-08-17.** It stayed open on one thing: the prototype ROM, which reads its firmware partition at 4× the MBR LBA and, handed the same `aupd`, reads the image in full and then powers off without printing. That ROM was never made to boot and is not a target -- the machine this project emulates is a retail 5.5G. The behaviour is recorded rather than carried as a debt. See [research/07](07-the-flash-images.md) |
| 13 | `ipodloader2`'s `loader.bin` **NOT IN USE** | **no recipe, and never was executed** — it is a documentation source | Apple's bootloader's handoff | it has **zero `aupd` handling**, so it was kept as a way to bisect the blocker | ⚪ | **Nothing — the retirement condition is already met.** `cold-boot.sh` boots Apple's own retail bootloader out of the NOR dump, entering at `0x0`; the console receipt is `BootLoader running on iPod M25 … Retail mode … Running 'osos' 0 from 0x10000000`. `ipodloader2` is a *documentation source* (its MMAP encoding and register names), never executed. See [research/02](02-retailos-boot.md) §Provenance |
| 14 | `--osos=FILE` in the cold-boot recipe **RETIRED 2026-08-13** | **no cold recipe.** `--osos=` remains in the three warm-entry recipes, where it is not a bypass but the only way in: `warm-boot.sh` (`OSOS_correct.bin`), `flsh.sh` (a flash image), `rockbox.sh` (the oracle) | a pre-placed `OSOS_correct.bin` region at `0x10000000` | ~~`--boot-osos` structurally requires it~~ — it never did | 🟢 | **`--boot-osos` requires an image at the ENTRY, which is not the same as requiring `--osos=`.** Only a *warm* boot enters `0x10000000` with nothing but zeroed SDRAM; a cold boot enters NOR at `0` and the ROM loads the image itself. The one thing the pre-placed copy was still buying was `extract_symbols`, a *reporting* instrument — so symbols are now recovered from the SDRAM the ROM filled, bounded by the ATA DMA high-water mark, and produce **the same 140 names at the same addresses**. Removing the flag left the run identical to the instruction, and **what the ROM loads is byte-identical to `OSOS_correct.bin` across all 7 559 680 bytes**. It also stopped the recipe silently swallowing 40 unmapped writes — see below |
| 15 | MMAP window size fixed at **64 MB** **RETIRED 2026-08-13** | **no recipe** — it was a hardcoded constant in `lib.rs`, now decoded | the size/mask field of every MMAP window | the base addresses decoded from `ipodloader2`; the size field did not, so it was hardcoded | 🟢 | **Decoded, not guessed.** Rockbox's `crt0-pp.S` names the halves: the size is a `MASK` in **LOGICAL** bits 13:4 (bit *m* compares address bit *m+16*), and PHYSICAL holds only R/W/DATA/CODE `FLAGS`. The `0x0f84`-vs-`0x3f84` difference this row blamed is PP502x vs PP5002 — a part number, not a size. RetailOS's SDRAM window leaves address bit 26 uncompared, so it answers for `0x04000000` as well as `0x00000000`; that don't-care is exactly what it was reading through. See [research/03](03-rtxc-and-the-video-coprocessor.md) §33 |
| 16 | **— NEVER EXISTED.** The number was skipped, not vacated | **n/a** | nothing — there was never a bypass here to fake anything | **The gap is a miscount, and it is checkable.** `git log -p --all -- research/04-bypass-ledger.md` contains **no row 16 in any revision**, and `git log -p --all` across the whole repository contains **no `#16` at all** except ARM shift amounts (`mov r4, r4, lsr #16`). At `f7348b6^` the table read `1…12, 14, 15, 13` — fifteen rows whose highest number was 15 — and `f7348b6` added the VideoCore ablation as **17**. Every other retirement in this file left a struck-through tombstone row; this one has no tombstone because there is nothing to entomb | 🟢 n/a | n/a — this row exists so the gap stops reading as a silent deletion, which is the failure mode the whole file is against |

| 17 | ~~`--force-vc-upload` · `--force-vc-retire`~~ **RETIRED 2026-08-19 — flags deleted** | **no recipe, and now no code.** Both spellings, the `force_vc_retire` field, the `force_retire_log` and the hot-loop block that zeroed `channel+0x18` are gone from `lib.rs` and `trace.rs` | the VideoCore transfer engine's two completion signals: RTXC semaphore `0xe0`, and the four in-use bytes at `channel+0x18` | **an ablation, deliberately kept known-wrong.** `APPLEBOOT` blocked forever uploading `vmcs.bin`; this opened the door to see what was behind it | 🟢 | **Done — and the retirement condition was met six days before the code was removed.** `0x60009000` is a second PP502x DMA controller, modelled since 2026-08-13; it moves all 201 216 bytes of `vmcs.bin` in 4 chunks and **the in-use ring drains to `[0,0,0,0]` with the emulator never touching it**, because the real driver's ISR owns it (research/10 Addendum 9 §6). The model reproduces every number this ablation ever measured — 62 tasks, 256 ATA commands, 302 DMA transfers — and additionally delivers the firmware, which the ablation never did. The ledger said for six days that the flags "should be deleted"; keeping a known-wrong switch alive for a comparison nobody was going to re-run is how a codebase accumulates bypasses that outlive their reason. `--force-sem=ID[,ID…]` survives, because "make an RTXC pend return" is a general instrument rather than one experiment's switch, and it announces every pend it eats |

| 7 | ~~`COP_STATUS` sticky `0x80000000`~~ **RETIRED 2026-08-19** | **no recipe and no default.** `trace.rs` installs the override only under `--no-second-core`, which is arm B and is in nothing | the coprocessor is permanently asleep | we ran one core; firmware wakes the COP and waits for it to sleep again | 🟢 | **Done — a second ARM7TDMI runs from the reset vector, and the default was flipped on evidence rather than on principle.** Both cores enter at the same address; the coprocessor reads `PROC_ID`, decides it is `0xAA`, and takes Apple's seven-instruction path at `0x8054` — park at `COP_CTL`, two nops, `ldr pc, [0x40000050]`. **The one thing that had to be got right was concurrency, not registers**: Apple's bootloader writes that entry vector *two instructions* before the wake, and with a 1000-instruction quantum the CPU ran on into the OS and overwrote it, so the coprocessor read an instruction word, jumped to it as an address and wandered 27 660 256 code buckets. A wake now ends the running core's turn. **At the moment of flipping, every recipe measured here is identical with one core and two** — retail **599 ATA commands / 2 916 non-black pixels**, cold-booted Rockbox **10 304 / 74 057**, in both arms — so the flip re-baselines nothing and every number already in `research/` stands. `--cop-trace` gives the coprocessor the instruments the CPU has had all along, and `--no-second-core` remains as arm B: not a bypass, but a smaller machine than the part, which is an honest thing to be able to ask for |

## What is actually switched on, on the path we measure

Verified 2026-08-14 against the `exec` lines in `tools/ipod-boot/*.sh`, `trace.rs` and `lib.rs`.

**`retail-boot.sh` — the configuration every current number in `research/` is measured on — carries
exactly four live bypasses:**

| # | why it is live | switchable? |
|---|---|---|
| **#6** BCM replies synthesised | `--bcm` is in the recipe | yes — and 🔴 **the only known-wrong one still on this path** |
| **#7** COP asleep | `trace.rs:2102`, no flag | no |
| **#8** PLL locked | `trace.rs:2106`, no flag | no |
| **#9** `IDE0_CFG` bit 3 | `Ata::read`, no flag | no |

Three of the four cannot be turned off from a recipe at all, which is worth stating plainly: a reader
who greps the recipe for "which bypasses am I running" finds one of the four.

**#4 `--sysinfo` is not on the retail path at all.** It is easy to assume it is, because it is the
only bypass in the table whose row does not say "retired" — but `cold-boot.sh`, `retail-boot.sh` and
`flash-update.sh` pass no `--sysinfo` and print no `sysinfo at …` line. The cold boot writes the
handoff block the way the hardware does, which is #4's own retirement condition. It survives only in
the three warm-entry recipes, which by construction have no bootloader to write it — and there it is
not drift, it is the entry mechanism.

Everything else in the table is either retired, or an ablation (#17) that no recipe carries.

## #6 today — what `--bcm-registry` supplies, what is still assumed, and what retiring it now means

2026-08-14. The row above is a chronology; this is the current state, so that a reader deciding
whether to trust a display measurement does not have to reconstruct it from nine retractions.

**#6 is still 🔴 and still live in all six recipes**, because every recipe passes `--bcm` and `--bcm`
is the synthesis: `BCMA_COMMAND` acknowledged, `CONTROL` answering a fixed `0x52`. What changed on
2026-08-14 is that the *consequence* of #6 — a panel that could not be drawn to — is separable from
#6 itself, behind a second flag.

### What `--bcm-registry` supplies

Four artefacts, all derived field-by-field from RetailOS's own reader ([research/10](10-the-resource-image.md)
Addendum 29 §2–§3), published by `on_write(0x10000400)` — the same trigger that already synthesised
"firmware up":

1. **The 16-byte header at co-processor internal `0x1f0`.** `+0x08` reads exactly `1` (the value
   `FUN_00288058` tests for) and `+0x0c` is a non-zero 4-aligned pointer to the directory.
2. **An eight-slot `u16` channel directory** at that pointer. A slot's value is a byte offset from
   the directory base, and **the matching slot's index is the channel id** — which is what
   `FUN_00286aa8` stores where the `-1` used to stay.
3. **One 0x50-byte record**, tagged **2** (the display service) in slot 0: service tag at `+0x04`,
   TX ring bounds at `+0x06`/`+0x08`, RX at `+0x0a`/`+0x0c`, and four ring pointers at `+0x10`,
   `+0x20`, `+0x30`, `+0x40` — each alone in its own 16-byte block so either side can move its own
   without touching the other's.
4. **A responder for the ring.** A write to record `+0x20` (the doorbell) drains every complete
   request from the TX ring and appends a reply to the RX ring, in the 16-byte wire format
   `{magic 0xf1a55a1f, sequence, opcode, length}` with the payload padded up to 16.

Tags **1** (GENCMD — one service on one channel, a printf-formatted command string in and a text
stream back) and **7** (unidentified) are *not* published. Nothing in a boot that ends at the
charging screen needs them.

### What it buys, measured

Both arms of `retail-boot.sh --clock=5 --stop-when-idle=400000000` at `BUDGET=4000000000`, run
today:

| | `--bcm-registry` **off** — the control | **on** |
|---|---|---|
| Idle at | 1 610 279 157 | 1 609 736 757 |
| code buckets | 38 266 | 38 518 |
| ata commands | 770 | 706 |
| `pp dma` | 4 transfers, 201 216 B | **104 transfers, 5 225 216 B** |
| `bcm` halfwords | 230 572 written, 28 read | **2 749 468 written, 5 420 read** |
| `bcm gencmd` | — | **165 requests answered, 0 dropped** |
| unmapped | 4 reads, 0 writes | **none** |
| `0xE0000` non-black pixels | **2 916** — the boot ROM's Apple logo, centred | **76 607** — RetailOS's own "Charged" screen |

> *The control column read `2 922 — the boot ROM's logo, byte-identical to the handoff dump` until
> 2026-08-14. The hunch in it was right and the number was of a picture in the wrong place: those
> 2 922 were the logo lying unplaced in the co-processor's command-parameter buffer, plus the six
> non-zero words of the rectangle header that said where it belonged. `LCD_UPDATERECT` is
> implemented now; the same pixels sit centred at (129,81)-(190,158) and score **2 916**.
> [research/14](14-the-apple-logo.md).*

**Off is the control arm and every other number in `research/` is measured on it.** The flag is in no
recipe for that reason: it changes 5 MB of DMA traffic and 64 ATA commands, so a run that carries it
is not comparable to anything measured before it.

### What is still assumed — the four, plus the timing model that does not exist

This is the part that must not be lost, because a description that reads as complete while parts of
it are chosen is worse than an admitted gap. From Addendum 29 §6, verbatim in substance:

1. **Where the base lives (`0x40000`) and how big the rings are (8 KiB each).** The reader constrains
   the base only to be non-zero and 4-aligned, and the rings only to fit in a `u16` offset from it.
2. **Handle values.** They are a counter. Nothing in the reader constrains them beyond non-zero.
3. **Non-8 reply payloads.** Opcodes other than 8 reply with a handle in payload word 0. Six call
   sites read that word; none of them branches on it in any path reached here, so the value is
   unconstrained by measurement.
4. **Surfaces are allocated from `0xE0000` upward — on Rockbox's authority, not the
   co-processor's.** Rockbox calls `0xE0000` `BCMA_CMDPARAM` and puts the panel image there, and
   Apple's bootloader fills exactly `0xe0000..0x10581e`. The choice is consistent with the published
   map, but the reply format says the co-processor returns *an* address, not *which* — **so if this
   is wrong, the frame lands somewhere else and the 76 607-pixel claim is about the wrong buffer.**
   ~~This is the largest of the four and the one to attack first.~~
   **Attacked 2026-08-14 and it is now a *known-wrong* choice, still in place.** `0xE0000` is the
   co-processor's **command-parameter buffer** — the staging area for the command interface, whose
   `LCD_UPDATERECT` reads an 8-word header there and places the rectangle behind it
   ([research/14](14-the-apple-logo.md)). A real co-processor would not hand that address out as a
   free resource. The model still does, because moving it means modelling what makes a surface
   *visible* (`element_add` / `update_submit`, proposed rather than derived), which is a bigger
   speculative step than the one it fixes. **The 76 607-pixel claim is unaffected and now has a
   control**: the whole registry arm is byte-identical across implementing the command interface —
   `Idle @1 812 316 856`, 38 476 buckets, 706 ata, 521 gencmd answered, and `cmp` silent on the two
   framebuffer dumps.

And a fifth thing, which is not on that list because it is an absence rather than a choice:

5. **There is no timing model.** The reply is placed **synchronously, inside the doorbell write**.
   RetailOS tolerates that because `FUN_002883d4` refreshes the co-processor's write pointer
   (`FUN_002885cc`) before it blocks — but a real co-processor answers later, and **any bug that
   only appears when the reply is late cannot appear here.** This emulator has already made the
   answers-too-early mistake twice, with the drive's `IDE_COMPLETION_USEC` and the click wheel's
   `OPTO_REPLY_USEC`; both times the firmware armed a wait and only *then* acknowledged, and a device
   that finished inside the store raced code written assuming it could not.

### What retiring #6 now means

It has narrowed three times and it narrows again here. It is no longer "execute `vmcs.bin`", and it
is no longer "publish a service directory" — that is built. **Retiring #6 now means removing the
five assumptions above**, in this order:

1. **Establish where a surface really lives.** Either from the co-processor's own code (the export
   table at `0x2160C` in the `rsrc` `vmcs.bin` gives `dispman_*` and `vc_image_*` real addresses —
   [research/11](11-the-videocore-runtime.md)), or by an ablation: move the allocator's base and see
   whether the drawn frame follows it. If it does, the pixels are ours and the address is arbitrary;
   if the panel goes blank, the address is load-bearing and must be sourced.
2. **Give the responder a delay** and re-run, so "answers late" is exercised rather than assumed.
3. **Publish tags 1 and 7**, at which point GENCMD becomes reachable and the third service is
   forced to identify itself.
4. **Retire the `--bcm` synthesis underneath it** — `BCMA_COMMAND` and the fixed `0x52` — which is
   the original #6 and the only part that is still on every recipe.

Only when a frame reaches the panel with none of those five in place is the row 🟢. Until then, a
drawn frame is evidence that RetailOS's *own* pipeline works end to end — which is what Addendum 29
claimed and all it claimed — and not evidence that we have a co-processor.

**The ARM side of that pipeline, described as it works rather than as it was discovered, is
[research/12](12-how-retailos-draws.md).**

## Retiring #1 — `0x70000030` is the NOR write gate, and it is nothing to do with SDRAM

Removing `--rdval=0x70000030=0x08000000` strands the boot before it prints a single character, in a
three-instruction spin. That is the whole identification, because the spin says what the register is:

```
40001378  mov  r2, #0x70000000
4000137c  ldr  r1, [r2, #0x30]
40001380  tst  r1, #0x8000000      ; bit 27
40001384  beq  0x4000137c
40001388  ldr  r1, [r2, #0x30]
4000138c  cmp  r0, #0x0            ; the argument
40001390  movne r0, #0x40000000    ; bit 30
40001394  bic  r1, r1, #0x40000000
40001398  orr  r0, r1, r0
4000139c  str  r0, [r2, #0x30]
400013a0  bx   lr
```

**Wait for bit 27, then set bit 30 to a boolean.** Six `BL`s reach it, and they pair off:
`0x40009dc8`/`0x40009df8`, `0x4000a108`/`0x4000a140`, `0x4000a3d0`/`0x4000a408` — a `1` on the way
in, a `0` on the way out. What sits between each pair is a **JEDEC NOR command sequence**. One pair
calls it directly:

```
40009f88  ...
40009f9c  strh r3, [lr, #0xaa]     ; 0xAAAA -> 0xAAAA
40009fa8  strh r3, [r12, #0x54]    ; 0x5555 -> 0x5554
40009fb0  strh r3, [lr, #0xaa]     ; 0x9090 -> 0xAAAA   (autoselect)
40009fb4  ldrh r3, [r1, #0x0]      ; manufacturer / device ID
40009fd0  strh r3, [r1, #0x0]      ; 0xF0F0             (reset)
```

The other two dispatch through `[r0, #0x8]` and `[r0, #0xc]` of a 0x30-stride table at
`0x400150e0`, indexed by a device type held in the object at `+0x1c`. That table is the corroboration
— its first word per row is a **JEDEC ID pair**:

| row | `+0x00` | manufacturer |
|---|---|---|
| 0 | `22b200ec` | `0x00EC` — Samsung |
| 1, 2 | `226b0001` | `0x0001` — AMD / Spansion |
| 3 | `273f00bf` | `0x00BF` — SST |

and the `+0x8` method, `0x40009e54`, is a sector erase in the same command set: unlock, `0x8080`,
unlock, `0x3030` at the sector address, poll, `0xF0F0`. The object the whole driver works from
carries the magic `'Cfi!'` at `+0x00`. So bit 30 is the **NOR write gate** — closed, a store to flash
is an ordinary store; open, it is a flash command —
and bit 27 is the controller saying the gate may be moved. That places the register with its
neighbours in `pp5020.h` (`DEV_TIMING1` `+0x34`, `XMB_NOR_CFG` `+0x38`, `XMB_RAM_CFG` `+0x3c`): it is
the head of the **external memory bus** group, which is why the SDRAM path touches it too — the
bring-up at `0x4000359c` clears bits 11:8 and sets bits 16 and 10 in it immediately before
programming the RAM. Hence the original "polled during SDRAM bring-up": the register is shared, the
*poll* is not.

**Bit 27 is not an echo of bit 30, and that is measured rather than argued.** The enable path waits
for bit 27 while bit 30 is still clear, so a bit that mirrored the gate would deadlock on the first
call — which is exactly what removing the override does. Nothing in the image writes bit 27 either:
all three writers reach `+0x30` through `bic`/`orr` of other fields.

The model is therefore: **bit 27 is a read-only ready flag the firmware cannot clear, and the rest of
the word is ordinary storage.** A bus that finishes every access inside the access is never busy, so
it reads ready — but the difference from the `--rdval` is not cosmetic. The override answered the
*whole word* as `0x08000000`, so the gate, and the `bic`/`orr` fields at 16, 11:8 and 7:4, read back
as something the firmware never wrote. Now they read back what it wrote, and the gate is observable:

```
xmb: NOR write gate opened 4 times, closed 4; SDRAM config kicked 2 times
```

Four opens, four closes — paired, as a bracket must be. And `--pagelog=0x0:0x10000:0x4` finds
**16 byte-writes at `0x0000aaa8`** and 16 at `0x00000000` in the NOR aperture and nowhere else in the
low 64 KB: four command sequences of two halfword writes each, one per gate opening.

## Retiring #2 — `XMB_RAM_CFG` bit 24 is the command and bit 31 is its completion

Removing `--rdval=0x7000003c=0x80000000` strands the boot at `0x400035c4`. The function around it is
SDRAM bring-up, and it is a two-shot command sequence:

```
40003590  stmdb sp!, {r4-r6, lr}
40003594  cmp  r0, #0x2                ; only for memory type 2
4000359c  mov  r4, #0x70000000
400035a0  ldr  r0, [r4, #0x30]         ; the bus control register — bits 11:8 clear,
400035a4  bic  r0, r0, #0xf00          ; bits 16 and 10 set
400035a8  orr  r0, r0, #0x10400
400035ac  str  r0, [r4, #0x30]
400035b0  ldr  r0, =0x200716d0         ; the geometry word
400035b4  str  r0, [r4, #0x3c]
400035b8  ldr  r1, [r4, #0x3c]
400035bc  orr  r1, r1, #0x1000000      ; bit 24 — GO
400035c0  str  r1, [r4, #0x3c]
400035c4  ldr  r1, [r4, #0x3c]
400035c8  tst  r1, #0x80000000         ; bit 31 — done
400035cc  beq  0x400035c4
400035d0  bic  r5, r0, #0x30000        ; bits 17:16 come from the probe
400035d4  bl   0x40008ba8
400035d8  orr  r0, r0, r5
400035dc  str  r0, [r4, #0x3c]
400035e0  ldr  r0, [r4, #0x3c]
400035e4  orr  r0, r0, #0x1000000      ; kick again
400035e8  str  r0, [r4, #0x3c]
400035ec  ldr  r0, [r4, #0x3c]
400035f0  tst  r0, #0x80000000
400035f4  beq  0x400035ec
400035f8  ldmia sp!, {r4-r6, pc}
```

**Stage the configuration, set bit 24, wait for bit 31.** Twice, because `0x40008ba8` in between is an
array **size probe** — it stores `0x30000` at `0x10000040`, then writes `0x10000840`, `0x10000440` and
`0x10000240` with different values and reads the first back, which is address-line aliasing detection
— and its answer becomes bits 17:16 of the second configuration.

So the ledger row was right that no value can satisfy this, and wrong about everything else. The bit
is 31, not 30. There is no set-then-clear phase — both waits are for the same bit, in the same
direction. And an alternating value is not merely unlike hardware: it would have passed *whatever*
the firmware did, including not issuing the command at all.

The model is a completion that follows the command: **writing bit 24 completes the configuration and
sets bit 31; writing the register without bit 24 stages a new configuration and clears it.** Applying
a configuration to a modelled array takes no time, so the completion lands on the kick — but it lands
*because of* the kick, which is the part a constant could not express. `ram_kicks` reads **2** over a
whole boot, which is the two configurations and nothing else.

### How both are modelled

`Xmb` in `lib.rs`, installed unconditionally by `map_hardware` — this is not optional hardware, and
it was never a debugging aid. It keeps its state **in the register file rather than in the struct**,
by filtering the byte a store leaves behind, so a snapshot carries the handshake without knowing the
device exists. The two counters print on every boot because both are falsifiable: gate opens must
equal closes, and kicks must equal the bring-up's configurations.

### What it is worth

| 600 M instructions, `--clock=5 --pmu` | before (two `--rdval`) | after (modelled) |
|---|---|---|
| instructions executed | 599 999 952 | 599 999 952 |
| console | `Running 'osos' 0 from 0x10000000` | identical |
| ata commands | 88 | 88 |
| ata dma | 60 transfers, 7 565 312 bytes | identical |
| unmapped | 160 reads, 0 writes, 1 page | identical |
| irqs | 403 199 asserted, 95 217 taken | identical |

Byte-identical output, flags removed. Rockbox — the oracle — is byte-identical too, at the documented
913 611 instructions, and reports **zero** gate opens and **zero** kicks: it is warm-entered after the
bootloader has already done the bring-up, so it should never touch either register, and it does not.

One side effect worth stating, because it is a trap in `trace.rs` rather than in the hardware:
`--rdval` **suppresses the built-in `COP_STATUS` + `PLL_STATUS` pair** (bypasses #7 and #8), which are
installed only when no override is given at all. Dropping the recipe's two flags therefore switched
those two *on*. Controlled for by re-running the old recipe with both added explicitly: byte-identical
to the old recipe, so they are inert on this path — but the guard means the honest A/B is a
three-way, not a two-way, and it would not have been if the defaults were unconditional.

**What is still assumed.** That bit 27 reads ready *always*. Nothing in this ROM can distinguish that
from a ready flag with a real busy window, because nothing in the image ever finds it clear — the
firmware's own waits are the only readers, and they are satisfied on the first read. Settling it
needs the part's datasheet or a logic capture of a real PP5022 during a flash write, neither of which
this project has. The same caveat, smaller, applies to bit 31: the completion is immediate here
because the modelled array has nothing to configure.

## Retiring the flags in `flsh.sh` and `flash-update.sh` — the two recipes the retirement missed

**Measured 2026-08-14.** #1, #2 and #3 were retired on 2026-08-13 and the flags were removed from
`cold-boot.sh`. They were **not** removed from the other two recipes that carried them, and this file
did not notice, because until today it recorded *that* a bypass was retired and never *where its flag
still lived*. What that cost:

| recipe | still passing | ledger row | retired |
|---|---|---|---|
| `flsh.sh` | `--rdval=0x70000030=0x08000000`, `--rdval=0x7000003c=0x80000000`; and **no `--nor`** | #1, #2 | 2026-08-13 |
| `flash-update.sh` | both of the above **plus `--i2c-fill=0xff`** | #1, #2, #3 | 2026-08-13 |

The second line is the one that matters. `flash-update.sh` is the recipe that **proved #12's
retirement** — 248 sector erases, 507 904 words programmed, a result byte-identical to the pristine
ROM — and that proof was obtained with three retired bypasses switched on. A proof carrying known-
false models is not obviously wrong, but it is not obviously right either, and the only way to find
out is to re-run it.

### The A/B is a three-way, because `--rdval` has a side effect

`trace.rs:2102` installs #7 (`COP_STATUS`) and #8 (`PLL_STATUS`) **only `if read_overrides.is_empty()`**.
Any `--rdval` therefore *suppresses* them. Dropping a recipe's two `--rdval`s does not remove two
bypasses, it removes two and adds two. So every arm below has a control that keeps the old flags and
adds the COP/PLL pair back explicitly, separating "the flags did nothing" from "two changes
cancelled".

### `flsh.sh` — the flags are inert, and the proof is stronger than byte-identity

Four arms per image, `IMG=diag` and `IMG=disk`, budget 200 M:

| arm | flags |
|---|---|
| A | as the recipe stood |
| A2 | A, plus `--rdval=0x60007004=0x80000000 --rdval=0x6000603c=0x80000000` (the suppressed pair, restored) |
| B0 | the two `--rdval`s removed |
| B | B0 plus `--nor` — the proposed recipe |

**All four are identical, on both images, apart from the banner lines** that echo which flags were
given (`diff` output is three lines: the two `rdval …` announcements and, for B, the `nor model:` and
`nor: 0 sector erases` lines). Same halt PC, same `51 185 488` unmapped reads, same
`ata commands: 0`, same `xmb: … 0 times`.

Byte-identity is the weak form of this result. The strong form is `--readlog`, which says the
registers are **never read at all**:

```
diag  --- reads of watched addresses: 2000000 ---
        [0xb0020000] read by 0x1000c6a4  x2000000   <- positive control, log cap
disk  --- reads of watched addresses: 1 ---
        [0x6000603c] read by 0x100099c0  x1  first @71043
```

Zero reads of `0x70000030` and zero of `0x7000003c` in either image, against a control in the same
run that logged two million reads of an address that *is* touched — a control capable of passing, per
the lesson in §snapshot/restore. An override on a register nothing reads cannot do anything.

The one live reader found is #8: the `disk` image reads `PLL_STATUS` exactly once. Under the old
flags that read returned the register file; under the new ones it returns `0x80000000`. The run is
byte-identical either way, so the value is not load-bearing — but it is the reason the control arm
exists, and it is the only case in this whole exercise where removing the flags changed *any* value
the firmware saw.

**Why the A/B is inert is itself a finding, and it is not a flattering one.** `flsh.sh` does not
currently boot either image: `diag` spins forever at `0x1000c6a0` polling an unmapped halfword at
`0xb0020000`, and `disk` goes `Lost` after 127 952 instructions. Neither reaches the memory bus
controller, which is exactly why neither register is read. So this arm retires the flags, and does
**not** establish that #1/#2 are safely retired *on a working `flsh.sh`* — nothing here exercises
them. The retail path does, and that is where the retirement's evidence actually lives.

`--nor` is added for the same reason `cold-boot.sh` has it: without it the flash is a byte array that
answers a JEDEC identify with `0x1ffe`/`0xea00`. It is currently unexercised here — `nor: 0 sector
erases, 0 words programmed` — and is passed so that whatever gets these images running meets a device
rather than a mismatch.

### `flash-update.sh` — #12's retirement proof, re-obtained without the three bypasses

Four arms, each a fresh disk built from the pristine `Firmware-20.6.3`, each booting twice, budget
600 M per boot. Two on the pristine retail ROM (where the authentic outcome is a no-op plus the
bookkeeping write) and two on a scratch ROM with 64 bytes zeroed at `0xc0000` — 47 of which actually
differ from the pristine bytes — which is what forces the erase/program path.

| | A: as-is (`--rdval` ×2 + `--i2c-fill=0xff`) | B: proposed (`--pmu`, no `--rdval`) |
|---|---|---|
| boot 1 console | `Running 'aupd'` · `iPod CFI Flash Firmware update` ×2 | identical |
| boot 1 ata commands | 24 | 24 |
| boot 1 `xmb` | 5 gate opens, 5 closes, 2 kicks | identical |
| boot 2 console | `Retail mode` · `Running 'osos' 0 from 0x10000000` | identical |
| boot 2 ata commands | 256 | 256 |
| boot 2 `xmb` | 4 opens, 4 closes, 2 kicks | identical |
| unmapped | 4 reads, 0 writes, 1 page | identical |
| boot 2 ata dma | 512 transfers, 22 595 072 B | 511 transfers, 22 563 328 B |

**and on the perturbed ROM, which is the actual #12 proof:**

| | A: as-is | B: proposed |
|---|---|---|
| `nor:` | **248 sector erases, 507 904 words programmed** | **identical** |
| cycle tallies | `0x30`×248 · `0x80`×248 · `0xa0`×507 904 · `0xaa`×508 406 · `0x55`×508 406 · `0x90`×6 · `0xf0`×255 | **identical, every one** |
| `xmb` gate | 255 opens, 255 closes | identical |
| flash after boot 1 vs the pristine dump | **byte-identical** | **byte-identical** |
| the two arms' repaired flash images vs each other | — | **bit-identical (`cmp`)** |

**The retirement claim for #1, #2 and #3 survives, and #12's proof survives with it.** The three
bypasses were doing nothing that decided anything. The A2 control arm is byte-identical to A, so the
`read_overrides` guard was inert here too.

The only differences anywhere in the four arms are **one ATA DMA transfer** (32 768 bytes) and **4 BCM
halfwords out of 370 606**, both at the tail of a *budget-exhausted* run, plus the instruction the run
happens to stop on. That is #3's retirement showing up exactly where it should: a modelled PCF50605
spends different simulated time in the I²C poll than a constant `0xff` does, so the same 600 M
instructions land a hair further along. Nothing that decides the boot moves.

**What this did not test.** Both recipes were re-run at their own budgets, not to quiescence, and
`flsh.sh`'s arms never reached the registers at all. The claim established is "removing these flags
changes nothing observable in these recipes", not "these registers are correctly modelled" — that
claim belongs to §Retiring #1 and §Retiring #2 and rests on the retail path, which exercises both
registers on every boot (`xmb: … 4 opens, 4 closes … 2 kicks`).

### Build provenance, because it nearly confounded this

Every arm above was run twice: once on a binary built from the working tree, and once on a binary
built from a **clean worktree at `ae05101`** — because a second agent was editing `lib.rs` in the
same tree, and a `cargo test` mid-session silently rebuilt the binary underneath the measurements.
Both builds agree on every number in this section, and the clean build reproduces the retail baseline
exactly (256 ATA commands · `pp dma: 4 transfers, 201216 bytes` · 4 unmapped reads · `xmb` 4/4/2 ·
`ide irq: raised 907, delivered 357, acked 368`). `cargo test --release` at `ae05101` is **15 passed,
0 failed**.

That is not bookkeeping fussiness. This project's own rule — *a fixed instrument's first job is to
re-run the conclusions the old one produced, and it needs the old binary kept* — is about exactly
this hazard, and the hazard here was not a deliberate instrument change but a **shared working tree**.
An A/B is only sound if both arms saw the same emulator; the way to guarantee that is to name the
commit the binary came from, not the directory.

The same shared tree bit a second time, and the second bite is worth a rule of its own: everything
this section describes — the ledger rewrite, both recipe edits — was `git add`ed and then swept into
**`3661b0f`**, a *different* agent's commit, message and all, because `git add` stages into one
shared index. The content is intact and the table survived (both agents edited the same two rows
without collision), but the history now files a four-defect ledger repair under
`ledger: #6 and #7 carry the audit's corrections`. **Two agents in one working tree share the index
and the branch, not just the files.** Stage and commit in one step, or work in a worktree.

## snapshot / restore — ✅ faithful; the control that condemned it was wrong

`--snapshot=N:FILE` and `--restore=FILE`. Saved: CPU including banked registers and SPSRs, every
memory region, aliases, read overrides, the microsecond clock, interrupt-pending, timer deadlines,
ATA scalars (taskfile, buffer, `cfg`, IRQ latch) and the BCM (sparse internal memory plus address
latches and phases). Deliberately **not** saved: profile, call log, unmapped map, region counters and
device logs — a restored run measures itself rather than inheriting half a measurement.

**It was briefly recorded here as untrusted. That was wrong, and the error is worth keeping.** The
first control compared *the last 20 `BL`s* between a restored run and an uninterrupted one, and they
differed — so the tool was condemned. In fact both runs were sitting in the same tight poll; the
restored run had made **no calls at all** in its window, while the uninterrupted run's last-20 came
from before it entered the loop. The comparison could not have passed even for a perfect snapshot.

The correct control compares **end state**:

| | end PC | usec | ata commands |
|---|---|---|---|
| restored (400M snapshot + 200M) | `0x40000a30` loop | 120 000 000 | 0 |
| uninterrupted 600M | `0x40000a30` loop | 119 999 992 | 10 |

Identical loop, identical simulated time. The `ata commands` difference *is the documented
behaviour* — that log is a measurement and is intentionally not restored.

**The lesson is not "the tool was fine".** It is that a control has to be capable of passing. A
comparison that a correct implementation would also fail is not evidence, and this one nearly cost a
working instrument. Restore + 200M runs in 17s against 70s for the uninterrupted 600M.

## Fidelity knobs — not correctness bypasses, but they change behaviour

| Knob | Effect | Note |
|---|---|---|
| `--clock=N` | instructions per simulated microsecond; **75 is real time and is the default since 2026-08-17** | lowering it makes time outrun the code, collapsing timeout waits. **Timing-sensitive code can notice** — and it did. The default was **5** from research/03 (§the clock knob) until now: a deliberate accelerant, because the bootloader polls with timeouts and a low clock skips its delay loops, reaching 5 ATA commands in a 600 M budget where real time reached 2. It was never turned back, so every measurement in this project was taken on a machine whose own sense of time ran **15x fast** — including every DRM measurement. The operator found it from the other end, by playing: *"when i play brick the balls just shoot immediately super fast, nearly unplayable"*. That is this row, observed. **It is not the DRM cause** — a 6 G A/B at 5 and at 75 lands on the same 933 ATA commands, the same 75 267 non-black pixels, 40 331 code buckets against 40 322, and the flag at `0x14937190` zero in both. **And real time is not slower where it matters**: at the 1.7 G snapshot budget it reaches *further* — 632 ATA commands and 31 473 buckets against 305 and 25 506 — because fewer timer interrupts per instruction means less of the budget spent in ISRs. `--wheel-click-instr` moved with it, 20 000 -> 300 000, because what the firmware's wheel poll sees is the *simulated* interval and that had to stay at 4 ms; `--selftest` is byte-identical across the change, 43 frames posted and the same 40/38/33 arrivals in RetailOS's decoder, scroll and wheel handlers |
| `--map`, `--poke` | ad-hoc regions and words | experiment scaffolding; nothing in the current recipe uses them |

## HLE-track stubs (the independent B3/B4 path)

`--stub=Audio:52=1` and the framework stubs in `trace.rs` reimplement RetailOS functions rather than
running them. **B7 retires this entire category** — a booted RetailOS binds all 433 functions itself.

---

## What is *not* a bypass

Worth stating, so the list above is not read as "everything is fake":

- **The ARM core** — fuzz-verified against a reference.
- **The memory model** — SDRAM sizing, aliasing and the remapped views are modelled, not faked.
- **Interrupts and timers** — the firmware programs its own ~1 kHz tick and we deliver it.
- **ATA** — `IDENTIFY`, `SET FEATURES`, `READ SECTORS` with LBA28, backed by a real image. The ROM
  reads a megabyte through it by PIO.
- **ATA bus-master DMA** — the engine at `IDE_BASE+0x400/0x408/0x40c`, modelled from Apple's own
  bootloader programming it (see [research/03](03-rtxc-and-the-video-coprocessor.md) §26). `0xc8`
  stages sectors without asserting DRQ; the GO bit commits them to memory. **This is what loads
  `osos`** — 7.5 MB in 128 KB transfers.
- **The MMAP unit** — all 8 windows, decoded from Rockbox's `crt0-pp.S`: base plus a compare-mask in
  LOGICAL, permission flags in PHYSICAL, address bits 31:30 always compared. Uncompared mask bits
  above a window's size are don't-cares, so one window can answer for several disjoint ranges — and
  it does. Translation runs two levels because the hardware has two: the MMAP unit, then the memory
  controller's mirrors.
- **The BCM *transport*** — address latching, data window and the command encoding are real; only the
  co-processor's *replies* are synthesised (#6).
- **The NOR flash** (`--nor`) — a JEDEC part, not a byte array: autoselect ID, sector erase, word
  program that can only *clear* bits, reset from any state, and a CFI query table nothing reads.
  Answers as SST `0xbf`/`0x273f`, which is row 3 of the ROM's own eight-row device table. Both
  address windows are one chip, so an erase through the reset alias at 0 is visible at
  `0x20000000`. Verified by making it do real work: perturb 64 bytes of a scratch ROM copy and the
  updater erases 248 sectors, programs 507 904 words, and leaves the image byte-identical to the
  pristine dump.
- **Rockbox booting** — a test, not a dependency, and permanent.
- **The identity arms of the DRM research *(not published)* §5 and §5b** — three NOR images whose `SysCfg`
  identity fields (`SrNm` at `0x401c`, the `FwId` GUID at `0x4034`) are edited to say the machine is
  a different iPod. They are not bypasses because **nothing is patched out and no comparison is
  faked**: the firmware runs its own check against a different input, which is what a different
  device would supply. **No recipe passes them.** They exist only as `FLASH=` on named experiment
  runs, `tools/ipod-boot/retail-boot.sh` still defaults to the unmodified dump, and the control that
  gives them meaning (arm D — one bit changed, and the boot returns to the stock run instruction for
  instruction) is only readable *because* the default is untouched. If one of them ever ends up in a
  recipe it becomes a bypass that day, and this bullet moves into the table above.

## Order of retirement

Roughly by what each unblocks, not by how wrong it is:

0. **#11 — RETIRED.** The MMAP unit is modelled from `ipodloader2`'s encoding; logical 0 maps to
   SDRAM when the firmware programs it, and RetailOS executes. Originally: Retired 2026-08-13; it was the checksum bug. The image now loads
   into real SDRAM and sums to `0x2c7c86cf` against an expected `0x2c7c48f3` — the whole remaining
   gap is **one 512-byte sector**. See [research/03](03-rtxc-and-the-video-coprocessor.md) §27.
0a. **#15 — RETIRED.** The window encoding is Rockbox's documented one, not a hardcoded size. With
   it, RetailOS makes **zero** unmapped accesses through the boot, runs to budget, and drives the
   BCM. See [research/03](03-rtxc-and-the-video-coprocessor.md) §33.
0b. **#14 — RETIRED.** `--boot-osos` never required `--osos=`; it required an image *at the entry*,
   and only the warm path enters somewhere the ROM has not already filled. `cold-boot.sh` no longer
   passes it, and the ROM's own load is byte-identical to `OSOS_correct.bin` — all 7 559 680 bytes,
   compared at the handover. The cost of carrying it was not zero after all: the `osos-low` mirror
   it installed at address 0 was **writable**, where NOR is not, so it had been silently absorbing
   the bootloader's JEDEC flash-unlock writes (`0xaaaa` / `0x5554` / `0x0`, 40 of them, from
   `0x40009f9c`..`0x40009fd8`). Those are now reported unmapped — which is #12 asking to be modelled.

0c. **#5 — RETIRED.** The warm constant is `0x000b0011`, the `HwVr` the cold path reads out of the
   NOR dump, and the warm path was re-validated against it: identical selector traffic, identical
   ATA command count, no new unmapped accesses, four more code buckets. Its only visible effect is
   RetailOS asking for 4 sectors of the MBR instead of 1.
1. **THE BOOT LOOP IS A MISSING FONT, AND THE FONT IS IN `rsrc`.** RetailOS asks its font registry
   for *Podium Sans* at **18 pt**; the registry holds Podium Sans at 14, 16, 22 and 28 only. The
   lookup at `0x00221e24` returns a **designed** null delegate, that null is bound into 400 objects,
   and the first call through one lands on `bx 0` — the reset vector. `PodiumSans18.ttf` is a real
   126 524-byte file in `/Resources/Fonts/` inside the **`rsrc`** image, listed in the same firmware
   directory the ROM used to load `osos`, and **nothing has ever read it**. RetailOS's last disk
   activity is a single MBR read, 360 M instructions before the first font lookup. The open question
   is now *why it stops after the MBR*. See [research/10](10-the-resource-image.md).

1y. *(superseded)* **THE BOOT LOOP IS REAL, AND IT IS A NULL DELEGATE.** RetailOS gets ~99 simulated seconds in,
   calls a member function through a two-word delegate at object `+0x20` (`0x13e26f8c`) that
   **nobody ever writes**, loads a zero vtable slot, and `bx 0` lands on its own reset vector —
   ~500 resets per 2 G instructions at `--clock=5`. Not a bypass artefact: `--rdval` on the GPIO
   next to it is ruled out (§48), the PMU is ruled out (§38), the BCM is ruled out (§39/§41), and
   the IDENTIFY model is exonerated by A/B (§47). The target is the **missing write**: the
   constructor at `0x00211a70` writes `+0x1c` and `+0x34..+0x40` and skips `+0x20`. See §46–48.

1z. *(superseded)* **THE BOOT LOOP DOES NOT EXIST.** It was never a bypass artefact and never an unconstructed
   object: the **osos body on disk was missing its first sector**, so RetailOS ran with no ARM
   exception vector table at address 0 and every pointer 512 bytes off. The disk was repaired at
   13 Aug 02:05 for an unrelated reason and the symptom was never re-measured. Re-measured now:
   **one** arrival at `0x00000000` — the cold reset — under `--pmu` *and* under the old
   `--i2c-fill=0xff`, and equally under the binary built from `078615d`, the commit that first
   reported the loop. RetailOS boots and reaches its RTXC **idle task** at 119.7 M instructions.
   See [research/03](03-rtxc-and-the-video-coprocessor.md) §40. **#6 remains the right next device**
   for the display, on its own merits — never as a suspect.

   Previously believed (all three of these were explaining a corrupted input):
   the boot loop is not a bypass artefact, both suspects exhausted (#3 by measurement, #6 by
   attribution §39); before that, **#3 is retired** and its retirement eliminated the PMU because a
   fully modelled chip still boot-looped 253 times against the bypass's 255.

   Originally, on both #3 and #6: RetailOS boot-loops 255 times over 1.2G instructions:
   a virtual call through an uninitialised object reads its vtable out of the exception vectors,
   the slot is unmapped, and `bx 0` lands on the reset vector (see
   [research/03](03-rtxc-and-the-video-coprocessor.md) §35). Something constructed nothing. These
   two are the bypasses that let an init path "succeed" against hardware that never answered —
   all-ones for every I²C read, and synthesised BCM replies. **Neither can be A/B tested**: removing
   either one strands the run inside Apple's *bootloader*, before RetailOS is reached at all
   (§36). They can only be replaced by real models, never toggled off to see what changes.
0c. **Where the boot actually is — re-measured 2026-08-14.** One command:
   `BUDGET=600000000 ./tools/ipod-boot/retail-boot.sh --clock=5 --stop-when-idle=400000000`.

   > *(retracted in place — both items below described the warm/prototype era and were an era out of
   > date. Kept because what they got wrong is instructive: **every one of their claims was an
   > absence**, and every one of them dissolved when the configuration and the stop window moved.)*
   >
   > 0c. **The boot's wall is no longer reach.** 12 G instructions at `--clock=5` is 2 400 simulated
   >    seconds and executes exactly the same 6 028 profile buckets as the 4 G run. RetailOS reads LBA 0
   >    once, never touches the disk again, and masks the IDE interrupt. The two sharpest open questions
   >    are **why it stops after the partition table** and **what starts the application layer** —
   >    `VCUpdateTask`, `DiskReaderTask`, `eAppMotor` and `eAppAsyncIO` all exist and none of them are
   >    ever created. See [research/03](03-rtxc-and-the-video-coprocessor.md) §45.
   > 1a. **The uninitialised vtable at `0x00183e7c`** is now the sharpest open defect — a virtual call
   >    through an object whose vtable pointer is `0xea00001c`, an ARM branch word rather than an
   >    address, firing 2 080 times over 2 G instructions and not crashing. Not a bypass, and not
   >    §35's crash (that was the sector shift). See [research/03](03-rtxc-and-the-video-coprocessor.md) §44.

   What that command reports now:

   - **256 ATA commands**, not one. The disk is read past the partition table and kept: `lba 63` and
     `lba 96` (the firmware directory), 57 × 128 KB of `osos` by DMA, then **`lba 14864`** — the
     `rsrc` volume's FAT boot sector — **`lba 14870`** (the FAT) and **`lba 14866`**. `rsrc` is
     mounted. `ata dma: 303 transfers, 15 691 264 bytes`.
   - **The IDE interrupt is not masked**: `ide irq: raised 907 times, DELIVERED to a handler 357
     times, acked by status read 368 times; enabled=1 pending=0`.
   - **`vmcs.bin` is delivered**: `pp dma: 4 transfers, 201216 bytes` into the co-processor at
     `0x30000000`, all four descriptors retired by RetailOS's own ISR.
   - **`unmapped: 4 reads, 0 writes across 1 pages`** — all four at `0xea000078`, from `0x000a0bd0`.
     That is the same *shape* of defect 1a named (an ARM branch word read as a pointer) and it is
     what is left of it: `0x00183e7c` itself, watched with `--enterlog` on this run, is reached
     **zero times**. It fired 15 452 times on the 12 G prototype run; it does not execute at all on
     the retail path.
   - **The startup sequence is four phases in, not one and not five.** `--enterlog` on the five
     dispatch sites of `0x001d28e0` and their five return points: phases 1, 2 and 3 are dispatched
     (`0x001d28f8`, `0x001d290c`, `0x001d2920`) and each returns; **phase 4 is dispatched at
     `0x001d2938` and never returns**, so `0x001d293c` is never reached and phase 5 is never
     dispatched at all. *(Research/20 Addendum 11 §4's "5 of 5 entered, 4 returned" cannot be right
     as written — its own §5 has phase 4 not returning, and phase 5 is dispatched by the instruction
     after phase 4 returns. At this budget the measurement is 4 dispatched, 3 returned.)*

1a. **The sharpest open defect is the block on RTXC semaphore `0xd1`.** `MP3ExampleTask`
   (priority 52) sits in `KS_pend` on `0xd1`, six frames deep in a recursive view-tree builder,
   inside startup phase 4 — and `APPLEBOOT` is pended on `0xea` waiting for that same task. The
   mechanism is not broken: `--enterlog` on `0x00189444`, the matching post, records **377 arrivals**
   in a 600 M run, all from `lr=0x0018955c`. What has not happened is one more completion. Widening
   2 G → 6 G moves nothing. See [research/10](10-the-resource-image.md) Addendum 11 §5.

   The discriminator that makes this a *genuine* block, and not the artefact 0c above was:
   `state = 0x40` with resume PC `0x000a694c`, and a trailing window carrying **871 501 CPU sleeps**.
   A busy machine reports **0**. Never conclude "blocked" without that number.
1b. **`--clock` has become load-bearing for reach, and that is a problem.** RetailOS's boot is
   paced by simulated time, and this interpreter is ~~~1000×~~ **~3.4×** slower than the hardware
   (21.0 M instr/sec measured 2026-08-13 against ~72 MIPS; the 1000× figure was wrong by 300× and
   sat here unchallenged — see the hardware bill of materials *(not published)* §9), so every
   milestone past ~5 simulated seconds currently costs either an enormous budget or `--clock=5`.
   The knob is listed under "fidelity knobs" below with the warning that timing-sensitive code can
   notice it; ~~every result obtained with it (the partition-table read, the extra tasks, the
   repeated drive init) is a **lead until confirmed at `--clock=75`**~~. See §42.

   > **CONFIRMED at `--clock=75` — real time — on 2026-08-14. The caveat is retired.** It had stood
   > over every measurement past ~5 simulated seconds for days and had never once been run, because
   > it costs ~25 G instructions (~20 min) to reach the same place.
   >
   > `retail-boot.sh --clock=75`, budget 25 G, against the same three milestone addresses:
   >
   > | | `--clock=5` | `--clock=75` |
   > |---|---|---|
   > | `0x0016b044` (`MP3ExampleTask` body) | @52 249 735 | @120 826 059 |
   > | `0x0019db14` (view-tree builder) | @218 810 613 | @687 340 287 |
   > | `0x00284538` (`APPLEBOOT` end of body) | @419 532 867 | @834 677 799 |
   > | ata commands | 770 | **770** |
   > | ide irq raised | 1607 | **1607** |
   > | bcm frame updates | 2 | **2** |
   > | cpu sleep halts | 2 685 679 | 138 406 920 |
   >
   > **The machine does identical work; only the waiting changes.** Instruction offsets stretch ~2×,
   > not 15×, because most of the boot is compute-bound — only the parts that wait scale with the
   > clock, and the halt count rising 51× is exactly where that time goes. Every count that describes
   > what the firmware *did* is unchanged to the unit.
   >
   > Two consequences. Every result in `research/` obtained at `--clock=5` firms up from lead to
   > measurement. And — the reason this was run now — **the 1.61 G idle is not a pacing artefact**:
   > RetailOS reaches the same places at real time and still draws nothing. The gate is real.
0d. **#12 — RETIRED on the retail ROM.** The bypass was hand-applying the updater's own last write.
   Given Apple's directory with `aupd` in it, the retail ROM runs the updater, the updater
   completes, and it marks the entry done itself; the next boot runs `osos`. What was missing was a
   NOR that answers a JEDEC identify (`--nor`) and a disk it is allowed to write (`--disk-writable`).
   The erase/program half is exercised by a deliberately-perturbed flash: 248 sector erases,
   507 904 words programmed, and the result byte-identical to the pristine ROM. See §below and
   [research/07](07-the-flash-images.md). **Re-run 2026-08-14 with #1, #2 and #3 removed from the
   recipe** — every one of those numbers is unchanged, and the two arms' repaired flash images are
   `cmp`-identical. The proof did not depend on the retired bypasses it was carrying.
2. **#13** — retires when the ROM's halt is understood. **#12 is CLOSED as out of scope, 2026-08-17.**
   It was open on one thing only: the prototype ROM, which reads its firmware partition at 4× the
   MBR LBA, reads the image in full and then powers off without printing. We never got that ROM to
   boot, and this project does not target it — the machine being emulated is a retail 5.5G, where
   #12 has been retired since 2026-08-13 with the updater's own erase/program cycle exercised and
   the repaired flash byte-identical to the pristine ROM. A row kept open against hardware nobody
   intends to support is not an outstanding bypass, it is a wish; the prototype's behaviour is
   recorded in [research/07](07-the-flash-images.md) for whoever wants it.
3. — *(#6 promoted to 1; executing `vmcs` is the difference between "a frame appeared" and "the
   display works")*
4. **#9 — RETIRED 2026-08-17.** *(**#10 is retired**, so this line no longer names a pair.)* It was
   live-and-unswitchable in every recipe, which raised its priority rather than lowering it:
   nothing in a recipe revealed it. It is switchable now (`--no-ide-irq-latch`), and both halves of
   a retirement are in hand — the firmware names the bit (the ISR reads `0x20400028` and writes
   `0x20400020`), and taking the bit away costs the boot its disk.
5. **#1, #2 — RETIRED.** Both were the external memory bus controller, and both were identified the
   way this row predicted: by reading the ROM around the poll site. The surprise was that #1 has
   nothing to do with SDRAM — its poll brackets NOR flash commands — and that #2's recorded shape
   (bit 30, set-then-clear) was wrong in both the bit and the direction. **#5, #11** remain.
6. **#7** — the second core; only matters when something needs the COP. **It is now measurable
   without being emulated**: `--cop-awake` stops installing the `COP_STATUS` override, so the arm
   where the model does not lie about the coprocessor exists for the first time. On the DRM path
   the A/B is identical to the instruction — 25 506 code buckets in both arms, the same call at
   55 050 392 and the same failing return at 1 628 342 943 — and `wake_cop` at `0x000cfb20` does
   not execute until 1 700 032 431, *after* the failure. Whatever the second core is for, it is not
   what refuses the keys.
7. **#8 — RETIRED as a whole-word override 2026-08-17.** The claim (bit 31 says locked) was always
   defensible; the mechanism asserted 32 bits to make a claim about one. It is an OR-mask now, the
   boot is identical at 1.7 G, and the switch-over's own failure is written into the row: an
   override table that `page_is_plain` does not know about is never consulted.

## Independent corroboration from `daniel5151/clicky` (2026-08-13)

An iPod **4G / PP5020** emulator, examined as a documentation source only — **it carries no licence
at all**, so all rights are reserved and no code may be reused. That costs us nothing: every
register fact worth having traces back to Rockbox's `pp5020.h`, which we already hold.

It is not ahead of us on the boot path — its primary path is an **HLE bootloader** that memcpys
`osos` into SDRAM and sets PC, its CFI model implements *software-ID only* with no erase or program,
so it cannot run `aupd` at all. But it reached two of our findings independently, by a different
method, on a different SoC revision:

| our finding | how we got it | clicky | how they got it |
|---|---|---|---|
| `0x70000030` **bit 27** must read set | disassembly of the 11-instruction spin at `0x4000137c` | same bit, forced set | empirically — Apple's 4G Flash ROM hung otherwise |
| `0x7000003c` **bit 31** is completion | disassembly of `0x40003590` | same bit, forced set | same |

Two projects, two methods, two SoC revisions, the same two bits — and, usefully, **those were the
only two undocumented gates either project needed** to get through Apple's ROM. That is a negative
result worth having: the ROM depends on very few unmapped bits.

Where we are strictly ahead on the same registers: they model **no NOR writes**, so `0x70000030`
bit 30 as the **write gate** is uniquely ours and uncontradicted; they blanket-set `0x7000003c`
bit 31 on every write and never identify **bit 24 as the command**, so our handshake is finer than
theirs. They also misname `0x70000030` as "(?) Dev Timing 0".

Neither project can answer **"is bit 27 ever *not* ready?"** — both force it permanently set. That
remains open and needs a datasheet or a logic capture, as NEXT.md item 5 says.

### The flash part: their choice was better evidenced than ours, and we have switched

The ROM's accept-table holds two SST rows with identical uniform 4 KiB geometry. We drove row 4,
`0xbf`/`0x2781`, named for `SST39VF800A`. clicky independently drives row 3, `0xbf`/`0x273f`,
labelled `SST39WF800A`.

**Our own [research/05](05-the-chip-inventory.md) §NOR already said `WF` wins** — iPodLinux and the
EE Times 5.5G BOM both name `39WF800A`, and the Rockbox wiki's `VF` spelling cites iPodLinux as its
source, making it a downstream typo. Our two research files disagreed with each other and did not
cross-reference; a third project agreeing with the one we were not following is what surfaced it.

A/B'd over a full `flash-update.sh` run before switching: identical console output, identical ATA
command count, identical flash behaviour. Not bit-identical — after ~600 M instructions the resting
state differs by six interrupts out of 995 200 — but nothing that decides the boot changes. Now on
`0x273f`. **This still does not establish what the hardware carried**; only a board photograph does.

---

## #17 — what the VideoCore ablation bought, and why it is now deleted rather than merely off

> **Deleted 2026-08-19.** Everything below is the record of what this ablation was for and what it
> found; the flags themselves no longer exist. `--force-vc-upload`, `--force-vc-retire`, the
> `force_vc_retire` field, the `force_retire_log` and the hot-loop block that zeroed the four
> in-use bytes at `channel+0x18` are all gone. The reason is in the retired-entries table: the
> transfer engine at `0x60009000` has been modelled since 2026-08-13 and its ring drains without
> the emulator touching it, so both halves had been reproducing — badly — a run the machine can
> already do. `--force-sem=ID[,ID…]` remains as the general form.


Added 2026-08-13. Two flags, both off by default, neither in any recipe:

- **`--force-vc-upload`** — `KS_pend` at `0x000a6924` returns `0` instead of blocking, for semaphore
  `0xe0` only. Keyed on the pend because the wait is reached by a *tail* `B` from the counting
  acquire at `0x000a0ebc`: there is no call frame to patch, and the transfer object's address is
  heap-dependent while the instruction is not. Its control is the arrival count — the unablated run
  pends on `0xe0` exactly once, and the ablated run eats exactly one, at the same instruction.
- **`--force-vc-retire`** — zeroes the four in-use bytes at `channel+0x18` on the spin edge
  (`0x00159bc8` with `r2 != 0`). Keyed on the retry rather than the loop head so that a genuinely
  free ring is untouched and the fast path stays bit-identical.

**Stage 1 alone makes the machine worse, which is itself the finding.** `APPLEBOOT` stops blocking
and starts *spinning* — 71 % of the steady state in a 48-byte window — because the semaphore is only
one of two completion signals; the other is a drain barrier in SDRAM that no event post can clear.
Spinning at priority 15 starves the device layer: 45 tasks become 44, 96 ATA commands become 90.

**Stage 2 gets through.** `APPLEBOOT` reaches `0x00284518` and `0x0028452c` for the first time, 45
tasks become **62** (`MP3ExampleTask`, `TcMemManagerThread`, `iMAImageCacheThread`,
`CIapIncomingProcessThread`, `StreamCacheReadTask` among them), ATA commands go 96 → 256 and DMA
8.1 MB → 15.7 MB. Then it blocks on **`0xea`**, waiting for the co-processor to answer a request.

**The verdict this bypass was built to produce:** RetailOS does not merely need the transfer
acknowledged. It needs the co-processor to answer. See [research/10](10-the-resource-image.md)
Addendum 8 §7 — including what is still *not* established, which is how much of the VideoCore has to
be real.

### It also retired a wrong measurement

Building it exposed an instrument bug that had produced a confident false negative: `--watch-range`
could not see word-sized writes into a mapped region, because `read32`/`write32` hoist the `count()`
call behind `accounting || page_log || !read_addrs.is_empty()` and `watch_range` was not in that
list. Research/20 Addendum 7 §5's "the engine at `0x60009000` is never programmed" was that bug. It
**is** programmed. Fixed in `lib.rs`; control is 0 writes before and 208 after with the flag alone,
and a byte-identical baseline run. Addendum 8 §8 has the detail.

**And that engine was not the only casualty.** The audit that fix owed — every absence-shaped claim
in the project re-run against the fixed instrument — found `--input-regs` silenced by the same hoist
on the *read* path too, and retracted two further published conclusions in
[research/09](09-what-the-hardware-must-supply.md): a heap record's delegate field reported as
written by "nobody", and a sibling record's non-zero field written off as uninitialised heap. Both
were fully written. Nothing in this ledger changes — no bypass was justified by either — but the
count of wrong conclusions traceable to this one hoist is now **three**. Addendum 8b.
