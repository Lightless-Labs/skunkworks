import type { FluxConfig, HostKind } from "./types.ts";

export function isActiveHostPreference(value: string | undefined): boolean {
	return value === undefined || value === "" || value === "active";
}

export function claudeEffortArgument(value: string | undefined): string | undefined {
	if (isActiveHostPreference(value) || value === "off") return undefined;
	// Claude Code currently accepts low, medium, high, xhigh, and max. Flux's
	// cross-host config exposes minimal rather than max, so clamp minimal to the
	// lowest Claude-supported effort instead of emitting an invalid flag.
	if (value === "minimal") return "low";
	return value;
}

export function codexReasoningEffortArgument(value: string | undefined): string | undefined {
	if (isActiveHostPreference(value) || value === "off") return undefined;
	return value;
}

function modelArg(host: HostKind, model: string | undefined): string {
	if (isActiveHostPreference(model)) return "active/default";
	if (host === "codex") return `-m ${model}`;
	if (host === "claude-code") return `--model ${model}`;
	return model!;
}

function thinkingArg(host: HostKind, thinking: string | undefined): string {
	if (thinking === "off") return "no CLI arg (off requested)";
	if (host === "claude-code") {
		const effort = claudeEffortArgument(thinking);
		return effort ? `--effort ${effort}` : "active/default";
	}
	if (host === "codex") {
		const effort = codexReasoningEffortArgument(thinking);
		return effort ? `-c model_reasoning_effort=${JSON.stringify(effort)}` : "active/default";
	}
	if (isActiveHostPreference(thinking)) return "active/default";
	return thinking!;
}

export function formatHostSidecarStatus(config: FluxConfig): string[] {
	const lines: string[] = [];
	const hosts: HostKind[] = ["pi", "claude-code", "codex"];
	for (const host of hosts) {
		const settings = config.hostSidecar[host] ?? {};
		const model = settings.model ?? "active";
		const thinking = settings.thinkingEffort ?? "active";
		if (host === "pi") {
			lines.push(`- pi: configured model=${model}, configured thinking=${thinking}; resolved via Pi registry in /flux status`);
			continue;
		}
		const resolvedNote = host === "claude-code" ? "resolved by Claude Code CLI; sidecar uses --safe-mode" : "resolved by Codex CLI";
		lines.push(
			`- ${host}: configured model=${model}, effective model arg=${modelArg(host, model)}, configured thinking=${thinking}, effective thinking arg=${thinkingArg(host, thinking)}; ${resolvedNote}`,
		);
	}
	return lines;
}
