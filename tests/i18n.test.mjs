import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  catalogues,
  DEFAULT_LOCALE,
  formatList,
  formatNumber,
  LOCALE_STORAGE_KEY,
  resolveSupportedLocale,
  SUPPORTED_LOCALES,
  translate,
  translationPlaceholders
} from "../src/i18n.ts";
import {
  localiseAnnotationKind,
  localiseAnnotationWarnings
} from "../src/annotationLocalisation.ts";
import {
  localiseFormDraftError,
  localiseFormKind,
  localiseFormWarnings
} from "../src/formLocalisation.ts";
import { localiseContentWarnings } from "../src/contentEditLocalisation.ts";
import {
  localiseArchiveOutcome,
  localiseArchiveProfileDescription,
  localiseArchiveRule,
  localiseArchiveScope,
  localiseArchiveValidator,
  localiseArchiveWarnings
} from "../src/archiveLocalisation.ts";
import {
  batchRecipeExceptionKey,
  localiseBatchInputOrigin,
  localiseBatchInspectionError,
  localiseBatchNote,
  localiseBatchRecipeDescription,
  localiseBatchRecipeName,
  localiseBatchRecipeSettingsError,
  localiseBatchSkippedReason,
  localiseBatchSteps,
  localiseBatchWarnings
} from "../src/batchLocalisation.ts";
import { BUILT_IN_BATCH_RECIPES } from "../src/batchRecipes.ts";
import {
  bookmarkPdfOpeningErrorKey,
  localiseBookmarkWarnings,
  localisePrintedContentsValidation
} from "../src/bookmarkLocalisation.ts";
import {
  localiseCertificateFieldKind,
  localiseCertificateFieldName,
  localiseCertificateSigningTime,
  localiseCertificateSummary,
  localiseCertificateWarnings,
  safeCertificateDocumentText
} from "../src/certificateLocalisation.ts";
import {
  describeOcrReadiness,
  localiseOcrLanguage,
  localiseOcrReviewWarnings,
  localiseSearchableOcrWarnings
} from "../src/ocrLocalisation.ts";
import {
  describePlannedPage,
  localiseOrganiseWarnings
} from "../src/organiseLocalisation.ts";
import {
  localiseFinishPaperName,
  localiseFinishRangeError,
  localisePageFinishWarnings
} from "../src/pageFinishLocalisation.ts";
import {
  localiseRedactionSearchError,
  localiseRedactionWarnings
} from "../src/redactionLocalisation.ts";
import { localisePdfJobFailure, localisePdfJobStage } from "../src/pdfJobs.ts";
import {
  describeScannerDiscovery,
  localiseScanPresetDescription,
  localiseScanPresetName,
  localiseScanWarnings
} from "../src/scanLocalisation.ts";

test("publishes the four exact release locales with British English as the default", () => {
  assert.deepEqual(SUPPORTED_LOCALES, ["en-GB", "en-US", "tr-TR", "de-DE"]);
  assert.equal(DEFAULT_LOCALE, "en-GB");
  assert.match(LOCALE_STORAGE_KEY, /^tufekci-paperworks\.[a-z.-]+\.v1$/u);
  assert.equal(resolveSupportedLocale("EN-us"), "en-US");
  assert.equal(resolveSupportedLocale("tr-TR"), "tr-TR");
  assert.equal(resolveSupportedLocale("fr-FR"), "en-GB");
  assert.equal(resolveSupportedLocale(null), "en-GB");
});

test("keeps every catalogue complete, non-empty, and placeholder-compatible", () => {
  const canonicalKeys = Object.keys(catalogues[DEFAULT_LOCALE]).sort();
  assert.ok(canonicalKeys.length >= 150);

  for (const locale of SUPPORTED_LOCALES) {
    const catalogue = catalogues[locale];
    assert.deepEqual(Object.keys(catalogue).sort(), canonicalKeys, `${locale} key set`);
    for (const key of canonicalKeys) {
      const value = catalogue[key];
      assert.equal(typeof value, "string", `${locale}:${key}`);
      assert.ok(value.trim().length > 0, `${locale}:${key} is blank`);
      assert.doesNotMatch(value, /(?:Ã.|�)/u, `${locale}:${key} contains broken UTF-8`);
      assert.deepEqual(
        translationPlaceholders(value),
        translationPlaceholders(catalogues[DEFAULT_LOCALE][key]),
        `${locale}:${key} placeholders`
      );
    }
  }
});

test("interpolates bounded values and preserves locale-specific product language", () => {
  assert.equal(
    translate("en-GB", "merge.drag.aria", { name: "source.pdf" }),
    "Drag source.pdf to reorder"
  );
  assert.equal(translate("en-GB", "workflow.organise.title"), "Organise Pages");
  assert.equal(translate("en-US", "workflow.organise.title"), "Organize Pages");
  assert.equal(
    translate("de-DE", "organise.actions.selection", { count: 3, current: 2 }),
    "3 Seiten ausgewählt, Seite 2 aktiv"
  );
  assert.equal(
    translate("tr-TR", "organise.selection.other", { count: 3 }),
    "3 sayfa seçili"
  );
  assert.equal(translate("tr-TR", "workflow.ocr.title"), "Metni Tanı");
  assert.equal(translate("de-DE", "workflow.ocr.title"), "Text erkennen");
  assert.equal(formatNumber("en-GB", 1234.5), "1,234.5");
  assert.equal(formatNumber("de-DE", 1234.5), "1.234,5");
  assert.equal(formatList("en-GB", ["crop", "deskew"]), "crop and deskew");
  assert.equal(formatList("de-DE", ["Zuschneiden", "Begradigen"]), "Zuschneiden und Begradigen");
});

test("localises OCR, scan, scanner, and stable native job outcomes", () => {
  const german = (key, values) => translate("de-DE", key, values);
  const turkish = (key, values) => translate("tr-TR", key, values);
  const job = {
    createdAtMs: 1,
    error: "Private scanner driver details must not be displayed",
    errorCode: "scanner-capture-failed",
    jobId: "scanner-capture-1-1",
    kind: "scanner-capture",
    progress: 54,
    result: null,
    stage: "Receiving private scanner page 1",
    stageCode: "scanner-capture-capturing",
    status: "failed",
    updatedAtMs: 2
  };

  assert.equal(
    localisePdfJobStage(job, turkish),
    translate("tr-TR", "job.stage.scannerCaptureCapturing")
  );
  assert.equal(
    localisePdfJobFailure(job, german),
    "Der angeschlossene Scanner konnte die angeforderten Seiten nicht erfassen. Gerät und Einstellungen prüfen und erneut versuchen."
  );
  assert.doesNotMatch(localisePdfJobFailure(job, german), /Private|driver/u);
  assert.equal(localiseOcrLanguage("eng", "English", german), "Englisch");
  assert.equal(localiseOcrLanguage("eng+tur", "", turkish), "İngilizce + Türkçe");
  assert.equal(
    describeOcrReadiness(
      {
        languageAvailable: false,
        ocrMyPdf: { available: true },
        ready: false,
        tesseract: { available: true }
      },
      german
    ),
    translate("de-DE", "ocr.engine.languageMissing")
  );
  assert.equal(
    describeScannerDiscovery(
      {
        backendName: "SANE",
        devices: [{}, {}],
        status: "devices-found"
      },
      turkish,
      (value) => formatNumber("tr-TR", value)
    ),
    "SANE üzerinden 2 bağlı tarayıcı bulundu."
  );
  assert.equal(localiseScanPresetName("driving-licence", "", german), "Führerschein");
  assert.equal(
    localiseScanPresetDescription("a4", "", turkish),
    translate("tr-TR", "scan.preset.a4.description")
  );
  assert.deepEqual(
    localiseSearchableOcrWarnings(
      ["OCR completed, but pages do not contain searchable text."],
      german
    ),
    [translate("de-DE", "ocr.warning.pagesWithoutText")]
  );
  assert.deepEqual(
    localiseOcrReviewWarnings(
      { lowConfidenceCount: 3, lowConfidenceWords: [{}], malformedRows: 1, wordCount: 4 },
      turkish
    ),
    [
      translate("tr-TR", "ocrReview.warning.malformed.one", { count: 1 }),
      translate("tr-TR", "ocrReview.warning.truncated", { count: 1 })
    ]
  );
  assert.deepEqual(
    localiseScanWarnings(
      {
        encryption: "AES-256",
        pagesWithoutSearchableText: [2],
        usedImageMagick: true,
        warnings: []
      },
      german
    ),
    [
      translate("de-DE", "scan.warning.imageMagick"),
      translate("de-DE", "scan.warning.searchableText"),
      translate("de-DE", "scan.warning.encrypted")
    ]
  );
});

test("localises organiser page plans, native stages, failures, and export warnings", () => {
  const german = (key, values) => translate("de-DE", key, values);
  const turkish = (key, values) => translate("tr-TR", key, values);
  const organiserJob = {
    createdAtMs: 1,
    error: "Private native organiser failure with C:\\Users\\person\\source.pdf",
    errorCode: null,
    jobId: "organise-1-1",
    kind: "organise",
    progress: 65,
    result: null,
    stage: "Private source-page details",
    stageCode: "organise-arranging",
    status: "failed",
    updatedAtMs: 2
  };

  assert.equal(
    localisePdfJobStage(organiserJob, turkish),
    translate("tr-TR", "job.stage.organiseArranging")
  );
  assert.equal(
    localisePdfJobFailure(organiserJob, german),
    translate("de-DE", "organise.export.failed")
  );
  assert.doesNotMatch(localisePdfJobFailure(organiserJob, german), /Private|Users|source\.pdf/u);
  assert.equal(
    describePlannedPage(
      { kind: "imported", name: "quelle.pdf", page: 2, rotation: 90 },
      german
    ),
    "Importierte Seite 2 aus quelle.pdf, um 90° gedreht"
  );
  assert.equal(
    describePlannedPage({ kind: "blank", paper: "A4", rotation: 0 }, turkish),
    "Yeni boş A4 sayfa"
  );
  assert.deepEqual(
    localiseOrganiseWarnings(
      [
        "Imported pages from encrypted source archive.pdf are not password-protected in this copy.",
        "Unknown backend detail containing C:\\Private\\document.pdf",
        "Unknown backend detail containing C:\\Private\\document.pdf"
      ],
      german
    ),
    [
      translate("de-DE", "organise.warning.importedUnprotected", { name: "archive.pdf" }),
      translate("de-DE", "organise.warning.generic")
    ]
  );
});

test("wires locale state into the application shell and migrated safety workflows", async () => {
  const [main, provider, app, merge, protection, safety, progress] = await Promise.all([
    readFile(new URL("../src/main.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/I18nProvider.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/App.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/MergeStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/OutputProtectionFields.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/PdfEditSafetyNotice.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/PdfJobProgress.tsx", import.meta.url), "utf8")
  ]);

  assert.match(main, /<I18nProvider>/u);
  assert.match(provider, /document\.documentElement\.lang = locale/u);
  assert.match(provider, /window\.localStorage\.setItem\(LOCALE_STORAGE_KEY, locale\)/u);
  assert.match(app, /SUPPORTED_LOCALES\.map/u);
  assert.match(app, /aria-label=\{t\("locale\.selector\.label"\)\}/u);
  assert.match(app, /describePlannedPage\(/u);
  assert.match(app, /localiseOrganiseWarnings\(job\.result\.warnings, t\)/u);
  assert.match(app, /localisePdfJobStage\(scanJob, t\)/u);
  assert.match(app, /t\("organise\.actions\.title"\)/u);
  assert.match(app, /t\("app\.drop\.title"\)/u);
  assert.match(app, /t\("safety\.organiser\.signatureTitle"\)/u);
  assert.match(app, /\[activeWorkflowId, workflows\]/u);
  assert.match(merge, /t\("merge\.navigation\.title"\)/u);
  assert.match(protection, /t\("protection\.validation"\)/u);
  assert.match(safety, /t\("safety\.acknowledgement"\)/u);
  assert.match(progress, /t\("job\.diagnostic\.title"\)/u);
  assert.doesNotMatch(
    app,
    /PDF\/A engine readiness could not be checked|Job Status Unavailable|Creating Scan PDF|Page Actions|Drop a PDF or images here|Imported page preview unavailable|Reading document data\.\.\.|Blank PDF page/u
  );
  assert.doesNotMatch(app, /job\.error \|\||pdfError \?\?/u);

  for (const [name, source] of [
    ["merge", merge],
    ["protection", protection],
    ["safety", safety],
    ["progress", progress]
  ]) {
    assert.doesNotMatch(
      source,
      /Preserve source bookmarks|Protect this output|Checking document trust|Diagnostic details/u,
      `${name} retains migrated canonical copy`
    );
  }
});

test("localises the complete visual-signature creation, placement, and vault surface", async () => {
  const [studio, drawPad, layer, vault] = await Promise.all([
    readFile(new URL("../src/SignatureStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/SignatureDrawPad.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/VisualSignatureLayer.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/SignatureVault.tsx", import.meta.url), "utf8")
  ]);

  assert.equal(translate("en-GB", "signature.ink.legend"), "Ink colour");
  assert.equal(translate("en-US", "signature.ink.legend"), "Ink color");
  assert.equal(translate("tr-TR", "signature.method.draw"), "Çiz");
  assert.equal(translate("de-DE", "signature.method.draw"), "Zeichnen");
  assert.match(studio, /localiseArtworkError/u);
  assert.match(studio, /t\("signature\.security\.visualNote"\)/u);
  assert.match(drawPad, /t\("signature\.draw\.aria"\)/u);
  assert.match(layer, /t\("signature\.layer\.placementAria"/u);
  assert.match(vault, /t\("signature\.vault\.note"\)/u);

  for (const [name, source] of [
    ["studio", studio],
    ["draw pad", drawPad],
    ["placement layer", layer],
    ["vault", vault]
  ]) {
    assert.doesNotMatch(
      source,
      /Visual Signatures and Initials|Add to This Session|Draw a visual signature|Encrypted visual-mark library/u,
      `${name} retains migrated canonical copy`
    );
  }
});

test("localises the complete searchable OCR and scanner intake surfaces", async () => {
  const [app, ocr, review] = await Promise.all([
    readFile(new URL("../src/App.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/SearchableOcrStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/OcrReviewDialog.tsx", import.meta.url), "utf8")
  ]);

  assert.match(app, /describeScannerDiscovery\(scannerDiscovery, t, formatNumber\)/u);
  assert.match(app, /localiseScanPresetName\(preset\.id, preset\.name, t\)/u);
  assert.match(app, /t\("scan\.settings\.title"\)/u);
  assert.match(app, /t\("scanner\.connected\.title"\)/u);
  assert.match(ocr, /localiseSearchableOcrWarnings/u);
  assert.match(ocr, /localisePdfJobFailure/u);
  assert.match(ocr, /t\("ocr\.heading\.title"\)/u);
  assert.match(review, /localiseOcrReviewWarnings/u);
  assert.match(review, /t\("ocrReview\.title"\)/u);

  for (const [name, source] of [
    ["scan and scanner", app],
    ["searchable OCR", ocr],
    ["OCR review", review]
  ]) {
    assert.doesNotMatch(
      source,
      />\s*(?:Connected scanner|Recognise Text|Review OCR Confidence|Scan settings)\s*</u,
      `${name} retains migrated canonical copy`
    );
  }
});

test("localises the complete print preparation and system-dialogue surface", async () => {
  const [app, printStudio] = await Promise.all([
    readFile(new URL("../src/App.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/PrintStudio.tsx", import.meta.url), "utf8")
  ]);

  assert.equal(translate("en-US", "workflow.print.stage"), "System dialog");
  assert.equal(translate("tr-TR", "workflow.print.title"), "Yazdır");
  assert.equal(translate("de-DE", "workflow.print.title"), "Drucken");
  assert.match(app, /id: "print"/u);
  assert.match(app, /event\.key\.toLocaleLowerCase\("en-GB"\) !== "p"/u);
  assert.match(printStudio, /useI18n\(\)/u);
  assert.match(printStudio, /t\("print\.system\.description"\)/u);
  assert.match(printStudio, /t\("print\.preview\.aria"\)/u);
  assert.match(printStudio, /aria-invalid=\{Boolean\(visibleRangeError\)\}/u);
  assert.doesNotMatch(
    printStudio,
    />\s*(?:Print|Pages|Current page|Custom range|System Print Settings)\s*</u
  );
});

test("localises page import and the complete cross-document transfer surface", async () => {
  const [app, importPages, transfer] = await Promise.all([
    readFile(new URL("../src/App.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/ImportPagesDialog.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/PageTransferDialog.tsx", import.meta.url), "utf8")
  ]);

  assert.equal(translate("en-GB", "organise.actions.transfer"), "Copy or Move");
  assert.equal(translate("en-US", "transfer.eyebrow"), "Organize Pages");
  assert.equal(translate("tr-TR", "transfer.mode.move"), "Sayfaları taşı");
  assert.equal(translate("de-DE", "transfer.mode.move"), "Seiten verschieben");
  assert.match(app, /t\("organise\.actions\.transfer"\)/u);
  assert.match(importPages, /t\("importPages\.title"\)/u);
  assert.match(importPages, /t\("importPages\.certificate\.acknowledgement"\)/u);
  assert.match(transfer, /t\("transfer\.title"\)/u);
  assert.match(transfer, /t\("transfer\.certificate\.destination"\)/u);
  assert.match(transfer, /t\("transfer\.source\.drag\.aria"/u);
  assert.doesNotMatch(
    transfer,
    />\s*(?:Move or Copy Pages Between PDFs|Choose the receiving PDF|Publish and Move)\s*</u
  );
  assert.doesNotMatch(
    importPages,
    />\s*(?:Import Pages from Another PDF|Review Pages|Choose PDF|Opening password)\s*</u
  );
});

test("localises split, protection, compression, activity, and signed updates", async () => {
  const [split, protection, compression, activity, updates] = await Promise.all([
    readFile(new URL("../src/SplitStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/ProtectionStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/CompressionStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/OperationAuditDialog.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/UpdateDialog.tsx", import.meta.url), "utf8")
  ]);
  const german = (key, values) => translate("de-DE", key, values);
  const turkish = (key, values) => translate("tr-TR", key, values);

  assert.equal(translate("en-GB", "split.heading.title"), "Split or Extract Pages");
  assert.equal(translate("de-DE", "protect.mode.add"), "Passwort hinzufügen");
  assert.equal(translate("tr-TR", "compression.heading.title"), "PDF'yi Sıkıştır");
  assert.equal(translate("de-DE", "activity.title"), "Vorgangsverlauf");
  assert.equal(translate("tr-TR", "update.channel.stable"), "Kararlı");

  const privateCompressionJob = {
    createdAtMs: 1,
    error: "Private native failure containing C:\\Users\\person\\document.pdf",
    errorCode: null,
    jobId: "compression-1-1",
    kind: "compression",
    progress: 44,
    result: null,
    stage: "Private embedded-image detail",
    stageCode: "compression-analysing",
    status: "failed",
    updatedAtMs: 2
  };
  assert.equal(
    localisePdfJobStage(privateCompressionJob, german),
    translate("de-DE", "job.stage.compressionAnalysing")
  );
  assert.equal(
    localisePdfJobFailure(privateCompressionJob, turkish),
    translate("tr-TR", "compression.error.exportFailed")
  );
  assert.doesNotMatch(
    localisePdfJobFailure(privateCompressionJob, turkish),
    /Private|Users|document\.pdf/u
  );

  for (const [name, source] of [
    ["split", split],
    ["protection", protection],
    ["compression", compression],
    ["activity", activity],
    ["updates", updates]
  ]) {
    assert.match(source, /useI18n\(\)/u, `${name} is not connected to the locale provider`);
  }
  assert.match(split, /localiseSplitWarnings/u);
  assert.match(compression, /localiseCompressionWarnings/u);
  assert.match(activity, /localisePersistenceWarning/u);
  assert.match(updates, /t\("update\.assurance"\)/u);
  assert.doesNotMatch(
    `${split}\n${protection}\n${compression}\n${activity}\n${updates}`,
    />\s*(?:Split or Extract Pages|Password Protection|Compress PDF|Operation history|Application updates)\s*</u
  );
  assert.doesNotMatch(compression, /preview\.warnings\.map|exportResult\.warnings\.map/u);
});

test("localises document health and privacy cleaning without exposing native prose", async () => {
  const [health, privacy] = await Promise.all([
    readFile(new URL("../src/HealthStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/PrivacyStudio.tsx", import.meta.url), "utf8")
  ]);
  const german = (key, values) => translate("de-DE", key, values);
  const turkish = (key, values) => translate("tr-TR", key, values);
  const healthJob = {
    createdAtMs: 1,
    error: "Private health failure containing C:\\Users\\person\\health.pdf",
    errorCode: null,
    jobId: "health-1-1",
    kind: "health",
    progress: 48,
    result: null,
    stage: "Inspecting private object details",
    stageCode: "health-inspecting",
    status: "failed",
    updatedAtMs: 2
  };
  const privacyJob = {
    createdAtMs: 1,
    error: "Private privacy failure containing C:\\Users\\person\\clean.pdf",
    errorCode: "privacy-inspection-failed",
    jobId: "privacy-inspection-1-1",
    kind: "privacy-inspection",
    progress: 96,
    result: null,
    stage: "Preparing private source report",
    stageCode: "privacy-inspection-reporting",
    status: "failed",
    updatedAtMs: 2
  };

  assert.equal(translate("de-DE", "health.heading.title"), "Dokumentzustand");
  assert.equal(translate("tr-TR", "privacy.heading.title"), "Gizlilik Temizleyici");
  assert.equal(
    localisePdfJobStage(healthJob, turkish),
    translate("tr-TR", "job.stage.healthInspecting")
  );
  assert.equal(
    localisePdfJobFailure(healthJob, german),
    translate("de-DE", "job.error.healthFailed")
  );
  assert.equal(
    localisePdfJobStage(privacyJob, german),
    translate("de-DE", "job.stage.privacyInspectionReporting")
  );
  assert.equal(
    localisePdfJobFailure(privacyJob, turkish),
    translate("tr-TR", "job.error.privacyInspectionFailed")
  );
  assert.doesNotMatch(
    `${localisePdfJobFailure(healthJob, german)}\n${localisePdfJobFailure(privacyJob, turkish)}`,
    /Private|Users|health\.pdf|clean\.pdf/u
  );

  for (const [name, source] of [
    ["document health", health],
    ["privacy cleaner", privacy]
  ]) {
    assert.match(source, /useI18n\(\)/u, `${name} is not connected to the locale provider`);
    assert.match(source, /localisePdfJobFailure/u);
    assert.doesNotMatch(
      source,
      /\{finding\.(?:title|detail)\}/u,
      `${name} displays native finding prose`
    );
  }
  assert.match(health, /healthFindingTitleKeys/u);
  assert.match(privacy, /localisePrivacyWarnings/u);
  assert.doesNotMatch(privacy, /status\.result\.warnings\.map/u);
  assert.doesNotMatch(
    `${health}\n${privacy}`,
    />\s*(?:Document Health|Privacy Cleaner|Inspect Document|Content to remove)\s*</u
  );
});

test("localises PDF comparison and Page Finish without exposing native outcomes", async () => {
  const [comparison, finish] = await Promise.all([
    readFile(new URL("../src/ComparisonStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/PageFinishStudio.tsx", import.meta.url), "utf8")
  ]);
  const german = (key, values) => translate("de-DE", key, values);
  const turkish = (key, values) => translate("tr-TR", key, values);
  const finishingJob = {
    createdAtMs: 1,
    error: "Private finishing failure containing C:\\Users\\person\\finished.pdf",
    errorCode: null,
    jobId: "finishing-1-1",
    kind: "finishing",
    progress: 84,
    result: null,
    stage: "Applying private output password",
    stageCode: "finishing-protecting",
    status: "failed",
    updatedAtMs: 2
  };

  assert.equal(translate("de-DE", "comparison.heading.title"), "PDFs vergleichen");
  assert.equal(translate("tr-TR", "finish.heading.title"), "Sayfa Son İşlemleri");
  assert.equal(localiseFinishPaperName("custom", "Custom", german), "Benutzerdefiniert");
  assert.equal(
    localiseFinishRangeError(
      "The selection must stay between pages 1 and 12.",
      turkish
    ),
    translate("tr-TR", "finish.range.error.bounds", { count: "12" })
  );
  assert.deepEqual(
    localisePageFinishWarnings(
      [
        "Page finishing was applied to 2 selected pages. The source PDF was not changed.",
        "Private native warning containing C:\\Users\\person\\source.pdf",
        "Private native warning containing C:\\Users\\person\\source.pdf"
      ],
      german,
      (value) => formatNumber("de-DE", value)
    ),
    [
      translate("de-DE", "finish.warning.applied.other", { count: "2" }),
      translate("de-DE", "finish.warning.generic")
    ]
  );
  assert.equal(
    localisePdfJobStage(finishingJob, turkish),
    translate("tr-TR", "job.stage.finishingProtecting")
  );
  assert.equal(
    localisePdfJobFailure(finishingJob, german),
    translate("de-DE", "job.error.finishingFailed")
  );
  assert.doesNotMatch(
    localisePdfJobFailure(finishingJob, german),
    /Private|Users|finished\.pdf/u
  );

  for (const [name, source] of [
    ["comparison", comparison],
    ["Page Finish", finish]
  ]) {
    assert.match(source, /useI18n\(\)/u, `${name} is not connected to the locale provider`);
  }
  assert.match(comparison, /ComparisonUserError/u);
  assert.doesNotMatch(comparison, /describePdfError|errorMessage\(/u);
  assert.match(finish, /localisePageFinishWarnings/u);
  assert.match(finish, /localisePdfJobFailure/u);
  assert.doesNotMatch(finish, /(?:inspection|result)\.warnings\.map/u);
  assert.doesNotMatch(
    `${comparison}\n${finish}`,
    />\s*(?:Compare PDFs|Page Finish|Output summary|Bates numbering|Visual tolerance)\s*</u
  );
});

test("localises annotations and forms without exposing native outcomes", async () => {
  const [annotations, forms] = await Promise.all([
    readFile(new URL("../src/AnnotationStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/FormStudio.tsx", import.meta.url), "utf8")
  ]);
  const german = (key, values) => translate("de-DE", key, values);
  const turkish = (key, values) => translate("tr-TR", key, values);
  const annotationJob = {
    createdAtMs: 1,
    error: "Private annotation failure containing C:\\Users\\person\\annotated.pdf",
    errorCode: null,
    jobId: "annotations-1-1",
    kind: "annotations",
    progress: 82,
    result: null,
    stage: "Applying private output password",
    stageCode: "annotations-protecting",
    status: "failed",
    updatedAtMs: 2
  };
  const formJob = {
    createdAtMs: 1,
    error: "Private form failure containing C:\\Users\\person\\form.pdf",
    errorCode: "form-inspection-failed",
    jobId: "form-inspection-1-1",
    kind: "form-inspection",
    progress: 70,
    result: null,
    stage: "Inspecting private field details",
    stageCode: "form-inspection-inspecting",
    status: "failed",
    updatedAtMs: 2
  };

  assert.equal(translate("de-DE", "annotation.heading.title"), "PDF kommentieren");
  assert.equal(translate("tr-TR", "form.heading.title"), "Formları Doldur ve Düzleştir");
  assert.equal(localiseAnnotationKind("freehand", german), "Freihand");
  assert.equal(localiseFormKind("checkbox", turkish), "Onay kutusu");
  assert.equal(
    localiseFormDraftError("Use at most 24 characters.", german, (value) =>
      formatNumber("de-DE", value)
    ),
    translate("de-DE", "form.validation.maximum", { count: "24" })
  );
  assert.deepEqual(
    localiseAnnotationWarnings(
      [
        "2 existing standard annotations can be moved, restyled, duplicated, or deleted in this workspace.",
        "Private native warning containing C:\\Users\\person\\source.pdf",
        "Private native warning containing C:\\Users\\person\\source.pdf"
      ],
      turkish,
      (value) => formatNumber("tr-TR", value)
    ),
    [
      translate("tr-TR", "annotation.warning.editable.other", { count: "2" }),
      translate("tr-TR", "annotation.warning.generic")
    ]
  );
  assert.deepEqual(
    localiseFormWarnings(
      [
        "1 supported field were flattened into static page content and are no longer editable. Signature fields, push buttons, unsupported fields, and fields without complete widget geometry remain interactive.",
        "Private native warning containing C:\\Users\\person\\form.pdf"
      ],
      german,
      (value) => formatNumber("de-DE", value)
    ),
    [
      translate("de-DE", "form.warning.flattened.one", { count: "1" }),
      translate("de-DE", "form.warning.generic")
    ]
  );
  assert.equal(
    localisePdfJobStage(annotationJob, turkish),
    translate("tr-TR", "job.stage.annotationsProtecting")
  );
  assert.equal(
    localisePdfJobFailure(annotationJob, german),
    translate("de-DE", "job.error.annotationsFailed")
  );
  assert.equal(
    localisePdfJobStage(formJob, german),
    translate("de-DE", "job.stage.formInspectionInspecting")
  );
  assert.equal(
    localisePdfJobFailure(formJob, turkish),
    translate("tr-TR", "job.error.formInspectionFailed")
  );
  assert.doesNotMatch(
    `${localisePdfJobFailure(annotationJob, german)}\n${localisePdfJobFailure(formJob, turkish)}`,
    /Private|Users|annotated\.pdf|form\.pdf/u
  );

  for (const [name, source] of [
    ["annotations", annotations],
    ["forms", forms]
  ]) {
    assert.match(source, /useI18n\(\)/u, `${name} is not connected to the locale provider`);
    assert.match(source, /localisePdfJobFailure/u);
    assert.doesNotMatch(source, /describePdfError|errorMessage\(/u);
    assert.doesNotMatch(source, /(?:inspection|result)\.warnings\.map/u);
    assert.doesNotMatch(source, />\s*(?:Annotate PDF|Fill &amp; Flatten Forms)\s*</u);
  }
});

test("localises page content and permanent redaction without exposing native outcomes", async () => {
  const [content, redaction, safety] = await Promise.all([
    readFile(new URL("../src/ContentEditStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/RedactionStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/PdfEditSafetyNotice.tsx", import.meta.url), "utf8")
  ]);
  const german = (key, values) => translate("de-DE", key, values);
  const turkish = (key, values) => translate("tr-TR", key, values);
  const contentJob = {
    createdAtMs: 1,
    error: "Private content failure containing C:\\Users\\person\\content.pdf",
    errorCode: null,
    jobId: "content-1-1",
    kind: "content",
    progress: 82,
    result: null,
    stage: "Applying private output password",
    stageCode: "content-protecting",
    status: "failed",
    updatedAtMs: 2
  };
  const redactionJob = {
    createdAtMs: 1,
    error: "Private redaction failure containing C:\\Users\\person\\redacted.pdf",
    errorCode: "redaction-inspection-failed",
    jobId: "redaction-inspection-1-1",
    kind: "redaction-inspection",
    progress: 64,
    result: null,
    stage: "Inspecting private redaction geometry",
    stageCode: "redaction-inspection-inspecting",
    status: "failed",
    updatedAtMs: 2
  };

  assert.equal(translate("de-DE", "content.heading.title"), "Seiteninhalt bearbeiten");
  assert.equal(translate("tr-TR", "redaction.heading.title"), "Kalıcı Karartma");
  assert.equal(
    localiseRedactionSearchError("Enter text or a name to find.", german),
    translate("de-DE", "redaction.search.error.textRequired")
  );
  assert.deepEqual(
    localiseContentWarnings(
      [
        "2 text objects and 1 image object could not be edited safely in this release and will be preserved unchanged.",
        "Private native warning containing C:\\Users\\person\\content.pdf",
        "Private native warning containing C:\\Users\\person\\content.pdf"
      ],
      turkish,
      (value) => formatNumber("tr-TR", value)
    ),
    [
      translate("tr-TR", "content.warning.readOnly", { images: "1", text: "2" }),
      translate("tr-TR", "content.warning.generic")
    ]
  );
  assert.deepEqual(
    localiseRedactionWarnings(
      [
        "2 pages were flattened to reviewed raster artwork with 3 native-applied permanent redaction regions.",
        "Private native warning containing C:\\Users\\person\\redacted.pdf"
      ],
      german,
      (value) => formatNumber("de-DE", value)
    ),
    [
      translate("de-DE", "redaction.warning.flattened.manyPagesManyRegions", {
        pages: "2",
        regions: "3"
      }),
      translate("de-DE", "redaction.warning.generic")
    ]
  );
  assert.equal(
    localisePdfJobStage(contentJob, turkish),
    translate("tr-TR", "job.stage.contentProtecting")
  );
  assert.equal(
    localisePdfJobFailure(contentJob, german),
    translate("de-DE", "job.error.contentFailed")
  );
  assert.equal(
    localisePdfJobStage(redactionJob, german),
    translate("de-DE", "job.stage.redactionInspectionInspecting")
  );
  assert.equal(
    localisePdfJobFailure(redactionJob, turkish),
    translate("tr-TR", "job.error.redactionInspectionFailed")
  );
  assert.doesNotMatch(
    `${localisePdfJobFailure(contentJob, german)}\n${localisePdfJobFailure(redactionJob, turkish)}`,
    /Private|Users|content\.pdf|redacted\.pdf/u
  );

  for (const [name, source] of [
    ["page content", content],
    ["redaction", redaction]
  ]) {
    assert.match(source, /useI18n\(\)/u, `${name} is not connected to the locale provider`);
    assert.match(source, /localisePdfJobFailure/u);
    assert.doesNotMatch(source, /describePdfError|errorMessage\(/u);
    assert.doesNotMatch(source, /(?:inspection|result)\.warnings\.map/u);
    assert.doesNotMatch(source, /toLocaleString\("en-GB"\)/u);
    assert.doesNotMatch(source, /job\.error\s*\|\|/u);
  }
  assert.doesNotMatch(safety, /\{check\.error\}/u);
  assert.doesNotMatch(content, />\s*Edit Page Content\s*</u);
  assert.doesNotMatch(redaction, />\s*Permanent Redaction\s*</u);
});

test("localises PDF standards and batch recipes without exposing native outcomes", async () => {
  const [archive, batch, germanSource, turkishSource] = await Promise.all([
    readFile(new URL("../src/ArchiveStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/BatchRecipeStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/locales/de-DE.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/locales/tr-TR.ts", import.meta.url), "utf8")
  ]);
  const german = (key, values) => translate("de-DE", key, values);
  const turkish = (key, values) => translate("tr-TR", key, values);
  const archiveJob = {
    createdAtMs: 1,
    error: "Private validator failure containing C:\\Users\\person\\archive.pdf",
    errorCode: "archive-failed",
    jobId: "archive-1-1",
    kind: "archive",
    progress: 52,
    result: null,
    stage: "Private PDF/X rule containing C:\\Users\\person\\archive.pdf",
    stageCode: "archive-preflighting",
    status: "failed",
    updatedAtMs: 2
  };
  const batchJob = {
    ...archiveJob,
    errorCode: "batch-failed",
    jobId: "batch-1-1",
    kind: "batch",
    stage: "Private file 2 name and compression details",
    stageCode: "batch-compressing"
  };

  assert.equal(translate("de-DE", "archive.heading.title"), "PDF-Normen");
  assert.equal(translate("tr-TR", "batch.heading.title"), "Toplu Tarifler");
  assert.equal(
    localiseArchiveProfileDescription("pdfa-2b", german),
    translate("de-DE", "archive.profile.description.pdfa2b")
  );
  assert.equal(
    localiseArchiveOutcome("preflight-passed", turkish),
    translate("tr-TR", "archive.report.outcome.preflightPassed")
  );
  assert.equal(
    localiseArchiveScope("independent-validation", german),
    translate("de-DE", "archive.report.scope.independent")
  );
  assert.equal(
    localiseArchiveRule(
      {
        clause: "C:\\Private\\rule.txt",
        description: "Private native rule prose",
        failedChecks: 1,
        specification: "Private standard",
        testNumber: "../../../private"
      },
      false,
      turkish
    ),
    translate("tr-TR", "archive.report.validationRule")
  );
  assert.doesNotMatch(
    localiseArchiveValidator(
      "independent-validation",
      "C:\\Users\\person\\validator.exe",
      german
    ),
    /Users|validator\.exe/u
  );
  assert.deepEqual(
    localiseArchiveWarnings(
      [
        "Private validator warning containing C:\\Users\\person\\archive.pdf",
        "Private validator warning containing C:\\Users\\person\\archive.pdf"
      ],
      german
    ),
    [translate("de-DE", "archive.warning.generic")]
  );
  assert.equal(
    localisePdfJobStage(archiveJob, turkish),
    translate("tr-TR", "job.stage.archivePreflighting")
  );
  assert.equal(
    localisePdfJobFailure(archiveJob, german),
    translate("de-DE", "job.error.archiveFailed")
  );
  assert.equal(
    localisePdfJobStage(batchJob, german),
    translate("de-DE", "job.stage.batchCompressing")
  );
  assert.equal(
    localisePdfJobFailure(batchJob, turkish),
    translate("tr-TR", "job.error.batchFailed")
  );

  const privateRecipe = BUILT_IN_BATCH_RECIPES[1];
  assert.equal(
    localiseBatchRecipeName(privateRecipe, german),
    translate("de-DE", "batch.recipe.builtIn.private.name")
  );
  assert.equal(
    localiseBatchRecipeDescription(privateRecipe, turkish),
    translate("tr-TR", "batch.recipe.builtIn.private.description")
  );
  assert.equal(
    localiseBatchInputOrigin("connected-scanner", german),
    translate("de-DE", "batch.origin.connectedScanner")
  );
  assert.equal(
    localiseBatchRecipeSettingsError(
      "Enable searchable OCR before deskewing scanned pages.",
      turkish
    ),
    translate("tr-TR", "batch.recipe.error.deskewRequiresOcr")
  );
  assert.equal(
    batchRecipeExceptionKey(new Error("Private storage detail")),
    "batch.recipe.error.save"
  );
  assert.equal(
    localiseBatchInspectionError("Private parser detail", german),
    translate("de-DE", "batch.source.error.inspect")
  );
  assert.deepEqual(
    localiseBatchSteps(["Compression", "Private native step"], turkish),
    [
      translate("tr-TR", "batch.step.compression"),
      translate("tr-TR", "batch.step.generic")
    ]
  );
  assert.equal(
    localiseBatchNote("Private note with C:\\Users\\person\\source.pdf", german),
    translate("de-DE", "batch.result.note.generic")
  );
  assert.equal(
    localiseBatchSkippedReason("Private skip reason", turkish),
    translate("tr-TR", "batch.result.skipped.generic")
  );
  assert.deepEqual(
    localiseBatchWarnings(
      [
        "Private warning with C:\\Users\\person\\batch.pdf",
        "Private warning with C:\\Users\\person\\batch.pdf"
      ],
      german,
      (value) => formatNumber("de-DE", value)
    ),
    [translate("de-DE", "batch.warning.generic")]
  );

  assert.doesNotMatch(
    `${localisePdfJobFailure(archiveJob, german)}\n${localisePdfJobFailure(batchJob, turkish)}`,
    /Private|Users|archive\.pdf|batch\.pdf/u
  );
  for (const [name, source] of [
    ["PDF standards", archive],
    ["batch recipes", batch]
  ]) {
    assert.match(source, /useI18n\(\)/u, `${name} is not connected to the locale provider`);
    assert.match(source, /localisePdfJobFailure/u);
    assert.doesNotMatch(source, /describePdfError|errorMessage\(/u);
    assert.doesNotMatch(source, /job\.error\s*\|\|/u);
    assert.doesNotMatch(source, /toLocaleString\("en-GB"\)/u);
  }
  assert.doesNotMatch(
    archive,
    /archiveReadiness\?\.detail|report\.scopeNote|failure\.description|result\.warnings\.map/u
  );
  assert.doesNotMatch(
    batch,
    />\s*\{(?:(?:batchInspectionJob|batchJob)\.connectionError|source\.path|outputDirectory)\}\s*</u
  );
  assert.doesNotMatch(
    batch,
    /\{item\.(?:note|skippedReason)\}|item\.stepsApplied\.join|item\.warnings\.map|selectedRecipe\.description/u
  );
  assert.doesNotMatch(archive, />\s*PDF Standards\s*</u);
  assert.doesNotMatch(batch, />\s*Batch Recipes\s*</u);

  const explicitlyTranslatedKeys = Object.keys(catalogues["en-GB"]).filter(
    (key) =>
      key.startsWith("archive.") ||
      key.startsWith("batch.") ||
      /^job\.(?:error\.(?:archive|batch)|stage\.(?:archive|batch))/u.test(key)
  );
  for (const key of explicitlyTranslatedKeys) {
    const literal = JSON.stringify(key).replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
    assert.match(germanSource, new RegExp(literal, "u"), `de-DE does not override ${key}`);
    assert.match(turkishSource, new RegExp(literal, "u"), `tr-TR does not override ${key}`);
  }
});

test("localises bookmarks and printed contents without exposing native outcomes", async () => {
  const [bookmarks, germanSource, turkishSource] = await Promise.all([
    readFile(new URL("../src/BookmarkStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/locales/de-DE.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/locales/tr-TR.ts", import.meta.url), "utf8")
  ]);
  const german = (key, values) => translate("de-DE", key, values);
  const turkish = (key, values) => translate("tr-TR", key, values);
  const exportJob = {
    createdAtMs: 1,
    error: "Private bookmark failure containing C:\\Users\\person\\outline.pdf",
    errorCode: "bookmarks-failed",
    jobId: "bookmarks-1-1",
    kind: "bookmarks",
    progress: 41,
    result: null,
    stage: "Building private contents for C:\\Users\\person\\outline.pdf",
    stageCode: "bookmarks-preparing-contents",
    status: "failed",
    updatedAtMs: 2
  };
  const inspectionJob = {
    ...exportJob,
    errorCode: "bookmark-inspection-failed",
    jobId: "bookmark-inspection-1-1",
    kind: "bookmark-inspection",
    stage: "Inspecting private destination details",
    stageCode: "bookmark-inspection-inspecting"
  };

  assert.equal(translate("de-DE", "bookmark.heading.title"), "Lesezeichen und Inhaltsverzeichnis");
  assert.equal(translate("tr-TR", "bookmark.action.review"), "Yer İmlerini İncele");
  assert.equal(
    localisePrintedContentsValidation(
      "Enter a title for the printed contents pages.",
      german
    ),
    translate("de-DE", "bookmark.contents.validation.titleRequired")
  );
  assert.equal(
    localisePrintedContentsValidation("Private validation with C:\\Users\\person", turkish),
    translate("tr-TR", "bookmark.contents.validation.generic")
  );
  assert.equal(
    bookmarkPdfOpeningErrorKey({ name: "InvalidPDFException" }),
    "bookmark.error.damaged"
  );
  assert.equal(
    bookmarkPdfOpeningErrorKey({ privatePath: "C:\\Users\\person\\outline.pdf" }),
    "bookmark.error.review"
  );
  assert.deepEqual(
    localiseBookmarkWarnings(
      [
        "2 bookmarks use an unsupported, missing, or external destination. Assign a page before exporting the edited tree.",
        "Added 2 printed contents pages with 12 linked entries; source pages moved forward by 2.",
        "Private warning containing C:\\Users\\person\\outline.pdf",
        "Private warning containing C:\\Users\\person\\outline.pdf"
      ],
      german,
      (value) => formatNumber("de-DE", value)
    ),
    [
      translate("de-DE", "bookmark.warning.unresolved.other", { count: "2" }),
      translate("de-DE", "bookmark.warning.contentsAdded.manyPagesManyEntries", {
        entries: "12",
        pages: "2"
      }),
      translate("de-DE", "bookmark.warning.generic")
    ]
  );
  assert.equal(
    localisePdfJobStage(exportJob, turkish),
    translate("tr-TR", "job.stage.bookmarksPreparingContents")
  );
  assert.equal(
    localisePdfJobFailure(exportJob, german),
    translate("de-DE", "job.error.bookmarksFailed")
  );
  assert.equal(
    localisePdfJobStage(inspectionJob, german),
    translate("de-DE", "job.stage.bookmarkInspectionInspecting")
  );
  assert.equal(
    localisePdfJobFailure(inspectionJob, turkish),
    translate("tr-TR", "job.error.bookmarkInspectionFailed")
  );
  assert.doesNotMatch(
    `${localisePdfJobFailure(exportJob, german)}\n${localisePdfJobFailure(inspectionJob, turkish)}`,
    /Private|Users|outline\.pdf/u
  );

  assert.match(bookmarks, /useI18n\(\)/u);
  assert.match(bookmarks, /localisePdfJobFailure/u);
  assert.match(bookmarks, /localiseBookmarkWarnings/u);
  assert.doesNotMatch(bookmarks, /describePdfError|errorMessage\(/u);
  assert.doesNotMatch(bookmarks, /(?:inspection|result)\.warnings\.map/u);
  assert.doesNotMatch(bookmarks, /job\.error\s*\|\|/u);
  assert.doesNotMatch(bookmarks, /toLocaleString\("en-GB"\)/u);
  assert.doesNotMatch(bookmarks, />\s*\{(?:sourcePath|result\.outputPath|inspection\.fileName)\}\s*</u);
  assert.doesNotMatch(bookmarks, />\s*Bookmarks &amp; Contents\s*</u);

  const explicitlyTranslatedKeys = Object.keys(catalogues["en-GB"]).filter(
    (key) =>
      key.startsWith("bookmark.") ||
      /^job\.(?:error\.bookmark|stage\.bookmark)/u.test(key)
  );
  for (const key of explicitlyTranslatedKeys) {
    const literal = JSON.stringify(key).replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
    assert.match(germanSource, new RegExp(literal, "u"), `de-DE does not override ${key}`);
    assert.match(turkishSource, new RegExp(literal, "u"), `tr-TR does not override ${key}`);
  }
});

test("localises certificate signing and validation without exposing native outcomes", async () => {
  const [studio, nativeCertificate, germanSource, turkishSource] = await Promise.all([
    readFile(new URL("../src/CertificateStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src-tauri/src/certificate.rs", import.meta.url), "utf8"),
    readFile(new URL("../src/locales/de-DE.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/locales/tr-TR.ts", import.meta.url), "utf8")
  ]);
  const german = (key, values) => translate("de-DE", key, values);
  const turkish = (key, values) => translate("tr-TR", key, values);
  const signingJob = {
    createdAtMs: 1,
    error: "Private certificate failure at C:\\Users\\person\\identity.p12",
    errorCode: "certificate-failed",
    jobId: "certificate-1-1",
    kind: "certificate",
    progress: 35,
    result: null,
    stage: "Applying private certificate from C:\\Users\\person\\identity.p12",
    stageCode: "certificate-signing",
    status: "failed",
    updatedAtMs: 2
  };
  const validationJob = {
    ...signingJob,
    error: "Private validation report for C:\\Users\\person\\agreement.pdf",
    errorCode: "certificate-validation-failed",
    jobId: "certificate-validation-1-1",
    kind: "certificate-validation",
    progress: 92,
    stage: "Reviewing private signer data",
    stageCode: "certificate-validation-reviewing"
  };

  assert.equal(translate("de-DE", "certificate.heading.title"), "Zertifikatssignaturen");
  assert.equal(translate("tr-TR", "certificate.mode.sign"), "İmzala");
  assert.equal(
    localiseCertificateSummary(
      { intact: true, state: "indeterminate", trusted: false },
      german
    ),
    translate("de-DE", "certificate.report.summary.intactUntrusted")
  );
  assert.deepEqual(
    localiseCertificateWarnings(
      [
        "No trusted timestamp was requested. The signing time may be self-reported rather than independently proven.",
        "Only the first 512 signature fields are listed because the PDF exceeds the bounded report limit.",
        "Private pyHanko diagnostic at C:\\Users\\person\\agreement.pdf",
        "Private pyHanko diagnostic at C:\\Users\\person\\agreement.pdf"
      ],
      turkish,
      (value) => formatNumber("tr-TR", value)
    ),
    [
      translate("tr-TR", "certificate.warning.noTimestamp"),
      translate("tr-TR", "certificate.warning.fieldLimit", { count: "512" }),
      translate("tr-TR", "certificate.warning.generic")
    ]
  );
  assert.equal(
    localiseCertificateFieldName("Embedded certificate signature", german),
    translate("de-DE", "certificate.report.field.embedded")
  );
  assert.equal(
    localiseCertificateFieldName("C:\\Users\\person\\private-field", german),
    translate("de-DE", "certificate.report.field.unnamed")
  );
  assert.equal(
    localiseCertificateFieldKind("document-timestamp", turkish),
    translate("tr-TR", "certificate.report.field.kind.timestamp")
  );
  assert.equal(safeCertificateDocumentText("Approved in London"), "Approved in London");
  assert.equal(safeCertificateDocumentText("C:\\Users\\person\\reason.txt"), null);
  assert.equal(
    localiseCertificateSigningTime(
      "D:20260731153000+01'00'",
      german,
      (value) => new Date(value).toISOString()
    ),
    "Signaturzeit: 2026-07-31T14:30:00.000Z"
  );
  assert.equal(
    localiseCertificateSigningTime("D:20260231090000Z", german, String),
    null
  );
  assert.equal(
    localisePdfJobStage(signingJob, turkish),
    translate("tr-TR", "job.stage.certificateSigning")
  );
  assert.equal(
    localisePdfJobStage(validationJob, german),
    translate("de-DE", "job.stage.certificateValidationReviewing")
  );
  assert.equal(
    localisePdfJobFailure(signingJob, german),
    translate("de-DE", "job.error.certificateFailed")
  );
  assert.equal(
    localisePdfJobFailure(validationJob, turkish),
    translate("tr-TR", "job.error.certificateValidationFailed")
  );
  assert.doesNotMatch(
    `${localisePdfJobFailure(signingJob, german)}\n${localisePdfJobFailure(validationJob, turkish)}`,
    /Private|Users|identity\.p12|agreement\.pdf/u
  );

  assert.match(studio, /useI18n\(\)/u);
  assert.match(studio, /localiseCertificateSummary/u);
  assert.match(studio, /localiseCertificateWarnings/u);
  assert.match(studio, /localisePdfJobFailure/u);
  assert.doesNotMatch(studio, /errorMessage\(/u);
  assert.doesNotMatch(studio, /capabilities\?\.detail/u);
  assert.doesNotMatch(studio, /job\.error\s*\|\|/u);
  assert.doesNotMatch(studio, /<pre>\{report\.details\}<\/pre>/u);
  assert.doesNotMatch(studio, />\s*\{(?:sourcePath|pkcs12Path|report\.summary)\}\s*</u);
  assert.match(nativeCertificate, /MAX_SIGNATURE_REPORT_FIELDS: usize = 512/u);
  assert.match(nativeCertificate, /MAX_SIGNATURE_FIELD_TEXT_BYTES: usize = 1024/u);
  assert.match(nativeCertificate, /report\.details\.clear\(\)/u);

  const explicitlyTranslatedKeys = Object.keys(catalogues["en-GB"]).filter(
    (key) =>
      key.startsWith("certificate.") ||
      /^job\.(?:error\.certificate|stage\.certificate)/u.test(key)
  );
  for (const key of explicitlyTranslatedKeys) {
    const literal = JSON.stringify(key).replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
    assert.match(germanSource, new RegExp(literal, "u"), `de-DE does not override ${key}`);
    assert.match(turkishSource, new RegExp(literal, "u"), `tr-TR does not override ${key}`);
  }
});

test("localises protected-PDF opening without retaining raw native outcomes", async () => {
  const [dialog, hook, pdf, app, germanSource, turkishSource] = await Promise.all([
    readFile(new URL("../src/PdfPasswordDialog.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/usePdfDocument.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/pdf.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/App.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/locales/de-DE.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/locales/tr-TR.ts", import.meta.url), "utf8")
  ]);

  assert.equal(translate("de-DE", "pdfPassword.eyebrow"), "Geschütztes PDF");
  assert.equal(
    translate("de-DE", "pdfPassword.title.initial"),
    "Öffnungspasswort eingeben"
  );
  assert.equal(translate("de-DE", "pdfPassword.action.open"), "PDF öffnen");
  assert.equal(translate("tr-TR", "pdfPassword.eyebrow"), "Korumalı PDF");
  assert.equal(
    translate("tr-TR", "pdfPassword.title.incorrect"),
    "Bu parola işe yaramadı"
  );
  assert.equal(translate("tr-TR", "pdfPassword.action.open"), "PDF'yi Aç");
  assert.equal(
    translate("de-DE", "app.document.error.changed"),
    "Die PDF-Datei wurde auf diesem Gerät geändert. Öffnen Sie sie erneut, um fortzufahren."
  );
  assert.equal(
    translate("tr-TR", "app.document.error.cancelled"),
    "Parola korumalı PDF'yi açma işlemi iptal edildi."
  );

  assert.match(dialog, /useI18n\(\)/u);
  assert.match(dialog, /aria-live="polite"/u);
  assert.match(dialog, /aria-invalid=\{request\.incorrect\}/u);
  assert.match(dialog, /autoComplete="off"/u);
  assert.match(dialog, /validPdfOpeningPasswordInput/u);
  assert.doesNotMatch(
    dialog,
    />\s*(?:Protected PDF|That password did not work|Enter the opening password|Password|Cancel|Open PDF)\s*</u
  );
  assert.match(hook, /classifyPdfOpenError\(reason, pdfRangeFailure\(task\)\)/u);
  assert.match(hook, /setError\("cancelled"\)/u);
  assert.match(hook, /setPasswordRequest\(null\);\s+setDocument\(loadedDocument\)/u);
  assert.doesNotMatch(hook, /describePdfError|error\.message|setError\(reason/u);
  assert.match(pdf, /classifyPdfRangeFailure\(reason\)/u);
  assert.doesNotMatch(pdf, /error\.message|failure\s*=\s*typeof reason/u);
  assert.match(app, /setDocumentReadError\("unreadable"\)/u);
  assert.match(app, /pdfOpenErrorTranslationKey\(pdfError\)/u);

  const explicitlyTranslatedKeys = Object.keys(catalogues["en-GB"]).filter(
    (key) => key.startsWith("pdfPassword.") || key.startsWith("app.document.error.")
  );
  for (const key of explicitlyTranslatedKeys) {
    const literal = JSON.stringify(key).replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
    assert.match(germanSource, new RegExp(literal, "u"), `de-DE does not override ${key}`);
    assert.match(turkishSource, new RegExp(literal, "u"), `tr-TR does not override ${key}`);
  }
});

test("localises PDF canvas states and text search without retaining parser outcomes", async () => {
  const [canvas, searchHook, app, germanSource, turkishSource] = await Promise.all([
    readFile(new URL("../src/PdfPageCanvas.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/usePdfSearch.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/App.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/locales/de-DE.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/locales/tr-TR.ts", import.meta.url), "utf8")
  ]);

  assert.equal(
    translate("en-GB", "search.failed"),
    "The search could not read text from one or more document pages."
  );
  assert.equal(
    translate("de-DE", "pdfCanvas.pageAria", { page: "3" }),
    "Dargestellte PDF-Seite 3"
  );
  assert.equal(
    translate("tr-TR", "pdfCanvas.rendering", { page: "2" }),
    "2. sayfa görüntüleniyor"
  );
  assert.equal(
    translate("de-DE", "search.failed"),
    "Die Dokumentsuche konnte den Text mindestens einer Seite nicht lesen."
  );
  assert.equal(
    translate("tr-TR", "search.failed"),
    "Arama, belgenin bir veya daha fazla sayfasındaki metni okuyamadı."
  );

  assert.match(canvas, /useI18n\(\)/u);
  assert.match(canvas, /t\("pdfCanvas\.annotationLayerAria"\)/u);
  assert.match(canvas, /t\("pdfCanvas\.error"\)/u);
  assert.doesNotMatch(canvas, /Display only in Tufekci|Rendered PDF page/u);
  assert.match(searchHook, /classifyPdfSearchError\(reason\)/u);
  assert.doesNotMatch(searchHook, /reason\.message|Document search failed/u);
  assert.match(app, /usePdfSearch\(plannedSearchPages, searchQuery, locale\)/u);

  const explicitlyTranslatedKeys = Object.keys(catalogues["en-GB"]).filter(
    (key) => key.startsWith("pdfCanvas.") || key.startsWith("search.")
  );
  for (const key of explicitlyTranslatedKeys) {
    const literal = JSON.stringify(key).replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
    assert.match(germanSource, new RegExp(literal, "u"), `de-DE does not override ${key}`);
    assert.match(turkishSource, new RegExp(literal, "u"), `tr-TR does not override ${key}`);
  }
});
