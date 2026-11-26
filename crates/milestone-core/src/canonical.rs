use crate::ast::{ClassPolicy, MilestoneSpec, Threshold};
use rust_decimal::Decimal;
use serde_json::{Map, Value};
use tiny_keccak::{Hasher, Keccak};

/// Returns a canonically normalized clone of a `MilestoneSpec` where:
/// - string fields are trimmed, currency codes uppercased
/// - Decimal amounts are normalized (no trailing fractional zeros)
/// - unordered set-like Vec fields are sorted and deduplicated
/// - ordered Vec fields (like `precedence_order`) preserve their semantic order
pub fn canonicalize_spec(spec: &MilestoneSpec) -> MilestoneSpec {
    let mut s = spec.clone();
    s.schema_version = s.schema_version.trim().to_string();
    s.id = s.id.trim().to_string();
    s.title = s.title.trim().to_string();
    s.subject.name = s.subject.name.trim().to_string();
    s.subject.jurisdiction = s
        .subject
        .jurisdiction
        .as_deref()
        .map(|j| j.trim().to_uppercase())
        .filter(|j| !j.is_empty());

    let mut clean_ids = std::collections::BTreeMap::new();
    for (k, v) in &s.subject.identifiers {
        clean_ids.insert(k.trim().to_string(), v.trim().to_string());
    }
    s.subject.identifiers = clean_ids;

    if let Threshold::Money(ref mut money) = s.predicate.threshold {
        money.amount = normalize_decimal(money.amount);
        money.currency = money
            .currency
            .as_deref()
            .map(|c| c.trim().to_uppercase())
            .filter(|c| !c.is_empty());
    }

    match &mut s.predicate.class_policy {
        ClassPolicy::Fundraising(fp) => {
            if let Some(ref mut inst) = fp.allowed_instruments {
                inst.sort();
                inst.dedup();
            }
        }
        ClassPolicy::Employment(ep) => {
            ep.target_employer = ep.target_employer.trim().to_string();
            if let Some(ref mut rels) = ep.allowed_relationships {
                rels.sort();
                rels.dedup();
            }
        }
        ClassPolicy::Acquisition(ap) => {
            if let Some(ref mut ct) = ap.consideration_types {
                ct.sort();
                ct.dedup();
            }
            if let Some(ref mut ts) = ap.transaction_structures {
                ts.sort();
                ts.dedup();
            }
        }
        ClassPolicy::Audience(aud) => {
            aud.platform = aud
                .platform
                .as_deref()
                .map(|p| p.trim().to_lowercase())
                .filter(|p| !p.is_empty());
        }
        ClassPolicy::Arr(_) => {}
    }

    s.observation.timezone = s
        .observation
        .timezone
        .as_deref()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());

    if let Some(ref mut cp) = s.currency_policy {
        cp.reporting_currency = cp
            .reporting_currency
            .as_deref()
            .map(|c| c.trim().to_uppercase())
            .filter(|c| !c.is_empty());
    }

    if let Some(ref mut primary) = s.evidence_policy.primary_source {
        primary.name = primary.name.trim().to_string();
    }
    for src in &mut s.evidence_policy.fallback_sources {
        src.name = src.name.trim().to_string();
    }
    s.evidence_policy.fallback_sources.sort();
    s.evidence_policy.fallback_sources.dedup();

    // Note: precedence_order is strictly ordered — only deduplicate preserving first occurrence.
    let mut seen_precedence = std::collections::BTreeSet::new();
    s.evidence_policy
        .precedence_order
        .retain(|st| seen_precedence.insert(*st));

    s.invalidation_policy.invalid_triggers.sort();
    s.invalidation_policy.invalid_triggers.dedup();

    s
}

fn normalize_decimal(d: Decimal) -> Decimal {
    let n = d.normalize();
    if n.is_zero() {
        Decimal::ZERO
    } else {
        n
    }
}

/// Recursively sorts all JSON object keys lexicographically so the serialized byte
/// representation is independent of struct field order or map insertion order.
fn sort_json_value(val: Value) -> Value {
    match val {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map
                .into_iter()
                .map(|(k, v)| (k, sort_json_value(v)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut sorted = Map::new();
            for (k, v) in entries {
                sorted.insert(k, v);
            }
            Value::Object(sorted)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(sort_json_value).collect()),
        other => other,
    }
}

/// Produces deterministic, whitespace-free canonical UTF-8 JSON bytes for a `MilestoneSpec`.
pub fn canonical_bytes(spec: &MilestoneSpec) -> Vec<u8> {
    let normalized = canonicalize_spec(spec);
    let value = serde_json::to_value(&normalized)
        .expect("MilestoneSpec serialization to serde_json::Value must succeed");
    let sorted = sort_json_value(value);
    serde_json::to_vec(&sorted).expect("sorted Value serialization must succeed")
}

/// Computes the Keccak-256 hash of `canonical_bytes(spec)`.
pub fn rule_hash(spec: &MilestoneSpec) -> [u8; 32] {
    let bytes = canonical_bytes(spec);
    let mut keccak = Keccak::v256();
    let mut out = [0u8; 32];
    keccak.update(&bytes);
    keccak.finalize(&mut out);
    out
}

/// Returns the `0x`-prefixed lowercase hexadecimal Keccak-256 rule hash.
pub fn rule_hash_hex(spec: &MilestoneSpec) -> String {
    let hash = rule_hash(spec);
    let mut s = String::with_capacity(66);
    s.push_str("0x");
    for byte in hash {
        use std::fmt::Write;
        let _ = write!(&mut s, "{:02x}", byte);
    }
    s
}
