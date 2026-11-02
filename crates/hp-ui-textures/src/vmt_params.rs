pub mod fam {
    pub const LM: u16 = 1;
    pub const VL: u16 = 1 << 1;
    pub const UN: u16 = 1 << 2;
    pub const WA: u16 = 1 << 3;
    pub const RE: u16 = 1 << 4;
    pub const SP: u16 = 1 << 5;
    pub const DE: u16 = 1 << 6;
    pub const SK: u16 = 1 << 7;
    pub const B4: u16 = 1 << 8;
    pub const ALL: u16 = u16::MAX;
    pub const LIT: u16 = LM | VL;
}

pub const SHADERS: &[(&str, u16)] = &[
    ("LightmappedGeneric", fam::LM),
    ("VertexLitGeneric", fam::VL),
    ("UnlitGeneric", fam::UN),
    ("UnlitTwoTexture", fam::UN),
    ("WorldVertexTransition", fam::LM),
    ("Lightmapped_4WayBlend", fam::LM | fam::B4),
    ("Water", fam::WA),
    ("Refract", fam::RE),
    ("Sprite", fam::SP),
    ("SpriteCard", fam::SP),
    ("DecalModulate", fam::DE),
    ("Cloud", fam::SK),
    ("Sky", fam::SK),
    ("Patch", fam::ALL),
    ("Wireframe", fam::UN),
];

pub fn family_of(shader: &str) -> u16 {
    let base = hp_vmt::shader_base_name(shader);
    SHADERS.iter().find(|(s, _)| s.eq_ignore_ascii_case(&base)).map_or(fam::ALL, |(_, f)| *f)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Texture,
    Flag,
    Float { min: f32, max: f32 },
    Int { min: i32, max: i32 },
    Color,
    Vec3,
    Choice(&'static [(&'static str, &'static str, &'static str)]),
    Text,
    SurfaceProp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Base,
    Transparency,
    Bump,
    Detail,
    Reflection,
    Phong,
    RimLight,
    SelfIllum,
    Lighting,
    Treesway,
    Water,
    Refract,
    Sprite,
    Compile,
    Other,
}

impl Category {
    pub const ALL: [Category; 15] = [
        Category::Base,
        Category::Transparency,
        Category::Bump,
        Category::Detail,
        Category::Reflection,
        Category::Phong,
        Category::RimLight,
        Category::SelfIllum,
        Category::Lighting,
        Category::Treesway,
        Category::Water,
        Category::Refract,
        Category::Sprite,
        Category::Compile,
        Category::Other,
    ];

    pub fn label(self) -> (&'static str, &'static str) {
        match self {
            Category::Base => ("Base texture", "Texture de base"),
            Category::Transparency => ("Transparency", "Transparence"),
            Category::Bump => ("Bump map", "Relief (bump)"),
            Category::Detail => ("Detail", "Détail"),
            Category::Reflection => ("Reflections", "Reflets"),
            Category::Phong => ("Phong", "Phong"),
            Category::RimLight => ("Rim light", "Lumière de contour"),
            Category::SelfIllum => ("Self-illumination", "Auto-illumination"),
            Category::Lighting => ("Lighting and surface", "Éclairage et surface"),
            Category::Treesway => ("Tree sway", "Balancement (arbres)"),
            Category::Water => ("Water", "Eau"),
            Category::Refract => ("Refraction", "Réfraction"),
            Category::Sprite => ("Sprite", "Sprite"),
            Category::Compile => ("Compile flags", "Options de compilation"),
            Category::Other => ("Other parameters", "Autres paramètres"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VmtParam {
    pub key: &'static str,
    pub kind: Kind,
    pub category: Category,
    pub shaders: u16,
    pub default: &'static str,
    pub english: &'static str,
    pub french: &'static str,
}

const fn p(
    key: &'static str,
    kind: Kind,
    category: Category,
    shaders: u16,
    default: &'static str,
    english: &'static str,
    french: &'static str,
) -> VmtParam {
    VmtParam { key, kind, category, shaders, default, english, french }
}

use Category as C;
use Kind as K;
use fam::*;

const F01: Kind = K::Float { min: 0.0, max: 1.0 };

const DETAIL_MODES: &[(&str, &str, &str)] = &[
    ("0", "Multiply (mod2x)", "Multiplier (mod2x)"),
    ("1", "Additive", "Ajouter"),
    ("2", "Alpha blend", "Fondu alpha"),
    ("3", "Fade", "Fondu"),
    ("4", "Base over detail", "Base par-dessus le détail"),
    ("5", "Additive self-illum", "Auto-illumination ajoutée"),
    ("6", "Additive, no light", "Ajouter sans lumière"),
    ("7", "Two pattern decal", "Decal à deux motifs"),
    ("8", "Multiply", "Multiplier"),
    ("9", "Mask base alpha", "Masquer l'alpha de base"),
    ("10", "SSBump bump", "Bump SSBump"),
    ("11", "SSBump albedo", "Albedo SSBump"),
];

const FOUR_WAY_MODES: &[(&str, &str, &str)] = &[("0", "Normal", "Normal"), ("1", "Modulate 2x", "Moduler 2x")];

const SWAY_MODES: &[(&str, &str, &str)] =
    &[("0", "Off", "Désactivé"), ("1", "Classic", "Classique"), ("2", "Radial (bushes)", "Radial (buissons)")];

const SPRITE_MODES: &[(&str, &str, &str)] = &[
    ("parallel_upright", "Parallel, upright", "Parallèle, vertical"),
    ("vp_parallel", "Facing the view", "Face à la vue"),
    ("oriented", "Oriented", "Orienté"),
    ("vp_parallel_oriented", "Facing the view, oriented", "Face à la vue, orienté"),
];

pub const PARAMS: &[VmtParam] = &[
    p("$basetexture", K::Texture, C::Base, ALL, "", "Main texture (diffuse).", "Texture principale (diffuse)."),
    p(
        "$basetexture2",
        K::Texture,
        C::Base,
        LM | UN,
        "",
        "Second texture of a blend (WorldVertexTransition).",
        "Deuxième texture d'un mélange (WorldVertexTransition).",
    ),
    p(
        "$texture2",
        K::Texture,
        C::Base,
        UN,
        "",
        "Second texture (UnlitTwoTexture).",
        "Deuxième texture (UnlitTwoTexture).",
    ),
    p(
        "$blendmodulatetexture",
        K::Texture,
        C::Base,
        LM,
        "",
        "Sharpens the blend of WorldVertexTransition.",
        "Rend plus net le mélange de WorldVertexTransition.",
    ),
    p(
        "$basetexture3",
        K::Texture,
        C::Base,
        B4,
        "",
        "Third texture of a 4-way blend.",
        "Troisième texture d'un mélange à 4 textures.",
    ),
    p(
        "$basetexture4",
        K::Texture,
        C::Base,
        B4,
        "",
        "Fourth texture of a 4-way blend.",
        "Quatrième texture d'un mélange à 4 textures.",
    ),
    p(
        "$texture1_lumstart",
        F01,
        C::Base,
        B4,
        "0",
        "Luminance of texture 1 where its blend starts (with lumend).",
        "Luminance de la texture 1 où son mélange commence (avec lumend).",
    ),
    p(
        "$texture1_lumend",
        F01,
        C::Base,
        B4,
        "1",
        "Luminance of texture 1 where its blend is full (swap with lumstart to invert).",
        "Luminance de la texture 1 où son mélange est plein (inversez avec lumstart pour retourner).",
    ),
    p(
        "$texture1_uvscale",
        K::Text,
        C::Base,
        B4,
        "[1 1]",
        "Scale of texture 1 ([x y]).",
        "Échelle de la texture 1 ([x y]).",
    ),
    p(
        "$texture2_lumstart",
        F01,
        C::Base,
        B4,
        "0",
        "Luminance of texture 2 where its blend starts (with lumend).",
        "Luminance de la texture 2 où son mélange commence (avec lumend).",
    ),
    p(
        "$texture2_lumend",
        F01,
        C::Base,
        B4,
        "1",
        "Luminance of texture 2 where its blend is full (swap with lumstart to invert).",
        "Luminance de la texture 2 où son mélange est plein (inversez avec lumstart pour retourner).",
    ),
    p(
        "$texture2_uvscale",
        K::Text,
        C::Base,
        B4,
        "[1 1]",
        "Scale of texture 2 ([x y]).",
        "Échelle de la texture 2 ([x y]).",
    ),
    p(
        "$texture3_lumstart",
        F01,
        C::Base,
        B4,
        "0",
        "Luminance of texture 3 where its blend starts (with lumend).",
        "Luminance de la texture 3 où son mélange commence (avec lumend).",
    ),
    p(
        "$texture3_lumend",
        F01,
        C::Base,
        B4,
        "1",
        "Luminance of texture 3 where its blend is full (swap with lumstart to invert).",
        "Luminance de la texture 3 où son mélange est plein (inversez avec lumstart pour retourner).",
    ),
    p(
        "$texture3_uvscale",
        K::Text,
        C::Base,
        B4,
        "[1 1]",
        "Scale of texture 3 ([x y]).",
        "Échelle de la texture 3 ([x y]).",
    ),
    p(
        "$texture4_lumstart",
        F01,
        C::Base,
        B4,
        "0",
        "Luminance of texture 4 where its blend starts (with lumend).",
        "Luminance de la texture 4 où son mélange commence (avec lumend).",
    ),
    p(
        "$texture4_lumend",
        F01,
        C::Base,
        B4,
        "1",
        "Luminance of texture 4 where its blend is full (swap with lumstart to invert).",
        "Luminance de la texture 4 où son mélange est plein (inversez avec lumstart pour retourner).",
    ),
    p(
        "$texture4_uvscale",
        K::Text,
        C::Base,
        B4,
        "[1 1]",
        "Scale of texture 4 ([x y]).",
        "Échelle de la texture 4 ([x y]).",
    ),
    p(
        "$texture2_blendstart",
        F01,
        C::Base,
        B4,
        "0",
        "Painted amount where texture 2 starts to show (sharpness, with blendend).",
        "Quantité peinte où la texture 2 commence à se montrer (netteté, avec blendend).",
    ),
    p(
        "$texture2_blendend",
        F01,
        C::Base,
        B4,
        "1",
        "Painted amount where texture 2 fully shows.",
        "Quantité peinte où la texture 2 se montre entièrement.",
    ),
    p(
        "$lumblendfactor2",
        F01,
        C::Base,
        B4,
        "0",
        "0: texture 2 blends by the darkness of the textures under it, 1: by its own luminance.",
        "0 : la texture 2 se mélange selon le sombre des textures dessous, 1 : selon sa propre luminance.",
    ),
    p(
        "$texture2_blendmode",
        K::Choice(FOUR_WAY_MODES),
        C::Base,
        B4,
        "0",
        "How texture 2 is laid over the others.",
        "Comment la texture 2 se pose sur les autres.",
    ),
    p(
        "$texture3_blendstart",
        F01,
        C::Base,
        B4,
        "0",
        "Painted amount where texture 3 starts to show (sharpness, with blendend).",
        "Quantité peinte où la texture 3 commence à se montrer (netteté, avec blendend).",
    ),
    p(
        "$texture3_blendend",
        F01,
        C::Base,
        B4,
        "1",
        "Painted amount where texture 3 fully shows.",
        "Quantité peinte où la texture 3 se montre entièrement.",
    ),
    p(
        "$lumblendfactor3",
        F01,
        C::Base,
        B4,
        "0",
        "0: texture 3 blends by the darkness of the textures under it, 1: by its own luminance.",
        "0 : la texture 3 se mélange selon le sombre des textures dessous, 1 : selon sa propre luminance.",
    ),
    p(
        "$texture3_blendmode",
        K::Choice(FOUR_WAY_MODES),
        C::Base,
        B4,
        "0",
        "How texture 3 is laid over the others.",
        "Comment la texture 3 se pose sur les autres.",
    ),
    p(
        "$texture4_blendstart",
        F01,
        C::Base,
        B4,
        "0",
        "Painted amount where texture 4 starts to show (sharpness, with blendend).",
        "Quantité peinte où la texture 4 commence à se montrer (netteté, avec blendend).",
    ),
    p(
        "$texture4_blendend",
        F01,
        C::Base,
        B4,
        "1",
        "Painted amount where texture 4 fully shows.",
        "Quantité peinte où la texture 4 se montre entièrement.",
    ),
    p(
        "$lumblendfactor4",
        F01,
        C::Base,
        B4,
        "0",
        "0: texture 4 blends by the darkness of the textures under it, 1: by its own luminance.",
        "0 : la texture 4 se mélange selon le sombre des textures dessous, 1 : selon sa propre luminance.",
    ),
    p(
        "$texture4_blendmode",
        K::Choice(FOUR_WAY_MODES),
        C::Base,
        B4,
        "0",
        "How texture 4 is laid over the others.",
        "Comment la texture 4 se pose sur les autres.",
    ),
    p(
        "$texture1_bumpblendfactor",
        F01,
        C::Bump,
        B4,
        "1",
        "How much of the normal map texture 1 shows (without $bumpmap1).",
        "Part de la normal map que montre la texture 1 (sans $bumpmap1).",
    ),
    p(
        "$texture2_bumpblendfactor",
        F01,
        C::Bump,
        B4,
        "1",
        "How much of the normal map texture 2 shows (without $bumpmap2).",
        "Part de la normal map que montre la texture 2 (sans $bumpmap2).",
    ),
    p(
        "$texture3_bumpblendfactor",
        F01,
        C::Bump,
        B4,
        "1",
        "How much of the normal map texture 3 shows (without $bumpmap3).",
        "Part de la normal map que montre la texture 3 (sans $bumpmap3).",
    ),
    p(
        "$texture4_bumpblendfactor",
        F01,
        C::Bump,
        B4,
        "1",
        "How much of the normal map texture 4 shows (without $bumpmap4).",
        "Part de la normal map que montre la texture 4 (sans $bumpmap4).",
    ),
    p("$color", K::Color, C::Base, LM | UN, "[1 1 1]", "Tint of the texture.", "Teinte de la texture."),
    p("$color2", K::Color, C::Base, VL, "[1 1 1]", "Tint of the model texture.", "Teinte de la texture du modèle."),
    p(
        "$basetexturetransform",
        K::Text,
        C::Base,
        ALL,
        "center .5 .5 scale 1 1 rotate 0 translate 0 0",
        "Moves, scales or turns the texture.",
        "Déplace, met à l'échelle ou tourne la texture.",
    ),
    p(
        "$frame",
        K::Int { min: 0, max: 512 },
        C::Base,
        ALL,
        "0",
        "Frame of an animated texture.",
        "Image d'une texture animée.",
    ),
    p(
        "$seamless_scale",
        K::Float { min: 0.0, max: 10.0 },
        C::Base,
        LM | VL,
        "0",
        "Triplanar mapping without seams (scale).",
        "Projection triplanaire sans couture (échelle).",
    ),
    p(
        "$translucent",
        K::Flag,
        C::Transparency,
        ALL,
        "0",
        "Uses the alpha of the texture as transparency.",
        "Utilise l'alpha de la texture comme transparence.",
    ),
    p(
        "$alphatest",
        K::Flag,
        C::Transparency,
        ALL,
        "0",
        "Cuts out the pixels of low alpha (fences, leaves).",
        "Découpe les pixels d'alpha faible (grillages, feuilles).",
    ),
    p(
        "$alphatestreference",
        F01,
        C::Transparency,
        ALL,
        "0.5",
        "Alpha under which $alphatest cuts.",
        "Alpha sous lequel $alphatest découpe.",
    ),
    p(
        "$allowalphatocoverage",
        K::Flag,
        C::Transparency,
        ALL,
        "0",
        "Smooths the edges of $alphatest with MSAA.",
        "Adoucit les bords de $alphatest avec le MSAA.",
    ),
    p(
        "$additive",
        K::Flag,
        C::Transparency,
        ALL,
        "0",
        "Adds the color to what is behind (glows).",
        "Ajoute la couleur à ce qui est derrière (lueurs).",
    ),
    p("$alpha", F01, C::Transparency, ALL, "1", "Opacity of the whole material.", "Opacité de tout le matériau."),
    p(
        "$nocull",
        K::Flag,
        C::Transparency,
        ALL,
        "0",
        "Draws both sides of the faces.",
        "Dessine les deux côtés des faces.",
    ),
    p(
        "$vertexalpha",
        K::Flag,
        C::Transparency,
        ALL,
        "0",
        "Uses the alpha of the vertices.",
        "Utilise l'alpha des sommets.",
    ),
    p(
        "$vertexcolor",
        K::Flag,
        C::Transparency,
        ALL,
        "0",
        "Uses the color of the vertices.",
        "Utilise la couleur des sommets.",
    ),
    p(
        "$ignorez",
        K::Flag,
        C::Transparency,
        ALL,
        "0",
        "Drawn over everything (ignores the depth).",
        "Dessiné par-dessus tout (ignore la profondeur).",
    ),
    p("$decal", K::Flag, C::Transparency, ALL, "0", "Material used as a decal.", "Matériau utilisé comme decal."),
    p(
        "$decalscale",
        K::Float { min: 0.01, max: 10.0 },
        C::Transparency,
        ALL,
        "1",
        "Size of the decal.",
        "Taille du decal.",
    ),
    p(
        "$bumpmap",
        K::Texture,
        C::Bump,
        LIT | WA | RE,
        "",
        "Normal map: relief under the light.",
        "Normal map : relief sous la lumière.",
    ),
    p(
        "$bumpmap2",
        K::Texture,
        C::Bump,
        LM,
        "",
        "Normal map of the second texture of a blend.",
        "Normal map de la deuxième texture d'un mélange.",
    ),
    p(
        "$ssbump",
        K::Flag,
        C::Bump,
        LM,
        "0",
        "The bump map is a self-shadowed bump map.",
        "La bump map est une bump map auto-ombrée (ssbump).",
    ),
    p(
        "$bumpframe",
        K::Int { min: 0, max: 512 },
        C::Bump,
        LIT,
        "0",
        "Frame of an animated bump map.",
        "Image d'une bump map animée.",
    ),
    p(
        "$bumptransform",
        K::Text,
        C::Bump,
        LIT,
        "center .5 .5 scale 1 1 rotate 0 translate 0 0",
        "Moves, scales or turns the bump map.",
        "Déplace, met à l'échelle ou tourne la bump map.",
    ),
    p(
        "$nodiffusebumplighting",
        K::Flag,
        C::Bump,
        LIT,
        "0",
        "The bump map only shapes the reflections.",
        "La bump map ne sert qu'aux reflets.",
    ),
    p("$addbumpmaps", K::Flag, C::Bump, LM, "0", "Adds the two bump maps.", "Additionne les deux bump maps."),
    p(
        "$detail",
        K::Texture,
        C::Detail,
        LIT | UN,
        "",
        "Detail texture over the base texture.",
        "Texture de détail par-dessus la texture de base.",
    ),
    p(
        "$detailscale",
        K::Float { min: 0.0, max: 64.0 },
        C::Detail,
        LIT | UN,
        "4",
        "Repeats of the detail texture.",
        "Répétitions de la texture de détail.",
    ),
    p("$detailblendfactor", F01, C::Detail, LIT | UN, "1", "Strength of the detail.", "Force du détail."),
    p(
        "$detailblendmode",
        K::Choice(DETAIL_MODES),
        C::Detail,
        LIT | UN,
        "0",
        "How the detail is mixed.",
        "Comment le détail est mélangé.",
    ),
    p("$detailtint", K::Color, C::Detail, LIT | UN, "[1 1 1]", "Tint of the detail.", "Teinte du détail."),
    p(
        "$detailframe",
        K::Int { min: 0, max: 512 },
        C::Detail,
        LIT | UN,
        "0",
        "Frame of an animated detail texture.",
        "Image d'une texture de détail animée.",
    ),
    p(
        "$envmap",
        K::Text,
        C::Reflection,
        LIT | UN | WA,
        "",
        "Reflected cubemap: env_cubemap for the nearest env_cubemap.",
        "Cubemap reflétée : env_cubemap pour l'env_cubemap le plus proche.",
    ),
    p(
        "$envmapmask",
        K::Texture,
        C::Reflection,
        LIT | UN,
        "",
        "Where the reflections show.",
        "Où les reflets apparaissent.",
    ),
    p(
        "$basealphaenvmapmask",
        K::Flag,
        C::Reflection,
        LIT,
        "0",
        "The alpha of the base texture masks the reflections.",
        "L'alpha de la texture de base masque les reflets.",
    ),
    p(
        "$normalmapalphaenvmapmask",
        K::Flag,
        C::Reflection,
        LIT,
        "0",
        "The alpha of the bump map masks the reflections.",
        "L'alpha de la bump map masque les reflets.",
    ),
    p(
        "$envmaptint",
        K::Color,
        C::Reflection,
        LIT | UN,
        "[1 1 1]",
        "Tint and strength of the reflections.",
        "Teinte et force des reflets.",
    ),
    p("$envmapcontrast", F01, C::Reflection, LIT | UN, "0", "Contrast of the reflections.", "Contraste des reflets."),
    p(
        "$envmapsaturation",
        F01,
        C::Reflection,
        LIT | UN,
        "1",
        "Saturation of the reflections.",
        "Saturation des reflets.",
    ),
    p(
        "$fresnelreflection",
        F01,
        C::Reflection,
        LM,
        "1",
        "Reflections stronger at grazing angles.",
        "Reflets plus forts aux angles rasants.",
    ),
    p(
        "$envmapframe",
        K::Int { min: 0, max: 512 },
        C::Reflection,
        LIT,
        "0",
        "Frame of an animated cubemap.",
        "Image d'une cubemap animée.",
    ),
    p("$phong", K::Flag, C::Phong, VL, "0", "Glossy highlights on models.", "Reflets brillants sur les modèles."),
    p(
        "$phongexponent",
        K::Float { min: 0.0, max: 255.0 },
        C::Phong,
        VL,
        "5",
        "Sharpness of the highlights.",
        "Netteté des reflets brillants.",
    ),
    p("$phongexponenttexture", K::Texture, C::Phong, VL, "", "Texture of the sharpness.", "Texture de la netteté."),
    p(
        "$phongboost",
        K::Float { min: 0.0, max: 100.0 },
        C::Phong,
        VL,
        "1",
        "Strength of the highlights.",
        "Force des reflets brillants.",
    ),
    p(
        "$phongfresnelranges",
        K::Vec3,
        C::Phong,
        VL,
        "[0 0.5 1]",
        "Highlight strength facing, at 45 degrees and grazing.",
        "Force du reflet de face, à 45 degrés et rasant.",
    ),
    p("$phongtint", K::Color, C::Phong, VL, "[1 1 1]", "Tint of the highlights.", "Teinte des reflets brillants."),
    p(
        "$phongalbedotint",
        K::Flag,
        C::Phong,
        VL,
        "0",
        "The highlights take the color of the texture.",
        "Les reflets prennent la couleur de la texture.",
    ),
    p(
        "$basemapalphaphongmask",
        K::Flag,
        C::Phong,
        VL,
        "0",
        "The alpha of the base texture masks the highlights.",
        "L'alpha de la texture de base masque les reflets.",
    ),
    p(
        "$invertphongmask",
        K::Flag,
        C::Phong,
        VL,
        "0",
        "Inverts the mask of the highlights.",
        "Inverse le masque des reflets.",
    ),
    p(
        "$phongwarptexture",
        K::Texture,
        C::Phong,
        VL,
        "",
        "Color ramp of the highlights (iridescence).",
        "Rampe de couleur des reflets (irisation).",
    ),
    p(
        "$rimlight",
        K::Flag,
        C::RimLight,
        VL,
        "0",
        "Light on the edges of the model.",
        "Lumière sur les contours du modèle.",
    ),
    p(
        "$rimlightexponent",
        K::Float { min: 0.0, max: 255.0 },
        C::RimLight,
        VL,
        "4",
        "Thinness of the rim light.",
        "Finesse de la lumière de contour.",
    ),
    p(
        "$rimlightboost",
        K::Float { min: 0.0, max: 100.0 },
        C::RimLight,
        VL,
        "1",
        "Strength of the rim light.",
        "Force de la lumière de contour.",
    ),
    p(
        "$rimmask",
        K::Flag,
        C::RimLight,
        VL,
        "0",
        "The alpha of the exponent texture masks the rim light.",
        "L'alpha de la texture d'exposant masque la lumière de contour.",
    ),
    p(
        "$selfillum",
        K::Flag,
        C::SelfIllum,
        LIT | UN,
        "0",
        "The texture glows where its alpha is white.",
        "La texture brille là où son alpha est blanc.",
    ),
    p(
        "$selfillumtint",
        K::Color,
        C::SelfIllum,
        LIT | UN,
        "[1 1 1]",
        "Tint and strength of the glow.",
        "Teinte et force de la lueur.",
    ),
    p(
        "$selfillummask",
        K::Texture,
        C::SelfIllum,
        LIT,
        "",
        "Where it glows (instead of the alpha).",
        "Où ça brille (au lieu de l'alpha).",
    ),
    p(
        "$selfillumfresnel",
        K::Flag,
        C::SelfIllum,
        VL,
        "0",
        "The glow depends on the view angle.",
        "La lueur dépend de l'angle de vue.",
    ),
    p(
        "$selfillumfresnelminmaxexp",
        K::Vec3,
        C::SelfIllum,
        VL,
        "[0 1 1]",
        "Glow facing, at grazing angles, and the curve.",
        "Lueur de face, aux angles rasants, et la courbe.",
    ),
    p(
        "$surfaceprop",
        K::SurfaceProp,
        C::Lighting,
        ALL,
        "default",
        "Sounds, impacts and physics of the surface.",
        "Sons, impacts et physique de la surface.",
    ),
    p(
        "$surfaceprop2",
        K::SurfaceProp,
        C::Lighting,
        LM,
        "default",
        "Surface of the second texture of a blend.",
        "Surface de la deuxième texture d'un mélange.",
    ),
    p(
        "$halflambert",
        K::Flag,
        C::Lighting,
        VL,
        "0",
        "Softer lighting that wraps around the model.",
        "Éclairage plus doux qui enveloppe le modèle.",
    ),
    p(
        "$lightwarptexture",
        K::Texture,
        C::Lighting,
        VL,
        "",
        "Color ramp of the lighting (cartoon look).",
        "Rampe de couleur de l'éclairage (style cartoon).",
    ),
    p("$nodecal", K::Flag, C::Lighting, ALL, "0", "No decals on this material.", "Aucun decal sur ce matériau."),
    p("$nofog", K::Flag, C::Lighting, ALL, "0", "Not affected by the fog.", "Pas affecté par le brouillard."),
    p("$model", K::Flag, C::Lighting, UN, "0", "The material goes on a model.", "Le matériau va sur un modèle."),
    p(
        "%keywords",
        K::Text,
        C::Lighting,
        ALL,
        "",
        "Words for searching the material in Hammer.",
        "Mots pour chercher le matériau dans Hammer.",
    ),
    p(
        "%tooltexture",
        K::Texture,
        C::Lighting,
        ALL,
        "",
        "Texture shown by the editors.",
        "Texture montrée par les éditeurs.",
    ),
    p(
        "%detailtype",
        K::Text,
        C::Lighting,
        LM,
        "",
        "Detail props of the surface (detail.vbsp).",
        "Props de détail de la surface (detail.vbsp).",
    ),
    p(
        "$treesway",
        K::Choice(SWAY_MODES),
        C::Treesway,
        VL,
        "0",
        "The model sways in the wind.",
        "Le modèle se balance dans le vent.",
    ),
    p(
        "$treeswayheight",
        K::Float { min: 0.0, max: 10000.0 },
        C::Treesway,
        VL,
        "1000",
        "Height of the sway.",
        "Hauteur du balancement.",
    ),
    p(
        "$treeswaystartheight",
        F01,
        C::Treesway,
        VL,
        "0.2",
        "Height where the sway starts (0 to 1).",
        "Hauteur où le balancement commence (0 à 1).",
    ),
    p(
        "$treeswayradius",
        K::Float { min: 0.0, max: 10000.0 },
        C::Treesway,
        VL,
        "300",
        "Radius of the sway.",
        "Rayon du balancement.",
    ),
    p(
        "$treeswaystartradius",
        F01,
        C::Treesway,
        VL,
        "0.1",
        "Radius where the sway starts (0 to 1).",
        "Rayon où le balancement commence (0 à 1).",
    ),
    p(
        "$treeswayspeed",
        K::Float { min: 0.0, max: 100.0 },
        C::Treesway,
        VL,
        "1",
        "Speed of the sway.",
        "Vitesse du balancement.",
    ),
    p(
        "$treeswaystrength",
        K::Float { min: 0.0, max: 100.0 },
        C::Treesway,
        VL,
        "10",
        "Strength of the sway.",
        "Force du balancement.",
    ),
    p(
        "$treeswayscrumblespeed",
        K::Float { min: 0.0, max: 100.0 },
        C::Treesway,
        VL,
        "0.1",
        "Speed of the leaf shake.",
        "Vitesse du frémissement des feuilles.",
    ),
    p(
        "$treeswayscrumblestrength",
        K::Float { min: 0.0, max: 100.0 },
        C::Treesway,
        VL,
        "0.1",
        "Strength of the leaf shake.",
        "Force du frémissement des feuilles.",
    ),
    p("$normalmap", K::Texture, C::Water, WA | RE, "", "Waves (normal map).", "Vagues (normal map)."),
    p(
        "$reflecttexture",
        K::Text,
        C::Water,
        WA,
        "_rt_WaterReflection",
        "Reflection render target.",
        "Cible de rendu du reflet.",
    ),
    p(
        "$refracttexture",
        K::Text,
        C::Water,
        WA,
        "_rt_WaterRefraction",
        "Refraction render target.",
        "Cible de rendu de la réfraction.",
    ),
    p(
        "$reflectamount",
        K::Float { min: 0.0, max: 2.0 },
        C::Water,
        WA,
        "0.8",
        "Distortion of the reflection.",
        "Distorsion du reflet.",
    ),
    p(
        "$refractamount",
        K::Float { min: 0.0, max: 2.0 },
        C::Water,
        WA | RE,
        "0.5",
        "Distortion of what is seen through.",
        "Distorsion de ce qu'on voit à travers.",
    ),
    p("$reflecttint", K::Color, C::Water, WA, "[1 1 1]", "Tint of the reflection.", "Teinte du reflet."),
    p("$refracttint", K::Color, C::Water, WA | RE, "[1 1 1]", "Tint of the refraction.", "Teinte de la réfraction."),
    p(
        "$reflectentities",
        K::Flag,
        C::Water,
        WA,
        "0",
        "Entities show in the reflection.",
        "Les entités apparaissent dans le reflet.",
    ),
    p(
        "$forceexpensive",
        K::Flag,
        C::Water,
        WA,
        "0",
        "Always the full quality water.",
        "Toujours l'eau en qualité maximale.",
    ),
    p("$fogenable", K::Flag, C::Water, WA, "0", "Fog under the water.", "Brouillard sous l'eau."),
    p("$fogcolor", K::Color, C::Water, WA, "[0 0 0]", "Color of the water fog.", "Couleur du brouillard de l'eau."),
    p(
        "$fogstart",
        K::Float { min: 0.0, max: 10000.0 },
        C::Water,
        WA,
        "0",
        "Distance where the water fog starts.",
        "Distance où le brouillard de l'eau commence.",
    ),
    p(
        "$fogend",
        K::Float { min: 0.0, max: 10000.0 },
        C::Water,
        WA,
        "200",
        "Distance where the water fog is full.",
        "Distance où le brouillard de l'eau est complet.",
    ),
    p(
        "$abovewater",
        K::Flag,
        C::Water,
        WA,
        "1",
        "Material seen from above the water.",
        "Matériau vu depuis au-dessus de l'eau.",
    ),
    p(
        "$bottommaterial",
        K::Text,
        C::Water,
        WA,
        "",
        "Material seen from under the water.",
        "Matériau vu depuis sous l'eau.",
    ),
    p("$underwateroverlay", K::Text, C::Water, WA, "", "Screen overlay under the water.", "Effet d'écran sous l'eau."),
    p(
        "$scroll1",
        K::Vec3,
        C::Water,
        WA,
        "[0 0 0]",
        "Scroll of the first wave layer.",
        "Défilement de la première couche de vagues.",
    ),
    p(
        "$scroll2",
        K::Vec3,
        C::Water,
        WA,
        "[0 0 0]",
        "Scroll of the second wave layer.",
        "Défilement de la deuxième couche de vagues.",
    ),
    p(
        "$bluramount",
        K::Float { min: 0.0, max: 10.0 },
        C::Refract,
        RE,
        "0",
        "Blur of what is seen through.",
        "Flou de ce qu'on voit à travers.",
    ),
    p(
        "$refracttinttexture",
        K::Texture,
        C::Refract,
        RE,
        "",
        "Texture tinting the refraction.",
        "Texture qui teinte la réfraction.",
    ),
    p(
        "$spriteorientation",
        K::Choice(SPRITE_MODES),
        C::Sprite,
        SP,
        "parallel_upright",
        "How the sprite faces the camera.",
        "Comment le sprite fait face à la caméra.",
    ),
    p(
        "$spriteorigin",
        K::Text,
        C::Sprite,
        SP,
        "[0.5 0.5]",
        "Origin of the sprite (0 to 1).",
        "Origine du sprite (0 à 1).",
    ),
    p(
        "$spriterendermode",
        K::Int { min: 0, max: 10 },
        C::Sprite,
        SP,
        "0",
        "Render mode of the sprite.",
        "Mode de rendu du sprite.",
    ),
    p("%compilenodraw", K::Flag, C::Compile, ALL, "0", "Not drawn (like nodraw).", "Pas dessiné (comme nodraw)."),
    p("%compilesky", K::Flag, C::Compile, ALL, "0", "Sky face.", "Face de ciel."),
    p("%compile2dsky", K::Flag, C::Compile, ALL, "0", "2D sky face only.", "Face de ciel 2D seulement."),
    p("%compileskip", K::Flag, C::Compile, ALL, "0", "Skipped by VBSP.", "Ignoré par VBSP."),
    p("%compilehint", K::Flag, C::Compile, ALL, "0", "Hint face (cuts visleaves).", "Face hint (coupe les visleaves)."),
    p(
        "%compileclip",
        K::Flag,
        C::Compile,
        ALL,
        "0",
        "Blocks players and NPCs, invisible.",
        "Bloque les joueurs et les PNJ, invisible.",
    ),
    p("%compileplayerclip", K::Flag, C::Compile, ALL, "0", "Blocks the players only.", "Bloque seulement les joueurs."),
    p("%compilenpcclip", K::Flag, C::Compile, ALL, "0", "Blocks the NPCs only.", "Bloque seulement les PNJ."),
    p("%compilegrenadeclip", K::Flag, C::Compile, ALL, "0", "Blocks the grenades.", "Bloque les grenades."),
    p("%compiletrigger", K::Flag, C::Compile, ALL, "0", "Trigger volume.", "Volume de trigger."),
    p("%compileladder", K::Flag, C::Compile, ALL, "0", "Climbable.", "Escaladable."),
    p("%compilewater", K::Flag, C::Compile, ALL, "0", "Water volume.", "Volume d'eau."),
    p("%compileslime", K::Flag, C::Compile, ALL, "0", "Slime volume.", "Volume de slime."),
    p("%compilenonsolid", K::Flag, C::Compile, ALL, "0", "Not solid.", "Pas solide."),
    p(
        "%compiledetail",
        K::Flag,
        C::Compile,
        ALL,
        "0",
        "Detail brush (does not cut visleaves).",
        "Brosse de détail (ne coupe pas les visleaves).",
    ),
    p("%compilepassbullets", K::Flag, C::Compile, ALL, "0", "Bullets go through.", "Les balles passent à travers."),
    p("%compileorigin", K::Flag, C::Compile, ALL, "0", "Origin brush.", "Brosse d'origine."),
    p("%noportal", K::Flag, C::Compile, ALL, "0", "No portal can be placed on it.", "Aucun portail ne peut s'y poser."),
    p("%playerclip", K::Flag, C::Compile, ALL, "0", "Player clip (old name).", "Clip de joueur (ancien nom)."),
];

pub fn params_of(category: Category, family: u16, all: bool) -> impl Iterator<Item = &'static VmtParam> {
    PARAMS.iter().filter(move |p| p.category == category && (all || p.shaders & family != 0))
}

pub fn find(key: &str) -> Option<&'static VmtParam> {
    PARAMS.iter().find(|p| p.key.eq_ignore_ascii_case(key))
}

pub fn parse_color(v: &str) -> Option<[f32; 3]> {
    let t = v.trim();
    let (inner, scale) = if let Some(s) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        (s, 1.0)
    } else if let Some(s) = t.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        (s, 255.0)
    } else {
        let g: f32 = t.parse().ok()?;
        return g.is_finite().then_some([g, g, g]);
    };
    let n: Vec<f32> = inner.split_whitespace().filter_map(|s| s.parse().ok()).collect();
    match n.as_slice() {
        [r, g, b, ..] if [r, g, b].iter().all(|c| c.is_finite()) => Some([r / scale, g / scale, b / scale]),
        _ => None,
    }
}

pub fn format_color(c: [f32; 3]) -> String {
    let f = |v: f32| {
        let s = format!("{:.3}", v);
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s.is_empty() || s == "-" { "0".to_owned() } else { s.to_owned() }
    };
    format!("[{} {} {}]", f(c[0]), f(c[1]), f(c[2]))
}

pub fn format_number(v: f32) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" { "0".to_owned() } else { s.to_owned() }
}

pub fn set_shader_in_text(text: &str, shader: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if text[i..].starts_with("//") {
            i = text[i..].find('\n').map_or(bytes.len(), |n| i + n);
        } else {
            break;
        }
    }
    if i >= bytes.len() || bytes[i] == b'{' {
        return None;
    }
    let (start, end, quoted) = if bytes[i] == b'"' {
        let close = text[i + 1..].find('"')? + i + 1;
        (i, close + 1, true)
    } else {
        let end = text[i..].find(|c: char| c.is_whitespace() || c == '{').map_or(bytes.len(), |n| i + n);
        (i, end, false)
    };
    let name = if quoted { format!("\"{shader}\"") } else { shader.to_owned() };
    Some(format!("{}{}{}", &text[..start], name, &text[end..]))
}

pub fn new_material_text(shader: &str) -> String {
    format!("\"{shader}\"\n{{\n}}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_clean() {
        let mut seen = std::collections::HashSet::new();
        for p in PARAMS {
            assert!(seen.insert(p.key.to_ascii_lowercase()), "twice: {}", p.key);
            assert!(p.key.starts_with('$') || p.key.starts_with('%'), "{}", p.key);
            assert!(!p.english.is_empty() && !p.french.is_empty(), "{}", p.key);
            for text in [p.english, p.french] {
                assert!(!text.contains('\u{2014}') && !text.contains('\u{2013}'), "dash in {}", p.key);
            }
            if let Kind::Choice(list) = p.kind {
                assert!(list.iter().any(|(v, _, _)| *v == p.default), "{}: default not in the list", p.key);
            }
        }
        for c in Category::ALL {
            assert_eq!(params_of(c, fam::ALL, true).next().is_some(), c != Category::Other, "{c:?}");
        }
        assert!(params_of(Category::Phong, family_of("LightmappedGeneric"), false).next().is_none());
        assert!(params_of(Category::Phong, family_of("VertexLitGeneric"), false).next().is_some());
        assert_eq!(family_of("SDK_LightmappedGeneric_DX9"), fam::LM);
        let four: Vec<&str> =
            params_of(Category::Base, family_of("Lightmapped_4WayBlend"), false).map(|p| p.key).collect();
        for key in [
            "$basetexture",
            "$basetexture4",
            "$texture1_lumstart",
            "$texture4_blendend",
            "$lumblendfactor3",
            "$texture2_uvscale",
        ] {
            assert!(four.contains(&key), "{key}");
        }
        assert!(
            params_of(Category::Base, family_of("LightmappedGeneric"), false).all(|p| p.key != "$texture2_blendstart")
        );
        assert_eq!(find("$BaseTexture").map(|p| p.key), Some("$basetexture"));
    }

    #[test]
    fn colors_numbers_and_shaders() {
        assert_eq!(parse_color("[1 0.5 0]"), Some([1.0, 0.5, 0.0]));
        assert_eq!(parse_color("{255 0 51}"), Some([1.0, 0.0, 0.2]));
        assert_eq!(parse_color(" .5 "), Some([0.5, 0.5, 0.5]));
        assert_eq!(parse_color("[1 2]"), None);
        assert_eq!(format_color([1.0, 0.5, 0.0]), "[1 0.5 0]");
        assert_eq!(format_color([0.333_333, 2.0, 0.0]), "[0.333 2 0]");
        assert_eq!(format_number(4.0), "4");
        assert_eq!(format_number(0.25), "0.25");
        let src = "// head\n\"LightmappedGeneric\"\n{\n\t$basetexture a/b\n}\n";
        assert_eq!(
            set_shader_in_text(src, "VertexLitGeneric").as_deref(),
            Some("// head\n\"VertexLitGeneric\"\n{\n\t$basetexture a/b\n}\n")
        );
        let bare = "UnlitGeneric{ $basetexture x }";
        assert_eq!(set_shader_in_text(bare, "Sprite").as_deref(), Some("Sprite{ $basetexture x }"));
        assert_eq!(set_shader_in_text("  { }", "X"), None);
        let new = new_material_text("Water");
        assert_eq!(hp_vmt::parse(&new).expect("parse").shader, "Water");
    }
}
