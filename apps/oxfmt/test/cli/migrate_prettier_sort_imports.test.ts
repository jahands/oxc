import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { execa } from "execa";
import { describe, expect, it } from "vitest";
import { format } from "../../dist/index.js";
import { regexToGlobs } from "../../src-js/cli/migration/prettier-sort-imports";

const CLI_PATH = join(import.meta.dirname, "..", "..", "dist", "cli.js");

const REGEXES = [
  "^~.+",
  "^@.+",
  "^#.+",
  "^a.b$",
  "^a.+b$",
  "^a.?b",
  "a.{2}b",
  "^.+/foo$",
  "^.$",
  "^[^a]b",
  "^(a|.)b",
  "^[.]{2}\\/",
  "^[.]\\/(?!index)",
  "^(@repo|@wci)(\\/.*)$",
  "^@[^/]+/.+$",
  "^[^/]+$",
  "^a{2,3}b$",
  ".css$",
];

const SOURCES = [
  "~/lib/a",
  "~x",
  "~",
  "@/x",
  "@scope/pkg/sub",
  "@wci",
  "#/x",
  "a/b",
  "axb",
  "a//b",
  "ax/b",
  "a/x/b",
  "/foo",
  "x/foo",
  "/",
  "/b",
  "./index",
  "./indexer",
  "../x/y",
  "a/x.css",
  "aab",
];

describe("regexToGlobs", () => {
  it.each(REGEXES)("should match the same sources as %s", async (regex) => {
    const globs = regexToGlobs(regex);
    expect(globs).not.toBeNull();

    // Imports matched by the globs are sorted above the `=sentinel` import.
    const input = [...SOURCES, "=sentinel"]
      .map((source, i) => `import x${i} from "${source}";`)
      .join("\n");
    const result = await format("a.ts", `${input}\n`, {
      sortImports: {
        newlinesBetween: false,
        customGroups: [
          { groupName: "sentinel", elementNamePattern: ["=sentinel"] },
          { groupName: "regex", elementNamePattern: globs ?? [] },
        ],
        groups: ["regex", "sentinel", "unknown"],
      },
    });
    const lines = result.code.trimEnd().split("\n");
    const matched = lines
      .slice(0, lines.indexOf(`import x${SOURCES.length} from "=sentinel";`))
      .map((line) => /from "(.*)";$/.exec(line)?.[1]);

    const re = new RegExp(regex);
    expect(new Set(matched)).toStrictEqual(new Set(SOURCES.filter((s) => re.test(s))));
  });

  it("should not convert a lookahead with a pattern", () => {
    expect(regexToGlobs("^(?!.*[.]css$)[./].*$")).toBeNull();
  });
});

// The expected output is Prettier's with the same `.prettierrc.json`.
describe("--migrate prettier with @ianvs/prettier-plugin-sort-imports", () => {
  const run = async (prettierrc: unknown, files: Record<string, string>) => {
    const dir = await mkdtemp(join(tmpdir(), "oxfmt-migrate-ianvs-"));
    try {
      await writeFile(join(dir, ".prettierrc.json"), JSON.stringify(prettierrc));
      await execa("node", [CLI_PATH, "--migrate", "prettier"], { cwd: dir });
      await Promise.all(
        Object.entries(files).map(async ([path, code]) => {
          await mkdir(dirname(join(dir, path)), { recursive: true });
          await writeFile(join(dir, path), code);
        }),
      );
      await execa("node", [CLI_PATH, ...Object.keys(files)], { cwd: dir });
      return Object.fromEntries(
        await Promise.all(
          Object.keys(files).map(async (path) => [path, await readFile(join(dir, path), "utf8")]),
        ),
      );
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  };

  it("should match `.` against `/`", async () => {
    const formatted = await run(
      {
        plugins: ["@ianvs/prettier-plugin-sort-imports"],
        importOrderTypeScriptVersion: "5.4.0",
        importOrder: ["<BUILTIN_MODULES>", "<THIRD_PARTY_MODULES>", "", "^~.+", "", "^[./]"],
      },
      {
        "a.ts": `import { a } from "~/lib/a";\nimport { z } from "zod";\nimport { r } from "./r";\n`,
      },
    );
    expect(formatted["a.ts"]).toBe(
      `import { z } from "zod";\n\nimport { a } from "~/lib/a";\n\nimport { r } from "./r";\n`,
    );
  });

  it("should combine the options of overrides that match the same file", async () => {
    const input = `import { z } from "zod";\nimport { a } from "./a";\nimport fs from "node:fs";\n`;
    const formatted = await run(
      {
        plugins: ["@ianvs/prettier-plugin-sort-imports"],
        importOrderTypeScriptVersion: "5.4.0",
        overrides: [
          {
            files: "*.ts",
            options: { importOrder: ["<BUILTIN_MODULES>", "^[.]", "<THIRD_PARTY_MODULES>"] },
          },
          { files: "lib/**", options: { importOrderCaseSensitive: false } },
        ],
      },
      { "b.ts": input, "lib/a.ts": input },
    );
    const expected = `import fs from "node:fs";\nimport { a } from "./a";\nimport { z } from "zod";\n`;
    expect(formatted["b.ts"]).toBe(expected);
    expect(formatted["lib/a.ts"]).toBe(expected);
  });

  it("should merge type imports and value imports separately below TypeScript 4.5", async () => {
    const formatted = await run(
      { plugins: ["@ianvs/prettier-plugin-sort-imports"] },
      {
        "a.ts": `import { z } from "zod";\nimport type { Z } from "zod";\nimport { type Y, y } from "zod";\n`,
      },
    );
    expect(formatted["a.ts"]).toBe(
      `import { y, z, type Y } from "zod";\nimport type { Z } from "zod";\n`,
    );
  });
});
