//! A separate read-only credential: possession cannot submit commands or spend quota.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn default_runtime_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("ARCHIE_WORLD_RUNTIME_DIR") {
        return path.into();
    }
    if let Some(path) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(path).join("archie-worldd");
    }
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(path).join("ArchieOS/worldd/runtime");
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|p| p.join(".cache/archie-worldd/runtime"))
        .unwrap_or_else(|| PathBuf::from("/nonexistent/archie-worldd-runtime-unconfigured"))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Credential {
    schema_version: u16,
    token: String,
}

pub fn read_token(runtime: &Path) -> Result<String, String> {
    let path = runtime.join("projection.json");
    let meta = path
        .symlink_metadata()
        .map_err(|_| "private World projection unavailable")?;
    if !meta.is_file() || meta.len() > 512 {
        return Err("invalid World projection credential".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err("World projection credential is not private".into());
        }
    }
    let value: Credential = serde_json::from_slice(
        &std::fs::read(path).map_err(|_| "cannot read World projection credential")?,
    )
    .map_err(|_| "invalid World projection credential")?;
    if value.schema_version != 1
        || value.token.len() != 64
        || !value.token.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("invalid World projection credential".into());
    }
    Ok(value.token)
}
#[cfg(feature = "provider")]
pub fn ensure(runtime: &Path) -> Result<(), String> {
    use rand::RngCore;
    crate::exploration_control::secure_owner_path(runtime, true)?;
    let path = runtime.join("projection.json");
    if path.exists() {
        crate::exploration_control::secure_owner_path(&path, false)?;
        read_token(runtime)?;
        return Ok(());
    }
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let token = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    crate::exploration_runtime::atomic_write(
        &path,
        &serde_json::to_vec(&Credential {
            schema_version: 1,
            token,
        })
        .map_err(|e| e.to_string())?,
    )?;
    crate::exploration_control::secure_owner_path(&path, false)
}
pub fn authorized(runtime: &Path, header: Option<&str>) -> bool {
    let Some(value) = header.and_then(|v| v.strip_prefix("Bearer ")) else {
        return false;
    };
    let Ok(token) = read_token(runtime) else {
        return false;
    };
    value.len() == token.len()
        && value
            .bytes()
            .zip(token.bytes())
            .fold(0u8, |diff, (a, b)| std::hint::black_box(diff | (a ^ b)))
            == 0
}
#[cfg(all(test, feature = "provider"))]
mod tests {
    use super::*;
    #[test]
    fn separate_view_credential_refuses_anonymous_and_cannot_read_command_token() {
        let root =
            std::env::temp_dir().join(format!("archie-view-{:032x}", rand::random::<u128>()));
        crate::exploration_runtime::private_directory(&root).unwrap();
        ensure(&root).unwrap();
        assert!(!root.join("control.json").exists());
        assert!(!authorized(&root, None));
        assert!(!authorized(&root, Some("Bearer bad")));
        let token = read_token(&root).unwrap();
        assert!(authorized(&root, Some(&format!("Bearer {token}"))));
        std::fs::remove_dir_all(root).unwrap();
    }
}
