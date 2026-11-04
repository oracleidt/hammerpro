@group(1) @binding(0) var sky_rt: texture_2d<f32>;
@group(1) @binding(1) var sky_lf: texture_2d<f32>;
@group(1) @binding(2) var sky_bk: texture_2d<f32>;
@group(1) @binding(3) var sky_ft: texture_2d<f32>;
@group(1) @binding(4) var sky_up: texture_2d<f32>;
@group(1) @binding(5) var sky_dn: texture_2d<f32>;

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) vi: u32) -> SkyOut {
    let x = f32((vi << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vi & 2u) * 2.0 - 1.0;
    var out: SkyOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.ndc = vec2<f32>(x, y);
    return out;
}

fn face_uv(i: u32, s: f32, t: f32) -> vec2<f32> {
    let uv = vec2<f32>((s + 1.0) * 0.5, (1.0 - t) * 0.5) * view.sky_uv[i].xy + view.sky_uv[i].zw;
    return clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
}

fn sky_paint_color(d: vec3<f32>) -> vec3<f32> {
    let a = view.sky_paint[0];
    let b = view.sky_paint[1];
    let s = view.sky_paint[2];
    let sc = view.sky_paint[3];
    let dk = view.sky_paint[4];
    let z = clamp(d.z, 0.0, 1.0);
    var c = mix(b.xyz, a.xyz, pow(z, a.w));
    if s.w > 0.0 {
        c += sc.xyz * pow(clamp(dot(d, s.xyz), 0.0, 1.0), s.w);
    }
    let st = view.sky_paint[5];
    let sz = max(d.z, 1e-3);
    let suv = d.xy / sz * st.x + vec2<f32>(st.z * view.sky.z, 0.0);
    let gx = dpdx(suv);
    let gy = dpdy(suv);
    if st.w > 0.0 && d.z > 0.0 {
        let t = textureSampleGrad(sky_rt, samp, suv, gx, gy).rgb;
        let raw = select(1.055 * pow(t, vec3<f32>(1.0 / 2.4)) - 0.055, t * 12.92, t <= vec3<f32>(0.0031308));
        c += raw * (pow(sz, st.y) * st.w);
    }
    if dk.w > 0.0 {
        let hxy = length(d.xy);
        var dn = 0.0;
        if hxy > 1e-4 {
            dn = dot(d.xy, s.xy) / hxy;
        }
        let h = max(pow(1.0 - z, 4.5) * (0.582 + 0.381 * dn), 1e-6);
        let w = dk.w * pow(h, sc.w);
        c += (dk.xyz * 2.0 - c) * w;
    }
    return c * b.w;
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let w = view.forward.xyz + view.right.xyz * (in.ndc.x * view.proj.x) + view.up.xyz * (in.ndc.y * view.proj.y);
    let d0 = normalize(w);
    if (view.opts.x & OPT_SKY_PAINT) != 0u {
        return vec4<f32>(sky_paint_color(d0) * bitcast<f32>(view.misc.z), 1.0);
    }
    let c = view.sky.x;
    let s = view.sky.y;
    let d = vec3<f32>(c * d0.x + s * d0.y, -s * d0.x + c * d0.y, d0.z);
    let a = abs(d);
    var color: vec4<f32>;
    if a.x >= a.y && a.x >= a.z {
        if d.x > 0.0 {
            color = textureSampleLevel(sky_rt, samp_clamp, face_uv(0u, -d.y / a.x, d.z / a.x), 0.0);
        } else {
            color = textureSampleLevel(sky_lf, samp_clamp, face_uv(1u, d.y / a.x, d.z / a.x), 0.0);
        }
    } else if a.y >= a.z {
        if d.y > 0.0 {
            color = textureSampleLevel(sky_bk, samp_clamp, face_uv(2u, d.x / a.y, d.z / a.y), 0.0);
        } else {
            color = textureSampleLevel(sky_ft, samp_clamp, face_uv(3u, -d.x / a.y, d.z / a.y), 0.0);
        }
    } else if d.z > 0.0 {
        color = textureSampleLevel(sky_up, samp_clamp, face_uv(4u, -d.y / a.z, -d.x / a.z), 0.0);
    } else {
        color = textureSampleLevel(sky_dn, samp_clamp, face_uv(5u, -d.y / a.z, d.x / a.z), 0.0);
    }
    return vec4<f32>(color.rgb * bitcast<f32>(view.misc.z), 1.0);
}
