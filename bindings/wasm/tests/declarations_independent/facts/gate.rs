//! Ratchets from the complete comparison, never from the sampled census.
use super::model::Fact;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[path = "ceilings.rs"]
mod ceilings;

// These ceilings may only decrease. A row cannot borrow another row's budget.
const LEFT: (usize, usize, usize) = (237, 0, 237);

#[derive(Default)]
pub(super) struct Census {
    records: Vec<Value>,
    differing: BTreeSet<String>,
    less: BTreeSet<String>,
    more: BTreeSet<String>,
    classes: BTreeMap<&'static str, BTreeSet<String>>,
    families: BTreeMap<String, (usize, usize, usize, usize)>,
}

fn class(fact: &Fact) -> &'static str {
    match fact.kind.as_str() {
        "identity" => "identity",
        "identities" => "selector identity",
        "position" => "position",
        "window" | "gap" => "address interval",
        "covers" => "conditional coverage",
        "external" => "external input",
        "order" => "relative order",
        "field" if fact.field == "*" => "whole object content",
        "field" if fact.field.starts_with('~') => "derived content",
        "field" => "stored content",
        other => panic!("unclassified fact {other}"),
    }
}

impl Census {
    pub fn record(&mut self, family: &str, line: usize, command: &str, less: &[Fact], more: &[Fact], record: Value) {
        let key = format!("{family}:{line}:{command}");
        let family_count = self.families.entry(family.into()).or_default();
        family_count.0 += 1;
        if !less.is_empty() || !more.is_empty() {
            self.differing.insert(key.clone());
            family_count.1 += 1;
        }
        if !less.is_empty() {
            self.less.insert(key.clone());
            family_count.2 += 1;
        }
        if !more.is_empty() {
            self.more.insert(key.clone());
            family_count.3 += 1;
        }
        for fact in more {
            self.classes.entry(class(fact)).or_default().insert(key.clone());
        }
        self.records.push(record);
    }

    pub fn finish(self, rows: usize, handlers: usize) {
        let counts = (self.differing.len(), self.less.len(), self.more.len());
        let report = json!({
            "rows":rows,"handlers":handlers,"differing":counts.0,"less":counts.1,"more":counts.2,
            "classes":self.classes,"families":self.families,"paths":self.records,
        });
        if let Some(path) = std::env::var_os("C8_PATH_FACT_REPORT") {
            std::fs::write(path, serde_json::to_vec(&report).unwrap()).unwrap();
        }
        eprintln!("FULL_PATHS {rows}, handlers {handlers}, differing {}, LESS {}, MORE {}", counts.0, counts.1, counts.2);
        for (name, keys) in &self.classes {
            eprintln!("FULL_PATH_MORE_CLASS {name}: {} {keys:?}", keys.len());
        }
        for (name, counts) in &self.families {
            eprintln!("FULL_PATH_FAMILY {name}: rows/differing/less/more {counts:?}");
        }
        let mut limits = BTreeMap::new();
        for (family, rows) in ceilings::ROWS {
            for row in rows.split(',') {
                let (line, count) = row.split_once('=').unwrap();
                assert!(limits.insert((*family, line.parse::<usize>().unwrap()), count.parse::<usize>().unwrap()).is_none());
            }
        }
        for row in &self.records {
            let key = (row["family"].as_str().unwrap(), row["line"].as_u64().unwrap() as usize);
            let limit = limits.get(&key).copied().unwrap_or(0);
            assert!(row["more"].as_array().unwrap().len() <= limit, "path excess grew: {} (ceiling {limit})", row["key"]);
        }
        for (name, paths) in &self.classes {
            let limit = ceilings::CLASSES.iter().find(|(class, _)| class == name).map_or(0, |(_, limit)| *limit);
            assert!(paths.len() <= limit, "class excess grew: {name}");
        }
        assert!(
            counts.0 <= LEFT.0 && counts.1 <= LEFT.1 && counts.2 <= LEFT.2,
            "complete path comparison ceiling exceeded: {counts:?}; LESS {:?}",
            self.less
        );
    }
}
