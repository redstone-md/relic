import type { Dict, Locale } from "@/lib/i18n";

// A language is a folder here: every file in it is one dictionary, and they
// are merged. A new language is a new folder and one line below.
function merged(files: Record<string, { default: Dict }>): Dict {
  return Object.assign({}, ...Object.values(files).map((file) => file.default));
}

export const LOCALES: readonly Locale[] = [
  { code: "en", name: "English", plurals: ["one", "other"], dict: {} },
  {
    code: "ru",
    name: "Русский",
    plurals: ["one", "few", "many", "other"],
    dict: merged(import.meta.glob<{ default: Dict }>("./ru/*.ts", { eager: true })),
  },
];
