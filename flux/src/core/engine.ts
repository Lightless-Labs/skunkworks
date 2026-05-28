import { appendFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { randomUUID } from "node:crypto";
import { digestSnapshot, formatSnapshotForPrompt } from "./context.ts";
import { callSidecarModel, pickModel } from "./modelClient.ts";
import type { AgentContextSnapshot, FluxConfig, FluxState, PromptProfile, StrayThought, TriggerEvent } from "./types.ts";

function weightedPick<T extends { weight?: number }>(items: T[], random = Math.random): T | undefined {
	if (items.length === 0) return undefined;
	const total = items.reduce((sum, item) => sum + Math.max(0, item.weight ?? 1), 0);
	if (total <= 0) return items[0];
	let cursor = random() * total;
	for (const item of items) {
		cursor -= Math.max(0, item.weight ?? 1);
		if (cursor <= 0) return item;
	}
	return items[items.length - 1];
}

export function selectPromptProfile(config: FluxConfig, trigger: TriggerEvent, random = Math.random): PromptProfile {
	const pool =
		(trigger.name && config.promptProfiles[trigger.name]) || config.promptProfiles[trigger.kind] || config.promptProfiles.default || [];
	return weightedPick(pool, random) ?? { name: "fallback", style: config.prompt.style, system: config.prompt.system };
}

export function buildThoughtPrompt(
	config: FluxConfig,
	snapshot: AgentContextSnapshot,
	trigger: TriggerEvent,
	profile = selectPromptProfile(config, trigger),
): string {
	return [
		profile.style || config.prompt.style,
		`Trigger: ${trigger.name ?? trigger.kind}`,
		"Agent context snapshot:",
		formatSnapshotForPrompt(snapshot, config),
		"Return exactly one note. Do not include preamble, labels, markdown headings, or multiple options.",
	].join("\n\n");
}

export async function generateStrayThought(
	config: FluxConfig,
	state: FluxState,
	snapshot: AgentContextSnapshot,
	trigger: TriggerEvent,
	signal?: AbortSignal,
): Promise<StrayThought> {
	const model = pickModel(config, trigger);
	const profile = selectPromptProfile(config, trigger);
	const prompt = buildThoughtPrompt(config, snapshot, trigger, profile);
	const content = await callSidecarModel(model, profile.system ?? config.prompt.system, prompt, signal);
	const thought: StrayThought = {
		id: randomUUID(),
		createdAt: new Date().toISOString(),
		model: `${model.provider}/${model.model}`,
		promptProfile: profile.name,
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
		.replace(/^\s*(stray thought|thought|suggestion|note|feedback)\s*:\s*/i, "")
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
