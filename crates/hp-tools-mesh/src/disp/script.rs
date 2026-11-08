use egui::Modifiers;
use glam::DVec3;
use hp_app::state::AppState;
use hp_app::tool::{ScriptedStroke, StrokeOutcome, ToolOption};
use hp_geom::Falloff;

use super::paint::selected_disp_faces;
use super::sculpt::{BoundsLimit, PROFILE_FLAT, PROFILE_U, PROFILE_V, PushMode, SculptOp};
use super::seams::nearest_disp_face;
use super::tags::{TagKind, TagValue};
use super::{AlphaEffect, DisplacementTool, GeoEffect, Interaction, PaintAxis, SubdivisionShape};

pub const MAX_STROKE_POINTS: usize = 4096;

fn on_off(b: bool) -> String {
    if b { "on" } else { "off" }.to_owned()
}

fn num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_owned() } else { s.to_owned() }
}

fn nums(v: &[f64]) -> String {
    v.iter().map(|x| num(*x)).collect::<Vec<_>>().join(" ")
}

fn parse_nums(text: &str) -> Option<Vec<f64>> {
    text.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty())
        .map(|w| w.parse::<f64>().ok().filter(|x| x.is_finite()))
        .collect()
}

impl DisplacementTool {
    pub(super) fn script_option_values(&self) -> Vec<ToolOption> {
        let b = &self.brush;
        let s = &b.sculpt;
        let mode = match self.interaction {
            Interaction::Select => "select",
            Interaction::Geometry => "geometry",
            Interaction::Alpha => "alpha",
            Interaction::Sculpt => "sculpt",
            Interaction::Tag => "tag",
        };
        let effect = match b.geo_effect {
            GeoEffect::Raise => "raise",
            GeoEffect::Lower => "lower",
            GeoEffect::Smooth => "smooth",
            GeoEffect::Set => "set",
            GeoEffect::Relax => "relax",
        };
        let axis = match b.axis {
            PaintAxis::FaceNormal => "normal",
            PaintAxis::SubdivNormal => "subdiv",
            PaintAxis::View => "view",
            PaintAxis::ViewFlat => "view_flat",
            PaintAxis::BrushCenter => "center",
            PaintAxis::X => "x",
            PaintAxis::Y => "y",
            PaintAxis::Z => "z",
            PaintAxis::Custom => "custom",
        };
        let falloff = match b.falloff {
            Falloff::Constant => "constant",
            Falloff::Linear => "linear",
            Falloff::Smooth => "smooth",
        };
        let alpha = match b.alpha_effect {
            AlphaEffect::Add => "add",
            AlphaEffect::Remove => "remove",
            AlphaEffect::Set => "set",
            AlphaEffect::Smooth => "smooth",
        };
        let sculpt = match s.op {
            SculptOp::Push => "push",
            SculptOp::Carve => "carve",
            SculptOp::Project => "project",
            SculptOp::Pinch => "pinch",
            SculptOp::Grab => "grab",
            SculptOp::Inflate => "inflate",
            SculptOp::Flatten => "flatten",
            SculptOp::Relax => "relax",
        };
        let push = match s.push_mode {
            PushMode::Absolute => "absolute",
            PushMode::Adaptive => "adaptive",
        };
        let bounds = match s.bounds {
            BoundsLimit::Attenuated => "attenuated",
            BoundsLimit::Additive => "additive",
        };
        let tag_kind = match b.tag.kind {
            TagKind::Walkable => "walkable",
            TagKind::Buildable => "buildable",
            TagKind::Remove => "remove",
        };
        let tag_value = match b.tag.value {
            TagValue::On => "on",
            TagValue::Off => "off",
            TagValue::Auto => "auto",
        };
        let c = b.custom_axis;
        let mut options = vec![
            ToolOption::new(
                "full_displacement",
                on_off(self.full_displacement),
                "on: create all quad faces of selected blocks; off: create only upward faces unless faces are selected",
            ),
            ToolOption::new(
                "subdivision_shape",
                match self.subdivision_shape {
                    SubdivisionShape::Smooth => "smooth",
                    SubdivisionShape::Sphere => "sphere",
                },
                "subdivision shape: smooth or sphere; sphere treats connected selections separately",
            ),
            ToolOption::new(
                "subdivision_amount",
                num(self.subdivision_amount),
                "subdivision or sphere strength (0 to 1)",
            ),
            ToolOption::new(
                "mode",
                mode,
                "select, geometry (paint heights), alpha (paint the blend), sculpt, or tag (paint the walkable, \
                 buildable and remove triangle tags)",
            ),
            ToolOption::new(
                "effect",
                effect,
                "geometry painting: raise, lower, smooth, relax or set (Hammer's Raise to: a target height; raise_to is \
                 also read)",
            ),
            ToolOption::new(
                "axis",
                axis,
                "direction vertices move: normal (face normal), subdiv (surface normal), center (the surface normal \
                 under the brush center), view (toward the 3D camera), view_flat (toward the 3D camera on X and Y \
                 only: the camera direction flattened onto the ground, never up or down), x, y, z or custom (Alt \
                 and a right click on a face in the 3D view sets it)",
            ),
            ToolOption::new("custom_axis", nums(&[c.x, c.y, c.z]), "direction of the custom axis: \"x y z\""),
            ToolOption::new("radius", num(b.radius), "brush radius in units (1 to 8192)"),
            ToolOption::new(
                "strength",
                num(b.strength),
                "brush intensity: raise, lower, pinch and inflate speed in units per second (0 to 8192)",
            ),
            ToolOption::new("smooth", num(b.smooth), "smoothing strength, also of Shift while sculpting (0 to 1)"),
            ToolOption::new(
                "target_height",
                num(b.target_height),
                "height of the set effect along the axis (-16384 to 16384)",
            ),
            ToolOption::new("falloff", falloff, "constant, linear or smooth"),
            ToolOption::new(
                "spatial",
                on_off(b.spatial),
                "on: 3D distance on the surface (paints walls and across tiles)",
            ),
            ToolOption::new("line_of_sight", on_off(b.line_of_sight), "on: only what the camera sees at the brush"),
            ToolOption::new(
                "autosew",
                on_off(b.autosew),
                "on: live sewing, the neighbours follow every sample and the stroke ends sewn",
            ),
            ToolOption::new(
                "selected_only",
                on_off(b.selected_only),
                "on: with displacements selected only those are painted (Hammer); off: everything under the brush",
            ),
            ToolOption::new(
                "continuous",
                on_off(b.continuous || self.relaxing()),
                "on: holding the button still keeps painting (Hammer++ continuous mouse); off: only while moving",
            ),
            ToolOption::new("alpha_effect", alpha, "alpha painting: add, remove, set or smooth"),
            ToolOption::new("alpha_value", num(b.alpha_value), "target alpha of alpha set (0 to 255)"),
            ToolOption::new(
                "alpha_strength",
                num(b.alpha_strength),
                "alpha add and remove speed per second (0 to 16384)",
            ),
            ToolOption::new(
                "blend_channel",
                b.blend_channel.map_or_else(|| "alpha".to_owned(), |ch| (ch + 1).to_string()),
                "alpha (two texture blend) or the texture 1 to 4 of a 4-way blend material",
            ),
            ToolOption::new(
                "sculpt",
                sculpt,
                "sculpt operation: push, carve, project, pinch, grab, inflate, flatten or relax (redistribute vertices on the original surface while protecting borders and triangle orientation). Ctrl inverts except relax",
            ),
            ToolOption::new("push_mode", push, "push offset: absolute (units) or adaptive (percent of the radius)"),
            ToolOption::new(
                "sculpt_bounds",
                bounds,
                "push and carve bounds limit: attenuated (each vertex moves once per stroke) or additive (keeps \
                 adding while the brush stays)",
            ),
            ToolOption::new(
                "falloff_position",
                num(s.falloff_position),
                "push: percent of the radius that gets the whole offset (0 to 100)",
            ),
            ToolOption::new(
                "falloff_final",
                num(s.falloff_final),
                "push: percent of the offset the falloff never goes below (0 to 100)",
            ),
            ToolOption::new("push_distance", num(s.distance), "push offset in units (0 to 4096)"),
            ToolOption::new("push_percent", num(s.percent), "push offset in percent of the radius (0 to 400)"),
            ToolOption::new("carve_depth", num(s.carve_depth), "carve depth in units (0 to 4096)"),
            ToolOption::new(
                "carve_profile",
                nums(&s.profile),
                "carve cross profile: v, u, flat or 5 depths from 0 to 1",
            ),
            ToolOption::new(
                "project_range",
                num(s.project_range),
                "project: brushes farther than this are ignored (1 to 16384)",
            ),
            ToolOption::new(
                "flatten_plane",
                if s.flatten_fixed { "fixed" } else { "average" },
                "flatten: average (plane of the vertices under the brush at each sample) or fixed (plane taken at the                  stroke start)",
            ),
            ToolOption::new(
                "flatten_strength",
                num(s.flatten_strength),
                "flatten speed, share of the distance to the plane closed per step (0 to 1)",
            ),
            ToolOption::new(
                "relax_strength",
                num(s.relax_strength),
                "relax speed (0 to 1), protects the original surface and pinned borders",
            ),
            ToolOption::new(
                "scale_factor",
                num(self.scale_factor),
                "factor of the Scale button of the panel (-16 to 16)",
            ),
            ToolOption::new("tag_kind", tag_kind, "tag mode: walkable, buildable or remove"),
            ToolOption::new(
                "tag_value",
                tag_value,
                "tag mode: on (forced on, removed), off (forced off, not removed) or auto (back to the slope)",
            ),
            ToolOption::new(
                "mask_grid",
                on_off(self.mask_grid),
                "on: hide the triangle grid of the selected displacements (Hammer's Grid mask)",
            ),
            ToolOption::new(
                "mask_select",
                on_off(self.mask_select),
                "on: do not tint the selected displacements (Hammer's Select mask)",
            ),
            ToolOption::new(
                "keep_shape",
                on_off(self.keep_shape),
                "on: changing the power resamples the shape of the displacements (default), off: rebuilds them flat",
            ),
        ];
        options.extend(self.terrain.values());
        options.extend(self.blend.values());
        options.extend(self.slope.values());
        options
    }

    pub(super) fn script_set_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        let v = value.trim().to_ascii_lowercase();
        let flag = match v.as_str() {
            "on" | "true" | "1" => Some(true),
            "off" | "false" | "0" => Some(false),
            _ => None,
        };
        let number = v.parse::<f64>().ok().filter(|n| n.is_finite());
        let within = |lo: f64, hi: f64| number.filter(|n| (lo..=hi).contains(n));
        let b = &mut self.brush;
        let done = match key {
            "full_displacement" => flag.map(|full| self.full_displacement = full).is_some(),
            "subdivision_shape" => match v.as_str() {
                "smooth" => Some(SubdivisionShape::Smooth),
                "sphere" => Some(SubdivisionShape::Sphere),
                _ => None,
            }
            .map(|shape| self.subdivision_shape = shape)
            .is_some(),
            "subdivision_amount" => within(0.0, 1.0).map(|amount| self.subdivision_amount = amount).is_some(),
            "mode" => match v.as_str() {
                "select" => Some(Interaction::Select),
                "geometry" | "paint" => Some(Interaction::Geometry),
                "alpha" => Some(Interaction::Alpha),
                "sculpt" => Some(Interaction::Sculpt),
                "tag" => Some(Interaction::Tag),
                _ => None,
            }
            .map(|m| self.interaction = m)
            .is_some(),
            "effect" => match v.as_str() {
                "raise" => Some(GeoEffect::Raise),
                "lower" => Some(GeoEffect::Lower),
                "smooth" => Some(GeoEffect::Smooth),
                "relax" => Some(GeoEffect::Relax),
                "set" | "raise_to" => Some(GeoEffect::Set),
                _ => None,
            }
            .map(|e| b.geo_effect = e)
            .is_some(),
            "sculpt" => match v.as_str() {
                "push" => Some(SculptOp::Push),
                "carve" => Some(SculptOp::Carve),
                "project" => Some(SculptOp::Project),
                "pinch" => Some(SculptOp::Pinch),
                "grab" => Some(SculptOp::Grab),
                "inflate" => Some(SculptOp::Inflate),
                "flatten" => Some(SculptOp::Flatten),
                "relax" => Some(SculptOp::Relax),
                _ => None,
            }
            .map(|o| b.sculpt.op = o)
            .is_some(),
            "axis" => match v.as_str() {
                "normal" => Some(PaintAxis::FaceNormal),
                "subdiv" => Some(PaintAxis::SubdivNormal),
                "view" => Some(PaintAxis::View),
                "view_flat" | "view_horizontal" => Some(PaintAxis::ViewFlat),
                "center" | "brush_center" => Some(PaintAxis::BrushCenter),
                "x" => Some(PaintAxis::X),
                "y" => Some(PaintAxis::Y),
                "z" => Some(PaintAxis::Z),
                "custom" => Some(PaintAxis::Custom),
                _ => None,
            }
            .map(|a| b.axis = a)
            .is_some(),
            "custom_axis" => match parse_nums(&v).as_deref() {
                Some([x, y, z]) if DVec3::new(*x, *y, *z).length_squared() > 1e-12 => {
                    b.custom_axis = DVec3::new(*x, *y, *z);
                    true
                }
                _ => false,
            },
            "falloff" => match v.as_str() {
                "constant" => Some(Falloff::Constant),
                "linear" => Some(Falloff::Linear),
                "smooth" => Some(Falloff::Smooth),
                _ => None,
            }
            .map(|f| b.falloff = f)
            .is_some(),
            "radius" => within(1.0, 8192.0).map(|n| b.radius = n).is_some(),
            "strength" => within(0.0, 8192.0).map(|n| b.strength = n).is_some(),
            "smooth" => within(0.0, 1.0).map(|n| b.smooth = n).is_some(),
            "target_height" => within(-16384.0, 16384.0).map(|n| b.target_height = n).is_some(),
            "spatial" => flag.map(|f| b.spatial = f).is_some(),
            "autosew" => flag.map(|f| b.autosew = f).is_some(),
            "line_of_sight" => flag.map(|f| b.line_of_sight = f).is_some(),
            "selected_only" => flag.map(|f| b.selected_only = f).is_some(),
            "continuous" => flag.map(|f| b.continuous = f).is_some(),
            "alpha_effect" => match v.as_str() {
                "add" => Some(AlphaEffect::Add),
                "remove" => Some(AlphaEffect::Remove),
                "set" => Some(AlphaEffect::Set),
                "smooth" => Some(AlphaEffect::Smooth),
                _ => None,
            }
            .map(|e| b.alpha_effect = e)
            .is_some(),
            "alpha_value" => within(0.0, 255.0).map(|n| b.alpha_value = n).is_some(),
            "alpha_strength" => within(0.0, 16384.0).map(|n| b.alpha_strength = n).is_some(),
            "blend_channel" => match v.as_str() {
                "alpha" | "none" => {
                    b.blend_channel = None;
                    true
                }
                _ => match v.parse::<usize>() {
                    Ok(ch @ 1..=hp_vmf::multiblend::CHANNELS) => {
                        b.blend_channel = Some(ch - 1);
                        true
                    }
                    _ => false,
                },
            },
            "push_mode" => match v.as_str() {
                "absolute" => Some(PushMode::Absolute),
                "adaptive" => Some(PushMode::Adaptive),
                _ => None,
            }
            .map(|m| b.sculpt.push_mode = m)
            .is_some(),
            "sculpt_bounds" => match v.as_str() {
                "attenuated" => Some(BoundsLimit::Attenuated),
                "additive" => Some(BoundsLimit::Additive),
                _ => None,
            }
            .map(|m| b.sculpt.bounds = m)
            .is_some(),
            "falloff_position" => within(0.0, 100.0).map(|n| b.sculpt.falloff_position = n).is_some(),
            "falloff_final" => within(0.0, 100.0).map(|n| b.sculpt.falloff_final = n).is_some(),
            "tag_kind" => match v.as_str() {
                "walkable" => Some(TagKind::Walkable),
                "buildable" => Some(TagKind::Buildable),
                "remove" => Some(TagKind::Remove),
                _ => None,
            }
            .map(|k| b.tag.kind = k)
            .is_some(),
            "tag_value" => match v.as_str() {
                "on" => Some(TagValue::On),
                "off" => Some(TagValue::Off),
                "auto" => Some(TagValue::Auto),
                _ => None,
            }
            .map(|k| b.tag.value = k)
            .is_some(),
            "mask_grid" => flag.map(|f| self.mask_grid = f).is_some(),
            "mask_select" => flag.map(|f| self.mask_select = f).is_some(),
            "keep_shape" => flag.map(|f| self.keep_shape = f).is_some(),
            "push_distance" => within(0.0, 4096.0).map(|n| b.sculpt.distance = n).is_some(),
            "push_percent" => within(0.0, 400.0).map(|n| b.sculpt.percent = n).is_some(),
            "carve_depth" => within(0.0, 4096.0).map(|n| b.sculpt.carve_depth = n).is_some(),
            "carve_profile" => match v.as_str() {
                "v" => Some(PROFILE_V),
                "u" => Some(PROFILE_U),
                "flat" => Some(PROFILE_FLAT),
                _ => match parse_nums(&v).as_deref() {
                    Some(p) if p.len() == 5 && p.iter().all(|x| (0.0..=1.0).contains(x)) => {
                        Some([p[0], p[1], p[2], p[3], p[4]])
                    }
                    _ => None,
                },
            }
            .map(|p| b.sculpt.profile = p)
            .is_some(),
            "project_range" => within(1.0, 16384.0).map(|n| b.sculpt.project_range = n).is_some(),
            "flatten_plane" => match v.as_str() {
                "average" => Some(false),
                "fixed" => Some(true),
                _ => None,
            }
            .map(|f| b.sculpt.flatten_fixed = f)
            .is_some(),
            "flatten_strength" => within(0.0, 1.0).map(|n| b.sculpt.flatten_strength = n).is_some(),
            "relax_strength" => within(0.0, 1.0).map(|n| b.sculpt.relax_strength = n).is_some(),
            "scale_factor" => within(-16.0, 16.0).map(|n| self.scale_factor = n).is_some(),
            "create" => match value.trim().parse::<u32>() {
                Ok(p @ 2..=4) => {
                    self.create(state, p);
                    true
                }
                _ => false,
            },
            "select" => match v.as_str() {
                "all" => {
                    self.select_all(state);
                    true
                }
                "adjacent" => {
                    self.select_adjacent(state);
                    true
                }
                "solids" => {
                    self.select_of_solids(state);
                    true
                }
                "none" => {
                    state.doc.clear_face_selection();
                    true
                }
                _ => false,
            },
            "cast_shadows" => flag.map(|f| super::commands::set_cast_shadows(state, f)).is_some(),
            "lightmap_scale" => match v.parse::<i32>() {
                Ok(n @ 1..=1024) => {
                    self.set_lightmap_scale(state, n);
                    true
                }
                _ => false,
            },
            "tag_selection" => {
                let (kind, value) = (self.brush.tag.kind, self.brush.tag.value);
                self.tag_selection(state, kind, value);
                true
            }
            _ => self.terrain.set(key, value) || self.blend.set(key, value) || self.slope.set(key, value),
        };
        if done {
            self.brush.sanitize();
            self.cursor = None;
        }
        done
    }

    pub(super) fn script_stroke(&mut self, state: &mut AppState, s: &ScriptedStroke) -> Result<StrokeOutcome, String> {
        if self.interaction == Interaction::Select {
            return Err("the displacement tool is in select mode: set its mode to geometry, alpha or sculpt".to_owned());
        }
        if s.points.is_empty() || s.points.len() > MAX_STROKE_POINTS {
            return Err(format!("a stroke has 1 to {MAX_STROKE_POINTS} points"));
        }
        if s.points.iter().any(|p| !p.is_finite()) {
            return Err("the points must be finite".to_owned());
        }
        if self.stroke.is_some() || state.doc.in_transaction() {
            return Err("another edit is in progress (a drag in a view?): try again".to_owned());
        }
        let filter = match &s.faces {
            Some(faces) => {
                if let Some(f) = faces.iter().find(|f| state.doc.face_side(**f).is_none_or(|side| side.disp.is_none()))
                {
                    return Err(format!("solid {} side {} is not a displacement", f.solid, f.side));
                }
                faces.clone()
            }
            None if self.brush.selected_only => selected_disp_faces(&state.doc),
            None => Vec::new(),
        };
        if self.disp_count(state) == 0 {
            return Err("the map has no displacement".to_owned());
        }
        if self.interaction == Interaction::Tag && s.smooth {
            return Err("smooth applies to sculpting only".to_owned());
        }
        let sculpting = self.interaction == Interaction::Sculpt || self.relaxing();
        let mods = Modifiers {
            shift: if sculpting { s.smooth } else { s.invert },
            ctrl: sculpting && s.invert,
            command: sculpting && s.invert,
            ..Modifiers::default()
        };
        let saved_eye = self.toward_eye;
        let saved_flat = self.toward_eye_flat;
        let saved_dt = state.frame.dt;
        self.toward_eye = s.forward.and_then(|f| (-f).try_normalize());
        self.toward_eye_flat = s.forward.and_then(|f| super::paint::flat_direction(-f));
        state.frame.dt = if s.seconds_per_sample.is_finite() && s.seconds_per_sample > 0.0 {
            s.seconds_per_sample
        } else {
            1.0 / 60.0
        };
        let limited = !filter.is_empty();
        self.begin_stroke_with(state, 0, filter, s.undo_label.clone());
        let mut out = StrokeOutcome::default();
        for p in &s.points {
            let Some(face) = nearest_disp_face(&state.doc, *p) else { continue };
            let n = self.paint_on(state, face, *p, mods, s.eye);
            if n > 0 {
                out.samples_painted += 1;
                out.vertices_changed += n;
            }
        }
        out.faces = self.stroke.as_ref().map(|st| st.painted.clone()).unwrap_or_default();
        self.end_stroke(state);
        state.frame.dt = saved_dt;
        self.toward_eye = saved_eye;
        self.toward_eye_flat = saved_flat;
        if out.vertices_changed == 0 {
            out.note = Some(
                if limited {
                    "nothing changed: the brush did not reach the displacements the stroke is limited to (the \
                     selected ones, or faces)"
                } else if self.interaction == Interaction::Sculpt && self.brush.sculpt.op == SculptOp::Project {
                    "nothing changed: project needs selected brushes to project onto"
                } else {
                    "nothing changed: no displacement under the brush (radius too small, points away from the \
                     surface, or line of sight)"
                }
                .to_owned(),
            );
        }
        Ok(out)
    }
}
