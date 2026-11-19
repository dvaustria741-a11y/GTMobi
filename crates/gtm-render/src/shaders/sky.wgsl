// GTMobi sky: full-screen triangle, gradient from horizon to zenith, sun disc and glow.
// Output is tonemapped with the same ACES curve as pbr.wgsl so distance fog (which mixes toward
// the horizon color before tonemapping) meets the sky without a seam. The surface is sRGB.

struct Sky {
    inv_view_proj : mat4x4<f32>,
    cam_pos       : vec4<f32>,
    sun_dir       : vec4<f32>,  // xyz = unit vector toward the sun
    horizon       : vec4<f32>,  // linear rgb
    zenith        : vec4<f32>,  // linear rgb
    sun_color     : vec4<f32>,  // rgb, w = intensity
}

@group(0) @binding(0) var<uniform> sky : Sky;

struct VOut {
    @builtin(position) clip : vec4<f32>,
    @location(0) ray        : vec3<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let ndc = p[vi];
    let far = sky.inv_view_proj * vec4<f32>(ndc, 1.0, 1.0);
    var out: VOut;
    out.clip = vec4<f32>(ndc, 1.0, 1.0);
    out.ray = far.xyz / far.w - sky.cam_pos.xyz;
    return out;
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3(0.0), vec3(1.0));
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let ray = normalize(in.ray);
    let sun = normalize(sky.sun_dir.xyz);

    let t = smoothstep(0.0, 0.65, ray.y);
    var color = mix(sky.horizon.rgb, sky.zenith.rgb, t);

    let cos_sun = dot(ray, sun);
    let disc = smoothstep(0.9996, 0.9999, cos_sun);
    let glow = pow(max(cos_sun, 0.0), 48.0) * 0.5 + pow(max(cos_sun, 0.0), 6.0) * 0.08;
    let above = smoothstep(-0.05, 0.05, sun.y);
    color += sky.sun_color.rgb * (disc * 6.0 + glow) * above;

    return vec4<f32>(aces(color), 1.0);
}
