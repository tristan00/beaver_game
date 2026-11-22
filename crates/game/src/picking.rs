//! Select the visible object under the pointer, using its rendered geometry.
use bevy::{camera::primitives::Aabb, ecs::system::SystemParam, mesh::Indices, prelude::*};
use bevy::picking::mesh_picking::ray_cast::{Backfaces, ray_aabb_intersection_3d, ray_mesh_intersection};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Building(sim::buildings::BuildingId),
    Beaver(sim::colony::BeaverId),
    Lodge(sim::colony::LodgeId),
    /// A tree or felled trunk, picked by its model so its crown can be clicked.
    Resource(sim::resources::ResourceId),
}

impl Target {
    pub fn home(sim: &sim::SimState, home: sim::colony::LodgeId) -> Self {
        sim.work.homes.iter().find(|(_, id)| **id == home)
            .map_or(Self::Lodge(home), |(&building, _)| Self::Building(building))
    }
}

#[derive(SystemParam)]
pub struct Picker<'w, 's> {
    meshes: Res<'w, Assets<Mesh>>,
    targets: Query<'w, 's, (&'static Target, &'static Mesh3d, &'static GlobalTransform, &'static InheritedVisibility, &'static Aabb)>,
}

impl Picker<'_, '_> {
    #[cfg(test)]
    pub fn cast(&self, ray: Ray3d, ground_distance: Option<f32>) -> Option<Target> {
        self.cast_at(ray, ground_distance).map(|(t, _)| t)
    }

    /// The nearest object along the ray and how far along it the ray meets it.
    pub fn cast_at(&self, ray: Ray3d, ground_distance: Option<f32>) -> Option<(Target, f32)> {
        let mut nearest = ground_distance.unwrap_or(f32::INFINITY);
        let mut picked = None;
        for (target, handle, transform, visible, bounds) in &self.targets {
            if !visible.get() { continue; }
            let transform = transform.affine();
            let bounds = Aabb3d::new(bounds.center, bounds.half_extents);
            let Some(distance) = ray_aabb_intersection_3d(ray, &bounds, &transform) else { continue; };
            if distance > nearest { continue; }
            let Some(mesh) = self.meshes.get(&handle.0) else { continue; };
            if let Some(distance) = mesh_hit(ray, &transform, mesh).filter(|d| *d <= nearest) {
                nearest = distance;
                picked = Some((*target, distance));
            }
        }
        picked
    }
}

fn mesh_hit(ray: Ray3d, transform: &bevy::math::Affine3A, mesh: &Mesh) -> Option<f32> {
    let positions = mesh.try_attribute(Mesh::ATTRIBUTE_POSITION).ok()?.as_float3()?;
    let hit = match mesh.try_indices().ok() {
        Some(Indices::U16(indices)) => ray_mesh_intersection(ray, transform, positions, None, Some(indices.as_slice()), None, Backfaces::Include),
        Some(Indices::U32(indices)) => ray_mesh_intersection(ray, transform, positions, None, Some(indices.as_slice()), None, Backfaces::Include),
        None => ray_mesh_intersection::<u32>(ray, transform, positions, None, None, None, Backfaces::Include),
    }?;
    Some(hit.distance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::SystemState;

    #[test]
    fn completed_home_visuals_select_the_physical_building() {
        let mut sim = sim::SimState::new(1);
        let home = sim::colony::LodgeId(4);
        let building = sim::buildings::BuildingId(7);
        assert_eq!(Target::home(&sim, home), Target::Lodge(home));
        sim.work.homes.insert(building, home);
        assert_eq!(Target::home(&sim, home), Target::Building(building));
    }

    fn cube(world: &mut World, target: Target, position: Vec3, visible: bool) {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(2.0,2.0,2.0));
        world.spawn((target, Mesh3d(mesh), GlobalTransform::from_translation(position),
            if visible { InheritedVisibility::VISIBLE } else { InheritedVisibility::HIDDEN },
            Aabb::from_min_max(Vec3::splat(-1.0), Vec3::ONE)));
    }

    #[test]
    fn a_roof_can_be_selected_when_the_ground_hit_is_outside_its_footprint() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        let building = Target::Building(sim::buildings::BuildingId(1));
        cube(&mut world, building, Vec3::new(0.0,5.0,0.0), true);
        let ray = Ray3d::new(Vec3::new(0.0,10.0,5.0), Dir3::new(Vec3::new(0.0,-1.0,-1.0)).unwrap());
        let ground_distance = -ray.origin.y / ray.direction.y;
        assert!(ray.get_point(ground_distance).z < -1.0);
        let mut picker = SystemState::<Picker>::new(&mut world);
        assert_eq!(picker.get(&world).unwrap().cast(ray, Some(ground_distance)), Some(building));
        assert_eq!(picker.get(&world).unwrap().cast(ray, Some(1.0)), None, "terrain must occlude objects behind it");
    }

    #[test]
    fn nearest_visible_model_wins_without_a_nearby_beaver_stealing_the_building() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        let building = Target::Building(sim::buildings::BuildingId(1));
        let beaver = Target::Beaver(sim::colony::BeaverId(2));
        cube(&mut world, building, Vec3::ZERO, true);
        cube(&mut world, beaver, Vec3::new(4.0,0.0,0.0), true);
        cube(&mut world, Target::Building(sim::buildings::BuildingId(3)), Vec3::new(0.0,0.0,3.0), false);
        let ray = Ray3d::new(Vec3::new(0.0,0.0,10.0), Dir3::NEG_Z);
        let mut picker = SystemState::<Picker>::new(&mut world);
        assert_eq!(picker.get(&world).unwrap().cast(ray,None),Some(building));
        cube(&mut world, beaver, Vec3::new(0.0,0.0,3.0),true);
        assert_eq!(picker.get(&world).unwrap().cast(ray,None),Some(beaver));
    }
}
