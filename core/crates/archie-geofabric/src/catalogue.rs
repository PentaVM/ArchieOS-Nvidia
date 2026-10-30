//! The providers, as they were found.
//!
//! Every licence claim here was read from the source's own terms on the date recorded
//! against it, not taken from a comparison table. The one that decides the architecture is
//! Google's, and it is worth having in front of you rather than summarised: the Map Tiles
//! API policy forbids "image analysis, machine interpretation, object detection or
//! identification, geodata extraction or resale" and "offline uses, including for any of
//! the above".

use serde::Serialize;

use crate::{Cost, CoverageClass, GeoDataClass, GeoProviderId};

/// What a provider is, and what may be done with what it returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GeoProviderCapabilities {
    pub id: GeoProviderId,
    pub authority: &'static str,
    pub product: &'static str,
    pub coverage: CoverageClass,
    pub data_classes: &'static [GeoDataClass],

    /// Whether it looks like the real place.
    pub photorealistic: bool,
    /// Whether it carries entities with identity and attributes, rather than a surface.
    pub semantic_entities: bool,

    /// Whether the licence permits reasoning about the content. A licence question, not a
    /// technical one — every one of these is technically analysable.
    pub machine_analysis_allowed: bool,
    /// Whether the licence permits storing it and using it without the network.
    pub offline_allowed: bool,
    /// Whether Archie may serve it from its own machine.
    pub self_hostable: bool,

    pub supports_3d_tiles: bool,
    pub supports_gaussian_splats: bool,

    /// The zoom levels at which this source has something real to say, inclusive.
    pub useful_lod: (u8, u8),
    /// Which source to reach for first among equally-priced ones, lower first.
    ///
    /// Explicit because the alternative is a tie broken by declaration order, which is a
    /// real decision made invisibly — and the two open building sources are a real tie.
    pub preference: u8,

    pub cost: Cost,
    /// The licence, named as the source names it.
    pub license: &'static str,
    /// What must appear on screen wherever this is drawn.
    pub attribution: &'static str,
    /// The date the terms and access were last actually read, ISO 8601.
    pub verified_on: &'static str,
}

impl GeoProviderCapabilities {
    /// Whether this provider offers this kind of data at this scale.
    pub fn serves(&self, class: GeoDataClass, lod: u8) -> bool {
        self.data_classes.contains(&class) && lod >= self.useful_lod.0 && lod <= self.useful_lod.1
    }

    /// Whether Archie can keep this: store it, look at it again next year, reason about it.
    ///
    /// The distinction that matters when choosing between a beautiful stream and a plainer
    /// thing you own.
    pub const fn is_archies_to_keep(&self) -> bool {
        self.offline_allowed && self.machine_analysis_allowed && self.self_hostable
    }
}

const TERRAIN: &[GeoDataClass] = &[GeoDataClass::Terrain];
const BUILDINGS: &[GeoDataClass] = &[GeoDataClass::SemanticBuildings];
const MESH: &[GeoDataClass] = &[GeoDataClass::PhotorealMesh];

pub const PROVIDERS: [GeoProviderCapabilities; 11] = [
    GeoProviderCapabilities {
        id: GeoProviderId::NasaGibs,
        authority: "NASA EOSDIS Global Imagery Browse Services",
        product: "Open satellite raster visualizations: MODIS, VIIRS and HLS Landsat/Sentinel",
        coverage: CoverageClass::Global,
        data_classes: &[GeoDataClass::Imagery],
        photorealistic: true,
        semantic_entities: false,
        machine_analysis_allowed: true,
        offline_allowed: true,
        self_hostable: true,
        supports_3d_tiles: false,
        supports_gaussian_splats: false,
        useful_lod: (0, 12),
        preference: 10,
        cost: Cost::FreeAndOpen,
        license: "Public NASA/EOSDIS visualizations with source-specific NASA, USGS Landsat and Copernicus Sentinel credits; native resolution, date and coverage depend on the selected product",
        attribution: "NASA EOSDIS GIBS; HLS Landsat courtesy USGS; HLS Sentinel contains modified Copernicus Sentinel data",
        verified_on: "2026-10-24",
    },
    GeoProviderCapabilities {
        id: GeoProviderId::AwsTerrainTiles,
        authority: "Mapzen / Tilezen, hosted by AWS Open Data",
        product: "Terrain Tiles GeoTIFF",
        coverage: CoverageClass::Global,
        data_classes: TERRAIN,
        photorealistic: false,
        semantic_entities: false,
        machine_analysis_allowed: true,
        offline_allowed: true,
        self_hostable: true,
        supports_3d_tiles: false,
        supports_gaussian_splats: false,
        useful_lod: (0, 14),
        preference: 20,
        cost: Cost::FreeAndOpen,
        license: "Open terrain datasets; source-specific attribution: https://github.com/tilezen/joerd/blob/master/docs/attribution.md",
        attribution: "Terrain: Mapzen / Tilezen; SRTM, GMTED2010 and 3DEP courtesy USGS; NOAA ETOPO1; regional credits in capture attribution",
        verified_on: "2026-10-24",
    },
    // ------------------------------------------------------------------ what Archie owns
    GeoProviderCapabilities {
        id: GeoProviderId::ArchiePhotogrammetry,
        authority: "Archie",
        product: "Photogrammetry from Archie's own captures (OpenDroneMap, OpenMVS)",
        coverage: CoverageClass::WhereArchieHasCaptured,
        data_classes: &[
            GeoDataClass::PhotorealMesh,
            GeoDataClass::Engineering,
            GeoDataClass::Imagery,
            GeoDataClass::Terrain,
        ],
        photorealistic: true,
        semantic_entities: false,
        machine_analysis_allowed: true,
        offline_allowed: true,
        self_hostable: true,
        supports_3d_tiles: true,
        supports_gaussian_splats: false,
        // Nothing else has anything real to say this close in.
        useful_lod: (16, 24),
        // Archie's own capture: the only thing here with real detail, and the only thing it can keep without asking anybody.
        preference: 0,
        cost: Cost::Owned,
        license: "Archie's own capture; no third-party terms apply",
        attribution: "Captured by Archie",
        verified_on: "2026-09-09",
    },
    GeoProviderCapabilities {
        id: GeoProviderId::ArchieGaussianSplat,
        authority: "Archie",
        product: "Gaussian splats trained from Archie's own captures",
        coverage: CoverageClass::WhereArchieHasCaptured,
        data_classes: &[GeoDataClass::Radiance],
        photorealistic: true,
        // Splats are appearance. Identity and shape come from a layer beside them, which
        // is why radiance is its own class rather than a better mesh.
        semantic_entities: false,
        machine_analysis_allowed: true,
        offline_allowed: true,
        self_hostable: true,
        supports_3d_tiles: true,
        supports_gaussian_splats: true,
        useful_lod: (18, 24),
        // Owned, like the photogrammetry beside it.
        preference: 0,
        cost: Cost::Owned,
        license: "Archie's own capture; no third-party terms apply",
        attribution: "Captured by Archie",
        verified_on: "2026-09-09",
    },
    // ------------------------------------------------------------------ open data
    GeoProviderCapabilities {
        id: GeoProviderId::CopernicusDem30,
        authority: "European Space Agency, via the AWS Open Data registry",
        product: "Copernicus DEM GLO-30 elevation",
        coverage: CoverageClass::Global,
        data_classes: TERRAIN,
        photorealistic: false,
        semantic_entities: false,
        machine_analysis_allowed: true,
        offline_allowed: true,
        self_hostable: true,
        supports_3d_tiles: false,
        supports_gaussian_splats: false,
        // 30 m posts: the shape of the planet, and nothing about a street.
        useful_lod: (0, 13),
        // The only open global elevation source here.
        preference: 10,
        cost: Cost::FreeAndOpen,
        license: "Free for the general public under the Copernicus DEM terms; \
                  the S3 bucket takes unsigned requests and needs no account",
        attribution: "© DLR e.V. 2010-2014 and © Airbus Defence and Space GmbH, \
                      Copernicus DEM",
        verified_on: "2026-09-09",
    },
    GeoProviderCapabilities {
        id: GeoProviderId::OpenStreetMap,
        authority: "OpenStreetMap contributors",
        product: "OpenStreetMap extracts, as the installer already downloads them",
        coverage: CoverageClass::Global,
        // Not elevation. OSM carries the odd `ele` tag and no surface, and a fabric that
        // asked it for terrain would get nothing where it expected the shape of the ground.
        data_classes: BUILDINGS,
        photorealistic: false,
        // The reason it is here: names, addresses, heights, uses. A mesh knows none of it.
        semantic_entities: true,
        machine_analysis_allowed: true,
        offline_allowed: true,
        self_hostable: true,
        supports_3d_tiles: false,
        supports_gaussian_splats: false,
        useful_lod: (8, 20),
        // First among the open building sources: OSM is the one that carries names, addresses and uses, which is what makes a building an entity rather than a shape.
        preference: 10,
        cost: Cost::FreeAndOpen,
        license: "Open Database License (ODbL) 1.0",
        attribution: "© OpenStreetMap contributors",
        verified_on: "2026-09-09",
    },
    GeoProviderCapabilities {
        id: GeoProviderId::MicrosoftBuildingFootprints,
        authority: "Microsoft",
        product: "Global ML Building Footprints, ~1.4 billion buildings with heights on 174M",
        coverage: CoverageClass::Global,
        data_classes: BUILDINGS,
        photorealistic: false,
        semantic_entities: true,
        machine_analysis_allowed: true,
        offline_allowed: true,
        self_hostable: true,
        supports_3d_tiles: false,
        supports_gaussian_splats: false,
        useful_lod: (12, 20),
        // Second, and not because it is worse: it has coverage where OSM is thin and
        // heights on 174M of them, but no names. It complements rather than replaces.
        preference: 20,
        cost: Cost::FreeAndOpen,
        license: "Community Data License Agreement, Permissive, Version 2.0; \
                  public blob storage, no account or key",
        attribution: "Microsoft Global ML Building Footprints",
        verified_on: "2026-09-09",
    },
    // ------------------------------------------------------------------ free, with an account
    GeoProviderCapabilities {
        id: GeoProviderId::CesiumOsmBuildings,
        authority: "Cesium",
        product: "Cesium OSM Buildings, over 350 million buildings as 3D Tiles",
        coverage: CoverageClass::Global,
        data_classes: BUILDINGS,
        photorealistic: false,
        semantic_entities: true,
        machine_analysis_allowed: true,
        // Streamed from Cesium ion. There is no download, so there is nothing to keep.
        offline_allowed: false,
        self_hostable: false,
        supports_3d_tiles: true,
        supports_gaussian_splats: false,
        useful_lod: (12, 20),
        // The same OSM data, streamed. Behind the copy Archie can hold in its hand.
        preference: 30,
        cost: Cost::FreeWithAccount {
            env_var: "ARCHIE_CESIUM_ION_TOKEN",
        },
        license: "Derived from OpenStreetMap under ODbL; streamed under Cesium's \
                  Global 3D Content terms, which require an ion account token",
        attribution: "© OpenStreetMap contributors, via Cesium",
        verified_on: "2026-09-09",
    },
    // ------------------------------------------------------------------ billed
    GeoProviderCapabilities {
        id: GeoProviderId::GooglePhotorealistic,
        authority: "Google",
        product: "Photorealistic 3D Tiles, over 2,500 cities",
        coverage: CoverageClass::Global,
        data_classes: MESH,
        photorealistic: true,
        semantic_entities: false,
        // The decisive one. Not a judgement about quality — a quotation from the terms.
        machine_analysis_allowed: false,
        offline_allowed: false,
        self_hostable: false,
        supports_3d_tiles: true,
        supports_gaussian_splats: false,
        useful_lod: (14, 20),
        // Display only, and billed.
        preference: 30,
        cost: Cost::Metered {
            note: "Map Tiles API is billed per request and needs a billing account enabled",
        },
        license: "Map Tiles API policies: no image analysis, machine interpretation, \
                  object detection or identification, geodata extraction or resale, and \
                  no offline use; pre-fetching, indexing and storing are forbidden",
        attribution: "Google",
        verified_on: "2026-09-09",
    },
    GeoProviderCapabilities {
        id: GeoProviderId::BlacksharkSynth3d,
        authority: "Blackshark.ai",
        product: "SYNTH3D synthetic semantic globe",
        coverage: CoverageClass::Global,
        data_classes: &[GeoDataClass::SemanticBuildings, GeoDataClass::Terrain],
        // Buildings are geotypical: the right shape and archetype for the region, not the
        // actual facade of the actual building.
        photorealistic: false,
        semantic_entities: true,
        machine_analysis_allowed: true,
        offline_allowed: true,
        self_hostable: true,
        supports_3d_tiles: true,
        supports_gaussian_splats: false,
        useful_lod: (8, 18),
        // Commercial semantic globe.
        preference: 20,
        cost: Cost::Metered {
            note: "commercial licence, negotiated per contract",
        },
        license: "Commercial; offline and self-hosted datasets are available under \
                  contract, so the permissions above hold only for such a contract",
        attribution: "Blackshark.ai SYNTH3D",
        verified_on: "2026-09-09",
    },
    GeoProviderCapabilities {
        id: GeoProviderId::VantorVivid,
        authority: "Vantor",
        product: "Vivid Terrain and WorldView 3D",
        coverage: CoverageClass::Global,
        data_classes: &[GeoDataClass::Terrain, GeoDataClass::Imagery],
        photorealistic: true,
        semantic_entities: false,
        machine_analysis_allowed: true,
        offline_allowed: true,
        self_hostable: true,
        supports_3d_tiles: true,
        supports_gaussian_splats: false,
        useful_lod: (6, 17),
        // Commercial precision terrain.
        preference: 20,
        cost: Cost::Metered {
            note: "commercial licence, priced by area of interest",
        },
        license: "Commercial; permissions above hold only under such a licence",
        attribution: "Vantor",
        verified_on: "2026-09-09",
    },
];

/// Look a provider up.
pub fn provider(id: GeoProviderId) -> &'static GeoProviderCapabilities {
    PROVIDERS
        .iter()
        .find(|capabilities| capabilities.id == id)
        // Held there by `every_provider_id_has_an_entry`.
        .expect("every provider id has a catalogue entry")
}
