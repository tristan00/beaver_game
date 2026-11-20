//! Minimap image updates and navigation.
use super::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::RelativeCursorPosition;

#[derive(Resource)]
pub(super) struct MinimapImage(pub(super) Handle<Image>);

pub(super) fn setup_minimap(mut commands: Commands, sim: Res<Sim>, mut images: ResMut<Assets<Image>>) {
    let n = sim.0.world.size as u32;
    let img = Image::new_fill(Extent3d { width: n, height: n, depth_or_array_layers: 1 }, TextureDimension::D2, &[30, 30, 30, 255], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    commands.insert_resource(MinimapImage(images.add(img)));
}

pub(super) fn update_minimap(time: Res<Time>, mut next: Local<f32>, sim: Res<Sim>, img: Option<Res<MinimapImage>>, mut images: ResMut<Assets<Image>>) {
    let Some(img) = img else { return };
    let now = time.elapsed_secs();
    if now < *next {
        return;
    }
    *next = now + 2.0;
    let s = &sim.0;
    let n = s.world.size;
    let maxh = s.height.as_slice().iter().copied().fold(1.0f32, f32::max);
    let mut buf = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            let h = s.height.as_slice()[i] / maxh;
            let d = s.water.depth.as_slice()[i];
            let c: [f32; 3] = if d > sim::buildings::WET_DEPTH {
                let k = (d / 3.0).clamp(0.0, 1.0).sqrt();
                [0.42 - 0.34 * k, 0.70 - 0.36 * k, 0.92 - 0.26 * k]
            } else {
                [0.30 + 0.45 * h, 0.40 + 0.38 * h, 0.26 + 0.40 * h]
            };
            let seen = (s.fog.get((x, y)) / crate::flow::EXPLORED).min(1.0);
            let fog = crate::fog_render::UNEXPLORED_COLOR;
            let c = [c[0] * seen + fog[0] * (1.0 - seen), c[1] * seen + fog[1] * (1.0 - seen), c[2] * seen + fog[2] * (1.0 - seen)];
            buf[i * 4..i * 4 + 4].copy_from_slice(&[(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255]);
        }
    }
    for b in &s.buildings.buildings {
        for (x, y) in b.cells().filter(|c| s.fog.explored(*c)) {
            if x < n && y < n {
                buf[(y * n + x) * 4..(y * n + x) * 4 + 3].copy_from_slice(&[245, 245, 245]);
            }
        }
    }
    for bv in &s.colony.beavers {
        let (x, y) = (bv.pos.0.max(0.0) as usize, bv.pos.1.max(0.0) as usize);
        if x < n && y < n {
            buf[(y * n + x) * 4..(y * n + x) * 4 + 3].copy_from_slice(&[232, 132, 58]);
        }
    }
    if let Some(mut image) = images.get_mut(&img.0) {
        image.data = Some(buf);
    }
}

pub(super) fn minimap_click(q: Query<(&Interaction, &RelativeCursorPosition, &TagComp)>, sim: Res<Sim>, mut orbit: ResMut<crate::camera::OrbitCamera>) {
    for (i, rel, tag) in &q {
        if tag.0 != Tag::Minimap || *i != Interaction::Pressed {
            continue;
        }
        let Some(n) = rel.normalized else { continue };
        let size = sim.0.world.size as f32;
        let cell = (((n.x + 0.5) * size).clamp(0.0, size - 1.0), ((n.y + 0.5) * size).clamp(0.0, size - 1.0));
        orbit.target = cell_world(&sim.0.height, cell);
    }
}
