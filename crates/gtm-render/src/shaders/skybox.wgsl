// GTMobi skybox — procedural Florida atmosphere
// Driven by sun_direction + time-of-day palette from gtm-lighting
// No cubemap required — pure math sky matching GTA 6 Leonida look

struct CameraUniform {
    view_proj : mat4x4<f32>,
    view      : mat4x4<f32>,
    view_pos  : vec4<f32>,
}

struct SkyUniform {
    sun_direction : vec4<f32>,  // xyz = normalised toward sun, w = time 0-1
    horizon_color : vec4<f32>,  // Florida haze (orange/pink at dusk, blue noon)
    zenith_color  : vec4<f32>,  // deep blue above
    sun_color     : vec4<f32>,  // disc + bloom colour
    fog_density   : f32,
    _pad          : vec3<f32>,
}

@group(0) @binding(0) var<uniform> camera : CameraUniform;
@group(1) @binding(0) var<uniform> sky    : SkyUniform;

struct VOut {
    @builtin(position) clip  : vec4<f32>,
    @location(0)       ray_d : vec3<f32>,
}

// Full-screen triangle — no vertex buffer needed
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VOut {
    var pos = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    let p = pos[vi];

    // Reconstruct view-space ray from NDC, strip translation from view
    let inv_proj = /* approximation — proper: pass inv matrices */
        mat4x4<f32>(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(0.0, 0.0, 0.0, 1.0),
        );

    // Extract rotation-only view (strip translation column)
    var rot = camera.view;
    rot[3] = vec4(0.0, 0.0, 0.0, 1.0);

    let ndc_near = vec4(p.x, p.y, 0.0, 1.0);
    let view_ray = rot * ndc_near;

    return VOut(vec4(p, 0.9999, 1.0), view_ray.xyz);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn sky_gradient(ray: vec3<f32>) -> vec3<f32> {
    let up      = normalize(ray);
    let horizon = 1.0 - clamp(up.y * 2.5, 0.0, 1.0);
    return mix(sky.zenith_color.rgb, sky.horizon_color.rgb, horizon * horizon);
}

fn sun_disc(ray: vec3<f32>, sun_dir: vec3<f32>) -> f32 {
    let cos_angle = dot(normalize(ray), sun_dir);
    let disc      = smoothstep(0.9995, 0.9999, cos_angle);
    let bloom     = pow(max(cos_angle, 0.0), 32.0) * 0.4;
    return disc + bloom;
}

fn mie_scattering(ray: vec3<f32>, sun_dir: vec3<f32>) -> f32 {
    let cos_theta = dot(normalize(ray), sun_dir);
    let g         = 0.76;
    let g2        = g * g;
    return (1.0 - g2) / (4.0 * 3.14159 * pow(1.0 + g2 - 2.0 * g * cos_theta, 1.5));
}

// Florida haze — warm peach/pink tint near horizon
fn haze(ray: vec3<f32>) -> f32 {
    let horizon_weight = 1.0 - abs(normalize(ray).y);
    return pow(horizon_weight, 4.0) * sky.fog_density;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let ray     = normalize(in.ray_d);
    let sun_dir = sky.sun_direction.xyz;

    var color = sky_gradient(ray);

    // Mie (hazy glow around sun, strong near horizon)
    let mie   = mie_scattering(ray, sun_dir) * 0.015;
    color    += sky.sun_color.rgb * mie;

    // Sun disc + bloom
    let disc  = sun_disc(ray, sun_dir);
    color    += sky.sun_color.rgb * disc;

    // Florida horizon haze (peachy warm band)
    let h_col = vec3<f32>(1.0, 0.72, 0.42) * sky.sun_color.a; // intensity from time
    color     = mix(color, h_col, haze(ray));

    // Night stars — simple hash noise, only when sun is low
    let night_weight = clamp(-sun_dir.y * 3.0, 0.0, 1.0);
    if night_weight > 0.01 {
        let star_uv  = ray.xz / (ray.y + 0.001);
        let hash     = fract(sin(dot(star_uv, vec2(127.1, 311.7))) * 43758.5);
        let star     = step(0.998, hash) * night_weight * 0.8;
        color       += vec3(star);
    }

    return vec4<f32>(color, 1.0);
}
