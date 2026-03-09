use serde::Deserialize;

#[derive(Deserialize)]
#[serde(default)]
pub struct SuggestBuilderConfig {
    pub threshold: usize,
}

impl Default for SuggestBuilderConfig {
    fn default() -> Self {
        Self { threshold: 4 }
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct NeedlessBuilderConfig {
    pub threshold: usize,
}

impl Default for NeedlessBuilderConfig {
    fn default() -> Self {
        Self { threshold: 2 }
    }
}

/// Top-level config for the `global_side_effect` lint group.
///
/// Read from `dylint.toml` under the key `global_side_effect`:
/// ```toml
/// [global_side_effect.time]
/// additional_paths = ["my_crate::clock::now"]
/// ```
#[derive(Default, Deserialize)]
#[serde(default)]
pub struct GlobalSideEffectConfig {
    pub time: SubLintConfig,
    pub randomness: SubLintConfig,
    pub env: SubLintConfig,
}

/// Per-sublint configuration shared by all three `global_side_effect` lints.
#[derive(Default, Deserialize)]
#[serde(default)]
pub struct SubLintConfig {
    /// Extra paths to flag, merged with built-in defaults.
    pub additional_paths: Vec<String>,
    /// If set, replaces built-in defaults entirely.
    pub paths: Option<Vec<String>>,
}
