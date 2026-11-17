struct Node {
    mn: vec3<f32>,
    a: u32,
    mx: vec3<f32>,
    b: u32,
}

struct Tri {
    v0: vec3<f32>,
    id: u32,
    e1: vec3<f32>,
    p1: u32,
    e2: vec3<f32>,
    p2: u32,
}

struct Attr {
    info: vec4<u32>,
    uv01: vec4<f32>,
    uv2lm0: vec4<f32>,
    lm12: vec4<f32>,
    normal: vec4<f32>,
}

struct Params {
    seed: u32,
    bounces: u32,
    sky_samples: u32,
    style_bounce: u32,
    ao_radius: f32,
    clamp_lum: f32,
    passes: u32,
    nitems: u32,
    ncameras: u32,
    nbounce: u32,
    clear_root: u32,
    clear_tri: u32,
    has_clear: u32,
    mat_off: u32,
    light_off: u32,
    surf_off: u32,
    cam_off: u32,
    lists_off: u32,
    blights_off: u32,
    bcdf_off: u32,
    ainfo_off: u32,
    adata_off: u32,
    valid_off: u32,
    stats_base: u32,
    points_base: u32,
    item_base: u32,
    rq_ids: u32,
    sky_light: u32,
    contact_ao: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
}

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read> nodes: array<Node>;
@group(0) @binding(2) var<storage, read> tris: array<Tri>;
@group(0) @binding(3) var<storage, read> attrs: array<Attr>;
@group(0) @binding(4) var<storage, read> table: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read> words: array<u32>;
@group(0) @binding(6) var<storage, read> cache: array<vec4<f32>>;
@group(0) @binding(7) var<storage, read> batch: array<u32>;
@group(0) @binding(8) var<storage, read_write> outbuf: array<f32>;

const NONE: u32 = 0xffffffffu;
const LEAF: u32 = 0x80000000u;
const MAX_TRACE: f32 = 56755.84;
const TIE: f32 = 0.05;
const SURFACE_EPSILON: f32 = 0.03125;
const BOUNCE_OFFSET: f32 = 0.125;
const PI: f32 = 3.14159265358979;
const TAU: f32 = 6.28318530717959;
const R2_A: f32 = 0.75487767;
const R2_B: f32 = 0.56984025;
const R1_A: f32 = 0.618034;
const DIM_POINT: u32 = 0u;
const DIM_SKY: u32 = 1u;
const DIM_AO: u32 = 2u;
const DIM_PATH: u32 = 3u;
const DIM_AO_REACH: u32 = 4u;
const CONTACT_REACH: f32 = 1.5;
const DIM_LIGHT: u32 = 16u;
const K_OPAQUE: u32 = 0u;
const K_SKY: u32 = 1u;
const K_TRANSLUCENT: u32 = 2u;
const K_BLOCKER: u32 = 4u;

fn pcg(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn hash3(a: u32, b: u32, c: u32) -> u32 {
    return pcg(a ^ pcg(b ^ pcg(c)));
}

fn to_unit(h: u32) -> f32 {
    return f32(h >> 8u) * (1.0 / 16777216.0);
}

fn rnd(state: ptr<function, u32>) -> f32 {
    *state = pcg(*state);
    return to_unit(*state);
}

fn fract1(x: f32) -> f32 {
    return x - floor(x);
}

fn r2(n: u32, seed: u32, luxel: u32, dim: u32) -> vec2<f32> {
    let h = hash3(seed ^ 0x5bd1e995u, luxel, dim);
    let ox = to_unit(h);
    let oy = to_unit(pcg(h));
    let nf = f32(n);
    return vec2<f32>(fract1(ox + nf * R2_A), fract1(oy + nf * R2_B));
}

fn r1(n: u32, seed: u32, luxel: u32, dim: u32) -> f32 {
    let o = to_unit(hash3(seed ^ 0x68e31da4u, luxel, dim));
    return fract1(o + f32(n) * R1_A);
}

struct Frame {
    t: vec3<f32>,
    b: vec3<f32>,
}

fn frame(n: vec3<f32>) -> Frame {
    let sign = select(-1.0, 1.0, n.z >= 0.0);
    let a = -1.0 / (sign + n.z);
    let b = n.x * n.y * a;
    var f: Frame;
    f.t = vec3<f32>(1.0 + sign * n.x * n.x * a, sign * b, -sign * n.x);
    f.b = vec3<f32>(b, sign + n.y * n.y * a, -n.y);
    return f;
}

fn safe_normalize(v: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {
    let l = length(v);
    if (l > 1e-12 && l == l) {
        return v / l;
    }
    return fallback;
}

fn cosine_dir(n: vec3<f32>, u: f32, v: f32) -> vec3<f32> {
    let r = sqrt(u);
    let phi = TAU * v;
    let f = frame(n);
    let z = sqrt(max(1.0 - u, 0.0));
    return safe_normalize(f.t * (r * cos(phi)) + f.b * (r * sin(phi)) + n * z, n);
}

fn cone_dir(axis: vec3<f32>, sin_max: f32, u: f32, v: f32) -> vec3<f32> {
    if (sin_max <= 0.0) {
        return axis;
    }
    let cos_max = sqrt(max(1.0 - sin_max * sin_max, 0.0));
    let z = 1.0 - u * (1.0 - cos_max);
    let r = sqrt(max(1.0 - z * z, 0.0));
    let phi = TAU * v;
    let f = frame(axis);
    return safe_normalize(f.t * (r * cos(phi)) + f.b * (r * sin(phi)) + axis * z, axis);
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn oct_decode(p: u32) -> vec3<f32> {
    let e = unpack2x16snorm(p);
    var v = vec3<f32>(e.x, e.y, 1.0 - abs(e.x) - abs(e.y));
    if (v.z < 0.0) {
        let sx = select(-1.0, 1.0, v.x >= 0.0);
        let sy = select(-1.0, 1.0, v.y >= 0.0);
        let ox = (1.0 - abs(v.y)) * sx;
        let oy = (1.0 - abs(v.x)) * sy;
        v = vec3<f32>(ox, oy, v.z);
    }
    return normalize(v);
}

fn mat_kind(m: u32) -> u32 {
    return u32(table[P.mat_off + m * 3u].w);
}

fn mat_albedo(m: u32) -> vec3<f32> {
    return table[P.mat_off + m * 3u].xyz;
}

fn mat_transmit(m: u32) -> vec4<f32> {
    return table[P.mat_off + m * 3u + 1u];
}

fn mat_info(m: u32) -> vec4<u32> {
    return bitcast<vec4<u32>>(table[P.mat_off + m * 3u + 2u]);
}

fn alpha_byte(at: u32) -> u32 {
    let w = words[P.adata_off + (at >> 2u)];
    return (w >> ((at & 3u) * 8u)) & 255u;
}

fn alpha_at(id: u32, u: f32, v: f32) -> f32 {
    let a = attrs[id];
    let info = mat_info(a.info.x);
    if (info.x == NONE) {
        return 1.0;
    }
    let base = P.ainfo_off + info.x * 4u;
    let off = words[base];
    let w = words[base + 1u];
    let h = words[base + 2u];
    if (w == 0u || h == 0u) {
        return 1.0;
    }
    let bw = 1.0 - u - v;
    let tu = a.uv01.x * bw + a.uv01.z * u + a.uv2lm0.x * v;
    let tv = a.uv01.y * bw + a.uv01.w * u + a.uv2lm0.y * v;
    let xi = i32(floor(tu * f32(w)));
    let yi = i32(floor(tv * f32(h)));
    let x = u32(((xi % i32(w)) + i32(w)) % i32(w));
    let y = u32(((yi % i32(h)) + i32(h)) % i32(h));
    return f32(alpha_byte(off + y * w + x)) * (1.0 / 255.0);
}

fn solid(id: u32, u: f32, v: f32) -> bool {
    let m = attrs[id].info.x;
    let info = mat_info(m);
    if (info.x == NONE) {
        return true;
    }
    return alpha_at(id, u, v) >= mat_transmit(m).w;
}

fn priority(id: u32) -> u32 {
    let a = attrs[id];
    let k = mat_kind(a.info.x);
    if (k == K_BLOCKER) {
        return 0u;
    }
    if (k == K_SKY) {
        return 1u;
    }
    if (a.info.y != NONE) {
        return 3u;
    }
    return 2u;
}

struct Hit {
    t: f32,
    u: f32,
    v: f32,
    id: u32,
    front: bool,
    ok: bool,
}

fn visible(o: vec3<f32>, d: vec3<f32>, dist: f32) -> vec3<f32> {
    if (dist <= 1e-3) {
        return vec3<f32>(1.0);
    }
    if (any_opaque(o, d, 1e-3, dist)) {
        return vec3<f32>(0.0);
    }
    return transmit(o, d, dist);
}

fn sky_once(o: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    let h = closest(o, d, MAX_TRACE);
    if (!h.ok) {
        return vec3<f32>(0.0);
    }
    if (mat_kind(attrs[h.id].info.x) != K_SKY) {
        return vec3<f32>(0.0);
    }
    return transmit(o, d, h.t);
}

fn sky_visible(o: vec3<f32>, d: vec3<f32>, in_skybox: bool) -> vec3<f32> {
    var v = sky_once(o, d);
    if (max(max(v.x, v.y), v.z) <= 0.0 || in_skybox) {
        return v;
    }
    for (var c = 0u; c < P.ncameras; c = c + 1u) {
        let cam = table[P.cam_off + c];
        let p = cam.xyz + o * cam.w;
        v = v * sky_once(p, d);
        if (max(max(v.x, v.y), v.z) <= 0.0) {
            return vec3<f32>(0.0);
        }
    }
    return v;
}

struct LightC {
    f0: vec3<f32>,
    f1: vec3<f32>,
    f2: vec3<f32>,
    f3: vec3<f32>,
    ok: bool,
}

fn falloff(li: u32, to_light: vec3<f32>, dist: f32) -> f32 {
    let base = P.light_off + li * 7u;
    let kind = u32(table[base].w);
    let dir = table[base + 1u].xyz;
    if (kind == 2u) {
        let dot2 = max(-dot(to_light, dir), 0.0);
        let f = dot2 / (dist * dist);
        return select(0.0, f, f == f && f < 1e30);
    }
    let att = table[base + 4u];
    let spot = table[base + 5u];
    let start = spot.x;
    let end = spot.y;
    let hard = end > start;
    if (hard && dist > end) {
        return 0.0;
    }
    let dd = max(dist, 1.0);
    let e = min(dd, att.w);
    let denom = att.x + e * (att.y + e * att.z);
    var f = select(0.0, 1.0 / denom, denom > 0.0);
    if (kind == 1u) {
        let stopdot = spot.z;
        let stopdot2 = spot.w;
        let exponent = table[base + 6u].x;
        let dot2 = -dot(to_light, dir);
        if (dot2 <= stopdot2) {
            return 0.0;
        }
        f = f * dot2;
        if (dot2 <= stopdot && stopdot > stopdot2) {
            var mult = clamp((dot2 - stopdot2) / (stopdot - stopdot2), 0.0, 1.0);
            if (exponent != 0.0 && exponent != 1.0) {
                mult = pow(mult, exponent);
            }
            f = f * mult;
        }
    }
    if (hard) {
        let t = 1.0 - clamp((dd - start) / (end - start), 0.0, 1.0);
        f = f * (t * t * t * (t * (t * 6.0 - 15.0) + 10.0));
    }
    return select(0.0, max(f, 0.0), f == f && f < 1e30);
}

fn light_contrib(li: u32, pos: vec3<f32>, n0: vec3<f32>, n1: vec3<f32>, n2: vec3<f32>, n3: vec3<f32>, nn: u32, jitter: vec2<f32>, in_skybox: bool) -> LightC {
    var r: LightC;
    r.ok = false;
    r.f0 = vec3<f32>(0.0);
    r.f1 = vec3<f32>(0.0);
    r.f2 = vec3<f32>(0.0);
    r.f3 = vec3<f32>(0.0);
    let base = P.light_off + li * 7u;
    let L0 = table[base];
    let kind = u32(L0.w);
    let ldir = table[base + 1u].xyz;
    let reach = table[base + 2u].w;
    let size = table[base + 3u].w;
    if (kind <= 2u) {
        let center = L0.xyz;
        var lp = center;
        if (size > 0.0) {
            let to = safe_normalize(center - pos, vec3<f32>(0.0, 0.0, 1.0));
            let f = frame(to);
            let rr = sqrt(jitter.x);
            let phi = TAU * jitter.y;
            lp = lp + (f.t * (rr * cos(phi)) + f.b * (rr * sin(phi))) * size;
        }
        let delta = center - pos;
        let dist = length(delta);
        if (!(dist > 1e-4) || dist > reach) {
            return r;
        }
        let dir = delta / dist;
        if (dot(dir, n0) <= 0.0) {
            return r;
        }
        let fall = falloff(li, dir, dist);
        if (fall <= 0.0) {
            return r;
        }
        var tgt = lp;
        if (kind == 2u) {
            tgt = lp + ldir * SURFACE_EPSILON;
        }
        let to = tgt - pos;
        let len = length(to);
        var vis = vec3<f32>(1.0);
        if (len > 1e-4) {
            vis = visible(pos, to / len, len);
        }
        if (max(max(vis.x, vis.y), vis.z) <= 0.0) {
            return r;
        }
        let v = vis * fall;
        r.f0 = v * dot(dir, n0);
        if (nn > 1u) {
            r.f1 = v * max(dot(dir, n1), 0.0);
            r.f2 = v * max(dot(dir, n2), 0.0);
            r.f3 = v * max(dot(dir, n3), 0.0);
        }
        r.ok = true;
        return r;
    }
    if (kind == 3u) {
        let dir = cone_dir(-ldir, size, jitter.x, jitter.y);
        let d0 = dot(dir, n0);
        if (d0 <= 0.0) {
            return r;
        }
        let vis = sky_visible(pos, dir, in_skybox);
        if (max(max(vis.x, vis.y), vis.z) <= 0.0) {
            return r;
        }
        r.f0 = vis * d0;
        if (nn > 1u) {
            let c1 = max(dot(n1, dir), 0.0);
            let c2 = max(dot(n2, dir), 0.0);
            let c3 = max(dot(n3, dir), 0.0);
            r.f1 = vis * ((c1 + c1 * c1) * 0.5);
            r.f2 = vis * ((c2 + c2 * c2) * 0.5);
            r.f3 = vis * ((c3 + c3 * c3) * 0.5);
        }
        r.ok = true;
    }
    return r;
}

fn region_angle(n0: vec3<f32>, nk: vec3<f32>) -> f32 {
    let c = clamp(dot(n0, nk), -1.0, 1.0);
    return max(TAU - 2.0 * acos(c), 1e-3);
}

fn sky_ambient(pos: vec3<f32>, n0: vec3<f32>, n1: vec3<f32>, n2: vec3<f32>, n3: vec3<f32>, nn: u32, in_skybox: bool, luxel: u32, k: u32) -> LightC {
    var r: LightC;
    r.f0 = vec3<f32>(0.0);
    r.f1 = vec3<f32>(0.0);
    r.f2 = vec3<f32>(0.0);
    r.f3 = vec3<f32>(0.0);
    r.ok = true;
    let count = max(P.sky_samples, 1u);
    for (var j = 0u; j < count; j = j + 1u) {
        let uv = r2(k * count + j, P.seed, luxel, DIM_SKY);
        var d: vec3<f32>;
        if (nn > 1u) {
            let lobe = min(u32(uv.y * 4.0), 3u);
            let vv = uv.y * 4.0 - f32(lobe);
            var ln = n0;
            if (lobe == 1u) {
                ln = n1;
            } else if (lobe == 2u) {
                ln = n2;
            } else if (lobe == 3u) {
                ln = n3;
            }
            d = cosine_dir(ln, uv.x, vv);
        } else {
            d = cosine_dir(n0, uv.x, uv.y);
        }
        let c0 = dot(d, n0);
        if (c0 <= 0.0) {
            continue;
        }
        let vis = sky_visible(pos, d, in_skybox);
        if (max(max(vis.x, vis.y), vis.z) <= 0.0) {
            continue;
        }
        let w = vis / f32(count);
        if (nn == 1u) {
            r.f0 = r.f0 + w;
        } else {
            let c1 = max(dot(d, n1), 0.0);
            let c2 = max(dot(d, n2), 0.0);
            let c3 = max(dot(d, n3), 0.0);
            let sum = c0 + c1 + c2 + c3;
            if (sum <= 1e-6) {
                continue;
            }
            r.f0 = r.f0 + w * (2.0 / region_angle(n0, n0)) * TAU * 2.0 * c0 / sum;
            r.f1 = r.f1 + w * (2.0 / region_angle(n0, n1)) * TAU * 2.0 * c1 / sum;
            r.f2 = r.f2 + w * (2.0 / region_angle(n0, n2)) * TAU * 2.0 * c2 / sum;
            r.f3 = r.f3 + w * (2.0 / region_angle(n0, n3)) * TAU * 2.0 * c3 / sum;
        }
    }
    return r;
}

fn surf_dims(s: u32) -> vec4<u32> {
    return bitcast<vec4<u32>>(table[P.surf_off + s * 4u]);
}

fn surf_styles(s: u32) -> vec4<u32> {
    return bitcast<vec4<u32>>(table[P.surf_off + s * 4u + 1u]);
}

fn surf_info(s: u32) -> vec4<u32> {
    return bitcast<vec4<u32>>(table[P.surf_off + s * 4u + 2u]);
}

fn surf_extra(s: u32) -> vec4<u32> {
    return bitcast<vec4<u32>>(table[P.surf_off + s * 4u + 3u]);
}

fn slot_of(styles: vec4<u32>, slots: u32, style: u32) -> u32 {
    for (var i = 0u; i < slots; i = i + 1u) {
        if (styles[i] == style) {
            return i;
        }
    }
    return NONE;
}

var<private> acc: array<vec3<f32>, 32>;

fn add_acc(ns: u32, nn: u32, ps: u32, slot: u32, n: u32, v: vec3<f32>) {
    acc[(ps * ns + slot) * nn + n] = acc[(ps * ns + slot) * nn + n] + v;
}

struct PointData {
    pos: vec3<f32>,
    n0: vec3<f32>,
    n1: vec3<f32>,
    n2: vec3<f32>,
    n3: vec3<f32>,
}

fn load_point(i: u32) -> PointData {
    let b = P.points_base + i * 8u;
    var p: PointData;
    p.pos = vec3<f32>(bitcast<f32>(batch[b]), bitcast<f32>(batch[b + 1u]), bitcast<f32>(batch[b + 2u]));
    p.n0 = oct_decode(batch[b + 3u]);
    p.n1 = oct_decode(batch[b + 4u]);
    p.n2 = oct_decode(batch[b + 5u]);
    p.n3 = oct_decode(batch[b + 6u]);
    return p;
}

fn direct_sample(si: u32, luxel: u32, k: u32, pfirst: u32, pcount: u32) -> f32 {
    let dims = surf_dims(si);
    let styles = surf_styles(si);
    let info = surf_info(si);
    let nn = dims.z;
    let ns = dims.w;
    let in_skybox = info.w != 0u;
    let pi = min(u32(r1(k, P.seed, luxel, DIM_POINT) * f32(pcount)), pcount - 1u);
    let p = load_point(pfirst + pi);
    for (var j = 0u; j < info.z; j = j + 1u) {
        let li = words[P.lists_off + info.y + j];
        let base = P.light_off + li * 7u;
        let style = u32(table[base + 1u].w);
        let slot = slot_of(styles, ns, style);
        if (slot == NONE) {
            continue;
        }
        let kind = u32(table[base].w);
        var c: LightC;
        if (kind == 4u) {
            c = sky_ambient(p.pos, p.n0, p.n1, p.n2, p.n3, nn, in_skybox, luxel, k);
        } else {
            c = light_contrib(li, p.pos, p.n0, p.n1, p.n2, p.n3, nn, r2(k, P.seed, luxel, DIM_LIGHT + j), in_skybox);
            if (!c.ok) {
                continue;
            }
        }
        for (var ps = 0u; ps < P.passes; ps = ps + 1u) {
            let inten = table[base + 2u + ps].xyz;
            add_acc(ns, nn, ps, slot, 0u, inten * c.f0);
            if (nn > 1u) {
                add_acc(ns, nn, ps, slot, 1u, inten * c.f1);
                add_acc(ns, nn, ps, slot, 2u, inten * c.f2);
                add_acc(ns, nn, ps, slot, 3u, inten * c.f3);
            }
        }
    }
    if (P.ao_radius > 0.0) {
        let uv = r2(k, P.seed, luxel, DIM_AO);
        let d = cosine_dir(p.n0, uv.x, uv.y);
        let base = p.pos - p.n0 * 0.75;
        var reach = P.ao_radius;
        if (P.contact_ao != 0u) {
            let w = r1(k, P.seed, luxel, DIM_AO_REACH);
            reach = P.ao_radius * CONTACT_REACH * (1.0 - sqrt(max(1.0 - w, 0.0)));
        }
        if (reach > 0.25 && any_opaque(base, d, 0.25, reach)) {
            return 0.0;
        }
    }
    return 1.0;
}

fn cache_lookup(si: u32, s: f32, t: f32, ps: u32, slot: u32) -> vec3<f32> {
    let dims = surf_dims(si);
    let w = dims.x;
    let h = dims.y;
    if (w == 0u || h == 0u) {
        return vec3<f32>(0.0);
    }
    let x = clamp(s, 0.0, f32(w - 1u));
    let y = clamp(t, 0.0, f32(h - 1u));
    let x0 = u32(floor(x));
    let y0 = u32(floor(y));
    let x1 = min(x0 + 1u, w - 1u);
    let y1 = min(y0 + 1u, h - 1u);
    let fx = x - f32(x0);
    let fy = y - f32(y0);
    let base = surf_extra(si).x + (ps * dims.w + slot) * w * h;
    let first = surf_info(si).x;
    var sum = vec3<f32>(0.0);
    var wsum = 0.0;
    let xs = array<u32, 4>(x0, x1, x0, x1);
    let ys = array<u32, 4>(y0, y0, y1, y1);
    let ws = array<f32, 4>((1.0 - fx) * (1.0 - fy), fx * (1.0 - fy), (1.0 - fx) * fy, fx * fy);
    for (var c = 0u; c < 4u; c = c + 1u) {
        let i = ys[c] * w + xs[c];
        let wt = ws[c];
        if (wt > 0.0 && words[P.valid_off + first + i] != 0u) {
            sum = sum + cache[base + i].xyz * wt;
            wsum = wsum + wt;
        }
    }
    if (wsum > 1e-6) {
        return sum / wsum;
    }
    return vec3<f32>(0.0);
}

var<private> sample_acc: array<vec3<f32>, 8>;

fn bounce_nee(pos: vec3<f32>, n: vec3<f32>, rng: ptr<function, u32>, in_skybox: bool, styles: vec4<u32>, ns: u32, scale: vec3<f32>) {
    if (P.nbounce > 0u) {
        let x = rnd(rng);
        var lo = 0u;
        var hi = P.nbounce - 1u;
        loop {
            if (lo >= hi) {
                break;
            }
            let mid = (lo + hi) / 2u;
            if (bitcast<f32>(words[P.bcdf_off + mid]) < x) {
                lo = mid + 1u;
            } else {
                hi = mid;
            }
        }
        let kk = lo;
        var prev = 0.0;
        if (kk > 0u) {
            prev = bitcast<f32>(words[P.bcdf_off + kk - 1u]);
        }
        let pdf = max(bitcast<f32>(words[P.bcdf_off + kk]) - prev, 1e-8);
        let li = words[P.blights_off + kk];
        let base = P.light_off + li * 7u;
        let style = u32(table[base + 1u].w);
        let slot = slot_of(styles, ns, style);
        let j1 = rnd(rng);
        let j2 = rnd(rng);
        if (slot != NONE && (style == 0u || P.style_bounce != 0u)) {
            let c = light_contrib(li, pos, n, n, n, n, 1u, vec2<f32>(j1, j2), in_skybox);
            if (c.ok) {
                for (var ps = 0u; ps < P.passes; ps = ps + 1u) {
                    sample_acc[ps * 4u + slot] = sample_acc[ps * 4u + slot] + table[base + 2u + ps].xyz * c.f0 / pdf * scale;
                }
            }
        }
    }
}

fn indirect_sample(si: u32, luxel: u32, k: u32, pfirst: u32, pcount: u32) {
    let dims = surf_dims(si);
    let styles = surf_styles(si);
    let info = surf_info(si);
    let nn = dims.z;
    let ns = dims.w;
    if (P.bounces == 0u) {
        return;
    }
    let pi = min(u32(r1(k, P.seed ^ 0x9e37u, luxel, DIM_POINT) * f32(pcount)), pcount - 1u);
    let p = load_point(pfirst + pi);
    let uv = r2(k, P.seed, luxel, DIM_PATH);
    var dir: vec3<f32>;
    if (nn > 1u) {
        let lobe = min(u32(uv.y * 4.0), 3u);
        var ln = p.n0;
        if (lobe == 1u) {
            ln = p.n1;
        } else if (lobe == 2u) {
            ln = p.n2;
        } else if (lobe == 3u) {
            ln = p.n3;
        }
        dir = cosine_dir(ln, uv.x, uv.y * 4.0 - f32(lobe));
    } else {
        dir = cosine_dir(p.n0, uv.x, uv.y);
    }
    let c0 = dot(dir, p.n0);
    if (c0 <= 0.0) {
        return;
    }
    var w = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    if (nn > 1u) {
        let c1 = max(dot(dir, p.n1), 0.0);
        let c2 = max(dot(dir, p.n2), 0.0);
        let c3 = max(dot(dir, p.n3), 0.0);
        let sum = c0 + c1 + c2 + c3;
        if (sum <= 1e-6) {
            return;
        }
        w = vec4<f32>(c0, c1, c2, c3) * (4.0 / sum);
    }
    var rng = hash3(P.seed, luxel, (k * 0x2545f491u) ^ 0x51edu);
    var origin = p.pos;
    var d = dir;
    var thr = vec3<f32>(1.0);
    let in_skybox = info.w != 0u;
    for (var i = 0u; i < 8u; i = i + 1u) {
        sample_acc[i] = vec3<f32>(0.0);
    }
    for (var depth = 0u; depth < P.bounces; depth = depth + 1u) {
        let h = closest(origin, d, MAX_TRACE);
        if (!h.ok) {
            break;
        }
        thr = thr * transmit(origin, d, h.t);
        if (max(max(thr.x, thr.y), thr.z) <= 0.0) {
            break;
        }
        let a = attrs[h.id];
        let m = a.info.x;
        let kind = mat_kind(m);
        if (kind == K_SKY || kind == K_BLOCKER) {
            break;
        }
        let gn = a.normal.xyz;
        let front = dot(d, gn) < 0.0;
        let two_sided = mat_info(m).y != 0u;
        if (!front && !two_sided) {
            break;
        }
        var facing = gn;
        if (!front) {
            facing = -gn;
        }
        let hit = origin + d * h.t;
        let albedo = mat_albedo(m);
        let scale = thr * albedo;
        if (a.info.y != NONE) {
            let hs = a.info.y;
            let hdims = surf_dims(hs);
            let hstyles = surf_styles(hs);
            let bw = 1.0 - h.u - h.v;
            let ls = a.uv2lm0.z * bw + a.lm12.x * h.u + a.lm12.z * h.v;
            let lt = a.uv2lm0.w * bw + a.lm12.y * h.u + a.lm12.w * h.v;
            for (var hslot = 0u; hslot < hdims.w; hslot = hslot + 1u) {
                let style = hstyles[hslot];
                if (style != 0u && P.style_bounce == 0u) {
                    continue;
                }
                let slot = slot_of(styles, ns, style);
                if (slot == NONE) {
                    continue;
                }
                for (var ps = 0u; ps < P.passes; ps = ps + 1u) {
                    sample_acc[ps * 4u + slot] = sample_acc[ps * 4u + slot] + cache_lookup(hs, ls, lt, ps, hslot) * scale;
                }
            }
        } else {
            bounce_nee(hit + facing * BOUNCE_OFFSET, facing, &rng, in_skybox, styles, ns, scale);
            let sli = P.sky_light;
            if (sli != NONE) {
                let base = P.light_off + sli * 7u;
                let style = u32(table[base + 1u].w);
                let slot = slot_of(styles, ns, style);
                let u1 = rnd(&rng);
                let u2 = rnd(&rng);
                if (slot != NONE && (style == 0u || P.style_bounce != 0u)) {
                    let sd = cosine_dir(facing, u1, u2);
                    let vis = sky_visible(hit + facing * BOUNCE_OFFSET, sd, in_skybox);
                    for (var ps = 0u; ps < P.passes; ps = ps + 1u) {
                        sample_acc[ps * 4u + slot] = sample_acc[ps * 4u + slot] + table[base + 2u + ps].xyz * vis * scale;
                    }
                }
            }
        }
        thr = scale;
        if (depth >= 1u) {
            let q = min(max(max(thr.x, thr.y), thr.z), 0.95);
            if (q <= 0.0 || rnd(&rng) >= q) {
                break;
            }
            thr = thr / q;
        }
        origin = hit + facing * BOUNCE_OFFSET;
        let r1v = rnd(&rng);
        let r2v = rnd(&rng);
        d = cosine_dir(facing, r1v, r2v);
    }
    for (var ps = 0u; ps < P.passes; ps = ps + 1u) {
        for (var slot = 0u; slot < ns; slot = slot + 1u) {
            var c = sample_acc[ps * 4u + slot];
            let l = lum(c);
            if (P.clamp_lum > 0.0 && l > P.clamp_lum) {
                c = c * (P.clamp_lum / l);
            }
            for (var n = 0u; n < nn; n = n + 1u) {
                add_acc(ns, nn, ps, slot, n, c * w[n]);
            }
        }
    }
}

fn run_item(gid: u32, indirect: bool) {
    if (gid >= P.nitems) {
        return;
    }
    let ib = P.item_base + gid * 8u;
    let luxel = batch[ib];
    let si = batch[ib + 1u];
    let start = batch[ib + 2u];
    let count = batch[ib + 3u];
    let pfirst = batch[ib + 4u];
    let pcount = batch[ib + 5u];
    let out = batch[ib + 6u];
    let dims = surf_dims(si);
    let width = P.passes * dims.w * dims.z;
    for (var i = 0u; i < 32u; i = i + 1u) {
        acc[i] = vec3<f32>(0.0);
    }
    let probe = (P.passes - 1u) * dims.w * dims.z;
    var n = 0.0;
    var s = 0.0;
    var sq = 0.0;
    var ao = 0.0;
    if (pcount > 0u) {
        for (var k = start; k < start + count; k = k + 1u) {
            let before = acc[probe];
            var a = 1.0;
            if (indirect) {
                indirect_sample(si, luxel, k, pfirst, pcount);
            } else {
                a = direct_sample(si, luxel, k, pfirst, pcount);
            }
            let l = lum(acc[probe] - before);
            n = n + 1.0;
            s = s + l;
            sq = sq + l * l;
            ao = ao + a;
        }
    }
    for (var i = 0u; i < width; i = i + 1u) {
        outbuf[(out + i) * 3u] = acc[i].x;
        outbuf[(out + i) * 3u + 1u] = acc[i].y;
        outbuf[(out + i) * 3u + 2u] = acc[i].z;
    }
    let sb = P.stats_base + gid * 4u;
    outbuf[sb] = n;
    outbuf[sb + 1u] = s;
    outbuf[sb + 2u] = sq;
    outbuf[sb + 3u] = ao;
}

@compute @workgroup_size(64)
fn direct_main(@builtin(global_invocation_id) id: vec3<u32>) {
    run_item(id.x + id.y * 65535u * 64u, false);
}

@compute @workgroup_size(64)
fn indirect_main(@builtin(global_invocation_id) id: vec3<u32>) {
    run_item(id.x + id.y * 65535u * 64u, true);
}
