# Sealed-pool raw calibration (protocol-v2 `[calibration]`)

**Date:** 2026-10-16. **Run:** `calib-v2`, made with `bench/confirm/confirm.py --calibration` on the 5 calibration tasks in `sealed-sample.json`. These 5 are excluded from the confirmation sample.

**Arm:** raw. That is one `a2ctl run --attempts 1 --lineage-db none` call, issue-only, at effort high, graded by the hardened grader.

| Task | Resolved | Required tests passed | Tokens | Wall |
|---|---|---|---|---|
| e1-850b3f40dc0e | no | 4/6 | 0.19 M | 399 s |
| e1-a6b31e209fc9 | no | 57/61 | 1.00 M | 252 s |
| e1-d066eb06cdec | no | 4/9 | 0.22 M | 184 s |
| e1-e152953579a6 | no | 44/52 | 0.20 M | 268 s |
| e1-e6bde47eddf2 | no | 7/11 | 0.30 M | 517 s |

**Result:** raw resolved 0/5. Under the protocol's pre-stated rule this is a **floor warning**. It is informational, and no instruction or budget change follows.

The pipeline itself works on openlibrary end to end. Every candidate applied, none was rejected, partial credit is real, and there was no infrastructure failure.

Raw resolved 14% of ansible tasks in E1. At that rate, 0 of 5 has probability of about 0.47, so this does not show that the pool sits at the floor for the retry arms, which made about twice raw's rate in E1. The confirmation's power table stands as written.
