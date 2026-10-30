#!/bin/sh
# Archie, whole: the stage on a screen, and a voice that listens, answers and acts on it.
#
#   NEBIUS_TOKEN_FACTORY_KEY=... display/stage/run-archie.sh public
#
# The argument is how private what you say is. It has no default, because what a person
# says to their computer is theirs: `public` lets it reach Nemotron on Nebius, whose
# retention nobody has verified; anything else is refused before it leaves the machine.
#
# Say "Archie" to be heard. Applications open only as the stage policy spells them out.
# Move the mouse over the stage and Archie stops acting until you let go.
#
# Environment (all optional):
#   STAGE, STAGE_CLI, ARCHIE_VOICE   binaries                  default: from PATH
#   POLICY        stage policy                     default: core/crates/archie-stage/policy/apps.toml
#   ASR_MODEL     Nemotron 3.5 ASR GGUF            default: the one `nemo-speech pull` cached
#   PIPER, PIPER_PYTHON, PIPER_MODEL, PIPER_MODEL_EN   Piper adapter/runtime and voices
#                 see scripts/models/install-piper-voices.sh
#   HOST_WAYLAND  session the stage opens in       default: $WAYLAND_DISPLAY
#                 (WSLg: /mnt/wslg/runtime-dir/wayland-0)
#   SCREEN        which screen the stage takes     default: the last one
#   RUN_DIR       key, sockets, logs               default: /tmp/archie-run
#   LISTEN_LANGUAGE  what the recogniser listens for: es-US, en-US or auto  default: es-US
#                 (auto: either, and Archie answers in the language it was spoken to)
#   HEADPHONES=1  keep listening while Archie speaks, so it can be interrupted
#   ALWAYS=1      answer whatever is heard, named or not (one person alone)
#   SESSION_LOG   also write the session there, each line with its time
#   DESKTOP=1     ArchieOS's own shell inside the stage, as its desktop: the apps Archie
#                 opens are framed over it, between its top bar and its dock
#   ARCHIE_DISPLAY  the shell's binary                    default: archie-display
#   DESKTOP_ARGS  more shell flags, e.g. "--theme dark" or "--no-boot-splash"
#   WORLD_BIN_DIR   where archie, archie-worldd and archie-displayd are: World's dock icon
#                 then opens the globe on a bounded navigation session of its own (see
#                 docs/WORLD-EXPLORATION.md), renewed hourly while the shell is open
#   WORLD_SOURCE  World's first imagery              default: google_satellite
#   ARCHIE_ROOT   Core node root for World's console    default: none (console reads only help)
#   WORLD_CONTEXT ContextDomain of World's console      default: personal/owner
#   TEMPO         the stage's tempo: person, video, agent or live   default: person
#   STROKE_SPEED  how fast a drawn line goes, pixels a second       default: the tempo's
#   DRAW_SECONDS  how long a painting is planned to take            default: 150
#   PICTURES      where the paintings Archie can draw are           default: archie-arena's
#   STAGE_ARGS    more stage flags, e.g. "--windowed --width 1600 --height 900" or
#                 "--grab DIR/stage.png --grab-every-ms 5000" (the stage's own window, numbered)
#   REHEARSE      a painting to draw with no voice and no model ("the Mona Lisa"): the
#                 desktop, World and the drawing exactly as in a take, nothing listened to
#   REHEARSE_LANGUAGE  the narration's language in a rehearsal (en or es)   default: en
#   WAV_IN, WAV_OUT  a recording heard instead of the microphone, and a file Archie's voice
#                 goes to instead of the speaker
#   STAGE_ONLY=1  the stage and its desktop with nobody listening (footage, typed turns)
#   GROUND_MODEL, GROUND_ENDPOINT  the model that finds what a click names when the window
#                 declares nothing (Nebius unless an endpoint names a local NIM); without one,
#                 clicks are refused, the dock's included
set -eu

GRADE=${1:-}
case "$GRADE" in
    public|internal) ;;
    *) echo "run-archie: say how private this is: run-archie.sh public (or internal)" >&2; exit 64 ;;
esac
# Neither a rehearsal nor the stage alone listens, speaks or asks a model anything.
QUIET=${REHEARSE:-}
[ "${STAGE_ONLY:-0}" = 1 ] && QUIET=stage
# ARCHIE_VOICE_ENDPOINT names a reasoning model on this machine; then no key is needed.
[ -n "$QUIET" ] || [ -n "${NEBIUS_TOKEN_FACTORY_KEY:-}" ] || [ -n "${ARCHIE_VOICE_ENDPOINT:-}" ] || {
    echo "run-archie: set NEBIUS_TOKEN_FACTORY_KEY, or ARCHIE_VOICE_ENDPOINT for a local model" >&2
    exit 64
}

# Accessibility needs a session bus, and the stage's clients inherit it.
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ] && command -v dbus-run-session >/dev/null; then
    exec dbus-run-session -- "$0" "$@"
fi

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
STAGE=${STAGE:-archie-stage}
STAGE_CLI=${STAGE_CLI:-archie-stage-cli}
ARCHIE_VOICE=${ARCHIE_VOICE:-archie-voice}
POLICY=${POLICY:-$REPO/core/crates/archie-stage/policy/apps.toml}
ASR_MODEL=${ASR_MODEL:-$(ls "$HOME"/.cache/nemo-speech/models/nvidia/nemotron-3.5-asr-streaming-0.6b/*/*.gguf 2>/dev/null | head -n 1)}
PIPER_PYTHON=${PIPER_PYTHON:-python3}
PIPER=${PIPER:-$REPO/scripts/models/piper-json-bridge.py}
PIPER_VOICES=${PIPER_VOICES:-$REPO/models-cache/piper}
# The voices heard best (scripts/models/install-piper-voices.sh), else the earlier pair.
default_voice() { if [ -f "$PIPER_VOICES/$1.onnx" ]; then echo "$PIPER_VOICES/$1.onnx"; else echo "$PIPER_VOICES/$2.onnx"; fi; }
PIPER_MODEL=${PIPER_MODEL:-$(default_voice es_MX-claude-high es_MX-ald-medium)}
PIPER_MODEL_EN=${PIPER_MODEL_EN:-$(default_voice en_US-kristin-medium en_US-john-medium)}
HOST_WAYLAND=${HOST_WAYLAND:-${WAYLAND_DISPLAY:-}}
RUN_DIR=${RUN_DIR:-/tmp/archie-run}
SOCKET_NAME=archie-stage

[ -n "$HOST_WAYLAND" ] || { echo "run-archie: set HOST_WAYLAND to the Wayland session to open the stage in" >&2; exit 1; }

# The stage gets its own runtime directory, and PulseAudio finds its server through that
# variable: name the person's sound server before it changes, or the voice goes deaf.
if [ -z "${PULSE_SERVER:-}" ]; then
    if [ -S /mnt/wslg/PulseServer ]; then
        export PULSE_SERVER=unix:/mnt/wslg/PulseServer
    elif [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -S "$XDG_RUNTIME_DIR/pulse/native" ]; then
        export PULSE_SERVER="unix:$XDG_RUNTIME_DIR/pulse/native"
    fi
fi
# A rehearsal neither listens nor speaks: it needs no recogniser and no voices.
if [ -z "$QUIET" ]; then
    [ -n "$ASR_MODEL" ] && [ -f "$ASR_MODEL" ] || { echo "run-archie: no ASR model; run nemo-speech pull nemotron-3.5 or set ASR_MODEL" >&2; exit 1; }
    [ -f "$PIPER_MODEL" ] || { echo "run-archie: no Spanish Piper model at $PIPER_MODEL; run scripts/models/install-piper-voices.sh" >&2; exit 1; }
fi
# The bridge runs as `env python3`: put the Python that has Piper first on PATH, for this
# checkout's bridge and for the installed one (/usr/lib/archie) alike. Without it the bridge
# starts under the system Python, dies at the first reply, and the conversation ends.
if [ -z "$QUIET" ] && { [ "$PIPER" = "$REPO/scripts/models/piper-json-bridge.py" ] || [ "$PIPER_PYTHON" != python3 ]; }; then
    PIPER_PYTHON_BIN=$(command -v "$PIPER_PYTHON" 2>/dev/null || true)
    [ -n "$PIPER_PYTHON_BIN" ] || { echo "run-archie: Python runtime not found: $PIPER_PYTHON" >&2; exit 1; }
    PATH=$(dirname "$PIPER_PYTHON_BIN"):$PATH
    export PATH
    "$PIPER_PYTHON_BIN" -c 'import onnxruntime, piper' 2>/dev/null || {
        echo "run-archie: install Piper Python and ONNX Runtime in $PIPER_PYTHON" >&2
        exit 1
    }
fi

mkdir -p "$RUN_DIR/xdg"
chmod 700 "$RUN_DIR" "$RUN_DIR/xdg"
rm -f "$RUN_DIR/control.sock" "$RUN_DIR/xdg/$SOCKET_NAME"
[ -f "$RUN_DIR/key" ] || "$STAGE_CLI" keygen "$RUN_DIR/key" >/dev/null

if [ -x /usr/lib/at-spi-bus-launcher ]; then
    /usr/lib/at-spi-bus-launcher --launch-immediately >/dev/null 2>&1 &
fi

# What Archie says on its own while it works is shown under the stage as it is said.
NARRATION=$RUN_DIR/narration.json
rm -f "$NARRATION"
set -- --socket-name "$SOCKET_NAME" --control "$RUN_DIR/control.sock" --key-file "$RUN_DIR/key" \
    --narration-file "$NARRATION"
[ -n "${SCREEN:-}" ] && set -- "$@" --screen "$SCREEN"
[ -n "${TEMPO:-}" ] && set -- "$@" --tempo "$TEMPO"
[ -n "${STROKE_SPEED:-}" ] && set -- "$@" --stroke-speed "$STROKE_SPEED"
# shellcheck disable=SC2086 # STAGE_ARGS is a list of flags on purpose.
[ -n "${STAGE_ARGS:-}" ] && set -- "$@" $STAGE_ARGS
XDG_RUNTIME_DIR="$RUN_DIR/xdg" WAYLAND_DISPLAY="$HOST_WAYLAND" QT_QPA_PLATFORM=wayland \
    "$STAGE" "$@" > "$RUN_DIR/stage.log" 2>&1 &
STAGE_PID=$!
trap 'kill "$STAGE_PID" 2>/dev/null || true' EXIT INT TERM

for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
    [ -S "$RUN_DIR/control.sock" ] && [ -S "$RUN_DIR/xdg/$SOCKET_NAME" ] && break
    sleep 0.5
done
[ -S "$RUN_DIR/xdg/$SOCKET_NAME" ] || { echo "run-archie: the stage did not start; see $RUN_DIR/stage.log" >&2; exit 1; }

# ArchieOS's shell as the stage's desktop: Orb, Vault, Forge, Sentry and World in one
# screen, with what Archie opens framed over it. Qt Quick 3D draws with Mesa's software GL:
# a nested compositor shares no GPU buffers (WSLg included). The shell reads only; it has
# no authority the stage policy does not give an entry for it.
if [ "${DESKTOP:-0}" = 1 ]; then
    ARCHIE_DISPLAY=$(command -v "${ARCHIE_DISPLAY:-archie-display}") || {
        echo "run-archie: DESKTOP=1 needs archie-display (set ARCHIE_DISPLAY)" >&2
        exit 1
    }
    # World's console reaches Core through the archie CLI, and IRIS goals through this
    # stage's own socket, key and Policy. Every value is the host's, never the console's.
    set -- --world-context "\"${WORLD_CONTEXT:-personal/owner}\"" \
        --stage-socket "$RUN_DIR/control.sock" --stage-key-file "$RUN_DIR/key" --stage-policy "$POLICY"
    [ -n "${ARCHIE_ROOT:-}" ] && set -- "$@" --archie-root "$ARCHIE_ROOT"
    # shellcheck disable=SC2086 # DESKTOP_ARGS is a list of flags on purpose.
    [ -n "${DESKTOP_ARGS:-}" ] && set -- "$@" $DESKTOP_ARGS
    # Sentry's LAN inventory, when the node has one (escanear-lan.bat hands it over).
    [ -n "${ARCHIE_ROOT:-}" ] && [ -s "$ARCHIE_ROOT/var/lib/sentry/lan.json" ] \
        && set -- "$@" --lan-file "$ARCHIE_ROOT/var/lib/sentry/lan.json"
    if [ -n "${WORLD_BIN_DIR:-}" ]; then
        # globe_demo.py hands each of them on to the shell.
        for arg do set -- "$@" "--native-arg=$arg"; shift; done
        # displayd reads the same Core node, so the shell sees archied live and its telemetry.
        [ -n "${ARCHIE_ROOT:-}" ] && set -- --core-root "$ARCHIE_ROOT" "$@"
        # World's daemons and its owner session, with the shell as their window. Their own
        # loopback ports (17666/17667); the shell keeps the visual controls on 7668.
        XDG_RUNTIME_DIR="$RUN_DIR/xdg" WAYLAND_DISPLAY="$SOCKET_NAME" QT_QPA_PLATFORM=wayland \
            LIBGL_ALWAYS_SOFTWARE=1 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 ARCHIE_STAGE_DESKTOP=1 \
            python3 "$REPO/_dev/verification/world-globe/globe_demo.py" --bin-dir "$WORLD_BIN_DIR" \
                --run-dir "$RUN_DIR/world-$(date +%s)" --native-bin "$ARCHIE_DISPLAY" --shell \
                --native-port 7668 --source "${WORLD_SOURCE:-google_satellite}" --renew-navigation "$@" \
                > "$RUN_DIR/shell.log" 2>&1 &
    else
        XDG_RUNTIME_DIR="$RUN_DIR/xdg" WAYLAND_DISPLAY="$SOCKET_NAME" QT_QPA_PLATFORM=wayland \
            LIBGL_ALWAYS_SOFTWARE=1 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 ARCHIE_STAGE_DESKTOP=1 \
            "$ARCHIE_DISPLAY" --shell "$@" > "$RUN_DIR/shell.log" 2>&1 &
    fi
    SHELL_PID=$!
    trap 'kill "$SHELL_PID" "$STAGE_PID" 2>/dev/null || true' EXIT INT TERM
fi

if [ "${STAGE_ONLY:-0}" = 1 ]; then
    # The stage and its desktop, nobody listening: for looking around, for footage of the
    # desktop alone, and for typed turns (archie-voice do) against this stage.
    echo "run-archie: stage only, at $RUN_DIR (Ctrl+C ends it)"
    wait "$STAGE_PID"
    exit
fi
if [ -n "${REHEARSE:-}" ]; then
    # The drawing the conversation would start, on its own, narrated in subtitles.
    set -- draw "$REHEARSE" --language "${REHEARSE_LANGUAGE:-en}" \
        --socket "$RUN_DIR/control.sock" --key-file "$RUN_DIR/key" --policy "$POLICY" \
        --stage-display "$SOCKET_NAME" --app-log "$RUN_DIR/apps.log" \
        --subtitles "$NARRATION" --seconds "${DRAW_SECONDS:-150}"
    [ -n "${PICTURES:-}" ] && set -- "$@" --pictures "$PICTURES"
    XDG_RUNTIME_DIR="$RUN_DIR/xdg" OMP_WAIT_POLICY=PASSIVE \
        QT_QUICK_BACKEND=software LIBGL_ALWAYS_SOFTWARE=1 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 \
        "$ARCHIE_VOICE" "$@"
    exit
fi
set -- converse --grade "$GRADE" --asr-model "$ASR_MODEL" \
    --voice-engine piper --piper "$PIPER" --piper-model "$PIPER_MODEL" \
    --socket "$RUN_DIR/control.sock" --key-file "$RUN_DIR/key" --policy "$POLICY" \
    --stage-display "$SOCKET_NAME" --app-log "$RUN_DIR/apps.log" \
    --subtitles "$NARRATION" --seconds "${DRAW_SECONDS:-150}"
[ -n "${PICTURES:-}" ] && set -- "$@" --pictures "$PICTURES"
if [ -f "$PIPER_MODEL_EN" ]; then
    set -- "$@" --piper-model-en "$PIPER_MODEL_EN"
else
    echo "run-archie: English Piper voice is not installed; using Spanish for every reply" >&2
fi
[ -n "${LISTEN_LANGUAGE:-}" ] && set -- "$@" --listen-language "$LISTEN_LANGUAGE"
[ "${HEADPHONES:-0}" = 1 ] && set -- "$@" --headphones
[ "${ALWAYS:-0}" = 1 ] && set -- "$@" --always
# How strongly the recogniser favours the name and the demo's words. Only a GGUF that embeds
# its tokenizer boosts at all; on one that does, too much makes it hear those words in
# everything ("Photopeaw the Photopea" at 5).
[ -n "${NAME_BOOST:-}" ] && set -- "$@" --name-boost "$NAME_BOOST"
# Clicks by name: what a window declares to the accessibility bus is found with no model
# (the desktop's dock); the grounding model only looks for what declares nothing.
[ -n "${GROUND_MODEL:-}" ] && set -- "$@" --ground-model "$GROUND_MODEL"
[ -n "${GROUND_ENDPOINT:-}" ] && set -- "$@" --ground-endpoint "$GROUND_ENDPOINT"
# A recording in place of the microphone, and a file in place of the speaker: the whole
# spoken loop, tested without a room.
[ -n "${WAV_IN:-}" ] && set -- "$@" --wav-in "$WAV_IN"
[ -n "${WAV_OUT:-}" ] && set -- "$@" --wav-out "$WAV_OUT"

# Two thread pools share this process's cores; spinning ones starve each other.
# Applications opened by voice inherit the stage's runtime directory and software rendering.
voice() {
    XDG_RUNTIME_DIR="$RUN_DIR/xdg" OMP_WAIT_POLICY=PASSIVE \
        QT_QUICK_BACKEND=software LIBGL_ALWAYS_SOFTWARE=1 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 \
        "$ARCHIE_VOICE" "$@"
}
[ -n "${SESSION_LOG:-}" ] || { voice "$@"; exit; }

# The session is kept as the window shows it, each line with its wall-clock time: what was
# heard, what was said, the floor's decisions and the timings. Only the owner can read it.
mkdir -p "$(dirname "$SESSION_LOG")"
chmod 700 "$(dirname "$SESSION_LOG")"
(umask 077 && : >>"$SESSION_LOG")
{
    echo "# archie session $(date -Iseconds)"
    echo "# grade=$GRADE model=${ARCHIE_VOICE_MODEL:-default} endpoint=${ARCHIE_VOICE_ENDPOINT:-nebius} headphones=${HEADPHONES:-0} always=${ALWAYS:-0}"
} >>"$SESSION_LOG"
voice "$@" 2>&1 | {
    # Ctrl+C reaches the voice too; this reader keeps writing until the voice has stopped.
    trap '' INT TERM
    while IFS= read -r line; do
        printf '%s\n' "$line"
        printf '%s %s\n' "$(date +%T.%3N)" "$line" >>"$SESSION_LOG"
    done
}
echo "# ended $(date -Iseconds)" >>"$SESSION_LOG"
