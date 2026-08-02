import test from "node:test";
import assert from "node:assert/strict";
import {
  colourToPdfComponents,
  computeFinishPreview,
  expandFinishTemplate,
  formatBatesNumber,
  parseFinishPageRange
} from "../src/pageFinish.ts";

test("parses all, odd, even, reverse, and combined page ranges", () => {
  assert.deepEqual(parseFinishPageRange("all", 6).pages, [1, 2, 3, 4, 5, 6]);
  assert.deepEqual(parseFinishPageRange("odd", 6).pages, [1, 3, 5]);
  assert.deepEqual(parseFinishPageRange("even", 6).pages, [2, 4, 6]);
  assert.deepEqual(parseFinishPageRange("5-3,1", 6).pages, [1, 3, 4, 5]);
  assert.match(parseFinishPageRange("7", 6).error, /between pages/);
});

test("computes crop-only source placement", () => {
  const layout = computeFinishPreview(
    { pageNumber: 1, widthPt: 600, heightPt: 800, rotation: 0 },
    { topPt: 10, rightPt: 20, bottomPt: 30, leftPt: 40 },
    null
  );
  assert.ok(layout);
  assert.equal(layout.cropWidthPt, 540);
  assert.equal(layout.cropHeightPt, 760);
  assert.ok(Math.abs(layout.sourceLeftPercent + (40 / 540) * 100) < 0.0001);
});

test("fits a cropped source into target paper with margins", () => {
  const layout = computeFinishPreview(
    { pageNumber: 2, widthPt: 800, heightPt: 600, rotation: 90 },
    { topPt: 10, rightPt: 20, bottomPt: 30, leftPt: 40 },
    { widthPt: 595, heightPt: 842, marginPt: 36 }
  );
  assert.ok(layout);
  assert.equal(layout.outputWidthPt, 595);
  assert.equal(layout.outputHeightPt, 842);
  assert.ok(layout.sourceWidthPercent > 0);
});

test("rejects crop and resize settings that leave no visible page", () => {
  assert.equal(
    computeFinishPreview(
      { pageNumber: 1, widthPt: 100, heightPt: 100, rotation: 0 },
      { topPt: 40, rightPt: 40, bottomPt: 40, leftPt: 40 },
      null
    ),
    null
  );
});

test("expands page tokens and creates padded Bates numbers", () => {
  assert.equal(expandFinishTemplate("{file} | {page}/{pages}", 3, 9, "Case.pdf"), "Case.pdf | 3/9");
  assert.equal(formatBatesNumber("TF-", "-A", 7, 4, 2), "TF-0009-A");
});

test("converts interface colours to bounded PDF components", () => {
  assert.deepEqual(colourToPdfComponents("#235dd8"), [35 / 255, 93 / 255, 216 / 255]);
  assert.deepEqual(colourToPdfComponents("invalid"), [0, 0, 0]);
});
