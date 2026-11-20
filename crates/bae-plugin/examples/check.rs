//! `bae plugin check` until the app's CLI grows the subcommand: validates
//! plugin packages exactly like the host does (manifest, files, WGSL against
//! the generated prelude) and prints compiler-style diagnostics.
//!
//! ```text
//! cargo run -p bae-plugin --example check -- plugins/examples/vignette [more dirs...]
//! ```
//!
//! Exit status: 0 when every package loads (warnings allowed), 1 otherwise.

use std::process::ExitCode;

use bae_plugin::{DirSource, Package};

fn main() -> ExitCode {
    let dirs: Vec<String> = std::env::args().skip(1).collect();
    if dirs.is_empty() {
        eprintln!("usage: check <package-dir>...");
        return ExitCode::from(2);
    }
    let mut ok = true;
    for dir in &dirs {
        match Package::load(&DirSource::new(dir)) {
            Ok(p) => {
                for w in &p.warnings {
                    println!("{dir}/{w}");
                }
                let effects: Vec<&str> = p
                    .effects
                    .iter()
                    .map(|e| e.def.match_name.as_str())
                    .collect();
                println!(
                    "{dir}: ok — {} {} (API {}), effects: {}",
                    p.id(),
                    p.version(),
                    p.manifest.plugin.api,
                    if effects.is_empty() {
                        "none".to_owned()
                    } else {
                        effects.join(", ")
                    }
                );
            }
            Err(diags) => {
                ok = false;
                for d in &diags {
                    println!("{dir}/{d}");
                }
                println!("{dir}: failed ({} problem(s))", diags.len());
            }
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
