//! Text of an uploaded requirements file, for the CLI and the HTTP API alike.

use std::path::Path;

use anyq_pipeline::document::PAGE_BREAK;

#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("format {0} is not supported: use .pdf, .md or .txt")]
    Format(String),
    #[error("cannot read the PDF: {0}")]
    Pdf(String),
    #[error("the file is not UTF-8 text")]
    Encoding,
    #[error("no text in the file (a PDF needs a text layer, not a scan)")]
    Empty,
}

/// The document's text; a PDF's pages are joined with a form feed, so anchors
/// can name the page. Blocking: parsing a large PDF takes a while.
pub fn text_of(name: &str, bytes: &[u8]) -> Result<String, DocumentError> {
    let suffix = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let text = match suffix.as_str() {
        "pdf" => {
            // The PDF is untrusted input; a parser panic must become an error, not a crash.
            let pages =
                std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem_by_pages(bytes))
                    .map_err(|_| DocumentError::Pdf("the PDF parser crashed on this file".into()))?
                    .map_err(|e| DocumentError::Pdf(e.to_string()))?;
            pages.join(&PAGE_BREAK.to_string())
        }
        "md" | "markdown" | "txt" => {
            String::from_utf8(bytes.to_vec()).map_err(|_| DocumentError::Encoding)?
        }
        other => {
            return Err(DocumentError::Format(if other.is_empty() {
                "without an extension".into()
            } else {
                format!(".{other}")
            }));
        }
    };
    if text.trim().is_empty() {
        return Err(DocumentError::Empty);
    }
    Ok(text)
}

/// The first `# ` heading, else the first short line, else the file name.
pub fn title_of(text: &str, name: &str) -> String {
    // A typeset PDF has no markup: its title is the first line, when it is short.
    let first_line = || {
        text.lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .filter(|l| l.chars().count() <= 120)
            .map(str::to_owned)
    };
    text.lines()
        .find_map(|l| l.strip_prefix("# ").map(|t| t.trim().to_owned()))
        .or_else(first_line)
        .unwrap_or_else(|| {
            Path::new(name)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn markdown_is_titled_by_its_heading_and_unknown_formats_are_refused() {
        assert_eq!(
            title_of("# Порог расходов\n\nтекст", "spec.md"),
            "Порог расходов"
        );
        assert_eq!(
            title_of("\n\nУведомление о расходах\nтекст", "spec.md"),
            "Уведомление о расходах"
        );
        assert_eq!(title_of(&"длинно ".repeat(30), "spec.md"), "spec");
        assert!(
            matches!(text_of("spec.docx", b"x"), Err(DocumentError::Format(f)) if f == ".docx")
        );
        assert!(matches!(
            text_of("spec.md", b"  \n"),
            Err(DocumentError::Empty)
        ));
        assert!(matches!(
            text_of("spec.txt", &[0xff, 0xfe]),
            Err(DocumentError::Encoding)
        ));
    }

    #[test]
    fn a_pdf_is_read_page_by_page() {
        let text = text_of("spec.PDF", &minimal_pdf(&["First page", "Second page"])).unwrap();
        let pages: Vec<&str> = text.split(PAGE_BREAK).map(str::trim).collect();
        assert_eq!(pages, ["First page", "Second page"]);
        assert!(matches!(
            text_of("broken.pdf", b"%PDF-1.4 nonsense"),
            Err(DocumentError::Pdf(_))
        ));
    }

    /// A text PDF with one line per page, built by hand: no writer dependency.
    fn minimal_pdf(pages: &[&str]) -> Vec<u8> {
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            String::new(),
        ];
        let mut kids = Vec::new();
        for text in pages {
            let stream = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
            let (page, content) = (objects.len() + 1, objects.len() + 2);
            kids.push(format!("{page} 0 R"));
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {content} 0 R \
                 /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> >>"
            ));
            objects.push(format!(
                "<< /Length {} >>\nstream\n{stream}\nendstream",
                stream.len()
            ));
        }
        objects[1] = format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            kids.len()
        );
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (n, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{body}\nendobj\n", n + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
        for o in offsets {
            out.extend(format!("{o:010} 00000 n \n").bytes());
        }
        out.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .bytes(),
        );
        out
    }
}
