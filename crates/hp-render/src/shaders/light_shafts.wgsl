struct ShaftParams {
    exposure: f32,
    ldr: u32,
    pad: vec2<u32>,
};

@group(1) @binding(0) var<uniform> shaft: ShaftParams;

const HDR_COLOR_SCALE: f32 = 0.7;

struct ShaftOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
};

@vertex
fn vs_shaft(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> ShaftOut {
    var out: ShaftOut;
    out.clip = view.view_proj * vec4<f32>(pos, 1.0);
    out.uv = uv;
    out.color = color;
    out.world = pos;
    return out;
}

@fragment
fn fs_shaft(in: ShaftOut) -> @location(0) vec4<f32> {
    let across = 1.0 - abs(clamp(in.uv.x, 0.0, 1.0) * 2.0 - 1.0);
    let edge = smoothstep(0.0, 1.0, across);
    let start = smoothstep(0.0, 0.06, clamp(in.uv.y, 0.0, 1.0));
    let strength = max(in.color.a, 0.0) * edge * start;
    let ldr = shaft.ldr != 0u;
    let scale = select(shaft.exposure * bitcast<f32>(view.misc.w) * HDR_COLOR_SCALE, shaft.exposure, ldr);
    var rgb = max(in.color.rgb, vec3<f32>(0.0)) * strength;
    if ldr {
        rgb = srgb_to_linear(min(rgb, vec3<f32>(1.0)));
    }
    rgb = apply_fog_black(rgb * scale, in.world);
    return vec4<f32>(rgb, 0.0);
}

struct GlowOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) world: vec3<f32>,
};

const GLOW_RENDER_AMOUNT: f32 = 0.784;
const GLOW_HDR_COLOR_SCALE: f32 = 0.6;

@vertex
fn vs_glow(@builtin(vertex_index) i: u32, @location(0) center: vec3<f32>, @location(1) size: f32, @location(2) color: vec3<f32>) -> GlowOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let c = corners[i % 6u];
    let toward = normalize(view.eye.xyz - center);
    let world = center + (view.right.xyz * c.x + view.up.xyz * c.y) * size * 0.5 + toward * size * 0.5;
    var out: GlowOut;
    out.clip = view.view_proj * vec4<f32>(world, 1.0);
    out.corner = c;
    out.color = color;
    out.world = world;
    return out;
}

@fragment
fn fs_glow(in: GlowOut) -> @location(0) vec4<f32> {
    let r = length(in.corner);
    let profile = exp(-(r * r) * 5.0) * smoothstep(0.0, 1.0, clamp((1.0 - r) / 0.2, 0.0, 1.0));
    let ldr = shaft.ldr != 0u;
    let scale = select(shaft.exposure * bitcast<f32>(view.misc.w) * GLOW_HDR_COLOR_SCALE, shaft.exposure, ldr);
    var rgb = max(in.color, vec3<f32>(0.0)) * profile * GLOW_RENDER_AMOUNT;
    if ldr {
        rgb = srgb_to_linear(min(rgb, vec3<f32>(1.0)));
    }
    rgb = apply_fog_black(rgb * scale, in.world);
    return vec4<f32>(rgb, 0.0);
}
