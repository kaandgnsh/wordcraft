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
    /// Pages (0-based) whose words were recognised from their picture.
    pub ocr_pages: Vec<usize>,
}

/// Open the PDF in `bytes` (named `name`) as a document: `pages` (0-based) only, when given. The
/// errors for a missing or wrong password are the texts the `.docx` reader gives, so
/// [`crate::io::needs_password`] and [`crate::io::wrong_password`] (and the password dialog)
/// recognise them.
pub(crate) fn open_pdf(name: &str, bytes: &[u8], password: Option<&str>, pages: Option<&[usize]>) -> Result<(Document, PdfReport), String> {
    use pdfcraft_office::{Options, Target};
    let opts = Options { password: password.map(str::to_owned), pages: pages.map(<[usize]>::to_vec), title: None };
    let mut out = pdfcraft_office::convert(bytes, Target::Docx, &opts).map_err(|e| message(name, e))?;
    // Scans are read when the text recognition models are installed.
    #[cfg(not(target_arch = "wasm32"))]
    if !out.image_only_pages.is_empty()
        && let Some(ocr) = ocr()
    {
        out = pdfcraft_office::convert_with_ocr(bytes, Target::Docx, &opts, ocr).map_err(|e| message(name, e))?;
    }
    let doc = wordcraft_docx::read(&out.bytes).map_err(|e| format!("{name}: the converted PDF could not be read ({e})"))?;
    Ok((
        doc,
        PdfReport {
            page_count: out.page_count,
            truncated_pages: out.truncated_pages,
            image_only_pages: out.image_only_pages,
            ocr_pages: out.ocr_pages,
        },
    ))
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

/// The folder WordCraft keeps the text recognition models in, per user: `ocr-models` in the
/// app's data folder (`~/.local/share/wordcraft`, `~/Library/Application Support/WordCraft`,
/// `%LOCALAPPDATA%\WordCraft`).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn models_folder() -> std::path::PathBuf {
    use std::path::PathBuf;
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let home = var("HOME").unwrap_or_default();
    let base = if cfg!(windows) {
        var("LOCALAPPDATA").or_else(|| var("APPDATA")).unwrap_or(home).join("WordCraft")
    } else if cfg!(target_os = "macos") {
        home.join("Library/Application Support/WordCraft")
    } else {
        var("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share")).join("wordcraft")
    };
    base.join("ocr-models")
}

/// Where the models are looked for, in order: `WORDCRAFT_OCR_MODELS`, [`models_folder`], then
/// PdfCraft's (`PDFCRAFT_MODELS`, its installed packages, a source tree beside this one).
#[cfg(not(target_arch = "wasm32"))]
fn model_dirs() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    let mut dirs: Vec<PathBuf> = std::env::var_os("WORDCRAFT_OCR_MODELS").filter(|v| !v.is_empty()).map(PathBuf::from).into_iter().collect();
    dirs.push(models_folder());
    if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
        dirs.push(PathBuf::from(home).join(".local/share/pdfcraft/models"));
    }
    dirs.extend(["/usr/share/pdfcraft/models", "/usr/local/share/pdfcraft/models"].map(PathBuf::from));
    dirs.extend(pdfcraft_ocr::Models::search_dirs());
    dirs
}

/// The models in the first of `dirs` that has both files.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn models_in(dirs: &[std::path::PathBuf]) -> Option<pdfcraft_ocr::Models> {
    dirs.iter().find_map(|d| pdfcraft_ocr::Models::in_dir(d))
}

/// The installed text recognition models, if any.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn ocr_models() -> Option<pdfcraft_ocr::Models> {
    models_in(&model_dirs())
}

/// The recogniser, loaded once (it takes a moment) the first time a scan is opened.
#[cfg(not(target_arch = "wasm32"))]
fn ocr() -> Option<&'static pdfcraft_ocr::Ocr> {
    static OCR: std::sync::OnceLock<Option<pdfcraft_ocr::Ocr>> = std::sync::OnceLock::new();
    OCR.get_or_init(|| {
        let models = ocr_models()?;
        pdfcraft_ocr::Ocr::load(&models).inspect_err(|e| log::warn!("text recognition: {e}")).ok()
    })
    .as_ref()
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
