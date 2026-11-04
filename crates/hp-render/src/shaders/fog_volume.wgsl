struct FogVol {
    fog_a: vec4<f32>,
    fog_b: vec4<f32>,
    fog_c: vec4<f32>,
    lo: vec4<f32>,
    hi: vec4<f32>,
    depth: vec4<f32>,
};

@group(1) @binding(0) var<uniform> vol: FogVol;
@group(1) @binding(1) var scene_depth: texture_depth_2d;

struct FogOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_fog_volume(@builtin(vertex_index) vi: u32) -> FogOut {
    let x = f32((vi << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vi & 2u) * 2.0 - 1.0;
    var out: FogOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.ndc = vec2<f32>(x, y);
    return out;
}

fn fog_hash(p: vec3<f32>) -> f32 {
    let q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    let r = q + dot(q, q.yxz + 33.33);
    return fract((r.x + r.y) * r.z);
}

fn fog_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(fog_hash(i), fog_hash(i + vec3<f32>(1.0, 0.0, 0.0)), u.x);
    let b = mix(fog_hash(i + vec3<f32>(0.0, 1.0, 0.0)), fog_hash(i + vec3<f32>(1.0, 1.0, 0.0)), u.x);
    let c = mix(fog_hash(i + vec3<f32>(0.0, 0.0, 1.0)), fog_hash(i + vec3<f32>(1.0, 0.0, 1.0)), u.x);
    let d = mix(fog_hash(i + vec3<f32>(0.0, 1.0, 1.0)), fog_hash(i + vec3<f32>(1.0, 1.0, 1.0)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z);
}

fn safe_inv(v: f32) -> f32 {
    if abs(v) < 1.0e-6 {
        return select(-1.0e6, 1.0e6, v >= 0.0);
    }
    return 1.0 / v;
}

@fragment
fn fs_fog_volume(in: FogOut) -> @location(0) vec4<f32> {
    let size = view.viewport.xy;
    let px = vec2<i32>(clamp(in.clip.xy, vec2<f32>(0.0), size - vec2<f32>(1.0)));
    let raw = textureLoad(scene_depth, px, 0);
    let dir = view.forward.xyz + view.right.xyz * (in.ndc.x * view.proj.x) + view.up.xyz * (in.ndc.y * view.proj.y);
    let len = length(dir);
    let ray = dir / len;
    var scene = 1.0e9;
    if raw > 1.0e-7 {
        scene = vol.depth.y / (raw + vol.depth.x) * len;
    }
    let eye = view.eye.xyz;
    let inv = vec3<f32>(safe_inv(ray.x), safe_inv(ray.y), safe_inv(ray.z));
    let t1 = (vol.lo.xyz - eye) * inv;
    let t2 = (vol.hi.xyz - eye) * inv;
    let near_t = min(t1, t2);
    let far_t = max(t1, t2);
    let enter = max(max(max(near_t.x, near_t.y), near_t.z), 0.0);
    let exit = min(min(min(far_t.x, far_t.y), far_t.z), scene);
    if exit <= enter {
        discard;
    }
    let path = exit - enter;
    let start = vol.fog_b.w;
    let end = max(vol.fog_c.w, start + 1.0);
    var f = clamp((path - start) / (end - start), 0.0, 1.0) * vol.fog_a.w;
    let scale = vol.depth.z;
    var n = 0.0;
    for (var k = 0; k < 3; k++) {
        let p = eye + ray * mix(enter, exit, (f32(k) + 0.5) / 3.0);
        n += fog_noise(p * scale) * 0.6 + fog_noise(p * scale * 2.7) * 0.4;
    }
    f *= mix(0.8, 1.15, n / 3.0);
    f *= smoothstep(0.0, 6.0, path);
    var rgb = vol.fog_a.rgb;
    if vol.lo.w > 0.5 {
        let t = clamp(dot(view.forward.xyz, vol.fog_c.xyz) * 0.5 + 0.5, 0.0, 1.0);
        rgb = mix(vol.fog_b.rgb, vol.fog_a.rgb, t);
    }
    return vec4<f32>(rgb, clamp(f, 0.0, 1.0));
}
