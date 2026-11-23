//! End to end for the WM8758 and I2S sink: a bare-metal guest (`snippets/audio-tone.s`) programs
//! the codec over I2C, starts a DMA ring of a 1 kHz tone and restarts it from the FIQ handler, and
//! the WAV that comes out has to be the stream the guest played, at the rate the guest's own PLL
//! writes decode to, for as long as the guest played it.
//!
//! The guest is real ARM code run on the interpreter; nothing here calls the sink directly except
//! the two tests that say so.

use arm7tdmi::Bus as _;
use ipod_machine::hw::audio::parse_wav;
use ipod_machine::snippet::parse_words;
use ipod_machine::{map_hardware, EApp, Machine, Wm8758};
use std::path::PathBuf;

const CODE: &str = include_str!("../snippets/audio-tone.words");

const PARAMS: u32 = 0x400;
const TABLE: u32 = 0x500;
const RING: u32 = 0x1000;
const PHYS: u32 = 0x1000_0000;
const RING_SEGS: u32 = 4;
const TONE_HZ: f64 = 1000.0;

struct Plan {
    rate: u32,
    regs: Vec<(u8, u16)>,
    seg_frames: u32,
    total_segs: u32,
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// The register writes a driver makes to bring the codec up at `rate`: power, I2S format, the PLL
/// and divider for the rate, DAC mode, headphone volume. Values per `docs/hw/wm8758-i2s.md`.
fn bring_up(rate: u32) -> Vec<(u8, u16)> {
    let (pll, div, sr): ([u16; 4], u16, u16) = match rate {
        48000 => ([0x018, 0x00c, 0x093, 0x0e9], 2, 0),
        44100 => ([0x017, 0x021, 0x161, 0x026], 2, 0),
        32000 => ([0x018, 0x00c, 0x093, 0x0e9], 3, 1),
        24000 => ([0x018, 0x00c, 0x093, 0x0e9], 4, 2),
        22050 => ([0x017, 0x021, 0x161, 0x026], 4, 2),
        16000 => ([0x018, 0x00c, 0x093, 0x0e9], 5, 3),
        12000 => ([0x018, 0x00c, 0x093, 0x0e9], 6, 4),
        11025 => ([0x017, 0x021, 0x161, 0x026], 6, 4),
        8000 => ([0x018, 0x00c, 0x093, 0x0e9], 7, 5),
        _ => panic!("no PLL set for {rate}"),
    };
    let mut w = vec![
        (0x3d, 0x100),
        (0x31, 0x187),
        (0x34, 0x140),
        (0x35, 0x140),
        (0x02, 0x180),
        (0x2a, 0x004),
        (0x03, 0x00f),
        (0x01, 0x02f),
        (0x04, 0x010),
        (0x06, 0x001),
    ];
    for (i, v) in pll.iter().enumerate() {
        w.push((0x24 + i as u8, *v));
    }
    w.push((0x06, 0x100 | (div << 5) | 0x004 | 0x001));
    w.push((0x07, (sr << 1) | 1));
    w.extend([(0x32, 0x001), (0x33, 0x001), (0x2a, 0x000)]);
    w.extend([(0x01, 0x02e), (0x3d, 0x000), (0x0a, 0x008)]);
    w.extend([(0x0b, 0x0ff), (0x0c, 0x1ff), (0x34, 0x0b9), (0x35, 0x1b9)]);
    w
}

/// Frames per segment: a whole number of 1 kHz periods, so the ring wraps without a phase step,
/// and at least 10 ms.
fn segment_frames(rate: u32) -> u32 {
    let unit = rate / gcd(rate, 1000);
    unit * (rate / 100 / unit).max(1)
}

fn plan(rate: u32) -> Plan {
    let seg_frames = segment_frames(rate);
    let total_segs = (0.3 * f64::from(rate) / f64::from(seg_frames)).ceil() as u32;
    Plan {
        rate,
        regs: bring_up(rate),
        seg_frames,
        total_segs: total_segs.max(RING_SEGS * 2 + 1),
    }
}

/// One cycle of the ring, as the guest will fetch it: left is a sine, right a cosine at half
/// amplitude, so a swapped or shifted channel cannot match by accident.
fn ring(p: &Plan) -> Vec<(i16, i16)> {
    (0..p.seg_frames * RING_SEGS)
        .map(|n| {
            let ph = 2.0 * std::f64::consts::PI * TONE_HZ * f64::from(n) / f64::from(p.rate);
            (
                (16000.0 * ph.sin()).round() as i16,
                (8000.0 * ph.cos()).round() as i16,
            )
        })
        .collect()
}

fn put32(img: &mut [u8], at: u32, v: u32) {
    img[at as usize..at as usize + 4].copy_from_slice(&v.to_le_bytes());
}

fn image(p: &Plan) -> Vec<u8> {
    let seg_bytes = p.seg_frames * 4;
    let ring_bytes = seg_bytes * RING_SEGS;
    let mut img = vec![0u8; (RING + ring_bytes) as usize];
    let code = parse_words(CODE).unwrap();
    img[..code.len()].copy_from_slice(&code);
    assert!(code.len() <= PARAMS as usize);

    // DMA0_CMD: request id 2, wait for request, single shot, RAM to peripheral, interrupt, start.
    let cmd =
        (2 << 16) | (1 << 24) | (1 << 26) | (1 << 27) | (1 << 30) | (1 << 31) | (seg_bytes - 4);
    for (i, v) in [
        PHYS + RING,
        PHYS + RING + ring_bytes,
        seg_bytes,
        cmd,
        p.total_segs,
        PHYS + RING,
        0,
        0,
        TABLE,
    ]
    .iter()
    .enumerate()
    {
        put32(&mut img, PARAMS + 4 * i as u32, *v);
    }
    assert!(p.regs.len() < 64);
    for (i, (reg, val)) in p.regs.iter().enumerate() {
        let b0 = (u32::from(*reg) << 1) | u32::from(val >> 8);
        put32(
            &mut img,
            TABLE + 4 * i as u32,
            b0 | (u32::from(*val & 0xff) << 8),
        );
    }
    put32(&mut img, TABLE + 4 * p.regs.len() as u32, 0xffff_ffff);
    for (n, (l, r)) in ring(p).iter().enumerate() {
        let w = u32::from(*l as u16) | (u32::from(*r as u16) << 16);
        put32(&mut img, RING + 4 * n as u32, w);
    }
    img
}

fn machine(img: &[u8]) -> Machine {
    let mut m = Machine::new(&EApp::none(), 0x1100_0000, 0x0100_0000);
    map_hardware(&mut m, false);
    m.mem.i2c_base = Some(0x7000_c000);
    for (i, b) in img.iter().enumerate() {
        m.mem.write8(i as u32, *b);
    }
    m.cpu.regs[15] = 0;
    m
}

fn word(m: &mut Machine, off: u32) -> u32 {
    m.mem.read32(PARAMS + off)
}

fn out_path(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// Run until the guest sets its done flag, in slices so a hang fails rather than blocks.
fn run_to_done(m: &mut Machine) -> bool {
    for _ in 0..120 {
        m.run(500_000);
        if word(m, 24) != 0 {
            return true;
        }
    }
    false
}

/// Rising zero crossings of `left`, located by linear interpolation, as a frequency.
fn tone_hz(left: &[i16], rate: u32) -> f64 {
    let mut times = Vec::new();
    for (i, w) in left.windows(2).enumerate() {
        let (a, b) = (f64::from(w[0]), f64::from(w[1]));
        if a < 0.0 && b >= 0.0 {
            times.push(i as f64 + (-a) / (b - a));
        }
    }
    assert!(times.len() > 10, "{} crossings", times.len());
    let cycles = (times.len() - 1) as f64;
    cycles * f64::from(rate) / (times[times.len() - 1] - times[0])
}

fn play(rate: u32) {
    let p = plan(rate);
    let img = image(&p);
    let mut m = machine(&img);
    let wav = out_path(&format!("tone-{rate}.wav"));
    let _ = std::fs::remove_file(&wav);
    m.mem.enable_audio(&wav, Wm8758::DEFAULT_MCLK_HZ);
    assert!(
        run_to_done(&mut m),
        "{rate}: the guest never finished; pc {:#x}",
        m.cpu.regs[15]
    );

    let files = m.mem.audio.as_mut().unwrap().finish().unwrap();
    assert_eq!(
        files,
        vec![wav.clone()],
        "{rate}: exactly one rate was played"
    );
    let (got_rate, pcm) = parse_wav(&std::fs::read(&wav).unwrap()).expect("a canonical WAV");
    assert_eq!(
        got_rate, rate,
        "rate decoded from the guest's PLL and divider writes"
    );

    let frames = (p.total_segs * p.seg_frames) as usize;
    assert_eq!(pcm.len(), frames * 2, "{rate}: length");
    let ring = ring(&p);
    for (n, f) in pcm.chunks(2).enumerate() {
        assert_eq!((f[0], f[1]), ring[n % ring.len()], "{rate}: frame {n}");
    }
    let left: Vec<i16> = pcm.chunks(2).map(|f| f[0]).collect();
    let hz = tone_hz(&left, got_rate);
    assert!((hz - TONE_HZ).abs() < 0.5, "{rate}: decoded {hz} Hz");

    // One FIQ per segment, delivered as FIQ and not IRQ, counted by the guest and the machine.
    assert_eq!(word(&mut m, 28), p.total_segs, "{rate}: guest FIQ count");
    assert_eq!(m.fiqs_taken, u64::from(p.total_segs), "{rate}: FIQs taken");
    assert_eq!(m.irqs_taken, 0, "{rate}: nothing arrived as IRQ");

    // The DMA is paced at the decoded rate, so the run took as long as the audio does.
    let secs = frames as f64 / f64::from(rate);
    let ran = f64::from(m.mem.usec) / 1e6;
    assert!(
        ran >= secs && ran < secs * 1.05 + 0.01,
        "{rate}: {ran} s simulated for {secs} s of audio"
    );
    let a = m.mem.audio.as_ref().unwrap();
    assert_eq!(
        (a.frames, a.dropped_fifo_disabled, a.dropped_unclocked),
        (frames as u64, 0, 0)
    );
    assert_eq!(
        a.frames_dac_off, 0,
        "R3 enabled both DACs before the first frame"
    );
}

#[test]
fn a_1khz_tone_comes_out_at_44100() {
    play(44100);
}

#[test]
fn a_1khz_tone_comes_out_at_48000() {
    play(48000);
}

#[test]
fn every_rate_the_sound_layer_offers_plays_at_its_own_rate() {
    for rate in Wm8758::RATES {
        play(rate);
    }
}

#[test]
fn a_guest_that_never_clocks_the_codec_is_paced_but_records_nothing() {
    let mut p = plan(44100);
    p.regs = vec![(0x03, 0x00f), (0x04, 0x010)];
    let mut m = machine(&image(&p));
    let wav = out_path("unclocked.wav");
    let _ = std::fs::remove_file(&wav);
    m.mem.enable_audio(&wav, Wm8758::DEFAULT_MCLK_HZ);
    assert!(run_to_done(&mut m));
    let a = m.mem.audio.as_mut().unwrap();
    assert_eq!(a.frames, 0);
    assert_eq!(a.dropped_unclocked, u64::from(p.total_segs * p.seg_frames));
    assert!(a.finish().unwrap().is_empty());
    assert!(!wav.exists(), "no frame, no file");
}

#[test]
fn with_the_sink_off_nothing_about_the_machine_changes() {
    let p = plan(44100);
    let mut m = machine(&image(&p));
    // Not enabled: the DMA still completes at the old fixed pace, but priority routing is off, so
    // the completion arrives as IRQ, lands on the guest's `b .` IRQ vector and the guest stops.
    m.run(3_000_000);
    assert!(m.mem.audio.is_none() && !m.mem.fiq_routing);
    assert_eq!(
        (m.fiqs_taken, word(&mut m, 28), word(&mut m, 24)),
        (0, 0, 0)
    );
    assert!(
        m.mem.pp_dma_transfers >= 1,
        "the first segment was still moved"
    );
    assert_eq!(m.mem.i2s_paced_transfers, 1);
}

#[test]
fn cpu_stores_to_the_fifo_port_reach_the_sink_and_the_free_count_reads_empty() {
    let p = plan(48000);
    let mut m = machine(&image(&Plan { total_segs: 1, ..p }));
    let wav = out_path("pio.wav");
    m.mem.enable_audio(&wav, Wm8758::DEFAULT_MCLK_HZ);
    assert!(run_to_done(&mut m));
    let before = m.mem.audio.as_ref().unwrap().frames;
    m.mem.write32(0x7000_2840, 0x0002_0001);
    m.mem.write8(0x7000_2840, 3);
    m.mem.write8(0x7000_2841, 0);
    m.mem.write8(0x7000_2842, 4);
    m.mem.write8(0x7000_2843, 0);
    assert_eq!(m.mem.audio.as_ref().unwrap().frames, before + 2);
    let cfg = m.mem.read32(0x7000_280c);
    assert_eq!((cfg >> 16) & 0x3f, 16, "TX free count: empty");
    assert_eq!(cfg & 0x3f, 1, "the guest's request level survives the OR");
    let files = m.mem.audio.as_mut().unwrap().finish().unwrap();
    let (rate, pcm) = parse_wav(&std::fs::read(&files[0]).unwrap()).unwrap();
    assert_eq!(rate, 48000);
    assert_eq!(&pcm[pcm.len() - 4..], &[1, 2, 3, 4]);
}

/// The guest image is also written out, so the same bytes can be booted through `trace` and
/// `tools/emu/mkdrive.pl`: `trace ... --osos-from-disk --boot-osos --disk=DRIVE --audio-wav=FILE`.
#[test]
fn the_guest_image_is_kept_for_the_trace_boot() {
    for rate in [44100, 48000] {
        let img = image(&plan(rate));
        let path = out_path(&format!("audio-tone-{rate}.bin"));
        std::fs::write(&path, &img).unwrap();
        // mkdrive finds an image by two leading branch words.
        assert_eq!(img[3], 0xea);
        assert_eq!(img[7], 0xea);
    }
}

/// Only the audio DMA controller's interrupt (26) is routed as FIQ. RetailOS sets the priority bits
/// of other sources too, and with those routed its boot stalls in an FIQ storm.
#[test]
fn only_the_audio_dma_source_is_delivered_as_fiq() {
    const CPU_INT_EN: u32 = 0x6000_4024;
    const INT_FORCED_SET: u32 = 0x6000_4018;
    const CPU_INT_PRIORITY: u32 = 0x6000_402c;

    let run_with = |source: u32| {
        // Every vector is `b .`.
        let img: Vec<u8> = (0..16)
            .flat_map(|_| 0xeaff_fffe_u32.to_le_bytes())
            .collect();
        let mut m = machine(&img);
        m.mem
            .enable_audio(out_path("fiq-mask.wav"), Wm8758::DEFAULT_MCLK_HZ);
        m.cpu.cpsr.0 &= !0xc0;
        m.mem.write32(CPU_INT_PRIORITY, (1 << 26) | (1 << 27));
        m.mem.write32(CPU_INT_EN, 1 << source);
        m.mem.write32(INT_FORCED_SET, 1 << source);
        m.run(20_000);
        (m.irqs_taken, m.fiqs_taken)
    };
    let (irqs, fiqs) = run_with(27);
    assert!(irqs > 0 && fiqs == 0, "source 27: {irqs} IRQs, {fiqs} FIQs");
    let (irqs, fiqs) = run_with(26);
    assert!(fiqs > 0 && irqs == 0, "source 26: {irqs} IRQs, {fiqs} FIQs");
}

/// The stock FIQ handler finds its source by reading `CPU_FIQ_STAT`, not a DMA register.
#[test]
fn cpu_fiq_stat_shows_the_routed_audio_dma_source_until_it_is_serviced() {
    const CPU_FIQ_STAT: u32 = 0x6000_4008;
    const CPU_INT_EN: u32 = 0x6000_4024;
    const CPU_INT_DIS: u32 = 0x6000_4028;
    const CPU_INT_PRIORITY: u32 = 0x6000_402c;
    const DMA_MASTER: u32 = 0x6000_a000;
    const DMA0_CMD: u32 = 0x6000_b000;
    const DMA0_STATUS: u32 = 0x6000_b004;
    const AUDIO_DMA: u32 = 1 << 26;

    let mut m = machine(&[0; 16]);
    m.mem
        .enable_audio(out_path("fiq-stat.wav"), Wm8758::DEFAULT_MCLK_HZ);
    m.cpu.cpsr.0 |= 0xc0;
    m.mem.write32(DMA_MASTER, 1 << 31);
    m.mem.write32(DMA0_CMD, 1 << 30);
    m.mem.write32(DMA0_STATUS, 1 << 30);
    m.mem.write32(CPU_INT_EN, AUDIO_DMA);

    // Pending and enabled, but not routed: the source is an IRQ, not an FIQ.
    m.service_interrupts();
    assert_eq!(m.mem.read32(CPU_FIQ_STAT), 0);

    m.mem.write32(CPU_INT_PRIORITY, AUDIO_DMA);
    m.service_interrupts();
    assert_eq!(m.mem.read32(CPU_FIQ_STAT), AUDIO_DMA);

    // Level: it stays while the completion is unacknowledged.
    m.service_interrupts();
    assert_eq!(m.mem.read32(CPU_FIQ_STAT), AUDIO_DMA);

    // Not enabled.
    m.mem.write32(CPU_INT_DIS, AUDIO_DMA);
    m.service_interrupts();
    assert_eq!(m.mem.read32(CPU_FIQ_STAT), 0);
    m.mem.write32(CPU_INT_EN, AUDIO_DMA);
    m.service_interrupts();
    assert_eq!(m.mem.read32(CPU_FIQ_STAT), AUDIO_DMA);

    // Serviced: reading DMA0_STATUS clears the completion latch and the line drops.
    assert_ne!(m.mem.read32(DMA0_STATUS) & (1 << 30), 0);
    m.service_interrupts();
    assert_eq!(m.mem.read32(CPU_FIQ_STAT), 0);

    // A new completion, then the source is routed back to IRQ.
    m.mem.write32(DMA0_STATUS, 1 << 30);
    m.service_interrupts();
    assert_eq!(m.mem.read32(CPU_FIQ_STAT), AUDIO_DMA);
    m.mem.write32(CPU_INT_PRIORITY, 0);
    m.service_interrupts();
    assert_eq!(m.mem.read32(CPU_FIQ_STAT), 0);
}

/// With the sink off the register is backing store, as it was before the model wrote it.
#[test]
fn cpu_fiq_stat_is_left_alone_with_the_sink_off() {
    let mut m = machine(&[0; 16]);
    m.cpu.cpsr.0 |= 0xc0;
    m.mem.write32(0x6000_4008, 0x1234_5678);
    m.mem.write32(0x6000_a000, 1 << 31);
    m.mem.write32(0x6000_b000, 1 << 30);
    m.mem.write32(0x6000_b004, 1 << 30);
    m.mem.write32(0x6000_4024, 1 << 26);
    m.mem.write32(0x6000_402c, 1 << 26);
    m.service_interrupts();
    assert!(!m.mem.fiq_routing);
    assert_eq!(m.mem.read32(0x6000_4008), 0x1234_5678);
}
