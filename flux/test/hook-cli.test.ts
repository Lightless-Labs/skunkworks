import assert from "node:assert/strict";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { snapshotFromGenericPayload } from "../src/core/context.ts";
import { DEFAULT_CONFIG } from "../src/core/config.ts";
import { formatHookOutput, hookEventNameForOutput, inferKind } from "../src/core/hookCli.ts";
import type { StrayThought } from "../src/core/types.ts";

function fixture(name: string): unknown {
	return JSON.parse(readFileSync(join("test", "fixtures", name), "utf8"));
}

test("inferKind recognizes Claude Code hook event names", () => {
	assert.equal(inferKind(fixture("claude-stop.json")), "turn-end");
	assert.equal(inferKind(fixture("claude-post-tool-use.json")), "tool-result");
});

test("inferKind recognizes Codex post-turn and post-tool event names", () => {
	assert.equal(inferKind(fixture("codex-post-turn.json")), "turn-end");
	assert.equal(inferKind(fixture("codex-post-tool.json")), "tool-result");
});

test("host fixture snapshots include recent messages and tool events", () => {
	const claudeTool = snapshotFromGenericPayload("claude-code", fixture("claude-post-tool-use.json"), DEFAULT_CONFIG);
	assert.equal(claudeTool.host, "claude-code");
	assert.equal(claudeTool.cwd, "/workspace/project");
	assert.equal(claudeTool.lastUserMessages.at(-1)?.text, "Run the tests.");
	assert.equal(claudeTool.toolEvents.at(-1)?.name, "bash");
	assert.equal(claudeTool.toolEvents.at(-1)?.isError, true);

	const codexTool = snapshotFromGenericPayload("codex", fixture("codex-post-tool.json"), DEFAULT_CONFIG);
	assert.equal(codexTool.host, "codex");
	assert.equal(codexTool.toolEvents.at(-1)?.name, "read");
});

test("formatHookOutput emits safe no-fire shape", () => {
	assert.deepEqual(formatHookOutput("generic", "", undefined, false), { continue: true, flux: { fired: false } });
});

test("hookEventNameForOutput maps host payload events to documented hook event names", () => {
	assert.equal(hookEventNameForOutput(fixture("claude-stop.json"), "turn-end"), "Stop");
	assert.equal(hookEventNameForOutput(fixture("claude-post-tool-use.json"), "tool-result"), "PostToolUse");
	assert.equal(hookEventNameForOutput(fixture("codex-post-turn.json"), "turn-end"), "Stop");
	assert.equal(hookEventNameForOutput(fixture("codex-post-tool.json"), "tool-result"), "PostToolUse");
});

test("formatHookOutput emits Claude Code context shape", () => {
	const thought = fakeThought("claude-code");
	const output = formatHookOutput("claude-code", "rendered thought", thought, true, "Stop") as any;

	assert.equal(output.continue, true);
	assert.equal(output.hookSpecificOutput.hookEventName, "Stop");
	assert.equal(output.hookSpecificOutput.additionalContext, "rendered thought");
	assert.equal(output.flux, thought);
	assert.equal("additionalContext" in output, false);
});

test("formatHookOutput emits Codex documented hook-specific context shape", () => {
	const thought = fakeThought("codex");
	const output = formatHookOutput("codex", "rendered thought", thought, true, "PostToolUse") as any;

	assert.equal(output.continue, true);
	assert.equal(output.hookSpecificOutput.hookEventName, "PostToolUse");
	assert.equal(output.hookSpecificOutput.additionalContext, "rendered thought");
	assert.equal(output.flux, thought);
	assert.equal("additionalContext" in output, false);
	assert.equal("instructions" in output, false);
});

test("hook CLI fixture smoke emits documented hook-specific context", () => {
	const tmp = mkdtempSync(join(tmpdir(), "flux-hook-cli-smoke-"));
	try {
		mkdirSync(join(tmp, ".flux"));
		writeFileSync(join(tmp, ".flux", "config.json"), JSON.stringify({ storage: {} }), "utf8");
		const fakeClaude = join(tmp, "fake-claude.sh");
		writeFileSync(fakeClaude, '#!/bin/sh\nprintf "smoke note\\n"\n', "utf8");
		chmodSync(fakeClaude, 0o700);
		const fakeCodex = join(tmp, "fake-codex.sh");
		writeFileSync(
			fakeCodex,
			'#!/bin/sh\nprev=""\nfor arg in "$@"; do if [ "$prev" = "--output-last-message" ]; then printf "smoke note" > "$arg"; fi; prev="$arg"; done\n',
			"utf8",
		);
		chmodSync(fakeCodex, 0o700);

		for (const [host, fixtureName, expectedHookEventName, env] of [
			["claude-code", "claude-stop.json", "Stop", { FLUX_CLAUDE_COMMAND: fakeClaude }],
			["codex", "codex-post-turn.json", "Stop", { FLUX_CODEX_COMMAND: fakeCodex }],
		] as const) {
			const payload = fixture(fixtureName) as Record<string, unknown>;
			payload.cwd = tmp;
			const result = spawnSync(process.execPath, [resolve("dist/bin/flux-hook.js"), `--host=${host}`, "--force"], {
				cwd: tmp,
				input: JSON.stringify(payload),
				encoding: "utf8",
				env: { ...process.env, ...env },
			});
			assert.equal(result.status, 0, result.stderr);
			const output = JSON.parse(result.stdout) as any;
			assert.equal(output.continue, true);
			assert.equal(output.hookSpecificOutput.hookEventName, expectedHookEventName);
			assert.match(output.hookSpecificOutput.additionalContext, /smoke note/);
		}
	} finally {
		rmSync(tmp, { recursive: true, force: true });
	}
});

function fakeThought(host: "claude-code" | "codex"): StrayThought {
	return {
		id: "thought-id",
		createdAt: "2026-05-30T00:00:00.000Z",
		model: `${host}/native`,
		promptProfile: "test",
		trigger: { host, kind: "turn-end", name: "test-trigger", timestamp: 1 },
		content: "one concise note",
		contextDigest: "digest",
	};
}
