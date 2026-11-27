//! Stage 0: the specification as text the code can point into.
//!
//! The model only says *what* it quotes; where the quote is — section, page,
//! line — is found here, so an anchor in the report is never the model's word.
//! A quote that is not in the text is not found, and what is built on it is
//! dropped.

/// Page breaks: the PDF reader joins pages with a form feed.
pub const PAGE_BREAK: char = '\u{c}';

fn normalize_char(c: char, out: &mut String) {
    match c {
        // Formatting the model drops or changes when it copies a phrase.
        '*' | '_' | '`' | '#' | '|' | '>' => out.push(' '),
        '«' | '»' | '“' | '”' | '„' | '\'' => out.push('"'),
        '—' | '–' => out.push('-'),
        'ё' | 'Ё' => out.push('е'),
        _ => out.extend(c.to_lowercase()),
    }
}

pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        normalize_char(c, &mut out);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub page: Option<usize>,
    pub line: usize,
    pub section: String,
}

impl Location {
    pub fn anchor(&self) -> String {
        let mut parts = Vec::new();
        if !self.section.is_empty() {
            parts.push(self.section.clone());
        }
        if let Some(page) = self.page {
            parts.push(format!("стр. {page}"));
        }
        parts.push(format!("строка {}", self.line));
        parts.join(", ")
    }
}

struct Line {
    page: usize,
    line_on_page: usize,
    section: String,
    normalized: String,
}

pub struct Document {
    pub text: String,
    paged: bool,
    lines: Vec<Line>,
    /// The whole text normalized, lines joined by a space.
    normalized: String,
    /// For each byte of `normalized`, the index of its source line.
    owner: Vec<usize>,
}

/// `## Title` -> (2, "Title").
pub(crate) fn heading(line: &str) -> Option<(usize, String)> {
    let level = line.chars().take_while(|&c| c == '#').count();
    let rest = &line[level..];
    if !(1..=6).contains(&level) || !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let title = rest.trim().trim_matches(|c| c == '*' || c == ' ');
    (!title.is_empty()).then(|| (level, title.to_owned()))
}

impl Document {
    pub fn new(text: &str) -> Self {
        // A page break is a line of its own: the last line of a page must not
        // merge with the first line of the next one.
        let split = text.replace(PAGE_BREAK, &format!("\n{PAGE_BREAK}\n"));
        let mut lines = Vec::new();
        let mut normalized = String::new();
        let mut owner = Vec::new();
        let (mut page, mut page_break_line) = (1, None::<usize>);
        let mut path: Vec<(usize, String)> = Vec::new();
        for (index, line) in split.split('\n').enumerate() {
            if line.contains(PAGE_BREAK) {
                page += 1;
                page_break_line = Some(index);
            }
            if let Some((level, title)) = heading(line) {
                path.retain(|(l, _)| *l < level);
                path.push((level, title));
            }
            // The level-1 heading is the document title: it would prefix every anchor.
            let below_title: Vec<&str> = path
                .iter()
                .filter(|(l, _)| *l > 1)
                .map(|(_, t)| t.as_str())
                .collect();
            let titles = if below_title.is_empty() {
                path.iter().map(|(_, t)| t.as_str()).collect()
            } else {
                below_title
            };
            let line_norm = normalize(line);
            if !line_norm.is_empty() {
                normalized.push_str(&line_norm);
                normalized.push(' ');
                owner.extend(std::iter::repeat_n(index, line_norm.len() + 1));
            }
            lines.push(Line {
                page,
                line_on_page: page_break_line.map_or(index + 1, |b| index - b),
                section: titles.join(" › "),
                normalized: line_norm,
            });
        }
        Self {
            text: text.to_owned(),
            paged: text.contains(PAGE_BREAK),
            lines,
            normalized,
            owner,
        }
    }

    /// Where the quote is. A short quote can occur several times: then the
    /// occurrence whose section shares most words with `near` wins, else the first.
    pub fn locate(&self, quote: &str, near: Option<&str>) -> Option<Location> {
        let needle = normalize(quote);
        if needle.is_empty() {
            return None;
        }
        let hint: Vec<String> = normalize(near.unwrap_or(""))
            .split(' ')
            .map(str::to_owned)
            .collect();
        let overlap = |line: usize| {
            let section = normalize(&self.lines[line].section);
            section
                .split(' ')
                .filter(|w| !w.is_empty() && hint.iter().any(|h| h == w))
                .count()
        };
        let mut best: Option<(usize, usize)> = None; // (line, overlap)
        for (start, _) in self.normalized.match_indices(&needle) {
            let line = self.owner[start];
            let score = overlap(line);
            if best.is_none_or(|(_, s)| score > s) {
                best = Some((line, score));
            }
        }
        let (index, _) = best?;
        Some(self.at_line(index, near))
    }

    /// The place of the line with this zero-based index in the split text.
    pub fn at_line(&self, index: usize, near: Option<&str>) -> Location {
        let line = &self.lines[index];
        let mut section = line.section.clone();
        if section.is_empty()
            && let Some(near) = near
        {
            section = self.heading_above(index, near);
        }
        if self.paged {
            Location {
                page: Some(line.page),
                line: line.line_on_page,
                section,
            }
        } else {
            Location {
                page: None,
                line: index + 1,
                section,
            }
        }
    }

    /// Text without markup headings (a typeset PDF): the section the model named
    /// is taken only if a line with exactly that text stands above the quote.
    fn heading_above(&self, index: usize, near: &str) -> String {
        for part in near.rsplit(['›', '>', '/']) {
            let wanted = normalize(part);
            if !wanted.is_empty() && self.lines[..=index].iter().any(|l| l.normalized == wanted) {
                return part.trim().to_owned();
            }
        }
        String::new()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    const SPEC: &str = include_str!("../../../eval/cases/card-alerts/spec.md");

    #[test]
    fn quote_is_found_despite_markup_quotes_and_dashes() {
        let document = Document::new(SPEC);
        let location = document
            .locate(
                "**Ф-3.** Когда расходы переходят от суммы ниже порога",
                None,
            )
            .unwrap();
        assert_eq!(
            location.section,
            "Часть A. Новый функционал › Требования к изменениям › Функциональные требования"
        );
        assert_eq!(location.line, 39);
        assert!(document.locate("200. Successful - Created", None).is_some());
        assert!(document.locate("поле \"Порог, ₸\"", None).is_some());
    }

    #[test]
    fn quote_spanning_lines_is_found_and_invented_one_is_not() {
        let document = Document::new("# Т\n\nпервая строка\nвторая строка");
        assert!(document.locate("вторая строка первая", None).is_none());
        assert!(document.locate("первая строка вторая", None).is_some());
        assert!(document.locate("  ", None).is_none());
        assert!(
            Document::new(SPEC)
                .locate("Система должна быстро обрабатывать переводы", None)
                .is_none()
        );
    }

    #[test]
    fn pages_are_counted_from_form_feeds_and_lines_within_the_page() {
        let text = format!("стр один\nещё{PAGE_BREAK}стр два\nискомое");
        let location = Document::new(&text).locate("искомое", None).unwrap();
        assert_eq!((location.page, location.line), (Some(2), 2));
        assert_eq!(location.anchor(), "стр. 2, строка 2");
    }

    #[test]
    fn without_markup_headings_a_named_section_is_kept_only_if_it_is_in_the_text() {
        let text =
            format!("Часть A{PAGE_BREAK}Функциональные требования\nФ-1. Порог задаёт клиент.");
        let document = Document::new(&text);
        let found = document
            .locate(
                "Порог задаёт клиент",
                Some("Часть A › Функциональные требования"),
            )
            .unwrap();
        assert_eq!(found.section, "Функциональные требования");
        let invented = document
            .locate("Порог задаёт клиент", Some("Требования безопасности"))
            .unwrap();
        assert_eq!(invented.section, "");
    }

    #[test]
    fn repeated_quote_resolves_to_the_section_the_model_named() {
        let document = Document::new(SPEC);
        let first = document.locate("При ошибке", None).unwrap();
        assert!(first.section.contains("Дизайн процесса UI"));
        let in_api = document
            .locate("При ошибке", Some("Часть B. REST API › Ответ"))
            .unwrap();
        assert_eq!(in_api.line, 158);
    }

    #[test]
    fn normalize_is_idempotent() {
        let once = normalize("«Ёлка» — **да**");
        assert_eq!(normalize(&once), once);
        assert_eq!(once, "\"елка\" - да");
    }
}
