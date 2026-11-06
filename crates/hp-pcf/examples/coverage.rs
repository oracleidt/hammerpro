use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn collect(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if !entry.file_type()?.is_symlink() {
                collect(&entry.path(), files)?;
            }
        }
    } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pcf")) {
        files.push(path.to_owned());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    for path in std::env::args_os().skip(1) {
        collect(Path::new(&path), &mut files)?;
    }
    files.sort();
    files.dedup();
    if files.is_empty() {
        return Err("expected PCF files or folders".into());
    }
    let mut unsupported = BTreeMap::new();
    let mut systems = 0;
    let mut incomplete = 0;
    for path in &files {
        for system in hp_pcf::parse_file(&std::fs::read(path)?)? {
            systems += 1;
            let plan = hp_pcf::Plan::new(&system);
            incomplete += usize::from(!plan.unsupported.is_empty());
            for function in &plan.unsupported {
                let entry = unsupported.entry(function.clone()).or_insert_with(|| (0usize, String::new()));
                entry.0 += 1;
                if entry.1.is_empty() {
                    let op = [
                        ("emitter", &system.emitters),
                        ("initializer", &system.initializers),
                        ("operator", &system.operators),
                        ("force", &system.forces),
                        ("constraint", &system.constraints),
                        ("renderer", &system.renderers),
                    ]
                    .into_iter()
                    .flat_map(|(category, ops)| ops.iter().map(move |op| (category, op)))
                    .find(|(_, op)| &op.function == function);
                    entry.1 = format!("{} {:?}: {op:?}", path.display(), system.name);
                }
            }
        }
    }
    println!("files={} systems={systems} incomplete={incomplete}", files.len());
    for (function, (count, example)) in unsupported {
        println!("{count}\t{function}\t{example}");
    }
    Ok(())
}
