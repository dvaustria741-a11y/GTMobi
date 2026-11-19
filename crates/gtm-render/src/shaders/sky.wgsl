// GTMobi sky — physical atmosphere, two-octave Rayleigh+Mie
// Calibrated against GTA 6 Florida reference:
//   zenith  sRGB ≈ (200,212,215)  linear ≈ (0.578,0.658,0.680)
//   horizon sRGB ≈ ( 99, 96, 82)  linear ≈ (0.125,0.117,0.084)
//   sun     sRGB ≈ (253,252,245)  — near-white warm noon
// Florida atmosphere is humid/hazy: Mie dominant, sky strongly desaturated.
// Output: ACES-tonemapped sRGB, matches pbr.wgsl.

struct Sky {
    inv_view_proj : mat4x4<f32>,
    cam_pos       : vec4<f32>,
    sun_dir       : vec4<f32>,   // xyz = unit toward sun
    horizon       : vec4<f32>,   // linear rgb (fed from gtm-lighting, blended in)
    zenith        : vec4<f32>,   // linear rgb
    sun_color     : vec4<f32>,   // rgb, w = intensity scale
}

@group(0) @binding(0) var<uniform> sky : Sky;

struct VOut {
    @builtin(position) clip : vec4<f32>,
    @location(0) ray        : vec3<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VOut {
    var p = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    let ndc = p[vi];
    let far = sky.inv_view_proj * vec4<f32>(ndc, 1.0, 1.0);
    var out: VOut;
    out.clip = vec4<f32>(ndc, 1.0, 1.0);
    out.ray  = far.xyz / far.w - sky.cam_pos.xyz;
    return out;
}

// ── ACES filmic (identical curve to pbr.wgsl, keeps sky+scene seam-free) ─────
fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3(0.0), vec3(1.0));
}

// ── Rayleigh phase ────────────────────────────────────────────────────────────
fn phase_rayleigh(cos_theta: f32) -> f32 {
    return (3.0 / (16.0 * 3.14159265)) * (1.0 + cos_theta * cos_theta);
}

// ── Mie phase (Henyey-Greenstein, g tuned for humid Florida haze) ─────────────
fn phase_mie(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom = 1.0 + g2 - 2.0 * g * cos_theta;
    return (1.0 - g2) / (4.0 * 3.14159265 * pow(max(denom, 0.0001), 1.5));
}

// ── Optical depth along a ray from surface to space ──────────────────────────
// 4-step numerical integration, good enough on mobile
fn optical_depth(ray_dir: vec3<f32>, scale_height: f32) -> f32 {
    let steps = 4u;
    let step_len = 1.0 / f32(steps);
    var depth = 0.0;
    for (var i = 0u; i < steps; i += 1u) {
        let t = (f32(i) + 0.5) * step_len;
        // Altitude above "sea level" of the sample point (approximate spherical shell)
        let altitude = max(ray_dir.y * t, 0.0);
        depth += exp(-altitude / scale_height) * step_len;
    }
    return depth;
}

// ── Main atmospheric inscattering ─────────────────────────────────────────────
fn atmosphere(ray: vec3<f32>, sun: vec3<f32>, intensity: f32) -> vec3<f32> {
    // Florida-tuned coefficients.
    // Rayleigh: standard air (slightly reduced — overcast haze flattens it)
    let beta_r = vec3<f32>(3.8e-3, 7.8e-3, 18.0e-3);   // per-unit path, linear RGB
    // Mie: elevated for humid subtropical haze (much higher than clear day)
    let beta_m = vec3<f32>(12.0e-3, 11.5e-3, 10.0e-3);  // warm-tinted haze
    let scale_h_r = 0.25;   // Rayleigh scale height (normalised 0..1 atmosphere)
    let scale_h_m = 0.08;   // Mie scale height (ground-hugging)
    let g_mie     = 0.72;   // Mie asymmetry — moderate forward scatter

    let r = normalize(ray);
    let cos_theta = dot(r, sun);

    // Optical depth from camera to "top of atmosphere"
    let od_r = optical_depth(r, scale_h_r);
    let od_m = optical_depth(r, scale_h_m);

    // Transmittance
    let T = exp(-(beta_r * od_r + beta_m * od_m));

    // Phase functions
    let p_r = phase_rayleigh(cos_theta);
    let p_m = phase_mie(cos_theta, g_mie);

    // In-scatter (single scattering, two octaves)
    // Ground-bounce: soft warm fill from below horizon
    let ground_lift = clamp(-r.y * 2.5, 0.0, 1.0);
    let ground_bounce = vec3<f32>(0.14, 0.12, 0.08) * ground_lift * 0.4;

    let scatter_r = beta_r * od_r * p_r;
    let scatter_m = beta_m * od_m * p_m;

    var color = (scatter_r + scatter_m) * T * intensity + ground_bounce;

    // Warm white balance shift (Florida humidity biases sky warm-grey)
    // Add a tiny constant warm floor so zenith doesn't go too blue
    let wb = vec3<f32>(0.025, 0.020, 0.012);
    color += wb * T;

    return color;
}

// ── Sun disc + aureole ────────────────────────────────────────────────────────
fn sun_contrib(ray: vec3<f32>, sun: vec3<f32>, intensity: f32, sun_rgb: vec3<f32>) -> vec3<f32> {
    let cos_a = dot(normalize(ray), sun);
    // Hard disc — subtends ~0.5° but we soften slightly for mobile aliasing
    let disc   = smoothstep(0.9995, 0.9999, cos_a) * intensity * 8.0;
    // Corona/aureole rings
    let corona = pow(max(cos_a, 0.0), 96.0) * intensity * 1.5
               + pow(max(cos_a, 0.0),  8.0) * intensity * 0.06;
    // Only above horizon
    let above = smoothstep(-0.04, 0.04, sun.y);
    return sun_rgb * (disc + corona) * above;
}

// ── Mie horizon haze band (Florida's signature creamy horizon) ────────────────
// A separate fog-like term that adds the warm tan band independent of sun pos
fn horizon_haze(ray: vec3<f32>, horizon_rgb: vec3<f32>) -> vec3<f32> {
    let r = normalize(ray);
    // Strongest right at horizon (y ≈ 0), falls off both up and down
    let band = exp(-abs(r.y) * 6.0);  // narrow-ish band
    // Also slightly forward-peaked (haze toward sun direction)
    return horizon_rgb * band * 0.55;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let ray = in.ray;   // length encodes distance — normalize for direction
    let r   = normalize(ray);
    let sun = normalize(sky.sun_dir.xyz);

    // Intensity from sun_color.w (driven by gtm-lighting keyframes)
    let intensity = max(sky.sun_color.w, 0.0);

    // Physical inscattering
    var color = atmosphere(r, sun, intensity);

    // Art-direction blend: physical scatter + keyframed palette
    // This lets gtm-lighting drive dusk/dawn without fighting the physics
    let art_zenith  = sky.zenith.rgb;
    let art_horizon = sky.horizon.rgb;
    let t_zenith  = smoothstep(0.0, 0.7, r.y);
    let art_color = mix(art_horizon, art_zenith, t_zenith);
    // At noon (intensity ~5) physics dominates; at dusk (intensity ~1) palette takes over
    let blend = clamp(intensity * 0.15, 0.0, 0.7);
    color = mix(art_color, color, blend);

    // Horizon haze band — always present, strength tied to fog
    color += horizon_haze(r, sky.horizon.rgb);

    // Sun disc + aureole
    color += sun_contrib(ray, sun, intensity, sky.sun_color.rgb);

    // Night stars — simple hash, only when sun is well below horizon
    let night_w = clamp(-sun.y * 4.0 - 0.5, 0.0, 1.0);
    if night_w > 0.01 {
        let uv = r.xz / (abs(r.y) + 0.001);
        let h  = fract(sin(dot(uv, vec2(127.1, 311.7))) * 43758.5453);
        color += vec3(step(0.997, h) * night_w * 0.6);
    }

    return vec4<f32>(aces(color), 1.0);
}
