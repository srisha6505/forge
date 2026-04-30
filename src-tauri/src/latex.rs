//! LaTeX compilation. Tries tectonic first (single-binary, self-contained),
//! then falls back to xelatex then pdflatex. Output is written to a stable
//! per-source cache directory under the OS temp dir so repeated edits to
//! the same `.tex` file don't pile up artifacts.
//!
//! Agent D fleshes this out -- this file currently provides:
//!   - `LatexCompileResult` returned to the frontend
//!   - `LatexStatus` engine availability probe
//!   - skeleton compile() that picks an engine and shells out
//!
//! Errors from the engine are returned as `Err(log)` so the UI can show
//! the build log inline.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Maximum bytes of log returned to the UI. Catastrophic LaTeX runs can
/// emit megabytes; the renderer only needs the tail to diagnose.
const LOG_TAIL_BYTES: usize = 50 * 1024;

fn truncate_log(log: String) -> String {
    if log.len() <= LOG_TAIL_BYTES {
        return log;
    }
    // Drop the head, keep the tail. Slice on a char boundary so the
    // resulting String is valid UTF-8 even if we land mid-codepoint.
    let mut start = log.len() - LOG_TAIL_BYTES;
    while start < log.len() && !log.is_char_boundary(start) {
        start += 1;
    }
    let trimmed = log.len() - start;
    format!(
        "[... {} earlier bytes truncated ...]\n{}",
        trimmed,
        &log[start..]
    )
}

#[derive(Debug, Serialize)]
pub struct LatexCompileResult {
    pub pdf_path: String,
    pub log: String,
    pub engine: String,
}

#[derive(Debug, Serialize)]
pub struct LatexStatus {
    pub tectonic: bool,
    pub xelatex: bool,
    pub pdflatex: bool,
}

fn which(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn engine_status() -> LatexStatus {
    LatexStatus {
        tectonic: which("tectonic"),
        xelatex: which("xelatex"),
        pdflatex: which("pdflatex"),
    }
}

/// Stable per-source output dir under temp. Hashing the absolute path
/// keeps re-builds for the same file in the same directory and avoids
/// collisions between different .tex files with the same basename.
fn output_dir(source: &Path) -> std::io::Result<PathBuf> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("doc");
    let dir = std::env::temp_dir()
        .join("forge-latex")
        .join(format!("{}-{:x}", stem, hasher.finish()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Compile a `.tex` file to PDF. See module docs.
///
/// Returns `Ok(result)` on successful compilation, `Err(log)` on failure
/// (the log already includes the engine's diagnostic output so the UI
/// can render it directly).
pub fn compile(source: &Path) -> Result<LatexCompileResult, String> {
    if !source.exists() {
        return Err(format!("file not found: {}", source.display()));
    }
    let out_dir = output_dir(source).map_err(|e| e.to_string())?;
    let status = engine_status();

    if status.tectonic {
        run_tectonic(source, &out_dir)
    } else if status.xelatex {
        run_latex_engine("xelatex", source, &out_dir)
    } else if status.pdflatex {
        run_latex_engine("pdflatex", source, &out_dir)
    } else {
        Err("No LaTeX engine found on PATH. Install tectonic, xelatex, or pdflatex.".into())
    }
}

fn run_tectonic(source: &Path, out_dir: &Path) -> Result<LatexCompileResult, String> {
    // tectonic: --keep-logs writes <stem>.log alongside the PDF.
    let output = Command::new("tectonic")
        .arg("-X")
        .arg("compile")
        .arg("--keep-logs")
        .arg("--outdir")
        .arg(out_dir)
        .arg(source)
        .output()
        .map_err(|e| format!("failed to spawn tectonic: {e}"))?;

    let log = truncate_log(format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    ));
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "invalid source filename".to_string())?;
    let pdf = out_dir.join(format!("{stem}.pdf"));

    if output.status.success() && pdf.exists() {
        Ok(LatexCompileResult {
            pdf_path: pdf.to_string_lossy().to_string(),
            log,
            engine: "tectonic".to_string(),
        })
    } else {
        Err(log)
    }
}

fn run_latex_engine(
    engine: &str,
    source: &Path,
    out_dir: &Path,
) -> Result<LatexCompileResult, String> {
    // pdflatex/xelatex run twice for stable cross-references. Use
    // -interaction=nonstopmode so a missing package fails fast instead
    // of blocking on stdin. -halt-on-error so we surface the first error.
    let parent = source.parent().unwrap_or(out_dir);
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "invalid source filename".to_string())?;

    let mut last_log = String::new();
    for _ in 0..2 {
        let output = Command::new(engine)
            .arg("-interaction=nonstopmode")
            .arg("-halt-on-error")
            .arg(format!("-output-directory={}", out_dir.display()))
            .arg(source.file_name().unwrap_or_default())
            .current_dir(parent)
            .output()
            .map_err(|e| format!("failed to spawn {engine}: {e}"))?;
        last_log = truncate_log(format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ));
        if !output.status.success() {
            return Err(last_log);
        }
    }

    let pdf = out_dir.join(format!("{stem}.pdf"));
    if pdf.exists() {
        Ok(LatexCompileResult {
            pdf_path: pdf.to_string_lossy().to_string(),
            log: last_log,
            engine: engine.to_string(),
        })
    } else {
        Err(last_log)
    }
}
