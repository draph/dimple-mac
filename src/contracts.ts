export type RenderParams = { scale: number; offsetX: number; offsetY: number; theme: string };
import type { AppError } from "./i18n";
export type TargetSummary = { id: string; path: string; actualPath: string; linked: boolean; kind: string; error: AppError | null };
export type Badge = { id: string; width: number; height: number; opaque: boolean; text: string | null };
export type Entry = { params: RenderParams; badge: Badge; badgeError: AppError | null; importing: boolean; text: string | null; textCount: number | null; textPending: boolean; revision: number; editRevision: number; pinned: boolean; source: string; items: TargetSummary[]; issues: TargetSummary[]; error: AppError | null; choosing: boolean; reading: boolean; checking: boolean; preparing: boolean };
export type PrepareOperation = { revision: number; targetIds: string[]; action: { type: "restore" } | { type: "apply"; templateId: "big-sur-light-blue"; badgeId: string; params: RenderParams; images: {size: number; png: string}[] } };
export type PreparedOperation = { operationId: string; targetIds: string[]; linkedTargetIds: string[]; overwriteTargetIds: string[]; restore: boolean; confirmationToken: string | null; commitAvailable: boolean };

export type OperationProgress = { operationId: string; sequence: number; phase: "checking" | "running" | "uncertain" | "completed"; slow: boolean; stopped: boolean; items: { targetId: string; status: "pending" | "running" | "succeeded" | "failed" | "unknown"; error: AppError | null }[] };
export type OperationState = { canStop: boolean; canRetry: boolean; canContinue: boolean; revision: number; prepared: PreparedOperation | null; progress: OperationProgress | null };
