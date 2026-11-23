//! Comments as `Comment` objects: a sheet's `comments` map names, under each cell's key
//! (`<rowObject>:<colObject>`), the cell's `Comment`. A comment's tool id is its cell's address
//! (`comment:<sheetId>!B7`, its `address`, kept by the writer as rows and columns move): others'
//! changes of it are told under that id, so a write holding one cell's comment is not touched by
//! a comment made or changed on another cell. A comment has its `author`, its `text` (a
//! KDM `text`: two people editing it at once merge by character), its `replies` (a list of `Reply`
//! objects, each with its `author` and `text`: replies added at once are all kept), and
//! `threaded`, `resolved` and `visible` registers. Deleting a comment deletes its replies.

use std::collections::HashSet;

use kition_kit::kdm::{ObjectId, Replica, Tx};
use kition_kit::session::write_text;
use kition_sheet_model::Comment;
use serde_json::{Map, Value, json};

use crate::KdmResult;

pub(crate) const COMMENT: &str = "Comment";
pub(crate) const REPLY: &str = "Reply";
const TEXT: &str = "text";

/// The `Comment` object a map value names, if it is one and exists.
pub(crate) fn object_of(tx: &Tx<'_>, value: &Value) -> Option<ObjectId> {
    let id = ObjectId::from_string(value.as_str()?);
    (tx.exists(&id) && tx.object_type(&id).ok().as_deref() == Some(COMMENT)).then_some(id)
}

/// Makes `id`'s text field `field` read `new`, given that the view read `old` there (`None`: not
/// known): only the changed characters are written when the field holds `old`, so concurrent
/// typing elsewhere in it stays; otherwise the whole text is replaced. Answers whether it wrote.
fn put_text(tx: &mut Tx<'_>, id: &ObjectId, old: Option<&str>, new: &str) -> KdmResult<bool> {
    let len = tx.text_len(id, TEXT)?;
    match old.filter(|old| old.chars().count() == len) {
        Some(old) if old == new => Ok(false),
        Some(old) => {
            write_text(tx, id, TEXT, old, new).map_err(crate::kdm_err)?;
            Ok(true)
        }
        None => {
            if len > 0 {
                tx.text_delete(id, TEXT, 0, len)?;
            }
            if !new.is_empty() {
                tx.text_insert(id, TEXT, 0, new)?;
            }
            Ok(len > 0 || !new.is_empty())
        }
    }
}

fn set(tx: &mut Tx<'_>, id: &ObjectId, field: &str, v: Value) -> KdmResult<bool> {
    if tx.get(id, field)? == v {
        return Ok(false);
    }
    tx.set(id, field, v)?;
    Ok(true)
}

fn registers(c: &Comment) -> Map<String, Value> {
    [
        ("author".to_owned(), json!(c.author)),
        ("threaded".to_owned(), json!(c.threaded)),
        ("resolved".to_owned(), json!(c.resolved)),
        ("visible".to_owned(), json!(c.visible)),
    ]
    .into_iter()
    .collect()
}

/// Writes `after` as the comment `key` of `sheet`, whose view held `before` (`None`: none, or not
/// known): its `Comment` keeps its object, its registers set where they differ, its text and its
/// replies' texts written by character, replies added and removed. Answers whether it wrote.
pub(crate) fn write(tx: &mut Tx<'_>, sheet: &ObjectId, key: &str, address: &str, before: Option<&Comment>, after: &Comment) -> KdmResult<bool> {
    let current = tx.map_get(sheet, "comments", key)?;
    let Some(id) = object_of(tx, &current) else {
        let mut fields = registers(after);
        fields.insert("address".to_owned(), json!(address));
        let id = tx.create(COMMENT, fields)?;
        tx.text_insert(&id, TEXT, 0, &after.text)?;
        write_replies(tx, &id, &[], &after.replies)?;
        tx.map_set(sheet, "comments", key, json!(id.as_str()))?;
        return Ok(true);
    };
    let mut wrote = set(tx, &id, "address", json!(address))?;
    for (field, v) in registers(after) {
        wrote |= set(tx, &id, &field, v)?;
    }
    wrote |= put_text(tx, &id, before.map(|b| b.text.as_str()), &after.text)?;
    wrote |= write_replies(tx, &id, before.map_or(&[], |b| b.replies.as_slice()), &after.replies)?;
    Ok(wrote)
}

/// Makes `comment`'s replies `after`, given that the view held `before`: a reply equal to one
/// before keeps that one's object, the others pair in order (an edited reply), the rest are made
/// or deleted.
fn write_replies(tx: &mut Tx<'_>, comment: &ObjectId, before: &[(String, String)], after: &[(String, String)]) -> KdmResult<bool> {
    let mut seen = HashSet::new();
    let old: Vec<ObjectId> = tx
        .list_items(comment, "replies")?
        .into_iter()
        .filter(|o| tx.exists(o) && seen.insert(o.clone()) && tx.object_type(o).ok().as_deref() == Some(REPLY))
        .collect();
    let before = if before.len() == old.len() { before } else { &[] };
    let partner = crate::list::pair_items(before, after, None, true);
    let mut wrote = false;
    let mut out = Vec::with_capacity(after.len());
    for (k, (author, text)) in after.iter().enumerate() {
        match partner.get(k).copied().flatten().and_then(|i| Some((i, old.get(i)?.clone()))) {
            Some((i, id)) => {
                wrote |= set(tx, &id, "author", json!(author))?;
                wrote |= put_text(tx, &id, before.get(i).map(|(_, t)| t.as_str()), text)?;
                out.push(id);
            }
            None => {
                let id = tx.create(REPLY, [("author".to_owned(), json!(author))].into_iter().collect())?;
                tx.text_insert(&id, TEXT, 0, text)?;
                wrote = true;
                out.push(id);
            }
        }
    }
    let keep: HashSet<&ObjectId> = out.iter().collect();
    for id in &old {
        if !keep.contains(id) {
            tx.delete(id)?;
            wrote = true;
        }
    }
    wrote |= crate::list::sync_list(tx, comment, "replies", &out)? > 0;
    Ok(wrote)
}

/// Deletes a `Comment` object and its replies.
pub(crate) fn delete(tx: &mut Tx<'_>, id: &ObjectId) -> KdmResult<()> {
    for reply in tx.list_items(id, "replies")? {
        if tx.exists(&reply) {
            tx.delete(&reply)?;
        }
    }
    if tx.exists(id) {
        tx.delete(id)?;
    }
    Ok(())
}

/// Removes the comment `key` of `sheet` with its object. Answers whether there was one.
pub(crate) fn remove(tx: &mut Tx<'_>, sheet: &ObjectId, key: &str) -> KdmResult<bool> {
    let v = tx.map_get(sheet, "comments", key)?;
    if v.is_null() {
        return Ok(false);
    }
    if let Some(id) = object_of(tx, &v) {
        delete(tx, &id)?;
    }
    tx.map_delete(sheet, "comments", key)?;
    Ok(true)
}

/// A comment from the `Comment` object a map value names; `None` when it names none.
pub(crate) fn read(replica: &Replica, value: &Value) -> Option<Comment> {
    let id = ObjectId::from_string(value.as_str()?);
    if replica.object_type(&id).ok().as_deref() != Some(COMMENT) {
        return None;
    }
    let string = |id: &ObjectId, field: &str| replica.get(id, field).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
    let flag = |field: &str| replica.get(&id, field).ok().and_then(|v| v.as_bool()).unwrap_or(false);
    let mut seen = HashSet::new();
    let replies = replica
        .list(&id, "replies")
        .unwrap_or_default()
        .into_iter()
        .filter(|r| seen.insert(r.clone()) && replica.object_type(r).ok().as_deref() == Some(REPLY))
        .map(|r| (string(&r, "author"), replica.text(&r, TEXT).unwrap_or_default()))
        .collect();
    Some(Comment {
        author: string(&id, "author"),
        text: replica.text(&id, TEXT).unwrap_or_default(),
        replies,
        threaded: flag("threaded"),
        resolved: flag("resolved"),
        visible: flag("visible"),
    })
}
