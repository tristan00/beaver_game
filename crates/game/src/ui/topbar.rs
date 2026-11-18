//! The top strip (`docs/UI_SPEC.md` §1): Z1 clock and need meters at the left, Z2 stores ribbon in
//! the middle, Z3 time controls at the right. Each is a bark panel drawn from the view-models in
//! `hud.rs`; only the parts that changed are rebuilt, so hover state survives.
use super::hud::{clock_needs, speed_bar, stores_ribbon, ClockNeeds, SpeedButton, StoresRibbon};
use super::icons::IconAssets;
use super::kit::*;
use super::screens::{Do, Tone};
use crate::sim_driver::SimClock;
use crate::state::Sim;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use sim::events::Season;

/// Top and side margin of the strip.
pub const MARGIN: f32 = 10.0;
pub const STRIP_H: f32 = 52.0;
const METER_W: f32 = 52.0;

#[derive(Component)]
pub struct ClockRoot;
#[derive(Component)]
pub struct RibbonRoot;
#[derive(Component)]
pub struct SpeedRoot;

pub struct TopBarPlugin;

impl Plugin for TopBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_roots).add_systems(Update, (layout, refresh));
    }
}

fn spawn_roots(mut commands: Commands) {
    let strip = |side: bool| Node {
        position_type: PositionType::Absolute,
        top: px(MARGIN),
        height: px(STRIP_H),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: px(GRID * 2.0),
        padding: UiRect::axes(px(PAD), px(0)),
        display: Display::None,
        left: if side { px(MARGIN) } else { Val::Auto },
        ..default()
    };
    commands.spawn((ClockRoot, Blocker, panel_with(strip(true)), GlobalZIndex(10)));
    commands.spawn((RibbonRoot, Blocker, panel_with(strip(false)), GlobalZIndex(10)));
    commands.spawn((SpeedRoot, Blocker, panel_with(Node { right: px(MARGIN), column_gap: px(GRID), padding: UiRect::all(px(GRID)), ..strip(false) }), GlobalZIndex(10)));
}

#[allow(clippy::type_complexity)]
fn layout(
    windows: Query<&Window, With<PrimaryWindow>>,
    sim: Res<Sim>,
    clock: Query<&ComputedNode, With<ClockRoot>>,
    mut ribbon: Query<(&mut Node, &ComputedNode), (With<RibbonRoot>, Without<ClockRoot>, Without<SpeedRoot>)>,
    mut others: Query<&mut Node, Or<(With<ClockRoot>, With<SpeedRoot>)>>,
) {
    let Ok(w) = windows.single() else { return };
    // centred, but never over the clock when the window is narrow
    let clock_right = clock.single().map_or(0.0, |c| MARGIN + c.size.x * c.inverse_scale_factor + MARGIN);
    let show = if sim.0.colony.is_founded() { Display::Flex } else { Display::None };
    for mut n in &mut others {
        n.display = show;
    }
    for (mut n, c) in &mut ribbon {
        let width = c.size.x * c.inverse_scale_factor;
        n.display = show;
        n.left = px(((w.width() - width) / 2.0).max(clock_right));
    }
}

pub fn season_color(s: Season) -> Color {
    match s {
        Season::Spring => MOSS,
        Season::Summer => AMBER,
        Season::Autumn => Color::srgb(0.88, 0.52, 0.25),
        Season::Winter => POND,
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn refresh(
    time: Res<Time>,
    sim: Res<Sim>,
    clock: Res<SimClock>,
    icons: Res<IconAssets>,
    clock_root: Query<Entity, With<ClockRoot>>,
    ribbon_root: Query<Entity, With<RibbonRoot>>,
    speed_root: Query<Entity, With<SpeedRoot>>,
    mut commands: Commands,
    mut last: Local<(Option<ClockNeeds>, Option<StoresRibbon>, Option<Vec<SpeedButton>>)>,
    mut wait: Local<f32>,
) {
    if !sim.0.colony.is_founded() {
        *last = (None, None, None);
        return;
    }
    // the speed buttons answer a click at once; the sim figures refresh four times a second
    let speed = speed_bar(&clock);
    if last.2.as_ref() != Some(&speed) {
        if let Ok(e) = speed_root.single() {
            commands.entity(e).despawn_children();
            commands.entity(e).with_children(|p| spawn_speed(p, &speed, &icons));
        }
        last.2 = Some(speed);
    }
    *wait -= time.delta_secs();
    if *wait > 0.0 && last.0.is_some() {
        return;
    }
    *wait = 0.25;
    let cn = clock_needs(&sim.0);
    if last.0.as_ref() != Some(&cn) {
        if let Ok(e) = clock_root.single() {
            commands.entity(e).despawn_children();
            commands.entity(e).with_children(|p| spawn_clock(p, &cn, &icons));
        }
        last.0 = Some(cn);
    }
    let sr = stores_ribbon(&sim.0);
    if last.1.as_ref() != Some(&sr) {
        if let Ok(e) = ribbon_root.single() {
            commands.entity(e).despawn_children();
            commands.entity(e).with_children(|p| spawn_ribbon(p, &sr, &icons));
        }
        last.1 = Some(sr);
    }
}

fn spawn_clock(p: &mut Kids, c: &ClockNeeds, icons: &IconAssets) {
    p.spawn((Node { column_gap: px(GRID), align_items: AlignItems::Center, ..default() }, Tip::note("Season"))).with_children(|s| {
        s.spawn((Node { width: px(12), height: px(12), ..default() }, BackgroundColor(season_color(c.time.season))));
        label_bold(s, format!("{:?}", c.time.season), T_BODY, PITH);
    });
    p.spawn(Node { align_items: AlignItems::Center, column_gap: px(4), ..default() }).with_children(|d| {
        label(d, "Day", T_BODY, DIM);
        number(d, &c.time.day, T_TITLE, PITH);
    });
    for n in &c.needs {
        let color = match n.tone {
            Tone::Bad => BERRY,
            Tone::Warn => AMBER,
            _ => MOSS,
        };
        p.spawn((Node { flex_direction: FlexDirection::Column, align_items: AlignItems::FlexStart, row_gap: px(4), ..default() }, Tip::of(&n.num))).with_children(|m| {
            icon_node(m, icons, n.icon, 22.0);
            meter(m, n.frac, 5, METER_W, 5.0, color);
        });
    }
}

fn spawn_ribbon(p: &mut Kids, r: &StoresRibbon, icons: &IconAssets) {
    for i in &r.items {
        let color = if i.tone == Tone::Bad { BERRY } else { PITH };
        p.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|c| {
            icon_node(c, icons, i.icon, 24.0);
            number(c, &i.num, T_TITLE, color);
        });
    }
    p.spawn((Node { align_items: AlignItems::Center, ..default() }, Tip::of(&r.more))).with_children(|c| label(c, format!("+{}", r.more.parts.len()), T_BODY, DIM));
}

fn spawn_speed(p: &mut Kids, buttons: &[SpeedButton], icons: &IconAssets) {
    for b in buttons {
        let kind = if b.active { ButtonKind::Active } else { ButtonKind::Normal };
        let mut kb = KitButton::new(super::icons::Icon::Cancel, b.label, Do::Clock(b.cmd)).kind(kind).layer(5).tip(format!("{} ({})", b.label, b.key));
        kb.icon = None;
        spawn_button(p, icons, kb);
    }
}
