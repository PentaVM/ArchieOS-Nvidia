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
#   FIREFOX          the browser                       default: the first Firefox found, not a snap
#   FIREFOX_APP_ID   the app id its windows report     default: the build's RemotingName (ESR: firefox-esr)
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
# The checkout this script is in, also when it is started through a link to it.
HERE=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
export RUN_DIR=${RUN_DIR:-/tmp/archie-demo}
# The browser: the one named, else the first Firefox found. A snap's cannot reach the stage
# (its sandbox sees neither this /tmp nor a Wayland socket not named wayland-N), so it is
# passed over; Mozilla's own build or the distribution's package can.
find_firefox() {
    local candidate path
    for candidate in firefox firefox-esr /opt/firefox/firefox /usr/lib/firefox/firefox \
        /usr/lib/firefox-esr/firefox-esr "$HOME/.local/opt/firefox/firefox" "$HOME/firefox/firefox"; do
        path=$(command -v "$candidate" 2>/dev/null) || continue
        case "$(readlink -f "$path")" in
            /snap/*|*/snapd/*) continue ;;
        esac
        # Ubuntu's firefox package is only a wrapper that runs the snap.
        grep -qs '/snap/bin/firefox' "$path" && continue
        echo "$path"
        return 0
    done
    return 1
}
if [ -n "${FIREFOX:-}" ]; then
    FIREFOX_BIN=$(command -v "$FIREFOX") || { echo "nebius-demo: $FIREFOX is not installed" >&2; exit 1; }
elif ! FIREFOX_BIN=$(find_firefox); then
    FIREFOX_BIN=
    if [ "${STAGE_ONLY:-0}" != 1 ]; then
        echo "nebius-demo: no Firefox that can open on the stage (a snap's cannot). Install Mozilla's" >&2
        echo "  (https://www.mozilla.org/firefox/linux/) or the distribution's package, or set FIREFOX" >&2
        exit 1
    fi
fi
# The app id its windows report is the build's remoting name (an ESR's, Debian's or
# Mozilla's own, is firefox-esr), written beside the binary; a wrapper script is followed to
# the binary it runs.
firefox_app_id() {
    local bin dir
    bin=$(readlink -f "$1" 2>/dev/null) || return 1
    if [ ! -f "$(dirname "$bin")/application.ini" ]; then
        bin=$(grep -o '/[^ "]*/firefox[a-z-]*' "$bin" 2>/dev/null | head -n 1)
        [ -n "$bin" ] && bin=$(readlink -f "$bin")
    fi
    dir=$(dirname "${bin:-/nonexistent}")
    sed -n 's/^RemotingName=//p' "$dir/application.ini" 2>/dev/null | head -n 1
}
if [ -z "${FIREFOX_APP_ID:-}" ]; then
    FIREFOX_APP_ID=$( [ -n "$FIREFOX_BIN" ] && firefox_app_id "$FIREFOX_BIN" || true)
    if [ -z "$FIREFOX_APP_ID" ]; then
        case "$(basename "${FIREFOX_BIN:-firefox}")" in
            firefox-esr) FIREFOX_APP_ID=firefox-esr ;;
            *) FIREFOX_APP_ID=firefox ;;
        esac
    fi
fi
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
sed -e "s|@FIREFOX_APP_ID@|$FIREFOX_APP_ID|" -e "s|@FIREFOX@|${FIREFOX_BIN:-firefox}|" -e "s|@PROFILE@|$PROFILE|" \
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
# A build older than this checkout shows yesterday's desktop (its old bar, its old icons)
# with nothing saying so. One that CMake made from this checkout is brought up to date
# first (NO_REBUILD=1 skips it); any other older one is named.
fresh() {
    local bin=$1 build newer source
    shift
    [ -x "$bin" ] || return 0
    build=$(dirname "$(readlink -f "$bin")")
    newer=$(find "$@" -newer "$bin" -type f \( -name '*.qml' -o -name '*.js' -o -name '*.cpp' \
        -o -name '*.h' -o -name '*.hpp' -o -name '*.png' -o -name CMakeLists.txt \) -print -quit 2>/dev/null)
    [ -n "$newer" ] || return 0
    source=$(sed -n 's/^CMAKE_HOME_DIRECTORY:INTERNAL=//p' "$build/CMakeCache.txt" 2>/dev/null)
    if [ "${NO_REBUILD:-0}" != 1 ] && [ -n "$source" ] && [ "${source#"$REPO"/}" != "$source" ] \
        && command -v cmake >/dev/null; then
        echo "nebius-demo: $(basename "$bin") is older than ${newer#"$REPO"/}; rebuilding $build"
        cmake --build "$build" --target "$(basename "$bin")" >"$RUN_DIR/rebuild.log" 2>&1 || {
            echo "nebius-demo: the rebuild failed; see $RUN_DIR/rebuild.log" >&2
            exit 1
        }
        # Up to date by CMake's reckoning, also when what was newer is not one of its inputs.
        touch "$bin"
    else
        echo "nebius-demo: warning: $bin is older than ${newer#"$REPO"/}; it shows an earlier desktop" >&2
    fi
}
[ -z "${ARCHIE_DISPLAY:-}" ] && ARCHIE_DISPLAY=$(command -v archie-display) && export ARCHIE_DISPLAY
[ -z "${STAGE:-}" ] && STAGE=$(command -v archie-stage) && export STAGE
fresh "${ARCHIE_DISPLAY:-}" "$REPO/display/native" "$REPO/display/shell"
fresh "${STAGE:-}" "$REPO/display/stage" "$REPO/display/shell/brand" "$REPO/display/shell/prototype/Lang.js"
# The session's taskbar, window list and title bars find a window's icon through the
# desktop entry named for its app id: the stage's and the desktop's, with ArchieOS's mark.
# Hidden entries, for this user only; what the system already has is left as it is.
data=${XDG_DATA_HOME:-$HOME/.local/share}
put() {
    cmp -s "$1" "$2" 2>/dev/null && return 0
    mkdir -p "$(dirname "$2")" && cp "$1" "$2"
}
{
    put "$REPO/display/shell/brand/archieos-app-512.png" "$data/icons/hicolor/512x512/apps/archieos.png"
    put "$REPO/display/shell/brand/archieos-icon-256.png" "$data/icons/hicolor/256x256/apps/archieos.png"
    put "$REPO/distro/display/org.archieos.Stage.desktop" "$data/applications/org.archieos.Stage.desktop"
    [ -f /usr/share/applications/archie-display.desktop ] \
        || sed 's/^NoDisplay=false$/NoDisplay=true/' "$REPO/distro/display/archie-display.desktop" \
            > "$data/applications/archie-display.desktop"
} 2>/dev/null || echo "nebius-demo: could not install the icon under $data; windows keep a generic one" >&2
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
