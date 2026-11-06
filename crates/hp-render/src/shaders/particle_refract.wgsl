@group(2) @binding(0) var heat_scene: texture_2d<f32>;

@fragment
fn fs_particle_refract(in: ParticleOut) -> @location(0) vec4<f32> {
    fog_far_clip(in.world);
    let dims = vec2<f32>(textureDimensions(heat_scene));
    let last = vec2<i32>(dims) - vec2<i32>(1);
    let pixel = clamp(vec2<i32>(in.clip.xy), vec2<i32>(0), last);
    if in.clip.z + 1e-6 < heat_z(pixel) {
        discard;
    }
    let normal = textureSample(tex, samp_clamp, in.uv);
    let strength = clamp(in.addself, -1.0, 1.0);
    let shift = (normal.xy * 2.0 - 1.0) * strength;
    let uv = (in.clip.xy / dims);
    var distorted = clamp(uv + shift, vec2<f32>(0.5) / dims, vec2<f32>(1.0) - vec2<f32>(0.5) / dims);
    let shifted_pixel = clamp(vec2<i32>(distorted * dims), vec2<i32>(0), last);
    if in.clip.z + 1e-6 < heat_z(shifted_pixel) {
        distorted = uv;
    }
    let mask = normal.a * in.color.a * material.tint.a;
    if mask <= 1.0 / 255.0 || abs(strength) <= 1e-6 {
        discard;
    }
    let fog = select(fog_amount(in.world), 0.0, (in.effects & 65536u) != 0u);
    let alpha = mask * (1.0 - fog);
    var color = textureSample(heat_scene, samp_clamp, distorted).rgb;
    let blur = f32(in.effects & 65535u) / 4096.0;
    if blur > 0.0 {
        let step = vec2<f32>(blur) / dims;
        color = (color * 2.0
            + textureSample(heat_scene, samp_clamp, distorted + vec2<f32>(step.x, 0.0)).rgb
            + textureSample(heat_scene, samp_clamp, distorted - vec2<f32>(step.x, 0.0)).rgb
            + textureSample(heat_scene, samp_clamp, distorted + vec2<f32>(0.0, step.y)).rgb
            + textureSample(heat_scene, samp_clamp, distorted - vec2<f32>(0.0, step.y)).rgb) / 6.0;
    }
    return vec4<f32>(color, alpha);
}
