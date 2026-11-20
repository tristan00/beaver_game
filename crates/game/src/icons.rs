use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub const CELL: u32 = 64;
pub const COLUMNS: u32 = 16;
const MIPS: u32 = 4;

macro_rules! icons {
    ($($name:ident),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon { $($name),* }
        impl Icon {
            pub const ALL: &'static [Icon] = &[$(Icon::$name),*];
            pub fn index(self) -> usize { self as usize }
        }
    };
}

icons! {
    Food, Logs, Sticks, Mud, Baskets, Rope, Tools,
    CatBasics, CatWater, CatForest, CatBuilding, CatCraft, CatTransport, CatDefence,
    Dam, Lodge, Burrow, FoodCache, ScentMound, GuardPost, Canal, Lock, WaterWheel, Workshop,
    Warehouse, Dock, Barge, LoggingCamp, ForagerHut, MudPit, Mill, Storehouse,
    ForesterLodge, TreeNursery,
    RoleForager, RoleLogger, RoleMudDigger, RoleBuilder, RoleHauler, RoleCrafter,
    RoleGuard, RoleForester,
    ViewWorld, ViewWater, ViewGoods, ViewVegetation, ViewForest, ViewWorkAreas, ViewHousing,
    ViewElevation,
    Idle, Unstaffed, Blocked, Full, NoStore, NoPower, Padlock, HandLock, Warn, Urgent, Up, Down,
    Dot, DotEmpty, Check, Cross,
    Pause, Play, Fast2, Fast3, Undo, Search, Pin, PriorityUp, PriorityDown, Plus, Minus, Focus,
    Clear, Expand, Collapse, Colony, Jobs, Journal, Log, Follow, Area, Haul, Balance, Menu, Back, Settings,
    Beaver, Adult, Yearling, Kit, MoodGood, MoodOk, MoodBad, Health,
    Spring, Summer, Autumn, Winter, DrySummer,
    Hourglass, Walk, Tree, Plant, MudBank, Hammer, Crest, Rise, Flood, Push, Drop, Site,
    Building, Home, Profile, Upstream, North, Gauge, Wheel,
    Sapling, TreeYoung, TreeOld, Coppice,
}

#[derive(Resource, Clone)]
pub struct Icons {
    pub image: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
}

pub struct IconsPlugin;

impl Plugin for IconsPlugin {
    fn build(&self, _app: &mut App) {}

    fn finish(&self, app: &mut App) {
        let world = app.world_mut();
        let image = match world.get_resource_mut::<Assets<Image>>() {
            Some(mut images) => images.add(atlas_image()),
            None => Handle::default(),
        };
        let layout = match world.get_resource_mut::<Assets<TextureAtlasLayout>>() {
            Some(mut layouts) => layouts.add(atlas_layout()),
            None => Handle::default(),
        };
        world.insert_resource(Icons { image, layout });
    }
}

pub fn icon_node(icons: &Icons, icon: Icon, px: f32, color: Color) -> (ImageNode, Node) {
    (
        ImageNode::from_atlas_image(icons.image.clone(), TextureAtlas { layout: icons.layout.clone(), index: icon.index() })
            .with_color(color),
        Node { width: Val::Px(px), height: Val::Px(px), flex_shrink: 0.0, ..default() },
    )
}

fn atlas_rows() -> u32 {
    (Icon::ALL.len() as u32).div_ceil(COLUMNS)
}

fn atlas_layout() -> TextureAtlasLayout {
    TextureAtlasLayout::from_grid(UVec2::splat(CELL), COLUMNS, atlas_rows(), None, None)
}

fn atlas_pixels() -> (u32, u32, Vec<u8>) {
    use bevy::image::{CompressedImageFormats, ImageType};
    let image = Image::from_buffer(
        include_bytes!("../../../assets/ui/icons.png"), ImageType::Extension("png"),
        CompressedImageFormats::NONE, true, ImageSampler::Default,
        RenderAssetUsages::MAIN_WORLD,
    ).expect("embedded icon atlas must be a valid PNG");
    (image.width(), image.height(), image.data.expect("RGBA icon pixels"))
}

fn atlas_image() -> Image {
    let (w, h, base) = atlas_pixels();
    let mut data = base.clone();
    let (mut lw, mut lh, mut level) = (w, h, base);
    for _ in 1..MIPS {
        let (nw, nh) = (lw / 2, lh / 2);
        let mut next = vec![255u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let a = |dx: u32, dy: u32| level[(((2 * y + dy) * lw + 2 * x + dx) * 4 + 3) as usize] as u32;
                next[((y * nw + x) * 4 + 3) as usize] = ((a(0, 0) + a(1, 0) + a(0, 1) + a(1, 1) + 2) / 4) as u8;
            }
        }
        data.extend_from_slice(&next);
        (lw, lh, level) = (nw, nh, next);
    }
    let mut img = Image::new(
        Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![0; (w * h * 4) as usize],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = MIPS;
    img.data = Some(data);
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
    img
}
