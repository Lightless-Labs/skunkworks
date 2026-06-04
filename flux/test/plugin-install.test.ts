import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

function json(path: string): any {
	return JSON.parse(readFileSync(path, "utf8"));
}

test("Claude and Codex marketplace manifests expose Flux from the skunkworks repo root", () => {
	const claudeMarketplace = json("../.claude-plugin/marketplace.json");
	assert.equal(claudeMarketplace.name, "lightless-labs-skunkworks");
	assert.equal(claudeMarketplace.plugins[0].name, "flux");
	assert.equal(claudeMarketplace.plugins[0].source, "./flux");

	const codexMarketplace = json("../.agents/plugins/marketplace.json");
	assert.equal(codexMarketplace.name, "lightless-labs-skunkworks");
	assert.equal(codexMarketplace.plugins[0].name, "flux");
	assert.deepEqual(codexMarketplace.plugins[0].source, { source: "local", path: "./flux" });
});

test("host plugin hooks use the repo-level safe-fail wrapper", () => {
	const claudePlugin = json(".claude-plugin/plugin.json");
	assert.equal(claudePlugin.hooks, "./hooks/claude-code-hooks.json");
	const claudeHooks = JSON.stringify(json("hooks/claude-code-hooks.json"));
	assert.match(claudeHooks, /\$\{CLAUDE_PLUGIN_ROOT\}\/scripts\/flux-hook-wrapper\.mjs/);
	assert.match(claudeHooks, /--host=claude-code/);

	const codexPlugin = json(".codex-plugin/plugin.json");
	assert.equal(codexPlugin.hooks, "./hooks/codex-hooks.json");
	const codexHooks = JSON.stringify(json("hooks/codex-hooks.json"));
	assert.match(codexHooks, /\$\{PLUGIN_ROOT\}\/scripts\/flux-hook-wrapper\.mjs/);
	assert.match(codexHooks, /--host=codex/);
});

test("wrapper exits zero and emits host-safe JSON when setup fails", () => {
	const tempRoot = mkdtempSync(join(tmpdir(), "flux-wrapper-empty-"));
	try {
		writeFileSync(join(tempRoot, "package.json"), "{\"type\":\"module\"}\n", "utf8");
		const result = spawnSync(process.execPath, ["scripts/flux-hook-wrapper.mjs", "--host=generic"], {
			cwd: process.cwd(),
			input: "{}",
			encoding: "utf8",
			env: { ...process.env, FLUX_ROOT: tempRoot },
		});
		assert.equal(result.status, 0);
		const output = JSON.parse(result.stdout.trim());
		assert.equal(output.continue, true);
		assert.equal(typeof output.flux.error, "string");
	} finally {
		rmSync(tempRoot, { recursive: true, force: true });
	}
});
