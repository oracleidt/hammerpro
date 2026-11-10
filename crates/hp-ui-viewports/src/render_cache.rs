use hp_render::{Overlay, ViewOptions, ViewParams};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewKey {
    pub params: ViewParams,
    pub options: ViewOptions,
    pub content: u64,
    pub retained: u64,
    pub overlay: u64,
}

#[derive(Debug, Default)]
pub struct ContentClock {
    own_bumps: u64,
    retained: [u64; 4],
}

impl ContentClock {
    fn own(&mut self, renderer: &mut hp_render::Renderer, change: impl FnOnce(&mut hp_render::Renderer)) {
        let before = renderer.content_generation();
        change(renderer);
        self.own_bumps = self.own_bumps.wrapping_add(renderer.content_generation().wrapping_sub(before));
    }

    pub fn set_options(&mut self, renderer: &mut hp_render::Renderer, view: u32, options: ViewOptions) {
        self.own(renderer, |r| r.set_view_options(view, options));
    }

    pub fn set_retained(&mut self, renderer: &mut hp_render::Renderer, view: u32, overlay: &Overlay) {
        self.own(renderer, |r| r.set_retained_overlay(view, overlay));
        self.bump_retained(view);
    }

    pub fn clear_retained(&mut self, renderer: &mut hp_render::Renderer, view: u32) {
        self.own(renderer, |r| r.clear_retained_overlay(view));
        self.bump_retained(view);
    }

    fn bump_retained(&mut self, view: u32) {
        if let Some(serial) = self.retained.get_mut(view as usize) {
            *serial = serial.wrapping_add(1);
        }
    }

    pub fn retained(&self, view: usize) -> u64 {
        self.retained.get(view).copied().unwrap_or(0)
    }

    pub fn content(&self, renderer: &hp_render::Renderer) -> u64 {
        renderer.content_generation().wrapping_sub(self.own_bumps)
    }
}

#[derive(Debug, Default)]
pub struct RenderCache {
    keys: [Option<ViewKey>; 4],
    pub rendered: u64,
    pub reused: u64,
}

impl RenderCache {
    pub fn needs_render(&self, view: usize, key: &ViewKey, has_image: bool) -> bool {
        !has_image || self.keys.get(view).copied().flatten().as_ref() != Some(key)
    }

    pub fn rendered(&mut self, view: usize, key: ViewKey) {
        if let Some(k) = self.keys.get_mut(view) {
            *k = Some(key);
        }
        self.rendered += 1;
    }

    pub fn reused(&mut self) {
        self.reused += 1;
    }

    pub fn invalidate(&mut self, view: usize) {
        if let Some(k) = self.keys.get_mut(view) {
            *k = None;
        }
    }
}

struct Hasher(u64);

impl Hasher {
    fn word(&mut self, w: u32) {
        self.0 = (self.0 ^ u64::from(w)).wrapping_mul(0x0000_0100_0000_01b3);
    }

    fn floats(&mut self, v: &[f32]) {
        for f in v {
            self.word(f.to_bits());
        }
    }
}

pub fn overlay_hash(o: &Overlay) -> u64 {
    let mut h = Hasher(0xcbf2_9ce4_8422_2325);
    for (section, lines) in [(1u32, &o.lines), (2, &o.lines_on_top)] {
        h.word(section);
        h.word(lines.len() as u32);
        for l in lines.iter() {
            h.floats(&l.a);
            h.floats(&l.b);
            h.floats(&l.color);
        }
    }
    for (section, tris) in [(3u32, &o.triangles), (4, &o.triangles_on_top)] {
        h.word(section);
        h.word(tris.len() as u32);
        for t in tris.iter() {
            for p in &t.p {
                h.floats(p);
            }
            h.floats(&t.color);
        }
    }
    h.word(5);
    h.word(o.points.len() as u32);
    for p in &o.points {
        h.floats(&p.pos);
        h.floats(&p.color);
        h.floats(&[p.size_px]);
    }
    h.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_render::{OverlayLine, OverlayPoint, RenderMode, ViewKind};

    fn params() -> ViewParams {
        ViewParams {
            game_pixel_grid: false,
            size: [100, 80],
            kind: ViewKind::Perspective {
                eye: [0.0; 3],
                forward: [1.0, 0.0, 0.0],
                up: [0.0, 0.0, 1.0],
                fov_y_deg: 75.0,
                near: 1.0,
                far: 1000.0,
            },
            mode: RenderMode::Textured,
            grid: None,
            background: [0.0; 3],
            selection_color: [1.0; 4],
            show_sprites: true,
            show_models: true,
            msaa: true,
        }
    }

    #[test]
    fn unchanged_views_are_reused() {
        let mut cache = RenderCache::default();
        let key = ViewKey {
            params: params(),
            options: ViewOptions::default(),
            content: 7,
            retained: 0,
            overlay: overlay_hash(&Overlay::default()),
        };
        assert!(cache.needs_render(0, &key, true), "never rendered");
        cache.rendered(0, key);
        assert!(!cache.needs_render(0, &key, true));
        assert!(cache.needs_render(0, &key, false), "the image is gone");
        assert!(cache.needs_render(1, &key, true), "another view");
        assert!(cache.needs_render(0, &ViewKey { content: 8, ..key }, true), "content changed");
        let mut moved = params();
        moved.size = [101, 80];
        assert!(cache.needs_render(0, &ViewKey { params: moved, ..key }, true), "resized");
        cache.invalidate(0);
        assert!(cache.needs_render(0, &key, true));
        assert!(
            cache.needs_render(9, &key, true) && {
                cache.rendered(9, key);
                true
            },
            "out of range views always render"
        );
    }

    #[test]
    fn overlay_hash_sees_every_primitive() {
        let mut o = Overlay::default();
        let empty = overlay_hash(&o);
        o.lines.push(OverlayLine { a: [0.0; 3], b: [1.0; 3], color: [1.0; 4] });
        let one = overlay_hash(&o);
        assert_ne!(empty, one);
        o.lines[0].b[2] = 1.5;
        assert_ne!(one, overlay_hash(&o));
        let mut top = Overlay::default();
        top.lines_on_top.push(o.lines[0]);
        assert_ne!(overlay_hash(&top), overlay_hash(&o));
        let mut p = Overlay::default();
        p.points.push(OverlayPoint { pos: [0.0; 3], color: [1.0; 4], size_px: 4.0 });
        let a = overlay_hash(&p);
        p.points[0].size_px = 5.0;
        assert_ne!(a, overlay_hash(&p));
    }
}
