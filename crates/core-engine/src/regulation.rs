//! Military & law-enforcement regulation profiles, operational role profiles,
//! and deterministic compliance evaluation.
//!
//! Every check resolves against typed [`CapabilityTag`]s carried by
//! components and the draft's platform category — never product names, SKUs,
//! or free text (R3). Evaluation is pure: the report depends only on the
//! catalog, the draft, the chosen colorway, and the selected policy/role.
//!
//! Violations block compliance; recommendations never do. Both carry
//! `suggested_skus`: catalog components that provide the missing capability
//! *and* mount on the current platform (native or through an adapter hop),
//! in catalog order.

use crate::catalog::Catalog;
use crate::graph::{
    CapabilityTag, ComponentKind, ComponentSpec, PlatformCategory, PlatformSpec, resolve_mount,
};
use crate::loadout::{CHILD_LAYER, LoadoutDraft, LoadoutError};
use crate::pricing::Colorway;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// --- profiles -----------------------------------------------------------------

/// A dress-and-configuration regulation profile a build can be validated
/// against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegulationPolicy {
    /// US Army AR 670-1: Multicam or Coyote Brown only, minimum rifle
    /// magazine carriage (6 recommended), IFAK required, Black / Wolf Grey
    /// outer carriers forbidden.
    UsArmyAr670_1,
    /// USMC MARADMIN: Coyote Brown only, IFAK required, platform must be a
    /// plate carrier, chest rig, or LBE.
    UsmcMaradmin,
    /// Law-enforcement patrol: Black / Ranger Green / Wolf Grey only;
    /// holster mount required on belts, IFAK or comms required on carriers.
    LawEnforcementPatrol,
    /// Law-enforcement SWAT: Ranger Green / Multicam / Black / Wolf Grey
    /// only, IFAK required.
    LawEnforcementSwat,
}

impl RegulationPolicy {
    /// Human-readable profile name for compliance messages.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::UsArmyAr670_1 => "US Army AR 670-1",
            Self::UsmcMaradmin => "USMC MARADMIN",
            Self::LawEnforcementPatrol => "LE Patrol",
            Self::LawEnforcementSwat => "LE SWAT",
        }
    }
}

/// An operational role profile a build can be validated against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalRole {
    /// Breacher: back panel or breaching/sustainment utility plus IFAK.
    Breacher,
    /// JTAC: comms/PTT or admin-ATAK mount, placard, plus IFAK.
    Jtac,
    /// Drone operator: admin/utility storage plus IFAK.
    DroneOperator,
    /// Sniper / DM: prone-friendly profile; warns on stacked front placard
    /// child pouches.
    SniperDm,
    /// Recce: sustainment utility, magazine threshold, hydration/pack.
    Recce,
}

impl OperationalRole {
    /// Human-readable role name for compliance messages.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Breacher => "Breacher",
            Self::Jtac => "JTAC",
            Self::DroneOperator => "Drone Operator",
            Self::SniperDm => "Sniper / DM",
            Self::Recce => "Recce",
        }
    }
}

// --- report model ----------------------------------------------------------------

/// Typed rule code for one compliance violation or recommendation, with
/// machine-readable detail payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComplianceRuleCode {
    /// The chosen colorway is not permitted by the active policy.
    DisallowedColorway {
        /// The colorway the build selected.
        chosen: Colorway,
        /// Colorways the policy permits.
        allowed: Vec<Colorway>,
    },
    /// Mounted rifle magazine carriage is below the required minimum.
    InsufficientRifleMagCapacity {
        /// Minimum required magazines.
        minimum: u8,
        /// Magazines currently carried.
        found: u8,
    },
    /// A single required capability is missing from the build.
    MissingCapability {
        /// The capability that is absent.
        capability: CapabilityTag,
    },
    /// None of the listed alternative capabilities is present.
    MissingAnyCapability {
        /// Capabilities of which at least one is required.
        capabilities: Vec<CapabilityTag>,
    },
    /// The platform category cannot satisfy the active policy.
    UnsupportedPlatformCategory {
        /// Categories the policy accepts.
        allowed: Vec<PlatformCategory>,
        /// The draft's actual platform category (`None` = uncategorized
        /// legacy platform, which fail-closed never satisfies).
        found: Option<PlatformCategory>,
    },
    /// No placard-kind component is mounted (JTAC requirement).
    MissingPlacard,
    /// Rifle magazine carriage is below the recommended (non-blocking)
    /// level.
    RecommendRifleMagCapacity {
        /// Recommended magazines.
        recommended: u8,
        /// Magazines currently carried.
        found: u8,
    },
    /// The active role prefers a different platform category.
    RecommendPlatformCategory {
        /// Categories the role prefers.
        suggested: Vec<PlatformCategory>,
        /// The draft's actual platform category.
        found: Option<PlatformCategory>,
    },
    /// Child pouches stacked on the front placard impede the prone profile
    /// (Sniper / DM advisory).
    ProneProfileStacking {
        /// The child slot path carrying the stacked pouch.
        slot_id: String,
    },
}

/// One blocking compliance violation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceIssue {
    /// Typed rule that failed.
    pub code: ComplianceRuleCode,
    /// Human-readable description.
    pub message: String,
    /// Catalog SKUs that mount on the current platform and close the gap.
    pub suggested_skus: Vec<String>,
}

/// One non-blocking compliance recommendation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceRecommendation {
    /// Typed rule behind the recommendation.
    pub code: ComplianceRuleCode,
    /// Human-readable description.
    pub message: String,
    /// Catalog SKUs that mount on the current platform and close the gap.
    pub suggested_skus: Vec<String>,
}

/// The result of evaluating a draft against a policy and/or role.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ComplianceReport {
    /// Whether the build produced zero violations.
    pub compliant: bool,
    /// Blocking violations.
    pub violations: Vec<ComplianceIssue>,
    /// Non-blocking recommendations.
    pub recommendations: Vec<ComplianceRecommendation>,
}

// --- role presets -----------------------------------------------------------------

/// One placement inside a [`RolePreset`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetPlacement {
    /// Target slot: a platform slot id (`"front_placard"`) or a child slot
    /// path (`"m2.placard_front_pals"`). Child paths use the mount ids the
    /// preset's own fixed application order produces — adapters consume ids
    /// before the components they serve — so presets stay deterministic.
    pub slot_id: String,
    /// SKU of the component to equip.
    pub component_sku: String,
    /// PALS column index for grid placements (ignored by anchor mounts).
    pub col: u8,
    /// PALS row index for grid placements (ignored by anchor mounts).
    pub row: u8,
}

/// A curated one-click starter loadout: platform, colorway, policy, role,
/// and an ordered placement list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RolePreset {
    /// Stable preset identifier (e.g. `"jtac"`).
    pub preset_id: String,
    /// Human-readable preset name.
    pub name: String,
    /// The operational role the preset targets, if any. Role presets can be
    /// purely regulation-driven (e.g. LE patrol duty), in which case the
    /// preset validates against its [`policy`](Self::policy) alone.
    pub role: Option<OperationalRole>,
    /// The regulation policy the preset is validated against.
    pub policy: RegulationPolicy,
    /// Platform SKU the preset builds on.
    pub platform_sku: String,
    /// Colorway the preset configures by default.
    pub default_colorway: Colorway,
    /// Placements applied in order to instantiate the draft.
    pub placements: Vec<PresetPlacement>,
}

/// Failures while instantiating a [`RolePreset`].
#[derive(Debug, Clone, PartialEq, Error, Serialize, Deserialize)]
pub enum PresetApplyError {
    /// The preset references a platform absent from the catalog.
    #[error("preset `{preset_id}` references unknown platform `{sku}`")]
    UnknownPlatform {
        /// The failing preset.
        preset_id: String,
        /// The unknown platform SKU.
        sku: String,
    },
    /// One of the preset's placements was rejected by the engine.
    #[error(
        "preset `{preset_id}` placement #{index} (`{component_sku}` on `{slot_id}`) failed: {source}"
    )]
    PlacementFailed {
        /// The failing preset.
        preset_id: String,
        /// 1-based placement index.
        index: usize,
        /// The rejected component SKU.
        component_sku: String,
        /// The targeted slot.
        slot_id: String,
        /// The engine's rejection (boxed to keep this error compact).
        source: Box<LoadoutError>,
    },
}

// --- capability aggregation --------------------------------------------------------

/// Capabilities contributed by the draft's mounted components.
#[derive(Debug, Clone, Default)]
struct Capabilities {
    /// Every capability tag carried by mounted components.
    tags: Vec<CapabilityTag>,
    /// Whether a placard-kind component is mounted.
    placard_mounted: bool,
    /// Whether a back-panel-kind component is mounted.
    back_panel_mounted: bool,
}

impl Capabilities {
    /// Whether a unit capability tag is present.
    fn has(&self, tag: CapabilityTag) -> bool {
        self.tags.contains(&tag)
    }

    /// Whether any of the listed capabilities is present.
    fn has_any(&self, tags: &[CapabilityTag]) -> bool {
        tags.iter().any(|tag| self.has(*tag))
    }

    /// Total primary rifle magazine carriage across all mounted components.
    fn rifle_capacity(&self) -> u8 {
        self.tags
            .iter()
            .filter_map(|tag| match tag {
                CapabilityTag::RifleMagCapacity(count) => Some(*count),
                _ => None,
            })
            .sum()
    }
}

/// Aggregate the capabilities of every mounted (non-adapter) component.
fn aggregate_capabilities(catalog: &Catalog, draft: &LoadoutDraft) -> Capabilities {
    let mut caps = Capabilities::default();
    for item in draft.mounted().iter().filter(|item| !item.is_adapter) {
        let Some(component) = catalog.component(&item.sku) else {
            continue;
        };
        caps.tags.extend(component.capabilities.iter().copied());
        caps.placard_mounted |= component.kind == ComponentKind::Placard;
        caps.back_panel_mounted |= component.kind == ComponentKind::BackPanel;
    }
    caps
}

// --- suggestion resolution ----------------------------------------------------------

/// Whether `component` mounts on any slot of `platform` (native or through
/// one adapter hop).
fn mounts_on_platform(catalog: &Catalog, platform: &PlatformSpec, component: &ComponentSpec) -> bool {
    platform.slots.iter().any(|slot| {
        resolve_mount(platform, &slot.id, component, &catalog.adapter_bridges).is_compatible()
    })
}

/// Catalog components matching `predicate` that mount on `platform`, in
/// catalog order — the deterministic suggestion list.
fn suggest_where(
    catalog: &Catalog,
    platform: &PlatformSpec,
    predicate: impl Fn(&ComponentSpec) -> bool,
) -> Vec<String> {
    catalog
        .components
        .iter()
        .filter(|component| predicate(component))
        .filter(|component| mounts_on_platform(catalog, platform, component))
        .map(|component| component.sku.clone())
        .collect()
}

/// Whether two capability tags are the same variant, ignoring payload
/// (`RifleMagCapacity(2)` matches `RifleMagCapacity(0)`).
fn same_capability_kind(a: &CapabilityTag, b: &CapabilityTag) -> bool {
    matches!(
        (a, b),
        (CapabilityTag::RifleMagCapacity(_), CapabilityTag::RifleMagCapacity(_))
            | (CapabilityTag::MedicalIfak, CapabilityTag::MedicalIfak)
            | (CapabilityTag::CommsRoutingOrPtt, CapabilityTag::CommsRoutingOrPtt)
            | (CapabilityTag::AdminAtakUtility, CapabilityTag::AdminAtakUtility)
            | (CapabilityTag::BreachingOrOrdnance, CapabilityTag::BreachingOrOrdnance)
            | (CapabilityTag::HydrationSustainment, CapabilityTag::HydrationSustainment)
            | (CapabilityTag::HolsterRetention, CapabilityTag::HolsterRetention)
            | (CapabilityTag::ProneLowProfile, CapabilityTag::ProneLowProfile)
    )
}

/// Suggestions for components carrying any of `tags` (kind-matched).
fn suggest_for_tags(catalog: &Catalog, platform: &PlatformSpec, tags: &[CapabilityTag]) -> Vec<String> {
    suggest_where(catalog, platform, |component| {
        component
            .capabilities
            .iter()
            .any(|tag| tags.iter().any(|wanted| same_capability_kind(tag, wanted)))
    })
}

// --- evaluation ---------------------------------------------------------------------

/// Evaluate a draft against an optional regulation policy and operational
/// role, under a chosen colorway.
///
/// Pure and deterministic: the report depends only on the inputs. Violations
/// block compliance; recommendations never do. Suggestions list catalog SKUs
/// that mount on the draft's platform and close the gap.
pub fn evaluate_compliance(
    catalog: &Catalog,
    draft: &LoadoutDraft,
    colorway: Colorway,
    policy: Option<RegulationPolicy>,
    role: Option<OperationalRole>,
) -> ComplianceReport {
    let caps = aggregate_capabilities(catalog, draft);
    let mut ctx = EvalContext {
        catalog,
        draft,
        caps: &caps,
        violations: Vec::new(),
        recommendations: Vec::new(),
    };

    if let Some(policy) = policy {
        check_policy(&mut ctx, colorway, policy);
    }
    if let Some(role) = role {
        check_role(&mut ctx, role);
    }

    ComplianceReport {
        compliant: ctx.violations.is_empty(),
        violations: ctx.violations,
        recommendations: ctx.recommendations,
    }
}

/// Scratch state shared by the policy/role checkers.
struct EvalContext<'a> {
    catalog: &'a Catalog,
    draft: &'a LoadoutDraft,
    caps: &'a Capabilities,
    violations: Vec<ComplianceIssue>,
    recommendations: Vec<ComplianceRecommendation>,
}

impl EvalContext<'_> {
    /// Record a blocking violation.
    fn violation(&mut self, code: ComplianceRuleCode, message: String, suggested_skus: Vec<String>) {
        self.violations.push(ComplianceIssue {
            code,
            message,
            suggested_skus,
        });
    }

    /// Record a non-blocking recommendation.
    fn recommendation(
        &mut self,
        code: ComplianceRuleCode,
        message: String,
        suggested_skus: Vec<String>,
    ) {
        self.recommendations.push(ComplianceRecommendation {
            code,
            message,
            suggested_skus,
        });
    }

    /// The draft's platform.
    fn platform(&self) -> &PlatformSpec {
        self.draft.platform()
    }

    /// The draft platform's category (`None` for uncategorized platforms).
    fn category(&self) -> Option<PlatformCategory> {
        self.platform().category
    }

    /// Suggestions for a single missing capability.
    fn suggest_capability(&self, tag: CapabilityTag) -> Vec<String> {
        suggest_for_tags(self.catalog, self.platform(), &[tag])
    }

    /// Suggestions for any of a set of alternative capabilities.
    fn suggest_any(&self, tags: &[CapabilityTag]) -> Vec<String> {
        suggest_for_tags(self.catalog, self.platform(), tags)
    }
}

/// Colorway list formatting for messages: `multicam, coyote_brown`.
fn colorway_list(colorways: &[Colorway]) -> String {
    colorways
        .iter()
        .map(|colorway| format!("{colorway:?}").to_lowercase())
        .collect::<Vec<_>>()
        .join(", ")
}

/// AR 670-1 minimum rifle magazine carriage.
const AR670_1_MIN_MAGS: u8 = 3;
/// AR 670-1 recommended rifle magazine carriage.
const AR670_1_RECOMMENDED_MAGS: u8 = 6;
/// Recce minimum rifle magazine carriage.
const RECCE_MIN_MAGS: u8 = 4;

fn check_policy(ctx: &mut EvalContext<'_>, colorway: Colorway, policy: RegulationPolicy) {
    match policy {
        RegulationPolicy::UsArmyAr670_1 => {
            check_allowed_colorways(
                ctx,
                colorway,
                &[Colorway::Multicam, Colorway::CoyoteBrown],
                policy,
            );
            require_capability(
                ctx,
                CapabilityTag::MedicalIfak,
                format!("{} requires a medical IFAK", policy.display_name()),
            );
            check_rifle_capacity(
                ctx,
                AR670_1_MIN_MAGS,
                Some(AR670_1_RECOMMENDED_MAGS),
                format!("{} requires at least {AR670_1_MIN_MAGS} primary rifle magazines", policy.display_name()),
                format!("{} recommends {AR670_1_RECOMMENDED_MAGS} primary rifle magazines", policy.display_name()),
            );
        }
        RegulationPolicy::UsmcMaradmin => {
            check_allowed_colorways(ctx, colorway, &[Colorway::CoyoteBrown], policy);
            require_capability(
                ctx,
                CapabilityTag::MedicalIfak,
                format!("{} requires a medical IFAK", policy.display_name()),
            );
            check_platform_categories(
                ctx,
                &[
                    PlatformCategory::PlateCarrier,
                    PlatformCategory::ChestRig,
                    PlatformCategory::Lbe,
                ],
                format!("{} applies to plate carriers, chest rigs, and LBE only", policy.display_name()),
            );
        }
        RegulationPolicy::LawEnforcementPatrol => {
            check_allowed_colorways(
                ctx,
                colorway,
                &[Colorway::Black, Colorway::RangerGreen, Colorway::WolfGrey],
                policy,
            );
            match ctx.category() {
                Some(PlatformCategory::Belt) => {
                    require_capability(
                        ctx,
                        CapabilityTag::HolsterRetention,
                        format!("{} requires a holster mount on belt platforms", policy.display_name()),
                    );
                }
                _ => {
                    require_any_capability(
                        ctx,
                        &[CapabilityTag::MedicalIfak, CapabilityTag::CommsRoutingOrPtt],
                        format!(
                            "{} requires a medical IFAK or comms routing on carrier platforms",
                            policy.display_name()
                        ),
                    );
                }
            }
        }
        RegulationPolicy::LawEnforcementSwat => {
            check_allowed_colorways(
                ctx,
                colorway,
                &[
                    Colorway::RangerGreen,
                    Colorway::Multicam,
                    Colorway::Black,
                    Colorway::WolfGrey,
                ],
                policy,
            );
            require_capability(
                ctx,
                CapabilityTag::MedicalIfak,
                format!("{} requires a medical IFAK", policy.display_name()),
            );
        }
    }
}

fn check_role(ctx: &mut EvalContext<'_>, role: OperationalRole) {
    match role {
        OperationalRole::Breacher => {
            if !(ctx.caps.has(CapabilityTag::BreachingOrOrdnance)
                || ctx.caps.has(CapabilityTag::HydrationSustainment)
                || ctx.caps.back_panel_mounted)
            {
                let tags = [CapabilityTag::BreachingOrOrdnance, CapabilityTag::HydrationSustainment];
                ctx.violation(
                    ComplianceRuleCode::MissingAnyCapability { capabilities: tags.to_vec() },
                    format!(
                        "{} requires a back panel or breaching/sustainment utility",
                        role.display_name()
                    ),
                    ctx.suggest_any(&tags),
                );
            }
            require_capability(
                ctx,
                CapabilityTag::MedicalIfak,
                format!("{} requires a medical IFAK", role.display_name()),
            );
        }
        OperationalRole::Jtac => {
            require_any_capability(
                ctx,
                &[CapabilityTag::CommsRoutingOrPtt, CapabilityTag::AdminAtakUtility],
                format!(
                    "{} requires comms/PTT routing or an admin/ATAK mount",
                    role.display_name()
                ),
            );
            if !ctx.caps.placard_mounted {
                let suggested = suggest_where(ctx.catalog, ctx.platform(), |component| {
                    component.kind == ComponentKind::Placard
                });
                ctx.violation(
                    ComplianceRuleCode::MissingPlacard,
                    format!("{} requires a front placard", role.display_name()),
                    suggested,
                );
            }
            require_capability(
                ctx,
                CapabilityTag::MedicalIfak,
                format!("{} requires a medical IFAK", role.display_name()),
            );
        }
        OperationalRole::DroneOperator => {
            require_any_capability(
                ctx,
                &[CapabilityTag::AdminAtakUtility, CapabilityTag::HydrationSustainment],
                format!(
                    "{} requires admin/utility storage for the controller and batteries",
                    role.display_name()
                ),
            );
            require_capability(
                ctx,
                CapabilityTag::MedicalIfak,
                format!("{} requires a medical IFAK", role.display_name()),
            );
        }
        OperationalRole::SniperDm => {
            // Prone clearance: warn about stacked child pouches on the front
            // placard (child layer, z-index >= 30).
            for slot_id in front_placard_child_stacks(ctx.draft) {
                ctx.recommendation(
                    ComplianceRuleCode::ProneProfileStacking {
                        slot_id: slot_id.clone(),
                    },
                    format!(
                        "{} prone warning: front placard carries a stacked child pouch at z-layer {CHILD_LAYER} (`{slot_id}`)",
                        role.display_name()
                    ),
                    Vec::new(),
                );
            }
            // Platform preference: chest rigs and LBE sit lower for prone work.
            if ctx.category() == Some(PlatformCategory::PlateCarrier) {
                ctx.recommendation(
                    ComplianceRuleCode::RecommendPlatformCategory {
                        suggested: vec![PlatformCategory::ChestRig, PlatformCategory::Lbe],
                        found: ctx.category(),
                    },
                    format!(
                        "{} prefers a chest rig or LBE (or a slick front) for prone clearance",
                        role.display_name()
                    ),
                    Vec::new(),
                );
            }
        }
        OperationalRole::Recce => {
            check_rifle_capacity(
                ctx,
                RECCE_MIN_MAGS,
                None,
                format!("{} requires at least {RECCE_MIN_MAGS} primary rifle magazines", role.display_name()),
                String::new(),
            );
            require_capability(
                ctx,
                CapabilityTag::HydrationSustainment,
                format!("{} requires hydration or sustainment pack capability", role.display_name()),
            );
            require_any_capability(
                ctx,
                &[CapabilityTag::AdminAtakUtility, CapabilityTag::BreachingOrOrdnance],
                format!(
                    "{} requires sustainment/utility capacity",
                    role.display_name()
                ),
            );
        }
    }
}

// --- shared rule helpers -----------------------------------------------------------

/// Reject colorways outside `allowed`.
fn check_allowed_colorways(
    ctx: &mut EvalContext<'_>,
    chosen: Colorway,
    allowed: &[Colorway],
    policy: RegulationPolicy,
) {
    if !allowed.contains(&chosen) {
        ctx.violation(
            ComplianceRuleCode::DisallowedColorway {
                chosen,
                allowed: allowed.to_vec(),
            },
            format!(
                "{chosen:?} colorway prohibited under {} (allowed: {})",
                policy.display_name(),
                colorway_list(allowed)
            ),
            Vec::new(),
        );
    }
}

/// Require a single capability.
fn require_capability(ctx: &mut EvalContext<'_>, tag: CapabilityTag, message: String) {
    if !ctx.caps.has(tag) {
        ctx.violation(
            ComplianceRuleCode::MissingCapability { capability: tag },
            message,
            ctx.suggest_capability(tag),
        );
    }
}

/// Require at least one of `tags`.
fn require_any_capability(ctx: &mut EvalContext<'_>, tags: &[CapabilityTag], message: String) {
    if !ctx.caps.has_any(tags) {
        ctx.violation(
            ComplianceRuleCode::MissingAnyCapability {
                capabilities: tags.to_vec(),
            },
            message,
            ctx.suggest_any(tags),
        );
    }
}

/// Enforce a minimum rifle magazine count, optionally recommending a higher
/// (non-blocking) carriage level.
fn check_rifle_capacity(
    ctx: &mut EvalContext<'_>,
    minimum: u8,
    recommended: Option<u8>,
    violation_message: String,
    recommendation_message: String,
) {
    let found = ctx.caps.rifle_capacity();
    if found < minimum {
        ctx.violation(
            ComplianceRuleCode::InsufficientRifleMagCapacity { minimum, found },
            format!("{violation_message} (carrying {found})"),
            ctx.suggest_capability(CapabilityTag::RifleMagCapacity(0)),
        );
        return;
    }
    if let Some(recommended) = recommended {
        if found < recommended {
            ctx.recommendation(
                ComplianceRuleCode::RecommendRifleMagCapacity { recommended, found },
                format!("{recommendation_message} (carrying {found})"),
                ctx.suggest_capability(CapabilityTag::RifleMagCapacity(0)),
            );
        }
    }
}

/// Restrict the platform to `allowed` categories.
fn check_platform_categories(
    ctx: &mut EvalContext<'_>,
    allowed: &[PlatformCategory],
    message: String,
) {
    let found = ctx.category();
    if !found.is_some_and(|category| allowed.contains(&category)) {
        ctx.violation(
            ComplianceRuleCode::UnsupportedPlatformCategory {
                allowed: allowed.to_vec(),
                found,
            },
            message,
            Vec::new(),
        );
    }
}

/// Child slot paths whose host chain roots at the platform's
/// `front_placard` slot — stacked pouches that break the prone profile.
///
/// A mounted item counts when it sits at the child layer (`z_index >= 30`,
/// i.e. mounted onto another component) and walking its host chain (via the
/// `<mount_id>.<child_slot_id>` address) terminates at `front_placard`.
fn front_placard_child_stacks(draft: &LoadoutDraft) -> Vec<String> {
    draft
        .mounted()
        .iter()
        .filter(|item| !item.is_adapter && item.z_index >= CHILD_LAYER)
        .filter_map(|item| {
            // Walk the host chain to its platform-slot root.
            let mut cursor = item.slot_id.as_str();
            while let Some((parent_mount_id, _)) = cursor.split_once('.') {
                let parent = draft.mounted_item(parent_mount_id)?;
                cursor = parent.slot_id.as_str();
            }
            (cursor == "front_placard").then(|| item.slot_id.clone())
        })
        .collect()
}

// --- preset instantiation ------------------------------------------------------------

/// Instantiate a [`LoadoutDraft`] from a preset: resolves the platform, then
/// applies every placement **through the normal engine validation path**
/// ([`LoadoutDraft::equip_component`]) — adapter auto-injection and PALS
/// collision rules apply exactly as they do for manual builds.
pub fn apply_preset(catalog: &Catalog, preset: &RolePreset) -> Result<LoadoutDraft, PresetApplyError> {
    let platform = catalog
        .platform(&preset.platform_sku)
        .cloned()
        .ok_or_else(|| PresetApplyError::UnknownPlatform {
            preset_id: preset.preset_id.clone(),
            sku: preset.platform_sku.clone(),
        })?;
    let mut draft = LoadoutDraft::new(platform);
    for (index, placement) in preset.placements.iter().enumerate() {
        draft
            .equip_component(
                catalog,
                &placement.slot_id,
                &placement.component_sku,
                Some((placement.col, placement.row)),
            )
            .map_err(|source| PresetApplyError::PlacementFailed {
                preset_id: preset.preset_id.clone(),
                index: index + 1,
                component_sku: placement.component_sku.clone(),
                slot_id: placement.slot_id.clone(),
                source: Box::new(source),
            })?;
    }
    Ok(draft)
}
