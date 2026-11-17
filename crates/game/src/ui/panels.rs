//! Draws the navigation bar, the time label, the open screen and the hover explanations from the
//! view-models in `screens.rs`, and runs what the player clicks. Content is rebuilt only when its
//! view-model changed, so hover and scroll state survive between sim updates.
mod entity;
use super::explain::Explained;
use super::icons::{Icon, IconAssets};
use super::kit::*;
use super::screens::*;
use super::shell::{NavItem, Shell};
use crate::flow::{cell_world, Flow, Selection};
use entity::PanelShown;
use super::selection::{self as sel2, Selection as Selection2};
use crate::flow_view::{Action, UiLayout};
use crate::state::Sim;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;
use sim::events::Season;
use sim::ColonyHost;
use std::collections::HashMap;

pub const PANEL_W: f32 = 660.0;
/// Space kept below the open screen for the navigation bar.
pub const NAV_RESERVE: f32 = 84.0;
pub const TOP: f32 = 48.0;
const ROW_H: f32 = 24.0;
const REFRESH_SECS: f32 = 0.25;

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
    last_panel: Option<PanelShown>,
    panel_timer: f32,
    dirty: bool,
    timer: f32,
    last_nav: Option<NavBar>,
    last_time: Option<TimeBar>,
    last_screen: Option<Shown>,
}

impl Default for ShellUi {
    fn default() -> Self {
        ShellUi { shell: Shell::default(), sort: HashMap::new(), status: String::new(), hist: History::default(), panel: None, groups_open: Default::default(), last_panel: None, panel_timer: 0.0, dirty: true, timer: 0.0, last_nav: None, last_time: None, last_screen: None }
    }
}

#[derive(Clone, PartialEq)]
struct Shown {
    screen: Screen,
    sorts: Vec<Option<(usize, bool)>>,
    status: String,
}

/// Text shown while hovering a node: a title and its terms.
#[derive(Component, Clone, PartialEq, Debug)]
pub struct Tip {
    pub title: String,
    pub lines: Vec<(String, String)>,
}

impl Tip {
    pub fn note(title: impl Into<String>) -> Tip {
        Tip { title: title.into(), lines: Vec::new() }
    }
    /// "Label = value" over each term that adds up to it.
    pub fn of(n: &Explained) -> Tip {
        let fmt = |v: f32| format!("{v:+.*}{}", n.dec.max(if n.parts.iter().any(|p| p.value.fract().abs() > 0.004) { 1 } else { 0 }) as usize, n.unit);
        Tip {
            title: format!("{}: {}", n.label, n.show()),
            lines: n.parts.iter().map(|p| (p.label.clone(), fmt(p.value))).collect(),
        }
    }
}

#[derive(Component)]
struct NavRoot;
#[derive(Component)]
struct TimeRoot;
#[derive(Component)]
struct ScreenRoot;
#[derive(Component)]
struct ScreenBody;
#[derive(Component)]
struct ScreenHeader;
#[derive(Component)]
struct ScreenFooter;
#[derive(Component)]
struct TipRoot;
#[derive(Component)]
struct ToastRoot;

pub struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        let mut ui = ShellUi::default();
        // `--open NAME` opens a screen at start (screenshots); `BC_UI_OPEN` does the same.
        let name = app.world().get_resource::<crate::cli::Cli>().and_then(|c| c.open.clone()).or_else(|| std::env::var("BC_UI_OPEN").ok());
        ui.shell.open = name.and_then(|v| NavItem::ALL.into_iter().find(|n| n.label().eq_ignore_ascii_case(&v)));
        app.insert_resource(ui).add_systems(Startup, spawn_roots).add_systems(
            Update,
            (
                (layout_roots, refresh, refresh_toasts, hover_tint, scroll_wheel, tooltip).chain(),
                publish_blockers.before(crate::flow_view::ClickSet),
                handle_clicks.after(crate::flow_view::ClickSet),
                close_on_escape,
                (entity::sync_flow_selection, entity::select_for_screenshot, entity::layout_entity, entity::refresh_entity).chain(),
            ),
        );
    }
}

fn shadow() -> BoxShadow {
    BoxShadow::new(Color::srgba(0.25, 0.15, 0.05, 0.30), px(0), px(3), px(0), px(9))
}

fn spawn_roots(mut commands: Commands) {
    let abs = Node { position_type: PositionType::Absolute, ..default() };
    commands.spawn((
        NavRoot,
        Blocker,
        Node { height: px(62), align_items: AlignItems::Center, column_gap: px(6), padding: UiRect::axes(px(10), px(4)), ..abs.clone() },
        BackgroundColor(PANEL),
        Round(22.0),
        shadow(),
        GlobalZIndex(10),
    ));
    commands.spawn((
        TimeRoot,
        Blocker,
        Node { height: px(28), align_items: AlignItems::Center, column_gap: px(8), padding: UiRect::axes(px(12), px(0)), ..abs.clone() },
        BackgroundColor(PANEL),
        Round(14.0),
        shadow(),
        GlobalZIndex(10),
    ));
    commands
        .spawn((
            ScreenRoot,
            Blocker,
            Node { width: px(PANEL_W), flex_direction: FlexDirection::Column, padding: UiRect::all(px(10)), row_gap: px(6), display: Display::None, ..abs.clone() },
            BackgroundColor(PANEL),
            Round(18.0),
            BoxShadow::new(Color::srgba(0.25, 0.15, 0.05, 0.34), px(0), px(4), px(0), px(12)),
            GlobalZIndex(12),
        ))
        .with_children(|p| {
            p.spawn((ScreenHeader, Node { align_items: AlignItems::Center, column_gap: px(8), flex_shrink: 0.0, ..default() }));
            p.spawn((
                ScreenBody,
                ScrollArea,
                ScrollPosition::default(),
                Node { flex_direction: FlexDirection::Column, flex_shrink: 1.0, min_height: px(0), overflow: Overflow::scroll_y(), row_gap: px(10), padding: UiRect::right(px(4)), ..default() },
            ));
            p.spawn((ScreenFooter, Node { min_height: px(14), flex_shrink: 0.0, ..default() }));
        });
    entity::spawn_root(&mut commands);
    commands.spawn((
        ToastRoot,
        Blocker,
        Node { position_type: PositionType::Absolute, left: px(12), top: px(12), display: Display::None, flex_direction: FlexDirection::Column, row_gap: px(6), max_width: px(380), ..default() },
        GlobalZIndex(9),
    ));
    commands.spawn((
        TipRoot,
        Node { position_type: PositionType::Absolute, display: Display::None, flex_direction: FlexDirection::Column, padding: UiRect::axes(px(10), px(7)), row_gap: px(2), max_width: px(300), ..default() },
        BackgroundColor(Color::srgba(0.22, 0.15, 0.10, 0.96)),
        Round(10.0),
        GlobalZIndex(40),
    ));
}

fn season_color(s: Season) -> Color {
    match s {
        Season::Spring => Color::srgb(0.55, 0.78, 0.45),
        Season::Summer => Color::srgb(0.95, 0.75, 0.30),
        Season::Autumn => Color::srgb(0.88, 0.52, 0.25),
        Season::Winter => Color::srgb(0.58, 0.74, 0.88),
    }
}

#[allow(clippy::type_complexity)]
fn layout_roots(
    windows: Query<&Window, With<PrimaryWindow>>,
    layout: Res<UiLayout>,
    sim: Res<Sim>,
    ui: Res<ShellUi>,
    mut q: ParamSet<(Query<(&mut Node, &ComputedNode), With<NavRoot>>, Query<&mut Node, With<TimeRoot>>, Query<&mut Node, With<ScreenRoot>>)>,
) {
    let Ok(w) = windows.single() else { return };
    let founded = sim.0.colony.is_founded();
    let show = |on: bool| if on { Display::Flex } else { Display::None };
    for (mut n, c) in &mut q.p0() {
        let width = c.size.x * c.inverse_scale_factor;
        n.display = show(founded);
        n.left = px(((w.width() - width) / 2.0).max(8.0));
        n.bottom = px(12);
    }
    for mut n in &mut q.p1() {
        let right = layout.rect(Action::Pause).map(|r| r.min.x - 8.0).unwrap_or(w.width() - 10.0);
        n.display = show(founded);
        n.right = px((w.width() - right).max(8.0));
        n.top = px(10);
    }
    for mut n in &mut q.p2() {
        n.display = show(founded && ui.shell.open.is_some());
        n.width = px(PANEL_W.min(w.width() - 20.0));
        n.left = px((w.width() - 10.0 - PANEL_W.min(w.width() - 20.0)).max(10.0));
        n.top = px(TOP);
        n.max_height = px((w.height() - TOP - NAV_RESERVE).max(200.0));
    }
}

// ───────────────────────────── drawing ─────────────────────────────

fn icon_node(p: &mut Kids, icons: &IconAssets, icon: Icon, size: f32) {
    p.spawn((Node { width: px(size), height: px(size), flex_shrink: 0.0, ..default() }, ImageNode::new(icons.get(icon))));
}

fn label(p: &mut Kids, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(s), TextFont::from_font_size(size), TextColor(color), TextLayout::no_wrap()));
}

fn spawn_nav(p: &mut Kids, nav: &NavBar, icons: &IconAssets) {
    for e in &nav.entries {
        let base = if e.open { CHIP_ON } else { Color::srgba(0.0, 0.0, 0.0, 0.0) };
        p.spawn((
            Node { width: px(66), height: px(52), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, row_gap: px(1), flex_shrink: 0.0, ..default() },
            BackgroundColor(base),
            Round(14.0),
            Clickable { act: Do::Nav(e.item), layer: 5, scrolled: false, base, hot: ROW_HOT },
            Tip::note(if e.open { format!("{} (click to close)", e.label) } else { e.label.to_string() }),
        ))
        .with_children(|b| {
            icon_node(b, icons, e.icon, 30.0);
            label(b, e.label, 11.0, INK);
            if let Some(n) = &e.badge {
                b.spawn((
                    Node { position_type: PositionType::Absolute, top: px(0), right: px(6), min_width: px(18), height: px(18), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                    BackgroundColor(WARN),
                    Round(9.0),
                    Tip::of(n),
                ))
                .with_children(|c| label(c, n.show(), 11.0, INK));
            }
        });
    }
}

fn spawn_time(p: &mut Kids, t: &TimeBar) {
    p.spawn((Node { padding: UiRect::axes(px(9), px(2)), ..default() }, BackgroundColor(season_color(t.season)), Round(99.0), Tip::note("Season")))
        .with_children(|c| label(c, format!("{:?}", t.season), 12.0, INK));
    p.spawn((Node { align_items: AlignItems::Center, ..default() }, Tip::of(&t.day))).with_children(|c| label(c, format!("Day {}", t.day.show()), 13.0, INK));
}

fn kpi_card(p: &mut Kids, k: &Kpi, icons: &IconAssets) {
    p.spawn((
        Node { width: px(96), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(2), padding: UiRect::axes(px(6), px(6)), flex_shrink: 0.0, ..default() },
        BackgroundColor(PANEL_SOFT),
        Round(14.0),
        Tip::of(&k.num),
    ))
    .with_children(|c| {
        icon_node(c, icons, k.icon, 24.0);
        label(c, k.num.show(), 18.0, tone_color(if k.tone == Tone::Dim { Tone::Normal } else { k.tone }));
        if let Some(f) = k.frac {
            bar(c, f, 70.0, 5.0, tone_color(if matches!(k.tone, Tone::Normal | Tone::Dim) { Tone::Good } else { k.tone }));
        }
        label(c, short(&k.num.label), 10.0, DIM);
    });
}

/// Caption under a headline number (the label without its trailing "met").
fn short(s: &str) -> String {
    s.trim_end_matches(" met").to_string()
}

fn spawn_table(body: &mut Kids, t: &Table, idx: usize, sort: Option<(usize, bool)>, icons: &IconAssets) {
    body.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), flex_shrink: 0.0, ..default() }).with_children(|k| {
        label(k, t.title, 13.0, INK);
        k.spawn(Node { padding: UiRect::horizontal(px(5)), ..default() }).with_children(|h| {
            let heads = std::iter::once((t.name_col, t.name_w + 22.0)).chain(t.cols.iter().map(|c| (c.name, c.width)));
            for (ci, (name, w)) in heads.enumerate() {
                let arrow = match sort {
                    Some((c, asc)) if c == ci => if asc { " ^" } else { " v" },
                    _ => "",
                };
                let mut e = h.spawn((
                    Node { width: px(w), height: px(19), padding: UiRect::horizontal(px(3)), align_items: AlignItems::Center, overflow: Overflow::clip(), flex_shrink: 0.0, ..default() },
                    BackgroundColor(PANEL_SOFT),
                    Round(8.0),
                ));
                if !name.is_empty() {
                    e.insert((Clickable { act: Do::Sort(idx, ci), layer: 2, scrolled: false, base: PANEL_SOFT, hot: ROW_HOT }, Tip::note("Click to sort")));
                }
                e.with_children(|c| label(c, format!("{name}{arrow}"), 11.0, DIM));
            }
        });
        if t.rows.is_empty() {
            label(k, t.empty, FONT, DIM);
        }
        for (ri, row) in t.rows.iter().enumerate() {
            let base = if ri % 2 == 0 { ROW_A } else { ROW_B };
            let mut e = k.spawn((Node { height: px(ROW_H), padding: UiRect::horizontal(px(5)), align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }, BackgroundColor(base), Round(8.0)));
            if let Some(a) = &row.click {
                e.insert(Clickable { act: a.clone(), layer: 1, scrolled: true, base, hot: ROW_HOT });
            }
            e.with_children(|r| {
                r.spawn(Node { width: px(t.name_w + 22.0), column_gap: px(5), align_items: AlignItems::Center, overflow: Overflow::clip(), flex_shrink: 0.0, ..default() }).with_children(|n| {
                    icon_node(n, icons, row.icon, 17.0);
                    label(n, row.name.clone(), FONT, INK);
                });
                for (ci, cell) in row.cells.iter().enumerate() {
                    let w = t.cols.get(ci).map(|c| c.width).unwrap_or(60.0);
                    r.spawn(Node { width: px(w), height: percent(100), padding: UiRect::horizontal(px(3)), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, row_gap: px(1), overflow: Overflow::clip(), flex_shrink: 0.0, ..default() })
                        .with_children(|c| spawn_cell(c, cell, w, icons));
                }
            });
        }
    });
}

fn spawn_cell(c: &mut Kids, cell: &Cell, w: f32, icons: &IconAssets) {
    match cell {
        Cell::Text(s, tone) => label(c, s.clone(), FONT, tone_color(*tone)),
        Cell::Entry(icon, s) => {
            c.spawn(Node { column_gap: px(4), align_items: AlignItems::Center, ..default() }).with_children(|e| {
                icon_node(e, icons, *icon, 15.0);
                label(e, s.clone(), FONT, INK);
            });
        }
        Cell::Num { num, tone, frac } => {
            c.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(1), ..default() }, Tip::of(num))).with_children(|n| {
                label(n, num.show(), FONT, tone_color(*tone));
                if let Some(f) = frac {
                    bar(n, *f, (w - 10.0).min(48.0), 3.0, tone_color(if matches!(tone, Tone::Normal | Tone::Dim) { Tone::Good } else { *tone }));
                }
            });
        }
        Cell::Actions(list) => {
            c.spawn(Node { column_gap: px(4), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                for a in list {
                    r.spawn((
                        Node { padding: UiRect::axes(px(6), px(1)), column_gap: px(3), align_items: AlignItems::Center, flex_shrink: 0.0, ..default() },
                        BackgroundColor(BTN),
                        Round(99.0),
                        Clickable { act: a.act.clone(), layer: 3, scrolled: true, base: BTN, hot: BTN_HOT },
                        Tip::note(a.label.clone()),
                    ))
                    .with_children(|b| {
                        icon_node(b, icons, a.icon, 15.0);
                        label(b, a.label.clone(), 11.0, INK);
                    });
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
    mut ui: ResMut<ShellUi>,
    nav: Query<Entity, With<NavRoot>>,
    time_root: Query<Entity, With<TimeRoot>>,
    header: Query<Entity, With<ScreenHeader>>,
    body: Query<Entity, With<ScreenBody>>,
    footer: Query<Entity, With<ScreenFooter>>,
) {
    let s = &sim.0;
    if !s.colony.is_founded() {
        ui.hist = History::default();
        ui.last_nav = None;
        ui.last_time = None;
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

    let open = ui.shell.open;
    let nb = nav_bar(s, open);
    if ui.last_nav.as_ref() != Some(&nb) {
        if let Ok(e) = nav.single() {
            commands.entity(e).despawn_children();
            commands.entity(e).with_children(|p| spawn_nav(p, &nb, &icons));
        }
        ui.last_nav = Some(nb);
    }
    let tb = time_bar(s);
    if ui.last_time.as_ref() != Some(&tb) {
        if let Ok(e) = time_root.single() {
            commands.entity(e).despawn_children();
            commands.entity(e).with_children(|p| spawn_time(p, &tb));
        }
        ui.last_time = Some(tb);
    }

    let Some(item) = open else {
        ui.last_screen = None;
        return;
    };
    let mut scr = screen(s, item, ui.hist.rates());
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
            icon_node(p, &icons, item.icon(), 26.0);
            label(p, item.label(), 17.0, INK);
            p.spawn(Node { flex_grow: 1.0, ..default() });
            p.spawn((
                Node { padding: UiRect::axes(px(7), px(3)), column_gap: px(4), align_items: AlignItems::Center, ..default() },
                BackgroundColor(CHIP),
                Round(99.0),
                Clickable { act: Do::Nav(item), layer: 4, scrolled: false, base: CHIP, hot: ROW_HOT },
                Tip::note("Close"),
            ))
            .with_children(|b| {
                icon_node(b, &icons, Icon::Cancel, 16.0);
                label(b, "Close", 11.0, INK);
            });
        });
    }
    if let Ok(e) = body.single() {
        commands.entity(e).despawn_children();
        commands.entity(e).with_children(|p| {
            if !shown.screen.kpis.is_empty() {
                p.spawn(Node { column_gap: px(8), row_gap: px(8), flex_wrap: FlexWrap::Wrap, flex_shrink: 0.0, ..default() }).with_children(|k| {
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
                label(p, "Hover a number to see what adds up to it.", 11.0, DIM);
            } else {
                label(p, shown.status.clone(), 11.0, INK);
            }
        });
    }
    ui.last_screen = Some(shown);
}

/// Short-lived notices on the map: click one to fly the camera to its place; join requests carry
/// accept and turn-away buttons.
fn refresh_toasts(
    mut commands: Commands,
    sim: Res<Sim>,
    notices: Res<super::NoticesRes>,
    icons: Res<IconAssets>,
    mut root: Query<(Entity, &mut Node), With<ToastRoot>>,
    mut shown: Local<Vec<super::events::Toast>>,
) {
    let Ok((e, mut node)) = root.single_mut() else { return };
    let now = if sim.0.colony.is_founded() { super::events::toasts(&sim.0, &notices.0) } else { Vec::new() };
    if *shown == now {
        return;
    }
    node.display = if now.is_empty() { Display::None } else { Display::Flex };
    commands.entity(e).despawn_children();
    commands.entity(e).with_children(|p| {
        for t in &now {
            p.spawn((
                Node { padding: UiRect::axes(px(8), px(5)), column_gap: px(6), align_items: AlignItems::Center, ..default() },
                BackgroundColor(PANEL),
                Round(14.0),
                Clickable { act: t.jump.clone(), layer: 3, scrolled: false, base: PANEL, hot: ROW_HOT },
                Tip::note(format!("{} - click to fly there", t.alert.kind.label())),
            ))
            .with_children(|r| {
                icon_node(r, &icons, t.alert.kind.icon(), 20.0);
                label(r, t.alert.text.clone(), 12.0, INK);
                for a in &t.actions {
                    r.spawn((
                        Node { padding: UiRect::axes(px(6), px(1)), column_gap: px(3), align_items: AlignItems::Center, flex_shrink: 0.0, ..default() },
                        BackgroundColor(BTN),
                        Round(99.0),
                        Clickable { act: a.act.clone(), layer: 5, scrolled: false, base: BTN, hot: BTN_HOT },
                        Tip::note(a.label.clone()),
                    ))
                    .with_children(|b| {
                        icon_node(b, &icons, a.icon, 15.0);
                        label(b, a.label.clone(), 11.0, INK);
                    });
                }
            });
        }
    });
    *shown = now;
}

/// Shows the explanation of the number (or button) under the cursor.
fn tooltip(
    windows: Query<&Window, With<PrimaryWindow>>,
    tips: Query<(&Tip, &ComputedNode, &UiGlobalTransform, Option<&Clickable>)>,
    mut root: Query<(Entity, &mut Node), With<TipRoot>>,
    mut commands: Commands,
    mut shown: Local<Option<Tip>>,
) {
    let Ok(w) = windows.single() else { return };
    let Ok((e, mut node)) = root.single_mut() else { return };
    let cursor = w.physical_cursor_position();
    let hit = cursor.and_then(|c| {
        tips.iter()
            .filter(|(_, n, t, _)| n.size.x > 1.0 && n.contains_point(**t, c))
            // innermost (smallest) node wins, so a number beats the row or card around it
            .min_by(|a, b| (a.1.size.x * a.1.size.y).total_cmp(&(b.1.size.x * b.1.size.y)))
            .map(|(tip, ..)| tip.clone())
    });
    match (hit, w.cursor_position()) {
        (Some(tip), Some(pos)) => {
            if shown.as_ref() != Some(&tip) {
                commands.entity(e).despawn_children();
                commands.entity(e).with_children(|p| {
                    label(p, tip.title.clone(), 12.0, Color::srgb(1.0, 0.96, 0.85));
                    for (l, v) in &tip.lines {
                        p.spawn(Node { column_gap: px(14), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
                            label(r, l.clone(), 11.0, Color::srgb(0.85, 0.78, 0.66));
                            label(r, v.clone(), 11.0, Color::srgb(1.0, 0.96, 0.85));
                        });
                    }
                });
                *shown = Some(tip);
            }
            node.display = Display::Flex;
            node.left = px((pos.x + 14.0).min(w.width() - 230.0).max(4.0));
            node.top = px((pos.y + 18.0).min(w.height() - 40.0 - 14.0 * shown.as_ref().map_or(0, |t| t.lines.len()) as f32).max(4.0));
        }
        _ => {
            node.display = Display::None;
            *shown = None;
        }
    }
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
    if keys.just_pressed(KeyCode::Escape) && ui.shell.open.is_some() {
        ui.shell.close();
        ui.dirty = true;
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
    apply(act, &mut sim, &mut flow, &mut sel, &mut ui, &mut build, &mut reset_scroll);
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
                ui.status.clear();
            } else {
                ui.status = build.message.clone();
            }
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
            *sel = entity::to_flow(s, x);
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
}
