//! Every gif in `docs/media/` is a picture of what the panel showed, and nothing else.
//!
//! KNOWN-BUGS, *"The gif encode invents pixels the panel never showed"*: ffmpeg's default
//! `+transdiff` stores difference frames with a transparent index, and with disposal 1 the earlier
//! frame shows through the holes — `ipod-15-rockbox-wheel.gif` shipped yellow ticks that no frame
//! the machine produced contained. That entry's own test for "fixed" is **every frame in a shipped
//! gif reports `transparent no`**, and this is that test, run over the files rather than trusted
//! of the encoder. `ipod-film`'s `every_gif_is_encoded_without_transparent_differences` guards the
//! other side.

use std::path::{Path, PathBuf};

fn media_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/media")
}

/// What one gif's frames say about transparency.
#[derive(Debug, Default)]
struct Gif {
    frames: usize,
    transparent: usize,
}

/// Walk a GIF89a's blocks. Only the structure is read: the image data is skipped sub-block by
/// sub-block, and each Graphic Control Extension's packed byte is checked for bit 0, the
/// transparent-colour flag.
fn scan(b: &[u8]) -> Result<Gif, String> {
    if b.len() < 13 || !(b.starts_with(b"GIF89a") || b.starts_with(b"GIF87a")) {
        return Err("not a gif".into());
    }
    let mut g = Gif::default();
    let packed = b[10];
    let mut i = 13;
    if packed & 0x80 != 0 {
        i += 3 << ((packed & 7) + 1);
    }
    let skip_subblocks = |mut i: usize| -> Result<usize, String> {
        loop {
            let n = *b.get(i).ok_or("truncated sub-block")? as usize;
            i += 1;
            if n == 0 {
                return Ok(i);
            }
            i += n;
        }
    };
    loop {
        match *b.get(i).ok_or("no trailer")? {
            0x3b => return Ok(g),
            0x21 => {
                let label = *b.get(i + 1).ok_or("truncated extension")?;
                if label == 0xf9 {
                    let flags = *b.get(i + 3).ok_or("truncated GCE")?;
                    if flags & 1 != 0 {
                        g.transparent += 1;
                    }
                }
                i = skip_subblocks(i + 2)?;
            }
            0x2c => {
                g.frames += 1;
                let p = *b.get(i + 9).ok_or("truncated descriptor")?;
                i += 10;
                if p & 0x80 != 0 {
                    i += 3 << ((p & 7) + 1);
                }
                i += 1; // LZW minimum code size
                i = skip_subblocks(i)?;
            }
            other => return Err(format!("unexpected block 0x{other:02x} at {i}")),
        }
    }
}

#[test]
fn the_scanner_sees_a_transparent_frame_when_there_is_one() {
    // The control: a scanner that never finds the flag would pass every file just as quietly.
    // A 1x1 GIF89a, global table of two colours, one GCE with the transparent flag set, one image.
    let mut g = b"GIF89a\x01\x00\x01\x00\x80\x00\x00".to_vec();
    g.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
    g.extend_from_slice(&[0x21, 0xf9, 4, 0x05, 0, 0, 0, 0]);
    g.extend_from_slice(&[0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x4c, 0x01, 0]);
    g.push(0x3b);
    let s = scan(&g).unwrap();
    assert_eq!((s.frames, s.transparent), (1, 1));
    g[22] = 0x04; // the GCE packed byte: clear bit 0
    let s = scan(&g).unwrap();
    assert_eq!((s.frames, s.transparent), (1, 0));
}

#[test]
fn no_shipped_gif_has_a_transparent_frame() {
    let mut checked = 0;
    let mut bad = Vec::new();
    for e in std::fs::read_dir(media_dir()).expect("docs/media is in the repository") {
        let p = e.unwrap().path();
        if p.extension().and_then(|x| x.to_str()) != Some("gif") {
            continue;
        }
        let s =
            scan(&std::fs::read(&p).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert!(s.frames > 0, "{}: no frames", p.display());
        checked += 1;
        if s.transparent > 0 {
            bad.push(format!(
                "{}: {} of {} frames carry a transparent index",
                p.file_name().unwrap().to_string_lossy(),
                s.transparent,
                s.frames
            ));
        }
    }
    assert!(checked >= 1, "no gifs found, so this proves nothing");
    assert!(
        bad.is_empty(),
        "transdiff holes in shipped gifs:\n{}",
        bad.join("\n")
    );
}
