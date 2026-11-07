struct Material {
    tint: vec4<f32>,
    inv_tex_size: vec2<f32>,
    scroll: vec2<f32>,
    alpha_scale: f32,
    flags: u32,
    has_tex2: u32,
    blend4: u32,
    alpha_ref: f32,
    detail_mode: u32,
    detail_flags: u32,
    pad: u32,
    detail_uv: vec4<f32>,
    detail_tint: vec4<f32>,
    bump_uv: vec4<f32>,
    bump_scroll: vec2<f32>,
    bump_flags: u32,
    bump_layers: u32,
    env_flags: u32,
    env_cube: u32,
    env_pad: vec2<u32>,
    env_tint: vec4<f32>,
    env_sat: vec4<f32>,
    env_mask_uv: vec4<f32>,
    env_fresnel: vec4<f32>,
    env_ranges: vec4<f32>,
    self_illum: vec4<f32>,
    fw_lum_start: vec4<f32>,
    fw_lum_end: vec4<f32>,
    fw_blend_start: vec4<f32>,
    fw_blend_end: vec4<f32>,
    fw_lum_factor: vec4<f32>,
    fw_mod2x: vec4<f32>,
    fw_uv12: vec4<f32>,
    fw_uv34: vec4<f32>,
    base_uv: vec4<f32>,
};

fn base_uv_of(uv: vec2<f32>) -> vec2<f32> {
    return uv * material.base_uv.xy + material.base_uv.zw;
}

@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var tex2: texture_2d<f32>;
@group(1) @binding(2) var<uniform> material: Material;
@group(1) @binding(3) var tex3: texture_2d<f32>;
@group(1) @binding(4) var tex4: texture_2d<f32>;
@group(1) @binding(5) var tex_detail: texture_2d<f32>;
@group(1) @binding(6) var tex_bump: texture_2d<f32>;
@group(1) @binding(7) var tex_bump2: texture_2d<f32>;
@group(1) @binding(8) var tex_env_mask: texture_2d<f32>;
@group(1) @binding(9) var tex_self_illum: texture_2d<f32>;

const M_ALPHA_TEST: u32 = 2u;
const M_UNLIT: u32 = 16u;
const M_OPAQUE_TEXTURE: u32 = 256u;
const M_MODULATE: u32 = 512u;
const M_MODULATE_2X: u32 = 1024u;
const M_DECAL_MODULATE: u32 = 2048u;
const M_SELF_ILLUM: u32 = 4096u;
const SI_BASE_ALPHA: f32 = 0.0;
const SI_MASK_COLOR: f32 = 1.0;
const SI_MASK_ALPHA: f32 = 2.0;
const DETAIL_NONE: u32 = 0xffffffffu;

fn self_illum_mask(alpha: f32, uv: vec2<f32>) -> vec3<f32> {
    let m = textureSample(tex_self_illum, samp, uv);
    if (material.flags & M_SELF_ILLUM) == 0u {
        return vec3<f32>(0.0);
    }
    let mode = material.self_illum.w;
    if mode > SI_MASK_ALPHA - 0.5 {
        return vec3<f32>(clamp(m.a, 0.0, 1.0));
    }
    if mode > SI_MASK_COLOR - 0.5 {
        return clamp(m.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return vec3<f32>(clamp(alpha, 0.0, 1.0));
}
const B_ON: u32 = 1u;
const B_SS: u32 = 2u;
const B_TWO: u32 = 4u;
const B_FOUR: u32 = 8u;
const B_NO_DIFFUSE: u32 = 16u;

const BUMP_BASIS_0: vec3<f32> = vec3<f32>(0.81649661, 0.0, 0.57735026);
const BUMP_BASIS_1: vec3<f32> = vec3<f32>(-0.40824834, 0.70710677, 0.57735026);
const BUMP_BASIS_2: vec3<f32> = vec3<f32>(-0.40824822, -0.70710683, 0.57735026);

fn material_tint() -> vec4<f32> {
    if (view.opts.x & OPT_HDR_TINT) != 0u {
        return vec4<f32>(linear_to_srgb(material.tint.rgb), material.tint.a);
    }
    return material.tint;
}

fn bump_on() -> bool {
    return (material.bump_flags & B_ON) != 0u && (view.opts.x & OPT_NORMAL_MAPS) != 0u
        && (material.flags & M_UNLIT) == 0u;
}

fn bump_diffuse() -> bool {
    return (material.bump_flags & B_NO_DIFFUSE) == 0u;
}

const BLEND_PAINTED: u32 = 0x40000000u;

fn unpack_blend_layers(p: u32) -> vec4<f32> {
    let painted = select(0.0, 1.0, (p & BLEND_PAINTED) != 0u);
    let l = vec3<f32>(f32(p & 1023u), f32((p >> 10u) & 1023u), f32((p >> 20u) & 1023u)) / 1023.0;
    return vec4<f32>(l, painted);
}

fn blend_layers(packed: vec4<f32>, alpha: f32) -> vec3<f32> {
    if packed.w > 0.5 {
        return packed.xyz;
    }
    return vec3<f32>(clamp(alpha, 0.0, 1.0), 0.0, 0.0);
}

struct FourWay {
    color: vec4<f32>,
    cover: vec4<f32>,
};

fn fw_step(lo: f32, hi: f32, x: f32) -> f32 {
    let span = hi - lo;
    var t = select(0.0, 1.0, x >= lo);
    if abs(span) > 1e-6 {
        t = clamp((x - lo) / span, 0.0, 1.0);
    }
    return t * t * (3.0 - 2.0 * t);
}

fn fw_lum(c: vec3<f32>, k: i32) -> f32 {
    let l = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    return fw_step(material.fw_lum_start[k], material.fw_lum_end[k], l);
}

fn fw_layer(color: vec4<f32>, layer: vec4<f32>, b: f32, k: i32) -> vec4<f32> {
    if material.fw_mod2x[k] > 0.5 {
        return vec4<f32>(mix(color.rgb, color.rgb * layer.rgb * 2.0, b), color.a);
    }
    return mix(color, layer, b);
}

fn four_way_blend(uv: vec2<f32>, v: vec3<f32>) -> FourWay {
    let c1 = textureSample(tex, samp, uv * material.fw_uv12.xy);
    let c2 = textureSample(tex2, samp, uv * material.fw_uv12.zw);
    let c3 = textureSample(tex3, samp, uv * material.fw_uv34.xy);
    let c4 = textureSample(tex4, samp, uv * material.fw_uv34.zw);
    let l1 = fw_lum(c1.rgb, 0);
    let l2 = fw_lum(c2.rgb, 1);
    let l3 = fw_lum(c3.rgb, 2);
    let l4 = fw_lum(c4.rgb, 3);
    let f2 = mix(1.0 - l1, l2, material.fw_lum_factor.y);
    let b2 = fw_step(material.fw_blend_start.y, material.fw_blend_end.y, v.x * f2 + v.x);
    var color = fw_layer(c1, c2, b2, 1);
    var lum = mix(l1, l2, b2);
    let f3 = mix(1.0 - lum, l3, material.fw_lum_factor.z);
    let b3 = fw_step(material.fw_blend_start.z, material.fw_blend_end.z, v.y * f3 + v.y);
    color = fw_layer(color, c3, b3, 2);
    lum = mix(lum, l3, b3);
    let f4 = mix(1.0 - lum, l4, material.fw_lum_factor.w);
    let b4 = fw_step(material.fw_blend_start.w, material.fw_blend_end.w, v.z * f4 + v.z);
    color = fw_layer(color, c4, b4, 3);
    var out: FourWay;
    out.color = color;
    let k4 = 1.0 - b4;
    let k3 = 1.0 - b3;
    out.cover = vec4<f32>((1.0 - b2) * k3 * k4, b2 * k3 * k4, b3 * k4, b4);
    return out;
}

fn bump_mix(alpha: f32, weights: vec4<f32>) -> f32 {
    if (material.bump_flags & B_FOUR) != 0u {
        return dot(weights, unpack4x8unorm(material.bump_layers));
    }
    return alpha;
}

fn bump_texel(base_uv: vec2<f32>, alpha: f32, weights: vec4<f32>) -> vec3<f32> {
    let uv = base_uv * material.bump_uv.xy + material.bump_uv.zw + material.bump_scroll * view.sky.z;
    var t = textureSample(tex_bump, samp, uv).rgb;
    if (material.bump_flags & B_TWO) != 0u {
        t = mix(t, textureSample(tex_bump2, samp, uv).rgb, clamp(bump_mix(alpha, weights), 0.0, 1.0));
    }
    if (material.bump_flags & B_SS) != 0u {
        return t;
    }
    return t * 2.0 - 1.0;
}

fn bump_vector(texel: vec3<f32>) -> vec3<f32> {
    if (material.bump_flags & B_SS) != 0u {
        return BUMP_BASIS_0 * texel.x + BUMP_BASIS_1 * texel.y + BUMP_BASIS_2 * texel.z;
    }
    let l = length(texel);
    return select(vec3<f32>(0.0, 0.0, 1.0), texel / l, l > 1e-5);
}

fn bump_lightmap_weights(texel: vec3<f32>) -> vec3<f32> {
    if (material.bump_flags & B_SS) != 0u {
        return texel;
    }
    var dp = clamp(vec3<f32>(dot(texel, BUMP_BASIS_0), dot(texel, BUMP_BASIS_1), dot(texel, BUMP_BASIS_2)),
        vec3<f32>(0.0), vec3<f32>(1.0));
    dp *= dp;
    let sum = dp.x + dp.y + dp.z;
    return select(vec3<f32>(1.0 / 3.0), dp / sum, sum > 1e-6);
}

const E_ON: u32 = 1u;
const E_MASK_RGB: u32 = 2u;
const E_MASK_ALPHA: u32 = 4u;
const E_BASE_INV: u32 = 8u;
const E_BASE: u32 = 16u;
const E_PHONG: u32 = 32u;
const E_NO_TEX1: u32 = 64u;
const E_NO_TEX2: u32 = 128u;
const E_FRESNEL: u32 = 256u;
const E_PARALLAX: u32 = 512u;
const CUBE_SURFACE: u32 = 0xffffffffu;
const CUBE_NONE: u32 = 0xffffu;
const CUBE_INFO_STRIDE: u32 = 5u;

fn env_on() -> bool {
    return (material.env_flags & E_ON) != 0u && (view.opts.x & OPT_CUBEMAPS) != 0u;
}

fn env_slot(surface: u32) -> u32 {
    let s = select(material.env_cube, surface & 0xffffu, material.env_cube == CUBE_SURFACE);
    let n = arrayLength(&cube_info) / CUBE_INFO_STRIDE;
    let ok = s != CUBE_NONE && s < n && cube_info[min(s, n - 1u) * CUBE_INFO_STRIDE].x > 0.5;
    return select(0u, s, ok);
}

fn env_fresnel_ranges(n: vec3<f32>, v: vec3<f32>) -> f32 {
    var f = clamp(1.0 - dot(n, v), 0.0, 1.0);
    f = f * f;
    let r = material.env_ranges.xyz;
    return select(mix(r.x, r.y, 2.0 * f), mix(r.y, r.z, 2.0 * f - 1.0), f > 0.5);
}

fn env_reflection(
    world: vec3<f32>,
    n: vec3<f32>,
    surface: u32,
    base_uv: vec2<f32>,
    base_alpha: f32,
    blend: f32,
    light: vec3<f32>,
    gamma: bool,
) -> vec3<f32> {
    if !env_on() {
        return vec3<f32>(0.0);
    }
    let slot = env_slot(surface);
    let to_eye = view.eye.xyz - world;
    let el = length(to_eye);
    let v = select(vec3<f32>(0.0, 0.0, 1.0), to_eye / el, el > 1e-5);
    let r = 2.0 * dot(n, v) * n - v;
    let mask_texel = textureSample(tex_env_mask, samp, base_uv * material.env_mask_uv.xy + material.env_mask_uv.zw);
    let f = material.env_flags;
    let at = slot * CUBE_INFO_STRIDE;
    var dir = r;
    if (f & E_PARALLAX) != 0u {
        dir = parallax_reflection(world, r, cube_info[at + 1u], cube_info[at + 2u], cube_info[at + 3u], cube_info[at + 4u]);
    }
    var c = textureSample(cube_maps, samp, dir, i32(slot)).rgb * cube_info[at].y;
    var mask = vec3<f32>(1.0);
    if (f & E_MASK_RGB) != 0u {
        mask *= mask_texel.rgb;
    }
    if (f & E_MASK_ALPHA) != 0u {
        mask *= mask_texel.a;
    }
    if (f & E_BASE_INV) != 0u {
        mask *= 1.0 - base_alpha;
    }
    if (f & E_BASE) != 0u {
        mask *= base_alpha;
    }
    if (f & E_NO_TEX1) != 0u {
        mask *= blend;
    }
    if (f & E_NO_TEX2) != 0u {
        mask *= 1.0 - blend;
    }
    var tint = material.env_tint.rgb;
    if gamma {
        c = linear_to_srgb(min(c, vec3<f32>(1.0)));
    } else {
        tint = srgb_to_linear_ext(tint);
    }
    var spec = c * mask * tint;
    spec = mix(spec, spec * spec, material.env_tint.w);
    let grey = dot(spec, vec3<f32>(0.299, 0.587, 0.114));
    spec = mix(vec3<f32>(grey), spec, material.env_sat.rgb);
    if (f & E_FRESNEL) != 0u {
        let fr = pow(clamp(1.0 - dot(n, v), 0.0, 1.0), 5.0);
        let base = material.env_fresnel.x;
        spec *= fr * (1.0 - base) + base;
    }
    if (f & E_PHONG) != 0u {
        spec = mix(spec, spec * env_fresnel_ranges(n, v), material.env_fresnel.y);
    }
    spec *= mix(vec3<f32>(1.0), clamp(light, vec3<f32>(0.0), vec3<f32>(1.0)), material.env_sat.w);
    return max(spec, vec3<f32>(0.0));
}

fn srgb_to_linear_ext(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    let lo = x / 12.92;
    let hi = pow((x + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, x <= vec3<f32>(0.04045));
}

struct UvFrame {
    t: vec3<f32>,
    b: vec3<f32>,
};

fn uv_frame(n: vec3<f32>, world: vec3<f32>, uv: vec2<f32>) -> UvFrame {
    let dp1 = dpdx(world);
    let dp2 = dpdy(world);
    let duv1 = dpdx(uv);
    let duv2 = dpdy(uv);
    let dp2perp = cross(dp2, n);
    let dp1perp = cross(n, dp1);
    let side = select(1.0, -1.0, dot(dp1, dp2perp) < 0.0);
    let t = (dp2perp * duv1.x + dp1perp * duv2.x) * side;
    let b = (dp2perp * duv1.y + dp1perp * duv2.y) * side;
    let lt = length(t);
    let lb = length(b);
    var out: UvFrame;
    out.t = select(vec3<f32>(0.0), t / lt, lt > 1e-12);
    out.b = select(vec3<f32>(0.0), b / lb, lb > 1e-12);
    return out;
}

fn bump_to_world(v: vec3<f32>, f: UvFrame, n: vec3<f32>) -> vec3<f32> {
    return f.t * v.x + f.b * v.y + n * v.z;
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    let hi = 1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, x * 12.92, x <= vec3<f32>(0.0031308));
}

fn detail_sample(uv: vec2<f32>) -> vec4<f32> {
    var d = textureSample(tex_detail, samp, uv * material.detail_uv.xy + material.detail_uv.zw);
    if (material.detail_flags & 1u) != 0u {
        d = vec4<f32>(linear_to_srgb(d.rgb), d.a);
    }
    return vec4<f32>(d.rgb * material.detail_tint.rgb, d.a);
}

fn detail_combine(base: vec4<f32>, d: vec4<f32>) -> vec4<f32> {
    let f = material.detail_tint.w;
    var c = base;
    switch material.detail_mode {
        case 0u: {
            c = vec4<f32>(c.rgb * mix(vec3<f32>(1.0), 2.0 * d.rgb, f), c.a);
        }
        case 1u: {
            c = vec4<f32>(c.rgb + f * d.rgb, c.a);
        }
        case 2u: {
            c = vec4<f32>(mix(c.rgb, d.rgb, f * d.a), c.a);
        }
        case 3u: {
            c = mix(c, d, f);
        }
        case 4u: {
            c = vec4<f32>(mix(c.rgb, d.rgb, f * (1.0 - c.a)), d.a);
        }
        case 7u: {
            let dc = mix(d.r, d.a, c.a);
            c = vec4<f32>(c.rgb * mix(vec3<f32>(1.0), vec3<f32>(2.0 * dc), f), c.a);
        }
        case 8u: {
            c = mix(c, c * d, f);
        }
        case 9u: {
            c = vec4<f32>(c.rgb, mix(c.a, c.a * d.a, f));
        }
        case 11u: {
            c = vec4<f32>(c.rgb * dot(d.rgb, vec3<f32>(2.0 / 3.0)), c.a);
        }
        case 12u: {
            c = vec4<f32>(mix(c.rgb, d.rgb, d.a), c.a);
        }
        case 13u: {
            let masked = clamp(clamp(c.rgb * 2.0, vec3<f32>(0.0), vec3<f32>(1.0)) * d.a + (1.0 - d.a), vec3<f32>(0.0), vec3<f32>(1.0));
            c = vec4<f32>(masked * d.rgb, c.a);
        }
        default: {}
    }
    if (material.detail_flags & 2u) == 0u {
        c.a = base.a;
    }
    return c;
}

fn detail_post_lighting(lit: vec3<f32>, d: vec4<f32>) -> vec3<f32> {
    let f = material.detail_tint.w;
    if material.detail_mode == 5u {
        return lit + f * d.rgb;
    }
    if material.detail_mode == 6u {
        let upper = f >= 0.5;
        let mult = select(4.0 * f, 1.0 / max(f, 1e-6), upper);
        let add = select(-0.5 * mult, 1.0 - mult, upper);
        return lit + clamp(mult * d.rgb + add, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return lit;
}

fn modulate_output(texel: vec4<f32>, tint: vec4<f32>) -> vec4<f32> {
    let raw = linear_to_srgb(texel.rgb);
    var g: vec3<f32>;
    if (material.flags & M_DECAL_MODULATE) != 0u {
        g = raw * 2.0;
    } else {
        let c = clamp(vec4<f32>(raw * linear_to_srgb(tint.rgb), texel.a * tint.a), vec4<f32>(0.0), vec4<f32>(1.0));
        let rgb = mix(vec3<f32>(0.5), c.rgb, c.a);
        g = select(rgb, rgb * 2.0, (material.flags & M_MODULATE_2X) != 0u);
    }
    let f = pow(max(g, vec3<f32>(0.0)), vec3<f32>(2.2));
    return vec4<f32>(min(f * 0.5, vec3<f32>(1.0)), 1.0);
}

fn material_texture_alpha(a: f32) -> f32 {
    return select(a, 1.0, (material.flags & M_OPAQUE_TEXTURE) != 0u);
}

fn alpha_tested_out(forced: bool, a: f32) -> bool {
    return (forced || (material.flags & M_ALPHA_TEST) != 0u) && a < material.alpha_ref;
}
