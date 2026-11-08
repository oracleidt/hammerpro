struct SunGlowU {
    base: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    color: vec4<f32>,
    proxy: vec4<f32>,
    params: vec4<f32>,
};

@group(2) @binding(0) var<uniform> glow: SunGlowU;
@group(2) @binding(1) var scene_depth: texture_depth_2d;

struct SunGlowOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

var<private> GLOW_CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(1.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, 1.0),
);

var<private> GLOW_TAPS: array<vec2<f32>, 13> = array<vec2<f32>, 13>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(0.707, 0.707),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(-0.707, 0.707),
    vec2<f32>(-1.0, 0.0),
    vec2<f32>(-0.707, -0.707),
    vec2<f32>(0.0, -1.0),
    vec2<f32>(0.707, -0.707),
    vec2<f32>(0.5, 0.25),
    vec2<f32>(-0.5, 0.25),
    vec2<f32>(0.5, -0.25),
    vec2<f32>(-0.5, -0.25),
);

@vertex
fn vs_sun_glow(@builtin(vertex_index) vi: u32) -> SunGlowOut {
    let c = GLOW_CORNERS[vi % 6u];
    var out: SunGlowOut;
    out.uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    let world = glow.base.xyz + glow.right.xyz * c.x + glow.up.xyz * c.y;
    out.clip = view.view_proj * vec4<f32>(world, 1.0);
    return out;
}

fn sun_visible_fraction() -> f32 {
    let size = view.viewport.xy;
    let radius = glow.color.w;
    var seen = 0.0;
    for (var i = 0u; i < 13u; i++) {
        let at = glow.proxy.xy + GLOW_TAPS[i] * radius;
        let px = vec2<i32>(clamp(at, vec2<f32>(0.0), size - vec2<f32>(1.0)));
        if textureLoad(scene_depth, px, 0) <= glow.base.w {
            seen += 1.0;
        }
    }
    return seen / 13.0;
}

@fragment
fn fs_sun_glow(in: SunGlowOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp_clamp, in.uv) * material.tint;
    if glow.proxy.z < 0.5 && t.a < 0.02 {
        discard;
    }
    var vis = 1.0;
    let entity = glow.proxy.z > 0.5;
    let mode = u32(glow.proxy.w);
    if !entity || mode == 3u || mode == 9u {
        vis = sun_visible_fraction();
    } else {
        let px = vec2<i32>(in.clip.xy);
        if textureLoad(scene_depth, px, 0) > in.clip.z + 1e-6 { discard; }
    }
    if vis <= 0.0 {
        discard;
    }
    if entity {
        let alpha = t.a * glow.right.w;
        let color = floor(glow.color.rgb * vis * 255.0 + vec3<f32>(1e-4)) / 255.0;
        let tint = select(color, pow(color, vec3<f32>(2.2)), glow.up.w > 0.5);
        let exposure = select(glow.params.x, pow(glow.params.x, 2.2), glow.up.w > 0.5);
        var rgb = t.rgb * tint * exposure;
        if mode == 0u { return vec4<f32>(t.rgb, 1.0); }
        if mode == 3u || mode == 5u || mode == 7u || mode == 9u { rgb *= alpha; }
        return vec4<f32>(rgb, alpha);
    }
    return vec4<f32>(t.rgb * glow.color.rgb * vis, t.a);
}
