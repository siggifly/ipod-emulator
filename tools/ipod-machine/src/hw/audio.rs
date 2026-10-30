//! The I2S transmit path, as a sink: what the PP5022 sends the WM8758, written out as WAV.
//!
//! Off by default. `Memory::audio` is `None` until a run asks for it (`trace --audio-wav=FILE`),
//! and with it `None` nothing here is reached: the FIFO port stays plain backing store, the DMA
//! pacing keeps its fixed rate, and interrupts are all delivered as IRQ.
//!
//! What is modelled:
//!
//! - **The FIFO port** at `IISFIFO_WR` (`0x70002840`). Bytes arrive one at a time from the bus
//!   (a word store is four, a halfword store two); a word is complete when all four have landed.
//!   Low halfword is left, high halfword is right. Every FIFO format Rockbox is known to use puts
//!   the samples there; the big-endian formats (IISCONFIG bits 6..4 = 1 or 2) are not
//!   distinguished.
//! - **Gating.** A word is kept only while `IISCONFIG.TXFIFOEN` (bit 29) is set and the codec is
//!   producing LRC (see [`Wm8758::sample_rate`]). Anything else is counted and dropped, because
//!   on the part a FIFO that nobody clocks does not drain.
//! - **Rate.** From the codec's registers each time a word arrives (cached until the codec is
//!   written again). A change of rate starts a new WAV file; one file has one rate.
//!
//! What is not modelled: DAC volume, soft mute, the EQ and the 3D stage, so the file is the PCM
//! at the codec's input, not at its output. The FIFO has no depth here; the DMA completion pacing
//! in `run_pp_dma` stands in for it.

use crate::hw::wm8758::Wm8758;
use std::path::PathBuf;

/// `IISCONFIG`, and its `TXFIFOEN` bit.
pub const IISCONFIG: u32 = 0x7000_2800;
pub const IIS_TXFIFOEN: u32 = 1 << 29;
/// `IISFIFO_CFG`. Bits 21..16 are the transmit FIFO's free-slot count.
pub const IISFIFO_CFG: u32 = 0x7000_280c;
pub const IIS_TX_FREE_EMPTY: u32 = 16 << 16;
/// `IISFIFO_WR`.
pub const IISFIFO_WR: u32 = 0x7000_2840;
/// The whole I2S register block, for the page-plainness test.
pub const IIS_BLOCK: u32 = 0x7000_2800;
pub const IIS_BLOCK_LEN: u32 = 0x100;

struct Segment {
    rate: u32,
    pcm: Vec<i16>,
}

pub struct Audio {
    path: PathBuf,
    pub mclk_hz: f64,
    segments: Vec<Segment>,
    bytes: [u8; 4],
    have: u8,
    /// `(codec.writes when computed, rate)`.
    cached: Option<(u64, Option<u32>)>,
    /// Words that completed in the FIFO port, kept or not.
    pub words: u64,
    /// Frames written to the output.
    pub frames: u64,
    pub dropped_fifo_disabled: u64,
    pub dropped_unclocked: u64,
    /// Kept frames sent while neither DAC was enabled.
    pub frames_dac_off: u64,
    /// Files written by [`Audio::finish`].
    pub written: Vec<PathBuf>,
}

impl Audio {
    pub fn new(path: impl Into<PathBuf>, mclk_hz: f64) -> Self {
        Self {
            path: path.into(),
            mclk_hz,
            segments: Vec::new(),
            bytes: [0; 4],
            have: 0,
            cached: None,
            words: 0,
            frames: 0,
            dropped_fifo_disabled: 0,
            dropped_unclocked: 0,
            frames_dac_off: 0,
            written: Vec::new(),
        }
    }

    /// The codec's current frame rate, or `None` while it is not clocking.
    pub fn rate(&mut self, codec: &Wm8758) -> Option<u32> {
        match self.cached {
            Some((at, rate)) if at == codec.writes => rate,
            _ => {
                let rate = codec.sample_rate(self.mclk_hz);
                self.cached = Some((codec.writes, rate));
                rate
            }
        }
    }

    /// One byte stored at `IISFIFO_WR + off`. `iisconfig` is the register as it stands.
    pub fn fifo_byte(&mut self, off: u32, val: u8, codec: &Wm8758, iisconfig: u32) {
        self.bytes[(off & 3) as usize] = val;
        self.have |= 1 << (off & 3);
        if self.have == 0xf {
            self.have = 0;
            self.push_word(u32::from_le_bytes(self.bytes), codec, iisconfig);
        }
    }

    fn push_word(&mut self, word: u32, codec: &Wm8758, iisconfig: u32) {
        self.words += 1;
        if iisconfig & IIS_TXFIFOEN == 0 {
            self.dropped_fifo_disabled += 1;
            return;
        }
        let Some(rate) = self.rate(codec) else {
            self.dropped_unclocked += 1;
            return;
        };
        if codec.dac_enabled() == (false, false) {
            self.frames_dac_off += 1;
        }
        if self.segments.last().is_none_or(|s| s.rate != rate) {
            self.segments.push(Segment {
                rate,
                pcm: Vec::new(),
            });
        }
        let seg = self.segments.last_mut().unwrap();
        seg.pcm.push(word as u16 as i16);
        seg.pcm.push((word >> 16) as u16 as i16);
        self.frames += 1;
    }

    /// The path segment `n` is written to: the requested path for the first, `STEM.N.EXT` after.
    fn segment_path(&self, n: usize) -> PathBuf {
        if n == 0 {
            return self.path.clone();
        }
        let stem = self.path.file_stem().unwrap_or_default().to_string_lossy();
        let name = match self.path.extension() {
            Some(e) => format!("{stem}.{n}.{}", e.to_string_lossy()),
            None => format!("{stem}.{n}"),
        };
        self.path.with_file_name(name)
    }

    /// Write every captured segment and forget it. Writes nothing if no frame was captured.
    pub fn finish(&mut self) -> std::io::Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for n in 0..self.segments.len() {
            let path = self.segment_path(n);
            let seg = &self.segments[n];
            std::fs::write(&path, wav_bytes(seg.rate, &seg.pcm))?;
            out.push(path);
        }
        self.segments.clear();
        self.written.extend(out.iter().cloned());
        Ok(out)
    }

    pub fn report(&self) -> Vec<String> {
        let mut out = vec![format!(
            "audio sink -> {}: {} FIFO words, {} frames kept, {} dropped (TXFIFOEN clear), \
             {} dropped (codec not clocking), {} kept with both DACs off",
            self.path.display(),
            self.words,
            self.frames,
            self.dropped_fifo_disabled,
            self.dropped_unclocked,
            self.frames_dac_off,
        )];
        for (n, s) in self.segments.iter().enumerate() {
            out.push(format!(
                "  segment {n}: {} Hz, {} frames, {:.3} s -> {}",
                s.rate,
                s.pcm.len() / 2,
                (s.pcm.len() / 2) as f64 / f64::from(s.rate),
                self.segment_path(n).display()
            ));
        }
        for p in &self.written {
            out.push(format!("  wrote {}", p.display()));
        }
        if self.frames == 0 {
            out.push("  no frame captured; no file written".into());
        }
        out
    }
}

/// A canonical 44-byte-header PCM WAV: 16-bit, two channels, interleaved samples.
pub fn wav_bytes(rate: u32, pcm: &[i16]) -> Vec<u8> {
    let data = (pcm.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + data as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate * 4).to_le_bytes());
    v.extend_from_slice(&4u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data.to_le_bytes());
    for s in pcm {
        v.extend_from_slice(&s.to_le_bytes());
    }
    v
}

/// Read back what [`wav_bytes`] writes: `(rate, interleaved samples)`. `None` for anything else.
pub fn parse_wav(b: &[u8]) -> Option<(u32, Vec<i16>)> {
    let le32 = |at: usize| Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?));
    let le16 = |at: usize| Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?));
    if b.get(0..4)? != b"RIFF" || b.get(8..16)? != b"WAVEfmt " || b.get(36..40)? != b"data" {
        return None;
    }
    if le16(20)? != 1 || le16(22)? != 2 || le16(34)? != 16 {
        return None;
    }
    let len = le32(40)? as usize;
    let data = b.get(44..44 + len)?;
    let pcm = (0..len / 2)
        .map(|i| i16::from_le_bytes([data[2 * i], data[2 * i + 1]]))
        .collect();
    Some((le32(24)?, pcm))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codec_with(writes: &[(u8, u16)]) -> Wm8758 {
        let mut c = Wm8758::new();
        for &(reg, val) in writes {
            c.transfer(0x82, [(reg << 1) | (val >> 8) as u8, val as u8, 0, 0]);
        }
        c
    }

    /// Rockbox's bring-up for 44.1 kHz and 48 kHz: PLLEN, MS, the PLL constants, then CLKSEL with
    /// MCLKDIV /2.
    fn clocked(rate: u32) -> Wm8758 {
        let pll: [(u8, u16); 4] = if rate == 44100 {
            [(0x24, 0x017), (0x25, 0x021), (0x26, 0x161), (0x27, 0x026)]
        } else {
            [(0x24, 0x018), (0x25, 0x00c), (0x26, 0x093), (0x27, 0x0e9)]
        };
        let mut w = vec![(0x01, 0x02f), (0x06, 0x001)];
        w.extend(pll);
        w.push((0x06, 0x145));
        codec_with(&w)
    }

    fn send(a: &mut Audio, c: &Wm8758, cfg: u32, w: u32) {
        for (i, b) in w.to_le_bytes().iter().enumerate() {
            a.fifo_byte(i as u32, *b, c, cfg);
        }
    }

    #[test]
    fn the_wav_header_round_trips() {
        let pcm = [1i16, -2, 300, -400];
        let b = wav_bytes(44100, &pcm);
        assert_eq!(b.len(), 44 + 8);
        let (rate, back) = parse_wav(&b).unwrap();
        assert_eq!((rate, back.as_slice()), (44100, pcm.as_slice()));
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(u32::from_le_bytes(b[28..32].try_into().unwrap()), 44100 * 4);
        assert!(parse_wav(&b[..40]).is_none());
    }

    #[test]
    fn a_word_is_complete_only_when_all_four_bytes_have_landed() {
        let c = clocked(44100);
        let mut a = Audio::new("unused.wav", Wm8758::DEFAULT_MCLK_HZ);
        for (i, b) in [0x34u8, 0x12, 0x78].iter().enumerate() {
            a.fifo_byte(i as u32, *b, &c, IIS_TXFIFOEN);
        }
        assert_eq!(a.words, 0);
        a.fifo_byte(3, 0x56, &c, IIS_TXFIFOEN);
        assert_eq!((a.words, a.frames), (1, 1));
        assert_eq!(
            a.segments[0].pcm,
            [0x1234, 0x5678],
            "left is the low halfword"
        );
    }

    #[test]
    fn halfword_stores_complete_a_word_too() {
        let c = clocked(44100);
        let mut a = Audio::new("unused.wav", Wm8758::DEFAULT_MCLK_HZ);
        for (off, b) in [(0u32, 1u8), (1, 0), (2, 2), (3, 0)] {
            a.fifo_byte(off, b, &c, IIS_TXFIFOEN);
        }
        assert_eq!(a.segments[0].pcm, [1, 2]);
    }

    #[test]
    fn nothing_is_kept_while_the_fifo_is_disabled_or_the_codec_is_silent() {
        let c = clocked(44100);
        let mut a = Audio::new("unused.wav", Wm8758::DEFAULT_MCLK_HZ);
        send(&mut a, &c, 0, 0x0001_0001);
        assert_eq!((a.words, a.frames, a.dropped_fifo_disabled), (1, 0, 1));
        // Codec at reset: MS clear, so it is the I2S slave and nothing clocks the FIFO.
        let silent = Wm8758::new();
        send(&mut a, &silent, IIS_TXFIFOEN, 0x0001_0001);
        assert_eq!((a.words, a.frames, a.dropped_unclocked), (2, 0, 1));
        assert!(a.report().iter().any(|l| l.contains("no file written")));
    }

    #[test]
    fn a_rate_change_starts_a_new_file() {
        let dir = std::env::temp_dir().join(format!("wm8758-seg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut a = Audio::new(dir.join("out.wav"), Wm8758::DEFAULT_MCLK_HZ);
        let mut c = clocked(44100);
        send(&mut a, &c, IIS_TXFIFOEN, 1);
        send(&mut a, &c, IIS_TXFIFOEN, 2);
        // The same codec, reprogrammed to the 48 kHz PLL set and divider.
        for (reg, val) in [
            (0x24u8, 0x018u16),
            (0x25, 0x00c),
            (0x26, 0x093),
            (0x27, 0x0e9),
        ] {
            c.transfer(0x82, [reg << 1, val as u8, 0, 0]);
        }
        send(&mut a, &c, IIS_TXFIFOEN, 3);
        let files = a.finish().unwrap();
        assert_eq!(files, vec![dir.join("out.wav"), dir.join("out.1.wav")]);
        let (r0, p0) = parse_wav(&std::fs::read(&files[0]).unwrap()).unwrap();
        let (r1, p1) = parse_wav(&std::fs::read(&files[1]).unwrap()).unwrap();
        assert_eq!((r0, p0.len()), (44100, 4));
        assert_eq!((r1, p1.len()), (48000, 2));
        assert!(
            a.finish().unwrap().is_empty(),
            "finish forgets what it wrote"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn frames_sent_with_both_dacs_off_are_counted_but_kept() {
        let c = clocked(44100);
        let mut a = Audio::new("unused.wav", Wm8758::DEFAULT_MCLK_HZ);
        send(&mut a, &c, IIS_TXFIFOEN, 7);
        assert_eq!((a.frames, a.frames_dac_off), (1, 1));
        let mut on = c;
        on.transfer(0x82, [0x03 << 1, 0x0f, 0, 0]);
        send(&mut a, &on, IIS_TXFIFOEN, 7);
        assert_eq!((a.frames, a.frames_dac_off), (2, 1));
    }
}
