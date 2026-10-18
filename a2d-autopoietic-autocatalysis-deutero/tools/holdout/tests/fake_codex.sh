#!/bin/sh
# Fake codex for the confirmation-3 pipeline tests. Never calls a model.
# `--version` prints the pinned version; `exec ...` reads INSTRUCTIONS.md in the cwd
# and writes fixture outputs for the job type it recognises.
if [ "$1" = "--version" ]; then echo "${C3_TEST_VERSION:-codex-cli 0.162.0}"; exit 0; fi
here=$(dirname "$0")
echo '{"type":"thread.started"}'
echo '{"type":"turn.started"}'
if grep -q "choose and brief 40" INSTRUCTIONS.md; then
  mkdir -p families; : > families/NAMES.tsv
  i=1
  while [ $i -le 40 ]; do
    n=$(printf '%02d' $i)
    mkdir -p families/c3-f$n
    printf '# secret-family-name-%s\nBrief %s.\n' "$n" "$n" > families/c3-f$n/FAMILY.md
    printf 'c3-f%s\tsecret-family-name-%s\n' "$n" "$n" >> families/NAMES.tsv
    i=$((i + 1))
  done
elif grep -q "independent overlap check" INSTRUCTIONS.md; then
  : > VERDICTS.tsv
  i=1
  while [ $i -le 40 ]; do
    n=$(printf '%02d' $i)
    if [ "$n" = "05" ]; then v=CONFLICT; else v=OK; fi
    printf 'c3-f%s\t%s\n' "$n" "$v" >> VERDICTS.tsv
    i=$((i + 1))
  done
elif grep -q "author one task family" INSTRUCTIONS.md; then
  fid=$(grep -o 'c3-f[0-9][0-9]' INSTRUCTIONS.md | head -1)
  for l in 1 2 3 4; do
    d=out/$fid/$fid-l$l
    mkdir -p $d/mutants
    cp "$here/fixture/public_tests.rs" "$here/fixture/hidden_tests.rs" "$here/fixture/reference.rs" $d/
    for m in 1 2 3 4; do cp "$here/fixture/reference.rs" $d/mutants/m$m.rs; done
    seed="$fid-$l"
    if [ -n "$C3_TEST_DUP" ] && echo " $C3_TEST_DUP " | grep -q " $fid "; then seed="dup-$l"; fi
    words=$(printf '%s' "$seed" | shasum | cut -c1-40 | sed 's/./& /g')
    words="$words $(printf '%s' "$seed" | shasum -a 256 | cut -c1-64 | sed 's/./& /g')"
    printf '{"id": "%s-l%s", "family": "%s", "requirements": "SECRET-REQ %s", "test_timeout_secs": 10}\n' \
      "$fid" "$l" "$fid" "$words" > $d/task.json
  done
  printf 'report\n' > out/$fid/REPORT.md
  if [ -n "$C3_TEST_SYMLINK" ] && [ "$C3_TEST_SYMLINK" = "$fid" ]; then
    rm -f out/$fid/$fid-l1/hidden_tests.rs; ln -s /etc/hosts out/$fid/$fid-l1/hidden_tests.rs
  fi
  if [ -n "$C3_TEST_PROBE" ]; then cat "$C3_TEST_PROBE" > probe-result.txt 2>&1; fi
fi
if grep -q "repair rejected tasks" INSTRUCTIONS.md; then
  fid=$(ls out | head -1)
  if [ -n "$C3_TEST_FIXFAIL" ]; then printf 'BROKEN' > out/$fid/$fid-l1/reference.rs; exit 1; fi
  printf '\n// repaired\n' >> out/$fid/$fid-l4/reference.rs
fi
if [ -n "$C3_TEST_TOOL" ]; then echo '{"type":"item.completed","item":{"type":"mcp_tool_call"}}'; fi
echo '{"type":"item.completed","item":{"type":"agent_message","text":"SECRET-MODEL-TEXT DONE"}}'
echo '{"type":"turn.completed"}'
exit 0
