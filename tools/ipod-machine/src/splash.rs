//! The boot splash: an image somebody chose, fitted to the space Apple's logo occupies.
//!
//! A synthesised NOR has no `logo` image and could not carry Apple's if it did. The default is this
//! project's own click wheel ([`crate::nor::boot_screen`]); this module is what lets a person supply
//! something else. **What they supply is their business** — including, if they have extracted it
//! from a dump they own, Apple's. That is a decision about their own files, not one this program
//! makes for them.
//!
//! ## Formats
//!
//! PNG — 8-bit greyscale, RGB or RGBA, non-interlaced, which is what every export produces — and
//! binary PPM (`P6`). The PNG path reuses [`crate::ipsw::inflate`], which already exists for reading
//! firmware bundles; there is no image dependency and no new decompressor.
//!
//! Interlaced or 16-bit PNGs are **refused with the reason** rather than decoded badly. A splash
//! that comes out wrong is worse than one that does not come out.

use crate::ipsw::inflate;

/// A decoded image, 8 bits per channel, RGB.
#[derive(Clone, Debug)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    /// `w * h * 3` bytes.
    pub rgb: Vec<u8>,
}

/// Decode a PNG or PPM.
pub fn decode(data: &[u8]) -> Result<Image, String> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        decode_png(data)
    } else if data.starts_with(b"P6") {
        decode_ppm(data)
    } else {
        Err("not a PNG or a binary PPM. Those are the two this reads — export as PNG.".into())
    }
}

fn decode_ppm(d: &[u8]) -> Result<Image, String> {
    // `P6`, then width, height and maxval, each separated by whitespace, comments starting `#`.
    let mut it = d[2..].iter().copied().enumerate();
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut at = 2;
    while fields.len() < 3 {
        let Some((i, c)) = it.next() else {
            return Err("truncated PPM header".into());
        };
        at = i + 3;
        if c == b'#' {
            for (_, c) in it.by_ref() {
                if c == b'\n' {
                    break;
                }
            }
            continue;
        }
        if c.is_ascii_whitespace() {
            if !cur.is_empty() {
                fields.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c as char);
        }
    }
    let n = |s: &String| {
        s.parse::<usize>()
            .map_err(|_| format!("bad PPM header field {s}"))
    };
    let (w, h, max) = (n(&fields[0])?, n(&fields[1])?, n(&fields[2])?);
    if max != 255 {
        return Err(format!("PPM maxval is {max}; only 8-bit (255) is read"));
    }
    let want = w * h * 3;
    let rgb = d
        .get(at..at + want)
        .ok_or("PPM pixel data is short")?
        .to_vec();
    Ok(Image { w, h, rgb })
}

fn decode_png(d: &[u8]) -> Result<Image, String> {
    let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
    let mut at = 8;
    let (mut w, mut h, mut depth, mut colour, mut interlace) = (0usize, 0usize, 0u8, 0u8, 0u8);
    let mut idat: Vec<u8> = Vec::new();
    let mut palette: Vec<u8> = Vec::new();
    while at + 8 <= d.len() {
        let len = be(&d[at..at + 4]);
        let kind = &d[at + 4..at + 8];
        let body = d.get(at + 8..at + 8 + len).ok_or("truncated PNG chunk")?;
        match kind {
            b"IHDR" => {
                w = be(&body[0..4]);
                h = be(&body[4..8]);
                depth = body[8];
                colour = body[9];
                interlace = body[12];
            }
            b"PLTE" => palette = body.to_vec(),
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    if w == 0 || h == 0 {
        return Err("PNG has no IHDR".into());
    }
    if depth != 8 {
        return Err(format!(
            "this PNG is {depth} bits per channel; only 8 is read. Re-export as 8-bit."
        ));
    }
    if interlace != 0 {
        return Err("this PNG is interlaced (Adam7); only non-interlaced is read.".into());
    }
    // 0 grey, 2 RGB, 3 palette, 4 grey+alpha, 6 RGBA.
    let channels = match colour {
        0 => 1,
        2 => 3,
        3 => 1,
        4 => 2,
        6 => 4,
        other => return Err(format!("PNG colour type {other} is not read")),
    };
    if colour == 3 && palette.is_empty() {
        return Err("PNG is palette-coloured but carries no PLTE".into());
    }
    // zlib: two header bytes, then the deflate stream.
    let raw = inflate(
        idat.get(2..).ok_or("PNG IDAT is empty")?,
        h * (1 + w * channels),
    )?;

    // Un-filter, per PNG's five filter types.
    let stride = w * channels;
    let mut out = vec![0u8; h * stride];
    for y in 0..h {
        let base = y * (stride + 1);
        let ft = *raw.get(base).ok_or("PNG data is short")?;
        let line = raw
            .get(base + 1..base + 1 + stride)
            .ok_or("PNG data is short")?;
        for x in 0..stride {
            let a = if x >= channels {
                out[y * stride + x - channels]
            } else {
                0
            } as i32;
            let b = if y > 0 { out[(y - 1) * stride + x] } else { 0 } as i32;
            let c = if x >= channels && y > 0 {
                out[(y - 1) * stride + x - channels]
            } else {
                0
            } as i32;
            let v = line[x] as i32;
            out[y * stride + x] = match ft {
                0 => v,
                1 => v + a,
                2 => v + b,
                3 => v + (a + b) / 2,
                4 => {
                    // Paeth.
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    v + if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
                other => return Err(format!("PNG filter type {other} is not valid")),
            } as u8;
        }
    }

    // To RGB, flattening alpha onto nothing in particular — the splash composites onto its own
    // background later, so a transparent pixel becomes the background there.
    let mut rgb = vec![0u8; w * h * 3];
    for i in 0..w * h {
        let s = &out[i * channels..i * channels + channels];
        let (r, g, b) = match colour {
            0 | 4 => (s[0], s[0], s[0]),
            2 | 6 => (s[0], s[1], s[2]),
            3 => {
                let o = s[0] as usize * 3;
                let p = palette
                    .get(o..o + 3)
                    .ok_or("PNG palette index out of range")?;
                (p[0], p[1], p[2])
            }
            _ => unreachable!("colour type was checked above"),
        };
        rgb[i * 3] = r;
        rgb[i * 3 + 1] = g;
        rgb[i * 3 + 2] = b;
    }
    Ok(Image { w, h, rgb })
}

/// Fit an image into `bw`×`bh`, preserving its aspect ratio, and return RGB565.
///
/// **Aspect is preserved and the rest is left transparent to the caller**: the returned mask says
/// which pixels the image actually covers, so a portrait logo in a landscape box does not get
/// stretched and does not paint a black bar over the background either.
///
/// Downscaling is a box filter — every source pixel that lands in a destination pixel is averaged,
/// rather than one being sampled and the rest thrown away. At these sizes (a 1024-wide export into
/// 62 pixels) point sampling produces exactly the jagged, aliased result that makes a drawn mark
/// look wrong next to Apple's, which is smooth.
pub fn fit(img: &Image, bw: usize, bh: usize) -> (Vec<u16>, Vec<bool>) {
    let mut px = vec![0u16; bw * bh];
    let mut mask = vec![false; bw * bh];
    if img.w == 0 || img.h == 0 {
        return (px, mask);
    }
    // The largest scale that fits both ways.
    let s = (bw as f32 / img.w as f32).min(bh as f32 / img.h as f32);
    let (dw, dh) = (
        ((img.w as f32 * s).round() as usize).max(1),
        ((img.h as f32 * s).round() as usize).max(1),
    );
    let (ox, oy) = ((bw - dw.min(bw)) / 2, (bh - dh.min(bh)) / 2);

    for dy in 0..dh.min(bh) {
        for dx in 0..dw.min(bw) {
            // The source rectangle this destination pixel covers.
            let x0 = dx * img.w / dw;
            let x1 = (((dx + 1) * img.w).div_ceil(dw)).min(img.w).max(x0 + 1);
            let y0 = dy * img.h / dh;
            let y1 = (((dy + 1) * img.h).div_ceil(dh)).min(img.h).max(y0 + 1);
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let o = (sy * img.w + sx) * 3;
                    r += img.rgb[o] as u32;
                    g += img.rgb[o + 1] as u32;
                    b += img.rgb[o + 2] as u32;
                    n += 1;
                }
            }
            let (r, g, b) = ((r / n) as u16, (g / n) as u16, (b / n) as u16);
            let i = (oy + dy) * bw + ox + dx;
            px[i] = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
            mask[i] = true;
        }
    }
    (px, mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ppm(w: usize, h: usize, f: impl Fn(usize, usize) -> (u8, u8, u8)) -> Vec<u8> {
        let mut d = format!("P6\n{w} {h}\n255\n").into_bytes();
        for y in 0..h {
            for x in 0..w {
                let (r, g, b) = f(x, y);
                d.extend_from_slice(&[r, g, b]);
            }
        }
        d
    }

    #[test]
    fn a_ppm_round_trips() {
        let d = ppm(4, 2, |x, _| (x as u8 * 10, 1, 2));
        let img = decode(&d).expect("decodes");
        assert_eq!((img.w, img.h), (4, 2));
        assert_eq!(&img.rgb[..6], &[0, 1, 2, 10, 1, 2]);
    }

    /// **Aspect is preserved and the image is centred.** A wide image in a tall box must not be
    /// stretched to fill it — that is what makes a supplied logo look wrong.
    #[test]
    fn fitting_preserves_aspect_and_centres() {
        // 100x50 — twice as wide as tall — into the 62x78 box Apple's logo occupies.
        let img = decode(&ppm(100, 50, |_, _| (255, 255, 255))).expect("decodes");
        let (px, mask) = fit(&img, 62, 78);
        assert_eq!(px.len(), 62 * 78);
        // It fits by width, so 62 wide and 31 tall, centred vertically.
        let rows: Vec<usize> = (0..78)
            .filter(|&y| (0..62).any(|x| mask[y * 62 + x]))
            .collect();
        assert_eq!(rows.len(), 31, "a 2:1 image in a 62x78 box is 31 rows");
        assert!(
            rows[0] >= 22 && rows[0] <= 24,
            "centred vertically, got row {}",
            rows[0]
        );
        // Every covered pixel is white; nothing outside is claimed.
        assert!(mask.iter().zip(&px).all(|(&m, &p)| !m || p == 0xffff));
    }

    /// Downscaling **averages** rather than samples. A checkerboard reduced by point sampling comes
    /// out as one flat colour or as noise; averaged, it comes out grey — which is what stops a
    /// drawn mark looking jagged beside Apple's smooth one.
    #[test]
    fn downscaling_averages_instead_of_sampling() {
        let img = decode(&ppm(62 * 4, 78 * 4, |x, y| {
            if (x + y) % 2 == 0 {
                (255, 255, 255)
            } else {
                (0, 0, 0)
            }
        }))
        .expect("decodes");
        let (px, _) = fit(&img, 62, 78);
        // Every pixel should be mid-grey, not black or white.
        for (i, &p) in px.iter().enumerate() {
            let r = (p >> 11) & 0x1f;
            assert!(
                r > 5 && r < 26,
                "pixel {i} is {r}/31 — that is a sample, not an average"
            );
        }
    }

    /// Refusals name the reason rather than producing a bad picture.
    #[test]
    fn unreadable_images_are_refused_with_a_reason() {
        assert!(decode(b"not an image").unwrap_err().contains("PNG"));
        // A PNG that is 16-bit says so.
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&8u32.to_be_bytes());
        png.extend_from_slice(&8u32.to_be_bytes());
        png.extend_from_slice(&[16, 2, 0, 0, 0]);
        png.extend_from_slice(&[0; 4]);
        let e = decode(&png).unwrap_err();
        assert!(e.contains("16 bits"), "{e}");
    }
}
