use chrono::NaiveDate;
use regex::Regex;
use std::sync::OnceLock;

use crate::indexer::schema::StructuralMeta;

static RE_DATE: OnceLock<Regex> = OnceLock::new();
static RE_AMOUNT: OnceLock<Regex> = OnceLock::new();
static RE_EMAIL: OnceLock<Regex> = OnceLock::new();
static RE_ENTITY: OnceLock<Regex> = OnceLock::new();

fn re_date() -> &'static Regex {
    RE_DATE.get_or_init(|| {
        Regex::new(
            r"(?x)
            (\d{4}-\d{2}-\d{2})                        # ISO: 2024-01-15
            | (\d{1,2}/\d{1,2}/\d{4})                  # US:  01/15/2024
            | (\d{1,2}\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{4})  # 15 January 2024
            | ((?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2},?\s+\d{4}) # January 15, 2024
            ",
        )
        .unwrap()
    })
}

fn re_amount() -> &'static Regex {
    RE_AMOUNT.get_or_init(|| {
        Regex::new(
            r"(?i)\$\s*(\d{1,3}(?:,\d{3})*(?:\.\d{2})?)\s*(?:(million|M|billion|B|thousand|K))?",
        )
        .unwrap()
    })
}

fn re_email() -> &'static Regex {
    RE_EMAIL.get_or_init(|| {
        Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}").unwrap()
    })
}

fn re_entity() -> &'static Regex {
    // Simple heuristic: two or more consecutive capitalized words
    RE_ENTITY.get_or_init(|| {
        Regex::new(r"[A-Z][a-z]+(?:\s+[A-Z][a-z]+)+").unwrap()
    })
}

/// Extract structural metadata from document text using regex heuristics.
/// No ML or NLP — pure pattern matching.
pub fn extract_metadata(text: &str, path: &std::path::Path) -> StructuralMeta {
    let doc_type = infer_doc_type(text, path);
    let dates = extract_dates(text);
    let amounts = extract_amounts(text);
    let emails = extract_emails(text);
    let entities = extract_entities(text);

    StructuralMeta {
        doc_type,
        dates,
        amounts,
        emails,
        entities,
    }
}

fn infer_doc_type(text: &str, path: &std::path::Path) -> Option<String> {
    let text_lower = text.to_lowercase();
    let filename = path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Check filename first (most reliable signal)
    for (keyword, label) in &[
        ("contract", "contract"),
        ("agreement", "contract"),
        ("invoice", "invoice"),
        ("report", "report"),
        ("memo", "memo"),
        ("proposal", "proposal"),
        ("resume", "resume"),
        ("cv", "resume"),
        ("nda", "nda"),
        ("lease", "lease"),
        ("receipt", "receipt"),
    ] {
        if filename.contains(keyword) {
            return Some(label.to_string());
        }
    }

    // Fall back to text content heuristics
    let checks: &[(&str, &str)] = &[
        ("this agreement", "contract"),
        ("hereby agree", "contract"),
        ("terms and conditions", "contract"),
        ("invoice number", "invoice"),
        ("amount due", "invoice"),
        ("bill to", "invoice"),
        ("executive summary", "report"),
        ("table of contents", "report"),
        ("non-disclosure", "nda"),
        ("confidentiality agreement", "nda"),
        ("lease agreement", "lease"),
        ("rental agreement", "lease"),
    ];

    for (pattern, label) in checks {
        if text_lower.contains(pattern) {
            return Some(label.to_string());
        }
    }

    None
}

fn extract_dates(text: &str) -> Vec<NaiveDate> {
    let mut dates = Vec::new();
    for cap in re_date().find_iter(text) {
        let s = cap.as_str();
        if let Some(date) = parse_date_str(s) {
            if !dates.contains(&date) {
                dates.push(date);
            }
        }
    }
    dates.sort();
    dates.dedup();
    dates
}

fn parse_date_str(s: &str) -> Option<NaiveDate> {
    // Try ISO format first
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }
    // US format
    if let Ok(d) = NaiveDate::parse_from_str(s, "%m/%d/%Y") {
        return Some(d);
    }
    // Long formats — normalize and try
    let normalized = s
        .replace("January", "01").replace("February", "02")
        .replace("March", "03").replace("April", "04")
        .replace("May", "05").replace("June", "06")
        .replace("July", "07").replace("August", "08")
        .replace("September", "09").replace("October", "10")
        .replace("November", "11").replace("December", "12")
        .replace("Jan", "01").replace("Feb", "02").replace("Mar", "03")
        .replace("Apr", "04").replace("Jun", "06").replace("Jul", "07")
        .replace("Aug", "08").replace("Sep", "09").replace("Oct", "10")
        .replace("Nov", "11").replace("Dec", "12")
        .replace(',', "");

    NaiveDate::parse_from_str(normalized.trim(), "%d %m %Y")
        .or_else(|_| NaiveDate::parse_from_str(normalized.trim(), "%m %d %Y"))
        .ok()
}

fn extract_amounts(text: &str) -> Vec<f64> {
    let mut amounts = Vec::new();
    for cap in re_amount().captures_iter(text) {
        let num_str = cap[1].replace(',', "");
        if let Ok(num) = num_str.parse::<f64>() {
            let multiplier = match cap.get(2).map(|m| m.as_str().to_lowercase()).as_deref() {
                Some("million") | Some("m") => 1_000_000.0,
                Some("billion") | Some("b") => 1_000_000_000.0,
                Some("thousand") | Some("k") => 1_000.0,
                _ => 1.0,
            };
            amounts.push(num * multiplier);
        }
    }
    amounts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    amounts.dedup();
    amounts
}

fn extract_emails(text: &str) -> Vec<String> {
    re_email()
        .find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect()
}

fn extract_entities(text: &str) -> Vec<String> {
    let mut entities: Vec<String> = re_entity()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .filter(|e| e.split_whitespace().count() >= 2)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    entities.sort();
    // Limit to top 50 to avoid noise
    entities.truncate(50);
    entities
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_amounts() {
        let text = "The deal is worth $2.5 million. A smaller payment of $50,000 is due.";
        let amounts = extract_amounts(text);
        assert!(amounts.contains(&2_500_000.0));
        assert!(amounts.contains(&50_000.0));
    }

    #[test]
    fn test_extract_dates() {
        let text = "Signed on 2024-03-15. Effective from 01/01/2025.";
        let dates = extract_dates(text);
        assert_eq!(dates.len(), 2);
    }

    #[test]
    fn test_infer_doc_type() {
        let text = "This agreement is entered into by both parties.";
        let path = std::path::Path::new("contract.pdf");
        assert_eq!(
            infer_doc_type(text, path),
            Some("contract".to_string())
        );
    }
}
