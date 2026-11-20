// Water surface. The mesh carries, per vertex: colour.r = depth (m), colour.g = white water
// (0..1, from the Froude number), uv = world xz, uv_b = surface velocity (m/s, world xz).
// Ripples and streaks are advected along the velocity with a two-phase flow map, so rivers visibly
// run downstream and ponds only shiver in the wind; depth sets colour and how much bed shows
// through; foam gathers on the waterline and where water runs fast or falls over a step.
import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::main_pass_post_lighting_processing,
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings as view_bindings,
};
import bevy_render::color_operations::{linear_to_srgb, srgb_to_linear};

struct WaterStyle {
    // rgb: display colours (linear)
    shallow: vec4<f32>,
    deep: vec4<f32>,
    foam: vec4<f32>,
    ice: vec4<f32>,
    // x: depth (m) at which water reads deep, y: alpha in a film, z: alpha when deep,
    // w: ice cover 0..1 (winter)
    look: vec4<f32>,
}
@group(3) @binding(100) var<uniform> water: WaterStyle;

fn hash2(p: vec2<f32>) -> vec2<f32> {
    let q = vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)));
    return fract(sin(q) * 43758.5453) * 2.0 - 1.0;
}

// Gradient noise in -1..1.
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = dot(hash2(i), f);
    let b = dot(hash2(i + vec2(1.0, 0.0)), f - vec2(1.0, 0.0));
    let c = dot(hash2(i + vec2(0.0, 1.0)), f - vec2(0.0, 1.0));
    let d = dot(hash2(i + vec2(1.0, 1.0)), f - vec2(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y) * 1.6;
}

// Ripple height at p. Running water draws its ripples out into streaks along the flow by
// averaging samples a short way upstream (local offsets only: rotating world coordinates by a
// direction that varies over the surface would shear the pattern into stripes).
fn ripples(p: vec2<f32>, dir: vec2<f32>, stretch: f32, detail: f32) -> f32 {
    var h = 0.0;
    for (var k = 0; k < 3; k++) {
        let q = p - dir * (f32(k) * 0.45 * stretch);
        h += noise(q * 1.1) * 0.65 + noise(q * 2.3 + 7.3) * 0.3 * detail;
    }
    return h / 3.0;
}

// Two flow-map phases half a period apart. Screen derivatives below supply the
// normal, avoiding three complete noise evaluations per phase.
fn surface(p: vec2<f32>, vel_in: vec2<f32>, t: f32, detail: f32) -> f32 {
    // Fast water drags the pattern no faster than this, or streaks shear into stripes.
    let vel = vel_in * min(1.0, 1.2 / max(length(vel_in), 1e-4));
    let speed = length(vel);
    let dir = select(vec2(1.0, 0.0), vel / max(speed, 1e-4), speed > 1e-3);
    let stretch = smoothstep(0.03, 0.4, speed);
    // Still water drifts slowly with the wind.
    let drift = vec2(0.05, 0.03);
    let flow = vel + drift * (1.0 - stretch);
    let period = 2.0;
    var acc = 0.0;
    for (var k = 0; k < 2; k++) {
        let ph = fract(t / period + f32(k) * 0.5);
        let w = 1.0 - abs(2.0 * ph - 1.0);
        let q = p - flow * ph * period + f32(k) * vec2(3.7, 1.9);
        let h0 = ripples(q, dir, stretch, detail);
        acc += w * h0;
    }
    return acc;
}

// The camera's tone mapping (copied from the terrain shader) and its inverse, so water can aim at
// display colours like the rest of the palette.
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
    let pbr_input = pbr_input_from_standard_material(in, is_front);
    var depth = 0.0;
    var white = 0.0;
    var frozen = 0.0;
    @if(VERTEX_COLORS) {
        depth = max(in.color.r, 0.0);
        white = clamp(in.color.g, 0.0, 1.0);
        frozen = smoothstep(0.0, 0.08, in.color.b);
    }
    var vel = vec2(0.0);
    @if(VERTEX_UVS_B) {
        vel = in.uv_b;
    }
    let p = in.world_position.xz;
    let t = view_bindings::globals.time;
    // Ripples finer than a pixel or two only shimmer; fade them with the pixel footprint.
    let pix = length(fwidth(p));
    let detail = 1.0 - smoothstep(0.08, 0.35, pix);
    let s = surface(p, vel, t, detail) * (1.0 - frozen);
    let speed = length(vel);

    // Normal from the ripple slopes; calmer when deep and still, livelier when it runs.
    let amp = (0.06 + 0.18 * smoothstep(0.05, 0.6, speed)) * (1.0 - 0.7 * smoothstep(0.15, 0.8, pix));
    let displaced = in.world_position.xyz + vec3(0.0, s * amp, 0.0);
    let normal = normalize(cross(dpdy(displaced), dpdx(displaced)));
    let n = select(-normal, normal, normal.y >= 0.0);
    let v = normalize(view_bindings::view.world_position - in.world_position.xyz);

    // Colour: shallow water is mostly the bed seen through a tint; deep water is its own colour.
    let deep = 1.0 - exp(-depth / water.look.x);
    var col = mix(water.shallow.rgb, water.deep.rgb, deep);
    // Streaks: brighter and darker bands carried by the current.
    col *= 1.0 + 0.16 * s * (0.4 + smoothstep(0.03, 0.4, speed));
    let streak = smoothstep(0.14, 0.4, s) * smoothstep(0.04, 0.4, speed);
    col = mix(col, water.shallow.rgb * 1.3, streak * 0.32);
    var alpha = mix(water.look.y, water.look.z, 1.0 - exp(-depth / 0.25));

    // Sky reflection at grazing angles and a glint of the sun.
    let fresnel = pow(1.0 - max(dot(n, v), 0.0), 4.0);
    col = mix(col, vec3(0.80, 0.89, 0.93), fresnel * 0.2);
    var glint = 0.0;
    if view_bindings::lights.n_directional_lights > 0u {
        let l = normalize(view_bindings::lights.directional_lights[0].direction_to_light);
        let h = normalize(l + v);
        glint = pow(max(dot(n, h), 0.0), 60.0) * 0.35;
    }

    // Foam: a lacy line along the waterline, and white water where it runs fast or falls.
    let lace = noise(p * 2.3 + vec2(t * 0.15, -t * 0.11)) * 0.5 + 0.5;
    let shore = (1.0 - smoothstep(0.0, 0.025, depth)) * smoothstep(0.3, 0.8, lace) * 0.35;
    let churn = noise((p - vel * t * 0.7) * 3.1) * 0.5 + 0.5;
    let rapids = white * smoothstep(0.35, 0.7, churn + white * 0.3);
    let foam = clamp(max(shore * 0.85, rapids), 0.0, 1.0);
    col = mix(col, water.foam.rgb, foam);
    // Soft waterline: the surface fades into the wet ground over its first few centimetres.
    alpha *= smoothstep(0.0, 0.008, depth);
    alpha = max(alpha, foam * 0.9);
    col += vec3(glint);
    alpha = min(1.0, alpha + glint);

    // Blue translucent ice remains distinct from snow-covered ground. Broad
    // tonal patches and fine dark seams give the frozen sheet some structure.
    let ice = frozen * smoothstep(0.0, 0.008, depth);
    let ice_grain = noise(p * 0.18) * 0.5 + 0.5;
    let cracks = 1.0 - smoothstep(0.0, 0.055, abs(noise(p * 0.35 + vec2(8.0, 3.0))));
    var ice_col = water.ice.rgb * (0.82 + ice_grain * 0.18);
    ice_col = mix(ice_col, water.deep.rgb * 0.75, cracks * 0.22);
    col = mix(col, ice_col, ice * 0.92);
    alpha = mix(alpha, 0.92, ice);

    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr_input, vec4(undo_display_transform(col), alpha));
    return out;
}
