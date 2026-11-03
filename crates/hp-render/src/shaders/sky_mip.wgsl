@group(0) @binding(0) var src: texture_depth_2d;

struct FragOut {
    @builtin(frag_depth) depth: f32,
}

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_min(@builtin(position) pos: vec4<f32>) -> FragOut {
    let p = vec2<i32>(pos.xy) * 2;
    let a = textureLoad(src, p, 0);
    let b = textureLoad(src, p + vec2<i32>(1, 0), 0);
    let c = textureLoad(src, p + vec2<i32>(0, 1), 0);
    let d = textureLoad(src, p + vec2<i32>(1, 1), 0);
    return FragOut(min(min(a, b), min(c, d)));
}
