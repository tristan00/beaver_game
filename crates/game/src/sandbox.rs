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

fn breach_under_cursor(keys: Res<ButtonInput<KeyCode>>, hover: Res<HoverCell>, typing: Option<Res<TextCapture>>, menu: Option<Res<MenuOpen>>, mut sim: ResMut<Sim>) {
    if !sim.0.sandbox || !keys.just_pressed(KeyCode::Delete) || typing.is_some_and(|t| t.0) || menu.is_some_and(|m| m.0) {
        return;
    }
    if let Some(c) = hover.0 {
        let n = breach_line(&mut sim.0, c);
        if n > 0 {
            info!("sandbox: breached {n} dam cells at {c:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_logic::{emit, line_cells, BuildOrder, Pending};

    #[test]
    fn in_the_sandbox_the_dam_and_canal_tools_build_at_once_and_delete_breaches() {
        let mut s = sim::sandbox::on_map("fixture").unwrap();
        let mut p = Pending::default();
        let dam = line_cells((60, 50), (60, 78));
        emit(&mut s, &mut p, &BuildOrder::Dam { cells: dam.clone(), height: 1.5 }).unwrap();
        assert!(dam.iter().all(|&(x, y)| s.water.has_dam(x, y)) && s.buildings.buildings.is_empty());
        let canal = line_cells((20, 40), (30, 40));
        let before = *s.height.get(25, 40).unwrap();
        emit(&mut s, &mut p, &BuildOrder::Canal { cells: canal }).unwrap();
        assert!((before - s.height.get(25, 40).unwrap() - sim::SimState::CANAL_DEPTH).abs() < 1e-5);
        assert_eq!(breach_line(&mut s, (60, 60)), dam.len());
        assert!(dam.iter().all(|&(x, y)| !s.water.has_dam(x, y)));
    }
}
