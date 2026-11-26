use milestone_adversarial::{attack_spec, ScenarioStatus};
use milestone_core::{
    AcquisitionCompletionEvent, AcquisitionPolicy, ArrMeasurementBasis, ArrPolicy,
    AvailabilityPolicy, CancellationPolicy, ChurnTreatment, ClassPolicy, Comparator,
    ConditionalCommitmentPolicy, ConflictPolicy, CurrencyPolicy, CutoffPrecision,
    DeadlineInclusivity, DiscountTreatment, EarnOutTreatment, EmploymentPolicy, EntityScope,
    EvidencePolicy, EvidenceSource, FinancingInstrument, FundraisingCompletionEvent,
    FundraisingPolicy, FxSource, FxTimestampRule, InvalidTrigger, InvalidationPolicy, Metric,
    MilestoneSpec, Money, ObservationMode, ObservationWindow, Outcome, Predicate,
    RestatementHandling, RetroactiveUnwindHandling, RevisionPolicy, SignedNotStartedTreatment,
    SourceType, Subject, SubjectType, Threshold, TranchePolicy, UnwindPolicy, UsageBasedTreatment,
};
use milestone_lint::{evaluate_listability, ListabilityStatus};
use milestone_parser::parse_headline;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

#[test]
fn test_1_and_2_raise_headline_flags_completion_event_and_distinguishes_term_sheet_vs_cash() {
    let parsed = parse_headline("Will Acme raise $5M before Dec 31?").unwrap();
    let spec = parsed.spec.expect("should parse fundraising headline");

    let report = evaluate_listability(&spec);
    assert_eq!(report.status, ListabilityStatus::NotListable);
    assert!(
        report.findings.iter().any(|f| f.id == "TIME-002"),
        "expected TIME-002 for undefined fundraising completion event"
    );

    let attack = attack_spec(&spec);
    assert!(
        attack.ambiguous_count >= 5,
        "bad fundraising spec must expose multiple ambiguities"
    );

    // Scenario 1: term sheet signed Dec 29, funds arrive Jan 3
    let sc1 = attack
        .results
        .iter()
        .find(|r| r.scenario_id == "fund-01")
        .expect("fund-01 present");
    assert_eq!(sc1.status, ScenarioStatus::Ambiguous);
    assert!(sc1
        .minimal_missing_policies
        .contains(&"predicate.class_policy.completion_event".to_string()));
    let has_yes_ts = sc1
        .divergent_interpretations
        .iter()
        .any(|d| d.policy_setting.contains("term_sheet_signed") && d.outcome == Outcome::Yes);
    let has_no_cash = sc1
        .divergent_interpretations
        .iter()
        .any(|d| d.policy_setting.contains("funds_received_in_bank") && d.outcome == Outcome::No);
    assert!(
        has_yes_ts && has_no_cash,
        "must distinguish term_sheet_signed (YES) from funds_received_in_bank (NO)"
    );
}

#[test]
fn test_3_arr_headline_flags_undefined_arr_methodology() {
    let parsed = parse_headline("Will Acme reach $1M ARR by June 30, 2027?").unwrap();
    let spec = parsed.spec.expect("should parse ARR headline");

    let report = evaluate_listability(&spec);
    assert_eq!(report.status, ListabilityStatus::NotListable);
    assert!(
        report.findings.iter().any(|f| f.id == "M-ARR-001"),
        "expected M-ARR-001 for undefined ARR measurement basis"
    );

    let attack = attack_spec(&spec);
    assert!(attack.ambiguous_count >= 5);
}

#[test]
fn test_8_fully_resolved_fundraising_spec_eliminates_ambiguities() {
    let deadline = chrono::DateTime::parse_from_rfc3339("2027-12-31T23:59:59Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let mut ids = BTreeMap::new();
    ids.insert("domain".into(), "acme.example".into());

    let resolved = MilestoneSpec {
        schema_version: "1.0".into(),
        id: "acme-raise-5m-resolved".into(),
        title: "Will Acme raise $5M in closed primary equity by Dec 31, 2027?".into(),
        subject: Subject {
            name: "Acme Corp".into(),
            subject_type: SubjectType::Company,
            jurisdiction: Some("US".into()),
            entity_scope: Some(EntityScope::ParentEntityOnly),
            identifiers: ids,
        },
        predicate: Predicate {
            metric: Metric::FundraisingTotal,
            comparator: Comparator::GreaterThanOrEqual,
            threshold: Threshold::Money(Money {
                amount: Decimal::from(5_000_000),
                currency: Some("USD".into()),
            }),
            class_policy: ClassPolicy::Fundraising(FundraisingPolicy {
                completion_event: Some(FundraisingCompletionEvent::LegalCloseAndFundsReceived),
                allowed_instruments: Some(vec![FinancingInstrument::PricedEquity]),
                include_secondary: Some(false),
                include_insider_capital: Some(false),
                include_existing_note_conversion: Some(false),
                tranche_policy: Some(TranchePolicy::CountOnlyFundedTranches),
                conditional_commitment_policy: Some(
                    ConditionalCommitmentPolicy::ExcludeConditional,
                ),
                unwind_policy: Some(UnwindPolicy::MustRemainClosedThroughFinalization),
            }),
        },
        observation: ObservationWindow {
            start: Some(
                chrono::DateTime::parse_from_rfc3339("2027-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            ),
            deadline: Some(deadline),
            timezone: Some("UTC".into()),
            deadline_inclusivity: Some(DeadlineInclusivity::Inclusive),
            observation_mode: Some(ObservationMode::AnytimeOnOrBeforeDeadline),
            cutoff_precision: Some(CutoffPrecision::ExactSecondUtc),
        },
        currency_policy: Some(CurrencyPolicy {
            reporting_currency: Some("USD".into()),
            fx_source: Some(FxSource::EcbDailyReference),
            fx_timestamp_rule: Some(FxTimestampRule::DeadlineClose),
            allow_non_reporting_currency: Some(true),
        }),
        evidence_policy: EvidencePolicy {
            primary_source: Some(EvidenceSource {
                name: "Escrow Bank Confirmation".into(),
                source_type: SourceType::BankOrEscrowConfirmation,
                independent_of_subject: Some(true),
                verification_method: Some("Signed bank wire ledger".into()),
            }),
            fallback_sources: vec![EvidenceSource {
                name: "SEC Form D".into(),
                source_type: SourceType::SecOrRegulatoryFiling,
                independent_of_subject: Some(true),
                verification_method: Some("EDGAR".into()),
            }],
            precedence_order: vec![
                SourceType::BankOrEscrowConfirmation,
                SourceType::SecOrRegulatoryFiling,
                SourceType::CapTableRegistry,
                SourceType::CompanyAttestation,
                SourceType::PressRelease,
            ],
            conflict_policy: Some(ConflictPolicy::UseHighestPrecedenceSource),
            availability_policy: Some(AvailabilityPolicy::FallbackToSecondaryInPrecedenceOrder),
            minimum_corroboration: Some(1),
        },
        revision_policy: RevisionPolicy {
            corrections_until: Some(deadline + chrono::Duration::days(14)),
            restatement_window_days: Some(14),
            restatement_handling: Some(RestatementHandling::ApplyRestatementsWithinWindow),
            retroactive_unwind_handling: Some(
                RetroactiveUnwindHandling::ReversesOutcomeIfWithinWindow,
            ),
        },
        invalidation_policy: InvalidationPolicy {
            yes_condition_summary: Some(
                "Primary equity closed and cash received >= $5M USD by deadline.".into(),
            ),
            no_condition_summary: Some(
                "Closed primary equity cash received < $5M USD by deadline.".into(),
            ),
            invalid_triggers: vec![InvalidTrigger::SourceUnavailablePastGracePeriod],
            cancellation_policy: Some(CancellationPolicy::ResolveNo),
        },
    };

    let listability = evaluate_listability(&resolved);
    assert_eq!(listability.status, ListabilityStatus::Listable);

    let attack = attack_spec(&resolved);
    assert_eq!(
        attack.ambiguous_count, 0,
        "fully resolved fundraising spec should resolve every adversarial scenario deterministically: {:?}",
        attack.results
    );
}

#[test]
fn test_adversarial_employment_and_acquisition_and_audience() {
    let emp = parse_headline("Will Alice join OpenAI before December 31, 2027?")
        .unwrap()
        .spec
        .unwrap();
    let emp_attack = attack_spec(&emp);
    assert!(emp_attack.ambiguous_count >= 4);

    let acq = parse_headline("Will Acme be acquired for $100M or more by 2028?")
        .unwrap()
        .spec
        .unwrap();
    let acq_attack = attack_spec(&acq);
    assert!(acq_attack.ambiguous_count >= 5);

    let aud = parse_headline("Will Alice reach 1M YouTube subscribers by December 31?")
        .unwrap()
        .spec
        .unwrap();
    let aud_attack = attack_spec(&aud);
    assert!(aud_attack.ambiguous_count >= 3);

    // Silence unused-import warnings while ensuring types remain exercised
    let _ = ArrPolicy {
        measurement_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
        include_services_revenue: Some(false),
        include_one_time_fees: Some(false),
        usage_based_treatment: Some(UsageBasedTreatment::Excluded),
        discount_treatment: Some(DiscountTreatment::NetOfDiscounts),
        churn_treatment: Some(ChurnTreatment::ExcludeChurnedAndNoticed),
        signed_not_started_treatment: Some(SignedNotStartedTreatment::ExcludeUntilStartDate),
        subsidiary_scope: Some(EntityScope::ParentEntityOnly),
    };
    let _ = EmploymentPolicy::default();
    let _ = AcquisitionPolicy {
        completion_event: Some(AcquisitionCompletionEvent::LegalClose),
        earn_out_treatment: Some(EarnOutTreatment::ExcludeContingentEarnOuts),
        ..Default::default()
    };
}
