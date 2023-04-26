use anyhow::Result;
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn gen_source_files_list() -> Result<()> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")?;
    let out_dir = env::var("OUT_DIR")?;

    let mut all_files: Vec<_> = walkdir::WalkDir::new(Path::new(&manifest_dir).join("src"))
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.metadata().map_or(false, |m| m.is_file())
                && Path::new(e.file_name())
                    .extension()
                    .map_or(false, |ex| ex == "rs")
        })
        .filter_map(|e| pathdiff::diff_paths(e.into_path(), &manifest_dir))
        .collect();

    all_files.sort();

    let mut out_file = File::create(Path::new(&out_dir).join("source_files_list.rs"))?;
    writeln!(
        out_file,
        "#[allow(unused)]\nconst SOURCE_FILES_COUNT: usize = {};",
        all_files.len()
    )?;
    writeln!(
        out_file,
        "#[allow(unused)]\nconst SOURCE_FILES: [&str; SOURCE_FILES_COUNT] = ["
    )?;
    for path in all_files {
        writeln!(out_file, "    \"{}\",", path.display())?;
    }
    writeln!(out_file, "];")?;
    Ok(())
}
