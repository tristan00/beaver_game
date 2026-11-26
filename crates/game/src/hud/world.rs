#![allow(deprecated)]
use super::panels::{stage_icon, status_line};
use super::ui::*;
use super::words::*;
use super::{HoverCell, HudState, MapLens, Tool};
use crate::build_logic;
use crate::camera::MainCamera;
use crate::entities_render::BeaverDrawn;
use crate::flow::{cell_world, height_at, Cell, Selection};
use crate::icons::Icon;
use crate::state::Sim;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use sim::buildings::{BuildingId, Good};
use sim::colony::{BeaverId, LodgeId};
use sim::resources::ResourceId;
use std::collections::{HashSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Thing {
    Beaver(BeaverId), Building(BuildingId), Resource(ResourceId), Lodge(LodgeId),
    Cell(Cell), Wolf(u64), Animal(u32),
}

impl Thing {
    pub fn key(self) -> u64 {
        match self {
            Thing::Beaver(b) => 1 << 60 | b.0,
            Thing::Building(b) => 2 << 60 | b.0 as u64,
            Thing::Resource(r) => 3 << 60 | r.0 as u64,
            Thing::Lodge(l) => 4 << 60 | l.0,
            Thing::Cell(c) => 5 << 60 | ((c.0 as u64) << 20) | c.1 as u64,
            Thing::Wolf(k) => (6 << 60) | (k & ((1 << 60) - 1)),
            Thing::Animal(id) => 7 << 60 | id as u64,
        }
    }
    pub fn selection(self) -> Selection {
        match self {
            Thing::Beaver(b) => Selection::Beaver(b),
            Thing::Building(b) => Selection::Site(b),
            Thing::Resource(r) => Selection::Tree(r),
            Thing::Lodge(l) => Selection::Lodge(l),
            Thing::Cell(c) => Selection::Cell(c),
            Thing::Wolf(k) => Selection::Wolf(k),
            Thing::Animal(id) => Selection::Animal(id),
        }
    }
}

impl From<crate::picking::Target> for Thing {
    fn from(t: crate::picking::Target) -> Self {
        use crate::picking::Target;
        match t { Target::Building(id) => Self::Building(id), Target::Beaver(id) => Self::Beaver(id), Target::Lodge(id) => Self::Lodge(id), Target::Resource(id) => Self::Resource(id) }
    }
}

#[derive(Resource, Default)]
pub struct HoverThing(pub Option<Thing>, pub Option<Thing>);
#[derive(Resource, Default)]
pub struct HoverPoint(pub Option<(Cell, (f32, f32))>);
#[derive(Resource, Default, PartialEq)]
pub struct WorksSnap(pub Option<Cell>);

const SNAP_M: f32 = 0.9;
const WATER_NAME_SHOW_HEIGHT: f32 = 50.0;
const WATER_NAME_FULL_HEIGHT: f32 = 65.0;
const FOREST_DOT_LIFT: f32 = 2.6;
const FOAM: Color = Color::srgb(0.96, 0.98, 0.96);
const WOOD_LINE: Color = Color::srgb(0.95, 0.85, 0.66);

fn cell_y(h: &sim::grid::HeightGrid, c: Cell, y: f32) -> Vec3 { cell_world(h, (c.0 as f32 + 0.5, c.1 as f32 + 0.5)).with_y(y) }
fn cell_at(h: &sim::grid::HeightGrid, c: Cell, lift: f32) -> Vec3 {
    let p = (c.0 as f32 + 0.5, c.1 as f32 + 0.5);
    cell_world(h, p).with_y(height_at(h, p.0, p.1) + lift)
}

pub fn works_top(s: &sim::SimState, c: Cell) -> f32 {
    let g = s.height.get(c.0, c.1).copied().unwrap_or(0.0);
    (g + sim::dams::built(s, c)).max(g + s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0)) + 0.12
}

fn works_cell_on_ray(s: &sim::SimState, id: BuildingId, origin: Vec3, dir: Vec3) -> Option<Cell> {
    let cells: Vec<Cell> = if let Some(d) = sim::dams::dam(s, id) {
        d.line().map(|c| c.cell).collect()
    } else {
        sim::canals::canal(s, id)?.cells.iter().map(|c| c.cell).collect()
    };
    cells.into_iter().filter_map(|c| {
        let p = cell_y(&s.height, c, works_top(s, c));
        let t = (p - origin).dot(dir);
        (t > 0.0).then(|| (c, (origin + dir * t).distance(p)))
    }).filter(|x| x.1 < SNAP_M).min_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0)
}

pub fn beaver_at(s: &sim::SimState, drawn: Option<&BeaverDrawn>, id: BeaverId) -> Option<(f32, f32)> {
    beaver_world(s, drawn, id).map(|w| crate::flow::world_cell(&s.height, w))
}

pub fn beaver_world(s: &sim::SimState, drawn: Option<&BeaverDrawn>, id: BeaverId) -> Option<Vec3> {
    let ag = s.agents.get(id)?;
    (!ag.inside).then(|| drawn.and_then(|d| d.at(id)).unwrap_or_else(|| crate::entities_render::sim_view::agent_world_pos(s, ag, 0.0)))
}

fn animal_on_ray(s: &sim::SimState, ray: Ray3d) -> Option<(u32, f32)> {
    use sim::wildlife::Kind;
    s.wildlife.animals.iter()
        .filter(|a| !crate::fog_render::unexplored(s, (a.pos.0.max(0.0) as usize, a.pos.1.max(0.0) as usize)))
        .filter_map(|a| {
            let size = match a.kind { Kind::Deer => 1.0, Kind::Rabbit => 0.32, Kind::Bird => 0.36 };
            let at = cell_world(&s.height, a.pos) + Vec3::Y * size * 0.6;
            let t = (at - ray.origin).dot(*ray.direction);
            (t > 0.0 && (ray.origin + *ray.direction * t).distance(at) < (size * 0.7).max(0.5)).then_some((a.id, t))
        }).min_by(|a, b| a.1.total_cmp(&b.1))
}

pub fn pick(s: &sim::SimState, drawn: Option<&BeaverDrawn>, cell: Cell, p: (f32, f32)) -> Thing {
    let d2 = |a: (f32, f32)| (a.0 - p.0).powi(2) + (a.1 - p.1).powi(2);
    if let Some((id, _)) = s.colony.beavers.iter().filter_map(|b| beaver_at(s, drawn, b.id).map(|q| (b.id, d2(q)))).filter(|x| x.1 < 0.81).min_by(|a, b| a.1.total_cmp(&b.1)) {
        return Thing::Beaver(id);
    }
    if let Some(w) = s.predators.wolves.iter().filter(|w| d2(w.pos) < 1.44).min_by(|a, b| d2(a.pos).total_cmp(&d2(b.pos))) { return Thing::Wolf(w.key()); }
    if let Some(b) = s.buildings.buildings.iter().find(|b| b.cells().any(|c| c == cell)) { return Thing::Building(b.id); }
    if let Some(r) = s.resources.at(cell).find(|r| r.amount > 0.0 || r.kind.flowing()) { return Thing::Resource(r.id); }
    if let Some(l) = s.colony.lodges.iter().find(|l| d2((l.pos.0 + 0.5, l.pos.1 + 0.5)) < 2.56) { return crate::picking::Target::home(s, l.id).into(); }
    Thing::Cell(cell)
}

pub fn thing_tip(s: &sim::SimState, t: Thing) -> Tip {
    match t {
        Thing::Beaver(id) => {
            let Some(b) = s.colony.beaver(id) else { return Tip::default() };
            let mut tip = Tip::titled(Some((stage_icon(b.stage), INK)), b.name.clone());
            if let Some((_, _, role)) = s.work.slot_of(id) { tip.push(TipLine::new(role.name(), "").icon(job_icon(role), INK)); }
            let (icon, col, doing) = super::inspect::activity(s, b);
            tip.push(TipLine::new(doing, "").icon(icon, col));
            if let Some(a) = s.agents.get(id) {
                if let Some(l) = &a.load {
                    tip.push(TipLine::new(l.good.name(), num(l.amount)).icon(good_icon(l.good), good_color(l.good)));
                    let walk = sim::agents::travel_pace(&s.nav, a);
                    if (walk - 1.0).abs() > 0.005 { tip.push(TipLine::new("Walk", format!("×{walk:.2}")).icon(Icon::Walk, MUTED)); }
                }
            }
            let (mi, mc) = mood_icon(b.mood);
            tip.push(TipLine::new("Mood", format!("{:.0}", b.mood * 100.0)).icon(mi, mc));
            tip.push(TipLine::new("Health", format!("{:.0}", b.health * 100.0)).icon(Icon::Health, if b.health < 0.5 { BAD } else { GOOD }));
            tip
        }
        Thing::Building(id) => {
            let Some(b) = s.buildings.building(id) else { return Tip::default() };
            let mut tip = Tip::titled(Some((building_icon(b.kind), INK)), b.kind.spec().name.as_str());
            if let Some((icon, words, goods, col)) = status_line(s, id) {
                let mut l = TipLine::new(words, "").color(col);
                if let Some(i) = icon { l = l.icon(i, col); }
                tip.push(l);
                for g in goods { tip.push(TipLine::new(g.name(), "").icon(good_icon(g), good_color(g)).sub()); }
            }
            if let Some(pl) = s.work.place(id).filter(|p| !p.slots.is_empty()) {
                let seats = if pl.open_ended() { String::new() } else { format!("/{}", pl.slots.len()) };
                tip.push(TipLine::new("Beavers", format!("{}{seats}", pl.filled())).icon(job_icon(pl.slots[0].role), INK));
            }
            if let Some(f) = s.output.of(id) {
                for (g, r) in f.rates() { tip.push(TipLine::new(g.name(), format!("{}/d", num(r))).icon(good_icon(g), good_color(g)).color(GOOD)); }
            }
            tip
        }
        Thing::Resource(rid) => {
            let Some(r) = s.resources.get(rid) else { return Tip::default() };
            let mut tip = Tip::titled(Some((resource_icon(r.kind), GOOD)), r.plant.map(|p| p.name()).or(r.species.map(|t| t.name())).unwrap_or(resource_name(r.kind)));
            let yields = match r.kind {
                k if k.is_tree() => {
                    tip.push(TipLine::new("Age", days(r.age_days)).icon(Icon::Hourglass, MUTED));
                    tip.push(TipLine::new("Fell work", num(r.amount)).icon(Icon::Hammer, MUTED));
                    r.species.and_then(|sp| r.stage()?.fell_yield(sp))
                }
                k if k.is_felled() => Some((r.amount, r.branches, r.bark)),
                _ => { tip = tip.with_value(num(r.amount)); None }
            };
            if let Some((logs, branches, bark)) = yields {
                let wood = sim::resources::ResourceKind::felled().yields()[0];
                for (name, g, v) in [(wood.name(), wood, logs + branches), ("Bark", Good::food(), bark)] {
                    if v > 0.0 { tip.push(TipLine::new(name, num(v)).icon(good_icon(g), good_color(g))); }
                }
            }
            let works = |p: &sim::work::Workplace| p.slots.iter().any(|sl| match sl.role.duty() {
                sim::content::Duty::Gather => sim::jobs::gather_kind(sl.role).is_some_and(|k| k.gather_sources().contains(&r.kind)),
                sim::content::Duty::Forest => r.kind.is_tree(),
                _ => false,
            });
            if let Some(b) = s.work.places.iter().find(|p| p.area.is_some_and(|a| a.contains(r.cell)) && works(p)).and_then(|p| s.buildings.building(p.building)) {
                tip.push(TipLine::new(format!("{}'s area", b.kind.spec().name.as_str()), "").icon(Icon::Area, MUTED));
            }
            if let Some(&(who, _)) = s.forestry.planted.get(&rid) {
                let name = s.colony.beaver(who).map_or_else(|| "a beaver now gone".into(), |b| b.name.clone());
                tip.push(TipLine::new("Planted by", name).icon(good_icon(Good::seedling()), GOOD));
            }
            tip
        }
        Thing::Lodge(id) => {
            let n = s.colony.beavers.iter().filter(|b| b.lodge == Some(id)).count();
            Tip::titled(Some((building_icon(sim::buildings::BuildingKind::home()), INK)), sim::buildings::BuildingKind::home().name()).line("Residents", format!("{n}"))
        }
        Thing::Wolf(_) => Tip::titled(Some((Icon::Wolf, BAD)), "Wolf"),
        Thing::Animal(id) => {
            let Some(a) = s.wildlife.get(id) else { return Tip::default() };
            let (icon, doing) = animal_doing(a);
            let mut tip = Tip::titled(Some((animal_icon(a.kind), INK)), animal_name(a.kind));
            tip.push(TipLine::new(doing, "").icon(icon, MUTED));
            tip
        }
        Thing::Cell(c) => {
            let mut tip = Tip::default();
            for (i, col, name, v) in cell_rows(s, c) { tip.push(TipLine::new(name, v).icon(i, col)); }
            tip
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn world_hover(
    cli: Res<crate::cli::Cli>, windows: Query<&Window, With<PrimaryWindow>>, cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    sim: Res<Sim>, over: Res<super::PointerOverUi>, hud: Res<HudState>, mut point: ResMut<HoverPoint>,
    mut thing: ResMut<HoverThing>, mut tips: ResMut<Tips>, menu: Res<super::MenuOpen>, picker: crate::picking::Picker,
    sel: Res<Selection>, mut snap: ResMut<WorksSnap>, drawn: Option<Res<BeaverDrawn>>,
) {
    let _p = ::sim::prof::scope("game/world hover");
    let (mut model, mut on_works_cell) = (None, None);
    let p = (|| {
        if over.0 || menu.0 || cli.report_dir.is_some() { return None; }
        let (cam, gt) = cams.single().ok()?;
        let ray = cam.viewport_to_world(gt, windows.single().ok()?.cursor_position()?).ok()?;
        if let Selection::Site(id) = *sel { on_works_cell = works_cell_on_ray(&sim.0, id, ray.origin, *ray.direction); }
        let ground = crate::flow::ray_surface(ray.origin, *ray.direction, &sim.0.height, &sim.0.water.depth);
        let hit = picker.cast_at(ray, ground.as_ref().map(|g| g.2));
        model = hit.map(|(t, _)| Thing::from(t));
        if let Some((id, t)) = animal_on_ray(&sim.0, ray) {
            if hit.is_none_or(|(_, d)| t < d) { model = Some(Thing::Animal(id)); }
        }
        let on_works = hit.and_then(|(t, d)| match t {
            crate::picking::Target::Building(id) => sim.0.buildings.building(id).filter(|b| b.kind.is_waterworks()).map(|_| d),
            _ => None,
        });
        if let Some(d) = on_works {
            let (x, y) = crate::flow::world_cell(&sim.0.height, ray.origin + *ray.direction * d);
            let n = sim.0.world.size as f32;
            (x >= 0.0 && y >= 0.0 && x < n && y < n).then_some(((x as usize, y as usize), (x, y)))
        } else {
            ground.map(|(c, hit, _)| (c, crate::flow::world_cell(&sim.0.height, hit)))
        }
    })();
    if point.0 != p { point.0 = p; }
    if snap.0 != on_works_cell { snap.0 = on_works_cell; }
    if p.is_some_and(|(c, _)| crate::fog_render::unexplored(&sim.0, c)) { model = None; }
    let ground = p.filter(|(c, _)| !crate::fog_render::unexplored(&sim.0, *c)).map(|(c, p)| pick(&sim.0, drawn.as_deref(), c, p));
    let t = model.or(ground);
    let order = if matches!(model, Some(Thing::Resource(_) | Thing::Animal(_))) { ground } else { t };
    if thing.0 != t || thing.1 != order { (thing.0, thing.1) = (t, order); }
    if let (Some((c, _)), Tool::None, Some(Thing::Cell(_))) = (p, hud.tool, t) {
        if sim.0.work.supplies == Some(c) {
            let mut tip = Tip::titled(Some((Icon::Haul, INK)), "Supplies");
            for g in Good::all() {
                let a = sim.0.buildings.stores.stock_of(sim::agents::SUPPLIES, g);
                if a > 0.05 { tip.push(TipLine::new(g.name(), num(a)).icon(good_icon(g), good_color(g))); }
            }
            tips.world = Some((u64::MAX, tip));
            return;
        }
        let here: Vec<_> = sim.0.work.piles.items.iter().filter(|q| q.cell == c).collect();
        if let Some(first) = here.first() {
            let mut tip = Tip::titled(Some((good_icon(first.good), good_color(first.good))), "On the ground");
            for q in &here { tip.push(TipLine::new(q.good.name(), num(q.amount)).icon(good_icon(q.good), good_color(q.good))); }
            let left = here.iter().map(|q| q.days_left(sim.0.time_days)).fold(f64::INFINITY, f64::min);
            tip.push(TipLine::new("Rots in", format!("{left:.0} d")));
            tips.world = Some((u64::MAX - 1 - first.id, tip));
            return;
        }
    }
    tips.world = match (t, hud.tool) { (Some(t), Tool::None) => Some((t.key(), thing_tip(&sim.0, t))), _ => None };
}

#[allow(clippy::too_many_arguments)]
pub fn markers(s: &sim::SimState, drawn: Option<&BeaverDrawn>, cam: &Camera, gt: &GlobalTransform, scale: f32, hud: &HudState, sel: Selection, problems: &[sim::hud::Problem], pops: &[super::Pop], now: f32, wolf_at: Option<Vec3>) -> Vec<VNode> {
    let mut out = Vec::new();
    let project = |mut w: Vec3, lift: f32| { w.y += lift; cam.world_to_viewport(gt, w).ok().map(|v| v / scale.max(0.01)) };
    let place = |p: (f32, f32), lift: f32| project(cell_world(&s.height, p), lift);
    let pin = |pos: Vec2, icon: Icon, col: Color, skin: Skin, side: f32| {
        let mut v = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(pos.x - side / 2.0), top: Val::Px(pos.y - side), width: Val::Px(side), height: Val::Px(side), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() });
        let mut ui = Ui::default();
        ui.icon(icon, side * 0.5, col);
        v.children = ui.nodes;
        v.skin(skin);
        v
    };
    for p in problems {
        let Some(f) = p.focus else { continue };
        let at = match (p.kind, wolf_at) {
            (sim::hud::ProblemKind::Wolf, Some(w)) => project(w, 2.4),
            (sim::hud::ProblemKind::Wolf, None) => None,
            _ => place((f.0 as f32 + 0.5, f.1 as f32 + 0.5), 2.4),
        };
        let Some(pos) = at else { continue };
        let (icon, col, _, _, skin) = super::panels::alert_token(s, p);
        let mut v = pin(pos, icon, col, skin, 34.0);
        let act = match p.fix { sim::hud::Fix::OpenDam { .. } => super::HudAction::Fix(p.fix, p.focus), _ => super::HudAction::Fix(sim::hud::Fix::Focus, p.focus) };
        v.action(act).tip(super::panels::alert_tip_in(s, p)).pin();
        out.push(v);
    }
    for b in s.buildings.buildings.iter().filter(|b| !b.kind.is_dam()) {
        let mut icon = s.work.place(b.id).and_then(|pl| {
            let empty = pl.slots.iter().filter(|x| x.beaver.is_none()).count();
            if pl.is_open_site() && sel == Selection::Site(b.id) && !pl.paused && empty == pl.slots.len() && !pl.slots.is_empty() {
                Some((job_icon(sim::jobs::Job::with_duty(sim::content::Duty::Build)), WARN, "No builder"))
            } else if !pl.paused && pl.filled() > 0 && pl.slots.iter().filter_map(|s| s.beaver).all(|id| s.agents.get(id).is_some_and(|a| a.stuck_ticks > 96)) {
                Some((Icon::Blocked, BAD, "Beavers can't get there"))
            } else { None }
        });
        if b.is_complete() && s.buildings.stranded(b, &s.water.depth) { icon = Some((Icon::Flood, WARN, "High and dry")); }
        if sim::buildings::flooded(b, &s.water.depth) { icon = Some((Icon::Flood, BAD, "Flooded")); }
        if hud.tool == Tool::Dam && hud.drowned.as_ref().is_some_and(|d| d.0 == hud.line && d.1.contains(&b.id)) { icon = Some((Icon::Flood, BAD, "This dam would flood it")); }
        if let (Some((i, c, words)), Some(pos)) = (icon, place(b.centre(), 2.2)) {
            let mut v = pin(pos, i, c, Skin::Pebble, 28.0);
            v.action(super::HudAction::SelectBuilding(b.id)).tip(Tip::titled(Some((building_icon(b.kind), INK)), b.kind.spec().name.as_str()).line(words, "")).pin();
            out.push(v);
        }
    }
    for id in sim::hud::idle_beavers(s) {
        if let Some(p) = beaver_world(s, drawn, id).and_then(|w| project(w, 1.2)) {
            let mut v = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(p.x - 5.0), top: Val::Px(p.y - 10.0), width: Val::Px(10.0), height: Val::Px(10.0), border: UiRect::all(Val::Px(1.5)), border_radius: BorderRadius::all(Val::Px(5.0)), ..default() });
            v.bg(ACCENT).border(BorderColor::all(PANEL));
            out.push(v);
        }
    }
    for &id in hud.selected.iter().filter(|id| s.colony.beaver(**id).is_some()) {
        if let Some(p) = beaver_world(s, drawn, id).and_then(|w| project(w, 0.2)) {
            let mut v = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(p.x - 13.0), top: Val::Px(p.y - 13.0), width: Val::Px(26.0), height: Val::Px(26.0), border: UiRect::all(Val::Px(2.0)), border_radius: BorderRadius::all(Val::Px(13.0)), ..default() });
            v.border(BorderColor::all(SELECT));
            out.push(v);
        }
    }
    for p in pops {
        if let super::PopAt::Cell(c) = p.at { if let Some(at) = place(c, 2.0) { out.push(pop_node(p, at, now)); } }
    }
    out
}

fn water_anchor(cells: &[Cell]) -> Option<Cell> {
    if cells.is_empty() { return None; }
    let (sx, sy) = cells.iter().fold((0.0f32, 0.0f32), |a, c| (a.0 + c.0 as f32, a.1 + c.1 as f32));
    let (cx, cy) = (sx / cells.len() as f32, sy / cells.len() as f32);
    cells.iter().copied().min_by(|a, b| ((a.0 as f32 - cx).powi(2) + (a.1 as f32 - cy).powi(2)).total_cmp(&((b.0 as f32 - cx).powi(2) + (b.1 as f32 - cy).powi(2))))
}

pub fn water_labels(s: &sim::SimState, cam: &Camera, gt: &GlobalTransform, scale: f32, hover: Option<Cell>, orbit: &crate::camera::OrbitCamera, lens: MapLens) -> Vec<VNode> {
    let in_water = crate::terrain_render::map_lens() == crate::terrain_render::WATER_LENS;
    let cam_h = (orbit.eye().y - orbit.target.y).max(0.0);
    let name_alpha = if in_water { 1.0 } else if lens == MapLens::World && cam_h >= WATER_NAME_SHOW_HEIGHT {
        ((cam_h - WATER_NAME_SHOW_HEIGHT) / (WATER_NAME_FULL_HEIGHT - WATER_NAME_SHOW_HEIGHT)).clamp(0.25, 1.0)
    } else { 0.0 };
    if !in_water && name_alpha <= 0.0 { return Vec::new(); }
    let n = s.height.width() as i32;
    let depth = |c: Cell| s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
    let surface = |c: Cell| s.height.get(c.0, c.1).copied().unwrap_or(0.0) + depth(c);
    let wet = |c: Cell| depth(c) > sim::buildings::WET_DEPTH;
    let project_cell = |c: Cell, level: f32| cam.world_to_viewport(gt, cell_y(&s.height, c, level + 1.3)).ok().map(|v| v / scale.max(0.01));
    let mut entries: Vec<(Option<String>, Option<f32>, Vec2)> = Vec::new();
    let mut seen_names = HashSet::new();
    for b in &s.buildings.buildings {
        let Some(d) = b.dam.as_deref() else { continue };
        let name = sim::names::pond_name(s, b.id).map(|n| n.to_string());
        if !in_water && name.is_none() { continue; }
        let on_dam: HashSet<Cell> = d.cells.iter().map(|c| c.cell).collect();
        let (ux, uy) = (-d.downstream.0, -d.downstream.1);
        let face: Vec<Cell> = d.line().filter_map(|c| {
            (1..=d.rows as i32 + 2).map(|k| (c.cell.0 as i32 + ux * k, c.cell.1 as i32 + uy * k))
                .find(|&(x, y)| x < 0 || y < 0 || x >= n || y >= n || !on_dam.contains(&(x as usize, y as usize)))
                .filter(|&(x, y)| x >= 0 && y >= 0 && x < n && y < n)
                .map(|(x, y)| (x as usize, y as usize))
        }).filter(|&c| wet(c)).collect();
        let flooded: Vec<Cell> = d.pond.iter().flat_map(|p| p.flooded.iter().copied()).filter(|&c| wet(c)).collect();
        let Some(mid) = water_anchor(if !flooded.is_empty() { &flooded } else { &face }) else { continue };
        let level = if !face.is_empty() { face.iter().map(|&c| surface(c)).sum::<f32>() / face.len() as f32 } else { surface(mid) };
        let Some(at) = project_cell(mid, level) else { continue };
        if let Some(ref nm) = name { if !seen_names.insert(nm.clone()) { continue; } }
        else if entries.iter().any(|e| e.1.is_some_and(|l| (l - level).abs() < 0.05) && e.2.distance(at) < 90.0) { continue; }
        entries.push((name, in_water.then_some(level), at));
    }
    if name_alpha > 0.0 {
        for pond in &s.names.ponds {
            if pond.name.is_empty() || seen_names.contains(&pond.name) { continue; }
            let wet_cells: Vec<Cell> = pond.cells().iter().copied().filter(|&c| wet(c)).collect();
            let chosen = if !wet_cells.is_empty() { &wet_cells[..] } else if pond.dam.is_some() { pond.cells() } else { continue };
            let Some(mid) = water_anchor(chosen) else { continue };
            let Some(at) = project_cell(mid, surface(mid)) else { continue };
            seen_names.insert(pond.name.clone());
            entries.push((Some(pond.name.clone()), None, at));
        }
    }
    let pointed = in_water.then(|| hover.filter(|&c| wet(c)).map(surface)).flatten();
    let reference = if in_water && (pointed.is_some() || entries.iter().filter(|e| e.1.is_some()).count() >= 2) {
        pointed.or_else(|| entries.iter().filter_map(|e| e.1).reduce(f32::min))
    } else { None };
    let mut out = Vec::new();
    for (name, level, at) in entries {
        let show_name = name.as_deref().filter(|_| name_alpha > 0.0);
        let dz = level.zip(reference).map(|(l, r)| l - r);
        if show_name.is_none() && dz.is_none() { continue; }
        let mut wrap = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(at.x - 110.0), top: Val::Px(at.y - 18.0), width: Val::Px(220.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(3.0), ..default() });
        let mut ui = Ui::default();
        if let Some(name) = show_name {
            let pill = ui.row(5.0, |r| { r.icon(Icon::Drop, 12.0, WATER.with_alpha(name_alpha)); r.bold(name, 13.0, INK.with_alpha(name_alpha)); });
            pill.node.padding = UiRect::axes(Val::Px(9.0), Val::Px(3.0));
            pill.node.border = UiRect::all(Val::Px(1.0));
            pill.node.border_radius = BorderRadius::all(Val::Px(6.0));
            pill.bg(Color::srgba(0.07, 0.10, 0.14, 0.80 * name_alpha)).border(BorderColor::all(WATER.with_alpha(0.45 * name_alpha))).click_through();
        }
        if let Some(dz) = dz {
            let even = dz.abs() < 0.05;
            let lvl = ui.row(3.0, |r| {
                if !even { r.icon(if dz > 0.0 { Icon::Up } else { Icon::Down }, 12.0, if dz > 0.0 { ACCENT } else { WATER }); }
                r.bold(if even { "0 m".into() } else { format!("{} m", num(dz.abs())) }, 12.0, INK);
            });
            lvl.node.padding = UiRect::axes(Val::Px(7.0), Val::Px(3.0));
            lvl.skin(Skin::Plate).tip(Tip::name(if pointed.is_some() { "Pond level, against the water pointed at" } else { "Pond level, against the lowest pond" })).click_through();
        }
        wrap.children = ui.nodes;
        out.push(wrap);
    }
    out
}

pub fn screen_pops(pops: &[super::Pop], now: f32) -> Vec<VNode> {
    pops.iter().filter_map(|p| match p.at { super::PopAt::Screen(at) => Some(pop_node(p, at, now)), _ => None }).collect()
}

fn pop_node(p: &super::Pop, at: Vec2, now: f32) -> VNode {
    let age = (now - p.born).max(0.0);
    let fade = (1.0 - age / super::POP_SECS).clamp(0.0, 1.0);
    let mut v = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(at.x - 24.0), top: Val::Px(at.y - 20.0 - age * 16.0), padding: UiRect::axes(Val::Px(9.0), Val::Px(5.0)), ..row_node(5.0) });
    let mut ui = Ui::default();
    for (i, c) in &p.icons { ui.icon(*i, 16.0, c.with_alpha(fade)); }
    if !p.text.is_empty() { ui.bold(p.text.clone(), 13.0, INK.with_alpha(fade)); }
    v.children = ui.nodes;
    v.skin(Skin::Plate);
    v.skin = Some((Skin::Plate, Color::srgba(1.0, 1.0, 1.0, fade)));
    v
}

pub fn frame_target(cam: &Camera, gt: &GlobalTransform, win: Vec2, s: &sim::SimState, target: Vec3, p: Vec3) -> Option<Vec3> {
    let v = cam.world_to_viewport(gt, p).ok()?;
    if v.x > win.x * 0.15 && v.x < win.x * 0.72 && v.y > win.y * 0.08 && v.y < win.y * 0.78 { return None; }
    let ray = cam.viewport_to_world(gt, Vec2::new(win.x * 0.435, win.y * 0.43)).ok()?;
    let (_, hit, _) = crate::flow::ray_heightfield(ray.origin, *ray.direction, &s.height)?;
    let d = target - hit;
    Some(Vec3::new(p.x + d.x, target.y, p.z + d.z))
}

fn bed_y(s: &sim::SimState, x: f32, y: f32) -> f32 {
    height_at(&s.height, x, y) - crate::terrain_render::channel_cut(s, x.round() as i32, y.round() as i32)
}

fn draw_contours(g: &mut Gizmos, s: &sim::SimState, (tx, ty): (usize, usize), r: usize) {
    let n = s.world.size;
    let depth = |x: usize, y: usize| s.water.depth.get(x, y).copied().unwrap_or(0.0);
    let line = Color::srgba(0.05, 0.18, 0.35, 0.55);
    let (near, max) = crate::terrain_render::flood_depths();
    let mut levels = vec![(crate::terrain_render::SHORE_DEPTH, Color::srgba(1.0, 1.0, 1.0, 0.9)), (sim::raft_landing::RAFT_DEPTH, line), (near, line), (max, line)];
    levels.extend(crate::terrain_render::DEPTH_BANDS.iter().map(|b| b.0).filter(|&d| d.is_finite() && d < near).map(|d| (d, line)));
    let point = |x: f32, y: f32| cell_world(&s.height, (x, y)).with_y(bed_y(s, x, y) + 0.05);
    for y in ty.saturating_sub(r)..(ty + r).min(n - 1) {
        for x in tx.saturating_sub(r)..(tx + r).min(n - 1) {
            let c = [depth(x, y), depth(x + 1, y), depth(x + 1, y + 1), depth(x, y + 1)];
            let pos = [(x as f32, y as f32), (x as f32 + 1.0, y as f32), (x as f32 + 1.0, y as f32 + 1.0), (x as f32, y as f32 + 1.0)];
            for &(level, colour) in &levels {
                let mut hits = [Vec3::ZERO; 4];
                let mut k = 0;
                for e in 0..4 {
                    let (a, b) = (c[e], c[(e + 1) % 4]);
                    if (a < level) != (b < level) {
                        let t = (level - a) / (b - a);
                        let (pa, pb) = (pos[e], pos[(e + 1) % 4]);
                        hits[k] = point(pa.0 + (pb.0 - pa.0) * t, pa.1 + (pb.1 - pa.1) * t);
                        k += 1;
                    }
                }
                if k >= 2 { g.line(hits[0], hits[1], colour); }
                if k == 4 { g.line(hits[2], hits[3], colour); }
            }
        }
    }
}

fn footprint_outline(g: &mut Gizmos, s: &sim::SimState, b: &sim::buildings::Building, colour: Color) {
    let ((w, d), (x0, y0)) = (b.kind.spec().footprint, (b.pos.0 as f32, b.pos.1 as f32));
    let at = |x: f32, y: f32| cell_world(&s.height, (x, y)).with_y(height_at(&s.height, x, y) + s.water.depth.get(x.max(0.0) as usize, y.max(0.0) as usize).copied().unwrap_or(0.0) + 0.12);
    for i in 0..w { let x = x0 + i as f32; g.line(at(x, y0), at(x + 1.0, y0), colour); g.line(at(x, y0 + d as f32), at(x + 1.0, y0 + d as f32), colour); }
    for j in 0..d { let y = y0 + j as f32; g.line(at(x0, y), at(x0, y + 1.0), colour); g.line(at(x0 + w as f32, y), at(x0 + w as f32, y + 1.0), colour); }
}

fn surface_y(s: &sim::SimState, c: Cell) -> Option<f32> {
    let d = s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
    (d > 0.02).then(|| s.height.get(c.0, c.1).copied().unwrap_or(0.0) + d)
}

fn cell_outline(g: &mut Gizmos, h: &sim::grid::HeightGrid, c: Cell, y: Option<f32>, colour: Color) {
    let (x, z) = (c.0 as f32, c.1 as f32);
    let pts: Vec<Vec3> = [(x, z), (x + 1.0, z), (x + 1.0, z + 1.0), (x, z + 1.0)].into_iter().map(|(a, b)| cell_world(h, (a, b)).with_y(y.unwrap_or(height_at(h, a, b)) + 0.08)).collect();
    for i in 0..4 { g.line(pts[i], pts[(i + 1) % 4], colour); }
}

pub fn stage_color(st: sim::forestry::TreeStage) -> Color {
    use sim::forestry::TreeStage::*;
    match st { Seedling => Color::srgb(0.78, 0.93, 0.62), Young => Color::srgb(0.45, 0.78, 0.36), Mature => Color::srgb(0.12, 0.5, 0.22), Old => Color::srgb(0.58, 0.4, 0.2), Snag => Color::srgb(0.6, 0.6, 0.58) }
}

fn tree_dot(g: &mut Gizmos, h: &sim::grid::HeightGrid, c: Cell, st: sim::forestry::TreeStage) {
    use sim::forestry::TreeStage::*;
    let (r, col, (x, z)) = (match st { Seedling => 0.14, Young => 0.24, Mature | Old => 0.36, Snag => 0.2 }, stage_color(st), (c.0 as f32 + 0.5, c.1 as f32 + 0.5));
    let p = |dx: f32, dz: f32| cell_world(h, (x + dx, z + dz)).with_y(height_at(h, x, z) + FOREST_DOT_LIFT);
    let (n, e, s, w) = (p(0.0, -r), p(r, 0.0), p(0.0, r), p(-r, 0.0));
    for (a, b) in [(n, e), (e, s), (s, w), (w, n), (n, s), (e, w)] { g.line(a, b, col); }
}

fn area_outline(g: &mut Gizmos, h: &sim::grid::HeightGrid, a: sim::work::Area, colour: Color) {
    let (x0, y0, x1, y1) = (a.min.0 as f32, a.min.1 as f32, a.max.0 as f32 + 1.0, a.max.1 as f32 + 1.0);
    for (from, to) in [((x0, y0), (x1, y0)), ((x1, y0), (x1, y1)), ((x1, y1), (x0, y1)), ((x0, y1), (x0, y0))] {
        let p = |t: f32| { let q = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t); cell_world(h, q).with_y(height_at(h, q.0, q.1) + 0.15) };
        for i in (0..24).step_by(2) { g.line(p(i as f32 / 24.0), p((i + 1) as f32 / 24.0), colour); }
    }
}

#[allow(clippy::too_many_arguments)]
fn dashed_ellipse(g: &mut Gizmos, h: &sim::grid::HeightGrid, at: (f32, f32), rx: f32, ry: f32, n: usize, lift: f32, colour: Color) {
    let p = |i: usize| {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let q = (at.0 + rx * t.cos(), at.1 + ry * t.sin());
        cell_world(h, q).with_y(height_at(h, q.0, q.1) + lift)
    };
    for i in (0..n).step_by(2) { g.line(p(i), p(i + 1), colour); }
}

fn draw_work_area(g: &mut Gizmos, h: &sim::grid::HeightGrid, a: sim::work::Area, round: bool, colour: Color) {
    if round {
        let (rx, ry) = ((a.max.0 - a.min.0) as f32 / 2.0 + 0.5, (a.max.1 - a.min.1) as f32 / 2.0 + 0.5);
        dashed_ellipse(g, h, ((a.min.0 + a.max.0) as f32 / 2.0 + 0.5, (a.min.1 + a.max.1) as f32 / 2.0 + 0.5), rx, ry, 96, 0.15, colour);
    } else {
        area_outline(g, h, a, colour);
    }
}

fn shown_lookouts(s: &sim::SimState, hud: &HudState, hover: Option<Cell>, sel: Selection, thing: Option<Thing>) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = [match sel { Selection::Site(id) => Some(id), _ => None }, match thing { Some(Thing::Building(id)) => Some(id), _ => None }]
        .into_iter().flatten().filter_map(|id| s.buildings.building(id).filter(|b| b.kind.watches()).map(sim::predators::lookout_centre)).collect();
    if let (Tool::Building(kind), Some(c)) = (hud.tool, hover) && kind.watches() {
        out.push(sim::predators::lookout_centre(&sim::buildings::Building::new(BuildingId(0), kind, c, sim::buildings::Status::Site)));
    }
    out.dedup();
    out
}

#[allow(clippy::too_many_arguments)]
pub fn lookout_range(
    mut commands: Commands, sim: Res<Sim>, hud: Res<HudState>, hover: Res<HoverCell>, sel: Res<Selection>,
    thing: Res<HoverThing>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>,
    mut shown: Local<Option<(Entity, (i32, i32))>>,
) {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let s = &sim.0;
    let want = shown_lookouts(s, &hud, hover.0, *sel, thing.0).first().copied();
    let key = want.map(|c| ((c.0 * 2.0) as i32, (c.1 * 2.0) as i32));
    if shown.map(|x| x.1) == key && (key.is_none() || shown.is_some()) { return; }
    if let Some((e, _)) = shown.take() { commands.entity(e).despawn(); }
    let (Some(at), Some(key)) = (want, key) else { return };
    let (h, r, rings, segs) = (&s.height, sim::predators::LOOKOUT_SIGHT, 24usize, 120usize);
    let ground = |q: (f32, f32)| cell_world(h, q).with_y(height_at(h, q.0, q.1) + s.water.depth.get(q.0.max(0.0) as usize, q.1.max(0.0) as usize).copied().unwrap_or(0.0).max(0.0) + 0.3);
    let mut pos = vec![ground(at).to_array()];
    for i in 1..=rings {
        let rr = r * i as f32 / rings as f32;
        for j in 0..segs { let t = j as f32 / segs as f32 * std::f32::consts::TAU; pos.push(ground((at.0 + rr * t.cos(), at.1 + rr * t.sin())).to_array()); }
    }
    let ring = |i: usize, j: usize| (1 + (i - 1) * segs + j % segs) as u32;
    let mut idx = Vec::new();
    for j in 0..segs { idx.extend([0, ring(1, j + 1), ring(1, j)]); }
    for i in 1..rings { for j in 0..segs { let (a, b, c, d) = (ring(i, j), ring(i, j + 1), ring(i + 1, j), ring(i + 1, j + 1)); idx.extend([a, b, c, b, d, c]); } }
    let n = pos.len();
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default()).with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos).with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n]).with_inserted_indices(Indices::U32(idx));
    let mat = materials.add(StandardMaterial { base_color: ACCENT.with_alpha(0.14), alpha_mode: AlphaMode::Blend, unlit: true, double_sided: true, cull_mode: None, depth_bias: 50.0, ..default() });
    *shown = Some((commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat), Transform::default(), bevy::light::NotShadowCaster)).id(), key));
}

fn draw_sight(g: &mut Gizmos, s: &sim::SimState, drawn: Option<&BeaverDrawn>, hud: &HudState, hover: Option<Cell>, sel: Selection, thing: Option<Thing>) {
    let h = &s.height;
    let mut beavers: Vec<BeaverId> = [match sel { Selection::Beaver(b) => Some(b), _ => None }, match thing { Some(Thing::Beaver(b)) => Some(b), _ => None }].into_iter().flatten().collect();
    beavers.dedup();
    for b in beavers {
        if let Some(at) = beaver_at(s, drawn, b) {
            let r = sim::fog::BEAVER_SIGHT;
            dashed_ellipse(g, h, at, r, r, ((r * 4.0) as usize).clamp(48, 400) & !1, 0.2, Color::WHITE.with_alpha(0.75));
        }
    }
    for w in s.predators.wolves.iter().filter(|w| sel == Selection::Wolf(w.key()) || thing == Some(Thing::Wolf(w.key()))) {
        let r = sim::predators::WOLF_SIGHT;
        dashed_ellipse(g, h, w.pos, r, r, ((r * 4.0) as usize).clamp(48, 400) & !1, 0.2, Color::srgb(0.92, 0.36, 0.3));
    }
    for at in shown_lookouts(s, hud, hover, sel, thing) {
        let (r, n) = (sim::predators::LOOKOUT_SIGHT, 360usize);
        let p = |i: usize, dr: f32| {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            let q = (at.0 + (r + dr) * t.cos(), at.1 + (r + dr) * t.sin());
            cell_world(h, q).with_y(height_at(h, q.0, q.1) + s.water.depth.get(q.0.max(0.0) as usize, q.1.max(0.0) as usize).copied().unwrap_or(0.0).max(0.0) + 0.35)
        };
        for i in 0..n { for dr in [0.0, -0.3] { g.line(p(i, dr), p(i + 1, dr), ACCENT); } }
    }
}

pub fn current_colour(speed: f32) -> Color {
    let (slow, mid, fast) = (Vec3::new(0.05, 0.16, 0.4), Vec3::ONE, Vec3::new(1.0, 0.55, 0.15));
    let c = if speed < 0.3 { slow.lerp(mid, speed / 0.3) } else { mid.lerp(fast, ((speed - 0.3) / 0.7).min(1.0)) };
    Color::srgb(c.x, c.y, c.z)
}

fn draw_currents(g: &mut Gizmos, s: &sim::SimState, (tx, ty): (f32, f32), distance: f32) {
    const MIN_DEPTH: f32 = 0.01;
    const SPREAD: u16 = 24;
    const OPEN: u8 = 4;
    let (h, n, r) = (&s.height, s.world.size as i32, (distance * 1.4).clamp(30.0, 120.0) as i32);
    let stride = if r <= 60 { 2 } else if r <= 95 { 3 } else { 4 };
    let (cx0, cy0) = (tx as i32, ty as i32);
    let ((x0, y0), (x1, y1)) = (((cx0 - r - 8).max(0), (cy0 - r - 8).max(0)), ((cx0 + r + 8).min(n), (cy0 + r + 8).min(n)));
    let (ww, wh) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
    let wet = |x: i32, y: i32| s.water.depth.get(x as usize, y as usize).copied().unwrap_or(0.0) >= MIN_DEPTH;
    let idx = |x: i32, y: i32| (y - y0) as usize * ww + (x - x0) as usize;
    let mut field: Vec<Option<(Vec2, f32, u16)>> = vec![None; ww * wh];
    let mut queue = VecDeque::new();
    for y in y0..y1 {
        for x in x0..x1 {
            if let Some(v) = s.water.velocity.get(x as usize, y as usize).map(|v| Vec2::new(v.0, v.1)).filter(|v| v.length() > 1e-4) {
                field[idx(x, y)] = Some((v.normalize(), v.length(), 0));
                queue.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let Some((d, v, k)) = field[idx(x, y)].filter(|x| x.2 < SPREAD) else { continue };
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx >= x0 && ny >= y0 && nx < x1 && ny < y1 && wet(nx, ny) && field[idx(nx, ny)].is_none() {
                field[idx(nx, ny)] = Some((d, v, k + 1));
                queue.push_back((nx, ny));
            }
        }
    }
    let current = |c: (i32, i32)| (c.0 >= x0 && c.1 >= y0 && c.0 < x1 && c.1 < y1).then(|| field[idx(c.0, c.1)].map(|(d, v, k)| (d, v / (1.0 + k as f32)))).flatten();
    let mut dist = vec![OPEN; ww * wh];
    let mut q = VecDeque::new();
    for y in y0..y1 { for x in x0..x1 { if !wet(x, y) { dist[idx(x, y)] = 0; q.push_back((x, y)); } } }
    let spread = |grid: &mut Vec<u8>, q: &mut VecDeque<(i32, i32)>, grow: bool| {
        while let Some((x, y)) = q.pop_front() {
            let k = grid[idx(x, y)];
            if (grow && k + 1 >= OPEN) || (!grow && k <= 1) { continue; }
            let next = if grow { k + 1 } else { k - 1 };
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx >= x0 && ny >= y0 && nx < x1 && ny < y1 && wet(nx, ny) {
                    let j = idx(nx, ny);
                    if (grow && grid[j] > next) || (!grow && grid[j] < next) { grid[j] = next; q.push_back((nx, ny)); }
                }
            }
        }
    };
    spread(&mut dist, &mut q, true);
    let mut open = vec![0u8; ww * wh];
    for (i, &d) in dist.iter().enumerate() { if d >= OPEN { open[i] = OPEN; q.push_back(((i % ww) as i32 + x0, (i / ww) as i32 + y0)); } }
    spread(&mut open, &mut q, false);
    let surface = |p: (f32, f32)| {
        let c = (p.0.max(0.0) as usize, p.1.max(0.0) as usize);
        cell_world(h, p).with_y(h.get(c.0, c.1).copied().unwrap_or(0.0) + s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0) + 0.15)
    };
    for y in ((cy0 - r).max(1)..(cy0 + r).min(n - 1)).filter(|y| y.rem_euclid(stride) == 0) {
        let shift = if (y / stride) % 2 == 1 { stride / 2 } else { 0 };
        for x in ((cx0 - r).max(1)..(cx0 + r).min(n - 1)).filter(|x| (x - shift).rem_euclid(stride) == 0) {
            if !wet(x, y) { continue; }
            let (i, j) = ((x - shift) / stride, y / stride);
            if x >= x0 && y >= y0 && x < x1 && y < y1 && open[idx(x, y)] > 0 && (j % 3 != 0 || (i + j / 3) % 3 != 0) { continue; }
            let Some((_, speed)) = current((x, y)) else { continue };
            let (mut p, mut pts, mut dir) = ((x as f32 + 0.5, y as f32 + 0.5), vec![(x as f32 + 0.5, y as f32 + 0.5)], Vec2::ZERO);
            for _ in 0..2 {
                let c = (p.0 as i32, p.1 as i32);
                if c.0 < 0 || c.1 < 0 || c.0 >= n || c.1 >= n || !wet(c.0, c.1) { break; }
                let Some((d, v)) = current(c) else { break };
                dir = d;
                let step = (0.8 + 0.6 * v.sqrt()).min(1.5);
                p = (p.0 + dir.x * step, p.1 + dir.y * step);
                pts.push(p);
            }
            if pts.len() < 2 { continue; }
            let (col, d3, side) = (current_colour(speed), Vec3::new(dir.x, 0.0, dir.y), Vec3::new(-dir.y, 0.0, dir.x));
            for nudge in [Vec3::ZERO, side * 0.06] {
                for w in pts.windows(2) { g.line(surface(w[0]) + nudge, surface(w[1]) + nudge, col); }
                let tip = surface(*pts.last().unwrap()) + nudge;
                g.line(tip, tip - d3 * 0.4 + side * 0.25, col);
                g.line(tip, tip - d3 * 0.4 - side * 0.25, col);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_world_overlay(mut g: Gizmos, sim: Res<Sim>, hud: Res<HudState>, hover: Res<HoverCell>, sel: Res<Selection>, orbit: Res<crate::camera::OrbitCamera>, thing: Res<HoverThing>, snap: Res<WorksSnap>, drawn: Option<Res<BeaverDrawn>>) {
    let _p = ::sim::prof::scope("game/draw world overlay");
    let (s, h, lens) = (&sim.0, &sim.0.height, crate::terrain_render::map_lens());
    let (tx, ty) = crate::flow::world_cell(h, orbit.target);
    let (utx, uty) = (tx.max(0.0) as usize, ty.max(0.0) as usize);
    if lens == MapLens::Water.code() {
        draw_currents(&mut g, s, (tx, ty), orbit.distance);
        let r = (orbit.distance * 1.4).clamp(30.0, 110.0) as usize;
        draw_contours(&mut g, s, (utx, uty), r);
        for d in s.resources.items().iter().filter(|d| d.has_yield() && d.cell.0.abs_diff(utx) <= r && d.cell.1.abs_diff(uty) <= r && crate::terrain_render::underwater_deposit(s, d)) {
            let [cr, cg, cb] = crate::terrain_render::deposit_color(d.kind);
            cell_outline(&mut g, h, d.cell, Some(bed_y(s, d.cell.0 as f32, d.cell.1 as f32)), Color::srgb(cr, cg, cb));
        }
        let [fr, fg, fb] = crate::terrain_render::FLOODED;
        for b in s.buildings.buildings.iter().filter(|b| sim::buildings::flooded(b, &s.water.depth)) { footprint_outline(&mut g, s, b, Color::srgb(fr, fg, fb)); }
    } else if lens == MapLens::Forest.code() {
        for t in s.resources.items().iter().filter(|t| t.cell.0.abs_diff(utx) <= 40 && t.cell.1.abs_diff(uty) <= 40) {
            if let Some(st) = t.stage() { tree_dot(&mut g, h, t.cell, st); }
        }
    } else if lens == MapLens::Deposits.code() {
        for d in crate::terrain_render::shown_deposits(s).filter(|d| !d.kind.is_tree() && d.cell.0.abs_diff(utx) <= 40 && d.cell.1.abs_diff(uty) <= 40) {
            let depth = s.water.depth.get(d.cell.0, d.cell.1).copied().unwrap_or(0.0);
            let [cr, cg, cb] = crate::terrain_render::deposit_color(d.kind);
            cell_outline(&mut g, h, d.cell, (depth > 0.03).then(|| h.get(d.cell.0, d.cell.1).copied().unwrap_or(0.0) + depth), Color::srgb(cr, cg, cb));
        }
    }
    if let Some(c) = snap.0 {
        let at = cell_y(h, c, works_top(s, c) + 0.06);
        ring(&mut g, at, 0.55, Color::WHITE.with_alpha(0.95));
        ring(&mut g, at, 0.48, Color::WHITE.with_alpha(0.6));
    }
    if let Selection::Site(id) = *sel {
        if let Some(b) = s.buildings.building(id) {
            if let Some(d) = b.dam.as_deref() {
                draw_dam(&mut g, s, &hud, b.id, d);
            } else {
                for c in b.cells() { cell_outline(&mut g, h, c, surface_y(s, c), SELECT); }
                if let Some(a) = s.work.place(id).and_then(|p| p.area) { draw_work_area(&mut g, h, a, b.kind.row().round_area, SELECT); }
            }
        }
    }
    if let Selection::Animal(id) = *sel {
        if let Some(a) = s.wildlife.get(id) { double_ring(&mut g, cell_world(h, a.pos).with_y(height_at(h, a.pos.0, a.pos.1) + 0.15), 0.6, 0.53, 0.0, SELECT); }
    }
    if let Selection::Tree(id) = *sel {
        if let Some(r) = s.resources.get(id) { double_ring(&mut g, cell_at(h, r.cell, 0.15), 0.55, 0.48, 0.0, SELECT); }
    }
    if hud.tool == Tool::None {
        let hover_col = Color::WHITE.with_alpha(0.8);
        match thing.0 {
            Some(Thing::Resource(id)) => if let Some(r) = s.resources.get(id) { ring(&mut g, cell_at(h, r.cell, 0.15), if r.kind.is_tree() || r.kind.is_felled() { 0.6 } else { 0.45 }, hover_col); }
            Some(Thing::Beaver(id)) => if let Some(w) = beaver_world(s, drawn.as_deref(), id) { ring(&mut g, w + Vec3::Y * 0.1, 0.45, hover_col); }
            Some(Thing::Lodge(id)) => if let Some(l) = s.colony.lodges.iter().find(|l| l.id == id) { ring(&mut g, cell_world(h, l.pos).with_y(height_at(h, l.pos.0, l.pos.1) + 0.15), 1.1, hover_col); }
            Some(Thing::Building(id)) => if let Some(b) = s.buildings.building(id) { for c in b.cells() { cell_outline(&mut g, h, c, surface_y(s, c), hover_col); } }
            _ => if let (Some(c), Some(Thing::Cell(_))) = (hover.0, thing.1) { ring(&mut g, cell_at(h, c, 0.15), 0.4, hover_col); }
        }
        let picked: Vec<BeaverId> = if !hud.selected.is_empty() { hud.selected.clone() } else if let Selection::Beaver(b) = *sel { vec![b] } else { Vec::new() };
        for id in picked {
            let Some(ag) = s.agents.get(id) else { continue };
            if let Some(sim::agents::Assignment::Go(to)) = ag.assignment { double_ring(&mut g, cell_at(h, to, 0.15), 0.4, 0.3, 0.0, ACCENT); }
            let queued = ACCENT.with_alpha(0.7);
            let mut from = ag.assignment.and_then(|a| order_cell(s, sim::agents::Order::Do(a))).unwrap_or(ag.cell);
            for &order in &ag.queue {
                let Some(c) = order_cell(s, order) else { continue };
                g.line(cell_at(h, from, 0.15), cell_at(h, c, 0.15), queued);
                ring(&mut g, cell_at(h, c, 0.15), 0.3, queued);
                from = c;
            }
        }
    }
    match hud.tool {
        Tool::None => {}
        Tool::Building(kind) => if let Some(c) = hover.0 {
            let pv = build_logic::site_preview(s, kind, c);
            if let Some(a) = sim::catalogue::work_area_at(kind, c) { draw_work_area(&mut g, h, a, kind.row().round_area, Color::WHITE.with_alpha(0.6)); }
            for &c in &pv.cells {
                cell_outline(&mut g, h, c, None, if pv.is_valid() { Color::WHITE } else { BAD });
                if !pv.is_valid() {
                    let p = |dx: f32, dz: f32| cell_world(h, (c.0 as f32 + dx, c.1 as f32 + dz)) + Vec3::Y * 0.1;
                    g.line(p(-0.35, -0.35), p(0.35, 0.35), BAD);
                    g.line(p(0.35, -0.35), p(-0.35, 0.35), BAD);
                }
            }
        },
        Tool::Dam => draw_line_ghost(&mut g, s, &hud, hover.0),
        Tool::Canal => draw_canal_ghost(&mut g, s, &hud, hover.0),
        Tool::Harvest(_) => match (hud.drag_from, hover.0) {
            (Some(a), Some(b)) => area_outline(&mut g, h, sim::work::Area { min: (a.0.min(b.0), a.1.min(b.1)), max: (a.0.max(b.0), a.1.max(b.1)) }, ACCENT),
            (None, Some(c)) => ring(&mut g, cell_at(h, c, 0.2), 0.45, ACCENT),
            _ => {}
        },
    }
    draw_sight(&mut g, s, drawn.as_deref(), &hud, hover.0, *sel, thing.0);
    draw_dam_wear(&mut g, s);
}

fn shoreline(g: &mut Gizmos, h: &sim::grid::HeightGrid, cells: &[Cell], y: f32, col: Color) {
    let set: HashSet<Cell> = cells.iter().copied().collect();
    for &c in cells {
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            if usize::try_from(c.0 as i64 + dx).ok().zip(usize::try_from(c.1 as i64 + dy).ok()).is_some_and(|n| set.contains(&n)) { continue; }
            let ((cx, cz), (ox, oz)) = ((c.0 as f32 + dx as f32 * 0.5, c.1 as f32 + dy as f32 * 0.5), (dy as f32 * 0.5, dx as f32 * 0.5));
            g.line(cell_world(h, (cx - ox * 0.6, cz - oz * 0.6)).with_y(y), cell_world(h, (cx + ox * 0.2, cz + oz * 0.2)).with_y(y), col);
        }
    }
}

fn draw_line_corners(g: &mut Gizmos, h: &sim::grid::HeightGrid, hud: &HudState, hover: Option<Cell>, double: bool) {
    for &c in &hud.corners {
        let p = cell_at(h, c, 0.15);
        if double { double_ring(g, p, 0.3, 0.22, 0.0, ACCENT); } else { ring(g, p, 0.3, ACCENT); }
    }
    match (hud.corners.last(), hover) {
        (Some(&last), Some(to)) if hud.drag_from.is_none() && last != to => {
            let (a, b) = (cell_at(h, last, 0.3), cell_at(h, to, 0.3));
            let n = ((a - b).length() * 2.0).ceil().max(1.0) as usize;
            for i in (0..n).step_by(2) { g.line(a.lerp(b, i as f32 / n as f32), a.lerp(b, ((i + 1) as f32 / n as f32).min(1.0)), Color::WHITE.with_alpha(0.45)); }
        }
        (None, Some(to)) => ring(g, cell_at(h, to, 0.15), 0.3, Color::WHITE.with_alpha(0.7)),
        _ => {}
    }
}

fn draw_line_ghost(g: &mut Gizmos, s: &sim::SimState, hud: &HudState, hover: Option<Cell>) {
    let h = &s.height;
    if let Some(pv) = hud.dam_preview() {
        if let Some(r) = hud.dam_settled().filter(|r| !r.water.flooded.is_empty()) {
            shoreline(g, h, &r.water.flooded, r.water.level + 0.06, FOAM);
            for &t in &r.trees { double_ring(g, cell_at(h, t, 0.25), 0.55, 0.5, 0.04, WATER); }
        }
        if let Some((_, ids)) = hud.drowned.as_ref().filter(|d| d.0 == hud.line) {
            for b in ids.iter().filter_map(|id| s.buildings.building(*id)) { footprint_outline(g, s, b, BAD); }
        }
        if let Some(c) = pv.spill_at.filter(|c| !hud.line.contains(c)) {
            ring(g, cell_y(h, c, pv.level.max(height_at(h, c.0 as f32 + 0.5, c.1 as f32 + 0.5)) + 0.1), 0.6, BAD);
        }
    }
    draw_line_corners(g, h, hud, hover, true);
}

fn draw_canal_ghost(g: &mut Gizmos, s: &sim::SimState, hud: &HudState, hover: Option<Cell>) {
    let h = &s.height;
    let square = |g: &mut Gizmos, c: Cell, r: f32, col: Color| {
        let p = cell_at(h, c, 0.12);
        let corners = [Vec3::new(-r, 0.0, -r), Vec3::new(r, 0.0, -r), Vec3::new(r, 0.0, r), Vec3::new(-r, 0.0, r)].map(|o| p + o);
        for i in 0..4 { g.line(corners[i], corners[(i + 1) % 4], col); }
    };
    match hud.canal_plan.as_ref().filter(|p| p.0 == hud.line).map(|p| &p.1) {
        Some(Ok(p)) => {
            for (i, c) in p.cells.iter().enumerate() {
                let d = p.expected.get(i).copied().unwrap_or(0.0);
                let col = if p.too_deep.contains(&c.cell) { BAD } else if d >= sim::canals::FLOAT_DEPTH { WATER } else if d >= sim::canals::WET_DEPTH { Color::srgb(0.6, 0.88, 0.92) } else { Color::srgb(0.85, 0.72, 0.52) };
                square(g, c.cell, 0.36, col);
                square(g, c.cell, 0.3, col);
            }
            double_ring(g, cell_at(h, p.gate, 0.2), 0.45, 0.38, 0.04, WOOD_LINE);
        }
        _ => for &c in &hud.line { square(g, c, 0.34, Color::WHITE.with_alpha(0.6)); },
    }
    draw_line_corners(g, h, hud, hover, false);
}

fn draw_dam_wear(g: &mut Gizmos, s: &sim::SimState) {
    let h = &s.height;
    for d in s.buildings.buildings.iter().filter_map(|b| b.dam.as_deref()) {
        let line: Vec<_> = d.line().collect();
        for w in line.windows(2) {
            if sim::dams::built(s, w[0].cell) <= 1e-3 || sim::dams::built(s, w[1].cell) <= 1e-3 {
                g.line(cell_at(h, w[0].cell, 0.95), cell_at(h, w[1].cell, 0.95), WOOD_LINE);
            }
        }
        for c in &d.cells {
            let built = sim::dams::built(s, c.cell);
            if built <= 0.0 || sim::dams::integrity(s, c.cell) >= sim::dams::REPAIR_BELOW { continue; }
            let p = cell_at(h, c.cell, built * 0.7);
            let dir = Vec3::new(d.downstream.0 as f32, 0.0, d.downstream.1 as f32);
            let side = Vec3::new(-dir.z, 0.0, dir.x);
            for k in [-0.25f32, 0.0, 0.25] {
                let a = p + dir * 0.3 + side * k * 0.5;
                let m = a + dir * 0.35 + side * k * 0.6 + Vec3::Y * 0.12;
                g.line(a, m, FOAM);
                g.line(m, m + dir * 0.3 + side * k * 0.4 - Vec3::Y * (built * 0.7 + 0.1), FOAM);
            }
        }
    }
}

fn ring(g: &mut Gizmos, at: Vec3, r: f32, col: Color) {
    for i in 0..18 {
        let (a0, a1) = (i as f32 / 18.0 * std::f32::consts::TAU, (i + 1) as f32 / 18.0 * std::f32::consts::TAU);
        g.line(at + Vec3::new(r * a0.cos(), 0.0, r * a0.sin()), at + Vec3::new(r * a1.cos(), 0.0, r * a1.sin()), col);
    }
}

fn double_ring(g: &mut Gizmos, at: Vec3, r1: f32, r2: f32, dy: f32, col: Color) {
    ring(g, at, r1, col);
    ring(g, at + Vec3::Y * dy, r2, col);
}

fn draw_dam(g: &mut Gizmos, s: &sim::SimState, hud: &HudState, id: BuildingId, d: &sim::dams::Dam) {
    let h = &s.height;
    let line: Vec<Cell> = d.line().map(|c| c.cell).collect();
    for w in line.windows(2) {
        for k in 0..4 {
            let off = Vec3::new(0.0, k as f32 * 0.04, (k as f32 - 1.5) * 0.06);
            let a = cell_world(h, (w[0].0 as f32, w[0].1 as f32)).with_y(works_top(s, w[0])) + off;
            let b = cell_world(h, (w[1].0 as f32, w[1].1 as f32)).with_y(works_top(s, w[1])) + off;
            g.line(a, b, ACCENT.with_alpha(0.85));
        }
    }
    for c in &d.cells {
        if sim::dams::built(s, c.cell) > 0.0 && sim::dams::integrity(s, c.cell) < sim::dams::REPAIR_BELOW {
            double_ring(g, cell_y(h, c.cell, works_top(s, c.cell)), 0.62, 0.56, 0.04, BAD);
        }
        if hud.dam_cells.contains(&c.cell) { cell_outline(g, h, c.cell, Some(works_top(s, c.cell) + 0.12), SELECT); }
    }
    if let Some((_, o)) = hud.opening.filter(|p| p.0 == id) {
        for &c in &line {
            if super::works::opening_fits(s, id, c, o).is_ok() {
                cell_outline(g, h, c, Some(works_top(s, c) + 0.1), GOOD);
                cell_outline(g, h, c, Some(works_top(s, c) + 0.14), GOOD.with_alpha(0.6));
            } else {
                cell_outline(g, h, c, Some(works_top(s, c) + 0.1), Color::WHITE.with_alpha(0.15));
            }
        }
    }
    for &end in [line.first(), line.last()].into_iter().flatten() {
        let at = cell_y(h, end, works_top(s, end) + 0.08);
        for r in [0.34, 0.28, 0.12] { ring(g, at, r, ACCENT); }
    }
    if let Some(p) = &d.pond { shoreline(g, h, &p.flooded, p.full_level + 0.05, WATER.with_alpha(0.8)); }
    if let Some((_, _, ghost)) = hud.dam_ghost.as_ref().filter(|gh| gh.0 == id) {
        for &c in ghost { cell_outline(g, h, c, Some(s.height.get(c.0, c.1).copied().unwrap_or(0.0) + 0.6), WOOD_LINE.with_alpha(0.8)); }
    }
}

pub fn mood_icon(m: f32) -> (Icon, Color) {
    if m >= 0.66 { (Icon::MoodGood, GOOD) } else if m >= 0.4 { (Icon::MoodOk, WARN) } else { (Icon::MoodBad, BAD) }
}

pub fn cell_rows(s: &sim::SimState, (x, y): Cell) -> Vec<(Icon, Color, &'static str, String)> {
    let (h, d) = (s.height.get(x, y).copied().unwrap_or(0.0), s.water.depth.get(x, y).copied().unwrap_or(0.0));
    let mut rows = vec![(Icon::ViewElevation, MUTED, "Height", num(h))];
    if d > 0.01 {
        rows.push((Icon::Drop, WATER, "Depth", format!("{d:.2} m")));
        rows.push((Icon::Crest, WATER, "Level", format!("{:.2} m", h + d)));
    }
    rows
}

fn order_cell(s: &sim::SimState, order: sim::agents::Order) -> Option<Cell> {
    use sim::agents::{Assignment, Order};
    match order {
        Order::Do(Assignment::Go(c)) => Some(c),
        Order::Do(Assignment::Resource(id)) => s.resources.get(id).map(|r| r.cell),
        Order::Do(Assignment::Area(_, a)) => Some(((a.min.0 + a.max.0) / 2, (a.min.1 + a.max.1) / 2)),
        Order::Do(Assignment::Slot(b, _)) | Order::Work(b) => s.buildings.building(b).map(|b| b.pos),
        Order::Do(Assignment::Mend | Assignment::Build) => None,
    }
}
