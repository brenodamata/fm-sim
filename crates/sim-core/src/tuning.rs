//! Tuning constants, loaded from data rather than compiled in.
//!
//! Tuning is a diffable data change with history, and balance can be adjusted
//! without recompiling. The harness loads the same file and can override any
//! key for a sweep without editing it on disk.
//!
//! The parser here handles flat `key = value` pairs, which is a valid TOML
//! subset — `config/tuning.toml` is real TOML. It exists so `sim-core` has no
//! dependencies beyond the RNG and hasher. Swapping in `toml` + `serde` is a
//! two-line change and worth doing once the config grows tables.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug)]
pub enum TuningError {
    Parse { line: usize, message: String },
    MissingKey(String),
    WrongType { key: String, expected: &'static str },
}

impl fmt::Display for TuningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TuningError::Parse { line, message } => write!(f, "line {line}: {message}"),
            TuningError::MissingKey(k) => write!(f, "missing key: {k}"),
            TuningError::WrongType { key, expected } => {
                write!(f, "key {key} is not a {expected}")
            }
        }
    }
}

impl std::error::Error for TuningError {}

/// Parsed tuning values.
///
/// `BTreeMap` rather than `HashMap`: ordered iteration, so the config hash in a
/// run manifest is stable regardless of insertion order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tuning {
    values: BTreeMap<String, String>,
}

impl Tuning {
    pub fn parse(source: &str) -> Result<Self, TuningError> {
        let mut values = BTreeMap::new();
        for (i, raw) in source.lines().enumerate() {
            let line = strip_comment(raw).trim();
            if line.is_empty() || line.starts_with('[') {
                continue;
            }
            let (key, value) = line.split_once('=').ok_or_else(|| TuningError::Parse {
                line: i + 1,
                message: format!("expected `key = value`, got {line:?}"),
            })?;
            let key = key.trim().to_string();
            let value = value.trim().trim_matches('"').to_string();
            if key.is_empty() {
                return Err(TuningError::Parse {
                    line: i + 1,
                    message: "empty key".into(),
                });
            }
            values.insert(key, value);
        }
        Ok(Tuning { values })
    }

    /// Override a key, as a harness sweep does. Returns self for chaining.
    pub fn with_override(mut self, key: &str, value: &str) -> Self {
        self.values.insert(key.to_string(), value.to_string());
        self
    }

    pub fn f32(&self, key: &str) -> Result<f32, TuningError> {
        self.raw(key)?
            .parse()
            .map_err(|_| TuningError::WrongType { key: key.into(), expected: "float" })
    }

    pub fn u32(&self, key: &str) -> Result<u32, TuningError> {
        self.raw(key)?
            .parse()
            .map_err(|_| TuningError::WrongType { key: key.into(), expected: "integer" })
    }

    pub fn usize(&self, key: &str) -> Result<usize, TuningError> {
        self.raw(key)?
            .parse()
            .map_err(|_| TuningError::WrongType { key: key.into(), expected: "integer" })
    }

    pub fn i32(&self, key: &str) -> Result<i32, TuningError> {
        self.raw(key)?
            .parse()
            .map_err(|_| TuningError::WrongType { key: key.into(), expected: "integer" })
    }

    fn raw(&self, key: &str) -> Result<&str, TuningError> {
        self.values
            .get(key)
            .map(|s| s.as_str())
            .ok_or_else(|| TuningError::MissingKey(key.to_string()))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(|k| k.as_str())
    }

    /// Stable hash of the whole config, recorded in the run manifest so that
    /// "why did this metric move?" is answerable six months later.
    pub fn config_hash(&self) -> u64 {
        use siphasher::sip::SipHasher13;
        use std::hash::Hasher;
        let mut h = SipHasher13::new_with_keys(0x9999_aaaa_bbbb_cccc, 0xdddd_eeee_ffff_0000);
        for (k, v) in &self.values {
            h.write(k.as_bytes());
            h.write_u8(0);
            h.write(v.as_bytes());
            h.write_u8(0);
        }
        h.finish()
    }
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(i) => &line[..i],
        None => line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# a comment
[worldgen]
clubs_per_division = 20
squad_size = 26
strength_spread = 2.5   # inline comment
label = "top flight"
"#;

    #[test]
    fn parses_flat_toml() {
        let t = Tuning::parse(SAMPLE).unwrap();
        assert_eq!(t.usize("clubs_per_division").unwrap(), 20);
        assert_eq!(t.f32("strength_spread").unwrap(), 2.5);
        assert_eq!(t.keys().count(), 4);
    }

    #[test]
    fn missing_key_is_an_error_not_a_default() {
        let t = Tuning::parse(SAMPLE).unwrap();
        assert!(matches!(t.f32("nope"), Err(TuningError::MissingKey(_))));
    }

    #[test]
    fn wrong_type_is_reported() {
        let t = Tuning::parse(SAMPLE).unwrap();
        assert!(matches!(t.u32("label"), Err(TuningError::WrongType { .. })));
    }

    #[test]
    fn overrides_apply() {
        let t = Tuning::parse(SAMPLE).unwrap().with_override("squad_size", "30");
        assert_eq!(t.usize("squad_size").unwrap(), 30);
    }

    #[test]
    fn config_hash_is_order_independent_but_value_sensitive() {
        let a = Tuning::parse("x = 1\ny = 2\n").unwrap();
        let b = Tuning::parse("y = 2\nx = 1\n").unwrap();
        assert_eq!(a.config_hash(), b.config_hash());
        let c = Tuning::parse("x = 1\ny = 3\n").unwrap();
        assert_ne!(a.config_hash(), c.config_hash());
    }
}
