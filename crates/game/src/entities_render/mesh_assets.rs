use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use std::{collections::HashMap, sync::LazyLock};

static LIBRARY: LazyLock<gltf::Gltf> = LazyLock::new(|| {
    gltf::Gltf::from_slice(include_bytes!("../../../../assets/models/colony.glb"))
        .expect("valid embedded colony mesh library")
});
static NAMES: LazyLock<HashMap<&'static str, usize>> = LazyLock::new(|| {
    LIBRARY.meshes().map(|mesh| (mesh.name().expect("named mesh"), mesh.index())).collect()
});

/// Static artwork keeps its original linear vertex colors and animation pivots.
pub fn load(name: &str) -> Mesh {
    let mesh = LIBRARY.meshes().nth(NAMES[name]).expect("embedded mesh");
    let primitive = mesh.primitives().next().expect("mesh primitive");
    let reader = primitive.reader(|_| LIBRARY.blob.as_deref());
    Mesh::new(PrimitiveTopology::TriangleList, default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, reader.read_positions().unwrap().collect::<Vec<_>>())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, reader.read_normals().unwrap().collect::<Vec<_>>())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, reader.read_colors(0).unwrap().into_rgba_f32().collect::<Vec<_>>())
        .with_inserted_indices(Indices::U32(reader.read_indices().unwrap().into_u32().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_models_have_complete_triangle_geometry() {
        assert_eq!(NAMES.len(), 419);
        for name in NAMES.keys() {
            let mesh = load(name);
            let count = mesh.count_vertices();
            assert!(count > 0, "{name}");
            assert_eq!(mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap().len(), count, "{name}");
            assert_eq!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).unwrap().len(), count, "{name}");
            let indices = mesh.indices().unwrap();
            assert_eq!(indices.len() % 3, 0, "{name}");
            assert!(indices.iter().all(|i| i < count), "{name}");
        }
    }
}
