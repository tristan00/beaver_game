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
use sim::buildings::{BuildingId, BuildingKind, Good};
use sim::colony::{BeaverId, LodgeId};
use sim::resources::ResourceId;

/// A thing in the world the pointer can be on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Thing {
    Beaver(BeaverId),
    Building(BuildingId),
    Resource(ResourceId),
    Lodge(LodgeId),
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
        }
    }
    pub fn selection(self) -> Selection {
        match self {
            Thing::Beaver(b) => Selection::Beaver(b),
            Thing::Building(b) => Selection::Site(b),
            Thing::Resource(r) => Selection::Tree(r),
            Thing::Lodge(l) => Selection::Lodge(l),
            Thing::Cell(c) => Selection::Cell(c),
        }
    }
}

#[derive(Resource, Default)]
pub struct HoverThing(pub Option<Thing>);

/// The world point under the pointer (continuous cell coordinates).
#[derive(Resource, Default)]
pub struct HoverPoint(pub Option<(f32, f32)>);

/// What is at continuous cell position `p`: a beaver within reach first, then a building, a
/// resource, a lodge, else the cell.
pub fn pick(s: &sim::SimState, p: (f32, f32)) -> Thing {
    let cell = (p.0.round().max(0.0) as usize, p.1.round().max(0.0) as usize);
    let d2 = |a: (f32, f32)| (a.0 - p.0).powi(2) + (a.1 - p.1).powi(2);
    if let Some(b) = s.colony.beavers.iter().filter(|b| d2(b.pos) < 0.8 * 0.8).min_by(|a, b| d2(a.pos).total_cmp(&d2(b.pos))) {
        return Thing::Beaver(b.id);
    }
    if let Some(b) = s.buildings.buildings.iter().find(|b| b.cells().any(|c| c == cell)) {
        return Thing::Building(b.id);
    }
    if let Some(r) = s.resources.at(cell).find(|r| r.amount > 0.0) {
        return Thing::Resource(r.id);
    }
    if let Some(l) = s.colony.lodges.iter().find(|l| d2(l.pos) < 1.6 * 1.6) {
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
        crate::flow::ray_heightfield(ray.origin, *ray.direction, &sim.0.height).map(|(_, hit, _)| crate::flow::world_cell(&sim.0.height, hit))
    })();
    if point.0 != p {
        point.0 = p;
    }
    let t = p.map(|p| pick(&sim.0, p));
    if thing.0 != t {
        thing.0 = t;
    }
    // Tips for world things only while no tool is active (a tool has its own cursor tag).
    tips.world = match (t, hud.tool) {
        (Some(t), Tool::None) => Some((t.key(), thing_tip(&sim.0, t))),
        _ => None,
    };
}

// ───────────────────────────── markers (W6) ─────────────────────────────

/// World-anchored icons over things that need attention, positioned on screen each frame.
pub fn markers(s: &sim::SimState, cam: &Camera, gt: &GlobalTransform, scale: f32, hud: &HudState) -> Vec<VNode> {
    let mut out = Vec::new();
    let place = |p: (f32, f32), lift: f32| -> Option<Vec2> {
        let mut w = cell_world(&s.height, p);
        w.y += lift;
        cam.world_to_viewport(gt, w).ok().map(|v| v / scale.max(0.01))
    };
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
        let (w, d) = b.kind.spec().footprint;
        let centre = (b.pos.0 as f32 + w as f32 / 2.0 - 0.5, b.pos.1 as f32 + d as f32 / 2.0 - 0.5);
        let mut icons: Vec<(Icon, Color)> = Vec::new();
        if let Some(pl) = s.work.place(b.id) {
            let empty = pl.slots.iter().filter(|x| x.worker.is_none()).count();
            if pl.site.is_some() {
                if empty == pl.slots.len() && !pl.slots.is_empty() {
                    icons.push((Icon::RoleBuilder, WARN));
                    icons.push((Icon::Cross, BAD));
                }
            } else if empty > 0 && b.is_complete() {
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
    // Idle beavers.
    for id in sim::hud::idle_beavers(s) {
        if let Some(b) = s.colony.beaver(id) {
            if let Some(p) = place(b.pos, 1.3) {
                push(p, &[(Icon::Idle, WARN)]);
            }
        }
    }
    // Selected beavers get a ring marker (nothing else uses it).
    for id in hud.selected.iter() {
        if let Some(b) = s.colony.beaver(*id) {
            if let Some(p) = place(b.pos, 0.2) {
                let mut v = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(p.x - 13.0), top: Val::Px(p.y - 13.0), width: Val::Px(26.0), height: Val::Px(26.0), border: UiRect::all(Val::Px(2.0)), border_radius: BorderRadius::all(Val::Px(13.0)), ..default() });
                v.border = Some(BorderColor::all(SELECT));
                out.push(v);
            }
        }
    }
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
                let len = (m.sqrt() * 12.0).clamp(0.35, 1.4);
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
    // A work area being drawn.
    if let (Some(_), Some(a), Some(b)) = (hud.area_edit, hud.area_from, hover.0) {
        let area = sim::work::Area { min: (a.0.min(b.0), a.1.min(b.1)), max: (a.0.max(b.0), a.1.max(b.1)) };
        area_outline(&mut g, h, area, ACCENT);
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
