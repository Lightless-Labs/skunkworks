import { appendFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { randomUUID } from "node:crypto";
import { digestSnapshot, formatSnapshotForPrompt } from "./context.ts";
import { callSidecarModel, pickModel } from "./modelClient.ts";
import type { AgentContextSnapshot, FluxConfig, FluxState, StrayThought, TriggerEvent } from "./types.ts";

export function buildThoughtPrompt(config: FluxConfig, snapshot: AgentContextSnapshot, trigger: TriggerEvent): string {
	return [
		config.prompt.style,
		`Trigger: ${trigger.name ?? trigger.kind}`,
		"Agent context snapshot:",
		formatSnapshotForPrompt(snapshot, config),
		"Return exactly one stray thought.",
	].join("\n\n");
}

export async function generateStrayThought(
	config: FluxConfig,
	state: FluxState,
	snapshot: AgentContextSnapshot,
	trigger: TriggerEvent,
	signal?: AbortSignal,
): Promise<StrayThought> {
	const model = pickModel(config);
	const prompt = buildThoughtPrompt(config, snapshot, trigger);
	const content = await callSidecarModel(model, config.prompt.system, prompt, signal);
	const thought: StrayThought = {
		id: randomUUID(),
		createdAt: new Date().toISOString(),
		model: `${model.provider}/${model.model}`,
		trigger,
		content: normalizeThought(content),
		contextDigest: digestSnapshot(snapshot),
	};
	state.lastThought = thought;
	appendThoughtLog(config, thought);
	return thought;
}

export function normalizeThought(content: string): string {
	return content
		.replace(/^\s*(stray thought|thought|suggestion)\s*:\s*/i, "")
		.trim()
		.replace(/^['\"]|['\"]$/g, "");
}

export function renderThoughtForAgent(thought: StrayThought): string {
	return `💭 Stray thought (${thought.trigger.name ?? thought.trigger.kind}, ${thought.model}): ${thought.content}`;
}

export function appendThoughtLog(config: FluxConfig, thought: StrayThought): void {
	const logPath = config.storage.thoughtLog;
	if (!logPath) return;
	try {
		mkdirSync(dirname(logPath), { recursive: true });
		appendFileSync(logPath, `${JSON.stringify(thought)}\n`, { encoding: "utf8", mode: 0o600 });
	} catch {
		// Logging must never break the host agent.
	}
}
