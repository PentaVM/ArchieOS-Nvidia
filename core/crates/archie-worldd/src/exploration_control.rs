//! Owner-only command transport, separate from worldd's public read-only snapshots.

#[cfg(windows)]
mod windows_privacy {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path, ptr};
    type Handle = *mut c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn LocalFree(memory: Handle) -> Handle;
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        fn GetTokenInformation(
            token: Handle,
            class: u32,
            data: Handle,
            length: u32,
            required: *mut u32,
        ) -> i32;
        fn ConvertSidToStringSidW(sid: Handle, string: *mut *mut u16) -> i32;
        fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
            string: *const u16,
            revision: u32,
            descriptor: *mut Handle,
            size: *mut u32,
        ) -> i32;
        fn SetFileSecurityW(path: *const u16, information: u32, descriptor: Handle) -> i32;
        fn GetFileSecurityW(
            path: *const u16,
            information: u32,
            descriptor: Handle,
            length: u32,
            required: *mut u32,
        ) -> i32;
        fn GetSecurityDescriptorOwner(
            descriptor: Handle,
            owner: *mut Handle,
            defaulted: *mut i32,
        ) -> i32;
        fn GetSecurityDescriptorDacl(
            descriptor: Handle,
            present: *mut i32,
            acl: *mut Handle,
            defaulted: *mut i32,
        ) -> i32;
        fn GetAclInformation(acl: Handle, information: Handle, length: u32, class: u32) -> i32;
        fn GetAce(acl: Handle, index: u32, ace: *mut Handle) -> i32;
        fn EqualSid(first: Handle, second: Handle) -> i32;
    }

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    fn aligned(length: u32) -> Vec<usize> {
        vec![0usize; (length as usize).div_ceil(std::mem::size_of::<usize>())]
    }

    fn with_sid<T>(operation: impl FnOnce(Handle) -> Result<T, String>) -> Result<T, String> {
        // SAFETY: all output pointers point to allocated storage; token buffers
        // remain alive until operation returns. Every owned OS allocation is freed.
        unsafe {
            let mut token = ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), 8, &mut token) == 0 {
                return Err("cannot identify world control owner".into());
            }
            let mut required = 0;
            GetTokenInformation(token, 1, ptr::null_mut(), 0, &mut required);
            if required == 0 {
                CloseHandle(token);
                return Err("cannot query world control owner".into());
            }
            let mut buffer = aligned(required);
            let success = GetTokenInformation(
                token,
                1,
                buffer.as_mut_ptr().cast(),
                required,
                &mut required,
            );
            CloseHandle(token);
            if success == 0 {
                return Err("cannot read world control owner".into());
            }
            // TOKEN_USER starts with SID_AND_ATTRIBUTES, whose first field is PSID.
            operation(*buffer.as_ptr().cast::<Handle>())
        }
    }

    pub(super) fn restrict(path: &Path, directory: bool) -> Result<(), String> {
        with_sid(|sid| {
            // SAFETY: SID is valid for this closure; conversion allocations are
            // copied before LocalFree, and SetFileSecurityW copies the descriptor.
            unsafe {
                let mut text: *mut u16 = ptr::null_mut();
                if ConvertSidToStringSidW(sid, &mut text) == 0 {
                    return Err("cannot encode world control owner".into());
                }
                let mut length = 0;
                while *text.add(length) != 0 {
                    length += 1;
                }
                let owner = String::from_utf16_lossy(std::slice::from_raw_parts(text, length));
                LocalFree(text.cast());
                let flags = if directory { "OICI" } else { "" };
                let sddl: Vec<u16> = format!("D:P(A;{flags};FA;;;{owner})")
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let mut descriptor = ptr::null_mut();
                if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    1,
                    &mut descriptor,
                    ptr::null_mut(),
                ) == 0
                {
                    return Err("cannot create world control private ACL".into());
                }
                let success = SetFileSecurityW(wide(path).as_ptr(), 0x8000_0004, descriptor);
                LocalFree(descriptor);
                if success == 0 {
                    return Err("cannot restrict world control ACL".into());
                }
                Ok(())
            }
        })
    }

    pub(super) fn verify(path: &Path) -> Result<(), String> {
        with_sid(|sid| {
            // SAFETY: Windows validates the file security descriptor and ACL.
            // All pointers returned by descriptor APIs remain within live storage.
            unsafe {
                let path = wide(path);
                let mut required = 0;
                GetFileSecurityW(path.as_ptr(), 5, ptr::null_mut(), 0, &mut required);
                if required == 0 {
                    return Err("cannot inspect world control ACL".into());
                }
                let mut buffer = aligned(required);
                let descriptor = buffer.as_mut_ptr().cast();
                if GetFileSecurityW(path.as_ptr(), 5, descriptor, required, &mut required) == 0 {
                    return Err("cannot read world control ACL".into());
                }
                let mut owner = ptr::null_mut();
                let mut defaulted = 0;
                if GetSecurityDescriptorOwner(descriptor, &mut owner, &mut defaulted) == 0
                    || EqualSid(owner, sid) == 0
                {
                    return Err("world control path belongs to another owner".into());
                }
                let mut present = 0;
                let mut acl = ptr::null_mut();
                if GetSecurityDescriptorDacl(descriptor, &mut present, &mut acl, &mut defaulted)
                    == 0
                    || present == 0
                    || acl.is_null()
                {
                    return Err("world control ACL is public".into());
                }
                // ACL_SIZE_INFORMATION = AceCount, AclBytesInUse, AclBytesFree.
                let mut information = [0u32; 3];
                if GetAclInformation(acl, information.as_mut_ptr().cast(), 12, 2) == 0
                    || information[0] == 0
                {
                    return Err("invalid world control ACL".into());
                }
                for index in 0..information[0] {
                    let mut ace = ptr::null_mut();
                    if GetAce(acl, index, &mut ace) == 0 {
                        return Err("invalid world control ACE".into());
                    }
                    // ACCESS_ALLOWED_ACE has a four-byte header, four-byte mask,
                    // then SID. Refuse every other ACE form and every other SID.
                    if *ace.cast::<u8>() != 0 || EqualSid(ace.cast::<u8>().add(8).cast(), sid) == 0
                    {
                        return Err("world control ACL permits another principal".into());
                    }
                }
                Ok(())
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archie_geofabric::exploration::ExplorationDomain;

    #[test]
    fn malformed_auth_and_unknown_fields_are_refused() {
        let token = "a".repeat(64);
        for request in [
            serde_json::json!({"token":"bad","command":{"action":"status","domain":"personal"}}),
            serde_json::json!({"token":token,"command":{"action":"status","domain":"personal"},"endpoint":"https://example.com"}),
            serde_json::json!({"token":token,"command":{"action":"status","domain":"unknown"}}),
            serde_json::json!({"token":token,"command":{"action":"status","domain":"personal","focus":true}}),
        ] {
            assert!(authenticate(&serde_json::to_vec(&request).unwrap(), &token).is_err());
        }
        let valid =
            serde_json::json!({"token":token,"command":{"action":"status","domain":"work"}});
        assert!(matches!(
            authenticate(&serde_json::to_vec(&valid).unwrap(), &token).unwrap(),
            ExploreCommand::Status {
                domain: ExplorationDomain::Work
            }
        ));
    }

    #[test]
    fn navigation_wire_accepts_only_typed_bounded_request_fields() {
        let token = "a".repeat(64);
        let request =
            serde_json::to_value(crate::navigation::NavigationRequest::default()).unwrap();
        let mut envelope =
            serde_json::json!({"token":token,"command":{"action":"navigate","request":request}});
        assert!(authenticate(&serde_json::to_vec(&envelope).unwrap(), &token).is_ok());
        envelope["command"]["request"]["endpoint"] = serde_json::json!("https://example.com");
        assert!(authenticate(&serde_json::to_vec(&envelope).unwrap(), &token).is_err());
    }

    #[test]
    fn unknown_nested_request_fields_are_refused() {
        let token = "a".repeat(64);
        let mut request =
            serde_json::to_value(archie_geofabric::exploration::ExplorationRequest::default())
                .unwrap();
        request["endpoint"] = serde_json::json!("https://example.com");
        let envelope =
            serde_json::json!({"token":token,"command":{"action":"plan","request":request}});
        assert!(authenticate(&serde_json::to_vec(&envelope).unwrap(), &token).is_err());
    }

    #[test]
    fn descriptors_refuse_non_loopback_and_invalid_tokens() {
        for address in [
            "192.0.2.1:7669",
            "0.0.0.0:1234",
            "example.com:1234",
            "127.0.0.1:0",
        ] {
            let descriptor = Descriptor {
                schema_version: 1,
                address: address.into(),
                token: "b".repeat(64),
            };
            assert!(validate_descriptor(&descriptor).is_err());
        }
        let mut descriptor = Descriptor {
            schema_version: 1,
            address: "127.0.0.1:12345".into(),
            token: "b".repeat(64),
        };
        assert!(validate_descriptor(&descriptor).is_ok());
        descriptor.token = "not a credential".into();
        assert!(validate_descriptor(&descriptor).is_err());
    }

    #[tokio::test]
    async fn private_control_authenticates_and_preserves_domain() {
        let root = std::env::temp_dir().join(format!(
            "archie-world-control-test-{}",
            rand::random::<u64>()
        ));
        let runtime = root.join("runtime");
        let service = ExplorationService::new(crate::exploration_runtime::ExplorationConfig {
            cache_dir: root.join("cache"),
            domain: ExplorationDomain::Work,
        })
        .unwrap();
        let (stop, shutdown) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(run(service.clone(), runtime.clone(), shutdown));
        for _ in 0..100 {
            if runtime.join("control.json").exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let (_duplicate_stop, duplicate_shutdown) = tokio::sync::watch::channel(false);
        assert!(run(service, runtime.clone(), duplicate_shutdown)
            .await
            .unwrap_err()
            .contains("another world command service"));
        let result = send_in(
            &runtime,
            ExploreCommand::Status {
                domain: ExplorationDomain::Work,
            },
        )
        .await
        .unwrap();
        assert_eq!(result["domain"], "work");
        assert!(send_in(
            &runtime,
            ExploreCommand::Status {
                domain: ExplorationDomain::Personal
            }
        )
        .await
        .is_err());
        let mut descriptor: Descriptor =
            serde_json::from_slice(&std::fs::read(runtime.join("control.json")).unwrap()).unwrap();
        descriptor.token = "0".repeat(64);
        let denied = exchange(
            &descriptor,
            ExploreCommand::Status {
                domain: ExplorationDomain::Work,
            },
        )
        .await;
        assert!(denied.is_err());
        stop.send(true).unwrap();
        task.await.unwrap().unwrap();
        assert!(!runtime.join("control.json").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
use crate::exploration::ExploreCommand;
use crate::exploration_runtime::{atomic_write, private_directory, ExplorationService};
use fs2::FileExt;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, Semaphore};

const REQUEST_LIMIT: usize = 32 * 1024;
const REPLY_LIMIT: usize = 8 * 1024 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(85);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    schema_version: u16,
    address: String,
    token: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    token: String,
    command: ExploreCommand,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    ok: bool,
    result: Option<Value>,
    error: Option<String>,
}

/// Isolated from Core state. An explicit override supports one runtime per domain.
pub use crate::exploration_projection::default_runtime_dir;

fn valid_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_descriptor(descriptor: &Descriptor) -> Result<SocketAddr, String> {
    let address: SocketAddr = descriptor
        .address
        .parse()
        .map_err(|_| "invalid world control address")?;
    if descriptor.schema_version != 1
        || !address.ip().is_loopback()
        || address.port() == 0
        || !valid_token(&descriptor.token)
    {
        return Err("invalid private world control descriptor".into());
    }
    Ok(address)
}

fn authenticate(bytes: &[u8], expected: &str) -> Result<ExploreCommand, String> {
    if bytes.len() > REQUEST_LIMIT {
        return Err("world command exceeds size limit".into());
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|_| "invalid world command")?;
    if !valid_token(&envelope.token) || !valid_token(expected) {
        return Err("world control authentication refused".into());
    }
    let difference = envelope
        .token
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |value, (a, b)| value | std::hint::black_box(a ^ b));
    if difference != 0 {
        return Err("world control authentication refused".into());
    }
    // The reusable geography DTO accepts metadata; the command wire is stricter.
    let raw: Value = serde_json::from_slice(bytes).map_err(|_| "invalid world command")?;
    if let Some(request) = raw["command"].get("request") {
        let navigation_fields = [
            "domain",
            "sources",
            "imagery_date",
            "max_zoom",
            "max_visible_tiles",
            "max_requests",
            "max_bytes",
            "duration_seconds",
            "signals",
            "photogrammetry",
        ];
        let exploration_fields = [
            "latitude",
            "longitude",
            "radius_m",
            "zoom",
            "layers",
            "domain",
            "max_tiles",
            "max_bytes",
            "imagery_sources",
            "imagery_date",
            "signal_updates",
            "signal_interval_seconds",
        ];
        let known: &[&str] = if raw["command"]["action"] == "navigate" {
            &navigation_fields
        } else {
            &exploration_fields
        };
        if request
            .as_object()
            .is_none_or(|fields| fields.keys().any(|key| !known.contains(&key.as_str())))
        {
            return Err("unknown exploration request field".into());
        }
    }
    Ok(envelope.command)
}

async fn frame(stream: &mut TcpStream, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut reader = BufReader::new(stream.take((limit + 1) as u64));
    reader
        .read_until(b'\n', &mut bytes)
        .await
        .map_err(|_| "world control read failed")?;
    if bytes.len() > limit || bytes.last() != Some(&b'\n') {
        return Err("invalid or oversized world control frame".into());
    }
    bytes.pop();
    Ok(bytes)
}

async fn serve(mut stream: TcpStream, service: ExplorationService, token: Arc<String>) {
    let outcome = async {
        let bytes = tokio::time::timeout(Duration::from_secs(3), frame(&mut stream, REQUEST_LIMIT))
            .await
            .map_err(|_| "world command read timed out")??;
        // The viewport capability is distinct and cannot invoke owner commands.
        let raw: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid world command")?;
        if raw.get("session_id").is_some() {
            return service.navigation_message(&bytes).await;
        }
        let command = authenticate(&bytes, &token)?;
        tokio::time::timeout(COMMAND_TIMEOUT, service.execute(command))
            .await
            .map_err(|_| "world command timed out".to_string())?
    }
    .await;
    let reply = match outcome {
        Ok(value) => Reply {
            ok: true,
            result: Some(value),
            error: None,
        },
        Err(error) => Reply {
            ok: false,
            result: None,
            error: Some(error),
        },
    };
    let mut bytes = serde_json::to_vec(&reply).unwrap_or_default();
    if bytes.len() + 1 > REPLY_LIMIT {
        bytes =
            br#"{"ok":false,"result":null,"error":"world response exceeds size limit"}"#.to_vec();
    }
    bytes.push(b'\n');
    let _ = tokio::time::timeout(Duration::from_secs(3), stream.write_all(&bytes)).await;
}

/// Bind an ephemeral loopback endpoint and atomically publish its owner-only credential.
pub async fn run(
    service: ExplorationService,
    runtime_dir: PathBuf,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    private_directory(&runtime_dir)?;
    restrict(&runtime_dir, true)?;
    verify_private(&runtime_dir, true)?;
    let lock_path = runtime_dir.join("control.lock");
    reject_symlink(&lock_path)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options
        .open(&lock_path)
        .map_err(|_| "cannot open world control lock")?;
    restrict(&lock_path, false)?;
    verify_private(&lock_path, false)?;
    lock.try_lock_exclusive()
        .map_err(|_| "another world command service owns this runtime directory")?;
    crate::exploration_projection::ensure(&runtime_dir)?;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| "cannot bind private world control")?;
    let mut random = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut random);
    let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let descriptor = Descriptor {
        schema_version: 1,
        address: listener
            .local_addr()
            .map_err(|e| e.to_string())?
            .to_string(),
        token: token.clone(),
    };
    let descriptor_path = runtime_dir.join("control.json");
    service
        .configure_navigation(runtime_dir.clone(), descriptor.address.clone())
        .await?;
    reject_symlink(&descriptor_path)?;
    // Windows cannot rename over an existing destination; the held instance lock
    // guarantees stale descriptor removal cannot affect another running daemon.
    if descriptor_path.exists() {
        std::fs::remove_file(&descriptor_path).map_err(|e| e.to_string())?;
    }
    atomic_write(
        &descriptor_path,
        &serde_json::to_vec(&descriptor).map_err(|e| e.to_string())?,
    )?;
    restrict(&descriptor_path, false)?;
    verify_private(&descriptor_path, false)?;
    let token = Arc::new(token);
    let capacity = Arc::new(Semaphore::new(8));
    let mut clients = tokio::task::JoinSet::new();
    let result = loop {
        if *shutdown.borrow() {
            break Ok(());
        }
        tokio::select! {
            changed = shutdown.changed() => { if changed.is_err() || *shutdown.borrow() { break Ok(()); } }
            accepted = listener.accept() => {
                let (stream, peer) = match accepted { Ok(pair) => pair, Err(_) => break Err("world control accept failed".into()) };
                if !peer.ip().is_loopback() { continue; }
                let Ok(permit) = capacity.clone().try_acquire_owned() else { continue; };
                let service = service.clone(); let token = token.clone();
                clients.spawn(async move { let _permit = permit; serve(stream, service, token).await; });
            }
            _ = clients.join_next(), if !clients.is_empty() => {}
        }
    };
    clients.abort_all();
    service.shutdown_navigation().await;
    while clients.join_next().await.is_some() {}
    let _ = std::fs::remove_file(descriptor_path);
    drop(lock);
    result
}

/// Send only to the authenticated owner-private daemon endpoint, never to a URL.
pub async fn send(command: ExploreCommand) -> Result<Value, String> {
    send_in(&default_runtime_dir(), command).await
}

async fn send_in(directory: &Path, command: ExploreCommand) -> Result<Value, String> {
    verify_private(directory, true)?;
    let path = directory.join("control.json");
    verify_private(&path, false)?;
    if std::fs::metadata(&path)
        .map_err(|_| "world service is unavailable")?
        .len()
        > 4096
    {
        return Err("invalid world control descriptor size".into());
    }
    let bytes = std::fs::read(path).map_err(|_| "cannot read private world endpoint")?;
    let descriptor: Descriptor =
        serde_json::from_slice(&bytes).map_err(|_| "invalid world control descriptor")?;
    exchange(&descriptor, command).await
}

async fn exchange(descriptor: &Descriptor, command: ExploreCommand) -> Result<Value, String> {
    let address = validate_descriptor(descriptor)?;
    let mut bytes = serde_json::to_vec(&Envelope {
        token: descriptor.token.clone(),
        command,
    })
    .map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    if bytes.len() > REQUEST_LIMIT {
        return Err("world command exceeds size limit".into());
    }
    let mut stream = tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(address))
        .await
        .map_err(|_| "world service connection timed out")?
        .map_err(|_| "world service is unavailable")?;
    tokio::time::timeout(Duration::from_secs(3), stream.write_all(&bytes))
        .await
        .map_err(|_| "world command write timed out")?
        .map_err(|_| "world command write failed")?;
    let bytes = tokio::time::timeout(Duration::from_secs(90), frame(&mut stream, REPLY_LIMIT))
        .await
        .map_err(|_| "world command response timed out")??;
    let reply: Reply =
        serde_json::from_slice(&bytes).map_err(|_| "invalid world service response")?;
    if reply.ok {
        reply
            .result
            .ok_or_else(|| "world response omitted result".into())
    } else {
        Err(reply
            .error
            .unwrap_or_else(|| "world command refused".into()))
    }
}

fn reject_symlink(path: &Path) -> Result<(), String> {
    if std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("refusing world control symlink".into());
    }
    Ok(())
}

/// Restrict and verify an existing credential path using this user's native ACL.
/// Used for independent read-only projection credentials; never shares command tokens.
pub fn secure_owner_path(path: &Path, directory: bool) -> Result<(), String> {
    reject_symlink(path)?;
    restrict(path, directory)?;
    verify_private(path, directory)
}

#[cfg(unix)]
fn restrict(path: &Path, directory: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        path,
        std::fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
    )
    .map_err(|e| e.to_string())
}

#[cfg(unix)]
fn verify_private(path: &Path, directory: bool) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    reject_symlink(path)?;
    let metadata = std::fs::metadata(path).map_err(|_| "private world control path unavailable")?;
    // SAFETY: geteuid takes no pointers and has no preconditions.
    if metadata.uid() != unsafe { geteuid() }
        || metadata.permissions().mode() & 0o077 != 0
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err("world control path is not owner-private".into());
    }
    Ok(())
}

#[cfg(windows)]
fn restrict(path: &Path, directory: bool) -> Result<(), String> {
    windows_privacy::restrict(path, directory)
}
#[cfg(windows)]
fn verify_private(path: &Path, directory: bool) -> Result<(), String> {
    reject_symlink(path)?;
    let metadata = std::fs::metadata(path).map_err(|_| "private world control path unavailable")?;
    if (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
        return Err("invalid world control path type".into());
    }
    windows_privacy::verify(path)
}
