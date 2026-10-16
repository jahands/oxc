#!/usr/bin/env node
// JHPATCH: Rewrites oxfmt's package names and version in the CI checkout so the fork publishes as `@jahands/oxfmt`
/* eslint-disable no-console */

// Usage: node .github/scripts/jahands-oxfmt-prepare.mjs <version>
// <version> is `<upstream oxfmt version>-jh.<n>`, e.g. `0.73.0-jh.1`.

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const rootDir = join(import.meta.dirname, "../..");
const read = (path) => readFileSync(join(rootDir, path), "utf8");
const write = (path, content) => writeFileSync(join(rootDir, path), content);

const version = process.argv[2] ?? "";

// `apps/oxfmt/package.json` is never rewritten, so it holds the upstream version on every run.
const upstreamVersion = JSON.parse(read("apps/oxfmt/package.json")).version;

if (!new RegExp(`^${upstreamVersion.replaceAll(".", "\\.")}-jh\\.\\d+$`).test(version)) {
  console.error(
    `Version must be ${upstreamVersion}-jh.<n> (upstream oxfmt ${upstreamVersion}), got "${version}".`,
  );
  process.exit(1);
}

const pkgPath = "npm/oxfmt/package.json";
const pkg = JSON.parse(read(pkgPath));
pkg.name = "@jahands/oxfmt";
pkg.version = version;
pkg.bugs = "https://github.com/jahands/oxc/issues";
// npm provenance requires `repository.url` to match the publishing repository.
pkg.repository.url = "git+https://github.com/jahands/oxc.git";
pkg.napi.packageName = "@jahands/oxfmt-binding";
write(pkgPath, `${JSON.stringify(pkg, null, 2)}\n`);

const bindingsPath = "apps/oxfmt/src-js/bindings.js";
const bindings = read(bindingsPath)
  .replaceAll("@oxfmt/binding-", "@jahands/oxfmt-binding-")
  .replaceAll(`'${upstreamVersion}'`, `'${version}'`)
  .replaceAll(`expected ${upstreamVersion} but`, `expected ${version} but`);
if (bindings.includes("@oxfmt/binding") || !bindings.includes(`'${version}'`)) {
  console.error(`${bindingsPath} still references @oxfmt/binding or lacks the version check.`);
  process.exit(1);
}
write(bindingsPath, bindings);

const cargoTomlPath = "apps/oxfmt/Cargo.toml";
const cargoToml = read(cargoTomlPath);
if (!/^version = ".*"$/m.test(cargoToml)) {
  console.error(`No package version found in ${cargoTomlPath}.`);
  process.exit(1);
}
write(cargoTomlPath, cargoToml.replace(/^version = ".*"$/m, `version = "${version}"`));

console.log(`Prepared @jahands/oxfmt ${version} (upstream oxfmt ${upstreamVersion}).`);
