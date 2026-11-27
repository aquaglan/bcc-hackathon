//! The report file: sections and order follow the organisers' task (п. 6) and
//! their report template; the questions to the author are our own section.
//!
//! A pure function of the report and the run's passport. Whether development
//! can start is decided here by code from the findings, not by the model.

use std::collections::HashMap;

use anyq_core::domain::{DropReason, Dropped, Finding, Question, Report, Requirement, Severity};

use crate::agent::{Criteria, Scope};

/// The organisers name these three kinds of defect as critical in the summary.
const KINDS: [(&str, &[&str]); 3] = [
    ("Непроверяемые", &["REQ-VERIFIABLE"]),
    ("Противоречивые", &["SET-CONSISTENT"]),
    ("Неполные", &["REQ-COMPLETE", "SET-COMPLETE"]),
];
/// How many major findings the "fix first" list shows after the critical ones.
const FIRST_MAJOR: usize = 5;

pub struct Passport {
    pub document: String,
    pub file: String,
    pub date: String,
    /// How long the run took, as shown to the reader.
    pub duration: String,
    pub agent_version: String,
    pub harness: String,
    pub pipeline_version: String,
}

fn severity(s: Severity) -> &'static str {
    match s {
        Severity::Critical => "🔴 Критично",
        Severity::Major => "🟡 Существенно",
        Severity::Minor => "🟢 Незначительно",
    }
}

/// Model text is shown as text: no table breaks, no raw HTML.
fn text(value: Option<&str>) -> String {
    value
        .unwrap_or("—")
        .replace('\n', " ")
        .replace('|', "\\|")
        .replace('<', "&lt;")
}

fn t(value: &str) -> String {
    text(Some(value))
}

pub fn readiness(findings: &[Finding]) -> String {
    let count = |s: Severity| findings.iter().filter(|f| f.severity == s).count();
    let (critical, major) = (count(Severity::Critical), count(Severity::Major));
    if critical > 0 {
        format!(
            "⛔ **Нельзя.** Разработку и тестирование начинать нельзя, пока не устранены \
             блокеры: критичных дефектов — {critical}."
        )
    } else if major > 0 {
        format!(
            "⚠️ **Условно.** Блокеров нет, существенных дефектов — {major}: до начала работы \
             по ним нужны ответы автора."
        )
    } else {
        "✅ **Можно.** Критичных и существенных дефектов не найдено.".to_owned()
    }
}

struct Writer<'a> {
    out: Vec<String>,
    criteria: &'a Criteria,
}

impl Writer<'_> {
    fn line(&mut self, line: impl Into<String>) {
        self.out.push(line.into());
    }

    fn criterion(&self, id: &str) -> String {
        self.criteria.name(id).unwrap_or(id).to_owned()
    }

    /// Every criterion of the scope with its mark: the check went through all
    /// of them, a criterion without a finding passed.
    fn marks(&mut self, scope: Scope, findings: &[&Finding]) {
        let marks: Vec<String> = self
            .criteria
            .of(scope)
            .map(|c| {
                let failed = findings.iter().any(|f| f.category == c.id);
                format!("{} {}", if failed { "⚠️" } else { "✅" }, c.name)
            })
            .collect();
        self.line(format!("Критерии: {}.", marks.join(" · ")));
        self.line("");
    }

    fn finding(&mut self, f: &Finding) {
        let unverified = if f.verified {
            ""
        } else {
            " · не проверено критиком"
        };
        let head = format!(
            "- **{}** · {} · {}{unverified}",
            severity(f.severity),
            self.criterion(&f.category),
            text(f.section.as_deref())
        );
        self.line(head);
        self.line(format!("  - Проблема: {}", t(&f.problem)));
        if let Some(quote) = &f.quote {
            self.line(format!("  - Цитата: «{}»", t(quote)));
        }
        self.line(format!("  - Исправить: {}", t(&f.suggestion)));
        if let Some(note) = &f.critic_note {
            self.line(format!("  - Критик подтвердил: {}", t(note)));
        }
    }

    fn findings(&mut self, findings: &[&Finding]) {
        if findings.is_empty() {
            self.line("Дефектов не найдено.");
        }
        for f in findings {
            self.finding(f);
        }
        self.line("");
    }

    /// `n` counts from one over the whole list; the place names the requirement
    /// the question came from, when it came from one.
    fn question(&mut self, n: usize, q: &Question, requirements: &[Requirement]) {
        let requirement = requirements
            .iter()
            .find(|r| Some(&r.section) == q.section.as_ref())
            .map(|r| format!(" · {}", title(r)))
            .unwrap_or_default();
        let place = q
            .section
            .as_deref()
            .map(|s| format!(" · {}", t(s)))
            .unwrap_or_default();
        self.line(format!(
            "- **В-{n}**{requirement}{place}: {} — _зачем:_ {}",
            t(&q.text),
            t(&q.reason)
        ));
    }
}

/// `R-08 (Ф-1)`, or `R-08` when the document gave the requirement no number.
fn title(r: &Requirement) -> String {
    match &r.source_id {
        Some(source) => format!("{} ({})", r.id, t(source)),
        None => r.id.clone(),
    }
}

/// `main_questions`: how many first questions, most important first, are the main ones.
pub fn render(
    report: &Report,
    passport: &Passport,
    criteria: &Criteria,
    main_questions: usize,
) -> String {
    let mut by_requirement: HashMap<&str, Vec<&Finding>> = HashMap::new();
    for f in &report.findings {
        if let Some(id) = &f.requirement_id {
            by_requirement.entry(id.as_str()).or_default().push(f);
        }
    }
    let set_findings: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| f.requirement_id.is_none())
        .collect();
    let count = |s: Severity| report.findings.iter().filter(|f| f.severity == s).count();
    let (critical, major, minor) = (
        count(Severity::Critical),
        count(Severity::Major),
        count(Severity::Minor),
    );
    let mut w = Writer {
        out: Vec::new(),
        criteria,
    };

    w.line(format!(
        "# Отчёт о проверке требований: {}",
        t(&passport.document)
    ));
    w.line("");
    w.line(format!(
        "> **Итог:** {} Требований — {}, дефектов — {} (критичных {critical}, существенных \
         {major}, незначительных {minor}), вопросов автору — {} (раздел 5).",
        readiness(&report.findings),
        report.requirements.len(),
        report.findings.len(),
        report.open_questions.len()
    ));
    w.line("");
    w.line("## 1. Паспорт прогона");
    w.line("");
    w.line("| Поле | Значение |");
    w.line("|---|---|");
    w.line(format!(
        "| Документ | {} (`{}`) |",
        t(&passport.document),
        t(&passport.file)
    ));
    w.line(format!("| Дата прогона | {} |", passport.date));
    w.line(format!("| Время анализа | {} |", passport.duration));
    w.line(format!("| Модель | {} |", t(&report.models.join(", "))));
    w.line(format!(
        "| Версия агента | `{}` |",
        t(&passport.agent_version)
    ));
    w.line(format!(
        "| Конвейер | `{}`, промпты {} |",
        passport.harness, passport.pipeline_version
    ));
    w.line(format!("| Критерии | версия {} |", criteria.version));
    w.line("");

    w.line("## 2. Как агент нарезал требования");
    w.line("");
    w.line(
        "Идентификаторы `R-..` присвоил агент; номер из документа указан, если он там есть. \
         Место найдено кодом по цитате.",
    );
    w.line("");
    w.line("| В отчёте | В документе | Формулировка | Цитата | Место |");
    w.line("|---|---|---|---|---|");
    for r in &report.requirements {
        w.line(format!(
            "| {} | {} | {} | «{}» | {} |",
            r.id,
            text(r.source_id.as_deref()),
            t(&r.summary),
            t(&r.quote),
            t(&r.section)
        ));
    }
    w.line("");

    w.line("## 3. Проверка каждого требования");
    w.line("");
    w.line(
        "У каждого требования — отметка по каждому критерию: ✅ — нарушений нет, ⚠️ — есть \
         дефект, он описан ниже.",
    );
    w.line("");
    for r in &report.requirements {
        w.line(format!("### {} — {}", title(r), t(&r.summary)));
        w.line("");
        w.line(format!("Цитата: «{}» ({}).", t(&r.quote), t(&r.section)));
        w.line("");
        let found = by_requirement
            .get(r.id.as_str())
            .cloned()
            .unwrap_or_default();
        w.marks(Scope::Requirement, &found);
        w.findings(&found);
    }

    w.line("## 4. Проверка всего набора требований");
    w.line("");
    // A set defect merged into a requirement's finding is still a defect of the set.
    let merged_into_requirement: Vec<&Dropped> = report
        .dropped
        .iter()
        .filter(|d| d.reason == DropReason::Duplicate && d.finding.requirement_id.is_none())
        .collect();
    let failed: Vec<&Finding> = set_findings
        .iter()
        .copied()
        .chain(merged_into_requirement.iter().map(|d| &d.finding))
        .collect();
    w.marks(Scope::Set, &failed);
    w.findings(&set_findings);
    let shown: Vec<&Dropped> = merged_into_requirement
        .iter()
        .copied()
        .filter(|d| d.kept_in.is_some())
        .collect();
    if !shown.is_empty() {
        w.line(
            "Найдено также при проверке отдельных требований — описано в разделе 3 и в счёт \
             дефектов дважды не входит:",
        );
        w.line("");
        for d in shown {
            let line = format!(
                "- {} · см. {}: {}",
                w.criterion(&d.finding.category),
                t(d.kept_in.as_deref().unwrap_or("—")),
                t(&d.finding.problem)
            );
            w.line(line);
        }
        w.line("");
    }

    w.line("## 5. Вопросы автору");
    w.line("");
    if report.open_questions.is_empty() {
        w.line("Вопросов нет.");
        w.line("");
    }
    let split = main_questions.min(report.open_questions.len());
    let has_rest = split < report.open_questions.len();
    if has_rest {
        w.line("Вопросы упорядочены по важности, повторы слиты. Без ответов на главные нельзя начинать.");
        w.line("");
        w.line("### Главные вопросы");
        w.line("");
    }
    for (n, q) in report.open_questions.iter().enumerate() {
        if has_rest && n == split {
            w.line("");
            w.line("### Остальные вопросы");
            w.line("");
        }
        w.question(n + 1, q, &report.requirements);
    }
    w.line("");

    w.line("## 6. Сводка");
    w.line("");
    w.line("| Показатель | Значение |");
    w.line("|---|---|");
    let with_defects = by_requirement.len();
    w.line(format!(
        "| Всего требований | {} |",
        report.requirements.len()
    ));
    w.line(format!(
        "| Требований без дефектов | {} |",
        report.requirements.len().saturating_sub(with_defects)
    ));
    w.line(format!("| Требований с дефектами | {with_defects} |"));
    w.line(format!("| Всего дефектов | {} |", report.findings.len()));
    w.line(format!("| — 🔴 критичных | {critical} |"));
    w.line(format!("| — 🟡 существенных | {major} |"));
    w.line(format!("| — 🟢 незначительных | {minor} |"));
    w.line(format!(
        "| — из них к набору в целом | {} |",
        set_findings.len()
    ));
    for (kind, ids) in KINDS {
        let n = report
            .findings
            .iter()
            .filter(|f| ids.contains(&f.category.as_str()))
            .count();
        w.line(format!("| {kind} формулировки | {n} |"));
    }
    w.line(format!(
        "| Вопросов автору | {} |",
        report.open_questions.len()
    ));
    w.line("");
    w.line("Критичные дефекты (блокеры разработки):");
    w.line("");
    let blockers: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Critical)
        .collect();
    if blockers.is_empty() {
        w.line("- нет");
    }
    for (n, f) in blockers.iter().enumerate() {
        let line = format!(
            "{}. **{}** · {} · {}: {}",
            n + 1,
            f.requirement_id.as_deref().unwrap_or("Набор"),
            w.criterion(&f.category),
            text(f.section.as_deref()),
            t(&f.problem)
        );
        w.line(line);
    }
    w.line("");

    let first: Vec<&Finding> = blockers
        .iter()
        .copied()
        .chain(
            report
                .findings
                .iter()
                .filter(|f| f.severity == Severity::Major)
                .take(FIRST_MAJOR),
        )
        .collect();
    w.line("## 7. Итог");
    w.line("");
    w.line(format!(
        "Можно ли начинать разработку и тестирование: {}",
        readiness(&report.findings)
    ));
    w.line("");
    w.line(format!("Оценка агента: {}", t(&report.summary)));
    w.line("");
    w.line("Что исправить в первую очередь:");
    w.line("");
    if first.is_empty() {
        w.line("Срочных исправлений нет.");
    }
    for (n, f) in first.iter().enumerate() {
        w.line(format!(
            "{}. {}: {}",
            n + 1,
            f.requirement_id.as_deref().unwrap_or("Набор"),
            t(&f.suggestion)
        ));
    }
    w.line("");

    w.line("## Приложение. Как проверялось");
    w.line("");
    w.line(
        "Требования нарезала модель; каждое требование и каждое замечание код нашёл в тексте \
         по дословной цитате, ненайденные отброшены, повторы слиты. Критичные и существенные \
         замечания второй проход модели пытался опровергнуть: опровергнутые убраны, \
         сомнительные стали вопросами автору, остальные без отметки «не проверено критиком». \
         Отметку «✅» у критерия ставит код: проверка прошла по всем критериям и не нашла по \
         нему дефекта. Итог «можно ли начинать» считает код по числу критичных и существенных \
         дефектов. Пустые разделы, заглушки и пустые таблицы находит код без модели.",
    );
    w.line("");
    w.line(
        "Критерии — наши формулировки по названиям свойств из ISO/IEC/IEEE 29148:2018 \
         (п. 5.2.5, 5.2.6), IEEE Std 830-1998 (п. 4.3) и чек-листа шаблона организаторов; \
         с текстом стандартов они не сверены.",
    );
    w.line("");
    w.line("### Что не вошло в отчёт");
    w.line("");
    let refuted: Vec<&Dropped> = report
        .dropped
        .iter()
        .filter(|d| d.reason == DropReason::Refuted)
        .collect();
    let merged = report.dropped.len() - refuted.len();
    if report.dropped.is_empty() {
        w.line("Ничего не отброшено.");
    } else {
        w.line(format!("Опровергнуто критиком: {}.", refuted.len()));
        w.line("");
        for d in refuted {
            let line = format!(
                "- {} · {}: {} — критик: {}",
                w.criterion(&d.finding.category),
                text(d.finding.section.as_deref()),
                t(&d.finding.problem),
                text(d.note.as_deref())
            );
            w.line(line);
        }
        w.line("");
        w.line(format!(
            "Слито как повтор: {merged}. Полный список — в выгрузке `--json`, поле \
             `report.dropped`."
        ));
    }
    w.line("");
    w.line("### Критерии");
    w.line("");
    w.line("| Критерий | Идентификатор | Источник |");
    w.line("|---|---|---|");
    for c in criteria.active() {
        w.line(format!("| {} | `{}` | {} |", c.name, c.id, c.source));
    }
    w.line("");
    w.out.join("\n")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use anyq_core::domain::Question;

    use super::*;
    use crate::agent::criteria;

    fn passport() -> Passport {
        Passport {
            document: "Порог расходов".into(),
            file: "spec.md".into(),
            date: "2026-11-26 12:00 +0500".into(),
            duration: "3 мин 05 с".into(),
            agent_version: "abc1234".into(),
            harness: "pipeline".into(),
            pipeline_version: "v".into(),
        }
    }

    fn finding(
        severity: Severity,
        requirement: Option<&str>,
        category: &str,
        verified: bool,
    ) -> Finding {
        Finding {
            requirement_id: requirement.map(str::to_owned),
            severity,
            category: category.into(),
            section: Some("ФТ, строка 3".into()),
            quote: Some("быстро".into()),
            problem: "Нельзя | проверить <b>".into(),
            suggestion: "Указать секунды".into(),
            verified,
            critic_note: None,
        }
    }

    fn report(findings: Vec<Finding>) -> Report {
        let requirement =
            |id: &str, source: Option<&str>, quote: &str, line: u8, summary: &str| Requirement {
                id: id.into(),
                source_id: source.map(str::to_owned),
                section: format!("ФТ, строка {line}"),
                quote: quote.into(),
                summary: summary.into(),
            };
        Report {
            summary: "Не готов.".into(),
            requirements: vec![
                requirement("R-01", Some("Ф-1"), "быстро", 3, "Скорость"),
                requirement("R-02", None, "лимит", 4, "Лимит"),
            ],
            findings,
            open_questions: vec![Question {
                id: "q1".into(),
                text: "Сколько секунд?".into(),
                reason: "порог".into(),
                section: None,
            }],
            models: vec!["model-a".into()],
            dropped: Vec::new(),
        }
    }

    fn rendered(findings: Vec<Finding>) -> String {
        render(&report(findings), &passport(), &criteria().unwrap(), 12)
    }

    #[test]
    fn report_has_every_section_the_task_requires_in_order() {
        let text = rendered(vec![finding(
            Severity::Critical,
            Some("R-01"),
            "REQ-VERIFIABLE",
            true,
        )]);
        let positions: Vec<usize> = [
            "> **Итог:** ⛔",
            "## 1. Паспорт прогона",
            "## 2. Как агент нарезал требования",
            "## 3. Проверка каждого требования",
            "## 4. Проверка всего набора требований",
            "## 5. Вопросы автору",
            "## 6. Сводка",
            "## 7. Итог",
        ]
        .iter()
        .map(|s| text.find(s).unwrap())
        .collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]));
        assert!(text.contains("| R-02 | — | Лимит | «лимит» | ФТ, строка 4 |"));
        assert!(text.contains("model-a") && text.contains("abc1234"));
        assert!(text.contains("| Время анализа | 3 мин 05 с |"));
    }

    #[test]
    fn model_text_cannot_break_the_table_or_inject_html() {
        let text = rendered(vec![finding(
            Severity::Major,
            Some("R-01"),
            "REQ-VERIFIABLE",
            true,
        )]);
        assert!(text.contains("Нельзя \\| проверить &lt;b>"));
        assert!(!text.contains("<b>"));
    }

    #[test]
    fn unverified_findings_say_so() {
        let text = rendered(vec![finding(
            Severity::Major,
            Some("R-01"),
            "REQ-VERIFIABLE",
            false,
        )]);
        assert!(text.contains("не проверено критиком"));
    }

    #[test]
    fn readiness_is_decided_by_severity() {
        let f = |s| finding(s, None, "X", true);
        assert!(readiness(&[f(Severity::Critical), f(Severity::Minor)]).contains("**Нельзя.**"));
        assert!(readiness(&[f(Severity::Major)]).contains("**Условно.**"));
        assert!(readiness(&[f(Severity::Minor)]).contains("**Можно.**"));
    }

    #[test]
    fn set_findings_and_counts_land_in_their_sections() {
        let text = rendered(vec![finding(
            Severity::Critical,
            None,
            "SET-CONSISTENT",
            true,
        )]);
        let start = text.find("## 4. Проверка всего набора").unwrap();
        let end = text.find("## 5. Вопросы автору").unwrap();
        let set = &text[start..end];
        assert!(set.contains("⚠️ Непротиворечивость · ✅ Выполнимость набора"));
        assert!(set.contains("**🔴 Критично** · Непротиворечивость · ФТ, строка 3"));
        assert!(text.contains("| Противоречивые формулировки | 1 |"));
        assert!(text.contains("| Требований без дефектов | 2 |"));
        assert!(text.contains("1. **Набор** · Непротиворечивость"));
    }

    fn dropped(f: Finding, reason: DropReason, kept_in: Option<&str>) -> Dropped {
        Dropped {
            finding: f,
            reason,
            kept_in: kept_in.map(str::to_owned),
            note: Some("довод критика".into()),
        }
    }

    #[test]
    fn a_set_defect_merged_into_a_requirement_still_fails_its_criterion() {
        let mut r = report(vec![finding(
            Severity::Major,
            Some("R-01"),
            "SET-CONSISTENT",
            true,
        )]);
        r.dropped = vec![dropped(
            finding(Severity::Major, None, "SET-CONSISTENT", true),
            DropReason::Duplicate,
            Some("R-01"),
        )];
        let text = render(&r, &passport(), &criteria().unwrap(), 12);
        let start = text.find("## 4. Проверка всего набора").unwrap();
        let set = &text[start..text.find("## 5. Вопросы автору").unwrap()];
        assert!(set.contains("⚠️ Непротиворечивость"), "{set}");
        assert!(set.contains("Найдено также при проверке отдельных требований"));
        assert!(set.contains("- Непротиворечивость · см. R-01: Нельзя \\| проверить &lt;b>"));
        // Dropped findings are not defects.
        assert!(text.contains("| Всего дефектов | 1 |"));
        assert!(text.contains("| — из них к набору в целом | 0 |"));
        assert!(text.contains("дефектов — 1 (критичных 0, существенных 1"));
    }

    #[test]
    fn refuted_findings_are_listed_in_the_appendix() {
        let mut r = report(vec![]);
        r.dropped = vec![
            dropped(
                finding(Severity::Major, Some("R-01"), "REQ-VERIFIABLE", false),
                DropReason::Refuted,
                None,
            ),
            dropped(
                finding(Severity::Major, Some("R-01"), "REQ-VERIFIABLE", false),
                DropReason::Duplicate,
                Some("R-02"),
            ),
        ];
        let text = render(&r, &passport(), &criteria().unwrap(), 12);
        let appendix = &text[text.find("### Что не вошло в отчёт").unwrap()..];
        assert!(appendix.contains("Опровергнуто критиком: 1."));
        assert!(appendix.contains("— критик: довод критика"));
        assert!(appendix.contains("Слито как повтор: 1."));
        assert!(!rendered(vec![]).contains("Опровергнуто"));
        assert!(rendered(vec![]).contains("Ничего не отброшено."));
    }

    #[test]
    fn each_requirement_shows_a_mark_for_every_criterion() {
        let text = rendered(vec![finding(
            Severity::Major,
            Some("R-01"),
            "REQ-VERIFIABLE",
            true,
        )]);
        let first = text.find("### R-01 (Ф-1) — Скорость").unwrap();
        let second = text.find("### R-02 — Лимит").unwrap();
        let end = text.find("## 4.").unwrap();
        assert!(text[first..second].contains("Цитата: «быстро» (ФТ, строка 3)."));
        assert!(
            text[first..second].contains("✅ Полнота")
                && text[first..second].contains("⚠️ Проверяемость")
        );
        assert!(
            !text[second..end].contains("⚠️") && text[second..end].contains("Дефектов не найдено.")
        );
    }

    #[test]
    fn questions_beyond_the_main_ones_are_set_apart() {
        let mut report = report(vec![]);
        let question = |n: usize| Question {
            id: format!("r1q{n}"),
            text: format!("Вопрос {n}?"),
            reason: "р".into(),
            section: None,
        };
        report.open_questions = (1..=3).map(question).collect();
        let text = render(&report, &passport(), &criteria().unwrap(), 2);
        let main = text.find("### Главные вопросы").unwrap();
        let rest = text.find("### Остальные вопросы").unwrap();
        let third = text.find("**В-3**: Вопрос 3?").unwrap();
        assert!(main < text.find("Вопрос 2?").unwrap() && rest < third);
        let short = render(&report, &passport(), &criteria().unwrap(), 5);
        assert!(!short.contains("### Главные вопросы"));
        report.open_questions[0].section = Some("ФТ, строка 3".into());
        let placed = render(&report, &passport(), &criteria().unwrap(), 5);
        assert!(placed.contains("**В-1** · R-01 (Ф-1) · ФТ, строка 3: Вопрос 1?"));
    }
}
