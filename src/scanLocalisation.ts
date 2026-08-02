import type { Translate, TranslationKey } from "./i18n";

export type ScannerDiscoveryStatus =
  | "backend-unavailable"
  | "devices-found"
  | "discovery-failed"
  | "no-devices"
  | "unsupported-platform";

type ScannerDiscoveryLike = {
  backendName: string;
  devices: unknown[];
  status: ScannerDiscoveryStatus;
};

type ScanWarningSource = {
  encryption: "AES-256" | "None";
  pagesWithoutSearchableText: number[];
  usedImageMagick: boolean;
  warnings: string[];
};

const presetNameKeys: Readonly<Record<string, TranslationKey>> = {
  a4: "scan.preset.a4.name",
  letter: "scan.preset.letter.name",
  "business-card": "scan.preset.business-card.name",
  "id-card": "scan.preset.id-card.name",
  "driving-licence": "scan.preset.driving-licence.name"
};

const presetDescriptionKeys: Readonly<Record<string, TranslationKey>> = {
  a4: "scan.preset.a4.description",
  letter: "scan.preset.letter.description",
  "business-card": "scan.preset.business-card.description",
  "id-card": "scan.preset.id-card.description",
  "driving-licence": "scan.preset.driving-licence.description"
};

export function localiseScanPresetName(id: string, fallback: string, t: Translate): string {
  const key = presetNameKeys[id];
  return key ? t(key) : fallback;
}

export function localiseScanPresetDescription(
  id: string,
  fallback: string,
  t: Translate
): string {
  const key = presetDescriptionKeys[id];
  return key ? t(key) : fallback;
}

export function describeScannerDiscovery(
  discovery: ScannerDiscoveryLike,
  t: Translate,
  formatNumber: (value: number) => string
): string {
  switch (discovery.status) {
    case "backend-unavailable":
      return t("scanner.discovery.status.backendUnavailable", {
        backend: discovery.backendName
      });
    case "devices-found":
      return t(
        discovery.devices.length === 1
          ? "scanner.discovery.status.devicesFound.one"
          : "scanner.discovery.status.devicesFound.other",
        {
          backend: discovery.backendName,
          count: formatNumber(discovery.devices.length)
        }
      );
    case "discovery-failed":
      return t("scanner.discovery.status.discoveryFailed", {
        backend: discovery.backendName
      });
    case "no-devices":
      return t("scanner.discovery.status.noDevices", { backend: discovery.backendName });
    case "unsupported-platform":
      return t("scanner.discovery.status.unsupportedPlatform");
  }
}

export function localiseScanWarnings(result: ScanWarningSource, t: Translate): string[] {
  const warnings: string[] = [];
  if (result.usedImageMagick) warnings.push(t("scan.warning.imageMagick"));
  if (result.pagesWithoutSearchableText.length > 0) {
    warnings.push(t("scan.warning.searchableText"));
  }
  if (result.warnings.some((warning) => warning.includes("Automatic page boundaries"))) {
    warnings.push(t("scan.warning.boundaries"));
  }
  if (result.encryption === "AES-256") warnings.push(t("scan.warning.encrypted"));
  if (warnings.length === 0 && result.warnings.length > 0) {
    warnings.push(t("scan.warning.generic"));
  }
  return [...new Set(warnings)];
}
