import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ThoughtModelCaller } from "./engine.ts";
import { hostNativeModelLabel } from "./engine.ts";
import type { HostKind } from "./types.ts";

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

async function callClaudeCli(prompt: string, systemPrompt: string, cwd: string | undefined, signal: AbortSignal | undefined): Promise<string> {
	const command = process.env.FLUX_CLAUDE_COMMAND || "claude";
	const args = ["-p", "--no-session-persistence", "--tools", "", "--system-prompt", systemPrompt];
	const result = await spawnWithInput(command, args, prompt, { cwd, signal, timeoutMs: 120_000 });
	assertSuccess(result, "claude CLI sidecar");
	return result.stdout.trim();
}

async function callCodexCli(prompt: string, systemPrompt: string, cwd: string | undefined, signal: AbortSignal | undefined): Promise<string> {
	const command = process.env.FLUX_CODEX_COMMAND || "codex";
	const tmp = mkdtempSync(join(tmpdir(), "flux-codex-"));
	const outputPath = join(tmp, "last-message.txt");
	try {
		const args = [
			"--ask-for-approval",
			"never",
			"exec",
			"--sandbox",
			"read-only",
			"--ephemeral",
			"--skip-git-repo-check",
			"--output-last-message",
			outputPath,
			"-",
		];
		const input = [`System instructions:\n${systemPrompt}`, `User request:\n${prompt}`].join("\n\n");
		const result = await spawnWithInput(command, args, input, { cwd, signal, timeoutMs: 120_000 });
		assertSuccess(result, "codex CLI sidecar");
		try {
			return readFileSync(outputPath, "utf8").trim();
		} catch {
			return result.stdout.trim();
		}
	} finally {
		rmSync(tmp, { recursive: true, force: true });
	}
}

export function createHostCliModelCaller(host: HostKind, cwd?: string): ThoughtModelCaller | undefined {
	if (host !== "claude-code" && host !== "codex") return undefined;
	return async ({ prompt, systemPrompt, signal }) => {
		if (host === "claude-code") {
			return { content: await callClaudeCli(prompt, systemPrompt, cwd, signal), model: hostNativeModelLabel(host, "claude-cli") };
		}
		return { content: await callCodexCli(prompt, systemPrompt, cwd, signal), model: hostNativeModelLabel(host, "codex-cli") };
	};
}
