pub mod minimizer;
pub mod scenarios;

use milestone_core::MilestoneSpec;
pub use minimizer::{
    analyze_scenario, AttackReport, DivergentInterpretation, ScenarioAttackResult, ScenarioStatus,
};
pub use scenarios::{generate_scenarios, AdversarialScenario};
use std::collections::BTreeMap;

/// Runs adversarial scenario generation and counterexample minimization against `spec`.
pub fn attack_spec(spec: &MilestoneSpec) -> AttackReport {
    let scenarios = generate_scenarios(spec);
    let mut results = Vec::with_capacity(scenarios.len());
    let mut policy_ambiguity_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut deterministic_count = 0;
    let mut ambiguous_count = 0;

    for scenario in &scenarios {
        let res = analyze_scenario(spec, scenario);
        match res.status {
            ScenarioStatus::Deterministic => deterministic_count += 1,
            ScenarioStatus::Ambiguous => {
                ambiguous_count += 1;
                for p in &res.minimal_missing_policies {
                    *policy_ambiguity_counts.entry(p.clone()).or_insert(0) += 1;
                }
            }
        }
        results.push(res);
    }

    let highest_risk_policy = policy_ambiguity_counts
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(k, _)| k.clone());

    AttackReport {
        spec_id: spec.id.clone(),
        title: spec.title.clone(),
        total_scenarios: scenarios.len(),
        deterministic_count,
        ambiguous_count,
        highest_risk_policy,
        policy_ambiguity_counts,
        results,
    }
}
