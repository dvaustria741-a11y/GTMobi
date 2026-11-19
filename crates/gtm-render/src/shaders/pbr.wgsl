// GTMobi PBR shader — GTA 6 visual target
// Cook-Torrance specular | Lambertian diffuse | ACES tonemap | Florida warm/teal grade

const PI: f32 = 3.14159265358979;

// GTA 6 Florida color grading
// Shadow: blue-teal (sky reflected in dark areas)
// Highlight: warm golden-orange (Florida sun)
const GRADE_SHADOW_TINT: vec3<f32>    = vec3<f32>(0.82, 0.90, 1.08);
const GRADE_HIGHLIGHT_TINT: vec3<f32> = vec3<f32>(1.10, 1.00, 0.86);

// ─── Bind groups ────────────────────────────────────────────────────────────
struct CameraUniform {
    view_proj   : mat4x4<f32>,
    view        : mat4x4<f32>,
    view_pos    : vec4<f32>,
}

struct LightUniform {
    direction   : vec4<f32>,  // xyz = dir to light, w = intensity
    color       : vec4<f32>,  // xyz = color, w = unused
    ambient     : vec4<f32>,  // xyz = ambient color, w = strength
    fog         : vec4<f32>,  // xyz = fog color (linear), w = density per meter
}

struct ModelUniform {
    model       : mat4x4<f32>,
    normal_mat  : mat4x4<f32>,
}

struct MaterialUniform {
    base_color_factor   : vec4<f32>,
    metallic_roughness  : vec4<f32>, // x=metallic y=roughness z=occlusion w=_
    emissive_factor     : vec3<f32>,
    flags               : u32,
}

@group(0) @binding(0) var<uniform> camera   : CameraUniform;
@group(1) @binding(0) var<uniform> light    : LightUniform;
@group(2) @binding(0) var<uniform> model    : ModelUniform;
@group(3) @binding(0) var<uniform> material : MaterialUniform;
@group(3) @binding(1) var albedo_tex        : texture_2d<f32>;
@group(3) @binding(2) var albedo_samp       : sampler;
@group(3) @binding(3) var normal_tex        : texture_2d<f32>;
@group(3) @binding(4) var normal_samp       : sampler;
@group(3) @binding(5) var mr_tex            : texture_2d<f32>;
@group(3) @binding(6) var mr_samp           : sampler;

// ─── Vertex ─────────────────────────────────────────────────────────────────
struct VertexInput {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) uv       : vec2<f32>,
    @location(3) tangent  : vec4<f32>,
    @location(4) color    : vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos     : vec4<f32>,
    @location(0)       world_pos    : vec3<f32>,
    @location(1)       world_normal : vec3<f32>,
    @location(2)       uv           : vec2<f32>,
    @location(3)       tangent      : vec3<f32>,
    @location(4)       bitangent    : vec3<f32>,
    @location(5)       vert_color   : vec4<f32>,
}

@vertex
fn vs_main(v: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = model.model * vec4<f32>(v.position, 1.0);
    out.clip_pos     = camera.view_proj * world;
    out.world_pos    = world.xyz;
    out.world_normal = normalize((model.normal_mat * vec4<f32>(v.normal, 0.0)).xyz);
    let wt           = normalize((model.model * vec4<f32>(v.tangent.xyz, 0.0)).xyz);
    out.tangent      = wt;
    out.bitangent    = cross(out.world_normal, wt) * v.tangent.w;
    out.uv           = v.uv;
    out.vert_color   = v.color;
    return out;
}

// ─── PBR helpers ────────────────────────────────────────────────────────────
fn d_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a  = roughness * roughness;
    let a2 = a * a;
    let d  = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

fn g_schlick(n_dot_x: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return n_dot_x / (n_dot_x * (1.0 - k) + k);
}

fn g_smith(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, roughness: f32) -> f32 {
    return g_schlick(max(dot(n, v), 0.0), roughness)
         * g_schlick(max(dot(n, l), 0.0), roughness);
}

fn fresnel(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// ACES fitted — Narkowicz 2015
fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3(0.0), vec3(1.0));
}

// GTA 6 Florida split-toning: warm highlights, blue-teal shadows
fn grade(color: vec3<f32>) -> vec3<f32> {
    let lum = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    let shadow_mix    = (1.0 - smoothstep(0.0, 0.5, lum)) * 0.35;
    let highlight_mix = smoothstep(0.5, 1.0, lum) * 0.25;
    return color
        * mix(vec3(1.0), GRADE_SHADOW_TINT, shadow_mix)
        * mix(vec3(1.0), GRADE_HIGHLIGHT_TINT, highlight_mix);
}

// ─── Fragment ────────────────────────────────────────────────────────────────
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // --- albedo
    var albedo   = material.base_color_factor.rgb * in.vert_color.rgb;
    if (material.flags & 1u) != 0u {
        albedo  *= textureSample(albedo_tex, albedo_samp, in.uv).rgb;
    }

    // --- normal
    var n = normalize(in.world_normal);
    if (material.flags & 2u) != 0u {
        let tbn    = mat3x3<f32>(in.tangent, in.bitangent, in.world_normal);
        let n_map  = textureSample(normal_tex, normal_samp, in.uv).rgb * 2.0 - 1.0;
        n = normalize(tbn * n_map);
    }

    // --- metallic / roughness
    var metallic  = material.metallic_roughness.x;
    var roughness = material.metallic_roughness.y;
    if (material.flags & 4u) != 0u {
        let mr     = textureSample(mr_tex, mr_samp, in.uv);
        metallic  *= mr.b;
        roughness *= mr.g;
    }
    roughness = max(roughness, 0.045); // prevent specular blowup

    let v   = normalize(camera.view_pos.xyz - in.world_pos);
    let l   = normalize(-light.direction.xyz);
    let h   = normalize(v + l);
    let f0  = mix(vec3<f32>(0.04), albedo, metallic);

    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 0.0);
    let n_dot_h = max(dot(n, h), 0.0);

    // Cook-Torrance
    let D   = d_ggx(n_dot_h, roughness);
    let G   = g_smith(n, v, l, roughness);
    let F   = fresnel(max(dot(h, v), 0.0), f0);
    let ks  = F;
    let kd  = (vec3(1.0) - ks) * (1.0 - metallic);

    let radiance = light.color.rgb * light.direction.w;
    let brdf_spec = D * G * F / (4.0 * n_dot_v * n_dot_l + 0.00001);
    let lo       = (kd * albedo / PI + brdf_spec) * radiance * n_dot_l;

    // Ambient
    let ambient = light.ambient.rgb * light.ambient.w * albedo;

    // Emissive (neon signs, city lights)
    var emissive = vec3<f32>(0.0);
    if (material.flags & 8u) != 0u {
        emissive = material.emissive_factor;
    }

    var color = ambient + lo + emissive;

    // Exponential distance fog toward the horizon color (before tonemapping).
    let dist = length(camera.view_pos.xyz - in.world_pos);
    color = mix(color, light.fog.rgb, 1.0 - exp(-dist * light.fog.w));

    // GTA 6 color grade
    color = grade(color);

    // ACES tonemap
    color = aces(color);

    // No manual gamma: the renderer selects an sRGB surface format, which encodes on write.

    return vec4<f32>(color, 1.0);
}
