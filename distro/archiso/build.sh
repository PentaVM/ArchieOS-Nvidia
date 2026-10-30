#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/../.." && pwd)"
output_dir="${1:-${repo_root}/out}"
work_dir="${ARCHIE_ISO_WORK_DIR:-${repo_root}/build/archiso}"
cargo_target="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-${HOME}/.cache}/archieos/target}"
native_build="${ARCHIE_NATIVE_BUILD_DIR:-${XDG_CACHE_HOME:-${HOME}/.cache}/archieos/native-display}"
cortex_build="${ARCHIE_CORTEX_BUILD_DIR:-${XDG_CACHE_HOME:-${HOME}/.cache}/archieos/cortex}"
sentry_build="${ARCHIE_SENTRY_BUILD_DIR:-${XDG_CACHE_HOME:-${HOME}/.cache}/archieos/sentry}"

for command in cargo cmake ctest ninja mkarchiso install file sha256sum repo-add pacman-key curl; do
  command -v "${command}" >/dev/null || {
    echo "archieos-build: missing required command: ${command}" >&2
    exit 69
  }
done

cmake -S "${repo_root}/display/native" -B "${native_build}" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INTERPROCEDURAL_OPTIMIZATION=ON
cmake --build "${native_build}" --parallel 2
cmake -S "${repo_root}/display/installer" -B "${native_build}/installer" -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build "${native_build}/installer" --parallel 2
file "${native_build}/archie-display" | grep -q 'ELF 64-bit.*x86-64' || {
  echo "archieos-build: archie-display is not a Linux x86-64 ELF release binary" >&2
  exit 65
}

cmake -S "${repo_root}/cortex" -B "${cortex_build}" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INTERPROCEDURAL_OPTIMIZATION=ON \
  -DARCHIE_CORTEX_BUILD_TESTS=ON
cmake --build "${cortex_build}" --parallel 2
ctest --test-dir "${cortex_build}" --output-on-failure
file "${cortex_build}/archie-cortexd" | grep -q 'ELF 64-bit.*x86-64' || {
  echo "archieos-build: archie-cortexd is not a Linux x86-64 ELF release binary" >&2
  exit 65
}

# Sentry's contracts and the person graph. Tests run here rather than only in CI: these
# are the rules that decide who may be identified, and an image must not be built from a
# tree where they do not hold.
cmake -S "${repo_root}/sentry" -B "${sentry_build}" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INTERPROCEDURAL_OPTIMIZATION=ON \
  -DSENTRY_BUILD_TESTS=ON
cmake --build "${sentry_build}" --parallel 2
ctest --test-dir "${sentry_build}" --output-on-failure
file "${sentry_build}/archie-sentry" | grep -q 'ELF 64-bit.*x86-64' || {
  echo "archieos-build: archie-sentry is not a Linux x86-64 ELF release binary" >&2
  exit 65
}

if (( EUID != 0 )); then
  echo "archieos-build: mkarchiso requires root; run this script with sudo on Arch Linux" >&2
  exit 77
fi

export CARGO_TARGET_DIR="${cargo_target}"
cargo build --locked --release --manifest-path "${repo_root}/core/Cargo.toml" \
  -p archie-installer -p archie-state -p archied -p archie-cli -p archie-displayd -p archie-shell-bridge -p archie-worldd -p archie-executord -p archie-task-runner -p archie-model-runtime -p archie-rem -p archie-lan -p archie-sentry

for binary in archie-map-fetch archie-install archie archied archie-db archie-displayd archie-shell-bridge archie-worldd archie-executord archie-task-runner archie-infer archie-ask archie-rem archie-lan archie-netwatch archie-print; do
  file "${cargo_target}/release/${binary}" | grep -q 'ELF 64-bit.*x86-64' || {
    echo "archieos-build: ${binary} is not a Linux x86-64 ELF release binary" >&2
    exit 65
  }
done

stage="$(mktemp -d --tmpdir archieos-profile.XXXXXXXX)"
trap 'rm -rf -- "${stage}"' EXIT
install -d "${stage}/airootfs/usr/bin"
cp -a "${script_dir}/airootfs" "${stage}/"
cp "${script_dir}/profiledef.sh" "${script_dir}/packages.x86_64" \
  "${script_dir}/pacman.conf" "${stage}/"
cat "${repo_root}/distro/installer/packages.x86_64" >>"${stage}/packages.x86_64"
if [[ "${ARCHIE_NVIDIA_PROFILE:-0}" == "1" ]]; then
  cat "${repo_root}/distro/nvidia/packages.x86_64" >>"${stage}/packages.x86_64"
fi
bash "${repo_root}/distro/database/stage-reviewed-packages.sh" "${stage}/reviewed-repo"
# Prefer the reviewed, signed extension packages over rolling-repository updates.
awk -v repository="${stage}/reviewed-repo" '
  /^\[core\]/ { print "[archie-reviewed]\nServer = file://" repository "\n" }
  { print }
' "${stage}/pacman.conf" >"${stage}/pacman.conf.reviewed"
mv -- "${stage}/pacman.conf.reviewed" "${stage}/pacman.conf"
cp -a "${script_dir}/efiboot" "${script_dir}/syslinux" "${stage}/"
install -Dm755 "${cargo_target}/release/archie" "${stage}/airootfs/usr/bin/archie"
install -Dm755 "${cargo_target}/release/archied" "${stage}/airootfs/usr/bin/archied"
install -Dm755 "${cargo_target}/release/archie-db" "${stage}/airootfs/usr/bin/archie-db"
install -Dm644 "${repo_root}/distro/database/pacman.conf" "${stage}/airootfs/etc/pacman.conf"
for unit in archie-postgres-init archie-postgres archie-database; do
  install -Dm644 "${repo_root}/distro/database/${unit}.service" "${stage}/airootfs/etc/systemd/system/${unit}.service"
done
for helper in initialize-cluster provision-node wait-postgres; do
  install -Dm755 "${repo_root}/distro/database/${helper}.sh" "${stage}/airootfs/usr/lib/archie/${helper}"
done
# The execution supervisor. A separate binary and a separate account because it holds the
# privilege archied must never acquire: creating cgroups and spawning workers.
install -Dm755 "${cargo_target}/release/archie-executord" \
  "${stage}/airootfs/usr/bin/archie-executord"
# The per-task sandbox launcher. Executed by the supervisor, once per task, and
# never by a person: it applies an irreversible Landlock ruleset and then execs.
install -Dm755 "${cargo_target}/release/archie-task-runner" \
  "${stage}/airootfs/usr/bin/archie-task-runner"
install -Dm755 "${cargo_target}/release/archie-displayd" \
  "${stage}/airootfs/usr/bin/archie-displayd"
install -Dm755 "${cargo_target}/release/archie-worldd" \
  "${stage}/airootfs/usr/bin/archie-worldd"
install -Dm755 "${cargo_target}/release/archie-install" "${stage}/airootfs/usr/bin/archie-install"
install -Dm755 "${native_build}/installer/archie-wizard" "${stage}/airootfs/usr/bin/archie-wizard"
install -Dm755 "${repo_root}/distro/installer/wizard-session.sh" "${stage}/airootfs/usr/bin/archie-wizard-session"
install -Dm755 "${repo_root}/distro/installer/finalize-target.sh" "${stage}/airootfs/usr/lib/archie/finalize-install-target"
install -Dm644 "${repo_root}/distro/installer/packages.x86_64" "${stage}/airootfs/usr/share/archie/installer-packages"
install -Dm755 "${cargo_target}/release/archie-map-fetch" "${stage}/airootfs/usr/bin/archie-map-fetch"
install -Dm755 "${cargo_target}/release/archie-infer" "${stage}/airootfs/usr/bin/archie-infer"
install -Dm755 "${cargo_target}/release/archie-ask" "${stage}/airootfs/usr/bin/archie-ask"
# Sentry's LAN inventory runs as the module worker modules/sentry/lan.toml names.
install -Dm755 "${cargo_target}/release/archie-lan" "${stage}/airootfs/usr/bin/archie-lan"
# The traffic audit. Installed and not enabled: it starts once the owner has named the
# interface in /etc/archie/netwatch.env. TShark comes from wireshark-cli.
install -Dm755 "${cargo_target}/release/archie-netwatch" "${stage}/airootfs/usr/bin/archie-netwatch"
install -Dm755 "${cargo_target}/release/archie-print" "${stage}/airootfs/usr/bin/archie-print"
install -Dm644 "${repo_root}/distro/sentry/archie-netwatch.service" "${stage}/airootfs/etc/systemd/system/archie-netwatch.service"
install -Dm644 "${repo_root}/distro/sentry/archie-netwatch.sysusers" "${stage}/airootfs/usr/lib/sysusers.d/archie-netwatch.conf"
install -Dm644 "${repo_root}/distro/sentry/archie-sentry-follow.service" "${stage}/airootfs/etc/systemd/system/archie-sentry-follow.service"
# REM's supervisor. The unit is installed and not enabled: with no jobs registered and
# no trainer it would have nothing to pause, and recording is the owner's to turn on.
install -Dm755 "${cargo_target}/release/archie-rem" "${stage}/airootfs/usr/bin/archie-rem"
install -Dm644 "${repo_root}/distro/rem/rem.toml" "${stage}/airootfs/etc/archie/rem.toml"
install -Dm644 "${repo_root}/distro/rem/archie-rem.service" "${stage}/airootfs/etc/systemd/system/archie-rem.service"
install -Dm644 "${repo_root}/distro/installer/archie-wizard.desktop" "${stage}/airootfs/usr/share/applications/archie-wizard.desktop"
install -Dm644 "${repo_root}/distro/installer/archie-installer.service" "${stage}/airootfs/etc/systemd/system/archie-installer.service"
install -Dm755 "${cargo_target}/release/archie-shell-bridge" \
  "${stage}/airootfs/usr/bin/archie-shell-bridge"
install -Dm755 "${native_build}/archie-display" \
  "${stage}/airootfs/usr/bin/archie-display"
# The ArchieOS mark (logo package v3.5, design by Xavi): the program's icon in menus,
# switchers and the session, under the app id archie-display gives Wayland (the stage policy names it too).
install -Dm644 "${repo_root}/display/shell/brand/archieos-app-512.png" \
  "${stage}/airootfs/usr/share/icons/hicolor/512x512/apps/archieos.png"
install -Dm644 "${repo_root}/display/shell/brand/archieos-icon-256.png" \
  "${stage}/airootfs/usr/share/icons/hicolor/256x256/apps/archieos.png"
install -Dm644 "${repo_root}/distro/display/archie-display.desktop" \
  "${stage}/airootfs/usr/share/applications/archie-display.desktop"
install -Dm755 "${cortex_build}/archie-cortexd" \
  "${stage}/airootfs/usr/bin/archie-cortexd"
# The person graph. A tool the owner runs, not a daemon: nothing about the relationship
# graph should happen without someone asking for it, so there is no service to enable.
install -Dm755 "${sentry_build}/archie-sentry" \
  "${stage}/airootfs/usr/bin/archie-sentry"
install -Dm644 "${repo_root}/docs/RFC-008-SENTRY-WORLD-MODEL.md" \
  "${stage}/airootfs/usr/share/doc/archie-os/RFC-008-SENTRY-WORLD-MODEL.md"

# Install the observatory as a separate, fail-open graphical stack. archied remains a
# multi-user service and has no ordering or requirement edge back to either display
# unit: a missing GPU or native-renderer crash cannot stop the control plane. QML,
# shaders and the satellite texture are compiled into the native ELF; no mutable web
# root or network-fetched visual asset is shipped.
install -Dm644 "${repo_root}/distro/display/archie-displayd.service" \
  "${stage}/airootfs/etc/systemd/system/archie-displayd.service"
install -Dm644 "${repo_root}/distro/display/archie-worldd.service" \
  "${stage}/airootfs/etc/systemd/system/archie-worldd.service"
install -Dm644 "${repo_root}/distro/display/archie-display.service" \
  "${stage}/airootfs/etc/systemd/system/archie-display.service"
install -Dm644 "${repo_root}/distro/display/archie-display.sysusers" \
  "${stage}/airootfs/usr/lib/sysusers.d/archie-display.conf"
install -Dm644 "${repo_root}/docs/RFC-006-DISPLAY-ISOLATION.md" \
  "${stage}/airootfs/usr/share/doc/archie-os/RFC-006-DISPLAY-ISOLATION.md"

# Cortex is a separate unprivileged analytical process. Its systemd socket owns the
# sole local IPC endpoint; the daemon has neither public egress nor control authority.
install -Dm644 "${repo_root}/distro/cortex/archie-cortex.service" \
  "${stage}/airootfs/etc/systemd/system/archie-cortex.service"
install -Dm644 "${repo_root}/distro/cortex/archie-cortex.socket" \
  "${stage}/airootfs/etc/systemd/system/archie-cortex.socket"
install -Dm644 "${repo_root}/distro/cortex/archie-cortex.sysusers" \
  "${stage}/airootfs/usr/lib/sysusers.d/archie-cortex.conf"
install -Dm644 "${repo_root}/distro/cortex/archie-cortex.tmpfiles" \
  "${stage}/airootfs/usr/lib/tmpfiles.d/archie-cortex.conf"
install -Dm644 "${repo_root}/docs/RFC-009-ANALYTICAL-CORTEX.md" \
  "${stage}/airootfs/usr/share/doc/archie-os/RFC-009-ANALYTICAL-CORTEX.md"

# Ship the module manifests so the image boots with a real cognitive topology rather
# than an empty one. They are declarative, contain no secrets, and are installed
# root-owned and world-readable: `archied` reads them and must not be able to edit them.
# /etc/archie is 0750 root:archie (tmpfiles.d), so group `archie` can traverse into this
# directory; the manifests themselves are 0644 and need no group of their own.
install -d "${stage}/airootfs/etc/archie/modules"
while IFS= read -r -d "" manifest; do
  family="$(basename "$(dirname "${manifest}")")"
  install -Dm644 "${manifest}"     "${stage}/airootfs/etc/archie/modules/${family}/$(basename "${manifest}")"
done < <(find "${repo_root}/modules" -name "*.toml" -type f -print0)

# Provider manifests are configuration contracts, not credentials. Stage the same
# registry that archied resolves during development so installed nodes do not silently
# boot with an empty provider catalog.
install -d "${stage}/airootfs/etc/archie/providers"
while IFS= read -r -d "" manifest; do
  install -Dm644 "${manifest}" \
    "${stage}/airootfs/etc/archie/providers/$(basename "${manifest}")"
done < <(find "${repo_root}/providers/data" -name "*.toml" -type f -print0)

# Model manifests say which models the node has and what each may see; the router reads
# them at boot. Only the manifests are staged. The weights they name are not in the image:
# they are fetched at first run and checked against the manifest's hash.
install -d "${stage}/airootfs/etc/archie/models"
while IFS= read -r -d "" manifest; do
  family="$(basename "$(dirname "${manifest}")")"
  install -Dm644 "${manifest}" \
    "${stage}/airootfs/etc/archie/models/${family}/$(basename "${manifest}")"
done < <(find "${repo_root}/models" -name "*.toml" -type f -print0)

install -d "${stage}/airootfs/etc/systemd/system/multi-user.target.wants"
install -d "${stage}/airootfs/etc/systemd/system/network-online.target.wants"
install -d "${stage}/airootfs/etc/systemd/system/graphical.target.wants"
install -d "${stage}/airootfs/etc/systemd/system/sockets.target.wants"
ln -s ../archie-installer.service "${stage}/airootfs/etc/systemd/system/graphical.target.wants/archie-installer.service"
# ArchieOS ships a complete non-interactive live configuration. A pristine Arch root
# otherwise starts systemd-firstboot on the serial console and blocks the daemon behind
# a timezone prompt, which is both unsuitable for an appliance and invisible headless.
ln -s /dev/null "${stage}/airootfs/etc/systemd/system/systemd-firstboot.service"
ln -s ../archied.service \
  "${stage}/airootfs/etc/systemd/system/multi-user.target.wants/archied.service"
ln -s ../archie-worldd.service \
  "${stage}/airootfs/etc/systemd/system/multi-user.target.wants/archie-worldd.service"
ln -s ../archie-cortex.socket \
  "${stage}/airootfs/etc/systemd/system/sockets.target.wants/archie-cortex.socket"
ln -s ../archie-displayd.service \
  "${stage}/airootfs/etc/systemd/system/multi-user.target.wants/archie-displayd.service"
ln -s ../archie-display.service \
  "${stage}/airootfs/etc/systemd/system/graphical.target.wants/archie-display.service"
# The image is an observatory appliance when a DRM device is present. graphical.target
# still pulls in multi-user.target first, and archie-display.service is only Wanted, so
# its failure never makes archied or the boot target fail.
ln -s /usr/lib/systemd/system/graphical.target \
  "${stage}/airootfs/etc/systemd/system/default.target"
ln -s /usr/lib/systemd/system/systemd-networkd.service \
  "${stage}/airootfs/etc/systemd/system/multi-user.target.wants/systemd-networkd.service"
ln -s /usr/lib/systemd/system/systemd-resolved.service \
  "${stage}/airootfs/etc/systemd/system/multi-user.target.wants/systemd-resolved.service"
ln -s /usr/lib/systemd/system/systemd-networkd-wait-online.service \
  "${stage}/airootfs/etc/systemd/system/network-online.target.wants/systemd-networkd-wait-online.service"
ln -s /run/systemd/resolve/stub-resolv.conf "${stage}/airootfs/etc/resolv.conf"

for forbidden in .git target _legacy tests screenshots secrets; do
  if find "${stage}/airootfs" -name "${forbidden}" -print -quit | grep -q .; then
    echo "archieos-build: forbidden release content found: ${forbidden}" >&2
    exit 66
  fi
done
if find "${stage}/airootfs/usr/bin" -type f \
  ! -name archie ! -name archied ! -name archie-db ! -name archie-displayd ! -name archie-worldd ! -name archie-cortexd \
  ! -name archie-display ! -name archie-shell-bridge ! -name archie-executord ! -name archie-task-runner \
  ! -name archie-sentry ! -name archie-install ! -name archie-wizard ! -name archie-map-fetch ! -name archie-infer ! -name archie-ask ! -name archie-rem ! -name archie-lan ! -name archie-netwatch ! -name archie-print ! -name archie-wizard-session \
  -print -quit | grep -q .; then
  echo "archieos-build: unexpected staged Archie binary" >&2
  exit 66
fi

install -d "${output_dir}" "${work_dir}"
mkarchiso -v -w "${work_dir}" -o "${output_dir}" "${stage}"
sha256sum "${output_dir}"/archieos-*.iso >"${output_dir}/SHA256SUMS"
echo "archieos-build: ISO and SHA256SUMS written to ${output_dir}"
