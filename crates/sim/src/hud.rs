//! Read-only views the interface needs: idle beavers, current problems with their cause and a fix,
//! the river's side profile, and how long the ponds would last through a dry summer. Nothing here
//! changes the state; the game crate turns a [`Fix`] into commands.
use crate::agents::Task;
use crate::buildings::{BuildingId, BuildingKind, Good, WET_DEPTH};
use crate::colony::{BeaverId, Stage};
use crate::resources::ResourceKind;
use crate::society::RequestId;
use crate::water::{Cell, Season};
use crate::work::Role;
use crate::SimState;

/// Most problems the interface shows at once.
pub const MAX_PROBLEMS: usize = 4;
/// Food stocks lasting fewer days than this raise a warning; under a third of it, urgent.
pub const FOOD_WARN_DAYS: f32 = 20.0;
/// A good counts as full at this share of its capacity.
pub const FULL_SHARE: f32 = 0.98;
/// A dam weaker than this is reported.
pub const WEAK_DAM: f32 = 0.35;

/// Working-age beavers (yearlings and adults) with no collect assignment and no workplace slot.
pub fn idle_beavers(sim: &SimState) -> Vec<BeaverId> {
    sim.colony
        .beavers
        .iter()
        .filter(|b| matches!(b.stage, Stage::Yearling | Stage::Adult))
        .filter(|b| sim.work.slot_of(b.id).is_none())
        .filter(|b| sim.agents.get(b.id).is_none_or(|a| a.assignment.is_none() && matches!(a.task, Task::Idle | Task::Rest)))
        .map(|b| b.id)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Urgent,
    Warning,
}

/// The one action a problem card's button performs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fix {
    /// Put an idle beaver into this empty slot.
    FillSlot { building: BuildingId, slot: usize },
    /// Send the idle beavers to gather this resource around the colony.
    Gather(ResourceKind),
    /// Start placing this building.
    Place(BuildingKind),
    /// Accept a wandering beaver.
    Accept(RequestId),
    /// Nothing to click: the card only explains (the jump button still works).
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProblemKind {
    Food,
    Idle,
    NoBuilders,
    StoreFull,
    WeakDam,
    DamFailed,
    Stranger,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub kind: ProblemKind,
    pub severity: Severity,
    /// Short title with the count, e.g. "3 beavers idle".
    pub title: String,
    /// Why, in plain words.
    pub cause: String,
    pub fix: Fix,
    /// Button text for `fix`.
    pub fix_label: String,
    /// Where the jump button takes the camera.
    pub focus: Option<Cell>,
}

fn good_name(g: Good) -> &'static str {
    match g {
        Good::Logs => "logs",
        Good::Sticks => "sticks",
        Good::Mud => "mud",
        Good::Stone => "stone",
        Good::Food => "food",
        Good::Baskets => "baskets",
        Good::Rope => "rope",
        Good::Tools => "tools",
    }
}

/// First empty slot with `role` in a workplace, sites included when `sites`.
fn empty_slot(sim: &SimState, want: impl Fn(Role, bool) -> bool) -> Option<(BuildingId, usize)> {
    sim.work.places.iter().find_map(|p| {
        let site = p.site.is_some();
        p.slots.iter().position(|s| s.worker.is_none() && want(s.role, site)).map(|i| (p.building, i))
    })
}

fn building_cell(sim: &SimState, id: BuildingId) -> Option<Cell> {
    sim.buildings.building(id).map(|b| b.pos)
}

fn colony_cell(sim: &SimState) -> Option<Cell> {
    sim.colony.beavers.first().map(|b| (b.pos.0.max(0.0) as usize, b.pos.1.max(0.0) as usize))
}

/// Food the colony eats per day when everyone is fed (hunger rate times each stage's share).
pub fn food_need_per_day(sim: &SimState) -> f32 {
    sim.colony.beavers.iter().map(|b| crate::tuning::colony::HUNGER_PER_DAY * b.stage.food_weight()).sum()
}

/// Current problems, urgent first, at most [`MAX_PROBLEMS`]. Each names its cause and a fix.
pub fn problems(sim: &SimState) -> Vec<Problem> {
    let mut out = Vec::new();
    if !sim.colony.is_founded() {
        return out;
    }
    let idle = idle_beavers(sim);
    let pop = sim.colony.population();

    // Food: the stock the colony actually eats from, against what the colony needs per day.
    let food = sim.buildings.stores.amount(Good::Food);
    let need = food_need_per_day(sim);
    let gain = sim.ledger.in_per_day(Good::Food);
    if pop > 0 && need > gain {
        let days = food / (need - gain);
        if days < FOOD_WARN_DAYS {
            let (fix, label) = match empty_slot(sim, |r, _| r == Role::Collect(ResourceKind::Plant)) {
                Some((building, slot)) if !idle.is_empty() => (Fix::FillSlot { building, slot }, "Staff the forager hut".to_string()),
                _ if !idle.is_empty() => (Fix::Gather(ResourceKind::Plant), format!("Send {} to gather food", idle.len())),
                _ if sim.tech.allows(BuildingKind::ForagerHut) => (Fix::Place(BuildingKind::ForagerHut), "Place a forager hut".to_string()),
                _ => (Fix::None, "Show".to_string()),
            };
            let title = if food < 0.5 { "No food in store".to_string() } else { format!("Food lasts {} days", days.floor() as i64) };
            out.push(Problem {
                kind: ProblemKind::Food,
                severity: if days < FOOD_WARN_DAYS / 3.0 { Severity::Urgent } else { Severity::Warning },
                title,
                cause: format!("{pop} beavers eat {need:.1}/day, gatherers bring {gain:.1}/day"),
                fix,
                fix_label: label,
                focus: colony_cell(sim),
            });
        }
    }

    // Dams that finished but could not be raised.
    if let Some((b, why)) = sim.work.dam_failures.last() {
        out.push(Problem {
            kind: ProblemKind::DamFailed,
            severity: Severity::Urgent,
            title: "A dam could not be raised".into(),
            cause: why.clone(),
            fix: Fix::None,
            fix_label: "Show".into(),
            focus: building_cell(sim, *b),
        });
    }

    // Weak dams: they breach at zero and release the pond.
    let n = sim.water.dam_height.width();
    let weak: Vec<Cell> = (0..n * n)
        .map(|i| (i % n, i / n))
        .filter(|&(x, y)| {
            sim.water.dam_height.get(x, y).is_some_and(|h| *h > 0.0) && sim.water.dam_integrity.get(x, y).is_some_and(|v| *v < WEAK_DAM)
        })
        .collect();
    if let Some(&first) = weak.first() {
        let worst = weak.iter().filter_map(|&(x, y)| sim.water.dam_integrity.get(x, y)).fold(1.0f32, |a, b| a.min(*b));
        out.push(Problem {
            kind: ProblemKind::WeakDam,
            severity: if worst < WEAK_DAM / 2.0 { Severity::Urgent } else { Severity::Warning },
            title: format!("Dam at {}% strength", (worst * 100.0).round() as i32),
            cause: "Water runs over its crest or it is ageing; it breaks at 0%".into(),
            fix: Fix::None,
            fix_label: "Show".into(),
            focus: Some(first),
        });
    }

    // Sites nobody is building.
    let stalled: Vec<BuildingId> = sim
        .work
        .places
        .iter()
        .filter(|p| p.site.is_some() && p.slots.iter().all(|s| s.worker.is_none()))
        .map(|p| p.building)
        .collect();
    if let Some(&first) = stalled.first() {
        let (fix, label) = if idle.is_empty() {
            (Fix::None, "Show".to_string())
        } else {
            (Fix::FillSlot { building: first, slot: 0 }, "Assign a builder".to_string())
        };
        let s = if stalled.len() == 1 { "" } else { "s" };
        out.push(Problem {
            kind: ProblemKind::NoBuilders,
            severity: Severity::Warning,
            title: format!("{} site{s} without builders", stalled.len()),
            cause: if idle.is_empty() { "Every working beaver is busy".into() } else { format!("{} beavers are idle", idle.len()) },
            fix,
            fix_label: label,
            focus: building_cell(sim, first),
        });
    }

    // Idle beavers (only when no card above already puts them to work).
    if !idle.is_empty() && !out.iter().any(|p| matches!(p.fix, Fix::FillSlot { .. } | Fix::Gather(_))) {
        let (fix, label) = match empty_slot(sim, |r, _| matches!(r, Role::Collect(_) | Role::Build | Role::Craft | Role::Haul)) {
            Some((building, slot)) => (Fix::FillSlot { building, slot }, "Fill an empty job".to_string()),
            None => (Fix::Gather(ResourceKind::Plant), "Send them to gather food".to_string()),
        };
        let s = if idle.len() == 1 { " is" } else { "s are" };
        out.push(Problem {
            kind: ProblemKind::Idle,
            severity: Severity::Warning,
            title: format!("{} beaver{s} idle", idle.len()),
            cause: "No job or workplace slot".into(),
            fix,
            fix_label: label,
            focus: sim.colony.beaver(idle[0]).map(|b| (b.pos.0.max(0.0) as usize, b.pos.1.max(0.0) as usize)),
        });
    }

    // Full stores: gatherers' loads are lost.
    for g in Good::ALL {
        let (have, cap) = (sim.buildings.stores.amount(g), sim.buildings.stores.capacity(g));
        if cap > 0.0 && have >= cap * FULL_SHARE && sim.ledger.in_per_day(g) > 0.0 {
            let unlocked = sim.tech.allows(BuildingKind::Warehouse);
            out.push(Problem {
                kind: ProblemKind::StoreFull,
                severity: Severity::Warning,
                title: format!("{} storage full", good_name(g)),
                cause: format!("{} of {} stored; new loads are lost", have.round() as i64, cap.round() as i64),
                fix: if unlocked { Fix::Place(BuildingKind::Warehouse) } else { Fix::None },
                fix_label: if unlocked { "Place a warehouse".into() } else { "Show".into() },
                focus: colony_cell(sim),
            });
            break;
        }
    }

    // A stranger waiting for an answer.
    if let Some(r) = sim.society.pending().first() {
        out.push(Problem {
            kind: ProblemKind::Stranger,
            severity: Severity::Warning,
            title: format!("{} asks to join", r.name),
            cause: if r.adult { "An adult who can work at once".into() } else { "A young beaver".into() },
            fix: Fix::Accept(r.id),
            fix_label: "Accept".into(),
            focus: Some(r.at),
        });
    }

    out.sort_by_key(|p| p.severity);
    out.truncate(MAX_PROBLEMS);
    out
}

// ───────────────────────────── river profile ─────────────────────────────

/// One river cell seen from the side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfilePoint {
    pub cell: Cell,
    /// Terrain height.
    pub bed: f32,
    /// Top of the dam on this cell; equals `bed` where there is none.
    pub crest: f32,
    /// Water surface (the crest where the cell is dry).
    pub surface: f32,
}

/// The main river from its source to its outlet, one point per cell.
pub fn river_profile(sim: &SimState) -> Vec<ProfilePoint> {
    let w = &sim.world;
    let n = w.size;
    let Some(mut cur) = (0..n * n)
        .map(|i| (i % n, i / n))
        .filter(|&(x, y)| w.main_river.get(x, y).copied().unwrap_or(false))
        .min_by_key(|&(x, y)| w.flow_acc.get(x, y).copied().unwrap_or(u32::MAX))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for _ in 0..n * 4 {
        out.push(profile_point(sim, cur));
        match w.downstream(cur.0, cur.1) {
            Some(next) if !out.iter().any(|p: &ProfilePoint| p.cell == next) => cur = next,
            _ => break,
        }
    }
    out
}

fn profile_point(sim: &SimState, (x, y): Cell) -> ProfilePoint {
    let bed = sim.height.get(x, y).copied().unwrap_or(0.0);
    let crest = bed + sim.water.dam_height.get(x, y).copied().unwrap_or(0.0);
    let surface = crest + sim.water.depth.get(x, y).copied().unwrap_or(0.0);
    ProfilePoint { cell: (x, y), bed, crest, surface }
}

/// The surface each profile point would have with the previewed dam: its pond level where the
/// pond reaches, unchanged elsewhere.
pub fn profile_with_preview(points: &[ProfilePoint], preview: &crate::dam_preview::DamPreview) -> Vec<f32> {
    points
        .iter()
        .map(|p| if preview.flooded.binary_search_by_key(&(p.cell.1, p.cell.0), |c| (c.1, c.0)).is_ok() { p.surface.max(preview.level) } else { p.surface })
        .collect()
}

// ───────────────────────────── ponds and the dry season ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ponds {
    /// Cells holding standing water that the river and lakes do not keep level (dammed ponds).
    pub cells: usize,
    pub volume: f32,
    /// Water the ponds lose per day in summer (evaporation and seepage).
    pub summer_loss_per_day: f32,
}

impl Ponds {
    /// Days the ponds would last through summer with no inflow; `None` without ponds.
    pub fn summer_cover_days(&self) -> Option<f32> {
        (self.cells > 0 && self.summer_loss_per_day > 0.0).then(|| self.volume / self.summer_loss_per_day)
    }
}

pub fn ponds(sim: &SimState) -> Ponds {
    let w = &sim.water;
    let (mut cells, mut volume) = (0, 0.0);
    for (i, &d) in w.depth.as_slice().iter().enumerate() {
        if d >= WET_DEPTH && !w.fed.as_slice()[i] {
            cells += 1;
            volume += d;
        }
    }
    let per_cell = w.config.evaporation * Season::Summer.evap_mult() + w.config.seepage;
    Ponds { cells, volume, summer_loss_per_day: cells as f32 * per_cell }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dam_preview::DamPreview;

    fn pt(x: usize, surface: f32) -> ProfilePoint {
        ProfilePoint { cell: (x, 0), bed: 0.0, crest: 0.0, surface }
    }

    #[test]
    fn preview_raises_only_flooded_points() {
        let pts = [pt(0, 1.0), pt(1, 1.0), pt(2, 3.0)];
        let p = DamPreview {
            level: 2.0,
            rise: 1.0,
            flooded: vec![(0, 0), (2, 0)],
            flooded_trees: 0,
            flooded_buildings: vec![],
            cost: vec![],
            pressure: 0.0,
            fill_days: 0.0,
        };
        assert_eq!(profile_with_preview(&pts, &p), vec![2.0, 1.0, 3.0]);
    }

    #[test]
    fn unfounded_colony_has_no_problems_and_a_profile() {
        let s = SimState::new(5);
        assert!(problems(&s).is_empty());
        let prof = river_profile(&s);
        assert!(prof.len() > 10, "main river profile has {} points", prof.len());
        assert!(prof.first().unwrap().bed >= prof.last().unwrap().bed, "the river runs downhill overall");
    }

    #[test]
    fn pond_cover_needs_ponds() {
        let p = Ponds { cells: 0, volume: 0.0, summer_loss_per_day: 0.0 };
        assert_eq!(p.summer_cover_days(), None);
        let p = Ponds { cells: 10, volume: 50.0, summer_loss_per_day: 2.0 };
        assert_eq!(p.summer_cover_days(), Some(25.0));
    }
}
