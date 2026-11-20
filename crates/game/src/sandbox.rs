use crate::hud::{HoverCell, MenuOpen, TextCapture};
use crate::state::Sim;
use bevy::prelude::*;
use bevy::input::keyboard::{Key, KeyboardInput};

pub struct SandboxPlugin;

impl Plugin for SandboxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, sandbox_panel)
            .add_systems(Update, (breach_under_cursor, climate_keys, update_panel));
    }
}

#[derive(Component)]
struct SandboxStatus;

fn sandbox_panel(mut commands: Commands, sim: Res<Sim>, cli: Res<crate::cli::Cli>) {
    if !sim.0.sandbox || cli.screenshot.is_some() { return; }
    commands.spawn((
        SandboxStatus,
        Text::new("River sandbox"),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        TextColor(Color::srgb(0.94, 0.96, 0.88)),
        Node { position_type: PositionType::Absolute, right: Val::Px(18.0), bottom: Val::Px(20.0), padding: UiRect::all(Val::Px(12.0)), ..default() },
        BackgroundColor(Color::srgba(0.12, 0.20, 0.18, 0.92)),
    ));
}

fn climate_keys(keys: Res<ButtonInput<KeyCode>>, typing: Option<Res<TextCapture>>, menu: Option<Res<MenuOpen>>, mut sim: ResMut<Sim>) {
    if !sim.0.sandbox || typing.is_some_and(|t| t.0) || menu.is_some_and(|m| m.0) { return; }
    if keys.just_pressed(KeyCode::F6) {
        use sim::water::Weather::*;
        sim.0.water.weather = match sim.0.water.weather {
            Seasonal => Clear, Clear => Rain, Rain => Storm, Storm => Drought, Drought => Seasonal,
        };
    }
    if keys.just_pressed(KeyCode::F7) {
        let next = sim::calendar::Season::at(sim.0.time_days).next();
        sim::sandbox::set_season(&mut sim.0, next);
    }
}

fn update_panel(sim: Res<Sim>, canopy: Res<crate::entities_render::CanopyView>, mut labels: Query<&mut Text, With<SandboxStatus>>) {
    let s = &sim.0;
    let f = s.water.forcing();
    let value = format!("River sandbox | {}\nWeather: {} | {:.1} mm/day\nCatchment flow: {:.2}x mean\nF6 weather | F7 season | Delete breach\nF8 canopies: {}\nSpace pause | 1/2/3 speed", sim::calendar::Season::at(s.time_days).name(), s.water.weather.name(), f.rain * 1000.0, f.inflow, if canopy.0 { "hidden" } else { "shown" });
    for mut label in &mut labels {
        if label.0 != value { label.0.clone_from(&value); }
    }
}

pub fn breach_line(s: &mut sim::SimState, at: (usize, usize)) -> usize {
    let cells = crate::build_logic::dam_line_at(s, at);
    cells.iter().filter(|&&(x, y)| s.apply(sim::Command::BreachDam { x, y }).is_ok()).count()
}

fn breach_under_cursor(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut events: MessageReader<KeyboardInput>,
    hover: Option<Res<HoverCell>>,
    selected: Option<Res<crate::flow::Selection>>,
    typing: Option<Res<TextCapture>>,
    menu: Option<Res<MenuOpen>>,
    mut sim: ResMut<Sim>,
) {
    let logical_delete = events.read().any(|e| e.state == bevy::input::ButtonState::Pressed && e.logical_key == Key::Delete);
    let delete = logical_delete || keys.is_some_and(|k| k.just_pressed(KeyCode::Delete));
    if !sim.0.sandbox || !delete || typing.is_some_and(|t| t.0) || menu.is_some_and(|m| m.0) {
        return;
    }
    let at = hover.and_then(|h| h.0).filter(|c| sim.0.water.has_dam(c.0, c.1)).or_else(|| {
        selected.and_then(|sel| match *sel {
            crate::flow::Selection::Cell(c) if sim.0.water.has_dam(c.0, c.1) => Some(c),
            _ => None,
        })
    });
    if let Some(c) = at {
        let n = breach_line(&mut sim.0, c);
        if n > 0 {
            info!("sandbox: breached {n} dam cells at {c:?}");
        }
    }
}

