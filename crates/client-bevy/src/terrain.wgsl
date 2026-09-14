// Realism pass (2026-09-14): photoscanned splat terrain.
//
// Four CC0 Poly Haven layers live in three 2D texture ARRAYS (albedo, GL
// normal, AO/rough/metal) so the whole blend costs three bindings — WebGL2
// caps a fragment stage at 16 textures and StandardMaterial already uses a
// handful.
//
//   layer 0 grass · 1 dirt · 2 cliff rock · 3 sand
//
// Weights: vertex colour r = dirt, g = sand, b = wet/darken (from Rust, with
// noise for natural patchiness); cliff comes from the geometric slope here.
// Ground layers project top-down; the cliff layer is triplanar so steep faces
// don't smear. Albedo is sampled at two scales and mixed by distance, which
// kills the obvious tile grid that made the old ground read as wallpaper.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

struct TerrainParams {
    // Per-layer albedo tint (rgb) — keeps each act's palette identity.
    tint0: vec4<f32>,
    tint1: vec4<f32>,
    tint2: vec4<f32>,
    tint3: vec4<f32>,
    // x: near tiling (world units per tile), y: far tiling,
    // z: slope where cliff begins (1 - normal.y), w: slope where cliff is full.
    tiling: vec4<f32>,
}

@group(2) @binding(100) var<uniform> params: TerrainParams;
@group(2) @binding(101) var albedo_arr: texture_2d_array<f32>;
@group(2) @binding(102) var albedo_smp: sampler;
@group(2) @binding(103) var normal_arr: texture_2d_array<f32>;
@group(2) @binding(104) var normal_smp: sampler;
@group(2) @binding(105) var arm_arr: texture_2d_array<f32>;
@group(2) @binding(106) var arm_smp: sampler;

fn tint(i: i32) -> vec3<f32> {
    switch i {
        case 0: { return params.tint0.rgb; }
        case 1: { return params.tint1.rgb; }
        case 2: { return params.tint2.rgb; }
        default: { return params.tint3.rgb; }
    }
}

// Cheap value noise for macro variation (no texture needed).
fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash2(i), hash2(i + vec2(1.0, 0.0)), u.x),
               mix(hash2(i + vec2(0.0, 1.0)), hash2(i + vec2(1.0, 1.0)), u.x), u.y);
}

// Unpack a GL tangent-space normal sample into a world-space normal for a
// planar projection whose tangent/bitangent are `t`/`b` and face normal `n`.
fn planar_normal(s: vec3<f32>, t: vec3<f32>, b: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let ts = s * 2.0 - 1.0;
    return normalize(t * ts.x + b * ts.y + n * ts.z);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let wp = in.world_position.xyz;
    // Degenerate pole triangles on generated rocks carry a zero normal;
    // fall back to up instead of propagating NaN (black specks).
    let n_raw = in.world_normal;
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(n_raw), dot(n_raw, n_raw) > 1e-8);
    let dist = length(wp - bevy_pbr::mesh_view_bindings::view.world_position);

    #ifdef VERTEX_COLORS
    let splat = in.color;
    #else
    let splat = vec4<f32>(0.0);
    #endif

    // Macro breakup: low-frequency noise nudges weights and brightness so a
    // hillside isn't one uniform carpet.
    let macro_n = vnoise(wp.xz / 260.0) * 0.6 + vnoise(wp.xz / 67.0) * 0.4;

    // v runs along -z so the GL bitangent (cross(N, T)) matches +v.
    let uv_near = vec2<f32>(wp.x, -wp.z) / params.tiling.x;
    let uv_far = vec2<f32>(wp.x, -wp.z) / params.tiling.y;
    let far_t = smoothstep(120.0, 700.0, dist);

    // ── Ground layers (top-down) ────────────────────────────────────────────
    var col = vec3<f32>(0.0);
    var nrm = vec3<f32>(0.0);
    var arm = vec3<f32>(0.0);

    let slope = 1.0 - n.y;
    var w_cliff = smoothstep(params.tiling.z, params.tiling.w, slope + (macro_n - 0.5) * 0.08);
    var w_dirt = clamp(splat.r + (macro_n - 0.55) * 0.5, 0.0, 1.0);
    var w_sand = clamp(splat.g, 0.0, 1.0);

    // Height-aware blend: the brighter/higher texel of the incoming layer wins
    // the transition, so dirt breaks through between grass clumps rather than
    // cross-fading like a watercolour.
    let g_near = textureSample(albedo_arr, albedo_smp, uv_near, 0).rgb;
    let g_far = textureSample(albedo_arr, albedo_smp, uv_far, 0).rgb;
    let d_near = textureSample(albedo_arr, albedo_smp, uv_near, 1).rgb;
    let s_near = textureSample(albedo_arr, albedo_smp, uv_near, 3).rgb;

    let g_arm = textureSample(arm_arr, arm_smp, uv_near, 0).rgb;
    let d_arm = textureSample(arm_arr, arm_smp, uv_near, 1).rgb;
    let s_arm = textureSample(arm_arr, arm_smp, uv_near, 3).rgb;

    let hd = clamp(w_dirt * 1.6 - 0.3 + (d_arm.r - g_arm.r) * 0.6, 0.0, 1.0);
    let hs = clamp(w_sand * 1.6 - 0.3 + (s_arm.r - g_arm.r) * 0.6, 0.0, 1.0);

    // The leafy-grass photo averages to straw at low mips, so distant
    // meadows read as desert; pull the far field back toward green.
    let far_green = mix(vec3<f32>(1.0), vec3<f32>(0.78, 1.0, 0.62), far_t);
    let grass_col = mix(g_near, g_far, far_t * 0.5) * tint(0) * far_green;
    col = mix(grass_col, d_near * tint(1), hd);
    col = mix(col, s_near * tint(3), hs);
    arm = mix(mix(g_arm, d_arm, hd), s_arm, hs);

    let tx = vec3<f32>(1.0, 0.0, 0.0);
    let tz = vec3<f32>(0.0, 0.0, -1.0);
    let up = vec3<f32>(0.0, 1.0, 0.0);
    let g_nrm = planar_normal(textureSample(normal_arr, normal_smp, uv_near, 0).rgb, tx, tz, up);
    let d_nrm = planar_normal(textureSample(normal_arr, normal_smp, uv_near, 1).rgb, tx, tz, up);
    let s_nrm = planar_normal(textureSample(normal_arr, normal_smp, uv_near, 3).rgb, tx, tz, up);
    nrm = normalize(mix(mix(g_nrm, d_nrm, hd), s_nrm, hs));

    // Tilt the detail normal onto the actual terrain slope (the ground layers
    // were projected as if the surface were flat).
    let rot_axis = cross(up, n);
    let s_len = length(rot_axis);
    if s_len > 1e-4 {
        let axis = rot_axis / s_len;
        let c = n.y;
        let s = s_len;
        nrm = nrm * c + cross(axis, nrm) * s + axis * dot(axis, nrm) * (1.0 - c);
    }
    // ── Cliff layer (triplanar) ─────────────────────────────────────────────
    {
        var bw = pow(abs(n), vec3<f32>(4.0)) + vec3<f32>(1e-4);
        bw = bw / (bw.x + bw.y + bw.z);
        let cs = params.tiling.x * 1.6;
        let uvx = wp.zy / cs;
        let uvy = vec2<f32>(wp.x, -wp.z) / cs;
        let uvz = wp.xy / cs;
        let cc = (textureSample(albedo_arr, albedo_smp, uvx, 2).rgb * bw.x
            + textureSample(albedo_arr, albedo_smp, uvy, 2).rgb * bw.y
            + textureSample(albedo_arr, albedo_smp, uvz, 2).rgb * bw.z) * tint(2);
        let ca = textureSample(arm_arr, arm_smp, uvx, 2).rgb * bw.x
            + textureSample(arm_arr, arm_smp, uvy, 2).rgb * bw.y
            + textureSample(arm_arr, arm_smp, uvz, 2).rgb * bw.z;
        // sign(0) = 0 would zero the tangent frame and NaN the normal
        // (black specks on axis-aligned faces).
        let sx = select(-1.0, 1.0, n.x >= 0.0);
        let sz = select(-1.0, 1.0, n.z >= 0.0);
        let cn = normalize(
            planar_normal(textureSample(normal_arr, normal_smp, uvx, 2).rgb, vec3(0.0, 0.0, -sx), up, vec3(sx, 0.0, 0.0)) * bw.x
            + planar_normal(textureSample(normal_arr, normal_smp, uvy, 2).rgb, tx, tz, up) * bw.y
            + planar_normal(textureSample(normal_arr, normal_smp, uvz, 2).rgb, vec3(sz, 0.0, 0.0), up, vec3(0.0, 0.0, sz)) * bw.z
        );
        // Rock pushes through where its own AO/height is high.
        let hc = clamp(w_cliff * 1.5 - 0.25 + (ca.r - arm.r) * 0.5, 0.0, 1.0);
        col = mix(col, cc, hc);
        arm = mix(arm, ca, hc);
        nrm = normalize(mix(nrm, cn, hc));
    }

    // Soften detail relief with distance so far hills don't sparkle.
    nrm = normalize(mix(nrm, n, far_t * 0.7));

    // Macro brightness/hue variation + wet darkening near water.
    col *= mix(0.82, 1.12, macro_n);
    col *= mix(1.0, 0.55, clamp(splat.b, 0.0, 1.0));

    pbr_input.material.base_color = vec4<f32>(col, 1.0);
    pbr_input.material.perceptual_roughness = clamp(arm.g * mix(1.0, 0.6, splat.b), 0.35, 1.0);
    pbr_input.material.metallic = 0.0;
    pbr_input.diffuse_occlusion *= vec3<f32>(mix(1.0, arm.r, 0.8));
    pbr_input.N = nrm;

    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
