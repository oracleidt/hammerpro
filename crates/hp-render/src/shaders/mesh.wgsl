override ALPHA_TEST: bool = false;
override BLEND_MODE: u32 = 0u;
override TINT_FILTER: u32 = 0u;
override TINT_OPACITY: bool = false;
override SELECTED_ONLY: bool = false;
override WIRE_RGB: u32 = 0u;
override WIRE_SET: u32 = 0u;
override ENTITY_ONLY: bool = false;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) state: vec3<u32>,
    @location(4) tint: vec4<f32>,
    @location(5) alpha: f32,
    @location(6) fade: f32,
    @location(7) blend: vec4<f32>,
    @location(9) baked: vec4<f32>,
    @location(10) @interpolate(flat) light_rec: u32,
    @location(11) static_light: vec4<f32>,
    @location(12) tangent: vec4<f32>,
    @location(13) @interpolate(flat) cube: u32,
};

fn unpack_normal(p: u32) -> vec3<f32> {
    return unpack4x8snorm(p).xyz;
}

fn unpack_alpha(p: u32) -> f32 {
    return f32(p >> 24u) / 255.0;
}

fn tint_kept(alpha: f32) -> bool {
    if TINT_FILTER == 1u {
        return alpha >= 0.999;
    }
    if TINT_FILTER == 2u {
        return alpha < 0.999;
    }
    return true;
}

@vertex
fn vs_chunk(
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) nrm_alpha: u32,
    @location(3) slot: u32,
    @location(13) blend: u32,
    @location(14) cube: u32,
) -> VsOut {
    let st = object_state(slot);
    let tint = unpack_srgba(st.tint);
    var out: VsOut;
    out.blend = unpack_blend_layers(blend);
    out.world = pos;
    out.normal = unpack_normal(nrm_alpha);
    out.uv = uv;
    out.state = vec3<u32>(st.flags, st.color, 0u);
    out.tint = tint;
    out.alpha = unpack_alpha(nrm_alpha);
    out.fade = 1.0;
    out.baked = vec4<f32>(0.0);
    out.light_rec = NO_MODEL_LIGHT;
    out.static_light = vec4<f32>(0.0);
    out.tangent = vec4<f32>(0.0);
    out.cube = cube & 0xffffu;
    let unselected = SELECTED_ONLY && (st.flags & F_SELECTED) == 0u;
    let not_entity = ENTITY_ONLY && (st.flags & F_BRUSH_ENTITY) == 0u;
    let compiled = (view.opts.x & OPT_COMPILED_LIGHT) != 0u && (st.flags & F_COMPILED_LIGHT) != 0u
        && (BLEND_MODE == 1u || BLEND_MODE == 2u);
    if hidden_in_view(st.flags) || !tint_kept(tint.a) || unselected || not_entity || compiled {
        out.clip = COLLAPSED;
    } else {
        out.clip = view.view_proj * vec4<f32>(pos, 1.0);
    }
    return out;
}

@vertex
fn vs_model(
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) nrm_alpha: u32,
    @location(3) cube: u32,
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    @location(8) tint: vec4<f32>,
    @location(10) slot: u32,
    @location(12) fade: vec2<f32>,
    @location(14) light: u32,
    @location(15) tangent: vec4<f32>,
    @builtin(vertex_index) vertex: u32,
) -> VsOut {
    let m = mat4x4<f32>(m0, m1, m2, m3);
    let st = object_state(slot);
    let world = m * vec4<f32>(pos, 1.0);
    var out: VsOut;
    out.world = world.xyz;
    out.normal = (m * vec4<f32>(unpack_normal(nrm_alpha), 0.0)).xyz;
    out.baked = model_light(light, vertex, world.xyz, out.normal);
    out.light_rec = light;
    out.static_light = model_static_light(light, vertex);
    out.tangent = vec4<f32>((m * vec4<f32>(tangent.xyz, 0.0)).xyz, select(1.0, -1.0, tangent.w < 0.0));
    out.uv = uv;
    out.cube = cube & 0xffffu;
    out.state = vec3<u32>(st.flags, st.color, 1u);
    let origin = m3.xyz;
    let detail = detail_fade(origin, fade);
    out.tint = vec4<f32>(srgb_to_linear(tint.rgb), tint.a * detail);
    out.alpha = unpack_alpha(nrm_alpha);
    let far = view.sky.w > 0.0 && !is_ortho() && distance(origin, view.eye.xyz) > view.sky.w;
    out.fade = model_fade(origin, fade);
    let unselected = SELECTED_ONLY && (st.flags & F_SELECTED) == 0u;
    if hidden_in_view(st.flags) || !tint_kept(out.tint.a) || far || out.fade <= 0.0 || detail <= 0.0 || unselected {
        out.clip = COLLAPSED;
    } else {
        out.clip = view.view_proj * world;
    }
    return out;
}

const MODEL_LIGHT_VEC4: u32 = 28u;
const NO_MODEL_LIGHT: u32 = 0xffffffffu;

fn model_record(light: u32) -> u32 {
    let base = light * MODEL_LIGHT_VEC4;
    if light == NO_MODEL_LIGHT || base + MODEL_LIGHT_VEC4 > arrayLength(&model_lights) {
        return NO_MODEL_LIGHT;
    }
    return base;
}

fn model_vertex_color(base: u32, vertex: u32) -> vec4<f32> {
    let head = model_lights[base + 6u];
    let offset = u32(head.x);
    let count = u32(head.y);
    if vertex < count && offset + vertex < arrayLength(&model_colors) {
        let c = model_colors[offset + vertex];
        let bgr = vec3<f32>(f32(c & 255u), f32((c >> 8u) & 255u), f32((c >> 16u) & 255u)) / 255.0;
        return vec4<f32>(pow(bgr.zyx * 2.0, vec3<f32>(2.2)), 1.0);
    }
    return vec4<f32>(0.0);
}

fn model_ambient(base: u32, n: vec3<f32>) -> vec3<f32> {
    let sq = n * n;
    return sq.x * model_lights[base + select(0u, 1u, n.x < 0.0)].rgb
        + sq.y * model_lights[base + select(2u, 3u, n.y < 0.0)].rgb
        + sq.z * model_lights[base + select(4u, 5u, n.z < 0.0)].rgb;
}

fn model_locals(base: u32, world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let lights = min(u32(model_lights[base + 6u].z), 4u);
    for (var i = 0u; i < lights; i++) {
        let at = base + 8u + i * 5u;
        let p = model_lights[at];
        let d = model_lights[at + 1u];
        let color = model_lights[at + 2u];
        let att = model_lights[at + 3u];
        let cone = model_lights[at + 4u];
        let kind = u32(p.w);
        var to_light = -d.xyz;
        var f = 1.0;
        if kind != 2u {
            let delta = p.xyz - world;
            let dist = max(length(delta), 1.0);
            to_light = delta / dist;
            if d.w > 0.0 && dist > d.w {
                f = 0.0;
            }
            f *= 1.0 / max(att.x + att.y * dist + att.z * dist * dist, 1e-6);
            if kind == 1u {
                let s = dot(-to_light, d.xyz);
                if s <= cone.y {
                    f = 0.0;
                } else if s <= cone.x {
                    var mult = clamp((s - cone.y) / max(cone.x - cone.y, 1e-6), 0.0, 1.0);
                    if cone.z != 0.0 && cone.z != 1.0 {
                        mult = pow(mult, cone.z);
                    }
                    f *= mult;
                }
            }
        }
        sum += color.rgb * (light_style(u32(color.w)) * f * max(dot(n, to_light), 0.0));
    }
    return sum;
}

fn model_light(light: u32, vertex: u32, world: vec3<f32>, normal: vec3<f32>) -> vec4<f32> {
    if (view.opts.x & (OPT_COMPILED_LIGHT | OPT_MODEL_LIGHT)) == 0u {
        return vec4<f32>(0.0);
    }
    if (view.opts.x & OPT_COMPILED_FULLBRIGHT) != 0u {
        return vec4<f32>(1.0);
    }
    let base = model_record(light);
    if base == NO_MODEL_LIGHT {
        return vec4<f32>(0.0);
    }
    let exposure = bitcast<f32>(view.misc.z);
    let len = length(normal);
    var n = vec3<f32>(0.0, 0.0, 1.0);
    if len > 1e-5 {
        n = normal / len;
    }
    let vc = model_vertex_color(base, vertex);
    var sum = select(model_ambient(base, n), vc.rgb, vc.w > 0.5);
    sum += model_locals(base, world, n);
    return vec4<f32>(sum * exposure, 1.0);
}

fn model_static_light(light: u32, vertex: u32) -> vec4<f32> {
    if (view.opts.x & (OPT_COMPILED_LIGHT | OPT_MODEL_LIGHT)) == 0u || (view.opts.x & OPT_COMPILED_FULLBRIGHT) != 0u {
        return vec4<f32>(0.0);
    }
    let base = model_record(light);
    if base == NO_MODEL_LIGHT {
        return vec4<f32>(0.0);
    }
    return model_vertex_color(base, vertex);
}

fn model_light_px(light: u32, static_light: vec4<f32>, world: vec3<f32>, bn: vec3<f32>) -> vec3<f32> {
    if (view.opts.x & OPT_COMPILED_FULLBRIGHT) != 0u {
        return vec3<f32>(1.0);
    }
    let base = model_record(light);
    if base == NO_MODEL_LIGHT {
        return vec3<f32>(1.0);
    }
    let len = length(bn);
    let n = select(vec3<f32>(0.0, 0.0, 1.0), bn / len, len > 1e-5);
    var sum = select(model_ambient(base, n) * min(len, 1.0), static_light.rgb, static_light.w > 0.5);
    sum += model_locals(base, world, bn);
    return sum * bitcast<f32>(view.misc.z);
}

fn model_fade(origin: vec3<f32>, fade: vec2<f32>) -> f32 {
    if (view.opts.x & OPT_MODEL_FADE) == 0u || is_ortho() || fade.y <= 0.0 {
        return 1.0;
    }
    let d = distance(origin, view.eye.xyz);
    let start = min(max(fade.x, 0.0), fade.y);
    if d <= start {
        return 1.0;
    }
    return clamp(1.0 - (d - start) / max(fade.y - start, 1.0), 0.0, 1.0);
}

fn detail_fade(origin: vec3<f32>, fade: vec2<f32>) -> f32 {
    if fade.y >= 0.0 || is_ortho() {
        return 1.0;
    }
    let ofs = origin - view.eye.xyz;
    let d2 = dot(ofs, ofs);
    let max2 = fade.y * fade.y;
    let fade2 = min(fade.x * fade.x, max2 - 1.0);
    if d2 >= max2 {
        return 0.0;
    }
    if d2 > fade2 {
        return clamp((max2 - d2) / max(max2 - fade2, 1e-3), 0.0, 1.0);
    }
    return 1.0;
}

fn bayer4(frag: vec2<f32>) -> f32 {
    let x = u32(frag.x) & 3u;
    let y = u32(frag.y) & 3u;
    let m = array<u32, 16>(0u, 8u, 2u, 10u, 12u, 4u, 14u, 6u, 3u, 11u, 1u, 9u, 15u, 7u, 13u, 5u);
    return (f32(m[y * 4u + x]) + 0.5) / 16.0;
}

fn editor_shade(n: vec3<f32>) -> f32 {
    return 0.75 + dot(n, vec3<f32>(0.12, 0.06, 0.25));
}

const TEXTURED_LIGHT: vec3<f32> = vec3<f32>(0.8305, 0.2491, 0.4983);

fn textured_shade(n: vec3<f32>) -> f32 {
    let h = clamp((dot(n, TEXTURED_LIGHT) + length(n)) * 0.5, 0.0, 1.0);
    return pow(0.5 + 0.65 * pow(h, 1.5), 2.2);
}

const EDITOR_RELIEF: f32 = 2.5;

fn editor_relief(v: vec3<f32>) -> vec3<f32> {
    let l = length(v);
    let s = vec3<f32>(v.xy * EDITOR_RELIEF, v.z);
    let sl = length(s);
    return select(v, s * (l / sl), sl > 1e-6);
}

fn sun_visibility(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if view.shadow.x < 0.5 {
        return 1.0;
    }
    let p = view.shadow_vp * vec4<f32>(world + n * view.shadow.w, 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || p.z > 1.0 || p.z < 0.0 {
        return 1.0;
    }
    let d = p.z - view.shadow.y;
    var sum = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let o = vec2<f32>(f32(x), f32(y)) * view.shadow.z;
            sum += textureSampleCompareLevel(shadow_map, shadow_samp, uv + o, d);
        }
    }
    return sum / 9.0;
}

fn local_visibility(l: Light, world: vec3<f32>, n: vec3<f32>) -> f32 {
    let count = u32(l.extra.w + 0.5);
    let first = u32(l.extra.z + 0.5);
    if first == 0u || (count != 1u && count != 6u) {
        return 1.0;
    }
    let v = world - l.pos_range.xyz;
    var face = 0u;
    if count == 6u {
        let a = abs(v);
        if a.x >= a.y && a.x >= a.z {
            face = select(1u, 0u, v.x > 0.0);
        } else if a.y >= a.z {
            face = select(3u, 2u, v.y > 0.0);
        } else {
            face = select(5u, 4u, v.z > 0.0);
        }
    }
    let ri = first - 1u + face;
    if ri >= arrayLength(&lights) {
        return 1.0;
    }
    let r = lights[ri];
    let m = mat4x4<f32>(r.pos_range, r.color, r.dir, r.cone);
    let texel = r.atten.y * max(length(v), 1.0);
    let p = m * vec4<f32>(world + n * (texel * 2.0), 1.0);
    if p.w <= 1e-4 {
        return 1.0;
    }
    let c = p.xyz / p.w;
    let uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || c.z > 1.0 || c.z < 0.0 {
        return 1.0;
    }
    let layer = i32(r.atten.x + 0.5);
    let step = 1.0 / f32(textureDimensions(local_shadow_maps).x);
    var sum = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let o = vec2<f32>(f32(x), f32(y)) * step;
            sum += textureSampleCompareLevel(local_shadow_maps, shadow_samp, uv + o, layer, c.z);
        }
    }
    return sum / 9.0;
}

fn lighting(world: vec3<f32>, n: vec3<f32>, ln: vec3<f32>, frag: vec2<f32>) -> vec3<f32> {
    var sum = view.ambient.rgb * min(length(ln), 1.0);
    let n_dir = min(view.misc.y, arrayLength(&lights));
    for (var i = 0u; i < n_dir; i++) {
        let l = lights[i];
        let ndl = max(dot(ln, -l.dir.xyz), 0.0);
        var vis = 1.0;
        if i == 0u && ndl > 0.0 {
            vis = sun_visibility(world, n);
        }
        sum += l.color.rgb * (ndl * vis);
    }
    let dims = view.cluster_dims;
    if dims.x == 0u || dims.y == 0u || dims.z == 0u {
        return sum;
    }
    let tile = vec2<u32>(max(frag, vec2<f32>(0.0)) / f32(max(dims.w, 1u)));
    let tx = min(tile.x, dims.x - 1u);
    let ty = min(tile.y, dims.y - 1u);
    let depth = dot(world - view.eye.xyz, view.forward.xyz);
    let near = view.cluster_z.x;
    var slice = 0u;
    if depth > near {
        slice = min(u32(log(depth / near) * view.cluster_z.z), dims.z - 1u);
    }
    let ci = (slice * dims.y + ty) * dims.x + tx;
    if ci >= arrayLength(&clusters) {
        return sum;
    }
    let range = clusters[ci];
    let n_idx = arrayLength(&light_indices);
    let n_lights = arrayLength(&lights);
    for (var k = 0u; k < range.y; k++) {
        let idx = range.x + k;
        if idx >= n_idx {
            break;
        }
        let li = light_indices[idx];
        if li >= n_lights {
            continue;
        }
        let l = lights[li];
        let d = l.pos_range.xyz - world;
        let dist2 = max(dot(d, d), 1e-4);
        let dist = sqrt(dist2);
        let to_light = d / dist;
        let ndl = max(dot(ln, to_light), 0.0);
        let x = dist / max(l.pos_range.w, 1e-3);
        let x2 = x * x;
        let win = clamp(1.0 - x2 * x2, 0.0, 1.0);
        var att = light_falloff(l, dist) * win * win;
        if l.color.w > 0.5 && l.color.w < 1.5 {
            att *= spot_cone(l, dot(-to_light, l.dir.xyz));
        }
        if att > 0.0 && ndl > 0.0 {
            att *= local_visibility(l, world, n);
        }
        sum += l.color.rgb * (ndl * att);
    }
    return sum;
}

fn grid_coverage(world: vec3<f32>, n: vec3<f32>) -> vec2<f32> {
    let a = abs(n);
    var p = world.xy;
    if a.x > a.z && a.x >= a.y {
        p = world.yz;
    } else if a.y > a.z && a.y > a.x {
        p = world.xz;
    }
    let step = max(view.grid3d.x, 1e-3);
    let major_step = step * max(view.grid3d.y, 1.0);
    let g = p / step;
    let w = max(fwidth(g), vec2<f32>(1e-5));
    let d = abs(fract(g - 0.5) - 0.5) / w;
    let minor = 1.0 - min(min(d.x, d.y), 1.0);
    let gm = p / major_step;
    let wm = max(fwidth(gm), vec2<f32>(1e-5));
    let dm = abs(fract(gm - 0.5) - 0.5) / wm;
    let major = 1.0 - min(min(dm.x, dm.y), 1.0);
    let dense = clamp((1.0 / max(w.x, w.y) - 3.0) / 3.0, 0.0, 1.0);
    return vec2<f32>(minor * dense, major);
}

@fragment
fn fs_mesh(in: VsOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
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
    var grid = vec2<f32>(0.0);
    if (view.opts.x & OPT_GRID_3D) != 0u {
        grid = grid_coverage(in.world, in.normal);
    }
    let len = length(in.normal);
    var n = vec3<f32>(0.0, 0.0, 1.0);
    if len > 1e-5 {
        n = in.normal / len;
    }
    if !front {
        n = -n;
    }
    let mode = view.misc.x;
    let shaded = mode == MODE_TEXTURED || mode == MODE_TEXTURED_WIRE || mode == MODE_LIT
        || (view.opts.x & OPT_COMPILED_LIGHT) != 0u;
    var bn = n;
    let bumped = shaded && bump_on();
    if bumped {
        let base_uv = in.uv * material.inv_tex_size;
        let texel = bump_texel(base_uv, in.alpha, cover);
        var frame = uv_frame(n, in.world, base_uv);
        let tl = length(in.tangent.xyz);
        if abs(in.tangent.w) > 0.5 && tl > 1e-5 {
            let t = in.tangent.xyz / tl;
            let tp = t - n * dot(n, t);
            let tpl = length(tp);
            if tpl > 1e-5 {
                frame.t = tp / tpl;
                frame.b = cross(n, frame.t) * in.tangent.w;
            }
        }
        var v = bump_vector(texel);
        let compiled_lit = (view.opts.x & OPT_COMPILED_LIGHT) != 0u;
        if (mode == MODE_TEXTURED || mode == MODE_TEXTURED_WIRE) && !compiled_lit {
            v = editor_relief(v);
        }
        bn = bump_to_world(v, frame, n);
    }
    var env = vec3<f32>(0.0);
    let reflects = shaded && BLEND_MODE != 3u;
    if reflects && env_on() {
        let bl = length(bn);
        let en = select(n, bn / bl, bl > 1e-5);
        env = env_reflection(in.world, en, in.cube, in.uv * material.inv_tex_size, albedo.a, in.alpha, vec3<f32>(1.0), false);
    }
    let glow = self_illum_mask(albedo.a, in.uv * material.inv_tex_size);
    let glow_tint = material.self_illum.rgb;
    if BLEND_MODE == 3u {
        if (material.flags & M_DECAL_MODULATE) != 0u && albedo.a <= 0.0 {
            discard;
        }
        if view.misc.x == MODE_FLAT {
            return vec4<f32>(0.5, 0.5, 0.5, 1.0);
        }
        return modulate_output(albedo, material.tint * in.tint);
    }
    albedo.a = material_texture_alpha(albedo.a);
    albedo = albedo * material.tint * in.tint;
    if alpha_tested_out(ALPHA_TEST, albedo.a) {
        discard;
    }
    if in.fade < 1.0 && in.fade < bayer4(in.clip.xy) {
        discard;
    }
    let unlit = (material.flags & M_UNLIT) != 0u;
    var rgb: vec3<f32>;
    var lit_by = vec3<f32>(1.0);
    if in.baked.w > 0.5 && mode != MODE_FLAT {
        var baked = in.baked.rgb;
        if bumped {
            baked = model_light_px(in.light_rec, in.static_light, in.world, bn);
        }
        rgb = select(albedo.rgb * baked, albedo.rgb * bitcast<f32>(view.misc.z), unlit);
        rgb = mix(rgb, albedo.rgb * glow_tint * bitcast<f32>(view.misc.z), glow);
    } else if mode == MODE_FLAT && (view.opts.x & OPT_NEUTRAL_FLAT) != 0u {
        rgb = select(albedo.rgb * textured_shade(n), albedo.rgb, unlit);
    } else if mode == MODE_FLAT {
        rgb = unpack_srgba(in.state.y).rgb * editor_shade(n);
    } else if mode == MODE_LIT {
        if !unlit {
            lit_by = lighting(in.world, n, bn, in.clip.xy);
        }
        rgb = select(albedo.rgb * lit_by, albedo.rgb, unlit);
        rgb = mix(rgb, albedo.rgb * glow_tint, glow);
    } else if mode == MODE_UNSHADED {
        rgb = albedo.rgb;
    } else {
        rgb = select(albedo.rgb * textured_shade(bn), albedo.rgb, unlit);
    }
    if reflects {
        let scale = mix(vec3<f32>(1.0), clamp(lit_by, vec3<f32>(0.0), vec3<f32>(1.0)), material.env_sat.w);
        rgb += env * scale * select(1.0, bitcast<f32>(view.misc.z), in.baked.w > 0.5 && !unlit);
    }
    if mode != MODE_FLAT && material.detail_mode != DETAIL_NONE {
        rgb = detail_post_lighting(rgb, detail);
    }
    let flags = in.state.x;
    let is_model = in.state.z != 0u;
    if (view.opts.x & OPT_WALKABLE) != 0u && n.z > 0.0 && n.z < 0.7 {
        rgb = mix(rgb, vec3<f32>(1.0, 0.25, 0.05), 0.45);
    }
    if !is_model && (view.opts.x & OPT_GRID_3D) != 0u {
        let fade = 1.0 - smoothstep(view.grid3d.z * 0.6, view.grid3d.z, distance(in.world, view.eye.xyz));
        let minor = grid.x * view.grid3d.w * fade;
        let major = grid.y * min(view.grid3d.w * 1.6, 1.0) * fade;
        rgb = mix(rgb, vec3<f32>(1.0), max(minor * 0.55, major * 0.8));
    }
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
    let selected = (flags & F_SELECTED) != 0u;
    if selected {
        rgb = mix(rgb, view.selection.rgb, view.selection.a);
    }
    if (flags & F_HOVERED) != 0u {
        rgb = mix(rgb, vec3<f32>(1.0), 0.18);
    }
    rgb = mix(rgb, fog_color(in.world), fog_amount(in.world));
    if BLEND_MODE == 0u {
        return vec4<f32>(rgb, 1.0);
    }
    var a: f32;
    if TINT_OPACITY {
        a = in.tint.a;
    } else {
        a = clamp(albedo.a * material.alpha_scale, 0.0, 1.0);
    }
    if selected {
        a = max(a, 0.6);
    }
    if BLEND_MODE == 2u {
        return vec4<f32>(rgb * a, 0.0);
    }
    return vec4<f32>(rgb, a);
}

@fragment
fn fs_fill(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(view.selection.rgb, view.grid3d.w);
}

fn game_wire_skipped(flags: u32) -> bool {
    let entity = (flags & F_BRUSH_ENTITY) != 0u;
    let other_set = (WIRE_SET == 1u && entity) || (WIRE_SET == 2u && !entity);
    return other_set || hidden_in_view(flags) || (flags & (F_GAME_HIDDEN | F_DISPLACEMENT | F_NO_GAME_WIRE | F_ROPE)) != 0u;
}

struct WireOut {
    @builtin(position) clip: vec4<f32>,
};

@vertex
fn vs_chunk_wire(
    @location(0) pos: vec3<f32>,
    @location(3) slot: u32,
) -> WireOut {
    var out: WireOut;
    if game_wire_skipped(object_state(slot).flags) {
        out.clip = COLLAPSED;
    } else {
        out.clip = view.view_proj * vec4<f32>(pos, 1.0);
    }
    return out;
}

@vertex
fn vs_chunk_outline(
    @location(0) pos: vec3<f32>,
    @location(2) nrm_alpha: u32,
    @location(3) slot: u32,
) -> WireOut {
    var out: WireOut;
    let n = unpack_normal(nrm_alpha);
    let facing = dot(n, view.eye.xyz - pos) >= -0.01 * length(n);
    if game_wire_skipped(object_state(slot).flags) || !facing {
        out.clip = COLLAPSED;
    } else {
        out.clip = view.view_proj * vec4<f32>(pos, 1.0);
    }
    return out;
}

fn lod_metric(origin: vec3<f32>) -> f32 {
    let up = view.up.xyz * 0.5;
    let c1 = view.view_proj * vec4<f32>(origin + up, 1.0);
    let c2 = view.view_proj * vec4<f32>(origin - up, 1.0);
    let y1 = select(c1.y * 1000.0, c1.y / c1.w, c1.w >= 0.001);
    let y2 = select(c2.y * 1000.0, c2.y / c2.w, c2.w >= 0.001);
    let width = 2.0 * view.viewport.y * abs(y2 - y1) * 0.5;
    return select(0.0, 100.0 / width, width != 0.0);
}

@vertex
fn vs_model_lod(
    @location(0) pos: vec3<f32>,
    @location(3) lod_lo: u32,
    @location(13) lod_hi: u32,
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    @location(8) tint: vec4<f32>,
    @location(10) slot: u32,
    @location(12) fade: vec2<f32>,
) -> WireOut {
    var out: WireOut;
    let m = mat4x4<f32>(m0, m1, m2, m3);
    let metric = lod_metric(m3.xyz);
    let picked = metric >= bitcast<f32>(lod_lo) && metric < bitcast<f32>(lod_hi);
    let gone = tint.a <= 0.0 || model_fade(m3.xyz, fade) <= 0.0;
    if game_wire_skipped(object_state(slot).flags) || !picked || gone {
        out.clip = COLLAPSED;
    } else {
        out.clip = view.view_proj * (m * vec4<f32>(pos, 1.0));
    }
    return out;
}

@fragment
fn fs_game_wire() -> @location(0) vec4<f32> {
    let c = vec3<f32>(f32(WIRE_RGB & 255u), f32((WIRE_RGB >> 8u) & 255u), f32((WIRE_RGB >> 16u) & 255u)) / 255.0;
    return vec4<f32>(srgb_to_linear(c), 1.0);
}

@fragment
fn fs_mask(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0);
}

@fragment
fn fs_shadow_alpha(in: VsOut) {
    let t = textureSample(tex, samp, in.uv * material.inv_tex_size).a;
    let a = material_texture_alpha(t) * material.tint.a;
    if alpha_tested_out(true, a) {
        discard;
    }
}
