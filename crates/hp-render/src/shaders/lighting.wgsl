fn cut_depth_at(m: texture_depth_2d_array, uv: vec2<f32>, layer: u32) -> f32 {
    let dims = vec2<f32>(textureDimensions(m));
    return textureLoad(m, vec2<i32>(clamp(uv * dims, vec2<f32>(0.0), dims - 1.0)), layer, 0);
}

const SUN_TAPS: u32 = 12u;
const SUN_DISK: array<vec2<f32>, 12> = array<vec2<f32>, 12>(
    vec2<f32>(-0.326, -0.406), vec2<f32>(-0.840, -0.074), vec2<f32>(-0.696, 0.457), vec2<f32>(-0.203, 0.621),
    vec2<f32>(0.962, -0.195), vec2<f32>(0.473, -0.480), vec2<f32>(0.519, 0.767), vec2<f32>(0.185, -0.893),
    vec2<f32>(0.507, 0.064), vec2<f32>(0.896, 0.412), vec2<f32>(-0.322, -0.933), vec2<f32>(-0.792, -0.598)
);
const MAX_PENUMBRA_TEXELS: f32 = 24.0;
const MAX_FILTER_TEXELS: f32 = 48.0;

fn shadow_depth_at(uv: vec2<f32>, layer: u32) -> f32 {
    let dims = vec2<f32>(textureDimensions(shadow_map));
    let t = vec2<i32>(clamp(uv * dims, vec2<f32>(0.0), dims - 1.0));
    return textureLoad(shadow_map, t, layer, 0);
}

fn layer_count(v: f32) -> u32 {
    return clamp(u32(v + 0.5), 1u, 8u);
}

fn sky_layer_of(m: texture_depth_2d_array, uv: vec2<f32>, layers: u32, depth: f32) -> i32 {
    for (var k = 0u; k < layers; k++) {
        let c = cut_depth_at(m, uv, k);
        if c <= 0.0 {
            return select(-1, 0, k == 0u);
        }
        if c <= depth {
            return i32(k);
        }
    }
    return -1;
}

fn sun_layer_of(uv: vec2<f32>, layers: u32, depth: f32) -> i32 {
    var found = -1;
    for (var k = 0u; k < layers; k++) {
        let c = cut_depth_at(sun_cut_map, uv, k);
        if c <= 0.0 {
            return select(found, 0, k == 0u);
        }
        if 1.0 - c > depth {
            return found;
        }
        found = i32(k);
    }
    return found;
}

fn sun_receiver_slope(n: vec3<f32>) -> vec2<f32> {
    let m = view.shadow_vp;
    let a = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), abs(n.x) > 0.9);
    let t1 = normalize(cross(n, a));
    let t2 = cross(n, t1);
    let p1 = (m * vec4<f32>(t1, 0.0)).xyz;
    let p2 = (m * vec4<f32>(t2, 0.0)).xyz;
    let u1 = vec2<f32>(p1.x * 0.5, -p1.y * 0.5);
    let u2 = vec2<f32>(p2.x * 0.5, -p2.y * 0.5);
    let det = u1.x * u2.y - u1.y * u2.x;
    if abs(det) < 1e-12 {
        return vec2<f32>(0.0);
    }
    let g = vec2<f32>(p1.z * u2.y - p2.z * u1.y, u1.x * p2.z - u2.x * p1.z) / det;
    let cap = view.shadow.y * 8.0 / max(view.shadow.z, 1e-6);
    let len = length(g);
    return select(g, g * (cap / len), len > cap);
}

fn sun_visibility(world: vec3<f32>, n: vec3<f32>, frag: vec2<f32>, sky3d: bool) -> f32 {
    if view.shadow.x < 0.5 {
        return 1.0;
    }
    let p = view.shadow_vp * vec4<f32>(world + n * view.shadow.w, 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || p.z > 1.0 || p.z < 0.0 {
        return 1.0;
    }
    let found = sun_layer_of(uv, layer_count(view.sun_soft.w), p.z + view.shadow.y);
    if found < 0 && !sky3d {
        return 0.0;
    }
    let layer = u32(max(found, 0));
    let d = p.z - view.shadow.y;
    let slope = sun_receiver_slope(n);
    let spread = view.sun_soft.x;
    let texel_world = max(view.sun_soft.z, 1e-3);
    let a = fract(sin(dot(floor(frag), vec2<f32>(12.9898, 78.233))) * 43758.547) * 6.2831853;
    let rot = mat2x2<f32>(cos(a), sin(a), -sin(a), cos(a));
    var disk = SUN_DISK;
    var radius = 1.0;
    let tap_step = select(1u, 2u, view.quality.x >= 2u);
    if spread > 0.0 {
        let search = clamp(2048.0 * spread / texel_world, 1.0, MAX_PENUMBRA_TEXELS);
        var blockers = 0.0;
        var depth_sum = 0.0;
        for (var i = 0u; i < SUN_TAPS; i += tap_step) {
            let o = rot * disk[i] * (search * view.shadow.z);
            let z = shadow_depth_at(uv + o, layer);
            let dt = d + dot(slope, o);
            if z < dt {
                blockers += 1.0;
                depth_sum += dt - z;
            }
        }
        if blockers < 0.5 {
            return 1.0;
        }
        let gap = max(depth_sum / blockers, 0.0) * view.sun_soft.y;
        radius = clamp(gap * spread / texel_world, 1.0, MAX_FILTER_TEXELS);
    }
    let rot2 = mat2x2<f32>(cos(a + 1.0472), sin(a + 1.0472), -sin(a + 1.0472), cos(a + 1.0472));
    var sum = 0.0;
    var taps = 0.0;
    for (var i = 0u; i < SUN_TAPS; i += tap_step) {
        let o = rot * disk[i] * (radius * view.shadow.z);
        sum += textureSampleCompareLevel(shadow_map, shadow_samp, uv + o, layer, d + dot(slope, o));
        let o2 = rot2 * disk[i] * (radius * 0.55 * view.shadow.z);
        sum += textureSampleCompareLevel(shadow_map, shadow_samp, uv + o2, layer, d + dot(slope, o2));
        taps += 2.0;
    }
    return sum / taps;
}

const SKY_LEVELS: f32 = 7.0;
const SKY_MARCH_STEPS: u32 = 18u;
const SKY_MARCH_FAR: f32 = 1100.0;
const SKY_AZIMUTHS: u32 = 8u;
const SKY_BAND: f32 = 0.2617994;
const SKY_COS: array<f32, 6> = array<f32, 6>(0.991445, 0.923880, 0.793353, 0.608761, 0.382683, 0.130526);
const SKY_SIN: array<f32, 6> = array<f32, 6>(0.130526, 0.382683, 0.608761, 0.793353, 0.923880, 0.991445);
const SKY_AREA: array<f32, 6> = array<f32, 6>(0.135517, 0.126282, 0.108441, 0.083209, 0.052308, 0.017841);
const SKY_OPEN_FLOOR: f32 = 2.112467;
const SKY_DIRS: array<vec2<f32>, 8> = array<vec2<f32>, 8>(
    vec2<f32>(1.0, 0.0),
    vec2<f32>(0.70710678, 0.70710678),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(-0.70710678, 0.70710678),
    vec2<f32>(-1.0, 0.0),
    vec2<f32>(-0.70710678, -0.70710678),
    vec2<f32>(0.0, -1.0),
    vec2<f32>(0.70710678, -0.70710678),
);

fn sky_uv(xy: vec2<f32>) -> vec2<f32> {
    return vec2<f32>((xy.x - view.sky_vis_a.x) * view.sky_vis_a.z, 1.0 - (xy.y - view.sky_vis_a.y) * view.sky_vis_a.z);
}

fn sky_layer(xy: vec2<f32>, z: f32) -> i32 {
    let uv = sky_uv(xy);
    if any(uv < vec2<f32>(0.0)) || any(uv >= vec2<f32>(1.0)) {
        return 0;
    }
    let depth = (view.sky_vis_b.x - z) / max(view.sky_vis_b.y, 1.0);
    return sky_layer_of(sky_cut_map, uv, layer_count(view.sky_vis_b.w), depth);
}

fn sky_height(xy: vec2<f32>, layer: u32) -> f32 {
    let uv = sky_uv(xy);
    if any(uv < vec2<f32>(0.0)) || any(uv >= vec2<f32>(1.0)) {
        return -1.0e9;
    }
    let dims = vec2<f32>(textureDimensions(sky_map));
    let d = textureLoad(sky_map, vec2<i32>(min(uv * dims, dims - 1.0)), layer, 0);
    if d >= 1.0 {
        return -1.0e9;
    }
    return view.sky_vis_b.x - d * view.sky_vis_b.y;
}

fn sky_height_lod(xy: vec2<f32>, layer: u32, level: u32) -> f32 {
    let uv = sky_uv(xy);
    if any(uv < vec2<f32>(0.0)) || any(uv >= vec2<f32>(1.0)) {
        return -1.0e9;
    }
    let dims = vec2<f32>(textureDimensions(sky_map, level));
    let d = textureLoad(sky_map, vec2<i32>(min(uv * dims, dims - 1.0)), layer, level);
    if d >= 1.0 {
        return -1.0e9;
    }
    return view.sky_vis_b.x - d * view.sky_vis_b.y;
}

fn sky_horizon(xy: vec2<f32>, dir: vec2<f32>, z: f32, layer: u32) -> f32 {
    let texel = max(view.sky_vis_b.z, 1.0);
    var horizon = (sky_height(xy, layer) - z) / max(texel * 0.5, 0.25);
    var r = 10.0;
    for (var i = 0u; i < SKY_MARCH_STEPS; i++) {
        var level: f32;
        var next: f32;
        if view.sky_march[0].z > 0.5 {
            r = view.sky_march[i].x;
            level = view.sky_march[i].y;
            next = SKY_MARCH_FAR + 1.0;
            if i + 1u < SKY_MARCH_STEPS {
                next = view.sky_march[i + 1u].x;
            }
        } else {
            level = clamp(round(log2(max(0.2 * r / texel, 1.0))), 0.0, SKY_LEVELS);
            next = r + texel * exp2(level) * 1.6;
        }
        let h = sky_height_lod(xy + dir * r, layer, u32(level));
        horizon = max(horizon, (h - z) / r);
        r = next;
        if r > SKY_MARCH_FAR {
            break;
        }
    }
    return horizon;
}

fn sky_visibility(world: vec3<f32>, n: vec3<f32>, sky3d: bool) -> f32 {
    let texel = max(view.sky_vis_b.z, 1.0);
    let z = world.z + max(texel * 1.5, 2.0);
    let found = sky_layer(world.xy, world.z - texel);
    if found < 0 && !sky3d {
        return 0.0;
    }
    let layer = u32(max(found, 0));
    let base = world.xy + n.xy * (texel * 2.0);
    var coss = SKY_COS;
    var sins = SKY_SIN;
    var areas = SKY_AREA;
    var dirs = SKY_DIRS;
    var sum = 0.0;
    for (var a = 0u; a < SKY_AZIMUTHS; a++) {
        let dir = dirs[a];
        if dot(n.xy, dir) <= 0.0 && n.z <= 0.0 {
            continue;
        }
        let horizon = atan(sky_horizon(base, dir, z, layer));
        for (var k = 0u; k < 6u; k++) {
            let open = clamp((f32(k + 1u) * SKY_BAND - horizon) / SKY_BAND, 0.0, 1.0);
            sum += areas[k] * open * max(dot(n, vec3<f32>(dir * coss[k], sins[k])), 0.0);
        }
    }
    return sum / SKY_OPEN_FLOOR;
}

fn ambient_term(world: vec3<f32>, ln: vec3<f32>, sky3d: bool) -> f32 {
    let len = length(ln);
    if view.sky_vis_a.w < 0.5 || len < 1e-5 {
        return min(len, 1.0);
    }
    return sky_visibility(world, ln / len, sky3d) * min(len, 1.0);
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
    if view.quality.x >= 2u {
        var quad = 0.0;
        for (var k = 0u; k < 4u; k++) {
            let o = (vec2<f32>(f32(k & 1u), f32(k >> 1u)) - vec2<f32>(0.5)) * step;
            quad += textureSampleCompareLevel(local_shadow_maps, shadow_samp, uv + o, layer, c.z);
        }
        return quad / 4.0;
    }
    var sum = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let o = vec2<f32>(f32(x), f32(y)) * step;
            sum += textureSampleCompareLevel(local_shadow_maps, shadow_samp, uv + o, layer, c.z);
        }
    }
    return sum / 9.0;
}

const INDIRECT_PROBES: u32 = 16u;

fn indirect_cell(c: vec3<i32>) -> i32 {
    let k0 = u32(c.x + 32768) & 0xffffu | ((u32(c.y + 32768) & 0xffffu) << 16u);
    let k1 = u32(c.z + 32768) & 0xffffu;
    let mask = bitcast<u32>(view.indirect_b.x);
    var h = (k0 * 0x9E3779B1u) ^ (k1 * 0x85EBCA77u);
    h = h ^ (h >> 15u);
    for (var i = 0u; i < INDIRECT_PROBES; i++) {
        let e = indirect_buf[(h + i) & mask];
        if e.w == 0u {
            return -1;
        }
        if e.x == k0 && e.y == k1 {
            return i32(e.z);
        }
    }
    return -1;
}

fn indirect_color(cell: u32, faces: vec3<u32>, sq: vec3<f32>) -> vec3<f32> {
    let base = bitcast<u32>(view.indirect_b.y) + cell * 3u;
    var halves: array<f32, 24>;
    for (var k = 0u; k < 3u; k++) {
        let v = indirect_buf[base + k];
        let a = unpack2x16float(v.x);
        let b = unpack2x16float(v.y);
        let c = unpack2x16float(v.z);
        let d = unpack2x16float(v.w);
        halves[k * 8u] = a.x;
        halves[k * 8u + 1u] = a.y;
        halves[k * 8u + 2u] = b.x;
        halves[k * 8u + 3u] = b.y;
        halves[k * 8u + 4u] = c.x;
        halves[k * 8u + 5u] = c.y;
        halves[k * 8u + 6u] = d.x;
        halves[k * 8u + 7u] = d.y;
    }
    let fx = faces.x * 3u;
    let fy = faces.y * 3u;
    let fz = faces.z * 3u;
    return sq.x * vec3<f32>(halves[fx], halves[fx + 1u], halves[fx + 2u])
        + sq.y * vec3<f32>(halves[fy], halves[fy + 1u], halves[fy + 2u])
        + sq.z * vec3<f32>(halves[fz], halves[fz + 1u], halves[fz + 2u]);
}

fn indirect_light(world: vec3<f32>, ln: vec3<f32>) -> vec4<f32> {
    let len = length(ln);
    if len < 1e-5 {
        return vec4<f32>(0.0);
    }
    let n = ln / len;
    let at = world + n * (0.5 / view.indirect_a.w);
    let g = (at - view.indirect_a.xyz) * view.indirect_a.w - vec3<f32>(0.5);
    let base = vec3<i32>(floor(g));
    let frac = g - floor(g);
    let faces = vec3<u32>(select(0u, 1u, n.x < 0.0), select(2u, 3u, n.y < 0.0), select(4u, 5u, n.z < 0.0));
    let sq = n * n;
    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    for (var corner = 0u; corner < 8u; corner++) {
        let off = vec3<i32>(i32(corner & 1u), i32((corner >> 1u) & 1u), i32((corner >> 2u) & 1u));
        let w3 = select(1.0 - frac, frac, vec3<bool>(off.x == 1, off.y == 1, off.z == 1));
        let w = w3.x * w3.y * w3.z;
        if w <= 0.0 {
            continue;
        }
        let cell = indirect_cell(base + off);
        if cell < 0 {
            continue;
        }
        sum += w * indirect_color(u32(cell), faces, sq);
        weight += w;
    }
    return vec4<f32>(sum / max(weight, 1e-4) * min(len, 1.0), weight);
}

fn lighting(world: vec3<f32>, n: vec3<f32>, ln: vec3<f32>, frag: vec2<f32>, flags: u32, model: bool) -> vec3<f32> {
    return lighting_world(world, n, ln, frag, flags, model) + projector_light(world, ln);
}

fn lighting_world(world: vec3<f32>, n: vec3<f32>, ln: vec3<f32>, frag: vec2<f32>, flags: u32, model: bool) -> vec3<f32> {
    let sky3d = (flags & F_SKY_3D) != 0u;
    var sum = vec3<f32>(0.0);
    var cover = 0.0;
    var indirect = vec3<f32>(0.0);
    if view.indirect_b.w > 0.5 {
        let ind = indirect_light(world, ln);
        indirect = ind.rgb;
        cover = smoothstep(0.02, 0.5, ind.a);
    }
    if cover < 0.999 && (view.ambient.r + view.ambient.g + view.ambient.b) > 0.0 {
        sum = view.ambient.rgb * ambient_term(world, ln, sky3d);
    }
    sum = mix(sum, indirect, cover);
    return sum + direct_lighting(world, n, ln, frag, flags, model, false);
}

fn direct_lighting(world: vec3<f32>, n: vec3<f32>, ln: vec3<f32>, frag: vec2<f32>, flags: u32, model: bool, delta: bool) -> vec3<f32> {
    return direct_lighting_from(world, n, ln, frag, flags, model, delta, world);
}

fn direct_lighting_from(world: vec3<f32>, n: vec3<f32>, ln: vec3<f32>, frag: vec2<f32>, flags: u32, model: bool, delta: bool, shadow_world: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let sky3d = (flags & F_SKY_3D) != 0u;
    let n_dir = select(min(view.misc.y, arrayLength(&lights)), 0u, delta);
    for (var i = 0u; i < n_dir; i++) {
        let l = lights[i];
        let ndl = max(dot(ln, -l.dir.xyz), 0.0);
        var vis = 1.0;
        if i == 0u && ndl > 0.0 {
            vis = sun_visibility(world, n, frag, sky3d);
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
        if (idx >> 1u) >= n_idx {
            break;
        }
        let li = (light_indices[idx >> 1u] >> ((idx & 1u) * 16u)) & 0xffffu;
        if li >= n_lights {
            continue;
        }
        let reach = lights[li].pos_range;
        let shape = lights[li].cone;
        let rd = reach.xyz - world;
        let rdist = sqrt(max(dot(rd, rd), 1e-4));
        if select(rdist > shape.w, rdist / max(reach.w, 1e-3) >= 1.0, shape.w <= shape.z) {
            continue;
        }
        let l = lights[li];
        if !delta && l.delta_color.w > 0.5 { continue; }
        let color = select(l.color.rgb, l.delta_color.rgb, delta);
        if all(color == vec3<f32>(0.0)) { continue; }
        let d = l.pos_range.xyz - world;
        let dist2 = max(dot(d, d), 1e-4);
        let dist = sqrt(dist2);
        let to_light = d / dist;
        let ndl = max(dot(ln, to_light), 0.0);
        let x = dist / max(l.pos_range.w, 1e-3);
        let x2 = x * x;
        var win = 1.0;
        if l.cone.w <= l.cone.z {
            win = clamp(1.0 - x2 * x2, 0.0, 1.0);
            win *= win;
        }
        var att = light_falloff(l, dist, model) * win;
        if l.color.w > 0.5 && l.color.w < 1.5 {
            att *= spot_cone(l, dot(-to_light, l.dir.xyz));
        }
        if att > 0.0 && ndl > 0.0 {
            att *= local_visibility(l, shadow_world, n);
        }
        sum += color * (ndl * att);
    }
    return sum;
}
