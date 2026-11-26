use crate::ast::{
    AcquisitionCompletionEvent, ArrMeasurementBasis, ConsiderationType, EmploymentQualifyingEvent,
    EmploymentRelationship, FinancingInstrument, FundraisingCompletionEvent, FxSource,
    FxTimestampRule, Outcome, SourceType, TransactionStructure, ValuationBasis,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A single structured piece of evidence supplied to the deterministic resolver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub id: String,
    pub source: String,
    pub source_type: SourceType,
    pub publication_time: DateTime<Utc>,
    pub observation_time: DateTime<Utc>,
    pub retrieval_time: DateTime<Utc>,
    pub value: EvidenceValue,
    pub content_hash: String,
    #[serde(default)]
    pub fx_quote: Option<FxQuote>,
    #[serde(default)]
    pub metadata: EvidenceMetadata,
}

/// Optional FX rate attached to an evidence item from an explicit FX source/timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FxQuote {
    pub base_currency: String,
    pub quote_currency: String,
    pub rate: Decimal,
    pub fx_source: FxSource,
    pub fx_timestamp: DateTime<Utc>,
    #[serde(default)]
    pub fx_timestamp_rule: Option<FxTimestampRule>,
}

/// Typed measurement carried by an evidence item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvidenceValue {
    Money { amount: Decimal, currency: String },
    Count { count: u64 },
    Employment { employed: bool, employer: String },
    InvalidationEvent { reason: String },
}

/// Structured qualifiers describing the real-world semantics of an evidence item
/// so the deterministic resolver can match it against the specification's policies.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceMetadata {
    // ARR semantics
    #[serde(default)]
    pub arr_basis: Option<ArrMeasurementBasis>,
    #[serde(default)]
    pub includes_services: Option<bool>,
    #[serde(default)]
    pub includes_one_time_fees: Option<bool>,
    #[serde(default)]
    pub includes_signed_not_started: Option<bool>,
    #[serde(default)]
    pub net_of_discounts: Option<bool>,

    // Fundraising semantics
    #[serde(default)]
    pub fundraising_event: Option<FundraisingCompletionEvent>,
    #[serde(default)]
    pub instruments: Vec<FinancingInstrument>,
    #[serde(default)]
    pub includes_secondary: Option<bool>,
    #[serde(default)]
    pub includes_insider_capital: Option<bool>,
    #[serde(default)]
    pub includes_note_conversion: Option<bool>,
    #[serde(default)]
    pub is_conditional_commitment: Option<bool>,
    #[serde(default)]
    pub unwound_before_finalization: Option<bool>,

    // Employment semantics
    #[serde(default)]
    pub employment_event: Option<EmploymentQualifyingEvent>,
    #[serde(default)]
    pub relationship: Option<EmploymentRelationship>,
    #[serde(default)]
    pub via_acquisition: Option<bool>,
    #[serde(default)]
    pub on_leave_of_absence: Option<bool>,
    #[serde(default)]
    pub never_commenced: Option<bool>,

    // Acquisition semantics
    #[serde(default)]
    pub acquisition_event: Option<AcquisitionCompletionEvent>,
    #[serde(default)]
    pub valuation_basis: Option<ValuationBasis>,
    #[serde(default)]
    pub consideration_types: Vec<ConsiderationType>,
    #[serde(default)]
    pub includes_contingent_earnout: Option<bool>,
    #[serde(default)]
    pub structure: Option<TransactionStructure>,
    #[serde(default)]
    pub terminated_before_close: Option<bool>,

    // Audience semantics
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub is_momentary_peak: Option<bool>,
    #[serde(default)]
    pub after_bot_purge: Option<bool>,
    #[serde(default)]
    pub channel_deleted: Option<bool>,
    #[serde(default)]
    pub migrated_from_channel_id: Option<String>,

    // Common flags
    #[serde(default)]
    pub is_restatement: Option<bool>,
    #[serde(default)]
    pub entity_scope_consolidated: Option<bool>,
    #[serde(default)]
    pub component_id: Option<String>,
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

/// Collection of evidence items presented for resolution.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceBundle {
    pub spec_id: String,
    #[serde(default)]
    pub evaluation_time: Option<DateTime<Utc>>,
    pub items: Vec<EvidenceItem>,
}

/// Detailed deterministic resolution result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub outcome: Outcome,
    pub reason_code: ResolutionReasonCode,
    pub explanation: String,
    pub applied_rule_hash: String,
    pub decisive_evidence_ids: Vec<String>,
    pub trace: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResolutionReasonCode {
    ThresholdSatisfied,
    ThresholdNotSatisfied,
    ConditionAchieved,
    ConditionNotAchieved,
    MissingRequiredPolicy,
    MissingEvidence,
    SourceConflict,
    UnsupportedEvidenceType,
    MissingFxPolicyOrQuote,
    EvidenceAfterDeadline,
    InvalidationTriggered,
    UnwoundOrTerminated,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ResolutionError {
    #[error("evidence bundle spec_id `{bundle}` does not match milestone spec id `{spec}`")]
    SpecIdMismatch { spec: String, bundle: String },

    #[error("invalid spec predicate/threshold configuration: {0}")]
    InvalidSpec(String),
}
