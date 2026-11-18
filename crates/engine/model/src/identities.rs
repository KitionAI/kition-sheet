//! File identities and provenance, separate from workbook content and replica history.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkbookIdentities {
    pub names: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SheetIdentities {
    pub object: String,
    pub axes: [Vec<String>; 2],
    #[serde(default)]
    pub tombstones: [BTreeMap<usize, String>; 2],
    pub cells: BTreeMap<String, CellIdentity>,
    pub drawings: BTreeMap<u32, String>,
    #[serde(default)]
    pub tables: Vec<u32>,
    #[serde(default)]
    pub pivots: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CellIdentity {
    pub object: String,
    pub by: Option<String>,
    pub key: Option<String>,
}
