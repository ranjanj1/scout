use chrono::NaiveDate;

use crate::indexer::schema::{DocRecord, StructuralMeta};
use crate::search::query::{CompareOp, QueryNode, QueryValue};

/// Evaluate a query node as a structural filter against a document.
/// Returns true if the document satisfies all field constraints in the query.
pub fn matches_filters(node: &QueryNode, doc: &DocRecord, meta: &StructuralMeta) -> bool {
    match node {
        QueryNode::Term(_) | QueryNode::Phrase(_) => true, // text nodes don't filter
        QueryNode::Field { name, op, value } => eval_field(name, op, value, doc, meta),
        QueryNode::And(left, right) => {
            matches_filters(left, doc, meta) && matches_filters(right, doc, meta)
        }
        QueryNode::Or(left, right) => {
            matches_filters(left, doc, meta) || matches_filters(right, doc, meta)
        }
        QueryNode::Not(inner) => !matches_filters(inner, doc, meta),
    }
}

/// Count how many field constraints in the query are satisfied (for soft scoring).
pub fn field_match_score(node: &QueryNode, doc: &DocRecord, meta: &StructuralMeta) -> f64 {
    let (matched, total) = count_field_matches(node, doc, meta);
    if total == 0 {
        return 0.0;
    }
    matched as f64 / total as f64
}

fn count_field_matches(
    node: &QueryNode,
    doc: &DocRecord,
    meta: &StructuralMeta,
) -> (usize, usize) {
    match node {
        QueryNode::Term(_) | QueryNode::Phrase(_) => (0, 0),
        QueryNode::Field { name, op, value } => {
            let matched = if eval_field(name, op, value, doc, meta) { 1 } else { 0 };
            (matched, 1)
        }
        QueryNode::And(left, right) | QueryNode::Or(left, right) => {
            let (lm, lt) = count_field_matches(left, doc, meta);
            let (rm, rt) = count_field_matches(right, doc, meta);
            (lm + rm, lt + rt)
        }
        QueryNode::Not(inner) => {
            let (m, t) = count_field_matches(inner, doc, meta);
            (t - m, t) // inverted
        }
    }
}

fn eval_field(
    name: &str,
    op: &CompareOp,
    value: &QueryValue,
    doc: &DocRecord,
    meta: &StructuralMeta,
) -> bool {
    match name {
        "type" | "doctype" => eval_type(op, value, meta),
        "path" => eval_path(op, value, doc),
        "amount" => eval_amount(op, value, meta),
        "date" => eval_date(op, value, meta),
        "email" => eval_email(op, value, meta),
        "since" => eval_since(op, value, doc),
        _ => true, // unknown fields don't filter
    }
}

fn eval_type(op: &CompareOp, value: &QueryValue, meta: &StructuralMeta) -> bool {
    let QueryValue::Text(target) = value else {
        return false;
    };
    let Some(doc_type) = &meta.doc_type else {
        return false;
    };
    match op {
        CompareOp::Eq => doc_type.to_lowercase().contains(&target.to_lowercase()),
        _ => false,
    }
}

fn eval_path(op: &CompareOp, value: &QueryValue, doc: &DocRecord) -> bool {
    let QueryValue::Text(target) = value else {
        return false;
    };
    let path_str = doc.path.to_string_lossy().to_lowercase();
    match op {
        CompareOp::Eq => path_str.contains(&target.to_lowercase()),
        _ => false,
    }
}

fn eval_amount(op: &CompareOp, value: &QueryValue, meta: &StructuralMeta) -> bool {
    let QueryValue::Number(target) = value else {
        return false;
    };
    if meta.amounts.is_empty() {
        return false;
    }
    // Check if any amount in the document satisfies the condition
    meta.amounts.iter().any(|&amount| match op {
        CompareOp::Eq => (amount - target).abs() < 0.01,
        CompareOp::Gt => amount > *target,
        CompareOp::Lt => amount < *target,
        CompareOp::Gte => amount >= *target,
        CompareOp::Lte => amount <= *target,
    })
}

fn eval_date(op: &CompareOp, value: &QueryValue, meta: &StructuralMeta) -> bool {
    let target = match value {
        QueryValue::Text(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d").ok(),
        QueryValue::Number(n) => {
            // Allow bare year: date:>2024
            NaiveDate::from_ymd_opt(*n as i32, 1, 1)
        }
    };
    let Some(target_date) = target else {
        return false;
    };
    if meta.dates.is_empty() {
        return false;
    }
    meta.dates.iter().any(|&doc_date| match op {
        CompareOp::Eq => doc_date == target_date,
        CompareOp::Gt => doc_date > target_date,
        CompareOp::Lt => doc_date < target_date,
        CompareOp::Gte => doc_date >= target_date,
        CompareOp::Lte => doc_date <= target_date,
    })
}

fn eval_email(op: &CompareOp, value: &QueryValue, meta: &StructuralMeta) -> bool {
    let QueryValue::Text(target) = value else {
        return false;
    };
    match op {
        CompareOp::Eq => meta
            .emails
            .iter()
            .any(|e| e.to_lowercase().contains(&target.to_lowercase())),
        _ => false,
    }
}

fn eval_since(op: &CompareOp, value: &QueryValue, doc: &DocRecord) -> bool {
    // `since` operates on mtime; parse duration string like "7d", "2w", "3m"
    let QueryValue::Text(s) = value else {
        return false;
    };
    let Some(cutoff) = parse_duration_cutoff(s) else {
        return false;
    };
    let Ok(mtime_secs) = doc
        .mtime
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
    else {
        return false;
    };
    match op {
        CompareOp::Eq | CompareOp::Gte | CompareOp::Gt => mtime_secs >= cutoff,
        CompareOp::Lt | CompareOp::Lte => mtime_secs < cutoff,
    }
}

/// Parse duration string ("7d", "2w", "3m", "1y") → Unix timestamp cutoff.
pub fn parse_duration_cutoff(s: &str) -> Option<u64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();

    let (num_str, unit) = s.split_at(s.len().saturating_sub(1));
    let n: u64 = num_str.parse().ok()?;

    let seconds = match unit.to_lowercase().as_str() {
        "d" => n * 86_400,
        "w" => n * 7 * 86_400,
        "m" => n * 30 * 86_400,
        "y" => n * 365 * 86_400,
        _ => return None,
    };

    Some(now.saturating_sub(seconds))
}
