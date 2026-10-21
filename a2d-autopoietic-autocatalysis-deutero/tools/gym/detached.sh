#!/bin/sh
# Start one gym v3 pipeline driver fully detached: its own session (setsid), no terminal, kept
# awake by caffeinate (copy of tools/holdout/detached.sh with the gym's own allowlist and log dir).
# A Claude Code background shell can be reaped under memory pressure, and a reaped driver means
# the next driver's recovery kills its in-flight jobs (their attempts are spent).
#
# The driver's stdout carries only gateway lines (counts and opaque ids). It goes to
# ~/.a2d-gym-drivers/<UTC>-<script>.out, outside the gym root and the confirmation-3 dirs.
# Every driver refuses to launch while the confirmation-3 isolation gate is closed.
#
# usage: tools/gym/detached.sh gym_author.py --parallel 3
set -eu
here=$(cd "$(dirname "$0")" && pwd)
script=${1:?script}
shift
case "$script" in
  gym_select.py|gym_author.py|gym_assemble.py|gym_similarity.py|gym_validate.py|gym_fix.py|gym_audit.py|gym_adjudicate.py|gym_pilot.py) ;;
  *) echo "unknown script" >&2; exit 2 ;;
esac
logs="$HOME/.a2d-gym-drivers"
mkdir -p "$logs"
out="$logs/$(date -u +%Y%m%dT%H%M%SZ)-${script%.py}.out"
nohup /usr/bin/perl -e 'use POSIX qw(setsid); setsid(); exec @ARGV or exit 127' \
  /usr/bin/env -u A2D_HOLDOUT_DIR /usr/bin/caffeinate -i /usr/bin/python3 -I "$here/$script" "$@" > "$out" 2>&1 < /dev/null &
echo "$out"
