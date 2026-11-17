//! Renders a generated world top-down to a PNG.
//!
//! `cargo run -p sim --example worldmap -- --seed S [--out file.png] [--scale N]`
//! Default output: `<workspace>/tmp/worldmap_seed<S>.png` (the nearest ancestor holding `AGENTS.md`),
//! falling back to the system temp dir. Exit code 2 on bad arguments, 1 on I/O or generation errors.
use sim::worldgen::{Tree, WorldGen};
use std::path::PathBuf;
use std::process::ExitCode;

type Rgb = [f32; 3];

fn main() -> ExitCode {
    match run() {
        Ok(path) => { println!("wrote {}", path.display()); ExitCode::SUCCESS }
        Err((code, msg)) => { eprintln!("worldmap: {msg}"); ExitCode::from(code) }
    }
}

fn run() -> Result<PathBuf, (u8, String)> {
    let (mut seed, mut out, mut scale) = (1u64, None, 6usize);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = |name: &str| args.next().ok_or((2, format!("{name} needs a value")));
        match a.as_str() {
            "--seed" => seed = val("--seed")?.parse().map_err(|e| (2, format!("bad --seed: {e}")))?,
            "--out" => out = Some(PathBuf::from(val("--out")?)),
            "--scale" => scale = val("--scale")?.parse().map_err(|e| (2, format!("bad --scale: {e}")))?,
            _ => return Err((2, format!("unknown argument {a} (use --seed S [--out F] [--scale N])"))),
        }
    }
    if !(1..=16).contains(&scale) { return Err((2, "--scale must be 1..=16".into())); }
    let out = out.unwrap_or_else(|| default_dir().join(format!("worldmap_seed{seed}.png")));
    let world = WorldGen::new(seed).generate().map_err(|e| (1, e.to_string()))?;
    let (w, h, rgb) = render(&world, scale);
    let png = encode_png(w, h, &rgb);
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| (1, format!("create {}: {e}", dir.display())))?;
    }
    std::fs::write(&out, png).map_err(|e| (1, format!("write {}: {e}", out.display())))?;
    Ok(out)
}

fn default_dir() -> PathBuf {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors().find(|d| d.join("AGENTS.md").is_file()).map(|d| d.join("tmp")).unwrap_or_else(std::env::temp_dir)
}

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb { [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t.clamp(0.0, 1.0)) }

fn tree_color(t: Tree) -> Rgb {
    match t {
        Tree::Aspen => [200.0, 205.0, 40.0],
        Tree::Willow => [90.0, 200.0, 140.0],
        Tree::Birch => [235.0, 230.0, 140.0],
        Tree::Alder => [30.0, 140.0, 50.0],
        Tree::Pine => [10.0, 60.0, 40.0],
    }
}

fn render(world: &sim::worldgen::World, s: usize) -> (usize, usize, Vec<u8>) {
    let n = world.size;
    let amp = world.surface.as_slice().iter().cloned().fold(0.0, f32::max).max(1.0);
    let sh = |x: isize, y: isize| *world.surface.get(x.clamp(0, n as isize - 1) as usize, y.clamp(0, n as isize - 1) as usize).unwrap();
    let mut cell = vec![[0.0f32; 3]; n * n];
    for y in 0..n {
        for x in 0..n {
            let (xi, yi) = (x as isize, y as isize);
            let hf = sh(xi, yi) / amp;
            let ground = lerp(lerp([176.0, 160.0, 105.0], [150.0, 120.0, 85.0], hf * 2.0), [225.0, 225.0, 225.0], (hf - 0.6) * 3.0);
            // Hillshade, light from the north-west, exaggerated so ridges read clearly.
            let (dx, dy) = ((sh(xi + 1, yi) - sh(xi - 1, yi)) / 2.0, (sh(xi, yi + 1) - sh(xi, yi - 1)) / 2.0);
            let shade = (0.95 - 0.55 * (dx + dy)).clamp(0.35, 1.45);
            let base = match world.forest.get(x, y).unwrap() {
                Some(t) => lerp(ground, tree_color(*t), 0.85),
                None => ground,
            };
            cell[y * n + x] = base.map(|c| c * shade);
        }
    }
    let wpx = n * s;
    let mut img = vec![0u8; wpx * wpx * 3];
    let mut put = |px: usize, py: usize, c: Rgb| {
        if px < wpx && py < wpx { for k in 0..3 { img[(py * wpx + px) * 3 + k] = c[k].clamp(0.0, 255.0) as u8; } }
    };
    for py in 0..wpx { for px in 0..wpx { put(px, py, cell[(py / s) * n + px / s]); } }
    let lake_c = [25.0, 70.0, 175.0];
    for y in 0..n { for x in 0..n {
        if *world.lake.get(x, y).unwrap() {
            for py in y * s..(y + 1) * s { for px in x * s..(x + 1) * s { put(px, py, lake_c); } }
        }
    } }
    // Rivers: discs along each river cell, wider with more upstream area.
    let river_c = [40.0, 130.0, 245.0];
    for y in 0..n { for x in 0..n {
        if !*world.river.get(x, y).unwrap() { continue; }
        let acc = *world.flow_acc.get(x, y).unwrap() as f32;
        let r = s as f32 * (0.45 + 0.12 * (acc / sim::worldgen::RIVER_MIN_ACCUMULATION as f32).log2().max(0.0)).min(1.4);
        let (cx, cy) = ((x * s + s / 2) as f32, (y * s + s / 2) as f32);
        let ri = r.ceil() as isize;
        for oy in -ri..=ri { for ox in -ri..=ri {
            if ((ox * ox + oy * oy) as f32) <= r * r {
                put((cx as isize + ox).max(0) as usize, (cy as isize + oy).max(0) as usize, river_c);
            }
        } }
    } }
    // Valley borders as thin dark lines; start valley border is brighter; springs as white dots.
    for y in 0..n { for x in 0..n {
        let v = world.valley.get(x, y).unwrap();
        for (nx, ny) in [(x + 1, y), (x, y + 1)] {
            if let Some(o) = world.valley.get(nx, ny) {
                if o != v {
                    let c = if *v == world.start || *o == world.start { [255.0, 80.0, 60.0] } else { [60.0, 20.0, 20.0] };
                    let (bx, by) = ((nx * s).min(wpx - 1), (ny * s).min(wpx - 1));
                    if nx != x { for py in y * s..(y + 1) * s { put(bx, py, c); } }
                    else { for px in x * s..(x + 1) * s { put(px, by, c); } }
                }
            }
        }
    } }
    for &(x, y) in &world.springs {
        for oy in 0..s.min(3) { for ox in 0..s.min(3) { put(x * s + s / 2 + ox, y * s + s / 2 + oy, [255.0, 255.0, 255.0]); } }
    }
    (wpx, wpx, img)
}

// ---- minimal PNG writer (RGB8, stored deflate blocks) ----

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 { c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 }; }
    }
    !c
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend(data);
    out.extend(&body);
    out.extend(crc32(&body).to_be_bytes());
}

fn encode_png(w: usize, h: usize, rgb: &[u8]) -> Vec<u8> {
    assert_eq!(rgb.len(), w * h * 3);
    let mut raw = Vec::with_capacity(h * (w * 3 + 1));
    for row in rgb.chunks(w * 3) { raw.push(0); raw.extend(row); }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push((i + 1 == blocks.len()) as u8);
        z.extend((b.len() as u16).to_le_bytes());
        z.extend((!(b.len() as u16)).to_le_bytes());
        z.extend(*b);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw { a = (a + x as u32) % 65521; b = (b + a) % 65521; }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend((w as u32).to_be_bytes());
    ihdr.extend((h as u32).to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}
