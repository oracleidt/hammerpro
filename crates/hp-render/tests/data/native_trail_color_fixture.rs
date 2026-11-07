use hp_pcf::dmx::Value;
use hp_pcf::editor::{Category, Document};
use hp_gma::{AddonJson, GmaSource, GmaWriter};
use std::path::Path;
fn attrs(v: Vec<(&str, Value)>) -> Vec<(String, Value)> {
    v.into_iter().map(|(k,v)| (k.into(),v)).collect()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let folder = std::env::args_os().nth(1).ok_or("output directory")?;
    let folder = Path::new(&folder);
    let mut doc = Document::new();
    let mut gma = GmaWriter::new(AddonJson { title: "Own trail color 117 fixture".into(), ..Default::default() });
    for (name,rgba) in [("white",[255;4]),("alpha64",[255,255,255,64]),("rgba128",[128,64,192,128]),("dark64",[16,48,64,64])] {
        let texture = hp_vtf::compose(&[vec![hp_vtf::RgbaImage { width: 4, height: 4, pixels: rgba.repeat(16) }]], &hp_vtf::ComposeOptions { format: hp_vtf::ImageFormat::Rgba8888, mipmaps: false, version_minor: 2, ..Default::default() })?;
        gma.add(&format!("materials/hp_trail117/{name}.vtf"), GmaSource::Bytes(texture)).unwrap();
    }
    for (material,shader,additive,addself) in [("cpu","UnlitGeneric",0,0.),("spline","SpriteCard",0,0.),("splineadd","SpriteCard",1,0.),("splineaddself","SpriteCard",0,0.5)] {
        let spline = if shader == "SpriteCard" { 1 } else { 0 };
        gma.add(&format!("materials/hp_trail117/{material}.vmt"), GmaSource::Bytes(format!("{shader}\n{{\n\"$basetexture\" \"hp_trail117/white\"\n\"$vertexcolor\" \"1\"\n\"$vertexalpha\" \"1\"\n\"$translucent\" \"1\"\n\"$nofog\" \"1\"\n\"$splinetype\" \"{spline}\"\n\"$depthblend\" \"0\"\n\"$additive\" \"{additive}\"\n\"$addself\" \"{addself}\"\n}}\n").into_bytes())).unwrap();
        for (name, tail) in [
            ("default", None), ("one", Some([1.;4])), ("dark", Some([0.,0.,0.,1.])),
            ("fade", Some([1.,1.,1.,0.])), ("mixed", Some([0.2,0.5,0.8,0.3])),
            ("over", Some([2.,2.,2.,1.])), ("alpha2", Some([1.,1.,1.,2.])),
            ("low",None),("white",None),("green",None),("alpha1",None),("alpha2byte",None),("alpha3",None),
        ] {
            let id = doc.create_system(&format!("hp_trail117_{material}_{name}"))?;
            let color=match name { "low"=>[16,128,240,64],"white"=>[255;4],"green"=>[0,255,15,128],"alpha1"=>[255,128,32,1],"alpha2byte"=>[255,128,32,2],"alpha3"=>[255,128,32,3],_=>[121,167,213,173] };
            doc.set_attrs(id, attrs(vec![("max_particles",Value::Int(1)),("radius",Value::Float(24.)),("color",Value::Color(color)),("material",Value::Str(format!("hp_trail117/{material}"))),("bounding_box_min",Value::Vec3([0.;3])),("bounding_box_max",Value::Vec3([0.;3]))]))?;
            doc.add_operator_with_params(id,Category::Emitters,"emit_instantaneously",attrs(vec![("num_to_emit",Value::Int(1))]))?;
            doc.add_operator_with_params(id,Category::Initializers,"Position Within Box Random",attrs(vec![("min",Value::Vec3([10.,0.,0.])),("max",Value::Vec3([10.,0.,0.]))]))?;
            doc.add_operator_with_params(id,Category::Initializers,"Lifetime Random",attrs(vec![("lifetime_min",Value::Float(100.)),("lifetime_max",Value::Float(100.))]))?;
            doc.add_operator_with_params(id,Category::Initializers,"Velocity Random",attrs(vec![("speed_in_local_coordinate_system_min",Value::Vec3([100.,0.,0.])),("speed_in_local_coordinate_system_max",Value::Vec3([100.,0.,0.]))]))?;
            doc.add_operator(id,Category::Operators,"Movement Basic")?;
            let mut params=attrs(vec![("min length",Value::Float(64.)),("max length",Value::Float(64.)),("constrain radius to length",Value::Bool(false))]);
            if let Some(tail)=tail { params.push(("tail color and alpha scale factor".into(),Value::Vec4(tail))); }
            doc.add_operator_with_params(id,Category::Renderers,"render_sprite_trail",params)?;
        }
    }
    std::fs::write(folder.join("hp_trail_color117.pcf"),doc.to_bytes()?)?;
    gma.write(&folder.join("hp_trail_color117.gma"))?;
    Ok(())
}
