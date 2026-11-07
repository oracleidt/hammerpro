use hp_pcf::dmx::Value;
use hp_pcf::editor::{Category, Document};
use hp_gma::{AddonJson, GmaSource, GmaWriter};
use std::path::Path;
fn attrs(v: Vec<(&str, Value)>) -> Vec<(String, Value)> { v.into_iter().map(|(k,v)|(k.into(),v)).collect() }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let folder=std::env::args_os().nth(1).ok_or("output directory")?;
    let folder=Path::new(&folder);
    let mut gma=GmaWriter::new(AddonJson { title:"Own screen-velocity renderer reference".into(), ..Default::default() });
    let pixels=(0..64).flat_map(|i| if i/8<4 { if i%8<4 {[255,0,0,255]}else{[0,0,255,255]} }else if i%8<4 {[0,255,0,255]}else{[255,255,0,255]}).collect();
    let image=hp_vtf::compose(&[vec![hp_vtf::RgbaImage{width:8,height:8,pixels}]],&hp_vtf::ComposeOptions{format:hp_vtf::ImageFormat::Rgba8888,mipmaps:false,version_minor:2,..Default::default()})?;
    gma.add("materials/hp_rootspeed117/quad.vtf",GmaSource::Bytes(image)).unwrap();
    gma.add("materials/hp_rootspeed117/quad.vmt",GmaSource::Bytes(b"SpriteCard { \"$basetexture\" \"hp_rootspeed117/quad\" \"$vertexcolor\" \"1\" \"$vertexalpha\" \"1\" \"$translucent\" \"1\" \"$splinetype\" \"0\" \"$depthblend\" \"0\" \"$nofog\" \"1\" }".to_vec())).unwrap();
    let mut doc=Document::new();
    for name in ["none","pi","p180","npi","root180_init0","root0_init180","p180_int","p180_string","p1_bool"] {
        let id=doc.create_system(&format!("hp_rootspeed117_{name}"))?;
        doc.set_attrs(id,attrs(vec![("max_particles",Value::Int(1)),("radius",Value::Float(24.)),("color",Value::Color([255;4])),("material",Value::Str("hp_rootspeed117/quad".into())),("bounding_box_min",Value::Vec3([0.;3])),("bounding_box_max",Value::Vec3([0.;3]))]))?;
        if name!="none" {
            let speed=match name {"pi"=>Value::Float(std::f32::consts::PI),"npi"=>Value::Float(-std::f32::consts::PI),"root0_init180"=>Value::Float(0.),"p180_int"=>Value::Int(180),"p180_string"=>Value::Str("180".into()),"p1_bool"=>Value::Bool(true),_=>Value::Float(180.)};
            doc.set_attr(id,"rotation_speed",speed)?;
        }
        doc.add_operator_with_params(id,Category::Emitters,"emit_instantaneously",attrs(vec![("num_to_emit",Value::Int(1))]))?;
        doc.add_operator_with_params(id,Category::Initializers,"Position Within Box Random",attrs(vec![("min",Value::Vec3([0.,0.,0.])),("max",Value::Vec3([0.,0.,0.]))]))?;
        doc.add_operator_with_params(id,Category::Initializers,"Lifetime Random",attrs(vec![("lifetime_min",Value::Float(100.)),("lifetime_max",Value::Float(100.))]))?;
        let velocity=[0.;3];
        doc.add_operator_with_params(id,Category::Initializers,"Velocity Random",attrs(vec![("speed_in_local_coordinate_system_min",Value::Vec3(velocity)),("speed_in_local_coordinate_system_max",Value::Vec3(velocity))]))?;
        if name=="initial30" {doc.add_operator_with_params(id,Category::Initializers,"Rotation Random",attrs(vec![("rotation_initial",Value::Float(30.)),("rotation_offset_min",Value::Float(0.)),("rotation_offset_max",Value::Float(0.))]))?;}
        if name=="root180_init0" || name=="root0_init180" {
            doc.add_operator_with_params(id,Category::Initializers,"Rotation Speed Random",attrs(vec![("rotation_speed_constant",Value::Float(if name=="root180_init0"{0.}else{180.})),("rotation_speed_random_min",Value::Float(0.)),("rotation_speed_random_max",Value::Float(0.))]))?;
        }
        doc.add_operator_with_params(id,Category::Operators,"Movement Basic",attrs(vec![("gravity",Value::Vec3([0.;3]))]))?;
        doc.add_operator_with_params(id,Category::Operators,"Remap Scalar",attrs(vec![("input field",Value::Int(5)),("output field",Value::Int(0)),("input minimum",Value::Float(-256.)),("input maximum",Value::Float(256.)),("output minimum",Value::Float(-256.)),("output maximum",Value::Float(256.))]))?;
        doc.add_operator_with_params(id,Category::Renderers,"render_animated_sprites",attrs(vec![("orientation_type",Value::Int(0))]))?;


    }
    std::fs::write(folder.join("hp_rootspeed117.pcf"),doc.to_bytes()?)?;
    gma.write(&folder.join("hp_rootspeed117.gma"))?;
    Ok(())
}
