import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import fluxPiExtension from "../src/adapters/pi/index.ts";

type RegisteredPi = {
	handlers: Record<string, (event: unknown, ctx: any) => Promise<void> | void>;
	commands: Record<string, { handler: (args: string, ctx: any) => Promise<void> | void }>;
};

function createPiHarness(): RegisteredPi {
	const harness: RegisteredPi = { handlers: {}, commands: {} };
	const pi = {
		on(name: string, handler: (event: unknown, ctx: any) => Promise<void> | void) {
			harness.handlers[name] = handler;
		},
		events: { on() {} },
		registerMessageRenderer() {},
		registerTool() {},
		registerCommand(name: string, command: { handler: (args: string, ctx: any) => Promise<void> | void }) {
			harness.commands[name] = command;
		},
		sendMessage() {},
	};
	fluxPiExtension(pi as any);
	return harness;
}

function createCtx(
	cwd: string,
	options: { model?: any; availableModels?: any[] } = {},
) {
	const notifications: Array<{ message: string; level: string }> = [];
	const statuses: Record<string, string> = {};
	return {
		cwd,
		hasUI: true,
		signal: undefined,
		model: options.model,
		sessionManager: { getBranch: () => [] },
		modelRegistry: {
			getAvailable: () => options.availableModels ?? [],
			find: (provider: string, modelId: string) => (options.availableModels ?? []).find((model) => model.provider === provider && model.id === modelId),
		},
		ui: {
			notify(message: string, level: string) {
				notifications.push({ message, level });
			},
			setStatus(key: string, value: string) {
				statuses[key] = value;
			},
			editor: async () => undefined,
		},
		notifications,
		statuses,
	};
}

function writeConfig(cwd: string, config: Record<string, unknown>): string {
	const dir = join(cwd, ".flux");
	mkdirSync(dir, { recursive: true });
	const path = join(dir, "config.json");
	writeFileSync(path, `${JSON.stringify(config, null, 2)}\n`, "utf8");
	return path;
}

async function withNoExplicitFluxConfig(run: () => Promise<void> | void): Promise<void> {
	const previous = process.env.FLUX_CONFIG;
	try {
		delete process.env.FLUX_CONFIG;
		await run();
	} finally {
		if (previous === undefined) delete process.env.FLUX_CONFIG;
		else process.env.FLUX_CONFIG = previous;
	}
}

test("Pi adapter reload syncs runtime enabled/random state from the cwd config", async () => {
	await withNoExplicitFluxConfig(async () => {
		const cwd = mkdtempSync(join(tmpdir(), "flux-pi-config-"));
		try {
			writeConfig(cwd, { enabled: false, randomInjections: false });
			const harness = createPiHarness();
			const ctx = createCtx(cwd);

			await harness.handlers.session_start?.({}, ctx);
			assert.equal(ctx.statuses.flux, "flux:off");

			await harness.commands.flux.handler("random on", ctx);
			await harness.commands.flux.handler("status", ctx);
			assert.match(ctx.notifications.at(-1)?.message ?? "", /enabled=false, random=true/);

			await harness.commands.flux.handler("reload", ctx);
			await harness.commands.flux.handler("status", ctx);
			assert.equal(ctx.statuses.flux, "flux:off");
			assert.match(ctx.notifications.at(-1)?.message ?? "", /enabled=false, random=false/);
		} finally {
			rmSync(cwd, { recursive: true, force: true });
		}
	});
});

test("Pi runtime on/off and random toggles do not rewrite .flux/config.json", async () => {
	await withNoExplicitFluxConfig(async () => {
		const cwd = mkdtempSync(join(tmpdir(), "flux-pi-runtime-"));
		try {
			const configPath = writeConfig(cwd, { enabled: true, randomInjections: false, random: { probability: 0.25 } });
			const before = readFileSync(configPath, "utf8");
			const harness = createPiHarness();
			const ctx = createCtx(cwd);

			await harness.handlers.session_start?.({}, ctx);
			await harness.commands.flux.handler("random on", ctx);
			await harness.commands.flux.handler("off", ctx);
			await harness.commands.flux.handler("on", ctx);

			assert.equal(readFileSync(configPath, "utf8"), before);
			await harness.commands.flux.handler("status", ctx);
			assert.match(ctx.notifications.at(-1)?.message ?? "", /enabled=true, random=true/);
		} finally {
			rmSync(cwd, { recursive: true, force: true });
		}
	});
});

test("Pi persistent config set commands update file-backed runtime state", async () => {
	await withNoExplicitFluxConfig(async () => {
		const cwd = mkdtempSync(join(tmpdir(), "flux-pi-persist-"));
		try {
			const configPath = writeConfig(cwd, { enabled: true, randomInjections: false });
			const harness = createPiHarness();
			const ctx = createCtx(cwd);

			await harness.handlers.session_start?.({}, ctx);
			await harness.commands.flux.handler("config set enabled false", ctx);
			await harness.commands.flux.handler("config random on", ctx);

			const persisted = JSON.parse(readFileSync(configPath, "utf8"));
			assert.equal(persisted.enabled, false);
			assert.equal(persisted.randomInjections, true);
			assert.equal(ctx.statuses.flux, "flux:off");
			await harness.commands.flux.handler("status", ctx);
			assert.match(ctx.notifications.at(-1)?.message ?? "", /enabled=false, random=true/);
		} finally {
			rmSync(cwd, { recursive: true, force: true });
		}
	});
});

test("Pi status shows configured vs resolved sidecar model and stale pin fallback", async () => {
	await withNoExplicitFluxConfig(async () => {
		const cwd = mkdtempSync(join(tmpdir(), "flux-pi-model-status-"));
		try {
			writeConfig(cwd, {
				hostSidecar: { pi: { model: "missing-sidecar", thinkingEffort: "high" } },
			});
			const activeModel = {
				provider: "openai-codex",
				id: "gpt-current",
				name: "Current Model",
				reasoning: true,
				thinkingLevelMap: { high: "high" },
			};
			const harness = createPiHarness();
			const ctx = createCtx(cwd, { model: activeModel, availableModels: [] });

			await harness.handlers.session_start?.({}, ctx);
			await harness.commands.flux.handler("status", ctx);

			const message = ctx.notifications.at(-1)?.message ?? "";
			assert.match(message, /Pi host sidecar:/);
			assert.match(message, /configured model=missing-sidecar/);
			assert.match(message, /resolved=openai-codex\/gpt-current/);
			assert.match(message, /configured thinking=high, resolved thinking=high/);
			assert.match(message, /warning=Flux Pi sidecar model not found or unavailable: missing-sidecar; falling back to active openai-codex\/gpt-current\./);
		} finally {
			rmSync(cwd, { recursive: true, force: true });
		}
	});
});
