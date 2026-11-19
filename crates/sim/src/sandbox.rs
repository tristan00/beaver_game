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

/// A canal needs face-connected cells: a diagonal-only raster leaves a series of
/// separate pits in a solver that exchanges water across cardinal faces.
pub fn canal_line(points: &[Cell]) -> Vec<Cell> {
    let path = line(points);
    let mut out = Vec::with_capacity(path.len() * 2);
    for (i, &c) in path.iter().enumerate() {
        if i > 0 {
            let prev = path[i - 1];
            if prev.0 != c.0 && prev.1 != c.1 {
                out.push((c.0, prev.1));
            }
        }
        out.push(c);
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(*c));
    out
}

pub fn set_season(s: &mut SimState, season: crate::calendar::Season) {
    let year = (s.time_days / crate::calendar::YEAR_DAYS).floor();
    let mut next = year * crate::calendar::YEAR_DAYS + (season as usize as f64 + 0.5) * crate::calendar::SEASON_DAYS;
    if next < s.time_days { next += crate::calendar::YEAR_DAYS; }
    s.time_days = next;
    s.water.time_days = s.time_days;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagonal_canals_are_face_connected_in_both_directions() {
        for (a, b) in [((2, 2), (12, 8)), ((12, 8), (2, 2)), ((3, 12), (12, 3))] {
            let cells = canal_line(&[a, b]);
            assert_eq!(cells.first(), Some(&a));
            assert_eq!(cells.last(), Some(&b));
            assert!(cells.windows(2).all(|p| p[0].0.abs_diff(p[1].0) + p[0].1.abs_diff(p[1].1) == 1));
        }
    }

    #[test]
    fn crossing_canals_excavate_each_cell_only_once() {
        let cells = canal_line(&[(2, 2), (8, 8), (8, 2), (2, 8)]);
        let unique: std::collections::HashSet<_> = cells.iter().collect();
        assert_eq!(cells.len(), unique.len());
    }
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
    let bank: Vec<f32> = w.velocity.as_slice().iter().map(|v| (v.0 * v.0 + v.1 * v.1).sqrt()).collect();
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
    match what {
        "rain" => return Ok(s.water.forcing().rain as f64 * 1000.0),
        "inflow" => return Ok(s.water.forcing().inflow as f64),
        "volume" => return Ok(s.water.total_volume()),
        _ => {}
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
        "bed" => *s.water.riverbed.get(c.0, c.1).unwrap(),
        "ice" => *s.water.ice.get(c.0, c.1).unwrap(),
        "speed" => {
            let (u, v) = *s.water.velocity.get(c.0, c.1).unwrap();
            u.hypot(v)
        }
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
            "dam" | "dam-to" | "canal" | "canal-to" => {
                let v = number(w.get(1), if w[0] == "canal" { "depth" } else if w[0] == "dam" { "height" } else { "crest" }).map_err(err)?;
                let pts = points(&w[2..]).map_err(err)?;
                if pts.iter().any(|&(x, y)| !s.height.in_bounds(x, y)) {
                    return Err(err("line endpoint is outside the map".into()));
                }
                let cells = if w[0].starts_with("canal") { canal_line(&pts) } else { line(&pts) };
                if w[0] == "canal-to" {
                    if !v.is_finite() { return Err(err("canal bed elevation must be finite".into())); }
                    for &(x, y) in &cells {
                        let depth = (*s.height.get(x, y).unwrap() - v).max(0.0);
                        if depth > 0.0 { s.water.dig_canal(&mut s.height, &[(x, y)], depth).map_err(|e| err(e.to_string()))?; }
                    }
                    report.lines.push(say(s, format!("canal-to of {} cells (bed {v} m)", cells.len())));
                    continue;
                }
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
                if !d.is_finite() || d <= 0.0 { return Err(err("days must be finite and positive".into())); }
                let t = std::time::Instant::now();
                run_days(s, d);
                report.lines.push(say(s, format!("ran {d} days in {:.1} s ({} physics steps so far)", t.elapsed().as_secs_f32(), s.water.steps_taken)));
            }
            "weather" => {
                let name = w.get(1).ok_or_else(|| err("missing weather".into()))?;
                s.water.weather = crate::water::Weather::parse(name).ok_or_else(|| err("weather: seasonal clear rain storm drought".into()))?;
                report.lines.push(say(s, format!("weather {}", s.water.weather.name())));
            }
            "season" => {
                let season = match w.get(1).copied() {
                    Some("spring") => crate::calendar::Season::Spring,
                    Some("summer") => crate::calendar::Season::Summer,
                    Some("autumn") => crate::calendar::Season::Autumn,
                    Some("winter") => crate::calendar::Season::Winter,
                    _ => return Err(err("season: spring summer autumn winter".into())),
                };
                set_season(s, season);
                report.lines.push(say(s, format!("season {}", season.name())));
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
            other => return Err(err(format!("unknown sandbox statement '{other}' (dam dam-to canal breach run weather season probe check dump)"))),
        }
    }
    let (s, _) = sim.ok_or("empty script")?;
    Ok((s, report))
}

