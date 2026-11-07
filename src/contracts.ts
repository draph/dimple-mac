import type { AppError } from "./i18n";
export type TargetSummary = { id: string; path: string; actualPath: string; linked: boolean; kind: string; error: AppError | null };
export type Entry = { revision: number; editRevision: number; pinned: boolean; source: string; items: TargetSummary[]; issues: TargetSummary[]; error: AppError | null; choosing: boolean; reading: boolean; checking: boolean; preparing: boolean };
export type PrepareOperation = { revision: number; targetIds: string[]; action: { type: "restore" } | { type: "apply"; templateId: "big-sur-light-blue"; badgeId: "code"; params: {scale: number; offsetX: number; offsetY: number}; images: {size: number; png: string}[] } };
export type PreparedOperation = { operationId: string; targetIds: string[]; linkedTargetIds: string[]; overwriteTargetIds: string[]; restore: boolean; confirmationToken: string | null; commitAvailable: boolean };

export type OperationProgress = { operationId: string; sequence: number; phase: "checking" | "running" | "uncertain" | "completed"; slow: boolean; stopped: boolean; items: { targetId: string; status: "pending" | "running" | "succeeded" | "failed" | "unknown"; error: AppError | null }[] };
export type OperationState = { revision: number; prepared: PreparedOperation | null; progress: OperationProgress | null };
