use crate::model::{Finding, LintCategory, Severity};
use milestone_core::{
    ClassPolicy, ConflictPolicy, MilestoneSpec, ObservationMode, SourceType, Threshold,
};
use rust_decimal::Decimal;

/// Independent static analysis pass over a `MilestoneSpec`.
pub trait LintPass {
    fn name(&self) -> &'static str;
    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding>;
}

pub struct TemporalCompletenessPass;
pub struct MetricCompletenessPass;
pub struct ThresholdCompletenessPass;
pub struct UnitCompletenessPass;
pub struct CurrencyDeterminismPass;
pub struct EvidenceDeterminismPass;
pub struct SourceConflictDeterminismPass;
pub struct RevisionDeterminismPass;
pub struct IdentityDeterminismPass;
pub struct SubjectControlRiskPass;
pub struct OutcomeExhaustivenessPass;
pub struct CancellationSemanticsPass;
pub struct EvidenceAvailabilityPass;
pub struct ObservationDeterminismPass;

pub fn default_passes() -> Vec<Box<dyn LintPass>> {
    vec![
        Box::new(TemporalCompletenessPass),
        Box::new(MetricCompletenessPass),
        Box::new(ThresholdCompletenessPass),
        Box::new(UnitCompletenessPass),
        Box::new(CurrencyDeterminismPass),
        Box::new(EvidenceDeterminismPass),
        Box::new(SourceConflictDeterminismPass),
        Box::new(RevisionDeterminismPass),
        Box::new(IdentityDeterminismPass),
        Box::new(SubjectControlRiskPass),
        Box::new(OutcomeExhaustivenessPass),
        Box::new(CancellationSemanticsPass),
        Box::new(EvidenceAvailabilityPass),
        Box::new(ObservationDeterminismPass),
    ]
}

impl LintPass for TemporalCompletenessPass {
    fn name(&self) -> &'static str {
        "TemporalCompleteness"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let obs = &spec.observation;

        if obs.deadline.is_none() {
            out.push(Finding {
                id: "TIME-001".into(),
                severity: Severity::Critical,
                category: LintCategory::TemporalCompleteness,
                title: "Observation deadline undefined.".into(),
                explanation: "No UTC deadline timestamp is configured. Resolvers cannot determine when the observation window closes.".into(),
                affected_path: "observation.deadline".into(),
                remediation: "Set `observation.deadline` to an explicit RFC 3339 UTC timestamp.".into(),
            });
        }

        if obs.timezone.is_none() {
            out.push(Finding {
                id: "TIME-003".into(),
                severity: Severity::High,
                category: LintCategory::TemporalCompleteness,
                title: "Reference timezone undefined.".into(),
                explanation: "Events occurring near midnight on the deadline date can fall on different calendar dates in local time vs UTC (for example, 23:58 PT vs 07:58 UTC next day).".into(),
                affected_path: "observation.timezone".into(),
                remediation: "Set `observation.timezone` explicitly (recommended: `UTC` or an explicit IANA zone matching the UTC deadline).".into(),
            });
        }

        if obs.deadline_inclusivity.is_none() {
            out.push(Finding {
                id: "TIME-004".into(),
                severity: Severity::Medium,
                category: LintCategory::TemporalCompleteness,
                title: "Deadline boundary inclusivity undefined.".into(),
                explanation: "The specification does not state whether an event timestamped at the exact `deadline` second counts (`<=` vs `<`).".into(),
                affected_path: "observation.deadline_inclusivity".into(),
                remediation: "Set `observation.deadline_inclusivity` to `inclusive` or `exclusive`.".into(),
            });
        }

        if obs.cutoff_precision.is_none() {
            out.push(Finding {
                id: "TIME-005".into(),
                severity: Severity::Low,
                category: LintCategory::TemporalCompleteness,
                title: "Cutoff timestamp precision undefined.".into(),
                explanation: "It is unspecified whether cutoff checks compare exact UTC seconds or end-of-calendar-day boundaries.".into(),
                affected_path: "observation.cutoff_precision".into(),
                remediation: "Set `observation.cutoff_precision` to `exact_second_utc` or `end_of_calendar_day`.".into(),
            });
        }

        if let (Some(start), Some(deadline)) = (obs.start, obs.deadline) {
            if start >= deadline {
                out.push(Finding {
                    id: "TIME-006".into(),
                    severity: Severity::Critical,
                    category: LintCategory::TemporalCompleteness,
                    title: "Observation window start is not earlier than deadline.".into(),
                    explanation: "The observation window has zero or negative duration, making resolution degenerate.".into(),
                    affected_path: "observation.start".into(),
                    remediation: "Ensure `observation.start` is strictly earlier than `observation.deadline`.".into(),
                });
            }
        }

        out
    }
}

impl LintPass for MetricCompletenessPass {
    fn name(&self) -> &'static str {
        "MetricCompleteness"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();

        if spec.predicate.metric.market_class() != spec.predicate.class_policy.market_class() {
            out.push(Finding {
                id: "METRIC-000".into(),
                severity: Severity::Critical,
                category: LintCategory::MetricCompleteness,
                title: "Predicate metric does not match class policy.".into(),
                explanation: "The declared `predicate.metric` belongs to a different market class than `predicate.class_policy`.".into(),
                affected_path: "predicate.class_policy".into(),
                remediation: "Align `predicate.metric` and `predicate.class_policy` to the same market class.".into(),
            });
            return out;
        }

        match &spec.predicate.class_policy {
            ClassPolicy::Arr(arr) => {
                if arr.measurement_basis.is_none() {
                    out.push(Finding {
                        id: "M-ARR-001".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "ARR measurement basis undefined.".into(),
                        explanation: "The specification does not define whether ARR means contracted recurring revenue, billed recurring revenue, collected cash recurring revenue, or annualized MRR. Different reasonable interpretations may produce different outcomes.".into(),
                        affected_path: "predicate.class_policy.measurement_basis".into(),
                        remediation: "Set `measurement_basis` explicitly (e.g., `contracted_recurring_revenue` or `annualized_mrr`).".into(),
                    });
                }
                if arr.include_services_revenue.is_none() {
                    out.push(Finding {
                        id: "M-ARR-002".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "Professional services revenue treatment undefined.".into(),
                        explanation: "It is unspecified whether implementation, onboarding, or consulting fees count toward ARR.".into(),
                        affected_path: "predicate.class_policy.include_services_revenue".into(),
                        remediation: "Set `include_services_revenue` to `false` or `true`.".into(),
                    });
                }
                if arr.include_one_time_fees.is_none() {
                    out.push(Finding {
                        id: "M-ARR-003".into(),
                        severity: Severity::Medium,
                        category: LintCategory::MetricCompleteness,
                        title: "One-time fee exclusion undefined.".into(),
                        explanation: "Setup fees and non-recurring charges can push borderline ARR figures across the threshold if not explicitly excluded.".into(),
                        affected_path: "predicate.class_policy.include_one_time_fees".into(),
                        remediation: "Set `include_one_time_fees` explicitly.".into(),
                    });
                }
                if arr.usage_based_treatment.is_none() {
                    out.push(Finding {
                        id: "M-ARR-004".into(),
                        severity: Severity::Medium,
                        category: LintCategory::MetricCompleteness,
                        title: "Usage-based revenue treatment undefined.".into(),
                        explanation: "Pure consumption or overage revenue can be excluded, annualized over trailing 30/90 days, or counted at TTM.".into(),
                        affected_path: "predicate.class_policy.usage_based_treatment".into(),
                        remediation: "Set `usage_based_treatment` explicitly.".into(),
                    });
                }
                if arr.discount_treatment.is_none() {
                    out.push(Finding {
                        id: "M-ARR-005".into(),
                        severity: Severity::Medium,
                        category: LintCategory::MetricCompleteness,
                        title: "Discount treatment undefined.".into(),
                        explanation: "It is unspecified whether ARR is measured gross of promotional discounts or net of contracted discounts.".into(),
                        affected_path: "predicate.class_policy.discount_treatment".into(),
                        remediation: "Set `discount_treatment` to `net_of_discounts` or `gross_before_discounts`.".into(),
                    });
                }
                if arr.churn_treatment.is_none() {
                    out.push(Finding {
                        id: "M-ARR-006".into(),
                        severity: Severity::Medium,
                        category: LintCategory::MetricCompleteness,
                        title: "Churn and non-renewal notice treatment undefined.".into(),
                        explanation: "Customers who submitted cancellation notices prior to the deadline may still be billed until term end.".into(),
                        affected_path: "predicate.class_policy.churn_treatment".into(),
                        remediation: "Set `churn_treatment` explicitly.".into(),
                    });
                }
                if arr.signed_not_started_treatment.is_none() {
                    out.push(Finding {
                        id: "M-ARR-007".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "Signed-not-started contract treatment undefined.".into(),
                        explanation: "Contracts signed before the deadline whose service start date is after the deadline will flip borderline outcomes depending on recognition timing.".into(),
                        affected_path: "predicate.class_policy.signed_not_started_treatment".into(),
                        remediation: "Set `signed_not_started_treatment` to `exclude_until_start_date` or `include_on_signature`.".into(),
                    });
                }
            }
            ClassPolicy::Fundraising(fp) => {
                if fp.completion_event.is_none() {
                    out.push(Finding {
                        id: "TIME-002".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "Fundraising completion event undefined.".into(),
                        explanation: "\"Raise\" may refer to term-sheet signature, definitive agreement, public announcement, or actual receipt of funds in bank. A completion event must be explicitly selected.".into(),
                        affected_path: "predicate.class_policy.completion_event".into(),
                        remediation: "Set `completion_event` (e.g., `funds_received_in_bank` or `definitive_agreement_signed`).".into(),
                    });
                }
                match &fp.allowed_instruments {
                    None => out.push(Finding {
                        id: "M-FUND-001".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "Qualifying financing instruments undefined.".into(),
                        explanation: "The specification does not state whether equity, SAFEs, convertible notes, venture debt, or credit facilities count toward the round total.".into(),
                        affected_path: "predicate.class_policy.allowed_instruments".into(),
                        remediation: "Specify `allowed_instruments` (e.g., `[priced_equity, safe]`).".into(),
                    }),
                    Some(v) if v.is_empty() => out.push(Finding {
                        id: "M-FUND-001B".into(),
                        severity: Severity::Critical,
                        category: LintCategory::MetricCompleteness,
                        title: "Allowed financing instruments list is empty.".into(),
                        explanation: "An empty `allowed_instruments` list prevents any financing from ever qualifying.".into(),
                        affected_path: "predicate.class_policy.allowed_instruments".into(),
                        remediation: "Add at least one qualifying instrument to `allowed_instruments`.".into(),
                    }),
                    _ => {}
                }
                if fp.include_secondary.is_none() {
                    out.push(Finding {
                        id: "M-FUND-002".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "Primary vs secondary capital treatment undefined.".into(),
                        explanation: "A headline round total can mix primary treasury proceeds with secondary founder/investor share sales.".into(),
                        affected_path: "predicate.class_policy.include_secondary".into(),
                        remediation: "Set `include_secondary` to `false` (primary only) or `true`.".into(),
                    });
                }
                if fp.include_existing_note_conversion.is_none() {
                    out.push(Finding {
                        id: "M-FUND-003".into(),
                        severity: Severity::Medium,
                        category: LintCategory::MetricCompleteness,
                        title: "Existing convertible note/SAFE conversion treatment undefined.".into(),
                        explanation: "Previously raised notes converting into a priced round may be double-counted or excluded from new capital raised.".into(),
                        affected_path: "predicate.class_policy.include_existing_note_conversion".into(),
                        remediation: "Set `include_existing_note_conversion` explicitly.".into(),
                    });
                }
                if fp.include_insider_capital.is_none() {
                    out.push(Finding {
                        id: "M-FUND-004".into(),
                        severity: Severity::Medium,
                        category: LintCategory::MetricCompleteness,
                        title: "Insider/founder bridge financing treatment undefined.".into(),
                        explanation: "Capital contributed by existing insiders or founders may or may not count as an external raise.".into(),
                        affected_path: "predicate.class_policy.include_insider_capital".into(),
                        remediation: "Set `include_insider_capital` explicitly.".into(),
                    });
                }
                if fp.tranche_policy.is_none() {
                    out.push(Finding {
                        id: "M-FUND-005".into(),
                        severity: Severity::Medium,
                        category: LintCategory::MetricCompleteness,
                        title: "Multi-tranche financing policy undefined.".into(),
                        explanation: "Rounds wired in milestone-gated tranches create ambiguity between committed total and funded tranches.".into(),
                        affected_path: "predicate.class_policy.tranche_policy".into(),
                        remediation: "Set `tranche_policy` (e.g., `count_only_funded_tranches`).".into(),
                    });
                }
                if fp.conditional_commitment_policy.is_none() {
                    out.push(Finding {
                        id: "M-FUND-006".into(),
                        severity: Severity::Medium,
                        category: LintCategory::MetricCompleteness,
                        title: "Conditional commitment treatment undefined.".into(),
                        explanation: "Soft-circled or condition-precedent commitments that have not waived conditions can diverge from unconditional capital.".into(),
                        affected_path: "predicate.class_policy.conditional_commitment_policy".into(),
                        remediation: "Set `conditional_commitment_policy` to `exclude_conditional` or `include_if_conditions_met_by_deadline`.".into(),
                    });
                }
            }
            ClassPolicy::Employment(emp) => {
                if emp.target_employer.trim().is_empty() {
                    out.push(Finding {
                        id: "M-EMP-000".into(),
                        severity: Severity::Critical,
                        category: LintCategory::MetricCompleteness,
                        title: "Target employer is empty.".into(),
                        explanation:
                            "No target employer name is specified for the employment milestone."
                                .into(),
                        affected_path: "predicate.class_policy.target_employer".into(),
                        remediation: "Provide a non-empty `target_employer`.".into(),
                    });
                }
                if emp.qualifying_event.is_none() {
                    out.push(Finding {
                        id: "M-EMP-001".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "Employment qualifying event undefined.".into(),
                        explanation: "\"Join\" may mean signing an offer letter, reaching the official start date, appearing on payroll, or a public announcement.".into(),
                        affected_path: "predicate.class_policy.qualifying_event".into(),
                        remediation: "Set `qualifying_event` (e.g., `official_start_date` or `first_payroll_cycle`).".into(),
                    });
                }
                match &emp.allowed_relationships {
                    None => out.push(Finding {
                        id: "M-EMP-002".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "Qualifying employment relationship types undefined.".into(),
                        explanation: "It is unspecified whether contractors, consultants, advisors, or board members count as joining the employer.".into(),
                        affected_path: "predicate.class_policy.allowed_relationships".into(),
                        remediation: "Set `allowed_relationships` (e.g., `[full_time_employee]`).".into(),
                    }),
                    Some(v) if v.is_empty() => out.push(Finding {
                        id: "M-EMP-002B".into(),
                        severity: Severity::Critical,
                        category: LintCategory::MetricCompleteness,
                        title: "Allowed employment relationships list is empty.".into(),
                        explanation: "No relationship type can satisfy the employment predicate.".into(),
                        affected_path: "predicate.class_policy.allowed_relationships".into(),
                        remediation: "Include at least one qualifying relationship type.".into(),
                    }),
                    _ => {}
                }
                if emp.acquisition_treatment.is_none() {
                    out.push(Finding {
                        id: "M-EMP-003".into(),
                        severity: Severity::Medium,
                        category: LintCategory::MetricCompleteness,
                        title: "Acqui-hire / employer acquisition treatment undefined.".into(),
                        explanation: "If the target employer acquires the subject's current company, it is unclear whether transition without an independent offer counts.".into(),
                        affected_path: "predicate.class_policy.acquisition_treatment".into(),
                        remediation: "Set `acquisition_treatment` explicitly.".into(),
                    });
                }
                if emp.leave_of_absence_treatment.is_none() {
                    out.push(Finding {
                        id: "M-EMP-004".into(),
                        severity: Severity::Low,
                        category: LintCategory::MetricCompleteness,
                        title: "Leave of absence treatment undefined.".into(),
                        explanation: "It is unspecified whether an employee who starts on paper but is immediately on garden leave or LOA satisfies the rule.".into(),
                        affected_path: "predicate.class_policy.leave_of_absence_treatment".into(),
                        remediation: "Set `leave_of_absence_treatment` explicitly.".into(),
                    });
                }
            }
            ClassPolicy::Acquisition(acq) => {
                if acq.completion_event.is_none() {
                    out.push(Finding {
                        id: "M-ACQ-001".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "Acquisition completion event undefined.".into(),
                        explanation: "\"Be acquired\" may mean public announcement, definitive agreement signing, regulatory approval, or legal closing.".into(),
                        affected_path: "predicate.class_policy.completion_event".into(),
                        remediation: "Set `completion_event` (e.g., `legal_close`).".into(),
                    });
                }
                if acq.valuation_basis.is_none() {
                    out.push(Finding {
                        id: "M-ACQ-002".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "Acquisition valuation basis undefined.".into(),
                        explanation: "Enterprise value, equity value, and headline deal value can diverge materially due to cash, assumed debt, and working capital adjustments.".into(),
                        affected_path: "predicate.class_policy.valuation_basis".into(),
                        remediation: "Set `valuation_basis` (`enterprise_value`, `equity_value`, or `headline_consideration`).".into(),
                    });
                }
                if acq.earn_out_treatment.is_none() {
                    out.push(Finding {
                        id: "M-ACQ-003".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "Contingent earn-out treatment undefined.".into(),
                        explanation: "Deals with $80M upfront + $30M contingent earn-out resolve differently depending on whether earn-outs count toward the $100M threshold.".into(),
                        affected_path: "predicate.class_policy.earn_out_treatment".into(),
                        remediation: "Set `earn_out_treatment` (`exclude_contingent_earn_outs`, `include_maximum_earn_out`, or `include_fair_value_at_close`).".into(),
                    });
                }
                if acq.consideration_types.is_none() {
                    out.push(Finding {
                        id: "M-ACQ-004".into(),
                        severity: Severity::Medium,
                        category: LintCategory::MetricCompleteness,
                        title: "Qualifying consideration types undefined.".into(),
                        explanation: "It is unspecified whether illiquid private acquirer stock or assumed debt count alongside cash and public stock.".into(),
                        affected_path: "predicate.class_policy.consideration_types".into(),
                        remediation: "Set `consideration_types` explicitly.".into(),
                    });
                }
                if acq.transaction_structures.is_none() {
                    out.push(Finding {
                        id: "M-ACQ-005".into(),
                        severity: Severity::Medium,
                        category: LintCategory::MetricCompleteness,
                        title: "Qualifying transaction structures undefined.".into(),
                        explanation: "Asset sales, majority equity purchases, and whole-company mergers must be explicitly included or excluded.".into(),
                        affected_path: "predicate.class_policy.transaction_structures".into(),
                        remediation: "Set `transaction_structures` explicitly.".into(),
                    });
                }
            }
            ClassPolicy::Audience(aud) => {
                if aud
                    .platform
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
                {
                    out.push(Finding {
                        id: "M-AUD-001".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "Target audience platform undefined.".into(),
                        explanation:
                            "The platform (e.g., YouTube, X, Substack) is not explicitly specified."
                                .into(),
                        affected_path: "predicate.class_policy.platform".into(),
                        remediation: "Set `platform` explicitly.".into(),
                    });
                }
                if aud.crossing_rule.is_none() {
                    out.push(Finding {
                        id: "M-AUD-002".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "Audience crossing rule undefined.".into(),
                        explanation: "It is unspecified whether a momentary intraday spike above the threshold counts or whether the count must hold at the deadline snapshot.".into(),
                        affected_path: "predicate.class_policy.crossing_rule".into(),
                        remediation: "Set `crossing_rule` (`momentary_crossing`, `end_of_day_snapshot`, or `deadline_timestamp_snapshot`).".into(),
                    });
                }
                if aud.count_source_mode.is_none() {
                    out.push(Finding {
                        id: "M-AUD-003".into(),
                        severity: Severity::High,
                        category: LintCategory::MetricCompleteness,
                        title: "Displayed count vs API count mode undefined.".into(),
                        explanation: "YouTube and other platforms abbreviate public UI counts (e.g., '1.00M') differently from API/studio exact counts or third-party trackers.".into(),
                        affected_path: "predicate.class_policy.count_source_mode".into(),
                        remediation: "Set `count_source_mode` explicitly.".into(),
                    });
                }
                if aud.bot_purge_policy.is_none() {
                    out.push(Finding {
                        id: "M-AUD-004".into(),
                        severity: Severity::Medium,
                        category: LintCategory::MetricCompleteness,
                        title: "Bot purge and spam adjustment policy undefined.".into(),
                        explanation: "Platform spam purges shortly after crossing can drop subscriber counts back below threshold.".into(),
                        affected_path: "predicate.class_policy.bot_purge_policy".into(),
                        remediation: "Set `bot_purge_policy` explicitly.".into(),
                    });
                }
            }
        }

        out
    }
}

impl LintPass for ThresholdCompletenessPass {
    fn name(&self) -> &'static str {
        "ThresholdCompleteness"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        match &spec.predicate.threshold {
            Threshold::Money(m) => {
                if m.amount <= Decimal::ZERO {
                    out.push(Finding {
                        id: "THRESH-001".into(),
                        severity: Severity::Critical,
                        category: LintCategory::ThresholdCompleteness,
                        title: "Monetary threshold must be positive.".into(),
                        explanation: "A zero or negative monetary threshold produces a trivial or ill-posed market.".into(),
                        affected_path: "predicate.threshold.value.amount".into(),
                        remediation: "Provide a strictly positive monetary threshold.".into(),
                    });
                }
            }
            Threshold::Count(c) => {
                if *c == 0 {
                    out.push(Finding {
                        id: "THRESH-002".into(),
                        severity: Severity::Critical,
                        category: LintCategory::ThresholdCompleteness,
                        title: "Count threshold must be positive.".into(),
                        explanation: "A zero count threshold is trivially satisfied.".into(),
                        affected_path: "predicate.threshold.value".into(),
                        remediation: "Provide a strictly positive count threshold.".into(),
                    });
                }
            }
            Threshold::Boolean(_) => {}
        }
        out
    }
}

impl LintPass for UnitCompletenessPass {
    fn name(&self) -> &'static str {
        "UnitCompleteness"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        if let Threshold::Money(m) = &spec.predicate.threshold {
            match m.currency.as_deref().map(str::trim) {
                None | Some("") => {
                    out.push(Finding {
                        id: "UNIT-001".into(),
                        severity: Severity::Critical,
                        category: LintCategory::UnitCompleteness,
                        title: "Threshold currency code undefined.".into(),
                        explanation: "A monetary threshold without an ISO-4217 currency code (e.g., USD vs CAD vs EUR) cannot be resolved unambiguously.".into(),
                        affected_path: "predicate.threshold.value.currency".into(),
                        remediation: "Set an explicit ISO-4217 currency code on `predicate.threshold.value.currency`.".into(),
                    });
                }
                Some(code) if code.len() != 3 || !code.chars().all(|c| c.is_ascii_alphabetic()) => {
                    out.push(Finding {
                        id: "UNIT-002".into(),
                        severity: Severity::High,
                        category: LintCategory::UnitCompleteness,
                        title: "Non-standard currency code.".into(),
                        explanation: format!(
                            "Currency code `{}` is not a standard 3-letter ISO-4217 code.",
                            code
                        ),
                        affected_path: "predicate.threshold.value.currency".into(),
                        remediation:
                            "Use a standard 3-letter ISO-4217 code such as `USD` or `EUR`.".into(),
                    });
                }
                _ => {}
            }
        }
        out
    }
}

impl LintPass for CurrencyDeterminismPass {
    fn name(&self) -> &'static str {
        "CurrencyDeterminism"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        if !matches!(spec.predicate.threshold, Threshold::Money(_)) {
            return out;
        }

        match &spec.currency_policy {
            None => {
                out.push(Finding {
                    id: "FX-001".into(),
                    severity: Severity::High,
                    category: LintCategory::CurrencyDeterminism,
                    title: "FX methodology undefined.".into(),
                    explanation: "For a monetary milestone, non-USD revenue, financing, or deal consideration requires an explicit FX source and fixing timestamp rule.".into(),
                    affected_path: "currency_policy".into(),
                    remediation: "Define `currency_policy.fx_source`, `currency_policy.fx_timestamp_rule`, and `currency_policy.reporting_currency`.".into(),
                });
            }
            Some(cp) => {
                if cp.fx_source.is_none() {
                    out.push(Finding {
                        id: "FX-002".into(),
                        severity: Severity::High,
                        category: LintCategory::CurrencyDeterminism,
                        title: "FX reference source undefined.".into(),
                        explanation: "Without an explicit FX rate source (e.g., ECB daily reference or Federal Reserve H.10), cross-currency evidence near the threshold can resolve differently.".into(),
                        affected_path: "currency_policy.fx_source".into(),
                        remediation: "Set `currency_policy.fx_source` explicitly.".into(),
                    });
                }
                if cp.fx_timestamp_rule.is_none() {
                    out.push(Finding {
                        id: "FX-003".into(),
                        severity: Severity::High,
                        category: LintCategory::CurrencyDeterminism,
                        title: "FX fixing timestamp rule undefined.".into(),
                        explanation: "Spot rate at transaction date, period average, and deadline close can yield materially different converted totals.".into(),
                        affected_path: "currency_policy.fx_timestamp_rule".into(),
                        remediation: "Set `currency_policy.fx_timestamp_rule` explicitly.".into(),
                    });
                }
                if cp.reporting_currency.is_none() {
                    out.push(Finding {
                        id: "FX-004".into(),
                        severity: Severity::Medium,
                        category: LintCategory::CurrencyDeterminism,
                        title: "Reporting currency not explicitly restated in currency_policy.".into(),
                        explanation: "Setting `currency_policy.reporting_currency` avoids mismatch between threshold denomination and financial statement currency.".into(),
                        affected_path: "currency_policy.reporting_currency".into(),
                        remediation: "Set `currency_policy.reporting_currency` to match `predicate.threshold.value.currency`.".into(),
                    });
                }
            }
        }
        out
    }
}

impl LintPass for EvidenceDeterminismPass {
    fn name(&self) -> &'static str {
        "EvidenceDeterminism"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let ev = &spec.evidence_policy;

        if ev.primary_source.is_none() {
            out.push(Finding {
                id: "EVIDENCE-001".into(),
                severity: Severity::High,
                category: LintCategory::EvidenceDeterminism,
                title: "No authoritative evidence source configured.".into(),
                explanation: "Without a designated authoritative primary evidence source, resolvers must choose ad hoc between press releases, filings, and self-reports.".into(),
                affected_path: "evidence_policy.primary_source".into(),
                remediation: "Configure `evidence_policy.primary_source` with name, source_type, and verification_method.".into(),
            });
        } else if let Some(primary) = &ev.primary_source {
            if primary
                .verification_method
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                out.push(Finding {
                    id: "EVIDENCE-002".into(),
                    severity: Severity::Low,
                    category: LintCategory::EvidenceDeterminism,
                    title: "Primary evidence verification method undocumented.".into(),
                    explanation: "Documenting how the primary source is verified reduces operational ambiguity during resolution.".into(),
                    affected_path: "evidence_policy.primary_source.verification_method".into(),
                    remediation: "Populate `verification_method` on `evidence_policy.primary_source`.".into(),
                });
            }
        }

        out
    }
}

impl LintPass for SourceConflictDeterminismPass {
    fn name(&self) -> &'static str {
        "SourceConflictDeterminism"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let ev = &spec.evidence_policy;

        if ev.conflict_policy.is_none() {
            out.push(Finding {
                id: "CONFLICT-001".into(),
                severity: Severity::High,
                category: LintCategory::SourceConflictDeterminism,
                title: "Evidence conflict policy undefined.".into(),
                explanation: "When two evidence sources report conflicting values (e.g., press release says $6M while bank/filing shows $4.5M), no rule specifies which source wins or whether the market blocks.".into(),
                affected_path: "evidence_policy.conflict_policy".into(),
                remediation: "Set `evidence_policy.conflict_policy` and define `evidence_policy.precedence_order`.".into(),
            });
        }

        let needs_precedence = !ev.fallback_sources.is_empty()
            || matches!(
                ev.conflict_policy,
                Some(ConflictPolicy::UseHighestPrecedenceSource)
            );

        if needs_precedence && ev.precedence_order.is_empty() {
            out.push(Finding {
                id: "CONFLICT-002".into(),
                severity: Severity::High,
                category: LintCategory::SourceConflictDeterminism,
                title: "Source precedence hierarchy undefined.".into(),
                explanation: "Conflict policy or fallback sources require an explicit `precedence_order` ranking `SourceType` values.".into(),
                affected_path: "evidence_policy.precedence_order".into(),
                remediation: "Define `evidence_policy.precedence_order` from highest authority to lowest.".into(),
            });
        }

        out
    }
}

impl LintPass for RevisionDeterminismPass {
    fn name(&self) -> &'static str {
        "RevisionDeterminism"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let rev = &spec.revision_policy;

        if rev.restatement_handling.is_none() {
            out.push(Finding {
                id: "REVISION-001".into(),
                severity: Severity::High,
                category: LintCategory::RevisionDeterminism,
                title: "Restatement policy undefined.".into(),
                explanation: "Accounting figures, subscriber counts (bot purges), and announced financings can be restated after the initial observation date.".into(),
                affected_path: "revision_policy.restatement_handling".into(),
                remediation: "Set `revision_policy.restatement_handling` and define a cutoff (`corrections_until` or `restatement_window_days`).".into(),
            });
        }

        if rev.corrections_until.is_none() && rev.restatement_window_days.is_none() {
            out.push(Finding {
                id: "REVISION-002".into(),
                severity: Severity::Medium,
                category: LintCategory::RevisionDeterminism,
                title: "Restatement cutoff window undefined.".into(),
                explanation: "No explicit finalization date (`corrections_until`) or `restatement_window_days` bounds how long revisions can alter settlement.".into(),
                affected_path: "revision_policy.corrections_until".into(),
                remediation: "Specify `revision_policy.corrections_until` or `revision_policy.restatement_window_days`.".into(),
            });
        }

        out
    }
}

impl LintPass for IdentityDeterminismPass {
    fn name(&self) -> &'static str {
        "IdentityDeterminism"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();

        if spec.subject.name.trim().is_empty() {
            out.push(Finding {
                id: "ID-001".into(),
                severity: Severity::Critical,
                category: LintCategory::IdentityDeterminism,
                title: "Subject name is empty.".into(),
                explanation: "No subject entity or person name is defined.".into(),
                affected_path: "subject.name".into(),
                remediation: "Set `subject.name` to the canonical legal entity or person name."
                    .into(),
            });
        }

        if spec.subject.identifiers.is_empty() {
            out.push(Finding {
                id: "ID-002".into(),
                severity: Severity::Medium,
                category: LintCategory::IdentityDeterminism,
                title: "No disambiguating external subject identifiers provided.".into(),
                explanation: "Common names (e.g., 'Acme' or 'Alice') without domain, legal entity identifier, or channel ID risk entity confusion.".into(),
                affected_path: "subject.identifiers".into(),
                remediation: "Add canonical identifiers such as `domain`, `legal_name`, `sec_cik`, or `youtube_channel_id`.".into(),
            });
        }

        match &spec.predicate.class_policy {
            ClassPolicy::Arr(arr)
                if arr.subsidiary_scope.is_none() && spec.subject.entity_scope.is_none() =>
            {
                out.push(Finding {
                    id: "ID-003".into(),
                    severity: Severity::Medium,
                    category: LintCategory::IdentityDeterminism,
                    title: "Subsidiary consolidation scope undefined.".into(),
                    explanation: "It is unspecified whether parent-only ARR or consolidated group ARR (including subsidiaries) is measured.".into(),
                    affected_path: "predicate.class_policy.subsidiary_scope".into(),
                    remediation: "Set `subsidiary_scope` or `subject.entity_scope` explicitly.".into(),
                });
            }
            ClassPolicy::Employment(emp) if emp.target_employer_scope.is_none() => {
                out.push(Finding {
                    id: "ID-004".into(),
                    severity: Severity::Medium,
                    category: LintCategory::IdentityDeterminism,
                    title: "Target employer subsidiary scope undefined.".into(),
                    explanation: "Joining a remote subsidiary, research lab, or wholly owned affiliate may or may not count as joining the named employer.".into(),
                    affected_path: "predicate.class_policy.target_employer_scope".into(),
                    remediation: "Set `target_employer_scope` explicitly.".into(),
                });
            }
            ClassPolicy::Audience(aud) if aud.channel_scope.is_none() => {
                out.push(Finding {
                    id: "ID-005".into(),
                    severity: Severity::High,
                    category: LintCategory::IdentityDeterminism,
                    title: "Audience channel scope undefined.".into(),
                    explanation: "A creator may operate multiple channels (main, clips, shorts, regional) or rename/migrate a channel.".into(),
                    affected_path: "predicate.class_policy.channel_scope".into(),
                    remediation: "Set `channel_scope` (e.g., `single_named_channel_id`) and specify `subject.identifiers`.".into(),
                });
            }
            _ => {}
        }

        out
    }
}

impl LintPass for SubjectControlRiskPass {
    fn name(&self) -> &'static str {
        "SubjectControlRisk"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let ev = &spec.evidence_policy;

        if let Some(primary) = &ev.primary_source {
            let is_self_reported = matches!(
                primary.source_type,
                SourceType::CompanyAttestation | SourceType::PressRelease
            );
            let independent = primary.independent_of_subject.unwrap_or(!is_self_reported);

            let has_independent_corroboration = ev
                .fallback_sources
                .iter()
                .any(|s| s.independent_of_subject.unwrap_or(false))
                && ev.minimum_corroboration.unwrap_or(1) >= 2;

            if !independent && !has_independent_corroboration {
                out.push(Finding {
                    id: "EVIDENCE-004".into(),
                    severity: Severity::Critical,
                    category: LintCategory::SubjectControlRisk,
                    title: "Subject is sole authoritative evidence source.".into(),
                    explanation: "The person or company whose milestone is being traded can unilaterally determine whether the market resolves YES or NO through self-attestation or selective disclosure.".into(),
                    affected_path: "evidence_policy.primary_source".into(),
                    remediation: "Require an independent primary source (e.g., audited financials, regulatory filing, escrow confirmation, or third-party API) or multi-source independent corroboration.".into(),
                });
            }
        }

        out
    }
}

impl LintPass for OutcomeExhaustivenessPass {
    fn name(&self) -> &'static str {
        "OutcomeExhaustiveness"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let inv = &spec.invalidation_policy;

        if inv.yes_condition_summary.is_none() || inv.no_condition_summary.is_none() {
            out.push(Finding {
                id: "OUTCOME-001".into(),
                severity: Severity::Low,
                category: LintCategory::OutcomeExhaustiveness,
                title: "Explicit YES/NO outcome summaries missing.".into(),
                explanation: "Human-readable summaries of exact YES and NO satisfaction conditions help prevent disputes during charter review.".into(),
                affected_path: "invalidation_policy".into(),
                remediation: "Populate `invalidation_policy.yes_condition_summary` and `no_condition_summary`.".into(),
            });
        }

        if inv.invalid_triggers.is_empty() {
            out.push(Finding {
                id: "OUTCOME-002".into(),
                severity: Severity::Medium,
                category: LintCategory::OutcomeExhaustiveness,
                title: "Invalidation (INVALID) triggers undefined.".into(),
                explanation: "The specification does not enumerate conditions under which the market resolves INVALID instead of NO (e.g., discontinued metric or missing source past grace period).".into(),
                affected_path: "invalidation_policy.invalid_triggers".into(),
                remediation: "Specify explicit `invalid_triggers` in `invalidation_policy`.".into(),
            });
        }

        out
    }
}

impl LintPass for CancellationSemanticsPass {
    fn name(&self) -> &'static str {
        "CancellationSemantics"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();

        if spec.invalidation_policy.cancellation_policy.is_none() {
            out.push(Finding {
                id: "CANCEL-001".into(),
                severity: Severity::Medium,
                category: LintCategory::CancellationSemantics,
                title: "Subject dissolution or event cancellation policy undefined.".into(),
                explanation: "If the subject company shuts down, merges, or the platform discontinues the metric before the deadline, it is unclear whether the market resolves NO or INVALID.".into(),
                affected_path: "invalidation_policy.cancellation_policy".into(),
                remediation: "Set `invalidation_policy.cancellation_policy` explicitly.".into(),
            });
        }

        match &spec.predicate.class_policy {
            ClassPolicy::Fundraising(fp) if fp.unwind_policy.is_none() => {
                out.push(Finding {
                    id: "CANCEL-002".into(),
                    severity: Severity::Medium,
                    category: LintCategory::CancellationSemantics,
                    title: "Round unwind / investor withdrawal policy undefined.".into(),
                    explanation: "If a financing round closes or is announced and later unwinds or sees investor withdrawal, the outcome depends on `unwind_policy`.".into(),
                    affected_path: "predicate.class_policy.unwind_policy".into(),
                    remediation: "Set `predicate.class_policy.unwind_policy` explicitly.".into(),
                });
            }
            ClassPolicy::Employment(emp) if emp.announced_never_started_policy.is_none() => {
                out.push(Finding {
                    id: "CANCEL-003".into(),
                    severity: Severity::Medium,
                    category: LintCategory::CancellationSemantics,
                    title: "Announced-but-rescinded employment policy undefined.".into(),
                    explanation: "An employment hire can be publicly announced or signed and then rescinded prior to the start date.".into(),
                    affected_path: "predicate.class_policy.announced_never_started_policy".into(),
                    remediation: "Set `predicate.class_policy.announced_never_started_policy` explicitly.".into(),
                });
            }
            ClassPolicy::Acquisition(acq)
                if acq.termination_after_announcement_policy.is_none() =>
            {
                out.push(Finding {
                    id: "CANCEL-004".into(),
                    severity: Severity::High,
                    category: LintCategory::CancellationSemantics,
                    title: "Deal termination after announcement policy undefined.".into(),
                    explanation: "An acquisition announced before the deadline and terminated due to antitrust or financing failure creates conflicting YES/NO claims unless explicitly governed.".into(),
                    affected_path: "predicate.class_policy.termination_after_announcement_policy".into(),
                    remediation: "Set `predicate.class_policy.termination_after_announcement_policy` explicitly.".into(),
                });
            }
            ClassPolicy::Audience(aud) => {
                if aud.deleted_or_suspended_channel_policy.is_none() {
                    out.push(Finding {
                        id: "CANCEL-005".into(),
                        severity: Severity::Medium,
                        category: LintCategory::CancellationSemantics,
                        title: "Deleted or suspended channel policy undefined.".into(),
                        explanation: "If the creator's channel is deleted or suspended before the deadline, resolvers need an explicit rule (NO vs INVALID vs prior snapshot).".into(),
                        affected_path: "predicate.class_policy.deleted_or_suspended_channel_policy".into(),
                        remediation: "Set `predicate.class_policy.deleted_or_suspended_channel_policy` explicitly.".into(),
                    });
                }
                if aud.migrated_channel_policy.is_none() {
                    out.push(Finding {
                        id: "CANCEL-006".into(),
                        severity: Severity::Low,
                        category: LintCategory::CancellationSemantics,
                        title: "Channel rename or migration policy undefined.".into(),
                        explanation: "Channel rebrands or verified handle migrations can break naive channel-name tracking.".into(),
                        affected_path: "predicate.class_policy.migrated_channel_policy".into(),
                        remediation: "Set `predicate.class_policy.migrated_channel_policy` explicitly.".into(),
                    });
                }
            }
            _ => {}
        }

        out
    }
}

impl LintPass for EvidenceAvailabilityPass {
    fn name(&self) -> &'static str {
        "EvidenceAvailability"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();
        let ev = &spec.evidence_policy;

        if ev.availability_policy.is_none() {
            out.push(Finding {
                id: "AVAIL-001".into(),
                severity: Severity::Medium,
                category: LintCategory::EvidenceAvailability,
                title: "Evidence unavailability policy undefined.".into(),
                explanation: "If historical metrics or private company disclosures are unavailable at the observation deadline, no rule dictates fallback vs UNRESOLVED vs INVALID.".into(),
                affected_path: "evidence_policy.availability_policy".into(),
                remediation: "Set `evidence_policy.availability_policy` explicitly.".into(),
            });
        }

        if ev.primary_source.is_some() && ev.fallback_sources.is_empty() {
            out.push(Finding {
                id: "AVAIL-002".into(),
                severity: Severity::Low,
                category: LintCategory::EvidenceAvailability,
                title: "No fallback evidence sources configured.".into(),
                explanation: "Relying on a single primary source without fallback increases the chance of prolonged UNRESOLVED states if that source lags or goes offline.".into(),
                affected_path: "evidence_policy.fallback_sources".into(),
                remediation: "Add one or more secondary `fallback_sources` and order them in `precedence_order`.".into(),
            });
        }

        out
    }
}

impl LintPass for ObservationDeterminismPass {
    fn name(&self) -> &'static str {
        "ObservationDeterminism"
    }

    fn check(&self, spec: &MilestoneSpec) -> Vec<Finding> {
        let mut out = Vec::new();

        if spec.observation.observation_mode.is_none() {
            out.push(Finding {
                id: "OBS-001".into(),
                severity: Severity::High,
                category: LintCategory::ObservationDeterminism,
                title: "Observation mode (point-in-time vs anytime before deadline) undefined.".into(),
                explanation: "For metrics that can fluctuate (such as ARR or subscriber count), hitting the threshold in Q2 and dropping below by the Q4 deadline resolves YES under `anytime_on_or_before_deadline` but NO under `point_in_time_at_deadline`.".into(),
                affected_path: "observation.observation_mode".into(),
                remediation: "Set `observation.observation_mode` explicitly.".into(),
            });
        }

        if let (ClassPolicy::Audience(aud), Some(mode)) = (
            &spec.predicate.class_policy,
            spec.observation.observation_mode,
        ) {
            if let Some(crossing) = aud.crossing_rule {
                let conflict = matches!(
                    (mode, crossing),
                    (
                        ObservationMode::PointInTimeAtDeadline,
                        milestone_core::CrossingRule::MomentaryCrossing
                    ) | (
                        ObservationMode::AnytimeOnOrBeforeDeadline,
                        milestone_core::CrossingRule::DeadlineTimestampSnapshot
                    )
                );
                if conflict {
                    out.push(Finding {
                        id: "OBS-002".into(),
                        severity: Severity::Critical,
                        category: LintCategory::ObservationDeterminism,
                        title: "Observation mode contradicts audience crossing rule.".into(),
                        explanation: "`observation.observation_mode` and `predicate.class_policy.crossing_rule` express conflicting temporal semantics.".into(),
                        affected_path: "predicate.class_policy.crossing_rule".into(),
                        remediation: "Align `observation.observation_mode` and `crossing_rule`.".into(),
                    });
                }
            }
        }

        out
    }
}
