---
title: "A Deprecated CLI Flag Can Silently Re-Enable a Tool: Probe Denials, Count Every Tool Kind"
date: 2026-10-17
category: best-practices
module: a2d-providers / a2d-lift / nano / phase-B tooling
problem_type: instrument_integrity
component: provider-isolation
symptoms:
  - "Every live phase-B proposal was voided: 'void tool event(s): error'"
  - "A coder call with the provider's exact no-tools flags answered with a live URL from a web search"
  - "integrity_ok stayed true: the tool-call count sees only `exec` lines"
root_cause: cli_upgrade_deprecated_the_disable_flag_and_turned_the_tool_on_by_default
---

# A Deprecated CLI Flag Can Silently Re-Enable a Tool

**Evidence:** A²D LOG, C2-WEB (2026-10-17). Related: `reviewer-clis-need-filesystem-isolation-2026-10-14.md`, `static-guards-are-not-isolation-use-os-sandbox-2026-10-16.md`, `seatbelt-sandbox-for-compilers-and-test-binaries-2026-10-17.md`.

## What happened
- **The setup:** the A²D provider runs `codex exec` with `--disable web_search_request` and seven other `--disable` flags. On 2026-10-14 it was "verified live" by asking for `ls /`, which checks shell access only.
- **The change:** codex-cli 0.162.0 deprecated the `web_search_request` feature and turned web search **on by default**.
  - The old flag is now ignored. Its only trace is a deprecation notice: an `error` item in `--json` mode, or a stderr line in plain mode.
- **Silent half:** the controller's tool count (`codex_tool_calls`) counts `exec` lines, while a search prints `web search: <query>`. So `integrity_ok` could not see a search.
  - The dev tasks' hidden tests live in this public repo, so a search was a potential leak channel.
- **Loud half:** the phase-B proposer audit fails closed on any unknown item. It voided every proposal, which a live smoke in a throwaway directory caught before the real rounds.
- **Exposure:** every recorded transcript stores full stderr, so a scan bounded it. There were **0 searches in about 15k calls** (dev and holdout, counts only for holdouts). No result was affected.

## Lessons
- **Verify each denial with a probe that asks for that exact capability.** "Run `ls /`" proves nothing about web search.
  - Re-run the probes on every CLI version change. Record the CLI version in each transcript and disclose any mismatch.
- **Count every tool kind the CLI can emit, not the one you expected.** Exec, web search, MCP tool calls, file changes and anything new all count. An exec-only counter is a static guard in disguise.
- **Prefer the documented config key over feature flags,** e.g. `-c web_search="disabled"`, and confirm it with the probe: here the model reported "NO-WEB-TOOL" with no search event.
- **Strict event audits are worth their false alarms.** The fail-closed proposer audit surfaced the problem; the lenient counter hid it.
- **Smoke new tooling live, end to end, in a throwaway directory** before the budgeted run. One smoke cost about 60 Luna and 4 Sol, and saved six rounds of guaranteed-invalid proposals.
- **Store full transcripts.** Because they existed, the exposure question took one count-only scan instead of a rerun.
- **When the registered instrument is frozen, amend by tightening only.**
  - Add a pre-registered, count-only audit where any hit means IntegrityReview, and fix the instrument after the decision.
  - A tightening can only remove a claim, so the existing reviews stand. A loosening would need a re-review.
