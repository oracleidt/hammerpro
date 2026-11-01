use super::*;

fn state() -> AppState {
    crate::register_translations();
    let mut s = hp_app::testing::test_state();
    s.runtime.lab.logic_open = true;
    s
}

#[test]
fn the_window_draws_the_test_map() {
    let mut s = state();
    let mut view = LogicalView::default();
    hp_app::testing::run_ctx(&mut s, |ctx, st| view.show(ctx, st));
    assert_eq!(view.filter(), NodeFilter::All, "every entity by default, like Hammer");
    assert_eq!(view.graph().nodes.len(), 3, "the unnamed info_player_start too");
    assert!(view.graph().index.contains_key(&4));
    assert_eq!(view.graph().edges.len(), 1);
    let t = view.graph().index[&6];
    let l = view.graph().index[&5];
    assert!(view.node_pos(t)[0] < view.node_pos(l)[0]);
    s.runtime.lab.logic_open = false;
    hp_app::testing::run_ctx(&mut s, |ctx, st| view.show(ctx, st));
    assert!(!s.doc.is_dirty());
}

#[test]
fn the_filter_of_a_view_changes_its_nodes() {
    let mut s = state();
    let mut view = LogicalView::default();
    view.set_filter(NodeFilter::NamedAndConnected);
    view.refresh(&s);
    assert_eq!(view.graph().nodes.len(), 2);
    view.set_filter(NodeFilter::All);
    view.refresh(&s);
    assert_eq!(view.graph().nodes.len(), 3);
    hp_app::testing::run_ctx(&mut s, |ctx, st| view.show(ctx, st));
    assert_eq!(view.graph().nodes.len(), 3);
}

#[test]
fn moving_nodes_is_one_undo_step() {
    let mut s = state();
    write_positions(&mut s, UNDO_MOVE, &[(5, [100.0, -200.0]), (6, [0.0, 0.0])]);
    assert_eq!(s.doc.undo_label(), Some(UNDO_MOVE));
    let e = &s.doc.scene().entity(5).expect("light").entity;
    assert_eq!(e.editor.logical_pos.as_deref(), Some("[100 200]"));
    let mut view = LogicalView::default();
    view.refresh(&s);
    let l = view.graph().index[&5];
    assert_eq!(view.node_pos(l), [100.0, -200.0]);
    clear_positions(&mut s, &[5]);
    assert!(s.doc.scene().entity(5).expect("light").entity.editor.logical_pos.is_none());
    assert!(s.doc.undo());
    assert!(s.doc.undo());
    assert!(s.doc.scene().entity(5).expect("light").entity.editor.logical_pos.is_none());
    write_positions(&mut s, UNDO_MOVE, &[(5, [10.0, 20.0])]);
    let text = s.doc.to_vmf_string();
    assert!(text.contains("\"logicalpos\" \"[10 -20]\""), "{text}");
}

#[test]
fn arranging_stores_every_position() {
    let mut s = state();
    let mut view = LogicalView::default();
    view.refresh(&s);
    view.arrange(&mut s);
    assert_eq!(s.doc.undo_label(), Some(UNDO_ARRANGE));
    for id in [5, 6] {
        assert!(s.doc.scene().entity(id).expect("entity").entity.editor.logical_pos.is_some());
    }
}

#[test]
fn new_outputs_name_their_target() {
    let mut s = state();
    let link = NewLink {
        from: 6,
        to: 4,
        output: "OnTrigger".into(),
        input: "Kill".into(),
        param: String::new(),
        delay: 0.5,
        once: true,
        name: "spawn_point".into(),
        needs_name: true,
        at: Pos2::ZERO,
    };
    add_link(&mut s, &link);
    assert_eq!(s.doc.undo_label(), Some("Add output"));
    let trigger = &s.doc.scene().entity(6).expect("trigger").entity;
    let c = trigger.connections.last().expect("output");
    assert_eq!((c.target.as_str(), c.input.as_str(), c.times), ("spawn_point", "Kill", 1));
    assert_eq!(s.doc.scene().entity(4).expect("start").entity.get("targetname"), Some("spawn_point"));
    assert!(s.doc.undo());
    assert!(s.doc.scene().entity(4).expect("start").entity.get("targetname").is_none());
}

#[test]
fn many_entities_stay_fast() {
    let mut s = state();
    let mut map = hp_vmf::Map::new_empty();
    for i in 0..3000u32 {
        let mut e = hp_vmf::Entity { id: 10 + i, classname: "logic_relay".into(), ..Default::default() };
        e.set("targetname", format!("r{i}"));
        e.set("origin", "0 0 0");
        e.connections.push(hp_vmf::Connection {
            output: "OnTrigger".into(),
            target: format!("r{}", (i * 7 + 1) % 3000),
            input: "Trigger".into(),
            ..Default::default()
        });
        map.entities.push(e);
    }
    s.doc = hp_editor::Document::from_map(map);
    let mut view = LogicalView::default();
    let t = std::time::Instant::now();
    hp_app::testing::run_ctx(&mut s, |ctx, st| view.show(ctx, st));
    assert_eq!(view.graph().nodes.len(), 3000);
    assert_eq!(view.graph().edges.len(), 3000);
    assert!(t.elapsed().as_secs_f32() < 5.0, "{:?}", t.elapsed());
    let t = std::time::Instant::now();
    hp_app::testing::run_ctx(&mut s, |ctx, st| view.show(ctx, st));
    assert!(t.elapsed().as_secs_f32() < 2.0, "{:?}", t.elapsed());
}

#[test]
fn french_texts_fit_the_window() {
    let _g = hp_app::testing::lang_lock();
    let mut s = state();
    hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    let mut view = LogicalView::default();
    hp_app::testing::run_ctx(&mut s, |ctx, st| view.show(ctx, st));
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    assert_eq!(view.graph().nodes.len(), 3);
}

#[test]
fn a_search_shows_only_the_matching_entities() {
    let mut s = state();
    let mut view = LogicalView::default();
    hp_app::testing::run_ctx(&mut s, |ctx, st| view.show(ctx, st));
    let l = view.graph().index[&5];
    let t = view.graph().index[&6];
    let class = view.graph().nodes[l].class.clone();
    assert!(!view.graph().nodes[t].class.contains(&class), "the search must tell the two entities apart");
    view.update_shown();
    assert!(view.is_shown(l) && view.is_shown(t));
    assert!(view.graph().edges.iter().all(|e| view.edge_shown(e)));
    view.search = class.to_ascii_uppercase();
    view.update_shown();
    assert!(view.is_shown(l));
    assert!(!view.is_shown(t));
    assert!(view.graph().edges.iter().all(|e| !view.edge_shown(e)));
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
    let hidden_center = view.node_rect(rect, t).center();
    assert_ne!(view.node_at(rect, hidden_center), Some(t));
    hp_app::testing::run_ctx(&mut s, |ctx, st| view.show(ctx, st));
    view.search.clear();
    view.update_shown();
    assert!(view.is_shown(t));
}
