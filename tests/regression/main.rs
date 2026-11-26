use chrono::{DateTime, Duration, Utc};
use milestone_core::{
    from_json_str, from_yaml_str, rule_hash, ArrMeasurementBasis, ArrPolicy, AvailabilityPolicy,
    CancellationPolicy, ChurnTreatment, ClassPolicy, Comparator, ConflictPolicy, CurrencyPolicy,
    CutoffPrecision, DeadlineInclusivity, DiscountTreatment, EntityScope, EvidenceBundle,
    EvidenceItem, EvidenceMetadata, EvidencePolicy, EvidenceSource, EvidenceValue, FxSource,
    FxTimestampRule, InvalidTrigger, InvalidationPolicy, Metric, MilestoneSpec, Money,
    ObservationMode, ObservationWindow, Outcome, Predicate, ResolutionReasonCode,
    RestatementHandling, RetroactiveUnwindHandling, RevisionPolicy, SignedNotStartedTreatment,
    SourceType, Subject, SubjectType, Threshold, UsageBasedTreatment,
};
use milestone_lint::{evaluate_listability, ListabilityStatus};
use milestone_resolver::resolve;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

fn dt(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn resolved_arr_spec() -> MilestoneSpec {
    let deadline = dt("2027-06-30T23:59:59Z");
    let mut ids = BTreeMap::new();
    ids.insert("domain".into(), "acme.example".into());
    MilestoneSpec {
        schema_version: "1.0".into(),
        id: "acme-arr-1m".into(),
        title: "Will Acme reach $1M ARR by June 30, 2027?".into(),
        subject: Subject {
            name: "Acme".into(),
            subject_type: SubjectType::Company,
            jurisdiction: Some("US".into()),
            entity_scope: Some(EntityScope::ConsolidatedGroupWithSubsidiaries),
            identifiers: ids,
        },
        predicate: Predicate {
            metric: Metric::AnnualRecurringRevenue,
            comparator: Comparator::GreaterThanOrEqual,
            threshold: Threshold::Money(Money {
                amount: Decimal::from(1_000_000),
                currency: Some("USD".into()),
            }),
            class_policy: ClassPolicy::Arr(ArrPolicy {
                measurement_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                include_services_revenue: Some(false),
                include_one_time_fees: Some(false),
                usage_based_treatment: Some(UsageBasedTreatment::Excluded),
                discount_treatment: Some(DiscountTreatment::NetOfDiscounts),
                churn_treatment: Some(ChurnTreatment::ExcludeChurnedAndNoticed),
                signed_not_started_treatment: Some(
                    SignedNotStartedTreatment::ExcludeUntilStartDate,
                ),
                subsidiary_scope: Some(EntityScope::ConsolidatedGroupWithSubsidiaries),
            }),
        },
        observation: ObservationWindow {
            start: Some(dt("2026-01-01T00:00:00Z")),
            deadline: Some(deadline),
            timezone: Some("UTC".into()),
            deadline_inclusivity: Some(DeadlineInclusivity::Inclusive),
            observation_mode: Some(ObservationMode::PointInTimeAtDeadline),
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
                name: "Audited Revenue Ledger".into(),
                source_type: SourceType::AuditedFinancials,
                independent_of_subject: Some(true),
                verification_method: Some("CPA audit".into()),
            }),
            fallback_sources: vec![EvidenceSource {
                name: "SEC Filing".into(),
                source_type: SourceType::SecOrRegulatoryFiling,
                independent_of_subject: Some(true),
                verification_method: Some("EDGAR".into()),
            }],
            precedence_order: vec![
                SourceType::AuditedFinancials,
                SourceType::SecOrRegulatoryFiling,
                SourceType::CompanyAttestation,
                SourceType::PressRelease,
                SourceType::ThirdPartyMedia,
            ],
            conflict_policy: Some(ConflictPolicy::UseHighestPrecedenceSource),
            availability_policy: Some(AvailabilityPolicy::FallbackToSecondaryInPrecedenceOrder),
            minimum_corroboration: Some(1),
        },
        revision_policy: RevisionPolicy {
            corrections_until: Some(deadline + Duration::days(30)),
            restatement_window_days: Some(30),
            restatement_handling: Some(RestatementHandling::ApplyRestatementsWithinWindow),
            retroactive_unwind_handling: Some(
                RetroactiveUnwindHandling::ReversesOutcomeIfWithinWindow,
            ),
        },
        invalidation_policy: InvalidationPolicy {
            yes_condition_summary: Some("Contracted ARR >= $1M USD".into()),
            no_condition_summary: Some("Contracted ARR < $1M USD".into()),
            invalid_triggers: vec![InvalidTrigger::SourceUnavailablePastGracePeriod],
            cancellation_policy: Some(CancellationPolicy::ResolveNo),
        },
    }
}

fn arr_evidence(_spec_id: &str, amount: &str, obs: DateTime<Utc>, st: SourceType) -> EvidenceItem {
    EvidenceItem {
        id: format!("ev-{}-{}", amount, st as u8),
        source: format!("{:?}", st),
        source_type: st,
        publication_time: obs,
        observation_time: obs,
        retrieval_time: obs,
        value: EvidenceValue::Money {
            amount: amount.parse().unwrap(),
            currency: "USD".into(),
        },
        content_hash: "sha256:test".into(),
        fx_quote: None,
        metadata: EvidenceMetadata {
            arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
            includes_services: Some(false),
            includes_one_time_fees: Some(false),
            includes_signed_not_started: Some(false),
            net_of_discounts: Some(true),
            ..Default::default()
        },
    }
}

#[test]
fn test_4_exact_arr_threshold_boundaries() {
    let spec = resolved_arr_spec();
    let deadline = spec.observation.deadline.unwrap();

    for (amt, expected) in [
        ("999999.99", Outcome::No),
        ("1000000.00", Outcome::Yes),
        ("1000000.01", Outcome::Yes),
    ] {
        let bundle = EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: None,
            items: vec![arr_evidence(
                &spec.id,
                amt,
                deadline,
                SourceType::AuditedFinancials,
            )],
        };
        let res = resolve(&spec, &bundle).unwrap();
        assert_eq!(res.outcome, expected, "failed for amount {}", amt);
    }
}

#[test]
fn test_5_deadline_second_boundaries() {
    let mut spec = resolved_arr_spec();
    let deadline = spec.observation.deadline.unwrap();

    // Inclusive
    spec.observation.deadline_inclusivity = Some(DeadlineInclusivity::Inclusive);
    for (offset, expected) in [(-1, Outcome::Yes), (0, Outcome::Yes), (1, Outcome::No)] {
        let bundle = EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: None,
            items: vec![arr_evidence(
                &spec.id,
                "1000000.00",
                deadline + Duration::seconds(offset),
                SourceType::AuditedFinancials,
            )],
        };
        assert_eq!(resolve(&spec, &bundle).unwrap().outcome, expected);
    }

    // Exclusive
    spec.observation.deadline_inclusivity = Some(DeadlineInclusivity::Exclusive);
    for (offset, expected) in [(-1, Outcome::Yes), (0, Outcome::No), (1, Outcome::No)] {
        let bundle = EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: None,
            items: vec![arr_evidence(
                &spec.id,
                "1000000.00",
                deadline + Duration::seconds(offset),
                SourceType::AuditedFinancials,
            )],
        };
        assert_eq!(resolve(&spec, &bundle).unwrap().outcome, expected);
    }

    // Undefined inclusivity at exact deadline -> UNRESOLVED
    spec.observation.deadline_inclusivity = None;
    let exact_bundle = EvidenceBundle {
        spec_id: spec.id.clone(),
        evaluation_time: None,
        items: vec![arr_evidence(
            &spec.id,
            "1000000.00",
            deadline,
            SourceType::AuditedFinancials,
        )],
    };
    assert_eq!(
        resolve(&spec, &exact_bundle).unwrap().outcome,
        Outcome::Unresolved
    );
}

#[test]
fn test_6_canonical_keccak_rule_hash_across_formats() {
    let spec = resolved_arr_spec();
    let yaml = serde_yaml::to_string(&spec).unwrap();
    let json = serde_json::to_string_pretty(&spec).unwrap();

    let s_yaml = from_yaml_str(&yaml).unwrap();
    let s_json = from_json_str(&json).unwrap();
    assert_eq!(rule_hash(&spec), rule_hash(&s_yaml));
    assert_eq!(rule_hash(&s_yaml), rule_hash(&s_json));
}

#[test]
fn test_7_source_conflict_without_precedence_returns_unresolved_source_conflict() {
    let mut spec = resolved_arr_spec();
    spec.evidence_policy.conflict_policy = None;
    spec.evidence_policy.precedence_order.clear();

    let deadline = spec.observation.deadline.unwrap();
    let bundle = EvidenceBundle {
        spec_id: spec.id.clone(),
        evaluation_time: None,
        items: vec![
            arr_evidence(&spec.id, "1100000.00", deadline, SourceType::PressRelease),
            arr_evidence(
                &spec.id,
                "950000.00",
                deadline,
                SourceType::AuditedFinancials,
            ),
        ],
    };

    let res = resolve(&spec, &bundle).unwrap();
    assert_eq!(res.outcome, Outcome::Unresolved);
    assert_eq!(res.reason_code, ResolutionReasonCode::SourceConflict);

    // With explicit precedence, AuditedFinancials wins -> NO
    let resolved_spec = resolved_arr_spec();
    let res2 = resolve(&resolved_spec, &bundle).unwrap();
    assert_eq!(res2.outcome, Outcome::No);
}

#[test]
fn test_subject_control_risk_triggers_critical() {
    let mut spec = resolved_arr_spec();
    spec.evidence_policy.primary_source = Some(EvidenceSource {
        name: "Founder Self-Attestation".into(),
        source_type: SourceType::CompanyAttestation,
        independent_of_subject: Some(false),
        verification_method: Some("Signed blog post".into()),
    });
    spec.evidence_policy.fallback_sources.clear();
    spec.evidence_policy.minimum_corroboration = Some(1);

    let report = evaluate_listability(&spec);
    assert_eq!(report.status, ListabilityStatus::NotListable);
    assert!(report.findings.iter().any(|f| f.id == "EVIDENCE-004"));
}

#[test]
fn test_9_example_corpus_has_at_least_25_valid_specs() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let examples_dir = manifest_dir.parent().unwrap().join("examples");

    let mut bad_count = 0usize;
    let mut resolved_count = 0usize;

    for class_name in [
        "arr",
        "fundraising",
        "employment",
        "acquisition",
        "audience",
    ] {
        let class_dir = examples_dir.join(class_name);
        for entry in std::fs::read_dir(class_dir.join("bad")).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                let raw = std::fs::read_to_string(&path).unwrap();
                let spec = from_yaml_str(&raw)
                    .unwrap_or_else(|e| panic!("failed to parse {:?}: {}", path, e));
                let report = evaluate_listability(&spec);
                assert_eq!(
                    report.status,
                    ListabilityStatus::NotListable,
                    "bad example {:?} should be NOT_LISTABLE",
                    path
                );
                bad_count += 1;
            }
        }
        for entry in std::fs::read_dir(class_dir.join("resolved")).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                let raw = std::fs::read_to_string(&path).unwrap();
                let spec = from_yaml_str(&raw)
                    .unwrap_or_else(|e| panic!("failed to parse {:?}: {}", path, e));
                let report = evaluate_listability(&spec);
                assert_eq!(
                    report.status,
                    ListabilityStatus::Listable,
                    "resolved example {:?} should be LISTABLE, got findings: {:?}",
                    path,
                    report.findings
                );
                let attack = milestone_adversarial::attack_spec(&spec);
                assert_eq!(
                    attack.ambiguous_count, 0,
                    "resolved example {:?} should have 0 ambiguous scenarios, got: {:?}",
                    path, attack.results
                );
                resolved_count += 1;
            }
        }
    }

    assert!(
        bad_count + resolved_count >= 25,
        "expected >= 25 example specs, found {}",
        bad_count + resolved_count
    );
}
