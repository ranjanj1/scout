/// DSL query parser — hand-written recursive descent.
///
/// Grammar (informal):
///   query      := clause (WS+ clause)*
///   clause     := NOT clause | group | field_expr | phrase | term
///   field_expr := IDENT ':' (comparator value | value)
///   comparator := '>=' | '<=' | '>' | '<'
///   phrase     := '"' text '"'
///   group      := '(' query ')'
///   term       := word
use crate::error::{Result, SearchError};

#[derive(Debug, Clone, PartialEq)]
pub enum QueryNode {
    /// A bare word (trigram search)
    Term(String),
    /// A quoted phrase (trigram search on the full phrase)
    Phrase(String),
    /// A field filter: `type:contract`, `amount:>1M`, `date:>2024-01-01`
    Field {
        name: String,
        op: CompareOp,
        value: QueryValue,
    },
    And(Box<QueryNode>, Box<QueryNode>),
    Or(Box<QueryNode>, Box<QueryNode>),
    Not(Box<QueryNode>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompareOp {
    Eq,
    Gt,
    Lt,
    Gte,
    Lte,
}

#[derive(Debug, Clone, PartialEq)]
pub enum QueryValue {
    Text(String),
    Number(f64),
}

/// Parse a DSL query string into a QueryNode tree.
pub fn parse_query(input: &str) -> Result<QueryNode> {
    let mut parser = Parser::new(input);
    let node = parser.parse_query()?;
    parser.skip_whitespace();
    if !parser.is_eof() {
        return Err(SearchError::QuerySyntax(format!(
            "unexpected token at position {}: {:?}",
            parser.pos,
            &input[parser.pos..]
        )));
    }
    Ok(node)
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser { input, pos: 0 }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        while self.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
            self.advance();
        }
    }

    fn parse_query(&mut self) -> Result<QueryNode> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<QueryNode> {
        let mut left = self.parse_and()?;
        loop {
            self.skip_whitespace();
            if self.input[self.pos..].starts_with("OR ") {
                self.pos += 3;
                let right = self.parse_and()?;
                left = QueryNode::Or(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<QueryNode> {
        let mut left = self.parse_clause()?;
        loop {
            self.skip_whitespace();
            if self.input[self.pos..].starts_with("AND ") {
                self.pos += 4;
                let right = self.parse_clause()?;
                left = QueryNode::And(Box::new(left), Box::new(right));
            } else if !self.is_eof()
                && self.peek() != Some(')')
                && !self.input[self.pos..].starts_with("OR ")
            {
                // Implicit AND between adjacent clauses
                match self.parse_clause() {
                    Ok(right) => left = QueryNode::And(Box::new(left), Box::new(right)),
                    Err(_) => break,
                }
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_clause(&mut self) -> Result<QueryNode> {
        self.skip_whitespace();

        if self.input[self.pos..].starts_with("NOT ") {
            self.pos += 4;
            let inner = self.parse_clause()?;
            return Ok(QueryNode::Not(Box::new(inner)));
        }

        if self.peek() == Some('(') {
            self.advance();
            let inner = self.parse_query()?;
            self.skip_whitespace();
            if self.peek() != Some(')') {
                return Err(SearchError::QuerySyntax("expected closing ')'".into()));
            }
            self.advance();
            return Ok(inner);
        }

        if self.peek() == Some('"') {
            return self.parse_phrase();
        }

        self.parse_term_or_field()
    }

    fn parse_phrase(&mut self) -> Result<QueryNode> {
        self.advance(); // consume opening "
        let start = self.pos;
        while !self.is_eof() && self.peek() != Some('"') {
            self.advance();
        }
        let phrase = self.input[start..self.pos].to_string();
        if self.peek() == Some('"') {
            self.advance();
        }
        Ok(QueryNode::Phrase(phrase))
    }

    fn parse_term_or_field(&mut self) -> Result<QueryNode> {
        let word = self.read_word()?;

        // Check if this is a field expression: word followed by ':'
        if self.peek() == Some(':') {
            self.advance(); // consume ':'
            let (op, value) = self.parse_field_value()?;
            return Ok(QueryNode::Field {
                name: word.to_lowercase(),
                op,
                value,
            });
        }

        Ok(QueryNode::Term(word.to_lowercase()))
    }

    fn parse_field_value(&mut self) -> Result<(CompareOp, QueryValue)> {
        // Detect comparator operator
        let op = if self.input[self.pos..].starts_with(">=") {
            self.pos += 2;
            CompareOp::Gte
        } else if self.input[self.pos..].starts_with("<=") {
            self.pos += 2;
            CompareOp::Lte
        } else if self.peek() == Some('>') {
            self.advance();
            CompareOp::Gt
        } else if self.peek() == Some('<') {
            self.advance();
            CompareOp::Lt
        } else {
            CompareOp::Eq
        };

        // Parse the value
        let raw = self.read_word()?;
        let value = parse_query_value(&raw);

        Ok((op, value))
    }

    fn read_word(&mut self) -> Result<String> {
        self.skip_whitespace();
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_whitespace() || c == ')' || c == '"' || c == ':' {
                break;
            }
            self.advance();
        }
        if self.pos == start {
            return Err(SearchError::QuerySyntax(format!(
                "expected a word at position {}",
                self.pos
            )));
        }
        Ok(self.input[start..self.pos].to_string())
    }
}

/// Parse a raw value string into a typed QueryValue.
/// Handles shorthand: "1M" → 1_000_000.0, "500K" → 500_000.0
fn parse_query_value(s: &str) -> QueryValue {
    let lower = s.to_lowercase();

    // Try numeric with suffix
    let (num_part, multiplier) = if lower.ends_with('m') {
        (&s[..s.len() - 1], 1_000_000.0)
    } else if lower.ends_with('k') {
        (&s[..s.len() - 1], 1_000.0)
    } else if lower.ends_with('b') {
        (&s[..s.len() - 1], 1_000_000_000.0)
    } else {
        (s, 1.0)
    };

    let cleaned = num_part.replace(',', "");
    if let Ok(num) = cleaned.parse::<f64>() {
        return QueryValue::Number(num * multiplier);
    }

    QueryValue::Text(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_term() {
        assert_eq!(parse_query("hello").unwrap(), QueryNode::Term("hello".into()));
    }

    #[test]
    fn test_parse_phrase() {
        assert_eq!(
            parse_query("\"purchase agreement\"").unwrap(),
            QueryNode::Phrase("purchase agreement".into())
        );
    }

    #[test]
    fn test_parse_field_eq() {
        assert_eq!(
            parse_query("type:contract").unwrap(),
            QueryNode::Field {
                name: "type".into(),
                op: CompareOp::Eq,
                value: QueryValue::Text("contract".into()),
            }
        );
    }

    #[test]
    fn test_parse_field_numeric() {
        assert_eq!(
            parse_query("amount:>1M").unwrap(),
            QueryNode::Field {
                name: "amount".into(),
                op: CompareOp::Gt,
                value: QueryValue::Number(1_000_000.0),
            }
        );
    }

    #[test]
    fn test_parse_and() {
        let q = parse_query("type:contract amount:>1M").unwrap();
        assert!(matches!(q, QueryNode::And(_, _)));
    }

    #[test]
    fn test_parse_not() {
        let q = parse_query("NOT type:invoice").unwrap();
        assert!(matches!(q, QueryNode::Not(_)));
    }

    #[test]
    fn test_parse_explicit_or() {
        let q = parse_query("type:invoice OR type:receipt").unwrap();
        assert!(matches!(q, QueryNode::Or(_, _)));
    }
}
