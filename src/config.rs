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
