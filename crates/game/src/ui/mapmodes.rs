//! Map modes (owner: `mapmodes`): Water, Forest, Food and Work each paint information on the map
//! and offer actions that open the tools that already exist (the build menu, the area-assign
//! order, the work-area editor, the workplace panel).
//!
//! Everything here is pure: `overlay` reads `SimState` and returns what to paint and the numbers
//! for the legend; `run_action` changes UI state only. Drawing is `map_overlay.rs`, buttons are
//! `modebar.rs`.

use std::collections::BTreeSet;

use sim::agents::Assignment;
use sim::buildings::BuildingKind;
use sim::colony::{BeaverId, Stage};
use sim::resources::{Resource, ResourceKind};
use sim::water::Cell;
use sim::work::{Area, Role};
use sim::SimState;

use super::explain::{Explained, Numbers, Part};
use super::icons::Icon;
use super::selection::Selection;
use super::shell::{NavItem, Shell};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MapMode {
    /// Plain map, no overlay.
    #[default]
    Normal,
    /// Water depth and flow arrows.
    Water,
    /// Trees, felled trees and the areas marked for felling.
    Forest,
    /// Food plants and bark.
    Food,
    /// Work areas and workers.
    Work,
}

/// An action offered while a mode is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModeAction {
    BuildDam,
    BuildCanal,
    AssignFellers,
    AssignForagers,
    EditWorkArea,
    FillSlots,
}

impl ModeAction {
    pub const ALL: [ModeAction; 6] = [
        ModeAction::BuildDam,
        ModeAction::BuildCanal,
        ModeAction::AssignFellers,
        ModeAction::AssignForagers,
        ModeAction::EditWorkArea,
        ModeAction::FillSlots,
    ];
    pub fn icon(self) -> Icon {
        match self {
            ModeAction::BuildDam => Icon::Build,
            ModeAction::BuildCanal => Icon::Mud,
            ModeAction::AssignFellers => Icon::Collect,
            ModeAction::AssignForagers => Icon::Plants,
            ModeAction::EditWorkArea => Icon::MoveArea,
            ModeAction::FillSlots => Icon::FillSlot,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ModeAction::BuildDam => "Build dam",
            ModeAction::BuildCanal => "Build canal",
            ModeAction::AssignFellers => "Assign fellers",
            ModeAction::AssignForagers => "Assign foragers",
            ModeAction::EditWorkArea => "Edit work area",
            ModeAction::FillSlots => "Fill slots",
        }
    }
    /// What the button does, shown on hover.
    pub fn hint(self) -> &'static str {
        match self {
            ModeAction::BuildDam => "Opens the build menu with the dam chosen; then draw it on the map.",
            ModeAction::BuildCanal => "Opens the build menu with the canal chosen; then draw it on the map.",
            ModeAction::AssignFellers => "Drag a rectangle over trees: the chosen beavers fell inside it.",
            ModeAction::AssignForagers => "Drag a rectangle over plants: the chosen beavers forage inside it.",
            ModeAction::EditWorkArea => "Opens the work-area editor: drag the inside to move the area, its border to resize it.",
            ModeAction::FillSlots => "Opens the workplace panel, where each open slot takes a beaver.",
        }
    }
}

impl MapMode {
    pub const ALL: [MapMode; 5] = [MapMode::Normal, MapMode::Water, MapMode::Forest, MapMode::Food, MapMode::Work];
    /// The four modes with an overlay.
    pub const PAINTED: [MapMode; 4] = [MapMode::Water, MapMode::Forest, MapMode::Food, MapMode::Work];
    pub fn icon(self) -> Icon {
        match self {
            MapMode::Normal => Icon::NavColony,
            MapMode::Water => Icon::ModeWater,
            MapMode::Forest => Icon::ModeForest,
            MapMode::Food => Icon::ModeFood,
            MapMode::Work => Icon::ModeWork,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            MapMode::Normal => "Map",
            MapMode::Water => "Water",
            MapMode::Forest => "Forest",
            MapMode::Food => "Food",
            MapMode::Work => "Work",
        }
    }
    pub fn from_name(s: &str) -> Option<MapMode> {
        MapMode::ALL.into_iter().find(|m| m.label().eq_ignore_ascii_case(s))
    }
    /// One line of what the mode paints, shown on hover.
    pub fn hint(self) -> &'static str {
        match self {
            MapMode::Normal => "The plain map.",
            MapMode::Water => "Water depth (darker is deeper) and which way it flows.",
            MapMode::Forest => "Standing trees, felled trees and the ground marked for felling.",
            MapMode::Food => "Plants to forage, bark to strip and the larder.",
            MapMode::Work => "Work areas and the beavers working in them.",
        }
    }
    pub fn actions(self) -> &'static [ModeAction] {
        match self {
            MapMode::Normal => &[],
            MapMode::Water => &[ModeAction::BuildDam, ModeAction::BuildCanal],
            MapMode::Forest => &[ModeAction::AssignFellers],
            MapMode::Food => &[ModeAction::AssignForagers],
            MapMode::Work => &[ModeAction::EditWorkArea, ModeAction::FillSlots],
        }
    }
}

// ───────────────────────────── what a mode paints ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Dam,
    Canal,
    /// A tree inside ground marked for felling.
    Felling,
    /// A felled tree with logs or branches left.
    Felled,
    /// A felled tree with bark left to strip.
    Bark,
    Worker,
    OpenSlot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tint {
    pub cell: Cell,
    pub rgba: [f32; 4],
    /// Extra height above the ground: tree tints float at canopy height so they show over the leaves.
    pub up: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arrow {
    pub cell: Cell,
    /// Unit direction (east, south).
    pub dir: (f32, f32),
    /// 0..=1, relative to the fastest water on the map.
    pub strength: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub cell: Cell,
    pub kind: MarkKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    pub area: Area,
    pub rgba: [f32; 4],
}

/// One line of the legend: a colour swatch, a caption and a number that explains itself.
#[derive(Clone, Debug, PartialEq)]
pub struct LegendRow {
    pub swatch: [f32; 4],
    pub icon: Option<Icon>,
    pub num: Explained,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
    pub mode: MapMode,
    pub tints: Vec<Tint>,
    pub arrows: Vec<Arrow>,
    pub marks: Vec<Mark>,
    pub outlines: Vec<Outline>,
    pub legend: Vec<LegendRow>,
}

impl Numbers for Overlay {
    fn numbers(&self) -> Vec<&Explained> {
        self.legend.iter().map(|r| &r.num).collect()
    }
}

const TREE_GREEN: [f32; 3] = [0.10, 0.55, 0.18];
const PLANT_GREEN: [f32; 3] = [0.55, 0.90, 0.25];

fn rows(v: Vec<(Icon, [f32; 4], Explained)>) -> Vec<LegendRow> {
    v.into_iter().map(|(i, s, n)| LegendRow { swatch: s, icon: Some(i), num: n }).collect()
}

fn part(label: &str, v: usize) -> Part {
    Explained::part(label, v as f32)
}

fn is_tree(k: ResourceKind) -> bool {
    matches!(k, ResourceKind::Tree | ResourceKind::Felled)
}

/// Ground the colony has marked for gathering `want`: work areas whose slots collect it, and the
/// rectangles dragged for individual beavers.
pub fn marked_areas(sim: &SimState, want: impl Fn(ResourceKind) -> bool) -> Vec<Area> {
    let mut v: Vec<Area> = sim
        .work
        .places
        .iter()
        .filter(|p| p.slots.iter().any(|s| matches!(s.role, Role::Collect(k) if want(k))))
        .filter_map(|p| p.area)
        .collect();
    for a in sim.agents.by_id.values() {
        if let Some(Assignment::Area(k, area)) = a.assignment {
            if want(k) && !v.contains(&area) {
                v.push(area);
            }
        }
    }
    v
}

/// Beavers gathering a kind `want` accepts: those in a collecting slot, and those on an order.
fn gatherers(sim: &SimState, want: impl Fn(ResourceKind) -> bool) -> (usize, usize) {
    let mut slot: BTreeSet<BeaverId> = BTreeSet::new();
    for p in &sim.work.places {
        for s in &p.slots {
            if let (Role::Collect(k), Some(w)) = (s.role, s.worker) {
                if want(k) {
                    slot.insert(w);
                }
            }
        }
    }
    let ordered = sim
        .agents
        .by_id
        .values()
        .filter(|a| !slot.contains(&a.id))
        .filter(|a| match a.assignment {
            Some(Assignment::Area(k, _)) => want(k),
            Some(Assignment::Resource(r)) => sim.resources.get(r).is_some_and(|r| want(r.kind)),
            _ => false,
        })
        .count();
    (slot.len(), ordered)
}

/// Computes the overlay of `mode` from the sim.
pub fn overlay(sim: &SimState, mode: MapMode) -> Overlay {
    let mut o = Overlay { mode, tints: vec![], arrows: vec![], marks: vec![], outlines: vec![], legend: vec![] };
    match mode {
        MapMode::Normal => {}
        MapMode::Water => water(sim, &mut o),
        MapMode::Forest => forest(sim, &mut o),
        MapMode::Food => food(sim, &mut o),
        MapMode::Work => work(sim, &mut o),
    }
    o
}

/// Depth below which water counts as shallow.
const SHALLOW: f32 = 0.5;

fn water(sim: &SimState, o: &mut Overlay) {
    let w = &sim.water;
    let (gw, gh) = (w.depth.width(), w.depth.height());
    let (mut shallow, mut deep, mut v_shallow, mut v_deep) = (0usize, 0usize, 0.0f32, 0.0f32);
    let mut deepest = (0.0f32, (0usize, 0usize));
    let mut speeds = Vec::new();
    let (mut dam_ok, mut dam_weak, mut canals) = (0usize, 0usize, 0usize);
    for y in 0..gh {
        for x in 0..gw {
            let d = w.depth.get(x, y).copied().unwrap_or(0.0);
            if d > 0.03 {
                let t = (d / 2.0).clamp(0.0, 1.0);
                // pale cyan in the shallows, deep blue in the deeps
                let rgba = [0.55 - 0.45 * t, 0.85 - 0.5 * t, 1.0 - 0.15 * t, 0.45 + 0.4 * t];
                o.tints.push(Tint { cell: (x, y), rgba, up: 0.0 });
                if d < SHALLOW {
                    shallow += 1;
                    v_shallow += d;
                } else {
                    deep += 1;
                    v_deep += d;
                }
                if d > deepest.0 {
                    deepest = (d, (x, y));
                }
                let (fx, fy) = w.flow_at(x, y);
                let s = (fx * fx + fy * fy).sqrt();
                if s > 1e-5 {
                    speeds.push(((x, y), fx, fy, s));
                }
            }
            if w.has_dam(x, y) {
                o.marks.push(Mark { cell: (x, y), kind: MarkKind::Dam });
                if w.dam_integrity.get(x, y).copied().unwrap_or(1.0) >= 0.5 {
                    dam_ok += 1;
                } else {
                    dam_weak += 1;
                }
            }
            if w.canal.get(x, y).copied().unwrap_or(false) {
                o.marks.push(Mark { cell: (x, y), kind: MarkKind::Canal });
                canals += 1;
            }
        }
    }
    // One arrow per 3x3 block: the fastest water in it, if it moves noticeably.
    let top = speeds.iter().map(|s| s.3).fold(0.0f32, f32::max);
    let (mut slow_n, mut fast_n) = (0usize, 0usize);
    let mut best: std::collections::BTreeMap<(usize, usize), ((usize, usize), f32, f32, f32)> = Default::default();
    for &(c, fx, fy, s) in &speeds {
        if s >= 0.1 * top {
            if s >= 0.5 * top {
                fast_n += 1;
            } else {
                slow_n += 1;
            }
            let e = best.entry((c.0 / 3, c.1 / 3)).or_insert((c, fx, fy, s));
            if s > e.3 {
                *e = (c, fx, fy, s);
            }
        }
    }
    for (c, fx, fy, s) in best.into_values() {
        o.arrows.push(Arrow { cell: c, dir: (fx / s, fy / s), strength: (s / top).clamp(0.0, 1.0) });
    }
    let cyan = [0.55, 0.85, 1.0, 0.7];
    let blue = [0.1, 0.35, 0.85, 0.85];
    let (r1, r2) = (|v: f32| (v * 100.0).round() / 100.0, |v: f32| v);
    o.legend = rows(vec![
        (Icon::ModeWater, cyan, Explained::sum("Water cells", "", vec![part("Shallow (under 0.5)", shallow), part("Deep", deep)]).of("water cells", [])),
        (Icon::ModeWater, blue, Explained::sum("Water held", "", vec![Explained::part("In the shallows", r1(v_shallow)), Explained::part("In the deeps", r2(v_deep))]).dp(1).of("water held", [])),
        (Icon::ModeWater, blue, Explained::reading("Deepest water", "", deepest.0, &format!("the deepest cell, at {},{}", (deepest.1).0, (deepest.1).1)).dp(2)),
        (Icon::Haul, [1.0, 1.0, 1.0, 0.9], Explained::sum("Cells with current", "", vec![part("Slow current", slow_n), part("Fast current", fast_n)]).of("flowing cells", [])),
        (Icon::Build, [0.45, 0.30, 0.15, 1.0], Explained::sum("Dam cells", "", vec![part("Sound (50% or more)", dam_ok), part("Weak", dam_weak)]).of("dam cells", [])),
        (Icon::Mud, [0.6, 0.45, 0.2, 1.0], Explained::reading("Canal cells", "", canals as f32, "the dug-canal map")),
    ]);
}

fn forest(sim: &SimState, o: &mut Overlay) {
    let marked = marked_areas(sim, is_tree);
    let trees: Vec<&Resource> = sim.resources.items.iter().filter(|r| r.kind == ResourceKind::Tree).collect();
    let near = |c: Cell| trees.iter().filter(|t| t.cell.0.abs_diff(c.0) <= 2 && t.cell.1.abs_diff(c.1) <= 2).count();
    let (mut in_marked, mut outside, mut densest) = (0usize, 0usize, 0usize);
    for t in &trees {
        let n = near(t.cell);
        densest = densest.max(n);
        let a = 0.2 + 0.55 * (n as f32 / 12.0).min(1.0);
        o.tints.push(Tint { cell: t.cell, rgba: [0.0, 0.32, 0.08, a], up: 2.3 });
        if marked.iter().any(|m| m.contains(t.cell)) {
            in_marked += 1;
            o.marks.push(Mark { cell: t.cell, kind: MarkKind::Felling });
        } else {
            outside += 1;
        }
    }
    let (mut with_logs, mut scraps, mut logs) = (0usize, 0usize, 0.0f32);
    for r in sim.resources.items.iter().filter(|r| r.kind == ResourceKind::Felled && r.has_yield()) {
        o.marks.push(Mark { cell: r.cell, kind: MarkKind::Felled });
        if r.amount > 0.0 {
            with_logs += 1;
            logs += r.amount;
        } else {
            scraps += 1;
        }
    }
    for a in &marked {
        o.outlines.push(Outline { area: *a, rgba: [1.0, 0.85, 0.2, 1.0] });
    }
    let (slot, ordered) = gatherers(sim, is_tree);
    o.legend = rows(vec![
        (Icon::ModeForest, [TREE_GREEN[0], TREE_GREEN[1], TREE_GREEN[2], 0.9], Explained::sum("Standing trees", "", vec![part("Marked for felling", in_marked), part("Not marked", outside)]).of("standing trees", [])),
        (Icon::Logs, [0.95, 0.55, 0.15, 1.0], Explained::sum("Felled trees", "", vec![part("With logs left", with_logs), part("Branches and bark only", scraps)]).of("felled trees", [])),
        (Icon::Logs, [0.95, 0.55, 0.15, 1.0], Explained::reading("Logs lying", "", logs, "the felled trees' log stock").dp(0)),
        (Icon::ModeForest, [TREE_GREEN[0], TREE_GREEN[1], TREE_GREEN[2], 0.9], Explained::reading("Densest stand", "", densest as f32, "trees within two cells of the thickest tree")),
        (Icon::Collect, [1.0, 0.85, 0.2, 1.0], Explained::sum("Fellers at work", "", vec![part("In work areas", slot), part("On dragged orders", ordered)]).of("fellers", [])),
    ]);
}

fn food(sim: &SimState, o: &mut Overlay) {
    let (mut ripe, mut regrowing, mut on_plants) = (0usize, 0usize, 0.0f32);
    for r in sim.resources.items.iter().filter(|r| r.kind == ResourceKind::Plant) {
        let f = if r.max_amount > 0.0 { (r.amount / r.max_amount).clamp(0.0, 1.0) } else { 0.0 };
        o.tints.push(Tint { cell: r.cell, rgba: [PLANT_GREEN[0], PLANT_GREEN[1], PLANT_GREEN[2], 0.2 + 0.7 * f], up: 0.15 });
        on_plants += r.amount;
        if f >= 0.99 {
            ripe += 1;
        } else {
            regrowing += 1;
        }
    }
    let (mut bark_trees, mut bark) = (0usize, 0.0f32);
    for r in sim.resources.items.iter().filter(|r| r.kind == ResourceKind::Felled && r.bark > 0.0) {
        o.marks.push(Mark { cell: r.cell, kind: MarkKind::Bark });
        bark_trees += 1;
        bark += r.bark;
    }
    for a in marked_areas(sim, |k| k == ResourceKind::Plant) {
        o.outlines.push(Outline { area: a, rgba: [0.75, 1.0, 0.35, 1.0] });
    }
    let (slot, ordered) = gatherers(sim, |k| k == ResourceKind::Plant);
    o.legend = rows(vec![
        (Icon::Plants, [PLANT_GREEN[0], PLANT_GREEN[1], PLANT_GREEN[2], 0.9], Explained::sum("Plants", "", vec![part("Ripe", ripe), part("Regrowing", regrowing)]).of("plants", [])),
        (Icon::Food, [PLANT_GREEN[0], PLANT_GREEN[1], PLANT_GREEN[2], 0.9], Explained::reading("Food on plants", "", on_plants, "the plants' stock").dp(1)),
        (Icon::Bark, [0.55, 0.35, 0.18, 1.0], Explained::sum("Trees with bark", "", vec![part("Felled, bark not yet stripped", bark_trees)]).of("bark trees", [])),
        (Icon::Bark, [0.55, 0.35, 0.18, 1.0], Explained::reading("Bark waiting", "", bark, "the felled trees' bark stock").dp(1)),
        (Icon::Food, [1.0, 0.9, 0.4, 1.0], Explained::reading("Larder", "", sim.colony.food, "the colony larder").dp(1)),
        (Icon::Plants, [0.75, 1.0, 0.35, 1.0], Explained::sum("Foragers at work", "", vec![part("In work areas", slot), part("On dragged orders", ordered)]).of("foragers", [])),
    ]);
}

fn role_name(r: Role) -> &'static str {
    match r {
        Role::Collect(k) if is_tree(k) => "Felling",
        Role::Collect(ResourceKind::Plant) => "Foraging",
        Role::Collect(ResourceKind::MudBank) => "Mud digging",
        Role::Collect(_) => "Stone gathering",
        Role::Build => "Building",
        Role::Haul => "Hauling",
        Role::Craft => "Crafting",
    }
}

/// Colour of a work area by what it gathers.
pub fn area_rgba(role: Option<Role>) -> [f32; 4] {
    match role {
        Some(Role::Collect(k)) if is_tree(k) => [0.45, 0.95, 0.4, 1.0],
        Some(Role::Collect(ResourceKind::Plant)) => [0.7, 1.0, 0.55, 1.0],
        Some(Role::Collect(ResourceKind::MudBank)) => [0.95, 0.7, 0.4, 1.0],
        Some(Role::Collect(ResourceKind::StoneOutcrop)) => [0.8, 0.85, 0.95, 1.0],
        _ => [0.95, 0.9, 0.5, 1.0],
    }
}

const ROLES: [&str; 7] = ["Felling", "Foraging", "Mud digging", "Stone gathering", "Building", "Hauling", "Crafting"];

fn work(sim: &SimState, o: &mut Overlay) {
    let mut areas = [0usize; 7];
    let (mut filled, mut open) = ([0usize; 7], [0usize; 7]);
    let mut posted: BTreeSet<BeaverId> = BTreeSet::new();
    for p in &sim.work.places {
        if let Some(a) = p.area {
            let rgba = area_rgba(p.slots.first().map(|s| s.role));
            o.outlines.push(Outline { area: a, rgba });
            let mut c = rgba;
            c[3] = 0.16;
            for y in a.min.1..=a.max.1 {
                for x in a.min.0..=a.max.0 {
                    o.tints.push(Tint { cell: (x, y), rgba: c, up: 0.0 });
                }
            }
            if let Some(s) = p.slots.first() {
                let i = ROLES.iter().position(|n| *n == role_name(s.role)).unwrap_or(0);
                areas[i] += 1;
            }
        }
        let at = sim.buildings.buildings.iter().find(|b| b.id == p.building).map(|b| b.pos);
        for s in &p.slots {
            let i = ROLES.iter().position(|n| *n == role_name(s.role)).unwrap_or(0);
            match s.worker {
                Some(w) => {
                    filled[i] += 1;
                    posted.insert(w);
                    if let Some(a) = sim.agents.get(w) {
                        o.marks.push(Mark { cell: a.cell, kind: MarkKind::Worker });
                    }
                }
                None => {
                    open[i] += 1;
                    if let Some(c) = at {
                        o.marks.push(Mark { cell: c, kind: MarkKind::OpenSlot });
                    }
                }
            }
        }
    }
    let idle = sim
        .colony
        .beavers
        .iter()
        .filter(|b| b.stage == Stage::Adult && !posted.contains(&b.id) && sim.agents.get(b.id).is_some_and(|a| a.assignment.is_none()))
        .count();
    let split = |v: [usize; 7]| ROLES.iter().zip(v).map(|(n, c)| part(n, c)).collect::<Vec<_>>();
    o.legend = rows(vec![
        (Icon::MoveArea, [0.95, 0.9, 0.5, 1.0], Explained::sum("Work areas", "", split(areas)).of("work areas", [])),
        (Icon::Collector, [0.45, 0.95, 0.4, 1.0], Explained::sum("Slots filled", "", split(filled)).of("slots filled", [])),
        (Icon::FillSlot, [1.0, 0.45, 0.35, 1.0], Explained::sum("Slots open", "", split(open)).of("slots open", [])),
        (Icon::Rest, [0.8, 0.8, 0.8, 1.0], Explained::sum("Adults without a post", "", vec![part("Adults with no slot or order", idle)]).of("idle adults", [])),
    ]);
}

// ───────────────────────────── actions ─────────────────────────────

/// Adults with nothing to do, up to `n`, for an area order when nobody is selected.
pub fn crew(sim: &SimState, n: usize) -> Vec<BeaverId> {
    sim.colony
        .beavers
        .iter()
        .filter(|b| b.stage == Stage::Adult && sim.work.slot_of(b.id).is_none() && sim.agents.get(b.id).is_some_and(|a| a.assignment.is_none()))
        .map(|b| b.id)
        .take(n)
        .collect()
}

/// How many idle adults take an area order when no beaver is selected.
pub const CREW: usize = 3;

/// What a triggered action did, for the status line.
pub type Outcome = Result<String, String>;

/// Triggers `act`: opens the existing tool it stands for. `panel` is the entity panel slot of
/// `ShellUi`; `selected_building` is the building the player has selected, preferred as target.
pub fn run_action(
    act: ModeAction,
    sim: &SimState,
    shell: &mut Shell,
    panel: &mut Option<Selection>,
    build: &mut crate::build::BuildUi,
    selected_building: Option<sim::buildings::BuildingId>,
) -> Outcome {
    match act {
        ModeAction::BuildDam | ModeAction::BuildCanal => {
            let kind = if act == ModeAction::BuildDam { BuildingKind::Dam } else { BuildingKind::Canal };
            if build.tool != Some(kind) {
                crate::build::pick_tool(sim, build, kind);
            }
            if build.tool != Some(kind) {
                return Err(build.message.clone());
            }
            shell.open = Some(NavItem::Buildings);
            shell.armed = None;
            shell.editor = None;
            Ok(format!("{} chosen: draw it on the map.", kind.spec().name))
        }
        ModeAction::AssignFellers | ModeAction::AssignForagers => {
            let kind = if act == ModeAction::AssignFellers { ResourceKind::Tree } else { ResourceKind::Plant };
            build.select_tool(None);
            shell.close();
            shell.editor = None;
            shell.armed = Some(kind);
            let who = if act == ModeAction::AssignFellers { "fellers" } else { "foragers" };
            Ok(format!("Drag a rectangle on the map to assign {who} (selected beavers, or idle adults)."))
        }
        ModeAction::EditWorkArea => {
            let target = target_place(sim, selected_building, |p| p.area.is_some()).ok_or("No work area to edit yet.")?;
            build.select_tool(None);
            shell.close();
            shell.armed = None;
            shell.editor = Some(target);
            Ok("Work-area editor: drag the inside to move the area, its border to resize.".into())
        }
        ModeAction::FillSlots => {
            let target = target_place(sim, selected_building, |p| p.slots.iter().any(|s| s.worker.is_none())).ok_or("Every slot is filled.")?;
            shell.armed = None;
            shell.editor = None;
            *panel = Some(Selection::Building(target));
            Ok("Workplace panel: pick a beaver for each open slot.".into())
        }
    }
}

/// The selected workplace if it qualifies, else the first one that does.
fn target_place(sim: &SimState, preferred: Option<sim::buildings::BuildingId>, ok: impl Fn(&sim::work::Workplace) -> bool) -> Option<sim::buildings::BuildingId> {
    preferred
        .and_then(|b| sim.work.place(b))
        .filter(|p| ok(p))
        .or_else(|| sim.work.places.iter().find(|p| ok(p)))
        .map(|p| p.building)
}

#[cfg(test)]
#[path = "mapmodes_tests.rs"]
mod tests;
