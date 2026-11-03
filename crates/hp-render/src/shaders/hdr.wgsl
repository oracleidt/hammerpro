struct HdrParams {
    size: vec2<u32>,
    inset: vec2<u32>,
    lum: vec4<f32>,
    bloom: vec4<f32>,
    blur: vec4<f32>,
};

@group(0) @binding(0) var frame: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> bins: array<atomic<u32>, 16>;
@group(0) @binding(2) var<uniform> hp: HdrParams;

@compute @workgroup_size(16, 16)
fn histogram(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = hp.inset + id.xy * 2u;
    if p.x >= hp.size.x - hp.inset.x || p.y >= hp.size.y - hp.inset.y {
        return;
    }
    let c = textureLoad(frame, vec2<i32>(p), 0).rgb;
    var lum = dot(c, hp.lum.xyz);
    if !(lum > 0.0) {
        lum = 0.0;
    }
    var b = 15u;
    if lum < 1.0 {
        b = min(u32(pow(lum, 0.6666667) * 16.0), 15u);
    }
    atomicAdd(&bins[b], 1u);
}
