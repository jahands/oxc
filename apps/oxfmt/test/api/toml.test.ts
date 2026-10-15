import { describe, expect, it } from "vitest";
import { format } from "../../dist/index.js";

// Formatting itself is tested in `oxc-toml`; these pin the oxfmt wiring.
// File names, `useTabs` and `insertFinalNewline` are covered by `test/cli`.
describe("TOML files (oxc_formatter_toml)", () => {
  it("should map tabWidth, printWidth and trailingComma", async () => {
    const source = `a = ["aaaa", "bbbb", "cccc"]\n`;
    const result = await format("a.toml", source, {
      tabWidth: 4,
      printWidth: 20,
      trailingComma: "none",
    });
    expect(result.errors).toStrictEqual([]);
    expect(result.code).toMatchInlineSnapshot(`
      "a = [
          "aaaa",
          "bbbb",
          "cccc"
      ]
      "
    `);
  });

  it("should map endOfLine", async () => {
    const result = await format("a.toml", "a=1\nb=2\n", { endOfLine: "crlf" });
    expect(result.errors).toStrictEqual([]);
    expect(result.code).toBe("a = 1\r\nb = 2\r\n");
  });

  it("should keep Taplo defaults without toml options", async () => {
    const source = `a = 1 # c\nbbbbb = 2\n\n\nc = [1, 2]\n`;
    const result = await format("a.toml", source);
    expect(result.errors).toStrictEqual([]);
    expect(result.code).toBe(source);
  });

  it("should map toml options", async () => {
    const source = `a = 1 # c\nbbbbb = 2\n\n\nc = [1, 2]\n`;
    const result = await format("a.toml", source, {
      toml: {
        alignComments: true,
        alignSingleComments: true,
        allowedBlankLines: 1,
        compactArrays: false,
      },
    });
    expect(result.errors).toStrictEqual([]);
    expect(result.code).toMatchInlineSnapshot(`
      "a = 1     # c
      bbbbb = 2

      c = [ 1, 2 ]
      "
    `);
  });

  it("should apply toml options to Markdown code blocks", async () => {
    const source = "```toml\na = 1\nbbbbb = 2\n```\n";
    const result = await format("a.md", source, { toml: { alignEntries: true } });
    expect(result.errors).toStrictEqual([]);
    expect(result.code).toMatchInlineSnapshot(`
      "\`\`\`toml
      a     = 1
      bbbbb = 2
      \`\`\`
      "
    `);
  });

  it("should accept TOML 1.1 syntax", async () => {
    const source = `esc="\\e[0m"\ntime=07:32\ninline={ a=1,\n  b=2, }\n`;
    const result = await format("a.toml", source);
    expect(result.errors).toStrictEqual([]);
    expect(result.code).toMatchInlineSnapshot(`
      "esc = "\\e[0m"
      time = 07:32
      inline = { a = 1, b = 2 }
      "
    `);
  });

  it("should report a diagnostic for broken input", async () => {
    // `oxc-toml` alone would format around the invalid range
    const source = "a   =   1\nb = = 1\n";
    const result = await format("broken.toml", source);
    expect(result.code).toBe(source);
    expect(result.errors).toHaveLength(1);
    expect(result.errors[0].message).toMatch(/Syntax error/);
  });
});
