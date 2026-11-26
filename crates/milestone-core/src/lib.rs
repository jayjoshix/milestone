pub mod ast;
pub mod canonical;
pub mod error;
pub mod evidence;

pub use ast::*;
pub use canonical::{canonical_bytes, canonicalize_spec, rule_hash, rule_hash_hex};
pub use error::SpecError;
pub use evidence::*;

/// Loads a `MilestoneSpec` from a YAML string.
pub fn from_yaml_str(input: &str) -> Result<MilestoneSpec, SpecError> {
    Ok(serde_yaml::from_str(input)?)
}

/// Loads a `MilestoneSpec` from a JSON string.
pub fn from_json_str(input: &str) -> Result<MilestoneSpec, SpecError> {
    Ok(serde_json::from_str(input)?)
}

/// Serializes a `MilestoneSpec` to formatted YAML.
pub fn to_yaml_string(spec: &MilestoneSpec) -> Result<String, SpecError> {
    Ok(serde_yaml::to_string(spec)?)
}

/// Serializes a `MilestoneSpec` to pretty JSON.
pub fn to_json_string(spec: &MilestoneSpec) -> Result<String, SpecError> {
    Ok(serde_json::to_string_pretty(spec)?)
}
