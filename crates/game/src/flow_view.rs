//! Views for the game flow: time-control buttons, status and detail panel, the beaver ledger,
//! the game-over panel, scout/beaver/lodge markers, the fog colouring of the terrain and the
//! scripted `--autoplay-settle` run. Layout is computed in one pure function so hit-testing and
//! drawing always agree.
use crate::camera::OrbitCamera;
use crate::flow::*;
use crate::sim_driver::{SimClock, Speed};
use crate::state::Sim;
use crate::terrain_render::{build_mesh, Terrain};
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use sim::colony::{Beaver, BeaverId, LodgeId};
use std::collections::HashMap;

const PANEL_BG: Color = Color::srgba(0.05, 0.07, 0.10, 0.82);
const BTN_BG: Color = Color::srgba(0.16, 0.20, 0.26, 0.92);
const BTN_HOT: Color = Color::srgba(0.30, 0.38, 0.50, 0.95);
const BTN_ON: Color = Color::srgba(0.20, 0.48, 0.30, 0.95);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Pause,
    Speed(Speed),
    Continue,
    Restart,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PanelKind {
    Info,
    GameOver,
}

/// Screen rectangles of everything clickable or opaque, in logical pixels.
#[derive(Resource, Default)]
pub struct UiLayout {
    pub buttons: Vec<(Action, Rect)>,
    pub panels: Vec<(PanelKind, Rect)>,
    /// Extra opaque areas published by the ui overlay (nav bar, screens, time label).
    pub extra: Vec<Rect>,
}

impl UiLayout {
    pub fn over_ui(&self, p: Vec2) -> bool {
        self.buttons.iter().any(|(_, r)| r.contains(p)) || self.panels.iter().any(|(_, r)| r.contains(p)) || self.extra.iter().any(|r| r.contains(p))
    }
    pub fn hit(&self, p: Vec2) -> Option<Action> {
        self.buttons.iter().find(|(_, r)| r.contains(p)).map(|(a, _)| *a)
    }
    pub fn rect(&self, a: Action) -> Option<Rect> {
        self.buttons.iter().find(|(x, _)| *x == a).map(|(_, r)| *r)
    }
    pub fn panel(&self, k: PanelKind) -> Option<Rect> {
        self.panels.iter().find(|(x, _)| *x == k).map(|(_, r)| *r)
    }
}

/// A left click that landed on the world (set by [`detect_click`], consumed by picking).
#[derive(Resource, Default)]
pub struct WorldClick(pub Option<Vec2>);

#[derive(SystemSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct ClickSet;

fn rc(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, x + w, y + h)
}

pub const BUTTON_Y: f32 = 10.0;

/// Pure layout of all UI rectangles for a window of `win` logical pixels.
pub fn compute_layout(win: Vec2, show_info: bool, gameover: bool) -> UiLayout {
    let mut l = UiLayout::default();
    // top-right button strip
    let widths = [(Action::Pause, 48.0), (Action::Speed(Speed::X1), 38.0), (Action::Speed(Speed::X3), 38.0), (Action::Speed(Speed::X10), 44.0)];
    let total: f32 = widths.iter().map(|w| w.1 + 4.0).sum();
    let mut x = win.x - 10.0 - total + 4.0;
    for (a, w) in widths {
        l.buttons.push((a, rc(x, BUTTON_Y, w, 28.0)));
        x += w + 4.0;
    }
    // info panel: only while scouting or when something is selected
    if show_info {
        l.panels.push((PanelKind::Info, rc(10.0, win.y - 10.0 - 110.0, 380.0, 110.0)));
    }
    if gameover {
        let (w, h) = (400.0, 200.0);
        let (px, py) = ((win.x - w) / 2.0, (win.y - h) / 2.0);
        l.panels.push((PanelKind::GameOver, rc(px, py, w, h)));
        l.buttons.push((Action::Continue, rc(px + 20.0, py + h - 54.0, 170.0, 36.0)));
        l.buttons.push((Action::Restart, rc(px + w - 190.0, py + h - 54.0, 170.0, 36.0)));
    }
    l
}

// ───────────────────────────── components ─────────────────────────────

#[derive(Component)]
struct Btn(Action);
#[derive(Component)]
struct PanelNode(PanelKind);
#[derive(Component)]
struct BtnLabel(Action);
#[derive(Component)]
struct InfoText;
#[derive(Component)]
struct BannerText;
#[derive(Component)]
struct GameOverText;
#[derive(Component)]
struct ScoutMarker;
#[derive(Component)]
struct BeaverMarker;
#[derive(Component)]
struct LodgeMarker;

#[derive(Resource)]
struct Visuals {
    beaver: (Handle<Mesh>, Handle<StandardMaterial>),
    lodge: (Handle<Mesh>, Handle<StandardMaterial>),
    scout: (Handle<Mesh>, Handle<StandardMaterial>),
}

/// Scripted opening for `--autoplay-settle`.
#[derive(Resource)]
struct Autoplay {
    scouting_since: Option<f32>,
}

pub struct FlowViewPlugin;

impl Plugin for FlowViewPlugin {
    fn build(&self, app: &mut App) {
        let autoplay = app.world().resource::<crate::cli::Cli>().autoplay_settle;
        app.init_resource::<UiLayout>()
            .init_resource::<WorldClick>()
            .insert_resource(FogBase { generation: u32::MAX, colors: Vec::new() })
            .add_systems(Startup, (spawn_ui, spawn_visuals))
            .add_systems(Update, (keys, layout_ui, crate::build::layout_build, detect_click, crate::build::build_input).chain().in_set(ClickSet))
            .add_systems(
                Update,
                (apply_fog, sync_markers, update_ui, draw_gizmos).after(ClickSet),
            );
        if autoplay {
            app.insert_resource(Autoplay { scouting_since: None })
                .insert_resource(CaptureGate(false))
                .add_systems(Update, autoplay_settle.after(ClickSet));
        }
    }
}

// ───────────────────────────── spawn ─────────────────────────────

fn font(size: f32) -> TextFont {
    TextFont::from_font_size(size)
}

fn spawn_ui(mut commands: Commands) {
    let abs = |_: ()| Node { position_type: PositionType::Absolute, ..default() };
    // panels
    commands
        .spawn((PanelNode(PanelKind::Info), Node { padding: UiRect::all(px(10)), ..abs(()) }, BackgroundColor(PANEL_BG)))
        .with_children(|p| {
            p.spawn((Text::new(""), font(14.0), InfoText));
        });
    commands
        .spawn((PanelNode(PanelKind::GameOver), Node { padding: UiRect::all(px(16)), ..abs(()) }, BackgroundColor(Color::srgba(0.12, 0.05, 0.05, 0.93))))
        .with_children(|p| {
            p.spawn((Text::new(""), font(15.0), GameOverText));
        });
    // banner
    commands.spawn((
        Text::new(""),
        font(18.0),
        TextLayout::justify(Justify::Center),
        Node { position_type: PositionType::Absolute, top: px(48), left: px(0), width: percent(100), justify_content: JustifyContent::Center, ..default() },
        BannerText,
    ));
    // buttons
    let mut spawn_btn = |a: Action, label: &str| {
        commands
            .spawn((Btn(a), Node { position_type: PositionType::Absolute, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(BTN_BG)))
            .with_children(|p| {
                p.spawn((Text::new(label), font(14.0), BtnLabel(a)));
            });
    };
    spawn_btn(Action::Pause, "Pause");
    for s in Speed::ALL {
        spawn_btn(Action::Speed(s), s.label());
    }
    spawn_btn(Action::Continue, "Keep watching");
    spawn_btn(Action::Restart, "Restart");
}

fn spawn_visuals(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mat = |c: Color| StandardMaterial { base_color: c, perceptual_roughness: 0.9, ..default() };
    commands.insert_resource(Visuals {
        beaver: (meshes.add(Capsule3d::new(0.35, 0.6)), mats.add(mat(Color::srgb(0.45, 0.28, 0.14)))),
        lodge: (meshes.add(Sphere::new(2.2)), mats.add(mat(Color::srgb(0.38, 0.24, 0.12)))),
        scout: (meshes.add(Capsule3d::new(0.45, 0.8)), mats.add(StandardMaterial { emissive: LinearRgba::new(0.4, 0.3, 0.0, 1.0), ..mat(Color::srgb(0.95, 0.8, 0.2)) })),
    });
}

// ───────────────────────────── input ─────────────────────────────

fn keys(keys: Res<ButtonInput<KeyCode>>, mut sel: ResMut<Selection>) {
    if keys.just_pressed(KeyCode::Escape) {
        *sel = Selection::None;
    }
}

fn layout_ui(
    windows: Query<&Window, With<PrimaryWindow>>,
    sel: Res<Selection>,
    sim: Res<Sim>,
    flow: Res<Flow>,
    mut layout: ResMut<UiLayout>,
    mut nodes: Query<(&mut Node, Option<&Btn>, Option<&PanelNode>)>,
) {
    let Ok(w) = windows.single() else { return };
    let extra = std::mem::take(&mut layout.extra);
    *layout = compute_layout(Vec2::new(w.width(), w.height()), !sim.0.colony.is_founded() || matches!(*sel, Selection::Cell(_)), flow.phase == Phase::GameOver);
    layout.extra = extra;
    for (mut node, btn, panel) in &mut nodes {
        let rect = btn.and_then(|b| layout.rect(b.0)).or_else(|| panel.and_then(|p| layout.panel(p.0 )));
        match rect {
            Some(r) => {
                node.display = Display::Flex;
                node.left = px(r.min.x);
                node.top = px(r.min.y);
                node.width = px(r.width());
                node.height = px(r.height());
            }
            None if btn.is_some() || panel.is_some() => node.display = Display::None,
            None => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn detect_click(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut press: Local<Option<Vec2>>,
    layout: Res<UiLayout>,
    mut wc: ResMut<WorldClick>,
    mut clock: ResMut<SimClock>,
    mut sim: ResMut<Sim>,
    mut flow: ResMut<Flow>,
    mut sel: ResMut<Selection>,
) {
    let Ok(w) = windows.single() else { return };
    let Some(cursor) = w.cursor_position() else { return };
    if buttons.just_pressed(MouseButton::Left) {
        *press = Some(cursor);
    }
    if buttons.just_released(MouseButton::Left) {
        let Some(start) = press.take() else { return };
        if start.distance(cursor) > 5.0 {
            return; // a drag orbits the camera
        }
        match layout.hit(cursor) {
            Some(a) => do_action(a, &mut clock, &mut sim, &mut flow, &mut sel),
            None if layout.over_ui(cursor) => {}
            None => wc.0 = Some(cursor),
        }
    }
}

fn do_action(a: Action, clock: &mut SimClock, sim: &mut Sim, flow: &mut Flow, sel: &mut Selection) {
    match a {
        Action::Pause => clock.toggle_pause(),
        Action::Speed(s) => clock.set_speed(s),
        Action::Continue => {
            if flow.fire(FlowEvent::Continue) {
                clock.paused = false;
            }
        }
        Action::Restart => restart(&mut sim.0, flow, clock, sel),
    }
}

fn needs_line(b: &Beaver) -> String {
    let n = b.needs;
    format!("food {:.0}%  rest {:.0}%  warmth {:.0}%  safety {:.0}%  social {:.0}%", n.food * 100.0, n.rest * 100.0, n.warmth * 100.0, n.safety * 100.0, n.social * 100.0)
}

fn job_name(b: &Beaver) -> String {
    b.job.or(b.current_job).map(|j| format!("{j:?}")).unwrap_or_else(|| "idle".into())
}

pub fn selection_text(sim: &Sim, sel: Selection, hint: Option<SiteHint>) -> String {
    let c = &sim.0.colony;
    match sel {
        Selection::None => String::new(),
        Selection::Beaver(id) => match c.beaver(id) {
            Some(b) => format!(
                "Beaver {}  ({}, {:.1} y)  job {}  mood {:.0}%  health {:.0}%\n{}",
                b.name,
                c.family(b.family).map(|f| f.name.as_str()).unwrap_or("?"),
                b.age_years(),
                job_name(b),
                b.mood * 100.0,
                b.health * 100.0,
                needs_line(b)
            ),
            None => "That beaver is gone".into(),
        },
        Selection::Lodge(id) => match c.lodges.iter().find(|l| l.id == id) {
            Some(l) => {
                let n = c.beavers.iter().filter(|b| b.lodge == Some(id)).count();
                let fam = c.family(l.family).map(|f| f.name.as_str()).unwrap_or("?");
                format!("Lodge of the {fam}  condition {:.0}%  {n} residents", l.condition * 100.0)
            }
            None => "That lodge is gone".into(),
        },
        Selection::Building(cell) => format!("Dam at ({}, {})", cell.0, cell.1),
        Selection::Tree(_) | Selection::Site(_) => String::new(),
        Selection::Cell(cell) => {
            let w = &sim.0.world;
            let valley = w.valley_of(cell).map(|v| format!("valley {}{}", v.0, if w.is_open(v) { "" } else { " (locked)" })).unwrap_or_default();
            let h = hint.unwrap_or_else(|| site_hint(w, cell));
            format!("Cell ({}, {})  {valley}\n{}", cell.0, cell.1, hint_text(&h))
        }
    }
}

pub fn hint_text(h: &SiteHint) -> String {
    format!("Site: {}  water {:.0}%  trees {:.0}%  safety {:.0}%", h.verdict(), h.water * 100.0, h.trees * 100.0, h.safety * 100.0)
}

#[allow(clippy::too_many_arguments)]
fn info_text(sim: &Sim, flow: &Flow, sel: Selection, hover: &Hover) -> String {
    let c = &sim.0.colony;
    let mut s = String::new();
    if !c.is_founded() {
        let total = (sim.0.world.size * sim.0.world.size) as f32;
        s += &format!("Scouting   explored {:.0}%   scouts {}\n", flow.fog.explored_cells() as f32 / total * 100.0, flow.scouts.len());
        if let Some(h) = hover.hint.filter(|_| hover.cell.is_some()) {
            s += &format!("Under cursor ({}, {})\n{}\n", hover.cell.unwrap().0, hover.cell.unwrap().1, hint_text(&h));
        } else {
            s += "Hover the land to read a site hint (water, trees, safety)\n";
        }
        if !flow.message.is_empty() {
            s += &format!("! {}\n", flow.message);
        }
        s += &selection_text(sim, sel, hover.hint);
        return s;
    }
    selection_text(sim, sel, hover.hint)
}

fn update_ui(
    sim: Res<Sim>,
    flow: Res<Flow>,
    sel: Res<Selection>,
    hover: Res<Hover>,
    clock: Res<SimClock>,
    layout: Res<UiLayout>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut texts: ParamSet<(
        Query<&mut Text, With<InfoText>>,
        Query<&mut Text, With<BannerText>>,
        Query<&mut Text, With<GameOverText>>,
        Query<(&BtnLabel, &mut Text)>,
    )>,
    mut btn_colors: Query<(&Btn, &mut BackgroundColor)>,
) {
    let text = info_text(&sim, &flow, *sel, &hover);
    for mut t in &mut texts.p0() {
        if t.0 != text {
            t.0 = text.clone();
        }
    }
    let banner = match flow.phase {
        Phase::Settled => String::new(),
        p => p.banner().to_string(),
    };
    for mut t in &mut texts.p1() {
        t.0 = banner.clone();
    }
    let go = match sim.0.colony.collapse() {
        Some(r) => format!(
            "THE COLONY HAS COLLAPSED\n\nCause: {:?}\nDay {:.0}   peak population {}\nDeaths {}   departures {}\n\nKeep watching the valley, or start over?",
            r.cause, r.day, r.peak_population, r.deaths, r.departures
        ),
        None if flow.phase == Phase::GameOver => "THE COLONY HAS COLLAPSED".into(),
        None => String::new(),
    };
    for mut t in &mut texts.p2() {
        t.0 = go.clone();
    }
    for (l, mut t) in &mut texts.p3() {
        let label = match l.0 {
            Action::Pause => if clock.paused { "Play".to_string() } else { "Pause".to_string() },
            _ => continue,
        };
        if t.0 != label {
            t.0 = label;
        }
    }
    // button states
    let cursor = windows.single().ok().and_then(|w| w.cursor_position());
    for (b, mut bg) in &mut btn_colors {
        let hot = cursor.zip(layout.rect(b.0)).is_some_and(|(c, r)| r.contains(c));
        let on = match b.0 {
            Action::Pause => clock.paused,
            Action::Speed(s) => !clock.paused && clock.speed == s,
            _ => false,
        };
        bg.0 = if hot { BTN_HOT } else if on { BTN_ON } else { BTN_BG };
    }
}

// ───────────────────────────── ledger ─────────────────────────────

#[derive(Resource)]
struct FogBase {
    generation: u32,
    colors: Vec<[f32; 4]>,
}

/// Unexplored ground: soft plum mist (never slate or black).
const FOG_COLOR: [f32; 3] = [0.55, 0.50, 0.66];

fn apply_fog(
    mut flow: ResMut<Flow>,
    sim: Res<Sim>,
    mut base: ResMut<FogBase>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain: Query<&Mesh3d, With<Terrain>>,
) {
    let Ok(handle) = terrain.single() else { return };
    if base.generation != flow.generation {
        // new world (or first frame): rebuild the mesh and remember its natural colours
        let mesh = build_mesh(&sim.0.height);
        if let Some(bevy::mesh::VertexAttributeValues::Float32x4(c)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
            base.colors = c.clone();
        }
        let _ = meshes.insert(&handle.0, mesh);
        base.generation = flow.generation;
        flow.fog.dirty = true;
    }
    if !flow.fog.dirty {
        return;
    }
    flow.fog.dirty = false;
    let Some(mut mesh) = meshes.get_mut(&handle.0) else { return };
    if base.colors.len() != flow.fog.seen.len() {
        return;
    }
    let out: Vec<[f32; 4]> = base
        .colors
        .iter()
        .zip(&flow.fog.seen)
        .map(|(c, e)| {
            let e = e.clamp(0.0, 1.0);
            [c[0] * e + FOG_COLOR[0] * (1.0 - e), c[1] * e + FOG_COLOR[1] * (1.0 - e), c[2] * e + FOG_COLOR[2] * (1.0 - e), 1.0]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, out);
}

// ───────────────────────────── markers ─────────────────────────────

#[allow(clippy::too_many_arguments)]
fn sync_markers(
    mut commands: Commands,
    sim: Res<Sim>,
    flow: Res<Flow>,
    vis: Option<Res<Visuals>>,
    mut scouts: Local<Vec<Entity>>,
    mut beavers: Local<HashMap<BeaverId, Entity>>,
    mut lodges: Local<HashMap<LodgeId, Entity>>,
    mut last_gen: Local<u32>,
    mut tf: Query<&mut Transform>,
) {
    let Some(vis) = vis else { return };
    let h = &sim.0.height;
    if *last_gen != flow.generation {
        for e in scouts.drain(..).chain(beavers.drain().map(|b| b.1)).chain(lodges.drain().map(|l| l.1)) {
            commands.entity(e).despawn();
        }
        *last_gen = flow.generation;
    }
    // scouts exist until the colony is founded
    let want = if flow.phase == Phase::Scouting || flow.phase == Phase::Arrival { flow.scouts.len() } else { 0 };
    while scouts.len() < want {
        scouts.push(commands.spawn((ScoutMarker, Mesh3d(vis.scout.0.clone()), MeshMaterial3d(vis.scout.1.clone()), Transform::default())).id());
    }
    while scouts.len() > want {
        commands.entity(scouts.pop().unwrap()).despawn();
    }
    for (e, s) in scouts.iter().zip(&flow.scouts) {
        if let Ok(mut t) = tf.get_mut(*e) {
            t.translation = cell_world(h, s.pos) + Vec3::Y * 0.8;
        }
    }
    let c = &sim.0.colony;
    beavers.retain(|id, e| {
        let alive = c.beaver(*id).is_some();
        if !alive {
            commands.entity(*e).despawn();
        }
        alive
    });
    for b in &c.beavers {
        let p = cell_world(h, b.pos) + Vec3::Y * 0.45;
        let scale = if b.age_years() < 1.0 { 0.65 } else { 1.0 };
        match beavers.get(&b.id) {
            Some(e) => {
                if let Ok(mut t) = tf.get_mut(*e) {
                    t.translation = p;
                }
            }
            None => {
                let e = commands
                    .spawn((BeaverMarker, Mesh3d(vis.beaver.0.clone()), MeshMaterial3d(vis.beaver.1.clone()), Transform::from_translation(p).with_scale(Vec3::splat(scale))))
                    .id();
                beavers.insert(b.id, e);
            }
        }
    }
    lodges.retain(|id, e| {
        let alive = c.lodges.iter().any(|l| l.id == *id);
        if !alive {
            commands.entity(*e).despawn();
        }
        alive
    });
    for l in &c.lodges {
        if !lodges.contains_key(&l.id) {
            let p = cell_world(h, l.pos);
            let e = commands
                .spawn((LodgeMarker, Mesh3d(vis.lodge.0.clone()), MeshMaterial3d(vis.lodge.1.clone()), Transform::from_translation(p + Vec3::Y * 0.6).with_scale(Vec3::new(1.0, 0.7, 1.0))))
                .id();
            lodges.insert(l.id, e);
        }
    }
}

fn draw_gizmos(mut gizmos: Gizmos, sim: Res<Sim>, sel: Res<Selection>, hover: Res<Hover>, flow: Res<Flow>) {
    let h = &sim.0.height;
    let flat = |p: Vec3| Isometry3d::new(p + Vec3::Y * 0.15, Quat::from_rotation_arc(Vec3::Z, Vec3::Y));
    if let (Some(c), Some(hint)) = (hover.cell, hover.hint) {
        if flow.phase == Phase::Scouting {
            let o = hint.overall();
            let color = Color::srgb(1.0 - o, 0.3 + 0.7 * o, 0.25);
            gizmos.circle(flat(cell_world(h, (c.0 as f32, c.1 as f32))), 2.0, color);
        }
    }
    let target = match *sel {
        Selection::Cell(c) | Selection::Building(c) => Some((cell_world(h, (c.0 as f32, c.1 as f32)), 1.0)),
        Selection::Beaver(id) => sim.0.colony.beaver(id).map(|b| (cell_world(h, b.pos), 1.0)),
        Selection::Lodge(id) => sim.0.colony.lodges.iter().find(|l| l.id == id).map(|l| (cell_world(h, l.pos), 3.2)),
        Selection::Tree(id) => sim.0.resources.get(id).map(|r| (cell_world(h, (r.cell.0 as f32, r.cell.1 as f32)), 1.0)),
        Selection::Site(id) => sim.0.buildings.building(id).map(|b| (cell_world(h, (b.pos.0 as f32 + 1.0, b.pos.1 as f32 + 1.0)), 2.0)),
        Selection::None => None,
    };
    if let Some((p, r)) = target {
        gizmos.circle(flat(p), r, Color::srgb(1.0, 0.95, 0.3));
        gizmos.circle(flat(p), r + 0.25, Color::srgb(1.0, 0.95, 0.3));
    }
}

// ───────────────────────────── autoplay ─────────────────────────────

/// `--autoplay-settle`: scout for a few seconds, settle at the best explored site, let the camera
/// settle on the lodge, then open the screenshot gate.
fn autoplay_settle(
    mut ap: ResMut<Autoplay>,
    mut sim: ResMut<Sim>,
    mut flow: ResMut<Flow>,
    mut sel: ResMut<Selection>,
    mut clock: ResMut<SimClock>,
    mut gate: ResMut<CaptureGate>,
    mut exit: MessageWriter<AppExit>,
    orbit: Res<OrbitCamera>,
    demo: Option<Res<crate::build::DemoDam>>,
) {
    let _ = &orbit;
    if std::env::var("BC_AUTOPLAY_UI").is_ok_and(|v| v.contains("gameover")) && flow.phase == Phase::Settled && flow.real_secs - flow.settled_at.unwrap_or(0.0) > 0.5 {
        flow.phase = Phase::GameOver;
    }
    match flow.phase {
        Phase::Scouting => {
            let since = *ap.scouting_since.get_or_insert(flow.real_secs);
            let waited = flow.real_secs - since;
            if waited >= 4.0 {
                match best_site(&sim.0.world, &sim.0.water.depth, &flow.fog) {
                    Some(site) => match settle(&mut sim.0, &mut flow, site) {
                        Ok(()) => {
                            *sel = Selection::Lodge(sim.0.colony.lodges[0].id);
                            clock.set_speed(Speed::X10);
                            info!("autoplay: settled at {site:?}");
                        }
                        Err(e) => {
                            error!("autoplay: settle failed: {e}");
                            exit.write(AppExit::error());
                        }
                    },
                    None if waited > 30.0 => {
                        error!("autoplay: no settleable site found");
                        exit.write(AppExit::error());
                    }
                    None => {}
                }
            }
        }
        Phase::Settled | Phase::GameOver => {
            let t = flow.real_secs - flow.settled_at.unwrap_or(flow.real_secs);
            if demo.is_none() {
                gate.0 = t >= 3.0 && flow.focus.is_none();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settled_default_view_has_only_the_time_controls() {
        let l = compute_layout(Vec2::new(1280.0, 720.0), false, false);
        assert!(l.panels.is_empty(), "no info card, ledger or debug text");
        let mut acts: Vec<_> = l.buttons.iter().map(|(a, _)| *a).collect();
        acts.sort_by_key(|a| format!("{a:?}"));
        assert_eq!(acts.len(), 4, "pause and three speeds: {acts:?}");
    }

    #[test]
    fn info_card_shows_only_when_asked_for() {
        assert!(compute_layout(Vec2::new(1280.0, 720.0), true, false).panel(PanelKind::Info).is_some());
        assert!(compute_layout(Vec2::new(1280.0, 720.0), false, true).panel(PanelKind::GameOver).is_some());
    }
}
