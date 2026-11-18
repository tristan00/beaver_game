//! bevy_ui overlay: the map fills the screen, with the conventional HUD of `docs/UI_SPEC.md`
//! around it: clock and needs (top left), stores ribbon (top centre), time controls (top right),
//! alerts, inspector, toolbar with its build menu (bottom centre) and map modes (bottom left).
//! The job panel and the ledger screens open centred, one at a time; every number explains itself
//! on hover.
//!
//! Fixed zones, each its own module built from `kit`: `topbar`, `toolbar` (+ `buildmenu`),
//! `alerts`, `inspector`, `modebar`, `jobs`, and `panels` for the open ledger screen.
//!
//! `hud.rs` and `screens.rs` hold the pure view-models (tested without a window); `panels.rs`
//! draws the screens and runs clicks; `shell.rs` is the navigation state; `explain.rs` is number
//! provenance; `kit.rs` has the shared look.
#[cfg_attr(not(test), allow(dead_code))] // `audit` and `Numbers` run in tests
pub mod alerts;
pub mod buildmenu;
pub mod events;
pub mod explain;
pub mod gestures;
pub mod hud;
pub mod icons;
pub mod inspector;
pub mod jobs;
pub mod kit;
#[cfg(test)]
mod kit_tests;
pub mod mapmodes;
pub mod modebar;
mod panels;
pub mod screens;
#[allow(dead_code)] // stub until its owner task lands
pub mod selection;
pub mod shell;
pub mod toolbar;
pub mod topbar;
pub use panels::ShellUi;

/// Recent happenings shown as toasts; fed from the colony log by `flow::watch_collapse`.
#[derive(Resource, Default)]
pub struct NoticesRes(pub events::Notices);

use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NoticesRes>().add_systems(Update, (events::stage_demo, events::draw_pins)).add_plugins((kit::KitPlugin, panels::PanelsPlugin, topbar::TopBarPlugin, toolbar::ToolbarPlugin, jobs::JobsPlugin, alerts::AlertsPlugin, inspector::InspectorPlugin, icons::IconsPlugin, modebar::ModeBarPlugin))
            ;
    }
}
