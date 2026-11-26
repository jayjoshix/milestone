use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Top-level canonical specification for a binary milestone prediction market.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneSpec {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub id: String,
    pub title: String,
    pub subject: Subject,
    pub predicate: Predicate,
    pub observation: ObservationWindow,
    #[serde(default)]
    pub currency_policy: Option<CurrencyPolicy>,
    #[serde(default)]
    pub evidence_policy: EvidencePolicy,
    #[serde(default)]
    pub revision_policy: RevisionPolicy,
    #[serde(default)]
    pub invalidation_policy: InvalidationPolicy,
}

fn default_schema_version() -> String {
    "1.0".to_string()
}

/// The five supported milestone market classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketClass {
    Arr,
    Fundraising,
    Employment,
    Acquisition,
    Audience,
}

/// Entity whose milestone is being resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    pub name: String,
    #[serde(rename = "type")]
    pub subject_type: SubjectType,
    #[serde(default)]
    pub jurisdiction: Option<String>,
    #[serde(default)]
    pub entity_scope: Option<EntityScope>,
    #[serde(default)]
    pub identifiers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectType {
    Company,
    Person,
    Product,
    Channel,
    Organization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityScope {
    ParentEntityOnly,
    ConsolidatedGroupWithSubsidiaries,
    NamedEntitiesOnly,
}

/// Core resolution predicate: metric + comparator + threshold + class-specific semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Predicate {
    pub metric: Metric,
    pub comparator: Comparator,
    pub threshold: Threshold,
    pub class_policy: ClassPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    AnnualRecurringRevenue,
    FundraisingTotal,
    EmploymentStatus,
    AcquisitionValuation,
    AudienceCount,
}

impl Metric {
    pub fn market_class(self) -> MarketClass {
        match self {
            Metric::AnnualRecurringRevenue => MarketClass::Arr,
            Metric::FundraisingTotal => MarketClass::Fundraising,
            Metric::EmploymentStatus => MarketClass::Employment,
            Metric::AcquisitionValuation => MarketClass::Acquisition,
            Metric::AudienceCount => MarketClass::Audience,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparator {
    #[serde(alias = ">=")]
    GreaterThanOrEqual,
    #[serde(alias = ">")]
    GreaterThan,
    #[serde(alias = "==")]
    Equal,
    #[serde(alias = "<=")]
    LessThanOrEqual,
    #[serde(alias = "<")]
    LessThan,
    Achieved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Threshold {
    Money(Money),
    Count(u64),
    Boolean(bool),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount: Decimal,
    #[serde(default)]
    pub currency: Option<String>,
}

/// Class-specific semantic policies. Fields intentionally use Option<...> so
/// unspecified interpretations remain explicitly null rather than silently guessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "class", rename_all = "snake_case")]
pub enum ClassPolicy {
    Arr(ArrPolicy),
    Fundraising(FundraisingPolicy),
    Employment(EmploymentPolicy),
    Acquisition(AcquisitionPolicy),
    Audience(AudiencePolicy),
}

impl ClassPolicy {
    pub fn market_class(&self) -> MarketClass {
        match self {
            ClassPolicy::Arr(_) => MarketClass::Arr,
            ClassPolicy::Fundraising(_) => MarketClass::Fundraising,
            ClassPolicy::Employment(_) => MarketClass::Employment,
            ClassPolicy::Acquisition(_) => MarketClass::Acquisition,
            ClassPolicy::Audience(_) => MarketClass::Audience,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrPolicy {
    #[serde(default)]
    pub measurement_basis: Option<ArrMeasurementBasis>,
    #[serde(default)]
    pub include_services_revenue: Option<bool>,
    #[serde(default)]
    pub include_one_time_fees: Option<bool>,
    #[serde(default)]
    pub usage_based_treatment: Option<UsageBasedTreatment>,
    #[serde(default)]
    pub discount_treatment: Option<DiscountTreatment>,
    #[serde(default)]
    pub churn_treatment: Option<ChurnTreatment>,
    #[serde(default)]
    pub signed_not_started_treatment: Option<SignedNotStartedTreatment>,
    #[serde(default)]
    pub subsidiary_scope: Option<EntityScope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrMeasurementBasis {
    ContractedRecurringRevenue,
    BilledRecurringRevenue,
    CollectedCashRecurringRevenue,
    AnnualizedMrr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageBasedTreatment {
    Excluded,
    #[serde(alias = "trailing_12_months")]
    Trailing12Months,
    #[serde(alias = "annualized_trailing_30_days")]
    AnnualizedTrailing30Days,
    #[serde(alias = "annualized_trailing_90_days")]
    AnnualizedTrailing90Days,
    CommittedMinimumOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscountTreatment {
    NetOfDiscounts,
    GrossBeforeDiscounts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChurnTreatment {
    ExcludeChurnedAndNoticed,
    ExcludeAlreadyChurnedOnly,
    IncludeUntilContractEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignedNotStartedTreatment {
    ExcludeUntilStartDate,
    IncludeOnSignature,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundraisingPolicy {
    #[serde(default)]
    pub completion_event: Option<FundraisingCompletionEvent>,
    #[serde(default)]
    pub allowed_instruments: Option<Vec<FinancingInstrument>>,
    #[serde(default)]
    pub include_secondary: Option<bool>,
    #[serde(default)]
    pub include_insider_capital: Option<bool>,
    #[serde(default)]
    pub include_existing_note_conversion: Option<bool>,
    #[serde(default)]
    pub tranche_policy: Option<TranchePolicy>,
    #[serde(default)]
    pub conditional_commitment_policy: Option<ConditionalCommitmentPolicy>,
    #[serde(default)]
    pub unwind_policy: Option<UnwindPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FundraisingCompletionEvent {
    TermSheetSigned,
    DefinitiveAgreementSigned,
    PublicAnnouncement,
    FundsReceivedInBank,
    LegalCloseAndFundsReceived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinancingInstrument {
    PricedEquity,
    Safe,
    ConvertibleNote,
    VentureDebt,
    CreditFacility,
    RevenueBasedFinancing,
    Grant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranchePolicy {
    CountOnlyFundedTranches,
    CountCommittedFullRoundIfFirstTrancheFunded,
    CountCommittedFullRoundAtClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionalCommitmentPolicy {
    ExcludeConditional,
    IncludeIfConditionsMetByDeadline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnwindPolicy {
    MustRemainClosedThroughFinalization,
    SnapshotAtCloseIgnoresLaterUnwind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmploymentPolicy {
    pub target_employer: String,
    #[serde(default)]
    pub target_employer_scope: Option<EmployerScope>,
    #[serde(default)]
    pub qualifying_event: Option<EmploymentQualifyingEvent>,
    #[serde(default)]
    pub allowed_relationships: Option<Vec<EmploymentRelationship>>,
    #[serde(default)]
    pub acquisition_treatment: Option<AcquisitionEmploymentTreatment>,
    #[serde(default)]
    pub leave_of_absence_treatment: Option<LeaveTreatment>,
    #[serde(default)]
    pub announced_never_started_policy: Option<AnnouncedNeverStartedPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmployerScope {
    NamedParentEntityOnly,
    NamedEntityAndWhollyOwnedSubsidiaries,
    IncludeAffiliatesAndLabs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmploymentQualifyingEvent {
    SignedOfferLetter,
    OfficialStartDate,
    FirstPayrollCycle,
    PublicAppointmentAnnouncement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmploymentRelationship {
    FullTimeEmployee,
    PartTimeEmployee,
    Contractor,
    Consultant,
    Advisor,
    BoardMember,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionEmploymentTreatment {
    CountsAsJoining,
    DoesNotCountUnlessNewEmploymentAgreement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaveTreatment {
    CountsIfActiveOnRoster,
    RequiresActiveWorkingDay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnouncedNeverStartedPolicy {
    DoesNotResolveYesWithoutActualStart,
    ResolvesYesOnAnnouncement,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcquisitionPolicy {
    #[serde(default)]
    pub completion_event: Option<AcquisitionCompletionEvent>,
    #[serde(default)]
    pub valuation_basis: Option<ValuationBasis>,
    #[serde(default)]
    pub consideration_types: Option<Vec<ConsiderationType>>,
    #[serde(default)]
    pub earn_out_treatment: Option<EarnOutTreatment>,
    #[serde(default)]
    pub transaction_structures: Option<Vec<TransactionStructure>>,
    #[serde(default)]
    pub termination_after_announcement_policy: Option<TerminationPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionCompletionEvent {
    PublicAnnouncement,
    DefinitiveAgreementSigned,
    RegulatoryClearanceAndClose,
    LegalClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValuationBasis {
    EnterpriseValue,
    EquityValue,
    HeadlineConsideration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsiderationType {
    Cash,
    PublicStock,
    PrivateStock,
    DebtAssumed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EarnOutTreatment {
    ExcludeContingentEarnOuts,
    IncludeMaximumEarnOut,
    IncludeFairValueAtClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionStructure {
    WholeCompanyMergerOrAcquisition,
    MajorityEquityStake,
    SubstantiallyAllAssets,
    MinorityStake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationPolicy {
    ResolvesNoIfTerminatedBeforeClose,
    SnapshotAtAnnouncement,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudiencePolicy {
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub channel_scope: Option<ChannelScope>,
    #[serde(default)]
    pub crossing_rule: Option<CrossingRule>,
    #[serde(default)]
    pub count_source_mode: Option<AudienceCountMode>,
    #[serde(default)]
    pub bot_purge_policy: Option<BotPurgePolicy>,
    #[serde(default)]
    pub deleted_or_suspended_channel_policy: Option<DeletedChannelPolicy>,
    #[serde(default)]
    pub migrated_channel_policy: Option<MigratedChannelPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelScope {
    SingleNamedChannelId,
    AggregateCreatorChannels,
    PrimaryChannelIncludingRebrands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossingRule {
    MomentaryCrossing,
    EndOfDaySnapshot,
    DeadlineTimestampSnapshot,
    SustainedForWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudienceCountMode {
    OfficialApiExactOrPublicTier,
    DisplayedUiCount,
    ThirdPartyTracker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BotPurgePolicy {
    NetAfterPlatformAdjustmentsUntilFinalization,
    RawSnapshotAtObservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletedChannelPolicy {
    ResolvesNo,
    ResolvesInvalid,
    UseLastValidSnapshotBeforeDeletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigratedChannelPolicy {
    FollowVerifiedOfficialMigration,
    OnlyOriginalChannelId,
}

/// Time window and boundary semantics for observing the predicate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationWindow {
    #[serde(default)]
    pub start: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deadline: Option<DateTime<Utc>>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub deadline_inclusivity: Option<DeadlineInclusivity>,
    #[serde(default)]
    pub observation_mode: Option<ObservationMode>,
    #[serde(default)]
    pub cutoff_precision: Option<CutoffPrecision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadlineInclusivity {
    Inclusive,
    Exclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationMode {
    PointInTimeAtDeadline,
    AnytimeOnOrBeforeDeadline,
    SustainedThroughDeadline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CutoffPrecision {
    ExactSecondUtc,
    EndOfCalendarDay,
}

/// Currency conversion and reporting denomination policy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrencyPolicy {
    #[serde(default)]
    pub reporting_currency: Option<String>,
    #[serde(default)]
    pub fx_source: Option<FxSource>,
    #[serde(default)]
    pub fx_timestamp_rule: Option<FxTimestampRule>,
    #[serde(default)]
    pub allow_non_reporting_currency: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FxSource {
    EcbDailyReference,
    FederalReserveH10,
    BloombergFixing,
    ReutersSpot,
    ExplicitRateTable,
    NoneRequiredSingleCurrency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FxTimestampRule {
    DeadlineClose,
    TransactionDateSpot,
    PeriodAverage,
    FixedSnapshotTime,
}

/// Authoritative evidence configuration and source conflict resolution rules.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidencePolicy {
    #[serde(default)]
    pub primary_source: Option<EvidenceSource>,
    #[serde(default)]
    pub fallback_sources: Vec<EvidenceSource>,
    #[serde(default)]
    pub precedence_order: Vec<SourceType>,
    #[serde(default)]
    pub conflict_policy: Option<ConflictPolicy>,
    #[serde(default)]
    pub availability_policy: Option<AvailabilityPolicy>,
    #[serde(default)]
    pub minimum_corroboration: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EvidenceSource {
    pub name: String,
    pub source_type: SourceType,
    #[serde(default)]
    pub independent_of_subject: Option<bool>,
    #[serde(default)]
    pub verification_method: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    AuditedFinancials,
    SecOrRegulatoryFiling,
    BankOrEscrowConfirmation,
    CapTableRegistry,
    CompanyAttestation,
    OfficialPlatformApi,
    PublicWebPage,
    PressRelease,
    ThirdPartyMedia,
    CourtOrLegalFiling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    UseHighestPrecedenceSource,
    RequireCorroboration,
    UnresolvedOnConflict,
    InvalidOnUnreconciledConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityPolicy {
    ReturnUnresolvedIfUnavailable,
    FallbackToSecondaryInPrecedenceOrder,
    ResolveInvalidAfterGracePeriod,
}

/// Rules governing restatements, corrections, and post-deadline unwinds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionPolicy {
    #[serde(default)]
    pub corrections_until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub restatement_window_days: Option<u32>,
    #[serde(default)]
    pub restatement_handling: Option<RestatementHandling>,
    #[serde(default)]
    pub retroactive_unwind_handling: Option<RetroactiveUnwindHandling>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestatementHandling {
    ApplyRestatementsWithinWindow,
    IgnorePostDeadlineRestatements,
    MustHoldThroughFinalizationWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetroactiveUnwindHandling {
    ReversesOutcomeIfWithinWindow,
    DoesNotReverseOnceSettled,
}

/// Exhaustiveness of YES / NO / INVALID outcomes and cancellation semantics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidationPolicy {
    #[serde(default)]
    pub yes_condition_summary: Option<String>,
    #[serde(default)]
    pub no_condition_summary: Option<String>,
    #[serde(default)]
    pub invalid_triggers: Vec<InvalidTrigger>,
    #[serde(default)]
    pub cancellation_policy: Option<CancellationPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidTrigger {
    SourceUnavailablePastGracePeriod,
    SubjectCeasedToExistBeforeEvent,
    AmbiguousIdentitySplit,
    PlatformDiscontinuedMetric,
    FraudOrMaterialMisrepresentation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationPolicy {
    ResolveNo,
    ResolveInvalid,
    CarryToSuccessorEntity,
}

/// Possible deterministic outcomes of milestone resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Outcome {
    Yes,
    No,
    Invalid,
    Unresolved,
}
