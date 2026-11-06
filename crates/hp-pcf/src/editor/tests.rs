use super::*;

#[test]
fn editing_roundtrip_history_and_stable_ids() {
    let mut doc = Document::new();
    let system = doc.create_system("test_effect").unwrap();
    let emit = doc.add_operator(system, Category::Emitters, "emit_continuously").unwrap();
    doc.set_attr(emit, "emission_rate", Value::Float(24.0)).unwrap();
    let initial = doc.to_bytes().unwrap();
    doc.mark_saved();
    doc.set_attr(emit, "emission_rate", Value::Float(48.0)).unwrap();
    assert!(doc.is_modified());
    assert!(doc.undo());
    assert!(!doc.is_modified());
    assert_eq!(doc.to_bytes().unwrap(), initial);
    assert!(doc.redo());
    assert_eq!(doc.element(emit).unwrap().float("emission_rate"), Some(48.0));
    let parsed = Document::parse(&doc.to_bytes().unwrap()).unwrap();
    assert_eq!(parsed.dmx(), doc.dmx());
    assert_eq!(parsed.definitions()[0].emitters[0].float("emission_rate"), Some(48.0));
}

#[test]
fn binary_all_preserved_types_and_external_references() {
    let mut doc = Document::new();
    let id = doc.create_system("roundtrip").unwrap();
    let mut values = vec![
        Value::Element(None),
        Value::Element(Some(id)),
        Value::External("foreign-id".into()),
        Value::References(vec![
            Reference::Element(Some(id)),
            Reference::Element(None),
            Reference::External("external-guid".into()),
        ]),
        Value::Int(-214),
        Value::Float(0.125),
        Value::Bool(true),
        Value::Str("with unicode é".into()),
        Value::Color([255, 0, 128, 64]),
        Value::Vec2([1.0, -1.0]),
        Value::Vec3([1.0, 2.0, 3.0]),
        Value::Vec4([1.0, 2.0, 3.0, 4.0]),
        Value::Elements(vec![Some(id), None]),
        Value::Ints(vec![-1, 0, 1]),
        Value::Floats(vec![1.0, -5.0]),
        Value::Vec3s(vec![[1.0; 3], [2.0; 3]]),
        Value::Strs(vec!["hello".into(), "".into()]),
        Value::Raw { kind: 6, data: vec![3, 0, 0, 0, 1, 2, 3] },
        Value::Raw { kind: 7, data: 1523i32.to_le_bytes().to_vec() },
        Value::Raw { kind: 12, data: [1f32, 2.0, 3.0].into_iter().flat_map(f32::to_le_bytes).collect() },
        Value::Raw { kind: 13, data: [1f32, 2.0, 3.0, 4.0].into_iter().flat_map(f32::to_le_bytes).collect() },
        Value::Raw { kind: 14, data: [1f32; 16].into_iter().flat_map(f32::to_le_bytes).collect() },
        Value::Raw { kind: 20, data: vec![2, 0, 0, 0, 1, 0, 0, 0, 255, 0, 0, 0, 0] },
    ];
    for (kind, size) in [(18, 1), (21, 4), (22, 4), (23, 8), (25, 16), (26, 12), (27, 16), (28, 64)] {
        let mut data = 2i32.to_le_bytes().to_vec();
        data.extend(vec![0; size * 2]);
        values.push(Value::Raw { kind, data });
    }
    for (i, value) in values.into_iter().enumerate() {
        doc.set_attr(id, &format!("attr{i}"), value).unwrap();
    }
    let bytes = doc.to_bytes().unwrap();
    assert!(bytes.starts_with(b"<!-- dmx encoding binary 5 format pcf 2 -->"));
    let parsed = Document::parse(&bytes).unwrap();
    assert_eq!(parsed.dmx(), doc.dmx());
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
    assert!(doc.set_attr(id, "invalid_raw", Value::Raw { kind: 14, data: vec![0; 4] }).is_err());
    assert!(doc.set_attr(id, "invalid_ref", Value::Element(Some(usize::MAX))).is_err());
}

#[test]
fn duplicate_child_links_copy_and_reorder_are_independent() {
    let mut doc = Document::new();
    let parent = doc.create_system("parent").unwrap();
    let child = doc.create_system("child").unwrap();
    doc.add_child(parent, child, 0.25).unwrap();
    assert!(matches!(doc.add_child(child, parent, 0.0), Err(EditError::ChildCycle)));
    let op = doc.add_operator(parent, Category::Operators, "Custom Unsupported Source Operator").unwrap();
    doc.set_attr(op, "custom", Value::Float(9.0)).unwrap();
    let copy = doc.duplicate_system(parent, "parent_copy").unwrap();
    let copied_op = doc.operators(copy, Category::Operators)[0];
    assert_ne!(op, copied_op);
    doc.set_attr(copied_op, "custom", Value::Float(12.0)).unwrap();
    assert_eq!(doc.element(op).unwrap().float("custom"), Some(9.0));
    let copied_child = doc.children(copy)[0];
    assert_eq!(doc.element(copied_child).unwrap().elements("child"), vec![child]);
    doc.copy_operator(op).unwrap();
    let pasted = doc.paste_operator(copy, Category::Operators).unwrap();
    doc.move_operator(copy, Category::Operators, 1, 0).unwrap();
    assert_eq!(doc.operators(copy, Category::Operators)[0], pasted);
    doc.delete_system(child).unwrap();
    assert!(doc.children(parent).is_empty());
    assert!(doc.children(copy).is_empty());
    assert!(doc.undo());
    assert_eq!(doc.system_name(child), Some("child"));
    assert!(doc.validate().iter().all(|i| i.severity != Severity::Error));
    Document::parse(&doc.to_bytes().unwrap()).unwrap();
}

#[test]
fn importing_renames_collisions_and_remaps_children() {
    let mut source = Document::preset(Preset::Smoke);
    let parent = source.systems()[0];
    let child = source.create_system("smoke_2").unwrap();
    source.add_child(parent, child, 0.0).unwrap();
    let mut dest = Document::preset(Preset::Smoke);
    let imported = dest.import(&source).unwrap();
    assert_eq!(dest.system_name(imported[0]), Some("smoke_2"));
    assert_eq!(dest.system_name(imported[1]), Some("smoke_2_2"));
    let wrapper = dest.children(imported[0])[0];
    assert_eq!(dest.element(wrapper).unwrap().elements("child"), vec![imported[1]]);
    assert_eq!(dest.systems().len(), 3);
    assert_eq!(dest.dmx().ids.iter().collect::<HashSet<_>>().len(), dest.dmx().ids.len());
    assert!(dest.undo());
    assert_eq!(dest.systems().len(), 1);
    assert!(dest.redo());
    assert_eq!(dest.systems().len(), 3);
}

#[test]
fn preset_definitions_simulate_and_roundtrip() {
    for preset in Preset::ALL {
        let doc = Document::preset(preset);
        assert!(doc.validate().is_empty(), "{:?}", doc.validate());
        let defs = doc.definitions();
        let plan = crate::Plan::new(&defs[0]);
        assert!(plan.unsupported.is_empty(), "{:?}", plan.unsupported);
        let parsed = Document::parse(&doc.to_bytes().unwrap()).unwrap();
        assert_eq!(parsed.definitions(), defs);
    }
}

#[test]
fn manifest_keeps_entries_and_deduplicates() {
    let initial = update_manifest("", "particles/a.pcf").unwrap();
    let appended = update_manifest(&initial, "particles/b.pcf").unwrap();
    assert!(appended.contains("particles/a.pcf"));
    assert!(appended.contains("particles/b.pcf"));
    assert_eq!(update_manifest(&appended, "particles/A.pcf").unwrap(), appended);
    assert!(update_manifest("invalid", "particles/a.pcf").is_err());
}

#[test]
fn operator_templates_add_as_one_undoable_edit() {
    let mut doc = Document::new();
    let id = doc.create_system("effect").unwrap();
    let revision = doc.revision();
    let op = doc
        .add_operator_with_params(
            id,
            Category::Emitters,
            "emit_continuously",
            vec![("emission_rate".into(), Value::Float(12.0))],
        )
        .unwrap();
    assert_eq!(doc.revision(), revision + 1);
    assert_eq!(doc.element(op).unwrap().float("emission_rate"), Some(12.0));
    assert!(doc.undo());
    assert!(doc.operators(id, Category::Emitters).is_empty());
    assert_eq!(doc.undo_label(), Some("Create system"));
    assert!(
        doc.add_operator_with_params(
            id,
            Category::Emitters,
            "emit_continuously",
            vec![("functionName".into(), Value::Str("wrong".into()))]
        )
        .is_err()
    );
}

#[test]
fn supported_sequences_and_unsupported_bone_locks_are_reported() {
    let mut doc = Document::new();
    let system = doc.create_system("unsupported_preview").unwrap();
    doc.add_operator(system, Category::Initializers, "sequence two random").unwrap();
    doc.add_operator(system, Category::Operators, "movement lock to control point").unwrap();
    doc.add_operator(system, Category::Operators, "movement lock to bone").unwrap();
    let plan = crate::Plan::new(&doc.definitions()[0]);
    assert_eq!(plan.unsupported, vec!["movement lock to bone"]);
    let issues = doc.validate();
    assert_eq!(issues.iter().filter(|i| i.message.contains("no preview implementation")).count(), 1);
    assert_eq!(Document::parse(&doc.to_bytes().unwrap()).unwrap().definitions(), doc.definitions());
}

#[test]
fn parameter_batches_are_atomic_and_one_undo_step() {
    let mut doc = Document::new();
    let system = doc.create_system("curve").unwrap();
    let revision = doc.revision();
    doc.set_attrs(system, vec![("start_time".into(), Value::Float(0.25)), ("end_time".into(), Value::Float(0.75))])
        .unwrap();
    assert_eq!(doc.revision(), revision + 1);
    assert_eq!(doc.element(system).unwrap().float("end_time"), Some(0.75));
    assert!(doc.undo());
    assert!(doc.element(system).unwrap().get("start_time").is_none());
    assert!(doc.element(system).unwrap().get("end_time").is_none());
    assert!(
        doc.set_attrs(system, vec![("start_time".into(), Value::Float(0.5)), ("bad".into(), Value::Float(f32::NAN))])
            .is_err()
    );
    assert!(doc.element(system).unwrap().get("start_time").is_none());
    assert!(
        doc.set_attrs(system, vec![("Start".into(), Value::Float(0.5)), ("start".into(), Value::Float(0.75))]).is_err()
    );
}

#[test]
fn authoring_uses_native_parameter_types_and_preserves_undo() {
    let mut doc = Document::new();
    let system = doc.create_system("native_types").unwrap();
    let renderer = doc
        .add_operator_with_params(
            system,
            Category::Renderers,
            "render_animated_sprites",
            vec![("animation rate".into(), Value::Int(2)), ("use animation rate as FPS".into(), Value::Int(1))],
        )
        .unwrap();
    assert_eq!(doc.element(renderer).unwrap().get("animation rate"), Some(&Value::Float(2.0)));
    assert_eq!(doc.element(renderer).unwrap().get("use animation rate as FPS"), Some(&Value::Bool(true)));
    let sequence = doc
        .add_operator_with_params(
            system,
            Category::Initializers,
            "Sequence Random",
            vec![("sequence_min".into(), Value::Float(1.0))],
        )
        .unwrap();
    assert_eq!(doc.element(sequence).unwrap().get("sequence_min"), Some(&Value::Int(1)));
    let revision = doc.revision();
    assert!(matches!(
        doc.set_attrs(
            renderer,
            vec![("animation rate".into(), Value::Float(4.0)), ("use animation rate as FPS".into(), Value::Int(2)),]
        ),
        Err(EditError::ParameterType { .. })
    ));
    assert_eq!(doc.revision(), revision);
    assert_eq!(doc.element(renderer).unwrap().get("animation rate"), Some(&Value::Float(2.0)));
    doc.set_attr(renderer, "use animation rate as FPS", Value::Float(0.0)).unwrap();
    assert_eq!(doc.element(renderer).unwrap().get("use animation rate as FPS"), Some(&Value::Bool(false)));
    assert!(doc.undo());
    assert_eq!(doc.element(renderer).unwrap().get("use animation rate as FPS"), Some(&Value::Bool(true)));
    assert_eq!(Document::parse(&doc.to_bytes().unwrap()).unwrap().dmx(), doc.dmx());
}

#[test]
fn incorrect_imported_types_are_reported_without_rewriting_the_file() {
    let mut doc = Document::new();
    let system = doc.create_system("imported").unwrap();
    let renderer = doc.add_operator(system, Category::Renderers, "render_animated_sprites").unwrap();
    put(&mut doc.data.elements[renderer], "use animation rate as FPS", Value::Int(1));
    let bytes = doc.to_bytes().unwrap();
    let parsed = Document::parse(&bytes).unwrap();
    assert!(parsed.validate().iter().any(|issue| {
        issue.element == renderer
            && issue.attribute.as_deref() == Some("use animation rate as FPS")
            && issue.message.contains("DMX type bool")
    }));
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
    assert!(!parsed.is_modified());
}
