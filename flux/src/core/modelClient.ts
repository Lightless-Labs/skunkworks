import type { FluxConfig, FluxModelSpec, TriggerEvent } from "./types.ts";

export class FluxModelError extends Error {}

function isUsable(model: FluxModelSpec): boolean {
	return Boolean(model.apiKey || (model.apiKeyEnv && process.env[model.apiKeyEnv]));
}

export function resolveModelPool(config: FluxConfig, trigger?: TriggerEvent): FluxModelSpec[] {
	const byName = new Map(config.models.map((model) => [model.name, model]));
	const poolNames = trigger
		? (trigger.name && config.modelPools[trigger.name]) || config.modelPools[trigger.kind] || config.modelPools.default
		: config.modelPools.default;
	const pooled = (poolNames ?? []).map((name) => byName.get(name)).filter((model): model is FluxModelSpec => Boolean(model));
	const usablePooled = pooled.filter(isUsable);
	if (usablePooled.length > 0) return usablePooled;
	return config.models.filter(isUsable);
}

export function pickModel(config: FluxConfig, trigger?: TriggerEvent, random = Math.random): FluxModelSpec {
	const usable = resolveModelPool(config, trigger);
	if (usable.length === 0) {
		throw new FluxModelError("No usable Flux sidecar models. Configure .flux/config.json models with apiKeyEnv/apiKey.");
	}
	return usable[Math.floor(random() * usable.length)]!;
}

function apiKey(model: FluxModelSpec): string {
	const key = model.apiKey ?? (model.apiKeyEnv ? process.env[model.apiKeyEnv] : undefined);
	if (!key) throw new FluxModelError(`Missing API key for Flux model ${model.name}`);
	return key;
}

async function parseError(response: Response): Promise<string> {
	const text = await response.text().catch(() => "");
	return `${response.status} ${response.statusText}${text ? `: ${text.slice(0, 1000)}` : ""}`;
}

export async function callSidecarModel(
	model: FluxModelSpec,
	systemPrompt: string,
	userPrompt: string,
	signal?: AbortSignal,
): Promise<string> {
	if (model.provider === "anthropic") return callAnthropic(model, systemPrompt, userPrompt, signal);
	return callOpenAICompatible(model, systemPrompt, userPrompt, signal);
}

async function callOpenAICompatible(
	model: FluxModelSpec,
	systemPrompt: string,
	userPrompt: string,
	signal?: AbortSignal,
): Promise<string> {
	const baseUrl = (model.baseUrl ?? "https://api.openai.com/v1").replace(/\/$/, "");
	const response = await fetch(`${baseUrl}/chat/completions`, {
		method: "POST",
		signal,
		headers: {
			"content-type": "application/json",
			authorization: `Bearer ${apiKey(model)}`,
			...(model.headers ?? {}),
		},
		body: JSON.stringify({
			model: model.model,
			messages: [
				{ role: "system", content: systemPrompt },
				{ role: "user", content: userPrompt },
			],
			temperature: model.temperature ?? 0.8,
			max_tokens: model.maxTokens ?? 420,
		}),
	});
	if (!response.ok) throw new FluxModelError(await parseError(response));
	const json = (await response.json()) as any;
	const content = json?.choices?.[0]?.message?.content;
	if (typeof content !== "string") throw new FluxModelError("OpenAI-compatible response did not contain choices[0].message.content");
	return content.trim();
}

async function callAnthropic(
	model: FluxModelSpec,
	systemPrompt: string,
	userPrompt: string,
	signal?: AbortSignal,
): Promise<string> {
	const baseUrl = (model.baseUrl ?? "https://api.anthropic.com/v1").replace(/\/$/, "");
	const response = await fetch(`${baseUrl}/messages`, {
		method: "POST",
		signal,
		headers: {
			"content-type": "application/json",
			"x-api-key": apiKey(model),
			"anthropic-version": "2023-06-01",
			...(model.headers ?? {}),
		},
		body: JSON.stringify({
			model: model.model,
			system: systemPrompt,
			messages: [{ role: "user", content: userPrompt }],
			temperature: model.temperature ?? 0.8,
			max_tokens: model.maxTokens ?? 420,
		}),
	});
	if (!response.ok) throw new FluxModelError(await parseError(response));
	const json = (await response.json()) as any;
	const text = json?.content?.find?.((part: any) => part?.type === "text")?.text;
	if (typeof text !== "string") throw new FluxModelError("Anthropic response did not contain a text content block");
	return text.trim();
}
