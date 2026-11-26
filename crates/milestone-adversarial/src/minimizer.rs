use crate::scenarios::AdversarialScenario;
use chrono::Duration;
use milestone_core::{
    AcquisitionCompletionEvent, AcquisitionEmploymentTreatment, AnnouncedNeverStartedPolicy,
    ArrMeasurementBasis, ClassPolicy, ConditionalCommitmentPolicy, ConflictPolicy,
    ConsiderationType, CrossingRule, CurrencyPolicy, DeadlineInclusivity, DeletedChannelPolicy,
    DiscountTreatment, EarnOutTreatment, EmploymentQualifyingEvent, EmploymentRelationship,
    EntityScope, FinancingInstrument, FundraisingCompletionEvent, FxSource, FxTimestampRule,
    LeaveTreatment, MilestoneSpec, ObservationMode, Outcome, RestatementHandling,
    SignedNotStartedTreatment, SourceType, TerminationPolicy, TransactionStructure, UnwindPolicy,
    ValuationBasis,
};
use milestone_resolver::resolve;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScenarioStatus {
    Deterministic,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DivergentInterpretation {
    pub policy_setting: String,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioAttackResult {
    pub scenario_id: String,
    pub name: String,
    pub description: String,
    pub status: ScenarioStatus,
    pub current_outcome: Outcome,
    pub reason: String,
    pub minimal_missing_policies: Vec<String>,
    pub divergent_interpretations: Vec<DivergentInterpretation>,
    pub minimal_clarification: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackReport {
    pub spec_id: String,
    pub title: String,
    pub total_scenarios: usize,
    pub deterministic_count: usize,
    pub ambiguous_count: usize,
    pub highest_risk_policy: Option<String>,
    pub policy_ambiguity_counts: BTreeMap<String, usize>,
    pub results: Vec<ScenarioAttackResult>,
}

/// Evaluates `scenario` against `spec` and minimizes the missing policy rule(s)
/// needed to eliminate ambiguity.
pub fn analyze_scenario(
    spec: &MilestoneSpec,
    scenario: &AdversarialScenario,
) -> ScenarioAttackResult {
    let base_res = resolve(spec, &scenario.bundle);
    let current_outcome = match &base_res {
        Ok(r) => r.outcome,
        Err(_) => Outcome::Unresolved,
    };

    // Check single-field candidate perturbations for any field currently None/unset in `spec`.
    let candidates = candidate_field_variants(spec, scenario);

    let mut divergent_field: Option<(String, Vec<DivergentInterpretation>)> = None;
    for (field_path, variants) in &candidates {
        let mut outcomes: Vec<DivergentInterpretation> = Vec::new();
        for (label, modified_spec, modified_bundle) in variants {
            if let Ok(r) = resolve(modified_spec, modified_bundle) {
                if r.outcome != Outcome::Unresolved {
                    outcomes.push(DivergentInterpretation {
                        policy_setting: label.clone(),
                        outcome: r.outcome,
                    });
                }
            }
        }
        let distinct_outcomes: std::collections::BTreeSet<Outcome> =
            outcomes.iter().map(|o| o.outcome).collect();
        if distinct_outcomes.len() >= 2 {
            divergent_field = Some((field_path.clone(), outcomes));
            break;
        }
    }

    if let Some((field_path, interpretations)) = divergent_field {
        let clarification = interpretations
            .iter()
            .find(|i| i.outcome == Outcome::No)
            .or_else(|| interpretations.first())
            .map(|i| i.policy_setting.clone());

        return ScenarioAttackResult {
            scenario_id: scenario.id.clone(),
            name: scenario.name.clone(),
            description: scenario.description.clone(),
            status: ScenarioStatus::Ambiguous,
            current_outcome,
            reason: format!(
                "Specification leaves `{}` undefined, allowing reasonable interpretations to diverge.",
                field_path
            ),
            minimal_missing_policies: vec![field_path],
            divergent_interpretations: interpretations,
            minimal_clarification: clarification,
        };
    }

    // If current_outcome is still Unresolved (e.g., requires a pair of fields like conflict_policy + precedence_order),
    // search minimal single or pair fixes that transition UNRESOLVED -> decisive outcome.
    if current_outcome == Outcome::Unresolved {
        // Check if conflict_policy + precedence_order fixes it
        if spec.evidence_policy.conflict_policy.is_none()
            || spec.evidence_policy.precedence_order.is_empty()
        {
            let mut test_a = spec.clone();
            test_a.evidence_policy.conflict_policy =
                Some(ConflictPolicy::UseHighestPrecedenceSource);
            test_a.evidence_policy.precedence_order = vec![
                SourceType::AuditedFinancials,
                SourceType::SecOrRegulatoryFiling,
                SourceType::BankOrEscrowConfirmation,
                SourceType::OfficialPlatformApi,
                SourceType::CapTableRegistry,
                SourceType::CompanyAttestation,
                SourceType::PressRelease,
                SourceType::ThirdPartyMedia,
            ];

            let mut test_b = spec.clone();
            test_b.evidence_policy.conflict_policy =
                Some(ConflictPolicy::UseHighestPrecedenceSource);
            test_b.evidence_policy.precedence_order = vec![
                SourceType::PressRelease,
                SourceType::ThirdPartyMedia,
                SourceType::CompanyAttestation,
                SourceType::BankOrEscrowConfirmation,
                SourceType::SecOrRegulatoryFiling,
                SourceType::AuditedFinancials,
                SourceType::OfficialPlatformApi,
            ];

            let ra = resolve(&test_a, &scenario.bundle)
                .map(|r| r.outcome)
                .unwrap_or(Outcome::Unresolved);
            let rb = resolve(&test_b, &scenario.bundle)
                .map(|r| r.outcome)
                .unwrap_or(Outcome::Unresolved);
            let mut missing = Vec::new();
            if spec.evidence_policy.conflict_policy.is_none() {
                missing.push("evidence_policy.conflict_policy".to_string());
            }
            if spec.evidence_policy.precedence_order.is_empty() {
                missing.push("evidence_policy.precedence_order".to_string());
            }
            let mut interps = Vec::new();
            if ra != Outcome::Unresolved {
                interps.push(DivergentInterpretation {
                    policy_setting:
                        "conflict_policy = use_highest_precedence_source (audited/official first)"
                            .into(),
                    outcome: ra,
                });
            }
            if rb != Outcome::Unresolved && rb != ra {
                interps.push(DivergentInterpretation {
                    policy_setting:
                        "conflict_policy = use_highest_precedence_source (media/PR first)".into(),
                    outcome: rb,
                });
            }
            return ScenarioAttackResult {
                scenario_id: scenario.id.clone(),
                name: scenario.name.clone(),
                description: scenario.description.clone(),
                status: ScenarioStatus::Ambiguous,
                current_outcome,
                reason: "Conflicting or unresolved evidence cannot be settled without explicit conflict and precedence rules.".into(),
                minimal_missing_policies: if missing.is_empty() {
                    vec![scenario.target_policy_hint.clone()]
                } else {
                    missing
                },
                divergent_interpretations: interps,
                minimal_clarification: Some(
                    "conflict_policy = use_highest_precedence_source with explicit precedence_order".into(),
                ),
            };
        }

        // Check currency_policy pair (fx_source + fx_timestamp_rule)
        let missing_fx = spec
            .currency_policy
            .as_ref()
            .map(|c| c.fx_source.is_none() || c.fx_timestamp_rule.is_none())
            .unwrap_or(true);
        if missing_fx && scenario.target_policy_hint.starts_with("currency_policy") {
            let mut test = spec.clone();
            test.currency_policy = Some(CurrencyPolicy {
                reporting_currency: Some("USD".into()),
                fx_source: Some(FxSource::EcbDailyReference),
                fx_timestamp_rule: Some(FxTimestampRule::DeadlineClose),
                allow_non_reporting_currency: Some(true),
            });
            let r_ecb = resolve(&test, &scenario.bundle)
                .map(|r| r.outcome)
                .unwrap_or(Outcome::Unresolved);
            return ScenarioAttackResult {
                scenario_id: scenario.id.clone(),
                name: scenario.name.clone(),
                description: scenario.description.clone(),
                status: ScenarioStatus::Ambiguous,
                current_outcome,
                reason: "Non-reporting currency evidence requires explicit FX source and fixing timestamp.".into(),
                minimal_missing_policies: vec![
                    "currency_policy.fx_source".into(),
                    "currency_policy.fx_timestamp_rule".into(),
                ],
                divergent_interpretations: if r_ecb != Outcome::Unresolved {
                    vec![DivergentInterpretation {
                        policy_setting: "fx_source = ecb_daily_reference, fx_timestamp_rule = deadline_close".into(),
                        outcome: r_ecb,
                    }]
                } else {
                    vec![]
                },
                minimal_clarification: Some("currency_policy = { fx_source: ecb_daily_reference, fx_timestamp_rule: deadline_close }".into()),
            };
        }

        // Single-field missing policy that yields a single decisive outcome when filled
        for (field_path, variants) in &candidates {
            if let Some((label, modified_spec, modified_bundle)) = variants.first() {
                if let Ok(r) = resolve(modified_spec, modified_bundle) {
                    if r.outcome != Outcome::Unresolved {
                        return ScenarioAttackResult {
                            scenario_id: scenario.id.clone(),
                            name: scenario.name.clone(),
                            description: scenario.description.clone(),
                            status: ScenarioStatus::Ambiguous,
                            current_outcome,
                            reason: base_res
                                .as_ref()
                                .map(|b| b.explanation.clone())
                                .unwrap_or_else(|e| e.to_string()),
                            minimal_missing_policies: vec![field_path.clone()],
                            divergent_interpretations: vec![DivergentInterpretation {
                                policy_setting: label.clone(),
                                outcome: r.outcome,
                            }],
                            minimal_clarification: Some(label.clone()),
                        };
                    }
                }
            }
        }

        return ScenarioAttackResult {
            scenario_id: scenario.id.clone(),
            name: scenario.name.clone(),
            description: scenario.description.clone(),
            status: ScenarioStatus::Ambiguous,
            current_outcome,
            reason: base_res
                .map(|r| r.explanation)
                .unwrap_or_else(|e| e.to_string()),
            minimal_missing_policies: vec![scenario.target_policy_hint.clone()],
            divergent_interpretations: vec![],
            minimal_clarification: Some(format!("Define `{}`", scenario.target_policy_hint)),
        };
    }

    // Deterministic!
    let explanation = base_res
        .map(|r| r.explanation)
        .unwrap_or_else(|_| "Deterministic".into());
    ScenarioAttackResult {
        scenario_id: scenario.id.clone(),
        name: scenario.name.clone(),
        description: scenario.description.clone(),
        status: ScenarioStatus::Deterministic,
        current_outcome,
        reason: explanation,
        minimal_missing_policies: vec![],
        divergent_interpretations: vec![],
        minimal_clarification: None,
    }
}

type Variant = (String, MilestoneSpec, milestone_core::EvidenceBundle);

fn candidate_field_variants(
    spec: &MilestoneSpec,
    scenario: &AdversarialScenario,
) -> Vec<(String, Vec<Variant>)> {
    let mut fields: Vec<(String, Vec<Variant>)> = Vec::new();
    let bundle = &scenario.bundle;

    // 1. Timezone ambiguity when observation.timezone is None and scenario models local midnight
    if spec.observation.timezone.is_none() && scenario.timezone_ambiguity_note.is_some() {
        let mut s_utc = spec.clone();
        s_utc.observation.timezone = Some("UTC".into());
        match &mut s_utc.predicate.class_policy {
            ClassPolicy::Fundraising(f) => {
                f.completion_event
                    .get_or_insert(FundraisingCompletionEvent::LegalCloseAndFundsReceived);
                f.allowed_instruments
                    .get_or_insert(vec![FinancingInstrument::PricedEquity]);
                f.include_secondary.get_or_insert(false);
            }
            ClassPolicy::Arr(a) => {
                a.measurement_basis
                    .get_or_insert(ArrMeasurementBasis::ContractedRecurringRevenue);
            }
            ClassPolicy::Employment(e) => {
                e.qualifying_event
                    .get_or_insert(EmploymentQualifyingEvent::OfficialStartDate);
                e.allowed_relationships
                    .get_or_insert(vec![EmploymentRelationship::FullTimeEmployee]);
            }
            ClassPolicy::Acquisition(a) => {
                a.completion_event
                    .get_or_insert(AcquisitionCompletionEvent::LegalClose);
                a.valuation_basis
                    .get_or_insert(ValuationBasis::EnterpriseValue);
                a.consideration_types
                    .get_or_insert(vec![ConsiderationType::Cash]);
                a.transaction_structures
                    .get_or_insert(vec![TransactionStructure::WholeCompanyMergerOrAcquisition]);
            }
            ClassPolicy::Audience(a) => {
                a.crossing_rule
                    .get_or_insert(CrossingRule::MomentaryCrossing);
            }
        }
        let mut s_pt = s_utc.clone();
        s_pt.observation.timezone = Some("America/Los_Angeles".into());
        if let Some(d) = s_pt.observation.deadline {
            s_pt.observation.deadline = Some(d + Duration::hours(8));
        }
        fields.push((
            "observation.timezone".into(),
            vec![
                ("timezone = UTC".into(), s_utc, bundle.clone()),
                (
                    "timezone = America/Los_Angeles".into(),
                    s_pt,
                    bundle.clone(),
                ),
            ],
        ));
    }

    // 2. Deadline inclusivity
    if spec.observation.deadline_inclusivity.is_none() {
        let mut s_inc = spec.clone();
        s_inc.observation.deadline_inclusivity = Some(DeadlineInclusivity::Inclusive);
        let mut s_exc = spec.clone();
        s_exc.observation.deadline_inclusivity = Some(DeadlineInclusivity::Exclusive);
        fields.push((
            "observation.deadline_inclusivity".into(),
            vec![
                (
                    "deadline_inclusivity = inclusive".into(),
                    s_inc,
                    bundle.clone(),
                ),
                (
                    "deadline_inclusivity = exclusive".into(),
                    s_exc,
                    bundle.clone(),
                ),
            ],
        ));
    }

    // 3. Observation mode
    if spec.observation.observation_mode.is_none() {
        let mut s_any = spec.clone();
        s_any.observation.observation_mode = Some(ObservationMode::AnytimeOnOrBeforeDeadline);
        let mut s_pit = spec.clone();
        s_pit.observation.observation_mode = Some(ObservationMode::PointInTimeAtDeadline);
        fields.push((
            "observation.observation_mode".into(),
            vec![
                (
                    "observation_mode = anytime_on_or_before_deadline".into(),
                    s_any,
                    bundle.clone(),
                ),
                (
                    "observation_mode = point_in_time_at_deadline".into(),
                    s_pit,
                    bundle.clone(),
                ),
            ],
        ));
    }

    // 4. Revision / restatement handling
    if spec.revision_policy.restatement_handling.is_none() {
        let mut s_apply = spec.clone();
        s_apply.revision_policy.restatement_handling =
            Some(RestatementHandling::ApplyRestatementsWithinWindow);
        s_apply.revision_policy.restatement_window_days = Some(30);
        let mut s_ignore = spec.clone();
        s_ignore.revision_policy.restatement_handling =
            Some(RestatementHandling::IgnorePostDeadlineRestatements);
        fields.push((
            "revision_policy.restatement_handling".into(),
            vec![
                (
                    "restatement_handling = apply_restatements_within_window".into(),
                    s_apply,
                    bundle.clone(),
                ),
                (
                    "restatement_handling = ignore_post_deadline_restatements".into(),
                    s_ignore,
                    bundle.clone(),
                ),
            ],
        ));
    }

    // 5. Class-specific fields
    match &spec.predicate.class_policy {
        ClassPolicy::Fundraising(fp) => {
            if fp.completion_event.is_none() {
                let mut v = Vec::new();
                for (lbl, ev) in [
                    (
                        "completion_event = term_sheet_signed",
                        FundraisingCompletionEvent::TermSheetSigned,
                    ),
                    (
                        "completion_event = funds_received_in_bank",
                        FundraisingCompletionEvent::FundsReceivedInBank,
                    ),
                    (
                        "completion_event = definitive_agreement_signed",
                        FundraisingCompletionEvent::DefinitiveAgreementSigned,
                    ),
                    (
                        "completion_event = legal_close_and_funds_received",
                        FundraisingCompletionEvent::LegalCloseAndFundsReceived,
                    ),
                ] {
                    let mut s = spec.clone();
                    if let ClassPolicy::Fundraising(ref mut f) = s.predicate.class_policy {
                        f.completion_event = Some(ev);
                        // Ensure missing secondary/instruments doesn't mask completion_event in single-field test
                        if f.allowed_instruments.is_none() {
                            f.allowed_instruments = Some(vec![
                                FinancingInstrument::PricedEquity,
                                FinancingInstrument::Safe,
                            ]);
                        }
                        if f.include_secondary.is_none() {
                            f.include_secondary = Some(false);
                        }
                    }
                    v.push((lbl.into(), s, bundle.clone()));
                }
                fields.push(("predicate.class_policy.completion_event".into(), v));
            }
            if fp.allowed_instruments.is_none() {
                let mut s_eq = spec.clone();
                if let ClassPolicy::Fundraising(ref mut f) = s_eq.predicate.class_policy {
                    f.allowed_instruments = Some(vec![FinancingInstrument::PricedEquity]);
                    if f.completion_event.is_none() {
                        f.completion_event =
                            Some(FundraisingCompletionEvent::LegalCloseAndFundsReceived);
                    }
                }
                let mut s_debt = spec.clone();
                if let ClassPolicy::Fundraising(ref mut f) = s_debt.predicate.class_policy {
                    f.allowed_instruments = Some(vec![
                        FinancingInstrument::PricedEquity,
                        FinancingInstrument::Safe,
                        FinancingInstrument::VentureDebt,
                    ]);
                    if f.completion_event.is_none() {
                        f.completion_event =
                            Some(FundraisingCompletionEvent::LegalCloseAndFundsReceived);
                    }
                }
                fields.push((
                    "predicate.class_policy.allowed_instruments".into(),
                    vec![
                        (
                            "allowed_instruments = [priced_equity]".into(),
                            s_eq,
                            bundle.clone(),
                        ),
                        (
                            "allowed_instruments = [priced_equity, safe, venture_debt]".into(),
                            s_debt,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
            if fp.include_secondary.is_none() {
                let mut s_no = spec.clone();
                if let ClassPolicy::Fundraising(ref mut f) = s_no.predicate.class_policy {
                    f.include_secondary = Some(false);
                    if f.completion_event.is_none() {
                        f.completion_event =
                            Some(FundraisingCompletionEvent::LegalCloseAndFundsReceived);
                    }
                    if f.allowed_instruments.is_none() {
                        f.allowed_instruments = Some(vec![FinancingInstrument::PricedEquity]);
                    }
                }
                let mut s_yes = s_no.clone();
                if let ClassPolicy::Fundraising(ref mut f) = s_yes.predicate.class_policy {
                    f.include_secondary = Some(true);
                }
                fields.push((
                    "predicate.class_policy.include_secondary".into(),
                    vec![
                        ("include_secondary = false".into(), s_no, bundle.clone()),
                        ("include_secondary = true".into(), s_yes, bundle.clone()),
                    ],
                ));
            }
            if fp.include_existing_note_conversion.is_none() {
                let mut s_no = spec.clone();
                if let ClassPolicy::Fundraising(ref mut f) = s_no.predicate.class_policy {
                    f.include_existing_note_conversion = Some(false);
                    if f.completion_event.is_none() {
                        f.completion_event =
                            Some(FundraisingCompletionEvent::LegalCloseAndFundsReceived);
                    }
                    if f.allowed_instruments.is_none() {
                        f.allowed_instruments = Some(vec![FinancingInstrument::PricedEquity]);
                    }
                    if f.include_secondary.is_none() {
                        f.include_secondary = Some(false);
                    }
                }
                let mut s_yes = s_no.clone();
                if let ClassPolicy::Fundraising(ref mut f) = s_yes.predicate.class_policy {
                    f.include_existing_note_conversion = Some(true);
                }
                fields.push((
                    "predicate.class_policy.include_existing_note_conversion".into(),
                    vec![
                        (
                            "include_existing_note_conversion = false".into(),
                            s_no,
                            bundle.clone(),
                        ),
                        (
                            "include_existing_note_conversion = true".into(),
                            s_yes,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
            if fp.conditional_commitment_policy.is_none() {
                let mut s_ex = spec.clone();
                if let ClassPolicy::Fundraising(ref mut f) = s_ex.predicate.class_policy {
                    f.conditional_commitment_policy =
                        Some(ConditionalCommitmentPolicy::ExcludeConditional);
                    if f.completion_event.is_none() {
                        f.completion_event =
                            Some(FundraisingCompletionEvent::LegalCloseAndFundsReceived);
                    }
                    if f.allowed_instruments.is_none() {
                        f.allowed_instruments = Some(vec![FinancingInstrument::PricedEquity]);
                    }
                    if f.include_secondary.is_none() {
                        f.include_secondary = Some(false);
                    }
                }
                let mut s_in = s_ex.clone();
                if let ClassPolicy::Fundraising(ref mut f) = s_in.predicate.class_policy {
                    f.conditional_commitment_policy =
                        Some(ConditionalCommitmentPolicy::IncludeIfConditionsMetByDeadline);
                }
                fields.push((
                    "predicate.class_policy.conditional_commitment_policy".into(),
                    vec![
                        (
                            "conditional_commitment_policy = exclude_conditional".into(),
                            s_ex,
                            bundle.clone(),
                        ),
                        (
                            "conditional_commitment_policy = include_if_conditions_met_by_deadline"
                                .into(),
                            s_in,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
            if fp.unwind_policy.is_none() {
                let mut s_rem = spec.clone();
                if let ClassPolicy::Fundraising(ref mut f) = s_rem.predicate.class_policy {
                    f.unwind_policy = Some(UnwindPolicy::MustRemainClosedThroughFinalization);
                    if f.completion_event.is_none() {
                        f.completion_event =
                            Some(FundraisingCompletionEvent::LegalCloseAndFundsReceived);
                    }
                    if f.allowed_instruments.is_none() {
                        f.allowed_instruments = Some(vec![FinancingInstrument::PricedEquity]);
                    }
                    if f.include_secondary.is_none() {
                        f.include_secondary = Some(false);
                    }
                }
                let mut s_snap = s_rem.clone();
                if let ClassPolicy::Fundraising(ref mut f) = s_snap.predicate.class_policy {
                    f.unwind_policy = Some(UnwindPolicy::SnapshotAtCloseIgnoresLaterUnwind);
                }
                fields.push((
                    "predicate.class_policy.unwind_policy".into(),
                    vec![
                        (
                            "unwind_policy = must_remain_closed_through_finalization".into(),
                            s_rem,
                            bundle.clone(),
                        ),
                        (
                            "unwind_policy = snapshot_at_close_ignores_later_unwind".into(),
                            s_snap,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
        }
        ClassPolicy::Arr(arr) => {
            if arr.measurement_basis.is_none() {
                let mut s_c = spec.clone();
                if let ClassPolicy::Arr(ref mut a) = s_c.predicate.class_policy {
                    a.measurement_basis = Some(ArrMeasurementBasis::ContractedRecurringRevenue);
                    a.include_services_revenue.get_or_insert(false);
                    a.include_one_time_fees.get_or_insert(false);
                    a.discount_treatment
                        .get_or_insert(DiscountTreatment::NetOfDiscounts);
                }
                let mut s_m = s_c.clone();
                if let ClassPolicy::Arr(ref mut a) = s_m.predicate.class_policy {
                    a.measurement_basis = Some(ArrMeasurementBasis::AnnualizedMrr);
                }
                fields.push((
                    "predicate.class_policy.measurement_basis".into(),
                    vec![
                        (
                            "measurement_basis = contracted_recurring_revenue".into(),
                            s_c,
                            bundle.clone(),
                        ),
                        (
                            "measurement_basis = annualized_mrr".into(),
                            s_m,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
            if arr.include_services_revenue.is_none() {
                let mut s_no = spec.clone();
                if let ClassPolicy::Arr(ref mut a) = s_no.predicate.class_policy {
                    a.measurement_basis
                        .get_or_insert(ArrMeasurementBasis::ContractedRecurringRevenue);
                    a.include_services_revenue = Some(false);
                }
                let mut s_yes = s_no.clone();
                if let ClassPolicy::Arr(ref mut a) = s_yes.predicate.class_policy {
                    a.include_services_revenue = Some(true);
                }
                fields.push((
                    "predicate.class_policy.include_services_revenue".into(),
                    vec![
                        (
                            "include_services_revenue = false".into(),
                            s_no,
                            bundle.clone(),
                        ),
                        (
                            "include_services_revenue = true".into(),
                            s_yes,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
            if arr.include_one_time_fees.is_none() {
                let mut s_no = spec.clone();
                if let ClassPolicy::Arr(ref mut a) = s_no.predicate.class_policy {
                    a.measurement_basis
                        .get_or_insert(ArrMeasurementBasis::ContractedRecurringRevenue);
                    a.include_one_time_fees = Some(false);
                }
                let mut s_yes = s_no.clone();
                if let ClassPolicy::Arr(ref mut a) = s_yes.predicate.class_policy {
                    a.include_one_time_fees = Some(true);
                }
                fields.push((
                    "predicate.class_policy.include_one_time_fees".into(),
                    vec![
                        ("include_one_time_fees = false".into(), s_no, bundle.clone()),
                        ("include_one_time_fees = true".into(), s_yes, bundle.clone()),
                    ],
                ));
            }
            if arr.signed_not_started_treatment.is_none() {
                let mut s_ex = spec.clone();
                if let ClassPolicy::Arr(ref mut a) = s_ex.predicate.class_policy {
                    a.measurement_basis
                        .get_or_insert(ArrMeasurementBasis::ContractedRecurringRevenue);
                    a.signed_not_started_treatment =
                        Some(SignedNotStartedTreatment::ExcludeUntilStartDate);
                }
                let mut s_in = s_ex.clone();
                if let ClassPolicy::Arr(ref mut a) = s_in.predicate.class_policy {
                    a.signed_not_started_treatment =
                        Some(SignedNotStartedTreatment::IncludeOnSignature);
                }
                fields.push((
                    "predicate.class_policy.signed_not_started_treatment".into(),
                    vec![
                        (
                            "signed_not_started_treatment = exclude_until_start_date".into(),
                            s_ex,
                            bundle.clone(),
                        ),
                        (
                            "signed_not_started_treatment = include_on_signature".into(),
                            s_in,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
            if arr.discount_treatment.is_none() {
                let mut s_net = spec.clone();
                if let ClassPolicy::Arr(ref mut a) = s_net.predicate.class_policy {
                    a.measurement_basis
                        .get_or_insert(ArrMeasurementBasis::ContractedRecurringRevenue);
                    a.discount_treatment = Some(DiscountTreatment::NetOfDiscounts);
                }
                let mut s_gross = s_net.clone();
                if let ClassPolicy::Arr(ref mut a) = s_gross.predicate.class_policy {
                    a.discount_treatment = Some(DiscountTreatment::GrossBeforeDiscounts);
                }
                fields.push((
                    "predicate.class_policy.discount_treatment".into(),
                    vec![
                        (
                            "discount_treatment = net_of_discounts".into(),
                            s_net,
                            bundle.clone(),
                        ),
                        (
                            "discount_treatment = gross_before_discounts".into(),
                            s_gross,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
            if arr.subsidiary_scope.is_none() && spec.subject.entity_scope.is_none() {
                let mut s_p = spec.clone();
                if let ClassPolicy::Arr(ref mut a) = s_p.predicate.class_policy {
                    a.measurement_basis
                        .get_or_insert(ArrMeasurementBasis::ContractedRecurringRevenue);
                    a.subsidiary_scope = Some(EntityScope::ParentEntityOnly);
                }
                let mut s_c = s_p.clone();
                if let ClassPolicy::Arr(ref mut a) = s_c.predicate.class_policy {
                    a.subsidiary_scope = Some(EntityScope::ConsolidatedGroupWithSubsidiaries);
                }
                fields.push((
                    "predicate.class_policy.subsidiary_scope".into(),
                    vec![
                        (
                            "subsidiary_scope = parent_entity_only".into(),
                            s_p,
                            bundle.clone(),
                        ),
                        (
                            "subsidiary_scope = consolidated_group_with_subsidiaries".into(),
                            s_c,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
        }
        ClassPolicy::Employment(emp) => {
            if emp.qualifying_event.is_none() {
                let mut s_o = spec.clone();
                if let ClassPolicy::Employment(ref mut e) = s_o.predicate.class_policy {
                    e.qualifying_event = Some(EmploymentQualifyingEvent::SignedOfferLetter);
                    e.allowed_relationships
                        .get_or_insert(vec![EmploymentRelationship::FullTimeEmployee]);
                }
                let mut s_s = s_o.clone();
                if let ClassPolicy::Employment(ref mut e) = s_s.predicate.class_policy {
                    e.qualifying_event = Some(EmploymentQualifyingEvent::OfficialStartDate);
                }
                fields.push((
                    "predicate.class_policy.qualifying_event".into(),
                    vec![
                        (
                            "qualifying_event = signed_offer_letter".into(),
                            s_o,
                            bundle.clone(),
                        ),
                        (
                            "qualifying_event = official_start_date".into(),
                            s_s,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
            if emp.allowed_relationships.is_none() {
                let mut s_fte = spec.clone();
                if let ClassPolicy::Employment(ref mut e) = s_fte.predicate.class_policy {
                    e.qualifying_event
                        .get_or_insert(EmploymentQualifyingEvent::OfficialStartDate);
                    e.allowed_relationships = Some(vec![EmploymentRelationship::FullTimeEmployee]);
                }
                let mut s_all = s_fte.clone();
                if let ClassPolicy::Employment(ref mut e) = s_all.predicate.class_policy {
                    e.allowed_relationships = Some(vec![
                        EmploymentRelationship::FullTimeEmployee,
                        EmploymentRelationship::Contractor,
                        EmploymentRelationship::Consultant,
                        EmploymentRelationship::Advisor,
                    ]);
                }
                fields.push((
                    "predicate.class_policy.allowed_relationships".into(),
                    vec![
                        ("allowed_relationships = [full_time_employee]".into(), s_fte, bundle.clone()),
                        ("allowed_relationships = [full_time_employee, contractor, consultant, advisor]".into(), s_all, bundle.clone()),
                    ],
                ));
            }
            if emp.acquisition_treatment.is_none() {
                let mut s_no = spec.clone();
                if let ClassPolicy::Employment(ref mut e) = s_no.predicate.class_policy {
                    e.qualifying_event
                        .get_or_insert(EmploymentQualifyingEvent::OfficialStartDate);
                    e.allowed_relationships
                        .get_or_insert(vec![EmploymentRelationship::FullTimeEmployee]);
                    e.acquisition_treatment = Some(
                        AcquisitionEmploymentTreatment::DoesNotCountUnlessNewEmploymentAgreement,
                    );
                }
                let mut s_yes = s_no.clone();
                if let ClassPolicy::Employment(ref mut e) = s_yes.predicate.class_policy {
                    e.acquisition_treatment = Some(AcquisitionEmploymentTreatment::CountsAsJoining);
                }
                fields.push((
                    "predicate.class_policy.acquisition_treatment".into(),
                    vec![
                        ("acquisition_treatment = does_not_count_unless_new_employment_agreement".into(), s_no, bundle.clone()),
                        ("acquisition_treatment = counts_as_joining".into(), s_yes, bundle.clone()),
                    ],
                ));
            }
            if emp.announced_never_started_policy.is_none() {
                let mut s_no = spec.clone();
                if let ClassPolicy::Employment(ref mut e) = s_no.predicate.class_policy {
                    e.qualifying_event
                        .get_or_insert(EmploymentQualifyingEvent::OfficialStartDate);
                    e.allowed_relationships
                        .get_or_insert(vec![EmploymentRelationship::FullTimeEmployee]);
                    e.announced_never_started_policy =
                        Some(AnnouncedNeverStartedPolicy::DoesNotResolveYesWithoutActualStart);
                }
                let mut s_yes = s_no.clone();
                if let ClassPolicy::Employment(ref mut e) = s_yes.predicate.class_policy {
                    e.announced_never_started_policy =
                        Some(AnnouncedNeverStartedPolicy::ResolvesYesOnAnnouncement);
                }
                fields.push((
                    "predicate.class_policy.announced_never_started_policy".into(),
                    vec![
                        ("announced_never_started_policy = does_not_resolve_yes_without_actual_start".into(), s_no, bundle.clone()),
                        ("announced_never_started_policy = resolves_yes_on_announcement".into(), s_yes, bundle.clone()),
                    ],
                ));
            }
            if emp.leave_of_absence_treatment.is_none() {
                let mut s_act = spec.clone();
                if let ClassPolicy::Employment(ref mut e) = s_act.predicate.class_policy {
                    e.qualifying_event
                        .get_or_insert(EmploymentQualifyingEvent::OfficialStartDate);
                    e.allowed_relationships
                        .get_or_insert(vec![EmploymentRelationship::FullTimeEmployee]);
                    e.leave_of_absence_treatment = Some(LeaveTreatment::RequiresActiveWorkingDay);
                }
                let mut s_ros = s_act.clone();
                if let ClassPolicy::Employment(ref mut e) = s_ros.predicate.class_policy {
                    e.leave_of_absence_treatment = Some(LeaveTreatment::CountsIfActiveOnRoster);
                }
                fields.push((
                    "predicate.class_policy.leave_of_absence_treatment".into(),
                    vec![
                        (
                            "leave_of_absence_treatment = requires_active_working_day".into(),
                            s_act,
                            bundle.clone(),
                        ),
                        (
                            "leave_of_absence_treatment = counts_if_active_on_roster".into(),
                            s_ros,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
        }
        ClassPolicy::Acquisition(acq) => {
            if acq.completion_event.is_none() {
                let mut s_ann = spec.clone();
                if let ClassPolicy::Acquisition(ref mut a) = s_ann.predicate.class_policy {
                    a.completion_event = Some(AcquisitionCompletionEvent::PublicAnnouncement);
                    a.valuation_basis
                        .get_or_insert(ValuationBasis::EnterpriseValue);
                    a.consideration_types
                        .get_or_insert(vec![ConsiderationType::Cash]);
                    a.transaction_structures
                        .get_or_insert(vec![TransactionStructure::WholeCompanyMergerOrAcquisition]);
                }
                let mut s_cls = s_ann.clone();
                if let ClassPolicy::Acquisition(ref mut a) = s_cls.predicate.class_policy {
                    a.completion_event = Some(AcquisitionCompletionEvent::LegalClose);
                }
                fields.push((
                    "predicate.class_policy.completion_event".into(),
                    vec![
                        (
                            "completion_event = public_announcement".into(),
                            s_ann,
                            bundle.clone(),
                        ),
                        (
                            "completion_event = legal_close".into(),
                            s_cls,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
            if acq.valuation_basis.is_none() {
                let mut s_ev = spec.clone();
                if let ClassPolicy::Acquisition(ref mut a) = s_ev.predicate.class_policy {
                    a.completion_event
                        .get_or_insert(AcquisitionCompletionEvent::LegalClose);
                    a.valuation_basis = Some(ValuationBasis::EnterpriseValue);
                    a.consideration_types
                        .get_or_insert(vec![ConsiderationType::Cash]);
                    a.transaction_structures
                        .get_or_insert(vec![TransactionStructure::WholeCompanyMergerOrAcquisition]);
                }
                let mut s_eqv = s_ev.clone();
                if let ClassPolicy::Acquisition(ref mut a) = s_eqv.predicate.class_policy {
                    a.valuation_basis = Some(ValuationBasis::EquityValue);
                }
                fields.push((
                    "predicate.class_policy.valuation_basis".into(),
                    vec![
                        (
                            "valuation_basis = enterprise_value".into(),
                            s_ev,
                            bundle.clone(),
                        ),
                        (
                            "valuation_basis = equity_value".into(),
                            s_eqv,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
            if acq.earn_out_treatment.is_none() {
                let mut s_ex = spec.clone();
                if let ClassPolicy::Acquisition(ref mut a) = s_ex.predicate.class_policy {
                    a.completion_event
                        .get_or_insert(AcquisitionCompletionEvent::LegalClose);
                    a.valuation_basis
                        .get_or_insert(ValuationBasis::EnterpriseValue);
                    a.consideration_types
                        .get_or_insert(vec![ConsiderationType::Cash]);
                    a.transaction_structures
                        .get_or_insert(vec![TransactionStructure::WholeCompanyMergerOrAcquisition]);
                    a.earn_out_treatment = Some(EarnOutTreatment::ExcludeContingentEarnOuts);
                }
                let mut s_in = s_ex.clone();
                if let ClassPolicy::Acquisition(ref mut a) = s_in.predicate.class_policy {
                    a.earn_out_treatment = Some(EarnOutTreatment::IncludeMaximumEarnOut);
                }
                fields.push((
                    "predicate.class_policy.earn_out_treatment".into(),
                    vec![
                        (
                            "earn_out_treatment = exclude_contingent_earn_outs".into(),
                            s_ex,
                            bundle.clone(),
                        ),
                        (
                            "earn_out_treatment = include_maximum_earn_out".into(),
                            s_in,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
            if acq.consideration_types.is_none() {
                let mut s_cash = spec.clone();
                if let ClassPolicy::Acquisition(ref mut a) = s_cash.predicate.class_policy {
                    a.completion_event
                        .get_or_insert(AcquisitionCompletionEvent::LegalClose);
                    a.valuation_basis
                        .get_or_insert(ValuationBasis::EnterpriseValue);
                    a.consideration_types = Some(vec![
                        ConsiderationType::Cash,
                        ConsiderationType::PublicStock,
                    ]);
                    a.transaction_structures
                        .get_or_insert(vec![TransactionStructure::WholeCompanyMergerOrAcquisition]);
                }
                let mut s_all = s_cash.clone();
                if let ClassPolicy::Acquisition(ref mut a) = s_all.predicate.class_policy {
                    a.consideration_types = Some(vec![
                        ConsiderationType::Cash,
                        ConsiderationType::PublicStock,
                        ConsiderationType::PrivateStock,
                    ]);
                }
                fields.push((
                    "predicate.class_policy.consideration_types".into(),
                    vec![
                        (
                            "consideration_types = [cash, public_stock]".into(),
                            s_cash,
                            bundle.clone(),
                        ),
                        (
                            "consideration_types = [cash, public_stock, private_stock]".into(),
                            s_all,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
            if acq.transaction_structures.is_none() {
                let mut s_full = spec.clone();
                if let ClassPolicy::Acquisition(ref mut a) = s_full.predicate.class_policy {
                    a.completion_event
                        .get_or_insert(AcquisitionCompletionEvent::LegalClose);
                    a.valuation_basis
                        .get_or_insert(ValuationBasis::EnterpriseValue);
                    a.consideration_types
                        .get_or_insert(vec![ConsiderationType::Cash]);
                    a.transaction_structures =
                        Some(vec![TransactionStructure::WholeCompanyMergerOrAcquisition]);
                }
                let mut s_maj = s_full.clone();
                if let ClassPolicy::Acquisition(ref mut a) = s_maj.predicate.class_policy {
                    a.transaction_structures = Some(vec![
                        TransactionStructure::WholeCompanyMergerOrAcquisition,
                        TransactionStructure::MajorityEquityStake,
                    ]);
                }
                fields.push((
                    "predicate.class_policy.transaction_structures".into(),
                    vec![
                        ("transaction_structures = [whole_company_merger_or_acquisition]".into(), s_full, bundle.clone()),
                        ("transaction_structures = [whole_company_merger_or_acquisition, majority_equity_stake]".into(), s_maj, bundle.clone()),
                    ],
                ));
            }
            if acq.termination_after_announcement_policy.is_none() {
                let mut s_no = spec.clone();
                if let ClassPolicy::Acquisition(ref mut a) = s_no.predicate.class_policy {
                    a.completion_event
                        .get_or_insert(AcquisitionCompletionEvent::PublicAnnouncement);
                    a.valuation_basis
                        .get_or_insert(ValuationBasis::EnterpriseValue);
                    a.consideration_types
                        .get_or_insert(vec![ConsiderationType::Cash]);
                    a.transaction_structures
                        .get_or_insert(vec![TransactionStructure::WholeCompanyMergerOrAcquisition]);
                    a.termination_after_announcement_policy =
                        Some(TerminationPolicy::ResolvesNoIfTerminatedBeforeClose);
                }
                let mut s_yes = s_no.clone();
                if let ClassPolicy::Acquisition(ref mut a) = s_yes.predicate.class_policy {
                    a.termination_after_announcement_policy =
                        Some(TerminationPolicy::SnapshotAtAnnouncement);
                }
                fields.push((
                    "predicate.class_policy.termination_after_announcement_policy".into(),
                    vec![
                        ("termination_after_announcement_policy = resolves_no_if_terminated_before_close".into(), s_no, bundle.clone()),
                        ("termination_after_announcement_policy = snapshot_at_announcement".into(), s_yes, bundle.clone()),
                    ],
                ));
            }
        }
        ClassPolicy::Audience(aud) => {
            if aud.crossing_rule.is_none() {
                let mut s_mom = spec.clone();
                s_mom.observation.observation_mode =
                    Some(ObservationMode::AnytimeOnOrBeforeDeadline);
                if let ClassPolicy::Audience(ref mut a) = s_mom.predicate.class_policy {
                    a.crossing_rule = Some(CrossingRule::MomentaryCrossing);
                }
                let mut s_snap = spec.clone();
                s_snap.observation.observation_mode = Some(ObservationMode::PointInTimeAtDeadline);
                if let ClassPolicy::Audience(ref mut a) = s_snap.predicate.class_policy {
                    a.crossing_rule = Some(CrossingRule::DeadlineTimestampSnapshot);
                }
                fields.push((
                    "predicate.class_policy.crossing_rule".into(),
                    vec![
                        (
                            "crossing_rule = momentary_crossing".into(),
                            s_mom,
                            bundle.clone(),
                        ),
                        (
                            "crossing_rule = deadline_timestamp_snapshot".into(),
                            s_snap,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
            if aud.deleted_or_suspended_channel_policy.is_none() {
                let mut s_no = spec.clone();
                if let ClassPolicy::Audience(ref mut a) = s_no.predicate.class_policy {
                    a.deleted_or_suspended_channel_policy = Some(DeletedChannelPolicy::ResolvesNo);
                }
                let mut s_inv = spec.clone();
                if let ClassPolicy::Audience(ref mut a) = s_inv.predicate.class_policy {
                    a.deleted_or_suspended_channel_policy =
                        Some(DeletedChannelPolicy::ResolvesInvalid);
                }
                fields.push((
                    "predicate.class_policy.deleted_or_suspended_channel_policy".into(),
                    vec![
                        (
                            "deleted_or_suspended_channel_policy = resolves_no".into(),
                            s_no,
                            bundle.clone(),
                        ),
                        (
                            "deleted_or_suspended_channel_policy = resolves_invalid".into(),
                            s_inv,
                            bundle.clone(),
                        ),
                    ],
                ));
            }
        }
    }

    fields
}
