fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let c = match i as i32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    };
    c.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8)
}

pub fn sample_image(width: usize, height: usize) -> Vec<u8> {
    let (w, h) = (width.max(8), height.max(8));
    let mut px = vec![255u8; w * h * 4];
    let skin: [[u8; 3]; 6] =
        [[255, 224, 196], [234, 192, 160], [198, 145, 110], [141, 85, 60], [92, 58, 40], [60, 38, 28]];
    let swatch: [[u8; 3]; 6] =
        [[200, 40, 40], [230, 170, 40], [70, 160, 60], [40, 150, 190], [60, 70, 190], [150, 60, 170]];
    for y in 0..h {
        let fy = y as f32 / h as f32;
        for x in 0..w {
            let fx = x as f32 / w as f32;
            let rgb = if fy < 0.5 {
                let t = fy / 0.5;
                let sky_top = [40.0, 90.0, 190.0];
                let sky_low = [190.0, 215.0, 240.0];
                let mix = |a: f32, b: f32| (a + (b - a) * t).round() as u8;
                let base = [mix(sky_top[0], sky_low[0]), mix(sky_top[1], sky_low[1]), mix(sky_top[2], sky_low[2])];
                let hill = 0.3 + 0.12 * (fx * 9.0).sin() + 0.06 * (fx * 23.0).cos();
                if fy > hill + 0.12 { [60, (110.0 + 60.0 * fx) as u8, 50] } else { base }
            } else if fy < 0.64 {
                hsv(fx, 0.9, 0.95)
            } else if fy < 0.74 {
                let v = (fx * 255.0).round() as u8;
                [v, v, v]
            } else if fy < 0.87 {
                skin[((fx * 6.0) as usize).min(5)]
            } else {
                swatch[((fx * 6.0) as usize).min(5)]
            };
            let at = (y * w + x) * 4;
            px[at..at + 3].copy_from_slice(&rgb);
        }
    }
    px
}
