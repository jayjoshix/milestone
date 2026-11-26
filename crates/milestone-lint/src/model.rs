use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Info => write!(f, "INFO"),
            Severity::Low => write!(f, "LOW"),
            Severity::Medium => write!(f, "MEDIUM"),
            Severity::High => write!(f, "HIGH"),
            Severity::Critical => write!(f, "CRITICAL"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintCategory {
    TemporalCompleteness,
    MetricCompleteness,
    ThresholdCompleteness,
    UnitCompleteness,
    CurrencyDeterminism,
    EvidenceDeterminism,
    SourceConflictDeterminism,
    RevisionDeterminism,
    IdentityDeterminism,
    SubjectControlRisk,
    OutcomeExhaustiveness,
    CancellationSemantics,
    EvidenceAvailability,
    ObservationDeterminism,
}

impl fmt::Display for LintCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub category: LintCategory,
    pub title: String,
    pub explanation: String,
    pub affected_path: String,
    pub remediation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ListabilityStatus {
    Listable,
    ListableWithWarnings,
    NotListable,
}

impl fmt::Display for ListabilityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ListabilityStatus::Listable => write!(f, "LISTABLE"),
            ListabilityStatus::ListableWithWarnings => write!(f, "LISTABLE_WITH_WARNINGS"),
            ListabilityStatus::NotListable => write!(f, "NOT_LISTABLE"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeverityCounts {
    pub critical: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
    pub info: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListabilityReport {
    pub spec_id: String,
    pub title: String,
    pub rule_hash: String,
    pub status: ListabilityStatus,
    pub counts: SeverityCounts,
    pub reasons: Vec<String>,
    pub findings: Vec<Finding>,
    pub suggested_next_step: Option<String>,
}
