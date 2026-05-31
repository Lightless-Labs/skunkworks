import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { DEFAULT_CONFIG, loadConfig } from "../src/core/config.ts";
import {
	formatPromptProfiles,
	setConfigEnabled,
	setModelPool,
	setPersistentRandomEnabled,
	setRandomFrequency,
	upsertModel,
	validateFluxConfig,
} from "../src/core/configActions.ts";
import { formatSnapshotForPrompt } from "../src/core/context.ts";
import { isDeliveryMode, piDeliverAs, supportedDeliveryModes } from "../src/core/delivery.ts";
import { generateStrayThought, selectPromptProfile } from "../src/core/engine.ts";
import { resolveModelPool } from "../src/core/modelClient.ts";
import { createInitialState, shouldFireTrigger } from "../src/core/triggers.ts";
import type { AgentContextSnapshot, FluxConfig, FluxModelSpec, TriggerEvent } from "../src/core/types.ts";

function cloneConfig(): FluxConfig {
	return structuredClone(DEFAULT_CONFIG) as FluxConfig;
}

function snapshot(overrides: Partial<AgentContextSnapshot> = {}): AgentContextSnapshot {
	return {
		host: "generic",
		cwd: "/workspace",
		lastUserMessages: [],
		lastAssistantMessages: [],
		toolEvents: [],
		...overrides,
	};
}

function event(overrides: Partial<TriggerEvent> = {}): TriggerEvent {
	return {
		host: "generic",
		kind: "turn-end",
		timestamp: 1_000,
		...overrides,
	};
}

function model(name: string, apiKey?: string): FluxModelSpec {
	return { name, provider: "anthropic", model: `${name}-model`, apiKey };
}

test("loadConfig deep-merges partial overrides onto defaults", () => {
	const tmp = mkdtempSync(join(tmpdir(), "flux-config-test-"));
	const configPath = join(tmp, "partial.json");
	const previousFluxConfig = process.env.FLUX_CONFIG;
	writeFileSync(
		configPath,
		JSON.stringify({
			random: { probability: 0.42 },
			context: { maxChars: 321 },
			models: [{ name: "literal", provider: "anthropic", model: "claude-test", apiKey: "test-key" }],
			modelPools: { default: ["literal"] },
			promptProfiles: { random: [{ name: "override-random", style: "custom random" }] },
			storage: {},
		}),
	);

	try {
		process.env.FLUX_CONFIG = configPath;
		const loaded = loadConfig(tmp);

		assert.equal(loaded.path, configPath);
		assert.equal(loaded.config.random.probability, 0.42);
		assert.equal(loaded.config.random.minIntervalMs, DEFAULT_CONFIG.random.minIntervalMs);
		assert.equal(loaded.config.context.maxChars, 321);
		assert.equal(loaded.config.context.maxUserMessages, DEFAULT_CONFIG.context.maxUserMessages);
		assert.deepEqual(
			loaded.config.models.map((spec) => spec.name),
			["literal"],
		);
		assert.equal(loaded.config.promptProfiles.random?.[0]?.name, "override-random");
		assert.equal(loaded.config.promptProfiles.default?.[0]?.name, DEFAULT_CONFIG.promptProfiles.default?.[0]?.name);
		assert.equal(loaded.config.storage.thoughtLog, join(tmp, "thoughts.jsonl"));
	} finally {
		if (previousFluxConfig === undefined) delete process.env.FLUX_CONFIG;
		else process.env.FLUX_CONFIG = previousFluxConfig;
		rmSync(tmp, { recursive: true, force: true });
	}
});

test("shouldFireTrigger applies random defaults from config.random", () => {
	const config = cloneConfig();
	config.random = { probability: 0.5, minIntervalMs: 100, afterEvents: 2 };
	config.triggers = [{ name: "random-turn", kind: "random", enabled: true }];
	const state = createInitialState();

	assert.equal(shouldFireTrigger(config, state, event({ timestamp: 1_000 }), snapshot(), () => 0), undefined);

	const fired = shouldFireTrigger(config, state, event({ timestamp: 1_000 }), snapshot(), () => 0.49);
	assert.equal(fired?.name, "random-turn");
	assert.equal(state.lastTriggerAt["random-turn"], 1_000);

	assert.equal(shouldFireTrigger(config, state, event({ timestamp: 1_050 }), snapshot(), () => 0), undefined);
	assert.equal(shouldFireTrigger(config, state, event({ timestamp: 1_100 }), snapshot(), () => 0.5), undefined);
});

test("trigger-level random frequency settings override config.random", () => {
	const config = cloneConfig();
	config.random = { probability: 0, minIntervalMs: 60_000, afterEvents: 99 };
	config.triggers = [
		{ name: "override-random", kind: "random", enabled: true, probability: 1, minIntervalMs: 0, afterEvents: 1 },
	];
	const state = createInitialState();

	const fired = shouldFireTrigger(config, state, event({ timestamp: 1 }), snapshot(), () => 0.99);
	assert.equal(fired?.name, "override-random");
});

test("loop-detected triggers fire only when recent context matches a loop pattern", () => {
	const config = cloneConfig();
	config.triggers = [{ name: "loop", kind: "loop-detected", enabled: true, probability: 1, patterns: ["same error", "E\\d+"] }];
	const state = createInitialState();

	assert.equal(
		shouldFireTrigger(
			config,
			state,
			event({ timestamp: 1, kind: "turn-end" }),
			snapshot({ lastAssistantMessages: [{ role: "assistant", text: "No issue here." }] }),
		),
		undefined,
	);

	const plainTextMatch = shouldFireTrigger(
		config,
		state,
		event({ timestamp: 2, kind: "turn-end" }),
		snapshot({ lastUserMessages: [{ role: "user", text: "We hit the same error again." }] }),
	);
	assert.equal(plainTextMatch?.name, "loop");

	const regexMatch = shouldFireTrigger(
		config,
		createInitialState(),
		event({ timestamp: 3, kind: "tool-result" }),
		snapshot({ toolEvents: [{ name: "test", result: "failed with E123" }] }),
	);
	assert.equal(regexMatch?.name, "loop");
});

test("selectPromptProfile resolves trigger name, then kind, then default and honors weights", () => {
	const config = cloneConfig();
	config.promptProfiles = {
		default: [{ name: "default-profile", style: "default style" }],
		random: [
			{ name: "kind-a", weight: 1, style: "kind A" },
			{ name: "kind-b", weight: 3, style: "kind B" },
		],
		"named-trigger": [{ name: "name-profile", style: "name style" }],
	};

	assert.equal(selectPromptProfile(config, event({ name: "named-trigger", kind: "random" }), () => 0.99).name, "name-profile");
	assert.equal(selectPromptProfile(config, event({ name: "other", kind: "random" }), () => 0).name, "kind-a");
	assert.equal(selectPromptProfile(config, event({ name: "other", kind: "random" }), () => 0.99).name, "kind-b");
	assert.equal(selectPromptProfile(config, event({ kind: "external" }), () => 0.99).name, "default-profile");
});

test("resolveModelPool resolves trigger name, then kind, then default, then any usable model", () => {
	const config = cloneConfig();
	delete process.env.FLUX_UNSET_FOR_MODEL_POOL_TEST;
	config.models = [
		model("name-model", "key-name"),
		model("kind-model", "key-kind"),
		model("default-model", "key-default"),
		{ name: "unusable", provider: "anthropic", model: "unusable-model", apiKeyEnv: "FLUX_UNSET_FOR_MODEL_POOL_TEST" },
	];
	config.modelPools = {
		default: ["default-model"],
		random: ["kind-model"],
		"named-trigger": ["name-model"],
		"unusable-pool": ["unusable"],
	};

	assert.deepEqual(
		resolveModelPool(config, event({ name: "named-trigger", kind: "random" })).map((spec) => spec.name),
		["name-model"],
	);
	assert.deepEqual(resolveModelPool(config, event({ name: "other", kind: "random" })).map((spec) => spec.name), ["kind-model"]);
	assert.deepEqual(resolveModelPool(config, event({ kind: "manual" })).map((spec) => spec.name), ["default-model"]);
	assert.deepEqual(
		resolveModelPool(config, event({ name: "unusable-pool", kind: "external" })).map((spec) => spec.name),
		["name-model", "kind-model", "default-model"],
	);
});

test("config actions validate and mutate common slash-command settings", () => {
	const config = cloneConfig();
	config.models = [model("fast", "key"), model("careful", "key")];
	config.modelPools = { default: ["fast"] };

	assert.equal(setConfigEnabled(config, "false").ok, true);
	assert.equal(config.enabled, false);
	assert.equal(setPersistentRandomEnabled(config, "off").ok, true);
	assert.equal(config.randomInjections, false);
	assert.equal(setRandomFrequency(config, "probability", "0.25").ok, true);
	assert.equal(config.random.probability, 0.25);
	assert.equal(upsertModel(config, ["tiny", "openai-compatible", "gpt-mini", "apiKeyEnv=OPENAI_API_KEY", "maxTokens=123"]).ok, true);
	assert.equal(config.models.find((spec) => spec.name === "tiny")?.maxTokens, 123);
	assert.equal(setModelPool(config, "random", "fast,careful,tiny").ok, true);
	assert.deepEqual(config.modelPools.random, ["fast", "careful", "tiny"]);
	assert.equal(validateFluxConfig(config).ok, true);

	assert.equal(setConfigEnabled(config, "maybe").ok, false);
	assert.equal(setRandomFrequency(config, "probability", "2").ok, false);
	assert.equal(upsertModel(config, ["bad", "unknown", "model"]).ok, false);
	assert.equal(setModelPool(config, "manual", "missing").ok, false);
});

test("formatPromptProfiles includes profile styles, not only names", () => {
	const formatted = formatPromptProfiles(cloneConfig());
	assert.match(formatted, /Flux prompt profiles/);
	assert.match(formatted, /local-spark/);
	assert.match(formatted, /Offer one narrow/);
});

test("delivery modes are limited to agent message delivery, not hook transports", () => {
	assert.equal(isDeliveryMode("steer"), true);
	assert.equal(isDeliveryMode("followUp"), true);
	assert.equal(isDeliveryMode("nextTurn"), true);
	assert.equal(isDeliveryMode("stdout"), false);
	assert.equal(isDeliveryMode("file"), false);
	assert.equal(piDeliverAs("stdout"), undefined);
	assert.match(supportedDeliveryModes(), /steer/);
});

test("generateStrayThought can use an injected host-native model caller", async () => {
	const config = cloneConfig();
	config.storage = {};
	const state = createInitialState();
	const thought = await generateStrayThought(config, state, snapshot(), event({ kind: "manual", name: "manual" }), {
		modelCaller: async ({ systemPrompt, prompt }) => {
			assert.match(systemPrompt, /Flux/);
			assert.match(prompt, /Agent context snapshot/);
			return { content: "Note: use the host model", model: "pi/test-model" };
		},
	});

	assert.equal(thought.model, "pi/test-model");
	assert.equal(thought.content, "use the host model");
	assert.equal(state.lastThought, thought);
});

test("formatSnapshotForPrompt includes recent context and clamps long snapshots", () => {
	const config = cloneConfig();
	const formatted = formatSnapshotForPrompt(
		snapshot({
			lastUserMessages: [{ role: "user", text: "Please fix the failing parser." }],
			lastAssistantMessages: [{ role: "assistant", text: "I will inspect the parser tests." }],
			toolEvents: [{ name: "read", input: { path: "src/parser.ts" }, result: "parser source" }],
		}),
		config,
	);

	assert.match(formatted, /Host: generic/);
	assert.match(formatted, /Recent user messages:/);
	assert.match(formatted, /Please fix the failing parser/);
	assert.match(formatted, /Recent assistant responses:/);
	assert.match(formatted, /Recent tool events:/);
	assert.match(formatted, /read input=/);

	const clampingConfig = cloneConfig();
	clampingConfig.context.maxChars = 120;
	const clamped = formatSnapshotForPrompt(
		snapshot({ systemPrompt: `${"x".repeat(500)}tail-sentinel` }),
		clampingConfig,
	);
	assert.ok(clamped.length <= clampingConfig.context.maxChars);
	assert.match(clamped, /truncated/);
	assert.doesNotMatch(clamped, /tail-sentinel/);
});
