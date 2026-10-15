// JHPATCH: Translate @ianvs/prettier-plugin-sort-imports options to `sortImports` for `--migrate prettier`

import { builtinModules } from "node:module";
import { isDeepStrictEqual } from "node:util";

export const IANVS_PLUGIN = "@ianvs/prettier-plugin-sort-imports";

export const IANVS_OPTIONS = new Set([
  "importOrder",
  "importOrderParserPlugins",
  "importOrderTypeScriptVersion",
  "importOrderCaseSensitive",
  "importOrderSafeSideEffects",
]);

const BUILTIN_MODULES = "<BUILTIN_MODULES>";
const THIRD_PARTY_MODULES = "<THIRD_PARTY_MODULES>";
const TYPES = "<TYPES>";
// https://github.com/IanVS/prettier-plugin-sort-imports/blob/main/src/constants.ts
const DEFAULT_IMPORT_ORDER = [BUILTIN_MODULES, THIRD_PARTY_MODULES, "^[.]"];
const BUILTIN_MODULE_NAMES = [
  ...builtinModules.flatMap((m) => (m.startsWith("node:") ? [m] : [`node:${m}`, m])),
  "node:test",
];
const BUILTIN_MODULES_REGEX_STR = `^(?:${BUILTIN_MODULE_NAMES.join("|")})$`;
// Oxfmt's `builtin` selector also matches `bun:*`, which the plugin sorts as a third party module.
const BUN_GROUP = "bun";

type SortGroupItem = string | string[] | { newlinesBetween: boolean };

interface CustomGroup {
  groupName: string;
  elementNamePattern?: string[];
  modifiers?: string[];
}

/**
 * Build the `sortImports` value that reproduces `@ianvs/prettier-plugin-sort-imports`
 * with the given plugin options.
 * https://github.com/IanVS/prettier-plugin-sort-imports#options
 *
 * @param options - The plugin's options (`importOrder*`), unset ones take the plugin defaults
 * @param warnings - Receives a message for each setting that cannot be reproduced, unless it is already there
 * @param prefix - Prepended to warnings, e.g. `"overrides: "`
 */
export function migrateIanvsSortImports(
  options: Record<string, unknown>,
  warnings: string[],
  prefix: string,
): Record<string, unknown> | false {
  // Overrides inherit the top-level options, so their warnings may repeat.
  const warn = (message: string) => {
    if (!warnings.includes(message) && !warnings.includes(prefix + message)) {
      warnings.push(prefix + message);
    }
  };

  const rawImportOrder = options.importOrder ?? DEFAULT_IMPORT_ORDER;
  if (!Array.isArray(rawImportOrder) || rawImportOrder.some((p) => typeof p !== "string")) {
    warn(`"importOrder" must be an array of strings, skipping...`);
    return false;
  }
  if (rawImportOrder.length === 0) return false;

  const importOrder = normalizeImportOrder(rawImportOrder);
  const hasTypes = importOrder.some((p) => p.includes(TYPES));
  const isSeparator = (p: string) => p.trim() === "";

  if (isSeparator(importOrder[0])) {
    warn(
      `"importOrder": a leading "" (blank line after top of file comments) is not supported, skipping...`,
    );
  }

  const customGroups: CustomGroup[] = [];
  // Matched after every other custom group, like the plugin's `<TYPES>` fallback.
  const fallbackGroups: CustomGroup[] = [];
  const groups: SortGroupItem[] = [];
  const seen = new Set<string>();
  let pendingNewline = false;
  let builtinSeen = false;

  for (const pattern of importOrder) {
    if (isSeparator(pattern)) {
      pendingNewline = groups.length > 0;
      continue;
    }
    if (seen.has(pattern)) continue;
    seen.add(pattern);

    let group: string | string[];
    if (pattern === BUILTIN_MODULES_REGEX_STR) {
      group = hasTypes ? "value-builtin" : "builtin";
      builtinSeen = true;
    } else if (pattern === THIRD_PARTY_MODULES) {
      group = ["unknown", BUN_GROUP];
      fallbackGroups.unshift({
        groupName: BUN_GROUP,
        elementNamePattern: ["bun:*"],
        ...(hasTypes && { modifiers: ["value"] }),
      });
    } else if (pattern === TYPES) {
      group = TYPES;
      fallbackGroups.push({ groupName: TYPES, modifiers: ["type"] });
    } else {
      const isType = pattern.includes(TYPES);
      // The plugin strips the first `<TYPES>` and matches the rest as a regex.
      const regexSource = pattern.replace(TYPES, "");
      if ([BUILTIN_MODULES, THIRD_PARTY_MODULES, TYPES].some((w) => regexSource.includes(w))) {
        warn(
          `"importOrder": "${pattern}" matches no imports (a special word after "<TYPES>" is a literal regex), skipping...`,
        );
        continue;
      }
      const globs = regexToGlobs(regexSource);
      if (!globs) {
        warn(`"importOrder": regex "${pattern}" cannot be converted to glob patterns, skipping...`);
        continue;
      }
      const regex = new RegExp(regexSource);
      if (builtinSeen && !isType && BUILTIN_MODULE_NAMES.some((m) => regex.test(m))) {
        warn(
          `"importOrder": Node.js builtin modules that match "${pattern}" are sorted in its group, not in "<BUILTIN_MODULES>"`,
        );
      }
      // Names made of selector characters could collide with predefined group names.
      group = /^[a-z_-]+$/.test(pattern) ? `importOrder:${pattern}` : pattern;
      customGroups.push({
        groupName: group,
        elementNamePattern: globs,
        ...(hasTypes && { modifiers: [isType ? "type" : "value"] }),
      });
    }

    if (pendingNewline) groups.push({ newlinesBetween: true });
    pendingNewline = false;
    groups.push(group);
  }

  const combine = !hasTypes && canCombineTypeAndValueImports(options);
  const isCaseSensitive = options.importOrderCaseSensitive === true;

  if (isCaseSensitive) {
    warn(
      `"importOrderCaseSensitive" is migrated to "ignoreCase": false, which orders a digit after symbols and the end of a name; the plugin orders it before them`,
    );
  }
  if (
    Array.isArray(options.importOrderSafeSideEffects)
    && options.importOrderSafeSideEffects.length > 0
  ) {
    warn(`"importOrderSafeSideEffects" is not supported, side effect imports keep their position`);
  }
  return {
    partitionBySideEffect: true,
    collation: isCaseSensitive ? "natural" : "intl",
    ignoreCase: !isCaseSensitive,
    sortSpecifiers: true,
    hoist: true,
    mergeDuplicates: true,
    splitTypeSpecifiers: hasTypes,
    combineTypeAndValueImports: combine,
    newlinesBetween: false,
    customGroups: [...customGroups, ...fallbackGroups],
    groups,
  };
}

// The `sortImports` keys that each plugin option affects.
const KEYS_BY_OPTION: Record<string, string[]> = {
  importOrder: ["customGroups", "groups", "splitTypeSpecifiers", "combineTypeAndValueImports"],
  importOrderParserPlugins: ["combineTypeAndValueImports"],
  importOrderTypeScriptVersion: ["combineTypeAndValueImports"],
  importOrderCaseSensitive: ["collation", "ignoreCase"],
  importOrderSafeSideEffects: [],
};

/**
 * Build the `sortImports` value of each override, to be deep merged over the top-level one as Oxfmt does.
 * An override gets only the keys that its own plugin options affect,
 * so that overrides matching the same file combine their options as they do in Prettier.
 *
 * @param topLevelOptions - The top-level plugin options
 * @param overrideOptions - The plugin options of each override
 * @param warnings - Receives a message for each setting that cannot be reproduced
 * @returns For each override, its `sortImports` value, or `undefined` to leave it unset
 */
export function migrateIanvsOverrides(
  topLevelOptions: Record<string, unknown>,
  overrideOptions: Record<string, unknown>[],
  warnings: string[],
): (Record<string, unknown> | false | undefined)[] {
  // The top-level warnings are already reported.
  const topLevel = migrateIanvsSortImports(topLevelOptions, [], "");
  const results = overrideOptions.map((options) => {
    if (Object.keys(options).length === 0) return undefined;
    const merged = migrateIanvsSortImports(
      { ...topLevelOptions, ...options },
      warnings,
      "overrides: ",
    );
    if (!topLevel) return merged || undefined;
    if (!merged) return false;
    const keys = new Set(Object.keys(options).flatMap((key) => KEYS_BY_OPTION[key] ?? []));
    const picked = Object.fromEntries(Object.entries(merged).filter(([key]) => keys.has(key)));
    return Object.keys(picked).length > 0 ? picked : undefined;
  });

  const mergeConfig = (
    base: Record<string, unknown> | false,
    overlay: Record<string, unknown> | false | undefined,
  ) => {
    if (overlay === undefined) return base;
    return base && overlay ? { ...base, ...overlay } : overlay;
  };
  for (let j = 0; j < results.length; j++) {
    for (let i = 0; i < j; i++) {
      if (results[i] === undefined || results[j] === undefined) continue;
      const expected = migrateIanvsSortImports(
        { ...topLevelOptions, ...overrideOptions[i], ...overrideOptions[j] },
        [],
        "",
      );
      const actual = mergeConfig(mergeConfig(topLevel, results[i]), results[j]);
      if (!isDeepStrictEqual(expected, actual)) {
        warnings.push(
          `overrides: files matched by both overrides[${i}] and overrides[${j}] get different "sortImports" options than the plugin options Prettier uses for them`,
        );
      }
    }
  }
  return results;
}

// https://github.com/IanVS/prettier-plugin-sort-imports/blob/main/src/utils/normalize-plugin-options.ts
function normalizeImportOrder(importOrder: string[]): string[] {
  const result = [...importOrder];
  const spliceIndex = result[0].trim() === "" ? 1 : 0;
  if (!result.includes(THIRD_PARTY_MODULES)) {
    result.splice(spliceIndex, 0, THIRD_PARTY_MODULES);
  }
  if (!result.includes(BUILTIN_MODULES) && !result.includes(BUILTIN_MODULES_REGEX_STR)) {
    result.splice(spliceIndex, 0, BUILTIN_MODULES);
  }
  return result.map((p) => (p === BUILTIN_MODULES ? BUILTIN_MODULES_REGEX_STR : p));
}

// https://github.com/IanVS/prettier-plugin-sort-imports/blob/main/src/utils/normalize-plugin-options.ts
function canCombineTypeAndValueImports(options: Record<string, unknown>): boolean {
  const parserPlugins = Array.isArray(options.importOrderParserPlugins)
    ? options.importOrderParserPlugins
    : ["typescript", "jsx"];
  // A plugin with options is a JSON string (`'["typescript", { "dts": true }]'`), which the plugin does not count.
  if (!parserPlugins.includes("typescript")) return true;
  const version =
    typeof options.importOrderTypeScriptVersion === "string"
      ? options.importOrderTypeScriptVersion
      : "1.0.0";
  return !isSemverBelow(version, [4, 5, 0]);
}

// https://semver.org/#is-there-a-suggested-regular-expression-regex-to-check-a-semver-string, with an optional `v`
const SEMVER_REGEX =
  /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+[0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*)?$/;

// `semver.lt(version, target)`; the plugin replaces a version that is not valid semver with `1.0.0`.
function isSemverBelow(version: string, target: [number, number, number]): boolean {
  const match = SEMVER_REGEX.exec(version.trim());
  const parts = match ? [Number(match[1]), Number(match[2]), Number(match[3])] : [1, 0, 0];
  for (let i = 0; i < 3; i++) {
    if (parts[i] !== target[i]) return parts[i] < target[i];
  }
  // A prerelease is below its release.
  return match?.[4] !== undefined;
}

// ---
// Regex to glob conversion
// ---

type RegexNode =
  | { kind: "text"; glob: string }
  // `*`: any run of characters without `/`
  | { kind: "segment" }
  // Any string, including `/`
  | { kind: "any" }
  | { kind: "start" }
  | { kind: "end" }
  | { kind: "alt"; alternatives: RegexNode[][] }
  | { kind: "notAhead"; text: string };

const MAX_GLOBS = 16;

// Regex `.`, which matches `/`; glob `?` does not.
const ANY_CHAR: RegexNode = {
  kind: "alt",
  alternatives: [[{ kind: "text", glob: "?" }], [{ kind: "text", glob: "/" }]],
};

/**
 * Convert a JavaScript regex source to glob patterns that `sortImports.customGroups[].elementNamePattern`
 * matches against the same import sources.
 *
 * Supports literals, escapes, character classes, `.`, `.*`, `.+`, `[^/]*`, groups, alternation,
 * `?` and `{n,m}` quantifiers, `^` / `$` anchors, and a negative lookahead of literal text before the unanchored end.
 *
 * @returns The globs, or `null` when the regex uses anything else
 */
export function regexToGlobs(source: string): string[] | null {
  let alternatives: RegexNode[][];
  try {
    new RegExp(source);
    alternatives = new RegexParser(source).parse();
  } catch {
    return null;
  }

  const globs: string[] = [];
  for (const alternative of alternatives) {
    const sequences = expandComplexAlternations(alternative);
    if (!sequences) return null;
    for (const sequence of sequences) {
      const anchored = applyAnchors(sequence);
      if (!anchored) return null;
      const expanded = expandNegativeLookahead(anchored);
      if (!expanded) return null;
      for (const nodes of expanded) globs.push(...renderGlobs(nodes));
      if (globs.length > MAX_GLOBS) return null;
    }
  }
  return [...new Set(globs)];
}

class RegexParser {
  private readonly source: string;
  private pos = 0;

  constructor(source: string) {
    this.source = source;
  }

  parse(): RegexNode[][] {
    const alternatives = this.parseAlternatives();
    if (this.pos !== this.source.length) throw new Error("unexpected `)`");
    return alternatives;
  }

  private parseAlternatives(): RegexNode[][] {
    const alternatives = [this.parseSequence()];
    while (this.source[this.pos] === "|") {
      this.pos++;
      alternatives.push(this.parseSequence());
    }
    return alternatives;
  }

  private parseSequence(): RegexNode[] {
    const nodes: RegexNode[] = [];
    while (this.pos < this.source.length) {
      const char = this.source[this.pos];
      if (char === "|" || char === ")") break;
      const atom = this.parseAtom();
      nodes.push(...this.parseQuantifier(atom));
    }
    return nodes;
  }

  private parseAtom(): RegexNode {
    const char = this.source[this.pos++];
    switch (char) {
      case "^":
        return { kind: "start" };
      case "$":
        return { kind: "end" };
      case ".":
        return ANY_CHAR;
      case "[":
        return this.parseClass();
      case "\\":
        return this.parseEscape();
      case "(":
        return this.parseGroup();
      case "*":
      case "+":
      case "?":
        throw new Error("nothing to repeat");
      default:
        return { kind: "text", glob: escapeGlob(char) };
    }
  }

  private parseGroup(): RegexNode {
    let isNegativeLookahead = false;
    if (this.source.startsWith("?:", this.pos)) {
      this.pos += 2;
    } else if (this.source.startsWith("?!", this.pos)) {
      this.pos += 2;
      isNegativeLookahead = true;
    } else if (/^\?<[A-Za-z_$][\w$]*>/.test(this.source.slice(this.pos))) {
      this.pos = this.source.indexOf(">", this.pos) + 1;
    } else if (this.source[this.pos] === "?") {
      throw new Error("unsupported group");
    }
    const alternatives = this.parseAlternatives();
    if (this.source[this.pos++] !== ")") throw new Error("unterminated group");
    if (!isNegativeLookahead) return { kind: "alt", alternatives };

    const text = alternatives.length === 1 ? literalText(alternatives[0]) : null;
    if (text === null || text === "") throw new Error("unsupported lookahead");
    return { kind: "notAhead", text };
  }

  private parseEscape(): RegexNode {
    const char = this.source[this.pos++];
    switch (char) {
      case "d":
        return { kind: "text", glob: "[0-9]" };
      case "D":
        return { kind: "text", glob: "[!0-9]" };
      case "w":
        return { kind: "text", glob: "[a-zA-Z0-9_]" };
      case "W":
        return { kind: "text", glob: "[!a-zA-Z0-9_]" };
      default:
        if (char === undefined || /[A-Za-z0-9]/.test(char)) throw new Error("unsupported escape");
        return { kind: "text", glob: escapeGlob(char) };
    }
  }

  private parseClass(): RegexNode {
    const negated = this.source[this.pos] === "^";
    if (negated) this.pos++;
    const members: string[] = [];
    while (this.source[this.pos] !== "]") {
      if (this.pos >= this.source.length) throw new Error("unterminated class");
      let char = this.source[this.pos++];
      if (char === "\\") {
        char = this.source[this.pos++];
        if (char === "d") {
          members.push("0-9");
          continue;
        }
        if (char === "w") {
          members.push("a-zA-Z0-9_");
          continue;
        }
        if (char === undefined || /[A-Za-z0-9]/.test(char)) throw new Error("unsupported escape");
      }
      if (char === "-" && members.length > 0 && this.source[this.pos] !== "]") {
        const end = this.source[this.pos++];
        if (end === "\\" || end === "[") throw new Error("unsupported range");
        members[members.length - 1] += `-${end}`;
        continue;
      }
      members.push(char);
    }
    this.pos++;

    if (members.some((m) => /[[\]\\]/.test(m))) throw new Error("unsupported class member");
    if (members.length === 0) throw new Error("empty class");
    if (!negated && members.length === 1 && members[0].length === 1) {
      return { kind: "text", glob: escapeGlob(members[0]) };
    }
    // `-` is literal at the end of a glob class, `!` / `^` are literal after the first position.
    const sorted = [
      ...members.filter((m) => m !== "-" && m !== "!" && m !== "^"),
      ...members.filter((m) => m === "!" || m === "^"),
      ...members.filter((m) => m === "-"),
    ];
    if (!negated && (sorted[0] === "!" || sorted[0] === "^")) throw new Error("unsupported class");
    return { kind: "text", glob: `[${negated ? "!" : ""}${sorted.join("")}]` };
  }

  private parseQuantifier(atom: RegexNode): RegexNode[] {
    const rest = this.source.slice(this.pos);
    const match = /^(?:([?*+])|\{(\d+)(?:(,)(\d*))?\})\??/.exec(rest);
    if (!match) return [atom];
    this.pos += match[0].length;

    if (atom.kind === "start" || atom.kind === "end" || atom.kind === "notAhead") {
      throw new Error("unsupported quantifier");
    }
    const isAnyChar = atom === ANY_CHAR;
    const isNonSlash = atom.kind === "text" && atom.glob === "[!/]";
    const [, symbol, minText, comma, maxText] = match;
    let min = Number(minText);
    let max = comma ? Number(maxText || Infinity) : min;
    if (symbol) {
      min = symbol === "+" ? 1 : 0;
      max = symbol === "?" ? 1 : Infinity;
    }

    if (max === Infinity) {
      if (!isAnyChar && !isNonSlash) throw new Error("unsupported quantifier");
      // `[^/]` is `?`.
      const repeated: RegexNode[] = Array.from({ length: min }, () =>
        isAnyChar ? ANY_CHAR : { kind: "text", glob: "?" },
      );
      return [...repeated, isAnyChar ? { kind: "any" } : { kind: "segment" }];
    }
    if (max < min || max > 10) throw new Error("unsupported quantifier");
    const alternatives: RegexNode[][] = [];
    for (let count = min; count <= max; count++) {
      alternatives.push(Array.from({ length: count }, () => atom));
    }
    return alternatives.length === 1 ? alternatives[0] : [{ kind: "alt", alternatives }];
  }
}

function escapeGlob(text: string): string {
  return text.replaceAll(/[*?[\]{}\\,!]/g, "\\$&");
}

// The literal text of `nodes`, or `null` if they match anything but one fixed string.
function literalText(nodes: RegexNode[]): string | null {
  let text = "";
  for (const node of nodes) {
    if (node.kind !== "text" || node.glob.startsWith("[")) return null;
    text += node.glob.replace(/^\\/, "");
  }
  return text;
}

function isSimple(node: RegexNode): boolean {
  if (node.kind === "text" || node.kind === "segment") return true;
  if (node.kind === "alt") return node.alternatives.every((a) => a.every(isSimple));
  return false;
}

// Alternations that contain anchors, `any` or lookaheads become separate sequences;
// the others are printed as `{a,b}`.
function expandComplexAlternations(nodes: RegexNode[]): RegexNode[][] | null {
  let sequences: RegexNode[][] = [[]];
  for (const node of nodes) {
    if (node.kind === "alt" && !isSimple(node)) {
      const expanded: RegexNode[][] = [];
      for (const alternative of node.alternatives) {
        const inner = expandComplexAlternations(alternative);
        if (!inner) return null;
        for (const sequence of sequences) {
          for (const tail of inner) expanded.push([...sequence, ...tail]);
        }
      }
      if (expanded.length > MAX_GLOBS) return null;
      sequences = expanded;
    } else {
      for (const sequence of sequences) sequence.push(node);
    }
  }
  return sequences;
}

// Replace `^` / `$` with `any` at the unanchored ends; anchors elsewhere are unsupported.
function applyAnchors(nodes: RegexNode[]): RegexNode[] | null {
  let start = 0;
  while (nodes[start]?.kind === "start") start++;
  let end = nodes.length;
  while (end > start && nodes[end - 1].kind === "end") end--;
  const body = nodes.slice(start, end);
  if (body.some((n) => n.kind === "start" || n.kind === "end")) return null;
  return [
    ...(start > 0 ? [] : [{ kind: "any" } as const]),
    ...body,
    ...(end < nodes.length ? [] : [{ kind: "any" } as const]),
  ];
}

// `(?!abc)` followed by `any` matches the strings that do not start with `abc`:
// `""`, `a`, `ab`, or `[!a]`, `a[!b]`, `ab[!c]` followed by anything.
function expandNegativeLookahead(nodes: RegexNode[]): RegexNode[][] | null {
  const index = nodes.findIndex((n) => n.kind === "notAhead");
  if (index === -1) return [nodes];
  const node = nodes[index];
  if (node.kind !== "notAhead" || /[\]\\]/.test(node.text)) return null;
  const rest = nodes.slice(index + 1);
  if (rest.length === 0) return [nodes.slice(0, index)];
  if (rest.length !== 1 || rest[0].kind !== "any") return null;

  const prefix = nodes.slice(0, index);
  const prefixes = Array.from({ length: node.text.length }, (_, i) => node.text.slice(0, i));
  const text = (value: string): RegexNode => ({
    kind: "text",
    glob: escapeGlob(value),
  });
  const shorter: RegexNode = { kind: "alt", alternatives: prefixes.map((p) => [text(p)]) };
  const differing: RegexNode = {
    kind: "alt",
    alternatives: prefixes.map((p, i) => [text(p), { kind: "text", glob: `[!${node.text[i]}]` }]),
  };
  return [
    [...prefix, shorter],
    [...prefix, differing, { kind: "any" }],
  ];
}

function renderSimple(nodes: RegexNode[]): string {
  return nodes
    .map((node) => {
      if (node.kind === "text") return node.glob;
      if (node.kind === "segment") return "*";
      if (node.kind === "alt") {
        const alternatives = node.alternatives.map(renderSimple);
        return alternatives.length === 1 ? alternatives[0] : `{${alternatives.join(",")}}`;
      }
      throw new Error("unreachable");
    })
    .join("");
}

// `any` is `**/*` at the start, `**` after a trailing `/`, `*` or `*/**` at the end, and `*` or `*/**/*` in between.
function renderGlobs(nodes: RegexNode[]): string[] {
  const merged: RegexNode[] = [];
  for (const node of nodes) {
    const last = merged.at(-1);
    if (node.kind === "any" && (last?.kind === "any" || last?.kind === "segment")) {
      merged[merged.length - 1] = node;
    } else if (node.kind === "segment" && (last?.kind === "any" || last?.kind === "segment")) {
      continue;
    } else {
      merged.push(node);
    }
  }
  if (merged.length === 1 && merged[0].kind === "any") return ["**"];

  let globs = [""];
  merged.forEach((node, i) => {
    if (node.kind !== "any") {
      const text = renderSimple([node]);
      globs = globs.map((g) => g + text);
      return;
    }
    let variants: string[];
    if (i === 0) variants = ["**/*"];
    else if (i === merged.length - 1) variants = globs[0].endsWith("/") ? ["**"] : ["*", "*/**"];
    else variants = ["*", "*/**/*"];
    globs = globs.flatMap((g) => variants.map((v) => g + v));
  });
  return globs;
}
