# Agentic Software Engineering: State of the Art

**Created:** 2026-04-01
**Scope:** Comprehensive survey of autonomous/agentic software engineering systems, benchmarks, multi-agent approaches, self-improving code systems, and remaining gaps.

---

## 1. Current Systems

### 1.1 Devin (Cognition Labs)

**Architecture:** Sandboxed workspace containing a shell, code editor, browser, and persistent file system. Devin independently plans, executes, and iterates on engineering tasks requiring thousands of decisions. It recalls relevant context at each step and learns over time.

**Capabilities:**
- Excels at tasks with clear, upfront requirements and verifiable outcomes (4-8 hr junior engineer scope)
- Migrating/modernizing repos, fixing vulnerabilities (SonarQube/Veracode), writing unit tests, completing small tickets
- Infinitely parallelizable -- Goldman Sachs deployed it as "Employee #1" in a hybrid workforce
- Integrated with Slack, Teams, Jira for collaboration
- 20x efficiency gain reported for vulnerability remediation (30 min human vs 1.5 min Devin)

**Performance (2025-2026):**
- 4x faster at problem solving vs 2024; 2x more efficient in resource consumption
- 67% PR merge rate (up from 34%)
- SWE-bench: 13.86% resolution (7x improvement over prior AI baselines of 1.96%)
- Devin 2.0 completes 83% more junior-level tasks per Agent Compute Unit vs 1.x
- Only 15% complex task completion without human help

**Limitations:**
- "Senior-level at codebase understanding but junior at execution"
- Struggles with complex code, unnecessary abstractions, inconsistent task performance
- Cannot manage stakeholders, handle ambiguity, or deal with interpersonal dynamics
- Pricing dropped from $500/mo to $20/mo with Devin 2.0 (suggests commoditization pressure)

**Sources:** [Cognition 2025 Performance Review](https://cognition.ai/blog/devin-annual-performance-review-2025), [VentureBeat: Devin 2.0](https://venturebeat.com/programming-development/devin-2-0-is-here-cognition-slashes-price-of-ai-software-engineer-to-20-per-month-from-500/)

---

### 1.2 OpenHands (formerly OpenDevin)

**Architecture:** Modular SDK with event-sourced state model, deterministic replay, immutable agent configuration, and typed tool system with MCP integration. V1 refactored from V0's monolithic sandbox-centric design into reusable agent, tool, and workspace packages. Workspace abstraction enables same agent to run locally or remotely in secure containers.

**Agent types:**
- **CodeActAgent:** Generalist code-writing and debugging
- **BrowserAgent:** Web navigation and web-based task execution
- **Micro-agents:** Lightweight agents from natural language or minimal interface demonstrations
- Hierarchical agent structures with delegation primitives

**Capabilities:**
- Full environment interaction: write code, interact with command line, browse web
- Evaluation harness supporting 15+ benchmarks (SWE-bench, HumanEvalFix, ML-Bench, WebArena, MiniWoB++, GAIA, GPQA, etc.)
- MIT licensed, 64k+ GitHub stars, 188+ contributors, 2.1k+ contributions

**Key differentiator:** Open-source platform play -- the "Linux of agentic coding." Composable SDK lets anyone build and deploy agents at scale.

**Sources:** [OpenHands Platform](https://openhands.dev/), [arXiv: OpenHands Paper](https://arxiv.org/abs/2407.16741), [OpenHands SDK Paper](https://arxiv.org/html/2511.03690v1)

---

### 1.3 SWE-Agent (Princeton/Stanford)

**Architecture:** Custom Agent-Computer Interface (ACI) -- LM-centric commands and feedback formats designed to make it easier for the LM to browse repositories, view/edit/execute code files. Key insight: the *interface design* matters as much as the model.

**Performance:**
- SWE-bench full: 12.29% resolution (state-of-the-art at time of publication)
- HumanEvalFix: 87.7% pass@1 (far exceeding prior non-interactive LMs)
- Mini-SWE-Agent (100-line version): >74% on SWE-bench Verified
- Published at NeurIPS 2024

**Key insight:** Simple, well-designed interfaces beat complex agent frameworks. The ACI concept -- making computers easier for LLMs to use -- is a foundational contribution.

**Sources:** [SWE-Agent GitHub](https://github.com/SWE-agent/SWE-agent), [arXiv: SWE-Agent](https://arxiv.org/abs/2405.15793), [Mini SWE-Agent](https://github.com/SWE-agent/mini-swe-agent)

---

### 1.4 Factory AI

**Architecture:** "Droids" -- LLM-agnostic, interface-agnostic agents that work from terminal, IDE, Slack, Linear, browsers, or custom scripts. Focus on enterprise workflow integration and full SDLC automation.

**Capabilities:**
- Autonomous feature development, migrations, modernization, code review, testing
- Pulls context, implements solutions, creates PRs with full traceability (ticket-to-code)
- Customers include MongoDB, Ernst & Young, Zapier, Bayer
- 200% QoQ growth in 2025; $50M Series B (NEA, Sequoia, Nvidia, JPMorgan)

**Differentiator:** Enterprise-first approach -- deep integration with existing workflows (Jira, Linear, etc.) rather than standalone agent. Focus on "agent-native development" paradigm where agents are first-class participants in the SDLC.

**Sources:** [Factory AI](https://factory.ai), [NEA: Factory Platform](https://www.nea.com/blog/factory-the-platform-for-agent-native-development), [SiliconANGLE: Factory Droids](https://siliconangle.com/2025/09/25/factory-unleashes-droids-software-agents-50m-fresh-funding/)

---

### 1.5 Cursor Agent Mode

**Architecture:** Mixture-of-experts (MoE) language model specialized for SE through RL in diverse development environments (Composer model). Pipeline structure:
- **Planners** continuously explore codebase, create tasks with parallel sub-planners
- **Workers** pick up tasks independently without cross-coordination
- **Judge agent** determines whether to continue
- Up to 8 agents simultaneously via Git worktree isolation
- Generates at 250 tokens/sec with tools for file read/edit, terminal commands, codebase-wide semantic search

**Capabilities:**
- Autonomous multi-file refactors, terminal command execution, repository-wide changes
- Has experimented with hundreds of concurrent agents on a single project, writing 1M+ lines of code
- Running coding agents autonomously for weeks at a time

**2026 direction:** "Self-driving codebases" concept -- AI systems managing larger portions of code maintenance and evolution autonomously.

**Sources:** [Cursor Agent](https://cursor.com/product), [Cursor: Scaling Agents](https://cursor.com/blog/scaling-agents), [TechCrunch: Cursor Agentic Coding](https://techcrunch.com/2026/03/05/cursor-is-rolling-out-a-new-system-for-agentic-coding/)

---

### 1.6 Claude Code (Anthropic)

**Architecture:** Single-threaded master loop (codenamed "nO") -- `while(tool_call) -> execute tool -> feed results -> repeat`. Built with Bun, CommanderJS, React Ink for terminal rendering.

Key design decisions:
- **Single flat message history** -- explicitly avoids threaded conversations or competing agent personas
- **Real-time steering** via async dual-buffer queue ("h2A") -- users can inject instructions mid-task
- **Context compression** ("Compressor wU2") triggers at ~92% context utilization, summarizes and moves info to markdown-based project memory
- **Sub-agent spawning** via `dispatch_agent` tool with depth limits (no recursive sub-agent explosion)
- **Tool Search Tool** -- access thousands of tools without consuming context window

**Capabilities:**
- Full terminal integration: read/write/execute code, git workflows, natural language commands
- Multi-agent orchestration: main agent + explore agents for codebase analysis
- MCP integration for extensibility
- Surpassed $1B annualized revenue by Nov 2025, ~$2B run rate by early 2026

**Key differentiator:** The leaked system prompt revealed a remarkably clean architecture. The simplicity of the single-threaded loop with controlled sub-agent spawning is a counterpoint to more complex multi-agent designs.

**Sources:** [Claude Code Docs](https://code.claude.com/docs/en/overview), [Claude Code GitHub](https://github.com/anthropics/claude-code), [ZenML: Claude Code Architecture](https://www.zenml.io/llmops-database/claude-code-agent-architecture-single-threaded-master-loop-for-autonomous-coding)

---

### 1.7 OpenAI Codex CLI

**Architecture:** Two execution modes:
1. **Cloud sandbox** preloaded with your repository for parallel background tasks
2. **Terminal-first CLI** running locally with granular approval controls

Three approval levels:
- **Suggest** -- proposes changes, asks before applying
- **Auto Edit** (workspace-write) -- can read/edit within workspace, run routine commands
- **Full Auto** (danger-full-access) -- no sandbox restrictions

Powered by codex-1 (o3 variant optimized for SE), then GPT-5.3-Codex, now GPT-5.4. Ships with first-party web search tool. Supports AGENTS.md and MCP for repo customization.

**Capabilities:**
- Local agent-style coding over real repositories
- Iterative review and apply edits with human oversight
- Orchestration via OpenAI Agents SDK
- Open-source CLI component

**Sources:** [OpenAI: Introducing Codex](https://openai.com/index/introducing-codex/), [Codex CLI GitHub](https://github.com/openai/codex), [Pragmatic Engineer: How Codex is Built](https://newsletter.pragmaticengineer.com/p/how-codex-is-built)

---

### 1.8 Google Gemini CLI

**Architecture:** ReAct (Reason and Act) loop with built-in tools and MCP server support. Open source. Powered by Gemini 2.5 Pro with 1M token context window.

**Capabilities:**
- Code understanding, file manipulation, command execution, troubleshooting
- Google Search grounding for real-time external context
- MCP extensibility and bundled extensions
- Non-interactive invocation for automation/scripting
- Generous free tier: 60 requests/min, 1000 requests/day

**Differentiator:** Massive context window (1M tokens) enables handling entire large codebases. Free tier makes it accessible for experimentation. Open source.

**Sources:** [Google: Gemini CLI](https://blog.google/innovation-and-ai/technology/developers-tools/introducing-gemini-cli-open-source-ai-agent/), [Gemini CLI GitHub](https://github.com/google-gemini/gemini-cli)

---

## 2. Benchmarks

### 2.1 SWE-bench Family

**SWE-bench (Original):** 2,294 real GitHub issues from 12 Python repos. Tests end-to-end issue resolution. Early results: ~2% for base models, ~12% for SWE-Agent.

**SWE-bench Verified (500 tasks, Python-only):**
- Top score: Claude Opus 4.5 at 80.9%
- **CONTAMINATED** -- OpenAI's audit found every frontier model could reproduce verbatim gold patches. OpenAI has stopped reporting Verified scores.

**SWE-bench Pro (1,865 tasks, multi-language):**
- GPT-5.4: 57.7%
- GPT-5.3-Codex: 56.8%
- Claude Opus 4.5: 45.9% (SEAL standardized scaffolding) to 55.4% (best scaffold)
- Critical finding: same model (Opus 4.5) scored 50.2% to 55.4% depending on scaffold -- **agent architecture matters as much as model capability**

**SWE-bench Live:** Continuously updated with new issues to prevent contamination.

**SWE-rebench:** Alternative evaluation; Claude Opus 4.6 claimed #1 spot.

### 2.2 HumanEval / MBPP

**HumanEval:** 164 Python programming problems.
- Top score: Kimi K2 0905 at 94.5%; average across models: 80.9%
- Essentially saturated for frontier models

**MBPP (Mostly Basic Programming Problems):** 974 crowd-sourced problems.
- Similarly approaching saturation for frontier models

**HumanEval Pro / MBPP Pro (2025):** Self-invoking code generation -- must solve base problem then use it to solve harder problem.
- o1-mini: 96.2% on HumanEval but only 76.2% on HumanEval Pro
- Reveals that progressive reasoning remains a significant gap

### 2.3 Aider Polyglot

Tests coding ability across C++, Go, Java, JavaScript, Python, Rust through 225 Exercism problems. Two attempts per problem (second attempt includes test feedback).

- GPT-5: 88.0%
- Refact.ai Agent + Claude 3.7 Sonnet: 92.9% (agentic scaffold)
- Average across all models: 58.1%

### 2.4 LiveCodeBench

Contamination-free benchmark collecting fresh problems from LeetCode, AtCoder, CodeForces. Tests self-repair, code execution, test output prediction beyond just generation.

- Gemini 3 Pro Preview: 91.7%
- Gemini 3 Flash Preview: 90.8%
- DeepSeek V3.2 Speciale: 89.6%
- 201 models evaluated; 1000+ problems in v6

**LiveCodeBench Pro:** Problems from Codeforces rated rounds, ICPC, IOI with Olympiad medalist vetting.

### 2.5 Benchmark Critique

**What benchmarks miss:**
- Real-world software engineering involves ambiguous requirements, multi-stakeholder coordination, architectural decisions -- none of which are tested
- SWE-bench Verified contamination undermines the most-cited benchmark
- Scaffold variance (5+ percentage points for same model) means we're partly benchmarking agent design, not just model capability
- No benchmarks for long-horizon tasks (days/weeks of work), codebase-scale refactoring, or production deployment
- No benchmarks for understanding *why* code should change (business context, user impact)

**Sources:** [SWE-bench](https://www.swebench.com/), [Morph: SWE-bench Pro](https://www.morphllm.com/swe-bench-pro), [Aider Leaderboards](https://aider.chat/docs/leaderboards/), [LiveCodeBench](https://livecodebench.github.io/), [Epoch AI](https://epoch.ai/benchmarks/swe-bench-verified)

---

## 3. Multi-Agent Approaches

### 3.1 MetaGPT

**Architecture:** Assigns LLM agents to roles mimicking a software company (Product Manager, Architect, Engineer, QA). Agents communicate through structured artifacts (PRDs, design docs, code, test plans) rather than free-form chat. Key innovation: "meta-programming" -- using human procedural knowledge (SOPs) to structure agent collaboration.

- Launched MGX (MetaGPT X) in Feb 2025: "world's first AI agent development team"
- AFlow paper (automating agentic workflow generation) accepted as oral at ICLR 2025 (top 1.8%)

**What works:** Structured communication through artifacts reduces role confusion. SOP-driven workflows provide guardrails.

**What doesn't:** Can still produce incoherent designs when tasks exceed model context. Heavy framework overhead.

### 3.2 ChatDev

**Architecture:** Virtual software company simulation. Agents hold organizational roles (CEO, CTO, Programmer, Tester, Reviewer) and collaborate through structured dialogues.

**What works:** The dialogue-based approach produces reasonable software for small, well-defined tasks.

**What doesn't:** Role confusion, duplicated efforts, repetitive/unproductive exchanges. Does not scale to real-world codebase complexity.

### 3.3 AgentCoder

**Architecture:** Streamlined three-agent design:
1. **Programmer agent** -- generates code
2. **Test designer agent** -- creates test cases
3. **Test executor agent** -- runs and validates

**Advantages:** Significantly lower token overhead than MetaGPT or ChatDev. The simplicity is the point -- separation of generation from validation is the key structural insight.

### 3.4 MASAI (Microsoft)

**Architecture:** Modular Architecture for Software-engineering AI agents. Different LLM-powered sub-agents are instantiated with well-defined objectives and strategies tuned to those objectives.

**Key advantages:**
1. Different problem-solving strategies per sub-agent
2. Sub-agents gather information from different sources across a repository
3. Avoids unnecessarily long trajectories (cost and context inflation)

**Performance:** 28.33% on SWE-bench Lite at <$2/issue average cost. Published as poster at NeurIPS 2024 Workshop on Open-World Agents.

### 3.5 Agentless

**Architecture:** NOT an agent at all. Three-phase pipeline: localization -> repair -> patch validation. The LLM performs each task but does NOT decide future actions or use complex tools.

**Performance:**
- SWE-bench Lite: 27.33% (best open-source at time of publication)
- With Claude 3.5 Sonnet: 40.7% (Lite), 50.8% (Verified)
- Average cost: $0.34 per issue

**Key insight:** A simple pipeline without autonomous agent behavior can match or beat complex agent frameworks. This is the strongest argument against over-engineering agent architectures.

### 3.6 When Does Multi-Agent Outperform Single-Agent?

Evidence is mixed:
- Multi-agent shines when tasks naturally decompose into independent sub-problems with clear interfaces
- For well-defined, bounded tasks, single-agent or even agentless approaches are competitive or superior
- Multi-agent coordination overhead can exceed benefits for simpler tasks
- The critical factor is **scaffold design** (how agents communicate and share context), not the number of agents

**Sources:** [MetaGPT GitHub](https://github.com/FoundationAgents/MetaGPT), [MetaGPT Paper](https://arxiv.org/abs/2308.00352), [AgentCoder Paper](https://arxiv.org/pdf/2312.13010), [MASAI Paper](https://arxiv.org/abs/2406.11638), [Agentless Paper](https://arxiv.org/abs/2407.01489)

---

## 4. Self-Improving Code Systems

### 4.1 AlphaCode / AlphaCode 2 (DeepMind)

**Architecture:** Generate-then-filter approach:
1. Generate millions of diverse programs using transformer-based networks
2. Filter and cluster programs
3. Select max 10 submissions per problem

Self-supervised learning with encoder-decoder transformer. Achieved approximately human-level performance on Codeforces competitive programming.

**AlphaCode 2** uses Gemini models as the backbone with improved filtering/selection, reaching ~85th percentile on Codeforces.

### 4.2 FunSearch (DeepMind, 2023)

**Architecture:** LLM + evolutionary search. Self-improving loop:
1. **Selection:** Choose best programs from database
2. **Generation:** LLM proposes improved version
3. **Evaluation:** Automatically execute and score
4. **Update:** Add to database if improved, maintaining diversity

**Key result:** Discovered genuinely new mathematical constructions for the cap set problem, surpassing known human solutions. First LLM-based system to make a novel scientific discovery.

**Critical design principle:** The LLM is the *mutation operator* in an evolutionary algorithm. Quality comes from the evolutionary scaffold, not the LLM alone.

### 4.3 AlphaEvolve (DeepMind, 2025)

**Architecture:** Evolutionary coding agent that pairs Gemini models with automated evaluators. General-purpose system (unlike domain-specific predecessors).

**How it works:**
1. LLM produces variants of existing algorithms
2. Automated evaluators verify/score variants
3. Evolutionary framework selects most effective ones
4. Repeat

**Key achievements:**
- Improved Strassen's 1969 matrix multiplication algorithm (4x4 complex-valued matrices: 48 scalar multiplications)
- Across 50 open mathematical problems: rediscovered SOTA 75% of the time, found improved solutions 20% of the time
- **Self-referential application:** Enhanced efficiency of Google's data centers, chip design, and AI training (including training the LLMs underlying AlphaEvolve itself)
- 0.7% recovery of Google's worldwide compute resources (in production for 1+ year)

**Significance:** First general-purpose evolutionary coding agent achieving real-world impact. The self-referential property (improving its own infrastructure) is the closest thing to genuine recursive self-improvement demonstrated to date.

### 4.4 Sol-Ver: Self-Play for Code (2025)

Self-play solver-verifier framework that jointly improves a single model's code generation and test generation:
- LLM-as-a-solver generates code
- LLM-as-a-verifier generates tests
- Iterative refinement of both without human annotations or teacher models

### 4.5 Self-Debugging / Self-Repair

Active research area (2024-2025):
- **Self-Debugging (ICLR 2024):** LLM debugs its own predictions via few-shot demonstrations, performing "rubber duck debugging" by investigating execution results and explaining code in natural language
- **Self-Debugging with Self-Generated Tests (ACL 2025):** Post-execution self-debugging struggles with test bias; in-execution self-debugging mitigates bias via intermediate execution states
- **BESTER (2024):** Best-first tree search with self-reflections for code debugging; achieved SOTA Pass@1 on three benchmarks

### 4.6 Systems That Modify Their Own Source Code

No production system truly modifies its own source code in a recursive self-improvement loop. The closest examples:
- **AlphaEvolve** improving Google's training infrastructure (indirect self-improvement)
- **Karpathy's autoresearch** modifying its own training code (bounded, supervised)
- Various self-play frameworks improving model weights through generated training data

The "bootstrapping compiler" analogy remains aspirational. True recursive self-improvement requires a system that can improve its *own reasoning capability*, not just optimize parameters or infrastructure.

**Sources:** [AlphaCode Paper](https://www.science.org/doi/10.1126/science.abq1158), [AlphaEvolve Blog](https://deepmind.google/blog/alphaevolve-a-gemini-powered-coding-agent-for-designing-advanced-algorithms/), [AlphaEvolve Paper](https://arxiv.org/abs/2506.13131), [Sol-Ver Paper](https://arxiv.org/abs/2502.14948), [Self-Debug Paper](https://arxiv.org/abs/2304.05128)

---

## 5. The Gap: What's Missing for a Fully Autonomous Software Factory

### 5.1 Hard Empirical Evidence

**The METR Productivity Paradox (2025):**
A randomized controlled trial with 16 experienced open-source developers found that AI tools made developers **19% slower** on real tasks in their own repositories. Key findings:
- Developers expected 24% speedup; believed AI had helped by 20% even after experiencing slowdown
- <44% of AI generations were accepted
- Most time wasted on reviewing, testing, modifying AI-generated code
- "Tacit knowledge" about the codebase that AI tools couldn't access was a major factor

This is the most rigorous study to date and should give pause to "10x developer" claims.

### 5.2 Unsolved Technical Problems

**Long-horizon planning and coherence:**
Current agents struggle with tasks that span more than a few hours. Maintaining architectural coherence across thousands of file changes requires planning capabilities that no current system demonstrates. Agents that run for weeks (as Cursor experiments with) need fundamentally different state management than single-task agents.

**Architectural decision-making:**
No system can reason about trade-offs between architectural approaches (monolith vs microservices, eventual consistency vs strong consistency, etc.) in the context of a specific business. This requires understanding of non-functional requirements, future scaling needs, team capabilities, and organizational constraints.

**Understanding business requirements:**
Translating vague stakeholder requirements into precise technical specifications is an unsolved problem. Current systems need clear, unambiguous inputs to function well (Devin's "15% complex task completion" reflects this).

**Testing beyond unit tests:**
AI agents can generate unit tests but struggle with integration tests, end-to-end tests, performance tests, chaos engineering, and -- critically -- knowing *what to test*. The validation problem: "how can you prove that software works if both the implementation and the tests are being written by coding agents?"

**Deployment and operations:**
No current system handles the full loop from code to production: CI/CD configuration, infrastructure provisioning, monitoring setup, incident response, capacity planning.

**Security:**
AI-generated code introduces novel attack surfaces. No current system can reason about security implications holistically (threat modeling, access control design, data flow analysis for privacy compliance).

**Continual long-term memory:**
Working memory (context window) has grown to 1M+ tokens, but genuine long-term learning from production feedback, previous failures, and organizational knowledge requires a breakthrough that "business-as-usual engineering" may not achieve.

**Handling ambiguity and "feel":**
Headless autonomous tools struggle with aesthetic and experiential quality -- layout, animations, UX polish. "Translating visual intent into text paragraphs" is an unsolved human-machine communication problem.

### 5.3 Systemic Challenges

**The "Almost Right" problem:**
AI-generated code compiles and passes tests but contains subtle logic errors that require line-by-line review without the context of having written it. Average PR sizes increased 150%, with a 9% rise in bug counts.

**Code review bottleneck:**
Even if agents produce code 10x faster, human review capacity does not scale. This creates a new bottleneck that could negate productivity gains.

**Coordination across repos and teams:**
Real software engineering involves cross-team dependencies, API contracts, migration coordination, backward compatibility -- all requiring social/organizational context that no agent possesses.

**Learning from production:**
No system closes the feedback loop from production incidents back to code changes. An autonomous factory needs to observe production behavior, correlate issues to code, and self-correct.

### 5.4 What Level 5 Autonomy Requires

A fully autonomous software factory would need:
1. **Specification synthesis** -- derive precise specs from vague inputs
2. **Architectural reasoning** -- make and justify design trade-offs
3. **Long-horizon execution** -- maintain coherence over weeks/months of work
4. **Self-verification** -- prove correctness beyond test coverage
5. **Production awareness** -- observe and respond to runtime behavior
6. **Organizational learning** -- accumulate knowledge across projects and teams
7. **Risk assessment** -- know when to ask for human input vs proceed autonomously
8. **Symbolic reasoning integration** -- combine statistical generation with logical verification

None of these are fully solved. Most are not close.

**Sources:** [METR Study](https://metr.org/blog/2025-07-10-early-2025-ai-experienced-os-dev-study/), [MIT News: Roadblocks to Autonomous SE](https://news.mit.edu/2025/can-ai-really-code-study-maps-roadblocks-to-autonomous-software-engineering-0716), [Builder.io: AI Software Engineer in 2026](https://www.builder.io/blog/ai-software-engineer)

---

## 6. Karpathy's Autoresearch

### 6.1 The Pattern

Andrej Karpathy released autoresearch in early 2025 as an open-source tool for autonomous ML experimentation. The core idea: **hill-climbing for knowledge work**.

### 6.2 Architecture: Three Files, One Metric

| File | Owner | Purpose |
|------|-------|---------|
| `prepare.py` | Immutable | Data preparation, tokenizer (8192-token BPE), evaluation metric (val_bpb). Neither human nor agent touches this. Guarantees consistent measurement. |
| `train.py` | Agent | 630-line sandbox: GPT model architecture, Muon+AdamW optimizer, full training loop. Agent can rewrite anything here. |
| `program.md` | Human | Markdown document carrying three registers: instructions (search directions), constraints (invariants), stopping criteria. |

### 6.3 The Loop

```
while not stopping_criteria:
    1. Agent reads program.md for direction
    2. Agent modifies train.py (swap activations, restructure attention, change LR schedule, etc.)
    3. Training runs for exactly 5 minutes (fixed compute budget)
    4. Evaluate val_bpb
    5. If improved: keep change (git commit). If not: revert.
    6. Log everything.
```

**Throughput:** ~12 experiments/hour, ~100 experiments overnight on a single GPU.

### 6.4 Results

- Found ~20 additive improvements that transferred to larger models
- Dropped "Time to GPT-2" from 2.02 hours to 1.80 hours (11% efficiency gain on an already well-tuned project)
- Shopify CEO Tobi Lutke: 19% validation score improvement from 37 experiments on a 0.8B parameter model, results in one night

### 6.5 Design Principles

- **One GPU, one file, one metric** -- radical simplicity
- **Fixed compute budget** -- every experiment costs the same, enabling fair comparison
- **Markdown as contract** -- the program.md is the human-agent interface, carrying instructions, constraints, and stopping criteria in a single document
- **Additive improvement** -- changes are kept only if they improve the metric, creating a monotonically improving trajectory

### 6.6 Broader Significance

Karpathy frames this as the beginning of the "self-improvement loopy era" of AI. The next step: **asynchronously massively collaborative** -- not emulating a single PhD student, but an entire research community.

The pattern generalizes beyond ML training: any domain with a clear metric, a fast feedback loop, and a modifiable codebase can use this approach. It is essentially **evolutionary search with an LLM as the mutation operator** -- conceptually identical to FunSearch/AlphaEvolve but democratized to a single GPU.

### 6.7 Related: Sakana AI's AI Scientist

**AI Scientist v2 (2025):** End-to-end agentic system that generates hypotheses, runs experiments, analyzes data, and writes scientific papers. Uses progressive agentic tree search guided by an experiment manager agent.

- First AI-generated paper accepted through blind peer review (ICLR 2025 ICBINB workshop, average score 6.33)
- Published in Nature
- Limitations: poor novelty assessment (misclassifying established concepts as novel), 42% experiment failure rate due to coding errors

**Sources:** [Karpathy/autoresearch GitHub](https://github.com/karpathy/autoresearch), [Fortune: Karpathy Loop](https://fortune.com/2026/03/17/andrej-karpathy-loop-autonomous-ai-agents-future/), [VentureBeat: Autoresearch](https://venturebeat.com/technology/andrej-karpathys-new-open-source-autoresearch-lets-you-run-hundreds-of-ai), [The New Stack: 630-line Script](https://thenewstack.io/karpathy-autonomous-experiment-loop/), [Sakana AI: AI Scientist](https://sakana.ai/ai-scientist/), [AI Scientist v2 GitHub](https://github.com/SakanaAI/AI-Scientist-v2)

---

## 7. Synthesis and Key Takeaways

### The Landscape in One Table

| System | Approach | Best Benchmark | Open Source | Key Strength |
|--------|----------|---------------|-------------|--------------|
| Devin | Full sandbox agent | 13.86% SWE-bench | No | Enterprise integration |
| OpenHands | Modular SDK platform | Multi-benchmark | Yes (MIT) | Composable, extensible |
| SWE-Agent | ACI-focused agent | 12.29% SWE-bench, 74%+ Verified (mini) | Yes | Interface design insight |
| Factory | Enterprise Droids | N/A (private) | No | Ticket-to-code traceability |
| Cursor | MoE + multi-agent pipeline | N/A (proprietary) | No | Scale (100s concurrent agents) |
| Claude Code | Single-threaded master loop | #1 SWE-rebench (Opus 4.6) | Partially | Simplicity, revenue ($2B ARR) |
| Codex CLI | Cloud + local dual mode | 57.7% SWE-bench Pro (GPT-5.4) | CLI: Yes | Model strength (GPT-5.x) |
| Gemini CLI | ReAct loop | 91.7% LiveCodeBench (Gemini 3 Pro) | Yes | 1M context, free tier |

### The Three Key Insights

1. **Scaffold > Model:** The same model can score 50-55% depending on agent architecture. Interface design, context management, and tool orchestration matter as much as raw model capability. SWE-Agent's ACI and Agentless's simplicity both demonstrate this.

2. **Simple beats complex (for now):** Agentless (no agent) matches complex agent frameworks. Claude Code's single-threaded loop powers $2B ARR. Karpathy's 630-line script finds improvements that transfer to larger models. The winning pattern in 2026 is minimal, well-designed loops -- not elaborate multi-agent orchestrations.

3. **The gap is not generation but verification:** Current systems can generate plausible code at scale. The bottleneck has shifted to *knowing whether the code is correct* -- and this requires understanding business context, architectural intent, and production behavior that no benchmark measures and no current system possesses.

### The Evolutionary Pattern

A common thread across FunSearch, AlphaEvolve, autoresearch, and Sol-Ver: **LLM as mutation operator + automated evaluation + evolutionary selection**. This pattern works wherever you have:
- A clear, computable fitness function
- A fast feedback loop
- A search space expressible as code

This is the most promising architecture for self-improving systems, but it is fundamentally bounded by the quality of the evaluation function. For software engineering, we lack evaluation functions for the things that matter most (architectural quality, maintainability, security, user experience).
