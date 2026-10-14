# Blocker: No Container Runtime for SWE-Bench Pro Evaluation

**Date:** 2026-10-14
**Blocks:** E1 step 2 (official SWE-Bench Pro container grading)
**Status:** Resolved by a design change (see below). This is not waiting on any outside input.

## Facts

- `colima start` fails with `VZErrorDomain Code=2 "Virtualization is not available on this hardware."` This macOS host is itself a VM (`Apple M4 (Virtual)`, `kern.hv_vmm_present=1`) and has no nested virtualization. No local Docker/OCI runtime is possible. QEMU TCG would mean full software emulation of amd64 on top of software-emulated aarch64, which is unusable.
- All 642 SWE-Bench Pro V2 images are amd64-only (`docs/benchmark-results/external-value/e1/source-verification.md`).
- Free disk is about 12 GiB, not the ~58 GB the plan assumed.
- No Modal or other remote-execution credentials are available to this project, and none are assumed.

The Colima profile was restored to its original 2 CPU / 2 GiB after the failed start.

## Design Change

Grade a SWE-Bench Pro V2 Python subset **natively**, without containers:

1. Create a history-free checkout per task: `git init` + `git fetch --depth 1 <upstream> <base_commit>`. Only the base snapshot is visible; the fix is a descendant commit and is never fetched.
2. Create a per-task venv with `uv` (Python ≥ the repo's `python_requires`) and install the repo's requirements with `--exclude-newer <base commit date>`. This is the native equivalent of the images' `pypi-timemachine` pinning.
3. Grade like the official `test.sh`:
   1. revert agent edits to test paths;
   2. apply `test_patch`;
   3. run `selected_test_files_to_run` with pytest and JUnit output.

   **Resolved** iff `fail_to_pass ∪ pass_to_pass` all pass.
4. **Per-task validity gate (pre-registered):** a task enters the sample only if the gold patch resolves it and the empty patch does not under this native grader. Tasks that fail the gate are excluded before any model call, and the exclusions are recorded.

Spike (ansible `0ea40e09`): empty patch 15/16 required tests passed → not resolved; gold patch 16/16 → resolved. The fetch took 1.8 s (32 MB), venv setup about 3 s, and grading a few seconds.

**Consequences:**
- The grader is not the official Harbor grader. Results are reported as "native-graded SWE-Bench Pro V2 subset", never as SWE-Bench Pro scores.
- Repos needing display stacks or services (qutebrowser, NodeBB) are out of scope at first; ansible and openlibrary come first.
- The agent must stay offline except for the model endpoint (existing `sandbox-exec` `AllowList`, which also blocks GitHub hosts). The dataset's gold patch, test patch and test names never enter agent context.
