//! Stage without a model: placeholders, empty sections and empty tables.
//!
//! Finding what is not there needs no model, and the model skips it: it reads
//! the requirements that exist. The rules here are pure functions of the text.

use crate::agent::Lint;
use crate::document::{Document, PAGE_BREAK, heading};

pub const SUGGESTION: &str =
    "Заполнить раздел до начала разработки либо убрать его и указать, где лежит содержание.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Placeholder,
    EmptySection,
    EmptyTable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Zero-based index of the line in the text split as `Document` splits it.
    pub line: usize,
    pub kind: Kind,
    /// The placeholder line, the heading or the table header row.
    pub quote: String,
}

impl Hit {
    pub fn problem(&self) -> &'static str {
        match self.kind {
            Kind::Placeholder => "Раздел не заполнен: вместо содержания стоит заглушка.",
            Kind::EmptySection => "Раздел пуст.",
            Kind::EmptyTable => "Таблица не заполнена: в ней нет ни одной строки с данными.",
        }
    }
}

fn is_table_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

fn cells(line: &str) -> Vec<&str> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect()
}

fn is_separator(line: &str) -> bool {
    is_table_row(line)
        && cells(line)
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':')))
}

/// A horizontal rule carries no content of its own.
fn is_rule(line: &str) -> bool {
    line.len() >= 3 && line.chars().all(|c| c == '-' || c == '*' || c == '_')
}

pub fn find(document: &Document, config: &Lint) -> Vec<Hit> {
    let split = document
        .text
        .replace(PAGE_BREAK, &format!("\n{PAGE_BREAK}\n"));
    let lines: Vec<&str> = split.split('\n').collect();
    let markers: Vec<String> = config
        .placeholder_markers
        .iter()
        .map(|m| m.to_lowercase())
        .collect();
    let mut hits = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if let Some((level, _)) = heading(line) {
            let next = lines[i + 1..]
                .iter()
                .map(|l| l.trim())
                .find(|l| !l.is_empty() && !is_rule(l));
            if next.is_none_or(|n| heading(n).is_some_and(|(l, _)| l <= level)) {
                hits.push(Hit {
                    line: i,
                    kind: Kind::EmptySection,
                    quote: line.to_owned(),
                });
            }
        } else if is_table_row(line) {
            let start = i;
            let mut end = i;
            while end + 1 < lines.len() && is_table_row(lines[end + 1]) {
                end += 1;
            }
            if end > start && is_separator(lines[start + 1]) {
                let filled = lines[start + 2..=end]
                    .iter()
                    .any(|row| cells(row).iter().any(|c| !c.is_empty()));
                if !filled {
                    hits.push(Hit {
                        line: start,
                        kind: Kind::EmptyTable,
                        quote: line.to_owned(),
                    });
                }
            }
            i = end;
        } else if !line.is_empty()
            && line.chars().count() <= config.placeholder_max_chars
            && markers.iter().any(|m| line.to_lowercase().contains(m))
        {
            hits.push(Hit {
                line: i,
                kind: Kind::Placeholder,
                quote: line.to_owned(),
            });
        }
        i += 1;
    }
    hits
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    const SPEC: &str = include_str!("../../../eval/cases/card-alerts/spec.md");

    fn config() -> Lint {
        crate::agent::agent().unwrap().lint
    }

    fn found(text: &str) -> Vec<(Kind, usize)> {
        find(&Document::new(text), &config())
            .into_iter()
            .map(|h| (h.kind, h.line + 1))
            .collect()
    }

    #[test]
    fn the_real_case_has_the_placeholder_and_the_empty_table() {
        assert_eq!(
            found(SPEC),
            [(Kind::Placeholder, 57), (Kind::EmptyTable, 89)]
        );
    }

    #[test]
    fn a_short_line_with_a_marker_is_a_placeholder() {
        assert_eq!(found("# T\n\n## A\nТБД: tbd\n"), [(Kind::Placeholder, 4)]);
    }

    #[test]
    fn a_long_sentence_with_a_marker_is_not_a_placeholder() {
        let text = "## A\nПоле не заполнено, поэтому сервис возвращает ошибку клиенту сразу же.\n";
        assert!(found(text).is_empty());
    }

    #[test]
    fn a_heading_before_a_heading_of_the_same_level_or_the_end_is_empty() {
        assert_eq!(
            found("## A\n\n## B\ntext\n## C\n"),
            [(Kind::EmptySection, 1), (Kind::EmptySection, 5)]
        );
    }

    #[test]
    fn a_heading_before_a_deeper_heading_is_not_empty() {
        assert!(found("## A\n\n### B\ntext\n").is_empty());
    }

    #[test]
    fn a_table_without_data_is_empty_and_with_one_filled_row_is_not() {
        assert_eq!(
            found("## A\n| a | b |\n|---|---|\n| | |\n"),
            [(Kind::EmptyTable, 2)]
        );
        assert!(found("## A\n| a | b |\n|---|---|\n| | |\n| 1 | |\n").is_empty());
    }
}
