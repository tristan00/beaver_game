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
use sim::buildings::{BuildingId, Good};
use sim::colony::{BeaverId, LodgeId};
use sim::resources::ResourceId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Thing {
    Beaver(BeaverId),
    Building(BuildingId),
    Resource(ResourceId),
    Lodge(LodgeId),
    Cell(Cell),
    Wolf(u64),
    Animal(u32),
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
    fn from(target: crate::picking::Target) -> Self {
        use crate::picking::Target;
        match target {
            Target::Building(id) => Self::Building(id),
            Target::Beaver(id) => Self::Beaver(id),
            Target::Lodge(id) => Self::Lodge(id),
            Target::Resource(id) => Self::Resource(id),
        }
    }
}

#[derive(Resource, Default)]
/// What is under the pointer, and what a right-click order goes to: the same, except that a
/// tree picked by its crown gives way to the ground behind it, so crowns never swallow moves.
pub struct HoverThing(pub Option<Thing>, pub Option<Thing>);

#[derive(Resource, Default)]
pub struct HoverPoint(pub Option<(Cell, (f32, f32))>);

/// While a dam or canal is selected, the cell along it the pointer is on: the one nearest its
/// line of sight, within about a cell, however the sticks catch the ray. A click picks it.
#[derive(Resource, Default, PartialEq)]
pub struct WorksSnap(pub Option<Cell>);

/// How far from the pointer's line of sight (m) a dam or canal cell can still be picked.
const SNAP_M: f32 = 0.9;

/// Where a dam's or canal's cell is pointed at: the top of what stands there, or the water on it.
pub fn works_top(s: &sim::SimState, c: Cell) -> f32 {
    let ground = s.height.get(c.0, c.1).copied().unwrap_or(0.0);
    let built = sim::dams::built(s, c);
    let water = ground + s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
    (ground + built).max(water) + 0.12
}

/// The cell of the dam's line, or of the canal, nearest a line of sight, if close enough.
fn works_cell_on_ray(s: &sim::SimState, id: BuildingId, origin: Vec3, dir: Vec3) -> Option<Cell> {
    let cells: Vec<Cell> = if let Some(d) = sim::dams::dam(s, id) {
        d.line().map(|c| c.cell).collect()
    } else {
        sim::canals::canal(s, id)?.cells.iter().map(|c| c.cell).collect()
    };
    cells
        .into_iter()
        .filter_map(|c| {
            let p = cell_world(&s.height, (c.0 as f32, c.1 as f32)).with_y(works_top(s, c));
            let t = (p - origin).dot(dir);
            (t > 0.0).then(|| (c, (origin + dir * t).distance(p)))
        })
        .filter(|x| x.1 < SNAP_M)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|x| x.0)
}

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

/// The nearest wild animal the fog does not hide that the ray goes through, roughly by its
/// body, and how far along the ray it is.
fn animal_on_ray(s: &sim::SimState, ray: Ray3d) -> Option<(u32, f32)> {
    use sim::wildlife::Kind;
    s.wildlife
        .animals
        .iter()
        .filter(|a| !crate::fog_render::unexplored(s, (a.pos.0.max(0.0) as usize, a.pos.1.max(0.0) as usize)))
        .filter_map(|a| {
            // As drawn: a deer about a cell long, rabbits and birds a third of that.
            let size = match a.kind {
                Kind::Deer => 1.0,
                Kind::Rabbit => 0.32,
                Kind::Bird => 0.36,
            };
            let at = cell_world(&s.height, a.pos) + Vec3::Y * size * 0.6;
            let t = (at - ray.origin).dot(*ray.direction);
            (t > 0.0 && (ray.origin + *ray.direction * t).distance(at) < (size * 0.7).max(0.5)).then_some((a.id, t))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

pub fn pick(s: &sim::SimState, cell: Cell, p: (f32, f32)) -> Thing {
    let d2 = |a: (f32, f32)| (a.0 - p.0).powi(2) + (a.1 - p.1).powi(2);
    let near = s.colony.beavers.iter().filter_map(|b| beaver_at(s, b.id).map(|q| (b.id, d2(q)))).filter(|x| x.1 < 0.9 * 0.9).min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((id, _)) = near {
        return Thing::Beaver(id);
    }
    if let Some(w) = s.predators.wolves.iter().filter(|w| d2(w.pos) < 1.2 * 1.2).min_by(|a, b| d2(a.pos).total_cmp(&d2(b.pos))) {
        return Thing::Wolf(w.key());
    }
    if let Some(b) = s.buildings.buildings.iter().find(|b| b.cells().any(|c| c == cell)) {
        return Thing::Building(b.id);
    }
    if let Some(r) = s.resources.at(cell).find(|r| r.amount > 0.0 || r.kind.flowing()) {
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
            // Its job, then what it is doing about it now.
            if let Some((_, _, role)) = s.work.slot_of(id) {
                tip.push(TipLine::new(role.name(), "").icon(job_icon(role), INK));
            }
            let (icon, col, doing) = super::inspect::activity(s, b);
            tip.push(TipLine::new(doing, "").icon(icon, col));
            if let Some(a) = s.agents.get(id) {
                if let Some(l) = &a.load {
                    tip.push(TipLine::new(l.good.name(), num(l.amount)).icon(good_icon(l.good), good_color(l.good)));
                    let walk = sim::agents::travel_pace(&s.nav, a);
                    if (walk - 1.0).abs() > 0.005 {
                        tip.push(TipLine::new("Walk", format!("×{walk:.2}")).icon(Icon::Walk, MUTED));
                    }
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
                if let Some(i) = icon {
                    l = l.icon(i, col);
                }
                tip.push(l);
                for g in goods {
                    tip.push(TipLine::new(g.name(), "").icon(good_icon(g), good_color(g)).sub());
                }
            }
            if let Some(pl) = s.work.place(id).filter(|p| !p.slots.is_empty()) {
                let filled = pl.slots.iter().filter(|x| x.beaver.is_some()).count();
                let seats = if pl.open_ended() { String::new() } else { format!("/{}", pl.slots.len()) };
                tip.push(TipLine::new("Beavers", format!("{filled}{seats}")).icon(job_icon(pl.slots[0].role), INK));
            }
            if let Some(f) = s.output.of(id) {
                for (g, r) in f.rates() {
                    tip.push(TipLine::new(g.name(), format!("{}/d", num(r))).icon(good_icon(g), good_color(g)).color(GOOD));
                }
            }
            tip
        }
        Thing::Resource(rid) => {
            let Some(r) = s.resources.get(rid) else { return Tip::default() };

            let mut tip = Tip::titled(Some((resource_icon(r.kind), GOOD)), r.plant.map(|p| p.name()).or(r.species.map(|t| t.name())).unwrap_or(resource_name(r.kind)));
            // Trees: age, chewing left, and what felling gives; trunks: what is left to haul.
            let yields = match r.kind {
                k if k.is_tree() => {
                    tip.push(TipLine::new("Age", days(r.age_days)).icon(Icon::Hourglass, MUTED));
                    tip.push(TipLine::new("Fell work", num(r.amount)).icon(Icon::Hammer, MUTED));
                    r.species.and_then(|sp| r.stage()?.fell_yield(sp))
                }
                k if k.is_felled() => Some((r.amount, r.branches, r.bark)),
                _ => {
                    tip = tip.with_value(num(r.amount));
                    None
                }
            };
            if let Some((logs, branches, bark)) = yields {
                for (name, icon, col, v) in [(sim::resources::ResourceKind::felled().yields()[0].name(), good_icon(sim::resources::ResourceKind::felled().yields()[0]), good_color(sim::resources::ResourceKind::felled().yields()[0]), logs + branches), ("Bark", good_icon(Good::food()), good_color(Good::food()), bark)] {
                    if v > 0.0 {
                        tip.push(TipLine::new(name, num(v)).icon(icon, col));
                    }
                }
            }
            // The area of a building that works this kind of thing, not just any area over it.
            let works = |p: &sim::work::Workplace| p.slots.iter().any(|sl| match sl.role.duty() {
                sim::content::Duty::Gather => sim::jobs::gather_kind(sl.role).is_some_and(|k| k.gather_sources().contains(&r.kind)),
                sim::content::Duty::Forest => r.kind.is_tree(),
                _ => false,
            });
            if let Some(p) = s.work.places.iter().find(|p| p.area.is_some_and(|a| a.contains(r.cell)) && works(p)) {
                if let Some(b) = s.buildings.building(p.building) {
                    tip.push(TipLine::new(format!("{}'s area", b.kind.spec().name.as_str()), "").icon(Icon::Area, MUTED));
                }
            }
            if let Some(&(who, _)) = s.forestry.planted.get(&rid) {
                let name = s.colony.beaver(who).map_or_else(|| "a beaver now gone".to_string(), |b| b.name.clone());
                tip.push(TipLine::new("Planted by", name).icon(crate::hud::words::good_icon(sim::buildings::Good::seedling()), GOOD));
            }
            tip
        }
        Thing::Lodge(id) => {
            let n = s.colony.beavers.iter().filter(|b| b.lodge == Some(id)).count();
            Tip::titled(Some((crate::hud::words::building_icon(sim::buildings::BuildingKind::home()), INK)), sim::buildings::BuildingKind::home().name()).line("Residents", format!("{n}"))
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
            for (i, col, name, v) in cell_rows(s, c) {
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
    sel: Res<Selection>,
    mut snap: ResMut<WorksSnap>,
) {
    let _p = ::sim::prof::scope("game/world hover");
    let mut model = None;
    let mut on_works_cell = None;
    let p = (|| {
        if over.0 || menu.0 || cli.report_dir.is_some() {
            return None;
        }
        let cursor = windows.single().ok()?.cursor_position()?;
        let (cam, gt) = cams.single().ok()?;
        let ray = cam.viewport_to_world(gt, cursor).ok()?;
        if let Selection::Site(id) = *sel {
            on_works_cell = works_cell_on_ray(&sim.0, id, ray.origin, *ray.direction);
        }
        let ground = crate::flow::ray_surface(ray.origin, *ray.direction, &sim.0.height, &sim.0.water.depth);
        let hit = picker.cast_at(ray, ground.as_ref().map(|g|g.2));
        model = hit.map(|(t, _)| Thing::from(t));
        if let Some((id, t)) = animal_on_ray(&sim.0, ray) {
            if hit.is_none_or(|(_, d)| t < d) {
                model = Some(Thing::Animal(id));
            }
        }
        // On a dam or canal the cell is the one the pointer touches on the structure, not the
        // ground the ray would reach behind it, so its ends can be grabbed.
        let on_works = hit.and_then(|(t, d)| match t {
            crate::picking::Target::Building(id) => sim.0.buildings.building(id).filter(|b| b.kind.is_waterworks()).map(|_| d),
            _ => None,
        });
        if let Some(d) = on_works {
            let p = ray.origin + *ray.direction * d;
            let (x, y) = crate::flow::world_cell(&sim.0.height, p);
            let n = sim.0.world.size as f32;
            (x >= 0.0 && y >= 0.0 && x < n && y < n).then_some(((x as usize, y as usize), (x, y)))
        } else {
            ground.map(|(c, hit, _)| (c, crate::flow::world_cell(&sim.0.height, hit)))
        }
    })();
    if point.0 != p {
        point.0 = p;
    }
    if snap.0 != on_works_cell {
        snap.0 = on_works_cell;
    }
    // Nothing in the fog of war can be pointed at.
    if p.is_some_and(|(c, _)| crate::fog_render::unexplored(&sim.0, c)) {
        model = None;
    }
    let ground = p.filter(|(c, _)| !crate::fog_render::unexplored(&sim.0, *c)).map(|(c, p)| pick(&sim.0, c, p));
    let t = model.or(ground);
    let order = if matches!(model, Some(Thing::Resource(_) | Thing::Animal(_))) { ground } else { t };
    if thing.0 != t || thing.1 != order {
        thing.0 = t;
        thing.1 = order;
    }
    if let (Some((c, _)), Some(pile)) = (p, sim.0.work.supplies) {
        if c == pile && hud.tool == Tool::None && matches!(t, Some(Thing::Cell(_))) {
            let mut tip = Tip::titled(Some((Icon::Haul, INK)), "Supplies");
            for g in Good::all() {
                let a = sim.0.work.stock_of(sim::agents::SUPPLIES, g);
                if a > 0.05 {
                    tip.push(TipLine::new(g.name(), num(a)).icon(good_icon(g), good_color(g)));
                }
            }
            tips.world = Some((u64::MAX, tip));
            return;
        }
    }
    if let Some((c, _)) = p {
        let here: Vec<&sim::piles::Pile> = sim.0.work.piles.items.iter().filter(|q| q.cell == c).collect();
        if let (Some(first), Tool::None, Some(Thing::Cell(_))) = (here.first(), hud.tool, t) {
            let mut tip = Tip::titled(Some((good_icon(first.good), good_color(first.good))), "On the ground");
            for q in &here {
                tip.push(TipLine::new(q.good.name(), num(q.amount)).icon(good_icon(q.good), good_color(q.good)));
            }
            let left = here.iter().map(|q| q.days_left(sim.0.time_days)).fold(f64::INFINITY, f64::min);
            tip.push(TipLine::new("Rots in", format!("{left:.0} d")));
            tips.world = Some((u64::MAX - 1 - first.id, tip));
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
pub fn markers(s: &sim::SimState, cam: &Camera, gt: &GlobalTransform, scale: f32, hud: &HudState, sel: Selection, problems: &[sim::hud::Problem], pops: &[super::Pop], now: f32, wolf_at: Option<Vec3>) -> Vec<VNode> {
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
        // The wolf's pin rides over the wolf as drawn, not its cell, so it doesn't hop, and only
        // while the colony sees it: out of sight, where it is stays unknown.
        let at = match (p.kind, wolf_at) {
            (sim::hud::ProblemKind::Wolf, Some(w)) => project(w, 2.4),
            (sim::hud::ProblemKind::Wolf, None) => None,
            _ => place((f.0 as f32 + 0.5, f.1 as f32 + 0.5), 2.4),
        };
        let Some(pos) = at else { continue };
        let (icon, col, _, _, skin) = super::panels::alert_token(s, p);
        let mut v = pin(pos, icon, col, skin, 34.0);
        let act = match p.fix {
            sim::hud::Fix::OpenDam { .. } => super::HudAction::Fix(p.fix, p.focus),
            _ => super::HudAction::Fix(sim::hud::Fix::Focus, p.focus),
        };
        v.action(act).tip(super::panels::alert_tip_in(s, p)).pin();
        out.push(v);
    }
    for b in s.buildings.buildings.iter().filter(|b| !b.kind.is_dam()) {
        let selected = matches!(sel, Selection::Site(id) if id == b.id);
        let mut icon: Option<(Icon, Color, &str)> = None;
        if let Some(pl) = s.work.place(b.id) {
            let empty = pl.slots.iter().filter(|x| x.beaver.is_none()).count();
            if pl.is_open_site() && selected && !pl.paused && empty == pl.slots.len() && !pl.slots.is_empty() {
                icon = Some((crate::hud::words::job_icon(sim::jobs::Job::with_duty(sim::content::Duty::Build)), WARN, "No builder"));
            } else if !pl.paused && pl.filled() > 0 && pl.slots.iter().filter_map(|s| s.beaver).all(|id| s.agents.get(id).is_some_and(|a| a.stuck_ticks > 96)) {
                icon = Some((Icon::Blocked, BAD, "Beavers can't get there"));
            }
        }
        if b.is_complete() && s.buildings.stranded(b, &s.water.depth) {
            icon = Some((Icon::Flood, WARN, "High and dry"));
        }
        if sim::buildings::flooded(b, &s.water.depth) {
            icon = Some((Icon::Flood, BAD, "Flooded"));
        }
        // The dam being drawn would drown it.
        if hud.tool == super::Tool::Dam && hud.drowned.as_ref().is_some_and(|d| d.0 == hud.line && d.1.contains(&b.id)) {
            icon = Some((Icon::Flood, BAD, "This dam would flood it"));
        }
        if let (Some((i, c, words)), Some(pos)) = (icon, place(building_centre(b), 2.2)) {
            let mut v = pin(pos, i, c, Skin::Pebble, 28.0);
            v.action(super::HudAction::SelectBuilding(b.id)).tip(Tip::titled(Some((building_icon(b.kind), INK)), b.kind.spec().name.as_str()).line(words, "")).pin();
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
    // Pops at a cell float in the world; pops at the cursor (refusals) are drawn above every
    // panel by `screen_pops`, or a refusal over a panel would hide behind it.
    for p in pops {
        if let super::PopAt::Cell(c) = p.at {
            if let Some(at) = place(c, 2.0) {
                out.push(pop_node(p, at, now));
            }
        }
    }
    out
}

/// The water lens: over each dam's pond, how high it stands against the water under the
/// pointer (against the lowest pond in view when the pointer isn't on water), so what joining
/// two of them would do reads at a glance.
pub fn pond_levels(s: &sim::SimState, cam: &Camera, gt: &GlobalTransform, scale: f32, hover: Option<Cell>) -> Vec<VNode> {
    if crate::terrain_render::map_lens() != crate::terrain_render::WATER_LENS {
        return Vec::new();
    }
    let n = s.height.width() as i32;
    let depth = |c: Cell| s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
    let surface = |c: Cell| s.height.get(c.0, c.1).copied().unwrap_or(0.0) + depth(c);
    let wet = |c: Cell| depth(c) > sim::buildings::WET_DEPTH;
    // Each pond's level: the water against its dam's upstream face, and where to show it.
    let mut ponds: Vec<(f32, Vec2)> = Vec::new();
    for d in s.buildings.buildings.iter().filter_map(|b| b.dam.as_deref()) {
        let on_dam: std::collections::HashSet<Cell> = d.cells.iter().map(|c| c.cell).collect();
        let (ux, uy) = (-d.downstream.0, -d.downstream.1);
        let face: Vec<Cell> = d.line().filter_map(|c| {
            (1..=d.rows as i32 + 2).map(|k| (c.cell.0 as i32 + ux * k, c.cell.1 as i32 + uy * k)).find(|&(x, y)| x < 0 || y < 0 || x >= n || y >= n || !on_dam.contains(&(x as usize, y as usize)))
                .filter(|&(x, y)| x >= 0 && y >= 0 && x < n && y < n).map(|(x, y)| (x as usize, y as usize))
        }).filter(|&c| wet(c)).collect();
        if face.is_empty() {
            continue;
        }
        let level = face.iter().map(|&c| surface(c)).sum::<f32>() / face.len() as f32;
        // Over the middle of the pond's wet cells, or the middle of its wet face.
        let flooded: Vec<Cell> = d.pond.iter().flat_map(|p| p.flooded.iter().copied()).filter(|&c| wet(c)).collect();
        let around = if flooded.is_empty() { &face } else { &flooded };
        let (sx, sy) = around.iter().fold((0.0, 0.0), |a, c| (a.0 + c.0 as f32, a.1 + c.1 as f32));
        let centre = (sx / around.len() as f32, sy / around.len() as f32);
        let mid = *around.iter().min_by(|a, b| {
            let d = |c: &&Cell| (c.0 as f32 - centre.0).powi(2) + (c.1 as f32 - centre.1).powi(2);
            d(a).total_cmp(&d(b))
        }).unwrap_or(&face[0]);
        let mut w = cell_world(&s.height, (mid.0 as f32 + 0.5, mid.1 as f32 + 0.5));
        w.y = level + 1.2;
        let Some(at) = cam.world_to_viewport(gt, w).ok().map(|v| v / scale.max(0.01)) else { continue };
        // Two dams on one pond show it once.
        if ponds.iter().any(|p| (p.0 - level).abs() < 0.05 && p.1.distance(at) < 90.0) {
            continue;
        }
        ponds.push((level, at));
    }
    let pointed = hover.filter(|&c| wet(c)).map(surface);
    let Some(reference) = pointed.or_else(|| ponds.iter().map(|p| p.0).reduce(f32::min)) else { return Vec::new() };
    if pointed.is_none() && ponds.len() < 2 {
        return Vec::new();
    }
    ponds.iter().map(|&(level, at)| {
        let dz = level - reference;
        let even = dz.abs() < 0.05;
        let mut v = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(at.x - 26.0), top: Val::Px(at.y - 14.0), padding: UiRect::axes(Val::Px(7.0), Val::Px(3.0)), ..row_node(3.0) });
        let mut ui = Ui::default();
        if !even {
            ui.icon(if dz > 0.0 { Icon::Up } else { Icon::Down }, 12.0, if dz > 0.0 { ACCENT } else { WATER });
        }
        ui.bold(if even { "0 m".to_string() } else { format!("{} m", num(dz.abs())) }, 12.0, INK);
        v.children = ui.nodes;
        v.skin(Skin::Plate);
        v.tip(Tip::name(if pointed.is_some() { "Pond level, against the water pointed at" } else { "Pond level, against the lowest pond" })).click_through();
        v
    }).collect()
}

/// Pops at a screen point (refusals by the cursor), for the topmost layer.
pub fn screen_pops(pops: &[super::Pop], now: f32) -> Vec<VNode> {
    pops.iter().filter_map(|p| match p.at {
        super::PopAt::Screen(at) => Some(pop_node(p, at, now)),
        _ => None,
    }).collect()
}

fn pop_node(p: &super::Pop, at: Vec2, now: f32) -> VNode {
    let age = (now - p.born).max(0.0);
    let fade = (1.0 - age / super::POP_SECS).clamp(0.0, 1.0);
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
    v
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

/// The drawn bed under a point: the ground, less the channel a river cuts.
fn bed_y(s: &sim::SimState, x: f32, y: f32) -> f32 {
    height_at(&s.height, x, y) - crate::terrain_render::channel_cut(s, x.round() as i32, y.round() as i32)
}

/// Depth lines on the bed near the view at the water lens's band edges, and the shoreline, by
/// marching squares over cell centres.
fn draw_contours(g: &mut Gizmos, s: &sim::SimState, (tx, ty): (usize, usize), r: usize) {
    let n = s.world.size;
    let depth = |x: usize, y: usize| s.water.depth.get(x, y).copied().unwrap_or(0.0);
    let line = Color::srgba(0.05, 0.18, 0.35, 0.55);
    let (near, max) = crate::terrain_render::flood_depths();
    let mut levels = vec![(crate::terrain_render::SHORE_DEPTH, Color::srgba(1.0, 1.0, 1.0, 0.9)), (sim::raft_landing::RAFT_DEPTH, line), (near, line), (max, line)];
    levels.extend(crate::terrain_render::DEPTH_BANDS.iter().map(|b| b.0).filter(|&d| d.is_finite() && d < near).map(|d| (d, line)));
    let point = |x: f32, y: f32| {
        let mut v = cell_world(&s.height, (x, y));
        v.y = bed_y(s, x, y) + 0.05;
        v
    };
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
                if k >= 2 {
                    g.line(hits[0], hits[1], colour);
                }
                if k == 4 {
                    g.line(hits[2], hits[3], colour);
                }
            }
        }
    }
}

/// A building's footprint traced on the ground (or the water over it), a cell edge at a time.
fn footprint_outline(g: &mut Gizmos, s: &sim::SimState, b: &sim::buildings::Building, colour: Color) {
    let (w, d) = b.kind.spec().footprint;
    let (x0, y0) = (b.pos.0 as f32, b.pos.1 as f32);
    let at = |x: f32, y: f32| {
        let c = (x.max(0.0) as usize, y.max(0.0) as usize);
        let wet = s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
        cell_world(&s.height, (x, y)).with_y(height_at(&s.height, x, y) + wet + 0.12)
    };
    let mut edge = |a: (f32, f32), b: (f32, f32)| g.line(at(a.0, a.1), at(b.0, b.1), colour);
    for i in 0..w {
        let x = x0 + i as f32;
        edge((x, y0), (x + 1.0, y0));
        edge((x, y0 + d as f32), (x + 1.0, y0 + d as f32));
    }
    for j in 0..d {
        let y = y0 + j as f32;
        edge((x0, y), (x0, y + 1.0));
        edge((x0 + w as f32, y), (x0 + w as f32, y + 1.0));
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

/// A forester's reach: the dashed circle that fits its area (see `Area::reaches`).
fn round_outline(g: &mut Gizmos, h: &sim::grid::HeightGrid, a: sim::work::Area, colour: Color) {
    let (rx, ry) = ((a.max.0 - a.min.0) as f32 / 2.0 + 0.5, (a.max.1 - a.min.1) as f32 / 2.0 + 0.5);
    let (cx, cy) = ((a.min.0 + a.max.0) as f32 / 2.0, (a.min.1 + a.max.1) as f32 / 2.0);
    let n = 96;
    let p = |i: usize| {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let q = (cx + rx * t.cos(), cy + ry * t.sin());
        let mut v = cell_world(h, q);
        v.y = height_at(h, q.0, q.1) + 0.15;
        v
    };
    for i in (0..n).step_by(2) {
        g.line(p(i), p(i + 1), colour);
    }
}

/// A sight range: a dashed circle of radius `r` cells round `at` (cell coordinates), laid on
/// the ground.
fn sight_circle(g: &mut Gizmos, h: &sim::grid::HeightGrid, at: (f32, f32), r: f32, colour: Color) {
    let n = ((r * 4.0) as usize).clamp(48, 400) & !1;
    let p = |i: usize| {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let q = (at.0 + r * t.cos(), at.1 + r * t.sin());
        cell_world(h, q).with_y(height_at(h, q.0, q.1) + 0.2)
    };
    for i in (0..n).step_by(2) {
        g.line(p(i), p(i + 1), colour);
    }
}

/// The middles of the lookouts whose range shows: hovered, selected, or being placed.
fn shown_lookouts(s: &sim::SimState, hud: &HudState, hover: Option<Cell>, sel: Selection, thing: Option<Thing>) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = Vec::new();
    for id in [match sel { Selection::Site(id) => Some(id), _ => None }, match thing { Some(Thing::Building(id)) => Some(id), _ => None }].into_iter().flatten() {
        if let Some(b) = s.buildings.building(id).filter(|b| b.kind.watches()) {
            out.push(sim::predators::lookout_centre(b));
        }
    }
    if let (Tool::Building(kind), Some(c)) = (hud.tool, hover) && kind.watches() {
        let b = sim::buildings::Building { id: BuildingId(0), kind, pos: c, status: sim::buildings::Status::Site, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None };
        out.push(sim::predators::lookout_centre(&b));
    }
    out.dedup();
    out
}

/// A lookout's range is wide (more than a screen across at the usual zoom), so besides its
/// rim the ground it covers is washed over: one disc laid on the ground and water, redrawn
/// only when the lookout shown changes.
#[allow(clippy::too_many_arguments)]
pub fn lookout_range(
    mut commands: Commands,
    sim: Res<Sim>,
    hud: Res<HudState>,
    hover: Res<HoverCell>,
    sel: Res<Selection>,
    thing: Res<HoverThing>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shown: Local<Option<(Entity, (i32, i32))>>,
) {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let s = &sim.0;
    let want = shown_lookouts(s, &hud, hover.0, *sel, thing.0).first().copied();
    let key = want.map(|c| ((c.0 * 2.0) as i32, (c.1 * 2.0) as i32));
    if shown.map(|x| x.1) == key && (key.is_none() || shown.is_some()) {
        return;
    }
    if let Some((e, _)) = shown.take() {
        commands.entity(e).despawn();
    }
    let (Some(at), Some(key)) = (want, key) else { return };
    let h = &s.height;
    let r = sim::predators::LOOKOUT_SIGHT;
    let (rings, segs) = (24usize, 120usize);
    let ground = |q: (f32, f32)| {
        let c = (q.0.max(0.0) as usize, q.1.max(0.0) as usize);
        let wet = s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0).max(0.0);
        cell_world(h, q).with_y(height_at(h, q.0, q.1) + wet + 0.3)
    };
    let mut pos: Vec<[f32; 3]> = vec![ground(at).to_array()];
    for i in 1..=rings {
        let rr = r * i as f32 / rings as f32;
        for j in 0..segs {
            let t = j as f32 / segs as f32 * std::f32::consts::TAU;
            pos.push(ground((at.0 + rr * t.cos(), at.1 + rr * t.sin())).to_array());
        }
    }
    let mut idx: Vec<u32> = Vec::new();
    let ring = |i: usize, j: usize| (1 + (i - 1) * segs + j % segs) as u32;
    for j in 0..segs {
        idx.extend([0, ring(1, j + 1), ring(1, j)]);
    }
    for i in 1..rings {
        for j in 0..segs {
            let (a, b, c, d) = (ring(i, j), ring(i, j + 1), ring(i + 1, j), ring(i + 1, j + 1));
            idx.extend([a, b, c, b, d, c]);
        }
    }
    let n = pos.len();
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n])
        .with_inserted_indices(Indices::U32(idx));
    let mat = materials.add(StandardMaterial { base_color: ACCENT.with_alpha(0.14), alpha_mode: AlphaMode::Blend, unlit: true, double_sided: true, cull_mode: None, depth_bias: 50.0, ..default() });
    let e = commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat), Transform::default(), bevy::light::NotShadowCaster)).id();
    *shown = Some((e, key));
}

/// Sight ranges to show: of a hovered or selected beaver, the wolf, and of a lookout hovered,
/// selected or being placed.
fn draw_sight(g: &mut Gizmos, s: &sim::SimState, hud: &HudState, hover: Option<Cell>, sel: Selection, thing: Option<Thing>) {
    let h = &s.height;
    let wolf_col = Color::srgb(0.92, 0.36, 0.3);
    let mut beavers: Vec<BeaverId> = Vec::new();
    if let Selection::Beaver(b) = sel {
        beavers.push(b);
    }
    if let Some(Thing::Beaver(b)) = thing {
        beavers.push(b);
    }
    beavers.dedup();
    for b in beavers {
        if let Some(at) = beaver_at(s, b) {
            sight_circle(g, h, at, sim::fog::BEAVER_SIGHT, Color::WHITE.with_alpha(0.75));
        }
    }
    for w in s.predators.wolves.iter().filter(|w| sel == Selection::Wolf(w.key()) || thing == Some(Thing::Wolf(w.key()))) {
        sight_circle(g, h, w.pos, sim::predators::WOLF_SIGHT, wolf_col);
    }
    for at in shown_lookouts(s, hud, hover, sel, thing) {
        // A solid rim, doubled so it holds up from far out (the area is washed by
        // `lookout_range`).
        let r = sim::predators::LOOKOUT_SIGHT;
        let n = 360;
        let p = |i: usize, dr: f32| {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            let q = (at.0 + (r + dr) * t.cos(), at.1 + (r + dr) * t.sin());
            let c = (q.0.max(0.0) as usize, q.1.max(0.0) as usize);
            let wet = s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0).max(0.0);
            cell_world(h, q).with_y(height_at(h, q.0, q.1) + wet + 0.35)
        };
        for i in 0..n {
            for dr in [0.0, -0.3] {
                g.line(p(i, dr), p(i + 1, dr), ACCENT);
            }
        }
    }
}

/// The water lens: short flow lines traced along the current from seeds across the view,
/// down to a centimetre of water, each ending in an arrowhead and coloured by speed (pale
/// blue for slow water, white for a steady current, orange for fast water). The water engine
/// moves water along each channel's thread; water spread beside it is drawn going the way of
/// the nearest thread, slower.
/// A current's arrow colour by its speed (m/s): dark blue slow, white steady, orange fast.
pub fn current_colour(speed: f32) -> Color {
    let (slow, mid, fast) = (Vec3::new(0.05, 0.16, 0.4), Vec3::new(1.0, 1.0, 1.0), Vec3::new(1.0, 0.55, 0.15));
    let c = if speed < 0.3 { slow.lerp(mid, speed / 0.3) } else { mid.lerp(fast, ((speed - 0.3) / 0.7).min(1.0)) };
    Color::srgb(c.x, c.y, c.z)
}

fn draw_currents(g: &mut Gizmos, s: &sim::SimState, (tx, ty): (f32, f32), distance: f32) {
    const MIN_DEPTH: f32 = 0.01;
    let h = &s.height;
    let n = s.world.size as i32;
    let r = (distance * 1.4).clamp(30.0, 120.0) as i32;
    let stride = if r <= 60 { 2 } else if r <= 95 { 3 } else { 4 };
    // The engine moves water only along each channel's thread; water beside it, a pond
    // included, takes the way of the nearest thread it is joined to, up to SPREAD cells off,
    // slower the farther it lies.
    const SPREAD: u16 = 24;
    let (cx0, cy0) = (tx as i32, ty as i32);
    let (x0, y0) = ((cx0 - r - 8).max(0), (cy0 - r - 8).max(0));
    let (x1, y1) = ((cx0 + r + 8).min(n), (cy0 + r + 8).min(n));
    let (ww, wh) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
    let wet = |x: i32, y: i32| s.water.depth.get(x as usize, y as usize).copied().unwrap_or(0.0) >= MIN_DEPTH;
    let mut field: Vec<Option<(Vec2, f32, u16)>> = vec![None; ww * wh];
    let mut queue = std::collections::VecDeque::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let Some(v) = s.water.velocity.get(x as usize, y as usize).map(|v| Vec2::new(v.0, v.1)).filter(|v| v.length() > 1e-4) else { continue };
            let i = (y - y0) as usize * ww + (x - x0) as usize;
            field[i] = Some((v.normalize(), v.length(), 0));
            queue.push_back((x, y));
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let Some((d, v, k)) = field[(y - y0) as usize * ww + (x - x0) as usize] else { continue };
        if k >= SPREAD {
            continue;
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx < x0 || ny < y0 || nx >= x1 || ny >= y1 || !wet(nx, ny) {
                continue;
            }
            let j = (ny - y0) as usize * ww + (nx - x0) as usize;
            if field[j].is_none() {
                field[j] = Some((d, v, k + 1));
                queue.push_back((nx, ny));
            }
        }
    }
    let current = |c: (i32, i32)| -> Option<(Vec2, f32)> {
        if c.0 < x0 || c.1 < y0 || c.0 >= x1 || c.1 >= y1 {
            return None;
        }
        field[(c.1 - y0) as usize * ww + (c.0 - x0) as usize].map(|(d, v, k)| (d, v / (1.0 + k as f32)))
    };
    // Open water (a pond, a wide reach): within OPEN steps of a cell OPEN steps from any
    // shore. There the way the water goes is plain, so it gets a third of the arrows; a
    // stream narrower than that keeps them all.
    const OPEN: u8 = 4;
    let open = {
        let mut dist = vec![OPEN; ww * wh];
        let mut q = std::collections::VecDeque::new();
        for y in y0..y1 {
            for x in x0..x1 {
                if !wet(x, y) {
                    let i = (y - y0) as usize * ww + (x - x0) as usize;
                    dist[i] = 0;
                    q.push_back((x, y));
                }
            }
        }
        let spread = |dist: &mut Vec<u8>, q: &mut std::collections::VecDeque<(i32, i32)>, grow: bool| {
            while let Some((x, y)) = q.pop_front() {
                let k = dist[(y - y0) as usize * ww + (x - x0) as usize];
                let next = if grow { k + 1 } else { k - 1 };
                if (grow && next >= OPEN) || (!grow && k <= 1) {
                    continue;
                }
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < x0 || ny < y0 || nx >= x1 || ny >= y1 || !wet(nx, ny) {
                        continue;
                    }
                    let j = (ny - y0) as usize * ww + (nx - x0) as usize;
                    if (grow && dist[j] > next) || (!grow && dist[j] < next) {
                        dist[j] = next;
                        q.push_back((nx, ny));
                    }
                }
            }
        };
        spread(&mut dist, &mut q, true);
        // Back out from the cells at least OPEN from shore, OPEN steps, over the water.
        let mut reach = vec![0u8; ww * wh];
        for (i, d) in dist.iter().enumerate() {
            if *d >= OPEN {
                reach[i] = OPEN;
                q.push_back(((i % ww) as i32 + x0, (i / ww) as i32 + y0));
            }
        }
        spread(&mut reach, &mut q, false);
        reach
    };
    let is_open = |x: i32, y: i32| x >= x0 && y >= y0 && x < x1 && y < y1 && open[(y - y0) as usize * ww + (x - x0) as usize] > 0;
    let colour = current_colour;
    let surface = |p: (f32, f32)| {
        let c = (p.0.max(0.0) as usize, p.1.max(0.0) as usize);
        let mut v = cell_world(h, p);
        v.y = h.get(c.0, c.1).copied().unwrap_or(0.0) + s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0) + 0.15;
        v
    };
    for y in ((cy0 - r).max(1)..(cy0 + r).min(n - 1)).filter(|y| y.rem_euclid(stride) == 0) {
        // Every other row half a step along, so short arrows don't line up into long lines.
        let shift = if (y / stride) % 2 == 1 { stride / 2 } else { 0 };
        for x in ((cx0 - r).max(1)..(cx0 + r).min(n - 1)).filter(|x| (x - shift).rem_euclid(stride) == 0) {
            if s.water.depth.get(x as usize, y as usize).copied().unwrap_or(0.0) < MIN_DEPTH {
                continue;
            }
            // In open water, every third row and every third seed along it, staggered.
            let (i, j) = ((x - shift) / stride, y / stride);
            if is_open(x, y) && (j % 3 != 0 || (i + j / 3) % 3 != 0) {
                continue;
            }
            let Some((_, speed)) = current((x, y)) else { continue };
            // Trace a few steps along the current; a line stops where the water does.
            let mut p = (x as f32 + 0.5, y as f32 + 0.5);
            let mut pts = vec![p];
            let mut dir = Vec2::ZERO;
            for _ in 0..2 {
                let c = (p.0 as i32, p.1 as i32);
                if c.0 < 0 || c.1 < 0 || c.0 >= n || c.1 >= n || s.water.depth.get(c.0 as usize, c.1 as usize).copied().unwrap_or(0.0) < MIN_DEPTH {
                    break;
                }
                let Some((d, v)) = current(c) else { break };
                dir = d;
                let step = (0.8 + 0.6 * v.sqrt()).min(1.5);
                p = (p.0 + dir.x * step, p.1 + dir.y * step);
                pts.push(p);
            }
            if pts.len() < 2 {
                continue;
            }
            let col = colour(speed);
            let (d3, side) = (Vec3::new(dir.x, 0.0, dir.y), Vec3::new(-dir.y, 0.0, dir.x));
            // Drawn twice, a hair apart, so a line reads at a distance.
            for nudge in [Vec3::ZERO, side * 0.06] {
                for w in pts.windows(2) {
                    g.line(surface(w[0]) + nudge, surface(w[1]) + nudge, col);
                }
                let tip = surface(*pts.last().unwrap()) + nudge;
                g.line(tip, tip - d3 * 0.4 + side * 0.25, col);
                g.line(tip, tip - d3 * 0.4 - side * 0.25, col);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_world_overlay(mut g: Gizmos, sim: Res<Sim>, hud: Res<HudState>, hover: Res<HoverCell>, sel: Res<Selection>, orbit: Res<crate::camera::OrbitCamera>, thing: Res<HoverThing>, snap: Res<WorksSnap>) {
    let _p = ::sim::prof::scope("game/draw world overlay");
    let s = &sim.0;
    let h = &s.height;
    let lens = crate::terrain_render::map_lens();
    let (tx, ty) = crate::flow::world_cell(h, orbit.target);
    if lens == MapLens::Water.code() {
        draw_currents(&mut g, s, (tx, ty), orbit.distance);
        // Depth lines on the bed about what the camera sees, what lies on the bed outlined at
        // the bed, and buildings standing too deep (flooded) outlined in the flooded colour.
        let r = (orbit.distance * 1.4).clamp(30.0, 110.0) as usize;
        let (tx, ty) = (tx.max(0.0) as usize, ty.max(0.0) as usize);
        draw_contours(&mut g, s, (tx, ty), r);
        for d in s.resources.items().iter().filter(|d| d.has_yield() && d.cell.0.abs_diff(tx) <= r && d.cell.1.abs_diff(ty) <= r && crate::terrain_render::underwater_deposit(s, d)) {
            let [cr, cg, cb] = crate::terrain_render::deposit_color(d.kind);
            cell_outline(&mut g, h, d.cell, Some(bed_y(s, d.cell.0 as f32, d.cell.1 as f32)), Color::srgb(cr, cg, cb));
        }
        let [fr, fg, fb] = crate::terrain_render::FLOODED;
        for b in s.buildings.buildings.iter().filter(|b| sim::buildings::flooded(b, &s.water.depth)) {
            footprint_outline(&mut g, s, b, Color::srgb(fr, fg, fb));
        }
    }
    if lens == MapLens::Forest.code() {
        let (r, tx, ty) = (40usize, tx.max(0.0) as usize, ty.max(0.0) as usize);
        for t in s.resources.items().iter().filter(|t| t.cell.0.abs_diff(tx) <= r && t.cell.1.abs_diff(ty) <= r) {
            if let Some(st) = t.stage() {
                tree_dot(&mut g, h, t.cell, st);
            }
        }
    }
    if lens == MapLens::Deposits.code() {
        // Near the view, outline each deposit's cell (on the surface when under water, where
        // the terrain tint is hidden). Trees stand out on their own.
        let (r, tx, ty) = (40usize, tx.max(0.0) as usize, ty.max(0.0) as usize);
        let near = |d: &&sim::resources::Resource| !d.kind.is_tree() && d.cell.0.abs_diff(tx) <= r && d.cell.1.abs_diff(ty) <= r;
        for d in crate::terrain_render::shown_deposits(s).filter(near) {
            let depth = s.water.depth.get(d.cell.0, d.cell.1).copied().unwrap_or(0.0);
            let top = (depth > 0.03).then(|| h.get(d.cell.0, d.cell.1).copied().unwrap_or(0.0) + depth);
            let [cr, cg, cb] = crate::terrain_render::deposit_color(d.kind);
            cell_outline(&mut g, h, d.cell, top, Color::srgb(cr, cg, cb));
        }
    }
    // The dam or canal cell a click would pick, ringed.
    if let Some(c) = snap.0 {
        let at = cell_world(h, (c.0 as f32, c.1 as f32)).with_y(works_top(s, c) + 0.06);
        ring(&mut g, at, 0.55, Color::WHITE.with_alpha(0.95));
        ring(&mut g, at, 0.48, Color::WHITE.with_alpha(0.6));
    }
    if let Some((b, d)) = match *sel { Selection::Site(id) => s.buildings.building(id).and_then(|b| b.dam.as_deref().map(|d| (b, d))), _ => None } {
        draw_dam(&mut g, s, &hud, b.id, d);
    } else if let Selection::Site(id) = *sel {
        if let Some(b) = s.buildings.building(id) {
            for c in b.cells() {
                cell_outline(&mut g, h, c, None, SELECT);
            }
            if let Some(a) = s.work.place(id).and_then(|p| p.area) {
                if b.kind.row().round_area { round_outline(&mut g, h, a, SELECT) } else { area_outline(&mut g, h, a, SELECT) }
            }
        }
    }
    if let Selection::Animal(id) = *sel {
        if let Some(a) = s.wildlife.get(id) {
            let at = cell_world(h, a.pos).with_y(height_at(h, a.pos.0, a.pos.1) + 0.15);
            ring(&mut g, at, 0.6, SELECT);
            ring(&mut g, at, 0.53, SELECT);
        }
    }
    if let Selection::Tree(id) = *sel {
        if let Some(r) = s.resources.get(id) {
            let (x, y) = (r.cell.0 as f32 + 0.5, r.cell.1 as f32 + 0.5);
            let at = cell_world(h, (x, y)).with_y(height_at(h, x, y) + 0.15);
            ring(&mut g, at, 0.55, SELECT);
            ring(&mut g, at, 0.48, SELECT);
        }
    }
    if hud.tool == Tool::None {
        // With beavers picked: where a right-click would send them, and where they are going.
        let picked: Vec<sim::colony::BeaverId> = match (*sel, hud.selected.is_empty()) {
            (_, false) => hud.selected.clone(),
            (Selection::Beaver(b), true) => vec![b],
            _ => Vec::new(),
        };
        // Whatever the pointer is over gets the same white mark: a ring round a spot of ground,
        // a plant, tree, mud bed, pebbles, beaver or lodge; an outline round a building.
        let hover_col = Color::WHITE.with_alpha(0.8);
        let at_ground = |x: f32, y: f32| cell_world(h, (x, y)).with_y(height_at(h, x, y) + 0.15);
        match thing.0 {
            Some(Thing::Resource(id)) => {
                if let Some(r) = s.resources.get(id) {
                    let big = r.kind.is_tree() || r.kind.is_felled();
                    ring(&mut g, at_ground(r.cell.0 as f32 + 0.5, r.cell.1 as f32 + 0.5), if big { 0.6 } else { 0.45 }, hover_col);
                }
            }
            Some(Thing::Beaver(id)) => {
                if let Some(b) = s.colony.beaver(id) {
                    ring(&mut g, at_ground(b.pos.0, b.pos.1), 0.45, hover_col);
                }
            }
            Some(Thing::Lodge(id)) => {
                if let Some(l) = s.colony.lodges.iter().find(|l| l.id == id) {
                    ring(&mut g, at_ground(l.pos.0, l.pos.1), 1.1, hover_col);
                }
            }
            Some(Thing::Building(id)) => {
                if let Some(b) = s.buildings.building(id) {
                    for c in b.cells() {
                        cell_outline(&mut g, h, c, None, hover_col);
                    }
                }
            }
            _ => {
                if let (Some(c), Some(Thing::Cell(_))) = (hover.0, thing.1) {
                    ring(&mut g, at_ground(c.0 as f32 + 0.5, c.1 as f32 + 0.5), 0.4, hover_col);
                }
            }
        }
        if !picked.is_empty() {
            for id in &picked {
                if let Some(sim::agents::Assignment::Go(to)) = s.agents.get(*id).and_then(|a| a.assignment) {
                    let at = cell_world(h, (to.0 as f32 + 0.5, to.1 as f32 + 0.5)).with_y(height_at(h, to.0 as f32 + 0.5, to.1 as f32 + 0.5) + 0.15);
                    ring(&mut g, at, 0.4, ACCENT);
                    ring(&mut g, at, 0.3, ACCENT);
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
                let probe = sim::buildings::Building { id: BuildingId(0), kind, pos: c, status: sim::buildings::Status::Site, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None };
                if let Some(a) = sim::catalogue::work_area(&probe) {
                    let area_col = Color::WHITE.with_alpha(0.6);
                    if kind.row().round_area { round_outline(&mut g, h, a, area_col) } else { area_outline(&mut g, h, a, area_col) }
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
        Tool::Canal => draw_canal_ghost(&mut g, s, &hud, hover.0),
        Tool::Harvest(_) => {
            if let (Some(a), Some(b)) = (hud.drag_from, hover.0) {
                let area = sim::work::Area { min: (a.0.min(b.0), a.1.min(b.1)), max: (a.0.max(b.0), a.1.max(b.1)) };
                area_outline(&mut g, h, area, ACCENT);
            } else if let Some(c) = hover.0 {
                ring(&mut g, cell_world(h, (c.0 as f32, c.1 as f32)).with_y(height_at(h, c.0 as f32, c.1 as f32) + 0.2), 0.45, ACCENT);
            }
        }
    }
    draw_sight(&mut g, s, &hud, hover.0, *sel, thing.0);
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
        // The pond the water engine makes behind the line, once its run is in.
        if let Some(r) = hud.dam_settled().filter(|r| !r.water.flooded.is_empty()) {
            shoreline(g, h, &r.water.flooded, r.water.level + 0.06, FOAM);
            for &t in &r.trees {
                let base = cell_world(h, (t.0 as f32, t.1 as f32)).with_y(height_at(h, t.0 as f32, t.1 as f32) + 0.25);
                ring(g, base, 0.55, WATER);
                ring(g, base + Vec3::Y * 0.04, 0.5, WATER);
            }
        }
        // Buildings the new pond would put too deep to use: their footprint ringed in red.
        if let Some((_, ids)) = hud.drowned.as_ref().filter(|d| d.0 == hud.line) {
            for b in ids.iter().filter_map(|id| s.buildings.building(*id)) {
                footprint_outline(g, s, b, BAD);
            }
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

/// While a canal is drawn: every cell coloured by what it will hold once dug (floats a load,
/// only wet, dry, or a cut too deep), the gate, the corners, and where the next click runs.
fn draw_canal_ghost(g: &mut Gizmos, s: &sim::SimState, hud: &HudState, hover: Option<Cell>) {
    let h = &s.height;
    let at = |c: Cell, lift: f32| cell_world(h, (c.0 as f32 + 0.5, c.1 as f32 + 0.5)).with_y(height_at(h, c.0 as f32 + 0.5, c.1 as f32 + 0.5) + lift);
    let square = |g: &mut Gizmos, c: Cell, r: f32, col: Color| {
        let p = at(c, 0.12);
        let (a, b, cc, d) = (p + Vec3::new(-r, 0.0, -r), p + Vec3::new(r, 0.0, -r), p + Vec3::new(r, 0.0, r), p + Vec3::new(-r, 0.0, r));
        g.line(a, b, col);
        g.line(b, cc, col);
        g.line(cc, d, col);
        g.line(d, a, col);
    };
    match hud.canal_plan.as_ref().filter(|p| p.0 == hud.line).map(|p| &p.1) {
        Some(Ok(p)) => {
            for (i, c) in p.cells.iter().enumerate() {
                let d = p.expected.get(i).copied().unwrap_or(0.0);
                let col = if p.too_deep.contains(&c.cell) {
                    BAD
                } else if d >= sim::canals::FLOAT_DEPTH {
                    WATER
                } else if d >= sim::canals::WET_DEPTH {
                    Color::srgb(0.6, 0.88, 0.92)
                } else {
                    Color::srgb(0.85, 0.72, 0.52)
                };
                square(g, c.cell, 0.36, col);
                square(g, c.cell, 0.3, col);
            }
            ring(g, at(p.gate, 0.2), 0.45, WOOD_LINE);
            ring(g, at(p.gate, 0.24), 0.38, WOOD_LINE);
        }
        _ => {
            for &c in &hud.line {
                square(g, c, 0.34, Color::WHITE.with_alpha(0.6));
            }
        }
    }
    for c in &hud.corners {
        ring(g, at(*c, 0.15), 0.3, ACCENT);
    }
    match (hud.corners.last(), hover) {
        (Some(last), Some(h2)) if hud.drag_from.is_none() && *last != h2 => {
            let (a, b) = (at(*last, 0.3), at(h2, 0.3));
            let n = ((a - b).length() * 2.0).ceil().max(1.0) as usize;
            for i in (0..n).step_by(2) {
                g.line(a.lerp(b, i as f32 / n as f32), a.lerp(b, ((i + 1) as f32 / n as f32).min(1.0)), Color::WHITE.with_alpha(0.45));
            }
        }
        (None, Some(h2)) => ring(g, at(h2, 0.15), 0.3, Color::WHITE.with_alpha(0.7)),
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
    let top = |c: Cell| works_top(s, c);
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
    // Placing a gate or lock: every cell of the line it can go on is lit; the rest stay dim.
    if let Some((_, o)) = hud.opening.filter(|p| p.0 == id) {
        for c in &line {
            if super::works::opening_fits(s, id, *c, o).is_ok() {
                cell_outline(g, h, *c, Some(top(*c) + 0.1), GOOD);
                cell_outline(g, h, *c, Some(top(*c) + 0.14), GOOD.with_alpha(0.6));
            } else {
                cell_outline(g, h, *c, Some(top(*c) + 0.1), Color::WHITE.with_alpha(0.15));
            }
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

pub fn mood_icon(m: f32) -> (Icon, Color) {
    if m >= 0.66 {
        (Icon::MoodGood, GOOD)
    } else if m >= 0.4 {
        (Icon::MoodOk, WARN)
    } else {
        (Icon::MoodBad, BAD)
    }
}

/// What the hover tip says about a plain cell of ground or water.
pub fn cell_rows(s: &sim::SimState, cell: Cell) -> Vec<(Icon, Color, &'static str, String)> {
    let (x, y) = cell;
    let h = s.height.get(x, y).copied().unwrap_or(0.0);
    let d = s.water.depth.get(x, y).copied().unwrap_or(0.0);
    let mut rows = vec![(Icon::ViewElevation, MUTED, "Height", num(h))];
    if d > 0.01 {
        rows.push((Icon::Drop, WATER, "Depth", format!("{d:.2} m")));
        rows.push((Icon::Crest, WATER, "Level", format!("{:.2} m", h + d)));
    }
    rows
}
