#!/usr/bin/env bash
# Headless only: with no window manager nothing gives the window X input focus, and winit then
# drops key events. Wait until the window is mapped (--onlyvisible), focus it, and repeat once
# after a second in case the first attempt raced with the map.
(
  xdotool search --sync --onlyvisible --name "AegisTARA spike" windowfocus >/dev/null 2>&1
  sleep 1
  xdotool search --onlyvisible --name "AegisTARA spike" windowfocus >/dev/null 2>&1
) &
exec "$@"
