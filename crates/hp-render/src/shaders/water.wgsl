struct WaterMat {
    refract: vec4<f32>,
    reflect: vec4<f32>,
    fog: vec4<f32>,
    ranges: vec4<f32>,
    bump_uv: vec4<f32>,
    scroll: vec4<f32>,
    scroll12: vec4<f32>,
    frames: vec4<f32>,
    bottom_refract: vec4<f32>,
    bottom_fog: vec4<f32>,
    bottom_ranges: vec4<f32>,
    flags: vec4<u32>,
};

struct WaterView {
    depth: vec4<f32>,
    background: vec4<f32>,
    under_fog: vec4<f32>,
    under_range: vec4<f32>,
    reflect_vp: mat4x4<f32>,
    reflect: vec4<f32>,
};

@group(1) @binding(0) var<uniform> water: WaterMat;
@group(1) @binding(1) var normal_tex: texture_2d<f32>;
@group(1) @binding(2) var base_tex: texture_2d<f32>;

@group(2) @binding(0) var<uniform> wview: WaterView;
@group(2) @binding(1) var scene_color: texture_2d<f32>;
@group(2) @binding(2) var scene_depth: texture_depth_2d;
@group(2) @binding(3) var reflect_tex: texture_2d<f32>;

@group(3) @binding(0) var sky_rt: texture_2d<f32>;
@group(3) @binding(1) var sky_lf: texture_2d<f32>;
@group(3) @binding(2) var sky_bk: texture_2d<f32>;
@group(3) @binding(3) var sky_ft: texture_2d<f32>;
@group(3) @binding(4) var sky_up: texture_2d<f32>;
@group(3) @binding(5) var sky_dn: texture_2d<f32>;

const W_REFRACT: u32 = 1u;
const W_REFLECT: u32 = 2u;
const W_CHEAP: u32 = 4u;
const W_CHEAP_BLEND: u32 = 8u;
const W_MULTITEXTURE: u32 = 16u;
const W_BASE: u32 = 32u;
const W_FRESNEL: u32 = 64u;
const W_NORMAL: u32 = 128u;
const W_BOTTOM: u32 = 256u;
const W_NO_ENVMAP: u32 = 512u;
const W_BELOW: u32 = 1024u;

const SSR_STEPS: u32 = 28u;
const SSR_REFINE: u32 = 5u;
const SSR_LENGTH: f32 = 8192.0;
const FAR_AWAY: f32 = 1.0e9;

struct WaterOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) flags: u32,
    @location(4) tint: vec4<f32>,
};

@vertex
fn vs_water(
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) nrm_alpha: u32,
    @location(3) slot: u32,
) -> WaterOut {
    let st = object_state(slot);
    var out: WaterOut;
    out.world = pos;
    out.normal = unpack4x8snorm(nrm_alpha).xyz;
    out.uv = uv;
    out.flags = st.flags;
    out.tint = unpack_srgba(st.tint);
    if hidden_in_view(st.flags) {
        out.clip = COLLAPSED;
    } else {
        out.clip = view.view_proj * vec4<f32>(pos, 1.0);
    }
    return out;
}

struct FullOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_full_water(@builtin(vertex_index) vi: u32) -> FullOut {
    let x = f32((vi << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vi & 2u) * 2.0 - 1.0;
    var out: FullOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.ndc = vec2<f32>(x, y);
    return out;
}

fn to_gamma(c: vec3<f32>) -> vec3<f32> {
    let x = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    let hi = 1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, x * 12.92, x <= vec3<f32>(0.0031308));
}

fn depth_px(uv: vec2<f32>) -> vec2<i32> {
    let size = view.viewport.xy;
    let p = clamp(uv * size, vec2<f32>(0.0), size - vec2<f32>(1.0));
    return vec2<i32>(p);
}

fn raw_depth(uv: vec2<f32>) -> f32 {
    return textureLoad(scene_depth, depth_px(uv), 0);
}

fn linear_depth(raw: f32) -> f32 {
    if raw <= 1.0e-7 {
        return FAR_AWAY;
    }
    return wview.depth.y / (raw + wview.depth.x);
}

fn scene_rgb(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(scene_color, samp_clamp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb;
}

fn screen_of(c: vec4<f32>) -> vec2<f32> {
    let ndc = c.xy / c.w;
    return vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
}

fn on_screen(uv: vec2<f32>) -> bool {
    return all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
}

fn sky_face_uv(i: u32, s: f32, t: f32) -> vec2<f32> {
    let uv = vec2<f32>((s + 1.0) * 0.5, (1.0 - t) * 0.5) * view.sky_uv[i].xy + view.sky_uv[i].zw;
    return clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
}

fn sky_color(dir: vec3<f32>) -> vec3<f32> {
    if wview.depth.z < 0.5 {
        return wview.background.rgb;
    }
    let d0 = normalize(dir);
    let c = view.sky.x;
    let s = view.sky.y;
    let d = vec3<f32>(c * d0.x + s * d0.y, -s * d0.x + c * d0.y, d0.z);
    let a = abs(d);
    var color: vec4<f32>;
    if a.x >= a.y && a.x >= a.z {
        if d.x > 0.0 {
            color = textureSampleLevel(sky_rt, samp_clamp, sky_face_uv(0u, -d.y / a.x, d.z / a.x), 0.0);
        } else {
            color = textureSampleLevel(sky_lf, samp_clamp, sky_face_uv(1u, d.y / a.x, d.z / a.x), 0.0);
        }
    } else if a.y >= a.z {
        if d.y > 0.0 {
            color = textureSampleLevel(sky_bk, samp_clamp, sky_face_uv(2u, d.x / a.y, d.z / a.y), 0.0);
        } else {
            color = textureSampleLevel(sky_ft, samp_clamp, sky_face_uv(3u, -d.x / a.y, d.z / a.y), 0.0);
        }
    } else if d.z > 0.0 {
        color = textureSampleLevel(sky_up, samp_clamp, sky_face_uv(4u, -d.y / a.z, -d.x / a.z), 0.0);
    } else {
        color = textureSampleLevel(sky_dn, samp_clamp, sky_face_uv(5u, -d.y / a.z, d.x / a.z), 0.0);
    }
    return color.rgb * bitcast<f32>(view.misc.z);
}

fn infinity_color(dir: vec3<f32>, offset: vec2<f32>) -> vec3<f32> {
    let c = view.view_proj * vec4<f32>(dir, 0.0);
    if c.w > 1.0e-4 {
        let uv = screen_of(c) + offset;
        if on_screen(uv) && raw_depth(uv) <= 1.0e-7 {
            return scene_rgb(uv);
        }
    }
    return sky_color(dir);
}

fn trace_reflection(p: vec3<f32>, r: vec3<f32>, offset: vec2<f32>) -> vec4<f32> {
    let eye = view.eye.xyz;
    let fwd = view.forward.xyz;
    let near = max(view.forward.w, 0.1);
    let d0 = dot(p - eye, fwd);
    let dr = dot(r, fwd);
    var len = SSR_LENGTH;
    if dr < -1.0e-4 {
        len = min(len, (d0 - near * 2.0) / -dr);
    }
    if len < 1.0 {
        return vec4<f32>(0.0);
    }
    let c0 = view.view_proj * vec4<f32>(p, 1.0);
    let c1 = view.view_proj * vec4<f32>(p + r * len, 1.0);
    var prev_t = 0.0;
    var prev_d = c0.w;
    var behind = vec3<f32>(0.0);
    for (var i = 1u; i <= SSR_STEPS; i++) {
        let f = f32(i) / f32(SSR_STEPS);
        let t = f * f;
        let c = mix(c0, c1, t);
        if c.w <= near {
            break;
        }
        let uv = screen_of(c);
        if !on_screen(uv) {
            let edge_uv = clamp(uv, vec2<f32>(0.002), vec2<f32>(0.998));
            let de = linear_depth(raw_depth(edge_uv));
            if behind.z == 0.0 && de < FAR_AWAY && c.w > de {
                behind = vec3<f32>(edge_uv, 1.0 - smoothstep(0.7, 1.0, t));
            }
            break;
        }
        let ds = linear_depth(raw_depth(uv));
        if c.w > ds {
            let thick = max(abs(c.w - prev_d) * 2.0 + 8.0, ds * 0.02);
            if c.w - ds < thick {
                var lo = prev_t;
                var hi = t;
                for (var k = 0u; k < SSR_REFINE; k++) {
                    let mid = (lo + hi) * 0.5;
                    let cm = mix(c0, c1, mid);
                    let dm = linear_depth(raw_depth(screen_of(cm)));
                    if cm.w > dm {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                let hit = screen_of(mix(c0, c1, hi));
                let edge = min(min(hit.x, 1.0 - hit.x), min(hit.y, 1.0 - hit.y));
                let w = smoothstep(0.0, 0.015, edge) * (1.0 - smoothstep(0.7, 1.0, hi));
                return vec4<f32>(scene_rgb(hit + offset), w);
            }
            if behind.z == 0.0 {
                behind = vec3<f32>(uv, 1.0 - smoothstep(0.7, 1.0, t));
            }
        }
        prev_t = t;
        prev_d = c.w;
    }
    if behind.z > 0.0 {
        return vec4<f32>(scene_rgb(behind.xy + offset), behind.z);
    }
    return vec4<f32>(0.0);
}

fn normal_texel(uv: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>) -> vec4<f32> {
    let count = water.frames.z;
    if count <= 1.5 {
        return textureSampleGrad(normal_tex, samp, uv, gx, gy);
    }
    let cols = max(water.frames.x, 1.0);
    let rows = max(water.frames.y, 1.0);
    let frame = floor(view.sky.z * water.frames.w) % count;
    let cell = vec2<f32>(frame % cols, floor(frame / cols));
    let grid = vec2<f32>(cols, rows);
    let local = fract(uv);
    return textureSampleGrad(normal_tex, samp, (cell + local) / grid, gx / grid, gy / grid);
}

fn decode_normal(t: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(to_gamma(t.rgb) * 2.0 - 1.0, t.a);
}

fn water_fog_of(path: f32, start: f32, inv_range: f32) -> f32 {
    return clamp((path - start) * inv_range, 0.0, 1.0);
}

fn eye_dir_at(uv: vec2<f32>) -> vec3<f32> {
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    return view.forward.xyz + view.right.xyz * (ndc.x * view.proj.x) + view.up.xyz * (ndc.y * view.proj.y);
}

fn water_fog_alpha(uv: vec2<f32>, depth: f32, water_z: f32, inv_range: f32) -> f32 {
    let d = min(depth, 1.0e6);
    let zp = view.eye.z + eye_dir_at(uv).z * d;
    let from_water = water_z - zp;
    var from_eye = view.eye.z - zp;
    if abs(from_eye) < 1.0e-4 {
        from_eye = 1.0e-4;
    }
    let f = clamp(from_water / from_eye, 0.0, 1.0);
    return clamp(f * d * inv_range, 0.0, 1.0);
}

fn editor_states(rgb_in: vec3<f32>, flags: u32) -> vec3<f32> {
    var rgb = rgb_in;
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
    return rgb;
}

@fragment
fn fs_water(in: WaterOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let flags = water.flags.x;
    let base_uv = in.uv * water.scroll.zw;
    let time = view.sky.z;
    let bump = base_uv * water.bump_uv.xy + water.bump_uv.zw + water.scroll.xy * time;
    let f45 = vec2<f32>(base_uv.x + base_uv.y, base_uv.y - base_uv.x);
    let extra1 = f45 * 0.1 + water.scroll12.xy * time;
    let extra2 = base_uv.yx * 0.45 + water.scroll12.zw * time;
    let gx = dpdx(bump);
    let gy = dpdy(bump);
    let gx1 = dpdx(extra1);
    let gy1 = dpdy(extra1);
    let gx2 = dpdx(extra2);
    let gy2 = dpdy(extra2);
    let dp1 = dpdx(in.world);
    let dp2 = dpdy(in.world);
    let duv1 = dpdx(base_uv);
    let duv2 = dpdy(base_uv);
    let base = textureSampleGrad(base_tex, samp, bump, gx, gy);

    var ng = vec3<f32>(0.0, 0.0, 1.0);
    let nlen = length(in.normal);
    if nlen > 1.0e-5 {
        ng = in.normal / nlen;
    }
    let eye = view.eye.xyz;
    let to_eye = eye - in.world;
    let below = (flags & W_BELOW) != 0u || dot(to_eye, ng) < 0.0;
    let dp2perp = cross(dp2, ng);
    let dp1perp = cross(ng, dp1);
    var tan_s = dp2perp * duv1.x + dp1perp * duv2.x;
    var tan_t = dp2perp * duv1.y + dp1perp * duv2.y;
    let tmax = max(dot(tan_s, tan_s), dot(tan_t, tan_t));
    if tmax > 1.0e-20 {
        let inv = inverseSqrt(tmax);
        tan_s *= inv;
        tan_t *= inv;
    } else {
        tan_s = vec3<f32>(0.0);
        tan_t = vec3<f32>(0.0);
    }

    var n = vec4<f32>(0.0, 0.0, 1.0, 1.0);
    if (flags & W_NORMAL) != 0u {
        var t = normal_texel(bump, gx, gy);
        if (flags & W_MULTITEXTURE) != 0u {
            let t1 = normal_texel(extra1, gx1, gy1);
            let t2 = normal_texel(extra2, gx2, gy2);
            let g = (to_gamma(t.rgb) + to_gamma(t1.rgb) + to_gamma(t2.rgb)) * 0.33;
            n = vec4<f32>(g * 2.0 - 1.0, (t.a + t1.a + t2.a) * 0.33);
        } else {
            n = decode_normal(t);
        }
    }
    let n_world = normalize(tan_s * n.x + tan_t * n.y + ng * max(n.z, 0.05));

    let uv0 = in.clip.xy * view.viewport.zw;
    let dw = max(dot(in.world - eye, view.forward.xyz), 1.0e-3);
    let off = n.xy * n.w;

    var rgb: vec3<f32>;
    if below {
        var tint = water.refract.rgb;
        var amount = water.refract.w;
        var fog = water.fog;
        var start = water.ranges.x;
        if (flags & W_BOTTOM) != 0u {
            tint = water.bottom_refract.rgb;
            amount = water.bottom_refract.w;
            fog = water.bottom_fog;
            start = water.bottom_ranges.x;
        }
        var uv_r = uv0 + off * amount;
        if linear_depth(raw_depth(uv_r)) < dw {
            uv_r = uv0;
        }
        let refr = scene_rgb(uv_r) * tint;
        let depth_fog = water_fog_of(dw, start, fog.w);
        rgb = mix(refr, fog.rgb, depth_fog);
    } else {
        let water_z = in.world.z;
        let fog0 = water_fog_alpha(uv0, linear_depth(raw_depth(uv0)), water_z, water.fog.w);
        let has_base = (flags & W_BASE) != 0u;
        var scale = fog0;
        if has_base {
            scale = 1.0;
        }
        var uv_r = uv0 + off * (water.refract.w * scale);
        let dr = linear_depth(raw_depth(uv_r));
        if dr < dw {
            uv_r = uv0;
        }
        let fog = water_fog_alpha(uv_r, linear_depth(raw_depth(uv_r)), water_z, water.fog.w);
        var refr = mix(scene_rgb(uv_r), water.fog.rgb, fog) * water.refract.rgb;
        refr = mix(refr, water.fog.rgb, clamp(fog - 0.05, 0.0, 1.0));
        let eye_t = normalize(vec3<f32>(dot(to_eye, tan_s), dot(to_eye, tan_t), dot(to_eye, ng)));
        var fresnel = pow(1.0 - clamp(dot(eye_t, n.xyz), 0.0, 1.0), 5.0);
        if !has_base {
            fresnel *= clamp((fog - 0.05) * 20.0, 0.0, 1.0);
        }
        var refl = vec3<f32>(0.0);
        let want_reflect = (flags & W_REFLECT) != 0u;
        let cheap = (flags & W_CHEAP) != 0u;
        if want_reflect {
            let r = reflect(-normalize(to_eye), ng);
            let roff = vec2<f32>(off.x, -off.y) * (water.reflect.w * scale);
            let rp = normalize(r + (tan_s * off.x + tan_t * off.y) * (water.reflect.w * scale));
            if wview.reflect.x > 0.5 {
                let rc = wview.reflect_vp * vec4<f32>(in.world, 1.0);
                let ruv = clamp(screen_of(rc) + roff, vec2<f32>(0.0), vec2<f32>(1.0));
                refl = textureSampleLevel(reflect_tex, samp_clamp, ruv, 0.0).rgb * water.reflect.rgb;
            } else {
                let hit = trace_reflection(in.world, r, roff);
                refl = mix(infinity_color(rp, roff), hit.rgb, hit.w) * water.reflect.rgb;
            }
        }
        let want_refract = (flags & W_REFRACT) != 0u;
        if want_refract && want_reflect {
            rgb = mix(refr, refl, fresnel);
        } else if want_reflect {
            if has_base {
                rgb = base.rgb + refl * fresnel * base.a;
            } else {
                rgb = refl;
            }
        } else if want_refract {
            rgb = refr;
        } else {
            rgb = scene_rgb(uv0);
        }
        if cheap {
            let e = normalize(to_eye);
            let rv = 2.0 * dot(n_world, e) * n_world - e;
            var spec = infinity_color(rv, vec2<f32>(0.0)) * water.reflect.rgb;
            if (flags & W_NO_ENVMAP) != 0u {
                spec = water.fog.rgb;
            }
            var f = water.bottom_ranges.z;
            if (flags & W_FRESNEL) != 0u {
                let d = 1.0 - max(0.0, dot(e, n_world));
                f = d * d * d * d * d;
            }
            if (flags & W_CHEAP_BLEND) != 0u {
                let span = max(water.ranges.w - water.ranges.z, 1.0);
                let amount = clamp((length(to_eye) - water.ranges.z) / span, 0.0, 1.0);
                var a = clamp(f + amount, 0.0, 1.0);
                if want_refract {
                    a *= clamp((fog0 - 0.05) * 20.0, 0.0, 1.0);
                }
                rgb = mix(rgb, spec, a);
            } else {
                rgb = mix(water.fog.rgb, spec, f);
            }
        }
    }
    rgb = rgb * in.tint.rgb;
    rgb = editor_states(rgb, in.flags);
    rgb = apply_fog(rgb, in.world);
    let eye_under = wview.under_range.z > 0.5;
    let under_face = (flags & W_BELOW) != 0u;
    if (in.flags & F_COMPILED_LIGHT) != 0u && (!front || under_face != eye_under) {
        discard;
    }
    return vec4<f32>(rgb, 1.0);
}

@fragment
fn fs_water_depth(in: WaterOut) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}

@fragment
fn fs_under_fog(in: FullOut) -> @location(0) vec4<f32> {
    let uv = in.clip.xy * view.viewport.zw;
    let d = linear_depth(raw_depth(uv));
    let dir = view.forward.xyz + view.right.xyz * (in.ndc.x * view.proj.x) + view.up.xyz * (in.ndc.y * view.proj.y);
    let dist = d * length(dir);
    let f = clamp((dist - wview.under_range.x) * wview.under_fog.w, 0.0, 1.0);
    return vec4<f32>(wview.under_fog.rgb, f);
}
