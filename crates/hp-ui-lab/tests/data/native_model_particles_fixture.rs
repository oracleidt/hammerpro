use hp_pcf::dmx::Value;
use hp_pcf::editor::{Category,Document};
use hp_gma::{AddonJson,GmaSource,GmaWriter};
use std::path::Path;
fn attrs(v:Vec<(&str,Value)>)->Vec<(String,Value)>{v.into_iter().map(|(k,v)|(k.into(),v)).collect()}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let folder=std::env::args_os().nth(1).ok_or("output")?;let folder=Path::new(&folder);
 let assets=std::env::args_os().nth(2).ok_or("compiled model game directory")?;let assets=Path::new(&assets);
 let mut gma=GmaWriter::new(AddonJson{title:"Owned native dynamic model particle reference".into(),..Default::default()});
 for path in ["models/hp_model117/probe.mdl","models/hp_model117/probe.vvd","models/hp_model117/probe.dx90.vtx","models/hp_model117/probe.dx80.vtx"]{gma.add(path,GmaSource::Bytes(std::fs::read(assets.join(path))?)).unwrap();}
 gma.add("materials/hp_model117/gray.vmt",GmaSource::Bytes(br#"VertexLitGeneric { "$basetexture" "hp_model117/white" "$model" "1" "$vertexcolor" "1" "$vertexalpha" "1" }"#.to_vec())).unwrap();
 let white=hp_vtf::compose(&[vec![hp_vtf::RgbaImage{width:8,height:8,pixels:[255;4].repeat(64)}]],&hp_vtf::ComposeOptions{format:hp_vtf::ImageFormat::Rgba8888,mipmaps:false,version_minor:2,..Default::default()})?;
 gma.add("materials/hp_model117/white.vtf",GmaSource::Bytes(white)).unwrap();
 let mut doc=Document::new();
 for name in ["base","half","double","rotation90","yaw90","normal_x","normal_y","normal_z","orient_default","gray","alpha","error","stock"]{
  let id=doc.create_system(&format!("hp_modeldyn117_{name}"))?;
  doc.set_attrs(id,attrs(vec![("max_particles",Value::Int(1)),("radius",Value::Float(match name{"half"=>0.5,"double"=>2.,_=>1.})),("color",Value::Color(match name{"gray"=>[128,128,128,255],"alpha"=>[255,255,255,128],_=>[255;4]})),("material",Value::Str("hp_model117/gray".into())),("bounding_box_min",Value::Vec3([0.;3])),("bounding_box_max",Value::Vec3([0.;3]))]))?;
  if name=="rotation90"{doc.set_attr(id,"rotation",Value::Float(std::f32::consts::FRAC_PI_2))?;}
  if name.starts_with("normal_"){doc.set_attr(id,"normal",Value::Vec3(match name{"normal_x"=>[1.,0.,0.],"normal_y"=>[0.,1.,0.],_=>[0.,0.,1.]}))?;}
  doc.add_operator_with_params(id,Category::Emitters,"emit_instantaneously",attrs(vec![("num_to_emit",Value::Int(1))]))?;
  doc.add_operator_with_params(id,Category::Initializers,"Position Within Box Random",attrs(vec![("min",Value::Vec3([10.,0.,0.])),("max",Value::Vec3([10.,0.,0.]))]))?;
  doc.add_operator_with_params(id,Category::Initializers,"Lifetime Random",attrs(vec![("lifetime_min",Value::Float(100.)),("lifetime_max",Value::Float(100.))]))?;
  if name=="yaw90"{doc.add_operator_with_params(id,Category::Initializers,"Rotation Yaw Random",attrs(vec![("yaw_initial",Value::Float(90.)),("yaw_offset_min",Value::Float(0.)),("yaw_offset_max",Value::Float(0.))]))?;}
  let model=match name{"error"=>"error.mdl","stock"=>"hunter/blocks/cube025x025x025.mdl",_=>"hp_model117/probe.mdl"};
  doc.add_operator_with_params(id,Category::Renderers,"Render models",attrs(vec![("sequence 0 model",Value::Str(model.into())),("orient model z to normal",Value::Bool(name.starts_with("normal_")||name=="orient_default"))]))?;
 }
 std::fs::write(folder.join("hp_modeldyn117.pcf"),doc.to_bytes()?)?;gma.write(&folder.join("hp_modeldyn117.gma"))?;Ok(())
}
