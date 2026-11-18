//! TEMPORARY STUB: same public API as the procedural icon atlas being drawn on another branch.
//! Draws every icon as a plain tinted square so the HUD can be built and tested meanwhile.
use bevy::prelude::*;

macro_rules! icons {
    ($($v:ident),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon { $($v),* }
        impl Icon {
            pub const ALL: &'static [Icon] = &[$(Icon::$v),*];
            pub fn index(self) -> usize { self as usize }
        }
    };
}

icons!(
    Food, Logs, Sticks, Mud, Stone, Baskets, Rope, Tools,
    CatBasics, CatWater, CatForest, CatBuilding, CatCraft, CatTransport, CatDefence,
    Dam, Lodge, Burrow, FoodCache, ScentMound, GuardPost, Canal, Lock, WaterWheel, Workshop, Warehouse, Dock, Barge,
    LoggingCamp, ForagerHut, MudPit, QuarryCamp, Mill, Storehouse, EldersDen,
    RoleForager, RoleLogger, RoleMudDigger, RoleQuarrier, RoleBuilder, RoleHauler, RoleCrafter, RoleElder, RoleGuard,
    ViewWorld, ViewWater, ViewGoods, ViewVegetation, ViewForest, ViewWorkAreas, ViewHousing,
    Idle, Unstaffed, Blocked, Full, NoStore, NoPower, Padlock, HandLock, Warn, Urgent, Up, Down, Dot, DotEmpty, Check, Cross,
    Pause, Play, Fast2, Fast3, Undo, Search, Pin, PriorityUp, PriorityDown, Plus, Minus, Focus, Clear, Expand, Collapse,
    Colony, Jobs, Journal, Log, Follow, Area, Haul, Menu, Back, Settings,
    Beaver, Adult, Yearling, Kit, ElderBeaver, MoodGood, MoodOk, MoodBad, Health,
    Spring, Summer, Autumn, Winter, DrySummer,
    Hourglass, Walk, Tree, Plant, MudBank, Outcrop, Hammer, Crest, Rise, Flood, Push, Drop, Site, Building, Home, Profile,
    Upstream, North, Gauge, Wheel,
);

#[derive(Resource, Clone)]
pub struct Icons {
    pub image: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
}

pub struct IconsPlugin;

impl Plugin for IconsPlugin {
    fn build(&self, app: &mut App) {
        let layout = app.world_mut().resource_mut::<Assets<TextureAtlasLayout>>().add(TextureAtlasLayout::from_grid(UVec2::splat(1), 1, 1, None, None));
        app.insert_resource(Icons { image: Handle::default(), layout });
    }
}

/// UI image node for `icon` at `px` square, tinted `color`.
pub fn icon_node(_icons: &Icons, _icon: Icon, px: f32, color: Color) -> (ImageNode, Node) {
    (ImageNode { color, ..default() }, Node { width: Val::Px(px), height: Val::Px(px), ..default() })
}
