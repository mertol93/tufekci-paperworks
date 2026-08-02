import type { Translate, TranslationKey } from "./i18n";

type CertificateValidationState =
  | "unsigned"
  | "valid"
  | "invalid"
  | "indeterminate"
  | "unavailable";

type CertificateValidationSummary = {
  intact?: boolean | null;
  state: CertificateValidationState;
  trusted?: boolean | null;
};

const exactWarningKeys: Readonly<Record<string, TranslationKey>> = {
  "Cryptographic integrity appears intact, but the signer certificate did not chain to a configured trust root.":
    "certificate.warning.untrustedChain",
  "No trusted timestamp was requested. The signing time may be self-reported rather than independently proven.":
    "certificate.warning.noTimestamp",
  "Structural signature detection does not prove cryptographic integrity or signer identity.":
    "certificate.warning.structuralOnly",
  "The certificate signature is structurally intact, but pyHanko did not establish a trusted certificate chain. Add the appropriate root certificate and validate again before relying on signer identity.":
    "certificate.warning.untrustedIdentity",
  "The pyHanko validation report exceeded the display limit and was truncated.":
    "certificate.warning.reportTruncated"
};

const generatedFieldNames: Readonly<Record<string, TranslationKey>> = {
  "Embedded certificate signature": "certificate.report.field.embedded",
  "Unnamed signature field": "certificate.report.field.unnamed"
};

const privatePath = /(?:[A-Za-z]:[\\/]|\\\\|\/(?:home|private|tmp|users)\/)/iu;

export function localiseCertificateSummary(
  report: CertificateValidationSummary,
  t: Translate
) {
  switch (report.state) {
    case "valid":
      return t("certificate.report.summary.valid");
    case "invalid":
      return t("certificate.report.summary.invalid");
    case "unsigned":
      return t("certificate.report.summary.unsigned");
    case "unavailable":
      return t("certificate.report.summary.unavailable");
    default:
      return report.intact === true && report.trusted === false
        ? t("certificate.report.summary.intactUntrusted")
        : t("certificate.report.summary.indeterminate");
  }
}

export function localiseCertificateWarnings(
  warnings: string[],
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  const localised = warnings.map((warning) => {
    const exactKey = exactWarningKeys[warning];
    if (exactKey) {
      return t(exactKey);
    }

    const fieldLimit = warning.match(
      /^Only the first (\d{1,4}) signature fields are listed because the PDF exceeds the bounded report limit\.$/u
    );
    if (fieldLimit) {
      const count = Number(fieldLimit[1]);
      if (count >= 1 && count <= 512) {
        return t("certificate.warning.fieldLimit", { count: formatNumber(count) });
      }
    }

    return t("certificate.warning.generic");
  });

  return [...new Set(localised)];
}

export function localiseCertificateFieldName(value: string, t: Translate) {
  const generatedKey = generatedFieldNames[value];
  if (generatedKey) {
    return t(generatedKey);
  }
  return safeCertificateDocumentText(value) ?? t("certificate.report.field.unnamed");
}

export function localiseCertificateFieldKind(value: string, t: Translate) {
  if (value === "approval") {
    return t("certificate.report.field.kind.approval");
  }
  if (value === "document-timestamp") {
    return t("certificate.report.field.kind.timestamp");
  }
  return t("certificate.report.field.kind.unknown");
}

export function safeCertificateDocumentText(value?: string | null) {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.length > 1024 ||
    /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F]/u.test(value) ||
    privatePath.test(value)
  ) {
    return null;
  }
  return value.trim() || null;
}

export function localiseCertificateSigningTime(
  value: string | null | undefined,
  t: Translate,
  formatDate: (value: Date | number, options?: Intl.DateTimeFormatOptions) => string
) {
  const match = value?.match(
    /^D:(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})(Z|([+-])(\d{2})'?(\d{2})'?)$/u
  );
  if (!match) {
    return null;
  }

  const [, yearText, monthText, dayText, hourText, minuteText, secondText, zone, sign, zoneHourText, zoneMinuteText] =
    match;
  const year = Number(yearText);
  const month = Number(monthText);
  const day = Number(dayText);
  const hour = Number(hourText);
  const minute = Number(minuteText);
  const second = Number(secondText);
  const zoneHour = Number(zoneHourText ?? 0);
  const zoneMinute = Number(zoneMinuteText ?? 0);
  if (
    month < 1 ||
    month > 12 ||
    day < 1 ||
    day > 31 ||
    hour > 23 ||
    minute > 59 ||
    second > 59 ||
    zoneHour > 23 ||
    zoneMinute > 59
  ) {
    return null;
  }

  const wallTime = Date.UTC(year, month - 1, day, hour, minute, second);
  const wallDate = new Date(wallTime);
  if (
    wallDate.getUTCFullYear() !== year ||
    wallDate.getUTCMonth() !== month - 1 ||
    wallDate.getUTCDate() !== day ||
    wallDate.getUTCHours() !== hour ||
    wallDate.getUTCMinutes() !== minute ||
    wallDate.getUTCSeconds() !== second
  ) {
    return null;
  }

  const offsetMinutes = zone === "Z" ? 0 : (zoneHour * 60 + zoneMinute) * (sign === "+" ? 1 : -1);
  const instant = wallTime - offsetMinutes * 60_000;
  return t("certificate.report.field.signingTime", {
    time: formatDate(instant, {
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      month: "short",
      timeZone: "UTC",
      timeZoneName: "short",
      year: "numeric"
    })
  });
}
