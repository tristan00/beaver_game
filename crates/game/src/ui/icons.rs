//! Icon identities and their loader. One enum variant per icon file under `assets/icons/`;
//! `file()` is the asset path. `IconsPlugin` loads every icon through the `AssetServer` at startup
//! into [`IconAssets`], so UI code asks for `icons.get(Icon::Logs)` and gets a `Handle<Image>`.
//! Text labels are used only where an icon would be ambiguous.

use bevy::asset::LoadState;
use bevy::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    // resources
    Logs,
    Branches,
    Bark,
    Plants,
    Mud,
    Stone,
    Baskets,
    Rope,
    Tools,
    // needs
    Food,
    Shelter,
    Rest,
    Mood,
    Social,
    // jobs (docs/UI_SPEC.md: Collector, Builder, Hauler, Specialist)
    Collector,
    Builder,
    Hauler,
    Specialist,
    // job verbs used by order buttons
    Collect,
    Carry,
    Build,
    Haul,
    Craft,
    // actions
    Assign,
    FillSlot,
    MoveArea,
    Accept,
    TurnAway,
    Cancel,
    Demolish,
    // map modes
    ModeWater,
    ModeForest,
    ModeFood,
    ModeWork,
    // navigation bar
    NavColony,
    NavBeavers,
    NavStores,
    NavBuild,
    NavTech,
    NavEvents,
}

impl Icon {
    /// Every icon, in sheet order.
    pub const ALL: [Icon; 40] = [
        Icon::Logs, Icon::Branches, Icon::Bark, Icon::Plants, Icon::Mud, Icon::Stone, Icon::Baskets, Icon::Rope, Icon::Tools,
        Icon::Food, Icon::Shelter, Icon::Rest, Icon::Mood, Icon::Social,
        Icon::Collector, Icon::Builder, Icon::Hauler, Icon::Specialist,
        Icon::Collect, Icon::Carry, Icon::Build, Icon::Haul, Icon::Craft,
        Icon::Assign, Icon::FillSlot, Icon::MoveArea, Icon::Accept, Icon::TurnAway, Icon::Cancel, Icon::Demolish,
        Icon::ModeWater, Icon::ModeForest, Icon::ModeFood, Icon::ModeWork,
        Icon::NavColony, Icon::NavBeavers, Icon::NavStores, Icon::NavBuild, Icon::NavTech, Icon::NavEvents,
    ];

    /// The icons listed in `docs/UI_SPEC.md` section 6.
    pub const STYLE_GUIDE: [Icon; 24] = [
        Icon::Logs, Icon::Branches, Icon::Bark, Icon::Plants, Icon::Mud, Icon::Stone,
        Icon::Food, Icon::Shelter, Icon::Rest, Icon::Mood,
        Icon::Collector, Icon::Builder, Icon::Hauler, Icon::Specialist,
        Icon::Assign, Icon::Build, Icon::FillSlot, Icon::MoveArea, Icon::Accept, Icon::TurnAway,
        Icon::ModeWater, Icon::ModeForest, Icon::ModeFood, Icon::ModeWork,
    ];

    /// Asset path relative to the asset root, e.g. `icons/logs.png`.
    pub fn file(self) -> String {
        format!("icons/{}.png", format!("{self:?}").to_lowercase())
    }
}

impl From<sim::buildings::Good> for Icon {
    fn from(g: sim::buildings::Good) -> Icon {
        use sim::buildings::Good::*;
        match g {
            Logs => Icon::Logs,
            Sticks => Icon::Branches,
            Mud => Icon::Mud,
            Stone => Icon::Stone,
            Food => Icon::Food,
            Baskets => Icon::Baskets,
            Rope => Icon::Rope,
            Tools => Icon::Tools,
        }
    }
}

/// The repository's `assets/` folder. Bevy's default root is the crate folder under `cargo run`
/// (`crates/game/assets`, which does not exist) or the executable's folder, so `main` passes this
/// to `AssetPlugin::file_path`. Order: `BEAVER_ASSETS`, `assets/` beside the executable (a shipped
/// build), then the workspace `assets/` this crate was built from.
pub fn asset_root() -> PathBuf {
    if let Ok(dir) = std::env::var("BEAVER_ASSETS") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("assets"))) {
        if dir.join("icons").is_dir() {
            return dir;
        }
    }
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets"))
}

/// Handles to every icon image, filled at startup.
#[derive(Resource, Default)]
pub struct IconAssets {
    handles: HashMap<Icon, Handle<Image>>,
    reported: bool,
}

impl IconAssets {
    pub fn get(&self, icon: Icon) -> Handle<Image> {
        self.handles.get(&icon).cloned().unwrap_or_default()
    }
}

pub struct IconsPlugin;

impl Plugin for IconsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IconAssets>()
            .add_systems(Startup, load_icons)
            .add_systems(Update, report_icons.run_if(|i: Res<IconAssets>| !i.reported));
    }
}

pub fn load_icons(server: Res<AssetServer>, mut icons: ResMut<IconAssets>) {
    for icon in Icon::ALL {
        icons.handles.insert(icon, server.load(icon.file()));
    }
}

/// Logs once when every icon has loaded, or names each one that failed.
pub fn report_icons(server: Res<AssetServer>, mut icons: ResMut<IconAssets>) {
    let mut failed = Vec::new();
    for icon in Icon::ALL {
        match server.load_state(&icons.get(icon)) {
            LoadState::Loaded => {}
            LoadState::Failed(e) => failed.push(format!("{} ({e})", icon.file())),
            _ => return, // still loading
        }
    }
    icons.reported = true;
    if failed.is_empty() {
        info!("icons: all {} loaded from {}", Icon::ALL.len(), asset_root().display());
    } else {
        error!("icons: {} of {} failed to load: {}", failed.len(), Icon::ALL.len(), failed.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Loads every icon through a real `AssetServer` rooted where the game looks, and checks each
    /// one reaches `Loaded` as an `Image` with the 96 px tile size.
    #[test]
    fn every_icon_loads_through_the_asset_server() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin { file_path: asset_root().to_string_lossy().into_owned(), ..default() },
            bevy::image::ImagePlugin::default(),
            IconsPlugin,
        ));
        // In the game the render plugin registers the PNG loader; without a GPU the test does it.
        app.register_asset_loader(bevy::image::ImageLoader::new(bevy::image::CompressedImageFormats::NONE));
        for _ in 0..2000 {
            app.update();
            if app.world().resource::<IconAssets>().reported {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let server = app.world().resource::<AssetServer>();
        let icons = app.world().resource::<IconAssets>();
        let images = app.world().resource::<Assets<Image>>();
        for icon in Icon::ALL {
            let h = icons.get(icon);
            assert!(matches!(server.load_state(&h), LoadState::Loaded), "{} did not load: {:?}", icon.file(), server.load_state(&h));
            let img = images.get(&h).unwrap_or_else(|| panic!("{} has no Image", icon.file()));
            assert_eq!((img.width(), img.height()), (96, 96), "{}", icon.file());
        }
    }

    #[test]
    fn style_guide_icons_are_all_in_the_loaded_set() {
        for icon in Icon::STYLE_GUIDE {
            assert!(Icon::ALL.contains(&icon), "{icon:?}");
        }
        let mut files: Vec<_> = Icon::ALL.iter().map(|i| i.file()).collect();
        files.sort();
        files.dedup();
        assert_eq!(files.len(), Icon::ALL.len());
    }
}
