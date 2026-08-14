#!/usr/bin/env python3
"""Extract the `rsrc` volume from an iPod firmware partition, and files out of it.

Nothing in this project could read `rsrc` before: the emulator mounts it the way RetailOS does, but
there was no way to get a file out onto the host and look at it. That is how `vmcs.bin` went
un-analysed for the whole project while sitting at the centre of the last open bypass.

    ./rsrc-extract.py DISK.img --list
    ./rsrc-extract.py DISK.img --get RESOUR~1/VIDEOC~1/BOOT/VMCS.BIN -o vmcs.bin
    ./rsrc-extract.py DISK.img --volume rsrc.img        # the whole 5 MB volume

Deliberately pure parsing — no `hdiutil`, no `diskutil`, no mounting. This project's disk images are
irreplaceable reference material and a partitioning command aimed at the wrong device is the one
mistake here with no undo.

Layout, all measured rather than assumed (see research/17):
  * MBR partition 0, type 0x00, starts at LBA 63.
  * The `!ATA` directory sits at partition + 0x4200, 40-byte entries.
  * Fields are `!ATA` magic, then a 4-char tag stored as a u32 (so `rsrc` reads as `crsr` in a
    byte dump — it is a little-endian constant, not a string), then at +0x0c the device offset
    relative to the partition, and at +0x10 the length.
  * `rsrc` itself carries a 0x200 header before its FAT12 boot sector.
"""
import argparse, struct, sys


def read_directory(disk, part_lba=63):
    """Yield (tag, offset, length) for every image in the firmware partition."""
    part = part_lba * 512
    with open(disk, "rb") as f:
        f.seek(part + 0x4200)
        d = f.read(0x200)
    if d[:4] != b"!ATA":
        sys.exit(f"{disk}: no !ATA directory at partition+0x4200 — wrong image or wrong partition")
    for off in range(0, len(d) - 40, 40):
        e = d[off : off + 40]
        if e[:4] != b"!ATA":
            break
        tag = e[4:8][::-1].decode("latin1")
        devoff, length = struct.unpack("<II", e[0x0C:0x14])
        yield tag, part + devoff, length


class Fat:
    """Just enough FAT12 to walk a read-only volume. No write path, deliberately."""

    def __init__(self, data):
        self.d = data
        bps = struct.unpack("<H", data[11:13])[0]
        self.spc = data[13]
        rsvd = struct.unpack("<H", data[14:16])[0]
        nfat = data[16]
        self.nroot = struct.unpack("<H", data[17:19])[0]
        spf = struct.unpack("<H", data[22:24])[0]
        self.bps = bps
        self.fat = rsvd * bps
        self.root = self.fat + nfat * spf * bps
        self.data = self.root + self.nroot * 32

    def next_cluster(self, n):
        o = self.fat + (n * 3) // 2
        v = struct.unpack("<H", self.d[o : o + 2])[0]
        return (v >> 4) if (n & 1) else (v & 0xFFF)

    def read_chain(self, clus, size):
        out = bytearray()
        c = clus
        # The size bound is load-bearing as well as the FAT terminator: a cross-linked or truncated
        # chain would otherwise walk until it hit a value that happened to look like end-of-chain.
        while 2 <= c < 0xFF0 and len(out) < size:
            o = self.data + (c - 2) * self.spc * self.bps
            out += self.d[o : o + self.spc * self.bps]
            c = self.next_cluster(c)
        return bytes(out[:size])

    def walk(self, off=None, nents=None, path=""):
        off = self.root if off is None else off
        nents = self.nroot if nents is None else nents
        for i in range(nents):
            e = self.d[off + i * 32 : off + i * 32 + 32]
            if e[0] in (0, 0xE5) or e[11] & 0x0F == 0x0F:
                continue
            base, ext = e[0:8].decode("latin1").rstrip(), e[8:11].decode("latin1").rstrip()
            name = base + ("." + ext if ext else "")
            if name in (".", ".."):
                continue
            clus, size = struct.unpack("<H", e[26:28])[0], struct.unpack("<I", e[28:32])[0]
            full = path + name
            if e[11] & 0x10:
                yield full + "/", clus, 0, True
                o = self.data + (clus - 2) * self.spc * self.bps
                yield from self.walk(o, self.spc * self.bps // 32, full + "/")
            else:
                yield full, clus, size, False


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("disk")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--get", metavar="PATH")
    ap.add_argument("--volume", metavar="OUT")
    ap.add_argument("-o", "--out")
    a = ap.parse_args()

    entry = next((e for e in read_directory(a.disk) if e[0] == "rsrc"), None)
    if not entry:
        sys.exit("no `rsrc` image in the firmware directory")
    _, off, length = entry
    with open(a.disk, "rb") as f:
        f.seek(off)
        vol = f.read(length)

    if a.volume:
        open(a.volume, "wb").write(vol)
        print(f"{a.volume}: {len(vol)} bytes")
        return

    fat = Fat(vol[0x200:])  # past the image header, at the FAT boot sector
    if a.list or not a.get:
        for name, clus, size, isdir in fat.walk():
            print(f"  {name:<44} {'' if isdir else size}")
        return

    want = a.get.upper().lstrip("/")
    for name, clus, size, isdir in fat.walk():
        if not isdir and name.upper() == want:
            data = fat.read_chain(clus, size)
            out = a.out or name.rsplit("/", 1)[-1]
            open(out, "wb").write(data)
            print(f"{out}: {len(data)} bytes")
            return
    sys.exit(f"{a.get}: not found — run --list for the tree")


if __name__ == "__main__":
    main()
