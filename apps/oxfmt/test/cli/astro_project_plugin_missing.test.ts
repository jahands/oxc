import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execa } from "execa";
import { expect, it } from "vitest";

const CLI_PATH = join(import.meta.dirname, "..", "..", "dist", "cli.js");

// The project lives outside this repository, so no parent `node_modules` provides the plugin
it("should report an error for `astro.useProjectPlugin` when the project does not have the plugin", async () => {
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
