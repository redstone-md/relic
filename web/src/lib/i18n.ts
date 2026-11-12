//! Every word the panel says, in each language it speaks.
//!
//! The code is written in English and the English text is the key: `t("Save
//! settings")`. A language is one folder of `src/locales`, whose dictionaries
//! map those keys to its own words; whatever a dictionary leaves out is shown
//! in English. Blanks are named in braces: `t("Signed in as {name}", { name })`.
//!
//! One English text sometimes needs two translations ("Open" the verb, "Open"
//! the state of an invite). Such a key carries a context after ` ## `:
//! `t("Open ## invite state")`. The dictionary key is the whole string, and
//! when there is no translation only the part before the separator is shown.
//! The same holds for both forms of `plural`. Use it only where a plain key
//! would collide.

import { useSyncExternalStore } from "react";

import { LOCALES } from "@/locales";

/**
 * What a language says for one English text: a sentence, or the forms of a
 * counted one in the order of `Intl.PluralRules` categories that language
 * uses, as its `PLURALS` lists them.
 */
export type Entry = string | readonly string[];
export type Dict = Record<string, Entry>;

export interface Locale {
  /** BCP 47 code, also handed to `Intl`. */
  code: string;
  /** What the language calls itself. */
  name: string;
  /** Plural categories in the order counted entries list their forms. */
  plurals: readonly Intl.LDMLPluralRule[];
  dict: Dict;
}

type Params = Record<string, string | number>;

const KEY = "relic.language";
const FALLBACK = "en";

function initial(): string {
  try {
    const stored = localStorage.getItem(KEY);
    if (stored && LOCALES.some((locale) => locale.code === stored)) return stored;
  } catch {
    // Storage may be blocked; fall through to what the browser asks for.
  }
  for (const wanted of navigator.languages ?? [navigator.language]) {
    const base = wanted.toLowerCase().split("-")[0];
    const match = LOCALES.find((locale) => locale.code === base);
    if (match) return match.code;
  }
  return FALLBACK;
}

let current = LOCALES.find((locale) => locale.code === initial()) ?? LOCALES[0];
let rules = new Intl.PluralRules(current.code);
const listeners = new Set<() => void>();
document.documentElement.lang = current.code;

/** Splits an English text from the context that tells its meanings apart. */
const CONTEXT = " ## ";

function bare(source: string): string {
  const at = source.indexOf(CONTEXT);
  return at < 0 ? source : source.slice(0, at);
}

function fill(text: string, params?: Params): string {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole,
  );
}

/** The current language's words for an English text. */
export function t(source: string, params?: Params): string {
  const entry = current.dict[source];
  return fill(typeof entry === "string" ? entry : bare(source), params);
}

/**
 * A counted text: `plural(n, "{count} file", "{count} files")`. The plural
 * English form is the key, and `{count}` is always filled with the number.
 */
export function plural(count: number, one: string, other: string, params?: Params): string {
  const filled = { count, ...params };
  const entry = current.dict[other];
  if (Array.isArray(entry)) {
    const index = current.plurals.indexOf(rules.select(count));
    return fill(entry[index] ?? entry[entry.length - 1], filled);
  }
  if (typeof entry === "string") return fill(entry, filled);
  return fill(bare(count === 1 ? one : other), filled);
}

/** The code of the language in use, for `Intl` formatters. */
export function language(): string {
  return current.code;
}

export function setLanguage(code: string) {
  const next = LOCALES.find((locale) => locale.code === code);
  if (!next || next === current) return;
  current = next;
  rules = new Intl.PluralRules(next.code);
  document.documentElement.lang = next.code;
  try {
    localStorage.setItem(KEY, next.code);
  } catch {
    // Not remembered, still applied.
  }
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * Makes a component draw again when the language changes. Call it in every
 * component that shows text and use what it returns.
 */
export function useI18n() {
  const code = useSyncExternalStore(subscribe, language);
  return { t, plural, language: code, setLanguage, locales: LOCALES };
}
