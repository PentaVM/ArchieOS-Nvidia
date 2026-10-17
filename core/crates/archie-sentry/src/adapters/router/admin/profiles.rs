use crate::engine::AdapterError;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// A command dialect, not certification of every device sold under a brand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Firmware {
    Openwrt,
    Asuswrt,
    Freshtomato,
    Ddwrt,
    Vyos,
    Opnsense,
    Pfsense,
    Routeros,
    Linux,
}
impl Firmware {
    pub const ALL: &'static [Self] = &[
        Self::Openwrt,
        Self::Asuswrt,
        Self::Freshtomato,
        Self::Ddwrt,
        Self::Vyos,
        Self::Opnsense,
        Self::Pfsense,
        Self::Routeros,
        Self::Linux,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Openwrt => "openwrt",
            Self::Asuswrt => "asuswrt",
            Self::Freshtomato => "freshtomato",
            Self::Ddwrt => "ddwrt",
            Self::Vyos => "vyos",
            Self::Opnsense => "opnsense",
            Self::Pfsense => "pfsense",
            Self::Routeros => "routeros",
            Self::Linux => "linux",
        }
    }
    pub fn enrollment_help(self) -> &'static str {
        match self {
        Self::Openwrt => "Enable LAN SSH and enroll an owner key in LuCI System/Administration/SSH-Keys. Root access is checked, never inferred from Dropbear.",
        Self::Asuswrt => "On a firmware exposing SSH, enable LAN-only SSH and enroll the owner's authorized key in Administration/System. Stock firmware availability varies.",
        Self::Freshtomato => "Enable SSH Daemon and enroll the owner key in Administration/Admin Access. Do not enable WAN SSH for local management.",
        Self::Ddwrt => "Enable Secure Shell and enroll the owner key in Services. Firmware/build availability varies.",
        Self::Vyos => "Configure service ssh and an owner's login public key. Existing sudo is checked only with explicit sudo-read opt-in.",
        Self::Opnsense => "Enable SSH under System/Settings/Administration and enroll the owner's public key in its user account. API permissions and SSH rights are distinct.",
        Self::Pfsense => "Enable Secure Shell and enroll the owner's key with the required shell privilege in User Manager. Shell privilege is not inferred from GUI access.",
        Self::Routeros => "Enable the LAN management SSH service and import an owner public key for its account. Effective group policies are queried; RouterOS does not provide POSIX root.",
        Self::Linux => "Enroll the owner key through the router's supported administrative procedure. Existing root/sudo rights are measured, not granted.",
    }
    }
}
impl FromStr for Firmware {
    type Err = AdapterError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .find(|p| p.as_str() == value)
            .copied()
            .ok_or(AdapterError::Unsupported)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReadKind {
    Identity,
    Interfaces,
    Routes,
    Neighbors,
    Leases,
    Connections,
    System,
    Wan,
}
impl ReadKind {
    pub const ALL: &'static [Self] = &[
        Self::Identity,
        Self::Interfaces,
        Self::Routes,
        Self::Neighbors,
        Self::Leases,
        Self::Connections,
        Self::System,
        Self::Wan,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Interfaces => "interfaces",
            Self::Routes => "routes",
            Self::Neighbors => "neighbors",
            Self::Leases => "leases",
            Self::Connections => "connections",
            Self::System => "system",
            Self::Wan => "wan",
        }
    }
}
impl FromStr for ReadKind {
    type Err = AdapterError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .find(|p| p.as_str() == value)
            .copied()
            .ok_or(AdapterError::Unsupported)
    }
}

/// Only named reads, never an arbitrary command supplied by the caller/model.
pub(super) fn command(firmware: Firmware, kind: ReadKind) -> Option<&'static str> {
    use Firmware::*;
    use ReadKind::*;
    if firmware == Routeros {
        return Some(match kind {
            Identity => "/system resource print",
            Interfaces => "/interface print detail without-paging",
            Routes => "/ip route print detail without-paging",
            Neighbors => "/ip arp print detail without-paging",
            Leases => "/ip dhcp-server lease print detail without-paging",
            Connections => "/ip firewall connection print detail without-paging",
            System => "/system resource print",
            Wan => return None,
        });
    }
    if matches!(firmware, Opnsense | Pfsense) {
        return Some(match kind {
            Identity => {
                if firmware == Opnsense {
                    "opnsense-version -a"
                } else {
                    "cat /etc/version"
                }
            }
            Interfaces => "ifconfig -a",
            Routes => "netstat -rn",
            Neighbors => "arp -an",
            System => "uptime",
            Connections => "pfctl -ss",
            Leases | Wan => return None,
        });
    }
    Some(match kind {
        Identity if firmware == Openwrt => "ubus call system board",
        Identity => "uname -a",
        Interfaces => "ip -s link",
        Routes => "ip route show",
        Neighbors => "ip neigh show",
        Leases => match firmware {
            Openwrt => "cat /tmp/dhcp.leases",
            Asuswrt => "cat /var/lib/misc/dnsmasq.leases",
            Ddwrt => "cat /tmp/dnsmasq.leases",
            Freshtomato => "cat /var/lib/misc/dnsmasq.leases",
            _ => return None,
        },
        Connections => "head -n 128 /proc/net/nf_conntrack",
        System if firmware == Openwrt => "ubus call system info",
        System => "uptime",
        Wan if firmware == Openwrt => "ubus call network.interface.wan status",
        Wan => return None,
    })
}

pub(super) const FINGERPRINT: &str = "sh -c 'for f in /etc/os-release /etc/openwrt_release /etc/banner /etc/platform; do if test -r \"$f\"; then head -c 1024 \"$f\"; echo; fi; done; uname -s'";

pub(super) fn identify(text: &str) -> Result<Option<Firmware>, AdapterError> {
    let mut matches = Vec::new();
    for line in text.lines().map(str::trim) {
        let lower = line.to_ascii_lowercase();
        let id = lower
            .strip_prefix("id=")
            .or_else(|| lower.strip_prefix("distrib_id="))
            .map(|s| s.trim_matches(['\'', '"']));
        let found = match id {
            Some("openwrt") => Some(Firmware::Openwrt),
            Some("vyos") => Some(Firmware::Vyos),
            _ => None,
        }
        .or_else(|| {
            if lower.starts_with("asuswrt") {
                Some(Firmware::Asuswrt)
            } else if lower.starts_with("freshtomato") {
                Some(Firmware::Freshtomato)
            } else if lower.starts_with("dd-wrt") {
                Some(Firmware::Ddwrt)
            } else if lower.starts_with("opnsense ") {
                Some(Firmware::Opnsense)
            } else if lower == "pfsense" {
                Some(Firmware::Pfsense)
            } else {
                None
            }
        });
        if let Some(found) = found {
            if !matches.contains(&found) {
                matches.push(found)
            }
        }
    }
    if matches.len() > 1 {
        return Err(AdapterError::Protocol);
    }
    Ok(if matches.len() == 1 {
        matches.first().copied()
    } else if text.lines().any(|s| s.trim() == "Linux") {
        Some(Firmware::Linux)
    } else {
        None
    })
}
