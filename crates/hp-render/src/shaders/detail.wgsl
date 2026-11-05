override DETAIL_DIST: f32 = 1200.0;
override DETAIL_FADE: f32 = 400.0;

override WIRE_RGB: u32 = 0u;
override LINE_LIST: bool = false;

struct DetailOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
    @location(2) color: vec4<f32>,
    @location(3) world: vec3<f32>,
};

var<private> TRIS: array<u32, 6> = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
var<private> EDGES: array<u32, 10> = array<u32, 10>(0u, 1u, 1u, 2u, 2u, 3u, 3u, 0u, 0u, 2u);

@vertex
fn vs_detail(
    @builtin(vertex_index) vi: u32,
    @location(0) origin: vec3<f32>,
    @location(1) orient: u32,
    @location(2) right: vec3<f32>,
    @location(3) slot: u32,
    @location(4) up: vec3<f32>,
    @location(5) color: vec4<f32>,
    @location(6) rect: vec4<f32>,
    @location(7) tex_rect: vec4<f32>,
) -> DetailOut {
    var out: DetailOut;
    let st = object_state(slot);
    let ofs = origin - view.eye.xyz;
    let dist2 = dot(ofs, ofs);
    let max2 = DETAIL_DIST * DETAIL_DIST;
    let start = max(DETAIL_DIST - DETAIL_FADE, 0.0);
    let fade2 = min(start * start, max2 - 1.0);
    if hidden_in_view(st.flags) || is_ortho() || dist2 >= max2 {
        out.clip = COLLAPSED;
        return out;
    }
    out.alpha = 1.0 - clamp((dist2 - fade2) / max(max2 - fade2, 1e-3), 0.0, 1.0);
    out.color = vec4<f32>(srgb_to_linear(color.rgb), color.a);
    var dx = right;
    var dy = up;
    let mode = orient & 255u;
    let sway = f32(orient >> 8u) / 255.0;
    if mode == 2u {
        let h = vec2<f32>(-ofs.y, ofs.x);
        let len = max(length(h), 1e-4);
        dx = vec3<f32>(h / len, 0.0);
        dy = vec3<f32>(0.0, 0.0, 1.0);
    } else if mode == 1u {
        dx = view.right.xyz;
        dy = view.up.xyz;
    }
    var corner: u32;
    if LINE_LIST {
        corner = EDGES[vi % 10u];
    } else {
        corner = TRIS[vi % 6u];
    }
    let ul = rect.xy;
    let lr = rect.zw;
    var p = origin + dx * ul.x + dy * ul.y;
    var uv = tex_rect.xy;
    if corner == 1u || corner == 2u {
        p += dy * (lr.y - ul.y);
        uv.y = tex_rect.w;
    }
    if corner == 2u || corner == 3u {
        p += dx * (lr.x - ul.x);
        uv.x = tex_rect.z;
    }
    if sway > 0.0 && (corner == 0u || corner == 3u) {
        let phase = origin.x * 0.043 + origin.y * 0.061;
        let t = view.sky.z;
        let wave = sin(t * 1.9 + phase) * 0.7 + sin(t * 3.1 + phase * 1.7) * 0.3;
        p += dx * (wave * sway * 0.3 * (ul.y - lr.y));
    }
    out.uv = uv;
    out.world = p;
    out.clip = view.view_proj * vec4<f32>(p, 1.0);
    return out;
}

@fragment
fn fs_detail_wire() -> @location(0) vec4<f32> {
    let c = vec3<f32>(f32(WIRE_RGB & 255u), f32((WIRE_RGB >> 8u) & 255u), f32((WIRE_RGB >> 16u) & 255u)) / 255.0;
    return vec4<f32>(srgb_to_linear(c), 1.0);
}

@fragment
fn fs_detail(in: DetailOut) -> @location(0) vec4<f32> {
    let size = 1.0 / max(material.inv_tex_size, vec2<f32>(1e-6));
    var uv = in.uv;
    if size.x > size.y {
        uv.y *= size.x / size.y;
    }
    let t = textureSample(tex, samp, uv) * material.tint * in.color;
    let a = t.a * in.alpha;
    if a < 0.02 {
        discard;
    }
    return vec4<f32>(apply_fog(t.rgb, in.world), a);
}
