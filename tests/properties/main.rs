use chrono::{DateTime, Duration, Utc};
use milestone_core::{
    canonical_bytes, from_json_str, from_yaml_str, rule_hash, ArrMeasurementBasis, ArrPolicy,
    AvailabilityPolicy, CancellationPolicy, ChurnTreatment, ClassPolicy, Comparator,
    ConflictPolicy, CurrencyPolicy, CutoffPrecision, DeadlineInclusivity, DiscountTreatment,
    EntityScope, EvidenceBundle, EvidenceItem, EvidenceMetadata, EvidencePolicy, EvidenceSource,
    EvidenceValue, FinancingInstrument, FundraisingCompletionEvent, FundraisingPolicy, FxQuote,
    FxSource, FxTimestampRule, InvalidTrigger, InvalidationPolicy, Metric, MilestoneSpec, Money,
    ObservationMode, ObservationWindow, Outcome, Predicate, RestatementHandling,
    RetroactiveUnwindHandling, RevisionPolicy, SignedNotStartedTreatment, SourceType, Subject,
    SubjectType, Threshold, UsageBasedTreatment,
};
use milestone_resolver::resolve;
use proptest::prelude::*;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

fn dt(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn fully_specified_arr_spec() -> MilestoneSpec {
    let deadline = dt("2027-06-30T23:59:59Z");
    let mut ids = BTreeMap::new();
    ids.insert("domain".into(), "acme.example".into());
    MilestoneSpec {
        schema_version: "1.0".into(),
        id: "acme-arr-1m".into(),
        title: "Will Acme reach $1M ARR by June 30, 2027?".into(),
        subject: Subject {
            name: "Acme Corp".into(),
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
                verification_method: Some("CPA sign-off".into()),
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
            yes_condition_summary: Some(
                "Audited contracted ARR >= $1,000,000 USD on deadline.".into(),
            ),
            no_condition_summary: Some(
                "Audited contracted ARR < $1,000,000 USD on deadline.".into(),
            ),
            invalid_triggers: vec![InvalidTrigger::SourceUnavailablePastGracePeriod],
            cancellation_policy: Some(CancellationPolicy::ResolveNo),
        },
    }
}

fn make_arr_item(
    id: &str,
    source_type: SourceType,
    amount: Decimal,
    ccy: &str,
    obs: DateTime<Utc>,
) -> EvidenceItem {
    EvidenceItem {
        id: id.into(),
        source: format!("{:?}-src", source_type),
        source_type,
        publication_time: obs,
        observation_time: obs,
        retrieval_time: obs,
        value: EvidenceValue::Money {
            amount,
            currency: ccy.into(),
        },
        content_hash: format!("hash-{}", id),
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

proptest! {
    #[test]
    fn prop_determinism_repeated_resolve(cents in 90_000_000_i64..110_000_000_i64) {
        let spec = fully_specified_arr_spec();
        let deadline = spec.observation.deadline.unwrap();
        let amt = Decimal::new(cents, 2);
        let bundle = EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![make_arr_item("1", SourceType::AuditedFinancials, amt, "USD", deadline)],
        };
        let r1 = resolve(&spec, &bundle).unwrap();
        let r2 = resolve(&spec, &bundle).unwrap();
        prop_assert_eq!(&r1, &r2);
        // Finality: never simultaneously satisfies YES and NO; decisive YES iff amt >= 1_000_000
        if amt >= Decimal::from(1_000_000) {
            prop_assert_eq!(r1.outcome, Outcome::Yes);
        } else {
            prop_assert_eq!(r1.outcome, Outcome::No);
        }
    }

    #[test]
    fn prop_canonicalization_invariant_to_set_order_and_decimal_scale(
        reverse_instruments in any::<bool>(),
        trailing_zeros in 0u32..5u32,
    ) {
        let mut s1 = MilestoneSpec {
            schema_version: "1.0".into(),
            id: "fund-canon".into(),
            title: "Will Acme raise $5M?".into(),
            subject: Subject {
                name: "Acme".into(),
                subject_type: SubjectType::Company,
                jurisdiction: Some("us".into()),
                entity_scope: None,
                identifiers: BTreeMap::new(),
            },
            predicate: Predicate {
                metric: Metric::FundraisingTotal,
                comparator: Comparator::GreaterThanOrEqual,
                threshold: Threshold::Money(Money {
                    amount: Decimal::from(5_000_000),
                    currency: Some("usd".into()),
                }),
                class_policy: ClassPolicy::Fundraising(FundraisingPolicy {
                    completion_event: Some(FundraisingCompletionEvent::FundsReceivedInBank),
                    allowed_instruments: Some(vec![
                        FinancingInstrument::PricedEquity,
                        FinancingInstrument::Safe,
                    ]),
                    ..Default::default()
                }),
            },
            observation: ObservationWindow {
                deadline: Some(dt("2027-12-31T23:59:59Z")),
                ..Default::default()
            },
            currency_policy: None,
            evidence_policy: EvidencePolicy::default(),
            revision_policy: RevisionPolicy::default(),
            invalidation_policy: InvalidationPolicy::default(),
        };

        let mut s2 = s1.clone();
        s2.subject.jurisdiction = Some("  US ".into());
        if let Threshold::Money(ref mut m) = s2.predicate.threshold {
            let scale_mult = 10i64.pow(trailing_zeros);
            m.amount = Decimal::new(5_000_000i64 * scale_mult, trailing_zeros);
            m.currency = Some("USD".into());
        }
        if reverse_instruments {
            if let ClassPolicy::Fundraising(ref mut f) = s2.predicate.class_policy {
                f.allowed_instruments = Some(vec![
                    FinancingInstrument::Safe,
                    FinancingInstrument::PricedEquity,
                ]);
            }
        }
        s1.observation.deadline = Some(dt("2028-01-01T05:29:59+05:30"));

        prop_assert_eq!(canonical_bytes(&s1), canonical_bytes(&s2));
        prop_assert_eq!(rule_hash(&s1), rule_hash(&s2));

        // Round-trip through YAML and JSON with whitespace/comment/ordering differences
        let yaml = serde_yaml::to_string(&s1).unwrap();
        let json = serde_json::to_string_pretty(&s2).unwrap();
        let p_yaml = from_yaml_str(&format!("---\n# comment\n{}\n\n", yaml)).unwrap();
        let p_json = from_json_str(&format!("  \n{}\n", json)).unwrap();
        prop_assert_eq!(rule_hash(&p_yaml), rule_hash(&p_json));
    }

    #[test]
    fn prop_evidence_permutation_invariance(perm in prop::collection::vec(0usize..100, 3..8)) {
        let spec = fully_specified_arr_spec();
        let deadline = spec.observation.deadline.unwrap();
        let base_items = vec![
            make_arr_item("a", SourceType::PressRelease, Decimal::from(1_200_000), "USD", deadline),
            make_arr_item("b", SourceType::CompanyAttestation, Decimal::from(1_050_000), "USD", deadline),
            make_arr_item("c", SourceType::AuditedFinancials, Decimal::from(990_000), "USD", deadline),
            make_arr_item("d", SourceType::SecOrRegulatoryFiling, Decimal::from(990_000), "USD", deadline),
        ];
        let mut permuted = base_items.clone();
        for (i, p) in perm.iter().enumerate() {
            let a = i % permuted.len();
            let b = p % permuted.len();
            permuted.swap(a, b);
        }
        let b1 = EvidenceBundle { spec_id: spec.id.clone(), evaluation_time: None, items: base_items };
        let b2 = EvidenceBundle { spec_id: spec.id.clone(), evaluation_time: None, items: permuted };
        let r1 = resolve(&spec, &b1).unwrap();
        let r2 = resolve(&spec, &b2).unwrap();
        prop_assert_eq!(r1.outcome, r2.outcome);
        prop_assert_eq!(r1.reason_code, r2.reason_code);
        prop_assert_eq!(r1.decisive_evidence_ids, r2.decisive_evidence_ids);
    }

    #[test]
    fn prop_threshold_boundaries(delta in -5i64..=5i64) {
        let spec = fully_specified_arr_spec();
        let deadline = spec.observation.deadline.unwrap();
        let amt = Decimal::from(1_000_000) + Decimal::new(delta, 2);
        let bundle = EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: None,
            items: vec![make_arr_item("t", SourceType::AuditedFinancials, amt, "USD", deadline)],
        };
        let res = resolve(&spec, &bundle).unwrap();
        let expected = if delta >= 0 { Outcome::Yes } else { Outcome::No };
        prop_assert_eq!(res.outcome, expected);
    }

    #[test]
    fn prop_deadline_boundaries(offset_secs in -2i64..=2i64) {
        let spec = fully_specified_arr_spec();
        let deadline = spec.observation.deadline.unwrap();
        let obs = deadline + Duration::seconds(offset_secs);
        let bundle = EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: None,
            items: vec![make_arr_item("d", SourceType::AuditedFinancials, Decimal::from(1_000_000), "USD", obs)],
        };
        let res = resolve(&spec, &bundle).unwrap();
        // spec is Inclusive (<= deadline)
        let expected = if offset_secs <= 0 { Outcome::Yes } else { Outcome::No };
        prop_assert_eq!(res.outcome, expected);
    }

    #[test]
    fn prop_currency_conversion_respects_configured_fx_source(eur_cents in 80_000_000_i64..110_000_000_i64) {
        let spec = fully_specified_arr_spec();
        let deadline = spec.observation.deadline.unwrap();
        let eur = Decimal::new(eur_cents, 2);
        let rate = Decimal::new(110, 2); // 1.10 USD/EUR

        let mut item = make_arr_item("fx", SourceType::AuditedFinancials, eur, "EUR", deadline);
        item.fx_quote = Some(FxQuote {
            base_currency: "EUR".into(),
            quote_currency: "USD".into(),
            rate,
            fx_source: FxSource::EcbDailyReference,
            fx_timestamp: deadline,
            fx_timestamp_rule: Some(FxTimestampRule::DeadlineClose),
        });
        let bundle = EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: None,
            items: vec![item.clone()],
        };
        let res = resolve(&spec, &bundle).unwrap();
        let expected = if eur * rate >= Decimal::from(1_000_000) {
            Outcome::Yes
        } else {
            Outcome::No
        };
        prop_assert_eq!(res.outcome, expected);

        // Wrong FX source must never guess -> UNRESOLVED
        item.fx_quote.as_mut().unwrap().fx_source = FxSource::BloombergFixing;
        let bad_bundle = EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: None,
            items: vec![item],
        };
        let bad_res = resolve(&spec, &bad_bundle).unwrap();
        prop_assert_eq!(bad_res.outcome, Outcome::Unresolved);
    }
}
