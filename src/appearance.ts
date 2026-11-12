export type Appearance = "system" | "dark" | "light";

export function createAppearance(
  storage: () => Pick<Storage, "getItem" | "setItem">,
  media: Pick<MediaQueryList, "matches"> & Pick<EventTarget, "addEventListener">,
  root: Pick<HTMLElement, "dataset">,
) {
  let preference: Appearance = "system";
  try {
    const saved = storage().getItem("appearance");
    if (saved === "system" || saved === "dark" || saved === "light") preference = saved;
  } catch { /* Storage may be unavailable; use the system appearance. */ }
  let snapshot = { preference, saveFailed: false };
  const listeners = new Set<() => void>();
  const apply = () => {
    root.dataset.theme = snapshot.preference === "system" ? (media.matches ? "dark" : "light") : snapshot.preference;
  };
  apply();
  media.addEventListener("change", apply);
  return {
    getSnapshot: () => snapshot,
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    select(preference: Appearance) {
      let saveFailed = false;
      try { storage().setItem("appearance", preference); }
      catch { saveFailed = true; }
      snapshot = { preference, saveFailed };
      apply();
      listeners.forEach(listener => listener());
    },
  };
}

export type AppearanceController = ReturnType<typeof createAppearance>;
