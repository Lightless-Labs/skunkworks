import { createHash } from "node:crypto";
import { textFromUnknown } from "./context.ts";
import type { AgentContextSnapshot, AgentToolEvent, FluxConfig, FluxState, ToolFingerprintEvent, TriggerConfig, TriggerEvent, TriggerKind } from "./types.ts";

const DEFAULT_REPEAT_WINDOW_EVENTS = 12;
const MAX_TOOL_FINGERPRINT_HISTORY = 40;

export function createInitialState(): FluxState {
	return { observedEvents: 0, lastTriggerAt: {}, recentToolFingerprints: [] };
}

function matchesLoopPattern(snapshot: AgentContextSnapshot, trigger: TriggerConfig): boolean {
	const haystack = [
		...snapshot.lastUserMessages.map((m) => m.text),
		...snapshot.lastAssistantMessages.map((m) => m.text),
		...snapshot.toolEvents.map((e) => `${e.name} ${textFromUnknown(e.input)} ${textFromUnknown(e.result)}`),
	]
		.join("\n")
		.toLowerCase();
	return (trigger.patterns ?? []).some((pattern) => {
		try {
			return new RegExp(pattern, "i").test(haystack);
		} catch {
			return haystack.includes(pattern.toLowerCase());
		}
	});
}

function triggerKindMatches(trigger: TriggerConfig, eventKind: TriggerKind): boolean {
	if (trigger.kind === eventKind) return true;
	if (trigger.kind === "random" && (eventKind === "turn-end" || eventKind === "tool-result")) return true;
	if (trigger.kind === "loop-detected" && (eventKind === "turn-end" || eventKind === "tool-result")) return true;
	return false;
}

function normalizeFingerprintBase(value: unknown): string {
	return textFromUnknown(value)
		.toLowerCase()
		.replace(/\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/gi, "<uuid>")
		.replace(/\b\d{4}-\d{2}-\d{2}t\d{2}:\d{2}:\d{1,2}(?:\.\d+)?z\b/gi, "<timestamp>")
		.replace(/\b0x[0-9a-f]+\b/gi, "<hex>")
		.replace(/\b[0-9a-f]{16,}\b/gi, "<hex>")
		.replace(/\/var\/folders\/\S+|\/tmp\/\S+|\/private\/var\/folders\/\S+/g, "<tmp-path>")
		.replace(/\s+/g, " ")
		.trim()
		.slice(0, 4_000);
}

function normalizeFingerprintResult(value: unknown): string {
	return normalizeFingerprintBase(value).replace(/\b\d+\b/g, "<number>");
}

function hashToolFingerprint(toolEvent: AgentToolEvent): string {
	return createHash("sha256")
		.update(
			[
				normalizeFingerprintBase(toolEvent.name),
				normalizeFingerprintBase(toolEvent.input),
				normalizeFingerprintResult(toolEvent.result),
				toolEvent.isError ? "error" : "ok",
			].join("\n---\n"),
		)
		.digest("hex")
		.slice(0, 24);
}

function toolEventFromPayload(payload: unknown): AgentToolEvent | undefined {
	if (typeof payload !== "object" || payload === null) return undefined;
	const record = payload as Record<string, unknown>;
	const name = record.toolName ?? record.name;
	if (name === undefined) return undefined;
	return {
		name: String(name),
		input: record.input ?? record.args,
		result: record.result ?? record.content ?? record.details,
		isError: Boolean(record.isError),
		timestamp: Number(record.timestamp) || undefined,
	};
}

function latestToolEvent(event: TriggerEvent, snapshot: AgentContextSnapshot): AgentToolEvent | undefined {
	return toolEventFromPayload(event.payload) ?? snapshot.toolEvents.at(-1);
}

function fingerprintToolEvent(toolEvent: AgentToolEvent, timestamp: number): ToolFingerprintEvent {
	return {
		fingerprint: hashToolFingerprint(toolEvent),
		toolName: toolEvent.name,
		isError: toolEvent.isError,
		timestamp,
	};
}

function observeToolFingerprint(state: FluxState, event: TriggerEvent, snapshot: AgentContextSnapshot): void {
	state.recentToolFingerprints ??= [];
	if (event.kind !== "tool-result") return;
	const toolEvent = latestToolEvent(event, snapshot);
	if (!toolEvent) return;
	state.recentToolFingerprints.push(fingerprintToolEvent(toolEvent, event.timestamp));
	if (state.recentToolFingerprints.length > MAX_TOOL_FINGERPRINT_HISTORY) {
		state.recentToolFingerprints.splice(0, state.recentToolFingerprints.length - MAX_TOOL_FINGERPRINT_HISTORY);
	}
}

function matchesRepeatedToolFingerprint(state: FluxState, trigger: TriggerConfig): boolean {
	const threshold = trigger.repeatThreshold;
	if (!threshold || threshold <= 1) return false;
	const recent = state.recentToolFingerprints ?? [];
	const latest = recent.at(-1);
	if (!latest) return false;
	const windowSize = trigger.repeatWindowEvents ?? DEFAULT_REPEAT_WINDOW_EVENTS;
	const window = recent.slice(-windowSize);
	const matches = window.filter((item) => {
		if (item.fingerprint !== latest.fingerprint) return false;
		if (trigger.repeatRequireError && !item.isError) return false;
		return true;
	});
	return matches.length >= threshold;
}

function matchesLoop(snapshot: AgentContextSnapshot, state: FluxState, trigger: TriggerConfig, event: TriggerEvent): boolean {
	return matchesLoopPattern(snapshot, trigger) || (event.kind === "tool-result" && matchesRepeatedToolFingerprint(state, trigger));
}

export function shouldFireTrigger(
	config: FluxConfig,
	state: FluxState,
	event: TriggerEvent,
	snapshot: AgentContextSnapshot,
	random = Math.random,
): TriggerConfig | undefined {
	if (!config.enabled) return undefined;
	state.observedEvents += 1;
	observeToolFingerprint(state, event, snapshot);

	for (const trigger of config.triggers) {
		if (trigger.enabled === false) continue;
		if (!triggerKindMatches(trigger, event.kind)) continue;
		if (trigger.kind === "random" && !config.randomInjections) continue;
		const afterEvents = trigger.afterEvents ?? (trigger.kind === "random" ? config.random.afterEvents : undefined);
		if (afterEvents && state.observedEvents < afterEvents) continue;
		const last = state.lastTriggerAt[trigger.name] ?? 0;
		const minIntervalMs = trigger.minIntervalMs ?? (trigger.kind === "random" ? config.random.minIntervalMs : undefined);
		if (minIntervalMs && event.timestamp - last < minIntervalMs) continue;
		if (trigger.tools?.length && event.payload && typeof event.payload === "object") {
			const toolName = String((event.payload as Record<string, unknown>).toolName ?? (event.payload as Record<string, unknown>).name ?? "");
			if (!trigger.tools.includes(toolName)) continue;
		}
		if (trigger.kind === "loop-detected" && !matchesLoop(snapshot, state, trigger, event)) continue;
		const p = trigger.probability ?? (trigger.kind === "random" ? config.random.probability : 1);
		if (p <= 0) continue;
		if (p < 1 && random() >= p) continue;
		state.lastTriggerAt[trigger.name] = event.timestamp;
		return trigger;
	}
	return undefined;
}
