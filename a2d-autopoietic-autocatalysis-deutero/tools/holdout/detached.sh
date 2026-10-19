#!/bin/sh
# Start one confirmation-3 pipeline driver fully detached: its own session (setsid), no
# terminal, kept awake by caffeinate. A Claude Code background shell can be reaped under
# memory pressure, and a reaped driver means the next driver's recovery kills its in-flight
# jobs (their attempts are spent), so long drivers (c3_select.py, c3_author.py, c3_fix.py)
# and the step-5 drivers (c3_audit.py, c3_adjudicate.py) are launched only through this script.
#
# The driver's stdout carries only gateway lines (counts and opaque ids). It goes to
# ~/.a2d-c3-drivers/<UTC>-<script>.out, outside the holdout dir, so following progress never
# means reading under $A2D_HOLDOUT_DIR.
#
# usage: tools/holdout/detached.sh c3_author.py --parallel 3
set -eu
here=$(cd "$(dirname "$0")" && pwd)
script=${1:?script}
shift
case "$script" in
  c3_select.py|c3_author.py|c3_assemble.py|c3_similarity.py|c3_validate.py|c3_fix.py|c3_audit.py|c3_adjudicate.py) ;;
  *) echo "unknown script" >&2; exit 2 ;;
esac
logs="$HOME/.a2d-c3-drivers"
mkdir -p "$logs"
out="$logs/$(date -u +%Y%m%dT%H%M%SZ)-${script%.py}.out"
nohup /usr/bin/perl -e 'use POSIX qw(setsid); setsid(); exec @ARGV or exit 127' \
  /usr/bin/caffeinate -i /usr/bin/python3 -I "$here/$script" "$@" > "$out" 2>&1 < /dev/null &
echo "$out"
