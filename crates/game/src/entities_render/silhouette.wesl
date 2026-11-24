// A beaver seen through whatever stands in front of it (trees, mostly): one flat colour,
// drawn only where something else is nearer the camera (see silhouette.rs).
import bevy_pbr::render::forward_io::VertexOutput;

struct Silhouette {
    color: vec4<f32>,
}
@group(3) @binding(0) var<uniform> silhouette: Silhouette;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return silhouette.color;
}
