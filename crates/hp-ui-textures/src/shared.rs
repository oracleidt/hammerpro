use std::path::PathBuf;

fn key(name: &str) -> egui::Id {
    egui::Id::unique(("hp_ui_textures", name))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplacePrefill {
    pub find: String,
    pub replace: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditorRequest(pub Vec<String>);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportRequest(pub Vec<PathBuf>);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrowserSelection(pub Vec<String>);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BrowseForFaces(pub bool);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextureLink {
    pub key: String,
    pub texture: String,
}

const TEXTURE_LINK: &str = "texture_link";
const REPLACE: &str = "replace_prefill";
const EDITOR: &str = "editor_request";
const IMPORT: &str = "import_request";
const SELECTION: &str = "browser_selection";
const BROWSE: &str = "browse_for_faces";

pub fn post_replace(ctx: &egui::Context, prefill: ReplacePrefill) {
    ctx.data_mut(|d| d.insert_temp(key(REPLACE), prefill));
}

pub fn take_replace(ctx: &egui::Context) -> Option<ReplacePrefill> {
    ctx.data_mut(|d| d.remove_temp::<ReplacePrefill>(key(REPLACE)))
}

pub fn post_texture_link(ctx: &egui::Context, param: &str, texture: &str) {
    ctx.data_mut(|d| {
        d.insert_temp(key(TEXTURE_LINK), TextureLink { key: param.to_owned(), texture: texture.to_owned() })
    });
}

pub fn take_texture_link(ctx: &egui::Context) -> Option<TextureLink> {
    ctx.data_mut(|d| d.remove_temp::<TextureLink>(key(TEXTURE_LINK)))
}

pub fn post_editor(ctx: &egui::Context, materials: Vec<String>) {
    ctx.data_mut(|d| d.insert_temp(key(EDITOR), EditorRequest(materials)));
}

pub fn take_editor(ctx: &egui::Context) -> Option<EditorRequest> {
    ctx.data_mut(|d| d.remove_temp::<EditorRequest>(key(EDITOR)))
}

pub fn post_import(ctx: &egui::Context, files: Vec<PathBuf>) {
    ctx.data_mut(|d| d.insert_temp(key(IMPORT), ImportRequest(files)));
}

pub fn take_import(ctx: &egui::Context) -> Option<ImportRequest> {
    ctx.data_mut(|d| d.remove_temp::<ImportRequest>(key(IMPORT)))
}

pub fn set_selection(ctx: &egui::Context, names: Vec<String>) {
    ctx.data_mut(|d| d.insert_temp(key(SELECTION), BrowserSelection(names)));
}

pub fn selection(ctx: &egui::Context) -> Vec<String> {
    ctx.data(|d| d.get_temp::<BrowserSelection>(key(SELECTION))).map(|s| s.0).unwrap_or_default()
}

pub fn set_browse_for_faces(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(key(BROWSE), BrowseForFaces(on)));
}

pub fn browse_for_faces(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<BrowseForFaces>(key(BROWSE))).is_some_and(|b| b.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let ctx = egui::Context::default();
        assert!(take_replace(&ctx).is_none());
        post_replace(&ctx, ReplacePrefill { find: "a".into(), replace: "b".into() });
        assert_eq!(take_replace(&ctx), Some(ReplacePrefill { find: "a".into(), replace: "b".into() }));
        assert!(take_replace(&ctx).is_none());

        post_editor(&ctx, vec!["x".into()]);
        assert_eq!(take_editor(&ctx).map(|r| r.0), Some(vec!["x".to_owned()]));
        assert!(take_editor(&ctx).is_none());

        assert!(take_texture_link(&ctx).is_none());
        post_texture_link(&ctx, "$basetexture", "a/b");
        assert_eq!(take_texture_link(&ctx).map(|l| l.texture), Some("a/b".to_owned()));
        assert!(take_texture_link(&ctx).is_none());

        post_import(&ctx, vec![PathBuf::from("a.png")]);
        assert_eq!(take_import(&ctx).map(|r| r.0.len()), Some(1));

        set_selection(&ctx, vec!["m".into()]);
        assert_eq!(selection(&ctx), ["m"]);

        assert!(!browse_for_faces(&ctx));
        set_browse_for_faces(&ctx, true);
        assert!(browse_for_faces(&ctx));
    }
}
