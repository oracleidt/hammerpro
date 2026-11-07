use hp_pcf::dmx::Value;
use hp_pcf::editor::{Category, Document};
use hp_gma::{AddonJson, GmaSource, GmaWriter};
use std::path::Path;
fn attrs(v: Vec<(&str, Value)>) -> Vec<(String, Value)> { v.into_iter().map(|(k,v)|(k.into(),v)).collect() }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let folder=std::env::args_os().nth(1).ok_or("output directory")?;
    let folder=Path::new(&folder);
    let mut gma=GmaWriter::new(AddonJson { title:"Own screen-velocity renderer reference".into(), ..Default::default() });
    let pixels:Vec<u8>=(0..64).flat_map(|i| if i/8<4 { if i%8<4 {[255,0,0,255]}else{[0,0,255,255]} }else if i%8<4 {[0,255,0,255]}else{[255,255,0,255]}).collect();
    for mode in 0..4 {
        let flags=(if mode&1!=0 {hp_vtf::flags::CLAMPS}else{0}) | (if mode&2!=0 {hp_vtf::flags::CLAMPT}else{0});
        let image=hp_vtf::compose(&[vec![hp_vtf::RgbaImage{width:8,height:8,pixels:pixels.clone()}]],&hp_vtf::ComposeOptions{flags,format:hp_vtf::ImageFormat::Rgba8888,mipmaps:false,version_minor:2,..Default::default()})?;
        gma.add(&format!("materials/hp_velocity117/quad_{mode}.vtf"),GmaSource::Bytes(image.clone())).unwrap();
        if mode==0 {gma.add("materials/hp_velocity117/quad.vtf",GmaSource::Bytes(image)).unwrap();}
    }
    gma.add("materials/hp_velocity117/quad.vmt",GmaSource::Bytes(b"SpriteCard { \"$basetexture\" \"hp_velocity117/quad\" \"$vertexcolor\" \"1\" \"$vertexalpha\" \"1\" \"$translucent\" \"1\" \"$splinetype\" \"0\" \"$depthblend\" \"0\" \"$nofog\" \"1\" }".to_vec())).unwrap();
    let mut doc=Document::new();
    for name in ["plain","only","before","after","rate0","forward90","rate1","rate30","zero","depth","initial30"] {
        let id=doc.create_system(&format!("hp_velocity117_{name}"))?;
        doc.set_attrs(id,attrs(vec![("max_particles",Value::Int(1)),("radius",Value::Float(24.)),("color",Value::Color([255;4])),("material",Value::Str("hp_velocity117/quad".into())),("bounding_box_min",Value::Vec3([0.;3])),("bounding_box_max",Value::Vec3([0.;3]))]))?;
        doc.add_operator_with_params(id,Category::Emitters,"emit_instantaneously",attrs(vec![("num_to_emit",Value::Int(1))]))?;
        doc.add_operator_with_params(id,Category::Initializers,"Position Within Box Random",attrs(vec![("min",Value::Vec3([10.,0.,0.])),("max",Value::Vec3([10.,0.,0.]))]))?;
        doc.add_operator_with_params(id,Category::Initializers,"Lifetime Random",attrs(vec![("lifetime_min",Value::Float(100.)),("lifetime_max",Value::Float(100.))]))?;
        let velocity=if name=="zero"{[0.;3]}else if name=="depth"{[0.,100.,0.]}else{[100.,0.,100.]};
        doc.add_operator_with_params(id,Category::Initializers,"Velocity Random",attrs(vec![("speed_in_local_coordinate_system_min",Value::Vec3(velocity)),("speed_in_local_coordinate_system_max",Value::Vec3(velocity))]))?;
        if name=="initial30" {doc.add_operator_with_params(id,Category::Initializers,"Rotation Random",attrs(vec![("rotation_initial",Value::Float(30.)),("rotation_offset_min",Value::Float(0.)),("rotation_offset_max",Value::Float(0.))]))?;}
        doc.add_operator_with_params(id,Category::Operators,"Movement Basic",attrs(vec![("gravity",Value::Vec3([0.;3]))]))?;
        if name=="after" || name=="plain" {doc.add_operator(id,Category::Renderers,"render_animated_sprites")?;}
        if name!="plain" {doc.add_operator_with_params(id,Category::Renderers,"render_screen_velocity_rotate",attrs(vec![("rotate_rate(dps)",Value::Float(match name {"rate0"=>0.,"rate1"=>1.,"rate30"=>30.,_=>3600.})),("forward_angle",Value::Float(if name=="forward90"{90.}else{0.}))]))?;}
        if name!="plain" && name!="after" && name!="only" {doc.add_operator(id,Category::Renderers,"render_animated_sprites")?;}
    }
    std::fs::write(folder.join("hp_velocity117.pcf"),doc.to_bytes()?)?;
    gma.write(&folder.join("hp_velocity117.gma"))?;
    Ok(())
}
