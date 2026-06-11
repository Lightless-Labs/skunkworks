import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { DEFAULT_CONFIG } from "../src/core/config.ts";
import { createHostCliModelCaller } from "../src/core/hostCliModelClient.ts";
import type { FluxConfig } from "../src/core/types.ts";

function configWithHost(host: "codex" | "claude-code", settings: { model?: string; thinkingEffort?: string }): FluxConfig {
	const config = structuredClone(DEFAULT_CONFIG) as FluxConfig;
	config.hostSidecar[host] = settings;
	return config;
}

function withFakeCommand(body: string): { dir: string; command: string; capturePath: string; cleanup: () => void } {
	const dir = mkdtempSync(join(tmpdir(), "flux-host-cli-test-"));
	const command = join(dir, "fake-cli.mjs");
	const capturePath = join(dir, "capture.json");
	writeFileSync(command, `#!/usr/bin/env node\n${body}\n`, "utf8");
	chmodSync(command, 0o700);
	return { dir, command, capturePath, cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

test("createHostCliModelCaller invokes Codex with approval policy before exec subcommand", async () => {
	const fake = withFakeCommand(`
import { readFileSync, writeFileSync } from "node:fs";
const stdin = readFileSync(0, "utf8");
writeFileSync(process.env.FLUX_CAPTURE_PATH, JSON.stringify({ args: process.argv.slice(2), stdin, suppress: process.env.FLUX_SUPPRESS }));
const outputIndex = process.argv.indexOf("--output-last-message");
if (outputIndex !== -1) writeFileSync(process.argv[outputIndex + 1], "codex sidecar note");
`);
	const previousCommand = process.env.FLUX_CODEX_COMMAND;
	const previousCapture = process.env.FLUX_CAPTURE_PATH;
	process.env.FLUX_CODEX_COMMAND = fake.command;
	process.env.FLUX_CAPTURE_PATH = fake.capturePath;
	try {
		const caller = createHostCliModelCaller("codex", fake.dir);
		assert.ok(caller);
		const result = await caller({ systemPrompt: "system prompt", prompt: "user prompt" } as any);
		const captured = JSON.parse(readFileSync(fake.capturePath, "utf8")) as { args: string[]; stdin: string; suppress: string };

		assert.equal(result.content, "codex sidecar note");
		assert.equal(result.model, "codex/codex-cli");
		assert.deepEqual(captured.args.slice(0, 3), ["--ask-for-approval", "never", "exec"]);
		assert.ok(captured.args.includes("--ephemeral"));
		assert.ok(captured.args.includes("--skip-git-repo-check"));
		assert.equal(captured.args.at(-1), "-");
		assert.match(captured.stdin, /System instructions:\nsystem prompt/);
		assert.match(captured.stdin, /User request:\nuser prompt/);
		assert.equal(captured.suppress, "1");
	} finally {
		if (previousCommand === undefined) delete process.env.FLUX_CODEX_COMMAND;
		else process.env.FLUX_CODEX_COMMAND = previousCommand;
		if (previousCapture === undefined) delete process.env.FLUX_CAPTURE_PATH;
		else process.env.FLUX_CAPTURE_PATH = previousCapture;
		fake.cleanup();
	}
});

test("createHostCliModelCaller invokes Claude print mode with tools disabled", async () => {
	const fake = withFakeCommand(`
import { readFileSync, writeFileSync } from "node:fs";
const stdin = readFileSync(0, "utf8");
writeFileSync(process.env.FLUX_CAPTURE_PATH, JSON.stringify({ args: process.argv.slice(2), stdin, suppress: process.env.FLUX_SUPPRESS }));
process.stdout.write("claude sidecar note\\n");
`);
	const previousCommand = process.env.FLUX_CLAUDE_COMMAND;
	const previousCapture = process.env.FLUX_CAPTURE_PATH;
	process.env.FLUX_CLAUDE_COMMAND = fake.command;
	process.env.FLUX_CAPTURE_PATH = fake.capturePath;
	try {
		const caller = createHostCliModelCaller("claude-code", fake.dir);
		assert.ok(caller);
		const result = await caller({ systemPrompt: "system prompt", prompt: "user prompt" } as any);
		const captured = JSON.parse(readFileSync(fake.capturePath, "utf8")) as { args: string[]; stdin: string; suppress: string };

		assert.equal(result.content, "claude sidecar note");
		assert.equal(result.model, "claude-code/claude-cli");
		assert.deepEqual(captured.args, ["-p", "--no-session-persistence", "--tools", "", "--system-prompt", "system prompt"]);
		assert.equal(captured.stdin, "user prompt");
		assert.equal(captured.suppress, "1");
	} finally {
		if (previousCommand === undefined) delete process.env.FLUX_CLAUDE_COMMAND;
		else process.env.FLUX_CLAUDE_COMMAND = previousCommand;
		if (previousCapture === undefined) delete process.env.FLUX_CAPTURE_PATH;
		else process.env.FLUX_CAPTURE_PATH = previousCapture;
		fake.cleanup();
	}
});

test("createHostCliModelCaller passes configured Codex model and reasoning effort", async () => {
	const fake = withFakeCommand(`
import { readFileSync, writeFileSync } from "node:fs";
readFileSync(0, "utf8");
writeFileSync(process.env.FLUX_CAPTURE_PATH, JSON.stringify({ args: process.argv.slice(2) }));
const outputIndex = process.argv.indexOf("--output-last-message");
if (outputIndex !== -1) writeFileSync(process.argv[outputIndex + 1], "configured codex note");
`);
	const previousCommand = process.env.FLUX_CODEX_COMMAND;
	const previousCapture = process.env.FLUX_CAPTURE_PATH;
	process.env.FLUX_CODEX_COMMAND = fake.command;
	process.env.FLUX_CAPTURE_PATH = fake.capturePath;
	try {
		const caller = createHostCliModelCaller("codex", fake.dir);
		assert.ok(caller);
		const result = await caller({ systemPrompt: "system", prompt: "prompt", config: configWithHost("codex", { model: "gpt-5.5", thinkingEffort: "high" }) } as any);
		const captured = JSON.parse(readFileSync(fake.capturePath, "utf8")) as { args: string[] };
		assert.equal(result.content, "configured codex note");
		assert.equal(result.model, "codex/codex-cli/gpt-5.5/high");
		assert.deepEqual(captured.args.slice(0, 6), ["--ask-for-approval", "never", "exec", "-m", "gpt-5.5", "-c"]);
		assert.equal(captured.args[6], 'model_reasoning_effort="high"');
	} finally {
		if (previousCommand === undefined) delete process.env.FLUX_CODEX_COMMAND;
		else process.env.FLUX_CODEX_COMMAND = previousCommand;
		if (previousCapture === undefined) delete process.env.FLUX_CAPTURE_PATH;
		else process.env.FLUX_CAPTURE_PATH = previousCapture;
		fake.cleanup();
	}
});

test("createHostCliModelCaller passes configured Claude model without unvalidated thinking flags", async () => {
	const fake = withFakeCommand(`
import { readFileSync, writeFileSync } from "node:fs";
readFileSync(0, "utf8");
writeFileSync(process.env.FLUX_CAPTURE_PATH, JSON.stringify({ args: process.argv.slice(2) }));
process.stdout.write("configured claude note\\n");
`);
	const previousCommand = process.env.FLUX_CLAUDE_COMMAND;
	const previousCapture = process.env.FLUX_CAPTURE_PATH;
	process.env.FLUX_CLAUDE_COMMAND = fake.command;
	process.env.FLUX_CAPTURE_PATH = fake.capturePath;
	try {
		const caller = createHostCliModelCaller("claude-code", fake.dir);
		assert.ok(caller);
		const result = await caller({ systemPrompt: "system prompt", prompt: "user prompt", config: configWithHost("claude-code", { model: "opus", thinkingEffort: "high" }) } as any);
		const captured = JSON.parse(readFileSync(fake.capturePath, "utf8")) as { args: string[] };
		assert.equal(result.content, "configured claude note");
		assert.equal(result.model, "claude-code/claude-cli/opus");
		assert.deepEqual(captured.args, ["-p", "--no-session-persistence", "--tools", "", "--model", "opus", "--system-prompt", "system prompt"]);
	} finally {
		if (previousCommand === undefined) delete process.env.FLUX_CLAUDE_COMMAND;
		else process.env.FLUX_CLAUDE_COMMAND = previousCommand;
		if (previousCapture === undefined) delete process.env.FLUX_CAPTURE_PATH;
		else process.env.FLUX_CAPTURE_PATH = previousCapture;
		fake.cleanup();
	}
});
