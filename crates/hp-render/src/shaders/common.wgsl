struct View {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    forward: vec4<f32>,
    viewport: vec4<f32>,
    selection: vec4<f32>,
    grid_a: vec4<f32>,
    grid_b: vec4<f32>,
    ortho: vec4<f32>,
    ambient: vec4<f32>,
    cluster_dims: vec4<u32>,
    cluster_z: vec4<f32>,
    misc: vec4<u32>,
    shadow_vp: mat4x4<f32>,
    shadow: vec4<f32>,
    opts: vec4<u32>,
    grid3d: vec4<f32>,
    fog_a: vec4<f32>,
    fog_b: vec4<f32>,
    fog_c: vec4<f32>,
    sky: vec4<f32>,
    sky3d: vec4<f32>,
    proj: vec4<f32>,
    sky_uv: array<vec4<f32>, 6>,
    sky_vis_a: vec4<f32>,
    sky_vis_b: vec4<f32>,
    sun_soft: vec4<f32>,
    sky_paint: array<vec4<f32>, 6>,
    indirect_a: vec4<f32>,
    indirect_b: vec4<f32>,
    sky_march: array<vec4<f32>, 18>,
    control_vp: mat4x4<f32>,
    control_params: vec4<f32>,
    control_color: vec4<f32>,
};

struct ObjState {
    flags: u32,
    color: u32,
    tint: u32,
    classic: u32,
};

struct Light {
    pos_range: vec4<f32>,
    color: vec4<f32>,
    dir: vec4<f32>,
    cone: vec4<f32>,
    atten: vec4<f32>,
    extra: vec4<f32>,
};

fn spot_cone(l: Light, s: f32) -> f32 {
    if l.extra.y < 0.5 {
        return smoothstep(l.cone.y, l.cone.x, s);
    }
    if s <= l.cone.y {
        return 0.0;
    }
    var m = 1.0;
    if s < l.cone.x {
        m = clamp((s - l.cone.y) / max(l.cone.x - l.cone.y, 1e-6), 0.0, 1.0);
        if l.extra.x != 0.0 && l.extra.x != 1.0 {
            m = pow(m, l.extra.x);
        }
    }
    return m * s;
}

fn light_falloff(l: Light, dist: f32, model: bool) -> f32 {
    if model && l.dir.w > 0.0 && dist > l.dir.w {
        return 0.0;
    }
    let d = min(max(dist, 1.0), max(l.atten.w, 1.0));
    let denom = l.atten.x + d * (l.atten.y + d * l.atten.z);
    var f = 1.0;
    if denom > 1e-12 {
        f = 1.0 / denom;
    }
    if l.cone.w > l.cone.z {
        if dist > l.cone.w {
            return 0.0;
        }
        let t = 1.0 - clamp((max(dist, 1.0) - l.cone.z) / (l.cone.w - l.cone.z), 0.0, 1.0);
        f *= t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    }
    return f;
}

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<storage, read> objects: array<ObjState>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var samp_clamp: sampler;
@group(0) @binding(4) var<storage, read> lights: array<Light>;
@group(0) @binding(5) var<storage, read> clusters: array<vec2<u32>>;
@group(0) @binding(6) var<storage, read> light_indices: array<u32>;
@group(0) @binding(22) var control_map: texture_depth_2d_array;
@group(0) @binding(7) var shadow_map: texture_depth_2d_array;
@group(0) @binding(8) var shadow_samp: sampler_comparison;
@group(0) @binding(9) var<storage, read> model_lights: array<vec4<f32>>;
@group(0) @binding(10) var<storage, read> model_colors: array<u32>;
@group(0) @binding(11) var<uniform> light_styles: array<vec4<f32>, 32>;

struct Projector {
    pos_near: vec4<f32>,
    fwd_far: vec4<f32>,
    right_tan: vec4<f32>,
    up_layer: vec4<f32>,
    color_const: vec4<f32>,
    lin_quad: vec4<f32>,
    shadow_vp: mat4x4<f32>,
};

@group(0) @binding(19) var<storage, read> projectors: array<Projector, 16>;
@group(0) @binding(20) var proj_tex: texture_2d_array<f32>;
@group(0) @binding(21) var proj_shadow: texture_depth_2d_array;

fn projector_light(world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    for (var i = 0u; i < 16u; i++) {
        let p = projectors[i];
        if p.lin_quad.z < 0.5 {
            continue;
        }
        let d = world - p.pos_near.xyz;
        let z = dot(d, p.fwd_far.xyz);
        if z <= p.pos_near.w || z >= p.fwd_far.w {
            continue;
        }
        let inv = 1.0 / (z * p.right_tan.w);
        let sx = dot(d, p.right_tan.xyz) * inv;
        let sy = dot(d, p.up_layer.xyz) * inv;
        if abs(sx) >= 1.0 || abs(sy) >= 1.0 {
            continue;
        }
        var tex = vec3<f32>(1.0);
        if p.up_layer.w >= 0.0 {
            tex = textureSampleLevel(proj_tex, samp_clamp, vec2<f32>(sx * 0.5 + 0.5, 0.5 - sy * 0.5), i32(p.up_layer.w), 0.0).rgb;
        }
        let dist = max(length(d), 1.0);
        let ndl = max(dot(n, -d / dist), 0.0);
        var vis = 1.0;
        if p.lin_quad.w >= 0.0 {
            let c = p.shadow_vp * vec4<f32>(world + n * 1.5, 1.0);
            let q = c.xyz / max(c.w, 1e-6);
            let suv = vec2<f32>(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5);
            if suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0 {
                vis = textureSampleCompareLevel(proj_shadow, shadow_samp, suv, i32(p.lin_quad.w), q.z - 0.0004);
            }
        }
        let att = clamp((p.fwd_far.w - dist) / (0.4 * p.fwd_far.w), 0.0, 1.0)
            * clamp(p.color_const.w + (p.lin_quad.x + p.lin_quad.y / dist) / dist, 0.0, 1.0);
        sum += p.color_const.rgb * tex * (ndl * att * vis);
    }
    return sum;
}

@group(0) @binding(12) var cube_maps: texture_cube_array<f32>;
@group(0) @binding(13) var<storage, read> cube_info: array<vec4<f32>>;

@group(0) @binding(14) var local_shadow_maps: texture_depth_2d_array;
@group(0) @binding(15) var sky_map: texture_depth_2d_array;
@group(0) @binding(16) var sun_cut_map: texture_depth_2d_array;
@group(0) @binding(17) var sky_cut_map: texture_depth_2d_array;
@group(0) @binding(18) var<storage, read> indirect_buf: array<vec4<u32>>;

fn light_style(s: u32) -> f32 {
    let i = min(s, 127u);
    return light_styles[i / 4u][i % 4u];
}

const F_SELECTED: u32 = 1u;
const F_HIDDEN: u32 = 2u;
const F_HOVERED: u32 = 4u;
const F_NO_3D: u32 = 8u;
const F_NO_2D: u32 = 16u;
const F_ALWAYS_EDGES: u32 = 32u;
const F_INVALID: u32 = 64u;
const F_DIMMED: u32 = 128u;
const F_INSTANCE: u32 = 256u;
const F_SKY_3D: u32 = 512u;
const F_ROPE: u32 = 1024u;
const F_GAME_HIDDEN: u32 = 2048u;
const F_DISPLACEMENT: u32 = 4096u;
const F_NO_GAME_WIRE: u32 = 8192u;
const F_BRUSH_ENTITY: u32 = 16384u;
const F_COMPILED_LIGHT: u32 = 32768u;
const F_NO_PROBE: u32 = 65536u;
const F_NO_SHADOW: u32 = 131072u;

const OPT_WALKABLE: u32 = 1u;
const OPT_FOG: u32 = 2u;
const OPT_GRID_3D: u32 = 8u;
const OPT_SKY_PASS: u32 = 16u;
const OPT_MODEL_FADE: u32 = 32u;
const OPT_NEUTRAL_FLAT: u32 = 64u;
const OPT_DETAIL_SELECTED: u32 = 128u;
const OPT_DETAIL_NONE: u32 = 256u;
const OPT_WALK_MARKS: u32 = 512u;
const OPT_SOURCE_2D: u32 = 1024u;
const OPT_GAME_WIRE: u32 = 2048u;
const OPT_COMPILED_LIGHT: u32 = 4096u;
const OPT_COMPILED_FULLBRIGHT: u32 = 8192u;
const OPT_NORMAL_MAPS: u32 = 16384u;
const OPT_CUBEMAPS: u32 = 32768u;
const OPT_PROBE: u32 = 65536u;
const OPT_SHADOW_PASS: u32 = 131072u;
const OPT_MODEL_LIGHT: u32 = 262144u;
const OPT_SUN_PASS: u32 = 524288u;
const OPT_SKY_CUT: u32 = 1048576u;
const OPT_SKY_FACES: u32 = 2097152u;
const OPT_REFLECT_WORLD: u32 = 4194304u;
const OPT_REFLECT_SKY: u32 = 8388608u;
const OPT_CLASSIC_COLORS: u32 = 16777216u;
const OPT_LIGHT_BACKGROUND: u32 = 33554432u;
const OPT_SKY_PAINT: u32 = 67108864u;
const OPT_HDR_TINT: u32 = 134217728u;
const OPT_MODEL_LODS: u32 = 268435456u;

const EDGE_DETAIL_BIT: u32 = 0x80000000u;
const EDGE_SOURCE_BIT: u32 = 0x40000000u;
const EDGE_CLASS_SHIFT: u32 = 29u;
const EDGE_SLOT_MASK: u32 = 0x1fffffffu;

const MODE_TEXTURED: u32 = 0u;
const MODE_FLAT: u32 = 1u;
const MODE_WIREFRAME: u32 = 2u;
const MODE_TEXTURED_WIRE: u32 = 3u;
const MODE_LIT: u32 = 4u;
const MODE_UNSHADED: u32 = 5u;
const MODE_SMOOTHING: u32 = 6u;

const INVALID_RGB: vec3<f32> = vec3<f32>(1.0, 0.05, 0.05);
const INSTANCE_RGB: vec3<f32> = vec3<f32>(0.35, 0.75, 1.0);

const COLLAPSED: vec4<f32> = vec4<f32>(2.0, 2.0, 2.0, 1.0);

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn unpack_srgba(c: u32) -> vec4<f32> {
    let v = unpack4x8unorm(c);
    return vec4<f32>(srgb_to_linear(v.rgb), v.a);
}

fn state_color(st: ObjState) -> u32 {
    if (view.opts.x & OPT_CLASSIC_COLORS) != 0u && st.classic != 0u {
        return st.classic;
    }
    return st.color;
}

fn object_state(slot: u32) -> ObjState {
    if slot >= arrayLength(&objects) {
        return ObjState(0u, 0xffffffffu, 0xffffffffu, 0u);
    }
    return objects[slot];
}

fn is_ortho() -> bool {
    return view.eye.w > 0.5;
}

fn hidden_in_view(flags: u32) -> bool {
    let mask = select(F_NO_3D, F_NO_2D, is_ortho());
    let probing = (view.opts.x & OPT_PROBE) != 0u;
    let probe = probing && (flags & F_COMPILED_LIGHT) != 0u;
    let shadow_pass = (view.opts.x & OPT_SHADOW_PASS) != 0u;
    let sun_pass = (view.opts.x & OPT_SUN_PASS) != 0u;
    let sky_faces = (view.opts.x & OPT_SKY_FACES) != 0u;
    let hidden = select(F_HIDDEN, 0u, probe) | select(0u, F_NO_PROBE, probing) | select(0u, F_NO_SHADOW | F_ROPE, shadow_pass)
        | select(0u, F_SKY_3D, sun_pass) | select(0u, F_BRUSH_ENTITY, sky_faces || (view.opts.x & OPT_REFLECT_WORLD) != 0u);
    if (flags & (hidden | mask)) != 0u {
        return true;
    }
    if (view.opts.x & OPT_REFLECT_SKY) != 0u && (flags & F_SKY_3D) == 0u {
        return true;
    }
    if (view.opts.x & OPT_GAME_WIRE) != 0u && (flags & F_DISPLACEMENT) != 0u {
        return true;
    }
    return (view.opts.x & OPT_SKY_PASS) != 0u && (flags & F_SKY_3D) == 0u;
}

fn fog_on() -> bool {
    return (view.opts.x & OPT_FOG) != 0u && !is_ortho();
}

fn fog_depth(world: vec3<f32>) -> f32 {
    return dot(world - view.eye.xyz, view.forward.xyz);
}

fn fog_far_clip(world: vec3<f32>) {
    if fog_on() && view.proj.z > 0.0 && fog_depth(world) > view.proj.z {
        discard;
    }
}

fn fog_amount(world: vec3<f32>) -> f32 {
    if !fog_on() {
        return 0.0;
    }
    let start = view.fog_b.w;
    let end = max(view.fog_c.w, start + 1.0);
    return clamp((fog_depth(world) - start) / (end - start), 0.0, view.fog_a.w);
}

fn fog_gamma(c: vec3<f32>) -> vec3<f32> {
    let v = max(c, vec3<f32>(0.0));
    let lo = v * 12.92;
    let hi = 1.055 * pow(v, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(hi, lo, v <= vec3<f32>(0.0031308));
}

fn fog_blend(rgb: vec3<f32>, target_gamma: vec3<f32>, f: f32) -> vec3<f32> {
    if f <= 0.0 {
        return rgb;
    }
    return srgb_to_linear(mix(fog_gamma(rgb), target_gamma, f));
}

fn apply_fog(rgb: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    fog_far_clip(world);
    return fog_blend(rgb, view.fog_a.rgb, fog_amount(world));
}

fn apply_fog_black(rgb: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    fog_far_clip(world);
    return fog_blend(rgb, vec3<f32>(0.0), fog_amount(world));
}
