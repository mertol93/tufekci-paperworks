import type { Translate, TranslationKey } from "./i18n";

export type ArchiveAssessment = "independent-validation" | "structural-preflight";
export type ArchiveOutcome =
  | "conforms"
  | "does-not-conform"
  | "preflight-passed"
  | "preflight-failed";
export type ArchiveProfile =
  | "pdfa-1b"
  | "pdfa-2b"
  | "pdfa-3b"
  | "pdfua-1"
  | "pdfua-2"
  | "pdfx-1a-2001"
  | "pdfx-3-2002"
  | "pdfx-4";

type ArchiveRuleFailure = {
  clause?: string | null;
  description?: string | null;
  failedChecks: number;
  specification?: string | null;
  testNumber?: string | null;
};

const profileDescriptionKeys = {
  "pdfa-1b": "archive.profile.description.pdfa1b",
  "pdfa-2b": "archive.profile.description.pdfa2b",
  "pdfa-3b": "archive.profile.description.pdfa3b",
  "pdfua-1": "archive.profile.description.pdfua1",
  "pdfua-2": "archive.profile.description.pdfua2",
  "pdfx-1a-2001": "archive.profile.description.pdfx1a",
  "pdfx-3-2002": "archive.profile.description.pdfx3",
  "pdfx-4": "archive.profile.description.pdfx4"
} as const satisfies Record<ArchiveProfile, TranslationKey>;

const outcomeKeys = {
  conforms: "archive.report.outcome.conforms",
  "does-not-conform": "archive.report.outcome.doesNotConform",
  "preflight-passed": "archive.report.outcome.preflightPassed",
  "preflight-failed": "archive.report.outcome.preflightFailed"
} as const satisfies Record<ArchiveOutcome, TranslationKey>;

const preflightRuleKeys: Readonly<Record<string, TranslationKey>> = {
  declaration: "archive.rule.declaration",
  encryption: "archive.rule.encryption",
  fonts: "archive.rule.fonts",
  forms: "archive.rule.forms",
  "icc-profiles": "archive.rule.iccProfiles",
  javascript: "archive.rule.javascript",
  "object-integrity": "archive.rule.objectIntegrity",
  "output-intent": "archive.rule.outputIntent",
  "page-boxes": "archive.rule.pageBoxes",
  "printable-content": "archive.rule.printableContent",
  "self-contained": "archive.rule.selfContained",
  trapping: "archive.rule.trapping"
};

export function localiseArchiveProfileDescription(profile: ArchiveProfile, t: Translate) {
  return t(profileDescriptionKeys[profile]);
}

export function localiseArchiveOutcome(outcome: ArchiveOutcome, t: Translate) {
  return t(outcomeKeys[outcome]);
}

export function localiseArchiveScope(assessment: ArchiveAssessment, t: Translate) {
  return t(
    assessment === "structural-preflight"
      ? "archive.report.scope.preflight"
      : "archive.report.scope.independent"
  );
}

export function localiseArchiveValidator(
  assessment: ArchiveAssessment,
  version: string | null | undefined,
  t: Translate
) {
  const name = t(
    assessment === "structural-preflight"
      ? "archive.report.validator.builtIn"
      : "archive.report.validator.independent"
  );
  const safeVersion = version?.trim();
  return safeVersion && /^[0-9]+(?:[._+-][0-9A-Za-z]+){0,7}$/u.test(safeVersion)
    ? t("archive.report.validator.version", { name, version: safeVersion })
    : name;
}

export function localiseArchiveRule(
  failure: ArchiveRuleFailure,
  preflight: boolean,
  t: Translate
) {
  if (preflight && failure.testNumber) {
    const key = preflightRuleKeys[failure.testNumber];
    if (key) {
      return t(key);
    }
  }
  if (
    failure.specification === "ISO 19005 PDF/A" &&
    failure.clause === "Document encryption"
  ) {
    return t("archive.rule.encryption");
  }
  const identifier = safeRuleIdentifier(failure);
  return identifier
    ? t("archive.report.ruleWithIdentifier", { identifier })
    : t(preflight ? "archive.report.preflightCheck" : "archive.report.validationRule");
}

export function localiseArchiveWarnings(warnings: string[], t: Translate) {
  const keys: Readonly<Record<string, TranslationKey>> = {
    "PDF/A conversion is a structural rewrite and invalidates existing certificate signatures.":
      "archive.warning.signatureInvalidated",
    "The protected source was validated through a private decrypted copy, but the original cannot conform because PDF/A forbids encryption.":
      "archive.warning.encryptedSource"
  };
  return [
    ...new Set(
      warnings.map((warning) => t(keys[warning] ?? "archive.warning.generic"))
    )
  ];
}

function safeRuleIdentifier(failure: ArchiveRuleFailure) {
  const values = [
    safeRuleNumber(failure.clause),
    safeRuleNumber(failure.testNumber)
  ].filter((value): value is string => Boolean(value));
  return values.length > 0 ? [...new Set(values)].join(" | ") : null;
}

function safeRuleNumber(value: string | null | undefined) {
  const normalised = value?.trim();
  return normalised && /^[0-9]+(?:[.:-][0-9A-Za-z]+){0,12}$/u.test(normalised)
    ? normalised
    : null;
}
