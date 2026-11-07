mod alpha;
mod edit;
mod error;
mod fallback;
mod four_way;
mod info;
mod material;
mod patch;
mod texture_transform;
mod value;
mod write;

#[cfg(test)]
mod real_data_tests;

pub use alpha::{AlphaUsage, BlendMode, quantize_alpha_reference};
pub use edit::{
    EditOutcome, remove_param_in_text, remove_param_in_text_detailed, set_param_everywhere_in_text, set_param_in_text,
    set_param_in_text_detailed, set_param_with_fallbacks_in_text,
};
pub use error::VmtError;
pub use fallback::DEFAULT_DX_LEVEL;
pub use four_way::{FOUR_WAY_LAYERS, FourWayBlend, parse_vec2};
pub use hp_kv::Block;
pub use info::{CompileFlags, DEFAULT_ALPHA_TEST_REFERENCE, ShaderKind, ToolKind, shader_base_name};
pub use material::{Material, parse, parse_bytes};
pub use patch::{MAX_INCLUDE_DEPTH, normalize_include_path, resolve_patch};
pub use texture_transform::TextureTransform;
pub use value::{normalize_material_name, normalize_texture_path, parse_bool, parse_f32, parse_i32, parse_vec3};
pub use write::{DEFAULT_IMAGE_SHADER, ImageMaterialOptions, for_image, write};
