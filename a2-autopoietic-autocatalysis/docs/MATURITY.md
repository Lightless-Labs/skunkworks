# A² Maturity

**Current authoritative state:** `PreStage0`
**Bootstrap profile:** B0 (human approval required)
**Live germline mutation:** disabled

The machine-readable source of current status is:

```bash
cargo run -p a2ctl -- status --json
```

The status model is implemented in `crates/a2ctl/src/maturity.rs`. Documentation does not advance maturity. A transition requires the roadmap's current-HEAD evidence and an explicit code change to the authoritative state.

## States

| State | Meaning |
|---|---|
| `PreStage0` | Current prototype. Mutation-capable live-germline entrypoints are locked out. |
| `Stage0GatePassed` | The exact Phase-4 Bazel suite passed in clean developer and independent CI checkouts with valid operator-mounted root-of-trust and escrow inputs. |
| `Stage1B0` | Phase 5 proved one B0-approved atomic admission, descendant inheritance, independent acceptance, and rollback while Stage-0 remained green. |
| `Stage2B0` | At least 10 predeclared substantive internal tasks met the roadmap's independent acceptance, approval, journaling, inheritance, and no-regression requirements. |

## Missing Evidence

A² currently lacks authoritative evidence for:

- canonical Bazel self-hosting and Cargo/Bazel crate target parity;
- a frozen public mission battery and mission-category floors;
- operator-mounted root-of-trust and hidden-sentinel escrow validation;
- B0-approved atomic Git admission, promotion journaling, descendant inheritance, and rollback;
- digest-checked constitutional semantics with non-vacuous executable invariant checks;
- end-to-end membrane/capability enforcement;
- an independently verified sealed external-value campaign.

The legacy `a2ctl sentinel` command runs six **public developer health checks**. Its compile, test, unsafe-code, clippy, documentation, and lockfile checks remain useful local feedback, but they are not hidden, do not consume operator escrow, and do not establish the Stage-0 gate.

## Mutation Lockout

At `PreStage0`, the CLI refuses every mutation-capable `--apply` path before creating logs, provider processes, worktrees, lineage databases, or changing Git state:

- `task --apply`
- `run --apply`
- `autopilot --apply`
- `autopilot-resident --apply`
- `scan --run --apply`

The lockout remains until both the Phase-4 Stage-0 gate and Phase-5 B0 end-to-end trace pass. Candidate-only, dry-run, status, and public health-check paths remain available.
