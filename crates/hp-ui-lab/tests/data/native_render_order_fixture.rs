use hp_pcf::dmx::Value;
use hp_pcf::editor::{Category, Document};
use hp_gma::{AddonJson, GmaSource, GmaWriter};
use std::path::Path;
fn attrs(v: Vec<(&str, Value)>) -> Vec<(String, Value)> { v.into_iter().map(|(k,v)|(k.into(),v)).collect() }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let folder=std::env::args_os().nth(1).ok_or("output directory")?;
    let folder=Path::new(&folder);
    let mut gma=GmaWriter::new(AddonJson { title:"Own screen-velocity renderer reference".into(), ..Default::default() });
    let pixels=[255u8;4].repeat(64);
    let image=hp_vtf::compose(&[vec![hp_vtf::RgbaImage{width:8,height:8,pixels}]],&hp_vtf::ComposeOptions{format:hp_vtf::ImageFormat::Rgba8888,mipmaps:false,version_minor:2,..Default::default()})?;
    gma.add("materials/hp_renderorder117/quad.vtf",GmaSource::Bytes(image)).unwrap();
    gma.add("materials/hp_renderorder117/quad.vmt",GmaSource::Bytes(b"SpriteCard { \"$basetexture\" \"hp_renderorder117/quad\" \"$vertexcolor\" \"1\" \"$vertexalpha\" \"1\" \"$translucent\" \"1\" \"$splinetype\" \"0\" \"$depthblend\" \"0\" \"$nofog\" \"1\" }".to_vec())).unwrap();
    let mut doc=Document::new();
    for name in ["single_default","double_default","single_sorted","double_sorted","single_unsorted","double_unsorted"] {
        let id=doc.create_system(&format!("hp_renderorder117_{name}"))?;
        doc.set_attrs(id,attrs(vec![("max_particles",Value::Int(2)),("radius",Value::Float(24.)),("color",Value::Color([255,255,255,128])),("material",Value::Str("hp_renderorder117/quad".into())),("bounding_box_min",Value::Vec3([0.;3])),("bounding_box_max",Value::Vec3([0.;3]))]))?;
        doc.add_operator_with_params(id,Category::Emitters,"emit_instantaneously",attrs(vec![("num_to_emit",Value::Int(2))]))?;
        if !name.ends_with("_default") { doc.set_attr(id,"Sort particles",Value::Bool(name.ends_with("_sorted")))?; }
        doc.add_operator_with_params(id,Category::Initializers,"Remap Particle Count to Scalar",attrs(vec![("input minimum",Value::Int(0)),("input maximum",Value::Int(1)),("output field",Value::Int(10)),("output minimum",Value::Float(0.)),("output maximum",Value::Float(1.))]))?;
        for (field,min,max) in [(0,[10.,0.,64.],[10.,32.,64.]),(6,[1.,0.,0.],[0.,0.,1.])] {
            doc.add_operator_with_params(id,Category::Initializers,"Remap Scalar to Vector",attrs(vec![("input field",Value::Int(10)),("input minimum",Value::Float(0.)),("input maximum",Value::Float(1.)),("output field",Value::Int(field)),("output minimum",Value::Vec3(min)),("output maximum",Value::Vec3(max)),("use local system",Value::Bool(false))]))?;
        }
        doc.add_operator_with_params(id,Category::Initializers,"Lifetime Random",attrs(vec![("lifetime_min",Value::Float(100.)),("lifetime_max",Value::Float(100.))]))?;
        let velocity=[0.;3];
        doc.add_operator_with_params(id,Category::Initializers,"Velocity Random",attrs(vec![("speed_in_local_coordinate_system_min",Value::Vec3(velocity)),("speed_in_local_coordinate_system_max",Value::Vec3(velocity))]))?;
        if name=="initial30" {doc.add_operator_with_params(id,Category::Initializers,"Rotation Random",attrs(vec![("rotation_initial",Value::Float(30.)),("rotation_offset_min",Value::Float(0.)),("rotation_offset_max",Value::Float(0.))]))?;}
        doc.add_operator_with_params(id,Category::Operators,"Movement Basic",attrs(vec![("gravity",Value::Vec3([0.;3]))]))?;
        for _ in 0..if name.starts_with("double"){2}else{1} {doc.add_operator_with_params(id,Category::Renderers,"render_animated_sprites",attrs(vec![("orientation_type",Value::Int(0))]))?;}


    }
    std::fs::write(folder.join("hp_renderorder117.pcf"),doc.to_bytes()?)?;
    gma.write(&folder.join("hp_renderorder117.gma"))?;
    Ok(())
}