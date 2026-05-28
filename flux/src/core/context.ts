import { createHash } from "node:crypto";
import type { AgentContextSnapshot, AgentMessage, AgentToolEvent, FluxConfig, HostKind } from "./types.ts";

function clamp(text: string, maxChars: number): string {
	if (text.length <= maxChars) return text;
	return `${text.slice(0, maxChars)}\n…[truncated ${text.length - maxChars} chars]`;
}

export function textFromUnknown(value: unknown): string {
	if (value === null || value === undefined) return "";
	if (typeof value === "string") return value;
	if (typeof value === "number" || typeof value === "boolean") return String(value);
	if (Array.isArray(value)) return value.map(textFromUnknown).filter(Boolean).join("\n");
	if (typeof value === "object") {
		const record = value as Record<string, unknown>;
		if (typeof record.text === "string") return record.text;
		if (typeof record.content === "string") return record.content;
		if (Array.isArray(record.content)) return textFromUnknown(record.content);
		try {
			return JSON.stringify(value);
		} catch {
			return String(value);
		}
	}
	return String(value);
}

export function digestSnapshot(snapshot: AgentContextSnapshot): string {
	return createHash("sha256").update(JSON.stringify(snapshot)).digest("hex").slice(0, 16);
}

export function formatSnapshotForPrompt(snapshot: AgentContextSnapshot, config: FluxConfig): string {
	const lines: string[] = [];
	lines.push(`Host: ${snapshot.host}`);
	if (snapshot.cwd) lines.push(`CWD: ${snapshot.cwd}`);
	if (snapshot.sessionPrompt) lines.push(`Session starting prompt:\n${snapshot.sessionPrompt}`);
	if (snapshot.systemPrompt) lines.push(`System prompt excerpt:\n${clamp(snapshot.systemPrompt, 2_000)}`);
	if (snapshot.lastUserMessages.length > 0) {
		lines.push("Recent user messages:");
		for (const message of snapshot.lastUserMessages) lines.push(`- ${clamp(message.text, 2_000)}`);
	}
	if (snapshot.lastAssistantMessages.length > 0) {
		lines.push("Recent assistant responses:");
		for (const message of snapshot.lastAssistantMessages) lines.push(`- ${clamp(message.text, 2_500)}`);
	}
	if (snapshot.toolEvents.length > 0) {
		lines.push("Recent tool events:");
		for (const event of snapshot.toolEvents) {
			const input = event.input === undefined ? "" : ` input=${clamp(textFromUnknown(event.input), 800)}`;
			const result = event.result === undefined ? "" : ` result=${clamp(textFromUnknown(event.result), 800)}`;
			const err = event.isError ? " ERROR" : "";
			lines.push(`- ${event.name}${err}${input}${result}`);
		}
	}
	return clamp(lines.join("\n\n"), config.context.maxChars);
}

export function snapshotFromGenericPayload(host: HostKind, payload: unknown, config: FluxConfig): AgentContextSnapshot {
	const record = typeof payload === "object" && payload !== null ? (payload as Record<string, unknown>) : {};
	const messages = Array.isArray(record.messages) ? record.messages : [];
	const agentMessages: AgentMessage[] = messages.map((message) => {
		const m = typeof message === "object" && message !== null ? (message as Record<string, unknown>) : {};
		const role = ["system", "user", "assistant", "tool", "custom"].includes(String(m.role))
			? (String(m.role) as AgentMessage["role"])
			: "custom";
		return { role, text: textFromUnknown(m.content ?? m.text ?? message), timestamp: Number(m.timestamp) || undefined };
	});
	const toolEventsRaw = Array.isArray(record.toolEvents) ? record.toolEvents : Array.isArray(record.tools) ? record.tools : [];
	const toolEvents: AgentToolEvent[] = toolEventsRaw.map((event) => {
		const e = typeof event === "object" && event !== null ? (event as Record<string, unknown>) : {};
		return {
			name: String(e.name ?? e.toolName ?? "tool"),
			input: e.input ?? e.args,
			result: e.result,
			isError: Boolean(e.isError),
			timestamp: Number(e.timestamp) || undefined,
		};
	});
	return {
		host,
		cwd: typeof record.cwd === "string" ? record.cwd : process.cwd(),
		sessionPrompt: typeof record.sessionPrompt === "string" ? record.sessionPrompt : undefined,
		systemPrompt: typeof record.systemPrompt === "string" ? record.systemPrompt : undefined,
		lastUserMessages: agentMessages.filter((m) => m.role === "user").slice(-config.context.maxUserMessages),
		lastAssistantMessages: agentMessages.filter((m) => m.role === "assistant").slice(-config.context.maxAssistantMessages),
		toolEvents: toolEvents.slice(-config.context.maxToolEvents),
		metadata: { rawEventType: record.event ?? record.hook_event_name ?? record.type },
	};
}
