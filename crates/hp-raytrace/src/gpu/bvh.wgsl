fn intersect(tr: Tri, o: vec3<f32>, d: vec3<f32>) -> vec4<f32> {
    let p = cross(d, tr.e2);
    let det = dot(tr.e1, p);
    if (abs(det) < 1e-12) {
        return vec4<f32>(-1.0, 0.0, 0.0, 0.0);
    }
    let inv = 1.0 / det;
    let s = o - tr.v0;
    let u = dot(s, p) * inv;
    if (u < -1e-6 || u > 1.000001) {
        return vec4<f32>(-1.0, 0.0, 0.0, 0.0);
    }
    let q = cross(s, tr.e1);
    let v = dot(d, q) * inv;
    if (v < -1e-6 || u + v > 1.000001) {
        return vec4<f32>(-1.0, 0.0, 0.0, 0.0);
    }
    return vec4<f32>(dot(tr.e2, q) * inv, u, v, det);
}

fn slab(n: Node, o: vec3<f32>, inv: vec3<f32>, tmax: f32) -> f32 {
    let t0 = (n.mn - o) * inv;
    let t1 = (n.mx - o) * inv;
    let lo = min(t0, t1);
    let hi = max(t0, t1);
    let near = max(max(max(lo.x, lo.y), lo.z), 0.0);
    let far = min(min(min(hi.x, hi.y), hi.z), tmax);
    return select(1e30, near, near <= far);
}

fn safe_inv(d: vec3<f32>) -> vec3<f32> {
    let e = 1e-20;
    let x = select(d.x, select(-e, e, d.x >= 0.0), abs(d.x) < e);
    let y = select(d.y, select(-e, e, d.y >= 0.0), abs(d.y) < e);
    let z = select(d.z, select(-e, e, d.z >= 0.0), abs(d.z) < e);
    return vec3<f32>(1.0 / x, 1.0 / y, 1.0 / z);
}

fn closest(o: vec3<f32>, d: vec3<f32>, tmax: f32) -> Hit {
    var best: Hit;
    best.ok = false;
    best.t = tmax;
    var best_p = 0u;
    let inv = safe_inv(d);
    var stack: array<u32, 64>;
    var sp = 0u;
    if (slab(nodes[0], o, inv, tmax) >= 1e29) {
        return best;
    }
    stack[0] = 0u;
    sp = 1u;
    loop {
        if (sp == 0u) {
            break;
        }
        sp = sp - 1u;
        let n = nodes[stack[sp]];
        if ((n.b & LEAF) != 0u) {
            let cnt = n.b & ~LEAF;
            for (var k = n.a; k < n.a + cnt; k = k + 1u) {
                let tr = tris[k];
                let h = intersect(tr, o, d);
                if (h.x <= 1e-3 || h.x > best.t + TIE) {
                    continue;
                }
                let p = priority(tr.id);
                var better = false;
                if (!best.ok) {
                    better = h.x <= tmax;
                } else {
                    better = h.x < best.t - TIE || (p > best_p && h.x <= best.t + TIE) || (p == best_p && h.x < best.t);
                }
                if (better && solid(tr.id, h.y, h.z)) {
                    best.ok = true;
                    best.t = h.x;
                    best.u = h.y;
                    best.v = h.z;
                    best.id = tr.id;
                    best.front = h.w < 0.0;
                    best_p = p;
                }
            }
            continue;
        }
        let limit = best.t + TIE;
        let tl = slab(nodes[n.a], o, inv, limit);
        let trr = slab(nodes[n.b], o, inv, limit);
        var first = n.a;
        var second = n.b;
        var tf = tl;
        var ts = trr;
        if (trr < tl) {
            first = n.b;
            second = n.a;
            tf = trr;
            ts = tl;
        }
        if (ts < 1e29 && sp < 64u) {
            stack[sp] = second;
            sp = sp + 1u;
        }
        if (tf < 1e29 && sp < 64u) {
            stack[sp] = first;
            sp = sp + 1u;
        }
    }
    return best;
}

fn any_opaque(o: vec3<f32>, d: vec3<f32>, tmin: f32, tmax: f32) -> bool {
    let inv = safe_inv(d);
    var stack: array<u32, 64>;
    var sp = 0u;
    if (slab(nodes[0], o, inv, tmax) >= 1e29) {
        return false;
    }
    stack[0] = 0u;
    sp = 1u;
    loop {
        if (sp == 0u) {
            break;
        }
        sp = sp - 1u;
        let n = nodes[stack[sp]];
        if ((n.b & LEAF) != 0u) {
            let cnt = n.b & ~LEAF;
            for (var k = n.a; k < n.a + cnt; k = k + 1u) {
                let tr = tris[k];
                let h = intersect(tr, o, d);
                if (h.x > tmin && h.x < tmax && solid(tr.id, h.y, h.z)) {
                    return true;
                }
            }
            continue;
        }
        if (slab(nodes[n.a], o, inv, tmax) < 1e29 && sp < 64u) {
            stack[sp] = n.a;
            sp = sp + 1u;
        }
        if (slab(nodes[n.b], o, inv, tmax) < 1e29 && sp < 64u) {
            stack[sp] = n.b;
            sp = sp + 1u;
        }
    }
    return false;
}

fn transmit(o: vec3<f32>, d: vec3<f32>, tmax: f32) -> vec3<f32> {
    if (P.has_clear == 0u) {
        return vec3<f32>(1.0);
    }
    var t = vec3<f32>(1.0);
    let inv = safe_inv(d);
    var stack: array<u32, 64>;
    var sp = 0u;
    let root = P.clear_root;
    if (slab(nodes[root], o, inv, tmax) >= 1e29) {
        return t;
    }
    stack[0] = root;
    sp = 1u;
    loop {
        if (sp == 0u) {
            break;
        }
        sp = sp - 1u;
        let n = nodes[stack[sp]];
        if ((n.b & LEAF) != 0u) {
            let cnt = n.b & ~LEAF;
            for (var k = n.a; k < n.a + cnt; k = k + 1u) {
                let tr = tris[P.clear_tri + k];
                let h = intersect(tr, o, d);
                if (h.x > 1e-3 && h.x < tmax) {
                    let m = attrs[tr.id].info.x;
                    let a = alpha_at(tr.id, h.y, h.z);
                    let f = vec3<f32>(1.0) - (vec3<f32>(1.0) - mat_transmit(m).xyz) * a;
                    t = t * f;
                    if (max(max(t.x, t.y), t.z) < 1e-3) {
                        return vec3<f32>(0.0);
                    }
                }
            }
            continue;
        }
        if (slab(nodes[root + n.a], o, inv, tmax) < 1e29 && sp < 64u) {
            stack[sp] = root + n.a;
            sp = sp + 1u;
        }
        if (slab(nodes[root + n.b], o, inv, tmax) < 1e29 && sp < 64u) {
            stack[sp] = root + n.b;
            sp = sp + 1u;
        }
    }
    return t;
}

