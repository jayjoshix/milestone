use milestone_adversarial::{AttackReport, ScenarioAttackResult, ScenarioStatus};
use milestone_core::MarketClass;

pub fn friendly_rule_name(path: &str, class: MarketClass) -> String {
    let prefix = match class {
        MarketClass::Arr => "arr",
        MarketClass::Fundraising => "fundraising",
        MarketClass::Employment => "employment",
        MarketClass::Acquisition => "acquisition",
        MarketClass::Audience => "audience",
    };

    if let Some(rest) = path.strip_prefix("predicate.class_policy.") {
        return format!("{}.{}", prefix, rest);
    }
    if let Some(rest) = path.strip_prefix("evidence_policy.") {
        return format!("evidence.{}", rest);
    }
    if let Some(rest) = path.strip_prefix("currency_policy.") {
        return format!("currency.{}", rest);
    }
    if path == "currency_policy" {
        return "currency.fx_policy".to_string();
    }
    if let Some(rest) = path.strip_prefix("revision_policy.") {
        return format!("revision.{}", rest);
    }
    path.to_string()
}

pub fn friendly_question(path: &str, class: MarketClass, fallback: &str) -> String {
    match (class, path) {
        (MarketClass::Fundraising, "predicate.class_policy.completion_event") => {
            "What event counts as \"raising\" capital?".into()
        }
        (MarketClass::Fundraising, "predicate.class_policy.allowed_instruments") => {
            "Which financing types count?".into()
        }
        (MarketClass::Fundraising, "predicate.class_policy.include_secondary") => {
            "Do secondary share sales count toward the raise?".into()
        }
        (MarketClass::Arr, "predicate.class_policy.measurement_basis") => {
            "What accounting basis defines ARR?".into()
        }
        (MarketClass::Arr, "predicate.class_policy.include_services_revenue") => {
            "Does professional services revenue count toward ARR?".into()
        }
        (MarketClass::Arr, "predicate.class_policy.signed_not_started_treatment") => {
            "Do signed contracts not yet live count toward ARR?".into()
        }
        (MarketClass::Employment, "predicate.class_policy.qualifying_event") => {
            "What event counts as \"joining\"?".into()
        }
        (MarketClass::Employment, "predicate.class_policy.allowed_relationships") => {
            "Do contractors or advisors count as joining?".into()
        }
        (MarketClass::Acquisition, "predicate.class_policy.completion_event") => {
            "Does announcement or legal close control?".into()
        }
        (MarketClass::Acquisition, "predicate.class_policy.valuation_basis") => {
            "Is the threshold enterprise value or equity value?".into()
        }
        (MarketClass::Acquisition, "predicate.class_policy.earn_out_treatment") => {
            "Do contingent earn-outs count toward the deal price?".into()
        }
        (MarketClass::Audience, "predicate.class_policy.crossing_rule") => {
            "Does a momentary spike count or only the deadline snapshot?".into()
        }
        (MarketClass::Audience, "predicate.class_policy.count_source_mode") => {
            "Does displayed count or official API count control?".into()
        }
        (_, "evidence_policy.primary_source") => "What evidence decides the result?".into(),
        (_, "evidence_policy.conflict_policy") => "Which source wins if reports disagree?".into(),
        (_, "currency_policy") | (_, "currency_policy.fx_source") => {
            "How are non-USD amounts converted?".into()
        }
        (_, "observation.timezone") => "Which timezone controls the deadline?".into(),
        (_, "observation.observation_mode") => {
            "Must the threshold hold at the deadline or at any prior point?".into()
        }
        (_, "revision_policy.restatement_handling") => {
            "Do post-deadline restatements change the outcome?".into()
        }
        _ => fallback.trim_end_matches('.').to_string(),
    }
}

pub fn candidate_choices(path: &str, class: MarketClass) -> Vec<&'static str> {
    match (class, path) {
        (MarketClass::Fundraising, "predicate.class_policy.completion_event") => vec![
            "term_sheet_signed",
            "definitive_agreement_signed",
            "funds_received_in_bank",
            "legal_close_and_funds_received",
        ],
        (MarketClass::Fundraising, "predicate.class_policy.allowed_instruments") => {
            vec!["priced_equity", "safe", "convertible_note", "venture_debt"]
        }
        (MarketClass::Arr, "predicate.class_policy.measurement_basis") => vec![
            "contracted_recurring_revenue",
            "billed_recurring_revenue",
            "collected_cash_recurring_revenue",
            "annualized_mrr",
        ],
        (MarketClass::Arr, "predicate.class_policy.include_services_revenue") => {
            vec!["false (software recurring only)", "true (include services)"]
        }
        (MarketClass::Employment, "predicate.class_policy.qualifying_event") => vec![
            "signed_offer_letter",
            "official_start_date",
            "first_payroll_cycle",
        ],
        (MarketClass::Acquisition, "predicate.class_policy.completion_event") => vec![
            "public_announcement",
            "definitive_agreement_signed",
            "legal_close",
        ],
        (MarketClass::Acquisition, "predicate.class_policy.earn_out_treatment") => vec![
            "exclude_contingent_earn_outs",
            "include_maximum_earn_out",
            "include_fair_value_at_close",
        ],
        (MarketClass::Acquisition, "predicate.class_policy.valuation_basis") => {
            vec!["enterprise_value", "equity_value", "headline_consideration"]
        }
        (MarketClass::Audience, "predicate.class_policy.crossing_rule") => vec![
            "momentary_crossing",
            "deadline_timestamp_snapshot",
            "end_of_day_snapshot",
        ],
        (_, "evidence_policy.conflict_policy") => vec![
            "use_highest_precedence_source (with explicit precedence_order)",
            "require_corroboration",
            "unresolved_on_conflict",
        ],
        (_, "currency_policy.fx_source") | (_, "currency_policy") => {
            vec![
                "ecb_daily_reference",
                "federal_reserve_h10",
                "bloomberg_fixing",
            ]
        }
        (_, "observation.timezone") => vec!["UTC", "America/Los_Angeles", "America/New_York"],
        _ => vec![],
    }
}

pub fn pick_hero_scenario(
    report: &AttackReport,
    class: MarketClass,
) -> Option<&ScenarioAttackResult> {
    let preferred_ids: &[&str] = match class {
        MarketClass::Fundraising => &["fund-01", "fund-02", "fund-04", "fund-06"],
        MarketClass::Arr => &["arr-01", "arr-02", "arr-04", "arr-05"],
        MarketClass::Employment => &["emp-01", "emp-02", "emp-04"],
        MarketClass::Acquisition => &["acq-03", "acq-01", "acq-02"],
        MarketClass::Audience => &["aud-01", "aud-02", "aud-04"],
    };

    for id in preferred_ids {
        if let Some(sc) = report
            .results
            .iter()
            .find(|r| r.scenario_id == *id && r.status == ScenarioStatus::Ambiguous)
        {
            return Some(sc);
        }
    }

    report
        .results
        .iter()
        .find(|r| r.status == ScenarioStatus::Ambiguous && r.divergent_interpretations.len() >= 2)
        .or_else(|| {
            report
                .results
                .iter()
                .find(|r| r.status == ScenarioStatus::Ambiguous)
        })
}
