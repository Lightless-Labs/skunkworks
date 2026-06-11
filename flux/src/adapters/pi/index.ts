import { chmodSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { Box, Text } from "@earendil-works/pi-tui";
import { Type } from "typebox";
import { DEFAULT_CONFIG, loadConfig } from "../../core/config.ts";
import {
	formatPromptProfiles,
	setConfigEnabled,
	setHostSidecar,
	setModelPool,
	setPersistentRandomEnabled,
	setRandomFrequency,
	upsertModel,
	upsertPromptProfile,
	validateFluxConfig,
} from "../../core/configActions.ts";
import { snapshotFromGenericPayload, textFromUnknown } from "../../core/context.ts";
import { piDeliverAs, supportedDeliveryModes } from "../../core/delivery.ts";
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
	chmodSync(path, 0o600);
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
		`hostSidecar=${JSON.stringify(config.hostSidecar)}`,
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
	options: { apiKey: string; headers?: Record<string, string>; signal?: AbortSignal; reasoning?: string },
) => Promise<{ stopReason?: string; content: Array<{ type: string; text?: string }> }>;

async function loadPiComplete(): Promise<PiComplete> {
	try {
		// @ts-ignore: pi-ai is provided by the host Pi runtime; Flux keeps it out of direct core deps.
		const piAi = (await import("@earendil-works/pi-ai")) as { complete: PiComplete; completeSimple?: PiComplete };
		return piAi.completeSimple ?? piAi.complete;
	} catch (error) {
		const codingAgentUrl = import.meta.resolve("@earendil-works/pi-coding-agent");
		const bundledPiAiUrl = new URL("../node_modules/@earendil-works/pi-ai/dist/index.js", codingAgentUrl).href;
		try {
			const piAi = (await import(bundledPiAiUrl)) as { complete: PiComplete; completeSimple?: PiComplete };
			return piAi.completeSimple ?? piAi.complete;
		} catch {
			throw error;
		}
	}
}

function modelLabel(model: { provider: string; id: string }): string {
	return `${model.provider}/${model.id}`;
}

async function resolvePiSidecarModel(ctx: ExtensionContext, config: FluxConfig): Promise<NonNullable<ExtensionContext["model"]>> {
	const preference = config.hostSidecar.pi?.model ?? "active";
	if (preference === "active") {
		if (!ctx.model) throw new Error("No Pi model selected for Flux host-native sidecar generation.");
		return ctx.model;
	}
	const slash = preference.indexOf("/");
	if (slash > 0) {
		const found = ctx.modelRegistry.find(preference.slice(0, slash), preference.slice(slash + 1));
		if (found) return found as NonNullable<ExtensionContext["model"]>;
	}
	const available = await Promise.resolve(ctx.modelRegistry.getAvailable());
	const lower = preference.toLowerCase();
	const found = available.find((model: any) => model.id?.toLowerCase?.() === lower || model.name?.toLowerCase?.() === lower || modelLabel(model).toLowerCase() === lower);
	if (found) return found as NonNullable<ExtensionContext["model"]>;
	throw new Error(`Flux Pi sidecar model not found or unavailable: ${preference}`);
}

const THINKING_LEVEL_ORDER = ["off", "minimal", "low", "medium", "high", "xhigh"];

function supportedThinkingLevels(model: NonNullable<ExtensionContext["model"]>): string[] {
	if (!(model as any).reasoning) return ["off"];
	return THINKING_LEVEL_ORDER.filter((level) => {
		const mapped = (model as any).thinkingLevelMap?.[level];
		if (mapped === null) return false;
		if (level === "xhigh") return mapped !== undefined;
		return true;
	});
}

function clampThinkingLevel(model: NonNullable<ExtensionContext["model"]>, requested: string): string {
	const supported = supportedThinkingLevels(model);
	if (supported.includes(requested)) return requested;
	const requestedIndex = THINKING_LEVEL_ORDER.indexOf(requested);
	if (requestedIndex === -1) return supported[0] ?? "off";
	for (let i = requestedIndex; i < THINKING_LEVEL_ORDER.length; i++) if (supported.includes(THINKING_LEVEL_ORDER[i]!)) return THINKING_LEVEL_ORDER[i]!;
	for (let i = requestedIndex - 1; i >= 0; i--) if (supported.includes(THINKING_LEVEL_ORDER[i]!)) return THINKING_LEVEL_ORDER[i]!;
	return supported[0] ?? "off";
}

function resolvePiThinking(model: NonNullable<ExtensionContext["model"]>, config: FluxConfig): string | undefined {
	const preference = config.hostSidecar.pi?.thinkingEffort ?? "active";
	if (preference === "active") return undefined;
	return clampThinkingLevel(model, preference);
}

function createPiModelCaller(ctx: ExtensionContext, signal?: AbortSignal): ThoughtModelCaller {
	return async ({ config, systemPrompt, prompt }) => {
		const model = await resolvePiSidecarModel(ctx, config);
		const auth = await ctx.modelRegistry.getApiKeyAndHeaders(model);
		if (!auth.ok || !auth.apiKey) throw new Error(auth.ok ? `No API key for ${model.provider}` : auth.error);
		const complete = await loadPiComplete();
		const reasoning = resolvePiThinking(model, config);
		const userMessage: PiMessage = {
			role: "user",
			content: [{ type: "text", text: prompt }],
			timestamp: Date.now(),
		};
		const response = await complete(
			model,
			{ systemPrompt, messages: [userMessage] },
			{ apiKey: auth.apiKey, headers: auth.headers, signal, ...(reasoning && reasoning !== "off" ? { reasoning } : {}) },
		);
		if (response.stopReason === "aborted") throw new Error("Flux Pi sidecar generation was aborted.");
		const content = response.content
			.filter((part): part is { type: "text"; text: string } => part.type === "text" && typeof part.text === "string")
			.map((part) => part.text)
			.join("\n");
		return { content, model: hostNativeModelLabel("pi", `${modelLabel(model)}${reasoning ? `:${reasoning}` : ""}`) };
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
		const deliverAs = piDeliverAs(config.delivery);
		if (!deliverAs) {
			currentCtx?.ui.notify(
				`Flux delivery mode "${String(config.delivery)}" is not supported by the Pi adapter. Use ${supportedDeliveryModes()}. Hook integrations always emit their host JSON on stdout.`,
				"error",
			);
			return;
		}
		pi.sendMessage(
			{
				customType: CUSTOM_TYPE,
				content,
				display,
				details: thought,
			},
			{ deliverAs, triggerTurn },
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
			"Manage Flux: /flux status | on | off | random on|off | think [reason] | reload | config [status|init|edit|set|random|model|host|pool|prompt|models|prompts]",
		handler: async (args, ctx) => {
			const parts = args.trim().split(/\s+/).filter(Boolean);
			const command = parts[0] ?? "status";
			const configPath = () => loaded.path ?? join(ctx.cwd, ".flux", "config.json");
			const persistLoadedConfig = (): boolean => {
				const validation = validateFluxConfig(loaded.config);
				if (!validation.ok) {
					ctx.ui.notify(validation.message, "error");
					return false;
				}
				const path = configPath();
				const nextRandomEnabled = loaded.config.randomInjections;
				writeConfigFile(path, loaded.config);
				refreshConfig(ctx.cwd);
				randomEnabled = nextRandomEnabled;
				loaded.config.randomInjections = randomEnabled;
				return true;
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
					const validation = validateFluxConfig(parsed);
					if (!validation.ok) {
						ctx.ui.notify(validation.message, "error");
						return;
					}
					writeConfigFile(path, parsed);
					refreshConfig(ctx.cwd);
					randomEnabled = loaded.config.randomInjections;
					ctx.ui.notify(`Saved and reloaded Flux config: ${path}`, "info");
					return;
				}
				if (subcommand === "set") {
					if (parts[2] !== "enabled") {
						ctx.ui.notify("Usage: /flux config set enabled true|false", "info");
						return;
					}
					const result = setConfigEnabled(loaded.config, parts[3]);
					if (!result.ok) {
						ctx.ui.notify(result.message, "error");
						return;
					}
					if (!persistLoadedConfig()) return;
					ctx.ui.setStatus("flux", loaded.config.enabled ? "flux:on" : "flux:off");
					ctx.ui.notify(`${result.message} in ${loaded.path}`, "info");
					return;
				}
				if (subcommand === "random") {
					if (parts[2] === "on" || parts[2] === "off") {
						const result = setPersistentRandomEnabled(loaded.config, parts[2]);
						if (!result.ok) {
							ctx.ui.notify(result.message, "error");
							return;
						}
						if (!persistLoadedConfig()) return;
						ctx.ui.notify(`${result.message} in ${loaded.path}`, "info");
						return;
					}
					const result = setRandomFrequency(loaded.config, parts[2], parts[3]);
					if (!result.ok) {
						ctx.ui.notify(result.message, "error");
						return;
					}
					if (!persistLoadedConfig()) return;
					ctx.ui.notify(`${result.message} in ${loaded.path}`, "info");
					return;
				}
				if (subcommand === "model") {
					const result = upsertModel(loaded.config, parts.slice(2));
					if (!result.ok) {
						ctx.ui.notify(result.message, "error");
						return;
					}
					if (!persistLoadedConfig()) return;
					ctx.ui.notify(`${result.message} in ${loaded.path}`, "info");
					return;
				}
				if (subcommand === "host") {
					if (parts[2] === "models") {
						const available = await Promise.resolve(ctx.modelRegistry.getAvailable());
						const lines = [
							"Available host models:",
							...available.map((model: any) => {
								const thinking = supportedThinkingLevels(model).join("|");
								return `- ${model.provider}/${model.id} (${model.name ?? model.id}; thinking: ${thinking})`;
							}),
						];
						ctx.ui.notify(lines.join("\n"), "info");
						return;
					}
					const result = setHostSidecar(loaded.config, parts.slice(2));
					if (!result.ok) {
						ctx.ui.notify(result.message, "error");
						return;
					}
					if (!persistLoadedConfig()) return;
					ctx.ui.notify(`${result.message} in ${loaded.path}`, "info");
					return;
				}
				if (subcommand === "pool") {
					const result = setModelPool(loaded.config, parts[2], parts.slice(3).join(" "));
					if (!result.ok) {
						ctx.ui.notify(result.message, "error");
						return;
					}
					if (!persistLoadedConfig()) return;
					ctx.ui.notify(`${result.message} in ${loaded.path}`, "info");
					return;
				}
				if (subcommand === "prompt") {
					const result = upsertPromptProfile(loaded.config, parts.slice(2));
					if (!result.ok) {
						ctx.ui.notify(result.message, "error");
						return;
					}
					if (!persistLoadedConfig()) return;
					ctx.ui.notify(`${result.message} in ${loaded.path}`, "info");
					return;
				}
				if (subcommand === "models") {
					const lines = [
						"Flux models:",
						...loaded.config.models.map((model) => `- ${model.name}: ${model.provider}/${model.model}${model.thinkingEffort ? `:${model.thinkingEffort}` : ""}`),
						"Host sidecar:",
						...Object.entries(loaded.config.hostSidecar).map(([host, settings]) => `- ${host}: model=${settings?.model ?? "active"}, thinking=${settings?.thinkingEffort ?? "active"}`),
						"Model pools:",
						...Object.entries(loaded.config.modelPools).map(([name, models]) => `- ${name}: ${models.join(", ")}`),
					];
					ctx.ui.notify(lines.join("\n"), "info");
					return;
				}
				if (subcommand === "prompts") {
					ctx.ui.notify(formatPromptProfiles(loaded.config), "info");
					return;
				}
				ctx.ui.notify("Usage: /flux config status | init | edit | set enabled | random | model | host | pool | prompt | models | prompts", "info");
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
