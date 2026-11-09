pub mod camera;
mod collaboration;
pub mod fly_texture;
pub mod input;
pub mod io;
pub mod layers;
pub mod layout;
pub mod logical;
pub mod menu;
pub mod overlay;
pub mod paint;
pub mod params;
pub mod perf_ui;
pub mod pick;
pub mod placement;
pub mod player;
pub mod render_cache;
mod tr;
pub mod trains;
pub mod warp;
pub mod workflow;

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

use egui::{
    Align, Align2, Color32, CursorIcon, Id, Layout, Pos2, Rect, Sense, Stroke, StrokeKind, UiBuilder, Vec2, vec2,
};
use glam::{DVec2, DVec3};
use hp_app::dnd::DragPayload;
use hp_app::settings::{CtrlWheel, Disp2dSetting, Disp2dViewSetting, RenderModeSetting};
use hp_app::view::{Camera2d, QuadrantView, ViewInfo, ViewKind};
use hp_app::widgets::fmt_coord;
use hp_app::{Action, AppState, Severity, Tool, ToolFlow, ToolId, ViewInput, t, tf};
use hp_render::{Overlay, RenderStats};

use crate::camera::FlyInput;
use crate::input::{FlyKeys, FrameInput, MIDDLE, PRIMARY, SECONDARY};
use crate::layers::OverlayLayer;
use crate::layout::{clamp_split, pixel_align, quadrant_view, split_from_pos, visible_views};
use crate::logical::LogicalContent;
use crate::menu::{
    ENTRIES, MenuChoice, MenuContext, MenuEntry, OpenMenu, entry_enabled, submenu_actions, submenu_title,
};
use crate::params::{RenderConfig, view_info, view_options, view_params};
use crate::pick::Picker;
use crate::placement::Target;
use crate::render_cache::{RenderCache, ViewKey};

pub use tr::TR;

pub const CLICK_TRAVEL: f32 = 4.0;

const LIGHT_KEYS: [egui::Key; player::LISTED_LIGHTS] = [
    egui::Key::Num1,
    egui::Key::Num2,
    egui::Key::Num3,
    egui::Key::Num4,
    egui::Key::Num5,
    egui::Key::Num6,
    egui::Key::Num7,
    egui::Key::Num8,
    egui::Key::Num9,
];

const MIN_CAMERA_SPEED: f32 = 50.0;
const MAX_CAMERA_SPEED: f32 = 20000.0;
const HEADER_HEIGHT: f32 = 20.0;
const POINTER_ONLY: Sense = Sense::CLICK.union(Sense::DRAG);

pub fn register_translations() {
    hp_app::i18n::register(TR);
    for table in layers::TABLES {
        hp_app::i18n::register(table);
    }
    hp_app::i18n::register(workflow::TR);
    hp_app::i18n::register(perf_ui::TR);
    hp_app::i18n::register(fly_texture::TR);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Click {
    Menu,
    Tool,
    Nothing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Gesture {
    Look {
        view: usize,
        kind: ViewKind,
        button: usize,
        press: Pos2,
        travel: f32,
        pending: Vec2,
        looking: bool,
        flew: bool,
        click: Click,
    },
    Pan {
        view: usize,
        kind: ViewKind,
        button: usize,
        depth: f64,
        travel: f32,
    },
    Strafe {
        view: usize,
    },
}

impl Gesture {
    fn view(&self) -> usize {
        match *self {
            Gesture::Look { view, .. } | Gesture::Pan { view, .. } | Gesture::Strafe { view } => view,
        }
    }

    fn flies(&self) -> bool {
        matches!(self, Gesture::Look { kind: ViewKind::Perspective, .. })
    }

    fn space_flies_up(&self) -> bool {
        !matches!(self, Gesture::Look { button: PRIMARY, .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Mouselook {
    view: usize,
    anchor: Pos2,
}

struct Slot {
    index: usize,
    kind: ViewKind,
    rect: Rect,
    response: egui::Response,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PickKey {
    view: usize,
    pos: Pos2,
    cam: [f64; 5],
    revision: u64,
}

fn cam_key(state: &AppState) -> [f64; 5] {
    let c = &state.cameras.cam3d;
    [c.pos.x, c.pos.y, c.pos.z, c.yaw_deg, c.pitch_deg]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PreviewShape {
    #[default]
    None,
    Face,
    Box,
    Marker,
}

#[derive(Clone, Debug, Default)]
struct DropPreview {
    key: Option<(usize, Pos2, u64)>,
    target: Option<Target>,
    shape: PreviewShape,
    polygon: Vec<DVec3>,
    local_bounds: (DVec3, DVec3),
    origin: DVec3,
}

struct PendingPrefab {
    rx: Receiver<Result<String, String>>,
    path: PathBuf,
    target: Target,
    axes: Option<(usize, usize)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedKey {
    kind: ViewKind,
    tool: Option<(ToolId, u64)>,
    layers: Vec<Option<u64>>,
}

const RETAINED_KEEP: usize = 1 << 14;

pub struct Viewports {
    overlay: Overlay,
    layers: Vec<Box<dyn OverlayLayer>>,
    picker: Picker,
    retained: [Option<RetainedKey>; 4],
    retained_overlay: Overlay,
    layer_version_scratch: Vec<Option<u64>>,
    keys: FlyKeys,
    velocity: DVec3,
    gesture: Option<Gesture>,
    mouselook: Option<Mouselook>,
    warp: warp::CursorWarp,
    cursor_confined: bool,
    player_view: Option<usize>,
    player_props: player::PropBodies,
    player_doors: player::Doors,
    player_game_version: Option<u64>,
    player_io: Option<io::IoSim>,
    player_trains: Option<trains::Trains>,
    player_lights: player::PlayerLights,
    player_io_hidden: std::collections::HashSet<hp_vmf::Id>,
    player_world: Option<(std::sync::Arc<hp_editor::limits::world::BspWorld>, bool)>,
    player_hidden_world: usize,
    player_game_hidden: Vec<hp_editor::ObjRef>,
    player_clusters: player::ObjClusters,
    player_groups: Option<std::sync::Arc<Vec<bool>>>,
    player_flow_key: Option<(Vec<bool>, Vec<Option<hp_editor::limits::portals::ScreenRect>>)>,
    capture_view: Option<usize>,
    swallow: bool,
    menu: Option<OpenMenu>,
    hover_key: Option<PickKey>,
    hover_hit: Option<DVec3>,
    sel_key: Option<(u64, usize)>,
    sel_center: Option<DVec3>,
    preview: DropPreview,
    pending_prefab: Option<PendingPrefab>,
    frame: u64,
    rects: [Option<(ViewKind, Rect)>; 4],
    logical_rects: [Option<Rect>; 4],
    logical: Option<Box<dyn LogicalContent>>,
    header_rects: [Rect; 4],
    info_rects: [Rect; 4],
    perf_rects: [Rect; 4],
    stats: Option<(f64, RenderStats)>,
    render_cache: RenderCache,
}

impl Default for Viewports {
    fn default() -> Self {
        Self::new()
    }
}

impl Viewports {
    pub fn new() -> Self {
        Self {
            overlay: Overlay::default(),
            layers: {
                let mut layers = layers::default_layers();
                layers.extend(workflow::layers());
                layers
            },
            picker: Picker::default(),
            retained: [None, None, None, None],
            retained_overlay: Overlay::default(),
            layer_version_scratch: Vec::new(),
            keys: FlyKeys::default(),
            velocity: DVec3::ZERO,
            gesture: None,
            mouselook: None,
            warp: warp::CursorWarp::default(),
            cursor_confined: false,
            player_view: None,
            player_props: player::PropBodies::default(),
            player_doors: player::Doors::default(),
            player_game_version: None,
            player_io: None,
            player_trains: None,
            player_lights: player::PlayerLights::default(),
            player_io_hidden: std::collections::HashSet::new(),
            player_world: None,
            player_hidden_world: 0,
            player_game_hidden: Vec::new(),
            player_clusters: player::ObjClusters::default(),
            player_groups: None,
            player_flow_key: None,
            capture_view: None,
            swallow: false,
            menu: None,
            hover_key: None,
            hover_hit: None,
            sel_key: None,
            sel_center: None,
            preview: DropPreview::default(),
            pending_prefab: None,
            frame: 0,
            rects: [None; 4],
            logical_rects: [None; 4],
            logical: None,
            header_rects: [Rect::NOTHING; 4],
            info_rects: [Rect::NOTHING; 4],
            perf_rects: [Rect::NOTHING; 4],
            stats: None,
            render_cache: RenderCache::default(),
        }
    }

    pub fn captures_keyboard(&self) -> bool {
        self.mouselook.is_some() || self.gesture.is_some_and(|g| g.flies()) || self.player_view.is_some()
    }

    pub fn is_navigating(&self) -> bool {
        self.mouselook.is_some() || self.gesture.is_some() || self.velocity != DVec3::ZERO || self.player_view.is_some()
    }

    pub fn is_mouselook(&self) -> bool {
        self.mouselook.is_some()
    }

    pub fn is_context_menu_open(&self) -> bool {
        self.menu.is_some()
    }

    pub fn view_rects(&self) -> [Option<(ViewKind, Rect)>; 4] {
        self.rects
    }

    pub fn logical_rects(&self) -> [Option<Rect>; 4] {
        self.logical_rects
    }

    pub fn hovered_view(&self, state: &AppState, ctx: &egui::Context) -> Option<(ViewInfo, Pos2)> {
        let index = state.ui.hovered_view?;
        let (kind, rect) = self.rects.get(index).copied().flatten()?;
        let pos = ctx.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p))?;
        let cfg = RenderConfig::from_state(state);
        Some((view_info(index, kind, rect, &state.cameras, &cfg, ctx.pixels_per_point()), pos))
    }

    pub fn set_logical_content(&mut self, content: Box<dyn LogicalContent>) {
        self.logical = Some(content);
    }

    pub fn add_layer(&mut self, layer: Box<dyn OverlayLayer>) {
        self.layers.push(layer);
    }

    pub fn pick_hint(&self, state: &AppState) -> Option<String> {
        pick::hint(state, self.picker.faces().len())
    }

    pub fn layer_names(&self) -> Vec<&'static str> {
        self.layers.iter().map(|l| l.name()).collect()
    }

    pub fn view_size(&self, kind: ViewKind) -> Option<Vec2> {
        self.rects.iter().flatten().find(|(k, _)| *k == kind).map(|(_, r)| r.size())
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, tool: &mut dyn Tool) {
        self.frame = self.frame.wrapping_add(1);
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        let area = ui.available_rect_before_wrap();
        ui.allocate_rect(area, Sense::hover());
        ui.painter().rect_filled(area, 0, ui.visuals().extreme_bg_color);

        self.poll_prefab(state);
        self.picker.sync(state);
        for layer in &mut self.layers {
            layer.prepare(state);
        }
        self.keys.layout = state.settings.view.keyboard_layout;
        let input = FrameInput::gather_for(&ctx, self.keys.layout);
        self.keys.update_from_ctx(&ctx, &input);
        let cfg = RenderConfig::from_state(state);

        if state.ui.maximized_view.is_some_and(|m| m >= 4) {
            state.ui.maximized_view = None;
        }
        let split_x = clamp_split(state.settings.ui.split_x);
        let split_y = clamp_split(state.settings.ui.split_y);
        let (rects, maximized) = visible_views(area, split_x, split_y, state.ui.maximized_view);

        let mut slots: Vec<Slot> = Vec::with_capacity(4);
        self.rects = [None; 4];
        self.logical_rects = [None; 4];
        state.ui.hovered_logical = None;
        for (index, r) in rects.iter().enumerate() {
            let Some(r) = r else {
                self.header_rects[index] = Rect::NOTHING;
                self.info_rects[index] = Rect::NOTHING;
                self.perf_rects[index] = Rect::NOTHING;
                continue;
            };
            let Some(kind) = quadrant_view(&state.settings.ui.quadrants, index).kind() else {
                self.header_rects[index] = Rect::NOTHING;
                self.info_rects[index] = Rect::NOTHING;
                self.perf_rects[index] = Rect::NOTHING;
                let rect = pixel_align(*r, ppp, false);
                self.logical_rects[index] = Some(rect);
                logical::view_ui(ui, state, self.logical.as_deref_mut(), index, rect, maximized);
                continue;
            };
            let rect = pixel_align(*r, ppp, kind.is_2d());
            let response = ui.interact(rect, ui.scope_id().with(("hp_viewport", index)), POINTER_ONLY);
            self.rects[index] = Some((kind, rect));
            slots.push(Slot { index, kind, rect, response });
        }
        if !maximized {
            splitters(ui, state, area, split_x, split_y);
        }

        let menus_were_open = self.menu.is_some() || egui::Popup::is_any_open(&ctx);
        if menus_were_open && (input.pressed[PRIMARY] || input.pressed[MIDDLE]) {
            self.swallow = true;
        }
        let dnd_active = egui::DragAndDrop::has_any_payload(&ctx);
        let on_header = |i: usize| {
            input.pointer.is_some_and(|p| {
                self.header_rects[i].contains(p) || self.info_rects[i].contains(p) || self.perf_rects[i].contains(p)
            })
        };
        let hovered = slots
            .iter()
            .find(|s| (s.response.hovered() && !on_header(s.index)) || (dnd_active && s.response.contains_pointer()))
            .map(|s| s.index);
        state.ui.hovered_view = hovered.or(self.mouselook.map(|m| m.view));
        if let Some(h) = hovered
            && input.pressed.iter().any(|&p| p)
        {
            state.ui.focused_view = Some(h);
            ctx.memory_mut(|m| m.stop_text_input());
        }

        let tool_id = tool.id();
        let nav_view = self.navigate(&ctx, state, &input, &slots, hovered, &cfg, ppp, dnd_active, tool);
        let confine = input.focused
            && (self.mouselook.is_some()
                || self.player_view.is_some()
                || matches!(self.gesture, Some(Gesture::Look { looking: true, .. })));
        if confine != self.cursor_confined {
            self.cursor_confined = confine;
            let mode = if confine { egui::CursorGrab::Confined } else { egui::CursorGrab::None };
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorGrab(mode));
        }
        if nav_view.is_none() && !dnd_active && !menus_were_open {
            self.key_navigation(&ctx, state, &input, &slots, hovered);
        }
        if self.menu.is_none() && !menus_were_open {
            self.picker.keys(&ctx, state);
        }
        let nav_capture = nav_view.is_none()
            || (self.mouselook.is_some_and(|m| Some(m.view) == hovered) && tool_works_while_navigating(tool.id()));
        if self.capture_view.is_none()
            && !self.swallow
            && input.pressed[PRIMARY]
            && nav_capture
            && let Some(h) = hovered
        {
            self.capture_view = Some(h);
        }

        let mut any_hover_world = false;
        for slot in &slots {
            any_hover_world |=
                self.view_ui(ui, &ctx, state, tool, slot, &cfg, &input, hovered, nav_view, maximized, ppp);
        }
        if nav_view.is_none() && !any_hover_world {
            state.hover_world = None;
        }
        self.drop_stale_retained(state, tool_id, &slots);

        self.paint_player_counters(ui, state);

        self.context_menu_ui(ui, &ctx, state, tool);

        if !input.down.iter().any(|&d| d) {
            self.capture_view = None;
            self.swallow = false;
        }
        if self.is_navigating() {
            ctx.request_repaint();
        }
    }

    fn player_frame(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        input: &FrameInput,
        slots: &[Slot],
        hovered: Option<usize>,
        dt: f64,
    ) -> Option<usize> {
        let three_d = |i: usize| slots.iter().find(|s| s.index == i && s.kind == ViewKind::Perspective);
        if std::mem::take(&mut state.runtime.lab.player.start_request) {
            let view = hovered
                .and_then(three_d)
                .or_else(|| state.ui.focused_view.and_then(three_d))
                .or_else(|| slots.iter().find(|s| s.kind == ViewKind::Perspective))
                .map(|s| s.index);
            match view {
                Some(v) => {
                    let (feet, yaw) = player::start_position(state);
                    let p = &mut state.runtime.lab.player;
                    p.saved_camera = Some(state.cameras.cam3d);
                    p.feet = feet;
                    p.velocity = DVec3::ZERO;
                    p.on_ground = false;
                    p.crouched = false;
                    p.duck = 0.0;
                    p.active = true;
                    state.cameras.cam3d.yaw_deg = yaw;
                    state.cameras.cam3d.pitch_deg = 0.0;
                    self.player_view = Some(v);
                    self.mouselook = None;
                    fly_texture::leave(state);
                    self.gesture = None;
                    self.velocity = DVec3::ZERO;
                    self.keys.clear();
                    state.ui.focused_view = Some(v);
                    state.notify(Severity::Info, t!("viewport.player.started"));
                }
                None => state.notify(Severity::Warning, t!("viewport.player.no_3d_view")),
            }
        }
        if let Some(place) = state.runtime.lab.player.place_request.take()
            && self.player_view.is_some()
        {
            let p = &mut state.runtime.lab.player;
            if let Some(feet) = place.feet {
                p.feet = feet;
            } else if let Some(eye) = place.eye {
                p.feet = eye - DVec3::new(0.0, 0.0, player::EYE);
            }
            if place.feet.is_some() || place.eye.is_some() {
                p.velocity = DVec3::ZERO;
                p.on_ground = false;
            }
            if let Some(noclip) = place.noclip {
                p.noclip = noclip;
            }
            if let Some(yaw) = place.yaw {
                state.cameras.cam3d.yaw_deg = yaw;
            }
            if let Some(pitch) = place.pitch {
                state.cameras.cam3d.pitch_deg = pitch.clamp(-89.0, 89.0);
            }
        }
        let v = self.player_view?;
        if dt > 0.2 {
            log::info!("player view: a frame took {:.0} ms", dt * 1000.0);
        }
        let leave = !state.runtime.lab.player.active || three_d(v).is_none() || input.escape_pressed;
        if leave {
            if input.escape_pressed {
                input::consume_escape(ctx);
            }
            state.runtime.lab.player.active = false;
            if let Some(cam) = state.runtime.lab.player.saved_camera.take() {
                state.cameras.cam3d = cam;
            }
            state.assets.set_player_model(None);
            self.player_doors = player::Doors::default();
            state.assets.set_play_transforms(&[]);
            state.runtime.health.play_from = None;
            state.runtime.health.play_view = None;
            state.runtime.health.play_open_doors.clear();
            state.assets.set_game_hidden(&[]);
            self.player_groups = None;
            self.player_flow_key = None;
            self.player_game_version = None;
            self.player_io = None;
            self.player_trains = None;
            state.runtime.lab.player.trains.clear();
            state.runtime.lab.player.train_requests.clear();
            self.player_lights = player::PlayerLights::default();
            state.assets.set_light_styles(&[]);
            state.assets.set_projector_overrides(&[]);
            state.runtime.lab.player.light_styles = Default::default();
            state.runtime.lab.player.light_requests.clear();
            self.player_io_hidden.clear();
            state.assets.set_play_hidden(&[]);
            state.assets.set_play_world(None, None);
            self.player_world = None;
            self.player_hidden_world = 0;
            self.player_game_hidden.clear();
            state.runtime.health.play_portal_states.clear();
            self.player_view = None;
            self.keys.clear();
            return None;
        }
        let (noclip, third, wire, next_model, hide, use_key, tools, world_key, train_key) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::V),
                i.consume_key(egui::Modifiers::NONE, egui::Key::C),
                i.consume_key(egui::Modifiers::NONE, egui::Key::X),
                i.consume_key(egui::Modifiers::NONE, egui::Key::M),
                i.consume_key(egui::Modifiers::NONE, egui::Key::H),
                i.consume_key(egui::Modifiers::NONE, egui::Key::E),
                i.consume_key(egui::Modifiers::NONE, egui::Key::N),
                i.consume_key(egui::Modifiers::NONE, egui::Key::B),
                i.consume_key(egui::Modifiers::NONE, egui::Key::T),
            )
        });
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::L)) {
            let r = &mut state.settings.render;
            r.player_compiled_lighting = !r.player_compiled_lighting;
            state.settings_dirty = true;
        }
        let next = next_model.then(|| player::next_player_model(state, &state.runtime.lab.player.model)).flatten();
        {
            let p = &mut state.runtime.lab.player;
            p.noclip ^= noclip;
            p.third_person ^= third;
            if wire {
                p.wireframe = (p.wireframe + 1) % 4;
            }
            if p.wireframe > 0 {
                state.assets.set_model_lods(true);
            }
            p.hide_counters ^= hide;
            p.hide_tools ^= tools;
            p.editor_world ^= world_key;
            if let Some(m) = next {
                p.model = m;
                p.third_person = true;
            }
            if p.third_person && input.wheel != 0.0 {
                p.camera_distance = player::zoom_camera(p.camera_distance, input.wheel);
            }
        }
        let fly = self.keys.fly_input_with(input.modifiers, true);
        let m = player::Move {
            forward: fly.forward,
            right: fly.right,
            jump: ctx.input(|i| i.key_down(egui::Key::Space)) && input.focused,
            crouch: input.modifiers.ctrl,
            run: input.modifiers.shift,
        };
        let mut p = state.runtime.lab.player.clone();
        let cam = state.cameras.cam3d;
        let sim = self.player_io.get_or_insert_with(|| io::IoSim::new(state));
        let trains = self.player_trains.get_or_insert_with(|| trains::Trains::new(state));
        for (name, start) in std::mem::take(&mut p.train_requests) {
            trains.request(name.as_deref(), start);
        }
        if train_key && !trains.is_empty() {
            trains.toggle_all();
        }
        if use_key {
            let look = state.cameras.cam3d.forward();
            if let Some(id) = self.player_doors.use_pick(state, &self.player_props, trains, player::eye(&p), look) {
                if trains.use_entity(id) == Some(false) {
                    sim.note_no_control(state, id);
                }
                sim.use_entity(state, &mut self.player_doors, id, p.feet);
            }
        }
        let mut transforms = self.player_doors.animate(dt);
        let height = if p.crouched { player::CROUCH_HEIGHT } else { player::HEIGHT };
        sim.step(state, &mut self.player_doors, p.feet, height, dt);
        for (id, input, param) in sim.take_train_inputs() {
            trains.input(id, &input, &param);
        }
        if let Some((to, yaw)) = sim.teleport.take() {
            p.feet = to;
            p.velocity = DVec3::ZERO;
            if let Some(yaw) = yaw {
                state.cameras.cam3d.yaw_deg = yaw;
            }
        }
        let world = state.runtime.health.world.clone().filter(|_| !state.runtime.lab.player.editor_world);
        trains.set_world(world.as_ref());
        let riding = if p.on_ground && !p.noclip {
            trains.standing_on(p.feet, Some(sim.hidden()), Some(&self.player_props))
        } else {
            None
        };
        trains.step(dt, sim.hidden());
        if !p.noclip {
            trains.move_player(riding, &mut p.feet, &mut state.cameras.cam3d.yaw_deg, height, Some(sim.hidden()));
        }
        for (id, output) in trains.take_fired() {
            sim.fire(state, id, output);
        }
        let moved = trains.transforms();
        sim.set_moved(&moved);
        transforms.extend(moved);
        p.trains = trains.report();
        if world.as_ref().map(std::sync::Arc::as_ptr)
            != self.player_world.as_ref().map(|(w, _)| std::sync::Arc::as_ptr(w))
        {
            self.player_world = world.clone().map(|w| (w, player::world_stale(state)));
        }
        let version = state.runtime.health.play_version;
        let flow_key = player::flow_key(state.runtime.health.play_flow.as_deref());
        let game_changed = self.player_game_version != Some(version) || self.player_flow_key != flow_key;
        if game_changed || self.player_groups.is_none() {
            self.player_flow_key = flow_key;
            let rt = &state.runtime.health;
            let areas = rt.vis.as_ref().map(|v| v.cluster_areas()).unwrap_or_default();
            let flow = rt.play_flow.as_deref().filter(|f| f.own_area.is_some());
            self.player_groups = world.as_ref().map(|w| {
                let drawn = player::world_groups_drawn(w, rt.play_visible.as_deref(), &areas, flow);
                match &self.player_groups {
                    Some(old) if **old == drawn => std::sync::Arc::clone(old),
                    _ => std::sync::Arc::new(drawn),
                }
            });
        }
        let light_requests = std::mem::take(&mut p.light_requests);
        let light_report = match &world {
            Some(w) => {
                use hp_editor::limits::world_light::LightCmd;
                let table = &w.light.styles;
                let mut cmds = sim.take_light_inputs();
                if !table.lights.is_empty() {
                    let keys = ctx.input_mut(|i| {
                        (0..player::LISTED_LIGHTS)
                            .filter(|k| i.consume_key(egui::Modifiers::NONE, LIGHT_KEYS[*k]))
                            .collect::<Vec<usize>>()
                    });
                    let switched = keys.iter().filter_map(|k| table.lights.get(*k));
                    cmds.extend(switched.map(|l| (l.name.clone(), LightCmd::Toggle)));
                }
                cmds.extend(light_requests.into_iter().map(|(name, on)| {
                    let cmd = match on {
                        Some(true) => LightCmd::On,
                        Some(false) => LightCmd::Off,
                        None => LightCmd::Toggle,
                    };
                    (name, cmd)
                }));
                let values = self.player_lights.step(table, std::sync::Arc::as_ptr(w) as usize, dt, cmds);
                state.assets.set_light_styles(&values);
                self.player_lights.report(table)
            }
            None => {
                sim.take_light_inputs();
                hp_app::runtime::LightStyleReport::default()
            }
        };
        state.assets.set_projector_overrides(&sim.projector_overrides());
        state.assets.set_play_world(world.clone(), self.player_groups.clone());
        state.runtime.health.play_portal_states =
            sim.portals.iter().map(|(name, open)| (name.clone(), *open)).collect();
        state.assets.set_play_transforms(&transforms);
        state.runtime.health.play_from = Some(state.cameras.cam3d.pos);
        let aspect = self
            .rects
            .get(v)
            .copied()
            .flatten()
            .map_or(16.0 / 9.0, |(_, r)| f64::from(r.width() / r.height().max(1.0)));
        state.runtime.health.play_view =
            Some(player::view_cone(&state.cameras.cam3d, f64::from(state.settings.view.fov_deg), aspect));
        state.runtime.health.play_open_doors = self.player_doors.open_names(state);
        if game_changed {
            self.player_game_version = Some(version);
            self.player_clusters.update(state);
            self.player_game_hidden = player::game_hidden(state, &self.player_clusters);
            let scene = state.doc.scene();
            self.player_game_hidden.retain(|o| match *o {
                hp_editor::ObjRef::Solid(s) => match scene.solid(s).map(|n| n.owner) {
                    Some(hp_editor::Owner::Entity(e)) => !trains.carries(e),
                    _ => true,
                },
                hp_editor::ObjRef::Entity(e) => !trains.carries(e),
            });
            state.assets.set_game_hidden(&self.player_game_hidden);
        }
        let io_changed = sim.hidden() != &self.player_io_hidden;
        let world_addr = world.as_ref().map_or(0, |w| std::sync::Arc::as_ptr(w) as usize);
        if io_changed || game_changed || self.player_hidden_world != world_addr {
            self.player_io_hidden = sim.hidden().clone();
            self.player_hidden_world = world_addr;
            let scene = state.doc.scene();
            let mut objs: Vec<hp_editor::ObjRef> = self.player_game_hidden.clone();
            let none: Vec<hp_vmf::Id> =
                scene.entities().iter().filter(|(_, n)| player::render_none(&n.entity)).map(|(id, _)| *id).collect();
            for id in self.player_io_hidden.iter().chain(&none) {
                if scene.is_brush_entity(*id) {
                    objs.extend(
                        scene
                            .solids()
                            .iter()
                            .filter(|(_, n)| n.owner == hp_editor::Owner::Entity(*id))
                            .map(|(s, _)| hp_editor::ObjRef::Solid(*s)),
                    );
                } else {
                    objs.push(hp_editor::ObjRef::Entity(*id));
                }
            }
            if let Some(w) = &world {
                objs.extend(player::world_solids(state));
                objs.extend(player::compiled_replaced_entities(state, w));
            }
            state.assets.set_play_hidden(&objs);
        }
        self.player_props.update(state, p.feet, &self.player_doors, trains, &self.player_io_hidden);
        let c = player::DocCollider {
            state,
            props: Some(&self.player_props),
            except: None,
            doors: Some(&self.player_doors),
            hidden: Some(&self.player_io_hidden),
            trains: Some(&*trains),
        };
        let pushes = player::step(&mut p, m, cam.yaw_deg, cam.pitch_deg, dt, &c);
        if let Some(yaw) = player::teleport(state, &mut p, Some(&*trains)) {
            state.cameras.cam3d.yaw_deg = yaw;
        }
        state.cameras.cam3d.pos = player::camera_position(state, &p, Some(&*trains));
        let pose = player::model_pose(&mut p, cam.yaw_deg, dt);
        let third = p.third_person;
        p.light_styles = light_report;
        state.runtime.lab.player = p;
        state.assets.set_player_model(third.then_some(pose));
        if !pushes.is_empty() {
            player::push_props(state, &self.player_props, Some(&*trains), &pushes);
        }
        ctx.request_repaint();
        Some(v)
    }

    fn paint_player_counters(&self, ui: &egui::Ui, state: &AppState) {
        let Some(v) = self.player_view else { return };
        let Some((_, rect)) = self.rects.get(v).copied().flatten() else { return };
        let io_log: Vec<String> = self.player_io.as_ref().map(|s| s.log().cloned().collect()).unwrap_or_default();
        let world = self.player_world.as_ref().map(|(w, stale)| {
            let aspect = f64::from(rect.width() / rect.height().max(1.0));
            let drawn = self.player_groups.as_deref().map_or(&[][..], Vec::as_slice);
            player::world_counts(state, w, aspect, drawn, *stale)
        });
        let lines = player::counter_lines(state, state.frame.fps, v, &io_log, world);
        let painter = ui.painter_at(rect);
        let size = (rect.width() / 75.0).clamp(9.0, 12.0);
        let font = egui::FontId::monospace(size);
        let wrap = (rect.width() * 0.5).clamp(140.0, 560.0);
        let mut y = rect.top() + HEADER_HEIGHT + 4.0;
        let galleys: Vec<_> =
            lines.into_iter().map(|l| painter.layout(l, font.clone(), egui::Color32::from_gray(235), wrap)).collect();
        let w = galleys.iter().map(|g| g.size().x).fold(0.0f32, f32::max);
        let h: f32 = galleys.iter().map(|g| g.size().y + 1.0).sum();
        painter.rect_filled(
            Rect::from_min_size(egui::pos2(rect.left() + 4.0, y - 3.0), egui::vec2(w + 10.0, h + 6.0)),
            3.0,
            egui::Color32::from_black_alpha(150),
        );
        for g in galleys {
            let gh = g.size().y;
            painter.galley(egui::pos2(rect.left() + 9.0, y), g, egui::Color32::WHITE);
            y += gh + 1.0;
        }
        if !state.runtime.lab.player.third_person {
            let c = rect.center();
            let stroke = egui::Stroke::new(1.5, egui::Color32::from_white_alpha(200));
            painter.line_segment([c - egui::vec2(6.0, 0.0), c + egui::vec2(6.0, 0.0)], stroke);
            painter.line_segment([c - egui::vec2(0.0, 6.0), c + egui::vec2(0.0, 6.0)], stroke);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn navigate(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        input: &FrameInput,
        slots: &[Slot],
        hovered: Option<usize>,
        cfg: &RenderConfig,
        ppp: f32,
        dnd_active: bool,
        tool: &mut dyn Tool,
    ) -> Option<usize> {
        let dt = if state.frame.dt > 0.0 { f64::from(state.frame.dt) } else { f64::from(ctx.input(|i| i.stable_dt)) };
        let text_focus = ctx.text_edit_focused();
        let sens = f64::from(state.settings.view.mouse_sensitivity);
        let invert_y = state.settings.view.invert_mouse_y;
        let invert_x = state.settings.workflow.invert_mouse_x;
        let scheme = state.settings.workflow.nav_scheme;
        let slot_of = |i: usize| slots.iter().find(|s| s.index == i);
        let look = |cam: &mut hp_app::view::Camera3d, d: Vec2| {
            camera::mouselook(cam, DVec2::new(f64::from(d.x), f64::from(d.y)), sens, invert_x, invert_y);
        };

        if let Some(v) = self.player_frame(ctx, state, input, slots, hovered, dt) {
            let Some(s) = slot_of(v) else { return Some(v) };
            look(&mut state.cameras.cam3d, input.look_delta());
            ctx.set_cursor_icon(CursorIcon::None);
            if self.warp.request(input.focused, input.pointer, input.motion.is_some(), s.rect.center()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CursorPosition(s.rect.center()));
            }
            return Some(v);
        }

        if let Some(ml) = self.mouselook {
            let valid = slot_of(ml.view).is_some_and(|s| s.kind == ViewKind::Perspective);
            if !valid || input.escape_pressed || input.z_exit_pressed || !input.focused {
                if input.escape_pressed {
                    input::consume_escape(ctx);
                }
                if input.z_exit_pressed {
                    input::consume_z(ctx, self.keys.layout, true);
                }
                self.mouselook = None;
                self.keys.clear();
            } else {
                look(&mut state.cameras.cam3d, input.look_delta());
                ctx.set_cursor_icon(CursorIcon::None);
                if self.warp.request(input.focused, input.pointer, input.motion.is_some(), ml.anchor) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CursorPosition(ml.anchor));
                }
                fly_texture::frame(ctx, state, input, ml.view);
                if input.pressed[PRIMARY]
                    && hovered == Some(ml.view)
                    && !self.swallow
                    && self.menu.is_none()
                    && !dnd_active
                    && pick::pending(state).is_none()
                    && !tool_works_while_navigating(tool.id())
                    && let Some(s) = slot_of(ml.view)
                {
                    let view = view_info(s.index, s.kind, s.rect, &state.cameras, cfg, ppp);
                    tool.click_while_flying(state, &view, s.rect.center(), input.modifiers);
                }
            }
        } else if input.z_pressed && !text_focus && self.gesture.is_none() && !state.runtime.selection.modal_transform {
            let candidate = match hovered {
                Some(h) => Some(h),
                None => state.ui.focused_view,
            };
            let target = candidate.and_then(slot_of).filter(|s| s.kind == ViewKind::Perspective);
            if let Some(s) = target {
                self.mouselook = Some(Mouselook { view: s.index, anchor: s.rect.center() });
                self.gesture = None;
                state.ui.focused_view = Some(s.index);
                input::consume_z(ctx, self.keys.layout, false);
                self.keys.clear();
                self.warp.reset();
                if self.warp.request(input.focused, input.pointer, input.motion.is_some(), s.rect.center()) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CursorPosition(s.rect.center()));
                }
            }
        }

        if self.mouselook.is_none() {
            fly_texture::leave(state);
        }

        let mut ended = None;
        match self.gesture {
            Some(Gesture::Look {
                view,
                kind,
                button,
                press,
                mut travel,
                mut pending,
                mut looking,
                mut flew,
                click,
            }) => {
                let both = input.down[PRIMARY] && input.down[SECONDARY];
                if scheme.hammer_gestures()
                    && kind == ViewKind::Perspective
                    && button == SECONDARY
                    && both
                    && self.capture_view.is_none()
                    && !self.swallow
                {
                    self.gesture = Some(Gesture::Strafe { view });
                } else if !input.down[button] {
                    let is_click = !looking && !flew && travel < CLICK_TRAVEL && !self.swallow;
                    self.gesture = None;
                    match click {
                        Click::Menu if is_click && tool_takes_right_click(tool.id(), kind, input.modifiers) => {}
                        Click::Menu => {
                            if is_click {
                                self.open_menu(state, slot_of(view), kind, press, cfg, ppp);
                            }
                            ended = Some(view);
                        }
                        Click::Tool if is_click && slot_of(view).is_some_and(|s| s.response.middle_clicked()) => {}
                        Click::Tool | Click::Nothing => ended = Some(view),
                    }
                } else {
                    travel += input.delta.length();
                    if kind == ViewKind::Perspective {
                        let d = input.look_delta();
                        if looking {
                            look(&mut state.cameras.cam3d, d);
                        } else {
                            pending += d;
                            if travel >= CLICK_TRAVEL {
                                looking = true;
                                look(&mut state.cameras.cam3d, pending);
                                pending = Vec2::ZERO;
                            }
                        }
                        if looking {
                            ctx.set_cursor_icon(CursorIcon::None);
                            if self.warp.request(input.focused, input.pointer, input.motion.is_some(), press) {
                                ctx.send_viewport_cmd(egui::ViewportCommand::CursorPosition(press));
                            }
                        }
                        flew |= !self.keys.fly_input_with(input.modifiers, button != PRIMARY).is_idle();
                    }
                    self.gesture =
                        Some(Gesture::Look { view, kind, button, press, travel, pending, looking, flew, click });
                }
            }
            Some(Gesture::Strafe { view }) => {
                if !(input.down[PRIMARY] && input.down[SECONDARY]) {
                    self.gesture = None;
                    ended = Some(view);
                    self.swallow = true;
                } else {
                    let speed = f64::from(state.settings.view.camera_speed).max(1.0);
                    let fast = f64::from(state.settings.view.camera_fast_multiplier).max(1.0);
                    let step = speed * camera::STRAFE_FRACTION * if input.modifiers.shift { fast } else { 1.0 };
                    let d = input.look_delta();
                    camera::strafe_3d(&mut state.cameras.cam3d, DVec2::new(f64::from(d.x), f64::from(d.y)), step);
                    ctx.set_cursor_icon(CursorIcon::AllScroll);
                }
            }
            Some(Gesture::Pan { view, kind, button, depth, travel }) => {
                if !input.down[button] {
                    self.gesture = None;
                    let click = button == MIDDLE
                        && travel < CLICK_TRAVEL
                        && !self.swallow
                        && slot_of(view).is_some_and(|s| s.response.middle_clicked());
                    if !click {
                        ended = Some(view);
                    }
                } else {
                    let travel = travel + input.delta.length();
                    self.gesture = Some(Gesture::Pan { view, kind, button, depth, travel });
                    let d = DVec2::new(f64::from(input.delta.x), f64::from(input.delta.y));
                    if kind.is_2d() {
                        camera::pan_2d(state.cameras.cam2d_mut(kind), d);
                    } else {
                        let h = slot_of(view).map_or(1.0, |s| f64::from(s.rect.height()));
                        camera::pan_3d(&mut state.cameras.cam3d, d, depth, h);
                    }
                    ctx.set_cursor_icon(CursorIcon::Grabbing);
                }
            }
            None => {
                if self.mouselook.is_none()
                    && !self.swallow
                    && !dnd_active
                    && let Some(s) = hovered.and_then(slot_of)
                {
                    let hammer_3d = scheme.hammer_gestures() && s.kind == ViewKind::Perspective;
                    let space = input.space_down && !text_focus;
                    let press = input.pointer.unwrap_or(s.rect.center());
                    let look_with = |button: usize, click: Click| Gesture::Look {
                        view: s.index,
                        kind: s.kind,
                        button,
                        press,
                        travel: 0.0,
                        pending: Vec2::ZERO,
                        looking: false,
                        flew: false,
                        click,
                    };
                    if hammer_3d
                        && input.down[PRIMARY]
                        && input.down[SECONDARY]
                        && (input.pressed[PRIMARY] || input.pressed[SECONDARY])
                        && self.capture_view.is_none()
                    {
                        self.gesture = Some(Gesture::Strafe { view: s.index });
                        self.menu = None;
                    } else if hammer_3d && space && input.pressed[PRIMARY] {
                        self.gesture = Some(look_with(PRIMARY, Click::Nothing));
                    } else if hammer_3d && space && input.pressed[SECONDARY] {
                        let depth = self.pan_depth(state, s, input.pointer, cfg, ppp);
                        self.gesture =
                            Some(Gesture::Pan { view: s.index, kind: s.kind, button: SECONDARY, depth, travel: 0.0 });
                        self.menu = None;
                    } else if input.pressed[SECONDARY] && tool.takes_right_drag(s.kind, input.modifiers) {
                        self.menu = None;
                    } else if input.pressed[SECONDARY] {
                        self.gesture = Some(look_with(SECONDARY, Click::Menu));
                        self.menu = None;
                    } else if input.pressed[MIDDLE] && tool.takes_middle_drag(s.kind, input.modifiers) {
                        self.menu = None;
                    } else if input.pressed[MIDDLE] {
                        if scheme.middle_looks() && s.kind == ViewKind::Perspective {
                            self.gesture = Some(look_with(MIDDLE, Click::Tool));
                        } else {
                            let depth =
                                if s.kind.is_2d() { 0.0 } else { self.pan_depth(state, s, input.pointer, cfg, ppp) };
                            self.gesture =
                                Some(Gesture::Pan { view: s.index, kind: s.kind, button: MIDDLE, depth, travel: 0.0 });
                        }
                    } else if input.pressed[PRIMARY] && s.kind.is_2d() && space {
                        self.gesture = Some(Gesture::Pan {
                            view: s.index,
                            kind: s.kind,
                            button: PRIMARY,
                            depth: 0.0,
                            travel: 0.0,
                        });
                    }
                }
            }
        }

        let flying = self.mouselook.is_some() || self.gesture.is_some_and(|g| g.flies());
        let space_up = self.gesture.is_none_or(|g| g.space_flies_up());
        let fly = if flying { self.keys.fly_input_with(input.modifiers, space_up) } else { FlyInput::default() };
        let speed = f64::from(state.settings.view.camera_speed);
        let fast = f64::from(state.settings.view.camera_fast_multiplier);
        let target = camera::fly_target_velocity(&state.cameras.cam3d, &fly, speed, fast);
        let response = state.settings.workflow.fly_response_secs();
        self.velocity = camera::smooth_velocity_with(self.velocity, target, dt, response);
        if self.velocity != DVec3::ZERO {
            camera::integrate(&mut state.cameras.cam3d, self.velocity, dt);
        }

        if input.wheel != 0.0 && flying {
            let v = &mut state.settings.view;
            let factor = 1.25f32.powf(input.wheel.clamp(-10.0, 10.0));
            v.camera_speed = (v.camera_speed * factor).clamp(MIN_CAMERA_SPEED, MAX_CAMERA_SPEED);
            state.settings_dirty = true;
            let text = tf!("viewport.camera_speed", v.camera_speed.round());
            state.toasts.push_keyed("camera_speed", Severity::Info, text);
        } else if input.wheel != 0.0
            && !dnd_active
            && let Some(s) = hovered.and_then(slot_of)
        {
            let offered = self.mouselook.is_none() && self.gesture.is_none() && !self.swallow && self.menu.is_none();
            let handled = offered && {
                let view = view_info(s.index, s.kind, s.rect, &state.cameras, cfg, ppp);
                tool.wheel(state, &view, input.wheel, input.modifiers) == ToolFlow::Handled
            };
            if !handled {
                self.wheel(state, s, input, cfg, ppp);
            }
        }

        self.mouselook.map(|m| m.view).or(self.gesture.map(|g| g.view())).or(ended)
    }

    fn wheel(&mut self, state: &mut AppState, s: &Slot, input: &FrameInput, cfg: &RenderConfig, ppp: f32) {
        let notches = f64::from(input.wheel);
        let view = view_info(s.index, s.kind, s.rect, &state.cameras, cfg, ppp);
        if s.kind.is_2d() {
            if input.modifiers.ctrl || input.modifiers.command {
                match state.settings.workflow.ctrl_wheel {
                    CtrlWheel::GridSize => {
                        state.request(if notches > 0.0 { Action::GridSmaller } else { Action::GridLarger });
                    }
                    CtrlWheel::ZoomAll2d => zoom_all_2d(state, ZOOM_STEP_FACTOR.powf(notches.clamp(-20.0, 20.0))),
                }
            } else {
                let anchor =
                    if state.settings.view.zoom_to_cursor { input.pointer.map(|p| view.screen_to_2d(p)) } else { None };
                camera::zoom_2d(state.cameras.cam2d_mut(s.kind), notches, anchor);
                if state.settings.workflow.link_2d_zoom {
                    let zoom = state.cameras.cam2d(s.kind).zoom;
                    set_zoom_2d(state, zoom, Some(s.kind));
                }
            }
        } else {
            let dir = match input.pointer {
                Some(p) if state.settings.view.zoom_to_cursor && self.mouselook.is_none() => view.ray(p).1,
                _ => state.cameras.cam3d.forward(),
            };
            let step = camera::dolly_step(
                f64::from(state.settings.view.camera_speed),
                input.modifiers.shift,
                f64::from(state.settings.view.camera_fast_multiplier),
            );
            camera::wheel_dolly(&mut state.cameras.cam3d, dir, notches, step);
        }
    }

    fn key_navigation(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        input: &FrameInput,
        slots: &[Slot],
        hovered: Option<usize>,
    ) {
        if self.gesture.is_some() || self.mouselook.is_some() || self.menu.is_some() {
            return;
        }
        let Some(s) = hovered.and_then(|h| slots.iter().find(|s| s.index == h)) else { return };
        let busy = ctx.text_edit_focused()
            || egui::Popup::is_any_open(ctx)
            || ctx.memory(|m| m.focused().is_some() || m.top_modal_layer().is_some());
        if busy {
            return;
        }
        let arrows_free = state.doc.selection().is_empty() || !state.settings.edit.arrow_key_nudge;
        let any_arrow = input.arrow_presses.iter().any(|&n| n > 0) || input.arrows_down.iter().any(|&d| d);
        if arrows_free && any_arrow && input::ARROWS.iter().all(|k| key_unbound(&state.keys, *k)) {
            if s.kind.is_2d() {
                let [l, r, u, d] = input.arrow_presses.map(f64::from);
                if l + r + u + d > 0.0 {
                    let cam = state.cameras.cam2d_mut(s.kind);
                    let zoom = cam.zoom.max(Camera2d::MIN_ZOOM);
                    let step_x = f64::from(s.rect.width()) / zoom / 8.0;
                    let step_y = f64::from(s.rect.height()) / zoom / 8.0;
                    cam.center.x += (r - l) * step_x;
                    cam.center.y += (u - d) * step_y;
                }
                input::consume_arrows(ctx);
            } else if state.settings.workflow.nav_scheme.hammer_gestures() {
                let [l, r, u, d] = input.arrows_down.map(|b| f64::from(u8::from(b)));
                if l + r + u + d > 0.0 {
                    let dt = f64::from(state.frame.dt.max(0.0));
                    camera::arrow_look(&mut state.cameras.cam3d, r - l, u - d, dt);
                    ctx.request_repaint();
                }
                input::consume_arrows(ctx);
            }
        }
        let typed_unbound = input.digit_logical.is_none_or(|k| key_unbound(&state.keys, k));
        if let Some(digit) = input.digit
            && s.kind.is_2d()
            && let Some(zoom) = camera::preset_zoom(digit)
            && key_unbound(&state.keys, digit_key(digit))
            && typed_unbound
        {
            if state.settings.workflow.link_2d_zoom {
                set_zoom_2d(state, zoom, None);
            } else {
                let cam = state.cameras.cam2d_mut(s.kind);
                cam.zoom_at(zoom / cam.zoom.max(Camera2d::MIN_ZOOM), cam.center);
            }
            input::consume_digits(ctx);
        }
    }

    fn pan_depth(&self, state: &AppState, s: &Slot, pointer: Option<Pos2>, cfg: &RenderConfig, ppp: f32) -> f64 {
        let view = view_info(s.index, s.kind, s.rect, &state.cameras, cfg, ppp);
        pointer
            .and_then(|p| placement::pick_surface(state, &view, p))
            .map(|hit| (hit.point - view.cam3d.pos).dot(view.cam3d.forward()))
            .filter(|d| d.is_finite() && *d > 1.0)
            .unwrap_or(512.0)
            .min(65536.0)
    }

    fn selection_center(&mut self, state: &AppState) -> Option<DVec3> {
        let sel = state.doc.selection();
        let key = (state.doc.revision(), sel.objects.len() + sel.faces.len());
        if self.sel_key != Some(key) {
            self.sel_key = Some(key);
            self.sel_center = state.doc.selection_center(&|e| state.assets.entity_bounds(e)).filter(|c| c.is_finite());
        }
        self.sel_center
    }

    fn depth_2d(&mut self, state: &AppState, kind: ViewKind) -> f64 {
        match (kind.axes_2d(), self.selection_center(state)) {
            (Some((_, _, d)), Some(c)) => c[d],
            _ => 0.0,
        }
    }

    fn update_hover(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2) -> bool {
        if view.is_2d() {
            let depth = self.depth_2d(state, view.kind);
            state.hover_world = Some(placement::point_2d(state, view, pos, depth));
            return true;
        }
        let key = PickKey { view: view.index, pos, cam: cam_key(state), revision: state.doc.revision() };
        if self.hover_key != Some(key) {
            self.hover_key = Some(key);
            self.hover_hit = placement::pick_surface(state, view, pos).map(|h| h.point);
        }
        state.hover_world = self.hover_hit;
        self.hover_hit.is_some()
    }

    #[allow(clippy::too_many_arguments)]
    fn view_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        state: &mut AppState,
        tool: &mut dyn Tool,
        slot: &Slot,
        cfg: &RenderConfig,
        input: &FrameInput,
        hovered: Option<usize>,
        nav_view: Option<usize>,
        maximized: bool,
        ppp: f32,
    ) -> bool {
        let i = slot.index;
        let view = view_info(i, slot.kind, slot.rect, &state.cameras, cfg, ppp);
        let nav_here = nav_view == Some(i);
        let is_hovered = hovered == Some(i);
        let pointer_here = if is_hovered || self.capture_view == Some(i) { input.pointer } else { None };

        let mut hover_set = false;
        if is_hovered
            && !nav_here
            && let Some(p) = input.pointer
        {
            hover_set = self.update_hover(state, &view, p);
        }

        let mut drop_label: Option<String> = None;
        if let Some(payload) = slot.response.dnd_release_payload::<DragPayload>() {
            if let Some(p) = input.pointer {
                self.perform_drop(ctx, state, &view, &payload, p);
            }
            self.preview = DropPreview::default();
        } else if let Some(payload) = slot.response.dnd_hover_payload::<DragPayload>()
            && let Some(p) = input.pointer
        {
            drop_label = Some(self.update_preview(state, &view, &payload, p));
        }

        let dnd_active = egui::DragAndDrop::has_any_payload(ctx);
        let routed = if nav_here {
            self.capture_view == Some(i)
                || (self.capture_view.is_none()
                    && is_hovered
                    && slot.kind == ViewKind::Perspective
                    && tool_works_while_navigating(tool.id()))
        } else {
            self.capture_view == Some(i) || (self.capture_view.is_none() && is_hovered)
        };
        let picking = pick::pending(state).is_some();
        if routed && !self.swallow && self.menu.is_none() && !dnd_active {
            if picking {
                if !nav_here
                    && slot.response.clicked()
                    && let Some(p) = pointer_here
                {
                    let depth = self.depth_2d(state, view.kind);
                    self.picker.click(state, &view, p, input.modifiers, depth);
                }
            } else {
                let vi = ViewInput {
                    view: &view,
                    response: &slot.response,
                    pointer: pointer_here,
                    modifiers: input.modifiers,
                    navigating: nav_here,
                };
                tool.view_input(state, &vi);
            }
        }
        let view = view_info(i, slot.kind, slot.rect, &state.cameras, cfg, ppp);

        if is_hovered && nav_view.is_none() && self.gesture.is_none() {
            if slot.kind.is_2d() && input.space_down && !ctx.text_edit_focused() {
                ctx.set_cursor_icon(CursorIcon::Grab);
            } else if picking {
                ctx.set_cursor_icon(CursorIcon::Crosshair);
            } else if let Some(p) = input.pointer
                && let Some(c) = tool.cursor(state, &view, p)
            {
                ctx.set_cursor_icon(c);
            }
        }

        self.overlay.clear();
        for layer in &self.layers {
            layer.add(state, &view, &mut self.overlay);
        }
        self.picker.add_overlay(state, &view, &mut self.overlay);
        if !view.is_2d() && state.settings.view.show_3d_grid {
            overlay::add_grid_3d(&mut self.overlay, &view, state.settings.view.grid_highlight_every);
        }
        let frame = *cfg.frame(view.kind);
        if state.settings.edit.show_grid && !frame.is_plain() {
            let major = state.settings.view.grid_highlight_every;
            if view.is_2d() {
                let min_px = f64::from(cfg.options_2d.grid_min_px).max(2.0);
                overlay::add_lattice_2d(&mut self.overlay, &view, &frame, major, min_px);
            } else if state.settings.grid.plane_3d {
                overlay::add_grid_plane_3d(&mut self.overlay, &view, &frame, major);
            }
        }
        if drop_label.is_some() {
            self.add_preview_overlay(&view);
        }
        tool.overlay(state, &view, &mut self.overlay);
        if state.gfx.is_some() && self.refill_retained(state, tool, &view) {
            if let Some(gfx) = state.gfx.as_mut() {
                gfx.renderer.set_retained_overlay(i as u32, &self.retained_overlay);
            }
            self.trim_retained_scratch();
        }

        let painter = ui.painter_at(slot.rect);
        let bg = if view.is_2d() { cfg.background_2d } else { cfg.background_3d };
        let bg = Color32::from_rgb((bg[0] * 255.0) as u8, (bg[1] * 255.0) as u8, (bg[2] * 255.0) as u8);
        let mut drawn = false;
        if !view.is_2d() && state.runtime.fade.sample(state).is_some() {
            ctx.request_repaint();
        }
        if let Some(gfx) = state.gfx.as_mut() {
            let mut params = view_params(&view, cfg);
            if !view.is_2d() {
                params.size = params::scaled_size(params.size, cfg.render_scale);
            }
            let mut options = view_options(&view, cfg);
            let animate = state.settings.render.animate_materials && state.tool != ToolId::Texture;
            let shows = gfx.renderer.view_shows_animation(i as u32) || gfx.renderer.view_texture(i as u32).is_none();
            if !view.is_2d()
                && animate
                && shows
                && (gfx.renderer.has_animated_materials() || gfx.renderer.detail_sprites_sway())
            {
                options.time = (state.frame.time % 3600.0) as f32;
                ctx.request_repaint();
            }
            options.compiled_lighting = self.player_view == Some(i)
                && self.player_world.is_some()
                && state.settings.render.player_compiled_lighting;
            if self.player_view == Some(i) {
                options.fog_volume_inside = options.fog_volumes;
                options.fog_volumes = false;
            }
            gfx.renderer.set_tone_settings(params::tone_for(&state.settings.render, &cfg.plan));
            gfx.renderer.set_quality(params::quality_for(&cfg.plan));
            gfx.renderer.set_lazy_exposure(true);
            gfx.renderer.set_view_options(i as u32, options);
            let key = ViewKey {
                params,
                content: gfx.renderer.content_generation(),
                overlay: render_cache::overlay_hash(&self.overlay),
            };
            let has_image = gfx.renderer.view_texture(i as u32).is_some();
            if self.render_cache.needs_render(i, &key, has_image) {
                gfx.renderer.render_view(i as u32, &params, &self.overlay);
                self.render_cache.rendered(i, key);
                if gfx.renderer.wants_another_frame() {
                    ctx.request_repaint();
                }
                state.runtime.render.views_rendered += 1;
                if !view.is_2d() {
                    gfx.renderer.step_tone(i as u32, state.frame.dt);
                }
            } else {
                self.render_cache.reused();
                state.runtime.render.views_reused += 1;
            }
            if let Some(tex) = gfx.view_egui_texture(i as u32) {
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                painter.image(tex, slot.rect, uv, Color32::WHITE);
                drawn = true;
            }
        }
        if !drawn {
            paint::placeholder(&painter, slot.rect, bg, t!(slot.kind.label_key()), t!("viewport.starting"));
        }

        tool.paint(state, &view, &painter);
        self.paint_overlays(&painter, state, &view, input, drop_label);
        let focused = state.ui.focused_view == Some(i);
        let border = if focused { hp_app::theme::ACCENT.gamma_multiply(0.55) } else { Color32::from_white_alpha(14) };
        painter.rect_stroke(slot.rect, 0, Stroke::new(1.0, border), StrokeKind::Inside);

        self.header_ui(ui, state, slot, &view, maximized);
        if !view.is_2d() && state.settings.render.collision_only {
            collision_legend(ui, slot.rect);
        }
        if !view.is_2d() && state.settings.workflow.show_view_info {
            self.view_info_ui(ui, state, slot, &view);
        } else {
            self.info_rects[i] = Rect::NOTHING;
        }
        self.perf_rects[i] = if view.is_2d() {
            Rect::NOTHING
        } else {
            perf_ui::show(ui, state, slot.rect, slot.rect.top() + HEADER_HEIGHT + 10.0, i)
        };
        hover_set
    }

    fn view_info_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, slot: &Slot, view: &ViewInfo) {
        let now = state.frame.time;
        let stale = self.stats.as_ref().is_none_or(|(at, _)| (now - at).abs() > 0.5);
        if stale && let Some(gfx) = state.gfx.as_ref() {
            self.stats = Some((now, gfx.renderer.stats()));
        }
        let mut lines = workflow::info_lines(state.frame.fps, &view.cam3d, self.stats.as_ref().map(|(_, s)| s));
        if let Some((_, s)) = &self.stats {
            lines.push(layers::render::culling_line(s));
        }
        let painter = ui.painter_at(slot.rect);
        let galley = painter.layout_no_wrap(lines.join("\n"), egui::FontId::monospace(11.0), paint::TEXT);
        let top = slot.rect.top() + HEADER_HEIGHT + 10.0;
        let rect =
            Align2::RIGHT_TOP.anchor_size(Pos2::new(slot.rect.right() - 6.0, top), galley.size() + vec2(10.0, 6.0));
        let rect = rect.intersect(slot.rect);
        let response = ui.interact(rect, ui.scope_id().with(("hp_viewport_info", slot.index)), Sense::CLICK);
        let bg = if response.hovered() { paint::PILL_BG.gamma_multiply(1.3) } else { paint::PILL_BG };
        painter.rect_filled(rect, 4, bg);
        painter.galley(rect.min + vec2(5.0, 3.0), galley, paint::TEXT);
        let response = response.on_hover_text(t!("viewport.info_copy_tip"));
        if response.clicked() {
            let command = workflow::setpos_command(&view.cam3d);
            ui.ctx().copy_text(command.clone());
            state.notify(Severity::Info, tf!("viewport.info_copied", command));
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        self.info_rects[slot.index] = rect;
    }

    fn refill_retained(&mut self, state: &AppState, tool: &dyn Tool, view: &ViewInfo) -> bool {
        let Some(slot) = self.retained.get_mut(view.index) else { return false };
        let tool_part = tool.retained_overlay_version(state, view).map(|version| (tool.id(), version));
        let parts = &mut self.layer_version_scratch;
        parts.clear();
        parts.extend(self.layers.iter().map(|l| l.retained_version(state, view)));
        let wanted = tool_part.is_some() || parts.iter().any(Option::is_some);
        if (!wanted && slot.is_none())
            || (wanted
                && slot
                    .as_ref()
                    .is_some_and(|key| key.kind == view.kind && key.tool == tool_part && key.layers == *parts))
        {
            return false;
        }
        self.retained_overlay.clear();
        if wanted {
            if tool_part.is_some() {
                tool.retained_overlay(state, view, &mut self.retained_overlay);
            }
            for (layer, version) in self.layers.iter().zip(parts.iter()) {
                if version.is_some() {
                    layer.retained(state, view, &mut self.retained_overlay);
                }
            }
        }
        *slot = wanted.then(|| RetainedKey { kind: view.kind, tool: tool_part, layers: parts.clone() });
        true
    }

    fn trim_retained_scratch(&mut self) {
        let o = &self.retained_overlay;
        let capacity = o.lines.capacity()
            + o.lines_on_top.capacity()
            + o.triangles.capacity()
            + o.triangles_on_top.capacity()
            + o.points.capacity();
        if capacity > RETAINED_KEEP {
            self.retained_overlay = Overlay::default();
        } else {
            self.retained_overlay.clear();
        }
    }

    fn drop_stale_retained(&mut self, state: &mut AppState, tool: ToolId, slots: &[Slot]) {
        let Some(gfx) = state.gfx.as_mut() else { return };
        for (i, key) in self.retained.iter_mut().enumerate() {
            let shown = slots.iter().any(|s| s.index == i);
            if !shown && key.as_ref().is_some_and(|k| k.tool.is_some_and(|(t, _)| t != tool)) {
                gfx.renderer.clear_retained_overlay(i as u32);
                *key = None;
            }
        }
    }

    fn paint_overlays(
        &self,
        painter: &egui::Painter,
        state: &AppState,
        view: &ViewInfo,
        input: &FrameInput,
        drop_label: Option<String>,
    ) {
        let rect = view.rect;
        collaboration::paint(state, view, painter);
        paint::axis_gizmo(painter, rect.left_bottom() + vec2(30.0, -30.0), view);
        if walk_legend_shown(state, view) {
            let r = &state.settings.render;
            paint::walk_legend(painter, rect.right_bottom() + vec2(-6.0, -6.0), r.disp_walk_mode, r.disp_walk_angle);
        }
        if !view.is_2d() && state.settings.view.show_fps && !state.settings.workflow.show_view_info {
            paint::text_pill(
                painter,
                rect.right_top() + vec2(-6.0, 6.0),
                Align2::RIGHT_TOP,
                tf!("status.fps", format!("{:.0}", state.frame.fps)),
                paint::TEXT,
            );
        }
        for layer in &self.layers {
            layer.paint(state, view, painter);
        }
        if let Some(hint) = pick::hint(state, self.picker.faces().len()) {
            paint::text_pill(
                painter,
                rect.center_top() + vec2(0.0, HEADER_HEIGHT + 12.0),
                Align2::CENTER_TOP,
                hint,
                paint::overlay_color(pick::PICK_COLOR),
            );
        }
        if let Some(ml) = self.mouselook
            && ml.view == view.index
        {
            paint::crosshair(painter, rect.center());
        }
        if let (Some(label), Some(p)) = (drop_label, input.pointer) {
            paint::text_pill(painter, p + vec2(16.0, 18.0), Align2::LEFT_TOP, label, paint::TEXT);
        }
    }

    fn header_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, slot: &Slot, view: &ViewInfo, maximized: bool) {
        let i = slot.index;
        let rect = slot.rect;
        let header = Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2((rect.width() - 8.0).max(8.0), HEADER_HEIGHT));
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(header)
                .layout(Layout::left_to_right(Align::Center))
                .id_salt(("hp_viewport_header", i)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let shown = egui::Frame::new()
            .fill(paint::PILL_BG)
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(3, 1))
            .show(&mut child, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let focused = state.ui.focused_view == Some(i);
                let name = t!(slot.kind.label_key());
                let label = view_label(ui, name, focused);
                let label = if slot.kind == ViewKind::Perspective {
                    let cam = view.cam3d;
                    label.on_hover_ui(|ui| {
                        ui.label(tf!("viewport.camera_pos", hp_app::widgets::fmt_vec3(cam.pos.round())));
                        ui.label(tf!(
                            "viewport.camera_angles",
                            format!("{:.0}", cam.yaw_deg),
                            format!("{:.0}", cam.pitch_deg)
                        ));
                        ui.weak(t!("viewport.change_view"));
                    })
                } else {
                    label.on_hover_text(t!("viewport.change_view"))
                };
                if label.double_clicked() {
                    toggle_maximize(state, i);
                    egui::Popup::close_all(ui.ctx());
                }
                egui::Popup::menu(&label).show(|ui| {
                    ui.set_min_width(170.0);
                    logical::view_type_menu(ui, state, i, QuadrantView::View(slot.kind));
                    if slot.kind.is_2d() {
                        disp_2d_menu(ui, state, i);
                    }
                    if slot.kind == ViewKind::Perspective {
                        ui.separator();
                        ui.label(egui::RichText::new(t!("menu.render_mode")).weak().small());
                        let current = state.settings.view.render_mode;
                        for (mode, action) in RENDER_MODES {
                            let sc = state.keys.shortcut(action).map(|s| hp_app::actions::shortcut_label(&s));
                            let mut b = egui::Button::selectable(current == mode, action.label());
                            if let Some(sc) = sc {
                                b = b.shortcut_text(sc);
                            }
                            if ui.add(b).clicked() && current != mode {
                                state.request(action);
                            }
                        }
                    }
                });
                if view.is_2d() {
                    let zoom = tf!("viewport.zoom", format_zoom_percent(view.cam2d.zoom));
                    let text = if state.settings.grid.mode == hp_app::settings::GridMode::PerView {
                        format!("{}   {zoom}", tf!("status.grid", fmt_coord(view.grid)))
                    } else {
                        zoom
                    };
                    ui.label(egui::RichText::new(text).size(11.0).color(paint::TEXT_WEAK));
                } else if state.settings.grid.mode == hp_app::settings::GridMode::PerView {
                    let text = tf!("status.grid", fmt_coord(view.grid));
                    ui.label(egui::RichText::new(text).size(11.0).color(paint::TEXT_WEAK));
                }
                logical::maximize_button(ui, state, i, maximized);
            });
        self.header_rects[i] = shown.response.rect;
    }

    fn open_menu(
        &mut self,
        state: &mut AppState,
        slot: Option<&Slot>,
        kind: ViewKind,
        press: Pos2,
        cfg: &RenderConfig,
        ppp: f32,
    ) {
        let Some(s) = slot else { return };
        let view = view_info(s.index, kind, s.rect, &state.cameras, cfg, ppp);
        let (point, normal) = if kind.is_2d() {
            let depth = self.depth_2d(state, kind);
            (Some(placement::point_2d(state, &view, press, depth)), None)
        } else {
            match placement::pick_surface(state, &view, press) {
                Some(hit) => (Some(hit.point), Some(hit.normal)),
                None => (None, None),
            }
        };
        self.menu = Some(OpenMenu { view, pos: press, point, normal, opened_frame: self.frame });
        state.ui.focused_view = Some(s.index);
        let class = state.current_entity_class.trim().to_owned();
        if !class.is_empty() {
            let _ = placement::placement_bounds(state, &class, None);
        }
    }

    fn context_menu_ui(&mut self, ui: &egui::Ui, ctx: &egui::Context, state: &mut AppState, tool: &mut dyn Tool) {
        let Some(menu) = self.menu else { return };
        let cx = menu_context(state, &menu);
        let tool_entries = tool.context_entries(state);
        let mut open = true;
        let mut choice = None;
        let id = Id::unique(("hp_viewport_context_menu", menu.opened_frame));
        egui::Popup::new(id, ctx.clone(), egui::PopupAnchor::Position(menu.pos), ui.layer_id())
            .kind(egui::PopupKind::Menu)
            .layout(Layout::top_down_justified(Align::Min))
            .style(egui::menu::menu_style)
            .width(260.0)
            .open_bool(&mut open)
            .show(|ui| {
                ui.set_min_width(240.0);
                for entry in &tool_entries {
                    if ui.add_enabled(entry.enabled, egui::Button::new(entry.label.as_str())).clicked() {
                        choice = Some(MenuChoice::Tool(entry.id));
                    }
                }
                if !tool_entries.is_empty() {
                    ui.separator();
                }
                for &entry in ENTRIES {
                    let enabled = entry_enabled(entry, cx, |a| a.enabled(state));
                    match entry {
                        MenuEntry::Separator => {
                            ui.separator();
                        }
                        MenuEntry::Action(action) => {
                            let sc = state.keys.shortcut(action);
                            if hp_app::widgets::action_button(ui, action.label(), sc.as_ref(), enabled) {
                                choice = Some(MenuChoice::Action(action));
                            }
                        }
                        MenuEntry::Submenu(group) => {
                            let actions = submenu_actions(group);
                            ui.add_enabled_ui(enabled, |ui| {
                                ui.menu_button(t!(submenu_title(group)), |ui| {
                                    ui.set_min_width(160.0);
                                    for action in actions {
                                        let sc = state.keys.shortcut(action);
                                        let on = action.enabled(state);
                                        if hp_app::widgets::action_button(ui, action.label(), sc.as_ref(), on) {
                                            choice = Some(MenuChoice::Action(action));
                                        }
                                    }
                                });
                            });
                        }
                        MenuEntry::CenterHere => {
                            if ui.add_enabled(enabled, egui::Button::new(t!("viewport.center_here"))).clicked() {
                                choice = Some(MenuChoice::CenterHere);
                            }
                        }
                        MenuEntry::CreateHere => {
                            let class = state.current_entity_class.trim();
                            let label = if class.is_empty() {
                                t!("viewport.create_entity_here").to_owned()
                            } else {
                                tf!("viewport.create_here", class)
                            };
                            if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                                choice = Some(MenuChoice::CreateHere);
                            }
                        }
                    }
                }
            });
        if let Some(c) = choice {
            match c {
                MenuChoice::Tool(id) => tool.context_command(state, id),
                c => self.run_menu_choice(state, &menu, c),
            }
            open = false;
        }
        if !open {
            self.menu = None;
        }
    }

    fn run_menu_choice(&mut self, state: &mut AppState, menu: &OpenMenu, choice: MenuChoice) {
        match choice {
            MenuChoice::Tool(_) => {}
            MenuChoice::Action(a) => state.request(a),
            MenuChoice::CenterHere => {
                let Some(mut p) = menu.point else { return };
                match menu.view.kind {
                    ViewKind::Top => p.z = state.cameras.front.center.y,
                    ViewKind::Front => p.x = state.cameras.top.center.x,
                    ViewKind::Side => p.y = state.cameras.top.center.y,
                    _ => {}
                }
                state.cameras.center_2d(p);
                state.cameras.cam3d.look_at(p);
            }
            MenuChoice::CreateHere => {
                let Some(point) = menu.point else { return };
                let class = state.current_entity_class.trim().to_owned();
                if class.is_empty() {
                    return;
                }
                if placement::is_brush_class(state, &class) {
                    state.notify(Severity::Warning, tf!("viewport.brush_class", class));
                    return;
                }
                let target = Target {
                    point,
                    normal: menu.normal.unwrap_or(menu.view.kind.toward_viewer()),
                    face: None,
                    on_surface: !menu.view.is_2d() && menu.normal.is_some(),
                    is_2d: menu.view.is_2d(),
                };
                let bounds = placement::placement_bounds(state, &class, None);
                let keys = prop_model_keys(state, &class);
                let extra: Vec<(&str, &str)> = keys.iter().map(|(k, v)| (*k, v.as_str())).collect();
                if let Err(e) = placement::create_entity(state, &class, &target, bounds, &extra) {
                    state.notify(Severity::Error, tf!("viewport.edit_failed", e));
                }
            }
        }
    }

    fn update_preview(&mut self, state: &mut AppState, view: &ViewInfo, payload: &DragPayload, pos: Pos2) -> String {
        let key = (view.index, pos, state.doc.revision());
        if self.preview.key != Some(key) {
            self.preview.key = Some(key);
            let depth = self.depth_2d(state, view.kind);
            let target = placement::target_at(state, view, pos, depth);
            self.preview.polygon.clear();
            self.preview.local_bounds = (DVec3::splat(-placement::DEFAULT_HALF), DVec3::splat(placement::DEFAULT_HALF));
            self.preview.shape = match payload {
                DragPayload::Material(_) => {
                    if let Some(face) = target.face
                        && !view.is_2d()
                        && let Some(poly) = state.doc.face_polygon(face)
                    {
                        self.preview.polygon.extend_from_slice(poly);
                        PreviewShape::Face
                    } else {
                        PreviewShape::None
                    }
                }
                DragPayload::Model { path, .. } => {
                    if let (_, Some(b), _) = placement::prop_for_model(state, path) {
                        self.preview.local_bounds = b;
                    }
                    PreviewShape::Box
                }
                DragPayload::EntityClass(class) => {
                    self.preview.local_bounds = placement::placement_bounds(state, class, None);
                    PreviewShape::Box
                }
                DragPayload::Prefab(_) => PreviewShape::Marker,
            };
            let (lo, hi) = self.preview.local_bounds;
            self.preview.origin = placement::origin_for(state, &target, lo, hi);
            self.preview.target = Some(target);
        }
        let file_name =
            |p: &std::path::Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match payload {
            DragPayload::Material(name) => {
                if view.is_2d() {
                    if state.doc.selection().is_empty() {
                        t!("viewport.drop_no_selection").to_owned()
                    } else {
                        tf!("viewport.drop_apply_selection", name)
                    }
                } else if self.preview.shape != PreviewShape::Face {
                    t!("viewport.drop_no_face").to_owned()
                } else {
                    tf!("viewport.drop_apply_face", name)
                }
            }
            DragPayload::Model { path, .. } => tf!("viewport.drop_place", path),
            DragPayload::EntityClass(class) => tf!("viewport.drop_place", class),
            DragPayload::Prefab(path) => tf!("viewport.drop_paste_prefab", file_name(path)),
        }
    }

    fn add_preview_overlay(&mut self, view: &ViewInfo) {
        let Some(target) = self.preview.target else { return };
        let color = overlay::DROP_COLOR;
        match self.preview.shape {
            PreviewShape::None => {}
            PreviewShape::Face => overlay::face_highlight(&mut self.overlay, view, &self.preview.polygon, color),
            PreviewShape::Marker => overlay::cross_marker(&mut self.overlay, view, target.point, 10.0, color),
            PreviewShape::Box => {
                let (lo, hi) = self.preview.local_bounds;
                let origin = self.preview.origin;
                overlay::cross_marker(&mut self.overlay, view, target.point, 10.0, color);
                overlay::box_lines_on_top(&mut self.overlay, origin + lo, origin + hi, color);
            }
        }
    }

    fn perform_drop(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        view: &ViewInfo,
        payload: &DragPayload,
        pos: Pos2,
    ) {
        state.ui.focused_view = Some(view.index);
        let depth = self.depth_2d(state, view.kind);
        let target = placement::target_at(state, view, pos, depth);
        match payload {
            DragPayload::Material(name) => drop_material(state, view, &target, name),
            DragPayload::Model { path, skin: dragged_skin } => {
                let (class, bounds, err) = placement::prop_for_model(state, path);
                if let Some(e) = err {
                    state.log(Severity::Warning, tf!("viewport.model_unavailable", path, e));
                }
                let bounds =
                    bounds.unwrap_or((DVec3::splat(-placement::DEFAULT_HALF), DVec3::splat(placement::DEFAULT_HALF)));
                let skin = if *dragged_skin > 0 { *dragged_skin } else { drop_skin(state, path) };
                let keys = placement::model_keys(path, skin);
                let extra: Vec<(&str, &str)> = keys.iter().map(|(k, v)| (*k, v.as_str())).collect();
                match placement::create_entity(state, class, &target, bounds, &extra) {
                    Ok(_) => {
                        state.current_model = Some(path.clone());
                        state.current_model_skin = skin;
                    }
                    Err(e) => state.notify(Severity::Error, tf!("viewport.edit_failed", e)),
                }
            }
            DragPayload::EntityClass(class) => {
                if placement::is_brush_class(state, class) {
                    state.notify(Severity::Warning, tf!("viewport.brush_class", class));
                    return;
                }
                let bounds = placement::placement_bounds(state, class, None);
                let keys = prop_model_keys(state, class);
                let extra: Vec<(&str, &str)> = keys.iter().map(|(k, v)| (*k, v.as_str())).collect();
                match placement::create_entity(state, class, &target, bounds, &extra) {
                    Ok(_) => state.current_entity_class.clone_from(class),
                    Err(e) => state.notify(Severity::Error, tf!("viewport.edit_failed", e)),
                }
            }
            DragPayload::Prefab(path) => self.start_prefab(ctx, state, view, target, path.clone()),
        }
    }

    fn start_prefab(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        view: &ViewInfo,
        target: Target,
        path: PathBuf,
    ) {
        let (tx, rx) = std::sync::mpsc::channel();
        let read_path = path.clone();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-prefab-read".to_owned()).spawn(move || {
            let result =
                std::fs::read(&read_path).map(|b| String::from_utf8_lossy(&b).into_owned()).map_err(|e| e.to_string());
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        match spawned {
            Ok(_) => {
                let axes = view.kind.axes_2d().map(|(r, u, _)| (r, u));
                self.pending_prefab = Some(PendingPrefab { rx, path, target, axes });
            }
            Err(e) => state.notify(Severity::Error, tf!("viewport.prefab_read_failed", path.display(), e)),
        }
    }

    fn poll_prefab(&mut self, state: &mut AppState) {
        let Some(pending) = &self.pending_prefab else { return };
        let result = match pending.rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("reader stopped".to_owned()),
        };
        let Some(pending) = self.pending_prefab.take() else { return };
        match result {
            Ok(text) => {
                if let Err(e) = paste_prefab(state, &text, &pending.target, pending.axes) {
                    state.notify(Severity::Error, tf!("viewport.edit_failed", e));
                }
            }
            Err(e) => state.notify(Severity::Error, tf!("viewport.prefab_read_failed", pending.path.display(), e)),
        }
    }

    pub fn is_loading_prefab(&self) -> bool {
        self.pending_prefab.is_some()
    }
}

fn menu_context(state: &AppState, menu: &OpenMenu) -> MenuContext {
    MenuContext { has_point: menu.point.is_some(), has_entity_class: !state.current_entity_class.trim().is_empty() }
}

const ZOOM_STEP_FACTOR: f64 = camera::ZOOM_STEP;

pub fn zoom_all_2d(state: &mut AppState, factor: f64) {
    if !factor.is_finite() || factor <= 0.0 {
        return;
    }
    for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
        let cam = state.cameras.cam2d_mut(kind);
        cam.zoom_at(factor, cam.center);
    }
}

pub fn set_zoom_2d(state: &mut AppState, zoom: f64, except: Option<ViewKind>) {
    if !zoom.is_finite() || zoom <= 0.0 {
        return;
    }
    for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
        if Some(kind) == except {
            continue;
        }
        let cam = state.cameras.cam2d_mut(kind);
        cam.zoom_at(zoom / cam.zoom.max(Camera2d::MIN_ZOOM), cam.center);
    }
}

fn key_unbound(keys: &hp_app::actions::KeyBindings, key: egui::Key) -> bool {
    keys.action_for(&egui::KeyboardShortcut::new(egui::Modifiers::NONE, key)).is_none()
}

fn digit_key(digit: u8) -> egui::Key {
    match digit {
        1 => egui::Key::Num1,
        2 => egui::Key::Num2,
        3 => egui::Key::Num3,
        4 => egui::Key::Num4,
        5 => egui::Key::Num5,
        6 => egui::Key::Num6,
        7 => egui::Key::Num7,
        8 => egui::Key::Num8,
        _ => egui::Key::Num9,
    }
}

fn drop_skin(state: &AppState, path: &str) -> u32 {
    let same = |m: &str| m.trim().replace('\\', "/").eq_ignore_ascii_case(path.trim());
    if state.current_model.as_deref().is_some_and(same) { state.current_model_skin } else { 0 }
}

fn prop_model_keys(state: &AppState, class: &str) -> Vec<(&'static str, String)> {
    match state.current_model.as_deref().filter(|m| !m.trim().is_empty()) {
        Some(model) if placement::is_prop_class(state, class) => placement::model_keys(model, state.current_model_skin),
        _ => Vec::new(),
    }
}

pub(crate) const RENDER_MODES: [(RenderModeSetting, Action); 10] = [
    (RenderModeSetting::Textured, Action::RenderTextured),
    (RenderModeSetting::Unshaded, Action::RenderUnshaded),
    (RenderModeSetting::Flat, Action::RenderFlat),
    (RenderModeSetting::Wireframe, Action::RenderWireframe),
    (RenderModeSetting::TexturedWire, Action::RenderTexturedWire),
    (RenderModeSetting::Lit, Action::RenderLit),
    (RenderModeSetting::SmoothingGroups, Action::RenderSmoothingGroups),
    (RenderModeSetting::Baked, Action::RenderBaked),
    (RenderModeSetting::LightmapGrid, Action::RenderLightmapGrid),
    (RenderModeSetting::ColorCorrected, Action::RenderColorCorrected),
];

pub fn tool_takes_right_click(tool: ToolId, kind: ViewKind, modifiers: egui::Modifiers) -> bool {
    tool == ToolId::Slice || tool == ToolId::Texture && kind == ViewKind::Perspective && !modifiers.shift
}

pub fn tool_works_while_navigating(tool: ToolId) -> bool {
    tool == ToolId::Displacement
}

pub fn format_zoom_percent(zoom: f64) -> String {
    let pct = zoom * 100.0;
    if !pct.is_finite() {
        return "?".to_owned();
    }
    let text = if pct >= 9.95 {
        format!("{pct:.0}")
    } else if pct >= 0.995 {
        format!("{pct:.1}")
    } else {
        format!("{pct:.2}")
    };
    if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').to_owned() } else { text }
}

fn toggle_maximize(state: &mut AppState, index: usize) {
    state.ui.maximized_view = if state.ui.maximized_view == Some(index) { None } else { Some(index) };
}

pub fn disp_2d_label_key(mode: Disp2dSetting) -> &'static str {
    match mode {
        Disp2dSetting::Mesh => "viewport.disp_mesh",
        Disp2dSetting::SelectedMesh => "viewport.disp_selected",
        Disp2dSetting::Outline => "viewport.disp_outline",
    }
}

fn disp_2d_menu(ui: &mut egui::Ui, state: &mut AppState, index: usize) {
    ui.separator();
    ui.label(egui::RichText::new(t!("viewport.disp_2d")).weak().small());
    let r = &state.settings.render;
    let Some(current) = r.displacements_2d_views.get(index).copied() else { return };
    let global = tf!("viewport.disp_global", t!(disp_2d_label_key(r.displacements_2d)));
    let mut choice = None;
    if ui.add(egui::Button::selectable(current == Disp2dViewSetting::Global, global)).clicked() {
        choice = Some(Disp2dViewSetting::Global);
    }
    for mode in Disp2dSetting::ALL {
        let own = Disp2dViewSetting::Own(mode);
        if ui.add(egui::Button::selectable(current == own, t!(disp_2d_label_key(mode)))).clicked() {
            choice = Some(own);
        }
    }
    if let Some(c) = choice
        && let Some(slot) = state.settings.render.displacements_2d_views.get_mut(index)
        && *slot != c
    {
        *slot = c;
        state.settings_dirty = true;
    }
    let mut source = state.settings.render.disp_source_2d;
    if ui.checkbox(&mut source, t!("viewport.disp_source")).on_hover_text(t!("viewport.disp_source_tip")).changed() {
        state.settings.render.disp_source_2d = source;
        state.settings_dirty = true;
    }
}

fn walk_legend_shown(state: &AppState, view: &ViewInfo) -> bool {
    let r = &state.settings.render;
    if !r.disp_walkable || !r.show_displacements {
        return false;
    }
    !view.is_2d()
        || (state.ui.maximized_view == Some(view.index)
            && r.displacements_in_2d
            && r.displacements_2d_of(view.index) != Disp2dSetting::Outline)
}

fn view_label(ui: &mut egui::Ui, text: &str, focused: bool) -> egui::Response {
    let color = if focused { hp_app::theme::ACCENT } else { paint::TEXT };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::proportional(12.0), color);
    let size = vec2(galley.size().x + 18.0, 18.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::CLICK);
    if resp.hovered() {
        ui.painter().rect_filled(rect, 3, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let text_pos = Pos2::new(rect.left() + 4.0, rect.center().y - galley.size().y * 0.5);
    ui.painter().galley(text_pos, galley, color);
    paint::chevron(ui.painter(), Pos2::new(rect.right() - 7.0, rect.center().y), paint::TEXT_WEAK);
    resp
}

fn splitters(ui: &mut egui::Ui, state: &mut AppState, area: Rect, split_x: f32, split_y: f32) {
    let id = ui.scope_id().with("hp_viewport_splitters");
    let v = ui.interact(layout::vertical_splitter_rect(area, split_x), id.with("v"), POINTER_ONLY);
    let h = ui.interact(layout::horizontal_splitter_rect(area, split_y), id.with("h"), POINTER_ONLY);
    let c = ui.interact(layout::cross_rect(area, split_x, split_y), id.with("c"), POINTER_ONLY);
    let pointer = ui.ctx().pointer_latest_pos();
    let mut changed = false;
    let mut save = false;

    let (move_x, move_y) = if c.dragged() || c.hovered() {
        (true, true)
    } else {
        (v.dragged() || v.hovered(), h.dragged() || h.hovered())
    };
    if c.hovered() || c.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Move);
    } else if v.hovered() || v.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeColumn);
    } else if h.hovered() || h.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeRow);
    }
    if let Some(p) = pointer {
        if c.dragged() || v.dragged() {
            state.settings.ui.split_x = split_from_pos(area.left(), area.width(), p.x);
            changed = true;
        }
        if c.dragged() || h.dragged() {
            state.settings.ui.split_y = split_from_pos(area.top(), area.height(), p.y);
            changed = true;
        }
    }
    if c.double_clicked() {
        state.settings.ui.split_x = 0.5;
        state.settings.ui.split_y = 0.5;
        save = true;
    } else if v.double_clicked() {
        state.settings.ui.split_x = 0.5;
        save = true;
    } else if h.double_clicked() {
        state.settings.ui.split_y = 0.5;
        save = true;
    }
    save |= c.drag_stopped() || v.drag_stopped() || h.drag_stopped();
    if save {
        state.settings_dirty = true;
    }
    let x = area.left() + area.width() * clamp_split(state.settings.ui.split_x);
    let y = area.top() + area.height() * clamp_split(state.settings.ui.split_y);
    let accent = hp_app::theme::ACCENT.gamma_multiply(if changed { 0.9 } else { 0.6 });
    let painter = ui.painter();
    if move_x {
        painter.rect_filled(
            Rect::from_center_size(Pos2::new(x, area.center().y), vec2(layout::GAP, area.height())),
            0,
            accent,
        );
    }
    if move_y {
        painter.rect_filled(
            Rect::from_center_size(Pos2::new(area.center().x, y), vec2(area.width(), layout::GAP)),
            0,
            accent,
        );
    }
}

fn drop_material(state: &mut AppState, view: &ViewInfo, target: &Target, name: &str) {
    let faces = if view.is_2d() {
        let objs = state.doc.selected_objects();
        if objs.is_empty() { state.doc.selected_faces() } else { state.doc.all_faces_of(&objs) }
    } else {
        target.face.into_iter().collect()
    };
    if faces.is_empty() {
        let key = if view.is_2d() { "viewport.drop_no_selection" } else { "viewport.drop_no_face" };
        state.notify(Severity::Info, t!(key));
        return;
    }
    match state.doc.apply_material(&faces, name) {
        Ok(n) => {
            state.use_material(name);
            state.log(Severity::Info, tf!("viewport.material_applied", name, n));
        }
        Err(e) => state.notify(Severity::Error, tf!("viewport.edit_failed", e)),
    }
}

fn paste_prefab(
    state: &mut AppState,
    text: &str,
    target: &Target,
    axes: Option<(usize, usize)>,
) -> hp_editor::EditResult<()> {
    state.doc.begin("Paste prefab");
    let result: hp_editor::EditResult<()> = (|| {
        let objs = state.doc.paste(text, DVec3::ZERO)?;
        let bounds = state.doc.selection_bounds(&|e| state.assets.entity_bounds(e));
        if let Some(b) = bounds.filter(|b| !b.is_empty() && b.min.is_finite() && b.max.is_finite()) {
            let mut offset = match axes {
                Some(axes) => placement::box_offset_2d(b.min, b.max, target.point, axes),
                None if target.on_surface => placement::box_rest_offset(b.min, b.max, target.point, target.normal),
                None => target.point - b.center(),
            };
            let keep = placement::aligned_axis(target.normal);
            for k in 0..3 {
                if Some(k) != keep || axes.is_some() {
                    offset[k] = state.snap(offset[k]);
                }
            }
            if offset != DVec3::ZERO && offset.is_finite() {
                state.doc.translate(&objs, offset)?;
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            state.doc.commit();
            Ok(())
        }
        Err(e) => {
            state.doc.cancel();
            Err(e)
        }
    }
}

fn collision_legend(ui: &egui::Ui, rect: Rect) {
    let painter = ui.painter_at(rect);
    let galley = painter.layout_no_wrap(
        t!("viewport.collision_legend").to_owned(),
        egui::FontId::proportional(12.0),
        paint::TEXT,
    );
    let at = Pos2::new(rect.center().x, rect.bottom() - 8.0);
    let pill = Align2::CENTER_BOTTOM.anchor_size(at, galley.size() + vec2(34.0, 8.0)).intersect(rect);
    painter.rect_filled(pill, 6.0, paint::PILL_BG);
    let swatch = |x: f32, color: Color32| {
        let r = Rect::from_center_size(Pos2::new(x, pill.center().y), vec2(9.0, 9.0));
        painter.rect_filled(r, 1.0, color);
    };
    swatch(pill.left() + 10.0, Color32::from_rgb(190, 230, 26));
    swatch(pill.left() + 22.0, Color32::from_rgb(255, 107, 10));
    painter.galley(Pos2::new(pill.left() + 30.0, pill.top() + 4.0), galley, paint::TEXT);
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_hooks;
#[cfg(test)]
mod tests_integration;
#[cfg(test)]
mod tests_logical;
#[cfg(test)]
mod tests_workflow;
#[cfg(test)]
mod trains_tests;
