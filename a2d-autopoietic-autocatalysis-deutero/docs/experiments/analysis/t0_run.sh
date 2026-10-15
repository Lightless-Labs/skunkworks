#!/bin/bash
# T0: one Sol call per staged case, 4 in parallel; each runs with the case dir as cwd.
cd "$1" || exit 1
one() {
  local d="$1"
  [ -s "$d/out.json" ] && return 0
  ( cd "$d" && codex exec "Read PROMPT.md in the current directory and follow it exactly." -m gpt-6.1-sol \
      -c 'model_reasoning_effort="medium"' -c 'approval_policy="never"' --sandbox read-only \
      --skip-git-repo-check --ephemeral --ignore-user-config --ignore-rules -o out.json \
      < /dev/null > codex.log 2>&1; echo "exit=$?" >> codex.log )
  echo "$(date +%H:%M) $d $(tail -1 "$d/codex.log")"
}
for d in */; do
  d="${d%/}"
  while [ "$(jobs -rp | wc -l)" -ge 4 ]; do sleep 2; done
  one "$d" &
done
wait
echo T0_DONE
