import manifest from "./assets/icons.json" with { type: "json" };
import en from "./locales/en.json" with { type: "json" };
import zh from "./locales/zh-Hans.json" with { type: "json" };

export const icons = manifest;
export const categories = ["development", "tools", "arrows", "media"];
export function searchIcons(query: string, category = "") {
  const needle = query.trim().toLowerCase();
  return icons.filter(icon => (!category || icon.category === category) &&
    [icon.id, ...icon.keywords, en[icon.name as keyof typeof en], zh[icon.name as keyof typeof zh],
      en[`category.${icon.category}` as keyof typeof en], zh[`category.${icon.category}` as keyof typeof zh]]
      .some(value => value.toLowerCase().includes(needle)));
}
