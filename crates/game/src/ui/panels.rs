//! The screen zone: the one open ledger screen (Colony, Stores, Buildings, Tech, Events)
//! drawn centred over the map from the view-models in `screens.rs`, plus click handling for every
//! zone. Content is rebuilt only when its view-model changed, so hover and scroll state survive
//! between sim updates. The other zones live in `topbar`, `toolbar`, `jobs`, `alerts` and `inspector`;
//! all of them are built from `kit`.
use super::icons::{Icon, IconAssets};
use super::inspector;
use super::kit::*;
use super::screens::*;
use super::shell::{NavItem, Shell};
use crate::flow::{cell_world, Flow, Selection};
use super::selection::{self as sel2, Selection as Selection2};
use crate::flow_view::UiLayout;
use crate::state::Sim;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;
use sim::ColonyHost;
use std::collections::HashMap;

pub const PANEL_W: f32 = 760.0;
/// Space kept below the open screen for the toolbar.
pub const BOTTOM_RESERVE: f32 = 92.0;
pub const TOP: f32 = ZONE_TOP;
pub const REFRESH_SECS: f32 = 0.25;

#[derive(Resource)]
pub struct ShellUi {
    pub shell: Shell,
    /// Sort state per (screen, table index): (column, ascending).
    pub sort: HashMap<(NavItem, usize), (usize, bool)>,
    pub status: String,
    pub hist: History,
    /// The entity whose panel is open (beaver, family, building, dam or tree).
    pub panel: Option<Selection2>,
    /// Detail groups the person opened on the panel.
    pub groups_open: std::collections::HashSet<&'static str>,
    pub(super) last_panel: Option<inspector::PanelShown>,
    pub(super) panel_timer: f32,
    pub(super) dirty: bool,
    timer: f32,
    last_screen: Option<Shown>,
}

impl Default for ShellUi {
    fn default() -> Self {
        ShellUi { shell: Shell::default(), sort: HashMap::new(), status: String::new(), hist: History::default(), panel: None, groups_open: Default::default(), last_panel: None, panel_timer: 0.0, dirty: true, timer: 0.0, last_screen: None }
    }
}

#[derive(Clone, PartialEq)]
struct Shown {
    screen: Screen,
    sorts: Vec<Option<(usize, bool)>>,
    status: String,
}

#[derive(Component)]
struct ScreenRoot;
#[derive(Component)]
struct ScreenBody;
#[derive(Component)]
struct ScreenHeader;
#[derive(Component)]
struct ScreenFooter;

pub struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        let mut ui = ShellUi::default();
        // `--open NAME` opens a screen at start (screenshots); `BC_UI_OPEN` does the same.
        let name = app.world().get_resource::<crate::cli::Cli>().and_then(|c| c.open.clone()).or_else(|| std::env::var("BC_UI_OPEN").ok());
        ui.shell.mode = app.world().get_resource::<crate::cli::Cli>().and_then(|c| c.mode.as_deref().and_then(super::mapmodes::MapMode::from_name)).unwrap_or_default();
        ui.shell.open = name.as_ref().and_then(|v| NavItem::ALL.into_iter().find(|n| n.label().eq_ignore_ascii_case(v)));
        ui.shell.tool = name.as_ref().and_then(|v| super::shell::HudTool::ALL.into_iter().find(|t| t.off_reason().is_none() && t.label().eq_ignore_ascii_case(v)));
        app.insert_resource(ui).add_systems(Startup, spawn_roots).add_systems(
            Update,
            (
                (layout_roots, refresh, hover_tint, scroll_wheel, tooltip).chain(),
                publish_blockers.before(crate::flow_view::ClickSet),
                handle_clicks.after(crate::flow_view::ClickSet),
                close_on_escape,
            ),
        );
    }
}

fn spawn_roots(mut commands: Commands) {
    commands
        .spawn((
            ScreenRoot,
            Blocker,
            panel(Node { width: px(PANEL_W), position_type: PositionType::Absolute, display: Display::None, ..default() }),
            GlobalZIndex(12),
        ))
        .with_children(|p| {
            p.spawn((ScreenHeader, Node { align_items: AlignItems::Center, column_gap: px(GRID), flex_shrink: 0.0, ..default() }));
            p.spawn((
                ScreenBody,
                ScrollArea,
                ScrollPosition::default(),
                Node { flex_direction: FlexDirection::Column, flex_shrink: 1.0, min_height: px(0), overflow: Overflow::scroll_y(), row_gap: px(GRID * 2.0), padding: UiRect::right(px(4)), ..default() },
            ));
            p.spawn((ScreenFooter, Node { min_height: px(18), flex_shrink: 0.0, ..default() }));
        });
    spawn_tip_root(&mut commands);
}

fn layout_roots(windows: Query<&Window, With<PrimaryWindow>>, sim: Res<Sim>, ui: Res<ShellUi>, mut q: Query<&mut Node, With<ScreenRoot>>) {
    let Ok(w) = windows.single() else { return };
    let founded = sim.0.colony.is_founded();
    for mut n in &mut q {
        let width = PANEL_W.min(w.width() - 20.0);
        n.display = if founded && ui.shell.open.is_some() { Display::Flex } else { Display::None };
        n.width = px(width);
        n.left = px(((w.width() - width) / 2.0).max(10.0));
        n.top = px(TOP + 4.0);
        n.max_height = px((w.height() - TOP - BOTTOM_RESERVE).max(200.0));
    }
}

// ───────────────────────────── drawing ─────────────────────────────

fn kpi_card(p: &mut Kids, k: &Kpi, icons: &IconAssets) {
    p.spawn((
        Node { width: px(112), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(4), padding: UiRect::axes(px(8), px(8)), flex_shrink: 0.0, ..default() },
        BackgroundColor(RAISED),
        Corner::Button,
    ))
    .with_children(|c| {
        icon_node(c, icons, k.icon, 28.0);
        number(c, &k.num, 20.0, tone_color(if k.tone == Tone::Dim { Tone::Normal } else { k.tone }));
        if let Some(f) = k.frac {
            bar(c, f, 80.0, 6.0, tone_color(if matches!(k.tone, Tone::Normal | Tone::Dim) { Tone::Good } else { k.tone }));
        }
        label(c, short(&k.num.label), T_MIN, DIM);
    });
}

/// Caption under a headline number (the label without its trailing "met").
fn short(s: &str) -> String {
    s.trim_end_matches(" met").to_string()
}

fn spawn_table(body: &mut Kids, t: &Table, idx: usize, sort: Option<(usize, bool)>, icons: &IconAssets) {
    body.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), flex_shrink: 0.0, ..default() }).with_children(|k| {
        label_bold(k, t.title, T_TITLE, PITH);
        k.spawn(Node { padding: UiRect::horizontal(px(GRID)), ..default() }).with_children(|h| {
            let heads = std::iter::once((t.name_col, t.name_w + 22.0)).chain(t.cols.iter().map(|c| (c.name, c.width)));
            for (ci, (name, w)) in heads.enumerate() {
                let arrow = match sort {
                    Some((c, asc)) if c == ci => if asc { " ^" } else { " v" },
                    _ => "",
                };
                let mut e = h.spawn((
                    Node { width: px(w), height: px(28), padding: UiRect::horizontal(px(4)), align_items: AlignItems::Center, overflow: Overflow::clip(), flex_shrink: 0.0, ..default() },
                    BackgroundColor(BARK),
                ));
                if !name.is_empty() {
                    e.insert((Clickable { act: Do::Sort(idx, ci), layer: 2, scrolled: false, base: BARK, hot: LINE }, Tip::note("Click to sort")));
                }
                e.with_children(|c| label(c, format!("{name}{arrow}"), T_MIN, if arrow.is_empty() { DIM } else { POND }));
            }
        });
        if t.rows.is_empty() {
            label(k, t.empty, T_BODY, DIM);
        }
        for (ri, row_data) in t.rows.iter().enumerate() {
            let base = row_fill(ri);
            let mut e = k.spawn(row(ri));
            if let Some(a) = &row_data.click {
                e.insert(Clickable { act: a.clone(), layer: 1, scrolled: true, base, hot: LINE });
            }
            e.with_children(|r| {
                r.spawn(Node { width: px(t.name_w + 22.0), column_gap: px(GRID), align_items: AlignItems::Center, overflow: Overflow::clip(), flex_shrink: 0.0, ..default() }).with_children(|n| {
                    icon_node(n, icons, row_data.icon, 24.0);
                    label(n, row_data.name.clone(), T_BODY, PITH);
                });
                for (ci, cell) in row_data.cells.iter().enumerate() {
                    let w = t.cols.get(ci).map(|c| c.width).unwrap_or(60.0);
                    r.spawn(Node { width: px(w), height: percent(100), padding: UiRect::horizontal(px(4)), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, row_gap: px(2), overflow: Overflow::clip(), flex_shrink: 0.0, ..default() })
                        .with_children(|c| spawn_cell(c, cell, w, icons));
                }
            });
        }
    });
}

fn spawn_cell(c: &mut Kids, cell: &Cell, w: f32, icons: &IconAssets) {
    match cell {
        Cell::Text(s, tone) => label(c, s.clone(), T_BODY, tone_color(*tone)),
        Cell::Entry(icon, s) => {
            c.spawn(Node { column_gap: px(4), align_items: AlignItems::Center, ..default() }).with_children(|e| {
                icon_node(e, icons, *icon, 20.0);
                label(e, s.clone(), T_BODY, PITH);
            });
        }
        Cell::Num { num, tone, frac } => {
            c.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }).with_children(|n| {
                number(n, num, T_BODY, tone_color(*tone));
                if let Some(f) = frac {
                    bar(n, *f, (w - 10.0).min(56.0), 6.0, tone_color(if matches!(tone, Tone::Normal | Tone::Dim) { Tone::Good } else { *tone }));
                }
            });
        }
        Cell::Actions(list) => {
            c.spawn(Node { column_gap: px(4), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                for a in list {
                    spawn_button_in(r, icons, KitButton::new(a.icon, a.label.clone(), a.act.clone()).layer(3).tip(a.label.clone()).compact(), true);
                }
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    icons: Res<IconAssets>,
    tool: Res<crate::build::BuildUi>,
    mut ui: ResMut<ShellUi>,
    header: Query<Entity, With<ScreenHeader>>,
    body: Query<Entity, With<ScreenBody>>,
    footer: Query<Entity, With<ScreenFooter>>,
) {
    let s = &sim.0;
    if !s.colony.is_founded() {
        ui.hist = History::default();
        ui.last_screen = None;
        return;
    }
    ui.hist.record(s);
    ui.timer -= time.delta_secs();
    if ui.timer > 0.0 && !ui.dirty {
        return;
    }
    ui.timer = REFRESH_SECS;
    ui.dirty = false;

    let Some(item) = ui.shell.open else {
        ui.last_screen = None;
        return;
    };
    let mut scr = screen(s, item, ui.hist.rates());
    if let (NavItem::Buildings, Some(k)) = (item, tool.tool) {
        mark_chosen(&mut scr, k);
    }
    let mut sorts = Vec::new();
    for (i, t) in scr.tables.iter_mut().enumerate() {
        let st = ui.sort.get(&(item, i)).copied();
        if let Some((c, asc)) = st {
            sort_rows(&mut t.rows, c, asc);
        }
        sorts.push(st);
    }
    let shown = Shown { screen: scr, sorts, status: ui.status.clone() };
    if ui.last_screen.as_ref() == Some(&shown) {
        return;
    }
    let first = ui.last_screen.as_ref().map(|d| d.screen.item) != Some(item);
    if first && let Ok(e) = header.single() {
        commands.entity(e).despawn_children();
        commands.entity(e).with_children(|p| {
            icon_node(p, &icons, item.icon(), 32.0);
            label_bold(p, item.label(), T_TITLE + 2.0, PITH);
            p.spawn(Node { flex_grow: 1.0, ..default() });
            spawn_button(p, &icons, KitButton::new(Icon::Cancel, "Close", Do::Nav(item)).layer(4).tip("Close"));
        });
    }
    if let Ok(e) = body.single() {
        commands.entity(e).despawn_children();
        commands.entity(e).with_children(|p| {
            if !shown.screen.kpis.is_empty() {
                p.spawn(Node { column_gap: px(GRID), row_gap: px(GRID), flex_wrap: FlexWrap::Wrap, flex_shrink: 0.0, ..default() }).with_children(|k| {
                    for kpi in &shown.screen.kpis {
                        kpi_card(k, kpi, &icons);
                    }
                });
            }
            for (i, t) in shown.screen.tables.iter().enumerate() {
                spawn_table(p, t, i, shown.sorts[i], &icons);
            }
        });
    }
    if let Ok(e) = footer.single() {
        commands.entity(e).despawn_children();
        commands.entity(e).with_children(|p| {
            if shown.status.is_empty() {
                label(p, "Hover a number to see what adds up to it.", T_MIN, DIM);
            } else {
                label(p, shown.status.clone(), T_MIN, PITH);
            }
        });
    }
    ui.last_screen = Some(shown);
}

/// Tells the world picking code which screen areas are UI.
fn publish_blockers(mut layout: ResMut<UiLayout>, q: Query<(&ComputedNode, &UiGlobalTransform, &Node), With<Blocker>>) {
    layout.extra.clear();
    for (n, t, node) in &q {
        if node.display == Display::None || n.size.x < 1.0 {
            continue;
        }
        let c = t.transform_point2(Vec2::ZERO) * n.inverse_scale_factor;
        let half = n.size * n.inverse_scale_factor / 2.0;
        layout.extra.push(Rect::from_corners(c - half, c + half));
    }
}

fn scroll_wheel(
    windows: Query<&Window, With<PrimaryWindow>>,
    scroll: Res<AccumulatedMouseScroll>,
    mut q: Query<(&ComputedNode, &UiGlobalTransform, &mut ScrollPosition), With<ScrollArea>>,
) {
    let Some(cursor) = windows.single().ok().and_then(|w| w.physical_cursor_position()) else { return };
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y * 36.0,
        MouseScrollUnit::Pixel => scroll.delta.y,
    };
    if lines == 0.0 {
        return;
    }
    for (n, t, mut pos) in &mut q {
        if n.size.x > 1.0 && n.contains_point(*t, cursor) {
            let max = ((n.content_size.y - n.size.y) * n.inverse_scale_factor).max(0.0);
            pos.y = (pos.y - lines).clamp(0.0, max);
        }
    }
}

fn close_on_escape(keys: Res<ButtonInput<KeyCode>>, mut ui: ResMut<ShellUi>) {
    if keys.just_pressed(KeyCode::Escape) {
        if ui.shell.armed.is_some() || ui.shell.editor.is_some() {
            ui.shell.armed = None;
            ui.shell.editor = None;
            ui.dirty = true;
        } else if ui.shell.tool.is_some() {
            ui.shell.tool = None;
            ui.dirty = true;
        } else if ui.shell.open.is_some() {
            ui.shell.close();
            ui.dirty = true;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_clicks(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut press: Local<Option<Vec2>>,
    clickables: Query<(&Clickable, &ComputedNode, &UiGlobalTransform)>,
    scroll_areas: Query<(&ComputedNode, &UiGlobalTransform), With<ScrollArea>>,
    mut scroll_pos: Query<&mut ScrollPosition, With<ScrollArea>>,
    mut sim: ResMut<Sim>,
    mut flow: ResMut<Flow>,
    mut sel: ResMut<Selection>,
    mut ui: ResMut<ShellUi>,
    mut build: ResMut<crate::build::BuildUi>,
    mut clock: ResMut<crate::sim_driver::SimClock>,
    mut settle: ResMut<super::settle::SettleUi>,
) {
    let Ok(w) = windows.single() else { return };
    let (Some(phys), Some(logical)) = (w.physical_cursor_position(), w.cursor_position()) else { return };
    if buttons.just_pressed(MouseButton::Left) {
        *press = Some(logical);
    }
    if !buttons.just_released(MouseButton::Left) {
        return;
    }
    let Some(start) = press.take() else { return };
    if start.distance(logical) > 5.0 {
        return;
    }
    let Some((act, _)) = hit_test(phys, &clickables, &scroll_areas) else { return };
    let mut reset_scroll = false;
    if let Do::Clock(cmd) = act {
        super::hud::run_clock(cmd, &mut clock);
    } else if let Do::Settle(go) = act {
        super::settle::run(go, &mut settle, &mut sim, &mut flow, &mut clock, &mut ui);
    } else {
        apply(act, &mut sim, &mut flow, &mut sel, &mut ui, &mut build, &mut reset_scroll);
    }
    ui.dirty = true;
    if reset_scroll {
        for mut p in &mut scroll_pos {
            p.y = 0.0;
        }
    }
}

/// Runs one click. Errors from the sim are shown in the screen footer, never swallowed.
pub fn apply(act: Do, sim: &mut Sim, flow: &mut Flow, sel: &mut Selection, ui: &mut ShellUi, build: &mut crate::build::BuildUi, reset_scroll: &mut bool) {
    let s = &mut sim.0;
    match act {
        Do::Nav(item) => {
            ui.shell.toggle(item);
            ui.status.clear();
            *reset_scroll = true;
        }
        Do::Place(kind) => {
            crate::build::pick_tool(s, build, kind);
            if build.tool == Some(kind) {
                ui.shell.close();
                ui.shell.tool = None;
                ui.status.clear();
            } else {
                ui.status = build.message.clone();
            }
        }
        // The clock is a resource of its own; `handle_clicks` runs it before `apply`.
        Do::Clock(_) | Do::Settle(_) => {}
        Do::Tool(t) => {
            ui.shell.toggle_tool(t);
            ui.status = t.off_reason().map(str::to_string).unwrap_or_default();
            *reset_scroll = true;
        }
        Do::Tab(g) => {
            ui.shell.build_tab = g;
            ui.status.clear();
        }
        Do::SetJob(id, job) => {
            ui.status = match s.colony.set_job(id, job) {
                Ok(()) => String::new(),
                Err(e) => e.to_string(),
            };
        }
        Do::Promote(f, i) => {
            ui.status = crate::build_logic::promote_priority(&mut s.colony, f, i).err().unwrap_or_default();
        }
        Do::Staff(job, d) => {
            let cur = s.colony.beavers.iter().filter(|b| b.job == Some(job)).count() as i32;
            ui.status = match crate::build_logic::set_job_count(&mut s.colony, job, (cur + d).max(0) as usize) {
                Ok(n) => format!("{n} on {job:?} duty."),
                Err(e) => e,
            };
        }
        Do::Select(x) => {
            ui.panel = Some(x);
            *sel = inspector::to_flow(s, x);
            if let Some(at) = sel2::locate(s, x) {
                flow.focus = Some(cell_world(&s.height, (at.0 as f32, at.1 as f32)));
            }
            ui.status.clear();
        }
        Do::ClosePanel => {
            ui.panel = None;
            *sel = Selection::None;
            ui.status.clear();
        }
        Do::Cmd(cmd) => {
            ui.status = match s.apply(cmd) {
                Ok(()) => "Done.".into(),
                Err(e) => format!("Cannot do that: {e}."),
            };
        }
        Do::Group(g) => {
            if !ui.groups_open.remove(g) {
                ui.groups_open.insert(g);
            }
        }
        Do::Mode(m) => {
            ui.shell.mode = if ui.shell.mode == m { super::mapmodes::MapMode::Normal } else { m };
            ui.shell.armed = None;
            ui.shell.editor = None;
            ui.status.clear();
        }
        Do::ModeAct(a) => {
            let building = match ui.panel {
                Some(Selection2::Building(b)) => Some(b),
                _ => None,
            };
            let mut panel = None;
            let r = super::mapmodes::run_action(a, s, &mut ui.shell, &mut panel, build, building);
            ui.status = match r {
                Ok(m) => m,
                Err(e) => format!("Cannot do that: {e}"),
            };
            if let Some(x) = panel {
                ui.panel = Some(x);
                *sel = inspector::to_flow(s, x);
                if let Some(at) = sel2::locate(s, x) {
                    flow.focus = Some(cell_world(&s.height, (at.0 as f32, at.1 as f32)));
                }
            }
        }
        Do::Focus(x, y) => flow.focus = Some(cell_world(&s.height, (x as f32, y as f32))),
        Do::Sort(table, col) => {
            if let Some(item) = ui.shell.open {
                let key = (item, table);
                let next = match ui.sort.get(&key) {
                    Some((c, asc)) if *c == col => (col, !asc),
                    _ => (col, true),
                };
                ui.sort.insert(key, next);
            }
        }
        Do::Pick(id) => {
            if let Some(b) = s.colony.beaver(id) {
                *sel = Selection::Beaver(id);
                flow.focus = Some(cell_world(&s.height, b.pos));
                ui.status = format!("Following {} on the map.", b.name);
            } else {
                ui.status = "That beaver is gone.".into();
            }
        }
        Do::Jump(x, y) => {
            flow.focus = Some(cell_world(&s.height, (x, y)));
            *sel = Selection::Cell((x.max(0.0) as usize, y.max(0.0) as usize));
            ui.status = format!("Camera moved to ({x:.0}, {y:.0}).");
        }
        Do::Choose(id, opt) => {
            let mut host = ColonyHost { colony: &mut s.colony };
            ui.status = match s.events.resolve(id, opt, &mut host) {
                Ok(()) => "Decision made.".into(),
                Err(e) => format!("Cannot decide: {e}."),
            };
        }
        Do::Answer(id, accept) => {
            ui.status = match s.apply(super::events::answer_command(id, accept)) {
                Ok(()) => if accept { "Welcome to the colony.".into() } else { "Turned away.".into() },
                Err(e) => format!("Cannot answer: {e:?}."),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Rig {
        sim: Sim,
        flow: Flow,
        sel: Selection,
        ui: ShellUi,
        build: crate::build::BuildUi,
    }

    fn rig() -> Rig {
        let sim = Sim(super::super::screens::tests::lived_in(3));
        let flow = Flow::new(&sim.0.world);
        Rig { sim, flow, sel: Selection::None, ui: ShellUi::default(), build: Default::default() }
    }

    fn click(r: &mut Rig, a: Do) {
        let mut reset = false;
        apply(a, &mut r.sim, &mut r.flow, &mut r.sel, &mut r.ui, &mut r.build, &mut reset);
    }

    #[test]
    fn each_screen_opens_from_the_bar_and_closes_again() {
        let mut r = rig();
        assert_eq!(r.ui.shell.open, None, "the first view is only the map, bar and time controls");
        for item in NavItem::ALL {
            click(&mut r, Do::Nav(item));
            assert_eq!(r.ui.shell.open, Some(item), "{item:?} opens");
            let nav = nav_bar(&r.sim.0, r.ui.shell.open);
            assert_eq!(nav.entries.iter().filter(|e| e.open).count(), 1, "one screen at a time");
            assert!(!screen(&r.sim.0, item, Rates::default()).tables.is_empty());
            click(&mut r, Do::Nav(item));
            assert_eq!(r.ui.shell.open, None, "{item:?} closes");
        }
        // opening another screen replaces the open one
        click(&mut r, Do::Nav(NavItem::Colony));
        click(&mut r, Do::Nav(NavItem::Stores));
        assert_eq!(r.ui.shell.open, Some(NavItem::Stores));
        click(&mut r, Do::Nav(NavItem::Stores));
        assert_eq!(r.ui.shell.open, None);
    }

    #[test]
    fn header_click_sorts_then_reverses() {
        let mut r = rig();
        click(&mut r, Do::Nav(NavItem::Colony));
        click(&mut r, Do::Sort(1, 4));
        assert_eq!(r.ui.sort[&(NavItem::Colony, 1)], (4, true));
        click(&mut r, Do::Sort(1, 4));
        assert_eq!(r.ui.sort[&(NavItem::Colony, 1)], (4, false));
    }

    #[test]
    fn pick_and_jump_move_the_camera_focus_and_report_gone() {
        let mut r = rig();
        let id = r.sim.0.colony.beavers[0].id;
        click(&mut r, Do::Pick(id));
        assert_eq!(r.sel, Selection::Beaver(id));
        assert!(r.flow.focus.is_some());
        r.flow.focus = None;
        click(&mut r, Do::Jump(10.0, 12.0));
        assert!(r.flow.focus.is_some());
        assert_eq!(r.sel, Selection::Cell((10, 12)));
        click(&mut r, Do::Pick(sim::colony::BeaverId(u64::MAX)));
        assert_eq!(r.ui.status, "That beaver is gone.");
    }

    #[test]
    fn choosing_resolves_a_pending_event() {
        let mut r = rig();
        let Some(p) = r.sim.0.events.pending().first().cloned() else { return };
        click(&mut r, Do::Choose(p.id, 0));
        assert!(r.sim.0.events.pending().iter().all(|x| x.id != p.id));
        assert_eq!(r.ui.status, "Decision made.");
        click(&mut r, Do::Choose(p.id, 0));
        assert!(r.ui.status.starts_with("Cannot decide"));
    }

    #[test]
    fn placing_a_building_closes_the_screen_and_picks_the_tool() {
        let mut r = rig();
        click(&mut r, Do::Nav(NavItem::Buildings));
        click(&mut r, Do::Place(sim::buildings::BuildingKind::Lodge));
        assert_eq!(r.build.tool, Some(sim::buildings::BuildingKind::Lodge));
        assert_eq!(r.ui.shell.open, None, "the map is free to place on");
    }

    #[test]
    fn staffing_moves_adults_onto_a_job() {
        let mut r = rig();
        let before = r.sim.0.colony.beavers.iter().filter(|b| b.job == Some(sim::colony::Job::Logger)).count();
        click(&mut r, Do::Staff(sim::colony::Job::Logger, 1));
        let after = r.sim.0.colony.beavers.iter().filter(|b| b.job == Some(sim::colony::Job::Logger)).count();
        assert!(after >= before);
        assert!(r.ui.status.contains("Logger"), "{}", r.ui.status);
    }

    #[test]
    fn tips_list_every_part_of_a_number() {
        let sim = Sim(super::super::screens::tests::lived_in(3));
        let s = screen(&sim.0, NavItem::Colony, Rates::default());
        let tip = Tip::of(&s.kpis[0].num);
        assert_eq!(tip.lines.len(), s.kpis[0].num.parts.len());
        assert!(tip.title.starts_with("Beavers: "));
    }

    /// Runs the camera easing until it stops, as the game does each frame.
    fn fly(r: &mut Rig, orbit: &mut crate::camera::OrbitCamera) {
        for _ in 0..600 {
            let Some(f) = r.flow.focus else { return };
            let (next, arrived) = crate::flow::ease_step(*orbit, f, 0.1, false);
            *orbit = next;
            if arrived {
                r.flow.focus = None;
            }
        }
    }

    #[test]
    fn activating_any_event_or_alert_moves_the_camera_target_to_its_place() {
        use super::super::events::{alerts, every_kind_for_tests, AlertKind};
        let mut r = rig();
        let notices = every_kind_for_tests(&mut r.sim.0);
        let mut found = std::collections::HashSet::new();
        for a in alerts(&r.sim.0).into_iter().chain(notices.items.iter().cloned()) {
            let t = super::super::events::toast(a.clone());
            let mut orbit = crate::camera::OrbitCamera { target: Vec3::new(500.0, 0.0, 500.0), yaw: 0.8, pitch: 0.7, distance: 80.0 };
            click(&mut r, t.jump.clone());
            fly(&mut r, &mut orbit);
            let want = cell_world(&r.sim.0.height, (a.at.0 as f32, a.at.1 as f32));
            assert!(orbit.target.distance(want) < 0.01, "{:?}: camera at {:?}, place at {:?}", a.kind, orbit.target, want);
            found.insert(a.kind);
        }
        assert_eq!(found.len(), AlertKind::ALL.len(), "all ten kinds were activated");
    }

    #[test]
    fn accepting_a_join_request_adds_a_beaver_to_a_family_and_turning_away_changes_nothing() {
        let mut r = rig();
        let colony = r.sim.0.colony.clone();
        let (a, b) = (r.sim.0.society.raise(&colony, true, 1.0).unwrap(), r.sim.0.society.raise(&colony, true, 1.0).unwrap());
        let pop = r.sim.0.colony.population();
        click(&mut r, Do::Answer(b, false));
        assert_eq!(r.sim.0.colony.population(), pop, "turning away leaves the population unchanged");
        assert!(r.sim.0.society.pending().iter().all(|x| x.id != b));
        click(&mut r, Do::Answer(a, true));
        assert_eq!(r.sim.0.colony.population(), pop + 1);
        let newcomer = r.sim.0.colony.beavers.last().unwrap();
        assert!(r.sim.0.colony.family(newcomer.family).is_some(), "the newcomer belongs to a family");
        assert_eq!(r.ui.status, "Welcome to the colony.");
    }

    #[test]
    fn toolbar_clicks_open_one_menu_and_off_tools_say_why() {
        use super::super::shell::HudTool;
        let mut r = rig();
        click(&mut r, Do::Tool(HudTool::Build));
        assert_eq!(r.ui.shell.tool, Some(HudTool::Build));
        click(&mut r, Do::Tool(HudTool::Jobs));
        assert_eq!(r.ui.shell.tool, Some(HudTool::Jobs));
        click(&mut r, Do::Tool(HudTool::Demolish));
        assert_eq!(r.ui.shell.tool, Some(HudTool::Jobs), "an off tool changes nothing");
        assert!(r.ui.status.contains("taken down"), "{}", r.ui.status);
    }

    #[test]
    fn choosing_a_build_tile_arms_the_tool_and_closes_the_menu() {
        use super::super::shell::HudTool;
        let mut r = rig();
        click(&mut r, Do::Tool(HudTool::Build));
        click(&mut r, Do::Tab(None));
        click(&mut r, Do::Place(sim::buildings::BuildingKind::Lodge));
        assert_eq!(r.build.tool, Some(sim::buildings::BuildingKind::Lodge));
        assert_eq!(r.ui.shell.tool, None);
    }

    #[test]
    fn gather_arms_an_area_order_and_closes_the_menu() {
        use super::super::mapmodes::ModeAction;
        use super::super::shell::HudTool;
        let mut r = rig();
        click(&mut r, Do::Tool(HudTool::Gather));
        click(&mut r, Do::ModeAct(ModeAction::AssignFellers));
        assert!(r.ui.shell.armed.is_some());
        assert_eq!(r.ui.shell.tool, None);
    }

    #[test]
    fn job_panel_buttons_set_a_beavers_job_and_a_familys_priorities() {
        let mut r = rig();
        let id = r.sim.0.colony.beavers.iter().find(|b| b.stage == sim::colony::Stage::Adult).unwrap().id;
        click(&mut r, Do::SetJob(id, Some(sim::colony::Job::Builder)));
        assert_eq!(r.sim.0.colony.beaver(id).unwrap().job, Some(sim::colony::Job::Builder));
        click(&mut r, Do::SetJob(id, None));
        assert_eq!(r.sim.0.colony.beaver(id).unwrap().job, None);
        let fam = r.sim.0.colony.beaver(id).unwrap().family;
        let before = r.sim.0.colony.family(fam).unwrap().priorities.clone();
        if before.len() > 1 {
            click(&mut r, Do::Promote(fam, 1));
            let after = r.sim.0.colony.family(fam).unwrap().priorities.clone();
            assert_eq!(after[0], before[1]);
        }
    }
}
