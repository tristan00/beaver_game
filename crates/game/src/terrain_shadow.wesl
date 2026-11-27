import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings as view_bindings,
    mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
    shadows::fetch_directional_shadow,
};
import bevy_render::color_operations::{linear_to_srgb, srgb_to_linear};

struct PlumShadow {
    plum: vec4<f32>,
    edge: vec4<f32>,
}
@group(3) @binding(100) var<uniform> plum_shadow: PlumShadow;

fn sun_visibility(p: ptr<function, bevy_pbr::render::pbr_types::PbrInput>) -> f32 {
    if ((*p).flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) == 0u || view_bindings::lights.n_directional_lights == 0u {
        return 1.0;
    }
    if (view_bindings::lights.directional_lights[0].flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) == 0u {
        return 1.0;
    }
    let view_z = dot(vec4<f32>(
        view_bindings::view.view_from_world[0].z,
        view_bindings::view.view_from_world[1].z,
        view_bindings::view.view_from_world[2].z,
        view_bindings::view.view_from_world[3].z
    ), (*p).world_position);
    return fetch_directional_shadow(0u, (*p).world_position, (*p).world_normal, view_z, (*p).frag_coord.xy);
}

fn tm_curve(v: f32) -> f32 {
    return 1.0 - exp(-v);
}

fn display_transform(col: vec3<f32>) -> vec3<f32> {
    let ycbcr = col * mat3x3<f32>(0.2126, 0.7152, 0.0722, -0.1146, -0.3854, 0.5, 0.5, -0.4542, -0.0458);
    let bt = tm_curve(length(ycbcr.yz) * 2.4);
    var desat = max((bt - 0.7) * 0.8, 0.0);
    desat *= desat;
    let desat_col = mix(col, ycbcr.xxx, desat);
    let tm0 = col * max(0.0, tm_curve(ycbcr.x) / max(1e-5, dot(col, vec3(0.2126, 0.7152, 0.0722))));
    let tm1 = vec3(tm_curve(desat_col.r), tm_curve(desat_col.g), tm_curve(desat_col.b));
    return mix(tm0, tm1, bt * bt) * 0.97;
}

fn undo_display_transform(d: vec3<f32>) -> vec3<f32> {
    let goal = clamp(d, vec3(1e-4), vec3(0.95));
    var x = goal;
    for (var i = 0; i < 8; i++) {
        x = x * goal / max(display_transform(x), vec3(1e-5));
    }
    return x;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);

    let vis = smoothstep(plum_shadow.edge.x, plum_shadow.edge.y, sun_visibility(&pbr_input));
    pbr_input.flags = pbr_input.flags & ~MESH_FLAGS_SHADOW_RECEIVER_BIT;
    let raw = apply_pbr_lighting(pbr_input);
    var flat = pbr_input;
    flat.material.base_color = vec4(1.0);
    flat.N = vec3(0.0, 1.0, 0.0);
    flat.world_normal = vec3(0.0, 1.0, 0.0);
    flat.diffuse_occlusion = vec3(1.0);
    flat.specular_occlusion = 1.0;
    let relief = raw.rgb / max(apply_pbr_lighting(flat).rgb * pbr_input.material.base_color.rgb, vec3(1e-4));
    let lit = vec4(pbr_input.material.base_color.rgb * mix(vec3(1.0), relief, plum_shadow.edge.w), raw.a);
    let shade = srgb_to_linear(mix(linear_to_srgb(clamp(lit.rgb, vec3(0.0), vec3(1.0))), plum_shadow.plum.rgb, plum_shadow.plum.a));

    var out: FragmentOutput;
    let shown = undo_display_transform(mix(shade, lit.rgb, vis));
    out.color = main_pass_post_lighting_processing(pbr_input, vec4(shown, lit.a));
    if plum_shadow.edge.z > 0.5 {
        out.color = select(select(vec4(0.0, 1.0, 1.0, 1.0), vec4(1.0, 0.0, 1.0, 1.0), vis <= 0.0), vec4(1.0, 1.0, 0.0, 1.0), vis >= 1.0);
    }
    return out;
}
