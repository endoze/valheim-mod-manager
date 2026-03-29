use lasso::{Rodeo, Spur};

/// Type alias for the string interner (lasso::Rodeo).
///
/// The interner stores unique strings and returns small integer keys that can be
/// used to retrieve them later, reducing memory usage for repeated strings.
pub type StringInterner = Rodeo;

/// Type alias for interned string keys (lasso::Spur).
///
/// These keys are compact integer identifiers that reference strings stored in
/// a StringInterner.
pub type InternKey = Spur;

/// Interns an optional string value.
///
/// # Parameters
///
/// * `interner` - The string interner to use
/// * `s` - Optional string slice to intern
///
/// # Returns
///
/// An optional intern key, or None if the input was None.
pub fn intern_option(interner: &mut StringInterner, s: Option<&str>) -> Option<InternKey> {
  s.map(|val| interner.get_or_intern(val))
}

/// Resolves an optional interned key back to a String.
///
/// # Parameters
///
/// * `interner` - The string interner containing the interned strings
/// * `key` - Optional intern key to resolve
///
/// # Returns
///
/// The resolved string, or None if the key was None.
pub fn resolve_option(interner: &StringInterner, key: Option<InternKey>) -> Option<String> {
  key.map(|k| interner.resolve(&k).to_string())
}

/// Interns a slice of strings into a vector of intern keys.
///
/// # Parameters
///
/// * `interner` - The string interner to use
/// * `strings` - Slice of strings to intern
///
/// # Returns
///
/// A vector of intern keys corresponding to the input strings.
pub fn intern_vec(interner: &mut StringInterner, strings: &[String]) -> Vec<InternKey> {
  strings
    .iter()
    .map(|s| interner.get_or_intern(s.as_str()))
    .collect()
}

/// Resolves a slice of intern keys back to a vector of Strings.
///
/// # Parameters
///
/// * `interner` - The string interner containing the interned strings
/// * `keys` - Slice of intern keys to resolve
///
/// # Returns
///
/// A vector of resolved strings.
pub fn resolve_vec(interner: &StringInterner, keys: &[InternKey]) -> Vec<String> {
  keys
    .iter()
    .map(|k| interner.resolve(k).to_string())
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_intern_option_some() {
    let mut interner = StringInterner::default();
    let key = intern_option(&mut interner, Some("test"));
    assert!(key.is_some());
    assert_eq!(resolve_option(&interner, key), Some("test".to_string()));
  }

  #[test]
  fn test_intern_option_none() {
    let mut interner = StringInterner::default();
    let key = intern_option(&mut interner, None);
    assert!(key.is_none());
    assert_eq!(resolve_option(&interner, key), None);
  }

  #[test]
  fn test_intern_vec() {
    let mut interner = StringInterner::default();
    let strings = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let keys = intern_vec(&mut interner, &strings);
    assert_eq!(keys.len(), 3);
    assert_eq!(resolve_vec(&interner, &keys), strings);
  }

  #[test]
  fn test_intern_vec_empty() {
    let mut interner = StringInterner::default();
    let strings: Vec<String> = vec![];
    let keys = intern_vec(&mut interner, &strings);
    assert!(keys.is_empty());
    assert!(resolve_vec(&interner, &keys).is_empty());
  }

  #[test]
  fn test_deduplication() {
    let mut interner = StringInterner::default();
    let key1 = interner.get_or_intern("duplicate");
    let key2 = interner.get_or_intern("duplicate");
    assert_eq!(key1, key2);
    assert_eq!(interner.len(), 1);
  }

  #[test]
  fn test_multiple_strings() {
    let mut interner = StringInterner::default();
    let key_a = interner.get_or_intern("a");
    let key_b = interner.get_or_intern("b");
    let key_a2 = interner.get_or_intern("a");

    assert_eq!(key_a, key_a2);
    assert_ne!(key_a, key_b);
    assert_eq!(interner.len(), 2);
  }
}
