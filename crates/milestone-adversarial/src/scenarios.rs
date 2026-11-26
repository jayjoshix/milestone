use chrono::{DateTime, Duration, Utc};
use milestone_core::{
    AcquisitionCompletionEvent, ArrMeasurementBasis, ClassPolicy, ConsiderationType,
    EmploymentQualifyingEvent, EmploymentRelationship, EvidenceBundle, EvidenceItem,
    EvidenceMetadata, EvidenceValue, FinancingInstrument, FundraisingCompletionEvent, FxQuote,
    FxSource, FxTimestampRule, MilestoneSpec, SourceType, Threshold, TransactionStructure,
    ValuationBasis,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdversarialScenario {
    pub id: String,
    pub name: String,
    pub description: String,
    pub target_policy_hint: String,
    pub bundle: EvidenceBundle,
    /// Optional local-timezone offset test (e.g., if `observation.timezone` is unset,
    /// an event at 23:58 local time shifts relative to UTC deadline).
    #[serde(default)]
    pub timezone_ambiguity_note: Option<String>,
}

fn default_deadline(spec: &MilestoneSpec) -> DateTime<Utc> {
    spec.observation.deadline.unwrap_or_else(|| {
        DateTime::parse_from_rfc3339("2027-12-31T23:59:59Z")
            .unwrap()
            .with_timezone(&Utc)
    })
}

fn money_amount(spec: &MilestoneSpec, fallback: i64) -> (Decimal, String) {
    match &spec.predicate.threshold {
        Threshold::Money(m) => (
            m.amount,
            m.currency
                .as_deref()
                .filter(|c| !c.trim().is_empty())
                .unwrap_or("USD")
                .to_uppercase(),
        ),
        _ => (Decimal::from(fallback), "USD".to_string()),
    }
}

fn count_target(spec: &MilestoneSpec, fallback: u64) -> u64 {
    match &spec.predicate.threshold {
        Threshold::Count(c) => *c,
        _ => fallback,
    }
}

fn pct(amount: Decimal, num: i64, den: i64) -> Decimal {
    ((amount * Decimal::from(num)) / Decimal::from(den)).round_dp(2)
}

fn make_item(
    id: &str,
    source: &str,
    source_type: SourceType,
    obs: DateTime<Utc>,
    pub_t: DateTime<Utc>,
    value: EvidenceValue,
    metadata: EvidenceMetadata,
) -> EvidenceItem {
    EvidenceItem {
        id: id.to_string(),
        source: source.to_string(),
        source_type,
        publication_time: pub_t,
        observation_time: obs,
        retrieval_time: pub_t,
        value,
        content_hash: format!("sha256:adv-{}", id),
        fx_quote: None,
        metadata,
    }
}

/// Generates domain-authentic adversarial scenarios tailored to `spec`'s market class,
/// threshold, currency, and deadline.
pub fn generate_scenarios(spec: &MilestoneSpec) -> Vec<AdversarialScenario> {
    let deadline = default_deadline(spec);
    match &spec.predicate.class_policy {
        ClassPolicy::Fundraising(_) => fundraising_scenarios(spec, deadline),
        ClassPolicy::Arr(_) => arr_scenarios(spec, deadline),
        ClassPolicy::Employment(emp) => employment_scenarios(spec, deadline, &emp.target_employer),
        ClassPolicy::Acquisition(_) => acquisition_scenarios(spec, deadline),
        ClassPolicy::Audience(aud) => {
            audience_scenarios(spec, deadline, aud.platform.as_deref().unwrap_or("youtube"))
        }
    }
}

fn fundraising_scenarios(
    spec: &MilestoneSpec,
    deadline: DateTime<Utc>,
) -> Vec<AdversarialScenario> {
    let (target, ccy) = money_amount(spec, 5_000_000);
    let mut out = Vec::with_capacity(12);

    // 1. Term sheet signed Dec 29, funds arrive Jan 3
    out.push(AdversarialScenario {
        id: "fund-01".into(),
        name: "Term sheet before deadline, cash settles after".into(),
        description: format!(
            "${} term sheet signed 2 days before deadline; funds wire 3 days after deadline.",
            target
        ),
        target_policy_hint: "predicate.class_policy.completion_event".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(10)),
            items: vec![
                make_item(
                    "ts-1",
                    "Company Legal Counsel",
                    SourceType::CompanyAttestation,
                    deadline - Duration::days(2),
                    deadline - Duration::days(2),
                    EvidenceValue::Money {
                        amount: target,
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        fundraising_event: Some(FundraisingCompletionEvent::TermSheetSigned),
                        instruments: vec![FinancingInstrument::PricedEquity],
                        includes_secondary: Some(false),
                        ..Default::default()
                    },
                ),
                make_item(
                    "wire-1",
                    "Escrow Bank",
                    SourceType::BankOrEscrowConfirmation,
                    deadline + Duration::days(3),
                    deadline + Duration::days(3),
                    EvidenceValue::Money {
                        amount: target,
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        fundraising_event: Some(FundraisingCompletionEvent::FundsReceivedInBank),
                        instruments: vec![FinancingInstrument::PricedEquity],
                        includes_secondary: Some(false),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 2. Mixed Equity + Venture Debt
    let eq_amt = pct(target, 80, 100);
    let debt_amt = pct(target, 40, 100);
    out.push(AdversarialScenario {
        id: "fund-02".into(),
        name: "Equity plus venture debt hybrid round".into(),
        description: format!(
            "{} priced equity + {} venture debt (total {} exceeds threshold only if debt counts).",
            eq_amt,
            debt_amt,
            eq_amt + debt_amt
        ),
        target_policy_hint: "predicate.class_policy.allowed_instruments".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![
                make_item(
                    "eq-part",
                    "Escrow Bank",
                    SourceType::BankOrEscrowConfirmation,
                    deadline - Duration::days(5),
                    deadline - Duration::days(5),
                    EvidenceValue::Money {
                        amount: eq_amt,
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        fundraising_event: Some(
                            FundraisingCompletionEvent::LegalCloseAndFundsReceived,
                        ),
                        instruments: vec![FinancingInstrument::PricedEquity],
                        includes_secondary: Some(false),
                        component_id: Some("equity".into()),
                        ..Default::default()
                    },
                ),
                make_item(
                    "debt-part",
                    "Escrow Bank",
                    SourceType::BankOrEscrowConfirmation,
                    deadline - Duration::days(5),
                    deadline - Duration::days(5),
                    EvidenceValue::Money {
                        amount: debt_amt,
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        fundraising_event: Some(
                            FundraisingCompletionEvent::LegalCloseAndFundsReceived,
                        ),
                        instruments: vec![FinancingInstrument::VentureDebt],
                        includes_secondary: Some(false),
                        component_id: Some("debt".into()),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 3. Announcement vs actual closed amount (Source Conflict)
    out.push(AdversarialScenario {
        id: "fund-03".into(),
        name: "Press release overstates closed capital".into(),
        description: format!(
            "Press release announces {}; bank/escrow confirmation shows only {} closed.",
            pct(target, 120, 100),
            pct(target, 90, 100)
        ),
        target_policy_hint: "evidence_policy.conflict_policy".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(2)),
            items: vec![
                make_item(
                    "pr-1",
                    "Company Blog",
                    SourceType::PressRelease,
                    deadline - Duration::days(4),
                    deadline - Duration::days(4),
                    EvidenceValue::Money {
                        amount: pct(target, 120, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        fundraising_event: Some(
                            FundraisingCompletionEvent::LegalCloseAndFundsReceived,
                        ),
                        instruments: vec![FinancingInstrument::PricedEquity],
                        includes_secondary: Some(false),
                        ..Default::default()
                    },
                ),
                make_item(
                    "bank-1",
                    "Escrow Bank",
                    SourceType::BankOrEscrowConfirmation,
                    deadline - Duration::days(4),
                    deadline - Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 90, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        fundraising_event: Some(
                            FundraisingCompletionEvent::LegalCloseAndFundsReceived,
                        ),
                        instruments: vec![FinancingInstrument::PricedEquity],
                        includes_secondary: Some(false),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 4. SAFE executed before deadline, payment arrives after
    out.push(AdversarialScenario {
        id: "fund-04".into(),
        name: "SAFE signed before deadline, wire arrives later".into(),
        description: format!(
            "{} SAFE executed 10 days before deadline; investor wire settles 4 days after deadline.",
            target
        ),
        target_policy_hint: "predicate.class_policy.allowed_instruments".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(6)),
            items: vec![
                make_item(
                    "safe-sign",
                    "Cap Table Registry",
                    SourceType::CapTableRegistry,
                    deadline - Duration::days(10),
                    deadline - Duration::days(10),
                    EvidenceValue::Money {
                        amount: target,
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        fundraising_event: Some(FundraisingCompletionEvent::DefinitiveAgreementSigned),
                        instruments: vec![FinancingInstrument::Safe],
                        includes_secondary: Some(false),
                        ..Default::default()
                    },
                ),
                make_item(
                    "safe-wire",
                    "Escrow Bank",
                    SourceType::BankOrEscrowConfirmation,
                    deadline + Duration::days(4),
                    deadline + Duration::days(4),
                    EvidenceValue::Money {
                        amount: target,
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        fundraising_event: Some(FundraisingCompletionEvent::FundsReceivedInBank),
                        instruments: vec![FinancingInstrument::Safe],
                        includes_secondary: Some(false),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 5. Foreign currency EUR received near threshold
    let eur_amt = pct(target, 95, 100);
    let mut eur_item = make_item(
        "eur-wire",
        "Escrow Bank",
        SourceType::BankOrEscrowConfirmation,
        deadline - Duration::days(3),
        deadline - Duration::days(3),
        EvidenceValue::Money {
            amount: eur_amt,
            currency: "EUR".into(),
        },
        EvidenceMetadata {
            fundraising_event: Some(FundraisingCompletionEvent::LegalCloseAndFundsReceived),
            instruments: vec![FinancingInstrument::PricedEquity],
            includes_secondary: Some(false),
            ..Default::default()
        },
    );
    let (fx_src, fx_rule) = spec
        .currency_policy
        .as_ref()
        .map(|cp| {
            (
                cp.fx_source.unwrap_or(FxSource::EcbDailyReference),
                cp.fx_timestamp_rule
                    .unwrap_or(FxTimestampRule::DeadlineClose),
            )
        })
        .unwrap_or((FxSource::EcbDailyReference, FxTimestampRule::DeadlineClose));
    eur_item.fx_quote = Some(FxQuote {
        base_currency: "EUR".into(),
        quote_currency: ccy.clone(),
        rate: Decimal::new(108, 2), // 1.08 => 0.95 * 1.08 = 1.026 >= 1.0
        fx_source: fx_src,
        fx_timestamp: deadline,
        fx_timestamp_rule: Some(fx_rule),
    });
    out.push(AdversarialScenario {
        id: "fund-05".into(),
        name: "Non-reporting currency (EUR) proceeds near parity".into(),
        description: format!(
            "{} EUR received before deadline (1.08 ECB vs 1.04 spot flips threshold).",
            eur_amt
        ),
        target_policy_hint: "currency_policy.fx_source".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![eur_item],
        },
        timezone_ambiguity_note: None,
    });

    // 6. Primary + Secondary split
    out.push(AdversarialScenario {
        id: "fund-06".into(),
        name: "Primary issuance combined with founder secondary".into(),
        description: format!(
            "{} primary treasury issuance + {} secondary share sale.",
            pct(target, 60, 100),
            pct(target, 60, 100)
        ),
        target_policy_hint: "predicate.class_policy.include_secondary".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![
                make_item(
                    "prim-1",
                    "Escrow Bank",
                    SourceType::BankOrEscrowConfirmation,
                    deadline - Duration::days(7),
                    deadline - Duration::days(7),
                    EvidenceValue::Money {
                        amount: pct(target, 60, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        fundraising_event: Some(
                            FundraisingCompletionEvent::LegalCloseAndFundsReceived,
                        ),
                        instruments: vec![FinancingInstrument::PricedEquity],
                        includes_secondary: Some(false),
                        component_id: Some("primary".into()),
                        ..Default::default()
                    },
                ),
                make_item(
                    "sec-1",
                    "Escrow Bank",
                    SourceType::BankOrEscrowConfirmation,
                    deadline - Duration::days(7),
                    deadline - Duration::days(7),
                    EvidenceValue::Money {
                        amount: pct(target, 60, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        fundraising_event: Some(
                            FundraisingCompletionEvent::LegalCloseAndFundsReceived,
                        ),
                        instruments: vec![FinancingInstrument::PricedEquity],
                        includes_secondary: Some(true),
                        component_id: Some("secondary".into()),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 7. Existing convertible notes convert
    out.push(AdversarialScenario {
        id: "fund-07".into(),
        name: "Conversion of prior convertible notes counted in round".into(),
        description: format!(
            "{} new cash + {} conversion of prior-year convertible notes.",
            pct(target, 45, 100),
            pct(target, 75, 100)
        ),
        target_policy_hint: "predicate.class_policy.include_existing_note_conversion".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![
                make_item(
                    "new-cash",
                    "Escrow Bank",
                    SourceType::BankOrEscrowConfirmation,
                    deadline - Duration::days(6),
                    deadline - Duration::days(6),
                    EvidenceValue::Money {
                        amount: pct(target, 45, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        fundraising_event: Some(
                            FundraisingCompletionEvent::LegalCloseAndFundsReceived,
                        ),
                        instruments: vec![FinancingInstrument::PricedEquity],
                        includes_secondary: Some(false),
                        includes_note_conversion: Some(false),
                        component_id: Some("new_cash".into()),
                        ..Default::default()
                    },
                ),
                make_item(
                    "note-conv",
                    "Cap Table Registry",
                    SourceType::BankOrEscrowConfirmation,
                    deadline - Duration::days(6),
                    deadline - Duration::days(6),
                    EvidenceValue::Money {
                        amount: pct(target, 75, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        fundraising_event: Some(
                            FundraisingCompletionEvent::LegalCloseAndFundsReceived,
                        ),
                        instruments: vec![FinancingInstrument::PricedEquity],
                        includes_secondary: Some(false),
                        includes_note_conversion: Some(true),
                        component_id: Some("note_conversion".into()),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 8. Timezone boundary: 23:58 local time vs Jan 1 UTC
    out.push(AdversarialScenario {
        id: "fund-08".into(),
        name: "Midnight timezone boundary (23:58 PT vs Jan 1 UTC)".into(),
        description: "Wire clears at 23:58 US Pacific Time on deadline date (07:58 UTC the next day).".into(),
        target_policy_hint: "observation.timezone".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![make_item(
                "tz-wire",
                "Escrow Bank",
                SourceType::BankOrEscrowConfirmation,
                deadline + Duration::hours(7) + Duration::minutes(58),
                deadline + Duration::hours(8),
                EvidenceValue::Money {
                    amount: target,
                    currency: ccy.clone(),
                },
                EvidenceMetadata {
                    fundraising_event: Some(FundraisingCompletionEvent::LegalCloseAndFundsReceived),
                    instruments: vec![FinancingInstrument::PricedEquity],
                    includes_secondary: Some(false),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: Some("23:58 local PT is 07:58 UTC next day".into()),
    });

    // 9. Round closes and later unwinds
    out.push(AdversarialScenario {
        id: "fund-09".into(),
        name: "Round closes before deadline and later unwinds".into(),
        description: format!(
            "{} round closes before deadline, then unwinds 5 days later.",
            pct(target, 120, 100)
        ),
        target_policy_hint: "predicate.class_policy.unwind_policy".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(10)),
            items: vec![make_item(
                "unwind-1",
                "Escrow Bank",
                SourceType::BankOrEscrowConfirmation,
                deadline - Duration::days(2),
                deadline + Duration::days(5),
                EvidenceValue::Money {
                    amount: pct(target, 120, 100),
                    currency: ccy.clone(),
                },
                EvidenceMetadata {
                    fundraising_event: Some(FundraisingCompletionEvent::LegalCloseAndFundsReceived),
                    instruments: vec![FinancingInstrument::PricedEquity],
                    includes_secondary: Some(false),
                    unwound_before_finalization: Some(true),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: None,
    });

    // 10. Conditional commitment
    out.push(AdversarialScenario {
        id: "fund-10".into(),
        name: "Conditional investor commitment".into(),
        description: format!(
            "{} commitment executed subject to unsatisfied commercial milestone.",
            target
        ),
        target_policy_hint: "predicate.class_policy.conditional_commitment_policy".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![make_item(
                "cond-1",
                "Escrow Bank",
                SourceType::BankOrEscrowConfirmation,
                deadline - Duration::days(3),
                deadline - Duration::days(3),
                EvidenceValue::Money {
                    amount: target,
                    currency: ccy.clone(),
                },
                EvidenceMetadata {
                    fundraising_event: Some(FundraisingCompletionEvent::LegalCloseAndFundsReceived),
                    instruments: vec![FinancingInstrument::PricedEquity],
                    includes_secondary: Some(false),
                    is_conditional_commitment: Some(true),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: None,
    });

    // 11. Exact deadline boundary (deadline vs deadline + 1s)
    out.push(AdversarialScenario {
        id: "fund-11".into(),
        name: "Exact deadline second boundary".into(),
        description: "Funds settle at the exact `deadline` second (`<=` vs `<`).".into(),
        target_policy_hint: "observation.deadline_inclusivity".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::seconds(10)),
            items: vec![make_item(
                "exact-sec",
                "Escrow Bank",
                SourceType::BankOrEscrowConfirmation,
                deadline,
                deadline,
                EvidenceValue::Money {
                    amount: target,
                    currency: ccy,
                },
                EvidenceMetadata {
                    fundraising_event: Some(FundraisingCompletionEvent::LegalCloseAndFundsReceived),
                    instruments: vec![FinancingInstrument::PricedEquity],
                    includes_secondary: Some(false),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: None,
    });

    out
}

fn arr_scenarios(spec: &MilestoneSpec, deadline: DateTime<Utc>) -> Vec<AdversarialScenario> {
    let (target, ccy) = money_amount(spec, 1_000_000);
    let mut out = Vec::with_capacity(12);

    // 1. Contracted ARR vs Annualized MRR
    out.push(AdversarialScenario {
        id: "arr-01".into(),
        name: "Contracted ARR exceeds threshold while annualized MRR lags".into(),
        description: format!(
            "Signed contracted ARR is {}, while billed annualized MRR is {}.",
            pct(target, 110, 100),
            pct(target, 94, 100)
        ),
        target_policy_hint: "predicate.class_policy.measurement_basis".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(2)),
            items: vec![
                make_item(
                    "arr-contracted",
                    "Audited Revenue Ledger",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 110, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        includes_services: Some(false),
                        includes_one_time_fees: Some(false),
                        net_of_discounts: Some(true),
                        ..Default::default()
                    },
                ),
                make_item(
                    "arr-mrr",
                    "Billing System",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 94, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::AnnualizedMrr),
                        includes_services: Some(false),
                        includes_one_time_fees: Some(false),
                        net_of_discounts: Some(true),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 2. Professional services included
    out.push(AdversarialScenario {
        id: "arr-02".into(),
        name: "Implementation & consulting services push ARR over threshold".into(),
        description: format!(
            "{} software ARR + {} professional services onboarding.",
            pct(target, 86, 100),
            pct(target, 24, 100)
        ),
        target_policy_hint: "predicate.class_policy.include_services_revenue".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![
                make_item(
                    "saas-core",
                    "Audited Revenue Ledger",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 86, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        includes_services: Some(false),
                        component_id: Some("saas".into()),
                        ..Default::default()
                    },
                ),
                make_item(
                    "ps-fees",
                    "Audited Revenue Ledger",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 24, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        includes_services: Some(true),
                        component_id: Some("services".into()),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 3. One-time fees
    out.push(AdversarialScenario {
        id: "arr-03".into(),
        name: "One-time setup fees included in headline ARR".into(),
        description: format!(
            "{} recurring + {} one-time setup fees.",
            pct(target, 92, 100),
            pct(target, 15, 100)
        ),
        target_policy_hint: "predicate.class_policy.include_one_time_fees".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![
                make_item(
                    "rec-part",
                    "Audited Revenue Ledger",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 92, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        includes_one_time_fees: Some(false),
                        component_id: Some("recurring".into()),
                        ..Default::default()
                    },
                ),
                make_item(
                    "onetime-part",
                    "Audited Revenue Ledger",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 15, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        includes_one_time_fees: Some(true),
                        component_id: Some("onetime".into()),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 4. Signed-not-started contracts
    out.push(AdversarialScenario {
        id: "arr-04".into(),
        name: "Contracts signed before deadline with future start date".into(),
        description: format!(
            "{} live ARR + {} signed contracts starting 15 days post-deadline.",
            pct(target, 89, 100),
            pct(target, 21, 100)
        ),
        target_policy_hint: "predicate.class_policy.signed_not_started_treatment".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![
                make_item(
                    "live-arr",
                    "Audited Revenue Ledger",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 89, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        includes_signed_not_started: Some(false),
                        component_id: Some("live".into()),
                        ..Default::default()
                    },
                ),
                make_item(
                    "future-start-arr",
                    "Audited Revenue Ledger",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 21, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        includes_signed_not_started: Some(true),
                        component_id: Some("signed_future".into()),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 5. Gross vs Net of Discounts
    out.push(AdversarialScenario {
        id: "arr-05".into(),
        name: "Promotional discounts drop net ARR below threshold".into(),
        description: format!(
            "Gross list ARR is {}, net ARR after discounts is {}.",
            pct(target, 106, 100),
            pct(target, 93, 100)
        ),
        target_policy_hint: "predicate.class_policy.discount_treatment".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![make_item(
                "gross-arr",
                "Audited Revenue Ledger",
                SourceType::AuditedFinancials,
                deadline,
                deadline + Duration::days(1),
                EvidenceValue::Money {
                    amount: pct(target, 106, 100),
                    currency: ccy.clone(),
                },
                EvidenceMetadata {
                    arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                    net_of_discounts: Some(false),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: None,
    });

    // 6. Subsidiary consolidation
    out.push(AdversarialScenario {
        id: "arr-06".into(),
        name: "Parent-only vs consolidated subsidiary ARR".into(),
        description: format!(
            "Parent entity has {} ARR; wholly-owned subsidiary adds {}.",
            pct(target, 74, 100),
            pct(target, 36, 100)
        ),
        target_policy_hint: "predicate.class_policy.subsidiary_scope".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![
                make_item(
                    "parent-arr",
                    "Audited Revenue Ledger",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 74, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        entity_scope_consolidated: Some(false),
                        component_id: Some("parent".into()),
                        ..Default::default()
                    },
                ),
                make_item(
                    "sub-arr",
                    "Audited Revenue Ledger",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 36, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        entity_scope_consolidated: Some(true),
                        component_id: Some("subsidiary".into()),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 7. Peak in Q2, dipped below at deadline (observation_mode)
    out.push(AdversarialScenario {
        id: "arr-07".into(),
        name: "Mid-period spike above threshold followed by churn before deadline".into(),
        description: format!(
            "ARR hit {} 60 days before deadline, then churned to {} at deadline.",
            pct(target, 105, 100),
            pct(target, 94, 100)
        ),
        target_policy_hint: "observation.observation_mode".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![
                make_item(
                    "mid-peak",
                    "Audited Revenue Ledger",
                    SourceType::AuditedFinancials,
                    deadline - Duration::days(60),
                    deadline - Duration::days(58),
                    EvidenceValue::Money {
                        amount: pct(target, 105, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        ..Default::default()
                    },
                ),
                make_item(
                    "end-dip",
                    "Audited Revenue Ledger",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 94, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 8. Post-deadline restatement
    out.push(AdversarialScenario {
        id: "arr-08".into(),
        name: "Post-deadline accounting restatement".into(),
        description: format!(
            "Initial deadline report shows {}, restated 12 days later to {}.",
            pct(target, 103, 100),
            pct(target, 95, 100)
        ),
        target_policy_hint: "revision_policy.restatement_handling".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(20)),
            items: vec![
                make_item(
                    "arr-orig",
                    "Audited Revenue Ledger",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 103, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        is_restatement: Some(false),
                        ..Default::default()
                    },
                ),
                make_item(
                    "arr-restated",
                    "Audited Revenue Ledger",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(12),
                    EvidenceValue::Money {
                        amount: pct(target, 95, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        is_restatement: Some(true),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 9. Source conflict: self-attestation vs audited financials
    out.push(AdversarialScenario {
        id: "arr-09".into(),
        name: "Founder attestation conflicts with audited financials".into(),
        description: format!(
            "Company attestation claims {}; audited financials report {}.",
            pct(target, 104, 100),
            pct(target, 97, 100)
        ),
        target_policy_hint: "evidence_policy.conflict_policy".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(5)),
            items: vec![
                make_item(
                    "arr-ceo",
                    "Company Attestation",
                    SourceType::CompanyAttestation,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 104, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        ..Default::default()
                    },
                ),
                make_item(
                    "arr-audit",
                    "Independent Auditor",
                    SourceType::AuditedFinancials,
                    deadline,
                    deadline + Duration::days(4),
                    EvidenceValue::Money {
                        amount: pct(target, 97, 100),
                        currency: ccy,
                    },
                    EvidenceMetadata {
                        arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 10. EUR contracts near FX boundary
    let mut eur_arr = make_item(
        "arr-eur",
        "Audited Revenue Ledger",
        SourceType::AuditedFinancials,
        deadline,
        deadline + Duration::days(1),
        EvidenceValue::Money {
            amount: pct(target, 95, 100),
            currency: "EUR".into(),
        },
        EvidenceMetadata {
            arr_basis: Some(ArrMeasurementBasis::ContractedRecurringRevenue),
            ..Default::default()
        },
    );
    let (fx_src, fx_rule) = spec
        .currency_policy
        .as_ref()
        .map(|cp| {
            (
                cp.fx_source.unwrap_or(FxSource::EcbDailyReference),
                cp.fx_timestamp_rule
                    .unwrap_or(FxTimestampRule::DeadlineClose),
            )
        })
        .unwrap_or((FxSource::EcbDailyReference, FxTimestampRule::DeadlineClose));
    eur_arr.fx_quote = Some(FxQuote {
        base_currency: "EUR".into(),
        quote_currency: "USD".into(),
        rate: Decimal::new(108, 2),
        fx_source: fx_src,
        fx_timestamp: deadline,
        fx_timestamp_rule: Some(fx_rule),
    });
    out.push(AdversarialScenario {
        id: "arr-10".into(),
        name: "EUR-denominated ARR requiring FX conversion".into(),
        description: "95% of threshold in EUR converts above or below 100% depending on FX source."
            .into(),
        target_policy_hint: "currency_policy.fx_source".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(2)),
            items: vec![eur_arr],
        },
        timezone_ambiguity_note: None,
    });

    out
}

fn employment_scenarios(
    spec: &MilestoneSpec,
    deadline: DateTime<Utc>,
    employer: &str,
) -> Vec<AdversarialScenario> {
    let mut out = Vec::with_capacity(12);

    // 1. Offer signed before deadline, start date after
    out.push(AdversarialScenario {
        id: "emp-01".into(),
        name: "Offer signed before deadline, start date after".into(),
        description:
            "Offer letter executed 4 days before deadline; official start date is 12 days after."
                .into(),
        target_policy_hint: "predicate.class_policy.qualifying_event".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(15)),
            items: vec![
                make_item(
                    "offer-1",
                    "HR Verification",
                    SourceType::CompanyAttestation,
                    deadline - Duration::days(4),
                    deadline - Duration::days(4),
                    EvidenceValue::Employment {
                        employed: true,
                        employer: employer.to_string(),
                    },
                    EvidenceMetadata {
                        employment_event: Some(EmploymentQualifyingEvent::SignedOfferLetter),
                        relationship: Some(EmploymentRelationship::FullTimeEmployee),
                        ..Default::default()
                    },
                ),
                make_item(
                    "start-1",
                    "Payroll Registry",
                    SourceType::SecOrRegulatoryFiling,
                    deadline + Duration::days(12),
                    deadline + Duration::days(12),
                    EvidenceValue::Employment {
                        employed: true,
                        employer: employer.to_string(),
                    },
                    EvidenceMetadata {
                        employment_event: Some(EmploymentQualifyingEvent::OfficialStartDate),
                        relationship: Some(EmploymentRelationship::FullTimeEmployee),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 2. Contractor / Consultant role
    out.push(AdversarialScenario {
        id: "emp-02".into(),
        name: "Joins as contractor/consultant rather than FTE".into(),
        description: "Subject signs full-time independent consulting agreement before deadline."
            .into(),
        target_policy_hint: "predicate.class_policy.allowed_relationships".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![make_item(
                "contractor-1",
                "Payroll Registry",
                SourceType::SecOrRegulatoryFiling,
                deadline - Duration::days(5),
                deadline - Duration::days(5),
                EvidenceValue::Employment {
                    employed: true,
                    employer: employer.to_string(),
                },
                EvidenceMetadata {
                    employment_event: Some(EmploymentQualifyingEvent::OfficialStartDate),
                    relationship: Some(EmploymentRelationship::Contractor),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: None,
    });

    // 3. Advisor / Board role
    out.push(AdversarialScenario {
        id: "emp-03".into(),
        name: "Appointed as advisor rather than employee".into(),
        description: "Subject joins as a strategic advisor prior to deadline.".into(),
        target_policy_hint: "predicate.class_policy.allowed_relationships".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![make_item(
                "advisor-1",
                "Press Release",
                SourceType::SecOrRegulatoryFiling,
                deadline - Duration::days(3),
                deadline - Duration::days(3),
                EvidenceValue::Employment {
                    employed: true,
                    employer: employer.to_string(),
                },
                EvidenceMetadata {
                    employment_event: Some(EmploymentQualifyingEvent::OfficialStartDate),
                    relationship: Some(EmploymentRelationship::Advisor),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: None,
    });

    // 4. Acqui-hire
    out.push(AdversarialScenario {
        id: "emp-04".into(),
        name: "Target employer acquires subject's startup".into(),
        description:
            "Subject transfers onto target roster via acqui-hire without new individual offer."
                .into(),
        target_policy_hint: "predicate.class_policy.acquisition_treatment".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![make_item(
                "acquihire-1",
                "Regulatory Filing",
                SourceType::SecOrRegulatoryFiling,
                deadline - Duration::days(2),
                deadline - Duration::days(2),
                EvidenceValue::Employment {
                    employed: true,
                    employer: employer.to_string(),
                },
                EvidenceMetadata {
                    employment_event: Some(EmploymentQualifyingEvent::OfficialStartDate),
                    relationship: Some(EmploymentRelationship::FullTimeEmployee),
                    via_acquisition: Some(true),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: None,
    });

    // 5. Announced/signed but never started (rescinded)
    out.push(AdversarialScenario {
        id: "emp-05".into(),
        name: "Hire announced before deadline, offer rescinded before start".into(),
        description: "Public appointment announced before deadline; subject withdraws and never commences work.".into(),
        target_policy_hint: "predicate.class_policy.announced_never_started_policy".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(14)),
            items: vec![make_item(
                "rescinded-1",
                "Press Release",
                SourceType::SecOrRegulatoryFiling,
                deadline - Duration::days(5),
                deadline + Duration::days(7),
                EvidenceValue::Employment {
                    employed: true,
                    employer: employer.to_string(),
                },
                EvidenceMetadata {
                    employment_event: Some(EmploymentQualifyingEvent::OfficialStartDate),
                    relationship: Some(EmploymentRelationship::FullTimeEmployee),
                    never_commenced: Some(true),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: None,
    });

    // 6. Leave of absence
    out.push(AdversarialScenario {
        id: "emp-06".into(),
        name: "On roster before deadline but on immediate leave of absence".into(),
        description:
            "Official hire date is before deadline, while subject remains on unpaid leave.".into(),
        target_policy_hint: "predicate.class_policy.leave_of_absence_treatment".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![make_item(
                "loa-1",
                "Payroll Registry",
                SourceType::SecOrRegulatoryFiling,
                deadline - Duration::days(2),
                deadline - Duration::days(2),
                EvidenceValue::Employment {
                    employed: true,
                    employer: employer.to_string(),
                },
                EvidenceMetadata {
                    employment_event: Some(EmploymentQualifyingEvent::OfficialStartDate),
                    relationship: Some(EmploymentRelationship::FullTimeEmployee),
                    on_leave_of_absence: Some(true),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: None,
    });

    // 7. Source conflict (Media rumor vs Official filing)
    out.push(AdversarialScenario {
        id: "emp-07".into(),
        name: "Tech media reports hire while official payroll reports negative".into(),
        description:
            "Third-party media reports joining; official employer verification states not employed."
                .into(),
        target_policy_hint: "evidence_policy.conflict_policy".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(2)),
            items: vec![
                make_item(
                    "media-hire",
                    "Tech Press",
                    SourceType::ThirdPartyMedia,
                    deadline - Duration::days(1),
                    deadline - Duration::days(1),
                    EvidenceValue::Employment {
                        employed: true,
                        employer: employer.to_string(),
                    },
                    EvidenceMetadata {
                        employment_event: Some(EmploymentQualifyingEvent::OfficialStartDate),
                        relationship: Some(EmploymentRelationship::FullTimeEmployee),
                        ..Default::default()
                    },
                ),
                make_item(
                    "hr-deny",
                    "Official Employer Filing",
                    SourceType::SecOrRegulatoryFiling,
                    deadline,
                    deadline + Duration::days(1),
                    EvidenceValue::Employment {
                        employed: false,
                        employer: employer.to_string(),
                    },
                    EvidenceMetadata {
                        employment_event: Some(EmploymentQualifyingEvent::OfficialStartDate),
                        relationship: Some(EmploymentRelationship::FullTimeEmployee),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    out
}

fn acquisition_scenarios(
    spec: &MilestoneSpec,
    deadline: DateTime<Utc>,
) -> Vec<AdversarialScenario> {
    let (target, ccy) = money_amount(spec, 100_000_000);
    let mut out = Vec::with_capacity(12);

    // 1. Announced before deadline, closes after
    out.push(AdversarialScenario {
        id: "acq-01".into(),
        name: "Acquisition announced before deadline, closes after".into(),
        description: format!(
            "Signed merger announced at {} 10 days before deadline; regulatory close occurs 45 days later.",
            pct(target, 120, 100)
        ),
        target_policy_hint: "predicate.class_policy.completion_event".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(50)),
            items: vec![
                make_item(
                    "acq-announce",
                    "SEC 8-K",
                    SourceType::SecOrRegulatoryFiling,
                    deadline - Duration::days(10),
                    deadline - Duration::days(10),
                    EvidenceValue::Money {
                        amount: pct(target, 120, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        acquisition_event: Some(AcquisitionCompletionEvent::PublicAnnouncement),
                        valuation_basis: Some(ValuationBasis::EnterpriseValue),
                        consideration_types: vec![ConsiderationType::Cash],
                        structure: Some(TransactionStructure::WholeCompanyMergerOrAcquisition),
                        ..Default::default()
                    },
                ),
                make_item(
                    "acq-close",
                    "SEC 8-K",
                    SourceType::SecOrRegulatoryFiling,
                    deadline + Duration::days(45),
                    deadline + Duration::days(45),
                    EvidenceValue::Money {
                        amount: pct(target, 120, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        acquisition_event: Some(AcquisitionCompletionEvent::LegalClose),
                        valuation_basis: Some(ValuationBasis::EnterpriseValue),
                        consideration_types: vec![ConsiderationType::Cash],
                        structure: Some(TransactionStructure::WholeCompanyMergerOrAcquisition),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 2. Enterprise Value vs Equity Value
    out.push(AdversarialScenario {
        id: "acq-02".into(),
        name: "Equity value exceeds threshold while enterprise value is below".into(),
        description: format!(
            "Equity purchase price is {}; net enterprise value after cash on balance sheet is {}.",
            pct(target, 108, 100),
            pct(target, 91, 100)
        ),
        target_policy_hint: "predicate.class_policy.valuation_basis".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![
                make_item(
                    "acq-eqv",
                    "SEC 8-K",
                    SourceType::SecOrRegulatoryFiling,
                    deadline - Duration::days(5),
                    deadline - Duration::days(5),
                    EvidenceValue::Money {
                        amount: pct(target, 108, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        acquisition_event: Some(AcquisitionCompletionEvent::LegalClose),
                        valuation_basis: Some(ValuationBasis::EquityValue),
                        consideration_types: vec![ConsiderationType::Cash],
                        structure: Some(TransactionStructure::WholeCompanyMergerOrAcquisition),
                        ..Default::default()
                    },
                ),
                make_item(
                    "acq-ev",
                    "SEC 8-K",
                    SourceType::SecOrRegulatoryFiling,
                    deadline - Duration::days(5),
                    deadline - Duration::days(5),
                    EvidenceValue::Money {
                        amount: pct(target, 91, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        acquisition_event: Some(AcquisitionCompletionEvent::LegalClose),
                        valuation_basis: Some(ValuationBasis::EnterpriseValue),
                        consideration_types: vec![ConsiderationType::Cash],
                        structure: Some(TransactionStructure::WholeCompanyMergerOrAcquisition),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 3. Contingent Earn-Out
    out.push(AdversarialScenario {
        id: "acq-03".into(),
        name: "Upfront cash below threshold plus contingent earn-out".into(),
        description: format!(
            "{} upfront cash + {} contingent 2-year revenue earn-out.",
            pct(target, 80, 100),
            pct(target, 35, 100)
        ),
        target_policy_hint: "predicate.class_policy.earn_out_treatment".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![
                make_item(
                    "acq-upfront",
                    "SEC 8-K",
                    SourceType::SecOrRegulatoryFiling,
                    deadline - Duration::days(3),
                    deadline - Duration::days(3),
                    EvidenceValue::Money {
                        amount: pct(target, 80, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        acquisition_event: Some(AcquisitionCompletionEvent::LegalClose),
                        valuation_basis: Some(ValuationBasis::EnterpriseValue),
                        consideration_types: vec![ConsiderationType::Cash],
                        includes_contingent_earnout: Some(false),
                        structure: Some(TransactionStructure::WholeCompanyMergerOrAcquisition),
                        component_id: Some("upfront".into()),
                        ..Default::default()
                    },
                ),
                make_item(
                    "acq-earnout",
                    "SEC 8-K",
                    SourceType::SecOrRegulatoryFiling,
                    deadline - Duration::days(3),
                    deadline - Duration::days(3),
                    EvidenceValue::Money {
                        amount: pct(target, 35, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        acquisition_event: Some(AcquisitionCompletionEvent::LegalClose),
                        valuation_basis: Some(ValuationBasis::EnterpriseValue),
                        consideration_types: vec![ConsiderationType::Cash],
                        includes_contingent_earnout: Some(true),
                        structure: Some(TransactionStructure::WholeCompanyMergerOrAcquisition),
                        component_id: Some("earnout".into()),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 4. Illiquid private stock consideration
    out.push(AdversarialScenario {
        id: "acq-04".into(),
        name: "Majority consideration paid in private acquirer stock".into(),
        description: format!(
            "{} cash + {} illiquid private acquirer stock.",
            pct(target, 65, 100),
            pct(target, 45, 100)
        ),
        target_policy_hint: "predicate.class_policy.consideration_types".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![
                make_item(
                    "acq-cash",
                    "SEC 8-K",
                    SourceType::SecOrRegulatoryFiling,
                    deadline - Duration::days(4),
                    deadline - Duration::days(4),
                    EvidenceValue::Money {
                        amount: pct(target, 65, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        acquisition_event: Some(AcquisitionCompletionEvent::LegalClose),
                        valuation_basis: Some(ValuationBasis::EnterpriseValue),
                        consideration_types: vec![ConsiderationType::Cash],
                        structure: Some(TransactionStructure::WholeCompanyMergerOrAcquisition),
                        component_id: Some("cash".into()),
                        ..Default::default()
                    },
                ),
                make_item(
                    "acq-privstock",
                    "SEC 8-K",
                    SourceType::SecOrRegulatoryFiling,
                    deadline - Duration::days(4),
                    deadline - Duration::days(4),
                    EvidenceValue::Money {
                        amount: pct(target, 45, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        acquisition_event: Some(AcquisitionCompletionEvent::LegalClose),
                        valuation_basis: Some(ValuationBasis::EnterpriseValue),
                        consideration_types: vec![ConsiderationType::PrivateStock],
                        structure: Some(TransactionStructure::WholeCompanyMergerOrAcquisition),
                        component_id: Some("private_stock".into()),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 5. Partial acquisition / asset sale
    out.push(AdversarialScenario {
        id: "acq-05".into(),
        name: "51% majority stake / asset carve-out instead of full merger".into(),
        description: format!(
            "Acquirer purchases 51% controlling stake for {}.",
            pct(target, 105, 100)
        ),
        target_policy_hint: "predicate.class_policy.transaction_structures".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![make_item(
                "acq-partial",
                "SEC 8-K",
                SourceType::SecOrRegulatoryFiling,
                deadline - Duration::days(4),
                deadline - Duration::days(4),
                EvidenceValue::Money {
                    amount: pct(target, 105, 100),
                    currency: ccy.clone(),
                },
                EvidenceMetadata {
                    acquisition_event: Some(AcquisitionCompletionEvent::LegalClose),
                    valuation_basis: Some(ValuationBasis::EnterpriseValue),
                    consideration_types: vec![ConsiderationType::Cash],
                    structure: Some(TransactionStructure::MajorityEquityStake),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: None,
    });

    // 6. Deal terminated after announcement
    out.push(AdversarialScenario {
        id: "acq-06".into(),
        name: "Deal announced before deadline, terminated on antitrust grounds".into(),
        description: format!(
            "{} acquisition announced before deadline, later terminated.",
            pct(target, 115, 100)
        ),
        target_policy_hint: "predicate.class_policy.termination_after_announcement_policy".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(20)),
            items: vec![make_item(
                "acq-term",
                "SEC 8-K",
                SourceType::SecOrRegulatoryFiling,
                deadline - Duration::days(15),
                deadline + Duration::days(15),
                EvidenceValue::Money {
                    amount: pct(target, 115, 100),
                    currency: ccy.clone(),
                },
                EvidenceMetadata {
                    acquisition_event: Some(AcquisitionCompletionEvent::PublicAnnouncement),
                    valuation_basis: Some(ValuationBasis::EnterpriseValue),
                    consideration_types: vec![ConsiderationType::Cash],
                    structure: Some(TransactionStructure::WholeCompanyMergerOrAcquisition),
                    terminated_before_close: Some(true),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: None,
    });

    // 7. Conflicting media rumor vs regulatory filing
    out.push(AdversarialScenario {
        id: "acq-07".into(),
        name: "Media leaks higher valuation than definitive SEC filing".into(),
        description: format!(
            "Financial press reports {}; SEC 8-K discloses {}.",
            pct(target, 105, 100),
            pct(target, 94, 100)
        ),
        target_policy_hint: "evidence_policy.conflict_policy".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(2)),
            items: vec![
                make_item(
                    "acq-media",
                    "Financial Media",
                    SourceType::ThirdPartyMedia,
                    deadline - Duration::days(2),
                    deadline - Duration::days(2),
                    EvidenceValue::Money {
                        amount: pct(target, 105, 100),
                        currency: ccy.clone(),
                    },
                    EvidenceMetadata {
                        acquisition_event: Some(AcquisitionCompletionEvent::LegalClose),
                        valuation_basis: Some(ValuationBasis::EnterpriseValue),
                        consideration_types: vec![ConsiderationType::Cash],
                        structure: Some(TransactionStructure::WholeCompanyMergerOrAcquisition),
                        ..Default::default()
                    },
                ),
                make_item(
                    "acq-sec",
                    "SEC 8-K",
                    SourceType::SecOrRegulatoryFiling,
                    deadline - Duration::days(2),
                    deadline - Duration::days(1),
                    EvidenceValue::Money {
                        amount: pct(target, 94, 100),
                        currency: ccy,
                    },
                    EvidenceMetadata {
                        acquisition_event: Some(AcquisitionCompletionEvent::LegalClose),
                        valuation_basis: Some(ValuationBasis::EnterpriseValue),
                        consideration_types: vec![ConsiderationType::Cash],
                        structure: Some(TransactionStructure::WholeCompanyMergerOrAcquisition),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    out
}

fn audience_scenarios(
    spec: &MilestoneSpec,
    deadline: DateTime<Utc>,
    platform: &str,
) -> Vec<AdversarialScenario> {
    let target = count_target(spec, 1_000_000);
    let above = target + (target / 100).max(1_000);
    let below = target.saturating_sub((target / 200).max(500));
    let mut out = Vec::with_capacity(12);

    // 1. Momentary crossing before deadline, below at deadline
    out.push(AdversarialScenario {
        id: "aud-01".into(),
        name: "Momentary crossing before deadline, drops below at snapshot".into(),
        description: format!(
            "Channel crosses {} 3 days before deadline, then sits at {} at deadline.",
            above, below
        ),
        target_policy_hint: "predicate.class_policy.crossing_rule".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::hours(1)),
            items: vec![
                make_item(
                    "aud-peak",
                    "Platform Data API",
                    SourceType::OfficialPlatformApi,
                    deadline - Duration::days(3),
                    deadline - Duration::days(3),
                    EvidenceValue::Count { count: above },
                    EvidenceMetadata {
                        platform: Some(platform.into()),
                        is_momentary_peak: Some(true),
                        ..Default::default()
                    },
                ),
                make_item(
                    "aud-end",
                    "Platform Data API",
                    SourceType::OfficialPlatformApi,
                    deadline,
                    deadline,
                    EvidenceValue::Count { count: below },
                    EvidenceMetadata {
                        platform: Some(platform.into()),
                        is_momentary_peak: Some(false),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 2. Post-deadline bot purge restatement
    out.push(AdversarialScenario {
        id: "aud-02".into(),
        name: "Bot purge drops subscriber count below threshold".into(),
        description: format!(
            "Count reads {} at deadline, then drops to {} after platform spam purge 48h later.",
            above, below
        ),
        target_policy_hint: "revision_policy.restatement_handling".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(4)),
            items: vec![
                make_item(
                    "aud-raw",
                    "Platform Data API",
                    SourceType::OfficialPlatformApi,
                    deadline,
                    deadline,
                    EvidenceValue::Count { count: above },
                    EvidenceMetadata {
                        platform: Some(platform.into()),
                        after_bot_purge: Some(false),
                        is_restatement: Some(false),
                        ..Default::default()
                    },
                ),
                make_item(
                    "aud-purged",
                    "Platform Data API",
                    SourceType::OfficialPlatformApi,
                    deadline,
                    deadline + Duration::days(2),
                    EvidenceValue::Count { count: below },
                    EvidenceMetadata {
                        platform: Some(platform.into()),
                        after_bot_purge: Some(true),
                        is_restatement: Some(true),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 3. Deleted/suspended channel
    out.push(AdversarialScenario {
        id: "aud-03".into(),
        name: "Channel suspended/deleted prior to deadline".into(),
        description: "Creator channel is suspended 2 days before deadline after previously approaching threshold.".into(),
        target_policy_hint: "predicate.class_policy.deleted_or_suspended_channel_policy".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![make_item(
                "aud-del",
                "Platform Data API",
                SourceType::OfficialPlatformApi,
                deadline - Duration::days(2),
                deadline - Duration::days(2),
                EvidenceValue::Count { count: above },
                EvidenceMetadata {
                    platform: Some(platform.into()),
                    channel_deleted: Some(true),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: None,
    });

    // 4. API exact count vs third-party tracker / UI abbreviation
    out.push(AdversarialScenario {
        id: "aud-04".into(),
        name: "Third-party tracker diverges from official platform API".into(),
        description: format!(
            "Third-party tracker estimates {}, while official platform API reports {}.",
            above, below
        ),
        target_policy_hint: "evidence_policy.conflict_policy".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::hours(1)),
            items: vec![
                make_item(
                    "aud-tracker",
                    "Third-Party Tracker",
                    SourceType::ThirdPartyMedia,
                    deadline,
                    deadline,
                    EvidenceValue::Count { count: above },
                    EvidenceMetadata {
                        platform: Some(platform.into()),
                        ..Default::default()
                    },
                ),
                make_item(
                    "aud-api",
                    "Platform Data API",
                    SourceType::OfficialPlatformApi,
                    deadline,
                    deadline,
                    EvidenceValue::Count { count: below },
                    EvidenceMetadata {
                        platform: Some(platform.into()),
                        ..Default::default()
                    },
                ),
            ],
        },
        timezone_ambiguity_note: None,
    });

    // 5. Regional midnight boundary
    out.push(AdversarialScenario {
        id: "aud-05".into(),
        name: "Regional timestamp crossing (23:55 creator time vs UTC)".into(),
        description: "Threshold crossed at 23:55 local time on deadline date (after UTC midnight)."
            .into(),
        target_policy_hint: "observation.timezone".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::days(1)),
            items: vec![make_item(
                "aud-tz",
                "Platform Data API",
                SourceType::OfficialPlatformApi,
                deadline + Duration::hours(4) + Duration::minutes(55),
                deadline + Duration::hours(5),
                EvidenceValue::Count { count: above },
                EvidenceMetadata {
                    platform: Some(platform.into()),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: Some("23:55 EST is 04:55 UTC next day".into()),
    });

    // 6. Exact threshold boundary (target - 1 vs target)
    out.push(AdversarialScenario {
        id: "aud-06".into(),
        name: "Exact deadline second at exact threshold".into(),
        description: format!("Count is exactly {} at exact deadline second.", target),
        target_policy_hint: "observation.deadline_inclusivity".into(),
        bundle: EvidenceBundle {
            spec_id: spec.id.clone(),
            evaluation_time: Some(deadline + Duration::seconds(5)),
            items: vec![make_item(
                "aud-exact",
                "Platform Data API",
                SourceType::OfficialPlatformApi,
                deadline,
                deadline,
                EvidenceValue::Count { count: target },
                EvidenceMetadata {
                    platform: Some(platform.into()),
                    ..Default::default()
                },
            )],
        },
        timezone_ambiguity_note: None,
    });

    out
}
