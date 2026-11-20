#![allow(deprecated)]
use crate::camera::OrbitCamera;
use crate::cli::Cli;
use crate::flow::{Flow, Selection};
use crate::hud::{self, MenuOpen, ACCENT, FAINT, GOOD, INK, MUTED, PANEL, TIMBER};
use crate::icons::{icon_node, Icon, Icons};
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
    back_to: Screen,
    was_paused: bool,
    pub map: MapChoice,
    pub ui_scale: f32,
    pub auto_pause: bool,
    seen_urgent: Vec<String>,
    confirm: Option<MenuAction>,
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

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<Cli>().clone();
        let forced = std::env::var("BC_MENU").ok();
        let benchmarking = std::env::var_os("BC_PERF").is_some()
            && (std::env::var_os("BC_SPEED").is_some() || std::env::var_os("BC_REPORT_BENCH").is_some());
        let scripted = cli.colony_script.is_some() || cli.sandbox
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
            auto_pause: !benchmarking,
            seen_urgent: Vec::new(),
            confirm: None,
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
            let busy = hud.as_ref().is_some_and(|h| h.tool != hud::Tool::None || h.open_cat.is_some() || h.colony.is_some() || h.search.is_some() || h.order_mode.is_some() || h.help);
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
        match a {
            MenuAction::NewGame | MenuAction::Restart => {
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
                } else if sim.0.sandbox {
                    crate::startflow::begin_sandbox(&sim.0, &mut flow, &mut clock, &mut orbit, &mut sel);
                } else if sim.0.colony.is_founded() {
                    crate::startflow::begin(&sim.0, &mut flow, &mut clock, &mut orbit, &mut sel);
                } else {
                    let camp = planned.0.as_ref().map(|d| d.camp);
                    crate::startflow::begin_scouting(&sim.0, &mut flow, &mut clock, &mut orbit, &mut sel, camp);
                }
                clock.paused = true;
                menu.seen_urgent.clear();
                menu.screen = None;
            }
            MenuAction::MapPrev | MenuAction::MapNext => {
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
            MenuAction::Ask(_) => menu.confirm = Some(a),
            MenuAction::Confirm(_) => {}
        }
    }
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

fn auto_pause(time: Res<Time>, mut next: Local<f32>, sim: Res<Sim>, mut menu: ResMut<Menu>, mut clock: ResMut<SimClock>, hud_state: Option<ResMut<hud::HudState>>) {
    if sim.0.colony.is_collapsed() && menu.screen.is_none() {
        clock.paused = true;
        menu.was_paused = true;
        menu.screen = Some(Screen::Pause);
    }
    let now = time.elapsed_secs();
    if now < *next || menu.screen.is_some() {
        return;
    }
    *next = now + 1.0;
    let urgent: Vec<String> = sim::hud::problems(&sim.0).into_iter().filter(|p| p.severity == Severity::Urgent).map(|p| format!("{:?}{:?}", p.kind, p.subject)).collect();
    let new: Vec<String> = urgent.iter().filter(|t| !menu.seen_urgent.contains(t)).cloned().collect();
    menu.seen_urgent = urgent;
    if menu.auto_pause && !clock.paused {
        if let Some(t) = new.first() {
            clock.paused = true;
            if let Some(mut h) = hud_state {
                let _ = &t;
                h.notify_paused(now);
            }
        }
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
        r.spawn((card, ScrollPosition::default(), bevy::ui::RelativeCursorPosition::default(), Interaction::default(), hud::Blocks, FocusPolicy::Block, BackgroundColor(PANEL), border)).with_children(|p| match screen {
            Screen::Start => start_screen(p, &menu, &current.0),
            Screen::Pause => pause_screen(p, &menu, &icons, &sim.0, &clock),
            Screen::Settings => settings_screen(p, &menu, &icons),
        });
    });
}

fn start_screen(p: &mut ChildSpawnerCommands, menu: &Menu, current: &MapChoice) {
    let _ = current;
    hud::bold(p, "Beaver Colony", 30.0, INK);
    p.spawn(Node { height: Val::Px(6.0), ..default() });
    wide(p, "Play", MenuAction::NewGame, ACCENT);
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| {
        button(r, "<", MenuAction::MapPrev, FAINT, false);
        r.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, flex_grow: 1.0, ..default() }).with_children(|c| {
            let label = menu.map.label();
            hud::bold(c, label.split(',').next().unwrap_or("").to_string(), 14.0, INK);
            match &menu.map {
                MapChoice::Real(id) => {
                    if let Some(m) = sim::maps::all().into_iter().find(|m| &m.id == id) {
                        hud::text(c, format!("{:.1} km", m.km()), 11.0, MUTED);
                    }
                }
                MapChoice::Generated(seed) => {
                    c.spawn(hud::row(6.0)).with_children(|r| {
                        button(r, "<", MenuAction::SeedPrev, FAINT, false);
                        hud::bold(r, format!("{seed}"), 12.0, INK);
                        button(r, ">", MenuAction::SeedNext, FAINT, false);
                    });
                }
            }
        });
        button(r, ">", MenuAction::MapNext, FAINT, false);
    });
    wide(p, "Settings", MenuAction::OpenSettings, FAINT);
    wide(p, "Quit", MenuAction::Quit, FAINT);
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
        hud::text(p, "Start again with shelter and a reliable food supply before expanding.", 13.0, INK);
    } else {
        wide(p, "Resume", MenuAction::Resume, ACCENT);
    }
    wide(p, "Settings", MenuAction::OpenSettings, FAINT);
    wide(p, "Restart", MenuAction::Ask(Ask::Restart), FAINT);
    wide(p, "Menu", MenuAction::Ask(Ask::ToTitle), FAINT);
    wide(p, "Quit", MenuAction::Ask(Ask::Quit), FAINT);
}

fn settings_screen(p: &mut ChildSpawnerCommands, menu: &Menu, icons: &Icons) {
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| {
        hud::bold(r, "Settings", 22.0, INK);
        icon_button(r, icons, Icon::Back, MenuAction::Back, FAINT);
    });
    hud::bold(p, "UI size", 12.0, MUTED);
    p.spawn(hud::row(6.0)).with_children(|r| {
        for s in SCALES {
            let label = format!("{:.0}%", s * 100.0);
            button(r, &label, MenuAction::Scale(s), FAINT, (menu.ui_scale - s).abs() < 0.01);
        }
    });
    hud::bold(p, "Alerts", 12.0, MUTED);
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..hud::row(8.0) }).with_children(|r| {
        hud::text(r, "Auto-pause", 13.0, INK);
        button(r, if menu.auto_pause { "On" } else { "Off" }, MenuAction::ToggleAutoPause, if menu.auto_pause { GOOD.with_alpha(0.25) } else { FAINT }, menu.auto_pause);
    });
    hud::bold(p, "Keys", 12.0, MUTED);
    hud::text(p, "Scroll to see all controls", 11.0, MUTED);
    for (k, v) in [
        ("Select", "LMB"),
        ("Move", "RMB / M then click"),
        ("Gather / work", "G / R then click"),
        ("Cancel", "RMB / Esc"),
        ("Orbit", "Middle drag"),
        ("Pan", "WASD"),
        ("Rotate", "Q E"),
        ("Zoom", "Wheel"),
        ("Build", "Z X C V B N"),
        ("Crest", "[ ]"),
        ("Views", "F1–F5 / Alt"),
        ("Tree canopies", "F8"),
        ("Colony", "Tab / J / L"),
        ("Idle", "I"),
        ("Help", "H"),
        ("Search", "/"),
        ("Undo", "Ctrl+Z"),
        ("Pause", "Space"),
        ("Speed", "1 2 3"),
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
