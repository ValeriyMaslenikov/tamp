// i18n runtime — dependency-free and synchronous (the JSON dictionaries are
// statically imported and bundled). English is the source of truth and the
// runtime fallback; uk.json mirrors its keys. See docs/i18n.md.

import en from "./en.json";
import uk from "./uk.json";
import dayjs from "../lib/dayjs";

export type Locale = "en" | "uk";

// A dictionary node is either a leaf string, a plural object (CLDR categories
// keyed by Intl.PluralRules), or a nested group of more nodes.
type PluralObject = { other: string } & Partial<
  Record<Intl.LDMLPluralRule, string>
>;
type DictNode = string | PluralObject | { [key: string]: DictNode };

const DICTS: Record<Locale, DictNode> = { en, uk };

let activeLocale: Locale = "en";
let activeDict: DictNode = DICTS.en;
let pluralRules = new Intl.PluralRules("en");

/**
 * Map a `locale` setting + the browser language to a concrete locale.
 * `"en"`/`"uk"` pass through; `"system"` resolves to `uk` when the navigator
 * language starts with `uk` (e.g. `uk`, `uk-UA`), otherwise `en`.
 */
export function resolveLocale(setting: string, navLang: string): Locale {
  if (setting === "en" || setting === "uk") return setting;
  return navLang.toLowerCase().startsWith("uk") ? "uk" : "en";
}

/**
 * Activate a locale: swap the active dictionary, rebuild the plural rules,
 * point dayjs at the matching locale (so relative times follow the active
 * language), and reflect it on `<html lang>` so the platform picks correct
 * hyphenation/fonts.
 */
export function setLocale(locale: Locale): void {
  activeLocale = locale;
  activeDict = DICTS[locale] ?? DICTS.en;
  pluralRules = new Intl.PluralRules(locale);
  // Keep dayjs's relative-time output ("5 minutes ago" / "5 хвилин тому") in
  // sync with the UI language. "en" is dayjs's default; "uk" is loaded in
  // ../lib/dayjs.
  dayjs.locale(locale);
  // `document` is absent under non-DOM test/SSR contexts; guard so t() stays
  // usable everywhere. manual: in the running webview this sets <html lang>.
  if (typeof document !== "undefined") {
    document.documentElement.lang = locale;
  }
}

/** The currently active locale (defaults to "en" until setLocale is called). */
export function getLocale(): Locale {
  return activeLocale;
}

/** Walk a dotted key (`"a.b.c"`) through a dictionary; null if any hop misses. */
function lookup(dict: DictNode, key: string): DictNode | null {
  let node: DictNode = dict;
  for (const part of key.split(".")) {
    if (typeof node !== "object" || node === null || !(part in node)) {
      return null;
    }
    node = (node as { [k: string]: DictNode })[part];
  }
  return node;
}

function isPluralObject(node: DictNode): node is PluralObject {
  return typeof node === "object" && node !== null && "other" in node;
}

/** Replace `{name}` placeholders with the matching param (count included). */
function interpolate(
  template: string,
  params?: Record<string, string | number>,
): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}

/**
 * Resolve a dotted `key` to a localized string. If the value is a plural object
 * and `params.count` is a number, the CLDR category is chosen via
 * `Intl.PluralRules` (falling back to `other`). `{name}` placeholders are
 * interpolated from `params`. Missing keys fall back to the `en` dictionary,
 * then to the raw key. Never throws.
 */
export function t(
  key: string,
  params?: Record<string, string | number>,
): string {
  let node = lookup(activeDict, key);
  if (node === null && activeDict !== DICTS.en) {
    node = lookup(DICTS.en, key);
  }
  if (node === null) return key;

  if (isPluralObject(node)) {
    const count = params?.count;
    if (typeof count === "number") {
      const category = pluralRules.select(count);
      const form = node[category] ?? node.other;
      return interpolate(form, params);
    }
    // No numeric count: degrade to the `other` form rather than throwing.
    return interpolate(node.other, params);
  }

  if (typeof node === "string") return interpolate(node, params);

  // The key pointed at a nested group, not a string — treat as missing.
  return key;
}
