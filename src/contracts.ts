import type { AppError } from "./i18n";
export type TargetSummary = { id: string; path: string; actualPath: string; linked: boolean; kind: string; error: AppError | null };
export type Entry = { revision: number; source: string; items: TargetSummary[]; issues: TargetSummary[]; error: AppError | null; choosing: boolean; reading: boolean; checking: boolean; preparing: boolean };
export type PrepareOperation = { revision: number; targetIds: string[]; action: { type: "restore" } | { type: "apply"; templateId: "big-sur-light-blue"; badgeId: "code"; params: {scale: number; offsetX: number; offsetY: number}; images: {size: number; png: string}[] } };
export type PreparedOperation = { operationId: string; targetIds: string[]; linkedTargetIds: string[]; commitAvailable: false };
