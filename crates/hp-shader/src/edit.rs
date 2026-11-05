use std::collections::HashMap;

use serde_json::{Value, json};

use crate::catalog::{CATEGORIES, NODES, NodeDef, ParamKind, node_def};
use crate::graph::{Graph, default_param};
use crate::presets::PRESETS;

pub const MAX_OPS: usize = 200;

fn node_ref(v: Option<&Value>, aliases: &HashMap<String, u32>, graph: &Graph, what: &str) -> Result<u32, String> {
    let v = v.ok_or_else(|| format!("{what} is required"))?;
    let id = match v {
        Value::Number(n) => {
            n.as_u64().and_then(|x| u32::try_from(x).ok()).ok_or_else(|| format!("{what}: bad node id"))?
        }
        Value::String(s) => *aliases.get(s).ok_or_else(|| format!("{what}: no node named \"{s}\" in this call"))?,
        _ => return Err(format!("{what}: a node id or an alias is expected")),
    };
    if graph.node(id).is_none() {
        return Err(format!("{what}: no node {id}"));
    }
    Ok(id)
}

fn text<'a>(op: &'a Value, key: &str) -> Result<&'a str, String> {
    op.get(key).and_then(Value::as_str).ok_or_else(|| format!("{key} is required (text)"))
}

fn position(v: Option<&Value>) -> Result<Option<[f32; 2]>, String> {
    let Some(v) = v.filter(|v| !v.is_null()) else { return Ok(None) };
    let a = v.as_array().filter(|a| a.len() == 2).ok_or("pos is [x, y]")?;
    let x = a[0].as_f64().ok_or("pos is [x, y]")? as f32;
    let y = a[1].as_f64().ok_or("pos is [x, y]")? as f32;
    Ok(Some([x, y]))
}

fn numbers(v: &Value) -> Result<Vec<f32>, String> {
    match v {
        Value::Number(n) => Ok(vec![n.as_f64().unwrap_or(0.0) as f32]),
        Value::Array(a) => {
            a.iter().map(|x| x.as_f64().map(|f| f as f32).ok_or_else(|| "numbers are expected".to_owned())).collect()
        }
        _ => Err("a number or a list of 1, 3 or 4 numbers is expected".to_owned()),
    }
}

fn apply_one(graph: &mut Graph, aliases: &mut HashMap<String, u32>, op: &Value) -> Result<Value, String> {
    let name = text(op, "op")?;
    match name {
        "add_node" => {
            let kind = text(op, "type")?;
            let pos = match position(op.get("pos"))? {
                Some(p) => p,
                None => {
                    let right = graph.nodes.iter().map(|n| n.pos[0]).fold(f32::MIN, f32::max);
                    [if graph.nodes.is_empty() { 0.0 } else { right + 240.0 }, 0.0]
                }
            };
            let id = graph.add_node(kind, pos)?;
            let fill = (|| -> Result<(), String> {
                if let Some(params) = op.get("params").and_then(Value::as_object) {
                    for (k, v) in params {
                        graph.set_param(id, k, v.clone())?;
                    }
                }
                if let Some(inputs) = op.get("inputs").and_then(Value::as_object) {
                    for (k, v) in inputs {
                        graph.set_input_default(id, k, &numbers(v)?)?;
                    }
                }
                Ok(())
            })();
            if let Err(e) = fill {
                graph.remove_node(id);
                return Err(e);
            }
            if let Some(alias) = op.get("as").and_then(Value::as_str) {
                if alias.is_empty() || alias.len() > 48 || alias.chars().all(|c| c.is_ascii_digit()) {
                    graph.remove_node(id);
                    return Err("as: an alias is a name that is not a number (1 to 48 characters)".to_owned());
                }
                aliases.insert(alias.to_owned(), id);
            }
            Ok(json!({ "op": name, "id": id, "type": kind }))
        }
        "remove_node" => {
            let id = node_ref(op.get("node"), aliases, graph, "node")?;
            graph.remove_node(id);
            aliases.retain(|_, v| *v != id);
            Ok(json!({ "op": name, "id": id }))
        }
        "set_param" => {
            let id = node_ref(op.get("node"), aliases, graph, "node")?;
            if let Some(params) = op.get("params").and_then(Value::as_object) {
                for (k, v) in params {
                    graph.set_param(id, k, v.clone())?;
                }
            } else {
                graph.set_param(id, text(op, "name")?, op.get("value").cloned().ok_or("value is required")?)?;
            }
            Ok(json!({ "op": name, "id": id }))
        }
        "set_input" => {
            let id = node_ref(op.get("node"), aliases, graph, "node")?;
            let pin = text(op, "pin")?;
            match op.get("value") {
                None | Some(Value::Null) => graph.clear_input_default(id, pin),
                Some(v) => graph.set_input_default(id, pin, &numbers(v)?)?,
            }
            Ok(json!({ "op": name, "id": id }))
        }
        "connect" => {
            let from = node_ref(op.get("from"), aliases, graph, "from")?;
            let to = node_ref(op.get("to"), aliases, graph, "to")?;
            graph.connect(from, text(op, "out")?, to, text(op, "in")?)?;
            Ok(json!({ "op": name, "from": from, "to": to }))
        }
        "disconnect" => {
            let to = node_ref(op.get("to"), aliases, graph, "to")?;
            let removed = graph.disconnect(to, text(op, "in")?);
            Ok(json!({ "op": name, "to": to, "removed": removed }))
        }
        "move_node" => {
            let id = node_ref(op.get("node"), aliases, graph, "node")?;
            let pos = position(op.get("pos"))?.ok_or("pos is required")?;
            if let Some(n) = graph.node_mut(id) {
                n.pos = pos;
            }
            Ok(json!({ "op": name, "id": id }))
        }
        "set_size" => {
            let get =
                |k: &str| op.get(k).and_then(Value::as_u64).map(|v| v.min(u64::from(crate::MAX_BAKE_SIZE)) as u32);
            graph.settings.width = get("width").ok_or("width is required (0 means automatic)")?;
            graph.settings.height = get("height").unwrap_or(0);
            Ok(json!({ "op": name, "width": graph.settings.width, "height": graph.settings.height }))
        }
        "clear" => {
            graph.nodes.clear();
            graph.links.clear();
            aliases.clear();
            Ok(json!({ "op": name }))
        }
        other => Err(format!(
            "unknown op \"{other}\" (add_node, remove_node, set_param, set_input, connect, disconnect, move_node, set_size, clear)"
        )),
    }
}

pub fn apply_ops(graph: &mut Graph, ops: &[Value]) -> Result<Vec<Value>, String> {
    if ops.len() > MAX_OPS {
        return Err(format!("at most {MAX_OPS} ops per call"));
    }
    let mut work = graph.clone();
    let mut aliases: HashMap<String, u32> = HashMap::new();
    let mut results = Vec::with_capacity(ops.len());
    for (i, op) in ops.iter().enumerate() {
        match apply_one(&mut work, &mut aliases, op) {
            Ok(r) => results.push(r),
            Err(e) => return Err(format!("op {}: {e}", i + 1)),
        }
    }
    *graph = work;
    Ok(results)
}

fn param_json(p: &crate::catalog::ParamDef) -> Value {
    let default = default_param(p);
    match p.kind {
        ParamKind::Float { min, max, .. } => {
            json!({ "name": p.name, "type": "number", "min": min, "max": max, "default": default })
        }
        ParamKind::Int { min, max, .. } => {
            json!({ "name": p.name, "type": "integer", "min": min, "max": max, "default": default })
        }
        ParamKind::Bool { .. } => json!({ "name": p.name, "type": "boolean", "default": default }),
        ParamKind::Color { .. } => json!({ "name": p.name, "type": "color [r, g, b, a] 0 to 1", "default": default }),
        ParamKind::Choice { options, .. } => {
            json!({ "name": p.name, "type": "choice", "options": options, "default": default })
        }
        ParamKind::Text { .. } => json!({ "name": p.name, "type": "text", "default": default }),
        ParamKind::Vec2 { min, max, .. } => {
            json!({ "name": p.name, "type": "vector [x, y]", "min": min, "max": max, "default": default })
        }
        ParamKind::Gradient => {
            json!({ "name": p.name, "type": "list of {pos 0 to 1, color [r, g, b, a]}, 2 to 16 stops", "default": default })
        }
    }
}

pub fn node_json(def: &NodeDef) -> Value {
    json!({
        "type": def.id,
        "category": def.category,
        "description": def.description,
        "inputs": def.inputs.iter().map(|p| json!({ "name": p.name, "type": p.ty.name(), "default": p.default })).collect::<Vec<_>>(),
        "outputs": def.outputs.iter().map(|p| json!({ "name": p.name, "type": p.ty.name() })).collect::<Vec<_>>(),
        "params": def.params.iter().map(param_json).collect::<Vec<_>>(),
    })
}

pub fn describe_nodes(category: Option<&str>, query: Option<&str>) -> Result<Value, String> {
    if let Some(c) = category
        && !CATEGORIES.contains(&c)
    {
        return Err(format!("no category \"{c}\" (categories: {})", CATEGORIES.join(", ")));
    }
    let q = query.map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty());
    let list: Vec<Value> = NODES
        .iter()
        .filter(|n| category.is_none_or(|c| n.category == c))
        .filter(|n| {
            q.as_ref().is_none_or(|q| n.id.contains(q.as_str()) || n.description.to_lowercase().contains(q.as_str()))
        })
        .map(node_json)
        .collect();
    Ok(json!({ "categories": CATEGORIES, "nodes": list }))
}

pub fn describe_presets() -> Value {
    json!(PRESETS.iter().map(|p| json!({ "id": p.id, "description": p.description })).collect::<Vec<_>>())
}

pub fn describe_graph(graph: &Graph) -> Value {
    let nodes: Vec<Value> = graph
        .nodes
        .iter()
        .map(|n| {
            let def = node_def(&n.kind);
            let params: serde_json::Map<String, Value> = def
                .map(|d| {
                    d.params
                        .iter()
                        .filter_map(|p| graph.param_value(n.id, p.name).map(|v| (p.name.to_owned(), v)))
                        .collect()
                })
                .unwrap_or_default();
            json!({ "id": n.id, "type": n.kind, "pos": n.pos, "params": params, "inputs": n.inputs })
        })
        .collect();
    json!({
        "settings": graph.settings,
        "nodes": nodes,
        "links": graph.links,
        "outputs": graph.output_ids().iter().map(|id| json!({
            "node": id,
            "target": graph.output_target(*id),
            "file_suffix": graph.output_suffix(*id),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_graph_with_aliases() {
        let mut g = Graph::new();
        let ops = vec![
            json!({ "op": "add_node", "type": "texture", "as": "t", "params": { "source": "a.png" } }),
            json!({ "op": "add_node", "type": "blur", "as": "b", "params": { "radius": 3 } }),
            json!({ "op": "add_node", "type": "output", "as": "o" }),
            json!({ "op": "connect", "from": "t", "out": "rgba", "to": "b", "in": "color" }),
            json!({ "op": "connect", "from": "b", "out": "color", "to": "o", "in": "color" }),
            json!({ "op": "set_input", "node": "o", "pin": "alpha", "value": 0.5 }),
            json!({ "op": "set_size", "width": 256 }),
        ];
        let r = apply_ops(&mut g, &ops).unwrap();
        assert_eq!(r.len(), 7);
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.links.len(), 2);
        assert_eq!(g.settings.width, 256);
        assert!(g.nodes[1].pos[0] > g.nodes[0].pos[0], "new nodes are placed to the right");
        assert_eq!(g.input_default(3, "alpha"), Some([0.5, 0.5, 0.5, 1.0]));
        let r = apply_ops(
            &mut g,
            &[json!({ "op": "remove_node", "node": 2 }), json!({ "op": "disconnect", "to": 3, "in": "color" })],
        )
        .unwrap();
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(r[1]["removed"], json!(false), "removing a node removed its links");
    }

    #[test]
    fn a_bad_op_changes_nothing() {
        let mut g = Graph::new();
        let before = g.clone();
        let e = apply_ops(
            &mut g,
            &[json!({ "op": "add_node", "type": "blur" }), json!({ "op": "add_node", "type": "warp_drive" })],
        )
        .unwrap_err();
        assert!(e.starts_with("op 2:"), "{e}");
        assert_eq!(g, before);
        assert!(
            apply_ops(&mut g, &[json!({ "op": "add_node", "type": "blur", "params": { "radius": "x" } })]).is_err()
        );
        assert!(g.nodes.is_empty(), "a failed add leaves no node");
        assert!(apply_ops(&mut g, &[json!({ "op": "connect", "from": "x", "out": "a", "to": 1, "in": "b" })]).is_err());
        assert!(apply_ops(&mut g, &[json!({ "op": "teleport" })]).is_err());
        assert!(apply_ops(&mut g, &vec![json!({ "op": "clear" }); MAX_OPS + 1]).is_err());
        assert!(apply_ops(&mut g, &[json!({ "op": "add_node", "type": "blur", "as": "7" })]).is_err());
    }

    #[test]
    fn the_catalog_is_described() {
        let all = describe_nodes(None, None).unwrap();
        assert_eq!(all["nodes"].as_array().unwrap().len(), NODES.len());
        let blur = describe_nodes(None, Some("blur")).unwrap();
        assert_eq!(blur["nodes"][0]["type"], "blur");
        assert!(describe_nodes(Some("nope"), None).is_err());
        assert!(describe_presets().as_array().unwrap().len() >= 8);
        let g = crate::preset("tint", "a.png").unwrap();
        let d = describe_graph(&g);
        assert_eq!(d["nodes"].as_array().unwrap().len(), 3);
        assert_eq!(d["outputs"][0]["target"], "basetexture");
    }
}
