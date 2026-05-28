import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import type { FluxConfig } from "./types.ts";

export const DEFAULT_CONFIG: FluxConfig = {
	enabled: true,
	randomInjections: true,
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
	triggers: [
		{
			name: "random-turn-end",
			kind: "random",
			enabled: true,
			probability: 0.18,
			minIntervalMs: 180_000,
			afterEvents: 2,
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
			"You are Flux, an ADHD-brain sidecar for a coding agent.",
			"Generate one concise stray thought that may help the main agent escape tunnel vision.",
			"Be specific to the visible context. Prefer creative reframes, cheap checks, overlooked constraints, or alternative hypotheses.",
			"Do not command the agent. Do not repeat its plan. Do not be verbose. Output only the stray thought text.",
		].join("\n"),
		style: "Prefix is added by the host. Keep the thought under 120 words.",
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
