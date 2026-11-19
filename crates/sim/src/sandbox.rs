use crate::water::{Cell, WaterError};
use crate::{Command, SimError, SimState};

pub const STEP_DAYS: f32 = 1.0 / 144.0;

impl SimState {
    pub fn is_running(&self) -> bool {
        self.sandbox || self.colony.is_founded()
    }

    pub fn sandbox(seed: u64, world: crate::worldgen::World) -> Self {
        let size = world.size;
        let mut s = Self::from_world(seed, world);
        s.sandbox = true;
        s.fog = crate::fog::Fog::clear(size);
        s
    }
}

pub fn on_map(id: &str) -> Result<SimState, String> {
    if id == "fixture" {
        return Ok(SimState::sandbox(1, crate::worldgen::fixture(1)));
    }
    let (info, world) = crate::maps::world(id).map_err(|e| format!("map {id}: {e}"))?;
    Ok(SimState::sandbox(info.seed(), world))
}

pub(crate) fn apply(s: &mut SimState, cmd: Command) -> Result<(), SimError> {
    let bad = |e: WaterError| SimError::InvalidCommand(e.to_string());
    match cmd {
        Command::BuildDam { cells, height } | Command::OrderDam { cells, height } => s.water.build_dam(&cells, height).map_err(bad),
        Command::OrderDamTo { cells, crest } => {
            let (cells, heights) = crate::dam_preview::heights_to(s, &cells, crest);
            if cells.is_empty() {
                return Err(SimError::InvalidCommand("the ground is already that high".into()));
            }
            let pairs: Vec<(Cell, f32)> = cells.into_iter().zip(heights).collect();
            s.water.raise_dam(&pairs).map_err(bad)
        }
        Command::BreachDam { x, y } => s.water.breach(x, y).map_err(bad),
        Command::DigCanal { cells, depth } => s.water.dig_canal(&mut s.height, &cells, depth).map_err(bad),
        Command::OrderCanal { cells } => s.water.dig_canal(&mut s.height, &cells, SimState::CANAL_DEPTH).map_err(bad),
        other => Err(SimError::InvalidCommand(format!("sandbox has no colony: '{}' is not available", crate::script::to_line(&other)))),
    }
}

pub fn run_days(s: &mut SimState, days: f64) {
    let end = s.time_days + days;
    while s.time_days < end - 1e-9 {
        let dt = STEP_DAYS.min((end - s.time_days) as f32);
        s.step(dt).expect("sandbox step");
    }
}

pub fn line(points: &[Cell]) -> Vec<Cell> {
    let mut out: Vec<Cell> = Vec::new();
    for w in points.windows(2) {
        let (mut x, mut y) = (w[0].0 as i64, w[0].1 as i64);
        let (x1, y1) = (w[1].0 as i64, w[1].1 as i64);
        let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
        let (sx, sy) = ((x1 - x).signum(), (y1 - y).signum());
        let mut err = dx + dy;
        loop {
            let c = (x as usize, y as usize);
            if out.last() != Some(&c) {
                out.push(c);
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
    if points.len() == 1 {
        out.push(points[0]);
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Probe {
    pub ground: f32,
    pub dam: f32,
    pub depth: f32,
    pub surface: f32,
    pub width: f32,
    pub discharge: f32,
    pub canal: bool,
}

pub fn probe(s: &SimState, c: Cell) -> Option<Probe> {
    let w = &s.water;
    let ground = *s.height.get(c.0, c.1)?;
    let dam = *w.dam_height.get(c.0, c.1)?;
    let depth = *w.depth.get(c.0, c.1)?;
    Some(Probe {
        ground,
        dam,
        depth,
        surface: ground + dam + depth,
        width: *w.width.get(c.0, c.1)?,
        discharge: *w.discharge.get(c.0, c.1)?,
        canal: *w.canal.get(c.0, c.1)?,
    })
}

pub fn dump(s: &SimState) -> Vec<u8> {
    let w = &s.water;
    let mut out = (s.world.size as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&s.water.cell_m.to_le_bytes());
    let river: Vec<f32> = s.world.river.as_slice().iter().map(|&r| if r { 1.0 } else { 0.0 }).collect();
    let flag = |g: &crate::Grid<bool>| -> Vec<f32> { g.as_slice().iter().map(|&r| if r { 1.0 } else { 0.0 }).collect() };
    let (canal, channel) = (flag(&w.canal), flag(&w.channel));
    let bank: Vec<f32> = (0..w.depth.as_slice().len()).map(|i| w.bank_depth(i)).collect();
    for layer in [s.height.as_slice(), w.dam_height.as_slice(), w.depth.as_slice(), w.width.as_slice(), w.discharge.as_slice(), &river, &canal, &channel, &bank] {
        for v in layer {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

pub struct Books {
    start: f64,
}

impl Books {
    pub fn open(s: &SimState) -> Books {
        Books { start: s.water.total_volume() - (s.water.added_volume - s.water.lost_volume) }
    }
    pub fn error(&self, s: &SimState) -> f64 {
        let w = &s.water;
        let expect = self.start + w.added_volume - w.lost_volume;
        (w.total_volume() - expect).abs() / w.added_volume.max(1.0)
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub map: String,
    pub lines: Vec<String>,
    pub checks: Vec<(String, bool)>,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|c| c.1)
    }
}

fn cell(t: &str) -> Result<Cell, String> {
    let (a, b) = t.split_once(',').ok_or_else(|| format!("bad cell '{t}' (want x,y)"))?;
    Ok((a.trim().parse().map_err(|_| format!("bad cell '{t}'"))?, b.trim().parse().map_err(|_| format!("bad cell '{t}'"))?))
}

fn number(t: Option<&&str>, what: &str) -> Result<f32, String> {
    let t = t.ok_or_else(|| format!("missing {what}"))?;
    t.parse().map_err(|_| format!("bad {what} '{t}'"))
}

fn points(w: &[&str]) -> Result<Vec<Cell>, String> {
    if w.is_empty() {
        return Err("missing cells".into());
    }
    w.iter().map(|t| cell(t)).collect()
}

pub fn quantity(s: &SimState, books: &Books, what: &str, at: Option<Cell>) -> Result<f64, String> {
    if what == "balance" {
        return Ok(books.error(s));
    }
    if what == "day" {
        return Ok(s.time_days);
    }
    let c = at.ok_or_else(|| format!("'{what}' needs a cell"))?;
    let p = probe(s, c).ok_or_else(|| format!("cell {c:?} is off the map"))?;
    Ok(match what {
        "ground" => p.ground,
        "dam" => p.dam,
        "depth" => p.depth,
        "surface" => p.surface,
        "width" => p.width,
        "discharge" => p.discharge,
        other => return Err(format!("unknown quantity '{other}' (ground dam depth surface width discharge balance day)")),
    } as f64)
}

pub fn describe(s: &SimState, c: Cell) -> String {
    match probe(s, c) {
        Some(p) => format!(
            "({},{}) ground {:.2} m, dam {:.2} m, water {:.3} m deep, surface {:.2} m, wetted width {:.2} m, flow {:.4} m3/s{}",
            c.0,
            c.1,
            p.ground,
            p.dam,
            p.depth,
            p.surface,
            p.width,
            p.discharge,
            if p.canal { ", canal" } else { "" }
        ) + &s.water.downstream(c.0, c.1).map(|d| format!(", drains to ({},{})", d.0, d.1)).unwrap_or_default(),
        None => format!("({},{}) is off the map", c.0, c.1),
    }
}

pub fn run_script(text: &str) -> Result<(SimState, Report), String> {
    let mut sim: Option<(SimState, Books)> = None;
    let mut report = Report::default();
    for (n, raw) in text.lines().enumerate() {
        let line_text = raw.split('#').next().unwrap_or("").trim();
        if line_text.is_empty() {
            continue;
        }
        let err = |e: String| format!("line {}: {e}", n + 1);
        let w: Vec<&str> = line_text.split_whitespace().collect();
        let Some((s, books)) = sim.as_mut() else {
            match w.as_slice() {
                ["map", id] => {
                    let s = on_map(id).map_err(err)?;
                    let books = Books::open(&s);
                    report.map = id.to_string();
                    sim = Some((s, books));
                    continue;
                }
                _ => return Err(err("a sandbox script starts with 'map ID'".into())),
            }
        };
        let say = |s: &SimState, msg: String| format!("day {:>6.2}: {msg}", s.time_days);
        match w[0] {
            "dam" | "dam-to" | "canal" => {
                let v = number(w.get(1), if w[0] == "canal" { "depth" } else if w[0] == "dam" { "height" } else { "crest" }).map_err(err)?;
                let cells = line(&points(&w[2..]).map_err(err)?);
                let cmd = match w[0] {
                    "dam" => Command::BuildDam { cells: cells.clone(), height: v },
                    "dam-to" => Command::OrderDamTo { cells: cells.clone(), crest: v },
                    _ => Command::DigCanal { cells: cells.clone(), depth: v },
                };
                s.apply(cmd).map_err(|e| err(e.to_string()))?;
                report.lines.push(say(s, format!("{} of {} cells ({v} m)", w[0], cells.len())));
            }
            "breach" => {
                let cells = line(&points(&w[1..]).map_err(err)?);
                let dams: Vec<Cell> = cells.into_iter().filter(|c| s.water.has_dam(c.0, c.1)).collect();
                if dams.is_empty() {
                    return Err(err("no dam there to breach".into()));
                }
                for c in &dams {
                    s.apply(Command::BreachDam { x: c.0, y: c.1 }).map_err(|e| err(e.to_string()))?;
                }
                report.lines.push(say(s, format!("breached {} dam cells", dams.len())));
            }
            "run" => {
                let d: f64 = number(w.get(1), "days").map_err(err)? as f64;
                let t = std::time::Instant::now();
                run_days(s, d);
                report.lines.push(say(s, format!("ran {d} days in {:.1} s", t.elapsed().as_secs_f32())));
            }
            "probe" => {
                let c = cell(w.get(1).ok_or_else(|| err("missing cell".into()))?).map_err(err)?;
                let label = w.get(2..).map(|r| r.join(" ")).unwrap_or_default();
                report.lines.push(say(s, format!("{label} {}", describe(s, c)).trim().to_string()));
            }
            "check" => {
                let (what, at, rest) = match w.get(2).map(|t| t.contains(',')) {
                    Some(true) => (w[1], Some(cell(w[2]).map_err(err)?), &w[3..]),
                    _ => (*w.get(1).ok_or_else(|| err("missing quantity".into()))?, None, w.get(2..).unwrap_or(&[])),
                };
                let (op, want) = match rest {
                    [op, v] => (*op, v.parse::<f64>().map_err(|_| err(format!("bad value '{v}'")))?),
                    _ => return Err(err("want 'check QUANTITY [X,Y] OP VALUE'".into())),
                };
                let have = quantity(s, books, what, at).map_err(err)?;
                let ok = match op {
                    ">" => have > want,
                    ">=" => have >= want,
                    "<" => have < want,
                    "<=" => have <= want,
                    other => return Err(err(format!("unknown operator '{other}'"))),
                };
                let name = format!("{} {op} {want} (is {have:.4})", w[1..w.len() - 2].join(" "));
                report.lines.push(say(s, format!("{} {name}", if ok { "PASS" } else { "FAIL" })));
                report.checks.push((name, ok));
            }
            "dump" => {
                let path = w.get(1).ok_or_else(|| err("missing file".into()))?;
                std::fs::write(path, dump(s)).map_err(|e| err(format!("cannot write {path}: {e}")))?;
                report.lines.push(say(s, format!("dumped the water to {path}")));
            }
            other => return Err(err(format!("unknown sandbox statement '{other}' (dam dam-to canal breach run probe check dump)"))),
        }
    }
    let (s, _) = sim.ok_or("empty script")?;
    Ok((s, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> SimState {
        on_map("fixture").unwrap()
    }

    fn stream(s: &SimState) -> Vec<Cell> {
        let n = s.world.size;
        let mut out: Vec<Cell> = (0..n * n).map(|i| (i % n, i / n)).filter(|c| s.water.discharge.get(c.0, c.1).is_some_and(|q| *q > 1e-4) && s.world.river.get(c.0, c.1) == Some(&true)).collect();
        out.sort_by_key(|c| c.0);
        out
    }

    #[test]
    fn a_sandbox_has_no_beavers_and_runs_only_water() {
        let mut s = fixture();
        run_days(&mut s, 2.0);
        assert!(s.colony.beavers.is_empty() && s.agents.by_id.is_empty());
        assert!((s.time_days - 2.0).abs() < 1e-6 && (s.water.time_days - 2.0).abs() <= STEP_DAYS as f64);
        assert!(s.apply(Command::Settle { cell: (5, 5) }).is_err());
    }

    #[test]
    fn a_placed_dam_is_there_at_once_and_ponds_the_stream_behind_it() {
        let mut s = fixture();
        let river = stream(&s);
        let mid = river[river.len() / 2];
        let dam = line(&[(mid.0, mid.1.saturating_sub(12)), (mid.0, mid.1 + 12)]);
        let up = *river.iter().rev().find(|c| c.0 + 3 == mid.0).unwrap();
        let before = probe(&s, up).unwrap();
        s.apply(Command::OrderDam { cells: dam.clone(), height: 1.5 }).unwrap();
        assert!(dam.iter().all(|c| (s.water.dam_height.get(c.0, c.1).unwrap() - 1.5).abs() < 1e-6));
        run_days(&mut s, 5.0);
        let after = probe(&s, up).unwrap();
        assert!(after.surface > before.surface + 0.3, "{before:?} -> {after:?}");
        assert!(Books::open(&fixture()).error(&fixture()) < 1e-9);
    }

    #[test]
    fn a_script_builds_runs_and_checks() {
        let s = fixture();
        let river = stream(&s);
        let mid = river[river.len() / 2];
        let up = *river.iter().rev().find(|c| c.0 + 3 == mid.0).unwrap();
        let script = format!(
            "map fixture\nprobe {},{} before\ndam 1.5 {},{} {},{}\nrun 5\ncheck depth {},{} > 0.3\ncheck balance < 0.01\nbreach {},{} {},{}\nrun 3\ncheck depth {},{} < 0.3\n",
            up.0, up.1, mid.0, mid.1 - 12, mid.0, mid.1 + 12, up.0, up.1, mid.0, mid.1 - 12, mid.0, mid.1 + 12, up.0, up.1
        );
        let (_, r) = run_script(&script).unwrap();
        assert!(r.passed(), "{:#?}", r.lines);
        assert_eq!(r.checks.len(), 3);
    }

    #[test]
    fn scripts_refuse_bad_lines() {
        assert!(run_script("dam 1 1,1 2,2").is_err());
        assert!(run_script("map nowhere").is_err());
        assert!(run_script("map fixture\nfly 3").is_err());
        assert!(run_script("map fixture\nbreach 3,3").is_err());
        assert!(run_script("map fixture\ncheck depth 3,3 ~ 1").is_err());
    }

    fn lake_world() -> SimState {
        let n = 96;
        let lake = |x: usize| x >= 72;
        let raw: Vec<f32> = (0..n * n)
            .map(|i| {
                let (x, y) = (i % n, i / n);
                if lake(x) { 8.0 } else { (10.0 - 0.02 * x as f32 + 0.1 * (y as f32 - 48.0).abs()).max(8.3) }
            })
            .collect();
        let cover: Vec<u8> = (0..n * n).map(|i| if lake(i % n) { 80 } else { 10 }).collect();
        let map = crate::worldgen::RealMap { cell_m: 1.0, inflows: &[(0, 48, 0.5)], cover: Some(&cover) };
        SimState::sandbox(1, crate::worldgen::from_heights(1, n, raw, &map).unwrap())
    }

    #[test]
    fn a_lake_keeps_its_level_takes_in_the_stream_and_fills_a_canal_dug_from_it_unless_a_dam_shuts_it() {
        let mut s = lake_world();
        let books = Books::open(&s);
        let deep = (88, 48);
        let before = probe(&s, deep).unwrap().surface;
        run_days(&mut s, 3.0);
        assert!((probe(&s, deep).unwrap().surface - before).abs() < 1e-5);
        assert!(books.error(&s) < 1e-3, "books off by {}", books.error(&s));
        let arm = line(&[(71, 44), (60, 44)]);
        assert!(probe(&s, (64, 44)).unwrap().depth == 0.0);
        s.apply(Command::DigCanal { cells: arm.clone(), depth: 1.5 }).unwrap();
        run_days(&mut s, 0.1);
        assert!(arm.iter().all(|c| (probe(&s, *c).unwrap().surface - 8.0).abs() < 1e-4), "{}", describe(&s, (60, 44)));
        assert!(books.error(&s) < 1e-3);
        let mut t = lake_world();
        t.apply(Command::BuildDam { cells: line(&[(72, 41), (72, 47)]), height: 2.0 }).unwrap();
        t.apply(Command::DigCanal { cells: arm, depth: 1.5 }).unwrap();
        run_days(&mut t, 0.1);
        assert!(probe(&t, (64, 44)).unwrap().depth < 0.05, "{}", describe(&t, (64, 44)));
    }

    #[test]
    fn a_stream_entering_a_new_pond_stays_in_its_banks() {
        let mut s = fixture();
        let river = stream(&s);
        let mid = river[river.len() / 2];
        s.apply(Command::OrderDam { cells: line(&[(mid.0, mid.1 - 20), (mid.0, mid.1 + 20)]), height: 1.0 }).unwrap();
        run_days(&mut s, 0.5);
        let n = s.world.size;
        for i in 0..n * n {
            let c = (i % n, i / n);
            let p = probe(&s, c).unwrap();
            if s.world.river.as_slice()[i] && p.discharge > 1e-4 && p.depth > 0.0 {
                assert!(s.water.channel.as_slice()[i] && p.depth <= s.water.bank_depth(i) + 1e-4, "{}", describe(&s, c));
            }
        }
    }

    #[test]
    fn a_line_touches_every_step_and_both_ends() {
        let l = line(&[(0, 0), (4, 2)]);
        assert_eq!((l[0], *l.last().unwrap()), ((0, 0), (4, 2)));
        assert!(l.windows(2).all(|w| w[0].0.abs_diff(w[1].0) <= 1 && w[0].1.abs_diff(w[1].1) <= 1));
        assert_eq!(line(&[(3, 3)]), vec![(3, 3)]);
    }
}
