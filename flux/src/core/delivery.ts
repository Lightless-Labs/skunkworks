import type { DeliveryMode } from "./types.ts";

export const AGENT_DELIVERY_MODES = ["steer", "followUp", "nextTurn"] as const satisfies readonly DeliveryMode[];

export function isDeliveryMode(value: unknown): value is DeliveryMode {
	return typeof value === "string" && AGENT_DELIVERY_MODES.includes(value as DeliveryMode);
}

export function supportedDeliveryModes(): string {
	return AGENT_DELIVERY_MODES.join(" | ");
}

export function piDeliverAs(value: unknown): DeliveryMode | undefined {
	return isDeliveryMode(value) ? value : undefined;
}
