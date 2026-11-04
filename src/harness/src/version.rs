//! The version of record: the one line of the VERSION file at the
//! repository root. The kernel's build script hands the same line to the
//! banner, the pack stamps it into the packages and the release verbs
//! name the archives by it; the gates that expect the banner read it here.
use std::path::Path;

/// The file, relative to the repository root.
pub const FILE: &str = "VERSION";

/// The version in a VERSION file's text: one ASCII line without spaces,
/// its line end dropped.
pub fn parse(text: &str) -> Result<String, String> {
    let v = text.trim_end_matches(['\n', '\r']);
    if v.is_empty() || !v.is_ascii() || v.chars().any(char::is_whitespace) {
        return Err(format!("{FILE} at the repository root must hold one ASCII line without spaces (read {v:?})"));
    }
    Ok(v.to_string())
}

/// The version of the checkout at `repo`.
pub fn read(repo: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(repo.join(FILE)).map_err(|e| format!("{FILE} at the repository root: {e}"))?;
    parse(&text)
}
