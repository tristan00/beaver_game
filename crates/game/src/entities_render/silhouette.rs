//! Beavers hidden behind trees (or anything else) show through as a flat silhouette, instead
//! of the trees being cut away round them. Each body part carries a twin drawn with
//! `Silhouette`: its depth test passes only where the scene is nearer, and its depth is pulled
//! towards the camera by about one per cent of the distance first, so a beaver's own head and
//! legs never count as hiding it.
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, CompareFunction, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct Silhouette {
    #[uniform(0)]
    pub color: Vec4,
}

impl Material for Silhouette {
    fn fragment_shader() -> ShaderRef {
        "embedded://beaver_colony/entities_render/silhouette.wesl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            // Depth runs reversed (nearer is larger): pass only where the scene stands nearer
            // than this fragment brought forward by 2^16 steps, about 1/128 of its depth.
            ds.depth_compare = Some(CompareFunction::Less);
            ds.depth_write_enabled = Some(false);
            ds.bias.constant = 1 << 16;
            ds.bias.slope_scale = 0.0;
            ds.bias.clamp = 0.0;
        }
        Ok(())
    }
}

pub struct SilhouettePlugin;

impl Plugin for SilhouettePlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "silhouette.wesl");
        app.add_plugins(MaterialPlugin::<Silhouette>::default()).add_systems(Startup, init);
    }
}

/// The one silhouette material every beaver shares.
#[derive(Resource)]
pub struct SilhouetteMat(pub Handle<Silhouette>);

fn init(mut commands: Commands, mut mats: ResMut<Assets<Silhouette>>) {
    // Warm fur, half seen.
    let c = LinearRgba::from(Color::srgb(0.86, 0.6, 0.36));
    commands.insert_resource(SilhouetteMat(mats.add(Silhouette { color: Vec4::new(c.red, c.green, c.blue, 0.6) })));
}

/// A part's silhouette twin: a child of the part, so it moves, bends and hides with it.
pub fn twin(p: &mut ChildSpawnerCommands, mesh: Handle<Mesh>, mat: &Handle<Silhouette>) {
    p.spawn((Mesh3d(mesh), MeshMaterial3d(mat.clone()), Transform::default(), bevy::light::NotShadowCaster));
}
