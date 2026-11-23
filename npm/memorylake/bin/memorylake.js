#!/usr/bin/env node
// Runs the MemoryLake CLI binary that npm installed for this machine.
//
// The binary comes in a per-platform package (@memorylake/cli-<os>-<cpu>)
// that npm picks as an optional dependency. This finds it and runs it with the
// same arguments, standard streams and exit status.
"use strict";

const { spawnSync } = require("child_process");

const SCOPE = "@memorylake";
const SUPPORTED = new Set(["darwin arm64", "darwin x64", "linux arm64", "linux x64", "win32 arm64", "win32 x64"]);

function fail(message) {
  process.stderr.write(`memorylake: ${message}\n`);
  process.exit(1);
}

function isMusl() {
  if (process.platform !== "linux") return false;
  try {
    return !process.report.getReport().header.glibcVersionRuntime;
  } catch {
    return false;
  }
}

function binaryPath() {
  const key = `${process.platform} ${process.arch}`;
  if (!SUPPORTED.has(key) || isMusl()) {
    fail(
      `there is no prebuilt binary for ${key}${isMusl() ? " (musl)" : ""}. ` +
        "See https://github.com/memorylake-ai/memorylake-cli#install for other ways to install.",
    );
  }
  const pkg = `${SCOPE}/cli-${process.platform}-${process.arch}`;
  const exe = process.platform === "win32" ? "memorylake.exe" : "memorylake";
  try {
    return require.resolve(`${pkg}/bin/${exe}`);
  } catch {
    fail(
      `${pkg} is not installed, so the binary for this machine is missing. ` +
        "It is an optional dependency: reinstall without --omit=optional / --no-optional, " +
        "for example `npm install -g @memorylake/cli`.",
    );
  }
}

const result = spawnSync(binaryPath(), process.argv.slice(2), { stdio: "inherit", windowsHide: true });
if (result.error) fail(`could not run the binary: ${result.error.message}`);
if (result.signal) {
  // Die of the same signal, so a caller sees what the binary saw.
  process.kill(process.pid, result.signal);
}
process.exit(result.status ?? 1);
