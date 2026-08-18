# The disk images — what they are, and how they are protected

**`resources/drives/ipod8g-retail.img` is irreplaceable.** It is a real 5.5G's 8 GB drive with
a working RetailOS install: 56 titles in `Games_RO` (counted from the FAT32 directory entries
2026-08-14; this line said 196, which had no source), the matching `GameData_RW` / `GameStats_WO`
trees, `iPod_Control`, an `iTunesDB`, and the SC Info keystore. It cannot be regenerated — restoring
a fresh one would produce a different disk, and the games came from a specific purchase history.

| | |
|---|---|
| `ipod8g-retail.img` | the working image. **SHA-256 `b217f513621bf3d2398c96139e6d7cbf3d7b6e9da953c23a59dc852fffd47b07`**, 8 589 934 592 bytes, mtime 2026-08-13 16:48 |
| `ipod8g-retail.PRISTINE.img` | APFS copy-on-write clone of the above, **`chmod 444`**. Costs no disk until one of them diverges |
| `ipod8g.img` · `ipod8g-rockbox.img` · `ipod2.img` | other targets; not this one |

Verify before trusting a measurement, and after anything that could have written:

```sh
shasum -a 256 resources/drives/ipod8g-retail.img
```

## How the recipes protect it

`retail-boot.sh` **never writes the source**. It makes a `cp -c` clone per run — an APFS
copy-on-write clone, ~3 ms for 8 GB — passes `--disk-writable` against the clone, and deletes it on
exit via a `trap`. RetailOS really does write during boot (FSInfo, both FATs, `Contacts`,
`Calendars`, `Notes`, `Accessories`, two vCards, and it deletes `IC-Info.sid`), so the clone is not
a precaution against a hypothetical — it is a precaution against something that happens every run.

`WORKDISK=path` keeps a clone across runs, for when accumulated state is the point. **Never point
`WORKDISK` at the source image.** `flash-update.sh` uses the same clone-into-`$WORK` pattern.

## The trap this is guarding against

The measurement discipline in this project is built on the source image being byte-identical across
sessions — `--verify-memory`, the byte-identical `osos` load, the 41-changed-sector disk diff, the
Rockbox oracle's run log. **All of those compare against a disk assumed unchanged.** A single run
that wrote the source would not announce itself; it would quietly move every future baseline, and
the failure would look like the emulator having changed rather than the input.

That is the same shape as the nine instrument failures recorded in the README, applied to data
instead of to a counter. The clone plus the recorded hash makes it detectable.

**Advancing past RetailOS's setup screens writes to the disk** — language selection and first-boot
setup both persist. Do that on a clone, and keep the pristine hash to prove the source is untouched.
