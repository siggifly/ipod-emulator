#!/usr/bin/env python3
"""Read the iPod's FAT32 data partition out of a disk image, without mounting anything.

`diskutil`, `hdiutil attach` and every partitioning command are forbidden in this project
(`safety-and-working-model.md`) — they operate on real disks and a typo reaches one. This walks the
MBR and the FAT32 structures itself, opens the image **read-binary only**, and never writes.

Point it at the PRISTINE clone by default, which is `chmod 444`, so even a bug cannot damage the
working image.

    tools/fat-read.py tree
    tools/fat-read.py find sc\\ info
    tools/fat-read.py cat /iPod_Control/Device/SysInfo out.txt
    tools/fat-read.py catall Manifest.plist _out/manifests
    tools/fat-read.py lba 6362904 6357016

`lba` is the one that pairs with the emulator: `trace`'s `ata dma:` log prints absolute LBAs, and
this turns them back into paths, which is how "what did RetailOS actually read" becomes answerable.

Set IMG=<path> to read a different image. Long filenames (VFAT LFN) are reconstructed; without them
the 196-vs-56 game count could not have been checked, because the game directories are 5-character
short names but everything around them is not.
"""
import struct, sys, os, bisect

IMG = os.environ.get(
    'IMG',
    os.path.join(os.path.dirname(os.path.abspath(__file__)), '..',
                 'resources/derived/disk/ipod8g-retail.PRISTINE.img'))


class Vol:
    """One FAT32 volume inside an MBR-partitioned image. Read-only, by construction."""

    def __init__(self, path):
        self.f = open(path, 'rb')          # 'rb' — never 'r+b'
        mbr = self.rd(0, 512)
        if mbr[510:512] != b'\x55\xaa':
            raise SystemExit('no MBR signature in %s' % path)
        self.parts = []
        for i in range(4):
            e = mbr[446 + 16 * i:446 + 16 * i + 16]
            typ, lba, n = e[4], *struct.unpack('<II', e[8:16])
            if typ and n:
                self.parts.append((typ, lba, n))
        # 0x0b/0x0c are FAT32; the iPod's firmware partition is type 0x00 and is not a filesystem.
        cand = [p for p in self.parts if p[0] in (0x0b, 0x0c)]
        if not cand:
            raise SystemExit('no FAT32 partition; table is %r' % (self.parts,))
        self.ptype, self.pstart, self.psize = cand[0]

        bs = self.rd(self.pstart * 512, 512)
        self.bps = struct.unpack('<H', bs[11:13])[0]
        self.spc = bs[13]
        self.rsvd = struct.unpack('<H', bs[14:16])[0]
        self.nfat = bs[16]
        rootent = struct.unpack('<H', bs[17:19])[0]
        self.spf = struct.unpack('<I', bs[36:40])[0] or struct.unpack('<H', bs[22:24])[0]
        self.rootclus = struct.unpack('<I', bs[44:48])[0]
        self.fat_start = self.pstart + self.rsvd
        self.data_start = (self.fat_start + self.nfat * self.spf
                           + (rootent * 32 + self.bps - 1) // self.bps)
        self.fat = self.rd(self.fat_start * 512, self.spf * 512)

    def rd(self, off, n):
        self.f.seek(off)
        return self.f.read(n)

    def clus_lba(self, c):
        return self.data_start + (c - 2) * self.spc

    def chain(self, c):
        """Cluster chain from `c`. Stops on a repeat, so a corrupt FAT loops finitely."""
        out, seen = [], set()
        while 2 <= c < 0x0ffffff8 and c not in seen:
            seen.add(c)
            out.append(c)
            c = struct.unpack('<I', self.fat[c * 4:c * 4 + 4])[0] & 0x0fffffff
        return out

    def readdir(self, clus):
        ents, lfn = [], []
        for c in self.chain(clus):
            data = self.rd(self.clus_lba(c) * 512, self.spc * 512)
            for o in range(0, len(data), 32):
                e = data[o:o + 32]
                if not e or e[0] == 0x00:
                    return ents
                if e[0] == 0xe5:            # deleted
                    lfn = []
                    continue
                if e[11] == 0x0f:           # long-filename fragment
                    lfn.append((e[0] & 0x3f,
                                (e[1:11] + e[14:26] + e[28:32]).decode('utf-16-le', 'ignore')))
                    continue
                short, ext = e[0:8].decode('latin1').rstrip(), e[8:11].decode('latin1').rstrip()
                name = short + ('.' + ext if ext else '')
                if lfn:
                    lfn.sort()
                    name = ''.join(p for _, p in lfn).split('\x00')[0]
                lfn = []
                if name in ('.', '..'):
                    continue
                first = (struct.unpack('<H', e[20:22])[0] << 16) | struct.unpack('<H', e[26:28])[0]
                ents.append((name, e[11], first, struct.unpack('<I', e[28:32])[0]))
        return ents

    def walk(self):
        """(path, first_cluster, size, is_dir) for every entry, depth-first."""
        stack = [('', self.rootclus)]
        while stack:
            pref, c = stack.pop()
            for name, attr, first, size in self.readdir(c):
                p = pref + '/' + name
                isdir = bool(attr & 0x10)
                yield (p, first, size, isdir)
                if isdir and first >= 2:
                    stack.append((p, first))

    def read_file(self, first, size):
        buf = b''
        for c in self.chain(first):
            buf += self.rd(self.clus_lba(c) * 512, self.spc * 512)
        return buf[:size]


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else 'info'
    v = Vol(IMG)
    print('# %s: FAT32 type %#x at LBA %d, %d sectors/cluster, data starts at LBA %d'
          % (os.path.basename(IMG), v.ptype, v.pstart, v.spc, v.data_start), file=sys.stderr)

    if cmd == 'tree':
        for p, first, size, isdir in v.walk():
            print('%s%s\tclus=%d\tsize=%d\tlba0=%d'
                  % (p, '/' if isdir else '', first, size,
                     v.clus_lba(first) if first >= 2 else -1))

    elif cmd == 'find':
        needle = sys.argv[2].lower()
        for p, first, size, isdir in v.walk():
            if needle in p.lower():
                lbas = [v.clus_lba(c) for c in v.chain(first)] if first >= 2 else []
                rng = ('%d..%d (%d clusters)' % (lbas[0], lbas[-1] + v.spc - 1, len(lbas))
                       if lbas else '-')
                print('%-72s %s size=%-10d lba %s' % (p, 'DIR' if isdir else '   ', size, rng))

    elif cmd == 'cat':
        want = sys.argv[2]
        out = sys.argv[3] if len(sys.argv) > 3 else None
        for p, first, size, isdir in v.walk():
            if p == want and not isdir:
                buf = v.read_file(first, size)
                if out:
                    open(out, 'wb').write(buf)
                    print('wrote %d bytes to %s' % (len(buf), out), file=sys.stderr)
                else:
                    sys.stdout.buffer.write(buf)
                return
        raise SystemExit('not found: %s' % want)

    elif cmd == 'catall':
        suffix, outd = sys.argv[2], sys.argv[3]
        os.makedirs(outd, exist_ok=True)
        n = 0
        for p, first, size, isdir in v.walk():
            if isdir or not p.endswith(suffix) or first < 2:
                continue
            open(os.path.join(outd, p.strip('/').replace('/', '__')), 'wb').write(
                v.read_file(first, size))
            n += 1
        print('extracted %d' % n, file=sys.stderr)

    elif cmd == 'lba':
        ivs = []
        for p, first, size, isdir in v.walk():
            if first < 2:
                continue
            for c in v.chain(first):
                s = v.clus_lba(c)
                ivs.append((s, s + v.spc, p))
        ivs.sort()
        starts = [x[0] for x in ivs]
        for a in sys.argv[2:]:
            n = int(a, 0)
            i = bisect.bisect_right(starts, n) - 1
            print(n, '->', ivs[i][2] if i >= 0 and ivs[i][0] <= n < ivs[i][1]
                  else '(metadata, free space, or outside the data area)')

    else:
        print(__doc__)


if __name__ == '__main__':
    main()
