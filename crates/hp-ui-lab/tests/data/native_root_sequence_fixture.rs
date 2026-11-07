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
    let mut image=hp_vtf::compose(&[vec![hp_vtf::RgbaImage{width:8,height:8,pixels}]],&hp_vtf::ComposeOptions{format:hp_vtf::ImageFormat::Rgba8888,mipmaps:false,version_minor:3,..Default::default()})?;
    // Authored three one-frame sequences, each inside one solid-color quadrant.
    let mut sheet=Vec::new();
    sheet.extend_from_slice(&0u32.to_le_bytes());sheet.extend_from_slice(&3u32.to_le_bytes());
    for (number,uv) in [(0u32,[1./16.,1./16.,7./16.,7./16.]),(1,[9./16.,1./16.,15./16.,7./16.]),(2,[1./16.,9./16.,7./16.,15./16.])] {
        for value in [number,1,1] {sheet.extend_from_slice(&value.to_le_bytes());}
        sheet.extend_from_slice(&1f32.to_le_bytes());sheet.extend_from_slice(&1f32.to_le_bytes());
        for value in uv {sheet.extend_from_slice(&f32::to_le_bytes(value));}
    }
    assert_eq!(u32::from_le_bytes(image[12..16].try_into().unwrap()),96);
    let sheet_offset=u32::from_le_bytes(image[92..96].try_into().unwrap())+8;
    image.splice(96..96,[0;8]);image[12..16].copy_from_slice(&104u32.to_le_bytes());image[68..72].copy_from_slice(&3u32.to_le_bytes());
    image[84..88].copy_from_slice(&104u32.to_le_bytes());
    image[88..92].copy_from_slice(&[16,0,0,0]);image[92..96].copy_from_slice(&sheet_offset.to_le_bytes());
    image[96..100].copy_from_slice(&[48,0,0,0]);image[100..104].copy_from_slice(&(sheet_offset+4+sheet.len() as u32).to_le_bytes());
    let mut chunk=(sheet.len() as u32).to_le_bytes().to_vec();chunk.extend_from_slice(&sheet);image.splice(sheet_offset as usize..sheet_offset as usize,chunk);
    assert_eq!(hp_pcf::sheet_from_vtf(&image).unwrap().sequences.len(),3);
    assert_eq!(hp_vtf::decode_rgba8(&image,&Default::default())?.pixels.len(),256);
    gma.add("materials/hp_rootseqsheet117/quad.vtf",GmaSource::Bytes(image)).unwrap();
    gma.add("materials/hp_rootseqsheet117/quad.vmt",GmaSource::Bytes(b"SpriteCard { \"$basetexture\" \"hp_rootseqsheet117/quad\" \"$vertexcolor\" \"1\" \"$vertexalpha\" \"1\" \"$translucent\" \"1\" \"$splinetype\" \"0\" \"$depthblend\" \"0\" \"$nofog\" \"1\" }".to_vec())).unwrap();
    gma.add("materials/hp_rootseqsheet117/dual.vmt",GmaSource::Bytes(br#"SpriteCard { "$basetexture" "hp_rootseqsheet117/quad" "$vertexcolor" "1" "$vertexalpha" "1" "$translucent" "1" "$splinetype" "0" "$depthblend" "0" "$nofog" "1" "$dualsequence" "1" "$sequence_blend_mode" "0" }"#.to_vec())).unwrap();
    let mut doc=Document::new();
    for name in ["first_none", "first_one", "first_two", "first_root1_init0", "first_root0_init2", "second_none", "second_one", "second_two", "second_root1_init0", "second_root0_init2"] {
        let id=doc.create_system(&format!("hp_rootseqsheet117_{name}"))?;
        doc.set_attrs(id,attrs(vec![("max_particles",Value::Int(1)),("radius",Value::Float(24.)),("color",Value::Color([255;4])),("material",Value::Str("hp_rootseqsheet117/quad".into())),("bounding_box_min",Value::Vec3([0.;3])),("bounding_box_max",Value::Vec3([0.;3]))]))?;
        let second=name.starts_with("second_");
        if second {doc.set_attr(id,"material",Value::Str("hp_rootseqsheet117/dual".into()))?;}
        let case=name.split_once('_').unwrap().1;
        if case!="none" {
            let value=match case {"two"=>Value::Int(2),"root0_init2"=>Value::Int(0),_=>Value::Int(1)};
            doc.set_attr(id,if second{"sequence_number 1"}else{"sequence_number"},value)?;
        }
        doc.add_operator_with_params(id,Category::Emitters,"emit_instantaneously",attrs(vec![("num_to_emit",Value::Int(1))]))?;
        doc.add_operator_with_params(id,Category::Initializers,"Position Within Box Random",attrs(vec![("min",Value::Vec3([0.,0.,0.])),("max",Value::Vec3([0.,0.,0.]))]))?;
        doc.add_operator_with_params(id,Category::Initializers,"Lifetime Random",attrs(vec![("lifetime_min",Value::Float(100.)),("lifetime_max",Value::Float(100.))]))?;
        let velocity=[0.;3];
        doc.add_operator_with_params(id,Category::Initializers,"Velocity Random",attrs(vec![("speed_in_local_coordinate_system_min",Value::Vec3(velocity)),("speed_in_local_coordinate_system_max",Value::Vec3(velocity))]))?;
        if case=="root1_init0" || case=="root0_init2" {
            let value=if case=="root1_init0"{0}else{2};
            doc.add_operator_with_params(id,Category::Initializers,if second{"Sequence Two Random"}else{"Sequence Random"},attrs(vec![("sequence_min",Value::Int(value)),("sequence_max",Value::Int(value))]))?;
        }
        doc.add_operator_with_params(id,Category::Operators,"Movement Basic",attrs(vec![("gravity",Value::Vec3([0.;3]))]))?;
        doc.add_operator_with_params(id,Category::Operators,"Remap Scalar",attrs(vec![("input field",Value::Int(if second{13}else{9})),("output field",Value::Int(0)),("input minimum",Value::Float(-256.)),("input maximum",Value::Float(256.)),("output minimum",Value::Float(-256.)),("output maximum",Value::Float(256.))]))?;
        doc.add_operator_with_params(id,Category::Renderers,"render_animated_sprites",attrs(vec![("orientation_type",Value::Int(0))]))?;


    }
    std::fs::write(folder.join("hp_rootseqsheet117.pcf"),doc.to_bytes()?)?;
    gma.write(&folder.join("hp_rootseqsheet117.gma"))?;
    Ok(())
}
