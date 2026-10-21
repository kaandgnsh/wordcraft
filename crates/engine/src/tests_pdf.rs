//! Opening PDF files: converted to an editable document (Word's PDF Reflow).

use serde_json::{Value, json};

use crate::Session;
use crate::io::{needs_password, open_bytes, open_bytes_with, save_bytes, wrong_password};

/// A 595×842 PDF, one page per content stream, drawing Helvetica (/F1) and Helvetica-Bold (/F2).
fn pdf(pages: &[&str]) -> Vec<u8> {
    let mut objs: Vec<String> = Vec::new();
    let kids: Vec<String> = (0..pages.len()).map(|i| format!("{} 0 R", 5 + 2 * i)).collect();
    objs.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()));
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into());
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".into());
    for (i, content) in pages.iter().enumerate() {
        objs.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents {} 0 R /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> >>",
            6 + 2 * i
        ));
        objs.push(format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()));
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn heading_pdf() -> Vec<u8> {
    pdf(&["BT /F2 24 Tf 72 760 Td (Annual report) Tj ET BT /F1 11 Tf 72 720 Td (The body text of the report.) Tj ET"])
}

fn three_pages() -> Vec<u8> {
    pdf(&[
        "BT /F1 11 Tf 72 760 Td (Page one text) Tj ET",
        "BT /F1 11 Tf 72 760 Td (Page two text) Tj ET",
        "BT /F1 11 Tf 72 760 Td (Page three text) Tj ET",
    ])
}

/// A PDF made by PdfCraft (File › Protect) with the open password `kullanici`; see
/// ATTRIBUTION.md.
const ENCRYPTED: &[u8] = include_bytes!("../tests/data/encrypted.pdf");

fn text(doc: &wordcraft_doc::Document) -> String {
    doc.plain_text(wordcraft_doc::StoryRef::Body)
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("wordcraft-pdf-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn pdf_roundtrip_keeps_text() {
    let doc = crate::sample::sample_document();
    let pdf = save_bytes("a.pdf", &doc).unwrap();
    let back = squash(&text(&open_bytes("a.pdf", &pdf).unwrap()));
    let original = squash(&text(&doc));
    let first_line = original.split(". ").next().unwrap();
    let probe: String = first_line.chars().take(40).collect();
    assert!(back.contains(&probe), "{probe:?} in {back:?}");
}

#[test]
fn pdf_heading_becomes_heading_style() {
    let doc = open_bytes("r.pdf", &heading_pdf()).unwrap();
    let first = doc.body_fragment().blocks;
    let style = first.iter().find_map(|b| match b {
        wordcraft_doc::Block::Para(p) if !p.text.trim().is_empty() => Some(p.props.style.clone()),
        _ => None,
    });
    assert!(matches!(style.flatten().as_deref(), Some("Heading1" | "Title")), "first paragraph is a heading");
    assert!(squash(&text(&doc)).contains("The body text of the report."));
}

#[test]
fn pdf_by_signature_under_docx_name() {
    let doc = open_bytes("r.docx", &heading_pdf()).unwrap();
    assert!(text(&doc).contains("Annual report"));
}

#[test]
fn not_a_pdf_named_pdf() {
    let e = open_bytes("x.pdf", b"merhaba").unwrap_err();
    assert!(e.contains("x.pdf"), "{e}");
}

#[test]
fn pdf_password_errors_are_recognised() {
    let e = open_bytes_with("e.pdf", ENCRYPTED, None).unwrap_err();
    assert!(needs_password(&e), "{e}");
    let e = open_bytes_with("e.pdf", ENCRYPTED, Some("yanlis")).unwrap_err();
    assert!(wrong_password(&e), "{e}");
    let (doc, _) = open_bytes_with("e.pdf", ENCRYPTED, Some("kullanici")).unwrap();
    assert!(!text(&doc).trim().is_empty());
}

#[test]
fn saving_an_opened_pdf_does_not_overwrite_it() {
    let bytes = heading_pdf();
    let path = tmp("keep.pdf", &bytes);
    let mut s = Session::new(wordcraft_doc::Document::new());
    run(&mut s, "file.open", json!({"path": path.to_str().unwrap()}));
    run(&mut s, "document.setText", json!({"text": "changed"}));
    let r = run(&mut s, "file.save", json!({}));
    assert_eq!(r["saved"], json!(false), "{r}");
    assert_eq!(std::fs::read(&path).unwrap(), bytes, "the PDF is untouched");
    assert!(s.ui_requests.iter().any(|u| u == &json!({"open": "saveAs"})));
}

#[test]
fn hostile_pdf_never_panics() {
    let good = three_pages();
    for step in 1..8 {
        let _ = open_bytes("cut.pdf", &good[..good.len() * step / 8]);
    }
    for seed in 0..128usize {
        let mut v = good.clone();
        for i in (seed % 89..v.len()).step_by(89) {
            v[i] = v[i].wrapping_add(seed as u8).rotate_left(3);
        }
        let _ = open_bytes("bad.pdf", &v);
    }
}
