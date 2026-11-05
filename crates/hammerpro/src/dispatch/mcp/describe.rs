use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use egui::accesskit::{Node, NodeId, Role, Toggled, TreeUpdate};
use serde_json::{Value, json};

use super::automation::{UiHost, view_name};

const MAX_NODES: usize = 50_000;
const MAX_TEXTS: usize = 300;
const MAX_TEXT_CHARS: usize = 200;
const MAX_TOTAL_CHARS: usize = 60_000;

#[derive(Default)]
struct Capture {
    want: bool,
    ours: bool,
    others: bool,
    tree: Option<TreeUpdate>,
}

#[derive(Default)]
pub(crate) struct AccessPlugin(Arc<Mutex<Capture>>);

impl egui::Plugin for AccessPlugin {
    fn debug_name(&self) -> &'static str {
        "hammerpro_mcp_describe"
    }

    fn output_hook(&mut self, _ctx: &egui::Context, output: &mut egui::FullOutput) {
        let mut c = lock(&self.0);
        let update = output.platform_output.accesskit_update.as_ref();
        if c.want {
            if let Some(u) = update {
                c.tree = Some(u.clone());
                c.want = false;
            }
        } else if !c.ours {
            c.others = update.is_some();
        }
    }
}

fn lock(m: &Mutex<Capture>) -> MutexGuard<'_, Capture> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn capture(ctx: &egui::Context) -> Arc<Mutex<Capture>> {
    ctx.plugin_or_default::<AccessPlugin>().lock().0.clone()
}

pub(super) fn request_tree(ctx: &egui::Context) {
    let cap = capture(ctx);
    let enable = {
        let mut c = lock(&cap);
        c.want = true;
        c.tree = None;
        let enable = !c.others && !c.ours;
        if enable {
            c.ours = true;
        }
        enable
    };
    if enable {
        ctx.enable_accesskit();
    }
}

pub(super) fn tree_ready(ctx: &egui::Context) -> bool {
    lock(&capture(ctx)).tree.is_some()
}

pub(super) fn take_tree(ctx: &egui::Context) -> Option<TreeUpdate> {
    let cap = capture(ctx);
    let (tree, disable) = {
        let mut c = lock(&cap);
        let disable = c.ours && !c.want;
        if disable {
            c.ours = false;
        }
        (c.tree.clone(), disable)
    };
    if disable {
        ctx.disable_accesskit();
    }
    tree
}

pub(super) fn cancel(ctx: &egui::Context) {
    let cap = capture(ctx);
    let disable = {
        let mut c = lock(&cap);
        c.want = false;
        std::mem::take(&mut c.ours)
    };
    if disable {
        ctx.disable_accesskit();
    }
}

struct Container {
    kind: &'static str,
    title: String,
    rect: Option<egui::Rect>,
    labelled: bool,
    texts: Vec<String>,
    controls: Vec<Value>,
}

impl Container {
    fn to_json(&self) -> Value {
        json!({
            "title": self.title,
            "rect": self.rect.map(rect_json),
            "texts": self.texts,
            "controls": self.controls,
        })
    }
}

fn control_role(role: Role) -> Option<&'static str> {
    Some(match role {
        Role::Button => "button",
        Role::Link => "link",
        Role::CheckBox | Role::Switch => "checkbox",
        Role::RadioButton => "radio",
        Role::MenuItem => "menu item",
        Role::MenuItemCheckBox | Role::MenuItemRadio => "menu choice",
        Role::Tab => "tab",
        Role::ComboBox => "combo box",
        Role::TextInput | Role::MultilineTextInput | Role::SearchInput => "text field",
        Role::SpinButton => "number field",
        Role::Slider => "slider",
        _ => return None,
    })
}

fn rect_json(r: egui::Rect) -> Value {
    json!([r.left().round(), r.top().round(), r.width().round(), r.height().round()])
}

fn bounds(node: &Node) -> Option<egui::Rect> {
    let b = node.bounds()?;
    let r = egui::Rect::from_min_max(egui::pos2(b.x0 as f32, b.y0 as f32), egui::pos2(b.x1 as f32, b.y1 as f32));
    (r.is_finite() && r.is_positive()).then_some(r)
}

fn clip(s: &str) -> String {
    let s = s.trim();
    match s.char_indices().nth(MAX_TEXT_CHARS) {
        Some((i, _)) => format!("{}...", &s[..i]),
        None => s.to_owned(),
    }
}

fn node_text(node: &Node) -> Option<String> {
    let label = node.label().map(clip).filter(|s| !s.is_empty());
    let value = node.value().map(clip).filter(|s| !s.is_empty());
    let checked = |label: Option<String>| {
        let mark = match node.toggled() {
            Some(Toggled::True) => "[x]",
            Some(Toggled::Mixed) => "[-]",
            _ => "[ ]",
        };
        Some(format!("{mark} {}", label.unwrap_or_default()))
    };
    let text = match node.role() {
        Role::CheckBox | Role::Switch | Role::RadioButton | Role::MenuItemCheckBox | Role::MenuItemRadio => {
            checked(label)
        }
        Role::TextInput
        | Role::MultilineTextInput
        | Role::SearchInput
        | Role::SpinButton
        | Role::Slider
        | Role::ComboBox => match (label, value) {
            (Some(l), Some(v)) => Some(format!("{l}: {v}")),
            (Some(l), None) => Some(format!("{l}: (empty)")),
            (None, v) => v,
        },
        _ => label.or(value),
    }?;
    let disabled = node.is_disabled() && matches!(node.role(), Role::Button | Role::MenuItem | Role::Tab | Role::Link);
    Some(if disabled { format!("{text} (disabled)") } else { text })
}

fn panel_name(rect: Option<egui::Rect>, window: egui::Rect) -> &'static str {
    let Some(r) = rect else { return "panel" };
    let (w, h) = (window.width(), window.height());
    let near_top = r.top() <= window.top() + h * 0.12;
    if near_top && r.width() >= w * 0.7 && r.height() < h * 0.15 {
        "toolbar"
    } else if r.bottom() >= window.bottom() - 30.0 && r.height() < 48.0 {
        "status bar"
    } else if r.width() >= w * 0.6 && r.top() >= window.top() + h * 0.6 {
        "console"
    } else if r.left() <= window.left() + 60.0 && r.width() < w * 0.1 && r.height() >= h * 0.25 {
        "tool palette"
    } else if r.right() >= window.right() - 30.0 && r.width() < w * 0.45 && r.height() >= h * 0.3 {
        "dock"
    } else if near_top && r.height() < 40.0 {
        "map tabs"
    } else {
        "views"
    }
}

fn walk(tree: &TreeUpdate, window: egui::Rect) -> (Vec<Container>, Value) {
    let nodes: HashMap<NodeId, &Node> = tree.nodes.iter().map(|(id, n)| (*id, n)).collect();
    let Some(root) = tree.tree.as_ref().map(|t| t.root).or_else(|| tree.nodes.first().map(|(id, _)| *id)) else {
        return (Vec::new(), Value::Null);
    };
    let mut out: Vec<Container> = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0usize;
    let mut stack: Vec<(NodeId, Option<usize>)> = vec![(root, None)];
    let mut unions: Vec<Option<egui::Rect>> = Vec::new();
    while let Some((id, parent)) = stack.pop() {
        if seen.len() >= MAX_NODES || !seen.insert(id) {
            continue;
        }
        let Some(node) = nodes.get(&id) else { continue };
        if node.is_hidden() || node.role() == Role::Splitter {
            continue;
        }
        let mut current = parent;
        let kind = match node.role() {
            Role::Window if id != root => Some("window"),
            Role::Dialog => Some("dialog"),
            Role::Menu => Some("menu"),
            Role::Tooltip => Some("tooltip"),
            Role::Pane if parent.is_none() => Some("panel"),
            _ => None,
        };
        if let Some(kind) = kind {
            let label = node.label().map(clip).filter(|s| !s.is_empty());
            let labelled = label.is_some();
            let (rect, title) = (bounds(node), label.unwrap_or_default());
            out.push(Container { kind, title, rect, labelled, texts: Vec::new(), controls: Vec::new() });
            current = Some(out.len() - 1);
            unions.push(None);
        } else if let Some(i) = current
            && let Some(b) = bounds(node)
        {
            unions[i] = Some(unions[i].map_or(b, |u: egui::Rect| u.union(b)));
        }
        if kind.is_none()
            && let Some(i) = current
            && let Some(text) = node_text(node).or_else(|| control_role(node.role()).map(|_| String::new()))
        {
            let c = &mut out[i];
            if total < MAX_TOTAL_CHARS {
                match control_role(node.role()) {
                    Some(role) if c.controls.len() < MAX_TEXTS => {
                        total += text.len() + 32;
                        let at = bounds(node).map(|r| json!([r.center().x.round(), r.center().y.round()]));
                        c.controls.push(json!({ "role": role, "text": text, "at": at }));
                    }
                    Some(_) => {}
                    None => {
                        if c.texts.len() < MAX_TEXTS && c.texts.last() != Some(&text) && text != c.title {
                            total += text.len();
                            c.texts.push(text);
                        }
                    }
                }
            }
        }
        for child in node.children().iter().rev() {
            stack.push((*child, current));
        }
    }
    for (c, union) in out.iter_mut().zip(unions) {
        c.rect = c.rect.or(union).map(|r| r.intersect(window)).filter(|r| r.is_positive());
        if c.kind == "menu" && c.rect.is_some_and(|r| r.top() <= window.top() + 40.0 && r.height() < 40.0) {
            c.kind = "panel";
            c.title = "menu bar".to_owned();
            c.labelled = true;
        }
        if c.kind == "panel" && !c.labelled {
            c.title = panel_name(c.rect, window).to_owned();
        }
    }
    let focused = (tree.focus != root)
        .then(|| nodes.get(&tree.focus))
        .flatten()
        .map(|n| {
            json!({
                "role": format!("{:?}", n.role()),
                "label": n.label().map(clip),
                "value": n.value().map(clip),
                "rect": bounds(n).map(rect_json),
            })
        })
        .unwrap_or(Value::Null);
    (out, focused)
}

pub(super) fn describe(host: &mut dyn UiHost, ctx: &egui::Context, tree: Option<&TreeUpdate>, console: usize) -> Value {
    let window = ctx.viewport_rect();
    let (containers, focused) = tree.map(|t| walk(t, window)).unwrap_or_else(|| (Vec::new(), Value::Null));
    let of = |kinds: &[&str]| -> Vec<Value> {
        containers.iter().filter(|c| kinds.contains(&c.kind)).map(Container::to_json).collect()
    };
    let status_bar: Vec<&str> = containers
        .iter()
        .filter(|c| c.kind == "panel" && c.title == "status bar")
        .flat_map(|c| c.texts.iter().map(String::as_str).chain(c.controls.iter().filter_map(|x| x["text"].as_str())))
        .collect();
    let open_maps = host.open_maps();
    let prompts = host.prompts();
    let rects = host.view_rects();
    let logical = host.logical_rects();
    let state = host.state();
    let ui = &state.ui;
    let flags = [
        ("settings", ui.show_settings),
        ("keyboard_shortcuts", ui.show_shortcuts),
        ("about", ui.show_about),
        ("command_palette", ui.show_command_palette),
        ("compile", ui.show_compile),
        ("problems", ui.show_problems),
        ("find_entities", ui.show_find_entities),
        ("replace_textures", ui.show_replace_textures),
        ("map_properties", ui.show_map_properties),
        ("entity_report", ui.show_entity_report),
        ("map_info", ui.show_map_info),
        ("transform", ui.show_transform),
        ("paste_special", ui.show_paste_special),
        ("array_duplicate", ui.show_array_duplicate),
        ("go_to_brush", ui.show_goto),
        ("scatter_by_material", ui.show_scatter),
        ("material_editor", ui.show_material_editor),
        ("first_run", ui.show_first_run),
        ("shader_editor", state.runtime.shader.show),
        ("vtf_editor", state.runtime.vtf.show),
    ];
    let dialogs: Vec<&str> = flags.iter().filter(|(_, on)| *on).map(|(name, _)| *name).collect();
    let views: Vec<Value> = rects
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let (name, rect) = match (*r, logical[i]) {
                (Some((kind, rect)), _) => (view_name(kind), rect),
                (None, Some(rect)) => ("logical", rect),
                (None, None) => return None,
            };
            Some(json!({
                "index": i,
                "kind": name,
                "rect": rect_json(rect),
                "maximized": ui.maximized_view == Some(i),
                "focused": ui.focused_view == Some(i),
            }))
        })
        .collect();
    let s = &state.settings;
    let panels = json!({
        "dock_tab": format!("{:?}", ui.right_tab).to_ascii_lowercase(),
        "floating_panels": s.ui.floating_panels,
        "texture_browser": s.ui.show_texture_browser,
        "entity_properties": s.ui.show_entity_properties,
        "model_browser": s.ui.show_model_browser,
        "visgroups": s.ui.show_visgroups,
        "outliner": s.ui.show_outliner,
        "console": s.ui.show_console,
        "toolbar": s.workflow.show_toolbar,
        "tool_palette": s.workflow.show_tool_palette,
        "status_bar": s.workflow.show_status_bar,
    });
    let tail: Vec<Value> = state
        .console
        .lines()
        .rev()
        .take(console)
        .rev()
        .map(|l| json!({ "severity": format!("{:?}", l.severity).to_ascii_lowercase(), "text": clip(&l.text) }))
        .collect();
    let map = json!({
        "title": state.map_title(),
        "file": state.doc.path().map(|p| p.display().to_string()),
        "modified": state.doc.is_dirty(),
    });
    let tool = format!("{:?}", state.tool).to_ascii_lowercase();
    let toasts: Vec<Value> = state
        .toasts
        .live()
        .iter()
        .map(|t| json!({ "severity": format!("{:?}", t.severity).to_ascii_lowercase(), "text": clip(&t.text) }))
        .collect();
    json!({
        "window": {
            "width": window.width().round(),
            "height": window.height().round(),
            "pixels_per_point": ctx.pixels_per_point(),
        },
        "map": map,
        "open_maps": open_maps,
        "tool": tool,
        "views": views,
        "panels": panels,
        "dialogs_open": dialogs,
        "separate_windows": separate_windows(),
        "title_bar": title_bar(ctx, host.state()),
        "prompts": prompts,
        "windows": of(&["window", "dialog"]),
        "menus": of(&["menu"]),
        "tooltips": of(&["tooltip"]),
        "panes": of(&["panel"]),
        "status_bar": status_bar,
        "focused": focused,
        "toasts": toasts,
        "console_tail": tail,
        "accessibility_tree": tree.is_some(),
    })
}

fn separate_windows() -> Vec<Value> {
    hp_app::native_window::separate_windows()
        .into_iter()
        .map(|name| {
            let drive = match name {
                "compile" => "compile_map, compile_with_options, cancel_compile, get_compile_status, get_compile_log",
                "uv_editor" => "get_uv_layers, edit_uv_layer, set_face_texture, fit_texture, justify_texture",
                "portals" => "portal_file, set_setting",
                _ => "",
            };
            json!({
                "name": name,
                "title": super::automation::separate_window_title(name),
                "screenshot": format!("screenshot_window with window \"{name}\""),
                "drive": drive,
            })
        })
        .collect()
}

fn title_bar(ctx: &egui::Context, state: &hp_app::AppState) -> Value {
    if state.settings.ui.native_frames {
        return Value::Null;
    }
    let Some(g) = hp_app::chrome::last_geometry(ctx, egui::ViewportId::ROOT) else { return Value::Null };
    let rect = |r: egui::Rect| json!([r.left().round(), r.top().round(), r.width().round(), r.height().round()]);
    let buttons: Vec<Value> =
        g.buttons.iter().map(|(b, r)| json!({ "name": b.name(g.maximized), "rect": rect(*r) })).collect();
    json!({
        "rect": rect(g.title),
        "window_box": rect(g.visible),
        "maximized": g.maximized,
        "round_corners": g.radius > 0.0,
        "buttons": buttons,
        "note": "the menus (File ... MCP) are in the title bar; drag its empty part to move the window",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_of_widgets() {
        let mut n = Node::new(Role::CheckBox);
        n.set_label("Snap");
        n.set_toggled(Toggled::True);
        assert_eq!(node_text(&n).as_deref(), Some("[x] Snap"));
        let mut n = Node::new(Role::TextInput);
        n.set_label("Name");
        assert_eq!(node_text(&n).as_deref(), Some("Name: (empty)"));
        n.set_value("door");
        assert_eq!(node_text(&n).as_deref(), Some("Name: door"));
        let mut n = Node::new(Role::Button);
        n.set_label("OK");
        n.set_disabled();
        assert_eq!(node_text(&n).as_deref(), Some("OK (disabled)"));
        assert_eq!(node_text(&Node::new(Role::GenericContainer)), None);
        let long = "x".repeat(MAX_TEXT_CHARS * 2);
        assert!(clip(&long).len() <= MAX_TEXT_CHARS + 3);
        assert!(clip(&"é".repeat(MAX_TEXT_CHARS * 2)).ends_with("..."));
    }

    #[test]
    fn panels_are_named_by_place() {
        let w = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 960.0));
        let r = |x: f32, y: f32, width: f32, height: f32| {
            Some(egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(width, height)))
        };
        assert_eq!(panel_name(r(8.0, 26.0, 1584.0, 64.0), w), "toolbar");
        assert_eq!(panel_name(r(8.0, 938.0, 1584.0, 20.0), w), "status bar");
        assert_eq!(panel_name(r(53.0, 788.0, 1539.0, 145.0), w), "console");
        assert_eq!(panel_name(r(5.0, 101.0, 34.0, 831.0), w), "tool palette");
        assert_eq!(panel_name(r(1229.0, 99.0, 363.0, 684.0), w), "dock");
        assert_eq!(panel_name(r(50.0, 99.0, 180.0, 21.0), w), "map tabs");
        assert_eq!(panel_name(r(127.0, 129.0, 652.0, 347.0), w), "views");
        assert_eq!(panel_name(None, w), "panel");
    }

    #[test]
    fn a_tree_with_a_cycle_or_missing_nodes_is_walked_safely() {
        let root = NodeId(1);
        let mut r = Node::new(Role::Window);
        r.set_children(vec![NodeId(2), NodeId(99)]);
        let mut win = Node::new(Role::Window);
        win.set_label("Settings");
        win.set_children(vec![NodeId(3), root]);
        let mut label = Node::new(Role::Label);
        label.set_label("Grid size");
        label.set_children(vec![NodeId(2)]);
        let tree = TreeUpdate {
            nodes: vec![(root, r), (NodeId(2), win), (NodeId(3), label)],
            tree: Some(egui::accesskit::TreeInfo::new(root)),
            tree_id: egui::accesskit::TreeId::ROOT,
            focus: NodeId(3),
        };
        let w = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let (containers, focused) = walk(&tree, w);
        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].title, "Settings");
        assert_eq!(containers[0].texts, vec!["Grid size".to_owned()]);
        assert!(containers[0].controls.is_empty());
        assert_eq!(control_role(Role::Button), Some("button"));
        assert_eq!(control_role(Role::Label), None);
        assert_eq!(focused["label"], "Grid size");
    }
}
