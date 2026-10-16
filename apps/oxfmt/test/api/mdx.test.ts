import { describe, expect, it } from "vitest";
import { format } from "../../dist/index.js";

describe("MDX comments", () => {
  it("should keep multi-line comments as-is", async () => {
    const input = `# Comments

{/*
  multi line
  comment with *emph*
*/}

{/*
para one

para two
*/}

{/*
comment
*/}
Text right after the comment.

- item

  {/*
    nested
  */}

> quote
>
> {/*
> in quote
> */}
`;
    const result = await format("a.mdx", input);

    expect(result.code).toBe(input);
    expect(result.errors).toStrictEqual([]);
  });

  it("should still format single-line comments", async () => {
    const input = `{/*   single line   */}

Text {/* inline */} and \`{/*\` code
\`*/}\` here.
`;
    const result = await format("a.mdx", input);

    expect(result.code).toBe(`{/* single line */}

Text {/* inline */} and \`{/*\` code
\`*/}\` here.
`);
    expect(result.errors).toStrictEqual([]);
  });
});

describe("MDX sortImports.hoist", () => {
  it("should hoist imports within each ESM block and code block", async () => {
    const input = `export const meta = { title: "x" };
import { b } from "b";
import { a } from "a";

# Title

export const other = 1;
import { c } from "c";

\`\`\`ts
const value = 1;
import { d } from "d";
\`\`\`
`;
    const result = await format("a.mdx", input, { sortImports: { hoist: true } });

    expect(result.code).toBe(`import { a } from "a";
import { b } from "b";

export const meta = { title: "x" };

# Title

import { c } from "c";

export const other = 1;

\`\`\`ts
import { d } from "d";

const value = 1;
\`\`\`
`);
    expect(result.errors).toStrictEqual([]);
  });
});
