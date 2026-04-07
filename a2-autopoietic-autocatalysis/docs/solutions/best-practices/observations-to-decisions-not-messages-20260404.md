---
module: stagnation-detector
date: 2026-04-04
problem_type: best_practice
component: service_object
severity: medium
related_components:
  - governor
tags:
  - autonomous-systems
  - api-design
  - decision-types
  - human-in-the-loop
  - autopoiesis
applies_when:
  - "Designing components in an autonomous loop"
  - "A subsystem currently returns advisory text strings"
  - "There is no human reader downstream of a diagnostic"
---

# In autonomous loops, return decisions — not messages

## Principle

When a subsystem in an autonomous loop produces output that no human will
read, it must return a *typed decision the next stage can act on*, not a
human-readable string. Strings are an anti-pattern wherever the consumer is
code: they push parsing, interpretation, and exhaustiveness checking onto a
caller that has neither the context nor the ability to fail loudly when the
producer changes its mind.

The smell: any function in an autonomous pipeline whose return type is
`String`, `&str`, or `Option<String>`, and whose docstring says "describes
the recommended action."

## Concrete shape

Replace:

```rust
fn suggest_strategy_change(&self) -> Option<String> {
    if self.recent_promotions == 0 {
        Some("Consider switching model or raising temperature".into())
    } else {
        None
    }
}
```

With:

```rust
enum StrategyChange {
    None,
    SwitchModel,
    DecomposeTask,
    RaiseTemperature,
}

fn suggest_strategy_change(&self) -> StrategyChange { ... }
```

The differences look cosmetic. They are not.

- The caller now `match`es exhaustively. Adding a new variant becomes a
  compile-time conversation with every consumer.
- The decision is a value the governor can route on, log structurally,
  count in metrics, and replay deterministically in tests.
- The detector cannot accidentally invent a new "suggestion" that no part
  of the system knows how to execute. The vocabulary is finite and shared.
- The boundary between *observation* (trends, deltas, counters) and
  *decision* (which lever to pull) becomes explicit and auditable. With
  strings, that boundary is inside an LLM-shaped blob in the middle of the
  pipeline.

## The deeper rule

Autonomous loops should not contain any stage that produces "advice." A loop
is closed when every output is consumed by code that knows what to do with
every possible value. Advice — a free-form recommendation whose addressee is
unspecified — is the exact opposite shape: it is an output that requires a
*reader*, and in an autonomous system the reader does not exist.

The presence of advisory strings inside a closed loop usually means a
missing component: somewhere there is a decision that ought to be made, and
nobody made it, so it was kicked downstream as a sentence. Find the
decision, name its variants, make the type system carry it.

## How to spot the pattern

Walk the data flow from the loop's outermost driver inward and ask, at each
stage: *what does the next stage do with this value?* If the honest answer
is "log it" or "include it in a prompt to another model," the value should
be data, not prose. If the honest answer is "branch on it," the type must
make the branches enumerable.

Strings survive in autonomous systems only as terminal artifacts —
human-facing logs, commit messages, post-hoc reports. The moment a string
becomes load-bearing for the loop's own next step, it is technical debt
that the system will eventually pay for in mysterious behavior changes when
the producer's wording drifts.

## Corollary for LLM-in-the-loop systems

This rule is doubly important when one of the stages is a language model.
LLMs will happily consume and emit prose, which makes it tempting to let
prose flow through structural boundaries the way data would. Don't. At every
LLM boundary inside a loop, define a typed schema (enum, struct, JSON
schema) and force the model output through it. The interior of the loop
should look like a normal program; only the LLM's input prompt and output
parser should know that natural language ever existed.
