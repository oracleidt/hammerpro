use crate::Renderer;
use crate::hooks::PassHook;

pub(crate) fn hooks(_renderer: &Renderer) -> Vec<Box<dyn PassHook>> {
    vec![
        Box::new(crate::lightmap::LightmapPass),
        Box::new(crate::compiled_light::CompiledLightPass),
        Box::new(crate::lightmap::post::PostPass::default()),
    ]
}
