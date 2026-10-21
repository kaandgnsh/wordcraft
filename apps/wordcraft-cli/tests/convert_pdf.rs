//! `convert` reads PDFs (PDF Reflow): a PDF in, a Word document out.
use std::process::Command;

#[test]
fn convert_pdf_to_docx() {
    let dir = std::env::temp_dir().join(format!("wc-convert-pdf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let doc = wordcraft_doc::Document::from_text("Converted from a PDF\nSecond paragraph");
    let json = dir.join("in.json");
    std::fs::write(&json, serde_json::to_vec(&doc).unwrap()).unwrap();
    let pdf = dir.join("in.pdf");
    let make = Command::new(env!("CARGO_BIN_EXE_wordcraft-cli")).arg("convert").arg(&json).arg(&pdf).output().unwrap();
    assert!(make.status.success(), "{}", String::from_utf8_lossy(&make.stderr));
    let docx = dir.join("out.docx");
    let run = Command::new(env!("CARGO_BIN_EXE_wordcraft-cli")).arg("convert").arg(&pdf).arg(&docx).output().unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let back = wordcraft_engine::io::open_bytes("out.docx", &std::fs::read(&docx).unwrap()).unwrap();
    let text = back.plain_text(wordcraft_doc::StoryRef::Body);
    assert!(text.contains("Converted from a PDF") && text.contains("Second paragraph"), "{text:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
