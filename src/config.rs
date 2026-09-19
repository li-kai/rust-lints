use serde::Deserialize;

/// Which logging framework to suggest in diagnostics.
///
/// Deserialized from `dylint.toml` as `"tracing"` or `"log"`.
/// Invalid values produce a serde error at config load time.
#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFramework {
    #[default]
    Tracing,
    Log,
}

impl LogFramework {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tracing => "tracing",
            Self::Log => "log",
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct DebugRemnantsConfig {
    /// Which logging framework to suggest: `"tracing"` (default) or `"log"`.
    pub suggested_framework: LogFramework,
}

#[derive(Deserialize)]
#[serde(default)]
pub struct SuggestBuilderConfig {
    pub threshold: usize,
    /// Derive names that exempt a struct from this lint.
    /// Matches the last path segment (e.g. `"Default"` matches both
    /// `#[derive(Default)]` and `#[derive(std::default::Default)]`).
    pub skip_derives: Vec<String>,
}

impl Default for SuggestBuilderConfig {
    fn default() -> Self {
        Self {
            threshold: 6,
            skip_derives: vec![
                "Default".into(),
                "Queryable".into(),
                "Insertable".into(),
                "Selectable".into(),
            ],
        }
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

/// Config for the `fallible_new` lint.
#[derive(Deserialize)]
#[serde(default)]
pub struct FallibleNewConfig {
    /// Also lint `fn new_*()` methods, not just `fn new()`.
    pub check_new_variants: bool,
}

impl Default for FallibleNewConfig {
    fn default() -> Self {
        Self {
            check_new_variants: true,
        }
    }
}

/// Whether the current compilation observes every module dependency that can
/// exist in any supported feature/target configuration.
///
/// Dead-edge diagnostics are only sound for a complete observation. A normal
/// rustc invocation sees one cfg slice, so incomplete is the safe default.
#[derive(Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeadEdgeCoverage {
    #[default]
    Incomplete,
    Complete,
}

/// Config for the `module_dependencies` lint.
#[derive(Deserialize)]
#[serde(default)]
pub struct ModuleDependenciesConfig {
    /// When true, every top-level module must appear in the config.
    pub exhaustive: bool,
    /// Set to `"complete"` only when this compilation observes the union of
    /// every supported feature/target configuration. Dead edges are not
    /// reported for the default `"incomplete"` coverage.
    pub dead_edge_coverage: DeadEdgeCoverage,
    /// Map of module name → list of modules it may depend on.
    pub allow: std::collections::HashMap<String, Vec<String>>,
}

impl Default for ModuleDependenciesConfig {
    fn default() -> Self {
        Self {
            exhaustive: false,
            dead_edge_coverage: DeadEdgeCoverage::Incomplete,
            allow: std::collections::HashMap::new(),
        }
    }
}

/// Per-sublint configuration shared by all four `global_side_effect` lints.
#[derive(Default, Deserialize)]
#[serde(default)]
pub struct SubLintConfig {
    /// Extra paths to flag, merged with built-in defaults.
    pub additional_paths: Vec<String>,
    /// If set, replaces built-in defaults entirely.
    pub paths: Option<Vec<String>>,
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
    pub logging_init: SubLintConfig,
}

/// An external type whose value must be destroyed on a particular thread or
/// execution context.
#[derive(Deserialize)]
pub struct ExternalThreadAffineTypeConfig {
    /// Fully qualified definition path, for example `objc2::rc::Retained`.
    pub path: String,
    /// The contract violated by destruction on another thread.
    pub reason: String,
}

/// Config for `unsafe_send_thread_affine_drop`.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UnsafeSendThreadAffineDropConfig {
    /// Dependency types that cannot carry the source-local contract attribute.
    pub external_types: Vec<ExternalThreadAffineTypeConfig>,
}
