// node --test npm/build-packages.test.mjs
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { MAIN_PACKAGE, PLATFORMS, buildPackages, npmVersion } from "./build-packages.mjs";

test("release tags map to versions that sort like the tags", () => {
  assert.equal(npmVersion("v20261117"), "20261117.0.0");
  assert.equal(npmVersion("v20261117.1"), "20261117.1.0");
  assert.equal(npmVersion("v20261117.12"), "20261117.12.0");
  for (const bad of ["20261117", "v2026111", "v20261117-rc1", "v20261117.1.2", "vlatest"]) {
    assert.throws(() => npmVersion(bad), /not a release tag/, bad);
  }
});

function fakeBinaries(tag, skip = []) {
  const dir = mkdtempSync(join(tmpdir(), "ml-npm-bin-"));
  for (const p of PLATFORMS) {
    if (skip.includes(p.target)) continue;
    const d = join(dir, `memorylake-${tag}-${p.target}`);
    mkdirSync(d, { recursive: true });
    writeFileSync(join(d, p.exe), `binary for ${p.target}`);
  }
  return dir;
}

test("one package per platform, then the main package pinned to them", () => {
  const tag = "v20261117.1";
  const out = mkdtempSync(join(tmpdir(), "ml-npm-out-"));
  const { version, order } = buildPackages({ tag, binaries: fakeBinaries(tag), out });

  assert.equal(version, "20261117.1.0");
  assert.equal(order.length, PLATFORMS.length + 1);
  assert.deepEqual(readFileSync(join(out, "publish-order.txt"), "utf8").trim().split("\n"), order);

  const main = JSON.parse(readFileSync(join(order.at(-1), "package.json"), "utf8"));
  assert.equal(main.name, MAIN_PACKAGE);
  assert.equal(main.version, version);
  assert.deepEqual(main.bin, { memorylake: "bin/memorylake.js" });
  assert.deepEqual(main.optionalDependencies, Object.fromEntries(PLATFORMS.map((p) => [p.name, version])));
  assert.ok(statSync(join(order.at(-1), "LICENSE")).isFile());

  for (const [i, p] of PLATFORMS.entries()) {
    const pkg = JSON.parse(readFileSync(join(order[i], "package.json"), "utf8"));
    assert.equal(pkg.name, p.name);
    assert.equal(pkg.version, version);
    assert.deepEqual(pkg.os, [p.os]);
    assert.deepEqual(pkg.cpu, [p.cpu]);
    assert.deepEqual(pkg.libc, p.os === "linux" ? ["glibc"] : undefined);
    const bin = join(order[i], "bin", p.exe);
    assert.equal(readFileSync(bin, "utf8"), `binary for ${p.target}`);
    if (process.platform !== "win32") assert.equal(statSync(bin).mode & 0o111, 0o111);
  }
});

test("a missing platform binary fails the build instead of publishing without it", () => {
  const tag = "v20261117";
  const out = mkdtempSync(join(tmpdir(), "ml-npm-out-"));
  assert.throws(
    () => buildPackages({ tag, binaries: fakeBinaries(tag, ["aarch64-pc-windows-msvc"]), out }),
    /missing binary for aarch64-pc-windows-msvc/,
  );
});

test("the launcher knows every platform the build publishes", () => {
  const launcher = readFileSync(new URL("./memorylake/bin/memorylake.js", import.meta.url), "utf8");
  const supported = JSON.parse(`[${/SUPPORTED = new Set\(\[([^\]]*)\]\)/.exec(launcher)[1]}]`);
  assert.deepEqual(supported.sort(), PLATFORMS.map((p) => `${p.os} ${p.cpu}`).sort());
  assert.match(launcher, new RegExp(`const SCOPE = "${MAIN_PACKAGE.split("/")[0]}"`));
});
