use chrono::{DateTime, Duration, Utc};
use milestone_core::{
    rule_hash_hex, AcquisitionCompletionEvent, AvailabilityPolicy, BotPurgePolicy,
    CancellationPolicy, ClassPolicy, Comparator, ConditionalCommitmentPolicy, ConflictPolicy,
    CrossingRule, DeadlineInclusivity, DeletedChannelPolicy, EmploymentQualifyingEvent,
    EvidenceBundle, EvidenceItem, EvidenceValue, FundraisingCompletionEvent, MilestoneSpec,
    ObservationMode, Outcome, Resolution, ResolutionError, ResolutionReasonCode,
    RestatementHandling, SourceType, TerminationPolicy, Threshold, UnwindPolicy,
};
use rust_decimal::Decimal;
use std::collections::BTreeMap;

/// Deterministically resolves a `MilestoneSpec` against an `EvidenceBundle`.
///
/// The resolver never guesses missing semantics. When required policy fields,
/// FX rules, source conflict precedence, or evidence are absent, it returns
/// `Outcome::Unresolved` with a structured reason code and execution trace.
pub fn resolve(
    spec: &MilestoneSpec,
    evidence: &EvidenceBundle,
) -> Result<Resolution, ResolutionError> {
    if evidence.spec_id != spec.id {
        return Err(ResolutionError::SpecIdMismatch {
            spec: spec.id.clone(),
            bundle: evidence.spec_id.clone(),
        });
    }

    if spec.predicate.metric.market_class() != spec.predicate.class_policy.market_class() {
        return Err(ResolutionError::InvalidSpec(
            "predicate.metric does not match predicate.class_policy".into(),
        ));
    }

    let hash_hex = rule_hash_hex(spec);
    let mut trace = Vec::new();
    trace.push(format!("Evaluating spec `{}` ({})", spec.id, hash_hex));

    let deadline = match spec.observation.deadline {
        Some(d) => d,
        None => {
            return Ok(unresolved(
                hash_hex,
                ResolutionReasonCode::MissingRequiredPolicy,
                "Cannot resolve: `observation.deadline` is undefined.",
                vec![],
                trace,
            ));
        }
    };

    if let Some(start) = spec.observation.start {
        if start >= deadline {
            return Err(ResolutionError::InvalidSpec(
                "observation.start must be earlier than observation.deadline".into(),
            ));
        }
    }

    // Sort items canonically so resolution is strictly invariant to evidence ordering.
    let mut items = evidence.items.clone();
    items.sort_by(|a, b| {
        (
            a.source_type,
            &a.source,
            a.observation_time,
            a.publication_time,
            &a.content_hash,
            &a.id,
        )
            .cmp(&(
                b.source_type,
                &b.source,
                b.observation_time,
                b.publication_time,
                &b.content_hash,
                &b.id,
            ))
    });

    if items.is_empty() {
        return Ok(handle_missing_evidence(spec, hash_hex, trace));
    }

    // Check explicit invalidation events first.
    for item in &items {
        if let EvidenceValue::InvalidationEvent { reason } = &item.value {
            trace.push(format!(
                "Observed InvalidationEvent from `{}`: {}",
                item.source, reason
            ));
            return Ok(resolve_cancellation(
                spec,
                hash_hex,
                vec![item.id.clone()],
                trace,
                reason,
            ));
        }
    }

    // Filter out items prior to observation.start, and enforce exact boundary inclusivity
    // and restatement rules.
    let mut in_window_items = Vec::new();
    for item in &items {
        if let Some(start) = spec.observation.start {
            if item.observation_time < start {
                trace.push(format!(
                    "Item `{}` skipped: observation_time {} precedes window start {}",
                    item.id, item.observation_time, start
                ));
                continue;
            }
        }

        if item.observation_time == deadline && spec.observation.deadline_inclusivity.is_none() {
            trace.push(format!(
                "Item `{}` falls on exact deadline {} and `observation.deadline_inclusivity` is undefined.",
                item.id, deadline
            ));
            return Ok(unresolved(
                hash_hex,
                ResolutionReasonCode::MissingRequiredPolicy,
                "Evidence falls on exact deadline timestamp, but `observation.deadline_inclusivity` is undefined.",
                vec![item.id.clone()],
                trace,
            ));
        }

        let within_deadline = match spec.observation.deadline_inclusivity {
            Some(DeadlineInclusivity::Exclusive) => item.observation_time < deadline,
            Some(DeadlineInclusivity::Inclusive) | None => item.observation_time <= deadline,
        };

        if !within_deadline {
            trace.push(format!(
                "Item `{}` excluded: observation_time {} is after deadline {}",
                item.id, item.observation_time, deadline
            ));
            continue;
        }

        // Check restatements
        if item.metadata.is_restatement == Some(true) {
            match spec.revision_policy.restatement_handling {
                None => {
                    return Ok(unresolved(
                        hash_hex,
                        ResolutionReasonCode::MissingRequiredPolicy,
                        "Evidence contains a restatement, but `revision_policy.restatement_handling` is undefined.",
                        vec![item.id.clone()],
                        trace,
                    ));
                }
                Some(RestatementHandling::IgnorePostDeadlineRestatements) => {
                    if item.publication_time > deadline {
                        trace.push(format!(
                            "Restatement `{}` ignored per `ignore_post_deadline_restatements`.",
                            item.id
                        ));
                        continue;
                    }
                }
                Some(RestatementHandling::ApplyRestatementsWithinWindow)
                | Some(RestatementHandling::MustHoldThroughFinalizationWindow) => {
                    let cutoff = spec.revision_policy.corrections_until.or_else(|| {
                        spec.revision_policy
                            .restatement_window_days
                            .map(|days| deadline + Duration::days(i64::from(days)))
                    });
                    if let Some(c) = cutoff {
                        if item.publication_time > c {
                            trace.push(format!(
                                "Restatement `{}` excluded: publication_time {} exceeds corrections cutoff {}.",
                                item.id, item.publication_time, c
                            ));
                            continue;
                        }
                    }
                }
            }
        }

        in_window_items.push(item.clone());
    }

    if in_window_items.is_empty() {
        // Distinguish whether all evidence was after deadline vs genuinely missing.
        trace
            .push("No qualifying in-window evidence items remain after deadline filtering.".into());
        return Ok(Resolution {
            outcome: Outcome::No,
            reason_code: ResolutionReasonCode::EvidenceAfterDeadline,
            explanation: "No qualifying evidence occurred on or before the observation deadline."
                .into(),
            applied_rule_hash: hash_hex,
            decisive_evidence_ids: items.iter().map(|i| i.id.clone()).collect(),
            trace,
        });
    }

    // Evaluate each in-window item semantically against the spec's class policy,
    // collecting either a deterministic per-item verdict or an immediate Unresolved if
    // the item exercises an undefined policy field.
    let mut evaluated: Vec<EvaluatedItem> = Vec::new();
    for item in &in_window_items {
        match evaluate_item(spec, item, deadline, &mut trace)? {
            ItemEval::MissingPolicy {
                reason_code,
                message,
            } => {
                return Ok(unresolved(
                    hash_hex,
                    reason_code,
                    &message,
                    vec![item.id.clone()],
                    trace,
                ));
            }
            ItemEval::Invalidated { reason } => {
                return Ok(resolve_cancellation(
                    spec,
                    hash_hex,
                    vec![item.id.clone()],
                    trace,
                    &reason,
                ));
            }
            ItemEval::Excluded { why } => {
                trace.push(format!("Item `{}` excluded by policy: {}", item.id, why));
            }
            ItemEval::Evaluated(ev) => evaluated.push(*ev),
        }
    }

    if evaluated.is_empty() {
        trace.push("All in-window items were excluded by semantic policy filters; predicate not satisfied.".into());
        return Ok(Resolution {
            outcome: Outcome::No,
            reason_code: ResolutionReasonCode::ConditionNotAchieved,
            explanation:
                "No in-window evidence satisfied the specification's semantic qualification rules."
                    .into(),
            applied_rule_hash: hash_hex,
            decisive_evidence_ids: in_window_items.iter().map(|i| i.id.clone()).collect(),
            trace,
        });
    }

    // Aggregate additive Money components (distinct `metadata.component_id`) per source
    // before applying temporal observation mode across snapshots.
    let evaluated = aggregate_components(spec, evaluated, &mut trace);

    // Apply observation mode across multiple timestamps from the same source when applicable.
    let per_source = match collapse_by_source(spec, &evaluated, deadline, &mut trace) {
        Ok(map) => map,
        Err((code, msg, ids)) => {
            return Ok(unresolved(hash_hex, code, &msg, ids, trace));
        }
    };

    // Resolve across distinct sources using conflict_policy & precedence_order.
    resolve_source_conflicts(spec, &per_source, hash_hex, trace)
}

#[derive(Debug, Clone)]
struct EvaluatedItem {
    item: EvidenceItem,
    satisfies: bool,
    comparable_key: String,
    converted_money: Option<Decimal>,
}

enum ItemEval {
    Evaluated(Box<EvaluatedItem>),
    Excluded {
        why: String,
    },
    Invalidated {
        reason: String,
    },
    MissingPolicy {
        reason_code: ResolutionReasonCode,
        message: String,
    },
}

fn evaluate_item(
    spec: &MilestoneSpec,
    item: &EvidenceItem,
    deadline: DateTime<Utc>,
    trace: &mut Vec<String>,
) -> Result<ItemEval, ResolutionError> {
    // Class-level semantic guards before comparing threshold values.
    match &spec.predicate.class_policy {
        ClassPolicy::Arr(arr) => {
            let basis = match arr.measurement_basis {
                Some(b) => b,
                None => {
                    return Ok(ItemEval::MissingPolicy {
                        reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                        message: "Cannot evaluate ARR evidence: `predicate.class_policy.measurement_basis` is undefined.".into(),
                    });
                }
            };
            if let Some(ev_basis) = item.metadata.arr_basis {
                if ev_basis != basis {
                    return Ok(ItemEval::Excluded {
                        why: format!(
                            "evidence ARR basis {:?} does not match required {:?}",
                            ev_basis, basis
                        ),
                    });
                }
            }
            if item.metadata.includes_services == Some(true) {
                match arr.include_services_revenue {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence includes professional services revenue, but `include_services_revenue` is undefined.".into(),
                        });
                    }
                    Some(false) => {
                        return Ok(ItemEval::Excluded {
                            why: "includes professional services revenue, which spec excludes"
                                .into(),
                        });
                    }
                    Some(true) => {}
                }
            }
            if item.metadata.includes_one_time_fees == Some(true) {
                match arr.include_one_time_fees {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence includes one-time fees, but `include_one_time_fees` is undefined.".into(),
                        });
                    }
                    Some(false) => {
                        return Ok(ItemEval::Excluded {
                            why: "includes one-time fees, which spec excludes".into(),
                        });
                    }
                    Some(true) => {}
                }
            }
            if item.metadata.includes_signed_not_started == Some(true) {
                match arr.signed_not_started_treatment {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence includes signed-not-started contracts, but `signed_not_started_treatment` is undefined.".into(),
                        });
                    }
                    Some(milestone_core::SignedNotStartedTreatment::ExcludeUntilStartDate) => {
                        return Ok(ItemEval::Excluded {
                            why: "includes signed-not-started contracts excluded by spec".into(),
                        });
                    }
                    Some(milestone_core::SignedNotStartedTreatment::IncludeOnSignature) => {}
                }
            }
            if item.metadata.net_of_discounts == Some(false) {
                match arr.discount_treatment {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence reports gross ARR before discounts, but `discount_treatment` is undefined.".into(),
                        });
                    }
                    Some(milestone_core::DiscountTreatment::NetOfDiscounts) => {
                        return Ok(ItemEval::Excluded {
                            why: "gross ARR before discounts excluded by `net_of_discounts`".into(),
                        });
                    }
                    Some(milestone_core::DiscountTreatment::GrossBeforeDiscounts) => {}
                }
            }
            if item.metadata.entity_scope_consolidated == Some(true) {
                let scope = arr.subsidiary_scope.or(spec.subject.entity_scope);
                match scope {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence includes subsidiary ARR, but `subsidiary_scope` is undefined.".into(),
                        });
                    }
                    Some(milestone_core::EntityScope::ParentEntityOnly) => {
                        return Ok(ItemEval::Excluded {
                            why: "consolidated subsidiary ARR excluded by `parent_entity_only`"
                                .into(),
                        });
                    }
                    _ => {}
                }
            }
        }
        ClassPolicy::Fundraising(fp) => {
            let required_event = match fp.completion_event {
                Some(ev) => ev,
                None => {
                    return Ok(ItemEval::MissingPolicy {
                        reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                        message: "Cannot evaluate fundraising evidence: `predicate.class_policy.completion_event` is undefined.".into(),
                    });
                }
            };
            if let Some(ev_event) = item.metadata.fundraising_event {
                if !fundraising_event_matches(required_event, ev_event) {
                    return Ok(ItemEval::Excluded {
                        why: format!(
                            "fundraising stage {:?} does not satisfy required completion_event {:?}",
                            ev_event, required_event
                        ),
                    });
                }
            } else {
                return Ok(ItemEval::MissingPolicy {
                    reason_code: ResolutionReasonCode::UnsupportedEvidenceType,
                    message: format!(
                        "Fundraising evidence `{}` lacks `metadata.fundraising_event`.",
                        item.id
                    ),
                });
            }

            if !item.metadata.instruments.is_empty() {
                let allowed = match &fp.allowed_instruments {
                    Some(v) => v,
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence specifies financing instruments, but `allowed_instruments` is undefined.".into(),
                        });
                    }
                };
                if item
                    .metadata
                    .instruments
                    .iter()
                    .any(|i| !allowed.contains(i))
                {
                    return Ok(ItemEval::Excluded {
                        why: "contains financing instrument(s) excluded by `allowed_instruments`"
                            .into(),
                    });
                }
            }

            if item.metadata.includes_secondary == Some(true) {
                match fp.include_secondary {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence includes secondary share sales, but `include_secondary` is undefined.".into(),
                        });
                    }
                    Some(false) => {
                        return Ok(ItemEval::Excluded {
                            why: "includes secondary proceeds, excluded by `include_secondary: false`".into(),
                        });
                    }
                    Some(true) => {}
                }
            }

            if item.metadata.includes_insider_capital == Some(true) {
                match fp.include_insider_capital {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence includes insider capital, but `include_insider_capital` is undefined.".into(),
                        });
                    }
                    Some(false) => {
                        return Ok(ItemEval::Excluded {
                            why: "includes insider capital excluded by spec".into(),
                        });
                    }
                    Some(true) => {}
                }
            }

            if item.metadata.includes_note_conversion == Some(true) {
                match fp.include_existing_note_conversion {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence includes existing convertible note conversion, but `include_existing_note_conversion` is undefined.".into(),
                        });
                    }
                    Some(false) => {
                        return Ok(ItemEval::Excluded {
                            why: "includes conversion of prior notes excluded by spec".into(),
                        });
                    }
                    Some(true) => {}
                }
            }

            if item.metadata.is_conditional_commitment == Some(true) {
                match fp.conditional_commitment_policy {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence involves conditional commitment, but `conditional_commitment_policy` is undefined.".into(),
                        });
                    }
                    Some(ConditionalCommitmentPolicy::ExcludeConditional) => {
                        return Ok(ItemEval::Excluded {
                            why: "conditional commitment excluded by `exclude_conditional`".into(),
                        });
                    }
                    Some(ConditionalCommitmentPolicy::IncludeIfConditionsMetByDeadline) => {}
                }
            }

            if item.metadata.unwound_before_finalization == Some(true) {
                match fp.unwind_policy {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Round unwound before finalization, and `unwind_policy` is undefined.".into(),
                        });
                    }
                    Some(UnwindPolicy::MustRemainClosedThroughFinalization) => {
                        return Ok(ItemEval::Excluded {
                            why: "round unwound prior to finalization".into(),
                        });
                    }
                    Some(UnwindPolicy::SnapshotAtCloseIgnoresLaterUnwind) => {}
                }
            }
        }
        ClassPolicy::Employment(emp) => {
            let required_event = match emp.qualifying_event {
                Some(ev) => ev,
                None => {
                    return Ok(ItemEval::MissingPolicy {
                        reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                        message: "Cannot evaluate employment evidence: `predicate.class_policy.qualifying_event` is undefined.".into(),
                    });
                }
            };
            if let Some(ev_event) = item.metadata.employment_event {
                if !employment_event_matches(required_event, ev_event) {
                    return Ok(ItemEval::Excluded {
                        why: format!(
                            "employment stage {:?} does not satisfy {:?}",
                            ev_event, required_event
                        ),
                    });
                }
            }
            if let Some(rel) = item.metadata.relationship {
                let allowed = match &emp.allowed_relationships {
                    Some(v) => v,
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence specifies employment relationship, but `allowed_relationships` is undefined.".into(),
                        });
                    }
                };
                if !allowed.contains(&rel) {
                    return Ok(ItemEval::Excluded {
                        why: format!("relationship {:?} not in allowed_relationships", rel),
                    });
                }
            }
            if item.metadata.via_acquisition == Some(true) {
                match emp.acquisition_treatment {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Employment occurred via acquisition, but `acquisition_treatment` is undefined.".into(),
                        });
                    }
                    Some(milestone_core::AcquisitionEmploymentTreatment::DoesNotCountUnlessNewEmploymentAgreement) => {
                        return Ok(ItemEval::Excluded {
                            why: "acqui-hire without new employment agreement excluded".into(),
                        });
                    }
                    Some(milestone_core::AcquisitionEmploymentTreatment::CountsAsJoining) => {}
                }
            }
            if item.metadata.never_commenced == Some(true) {
                match emp.announced_never_started_policy {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Employment was announced/signed but never commenced, and `announced_never_started_policy` is undefined.".into(),
                        });
                    }
                    Some(milestone_core::AnnouncedNeverStartedPolicy::DoesNotResolveYesWithoutActualStart) => {
                        return Ok(ItemEval::Excluded {
                            why: "employment rescinded/never commenced".into(),
                        });
                    }
                    Some(milestone_core::AnnouncedNeverStartedPolicy::ResolvesYesOnAnnouncement) => {}
                }
            }
            if item.metadata.on_leave_of_absence == Some(true) {
                match emp.leave_of_absence_treatment {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Subject on leave of absence, and `leave_of_absence_treatment` is undefined.".into(),
                        });
                    }
                    Some(milestone_core::LeaveTreatment::RequiresActiveWorkingDay) => {
                        return Ok(ItemEval::Excluded {
                            why: "on leave of absence without active working day".into(),
                        });
                    }
                    Some(milestone_core::LeaveTreatment::CountsIfActiveOnRoster) => {}
                }
            }
        }
        ClassPolicy::Acquisition(acq) => {
            let required_event = match acq.completion_event {
                Some(ev) => ev,
                None => {
                    return Ok(ItemEval::MissingPolicy {
                        reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                        message: "Cannot evaluate acquisition evidence: `predicate.class_policy.completion_event` is undefined.".into(),
                    });
                }
            };
            if let Some(ev_event) = item.metadata.acquisition_event {
                if !acquisition_event_matches(required_event, ev_event) {
                    return Ok(ItemEval::Excluded {
                        why: format!(
                            "acquisition stage {:?} does not satisfy {:?}",
                            ev_event, required_event
                        ),
                    });
                }
            }
            if let Some(ev_basis) = item.metadata.valuation_basis {
                let req_basis = match acq.valuation_basis {
                    Some(b) => b,
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence specifies valuation basis, but `valuation_basis` is undefined.".into(),
                        });
                    }
                };
                if ev_basis != req_basis {
                    return Ok(ItemEval::Excluded {
                        why: format!("valuation basis {:?} != required {:?}", ev_basis, req_basis),
                    });
                }
            }
            if item.metadata.includes_contingent_earnout == Some(true) {
                match acq.earn_out_treatment {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Deal includes contingent earn-out, but `earn_out_treatment` is undefined.".into(),
                        });
                    }
                    Some(milestone_core::EarnOutTreatment::ExcludeContingentEarnOuts) => {
                        return Ok(ItemEval::Excluded {
                            why: "includes contingent earn-out excluded by `exclude_contingent_earn_outs`".into(),
                        });
                    }
                    _ => {}
                }
            }
            if let Some(st) = item.metadata.structure {
                let allowed = match &acq.transaction_structures {
                    Some(v) => v,
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence specifies deal structure, but `transaction_structures` is undefined.".into(),
                        });
                    }
                };
                if !allowed.contains(&st) {
                    return Ok(ItemEval::Excluded {
                        why: format!("transaction structure {:?} excluded", st),
                    });
                }
            }
            if !item.metadata.consideration_types.is_empty() {
                let allowed = match &acq.consideration_types {
                    Some(v) => v,
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Evidence specifies consideration types, but `consideration_types` is undefined.".into(),
                        });
                    }
                };
                if item
                    .metadata
                    .consideration_types
                    .iter()
                    .any(|c| !allowed.contains(c))
                {
                    return Ok(ItemEval::Excluded {
                        why: "contains consideration type excluded by `consideration_types`".into(),
                    });
                }
            }
            if item.metadata.terminated_before_close == Some(true) {
                match acq.termination_after_announcement_policy {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Deal terminated after announcement, and `termination_after_announcement_policy` is undefined.".into(),
                        });
                    }
                    Some(TerminationPolicy::ResolvesNoIfTerminatedBeforeClose) => {
                        return Ok(ItemEval::Excluded {
                            why: "deal terminated prior to close".into(),
                        });
                    }
                    Some(TerminationPolicy::SnapshotAtAnnouncement) => {}
                }
            }
        }
        ClassPolicy::Audience(aud) => {
            if item.metadata.channel_deleted == Some(true) {
                match aud.deleted_or_suspended_channel_policy {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message: "Channel deleted/suspended, and `deleted_or_suspended_channel_policy` is undefined.".into(),
                        });
                    }
                    Some(DeletedChannelPolicy::ResolvesInvalid) => {
                        return Ok(ItemEval::Invalidated {
                            reason: "Channel deleted before observation deadline.".into(),
                        });
                    }
                    Some(DeletedChannelPolicy::ResolvesNo) => {
                        return Ok(ItemEval::Excluded {
                            why: "channel deleted before deadline".into(),
                        });
                    }
                    Some(DeletedChannelPolicy::UseLastValidSnapshotBeforeDeletion) => {}
                }
            }
            if item.metadata.is_momentary_peak == Some(true) {
                match aud.crossing_rule {
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                            message:
                                "Evidence is a momentary peak, and `crossing_rule` is undefined."
                                    .into(),
                        });
                    }
                    Some(CrossingRule::DeadlineTimestampSnapshot)
                    | Some(CrossingRule::EndOfDaySnapshot)
                    | Some(CrossingRule::SustainedForWindow) => {
                        if item.observation_time != deadline {
                            return Ok(ItemEval::Excluded {
                                why: "momentary peak prior to deadline does not satisfy snapshot crossing_rule".into(),
                            });
                        }
                    }
                    Some(CrossingRule::MomentaryCrossing) => {}
                }
            }
            if item.metadata.after_bot_purge == Some(false) {
                if let Some(BotPurgePolicy::NetAfterPlatformAdjustmentsUntilFinalization) =
                    aud.bot_purge_policy
                {
                    trace.push(format!(
                        "Item `{}` is pre-bot-purge snapshot under net_after_platform_adjustments policy.",
                        item.id
                    ));
                }
            }
        }
    }

    // Evaluate predicate threshold comparison against item.value
    let mut converted_money_opt = None;
    let (satisfies, comparable_key) = match (&spec.predicate.threshold, &item.value) {
        (Threshold::Money(target), EvidenceValue::Money { amount, currency }) => {
            let target_ccy = match target.currency.as_deref().map(str::trim) {
                Some(c) if !c.is_empty() => c.to_uppercase(),
                _ => {
                    return Ok(ItemEval::MissingPolicy {
                        reason_code: ResolutionReasonCode::MissingRequiredPolicy,
                        message: "Threshold currency code is undefined.".into(),
                    });
                }
            };
            let item_ccy = currency.trim().to_uppercase();
            let converted_amount = if item_ccy == target_ccy {
                *amount
            } else {
                let cp = match &spec.currency_policy {
                    Some(cp) => cp,
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingFxPolicyOrQuote,
                            message: format!(
                                "Evidence `{}` is denominated in {}, but `currency_policy` is undefined.",
                                item.id, item_ccy
                            ),
                        });
                    }
                };
                if cp.allow_non_reporting_currency == Some(false) {
                    return Ok(ItemEval::Excluded {
                        why: format!("non-reporting currency {} disallowed", item_ccy),
                    });
                }
                let req_fx_source = match cp.fx_source {
                    Some(s) => s,
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingFxPolicyOrQuote,
                            message: "Non-reporting currency evidence requires `currency_policy.fx_source`.".into(),
                        });
                    }
                };
                let req_fx_rule = match cp.fx_timestamp_rule {
                    Some(r) => r,
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingFxPolicyOrQuote,
                            message: "Non-reporting currency evidence requires `currency_policy.fx_timestamp_rule`.".into(),
                        });
                    }
                };
                let quote = match &item.fx_quote {
                    Some(q) => q,
                    None => {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingFxPolicyOrQuote,
                            message: format!(
                                "Evidence `{}` in {} lacks an `fx_quote` from {:?}.",
                                item.id, item_ccy, req_fx_source
                            ),
                        });
                    }
                };
                if quote.base_currency.to_uppercase() != item_ccy
                    || quote.quote_currency.to_uppercase() != target_ccy
                    || quote.fx_source != req_fx_source
                {
                    return Ok(ItemEval::MissingPolicy {
                        reason_code: ResolutionReasonCode::MissingFxPolicyOrQuote,
                        message: format!(
                            "FX quote on `{}` does not match configured source {:?} and pair {}/{}.",
                            item.id, req_fx_source, item_ccy, target_ccy
                        ),
                    });
                }
                if let Some(q_rule) = quote.fx_timestamp_rule {
                    if q_rule != req_fx_rule {
                        return Ok(ItemEval::MissingPolicy {
                            reason_code: ResolutionReasonCode::MissingFxPolicyOrQuote,
                            message: format!(
                                "FX quote timestamp rule {:?} does not match configured {:?}.",
                                q_rule, req_fx_rule
                            ),
                        });
                    }
                }
                *amount * quote.rate
            };

            converted_money_opt = Some(converted_amount);
            let ok = compare_decimal(converted_amount, spec.predicate.comparator, target.amount);
            trace.push(format!(
                "Item `{}` ({}): converted={} {} vs target={} {} -> {}",
                item.id,
                item.source,
                converted_amount.normalize(),
                target_ccy,
                target.amount.normalize(),
                target_ccy,
                ok
            ));
            (ok, format!("{}:{}", ok, converted_amount.normalize()))
        }
        (Threshold::Count(target), EvidenceValue::Count { count }) => {
            let ok = compare_u64(*count, spec.predicate.comparator, *target);
            trace.push(format!(
                "Item `{}` ({}): count={} vs target={} -> {}",
                item.id, item.source, count, target, ok
            ));
            (ok, format!("{}:{}", ok, count))
        }
        (Threshold::Boolean(target), EvidenceValue::Employment { employed, employer }) => {
            let employer_matches =
                if let ClassPolicy::Employment(emp) = &spec.predicate.class_policy {
                    employer
                        .trim()
                        .eq_ignore_ascii_case(emp.target_employer.trim())
                } else {
                    false
                };
            let ok = (*employed && employer_matches) == *target;
            trace.push(format!(
                "Item `{}` ({}): employed={} employer=`{}` -> {}",
                item.id, item.source, employed, employer, ok
            ));
            (ok, format!("{}:{}", ok, employer.trim().to_lowercase()))
        }
        _ => {
            return Ok(ItemEval::MissingPolicy {
                reason_code: ResolutionReasonCode::UnsupportedEvidenceType,
                message: format!(
                    "Evidence `{}` value kind does not match predicate threshold.",
                    item.id
                ),
            });
        }
    };

    Ok(ItemEval::Evaluated(Box::new(EvaluatedItem {
        item: item.clone(),
        satisfies,
        comparable_key,
        converted_money: converted_money_opt,
    })))
}

fn aggregate_components(
    spec: &MilestoneSpec,
    evaluated: Vec<EvaluatedItem>,
    trace: &mut Vec<String>,
) -> Vec<EvaluatedItem> {
    let target_money = match &spec.predicate.threshold {
        Threshold::Money(m) => m.amount,
        _ => return evaluated,
    };

    let mut plain = Vec::new();
    let mut grouped: BTreeMap<(SourceType, String), Vec<EvaluatedItem>> = BTreeMap::new();
    for ev in evaluated {
        if ev.item.metadata.component_id.is_some() && ev.converted_money.is_some() {
            grouped
                .entry((ev.item.source_type, ev.item.source.clone()))
                .or_default()
                .push(ev);
        } else {
            plain.push(ev);
        }
    }

    for ((_stype, sname), items) in grouped {
        if items.len() == 1 {
            plain.push(items.into_iter().next().unwrap());
            continue;
        }
        let sum: Decimal = items.iter().filter_map(|i| i.converted_money).sum();
        let satisfies = compare_decimal(sum, spec.predicate.comparator, target_money);
        let mut combined = items.last().unwrap().clone();
        let ids: Vec<String> = items.iter().map(|i| i.item.id.clone()).collect();
        combined.item.id = ids.join("+");
        combined.satisfies = satisfies;
        combined.converted_money = Some(sum);
        combined.comparable_key = format!("{}:{}", satisfies, sum.normalize());
        trace.push(format!(
            "Aggregated {} components for `{}`: sum={} vs target={} -> {}",
            items.len(),
            sname,
            sum.normalize(),
            target_money.normalize(),
            satisfies
        ));
        plain.push(combined);
    }

    plain
}

type SourceMap = BTreeMap<(SourceType, String), EvaluatedItem>;
type CollapseError = (ResolutionReasonCode, String, Vec<String>);

fn collapse_by_source(
    spec: &MilestoneSpec,
    evaluated: &[EvaluatedItem],
    deadline: DateTime<Utc>,
    trace: &mut Vec<String>,
) -> Result<SourceMap, CollapseError> {
    let mut by_source: BTreeMap<(SourceType, String), Vec<&EvaluatedItem>> = BTreeMap::new();
    for ev in evaluated {
        by_source
            .entry((ev.item.source_type, ev.item.source.clone()))
            .or_default()
            .push(ev);
    }

    let mut collapsed = BTreeMap::new();
    for ((stype, sname), mut group) in by_source {
        if group.len() == 1 {
            collapsed.insert((stype, sname), group[0].clone());
            continue;
        }

        // Multiple observations from the same source: need observation_mode if outcomes differ.
        let any_yes = group.iter().any(|g| g.satisfies);
        let any_no = group.iter().any(|g| !g.satisfies);

        group.sort_by(|a, b| {
            (a.item.observation_time, a.item.publication_time, &a.item.id).cmp(&(
                b.item.observation_time,
                b.item.publication_time,
                &b.item.id,
            ))
        });

        // If restatements exist at the same observation_time, take latest publication_time.
        let latest = (*group.last().unwrap()).clone();

        if any_yes && any_no {
            match spec.observation.observation_mode {
                None => {
                    return Err((
                        ResolutionReasonCode::MissingRequiredPolicy,
                        "Source reports differing values across timestamps, and `observation.observation_mode` is undefined.".into(),
                        group.iter().map(|g| g.item.id.clone()).collect(),
                    ));
                }
                Some(ObservationMode::AnytimeOnOrBeforeDeadline) => {
                    let yes_item = group.iter().find(|g| g.satisfies).unwrap();
                    trace.push(format!(
                        "Source `{}` satisfied threshold at {} under `anytime_on_or_before_deadline`.",
                        sname, yes_item.item.observation_time
                    ));
                    collapsed.insert((stype, sname), (*yes_item).clone());
                }
                Some(ObservationMode::PointInTimeAtDeadline) => {
                    // Prefer observation closest to (and <=) deadline, breaking ties by latest publication_time.
                    let at_deadline = group
                        .iter()
                        .rfind(|g| g.item.observation_time == deadline)
                        .copied()
                        .unwrap_or(group.last().copied().unwrap());
                    trace.push(format!(
                        "Source `{}` evaluated at snapshot {} under `point_in_time_at_deadline`.",
                        sname, at_deadline.item.observation_time
                    ));
                    collapsed.insert((stype, sname), at_deadline.clone());
                }
                Some(ObservationMode::SustainedThroughDeadline) => {
                    let no_item = group.iter().rfind(|g| !g.satisfies).unwrap();
                    trace.push(format!(
                        "Source `{}` failed sustained condition at {}.",
                        sname, no_item.item.observation_time
                    ));
                    collapsed.insert((stype, sname), (*no_item).clone());
                }
            }
        } else {
            collapsed.insert((stype, sname), latest);
        }
    }

    Ok(collapsed)
}

fn resolve_source_conflicts(
    spec: &MilestoneSpec,
    per_source: &BTreeMap<(SourceType, String), EvaluatedItem>,
    hash_hex: String,
    mut trace: Vec<String>,
) -> Result<Resolution, ResolutionError> {
    let entries: Vec<&EvaluatedItem> = per_source.values().collect();
    let all_yes = entries.iter().all(|e| e.satisfies);
    let all_no = entries.iter().all(|e| !e.satisfies);

    if entries.len() == 1 {
        let only = entries[0];
        return Ok(final_verdict(
            only.satisfies,
            hash_hex,
            vec![only.item.id.clone()],
            trace,
            &spec.predicate.threshold,
        ));
    }

    let has_outcome_conflict = !(all_yes || all_no);
    let distinct_keys: std::collections::BTreeSet<&str> =
        entries.iter().map(|e| e.comparable_key.as_str()).collect();
    let has_value_conflict = distinct_keys.len() > 1;

    if !has_outcome_conflict {
        if spec.evidence_policy.conflict_policy == Some(ConflictPolicy::RequireCorroboration) {
            let min = spec.evidence_policy.minimum_corroboration.unwrap_or(2) as usize;
            if entries.len() < min {
                return Ok(unresolved(
                    hash_hex,
                    ResolutionReasonCode::MissingEvidence,
                    &format!(
                        "Only {} independent source(s) provided; {} required by `require_corroboration`.",
                        entries.len(),
                        min
                    ),
                    entries.iter().map(|e| e.item.id.clone()).collect(),
                    trace,
                ));
            }
        }
        return Ok(final_verdict(
            all_yes,
            hash_hex,
            entries.iter().map(|e| e.item.id.clone()).collect(),
            trace,
            &spec.predicate.threshold,
        ));
    }

    // Sources disagree on YES vs NO!
    trace.push("Evidence sources conflict on outcome (YES vs NO).".into());
    let conflict_policy = match spec.evidence_policy.conflict_policy {
        Some(p) => p,
        None => {
            return Ok(unresolved(
                hash_hex,
                ResolutionReasonCode::SourceConflict,
                "Authoritative evidence sources conflict without an explicit `conflict_policy` and precedence hierarchy.",
                entries.iter().map(|e| e.item.id.clone()).collect(),
                trace,
            ));
        }
    };

    match conflict_policy {
        ConflictPolicy::UnresolvedOnConflict => Ok(unresolved(
            hash_hex,
            ResolutionReasonCode::SourceConflict,
            "Sources conflict and `conflict_policy` is `unresolved_on_conflict`.",
            entries.iter().map(|e| e.item.id.clone()).collect(),
            trace,
        )),
        ConflictPolicy::InvalidOnUnreconciledConflict => Ok(Resolution {
            outcome: Outcome::Invalid,
            reason_code: ResolutionReasonCode::SourceConflict,
            explanation: "Unreconciled source conflict triggers `INVALID` per `invalid_on_unreconciled_conflict`.".into(),
            applied_rule_hash: hash_hex,
            decisive_evidence_ids: entries.iter().map(|e| e.item.id.clone()).collect(),
            trace,
        }),
        ConflictPolicy::RequireCorroboration => {
            let min = spec.evidence_policy.minimum_corroboration.unwrap_or(2) as usize;
            let yes_count = entries.iter().filter(|e| e.satisfies).count();
            let no_count = entries.iter().filter(|e| !e.satisfies).count();
            if yes_count >= min && no_count < min {
                Ok(final_verdict(
                    true,
                    hash_hex,
                    entries.iter().filter(|e| e.satisfies).map(|e| e.item.id.clone()).collect(),
                    trace,
                    &spec.predicate.threshold,
                ))
            } else if no_count >= min && yes_count < min {
                Ok(final_verdict(
                    false,
                    hash_hex,
                    entries.iter().filter(|e| !e.satisfies).map(|e| e.item.id.clone()).collect(),
                    trace,
                    &spec.predicate.threshold,
                ))
            } else {
                Ok(unresolved(
                    hash_hex,
                    ResolutionReasonCode::SourceConflict,
                    "Sources conflict and neither side meets `minimum_corroboration`.",
                    entries.iter().map(|e| e.item.id.clone()).collect(),
                    trace,
                ))
            }
        }
        ConflictPolicy::UseHighestPrecedenceSource => {
            let order = &spec.evidence_policy.precedence_order;
            if order.is_empty() {
                return Ok(unresolved(
                    hash_hex,
                    ResolutionReasonCode::SourceConflict,
                    "`use_highest_precedence_source` configured, but `evidence_policy.precedence_order` is empty.",
                    entries.iter().map(|e| e.item.id.clone()).collect(),
                    trace,
                ));
            }

            let mut ranked: Vec<(usize, &EvaluatedItem)> = entries
                .iter()
                .filter_map(|e| {
                    order
                        .iter()
                        .position(|st| *st == e.item.source_type)
                        .map(|rank| (rank, *e))
                })
                .collect();

            if ranked.len() < entries.len() && has_value_conflict {
                // If an unranked source exists while ranked sources exist, check if any ranked exists.
                if ranked.is_empty() {
                    return Ok(unresolved(
                        hash_hex,
                        ResolutionReasonCode::SourceConflict,
                        "Conflicting sources are not present in `evidence_policy.precedence_order`.",
                        entries.iter().map(|e| e.item.id.clone()).collect(),
                        trace,
                    ));
                }
            }

            ranked.sort_by_key(|(rank, _)| *rank);
            let best_rank = ranked[0].0;
            let top_items: Vec<&EvaluatedItem> = ranked
                .iter()
                .filter(|(rank, _)| *rank == best_rank)
                .map(|(_, e)| *e)
                .collect();

            let top_yes = top_items.iter().all(|e| e.satisfies);
            let top_no = top_items.iter().all(|e| !e.satisfies);

            if !(top_yes || top_no) {
                return Ok(unresolved(
                    hash_hex,
                    ResolutionReasonCode::SourceConflict,
                    "Tie at highest source precedence rank with conflicting outcomes.",
                    top_items.iter().map(|e| e.item.id.clone()).collect(),
                    trace,
                ));
            }

            trace.push(format!(
                "Resolved conflict using highest precedence source `{:?}`.",
                top_items[0].item.source_type
            ));
            Ok(final_verdict(
                top_yes,
                hash_hex,
                top_items.iter().map(|e| e.item.id.clone()).collect(),
                trace,
                &spec.predicate.threshold,
            ))
        }
    }
}

fn handle_missing_evidence(
    spec: &MilestoneSpec,
    hash_hex: String,
    mut trace: Vec<String>,
) -> Resolution {
    trace.push("Evidence bundle contains 0 items.".into());
    match spec.evidence_policy.availability_policy {
        Some(AvailabilityPolicy::ResolveInvalidAfterGracePeriod) => Resolution {
            outcome: Outcome::Invalid,
            reason_code: ResolutionReasonCode::MissingEvidence,
            explanation: "No evidence available; spec mandates `INVALID`.".into(),
            applied_rule_hash: hash_hex,
            decisive_evidence_ids: vec![],
            trace,
        },
        _ => unresolved(
            hash_hex,
            ResolutionReasonCode::MissingEvidence,
            "No evidence items provided for resolution.",
            vec![],
            trace,
        ),
    }
}

fn resolve_cancellation(
    spec: &MilestoneSpec,
    hash_hex: String,
    ids: Vec<String>,
    trace: Vec<String>,
    reason: &str,
) -> Resolution {
    match spec.invalidation_policy.cancellation_policy {
        Some(CancellationPolicy::ResolveInvalid) => Resolution {
            outcome: Outcome::Invalid,
            reason_code: ResolutionReasonCode::InvalidationTriggered,
            explanation: format!("Invalidation triggered ({}): resolves INVALID.", reason),
            applied_rule_hash: hash_hex,
            decisive_evidence_ids: ids,
            trace,
        },
        Some(CancellationPolicy::ResolveNo) => Resolution {
            outcome: Outcome::No,
            reason_code: ResolutionReasonCode::InvalidationTriggered,
            explanation: format!("Cancellation triggered ({}): resolves NO per cancellation_policy.", reason),
            applied_rule_hash: hash_hex,
            decisive_evidence_ids: ids,
            trace,
        },
        Some(CancellationPolicy::CarryToSuccessorEntity) | None => unresolved(
            hash_hex,
            ResolutionReasonCode::MissingRequiredPolicy,
            &format!(
                "Invalidation/cancellation event encountered (`{}`), and `invalidation_policy.cancellation_policy` requires manual determination or is undefined.",
                reason
            ),
            ids,
            trace,
        ),
    }
}

fn final_verdict(
    satisfied: bool,
    hash_hex: String,
    ids: Vec<String>,
    trace: Vec<String>,
    threshold: &Threshold,
) -> Resolution {
    let (outcome, reason_code, explanation) = match (satisfied, threshold) {
        (true, Threshold::Boolean(_)) => (
            Outcome::Yes,
            ResolutionReasonCode::ConditionAchieved,
            "Qualifying evidence satisfies the milestone condition.".to_string(),
        ),
        (false, Threshold::Boolean(_)) => (
            Outcome::No,
            ResolutionReasonCode::ConditionNotAchieved,
            "Qualifying evidence does not satisfy the milestone condition.".to_string(),
        ),
        (true, _) => (
            Outcome::Yes,
            ResolutionReasonCode::ThresholdSatisfied,
            "Qualifying evidence satisfies the numeric threshold within the observation window."
                .to_string(),
        ),
        (false, _) => (
            Outcome::No,
            ResolutionReasonCode::ThresholdNotSatisfied,
            "Qualifying evidence does not satisfy the numeric threshold.".to_string(),
        ),
    };
    Resolution {
        outcome,
        reason_code,
        explanation,
        applied_rule_hash: hash_hex,
        decisive_evidence_ids: ids,
        trace,
    }
}

fn unresolved(
    hash_hex: String,
    reason_code: ResolutionReasonCode,
    explanation: &str,
    decisive_evidence_ids: Vec<String>,
    trace: Vec<String>,
) -> Resolution {
    Resolution {
        outcome: Outcome::Unresolved,
        reason_code,
        explanation: explanation.to_string(),
        applied_rule_hash: hash_hex,
        decisive_evidence_ids,
        trace,
    }
}

fn compare_decimal(value: Decimal, cmp: Comparator, target: Decimal) -> bool {
    match cmp {
        Comparator::GreaterThanOrEqual => value >= target,
        Comparator::GreaterThan => value > target,
        Comparator::Equal => value == target,
        Comparator::LessThanOrEqual => value <= target,
        Comparator::LessThan => value < target,
        Comparator::Achieved => value >= target,
    }
}

fn compare_u64(value: u64, cmp: Comparator, target: u64) -> bool {
    match cmp {
        Comparator::GreaterThanOrEqual => value >= target,
        Comparator::GreaterThan => value > target,
        Comparator::Equal => value == target,
        Comparator::LessThanOrEqual => value <= target,
        Comparator::LessThan => value < target,
        Comparator::Achieved => value >= target,
    }
}

fn fundraising_event_matches(
    required: FundraisingCompletionEvent,
    observed: FundraisingCompletionEvent,
) -> bool {
    use FundraisingCompletionEvent::*;
    let rank = |e| match e {
        PublicAnnouncement => 1,
        TermSheetSigned => 2,
        DefinitiveAgreementSigned => 3,
        FundsReceivedInBank => 4,
        LegalCloseAndFundsReceived => 5,
    };
    match required {
        PublicAnnouncement => observed == PublicAnnouncement || rank(observed) >= 3,
        _ => rank(observed) >= rank(required),
    }
}

fn employment_event_matches(
    required: EmploymentQualifyingEvent,
    observed: EmploymentQualifyingEvent,
) -> bool {
    use EmploymentQualifyingEvent::*;
    let rank = |e| match e {
        PublicAppointmentAnnouncement => 1,
        SignedOfferLetter => 2,
        OfficialStartDate => 3,
        FirstPayrollCycle => 4,
    };
    match required {
        PublicAppointmentAnnouncement => rank(observed) >= 1,
        _ => rank(observed) >= rank(required),
    }
}

fn acquisition_event_matches(
    required: AcquisitionCompletionEvent,
    observed: AcquisitionCompletionEvent,
) -> bool {
    use AcquisitionCompletionEvent::*;
    let rank = |e| match e {
        PublicAnnouncement => 1,
        DefinitiveAgreementSigned => 2,
        RegulatoryClearanceAndClose => 3,
        LegalClose => 4,
    };
    rank(observed) >= rank(required)
}
