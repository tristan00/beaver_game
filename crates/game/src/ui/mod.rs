//! bevy_ui overlay: the map fills the screen, with one stable navigation bar and the time
//! controls. The bar opens one screen at a time (Colony, Work, Stores, Buildings, Tech, Events);
//! every number on a screen explains itself on hover.
//!
//! Fixed zones, each its own module built from `kit`: `status` (top), `buildbar` (bottom), `alerts`,
//! `inspector`, `modebar`, and `panels` for the open screen.
//!
//! `screens.rs` holds the pure view-models (tested without a window); `panels.rs` draws them and
//! runs clicks; `shell.rs` is the navigation state; `explain.rs` is number provenance; `kit.rs`
//! has the shared look.
#[cfg_attr(not(test), allow(dead_code))] // `audit` and `Numbers` run in tests
pub mod alerts;
pub mod buildbar;
pub mod events;
pub mod explain;
pub mod gestures;
pub mod icons;
pub mod inspector;
pub mod kit;
#[cfg(test)]
mod kit_tests;
pub mod mapmodes;
pub mod modebar;
#[allow(dead_code)] // stub until its owner task lands
pub mod orders;
mod panels;
pub mod screens;
#[allow(dead_code)] // stub until its owner task lands
pub mod selection;
pub mod shell;
pub mod status;
pub use panels::ShellUi;

/// Recent happenings shown as toasts; fed from the colony log by `flow::watch_collapse`.
#[derive(Resource, Default)]
pub struct NoticesRes(pub events::Notices);

use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NoticesRes>().add_systems(Update, (events::stage_demo, events::draw_pins)).add_plugins((kit::KitPlugin, panels::PanelsPlugin, status::StatusPlugin, buildbar::BuildBarPlugin, alerts::AlertsPlugin, inspector::InspectorPlugin, icons::IconsPlugin, modebar::ModeBarPlugin))
            ;
    }
}
