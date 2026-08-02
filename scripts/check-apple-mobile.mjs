import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parse as parseYaml } from "yaml";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const defaultProjectDirectory = resolve(scriptDirectory, "..");

function requireCondition(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function readText(projectDirectory, path) {
  return readFileSync(resolve(projectDirectory, path), "utf8");
}

export function validateAppleMobileConfiguration(
  projectDirectory = defaultProjectDirectory
) {
  const iosConfiguration = JSON.parse(
    readText(projectDirectory, "src-tauri/tauri.ios.conf.json")
  );
  const iosBundle = iosConfiguration.bundle?.iOS;
  requireCondition(iosBundle, "The iOS bundle configuration is missing.");
  requireCondition(
    iosBundle.minimumSystemVersion === "16.0",
    "The supported iOS baseline must remain 16.0."
  );
  requireCondition(
    iosBundle.infoPlist === "Info.ios.plist",
    "The reviewed iOS Info.plist merge is not configured."
  );
  requireCondition(
    !Object.hasOwn(iosBundle, "developmentTeam"),
    "An Apple development-team identifier must not be committed."
  );
  const mobileWindow = iosConfiguration.app?.windows?.[0];
  requireCondition(
    mobileWindow?.minWidth <= 320 && mobileWindow?.minHeight <= 480,
    "The iOS window must allow a compact iPhone viewport."
  );

  const infoPlist = readText(projectDirectory, "src-tauri/Info.ios.plist");
  for (const key of [
    "LSSupportsOpeningDocumentsInPlace",
    "UIApplicationSupportsIndirectInputEvents",
    "UISupportedInterfaceOrientations",
    "UISupportedInterfaceOrientations~ipad"
  ]) {
    requireCondition(
      infoPlist.includes(`<key>${key}</key>`),
      `The reviewed iOS Info.plist is missing ${key}.`
    );
  }
  requireCondition(
    infoPlist.includes("UIInterfaceOrientationPortrait") &&
      infoPlist.includes("UIInterfaceOrientationLandscapeLeft") &&
      infoPlist.includes("UIInterfaceOrientationLandscapeRight"),
    "The iPhone and iPad orientations are incomplete."
  );

  const packageManifest = JSON.parse(readText(projectDirectory, "package.json"));
  const scripts = packageManifest.scripts ?? {};
  requireCondition(
    scripts["mobile:ios:init"] === "tauri ios init --ci",
    "The non-interactive iOS project generator is missing."
  );
  requireCondition(
    /tauri ios build --debug --target aarch64-sim --ci/u.test(
      scripts["mobile:ios:build-simulator"] ?? ""
    ),
    "The arm64 iOS simulator build command is missing."
  );
  requireCondition(
    /--export-method app-store-connect/u.test(scripts["mobile:ios:build"] ?? ""),
    "The App Store Connect export command is missing."
  );

  const cargoManifest = readText(projectDirectory, "src-tauri/Cargo.toml");
  requireCondition(
    cargoManifest.includes(
      "[target.'cfg(not(any(target_os = \"ios\", target_os = \"android\")))'.dependencies]"
    ) && cargoManifest.includes('tauri-plugin-updater = "2.10.0"'),
    "The unsupported self-updater must remain a desktop-only dependency."
  );
  for (const path of [
    "src-tauri/src/archive.rs",
    "src-tauri/src/certificate.rs",
    "src-tauri/src/ocr.rs",
    "src-tauri/src/pdf_tools.rs",
    "src-tauri/src/protection.rs"
  ]) {
    requireCondition(
      readText(projectDirectory, path).includes("current_capabilities"),
      `${path} must gate direct desktop-engine probes on runtime capabilities.`
    );
  }

  const viteConfiguration = readText(projectDirectory, "vite.config.ts");
  requireCondition(
    viteConfiguration.includes("process.env.TAURI_DEV_HOST") &&
      viteConfiguration.includes("port: 5174"),
    "The mobile development host and HMR channel are not configured."
  );
  const scannerBuild = readText(projectDirectory, "scripts/build-macos-scanner.mjs");
  requireCondition(
    scannerBuild.includes('tauriPlatform === "ios"') &&
      scannerBuild.includes('tauriTarget.includes("apple-ios")'),
    "The macOS scanner bridge build must skip iOS targets."
  );

  const index = readText(projectDirectory, "index.html");
  const styles = readText(projectDirectory, "src/styles.css");
  requireCondition(
    index.includes("viewport-fit=cover"),
    "The Apple mobile viewport must opt into reviewed safe-area handling."
  );
  requireCondition(
    styles.includes("env(safe-area-inset-top)") &&
      styles.includes("@media (pointer: coarse)") &&
      styles.includes("100dvh"),
    "Safe areas, dynamic viewports, and coarse-pointer targets are not represented in the interface."
  );

  const workflowText = readText(projectDirectory, ".github/workflows/apple-mobile.yml");
  const workflow = parseYaml(workflowText);
  const simulatorJob = workflow?.jobs?.["ios-simulator"];
  requireCondition(
    simulatorJob?.["runs-on"] === "macos-15",
    "The Apple mobile verification job must run on macOS 15."
  );
  requireCondition(
    simulatorJob?.env?.APPLE_DEVELOPMENT_TEAM === "0000000000" &&
      simulatorJob?.env?.CODE_SIGNING_ALLOWED === "NO",
    "The simulator job must use non-secret, disabled signing settings."
  );
  const workflowCommands = (simulatorJob?.steps ?? [])
    .map((step) => step?.run ?? "")
    .join("\n");
  for (const command of [
    "npm run mobile:ios:init",
    "npm run mobile:ios:build-simulator",
    "npm run release:apple-mobile-bundle"
  ]) {
    requireCondition(
      workflowCommands.includes(command),
      `The Apple mobile workflow is missing ${command}.`
    );
  }
  requireCondition(
    !workflowText.includes("secrets."),
    "Simulator verification must not depend on signing secrets."
  );

  const gitignore = readText(projectDirectory, ".gitignore");
  const sourceCheck = readText(projectDirectory, "scripts/check-source-tree.mjs");
  requireCondition(
    gitignore.includes("src-tauri/gen/") && sourceCheck.includes('"src-tauri/gen/"'),
    "Generated Xcode projects must remain outside the source archive."
  );

  return {
    minimumSystemVersion: iosBundle.minimumSystemVersion,
    simulatorRunner: simulatorJob["runs-on"],
    supportsIPhone: true,
    supportsIPad: true
  };
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === fileURLToPath(import.meta.url)) {
  try {
    const result = validateAppleMobileConfiguration();
    process.stdout.write(
      `Apple mobile source configuration verified for iOS ${result.minimumSystemVersion}+ on iPhone and iPad.\n`
    );
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
