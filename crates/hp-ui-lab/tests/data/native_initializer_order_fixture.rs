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
    gma.add("materials/hp_initorder117/quad.vtf",GmaSource::Bytes(image)).unwrap();
    gma.add("materials/hp_initorder117/quad.vmt",GmaSource::Bytes(b"SpriteCard { \"$basetexture\" \"hp_initorder117/quad\" \"$vertexcolor\" \"1\" \"$vertexalpha\" \"1\" \"$translucent\" \"1\" \"$splinetype\" \"0\" \"$depthblend\" \"0\" \"$nofog\" \"1\" }".to_vec())).unwrap();
    let mut doc=Document::new();
    for name in ["count", "radius_count", "count_radius", "radius_count_mul", "vector", "box_vector", "vector_box", "box_vector_mul", "color_vector", "color_vector_mul"] {
        let id=doc.create_system(&format!("hp_initorder117_{name}"))?;
        doc.set_attrs(id,attrs(vec![("max_particles",Value::Int(4)),("radius",Value::Float(24.)),("color",Value::Color([255;4])),("material",Value::Str("hp_initorder117/quad".into())),("bounding_box_min",Value::Vec3([0.;3])),("bounding_box_max",Value::Vec3([0.;3]))]))?;
        doc.add_operator_with_params(id,Category::Emitters,"emit_instantaneously",attrs(vec![("num_to_emit",Value::Int(4))]))?;
        let is_count=name.contains("count");
        let is_color=name.starts_with("color_");
        let multiplier=name.ends_with("mul");
        let base=if is_count{"Radius Random"}else if is_color{"Color Random"}else{"Position Within Box Random"};
        let base_params=if is_count{attrs(vec![("radius_min",Value::Float(7.)),("radius_max",Value::Float(7.))])}else if is_color{attrs(vec![("color1",Value::Color([128,128,128,255])),("color2",Value::Color([128,128,128,255]))])}else{attrs(vec![("min",Value::Vec3([7.,8.,9.])),("max",Value::Vec3([7.,8.,9.]))])};
        if name.starts_with("radius_")||name.starts_with("box_")||is_color {doc.add_operator_with_params(id,Category::Initializers,base,base_params.clone())?;}
        if is_count {
            doc.add_operator_with_params(id,Category::Initializers,"Remap Particle Count to Scalar",attrs(vec![("input minimum",Value::Int(0)),("input maximum",Value::Int(3)),("output field",Value::Int(3)),("output minimum",Value::Float(1.)),("output maximum",Value::Float(4.)),("output is scalar of initial random range",Value::Bool(multiplier))]))?;
        }else{
            doc.set_attr(id,"sequence_number",Value::Int(1))?;
            doc.add_operator_with_params(id,Category::Initializers,"Remap Scalar to Vector",attrs(vec![("input field",Value::Int(9)),("input minimum",Value::Float(0.)),("input maximum",Value::Float(1.)),("output field",Value::Int(if is_color{6}else{0})),("output minimum",Value::Vec3([1.,2.,3.])),("output maximum",Value::Vec3(if is_color{[0.5,0.25,1.]}else{[10.,20.,30.]})),("use local system",Value::Bool(false)),("output is scalar of initial random range",Value::Bool(multiplier))]))?;
        }
        if name.ends_with("_radius")||name.ends_with("_box") {doc.add_operator_with_params(id,Category::Initializers,base,base_params)?;}
        doc.add_operator_with_params(id,Category::Initializers,"Lifetime Random",attrs(vec![("lifetime_min",Value::Float(100.)),("lifetime_max",Value::Float(100.))]))?;
        let velocity=[0.;3];
        doc.add_operator_with_params(id,Category::Initializers,"Velocity Random",attrs(vec![("speed_in_local_coordinate_system_min",Value::Vec3(velocity)),("speed_in_local_coordinate_system_max",Value::Vec3(velocity))]))?;
        doc.add_operator_with_params(id,Category::Operators,"Movement Basic",attrs(vec![("gravity",Value::Vec3([0.;3]))]))?;
        if !is_color {
        doc.add_operator_with_params(id,Category::Operators,"Remap Scalar",attrs(vec![("input field",Value::Int(if is_count{3}else if is_color{6}else{0})),("output field",Value::Int(0)),("input minimum",Value::Float(-256.)),("input maximum",Value::Float(256.)),("output minimum",Value::Float(-256.)),("output maximum",Value::Float(256.))]))?;
        }
        doc.add_operator_with_params(id,Category::Renderers,"render_animated_sprites",attrs(vec![("orientation_type",Value::Int(0))]))?;


    }
    std::fs::write(folder.join("hp_initorder117.pcf"),doc.to_bytes()?)?;
    gma.write(&folder.join("hp_initorder117.gma"))?;
    Ok(())
}
