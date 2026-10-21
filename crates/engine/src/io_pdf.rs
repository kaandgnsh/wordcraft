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
    use pdfcraft_office::{Error, Options, Target};
    let opts = Options { password: password.map(str::to_owned), pages: pages.map(<[usize]>::to_vec), title: None };
    let out = pdfcraft_office::convert(bytes, Target::Docx, &opts).map_err(|e| match e {
        Error::NeedsPassword => wordcraft_docx::PASSWORD_REQUIRED.to_string(),
        Error::WrongPassword => wordcraft_docx::WRONG_PASSWORD.to_string(),
        Error::NotPdf => format!("{name}: not a PDF file"),
        other => format!("{name}: {other}"),
    })?;
    let doc = wordcraft_docx::read(&out.bytes).map_err(|e| format!("{name}: the converted PDF could not be read ({e})"))?;
    Ok((doc, PdfReport { page_count: out.page_count, truncated_pages: out.truncated_pages, image_only_pages: out.image_only_pages }))
}

/// Is this a PDF (its `%PDF-` header)?
pub(crate) fn is_pdf(bytes: &[u8]) -> bool {
    pdfcraft_office::is_pdf(bytes)
}
