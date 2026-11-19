use crate::hud::{HoverCell, MenuOpen, TextCapture};
use crate::state::Sim;
use bevy::prelude::*;

pub struct SandboxPlugin;

impl Plugin for SandboxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, breach_under_cursor);
    }
}

pub fn breach_line(s: &mut sim::SimState, at: (usize, usize)) -> usize {
    let cells = crate::build_logic::dam_line_at(s, at);
    cells.iter().filter(|&&(x, y)| s.apply(sim::Command::BreachDam { x, y }).is_ok()).count()
}

fn breach_under_cursor(keys: Option<Res<ButtonInput<KeyCode>>>, hover: Option<Res<HoverCell>>, typing: Option<Res<TextCapture>>, menu: Option<Res<MenuOpen>>, mut sim: ResMut<Sim>) {
    if !sim.0.sandbox || !keys.is_some_and(|k| k.just_pressed(KeyCode::Delete)) || typing.is_some_and(|t| t.0) || menu.is_some_and(|m| m.0) {
        return;
    }
    if let Some(c) = hover.and_then(|h| h.0) {
        let n = breach_line(&mut sim.0, c);
        if n > 0 {
            info!("sandbox: breached {n} dam cells at {c:?}");
        }
    }
}

