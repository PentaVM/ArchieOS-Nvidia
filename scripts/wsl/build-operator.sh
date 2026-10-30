#!/usr/bin/env bash
# Builds ArchieOS-Operator.wsl: the image a person imports into WSL to have Archie operate
# a screen. In it: the Stage, a browser to work in, IRIS (the Rust runtime, its weights and
# the text tower it reads phrases with), the reasoning models' manifests, and Sentry's
# network tools. Not in it: Python for IRIS, PyTorch, or any key. IRIS starts without
# downloading anything.
#
#   IRIS_WEIGHTS=/path/to/iris-beta21-step132000.pt IRIS_LICENSE=/path/to/archie-iris/LICENSE \
#   IRIS_TEXT=/path/to/siglip2-text      (archie-iris-text-pack's output) \
#   IRIS_CUDA_BIN=/path/to/archie-iris-serve built with --features cuda   (optional) \
#   STAGE=/path/to/archie-stage BIN=/path/to/target/release \
#     scripts/wsl/build-operator.sh [OUT_DIR | -]
#
# The image is larger than one GitHub release asset (2 GiB), so OUT_DIR receives it in
# parts (ArchieOS-Operator.wsl.001, .002 ...) with the SHA-256 of the whole; the installer
# joins them.
#
# With `-` the image is written to standard output and nothing else is: for a builder
# with no room to hold the image beside the tree it is made from.
# Run as root on Arch Linux with arch-install-scripts.
set -euo pipefail
repo_root=$(realpath "$(dirname "$0")/../..")
out=${1:-$repo_root/dist}
exec 3>&1 1>&2
# IRIS Beta 2.1, step 132,000: the release's weights file, checked before it goes in.
iris_weights=${IRIS_WEIGHTS:?IRIS_WEIGHTS is iris-beta21-step132000.pt}
iris_weights_sha=c6cadb02f5a5bffc4ea3c3348d32b475ef7531f643675c06c65451037e4f3b97
# SigLIP2's text tower (google/siglip2-base-patch16-384 at f775b65, Apache-2.0), cut by
# archie-iris-text-pack from the repository's model.safetensors, whose digest this is.
iris_text=${IRIS_TEXT:?IRIS_TEXT is archie-iris-text-pack output}
iris_text_source_sha=ed72c0ace85020ae610fc817c2538b9cae5a477b012a50859c60af5b3ad30857
# The licence and model card of IRIS's weights (from the archie-iris release): they travel
# with the weights.
iris_license=${IRIS_LICENSE:?IRIS_LICENSE is the LICENSE of the archie-iris release}
iris_card=${IRIS_MODEL_CARD:-}
# The CUDA build of archie-iris-serve. Without it the image carries the CPU build only.
iris_cuda_bin=${IRIS_CUDA_BIN:-}
stage=${STAGE:?STAGE is the built archie-stage}
# NeMo-Speech.cpp's SDK (scripts/models/install-nemotron-speech.sh builds it). archie-voice
# must have been built with NEMO_SPEECH_PREFIX=/opt/archie/nemo-speech, where it goes here.
nemo_sdk=${NEMO_SDK:?NEMO_SDK is the installed NeMo-Speech.cpp SDK}
# PrismML's llama.cpp, built for the CPU: bin/llama-server and the libraries beside it.
# It runs Bonsai, the optional reasoning model on the machine (archie-local).
llama=${LLAMA:?LLAMA is the directory holding the built llama-server and its libraries}
bin=${BIN:?BIN is the cargo release directory}
# Reuse the ISO/native build by default; this script packages, never starts the GUI.
native_display=${ARCHIE_DISPLAY_BIN:-${ARCHIE_NATIVE_BUILD_DIR:-${XDG_CACHE_HOME:-${HOME}/.cache}/archieos/native-display}/archie-display}
[[ -x "$native_display" ]] || { echo "build-operator: build display/native first (or set ARCHIE_DISPLAY_BIN)" >&2; exit 69; }
# ARCHIE_WSL_WORK_DIR: where the root is assembled, when /var/tmp has no room for 5 GB.
work_dir=${ARCHIE_WSL_WORK_DIR:-/var/tmp/archieos-wsl-operator}
rootfs=$work_dir/rootfs

[[ $EUID -eq 0 ]] || { echo 'run as root on Arch Linux' >&2; exit 1; }
command -v pacstrap >/dev/null
rm -rf --one-file-system "${work_dir:?}"
install -d "$rootfs"

# Qt for the Stage, Mesa's software GL (WSLg shares no GPU buffers with a nested
# compositor), and a font so a page has letters. No TShark: a WSL machine sees only its
# own virtual network, so archie-netwatch has nothing of the house to listen to here.
# ARCHIE_PACMAN_CONF: another pacman.conf for the build, for instance one whose
# XferCommand limits the download rate on a connection that is being used.
conf=()
[[ -n ${ARCHIE_PACMAN_CONF:-} ]] && conf=(-C "$ARCHIE_PACMAN_CONF")
# Packages are downloaded into the root being built and removed after each step, so the
# build needs room in one place only and never holds every archive at once.
# libpulse is the microphone and the speaker, through WSLg; dbus and at-spi2-core let
# the voice read what an application shows.
pacstrap "${conf[@]}" -K "$rootfs" base ca-certificates curl jq python firefox mesa \
  qt6-base qt6-declarative qt6-quick3d qt6-wayland ttf-dejavu libpulse dbus at-spi2-core
rm -rf "${rootfs:?}/var/cache/pacman/pkg"/*
# MLT and FFmpeg for the video bridge, which needs no window, and Kdenlive to edit in the
# Stage. ARCHIE_OPERATOR_APPS= (empty) leaves Kdenlive out, about 1.5 GB lighter; it is
# then one pacman command away, which archie-operate says when asked for it.
read -r -a apps <<<"${ARCHIE_OPERATOR_APPS-kdenlive}"
pacstrap "${conf[@]}" "$rootfs" mlt ffmpeg "${apps[@]}"
rm -rf "${rootfs:?}/var/cache/pacman/pkg"/*

rm -f "$rootfs/etc/resolv.conf" "$rootfs/boot"/vmlinuz-* "$rootfs/boot"/initramfs-*
cp -a "$repo_root/distro/wsl-operator/rootfs/." "$rootfs/"
chmod 0755 "$rootfs"/usr/bin/archie-{setup,session,operate,talk,video,design,local}
chmod 0755 "$rootfs/usr/lib/archie/init-pacman-keyring"
# The ArchieOS mark, transparent: the distro's own icon in Windows (Start, Terminal), and the
# icon WSLg's taskbar and title bars give the desktop's and the stage's windows (by app id).
brand=$repo_root/display/shell/brand
install -Dm644 "$brand/archieos.ico" "$rootfs/usr/lib/wsl/archieos.ico"
install -Dm644 "$brand/archieos-app-512.png" "$rootfs/usr/share/icons/hicolor/512x512/apps/archieos.png"
install -Dm644 "$brand/archieos-icon-256.png" "$rootfs/usr/share/icons/hicolor/256x256/apps/archieos.png"
install -Dm644 "$repo_root/distro/display/org.archieos.Stage.desktop" \
  "$rootfs/usr/share/applications/org.archieos.Stage.desktop"
# archie-offline is the menu's way into the desktop; this entry only names its window.
sed 's/^NoDisplay=false$/NoDisplay=true/' "$repo_root/distro/display/archie-display.desktop" \
  > "$rootfs/usr/share/applications/archie-display.desktop"

install -m 0755 "$stage" "$rootfs/usr/bin/archie-stage"
# archie-bridges and archie-mcp-server serve the video bridge: timelines rendered by MLT
# without a window, each with a project Kdenlive opens.
for binary in archie-arena-live archie-stage-cli archie-ask archie-lan archie-netwatch archie-print archie-sentry-probe \
  archie-bridges archie-mcp-server archie-voice archie archie-displayd archie-worldd archie-shell-bridge; do
  install -m 0755 "$bin/$binary" "$rootfs/usr/bin/$binary"
done
install -m 0755 "$native_display" "$rootfs/usr/bin/archie-display"
install -Dm 0755 "$repo_root/display/shell/start-native.sh" "$rootfs/usr/lib/archie/start-native.sh"
install -Dm 0755 "$llama/llama-server" "$rootfs/opt/archie/llama/bin/llama-server"
install -d "$rootfs/opt/archie/llama/lib"
cp -a "$llama"/*.so* "$rootfs/opt/archie/llama/lib/"

# The voice: the recogniser's runtime, the launcher, Piper's adapter and the script that
# fetches its two voices. The models themselves arrive with archie-setup.
install -d "$rootfs/opt/archie/nemo-speech"
cp -a "$nemo_sdk/lib" "$rootfs/opt/archie/nemo-speech/lib"
cp -a "$nemo_sdk/share" "$rootfs/opt/archie/nemo-speech/share" 2>/dev/null || true
install -Dm 0755 "$repo_root/display/stage/run-archie.sh" "$rootfs/usr/lib/archie/run-archie.sh"
install -Dm 0755 "$repo_root/scripts/models/piper-json-bridge.py" "$rootfs/usr/lib/archie/piper-json-bridge.py"
install -Dm 0755 "$repo_root/scripts/models/install-piper-voices.sh" "$rootfs/usr/lib/archie/install-piper-voices"
install -Dm 0644 "$repo_root/core/crates/archie-stage/policy/apps.toml" "$rootfs/usr/share/archie/stage/apps.toml"
# Explicit --preview retains the original design boards. Default archie-design opens
# the native renderer and its read-only bridge, reporting unavailable sources honestly.
install -d "$rootfs/usr/share/archie/shell-prototype"
cp -a "$repo_root/display/shell/prototype/." "$rootfs/usr/share/archie/shell-prototype/"

# Which models the node has: the reasoning models reached through Nebius, and IRIS.
while IFS= read -r -d '' manifest; do
  family=$(basename "$(dirname "$manifest")")
  install -Dm 0644 "$manifest" "$rootfs/etc/archie/models/$family/$(basename "$manifest")"
done < <(find "$repo_root/models/nebius" "$repo_root/models/iris" -maxdepth 1 -name '*.toml' -type f -print0)
install -Dm 0644 "$repo_root/ARCHIE.md" "$rootfs/usr/share/doc/archie-os/ARCHIE.md"
install -Dm 0644 "$repo_root/bridges/video/bridge.toml" "$rootfs/usr/share/archie/bridges/video/bridge.toml"
install -Dm 0644 "$repo_root/bridges/video/SKILL.md" "$rootfs/usr/share/archie/bridges/video/SKILL.md"
install -Dm 0644 "$repo_root/docs/WSL-OPERATOR.md" "$rootfs/usr/share/doc/archie-os/WSL-OPERATOR.md"

# IRIS: the Rust runtime, its weights and its text tower. No Python, no download at start.
[[ $(sha256sum "$iris_weights" | cut -d' ' -f1) == "$iris_weights_sha" ]] || {
  echo "build-operator: $iris_weights is not IRIS Beta 2.1 step 132000" >&2; exit 65; }
grep -qx "source sha256 $iris_text_source_sha" "$iris_text/SOURCE" || {
  echo "build-operator: $iris_text was not cut from SigLIP2 base-patch16-384 at f775b65" >&2; exit 65; }
models=$rootfs/var/lib/archie/models/iris
install -Dm 0644 "$iris_weights" "$models/iris-beta21-step132000.pt"
install -m 0644 "$iris_license" "$models/LICENSE"
[[ -n $iris_card ]] && install -m 0644 "$iris_card" "$models/MODEL_CARD.md"
install -m 0644 "$repo_root/docs/IRIS-LICENSES.md" "$models/LICENSES.md"
install -d "$models/siglip2-text"
install -m 0644 "$iris_text"/{model.safetensors,tokenizer.json,tokenizer_config.json,SOURCE} "$models/siglip2-text/"
for optional in special_tokens_map.json config.json; do
  [[ -f $iris_text/$optional ]] && install -m 0644 "$iris_text/$optional" "$models/siglip2-text/"
done
install -m 0755 "$bin/archie-iris-serve" "$rootfs/usr/bin/archie-iris-serve"
if [[ -n $iris_cuda_bin ]]; then
  install -m 0755 "$iris_cuda_bin" "$rootfs/usr/bin/archie-iris-serve-cuda"
  # cuBLAS, cuBLASLt, cuRAND and NVRTC, as linked; libcuda comes from the Windows driver
  # through /usr/lib/wsl/lib and is never copied.
  install -d "$rootfs/opt/archie/cuda/lib"
  ldd "$iris_cuda_bin" | awk '/=> \// {print $3}' | grep -E '/lib(cublas|cublasLt|curand|nvrtc)' | while read -r lib; do
    cp -L "$lib" "$rootfs/opt/archie/cuda/lib/"
    for builtin in "$(dirname "$lib")"/libnvrtc-builtins.so*; do
      [[ -e $builtin ]] && cp -L "$builtin" "$rootfs/opt/archie/cuda/lib/"
    done
  done
fi

arch-chroot "$rootfs" useradd --uid 1000 --create-home --shell /bin/bash archie
arch-chroot "$rootfs" passwd -l archie
install -d -o 1000 -g 1000 "$rootfs/var/lib/archie/models/asr" \
  "$rootfs/var/lib/archie/models/piper" "$rootfs/var/lib/archie/models/local" "$rootfs/opt/archie/voice-venv"
printf '%s\n' 'ArchieOS Operator. First time: archie-setup. Then: archie-talk, archie-operate, archie-video, archie-design.' > "$rootfs/etc/motd"

arch-chroot "$rootfs" pacman -Q > "$rootfs/package-versions.txt"
rm -rf "$rootfs/var/cache/pacman/pkg"/* "$rootfs/var/log"/* "$rootfs/usr/share/doc/qt6" "$rootfs/usr/share/gtk-doc"
: > "$rootfs/etc/machine-id"

# Every binary finds its libraries inside the image, or the image is not shipped.
for binary in archie-stage archie-arena-live archie-stage-cli archie-ask archie-lan archie-netwatch archie-print \
  archie-bridges archie-mcp-server archie-voice archie-iris-serve archie archie-display archie-displayd archie-worldd archie-shell-bridge; do
  if arch-chroot "$rootfs" ldd "/usr/bin/$binary" | grep -q 'not found'; then
    echo "build-operator: $binary lacks a library in the image" >&2
    arch-chroot "$rootfs" ldd "/usr/bin/$binary" | grep 'not found' >&2
    exit 65
  fi
done

if [[ -n $iris_cuda_bin ]]; then
  # Only the driver may be missing: WSL mounts it at /usr/lib/wsl/lib when it starts.
  missing=$(arch-chroot "$rootfs" env LD_LIBRARY_PATH=/opt/archie/cuda/lib ldd /usr/bin/archie-iris-serve-cuda \
    | grep 'not found' | grep -v 'libcuda\.so' || true)
  [[ -z $missing ]] || { echo "build-operator: archie-iris-serve-cuda lacks: $missing" >&2; exit 65; }
fi

du -sh "$rootfs"
# Final image packaging.
python3 "$repo_root/scripts/wsl/image-keyring.py" prepare "$rootfs"
if [[ $out == - ]]; then
  tar --numeric-owner --xattrs --acls -C "$rootfs" -cf - . | gzip -n -6 >&3
  exit 0
fi
out_dir=$(realpath -m "$out")
install -d "$out_dir"
rm -f "$out_dir"/ArchieOS-Operator.wsl "$out_dir"/ArchieOS-Operator.wsl.[0-9][0-9][0-9]
tar --numeric-owner --xattrs --acls -C "$rootfs" -cf - . | gzip -n -6 > "$out_dir/ArchieOS-Operator.wsl"
( cd "$out_dir" && sha256sum ArchieOS-Operator.wsl > SHA256SUMS )
# GitHub takes release assets of at most 2 GiB: a larger image goes up in parts.
if (( $(stat -c %s "$out_dir/ArchieOS-Operator.wsl") > 1900 * 1024 * 1024 )); then
  split -b 1900M -d -a 3 --numeric-suffixes=1 "$out_dir/ArchieOS-Operator.wsl" "$out_dir/ArchieOS-Operator.wsl."
  rm "$out_dir/ArchieOS-Operator.wsl"
fi
cp "$repo_root/scripts/wsl/install-operator.ps1" "$out_dir/install-archie-operator.ps1"
du -sh "$out_dir"/ArchieOS-Operator.wsl*
