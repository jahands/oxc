// JHPATCH: Prettier mdx parser override that keeps multi-line MDX comments verbatim (DIVERGENCES.md#mdx-multiline-comment)
/**
 * Prettier plugin that keeps multi-line MDX comments (`{/* ... *\/}`) verbatim.
 *
 * Prettier's `mdx` parser recognizes `{/* ... *\/}` only within one line.
 * A multi-line one is parsed as a paragraph, where `*...*` becomes emphasis and is printed as `_..._`,
 * which is not a comment anymore and fails to compile.
 * See `DIVERGENCES.md#mdx-multiline-comment`.
 */

import { parsers as markdownParsers } from "prettier/plugins/markdown";
import type { Parser } from "prettier";

type Point = { line: number; column: number; offset: number };

type MdastNode = {
  type: string;
  value?: string;
  children?: MdastNode[];
  position: { start: Point; end: Point };
};

const COMMENT_OPEN_RE = /\{\s*\/\*/gv;
const COMMENT_CLOSE_RE = /\*\/\s*\}/gv;
// Inline nodes whose `{/*` is not an unparsed comment: a parsed one, or literal text.
const LITERAL_INLINE_TYPES = new Set([
  "esComment",
  "inlineCode",
  "inlineMath",
  "html",
  "jsx",
  "liquidNode",
]);

const mdxParser = markdownParsers.mdx;

export const parsers: Record<string, Parser> = {
  mdx: {
    ...mdxParser,
    parse: async (text, options) => {
      const ast = (await mdxParser.parse(text, options)) as MdastNode;
      preserveMultilineComments(ast, text);
      return ast;
    },
  },
};

/**
 * Replaces each run of sibling block nodes covering a multi-line comment with one `html` node
 * holding their source text, which the `mdast` printer prints as-is.
 */
function preserveMultilineComments(node: MdastNode, text: string): void {
  const { children } = node;
  if (!children) return;

  for (let i = 0; i < children.length; i++) {
    const child = children[i];
    const commentEnd = child.type === "paragraph" ? findUnparsedCommentEnd(child, text) : -1;

    if (commentEnd === -1) {
      preserveMultilineComments(child, text);
      continue;
    }

    let last = i;
    while (last + 1 < children.length && children[last + 1].position.start.offset < commentEnd) {
      last++;
    }

    const { start } = child.position;
    const { end } = children[last].position;
    const containerPrefix = text.slice(start.offset - (start.column - 1), start.offset);
    children.splice(i, last - i + 1, {
      type: "html",
      value: stripContainerPrefix(text.slice(start.offset, end.offset), containerPrefix),
      position: { start, end },
    });
  }
}

/**
 * Removes the container prefix (list indentation, blockquote markers) from every line but the first,
 * as the printer adds it back to `html` nodes.
 *
 * @param containerPrefix - The text before the node on its first line, e.g. `"- "` or `"> "`
 */
function stripContainerPrefix(source: string, containerPrefix: string): string {
  if (containerPrefix === "") return source;

  const contentStartRe = containerPrefix.includes(">") ? /[^\s>]/v : /\S/v;
  return source
    .split("\n")
    .map((line, idx) => {
      if (idx === 0) return line;
      const contentStart = line.search(contentStartRe);
      const prefixLength = contentStart === -1 ? line.length : contentStart;
      return line.slice(Math.min(prefixLength, containerPrefix.length));
    })
    .join("\n");
}

/**
 * Returns the end offset of the first comment that opens in `paragraph` and is not an `esComment` node,
 * or `-1` if there is none.
 */
function findUnparsedCommentEnd(paragraph: MdastNode, text: string): number {
  const { start, end } = paragraph.position;
  const literalRanges: [number, number][] = [];
  collectLiteralRanges(paragraph, literalRanges);

  COMMENT_OPEN_RE.lastIndex = start.offset;
  for (let open; (open = COMMENT_OPEN_RE.exec(text)) && open.index < end.offset;) {
    const openIndex = open.index;
    if (literalRanges.some(([s, e]) => s <= openIndex && openIndex < e)) continue;

    COMMENT_CLOSE_RE.lastIndex = COMMENT_OPEN_RE.lastIndex;
    const close = COMMENT_CLOSE_RE.exec(text);
    return close === null ? -1 : close.index + close[0].length;
  }
  return -1;
}

function collectLiteralRanges(node: MdastNode, ranges: [number, number][]): void {
  for (const child of node.children ?? []) {
    if (LITERAL_INLINE_TYPES.has(child.type)) {
      ranges.push([child.position.start.offset, child.position.end.offset]);
    } else {
      collectLiteralRanges(child, ranges);
    }
  }
}
