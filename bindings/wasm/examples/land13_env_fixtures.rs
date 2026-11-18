//! Synthetic, contributor-created inputs for the C8-land-13-sheet offline environment probe.
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args().nth(1).ok_or("fixture directory required")?;
    std::fs::create_dir_all(&dir)?;
    let mut empty = SheetDocument::create("Empty.xlsx", 0, 0)?;
    empty.execute("sheet.rename", json!({"name":"Data"}))?;
    empty.execute("home.insertSheet", json!({"name":"Notes","before":9}))?;
    std::fs::write(std::path::Path::new(&dir).join("empty.xlsx"), empty.save(None)?)?;
    let mut drawing = SheetDocument::create("Drawings.xlsx", 0, 0)?;
    drawing.execute("drawing.insert", json!({"kind":"textBox","text":"Original note","frame":{"x":10,"y":10,"w":50,"h":30}}))?;
    std::fs::write(std::path::Path::new(&dir).join("drawings.xlsx"), drawing.save(None)?)?;
    Ok(())
}
