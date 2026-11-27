//! Compares the built-in effects with After Effects renders.
//!
//! ```sh
//! cargo run -p bae-effects --example ae_compare -- prepare   # inputs + cases → /private/tmp/bae-07-ae
//! tools/ae/aectl lock -- tools/ae/aectl run crates/bae-effects/tests/ae/render_cases.jsx   # on a machine with AE
//! cargo run -p bae-effects --example ae_compare -- compare   # BAE renders + table
//! cargo run -p bae-effects --example ae_compare -- golden    # copy AE outputs into tests/ae/golden/
//! ```

#[path = "../tests/ae/categories/mod.rs"]
mod categories;
#[path = "ae_compare/cli.rs"]
mod cli;
#[path = "../tests/ae/harness.rs"]
mod harness;

fn main() {
    cli::run();
}
