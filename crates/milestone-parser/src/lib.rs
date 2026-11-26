use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use milestone_core::{
    AcquisitionPolicy, ArrPolicy, AudiencePolicy, ClassPolicy, Comparator, EmploymentPolicy,
    EvidencePolicy, FundraisingPolicy, InvalidationPolicy, Metric, MilestoneSpec, Money,
    ObservationWindow, Predicate, RevisionPolicy, Subject, SubjectType, Threshold,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ParseStatus {
    ParsedWithGaps,
    Unrecognized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseOutput {
    pub status: ParseStatus,
    pub spec: Option<MilestoneSpec>,
    pub inferred_fields: Vec<String>,
    pub unresolved_gaps: Vec<String>,
    pub note: String,
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("empty milestone prompt")]
    EmptyInput,
}

/// Parses a modest set of structured natural-language milestone headlines into a
/// canonical `MilestoneSpec` while deliberately leaving undefined semantics (`None`)
/// rather than silently inventing methodology, evidence, FX, timezone, or revision rules.
pub fn parse_headline(input: &str) -> Result<ParseOutput, ParseError> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    let cleaned = raw.trim_end_matches('?').trim();
    let lower = cleaned.to_lowercase();

    // Detect deadline phrase ("by ..." or "before ...")
    let (body, deadline_opt, date_note) = extract_deadline_clause(cleaned);

    // 1. ARR: "Will <Subject> reach <Money> ARR"
    if lower.contains(" arr")
        || lower.ends_with("arr")
        || lower.contains("annual recurring revenue")
    {
        if let Some(subject_name) =
            extract_subject_before_verb(&body, &["reach", "hit", "achieve", "exceed"])
        {
            if let Some((amount, currency)) = extract_money(&body) {
                let id = slugify(&format!("{}-arr-{}", subject_name, amount));
                let spec = build_spec(
                    id,
                    raw.to_string(),
                    Subject {
                        name: subject_name,
                        subject_type: SubjectType::Company,
                        jurisdiction: None,
                        entity_scope: None,
                        identifiers: BTreeMap::new(),
                    },
                    Predicate {
                        metric: Metric::AnnualRecurringRevenue,
                        comparator: Comparator::GreaterThanOrEqual,
                        threshold: Threshold::Money(Money { amount, currency }),
                        class_policy: ClassPolicy::Arr(ArrPolicy::default()),
                    },
                    deadline_opt,
                );
                return Ok(parsed_with_standard_gaps(
                    spec,
                    vec![
                        "predicate.class_policy.measurement_basis".into(),
                        "predicate.class_policy.include_services_revenue".into(),
                        "predicate.class_policy.signed_not_started_treatment".into(),
                        "currency_policy".into(),
                        "evidence_policy.primary_source".into(),
                        "evidence_policy.conflict_policy".into(),
                        "observation.timezone".into(),
                        "revision_policy.restatement_handling".into(),
                    ],
                    date_note,
                ));
            }
        }
    }

    // 2. Fundraising: "Will <Subject> raise <Money>"
    if lower.contains(" raise ") || lower.contains(" raised ") {
        if let Some(subject_name) = extract_subject_before_verb(&body, &["raise", "raised"]) {
            if let Some((amount, currency)) = extract_money(&body) {
                let id = slugify(&format!("{}-raise-{}", subject_name, amount));
                let spec = build_spec(
                    id,
                    raw.to_string(),
                    Subject {
                        name: subject_name,
                        subject_type: SubjectType::Company,
                        jurisdiction: None,
                        entity_scope: None,
                        identifiers: BTreeMap::new(),
                    },
                    Predicate {
                        metric: Metric::FundraisingTotal,
                        comparator: Comparator::GreaterThanOrEqual,
                        threshold: Threshold::Money(Money { amount, currency }),
                        class_policy: ClassPolicy::Fundraising(FundraisingPolicy::default()),
                    },
                    deadline_opt,
                );
                return Ok(parsed_with_standard_gaps(
                    spec,
                    vec![
                        "predicate.class_policy.completion_event".into(),
                        "predicate.class_policy.allowed_instruments".into(),
                        "predicate.class_policy.include_secondary".into(),
                        "currency_policy".into(),
                        "evidence_policy.primary_source".into(),
                        "evidence_policy.conflict_policy".into(),
                        "observation.timezone".into(),
                        "revision_policy.restatement_handling".into(),
                    ],
                    date_note,
                ));
            }
        }
    }

    // 3. Employment: "Will <Person> join <Company>"
    if lower.contains(" join ") {
        if let Some((subject_name, employer)) = extract_join_entities(&body) {
            let id = slugify(&format!("{}-join-{}", subject_name, employer));
            let spec = build_spec(
                id,
                raw.to_string(),
                Subject {
                    name: subject_name,
                    subject_type: SubjectType::Person,
                    jurisdiction: None,
                    entity_scope: None,
                    identifiers: BTreeMap::new(),
                },
                Predicate {
                    metric: Metric::EmploymentStatus,
                    comparator: Comparator::Achieved,
                    threshold: Threshold::Boolean(true),
                    class_policy: ClassPolicy::Employment(EmploymentPolicy {
                        target_employer: employer,
                        ..Default::default()
                    }),
                },
                deadline_opt,
            );
            return Ok(parsed_with_standard_gaps(
                spec,
                vec![
                    "predicate.class_policy.qualifying_event".into(),
                    "predicate.class_policy.allowed_relationships".into(),
                    "predicate.class_policy.acquisition_treatment".into(),
                    "evidence_policy.primary_source".into(),
                    "evidence_policy.conflict_policy".into(),
                    "observation.timezone".into(),
                    "revision_policy.restatement_handling".into(),
                ],
                date_note,
            ));
        }
    }

    // 4. Acquisition: "Will <Company> be acquired for <Money>"
    if lower.contains("acquired") {
        if let Some(subject_name) =
            extract_subject_before_verb(&body, &["be acquired", "get acquired"])
        {
            if let Some((amount, currency)) = extract_money(&body) {
                let id = slugify(&format!("{}-acquired-{}", subject_name, amount));
                let spec = build_spec(
                    id,
                    raw.to_string(),
                    Subject {
                        name: subject_name,
                        subject_type: SubjectType::Company,
                        jurisdiction: None,
                        entity_scope: None,
                        identifiers: BTreeMap::new(),
                    },
                    Predicate {
                        metric: Metric::AcquisitionValuation,
                        comparator: Comparator::GreaterThanOrEqual,
                        threshold: Threshold::Money(Money { amount, currency }),
                        class_policy: ClassPolicy::Acquisition(AcquisitionPolicy::default()),
                    },
                    deadline_opt,
                );
                return Ok(parsed_with_standard_gaps(
                    spec,
                    vec![
                        "predicate.class_policy.completion_event".into(),
                        "predicate.class_policy.valuation_basis".into(),
                        "predicate.class_policy.earn_out_treatment".into(),
                        "predicate.class_policy.termination_after_announcement_policy".into(),
                        "currency_policy".into(),
                        "evidence_policy.primary_source".into(),
                        "observation.timezone".into(),
                    ],
                    date_note,
                ));
            }
        }
    }

    // 5. Audience: "Will <Creator> reach <Count> <Platform> subscribers/followers"
    if lower.contains("subscribers") || lower.contains("followers") {
        if let Some(subject_name) =
            extract_subject_before_verb(&body, &["reach", "hit", "gain", "exceed"])
        {
            if let Some(count) = extract_count(&body) {
                let platform = if lower.contains("youtube") {
                    Some("youtube".to_string())
                } else if lower.contains("substack") {
                    Some("substack".to_string())
                } else if lower.contains("twitch") {
                    Some("twitch".to_string())
                } else if lower.contains("x ") || lower.contains("twitter") {
                    Some("x".to_string())
                } else {
                    None
                };
                let id = slugify(&format!("{}-audience-{}", subject_name, count));
                let spec = build_spec(
                    id,
                    raw.to_string(),
                    Subject {
                        name: subject_name,
                        subject_type: SubjectType::Channel,
                        jurisdiction: None,
                        entity_scope: None,
                        identifiers: BTreeMap::new(),
                    },
                    Predicate {
                        metric: Metric::AudienceCount,
                        comparator: Comparator::GreaterThanOrEqual,
                        threshold: Threshold::Count(count),
                        class_policy: ClassPolicy::Audience(AudiencePolicy {
                            platform,
                            ..Default::default()
                        }),
                    },
                    deadline_opt,
                );
                return Ok(parsed_with_standard_gaps(
                    spec,
                    vec![
                        "predicate.class_policy.channel_scope".into(),
                        "predicate.class_policy.crossing_rule".into(),
                        "predicate.class_policy.count_source_mode".into(),
                        "observation.observation_mode".into(),
                        "observation.timezone".into(),
                        "evidence_policy.primary_source".into(),
                        "revision_policy.restatement_handling".into(),
                    ],
                    date_note,
                ));
            }
        }
    }

    Ok(ParseOutput {
        status: ParseStatus::Unrecognized,
        spec: None,
        inferred_fields: vec![],
        unresolved_gaps: vec![],
        note: "Headline did not match one of the 5 supported milestone patterns (ARR, Fundraising, Employment, Acquisition, Audience).".into(),
    })
}

fn build_spec(
    id: String,
    title: String,
    subject: Subject,
    predicate: Predicate,
    deadline: Option<DateTime<Utc>>,
) -> MilestoneSpec {
    MilestoneSpec {
        schema_version: "1.0".to_string(),
        id,
        title,
        subject,
        predicate,
        observation: ObservationWindow {
            start: None,
            deadline,
            timezone: None,
            deadline_inclusivity: None,
            observation_mode: None,
            cutoff_precision: None,
        },
        currency_policy: None,
        evidence_policy: EvidencePolicy::default(),
        revision_policy: RevisionPolicy::default(),
        invalidation_policy: InvalidationPolicy::default(),
    }
}

fn parsed_with_standard_gaps(
    spec: MilestoneSpec,
    mut gaps: Vec<String>,
    date_note: Option<String>,
) -> ParseOutput {
    if spec.observation.deadline.is_none() {
        gaps.insert(0, "observation.deadline".into());
    }
    if let Some(n) = date_note {
        gaps.push(n);
    }
    ParseOutput {
        status: ParseStatus::ParsedWithGaps,
        inferred_fields: vec![
            "subject.name".into(),
            "predicate.metric".into(),
            "predicate.threshold".into(),
        ],
        unresolved_gaps: gaps,
        spec: Some(spec),
        note: "Parsed headline into partial AST; semantic, evidentiary, FX, timezone, and revision policies intentionally remain unresolved.".into(),
    }
}

fn extract_subject_before_verb(body: &str, verbs: &[&str]) -> Option<String> {
    let trimmed = body.trim();
    let without_will = if trimmed.to_lowercase().starts_with("will ") {
        trimmed[5..].trim()
    } else {
        trimmed
    };
    let lower = without_will.to_lowercase();
    for verb in verbs {
        let needle = format!(" {} ", verb);
        if let Some(idx) = lower.find(&needle) {
            let subj = without_will[..idx].trim();
            if !subj.is_empty() {
                return Some(subj.to_string());
            }
        }
    }
    None
}

fn extract_join_entities(body: &str) -> Option<(String, String)> {
    let trimmed = body.trim();
    let without_will = if trimmed.to_lowercase().starts_with("will ") {
        trimmed[5..].trim()
    } else {
        trimmed
    };
    let lower = without_will.to_lowercase();
    let idx = lower.find(" join ")?;
    let subject = without_will[..idx].trim().to_string();
    let rest = without_will[idx + 6..].trim();
    // Employer is the remainder up to any trailing qualifier ("as", "in", etc.)
    let employer = rest
        .split_whitespace()
        .take_while(|w| {
            let lw = w.to_lowercase();
            lw != "as" && lw != "in" && lw != "on"
        })
        .collect::<Vec<_>>()
        .join(" ");
    if subject.is_empty() || employer.is_empty() {
        None
    } else {
        Some((subject, employer))
    }
}

fn extract_money(text: &str) -> Option<(Decimal, Option<String>)> {
    for raw_tok in text.split_whitespace() {
        let tok = raw_tok.trim_matches(|c: char| matches!(c, ',' | '.' | '?' | ';'));
        let (currency, rest) = if let Some(r) = tok.strip_prefix('$') {
            (Some("USD".to_string()), r)
        } else if let Some(r) = tok.strip_prefix('€') {
            (Some("EUR".to_string()), r)
        } else if let Some(r) = tok.strip_prefix('£') {
            (Some("GBP".to_string()), r)
        } else {
            continue;
        };

        if let Some(val) = parse_scaled_decimal(rest) {
            return Some((val, currency));
        }
    }
    None
}

fn extract_count(text: &str) -> Option<u64> {
    for raw_tok in text.split_whitespace() {
        let tok = raw_tok.trim_matches(|c: char| matches!(c, ',' | '.' | '?' | ';'));
        if tok.starts_with('$') || tok.starts_with('€') || tok.starts_with('£') {
            continue;
        }
        if let Some(d) = parse_scaled_decimal(tok) {
            if let Ok(u) = u64::try_from(d.trunc()) {
                if u > 0 {
                    return Some(u);
                }
            }
        }
    }
    None
}

fn parse_scaled_decimal(token: &str) -> Option<Decimal> {
    let clean = token.replace(',', "");
    if clean.is_empty() {
        return None;
    }
    let last = clean.chars().last()?;
    let (num_str, mult) = match last.to_ascii_uppercase() {
        'K' => (&clean[..clean.len() - 1], Decimal::from(1_000)),
        'M' => (&clean[..clean.len() - 1], Decimal::from(1_000_000)),
        'B' => (&clean[..clean.len() - 1], Decimal::from(1_000_000_000)),
        _ => (clean.as_str(), Decimal::ONE),
    };
    let d = Decimal::from_str(num_str).ok()?;
    Some((d * mult).normalize())
}

fn extract_deadline_clause(cleaned: &str) -> (String, Option<DateTime<Utc>>, Option<String>) {
    let lower = cleaned.to_lowercase();
    let split_pos = lower.rfind(" by ").or_else(|| lower.rfind(" before "));
    let Some(idx) = split_pos else {
        return (cleaned.to_string(), None, None);
    };

    let keyword_len = if lower[idx..].starts_with(" by ") {
        4
    } else {
        8
    };
    let body = cleaned[..idx].trim().to_string();
    let date_part = cleaned[idx + keyword_len..].trim();
    let (dt, note) = parse_date_phrase(date_part);
    (body, dt, note)
}

fn parse_date_phrase(phrase: &str) -> (Option<DateTime<Utc>>, Option<String>) {
    let clean = phrase.trim().trim_end_matches(&['?', '.', ','][..]);
    // ISO date YYYY-MM-DD
    if let Ok(nd) = NaiveDate::parse_from_str(clean, "%Y-%m-%d") {
        let dt = nd
            .and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap())
            .and_utc();
        return (Some(dt), None);
    }

    // Year only e.g. "2028"
    if clean.len() == 4 {
        if let Ok(year) = clean.parse::<i32>() {
            if let Some(nd) = NaiveDate::from_ymd_opt(year, 12, 31) {
                let dt = nd
                    .and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap())
                    .and_utc();
                return (
                    Some(dt),
                    Some(format!(
                        "observation.deadline (year-only phrase `{}` assumed {}-12-31T23:59:59Z)",
                        clean, year
                    )),
                );
            }
        }
    }

    // Month Day[, Year]
    let tokens: Vec<&str> = clean
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.len() >= 2 {
        if let Some(month) = parse_month(tokens[0]) {
            if let Ok(day) = tokens[1].parse::<u32>() {
                if tokens.len() >= 3 {
                    if let Ok(year) = tokens[2].parse::<i32>() {
                        if let Some(nd) = NaiveDate::from_ymd_opt(year, month, day) {
                            let dt = nd
                                .and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap())
                                .and_utc();
                            return (Some(dt), None);
                        }
                    }
                } else {
                    // Year omitted (e.g. "December 31"): infer current/next year and warn
                    let year = 2027;
                    if let Some(nd) = NaiveDate::from_ymd_opt(year, month, day) {
                        let dt = nd
                            .and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap())
                            .and_utc();
                        return (
                            Some(dt),
                            Some(format!(
                                "observation.deadline (year omitted in `{}`; placeholder {} requires explicit confirmation)",
                                clean, year
                            )),
                        );
                    }
                }
            }
        }
    }

    (
        None,
        Some(format!(
            "observation.deadline (could not parse `{}`)",
            clean
        )),
    )
}

fn parse_month(s: &str) -> Option<u32> {
    match s.to_lowercase().as_str() {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "sept" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}
