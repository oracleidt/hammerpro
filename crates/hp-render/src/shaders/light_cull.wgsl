struct CullParams {
    eye: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    forward: vec4<f32>,
    tan: vec4<f32>,
    dims: vec4<u32>,
    misc: vec4<u32>,
};

struct Cand {
    view: vec4<f32>,
    world: vec4<f32>,
    atten: vec4<f32>,
    info: vec4<u32>,
};

struct Bounds {
    x: vec2<f32>,
    y: vec2<f32>,
    z: vec2<f32>,
    len: vec4<f32>,
    lo: vec3<f32>,
    hi: vec3<f32>,
};

@group(0) @binding(0) var<uniform> p: CullParams;
@group(0) @binding(1) var<storage, read> cands: array<Cand>;
@group(0) @binding(2) var<storage, read> slices: array<u32>;
@group(0) @binding(3) var<storage, read_write> clusters: array<vec2<u32>>;
@group(0) @binding(4) var<storage, read_write> indices: array<u32>;
@group(0) @binding(5) var<storage, read_write> stats: array<atomic<u32>, 4>;
@group(0) @binding(6) var<storage, read_write> crowded: array<vec2<u32>>;

const BIG: f32 = 3.4028235e38;
const SHARED_MAX: u32 = 1024u;
const GROUP_X: u32 = 8u;
const GROUP_Y: u32 = 8u;
const COOP: u32 = 64u;
const PICK_MAX: u32 = 256u;

var<workgroup> shared_list: array<u32, 1024>;
var<workgroup> mask: array<atomic<u32>, 32>;
var<workgroup> word_base: array<u32, 32>;
var<workgroup> found_total: u32;
var<workgroup> hist: array<atomic<u32>, 256>;
var<workgroup> picked: array<u32, 256>;
var<workgroup> take_mask: array<atomic<u32>, 2>;
var<workgroup> co_total: u32;
var<workgroup> co_entry: vec2<u32>;
var<workgroup> co_prefix: u32;
var<workgroup> co_need: u32;
var<workgroup> co_count: u32;

fn edge(s: f32) -> f32 {
    return p.eye.w * exp(s / max(p.right.w, 1e-6));
}

fn tile_x(px: f32) -> f32 {
    return (px / p.up.w * 2.0 - 1.0) * p.tan.x;
}

fn tile_y(py: f32) -> f32 {
    return (1.0 - py / p.forward.w * 2.0) * p.tan.y;
}

fn bounds_of(x0: u32, x1: u32, y0: u32, y1: u32, s: u32) -> Bounds {
    let tile = p.tan.z;
    var b: Bounds;
    b.x = vec2<f32>(tile_x(f32(x0) * tile - 2.0), tile_x(f32(x1 + 1u) * tile + 2.0));
    b.y = vec2<f32>(tile_y(f32(y1 + 1u) * tile + 2.0), tile_y(f32(y0) * tile - 2.0));
    var z0 = 0.0;
    if s > 0u {
        z0 = edge(f32(s)) * 0.999;
    }
    var z1 = BIG;
    if s + 1u < p.dims.z {
        z1 = edge(f32(s + 1u)) * 1.001;
    }
    b.z = vec2<f32>(z0, z1);
    let v = vec4<f32>(b.x.x, b.x.y, b.y.x, b.y.y);
    b.len = sqrt(vec4<f32>(1.0) + v * v);
    let near = max(z0, p.eye.w * 0.999);
    let far = min(z1, p.tan.w);
    b.lo = vec3<f32>(min(b.x.x * near, b.x.x * far), min(b.y.x * near, b.y.x * far), near);
    b.hi = vec3<f32>(max(b.x.y * near, b.x.y * far), max(b.y.y * near, b.y.y * far), far);
    return b;
}

fn hits(b: Bounds, c: vec4<f32>) -> bool {
    let r = c.w + abs(c.w) * 1e-5 + 0.01;
    let gap = max(max(b.lo - c.xyz, c.xyz - b.hi), vec3<f32>(0.0));
    if b.hi.z >= b.lo.z && dot(gap, gap) > r * r * 1.0001 {
        return false;
    }
    return c.z + r >= b.z.x && c.z - r <= b.z.y
        && c.x - b.x.x * c.z >= -r * b.len.x
        && b.x.y * c.z - c.x >= -r * b.len.y
        && c.y - b.y.x * c.z >= -r * b.len.z
        && b.y.y * c.z - c.y >= -r * b.len.w;
}

fn center_of(x: u32, y: u32, s: u32) -> vec3<f32> {
    let tile = p.tan.z;
    let depth = p.eye.w * exp((f32(s) + 0.5) / max(p.right.w, 1e-6));
    let px = min((f32(x) + 0.5) * tile, p.up.w);
    let py = min((f32(y) + 0.5) * tile, p.forward.w);
    let nx = px / max(p.up.w, 1.0) * 2.0 - 1.0;
    let ny = 1.0 - py / max(p.forward.w, 1.0) * 2.0;
    return p.eye.xyz + p.forward.xyz * depth + p.right.xyz * (nx * p.tan.x * depth) + p.up.xyz * (ny * p.tan.y * depth);
}

fn score(c: Cand, center: vec3<f32>) -> f32 {
    let d = max(distance(c.world.xyz, center), 1.0);
    let dc = min(d, max(c.atten.w, 1.0));
    let denom = c.atten.x + dc * (c.atten.y + dc * c.atten.z);
    var falloff = 1.0;
    if denom > 1e-12 {
        falloff = 1.0 / denom;
    }
    let x = d / max(c.world.w, 1e-3);
    let window = clamp(1.0 - x * x * x * x, 0.0, 1.0);
    let s = bitcast<f32>(c.info.y) * falloff * window * window;
    if !(s >= 0.0 && s <= BIG) {
        return 0.0;
    }
    return s;
}

fn put(at: u32, v: u32) {
    let word = at >> 1u;
    if (at & 1u) == 0u {
        indices[word] = v & 0xffffu;
    } else {
        indices[word] = indices[word] | ((v & 0xffffu) << 16u);
    }
}

fn cand_at(k: u32, use_shared: bool, first: u32) -> u32 {
    if use_shared {
        return shared_list[k];
    }
    return slices[first + k];
}

@compute @workgroup_size(8, 8, 1)
fn cs_cull(
    @builtin(workgroup_id) wg: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
) {
    let s = wg.z;
    let gx0 = wg.x * GROUP_X;
    let gy0 = wg.y * GROUP_Y;
    let gx1 = min(gx0 + GROUP_X, p.dims.x) - 1u;
    let gy1 = min(gy0 + GROUP_Y, p.dims.y) - 1u;
    if li == 0u {
        found_total = 0u;
    }
    workgroupBarrier();
    let first = slices[s];
    let n = slices[s + 1u] - first;
    let group_bounds = bounds_of(gx0, gx1, gy0, gy1, s);
    let chunks = (n + SHARED_MAX - 1u) / SHARED_MAX;
    for (var ch = 0u; ch < chunks; ch++) {
        if li < 32u {
            atomicStore(&mask[li], 0u);
        }
        workgroupBarrier();
        let start = ch * SHARED_MAX;
        for (var k = li; k < SHARED_MAX; k += GROUP_X * GROUP_Y) {
            let pos = start + k;
            if pos < n && hits(group_bounds, cands[slices[first + pos]].view) {
                atomicOr(&mask[k >> 5u], 1u << (k & 31u));
            }
        }
        workgroupBarrier();
        if li == 0u {
            var acc = found_total;
            for (var w = 0u; w < 32u; w++) {
                word_base[w] = acc;
                acc += countOneBits(atomicLoad(&mask[w]));
            }
            found_total = acc;
        }
        workgroupBarrier();
        if li < 32u {
            var bits = atomicLoad(&mask[li]);
            var at = word_base[li];
            loop {
                if bits == 0u {
                    break;
                }
                let bit = firstTrailingBit(bits);
                bits = bits & (bits - 1u);
                if at < SHARED_MAX {
                    shared_list[at] = slices[first + start + li * 32u + bit];
                }
                at++;
            }
        }
        workgroupBarrier();
    }
    let found = workgroupUniformLoad(&found_total);
    let use_shared = found <= SHARED_MAX;
    var m = n;
    if use_shared {
        m = found;
    }
    let x = gx0 + lid.x;
    let y = gy0 + lid.y;
    if x >= p.dims.x || y >= p.dims.y {
        return;
    }
    let ci = (s * p.dims.y + y) * p.dims.x + x;
    let b = bounds_of(x, x, y, y, s);
    let cap = p.misc.x;
    let base = ci * cap;
    var count = 0u;
    for (var k = 0u; k < m; k++) {
        if hits(b, cands[cand_at(k, use_shared, first)].view) {
            count++;
        }
    }
    if count <= cap {
        var w = 0u;
        for (var k = 0u; k < m; k++) {
            let c = cand_at(k, use_shared, first);
            if hits(b, cands[c].view) {
                put(base + w, cands[c].info.x);
                w++;
            }
        }
        clusters[ci] = vec2<u32>(base, count);
        return;
    }
    clusters[ci] = vec2<u32>(base, 0u);
    let slot = atomicAdd(&stats[0], 1u);
    if slot < arrayLength(&crowded) {
        crowded[slot] = vec2<u32>(ci, u32(use_shared));
    }
}

fn member(c: u32, use_group: bool, gb: Bounds, b: Bounds) -> bool {
    let v = cands[c].view;
    return (!use_group || hits(gb, v)) && hits(b, v);
}

@compute @workgroup_size(64, 1, 1)
fn cs_crowded(
    @builtin(workgroup_id) wg: vec3<u32>,
    @builtin(num_workgroups) nwg: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
) {
    if li == 0u {
        co_total = min(atomicLoad(&stats[0]), arrayLength(&crowded));
    }
    let total = workgroupUniformLoad(&co_total);
    let cap = min(p.misc.x, PICK_MAX);
    let plane = p.dims.x * p.dims.y;
    for (var slot = wg.x; slot < total; slot += nwg.x) {
        if li == 0u {
            co_entry = crowded[slot];
            co_prefix = 0u;
            co_need = cap;
            co_count = 0u;
        }
        let entry = workgroupUniformLoad(&co_entry);
        let ci = entry.x;
        let use_group = entry.y != 0u;
        let s = ci / plane;
        let rest = ci % plane;
        let y = rest / p.dims.x;
        let x = rest % p.dims.x;
        let gx0 = (x / GROUP_X) * GROUP_X;
        let gy0 = (y / GROUP_Y) * GROUP_Y;
        let gx1 = min(gx0 + GROUP_X, p.dims.x) - 1u;
        let gy1 = min(gy0 + GROUP_Y, p.dims.y) - 1u;
        let gb = bounds_of(gx0, gx1, gy0, gy1, s);
        let b = bounds_of(x, x, y, y, s);
        let center = center_of(x, y, s);
        let first = slices[s];
        let n = slices[s + 1u] - first;
        var prefix = 0u;
        for (var shift = 24u; ; shift -= 8u) {
            for (var h = li; h < 256u; h += COOP) {
                atomicStore(&hist[h], 0u);
            }
            workgroupBarrier();
            let high = select(0xffffffffu << (shift + 8u), 0u, shift == 24u);
            for (var k = li; k < n; k += COOP) {
                let c = slices[first + k];
                if member(c, use_group, gb, b) {
                    let sc = score(cands[c], center);
                    let v = select(bitcast<u32>(sc), 0u, sc <= 0.0);
                    if (v & high) == prefix {
                        atomicAdd(&hist[(v >> shift) & 255u], 1u);
                    }
                }
            }
            workgroupBarrier();
            if li == 0u {
                var need = co_need;
                var digit = 0u;
                for (var d = 255i; d >= 0; d--) {
                    let count_d = atomicLoad(&hist[u32(d)]);
                    if count_d >= need {
                        digit = u32(d);
                        break;
                    }
                    need -= count_d;
                }
                co_need = need;
                co_prefix = co_prefix | (digit << shift);
            }
            prefix = workgroupUniformLoad(&co_prefix);
            if shift == 0u {
                break;
            }
        }
        let t = bitcast<f32>(prefix);
        let chunks = (n + COOP - 1u) / COOP;
        for (var equal = 0u; equal < 2u; equal++) {
            for (var ch = 0u; ch < chunks; ch++) {
                if workgroupUniformLoad(&co_count) >= cap {
                    break;
                }
                if li < 2u {
                    atomicStore(&take_mask[li], 0u);
                }
                workgroupBarrier();
                let k = ch * COOP + li;
                var take = false;
                var light = 0u;
                if k < n {
                    let c = slices[first + k];
                    if member(c, use_group, gb, b) {
                        let sc = score(cands[c], center);
                        take = select(sc > t, sc == t, equal == 1u);
                        light = cands[c].info.x;
                    }
                }
                if take {
                    atomicOr(&take_mask[li >> 5u], 1u << (li & 31u));
                }
                workgroupBarrier();
                let lo = atomicLoad(&take_mask[0]);
                let hi = atomicLoad(&take_mask[1]);
                var before = 0u;
                if li < 32u {
                    before = countOneBits(lo & ((1u << li) - 1u));
                } else {
                    before = countOneBits(lo) + countOneBits(hi & ((1u << (li - 32u)) - 1u));
                }
                let at = co_count + before;
                if take && at < cap {
                    picked[at] = light;
                }
                workgroupBarrier();
                if li == 0u {
                    co_count = min(co_count + countOneBits(lo) + countOneBits(hi), cap);
                }
                workgroupBarrier();
            }
        }
        let w = workgroupUniformLoad(&co_count);
        let base = ci * p.misc.x;
        for (var j = li; j * 2u < w; j += COOP) {
            var word = picked[j * 2u] & 0xffffu;
            if j * 2u + 1u < w {
                word = word | ((picked[j * 2u + 1u] & 0xffffu) << 16u);
            }
            indices[(base >> 1u) + j] = word;
        }
        if li == 0u {
            clusters[ci] = vec2<u32>(base, w);
        }
        workgroupBarrier();
    }
}
