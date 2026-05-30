import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { snapshotFromGenericPayload } from "../src/core/context.ts";
import { DEFAULT_CONFIG } from "../src/core/config.ts";
import { formatHookOutput, inferKind } from "../src/core/hookCli.ts";
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

test("formatHookOutput emits Claude Code context shape", () => {
	const thought = fakeThought("claude-code");
	const output = formatHookOutput("claude-code", "rendered thought", thought, true) as any;

	assert.equal(output.continue, true);
	assert.equal(output.additionalContext, "rendered thought");
	assert.equal(output.hookSpecificOutput.hookEventName, "Flux");
	assert.equal(output.hookSpecificOutput.additionalContext, "rendered thought");
	assert.equal(output.flux, thought);
});

test("formatHookOutput emits Codex instruction shape", () => {
	const thought = fakeThought("codex");
	const output = formatHookOutput("codex", "rendered thought", thought, true) as any;

	assert.equal(output.continue, true);
	assert.equal(output.instructions, "rendered thought");
	assert.equal(output.additionalContext, "rendered thought");
	assert.equal(output.flux, thought);
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
