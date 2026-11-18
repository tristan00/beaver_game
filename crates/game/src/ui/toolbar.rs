//! The toolbar (zone Z6, bottom centre) and the one menu that opens above it
//! (`docs/UI_SPEC.md` §1, §2). Build, Gather and Screens open their menu here; Jobs opens the
//! centred panel in `jobs.rs`. B, G, H, J and X are the keys. Haul and Demolish are drawn dimmed
//! with the reason on hover until the sim can do them.
use super::buildmenu::spawn_build_menu;
use super::hud::{build_menu, gather_actions, toolbar, BuildMenu, ToolEntry};
use super::icons::IconAssets;
use super::kit::*;
use super::panels::ShellUi;
use super::screens::{nav_bar, Do, NavBar};
use super::shell::{HudTool, NavItem};
use crate::state::Sim;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Space under the menus: the toolbar's height plus its margin.
pub const BAR_RESERVE: f32 = 84.0;

#[derive(Component)]
pub struct ToolbarRoot;
#[derive(Component)]
pub struct MenuRoot;

pub struct ToolbarPlugin;

impl Plugin for ToolbarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_roots).add_systems(Update, (hotkeys, layout, refresh).chain());
    }
}

fn spawn_roots(mut commands: Commands) {
    commands.spawn((
        ToolbarRoot,
        Blocker,
        panel_with(Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(GRID),
            padding: UiRect::all(px(GRID)),
            display: Display::None,
            ..default()
        }),
        GlobalZIndex(10),
    ));
    commands.spawn((
        MenuRoot,
        Blocker,
        panel(Node { position_type: PositionType::Absolute, display: Display::None, max_width: px(760), ..default() }),
        GlobalZIndex(11),
    ));
}

/// B, G, H, J, X open the matching tool; the toolbar button does the same.
fn hotkeys(keys: Res<ButtonInput<KeyCode>>, mut ui: ResMut<ShellUi>) {
    for (k, t) in [(KeyCode::KeyB, HudTool::Build), (KeyCode::KeyG, HudTool::Gather), (KeyCode::KeyH, HudTool::Haul), (KeyCode::KeyJ, HudTool::Jobs), (KeyCode::KeyX, HudTool::Demolish)] {
        if keys.just_pressed(k) {
            ui.shell.toggle_tool(t);
            ui.status.clear();
        }
    }
}

#[allow(clippy::type_complexity)]
fn layout(
    windows: Query<&Window, With<PrimaryWindow>>,
    sim: Res<Sim>,
    mut bar: Query<(&mut Node, &ComputedNode), (With<ToolbarRoot>, Without<MenuRoot>)>,
    mut menu: Query<(&mut Node, &ComputedNode), (With<MenuRoot>, Without<ToolbarRoot>)>,
) {
    let Ok(w) = windows.single() else { return };
    let founded = sim.0.colony.is_founded();
    for (mut n, c) in &mut bar {
        let width = c.size.x * c.inverse_scale_factor;
        n.display = if founded { Display::Flex } else { Display::None };
        n.left = px(((w.width() - width) / 2.0).max(8.0));
        n.bottom = px(12);
    }
    for (mut n, c) in &mut menu {
        let width = c.size.x * c.inverse_scale_factor;
        n.left = px(((w.width() - width) / 2.0).max(8.0));
        n.bottom = px(BAR_RESERVE);
    }
}

/// What the popup above the toolbar shows.
#[derive(Clone, PartialEq)]
enum Popup {
    None,
    Build(BuildMenu),
    Gather,
    Screens(NavBar),
    /// An area order waits for the drag, or the work-area editor is open.
    Hint(&'static str),
}

#[derive(Clone, PartialEq)]
struct Shown {
    bar: Vec<ToolEntry>,
    popup: Popup,
    status: String,
}

type Key = (Option<HudTool>, Option<crate::build_logic::Group>, Option<sim::buildings::BuildingKind>, bool);

#[allow(clippy::too_many_arguments)]
fn refresh(
    time: Res<Time>,
    sim: Res<Sim>,
    ui: Res<ShellUi>,
    tool: Res<crate::build::BuildUi>,
    icons: Res<IconAssets>,
    bar_root: Query<Entity, With<ToolbarRoot>>,
    menu_root: Query<Entity, With<MenuRoot>>,
    mut menu_node: Query<&mut Node, With<MenuRoot>>,
    mut commands: Commands,
    mut last: Local<Option<Shown>>,
    mut wait: Local<f32>,
    mut key: Local<Key>,
) {
    if !sim.0.colony.is_founded() {
        *last = None;
        return;
    }
    let now_key: Key = (ui.shell.tool, ui.shell.build_tab, tool.tool, ui.shell.armed.is_some() || ui.shell.editor.is_some());
    *wait -= time.delta_secs();
    if *wait > 0.0 && last.is_some() && *key == now_key {
        return;
    }
    *wait = 0.25;
    *key = now_key;
    let popup = if ui.shell.armed.is_some() {
        Popup::Hint("Drag a rectangle on the map to assign. Esc cancels.")
    } else if ui.shell.editor.is_some() {
        Popup::Hint("Editing work areas: drag the inside to move, the border to resize. Esc closes.")
    } else {
        match ui.shell.tool {
            Some(HudTool::Build) => Popup::Build(build_menu(&sim.0, ui.shell.build_tab, tool.tool)),
            Some(HudTool::Gather) => Popup::Gather,
            Some(HudTool::Screens) => Popup::Screens(nav_bar(&sim.0, ui.shell.open)),
            _ => Popup::None,
        }
    };
    let now = Shown { bar: toolbar(&sim.0, &ui.shell), popup, status: ui.status.clone() };
    if last.as_ref() == Some(&now) {
        return;
    }
    if let Ok(e) = bar_root.single() {
        commands.entity(e).despawn_children();
        commands.entity(e).with_children(|p| spawn_bar(p, &now.bar, &icons));
    }
    if let (Ok(e), Ok(mut n)) = (menu_root.single(), menu_node.single_mut()) {
        n.display = if now.popup == Popup::None { Display::None } else { Display::Flex };
        commands.entity(e).despawn_children();
        commands.entity(e).with_children(|p| match &now.popup {
            Popup::None => {}
            Popup::Build(m) => spawn_build_menu(p, m, &icons, &now.status),
            Popup::Gather => spawn_gather_menu(p, &icons),
            Popup::Screens(nav) => spawn_screens_list(p, nav, &icons),
            Popup::Hint(t) => label(p, *t, T_BODY, PITH),
        });
    }
    *last = Some(now);
}

fn badge(b: &mut Kids, n: &super::explain::Explained) {
    b.spawn((
        Node { position_type: PositionType::Absolute, top: px(-6), right: px(-6), min_width: px(22), height: px(22), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
        BackgroundColor(AMBER),
        Tip::of(n),
    ))
    .with_children(|c| label_bold(c, n.show(), T_MIN, RAIL));
}

fn spawn_bar(p: &mut Kids, entries: &[ToolEntry], icons: &IconAssets) {
    for e in entries {
        let t = e.tool;
        let label_text = match t.key() {
            Some(k) => format!("{}  {k}", t.label()),
            None => t.label().to_string(),
        };
        let kind = if e.active { ButtonKind::Active } else { ButtonKind::Normal };
        let tip = match e.off {
            Some(why) => format!("{}: {why}", t.label()),
            None if e.active => format!("{} (click to close)", t.label()),
            None => t.label().to_string(),
        };
        let id = spawn_button(p, icons, KitButton::new(t.icon(), label_text, Do::Tool(t)).kind(kind).layer(5).tip(tip));
        if e.off.is_some() {
            // dimmed: the shell ignores the click, the reason is the hover text
            p.commands().entity(id).with_children(|b| {
                b.spawn((Node { position_type: PositionType::Absolute, left: px(0), right: px(0), top: px(0), bottom: px(0), ..default() }, BackgroundColor(Color::srgba(0.16, 0.12, 0.10, 0.55))));
            });
        }
        if let Some(n) = &e.badge {
            p.commands().entity(id).with_children(|b| badge(b, n));
        }
    }
}

fn spawn_gather_menu(p: &mut Kids, icons: &IconAssets) {
    label_bold(p, "Gather", T_TITLE, PITH);
    p.spawn(Node { column_gap: px(GRID), ..default() }).with_children(|r| {
        for a in gather_actions() {
            spawn_button(r, icons, KitButton::new(a.icon(), a.label(), Do::ModeAct(a)).layer(6).tip(a.hint()));
        }
    });
    label(p, "Then drag a rectangle on the map. Selected beavers go; with none selected, idle adults do.", T_MIN, DIM);
}

fn spawn_screens_list(p: &mut Kids, nav: &NavBar, icons: &IconAssets) {
    label_bold(p, "Screens", T_TITLE, PITH);
    p.spawn(Node { column_gap: px(GRID), ..default() }).with_children(|r| {
        for e in &nav.entries {
            let kind = if e.open { ButtonKind::Active } else { ButtonKind::Normal };
            let id = spawn_button(r, icons, KitButton::new(e.icon, e.label, Do::Nav(e.item)).kind(kind).layer(6).tip(e.label));
            if let (NavItem::Events, Some(n)) = (e.item, &e.badge) {
                r.commands().entity(id).with_children(|b| badge(b, n));
            }
        }
    });
}
