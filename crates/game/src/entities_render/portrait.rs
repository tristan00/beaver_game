use bevy::app::{HierarchyPropagatePlugin, Propagate};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::RenderTarget;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::render_resource::{Face, TextureFormat};
use bevy::transform::TransformSystems;

use super::scene::Pose;
use super::sim_sync::{look_of, BodySprite, BEAVER_SIZE};
use super::sim_view::stand_y;
use super::{age_scale_days, looks_size, BeaverSprite, Look, SimBeaver};

#[derive(Resource)]
pub struct PortraitImage(pub Handle<Image>);

#[derive(Resource, Default)]
pub struct PortraitFocus(pub Option<u64>);

#[derive(Component)]
struct PortraitCam;

#[derive(Component)]
struct StandIn;

const LAYER: usize = 6;
pub const WIDTH: u32 = 720;
pub const HEIGHT: u32 = 480;
const AT: Vec3 = Vec3::new(0.0, -4000.0, 0.0);
const DEN_DARK: Color = Color::srgb(0.12, 0.09, 0.07);

pub struct PortraitPlugin;

impl Plugin for PortraitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PortraitFocus>()
            .add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(PostUpdate))
            .add_systems(Startup, setup)
            .add_systems(Update, choose)
            .add_systems(PostUpdate, aim.before(TransformSystems::Propagate));
    }
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let image = images.add(Image::new_target_texture(WIDTH, HEIGHT, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb)));
    commands.spawn((
        PortraitCam,
        Camera3d::default(),
        Camera { order: -5, is_active: false, ..default() },
        Tonemapping::SomewhatBoringDisplayTransform,
        RenderTarget::Image(image.clone().into()),
        Transform::from_translation(AT + Vec3::new(1.0, 1.0, 2.0)).looking_at(AT, Vec3::Y),
    ));
    let layer = RenderLayers::layer(LAYER);
    let mud = |c: Color| StandardMaterial { base_color: c, perceptual_roughness: 1.0, cull_mode: None::<Face>, ..default() };
    commands.spawn((Mesh3d(meshes.add(Cylinder::new(3.0, 0.1))), MeshMaterial3d(mats.add(mud(Color::srgb(0.36, 0.26, 0.18)))), Transform::from_translation(AT - Vec3::Y * 0.05), layer.clone()));
    commands.spawn((Mesh3d(meshes.add(Sphere::new(2.6))), MeshMaterial3d(mats.add(mud(Color::srgb(0.27, 0.19, 0.13)))), Transform::from_translation(AT).with_scale(Vec3::new(1.0, 0.7, 1.0)), layer.clone()));
    commands.spawn((
        PointLight { color: Color::srgb(1.0, 0.82, 0.6), intensity: 60_000.0, range: 8.0, ..default() },
        Transform::from_translation(AT + Vec3::new(0.6, 1.3, 1.1)),
        layer,
    ));
    commands.insert_resource(PortraitImage(image));
}

#[allow(clippy::too_many_arguments)]
fn choose(
    mut commands: Commands,
    hud: Option<Res<crate::hud::HudState>>,
    sim: Res<crate::state::Sim>,
    mut focus: ResMut<PortraitFocus>,
    live: Query<(&BeaverSprite, &Visibility), Or<(With<SimBeaver>, With<BodySprite>)>>,
    stand_in: Query<(Entity, &BeaverSprite), With<StandIn>>,
    mut cam: Query<(Entity, &mut Camera, Option<&RenderLayers>), With<PortraitCam>>,
) {
    let want = hud.as_ref().and_then(|h| match h.page() { Some(crate::hud::Page::Beaver(id)) => Some(id.0), _ => None });
    if focus.0 != want {
        focus.0 = want;
    }
    let Ok((cam_e, mut c, layers)) = cam.single_mut() else { return };
    if c.is_active != want.is_some() {
        c.is_active = want.is_some();
    }
    let out = want.is_some_and(|id| live.iter().any(|(sp, v)| sp.id == id && *v != Visibility::Hidden));
    let den = want.filter(|_| !out);
    let in_den = layers.is_some_and(|l| l.intersects(&RenderLayers::layer(LAYER)));
    if in_den != den.is_some() {
        let mut e = commands.entity(cam_e);
        if den.is_some() {
            e.insert((RenderLayers::layer(LAYER), AmbientLight { color: Color::srgb(1.0, 0.85, 0.7), brightness: 250.0, ..default() }));
            c.clear_color = ClearColorConfig::Custom(DEN_DARK);
        } else {
            e.insert(RenderLayers::default()).remove::<AmbientLight>();
            c.clear_color = ClearColorConfig::Default;
        }
    }
    let have = stand_in.iter().next();
    if have.map(|h| h.1.id) == den {
        return;
    }
    if let Some((e, _)) = have {
        commands.entity(e).despawn();
    }
    let Some(id) = den else { return };
    let s = &sim.0;
    let bid = sim::colony::BeaverId(id);
    let features = s.colony.looks_of(bid);
    let (pose, look, age) = match s.colony.beaver(bid) {
        Some(b) => (Pose::Rest, look_of(s, bid), b.age_days),
        None => (Pose::Idle, Look::default(), sim::colony::ADULT_DAYS),
    };
    commands.spawn((
        StandIn,
        BeaverSprite { id, pose, load: None, yaw: 0.0, wet: false, look, features: features.clone() },
        Transform::from_translation(AT).with_scale(Vec3::splat(BEAVER_SIZE * age_scale_days(age) * looks_size(&features))),
        Visibility::Inherited,
        Propagate(RenderLayers::layer(LAYER)),
    ));
}

#[derive(Clone, Copy)]
struct Shot {
    side: f32,
    dist: f32,
    up: f32,
    aim: f32,
    ahead: f32,
}

impl Shot {
    fn of(pose: Pose) -> Shot {
        match pose {
            Pose::Walk | Pose::Run | Pose::Climb => Shot { side: 0.35, dist: 3.0, up: 0.95, aim: 0.55, ahead: 0.0 },
            Pose::Carry => Shot { side: 0.35, dist: 3.4, up: 1.2, aim: 0.9, ahead: 0.0 },
            Pose::Forage => Shot { side: 1.1, dist: 3.4, up: 1.2, aim: 0.9, ahead: 0.2 },
            Pose::Watch => Shot { side: 0.6, dist: 3.4, up: 1.2, aim: 0.9, ahead: 0.0 },
            Pose::Swim => Shot { side: 0.4, dist: 3.0, up: 0.7, aim: 0.25, ahead: 0.0 },
            Pose::Gnaw | Pose::Dig | Pose::Work | Pose::Mend | Pose::Handle => Shot { side: 1.1, dist: 3.0, up: 1.1, aim: 0.55, ahead: 0.35 },
            _ => Shot { side: 0.6, dist: 2.8, up: 1.0, aim: 0.5, ahead: 0.0 },
        }
    }
    fn lerp(self, o: Shot, k: f32) -> Shot {
        let l = |a: f32, b: f32| a + (b - a) * k;
        Shot { side: l(self.side, o.side), dist: l(self.dist, o.dist), up: l(self.up, o.up), aim: l(self.aim, o.aim), ahead: l(self.ahead, o.ahead) }
    }
}

#[derive(Default)]
struct Follow {
    on: Option<Entity>,
    yaw: f32,
    shot: Option<Shot>,
    sign: f32,
    lift: f32,
}

fn cell_of(s: &sim::SimState, p: Vec3) -> (usize, usize) {
    let (cx, cy) = crate::flow::world_cell(&s.height, p);
    (cx.clamp(0.0, s.height.width() as f32 - 1.0) as usize, cy.clamp(0.0, s.height.height() as f32 - 1.0) as usize)
}

fn floor(s: &sim::SimState, p: Vec3) -> f32 {
    crate::flow::height_at_world(&s.height, p.x, p.z).max(stand_y(s, cell_of(s, p)))
}

fn things_near(s: &sim::SimState, at: (usize, usize)) -> std::collections::HashMap<(usize, usize), f32> {

    let near = |c: (usize, usize)| c.0.abs_diff(at.0) <= 4 && c.1.abs_diff(at.1) <= 4;
    let mut out = std::collections::HashMap::new();
    for r in s.resources.items().iter().filter(|r| near(r.cell)) {
        let h = match r.kind {
            k if k.is_tree() && r.has_yield() => 4.0,
            k if k.is_felled() => 0.9,
            _ => continue,
        };
        out.insert(r.cell, h);
    }
    for b in &s.buildings.buildings {
        let (w, h) = b.kind.spec().footprint;
        for c in (0..w).flat_map(|x| (0..h).map(move |y| (b.pos.0 + x, b.pos.1 + y))).filter(|c| near(*c)) {
            out.insert(c, 2.5);
        }
    }
    for x in at.0.saturating_sub(5)..=at.0 + 5 {
        for y in at.1.saturating_sub(5)..=at.1 + 5 {
            let Some(&h) = s.water.dam_height.get(x, y).filter(|h| **h > 0.0) else { continue };
            for c in (x.saturating_sub(1)..=x + 1).flat_map(|cx| (y.saturating_sub(1)..=y + 1).map(move |cy| (cx, cy))).filter(|c| near(*c)) {
                let v: &mut f32 = out.entry(c).or_insert(0.0);
                *v = v.max(h + 0.4_f32);
            }
        }
    }
    for p in s.work.piles.items.iter().filter(|p| near(p.cell)) {
        out.entry(p.cell).or_insert(0.9);
    }
    out
}

fn clear(s: &sim::SimState, things: &std::collections::HashMap<(usize, usize), f32>, look: Vec3, off: Vec3) -> f32 {
    use sim::nav::Surface;
    const STEPS: usize = 12;
    let own = cell_of(s, look);
    for i in 1..=STEPS {
        let p = look + off * (i as f32 / STEPS as f32);
        let c = cell_of(s, p);
        let solid = matches!(s.nav.surface_at(c), Some(Surface::Solid | Surface::LodgeInside));
        let thing = if c == own { 0.0 } else { things.get(&c).copied().unwrap_or(0.0) };
        if p.y < floor(s, p) + if solid { 1.6 } else { 0.05f32 }.max(thing) {
            return (i - 1) as f32 / STEPS as f32;
        }
    }
    1.0
}

#[allow(clippy::type_complexity)]
fn aim(
    time: Res<Time>,
    sim: Res<crate::state::Sim>,
    focus: Res<PortraitFocus>,
    live: Query<(Entity, &Transform, &BeaverSprite, &Visibility), (Or<(With<SimBeaver>, With<BodySprite>)>, Without<PortraitCam>)>,
    stand_in: Query<(Entity, &Transform, &BeaverSprite), (With<StandIn>, Without<PortraitCam>)>,
    mut cam: Query<&mut Transform, With<PortraitCam>>,
    mut st: Local<Follow>,
) {
    let Some(id) = focus.0 else {
        st.on = None;
        return;
    };
    let target = live
        .iter()
        .find(|(_, _, sp, v)| sp.id == id && **v != Visibility::Hidden)
        .map(|(e, t, sp, _)| (e, *t, sp.pose, true))
        .or_else(|| stand_in.iter().find(|(_, _, sp)| sp.id == id).map(|(e, t, sp)| (e, *t, sp.pose, false)));
    let Some((e, tr, pose, out)) = target else { return };
    let Ok(mut cam_tr) = cam.single_mut() else { return };
    let s = &sim.0;
    let dt = time.delta_secs();
    let fresh = st.on != Some(e);
    st.on = Some(e);
    let (yaw, _, _) = tr.rotation.to_euler(EulerRot::YXZ);
    if fresh {
        st.yaw = yaw;
        st.sign = 1.0;
        st.lift = 0.0;
    }
    let turn = 1.0 - (-dt * 3.0).exp();
    let d = (yaw - st.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    st.yaw += d * turn;
    let size = tr.scale.x;
    let fwd = Vec3::new(st.yaw.sin(), 0.0, st.yaw.cos());
    let mut want = Shot::of(pose);
    let offset = |sh: &Shot| Quat::from_rotation_y(sh.side) * fwd * sh.dist * size + Vec3::Y * (sh.up - sh.aim) * size;
    if out {
        let look = tr.translation + Vec3::Y * want.aim * size + fwd * want.ahead * size;
        let base = want.side;
        let things = things_near(s, cell_of(s, tr.translation));
        let mut best = (-1.0, want);
        for side in [st.sign * base, -st.sign * base, st.sign * (base + 0.7), -st.sign * (base + 0.7)] {
            let sh = Shot { side, ..want };
            let c = clear(s, &things, look, offset(&sh));
            if c > best.0 + 0.01 {
                best = (c, sh);
            }
            if c >= 1.0 {
                break;
            }
        }
        want = best.1;
        want.dist *= best.0.max(0.4);
        if want.side * st.sign < 0.0 {
            st.sign = -st.sign;
        }
    }
    let shot = if fresh { want } else { st.shot.unwrap_or(want).lerp(want, 1.0 - (-dt * 4.0).exp()) };
    st.shot = Some(shot);
    let look = tr.translation + Vec3::Y * shot.aim * size + fwd * shot.ahead * size;
    let mut at = look + offset(&shot);
    if out {
        let need = (floor(s, at) + 0.35 * size - at.y).max(0.0);
        st.lift = if fresh { need } else { st.lift + (need - st.lift) * (1.0 - (-dt * 4.0).exp()) };
        at.y += st.lift.max(need * 0.8);
    }
    *cam_tr = Transform::from_translation(at).looking_at(look, Vec3::Y);
}
