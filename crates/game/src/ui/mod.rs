//! bevy_ui overlay in a soft, rounded style: a top bar (date, season, population, mood, speed),
//! four compact ledgers beside the map (Families, Jobs, Stores, Trade) that open into detail
//! tables, and event cards with a filterable, scrollable event log.
//!
//! `ledgers.rs` and `events.rs` hold the pure view-models (tested without a window); `panels.rs`
//! draws them and runs clicks; `kit.rs` has the shared look.
pub mod events;
#[allow(dead_code)] // stub until its owner task lands
pub mod explain;
pub mod icons;
pub mod menu;
mod kit;
pub mod ledgers;
#[allow(dead_code)] // stub until its owner task lands
pub mod mapmodes;
#[allow(dead_code)] // stub until its owner task lands
pub mod orders;
mod panels;
#[allow(dead_code)] // stub until its owner task lands
pub mod selection;
#[allow(dead_code)] // stub until its owner task lands
pub mod shell;
pub use panels::LedgerUi;

use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((panels::PanelsPlugin, events::EventsPlugin, icons::IconsPlugin))
            .add_systems(PostUpdate, kit::apply_round.before(bevy::ui::UiSystems::Prepare));
    }
}
