//! The world side of the HUD: what is under the pointer (hover tips, selection), markers over
//! things that need attention (spec W6), outlines and arrows drawn on the terrain, and moving the
//! camera so a placement sits in the clear part of the screen (F3.3).
#![allow(deprecated)]
use super::panels::{stage_icon, status_line};
use super::ui::*;
use super::words::*;
use super::{HudState, HoverCell, MapLens, Tool};
use crate::build_logic;
use crate::camera::MainCamera;
use crate::flow::{cell_world, height_at, Cell, Selection};
use crate::icons::Icon;
use crate::state::Sim;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use sim::buildings::{BargeId, BuildingId, BuildingKind, Good};
use sim::colony::{BeaverId, LodgeId};
use sim::resources::ResourceId;

/// A thing in the world the pointer can be on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Thing {
    Beaver(BeaverId),
    Building(BuildingId),
    Resource(ResourceId),
    Lodge(LodgeId),
    Barge(BargeId),
    Cell(Cell),
}

impl Thing {
    pub fn key(self) -> u64 {
        match self {
            Thing::Beaver(b) => 1 << 60 | b.0,
            Thing::Building(b) => 2 << 60 | b.0 as u64,
            Thing::Resource(r) => 3 << 60 | r.0 as u64,
            Thing::Lodge(l) => 4 << 60 | l.0,
            Thing::Cell(c) => 5 << 60 | ((c.0 as u64) << 20) | c.1 as u64,
            Thing::Barge(b) => 6 << 60 | b.0,
        }
    }
    pub fn selection(self) -> Selection {
        match self {
            Thing::Beaver(b) => Selection::Beaver(b),
            Thing::Building(b) => Selection::Site(b),
            Thing::Resource(r) => Selection::Tree(r),
            Thing::Lodge(l) => Selection::Lodge(l),
            Thing::Barge(b) => Selection::Barge(b),
            Thing::Cell(c) => Selection::Cell(c),
        }
    }
}

#[derive(Resource, Default)]
pub struct HoverThing(pub Option<Thing>);

/// The world point under the pointer: the cell, and the point in `flow::world_cell` coordinates.
#[derive(Resource, Default)]
pub struct HoverPoint(pub Option<(Cell, (f32, f32))>);

/// Where a beaver is drawn, in `flow::world_cell` coordinates (the renderer's own position).
pub fn beaver_at(s: &sim::SimState, id: BeaverId) -> Option<(f32, f32)> {
    let ag = s.agents.get(id)?;
    if ag.inside {
        return None;
    }
    let w = crate::entities_render::sim_view::agent_world_pos(s, ag, 0.0);
    Some(crate::flow::world_cell(&s.height, w))
}

/// Where a beaver is drawn, as a world point.
pub fn beaver_world(s: &sim::SimState, id: BeaverId) -> Option<Vec3> {
    let ag = s.agents.get(id)?;
    if ag.inside {
        return None;
    }
    Some(crate::entities_render::sim_view::agent_world_pos(s, ag, 0.0))
}

/// The centre of a building's footprint in `flow::world_cell` coordinates.
pub fn building_centre(b: &sim::buildings::Building) -> (f32, f32) {
    let (w, d) = b.kind.spec().footprint;
    (b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + d as f32 / 2.0)
}

/// What is at continuous cell position `p`: a beaver within reach first, then a building, a
/// resource, a lodge, else the cell.
pub fn pick(s: &sim::SimState, cell: Cell, p: (f32, f32)) -> Thing {
    let d2 = |a: (f32, f32)| (a.0 - p.0).powi(2) + (a.1 - p.1).powi(2);
    let near = s.colony.beavers.iter().filter_map(|b| beaver_at(s, b.id).map(|q| (b.id, d2(q)))).filter(|x| x.1 < 0.9 * 0.9).min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((id, _)) = near {
        return Thing::Beaver(id);
    }
    let barge = s.buildings.barges.iter().map(|b| (b.id, d2((b.cell.0 as f32 + 0.5, b.cell.1 as f32 + 0.5)))).filter(|x| x.1 < 1.1 * 1.1).min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((id, _)) = barge {
        return Thing::Barge(id);
    }
    if let Some(b) = s.buildings.buildings.iter().find(|b| b.cells().any(|c| c == cell)) {
        return Thing::Building(b.id);
    }
    if let Some(r) = s.resources.at(cell).find(|r| r.amount > 0.0) {
        return Thing::Resource(r.id);
    }
    if let Some(l) = s.colony.lodges.iter().find(|l| d2((l.pos.0 + 0.5, l.pos.1 + 0.5)) < 1.6 * 1.6) {
        return Thing::Lodge(l.id);
    }
    Thing::Cell(cell)
}

/// The short hover tip for a world thing (spec §4.1).
pub fn thing_tip(s: &sim::SimState, t: Thing) -> Tip {
    match t {
        Thing::Beaver(id) => {
            let Some(b) = s.colony.beaver(id) else { return Tip::default() };
            let mut tip = Tip::titled(Some((stage_icon(b.stage), INK)), b.name.clone());
            match s.work.slot_of(id) {
                Some((bid, _, role)) => {
                    let place = s.buildings.building(bid).map(|x| x.kind.spec().name).unwrap_or("");
                    tip.push(TipLine::new(role_name(role), place).icon(role_icon(role), INK));
                }
                None => {
                    tip.push(TipLine::new("Free", "").icon(Icon::Idle, WARN));
                }
            }
            if let Some(a) = s.agents.get(id) {
                if let Some(l) = &a.load {
                    let _ = l;
                    tip.push(TipLine::new("Carrying", "").icon(Icon::Haul, MUTED));
                }
            }
            let (mi, mc) = super::inspect::mood_icon(b.mood);
            tip.push(TipLine::new("Mood", format!("{:.0}", b.mood * 100.0)).icon(mi, mc));
            tip.push(TipLine::new("Health", format!("{:.0}", b.health * 100.0)).icon(Icon::Health, if b.health < 0.5 { BAD } else { GOOD }));
            tip
        }
        Thing::Building(id) => {
            let Some(b) = s.buildings.building(id) else { return Tip::default() };
            let mut tip = Tip::titled(Some((building_icon(b.kind), INK)), b.kind.spec().name);
            if let Some((icon, words, goods, col)) = status_line(s, id) {
                let mut l = TipLine::new(words, "").color(col);
                if let Some(i) = icon {
                    l = l.icon(i, col);
                }
                tip.push(l);
                for g in goods {
                    tip.push(TipLine::new(good_name(g), "").icon(good_icon(g), good_color(g)).sub());
                }
            }
            if let Some(pl) = s.work.place(id).filter(|p| !p.slots.is_empty()) {
                let filled = pl.slots.iter().filter(|x| x.worker.is_some()).count();
                tip.push(TipLine::new("Workers", format!("{filled}/{}", pl.slots.len())).icon(role_icon(pl.slots[0].role), INK));
            }
            if let Some(f) = s.output.of(id) {
                for (g, r) in f.rates() {
                    tip.push(TipLine::new(good_name(g), format!("{}/d", num(r))).icon(good_icon(g), good_color(g)).color(GOOD));
                }
            }
            tip
        }
        Thing::Resource(rid) => {
            let Some(r) = s.resources.get(rid) else { return Tip::default() };
            let mut tip = Tip::titled(Some((resource_icon(r.kind), GOOD)), resource_name(r.kind)).with_value(num(r.amount));
            if let Some(p) = s.work.places.iter().find(|p| p.area.is_some_and(|a| a.contains(r.cell))) {
                if let Some(b) = s.buildings.building(p.building) {
                    tip.push(TipLine::new(b.kind.spec().name, "").icon(Icon::Area, MUTED));
                }
            }
            tip
        }
        Thing::Barge(id) => {
            let Some(b) = s.buildings.barge(id) else { return Tip::default() };
            let (icon, col, name) = super::inspect::barge_face(b);
            let mut tip = Tip::titled(Some((Icon::Barge, INK)), "Barge").with_value(super::inspect::barge_number(s, id).to_string());
            tip.push(TipLine::new(name, "").icon(icon, col));
            for (g, a) in super::inspect::aboard(b) {
                tip.push(TipLine::new(good_name(g), num(a)).icon(good_icon(g), good_color(g)));
            }
            tip
        }
        Thing::Lodge(id) => {
            let n = s.colony.beavers.iter().filter(|b| b.lodge == Some(id)).count();
            Tip::titled(Some((Icon::Lodge, INK)), "Lodge").line("Residents", format!("{n}"))
        }
        Thing::Cell(c) => {
            let mut tip = Tip::default();
            for (i, col, name, v) in super::inspect::cell_rows(s, c) {
                tip.push(TipLine::new(name, v).icon(i, col));
            }
            tip
        }
    }
}

/// Tracks the thing under the pointer and offers its tip to the tooltip system.
pub fn world_hover(
    windows: Query<&Window, With<PrimaryWindow>>,
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    sim: Res<Sim>,
    over: Res<super::PointerOverUi>,
    hud: Res<HudState>,
    mut point: ResMut<HoverPoint>,
    mut thing: ResMut<HoverThing>,
    mut tips: ResMut<Tips>,
    menu: Res<super::MenuOpen>,
) {
    let p = (|| {
        if over.0 || menu.0 {
            return None;
        }
        let cursor = windows.single().ok()?.cursor_position()?;
        let (cam, gt) = cams.single().ok()?;
        let ray = cam.viewport_to_world(gt, cursor).ok()?;
        crate::flow::ray_heightfield(ray.origin, *ray.direction, &sim.0.height).map(|(c, hit, _)| (c, crate::flow::world_cell(&sim.0.height, hit)))
    })();
    if point.0 != p {
        point.0 = p;
    }
    let t = p.map(|(c, p)| pick(&sim.0, c, p));
    if thing.0 != t {
        thing.0 = t;
    }
    // The supply pile has its own tip: what is left of the bundle.
    if let (Some((c, _)), Some(pile)) = (p, sim.0.work.supplies) {
        if c == pile && hud.tool == Tool::None {
            let mut tip = Tip::titled(Some((Icon::Haul, INK)), "Supplies");
            for g in Good::ALL {
                let a = sim.0.buildings.stores.amount(g);
                if a > 0.05 {
                    tip.push(TipLine::new(good_name(g), num(a)).icon(good_icon(g), good_color(g)));
                }
            }
            tips.world = Some((u64::MAX, tip));
            return;
        }
    }
    // Tips for world things only while no tool is active (a tool has its own cursor tag). In the
    // elevation lens a bare cell shows its heights in the cursor tag instead.
    let elevation = crate::terrain_render::map_lens() == MapLens::Elevation.code();
    tips.world = match (t, hud.tool) {
        (Some(Thing::Cell(_)), Tool::None) if elevation => None,
        (Some(t), Tool::None) => Some((t.key(), thing_tip(&sim.0, t))),
        _ => None,
    };
}

// ───────────────────────────── markers (W6) ─────────────────────────────

/// World-anchored icons over things that need attention, positioned on screen each frame.
pub fn markers(s: &sim::SimState, cam: &Camera, gt: &GlobalTransform, scale: f32, hud: &HudState, sel: Selection) -> Vec<VNode> {
    let mut out = Vec::new();
    let project = |mut w: Vec3, lift: f32| -> Option<Vec2> {
        w.y += lift;
        cam.world_to_viewport(gt, w).ok().map(|v| v / scale.max(0.01))
    };
    let place = |p: (f32, f32), lift: f32| -> Option<Vec2> { project(cell_world(&s.height, p), lift) };
    let mut push = |pos: Vec2, icons: &[(Icon, Color)]| {
        if out.len() >= 80 {
            return;
        }
        let mut v = VNode::new(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(pos.x - 12.0 * icons.len() as f32),
            top: Val::Px(pos.y - 26.0),
            padding: UiRect::all(Val::Px(2.0)),
            column_gap: Val::Px(1.0),
            border_radius: BorderRadius::all(Val::Px(5.0)),
            ..row_node(1.0)
        });
        v.bg = Color::srgba(0.08, 0.07, 0.06, 0.8);
        let mut ui = Ui::default();
        for (i, c) in icons {
            ui.icon(*i, 16.0, *c);
        }
        v.children = ui.nodes;
        out.push(v);
    };
    // Buildings with empty worker slots or a blocking limit.
    for b in s.buildings.buildings.iter() {
        let centre = building_centre(b);
        let mut icons: Vec<(Icon, Color)> = Vec::new();
        if let Some(pl) = s.work.place(b.id) {
            let empty = pl.slots.iter().filter(|x| x.worker.is_none()).count();
            if pl.site.is_some() {
                if empty == pl.slots.len() && !pl.slots.is_empty() {
                    icons.push((Icon::RoleBuilder, WARN));
                    icons.push((Icon::Cross, BAD));
                }
            } else if b.is_complete() && !pl.slots.is_empty() && empty == pl.slots.len() && pl.slots[0].role != sim::work::Role::Haul {
                // Only a workplace with nobody at all: a half-staffed one shows its dots when
                // selected, and stores work without haulers.
                icons.push((role_icon(pl.slots[0].role), WARN));
                icons.push((Icon::Cross, BAD));
            }
        }
        if b.is_complete() {
            if let Some(rep) = sim::hud::building_report(s, b.id) {
                if rep.factors.iter().any(|f| f.ok == Some(false) && f.label.to_lowercase().contains("store for")) {
                    icons.push((Icon::NoStore, WARN));
                }
            }
            if matches!(b.kind, BuildingKind::Warehouse | BuildingKind::Storehouse | BuildingKind::FoodCache) {
                let full = Good::ALL.iter().any(|g| s.buildings.stores.capacity(*g) > 0.0 && s.buildings.stores.amount(*g) >= s.buildings.stores.capacity(*g) * sim::hud::FULL_SHARE);
                if full {
                    icons.push((Icon::Full, WARN));
                }
            }
        }
        if !icons.is_empty() {
            if let Some(p) = place(centre, 2.2) {
                push(p, &icons);
            }
        }
    }
    // With a barge selected: each dock's number over it (the route picker names docks by number),
    // and the dam a refused route needs a lock on.
    let mut labels = Vec::new();
    if let Selection::Barge(bid) = sel {
        for (i, d) in super::inspect::docks(s).iter().enumerate() {
            if let Some(b) = s.buildings.building(*d) {
                if let Some(p) = place(building_centre(b), 1.6) {
                    let mut v = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(p.x - 22.0), top: Val::Px(p.y - 26.0), padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)), column_gap: Val::Px(3.0), border_radius: BorderRadius::all(Val::Px(5.0)), ..row_node(3.0) });
                    v.bg = Color::srgba(0.08, 0.07, 0.06, 0.85);
                    let mut ui = Ui::default();
                    ui.icon(Icon::Dock, 16.0, INK);
                    ui.bold(format!("{}", i + 1), 13.0, INK);
                    v.children = ui.nodes;
                    labels.push(v);
                }
            }
        }
        if let Some(c) = hud.route_err.as_ref().filter(|e| e.0 == bid).and_then(|e| blocking_dam(&e.1)) {
            if let Some(p) = place((c.0 as f32 + 0.5, c.1 as f32 + 0.5), 1.2) {
                push(p, &[(Icon::Lock, BAD), (Icon::Cross, BAD)]);
            }
        }
    }
    // The dam tool: where water would walk round the drawn dam.
    if hud.tool == Tool::Dam {
        if let Some(c) = hud.dam_preview().and_then(|p| p.spill_at) {
            if let Some(p) = place((c.0 as f32 + 0.5, c.1 as f32 + 0.5), 1.4) {
                push(p, &[(Icon::Flood, BAD), (Icon::Warn, BAD)]);
            }
        }
    }
    // The founding supply pile, until a store stands.
    if let Some(c) = s.work.supplies {
        if let Some(p) = place((c.0 as f32 + 0.5, c.1 as f32 + 0.5), 0.8) {
            push(p, &[(Icon::Logs, good_color(Good::Logs)), (Icon::Food, good_color(Good::Food))]);
        }
    }
    // Idle beavers.
    for id in sim::hud::idle_beavers(s) {
        if let Some(p) = beaver_world(s, id).and_then(|w| project(w, 1.3)) {
            push(p, &[(Icon::Idle, WARN)]);
        }
    }
    // Selected beavers get a ring marker (nothing else uses it).
    for id in hud.selected.iter() {
        if s.colony.beaver(*id).is_some() {
            if let Some(p) = beaver_world(s, *id).and_then(|w| project(w, 0.2)) {
                let mut v = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(p.x - 13.0), top: Val::Px(p.y - 13.0), width: Val::Px(26.0), height: Val::Px(26.0), border: UiRect::all(Val::Px(2.0)), border_radius: BorderRadius::all(Val::Px(13.0)), ..default() });
                v.border = Some(BorderColor::all(SELECT));
                out.push(v);
            }
        }
    }
    out.extend(labels);
    out
}

/// Where the camera target must move so `p` lands in the middle of the work rect (spec F3.3), or
/// `None` when it is already inside.
pub fn frame_target(cam: &Camera, gt: &GlobalTransform, win: Vec2, s: &sim::SimState, target: Vec3, p: Vec3) -> Option<Vec3> {
    let v = cam.world_to_viewport(gt, p).ok()?;
    let inside = v.x > win.x * 0.15 && v.x < win.x * 0.72 && v.y > win.y * 0.08 && v.y < win.y * 0.78;
    if inside {
        return None;
    }
    let want = Vec2::new(win.x * 0.435, win.y * 0.43);
    let ray = cam.viewport_to_world(gt, want).ok()?;
    let (_, hit, _) = crate::flow::ray_heightfield(ray.origin, *ray.direction, &s.height)?;
    let d = target - hit;
    Some(Vec3::new(p.x + d.x, target.y, p.z + d.z))
}

// ───────────────────────────── terrain overlay ─────────────────────────────

fn store_takes(kind: BuildingKind, g: Good) -> bool {
    match kind {
        BuildingKind::Warehouse | BuildingKind::Storehouse => true,
        BuildingKind::FoodCache => g == Good::Food,
        _ => false,
    }
}

/// The water surface over a cell (terrain plus depth), world height.
fn water_y(s: &sim::SimState, c: Cell) -> f32 {
    s.height.get(c.0, c.1).copied().unwrap_or(0.0) + s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0)
}

fn cell_outline(g: &mut Gizmos, h: &sim::grid::HeightGrid, c: Cell, y: Option<f32>, colour: Color) {
    let (x, z) = (c.0 as f32, c.1 as f32);
    let corners = [(x - 0.5, z - 0.5), (x + 0.5, z - 0.5), (x + 0.5, z + 0.5), (x - 0.5, z + 0.5)];
    let pts: Vec<Vec3> = corners
        .iter()
        .map(|&(a, b)| {
            let mut v = cell_world(h, (a, b));
            v.y = y.unwrap_or(height_at(h, a, b)) + 0.08;
            v
        })
        .collect();
    for i in 0..4 {
        g.line(pts[i], pts[(i + 1) % 4], colour);
    }
}

/// Forest lens colours: saplings pale, young mid, mature deep, old brown, snags grey.
pub fn stage_color(st: sim::forestry::TreeStage) -> Color {
    use sim::forestry::TreeStage::*;
    match st {
        Sapling => Color::srgb(0.78, 0.93, 0.62),
        Young => Color::srgb(0.45, 0.78, 0.36),
        Mature => Color::srgb(0.12, 0.5, 0.22),
        Old => Color::srgb(0.58, 0.4, 0.2),
        Snag => Color::srgb(0.6, 0.6, 0.58),
    }
}

/// Role colours of the work areas in the forest lens.
pub const LOGGER_AREA: Color = Color::srgb(0.91, 0.52, 0.23);
pub const FORESTER_AREA: Color = Color::srgb(0.55, 0.88, 0.45);

/// Height above the ground of the forest lens' dots: over the canopy, like a map layer.
const FOREST_DOT_LIFT: f32 = 2.6;

/// A tree in the forest lens: a diamond sized by stage, with a cross through it.
fn tree_dot(g: &mut Gizmos, h: &sim::grid::HeightGrid, c: Cell, st: sim::forestry::TreeStage) {
    use sim::forestry::TreeStage::*;
    let r = match st {
        Sapling => 0.14,
        Young => 0.24,
        Mature | Old => 0.36,
        Snag => 0.2,
    };
    let col = stage_color(st);
    let (x, z) = (c.0 as f32, c.1 as f32);
    let p = |dx: f32, dz: f32| {
        let mut v = cell_world(h, (x + dx, z + dz));
        v.y = height_at(h, x, z) + FOREST_DOT_LIFT;
        v
    };
    let (n, e, s, w) = (p(0.0, -r), p(r, 0.0), p(0.0, r), p(-r, 0.0));
    g.line(n, e, col);
    g.line(e, s, col);
    g.line(s, w, col);
    g.line(w, n, col);
    g.line(n, s, col);
    g.line(e, w, col);
}

fn area_outline(g: &mut Gizmos, h: &sim::grid::HeightGrid, a: sim::work::Area, colour: Color) {
    let (x0, y0, x1, y1) = (a.min.0 as f32 - 0.5, a.min.1 as f32 - 0.5, a.max.0 as f32 + 0.5, a.max.1 as f32 + 0.5);
    let n = 24;
    let mut edge = |from: (f32, f32), to: (f32, f32)| {
        for i in 0..n {
            if i % 2 == 1 {
                continue; // dashed
            }
            let t0 = i as f32 / n as f32;
            let t1 = (i + 1) as f32 / n as f32;
            let p = |t: f32| {
                let q = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
                let mut v = cell_world(h, q);
                v.y = height_at(h, q.0, q.1) + 0.15;
                v
            };
            g.line(p(t0), p(t1), colour);
        }
    };
    edge((x0, y0), (x1, y0));
    edge((x1, y0), (x1, y1));
    edge((x1, y1), (x0, y1));
    edge((x0, y1), (x0, y0));
}

pub fn draw_world_overlay(mut g: Gizmos, sim: Res<Sim>, hud: Res<HudState>, hover: Res<HoverCell>, sel: Res<Selection>, orbit: Res<crate::camera::OrbitCamera>, thing: Res<HoverThing>) {
    let s = &sim.0;
    let h = &s.height;
    let lens = crate::terrain_render::map_lens();
    let (tx, ty) = crate::flow::world_cell(h, orbit.target);
    if lens == MapLens::Water.code() {
        let r = 40i32;
        for y in ((ty as i32 - r).max(1)..(ty as i32 + r).min(s.world.size as i32 - 1)).step_by(3) {
            for x in ((tx as i32 - r).max(1)..(tx as i32 + r).min(s.world.size as i32 - 1)).step_by(3) {
                let (cx, cy) = (x as usize, y as usize);
                let d = s.water.depth.get(cx, cy).copied().unwrap_or(0.0);
                if d < 0.05 {
                    continue;
                }
                let (fx, fy) = s.water.flow_at(cx, cy);
                let m = (fx * fx + fy * fy).sqrt();
                if m < 1e-5 {
                    continue;
                }
                // m is flow per day: a brook of ~1/day gets a short arrow, a river of ~30/day a long one
                let len = (0.25 * m.sqrt()).clamp(0.35, 1.4);
                let dir = Vec2::new(fx, fy) / m;
                let mut a = cell_world(h, (cx as f32, cy as f32));
                a.y = h.get(cx, cy).copied().unwrap_or(0.0) + d + 0.12;
                let b = a + Vec3::new(dir.x, 0.0, dir.y) * len;
                let side = Vec3::new(-dir.y, 0.0, dir.x) * 0.25;
                let back = Vec3::new(dir.x, 0.0, dir.y) * 0.3;
                g.line(a, b, Color::WHITE);
                g.line(b, b - back + side, Color::WHITE);
                g.line(b, b - back - side, Color::WHITE);
            }
        }
    }
    if lens == MapLens::Goods.code() {
        for b in s.buildings.buildings.iter().filter(|b| b.is_complete()) {
            let Some(rep) = sim::hud::building_report(s, b.id) else { continue };
            let col = if rep.factors.first().is_some_and(|f| f.value.starts_with("0 of")) {
                BAD
            } else if rep.factors.iter().any(|f| f.ok == Some(false)) {
                WARN
            } else {
                GOOD
            };
            for c in b.cells() {
                cell_outline(&mut g, h, c, None, col);
            }
            if let Some((good, _)) = rep.ceiling {
                let (w, d) = b.kind.spec().footprint;
                let from = (b.pos.0 as f32 + w as f32 / 2.0 - 0.5, b.pos.1 as f32 + d as f32 / 2.0 - 0.5);
                let store = s.buildings.buildings.iter().filter(|o| o.is_complete() && o.id != b.id && store_takes(o.kind, good)).min_by_key(|o| o.pos.0.abs_diff(b.pos.0) + o.pos.1.abs_diff(b.pos.1));
                if let Some(o) = store {
                    let to = (o.pos.0 as f32, o.pos.1 as f32);
                    let lift = Vec3::Y * 1.2;
                    g.line(cell_world(h, from) + lift, cell_world(h, to) + lift, good_color(good));
                }
            }
        }
    }
    if lens == MapLens::Forest.code() {
        // Trees near the view by stage, and every logging and forestry area in its role colour.
        let (r, tx, ty) = (40usize, tx.max(0.0) as usize, ty.max(0.0) as usize);
        for t in s.resources.items.iter().filter(|t| t.cell.0.abs_diff(tx) <= r && t.cell.1.abs_diff(ty) <= r) {
            if let Some(st) = t.stage() {
                tree_dot(&mut g, h, t.cell, st);
            }
        }
        for p in &s.work.places {
            let col = match p.kind {
                BuildingKind::LoggingCamp => LOGGER_AREA,
                BuildingKind::ForesterLodge => FORESTER_AREA,
                _ => continue,
            };
            if let Some(a) = p.area {
                area_outline(&mut g, h, a, col);
            }
        }
    }
    // The selected building and its work area.
    if let Selection::Site(id) = *sel {
        if let Some(b) = s.buildings.building(id) {
            for c in b.cells() {
                cell_outline(&mut g, h, c, None, SELECT);
            }
            if let Some(a) = s.work.place(id).and_then(|p| p.area) {
                area_outline(&mut g, h, a, SELECT);
            }
        }
    }
    // The selected barge: its cell, and the docks of its route (or the draft being picked).
    if let Selection::Barge(id) = *sel {
        if let Some(b) = s.buildings.barge(id) {
            cell_outline(&mut g, h, b.cell, Some(water_y(s, b.cell)), SELECT);
            let d = super::RouteDraft::of(s, &hud, id);
            for (dock, col) in [(d.from, GOOD), (d.to, ACCENT)] {
                if let Some(db) = dock.and_then(|x| s.buildings.building(x)) {
                    for c in db.cells() {
                        cell_outline(&mut g, h, c, None, col);
                    }
                }
            }
        }
    }
    // A work area being drawn.
    if let (Some(id), Some(a), Some(b)) = (hud.area_edit, hud.area_from, hover.0) {
        area_outline(&mut g, h, super::drawn_area(s, id, a, b), ACCENT);
    }
    // Hovered thing (no tool): a light outline.
    if hud.tool == Tool::None {
        if let Some(Thing::Building(id)) = thing.0 {
            if let Some(b) = s.buildings.building(id) {
                for c in b.cells() {
                    cell_outline(&mut g, h, c, None, Color::WHITE.with_alpha(0.5));
                }
            }
        }
    }
    match hud.tool {
        Tool::None => {}
        Tool::Building(kind) => {
            if let Some(c) = hover.0 {
                let pv = build_logic::site_preview(s, kind, c);
                let col = if pv.is_valid() { Color::WHITE } else { BAD };
                // A gathering building shows the work area it will start with.
                let probe = sim::buildings::Building { id: BuildingId(0), kind, pos: c, status: sim::buildings::Status::Site, recipe: None, output: 0.0 };
                if let Some(a) = sim::catalogue::work_area(&probe) {
                    let area_col = match kind {
                        BuildingKind::LoggingCamp => LOGGER_AREA,
                        BuildingKind::ForesterLodge => FORESTER_AREA,
                        _ => Color::WHITE.with_alpha(0.6),
                    };
                    area_outline(&mut g, h, a, area_col);
                }
                for c in &pv.cells {
                    cell_outline(&mut g, h, *c, None, col);
                    if !pv.is_valid() {
                        // A cross in each blocked cell: shape, not only colour.
                        let a = cell_world(h, (c.0 as f32 - 0.35, c.1 as f32 - 0.35)) + Vec3::Y * 0.1;
                        let b = cell_world(h, (c.0 as f32 + 0.35, c.1 as f32 + 0.35)) + Vec3::Y * 0.1;
                        let a2 = cell_world(h, (c.0 as f32 + 0.35, c.1 as f32 - 0.35)) + Vec3::Y * 0.1;
                        let b2 = cell_world(h, (c.0 as f32 - 0.35, c.1 as f32 + 0.35)) + Vec3::Y * 0.1;
                        g.line(a, b, BAD);
                        g.line(a2, b2, BAD);
                    }
                }
            }
        }
        Tool::Dam | Tool::Canal => {
            if let Some(pv) = hud.dam_preview().filter(|_| hud.tool == Tool::Dam) {
                for c in &pv.flooded {
                    cell_outline(&mut g, h, *c, Some(pv.level), WATER);
                }
                // Where water gets round the dam below its crest: a red post with a ring at the
                // pond level (shape and colour), so the line can be extended over it.
                if let Some(c) = pv.spill_at {
                    let base = cell_world(h, (c.0 as f32, c.1 as f32));
                    let top = Vec3::new(base.x, pv.crest.max(base.y) + 2.0, base.z);
                    g.line(base, top, BAD);
                    for r in [0.45, 0.75] {
                        let n = 16;
                        for i in 0..n {
                            let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
                            let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
                            let y = pv.level.max(base.y) + 0.1;
                            g.line(Vec3::new(base.x + r * a0.cos(), y, base.z + r * a0.sin()), Vec3::new(base.x + r * a1.cos(), y, base.z + r * a1.sin()), BAD);
                        }
                    }
                    cell_outline(&mut g, h, c, None, BAD);
                }
            }
            let ok = match hud.tool {
                Tool::Dam => build_logic::check_dam(s, &hud.line, hud.dam_height).is_ok(),
                _ => build_logic::check_canal(s, &hud.line).is_ok(),
            };
            for c in &hud.line {
                cell_outline(&mut g, h, *c, None, if ok { Color::WHITE } else { BAD });
            }
            if hud.line.is_empty() {
                if let Some(c) = hover.0 {
                    cell_outline(&mut g, h, c, None, Color::WHITE.with_alpha(0.7));
                }
            }
        }
    }
}
