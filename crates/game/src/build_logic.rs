use sim::buildings::{Amount, BuildError, BuildingKind, WET_DEPTH};
use sim::water::Cell;
use sim::{Command, SimState};

pub fn is_wet(sim: &SimState, x: usize, y: usize) -> bool {
    sim.water.depth.get(x, y).is_some_and(|&d| d >= WET_DEPTH)
        || sim.world.river.get(x, y).copied().unwrap_or(false)
        || sim.world.lake.get(x, y).copied().unwrap_or(false)
}

pub fn anchor(kind: BuildingKind, c: Cell) -> Cell {
    let (w, h) = kind.spec().footprint;
    (c.0.saturating_sub(w / 2), c.1.saturating_sub(h / 2))
}

pub fn footprint(kind: BuildingKind, pos: Cell) -> Vec<Cell> {
    sim::buildings::Buildings::footprint(kind, pos)
}

#[derive(Clone, Debug)]
pub struct SitePreview {
    pub pos: Cell,
    pub cells: Vec<Cell>,
    pub verdict: Result<(), BuildError>,
}

impl SitePreview {
    pub fn is_valid(&self) -> bool {
        self.verdict.is_ok()
    }
}

pub fn site_preview(sim: &SimState, kind: BuildingKind, hover: Cell) -> SitePreview {
    let pos = anchor(kind, hover);
    SitePreview { pos, cells: footprint(kind, pos), verdict: sim.check_site(kind, pos) }
}

pub fn path_cost(kind: BuildingKind, n: usize) -> Vec<Amount> {
    kind.spec().cost.iter().map(|&(g, a)| (g, a * n as f32)).collect()
}

pub fn check_dam(sim: &SimState, cells: &[Cell]) -> Result<(), Option<(crate::icons::Icon, String)>> {
    use crate::icons::Icon;
    if cells.is_empty() { return Err(None); }
    if cells.iter().any(|&(x, y)| !sim.water.depth.in_bounds(x, y)) { return Err(Some((Icon::Cross, "Outside the map".into()))); }
    if !sim.sandbox && !sim::dams::line_crosses_water(sim, cells) { return Err(Some((Icon::Drop, "Must cross water".into()))); }
    sim::dams::check_line(sim, cells).map_err(|m| Some((Icon::Cross, sentence(&m))))
}

pub fn line_cost(sim: &SimState, cells: &[Cell]) -> f32 {
    let h = sim::dams::first_targets(sim, cells);
    sim::dam_preview::loads_for(sim, cells, &h) as f32 * sim::dams::sticks_per_step()
}

pub fn place_building(sim: &mut SimState, kind: BuildingKind, pos: Cell) -> Result<(), String> {
    sim.check_site(kind, pos).map_err(|e| crate::hud::words::place_error(&e))?;
    sim.apply(Command::PlaceSite { kind, pos }).map_err(|e| crate::hud::words::sim_error(&e.to_string()))
}

#[derive(Clone, Debug)]
pub struct LinePlan {
    pub line: Vec<Cell>,
    pub targets: Vec<(Cell, f32)>,
    pub preview: Option<sim::dam_preview::DamPreview>,
    pub sticks: f32,
    pub refusal: Option<(crate::icons::Icon, String)>,
    pub settle_cells: Vec<Cell>,
    pub settle_heights: Vec<f32>,
}

fn sentence(m: &str) -> String {
    let mut c = m.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

pub fn line_plan(sim: &SimState, line: &[Cell], extending: Option<sim::buildings::BuildingId>) -> LinePlan {
    use crate::icons::Icon;
    let mut out = LinePlan { line: line.to_vec(), targets: Vec::new(), preview: None, sticks: 0.0, refusal: None, settle_cells: Vec::new(), settle_heights: Vec::new() };
    match extending {
        Some(id) => match (sim::dams::dam(sim, id), sim::dams::plan_extend_to(sim, id, line)) {
            (Some(d), Ok(plan)) => {
                let row: Vec<&sim::dams::DamCell> = sim::dams::in_order(plan.cells.iter().filter(|c| c.row == 0)).collect();
                let cells: Vec<Cell> = row.iter().map(|c| c.cell).collect();
                let heights: Vec<f32> = row.iter().map(|c| c.target.max(sim::dams::built(sim, c.cell))).collect();
                out.preview = sim::dam_preview::preview_heights(sim, &cells, &heights);
                (out.settle_cells, out.settle_heights) = (cells, heights);
                out.sticks = sim::dams::plan_cost(sim, d, &plan);
                let added = plan.added(d);
                out.targets = plan.cells.iter().filter(|c| added.contains(&c.cell)).map(|c| (c.cell, c.target)).collect();
            }
            (_, Err(e)) => out.refusal = Some((Icon::Cross, sentence(&e))),
            (None, _) => out.refusal = Some((Icon::Cross, "Gone".into())),
        },
        None => {
            let h = sim::dams::first_targets(sim, line);
            out.targets = line.iter().copied().zip(h.iter().copied()).collect();
            out.preview = sim::dam_preview::preview_heights(sim, line, &h);
            (out.settle_cells, out.settle_heights) = (line.to_vec(), h);
            out.sticks = line_cost(sim, line);
            out.refusal = check_dam(sim, line).err().flatten();
        }
    }
    out
}
