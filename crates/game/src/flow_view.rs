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

pub const LEDGER_ROWS: usize = 16;
const ROW_H: f32 = 22.0;
const COLS: [(&str, f32); 6] = [("Name", 95.0), ("Family", 100.0), ("Age", 45.0), ("Job", 75.0), ("Mood", 50.0), ("Health", 55.0)];
const PANEL_BG: Color = Color::srgba(0.05, 0.07, 0.10, 0.82);
const BTN_BG: Color = Color::srgba(0.16, 0.20, 0.26, 0.92);
const BTN_HOT: Color = Color::srgba(0.30, 0.38, 0.50, 0.95);
const BTN_ON: Color = Color::srgba(0.20, 0.48, 0.30, 0.95);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Pause,
    Speed(Speed),
    Ledger,
    Detail,
    Continue,
    Restart,
    Sort(usize),
    Row(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PanelKind {
    Info,
    Ledger,
    GameOver,
}

/// Screen rectangles of everything clickable or opaque, in logical pixels.
#[derive(Resource, Default)]
pub struct UiLayout {
    pub buttons: Vec<(Action, Rect)>,
    pub panels: Vec<(PanelKind, Rect)>,
    /// Opaque rectangles of other UI modules (the build UI), rebuilt every frame.
    pub extra: Vec<Rect>,
}

impl UiLayout {
    pub fn over_ui(&self, p: Vec2) -> bool {
        self.buttons.iter().any(|(_, r)| r.contains(p)) || self.panels.iter().any(|(_, r)| r.contains(p))
            || self.extra.iter().any(|r| r.contains(p))
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

#[derive(Resource)]
pub struct UiState {
    pub ledger: bool,
    pub detail: bool,
    pub sort_col: usize,
    pub sort_asc: bool,
    pub offset: usize,
    pub rows: Vec<BeaverId>,
    refresh: f32,
}

impl Default for UiState {
    fn default() -> Self {
        UiState { ledger: false, detail: false, sort_col: 0, sort_asc: true, offset: 0, rows: Vec::new(), refresh: 0.0 }
    }
}

fn rc(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, x + w, y + h)
}

pub const BUTTON_Y: f32 = 10.0;

/// Pure layout of all UI rectangles for a window of `win` logical pixels.
pub fn compute_layout(win: Vec2, ui: &UiState, gameover: bool) -> UiLayout {
    let mut l = UiLayout::default();
    // top-right button strip
    let widths = [(Action::Pause, 48.0), (Action::Speed(Speed::X1), 38.0), (Action::Speed(Speed::X3), 38.0), (Action::Speed(Speed::X10), 44.0), (Action::Ledger, 92.0), (Action::Detail, 104.0)];
    let total: f32 = widths.iter().map(|w| w.1 + 4.0).sum();
    let mut x = win.x - 10.0 - total + 4.0;
    for (a, w) in widths {
        l.buttons.push((a, rc(x, BUTTON_Y, w, 28.0)));
        x += w + 4.0;
    }
    // info panel
    let h = if ui.detail { 340.0 } else { 150.0 };
    l.panels.push((PanelKind::Info, rc(10.0, win.y - 10.0 - h, 380.0, h)));
    // ledger
    if ui.ledger {
        let (w, hh) = (440.0, 34.0 + ROW_H * LEDGER_ROWS as f32 + 26.0);
        let (px, py) = (win.x - 10.0 - w, 48.0);
        l.panels.push((PanelKind::Ledger, rc(px, py, w, hh)));
        let mut cx = px + 12.0;
        for (i, (_, cw)) in COLS.iter().enumerate() {
            l.buttons.push((Action::Sort(i), rc(cx, py + 6.0, *cw - 4.0, 22.0)));
            cx += cw;
        }
        for i in 0..LEDGER_ROWS {
            l.buttons.push((Action::Row(i), rc(px + 8.0, py + 34.0 + ROW_H * i as f32, w - 16.0, ROW_H)));
        }
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
struct TrendBar(usize);
#[derive(Component)]
struct LedgerCell;
#[derive(Component)]
struct ScoutMarker;
#[derive(Component)]
struct BeaverMarker;
#[derive(Component)]
struct LodgeMarker;

const TREND_BARS: usize = 40;

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
            .init_resource::<UiState>()
            .insert_resource(FogBase { generation: u32::MAX, colors: Vec::new() })
            .add_systems(Startup, (spawn_ui, spawn_visuals))
            .add_systems(Update, (keys, layout_ui, crate::build::layout_build, detect_click, crate::build::build_input).chain().in_set(ClickSet))
            .add_systems(
                Update,
                (apply_fog, sync_markers, update_ui, refresh_ledger, draw_gizmos).after(ClickSet),
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
            p.spawn(Node { position_type: PositionType::Absolute, right: px(12), bottom: px(8), width: px(TREND_BARS as f32 * 3.5), height: px(34), align_items: AlignItems::FlexEnd, column_gap: px(0.5), ..default() })
                .with_children(|c| {
                    for i in 0..TREND_BARS {
                        c.spawn((Node { width: px(3), height: px(1), ..default() }, BackgroundColor(Color::srgb(0.45, 0.75, 0.5)), TrendBar(i)));
                    }
                });
        });
    commands.spawn((PanelNode(PanelKind::Ledger), abs(()), BackgroundColor(PANEL_BG)));
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
    spawn_btn(Action::Ledger, "Ledger L");
    spawn_btn(Action::Detail, "Detail Tab");
    for (i, (name, _)) in COLS.iter().enumerate() {
        spawn_btn(Action::Sort(i), name);
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

fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<UiState>,
    mut sel: ResMut<Selection>,
    sim: Res<Sim>,
) {
    if keys.just_pressed(KeyCode::KeyL) {
        ui.ledger = !ui.ledger;
    }
    if keys.just_pressed(KeyCode::Tab) {
        ui.detail = !ui.detail;
    }
    if keys.just_pressed(KeyCode::Escape) {
        *sel = Selection::None;
    }
    let max_off = sim.0.colony.population().saturating_sub(LEDGER_ROWS);
    if keys.just_pressed(KeyCode::PageDown) || keys.just_pressed(KeyCode::ArrowDown) {
        ui.offset = (ui.offset + if keys.just_pressed(KeyCode::PageDown) { LEDGER_ROWS } else { 1 }).min(max_off);
    }
    if keys.just_pressed(KeyCode::PageUp) || keys.just_pressed(KeyCode::ArrowUp) {
        ui.offset = ui.offset.saturating_sub(if keys.just_pressed(KeyCode::PageUp) { LEDGER_ROWS } else { 1 });
    }
}

fn layout_ui(
    windows: Query<&Window, With<PrimaryWindow>>,
    ui: Res<UiState>,
    flow: Res<Flow>,
    mut layout: ResMut<UiLayout>,
    mut nodes: Query<(&mut Node, Option<&Btn>, Option<&PanelNode>)>,
) {
    let Ok(w) = windows.single() else { return };
    *layout = compute_layout(Vec2::new(w.width(), w.height()), &ui, flow.phase == Phase::GameOver);
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
    mut ui: ResMut<UiState>,
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
            Some(a) => do_action(a, &mut clock, &mut ui, &mut sim, &mut flow, &mut sel),
            None if layout.over_ui(cursor) => {}
            None => wc.0 = Some(cursor),
        }
    }
}

fn do_action(a: Action, clock: &mut SimClock, ui: &mut UiState, sim: &mut Sim, flow: &mut Flow, sel: &mut Selection) {
    match a {
        Action::Pause => clock.toggle_pause(),
        Action::Speed(s) => clock.set_speed(s),
        Action::Ledger => ui.ledger = !ui.ledger,
        Action::Detail => ui.detail = !ui.detail,
        Action::Continue => {
            if flow.fire(FlowEvent::Continue) {
                clock.paused = false;
            }
        }
        Action::Restart => restart(&mut sim.0, flow, clock, sel),
        Action::Sort(c) => {
            if ui.sort_col == c {
                ui.sort_asc = !ui.sort_asc;
            } else {
                ui.sort_col = c;
                ui.sort_asc = true;
            }
            ui.refresh = 0.0;
        }
        Action::Row(i) => {
            if let Some(id) = ui.rows.get(ui.offset + i) {
                *sel = Selection::Beaver(*id);
            }
        }
    }
}

// ───────────────────────────── text ─────────────────────────────

fn season_name(sim: &Sim) -> String {
    format!("{:?}", sim.0.water.season())
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
fn info_text(sim: &Sim, flow: &Flow, sel: Selection, hover: &Hover, ui: &UiState, clock: &SimClock) -> String {
    let c = &sim.0.colony;
    let mut s = String::new();
    let speed = if clock.paused { "PAUSED".to_string() } else { clock.speed.label().to_string() };
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
    let year = (sim.0.time_days / 365.0) as u32;
    s += &format!("Day {:.0} (year {})  {}  speed {}\n", sim.0.time_days, year + 1, season_name(sim), speed);
    s += &format!("Beavers {}   food {:.0} ({:.1}/head)   wood {:.0}\n", c.population(), c.food, c.food_per_capita(), c.wood);
    s += &selection_text(sim, sel, hover.hint);
    if ui.detail {
        use sim::colony::Stage;
        let st = &c.stats;
        s += &format!("\n\nBirths {}  deaths {}  departures {}  immigrants {}  peak {}", st.births, st.deaths, st.departures, st.immigrants, st.peak_population);
        s += &format!("\nKits {}  yearlings {}  adults {}", c.count_stage(Stage::Kit), c.count_stage(Stage::Yearling), c.count_stage(Stage::Adult));
        let n = c.population().max(1) as f32;
        s += &format!("\nAvg mood {:.0}%  avg health {:.0}%  lodges {}  families {}", c.beavers.iter().map(|b| b.mood).sum::<f32>() / n * 100.0, c.beavers.iter().map(|b| b.health).sum::<f32>() / n * 100.0, c.lodges.len(), c.families.len());
        s += &format!("\nPond volume {:.0}  tools {:.0}%  orders {}", sim.0.water.total_volume(), c.tools * 100.0, c.orders.len());
        s += "\nRecent:";
        for l in flow.log.iter().rev().take(6) {
            s += &format!("\n  {l}");
        }
    }
    s
}

fn update_ui(
    sim: Res<Sim>,
    flow: Res<Flow>,
    sel: Res<Selection>,
    hover: Res<Hover>,
    ui: Res<UiState>,
    clock: Res<SimClock>,
    layout: Res<UiLayout>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut texts: ParamSet<(
        Query<&mut Text, With<InfoText>>,
        Query<&mut Text, With<BannerText>>,
        Query<&mut Text, With<GameOverText>>,
        Query<(&BtnLabel, &mut Text)>,
    )>,
    mut bars: Query<(&TrendBar, &mut Node)>,
    mut btn_colors: Query<(&Btn, &mut BackgroundColor)>,
) {
    let text = info_text(&sim, &flow, *sel, &hover, &ui, &clock);
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
            Action::Sort(i) => format!("{}{}", COLS[i].0, if ui.sort_col == i { if ui.sort_asc { " ^" } else { " v" } } else { "" }),
            _ => continue,
        };
        if t.0 != label {
            t.0 = label;
        }
    }
    // trend bars: population over the last samples
    let hist: Vec<usize> = flow.history.iter().rev().take(TREND_BARS).rev().map(|h| h.1).collect();
    let max = hist.iter().copied().max().unwrap_or(1).max(1) as f32;
    let start = TREND_BARS - hist.len();
    for (b, mut n) in &mut bars {
        n.height = px(if b.0 >= start { 2.0 + 32.0 * hist[b.0 - start] as f32 / max } else { 1.0 });
    }
    // button states
    let cursor = windows.single().ok().and_then(|w| w.cursor_position());
    for (b, mut bg) in &mut btn_colors {
        let hot = cursor.zip(layout.rect(b.0)).is_some_and(|(c, r)| r.contains(c));
        let on = match b.0 {
            Action::Pause => clock.paused,
            Action::Speed(s) => !clock.paused && clock.speed == s,
            Action::Ledger => ui.ledger,
            Action::Detail => ui.detail,
            _ => false,
        };
        bg.0 = if hot { BTN_HOT } else if on { BTN_ON } else { BTN_BG };
    }
}

// ───────────────────────────── ledger ─────────────────────────────

fn sort_key(b: &Beaver, col: usize, sim: &sim::SimState) -> (String, f32) {
    match col {
        0 => (b.name.clone(), 0.0),
        1 => (sim.colony.family(b.family).map(|f| f.name.clone()).unwrap_or_default(), 0.0),
        2 => (String::new(), b.age_days),
        3 => (job_name(b), 0.0),
        4 => (String::new(), b.mood),
        _ => (String::new(), b.health),
    }
}

fn refresh_ledger(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    sel: Res<Selection>,
    layout: Res<UiLayout>,
    mut ui: ResMut<UiState>,
    old: Query<Entity, With<LedgerCell>>,
) {
    ui.refresh -= time.delta_secs();
    if ui.refresh > 0.0 && ui.ledger {
        return;
    }
    ui.refresh = 0.4;
    for e in &old {
        commands.entity(e).despawn();
    }
    if !ui.ledger {
        return;
    }
    let c = &sim.0.colony;
    let mut rows: Vec<&Beaver> = c.beavers.iter().collect();
    let col = ui.sort_col;
    rows.sort_by(|a, b| {
        let (ka, kb) = (sort_key(a, col, &sim.0), sort_key(b, col, &sim.0));
        ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1)).then(a.id.0.cmp(&b.id.0))
    });
    if !ui.sort_asc {
        rows.reverse();
    }
    ui.rows = rows.iter().map(|b| b.id).collect();
    ui.offset = ui.offset.min(rows.len().saturating_sub(LEDGER_ROWS));
    let Some(panel) = layout.panel(PanelKind::Ledger) else { return };
    let cell = |commands: &mut Commands, x: f32, y: f32, s: String, color: Color| {
        commands.spawn((LedgerCell, Text::new(s), font(13.0), TextColor(color), Node { position_type: PositionType::Absolute, left: px(x), top: px(y), ..default() }));
    };
    for (i, b) in rows.iter().skip(ui.offset).take(LEDGER_ROWS).enumerate() {
        let y = panel.min.y + 36.0 + ROW_H * i as f32;
        if *sel == Selection::Beaver(b.id) {
            commands.spawn((LedgerCell, Node { position_type: PositionType::Absolute, left: px(panel.min.x + 8.0), top: px(y - 2.0), width: px(panel.width() - 16.0), height: px(ROW_H), ..default() }, BackgroundColor(Color::srgba(0.3, 0.5, 0.8, 0.4))));
        }
        let fam = c.family(b.family).map(|f| f.name.clone()).unwrap_or_default();
        let mood_col = if b.mood < 0.35 { Color::srgb(1.0, 0.5, 0.45) } else { Color::WHITE };
        let vals = [b.name.clone(), fam, format!("{:.1}", b.age_years()), job_name(b), format!("{:.0}%", b.mood * 100.0), format!("{:.0}%", b.health * 100.0)];
        let mut x = panel.min.x + 12.0;
        for (k, v) in vals.into_iter().enumerate() {
            cell(&mut commands, x, y, v, if k == 4 { mood_col } else { Color::WHITE });
            x += COLS[k].1;
        }
    }
    let foot = format!("{} beavers  rows {}-{}  (Up/Down scroll, click header to sort)", rows.len(), ui.offset + 1, (ui.offset + LEDGER_ROWS).min(rows.len()));
    cell(&mut commands, panel.min.x + 12.0, panel.max.y - 20.0, foot.chars().take(80).collect(), Color::srgb(0.7, 0.75, 0.8));
}

// ───────────────────────────── fog ─────────────────────────────

#[derive(Resource)]
struct FogBase {
    generation: u32,
    colors: Vec<[f32; 4]>,
}

const FOG_COLOR: [f32; 3] = [0.14, 0.17, 0.22];

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
    mut ui: ResMut<UiState>,
    demo: Option<Res<crate::build::DemoDam>>,
) {
    let _ = &orbit;
    // Debug aid for screenshots: BC_AUTOPLAY_UI=ledger,detail,gameover
    if let Ok(v) = std::env::var("BC_AUTOPLAY_UI") {
        if flow.phase == Phase::Settled && flow.real_secs - flow.settled_at.unwrap_or(0.0) > 0.5 {
            ui.ledger = v.contains("ledger");
            ui.detail = v.contains("detail");
            if v.contains("gameover") {
                flow.phase = Phase::GameOver;
            }
        }
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
