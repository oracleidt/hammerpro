struct BloomParams {
    lum: vec4<f32>,
    blur: vec4<f32>,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> bp: BloomParams;

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

fn shape(c: vec3<f32>) -> vec3<f32> {
    let m = max(c, vec3<f32>(0.0));
    let lum = max(dot(m, bp.lum.xyz), 1.0e-6);
    return pow(m, vec3<f32>(bp.lum.w)) * pow(lum, 2.0 - bp.lum.w);
}

@fragment
fn fs_down(in: FullOut) -> @location(0) vec4<f32> {
    let t = 1.0 / vec2<f32>(textureDimensions(src));
    var c = shape(textureSampleLevel(src, samp, in.uv + vec2<f32>(-t.x, -t.y), 0.0).rgb);
    c += shape(textureSampleLevel(src, samp, in.uv + vec2<f32>(t.x, -t.y), 0.0).rgb);
    c += shape(textureSampleLevel(src, samp, in.uv + vec2<f32>(-t.x, t.y), 0.0).rgb);
    c += shape(textureSampleLevel(src, samp, in.uv + vec2<f32>(t.x, t.y), 0.0).rgb);
    return vec4<f32>(c * 0.25, 1.0);
}

const TAP_OFFSET: array<f32, 6> = array<f32, 6>(1.3366, 3.4295, 5.4264, 7.4359, 9.4436, 11.4401);
const TAP_WEIGHT: array<f32, 6> = array<f32, 6>(0.2185, 0.0821, 0.0461, 0.0262, 0.0162, 0.0102);
const TAP_CENTER: f32 = 0.2013;

fn blur_tap(uv: vec2<f32>) -> vec3<f32> {
    let c = textureSampleLevel(src, samp, uv, 0.0).rgb;
    if bp.blur.y > 0.5 {
        return sqrt(max(c, vec3<f32>(0.0)));
    }
    return c;
}

fn blur(uv: vec2<f32>, dir: vec2<f32>) -> vec4<f32> {
    let t = dir / vec2<f32>(textureDimensions(src)) * bp.blur.x;
    var sum = blur_tap(uv) * TAP_CENTER;
    var w = TAP_CENTER;
    for (var k = 0u; k < 6u; k++) {
        let o = t * TAP_OFFSET[k];
        sum += (blur_tap(uv + o) + blur_tap(uv - o)) * TAP_WEIGHT[k];
        w += 2.0 * TAP_WEIGHT[k];
    }
    var rgb = sum / w;
    if bp.blur.y > 0.5 {
        rgb = rgb * rgb;
    }
    return vec4<f32>(rgb, 1.0);
}

@fragment
fn fs_blur_x(in: FullOut) -> @location(0) vec4<f32> {
    return blur(in.uv, vec2<f32>(1.0, 0.0));
}

@fragment
fn fs_blur_y(in: FullOut) -> @location(0) vec4<f32> {
    return blur(in.uv, vec2<f32>(0.0, 1.0));
}
