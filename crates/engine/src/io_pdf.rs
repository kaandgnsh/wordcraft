//! PDF Open: PdfCraft converts the PDF to Word, which the Word reader opens (Word's PDF Reflow).

use wordcraft_doc::Document;

/// What converting a PDF left out or found, for the message Open shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PdfReport {
    /// Pages in the PDF.
    pub page_count: usize,
    /// Pages (0-based) whose forms were nested too deep to read in full: text may be missing.
    pub truncated_pages: Vec<usize>,
    /// Pages (0-based) with pictures but no text: scans, whose words need text recognition.
    pub image_only_pages: Vec<usize>,
}

/// Open the PDF in `bytes` (named `name`) as a document: `pages` (0-based) only, when given. The
/// errors for a missing or wrong password are the texts the `.docx` reader gives, so
/// [`crate::io::needs_password`] and [`crate::io::wrong_password`] (and the password dialog)
/// recognise them.
pub(crate) fn open_pdf(name: &str, bytes: &[u8], password: Option<&str>, pages: Option<&[usize]>) -> Result<(Document, PdfReport), String> {
    use pdfcraft_office::{Options, Target};
    let opts = Options { password: password.map(str::to_owned), pages: pages.map(<[usize]>::to_vec), title: None };
    let out = pdfcraft_office::convert(bytes, Target::Docx, &opts).map_err(|e| message(name, e))?;
    let doc = wordcraft_docx::read(&out.bytes).map_err(|e| format!("{name}: the converted PDF could not be read ({e})"))?;
    Ok((doc, PdfReport { page_count: out.page_count, truncated_pages: out.truncated_pages, image_only_pages: out.image_only_pages }))
}

/// [`open_pdf`] of the pages named like `2-3, 5` (1-based; all when `pages` is `None` or
/// blank), ready to edit as a freshly opened document is. A range naming a page the PDF doesn't
/// have is an error that gives the PDF's page count.
pub(crate) fn open_pdf_pages(name: &str, bytes: &[u8], password: Option<&str>, pages: Option<&str>) -> Result<(Document, PdfReport), String> {
    let only = match pages.map(str::trim).filter(|t| !t.is_empty()) {
        None => None,
        Some(t) => {
            let count = pdfcraft_office::page_count(bytes, password).map_err(|e| message(name, e))?;
            Some(pdfcraft_office::parse_page_range(t, count).map_err(|e| message(name, e))?)
        }
    };
    let (mut doc, report) = open_pdf(name, bytes, password, only.as_deref())?;
    crate::io::opened(&mut doc);
    Ok((doc, report))
}

/// The file at `path` when it is a PDF, by its header or its `.pdf` name (one that isn't a PDF
/// then fails to convert, rather than opening as text); `None` for other files.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read_if_pdf(path: &str) -> Result<Option<Vec<u8>>, String> {
    let named = std::path::Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    if !named && !is_pdf(&crate::io::read_file_head(path, 1029)?) {
        return Ok(None);
    }
    crate::io::read_file_capped(path, pdfcraft_office::MAX_INPUT_BYTES as u64, "PDF").map(Some)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn read_if_pdf(_path: &str) -> Result<Option<Vec<u8>>, String> {
    Ok(None)
}

/// A conversion error as Open reports it: the password texts the `.docx` reader gives, other
/// errors with the file's name.
fn message(name: &str, e: pdfcraft_office::Error) -> String {
    use pdfcraft_office::Error;
    match e {
        Error::NeedsPassword => wordcraft_docx::PASSWORD_REQUIRED.to_string(),
        Error::WrongPassword => wordcraft_docx::WRONG_PASSWORD.to_string(),
        Error::NotPdf => format!("{name}: not a PDF file"),
        other => format!("{name}: {other}"),
    }
}

/// Is this a PDF (its `%PDF-` header)?
pub(crate) fn is_pdf(bytes: &[u8]) -> bool {
    pdfcraft_office::is_pdf(bytes)
}
