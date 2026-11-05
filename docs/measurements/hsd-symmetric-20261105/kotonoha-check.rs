//! Compare the downstream rule pipeline through its real dictionary loader.
use kotonoha::Engine;
use std::{
    io::{self, Write},
    path::Path,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let mut engine = Engine::with_default_rules();
    engine.load_dictionary(Path::new(&args[1]))?;
    let input = std::fs::read_to_string(&args[2])?;
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in input.lines() {
        let mut nodes = engine.text_to_analyze(line)?;
        let before_accent = format!("{nodes:?}");
        let phrases = engine.estimate_accent(&mut nodes);
        let labels = engine.text_to_labels(line)?;
        let tones = engine.text_to_phone_tones(line)?;
        let punct = engine.text_to_phone_tones_with_punct(line)?;
        let symbols = engine.text_to_prosody_symbols(line)?;
        assert_eq!(labels, engine.make_label(&nodes, &phrases));
        let record = format!(
            "{:?}",
            (
                &before_accent,
                &nodes,
                &phrases,
                &labels,
                &tones,
                &punct,
                &symbols
            )
        );
        output.write_all(&(record.len() as u64).to_le_bytes())?;
        output.write_all(record.as_bytes())?;
    }
    output.flush()?;
    Ok(())
}
