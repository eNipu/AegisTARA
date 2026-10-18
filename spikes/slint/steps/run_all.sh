#!/usr/bin/env bash
# Usage: steps/run_all.sh <binary> <out_dir>
# Runs every steps file through spikes/tools/gui_run.sh. focus-shim.sh gives the window X input
# focus, which a window manager would normally do (Xvfb has none, and winit then drops key events).
set -u
BIN="$1"; OUT="$2"; HERE="$(cd "$(dirname "$0")" && pwd)"; RUN="$HERE/../../tools/gui_run.sh"
SHIM="$HERE/focus-shim.sh"
"$RUN" "$OUT/a" "$HERE/a_core.steps" -- "$SHIM" "$BIN"
"$RUN" "$OUT/b" "$HERE/b_keyboard.steps" -- "$SHIM" "$BIN"
"$RUN" "$OUT/c" "$HERE/c_edge_add_delete_zoom.steps" -- "$SHIM" "$BIN"
"$RUN" "$OUT/d" "$HERE/d_readonly.steps" -- env READONLY=1 "$SHIM" "$BIN"
"$RUN" "$OUT/e" "$HERE/e_scale.steps" -- env NODES=2000 PERF=1 "$SHIM" "$BIN"
"$RUN" "$OUT/e_sw" "$HERE/e_scale.steps" -- env NODES=2000 SLINT_BACKEND=winit-software "$SHIM" "$BIN"
"$RUN" "$OUT/f" "$HERE/f_editor_reopen.steps" -- "$SHIM" "$BIN"
"$RUN" "$OUT/a_sw" "$HERE/a_core.steps" -- env SLINT_BACKEND=winit-software "$SHIM" "$BIN"
grep -h "^RESULT \(s6\|g2\|g0\|g3\) \|^CHANGE\|^ERROR" "$OUT"/a/app.log
