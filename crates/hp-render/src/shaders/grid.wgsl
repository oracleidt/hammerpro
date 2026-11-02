@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.5, 1.0);
}

fn hits_ascending(a: f32, b: f32, step: f32) -> bool {
    return ceil(a / step) * step < b;
}

fn hits_descending(a: f32, b: f32, step: f32) -> bool {
    return floor(b / step) * step > a;
}

const GRID_MINOR: vec3<f32> = vec3<f32>(0.26, 0.26, 0.26);
const GRID_MAJOR: vec3<f32> = vec3<f32>(0.42, 0.42, 0.42);
const GRID_1024: vec3<f32> = vec3<f32>(0.62, 0.36, 0.12);
const GRID_AXIS: vec3<f32> = vec3<f32>(0.0, 0.55, 0.62);
const GRID_64: vec3<f32> = vec3<f32>(0.5, 0.5, 0.56);
const GRID_LIMIT: vec3<f32> = vec3<f32>(0.85, 0.2, 0.2);
const LIGHT_MINOR: vec3<f32> = vec3<f32>(0.80, 0.80, 0.79);
const LIGHT_MAJOR: vec3<f32> = vec3<f32>(0.66, 0.66, 0.65);
const LIGHT_1024: vec3<f32> = vec3<f32>(0.86, 0.62, 0.40);
const LIGHT_64: vec3<f32> = vec3<f32>(0.72, 0.72, 0.76);

fn grid_rgb(dark: vec3<f32>, light: vec3<f32>) -> vec3<f32> {
    if (view.opts.x & OPT_LIGHT_BACKGROUND) != 0u {
        return srgb_to_linear(light);
    }
    return srgb_to_linear(dark);
}
const MAP_LIMIT: f32 = 16384.0;

fn grid_hit(r0: f32, r1: f32, u1: f32, u0: f32, step: f32) -> bool {
    let a = hits_ascending(r0, r1, step);
    let b = hits_descending(u1, u0, step);
    if view.grid3d.y > 0.5 {
        return a && b;
    }
    return a || b;
}

@fragment
fn fs_grid(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let ppu = max(view.grid_b.z, 1e-6);
    let px = floor(frag.xy);
    let r0 = view.ortho.x + (px.x - view.ortho.z) / ppu;
    let r1 = view.ortho.x + (px.x + 1.0 - view.ortho.z) / ppu;
    let u0 = view.ortho.y - (px.y - view.ortho.w) / ppu;
    let u1 = view.ortho.y - (px.y + 1.0 - view.ortho.w) / ppu;

    let out_r = r1 <= -MAP_LIMIT || r0 > MAP_LIMIT;
    let out_u = u0 < -MAP_LIMIT || u1 >= MAP_LIMIT;
    let edge_r = (r0 <= -MAP_LIMIT && -MAP_LIMIT < r1) || (r0 <= MAP_LIMIT && MAP_LIMIT < r1);
    let edge_u = (u1 < -MAP_LIMIT && -MAP_LIMIT <= u0) || (u1 < MAP_LIMIT && MAP_LIMIT <= u0);
    if (edge_r && !out_u) || (edge_u && !out_r) {
        return vec4<f32>(srgb_to_linear(GRID_LIMIT), 0.9);
    }
    if out_r || out_u {
        return vec4<f32>(0.0, 0.0, 0.0, 0.5);
    }

    let axis = (r0 <= 0.0 && 0.0 < r1) || (u1 < 0.0 && 0.0 <= u0);
    if axis && view.grid_b.y > 0.0 {
        return vec4<f32>(srgb_to_linear(GRID_AXIS), view.grid_b.y);
    }
    if view.grid_b.x > 0.0 && (hits_ascending(r0, r1, 1024.0) || hits_descending(u1, u0, 1024.0)) {
        return vec4<f32>(grid_rgb(GRID_1024, LIGHT_1024), view.grid_b.x);
    }
    let major = view.grid_a.z;
    if view.grid_a.w > 0.0 && grid_hit(r0, r1, u1, u0, major) {
        return vec4<f32>(grid_rgb(GRID_MAJOR, LIGHT_MAJOR), view.grid_a.w);
    }
    if view.grid3d.x > 0.0 && grid_hit(r0, r1, u1, u0, 64.0) {
        return vec4<f32>(grid_rgb(GRID_64, LIGHT_64), view.grid3d.x * view.grid_b.y);
    }
    let minor = view.grid_a.x;
    if view.grid_a.y > 0.0 && grid_hit(r0, r1, u1, u0, minor) {
        return vec4<f32>(grid_rgb(GRID_MINOR, LIGHT_MINOR), view.grid_a.y);
    }
    return vec4<f32>(0.0);
}
