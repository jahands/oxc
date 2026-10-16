// JHPATCH: Forward Tailwind's wrapped Astro parsers to the bundled or project prettier-plugin-astro (astro.useProjectPlugin)
/**
 * Stand-in for `prettier-plugin-astro` inside `prettier-plugin-tailwindcss`.
 *
 * The Tailwind plugin imports the Astro plugin once per process and wraps its parsers.
 * Each parser here forwards to the Astro plugin in `options.plugins`,
 * which is either the bundled one or the project's one (`astro.useProjectPlugin`).
 * The two versions build different ASTs, so the wrapped parser must match the printer in use.
 */

import * as babel from "prettier/plugins/babel";
import type { Parser, ParserOptions, Plugin } from "prettier";

function findAstroParser(options: ParserOptions, name: string): Parser {
  const plugin = options.plugins.findLast(
    (plugin): plugin is Plugin =>
      typeof plugin === "object" && "printers" in plugin && plugin.printers?.astro !== undefined,
  );
  const parser = plugin?.parsers?.[name];
  if (!parser) throw new Error(`No \`prettier-plugin-astro\` provides the \`${name}\` parser`);
  return parser;
}

export const parsers: Record<string, Parser> = {
  astro: {
    astFormat: "astro",
    parse: (text, options) => findAstroParser(options, "astro").parse(text, options),
    // 1.x nodes have `start` / `end`, 0.x nodes have `position`
    locStart: (node) => node.start ?? node.position.start.offset,
    locEnd: (node) => node.end ?? node.position.end.offset,
  },
  // Only 0.x has this parser, for `{expressions}` in the template
  astroExpressionParser: {
    ...babel.parsers["babel-ts"],
    preprocess: (text, options) => {
      const parser = findAstroParser(options, "astroExpressionParser");
      return parser.preprocess ? parser.preprocess(text, options) : text;
    },
    parse: (text, options) =>
      findAstroParser(options, "astroExpressionParser").parse(text, options),
  },
};
