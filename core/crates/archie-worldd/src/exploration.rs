//! Public read-only exploration contract. No HTTP client, credentials or desktop focus.
use archie_geofabric::exploration::{ExplorationDomain, ExplorationPlan, ExplorationRequest};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExploreCommand {
    Navigate {
        request: crate::navigation::NavigationRequest,
    },
    NavigationStatus {
        id: String,
        domain: ExplorationDomain,
    },
    NavigationStop {
        id: String,
        domain: ExplorationDomain,
    },
    Plan {
        request: ExplorationRequest,
    },
    Explore {
        request: ExplorationRequest,
    },
    Resolve {
        query: String,
        domain: ExplorationDomain,
    },
    Status {
        domain: ExplorationDomain,
    },
    Pause {
        id: String,
        domain: ExplorationDomain,
    },
    Resume {
        id: String,
        domain: ExplorationDomain,
    },
    Cancel {
        id: String,
        domain: ExplorationDomain,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    WaitingRateLimit,
    Paused,
    Complete,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub layer: String,
    pub kind: String,
    pub relative_path: String,
    pub source: String,
    pub retrieved_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationJob {
    pub id: String,
    pub domain: ExplorationDomain,
    pub state: JobState,
    pub request: ExplorationRequest,
    pub camera: archie_geofabric::exploration::CameraTarget,
    pub tiles_total: u64,
    pub tiles_done: u64,
    pub bytes_received: u64,
    #[serde(default)]
    pub stored_bytes: u64,
    #[serde(default)]
    pub signal_updates_done: u32,
    #[serde(default)]
    pub pinned_imagery_date: Option<String>,
    pub cache_hits: u64,
    pub error: Option<String>,
    pub artifacts: Vec<Artifact>,
    /// Bounded normalized samples only; no untrusted paths or provider JSON.
    pub preview: Option<serde_json::Value>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub plan: ExplorationPlan,
    #[serde(default)]
    pub projection_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub id: String,
    pub state: String,
    pub requests_sent: u64,
    pub bytes_received: u64,
    pub requests_per_minute: u32,
    pub max_in_flight: u32,
    pub in_flight: u32,
    pub remaining_requests: Option<u64>,
    pub retry_at_ms: Option<i64>,
    pub quota_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationSnapshot {
    pub schema_version: u16,
    pub source: String,
    pub generated_at_ms: i64,
    pub domain: ExplorationDomain,
    pub jobs: Vec<ExplorationJob>,
    /// Read-only viewer selection; absent for the normal latest-preview view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_job_id: Option<String>,
    pub providers: Vec<ProviderUsage>,
    pub error: Option<String>,
}

impl ExplorationSnapshot {
    pub fn unavailable(source: &str) -> Self {
        Self {
            schema_version: 1,
            source: source.into(),
            generated_at_ms: 0,
            domain: ExplorationDomain::Personal,
            jobs: vec![],
            selected_job_id: None,
            providers: vec![],
            error: Some("exploration service unavailable".into()),
        }
    }
}
