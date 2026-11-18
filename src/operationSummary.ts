import type { OperationProgress } from "./contracts";
import { translate, type Locale } from "./i18n.ts";

export function operationSucceeded(progress: OperationProgress): boolean {
  return progress.phase === "completed" && progress.items.length > 0
    && progress.items.every(item => item.status === "succeeded");
}

export function operationSummary(locale: Locale, progress: OperationProgress): string {
  const total = progress.items.length;
  if (progress.phase !== "completed") {
    return translate(locale, "operation.progress", {
      completed: progress.items.filter(item => item.status === "succeeded" || item.status === "failed").length,
      total,
    });
  }
  if (operationSucceeded(progress)) {
    return translate(locale, total === 1 ? "operation.success.one" : "operation.success.other", { total });
  }
  return (["succeeded", "failed", "pending", "unknown", "running"] as const).flatMap(status => {
    const count = progress.items.filter(item => item.status === status).length;
    return count > 0 ? [translate(locale, `operation.count.${status}`, { count })] : [];
  }).join(" · ");
}
