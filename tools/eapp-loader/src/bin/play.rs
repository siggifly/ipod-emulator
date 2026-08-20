//! Run an iPod game in a window.
//!
//!   play <game.bin> [--gamedir=DIR] [--scale=N] [--fps=N]
//!
//! Everything the offline `trace` tool established is wired up here: context arguments to the
//! frame vectors, the manifest texture pre-load, the allocator, the clock, and file I/O. The
//! difference is only that frames go to a window instead of a `.ppm`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eapp_loader::{EApp, Machine, Stop, Stub, FB_HEIGHT, FB_WIDTH};
use minifb::{Key, Scale, Window, WindowOptions};

const RAM_BASE: u32 = 0x1100_0000;
const RAM_SIZE: usize = 0x0080_0000;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: play <game.bin> [--gamedir=DIR] [--scale=N] [--fps=N] [--budget=N]");
        std::process::exit(2);
    };
    let opt = |k: &str, d: usize| -> usize {
        args.iter()
            .find_map(|a| a.strip_prefix(k))
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };

    let image = fs::read(path).unwrap_or_else(|e| {
        eprintln!("{path}: {e}");
        std::process::exit(1);
    });
    let app = EApp::parse(image).unwrap_or_else(|e| {
        eprintln!("not loadable: {e:?}");
        std::process::exit(1);
    });

    let mut m = Machine::new(&app, RAM_BASE, RAM_SIZE);

    // Identified framework entry points — see README §"The GL surface actually in use".
    m.set_stub("miscTBD", 0, Stub::Alloc);
    m.set_stub("miscTBD", 1, Stub::Free { arg: 0 });
    m.set_stub(
        "miscTBD",
        9,
        Stub::Clock {
            arg: 0,
            step: 16_667,
        },
    );
    m.set_stub("OpenGLES", 12, Stub::GlClear);
    m.set_stub("OpenGLES", 13, Stub::GlClearColor);
    m.set_stub("OpenGLES", 157, Stub::GlSwap);
    m.set_stub("OpenGLES", 137, Stub::GlVertexAttribPointer);
    m.set_stub("OpenGLES", 37, Stub::GlDrawArrays);
    m.set_stub("OpenGLES", 4, Stub::GlBindTexture);
    m.set_stub("OpenGLES", 19, Stub::GlCompressedTexImage2D);
    m.set_stub("Audio", 52, Stub::Value(1)); // a divisor; any non-zero avoids a divide-by-zero
    m.set_stub("Filesytem", 0, Stub::FileOpen { path: 1, out: 3 });
    m.set_stub("AsyncFileIO", 0, Stub::FileOpen { path: 1, out: 3 });
    m.set_stub("AsyncFileIO", 3, Stub::FileOpen { path: 1, out: 2 });
    let rd = Stub::FileRead {
        handle: 0,
        buffer: 1,
        length: 2,
        out: 3,
    };
    m.set_stub("Filesytem", 2, rd.clone());
    m.set_stub("AsyncFileIO", 2, rd);

    // Resources default to the directory two levels above the executable — the layout every
    // title ships as `<Game>/Executables/<name>.bin`.
    m.game_dir = args
        .iter()
        .find_map(|a| a.strip_prefix("--gamedir="))
        .map(PathBuf::from)
        .or_else(|| {
            PathBuf::from(path)
                .parent()?
                .parent()
                .map(|p| p.to_path_buf())
        });
    if let Some(d) = &m.game_dir {
        println!("resources: {}", d.display());
    }
    for l in m.preload_textures() {
        println!("  {l}");
    }

    let budget = opt("--budget=", 8_000_000);
    let ctx = vec![m.scratch(0x400), m.scratch(0x400), 0, 0];

    // Init vectors run once; the last non-zero vector is the per-frame callback.
    let mut frame_vector = None;
    for (i, &v) in app.vectors.iter().enumerate() {
        if v == 0 {
            continue;
        }
        let stop = m.call_with(v, &ctx, budget);
        println!("vector[{i}] {v:#010x} -> {stop:?}");
        frame_vector = Some(v);
    }
    let Some(frame_vector) = frame_vector else {
        eprintln!("no entry vector");
        std::process::exit(1);
    };

    let scale = match opt("--scale=", 3) {
        1 => Scale::X1,
        2 => Scale::X2,
        4 => Scale::X4,
        8 => Scale::X8,
        _ => Scale::X4,
    };
    let title = PathBuf::from(path)
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "iPod".into());

    let mut window = Window::new(
        &format!("{title} — iPod 5G"),
        FB_WIDTH,
        FB_HEIGHT,
        WindowOptions {
            scale,
            ..WindowOptions::default()
        },
    )
    .unwrap_or_else(|e| {
        eprintln!("cannot open window: {e}");
        std::process::exit(1);
    });

    let target = Duration::from_micros(1_000_000 / opt("--fps=", 30) as u64);
    window.set_target_fps(opt("--fps=", 30));

    let mut buf = vec![0u32; FB_WIDTH * FB_HEIGHT];
    let mut frames = 0usize;
    let started = Instant::now();
    let mut last_report = Instant::now();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let stop = m.call_with(frame_vector, &ctx, budget);
        frames += 1;

        // The emulator's framebuffer is packed RGB; the window wants 0RGB words.
        for (i, px) in m.framebuffer.chunks_exact(3).enumerate() {
            buf[i] = ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | px[2] as u32;
        }
        let _ = window.update_with_buffer(&buf, FB_WIDTH, FB_HEIGHT);

        if last_report.elapsed() > Duration::from_secs(2) {
            println!(
                "frame {frames}  {:.1} fps  {} quads  {} instructions",
                frames as f64 / started.elapsed().as_secs_f64(),
                m.quads_drawn,
                m.executed
            );
            last_report = Instant::now();
        }
        if !matches!(stop, Stop::Returned) {
            println!("stopped: {stop:?} after {frames} frames");
            // Keep the window up so the last frame stays visible.
            while window.is_open() && !window.is_key_down(Key::Escape) {
                let _ = window.update_with_buffer(&buf, FB_WIDTH, FB_HEIGHT);
                std::thread::sleep(target);
            }
            break;
        }
    }
}
