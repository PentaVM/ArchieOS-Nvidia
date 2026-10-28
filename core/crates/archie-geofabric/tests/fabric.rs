//! What may be looked at, what may be thought about, and what may be kept.
//!
//! These are three different permissions, and every accident this crate exists to prevent
//! comes from a system that had only one.

use archie_geofabric::{
    provider, Cost, CoverageClass, FabricPlan, FabricRefusal, GeoDataClass, GeoProviderId,
    TileProvenance, PROVIDERS,
};

const EVERY_ID: [GeoProviderId; 11] = [
    GeoProviderId::OpenStreetMap,
    GeoProviderId::MicrosoftBuildingFootprints,
    GeoProviderId::CopernicusDem30,
    GeoProviderId::NasaGibs,
    GeoProviderId::AwsTerrainTiles,
    GeoProviderId::ArchiePhotogrammetry,
    GeoProviderId::ArchieGaussianSplat,
    GeoProviderId::CesiumOsmBuildings,
    GeoProviderId::GooglePhotorealistic,
    GeoProviderId::BlacksharkSynth3d,
    GeoProviderId::VantorVivid,
];

#[test]
fn open_satellite_imagery_is_discoverable_without_enabling_a_billed_provider() {
    let plan = FabricPlan::free_only();
    assert_eq!(
        plan.source_for(GeoDataClass::Imagery, 12),
        Some(GeoProviderId::NasaGibs)
    );
    let tile = TileProvenance::from_provider(GeoProviderId::NasaGibs, None);
    assert!(tile.may_be_cached().is_ok());
    assert!(tile.may_be_reasoned_about().is_ok());
    assert!(!provider(GeoProviderId::NasaGibs).serves(GeoDataClass::Imagery, 13));
    assert!(!plan.is_enabled(GeoProviderId::GooglePhotorealistic));
}

// ------------------------------------------------------------- display is not analysis

/// The decisive one, and the reason this crate exists.
///
/// Google's tiles are excellent and contractually narrow: shown to a person, yes; analysed,
/// stored, or interpreted by a machine, no. A system that cannot tell those apart will
/// eventually do the second while believing it did the first.
#[test]
fn a_display_only_tile_may_be_shown_and_never_reasoned_about() {
    let tile = TileProvenance::from_provider(GeoProviderId::GooglePhotorealistic, None);

    assert!(tile.may_be_displayed());
    assert!(tile.is_display_only());

    let refusal = tile.may_be_reasoned_about().expect_err("must be refused");
    assert!(matches!(
        refusal,
        FabricRefusal::MachineAnalysisForbidden {
            provider: GeoProviderId::GooglePhotorealistic,
            ..
        }
    ));
    // The refusal quotes the terms, so nobody has to go and find out why.
    assert!(
        refusal.to_string().contains("machine interpretation"),
        "{refusal}"
    );
    assert!(
        tile.may_be_cached().is_err(),
        "storing it is also forbidden"
    );
}

/// A tile cannot grant itself rights its source does not have.
///
/// There is no constructor that takes permissions. If there were, the convenient ones would
/// eventually get passed, and the resulting analysis would look entirely correct.
#[test]
fn permissions_come_from_the_source_and_cannot_be_asserted_by_a_tile() {
    let forbidden = TileProvenance::from_provider(GeoProviderId::GooglePhotorealistic, None);
    let owned = TileProvenance::from_provider(GeoProviderId::ArchiePhotogrammetry, Some(1_000));

    assert!(forbidden.may_be_reasoned_about().is_err());
    assert!(owned.may_be_reasoned_about().is_ok());

    // Round-tripping is not a way to launder the permissions either.
    let text = serde_json::to_string(&forbidden).expect("serialisable");
    let back: TileProvenance = serde_json::from_str(&text).expect("deserialisable");
    assert_eq!(back, forbidden);
    assert!(back.may_be_reasoned_about().is_err());
}

/// What Archie captured itself carries no third-party terms at all.
#[test]
fn archies_own_capture_may_be_kept_and_thought_about() {
    for id in [
        GeoProviderId::ArchiePhotogrammetry,
        GeoProviderId::ArchieGaussianSplat,
    ] {
        let tile = TileProvenance::from_provider(id, Some(1_757_000_000_000));
        assert!(tile.may_be_reasoned_about().is_ok(), "{id:?}");
        assert!(tile.may_be_cached().is_ok(), "{id:?}");
        assert!(!tile.is_display_only(), "{id:?}");
        assert_eq!(tile.captured_at_ms(), Some(1_757_000_000_000));
        assert!(provider(id).is_archies_to_keep());
    }
}

/// A streamed source is not Archie's to keep, however permissive its data licence is.
///
/// Cesium's buildings are OpenStreetMap underneath and perfectly analysable — and there is
/// no download, so there is nothing on disk when the network is gone.
#[test]
fn a_streamed_source_may_be_analysed_and_still_cannot_be_kept() {
    let tile = TileProvenance::from_provider(GeoProviderId::CesiumOsmBuildings, None);
    assert!(tile.may_be_reasoned_about().is_ok());
    assert!(tile.may_be_cached().is_err());
    assert!(!provider(GeoProviderId::CesiumOsmBuildings).is_archies_to_keep());
}

// ------------------------------------------------------------- nothing bills by default

/// What ships bills nothing.
#[test]
fn the_shipping_plan_costs_nothing_and_asks_nobody() {
    let plan = FabricPlan::free_only();
    assert!(!plan.bills_anything());

    for id in plan.enabled() {
        let capabilities = provider(*id);
        assert!(!capabilities.cost.bills(), "{id:?}");
        assert!(
            matches!(capabilities.cost, Cost::Owned | Cost::FreeAndOpen),
            "{id:?} needs an account and must not be on by default"
        );
    }

    assert!(!plan.is_enabled(GeoProviderId::GooglePhotorealistic));
    assert!(!plan.is_enabled(GeoProviderId::BlacksharkSynth3d));
    assert!(!plan.is_enabled(GeoProviderId::VantorVivid));
    assert!(!plan.is_enabled(GeoProviderId::CesiumOsmBuildings));
}

/// Turning on a billing provider by the ordinary route is refused, with the reason.
#[test]
fn a_billing_provider_cannot_be_switched_on_by_accident() {
    let refused = FabricPlan::free_only().enabling(GeoProviderId::GooglePhotorealistic);
    assert!(matches!(
        refused,
        Err(FabricRefusal::WouldBill {
            provider: GeoProviderId::GooglePhotorealistic,
            ..
        })
    ));
}

/// Deciding to pay does not widen what the licence allows.
///
/// This is the trap: having paid for something feels like having bought the right to use
/// it however you like, and here it buys pixels and nothing else.
#[test]
fn paying_for_a_source_does_not_make_it_analysable() {
    let plan = FabricPlan::free_only().enabling_billed_source(GeoProviderId::GooglePhotorealistic);

    assert!(plan.is_enabled(GeoProviderId::GooglePhotorealistic));
    assert!(plan.bills_anything(), "and the plan says so");

    let tile = TileProvenance::from_provider(GeoProviderId::GooglePhotorealistic, None);
    assert!(tile.may_be_reasoned_about().is_err());
    assert!(tile.may_be_cached().is_err());
}

/// A free-with-account provider stays off while its token is missing.
#[test]
fn an_account_that_does_not_exist_does_not_enable_a_provider() {
    // Nothing in this process sets it, and a test must not set a real one.
    let refused = FabricPlan::free_only().enabling(GeoProviderId::CesiumOsmBuildings);
    match refused {
        Err(FabricRefusal::MissingAccount { env_var, .. }) => {
            assert_eq!(env_var, "ARCHIE_CESIUM_ION_TOKEN");
        }
        other => panic!("expected a missing-account refusal, got {other:?}"),
    }
}

// ------------------------------------------------------------- choosing a source

/// The free stack alone covers the planet: elevation everywhere, buildings with names.
///
/// This is the claim that makes "free only" a real default rather than a degraded one.
#[test]
fn the_free_stack_alone_covers_the_whole_planet() {
    let plan = FabricPlan::free_only();

    for lod in 0..=13 {
        assert_eq!(
            plan.source_for(GeoDataClass::Terrain, lod),
            Some(GeoProviderId::CopernicusDem30),
            "no elevation at zoom {lod}"
        );
    }
    for lod in 12..=20 {
        assert!(
            plan.source_for(GeoDataClass::SemanticBuildings, lod)
                .is_some(),
            "no buildings at zoom {lod}"
        );
    }
}

/// Close in, only Archie's own capture has anything real to say.
#[test]
fn at_street_level_the_only_real_detail_is_what_archie_captured() {
    let plan = FabricPlan::free_only();

    assert_eq!(
        plan.source_for(GeoDataClass::PhotorealMesh, 18),
        Some(GeoProviderId::ArchiePhotogrammetry)
    );
    assert_eq!(
        plan.source_for(GeoDataClass::Radiance, 20),
        Some(GeoProviderId::ArchieGaussianSplat)
    );
}

/// A source is never chosen outside the scale it is good at.
///
/// Thirty-metre elevation posts have nothing to say about a doorway, and drawing them there
/// would be a confident, smooth, wrong surface.
#[test]
fn a_source_is_not_used_beyond_the_scale_it_is_good_at() {
    let plan = FabricPlan::free_only();

    assert!(!provider(GeoProviderId::CopernicusDem30).serves(GeoDataClass::Terrain, 20));
    assert_ne!(
        plan.source_for(GeoDataClass::Terrain, 20),
        Some(GeoProviderId::CopernicusDem30)
    );
    assert!(!provider(GeoProviderId::ArchiePhotogrammetry).serves(GeoDataClass::PhotorealMesh, 4));
    assert_eq!(plan.source_for(GeoDataClass::PhotorealMesh, 4), None);
}

/// What Archie owns is preferred over anything streamed, even when the stream is free.
///
/// A source that can be stored, re-examined next year and reasoned about is worth more to
/// this system than one that only looks better, once, online.
#[test]
fn what_archie_owns_outranks_what_it_only_borrows() {
    let plan = FabricPlan::free_only()
        .enabling_billed_source(GeoProviderId::GooglePhotorealistic)
        .enabling_billed_source(GeoProviderId::VantorVivid);

    // Google covers 14-20 and Archie's photogrammetry 16-24. Where both could serve, the
    // owned one wins.
    assert_eq!(
        plan.source_for(GeoDataClass::PhotorealMesh, 18),
        Some(GeoProviderId::ArchiePhotogrammetry)
    );
    // Where nothing owned or open exists, the paid one is used rather than nothing.
    assert_eq!(
        plan.source_for(GeoDataClass::PhotorealMesh, 14),
        Some(GeoProviderId::GooglePhotorealistic)
    );

    // And the whole ordering is visible, not just the winner.
    assert_eq!(
        plan.sources_for(GeoDataClass::PhotorealMesh, 18),
        vec![
            GeoProviderId::ArchiePhotogrammetry,
            GeoProviderId::GooglePhotorealistic
        ]
    );
}

/// Open data outranks a bill for the same job.
#[test]
fn open_data_outranks_a_paid_source_that_does_the_same_job() {
    let plan = FabricPlan::free_only().enabling_billed_source(GeoProviderId::VantorVivid);
    assert_eq!(
        plan.source_for(GeoDataClass::Terrain, 10),
        Some(GeoProviderId::CopernicusDem30)
    );
}

/// A provider that is not enabled is never chosen, whatever it could have offered.
#[test]
fn a_provider_nobody_enabled_is_never_selected() {
    let plan = FabricPlan::default();
    for class in [
        GeoDataClass::Terrain,
        GeoDataClass::SemanticBuildings,
        GeoDataClass::PhotorealMesh,
        GeoDataClass::Radiance,
        GeoDataClass::Imagery,
        GeoDataClass::Engineering,
    ] {
        for lod in 0..=24 {
            assert_eq!(plan.source_for(class, lod), None, "{class:?} at {lod}");
        }
    }
}

// ------------------------------------------------------------- honesty of the catalogue

/// A licence that forbids storage cannot claim to be self-hostable, and a source Archie
/// may not analyse is not one it can keep.
#[test]
fn no_entry_claims_permissions_that_contradict_each_other() {
    for capabilities in PROVIDERS {
        if !capabilities.offline_allowed {
            assert!(
                !capabilities.self_hostable,
                "{:?} may not be stored yet claims to be self-hostable",
                capabilities.id
            );
        }
        if capabilities.is_archies_to_keep() {
            assert!(capabilities.offline_allowed && capabilities.machine_analysis_allowed);
        }
        assert!(
            capabilities.useful_lod.0 <= capabilities.useful_lod.1,
            "{:?} has an empty scale range",
            capabilities.id
        );
        assert!(
            !capabilities.data_classes.is_empty(),
            "{:?} supplies nothing",
            capabilities.id
        );
    }
}

/// A source Archie cannot keep is a source Archie is renting, and the ones being rented
/// are exactly the ones with somebody else's terms on them.
#[test]
fn only_owned_and_open_sources_are_archies_to_keep() {
    for capabilities in PROVIDERS {
        if matches!(capabilities.cost, Cost::Owned | Cost::FreeAndOpen) {
            assert!(
                capabilities.is_archies_to_keep(),
                "{:?} is free and open and should be Archie's to keep",
                capabilities.id
            );
        }
    }
    assert!(!provider(GeoProviderId::GooglePhotorealistic).is_archies_to_keep());
    assert!(!provider(GeoProviderId::CesiumOsmBuildings).is_archies_to_keep());
}

/// Everything names its licence, its attribution and the day somebody read them.
#[test]
fn every_entry_records_its_terms_and_when_they_were_read() {
    for capabilities in PROVIDERS {
        assert!(
            !capabilities.license.trim().is_empty(),
            "{:?}",
            capabilities.id
        );
        assert!(
            !capabilities.attribution.trim().is_empty(),
            "{:?}",
            capabilities.id
        );
        let date = capabilities.verified_on;
        assert_eq!(date.len(), 10, "{:?}: {date}", capabilities.id);
        assert!(
            date.chars()
                .enumerate()
                .all(|(index, character)| match index {
                    4 | 7 => character == '-',
                    _ => character.is_ascii_digit(),
                }),
            "{:?}: {date} is not an ISO date",
            capabilities.id
        );
    }
}

/// The Google entry records the actual prohibition, not a summary of it.
///
/// A future reader deciding whether to wire it into an analysis path should meet the terms
/// themselves rather than somebody's paraphrase.
#[test]
fn the_google_entry_quotes_the_clause_that_decides_the_architecture() {
    let google = provider(GeoProviderId::GooglePhotorealistic);
    for phrase in [
        "image analysis",
        "machine interpretation",
        "object detection",
        "geodata extraction",
        "offline",
    ] {
        assert!(
            google.license.contains(phrase),
            "the licence text is missing {phrase:?}"
        );
    }
    assert!(!google.machine_analysis_allowed);
    assert!(!google.offline_allowed);
}

/// A regional source names its region in words.
#[test]
fn coverage_that_is_not_global_says_where_it_is() {
    for capabilities in PROVIDERS {
        if let CoverageClass::Regional(area) = capabilities.coverage {
            assert!(!area.trim().is_empty(), "{:?}", capabilities.id);
        }
    }
    assert_eq!(
        provider(GeoProviderId::ArchiePhotogrammetry).coverage,
        CoverageClass::WhereArchieHasCaptured
    );
}

/// Every id resolves, so the lookup cannot panic.
#[test]
fn every_provider_id_has_an_entry() {
    for id in EVERY_ID {
        assert_eq!(provider(id).id, id);
    }
    assert_eq!(PROVIDERS.len(), EVERY_ID.len());

    let mut seen: Vec<GeoProviderId> = PROVIDERS.iter().map(|entry| entry.id).collect();
    seen.sort();
    let count = seen.len();
    seen.dedup();
    assert_eq!(seen.len(), count, "an id is used twice");
}

/// Which of two equally free sources is tried first is a stated decision, not an accident
/// of where each sits in an enum.
///
/// Both cover the planet's buildings for nothing. OSM is first because it carries names,
/// addresses and uses, which is what makes a building an entity rather than a shape;
/// Microsoft is second because it has reach where OSM is thin and heights on 174 million
/// of them. Moving either declaration must not change that.
#[test]
fn a_tie_between_free_sources_is_broken_by_a_stated_preference() {
    let plan = FabricPlan::free_only();

    assert_eq!(
        plan.sources_for(GeoDataClass::SemanticBuildings, 14),
        vec![
            GeoProviderId::OpenStreetMap,
            GeoProviderId::MicrosoftBuildingFootprints
        ]
    );

    // And the single answer agrees with the ordered list, because there is one rule.
    assert_eq!(
        plan.source_for(GeoDataClass::SemanticBuildings, 14),
        Some(GeoProviderId::OpenStreetMap)
    );

    let osm = provider(GeoProviderId::OpenStreetMap);
    let microsoft = provider(GeoProviderId::MicrosoftBuildingFootprints);
    assert_eq!(osm.cost, microsoft.cost, "the tie is real");
    assert!(
        osm.preference < microsoft.preference,
        "the order comes from the preference"
    );
    assert!(
        GeoProviderId::MicrosoftBuildingFootprints < GeoProviderId::OpenStreetMap,
        "and not from the declaration order, which runs the other way"
    );
}

/// Declaration order is arranged to disagree with the order sources are chosen in.
///
/// Not decoration. If cost were ever dropped out of the sort, the mistake would otherwise
/// be invisible — every query would still return a plausible provider. Arranged this way,
/// dropping it makes the billed source win, which a test can see.
#[test]
fn the_paid_sources_sort_first_by_declaration_and_last_by_choice() {
    assert!(GeoProviderId::GooglePhotorealistic < GeoProviderId::ArchiePhotogrammetry);
    assert!(GeoProviderId::VantorVivid < GeoProviderId::CopernicusDem30);

    let plan = FabricPlan::free_only()
        .enabling_billed_source(GeoProviderId::GooglePhotorealistic)
        .enabling_billed_source(GeoProviderId::VantorVivid);

    assert_eq!(
        plan.source_for(GeoDataClass::PhotorealMesh, 18),
        Some(GeoProviderId::ArchiePhotogrammetry),
        "the owned source wins even though the billed one is declared first"
    );
    assert_eq!(
        plan.source_for(GeoDataClass::Terrain, 10),
        Some(GeoProviderId::CopernicusDem30),
        "open elevation wins even though the billed one is declared first"
    );
}

/// OpenStreetMap is not an elevation model.
///
/// It carries the occasional `ele` tag and no surface at all. A fabric that asked it for
/// terrain would get nothing where it expected the shape of the ground — and because it
/// covers the planet and costs nothing, it is exactly the entry that would win that
/// selection by mistake.
#[test]
fn openstreetmap_is_not_offered_as_a_source_of_ground() {
    assert!(!provider(GeoProviderId::OpenStreetMap).serves(GeoDataClass::Terrain, 10));
    assert_eq!(
        FabricPlan::free_only().source_for(GeoDataClass::Terrain, 10),
        Some(GeoProviderId::CopernicusDem30)
    );
}
