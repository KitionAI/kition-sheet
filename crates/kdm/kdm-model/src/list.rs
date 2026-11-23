//! Ordered lists: making a replica list hold exactly the objects a view holds with as few
//! operations as possible ([`sync_list`]; ported from Kition Word's `kdm-model`, same owner and
//! licence).

use std::collections::{HashMap, HashSet};

use kition_kit::kdm::{ObjectId, Tx};

use crate::KdmResult;

/// For each item of `after`, the index of the item of `before` it is (`None`: a new one): an equal
/// item first; then, with `key`, the item of the same key (an edited table keeps its object), and
/// without it the items left over pair in order (an edited conditional format keeps its object).
pub(crate) fn pair_items<T: PartialEq, K: Fn(&T) -> u32>(before: &[T], after: &[T], key: Option<K>) -> Vec<Option<usize>> {
    let mut partner: Vec<Option<usize>> = vec![None; after.len()];
    let mut used = vec![false; before.len()];
    for (j, a) in after.iter().enumerate() {
        // The same place first: an unchanged list pairs with no search.
        let at = Some(j).filter(|&i| before.get(i) == Some(a) && !used[i]).or_else(|| (0..before.len()).find(|&i| !used[i] && before[i] == *a));
        if let Some(i) = at {
            partner[j] = Some(i);
            used[i] = true;
        }
    }
    match key {
        Some(key) => {
            for (j, a) in after.iter().enumerate() {
                if partner[j].is_some() {
                    continue;
                }
                if let Some(i) = (0..before.len()).find(|&i| !used[i] && key(&before[i]) == key(a)) {
                    partner[j] = Some(i);
                    used[i] = true;
                }
            }
        }
        None => {
            let mut free = (0..before.len()).filter(|&i| !used[i]).collect::<Vec<_>>().into_iter();
            for slot in partner.iter_mut().filter(|p| p.is_none()) {
                match free.next() {
                    Some(i) => *slot = Some(i),
                    None => break,
                }
            }
        }
    }
    partner
}

/// Makes `owner.field` list exactly `target` (distinct objects): entries not in `target`, and
/// repeated entries, are removed (their objects are not deleted: the caller decides that); the
/// entries already there keep their place as far as possible (the longest run already in
/// `target`'s order stays put) and the rest are moved; new entries are inserted. So moving one
/// element is one `list_move`. Answers how many list operations it wrote.
pub(crate) fn sync_list(tx: &mut Tx<'_>, owner: &ObjectId, field: &str, target: &[ObjectId]) -> KdmResult<usize> {
    let mut current = tx.list_items(owner, field)?;
    if current == target {
        return Ok(0);
    }
    let wanted: HashSet<&ObjectId> = target.iter().collect();
    let mut ops = 0;
    let mut seen = HashSet::new();
    let keep: Vec<bool> = current.iter().map(|o| wanted.contains(o) && seen.insert(o.clone())).collect();
    for index in (0..current.len()).rev() {
        if !keep[index] {
            tx.list_remove(owner, field, index)?;
            current.remove(index);
            ops += 1;
        }
    }
    // Target positions of what is already there, in its current order; the longest increasing
    // run of them stays.
    let position: HashMap<&ObjectId, usize> = target.iter().enumerate().map(|(i, o)| (o, i)).collect();
    let order: Vec<usize> = current.iter().filter_map(|o| position.get(o).copied()).collect();
    let stays: HashSet<usize> = longest_increasing(&order).into_iter().collect();
    for (j, object) in target.iter().enumerate() {
        if stays.contains(&j) {
            continue;
        }
        let after = match j.checked_sub(1).and_then(|p| target.get(p)) {
            Some(prev) => current.iter().position(|o| o == prev).map_or(0, |i| i + 1),
            None => 0,
        };
        match current.iter().position(|o| o == object) {
            Some(from) => {
                let to = if from < after { after - 1 } else { after };
                if from != to {
                    tx.list_move(owner, field, from, to)?;
                    let moved = current.remove(from);
                    current.insert(to, moved);
                    ops += 1;
                }
            }
            None => {
                tx.list_insert(owner, field, after, object)?;
                current.insert(after, object.clone());
                ops += 1;
            }
        }
    }
    Ok(ops)
}

/// The values of one longest strictly increasing subsequence of `values`.
pub(crate) fn longest_increasing(values: &[usize]) -> Vec<usize> {
    // tails[k]: index into `values` of the smallest tail of an increasing run of length k + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; values.len()];
    for (i, &v) in values.iter().enumerate() {
        let k = tails.partition_point(|&t| values[t] < v);
        prev[i] = k.checked_sub(1).map(|p| tails[p]);
        if k == tails.len() {
            tails.push(i);
        } else {
            tails[k] = i;
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(i) = at {
        out.push(values[i]);
        at = prev[i];
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_kit::kdm::{ActorId, DocId, Origin, Replica};
    use serde_json::{Map, json};

    fn with_list(n: usize) -> (Replica, Vec<ObjectId>) {
        let schema = <crate::SheetModel as kition_kit::session::KitModel>::schema();
        let mut r = Replica::create(schema, DocId::new(), ActorId::from_u64(1).unwrap_or_else(|_| panic!("actor"))).unwrap_or_else(|e| panic!("{e}"));
        let root = crate::root_id();
        let objects = r
            .transact(&Origin::system("t"), |tx| {
                tx.create_with_id("Workbook", &root, Map::new())?;
                let mut out = Vec::new();
                for i in 0..n {
                    let o = tx.create("Sheet", [("name".to_owned(), json!(format!("S{i}")))].into_iter().collect())?;
                    tx.list_insert(&root, "sheets", i, &o)?;
                    out.push(o);
                }
                Ok(out)
            })
            .unwrap_or_else(|error| panic!("{error}"));
        (r, objects)
    }

    fn sync(r: &mut Replica, target: &[ObjectId]) -> usize {
        let root = crate::root_id();
        let ops = r.transact(&Origin::system("t"), |tx| sync_list(tx, &root, "sheets", target)).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(r.list(&root, "sheets").unwrap_or_default(), target);
        ops
    }

    #[test]
    fn one_move_is_one_operation() {
        let (mut r, o) = with_list(5);
        assert_eq!(sync(&mut r, &[o[1].clone(), o[2].clone(), o[3].clone(), o[4].clone(), o[0].clone()]), 1);
        assert_eq!(sync(&mut r, &[o[0].clone(), o[1].clone(), o[2].clone(), o[3].clone(), o[4].clone()]), 1);
        // Reversing five: only one element can stay (the longest run in order is 1), so four moves.
        assert_eq!(sync(&mut r, &[o[4].clone(), o[3].clone(), o[2].clone(), o[1].clone(), o[0].clone()]), 4);
        assert_eq!(sync(&mut r, &[o[4].clone(), o[2].clone(), o[0].clone()]), 2);
    }

    #[test]
    fn longest_increasing_run() {
        assert_eq!(longest_increasing(&[3, 0, 1, 2]), vec![0, 1, 2]);
        assert_eq!(longest_increasing(&[]), Vec::<usize>::new());
        assert_eq!(longest_increasing(&[2, 1, 0]).len(), 1);
    }
}
