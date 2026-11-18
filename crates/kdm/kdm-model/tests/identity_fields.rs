//! Persistence metadata has its own exhaustive inventory alongside the content-field scanner.
//! Every object reference is restored by `Writer::prepare` / `create_restored`; positions are
//! live axis order, tombstone list indexes and A1 cell keys. Provenance is copied verbatim.
//! Table/pivot native ids are restored by the XLSX identity reader; feature_ids verifies them.

#[test]
fn every_persistence_identity_field_is_handled() {
    let source = include_str!("../../../engine/model/src/identities.rs");
    let fields: Vec<_> = source
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("pub "))
        .filter(|line| line.contains(": "))
        .map(|line| line.trim_end_matches(','))
        .collect();
    assert_eq!(
        fields,
        [
            "names: Vec<String>",
            "object: String",
            "axes: [Vec<String>; 2]",
            "tombstones: [BTreeMap<usize, String>; 2]",
            "cells: BTreeMap<String, CellIdentity>",
            "drawings: BTreeMap<u32, String>",
            "tables: Vec<u32>",
            "pivots: Vec<u32>",
            "object: String",
            "by: Option<String>",
            "key: Option<String>",
        ],
        "classify new persistence fields and verify their save/open bridge before extending this inventory"
    );
}
