import type { AppError, Locale } from "./i18n";

export type LanguagePreference = "system" | Locale;
export type LanguageStatus = {
  preference: LanguagePreference;
  locale: Locale;
  revision: number;
  error?: AppError;
};

export function acceptLanguage(previous: LanguageStatus | null, next: LanguageStatus): LanguageStatus {
  return previous && next.revision < previous.revision ? previous : next;
}
