#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use egui::{Key, Modifiers, PointerButton};
use glam::DMat4;
use hp_app::{Action, AppState, Severity, Tool, ToolFlow, ToolId, ViewInput, ViewKind, t, tf};
use hp_editor::{ObjRef, SelectOp};
use hp_physics::{Simulation, Transform};

mod scene;
mod tr;

pub fn register_translations() {
    hp_app::i18n::register(tr::TR);
}
pub fn tools() -> Vec<Box<dyn Tool>> {
    vec![Box::new(PhysicsTool::default())]
}

fn engine_settings(settings: hp_app::settings::PhysicsSettings) -> hp_physics::Settings {
    hp_physics::Settings {
        gravity: settings.gravity,
        air_density: settings.air_density,
        time_scale: settings.time_scale,
    }
    .clamped()
}

struct Binding {
    id: u64,
    obj: ObjRef,
    initial: DMat4,
}

struct Session {
    simulation: Simulation,
    bindings: Vec<Binding>,
    ragdolls: Vec<Ragdoll>,
    skipped: Vec<String>,
}

struct Ragdoll {
    entity: u32,
    rig: Arc<hp_ragdoll::Rig>,
    bodies: Vec<u64>,
    scale: f64,
    initial: DMat4,
}

impl Ragdoll {
    fn poses(&self, simulation: &Simulation) -> Option<Vec<DMat4>> {
        self.bodies
            .iter()
            .map(|id| {
                let transform = simulation.transform(*id)?;
                Some(DMat4::from_scale_rotation_translation(
                    glam::DVec3::splat(self.scale),
                    transform.rotation,
                    transform.position,
                ))
            })
            .collect()
    }

    fn saved(&self, simulation: &Simulation) -> Option<(u32, String, String)> {
        let origin = simulation.transform(*self.bodies.first()?)?.position;
        let angles: Option<Vec<_>> = self
            .rig
            .body_order()
            .iter()
            .map(|index| {
                let rotation = simulation.transform(self.bodies[*index])?.rotation;
                Some(format!("{index},{}", hp_vmf::fmt_vec3(hp_geom::quat_to_angles(rotation))))
            })
            .collect();
        Some((self.entity, hp_vmf::fmt_vec3(origin), angles?.join(",")))
    }
}

impl Session {
    fn bone_poses(&self) -> Vec<(u32, hp_app::state::BoneMatrices)> {
        self.ragdolls
            .iter()
            .filter_map(|ragdoll| {
                let poses = ragdoll.poses(&self.simulation)?;
                Some((ragdoll.entity, Arc::new(ragdoll.rig.skin(ragdoll.initial, &poses))))
            })
            .collect()
    }

    fn object_count(&self) -> usize {
        self.bindings.len() + self.ragdolls.len()
    }
    fn transforms(&self) -> Vec<(ObjRef, DMat4)> {
        self.bindings
            .iter()
            .filter_map(|binding| {
                let Transform { position, rotation } = self.simulation.transform(binding.id)?;
                let delta = DMat4::from_rotation_translation(rotation, position) * binding.initial.inverse();
                Some((binding.obj, delta))
            })
            .collect()
    }
}

type DocumentKey = (u64, u64, u64);

fn document_key(state: &AppState) -> DocumentKey {
    (state.sidecar.serial(), state.doc.revision(), state.assets.generation())
}

struct Pending {
    result: mpsc::Receiver<Result<Session, String>>,
    cancel: Arc<AtomicBool>,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub struct PhysicsTool {
    session: Option<Session>,
    pending: Option<Pending>,
    key: Option<DocumentKey>,
    drag: Option<(usize, f64)>,
}

impl PhysicsTool {
    fn start(&mut self, state: &mut AppState, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        if let Some(session) = self.session.as_mut() {
            session.simulation.paused = false;
            ctx.request_repaint();
            return;
        }
        if state.doc.selection().objects.is_empty() {
            state.notify(Severity::Info, t!("physics.select"));
            return;
        }
        state.runtime.lab.player.active = false;
        let snapshot = scene::Snapshot::capture(state);
        self.key = Some(document_key(state));
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (send, result) = mpsc::sync_channel(1);
        let ctx = ctx.clone();
        rayon::spawn(move || {
            let result = snapshot.build(&worker_cancel);
            let _ = send.send(result);
            ctx.request_repaint();
        });
        self.pending = Some(Pending { result, cancel });
    }

    fn reset(&mut self, state: &mut AppState) {
        self.pending = None;
        self.session = None;
        self.key = None;
        self.drag = None;
        state.assets.set_physics_transforms(&[]);
        state.assets.set_physics_bones(&[]);
    }

    fn save(&mut self, state: &mut AppState) {
        let valid = self.key == Some(document_key(state));
        let transforms = self.session.as_ref().filter(|_| valid).map(Session::transforms).unwrap_or_default();
        let ragdolls: Vec<_> = self
            .session
            .as_ref()
            .filter(|_| valid)
            .into_iter()
            .flat_map(|session| session.ragdolls.iter().filter_map(|ragdoll| ragdoll.saved(&session.simulation)))
            .collect();
        self.reset(state);
        if transforms.is_empty() && ragdolls.is_empty() {
            return;
        }
        let result = state.doc.transaction(t!("shell.undo.physics"), |doc| {
            for (obj, delta) in transforms {
                if !delta.abs_diff_eq(DMat4::IDENTITY, 1e-9) {
                    if let ObjRef::Entity(id) = obj
                        && doc
                            .scene()
                            .entity(id)
                            .is_some_and(|n| n.entity.angles().is_none() && doc.scene().entity_solid_count(id) == 0)
                    {
                        doc.set_entity_key(&[id], "angles", "0 0 0")?;
                    }
                    doc.transform(&[obj], delta)?;
                }
            }
            for (id, origin, angles) in ragdolls {
                doc.set_entity_key(&[id], "origin", &origin)?;
                doc.set_entity_key(&[id], "angleOverride", &angles)?;
            }
            Ok::<_, hp_editor::EditError>(())
        });
        if let Err(error) = result {
            state.notify(Severity::Error, tf!("physics.failed", error));
        }
    }

    fn toggle_pause(&mut self, state: &mut AppState, ctx: &egui::Context) {
        if let Some(session) = self.session.as_mut() {
            session.simulation.paused = !session.simulation.paused;
            session.simulation.end_drag();
            self.drag = None;
            ctx.request_repaint();
        } else {
            self.start(state, ctx);
        }
    }
}

impl Tool for PhysicsTool {
    fn id(&self) -> ToolId {
        ToolId::Physics
    }
    fn wants_side_panel(&self) -> bool {
        true
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.save(state);
    }

    fn cancel(&mut self, state: &mut AppState) -> bool {
        if self.session.is_none() && self.pending.is_none() {
            return false;
        }
        self.reset(state);
        true
    }

    fn action(&mut self, state: &mut AppState, action: Action) -> bool {
        if action == Action::Undo && (self.session.is_some() || self.pending.is_some()) {
            self.reset(state);
            true
        } else {
            false
        }
    }

    fn update(&mut self, state: &mut AppState, ctx: &egui::Context) {
        if self.key.is_some_and(|key| key != document_key(state)) {
            self.reset(state);
            return;
        }
        if let Some(pending) = &self.pending {
            match pending.result.try_recv() {
                Ok(Ok(mut session)) => {
                    self.pending = None;
                    if session.object_count() == 0 {
                        state.notify(Severity::Warning, t!("physics.no_bodies"));
                        self.reset(state);
                    } else {
                        if !session.skipped.is_empty() {
                            state.notify(Severity::Warning, tf!("physics.skipped", session.skipped.join(", ")));
                        }
                        session.simulation.paused = false;
                        self.session = Some(session);
                    }
                }
                Ok(Err(error)) => {
                    self.reset(state);
                    if !error.is_empty() {
                        state.notify(Severity::Error, tf!("physics.failed", error));
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.reset(state);
                    state.notify(Severity::Error, t!("physics.worker_failed"));
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(session) = self.session.as_mut() {
            if self.drag.is_some() && !ctx.input(|i| i.pointer.button_down(PointerButton::Primary)) {
                session.simulation.end_drag();
                self.drag = None;
            }
            session.simulation.settings = engine_settings(state.settings.physics);
            session.simulation.advance(f64::from(state.frame.dt));
            state.assets.set_physics_transforms(&session.transforms());
            state.assets.set_physics_bones(&session.bone_poses());
            if !session.simulation.paused {
                ctx.request_repaint();
            }
        }
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.view.kind != ViewKind::Perspective || input.navigating {
            return ToolFlow::Ignored;
        }
        let button = PointerButton::Primary;
        if self.drag.is_some() && !input.response.ctx.input(|i| i.pointer.button_down(button)) {
            self.drag = None;
            if let Some(session) = &mut self.session {
                session.simulation.end_drag();
            }
        }
        let Some(pos) = input.pointer.filter(|pos| input.view.rect.contains(*pos)) else { return ToolFlow::Ignored };
        let (origin, direction) = input.view.ray(pos);
        let Some(session) = self.session.as_mut() else {
            if self.pending.is_none() && input.response.clicked_by(button) {
                let hit = state.view_pick(&hp_geom::Ray::new(origin, direction), &hp_editor::PickOptions::default());
                if let Some(hit) = hit {
                    let obj = match hit.obj {
                        ObjRef::Solid(id) => match state.doc.scene().solid(id).map(|n| n.owner) {
                            Some(hp_editor::Owner::Entity(owner)) => ObjRef::Entity(owner),
                            _ => hit.obj,
                        },
                        _ => hit.obj,
                    };
                    state.doc.select(&[obj], if input.modifiers.shift { SelectOp::Toggle } else { SelectOp::Replace });
                } else if !input.modifiers.shift {
                    state.doc.clear_selection();
                }
                return ToolFlow::Handled;
            }
            return ToolFlow::Ignored;
        };
        if session.simulation.paused {
            return ToolFlow::Ignored;
        }
        if let Some((view, distance)) = self.drag {
            if view == input.view.index {
                session.simulation.drag_to(origin + direction * distance);
                return ToolFlow::Handled;
            }
        } else if input.response.ctx.input(|i| i.pointer.button_pressed(button))
            && input.response.hovered()
            && let Some((id, point)) = session.simulation.ray(origin, direction, 65536.0)
            && session.simulation.begin_drag(id, point)
        {
            self.drag = Some((input.view.index, origin.distance(point)));
            return ToolFlow::Handled;
        }
        if input.response.clicked_by(PointerButton::Secondary) {
            session.simulation.shoot(origin, direction, state.settings.physics.shoot_speed);
            return ToolFlow::Handled;
        }
        if input.response.clicked_by(PointerButton::Middle) {
            let point = session
                .simulation
                .ray_hit(origin, direction, 65536.0)
                .map_or(origin + direction * 512.0, |hit| hit.point);
            session.simulation.explode(
                point,
                state.settings.physics.explosion_radius,
                state.settings.physics.explosion_speed,
            );
            return ToolFlow::Handled;
        }
        ToolFlow::Ignored
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if (self.session.is_some() || self.pending.is_some())
            && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z))
        {
            self.reset(state);
            return true;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space)) {
            self.toggle_pause(state, ctx);
            return true;
        }
        false
    }

    fn takes_right_drag(&self, kind: ViewKind, _modifiers: Modifiers) -> bool {
        kind == ViewKind::Perspective && self.session.as_ref().is_some_and(|s| !s.simulation.paused)
    }

    fn takes_middle_drag(&self, kind: ViewKind, modifiers: Modifiers) -> bool {
        self.takes_right_drag(kind, modifiers)
    }

    fn options_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let busy = self.pending.is_some();
        let exists = self.session.is_some();
        let paused = self.session.as_ref().is_some_and(|s| s.simulation.paused);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !busy,
                    egui::Button::new(if exists && !paused { t!("physics.pause") } else { t!("physics.play") }),
                )
                .clicked()
            {
                self.toggle_pause(state, ui.ctx());
            }
            if ui.add_enabled(exists && paused, egui::Button::new(t!("physics.step"))).clicked()
                && let Some(session) = self.session.as_mut()
            {
                session.simulation.settings = engine_settings(state.settings.physics);
                session.simulation.step();
                state.assets.set_physics_transforms(&session.transforms());
                state.assets.set_physics_bones(&session.bone_poses());
            }
            if ui.add_enabled(exists, egui::Button::new(t!("physics.keep"))).clicked() {
                self.save(state);
            }
            if ui.add_enabled(exists || busy, egui::Button::new(t!("physics.reset"))).clicked() {
                self.reset(state);
            }
        });
        if busy {
            ui.spinner();
            ui.label(t!("physics.loading"));
        }
        if let Some(session) = &self.session {
            ui.label(tf!("physics.bodies", session.object_count()));
        }
        ui.separator();
        ui.label(t!("physics.hint"));
        let settings = &mut state.settings.physics;
        let before = *settings;
        egui::Grid::new("physics_settings").num_columns(2).show(ui, |ui| {
            for (key, value, min, max, speed) in [
                ("physics.gravity", &mut settings.gravity, -10000.0, 10000.0, 10.0),
                ("physics.air", &mut settings.air_density, 0.0, 100.0, 0.1),
                ("physics.time", &mut settings.time_scale, 0.01, 4.0, 0.01),
                ("physics.shoot", &mut settings.shoot_speed, 1.0, 10000.0, 10.0),
                ("physics.explosion", &mut settings.explosion_speed, 1.0, 10000.0, 10.0),
                ("physics.radius", &mut settings.explosion_radius, 1.0, 16384.0, 8.0),
            ] {
                ui.label(hp_app::i18n::tr(key));
                ui.add(egui::DragValue::new(value).range(min..=max).speed(speed));
                ui.end_row();
            }
        });
        state.settings_dirty |= *settings != before;
    }

    fn status_hint(&self, _state: &AppState) -> Option<String> {
        Some(t!("physics.hint").to_owned())
    }
}

#[cfg(test)]
mod tests;
