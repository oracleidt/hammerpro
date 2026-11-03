struct PresentParams {
    bloom: vec4<f32>,
};

@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var bloom_tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var<uniform> pp: PresentParams;

struct FullOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> FullOut {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    var out: FullOut;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 0.5 - y * 0.5);
    return out;
}

@fragment
fn fs_present(in: FullOut) -> @location(0) vec4<f32> {
    var rgb = textureLoad(scene, vec2<i32>(in.pos.xy), 0).rgb;
    if pp.bloom.x > 0.0 {
        rgb += textureSampleLevel(bloom_tex, samp, in.uv, 0.0).rgb * pp.bloom.x;
    }
    rgb = select(vec3<f32>(0.0), rgb, rgb == rgb);
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
