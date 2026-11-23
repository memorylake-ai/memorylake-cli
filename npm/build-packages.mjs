#!/usr/bin/env node
// Turns the release binaries into npm packages.
//
//   node npm/build-packages.mjs --tag v20261117 --binaries <dir> --out <dir>
//
// <dir> for --binaries holds the release archives unpacked, as the release
// workflow builds them: memorylake-<tag>-<target>/memorylake[.exe].
//
// It writes one package per platform, each holding just that platform's
// binary, and the package people install (npm/memorylake), which lists them
// all as optional dependencies. npm installs only the one whose `os`, `cpu`
// and `libc` match the machine, so an install downloads one binary, from
// whatever registry or mirror npm is set to use. This is the layout esbuild
// and Biome use for their native binaries.
//
// The order the packages must be published in (platforms first, so the
// main package never points at a version that is not there yet) is written
// to <out>/publish-order.txt, one directory per line.

import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

export const SCOPE = "@memorylake";
export const MAIN_PACKAGE = `${SCOPE}/cli`;

// Rust target -> the npm platform package that carries it. `os` and `cpu` use
// Node's process.platform / process.arch names; the launcher looks a package
// up by the same pair.
export const PLATFORMS = [
  { target: "aarch64-apple-darwin", os: "darwin", cpu: "arm64", exe: "memorylake" },
  { target: "x86_64-apple-darwin", os: "darwin", cpu: "x64", exe: "memorylake" },
  { target: "x86_64-unknown-linux-gnu", os: "linux", cpu: "x64", libc: "glibc", exe: "memorylake" },
  { target: "aarch64-unknown-linux-gnu", os: "linux", cpu: "arm64", libc: "glibc", exe: "memorylake" },
  { target: "x86_64-pc-windows-msvc", os: "win32", cpu: "x64", exe: "memorylake.exe" },
  { target: "aarch64-pc-windows-msvc", os: "win32", cpu: "arm64", exe: "memorylake.exe" },
].map((p) => ({ ...p, name: `${SCOPE}/cli-${p.os}-${p.cpu}` }));

/**
 * The npm version for a release tag. Tags are dates, with a counter for a
 * second release the same day (CONTRIBUTING.md): v20261117 -> 20261117.0.0,
 * v20261117.1 -> 20261117.1.0. Both sort the way the tags do.
 */
export function npmVersion(tag) {
  const m = /^v(\d{8})(?:\.(\d+))?$/.exec(tag);
  if (!m) throw new Error(`not a release tag: ${JSON.stringify(tag)} (expected vYYYYMMDD or vYYYYMMDD.N)`);
  return `${m[1]}.${Number(m[2] ?? 0)}.0`;
}

const REPOSITORY = { type: "git", url: "git+https://github.com/memorylake-ai/memorylake-cli.git" };
const HOMEPAGE = "https://github.com/memorylake-ai/memorylake-cli#readme";

function writeJson(path, value) {
  writeFileSync(path, JSON.stringify(value, null, 2) + "\n");
}

export function buildPackages({ tag, binaries, out }) {
  const version = npmVersion(tag);
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  const order = [];

  for (const p of PLATFORMS) {
    const src = join(binaries, `memorylake-${tag}-${p.target}`, p.exe);
    if (!existsSync(src)) throw new Error(`missing binary for ${p.target}: ${src}`);
    const dir = join(out, `cli-${p.os}-${p.cpu}`);
    mkdirSync(join(dir, "bin"), { recursive: true });
    copyFileSync(src, join(dir, "bin", p.exe));
    chmodSync(join(dir, "bin", p.exe), 0o755);
    writeJson(join(dir, "package.json"), {
      name: p.name,
      version,
      description: `The MemoryLake CLI binary for ${p.os} ${p.cpu}. Install ${MAIN_PACKAGE} instead.`,
      license: "MIT",
      repository: REPOSITORY,
      homepage: HOMEPAGE,
      os: [p.os],
      cpu: [p.cpu],
      ...(p.libc ? { libc: [p.libc] } : {}),
      files: ["bin"],
      preferUnplugged: true,
    });
    order.push(dir);
  }

  const template = join(here, "memorylake");
  const dir = join(out, "cli");
  mkdirSync(join(dir, "bin"), { recursive: true });
  const manifest = JSON.parse(readFileSync(join(template, "package.json"), "utf8"));
  manifest.version = version;
  manifest.optionalDependencies = Object.fromEntries(PLATFORMS.map((p) => [p.name, version]));
  writeJson(join(dir, "package.json"), manifest);
  copyFileSync(join(template, "bin", "memorylake.js"), join(dir, "bin", "memorylake.js"));
  chmodSync(join(dir, "bin", "memorylake.js"), 0o755);
  copyFileSync(join(template, "README.md"), join(dir, "README.md"));
  copyFileSync(join(here, "..", "LICENSE"), join(dir, "LICENSE"));
  order.push(dir);

  writeFileSync(join(out, "publish-order.txt"), order.join("\n") + "\n");
  return { version, order };
}

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i]?.replace(/^--/, "");
    if (!["tag", "binaries", "out"].includes(key) || argv[i + 1] === undefined) {
      throw new Error("usage: build-packages.mjs --tag vYYYYMMDD --binaries <dir> --out <dir>");
    }
    args[key] = argv[i + 1];
  }
  for (const key of ["tag", "binaries", "out"]) if (!args[key]) throw new Error(`--${key} is required`);
  return args;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    const { version, order } = buildPackages(parseArgs(process.argv.slice(2)));
    console.log(`built ${order.length} packages at ${version}`);
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}
