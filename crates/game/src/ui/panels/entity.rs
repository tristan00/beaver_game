//! The entity panel: what the player sees after clicking a beaver, family, building, dam or tree.
//! Drawn from the pure view-model in `ui/selection.rs`; headline numbers first, detail groups
//! folded until asked for.
use super::*;
use crate::ui::selection::{self as sel2, Panel, Selection as Selection2};

pub const ENTITY_W: f32 = 340.0;

#[derive(Component)]
pub struct EntityRoot;
#[derive(Component)]
pub struct EntityHeader;
#[derive(Component)]
pub struct EntityBody;

#[derive(Clone, PartialEq)]
pub struct PanelShown {
    sel: Option<Selection2>,
    panel: Option<Panel>,
    open: Vec<&'static str>,
    status: String,
}

pub fn spawn_root(commands: &mut Commands) {
    commands
        .spawn((
            EntityRoot,
            Blocker,
            Node {
                position_type: PositionType::Absolute,
                width: px(ENTITY_W),
                left: px(10),
                top: px(TOP),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(10)),
                row_gap: px(6),
                display: Display::None,
                ..default()
            },
            BackgroundColor(PANEL),
            Round(18.0),
            BoxShadow::new(Color::srgba(0.25, 0.15, 0.05, 0.34), px(0), px(4), px(0), px(12)),
            GlobalZIndex(11),
        ))
        .with_children(|p| {
            p.spawn((EntityHeader, Node { align_items: AlignItems::Center, column_gap: px(8), flex_shrink: 0.0, ..default() }));
            p.spawn((
                EntityBody,
                ScrollArea,
                ScrollPosition::default(),
                Node { flex_direction: FlexDirection::Column, flex_shrink: 1.0, min_height: px(0), overflow: Overflow::scroll_y(), row_gap: px(8), padding: UiRect::right(px(4)), ..default() },
            ));
        });
}

/// The map selection that goes with a panel selection, so the map highlights what the panel shows.
pub fn to_flow(s: &sim::SimState, x: Selection2) -> Selection {
    match x {
        Selection2::Beaver(id) => Selection::Beaver(id),
        Selection2::Family(f) => s.colony.lodges.iter().find(|l| l.family == f).map_or(Selection::None, |l| Selection::Lodge(l.id)),
        Selection2::Building(id) => Selection::Site(id),
        Selection2::Dam(c) => Selection::Building(c),
        Selection2::Tree(id) => Selection::Tree(id),
    }
}

/// The panel a map selection opens, if any.
fn from_flow(s: &sim::SimState, x: Selection) -> Option<Selection2> {
    match x {
        Selection::Beaver(id) => Some(Selection2::Beaver(id)),
        Selection::Lodge(id) => s.colony.lodges.iter().find(|l| l.id == id).map(|l| Selection2::Family(l.family)),
        Selection::Building(c) => Some(Selection2::Dam(c)),
        Selection::Tree(id) => Some(Selection2::Tree(id)),
        Selection::Site(id) => Some(Selection2::Building(id)),
        Selection::Cell(_) | Selection::None => None,
    }
}

/// Opens the panel of whatever the player picked (or closes it when they Escape) on the map.
pub fn sync_flow_selection(sim: Res<Sim>, sel: Res<Selection>, mut ui: ResMut<ShellUi>, mut last: Local<Selection>) {
    if *last == *sel {
        return;
    }
    *last = *sel;
    ui.panel = from_flow(&sim.0, *sel);
    ui.status.clear();
    ui.dirty = true;
}

/// `--select KIND`: once the start has begun, opens that kind of panel (for screenshots).
pub fn select_for_screenshot(
    cli: Res<crate::cli::Cli>,
    flow: Res<Flow>,
    mut sim: ResMut<Sim>,
    mut sel: ResMut<Selection>,
    mut ui: ResMut<ShellUi>,
    mut done: Local<bool>,
) {
    let Some(kind) = cli.select.as_deref() else { return };
    if *done || flow.phase != crate::flow::Phase::Settled || !sim.0.colony.is_founded() {
        return;
    }
    *done = true;
    let s = &mut sim.0;
    let from = s.colony.beavers.first().map_or((0.0, 0.0), |b| b.pos);
    let near = |c: (usize, usize)| (c.0 as f32 - from.0).powi(2) + (c.1 as f32 - from.1).powi(2);
    let pick = match kind {
        "beaver" => s.colony.beavers.iter().find(|b| b.stage == sim::colony::Stage::Adult).map(|b| Selection2::Beaver(b.id)),
        "family" => s.colony.families.first().map(|f| Selection2::Family(f.id)),
        "building" => s
            .buildings
            .buildings
            .iter()
            .filter(|b| b.is_complete() && b.kind != sim::buildings::BuildingKind::Dam)
            .min_by(|a, b| near(a.pos).total_cmp(&near(b.pos)))
            .map(|b| Selection2::Building(b.id)),
        "site" => {
            let kind = sim::buildings::BuildingKind::Workshop;
            let (cx, cy) = (from.0 as usize, from.1 as usize);
            let mut found = None;
            'outer: for r in 3..40usize {
                for x in cx.saturating_sub(r)..cx + r {
                    for y in cy.saturating_sub(r)..cy + r {
                        if let Ok(id) = s.place_site(kind, (x, y)) {
                            found = Some(Selection2::Building(id));
                            break 'outer;
                        }
                    }
                }
            }
            found
        }
        "dam" => {
            let w = s.height.width();
            (0..w * w).map(|i| (i % w, i / w)).filter(|&(x, y)| s.water.has_dam(x, y)).min_by(|a, b| near(*a).total_cmp(&near(*b))).map(Selection2::Dam)
        }
        _ => s
            .resources
            .items
            .iter()
            .filter(|r| r.kind == sim::resources::ResourceKind::Tree)
            .min_by(|a, b| near(a.cell).total_cmp(&near(b.cell)))
            .map(|r| Selection2::Tree(r.id)),
    };
    match pick {
        Some(x) => {
            ui.panel = Some(x);
            *sel = to_flow(s, x);
            ui.dirty = true;
        }
        None => error!("--select {kind}: nothing of that kind in this start"),
    }
}

pub fn layout_entity(windows: Query<&Window, With<PrimaryWindow>>, ui: Res<ShellUi>, sim: Res<Sim>, mut q: Query<&mut Node, With<EntityRoot>>) {
    let Ok(w) = windows.single() else { return };
    for mut n in &mut q {
        n.display = if sim.0.colony.is_founded() && ui.panel.is_some() { Display::Flex } else { Display::None };
        n.max_height = px((w.height() - TOP - NAV_RESERVE).max(200.0));
    }
}

fn spawn_stat(p: &mut Kids, st: &sel2::Stat, w: f32) {
    let fill = tone_color(if matches!(st.tone, Tone::Normal | Tone::Dim) { Tone::Good } else { st.tone });
    p.spawn((Node { width: px(w), flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }, Tip::of(&st.num))).with_children(|c| {
        c.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
            label(r, st.num.label.clone(), 11.0, DIM);
            label(r, st.num.show(), FONT, tone_color(st.tone));
        });
        if let Some(f) = st.frac {
            bar(c, f, w, 4.0, fill);
        }
    });
}

fn pill(p: &mut Kids, icons: &IconAssets, icon: Icon, text: &str, act: Option<Do>, tip: &str) {
    let live = act.is_some();
    let base = if live { BTN } else { CHIP };
    let mut e = p.spawn((
        Node { padding: UiRect::axes(px(7), px(2)), column_gap: px(4), align_items: AlignItems::Center, flex_shrink: 0.0, ..default() },
        BackgroundColor(base),
        Round(99.0),
        Tip::note(tip.to_string()),
    ));
    if let Some(a) = act {
        e.insert(Clickable { act: a, layer: 6, scrolled: true, base, hot: BTN_HOT });
    }
    e.with_children(|b| {
        icon_node(b, icons, icon, 15.0);
        label(b, text.to_string(), 11.0, if live { INK } else { DIM });
    });
}

fn spawn_header(p: &mut Kids, pn: &Panel, icons: &IconAssets) {
    icon_node(p, icons, pn.icon, 26.0);
    p.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|c| {
        label(c, pn.title.clone(), 16.0, INK);
        label(c, pn.kind, 10.0, DIM);
    });
    p.spawn(Node { flex_grow: 1.0, ..default() });
    pill(p, icons, Icon::MoveArea, "Show", Some(Do::Focus(pn.at.0, pn.at.1)), "Move the camera here");
    pill(p, icons, Icon::Cancel, "Close", Some(Do::ClosePanel), "Close this panel");
}

fn spawn_body(p: &mut Kids, pn: &Panel, open: &[&'static str], status: &str, icons: &IconAssets) {
    let w = ENTITY_W - 36.0;
    p.spawn((Text::new(pn.prose.clone()), TextFont::from_font_size(12.0), TextColor(DIM), Node { max_width: px(w), flex_shrink: 0.0, ..default() }));
    for f in &pn.facts {
        p.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }).with_children(|r| {
            icon_node(r, icons, f.icon, 15.0);
            label(r, f.label, 11.0, DIM);
            r.spawn((Text::new(f.value.clone()), TextFont::from_font_size(FONT), TextColor(INK), Node { max_width: px(w - 80.0), ..default() }));
        });
    }
    p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(5), flex_shrink: 0.0, ..default() }).with_children(|c| {
        for st in &pn.stats {
            spawn_stat(c, st, w);
        }
    });
    for g in &pn.groups {
        let on = open.contains(&g.title);
        pill(p, icons, Icon::Mood, &format!("{} {}", if on { "v" } else { ">" }, g.title), Some(Do::Group(g.title)), "Show or hide the detail");
        if on {
            p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), flex_shrink: 0.0, ..default() }).with_children(|c| {
                for st in &g.stats {
                    spawn_stat(c, st, w);
                }
            });
        }
    }
    if !pn.members.is_empty() {
        label(p, "Members", 12.0, INK);
        for (i, m) in pn.members.iter().enumerate() {
            let base = if i % 2 == 0 { ROW_A } else { ROW_B };
            let mood = tone_color(if m.mood < 0.3 { Tone::Bad } else if m.mood < 0.55 { Tone::Warn } else { Tone::Good });
            p.spawn((
                Node { height: px(ROW_H), padding: UiRect::horizontal(px(6)), column_gap: px(8), align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween, flex_shrink: 0.0, ..default() },
                BackgroundColor(base),
                Round(8.0),
                Clickable { act: Do::Select(Selection2::Beaver(m.id)), layer: 1, scrolled: true, base, hot: ROW_HOT },
            ))
            .with_children(|r| {
                label(r, m.name.clone(), FONT, INK);
                label(r, format!("{:?}", m.stage), 11.0, DIM);
                label(r, format!("mood {:.0}%", m.mood * 100.0), 11.0, mood);
            });
        }
    }
    if !pn.costs.is_empty() {
        label(p, "Cost delivered", 12.0, INK);
        for c in &pn.costs {
            p.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }).with_children(|r| {
                icon_node(r, icons, c.icon, 15.0);
                label(r, format!("{} {:.0}/{:.0}", crate::ui::screens::good_name(c.good), c.delivered, c.needed), FONT, INK);
                bar(r, if c.needed > 0.0 { c.delivered / c.needed } else { 1.0 }, 90.0, 4.0, GOOD);
            });
        }
    }
    if !pn.slots.is_empty() {
        label(p, "Worker slots", 12.0, INK);
        for sl in &pn.slots {
            p.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(3), padding: UiRect::all(px(5)), flex_shrink: 0.0, ..default() }, BackgroundColor(ROW_A), Round(8.0)))
                .with_children(|c| {
                    c.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                        icon_node(r, icons, sl.icon, 16.0);
                        label(r, format!("{} slot {}", sl.role, sl.slot + 1), FONT, INK);
                        r.spawn(Node { flex_grow: 1.0, ..default() });
                        match (&sl.worker, &sl.empty) {
                            (Some((id, name)), Some(cmd)) => {
                                pill(r, icons, Icon::Builder, name, Some(Do::Select(Selection2::Beaver(*id))), "Open this beaver");
                                pill(r, icons, Icon::Cancel, "Empty", Some(Do::Cmd(cmd.clone())), "Free this slot");
                            }
                            _ => label(r, "free", 11.0, DIM),
                        }
                    });
                    if !sl.candidates.is_empty() {
                        c.spawn(Node { column_gap: px(4), row_gap: px(3), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|r| {
                            for cand in sl.candidates.iter().take(3) {
                                pill(r, icons, Icon::FillSlot, &format!("+ {}", cand.name), Some(Do::Cmd(cand.fill.clone())), "Put this beaver in the slot");
                            }
                        });
                    }
                });
        }
    }
    if !pn.actions.is_empty() {
        p.spawn(Node { column_gap: px(5), row_gap: px(4), flex_wrap: FlexWrap::Wrap, flex_shrink: 0.0, ..default() }).with_children(|r| {
            for a in &pn.actions {
                let tip = if a.command.is_some() { "Do this" } else { "Not possible now" };
                pill(r, icons, a.icon, &a.label, a.command.clone().map(Do::Cmd), tip);
            }
        });
    }
    if !status.is_empty() {
        label(p, status.to_string(), 11.0, INK);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn refresh_entity(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    cli: Res<crate::cli::Cli>,
    icons: Res<IconAssets>,
    mut ui: ResMut<ShellUi>,
    header: Query<Entity, With<EntityHeader>>,
    body: Query<Entity, With<EntityBody>>,
) {
    ui.panel_timer -= time.delta_secs();
    if ui.panel_timer > 0.0 && !ui.dirty {
        return;
    }
    ui.panel_timer = REFRESH_SECS;
    let panel = ui.panel.and_then(|x| sel2::panel(&sim.0, x));
    if ui.panel.is_some() && panel.is_none() {
        ui.panel = None; // the entity is gone
    }
    // Screenshots show every detail group; play starts with them folded away.
    let mut open: Vec<&'static str> = if cli.screenshot.is_some() && cli.select.is_some() {
        panel.iter().flat_map(|p| p.groups.iter().map(|g| g.title)).collect()
    } else {
        ui.groups_open.iter().copied().collect()
    };
    open.sort_unstable();
    let shown = PanelShown { sel: ui.panel, panel, open, status: ui.status.clone() };
    if ui.last_panel.as_ref() == Some(&shown) {
        return;
    }
    let same_entity = ui.last_panel.as_ref().is_some_and(|l| l.sel == shown.sel);
    if let Some(pn) = &shown.panel {
        if !same_entity && let Ok(e) = header.single() {
            commands.entity(e).despawn_children();
            commands.entity(e).with_children(|p| spawn_header(p, pn, &icons));
        }
        if let Ok(e) = body.single() {
            commands.entity(e).despawn_children();
            commands.entity(e).with_children(|p| spawn_body(p, pn, &shown.open, &shown.status, &icons));
        }
    }
    ui.last_panel = Some(shown);
}
