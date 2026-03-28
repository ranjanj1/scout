use crate::indexer::schema::PostingEntry;

/// Compute the minimum byte-span window that covers all query term positions
/// across the document's posting lists.
///
/// Returns None if any query term is absent from the document.
/// Returns Some(0) if the document contains all terms in a single position window.
pub fn min_span(posting_lists: &[Vec<PostingEntry>], doc_id: u32) -> Option<u32> {
    if posting_lists.is_empty() {
        return None;
    }

    // Extract positions for this doc_id from each posting list
    let mut all_positions: Vec<&Vec<u32>> = Vec::new();
    for list in posting_lists {
        let positions = list
            .iter()
            .find(|e| e.doc_id == doc_id)
            .map(|e| &e.positions);
        match positions {
            Some(pos) if !pos.is_empty() => all_positions.push(pos),
            _ => return None, // Term not in this doc
        }
    }

    if all_positions.len() == 1 {
        return Some(0);
    }

    // Sliding window minimum span: merge sorted position lists,
    // advance the pointer with the smallest value each step.
    minimum_window_span(&all_positions)
}

/// Find the minimum span (max - min position) across a set of sorted position lists,
/// using a sliding window / multi-pointer approach.
fn minimum_window_span(lists: &[&Vec<u32>]) -> Option<u32> {
    let k = lists.len();
    let mut indices = vec![0usize; k];
    let mut min_span = u32::MAX;

    loop {
        let positions: Vec<u32> = (0..k).map(|i| lists[i][indices[i]]).collect();
        let lo = *positions.iter().min().unwrap();
        let hi = *positions.iter().max().unwrap();
        let span = hi - lo;
        if span < min_span {
            min_span = span;
        }

        // Advance the index that holds the minimum position
        let min_idx = (0..k).min_by_key(|&i| positions[i]).unwrap();
        indices[min_idx] += 1;
        if indices[min_idx] >= lists[min_idx].len() {
            break;
        }
    }

    if min_span == u32::MAX {
        None
    } else {
        Some(min_span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::schema::PostingEntry;

    #[test]
    fn test_min_span_single_term() {
        let list = vec![PostingEntry {
            doc_id: 0,
            positions: vec![5, 20, 50],
        }];
        let result = min_span(&[list], 0);
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_min_span_close_terms() {
        let list1 = vec![PostingEntry {
            doc_id: 0,
            positions: vec![10, 100],
        }];
        let list2 = vec![PostingEntry {
            doc_id: 0,
            positions: vec![13, 200],
        }];
        // Minimum span should be |13 - 10| = 3
        let result = min_span(&[list1, list2], 0);
        assert_eq!(result, Some(3));
    }

    #[test]
    fn test_min_span_missing_doc() {
        let list = vec![PostingEntry {
            doc_id: 1, // different doc
            positions: vec![5],
        }];
        let result = min_span(&[list], 0);
        assert_eq!(result, None);
    }
}
