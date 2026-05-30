import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { Box, Text } from "@earendil-works/pi-tui";
import { Type } from "typebox";
import { DEFAULT_CONFIG, loadConfig } from "../../core/config.ts";
import { snapshotFromGenericPayload, textFromUnknown } from "../../core/context.ts";
import { generateStrayThought, hostNativeModelLabel, renderThoughtForAgent, type ThoughtModelCaller } from "../../core/engine.ts";
import { createInitialState, shouldFireTrigger } from "../../core/triggers.ts";
import type { AgentContextSnapshot, AgentMessage, AgentToolEvent, FluxConfig, FluxState, TriggerEvent } from "../../core/types.ts";

const CUSTOM_TYPE = "flux:stray-thought";

function messageText(message: any): string {
	return textFromUnknown(message?.content ?? message?.text ?? "");
}

function cloneDefaultConfig(): FluxConfig {
	return JSON.parse(JSON.stringify(DEFAULT_CONFIG)) as FluxConfig;
}

function writeConfigFile(path: string, config: FluxConfig): void {
	mkdirSync(dirname(path), { recursive: true });
	writeFileSync(path, `${JSON.stringify(config, null, 2)}\n`, { encoding: "utf8", mode: 0o600 });
}

function formatConfigSummary(config: FluxConfig, path?: string): string {
	const random = config.random;
	const pools = Object.entries(config.modelPools)
		.map(([name, models]) => `${name}: ${models.join(", ")}`)
		.join("; ");
	return [
		`Flux config: ${path ?? "defaults (not written)"}`,
		`enabled=${config.enabled}, random=${config.randomInjections}`,
		`random.frequency probability=${random.probability}, minIntervalMs=${random.minIntervalMs}, afterEvents=${random.afterEvents}`,
		`models=${config.models.map((model) => model.name).join(", ") || "none"}`,
		`modelPools=${pools || "none"}`,
	].join("\n");
}

type PiMessage = {
	role: "user";
	content: Array<{ type: "text"; text: string }>;
	timestamp: number;
};

type PiComplete = (
	model: NonNullable<ExtensionContext["model"]>,
	input: { systemPrompt: string; messages: PiMessage[] },
	options: { apiKey: string; headers?: Record<string, string>; signal?: AbortSignal },
) => Promise<{ stopReason?: string; content: Array<{ type: string; text?: string }> }>;

async function loadPiComplete(): Promise<PiComplete> {
	try {
		// @ts-ignore: pi-ai is provided by the host Pi runtime; Flux keeps it out of direct core deps.
		return ((await import("@earendil-works/pi-ai")) as { complete: PiComplete }).complete;
	} catch (error) {
		const codingAgentUrl = import.meta.resolve("@earendil-works/pi-coding-agent");
		const bundledPiAiUrl = new URL("../node_modules/@earendil-works/pi-ai/dist/index.js", codingAgentUrl).href;
		try {
			return ((await import(bundledPiAiUrl)) as { complete: PiComplete }).complete;
		} catch {
			throw error;
		}
	}
}

function createPiModelCaller(ctx: ExtensionContext, signal?: AbortSignal): ThoughtModelCaller {
	return async ({ systemPrompt, prompt }) => {
		if (!ctx.model) throw new Error("No Pi model selected for Flux host-native sidecar generation.");
		const auth = await ctx.modelRegistry.getApiKeyAndHeaders(ctx.model);
		if (!auth.ok || !auth.apiKey) throw new Error(auth.ok ? `No API key for ${ctx.model.provider}` : auth.error);
		const complete = await loadPiComplete();
		const userMessage: PiMessage = {
			role: "user",
			content: [{ type: "text", text: prompt }],
			timestamp: Date.now(),
		};
		const response = await complete(
			ctx.model,
			{ systemPrompt, messages: [userMessage] },
			{ apiKey: auth.apiKey, headers: auth.headers, signal },
		);
		if (response.stopReason === "aborted") throw new Error("Flux Pi sidecar generation was aborted.");
		const content = response.content
			.filter((part): part is { type: "text"; text: string } => part.type === "text" && typeof part.text === "string")
			.map((part) => part.text)
			.join("\n");
		return { content, model: hostNativeModelLabel("pi", `${ctx.model.provider}/${ctx.model.id}`) };
	};
}

function snapshotFromPi(ctx: ExtensionContext, config: FluxConfig, hostPayload?: unknown): AgentContextSnapshot {
	const branch = ctx.sessionManager.getBranch?.() ?? ctx.sessionManager.getEntries?.() ?? [];
	const userMessages: AgentMessage[] = [];
	const assistantMessages: AgentMessage[] = [];
	const toolEvents: AgentToolEvent[] = [];
	let firstUser: string | undefined;

	for (const entry of branch as any[]) {
		if (entry?.type !== "message") continue;
		const message = entry.message;
		if (!message) continue;
		if (message.role === "user") {
			const text = messageText(message);
			firstUser ??= text;
			userMessages.push({ role: "user", text, timestamp: entry.timestamp ?? message.timestamp });
		} else if (message.role === "assistant") {
			const text = messageText(message);
			const calls: AgentToolEvent[] = [];
			for (const part of message.content ?? []) {
				if (part?.type === "toolCall") {
					calls.push({ name: String(part.name ?? part.toolName ?? "tool"), input: part.arguments ?? part.input, timestamp: entry.timestamp });
				}
			}
			assistantMessages.push({ role: "assistant", text, timestamp: entry.timestamp ?? message.timestamp, toolCalls: calls });
			toolEvents.push(...calls);
		} else if (message.role === "toolResult") {
			toolEvents.push({
				name: String(message.toolName ?? "tool"),
				input: message.input,
				result: message.content ?? message.details,
				isError: Boolean(message.isError),
				timestamp: entry.timestamp ?? message.timestamp,
			});
		}
	}

	const generic = hostPayload ? snapshotFromGenericPayload("pi", hostPayload, config) : undefined;
	return {
		host: "pi",
		cwd: ctx.cwd,
		sessionPrompt: firstUser ?? generic?.sessionPrompt,
		systemPrompt: ctx.getSystemPrompt?.(),
		lastUserMessages: userMessages.slice(-config.context.maxUserMessages),
		lastAssistantMessages: assistantMessages.slice(-config.context.maxAssistantMessages),
		toolEvents: toolEvents.slice(-config.context.maxToolEvents),
		metadata: generic?.metadata,
	};
}

async function runFlux(
	config: FluxConfig,
	state: FluxState,
	ctx: ExtensionContext,
	event: TriggerEvent,
	snapshot: AgentContextSnapshot,
	options: { force?: boolean; display?: boolean; triggerTurn?: boolean } = {},
) {
	if (!config.enabled) return;
	const trigger = options.force
		? { name: event.name ?? event.kind, kind: event.kind, enabled: true }
		: shouldFireTrigger(config, state, event, snapshot);
	if (!trigger) return;
	const triggerEvent = { ...event, name: trigger.name };
	const thought = await generateStrayThought(config, state, snapshot, triggerEvent, {
		signal: ctx.signal,
		modelCaller: createPiModelCaller(ctx, ctx.signal),
	});
	const content = renderThoughtForAgent(thought);
	const display = options.display ?? config.displayThoughts;
	piSendMessage(content, thought, config, options.triggerTurn ?? config.triggerTurn, display);
}

let piSendMessage: (
	content: string,
	thought: unknown,
	config: FluxConfig,
	triggerTurn: boolean,
	display: boolean,
) => void = () => undefined;

export default function fluxPiExtension(pi: ExtensionAPI) {
	let loaded = loadConfig(process.cwd());
	let state = createInitialState();
	let busy = false;
	let randomEnabled = loaded.config.randomInjections;
	let currentCtx: ExtensionContext | undefined;

	const refreshConfig = (cwd?: string) => {
		loaded = loadConfig(cwd ?? process.cwd());
		loaded.config.randomInjections = randomEnabled;
	};

	piSendMessage = (content, thought, config, triggerTurn, display) => {
		pi.sendMessage(
			{
				customType: CUSTOM_TYPE,
				content,
				display,
				details: thought,
			},
			{ deliverAs: config.delivery === "followUp" ? "followUp" : config.delivery === "nextTurn" ? "nextTurn" : "steer", triggerTurn },
		);
	};

	pi.registerMessageRenderer(CUSTOM_TYPE, (message, { expanded }, theme) => {
		const box = new Box(1, 1, (text) => theme.bg("customMessageBg", text));
		const details = message.details as any;
		let text = `${theme.fg("accent", "💭 Flux")} ${message.content}`;
		if (expanded && details) {
			text += `\n${theme.fg("dim", `${details.model ?? "unknown model"} · ${details.contextDigest ?? "no digest"}`)}`;
		}
		box.addChild(new Text(text, 0, 0));
		return box;
	});

	pi.on("session_start", async (_event, ctx) => {
		currentCtx = ctx;
		refreshConfig(ctx.cwd);
		state = createInitialState();
		randomEnabled = loaded.config.randomInjections;
		if (ctx.hasUI) ctx.ui.setStatus("flux", loaded.config.enabled ? "flux:on" : "flux:off");
	});

	pi.on("turn_end", async (event, ctx) => {
		if (busy) return;
		busy = true;
		try {
			loaded.config.randomInjections = randomEnabled;
			await runFlux(
				loaded.config,
				state,
				ctx,
				{ host: "pi", kind: "turn-end", timestamp: Date.now(), payload: event },
				snapshotFromPi(ctx, loaded.config),
			);
		} finally {
			busy = false;
		}
	});

	pi.on("tool_result", async (event, ctx) => {
		if (busy) return;
		busy = true;
		try {
			loaded.config.randomInjections = randomEnabled;
			await runFlux(
				loaded.config,
				state,
				ctx,
				{ host: "pi", kind: "tool-result", timestamp: Date.now(), payload: event },
				snapshotFromPi(ctx, loaded.config, event),
			);
		} finally {
			busy = false;
		}
	});

	pi.events.on("flux:trigger", async (payload) => {
		const ctx = currentCtx;
		if (!ctx || busy) return;
		busy = true;
		try {
			await runFlux(
				loaded.config,
				state,
				ctx,
				{ host: "pi", kind: "external", name: "flux:trigger", timestamp: Date.now(), payload },
				snapshotFromPi(ctx, loaded.config, payload),
				{ force: true },
			);
		} finally {
			busy = false;
		}
	});

	pi.registerTool({
		name: "flux_stray_thought",
		label: "Flux Stray Thought",
		description: "Ask Flux's sidecar model for one creative nudge based on the current session context.",
		promptSnippet: "Request a concise Flux stray thought when stuck, looping, or seeking a creative reframe.",
		parameters: Type.Object({
			reason: Type.Optional(Type.String({ description: "Why the stray thought is being requested." })),
			display: Type.Optional(Type.Boolean({ description: "Show the stray thought in the transcript. Default true." })),
			triggerTurn: Type.Optional(Type.Boolean({ description: "Queue another agent turn with the thought. Default false for tool calls." })),
		}),
		async execute(_toolCallId, params, signal, _onUpdate, ctx) {
			const trigger: TriggerEvent = {
				host: "pi",
				kind: "manual",
				name: "flux_stray_thought",
				timestamp: Date.now(),
				payload: { reason: params.reason },
			};
			const thought = await generateStrayThought(loaded.config, state, snapshotFromPi(ctx, loaded.config, params), trigger, {
				signal,
				modelCaller: createPiModelCaller(ctx, signal),
			});
			if (params.display ?? true) {
				pi.sendMessage(
					{ customType: CUSTOM_TYPE, content: renderThoughtForAgent(thought), display: true, details: thought },
					{ deliverAs: "steer", triggerTurn: params.triggerTurn ?? false },
				);
			}
			return { content: [{ type: "text", text: renderThoughtForAgent(thought) }], details: thought };
		},
	});

	pi.registerCommand("flux", {
		description:
			"Manage Flux: /flux status | on | off | random on|off | think [reason] | reload | config [status|init|edit|random|models|prompts]",
		handler: async (args, ctx) => {
			const parts = args.trim().split(/\s+/).filter(Boolean);
			const command = parts[0] ?? "status";
			const configPath = () => loaded.path ?? join(ctx.cwd, ".flux", "config.json");
			const persistLoadedConfig = () => {
				writeConfigFile(configPath(), loaded.config);
				refreshConfig(ctx.cwd);
				randomEnabled = loaded.config.randomInjections;
			};

			if (command === "reload") {
				refreshConfig(ctx.cwd);
				randomEnabled = loaded.config.randomInjections;
				ctx.ui.notify(`Flux config reloaded${loaded.path ? ` from ${loaded.path}` : " (defaults)"}`, "info");
				return;
			}
			if (command === "config") {
				const subcommand = parts[1] ?? "status";
				if (subcommand === "status" || subcommand === "path") {
					ctx.ui.notify(formatConfigSummary(loaded.config, loaded.path), "info");
					return;
				}
				if (subcommand === "init") {
					const path = configPath();
					if (existsSync(path)) {
						ctx.ui.notify(`Flux config already exists: ${path}`, "warning");
						return;
					}
					writeConfigFile(path, cloneDefaultConfig());
					refreshConfig(ctx.cwd);
					randomEnabled = loaded.config.randomInjections;
					ctx.ui.notify(`Created Flux config: ${path}`, "info");
					return;
				}
				if (subcommand === "edit") {
					const path = configPath();
					if (!ctx.hasUI) {
						ctx.ui.notify(`Edit ${path} manually, then run /flux reload.`, "info");
						return;
					}
					const edited = await ctx.ui.editor("Edit Flux config JSON", JSON.stringify(loaded.config, null, 2));
					if (edited === undefined) return;
					let parsed: FluxConfig;
					try {
						parsed = JSON.parse(edited) as FluxConfig;
					} catch (error) {
						ctx.ui.notify(`Invalid JSON: ${error instanceof Error ? error.message : String(error)}`, "error");
						return;
					}
					writeConfigFile(path, parsed);
					refreshConfig(ctx.cwd);
					randomEnabled = loaded.config.randomInjections;
					ctx.ui.notify(`Saved and reloaded Flux config: ${path}`, "info");
					return;
				}
				if (subcommand === "random") {
					const field = parts[2] as keyof FluxConfig["random"] | undefined;
					const value = parts[3];
					if (!field || value === undefined) {
						ctx.ui.notify(
							`Usage: /flux config random probability <0..1> | minIntervalMs <ms> | afterEvents <count>\nCurrent: ${JSON.stringify(loaded.config.random)}`,
							"info",
						);
						return;
					}
					if (!(field in loaded.config.random)) {
						ctx.ui.notify(`Unknown random field: ${field}`, "error");
						return;
					}
					const numeric = Number(value);
					if (!Number.isFinite(numeric)) {
						ctx.ui.notify(`Expected numeric value for ${field}`, "error");
						return;
					}
					if (field === "probability" && (numeric < 0 || numeric > 1)) {
						ctx.ui.notify("Probability must be between 0 and 1.", "error");
						return;
					}
					loaded.config.random[field] = numeric;
					persistLoadedConfig();
					ctx.ui.notify(`Set Flux random.${field}=${numeric} in ${loaded.path}`, "info");
					return;
				}
				if (subcommand === "models") {
					const lines = [
						"Flux models:",
						...loaded.config.models.map((model) => `- ${model.name}: ${model.provider}/${model.model}`),
						"Model pools:",
						...Object.entries(loaded.config.modelPools).map(([name, models]) => `- ${name}: ${models.join(", ")}`),
					];
					ctx.ui.notify(lines.join("\n"), "info");
					return;
				}
				if (subcommand === "prompts") {
					const lines = [
						"Flux prompt profiles:",
						...Object.entries(loaded.config.promptProfiles).map(
							([name, profiles]) => `- ${name}: ${profiles.map((profile) => `${profile.name}(${profile.weight ?? 1})`).join(", ")}`,
						),
					];
					ctx.ui.notify(lines.join("\n"), "info");
					return;
				}
				ctx.ui.notify("Usage: /flux config status | init | edit | random | models | prompts", "info");
				return;
			}
			if (command === "on" || command === "off") {
				loaded.config.enabled = command === "on";
				ctx.ui.setStatus("flux", loaded.config.enabled ? "flux:on" : "flux:off");
				ctx.ui.notify(`Flux ${loaded.config.enabled ? "enabled" : "disabled"}`, "info");
				return;
			}
			if (command === "random") {
				randomEnabled = (parts[1] ?? "status") === "on" ? true : (parts[1] ?? "status") === "off" ? false : randomEnabled;
				loaded.config.randomInjections = randomEnabled;
				ctx.ui.notify(`Flux random injections: ${randomEnabled ? "on" : "off"}`, "info");
				return;
			}
			if (command === "think") {
				await runFlux(
					loaded.config,
					state,
					ctx,
					{ host: "pi", kind: "manual", name: "flux command", timestamp: Date.now(), payload: { reason: parts.slice(1).join(" ") } },
					snapshotFromPi(ctx, loaded.config, { reason: parts.slice(1).join(" ") }),
					{ force: true, triggerTurn: ctx.hasUI },
				);
				return;
			}
			ctx.ui.notify(formatConfigSummary(loaded.config, loaded.path), "info");
		},
	});
}
