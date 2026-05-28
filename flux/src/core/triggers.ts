import type { AgentContextSnapshot, FluxConfig, FluxState, TriggerConfig, TriggerEvent, TriggerKind } from "./types.ts";

export function createInitialState(): FluxState {
	return { observedEvents: 0, lastTriggerAt: {} };
}

function matchesLoopPattern(snapshot: AgentContextSnapshot, trigger: TriggerConfig): boolean {
	const haystack = [
		...snapshot.lastUserMessages.map((m) => m.text),
		...snapshot.lastAssistantMessages.map((m) => m.text),
		...snapshot.toolEvents.map((e) => `${e.name} ${String(e.input ?? "")} ${String(e.result ?? "")}`),
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

export function shouldFireTrigger(
	config: FluxConfig,
	state: FluxState,
	event: TriggerEvent,
	snapshot: AgentContextSnapshot,
	random = Math.random,
): TriggerConfig | undefined {
	if (!config.enabled) return undefined;
	state.observedEvents += 1;

	for (const trigger of config.triggers) {
		if (trigger.enabled === false) continue;
		if (!triggerKindMatches(trigger, event.kind)) continue;
		if (trigger.kind === "random" && !config.randomInjections) continue;
		if (trigger.afterEvents && state.observedEvents < trigger.afterEvents) continue;
		const last = state.lastTriggerAt[trigger.name] ?? 0;
		if (trigger.minIntervalMs && event.timestamp - last < trigger.minIntervalMs) continue;
		if (trigger.tools?.length && event.payload && typeof event.payload === "object") {
			const toolName = String((event.payload as Record<string, unknown>).toolName ?? (event.payload as Record<string, unknown>).name ?? "");
			if (!trigger.tools.includes(toolName)) continue;
		}
		if (trigger.kind === "loop-detected" && !matchesLoopPattern(snapshot, trigger)) continue;
		const p = trigger.probability ?? (trigger.kind === "random" ? 0.1 : 1);
		if (p < 1 && random() > p) continue;
		state.lastTriggerAt[trigger.name] = event.timestamp;
		return trigger;
	}
	return undefined;
}
