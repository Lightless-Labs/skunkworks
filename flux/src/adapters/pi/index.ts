import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { Box, Text } from "@earendil-works/pi-tui";
import { Type } from "typebox";
import { loadConfig } from "../../core/config.ts";
import { snapshotFromGenericPayload, textFromUnknown } from "../../core/context.ts";
import { generateStrayThought, renderThoughtForAgent } from "../../core/engine.ts";
import { createInitialState, shouldFireTrigger } from "../../core/triggers.ts";
import type { AgentContextSnapshot, AgentMessage, AgentToolEvent, FluxConfig, FluxState, TriggerEvent } from "../../core/types.ts";

const CUSTOM_TYPE = "flux:stray-thought";

function messageText(message: any): string {
	return textFromUnknown(message?.content ?? message?.text ?? "");
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
	const thought = await generateStrayThought(config, state, snapshot, triggerEvent, ctx.signal);
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
			const thought = await generateStrayThought(loaded.config, state, snapshotFromPi(ctx, loaded.config, params), trigger, signal);
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
		description: "Manage Flux: /flux status | on | off | random on|off | think [reason] | reload",
		handler: async (args, ctx) => {
			const parts = args.trim().split(/\s+/).filter(Boolean);
			const command = parts[0] ?? "status";
			if (command === "reload") {
				refreshConfig(ctx.cwd);
				ctx.ui.notify(`Flux config reloaded${loaded.path ? ` from ${loaded.path}` : " (defaults)"}`, "info");
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
					{ force: true, triggerTurn: true },
				);
				return;
			}
			ctx.ui.notify(
				`Flux ${loaded.config.enabled ? "on" : "off"}; random ${randomEnabled ? "on" : "off"}; models ${loaded.config.models.length}; config ${loaded.path ?? "defaults"}`,
				"info",
			);
		},
	});
}
