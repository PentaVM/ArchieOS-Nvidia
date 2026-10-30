#!/bin/bash
# One recorded take of the demo on Xvfb :77: desktop, Sentry and Forge from the dock, then
# Archie draws the Mona Lisa, narrated. Writes raw.mkv, events.log (seconds since recording
# start) and draw.log.
set -u
S=/tmp/claude-0/-home-user/4934f298-af32-52e3-99bd-bc587ce668b6/scratchpad
OUT=$S/take
. /home/user/demo-run/env.sh
export ARCHIE_UI_LANG=en
pkill -x archie-display; pkill -x archie-stage; pkill -x firefox; pkill -x firefox-bin; sleep 1
[ -f $RUN/xvfb.pid ] && kill "$(cat $RUN/xvfb.pid)" 2>/dev/null; sleep 1
rm -f $RUN/control.sock $RUN/narration.json $XDG_RUNTIME_DIR/archie-stage $XDG_RUNTIME_DIR/archie-stage.lock
# A fresh browser profile: Photopea keeps old documents in its storage.
prefs=$(cat $RUN/ff-profile/user.js); rm -rf $RUN/ff-profile; mkdir -p $RUN/ff-profile; printf '%s\n' "$prefs" > $RUN/ff-profile/user.js
mkdir -p $XDG_RUNTIME_DIR && chmod 700 $XDG_RUNTIME_DIR
setsid nohup Xvfb :77 -screen 0 1920x1080x24 >$RUN/xvfb.log 2>&1 & echo $! > $RUN/xvfb.pid
sleep 2
DISPLAY=:77 QT_QPA_PLATFORM=xcb setsid nohup $STAGE --socket-name archie-stage --control $RUN/control.sock \
  --key-file $RUN/key --tempo person --presence-file "" --narration-file $RUN/narration.json >$RUN/stage.log 2>&1 &
echo $! > $RUN/stage.pid
# The compositor's socket appears only once it listens; an old file there refuses clients.
for i in $(seq 60); do [ -S $RUN/control.sock ] && grep -q 'screen 0' $RUN/stage.log && break; sleep 1; done
sleep 3
(cd /home/user/build-demo/native && WAYLAND_DISPLAY=archie-stage QT_QPA_PLATFORM=wayland LIBGL_ALWAYS_SOFTWARE=1 \
  ARCHIE_STAGE_DESKTOP=1 setsid nohup ./archie-display > $RUN/shell.log 2>&1 & echo $! > $RUN/shell.pid)
sleep 16
grep -q 'failed to start\|Failed to create' $RUN/shell.log && { echo "shell did not start"; cat $RUN/shell.log; exit 1; }
ffmpeg -loglevel error -y -f x11grab -framerate 15 -video_size 1920x1080 -i :77 \
  -c:v libx264 -preset ultrafast -crf 20 -pix_fmt yuv420p $OUT/raw.mkv & FFPID=$!
T0=$(date +%s.%N)
ev() { printf '%6.1f  %s\n' "$(echo "$(date +%s.%N) - $T0" | bc)" "$*" >> $OUT/events.log; }
: > $OUT/events.log
click() { "$BIN/archie-stage-cli" act --socket $RUN/control.sock --key-file $RUN/key --policy $RUN/apps.toml --app archie-display click "$1" "$2" >/dev/null 2>&1; }
ev desktop; sleep 8
ev sentry; click 1030 1022; sleep 9
ev forge; click 960 1022; sleep 9
ev orb; click 750 1022; sleep 6
ev draw
OMP_WAIT_POLICY=PASSIVE QT_QUICK_BACKEND=software LIBGL_ALWAYS_SOFTWARE=1 \
  "$BIN/archie-voice" draw "the Mona Lisa" --language en --socket $RUN/control.sock --key-file $RUN/key \
  --policy $RUN/apps.toml --stage-display archie-stage --app-log $RUN/apps.log \
  --subtitles $RUN/narration.json --seconds ${DRAW_SECONDS:-120} > $OUT/draw.log 2>&1
ev "drawn $?"
sleep 10
ev end
kill -INT $FFPID; wait $FFPID
