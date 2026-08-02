import assert from "node:assert/strict";
import { access, mkdir, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { E2E_CASE_IDS, writeE2eEvidence } from "../scripts/e2e-evidence.mjs";

const workspace = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const evidenceDirectory = path.join(workspace, "e2e-evidence");
const renderingFixture = path.join(workspace, "qa-fixtures", "unusual-page-sizes.pdf");
const encryptedRenderingFixture = path.join(workspace, "qa-fixtures", "encrypted-aes256.pdf");
const pageTransferTarget = path.join(workspace, "qa-fixtures", "accessibility-review.pdf");
const pageTransferCopyOutput = path.join(workspace, "qa-fixtures", "page-transfer-copy-output.pdf");
const pageTransferMoveOutput = path.join(workspace, "qa-fixtures", "page-transfer-move-output.pdf");
const contentEditOutput = path.join(workspace, "qa-fixtures", "content-edited-output.pdf");
const bookmarkContentsOutput = path.join(workspace, "qa-fixtures", "bookmark-contents-output.pdf");
const mergeNavigationOutput = path.join(workspace, "qa-fixtures", "merge-navigation-output.pdf");
const visualSignatureOutput = path.join(workspace, "qa-fixtures", "visual-signature-output.pdf");
const screenshotDirectory = process.env.PAPERWORKS_E2E_SCREENSHOT_DIR ?? "";
const completedCases = [];
let viewport = null;

describe("Tüfekci Paperworks native shell", () => {
  before(async () => {
    await browser.setWindowSize(1_280, 820);
  });

  after(async () => {
    if (completedCases.length === E2E_CASE_IDS.length) {
      await writeE2eEvidence({
        caseIds: completedCases,
        outputDirectory: evidenceDirectory,
        viewport
      });
    }
  });

  it("starts the real desktop shell, reaches engine readiness, and opens searchable OCR", async () => {
    const heading = await browser.$("h1");
    await heading.waitForDisplayed();
    assert.equal(await heading.getText(), "Tüfekci Paperworks");

    await setInterfaceLocale("en-GB");
    await waitForInterfaceLocale("en-GB");
    await browser.waitUntil(
      async () => /^\d+\/\d+ engines ready$/u.test(await browser.$(".system-title span").getText()),
      { timeout: 60_000, timeoutMsg: "The native engine readiness probe did not finish." }
    );
    assert.equal(await (await buttonNamed("Activity")).isEnabled(), true);

    const recogniseText = await browser.$('[role="tab"][aria-label="Recognise Text"]');
    await recogniseText.click();
    const ocrStudio = await browser.$(".searchable-ocr-studio");
    await ocrStudio.waitForDisplayed();
    assert.equal(await (await ocrStudio.$("h3")).getText(), "Recognise Text");
    assert.equal(await (await ocrStudio.$(".searchable-ocr-language select")).isExisting(), true);
    assert.equal(
      await (await ocrStudio.$("button.primary")).getText(),
      "Choose Destination and Recognise Text"
    );

    await browser.$('[role="tab"][aria-label="Organise Pages"]').click();

    viewport = await browser.execute(() => ({
      height: window.innerHeight,
      width: window.innerWidth
    }));
    assert.ok(
      viewport.width >= 960,
      `The native window width was ${viewport.width}px; expected at least 960px.`
    );
    assert.ok(
      viewport.height >= 640,
      `The native window height was ${viewport.height}px; expected at least 640px.`
    );
    recordCase("native-shell-readiness");
  });

  it("supports roving workflow keys and skip-link focus", async () => {
    const tabs = await browser.$$('[role="tab"]');
    assert.equal(tabs.length, 20);

    const organise = await browser.$('[role="tab"][aria-label="Organise Pages"]');
    await organise.click();
    await browser.execute((element) => element.focus(), organise);
    assert.equal(await activeElementId(), "workflow-tab-organise");
    await browser.keys("ArrowDown");
    const content = await browser.$('[role="tab"][aria-label="Edit Page Content"]');
    assert.equal(await content.getAttribute("aria-selected"), "true");
    assert.equal(await activeElementId(), "workflow-tab-content");

    await browser.keys("ArrowDown");
    const scan = await browser.$('[role="tab"][aria-label="Scan from Images"]');
    assert.equal(await scan.getAttribute("aria-selected"), "true");
    assert.equal(await activeElementId(), "workflow-tab-scan");

    await browser.keys("ArrowUp");
    assert.equal(await content.getAttribute("aria-selected"), "true");
    assert.equal(await activeElementId(), "workflow-tab-content");

    await browser.keys("ArrowUp");
    assert.equal(await organise.getAttribute("aria-selected"), "true");
    assert.equal(await activeElementId(), "workflow-tab-organise");

    await browser.keys("ArrowUp");
    const protect = await browser.$('[role="tab"][aria-label="Protect"]');
    assert.equal(await protect.getAttribute("aria-selected"), "true");
    assert.equal(await activeElementId(), "workflow-tab-protect");

    await browser.keys("ArrowDown");
    assert.equal(await organise.getAttribute("aria-selected"), "true");
    assert.equal(await activeElementId(), "workflow-tab-organise");

    const skipLink = await browser.$(".skip-link");
    await browser.execute((element) => element.focus(), skipLink);
    await browser.execute((element) => element.click(), skipLink);
    assert.equal(await activeElementId(), "document-editor");
    recordCase("workflow-keyboard-navigation");
  });

  it("contains modal focus, closes with Escape, and returns focus", async () => {
    const opener = await buttonNamed("Updates");
    await opener.click();

    const dialog = await browser.$('[role="dialog"][aria-labelledby="update-dialog-title"]');
    await dialog.waitForDisplayed();
    assert.equal(await browser.$("#update-dialog-title").getText(), "Application updates");

    const initialFocus = await browser.$('[aria-label="Close application updates"]');
    await browser.waitUntil(async () => initialFocus.isFocused(), {
      timeoutMsg: "The update dialogue did not receive its documented initial focus."
    });
    await browser.keys(["Tab"]);
    assert.equal(
      await browser.execute(
        () => document.activeElement?.closest('[data-dialog-root]')?.getAttribute("role") ?? null
      ),
      "dialog"
    );

    await browser.keys(["Escape"]);
    await dialog.waitForExist({ reverse: true });
    await browser.waitUntil(async () => opener.isFocused(), {
      timeoutMsg: "Focus did not return to the Updates button."
    });
    recordCase("modal-focus-management");
  });

  it("loads a generated PDF and applies reversible page operations", async () => {
    await browser.execute((fixturePath) => {
      window.__paperworksE2eOpenPaths = [fixturePath];
    }, renderingFixture);
    await (await buttonNamed("Open PDF or Images")).click();

    await browser.waitUntil(
      async () => {
        const documentName = await browser.$(".document-status strong");
        return (await documentName.isExisting()) && (await documentName.getText()) === "unusual-page-sizes.pdf";
      },
      { timeout: 60_000, timeoutMsg: "The generated PDF did not open in the native shell." }
    );
    await waitForThumbnailCount(4);

    await browser.waitUntil(
      async () => (await browser.$(".thumbnail-list .thumbnail").getAttribute("draggable")) === "true",
      { timeout: 30_000, timeoutMsg: "Page dragging did not unlock after the signature safety check." }
    );
    const thumbnails = await browser.$$(".thumbnail-list .thumbnail");
    assert.deepEqual(await thumbnailLabels(), [
      "Page 1, source page 1. Drag to reorder.",
      "Page 2, source page 2. Drag to reorder.",
      "Page 3, source page 3. Drag to reorder.",
      "Page 4, source page 4. Drag to reorder."
    ]);
    await dragPage(thumbnails[0], thumbnails[2]);
    await browser.waitUntil(
      async () =>
        JSON.stringify(await thumbnailLabels()) ===
        JSON.stringify([
          "Page 1, source page 2. Drag to reorder.",
          "Page 2, source page 3. Drag to reorder.",
          "Page 3, source page 1. Drag to reorder.",
          "Page 4, source page 4. Drag to reorder."
        ]),
      { timeoutMsg: "Dragging the first page onto the third page did not reorder the page plan." }
    );

    let organisedThumbnails = await browser.$$(".thumbnail-list .thumbnail");
    await organisedThumbnails[0].click();
    const selectionToggles = await browser.$$(".thumbnail-select-toggle");
    await selectionToggles[1].click();
    await browser.waitUntil(
      async () =>
        (await browser.$$(".thumbnail-list .thumbnail.is-selected")).length === 2 &&
        (await browser.$(".strip-title small").getText()) === "2 pages selected" &&
        /2 pages selected, page 2 active/iu.test(
          await browser.$(".page-actions-heading span").getText()
      ),
      { timeoutMsg: "The page strip did not expose its two-page selection state." }
    );
    const visibleSelectedThumbnails = await browser.execute(() => {
      const list = document.querySelector(".thumbnail-list");
      if (!(list instanceof HTMLElement)) {
        return 0;
      }
      const viewport = list.getBoundingClientRect();
      return Array.from(document.querySelectorAll(".thumbnail-list .thumbnail.is-selected")).filter(
        (thumbnail) => {
          const bounds = thumbnail.getBoundingClientRect();
          return (
            bounds.bottom > viewport.top &&
            bounds.top < viewport.bottom &&
            bounds.right > viewport.left &&
            bounds.left < viewport.right
          );
        }
      ).length;
    });
    assert.equal(visibleSelectedThumbnails, 2, "Both adjacent selected pages should remain visible.");
    if (screenshotDirectory) {
      await mkdir(screenshotDirectory, { recursive: true });
      await browser.execute(() => {
        window.scrollTo(0, 0);
        const thumbnails = document.querySelector(".thumbnail-list");
        const inspector = document.querySelector(".inspector-panel");
        if (thumbnails instanceof HTMLElement) {
          thumbnails.scrollTop = 0;
        }
        if (inspector instanceof HTMLElement) {
          inspector.scrollTop = 0;
        }
      });
      await browser.saveScreenshot(path.join(screenshotDirectory, "page-multi-selection-desktop.png"));
    }

    await rm(pageTransferCopyOutput, { force: true });
    await browser.execute((targetPath) => {
      window.__paperworksE2eOpenPaths = [targetPath];
    }, pageTransferTarget);
    await clickEnabledButton("Copy or Move", ".page-actions-panel");
    const transferDialog = await browser.$(".page-transfer-dialog");
    await transferDialog.waitForDisplayed();
    assert.equal(await browser.$("#page-transfer-title").getText(), "Move or Copy Pages Between PDFs");
    await clickEnabledButton("Choose PDF", ".page-transfer-dialog");
    await clickEnabledButton("Open Destination", ".page-transfer-dialog");
    await browser.waitUntil(
      async () => (await browser.$$(".page-transfer-destination-strip .destination-page")).length === 1,
      { timeout: 60_000, timeoutMsg: "The reviewed transfer destination did not open." }
    );
    const transferPayloadTypes = await dragPageTransfer(
      await browser.$(".page-transfer-source-strip"),
      await browser.$('[aria-label="Insert before destination page 1"]')
    );
    assert.ok(transferPayloadTypes.includes("application/x-tufekci-paperworks-page-transfer"));
    await browser.waitUntil(
      async () =>
        (await browser.$(".page-transfer-insertion-control input").getValue()) === "0" &&
        (await browser.$$(".page-transfer-inserted-group .transferred-page")).length === 2,
      { timeoutMsg: "The selected page group did not move to the chosen destination boundary." }
    );
    if (screenshotDirectory) {
      await browser.saveScreenshot(path.join(screenshotDirectory, "page-transfer-desktop.png"));
    }
    await browser.execute((outputPath) => {
      window.__paperworksE2eSavePath = outputPath;
    }, pageTransferCopyOutput);
    await clickEnabledButton("Publish Copy", ".page-transfer-dialog");
    await browser.waitUntil(
      async () =>
        (await browser.$(".page-transfer-success strong").getText()) ===
        "Copied 2 pages into page-transfer-copy-output.pdf.",
      { timeout: 60_000, timeoutMsg: "The copied-page destination was not verified." }
    );
    await access(pageTransferCopyOutput);
    await browser.$('[aria-label="Close page transfer"]').click();
    await transferDialog.waitForExist({ reverse: true });
    await waitForThumbnailCount(4);
    assert.equal((await browser.$$(".thumbnail-list .thumbnail.is-selected")).length, 2);

    await rm(pageTransferMoveOutput, { force: true });
    await browser.execute((targetPath) => {
      window.__paperworksE2eOpenPaths = [targetPath];
    }, pageTransferTarget);
    await clickEnabledButton("Copy or Move", ".page-actions-panel");
    await clickEnabledButton("Choose PDF", ".page-transfer-dialog");
    await clickEnabledButton("Open Destination", ".page-transfer-dialog");
    await browser.waitUntil(
      async () => (await browser.$$(".page-transfer-destination-strip .destination-page")).length === 1,
      { timeout: 60_000, timeoutMsg: "The move destination did not open." }
    );
    await browser.$('.page-transfer-mode input[value="move"]').click();
    await browser.execute((outputPath) => {
      window.__paperworksE2eSavePath = outputPath;
    }, pageTransferMoveOutput);
    await clickEnabledButton("Publish and Move", ".page-transfer-dialog");
    await browser.waitUntil(
      async () =>
        (await browser.$(".page-transfer-success strong").getText()) ===
        "Moved 2 pages into page-transfer-move-output.pdf.",
      { timeout: 60_000, timeoutMsg: "The moved-page destination was not verified." }
    );
    await access(pageTransferMoveOutput);
    await browser.$('[aria-label="Close page transfer"]').click();
    await browser.$(".page-transfer-dialog").waitForExist({ reverse: true });
    await waitForThumbnailCount(2);
    assert.deepEqual(await thumbnailLabels(), [
      "Page 1, source page 1. Drag to reorder.",
      "Page 2, source page 4. Drag to reorder."
    ]);
    await clickEnabled('[aria-label="Undo page operation"]', "Undo page operation");
    await waitForThumbnailCount(4);
    assert.deepEqual(await thumbnailLabels(), [
      "Page 1, source page 2. Drag to reorder.",
      "Page 2, source page 3. Drag to reorder.",
      "Page 3, source page 1. Drag to reorder.",
      "Page 4, source page 4. Drag to reorder."
    ]);
    organisedThumbnails = await browser.$$(".thumbnail-list .thumbnail");
    await organisedThumbnails[0].click();
    await browser.waitUntil(
      async () => {
        const selected = await browser.$$(".thumbnail-list .thumbnail.is-selected");
        return selected.length === 1 && (await organisedThumbnails[0].getAttribute("class")).includes("is-selected");
      },
      { timeoutMsg: "The restored first page did not become the sole active selection." }
    );
    const restoredSelectionToggles = await browser.$$(".thumbnail-select-toggle");
    await browser.waitUntil(async () => restoredSelectionToggles[1].isEnabled(), {
      timeout: 60_000,
      timeoutMsg: "The restored second-page selection control did not become available."
    });
    await browser.execute((element) => element.click(), restoredSelectionToggles[1]);
    await browser.waitUntil(
      async () =>
        (await browser.$$(".thumbnail-list .thumbnail.is-selected")).length === 2 &&
        (await restoredSelectionToggles[1].getAttribute("aria-pressed")) === "true",
      { timeoutMsg: "The restored page group could not be selected after transfer undo." }
    );
    await rm(pageTransferCopyOutput, { force: true });
    await rm(pageTransferMoveOutput, { force: true });

    organisedThumbnails = await browser.$$(".thumbnail-list .thumbnail");
    await dragPage(organisedThumbnails[0], organisedThumbnails[3]);
    await browser.waitUntil(
      async () =>
        JSON.stringify(await thumbnailLabels()) ===
        JSON.stringify([
          "Page 1, source page 1. Drag to reorder.",
          "Page 2, source page 4. Drag to reorder.",
          "Page 3, source page 2. Drag to reorder.",
          "Page 4, source page 3. Drag to reorder."
        ]),
      { timeoutMsg: "Dragging the two-page selection did not preserve its document order." }
    );
    assert.equal((await browser.$$(".thumbnail-list .thumbnail.is-selected")).length, 2);

    await clickEnabled('[aria-label="Undo page operation"]', "Undo page operation");
    await browser.waitUntil(
      async () =>
        JSON.stringify(await thumbnailLabels()) ===
        JSON.stringify([
          "Page 1, source page 2. Drag to reorder.",
          "Page 2, source page 3. Drag to reorder.",
          "Page 3, source page 1. Drag to reorder.",
          "Page 4, source page 4. Drag to reorder."
        ]),
      { timeoutMsg: "Undo did not restore the multi-page drag as one history operation." }
    );
    assert.equal((await browser.$$(".thumbnail-list .thumbnail.is-selected")).length, 2);

    await clickEnabledButton("Rotate Selected", ".page-actions-panel");
    await browser.waitUntil(
      async () => {
        const selected = await browser.$$(".thumbnail-list .thumbnail.is-selected");
        return (
          selected.length === 2 &&
          (await selected[0].getAttribute("data-page-rotation")) === "90" &&
          (await selected[1].getAttribute("data-page-rotation")) === "90"
        );
      },
      { timeoutMsg: "Rotate Selected did not rotate both selected pages." }
    );
    await clickEnabled('[aria-label="Undo page operation"]', "Undo page operation");
    await browser.waitUntil(
      async () => {
        const selected = await browser.$$(".thumbnail-list .thumbnail.is-selected");
        return (
          selected.length === 2 &&
          (await selected[0].getAttribute("data-page-rotation")) === "0" &&
          (await selected[1].getAttribute("data-page-rotation")) === "0"
        );
      },
      { timeoutMsg: "Undo did not restore both selected page rotations." }
    );

    await clickEnabledButton("Duplicate Selected", ".page-actions-panel");
    await waitForThumbnailCount(6);
    await clickEnabled('[aria-label="Undo page operation"]', "Undo page operation");
    await waitForThumbnailCount(4);
    assert.equal((await browser.$$(".thumbnail-list .thumbnail.is-selected")).length, 2);

    await clickEnabledButton("Move Later", ".page-actions-panel");
    await browser.waitUntil(
      async () =>
        JSON.stringify(await thumbnailLabels()) ===
        JSON.stringify([
          "Page 1, source page 1. Drag to reorder.",
          "Page 2, source page 2. Drag to reorder.",
          "Page 3, source page 3. Drag to reorder.",
          "Page 4, source page 4. Drag to reorder."
        ]),
      { timeoutMsg: "Move Later did not move the selected group by one position." }
    );
    await clickEnabled('[aria-label="Undo page operation"]', "Undo page operation");
    await browser.waitUntil(
      async () =>
        JSON.stringify(await thumbnailLabels()) ===
        JSON.stringify([
          "Page 1, source page 2. Drag to reorder.",
          "Page 2, source page 3. Drag to reorder.",
          "Page 3, source page 1. Drag to reorder.",
          "Page 4, source page 4. Drag to reorder."
        ]),
      { timeoutMsg: "Undo did not restore the selected group step movement." }
    );

    await clickEnabledButton("Delete Selected", ".page-actions-panel");
    await waitForThumbnailCount(2);
    assert.deepEqual(await thumbnailLabels(), [
      "Page 1, source page 1. Drag to reorder.",
      "Page 2, source page 4. Drag to reorder."
    ]);
    await clickEnabled('[aria-label="Undo page operation"]', "Undo page operation");
    await waitForThumbnailCount(4);

    organisedThumbnails = await browser.$$(".thumbnail-list .thumbnail");
    await organisedThumbnails[2].click();
    await browser.waitUntil(
      async () => (await browser.$$(".thumbnail-list .thumbnail.is-selected")).length === 1,
      { timeoutMsg: "A plain thumbnail click did not return to one active page." }
    );

    await clickEnabledButton("Duplicate", ".page-actions-panel");
    await waitForThumbnailCount(5);
    await clickEnabledButton("Blank Page", ".page-actions-panel");
    await waitForThumbnailCount(6);
    await clickEnabledButton("Rotate", ".page-actions-panel");
    assert.match(await browser.$(".page-actions-panel p").getText(), /rotated 90°/iu);
    await clickEnabledButton("Delete", ".page-actions-panel");
    await waitForThumbnailCount(5);

    await clickEnabled('[aria-label="Undo page operation"]', "Undo page operation");
    await waitForThumbnailCount(6);
    await clickEnabled('[aria-label="Redo page operation"]', "Redo page operation");
    await waitForThumbnailCount(5);
    recordCase("pdf-page-operations");
  });

  it("renders non-blank page pixels and searches the edited page plan", async () => {
    await browser.waitUntil(async () => {
      const sample = await renderedPageSample();
      return sample.width > 0 && sample.height > 0 && sample.inkSamples >= 4;
    }, {
      timeout: 60_000,
      timeoutMsg: "The selected PDF page did not render non-blank canvas pixels."
    });

    await (await browser.$('[aria-label="Search document"]')).click();
    const search = await browser.$('[aria-label="Search text in document"]');
    await search.waitForDisplayed();
    await search.setValue("Business card");
    await browser.waitUntil(
      async () => /2 matches on 2 pages/u.test(await browser.$(".search-status").getText()),
      { timeout: 60_000, timeoutMsg: "PDF text search did not find the duplicated source page." }
    );
    recordCase("pdf-search-and-rendering");
  });

  it("prepares the edited workspace locally and requests the system print dialogue", async () => {
    await browser.execute(() => {
      window.__paperworksE2ePrintRequests = 0;
      window.dispatchEvent(
        new KeyboardEvent("keydown", {
          bubbles: true,
          cancelable: true,
          ctrlKey: true,
          key: "p"
        })
      );
    });

    const printTab = await browser.$('[role="tab"][aria-label="Print"]');
    await browser.waitUntil(async () => (await printTab.getAttribute("aria-selected")) === "true", {
      timeoutMsg: "Ctrl+P did not route the open PDF to the print workflow."
    });
    const studio = await browser.$(".print-studio");
    await studio.waitForDisplayed();
    assert.equal(await (await studio.$(".print-document-summary small")).getText(), "5 workspace pages");
    assert.match(await (await studio.$(".print-system-note small")).getText(), /printer, copies, paper/iu);

    const rangeOptions = await studio.$$(".print-options input");
    assert.equal(rangeOptions.length, 3);
    await rangeOptions[2].click();
    const range = await studio.$(".print-range-field input");
    await range.setValue("3-1");
    assert.equal(await range.getAttribute("aria-invalid"), "true");
    assert.match(await (await studio.$(".print-status.is-error")).getText(), /first page.*before/iu);
    await range.setValue("1-2");
    await browser.waitUntil(async () => (await range.getAttribute("aria-invalid")) === "false", {
      timeoutMsg: "A valid custom print range remained invalid."
    });

    await clickEnabledButton("Prepare and Print...", ".print-studio");
    await browser.waitUntil(
      async () =>
        (await browser.execute(() => window.__paperworksE2ePrintRequests ?? 0)) === 1 &&
        (await browser.$$(".print-preview-pages figure")).length === 2,
      { timeout: 60_000, timeoutMsg: "The local print pages or system-dialogue request did not complete." }
    );
    assert.match(await (await studio.$(".print-status.is-success")).getText(), /system print dialogue opened/iu);
    const preparedGeometry = await browser.execute(() =>
      Array.from(document.querySelectorAll(".paperworks-print-page"), (page) => ({
        height: Number.parseFloat(page.style.getPropertyValue("--paperworks-print-height")),
        page: page.style.page,
        width: Number.parseFloat(page.style.getPropertyValue("--paperworks-print-width"))
      }))
    );
    assert.equal(preparedGeometry.length, 2);
    preparedGeometry.forEach((page, index) => {
      assert.ok(page.width > 100);
      assert.ok(page.height > 100);
      assert.equal(page.page, `paperworks-${index + 1}`);
    });
    await browser.waitUntil(async () => (await preparedPrintSample()).inkSamples >= 4, {
      timeout: 30_000,
      timeoutMsg: "The prepared print page did not contain non-blank rendered pixels."
    });

    if (screenshotDirectory) {
      await mkdir(screenshotDirectory, { recursive: true });
      await browser.saveScreenshot(path.join(screenshotDirectory, "print-workspace-desktop.png"));
    }
    recordCase("print-preparation-and-dialogue");
  });

  it("edits native-reviewed page text and publishes a verified copy", async () => {
    await rm(contentEditOutput, { force: true });
    await (await browser.$('[role="tab"][aria-label="Edit Page Content"]')).click();
    const studio = await browser.$(".content-edit-studio");
    await studio.waitForDisplayed();
    await clickEnabledButton("Open Content Workspace", ".content-edit-studio");

    const dialog = await browser.$('[role="dialog"][aria-labelledby="content-edit-dialog-title"]');
    await dialog.waitForDisplayed({ timeout: 60_000 });
    const replacement = await browser.$(".content-edit-properties textarea");
    await replacement.waitForDisplayed();
    assert.equal(await replacement.getValue(), "Business card");
    await replacement.setValue("Edited card");

    const undo = await browser.$('[aria-label="Undo"]');
    await browser.waitUntil(async () => undo.isEnabled(), {
      timeoutMsg: "The content editor did not record the text replacement."
    });
    await undo.click();
    await browser.waitUntil(async () => (await replacement.getValue()) !== "Edited card", {
      timeoutMsg: "Undo did not restore the previous text-editing state."
    });
    const redo = await browser.$('[aria-label="Redo"]');
    await redo.click();
    await browser.waitUntil(async () => (await replacement.getValue()) === "Edited card");

    await browser.execute((outputPath) => {
      window.__paperworksE2eSavePath = outputPath;
    }, contentEditOutput);
    await (await buttonNamed("Save New PDF", ".content-edit-dialog")).click();
    const result = await browser.$(".content-edit-result");
    await result.waitForDisplayed({ timeout: 60_000 });
    assert.match(await result.getText(), /1 text \| 0 image edits/u);
    await access(contentEditOutput);

    await (await browser.$('[aria-label="Close page-content workspace"]')).click();
    await dialog.waitForExist({ reverse: true });
    await rm(contentEditOutput, { force: true });
    recordCase("page-content-editing");
  });

  it("previews linked printed contents and publishes a verified copy", async () => {
    await rm(bookmarkContentsOutput, { force: true });
    await (await browser.$('[role="tab"][aria-label="Bookmarks & Contents"]')).click();
    const studio = await browser.$(".bookmark-studio");
    await studio.waitForDisplayed();
    await clickEnabledButton("Review Bookmarks", ".bookmark-studio");

    const dialog = await browser.$('[role="dialog"][aria-labelledby="bookmark-dialog-title"]');
    const reviewError = await browser.$(".bookmark-studio .engine-state.is-missing");
    await browser.waitUntil(
      async () => (await dialog.isDisplayed()) || (await reviewError.isDisplayed()),
      {
        timeout: 60_000,
        timeoutMsg: "Bookmark review neither opened its workspace nor reported an error."
      }
    );
    if (await reviewError.isDisplayed()) {
      throw new Error(`Bookmark review failed: ${await reviewError.getText()}`);
    }
    assert.equal((await browser.$$(".bookmark-tree > button")).length, 0);
    await (await buttonNamed("Add", ".bookmark-dialog")).click();
    await browser.waitUntil(async () => (await browser.$$(".bookmark-tree > button")).length === 1, {
      timeoutMsg: "The bookmark workspace did not create the contents source entry."
    });

    const contentsToggle = await browser.$(".printed-contents-toggle input");
    await contentsToggle.click();
    const summary = await browser.$(".printed-contents-summary");
    await summary.waitForDisplayed();
    assert.equal(
      await summary.getText(),
      "Linked entries: 1 | A4 pages: 1 | Source-page shift: 1"
    );
    assert.equal(await browser.$(".printed-contents-preview li strong").getText(), "2");
    const contentsLayout = await browser.execute(() => {
      const panelRect = document.querySelector(".printed-contents-panel")?.getBoundingClientRect();
      const protectionRect = document.querySelector(".output-protection-fields")?.getBoundingClientRect();
      return {
        panelBottom: panelRect?.bottom ?? 0,
        panelHeight: panelRect?.height ?? 0,
        protectionTop: protectionRect?.top ?? 0
      };
    });
    assert.ok(contentsLayout.panelHeight > 200, `Expected expanded contents controls, received ${contentsLayout.panelHeight}px`);
    assert.ok(
      contentsLayout.protectionTop >= contentsLayout.panelBottom,
      `Expected output protection below contents controls, received ${contentsLayout.protectionTop}px < ${contentsLayout.panelBottom}px`
    );

    if (screenshotDirectory) {
      await mkdir(screenshotDirectory, { recursive: true });
      await browser.execute(() => {
        const contentsPanel = document.querySelector(".printed-contents-panel");
        const detail = contentsPanel?.parentElement;
        if (contentsPanel instanceof HTMLElement && detail instanceof HTMLElement) {
          detail.scrollTop = Math.max(0, contentsPanel.offsetTop - 12);
        }
      });
      await browser.saveScreenshot(path.join(screenshotDirectory, "bookmark-contents-desktop.png"));
    }

    await browser.execute((outputPath) => {
      window.__paperworksE2eSavePath = outputPath;
    }, bookmarkContentsOutput);
    await clickEnabledButton("Save New PDF", ".bookmark-dialog");
    const result = await browser.$(".bookmark-export-result");
    const exportError = await browser.$(".bookmark-detail > .bookmark-error");
    await browser.waitUntil(
      async () => (await result.isDisplayed()) || (await exportError.isDisplayed()),
      {
        timeout: 60_000,
        timeoutMsg: "Bookmark export neither published a result nor reported an error."
      }
    );
    if (await exportError.isDisplayed()) {
      throw new Error(`Bookmark export failed: ${await exportError.getText()}`);
    }
    assert.match(
      await result.getText(),
      /Linked contents entries: 1 \| A4 pages: 1/u
    );
    assert.match(await result.getText(), /Pages: 5/u);
    await access(bookmarkContentsOutput);

    await (await browser.$('[aria-label="Close bookmark editor"]')).click();
    await dialog.waitForExist({ reverse: true });
    recordCase("bookmark-contents-publication");
  });

  it("drags merge sources and preserves selected bookmarks", async () => {
    await rm(mergeNavigationOutput, { force: true });
    await (await browser.$('[role="tab"][aria-label="Merge PDFs"]')).click();
    const studio = await browser.$(".assembly-studio");
    await studio.waitForDisplayed();
    await browser.waitUntil(async () => (await browser.$$(".assembly-sources > li")).length === 1, {
      timeoutMsg: "The merge plan did not seed the active PDF."
    });
    assert.deepEqual(await mergeSourceNames(), ["unusual-page-sizes.pdf"]);

    await browser.execute((fixturePath) => {
      window.__paperworksE2eOpenPaths = [fixturePath];
    }, bookmarkContentsOutput);
    await (await buttonNamed("Add PDFs", ".assembly-studio")).click();
    await browser.waitUntil(async () => (await browser.$$(".assembly-sources > li")).length === 2, {
      timeoutMsg: "The bookmarked merge source was not added."
    });
    assert.deepEqual(await mergeSourceNames(), [
      "unusual-page-sizes.pdf",
      "bookmark-contents-output.pdf"
    ]);

    let sourceCards = await browser.$$(".assembly-sources > li");
    const dragHandles = await browser.$$(".source-drag-handle");
    await dragMergeSource(dragHandles[1], sourceCards[1], sourceCards[0]);
    await browser.waitUntil(
      async () =>
        JSON.stringify(await mergeSourceNames()) ===
        JSON.stringify(["bookmark-contents-output.pdf", "unusual-page-sizes.pdf"]),
      { timeoutMsg: "Dragging the bookmarked source did not update the merge order." }
    );

    const pageRanges = await browser.$$(
      '.assembly-sources > li .assembly-field input:not([type="password"])'
    );
    assert.equal(pageRanges.length, 2);
    await pageRanges[0].setValue("2-5");
    await pageRanges[1].setValue("4-3");
    const preserveBookmarks = await browser.$(".merge-navigation-toggle input");
    assert.equal(await preserveBookmarks.isSelected(), true);

    if (screenshotDirectory) {
      await browser.execute((element) => element.scrollIntoView({ block: "center" }), preserveBookmarks);
      await browser.saveScreenshot(path.join(screenshotDirectory, "merge-navigation-desktop.png"));
    }

    await browser.execute((outputPath) => {
      window.__paperworksE2eSavePath = outputPath;
    }, mergeNavigationOutput);
    await clickEnabledButton("Choose Destination and Combine", ".assembly-studio");
    const result = await browser.$(".assembly-status.is-success");
    const mergeError = await browser.$(".assembly-status.is-error");
    await browser.waitUntil(
      async () => (await result.isDisplayed()) || (await mergeError.isDisplayed()),
      {
        timeout: 60_000,
        timeoutMsg: "Merge publication neither completed nor reported an error."
      }
    );
    if (await mergeError.isDisplayed()) {
      throw new Error(`Merge publication failed: ${await mergeError.getText()}`);
    }
    assert.match(await result.getText(), /6-page combined PDF/u);
    assert.match(await result.getText(), /1 bookmark preserved; 1 unresolved or unselected bookmark was omitted/u);
    await access(mergeNavigationOutput);

    await rm(mergeNavigationOutput, { force: true });
    await rm(bookmarkContentsOutput, { force: true });
    recordCase("merge-navigation-preservation");
  });

  it("creates, edits, reuses, locks, exports, and reopens visual signatures", async () => {
    await rm(visualSignatureOutput, { force: true });
    await browser.execute((fixturePath) => {
      window.__paperworksE2eOpenPaths = [fixturePath];
    }, renderingFixture);
    await (await buttonNamed("Open PDF or Images")).click();
    await browser.waitUntil(
      async () => (await browser.$(".document-status strong").getText()) === "unusual-page-sizes.pdf",
      { timeout: 60_000, timeoutMsg: "The visual-signature fixture did not open." }
    );
    await waitForThumbnailCount(4);
    await (await browser.$('[role="tab"][aria-label="Sign Document"]')).click();
    const signatureStudio = await browser.$(".signature-studio");
    await signatureStudio.waitForDisplayed();

    await dropSyntheticSignature();
    const preview = await browser.$('img[alt="Prepared transparent visual mark"]');
    await preview.waitForDisplayed();
    await browser.waitUntil(async () => (await signaturePixelSample(preview)).inkPixels > 0, {
      timeoutMsg: "The signature image was not processed into visible ink."
    });
    const pixels = await signaturePixelSample(preview);
    assert.ok(pixels.transparentPixels > 0, "The prepared signature retained no transparent background pixels.");
    assert.ok(pixels.inkPixels > 0, "The prepared signature retained no visible ink pixels.");

    await clickEnabledButton("Add to This Session", ".signature-studio");
    await browser.waitUntil(async () => (await browser.$$(".signature-asset")).length === 1);

    await clickEnabledButton("Initials", ".signature-studio");
    await clickEnabledButton("Type", ".signature-studio");
    const typedInput = await browser.$(".signature-typed-fields input");
    await typedInput.setValue("MT");
    const typedPreview = await browser.$('img[alt="Prepared transparent visual mark"]');
    await typedPreview.waitForDisplayed();
    await browser.waitUntil(async () => (await signaturePixelSample(typedPreview)).inkPixels > 0, {
      timeoutMsg: "The typed initials were not rasterised into visible ink."
    });
    await clickEnabledButton("Add to This Session", ".signature-studio");
    await browser.waitUntil(async () => (await browser.$$(".signature-asset")).length === 2);

    await clickEnabledButton("Signature", ".signature-studio");
    await clickEnabledButton("Draw", ".signature-studio");
    await drawFreehandVisualMark();
    const drawnPreview = await browser.$('img[alt="Prepared transparent visual mark"]');
    await drawnPreview.waitForDisplayed();
    await browser.waitUntil(async () => (await signaturePixelSample(drawnPreview)).inkPixels > 0, {
      timeoutMsg: "The freehand signature was not prepared into visible ink."
    });
    await clickEnabledButton("Add to This Session", ".signature-studio");
    await browser.waitUntil(async () => (await browser.$$(".signature-asset")).length === 3);

    const assets = await browser.$$(".signature-asset");
    assert.match(await assets[0].getText(), /My signature[\s\S]*Signature[\s\S]*Image/u);
    assert.match(await assets[1].getText(), /My initials[\s\S]*Initials[\s\S]*Type/u);
    assert.match(await assets[2].getText(), /My signature[\s\S]*Signature[\s\S]*Draw/u);

    await ensureDetailsOpen(".signature-vault");
    const vaultSaveFields = await browser.$$(".signature-vault-save input");
    assert.equal(vaultSaveFields.length, 3);
    await vaultSaveFields[0].setValue("Acceptance signature");
    await vaultSaveFields[1].setValue("correct library passphrase");
    await vaultSaveFields[2].setValue("correct library passphrase");
    await clickEnabledButton("Encrypt and Save", ".signature-vault");
    await browser.waitUntil(
      async () => {
        const currentStatus = await browser.$(".signature-vault-message.is-success");
        return (
          (await currentStatus.isDisplayed()) &&
          (await currentStatus.getText()) === "The visual mark was encrypted and saved locally."
        );
      },
      { timeout: 60_000, timeoutMsg: "The encrypted visual mark was not saved." }
    );

    await clickEnabledButton("Unlock", ".signature-vault-entry");
    const vaultUnlockField = await browser.$(".signature-vault-unlock input");
    await vaultUnlockField.setValue("incorrect library passphrase");
    await clickEnabledButton("Unlock", ".signature-vault-unlock");
    await browser.waitUntil(
      async () => {
        const currentError = await browser.$(".signature-vault-message.is-error");
        return (
          (await currentError.isDisplayed()) &&
          (await currentError.getText()) ===
            "The passphrase is incorrect, or the stored visual mark has been altered."
        );
      },
      {
        timeout: 60_000,
        timeoutMsg: "The vault did not return its sanitised passphrase outcome."
      }
    );
    const vaultRetryField = await browser.$(".signature-vault-unlock input");
    await vaultRetryField.setValue("correct library passphrase");
    await browser.waitUntil(
      async () => {
        const currentError = await browser.$(".signature-vault-message.is-error");
        return !(await currentError.isDisplayed());
      },
      { timeoutMsg: "Editing the vault passphrase did not clear the previous outcome." }
    );
    assert.equal(await vaultRetryField.getValue(), "correct library passphrase");
    await clickEnabledButton("Unlock", ".signature-vault-unlock");
    await browser.waitUntil(
      async () => {
        const currentStatus = await browser.$(".signature-vault-message.is-success");
        return (
          (await currentStatus.isDisplayed()) &&
          /Acceptance signature is unlocked/u.test(await currentStatus.getText())
        );
      },
      { timeout: 60_000, timeoutMsg: "The encrypted visual mark did not unlock on retry." }
    );
    assert.equal((await browser.$$(".signature-asset")).length, 4);

    await browser
      .$('.signature-vault-entry button[aria-label="Delete encrypted visual-mark copy"]')
      .click();
    await clickEnabledButton("Delete Copy", ".signature-vault-delete");
    await browser.waitUntil(
      async () => {
        const currentStatus = await browser.$(".signature-vault-message.is-success");
        return (
          (await currentStatus.isDisplayed()) &&
          (await currentStatus.getText()) ===
            "The encrypted visual-mark copy was deleted from local storage."
        );
      },
      { timeout: 60_000, timeoutMsg: "The encrypted visual-mark fixture was not deleted." }
    );

    const layer = await browser.$(".visual-signature-layer.is-editable");
    await dragVisualMarkToPage(assets[0], layer, 0.24, 0.72);
    await browser.waitUntil(async () => (await browser.$$(".visual-signature-placement")).length === 1);
    await dragVisualMarkToPage(assets[1], layer, 0.5, 0.46);
    await browser.waitUntil(async () => (await browser.$$(".visual-signature-placement")).length === 2);
    await dragVisualMarkToPage(assets[2], layer, 0.76, 0.72);
    await browser.waitUntil(async () => (await browser.$$(".visual-signature-placement")).length === 3, {
      timeoutMsg: "Dragging the prepared visual marks onto the page did not create three placements."
    });
    let overlay = (await browser.$$(".visual-signature-placement"))[1];
    await overlay.waitForDisplayed();

    const positionBeforeMove = await placementPosition(overlay);
    await moveVisualMark(overlay, 36, 24);
    await browser.waitUntil(async () => {
      const positionAfterMove = await placementPosition(overlay);
      return (
        Math.abs(positionAfterMove.left - positionBeforeMove.left) > 0.1 &&
        Math.abs(positionAfterMove.top - positionBeforeMove.top) > 0.1
      );
    }, { timeoutMsg: "The placed initials did not move with a pointer drag." });

    const rotation = await browser.$('.signature-placement-controls input[type="number"]');
    await rotation.setValue("18");
    await browser.waitUntil(async () => (await overlay.getAttribute("style")).includes("rotate(18deg)"), {
      timeoutMsg: "The visual signature rotation was not reflected on the page."
    });
    const size = await browser.$('.signature-placement-controls input[type="range"]');
    const initialSize = Number(await size.getValue());
    const resizedValue = Math.max(4, initialSize - 5);
    await browser.execute((element, value) => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      if (!setter) throw new Error("The native range value setter is unavailable.");
      setter.call(element, String(value));
      element.dispatchEvent(new Event("input", { bubbles: true }));
      element.dispatchEvent(new Event("change", { bubbles: true }));
    }, size, resizedValue);
    await browser.waitUntil(
      async () =>
        Math.abs(
          (await browser.execute(
            (element) => Number.parseFloat(element.style.width),
            overlay
          )) - resizedValue
        ) < 0.01,
      { timeoutMsg: "The visual signature did not resize proportionally." }
    );

    await clickEnabledButton("Duplicate", ".signature-placement-controls");
    await browser.waitUntil(async () => (await browser.$$(".visual-signature-placement")).length === 4);
    await clickEnabled('[aria-label="Undo visual mark placement change"]', "Undo visual mark placement change");
    await browser.waitUntil(async () => (await browser.$$(".visual-signature-placement")).length === 3);
    await clickEnabled('[aria-label="Redo visual mark placement change"]', "Redo visual mark placement change");
    await browser.waitUntil(async () => (await browser.$$(".visual-signature-placement")).length === 4);

    const restoredPlacements = await browser.$$(".visual-signature-placement");
    await restoredPlacements[3].click();
    await browser.$(".signature-placement-controls").waitForDisplayed();
    await clickEnabledButton("Lock", ".signature-placement-controls");
    overlay = await browser.$('.visual-signature-placement[aria-selected="true"]');
    await browser.waitUntil(async () => (await overlay.getAttribute("class")).includes("is-locked"));
    assert.equal(await (await buttonNamed("Delete", ".signature-placement-controls")).isEnabled(), false);

    if (screenshotDirectory) {
      await browser.execute((element) => element.scrollIntoView({ block: "center" }), overlay);
      await browser.saveScreenshot(path.join(screenshotDirectory, "visual-signature-editor-desktop.png"));
    }

    await browser.execute((outputPath) => {
      window.__paperworksE2eSavePath = outputPath;
    }, visualSignatureOutput);
    await (await buttonNamed("Export PDF")).click();
    const exportResult = await browser.$(".operation-banner.is-success");
    const exportError = await browser.$(".operation-banner.is-error");
    await browser.waitUntil(
      async () =>
        ((await exportResult.isDisplayed()) &&
          /visual-signature-output\.pdf/u.test(await exportResult.getText())) ||
        (await exportError.isDisplayed()),
      { timeout: 60_000, timeoutMsg: "Visual-signature export did not reach a terminal result." }
    );
    if (await exportError.isDisplayed()) {
      throw new Error(`Visual-signature export failed: ${await exportError.getText()}`);
    }
    assert.match(await exportResult.getText(), /visual-signature-output\.pdf/u);
    await access(visualSignatureOutput);

    await browser.execute((fixturePath) => {
      window.__paperworksE2eOpenPaths = [fixturePath];
    }, visualSignatureOutput);
    await (await buttonNamed("Open PDF or Images")).click();
    await browser.waitUntil(
      async () => (await browser.$(".document-status strong").getText()) === "visual-signature-output.pdf",
      { timeout: 60_000, timeoutMsg: "The exported visual-signature PDF did not reopen." }
    );
    await waitForThumbnailCount(4);
    await browser.waitUntil(async () => (await renderedPageSample()).inkSamples >= 4, {
      timeout: 60_000,
      timeoutMsg: "The reopened visual-signature PDF did not render document pixels."
    });
    await rm(visualSignatureOutput, { force: true });
    recordCase("signature-definition-and-placement");
  });

  it("switches release locales with accessible metadata and persistent preference", async () => {
    const picker = await browser.$(".locale-picker select");
    await picker.waitForDisplayed();

    await setInterfaceLocale("tr-TR");
    await waitForInterfaceLocale("tr-TR");
    assert.equal(await browser.$("#workflow-tab-merge").getAttribute("aria-label"), "PDF'leri Birleştir");
    assert.equal(await browser.$("#document-editor").getAttribute("aria-label"), "Belge düzenleyicisi");
    assert.equal(
      await browser.$(".pdf-canvas-container.is-page canvas").getAttribute("aria-label"),
      "Görüntülenen PDF sayfası 1"
    );
    await browser.$("#workflow-tab-organise").click();
    assert.equal(await browser.$(".page-actions-panel h3").getText(), "Sayfa İşlemleri");
    assert.equal(await (await buttonNamed("Öne Taşı", ".page-actions-panel")).isDisplayed(), true);
    assert.equal(await (await buttonNamed("Boş Sayfa", ".page-actions-panel")).isDisplayed(), true);
    assert.equal(
      await browser.$(".page-strip").getAttribute("aria-label"),
      "Sayfa küçük resimleri"
    );
    await browser.$("#workflow-tab-ocr").click();
    assert.equal(await browser.$(".searchable-ocr-studio h3").getText(), "Metni Tanı");
    assert.equal(
      await browser.$(".searchable-ocr-studio .primary.wide-button").getText(),
      "Hedefi Seç ve Metni Tanı"
    );
    await browser.$("#workflow-tab-scan").click();
    assert.equal(await browser.$(".scan-settings h3").getText(), "Tarama ayarları");
    assert.equal(await browser.$(".scanner-control-title strong").getText(), "Bağlı tarayıcı");
    await browser.$("#workflow-tab-merge").click();
    assert.equal(await browser.$(".assembly-studio h3").getText(), "PDF'leri Birleştir ve Sayfaları İçe Aktar");
    await browser.$("#workflow-tab-split").click();
    assert.equal(await browser.$(".assembly-studio h3").getText(), "Sayfaları Böl veya Ayıkla");
    await browser.$("#workflow-tab-protect").click();
    assert.equal(await browser.$(".protection-studio h3").getText(), "Parola Koruması");
    await browser.$("#workflow-tab-compress").click();
    assert.equal(await browser.$(".compression-studio h3").getText(), "PDF'yi Sıkıştır");
    await browser.$("#workflow-tab-health").click();
    assert.equal(await browser.$(".health-studio h3").getText(), "Belge Sağlığı");
    await browser.$("#workflow-tab-privacy").click();
    assert.equal(await browser.$(".privacy-studio h3").getText(), "Gizlilik Temizleyici");
    await browser.$("#workflow-tab-compare").click();
    assert.equal(await browser.$(".comparison-studio h3").getText(), "PDF'leri Karşılaştır");
    await browser.$("#workflow-tab-finish").click();
    assert.equal(await browser.$(".finish-studio h3").getText(), "Sayfa Son İşlemleri");
    assert.equal(
      await browser.$(".finish-studio .primary.wide-button").getText(),
      "Sayfa Son İşlemlerini Aç"
    );
    await browser.$("#workflow-tab-annotate").click();
    assert.equal(
      await browser.$(".annotation-studio h3").getText(),
      "PDF'ye Açıklama Ekle"
    );
    assert.equal(
      await browser.$(".annotation-studio .primary.wide-button").getText(),
      "Açıklama Çalışma Alanını Aç"
    );
    await browser.$("#workflow-tab-forms").click();
    assert.equal(
      await browser.$(".form-studio h3").getText(),
      "Formları Doldur ve Düzleştir"
    );
    assert.equal(
      await browser.$(".form-studio .primary.wide-button").getText(),
      "Form Çalışma Alanını Aç"
    );
    await browser.$("#workflow-tab-content").click();
    assert.equal(
      await browser.$(".content-edit-studio h3").getText(),
      "Sayfa İçeriğini Düzenle"
    );
    assert.equal(
      await browser.$(".content-edit-studio .primary.wide-button").getText(),
      "İçerik Çalışma Alanını Aç"
    );
    await browser.$("#workflow-tab-redact").click();
    assert.equal(
      await browser.$(".redaction-studio h3").getText(),
      "Kalıcı Karartma"
    );
    assert.equal(
      await browser.$(".redaction-studio .primary.wide-button").getText(),
      "Karartmaları İncele ve İşaretle"
    );

    await (await buttonNamed("Güncellemeler", ".top-actions")).click();
    await browser.$(".update-dialog").waitForDisplayed();
    assert.equal(await browser.$("#update-dialog-title").getText(), "Uygulama güncellemeleri");
    assert.match(await browser.$(".update-assurance").getText(), /Başarısız doğrulama/u);
    await browser
      .$('.update-dialog button[aria-label="Uygulama güncellemelerini kapat"]')
      .click();
    await browser.$(".update-dialog").waitForExist({ reverse: true });

    await (await buttonNamed("Etkinlik", ".top-actions")).click();
    await browser.$(".operation-audit-dialog").waitForDisplayed();
    assert.equal(await browser.$("#operation-audit-title").getText(), "İşlem geçmişi");
    await browser.waitUntil(
      async () =>
        await browser
          .$(
            '.operation-audit-dialog button[aria-label="İşlem geçmişini kapat"]'
          )
          .isClickable(),
      { timeoutMsg: "The Turkish activity close action did not become available." }
    );
    await browser
      .$(
        '.operation-audit-dialog button[aria-label="İşlem geçmişini kapat"]'
      )
      .click();
    await browser.$(".operation-audit-dialog").waitForExist({ reverse: true });

    await browser.$("#workflow-tab-archive").click();
    assert.equal(await browser.$(".archive-studio h3").getText(), "PDF Standartları");
    assert.equal(
      await browser.$(".archive-studio .archive-mode button").getText(),
      "PDF/A Oluştur"
    );
    await browser.$("#workflow-tab-batch").click();
    assert.equal(await browser.$(".batch-studio h3").getText(), "Toplu Tarifler");
    assert.equal(
      await browser.$(".batch-studio .batch-source-actions button").getText(),
      "PDF Ekle"
    );
    await browser.$("#workflow-tab-bookmarks").click();
    assert.equal(
      await browser.$(".bookmark-studio h3").getText(),
      "Yer İmleri ve İçindekiler"
    );
    assert.equal(
      await browser.$(".bookmark-studio .primary.wide-button").getText(),
      "Yer İmlerini İncele"
    );
    await browser.$("#workflow-tab-sign").click();
    assert.equal(
      await browser.$(".certificate-studio > summary strong").getText(),
      "Sertifika İmzaları"
    );
    await ensureDetailsOpen(".certificate-studio");
    assert.equal(
      await (await buttonNamed("İmzala", ".certificate-mode")).isDisplayed(),
      true
    );

    if (screenshotDirectory) {
      await browser.execute(() => window.scrollTo({ top: 0 }));
      await browser.saveScreenshot(
        path.join(screenshotDirectory, "localisation-tr-release-surfaces-desktop.png")
      );
    }

    await browser.execute((fixturePath) => {
      window.__paperworksE2eOpenPaths = [fixturePath];
    }, encryptedRenderingFixture);
    await browser.$(".top-actions .primary").click();
    const turkishPasswordDialog = await browser.$(".password-dialog");
    await turkishPasswordDialog.waitForDisplayed();
    assert.equal(
      await turkishPasswordDialog.$(".eyebrow").getText(),
      "Korumalı PDF"
    );
    assert.equal(
      await turkishPasswordDialog.$("#pdf-password-title").getText(),
      "Açma parolasını girin"
    );
    assert.equal(
      await turkishPasswordDialog.$("label").getText(),
      "Parola"
    );
    const turkishPasswordInput = await turkishPasswordDialog.$('input[type="password"]');
    await turkishPasswordInput.setValue("wrong-password");
    await (await buttonNamed("PDF'yi Aç", ".password-dialog")).click();
    await browser.waitUntil(
      async () =>
        (await turkishPasswordDialog.$("#pdf-password-title").getText()) ===
        "Bu parola işe yaramadı",
      { timeoutMsg: "The Turkish password dialogue did not report the rejected password." }
    );
    await turkishPasswordInput.setValue("paperworks-test");
    await (await buttonNamed("PDF'yi Aç", ".password-dialog")).click();
    await turkishPasswordDialog.waitForExist({ reverse: true });
    await browser.waitUntil(
      async () =>
        (await browser.$(".document-status strong").getText()) === "encrypted-aes256.pdf",
      { timeout: 60_000, timeoutMsg: "The encrypted rendering fixture did not open." }
    );
    await waitForThumbnailCount(1);

    await setInterfaceLocale("de-DE");
    await waitForInterfaceLocale("de-DE");
    await browser.execute((fixturePath) => {
      window.__paperworksE2eOpenPaths = [fixturePath];
    }, encryptedRenderingFixture);
    await browser.$(".top-actions .primary").click();
    const germanPasswordDialog = await browser.$(".password-dialog");
    await germanPasswordDialog.waitForDisplayed();
    assert.equal(
      await germanPasswordDialog.$(".eyebrow").getText(),
      "Geschütztes PDF"
    );
    assert.equal(
      await germanPasswordDialog.$("#pdf-password-title").getText(),
      "Öffnungspasswort eingeben"
    );
    assert.equal(
      await (await buttonNamed("PDF öffnen", ".password-dialog")).isDisplayed(),
      true
    );
    await (await buttonNamed("Abbrechen", ".password-dialog")).click();
    await germanPasswordDialog.waitForExist({ reverse: true });

    await browser.execute((fixturePath) => {
      window.__paperworksE2eOpenPaths = [fixturePath];
    }, renderingFixture);
    await browser.$(".top-actions .primary").click();
    await browser.waitUntil(
      async () =>
        (await browser.$(".document-status strong").getText()) === "unusual-page-sizes.pdf",
      { timeout: 60_000, timeoutMsg: "The localisation fixture did not reopen after cancellation." }
    );
    await waitForThumbnailCount(4);
    assert.equal(
      await browser.$(".pdf-canvas-container.is-page canvas").getAttribute("aria-label"),
      "Dargestellte PDF-Seite 1"
    );

    await browser.$("#workflow-tab-merge").click();
    assert.equal(await browser.$("#workflow-tab-organise").getAttribute("aria-label"), "Seiten organisieren");
    assert.equal(await browser.$(".active-workflow h2").getText(), "PDFs zusammenführen");
    assert.equal(await browser.$("#document-editor").getAttribute("aria-label"), "Dokumenteditor");
    assert.match(
      await browser.$(".document-status > div > span").getText(),
      /^\d+(?:\.\d{3})*,\d{1,2} KB \| Seite 1 von 4$/u
    );
    await browser.$("#workflow-tab-organise").click();
    assert.equal(await browser.$(".page-actions-panel h3").getText(), "Seitenaktionen");
    assert.equal(
      await (await buttonNamed("Weiter nach oben", ".page-actions-panel")).isDisplayed(),
      true
    );
    assert.equal(await (await buttonNamed("Leere Seite", ".page-actions-panel")).isDisplayed(), true);
    assert.equal(await browser.$(".page-strip").getAttribute("aria-label"), "Seitenminiaturen");

    if (screenshotDirectory) {
      await browser.execute(() => window.scrollTo({ top: 0 }));
      await browser.saveScreenshot(
        path.join(screenshotDirectory, "localisation-de-organiser-desktop.png")
      );
    }

    await browser.$("#workflow-tab-ocr").click();
    assert.equal(await browser.$(".searchable-ocr-studio h3").getText(), "Text erkennen");
    assert.equal(
      await browser.$(".searchable-ocr-studio .primary.wide-button").getText(),
      "Ziel auswählen und Text erkennen"
    );
    await browser.$("#workflow-tab-scan").click();
    assert.equal(await browser.$(".scan-settings h3").getText(), "Scaneinstellungen");
    assert.equal(await browser.$(".scanner-control-title strong").getText(), "Angeschlossener Scanner");
    await browser.$("#workflow-tab-split").click();
    assert.equal(await browser.$(".assembly-studio h3").getText(), "Seiten teilen oder extrahieren");
    await browser.$("#workflow-tab-protect").click();
    assert.equal(await browser.$(".protection-studio h3").getText(), "Passwortschutz");
    await browser.$("#workflow-tab-compress").click();
    assert.equal(await browser.$(".compression-studio h3").getText(), "PDF komprimieren");
    await browser.$("#workflow-tab-health").click();
    assert.equal(await browser.$(".health-studio h3").getText(), "Dokumentzustand");
    await browser.$("#workflow-tab-privacy").click();
    assert.equal(await browser.$(".privacy-studio h3").getText(), "Datenschutzbereinigung");
    await browser.$("#workflow-tab-compare").click();
    assert.equal(await browser.$(".comparison-studio h3").getText(), "PDFs vergleichen");
    await browser.$("#workflow-tab-finish").click();
    assert.equal(await browser.$(".finish-studio h3").getText(), "Seitenaufbereitung");
    assert.equal(
      await browser.$(".finish-studio .primary.wide-button").getText(),
      "Seitenaufbereitung öffnen"
    );
    await browser.$("#workflow-tab-annotate").click();
    assert.equal(await browser.$(".annotation-studio h3").getText(), "PDF kommentieren");
    assert.equal(
      await browser.$(".annotation-studio .primary.wide-button").getText(),
      "Anmerkungsarbeitsbereich öffnen"
    );
    await browser.$("#workflow-tab-forms").click();
    assert.equal(
      await browser.$(".form-studio h3").getText(),
      "Formulare ausfüllen und reduzieren"
    );
    assert.equal(
      await browser.$(".form-studio .primary.wide-button").getText(),
      "Formulararbeitsbereich öffnen"
    );
    await browser.$("#workflow-tab-content").click();
    assert.equal(
      await browser.$(".content-edit-studio h3").getText(),
      "Seiteninhalt bearbeiten"
    );
    assert.equal(
      await browser.$(".content-edit-studio .primary.wide-button").getText(),
      "Inhaltsarbeitsbereich öffnen"
    );
    await browser.$("#workflow-tab-redact").click();
    assert.equal(
      await browser.$(".redaction-studio h3").getText(),
      "Dauerhafte Schwärzung"
    );
    assert.equal(
      await browser.$(".redaction-studio .primary.wide-button").getText(),
      "Schwärzungen prüfen und markieren"
    );
    await browser.$("#workflow-tab-archive").click();
    assert.equal(await browser.$(".archive-studio h3").getText(), "PDF-Normen");
    assert.equal(
      await browser.$(".archive-studio .archive-mode button").getText(),
      "PDF/A erstellen"
    );
    await browser.$("#workflow-tab-batch").click();
    assert.equal(await browser.$(".batch-studio h3").getText(), "Stapelrezepte");
    assert.equal(
      await browser.$(".batch-studio .batch-source-actions button").getText(),
      "PDFs hinzufügen"
    );
    await browser.$("#workflow-tab-bookmarks").click();
    assert.equal(
      await browser.$(".bookmark-studio h3").getText(),
      "Lesezeichen und Inhaltsverzeichnis"
    );
    assert.equal(
      await browser.$(".bookmark-studio .primary.wide-button").getText(),
      "Lesezeichen prüfen"
    );
    await browser.$("#workflow-tab-sign").click();
    assert.equal(
      await browser.$(".certificate-studio > summary strong").getText(),
      "Zertifikatssignaturen"
    );
    await ensureDetailsOpen(".certificate-studio");
    assert.equal(
      await (await buttonNamed("Signieren", ".certificate-mode")).isDisplayed(),
      true
    );
    assert.equal(
      await browser.execute(() =>
        window.localStorage.getItem("tufekci-paperworks.interface-locale.v1")
      ),
      "de-DE"
    );

    if (screenshotDirectory) {
      await browser.execute(() => window.scrollTo({ top: 0 }));
      await browser.saveScreenshot(path.join(screenshotDirectory, "localisation-de-desktop.png"));
    }

    await setInterfaceLocale("en-GB");
    await waitForInterfaceLocale("en-GB");
    assert.equal(await browser.$("#workflow-tab-organise").getAttribute("aria-label"), "Organise Pages");
    recordCase("interface-localisation-switching");
  });
});

async function activeElementId() {
  return browser.execute(() => document.activeElement?.id ?? null);
}

async function setInterfaceLocale(locale) {
  const picker = await browser.$(".locale-picker select");
  await browser.execute((element, value) => {
    element.value = value;
    element.dispatchEvent(new Event("change", { bubbles: true }));
  }, picker, locale);
}

async function waitForInterfaceLocale(locale) {
  await browser.waitUntil(
    async () =>
      (await browser.$(".locale-picker select").getValue()) === locale &&
      (await browser.execute(() => document.documentElement.lang)) === locale,
    { timeoutMsg: `The interface did not switch to ${locale}.` }
  );
}

async function buttonNamed(name, ancestor = "") {
  const prefix = ancestor ? `//*[contains(concat(' ', normalize-space(@class), ' '), ' ${ancestor.slice(1)} ')]` : "";
  return browser.$(`${prefix}//button[normalize-space(.)="${name}"]`);
}

async function ensureDetailsOpen(selector) {
  const details = await browser.$(selector);
  if (!(await browser.execute((element) => element.open, details))) {
    await browser.$(`${selector} > summary`).click();
  }
  await browser.waitUntil(
    async () => browser.execute((element) => element.open, details),
    { timeoutMsg: `${selector} did not open.` }
  );
}

async function clickEnabledButton(name, ancestor = "") {
  await browser.waitUntil(async () => (await buttonNamed(name, ancestor)).isEnabled(), {
    timeout: 60_000,
    timeoutMsg: `${name} did not become available after the edit-safety check.`
  });
  const button = await buttonNamed(name, ancestor);
  await browser.execute((element) => element.click(), button);
}

async function clickEnabled(selector, label) {
  await browser.waitUntil(async () => (await browser.$(selector)).isEnabled(), {
    timeout: 60_000,
    timeoutMsg: `${label} did not become available after the edit-safety check.`
  });
  const element = await browser.$(selector);
  await browser.execute((target) => target.click(), element);
}

function recordCase(id) {
  assert.ok(E2E_CASE_IDS.includes(id));
  assert.equal(completedCases.includes(id), false);
  completedCases.push(id);
}

async function renderedPageSample() {
  return browser.execute(() => {
    const canvas = document.querySelector(".pdf-canvas-container.is-page canvas");
    if (!(canvas instanceof HTMLCanvasElement) || canvas.width < 1 || canvas.height < 1) {
      return { height: 0, inkSamples: 0, width: 0 };
    }
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) {
      return { height: canvas.height, inkSamples: 0, width: canvas.width };
    }
    const stepX = Math.max(1, Math.floor(canvas.width / 48));
    const stepY = Math.max(1, Math.floor(canvas.height / 48));
    let inkSamples = 0;
    for (let y = 0; y < canvas.height; y += stepY) {
      for (let x = 0; x < canvas.width; x += stepX) {
        const [red, green, blue, alpha] = context.getImageData(x, y, 1, 1).data;
        if (alpha > 0 && (red < 245 || green < 245 || blue < 245)) {
          inkSamples += 1;
        }
      }
    }
    return { height: canvas.height, inkSamples, width: canvas.width };
  });
}

async function preparedPrintSample() {
  return browser.execute(async () => {
    const image = document.querySelector(".paperworks-print-page img");
    if (!(image instanceof HTMLImageElement)) {
      return { height: 0, inkSamples: 0, width: 0 };
    }
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = 64;
    canvas.height = 64;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) {
      return { height: image.naturalHeight, inkSamples: 0, width: image.naturalWidth };
    }
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
    let inkSamples = 0;
    for (let index = 0; index < pixels.length; index += 16) {
      if (pixels[index] < 245 || pixels[index + 1] < 245 || pixels[index + 2] < 245) {
        inkSamples += 1;
      }
    }
    return { height: image.naturalHeight, inkSamples, width: image.naturalWidth };
  });
}

async function thumbnailLabels() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".thumbnail-list .thumbnail"), (thumbnail) =>
      thumbnail.getAttribute("aria-label")
    )
  );
}

async function dragPage(source, target) {
  await browser.execute((sourceElement) => {
    const transfer = new DataTransfer();
    sourceElement.dispatchEvent(
      new DragEvent("dragstart", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
  }, source);
  await browser.waitUntil(
    async () => (await source.getAttribute("class")).includes("is-dragging"),
    { timeoutMsg: "The page strip did not enter its visible dragging state." }
  );
  await browser.execute((sourceElement, targetElement) => {
    const transfer = new DataTransfer();
    targetElement.dispatchEvent(
      new DragEvent("dragenter", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
    targetElement.dispatchEvent(
      new DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
    targetElement.dispatchEvent(
      new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
    sourceElement.dispatchEvent(
      new DragEvent("dragend", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
  }, source, target);
}

async function dragPageTransfer(source, target) {
  return browser.execute((sourceElement, targetElement) => {
    const transfer = new DataTransfer();
    sourceElement.dispatchEvent(
      new DragEvent("dragstart", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
    targetElement.dispatchEvent(
      new DragEvent("dragenter", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
    targetElement.dispatchEvent(
      new DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
    targetElement.dispatchEvent(
      new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
    sourceElement.dispatchEvent(
      new DragEvent("dragend", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
    return Array.from(transfer.types);
  }, source, target);
}

async function mergeSourceNames() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".assembly-sources .source-name strong"), (element) =>
      element.textContent?.trim() ?? ""
    )
  );
}

async function dragMergeSource(handle, sourceCard, targetCard) {
  await browser.execute((element) => {
    const transfer = new DataTransfer();
    element.dispatchEvent(
      new DragEvent("dragstart", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
  }, handle);
  await browser.waitUntil(async () => (await sourceCard.getAttribute("class")).includes("is-dragging"), {
    timeoutMsg: "The merge source did not enter its visible dragging state."
  });
  await browser.execute((sourceElement, targetElement) => {
    const transfer = new DataTransfer();
    targetElement.dispatchEvent(
      new DragEvent("dragenter", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
    targetElement.dispatchEvent(
      new DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
    targetElement.dispatchEvent(
      new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
    sourceElement.dispatchEvent(
      new DragEvent("dragend", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
  }, handle, targetCard);
}

async function dragVisualMarkToPage(asset, layer, xRatio, yRatio) {
  await browser.execute((assetElement, layerElement, x, y) => {
    const transfer = new DataTransfer();
    assetElement.dispatchEvent(
      new DragEvent("dragstart", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
    const bounds = layerElement.getBoundingClientRect();
    const clientX = bounds.left + bounds.width * x;
    const clientY = bounds.top + bounds.height * y;
    layerElement.dispatchEvent(
      new DragEvent("dragover", {
        bubbles: true,
        cancelable: true,
        clientX,
        clientY,
        dataTransfer: transfer
      })
    );
    layerElement.dispatchEvent(
      new DragEvent("drop", {
        bubbles: true,
        cancelable: true,
        clientX,
        clientY,
        dataTransfer: transfer
      })
    );
    assetElement.dispatchEvent(
      new DragEvent("dragend", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
  }, asset, layer, xRatio, yRatio);
}

async function drawFreehandVisualMark() {
  const canvas = await browser.$('[aria-label="Draw a visual signature or initials"]');
  await canvas.waitForDisplayed();
  await browser.execute((element) => {
    const bounds = element.getBoundingClientRect();
    const pointerId = 71;
    let captured = false;
    element.setPointerCapture = (candidate) => {
      captured = candidate === pointerId;
    };
    element.hasPointerCapture = (candidate) => captured && candidate === pointerId;
    const emit = (type, xRatio, yRatio) => {
      element.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          button: 0,
          buttons: type === "pointerup" ? 0 : 1,
          cancelable: true,
          clientX: bounds.left + bounds.width * xRatio,
          clientY: bounds.top + bounds.height * yRatio,
          pointerId,
          pointerType: "mouse"
        })
      );
    };
    emit("pointerdown", 0.18, 0.62);
    emit("pointermove", 0.34, 0.35);
    emit("pointermove", 0.52, 0.64);
    emit("pointermove", 0.78, 0.32);
    emit("pointerup", 0.78, 0.32);
  }, canvas);
}

async function moveVisualMark(placement, deltaX, deltaY) {
  await browser.execute((element, xOffset, yOffset) => {
    const bounds = element.getBoundingClientRect();
    const pointerId = 72;
    let captured = false;
    element.setPointerCapture = (candidate) => {
      captured = candidate === pointerId;
    };
    element.hasPointerCapture = (candidate) => captured && candidate === pointerId;
    const startX = bounds.left + bounds.width / 2;
    const startY = bounds.top + bounds.height / 2;
    const emit = (type, clientX, clientY) => {
      element.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          button: 0,
          buttons: type === "pointerup" ? 0 : 1,
          cancelable: true,
          clientX,
          clientY,
          pointerId,
          pointerType: "mouse"
        })
      );
    };
    emit("pointerdown", startX, startY);
    emit("pointermove", startX + xOffset, startY + yOffset);
    emit("pointerup", startX + xOffset, startY + yOffset);
  }, placement, deltaX, deltaY);
}

async function placementPosition(placement) {
  return browser.execute((element) => ({
    left: Number.parseFloat(element.style.left),
    top: Number.parseFloat(element.style.top)
  }), placement);
}

async function dropSyntheticSignature() {
  const picker = await browser.$(".signature-image-picker");
  await browser.execute((element) => {
    const canvas = document.createElement("canvas");
    canvas.width = 240;
    canvas.height = 90;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Canvas was unavailable for the signature acceptance fixture.");
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.strokeStyle = "#111827";
    context.lineCap = "round";
    context.lineJoin = "round";
    context.lineWidth = 7;
    context.beginPath();
    context.moveTo(28, 58);
    context.bezierCurveTo(55, 18, 65, 75, 92, 38);
    context.bezierCurveTo(110, 18, 112, 72, 137, 48);
    context.bezierCurveTo(160, 25, 166, 61, 211, 39);
    context.stroke();

    const encoded = canvas.toDataURL("image/png").split(",")[1];
    const binary = atob(encoded);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
    const transfer = new DataTransfer();
    transfer.items.add(new File([bytes], "acceptance-signature.png", { type: "image/png" }));
    element.dispatchEvent(new DragEvent("dragenter", { bubbles: true, dataTransfer: transfer }));
    element.dispatchEvent(
      new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: transfer })
    );
  }, picker);
}

async function signaturePixelSample(image) {
  return browser.execute((element) => {
    if (!(element instanceof HTMLImageElement) || element.naturalWidth < 1 || element.naturalHeight < 1) {
      return { inkPixels: 0, transparentPixels: 0 };
    }
    const canvas = document.createElement("canvas");
    canvas.width = element.naturalWidth;
    canvas.height = element.naturalHeight;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) return { inkPixels: 0, transparentPixels: 0 };
    context.drawImage(element, 0, 0);
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
    let inkPixels = 0;
    let transparentPixels = 0;
    for (let offset = 0; offset < pixels.length; offset += 4) {
      if (pixels[offset + 3] < 10) transparentPixels += 1;
      if (pixels[offset + 3] > 100 && (pixels[offset] < 100 || pixels[offset + 1] < 100 || pixels[offset + 2] < 100)) {
        inkPixels += 1;
      }
    }
    return { inkPixels, transparentPixels };
  }, image);
}

async function waitForThumbnailCount(expected) {
  await browser.waitUntil(
    async () => (await browser.$$(".thumbnail-list .thumbnail")).length === expected,
    { timeoutMsg: `The page strip did not reach ${expected} thumbnails.` }
  );
}
