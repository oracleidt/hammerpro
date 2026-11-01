override ALPHA_TEST: bool = false;
override BLEND_MODE: u32 = 0u;
override TINT_OPACITY: bool = false;
override SKY_3D: bool = false;

struct ClParams {
    mode: u32,
    scale: f32,
    pad: vec2<f32>,
};

@group(2) @binding(0) var cl_atlas: texture_2d_array<f32>;
@group(2) @binding(1) var<uniform> cl: ClParams;

const CL_WHITE: u32 = 0xffffffffu;
const CL_STATIC: u32 = 0xffffffffu;
const CL_BUMPED: u32 = 0x40000000u;
const CL_LAYER_MASK: u32 = 0xffffu;

struct ClOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) lm: vec2<f32>,
    @location(3) alpha: f32,
    @location(4) @interpolate(flat) layer: u32,
    @location(5) @interpolate(flat) state: vec2<u32>,
    @location(6) @interpolate(flat) styles: u32,
    @location(7) @interpolate(flat) stride: f32,
    @location(8) @interpolate(flat) cube: u32,
    @location(9) normal: vec3<f32>,
    @location(10) blend: vec4<f32>,
};

@vertex
fn vs_compiled(
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) lm: vec2<f32>,
    @location(3) alpha: f32,
    @location(4) slot: u32,
    @location(5) layer: u32,
    @location(6) styles: u32,
    @location(7) stride: f32,
    @location(8) m0: vec4<f32>,
    @location(9) m1: vec4<f32>,
    @location(10) m2: vec4<f32>,
    @location(11) m3: vec4<f32>,
    @location(12) cube: u32,
    @location(13) normal: vec3<f32>,
    @location(14) blend: u32,
) -> ClOut {
    let st = object_state(slot);
    let m = mat4x4<f32>(m0, m1, m2, m3);
    let world = (m * vec4<f32>(pos, 1.0)).xyz;
    var out: ClOut;
    out.world = world;
    out.cube = cube & 0xffffu;
    out.normal = (m * vec4<f32>(normal, 0.0)).xyz;
    out.uv = uv;
    out.lm = lm;
    out.alpha = alpha;
    out.blend = unpack_blend_layers(blend);
    out.layer = layer;
    out.styles = styles;
    out.stride = stride;
    out.state = vec2<u32>(st.flags, st.tint);
    let tint = unpack_srgba(st.tint);
    if (!SKY_3D && hidden_in_view(st.flags)) || (BLEND_MODE == 0u && tint.a < 0.999) {
        out.clip = COLLAPSED;
    } else {
        out.clip = view.view_proj * vec4<f32>(world, 1.0);
    }
    return out;
}

fn cl_image(in: ClOut, b: u32) -> vec3<f32> {
    let layer = i32(in.layer & CL_LAYER_MASK);
    if in.styles == CL_STATIC {
        let at = in.lm + vec2<f32>(f32(b) * in.stride, 0.0);
        return textureSampleLevel(cl_atlas, samp_clamp, at, layer, 0.0).rgb;
    }
    let per = select(1u, 4u, (in.layer & CL_BUMPED) != 0u);
    var sum = vec3<f32>(0.0);
    for (var k = 0u; k < 4u; k++) {
        let s = (in.styles >> (k * 8u)) & 255u;
        if s == 255u {
            break;
        }
        let v = light_style(s);
        if v != 0.0 {
            let at = in.lm + vec2<f32>(f32(k * per + b) * in.stride, 0.0);
            sum += textureSampleLevel(cl_atlas, samp_clamp, at, layer, 0.0).rgb * v;
        }
    }
    if cl.mode == 1u {
        return pow(clamp(sum, vec3<f32>(0.0), vec3<f32>(4091.0 / 1024.0)), vec3<f32>(1.0 / 2.2));
    }
    return sum;
}

fn cl_light(in: ClOut) -> vec3<f32> {
    if cl.mode == 0u || in.layer == CL_WHITE {
        return vec3<f32>(1.0);
    }
    return cl_image(in, 0u);
}

fn cl_bumped(in: ClOut) -> bool {
    return cl.mode != 0u && in.layer != CL_WHITE && (in.layer & CL_BUMPED) != 0u;
}

fn cl_light_bumped(in: ClOut, texel: vec3<f32>, detail: vec3<f32>) -> vec3<f32> {
    var w: vec3<f32>;
    if (material.bump_flags & B_SS) != 0u {
        w = texel;
    } else {
        var dp = clamp(vec3<f32>(dot(texel, BUMP_BASIS_0), dot(texel, BUMP_BASIS_1), dot(texel, BUMP_BASIS_2)),
            vec3<f32>(0.0), vec3<f32>(1.0));
        dp *= dp;
        if material.detail_mode == 10u {
            dp *= 2.0 * detail;
        }
        let sum = dp.x + dp.y + dp.z;
        w = select(vec3<f32>(1.0 / 3.0), dp / sum, sum > 1e-6);
    }
    return w.x * cl_image(in, 1u) + w.y * cl_image(in, 2u) + w.z * cl_image(in, 3u);
}

fn cl_shade(albedo: vec3<f32>, light: vec3<f32>) -> vec3<f32> {
    return cl_shade_env(albedo, light, vec3<f32>(0.0));
}

fn cl_shade_env(albedo: vec3<f32>, light: vec3<f32>, env: vec3<f32>) -> vec3<f32> {
    if cl.mode == 1u {
        return srgb_to_linear(min(linear_to_srgb(albedo) * light + env, vec3<f32>(1.0)));
    }
    if cl.mode == 2u {
        return (albedo * light + env) * bitcast<f32>(view.misc.z);
    }
    return albedo + env;
}

@fragment
fn fs_compiled(in: ClOut) -> @location(0) vec4<f32> {
    let uv = in.uv * material.inv_tex_size + material.scroll * view.sky.z;
    var albedo = textureSample(tex, samp, uv);
    var cover = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    if material.has_tex2 != 0u {
        let b = textureSample(tex2, samp, uv);
        if material.blend4 != 0u {
            let fw = four_way_blend(uv, blend_layers(in.blend, in.alpha));
            albedo = fw.color;
            cover = fw.cover;
        } else {
            albedo = vec4<f32>(mix(albedo.rgb, b.rgb, in.alpha), albedo.a);
        }
    }
    let detail = detail_sample(uv);
    if material.detail_mode != DETAIL_NONE {
        albedo = detail_combine(albedo, detail);
    }
    var bump = vec3<f32>(0.0, 0.0, 1.0);
    let bumped = bump_on();
    if bumped {
        bump = bump_texel(in.uv * material.inv_tex_size, in.alpha, cover);
    }
    let nl = length(in.normal);
    let n = select(vec3<f32>(0.0, 0.0, 1.0), in.normal / nl, nl > 1e-5);
    var en = n;
    if bumped {
        let frame = uv_frame(n, in.world, in.uv * material.inv_tex_size);
        let bv = bump_to_world(bump_vector(bump), frame, n);
        let bl = length(bv);
        en = select(n, bv / bl, bl > 1e-5);
    }
    var env_raw = vec3<f32>(0.0);
    if env_on() {
        env_raw = env_reflection(in.world, en, in.cube, in.uv * material.inv_tex_size, albedo.a, in.alpha,
            vec3<f32>(1.0), cl.mode == 1u);
    }
    let glow = self_illum_mask(albedo.a, in.uv * material.inv_tex_size);
    albedo.a = material_texture_alpha(albedo.a);
    albedo = albedo * material.tint * unpack_srgba(in.state.y);
    if alpha_tested_out(ALPHA_TEST, albedo.a) {
        discard;
    }
    let unlit = (material.flags & M_UNLIT) != 0u;
    var light = vec3<f32>(1.0);
    if bumped && cl_bumped(in) {
        light = cl_light_bumped(in, bump, detail.rgb);
    } else {
        light = cl_light(in);
    }
    let env = env_raw * mix(vec3<f32>(1.0), clamp(light, vec3<f32>(0.0), vec3<f32>(1.0)), material.env_sat.w);
    var rgb = cl_shade_env(albedo.rgb, select(light, vec3<f32>(1.0), unlit), env);
    rgb = mix(rgb, cl_shade(albedo.rgb * material.self_illum.rgb, vec3<f32>(1.0)), glow);
    if material.detail_mode != DETAIL_NONE {
        rgb = detail_post_lighting(rgb, detail);
    }
    let flags = in.state.x;
    if (flags & F_INSTANCE) != 0u {
        rgb = mix(rgb, INSTANCE_RGB * (0.4 + dot(rgb, vec3<f32>(0.3, 0.59, 0.11))), 0.45);
    }
    if (flags & F_INVALID) != 0u {
        rgb = mix(rgb, INVALID_RGB, 0.5);
    }
    if (flags & F_SELECTED) != 0u {
        rgb = mix(rgb, view.selection.rgb, view.selection.a);
    }
    if (flags & F_HOVERED) != 0u {
        rgb = mix(rgb, vec3<f32>(1.0), 0.18);
    }
    rgb = mix(rgb, fog_color(in.world), fog_amount(in.world));
    if BLEND_MODE == 0u {
        return vec4<f32>(rgb, 1.0);
    }
    rgb = min(rgb, vec3<f32>(1.0));
    var a = clamp(albedo.a * material.alpha_scale, 0.0, 1.0);
    if TINT_OPACITY {
        a = unpack_srgba(in.state.y).a;
    }
    if BLEND_MODE == 2u {
        return vec4<f32>(rgb * a, 0.0);
    }
    return vec4<f32>(rgb, a);
}
