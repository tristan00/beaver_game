//! Build bar zone (bottom, centred): one button per screen. The open screen carries the Pond
//! bar; a count that wants attention sits on the button as a badge that explains itself on hover.
use super::icons::IconAssets;
use super::kit::*;
use super::screens::{nav_bar, Do, NavBar};
use crate::state::Sim;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::panels::ShellUi;

#[derive(Component)]
pub struct BuildBarRoot;

pub struct BuildBarPlugin;

impl Plugin for BuildBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_root).add_systems(Update, (layout, refresh));
    }
}

fn spawn_root(mut commands: Commands) {
    commands.spawn((
        BuildBarRoot,
        Blocker,
        panel_with(Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(GRID),
            padding: UiRect::all(px(GRID)),
            ..default()
        }),
        GlobalZIndex(10),
    ));
}

fn layout(windows: Query<&Window, With<PrimaryWindow>>, sim: Res<Sim>, mut q: Query<(&mut Node, &ComputedNode), With<BuildBarRoot>>) {
    let Ok(w) = windows.single() else { return };
    for (mut n, c) in &mut q {
        let width = c.size.x * c.inverse_scale_factor;
        n.display = if sim.0.colony.is_founded() { Display::Flex } else { Display::None };
        n.left = px(((w.width() - width) / 2.0).max(8.0));
        n.bottom = px(12);
    }
}

fn refresh(
    time: Res<Time>,
    sim: Res<Sim>,
    ui: Res<ShellUi>,
    icons: Res<IconAssets>,
    root: Query<Entity, With<BuildBarRoot>>,
    mut commands: Commands,
    mut last: Local<Option<NavBar>>,
    mut wait: Local<f32>,
    mut open: Local<Option<super::shell::NavItem>>,
) {
    if !sim.0.colony.is_founded() {
        *last = None;
        return;
    }
    *wait -= time.delta_secs();
    if *wait > 0.0 && last.is_some() && *open == ui.shell.open {
        return;
    }
    *wait = 0.25;
    *open = ui.shell.open;
    let nb = nav_bar(&sim.0, ui.shell.open);
    if last.as_ref() == Some(&nb) {
        return;
    }
    if let Ok(e) = root.single() {
        commands.entity(e).despawn_children();
        commands.entity(e).with_children(|p| spawn_bar(p, &nb, &icons));
    }
    *last = Some(nb);
}

pub fn spawn_bar(p: &mut Kids, nav: &NavBar, icons: &IconAssets) {
    for e in &nav.entries {
        let kind = if e.open { ButtonKind::Active } else { ButtonKind::Normal };
        let tip = if e.open { format!("{} (click to close)", e.label) } else { e.label.to_string() };
        let id = spawn_button(p, icons, KitButton::new(e.icon, e.label, Do::Nav(e.item)).kind(kind).layer(5).tip(tip));
        if let Some(n) = &e.badge {
            p.commands().entity(id).with_children(|b| {
                b.spawn((
                    Node { position_type: PositionType::Absolute, top: px(-6), right: px(-6), min_width: px(22), height: px(22), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                    BackgroundColor(AMBER),
                    Tip::of(n),
                ))
                .with_children(|c| label_bold(c, n.show(), T_MIN, RAIL));
            });
        }
    }
}
