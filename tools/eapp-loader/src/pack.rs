//! Storing a parked machine as what it contains rather than as how big it is.
//!
//! A snapshot of a booted 5.5G is 151 MB and almost all of it is zeros: the format writes the
//! whole address space, and an iPod that has finished booting has written a small fraction of it.
//! Stored raw, two parked devices cost 301 MB — **fifteen times what their drives cost**, because
//! a drive image is sparse on disk and a snapshot was not. That is backwards. The drive is the
//! irreplaceable half.
//!
//! Measured on this repository's own parked device:
//!
//! ```text
//! $ ls -l snapshots/my-5.5g.snap          151 MB
//! $ gzip -1 -c snapshots/my-5.5g.snap     4.1 MB      37:1
//! ```
//!
//! Nearly all of that ratio is runs of zeros rather than general redundancy, so a zero-run encoder
//! collects most of it without a compression crate. That is the deciding argument: `eapp-loader`
//! has **exactly one dependency**, a path dep on the CPU, and sixty lines is a smaller thing to own
//! than a supply chain — especially in a program whose whole claim is that it is auditable.
//!
//! The format is deliberately dull:
//!
//! ```text
//! "IPODSNZ1"          magic
//! u64                 length of the original bytes
//! then, repeating:
//!   u32 n + n bytes     a literal run
//!   u32 z               that many zero bytes
//! ```
//!
//! Both counts appear in every pair and either may be zero, so there is no framing to get wrong
//! and no optional field to disagree about. The stream ends when the decoded length reaches the
//! header's; a stream that ends early, or one that would overshoot, is refused.
//!
//! **A snapshot is a cache entry, and this is why that distinction earns its keep.** Nothing here
//! reads the older raw `IPODSNP7` file that used to be written: `unpack` refuses it, the caller
//! cold-boots, and the cost of that is one boot. A drive image would have to be migrated because
//! losing it loses somebody's iPod. A restore point can simply be regenerated, so the cheapest
//! correct thing is to regenerate it.

/// What a packed stream starts with. Distinct from the `IPODSNP7` that `Machine::snapshot` emits,
/// so the two can never be handed to the wrong reader — the magic is the discriminator.
const MAGIC: &[u8; 8] = b"IPODSNZ1";

/// Below this, a run of zeros is cheaper left inside the literal than broken out.
///
/// Breaking costs the eight bytes of framing that the next pair's two counts occupy; leaving the
/// zeros in the literal costs one byte each. So the break pays for itself above eight, and this is
/// set a little above the break-even rather than exactly at it — the win from shaving a nine-byte
/// run is one byte, and it is not worth the framing churn to chase.
const MIN_RUN: usize = 16;

/// The largest original length a header may claim.
///
/// A zero run is eight bytes of input that can become any number of bytes of output, so a corrupt
/// or truncated header is an out-of-memory panic waiting to happen — in the exact place that must
/// not panic, since a snapshot is written by a background thread as the window closes and a
/// truncated one is precisely what a crash leaves behind. A 5.5G snapshot is 151 MB; nothing this
/// program builds approaches half a gigabyte, so a header claiming more is corrupt by definition.
const MAX_LEN: usize = 512 << 20;

/// The one walk that both [`pack`] and [`packed_len`] use.
///
/// Two copies of this loop would drift, and the drift would be invisible in the worst way: the
/// estimate is a free-space check, so an estimate that has drifted *low* is a park that passes the
/// check and then fills the volume. One loop, two things done with each pair.
fn walk(raw: &[u8], mut emit: impl FnMut(&[u8], usize)) {
    let mut i = 0usize;
    while i < raw.len() {
        // Walk to the next run of zeros long enough to be worth breaking the literal for. Shorter
        // runs are stepped over and stay inside the literal, which is what MIN_RUN measures.
        let start = i;
        while i < raw.len() {
            if raw[i] != 0 {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < raw.len() && raw[j] == 0 {
                j += 1;
            }
            if j - i >= MIN_RUN {
                break;
            }
            i = j;
        }
        let lit = &raw[start..i];
        let mut j = i;
        while j < raw.len() && raw[j] == 0 {
            j += 1;
        }
        emit(lit, j - i);
        i = j;
    }
}

/// Squeeze a snapshot for storage. Never fails: any input round-trips.
pub fn pack(raw: &[u8]) -> Vec<u8> {
    let mut o = Vec::with_capacity(raw.len() / 16 + 64);
    o.extend_from_slice(MAGIC);
    o.extend_from_slice(&(raw.len() as u64).to_le_bytes());
    walk(raw, |lit, zeros| {
        o.extend_from_slice(&(lit.len() as u32).to_le_bytes());
        o.extend_from_slice(lit);
        o.extend_from_slice(&(zeros as u32).to_le_bytes());
    });
    o
}

/// What [`pack`] *would* produce, without producing it.
///
/// The window has to answer "will a park fit on this disk?" before it takes one, so the size has
/// to be knowable without the allocation. This is that number and it is exact, not an estimate —
/// `pack_and_packed_len_never_disagree` holds the two together.
pub fn packed_len(raw: &[u8]) -> usize {
    let mut n = 16usize; // magic + the original length
    walk(raw, |lit, _| n += 4 + lit.len() + 4);
    n
}

/// Read one back, or `None` if it is not one of ours, is truncated, or disagrees with itself.
///
/// Refusing rather than panicking is the same posture `Machine::restore` takes and for the same
/// reason: every caller already rebuilds the machine when the restore point turns out to be no
/// good, and killing the program is least helpful at exactly the moment a disk filled up.
pub fn unpack(b: &[u8]) -> Option<Vec<u8>> {
    if b.len() < 16 || &b[..8] != MAGIC {
        return None;
    }
    let want = u64::from_le_bytes(b[8..16].try_into().ok()?) as usize;
    if want > MAX_LEN {
        return None;
    }
    let mut o: Vec<u8> = Vec::with_capacity(want);
    let mut p = 16usize;

    let u32_at = |p: &mut usize| -> Option<usize> {
        if *p + 4 > b.len() {
            return None;
        }
        let v = u32::from_le_bytes(b[*p..*p + 4].try_into().ok()?) as usize;
        *p += 4;
        Some(v)
    };

    while o.len() < want {
        let n = u32_at(&mut p)?;
        // Both bounds are checked before either buffer is touched, so a stream claiming more than
        // it carries — or more than its own header — cannot allocate or read past anything.
        if p + n > b.len() || o.len() + n > want {
            return None;
        }
        o.extend_from_slice(&b[p..p + n]);
        p += n;

        let z = u32_at(&mut p)?;
        if o.len() + z > want {
            return None;
        }
        o.resize(o.len() + z, 0);

        // A pair that consumed nothing would spin here forever on a malformed stream. It cannot
        // happen in anything `pack` writes — every pair it emits advances — but this reads files
        // that may have been truncated mid-write, and a hang is worse than a refusal.
        if n == 0 && z == 0 {
            return None;
        }
    }
    (o.len() == want).then_some(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(raw: &[u8]) {
        let got = unpack(&pack(raw));
        assert_eq!(got.as_deref(), Some(raw), "round trip changed the bytes");
    }

    #[test]
    fn the_shapes_that_have_no_middle() {
        roundtrip(b"");
        roundtrip(b"\0");
        roundtrip(b"x");
        roundtrip(&[0u8; 4096]);
        roundtrip(&(0..=255u8).collect::<Vec<_>>());
        // Zeros at both ends, which is where an off-by-one in the literal walk shows up.
        let mut v = vec![0u8; 100];
        v.extend_from_slice(b"middle");
        v.extend(std::iter::repeat(0).take(100));
        roundtrip(&v);
    }

    #[test]
    fn a_snapshot_shaped_buffer_round_trips_and_actually_shrinks() {
        // The shape this exists for: mostly-zero address space with islands of real state.
        let mut raw = vec![0u8; 8 << 20];
        for (n, at) in [(0usize, 0usize), (1, 1 << 20), (2, 3 << 20), (3, 7 << 20)] {
            for k in 0..4096 {
                raw[at + k] = (k as u8).wrapping_add(n as u8);
            }
        }
        roundtrip(&raw);

        // **The round trip alone would pass if `pack` returned its input**, which is the whole
        // reason this assertion is here: it is the one that fails if the compression stops
        // compressing. Measured ratio on this buffer is far above 10; the bar is set low so an
        // encoder change has room to move without a spurious failure.
        let ratio = raw.len() as f64 / pack(&raw).len() as f64;
        assert!(ratio > 10.0, "packed only {ratio:.1}:1 — is it still compressing?");
    }

    /// The ratio on a **real** parked machine, which the synthetic buffer above cannot vouch for.
    ///
    /// Ignored because it needs a file this repository must never contain. Point it at one:
    ///
    /// ```text
    /// IPOD_PACK_SAMPLE="$HOME/Library/Application Support/ipod-emulator/snapshots/my-5.5g.snap" \
    ///   cargo test -p eapp-loader --lib pack:: -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn the_ratio_on_a_real_snapshot() {
        let Ok(path) = std::env::var("IPOD_PACK_SAMPLE") else {
            panic!("set IPOD_PACK_SAMPLE to a .snap file");
        };
        let raw = std::fs::read(&path).expect("read the sample");
        let packed = pack(&raw);
        println!(
            "  {} MB -> {:.1} MB   {:.0}:1",
            raw.len() >> 20,
            packed.len() as f64 / (1 << 20) as f64,
            raw.len() as f64 / packed.len() as f64
        );
        assert_eq!(unpack(&packed).as_deref(), Some(&raw[..]), "real snapshot round trip");
    }

    #[test]
    fn pack_and_packed_len_never_disagree() {
        // Every shape the other tests use, plus the awkward ones, through both paths. If these two
        // ever diverge the free-space check silently stops describing the write it guards.
        let mut cases: Vec<Vec<u8>> = vec![
            b"".to_vec(),
            b"\0".to_vec(),
            b"x".to_vec(),
            vec![0u8; MIN_RUN - 1],
            vec![0u8; MIN_RUN],
            vec![0u8; MIN_RUN + 1],
            (0..=255u8).collect(),
        ];
        let mut mixed = vec![0u8; 1 << 16];
        mixed[100..200].fill(0xEE);
        mixed[40000..40010].fill(0x01); // a run shorter than MIN_RUN on both sides
        cases.push(mixed);
        for c in &cases {
            assert_eq!(packed_len(c), pack(c).len(), "disagreed on {} bytes", c.len());
        }
    }

    #[test]
    fn incompressible_input_does_not_explode() {
        // No zero runs at all, so every byte is a literal. The overhead is the framing, and it
        // must stay bounded rather than growing per byte.
        let raw: Vec<u8> = (0..65536u32).map(|i| (i % 255 + 1) as u8).collect();
        roundtrip(&raw);
        assert!(
            pack(&raw).len() < raw.len() + 1024,
            "framing overhead grew with the data"
        );
    }

    #[test]
    fn a_truncated_stream_is_refused_rather_than_fatal() {
        let mut raw = vec![0u8; 1 << 20];
        raw[512..1024].fill(0xAB);
        let full = pack(&raw);
        // Every prefix, including the empty one. None may panic and none may claim success.
        for cut in 0..full.len() {
            assert!(unpack(&full[..cut]).is_none(), "accepted a {cut}-byte prefix");
        }
        assert_eq!(unpack(&full).as_deref(), Some(&raw[..]));
    }

    #[test]
    fn the_old_raw_snapshot_is_refused_so_the_caller_cold_boots() {
        // Not a migration: a restore point is a cache entry and regenerating it costs one boot.
        // What matters is that it is refused *cleanly*, since that is what routes the caller to
        // the cold-boot path rather than into a half-loaded machine.
        let mut old = b"IPODSNP7".to_vec();
        old.extend_from_slice(&[0u8; 4096]);
        assert!(unpack(&old).is_none());
    }

    #[test]
    fn a_lying_header_cannot_make_us_allocate() {
        let mut evil = MAGIC.to_vec();
        evil.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(unpack(&evil).is_none());

        // Just under the ceiling, but the stream carries nothing to fill it with.
        let mut short = MAGIC.to_vec();
        short.extend_from_slice(&((MAX_LEN - 1) as u64).to_le_bytes());
        short.extend_from_slice(&0u32.to_le_bytes());
        short.extend_from_slice(&0u32.to_le_bytes());
        assert!(unpack(&short).is_none());
    }
}
