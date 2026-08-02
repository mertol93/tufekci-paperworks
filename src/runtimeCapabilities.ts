export const RUNTIME_PLATFORMS = [
  "windows",
  "macos",
  "linux",
  "ios",
  "android",
  "other"
] as const;

export type RuntimePlatform = (typeof RUNTIME_PLATFORMS)[number];

export type RuntimeCapabilities = Readonly<{
  platform: RuntimePlatform;
  mobile: boolean;
  nativeFileDialogs: boolean;
  localPdfEditing: boolean;
  localVisualMarks: boolean;
  imageToPdf: boolean;
  externalProcesses: boolean;
  connectedScanning: boolean;
  cameraCapture: boolean;
  searchableOcr: boolean;
  certificateSigning: boolean;
  archivalPdf: boolean;
  passwordProtection: boolean;
  directUpdates: boolean;
  appStoreUpdates: boolean;
}>;

const booleanFields = [
  "mobile",
  "nativeFileDialogs",
  "localPdfEditing",
  "localVisualMarks",
  "imageToPdf",
  "externalProcesses",
  "connectedScanning",
  "cameraCapture",
  "searchableOcr",
  "certificateSigning",
  "archivalPdf",
  "passwordProtection",
  "directUpdates",
  "appStoreUpdates"
] as const satisfies readonly (keyof RuntimeCapabilities)[];
const capabilityFields = new Set<string>(["platform", ...booleanFields]);

export function parseRuntimeCapabilities(value: unknown): RuntimeCapabilities {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("The runtime capability report is invalid.");
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).length !== capabilityFields.size ||
    Object.keys(record).some((field) => !capabilityFields.has(field)) ||
    typeof record.platform !== "string" ||
    !RUNTIME_PLATFORMS.includes(record.platform as RuntimePlatform) ||
    booleanFields.some((field) => typeof record[field] !== "boolean")
  ) {
    throw new Error("The runtime capability report is invalid.");
  }

  const capabilities = record as RuntimeCapabilities;
  const expectedMobile = capabilities.platform === "ios" || capabilities.platform === "android";
  if (
    capabilities.mobile !== expectedMobile ||
    (capabilities.mobile && capabilities.externalProcesses) ||
    (capabilities.mobile && capabilities.connectedScanning) ||
    capabilities.appStoreUpdates !== (capabilities.platform === "ios") ||
    (capabilities.connectedScanning && !capabilities.externalProcesses) ||
    (capabilities.searchableOcr && !capabilities.externalProcesses) ||
    (capabilities.certificateSigning && !capabilities.externalProcesses) ||
    (capabilities.archivalPdf && !capabilities.externalProcesses) ||
    (capabilities.passwordProtection && !capabilities.externalProcesses)
  ) {
    throw new Error("The runtime capability report is inconsistent.");
  }
  return Object.freeze({ ...capabilities });
}

export function isAppleMobileRuntime(
  capabilities: RuntimeCapabilities | null
): boolean {
  return capabilities?.platform === "ios";
}
