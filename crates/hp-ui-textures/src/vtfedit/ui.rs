use egui::{Color32, RichText, Sense, Ui};
use hp_app::{AppState, Severity, t, tf, theme};
use hp_vtf::ops::{AlphaSource, HeightSource, ImageFileFormat, NormalAlpha, NormalKernel, ResizeFilter};
use hp_vtf::{EncodeQuality, ImageFormat, MipFilter, flags};

use super::store::Kind;
use super::{Background, Origin, PickKind, VtfEditor, face_suffix, pixels_to_color_image};

const ALPHA_MODES: &[&str] = &["opaque", "constant", "luminance", "red", "green", "blue", "invert", "image"];
const POW2: &[u32] = &[1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096];

impl VtfEditor {
    pub(super) fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        let save_key = crate::util::is_top_dialog(&ctx, ui.layer_id())
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
        self.top_bar(ui, state, &ctx);
        if save_key {
            if self.material_tab.is_some() {
                self.save_material(state);
            } else if self.can_save() {
                self.save_clicked(&ctx, state);
            }
        }
        if self.save.open {
            self.save_form_ui(ui, state, &ctx);
        }
        ui.separator();
        egui::Panel::bottom("hp_vtf_status").resizable(false).frame(egui::Frame::NONE).show(ui, |ui| {
            ui.separator();
            self.status_line(ui);
        });
        if self.loaded.is_none() {
            self.empty_ui(ui, &ctx);
            return;
        }
        if !self.material_editors.is_empty() {
            ui.horizontal_wrapped(|ui| {
                if ui.selectable_label(self.material_tab.is_none(), t!("vtf.tab.texture")).clicked() {
                    self.material_tab = None;
                }
                for (index, (path, editor)) in self.material_editors.iter().enumerate() {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let label = if editor.is_dirty() { format!("{name} *") } else { name.into_owned() };
                    if ui
                        .selectable_label(self.material_tab == Some(index), label)
                        .on_hover_text(path.display().to_string())
                        .clicked()
                    {
                        self.material_tab = Some(index);
                    }
                }
            });
            ui.separator();
        }
        if let Some(index) = self.material_tab {
            let names: Vec<String> = self.material_editors.iter().map(|(p, _)| p.display().to_string()).collect();
            let action = ui
                .push_id("hp_vtf_associated_vmt", |ui| {
                    let (path, editor) = &mut self.material_editors[index];
                    let mut location = |ui: &mut Ui| {
                        ui.weak(path.display().to_string());
                    };
                    editor.ui(ui, state, &names, index, &mut location)
                })
                .inner;
            match action {
                Some(crate::matedit_single::SingleAction::Save) => self.save_material(state),
                Some(crate::matedit_single::SingleAction::Select(index)) => self.material_tab = Some(index),
                None => {}
            }
            return;
        }
        egui::Panel::right("hp_vtf_side").resizable(true).default_size(350.0).min_size(280.0).show(ui, |ui| {
            self.side_ui(ui, state, &ctx);
        });
        self.viewer_ui(ui, &ctx);
    }

    fn can_save(&self) -> bool {
        !self.is_saving() && self.loaded.as_ref().is_some_and(|l| l.doc.is_dirty() && !matches!(l.origin, Origin::New))
    }

    fn save_clicked(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.start_save(ctx, state);
    }

    fn status_line(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if self.is_saving() {
                ui.spinner();
            }
            match &self.status {
                Some((sev, text)) => {
                    let color = match sev {
                        Severity::Error => theme::error(),
                        Severity::Warning => theme::warning(),
                        Severity::Success => theme::success(),
                        _ => ui.visuals().text_color(),
                    };
                    ui.add(egui::Label::new(RichText::new(text).color(color)).truncate());
                }
                None => {
                    ui.weak(t!("vtf.hint"));
                }
            }
        });
    }

    fn empty_ui(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(t!("vtf.empty.title")).strong().size(16.0));
            ui.add_space(6.0);
            ui.label(t!("vtf.empty.body"));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button(t!("vtf.open_file")).clicked() {
                    self.request_pick(ctx, PickKind::OpenFile, None);
                }
                if ui.button(t!("vtf.new_from_image")).clicked() {
                    self.request_pick(ctx, PickKind::NewFromImage, None);
                }
            });
            ui.add_space(8.0);
            ui.weak(t!("vtf.drop_hint"));
        });
    }

    fn top_bar(&mut self, ui: &mut Ui, state: &mut AppState, ctx: &egui::Context) {
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("vtf.texture"));
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.load_text).hint_text(t!("vtf.texture_hint")).desired_width(210.0),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button(t!("vtf.load")).clicked() || enter) && !self.load_text.trim().is_empty() {
                let name = self.load_text.clone();
                self.open_texture(state, &name, Vec::new(), None);
            }
            if ui.button(t!("vtf.open_file")).clicked() {
                self.request_pick(ctx, PickKind::OpenFile, None);
            }
            if ui.button(t!("vtf.new_from_image")).on_hover_text(t!("vtf.new_from_image_tip")).clicked() {
                self.request_pick(ctx, PickKind::NewFromImage, None);
            }
            ui.separator();
            let loaded = self.loaded.is_some();
            let origin_new = self.loaded.as_ref().is_some_and(|l| matches!(l.origin, Origin::New));
            let dirty = self.loaded.as_ref().is_some_and(|l| l.doc.is_dirty());
            if dirty {
                ui.colored_label(theme::warning(), t!("vtf.modified"));
            }
            let save = ui.add_enabled(self.can_save(), egui::Button::new(RichText::new(t!("vtf.save")).strong()));
            if save.on_hover_text(t!("vtf.save_tip")).clicked() {
                self.save_clicked(ctx, state);
            }
            let as_texture = ui.add_enabled(loaded && !self.is_saving(), egui::Button::new(t!("vtf.save_as_texture")));
            if as_texture.on_hover_text(t!("vtf.save_as_texture_tip")).clicked() {
                self.save.open = !self.save.open || origin_new;
                if self.save.name.is_empty()
                    && let Some(l) = &self.loaded
                {
                    self.save.name = match &l.origin {
                        Origin::Texture(n) => n.clone(),
                        _ => l.label.clone(),
                    };
                }
            }
            if ui.add_enabled(loaded && !self.is_saving(), egui::Button::new(t!("vtf.save_as_file"))).clicked() {
                let name = self.loaded.as_ref().map(|l| format!("{}.vtf", leaf(&l.label)));
                self.request_pick(ctx, PickKind::SaveVtf, name);
            }
            let reloadable = self.loaded.as_ref().is_some_and(|l| !matches!(l.origin, Origin::New));
            if ui.add_enabled(reloadable, egui::Button::new(t!("vtf.reload"))).clicked() {
                self.reload(state);
            }
            let ext = super::external_editor_ui(state);
            let can_ext = ext.is_some()
                && self.loaded.as_ref().is_some_and(|l| match &l.origin {
                    Origin::Texture(_) => true,
                    Origin::File(_) => true,
                    Origin::New => false,
                });
            let tip = match &ext {
                Some(e) => tf!("vtf.external_tip", e.label),
                None => t!("vtf.external_none").to_owned(),
            };
            if ui.add_enabled(can_ext, egui::Button::new(t!("vtf.external"))).on_hover_text(tip).clicked()
                && let Some(l) = &self.loaded
            {
                let materials = l.materials.clone();
                match l.origin.clone() {
                    Origin::Texture(name) => {
                        let param = self.loaded.as_ref().and_then(|l| l.param.clone());
                        self.open_external(ctx, state, &name, materials, param);
                    }
                    Origin::File(path) => self.open_external_file(ctx, state, &path),
                    Origin::New => {}
                }
            }
        });
    }

    fn save_form_ui(&mut self, ui: &mut Ui, state: &mut AppState, ctx: &egui::Context) {
        let dirs = state.settings.game.custom_texture_dirs.clone();
        let kind = self.loaded.as_ref().and_then(|l| l.located.as_ref()).map(|l| l.kind);
        let archive = matches!(kind, Some(Kind::Archive | Kind::Generated));
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new(t!("vtf.save_form.title")).strong());
            if dirs.is_empty() && self.save.path.trim().is_empty() {
                ui.weak(t!("vtf.save_form.destination_hint"));
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(t!("vtf.save_form.path"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.save.path)
                        .desired_width(340.0)
                        .hint_text(t!("vtf.save_form.path_hint")),
                );
                if ui.button(t!("common.browse")).clicked() {
                    let name = format!("{}.vtf", leaf(&self.save.name));
                    self.request_pick(ctx, PickKind::SaveTexturePath, Some(name));
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(t!("vtf.save_form.folder"));
                if !dirs.is_empty() {
                    self.save.dest = self.save.dest.min(dirs.len() - 1);
                    let current = crate::util::folder_label(&dirs[self.save.dest].path);
                    egui::ComboBox::from_id_salt("hp_vtf_save_dest").selected_text(current).show_ui(ui, |ui| {
                        for (i, d) in dirs.iter().enumerate() {
                            ui.selectable_value(&mut self.save.dest, i, crate::util::folder_label(&d.path))
                                .on_hover_text(d.path.display().to_string());
                        }
                    });
                }
                ui.label(t!("vtf.save_form.name"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.save.name)
                        .hint_text(t!("vtf.save_form.name_hint"))
                        .desired_width(220.0),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.save.create_vmt, t!("vtf.save_form.vmt"))
                    .on_hover_text(t!("vtf.save_form.vmt_tip"));
                if self.save.create_vmt {
                    egui::ComboBox::from_id_salt("hp_vtf_save_shader").selected_text(self.save.shader.clone()).show_ui(
                        ui,
                        |ui| {
                            for s in ["LightmappedGeneric", "UnlitGeneric", "VertexLitGeneric"] {
                                ui.selectable_value(&mut self.save.shader, s.to_owned(), s);
                            }
                        },
                    );
                }
                ui.checkbox(&mut self.save.overwrite, t!("vtf.save_form.overwrite"));
                let destination =
                    !self.save.path.trim().is_empty() || (!dirs.is_empty() && !self.save.name.trim().is_empty());
                let ok = destination && !self.is_saving();
                if ui.add_enabled(ok, egui::Button::new(RichText::new(t!("vtf.save_form.save")).strong())).clicked() {
                    self.start_save_as_texture(ctx, state);
                }
                if ui.button(t!("common.cancel")).clicked() {
                    self.save.open = false;
                }
            });
            if archive {
                ui.weak(t!("vtf.save_form.archive_note"));
            }
        });
    }

    fn side_ui(&mut self, ui: &mut Ui, state: &mut AppState, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            for (i, key) in ["vtf.tab.info", "vtf.tab.flags", "vtf.tab.ops", "vtf.tab.export"].iter().enumerate() {
                ui.selectable_value(&mut self.view.tab, i, RichText::new(t!(key)).strong());
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().id_salt(("hp_vtf_side_scroll", self.view.tab)).auto_shrink([false, false]).show(
            ui,
            |ui| match self.view.tab {
                0 => self.info_tab(ui, state),
                1 => self.flags_tab(ui),
                2 => self.ops_tab(ui, ctx),
                _ => self.export_tab(ui, ctx),
            },
        );
    }

    fn info_tab(&mut self, ui: &mut Ui, state: &mut AppState) {
        let Some(l) = self.loaded.as_mut() else { return };
        let origin = match &l.origin {
            Origin::Texture(n) => format!(
                "{n}  ({})",
                kind_label(l.located.as_ref().map(|x| x.kind), l.located.as_ref().map_or("", |x| x.label.as_str()))
            ),
            Origin::File(p) => p.display().to_string(),
            Origin::New => t!("vtf.origin_new").to_owned(),
        };
        egui::Grid::new("hp_vtf_info").num_columns(2).spacing([10.0, 5.0]).striped(true).show(ui, |ui| {
            ui.label(t!("vtf.info.source"));
            ui.add(egui::Label::new(origin).wrap());
            ui.end_row();

            ui.label(t!("vtf.info.version"));
            let mut minor = l.doc.version_minor;
            egui::ComboBox::from_id_salt("hp_vtf_version").selected_text(format!("7.{minor}")).show_ui(ui, |ui| {
                for m in 1..=5u32 {
                    ui.selectable_value(&mut minor, m, format!("7.{m}"));
                }
            });
            if minor != l.doc.version_minor {
                let r = l.doc.set_version(minor);
                l.rev += 1;
                if let Err(e) = r {
                    self.status = Some((Severity::Error, e.to_string()));
                }
            }
            ui.end_row();

            ui.label(t!("vtf.info.size"));
            let (w, h) = (l.doc.width(), l.doc.height());
            let pow2 = w.is_power_of_two() && h.is_power_of_two();
            ui.horizontal(|ui| {
                ui.label(format!("{w} x {h}"));
                if !pow2 {
                    ui.colored_label(theme::warning(), t!("vtf.info.not_pow2"))
                        .on_hover_text(t!("vtf.info.not_pow2_tip"));
                }
            });
            ui.end_row();

            ui.label(t!("vtf.info.format"));
            let source = l.doc.source_format().map_or("-", ImageFormat::name);
            ui.label(if l.doc.format.name() == source || l.doc.source_format().is_none() {
                l.doc.format.name().to_owned()
            } else {
                format!("{source} > {}", l.doc.format.name())
            });
            ui.end_row();

            ui.label(t!("vtf.info.mips"));
            ui.label(l.doc.mip_count().to_string());
            ui.end_row();

            ui.label(t!("vtf.info.frames"));
            ui.label(l.doc.frame_count().to_string());
            ui.end_row();

            ui.label(t!("vtf.info.faces"));
            ui.label(l.doc.face_count().to_string());
            ui.end_row();

            ui.label(t!("vtf.info.slices"));
            ui.label(l.doc.depth().to_string());
            ui.end_row();

            if let Some(info) = l.doc.info() {
                ui.label(t!("vtf.info.thumbnail"));
                ui.label(match info.low_res_format {
                    Some(f) => format!("{}x{} {}", info.low_res_width, info.low_res_height, f.name()),
                    None => "-".to_owned(),
                });
                ui.end_row();
                ui.label(t!("vtf.info.file_size"));
                ui.label(l.doc.original().map_or(0, <[u8]>::len).to_string() + " B");
                ui.end_row();
            }

            ui.label(t!("vtf.info.start_frame"));
            let max = l.doc.frame_count().saturating_sub(1) as u16;
            let locked = l.doc.face_count() == 6 && l.doc.version_minor < 5;
            let mut start = l.doc.first_frame.min(max);
            ui.add_enabled(!locked && max > 0, egui::DragValue::new(&mut start).range(0..=max));
            l.doc.first_frame = start;
            ui.end_row();

            ui.label(t!("vtf.info.bump_scale"));
            ui.add(egui::DragValue::new(&mut l.doc.bumpmap_scale).range(0.0..=1000.0).speed(0.05));
            ui.end_row();

            ui.label(t!("vtf.info.reflectivity"));
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!l.doc.auto_reflectivity, |ui| {
                        for c in &mut l.doc.reflectivity {
                            ui.add(egui::DragValue::new(c).range(0.0..=1.0).speed(0.005).fixed_decimals(3));
                        }
                    });
                });
                ui.checkbox(&mut l.doc.auto_reflectivity, t!("vtf.info.reflectivity_auto"))
                    .on_hover_text(t!("vtf.info.reflectivity_auto_tip"));
                if ui.small_button(t!("vtf.info.reflectivity_now")).clicked()
                    && let Some(img) = l.doc.image(0, 0)
                {
                    l.doc.reflectivity = hp_vtf::ops::reflectivity_of(img);
                    l.doc.auto_reflectivity = false;
                }
            });
            ui.end_row();

            ui.label(t!("vtf.info.flags"));
            ui.add(egui::Label::new(flags::names(l.doc.flags).join(", ")).wrap());
            ui.end_row();
        });
        ui.add_space(8.0);
        self.sessions_ui(ui, state);
    }

    fn sessions_ui(&mut self, ui: &mut Ui, _state: &mut AppState) {
        if self.sessions.is_empty() {
            return;
        }
        ui.separator();
        ui.label(RichText::new(t!("vtf.sessions")).strong());
        let mut stop = None;
        for (i, s) in self.sessions.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{} ({})", s.texture, s.editor));
                ui.weak(tf!("vtf.sessions_synced", s.synced));
                if let Some(e) = &s.error {
                    ui.colored_label(theme::error(), e);
                }
                if ui.small_button(t!("vtf.sessions_stop")).clicked() {
                    stop = Some(i);
                }
            });
        }
        if let Some(i) = stop {
            self.stop_session(i);
        }
    }

    fn flags_tab(&mut self, ui: &mut Ui) {
        let Some(l) = self.loaded.as_mut() else { return };
        ui.checkbox(&mut l.doc.recompute_alpha_flags, t!("vtf.flags.auto_alpha"))
            .on_hover_text(t!("vtf.flags.auto_alpha_tip"));
        ui.add_space(4.0);
        for &(bit, id) in flags::NAMED {
            let locked = bit == flags::ENVMAP
                || (l.doc.recompute_alpha_flags && (bit == flags::ONEBITALPHA || bit == flags::EIGHTBITALPHA));
            let mut on = l.doc.flags & bit != 0;
            let label = t!(&format!("vtf.flag.{id}"));
            let r = ui.add_enabled(!locked, egui::Checkbox::new(&mut on, label));
            r.on_hover_text(t!(&format!("vtf.flag.{id}.tip")));
            if on != (l.doc.flags & bit != 0) {
                if on {
                    l.doc.flags |= bit;
                } else {
                    l.doc.flags &= !bit;
                }
                if bit == flags::NORMAL || bit == flags::SSBUMP {
                    l.doc.rebuild |= l.doc.pixels_dirty;
                }
            }
        }
    }

    fn ops_tab(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.loaded.as_ref().is_some_and(|l| l.doc.is_volume()) {
            ui.colored_label(theme::warning(), t!("vtf.ops.volume_note"));
            return;
        }
        egui::CollapsingHeader::new(RichText::new(t!("vtf.ops.resize")).strong()).default_open(true).show(ui, |ui| {
            self.resize_ui(ui);
        });
        egui::CollapsingHeader::new(RichText::new(t!("vtf.ops.format")).strong()).default_open(true).show(ui, |ui| {
            self.format_ui(ui);
        });
        egui::CollapsingHeader::new(RichText::new(t!("vtf.ops.mips")).strong()).default_open(true).show(ui, |ui| {
            self.mips_ui(ui);
        });
        egui::CollapsingHeader::new(RichText::new(t!("vtf.ops.normal")).strong()).show(ui, |ui| {
            self.normal_ui(ui);
        });
        egui::CollapsingHeader::new(RichText::new(t!("vtf.ops.alpha")).strong()).show(ui, |ui| {
            self.alpha_ui(ui, ctx);
        });
        egui::CollapsingHeader::new(RichText::new(t!("vtf.ops.frames")).strong()).show(ui, |ui| {
            self.frames_ui(ui, ctx);
        });
    }

    fn resize_ui(&mut self, ui: &mut Ui) {
        let Some(l) = self.loaded.as_mut() else { return };
        let (w, h) = (l.doc.width(), l.doc.height());
        let mut apply = false;
        ui.horizontal(|ui| {
            ui.label(t!("vtf.ops.width"));
            size_combo(ui, "hp_vtf_w", &mut self.ops.width);
            ui.label(t!("vtf.ops.height"));
            size_combo(ui, "hp_vtf_h", &mut self.ops.height);
        });
        ui.horizontal(|ui| {
            if ui.small_button(t!("vtf.ops.nearest_pow2")).on_hover_text(t!("vtf.ops.nearest_pow2_tip")).clicked() {
                self.ops.width = hp_vtf::ops::nearest_power_of_two(w);
                self.ops.height = hp_vtf::ops::nearest_power_of_two(h);
            }
            if ui.small_button("/2").clicked() {
                self.ops.width = (w / 2).max(1);
                self.ops.height = (h / 2).max(1);
            }
            if ui.small_button("x2").clicked() {
                self.ops.width = (w * 2).min(hp_vtf::MAX_POW2_SIZE);
                self.ops.height = (h * 2).min(hp_vtf::MAX_POW2_SIZE);
            }
        });
        ui.horizontal(|ui| {
            ui.label(t!("vtf.ops.filter"));
            filter_combo(ui, "hp_vtf_filter", &mut self.ops.filter);
            if ui.button(t!("vtf.ops.apply")).clicked() {
                apply = true;
            }
        });
        ui.checkbox(&mut l.doc.resize_to_pow2, t!("vtf.ops.round_on_save"))
            .on_hover_text(t!("vtf.ops.round_on_save_tip"));
        if apply {
            let (nw, nh, f) = (self.ops.width, self.ops.height, self.ops.filter);
            let r = l.doc.resize_all(nw, nh, f);
            self.finish_edit(r, "vtf.status.resized");
        }
    }

    fn format_ui(&mut self, ui: &mut Ui) {
        let Some(l) = self.loaded.as_mut() else { return };
        let mut apply = false;
        ui.horizontal(|ui| {
            ui.label(t!("vtf.ops.target_format"));
            egui::ComboBox::from_id_salt("hp_vtf_format")
                .selected_text(self.ops.new_format.name())
                .width(150.0)
                .show_ui(ui, |ui| {
                    for &f in hp_vtf::ENCODABLE {
                        ui.selectable_value(&mut self.ops.new_format, f, format!("{}  ({})", f.name(), format_note(f)));
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(t!("vtf.ops.quality"));
            for (q, key) in [
                (EncodeQuality::Fast, "vtf.ops.q_fast"),
                (EncodeQuality::Normal, "vtf.ops.q_normal"),
                (EncodeQuality::Best, "vtf.ops.q_best"),
            ] {
                ui.selectable_value(&mut l.doc.quality, q, t!(key));
            }
        });
        if ui.button(t!("vtf.ops.convert")).on_hover_text(t!("vtf.ops.convert_tip")).clicked() {
            apply = true;
        }
        if apply {
            let f = self.ops.new_format;
            let r = l.doc.set_format(f);
            self.finish_edit(r, "vtf.status.converted");
        }
    }

    fn mips_ui(&mut self, ui: &mut Ui) {
        let Some(l) = self.loaded.as_mut() else { return };
        let mut apply = false;
        ui.checkbox(&mut l.doc.mipmaps, t!("vtf.ops.mips_enabled"));
        ui.horizontal(|ui| {
            ui.label(t!("vtf.ops.filter"));
            egui::ComboBox::from_id_salt("hp_vtf_mip_filter")
                .selected_text(mip_filter_label(self.ops.mip_filter))
                .show_ui(ui, |ui| {
                    for f in [MipFilter::FromFlags, MipFilter::Color, MipFilter::Data, MipFilter::Normal] {
                        ui.selectable_value(&mut self.ops.mip_filter, f, mip_filter_label(f));
                    }
                });
            if ui.button(t!("vtf.ops.regenerate")).on_hover_text(t!("vtf.ops.regenerate_tip")).clicked() {
                apply = true;
            }
        });
        if apply {
            let (on, f) = (l.doc.mipmaps, self.ops.mip_filter);
            let r = l.doc.regenerate_mipmaps(on, f);
            self.finish_edit(r, "vtf.status.mips_regenerated");
        }
    }

    fn normal_ui(&mut self, ui: &mut Ui) {
        let n = &mut self.ops.normal;
        egui::Grid::new("hp_vtf_normal").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(t!("vtf.normal.source"));
            egui::ComboBox::from_id_salt("hp_vtf_n_src")
                .selected_text(t!(&format!("vtf.height.{}", n.source.id())))
                .show_ui(ui, |ui| {
                    for s in HeightSource::ALL {
                        ui.selectable_value(&mut n.source, s, t!(&format!("vtf.height.{}", s.id())));
                    }
                });
            ui.end_row();
            ui.label(t!("vtf.normal.kernel"));
            egui::ComboBox::from_id_salt("hp_vtf_n_kernel")
                .selected_text(t!(&format!("vtf.kernel.{}", n.kernel.id())))
                .show_ui(ui, |ui| {
                    for k in NormalKernel::ALL {
                        ui.selectable_value(&mut n.kernel, k, t!(&format!("vtf.kernel.{}", k.id())));
                    }
                });
            ui.end_row();
            ui.label(t!("vtf.normal.scale"));
            ui.add(egui::Slider::new(&mut n.scale, 0.1..=20.0).logarithmic(true));
            ui.end_row();
            ui.label(t!("vtf.normal.alpha"));
            egui::ComboBox::from_id_salt("hp_vtf_n_alpha")
                .selected_text(t!(&format!("vtf.nalpha.{}", n.alpha.id())))
                .show_ui(ui, |ui| {
                    for a in NormalAlpha::ALL {
                        ui.selectable_value(&mut n.alpha, a, t!(&format!("vtf.nalpha.{}", a.id())));
                    }
                });
            ui.end_row();
        });
        ui.checkbox(&mut n.wrap, t!("vtf.normal.wrap"));
        ui.checkbox(&mut n.opengl, t!("vtf.normal.opengl")).on_hover_text(t!("vtf.normal.opengl_tip"));
        ui.checkbox(&mut self.ops.normal_flag, t!("vtf.normal.flag")).on_hover_text(t!("vtf.normal.flag_tip"));
        if ui.button(t!("vtf.normal.generate")).clicked() {
            let opts = self.ops.normal;
            let set_flag = self.ops.normal_flag;
            if let Some(l) = self.loaded.as_mut() {
                let r = l.doc.map_images(|img| hp_vtf::ops::normal_from_height(img, &opts));
                if r.is_ok() && set_flag {
                    l.doc.flags |= flags::NORMAL;
                    l.doc.flags &= !flags::SRGB;
                    if l.doc.format == ImageFormat::Dxt1 && opts.alpha != NormalAlpha::Opaque {
                        l.doc.format = ImageFormat::Dxt5;
                    }
                }
                self.finish_edit(r, "vtf.status.normal_done");
            }
        }
    }

    fn alpha_ui(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let mode = ALPHA_MODES[self.ops.alpha_mode.min(ALPHA_MODES.len() - 1)];
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("hp_vtf_alpha_mode")
                .selected_text(t!(&format!("vtf.alpha.{mode}")))
                .width(200.0)
                .show_ui(ui, |ui| {
                    for (i, m) in ALPHA_MODES.iter().enumerate() {
                        ui.selectable_value(&mut self.ops.alpha_mode, i, t!(&format!("vtf.alpha.{m}")));
                    }
                });
        });
        if mode == "constant" {
            ui.add(egui::Slider::new(&mut self.ops.alpha_value, 0..=255).text(t!("vtf.alpha.value")));
        }
        if ui.button(t!("vtf.ops.apply")).clicked() {
            if mode == "image" {
                self.request_pick(ctx, PickKind::AlphaMask, None);
            } else if let Some(source) = AlphaSource::from_id(mode, self.ops.alpha_value)
                && let Some(l) = self.loaded.as_mut()
            {
                let r = l.doc.map_images(|img| Ok(hp_vtf::ops::set_alpha(img, source)));
                self.finish_edit(r, "vtf.status.alpha_set");
            }
        }
    }

    fn frames_ui(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let Some(l) = self.loaded.as_mut() else { return };
        let filter = self.ops.filter;
        let mut result: Option<(Result<(), hp_vtf::VtfError>, &str)> = None;
        let mut pick: Option<PickKind> = None;
        let mut cube = l.doc.face_count() > 1;
        if ui.checkbox(&mut cube, t!("vtf.frames.cubemap")).on_hover_text(t!("vtf.frames.cubemap_tip")).changed() {
            result = Some((l.doc.set_cubemap(cube), "vtf.status.cubemap_changed"));
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button(t!("vtf.frames.duplicate")).clicked() {
                result = Some((l.doc.add_frame(None, filter), "vtf.status.frame_added"));
            }
            if ui.button(t!("vtf.frames.add_image")).clicked() {
                pick = Some(PickKind::AddFrame);
            }
            let frame = self.view.frame;
            if ui.add_enabled(l.doc.frame_count() > 1, egui::Button::new(t!("vtf.frames.remove"))).clicked() {
                result = Some((l.doc.remove_frame(frame), "vtf.status.frame_removed"));
            }
        });
        if ui.button(t!("vtf.frames.import_here")).on_hover_text(t!("vtf.frames.import_here_tip")).clicked() {
            pick = Some(PickKind::Import);
        }
        if let Some((r, key)) = result {
            self.finish_edit(r, key);
        }
        if let Some(kind) = pick {
            self.request_pick(ctx, kind, None);
        }
    }

    fn export_tab(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.label(t!("vtf.export.intro"));
        ui.horizontal(|ui| {
            ui.label(t!("vtf.export.format"));
            for f in ImageFileFormat::ALL {
                ui.selectable_value(&mut self.ops.export_format, f, f.extension().to_ascii_uppercase());
            }
        });
        let name = self.loaded.as_ref().map(|l| leaf(&l.label).to_owned()).unwrap_or_else(|| "texture".to_owned());
        let ext = self.ops.export_format.extension();
        if ui.button(t!("vtf.export.current")).on_hover_text(t!("vtf.export.current_tip")).clicked() {
            self.request_pick(ctx, PickKind::ExportImage, Some(format!("{name}.{ext}")));
        }
        let many = self.loaded.as_ref().is_some_and(|l| l.doc.frame_count() > 1 || l.doc.face_count() > 1);
        if ui
            .add_enabled(many, egui::Button::new(t!("vtf.export.all")))
            .on_hover_text(t!("vtf.export.all_tip"))
            .clicked()
        {
            self.request_pick(ctx, PickKind::ExportAll, Some(format!("{name}.{ext}")));
        }
        ui.add_space(8.0);
        ui.separator();
        ui.label(t!("vtf.import.intro"));
        if ui.button(t!("vtf.import.replace")).on_hover_text(t!("vtf.import.replace_tip")).clicked() {
            self.request_pick(ctx, PickKind::Import, None);
        }
    }

    fn viewer_ui(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        self.view_toolbar(ui, ctx);
        let Some(l) = self.loaded.as_ref() else { return };
        let key = (
            l.rev,
            self.view.frame,
            self.view.face,
            self.view.mip,
            self.view.slice,
            [self.view.channels.r, self.view.channels.g, self.view.channels.b, self.view.channels.a],
        );
        let stale = self.view.tex.as_ref().is_none_or(|(k, _)| *k != key);
        if stale {
            match self.current_image() {
                Ok(img) => {
                    let shown = hp_vtf::ops::view_channels(&img, self.view.channels);
                    let handle =
                        ctx.load_texture("hp_vtf_view", pixels_to_color_image(&shown), egui::TextureOptions::NEAREST);
                    self.view.pixels = Some(img);
                    self.view.tex = Some((key, handle));
                }
                Err(e) => {
                    self.view.tex = None;
                    self.view.pixels = None;
                    self.status = Some((Severity::Error, e));
                }
            }
        }
        if self.view.checker.is_none() {
            let mut px = Vec::with_capacity(16 * 16);
            for y in 0..16 {
                for x in 0..16 {
                    px.push(if (x / 8 + y / 8) % 2 == 0 { Color32::from_gray(150) } else { Color32::from_gray(105) });
                }
            }
            let img = egui::ColorImage { size: [16, 16], source_size: egui::Vec2::new(16.0, 16.0), pixels: px };
            let opts = egui::TextureOptions {
                magnification: egui::TextureFilter::Nearest,
                minification: egui::TextureFilter::Nearest,
                wrap_mode: egui::TextureWrapMode::Repeat,
                mipmap_mode: None,
            };
            self.view.checker = Some(ctx.load_texture("hp_vtf_checker", img, opts));
        }
        let avail = ui.available_size();
        let (rect, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(38));
        let Some((_, tex)) = &self.view.tex else { return };
        let size = tex.size_vec2();
        let fit = (rect.width() / size.x).min(rect.height() / size.y).clamp(0.01, 64.0) * 0.98;
        let mut scale = if self.view.zoom <= 0.0 { fit } else { self.view.zoom };
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let old = scale;
                scale = (scale * (1.0 + scroll * 0.002)).clamp(0.02, 64.0);
                if let Some(p) = resp.hover_pos() {
                    let center = rect.center() + self.view.pan;
                    let rel = (p - center) / old;
                    self.view.pan = p - rel * scale - rect.center();
                }
                self.view.zoom = scale;
            }
        }
        if resp.dragged() {
            self.view.pan += resp.drag_delta();
        }
        if resp.double_clicked() {
            self.view.zoom = 0.0;
            self.view.pan = egui::Vec2::ZERO;
        }
        let img_rect = egui::Rect::from_center_size(rect.center() + self.view.pan, size * scale);
        match self.view.background {
            Background::Checker => {
                if let Some(c) = &self.view.checker {
                    let clipped = img_rect.intersect(rect);
                    if clipped.is_positive() {
                        let min_uv = egui::pos2(
                            (clipped.min.x - img_rect.min.x) / 16.0,
                            (clipped.min.y - img_rect.min.y) / 16.0,
                        );
                        let max_uv = egui::pos2(
                            (clipped.max.x - img_rect.min.x) / 16.0,
                            (clipped.max.y - img_rect.min.y) / 16.0,
                        );
                        painter.image(c.id(), clipped, egui::Rect::from_min_max(min_uv, max_uv), Color32::WHITE);
                    }
                }
            }
            Background::Black => {
                painter.rect_filled(img_rect, 0.0, Color32::BLACK);
            }
            Background::White => {
                painter.rect_filled(img_rect, 0.0, Color32::WHITE);
            }
            Background::Gray => {
                painter.rect_filled(img_rect, 0.0, Color32::from_gray(128));
            }
        }
        painter.image(
            tex.id(),
            img_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.rect_stroke(img_rect, 0.0, egui::Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Outside);
        if let (Some(p), Some(img)) = (resp.hover_pos(), &self.view.pixels)
            && img_rect.contains(p)
        {
            let x = (((p.x - img_rect.min.x) / scale) as u32).min(img.width - 1);
            let y = (((p.y - img_rect.min.y) / scale) as u32).min(img.height - 1);
            let i = ((y * img.width + x) * 4) as usize;
            let px = &img.pixels[i..i + 4];
            let text = format!("{x}, {y}   R {} G {} B {} A {}   {:.0}%", px[0], px[1], px[2], px[3], scale * 100.0);
            let font = egui::FontId::monospace(12.0);
            let galley = painter.layout_no_wrap(text, font, Color32::WHITE);
            let pos = rect.left_bottom() + egui::vec2(6.0, -galley.size().y - 6.0);
            painter.rect_filled(
                egui::Rect::from_min_size(pos, galley.size()).expand(3.0),
                3.0,
                Color32::from_black_alpha(190),
            );
            painter.galley(pos, galley, Color32::WHITE);
        }
    }

    fn view_toolbar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let Some(l) = self.loaded.as_ref() else { return };
        let (frames, faces, mips, depth) = (l.doc.frame_count(), l.doc.face_count(), l.doc.mip_count(), l.doc.depth());
        self.view.mip = self.view.mip.min(mips.saturating_sub(1));
        self.view.frame = self.view.frame.min(frames.saturating_sub(1));
        self.view.face = self.view.face.min(faces.saturating_sub(1));
        self.view.slice = self.view.slice.min(depth.saturating_sub(1));
        ui.horizontal_wrapped(|ui| {
            let ch = &mut self.view.channels;
            ui.label(t!("vtf.view.channels"));
            ui.toggle_value(&mut ch.r, "R").on_hover_text(t!("vtf.view.red"));
            ui.toggle_value(&mut ch.g, "G").on_hover_text(t!("vtf.view.green"));
            ui.toggle_value(&mut ch.b, "B").on_hover_text(t!("vtf.view.blue"));
            ui.toggle_value(&mut ch.a, "A").on_hover_text(t!("vtf.view.alpha"));
            if ui.small_button(t!("vtf.view.alpha_only")).on_hover_text(t!("vtf.view.alpha_only_tip")).clicked() {
                *ch = hp_vtf::ops::Channels { r: false, g: false, b: false, a: true };
            }
            if ui.small_button(t!("vtf.view.rgb")).clicked() {
                *ch = hp_vtf::ops::Channels { r: true, g: true, b: true, a: false };
            }
            if ui.small_button(t!("vtf.view.all")).clicked() {
                *ch = hp_vtf::ops::Channels::default();
            }
            ui.separator();
            if mips > 1 {
                ui.label(t!("vtf.view.mip"));
                let (w, h) = ((l.doc.width() >> self.view.mip).max(1), (l.doc.height() >> self.view.mip).max(1));
                egui::ComboBox::from_id_salt("hp_vtf_mip")
                    .selected_text(format!("{} ({w}x{h})", self.view.mip))
                    .show_ui(ui, |ui| {
                        for m in 0..mips {
                            let (w, h) = ((l.doc.width() >> m).max(1), (l.doc.height() >> m).max(1));
                            ui.selectable_value(&mut self.view.mip, m, format!("{m} ({w}x{h})"));
                        }
                    });
            }
            if frames > 1 {
                ui.label(t!("vtf.view.frame"));
                let mut f = self.view.frame;
                ui.add(egui::DragValue::new(&mut f).range(0..=frames - 1));
                self.view.frame = f;
                ui.label(format!("/ {}", frames - 1));
                ui.toggle_value(&mut self.view.playing, t!("vtf.view.play"));
            }
            if faces > 1 {
                ui.label(t!("vtf.view.face"));
                egui::ComboBox::from_id_salt("hp_vtf_face")
                    .selected_text(t!(&format!("vtf.face.{}", face_suffix(self.view.face))))
                    .show_ui(ui, |ui| {
                        for f in 0..faces {
                            ui.selectable_value(&mut self.view.face, f, t!(&format!("vtf.face.{}", face_suffix(f))));
                        }
                    });
            }
            if depth > 1 {
                ui.label(t!("vtf.view.slice"));
                let mut s = self.view.slice;
                ui.add(egui::DragValue::new(&mut s).range(0..=depth - 1));
                self.view.slice = s;
            }
            ui.separator();
            if ui.small_button(t!("vtf.view.fit")).clicked() {
                self.view.zoom = 0.0;
                self.view.pan = egui::Vec2::ZERO;
            }
            if ui.small_button("100%").clicked() {
                self.view.zoom = 1.0;
                self.view.pan = egui::Vec2::ZERO;
            }
            if ui.small_button("200%").clicked() {
                self.view.zoom = 2.0;
                self.view.pan = egui::Vec2::ZERO;
            }
            egui::ComboBox::from_id_salt("hp_vtf_bg")
                .selected_text(bg_label(self.view.background))
                .width(90.0)
                .show_ui(ui, |ui| {
                    for b in [Background::Checker, Background::Black, Background::White, Background::Gray] {
                        ui.selectable_value(&mut self.view.background, b, bg_label(b));
                    }
                });
        });
        if self.view.playing && frames > 1 {
            let now = ctx.input(|i| i.time);
            let step = 1.0 / f64::from(self.view.fps.max(1.0));
            if now - self.view.last_step >= step {
                self.view.frame = (self.view.frame + 1) % frames;
                self.view.last_step = now;
            }
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(step));
        }
    }
}

fn leaf(label: &str) -> &str {
    let l = label.rsplit(['/', '\\']).next().unwrap_or(label);
    l.strip_suffix(".vtf").unwrap_or(l)
}

fn bg_label(b: Background) -> &'static str {
    match b {
        Background::Checker => t!("vtf.bg.checker"),
        Background::Black => t!("vtf.bg.black"),
        Background::White => t!("vtf.bg.white"),
        Background::Gray => t!("vtf.bg.gray"),
    }
}

fn mip_filter_label(f: MipFilter) -> &'static str {
    match f {
        MipFilter::FromFlags => t!("vtf.mipfilter.flags"),
        MipFilter::Color => t!("vtf.mipfilter.color"),
        MipFilter::Data => t!("vtf.mipfilter.data"),
        MipFilter::Normal => t!("vtf.mipfilter.normal"),
    }
}

fn kind_label(kind: Option<Kind>, label: &str) -> String {
    match kind {
        Some(Kind::Disk) => t!("vtf.kind.disk").to_owned(),
        Some(Kind::Archive) => tf!("vtf.kind.archive", label),
        Some(Kind::Generated) => t!("vtf.kind.generated").to_owned(),
        Some(Kind::Missing) | None => t!("vtf.kind.missing").to_owned(),
    }
}

fn format_note(f: ImageFormat) -> &'static str {
    match f {
        ImageFormat::Dxt1 => t!("vtf.fmt.dxt1"),
        ImageFormat::Dxt3 => t!("vtf.fmt.dxt3"),
        ImageFormat::Dxt5 => t!("vtf.fmt.dxt5"),
        ImageFormat::Ati1n => t!("vtf.fmt.ati1n"),
        ImageFormat::Ati2n => t!("vtf.fmt.ati2n"),
        ImageFormat::Bgra8888 | ImageFormat::Rgba8888 | ImageFormat::Abgr8888 | ImageFormat::Argb8888 => {
            t!("vtf.fmt.raw_alpha")
        }
        ImageFormat::Bgrx8888 | ImageFormat::Bgr888 | ImageFormat::Rgb888 => t!("vtf.fmt.raw"),
        ImageFormat::Bgr565 | ImageFormat::Rgb565 | ImageFormat::Bgra4444 | ImageFormat::Bgra5551 => {
            t!("vtf.fmt.small")
        }
        ImageFormat::Ia88 | ImageFormat::I8 | ImageFormat::A8 => t!("vtf.fmt.gray"),
        ImageFormat::Uv88 => t!("vtf.fmt.uv"),
        _ => "",
    }
}

fn size_combo(ui: &mut Ui, id: &str, value: &mut u32) {
    egui::ComboBox::from_id_salt(id).selected_text(value.to_string()).width(70.0).show_ui(ui, |ui| {
        for &p in POW2 {
            ui.selectable_value(value, p, p.to_string());
        }
    });
    ui.add(egui::DragValue::new(value).range(1..=hp_vtf::MAX_POW2_SIZE).speed(1.0));
}

fn filter_combo(ui: &mut Ui, id: &str, value: &mut ResizeFilter) {
    egui::ComboBox::from_id_salt(id).selected_text(t!(&format!("vtf.rfilter.{}", value.id()))).show_ui(ui, |ui| {
        for f in ResizeFilter::ALL {
            ui.selectable_value(value, f, t!(&format!("vtf.rfilter.{}", f.id())));
        }
    });
}
