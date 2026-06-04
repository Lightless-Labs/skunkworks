#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const wrapperDir = dirname(fileURLToPath(import.meta.url));
const fluxRoot = process.env.FLUX_ROOT || join(wrapperDir, "..");
const node = process.execPath;
const setupTimeoutMs = Number(process.env.FLUX_HOOK_SETUP_TIMEOUT_MS || 120_000);

function parseHost() {
	const hostArg = process.argv.find((arg) => arg.startsWith("--host="));
	if (hostArg) return hostArg.slice("--host=".length);
	if (process.argv.includes("--host")) {
		const index = process.argv.indexOf("--host");
		return process.argv[index + 1] || "generic";
	}
	if (process.env.CLAUDE_PLUGIN_ROOT) return "claude-code";
	if (process.env.PLUGIN_ROOT) return "codex";
	return "generic";
}

function extraArgs() {
	const args = [];
	for (let index = 2; index < process.argv.length; index += 1) {
		const arg = process.argv[index];
		if (arg.startsWith("--host=")) continue;
		if (arg === "--host") {
			index += 1;
			continue;
		}
		args.push(arg);
	}
	return args;
}

function writeSafeFailure(message) {
	try {
		process.stderr.write(`flux-hook-wrapper: ${message}\n`);
	} catch {
		// ignored: hooks must never fail the host agent because logging failed.
	}
	process.stdout.write(`${JSON.stringify({ continue: true, flux: { error: message } })}\n`);
}

function runSetup(command, args) {
	const result = spawnSync(command, args, {
		cwd: fluxRoot,
		encoding: "utf8",
		stdio: ["ignore", "pipe", "pipe"],
		timeout: setupTimeoutMs,
		env: process.env,
	});
	if (result.stdout) process.stderr.write(result.stdout);
	if (result.stderr) process.stderr.write(result.stderr);
	if (result.error) throw result.error;
	if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} exited ${result.status}`);
}

function newestMtime(path) {
	const stat = statSync(path);
	if (!stat.isDirectory()) return stat.mtimeMs;
	let newest = stat.mtimeMs;
	for (const entry of readdirSync(path)) {
		if (entry === "node_modules" || entry === "dist") continue;
		newest = Math.max(newest, newestMtime(join(path, entry)));
	}
	return newest;
}

function needsBuild(hookPath) {
	if (!existsSync(hookPath)) return true;
	try {
		const builtAt = statSync(hookPath).mtimeMs;
		return ["src", "bin", "package.json", "tsconfig.json", "tsconfig.hooks.json"].some((part) => newestMtime(join(fluxRoot, part)) > builtAt);
	} catch {
		return true;
	}
}

function ensureBuilt(hookPath) {
	if (!needsBuild(hookPath)) return;
	const tscPath = join(fluxRoot, "node_modules", "typescript", "bin", "tsc");
	if (!existsSync(tscPath)) runSetup("npm", ["install", "--ignore-scripts", "--no-audit", "--no-fund", "--include=dev", "--omit=peer"]);
	runSetup("npm", ["run", "build:hooks", "--silent"]);
}

const host = parseHost();
const hookPath = join(fluxRoot, "dist", "bin", "flux-hook.js");

try {
	ensureBuilt(hookPath);
	const result = spawnSync(node, [hookPath, `--host=${host}`, ...extraArgs()], {
		cwd: process.cwd(),
		encoding: "utf8",
		stdio: ["inherit", "pipe", "pipe"],
		env: process.env,
	});
	if (result.stdout) process.stdout.write(result.stdout);
	if (result.stderr) process.stderr.write(result.stderr);
	if (result.error) throw result.error;
	if (result.status !== 0) writeSafeFailure(`flux hook exited ${result.status}`);
} catch (error) {
	const message = error instanceof Error ? error.message : String(error);
	writeSafeFailure(message);
} finally {
	process.exit(0);
}
