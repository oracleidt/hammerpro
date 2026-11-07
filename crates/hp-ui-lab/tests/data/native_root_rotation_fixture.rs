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
    gma.add("materials/hp_rootrender117/quad.vtf",GmaSource::Bytes(image)).unwrap();
    gma.add("materials/hp_rootrender117/quad.vmt",GmaSource::Bytes(b"SpriteCard { \"$basetexture\" \"hp_rootrender117/quad\" \"$vertexcolor\" \"1\" \"$vertexalpha\" \"1\" \"$translucent\" \"1\" \"$splinetype\" \"0\" \"$depthblend\" \"0\" \"$nofog\" \"1\" }".to_vec())).unwrap();
    let mut doc=Document::new();
    for name in ["none","r30","r90","rm90","r30_init0","r0_init30"] {
        let id=doc.create_system(&format!("hp_rootrender117_{name}"))?;
        doc.set_attrs(id,attrs(vec![("max_particles",Value::Int(1)),("radius",Value::Float(24.)),("color",Value::Color([255;4])),("material",Value::Str("hp_rootrender117/quad".into())),("bounding_box_min",Value::Vec3([0.;3])),("bounding_box_max",Value::Vec3([0.;3]))]))?;
        if name!="none" {doc.set_attr(id,"rotation",Value::Float(match name {"r30"|"r30_init0"=>30.,"r90"=>90.,"rm90"=>-90.,_=>0.}))?;}
        doc.add_operator_with_params(id,Category::Emitters,"emit_instantaneously",attrs(vec![("num_to_emit",Value::Int(1))]))?;
        doc.add_operator_with_params(id,Category::Initializers,"Position Within Box Random",attrs(vec![("min",Value::Vec3([10.,0.,0.])),("max",Value::Vec3([10.,0.,0.]))]))?;
        doc.add_operator_with_params(id,Category::Initializers,"Lifetime Random",attrs(vec![("lifetime_min",Value::Float(100.)),("lifetime_max",Value::Float(100.))]))?;
        let velocity=[0.;3];
        doc.add_operator_with_params(id,Category::Initializers,"Velocity Random",attrs(vec![("speed_in_local_coordinate_system_min",Value::Vec3(velocity)),("speed_in_local_coordinate_system_max",Value::Vec3(velocity))]))?;
        if name=="r0_init30" {doc.add_operator_with_params(id,Category::Initializers,"Rotation Random",attrs(vec![("rotation_initial",Value::Float(30.)),("rotation_offset_min",Value::Float(0.)),("rotation_offset_max",Value::Float(0.))]))?;}
        if name=="r30_init0" {doc.add_operator_with_params(id,Category::Initializers,"Rotation Random",attrs(vec![("rotation_initial",Value::Float(0.)),("rotation_offset_min",Value::Float(0.)),("rotation_offset_max",Value::Float(0.))]))?;}
        doc.add_operator_with_params(id,Category::Operators,"Movement Basic",attrs(vec![("gravity",Value::Vec3([0.;3]))]))?;
        doc.add_operator_with_params(id,Category::Renderers,"render_animated_sprites",attrs(vec![("orientation_type",Value::Int(0))]))?;


    }
    for (name,value) in [("r30_int",Value::Int(30)),("r30_string",Value::Str("30".into())),("r1_bool",Value::Bool(true))] {
        let base=doc.find_system("hp_rootrender117_none").unwrap();
        let id=doc.duplicate_system(base,&format!("hp_rootrender117_{name}"))?;
        doc.set_attr(id,"rotation",value)?;
    }
    std::fs::write(folder.join("hp_rootrender117.pcf"),doc.to_bytes()?)?;
    gma.write(&folder.join("hp_rootrender117.gma"))?;
    Ok(())
}
