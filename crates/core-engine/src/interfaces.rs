//! Strongly typed attachment interface standards.
//!
//! Compatibility rules in this crate are expressed exclusively through these
//! enums — there is deliberately no free-text string matching anywhere in the
//! engine. Adding a new standard means adding a variant (and, where relevant,
//! an adapter entry in the caller's registry), never editing match arms on
//! product names.

use serde::{Deserialize, Serialize};

/// The physical attachment family an [`InterfaceStandard`] belongs to.
///
/// A component can only mount — directly or through a single adapter hop —
/// onto a slot of the same family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InterfaceFamily {
    /// Front placards and chest-rig flaps.
    Placard,
    /// Front face of a cummerbund.
    CummerbundFront,
    /// Rear face of a cummerbund.
    CummerbundRear,
    /// Zip-on back panels and rear MOLLE fields.
    BackPanel,
    /// Belts and belt-mounted pouches.
    Belt,
}

/// Every attachment standard known to the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InterfaceStandard {
    // --- Placard / front flap standards ---
    /// 1" QASM SwiftClip buckle — the de-facto cross-brand placard standard.
    SwiftClip1InchQasm,
    /// Low-profile G-Hook tuck mount.
    GhookLowProfile,
    /// Crye AVS / JPC detachable flap latch.
    CryeAvsFlap,
    /// Shaw Concepts ARC quad-point placard attachment.
    ShawArcQuad,

    // --- Cummerbund front standards ---
    /// Hook-and-loop flap closure.
    VelcroFlap,
    /// FirstSpear 2" Tubes (male/female).
    FirstSpearTubes2In,
    /// FirstSpear 4" Tubes.
    FirstSpearTubes4In,
    /// ROC 80 mm side-release buckle.
    RocBuckle80,

    // --- Cummerbund rear standards ---
    /// Shock-cord lace-through attachment.
    ShockCordLace,
    /// Internal hook-and-loop loop field.
    InternalVelcroLoop,
    /// Pass-through skeletal webbing channel.
    PassThroughSkeletal,
    /// Proprietary rigid TEGris skeletal channel.
    ProprietaryTegris,

    // --- Back panel standards ---
    /// Crye Precision #10 heavy zipper.
    CryeZip10,
    /// Ferro Concepts proprietary zipper.
    FerroZip,
    /// Spiritus Systems proprietary zipper.
    SpiritusZip,
    /// Velocity Systems proprietary YKK #10.
    VelocityYkk10,
    /// Plain MOLLE/PALS back field (no zipper track).
    StandardMolle,

    // --- Belt standards ---
    /// Inner hook / outer loop hook-and-loop belt stack.
    InnerHookOuterLoop,
    /// Inner loop / outer hook hook-and-loop belt stack.
    InnerLoopOuterHook,
    /// Standard 1" PALS webbing.
    StandardPals1In,
    /// Micro 0.5" PALS webbing.
    MicroPals05In,
}

impl InterfaceStandard {
    /// Every variant, in declaration order (useful for UI pickers).
    pub const ALL: [InterfaceStandard; 21] = [
        Self::SwiftClip1InchQasm,
        Self::GhookLowProfile,
        Self::CryeAvsFlap,
        Self::ShawArcQuad,
        Self::VelcroFlap,
        Self::FirstSpearTubes2In,
        Self::FirstSpearTubes4In,
        Self::RocBuckle80,
        Self::ShockCordLace,
        Self::InternalVelcroLoop,
        Self::PassThroughSkeletal,
        Self::ProprietaryTegris,
        Self::CryeZip10,
        Self::FerroZip,
        Self::SpiritusZip,
        Self::VelocityYkk10,
        Self::StandardMolle,
        Self::InnerHookOuterLoop,
        Self::InnerLoopOuterHook,
        Self::StandardPals1In,
        Self::MicroPals05In,
    ];

    /// The attachment family this standard belongs to.
    pub const fn family(&self) -> InterfaceFamily {
        match self {
            Self::SwiftClip1InchQasm
            | Self::GhookLowProfile
            | Self::CryeAvsFlap
            | Self::ShawArcQuad => InterfaceFamily::Placard,
            Self::VelcroFlap
            | Self::FirstSpearTubes2In
            | Self::FirstSpearTubes4In
            | Self::RocBuckle80 => InterfaceFamily::CummerbundFront,
            Self::ShockCordLace
            | Self::InternalVelcroLoop
            | Self::PassThroughSkeletal
            | Self::ProprietaryTegris => InterfaceFamily::CummerbundRear,
            Self::CryeZip10
            | Self::FerroZip
            | Self::SpiritusZip
            | Self::VelocityYkk10
            | Self::StandardMolle => InterfaceFamily::BackPanel,
            Self::InnerHookOuterLoop
            | Self::InnerLoopOuterHook
            | Self::StandardPals1In
            | Self::MicroPals05In => InterfaceFamily::Belt,
        }
    }

    /// Whether this standard is a zipper track (as opposed to a MOLLE field),
    /// relevant within the back-panel family.
    pub const fn is_zipper(&self) -> bool {
        matches!(
            self,
            Self::CryeZip10 | Self::FerroZip | Self::SpiritusZip | Self::VelocityYkk10
        )
    }
}

/// Armor plate cuts a platform can accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlateCut {
    /// SAPI / ESAPI cut.
    SapiEsapi,
    /// Swimmer cut.
    Swimmer,
    /// Shooter / 10x12 rectangular cut.
    Shooter,
}

/// Platform and armor sizing scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Size {
    /// Small.
    Small,
    /// Medium.
    Medium,
    /// Large.
    Large,
    /// Extra large.
    XLarge,
}
