//! Writes benchmark scene B1 as a project: `cargo run -p bae-render --example
//! b1_project -- <dir>` → `<dir>/B1.bae` (+ texture assets in `<dir>/assets`).
//! Render it with `bae render <dir>/B1.bae --comp b1 --frames all -o out/b1_####.png`.

use std::path::PathBuf;

use bae_model::Project;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "b1".into()));
    std::fs::create_dir_all(&dir)?;
    let (builder, comp) = bae_render::fixture::b1(&dir.join("assets"))?;
    let path = dir.join("B1.bae");
    Project::from_stage(builder.into_stage()).save_as(&path)?;
    println!("wrote {} (comp {comp})", path.display());
    Ok(())
}
