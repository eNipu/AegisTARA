#!/usr/bin/env bash
# Usage: gui_run.sh <out_dir> <steps_file> -- <command...>
# Starts a private Xvfb display, runs the GUI command, then executes the steps file.
# Steps file lines (one action per line):
#   sleep <seconds>
#   shot <name>                  -> <out_dir>/<name>.png (full screen)
#   move <x> <y>
#   click <button> <x> <y>       (button: 1 left, 3 right)
#   drag <x1> <y1> <x2> <y2>     (left-button drag)
#   key <keysym>                 (for example Escape, Down, Return, shift+F10)
#   type <text...>
#   scroll <up|down> <x> <y> <n>
set -u
OUT="$1"; STEPS="$2"; shift 2; [ "$1" = "--" ] && shift
mkdir -p "$OUT"
# Let Xvfb pick a free display number, so parallel runs never collide.
DFILE="$OUT/.display"; : > "$DFILE"
Xvfb -displayfd 3 -screen 0 1600x1000x24 -nolisten tcp 3>"$DFILE" >/dev/null 2>&1 &
XPID=$!
for _ in $(seq 1 100); do [ -s "$DFILE" ] && break; sleep 0.1; done
DISP=$(head -1 "$DFILE")
[ -z "$DISP" ] && { echo "Xvfb did not start" >&2; kill $XPID 2>/dev/null; exit 1; }
export DISPLAY=:$DISP
export WINIT_UNIX_BACKEND=x11 WAYLAND_DISPLAY= XDG_SESSION_TYPE=x11
export LIBGL_ALWAYS_SOFTWARE=1 VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
"$@" > "$OUT/app.log" 2>&1 &
APID=$!
while read -r cmd a b c d; do
  case "$cmd" in
    ""|"#"*) ;;
    sleep) sleep "$a" ;;
    shot) import -window root "$OUT/$a.png" 2>>"$OUT/harness.log" ;;
    move) xdotool mousemove "$a" "$b" ;;
    click) xdotool mousemove "$b" "$c"; sleep 0.2; xdotool click "$a" ;;
    drag) xdotool mousemove "$a" "$b" mousedown 1; sleep 0.2; xdotool mousemove "$c" "$d"; sleep 0.2; xdotool mouseup 1 ;;
    key) xdotool key "$a" ;;
    type) xdotool type --delay 40 "$a${b:+ $b}${c:+ $c}${d:+ $d}" ;;
    scroll) btn=4; [ "$a" = down ] && btn=5; xdotool mousemove "$b" "$c"; for i in $(seq 1 "$d"); do xdotool click $btn; sleep 0.05; done ;;
  esac
  if ! kill -0 $APID 2>/dev/null; then echo "app exited early" >> "$OUT/harness.log"; break; fi
done < "$STEPS"
kill $APID 2>/dev/null; sleep 0.5; kill -9 $APID 2>/dev/null
kill $XPID 2>/dev/null
echo "done; log at $OUT/app.log"
