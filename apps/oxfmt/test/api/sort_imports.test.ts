import { describe, expect, it } from "vitest";
import { format } from "../../dist/index.js";

describe("Sort imports", () => {
  it("should sort with customGroups", async () => {
    const input = `import { foo } from "./foo";
import { util } from "~/utils/util";
import { store } from "~/stores/store";
`;
    const result = await format("a.ts", input, {
      experimentalSortImports: {
        newlinesBetween: false,
        customGroups: [
          { elementNamePattern: ["~/stores/**"], groupName: "stores" },
          { elementNamePattern: ["~/utils/**"], groupName: "utils" },
        ],
        groups: ["stores", "utils", "sibling"],
      },
    });

    expect(result.code).toBe(
      `
import { store } from "~/stores/store";
import { util } from "~/utils/util";
import { foo } from "./foo";
`.trimStart(),
    );
    expect(result.errors).toStrictEqual([]);
  });

  it("should sort with customGroups using selector and modifiers", async () => {
    const input = `import { bar } from "@scope/bar";
import type { FooType } from "@scope/foo";
import { foo } from "@scope/foo";
import type { BarType } from "@scope/bar";
`;
    const result = await format("a.ts", input, {
      experimentalSortImports: {
        customGroups: [
          {
            groupName: "scope-types",
            elementNamePattern: ["@scope/**"],
            modifiers: ["type"],
          },
          {
            groupName: "scope-values",
            elementNamePattern: ["@scope/**"],
            modifiers: ["value"],
          },
        ],
        groups: ["scope-types", "scope-values", "unknown"],
      },
    });

    expect(result.code).toBe(
      `
import type { BarType } from "@scope/bar";
import type { FooType } from "@scope/foo";

import { bar } from "@scope/bar";
import { foo } from "@scope/foo";
`.trimStart(),
    );
    expect(result.errors).toStrictEqual([]);
  });

  it("should hoist imports with hoist", async () => {
    const input = `"use client";
import { b } from "b";
const x = 1;
// About a.
import { a } from "a";
`;
    const result = await format("a.ts", input, {
      sortImports: { hoist: true },
    });

    expect(result.code).toBe(
      `
"use client";

// About a.
import { a } from "a";
import { b } from "b";

const x = 1;
`.trimStart(),
    );
    expect(result.errors).toStrictEqual([]);
  });

  it("should sort with partitionBySideEffect, collation and sortSpecifiers", async () => {
    const input = `import { z, type T, a } from "#lib/z";
import { y } from "@scope/y";
import "./setup";
import { c, b } from "c";
`;
    const result = await format("a.ts", input, {
      sortImports: {
        partitionBySideEffect: true,
        collation: "intl",
        sortSpecifiers: true,
      },
    });

    expect(result.code).toBe(
      `
import { y } from "@scope/y";

import { a, z, type T } from "#lib/z";

import "./setup";

import { b, c } from "c";
`.trimStart(),
    );
    expect(result.errors).toStrictEqual([]);
  });
});

describe("Sort imports with mergeDuplicates and splitTypeSpecifiers", () => {
  const input = `import { useState } from "react";
import { z } from "zod";
import React, { type FC } from "react";
import type { ReactNode } from "react";
`;

  it("should merge imports of the same source", async () => {
    const result = await format("a.ts", input, {
      sortImports: { mergeDuplicates: true, sortSpecifiers: true },
    });

    expect(result.code).toBe(
      `
import React, { useState, type FC, type ReactNode } from "react";
import { z } from "zod";
`.trimStart(),
    );
    expect(result.errors).toStrictEqual([]);
  });

  it("should split inline type specifiers into type imports", async () => {
    const result = await format("a.ts", input, {
      sortImports: {
        mergeDuplicates: true,
        splitTypeSpecifiers: true,
        sortSpecifiers: true,
        groups: ["value-import", "type-import"],
      },
    });

    expect(result.code).toBe(
      `
import React, { useState } from "react";
import { z } from "zod";

import type { FC } from "react";
import type { ReactNode } from "react";
`.trimStart(),
    );
    expect(result.errors).toStrictEqual([]);
  });
});

describe("Sort imports in MDX", () => {
  const input = `import { z } from "zod";
import { a } from "a";

# Title

\`\`\`js
import { y } from "y";
import { b } from "b";
\`\`\`

import { d } from "d";
import { c } from "c";
export const meta = { title: "x" };
`;

  it("should sort each ESM block and code block", async () => {
    const result = await format("a.mdx", input, { sortImports: true });

    expect(result.code).toBe(
      `import { a } from "a";
import { z } from "zod";

# Title

\`\`\`js
import { b } from "b";
import { y } from "y";
\`\`\`

import { c } from "c";
import { d } from "d";
export const meta = { title: "x" };
`,
    );
    expect(result.errors).toStrictEqual([]);
  });

  it("should not sort when sortImports is disabled", async () => {
    const result = await format("a.mdx", input);

    expect(result.code).toBe(input);
    expect(result.errors).toStrictEqual([]);
  });

  it("should keep a block with other statements as-is, like Prettier", async () => {
    const block = `import { z } from "zod";
import   { a } from "a";
export *   from "./all";
`;
    const result = await format("a.mdx", block, { sortImports: true });

    expect(result.code).toBe(block);
    expect(result.errors).toStrictEqual([]);
  });

  it("should not add ; for single JSX element in code block", async () => {
    const block = `\`\`\`jsx
<div>Hello, JSX</div>
\`\`\`
`;
    const result = await format("a.mdx", block, { sortImports: true });

    expect(result.code).toBe(block);
    expect(result.errors).toStrictEqual([]);
  });
});
