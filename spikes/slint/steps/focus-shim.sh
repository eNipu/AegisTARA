#!/usr/bin/env bash
# Headless only: with no window manager nothing gives the window X input focus.
( xdotool search --sync --name "AegisTARA spike" windowfocus >/dev/null 2>&1 ) &
exec "$@"
