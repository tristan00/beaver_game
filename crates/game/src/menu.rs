//! Start menu, pause menu and settings. The game opens on the start menu over the slowly turning
//! valley; a new game begins paused. Esc with nothing else open brings up the pause menu. While a
//! menu is up the sim stands still, the HUD hides and its keys stand down (`hud::MenuOpen`).
#![allow(deprecated)] // `Interaction` drives the buttons, as in the HUD
use crate::camera::OrbitCamera;
use crate::cli::Cli;
use crate::flow::{Flow, Selection};
use crate::hud::{self, MenuOpen, ACCENT, BAD, FAINT, GOOD, INK, MUTED, PANEL, TIMBER};
use crate::sim_driver::SimClock;
use crate::state::{CurrentMap, MapChoice, PlannedDam, Sim};
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use sim::hud::Severity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Start,
    Pause,
    Settings,
}

#[derive(Resource)]
pub struct Menu {
    pub screen: Option<Screen>,
    /// Where Back from settings returns.
    back_to: Screen,
    /// Whether the sim was paused before the pause menu opened (Resume restores it).
    was_paused: bool,
    /// The map the next new game is played on.
    pub map: MapChoice,
    pub ui_scale: f32,
    /// Pause the game when a new urgent problem appears.
    pub auto_pause: bool,
    /// Urgent problem titles already seen (auto-pause fires once per new one).
    seen_urgent: Vec<String>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum MenuAction {
    NewGame,
    MapPrev,
    MapNext,
    SeedPrev,
    SeedNext,
    OpenSettings,
    Back,
    Resume,
    Restart,
    ToTitle,
    Quit,
    Scale(f32),
    ToggleAutoPause,
}

#[derive(Component)]
struct MenuRoot;

pub struct MenuPlugin;

const SCALES: [f32; 4] = [0.85, 1.0, 1.15, 1.3];

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<Cli>().clone();
        // Scripted scenes and plain screenshots run without menus; BC_MENU=start|pause|settings
        // opens one for a screenshot.
        let forced = std::env::var("BC_MENU").ok();
        let scripted = cli.plain_world() || cli.demo_pond || (cli.screenshot.is_some() && forced.is_none());
        let first = match forced.as_deref() {
            Some("pause") => Some(Screen::Pause),
            Some("settings") => Some(Screen::Settings),
            _ if scripted => None,
            _ => Some(Screen::Start),
        };
        app.insert_resource(Menu {
            screen: first,
            back_to: if first == Some(Screen::Pause) { Screen::Pause } else { Screen::Start },
            was_paused: true,
            map: match cli.map.as_deref() {
                Some("generated") => MapChoice::Generated(cli.seed),
                Some(id) => MapChoice::Real(id.to_string()),
                None => MapChoice::first(),
            },
            ui_scale: 1.0,
            auto_pause: true,
            seen_urgent: Vec::new(),
        })
        .init_resource::<MenuOpen>()
        .add_systems(Startup, spawn_root)
        .add_systems(Update, (menu_keys, menu_actions, hold_still, auto_pause, title_orbit, rebuild_menu).chain().before(hud::HudSet));
    }
}

fn spawn_root(mut commands: Commands) {
    commands.spawn((
        MenuRoot,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.04, 0.03, 0.45)),
        GlobalZIndex(50),
        Interaction::default(),
        FocusPolicy::Block,
        Visibility::Hidden,
    ));
}

/// Esc opens the pause menu when nothing else is open, and closes a menu again.
fn menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<Menu>,
    hud: Option<Res<hud::HudState>>,
    sel: Res<Selection>,
    typing: Option<Res<hud::TextCapture>>,
    mut clock: ResMut<SimClock>,
) {
    if !keys.just_pressed(KeyCode::Escape) || typing.is_some_and(|t| t.0) {
        return;
    }
    match menu.screen {
        Some(Screen::Pause) => {
            menu.screen = None;
            clock.paused = menu.was_paused;
        }
        Some(Screen::Settings) => menu.screen = Some(menu.back_to),
        Some(Screen::Start) => {}
        None => {
            let busy = hud.as_ref().is_some_and(|h| h.tool != hud::Tool::None || h.open_cat.is_some() || h.ledger_open || h.search.is_some());
            if !busy && *sel == Selection::None {
                menu.was_paused = clock.paused;
                clock.paused = true;
                menu.screen = Some(Screen::Pause);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn menu_actions(
    mut commands: Commands,
    q: Query<(&Interaction, &MenuAction), Changed<Interaction>>,
    mut menu: ResMut<Menu>,
    mut sim: ResMut<Sim>,
    mut current: ResMut<CurrentMap>,
    mut planned: ResMut<PlannedDam>,
    mut flow: ResMut<Flow>,
    mut clock: ResMut<SimClock>,
    mut orbit: ResMut<OrbitCamera>,
    mut sel: ResMut<Selection>,
    mut scale: ResMut<UiScale>,
    mut exit: MessageWriter<AppExit>,
) {
    let pressed: Vec<MenuAction> = q.iter().filter(|(i, _)| **i == Interaction::Pressed).map(|(_, a)| *a).collect();
    for a in pressed {
        match a {
            MenuAction::NewGame | MenuAction::Restart => {
                // Keep the world already on screen if it is the chosen map and nothing has happened.
                let fresh = current.0 == menu.map && sim.0.time_days == 0.0 && a == MenuAction::NewGame;
                if !fresh {
                    let map = if a == MenuAction::Restart { current.0.clone() } else { menu.map.clone() };
                    match crate::startflow::new_game(&map, &mut sim, &mut planned, &mut flow, &mut clock, &mut orbit, &mut sel) {
                        Ok(()) => {
                            current.0 = map;
                            commands.queue(hud::reset_for_new_game)
                        }
                        Err(e) => {
                            error!("{e}");
                            continue;
                        }
                    }
                } else {
                    // Same world: frame the family again (the title screen turned the camera).
                    crate::startflow::begin(&sim.0, &mut flow, &mut clock, &mut orbit, &mut sel);
                }
                clock.paused = true;
                menu.seen_urgent.clear();
                menu.screen = None;
            }
            MenuAction::MapPrev | MenuAction::MapNext => {
                // Real maps in order, then the generated valley.
                let mut options: Vec<MapChoice> = sim::maps::all().into_iter().map(|m| MapChoice::Real(m.id)).collect();
                let seed = match menu.map {
                    MapChoice::Generated(s) => s,
                    _ => 1,
                };
                options.push(MapChoice::Generated(seed));
                let i = options.iter().position(|o| *o == menu.map).unwrap_or(0);
                let n = options.len();
                let j = if a == MenuAction::MapNext { (i + 1) % n } else { (i + n - 1) % n };
                menu.map = options[j].clone();
            }
            MenuAction::SeedPrev | MenuAction::SeedNext => {
                if let MapChoice::Generated(s) = menu.map {
                    menu.map = MapChoice::Generated(if a == MenuAction::SeedNext { s.saturating_add(1) } else { s.saturating_sub(1).max(1) });
                }
            }
            MenuAction::OpenSettings => {
                menu.back_to = menu.screen.unwrap_or(Screen::Pause);
                menu.screen = Some(Screen::Settings);
            }
            MenuAction::Back => menu.screen = Some(menu.back_to),
            MenuAction::Resume => {
                menu.screen = None;
                clock.paused = menu.was_paused;
            }
            MenuAction::ToTitle => {
                menu.map = current.0.clone();
                menu.screen = Some(Screen::Start);
            }
            MenuAction::Quit => {
                exit.write(AppExit::Success);
            }
            MenuAction::Scale(s) => {
                menu.ui_scale = s;
                scale.0 = s;
            }
            MenuAction::ToggleAutoPause => menu.auto_pause = !menu.auto_pause,
        }
    }
}

/// Publishes whether a menu is up, and keeps the sim still under it.
fn hold_still(menu: Res<Menu>, mut open: ResMut<MenuOpen>, mut clock: ResMut<SimClock>) {
    let up = menu.screen.is_some();
    if open.0 != up {
        open.0 = up;
    }
    if up && !clock.paused {
        clock.paused = true;
    }
}

/// Pauses once for each new urgent problem (food about to run out, a dam about to break), if on.
fn auto_pause(time: Res<Time>, mut next: Local<f32>, sim: Res<Sim>, mut menu: ResMut<Menu>, mut clock: ResMut<SimClock>, hud_state: Option<ResMut<hud::HudState>>) {
    let now = time.elapsed_secs();
    if now < *next || menu.screen.is_some() {
        return;
    }
    *next = now + 1.0;
    let urgent: Vec<String> = sim::hud::problems(&sim.0).into_iter().filter(|p| p.severity == Severity::Urgent).map(|p| p.title).collect();
    let new: Vec<String> = urgent.iter().filter(|t| !menu.seen_urgent.contains(t)).cloned().collect();
    menu.seen_urgent = urgent;
    if menu.auto_pause && !clock.paused {
        if let Some(t) = new.first() {
            clock.paused = true;
            if let Some(mut h) = hud_state {
                h.msg = Some((format!("Paused: {t} (auto-pause, change it in Settings)"), now));
            }
        }
    }
}

/// The title screen slowly turns the camera around the valley.
fn title_orbit(time: Res<Time>, menu: Res<Menu>, mut orbit: ResMut<OrbitCamera>) {
    if menu.screen == Some(Screen::Start) {
        orbit.yaw += time.delta_secs() * 0.06;
        if orbit.distance < 60.0 {
            orbit.distance += (60.0 - orbit.distance) * (time.delta_secs() * 0.8).min(1.0);
        }
    }
}

// ───────────────────────────── drawing ─────────────────────────────

fn button(p: &mut ChildSpawnerCommands, label: &str, action: MenuAction, fill: Color, on: bool) {
    let ink = if fill == ACCENT { Color::srgb(0.13, 0.09, 0.05) } else { INK };
    p.spawn((
        Interaction::default(),
        FocusPolicy::Block,
        action,
        Node {
            padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
            border: UiRect::all(Val::Px(if on { 2.0 } else { 1.0 })),
            border_radius: BorderRadius::all(Val::Px(5.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(if on { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.14) }),
    ))
    .with_children(|b| hud::bold(b, label, 14.0, ink));
}

fn wide(p: &mut ChildSpawnerCommands, label: &str, action: MenuAction, fill: Color) {
    p.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|c| button(c, label, action, fill, false));
}

fn rebuild_menu(
    mut commands: Commands,
    menu: Res<Menu>,
    sim: Res<Sim>,
    current: Res<CurrentMap>,
    clock: Res<SimClock>,
    mut root: Query<(Entity, &mut Visibility, &mut BackgroundColor), With<MenuRoot>>,
) {
    if !menu.is_changed() {
        return;
    }
    let Ok((e, mut vis, mut bg)) = root.single_mut() else { return };
    let want = if menu.screen.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    commands.entity(e).despawn_related::<Children>();
    let Some(screen) = menu.screen else { return };
    // The title screen lets the valley show; pause and settings dim the game more.
    bg.0 = if screen == Screen::Start { Color::srgba(0.05, 0.04, 0.03, 0.25) } else { Color::srgba(0.05, 0.04, 0.03, 0.55) };
    let card = Node {
        width: Val::Px(440.0),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(10.0),
        padding: UiRect::all(Val::Px(22.0)),
        border: UiRect { top: Val::Px(3.0), ..UiRect::all(Val::Px(1.0)) },
        border_radius: BorderRadius::all(Val::Px(9.0)),
        ..default()
    };
    let border = BorderColor { top: TIMBER, ..BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.08)) };
    commands.entity(e).with_children(|r| {
        r.spawn((card, BackgroundColor(PANEL), border)).with_children(|p| match screen {
            Screen::Start => start_screen(p, &menu, &current.0),
            Screen::Pause => pause_screen(p, &sim.0, &clock),
            Screen::Settings => settings_screen(p, &menu),
        });
    });
}

fn start_screen(p: &mut ChildSpawnerCommands, menu: &Menu, current: &MapChoice) {
    hud::bold(p, "Beaver Colony", 30.0, INK);
    hud::text(p, "Shape the river, feed the family, build a colony.", 13.0, MUTED);
    p.spawn(Node { height: Val::Px(6.0), ..default() });
    wide(p, "New game", MenuAction::NewGame, ACCENT);
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| {
        button(r, "<", MenuAction::MapPrev, FAINT, false);
        r.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, flex_grow: 1.0, ..default() }).with_children(|c| {
            hud::bold(c, menu.map.label(), 14.0, INK);
            match &menu.map {
                MapChoice::Real(id) => {
                    if let Some(m) = sim::maps::all().into_iter().find(|m| &m.id == id) {
                        let ew = if m.lon < 0.0 { "W" } else { "E" };
                        hud::text(c, format!("Real terrain, {:.1} km across, {:.2} N {:.2} {ew}", m.km(), m.lat, m.lon.abs()), 11.0, MUTED);
                    }
                }
                MapChoice::Generated(_) => {
                    c.spawn(hud::row(6.0)).with_children(|r| {
                        button(r, "seed -", MenuAction::SeedPrev, FAINT, false);
                        button(r, "seed +", MenuAction::SeedNext, FAINT, false);
                    });
                }
            }
        });
        button(r, ">", MenuAction::MapNext, FAINT, false);
    });
    let same = menu.map == *current;
    hud::text(p, if same { "The valley behind this menu is the map you will play." } else { "New game builds this map (it takes a few seconds)." }, 11.0, MUTED);
    wide(p, "Settings", MenuAction::OpenSettings, FAINT);
    wide(p, "Quit", MenuAction::Quit, FAINT);
    hud::text(p, "The game starts paused: press Space or 1, 2, 3 to let time run.", 11.0, MUTED);
    hud::text(p, "Elevation: AWS Open Data Terrain Tiles (SRTM, USGS 3DEP, NRCan CDEM, EU-DEM).", 9.5, MUTED);
}

fn pause_screen(p: &mut ChildSpawnerCommands, s: &sim::SimState, clock: &SimClock) {
    let d = sim::calendar::Date::at(s.time_days);
    hud::bold(p, "Paused", 26.0, INK);
    hud::text(p, format!("{} {}, year {} - {} beavers - seed {}", d.season.name(), d.day, d.year, s.colony.population(), s.seed), 12.0, MUTED);
    let _ = clock;
    wide(p, "Resume", MenuAction::Resume, ACCENT);
    wide(p, "Settings", MenuAction::OpenSettings, FAINT);
    wide(p, "Restart this map", MenuAction::Restart, FAINT);
    wide(p, "Quit to title", MenuAction::ToTitle, FAINT);
    wide(p, "Quit game", MenuAction::Quit, FAINT);
    hud::text(p, "There is no saving yet: restarting or quitting loses this colony.", 11.0, BAD);
    hud::text(p, "Esc resumes.", 11.0, MUTED);
}

fn settings_screen(p: &mut ChildSpawnerCommands, menu: &Menu) {
    hud::bold(p, "Settings", 24.0, INK);
    hud::bold(p, "INTERFACE SIZE", 10.0, MUTED);
    p.spawn(hud::row(6.0)).with_children(|r| {
        for s in SCALES {
            let label = format!("{:.0}%", s * 100.0);
            button(r, &label, MenuAction::Scale(s), FAINT, (menu.ui_scale - s).abs() < 0.01);
        }
    });
    hud::bold(p, "ALERTS", 10.0, MUTED);
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| {
        hud::text(r, "Pause when an urgent problem appears", 13.0, INK);
        let (label, c) = if menu.auto_pause { ("On", GOOD) } else { ("Off", MUTED) };
        r.spawn(hud::row(6.0)).with_children(|r| {
            hud::bold(r, label, 13.0, c);
            button(r, "Change", MenuAction::ToggleAutoPause, FAINT, false);
        });
    });
    hud::bold(p, "CONTROLS", 10.0, MUTED);
    for (k, v) in [
        ("Select, place, draw a dam", "left click, drag"),
        ("Stop a tool, close", "right click, Esc"),
        ("Orbit / pan / rotate / zoom", "right drag / WASD / Q E / wheel"),
        ("Build categories", "Z X C V B N"),
        ("Dam crest", "[ ] or Ctrl+wheel, Enter builds"),
        ("World / Water / Goods view", "F1 F2 F3, hold Alt for water"),
        ("Flow ledger / search / undo", "L  /  Ctrl+Z"),
        ("Pause / speed", "Space, 1 2 3"),
        ("Pause menu", "Esc with nothing open"),
    ] {
        p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| {
            hud::text(r, k, 11.5, MUTED);
            hud::text(r, v, 11.5, INK);
        });
    }
    wide(p, "Back", MenuAction::Back, ACCENT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_include_the_default_and_grow() {
        assert!(SCALES.contains(&1.0));
        assert!(SCALES.windows(2).all(|w| w[0] < w[1]));
    }
}
