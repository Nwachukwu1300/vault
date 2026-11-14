//! Plain-text extraction for documents.

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Read;
use std::path::Path;

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default()
}

/// Whole-document text for txt, md and docx.
pub fn document_text(path: &Path) -> Result<String, String> {
    match extension(path).as_str() {
        "txt" | "md" => {
            let bytes = std::fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
        "docx" => docx_text(path),
        // shortcut: PDFs fail until per-page extraction lands in Phase 5.
        "pdf" => Err("PDF text extraction is not implemented yet".into()),
        other => Err(format!("Unsupported document type: {other}")),
    }
}

/// DOCX is a zip; the body text lives in `<w:t>` runs of `word/document.xml`.
fn docx_text(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Failed to open DOCX: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Not a valid DOCX: {e}"))?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|e| format!("DOCX has no document.xml: {e}"))?
        .read_to_string(&mut xml)
        .map_err(|e| format!("Failed to read document.xml: {e}"))?;
    docx_xml_text(&xml)
}

fn docx_xml_text(xml: &str) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut text = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event().map_err(|e| format!("Bad DOCX XML: {e}"))? {
            Event::Start(e) if e.name().as_ref() == "w:t" => in_text = true,
            Event::End(e) => match e.name().as_ref() {
                "w:t" => in_text = false,
                "w:p" => text.push('\n'),
                _ => {}
            },
            Event::Empty(e) => match e.name().as_ref() {
                "w:tab" => text.push('\t'),
                "w:br" | "w:cr" => text.push('\n'),
                _ => {}
            },
            Event::Text(t) if in_text => text.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if in_text => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    text.push(c);
                } else if let Some(s) = resolve_predefined_entity(&r) {
                    text.push_str(s);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_xml_paragraphs_entities_and_tabs() {
        let xml = r#"<w:document><w:body>
            <w:p><w:r><w:t>Q3 budget</w:t></w:r><w:r><w:tab/><w:t xml:space="preserve"> &amp; plan</w:t></w:r></w:p>
            <w:p><w:r><w:t>caf&#233;</w:t></w:r></w:p>
            <w:p><w:r><w:instrText>IGNORED</w:instrText></w:r></w:p>
        </w:body></w:document>"#;
        assert_eq!(docx_xml_text(xml).unwrap(), "Q3 budget\t & plan\ncafé");
    }
}
