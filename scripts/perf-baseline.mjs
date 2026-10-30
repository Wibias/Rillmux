#!/usr/bin/env node
/** Run the release performance probe several times and report stable medians. */
import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

export function median(values) {
  if (!values.length) throw new Error("median requires at least one value");
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2
    ? sorted[middle]
    : (sorted[middle - 1] + sorted[middle]) / 2;
}

export function summarizeRuns(runs) {
  if (!Array.isArray(runs) || runs.length === 0) {
    throw new Error("at least one performance run is required");
  }
  const metrics = [
    "window_ms",
    "working_set_mb",
    "private_mb",
    "cpu_pct_one_core",
  ];
  for (const run of runs) {
    for (const metric of metrics) {
      if (typeof run?.[metric] !== "number" || !Number.isFinite(run[metric])) {
        throw new Error(`performance run is missing numeric ${metric}`);
      }
    }
  }
  return {
    runs: runs.length,
    budget_pass: runs.every((run) => run.pass === true),
    window_ms: median(runs.map((run) => run.window_ms)),
    working_set_mb: median(runs.map((run) => run.working_set_mb)),
    private_mb: median(runs.map((run) => run.private_mb)),
    cpu_pct_one_core: median(runs.map((run) => run.cpu_pct_one_core)),
  };
}

function option(args, name, fallback) {
  const index = args.indexOf(name);
  return index >= 0 && args[index + 1] ? args[index + 1] : fallback;
}

function runOnce(args) {
  const probe = path.join(root, "scripts", "measure-release-perf.ps1");
  const command = [
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    probe,
    "-Exe",
    option(args, "--exe", "src-tauri/target/release/Rillmux.exe"),
    "-IdleSeconds",
    option(args, "--idle-seconds", "15"),
    "-SampleSeconds",
    option(args, "--sample-seconds", "10"),
  ];
  if (args.includes("--allow-running")) command.push("-AllowRunning");
  try {
    return JSON.parse(
      execFileSync("powershell", command, {
        cwd: root,
        encoding: "utf8",
        timeout: 120_000,
        maxBuffer: 4 * 1024 * 1024,
      }),
    );
  } catch (error) {
    const stdout = String(error?.stdout ?? "").trim();
    if (stdout) return JSON.parse(stdout);
    throw error;
  }
}

function main() {
  const args = process.argv.slice(2);
  const runs = Math.max(1, Number(option(args, "--runs", "3")));
  const measurements = [];
  for (let index = 0; index < runs; index += 1) {
    process.stderr.write(`performance run ${index + 1}/${runs}\n`);
    measurements.push(runOnce(args));
  }
  const summary = { ...summarizeRuns(measurements), measurements };
  const output = `${JSON.stringify(summary, null, 2)}\n`;
  const outputPath = option(args, "--out", null);
  if (outputPath) writeFileSync(path.resolve(root, outputPath), output);
  process.stdout.write(output);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
