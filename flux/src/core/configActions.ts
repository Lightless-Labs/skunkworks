import type { FluxConfig, FluxModelSpec } from "./types.ts";

export interface ConfigActionResult {
	ok: boolean;
	message: string;
}

function ok(message: string): ConfigActionResult {
	return { ok: true, message };
}

function err(message: string): ConfigActionResult {
	return { ok: false, message };
}

export function parseBoolean(value: string | undefined): boolean | undefined {
	if (value === undefined) return undefined;
	const normalized = value.toLowerCase();
	if (["true", "on", "yes", "1"].includes(normalized)) return true;
	if (["false", "off", "no", "0"].includes(normalized)) return false;
	return undefined;
}

export function setConfigEnabled(config: FluxConfig, value: string | undefined): ConfigActionResult {
	const parsed = parseBoolean(value);
	if (parsed === undefined) return err("Usage: /flux config set enabled true|false");
	config.enabled = parsed;
	return ok(`Set Flux enabled=${parsed}`);
}

export function setPersistentRandomEnabled(config: FluxConfig, value: string | undefined): ConfigActionResult {
	const parsed = parseBoolean(value);
	if (parsed === undefined) return err("Usage: /flux config random on|off");
	config.randomInjections = parsed;
	return ok(`Set Flux randomInjections=${parsed}`);
}

export function setRandomFrequency(config: FluxConfig, field: string | undefined, value: string | undefined): ConfigActionResult {
	if (!field || value === undefined) {
		return err(
			`Usage: /flux config random probability <0..1> | minIntervalMs <ms> | afterEvents <count>\nCurrent: ${JSON.stringify(config.random)}`,
		);
	}
	if (!(field in config.random)) return err(`Unknown random field: ${field}`);
	const numeric = Number(value);
	if (!Number.isFinite(numeric)) return err(`Expected numeric value for ${field}`);
	if (field === "probability" && (numeric < 0 || numeric > 1)) return err("Probability must be between 0 and 1.");
	if ((field === "minIntervalMs" || field === "afterEvents") && numeric < 0) return err(`${field} must be non-negative.`);
	config.random[field as keyof FluxConfig["random"]] = numeric;
	return ok(`Set Flux random.${field}=${numeric}`);
}

function parseKeyValueOptions(tokens: string[]): Record<string, string> {
	const options: Record<string, string> = {};
	for (const token of tokens) {
		const index = token.indexOf("=");
		if (index <= 0) continue;
		options[token.slice(0, index)] = token.slice(index + 1);
	}
	return options;
}

export function upsertModel(config: FluxConfig, parts: string[]): ConfigActionResult {
	const [name, provider, modelId, ...optionTokens] = parts;
	if (!name || !provider || !modelId) {
		return err(
			"Usage: /flux config model <name> <openai-compatible|anthropic> <model-id> [apiKeyEnv=ENV] [baseUrl=URL] [maxTokens=N] [temperature=N]",
		);
	}
	if (provider !== "openai-compatible" && provider !== "anthropic") {
		return err("Provider must be openai-compatible or anthropic.");
	}
	const options = parseKeyValueOptions(optionTokens);
	const existing = config.models.find((model) => model.name === name);
	const next: FluxModelSpec = {
		...(existing ?? {}),
		name,
		provider,
		model: modelId,
	};
	if (options.apiKeyEnv !== undefined) next.apiKeyEnv = options.apiKeyEnv;
	if (options.baseUrl !== undefined) next.baseUrl = options.baseUrl;
	if (options.maxTokens !== undefined) {
		const maxTokens = Number(options.maxTokens);
		if (!Number.isInteger(maxTokens) || maxTokens <= 0) return err("maxTokens must be a positive integer.");
		next.maxTokens = maxTokens;
	}
	if (options.temperature !== undefined) {
		const temperature = Number(options.temperature);
		if (!Number.isFinite(temperature) || temperature < 0 || temperature > 2) return err("temperature must be between 0 and 2.");
		next.temperature = temperature;
	}
	if (existing) Object.assign(existing, next);
	else config.models.push(next);
	return ok(`${existing ? "Updated" : "Added"} Flux model ${name}: ${provider}/${modelId}`);
}

export function setModelPool(config: FluxConfig, poolName: string | undefined, rawModels: string | undefined): ConfigActionResult {
	if (!poolName || !rawModels) return err("Usage: /flux config pool <pool-name> <model-a,model-b>");
	const models = rawModels
		.split(/[\s,]+/)
		.map((name) => name.trim())
		.filter(Boolean);
	if (models.length === 0) return err("Model pool must contain at least one model name.");
	const known = new Set(config.models.map((model) => model.name));
	const unknown = models.filter((name) => !known.has(name));
	if (unknown.length > 0) return err(`Unknown Flux model(s): ${unknown.join(", ")}`);
	config.modelPools[poolName] = models;
	return ok(`Set Flux model pool ${poolName}: ${models.join(", ")}`);
}

export function validateFluxConfig(config: FluxConfig): ConfigActionResult {
	if (typeof config.enabled !== "boolean") return err("Flux config enabled must be boolean.");
	if (typeof config.randomInjections !== "boolean") return err("Flux config randomInjections must be boolean.");
	if (!Number.isFinite(config.random.probability) || config.random.probability < 0 || config.random.probability > 1) {
		return err("Flux config random.probability must be between 0 and 1.");
	}
	if (!Number.isFinite(config.random.minIntervalMs) || config.random.minIntervalMs < 0) {
		return err("Flux config random.minIntervalMs must be non-negative.");
	}
	if (!Number.isFinite(config.random.afterEvents) || config.random.afterEvents < 0) {
		return err("Flux config random.afterEvents must be non-negative.");
	}
	const modelNames = new Set<string>();
	for (const model of config.models) {
		if (!model.name) return err("Every Flux model must have a name.");
		if (modelNames.has(model.name)) return err(`Duplicate Flux model name: ${model.name}`);
		modelNames.add(model.name);
	}
	for (const [pool, models] of Object.entries(config.modelPools)) {
		for (const model of models) {
			if (!modelNames.has(model)) return err(`Model pool ${pool} references unknown model: ${model}`);
		}
	}
	try {
		JSON.stringify(config);
	} catch (error) {
		return err(`Flux config is not JSON-serializable: ${error instanceof Error ? error.message : String(error)}`);
	}
	return ok("Flux config validated");
}

export function formatPromptProfiles(config: FluxConfig): string {
	const lines = ["Flux prompt profiles:"];
	for (const [pool, profiles] of Object.entries(config.promptProfiles)) {
		lines.push(`- ${pool}:`);
		for (const profile of profiles) {
			lines.push(`  - ${profile.name} (weight ${profile.weight ?? 1})`);
			lines.push(`    ${profile.style}`);
		}
	}
	return lines.join("\n");
}
