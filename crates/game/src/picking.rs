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
