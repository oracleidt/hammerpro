override FILTER: u32 = 0u;
override BOX_FILTER: u32 = 0u;
override COLLISION: bool = false;

const COLLISION_RGB: vec3<f32> = vec3<f32>(0.75, 0.9, 0.1);
override NO_COLLISION: bool = false;
const NO_COLLISION_RGB: vec3<f32> = vec3<f32>(1.0, 0.42, 0.04);
override DEPTH_BIAS: f32 = 0.0;

struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

var<private> BOX_EDGES: array<u32, 24> = array<u32, 24>(
    0u, 1u, 1u, 3u, 3u, 2u, 2u, 0u,
    4u, 5u, 5u, 7u, 7u, 6u, 6u, 4u,
    0u, 4u, 1u, 5u, 2u, 6u, 3u, 7u,
);

var<private> QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(1.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, 1.0),
);

fn filtered(flags: u32) -> bool {
    if hidden_in_view(flags) {
        return true;
    }
    let sel = (flags & F_SELECTED) != 0u;
    if (FILTER == 1u || FILTER == 3u) && sel {
        return true;
    }
    if FILTER == 2u && !sel {
        return true;
    }
    if FILTER == 3u && (flags & F_ALWAYS_EDGES) == 0u {
        return true;
    }
    if FILTER == 4u && (flags & F_INVALID) == 0u {
        return true;
    }
    if (flags & F_ROPE) != 0u && !is_ortho() && view.misc.x != MODE_WIREFRAME && FILTER != 2u {
        return true;
    }
    return false;
}

const ON_LIGHT_LUMA: f32 = 0.11;
const ON_LIGHT_CHROMA: f32 = 1.35;
const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

fn readable(rgb: vec3<f32>) -> vec3<f32> {
    if (view.opts.x & OPT_LIGHT_BACKGROUND) == 0u {
        return rgb;
    }
    let luma = dot(rgb, LUMA);
    var c = rgb;
    if luma > ON_LIGHT_LUMA {
        c = rgb * (ON_LIGHT_LUMA / luma);
    }
    let grey = vec3<f32>(dot(c, LUMA));
    return clamp(mix(grey, c, ON_LIGHT_CHROMA), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn edge_color(st: ObjState) -> vec4<f32> {
    var c = unpack_srgba(state_color(st));
    c = vec4<f32>(readable(c.rgb), c.a);
    if (st.flags & F_INSTANCE) != 0u {
        c = vec4<f32>(mix(c.rgb, INSTANCE_RGB, 0.5), c.a);
    }
    if (st.flags & F_DIMMED) != 0u {
        c = vec4<f32>(c.rgb * 0.15, c.a);
    }
    if (st.flags & F_SELECTED) != 0u {
        c = vec4<f32>(view.selection.rgb, 1.0);
    }
    if FILTER == 4u {
        c = vec4<f32>(INVALID_RGB, 1.0);
    }
    if (st.flags & F_HOVERED) != 0u {
        let light = (view.opts.x & OPT_LIGHT_BACKGROUND) != 0u;
        c = vec4<f32>(mix(c.rgb, select(vec3<f32>(1.0), vec3<f32>(0.0), light), 0.45), c.a);
    }
    return c;
}

fn biased(clip: vec4<f32>) -> vec4<f32> {
    let z = clip.z * (1.0 + DEPTH_BIAS);
    return vec4<f32>(clip.xy, select(z, min(z, clip.w), clip.w > 0.0), clip.w);
}

fn detail_hidden(flags: u32) -> bool {
    if !is_ortho() {
        return false;
    }
    if (view.opts.x & OPT_DETAIL_NONE) != 0u {
        return true;
    }
    return (view.opts.x & OPT_DETAIL_SELECTED) != 0u && (flags & F_SELECTED) == 0u;
}

fn source_hidden() -> bool {
    return !is_ortho() || (view.opts.x & OPT_SOURCE_2D) == 0u;
}

@vertex
fn vs_edge(@location(0) pos: vec3<f32>, @location(1) bits: u32) -> LineOut {
    let st = object_state(bits & EDGE_SLOT_MASK);
    var out: LineOut;
    let detail = (bits & EDGE_DETAIL_BIT) != 0u;
    let source = (bits & EDGE_SOURCE_BIT) != 0u;
    if filtered(st.flags) || (detail && detail_hidden(st.flags)) || (source && source_hidden()) {
        out.clip = COLLAPSED;
        out.color = vec4<f32>(0.0);
        return out;
    }
    out.clip = biased(view.view_proj * vec4<f32>(pos, 1.0));
    out.color = edge_color(st);
    if source {
        out.color = vec4<f32>(out.color.rgb * 0.5, out.color.a * 0.5);
    }
    return out;
}

@vertex
fn vs_box(
    @builtin(vertex_index) vi: u32,
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    @location(9) bmin: vec3<f32>,
    @location(10) slot: u32,
    @location(11) bmax: vec3<f32>,
) -> LineOut {
    let st = object_state(slot);
    var out: LineOut;
    let origin = m3.xyz;
    let far = view.sky.w > 0.0 && !is_ortho() && distance(origin, view.eye.xyz) > view.sky.w;
    if filtered(st.flags) || (BOX_FILTER == 1u && !far) {
        out.clip = COLLAPSED;
        out.color = vec4<f32>(0.0);
        return out;
    }
    let corner = BOX_EDGES[vi % 24u];
    let lp = vec3<f32>(
        select(bmin.x, bmax.x, (corner & 1u) != 0u),
        select(bmin.y, bmax.y, (corner & 2u) != 0u),
        select(bmin.z, bmax.z, (corner & 4u) != 0u),
    );
    let m = mat4x4<f32>(m0, m1, m2, m3);
    out.clip = biased(view.view_proj * (m * vec4<f32>(lp, 1.0)));
    if NO_COLLISION {
        out.color = vec4<f32>(NO_COLLISION_RGB, 1.0);
    } else {
        out.color = edge_color(st);
    }
    return out;
}

@vertex
fn vs_color(@location(0) pos: vec3<f32>, @location(1) color: vec4<f32>) -> LineOut {
    var out: LineOut;
    out.clip = biased(view.view_proj * vec4<f32>(pos, 1.0));
    out.color = vec4<f32>(srgb_to_linear(color.rgb), color.a);
    return out;
}

@vertex
fn vs_point(
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
) -> LineOut {
    var out: LineOut;
    let clip = view.view_proj * vec4<f32>(pos, 1.0);
    if clip.w <= 1e-6 {
        out.clip = COLLAPSED;
        out.color = vec4<f32>(0.0);
        return out;
    }
    let corner = QUAD[vi % 6u];
    let offset = corner * max(size, 1.0) * view.viewport.zw;
    out.clip = vec4<f32>(clip.xy + offset * clip.w, clip.z, clip.w);
    out.color = vec4<f32>(srgb_to_linear(color.rgb), color.a);
    return out;
}

@vertex
fn vs_model_edge(
    @location(0) pos: vec3<f32>,
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    @location(10) slot: u32,
) -> LineOut {
    let st = object_state(slot);
    var out: LineOut;
    if filtered(st.flags) {
        out.clip = COLLAPSED;
        out.color = vec4<f32>(0.0);
        return out;
    }
    let m = mat4x4<f32>(m0, m1, m2, m3);
    out.clip = biased(view.view_proj * (m * vec4<f32>(pos, 1.0)));
    if COLLISION {
        out.color = vec4<f32>(COLLISION_RGB, 0.9);
    } else {
        out.color = edge_color(st);
    }
    return out;
}

@vertex
fn vs_collision_hull(
    @location(0) pos: vec3<f32>,
    @location(2) nrm_alpha: u32,
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    @location(10) slot: u32,
) -> LineOut {
    let st = object_state(slot);
    var out: LineOut;
    if filtered(st.flags) {
        out.clip = COLLAPSED;
        out.color = vec4<f32>(0.0);
        return out;
    }
    let m = mat4x4<f32>(m0, m1, m2, m3);
    out.clip = view.view_proj * (m * vec4<f32>(pos, 1.0));
    let n = normalize((m * vec4<f32>(unpack4x8snorm(nrm_alpha).xyz, 0.0)).xyz + vec3<f32>(1e-6));
    let light = normalize(vec3<f32>(0.35, 0.55, 0.76));
    let shade = 0.4 + 0.6 * abs(dot(n, light));
    var rgb = COLLISION_RGB * shade;
    if (st.flags & F_SELECTED) != 0u {
        rgb = mix(rgb, vec3<f32>(1.0, 0.3, 0.3), 0.5);
    }
    out.color = vec4<f32>(rgb, 0.5);
    return out;
}

@vertex
fn vs_dot(@builtin(vertex_index) vi: u32, @location(0) pos: vec3<f32>, @location(1) bits: u32) -> LineOut {
    let st = object_state(bits & EDGE_SLOT_MASK);
    var out: LineOut;
    let detail = (bits & EDGE_DETAIL_BIT) != 0u;
    let source = (bits & EDGE_SOURCE_BIT) != 0u;
    if filtered(st.flags) || (detail && detail_hidden(st.flags)) || source {
        out.clip = COLLAPSED;
        out.color = vec4<f32>(0.0);
        return out;
    }
    let clip = view.view_proj * vec4<f32>(pos, 1.0);
    let corner = QUAD[vi % 6u];
    let offset = corner * max(f32(view.opts.z), 1.0) * 0.5 * view.viewport.zw * 2.0;
    out.clip = vec4<f32>(clip.xy + offset * clip.w, clip.z, clip.w);
    out.color = edge_color(st);
    return out;
}

var<private> WALK_RGB: array<vec3<f32>, 8> = array<vec3<f32>, 8>(
    vec3<f32>(60.0, 200.0, 90.0),
    vec3<f32>(250.0, 190.0, 40.0),
    vec3<f32>(235.0, 55.0, 70.0),
    vec3<f32>(150.0, 210.0, 60.0),
    vec3<f32>(235.0, 225.0, 50.0),
    vec3<f32>(245.0, 140.0, 45.0),
    vec3<f32>(240.0, 95.0, 55.0),
    vec3<f32>(150.0, 25.0, 110.0),
);

@vertex
fn vs_mark(@location(0) pos: vec3<f32>, @location(1) bits: u32) -> LineOut {
    let st = object_state(bits & EDGE_SLOT_MASK);
    var out: LineOut;
    if (view.opts.x & OPT_WALK_MARKS) == 0u || filtered(st.flags) || detail_hidden(st.flags) {
        out.clip = COLLAPSED;
        out.color = vec4<f32>(0.0);
        return out;
    }
    let walk = (bits >> EDGE_CLASS_SHIFT) & 7u;
    out.clip = biased(view.view_proj * vec4<f32>(pos, 1.0));
    let a = select(0.5, 0.3, is_ortho());
    out.color = vec4<f32>(srgb_to_linear(WALK_RGB[walk] / 255.0), a);
    return out;
}

struct WideLine {
    params: vec4<f32>,
};

@group(1) @binding(0) var<uniform> wide: WideLine;

@vertex
fn vs_wide(
    @builtin(vertex_index) vi: u32,
    @location(0) a: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) b: vec3<f32>,
) -> LineOut {
    var out: LineOut;
    out.color = vec4<f32>(srgb_to_linear(color.rgb), color.a);
    var ca = view.view_proj * vec4<f32>(a, 1.0);
    var cb = view.view_proj * vec4<f32>(b, 1.0);
    let eps = 1e-4;
    if ca.w < eps && cb.w < eps {
        out.clip = COLLAPSED;
        return out;
    }
    if ca.w < eps {
        ca = mix(ca, cb, (eps - ca.w) / (cb.w - ca.w));
    } else if cb.w < eps {
        cb = mix(cb, ca, (eps - cb.w) / (ca.w - cb.w));
    }
    let px = view.viewport.xy * 0.5;
    let sa = ca.xy / ca.w * px;
    let sb = cb.xy / cb.w * px;
    var dir = sb - sa;
    let len = length(dir);
    if len < 1e-4 {
        dir = vec2<f32>(1.0, 0.0);
    } else {
        dir = dir / len;
    }
    let normal = vec2<f32>(-dir.y, dir.x);
    let corner = QUAD[vi % 6u];
    let half = max(wide.params.x, 1.0) * 0.5;
    var c = select(ca, cb, corner.x > 0.0);
    let offset = (normal * corner.y + dir * corner.x) * half / px;
    c = vec4<f32>(c.xy + offset * c.w, c.z, c.w);
    out.clip = biased(c);
    return out;
}

@fragment
fn fs_color(in: LineOut) -> @location(0) vec4<f32> {
    return in.color;
}
