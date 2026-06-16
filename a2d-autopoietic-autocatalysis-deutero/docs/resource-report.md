# Resource Report — A²D Project Sessions

> **Note:** Claude Code session logs are global (not project-scoped), so resources from related and unrelated projects both appear. I've separated them accordingly.

---

## A²D-Specific Resources

### AI Models & Providers

| Model | Provider | ID |
|-------|----------|----|
| gpt-5.4, o3, o4-mini | OpenAI / Codex | — |
| Gemini 3 Pro / 3.1 Pro | Google | `gemini-3.1-pro-preview` |
| Kimi k2.5 | OpenCode | `kimi-for-coding/k2p5` |
| Kimi k2.7 code | OpenCode | `kimi-k2.7-code` |
| GLM 5.1 | OpenCode | `zai-coding-plan/glm-5.1` |
| GLM 5.2 | OpenCode | `zai-coding-plan/glm-5.2` |
| Minimax M2.7 | OpenCode | `minimax-coding-plan/MiniMax-M2.7` |
| Minimax 3 | OpenCode | exact alias may vary; A²D recognizes provisional opt-in aliases `minimax-coding-plan/MiniMax-3`, `minimax-coding-plan/Minimax-3`, `minimax-coding-plan/MiniMax-M3` |
| Kimi k2.7 | Pi | `pi/kimi-coding/k2p7` |
| Minimax 3 | Pi | `pi/minimax/MiniMax-M3` |
| GLM 5.2 | Pi | `pi/zai/glm-5.2` |
| Claude Opus 4.6 / Sonnet 4.6 | Anthropic | — |

### External Systems & Architectures Referenced

- **OpenAI Symphony** — Elixir/BEAM issue-to-PR orchestrator; patterns: workspace isolation, proof-of-work verification, exponential backoff
- **Gas Town** (Steve Yegge) — Go-based, 20-30+ parallel Claude Code agents, 17-day PoC; patterns: Beads (git/JSONL/SQLite), GUPP principle, topological task dispatch, Witness/Deacon separation, git worktrees
- **StrongDM Factory/Attractor** — 3-person team; patterns: holdout scenarios, NLSpec-first, Gene Transfusion, Digital Twin Universe (DTU), Pyramid Summaries
- **Waymo** — Referenced as Level 3 maturity (autonomous with safety driver)

### Persons Referenced

- **Steve Yegge** — Gas Town architect
- **Sam Schillace** (Microsoft) — Compounding Teams
- **Dan Shapiro** — Maturity scale
- **Gregory Bateson** — Deutero-learning / second-order cybernetics
- **Luke PM** — Architecture-code matching as CI
- **Hordijk & Steel** — RAF algorithm authors

### Theoretical Frameworks

- Autopoiesis (Maturana & Varela)
- Reflexively Autocatalytic Food Sets (RAF) — Hordijk-Steel polynomial-time algorithm
- Von Neumann self-reproduction
- Deutero-learning (Bateson)
- Wheeler's DDC Principle (Diverse Dependable Computing)
- Marginal Value Theorem (MVT) — optimal foraging, empirically violated at 0.25 ratio
- Hidden Semi-Markov Model (HSMM) — State 3 pre-failure, 24.6x hazard lift
- Borda Count — consensus scoring method

### Sibling Projects (Internal Knowledge Sources)

- **A²** — `~/Projects/lightless-labs/skunkworks/a2-autopoietic-autocatalysis/` — theoretical foundations
- **Agentic Engineering Sawdust** — `~/Projects/Banade-a-Bonnot/agentic-engineering-sawdust/` — 253 empirical learnings, 4.5GB sessions
- **Third Thoughts** — `~/Projects/lightless-labs/third-thoughts/` — 23 statistical techniques, 7,909 sessions
- **Foundry** — `~/Projects/lightless-labs/public/foundry/` — adversarial blind verification
- **Converge-Refinery** — `~/Projects/Banade-a-Bonnot/converge-refinery/` — multi-model convergence (Lamarckian/Darwinian/brainstorm modes)
- **Middens** (Rust CLI) — distributed agent session collection (SETI@Home-style)

### Benchmarks & Challenges

- **Sudoku** — 6 hidden holdout acceptance tests; A²D baseline: 83%, single-model baselines: 100%
- **Chess** — 4–9 tests (Scholar's Mate, castling, en passant, orientation-agnostic)
- **Rubik's Cube** — scramble/solve roundtrip

---

## Resources from Session Logs (Broader Ecosystem)

These appeared in session logs but relate to other projects in the Bande-à-Bonnot / Lightless Labs ecosystem.

### GitHub Repositories

- `Bande-a-Bonnot/Boucle-framework` — git hook enforcement & safety tools (bash-guard, branch-guard, file-guard, git-safe, read-once, session-log, worktree-guard, enforce, safety-check)
- `anthropics/claude-code` — 60+ issues tracked; security advisory `GHSA-4q92-rfm6-2cqx`

### External Documentation URLs

- https://bazelbuild.github.io/rules_rust/crate_universe_bzlmod.html
- https://developer.apple.com/documentation/authenticationservices
- https://developer.apple.com/documentation/security/keychain_services
- https://developer.apple.com/documentation/sign_in_with_apple/sign_in_with_apple_rest_api
- https://auth0.com/docs/secure/tokens/refresh-tokens/refresh-token-rotation
- https://cliffle.com/blog/rust-typestate/
- https://learn.microsoft.com/en-us/powershell/scripting/install/installing-powershell-on-windows

### Other Projects Referenced in Sessions

- `agentic-linear`, `converge-refinery`, `infinidash`, `JASONETTE-Reborn`, `parsiweb-previews`, `kumbaya`, `weatherby`, `phil-connors`, `ten-a-day`

### Services & Infrastructure

- **Cloud:** AWS (DynamoDB, Lambda, EventBridge, CloudFormation, API Gateway, IAM), Vercel, Cloudflare
- **CI/CD:** Buildkite, GitHub Actions
- **Tracking:** Linear, GitHub
- **Auth:** Sign In with Apple, Auth0, JWT/OAuth
- **Databases:** DynamoDB (primary), SQLite, PostgreSQL, MongoDB

---

> No external academic paper URLs were found — all academic references are by name/citation only.
> The A²D project documentation contains no hyperlinks; everything is referenced by name or local path.
