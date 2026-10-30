//! Fixed loopback read-only projection. No command token, provider URLs or Core writes.
use archie_worldd::exploration::ExplorationSnapshot;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn current() -> ExplorationSnapshot {
    tokio::time::timeout(std::time::Duration::from_millis(1500), fetch(None))
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_else(|| ExplorationSnapshot::unavailable("unavailable"))
}

pub fn valid_job_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub async fn selected(id: &str) -> Option<ExplorationSnapshot> {
    if !valid_job_id(id) {
        return None;
    }
    tokio::time::timeout(std::time::Duration::from_millis(1500), fetch(Some(id)))
        .await
        .ok()?
        .ok()
}

async fn fetch(selected: Option<&str>) -> Result<ExplorationSnapshot, ()> {
    let token = archie_worldd::exploration_projection::read_token(
        &archie_worldd::exploration_projection::default_runtime_dir(),
    )
    .map_err(|_| ())?;
    let port = match std::env::var("ARCHIE_WORLDD_PORT") {
        Ok(value) => value.parse::<u16>().map_err(|_| ())?,
        Err(std::env::VarError::NotPresent) => 7667,
        Err(_) => return Err(()),
    };
    if port == 0 {
        return Err(());
    }
    let mut stream = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|_| ())?;
    let path = match selected {
        Some(id) if valid_job_id(id) => format!("/v1/exploration/jobs/{id}"),
        Some(_) => return Err(()),
        None => "/v1/exploration".into(),
    };
    stream
        .write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n").as_bytes())
        .await
        .map_err(|_| ())?;
    let mut bytes = Vec::new();
    stream
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| ())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(());
    }
    let split = bytes.windows(4).position(|w| w == b"\r\n\r\n").ok_or(())?;
    let head = std::str::from_utf8(&bytes[..split]).map_err(|_| ())?;
    if head.lines().next().is_none_or(|s| s != "HTTP/1.1 200 OK") {
        return Err(());
    }
    let view: ExplorationSnapshot = serde_json::from_slice(&bytes[split + 4..]).map_err(|_| ())?;
    validate(&view)?;
    if view.selected_job_id.as_deref() != selected {
        return Err(());
    }
    Ok(view)
}

fn validate(view: &ExplorationSnapshot) -> Result<(), ()> {
    if view.schema_version != 1
        || !["live", "disabled", "unavailable"].contains(&view.source.as_str())
        || view.jobs.len() > 32
        || view.providers.len() > 16
    {
        return Err(());
    }
    if let Some(id) = view.selected_job_id.as_deref() {
        if !valid_job_id(id)
            || !view.jobs.iter().any(|job| job.id == id)
            || view
                .jobs
                .iter()
                .any(|job| job.id != id && job.preview.is_some())
        {
            return Err(());
        }
    }
    for job in &view.jobs {
        if job.domain != view.domain
            || job.request.domain != view.domain
            || job.id.len() != 32
            || !job.id.bytes().all(|b| b.is_ascii_hexdigit())
            || archie_geofabric::exploration::plan(&job.request).is_err()
            || job.artifacts.len() > 8194
        {
            return Err(());
        }
        if let Some(p) = &job.preview {
            if !p.is_object() {
                return Err(());
            }
            let terrain = &p["terrain"];
            if !terrain.is_null() {
                let w = terrain["width"].as_u64().ok_or(())?;
                let h = terrain["height"].as_u64().ok_or(())?;
                if w == 0
                    || h == 0
                    || w > 33
                    || h > 33
                    || terrain["elevations_m"].as_array().ok_or(())?.len() as u64 != w * h
                {
                    return Err(());
                }
            }
            if let Some(buildings) = p.get("buildings") {
                let buildings = buildings.as_array().ok_or(())?;
                if buildings.len() > 128
                    || buildings
                        .iter()
                        .any(|b| b["ring"].as_array().is_none_or(|ring| ring.len() > 2048))
                {
                    return Err(());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_foreign_schema_and_unbounded_projection() {
        let mut v = ExplorationSnapshot::unavailable("unavailable");
        assert!(validate(&v).is_ok());
        v.schema_version = 2;
        assert!(validate(&v).is_err());
    }

    #[test]
    fn selected_projection_accepts_imagery_only_and_never_substitutes_another_preview() {
        let request = archie_geofabric::exploration::ExplorationRequest::default();
        let plan = archie_geofabric::exploration::plan(&request).unwrap();
        let first = "1".repeat(32);
        let second = "2".repeat(32);
        let job = serde_json::json!({"id":first,"domain":"personal","state":"complete","request":request,
            "camera":plan.camera,"plan":plan,"tiles_total":0,"tiles_done":0,"bytes_received":0,"cache_hits":0,
            "error":null,"artifacts":[],"preview":{"imagery":[]},"created_at_ms":1,"updated_at_ms":1});
        let mut view = ExplorationSnapshot::unavailable("live");
        view.jobs.push(serde_json::from_value(job).unwrap());
        let mut latest = view.jobs[0].clone();
        latest.id = second;
        latest.preview = None;
        view.jobs.push(latest);
        view.selected_job_id = Some(first);
        assert!(validate(&view).is_ok());
        view.jobs[1].preview = Some(serde_json::json!({"imagery":[]}));
        assert!(validate(&view).is_err());
        view.jobs[1].preview = None;
        view.selected_job_id = Some("3".repeat(32));
        assert!(validate(&view).is_err());
    }
}
