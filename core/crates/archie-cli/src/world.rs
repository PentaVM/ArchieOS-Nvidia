//! Thin client of worldd's authenticated local command service.
use archie_geofabric::exploration::{
    self, ExplorationDomain, ExplorationLayer, ExplorationRequest, ImagerySource,
};
use archie_worldd::{exploration::ExploreCommand, navigation::NavigationRequest};
use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Domain {
    Personal,
    Work,
}
impl From<Domain> for ExplorationDomain {
    fn from(value: Domain) -> Self {
        match value {
            Domain::Personal => Self::Personal,
            Domain::Work => Self::Work,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Layer {
    Terrain,
    Buildings,
    Imagery,
    Signals,
    StreetView,
    Mesh,
}
impl From<Layer> for ExplorationLayer {
    fn from(value: Layer) -> Self {
        match value {
            Layer::Terrain => Self::Terrain,
            Layer::Buildings => Self::Buildings,
            Layer::Imagery => Self::Imagery,
            Layer::Signals => Self::Signals,
            Layer::StreetView => Self::StreetView,
            Layer::Mesh => Self::Mesh,
        }
    }
}

#[derive(Debug, Args)]
pub struct Region {
    #[arg(long, allow_hyphen_values = true)]
    pub latitude: f64,
    #[arg(long, allow_hyphen_values = true)]
    pub longitude: f64,
    #[arg(long, default_value_t = 500.0, allow_hyphen_values = true)]
    pub radius_m: f64,
    #[arg(long, default_value_t = 14)]
    pub zoom: u8,
    #[arg(long, value_enum, value_delimiter = ',', default_value = "terrain")]
    pub layers: Vec<Layer>,
    #[arg(long, value_enum)]
    pub domain: Domain,
    #[arg(long, default_value_t = 256)]
    pub max_tiles: u32,
    #[arg(long, default_value_t = 67_108_864)]
    pub max_bytes: u64,
    /// google-satellite, blue-marble, sentinel2, landsat, natural-color, infrared, night-lights, thermal.
    #[arg(long, value_delimiter = ',')]
    pub imagery_sources: Vec<ImagerySource>,
    #[arg(long)]
    pub imagery_date: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub signal_updates: u32,
    #[arg(long, default_value_t = 60)]
    pub signal_interval_seconds: u32,
}
impl Region {
    fn request(&self) -> Result<ExplorationRequest, String> {
        let request = ExplorationRequest {
            latitude: self.latitude,
            longitude: self.longitude,
            radius_m: self.radius_m,
            zoom: self.zoom,
            layers: self.layers.iter().copied().map(Into::into).collect(),
            domain: self.domain.into(),
            max_tiles: self.max_tiles,
            max_bytes: self.max_bytes,
            imagery_sources: self.imagery_sources.clone(),
            imagery_date: self.imagery_date.clone(),
            signal_updates: self.signal_updates,
            signal_interval_seconds: self.signal_interval_seconds,
        };
        exploration::plan(&request).map_err(|error| error.to_string())?;
        Ok(request)
    }
}

#[derive(Debug, Args)]
pub struct Navigation {
    #[arg(long, value_enum)]
    pub domain: Domain,
    #[arg(long, value_delimiter = ',')]
    pub imagery_sources: Vec<ImagerySource>,
    #[arg(long)]
    pub imagery_date: Option<String>,
    #[arg(long, default_value_t = 19)]
    pub max_zoom: u8,
    #[arg(long, default_value_t = 64)]
    pub max_visible_tiles: u32,
    #[arg(long, default_value_t = 1800)]
    pub max_requests: u32,
    #[arg(long, default_value_t = 268_435_456)]
    pub max_bytes: u64,
    #[arg(long, default_value_t = 3600)]
    pub duration_seconds: u32,
    #[arg(long)]
    pub no_signals: bool,
    /// Disable preview-only Earth Web textured meshes for this session.
    #[arg(long)]
    pub no_photogrammetry: bool,
}
impl Navigation {
    fn request(&self) -> Result<NavigationRequest, String> {
        let defaults = NavigationRequest::default();
        let request = NavigationRequest {
            domain: self.domain.into(),
            sources: if self.imagery_sources.is_empty() {
                defaults.sources
            } else {
                self.imagery_sources.clone()
            },
            imagery_date: self.imagery_date.clone(),
            max_zoom: self.max_zoom,
            max_visible_tiles: self.max_visible_tiles,
            max_requests: self.max_requests,
            max_bytes: self.max_bytes,
            duration_seconds: self.duration_seconds,
            signals: !self.no_signals,
            photogrammetry: !self.no_photogrammetry,
        };
        request.validate()?;
        Ok(request)
    }
}

#[derive(Debug, Subcommand)]
pub enum WorldCommand {
    /// Authorize bounded whole-Earth imagery streaming as the viewport moves.
    Navigate(Navigation),
    /// Read metadata for one navigation session.
    NavigationStatus {
        id: String,
        #[arg(long, value_enum)]
        domain: Domain,
    },
    /// Stop one bounded navigation session.
    NavigationStop {
        id: String,
        #[arg(long, value_enum)]
        domain: Domain,
    },
    /// Plan the explicit circle; downloads no provider data.
    Plan(Region),
    /// Submit a bounded capture; acceptance does not mean completion.
    Explore(Region),
    /// Resolve a place name into candidate coordinates; starts no capture.
    Resolve {
        query: String,
        #[arg(long, value_enum)]
        domain: Domain,
    },
    Status {
        #[arg(long, value_enum)]
        domain: Domain,
    },
    Pause {
        id: String,
        #[arg(long, value_enum)]
        domain: Domain,
    },
    Resume {
        id: String,
        #[arg(long, value_enum)]
        domain: Domain,
    },
    Cancel {
        id: String,
        #[arg(long, value_enum)]
        domain: Domain,
    },
}

impl WorldCommand {
    fn command(&self) -> Result<ExploreCommand, String> {
        Ok(match self {
            Self::Navigate(navigation) => ExploreCommand::Navigate {
                request: navigation.request()?,
            },
            Self::NavigationStatus { id, domain } | Self::NavigationStop { id, domain } => {
                if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err("invalid navigation session id".into());
                }
                if matches!(self, Self::NavigationStatus { .. }) {
                    ExploreCommand::NavigationStatus {
                        id: id.clone(),
                        domain: (*domain).into(),
                    }
                } else {
                    ExploreCommand::NavigationStop {
                        id: id.clone(),
                        domain: (*domain).into(),
                    }
                }
            }
            Self::Plan(region) => ExploreCommand::Plan {
                request: region.request()?,
            },
            Self::Explore(region) => ExploreCommand::Explore {
                request: region.request()?,
            },
            Self::Resolve { query, domain } => {
                if query.trim().is_empty() || query.chars().count() > 256 {
                    return Err("place query must contain 1..256 characters".into());
                }
                ExploreCommand::Resolve {
                    query: query.clone(),
                    domain: (*domain).into(),
                }
            }
            Self::Status { domain } => ExploreCommand::Status {
                domain: (*domain).into(),
            },
            Self::Pause { id, domain }
            | Self::Resume { id, domain }
            | Self::Cancel { id, domain } => {
                if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err("invalid exploration job id".into());
                }
                match self {
                    Self::Pause { .. } => ExploreCommand::Pause {
                        id: id.clone(),
                        domain: (*domain).into(),
                    },
                    Self::Resume { .. } => ExploreCommand::Resume {
                        id: id.clone(),
                        domain: (*domain).into(),
                    },
                    _ => ExploreCommand::Cancel {
                        id: id.clone(),
                        domain: (*domain).into(),
                    },
                }
            }
        })
    }
}

pub async fn run(command: &WorldCommand, json: bool) -> std::process::ExitCode {
    let result = match command.command() {
        Ok(request) => archie_worldd::exploration_control::send(request).await,
        Err(error) => Err(error),
    };
    match result {
        Ok(value) => {
            if !json && value.get("job").is_some() {
                println!("Command accepted; this acknowledges the requested job state, not capture completion.");
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&value).expect("JSON response")
            );
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("archie world: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: WorldCommand,
    }

    #[test]
    fn photogrammetry_is_new_navigation_default_and_can_be_disabled() {
        for (extra, expected) in [(vec![], true), (vec!["--no-photogrammetry"], false)] {
            let mut args = vec!["world", "navigate", "--domain", "personal"];
            args.extend(extra);
            let cli = TestCli::try_parse_from(args).unwrap();
            let value = serde_json::to_value(cli.command.command().unwrap()).unwrap();
            assert_eq!(value["request"]["photogrammetry"], expected);
        }
    }

    #[test]
    fn navigation_is_typed_bounded_and_requires_explicit_domain() {
        assert!(TestCli::try_parse_from(["world", "navigate"]).is_err());
        let cli = TestCli::try_parse_from([
            "world",
            "navigate",
            "--domain",
            "work",
            "--imagery-sources",
            "google-satellite,blue-marble",
            "--max-requests",
            "80",
            "--max-visible-tiles",
            "24",
            "--duration-seconds",
            "600",
        ])
        .unwrap();
        let value = serde_json::to_value(cli.command.command().unwrap()).unwrap();
        assert_eq!(value["action"], "navigate");
        assert_eq!(value["request"]["domain"], "work");
        assert_eq!(value["request"]["max_requests"], 80);
        assert_eq!(value["request"]["max_visible_tiles"], 24);
        assert_eq!(value["request"]["duration_seconds"], 600);
        assert_eq!(
            value["request"]["sources"],
            serde_json::json!(["google_satellite", "blue_marble"])
        );
        let cli =
            TestCli::try_parse_from(["world", "navigate", "--domain", "personal", "--no-signals"])
                .unwrap();
        let value = serde_json::to_value(cli.command.command().unwrap()).unwrap();
        assert_eq!(value["request"]["signals"], false);
        let cli = TestCli::try_parse_from([
            "world",
            "navigate",
            "--domain",
            "personal",
            "--duration-seconds",
            "3601",
        ])
        .unwrap();
        assert!(cli.command.command().is_err());
        for command in ["navigation-status", "navigation-stop"] {
            let cli = TestCli::try_parse_from(["world", command, "../escape", "--domain", "work"])
                .unwrap();
            assert!(cli.command.command().is_err());
        }
    }

    #[test]
    fn typed_imagery_and_bounded_refresh_flags_reach_the_request() {
        let cli = TestCli::try_parse_from([
            "world",
            "explore",
            "--latitude",
            "0",
            "--longitude",
            "0",
            "--domain",
            "personal",
            "--layers",
            "imagery,signals",
            "--imagery-sources",
            "google-satellite,blue-marble",
            "--imagery-date",
            "2024-02-29",
            "--signal-updates",
            "2",
            "--signal-interval-seconds",
            "120",
        ])
        .unwrap();
        let ExploreCommand::Explore { request } = cli.command.command().unwrap() else {
            panic!("explore")
        };
        assert_eq!(
            serde_json::to_value(&request.imagery_sources).unwrap(),
            serde_json::json!(["google_satellite", "blue_marble"])
        );
        assert_eq!(request.signal_updates, 2);
        assert_eq!(request.signal_interval_seconds, 120);
        assert_eq!(request.imagery_date.as_deref(), Some("2024-02-29"));
    }

    #[test]
    fn explicit_work_region_becomes_bounded_exploration() {
        let cli = TestCli::try_parse_from([
            "world",
            "explore",
            "--latitude",
            "-33",
            "--longitude",
            "-70",
            "--domain",
            "work",
            "--layers",
            "terrain,buildings",
        ])
        .unwrap();
        let ExploreCommand::Explore { request } = cli.command.command().unwrap() else {
            panic!("explore command");
        };
        assert_eq!(request.domain, ExplorationDomain::Work);
        assert_eq!(
            request.layers,
            vec![ExplorationLayer::Terrain, ExplorationLayer::Buildings]
        );
        assert_eq!(request.radius_m, 500.0);
        assert_eq!(request.max_bytes, 64 * 1024 * 1024);
    }

    #[test]
    fn missing_unknown_domain_and_invalid_geography_never_connect() {
        for argv in [
            vec!["world", "status"],
            vec!["world", "status", "--domain", "corporate"],
            vec!["world", "explore", "--latitude", "0", "--longitude", "0"],
        ] {
            assert!(TestCli::try_parse_from(argv).is_err());
        }
        let invalid = TestCli::try_parse_from([
            "world",
            "plan",
            "--latitude",
            "91",
            "--longitude",
            "0",
            "--domain",
            "personal",
        ])
        .unwrap();
        assert!(invalid.command.command().is_err());
        let invalid =
            TestCli::try_parse_from(["world", "resume", "../escape", "--domain", "work"]).unwrap();
        assert!(invalid.command.command().is_err());
    }
}
