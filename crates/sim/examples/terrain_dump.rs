//! Writes a generated world as one JSON file for `tools/terrain_metrics.py` (development aid).
//!
//! `cargo run -p sim --example terrain_dump -- --seed S [--out file.json]`
//! Default output: `<workspace>/tmp/terrain_seed<S>.json`. Exit 2 on bad arguments, 1 on I/O or generation errors.
//! Grids are row-major (`y * size + x`). `forest`: 0 none, 1..=5 = Aspen, Willow, Birch, Alder, Pine.
use sim::worldgen::{Tree, WorldGen};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(p) => { println!("wrote {}", p.display()); ExitCode::SUCCESS }
        Err((c, m)) => { eprintln!("terrain_dump: {m}"); ExitCode::from(c) }
    }
}

fn arr<T: Copy>(name: &str, out: &mut String, v: impl Iterator<Item = T>, f: impl Fn(T) -> String) {
    let _ = write!(out, "\"{name}\":[");
    for (i, x) in v.enumerate() { if i > 0 { out.push(','); } out.push_str(&f(x)); }
    out.push(']');
}

fn run() -> Result<PathBuf, (u8, String)> {
    let (mut seed, mut out) = (1u64, None);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = |n: &str| args.next().ok_or((2, format!("{n} needs a value")));
        match a.as_str() {
            "--seed" => seed = val("--seed")?.parse().map_err(|e| (2, format!("bad --seed: {e}")))?,
            "--out" => out = Some(PathBuf::from(val("--out")?)),
            _ => return Err((2, format!("unknown argument {a} (use --seed S [--out F])"))),
        }
    }
    let out = out.unwrap_or_else(|| {
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        here.ancestors().find(|d| d.join("AGENTS.md").is_file()).map(|d| d.join("tmp")).unwrap_or_else(std::env::temp_dir)
            .join(format!("terrain_seed{seed}.json"))
    });
    let w = WorldGen::new(seed).generate().map_err(|e| (1, e.to_string()))?;
    let mut s = format!("{{\"seed\":{},\"size\":{},\"start_valley\":{},", w.seed, w.size, w.start.0);
    let b = |v: bool| (v as u8).to_string();
    let f = |v: f32| format!("{v:.4}");
    arr("height", &mut s, w.height.as_slice().iter().copied(), f); s.push(',');
    arr("surface", &mut s, w.surface.as_slice().iter().copied(), f); s.push(',');
    arr("slope", &mut s, w.slope.as_slice().iter().copied(), f); s.push(',');
    arr("river", &mut s, w.river.as_slice().iter().copied(), b); s.push(',');
    arr("main_river", &mut s, w.main_river.as_slice().iter().copied(), b); s.push(',');
    arr("lake", &mut s, w.lake.as_slice().iter().copied(), b); s.push(',');
    arr("pond", &mut s, w.pond.as_slice().iter().copied(), b); s.push(',');
    arr("valley", &mut s, w.valley.as_slice().iter().copied(), |v| v.0.to_string()); s.push(',');
    arr("flow_to", &mut s, w.flow_to.as_slice().iter().copied(), |v| v.map_or("-1".into(), |i| i.to_string())); s.push(',');
    arr("forest", &mut s, w.forest.as_slice().iter().copied(), |t: Option<Tree>| {
        t.map_or(0, |t| Tree::ALL.iter().position(|&x| x == t).unwrap() + 1).to_string()
    });
    s.push('}');
    if let Some(d) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(d).map_err(|e| (1, format!("create {}: {e}", d.display())))?;
    }
    std::fs::write(&out, s).map_err(|e| (1, format!("write {}: {e}", out.display())))?;
    Ok(out)
}
