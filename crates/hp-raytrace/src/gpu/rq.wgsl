@group(0) @binding(9) var tlas: acceleration_structure;

const MASK_SURFACES: u32 = 1u;
const MASK_BLOCKERS: u32 = 2u;
const MASK_CLEAR: u32 = 4u;

fn rq_id(c: RayIntersection) -> u32 {
    return words[P.rq_ids + c.instance_custom_data + c.primitive_index];
}

fn rq_closest_mask(o: vec3<f32>, d: vec3<f32>, tmax: f32, mask: u32) -> Hit {
    var best: Hit;
    best.ok = false;
    best.t = tmax;
    if (tmax <= 1e-3) {
        return best;
    }
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_NONE, mask, 1e-3, tmax, o, d));
    loop {
        if (!rayQueryProceed(&rq)) {
            break;
        }
        let c = rayQueryGetCandidateIntersection(&rq);
        let id = rq_id(c);
        if (solid(id, c.barycentrics.x, c.barycentrics.y)) {
            rayQueryConfirmIntersection(&rq);
        }
    }
    let h = rayQueryGetCommittedIntersection(&rq);
    if (h.kind == RAY_QUERY_INTERSECTION_NONE) {
        return best;
    }
    best.ok = true;
    best.t = h.t;
    best.u = h.barycentrics.x;
    best.v = h.barycentrics.y;
    best.id = rq_id(h);
    best.front = h.front_face;
    return best;
}

fn closest(o: vec3<f32>, d: vec3<f32>, tmax: f32) -> Hit {
    let h = rq_closest_mask(o, d, tmax, MASK_SURFACES);
    var limit = tmax;
    if (h.ok) {
        limit = h.t - TIE;
    }
    let b = rq_closest_mask(o, d, limit, MASK_BLOCKERS);
    if (b.ok) {
        return b;
    }
    return h;
}

fn any_opaque(o: vec3<f32>, d: vec3<f32>, tmin: f32, tmax: f32) -> bool {
    if (tmax <= tmin) {
        return false;
    }
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_TERMINATE_ON_FIRST_HIT, MASK_SURFACES | MASK_BLOCKERS, tmin, tmax, o, d));
    loop {
        if (!rayQueryProceed(&rq)) {
            break;
        }
        let c = rayQueryGetCandidateIntersection(&rq);
        if (solid(rq_id(c), c.barycentrics.x, c.barycentrics.y)) {
            rayQueryConfirmIntersection(&rq);
        }
    }
    let h = rayQueryGetCommittedIntersection(&rq);
    return h.kind != RAY_QUERY_INTERSECTION_NONE;
}

fn transmit(o: vec3<f32>, d: vec3<f32>, tmax: f32) -> vec3<f32> {
    if (P.has_clear == 0u || tmax <= 1e-3) {
        return vec3<f32>(1.0);
    }
    var t = vec3<f32>(1.0);
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_NONE, MASK_CLEAR, 1e-3, tmax, o, d));
    loop {
        if (!rayQueryProceed(&rq)) {
            break;
        }
        let c = rayQueryGetCandidateIntersection(&rq);
        let id = rq_id(c);
        let m = attrs[id].info.x;
        let a = alpha_at(id, c.barycentrics.x, c.barycentrics.y);
        t = t * (vec3<f32>(1.0) - (vec3<f32>(1.0) - mat_transmit(m).xyz) * a);
        if (max(max(t.x, t.y), t.z) < 1e-3) {
            rayQueryTerminate(&rq);
            return vec3<f32>(0.0);
        }
    }
    return t;
}
