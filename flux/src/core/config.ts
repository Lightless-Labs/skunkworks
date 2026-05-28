import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import type { FluxConfig } from "./types.ts";

export const DEFAULT_CONFIG: FluxConfig = {
	enabled: true,
	randomInjections: true,
	random: {
		probability: 0.18,
		minIntervalMs: 180_000,
		afterEvents: 2,
	},
	delivery: "steer",
	displayThoughts: true,
	triggerTurn: true,
	context: {
		maxUserMessages: 4,
		maxAssistantMessages: 4,
		maxToolEvents: 12,
		maxChars: 18_000,
	},
	models: [
		{
			name: "openai-default",
			provider: "openai-compatible",
			model: "gpt-4.1-mini",
			baseUrl: "https://api.openai.com/v1",
			apiKeyEnv: "OPENAI_API_KEY",
			temperature: 0.9,
			maxTokens: 420,
		},
		{
			name: "anthropic-default",
			provider: "anthropic",
			model: "claude-3-5-haiku-latest",
			apiKeyEnv: "ANTHROPIC_API_KEY",
			temperature: 0.8,
			maxTokens: 420,
		},
	],
	modelPools: {
		default: ["openai-default", "anthropic-default"],
		random: ["openai-default", "anthropic-default"],
		"loop-detected": ["anthropic-default", "openai-default"],
	},
	triggers: [
		{
			name: "random-turn-end",
			kind: "random",
			enabled: true,
		},
		{
			name: "loop-language",
			kind: "loop-detected",
			enabled: true,
			probability: 1,
			minIntervalMs: 60_000,
			patterns: ["again", "still failing", "same error", "loop", "stuck", "retry"],
		},
	],
	prompt: {
		system: [
			"You are Flux, a secondary model asked to add a bounded note to a coding-agent session.",
			"Follow the selected trigger/profile instructions. Use only the supplied context snapshot.",
			"Do not pretend to have acted in the workspace. Do not call tools. Output only the requested note.",
		].join("\n"),
		style: "Write one concise note for the main agent. The host will add any prefix.",
	},
	promptProfiles: {
		default: [
			{
				name: "useful-note",
				weight: 1,
				style: "Write one concise, context-specific note that could help the main agent. Avoid generic encouragement.",
			},
		],
		random: [
			{
				name: "lateral-check",
				weight: 2,
				style:
					"Offer one lateral, context-specific thought: an overlooked edge case, constraint, cheap validation, or alternative hypothesis. Keep it under 120 words.",
			},
			{
				name: "weird-reframe",
				weight: 1,
				style:
					"Offer one playful but technically grounded reframe that may reveal a different path. Keep it useful, concrete, and under 120 words.",
			},
		],
		"loop-detected": [
			{
				name: "kind-critical-feedback",
				weight: 2,
				style:
					"The main agent may be looping or spending a long time without progress. Provide kind but honest critical feedback comparing what it has been doing against the apparent current task or goal. Name one likely mismatch, untested assumption, or course correction. Keep it under 160 words.",
			},
			{
				name: "break-loop-smallest-check",
				weight: 1,
				style:
					"The main agent may be repeating itself. Suggest one smallest possible check, reproduction, or state inspection that could break the loop. Be specific to the context and under 120 words.",
			},
		],
		"tool-result": [
			{
				name: "tool-output-implication",
				weight: 1,
				style:
					"The main agent just received tool output. Point out one implication, anomaly, or next check it might otherwise miss. Do not summarize the whole output.",
			},
		],
		manual: [
			{
				name: "requested-nudge",
				weight: 1,
				style:
					"A user, plugin, or the main agent explicitly requested a Flux note. Prioritize high-signal usefulness over randomness; give one actionable perspective grounded in the context.",
			},
		],
		external: [
			{
				name: "external-trigger",
				weight: 1,
				style:
					"Another extension or plugin requested a Flux note. Use the payload reason if present, and provide one concise note grounded in the session context.",
			},
		],
	},
	storage: {},
};

export interface LoadedConfig {
	config: FluxConfig;
	path?: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function deepMerge<T>(base: T, override: unknown): T {
	if (!isRecord(base) || !isRecord(override)) return (override ?? base) as T;
	const out: Record<string, unknown> = { ...base };
	for (const [key, value] of Object.entries(override)) {
		const current = out[key];
		if (Array.isArray(value)) out[key] = value;
		else if (isRecord(current) && isRecord(value)) out[key] = deepMerge(current, value);
		else if (value !== undefined) out[key] = value;
	}
	return out as T;
}

export function discoverConfigPath(cwd = process.cwd()): string | undefined {
	const explicit = process.env.FLUX_CONFIG;
	if (explicit) return resolve(cwd, explicit);

	const candidates = [
		join(cwd, ".flux", "config.json"),
		join(cwd, "flux.config.json"),
		join(homedir(), ".config", "flux", "config.json"),
	];
	return candidates.find((path) => existsSync(path));
}

export function loadConfig(cwd = process.cwd()): LoadedConfig {
	const path = discoverConfigPath(cwd);
	if (!path) return { config: DEFAULT_CONFIG };
	const raw = readFileSync(path, "utf8");
	const parsed = JSON.parse(raw) as unknown;
	const config = deepMerge(DEFAULT_CONFIG, parsed);
	if (!config.storage.thoughtLog) {
		config.storage.thoughtLog = join(dirname(path), "thoughts.jsonl");
	}
	return { config, path };
}

export function usableModels(config: FluxConfig) {
	return config.models.filter((model) => Boolean(model.apiKey || (model.apiKeyEnv && process.env[model.apiKeyEnv])));
}
