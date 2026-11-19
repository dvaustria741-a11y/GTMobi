// GTMobi sky v2 — art-directed Florida atmosphere, calibrated to GTA 6 reference
// Measurement from GTA 6 reference video (noon, overcast-humid Florida):
//   zenith  sRGB=(200,212,215)  pre-ACES=(0.445,0.571,0.613)
//   horizon sRGB=( 99, 96, 82)  pre-ACES=(0.099,0.095,0.076)
//   sun     sRGB=(253,252,245)  near-white warm noon
// Key insight: Florida sky is Mie-dominant, nearly achromatic at zenith, warm-tan
// at horizon (ground dust/humidity scatter, NOT blue). Pure Rayleigh physics gives
// wrong answer — this uses a calibrated gradient + Mie haze band on top.
// ACES tonemap identical to pbr.wgsl → no sky/scene seam.

struct Sky {
    inv_view_proj : mat4x4<f32>,
    cam_pos       : vec4<f32>,
    sun_dir       : vec4<f32>,   // xyz = unit toward sun
    horizon       : vec4<f32>,   // linear rgb (from gtm-lighting keyframes)
    zenith        : vec4<f32>,   // linear rgb (from gtm-lighting keyframes)
    sun_color     : vec4<f32>,   // rgb, w = intensity
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

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3(0.0), vec3(1.0));
}

// Henyey-Greenstein phase — forward-scattering Mie
fn phase_mie(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / (4.0 * 3.14159265 * pow(max(1.0 + g2 - 2.0*g*cos_theta, 0.0001), 1.5));
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let r   = normalize(in.ray);
    let sun = normalize(sky.sun_dir.xyz);
    let intensity = max(sky.sun_color.w, 0.0);

    // ── 1. Base sky gradient — calibrated directly to GTA 6 target ───────────
    // Target pre-ACES: zenith=(0.445,0.571,0.613), horizon=(0.099,0.095,0.076)
    // The gtm-lighting zenith/horizon colors are art-directed per time of day,
    // but at noon they're wrong for a humid sky. We blend toward calibrated values.
    //
    // Noon calibrated sky (pre-ACES, before tonemapping):
    let sky_zenith_cal  = vec3<f32>(0.445, 0.571, 0.613);  // steel blue-grey
    let sky_horizon_cal = vec3<f32>(0.099, 0.095, 0.076);  // warm sandy tan

    // Art-direction palette from gtm-lighting drives dusk/dawn/night
    let art_zenith  = sky.zenith.rgb;
    let art_horizon = sky.horizon.rgb;

    // Weight: at high intensity (noon~5) → calibrated physics. At low (dusk~1) → art palette.
    // clamp to [0,1]: noon gives 1.0, dusk gives 0.2
    let cal_weight = clamp(intensity * 0.22 - 0.1, 0.0, 1.0);

    let zenith_color  = mix(art_zenith,  sky_zenith_cal,  cal_weight);
    let horizon_color = mix(art_horizon, sky_horizon_cal, cal_weight);

    // Gradient: smooth curve from horizon to zenith.
    // Florida sky is unusually uniform — shallow gradient, not deep blue at top.
    let t = smoothstep(-0.08, 0.65, r.y);  // start just below horizon, flatten fast
    var color = mix(horizon_color, zenith_color, t * t);  // t² = slower at top = more uniform

    // ── 2. Rayleigh blue tint — subtle, desaturated ───────────────────────────
    // Florida atmosphere: Rayleigh contributes blue tint at zenith only.
    // Keep it very weak — the sky is almost achromatic in reference footage.
    let rayleigh_blue = vec3<f32>(-0.02, 0.0, 0.08);  // remove red, add blue
    let rayleigh_weight = clamp(r.y * 0.6, 0.0, 0.3) * clamp(intensity * 0.1, 0.0, 1.0);
    color += rayleigh_blue * rayleigh_weight;

    // ── 3. Mie horizon haze — the creamy warm band ────────────────────────────
    // This is the dominant feature of a humid subtropical horizon.
    // A warm-tan band centered at y=0, spreading ~15° above and below.
    let cos_theta_sun = dot(r, sun);
    let mie_phase = phase_mie(cos_theta_sun, 0.76);  // forward scatter toward sun
    let horizon_band = exp(-r.y * r.y * 18.0);       // tight band at y=0
    // Haze color: warm sandy tan (the Florida humidity dust color)
    let haze_color = vec3<f32>(0.22, 0.19, 0.13);    // pre-ACES warm tan
    let haze_strength = horizon_band * (0.4 + mie_phase * 0.15) * clamp(intensity * 0.15, 0.1, 1.0);
    color += haze_color * haze_strength;

    // ── 4. Sun disc + aureole ─────────────────────────────────────────────────
    // Only above horizon; warm near-white at noon
    let above_h = smoothstep(-0.03, 0.05, sun.y);
    let disc    = smoothstep(0.9995, 0.9999, cos_theta_sun) * intensity * 10.0;
    let aureole = pow(max(cos_theta_sun, 0.0), 96.0) * intensity * 2.5
                + pow(max(cos_theta_sun, 0.0),  8.0) * intensity * 0.08;
    color += sky.sun_color.rgb * (disc + aureole) * above_h;

    // ── 5. Night stars ────────────────────────────────────────────────────────
    let night_w = clamp(-sun.y * 4.0 - 0.5, 0.0, 1.0);
    if night_w > 0.01 {
        let uv  = r.xz / (abs(r.y) + 0.001);
        let hsh = fract(sin(dot(uv, vec2(127.1, 311.7))) * 43758.5453);
        color  += vec3(step(0.997, hsh) * night_w * 0.6);
    }

    return vec4<f32>(aces(color), 1.0);
}
