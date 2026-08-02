import type { Translate } from "./i18n";

type PlannedPageSummary =
  | {
      kind: "blank";
      paper: string;
      rotation: number;
    }
  | {
      kind: "imported";
      name: string;
      page: number;
      rotation: number;
    }
  | {
      kind: "primary";
      page: number;
      rotation: number;
    };

export function describePlannedPage(page: PlannedPageSummary, t: Translate): string {
  if (page.kind === "blank") {
    return page.rotation
      ? t("organise.pageSummary.blankRotated", {
          paper: page.paper,
          rotation: page.rotation
        })
      : t("organise.pageSummary.blank", { paper: page.paper });
  }
  if (page.kind === "imported") {
    return page.rotation
      ? t("organise.pageSummary.importedRotated", {
          name: page.name,
          page: page.page,
          rotation: page.rotation
        })
      : t("organise.pageSummary.imported", { name: page.name, page: page.page });
  }
  return page.rotation
    ? t("organise.pageSummary.primaryRotated", {
        page: page.page,
        rotation: page.rotation
      })
    : t("organise.pageSummary.primary", { page: page.page });
}

export function localiseOrganiseWarnings(warnings: string[], t: Translate): string[] {
  return unique(warnings.map((warning) => localiseOrganiseWarning(warning, t)));
}

function localiseOrganiseWarning(warning: string, t: Translate): string {
  if (warning === "The source was encrypted. The exported copy uses the new signing passwords and permissions.") {
    return t("organise.warning.sourceReprotected");
  }
  if (warning === "The source was encrypted. This organised copy is not password-protected.") {
    return t("organise.warning.sourceUnprotected");
  }
  if (warning === "Existing certificate signatures are invalidated by structural PDF export.") {
    return t("organise.warning.certificate");
  }
  if (warning === "This PDF contains form fields. Check their appearance and behaviour in the exported copy.") {
    return t("organise.warning.forms");
  }
  if (warning === "This PDF contains bookmarks. Check their destinations in the exported copy.") {
    return t("organise.warning.bookmarks");
  }
  if (warning === "The visual signature is flattened into the selected page, but it is not a certificate-backed digital signature.") {
    return t("organise.warning.visualSignature");
  }
  if (warning === "AES-256 reader permissions restrict changes, but permissions are advisory and do not provide cryptographic tamper evidence.") {
    return t("organise.warning.readerPermissions");
  }

  const importedPatterns: Array<[RegExp, Parameters<Translate>[0]]> = [
    [/^Imported pages from (.+) use the new output passwords and permissions\.$/u, "organise.warning.importedReprotected"],
    [/^Imported pages from encrypted source (.+) are not password-protected in this copy\.$/u, "organise.warning.importedUnprotected"],
    [/^The certificate signature in imported source (.+) cannot be preserved in the organised copy\.$/u, "organise.warning.importedCertificate"],
    [/^Imported source (.+) contains form fields\. Check their appearance in the organised copy\.$/u, "organise.warning.importedForms"],
    [/^Bookmarks from imported source (.+) are not added to the primary bookmark tree\.$/u, "organise.warning.importedBookmarks"]
  ];
  for (const [pattern, key] of importedPatterns) {
    const match = warning.match(pattern);
    if (match) return t(key, { name: match[1] });
  }
  return t("organise.warning.generic");
}

function unique(values: string[]): string[] {
  return [...new Set(values)];
}
