export type HostKind = "pi" | "claude-code" | "codex" | "generic";

export type TriggerKind =
	| "manual"
	| "external"
	| "random"
	| "turn-start"
	| "turn-end"
	| "tool-call"
	| "tool-result"
	| "loop-detected";

export type DeliveryMode = "steer" | "followUp" | "nextTurn" | "stdout" | "file";

export interface FluxModelSpec {
	/** Arbitrary label for status and telemetry. */
	name: string;
	/** Provider kind understood by Flux's minimal client. */
	provider: "openai-compatible" | "anthropic";
	/** Model identifier passed to the provider. */
	model: string;
	/** Base URL for OpenAI-compatible providers. Defaults to OpenAI Responses-compatible chat completions. */
	baseUrl?: string;
	/** Env var containing the API key. */
	apiKeyEnv?: string;
	/** Literal API key. Prefer apiKeyEnv for checked-in config. */
	apiKey?: string;
	/** Extra headers sent to the sidecar model provider. */
	headers?: Record<string, string>;
	/** Sampling temperature for thought generation. */
	temperature?: number;
	/** Max generated tokens. */
	maxTokens?: number;
	/** Optional provider-specific thinking/reasoning effort. */
	thinkingEffort?: "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | string;
}

export interface TriggerConfig {
	name: string;
	kind: TriggerKind;
	enabled?: boolean;
	/** Probability in [0, 1] for random/probabilistic triggers. */
	probability?: number;
	/** Minimum milliseconds between firings for this trigger. */
	minIntervalMs?: number;
	/** Only fire after this many observed events. */
	afterEvents?: number;
	/** Tool names this trigger applies to, if relevant. */
	tools?: string[];
	/** Regex patterns used by heuristic triggers. */
	patterns?: string[];
}

export interface FluxConfig {
	enabled: boolean;
	randomInjections: boolean;
	delivery: DeliveryMode;
	displayThoughts: boolean;
	triggerTurn: boolean;
	context: {
		maxUserMessages: number;
		maxAssistantMessages: number;
		maxToolEvents: number;
		maxChars: number;
	};
	models: FluxModelSpec[];
	triggers: TriggerConfig[];
	prompt: {
		system: string;
		style: string;
	};
	storage: {
		thoughtLog?: string;
	};
}

export interface AgentToolEvent {
	name: string;
	input?: unknown;
	result?: unknown;
	isError?: boolean;
	timestamp?: number;
}

export interface AgentMessage {
	role: "system" | "user" | "assistant" | "tool" | "custom";
	text: string;
	timestamp?: number;
	toolCalls?: AgentToolEvent[];
}

export interface AgentContextSnapshot {
	host: HostKind;
	cwd?: string;
	sessionPrompt?: string;
	systemPrompt?: string;
	lastUserMessages: AgentMessage[];
	lastAssistantMessages: AgentMessage[];
	toolEvents: AgentToolEvent[];
	metadata?: Record<string, unknown>;
}

export interface TriggerEvent {
	host: HostKind;
	kind: TriggerKind;
	name?: string;
	timestamp: number;
	payload?: unknown;
}

export interface StrayThought {
	id: string;
	createdAt: string;
	model: string;
	trigger: TriggerEvent;
	content: string;
	contextDigest: string;
}

export interface FluxState {
	observedEvents: number;
	lastTriggerAt: Record<string, number>;
	lastThought?: StrayThought;
}
