pub mod model;
pub mod passes;

use milestone_core::{rule_hash_hex, MilestoneSpec};
pub use model::{
    Finding, LintCategory, ListabilityReport, ListabilityStatus, Severity, SeverityCounts,
};
pub use passes::{default_passes, LintPass};

/// Runs all default static analysis passes against `spec` and returns sorted findings.
pub fn lint_spec(spec: &MilestoneSpec) -> Vec<Finding> {
    let passes = default_passes();
    let mut findings = Vec::new();
    for pass in passes {
        findings.extend(pass.check(spec));
    }
    // Sort deterministically: highest severity first, then category (metric/temporal before secondary), then ID.
    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| category_priority(a.category).cmp(&category_priority(b.category)))
            .then_with(|| a.id.cmp(&b.id))
    });
    findings
}

fn category_priority(c: LintCategory) -> u8 {
    match c {
        LintCategory::SubjectControlRisk => 0,
        LintCategory::MetricCompleteness => 1,
        LintCategory::TemporalCompleteness => 2,
        LintCategory::EvidenceDeterminism => 3,
        LintCategory::SourceConflictDeterminism => 4,
        LintCategory::CurrencyDeterminism => 5,
        LintCategory::ObservationDeterminism => 6,
        LintCategory::RevisionDeterminism => 7,
        LintCategory::CancellationSemantics => 8,
        LintCategory::IdentityDeterminism => 9,
        LintCategory::ThresholdCompleteness => 10,
        LintCategory::UnitCompleteness => 11,
        LintCategory::OutcomeExhaustiveness => 12,
        LintCategory::EvidenceAvailability => 13,
    }
}

/// Evaluates whether a specification is deterministic enough to be listed.
///
/// Rules:
/// - Any CRITICAL finding -> NOT_LISTABLE
/// - Any HIGH finding -> NOT_LISTABLE (unresolved resolution-affecting ambiguity)
/// - Only MEDIUM/LOW findings -> LISTABLE_WITH_WARNINGS
/// - Zero findings or only INFO findings -> LISTABLE
pub fn evaluate_listability(spec: &MilestoneSpec) -> ListabilityReport {
    let findings = lint_spec(spec);
    let mut counts = SeverityCounts::default();
    for f in &findings {
        match f.severity {
            Severity::Critical => counts.critical += 1,
            Severity::High => counts.high += 1,
            Severity::Medium => counts.medium += 1,
            Severity::Low => counts.low += 1,
            Severity::Info => counts.info += 1,
        }
    }

    let status = if counts.critical > 0 || counts.high > 0 {
        ListabilityStatus::NotListable
    } else if counts.medium > 0 || counts.low > 0 {
        ListabilityStatus::ListableWithWarnings
    } else {
        ListabilityStatus::Listable
    };

    let mut reasons = Vec::new();
    if counts.critical > 0 {
        reasons.push(format!(
            "{} CRITICAL finding(s) block deterministic resolution or introduce unilateral control risk.",
            counts.critical
        ));
    }
    if counts.high > 0 {
        reasons.push(format!(
            "{} HIGH-severity semantic or evidentiary ambiguity(ies) allow reasonable resolvers to diverge.",
            counts.high
        ));
    }
    if status == ListabilityStatus::ListableWithWarnings {
        reasons.push(format!(
            "No CRITICAL or HIGH findings, but {} MEDIUM and {} LOW warning(s) remain for operational hardening.",
            counts.medium, counts.low
        ));
    }
    if status == ListabilityStatus::Listable {
        reasons.push("All required semantic, temporal, FX, evidence, and revision policies are explicitly defined.".into());
    }

    let suggested_next_step = findings
        .first()
        .map(|f| format!("Define `{}` ({})", f.affected_path, f.title));

    ListabilityReport {
        spec_id: spec.id.clone(),
        title: spec.title.clone(),
        rule_hash: rule_hash_hex(spec),
        status,
        counts,
        reasons,
        findings,
        suggested_next_step,
    }
}
