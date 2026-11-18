//! The settle choice (`docs/UI_SPEC.md` §5, row 1): the game opens paused on the valley with three
//! candidate banks outlined. A click on usable ground picks it and fills the card (wood, forage,
//! flood risk); **Settle here** (or Enter) sends `Command::Settle`; **Cancel** drops the pick.
//! Nothing is founded, built or gathered until then.
//!
//! The logic (`candidates`, `SettleUi`) is pure and tested without a window; the plugin draws the
//! outlines, the names and the card, and feeds clicks in (`flow::pick_world` calls `SettleUi::click`).
use super::icons::{Icon, IconAssets};
use super::kit::*;
use super::panels::ShellUi;
use super::screens::Do;
use super::shell::HudTool;
use crate::camera::{MainCamera, OrbitCamera};
use crate::flow::{can_settle, cell_world, site_hint, Cell, Flow, Fog, Phase};
use crate::sim_driver::SimClock;
use crate::state::Sim;
use bevy::prelude::*;
use sim::resources::ResourceKind;
use sim::{Command, SimState};

/// Candidate banks are at least this many cells apart.
const SPACING: f32 = 14.0;
/// Radius (cells) in which forage is counted.
const FORAGE_RADIUS: i32 = 7;
/// Forage items in that radius that fill the meter.
const FORAGE_FULL: f32 = 10.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub cell: Cell,
    pub name: &'static str,
}

/// Wood, forage and flood risk of a place, each 0..=1 (flood: 1 is the riskiest).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Facts {
    pub wood: f32,
    pub forage: f32,
    pub flood: f32,
}

impl Facts {
    pub fn flood_word(&self) -> &'static str {
        match self.flood {
            f if f < 0.35 => "low",
            f if f < 0.65 => "medium",
            _ => "high",
        }
    }
}

pub fn facts(sim: &SimState, cell: Cell) -> Facts {
    let h = site_hint(&sim.world, cell);
    let r2 = FORAGE_RADIUS * FORAGE_RADIUS;
    let near = |c: Cell| {
        let (dx, dy) = (c.0 as i32 - cell.0 as i32, c.1 as i32 - cell.1 as i32);
        dx * dx + dy * dy <= r2
    };
    let plants = sim.resources.items.iter().filter(|r| r.kind == ResourceKind::Plant && r.has_yield() && near(r.cell)).count();
    Facts { wood: h.trees, forage: (plants as f32 / FORAGE_FULL).clamp(0.0, 1.0), flood: 1.0 - h.safety }
}

/// Why `cell` cannot be settled, or `Ok` (the game's own check: open valley, explored, dry).
pub fn usable(sim: &SimState, fog: &Fog, cell: Cell) -> Result<(), String> {
    if sim.colony.is_founded() {
        return Err("The family has already settled".into());
    }
    can_settle(&sim.world, &sim.water.depth, fog, cell).map_err(|e| e.message().to_string())
}

/// Three candidate banks in the start valley, far apart: the best timber, the best forage and the
/// safest ground, each by the water nearby too (a dam needs a river).
pub fn candidates(sim: &SimState, fog: &Fog) -> Vec<Candidate> {
    let usable_cells: Vec<(Cell, Facts, f32)> = (0..sim.world.size)
        .step_by(2)
        .flat_map(|y| (0..sim.world.size).step_by(2).map(move |x| (x, y)))
        .filter(|&c| sim.world.valley_of(c) == Some(sim.world.start) && usable(sim, fog, c).is_ok())
        .map(|c| (c, facts(sim, c), site_hint(&sim.world, c).water))
        .filter(|(_, _, water)| *water >= 0.5)
        .collect();
    let picks: [(&'static str, fn(&Facts) -> f32); 3] = [("Timber bank", |f| f.wood), ("Meadow bank", |f| f.forage), ("High bank", |f| 1.0 - f.flood)];
    let mut out: Vec<Candidate> = Vec::new();
    for (name, score) in picks {
        let far = |c: Cell| out.iter().all(|o| (((o.cell.0 as f32 - c.0 as f32).powi(2) + (o.cell.1 as f32 - c.1 as f32).powi(2)).sqrt()) >= SPACING);
        let best = usable_cells.iter().filter(|(c, _, _)| far(*c)).max_by(|a, b| (score(&a.1) + 0.5 * a.2).total_cmp(&(score(&b.1) + 0.5 * b.2)).then(b.0.cmp(&a.0)));
        if let Some((cell, _, _)) = best {
            out.push(Candidate { cell: *cell, name });
        }
    }
    out
}

/// The settle choice as state: which cell the player has picked, and why a click did nothing.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct SettleUi {
    pub chosen: Option<Cell>,
    pub message: String,
}

impl SettleUi {
    /// A click on `cell`: a usable cell becomes the pick; an unusable one keeps the old pick and
    /// says why. Returns whether the cell was taken.
    pub fn click(&mut self, sim: &SimState, fog: &Fog, cell: Cell) -> bool {
        match usable(sim, fog, cell) {
            Ok(()) => {
                self.chosen = Some(cell);
                self.message.clear();
                true
            }
            Err(why) => {
                self.message = why;
                false
            }
        }
    }

    /// **Settle here**: the command for the picked cell, if the pick is still usable. Clears the pick.
    pub fn confirm(&mut self, sim: &SimState, fog: &Fog) -> Option<Command> {
        let cell = self.chosen.take()?;
        match usable(sim, fog, cell) {
            Ok(()) => Some(Command::Settle { cell }),
            Err(why) => {
                self.message = why;
                None
            }
        }
    }

    /// **Cancel**: drop the pick so another can be made.
    pub fn cancel(&mut self) {
        self.chosen = None;
        self.message.clear();
    }
}

/// Runs the card's buttons and the Enter key: founds the colony and starts the clock, opening the
/// Build menu so the first thing offered is the burrow.
pub fn run(go: bool, ui: &mut SettleUi, sim: &mut Sim, flow: &mut Flow, clock: &mut SimClock, shell: &mut ShellUi) {
    if flow.phase != Phase::Scouting {
        return;
    }
    if !go {
        ui.cancel();
        return;
    }
    let Some(cmd) = ui.confirm(&sim.0, &flow.fog) else { return };
    let Command::Settle { cell } = cmd.clone() else { return };
    match sim.0.apply(cmd) {
        Ok(()) => {
            crate::flow::after_settle(&sim.0, flow, cell);
            clock.paused = false;
            shell.shell.tool = Some(HudTool::Build);
            shell.shell.build_tab = None;
        }
        Err(e) => ui.message = e.to_string(),
    }
}

// ───────────────────────────── drawing ─────────────────────────────

#[derive(Component)]
struct CardRoot;
#[derive(Component)]
struct NameTag(usize);

#[derive(Default, Reflect, GizmoConfigGroup)]
struct SettleGizmos;

pub struct SettlePlugin;

impl Plugin for SettlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SettleUi>().init_resource::<Offered>().init_gizmo_group::<SettleGizmos>().add_systems(Startup, spawn_roots).add_systems(
            Update,
            (offer, enter_key, draw_outlines, name_tags, card).chain(),
        );
    }
}

fn spawn_roots(mut commands: Commands, mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<SettleGizmos>();
    config.line.width = 4.0;
    config.depth_bias = -1.0;
    commands.spawn((CardRoot, Blocker, panel(Node { position_type: PositionType::Absolute, width: px(CARD_W), display: Display::None, ..default() }), GlobalZIndex(10)));
    for i in 0..3 {
        commands.spawn((
            NameTag(i),
            Text::new(""),
            font_bold(T_BODY),
            TextColor(PITH),
            Node { position_type: PositionType::Absolute, display: Display::None, ..default() },
            GlobalZIndex(5),
        ));
    }
}

const CARD_W: f32 = 440.0;

/// The candidates on offer, found once the choice opens.
#[derive(Resource, Default)]
struct Offered(Vec<Candidate>);

fn offer(sim: Res<Sim>, flow: Res<Flow>, mut offered: ResMut<Offered>) {
    if flow.phase == Phase::Scouting && offered.0.is_empty() {
        offered.0 = candidates(&sim.0, &flow.fog);
    }
    if flow.phase != Phase::Scouting {
        offered.0.clear();
    }
}

fn enter_key(keys: Res<ButtonInput<KeyCode>>, mut ui: ResMut<SettleUi>, mut sim: ResMut<Sim>, mut flow: ResMut<Flow>, mut clock: ResMut<SimClock>, mut shell: ResMut<ShellUi>) {
    if keys.just_pressed(KeyCode::Enter) {
        run(true, &mut ui, &mut sim, &mut flow, &mut clock, &mut shell);
    }
    if keys.just_pressed(KeyCode::Escape) && ui.chosen.is_some() {
        ui.cancel();
    }
}

fn draw_outlines(mut gizmos: Gizmos<SettleGizmos>, sim: Res<Sim>, flow: Res<Flow>, offered: Res<Offered>, ui: Res<SettleUi>) {
    if flow.phase != Phase::Scouting {
        return;
    }
    let flat = |c: Cell, half: f32| {
        let p = cell_world(&sim.0.height, (c.0 as f32 + 0.5, c.1 as f32 + 0.5)) + Vec3::Y * 0.3;
        (Isometry3d::new(p, Quat::from_rotation_arc(Vec3::Z, Vec3::Y)), Vec2::splat(half * 2.0))
    };
    for c in &offered.0 {
        let (iso, size) = flat(c.cell, 3.0);
        gizmos.rect(iso, size, if ui.chosen == Some(c.cell) { POND } else { PITH });
    }
    if let Some(c) = ui.chosen {
        let (iso, size) = flat(c, 1.5);
        gizmos.rect(iso, size, POND);
        let (iso, size) = flat(c, 1.8);
        gizmos.rect(iso, size, POND);
    }
}

fn name_tags(
    sim: Res<Sim>,
    flow: Res<Flow>,
    offered: Res<Offered>,
    ui: Res<SettleUi>,
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut tags: Query<(&NameTag, &mut Text, &mut Node, &mut TextColor)>,
) {
    let Ok((camera, gt)) = cams.single() else { return };
    for (tag, mut text, mut node, mut color) in &mut tags {
        let cand = offered.0.get(tag.0).filter(|_| flow.phase == Phase::Scouting);
        let screen = cand.and_then(|c| camera.world_to_viewport(gt, cell_world(&sim.0.height, (c.cell.0 as f32 + 0.5, c.cell.1 as f32 + 0.5)) + Vec3::Y * 3.0).ok());
        match (cand, screen) {
            (Some(c), Some(p)) => {
                node.display = Display::Flex;
                node.left = px(p.x - 40.0);
                node.top = px(p.y - 28.0);
                if text.0 != c.name {
                    text.0 = c.name.to_string();
                }
                color.0 = if ui.chosen == Some(c.cell) { POND } else { PITH };
            }
            _ => node.display = Display::None,
        }
    }
}

fn card(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    sim: Res<Sim>,
    flow: Res<Flow>,
    offered: Res<Offered>,
    ui: Res<SettleUi>,
    icons: Res<IconAssets>,
    mut root: Query<(Entity, &mut Node, &ComputedNode), With<CardRoot>>,
    mut commands: Commands,
    mut last: Local<Option<(Phase, SettleUi)>>,
) {
    let Ok(w) = windows.single() else { return };
    let Ok((e, mut node, c)) = root.single_mut() else { return };
    let open = flow.phase == Phase::Scouting;
    node.display = if open { Display::Flex } else { Display::None };
    node.left = px(((w.width() - c.size.x * c.inverse_scale_factor) / 2.0).max(8.0));
    node.bottom = px(12);
    let now = (flow.phase, ui.clone());
    if !open || last.as_ref() == Some(&now) {
        return;
    }
    *last = Some(now);
    let name = ui.chosen.and_then(|c| offered.0.iter().find(|o| o.cell == c)).map(|o| o.name);
    commands.entity(e).despawn_children();
    commands.entity(e).with_children(|p| {
        label_bold(p, "Where will the family settle?", T_TITLE, PITH);
        let Some(cell) = ui.chosen else {
            label(p, "Click dry ground by the river. Outlined banks are good places.", T_BODY, DIM);
            if !ui.message.is_empty() {
                label(p, ui.message.clone(), T_BODY, AMBER);
            }
            return;
        };
        let f = facts(&sim.0, cell);
        label(p, format!("{} at ({}, {}). Flood risk {}.", name.unwrap_or("Chosen ground"), cell.0, cell.1, f.flood_word()), T_BODY, DIM);
        for (i, (what, frac, color)) in [("Wood", f.wood, MOSS), ("Forage", f.forage, MOSS), ("Flood risk", f.flood, if f.flood >= 0.65 { BERRY } else if f.flood >= 0.35 { AMBER } else { MOSS })].into_iter().enumerate() {
            p.spawn((row(i), Node { align_items: AlignItems::Center, padding: UiRect::axes(px(GRID), px(0)), height: px(ROW_H - 8.0), ..default() })).with_children(|r| {
                r.spawn(Node { width: px(120), ..default() }).with_children(|n| label(n, what, T_BODY, PITH));
                meter(r, frac, 8, 200.0, 14.0, color);
            });
        }
        p.spawn(Node { column_gap: px(GRID), margin: UiRect::top(px(GRID)), ..default() }).with_children(|b| {
            spawn_button(b, &icons, KitButton::new(Icon::Build, "Settle here", Do::Settle(true)).kind(ButtonKind::Primary).tip("Settle here (Enter)"));
            spawn_button(b, &icons, KitButton::new(Icon::Cancel, "Cancel", Do::Settle(false)).tip("Pick another place (Esc)"));
        });
    });
}

/// Opens the camera on the valley so every candidate bank is in view.
pub fn frame_valley(sim: &SimState, orbit: &mut OrbitCamera) {
    let (cx, cy) = sim.world.valleys[sim.world.start.0 as usize].center;
    orbit.target = cell_world(&sim.height, (cx as f32, cy as f32));
    orbit.distance = 110.0;
    orbit.pitch = 0.9;
}

#[cfg(test)]
#[path = "settle_tests.rs"]
mod tests;
