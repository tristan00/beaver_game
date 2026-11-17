//! Map modes (owner: `mapmodes`): each draws its data on the map and offers its actions.

use super::icons::Icon;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MapMode {
    /// Plain map, no overlay.
    #[default]
    Normal,
    /// Water depth and flow arrows.
    Water,
    /// Forest density and felling marks.
    Forest,
    /// Food plants, stores and need.
    Food,
    /// Work areas and worker slots.
    Work,
}

/// An action offered while a mode is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeAction {
    PlaceDam,
    DigCanal,
    MarkFelling,
    AssignCollectors,
    DrawArea,
    EditArea,
    FillSlots,
}

impl ModeAction {
    pub fn icon(self) -> Icon {
        match self {
            ModeAction::PlaceDam | ModeAction::DigCanal => Icon::Build,
            ModeAction::MarkFelling | ModeAction::AssignCollectors => Icon::Collect,
            ModeAction::DrawArea | ModeAction::EditArea => Icon::MoveArea,
            ModeAction::FillSlots => Icon::Assign,
        }
    }
}

impl MapMode {
    pub const ALL: [MapMode; 5] = [MapMode::Normal, MapMode::Water, MapMode::Forest, MapMode::Food, MapMode::Work];
    pub fn icon(self) -> Icon {
        match self {
            MapMode::Normal => Icon::NavColony,
            MapMode::Water => Icon::ModeWater,
            MapMode::Forest => Icon::ModeForest,
            MapMode::Food => Icon::ModeFood,
            MapMode::Work => Icon::ModeWork,
        }
    }
    /// Actions that go with the mode. (`mapmodes` task finalises the lists.)
    pub fn actions(self) -> &'static [ModeAction] {
        match self {
            MapMode::Normal => &[],
            MapMode::Water => &[ModeAction::PlaceDam, ModeAction::DigCanal],
            MapMode::Forest => &[ModeAction::MarkFelling, ModeAction::AssignCollectors],
            MapMode::Food => &[ModeAction::AssignCollectors],
            MapMode::Work => &[ModeAction::DrawArea, ModeAction::EditArea, ModeAction::FillSlots],
        }
    }
}
