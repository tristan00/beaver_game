//! The job panel (`docs/UI_SPEC.md` §4): opens centred with J or from the toolbar. One row per open
//! job with its worker count and - / + buttons, then a strip of the adult beavers (idle ones in
//! amber). Click a beaver to select it; the selected beaver gets its own job buttons and its
//! family's priority order. Drawn from `hud::JobPanel`; the map stays visible behind it.
use super::hud::{job_panel, JobLine, JobPanel, SelectedBeaver};
use super::icons::{Icon, IconAssets};
use super::kit::*;
use super::panels::ShellUi;
use super::screens::{job_icon, Do};
use super::shell::HudTool;
use crate::flow::Selection;
use crate::state::Sim;
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub const PANEL_W: f32 = 520.0;

#[derive(Component)]
pub struct JobsRoot;

pub struct JobsPlugin;

impl Plugin for JobsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_root).add_systems(Update, (layout, refresh).chain());
        if app.world().resource::<crate::cli::Cli>().demo_jobs {
            app.insert_resource(crate::flow::CaptureGate(false)).init_resource::<DemoJobs>().add_systems(Update, demo_jobs.before(layout));
        }
    }
}

/// `--demo-jobs`: puts the first two adults on jobs, selects one and opens the panel, then lets
/// the capture gate open a moment later so the rows have refreshed.
#[derive(Resource, Default)]
struct DemoJobs {
    done_at: Option<f32>,
    since: f32,
}

fn demo_jobs(
    time: Res<Time>,
    flow: Res<crate::flow::Flow>,
    mut demo: ResMut<DemoJobs>,
    mut sim: ResMut<Sim>,
    mut sel: ResMut<Selection>,
    mut ui: ResMut<ShellUi>,
    mut gate: ResMut<crate::flow::CaptureGate>,
    mut exit: MessageWriter<AppExit>,
) {
    if flow.phase != crate::flow::Phase::Settled {
        return;
    }
    demo.since += time.delta_secs();
    if demo.done_at.is_none() {
        let adults: Vec<_> = sim.0.colony.beavers.iter().filter(|b| b.stage == sim::colony::Stage::Adult).map(|b| b.id).collect();
        if adults.len() < 2 {
            error!("demo-jobs: fewer than two adults");
            exit.write(AppExit::error());
            return;
        }
        for (id, job) in adults.iter().zip([sim::colony::Job::Logger, sim::colony::Job::Forager]) {
            if let Err(e) = sim.0.colony.set_job(*id, Some(job)) {
                error!("demo-jobs: {e}");
                exit.write(AppExit::error());
                return;
            }
        }
        *sel = Selection::Beaver(adults[0]);
        ui.shell.tool = Some(HudTool::Jobs);
        demo.done_at = Some(demo.since);
        info!("demo-jobs: {} -> Logger, {} -> Forager", adults[0].0, adults[1].0);
    }
    if let Some(at) = demo.done_at {
        gate.0 = demo.since - at >= 1.0;
    }
}

fn spawn_root(mut commands: Commands) {
    commands.spawn((JobsRoot, Blocker, panel(Node { position_type: PositionType::Absolute, width: px(PANEL_W), display: Display::None, ..default() }), GlobalZIndex(12)));
}

fn layout(windows: Query<&Window, With<PrimaryWindow>>, sim: Res<Sim>, ui: Res<ShellUi>, mut q: Query<(&mut Node, &ComputedNode), With<JobsRoot>>) {
    let Ok(w) = windows.single() else { return };
    for (mut n, c) in &mut q {
        let h = c.size.y * c.inverse_scale_factor;
        n.display = if sim.0.colony.is_founded() && ui.shell.tool == Some(HudTool::Jobs) { Display::Flex } else { Display::None };
        n.left = px(((w.width() - PANEL_W) / 2.0).max(8.0));
        n.top = px(((w.height() - h) / 2.0 - 40.0).max(super::kit::ZONE_TOP + 16.0));
    }
}

fn refresh(
    time: Res<Time>,
    sim: Res<Sim>,
    ui: Res<ShellUi>,
    sel: Res<Selection>,
    icons: Res<IconAssets>,
    root: Query<Entity, With<JobsRoot>>,
    mut commands: Commands,
    mut last: Local<Option<JobPanel>>,
    mut wait: Local<f32>,
    mut was_open: Local<bool>,
) {
    let open = sim.0.colony.is_founded() && ui.shell.tool == Some(HudTool::Jobs);
    if !open {
        *last = None;
        *was_open = false;
        return;
    }
    *wait -= time.delta_secs();
    if *wait > 0.0 && last.is_some() && *was_open {
        return;
    }
    *wait = 0.25;
    *was_open = true;
    let who = if let Selection::Beaver(id) = *sel { Some(id) } else { None };
    let jp = job_panel(&sim.0, who);
    if last.as_ref() == Some(&jp) {
        return;
    }
    if let Ok(e) = root.single() {
        commands.entity(e).despawn_children();
        commands.entity(e).with_children(|p| spawn_panel(p, &jp, &ui.status, &icons));
    }
    *last = Some(jp);
}

fn spawn_panel(p: &mut Kids, jp: &JobPanel, status: &str, icons: &IconAssets) {
    p.spawn(Node { align_items: AlignItems::Center, column_gap: px(GRID), ..default() }).with_children(|h| {
        icon_node(h, icons, HudTool::Jobs.icon(), 28.0);
        label_bold(h, "Jobs", T_TITLE + 2.0, PITH);
        h.spawn(Node { flex_grow: 1.0, ..default() });
        label(h, "Adults", T_BODY, DIM);
        number(h, &jp.adults, T_TITLE, PITH);
        label(h, "Idle", T_BODY, DIM);
        number(h, &jp.idle, T_TITLE, if jp.idle.value > 0.0 { AMBER } else { PITH });
        spawn_button(h, icons, KitButton::new(Icon::Cancel, "", Do::Tool(HudTool::Jobs)).layer(7).compact().tip("Close (J)"));
    });
    for (i, l) in jp.lines.iter().enumerate() {
        spawn_line(p, i, l, icons);
    }
    label(p, "Beavers", T_MIN, DIM);
    p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(GRID), row_gap: px(GRID), ..default() }).with_children(|strip| {
        for c in &jp.chips {
            let fill = if c.selected { LINE } else { RAISED };
            let doing = c.doing.map_or("idle".to_string(), |j| format!("{j:?}"));
            strip
                .spawn((
                    Node { padding: UiRect::axes(px(GRID), px(4)), column_gap: px(6), align_items: AlignItems::Center, ..default() },
                    BackgroundColor(fill),
                    Corner::Button,
                    Clickable { act: Do::Pick(c.id), layer: 7, scrolled: false, base: fill, hot: LINE },
                    Tip::note(format!("{}: {doing}. Click to select.", c.name)),
                ))
                .with_children(|chip| {
                    if let Some(j) = c.doing {
                        icon_node(chip, icons, job_icon(j), 20.0);
                    }
                    label(chip, c.name.clone(), T_MIN, if c.idle { AMBER } else { PITH });
                    if c.selected {
                        chip.spawn((Node { position_type: PositionType::Absolute, left: px(0), right: px(0), bottom: px(0), height: px(ACTIVE_BAR), ..default() }, BackgroundColor(POND)));
                    }
                });
        }
    });
    if let Some(s) = &jp.selected {
        spawn_selected(p, s, icons);
    }
    if !status.is_empty() {
        label(p, status, T_MIN, AMBER);
    }
    label(p, "Adults with no job follow their family's priorities.", T_MIN, DIM);
}

fn spawn_line(p: &mut Kids, i: usize, l: &JobLine, icons: &IconAssets) {
    p.spawn(row(i)).with_children(|r| {
        r.spawn(Node { width: px(150), column_gap: px(GRID), align_items: AlignItems::Center, ..default() }).with_children(|n| {
            icon_node(n, icons, l.icon, 24.0);
            label(n, format!("{:?}", l.job), T_BODY, PITH);
        });
        r.spawn(Node { width: px(36), ..default() }).with_children(|c| number(c, &l.assigned, T_TITLE, PITH));
        r.spawn(Node { width: px(190), ..default() }).with_children(|c| label(c, format!("assigned, {} working now", l.working), T_MIN, if l.working > 0 { MOSS } else { DIM }));
        r.spawn(Node { flex_grow: 1.0, ..default() });
        r.spawn(Node { column_gap: px(4), ..default() }).with_children(|b| {
            let mut less = KitButton::new(Icon::Cancel, "-", Do::Staff(l.job, -1)).layer(7).compact().tip("One fewer");
            less.icon = None;
            let mut more = KitButton::new(Icon::Assign, "+", Do::Staff(l.job, 1)).layer(7).compact().tip("One more");
            more.icon = None;
            if l.can_fewer {
                spawn_button(b, icons, less);
            }
            if l.can_more {
                spawn_button(b, icons, more);
            }
        });
    });
}

fn spawn_selected(p: &mut Kids, s: &SelectedBeaver, icons: &IconAssets) {
    label_bold(p, format!("{}: set job", s.name), T_BODY, PITH);
    p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), ..default() }).with_children(|r| {
        let mut opts: Vec<Option<sim::colony::Job>> = vec![None];
        opts.extend(s.options.iter().copied().map(Some));
        for o in opts {
            let kind = if s.set == o { ButtonKind::Active } else { ButtonKind::Normal };
            let mut b = KitButton::new(Icon::Assign, o.map_or("Auto".to_string(), |j| format!("{j:?}")), Do::SetJob(s.id, o)).kind(kind).layer(7).compact().tip("Set this beaver's own job");
            b.icon = None;
            spawn_button(r, icons, b);
        }
    });
    label(p, format!("{} family priorities, first is most wanted", s.family_name), T_MIN, DIM);
    p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), ..default() }).with_children(|r| {
        for (i, j) in s.priorities.iter().enumerate() {
            let mut b = KitButton::new(Icon::Assign, format!("{}. {j:?}", i + 1), Do::Promote(s.family, i)).layer(7).compact().tip(if i == 0 { "Most wanted" } else { "Move up" });
            b.icon = None;
            spawn_button(r, icons, b);
        }
    });
}
