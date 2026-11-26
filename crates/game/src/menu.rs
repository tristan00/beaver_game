#![allow(deprecated)]
use crate::camera::OrbitCamera;
use crate::cli::Cli;
use crate::flow::{Flow, Selection};
use crate::hud::{self, MenuOpen, ACCENT, BAD, BARK, FAINT, GOOD, INK, MUTED, PANEL, WARN};
use crate::icons::{icon_node, Icon, Icons};
use crate::sim_driver::SimClock;
use crate::state::{CurrentMap, MapChoice, PlannedDam, Sim};
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen { Start, Pause, Settings, Load }

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
    saves: Vec<std::path::PathBuf>,
    pub saved: Option<SaveNote>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SaveNote { Saving, Saved, Failed(String) }

type LoadedValley = Result<(sim::SimState, Option<sim::starts::DamSite>), String>;

#[derive(Resource)]
struct PendingValley {
    map: MapChoice,
    preview: bool,
    result: std::sync::Mutex<std::sync::mpsc::Receiver<LoadedValley>>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum MenuAction {
    NewGame, OpenLoad, LoadSave(usize), Save, MapPrev, MapNext, OpenSettings, Back,
    Resume, Restart, ToTitle, Quit, Scale(f32), Water(sim::water::EngineKind),
    Fahrenheit(bool), DevSpeed(bool), Volume(crate::audio::Channel, f32), Ask(Ask), Confirm(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ask { Restart, ToTitle, Quit }

#[derive(Component)]
struct MenuRoot;

pub struct MenuPlugin;

const SCALES: [f32; 4] = [0.85, 1.0, 1.15, 1.3];
const LEVELS: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<Cli>().clone();
        let forced = std::env::var("BC_MENU").ok();
        let benchmarking = std::env::var_os("BC_PERF").is_some() && (std::env::var_os("BC_SPEED").is_some() || std::env::var_os("BC_REPORT_BENCH").is_some());
        let scripted = cli.colony_script.is_some() || cli.sandbox || cli.load.is_some() || (benchmarking && std::env::var_os("BC_SETTLE").is_some()) || (cli.screenshot.is_some() && forced.is_none());
        let first = match forced.as_deref() { Some("pause") => Some(Screen::Pause), Some("settings") => Some(Screen::Settings), _ if scripted => None, _ => Some(Screen::Start) };
        app.insert_resource(Menu {
            screen: first, back_to: if first == Some(Screen::Pause) { Screen::Pause } else { Screen::Start }, was_paused: true,
            map: app.world().resource::<CurrentMap>().0.clone(), ui_scale: 1.0, water: sim::water::EngineKind::from_env(),
            confirm: None, loading: false, load_error: None, saves: Vec::new(), saved: None,
        })
        .init_resource::<MenuOpen>()
        .add_systems(Startup, spawn_root)
        .add_systems(Update, (menu_keys, load_keys, menu_actions, finish_loading, hold_still, pause_on_collapse, title_orbit, rebuild_menu).chain().before(hud::HudSet));
    }
}

fn spawn_root(mut commands: Commands) {
    commands.spawn((
        MenuRoot,
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
        BackgroundColor(Color::srgba(0.05, 0.04, 0.03, 0.45)), GlobalZIndex(50), Interaction::default(), FocusPolicy::Block, Visibility::Hidden,
    ));
}

fn menu_keys(
    keys: Res<ButtonInput<KeyCode>>, mut menu: ResMut<Menu>, hud: Option<Res<hud::HudState>>,
    lens: Option<Res<hud::MapLens>>, sel: Res<Selection>, flow: Res<Flow>, mut clock: ResMut<SimClock>,
) {
    if menu.loading || !keys.just_pressed(KeyCode::Escape) { return; }
    match menu.screen {
        Some(Screen::Pause) => { menu.screen = None; clock.paused = menu.was_paused; }
        Some(Screen::Settings | Screen::Load) => menu.screen = Some(menu.back_to),
        Some(Screen::Start) => {}
        None => if !hud.as_ref().is_some_and(|h| h.is_busy(*sel, lens.as_deref().copied().unwrap_or(hud::MapLens::World), flow.welcome)) {
            menu.saved = None;
            menu.was_paused = clock.paused;
            clock.paused = true;
            menu.screen = Some(Screen::Pause);
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn menu_actions(
    mut commands: Commands, q: Query<(&Interaction, &MenuAction), Changed<Interaction>>, mut menu: ResMut<Menu>,
    mut sim: ResMut<Sim>, current: Res<CurrentMap>, planned: Res<PlannedDam>, mut flow: ResMut<Flow>,
    mut clock: ResMut<SimClock>, mut orbit: ResMut<OrbitCamera>, mut sel: ResMut<Selection>,
    mut exit: MessageWriter<AppExit>, mut save: ResMut<crate::savegame::SaveRequest>,
    sim_thread: Option<Res<crate::sim_driver::SimulationThread>>,
) {
    if menu.loading { return; }
    let pressed: Vec<MenuAction> = q.iter().filter(|(i, _)| **i == Interaction::Pressed).map(|(_, &a)| match a {
        MenuAction::Confirm(yes) => match (yes, menu.confirm.take()) {
            (true, Some(MenuAction::Ask(Ask::Restart))) => MenuAction::Restart,
            (true, Some(MenuAction::Ask(Ask::ToTitle))) => MenuAction::ToTitle,
            (true, Some(MenuAction::Ask(Ask::Quit))) => MenuAction::Quit,
            _ => MenuAction::Confirm(false),
        },
        other => other,
    }).collect();
    for a in pressed {
        if sim::trace::enabled() { sim::trace::event(format!("MENU_ACTION {a:?}")); }
        match a {
            MenuAction::NewGame | MenuAction::Restart => {
                if current.0 == menu.map && sim.0.time_days == 0.0 && a == MenuAction::NewGame {
                    if let Err(error) = crate::startflow::begin_play(&mut sim.0, &mut flow, &mut clock, &mut orbit, &mut sel, planned.0.as_ref().map(|d| d.camp)) {
                        menu.load_error = Some(error);
                        continue;
                    }
                    (clock.paused, menu.load_error, menu.screen) = (true, None, None);
                } else {
                    let map = if a == MenuAction::Restart { current.0.clone() } else { menu.map.clone() };
                    spawn_valley_build(&mut commands, map, sim.0.sandbox, false);
                    (menu.loading, menu.load_error, clock.paused) = (true, None, true);
                    continue;
                }
            }
            MenuAction::OpenLoad => {
                menu.back_to = menu.screen.unwrap_or(Screen::Pause);
                menu.saves = crate::savegame::saves().into_iter().map(|s| s.0).collect();
                (menu.load_error, menu.screen) = (None, Some(Screen::Load));
            }
            MenuAction::LoadSave(i) => { if let Some(path) = menu.saves.get(i).cloned() { start_load(&mut commands, &mut menu, &mut clock, Some(path)); } continue; }
            MenuAction::Save => { save.0 = true; menu.saved = Some(SaveNote::Saving); continue; }
            MenuAction::MapPrev | MenuAction::MapNext => {
                let options = MapChoice::menu();
                let (i, n) = (options.iter().position(|o| *o == menu.map).unwrap_or(0), options.len());
                menu.map = options[if a == MenuAction::MapNext { (i + 1) % n } else { (i + n - 1) % n }].clone();
                if menu.screen == Some(Screen::Start) && menu.map != current.0 { spawn_valley_build(&mut commands, menu.map.clone(), sim.0.sandbox, true); }
            }
            MenuAction::OpenSettings => { menu.back_to = menu.screen.unwrap_or(Screen::Pause); menu.screen = Some(Screen::Settings); }
            MenuAction::Back => menu.screen = Some(menu.back_to),
            MenuAction::Resume => { menu.screen = None; clock.paused = menu.was_paused; }
            MenuAction::ToTitle => { menu.map = current.0.clone(); menu.screen = Some(Screen::Start); }
            MenuAction::Quit => { exit.write(AppExit::Success); }
            MenuAction::Scale(s) => menu.ui_scale = s,
            MenuAction::Fahrenheit(on) => set_fahrenheit(on),
            MenuAction::DevSpeed(on) => crate::sim_driver::set_dev_speed(on),
            MenuAction::Volume(ch, v) => commands.queue(move |w: &mut World| { if let Some(mut vols) = w.get_resource_mut::<crate::audio::AudioVolumes>() { vols.set(ch, v); } }),
            MenuAction::Water(kind) => {
                menu.water = kind;
                sim::water::set_default_engine(kind);
                sim.0.water.set_engine(kind);
                if let Some(t) = &sim_thread { t.set_water_engine(kind); }
            }
            MenuAction::Ask(_) => menu.confirm = Some(a),
            MenuAction::Confirm(_) => {}
        }
        if menu.screen.is_none() {
            commands.queue(|w: &mut World| { if let Some(mut hud) = w.get_resource_mut::<hud::HudState>() { hud.swallow_click(); } });
        }
    }
}

fn spawn_valley_build(commands: &mut Commands, map: MapChoice, sandbox: bool, preview: bool) {
    let (send, receive) = std::sync::mpsc::channel();
    let build_map = map.clone();
    std::thread::spawn(move || {
        let _ = send.send(if sandbox { build_map.sandbox().map(|s| (s, None)) } else { build_map.build().map(|(s, d)| (s, Some(d))) });
    });
    commands.insert_resource(PendingValley { map, preview, result: std::sync::Mutex::new(receive) });
}

fn start_load(commands: &mut Commands, menu: &mut Menu, clock: &mut SimClock, path: Option<std::path::PathBuf>) {
    let Some(path) = path.or_else(crate::savegame::latest_save) else { menu.load_error = Some("No saved game".into()); return; };
    let map = match sim::save::peek(&path) { Ok(h) => crate::savegame::map_from(&h.map), Err(e) => { menu.load_error = Some(e); return; } };
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(sim::save::load(&path).map(|(h, s)| {
            sim::trace::event(format!("LOADED path={} day={:.2} map={} saved_by={}", path.display(), h.day, h.map, h.version));
            (s, None)
        }));
    });
    commands.insert_resource(PendingValley { map, preview: false, result: std::sync::Mutex::new(receive) });
    (menu.loading, menu.load_error, clock.paused) = (true, None, true);
}

fn load_keys(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands, mut menu: ResMut<Menu>, mut clock: ResMut<SimClock>) {
    if (keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)) && keys.just_pressed(KeyCode::KeyL) && !menu.loading {
        start_load(&mut commands, &mut menu, &mut clock, None);
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_loading(
    pending: Option<Res<PendingValley>>, mut commands: Commands, mut menu: ResMut<Menu>, mut sim: ResMut<Sim>,
    mut planned: ResMut<PlannedDam>, mut current: ResMut<CurrentMap>, mut flow: ResMut<Flow>,
    mut clock: ResMut<SimClock>, mut orbit: ResMut<OrbitCamera>, mut sel: ResMut<Selection>,
) {
    let Some(pending) = pending else { return };
    let result = match pending.result.lock().unwrap().try_recv() {
        Ok(result) => result,
        Err(std::sync::mpsc::TryRecvError::Empty) => return,
        Err(_) => Err("The valley could not be prepared. Please try again.".into()),
    };
    if pending.preview {
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
    } else {
        menu.loading = false;
        match result.and_then(|(state, dam)| crate::startflow::install_game(state, dam, &mut sim, &mut planned, &mut flow, &mut clock, &mut orbit, &mut sel)) {
            Ok(()) => { current.0 = pending.map.clone(); (menu.screen, clock.paused) = (None, true); commands.queue(hud::reset_for_new_game); }
            Err(error) => menu.load_error = Some(error),
        }
    }
    commands.remove_resource::<PendingValley>();
}

fn hold_still(menu: Res<Menu>, mut open: ResMut<MenuOpen>, mut clock: ResMut<SimClock>) {
    let up = menu.screen.is_some();
    if open.0 != up { open.0 = up; }
    if up && !clock.paused { clock.paused = true; }
}

fn pause_on_collapse(sim: Res<Sim>, mut menu: ResMut<Menu>) {
    if sim.0.colony.is_collapsed() && menu.screen.is_none() { (menu.was_paused, menu.screen) = (true, Some(Screen::Pause)); }
}

fn title_orbit(time: Res<Time>, menu: Res<Menu>, mut orbit: ResMut<OrbitCamera>) {
    if menu.screen == Some(Screen::Start) {
        orbit.yaw += time.delta_secs() * 0.06;
        if orbit.distance < 60.0 { orbit.distance += (60.0 - orbit.distance) * (time.delta_secs() * 0.8).min(1.0); }
    }
}

fn btn_box(p: &mut ChildSpawnerCommands, action: MenuAction, node: Node, fill: Color, on: bool, body: impl FnOnce(&mut ChildSpawnerCommands)) {
    p.spawn((
        Interaction::default(), FocusPolicy::Block, action,
        Node { border: UiRect::all(Val::Px(if on { 2.0 } else { 1.0 })), border_radius: BorderRadius::all(Val::Px(5.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..node },
        BackgroundColor(fill), BorderColor::all(if on { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.14) }),
    )).with_children(body);
}

fn btn_ink(fill: Color) -> Color { if fill == ACCENT { Color::srgb(0.13, 0.09, 0.05) } else { INK } }

fn button(p: &mut ChildSpawnerCommands, label: &str, action: MenuAction, fill: Color, on: bool) {
    btn_box(p, action, Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)), ..default() }, fill, on, |b| hud::bold(b, label, 14.0, btn_ink(fill)));
}

fn icon_button(p: &mut ChildSpawnerCommands, icons: &Icons, icon: Icon, action: MenuAction, fill: Color) {
    btn_box(p, action, Node { padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)), ..default() }, fill, false, |b| { b.spawn(icon_node(icons, icon, 18.0, btn_ink(fill))); });
}

fn wide(p: &mut ChildSpawnerCommands, label: &str, action: MenuAction, fill: Color) {
    p.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|c| button(c, label, action, fill, false));
}

#[allow(clippy::too_many_arguments)]
fn rebuild_menu(
    mut commands: Commands, menu: Res<Menu>, sim: Res<Sim>, icons: Res<Icons>, vols: Option<Res<crate::audio::AudioVolumes>>,
    mut root: Query<(Entity, &mut Visibility, &mut BackgroundColor), With<MenuRoot>>,
) {
    if !menu.is_changed() && !(menu.screen == Some(Screen::Settings) && vols.as_ref().is_some_and(|v| v.is_changed())) { return; }
    let Ok((e, mut vis, mut bg)) = root.single_mut() else { return };
    vis.set_if_neq(if menu.screen.is_some() { Visibility::Inherited } else { Visibility::Hidden });
    commands.entity(e).despawn_related::<Children>();
    let Some(screen) = menu.screen else { return };
    bg.0 = Color::srgba(0.05, 0.04, 0.03, if screen == Screen::Start { 0.25 } else { 0.55 });
    let shell = Node { width: Val::Px(440.0), max_height: Val::Percent(92.0), overflow: Overflow::clip(), flex_direction: FlexDirection::Column, border: UiRect { top: Val::Px(3.0), ..UiRect::all(Val::Px(1.0)) }, border_radius: BorderRadius::all(Val::Px(9.0)), ..default() };
    let viewport = Node { width: Val::Percent(100.0), flex_grow: 1.0, min_height: Val::Px(0.0), overflow: Overflow::scroll_y(), flex_direction: FlexDirection::Column, row_gap: Val::Px(10.0), padding: UiRect { right: Val::Px(26.0), ..UiRect::all(Val::Px(22.0)) }, ..default() };
    let border = BorderColor { top: BARK, ..BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.08)) };
    commands.entity(e).with_children(|r| {
        r.spawn((shell, Interaction::default(), hud::Blocks, FocusPolicy::Block, BackgroundColor(PANEL), border)).with_children(|card| {
            let vp = card.spawn((viewport, hud::scrollbar::ScrollViewport, ScrollPosition::default(), bevy::ui::RelativeCursorPosition::default())).with_children(|p| {
                if menu.loading { hud::bold(p, "Preparing your valley...", 20.0, INK); hud::text(p, menu.map.label(), 13.0, MUTED); return; }
                if let Some(error) = &menu.load_error { hud::text(p, error, 13.0, BAD); }
                match screen {
                    Screen::Start => start_screen(p, &menu, &icons),
                    Screen::Pause => pause_screen(p, &menu, &icons, &sim.0),
                    Screen::Settings => settings_screen(p, &menu, &icons, vols.as_deref()),
                    Screen::Load => load_screen(p, &menu),
                }
            }).id();
            hud::scrollbar::spawn_scrollbar(card, vp, 12.0, 12.0, 6.0);
        });
    });
}

fn start_screen(p: &mut ChildSpawnerCommands, menu: &Menu, icons: &Icons) {
    hud::bold(p, "Beaver Colony", 30.0, INK);
    p.spawn(Node { height: Val::Px(6.0), ..default() });
    wide(p, "Play", MenuAction::NewGame, ACCENT);
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| {
        button(r, "<", MenuAction::MapPrev, FAINT, false);
        r.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, flex_grow: 1.0, ..default() }).with_children(|c| {
            hud::bold(c, menu.map.label().split(',').next().unwrap_or(""), 14.0, INK);
            if let MapChoice::Real(id) = &menu.map {
                if let Some(m) = sim::maps::all().into_iter().find(|m| &m.id == id) { hud::text(c, m.place, 11.0, MUTED); }
            }
        });
        button(r, ">", MenuAction::MapNext, FAINT, false);
    });
    if let Some(d) = menu.map.difficulty() {
        let col = match d { crate::state::Difficulty::Easy => GOOD, crate::state::Difficulty::Medium => WARN, crate::state::Difficulty::Hard => BAD };
        p.spawn((Node { justify_content: JustifyContent::Center, ..hud::row(6.0) }, hud::hover_tip("Difficulty"))).with_children(|r| {
            for i in 0..3 { r.spawn((Node { width: Val::Px(16.0), height: Val::Px(7.0), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() }, BackgroundColor(if i < d.level() { col } else { Color::srgba(1.0, 1.0, 1.0, 0.12) }))); }
            hud::bold(r, d.label(), 13.0, col);
        });
    }
    if let MapChoice::Real(id) = &menu.map {
        if let Some(s) = sim::maps::stats(id) {
            let share = |v: f32| if v > 0.0 && v < 0.01 { "<1%".into() } else { format!("{:.0}%", v * 100.0) };
            let mut stats = vec![(Icon::Upstream, s.streams.to_string(), "Streams flowing in"), (Icon::ViewForest, share(s.forest), "Forest"), (Icon::ViewWater, share(s.water), "Open water at the start"), (Icon::ViewElevation, format!("{:.0} m", s.relief_m), "Lowest to highest ground")];
            if let Some(c) = &s.climate { stats.extend([(Icon::Winter, format!("{:.0} mo", c.ice_months()), "Months under ice"), (Icon::Gauge, format!("{:.0} mm", c.runoff_year_mm()), "Water the streams carry in a year")]); }
            p.spawn(Node { flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, column_gap: Val::Px(12.0), row_gap: Val::Px(4.0), ..default() }).with_children(|r| {
                for (icon, value, name) in stats { r.spawn((hud::row(3.0), hud::hover_tip(name))).with_children(|c| { c.spawn(icon_node(icons, icon, 14.0, MUTED)); hud::text(c, value, 11.0, MUTED); }); }
            });
        }
    }
    if crate::savegame::latest_save().is_some() { wide(p, "Load", MenuAction::OpenLoad, FAINT); }
    wide(p, "Settings", MenuAction::OpenSettings, FAINT);
    wide(p, "Quit", MenuAction::Quit, FAINT);
}

fn load_screen(p: &mut ChildSpawnerCommands, menu: &Menu) {
    hud::bold(p, "Load", 20.0, INK);
    if menu.saves.is_empty() { hud::text(p, "No saved games", 13.0, MUTED); }
    for (i, path) in menu.saves.iter().enumerate() {
        let (title, detail) = match sim::save::peek(path) {
            Ok(h) => (crate::savegame::map_from(&h.map).label().split(',').next().unwrap_or("").to_string(), format!("Day {:.0} · {} beavers", h.day, h.population)),
            Err(e) => (path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), e),
        };
        btn_box(p, MenuAction::LoadSave(i), Node { flex_direction: FlexDirection::Column, align_items: AlignItems::FlexStart, padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)), ..default() }, FAINT, false, |b| {
            hud::bold(b, title, 14.0, INK);
            hud::text(b, detail, 11.0, MUTED);
        });
    }
    wide(p, "Back", MenuAction::Back, FAINT);
}

fn pause_screen(p: &mut ChildSpawnerCommands, menu: &Menu, icons: &Icons, s: &sim::SimState) {
    if menu.confirm.is_some() {
        p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| {
            hud::bold(r, "Lose colony?", 14.0, INK);
            r.spawn(hud::row(8.0)).with_children(|b| { icon_button(b, icons, Icon::Cross, MenuAction::Confirm(false), FAINT); icon_button(b, icons, Icon::Check, MenuAction::Confirm(true), ACCENT); });
        });
        return;
    }
    if s.colony.is_collapsed() {
        hud::bold(p, "The colony has been lost", 20.0, INK);
        hud::text(p, format!("Survived {:.0} days. {} beavers died.", s.time_days, s.colony.stats.deaths), 13.0, MUTED);
    } else {
        wide(p, "Resume", MenuAction::Resume, ACCENT);
        let label = match &menu.saved { Some(SaveNote::Saving) => "Saving…", Some(SaveNote::Saved) => "Saved", _ => "Save" };
        wide(p, label, MenuAction::Save, if menu.saved == Some(SaveNote::Saved) { GOOD.with_alpha(0.35) } else { FAINT });
        if let Some(SaveNote::Failed(e)) = &menu.saved { hud::text(p, e.clone(), 12.0, BAD); }
    }
    if crate::savegame::latest_save().is_some() { wide(p, "Load", MenuAction::OpenLoad, FAINT); }
    wide(p, "Settings", MenuAction::OpenSettings, FAINT);
    wide(p, "Restart", MenuAction::Ask(Ask::Restart), FAINT);
    wide(p, "Menu", MenuAction::Ask(Ask::ToTitle), FAINT);
    wide(p, "Quit", MenuAction::Ask(Ask::Quit), FAINT);
}

fn settings_screen(p: &mut ChildSpawnerCommands, menu: &Menu, icons: &Icons, vols: Option<&crate::audio::AudioVolumes>) {
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| {
        hud::bold(r, "Settings", 20.0, INK);
        icon_button(r, icons, Icon::Back, MenuAction::Back, FAINT);
    });
    if let Some(vols) = vols {
        hud::bold(p, "Sound", 12.0, MUTED);
        for (ch, name) in [(crate::audio::Channel::Master, "Master"), (crate::audio::Channel::Effects, "Sound effects"), (crate::audio::Channel::Ambience, "Ambience")] {
            let now = vols.level(ch);
            let nearest = LEVELS.iter().copied().fold(LEVELS[0], |a, l| if (l - now).abs() < (a - now).abs() { l } else { a });
            p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(6.0) }).with_children(|r| {
                hud::text(r, name, 12.0, INK);
                r.spawn(hud::row(4.0)).with_children(|b| {
                    for l in LEVELS {
                        let label = if l == 0.0 { "Off".into() } else { format!("{:.0}", l * 100.0) };
                        btn_box(b, MenuAction::Volume(ch, l), Node { width: Val::Px(44.0), padding: UiRect::vertical(Val::Px(5.0)), ..default() }, FAINT, l == nearest, |x| hud::bold(x, label, 12.0, INK));
                    }
                });
            });
        }
    }
    hud::bold(p, "UI size", 12.0, MUTED);
    p.spawn(hud::row(6.0)).with_children(|r| for s in SCALES { button(r, &format!("{:.0}%", s * 100.0), MenuAction::Scale(s), FAINT, (menu.ui_scale - s).abs() < 0.01); });
    hud::bold(p, "Temperature", 12.0, MUTED);
    p.spawn(hud::row(6.0)).with_children(|r| { let f = fahrenheit(); button(r, "°C", MenuAction::Fahrenheit(false), FAINT, !f); button(r, "°F", MenuAction::Fahrenheit(true), FAINT, f); });
    hud::bold(p, "Dev speed", 12.0, MUTED);
    p.spawn(hud::row(6.0)).with_children(|r| { let on = crate::sim_driver::dev_speed(); button(r, "Off", MenuAction::DevSpeed(false), FAINT, !on); button(r, "On", MenuAction::DevSpeed(true), FAINT, on); });
    hud::bold(p, "Water", 12.0, MUTED);
    p.spawn(hud::row(6.0)).with_children(|r| for kind in sim::water::EngineKind::ALL { button(r, water_label(kind), MenuAction::Water(kind), FAINT, menu.water == kind); });
    hud::bold(p, "Keys", 12.0, MUTED);
    for (k, v) in [
        ("Select", "LMB"), ("Order selected beavers", "RMB"), ("Step back", "RMB / Esc"), ("Pan", "WASD / arrows"),
        ("Rotate", "Q E / middle drag"), ("Zoom", "Wheel"), ("Dam, lodge, stick pile", "Z X C"), ("Gather, cut trees", "G T"),
        ("Build the dam", "Enter"), ("Undo", "Ctrl+Z"), ("Water, forest, height", "F1 F2 F3"), ("Peek water", "Alt"),
        ("Tree canopies", "F8"), ("Colony", "Tab"), ("Territory", "L"), ("Next free beaver", "I"),
        ("Pause, speed", if crate::sim_driver::dev_speed() { "Space, 1 2 3 4" } else { "Space, 1 2 3" }),
        ("Volume: master, effects, ambience", "- =  ; '  , ."), ("Mute", "M"), ("Menu", "Esc"),
    ] {
        p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| { hud::text(r, k, 12.0, MUTED); hud::bold(r, v, 12.0, INK); });
    }
    hud::bold(p, "About", 12.0, MUTED);
    hud::text(p, "Elevation: AWS Open Data Terrain Tiles (SRTM, USGS 3DEP, NRCan CDEM, EU-DEM)", 11.0, MUTED);
}

static FAHRENHEIT: AtomicU8 = AtomicU8::new(2);
fn units_path() -> std::path::PathBuf { crate::savegame::save_dir().join("units.txt") }

pub fn fahrenheit() -> bool {
    match FAHRENHEIT.load(Relaxed) {
        2 => { let f = std::fs::read_to_string(units_path()).is_ok_and(|t| t.trim() == "fahrenheit"); FAHRENHEIT.store(f as u8, Relaxed); f }
        v => v == 1,
    }
}

fn set_fahrenheit(on: bool) {
    FAHRENHEIT.store(on as u8, Relaxed);
    let _ = std::fs::create_dir_all(crate::savegame::save_dir());
    let _ = std::fs::write(units_path(), if on { "fahrenheit\n" } else { "celsius\n" });
}

pub fn degrees(celsius: f32) -> String {
    let (v, u) = if fahrenheit() { (celsius * 9.0 / 5.0 + 32.0, "F") } else { (celsius, "C") };
    format!("{:.0}°{u}", v.round() + 0.0)
}

fn water_label(kind: sim::water::EngineKind) -> &'static str {
    use sim::water::EngineKind::*;
    match kind { Routed => "Classic", Subgrid => "Detailed", Automaton => "Cellular", Equilibrium => "Steady" }
}
