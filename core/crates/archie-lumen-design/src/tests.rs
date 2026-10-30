use super::*;
use policy::*;
use simulation::*;
use std::ffi::CStr;

fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 2e-5, "{a} != {b}");
}
fn v(x: f32, y: f32, z: f32) -> DesignVec3 {
    DesignVec3 { x, y, z }
}
fn body(x: f32, target: f32) -> DesignBody {
    DesignBody {
        position: v(x, 0., 0.),
        velocity: v(0., 0., 0.),
        anchor: v(target, 0., 0.),
        fixed: 0,
    }
}

#[test]
fn exported_catalog_has_real_flattened_source_profiles() {
    let text = unsafe { CStr::from_ptr(design_presets_json()) }
        .to_str()
        .unwrap();
    assert!(text.starts_with('{'));
    for id in [
        "polar-prisma",
        "polar-obsidiana",
        "polar-titanio",
        "polar-holograma",
        "polar-cristal",
        "polar-circuitos",
        "polar-fotonico",
        "polar-cartografia",
    ] {
        assert!(text.contains(id));
    }
    assert!(text.contains("visualNodeStyle"));
    assert_eq!(NodeStyle::from_id(9), None);
    assert_eq!(NodeStyle::parse("../../shader"), None);
    assert_eq!(NodeStyle::parse("prism_cut"), Some(NodeStyle::PrismCut));
}

#[test]
fn poles_are_spatial_and_finite_at_center() {
    let w = v(-170., 100., 180.);
    let c = v(165., -105., -85.);
    near(pole_coordinate(w, w, c, 200.), 1.);
    near(pole_coordinate(c, w, c, 200.), 0.);
    near(pole_coordinate(v(-2.5, -2.5, 47.5), w, c, 200.), 0.5);
    near(pole_coordinate(w, w, w, 200.), 0.5);
    assert_eq!(bloom_weight(BloomMode::Selection, 0.8, false), 0.);
    assert_eq!(bloom_weight(BloomMode::Polar, 1., false), 1.);
    assert_eq!(bloom_weight(BloomMode::Off, 1., true), 0.);
}

#[test]
fn texture_tiers_have_source_dead_band_and_capabilities() {
    let p = TextureRequest {
        max_resolution: 4096,
        max_capability: 8192,
        quality: Quality::Original,
        projected_size: 3.,
        current: 512,
    };
    assert_eq!(material_texture_size(p), 512);
    assert_eq!(
        material_texture_size(TextureRequest {
            projected_size: 40.,
            ..p
        }),
        1024
    );
    assert_eq!(
        material_texture_size(TextureRequest {
            projected_size: 220.,
            ..p
        }),
        4096
    );
    assert_eq!(
        material_texture_size(TextureRequest {
            projected_size: 18.,
            current: 2048,
            ..p
        }),
        2048
    );
    assert_eq!(
        material_texture_size(TextureRequest {
            projected_size: 220.,
            max_capability: 1024,
            ..p
        }),
        1024
    );
    assert_eq!(
        material_texture_size(TextureRequest {
            projected_size: 220.,
            quality: Quality::Performance,
            ..p
        }),
        1024
    );
    assert_eq!(
        material_texture_size(TextureRequest {
            max_resolution: 0,
            ..p
        }),
        0
    );
    assert_eq!(texture_bytes(4096, 4096, 4, true), Some(89_478_484));
    assert_eq!(texture_bytes(4096, 4096, 4, false), Some(67_108_864));
}

#[test]
fn quality_uses_sustained_samples_and_preserves_content_limit() {
    let mut q = AdaptiveQuality::default();
    for _ in 0..3 {
        assert_eq!(q.sample(20.), None);
    }
    assert_eq!(q.sample(20.), Some(Quality::Performance));
    for _ in 0..7 {
        assert_eq!(q.sample(60.), None);
    }
    assert_eq!(q.sample(60.), Some(Quality::Balanced));
    for _ in 0..4 {
        q.sample(20.);
    }
    q.sample(40.);
    assert_eq!(q.sample(f32::NAN), None);
    let base = Decoration {
        ambient_count: 420,
        shell_density: 1.,
        bloom: true,
        bloom_strength: 0.5,
        pixel_ratio: 1.25,
        texture_resolution: 4096,
        visible_limit: 36000,
    };
    let fast = effective_quality(base, Quality::Performance);
    assert_eq!(fast.visible_limit, 36000);
    assert!(!fast.bloom);
    assert_eq!(fast.ambient_count, 80);
    near(fast.pixel_ratio, 0.8);
    assert_eq!(fast.texture_resolution, 1024);
}

#[test]
fn source_hybrid_parameters_and_near_lod_are_preserved() {
    let p = hybrid_parameters(NodeStyle::PrismCut, 0.035, 0.18, 1.3);
    near(p[0], 0.0567);
    near(p[1], 0.022);
    near(p[2], 0.1296);
    near(p[3], 1.3);
    assert_eq!(hybrid_lod(0.), [0., 0.]);
    assert_eq!(hybrid_lod(112.), [1., 1.]);
    assert_eq!(hybrid_state(true, false, false), 1.);
    near(hybrid_state(false, true, false), 0.78);
    near(hybrid_state(false, false, true), 0.55);
}

#[test]
fn source_surface_bake_is_linear_repeatable_and_has_packed_normals() {
    let mut signatures = std::collections::BTreeSet::new();
    for id in 1..=8 {
        let style = NodeStyle::from_id(id).unwrap();
        let a = surface::bake(style, 32, 1.).unwrap();
        let b = surface::bake(style, 32, 1.).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.albedo.len(), 4096);
        assert!(a.surface.chunks_exact(4).all(|p| p[2] >= 128));
        signatures.insert(a.albedo);
    }
    assert_eq!(signatures.len(), 8);
    let ceramic = surface::bake(NodeStyle::MatteCeramic, 32, 1.).unwrap();
    // At UV0 the authored matte height is 0.5: albedo=(0.94+0.03*0.5)^2.2.
    assert_eq!(&ceramic.albedo[..4], &[230, 230, 230, 255]);
    assert_eq!(ceramic.surface[3], 242);
    near(design_height(4, 0., 0., 2.), 0.5);
    assert!(surface::bake(NodeStyle::PrismCut, 0, 1.).is_none());
}

#[test]
fn ffi_surface_rejects_short_null_and_overlapping_buffers() {
    let mut a = vec![0; 16];
    let mut s = vec![0; 16];
    unsafe {
        assert!(!design_surface(
            1,
            2,
            1.,
            std::ptr::null_mut(),
            s.as_mut_ptr(),
            16
        ));
        assert!(!design_surface(
            1,
            2,
            1.,
            a.as_mut_ptr(),
            s.as_mut_ptr(),
            15
        ));
        assert!(!design_surface(
            1,
            2,
            1.,
            a.as_mut_ptr(),
            a.as_mut_ptr(),
            16
        ));
        assert!(!design_surface(
            99,
            2,
            1.,
            a.as_mut_ptr(),
            s.as_mut_ptr(),
            16
        ));
        assert!(design_surface(1, 2, 1., a.as_mut_ptr(), s.as_mut_ptr(), 16));
    }
    assert!(a.iter().any(|&b| b != 0));
}

#[test]
fn simulation_reports_small_position_changes_and_exact_final_rest() {
    let mut b = [body(0., 1.)];
    let m = DesignMotion {
        spring: 0.0001,
        ..DesignMotion::default()
    };
    assert_eq!(
        integrate(&mut b, None, 1. / 60., m) & POSITIONS_CHANGED,
        POSITIONS_CHANGED
    );
    assert!(b[0].position.x > 0.);
    let mut b = [body(-15., 10.)];
    let m = DesignMotion::default();
    for _ in 0..3000 {
        integrate(&mut b, None, 1. / 60., m);
    }
    assert_eq!(b[0].position, v(10., 0., 0.));
    assert_eq!(b[0].velocity, v(0., 0., 0.));
    assert_eq!(integrate(&mut b, None, 1. / 60., m), 0);
}

#[test]
fn simulation_cannot_sleep_at_a_turning_point_or_zero_delta() {
    let mut b = [body(1., 2.)];
    b[0].velocity.x = -23. / 60.;
    let result = integrate(&mut b, None, 1. / 60., DesignMotion::default());
    assert_eq!(result & SIMULATION_ACTIVE, SIMULATION_ACTIVE);
    let mut b = [body(0., 10.)];
    let before = b;
    assert_eq!(
        integrate(&mut b, None, 0., DesignMotion::default()),
        SIMULATION_ACTIVE
    );
    assert_eq!(before, b);
}

#[test]
fn free_motion_preserves_weak_damping_and_fixed_root() {
    let mut b = [body(2., 10.)];
    b[0].velocity.x = 0.00001;
    let m = DesignMotion {
        spring: 0.,
        damping: 1.,
        ..DesignMotion::default()
    };
    assert_eq!(
        integrate(&mut b, None, 1. / 60., m) & SIMULATION_ACTIVE,
        SIMULATION_ACTIVE
    );
    near(b[0].velocity.x, 0.00001);
    b[0].fixed = 1;
    integrate(&mut b, Some(v(10., 0., 0.)), 1. / 60., m);
    assert_eq!(b[0].position, b[0].anchor);
    assert_eq!(b[0].velocity, v(0., 0., 0.));
}

#[test]
fn simulation_sleeps_and_wakes_on_input_or_revision() {
    let mut b = [body(10., 10.)];
    let mut sim = Simulation::default();
    let m = DesignMotion::default();
    assert_eq!(sim.step(&mut b, None, 1. / 60., m, 0), 0);
    assert!(sim.sleeping());
    assert_eq!(
        sim.step(&mut b, Some(v(10., 0., 0.)), 1. / 60., m, 0) & SIMULATION_ACTIVE,
        SIMULATION_ACTIVE
    );
    assert!(!sim.sleeping());
    b[0] = body(10., 10.);
    sim.step(&mut b, None, 1. / 60., m, 1);
    assert!(sim.sleeping());
    b[0].anchor.x = 12.;
    assert_ne!(sim.step(&mut b, None, 1. / 60., m, 2), 0);
}

#[test]
fn ffi_step_rejects_invalid_input_without_partial_mutation() {
    let mut b = [body(0., 10.)];
    let before = b;
    let m = DesignMotion::default();
    unsafe {
        assert_eq!(
            design_step(b.as_mut_ptr(), 1, std::ptr::null(), f32::NAN, &m),
            0
        );
        assert_eq!(
            design_step(std::ptr::null_mut(), 1, std::ptr::null(), 0.02, &m),
            0
        );
        assert_ne!(
            design_step(b.as_mut_ptr(), 1, std::ptr::null(), 0.02, &m),
            0
        );
    }
    assert_ne!(before, b);
}

#[test]
fn continuity_is_root_scoped_and_bounded() {
    let mut memory = SpatialMemory::new(2);
    let mut old = body(20., 10.);
    old.velocity.x = 0.3;
    memory.remember(
        "root-a",
        [("one", old), ("two", body(3., 4.)), ("three", body(5., 6.))],
    );
    assert!(memory.restore("root-a", "one", 10.).is_none());
    let got = memory.restore("root-a", "three", 2.).unwrap();
    near(got.position.x, 2.1);
    assert!(memory.restore("root-b", "three", 2.).is_none());
}

#[test]
fn delivered_geometry_forms_have_uvs_normals_and_dimensions() {
    for id in 1..=7 {
        let m = mesh::geometry(NodeStyle::from_id(id).unwrap(), false).unwrap();
        assert_eq!(m.vertices.len() % 8, 0);
        assert_eq!(m.indices.len() % 3, 0);
        assert!(m
            .indices
            .iter()
            .all(|&i| (i as usize) < m.vertices.len() / 8));
        for vertex in m.vertices.chunks_exact(8) {
            assert!(vertex.iter().all(|x| x.is_finite()));
            near(v(vertex[3], vertex[4], vertex[5]).length(), 1.);
        }
    }
    assert!(mesh::geometry(NodeStyle::Original, false).is_none());
    assert!(mesh::geometry(NodeStyle::MatteCeramic, false).is_none());
    let crystal = mesh::geometry(NodeStyle::FracturedCrystal, false).unwrap();
    near(
        crystal
            .vertices
            .chunks_exact(8)
            .map(|p| p[1])
            .fold(f32::NEG_INFINITY, f32::max),
        0.95,
    );
    let box_file = mesh::geometry(NodeStyle::Microcircuit, false).unwrap();
    let box_dir = mesh::geometry(NodeStyle::Microcircuit, true).unwrap();
    near(
        box_file
            .vertices
            .chunks_exact(8)
            .map(|p| p[0])
            .fold(f32::NEG_INFINITY, f32::max),
        0.575,
    );
    near(
        box_dir
            .vertices
            .chunks_exact(8)
            .map(|p| p[0])
            .fold(f32::NEG_INFINITY, f32::max),
        0.675,
    );
}

#[test]
fn typed_setting_validation_and_invalidation_do_not_rebuild_anchors_for_color() {
    let s = VisualSettings {
        pole_color_mix: 42.,
        line_width_px: -1.,
        detail_frequency: f32::NAN,
        ..VisualSettings::default()
    }
    .validated();
    assert_eq!(s.pole_color_mix, 1.);
    assert_eq!(s.line_width_px, 0.5);
    assert_eq!(s.detail_frequency, 1.);
    assert_eq!(change_kind(["bloomStrength"]), ChangeKind::Uniform);
    assert_eq!(change_kind(["visualNodeStyle"]), ChangeKind::Material);
    assert_eq!(change_kind(["visualLinkStyle"]), ChangeKind::Links);
    assert_eq!(
        change_kind(["shellDensity", "visualLinkStyle"]),
        ChangeKind::Structure
    );
    assert_eq!(original_texture_resolution(4096, 2048, 4.), 1024);
    assert_eq!(visual_texture_resolution(4096, 512, Some(1.)), 1024);
    assert_eq!(bloom_mask_scale(Quality::Balanced), 0.375);
}

#[test]
fn extreme_finite_ffi_inputs_are_rejected_without_nan_or_mutation() {
    let mut bodies = [body(f32::MAX, 10.)];
    let before = bodies;
    unsafe {
        assert_eq!(
            design_step(
                bodies.as_mut_ptr(),
                1,
                std::ptr::null(),
                0.02,
                &DesignMotion::default()
            ),
            0
        );
    }
    assert_eq!(before, bodies);
}

#[test]
fn continuity_recovers_each_velocity_component_independently() {
    let mut old = body(2., 99.);
    old.velocity = v(0.3, f32::NAN, 0.7);
    let mut memory = SpatialMemory::new(4);
    memory.remember("root", [("id", old)]);
    let restored = memory.restore("root", "id", 20.).unwrap();
    assert_eq!(restored.velocity, v(0.3, 0., 0.7));
}

#[test]
fn mesh_ffi_query_fill_and_short_buffers_are_consistent() {
    let (mut nv, mut ni) = (0, 0);
    unsafe {
        assert!(design_mesh(
            1,
            false,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            0,
            &mut nv,
            &mut ni
        ));
        assert_eq!(nv, 42);
        assert_eq!(ni, 180);
        let mut vertices = vec![0.; nv * 8];
        let mut indices = vec![0; ni];
        assert!(!design_mesh(
            1,
            false,
            vertices.as_mut_ptr(),
            nv * 8 - 1,
            indices.as_mut_ptr(),
            ni,
            &mut nv,
            &mut ni
        ));
        assert!(design_mesh(
            1,
            false,
            vertices.as_mut_ptr(),
            nv * 8,
            indices.as_mut_ptr(),
            ni,
            &mut nv,
            &mut ni
        ));
        assert!(vertices.iter().all(|x| x.is_finite()));
        assert!(!design_mesh(
            0,
            false,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            0,
            &mut nv,
            &mut ni
        ));
    }
}

#[test]
fn ffi_material_policy_respects_capabilities_and_rejects_invalid_input() {
    assert_eq!(design_texture_size(4096, 1024, 0, 220., 512), 1024);
    assert_eq!(design_texture_size(4096, 8192, 1, 220., 512), 2048);
    assert_eq!(design_texture_size(4096, 8192, 2, 220., 512), 1024);
    assert_eq!(design_texture_size(4096, 8192, 99, 220., 512), 0);
    let mut out = [0.; 4];
    unsafe {
        assert!(design_hybrid(1, 0.035, 0.18, 1.3, out.as_mut_ptr(), 4));
        near(out[0], 0.0567);
        assert!(!design_hybrid(1, f32::NAN, 0.18, 1.3, out.as_mut_ptr(), 4));
    }
}

#[test]
fn springless_free_motion_and_constrained_targets_reach_source_rest() {
    let mut bodies = [body(2., 10.)];
    bodies[0].velocity.x = 1.;
    let motion = DesignMotion {
        spring: 0.,
        ..DesignMotion::default()
    };
    let mut flags = 3;
    for _ in 0..4000 {
        flags = integrate(&mut bodies, None, 1. / 60., motion);
        if flags == 0 {
            break;
        }
    }
    assert_eq!(flags, 0);
    assert!(bodies[0].position.x > 2. && bodies[0].position.x < 3.);
    assert_eq!(bodies[0].velocity, DesignVec3::default());
    let mut bodies = [body(0., 20.)];
    let motion = DesignMotion {
        radius: 5.,
        ..DesignMotion::default()
    };
    for _ in 0..3000 {
        integrate(&mut bodies, None, 1. / 60., motion);
    }
    assert_eq!(bodies[0].position, v(5., 0., 0.));
    assert_eq!(bodies[0].velocity, DesignVec3::default());
    let mut bodies = [body(2., 10.)];
    let motion = DesignMotion {
        damping: 0.,
        ..DesignMotion::default()
    };
    assert_eq!(integrate(&mut bodies, None, 1. / 60., motion), 0);
    assert_eq!(bodies[0].position.x, 2.);
}

mod content_sphere {
    use crate::content::{classify, scan, scene_json, ContentOptions};
    use std::fs;
    use std::path::PathBuf;

    fn tree(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("archie-content-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for dir in [
            "core/src",
            "display/native",
            "docs",
            ".git/objects",
            "target/release",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for (file, bytes) in [
            ("core/src/lib.rs", 120),
            ("core/Cargo.toml", 40),
            ("display/native/main.cpp", 300),
            ("docs/a.md", 10),
            ("docs/b.md", 10),
            ("docs/c.png", 5000),
            ("README.md", 64),
            (".git/objects/x", 9),
            ("target/release/big.so", 999),
        ] {
            fs::write(root.join(file), vec![b'x'; bytes]).unwrap();
        }
        root
    }

    #[test]
    fn classify_follows_the_design_table() {
        assert_eq!(classify("lib.rs"), 0);
        assert_eq!(classify("README.MD"), 1);
        assert_eq!(classify("c.png"), 2);
        assert_eq!(classify("Cargo.toml"), 3);
        assert_eq!(classify("x.tar"), 4);
        assert_eq!(classify("run.ps1"), 5);
        assert_eq!(classify(".gitignore"), 6);
        assert_eq!(classify("Makefile"), 6);
    }

    #[test]
    fn scan_skips_hidden_and_build_output_and_is_deterministic() {
        let root = tree("skip");
        let field = scan(&root, ContentOptions::default()).unwrap();
        let ids: Vec<&str> = field.nodes.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains(&"core/src/lib.rs") && ids.contains(&"docs/c.png"));
        assert!(!ids
            .iter()
            .any(|id| id.starts_with(".git") || id.starts_with("target")));
        assert_eq!(field.nodes[0].id, ".");
        assert_eq!(field.nodes[0].position, [0.0; 3]);
        assert_eq!(field.bytes, 120 + 40 + 300 + 10 + 10 + 5000 + 64);
        // Every node inside the field and in front of nothing random: same input, same scene.
        for node in &field.nodes {
            let r =
                (node.position[0].powi(2) + node.position[1].powi(2) + node.position[2].powi(2))
                    .sqrt();
            assert!(r <= field.radius * 0.9 + 1e-6, "{} at {r}", node.id);
        }
        let again = scan(&root, ContentOptions::default()).unwrap();
        assert_eq!(scene_json(&field, 7), scene_json(&again, 7));
        let docs = field.nodes.iter().find(|n| n.id == "docs").unwrap();
        assert_eq!(docs.subtree, 4);
        assert_eq!(docs.category, 1, "documents dominate docs/");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scan_respects_node_and_per_folder_bounds() {
        let root = tree("bounds");
        for k in 0..50 {
            fs::write(root.join(format!("docs/n{k:02}.txt")), b"y").unwrap();
        }
        let options = ContentOptions {
            limit: 20,
            files_per_folder: 5,
            ..ContentOptions::default()
        };
        let field = scan(&root, options).unwrap();
        assert!(field.nodes.len() <= 20);
        let docs = field.nodes.iter().position(|n| n.id == "docs").unwrap();
        assert!(
            field
                .nodes
                .iter()
                .filter(|n| n.parent == Some(docs) && !n.dir)
                .count()
                <= 5
        );
        assert!(field.truncated);
        assert!(field.elements >= 50);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ffi_writes_the_scene_or_nothing() {
        let root = tree("ffi");
        let path = root.to_string_lossy().into_owned();
        let options = crate::DesignContentOptions {
            limit: 2500,
            files_per_folder: 30,
            category_pull: 0.44,
            hierarchy_pull: 0.59,
            radial_layers: 0.63,
            size_influence: 0.09,
            scale: 1.0,
        };
        let mut out = vec![0u8; 1 << 20];
        let n = unsafe {
            crate::design_content_scene(
                path.as_ptr(),
                path.len(),
                &options,
                1,
                out.as_mut_ptr(),
                out.len(),
            )
        };
        assert!(n > 0);
        let text = std::str::from_utf8(&out[..n]).unwrap();
        assert!(text.starts_with("{\"generated_at_ms\":1,\"lens\":\"content\""));
        assert!(text.contains("\"kind\":\"root\"") && text.contains("\"content\":{\"name\":"));
        let mut small = vec![0u8; 16];
        assert_eq!(
            unsafe {
                crate::design_content_scene(
                    path.as_ptr(),
                    path.len(),
                    &options,
                    1,
                    small.as_mut_ptr(),
                    small.len(),
                )
            },
            0
        );
        let missing = "/definitely/not/here";
        assert_eq!(
            unsafe {
                crate::design_content_scene(
                    missing.as_ptr(),
                    missing.len(),
                    &options,
                    1,
                    out.as_mut_ptr(),
                    out.len(),
                )
            },
            0
        );
        let _ = fs::remove_dir_all(root);
    }
}

/// The port against Xavi's original itself: fixtures/golden.mjs ran the original
/// catalog.js and main.js settings with node and kept what they returned.
mod golden {
    use serde_json::Value;

    const PRESETS: &str = include_str!("../fixtures/presets.golden.json");

    fn json(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }
    /// Differences between two settings maps, numbers compared as numbers (1 is 1.0).
    fn differences(label: &str, port: &Value, original: &Value) -> Vec<String> {
        let (Some(port), Some(original)) = (port.as_object(), original.as_object()) else {
            return vec![format!("{label}: not an object")];
        };
        let mut out = Vec::new();
        for key in port
            .keys()
            .chain(original.keys().filter(|k| !port.contains_key(*k)))
        {
            let (a, b) = (&port.get(key), &original.get(key));
            let same = match (a, b) {
                (Some(Value::Number(x)), Some(Value::Number(y))) => x.as_f64() == y.as_f64(),
                _ => a == b,
            };
            if !same {
                out.push(format!("{label}.{key}: port {a:?}, original {b:?}"));
            }
        }
        out
    }
    fn merged(base: &Value, overrides: &Value) -> Value {
        let mut out = base.as_object().unwrap().clone();
        for (k, v) in overrides.as_object().unwrap() {
            out.insert(k.clone(), v.clone());
        }
        Value::Object(out)
    }

    #[test]
    fn catalog_holds_every_original_profile_value() {
        let (original, catalog) = (json(PRESETS), json(crate::CATALOG_JSON));
        let mut wrong = differences("defaults", &catalog["defaults"], &original["defaults"]);
        // field, crown, atlas and dense: the composition profiles of main.js, every field.
        let profiles = catalog["profiles"].as_array().expect("catalog profiles");
        let expected = original["original"].as_array().unwrap();
        assert_eq!(profiles.len(), expected.len());
        for (port, source) in profiles.iter().zip(expected) {
            assert_eq!(port["id"], source["id"]);
            assert_eq!(port["name"], source["name"]);
            let id = source["id"].as_str().unwrap();
            wrong.extend(differences(id, &port["overrides"], &source["overrides"]));
            // Applied, a profile is the defaults with its overrides on top.
            wrong.extend(differences(
                id,
                &merged(&catalog["defaults"], &port["overrides"]),
                &merged(&original["defaults"], &source["overrides"]),
            ));
        }
        wrong.extend(differences(
            "legacy",
            &merged(&catalog["defaults"], &catalog["legacy"]),
            &original["legacy"],
        ));
        let presets = catalog["presets"].as_array().unwrap();
        for source in original["presets"].as_array().unwrap() {
            let port = presets
                .iter()
                .find(|p| p["id"] == source["id"])
                .unwrap_or_else(|| panic!("preset {} missing", source["id"]));
            assert_eq!(port["name"], source["name"]);
            let id = source["id"].as_str().unwrap();
            wrong.extend(differences(id, &port["settings"], &source["settings"]));
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}

/// The neural sphere against Lumenfield's own module: fixtures/golden-neural.mjs ran
/// web/src/neural-sphere (API 2) with node on its sample index cut to depth 5.
mod neural_golden {
    use crate::neural::collate::locale_compare;
    use crate::neural::links::{
        nearest_pairs, pick_budget, structural_links, LinkPoint, KIND_WEIGHT,
    };
    use crate::neural::model::{category_of, hash, Processes, SphereModel, SphereNode, CATEGORIES};
    use crate::neural::motion::{
        birth_schedule, born_progress, smooth_factor, step_capped, MAX_SPEED, SETTLE_DISTANCE,
    };
    use crate::neural::settings::Settings;
    use crate::neural::sphere::{
        format_count, format_size, linear_colour, Rng, Sphere, CATEGORY_HEX, CREAM_HEX, FOLDER_HEX,
        HOT_HEX,
    };
    use serde_json::Value;
    use std::cmp::Ordering;

    const GOLDEN: &str = include_str!("../fixtures/neural.golden.json");
    const INDEX: &str = include_str!("../fixtures/neural-index.json");

    fn golden() -> Value {
        serde_json::from_str(GOLDEN).unwrap()
    }
    fn index() -> Vec<SphereNode> {
        let list: Value = serde_json::from_str(INDEX).unwrap();
        list.as_array()
            .unwrap()
            .iter()
            .map(|n| SphereNode {
                id: n["id"].as_str().unwrap().into(),
                p: n["p"].as_str().map(Into::into),
                k: n["k"].as_u64().unwrap() == 1,
                d: n["d"].as_u64().unwrap() as u32,
                s: n["s"].as_f64().unwrap(),
                m: n["m"].as_f64().unwrap(),
                n: n["n"].as_str().unwrap().into(),
            })
            .collect()
    }
    fn f(v: &Value) -> f64 {
        v.as_f64().unwrap()
    }
    fn close(a: f64, b: f64, label: &str) {
        assert!(
            (a - b).abs() <= 1e-9 * (1.0 + b.abs()),
            "{label}: port {a}, original {b}"
        );
    }
    /// Equal but for the last bits: serde_json (without float_roundtrip) may read the golden's
    /// shortest decimal one ulp off.
    fn same(a: f64, b: f64, label: &str) {
        assert!(
            (a - b).abs() <= 2.0 * f64::EPSILON * a.abs().max(b.abs()),
            "{label}: port {a}, original {b}"
        );
    }
    fn model() -> SphereModel {
        SphereModel::new(&index(), true, Processes::Example, &[]).unwrap()
    }

    #[test]
    fn collation_is_icu_root_order_as_node_sorts() {
        let g = golden();
        let list: Vec<&str> = g["collation"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let wrong: Vec<String> = list
            .windows(2)
            .filter(|w| locale_compare(w[0], w[1]) == Ordering::Greater)
            .map(|w| format!("{:?} > {:?}", w[0], w[1]))
            .collect();
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn hash_and_categories_match() {
        let g = golden();
        for row in g["hash"].as_array().unwrap() {
            same(
                hash(row[0].as_str().unwrap()),
                f(&row[1]),
                row[0].as_str().unwrap(),
            );
        }
        let by_id: std::collections::HashMap<String, SphereNode> =
            index().into_iter().map(|n| (n.id.clone(), n)).collect();
        for row in g["categories"].as_array().unwrap() {
            let n = &by_id[row[0].as_str().unwrap()];
            let cat = category_of(n.k, &n.n);
            let name = if cat == 7 {
                "folders"
            } else {
                CATEGORIES[cat].0
            };
            assert_eq!(name, row[1].as_str().unwrap(), "{}", n.id);
        }
    }

    #[test]
    fn model_shows_folds_and_places_everything_as_the_original() {
        let g = golden();
        let m = model();
        let o = &g["model"];
        let vis: Vec<&str> = m.vis.iter().map(|&i| m.id(i)).collect();
        let expected: Vec<&str> = o["vis"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(vis, expected);
        for row in o["folded"].as_array().unwrap() {
            let n = &m.nodes[m.by_id[row[0].as_str().unwrap()]];
            assert!(n.folded, "{}", n.node.id);
            assert_eq!(
                u64::from(n.hidden),
                row[1].as_u64().unwrap(),
                "{}",
                n.node.id
            );
        }
        for row in o["vw"].as_array().unwrap() {
            assert_eq!(
                u64::from(m.nodes[m.by_id[row[0].as_str().unwrap()]].vw),
                row[1].as_u64().unwrap()
            );
        }
        assert_eq!(u64::from(m.max_vd), o["maxVD"].as_u64().unwrap());
        let targets = o["target"].as_array().unwrap();
        assert_eq!(m.target.len(), targets.len());
        let order: Vec<&str> = targets.iter().map(|r| r[0].as_str().unwrap()).collect();
        assert_eq!(
            m.target_order
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            order
        );
        for row in targets {
            let id = row[0].as_str().unwrap();
            let at = m.target[id];
            for a in 0..3 {
                close(at[a], f(&row[a + 1]), &format!("target {id}[{a}]"));
            }
        }
        for row in o["dir"].as_array().unwrap() {
            let id = row[0].as_str().unwrap();
            let dir = m.nodes[m.by_id[id]].dir.unwrap();
            for a in 0..3 {
                close(dir[a], f(&row[a + 1]), &format!("dir {id}[{a}]"));
            }
        }
        let processes = o["processes"].as_array().unwrap();
        assert_eq!(m.processes.len(), processes.len());
        for (p, row) in m.processes.iter().zip(processes) {
            assert_eq!(p.id, row[0].as_str().unwrap());
            assert_eq!(m.id(p.host), row[1].as_str().unwrap());
            assert_eq!(p.name, row[2].as_str().unwrap());
        }
        let c = m.counts();
        assert_eq!(
            (c.0 as u64, u64::from(c.1), c.2 as u64, c.3 as u64),
            (
                o["counts"]["visible"].as_u64().unwrap(),
                o["counts"]["folded"].as_u64().unwrap(),
                o["counts"]["foldedPrograms"].as_u64().unwrap(),
                o["counts"]["total"].as_u64().unwrap()
            )
        );
    }

    #[test]
    fn unfolding_a_program_and_folding_off_match() {
        let g = golden();
        let mut m = model();
        m.toggle_fold(g["unfolded"]["id"].as_str().unwrap());
        assert_eq!(m.vis.len() as u64, g["unfolded"]["vis"].as_u64().unwrap());
        assert_eq!(
            m.counts().1 as u64,
            g["unfolded"]["counts"]["folded"].as_u64().unwrap()
        );
        m.set_fold(false);
        assert_eq!(m.vis.len() as u64, g["unfoldAll"]["vis"].as_u64().unwrap());
        assert_eq!(
            m.counts().1 as u64,
            g["unfoldAll"]["counts"]["folded"].as_u64().unwrap()
        );
    }

    #[test]
    fn mesh_bridge_links_and_budget_match() {
        let g = golden();
        let m = model();
        let points: Vec<LinkPoint> = m
            .vis
            .iter()
            .map(|&i| {
                let n = &m.nodes[i];
                LinkPoint {
                    id: n.node.id.clone(),
                    parent: n.node.p.clone().unwrap_or_default(),
                    dir: n.node.k,
                    p: m.target[&n.node.id],
                    category: n.cat,
                }
            })
            .collect();
        let links = structural_links(&points);
        let expected = g["links"].as_array().unwrap();
        assert_eq!(links.len(), expected.len());
        for (l, row) in links.iter().zip(expected) {
            assert_eq!(
                (l.a.as_str(), l.b.as_str(), l.kind as u64, l.weight),
                (
                    row[0].as_str().unwrap(),
                    row[1].as_str().unwrap(),
                    row[2].as_u64().unwrap(),
                    f(&row[3])
                )
            );
        }
        let half = pick_budget(&links, 0.5);
        let expected = g["budgetHalf"].as_array().unwrap();
        assert_eq!(half.len(), expected.len());
        for (l, row) in half.iter().zip(expected) {
            assert_eq!(
                (l.a.as_str(), l.b.as_str()),
                (row[0].as_str().unwrap(), row[1].as_str().unwrap())
            );
        }
        let coords: Vec<[f64; 3]> = points.iter().take(30).map(|p| p.p).collect();
        let pairs: Vec<Value> = nearest_pairs(&coords, 2)
            .into_iter()
            .map(|(a, b)| serde_json::json!([a, b]))
            .collect();
        assert_eq!(Value::Array(pairs), g["nearestPairs"]);
        for (k, w) in KIND_WEIGHT.iter().enumerate() {
            assert_eq!(*w, f(&g["kindWeight"][k.to_string()]));
        }
    }

    #[test]
    fn tree_and_bridge_curves_match() {
        let g = golden();
        let m = model();
        for row in g["curves"].as_array().unwrap() {
            let id = row[0].as_str().unwrap();
            let n = &m.nodes[m.by_id[id]];
            let parent = n.node.p.clone().unwrap();
            let (p, c) = (m.target[&parent], m.target[id]);
            let cdir = crate::neural::v3::normalize(c);
            let pdir = m.nodes[m.by_id[&parent]].dir.unwrap_or(cdir);
            let pts = Sphere::curve(p, pdir, c, cdir, parent == ".");
            let flat: Vec<f64> = pts.iter().flat_map(|v| v.iter().copied()).collect();
            let expected = row[1].as_array().unwrap();
            assert_eq!(flat.len(), expected.len());
            for (a, b) in flat.iter().zip(expected) {
                close(*a, f(b), &format!("curve {id}"));
            }
        }
        for row in g["sinks"].as_array().unwrap() {
            let (a, b) = (row[0].as_str().unwrap(), row[1].as_str().unwrap());
            let pts = Sphere::link_points(m.target[a], m.target[b], 3);
            let flat: Vec<f64> = pts.iter().flat_map(|v| v.iter().copied()).collect();
            for (x, y) in flat.iter().zip(row[2].as_array().unwrap()) {
                close(*x, f(y), &format!("sink {a}-{b}"));
            }
        }
    }

    #[test]
    fn palette_rng_build_and_motion_match() {
        let g = golden();
        let p = &g["palette"];
        let named = [
            ("code", CATEGORY_HEX[0]),
            ("documents", CATEGORY_HEX[1]),
            ("media", CATEGORY_HEX[2]),
            ("data", CATEGORY_HEX[3]),
            ("archive", CATEGORY_HEX[4]),
            ("executable", CATEGORY_HEX[5]),
            ("other", CATEGORY_HEX[6]),
            ("folders", FOLDER_HEX),
            ("cream", CREAM_HEX),
            ("hot", HOT_HEX),
            ("holo", 0x5fd4ff),
            ("background", 0x01060b),
        ];
        for (name, hex) in named {
            let c = linear_colour(hex);
            for (a, value) in c.iter().enumerate() {
                close(*value, f(&p[name][a]), name);
            }
        }
        let mut rng = Rng::new(7);
        for v in g["rng"].as_array().unwrap() {
            same(rng.float(), f(v), "rng");
        }
        let m = model();
        let list: Vec<(String, Option<String>, u32)> = m
            .vis
            .iter()
            .map(|&i| {
                (
                    m.id(i).to_string(),
                    m.nodes[i].node.p.clone(),
                    m.nodes[i].node.d,
                )
            })
            .collect();
        let birth = birth_schedule(&list, 6.0);
        for row in g["birth"].as_array().unwrap() {
            close(birth[row[0].as_str().unwrap()], f(&row[1]), "birth");
        }
        let progress: Vec<f64> = [(0.0, 1.0), (1.0, 1.0), (1.25, 1.0), (1.5, 1.0), (9.0, 1.0)]
            .iter()
            .map(|&(now, at)| born_progress(now, at))
            .collect();
        for (a, b) in progress.iter().zip(g["bornProgress"].as_array().unwrap()) {
            close(*a, f(b), "bornProgress");
        }
        for ((dt, r), b) in [
            (1.0 / 60.0, 0.18),
            (0.05, 0.18),
            (0.0, 0.18),
            (1.0 / 30.0, 1.0),
        ]
        .iter()
        .zip(g["smooth"].as_array().unwrap())
        {
            close(smooth_factor(*dt, *r), f(b), "smooth");
        }
        let mut cur = [0.0; 3];
        for row in g["steps"].as_array().unwrap() {
            let d = step_capped(
                &mut cur,
                [100.0, -40.0, 20.0],
                smooth_factor(1.0 / 60.0, 0.18),
                MAX_SPEED * 100.0 / 60.0,
                SETTLE_DISTANCE * 100.0,
            );
            close(d, f(&row[0]), "step");
            for a in 0..3 {
                close(cur[a], f(&row[a + 1]), "step position");
            }
        }
    }

    /// settings.ts `normalizeSettings` through the per-key setters the host uses.
    fn apply(settings: &mut Settings, patch: &Value) {
        let o = patch.as_object().unwrap();
        for key in [
            "brightness",
            "nodeSize",
            "glow",
            "holo",
            "rotationSpeed",
            "pulses",
            "pulseSpeed",
            "ghostOpacity",
            "linkOpacity",
            "linkBudget",
        ] {
            if let Some(v) = o.get(key).and_then(Value::as_f64) {
                settings.set_number(key, v);
            }
        }
        for key in ["ghosts", "base"] {
            if let Some(v) = o.get(key).and_then(Value::as_bool) {
                settings.set_flag(key, v);
            }
        }
        if let Some(v) = o.get("background").and_then(Value::as_str) {
            settings.set_background(v);
        }
        if let Some(links) = o.get("links").and_then(Value::as_object) {
            for (k, entry) in links {
                let Ok(kind) = k.parse::<usize>() else {
                    continue;
                };
                settings.set_link(
                    kind,
                    entry.get("on").and_then(Value::as_bool),
                    entry.get("opacity").and_then(Value::as_f64),
                );
            }
        }
        if let Some(v) = o
            .get("motion")
            .and_then(|m| m.get("response"))
            .and_then(Value::as_f64)
        {
            settings.set_number("motion.response", v);
        }
    }

    #[test]
    fn settings_take_changes_and_limits_as_the_original() {
        let g = golden();
        let check = |s: &Settings, r: &Value| {
            let pairs = [
                ("brightness", s.brightness),
                ("nodeSize", s.node_size),
                ("glow", s.glow),
                ("holo", s.holo),
                ("rotationSpeed", s.rotation_speed),
                ("pulses", s.pulses),
                ("pulseSpeed", s.pulse_speed),
                ("ghostOpacity", s.ghost_opacity),
                ("linkOpacity", s.link_opacity),
                ("linkBudget", s.link_budget),
            ];
            for (k, v) in pairs {
                assert_eq!(v, f(&r[k]), "{k}");
            }
            assert_eq!(s.ghosts, r["ghosts"].as_bool().unwrap());
            assert_eq!(s.base, r["base"].as_bool().unwrap());
            assert_eq!(s.background, r["background"].as_str().unwrap());
            for k in 0..7 {
                assert_eq!(
                    s.links[k].on,
                    r["links"][k.to_string()]["on"].as_bool().unwrap(),
                    "links.{k}.on"
                );
                assert_eq!(
                    s.links[k].opacity,
                    f(&r["links"][k.to_string()]["opacity"]),
                    "links.{k}.opacity"
                );
            }
            assert_eq!(s.motion_response, f(&r["motion"]["response"]));
        };
        check(&Settings::default(), &g["settings"]["defaults"]);
        for case in g["settings"]["cases"].as_array().unwrap() {
            let mut s = Settings::default();
            apply(&mut s, &case["patch"]);
            check(&s, &case["result"]);
        }
    }

    #[test]
    fn hud_numbers_read_as_in_spanish() {
        assert_eq!(format_count(1234), "1234");
        assert_eq!(format_count(12_345), "12.345");
        assert_eq!(format_count(1_234_567), "1.234.567");
        assert_eq!(format_size(0.0), "0 B");
        assert_eq!(format_size(1536.0), "1,5 KB");
        assert_eq!(format_size(769_126.4), "751,1 KB");
        assert_eq!(format_size(3.0 * 1024.0 * 1024.0 * 1024.0), "3 GB");
    }

    #[test]
    fn the_scene_builds_grows_and_settles_without_jumps() {
        let mut s = Sphere::new(&index(), Processes::Off, 7, None, false).unwrap();
        let tree_segments = s.lines[0].len() / 2;
        assert_eq!(
            tree_segments,
            (s.model.vis.len() - 1) * crate::neural::sphere::SEG
        );
        assert_eq!(s.points.len(), s.model.vis.len());
        assert!(s.points.iter().all(|p| p.born == 1.0));
        // The root's selection lights everything under it.
        assert_eq!(s.lines[7].len(), s.lines[0].len());
        s.begin_grow(6.0);
        assert!(s.points.iter().all(|p| p.position == [0.0; 3]));
        let mut steps = 0;
        let mut max_step = 0.0_f64;
        while s.building() && steps < 2000 {
            s.step(1.0 / 60.0);
            max_step = max_step.max(s.stats().max_step);
            steps += 1;
        }
        let stats = s.stats();
        assert_eq!(
            (stats.born, stats.moving, stats.leaving),
            (stats.total, 0, 0)
        );
        // Never more than 3 R/s: 0.05 R per frame of 1/60 s.
        assert!(max_step <= 0.05 + 1e-9, "max step {max_step}");
        assert!(stats.sparks > 0);
        for p in &s.points {
            assert!(p.position.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn folding_and_another_graph_move_instead_of_jumping() {
        let mut s = Sphere::new(&index(), Processes::Off, 7, None, false).unwrap();
        let folded = s
            .model
            .vis
            .iter()
            .map(|&i| s.model.id(i).to_string())
            .find(|id| {
                s.model.nodes[s.model.by_id[id]].folded && s.model.nodes[s.model.by_id[id]].node.k
            })
            .unwrap();
        let before = s.model.vis.len();
        assert!(s.activate(&folded));
        assert!(s.model.vis.len() > before);
        assert_eq!(s.selected(), folded);
        for _ in 0..600 {
            s.step(1.0 / 60.0);
            assert!(s.stats().max_step <= 0.05 + 1e-9);
        }
        assert_eq!(s.stats().moving, 0);
        // Half the index: what goes folds back into its parent, the selection survives or falls to the root.
        let half: Vec<SphereNode> = index().into_iter().filter(|n| n.d <= 3).collect();
        assert!(s.set_nodes(&half, true));
        assert!(s.stats().leaving > 0 || s.model.vis.len() < before);
        for _ in 0..900 {
            s.step(1.0 / 60.0);
        }
        assert_eq!(s.stats().leaving, 0);
        assert!(s.model.by_id.contains_key(s.selected()));
        assert!(
            !s.set_nodes(&[], true),
            "an index without a root changes nothing"
        );
    }

    #[test]
    fn settings_rebuild_only_what_needs_geometry() {
        let mut s = Sphere::new(&index(), Processes::Off, 7, None, false).unwrap();
        let rebuilds = s.stats().rebuilds;
        assert!(s.configure(|c| c.set_number("brightness", 2.0)));
        assert!(s.configure(|c| c.set_number("glow", 0.5)));
        assert_eq!(s.stats().rebuilds, rebuilds, "appearance is uniforms only");
        assert!(s.configure(|c| c.set_link(2, Some(true), None)));
        assert!(s.stats().rebuilds > rebuilds);
        assert!(!s.lines[2].is_empty(), "mesh links drawn");
        let counts = s.link_counts();
        assert!(counts[2] > 0 && counts[0] > 0 && counts[1] > 0);
        assert!(!s.configure(|c| c.set_number("brightness", f64::NAN)));
        assert_eq!(s.settings.brightness, 2.0);
    }

    #[test]
    fn c_abi_lists_draws_and_rejects_without_unwinding() {
        use crate::neural::ffi::*;
        use crate::neural::sphere::{NeuralPoint, NeuralSpark, NeuralVertex};
        use std::ffi::CString;
        let root = std::env::temp_dir().join(format!("archie-neural-ffi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src/ui")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        for (path, bytes) in [
            ("src/main.rs", 120),
            ("src/ui/app.ts", 40),
            ("README.md", 9),
            (".hidden/x", 1),
        ] {
            std::fs::write(root.join(path), vec![b'x'; bytes]).unwrap();
        }
        let path = root.to_string_lossy().into_owned();
        unsafe {
            assert!(neural_scan(std::ptr::null(), 0, 100, 10).is_null());
            assert!(neural_scan(b"/no/such/folder".as_ptr(), 15, 100, 10).is_null());
            let index = neural_scan(path.as_ptr(), path.len(), 100, 10);
            assert!(!index.is_null());
            // ., src, src/ui, README.md, src/main.rs, src/ui/app.ts: hidden entries are not listed.
            assert_eq!(neural_index_count(index), 6);
            let h = neural_new(index, 7, false);
            assert!(!h.is_null());
            let n = neural_points(h, std::ptr::null_mut(), 0);
            assert_eq!(n, 6);
            let mut points = vec![NeuralPoint::default(); n];
            assert_eq!(
                neural_points(h, points.as_mut_ptr(), n - 1),
                0,
                "a short buffer is refused"
            );
            assert_eq!(neural_points(h, points.as_mut_ptr(), n), n);
            let tree = neural_lines(h, 0, std::ptr::null_mut(), 0);
            assert_eq!(tree, 5 * 10 * 2);
            let mut lines = vec![NeuralVertex::default(); tree];
            assert_eq!(neural_lines(h, 0, lines.as_mut_ptr(), tree), tree);
            assert_eq!(
                neural_lines(h, 1, std::ptr::null_mut(), 0),
                0,
                "layer 1 shares layer 0"
            );
            assert_eq!(neural_lines(h, 99, std::ptr::null_mut(), 0), 0);
            let mut sparks = vec![NeuralSpark::default(); 350];
            assert_eq!(neural_sparks(h, sparks.as_mut_ptr(), 350), 350);
            assert_eq!(neural_decor(0, std::ptr::null_mut(), 0), 1600 * 3);
            assert_eq!(neural_decor(4, std::ptr::null_mut(), 0), 64 * 6 * 4);
            assert_eq!(neural_decor(9, std::ptr::null_mut(), 0), 0);
            let key = CString::new("glow").unwrap();
            assert!(neural_set_number(h, key.as_ptr(), 2.5));
            assert!(!neural_set_number(h, key.as_ptr(), f64::NAN));
            let bad = CString::new("nope").unwrap();
            assert!(!neural_set_number(h, bad.as_ptr(), 1.0));
            assert!(!neural_set_number(h, std::ptr::null(), 1.0));
            let red = CString::new("#AA0011").unwrap();
            assert!(neural_set_background(h, red.as_ptr()));
            assert!(neural_set_link(h, 2, 1, f64::NAN));
            let len = neural_settings_json(h, std::ptr::null_mut(), 0);
            let mut json = vec![0 as std::ffi::c_char; len + 1];
            assert_eq!(neural_settings_json(h, json.as_mut_ptr(), len + 1), len);
            let settings: serde_json::Value =
                serde_json::from_str(std::ffi::CStr::from_ptr(json.as_ptr()).to_str().unwrap())
                    .unwrap();
            assert_eq!(settings["glow"], 2.5);
            assert_eq!(settings["background"], "#aa0011");
            assert_eq!(settings["links"]["2"]["on"], true);
            let id = CString::new("src/main.rs").unwrap();
            assert!(neural_select(h, id.as_ptr()));
            let len = neural_selection(h, std::ptr::null(), std::ptr::null_mut(), 0);
            let mut text = vec![0 as std::ffi::c_char; len + 1];
            assert_eq!(
                neural_selection(h, std::ptr::null(), text.as_mut_ptr(), len + 1),
                len
            );
            let text = std::ffi::CStr::from_ptr(text.as_ptr())
                .to_str()
                .unwrap()
                .to_string();
            assert_eq!(
                text.lines().take(3).collect::<Vec<_>>(),
                ["src/main.rs", "MAIN.RS", "120 B"]
            );
            let len = neural_labels(h, std::ptr::null_mut(), 0);
            let mut labels = vec![0 as std::ffi::c_char; len + 1];
            assert_eq!(neural_labels(h, labels.as_mut_ptr(), len + 1), len);
            assert!(std::ffi::CStr::from_ptr(labels.as_ptr())
                .to_str()
                .unwrap()
                .contains("\tsrc\tSRC\n"));
            let mut at = [0.0_f32; 3];
            assert!(neural_position(h, id.as_ptr(), at.as_mut_ptr()));
            let origin = [at[0] * 4.0, at[1] * 4.0, at[2] * 4.0];
            let direction = [at[0] - origin[0], at[1] - origin[1], at[2] - origin[2]];
            let mut picked = vec![0 as std::ffi::c_char; 64];
            assert_eq!(
                neural_pick(
                    h,
                    origin.as_ptr(),
                    direction.as_ptr(),
                    2.2,
                    picked.as_mut_ptr(),
                    64
                ),
                11
            );
            assert_eq!(
                std::ffi::CStr::from_ptr(picked.as_ptr()).to_str().unwrap(),
                "src/main.rs"
            );
            neural_grow(h, 1.0);
            assert!(neural_building(h));
            for _ in 0..240 {
                neural_step(h, 1.0 / 60.0);
            }
            assert!(!neural_building(h));
            let mut stats = NeuralStats::default();
            assert!(neural_stats(h, &mut stats));
            assert_eq!((stats.visible, stats.born, stats.moving), (6, 6, 0));
            let mut counts = [0_u32; 7];
            assert!(neural_link_counts(h, counts.as_mut_ptr()));
            assert_eq!((counts[0], counts[1]), (2, 3));
            assert!(!neural_set_index(h, std::ptr::null(), true));
            assert!(neural_set_index(h, index, false));
            neural_free(h);
            neural_index_free(index);
            // Null handles are inert.
            neural_step(std::ptr::null_mut(), 0.1);
            assert_eq!(neural_points(std::ptr::null(), std::ptr::null_mut(), 0), 0);
            assert!(!neural_select(std::ptr::null_mut(), id.as_ptr()));
            neural_free(std::ptr::null_mut());
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn picking_selects_the_nearest_node_along_the_ray() {
        let mut s = Sphere::new(&index(), Processes::Off, 7, None, false).unwrap();
        let id = s
            .model
            .vis
            .iter()
            .map(|&i| s.model.id(i).to_string())
            .find(|id| !s.model.nodes[s.model.by_id[id]].node.k)
            .unwrap();
        let at = s.position_of(&id).unwrap();
        let origin = [at[0] * 4.0, at[1] * 4.0, at[2] * 4.0];
        let direction = [at[0] - origin[0], at[1] - origin[1], at[2] - origin[2]];
        assert_eq!(s.pick(origin, direction, 2.2).as_deref(), Some(id.as_str()));
        assert!(s.activate(&id));
        assert_eq!(
            s.selection_text(None)[0],
            s.model.nodes[s.model.by_id[&id]].node.n.to_uppercase()
        );
        assert_eq!(s.pick([1e6, 1e6, 1e6], [1.0, 0.0, 0.0], 2.2), None);
    }
}
