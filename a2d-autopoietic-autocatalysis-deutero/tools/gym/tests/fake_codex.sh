#!/bin/sh
# Fake codex for the gym v3 pipeline tests. Never calls a model.
# `--version` prints the pinned version; `exec ...` reads INSTRUCTIONS.md in the cwd and writes
# fixture outputs for the job type it recognises (keyed on the gym's instruction phrases).
if [ "$1" = "--version" ]; then echo "${GYM_TEST_VERSION:-codex-cli 0.162.0}"; exit 0; fi
here=$(dirname "$0")
if [ -n "$CODEX_HOME" ]; then mkdir -p "$CODEX_HOME/skills/.system" "$CODEX_HOME/sessions/2026/10/21"; : > "$CODEX_HOME/sessions/2026/10/21/rollout-fake.jsonl"; fi
echo '{"type":"thread.started"}'
echo '{"type":"turn.started"}'
if grep -q "choose and brief gym families" INSTRUCTIONS.md; then
  if [ -n "$GYM_TEST_FAILSELECT" ]; then exit 1; fi
  total=$(grep -o 'of each class, [0-9]* in all' INSTRUCTIONS.md | grep -o '[0-9][0-9]*' | head -1)
  mkdir -p families; : > families/NAMES.tsv; : > families/CLASSES.tsv
  i=1
  while [ $i -le "$total" ]; do
    n=$(printf '%03d' $i)
    case $(( (i - 1) % 5 )) in 0) c=depth;; 1) c=state;; 2) c=scale;; 3) c=surface;; *) c=edges;; esac
    mkdir -p families/g3-f$n
    printf '# secret-family-name-%s\nBrief %s.\n' "$n" "$n" > families/g3-f$n/FAMILY.md
    printf 'g3-f%s\tsecret-family-name-%s\n' "$n" "$n" >> families/NAMES.tsv
    printf 'g3-f%s\t%s\n' "$n" "$c" >> families/CLASSES.tsv
    i=$((i + 1))
  done
elif grep -q "independent overlap check" INSTRUCTIONS.md; then
  : > VERDICTS.tsv
  for d in families/g3-f*; do
    id=$(basename "$d"); i=$(echo "$id" | sed 's/g3-f0*//')
    if [ "$id" = "g3-f005" ] || [ "$i" -le "${GYM_TEST_CONFLICTS:-0}" ]; then v=CONFLICT; else v=OK; fi
    printf '%s\t%s\n' "$id" "$v" >> VERDICTS.tsv
  done
elif grep -q "author one task family" INSTRUCTIONS.md; then
  fid=$(grep -o 'g3-f[0-9][0-9][0-9]' INSTRUCTIONS.md | head -1)
  cls=$(grep -o 'rubric class is `[a-z]*`' INSTRUCTIONS.md | head -1 | sed 's/.*`\([a-z]*\)`/\1/')
  if [ -n "$GYM_TEST_FAILFID" ] && [ "$GYM_TEST_FAILFID" = "$fid" ]; then exit 1; fi
  flags=""
  if [ -n "$GYM_TEST_DUP" ] && echo " $GYM_TEST_DUP " | grep -q " $fid "; then flags="$flags dup"; fi
  if [ -n "$GYM_TEST_BADRUBRIC" ] && echo " $GYM_TEST_BADRUBRIC " | grep -q " $fid "; then flags="$flags bad-rubric"; fi
  /usr/bin/python3 -I "$here/fake_author.py" author "$fid" "$cls" $flags || exit 1
  if [ -n "$GYM_TEST_SYMLINK" ] && [ "$GYM_TEST_SYMLINK" = "$fid" ]; then
    rm -f out/$fid/$fid-l1/hidden_tests.rs; ln -s /etc/hosts out/$fid/$fid-l1/hidden_tests.rs
  fi
  if [ -n "$GYM_TEST_PROBE" ]; then cat "$GYM_TEST_PROBE" > probe-result.txt 2>&1; fi
  if [ -n "$GYM_TEST_HOLDOUT_PROBE" ]; then ls "$GYM_TEST_HOLDOUT_PROBE" > holdout-probe.txt 2>&1; fi
elif grep -q "adjudicate hidden tests" INSTRUCTIONS.md; then
  /usr/bin/python3 -I "$here/fake_author.py" adjudicate || exit 1
elif grep -q "requirements-only repair" INSTRUCTIONS.md; then
  /usr/bin/python3 -I "$here/fake_author.py" repair || exit 1
elif grep -q "repair rejected tasks" INSTRUCTIONS.md; then
  fid=$(ls out | head -1)
  if [ -n "$GYM_TEST_FIXFAIL" ]; then printf 'BROKEN' > out/$fid/$fid-l1/reference.rs; exit 1; fi
  for t in out/$fid/$fid-l*; do cp "$here/fixture/hidden_tests.rs" "$t/hidden_tests.rs"; done
fi
if [ -n "$GYM_TEST_ORPHAN" ]; then
  /usr/bin/perl -e 'use POSIX; fork and exit; POSIX::setsid(); fork and exit; open STDIN, "</dev/null";
    open STDOUT, ">/dev/null"; open STDERR, ">/dev/null"; %ENV = (); chdir "/"; exec "/bin/sleep", $ARGV[0]' "$GYM_TEST_ORPHAN"
fi
if [ -n "$GYM_TEST_TOOL" ]; then echo '{"type":"item.completed","item":{"type":"mcp_tool_call"}}'; fi
echo '{"type":"item.completed","item":{"type":"agent_message","text":"SECRET-MODEL-TEXT DONE"}}'
echo '{"type":"turn.completed"}'
exit 0
