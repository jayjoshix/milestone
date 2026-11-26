mod presenter;
mod web;

use clap::{Parser, Subcommand, ValueEnum};
use milestone_adversarial::{attack_spec, ScenarioAttackResult, ScenarioStatus};
use milestone_core::{
    from_json_str, from_yaml_str, rule_hash_hex, to_json_string, to_yaml_string, ClassPolicy,
    EvidenceBundle, MarketClass, MilestoneSpec, Threshold,
};
use milestone_lint::{evaluate_listability, ListabilityStatus, Severity};
use milestone_parser::{parse_headline, ParseStatus};
use milestone_resolver::resolve;
use presenter::{candidate_choices, friendly_question, friendly_rule_name, pick_hero_scenario};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(
    name = "milestone",
    version,
    about = "Detect ambiguous milestone-market resolution rules before trading"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
    Yaml,
}

#[derive(Subcommand)]
enum Commands {
    /// Check a natural-language milestone headline for resolution ambiguities
    Check {
        /// Milestone question (e.g. "Will Acme raise $5M before Dec 31, 2027?")
        headline: String,
    },
    /// Parse a natural-language milestone headline into a partial canonical AST
    Parse {
        /// Milestone question (e.g. "Will Acme reach $1M ARR by June 30, 2027?")
        headline: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Yaml)]
        format: OutputFormat,
    },
    /// Check a specification for resolution ambiguities and listability
    Lint {
        /// Path to YAML or JSON milestone specification
        spec: PathBuf,
        /// Show detailed internal paths, severities, and remediation hints
        #[arg(long)]
        verbose: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Compute canonical Keccak-256 rule hash
    Hash {
        /// Path to YAML or JSON milestone specification
        spec: PathBuf,
    },
    /// Generate adversarial scenarios and pinpoint minimal missing rules
    Attack {
        /// Path to YAML or JSON milestone specification
        spec: PathBuf,
        /// Display all generated scenarios instead of the most important ambiguity
        #[arg(long)]
        all: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Deterministically resolve a specification against an evidence bundle
    Resolve {
        /// Path to YAML or JSON milestone specification
        spec: PathBuf,
        /// Path to YAML or JSON evidence bundle
        evidence: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Produce a human-readable resolution charter from a specification
    Explain {
        /// Path to YAML or JSON milestone specification
        spec: PathBuf,
    },
    /// Launch the lightweight local interactive web demo
    Serve {
        /// Address to bind (e.g. 127.0.0.1:8787)
        #[arg(long, default_value = "127.0.0.1:8787")]
        addr: String,
    },
}

fn load_spec(path: &PathBuf) -> Result<MilestoneSpec, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("Failed to read {:?}: {}", path, e))?;
    if path.extension().and_then(|s| s.to_str()) == Some("json") {
        from_json_str(&raw).map_err(|e| format!("Invalid JSON spec: {}", e))
    } else {
        from_yaml_str(&raw).map_err(|e| format!("Invalid YAML spec: {}", e))
    }
}

fn load_evidence(path: &PathBuf) -> Result<EvidenceBundle, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("Failed to read {:?}: {}", path, e))?;
    if path.extension().and_then(|s| s.to_str()) == Some("json") {
        serde_json::from_str(&raw).map_err(|e| format!("Invalid JSON evidence: {}", e))
    } else {
        serde_yaml::from_str(&raw).map_err(|e| format!("Invalid YAML evidence: {}", e))
    }
}

fn print_out(s: &str) {
    let mut out = io::stdout().lock();
    if let Err(e) = writeln!(out, "{}", s) {
        if e.kind() == io::ErrorKind::BrokenPipe {
            process::exit(0);
        }
        eprintln!("stdout error: {}", e);
        process::exit(2);
    }
}

fn main() {
    let cli = Cli::parse();
    let code = match run(cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            2
        }
    };
    process::exit(code);
}

fn format_hero_counterexample(
    buf: &mut String,
    sc: &ScenarioAttackResult,
    class: MarketClass,
    concise: bool,
) {
    let primary_path = sc
        .minimal_missing_policies
        .first()
        .map(String::as_str)
        .unwrap_or("predicate.class_policy");
    let question = friendly_question(primary_path, class, &sc.name);
    let rule_name = friendly_rule_name(primary_path, class);

    if !concise {
        let _ = writeln!(buf, "Most important ambiguity");
        let _ = writeln!(buf, "────────────────────────");
        let _ = writeln!(buf, "{}\n", question);
        let _ = writeln!(buf, "Counterexample");
        let _ = writeln!(buf, "──────────────");
        let _ = writeln!(buf, "{}\n", sc.description);

        if sc.divergent_interpretations.len() >= 2 {
            let a = &sc.divergent_interpretations[0];
            let b = sc
                .divergent_interpretations
                .iter()
                .find(|d| d.outcome != a.outcome)
                .unwrap_or(&sc.divergent_interpretations[1]);
            let _ = writeln!(buf, "Reasonable interpretation A:");
            let _ = writeln!(buf, "{:?}", a.outcome);
            let _ = writeln!(buf, "  if {}\n", a.policy_setting);
            let _ = writeln!(buf, "Reasonable interpretation B:");
            let _ = writeln!(buf, "{:?}", b.outcome);
            let _ = writeln!(buf, "  if {}\n", b.policy_setting);
        } else if let Some(d) = sc.divergent_interpretations.first() {
            let _ = writeln!(buf, "Resolution requires specifying:");
            let _ = writeln!(buf, "  {} -> {:?}\n", d.policy_setting, d.outcome);
        }

        let _ = writeln!(buf, "Minimal missing rule");
        let _ = writeln!(buf, "────────────────────");
        let _ = writeln!(buf, "{}", rule_name);

        let choices = candidate_choices(primary_path, class);
        if !choices.is_empty() {
            let _ = writeln!(buf, "\nChoose one:");
            for c in choices {
                let _ = writeln!(buf, "- {}", c);
            }
        }
    } else {
        let _ = writeln!(buf, "Why:\n{}\n", question);
        let _ = writeln!(buf, "Counterexample:\n{}\n", sc.description);
        if sc.divergent_interpretations.len() >= 2 {
            let a = &sc.divergent_interpretations[0];
            let b = sc
                .divergent_interpretations
                .iter()
                .find(|d| d.outcome != a.outcome)
                .unwrap_or(&sc.divergent_interpretations[1]);
            let _ = writeln!(buf, "{:?} if {}", a.outcome, a.policy_setting);
            let _ = writeln!(buf, "{:?} if {}\n", b.outcome, b.policy_setting);
        }
        let _ = writeln!(buf, "Missing rule:\n{}", rule_name);
        let choices = candidate_choices(primary_path, class);
        if !choices.is_empty() {
            let _ = writeln!(buf, "\nCandidate policies:");
            for c in choices {
                let _ = writeln!(buf, "- {}", c);
            }
        }
    }
}

fn run(cli: Cli) -> Result<i32, String> {
    match cli.command {
        Commands::Check { headline } => {
            let parsed = parse_headline(&headline).map_err(|e| e.to_string())?;
            let Some(spec) = parsed.spec else {
                print_out(&format!(
                    "{}\n\nCOULD NOT PARSE HEADLINE\n{}",
                    headline, parsed.note
                ));
                return Ok(1);
            };
            let listability = evaluate_listability(&spec);
            let attack = attack_spec(&spec);
            let class = spec.predicate.class_policy.market_class();

            let mut buf = String::new();
            let _ = writeln!(buf, "{}\n", spec.title);
            match listability.status {
                ListabilityStatus::Listable => {
                    let _ = writeln!(buf, "READY TO LIST");
                    let _ = writeln!(buf, "Rule hash: {}", listability.rule_hash);
                }
                ListabilityStatus::ListableWithWarnings | ListabilityStatus::NotListable => {
                    let _ = writeln!(buf, "NOT READY TO LIST\n");
                    if let Some(hero) = pick_hero_scenario(&attack, class) {
                        format_hero_counterexample(&mut buf, hero, class, true);
                    } else if let Some(first) = listability.findings.first() {
                        let _ = writeln!(
                            buf,
                            "Why:\n{}\n\nMissing rule:\n{}",
                            friendly_question(&first.affected_path, class, &first.title),
                            friendly_rule_name(&first.affected_path, class)
                        );
                    }
                    let blocking = listability.counts.critical + listability.counts.high;
                    if blocking > 1 {
                        let _ = writeln!(
                            buf,
                            "\n({} total resolution ambiguities detected; run `milestone serve` or `milestone attack` to inspect all.)",
                            blocking
                        );
                    }
                }
            }
            print_out(buf.trim_end());
            Ok(0)
        }
        Commands::Parse { headline, format } => {
            let out = parse_headline(&headline).map_err(|e| e.to_string())?;
            let mut buf = String::new();
            match format {
                OutputFormat::Json => {
                    buf.push_str(&serde_json::to_string_pretty(&out).unwrap());
                }
                OutputFormat::Yaml | OutputFormat::Text => {
                    let _ = writeln!(buf, "Status: {:?}", out.status);
                    let _ = writeln!(buf, "Note: {}", out.note);
                    if !out.unresolved_gaps.is_empty() {
                        let _ = writeln!(buf, "\nUnresolved gaps:");
                        for g in &out.unresolved_gaps {
                            let _ = writeln!(buf, "  - {}", g);
                        }
                    }
                    if let Some(ref spec) = out.spec {
                        let _ = write!(
                            buf,
                            "\nCanonical AST (YAML):\n{}",
                            to_yaml_string(spec).unwrap()
                        );
                    }
                }
            }
            print_out(buf.trim_end());
            Ok(if out.status == ParseStatus::Unrecognized {
                1
            } else {
                0
            })
        }
        Commands::Lint {
            spec: path,
            verbose,
            format,
        } => {
            let spec = load_spec(&path)?;
            let report = evaluate_listability(&spec);
            let class = spec.predicate.class_policy.market_class();
            let mut buf = String::new();

            match format {
                OutputFormat::Json => {
                    buf.push_str(&serde_json::to_string_pretty(&report).unwrap());
                }
                OutputFormat::Yaml => {
                    buf.push_str(serde_yaml::to_string(&report).unwrap().trim_end());
                }
                OutputFormat::Text if verbose => {
                    let _ = writeln!(buf, "Milestone:\n{}", report.title);
                    let _ = writeln!(buf, "\nResult:\n{}", report.status);
                    let _ = writeln!(buf, "\nCRITICAL: {}", report.counts.critical);
                    let _ = writeln!(buf, "HIGH:     {}", report.counts.high);
                    let _ = writeln!(buf, "MEDIUM:   {}", report.counts.medium);
                    let _ = writeln!(buf, "LOW:      {}", report.counts.low);
                    let _ = writeln!(buf, "RuleHash: {}", report.rule_hash);

                    if !report.findings.is_empty() {
                        let _ = writeln!(buf);
                        for f in &report.findings {
                            let _ = writeln!(buf, "{} {} — {}", f.severity, f.id, f.title);
                            let _ = writeln!(buf, "  Path: {}", f.affected_path);
                            let _ = writeln!(buf, "  {}", f.explanation);
                            let _ = writeln!(buf, "  Fix: {}", f.remediation);
                            let _ = writeln!(buf);
                        }
                    }

                    if let Some(ref next) = report.suggested_next_step {
                        let _ = write!(buf, "Suggested next step:\n{}", next);
                    }
                }
                OutputFormat::Text => {
                    let status_label = match report.status {
                        ListabilityStatus::Listable => "LISTABLE",
                        ListabilityStatus::ListableWithWarnings => "LISTABLE WITH WARNINGS",
                        ListabilityStatus::NotListable => "NOT LISTABLE",
                    };
                    let _ = writeln!(buf, "{}\n", status_label);

                    if report.status == ListabilityStatus::Listable {
                        let _ = writeln!(buf, "No resolution ambiguities found.");
                        let _ = write!(buf, "Rule hash: {}", report.rule_hash);
                    } else {
                        let blocking: Vec<_> = report
                            .findings
                            .iter()
                            .filter(|f| matches!(f.severity, Severity::Critical | Severity::High))
                            .collect();
                        let shown = if blocking.is_empty() {
                            report.findings.iter().collect::<Vec<_>>()
                        } else {
                            blocking
                        };

                        let _ = writeln!(buf, "{} resolution ambiguities found\n", shown.len());

                        let mut seen_paths = BTreeSet::new();
                        let mut idx = 0usize;
                        for f in &shown {
                            if !seen_paths.insert(f.affected_path.as_str()) {
                                continue;
                            }
                            idx += 1;
                            if idx > 5 {
                                let _ = writeln!(
                                    buf,
                                    "... and {} more (run with --verbose to inspect all)\n",
                                    shown.len() - 5
                                );
                                break;
                            }
                            let q = friendly_question(&f.affected_path, class, &f.title);
                            let rule = friendly_rule_name(&f.affected_path, class);
                            let _ = writeln!(buf, "{}. {}", idx, q);
                            let _ = writeln!(buf, "   Missing: {}\n", rule);
                        }

                        let _ = writeln!(buf, "Run:\n  milestone attack {}\n", path.display());
                        let _ = write!(buf, "to generate concrete conflicting scenarios.");
                    }
                }
            }
            print_out(buf.trim_end());
            Ok(0)
        }
        Commands::Hash { spec: path } => {
            let spec = load_spec(&path)?;
            print_out(&format!("Rule hash:\n{}", rule_hash_hex(&spec)));
            Ok(0)
        }
        Commands::Attack {
            spec: path,
            all,
            format,
        } => {
            let spec = load_spec(&path)?;
            let report = attack_spec(&spec);
            let class = spec.predicate.class_policy.market_class();
            let mut buf = String::new();

            match format {
                OutputFormat::Json => {
                    buf.push_str(&serde_json::to_string_pretty(&report).unwrap());
                }
                OutputFormat::Yaml => {
                    buf.push_str(serde_yaml::to_string(&report).unwrap().trim_end());
                }
                OutputFormat::Text if !all => {
                    let _ = writeln!(buf, "Market\n──────\n{}\n", report.title);
                    let _ = writeln!(buf, "Result\n──────");
                    if report.ambiguous_count == 0 {
                        let _ = writeln!(buf, "DETERMINISTIC\n");
                        let _ = writeln!(
                            buf,
                            "Generated {} adversarial scenarios.",
                            report.total_scenarios
                        );
                        let _ = writeln!(buf, "0 allow conflicting resolutions.\n");
                        let _ = write!(buf, "Rule hash: {}", rule_hash_hex(&spec));
                    } else {
                        let _ = writeln!(buf, "NOT DETERMINISTIC\n");
                        let _ = writeln!(
                            buf,
                            "Generated {} adversarial scenarios.",
                            report.total_scenarios
                        );
                        let _ = writeln!(
                            buf,
                            "{} allow conflicting resolutions.\n",
                            report.ambiguous_count
                        );

                        if let Some(hero) = pick_hero_scenario(&report, class) {
                            format_hero_counterexample(&mut buf, hero, class, false);
                        }

                        if report.ambiguous_count > 1 {
                            let _ = writeln!(
                                buf,
                                "\n{} more ambiguous scenarios hidden.\nUse --all to display.",
                                report.ambiguous_count - 1
                            );
                        }
                    }
                }
                OutputFormat::Text => {
                    let _ = writeln!(buf, "Market: {}\n", report.title);
                    let _ = writeln!(
                        buf,
                        "Generated {} adversarial scenarios: {} deterministic, {} ambiguous.\n",
                        report.total_scenarios, report.deterministic_count, report.ambiguous_count
                    );

                    for r in &report.results {
                        let tag = match r.status {
                            ScenarioStatus::Deterministic => {
                                format!("DETERMINISTIC ({:?})", r.current_outcome)
                            }
                            ScenarioStatus::Ambiguous => "AMBIGUOUS".to_string(),
                        };
                        let _ = writeln!(buf, "[{}] {} — {}", tag, r.scenario_id, r.name);
                        let _ = writeln!(buf, "  Counterexample: {}", r.description);
                        if r.status == ScenarioStatus::Ambiguous {
                            if !r.minimal_missing_policies.is_empty() {
                                let names: Vec<String> = r
                                    .minimal_missing_policies
                                    .iter()
                                    .map(|p| friendly_rule_name(p, class))
                                    .collect();
                                let _ = writeln!(buf, "  Missing rule: {}", names.join(", "));
                            }
                            for d in &r.divergent_interpretations {
                                let _ =
                                    writeln!(buf, "    -> {:?} if {}", d.outcome, d.policy_setting);
                            }
                        }
                        let _ = writeln!(buf);
                    }
                }
            }
            print_out(buf.trim_end());
            Ok(0)
        }
        Commands::Resolve {
            spec: spec_path,
            evidence: ev_path,
            format,
        } => {
            let spec = load_spec(&spec_path)?;
            let evidence = load_evidence(&ev_path)?;
            let res = resolve(&spec, &evidence).map_err(|e| e.to_string())?;
            let mut buf = String::new();
            match format {
                OutputFormat::Json => {
                    buf.push_str(&serde_json::to_string_pretty(&res).unwrap());
                }
                OutputFormat::Yaml => {
                    buf.push_str(serde_yaml::to_string(&res).unwrap().trim_end());
                }
                OutputFormat::Text => {
                    let _ = writeln!(buf, "Outcome:     {:?}", res.outcome);
                    let _ = writeln!(buf, "Reason code: {:?}", res.reason_code);
                    let _ = writeln!(buf, "Rule hash:   {}", res.applied_rule_hash);
                    let _ = writeln!(buf, "Explanation: {}", res.explanation);
                    if !res.decisive_evidence_ids.is_empty() {
                        let _ =
                            writeln!(buf, "Evidence:    {}", res.decisive_evidence_ids.join(", "));
                    }
                    let _ = writeln!(buf, "\nResolution trace:");
                    for step in &res.trace {
                        let _ = writeln!(buf, "  - {}", step);
                    }
                }
            }
            print_out(buf.trim_end());
            Ok(0)
        }
        Commands::Explain { spec: path } => {
            let spec = load_spec(&path)?;
            let report = evaluate_listability(&spec);
            let mut buf = String::new();
            let _ = writeln!(
                buf,
                "============================================================"
            );
            let _ = writeln!(buf, "RESOLUTION CHARTER: {}", spec.title);
            let _ = writeln!(buf, "Spec ID:   {}", spec.id);
            let _ = writeln!(buf, "Rule Hash: {}", rule_hash_hex(&spec));
            let _ = writeln!(buf, "Status:    {}", report.status);
            let _ = writeln!(
                buf,
                "============================================================\n"
            );

            let _ = writeln!(buf, "1. SUBJECT");
            let _ = writeln!(
                buf,
                "   Name: {} ({:?})",
                spec.subject.name, spec.subject.subject_type
            );
            if let Some(ref scope) = spec.subject.entity_scope {
                let _ = writeln!(buf, "   Scope: {:?}", scope);
            }
            if !spec.subject.identifiers.is_empty() {
                for (k, v) in &spec.subject.identifiers {
                    let _ = writeln!(buf, "   Identifier[{}]: {}", k, v);
                }
            } else {
                let _ = writeln!(buf, "   Identifiers: [UNSPECIFIED]");
            }

            let _ = writeln!(buf, "\n2. PREDICATE");
            let _ = writeln!(buf, "   Metric:     {:?}", spec.predicate.metric);
            let _ = writeln!(buf, "   Comparator: {:?}", spec.predicate.comparator);
            match &spec.predicate.threshold {
                Threshold::Money(m) => {
                    let _ = writeln!(
                        buf,
                        "   Threshold:  {} {}",
                        m.amount.normalize(),
                        m.currency.as_deref().unwrap_or("[UNSPECIFIED]")
                    );
                }
                Threshold::Count(c) => {
                    let _ = writeln!(buf, "   Threshold:  {}", c);
                }
                Threshold::Boolean(b) => {
                    let _ = writeln!(buf, "   Threshold:  {}", b);
                }
            }
            match &spec.predicate.class_policy {
                ClassPolicy::Arr(a) => {
                    let _ = writeln!(
                        buf,
                        "   ARR Measurement Basis:    {:?}",
                        a.measurement_basis
                    );
                    let _ = writeln!(
                        buf,
                        "   Include Services:         {:?}",
                        a.include_services_revenue
                    );
                    let _ = writeln!(
                        buf,
                        "   Include One-Time Fees:    {:?}",
                        a.include_one_time_fees
                    );
                    let _ = writeln!(
                        buf,
                        "   Usage-Based Treatment:    {:?}",
                        a.usage_based_treatment
                    );
                    let _ = writeln!(
                        buf,
                        "   Discount Treatment:       {:?}",
                        a.discount_treatment
                    );
                    let _ = writeln!(buf, "   Churn Treatment:          {:?}", a.churn_treatment);
                    let _ = writeln!(
                        buf,
                        "   Signed-Not-Started Rule:  {:?}",
                        a.signed_not_started_treatment
                    );
                }
                ClassPolicy::Fundraising(f) => {
                    let _ = writeln!(buf, "   Completion Event:         {:?}", f.completion_event);
                    let _ = writeln!(
                        buf,
                        "   Allowed Instruments:      {:?}",
                        f.allowed_instruments
                    );
                    let _ = writeln!(
                        buf,
                        "   Include Secondary:        {:?}",
                        f.include_secondary
                    );
                    let _ = writeln!(
                        buf,
                        "   Include Insider Capital:  {:?}",
                        f.include_insider_capital
                    );
                    let _ = writeln!(
                        buf,
                        "   Include Note Conversion:  {:?}",
                        f.include_existing_note_conversion
                    );
                    let _ = writeln!(buf, "   Tranche Policy:           {:?}", f.tranche_policy);
                    let _ = writeln!(
                        buf,
                        "   Conditional Commitments:  {:?}",
                        f.conditional_commitment_policy
                    );
                    let _ = writeln!(buf, "   Unwind Policy:            {:?}", f.unwind_policy);
                }
                ClassPolicy::Employment(e) => {
                    let _ = writeln!(buf, "   Target Employer:          {}", e.target_employer);
                    let _ = writeln!(
                        buf,
                        "   Employer Scope:           {:?}",
                        e.target_employer_scope
                    );
                    let _ = writeln!(buf, "   Qualifying Event:         {:?}", e.qualifying_event);
                    let _ = writeln!(
                        buf,
                        "   Allowed Relationships:    {:?}",
                        e.allowed_relationships
                    );
                    let _ = writeln!(
                        buf,
                        "   Acquisition Treatment:    {:?}",
                        e.acquisition_treatment
                    );
                    let _ = writeln!(
                        buf,
                        "   Leave Treatment:          {:?}",
                        e.leave_of_absence_treatment
                    );
                }
                ClassPolicy::Acquisition(a) => {
                    let _ = writeln!(buf, "   Completion Event:         {:?}", a.completion_event);
                    let _ = writeln!(buf, "   Valuation Basis:          {:?}", a.valuation_basis);
                    let _ = writeln!(
                        buf,
                        "   Consideration Types:      {:?}",
                        a.consideration_types
                    );
                    let _ = writeln!(
                        buf,
                        "   Earn-Out Treatment:       {:?}",
                        a.earn_out_treatment
                    );
                    let _ = writeln!(
                        buf,
                        "   Transaction Structures:   {:?}",
                        a.transaction_structures
                    );
                    let _ = writeln!(
                        buf,
                        "   Termination Policy:       {:?}",
                        a.termination_after_announcement_policy
                    );
                }
                ClassPolicy::Audience(aud) => {
                    let _ = writeln!(buf, "   Platform:                 {:?}", aud.platform);
                    let _ = writeln!(buf, "   Channel Scope:            {:?}", aud.channel_scope);
                    let _ = writeln!(buf, "   Crossing Rule:            {:?}", aud.crossing_rule);
                    let _ = writeln!(
                        buf,
                        "   Count Source Mode:        {:?}",
                        aud.count_source_mode
                    );
                    let _ = writeln!(
                        buf,
                        "   Bot Purge Policy:         {:?}",
                        aud.bot_purge_policy
                    );
                    let _ = writeln!(
                        buf,
                        "   Deleted Channel Policy:   {:?}",
                        aud.deleted_or_suspended_channel_policy
                    );
                }
            }

            let _ = writeln!(buf, "\n3. OBSERVATION & FX");
            let _ = writeln!(
                buf,
                "   Deadline (UTC):     {:?}",
                spec.observation.deadline
            );
            let _ = writeln!(
                buf,
                "   Timezone:           {:?}",
                spec.observation.timezone
            );
            let _ = writeln!(
                buf,
                "   Inclusivity:        {:?}",
                spec.observation.deadline_inclusivity
            );
            let _ = writeln!(
                buf,
                "   Observation Mode:   {:?}",
                spec.observation.observation_mode
            );
            if let Some(ref cp) = spec.currency_policy {
                let _ = writeln!(buf, "   FX Source:          {:?}", cp.fx_source);
                let _ = writeln!(buf, "   FX Timestamp Rule:  {:?}", cp.fx_timestamp_rule);
            }

            let _ = writeln!(buf, "\n4. EVIDENCE & REVISIONS");
            let _ = writeln!(
                buf,
                "   Primary Source:     {:?}",
                spec.evidence_policy
                    .primary_source
                    .as_ref()
                    .map(|s| &s.name)
            );
            let _ = writeln!(
                buf,
                "   Conflict Policy:    {:?}",
                spec.evidence_policy.conflict_policy
            );
            let _ = writeln!(
                buf,
                "   Precedence Order:   {:?}",
                spec.evidence_policy.precedence_order
            );
            let _ = writeln!(
                buf,
                "   Restatement Rule:   {:?}",
                spec.revision_policy.restatement_handling
            );
            let _ = writeln!(
                buf,
                "   Cancellation Rule:  {:?}",
                spec.invalidation_policy.cancellation_policy
            );

            let _ = to_json_string(&spec);
            print_out(buf.trim_end());
            Ok(0)
        }
        Commands::Serve { addr } => {
            web::serve(&addr)?;
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_command_handles_raise_headline() {
        let parsed = parse_headline("Will Acme raise $5M before Dec 31, 2027?").unwrap();
        let spec = parsed.spec.unwrap();
        let attack = attack_spec(&spec);
        let class = spec.predicate.class_policy.market_class();
        let hero = pick_hero_scenario(&attack, class).unwrap();
        let mut buf = String::new();
        format_hero_counterexample(&mut buf, hero, class, true);
        assert!(buf.contains("fundraising.completion_event"));
        assert!(buf.contains("term_sheet_signed"));
        assert!(buf.contains("funds_received_in_bank"));
    }
}
