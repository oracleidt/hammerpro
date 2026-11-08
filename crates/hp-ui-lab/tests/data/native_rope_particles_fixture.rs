use hp_pcf::dmx::Value;
use hp_pcf::editor::{Category,Document};
use hp_gma::{AddonJson,GmaSource,GmaWriter};
use std::path::Path;
fn attrs(v:Vec<(&str,Value)>)->Vec<(String,Value)>{v.into_iter().map(|(k,v)|(k.into(),v)).collect()}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let out=std::env::args_os().nth(1).ok_or("output")?;let out=Path::new(&out);
 let mut gma=GmaWriter::new(AddonJson{title:"Owned native rope rendering reference".into(),..Default::default()});
 for name in ["quad","white"]{
 let pixels=(0..256).flat_map(|i|if name=="white"{[255;4]}else if i/16<4{[255,0,0,255]}else if i/16<8{[0,255,0,255]}else if i/16<12{[0,0,255,255]}else{[255,255,0,255]}).collect();
 let vtf=hp_vtf::compose(&[vec![hp_vtf::RgbaImage{width:16,height:16,pixels}]],&hp_vtf::ComposeOptions{format:hp_vtf::ImageFormat::Rgba8888,mipmaps:false,version_minor:2,..Default::default()})?;
 gma.add(&format!("materials/hp_rope117/{name}.vtf"),GmaSource::Bytes(vtf)).unwrap();
 for (kind,shader) in [("cpu","UnlitGeneric"),("spline","SpriteCard")]{
 let mat=format!("{shader} {{ \"$basetexture\" \"hp_rope117/{name}\" \"$vertexcolor\" \"1\" \"$vertexalpha\" \"1\" \"$translucent\" \"1\" \"$splinetype\" \"1\" \"$depthblend\" \"0\" \"$nofog\" \"1\" }}");
 gma.add(&format!("materials/hp_rope117/{kind}_{name}.vmt"),GmaSource::Bytes(mat.into_bytes())).unwrap();}}
 let mut doc=Document::new();
 for kind in ["cpu","spline"]{for case in ["base","sub1","sub6","texel1","offset","gradient","taper","straight"]{
 let id=doc.create_system(&format!("hp_rope117_{kind}_{case}"))?;
 doc.set_attrs(id,attrs(vec![("max_particles",Value::Int(3)),("radius",Value::Float(16.)),("material",Value::Str(format!("hp_rope117/{kind}_{}",if case=="gradient"||case=="taper"{"white"}else{"quad"}))),("color",Value::Color([255;4])),("bounding_box_min",Value::Vec3([0.;3])),("bounding_box_max",Value::Vec3([0.;3]))]))?;
 doc.add_operator_with_params(id,Category::Emitters,"emit_instantaneously",attrs(vec![("num_to_emit",Value::Int(3)),("emission_start_time",Value::Float(0.1))]))?;
 doc.add_operator_with_params(id,Category::Initializers,"Lifetime Random",attrs(vec![("lifetime_min",Value::Float(100.)),("lifetime_max",Value::Float(100.))]))?;
 doc.add_operator_with_params(id,Category::Initializers,"Position Along Path Sequential",attrs(vec![("start control point number",Value::Int(0)),("end control point number",Value::Int(1)),("particles to map from start to end",Value::Float(3.)),("bulge",Value::Float(if case=="straight"{0.}else{0.5})),("bulge control 0=random 1=orientation of start pnt 2=orientation of end point",Value::Int(1)),("mid point position",Value::Float(0.5))]))?;
 if case=="gradient"||case=="taper"{
 doc.add_operator_with_params(id,Category::Initializers,"Remap Particle Count to Scalar",attrs(vec![("input minimum",Value::Int(0)),("input maximum",Value::Int(2)),("output field",Value::Int(if case=="taper"{3}else{7})),("output minimum",Value::Float(if case=="taper"{4.}else{0.25})),("output maximum",Value::Float(if case=="taper"{32.}else{1.}))]))?;
 if case=="gradient"{doc.add_operator_with_params(id,Category::Initializers,"Remap Scalar to Vector",attrs(vec![("input field",Value::Int(7)),("input minimum",Value::Float(0.25)),("input maximum",Value::Float(1.)),("output field",Value::Int(6)),("output minimum",Value::Vec3([1.,0.,0.])),("output maximum",Value::Vec3([0.,0.,1.]))]))?;}
 }
 doc.add_operator_with_params(id,Category::Renderers,"render_rope",attrs(vec![("subdivision_count",Value::Int(match case{"sub1"=>1,"sub6"=>6,_=>3})),("texel_size",Value::Float(if case=="texel1"{1.}else{4.})),("texture_offset",Value::Float(if case=="offset"{0.25}else{0.}))]))?;
 }}
 std::fs::write(out.join("hp_rope117.pcf"),doc.to_bytes()?)?;gma.write(&out.join("hp_rope117.gma"))?;Ok(())}
