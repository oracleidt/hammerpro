override PARTICLE_BLEND_MODE: f32 = 0.0;

struct ParticleOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
    @location(3) addself: f32,
    @location(4) @interpolate(flat) lightmode: u32,
    @location(5) @interpolate(flat) effects: u32,
    @location(6) next_uv: vec2<f32>,
    @location(7) sequence_uv: vec4<f32>,
    @location(8) animation_blend: vec2<f32>,
    @location(9) @interpolate(flat) animation_flags: u32,
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
const P_TRAIL_CPU: u32 = 6u;
const P_TRAIL_SPLINE: u32 = 7u;
const P_SPLINE_LINEAR_COLOR: u32 = 131072u;

@vertex
fn vs_particle(
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec3<f32>,
    @location(1) size: f32,
    @location(2) axis: vec3<f32>,
    @location(3) roll: f32,
    @location(4) rect: vec4<f32>,
    @location(5) color: vec4<f32>,
    @location(6) mode_bits: u32,
    @location(7) bright: f32,
    @location(8) fade: vec2<f32>,
    @location(9) effects: u32,
    @location(10) next_rect: vec4<f32>,
    @location(11) sequence_rect: vec4<f32>,
    @location(12) sequence_next_rect: vec4<f32>,
    @location(13) animation_blend: vec2<f32>,
    @location(14) animation_flags: u32,
    @location(15) yaw: f32,
) -> ParticleOut {
    var out: ParticleOut;
    if is_ortho() || hidden_in_view(0u) {
        out.clip = COLLAPSED;
        return out;
    }
    let mode = mode_bits & 127u;
    out.effects = effects;
    if mode == P_TRAIL_SPLINE {
        out.effects |= P_SPLINE_LINEAR_COLOR;
    }
    out.lightmode = select(0u, mode - 3u, mode >= 4u && mode <= 5u);
    out.addself = f32(mode_bits >> 8u) / 256.0;
    if (mode_bits & 128u) != 0u {
        out.addself = f32(mode_bits >> 8u) / 65536.0 - 1.0;
    }
    let c = P_CORNERS[vi % 6u];
    let uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    out.uv = mix(rect.xy, rect.zw, uv);
    out.next_uv = mix(next_rect.xy, next_rect.zw, uv);
    out.sequence_uv = vec4<f32>(mix(sequence_rect.xy, sequence_rect.zw, uv), mix(sequence_next_rect.xy, sequence_next_rect.zw, uv));
    out.animation_blend = animation_blend;
    out.animation_flags = animation_flags;
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
    let yaw_axis = dy;
    if mode == P_SCREEN || mode == P_FLAT || mode == 4u || mode == 5u {
        let cr = cos(roll);
        let sr = sin(roll);
        let rx = dx * cr + dy * sr;
        let ry = dy * cr - dx * sr;
        dx = rx;
        dy = ry;
    }
    var offset = dx * (c.x * half) + dy * (c.y * half_y);
    if yaw != 0.0 && mode != P_STRETCHED && mode != P_TRAIL_CPU && mode != P_TRAIL_SPLINE {
        let cy = cos(yaw);
        let sy = sin(yaw);
        offset = offset * cy + cross(yaw_axis, offset) * sy + yaw_axis * dot(offset, yaw_axis) * (1.0 - cy);
    }
    var world = pos + offset;
    if mode == P_TRAIL_CPU || mode == P_TRAIL_SPLINE {
        // Trail UVs run from V=1 at the head to V=0 at the tail. A native
        // radius is the FULL ribbon width; ordinary sprites use twice that.
        let t = c.y * 0.5 + 0.5;
        var side = cross(pos - view.eye.xyz, axis);
        var squared = (side.z * side.z + side.y * side.y) + (side.x * side.x + 1e-10);
        if mode == P_TRAIL_SPLINE {
            let axis_length = length(axis);
            if axis_length == 0.0 {
                out.clip = COLLAPSED;
                return out;
            }
            side = cross((pos + axis * t) - view.eye.xyz, axis / axis_length);
            squared = dot(side, side);
        }
        if squared <= 0.0 {
            out.clip = COLLAPSED;
            return out;
        }
        side *= inverseSqrt(squared);
        world = (pos + side * (-c.x * half)) + axis * t;
    }
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
    // Source's spline-card shader takes the byte vertex color as linear. The
    // ordinary SpriteCard and CPU ribbon paths instead apply gamma 2.2.
    let rgb = select(pow(max(color.rgb, vec3<f32>(0.0)), vec3<f32>(2.2)), color.rgb, mode == P_TRAIL_SPLINE);
    out.color = vec4<f32>(rgb * max(bright, 0.0), alpha);
    out.clip = view.view_proj * vec4<f32>(world, 1.0);
    return out;
}

fn p_linear_to_srgb(x: f32) -> f32 {
    if x <= 0.0031308 {
        return x * 12.92;
    }
    return 1.055 * pow(x, 1.0 / 2.4) - 0.055;
}

fn particle_color(in: ParticleOut) -> vec3<f32> {
    if in.lightmode == 0u || (view.misc.x != MODE_LIT && (view.opts.x & OPT_COMPILED_LIGHT) == 0u) {
        return in.color.rgb;
    }
    var sum = vec3<f32>(0.0);
    for (var axis = 0u; axis < 3u; axis++) {
        var n = vec3<f32>(0.0);
        n[axis] = 1.0;
        sum += lighting(in.world, n, n, in.clip.xy, 0u, false);
        sum += lighting(in.world, -n, -n, in.clip.xy, 0u, false);
    }
    let light = clamp(sum / 6.0, vec3<f32>(0.0), vec3<f32>(1.0));
    if in.lightmode == 2u {
        return min(in.color.rgb + light, vec3<f32>(1.0));
    }
    return in.color.rgb * light;
}

fn particle_frame_mix(a: vec4<f32>, b: vec4<f32>, fraction: f32, maximum: bool, lum_fraction: f32) -> vec4<f32> {
    if maximum {
        let weights = vec3<f32>(0.3, 0.59, 0.11);
        return select(b, a, dot(weights, a.rgb * (1.0 - lum_fraction)) > dot(weights, b.rgb * lum_fraction));
    }
    return mix(a, b, fraction);
}

fn particle_texel(in: ParticleOut) -> vec4<f32> {
    let first = textureSample(tex, samp_clamp, in.uv);
    let next_dx = dpdx(in.next_uv);
    let next_dy = dpdy(in.next_uv);
    let sequence_dx = dpdx(in.sequence_uv);
    let sequence_dy = dpdy(in.sequence_uv);
    var color = first;
    if in.animation_blend.x > 0.0 || (in.animation_flags & 8u) != 0u {
        let next = textureSampleGrad(tex, samp_clamp, in.next_uv, next_dx, next_dy);
        color = particle_frame_mix(first, next, in.animation_blend.x, (in.animation_flags & 8u) != 0u, in.animation_blend.x);
    }
    if (in.animation_flags & 1u) != 0u {
        var other = textureSampleGrad(tex, samp_clamp, in.sequence_uv.xy, sequence_dx.xy, sequence_dy.xy);
        if in.animation_blend.y > 0.0 || (in.animation_flags & 16u) != 0u {
            let second_next = textureSampleGrad(tex, samp_clamp, in.sequence_uv.zw, sequence_dx.zw, sequence_dy.zw);
            other = particle_frame_mix(other, second_next, in.animation_blend.y, (in.animation_flags & 16u) != 0u, in.animation_blend.x);
        }
        let mode = (in.animation_flags >> 1u) & 3u;
        if mode == 0u {
            color = (color + other) * 0.5;
        } else if mode == 1u {
            color = vec4<f32>(other.rgb, color.a);
        } else {
            color = vec4<f32>(mix(color.rgb, other.rgb, other.a), color.a);
        }
    }
    return color;
}

@fragment
fn fs_particle(in: ParticleOut) -> @location(0) vec4<f32> {
    let t = particle_texel(in) * material.tint;
    let exposure = bitcast<f32>(view.misc.z);
    fog_far_clip(in.world);
    let fog = select(fog_amount(in.world), 0.0, (in.effects & 65536u) != 0u);
    let color = particle_color(in);
    let spline = (in.effects & P_SPLINE_LINEAR_COLOR) != 0u;
    let card = spline || (material.flags & M_SPRITE_CARD) != 0u;
    if card && PARTICLE_BLEND_MODE != 1.0 && t.a * in.color.a <= 0.01 {
        discard;
    }
    if PARTICLE_BLEND_MODE > 1.5 {
        let peak = p_linear_to_srgb(max(max(t.r, t.g), t.b));
        let a = select(clamp(t.a / max(peak, 1.0 / 255.0), 0.0, 1.0), t.a, card) * in.color.a;
        let rgb = fog_blend(t.rgb * color * a * exposure, vec3<f32>(0.0), fog);
        if !card && max(max(rgb.r, rgb.g), rgb.b) < 1.0 / 512.0 {
            discard;
        }
        return vec4<f32>(rgb, 0.0);
    }
    let a = t.a * in.color.a;
    if a <= 0.0 {
        discard;
    }
    let rgb = t.rgb * color * exposure;
    let fogged = fog_blend(rgb, view.fog_a.rgb, fog);
    if PARTICLE_BLEND_MODE > 0.5 {
        if card {
            return vec4<f32>(fogged * a * (1.0 + in.addself * in.color.a), a);
        }
        return vec4<f32>(fogged * (a + in.addself * t.a * 0.12), a);
    }
    return vec4<f32>(fogged, a);
}
