# Parked changes

Uncommitted work found in the shared checkout, saved verbatim so nothing is lost. Restore one with
`git apply <patch>` from this project directory.

| Patch | Origin | Why parked |
|---|---|---|
| `2026-07-10-senior-swe-provenance-result-fields.patch` | Pre-audit session, 2026-07-10; it had been left uncommitted | It adds more Senior-SWE-Bench provenance validation to the legacy `bench/self_correction.py` harness. E1 uses a separate driver, and the value-test-first plan freezes this kind of plumbing until E1 reports (`docs/plans/2026-10-14-value-test-first.md`). |
