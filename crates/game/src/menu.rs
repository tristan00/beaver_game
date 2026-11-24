#![allow(deprecated)]
use crate::camera::OrbitCamera;
use crate::cli::Cli;
use crate::flow::{Flow, Selection};
use crate::hud::{self, MenuOpen, ACCENT, FAINT, INK, MUTED, PANEL, TIMBER};
use crate::icons::{icon_node, Icon, Icons};
use crate::sim_driver::SimClock;
use crate::state::{CurrentMap, MapChoice, PlannedDam, Sim};
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Start,
    Pause,
    Settings,
    Load,
}

#[derive(Resource)]
pub struct Menu {
    pub screen: Option<Screen>,
    back_to: Screen,
    was_paused: bool,
    pub map: MapChoice,
    pub ui_scale: f32,
    pub water: sim::water::EngineKind,
    confirm: Option<MenuAction>,
    loading: bool,
    load_error: Option<String>,
    /// The saves the load screen lists, newest first.
    saves: Vec<std::path::PathBuf>,
    /// How the last save asked for from this menu went, shown on its Save button.
    pub saved: Option<SaveNote>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SaveNote {
    Saving,
    Saved,
    Failed(String),
}

type LoadedValley = Result<(sim::SimState, Option<sim::starts::DamSite>), String>;

#[derive(Resource)]
struct PendingValley {
    map: MapChoice,
    /// Built only to stand behind the start screen for the map the selector shows.
    preview: bool,
    result: std::sync::Mutex<std::sync::mpsc::Receiver<LoadedValley>>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum MenuAction {
    NewGame,
    /// Open the list of saved games.
    OpenLoad,
    /// Load the save at this place in the list.
    LoadSave(usize),
    Save,
    MapPrev,
    MapNext,
    OpenSettings,
    Back,
    Resume,
    Restart,
    ToTitle,
    Quit,
    Scale(f32),
    Water(sim::water::EngineKind),
    Fahrenheit(bool),
    Volume(crate::audio::Channel, f32),
    Ask(Ask),
    Confirm(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ask {
    Restart,
    ToTitle,
    Quit,
}

#[derive(Component)]
struct MenuRoot;

pub struct MenuPlugin;

const SCALES: [f32; 4] = [0.85, 1.0, 1.15, 1.3];
const LEVELS: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<Cli>().clone();
        let forced = std::env::var("BC_MENU").ok();
        let benchmarking = std::env::var_os("BC_PERF").is_some()
            && (std::env::var_os("BC_SPEED").is_some() || std::env::var_os("BC_REPORT_BENCH").is_some());
        let scripted = cli.colony_script.is_some() || cli.sandbox || cli.load.is_some()
            || (benchmarking && std::env::var_os("BC_SETTLE").is_some())
            || (cli.screenshot.is_some() && forced.is_none());
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
            map: app.world().resource::<CurrentMap>().0.clone(),
            ui_scale: 1.0,
            water: sim::water::EngineKind::from_env(),
            confirm: None,
            loading: false,
            load_error: None,
            saves: Vec::new(),
            saved: None,
        })
        .init_resource::<MenuOpen>()
        .add_systems(Startup, spawn_root)
        .add_systems(Update, (menu_keys, load_keys, menu_actions, finish_loading, hold_still, pause_on_collapse, title_orbit, rebuild_menu).chain().before(hud::HudSet));
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

fn menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<Menu>,
    hud: Option<Res<hud::HudState>>,
    sel: Res<Selection>,
    typing: Option<Res<hud::TextCapture>>,
    mut clock: ResMut<SimClock>,
) {
    if menu.loading || !keys.just_pressed(KeyCode::Escape) || typing.is_some_and(|t| t.0) {
        return;
    }
    match menu.screen {
        Some(Screen::Pause) => {
            menu.screen = None;
            clock.paused = menu.was_paused;
        }
        Some(Screen::Settings) | Some(Screen::Load) => menu.screen = Some(menu.back_to),
        Some(Screen::Start) => {}
        None => {
            let busy = hud.as_ref().is_some_and(|h| h.tool != hud::Tool::None || h.colony.is_some() || h.ledger.is_some() || h.order_mode.is_some() || h.help);
            if !busy && *sel == Selection::None {
                menu.saved = None;
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
    current: Res<CurrentMap>,
    planned: Res<PlannedDam>,
    mut flow: ResMut<Flow>,
    mut clock: ResMut<SimClock>,
    mut orbit: ResMut<OrbitCamera>,
    mut sel: ResMut<Selection>,
    mut scale: ResMut<UiScale>,
    mut exit: MessageWriter<AppExit>,
    mut save: ResMut<crate::savegame::SaveRequest>,
) {
    if menu.loading { return; }
    let mut pressed: Vec<MenuAction> = q.iter().filter(|(i, _)| **i == Interaction::Pressed).map(|(_, a)| *a).collect();
    for a in pressed.iter_mut() {
        if let MenuAction::Confirm(yes) = *a {
            let asked = menu.confirm.take();
            *a = match (yes, asked) {
                (true, Some(MenuAction::Ask(Ask::Restart))) => MenuAction::Restart,
                (true, Some(MenuAction::Ask(Ask::ToTitle))) => MenuAction::ToTitle,
                (true, Some(MenuAction::Ask(Ask::Quit))) => MenuAction::Quit,
                _ => MenuAction::Confirm(false),
            };
        }
    }
    for a in pressed {
        if sim::trace::enabled() { sim::trace::event(format!("MENU_ACTION {a:?}")); }
        match a {
            MenuAction::NewGame | MenuAction::Restart => {
                let fresh = current.0 == menu.map && sim.0.time_days == 0.0 && a == MenuAction::NewGame;
                if !fresh {
                    let map = if a == MenuAction::Restart { current.0.clone() } else { menu.map.clone() };
                    let (send, receive) = std::sync::mpsc::channel();
                    let build_map = map.clone();
                    let sandbox = sim.0.sandbox;
                    std::thread::spawn(move || {
                        let result = if sandbox { build_map.sandbox().map(|s| (s, None)) }
                            else { build_map.build().map(|(s, d)| (s, Some(d))) };
                        let _ = send.send(result);
                    });
                    commands.insert_resource(PendingValley { map, preview: false, result: std::sync::Mutex::new(receive) });
                    menu.loading = true;
                    menu.load_error = None;
                    clock.paused = true;
                    continue;
                } else {
                    let camp = planned.0.as_ref().map(|d| d.camp);
                    if let Err(error) = crate::startflow::begin_play(&mut sim.0, &mut flow, &mut clock, &mut orbit, &mut sel, camp) {
                        menu.load_error = Some(error);
                        continue;
                    }
                }
                clock.paused = true;
                menu.load_error = None;
                menu.screen = None;
            }
            MenuAction::OpenLoad => {
                menu.back_to = menu.screen.unwrap_or(Screen::Pause);
                menu.saves = crate::savegame::saves().into_iter().map(|s| s.0).collect();
                menu.load_error = None;
                menu.screen = Some(Screen::Load);
            }
            MenuAction::LoadSave(i) => {
                if let Some(path) = menu.saves.get(i).cloned() {
                    start_load(&mut commands, &mut menu, &mut clock, Some(path));
                }
                continue;
            }
            MenuAction::Save => {
                save.0 = true;
                menu.saved = Some(SaveNote::Saving);
                continue;
            }
            MenuAction::MapPrev | MenuAction::MapNext => {
                let options = MapChoice::menu();
                let i = options.iter().position(|o| *o == menu.map).unwrap_or(0);
                let n = options.len();
                let j = if a == MenuAction::MapNext { (i + 1) % n } else { (i + n - 1) % n };
                menu.map = options[j].clone();
                if menu.screen == Some(Screen::Start) && menu.map != current.0 {
                    let (send, receive) = std::sync::mpsc::channel();
                    let build_map = menu.map.clone();
                    let sandbox = sim.0.sandbox;
                    std::thread::spawn(move || {
                        let result = if sandbox { build_map.sandbox().map(|s| (s, None)) }
                            else { build_map.build().map(|(s, d)| (s, Some(d))) };
                        let _ = send.send(result);
                    });
                    commands.insert_resource(PendingValley { map: menu.map.clone(), preview: true, result: std::sync::Mutex::new(receive) });
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
            MenuAction::Fahrenheit(on) => set_fahrenheit(on),
            MenuAction::Volume(ch, v) => commands.queue(move |w: &mut World| {
                if let Some(mut vols) = w.get_resource_mut::<crate::audio::AudioVolumes>() {
                    vols.set(ch, v);
                }
            }),
            MenuAction::Water(kind) => {
                menu.water = kind;
                sim::water::set_default_engine(kind);
                sim.0.water.set_engine(kind);
            }
            MenuAction::Ask(_) => menu.confirm = Some(a),
            MenuAction::Confirm(_) => {}
        }
        if menu.screen.is_none() {
            commands.queue(|world: &mut World| {
                if let Some(mut hud) = world.get_resource_mut::<hud::HudState>() { hud.swallow_click(); }
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Load a save (the newest when none is named) in the background; `finish_loading` puts it in play.
fn start_load(commands: &mut Commands, menu: &mut Menu, clock: &mut SimClock, path: Option<std::path::PathBuf>) {
    let Some(path) = path.or_else(crate::savegame::latest_save) else {
        menu.load_error = Some("No saved game".into());
        return;
    };
    let map = match sim::save::peek(&path) {
        Ok(h) => crate::savegame::map_from(&h.map),
        Err(e) => {
            menu.load_error = Some(e);
            return;
        }
    };
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = sim::save::load(&path).map(|(h, s)| {
            sim::trace::event(format!("LOADED path={} day={:.2} map={} saved_by={}", path.display(), h.day, h.map, h.version));
            (s, None)
        });
        let _ = send.send(result);
    });
    commands.insert_resource(PendingValley { map, preview: false, result: std::sync::Mutex::new(receive) });
    menu.loading = true;
    menu.load_error = None;
    clock.paused = true;
}

/// Ctrl+L carries on the newest save from anywhere.
fn load_keys(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands, mut menu: ResMut<Menu>, mut clock: ResMut<SimClock>) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if ctrl && keys.just_pressed(KeyCode::KeyL) && !menu.loading {
        start_load(&mut commands, &mut menu, &mut clock, None);
    }
}

fn finish_loading(
    pending: Option<Res<PendingValley>>, mut commands: Commands, mut menu: ResMut<Menu>,
    mut sim: ResMut<Sim>, mut planned: ResMut<PlannedDam>, mut current: ResMut<CurrentMap>,
    mut flow: ResMut<Flow>, mut clock: ResMut<SimClock>, mut orbit: ResMut<OrbitCamera>, mut sel: ResMut<Selection>,
) {
    let Some(pending) = pending else { return };
    let result = match pending.result.lock().unwrap().try_recv() {
        Ok(result) => result,
        Err(std::sync::mpsc::TryRecvError::Empty) => return,
        Err(_) => Err("The valley could not be prepared. Please try again.".into()),
    };
    if pending.preview {
        // Only the valley the selector still shows, and only while the start screen is up.
        if pending.map == menu.map && menu.screen == Some(Screen::Start) {
            match result {
                Ok((state, dam)) => {
                    crate::startflow::install_preview(state, dam, &mut sim, &mut planned, &mut flow, &mut clock, &mut orbit, &mut sel);
                    current.0 = pending.map.clone();
                    commands.queue(hud::reset_for_new_game);
                }
                Err(error) => menu.load_error = Some(error),
            }
        }
        commands.remove_resource::<PendingValley>();
        return;
    }
    menu.loading = false;
    match result.and_then(|(state, dam)| {
        crate::startflow::install_game(state, dam, &mut sim, &mut planned, &mut flow, &mut clock, &mut orbit, &mut sel)
    }) {
        Ok(()) => {
            current.0 = pending.map.clone();
            menu.screen = None;
            clock.paused = true;
            commands.queue(hud::reset_for_new_game);
        }
        Err(error) => menu.load_error = Some(error),
    }
    commands.remove_resource::<PendingValley>();
}

fn hold_still(menu: Res<Menu>, mut open: ResMut<MenuOpen>, mut clock: ResMut<SimClock>) {
    let up = menu.screen.is_some();
    if open.0 != up {
        open.0 = up;
    }
    if up && !clock.paused {
        clock.paused = true;
    }
}

/// A collapsed colony stops the clock and opens the pause screen.
fn pause_on_collapse(sim: Res<Sim>, mut menu: ResMut<Menu>, mut clock: ResMut<SimClock>) {
    if sim.0.colony.is_collapsed() && menu.screen.is_none() {
        clock.paused = true;
        menu.was_paused = true;
        menu.screen = Some(Screen::Pause);
    }
}

fn title_orbit(time: Res<Time>, menu: Res<Menu>, mut orbit: ResMut<OrbitCamera>) {
    if menu.screen == Some(Screen::Start) {
        orbit.yaw += time.delta_secs() * 0.06;
        if orbit.distance < 60.0 {
            orbit.distance += (60.0 - orbit.distance) * (time.delta_secs() * 0.8).min(1.0);
        }
    }
}

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

/// A small step on a volume row.
fn level_button(p: &mut ChildSpawnerCommands, label: &str, action: MenuAction, on: bool) {
    p.spawn((
        Interaction::default(),
        FocusPolicy::Block,
        action,
        Node {
            width: Val::Px(44.0),
            padding: UiRect::vertical(Val::Px(5.0)),
            border: UiRect::all(Val::Px(if on { 2.0 } else { 1.0 })),
            border_radius: BorderRadius::all(Val::Px(5.0)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(FAINT),
        BorderColor::all(if on { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.14) }),
    ))
    .with_children(|b| hud::bold(b, label, 12.0, INK));
}

fn icon_button(p: &mut ChildSpawnerCommands, icons: &Icons, icon: Icon, action: MenuAction, fill: Color) {
    let ink = if fill == ACCENT { Color::srgb(0.13, 0.09, 0.05) } else { INK };
    p.spawn((
        Interaction::default(),
        FocusPolicy::Block,
        action,
        Node { padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)), border: UiRect::all(Val::Px(1.0)), border_radius: BorderRadius::all(Val::Px(5.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
        BackgroundColor(fill),
        BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.14)),
    ))
    .with_children(|b| {
        b.spawn(icon_node(icons, icon, 18.0, ink));
    });
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
    icons: Res<Icons>,
    vols: Option<Res<crate::audio::AudioVolumes>>,
    mut root: Query<(Entity, &mut Visibility, &mut BackgroundColor), With<MenuRoot>>,
) {
    let volume_moved = menu.screen == Some(Screen::Settings) && vols.as_ref().is_some_and(|v| v.is_changed());
    if !menu.is_changed() && !volume_moved {
        return;
    }
    let Ok((e, mut vis, mut bg)) = root.single_mut() else { return };
    let want = if menu.screen.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    commands.entity(e).despawn_related::<Children>();
    let Some(screen) = menu.screen else { return };
    bg.0 = if screen == Screen::Start { Color::srgba(0.05, 0.04, 0.03, 0.25) } else { Color::srgba(0.05, 0.04, 0.03, 0.55) };
    let card = Node {
        width: Val::Px(440.0),
        max_height: Val::Percent(92.0),
        overflow: Overflow::scroll_y(),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(10.0),
        padding: UiRect::all(Val::Px(22.0)),
        border: UiRect { top: Val::Px(3.0), ..UiRect::all(Val::Px(1.0)) },
        border_radius: BorderRadius::all(Val::Px(9.0)),
        ..default()
    };
    let border = BorderColor { top: TIMBER, ..BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.08)) };
    commands.entity(e).with_children(|r| {
        r.spawn((card, ScrollPosition::default(), bevy::ui::RelativeCursorPosition::default(), Interaction::default(), hud::Blocks, FocusPolicy::Block, BackgroundColor(PANEL), border)).with_children(|p| {
            if menu.loading {
                hud::bold(p, "Preparing your valley...", 22.0, INK);
                hud::text(p, menu.map.label(), 13.0, MUTED);
                return;
            }
            if let Some(error) = &menu.load_error { hud::text(p, error, 13.0, hud::BAD); }
            match screen {
                Screen::Start => start_screen(p, &menu, &icons, &current.0),
                Screen::Pause => pause_screen(p, &menu, &icons, &sim.0, &clock),
                Screen::Settings => settings_screen(p, &menu, &icons, vols.as_deref()),
                Screen::Load => load_screen(p, &menu),
            }
        });
    });
}

fn start_screen(p: &mut ChildSpawnerCommands, menu: &Menu, icons: &Icons, current: &MapChoice) {
    let _ = current;
    hud::bold(p, "Beaver Colony", 30.0, INK);
    p.spawn(Node { height: Val::Px(6.0), ..default() });
    wide(p, "Play", MenuAction::NewGame, ACCENT);
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| {
        button(r, "<", MenuAction::MapPrev, FAINT, false);
        r.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, flex_grow: 1.0, ..default() }).with_children(|c| {
            let label = menu.map.label();
            hud::bold(c, label.split(',').next().unwrap_or("").to_string(), 14.0, INK);
            if let MapChoice::Real(id) = &menu.map {
                if let Some(m) = sim::maps::all().into_iter().find(|m| &m.id == id) {
                    hud::text(c, m.place, 11.0, MUTED);
                }
            }
        });
        button(r, ">", MenuAction::MapNext, FAINT, false);
    });
    if let MapChoice::Real(id) = &menu.map {
        map_stats(p, icons, id);
    }
    if crate::savegame::latest_save().is_some() {
        wide(p, "Load", MenuAction::OpenLoad, FAINT);
    }
    wide(p, "Settings", MenuAction::OpenSettings, FAINT);
    wide(p, "Quit", MenuAction::Quit, FAINT);
}

/// Every saved game, newest first: where, which day and how many beavers. Click one to load it.
fn load_screen(p: &mut ChildSpawnerCommands, menu: &Menu) {
    hud::bold(p, "Load", 22.0, INK);
    if menu.saves.is_empty() {
        hud::text(p, "No saved games", 13.0, MUTED);
    }
    for (i, path) in menu.saves.iter().enumerate() {
        let (title, detail) = match sim::save::peek(path) {
            Ok(h) => {
                let place = crate::savegame::map_from(&h.map).label();
                (place.split(',').next().unwrap_or("").to_string(), format!("Day {:.0} · {} beavers", h.day, h.population))
            }
            Err(e) => (path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), e),
        };
        p.spawn((
            Interaction::default(),
            FocusPolicy::Block,
            MenuAction::LoadSave(i),
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(FAINT),
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.14)),
        ))
        .with_children(|b| {
            hud::bold(b, title, 14.0, INK);
            hud::text(b, detail, 11.0, MUTED);
        });
    }
    wide(p, "Back", MenuAction::Back, FAINT);
}

/// A row of icon + value pairs; hovering one names it.
fn map_stats(p: &mut ChildSpawnerCommands, icons: &Icons, id: &str) {
    let Some(s) = sim::maps::stats(id) else { return };
    let share = |v: f32| if v > 0.0 && v < 0.01 { "<1%".to_string() } else { format!("{:.0}%", v * 100.0) };
    let mut stats = vec![
        (Icon::Upstream, s.streams.to_string(), "Streams flowing in"),
        (Icon::ViewForest, share(s.forest), "Forest"),
        (Icon::ViewWater, share(s.water), "Open water at the start"),
        (Icon::ViewElevation, format!("{:.0} m", s.relief_m), "Lowest to highest ground"),
    ];
    if let Some(c) = &s.climate {
        stats.push((Icon::Winter, format!("{:.0} mo", c.ice_months()), "Months under ice"));
        stats.push((Icon::Gauge, format!("{:.0} mm", c.runoff_year_mm()), "Water the streams carry in a year"));
    }
    p.spawn(Node { flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, column_gap: Val::Px(12.0), row_gap: Val::Px(4.0), ..default() }).with_children(|r| {
        for (icon, value, name) in stats {
            r.spawn((hud::row(3.0), hud::hover_tip(name))).with_children(|c| {
                c.spawn(icon_node(icons, icon, 14.0, MUTED));
                hud::text(c, value, 11.0, MUTED);
            });
        }
    });
}

fn pause_screen(p: &mut ChildSpawnerCommands, menu: &Menu, icons: &Icons, s: &sim::SimState, clock: &SimClock) {
    let _ = (s, clock);
    if menu.confirm.is_some() {
        p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| {
            hud::bold(r, "Lose colony?", 15.0, INK);
            r.spawn(hud::row(8.0)).with_children(|b| {
                icon_button(b, icons, Icon::Cross, MenuAction::Confirm(false), FAINT);
                icon_button(b, icons, Icon::Check, MenuAction::Confirm(true), ACCENT);
            });
        });
        return;
    }
    if s.colony.is_collapsed() {
        hud::bold(p, "The colony has been lost", 22.0, INK);
        hud::text(p, format!("Survived {:.0} days. {} beavers died.", s.time_days, s.colony.stats.deaths), 13.0, MUTED);
    } else {
        wide(p, "Resume", MenuAction::Resume, ACCENT);
        let label = match &menu.saved {
            Some(SaveNote::Saving) => "Saving…",
            Some(SaveNote::Saved) => "Saved",
            _ => "Save",
        };
        wide(p, label, MenuAction::Save, if menu.saved == Some(SaveNote::Saved) { hud::GOOD.with_alpha(0.35) } else { FAINT });
        if let Some(SaveNote::Failed(e)) = &menu.saved {
            hud::text(p, e.clone(), 12.0, hud::BAD);
        }
    }
    if crate::savegame::latest_save().is_some() {
        wide(p, "Load", MenuAction::OpenLoad, FAINT);
    }
    wide(p, "Settings", MenuAction::OpenSettings, FAINT);
    wide(p, "Restart", MenuAction::Ask(Ask::Restart), FAINT);
    wide(p, "Menu", MenuAction::Ask(Ask::ToTitle), FAINT);
    wide(p, "Quit", MenuAction::Ask(Ask::Quit), FAINT);
}

fn settings_screen(p: &mut ChildSpawnerCommands, menu: &Menu, icons: &Icons, vols: Option<&crate::audio::AudioVolumes>) {
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| {
        hud::bold(r, "Settings", 22.0, INK);
        icon_button(r, icons, Icon::Back, MenuAction::Back, FAINT);
    });
    if let Some(vols) = vols {
        hud::bold(p, "Sound", 12.0, MUTED);
        for ch in crate::audio::Channel::ALL {
            let name = match ch {
                crate::audio::Channel::Master => "All",
                crate::audio::Channel::Water => "Water",
                crate::audio::Channel::Alerts => "Alerts",
            };
            let now = vols.level(ch);
            let nearest = LEVELS.iter().copied().fold(LEVELS[0], |a, l| if (l - now).abs() < (a - now).abs() { l } else { a });
            p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(6.0) }).with_children(|r| {
                hud::text(r, name, 12.0, INK);
                r.spawn(hud::row(4.0)).with_children(|b| {
                    for l in LEVELS {
                        let label = if l == 0.0 { "Off".to_string() } else { format!("{:.0}", l * 100.0) };
                        level_button(b, &label, MenuAction::Volume(ch, l), l == nearest);
                    }
                });
            });
        }
    }
    hud::bold(p, "UI size", 12.0, MUTED);
    p.spawn(hud::row(6.0)).with_children(|r| {
        for s in SCALES {
            let label = format!("{:.0}%", s * 100.0);
            button(r, &label, MenuAction::Scale(s), FAINT, (menu.ui_scale - s).abs() < 0.01);
        }
    });
    hud::bold(p, "Temperature", 12.0, MUTED);
    p.spawn(hud::row(6.0)).with_children(|r| {
        let f = fahrenheit();
        button(r, "°C", MenuAction::Fahrenheit(false), FAINT, !f);
        button(r, "°F", MenuAction::Fahrenheit(true), FAINT, f);
    });
    hud::bold(p, "Water", 12.0, MUTED);
    p.spawn(hud::row(6.0)).with_children(|r| {
        for kind in sim::water::EngineKind::ALL {
            button(r, water_label(kind), MenuAction::Water(kind), FAINT, menu.water == kind);
        }
    });
    hud::bold(p, "Keys", 12.0, MUTED);
    for (k, v) in [
        ("Select", "LMB"),
        ("Order selected beavers", "RMB"),
        ("Step back", "RMB / Esc"),
        ("Pan", "WASD / arrows"),
        ("Rotate", "Q E / middle drag"),
        ("Zoom", "Wheel"),
        ("Dam, shelter, warehouse", "Z X C"),
        ("Gather, cut trees", "G T"),
        ("Build the dam", "Enter"),
        ("Undo", "Ctrl+Z"),
        ("Water, forest, height", "F1 F2 F3"),
        ("Peek water", "Alt"),
        ("Tree canopies", "F8"),
        ("Colony book", "Tab"),
        ("Next free beaver", "I"),
        ("Pause, speed", "Space, 1 2 3"),
        ("Volume: all, alerts, water", "- =  ; '  , ."),
        ("Mute", "M"),
        ("Menu", "Esc"),
    ] {
        p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| {
            hud::text(r, k, 11.5, MUTED);
            hud::bold(r, v, 11.5, INK);
        });
    }
    hud::bold(p, "About", 12.0, MUTED);
    hud::text(p, "Elevation: AWS Open Data Terrain Tiles (SRTM, USGS 3DEP, NRCan CDEM, EU-DEM)", 9.5, MUTED);
}

/// Temperatures read in Fahrenheit rather than Celsius: the player's choice, kept between games
/// in a small file beside the saves.
static FAHRENHEIT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(2);
const UNITS_FILE: &str = "units.txt";

fn units_path() -> std::path::PathBuf {
    std::path::Path::new(crate::savegame::SAVE_DIR).join(UNITS_FILE)
}

pub fn fahrenheit() -> bool {
    use std::sync::atomic::Ordering::Relaxed;
    match FAHRENHEIT.load(Relaxed) {
        2 => {
            let f = std::fs::read_to_string(units_path()).is_ok_and(|t| t.trim() == "fahrenheit");
            FAHRENHEIT.store(f as u8, Relaxed);
            f
        }
        v => v == 1,
    }
}

fn set_fahrenheit(on: bool) {
    FAHRENHEIT.store(on as u8, std::sync::atomic::Ordering::Relaxed);
    let _ = std::fs::create_dir_all(crate::savegame::SAVE_DIR);
    let _ = std::fs::write(units_path(), if on { "fahrenheit
" } else { "celsius
" });
}

/// A temperature in the player's units, to the degree.
pub fn degrees(celsius: f32) -> String {
    let (v, u) = if fahrenheit() { (celsius * 9.0 / 5.0 + 32.0, "F") } else { (celsius, "C") };
    format!("{:.0}°{u}", v.round() + 0.0)
}

/// Short names for the water engines in Settings.
fn water_label(kind: sim::water::EngineKind) -> &'static str {
    match kind {
        sim::water::EngineKind::Routed => "Classic",
        sim::water::EngineKind::Subgrid => "Detailed",
        sim::water::EngineKind::Automaton => "Cellular",
        sim::water::EngineKind::Equilibrium => "Steady",
    }
}
