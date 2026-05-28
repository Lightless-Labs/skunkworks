# AGENTS.md instructions for flux

Flux is a TypeScript skunkworks project that experiments with agent hook integrations.

- Keep the provider-agnostic core in `src/core/`.
- Keep host-specific integrations in `src/adapters/<host>/`.
- Avoid hard-coding model names or providers; prefer `.flux/config.json` and environment variables.
- Treat hook payloads from host agents as untrusted input. Do not execute arbitrary shell from payloads.
- Prefer small, bounded context snapshots for sidecar model calls.
