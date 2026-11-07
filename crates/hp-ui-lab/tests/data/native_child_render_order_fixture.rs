use hp_pcf::dmx::Value;
use hp_pcf::editor::{Category, Document};
use hp_gma::{AddonJson, GmaSource, GmaWriter};
use std::path::Path;
fn attrs(v: Vec<(&str,Value)>)->Vec<(String,Value)>{v.into_iter().map(|(k,v)|(k.into(),v)).collect()}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let output=std::env::args_os().nth(1).ok_or("output directory")?;let output=Path::new(&output);
 let mut gma=GmaWriter::new(AddonJson{title:"Owned child collection draw ordering".into(),..Default::default()});
 let pixels=hp_vtf::compose(&[vec![hp_vtf::RgbaImage{width:8,height:8,pixels:[255;4].repeat(64)}]],&hp_vtf::ComposeOptions{format:hp_vtf::ImageFormat::Rgba8888,mipmaps:false,version_minor:2,..Default::default()})?;
 gma.add("materials/hp_childorder117/white.vtf",GmaSource::Bytes(pixels)).unwrap();
 gma.add("materials/hp_childorder117/white.vmt",GmaSource::Bytes(b"SpriteCard { \"$basetexture\" \"hp_childorder117/white\" \"$vertexcolor\" \"1\" \"$vertexalpha\" \"1\" \"$translucent\" \"1\" \"$splinetype\" \"0\" \"$depthblend\" \"0\" \"$nofog\" \"1\" }".to_vec())).unwrap();
 let mut doc=Document::new();let mut children=Vec::new();
 for (name,y,color,count) in [("red_one",0.,[255,0,0,128],1),("red_two",0.,[255,0,0,128],2),("blue_one",32.,[0,0,255,128],1)] {
  let id=doc.create_system(&format!("hp_childorder117_{name}"))?;
  doc.set_attrs(id,attrs(vec![("max_particles",Value::Int(1)),("radius",Value::Float(24.)),("color",Value::Color(color)),("material",Value::Str("hp_childorder117/white".into())),("bounding_box_min",Value::Vec3([0.;3])),("bounding_box_max",Value::Vec3([0.;3]))]))?;
  doc.add_operator_with_params(id,Category::Emitters,"emit_instantaneously",attrs(vec![("num_to_emit",Value::Int(1))]))?;
  doc.add_operator_with_params(id,Category::Initializers,"Position Within Box Random",attrs(vec![("min",Value::Vec3([10.,y,0.])),("max",Value::Vec3([10.,y,0.]))]))?;
  doc.add_operator_with_params(id,Category::Initializers,"Lifetime Random",attrs(vec![("lifetime_min",Value::Float(100.)),("lifetime_max",Value::Float(100.))]))?;
  for _ in 0..count {doc.add_operator(id,Category::Renderers,"render_animated_sprites")?;}
  children.push(id);
 }
 let mut roots=Vec::new();
 for (name,order) in [("rb",vec![0,2]),("br",vec![2,0]),("rbr",vec![0,2,0]),("brr",vec![2,0,0]),("r2b",vec![1,2]),("br2",vec![2,1])] {
  let id=doc.create_system(&format!("hp_childorder117_{name}"))?;
  doc.set_attrs(id,attrs(vec![("max_particles",Value::Int(0)),("bounding_box_min",Value::Vec3([10.,0.,0.])),("bounding_box_max",Value::Vec3([10.,32.,0.]))]))?;
  for child in order {doc.add_child(id,children[child],0.)?;}
  roots.push(id);
 }
 for (name,nested) in [("nested_rb",roots[0]),("nested_br",roots[1])] {
  let id=doc.create_system(&format!("hp_childorder117_{name}"))?;
  doc.set_attrs(id,attrs(vec![("max_particles",Value::Int(0)),("bounding_box_min",Value::Vec3([10.,0.,0.])),("bounding_box_max",Value::Vec3([10.,32.,0.]))]))?;
  doc.add_child(id,nested,0.)?;doc.add_child(id,children[0],0.)?;
 }
 for (name,extra) in [("drawn_rb",false),("drawn_rbr",true)] {
  let id=doc.duplicate_system(children[0],&format!("hp_childorder117_{name}"))?;
  doc.add_child(id,children[2],0.)?;
  if extra {doc.add_child(id,children[0],0.)?;}
 }
 std::fs::write(output.join("hp_childorder117.pcf"),doc.to_bytes()?)?;
 gma.write(&output.join("hp_childorder117.gma"))?;Ok(())
}
