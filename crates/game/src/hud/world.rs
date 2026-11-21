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

impl From<crate::picking::Target> for Thing {
    fn from(target: crate::picking::Target) -> Self {
        use crate::picking::Target;
        match target {
            Target::Building(id) => Self::Building(id),
            Target::Beaver(id) => Self::Beaver(id),
            Target::Lodge(id) => Self::Lodge(id),
        }
    }
}

#[derive(Resource, Default)]
pub struct HoverThing(pub Option<Thing>);

#[derive(Resource, Default)]
pub struct HoverPoint(pub Option<(Cell, (f32, f32))>);

pub fn beaver_at(s: &sim::SimState, id: BeaverId) -> Option<(f32, f32)> {
    let ag = s.agents.get(id)?;
    if ag.inside {
        return None;
    }
    let w = crate::entities_render::sim_view::agent_world_pos(s, ag, 0.0);
    Some(crate::flow::world_cell(&s.height, w))
}

pub fn beaver_world(s: &sim::SimState, id: BeaverId) -> Option<Vec3> {
    let ag = s.agents.get(id)?;
    if ag.inside {
        return None;
    }
    Some(crate::entities_render::sim_view::agent_world_pos(s, ag, 0.0))
}

pub fn building_centre(b: &sim::buildings::Building) -> (f32, f32) {
    let (w, d) = b.kind.spec().footprint;
    (b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + d as f32 / 2.0)
}

pub fn pick(s: &sim::SimState, cell: Cell, p: (f32, f32)) -> Thing {
    let d2 = |a: (f32, f32)| (a.0 - p.0).powi(2) + (a.1 - p.1).powi(2);
    let near = s.colony.beavers.iter().filter_map(|b| beaver_at(s, b.id).map(|q| (b.id, d2(q)))).filter(|x| x.1 < 0.9 * 0.9).min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((id, _)) = near {
        return Thing::Beaver(id);
    }
    if let Some(b) = s.buildings.buildings.iter().find(|b| b.cells().any(|c| c == cell)) {
        return Thing::Building(b.id);
    }
    if let Some(r) = s.resources.at(cell).find(|r| r.amount > 0.0 || r.kind == sim::resources::ResourceKind::RiverClay) {
        return Thing::Resource(r.id);
    }
    if let Some(l) = s.colony.lodges.iter().find(|l| d2((l.pos.0 + 0.5, l.pos.1 + 0.5)) < 1.6 * 1.6) {
        return crate::picking::Target::home(s, l.id).into();
    }
    Thing::Cell(cell)
}

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

pub fn world_hover(
    cli: Res<crate::cli::Cli>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    sim: Res<Sim>,
    over: Res<super::PointerOverUi>,
    hud: Res<HudState>,
    mut point: ResMut<HoverPoint>,
    mut thing: ResMut<HoverThing>,
    mut tips: ResMut<Tips>,
    menu: Res<super::MenuOpen>,
    picker: crate::picking::Picker,
) {
    let mut model = None;
    let p = (|| {
        if over.0 || menu.0 || cli.report_dir.is_some() {
            return None;
        }
        let cursor = windows.single().ok()?.cursor_position()?;
        let (cam, gt) = cams.single().ok()?;
        let ray = cam.viewport_to_world(gt, cursor).ok()?;
        let ground = crate::flow::ray_heightfield(ray.origin, *ray.direction, &sim.0.height);
        model = picker.cast(ray, ground.as_ref().map(|g|g.2)).map(Thing::from);
        ground.map(|(c, hit, _)| (c, crate::flow::world_cell(&sim.0.height, hit)))
    })();
    if point.0 != p {
        point.0 = p;
    }
    let t = model.or_else(|| p.map(|(c, p)| pick(&sim.0, c, p)));
    if thing.0 != t {
        thing.0 = t;
    }
    if let (Some((c, _)), Some(pile)) = (p, sim.0.work.supplies) {
        if c == pile && hud.tool == Tool::None && matches!(t, Some(Thing::Cell(_))) {
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
    tips.world = match (t, hud.tool) {
        (Some(t), Tool::None) => Some((t.key(), thing_tip(&sim.0, t))),
        _ => None,
    };
}

/// What is drawn over the map every frame: alert pins where trouble is, small
/// tokens over buildings that need a look, a dot over free beavers, rings around
/// the selected ones, and the pops of things that just happened.
#[allow(clippy::too_many_arguments)]
pub fn markers(s: &sim::SimState, cam: &Camera, gt: &GlobalTransform, scale: f32, hud: &HudState, sel: Selection, problems: &[sim::hud::Problem], pops: &[super::Pop], now: f32) -> Vec<VNode> {
    let mut out = Vec::new();
    let project = |mut w: Vec3, lift: f32| -> Option<Vec2> {
        w.y += lift;
        cam.world_to_viewport(gt, w).ok().map(|v| v / scale.max(0.01))
    };
    let place = |p: (f32, f32), lift: f32| -> Option<Vec2> { project(cell_world(&s.height, p), lift) };
    let pin = |pos: Vec2, icon: Icon, col: Color, skin: Skin, side: f32| -> VNode {
        let mut v = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(pos.x - side / 2.0), top: Val::Px(pos.y - side), width: Val::Px(side), height: Val::Px(side), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() });
        let mut ui = Ui::default();
        ui.icon(icon, side * 0.5, col);
        v.children = ui.nodes;
        v.skin(skin);
        v
    };
    for p in problems {
        let Some(f) = p.focus else { continue };
        let Some(pos) = place((f.0 as f32 + 0.5, f.1 as f32 + 0.5), 2.4) else { continue };
        let (icon, col, _, _, skin) = super::panels::alert_token(s, p);
        let mut v = pin(pos, icon, col, skin, 34.0);
        let act = match p.fix {
            sim::hud::Fix::OpenDam { .. } => super::HudAction::Fix(p.fix, p.focus),
            _ => super::HudAction::Fix(sim::hud::Fix::Focus, p.focus),
        };
        v.action(act);
        out.push(v);
    }
    for b in s.buildings.buildings.iter().filter(|b| b.kind != BuildingKind::Dam) {
        let selected = matches!(sel, Selection::Site(id) if id == b.id);
        let mut icon: Option<(Icon, Color)> = None;
        if let Some(pl) = s.work.place(b.id) {
            let empty = pl.slots.iter().filter(|x| x.worker.is_none()).count();
            if pl.is_open_site() && selected && !pl.paused && empty == pl.slots.len() && !pl.slots.is_empty() {
                icon = Some((Icon::RoleBuilder, WARN));
            } else if !pl.paused && pl.filled() > 0 && pl.slots.iter().filter_map(|s| s.worker).all(|id| s.agents.get(id).is_some_and(|a| a.stuck_ticks > 96)) {
                icon = Some((Icon::Blocked, BAD));
            }
        }
        if b.is_complete() && s.buildings.stranded(b, &s.water.depth) {
            icon = Some((Icon::Flood, WARN));
        }
        if let (Some((i, c)), Some(pos)) = (icon, place(building_centre(b), 2.2)) {
            out.push(pin(pos, i, c, Skin::Pebble, 28.0));
        }
    }
    if let Some(c) = s.work.supplies {
        if let Some(pos) = place((c.0 as f32 + 0.5, c.1 as f32 + 0.5), 0.9) {
            let mut v = pin(pos, Icon::Logs, good_color(Good::Sticks), Skin::Pebble, 26.0);
            v.solid = false;
            out.push(v);
        }
    }
    for id in sim::hud::idle_beavers(s) {
        if let Some(p) = beaver_world(s, id).and_then(|w| project(w, 1.2)) {
            let mut v = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(p.x - 5.0), top: Val::Px(p.y - 10.0), width: Val::Px(10.0), height: Val::Px(10.0), border: UiRect::all(Val::Px(1.5)), border_radius: BorderRadius::all(Val::Px(5.0)), ..default() });
            v.bg = ACCENT;
            v.border = Some(BorderColor::all(PANEL));
            out.push(v);
        }
    }
    for id in hud.selected.iter() {
        if s.colony.beaver(*id).is_some() {
            if let Some(p) = beaver_world(s, *id).and_then(|w| project(w, 0.2)) {
                let mut v = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(p.x - 13.0), top: Val::Px(p.y - 13.0), width: Val::Px(26.0), height: Val::Px(26.0), border: UiRect::all(Val::Px(2.0)), border_radius: BorderRadius::all(Val::Px(13.0)), ..default() });
                v.border = Some(BorderColor::all(SELECT));
                out.push(v);
            }
        }
    }
    for p in pops {
        let age = (now - p.born).max(0.0);
        let fade = (1.0 - age / super::POP_SECS).clamp(0.0, 1.0);
        let at = match p.at {
            super::PopAt::Screen(v) => Some(v),
            super::PopAt::Cell(c) => place(c, 2.0),
        };
        let Some(at) = at else { continue };
        let rise = age * 16.0;
        let mut v = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(at.x - 24.0), top: Val::Px(at.y - 20.0 - rise), padding: UiRect::axes(Val::Px(9.0), Val::Px(5.0)), column_gap: Val::Px(5.0), ..row_node(5.0) });
        let mut ui = Ui::default();
        for (i, c) in &p.icons {
            ui.icon(*i, 16.0, c.with_alpha(fade));
        }
        if !p.text.is_empty() {
            ui.bold(p.text.clone(), 13.0, INK.with_alpha(fade));
        }
        v.children = ui.nodes;
        v.skin(Skin::Plate);
        v.skin = Some((Skin::Plate, Color::srgba(1.0, 1.0, 1.0, fade)));
        out.push(v);
    }
    out
}

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


const FOREST_DOT_LIFT: f32 = 2.6;

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
                continue;
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
    if lens == MapLens::Forest.code() {
        let (r, tx, ty) = (40usize, tx.max(0.0) as usize, ty.max(0.0) as usize);
        for t in s.resources.items.iter().filter(|t| t.cell.0.abs_diff(tx) <= r && t.cell.1.abs_diff(ty) <= r) {
            if let Some(st) = t.stage() {
                tree_dot(&mut g, h, t.cell, st);
            }
        }
    }
    if let Some((b, d)) = match *sel { Selection::Site(id) => s.buildings.building(id).and_then(|b| b.dam.as_deref().map(|d| (b, d))), _ => None } {
        draw_dam(&mut g, s, &hud, b.id, d);
    } else if let Selection::Site(id) = *sel {
        if let Some(b) = s.buildings.building(id) {
            for c in b.cells() {
                cell_outline(&mut g, h, c, None, SELECT);
            }
            if let Some(a) = s.work.place(id).and_then(|p| p.area) {
                area_outline(&mut g, h, a, SELECT);
            }
        }
    }
    if let (Some(id), Some(a), Some(b)) = (hud.area_edit, hud.area_from, hover.0) {
        area_outline(&mut g, h, super::drawn_area(s, id, a, b), ACCENT);
    }
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
                let probe = sim::buildings::Building { id: BuildingId(0), kind, pos: c, status: sim::buildings::Status::Site, output: 0.0, condition: 1.0, reinforced: false, dam: None };
                if let Some(a) = sim::catalogue::work_area(&probe) {
                    let area_col = Color::WHITE.with_alpha(0.6);
                    area_outline(&mut g, h, a, area_col);
                }
                for c in &pv.cells {
                    cell_outline(&mut g, h, *c, None, col);
                    if !pv.is_valid() {
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
        Tool::Dam => draw_line_ghost(&mut g, s, &hud, hover.0),
    }
    draw_dam_wear(&mut g, s);
}

const FOAM: Color = Color::srgb(0.96, 0.98, 0.96);
const WOOD_LINE: Color = Color::srgb(0.95, 0.85, 0.66);

/// Dashed lines along the outer edges of a set of cells, at a given height.
fn shoreline(g: &mut Gizmos, h: &sim::grid::HeightGrid, cells: &[Cell], y: f32, col: Color) {
    let set: std::collections::HashSet<Cell> = cells.iter().copied().collect();
    for &c in cells {
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            let n = (c.0 as i64 + dx, c.1 as i64 + dy);
            if n.0 >= 0 && n.1 >= 0 && set.contains(&(n.0 as usize, n.1 as usize)) {
                continue;
            }
            let (cx, cz) = (c.0 as f32 + dx as f32 * 0.5, c.1 as f32 + dy as f32 * 0.5);
            let (ox, oz) = (dy as f32 * 0.5, dx as f32 * 0.5);
            // Part of each edge only: a dashed line reads as a planned shore.
            let a = cell_world(h, (cx - ox * 0.6, cz - oz * 0.6)).with_y(y);
            let b = cell_world(h, (cx + ox * 0.2, cz + oz * 0.2)).with_y(y);
            g.line(a, b, col);
        }
    }
}

/// While a dam is drawn: its corners, the shore its first stage would make, the
/// trees that would drown, and where the next click would run to.
fn draw_line_ghost(g: &mut Gizmos, s: &sim::SimState, hud: &HudState, hover: Option<Cell>) {
    let h = &s.height;
    if let Some(pv) = hud.dam_preview() {
        shoreline(g, h, &pv.flooded, pv.level + 0.06, FOAM);
        let flooded: std::collections::HashSet<Cell> = pv.flooded.iter().copied().collect();
        for t in s.resources.items.iter().filter(|r| r.kind == sim::resources::ResourceKind::Tree && r.amount > 0.0 && flooded.contains(&r.cell)) {
            if build_logic::is_wet(s, t.cell.0, t.cell.1) {
                continue;
            }
            let base = cell_world(h, (t.cell.0 as f32, t.cell.1 as f32)).with_y(height_at(h, t.cell.0 as f32, t.cell.1 as f32) + 0.25);
            ring(g, base, 0.55, WATER);
            ring(g, base + Vec3::Y * 0.04, 0.5, WATER);
        }
        if let Some(c) = pv.spill_at.filter(|c| !hud.line.contains(c)) {
            let base = cell_world(h, (c.0 as f32, c.1 as f32)).with_y(pv.level.max(height_at(h, c.0 as f32, c.1 as f32)) + 0.1);
            ring(g, base, 0.6, BAD);
        }
    }
    for c in &hud.corners {
        let at = cell_world(h, (c.0 as f32, c.1 as f32)).with_y(height_at(h, c.0 as f32, c.1 as f32) + 0.15);
        ring(g, at, 0.3, ACCENT);
        ring(g, at, 0.22, ACCENT);
    }
    match (hud.corners.last(), hover) {
        (Some(last), Some(at)) if hud.drag_from.is_none() && *last != at => {
            let lift = |c: Cell| cell_world(h, (c.0 as f32, c.1 as f32)).with_y(height_at(h, c.0 as f32, c.1 as f32) + 0.3);
            let (a, b) = (lift(*last), lift(at));
            let n = ((a - b).length() * 2.0).ceil().max(1.0) as usize;
            for i in (0..n).step_by(2) {
                g.line(a.lerp(b, i as f32 / n as f32), a.lerp(b, ((i + 1) as f32 / n as f32).min(1.0)), Color::WHITE.with_alpha(0.45));
            }
        }
        (None, Some(at)) => {
            ring(g, cell_world(h, (at.0 as f32, at.1 as f32)).with_y(height_at(h, at.0 as f32, at.1 as f32) + 0.15), 0.3, Color::WHITE.with_alpha(0.7));
        }
        _ => {}
    }
}

/// On every dam: string between the stakes of cells not yet started, and water
/// spraying through cells that have worn.
fn draw_dam_wear(g: &mut Gizmos, s: &sim::SimState) {
    let h = &s.height;
    for d in s.buildings.buildings.iter().filter_map(|b| b.dam.as_deref()) {
        let line: Vec<&sim::dams::DamCell> = d.line().collect();
        let staked = |c: Cell| sim::dams::built(s, c) <= 1e-3;
        let top = |c: Cell| cell_world(h, (c.0 as f32 + 0.5, c.1 as f32 + 0.5)).with_y(height_at(h, c.0 as f32 + 0.5, c.1 as f32 + 0.5) + 0.95);
        for w in line.windows(2) {
            if staked(w[0].cell) || staked(w[1].cell) {
                g.line(top(w[0].cell), top(w[1].cell), WOOD_LINE);
            }
        }
        for c in d.cells.iter() {
            let built = sim::dams::built(s, c.cell);
            if built <= 0.0 || sim::dams::integrity(s, c.cell) >= sim::dams::REPAIR_BELOW {
                continue;
            }
            let p = cell_world(h, (c.cell.0 as f32 + 0.5, c.cell.1 as f32 + 0.5));
            let y = height_at(h, c.cell.0 as f32 + 0.5, c.cell.1 as f32 + 0.5) + built * 0.7;
            let dir = Vec3::new(d.downstream.0 as f32, 0.0, d.downstream.1 as f32);
            let side = Vec3::new(-dir.z, 0.0, dir.x);
            for k in [-0.25f32, 0.0, 0.25] {
                let a = p.with_y(y) + dir * 0.3 + side * k * 0.5;
                let m = a + dir * 0.35 + side * k * 0.6 + Vec3::Y * 0.12;
                let e = m + dir * 0.3 + side * k * 0.4 - Vec3::Y * (built * 0.7 + 0.1);
                g.line(a, m, FOAM);
                g.line(m, e, FOAM);
            }
        }
    }
}

fn ring(g: &mut Gizmos, at: Vec3, r: f32, col: Color) {
    let n = 18;
    for i in 0..n {
        let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
        g.line(at + Vec3::new(r * a0.cos(), 0.0, r * a0.sin()), at + Vec3::new(r * a1.cos(), 0.0, r * a1.sin()), col);
    }
}

/// A selected dam glows amber along its crest, with a handle at each end to
/// drag it longer, worn cells ringed, the pond's shore and the suggested growth.
fn draw_dam(g: &mut Gizmos, s: &sim::SimState, hud: &HudState, id: BuildingId, d: &sim::dams::Dam) {
    let h = &s.height;
    let top = |c: Cell| {
        let ground = s.height.get(c.0, c.1).copied().unwrap_or(0.0);
        let built = sim::dams::built(s, c);
        let water = ground + s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
        (ground + built).max(water) + 0.12
    };
    let line: Vec<Cell> = d.line().map(|c| c.cell).collect();
    for w in line.windows(2) {
        for k in 0..4 {
            let lift = Vec3::Y * (k as f32 * 0.04);
            let off = Vec3::new(0.0, 0.0, (k as f32 - 1.5) * 0.06);
            let a = cell_world(h, (w[0].0 as f32, w[0].1 as f32)).with_y(top(w[0])) + lift + off;
            let b = cell_world(h, (w[1].0 as f32, w[1].1 as f32)).with_y(top(w[1])) + lift + off;
            g.line(a, b, ACCENT.with_alpha(0.85));
        }
    }
    for c in &d.cells {
        let built = sim::dams::built(s, c.cell);
        if built > 0.0 && sim::dams::integrity(s, c.cell) < sim::dams::REPAIR_BELOW {
            ring(g, cell_world(h, (c.cell.0 as f32, c.cell.1 as f32)).with_y(top(c.cell)), 0.62, BAD);
            ring(g, cell_world(h, (c.cell.0 as f32, c.cell.1 as f32)).with_y(top(c.cell) + 0.04), 0.56, BAD);
        }
        if hud.dam_cells.contains(&c.cell) {
            cell_outline(g, h, c.cell, Some(top(c.cell) + 0.12), SELECT);
        }
    }
    for end in [line.first(), line.last()].into_iter().flatten() {
        let at = cell_world(h, (end.0 as f32, end.1 as f32)).with_y(top(*end) + 0.08);
        for r in [0.34, 0.28, 0.12] {
            ring(g, at, r, ACCENT);
        }
    }
    if let Some(p) = &d.pond {
        shoreline(g, h, &p.flooded, p.full_level + 0.05, WATER.with_alpha(0.8));
    }
    if let Some((_, _, ghost)) = hud.dam_ghost.as_ref().filter(|gh| gh.0 == id) {
        for c in ghost {
            let y = s.height.get(c.0, c.1).copied().unwrap_or(0.0) + 0.6;
            cell_outline(g, h, *c, Some(y), WOOD_LINE.with_alpha(0.8));
        }
    }
}
