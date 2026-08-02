import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Accessibility,
  AlertCircle,
  AlertTriangle,
  Boxes,
  CheckCircle2,
  Database,
  Eye,
  EyeOff,
  FileSearch,
  FileWarning,
  FolderOpen,
  Info,
  Link2Off,
  Loader2,
  Palette,
  ShieldAlert,
  Type
} from "lucide-react";
import { useI18n } from "./I18nProvider";
import type { Translate, TranslationKey } from "./i18n";
import { localisePdfJobFailure } from "./pdfJobs";
import { PdfJobProgress } from "./PdfJobProgress";
import { usePdfJob } from "./usePdfJob";

type HealthStudioProps = {
  desktopMode: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
};

type FindingSeverity = "danger" | "info" | "warning";
type FindingCategory =
  | "accessibility"
  | "colour"
  | "document"
  | "fonts"
  | "pages"
  | "privacy"
  | "security"
  | "structure";
type HealthStatus = "attention" | "healthy" | "risk";

type HealthFinding = {
  category: FindingCategory;
  code: string;
  detail: string;
  pageNumber?: number;
  severity: FindingSeverity;
  title: string;
};

type PdfHealthResult = {
  accessibility: PdfAccessibilitySummary;
  blankPages: number[];
  dangerCount: number;
  duplicateGroups: number[][];
  fileName: string;
  fileSize: number;
  findings: HealthFinding[];
  infoCount: number;
  pageCount: number;
  pdfVersion: string;
  status: HealthStatus;
  technical: PdfTechnicalSummary;
  warningCount: number;
};

type PdfTechnicalSummary = {
  brokenReferenceCount: number;
  colourIssueCount: number;
  embeddedFontCount: number;
  fontCount: number;
  fontsMissingUnicodeMap: number;
  formContentErrorCount: number;
  formResourceErrorCount: number;
  formXobjectCount: number;
  iccProfileCount: number;
  invalidIccProfileCount: number;
  indirectObjectCount: number;
  missingResourceCount: number;
  outputIntentCount: number;
  pageContentErrorCount: number;
  pagesUsingDeviceCmyk: number[];
  unembeddedFontCount: number;
};

type PdfAccessibilitySummary = {
  defaultLanguage: string | null;
  displaysDocumentTitle: boolean;
  figureCount: number;
  figuresMissingAltText: number;
  interactivePagesWithoutStructuredTabOrder: number[];
  markedAsTagged: boolean;
  pagesWithStructureParents: number;
  structureElementCount: number;
  structureTreePresent: boolean;
  title: string | null;
};

type AccessibilityItem = {
  label: string;
  state: "ok" | "review";
  value: string;
};

type TechnicalItem = {
  detail: string;
  icon: typeof Database;
  label: string;
  state: "ok" | "review" | "risk";
  value: string;
};

const severityOrder: Record<FindingSeverity, number> = { danger: 0, warning: 1, info: 2 };
const categoryOrder: FindingCategory[] = [
  "security",
  "structure",
  "fonts",
  "colour",
  "privacy",
  "pages",
  "accessibility",
  "document"
];

export function HealthStudio({
  desktopMode,
  initialSourcePassword,
  initialSourcePath
}: HealthStudioProps) {
  const { formatNumber, locale, t } = useI18n();
  const [sourcePath, setSourcePath] = useState<string | null>(initialSourcePath ?? null);
  const [password, setPassword] = useState(initialSourcePassword ?? "");
  const [showPassword, setShowPassword] = useState(false);
  const [cancelBusy, setCancelBusy] = useState(false);
  const [jobNotice, setJobNotice] = useState<string | null>(null);
  const [result, setResult] = useState<PdfHealthResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const healthJob = usePdfJob<PdfHealthResult>(desktopMode, "health");
  const busy = healthJob.isActive;
  const findingGroups = useMemo(
    () => (result ? buildFindingGroups(result.findings, locale) : []),
    [locale, result]
  );
  const accessibilityItems = useMemo(
    () =>
      result
        ? buildAccessibilityItems(result.accessibility, result.pageCount, t, formatNumber)
        : [],
    [formatNumber, result, t]
  );
  const technicalItems = useMemo(
    () => (result ? buildTechnicalItems(result.technical, t, formatNumber) : []),
    [formatNumber, result, t]
  );

  useEffect(() => {
    if (initialSourcePath) {
      setSourcePath(initialSourcePath);
      setPassword(initialSourcePassword ?? "");
      setResult(null);
      setError(null);
      setJobNotice(null);
      healthJob.clearJob();
    }
  }, [initialSourcePassword, initialSourcePath]);

  useEffect(() => {
    const job = healthJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setResult(job.result);
      setJobNotice(null);
      setError(null);
    } else if (job.status === "cancelled") {
      setResult(null);
      setJobNotice(t("health.cancelled"));
      setError(null);
    } else if (job.status === "failed") {
      setResult(null);
      setJobNotice(null);
      setError(localisePdfJobFailure(job, t));
    }
  }, [healthJob.job?.jobId, healthJob.job?.status, t]);

  const chooseSource = async () => {
    setError(null);
    try {
      const selected = await open({
        directory: false,
        filters: [{ name: t("health.dialog.filter"), extensions: ["pdf"] }],
        multiple: false,
        title: t("health.dialog.choose")
      });
      if (typeof selected === "string") {
        setSourcePath(selected);
        setPassword("");
        setResult(null);
        setJobNotice(null);
        healthJob.clearJob();
      }
    } catch {
      setError(t("health.error.choose"));
    }
  };

  const inspectDocument = async () => {
    if (!desktopMode || !sourcePath || busy) {
      return;
    }
    setError(null);
    setJobNotice(null);
    setResult(null);
    try {
      await healthJob.startJob({
        inputPassword: password || null,
        inputPath: sourcePath
      });
    } catch {
      setError(t("health.error.start"));
    }
  };

  const cancelInspection = async () => {
    if (!healthJob.isActive || cancelBusy) {
      return;
    }
    setCancelBusy(true);
    try {
      await healthJob.cancelJob();
    } catch {
      setCancelBusy(false);
      setError(t("health.error.cancel"));
    }
  };

  return (
    <section className="health-studio">
      <div className="health-heading">
        <div>
          <h3>{t("health.heading.title")}</h3>
          <p>{t("health.heading.description")}</p>
        </div>
        <FileSearch size={18} aria-hidden="true" />
      </div>

      <button className="wide-button" disabled={!desktopMode || busy} onClick={chooseSource} type="button">
        <FolderOpen size={17} aria-hidden="true" />
        {sourcePath ? t("health.action.chooseAnother") : t("health.action.choose")}
      </button>

      {sourcePath ? (
        <div className="health-source">
          <FileSearch size={17} aria-hidden="true" />
          <span>
            <strong>{fileNameFromPath(sourcePath)}</strong>
            <small title={sourcePath}>{sourcePath}</small>
          </span>
        </div>
      ) : null}

      <label className="assembly-field">
        {t("health.password.label")}
        <input
          autoComplete="current-password"
          disabled={busy}
          onChange={(event) => {
            setPassword(event.target.value);
            setResult(null);
            setError(null);
            setJobNotice(null);
            healthJob.clearJob();
          }}
          spellCheck={false}
          type={showPassword ? "text" : "password"}
          value={password}
        />
      </label>
      <button className="show-passwords" onClick={() => setShowPassword((value) => !value)} type="button">
        {showPassword ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
        {showPassword ? t("common.hidePassword") : t("common.showPassword")}
      </button>

      <button
        className="primary wide-button"
        disabled={!desktopMode || !sourcePath || busy}
        onClick={inspectDocument}
        type="button"
      >
        {busy ? <Loader2 className="spin" size={17} aria-hidden="true" /> : <FileSearch size={17} aria-hidden="true" />}
        {busy
          ? t("health.action.inspecting")
          : result
            ? t("health.action.inspectAgain")
            : t("health.action.inspect")}
      </button>

      {healthJob.job ? (
        <PdfJobProgress
          cancelling={cancelBusy}
          connectionError={healthJob.connectionError}
          job={healthJob.job}
          onCancel={cancelInspection}
          onRetry={() => void inspectDocument()}
          retryDisabled={!desktopMode || !sourcePath || busy}
        />
      ) : null}

      {jobNotice ? (
        <div className="health-error is-info" role="status">
          <Info size={17} aria-hidden="true" />
          <span>{jobNotice}</span>
        </div>
      ) : null}

      {error ? (
        <div className="health-error" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{error}</span>
        </div>
      ) : null}

      {!healthJob.isActive && healthJob.connectionError ? (
        <div className="health-error" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{t("job.connectionError")}</span>
        </div>
      ) : null}

      {result ? (
        <div className="health-report" aria-live="polite">
          <div className={`health-summary is-${result.status}`}>
            {result.status === "healthy" ? (
              <CheckCircle2 size={21} aria-hidden="true" />
            ) : result.status === "risk" ? (
              <ShieldAlert size={21} aria-hidden="true" />
            ) : (
              <AlertTriangle size={21} aria-hidden="true" />
            )}
            <span>
              <strong>{healthStatusLabel(result.status, t)}</strong>
              <small>
                {t(
                  result.pageCount === 1
                    ? "health.summary.one"
                    : "health.summary.other",
                  {
                    count: formatNumber(result.pageCount),
                    size: formatFileSize(result.fileSize, formatNumber),
                    version: result.pdfVersion
                  }
                )}
              </small>
            </span>
          </div>

          <div className="health-counts" aria-label={t("health.counts.aria")}>
            <span className="is-danger">
              {t(
                result.dangerCount === 1 ? "health.counts.risk.one" : "health.counts.risk.other",
                { count: formatNumber(result.dangerCount) }
              )}
            </span>
            <span className="is-warning">
              {t(
                result.warningCount === 1
                  ? "health.counts.warning.one"
                  : "health.counts.warning.other",
                { count: formatNumber(result.warningCount) }
              )}
            </span>
            <span className="is-info">
              {t(
                result.infoCount === 1 ? "health.counts.note.one" : "health.counts.note.other",
                { count: formatNumber(result.infoCount) }
              )}
            </span>
          </div>

          <section className="health-technical" aria-labelledby="health-technical-title">
            <div className="health-technical-heading">
              <Boxes size={17} aria-hidden="true" />
              <span>
                <strong id="health-technical-title">{t("health.technical.title")}</strong>
                <small>{t("health.technical.description")}</small>
              </span>
            </div>
            <div className="health-technical-grid">
              {technicalItems.map((item) => {
                const ItemIcon = item.icon;
                return (
                  <article className={`is-${item.state}`} key={item.label}>
                    <ItemIcon size={16} aria-hidden="true" />
                    <span>
                      <strong>{item.label}</strong>
                      <b>{item.value}</b>
                      <small>{item.detail}</small>
                    </span>
                  </article>
                );
              })}
            </div>
          </section>

          <section className="health-accessibility" aria-labelledby="health-accessibility-title">
            <div className="health-accessibility-heading">
              <Accessibility size={17} aria-hidden="true" />
              <span>
                <strong id="health-accessibility-title">{t("health.accessibility.title")}</strong>
                <small>{t("health.accessibility.description")}</small>
              </span>
            </div>
            <ul className="health-accessibility-items">
              {accessibilityItems.map((item) => {
                const ItemIcon = item.state === "ok" ? CheckCircle2 : AlertTriangle;
                return (
                  <li className={`is-${item.state}`} key={item.label}>
                    <ItemIcon size={15} aria-hidden="true" />
                    <span>
                      <strong>{item.label}</strong>
                      <small>{item.value}</small>
                    </span>
                  </li>
                );
              })}
            </ul>
            <p>{t("health.accessibility.note")}</p>
          </section>

          {findingGroups.length === 0 ? (
            <div className="health-clear">
              <CheckCircle2 size={17} aria-hidden="true" />
              <span>{t("health.clear")}</span>
            </div>
          ) : (
            <div className="health-finding-groups">
              {findingGroups.map((group) => {
                const CategoryIcon = categoryIcon(group.category);
                return (
                  <section key={group.category}>
                    <header>
                      <CategoryIcon size={16} aria-hidden="true" />
                      <strong>{categoryLabel(group.category, t)}</strong>
                      <span>{formatNumber(group.findings.length)}</span>
                    </header>
                    <ul className="health-findings">
                      {group.findings.map((finding) => {
                        const FindingIcon =
                          finding.severity === "danger"
                            ? ShieldAlert
                            : finding.severity === "warning"
                              ? AlertTriangle
                              : Info;
                        const localisedFinding = localiseHealthFinding(finding, t);
                        return (
                          <li className={`is-${finding.severity}`} key={finding.code}>
                            <FindingIcon size={17} aria-hidden="true" />
                            <span>
                              <strong>
                                {localisedFinding.title}
                                {finding.pageNumber ? (
                                  <em>
                                    {t("health.finding.page", {
                                      page: formatNumber(finding.pageNumber)
                                    })}
                                  </em>
                                ) : null}
                              </strong>
                              <small>{localisedFinding.detail}</small>
                            </span>
                          </li>
                        );
                      })}
                    </ul>
                  </section>
                );
              })}
            </div>
          )}
        </div>
      ) : null}

      <p className="health-note">{t("health.note")}</p>
    </section>
  );
}

function buildAccessibilityItems(
  accessibility: PdfAccessibilitySummary,
  pageCount: number,
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
): AccessibilityItem[] {
  const tagged = accessibility.markedAsTagged && accessibility.structureTreePresent;
  const describedFigures = accessibility.figureCount - accessibility.figuresMissingAltText;
  return [
    {
      label: t("health.accessibility.item.title"),
      state: accessibility.title && accessibility.displaysDocumentTitle ? "ok" : "review",
      value: accessibility.title
        ? accessibility.displaysDocumentTitle
          ? accessibility.title
          : t("health.accessibility.value.filenameMayDisplay", {
              title: accessibility.title
            })
        : t("health.accessibility.value.missing")
    },
    {
      label: t("health.accessibility.item.language"),
      state: accessibility.defaultLanguage ? "ok" : "review",
      value: accessibility.defaultLanguage ?? t("health.accessibility.value.missing")
    },
    {
      label: t("health.accessibility.item.tags"),
      state: tagged && accessibility.structureElementCount > 0 ? "ok" : "review",
      value: tagged
        ? t("health.accessibility.value.tagsLinked", {
            elements: formatNumber(accessibility.structureElementCount),
            linked: formatNumber(accessibility.pagesWithStructureParents),
            pages: formatNumber(pageCount)
          })
        : accessibility.structureTreePresent
          ? t("health.accessibility.value.structureNotTagged")
          : accessibility.markedAsTagged
            ? t("health.accessibility.value.taggedWithoutTree")
            : t("health.accessibility.value.untagged")
    },
    {
      label: t("health.accessibility.item.figures"),
      state: tagged && accessibility.figuresMissingAltText === 0 ? "ok" : "review",
      value:
        !tagged
          ? t("health.accessibility.value.figuresUnavailable")
          : accessibility.figureCount === 0
            ? t("health.accessibility.value.noFigures")
            : t("health.accessibility.value.figuresDescribed", {
                described: formatNumber(describedFigures),
                total: formatNumber(accessibility.figureCount)
              })
    },
    {
      label: t("health.accessibility.item.tabOrder"),
      state:
        accessibility.interactivePagesWithoutStructuredTabOrder.length === 0 ? "ok" : "review",
      value:
        accessibility.interactivePagesWithoutStructuredTabOrder.length === 0
          ? t("health.accessibility.value.noPagesFlagged")
          : t(
              accessibility.interactivePagesWithoutStructuredTabOrder.length === 1
                ? "health.accessibility.value.pagesNeedReview.one"
                : "health.accessibility.value.pagesNeedReview.other",
              {
                count: formatNumber(
                  accessibility.interactivePagesWithoutStructuredTabOrder.length
                )
              }
            )
    }
  ];
}

function buildTechnicalItems(
  technical: PdfTechnicalSummary,
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
): TechnicalItem[] {
  const pageDamage =
    technical.pageContentErrorCount +
    technical.formContentErrorCount +
    technical.formResourceErrorCount +
    technical.missingResourceCount;
  const fontReview = technical.unembeddedFontCount + technical.fontsMissingUnicodeMap;
  return [
    {
      detail:
        technical.brokenReferenceCount === 0
          ? t("health.technical.object.traversed", {
              count: formatNumber(technical.indirectObjectCount)
            })
          : t(
              technical.brokenReferenceCount === 1
                ? "health.technical.object.dangling.one"
                : "health.technical.object.dangling.other",
              { count: formatNumber(technical.brokenReferenceCount) }
            ),
      icon: Database,
      label: t("health.technical.object.label"),
      state: technical.brokenReferenceCount > 0 ? "risk" : "ok",
      value:
        technical.brokenReferenceCount > 0
          ? t("health.technical.object.broken")
          : t("health.technical.object.resolved")
    },
    {
      detail:
        pageDamage === 0
          ? t("health.technical.content.passed", {
              forms: formatNumber(technical.formXobjectCount)
            })
          : t("health.technical.content.issues", {
              formResources: formatNumber(technical.formResourceErrorCount),
              formStreams: formatNumber(technical.formContentErrorCount),
              missing: formatNumber(technical.missingResourceCount),
              pageStreams: formatNumber(technical.pageContentErrorCount)
            }),
      icon: FileWarning,
      label: t("health.technical.content.label"),
      state: pageDamage > 0 ? "risk" : "ok",
      value:
        pageDamage > 0
          ? t("health.technical.content.damaged")
          : t("health.technical.content.readable")
    },
    {
      detail:
        technical.fontCount === 0
          ? t("health.technical.fonts.noneDetected")
          : t("health.technical.fonts.detail", {
              embedded: formatNumber(technical.embeddedFontCount),
              noUnicode: formatNumber(technical.fontsMissingUnicodeMap),
              unembedded: formatNumber(technical.unembeddedFontCount)
            }),
      icon: Type,
      label: t("health.technical.fonts.label"),
      state: fontReview > 0 ? "review" : "ok",
      value:
        technical.fontCount === 0
          ? t("health.technical.fonts.noneDeclared")
          : t("health.technical.fonts.embedded", {
              embedded: formatNumber(technical.embeddedFontCount),
              total: formatNumber(technical.fontCount)
            })
    },
    {
      detail: t("health.technical.colour.detail", {
        cmykPages: formatNumber(technical.pagesUsingDeviceCmyk.length),
        intents: formatNumber(technical.outputIntentCount),
        invalid: formatNumber(technical.invalidIccProfileCount),
        profiles: formatNumber(technical.iccProfileCount)
      }),
      icon: Palette,
      label: t("health.technical.colour.label"),
      state: technical.colourIssueCount > 0 ? "review" : "ok",
      value:
        technical.colourIssueCount > 0
          ? t("health.technical.colour.review")
          : t("health.technical.colour.consistent")
    }
  ];
}

function buildFindingGroups(findings: HealthFinding[], locale: string) {
  return categoryOrder.flatMap((category) => {
    const categoryFindings = findings
      .filter((finding) => finding.category === category)
      .sort(
        (left, right) =>
          severityOrder[left.severity] - severityOrder[right.severity] ||
          (left.pageNumber ?? 0) - (right.pageNumber ?? 0) ||
          left.code.localeCompare(right.code, locale)
      );
    return categoryFindings.length > 0
      ? [{ category, findings: categoryFindings }]
      : [];
  });
}

function categoryLabel(category: FindingCategory, t: Translate) {
  const keys: Record<FindingCategory, TranslationKey> = {
    accessibility: "health.category.accessibility",
    colour: "health.category.colour",
    document: "health.category.document",
    fonts: "health.category.fonts",
    pages: "health.category.pages",
    privacy: "health.category.privacy",
    security: "health.category.security",
    structure: "health.category.structure"
  };
  return t(keys[category]);
}

const healthFindingTitleKeys: Readonly<Record<string, TranslationKey>> = {
  "accessibility-display-title": "health.finding.title.accessibilityDisplayTitle",
  "accessibility-empty-structure": "health.finding.title.accessibilityEmptyStructure",
  "accessibility-figure-alt": "health.finding.title.accessibilityFigureAlt",
  "accessibility-language": "health.finding.title.accessibilityLanguage",
  "accessibility-mark-info": "health.finding.title.accessibilityMarkInfo",
  "accessibility-reading-order-review": "health.finding.title.accessibilityReadingOrder",
  "accessibility-structure-tree": "health.finding.title.accessibilityStructureTree",
  "accessibility-tab-order": "health.finding.title.accessibilityTabOrder",
  "accessibility-title": "health.finding.title.accessibilityTitle",
  "accessibility-untagged": "health.finding.title.accessibilityUntagged",
  attachments: "health.finding.title.attachments",
  "automatic-actions": "health.finding.title.automaticActions",
  bookmarks: "health.finding.title.bookmarks",
  "broken-object-references": "health.finding.title.brokenReferences",
  "certificate-signature": "health.finding.title.certificateSignature",
  "colour-device-cmyk-unmanaged": "health.finding.title.unmanagedCmyk",
  "colour-output-intent-missing": "health.finding.title.outputIntentMissing",
  "colour-profile-invalid": "health.finding.title.invalidColourProfile",
  encrypted: "health.finding.title.encrypted",
  "font-form-inspection-limit": "health.finding.title.fontFormLimit",
  "font-inspection-limit": "health.finding.title.fontLimit",
  "font-invalid-resources": "health.finding.title.invalidFonts",
  "font-simple-unicode-map": "health.finding.title.simpleFontUnicode",
  "font-standard-unembedded": "health.finding.title.standardFontUnembedded",
  "font-unembedded": "health.finding.title.unembeddedFonts",
  "font-unicode-map": "health.finding.title.fontUnicode",
  "form-xobject-inspection-limit": "health.finding.title.formInspectionLimit",
  "form-xobject-resource-cycle": "health.finding.title.formResourceCycle",
  "form-xobject-resources-invalid": "health.finding.title.formResourcesInvalid",
  forms: "health.finding.title.forms",
  "health-finding-limit": "health.finding.title.limit",
  javascript: "health.finding.title.javascript",
  "large-file": "health.finding.title.largeFile",
  "large-object-table": "health.finding.title.largeObjectTable",
  "launch-action": "health.finding.title.launchAction",
  "likely-blank-pages": "health.finding.title.blankPages",
  metadata: "health.finding.title.metadata",
  "mixed-page-sizes": "health.finding.title.mixedPageSizes",
  "named-destinations": "health.finding.title.namedDestinations",
  "resource-nesting-limit": "health.finding.title.resourceNestingLimit",
  "resource-reference-limit": "health.finding.title.resourceReferenceLimit",
  xfa: "health.finding.title.xfa"
};

const healthFindingPrefixKeys: ReadonlyArray<readonly [string, TranslationKey]> = [
  ["unusual-page-", "health.finding.title.unusualPage"],
  ["invalid-page-box-", "health.finding.title.invalidPageBox"],
  ["malformed-page-content-", "health.finding.title.malformedPageContent"],
  ["large-page-stream-", "health.finding.title.largePageStream"],
  ["oversized-image-", "health.finding.title.oversizedImage"],
  ["likely-duplicate-group-", "health.finding.title.duplicatePages"],
  ["resource-page-dictionary-", "health.finding.title.pageResourcesInvalid"],
  ["missing-resource-page-", "health.finding.title.missingPageResource"],
  ["malformed-page-operands-", "health.finding.title.malformedPageOperands"],
  ["malformed-form-content-page-", "health.finding.title.malformedFormContent"],
  ["invalid-form-resources-page-", "health.finding.title.invalidFormResources"]
];

function localiseHealthFinding(finding: HealthFinding, t: Translate) {
  const prefixKey = healthFindingPrefixKeys.find(([prefix]) =>
    finding.code.startsWith(prefix)
  )?.[1];
  const titleKey =
    healthFindingTitleKeys[finding.code] ?? prefixKey ?? "health.finding.title.generic";
  const detailKeys: Record<FindingCategory, TranslationKey> = {
    accessibility: "health.finding.detail.accessibility",
    colour: "health.finding.detail.colour",
    document: "health.finding.detail.document",
    fonts: "health.finding.detail.fonts",
    pages: "health.finding.detail.pages",
    privacy: "health.finding.detail.privacy",
    security: "health.finding.detail.security",
    structure: "health.finding.detail.structure"
  };
  return {
    detail: t(detailKeys[finding.category]),
    title: t(titleKey)
  };
}

function categoryIcon(category: FindingCategory) {
  if (category === "security") return ShieldAlert;
  if (category === "structure") return Link2Off;
  if (category === "fonts") return Type;
  if (category === "colour") return Palette;
  if (category === "accessibility") return Accessibility;
  if (category === "pages") return FileWarning;
  if (category === "privacy") return EyeOff;
  return Database;
}

function healthStatusLabel(status: HealthStatus, t: Translate) {
  if (status === "risk") return t("health.status.risk");
  if (status === "attention") return t("health.status.attention");
  return t("health.status.healthy");
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function formatFileSize(
  bytes: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  if (bytes < 1024) return `${formatNumber(bytes)} B`;
  if (bytes < 1024 * 1024) {
    return `${formatNumber(bytes / 1024, { maximumFractionDigits: 1 })} KB`;
  }
  return `${formatNumber(bytes / (1024 * 1024), { maximumFractionDigits: 1 })} MB`;
}
