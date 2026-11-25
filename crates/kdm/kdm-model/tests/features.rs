//! A sheet's features and comments are objects of their own (schema freeze, 2026-11-22): two
//! changes of different ones, or of different parts of one, made one after the other, both reach a
//! peer, and the replicas end identical. One test per container field: the `Sheet`'s `merges`,
//! `condFormats`, `validations`, `tables`, `sparklines`, `pivots` and `comments`, a `Table`'s
//! `columns`, a `Comment`'s `replies`.

mod common;

use common::{Session, SharedBlobs, assert_replica_clean, assert_same, assert_workbook_valid, at, edit, first, peer, range, rich, sync};
use kition_sheet_model::{CfRule, Comment, CondFormat, PivotTable, Sheet, Sparkline, Table, TableColumn, Validation, ValidationKind, Workbook};

fn pair() -> (SharedBlobs, Session, Session) {
    let blobs = SharedBlobs::default();
    let a = first(&rich(), &blobs);
    let b = peer(&a, 2, &blobs);
    (blobs, a, b)
}

/// `a` changes the first sheet one way, then another; then `b` takes both changes.
fn both(f: impl FnOnce(&mut Sheet), g: impl FnOnce(&mut Sheet)) -> (Session, Session) {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| {
        f(data(wb));
        vec![]
    });
    edit(&mut a, &blobs, |wb| {
        g(data(wb));
        vec![]
    });
    sync(&mut a, &mut b);
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "the replicas are identical");
    assert_same("views", &a.model().workbook, &b.model().workbook);
    assert_workbook_valid(&a.model().workbook);
    assert_replica_clean(a.replica());
    let (read, _) = kdm_model::read_document(a.replica(), &blobs).unwrap_or_else(|e| panic!("read: {e}"));
    assert_same("a fresh read", &a.model().workbook, &read);
    (a, b)
}

fn data(wb: &mut Workbook) -> &mut Sheet {
    wb.sheet_mut(0).unwrap_or_else(|| panic!("Data"))
}

fn sheet(s: &Session) -> &Sheet {
    &s.model().workbook.sheets[0]
}

#[test]
fn merges_made_at_once_are_both_kept() {
    let (a, _) = both(|s| s.merges.push(range("E20:F21")), |s| s.merges.push(range("H30:I31")));
    let merges = &sheet(&a).merges;
    assert!(merges.contains(&range("A12:C13")) && merges.contains(&range("E20:F21")) && merges.contains(&range("H30:I31")), "{merges:?}");
}

#[test]
fn conditional_formats_added_and_edited_at_once_are_all_kept() {
    let added = CondFormat {
        ranges: vec![range("F2:F9")],
        rule: CfRule::Expression { formula: "F2>1".into(), style: Box::default() },
        priority: 4,
        stop_if_true: false,
    };
    let (a, _) = both(
        |s| s.cond_formats.push(added.clone()),
        |s| {
            s.cond_formats[0].ranges = vec![range("B2:B9")];
            s.cond_formats[1].stop_if_true = false;
        },
    );
    let cfs = &sheet(&a).cond_formats;
    assert_eq!(cfs.len(), 4, "{cfs:?}");
    assert!(cfs.contains(&added));
    assert_eq!(cfs[0].ranges, vec![range("B2:B9")]);
    assert!(!cfs[1].stop_if_true);
}

#[test]
fn one_conditional_format_edited_in_two_fields_at_once_keeps_both() {
    let (a, _) = both(|s| s.cond_formats[0].priority = 9, |s| s.cond_formats[0].ranges = vec![range("B2:B7")]);
    let cf = &sheet(&a).cond_formats[0];
    assert_eq!((cf.priority, cf.ranges.clone()), (9, vec![range("B2:B7")]));
}

#[test]
fn validations_added_at_once_are_both_kept() {
    let v = |a1: &str, f1: &str| Validation { ranges: vec![range(a1)], kind: ValidationKind::Whole, f1: f1.into(), ..Default::default() };
    let (a, _) = both(|s| s.validations.push(v("F2:F5", "1")), |s| s.validations.push(v("G2:G5", "2")));
    let all = &sheet(&a).validations;
    assert_eq!(all.len(), 3);
    assert!(all.contains(&v("F2:F5", "1")) && all.contains(&v("G2:G5", "2")));
}

fn table(id: u32, name: &str, a1: &str) -> Table {
    Table {
        id,
        name: name.into(),
        range: range(a1),
        header_row: true,
        totals_row: false,
        columns: vec![TableColumn { name: "One".into(), totals: Default::default(), totals_label: None, formula: None }],
        style: "TableStyleLight1".into(),
        banded_rows: true,
        banded_cols: false,
        first_col: false,
        last_col: false,
        filter_button: true,
        criteria: vec![],
    }
}

#[test]
fn a_table_added_while_another_is_renamed_keeps_both() {
    let (a, _) = both(|s| s.tables.push(table(9, "Added", "K20:K24")), |s| s.tables[0].name = "Renamed".into());
    let names: Vec<&str> = sheet(&a).tables.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"Added") && names.contains(&"Renamed"), "{names:?}");
}

#[test]
fn two_columns_of_one_table_edited_at_once_keep_both_edits() {
    let (a, _) = both(|s| s.tables[0].columns[0].name = "Fruit name".into(), |s| s.tables[0].columns[1].formula = Some("[@Name]&\"?\"".into()));
    let columns = &sheet(&a).tables[0].columns;
    assert_eq!(columns[0].name, "Fruit name");
    assert_eq!(columns[1].formula.as_deref(), Some("[@Name]&\"?\""));
}

#[test]
fn sparklines_added_at_once_are_both_kept() {
    let line =
        |a1: &str| Sparkline { cell: at(a1), source: "Data!B2:B5".into(), kind: Default::default(), color: Default::default(), markers: false };
    let (a, _) = both(|s| s.sparklines.push(line("G3")), |s| s.sparklines.push(line("G4")));
    let all = &sheet(&a).sparklines;
    assert_eq!(all.len(), 3);
    assert!(all.contains(&line("G3")) && all.contains(&line("G4")));
}

#[test]
fn a_pivot_added_while_another_is_renamed_keeps_both() {
    let added = PivotTable { id: 2, name: "Pivot2".into(), source: "Data!$A$1:$B$5".into(), anchor: at("P2"), ..Default::default() };
    let (a, _) = both(|s| s.pivots.push(added.clone()), |s| s.pivots[0].name = "Renamed".into());
    let names: Vec<&str> = sheet(&a).pivots.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Renamed", "Pivot2"]);
}

fn note(author: &str, text: &str) -> Comment {
    Comment { author: author.into(), text: text.into(), replies: vec![], threaded: false, resolved: false, visible: false }
}

#[test]
fn comments_added_at_once_on_two_cells_are_both_kept() {
    let (a, _) = both(|s| _ = s.comments.insert(at("C20"), note("Ann", "first")), |s| _ = s.comments.insert(at("D20"), note("Bo", "second")));
    let comments = &sheet(&a).comments;
    assert_eq!(comments.get(&at("C20")), Some(&note("Ann", "first")));
    assert_eq!(comments.get(&at("D20")), Some(&note("Bo", "second")));
}

#[test]
fn replies_added_at_once_and_a_resolve_are_all_kept() {
    let (a, _) = both(
        |s| {
            if let Some(c) = s.comments.get_mut(&at("A2")) {
                c.replies.push(("Cy".into(), "agreed".into()));
                c.resolved = true;
            }
        },
        |s| {
            if let Some(c) = s.comments.get_mut(&at("A2")) {
                c.replies.push(("Di".into(), "not yet".into()));
            }
        },
    );
    let c = sheet(&a).comments.get(&at("A2")).cloned().unwrap_or_else(|| panic!("A2 has its comment"));
    assert!(c.resolved);
    assert_eq!(c.replies.len(), 3, "{:?}", c.replies);
    assert_eq!(c.replies[0], ("Bo".to_owned(), "yes".to_owned()));
    assert!(c.replies.contains(&("Cy".into(), "agreed".into())) && c.replies.contains(&("Di".into(), "not yet".into())));
}

#[test]
fn a_reply_edited_while_another_is_added_keeps_both() {
    let (a, _) = both(
        |s| {
            if let Some(c) = s.comments.get_mut(&at("A2")) {
                c.replies[0].1 = "yes indeed".into();
            }
        },
        |s| {
            if let Some(c) = s.comments.get_mut(&at("A2")) {
                c.replies.push(("Di".into(), "fine".into()));
            }
        },
    );
    let replies = sheet(&a).comments.get(&at("A2")).map(|c| c.replies.clone()).unwrap_or_default();
    assert_eq!(replies, [("Bo".to_owned(), "yes indeed".to_owned()), ("Di".to_owned(), "fine".to_owned())]);
}
