//! OddVoices VST3/CLAP plugin entry point.
//!
//! This binary is only used for standalone testing.
//! The actual plugin is exported as a cdylib from lib.rs.

fn main() {
    eprintln!("OddVoices plugin - use as VST3 or CLAP in a DAW");
    eprintln!("");
    eprintln!("To build:");
    eprintln!("  cargo build --release");
    eprintln!("  The .vst3 and .clap files will be in target/release/");
}
