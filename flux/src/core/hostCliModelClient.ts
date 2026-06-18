import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ThoughtModelCaller } from "./engine.ts";
import { hostNativeModelLabel } from "./engine.ts";
import { claudeEffortArgument, codexReasoningEffortArgument, isActiveHostPreference } from "./hostSidecar.ts";
import type { FluxConfig, HostKind } from "./types.ts";

interface SpawnResult {
	stdout: string;
	stderr: string;
	code: number | null;
	signal: NodeJS.Signals | null;
}

const MAX_CAPTURE_CHARS = 1_000_000;

function appendBounded(current: string, chunk: Buffer): string {
	const next = current + chunk.toString("utf8");
	if (next.length <= MAX_CAPTURE_CHARS) return next;
	return next.slice(next.length - MAX_CAPTURE_CHARS);
}

function spawnWithInput(
	command: string,
	args: string[],
	input: string,
	options: { cwd?: string; signal?: AbortSignal; timeoutMs?: number } = {},
): Promise<SpawnResult> {
	return new Promise((resolve, reject) => {
		const child = spawn(command, args, {
			cwd: options.cwd,
			shell: false,
			stdio: ["pipe", "pipe", "pipe"],
			env: { ...process.env, FLUX_SUPPRESS: "1" },
		});
		let stdout = "";
		let stderr = "";
		let settled = false;
		const timeout = options.timeoutMs
			? setTimeout(() => {
					child.kill("SIGTERM");
					setTimeout(() => {
						if (!child.killed) child.kill("SIGKILL");
					}, 5_000).unref();
				}, options.timeoutMs)
			: undefined;

		const abort = () => child.kill("SIGTERM");
		if (options.signal?.aborted) abort();
		else options.signal?.addEventListener("abort", abort, { once: true });

		child.stdout.on("data", (chunk) => {
			stdout = appendBounded(stdout, chunk);
		});
		child.stderr.on("data", (chunk) => {
			stderr = appendBounded(stderr, chunk);
		});
		child.on("error", (error) => {
			if (settled) return;
			settled = true;
			if (timeout) clearTimeout(timeout);
			options.signal?.removeEventListener("abort", abort);
			reject(error);
		});
		child.on("close", (code, signal) => {
			if (settled) return;
			settled = true;
			if (timeout) clearTimeout(timeout);
			options.signal?.removeEventListener("abort", abort);
			resolve({ stdout, stderr, code, signal });
		});
		child.stdin.end(input);
	});
}

function assertSuccess(result: SpawnResult, label: string): void {
	if (result.code === 0) return;
	const detail = result.stderr.trim() || result.stdout.trim() || (result.signal ? `terminated by ${result.signal}` : `exit ${result.code}`);
	throw new Error(`${label} failed: ${detail.slice(0, 2_000)}`);
}

function hostPreference(config: FluxConfig | undefined, host: HostKind): { model?: string; thinkingEffort?: string } {
	return config?.hostSidecar?.[host] ?? {};
}

async function callClaudeCli(
	prompt: string,
	systemPrompt: string,
	cwd: string | undefined,
	signal: AbortSignal | undefined,
	config: FluxConfig,
): Promise<{ content: string; detail: string }> {
	const command = process.env.FLUX_CLAUDE_COMMAND || "claude";
	const preference = hostPreference(config, "claude-code");
	const args = ["-p", "--no-session-persistence", "--tools", ""];
	if (!isActiveHostPreference(preference.model)) args.push("--model", preference.model!);
	const effort = claudeEffortArgument(preference.thinkingEffort);
	if (effort) args.push("--effort", effort);
	args.push("--system-prompt", systemPrompt);
	const result = await spawnWithInput(command, args, prompt, { cwd, signal, timeoutMs: 120_000 });
	const detailParts = ["claude-cli"];
	if (!isActiveHostPreference(preference.model)) detailParts.push(preference.model!);
	if (effort) detailParts.push(effort);
	const detail = detailParts.join("/");
	assertSuccess(result, "claude CLI sidecar");
	return { content: result.stdout.trim(), detail };
}

async function callCodexCli(
	prompt: string,
	systemPrompt: string,
	cwd: string | undefined,
	signal: AbortSignal | undefined,
	config: FluxConfig,
): Promise<{ content: string; detail: string }> {
	const command = process.env.FLUX_CODEX_COMMAND || "codex";
	const preference = hostPreference(config, "codex");
	const tmp = mkdtempSync(join(tmpdir(), "flux-codex-"));
	const outputPath = join(tmp, "last-message.txt");
	try {
		const args = ["--ask-for-approval", "never", "exec"];
		if (!isActiveHostPreference(preference.model)) args.push("-m", preference.model!);
		const effort = codexReasoningEffortArgument(preference.thinkingEffort);
		if (effort) {
			args.push("-c", `model_reasoning_effort=${JSON.stringify(effort)}`);
		}
		args.push(
			"--sandbox",
			"read-only",
			"--ephemeral",
			"--skip-git-repo-check",
			"--output-last-message",
			outputPath,
			"-",
		);
		const input = [`System instructions:\n${systemPrompt}`, `User request:\n${prompt}`].join("\n\n");
		const result = await spawnWithInput(command, args, input, { cwd, signal, timeoutMs: 120_000 });
		assertSuccess(result, "codex CLI sidecar");
		const detailParts = ["codex-cli"];
		if (!isActiveHostPreference(preference.model)) detailParts.push(preference.model!);
		if (effort) detailParts.push(effort);
		try {
			return { content: readFileSync(outputPath, "utf8").trim(), detail: detailParts.join("/") };
		} catch {
			return { content: result.stdout.trim(), detail: detailParts.join("/") };
		}
	} finally {
		rmSync(tmp, { recursive: true, force: true });
	}
}

function withActiveHostModel(config: FluxConfig, host: HostKind): FluxConfig {
	return {
		...config,
		hostSidecar: {
			...config.hostSidecar,
			[host]: { ...(config.hostSidecar[host] ?? {}), model: "active" },
		},
	};
}

export function createHostCliModelCaller(host: HostKind, cwd?: string): ThoughtModelCaller | undefined {
	if (host !== "claude-code" && host !== "codex") return undefined;
	return async ({ config, prompt, systemPrompt, signal }) => {
		const preference = hostPreference(config, host);
		try {
			if (host === "claude-code") {
				const response = await callClaudeCli(prompt, systemPrompt, cwd, signal, config);
				return { content: response.content, model: hostNativeModelLabel(host, response.detail) };
			}
			const response = await callCodexCli(prompt, systemPrompt, cwd, signal, config);
			return { content: response.content, model: hostNativeModelLabel(host, response.detail) };
		} catch (error) {
			if (isActiveHostPreference(preference.model)) throw error;
			const message = error instanceof Error ? error.message : String(error);
			const warning = `Flux ${host} sidecar model failed or is unavailable: ${preference.model}; falling back to active host model. ${message}`;
			const fallbackConfig = withActiveHostModel(config, host);
			if (host === "claude-code") {
				const response = await callClaudeCli(prompt, systemPrompt, cwd, signal, fallbackConfig);
				return { content: response.content, model: hostNativeModelLabel(host, response.detail), warning };
			}
			const response = await callCodexCli(prompt, systemPrompt, cwd, signal, fallbackConfig);
			return { content: response.content, model: hostNativeModelLabel(host, response.detail), warning };
		}
	};
}
