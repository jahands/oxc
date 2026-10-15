import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { execa } from "execa";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const CLI_PATH = join(import.meta.dirname, "..", "..", "dist", "cli.js");

const INPUT = `---
import b from "b"
import a from "a"
const items=["one","two"]
const showImage = true
---
<html lang="en">
<body class="min-h-screen bg-white text-black antialiased dark:bg-black dark:text-white sm:px-4">
{/* A comment */}
<main>
{showImage && (<img src="/image.png" alt="An image with a description long enough to break the line" />)}
<ul>{items.map((item) => <li class="p-2 flex">{item}</li>)}</ul>
<a href="/" class="text-sm hover:underline">home <span>icon</span></a>
</main>
<script>
const x   = 1
</script>
<style>
main{color:red}
</style>
</body>
</html>
`;

describe("`astro.useProjectPlugin`", () => {
  let project: string;

  beforeAll(async () => {
    project = await mkdtemp(join(tmpdir(), "oxfmt-astro-project-"));
    await mkdir(join(project, "node_modules"));
    await symlink(
      dirname(createRequire(import.meta.url).resolve("prettier-plugin-astro-0.14/package.json")),
      join(project, "node_modules", "prettier-plugin-astro"),
      "dir",
    );
    await writeFile(
      join(project, ".oxfmtrc.json"),
      JSON.stringify({ astro: { useProjectPlugin: true }, sortImports: {}, sortTailwindcss: {} }),
    );
    await writeFile(
      join(project, "fmt-bundled.json"),
      JSON.stringify({ astro: true, sortImports: {}, sortTailwindcss: {} }),
    );
  });

  afterAll(async () => {
    await rm(project, { recursive: true, force: true });
  });

  async function formatApp(args: string[]): Promise<string> {
    const path = join(project, "App.astro");
    await writeFile(path, INPUT);
    const { stdout, stderr, exitCode } = await execa(
      "node",
      [CLI_PATH, ...args, "App.astro", "--threads=1"],
      { cwd: project, reject: false },
    );
    expect({ exitCode, stderr }).toStrictEqual({ exitCode: 0, stderr: "" });
    expect(stdout).toContain("on 1 files");
    return readFile(path, "utf8");
  }

  it("should format with the plugin installed in the project", async () => {
    const projectOutput = await formatApp([]);
    expect(projectOutput).toMatchSnapshot();

    const bundledOutput = await formatApp(["--config", "fmt-bundled.json"]);
    expect(bundledOutput).toMatchSnapshot();
    expect(projectOutput).not.toBe(bundledOutput);
  });

  // The project lives outside this repository, so no parent `node_modules` provides the plugin
  it("should report an error when the project does not have the plugin", async () => {
    const dir = await mkdtemp(join(tmpdir(), "oxfmt-astro-"));
    try {
      await writeFile(join(dir, ".oxfmtrc.json"), '{ "astro": { "useProjectPlugin": true } }');
      await writeFile(join(dir, "App.astro"), "<p   />\n");

      // pnpm sets `NODE_PATH` to its hoisted `node_modules`, which has the bundled version
      const { NODE_PATH: _nodePath, ...env } = process.env;
      const { stdout, stderr, exitCode } = await execa("node", [CLI_PATH, "App.astro"], {
        cwd: dir,
        env,
        extendEnv: false,
        reject: false,
      });

      expect(exitCode).toBe(2);
      expect(stdout + stderr).toContain(
        "`astro.useProjectPlugin` is enabled, but `prettier-plugin-astro` cannot be resolved from",
      );
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });
});
