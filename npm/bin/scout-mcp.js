#!/usr/bin/env node
// contextgrep — downloads the platform binary from GitHub Releases and starts
// the MCP server over stdio. All extra args are forwarded to `contextgrep mcp`.
//
// Usage (via npx):
//   npx contextgrep@latest
//   npx contextgrep@latest --index /path/to/index

"use strict";

const { execFileSync, spawnSync } = require("child_process");
const { createWriteStream, chmodSync, existsSync, mkdirSync } = require("fs");
const { join } = require("path");
const { get } = require("https");
const { homedir } = require("os");

// ── Config ────────────────────────────────────────────────────────────────────

const REPO = "ranjanj1/contextgrep"; // e.g. "ranjan/scout"
const VERSION = require("../package.json").version;

// Maps Node.js platform/arch → GitHub Release asset name
const ASSET_MAP = {
  "darwin-arm64":  "contextgrep-macos-arm64",
  "darwin-x64":    "contextgrep-macos-x64",
  "linux-x64":     "contextgrep-linux-x64",
  "win32-x64":     "contextgrep-windows-x64.exe",
};

// ── Resolve binary ────────────────────────────────────────────────────────────

const key = `${process.platform}-${process.arch}`;
const asset = ASSET_MAP[key];

if (!asset) {
  console.error(`contextgrep: unsupported platform: ${key}`);
  console.error(`Supported: ${Object.keys(ASSET_MAP).join(", ")}`);
  process.exit(1);
}

const cacheDir = join(homedir(), ".contextgrep", VERSION);
const binaryName = process.platform === "win32" ? "contextgrep.exe" : "contextgrep";
const binaryPath = join(cacheDir, binaryName);

// ── Download if needed ────────────────────────────────────────────────────────

function download(url, dest, redirects = 5) {
  return new Promise((resolve, reject) => {
    if (redirects === 0) return reject(new Error("Too many redirects"));
    get(url, (res) => {
      if (res.statusCode === 301 || res.statusCode === 302) {
        return resolve(download(res.headers.location, dest, redirects - 1));
      }
      if (res.statusCode !== 200) {
        return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
      }
      const file = createWriteStream(dest);
      res.pipe(file);
      file.on("finish", () => file.close(resolve));
      file.on("error", reject);
    }).on("error", reject);
  });
}

async function ensureBinary() {
  if (existsSync(binaryPath)) return;

  mkdirSync(cacheDir, { recursive: true });

  const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${asset}`;
  process.stderr.write(`contextgrep: downloading ${asset} from GitHub Releases...\n`);

  try {
    await download(url, binaryPath);
  } catch (err) {
    console.error(`contextgrep: download failed: ${err.message}`);
    console.error(`  URL: ${url}`);
    console.error(`  Install manually: https://github.com/${REPO}/releases`);
    process.exit(1);
  }

  chmodSync(binaryPath, 0o755);
  process.stderr.write(`contextgrep: saved to ${binaryPath}\n`);
}

// ── Run ───────────────────────────────────────────────────────────────────────

(async () => {
  await ensureBinary();

  // Forward all user args after the implicit "mcp" subcommand
  // e.g.  npx contextgrep --index /my/index
  //  →    contextgrep mcp --index /my/index
  const args = ["mcp", ...process.argv.slice(2)];

  const result = spawnSync(binaryPath, args, { stdio: "inherit" });

  if (result.error) {
    console.error(`contextgrep: failed to start: ${result.error.message}`);
    process.exit(1);
  }
  process.exit(result.status ?? 0);
})();
