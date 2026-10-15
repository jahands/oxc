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
