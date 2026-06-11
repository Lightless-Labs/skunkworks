import assert from "node:assert/strict";
import test from "node:test";
import { callSidecarModel, FluxModelError } from "../src/core/modelClient.ts";
import type { FluxModelSpec } from "../src/core/types.ts";

function mockFetch(handler: (url: string, init: RequestInit) => Response | Promise<Response>) {
	const original = globalThis.fetch;
	globalThis.fetch = ((input: string | URL | Request, init?: RequestInit) => handler(String(input), init ?? {})) as typeof fetch;
	return () => {
		globalThis.fetch = original;
	};
}

test("callSidecarModel sends OpenAI-compatible chat completion requests", async () => {
	let capturedUrl = "";
	let capturedInit: RequestInit | undefined;
	const restore = mockFetch((url, init) => {
		capturedUrl = url;
		capturedInit = init;
		return Response.json({ choices: [{ message: { content: "  openai note  " } }] });
	});
	try {
		const model: FluxModelSpec = {
			name: "openai-test",
			provider: "openai-compatible",
			model: "gpt-test",
			baseUrl: "https://example.test/v1/",
			apiKey: "secret",
			temperature: 0.3,
			maxTokens: 77,
		};

		const content = await callSidecarModel(model, "system", "user");

		assert.equal(content, "openai note");
		assert.equal(capturedUrl, "https://example.test/v1/chat/completions");
		assert.equal(capturedInit?.method, "POST");
		assert.equal((capturedInit?.headers as Record<string, string>).authorization, "Bearer secret");
		const body = JSON.parse(String(capturedInit?.body)) as any;
		assert.equal(body.model, "gpt-test");
		assert.equal(body.temperature, 0.3);
		assert.equal(body.max_tokens, 77);
		assert.deepEqual(body.messages, [
			{ role: "system", content: "system" },
			{ role: "user", content: "user" },
		]);
	} finally {
		restore();
	}
});

test("callSidecarModel sends Anthropic messages requests", async () => {
	let capturedUrl = "";
	let capturedInit: RequestInit | undefined;
	const restore = mockFetch((url, init) => {
		capturedUrl = url;
		capturedInit = init;
		return Response.json({ content: [{ type: "text", text: "  anthropic note  " }] });
	});
	try {
		const model: FluxModelSpec = {
			name: "anthropic-test",
			provider: "anthropic",
			model: "claude-test",
			baseUrl: "https://anthropic.test/v1/",
			apiKey: "secret",
			temperature: 0.4,
			maxTokens: 88,
		};

		const content = await callSidecarModel(model, "system", "user");

		assert.equal(content, "anthropic note");
		assert.equal(capturedUrl, "https://anthropic.test/v1/messages");
		assert.equal(capturedInit?.method, "POST");
		assert.equal((capturedInit?.headers as Record<string, string>)["x-api-key"], "secret");
		assert.equal((capturedInit?.headers as Record<string, string>)["anthropic-version"], "2023-06-01");
		const body = JSON.parse(String(capturedInit?.body)) as any;
		assert.equal(body.model, "claude-test");
		assert.equal(body.system, "system");
		assert.equal(body.temperature, 0.4);
		assert.equal(body.max_tokens, 88);
		assert.deepEqual(body.messages, [{ role: "user", content: "user" }]);
	} finally {
		restore();
	}
});

test("callSidecarModel sends direct-provider thinking effort when configured", async () => {
	let openAiBody: any;
	let anthropicBody: any;
	const restore = mockFetch((url, init) => {
		const body = JSON.parse(String(init.body));
		if (url.includes("anthropic")) {
			anthropicBody = body;
			return Response.json({ content: [{ type: "text", text: "anthropic" }] });
		}
		openAiBody = body;
		return Response.json({ choices: [{ message: { content: "openai" } }] });
	});
	try {
		await callSidecarModel({ name: "oa", provider: "openai-compatible", model: "gpt", apiKey: "secret", thinkingEffort: "high" }, "system", "user");
		await callSidecarModel({ name: "an", provider: "anthropic", model: "claude", baseUrl: "https://anthropic.test/v1", apiKey: "secret", thinkingEffort: "low" }, "system", "user");
		assert.equal(openAiBody.reasoning_effort, "high");
		assert.deepEqual(anthropicBody.thinking, { type: "enabled", budget_tokens: 2048 });
		assert.equal(anthropicBody.max_tokens, 2468);
		assert.equal(anthropicBody.temperature, undefined);
	} finally {
		restore();
	}
});

test("callSidecarModel turns provider failures into FluxModelError", async () => {
	const restore = mockFetch(() => new Response("bad request details", { status: 400, statusText: "Bad Request" }));
	try {
		const model: FluxModelSpec = { name: "bad", provider: "anthropic", model: "claude-test", apiKey: "secret" };
		await assert.rejects(() => callSidecarModel(model, "system", "user"), (error) => {
			assert.ok(error instanceof FluxModelError);
			assert.match(String(error), /400 Bad Request: bad request details/);
			return true;
		});
	} finally {
		restore();
	}
});
