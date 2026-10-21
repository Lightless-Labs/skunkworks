# Gym: sandboxed() never deletes its probe profiles

**Found:** 2026-10-21 (gym v3 pre-flight). **Priority:** low. **Resolved:** 2026-10-21 (anonymous probes unlink their profile after reap; `test_isolation_checks` asserts none remain).

`gymlib.sandboxed()` writes one `_records/_probe/probe-<pid>-<thread>-<ns>.sb` per call and never
removes it. One `isolation_check` makes about 20 calls (the control, the denial probes, the write
probe, the broker copies), and every job launch runs `isolation_check`. Over a full authoring
campaign that leaves about 10^4 small files under `~/.a2d-gym` (disk was short on 10-21).

Fix: when `record is None`, unlink `prof` in `sandboxed()`'s `finally`, after `reap(tag)` succeeds.
Job profiles (`<record>/job.sb`) stay as the audit trail. Add a test that counts `_probe/*.sb`
before and after `isolation_check`.
