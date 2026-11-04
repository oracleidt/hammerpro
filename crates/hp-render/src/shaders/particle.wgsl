override BLEND_MODE: f32 = 0.0;

struct ParticleOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
};

var<private> P_CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(1.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, 1.0),
);

const P_SCREEN: u32 = 0u;
const P_UPRIGHT: u32 = 1u;
const P_FLAT: u32 = 2u;
const P_STRETCHED: u32 = 3u;

@vertex
fn vs_particle(
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec3<f32>,
    @location(1) size: f32,
    @location(2) axis: vec3<f32>,
    @location(3) roll: f32,
    @location(4) rect: vec4<f32>,
    @location(5) color: vec4<f32>,
    @location(6) mode: u32,
    @location(7) bright: f32,
    @location(8) fade: vec2<f32>,
) -> ParticleOut {
    var out: ParticleOut;
    if is_ortho() || hidden_in_view(0u) {
        out.clip = COLLAPSED;
        return out;
    }
    let c = P_CORNERS[vi % 6u];
    out.uv = mix(rect.xy, rect.zw, vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5));
    let half = max(size, 0.0) * 0.5;
    var half_y = half;
    var dx = view.right.xyz;
    var dy = view.up.xyz;
    let to_eye = view.eye.xyz - pos;
    if mode == P_UPRIGHT {
        let h = vec2<f32>(-to_eye.y, to_eye.x);
        let len = length(h);
        if len > 1e-4 {
            dx = vec3<f32>(h / len, 0.0);
            dy = vec3<f32>(0.0, 0.0, 1.0);
        }
    } else if mode == P_FLAT {
        var n = vec3<f32>(0.0, 0.0, 1.0);
        if length(axis) > 1e-4 {
            n = normalize(axis);
        }
        if abs(n.z) > 0.99 {
            dx = vec3<f32>(1.0, 0.0, 0.0);
        } else {
            dx = normalize(cross(vec3<f32>(0.0, 0.0, 1.0), n));
        }
        dy = cross(n, dx);
    } else if mode == P_STRETCHED {
        let len = length(axis);
        if len > 1e-4 {
            let d = axis / len;
            let s = cross(d, to_eye);
            if length(s) > 1e-4 {
                dx = normalize(s);
                dy = d;
                half_y = len * 0.5;
            }
        }
    }
    if mode == P_SCREEN || mode == P_FLAT {
        let cr = cos(roll);
        let sr = sin(roll);
        let rx = dx * cr + dy * sr;
        let ry = dy * cr - dx * sr;
        dx = rx;
        dy = ry;
    }
    let world = pos + dx * (c.x * half) + dy * (c.y * half_y);
    out.world = world;
    var alpha = color.a;
    let f0 = fade.x * 4.0;
    let f1 = fade.y * 4.0;
    if f1 > f0 {
        let center = view.view_proj * vec4<f32>(pos, 1.0);
        let top = view.view_proj * vec4<f32>(pos + view.up.xyz * max(size, 0.0), 1.0);
        if center.w > 1e-3 && top.w > 1e-3 {
            let share = abs(top.y / top.w - center.y / center.w) * 0.5;
            alpha *= 1.0 - clamp((share - f0) / (f1 - f0), 0.0, 1.0);
        }
    }
    out.color = vec4<f32>(srgb_to_linear(color.rgb) * max(bright, 0.0), alpha);
    out.clip = view.view_proj * vec4<f32>(world, 1.0);
    return out;
}

fn p_linear_to_srgb(x: f32) -> f32 {
    if x <= 0.0031308 {
        return x * 12.92;
    }
    return 1.055 * pow(x, 1.0 / 2.4) - 0.055;
}

@fragment
fn fs_particle(in: ParticleOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp_clamp, in.uv) * material.tint;
    let exposure = bitcast<f32>(view.misc.z);
    fog_far_clip(in.world);
    let fog = fog_amount(in.world);
    if BLEND_MODE > 1.5 {
        let peak = p_linear_to_srgb(max(max(t.r, t.g), t.b));
        let a = clamp(t.a / max(peak, 1.0 / 255.0), 0.0, 1.0) * in.color.a;
        let rgb = fog_blend(t.rgb * in.color.rgb * a * exposure, vec3<f32>(0.0), fog);
        if max(max(rgb.r, rgb.g), rgb.b) < 1.0 / 512.0 {
            discard;
        }
        return vec4<f32>(rgb, 0.0);
    }
    let a = t.a * in.color.a;
    if a < 1.0 / 255.0 {
        discard;
    }
    let rgb = t.rgb * in.color.rgb * exposure;
    let fogged = fog_blend(rgb, view.fog_a.rgb, fog);
    if BLEND_MODE > 0.5 {
        return vec4<f32>(fogged * a, a);
    }
    return vec4<f32>(fogged, a);
}
