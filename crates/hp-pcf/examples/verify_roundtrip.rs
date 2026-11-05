use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = PathBuf::from(args.next().ok_or("expected input PCF")?);
    let output = PathBuf::from(args.next().ok_or("expected output PCF")?);
    let bytes = std::fs::read(&input)?;
    let original = hp_pcf::editor::Document::parse(&bytes)?;
    let encoded = original.to_bytes()?;
    let decoded = hp_pcf::editor::Document::parse(&encoded)?;
    if original.dmx().elements != decoded.dmx().elements
        || original.dmx().ids != decoded.dmx().ids
        || original.definitions() != decoded.definitions()
    {
        return Err("roundtrip did not preserve the document".into());
    }
    std::fs::write(&output, &encoded)?;
    println!(
        "preserved {} systems, {} elements, {} attributes; {} -> {} bytes; output {}",
        original.systems().len(),
        original.dmx().elements.len(),
        original.dmx().elements.iter().map(|e| e.attrs.len()).sum::<usize>(),
        bytes.len(),
        encoded.len(),
        output.display()
    );
    for preset in hp_pcf::editor::Preset::ALL {
        let doc = hp_pcf::editor::Document::preset(preset);
        let folder = output.parent().ok_or("output must have a parent folder")?.join("addon");
        let exported = doc.export_addon(&folder, &format!("hp_{}.pcf", preset.name()))?;
        println!("{}: {}", preset.name(), exported.lua.replace('\n', "; "));
    }
    Ok(())
}
