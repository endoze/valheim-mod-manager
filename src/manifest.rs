use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
  pub version_number: String,
  #[serde(default)]
  pub name: String,
  #[serde(default)]
  pub description: String,
  #[serde(default)]
  pub website_url: Option<String>,
  #[serde(default)]
  pub dependencies: Vec<String>,
}

impl Manifest {
  pub fn from_file<P: AsRef<Path>>(path: P) -> AppResult<Self> {
    let manifest_content = fs::read_to_string(&path)
      .map_err(|e| AppError::Manifest(format!("Failed to read manifest file: {}", e)))?;

    let cleaned_content = Self::clean_manifest_content(&manifest_content);

    serde_json::from_str(&cleaned_content)
      .map_err(|e| AppError::Manifest(format!("Failed to parse manifest JSON: {}", e)))
  }

  /// Cleans invisible characters from manifest content while preserving valid non-ASCII characters
  fn clean_manifest_content(content: &str) -> String {
    content
      .chars()
      .filter(|&c| {
        (' '..='~').contains(&c)
          || c == '\n'
          || c == '\r'
          || c == '\t'
          || (c > '\u{007F}' && !is_invisible_control_char(c))
      })
      .collect()
  }
}

/// Checks if a character is an invisible control character that should be removed
fn is_invisible_control_char(c: char) -> bool {
  matches!(
    c,
    '\u{200B}' | // Zero width space
    '\u{200C}' | // Zero width non-joiner
    '\u{200D}' | // Zero width joiner
    '\u{200E}' | // Left-to-right mark
    '\u{200F}' | // Right-to-left mark
    '\u{FEFF}' | // Zero width no-break space (BOM)
    '\u{00A0}' | // Non-breaking space
    '\u{2060}' | // Word joiner
    '\u{2061}' | // Function application
    '\u{2062}' | // Invisible times
    '\u{2063}' | // Invisible separator
    '\u{2064}' // Invisible plus
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs::File;
  use std::io::Write;
  use tempfile::tempdir;

  #[test]
  fn test_clean_manifest_content() {
    let content = r#"{
      "version_number": "1.0.0",
      "name": "Test Mod",
      "description": "This is a test mod"
    }"#;

    let cleaned = Manifest::clean_manifest_content(content);
    assert_eq!(cleaned, content);

    let content_with_invisibles = format!(
      "{{{}\"version_number\": \"1.0.0\",{}\n\"name\": \"Test Mod\",\n\"description\": \"This is a test mod\"\n}}",
      '\u{200B}', '\u{FEFF}'
    );

    let cleaned = Manifest::clean_manifest_content(&content_with_invisibles);

    assert!(!cleaned.contains('\u{200B}'));
    assert!(!cleaned.contains('\u{FEFF}'));
    assert!(cleaned.contains("version_number"));
    assert!(cleaned.contains("1.0.0"));
    assert!(cleaned.contains("Test Mod"));

    let content_with_unicode = r#"{
      "version_number": "1.0.0",
      "name": "Test Mod",
      "description": "This is a test mod with unicode: 你好, ¿Cómo estás?"
    }"#;

    let cleaned = Manifest::clean_manifest_content(content_with_unicode);
    assert_eq!(cleaned, content_with_unicode);
  }

  #[test]
  fn test_is_invisible_control_char() {
    assert!(is_invisible_control_char('\u{200B}')); // Zero width space
    assert!(is_invisible_control_char('\u{FEFF}')); // BOM
    assert!(is_invisible_control_char('\u{00A0}')); // Non-breaking space

    assert!(!is_invisible_control_char('a'));
    assert!(!is_invisible_control_char('1'));
    assert!(!is_invisible_control_char('-'));
    assert!(!is_invisible_control_char(' '));
    assert!(!is_invisible_control_char('你'));
    assert!(!is_invisible_control_char('é'));
  }

  #[test]
  fn test_from_file() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("manifest.json");

    let manifest_content = r#"{
      "version_number": "1.2.3",
      "name": "Test Mod",
      "description": "Test mod description",
      "website_url": "https://example.com",
      "dependencies": ["Owner1-ModA", "Owner2-ModB"]
    }"#;

    let mut file = File::create(&file_path).unwrap();
    file.write_all(manifest_content.as_bytes()).unwrap();

    let manifest = Manifest::from_file(&file_path).unwrap();

    assert_eq!(manifest.version_number, "1.2.3");
    assert_eq!(manifest.name, "Test Mod");
    assert_eq!(manifest.description, "Test mod description");
    assert_eq!(
      manifest.website_url,
      Some("https://example.com".to_string())
    );
    assert_eq!(manifest.dependencies, vec!["Owner1-ModA", "Owner2-ModB"]);

    let malformed_path = dir.path().join("malformed.json");
    let malformed_content = r#"{
      "version_number": "1.0.0",
      "name": "Malformed JSON,
      "description": "Missing quotes"
    }"#;

    let mut file = File::create(&malformed_path).unwrap();
    file.write_all(malformed_content.as_bytes()).unwrap();

    let result = Manifest::from_file(&malformed_path);
    assert!(result.is_err());
  }
}
