#![allow(dead_code)]
use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions, Guard};
use kition_kits::{KitDefinition, KitManager};
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};
use std::path::PathBuf;

pub struct Fixture {
    pub(super) kits: KitManager,
    dir: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        let values: Vec<_> = (1..=12).map(|n| json!([format!("row{n}"), n, n * 2, n * 3])).collect();
        doc.execute("range.setValues", json!({"range":"A1","values":values})).unwrap();
        doc.execute("sheet.insert", json!({"name":"Sheet2","before":9})).unwrap();
        doc.execute("sheet.activate", json!({"sheet":"Sheet1"})).unwrap();
        doc.execute("select.set", json!({"range":"C5"})).unwrap();
        std::fs::write(dir.path().join("Book.xlsx"), doc.save(None).unwrap()).unwrap();
        let kit = KitDefinition::from_manifest_text(include_str!("../../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(dir.path().to_path_buf()), vec![(kit, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-sheet"))))]);
        Self { kits, dir }
    }

    pub fn bytes(&self) -> Vec<u8> {
        std::fs::read(self.dir.path().join("Book.xlsx")).unwrap()
    }

    pub fn colleague(&self, steps: &[(&str, Value)]) {
        let mut other = SheetDocument::open("Book.xlsx", &self.bytes()).unwrap();
        for (command, params) in steps {
            other.execute(command, params.clone()).unwrap_or_else(|e| panic!("colleague {command}: {e}"));
        }
        self.write_colleague_bytes(&other.save(None).unwrap());
    }

    pub fn colleague_rewrite(&self, edit: impl FnOnce(&mut kition_sheet_engine::model::Workbook)) {
        let (mut workbook, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &self.bytes()).unwrap();
        edit(&mut workbook);
        self.write_colleague_bytes(&kition_sheet_engine::io::save_bytes(&workbook, "Book.xlsx").unwrap());
    }

    fn write_colleague_bytes(&self, bytes: &[u8]) {
        std::fs::write(self.dir.path().join("Book.xlsx"), bytes).unwrap();
        let file = std::fs::File::options().write(true).open(self.dir.path().join("Book.xlsx")).unwrap();
        #[allow(clippy::disallowed_methods)] // a native test: the colleague's write is an hour old
        let an_hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        file.set_times(std::fs::FileTimes::new().set_modified(an_hour_ago)).unwrap();
    }

    pub async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!("Book.xlsx");
        match tools::call_as(
            &self.kits,
            &format!("sheet_{action}"),
            &input,
            CallOptions { guard: Guard::Premise, ..CallOptions::default() },
            "c8-reader",
        )
        .await
        .unwrap()
        {
            Ok(output) => Ok(output.output),
            Err(error) => Err(error.structured()),
        }
    }

    pub async fn basis(&self) -> Value {
        self.call("inspect", json!({})).await.unwrap()["revision"].clone()
    }

    pub async fn run(&self, basis: Value, command: &str, params: Value) -> Result<Value, Value> {
        self.call("run", json!({"basis":basis,"steps":[{"command":command,"params":params}]})).await
    }

    pub async fn shutdown(self) {
        self.kits.shutdown().await;
    }
}
