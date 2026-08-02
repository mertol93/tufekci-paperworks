import { lstat, readFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const maxCandidateFiles = 4_096;
const maxFileBytes = 5 * 1024 * 1024;
const maxSourceBytes = 50 * 1024 * 1024;
const textExtensions = new Set([
  ".css",
  ".desktop",
  ".editorconfig",
  ".gitattributes",
  ".gitignore",
  ".gitkeep",
  ".html",
  ".json",
  ".lock",
  ".m",
  ".md",
  ".mjs",
  ".plist",
  ".py",
  ".rs",
  ".svg",
  ".toml",
  ".ts",
  ".tsx",
  ".txt",
  ".yaml",
  ".yml"
]);
const binaryExtensions = new Set([".icns", ".ico", ".png", ".ttf"]);
const textDotfiles = new Set([".editorconfig", ".gitattributes", ".gitignore", ".gitkeep"]);
const requiredPaths = [
  ".gitattributes",
  ".gitignore",
  ".github/dependabot.yml",
  ".github/workflows/apple-mobile.yml",
  ".github/workflows/ci.yml",
  ".github/workflows/promote-update.yml",
  ".github/workflows/release.yml",
  "CHANGELOG.md",
  "CONTRIBUTING.md",
  "LICENSE",
  "README.md",
  "SECURITY.md",
  "docs/FEATURE_STATUS.md",
  "docs/E2E_TESTING.md",
  "docs/PRINTING_TESTING.md",
  "docs/RELEASE_PLAN.md",
  "docs/UPDATES.md",
  "e2e/native-shell.e2e.mjs",
  "package-lock.json",
  "package.json",
  "scripts/check-e2e-evidence.mjs",
  "scripts/check-e2e-matrix.mjs",
  "scripts/check-apple-mobile.mjs",
  "scripts/check-apple-mobile-bundle.mjs",
  "scripts/check-production-e2e-boundary.mjs",
  "scripts/e2e-evidence.mjs",
  "scripts/generate-platform-signing-config.mjs",
  "src-tauri/Cargo.lock",
  "src-tauri/Cargo.toml",
  "src-tauri/Info.ios.plist",
  "src-tauri/tauri.conf.json",
  "src-tauri/tauri.e2e.conf.json",
  "src-tauri/tauri.ios.conf.json",
  "src/e2eBridgeDisabled.ts",
  "src/e2eBridgeEnabled.ts",
  "src/PageTransferDialog.tsx",
  "src/pageSelection.ts",
  "src/pageTransfer.ts",
  "src/runtimeCapabilities.ts",
  "src/PrintStudio.tsx",
  "src/print.ts",
  "src/printRenderer.ts",
  "tests/page-selection.test.mjs",
  "tests/page-transfer.test.mjs",
  "tests/print.test.mjs",
  "tests/apple-mobile.test.mjs",
  "tests/runtime-capabilities.test.mjs",
  "wdio.conf.mjs"
];
const prohibitedPrefixes = [
  ".agents/",
  ".codex/",
  ".git/",
  ".idea/",
  ".vscode/",
  "artifacts/",
  "dist/",
  "node_modules/",
  "qa-fixtures/",
  "src-tauri/gen/",
  "src-tauri/target/",
  "target/"
];
const privateExtensions = new Set([".key", ".p12", ".pem", ".pfx"]);
const sensitiveTextPatterns = [
  {
    label: "a private-key block",
    pattern: /-----BEGIN (?:EC |OPENSSH |RSA )?PRIVATE KEY-----/u
  },
  {
    label: "an AWS access key",
    pattern: /\bAKIA[0-9A-Z]{16}\b/u
  },
  {
    label: "a GitHub token",
    pattern: /\b(?:ghp_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{20,})\b/u
  },
  {
    label: "an OpenAI-style secret key",
    pattern: /\bsk-[A-Za-z0-9]{20,}\b/u
  },
  {
    label: "a Slack token",
    pattern: /\bxox[baprs]-[A-Za-z0-9-]{10,}\b/u
  },
  {
    label: "credentials embedded in a URL",
    pattern: /https?:\/\/[^/\s:@]+:[^/\s@]+@/iu
  },
  {
    label: "a personal absolute home path",
    pattern: /(?:[A-Za-z]:\\Users\\[^\\\r\n]+\\|\/Users\/[^/\r\n]+\/|\/home\/[^/\r\n]+\/)/u
  }
];

export function validateCandidatePaths(candidatePaths) {
  if (candidatePaths.length === 0 || candidatePaths.length > maxCandidateFiles) {
    throw new Error(
      `The source tree must contain between 1 and ${maxCandidateFiles.toLocaleString("en-GB")} candidate files.`
    );
  }
  const seen = new Set();
  for (const candidate of candidatePaths) {
    const normalised = normaliseCandidatePath(candidate);
    const key = normalised.normalize("NFC").toLocaleLowerCase("en-GB");
    if (seen.has(key)) {
      throw new Error(`The source tree contains a duplicate path: ${normalised}.`);
    }
    seen.add(key);
    if (
      prohibitedPrefixes.some(
        (prefix) => normalised === prefix.slice(0, -1) || normalised.startsWith(prefix)
      ) ||
      normalised === ".env" ||
      normalised.startsWith(".env.")
    ) {
      throw new Error(`Generated or private path is not allowed in the source tree: ${normalised}.`);
    }
    if (privateExtensions.has(path.posix.extname(normalised).toLocaleLowerCase("en-GB"))) {
      throw new Error(`Private key material is not allowed in the source tree: ${normalised}.`);
    }
  }
  for (const required of requiredPaths) {
    if (!seen.has(required.toLocaleLowerCase("en-GB"))) {
      throw new Error(`Required source file is missing: ${required}.`);
    }
  }
  return candidatePaths.map(normaliseCandidatePath).sort((left, right) =>
    left.localeCompare(right, "en-GB")
  );
}

export function classifySourcePath(candidate) {
  const normalised = normaliseCandidatePath(candidate);
  if (normalised === "LICENSE") {
    return "text";
  }
  if (textDotfiles.has(path.posix.basename(normalised).toLocaleLowerCase("en-GB"))) {
    return "text";
  }
  const extension = path.posix.extname(normalised).toLocaleLowerCase("en-GB");
  if (textExtensions.has(extension)) {
    return "text";
  }
  if (binaryExtensions.has(extension)) {
    return "binary";
  }
  throw new Error(`Unsupported source file type: ${normalised}.`);
}

export function sensitiveTextFinding(text) {
  for (const { label, pattern } of sensitiveTextPatterns) {
    if (pattern.test(text)) {
      return label;
    }
  }
  return null;
}

export async function auditSourceTree(workspace) {
  const candidates = validateCandidatePaths(listGitCandidates(workspace));
  const decoder = new TextDecoder("utf-8", { fatal: true });
  let binaryFiles = 0;
  let sourceBytes = 0;
  let textFiles = 0;

  for (const candidate of candidates) {
    const absolutePath = path.join(workspace, ...candidate.split("/"));
    const metadata = await lstat(absolutePath);
    if (!metadata.isFile() || metadata.isSymbolicLink()) {
      throw new Error(`Source candidates must be ordinary files: ${candidate}.`);
    }
    if (metadata.size === 0 || metadata.size > maxFileBytes) {
      throw new Error(
        `Source file size must be between 1 byte and ${maxFileBytes.toLocaleString("en-GB")} bytes: ${candidate}.`
      );
    }
    sourceBytes += metadata.size;
    if (sourceBytes > maxSourceBytes) {
      throw new Error(
        `The source tree exceeds the ${maxSourceBytes.toLocaleString("en-GB")}-byte limit.`
      );
    }

    if (classifySourcePath(candidate) === "binary") {
      binaryFiles += 1;
      continue;
    }
    textFiles += 1;
    const bytes = await readFile(absolutePath);
    let text;
    try {
      text = decoder.decode(bytes);
    } catch {
      throw new Error(`Source text is not strict UTF-8: ${candidate}.`);
    }
    if (text.charCodeAt(0) === 0xfeff) {
      throw new Error(`Source text must not contain a UTF-8 byte-order mark: ${candidate}.`);
    }
    if (text.includes("\r")) {
      throw new Error(`Source text must use LF line endings: ${candidate}.`);
    }
    const sensitive = sensitiveTextFinding(text);
    if (sensitive) {
      throw new Error(`Source text contains ${sensitive}: ${candidate}.`);
    }
  }

  return {
    binaryFiles,
    candidateFiles: candidates.length,
    sourceBytes,
    textFiles
  };
}

function listGitCandidates(workspace) {
  const result = spawnSync(
    "git",
    ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
    {
      cwd: workspace,
      encoding: "utf8",
      windowsHide: true
    }
  );
  if (result.status !== 0) {
    throw new Error("Git could not enumerate the source candidates.");
  }
  return result.stdout.split("\0").filter(Boolean);
}

function normaliseCandidatePath(candidate) {
  if (typeof candidate !== "string" || !candidate || /[\0\r\n]/u.test(candidate)) {
    throw new Error("Source candidate paths must be non-empty and free of control characters.");
  }
  const normalised = candidate.replaceAll("\\", "/");
  if (
    normalised.startsWith("/") ||
    /^[A-Za-z]:\//u.test(normalised) ||
    normalised.split("/").some((part) => !part || part === "." || part === "..")
  ) {
    throw new Error(`Source candidate path is not safely relative: ${candidate}.`);
  }
  return normalised;
}

async function main() {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const report = await auditSourceTree(workspace);
  process.stdout.write(
    `Source tree audit passed for ${report.candidateFiles} files (${report.textFiles} text, ${report.binaryFiles} binary, ${report.sourceBytes} bytes).\n`
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
