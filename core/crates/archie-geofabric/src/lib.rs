//! The provider graph: what each source of 3D world data is, and what Archie may do with
//! it.
//!
//! The research this implements makes one architectural claim above the others, and it is
//! right: no provider should reach the renderer or Cerebra directly. A source is a set of
//! declared capabilities and a set of *permissions*, and the two are different things.
//!
//! # Permission is not capability
//!
//! Google's photorealistic tiles are technically excellent and contractually narrow. The
//! Map Tiles API policy, read on 2026-09-09, forbids using them for "image analysis,
//! machine interpretation, object detection or identification, geodata extraction or
//! resale" and "offline uses, including for any of the above", and forbids pre-fetching,
//! indexing and storing the content.
//!
//! Those are exactly the things Cerebra does. So the constraint is not that the tiles are
//! low quality — it is that a pixel from that source may be shown to a person and may not
//! be reasoned about, and a system that cannot tell those two apart will eventually do the
//! second while believing it did the first.
//!
//! That is why [`TileProvenance`] cannot be constructed with permissions of its own. It is
//! derived from the provider, and a tile can never claim more than the source it came from
//! allows.
//!
//! # Free, by preference and by default
//!
//! Nelson's constraint is that nothing bills. A metered provider is representable here —
//! pretending Google's tiles do not exist would make the architecture dishonest, not safe
//! — but it is off unless somebody turns it on, it can never become analysable by being
//! turned on, and [`FabricPlan::free_only`] is what ships.
//!
//! The selection order follows from that rather than from image quality: what Archie
//! captured itself outranks open data, which outranks a free account, which outranks a
//! bill. A source Archie owns can be stored, re-examined a year later, and reasoned about.
//! A prettier one that can only be looked at, once, online, is worth less to this system
//! than it looks.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub mod catalogue;
pub mod exploration;
pub mod globe;

pub use catalogue::{provider, GeoProviderCapabilities, PROVIDERS};

/// A source of 3D world data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoProviderId {
    // Declaration order is deliberately the opposite of the order these are chosen in.
    // Cost and `preference` decide that, and if either were ever dropped from the sort,
    // this arrangement makes the mistake show up as the paid source winning rather than
    // as nothing at all.
    /// Google Photorealistic 3D Tiles.
    GooglePhotorealistic,
    /// Blackshark.ai's synthetic semantic globe.
    BlacksharkSynth3d,
    /// Vantor Vivid Terrain and WorldView 3D.
    VantorVivid,
    /// Cesium's hosted OSM buildings tileset.
    CesiumOsmBuildings,
    /// Microsoft's machine-learning building footprints.
    MicrosoftBuildingFootprints,
    /// OpenStreetMap extracts, as the installer already downloads them.
    OpenStreetMap,
    /// Copernicus GLO-30 elevation.
    CopernicusDem30,
    /// Public NASA/EOSDIS GIBS raster visualizations, including HLS imagery.
    NasaGibs,
    /// Public Mapzen/Tileszen terrain GeoTIFFs hosted in AWS Open Data.
    AwsTerrainTiles,
    /// Photogrammetry from Archie's own drone, phone or robot captures.
    ArchiePhotogrammetry,
    /// Gaussian splats trained from Archie's own captures.
    ArchieGaussianSplat,
}

/// What kind of data a provider supplies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoDataClass {
    /// Elevation, as a surface.
    Terrain,
    /// Buildings and features with identity and attributes, not just shape.
    SemanticBuildings,
    /// A continuous textured surface. Looks like the place; knows nothing about it.
    PhotorealMesh,
    /// Gaussian splats: appearance without clean geometry.
    Radiance,
    /// Raster surface imagery.
    Imagery,
    /// Point clouds, CAD, BIM — measured geometry meant to be aligned with design.
    Engineering,
}

/// Where a provider has anything to offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageClass {
    Global,
    /// Named in plain words, because "regional" alone tells a reader nothing.
    Regional(&'static str),
    /// Only where Archie has been. Grows by going there.
    WhereArchieHasCaptured,
}

/// What it costs to read a provider.
///
/// The ordering is the preference: cheaper and freer sorts first, and
/// [`FabricPlan::source_for`] uses that directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cost {
    /// Archie captured it. Nothing to pay, nobody to ask, and it never expires.
    Owned,
    /// Open data. No account, no key, no invoice.
    FreeAndOpen,
    /// Free, and still gated behind an account whose token lives in this variable.
    FreeWithAccount { env_var: &'static str },
    /// Billed. Off unless somebody deliberately turns it on.
    Metered { note: &'static str },
}

impl Cost {
    pub const fn bills(self) -> bool {
        matches!(self, Self::Metered { .. })
    }
}

/// Why the fabric refused something.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FabricRefusal {
    #[error(
        "{provider:?} may not be reasoned about: {license}. It can be shown to a person \
         and nothing more"
    )]
    MachineAnalysisForbidden {
        provider: GeoProviderId,
        license: &'static str,
    },
    #[error("{provider:?} may not be stored: {license}")]
    OfflineForbidden {
        provider: GeoProviderId,
        license: &'static str,
    },
    #[error("{provider:?} bills for use ({note}); enable it deliberately or not at all")]
    WouldBill {
        provider: GeoProviderId,
        note: &'static str,
    },
    #[error("{provider:?} needs an account token in {env_var}, which is not set")]
    MissingAccount {
        provider: GeoProviderId,
        env_var: &'static str,
    },
}

/// Where a tile came from, and what may be done with it.
///
/// The permissions are private and derived, never supplied. A tile that could assert its
/// own rights would eventually assert the convenient ones, and the failure would be
/// silent: a correct-looking analysis over pixels that were licensed only to be looked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileProvenance {
    provider: GeoProviderId,
    /// When Archie observed it, for its own captures. `None` for a streamed third party,
    /// which does not say.
    captured_at_ms: Option<i64>,
    machine_analysis: bool,
    offline: bool,
}

impl TileProvenance {
    /// Stamp a tile with the rights of the source it came from.
    ///
    /// The only way to make one. There is no constructor that takes permissions.
    pub fn from_provider(id: GeoProviderId, captured_at_ms: Option<i64>) -> Self {
        let capabilities = provider(id);
        Self {
            provider: id,
            captured_at_ms,
            machine_analysis: capabilities.machine_analysis_allowed,
            offline: capabilities.offline_allowed,
        }
    }

    pub const fn provider(&self) -> GeoProviderId {
        self.provider
    }

    pub const fn captured_at_ms(&self) -> Option<i64> {
        self.captured_at_ms
    }

    /// Whether this tile may be shown. Everything here may be shown; that is the point of
    /// having it.
    pub const fn may_be_displayed(&self) -> bool {
        true
    }

    /// Whether Cerebra may reason about the content.
    pub fn may_be_reasoned_about(&self) -> Result<(), FabricRefusal> {
        if self.machine_analysis {
            return Ok(());
        }
        Err(FabricRefusal::MachineAnalysisForbidden {
            provider: self.provider,
            license: provider(self.provider).license,
        })
    }

    /// Whether the bytes may be written to disk and used later without the network.
    pub fn may_be_cached(&self) -> Result<(), FabricRefusal> {
        if self.offline {
            return Ok(());
        }
        Err(FabricRefusal::OfflineForbidden {
            provider: self.provider,
            license: provider(self.provider).license,
        })
    }

    /// A source that may be shown and not analysed. Worth naming, because it is the
    /// category that causes accidents.
    pub const fn is_display_only(&self) -> bool {
        !self.machine_analysis
    }
}

/// Which providers this installation is actually allowed to use.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FabricPlan {
    enabled: BTreeSet<GeoProviderId>,
}

impl FabricPlan {
    /// What ships: everything that costs nothing and needs nobody's permission.
    ///
    /// This is not a reduced mode. Open elevation, OpenStreetMap and Microsoft's footprints
    /// cover the whole planet with terrain and semantic buildings, and Archie's own
    /// captures are the only thing with real detail at street level anyway.
    pub fn free_only() -> Self {
        Self {
            enabled: PROVIDERS
                .iter()
                .filter(|capabilities| matches!(capabilities.cost, Cost::Owned | Cost::FreeAndOpen))
                .map(|capabilities| capabilities.id)
                .collect(),
        }
    }

    /// Turn a provider on.
    ///
    /// A billing provider is refused here rather than silently accepted, so that switching
    /// one on is a decision somebody made and can be found later. Use
    /// [`FabricPlan::enabling_billed_source`] to say it out loud.
    pub fn enabling(mut self, id: GeoProviderId) -> Result<Self, FabricRefusal> {
        let capabilities = provider(id);
        match capabilities.cost {
            Cost::Metered { note } => Err(FabricRefusal::WouldBill { provider: id, note }),
            Cost::FreeWithAccount { env_var } if std::env::var_os(env_var).is_none() => {
                Err(FabricRefusal::MissingAccount {
                    provider: id,
                    env_var,
                })
            }
            _ => {
                self.enabled.insert(id);
                Ok(self)
            }
        }
    }

    /// Turn on a provider that bills, having decided to.
    ///
    /// Separate from [`FabricPlan::enabling`] because the difference between free and
    /// billed should be visible at the call site rather than in a config file somebody
    /// edited a year ago. It does not widen what the source is licensed for: a
    /// display-only tile stays display-only however deliberately it was enabled.
    pub fn enabling_billed_source(mut self, id: GeoProviderId) -> Self {
        self.enabled.insert(id);
        self
    }

    pub fn is_enabled(&self, id: GeoProviderId) -> bool {
        self.enabled.contains(&id)
    }

    pub fn enabled(&self) -> impl Iterator<Item = &GeoProviderId> {
        self.enabled.iter()
    }

    /// Whether anything enabled here will produce a bill.
    pub fn bills_anything(&self) -> bool {
        self.enabled.iter().any(|id| provider(*id).cost.bills())
    }

    /// The best source for a kind of data at a given zoom level.
    ///
    /// The scale matters because no provider is good at all of them: open elevation is the
    /// whole planet and says nothing about a street, and a drone capture is a hectare in
    /// extraordinary detail. Among the sources that serve this class at this scale, the
    /// order is what Archie owns, then open data, then a free account, then a bill —
    /// which is a statement about what is useful to this system, not about which looks
    /// best.
    pub fn source_for(&self, class: GeoDataClass, lod: u8) -> Option<GeoProviderId> {
        // Delegates rather than repeating the ordering. Two copies of a preference rule
        // drift, and the drift would be invisible: both would keep returning a plausible
        // provider, just not the same one.
        self.sources_for(class, lod).first().copied()
    }

    /// Every source that could serve this class at this scale, best first.
    pub fn sources_for(&self, class: GeoDataClass, lod: u8) -> Vec<GeoProviderId> {
        let mut candidates: Vec<&GeoProviderCapabilities> = self
            .enabled
            .iter()
            .map(|id| provider(*id))
            .filter(|capabilities| capabilities.serves(class, lod))
            .collect();
        candidates.sort_by_key(|capabilities| {
            (capabilities.cost, capabilities.preference, capabilities.id)
        });
        candidates
            .into_iter()
            .map(|capabilities| capabilities.id)
            .collect()
    }
}
