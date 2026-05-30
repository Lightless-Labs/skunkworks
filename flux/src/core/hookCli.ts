import { readFileSync } from "node:fs";
import { loadConfig } from "./config.ts";
import { snapshotFromGenericPayload } from "./context.ts";
import { generateStrayThought, renderThoughtForAgent } from "./engine.ts";
import { createHostCliModelCaller } from "./hostCliModelClient.ts";
import { createInitialState, shouldFireTrigger } from "./triggers.ts";
import type { HostKind, TriggerEvent, TriggerKind } from "./types.ts";

export interface HookCliOptions {
	host: HostKind;
}

async function readStdin(): Promise<string> {
	if (process.stdin.isTTY) return "";
	const chunks: Buffer[] = [];
	for await (const chunk of process.stdin) chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
	return Buffer.concat(chunks).toString("utf8");
}

function parsePayload(raw: string): unknown {
	if (!raw.trim()) return {};
	try {
		return JSON.parse(raw);
	} catch {
		return { text: raw };
	}
}

export function inferKind(payload: unknown): TriggerKind {
	const record = typeof payload === "object" && payload !== null ? (payload as Record<string, unknown>) : {};
	const name = String(record.hook_event_name ?? record.event ?? record.type ?? "").toLowerCase();
	if (/tool.*result|post[_-]?tool|posttool|tool_response/.test(name)) return "tool-result";
	if (/tool/.test(name)) return "tool-call";
	if (/stop|post[_-]?turn|turn.*end|assistant.*end|response.*done/.test(name)) return "turn-end";
	if (/start|prompt|user/.test(name)) return "turn-start";
	if (/flux|manual|external/.test(name)) return "external";
	return "external";
}

export function formatHookOutput(host: HostKind, rendered: string, thought: unknown, fired: boolean): Record<string, unknown> {
	if (!fired) return { continue: true, flux: { fired: false } };
	if (host === "claude-code") {
		return {
			continue: true,
			hookSpecificOutput: { hookEventName: "Flux", additionalContext: rendered },
			additionalContext: rendered,
			flux: thought,
		};
	}
	if (host === "codex") return { continue: true, instructions: rendered, additionalContext: rendered, flux: thought };
	return { continue: true, additionalContext: rendered, flux: thought };
}

function output(host: HostKind, rendered: string, thought: unknown, fired: boolean) {
	process.stdout.write(JSON.stringify(formatHookOutput(host, rendered, thought, fired)) + "\n");
}

export async function runHookCli(options: HookCliOptions): Promise<void> {
	try {
		if (process.env.FLUX_SUPPRESS === "1") return output(options.host, "", undefined, false);
		const cwd = process.cwd();
		const { config } = loadConfig(cwd);
		const raw = await readStdin();
		const payload = parsePayload(raw);
		const snapshot = snapshotFromGenericPayload(options.host, payload, config);
		const event: TriggerEvent = { host: options.host, kind: inferKind(payload), timestamp: Date.now(), payload };
		const state = createInitialState();
		const force = process.argv.includes("--force") || event.kind === "external";
		const trigger = force ? { name: "hook", kind: event.kind, enabled: true } : shouldFireTrigger(config, state, event, snapshot);
		if (!config.enabled || !trigger) return output(options.host, "", undefined, false);
		event.name = trigger.name;
		const thought = await generateStrayThought(config, state, snapshot, event, {
			modelCaller: createHostCliModelCaller(options.host, snapshot.cwd),
		});
		output(options.host, renderThoughtForAgent(thought), thought, true);
	} catch (error) {
		const message = error instanceof Error ? error.message : String(error);
		process.stderr.write(`flux-hook: ${message}\n`);
		process.stdout.write(JSON.stringify({ continue: true, flux: { error: message } }) + "\n");
	}
}

export function runHookCliSyncFixture(path: string, host: HostKind = "generic") {
	return snapshotFromGenericPayload(host, JSON.parse(readFileSync(path, "utf8")), loadConfig(process.cwd()).config);
}
