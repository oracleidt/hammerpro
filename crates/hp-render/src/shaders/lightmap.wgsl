override ALPHA_TEST: bool = false;

struct LmFace {
    origin: vec2<f32>,
    density: f32,
    layer: u32,
    slot: u32,
    flags: u32,
    scale: f32,
    dims: u32,
    cube: u32,
    pad: u32,
};

struct LmParams {
    inv_size: vec2<f32>,
    exposure: f32,
    mode: u32,
};

@group(2) @binding(0) var atlas: texture_2d_array<f32>;
@group(2) @binding(1) var<storage, read> lm_faces: array<LmFace>;
@group(2) @binding(2) var<uniform> lm: LmParams;

const LM_VALID: u32 = 1u;
const LM_BAKED: u32 = 2u;
const LM_DISP: u32 = 4u;
const LM_WIDE: u32 = 8u;
const LM_BUMPED: u32 = 16u;
const LM_LDR: u32 = 0x100u;

struct LmOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) lm: vec2<f32>,
    @location(3) alpha: f32,
    @location(4) @interpolate(flat) face: u32,
    @location(5) @interpolate(flat) state: vec2<u32>,
    @location(6) blend: vec4<f32>,
};

fn lm_face(i: u32) -> LmFace {
    if i >= arrayLength(&lm_faces) {
        return LmFace(vec2<f32>(0.0), 1.0, 0u, 0u, 0u, 16.0, 0x00010001u, 0xffffu, 0u);
    }
    return lm_faces[i];
}

@vertex
fn vs_lm(
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) coords: vec2<f32>,
    @location(3) alpha: f32,
    @location(4) face: u32,
    @location(5) blend: u32,
) -> LmOut {
    let f = lm_face(face);
    let st = object_state(f.slot);
    let tint = unpack_srgba(st.tint);
    var out: LmOut;
    out.world = pos;
    out.uv = uv;
    out.lm = coords;
    out.alpha = alpha;
    out.face = face;
    out.state = vec2<u32>(st.flags, st.tint);
    out.blend = unpack_blend_layers(blend);
    let needs_light = (lm.mode & 0xffu) == 1u && (f.flags & LM_BAKED) == 0u;
    if (f.flags & LM_VALID) == 0u || needs_light || hidden_in_view(st.flags) || tint.a < 0.999 {
        out.clip = COLLAPSED;
    } else {
        out.clip = view.view_proj * vec4<f32>(pos, 1.0);
    }
    return out;
}

fn baked_image(f: LmFace, coords: vec2<f32>, k: u32) -> vec3<f32> {
    let w = f32(f.dims & 0xffffu);
    let h = f32(f.dims >> 16u);
    let t = clamp(coords * f.density, vec2<f32>(0.0), vec2<f32>(max(w - 1.0, 0.0), max(h - 1.0, 0.0)));
    let uv = (f.origin + t + vec2<f32>(f32(k) * w + 0.5, 0.5)) * lm.inv_size;
    return textureSampleLevel(atlas, samp_clamp, uv, i32(f.layer), 0.0).rgb;
}

fn baked_light(f: LmFace, coords: vec2<f32>) -> vec3<f32> {
    return baked_image(f, coords, 0u);
}

fn baked_light_bumped(f: LmFace, coords: vec2<f32>, texel: vec3<f32>, detail: vec3<f32>) -> vec3<f32> {
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
    return w.x * baked_image(f, coords, 1u) + w.y * baked_image(f, coords, 2u) + w.z * baked_image(f, coords, 3u);
}

fn finish(rgb_in: vec3<f32>, in: LmOut) -> vec4<f32> {
    var rgb = rgb_in;
    let flags = in.state.x;
    if (flags & F_INSTANCE) != 0u {
        rgb = mix(rgb, INSTANCE_RGB * (0.4 + dot(rgb, vec3<f32>(0.3, 0.59, 0.11))), 0.45);
    }
    if (flags & F_INVALID) != 0u {
        rgb = mix(rgb, INVALID_RGB, 0.5);
    }
    if (flags & F_DIMMED) != 0u {
        let grey = dot(rgb, vec3<f32>(0.3, 0.59, 0.11));
        rgb = mix(vec3<f32>(grey), rgb, 0.4) * 0.3;
    }
    if (flags & F_SELECTED) != 0u {
        rgb = mix(rgb, view.selection.rgb, view.selection.a);
    }
    if (flags & F_HOVERED) != 0u {
        rgb = mix(rgb, vec3<f32>(1.0), 0.18);
    }
    rgb = apply_fog(rgb, in.world);
    return vec4<f32>(rgb, 1.0);
}

@fragment
fn fs_baked(in: LmOut) -> @location(0) vec4<f32> {
    let tuv = in.uv * material.inv_tex_size;
    let uv = tuv + material.scroll * view.sky.z;
    var albedo = textureSample(tex, samp, base_uv_of(uv));
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
        bump = bump_texel(tuv, in.alpha, cover);
    }
    var env_raw = vec3<f32>(0.0);
    if env_on() {
        var n = normalize(cross(dpdy(in.world), dpdx(in.world)) + vec3<f32>(0.0, 0.0, 1e-9));
        n = select(n, -n, dot(n, view.eye.xyz - in.world) < 0.0);
        var en = n;
        if bumped {
            let frame = uv_frame(n, in.world, tuv);
            let bv = bump_to_world(bump_vector(bump), frame, n);
            let bl = length(bv);
            en = select(n, bv / bl, bl > 1e-5);
        }
        env_raw = env_reflection(in.world, en, lm_face(in.face).cube, tuv, albedo.a, in.alpha, vec3<f32>(1.0),
            (lm.mode & LM_LDR) != 0u);
    }
    let glow = self_illum_mask(albedo.a, tuv);
    albedo.a = material_texture_alpha(albedo.a);
    albedo = albedo * material_tint() * unpack_srgba(in.state.y);
    if alpha_tested_out(ALPHA_TEST, albedo.a) {
        discard;
    }
    let f = lm_face(in.face);
    var light: vec3<f32>;
    if bumped && (f.flags & LM_BUMPED) != 0u {
        light = baked_light_bumped(f, in.lm, bump, detail.rgb);
    } else {
        light = baked_light(f, in.lm);
    }
    let ldr = (lm.mode & LM_LDR) != 0u;
    let scale = select(lm.exposure * bitcast<f32>(view.misc.w), lm.exposure, ldr);
    var pn = normalize(cross(dpdy(in.world), dpdx(in.world)) + vec3<f32>(0.0, 0.0, 1e-9));
    pn = select(pn, -pn, dot(pn, view.eye.xyz - in.world) < 0.0);
    let projected = projector_light(in.world, pn);
    if ldr {
        light += pow(projected, vec3<f32>(1.0 / 2.2));
    } else {
        light += projected;
    }
    let unlit = (material.flags & M_UNLIT) != 0u;
    let env = env_raw * mix(vec3<f32>(1.0), clamp(light, vec3<f32>(0.0), vec3<f32>(1.0)), material.env_sat.w);
    let lit = select(light, vec3<f32>(1.0), unlit);
    var rgb: vec3<f32>;
    if ldr {
        rgb = srgb_to_linear(min(linear_to_srgb(albedo.rgb) * lit + env, vec3<f32>(1.0))) * scale;
    } else {
        rgb = (albedo.rgb * lit + env) * scale;
    }
    rgb = mix(rgb, albedo.rgb * material.self_illum.rgb * scale, glow);
    if material.detail_mode != DETAIL_NONE {
        rgb = detail_post_lighting(rgb, detail);
    }
    return finish(rgb, in);
}

fn luxel_checker(p: vec2<f32>, fw: vec2<f32>) -> f32 {
    let w = min(fw, vec2<f32>(4.0));
    let i = 2.0 * (abs(fract((p - 0.5 * w) * 0.5) - vec2<f32>(0.5)) - abs(fract((p + 0.5 * w) * 0.5) - vec2<f32>(0.5))) / w;
    return 0.5 - 0.5 * i.x * i.y;
}

@fragment
fn fs_grid(in: LmOut) -> @location(0) vec4<f32> {
    let fw = max(fwidth(in.lm), vec2<f32>(1e-5));
    let n = normalize(cross(dpdx(in.world), dpdy(in.world)) + vec3<f32>(0.0, 0.0, 1e-6));
    let f = lm_face(in.face);
    let t = clamp(log2(max(f.scale, 0.01) / 16.0) * 0.5, -1.0, 1.0);
    let neutral = vec3<f32>(0.42, 0.42, 0.45);
    let red = vec3<f32>(0.8, 0.08, 0.06);
    let green = vec3<f32>(0.1, 0.6, 0.15);
    var base = select(mix(neutral, green, t), mix(neutral, red, -t), t < 0.0);
    if (f.flags & LM_DISP) != 0u {
        let power = (f.flags >> 8u) & 15u;
        var tint = vec3<f32>(0.3, 0.55, 0.95);
        if power >= 4u {
            tint = vec3<f32>(0.6, 0.3, 0.95);
        } else if power == 3u {
            tint = vec3<f32>(0.2, 0.35, 0.95);
        }
        base = mix(base, tint, 0.35);
    }
    if (f.flags & LM_BAKED) != 0u {
        let l = baked_light(f, in.lm) * lm.exposure;
        let lum = clamp(dot(l, vec3<f32>(0.3, 0.59, 0.11)), 0.0, 1.5);
        base = base * (0.3 + 0.7 * lum);
    }
    let shade = 0.78 + abs(n.x) * 0.06 + abs(n.y) * 0.12 + abs(n.z) * 0.2;
    base = base * (1.0 - 0.14 * luxel_checker(in.lm, fw));
    let d = abs(fract(in.lm - vec2<f32>(0.5)) - vec2<f32>(0.5)) / fw;
    let line = 1.0 - clamp(min(d.x, d.y) - 0.25, 0.0, 1.0);
    let dense = clamp((max(fw.x, fw.y) - 0.15) * 4.0, 0.0, 1.0);
    var rgb = mix(base * shade, base * shade * 0.35, line * 0.8 * (1.0 - dense));
    if (f.flags & LM_WIDE) != 0u {
        let q = (in.lm - vec2<f32>(0.5)) / 32.0;
        let k = round(q);
        var d32 = abs(q - k) * 32.0 / fw;
        d32 = select(vec2<f32>(1e6), d32, k >= vec2<f32>(1.0));
        let l32 = 1.0 - clamp(min(d32.x, d32.y) - 0.5, 0.0, 1.0);
        rgb = mix(rgb, vec3<f32>(1.0, 0.8, 0.1), l32 * 0.5);
    }
    return finish(rgb, in);
}
