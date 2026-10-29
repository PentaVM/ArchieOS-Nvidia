#!/usr/bin/env bash
# The Nebius demo, in one command: ArchieOS's shell as the desktop of the Archie Stage, and
# Archie listening. Say "Archie, draw the Mona Lisa": Photopea opens over the desktop and
# Archie draws it with its own pointer, through the Stage's gate, saying what it does as it
# goes (and showing it under the picture). Ask for a website or a search and it opens in
# the same browser; ask it to stop drawing and it stops between two batches of lines.
#
#   NEBIUS_TOKEN_FACTORY_KEY=... display/session/nebius-demo.sh
#
# Everything run-archie.sh reads still applies (ASR_MODEL, PIPER_*, HOST_WAYLAND, SCREEN,
# HEADPHONES, ALWAYS, SESSION_LOG...). Besides:
#   FIREFOX          the browser                       default: firefox
#   FIREFOX_APP_ID   the app id its windows report     default: firefox (Debian's ESR: firefox-esr)
#   DRAW_SECONDS     how long a painting is planned    default: 150
#   ARCHIE_DISPLAY   the shell                         default: the native build, else PATH
#   WORLD_BIN_DIR    World's daemons and CLI           default: core/target/release, if built
#   RUN_DIR          key, sockets, logs, the profile   default: /tmp/archie-demo
#   ARCHIE_UI_LANG   the desktop's language: en       default: Spanish
#   ARCHIE_ROOT      Core node for World's console     default: /home/archie/archie-node, if there
#   SESSION_LOG      the take's log, each line timed   default: $RUN_DIR/takes/take-<time>.log
#
# The browser starts from a new profile every time: Photopea keeps its last documents in
# the browser's storage and reopens them, and a first run shows welcome pages. Nothing in
# it is the person's.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
export RUN_DIR=${RUN_DIR:-/tmp/archie-demo}
FIREFOX=${FIREFOX:-firefox}
FIREFOX_APP_ID=${FIREFOX_APP_ID:-firefox}
FIREFOX_BIN=$(command -v "$FIREFOX") || { echo "nebius-demo: $FIREFOX is not installed" >&2; exit 1; }
RUN_ARCHIE=${RUN_ARCHIE:-$REPO/display/stage/run-archie.sh}
[ -x "$RUN_ARCHIE" ] || RUN_ARCHIE=/usr/lib/archie/run-archie.sh

# On the ArchieOS-Operator WSL image: what archie-talk would set, and its key.
if [ -d /var/lib/archie/models ]; then
    voice_venv=/opt/archie/voice-venv
    [ -x "$voice_venv/bin/python" ] || voice_venv=/opt/archie/iris-venv
    export ASR_MODEL=${ASR_MODEL:-/var/lib/archie/models/asr/nemotron-3.5-asr-streaming-0.6b.q8_0.gguf}
    export PIPER=${PIPER:-/usr/lib/archie/piper-json-bridge.py}
    export PIPER_PYTHON=${PIPER_PYTHON:-$voice_venv/bin/python}
    export PIPER_VOICES=${PIPER_VOICES:-/var/lib/archie/models/piper}
fi
[ -S /mnt/wslg/runtime-dir/wayland-0 ] && export HOST_WAYLAND=${HOST_WAYLAND:-/mnt/wslg/runtime-dir/wayland-0}
if [ -z "${NEBIUS_TOKEN_FACTORY_KEY:-}" ] && [ -z "${ARCHIE_VOICE_ENDPOINT:-}" ] \
    && [ -s "$HOME/.config/archie/nebius.key" ]; then
    NEBIUS_TOKEN_FACTORY_KEY=$(cat "$HOME/.config/archie/nebius.key")
    export NEBIUS_TOKEN_FACTORY_KEY
fi
# The operator's own session uses the same sockets: one of the two at a time.
command -v archie-session >/dev/null && archie-session down >/dev/null 2>&1 || true

mkdir -p "$RUN_DIR"
chmod 700 "$RUN_DIR"
PROFILE=$RUN_DIR/firefox
# A browser left from an earlier take holds the profile's lock, and World's daemons from one
# that was cut short hold their private ports (17666/17667): the new take's World cannot start.
pkill -f -- "--profile $PROFILE" 2>/dev/null || true
pkill -u "$(id -u)" -f -- "archie-worldd --bind 127.0.0.1:17667" 2>/dev/null || true
pkill -u "$(id -u)" -f -- "archie-displayd .*--bind 127.0.0.1:17666" 2>/dev/null || true
rm -rf "${PROFILE:?}"
mkdir -p "$PROFILE" "$RUN_DIR/downloads"
cat > "$PROFILE/user.js" <<EOF
user_pref("browser.aboutwelcome.enabled", false);
user_pref("browser.preonboarding.enabled", false);
user_pref("termsofuse.bypassNotification", true);
user_pref("browser.startup.homepage_override.mstone", "ignore");
user_pref("startup.homepage_welcome_url", "");
user_pref("startup.homepage_welcome_url.additional", "");
user_pref("datareporting.policy.dataSubmissionPolicyBypassNotification", true);
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("toolkit.telemetry.reportingpolicy.firstRun", false);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.tabs.warnOnClose", false);
user_pref("browser.sessionstore.resume_from_crash", false);
user_pref("browser.startup.page", 0);
user_pref("browser.startup.couldRestoreSession.count", -1);
user_pref("browser.toolbars.bookmarks.visibility", "never");
user_pref("browser.translations.automaticallyPopup", false);
user_pref("media.webspeech.synth.enabled", false);
user_pref("media.autoplay.default", 5);
user_pref("browser.download.folderList", 2);
user_pref("browser.download.dir", "$RUN_DIR/downloads");
user_pref("browser.download.useDownloadDir", true);
user_pref("browser.crashReports.unsubmittedCheck.enabled", false);
user_pref("browser.tabs.crashReporting.sendReport", false);
EOF
# The browser draws only on the stage: when a take ends the stage goes and the browser loses
# its display, which Firefox counts as a crash and reports in a window of its own on the
# person's screen, take after take. It gets no crash reporter, and it is closed when the
# take ends, before anything else is.
export MOZ_CRASHREPORTER_DISABLE=1

# The policy this take runs with: the demo's, with this machine's browser and profile.
POLICY=$RUN_DIR/apps.toml
sed -e "s|@FIREFOX_APP_ID@|$FIREFOX_APP_ID|" -e "s|@FIREFOX@|$FIREFOX_BIN|" -e "s|@PROFILE@|$PROFILE|" \
    "$HERE/nebius-demo-apps.toml" > "$POLICY"
export POLICY DESKTOP=1 DRAW_SECONDS=${DRAW_SECONDS:-150}
# World's console asks Core (archied) for places and OS state when the demo node is there.
[ -z "${ARCHIE_ROOT:-}" ] && [ -S /home/archie/archie-node/run/archied.sock ] && export ARCHIE_ROOT=/home/archie/archie-node
# Every take keeps its own timed log: what was heard, said and done (the evidence .txt of
# the brief's capture rules starts from it).
mkdir -p "$RUN_DIR/takes"
export SESSION_LOG=${SESSION_LOG:-$RUN_DIR/takes/take-$(date +%Y%m%d-%H%M%S).log}
export LISTEN_LANGUAGE=${LISTEN_LANGUAGE:-auto}
# "Archie, open Sentry" is a click on the dock's button. The desktop names its buttons to the
# accessibility bus, so they are found with no model; the grounding model (docs/STAGE.md) is
# only asked about what a window does not declare. Without one, every click is refused.
export GROUND_MODEL=${GROUND_MODEL:-moonshotai/Kimi-K3}
# Binaries of this checkout's builds, when they are there; otherwise the installed ones.
[ -z "${ARCHIE_VOICE:-}" ] && [ -x "$REPO/core/target/release/archie-voice" ] && export ARCHIE_VOICE=$REPO/core/target/release/archie-voice
[ -z "${STAGE_CLI:-}" ] && [ -x "$REPO/core/target/release/archie-stage-cli" ] && export STAGE_CLI=$REPO/core/target/release/archie-stage-cli
[ -z "${PICTURES:-}" ] && [ -d "$REPO/core/crates/archie-arena/tasks" ] && export PICTURES=$REPO/core/crates/archie-arena/tasks
# The stage and the shell as README.md and docs/STAGE.md build them, else the native cache.
[ -z "${STAGE:-}" ] && [ -x "$REPO/build/stage/archie-stage" ] && export STAGE=$REPO/build/stage/archie-stage
[ -z "${ARCHIE_DISPLAY:-}" ] && [ -x "$REPO/build/display/archie-display" ] && export ARCHIE_DISPLAY=$REPO/build/display/archie-display
native=${XDG_CACHE_HOME:-$HOME/.cache}/archieos/native-display/archie-display
[ -z "${ARCHIE_DISPLAY:-}" ] && [ -x "$native" ] && export ARCHIE_DISPLAY=$native
# World's globe streams when its daemons are built here (cargo build --release -p archie-cli
# -p archie-worldd -p archie-displayd); without them its window opens and says what is missing.
release=$REPO/core/target/release
if [ -z "${WORLD_BIN_DIR:-}" ] && [ -x "$release/archie" ] && [ -x "$release/archie-worldd" ] \
    && [ -x "$release/archie-displayd" ]; then
    export WORLD_BIN_DIR=$release
fi
# Not exec: once the take ends, however it ends, this take's browser goes too, so none is
# left drawing to a stage that is gone.
trap 'pkill -TERM -f -- "--profile $PROFILE" 2>/dev/null || true' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
"$RUN_ARCHIE" public
