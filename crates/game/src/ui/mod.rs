//! bevy_ui overlay: the map fills the screen, with one stable navigation bar and the time
//! controls. The bar opens one screen at a time (Colony, Work, Stores, Buildings, Tech, Events);
//! every number on a screen explains itself on hover.
//!
//! `screens.rs` holds the pure view-models (tested without a window); `panels.rs` draws them and
//! runs clicks; `shell.rs` is the navigation state; `explain.rs` is number provenance; `kit.rs`
//! has the shared look.
#[cfg_attr(not(test), allow(dead_code))] // `audit` and `Numbers` run in tests
pub mod events;
pub mod explain;
pub mod icons;
pub mod menu;
mod kit;
#[allow(dead_code)] // stub until its owner task lands
pub mod mapmodes;
#[allow(dead_code)] // stub until its owner task lands
pub mod orders;
mod panels;
pub mod screens;
#[allow(dead_code)] // stub until its owner task lands
pub mod selection;
#[allow(dead_code)] // start menu is wired by the start tasks
pub mod shell;
pub use panels::ShellUi;

/// Recent happenings shown as toasts; fed from the colony log by `flow::watch_collapse`.
#[derive(Resource, Default)]
pub struct NoticesRes(pub events::Notices);

use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NoticesRes>().add_systems(Update, (events::stage_demo, events::draw_pins)).add_plugins((panels::PanelsPlugin, icons::IconsPlugin))
            .add_systems(PostUpdate, kit::apply_round.before(bevy::ui::UiSystems::Prepare));
    }
}
