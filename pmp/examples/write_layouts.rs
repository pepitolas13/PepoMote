//! Escribe `protocol/retro-layouts.json` (la tabla de plantillas de RetroArch
//! para los tests de paridad de Android e iOS):
//!
//!     cargo run --example write_layouts -- ../protocol/retro-layouts.json
fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "../protocol/retro-layouts.json".to_owned());
    std::fs::write(&path, pmp::retro::to_json()).expect("no puedo escribir el JSON");
    println!("escrito {path}");
}
