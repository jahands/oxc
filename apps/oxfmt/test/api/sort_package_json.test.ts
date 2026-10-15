import { describe, expect, it } from "vitest";
import { format } from "../../dist/index.js";

describe("Sort package.json fields", () => {
  it("should work with `format()` API", async () => {
    const pkgJSON = JSON.stringify({
      version: "1.0.0",
      name: "my-package",
    });
    const result1 = await format("package.json", pkgJSON);
    expect(result1.code).toBe(`{\n  "name": "my-package",\n  "version": "1.0.0"\n}\n`);
    expect(result1.errors).toStrictEqual([]);

    const result2 = await format("package.json", pkgJSON, { experimentalSortPackageJson: false });
    expect(result2.code).toBe(`{\n  "version": "1.0.0",\n  "name": "my-package"\n}\n`);
    expect(result2.errors).toStrictEqual([]);
  });

  it('should use the sort-package-json order with `preset: "sort-package-json"`', async () => {
    const pkgJSON = JSON.stringify({
      type: "module",
      sideEffects: false,
      main: "./index.js",
      exports: "./index.js",
      keywords: ["b", "a"],
    });
    const result1 = await format("package.json", pkgJSON, {
      sortPackageJson: { preset: "sort-package-json" },
    });
    expect(result1.code).toBe(
      `{\n  "keywords": [\n    "b",\n    "a"\n  ],\n  "sideEffects": false,\n  "type": "module",\n  "exports": "./index.js",\n  "main": "./index.js"\n}\n`,
    );
    expect(result1.errors).toStrictEqual([]);

    const result2 = await format("package.json", pkgJSON);
    expect(result2.code).toBe(
      `{\n  "keywords": [\n    "a",\n    "b"\n  ],\n  "type": "module",\n  "sideEffects": false,\n  "main": "./index.js",\n  "exports": "./index.js"\n}\n`,
    );
    expect(result2.errors).toStrictEqual([]);
  });
});
