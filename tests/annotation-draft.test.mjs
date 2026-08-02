import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  annotationChangeSet,
  annotationDraftFromInspection,
  annotationPayload,
  commitAnnotationHistory,
  createAnnotationHistory,
  rectBetween,
  redoAnnotationHistory,
  translateAnnotation,
  undoAnnotationHistory
} from "../src/annotationDraft.ts";

const draft = (overrides = {}) => ({
  colour: "#235dd8",
  fillColour: null,
  fontSize: 14,
  id: "annotation-1",
  imageDataUrl: null,
  kind: "rectangle",
  lineWidth: 2,
  opacity: 0.8,
  pageNumber: 1,
  points: [],
  rect: { height: 0.2, width: 0.3, x: 0.1, y: 0.2 },
  sourceAnnotationId: null,
  stamp: null,
  start: null,
  end: null,
  text: null,
  viewerAnnotationId: null,
  ...overrides
});

test("normalises drag rectangles in every direction", () => {
  assert.deepEqual(rectBetween({ x: 0.8, y: 0.7 }, { x: 0.2, y: 0.1 }), {
    height: 0.6,
    width: 0.6000000000000001,
    x: 0.2,
    y: 0.1
  });
});

test("moves rectangular and point annotations without crossing page edges", () => {
  const moved = translateAnnotation(draft(), 0.9, -0.5);
  assert.equal(moved.rect.height, 0.2);
  assert.equal(moved.rect.width, 0.3);
  assert.ok(Math.abs(moved.rect.x - 0.7) < 1e-12);
  assert.equal(moved.rect.y, 0);

  const ink = draft({
    kind: "freehand",
    points: [
      { x: 0.8, y: 0.8 },
      { x: 0.9, y: 0.95 }
    ],
    rect: null
  });
  const movedInk = translateAnnotation(ink, 0.5, 0.5).points;
  assert.ok(Math.abs(movedInk[0].x - 0.9) < 1e-12);
  assert.ok(Math.abs(movedInk[0].y - 0.85) < 1e-12);
  assert.deepEqual(movedInk[1], { x: 1, y: 1 });
});

test("commits, undoes, and redoes bounded annotation states", () => {
  const first = draft();
  const second = draft({ id: "annotation-2" });
  let history = createAnnotationHistory();
  history = commitAnnotationHistory(history, [first]);
  history = commitAnnotationHistory(history, [first, second]);
  assert.deepEqual(undoAnnotationHistory(history).present.map((item) => item.id), [
    "annotation-1"
  ]);
  history = undoAnnotationHistory(history);
  assert.deepEqual(redoAnnotationHistory(history).present.map((item) => item.id), [
    "annotation-1",
    "annotation-2"
  ]);
});

test("converts interface colours to bounded PDF colour components", () => {
  assert.deepEqual(annotationPayload(draft({ colour: "#ff8000" })).colour, [
    1,
    128 / 255,
    0
  ]);
});

test("converts inspected PDF colours while retaining source and viewer identities", () => {
  const inspected = annotationDraftFromInspection({
    ...annotationPayload(
      draft({
        colour: "#000000",
        fillColour: "#000000",
        sourceAnnotationId: "source-p1-a1-o10-g0"
      })
    ),
    colour: [1, 0.5, 0],
    fillColour: [0.1, 0.2, 0.3],
    sourceAnnotationId: "source-p1-a1-o10-g0",
    viewerAnnotationId: "10R"
  });

  assert.equal(inspected.colour, "#ff8000");
  assert.equal(inspected.fillColour, "#1a334d");
  assert.equal(inspected.sourceAnnotationId, "source-p1-a1-o10-g0");
  assert.equal(inspected.viewerAnnotationId, "10R");
  assert.equal("viewerAnnotationId" in annotationPayload(inspected), false);
});

test("separates new, updated, removed, and unchanged annotation operations", () => {
  const first = draft({
    id: "existing-1",
    sourceAnnotationId: "source-p1-a1-o10-g0",
    viewerAnnotationId: "10R"
  });
  const second = draft({
    id: "existing-2",
    sourceAnnotationId: "source-p1-a2-o11-g0",
    viewerAnnotationId: "11R"
  });
  const updated = { ...first, opacity: 0.55 };
  const added = draft({ id: "annotation-1" });
  const changes = annotationChangeSet([first, second], [updated, added]);

  assert.deepEqual(changes.newAnnotations.map((item) => item.id), ["annotation-1"]);
  assert.deepEqual(changes.updatedAnnotations.map((item) => item.id), ["existing-1"]);
  assert.deepEqual(changes.removedExistingAnnotationIds, ["source-p1-a2-o11-g0"]);
  assert.deepEqual(annotationChangeSet([first], [{ ...first }]), {
    newAnnotations: [],
    removedExistingAnnotationIds: [],
    updatedAnnotations: []
  });
});

test("selectively replaces editable source appearances without hiding read-only annotations", () => {
  const source = readFileSync(new URL("../src/PdfPageCanvas.tsx", import.meta.url), "utf8");
  assert.match(source, /hiddenAnnotationIdsKey \? AnnotationMode\.DISABLE/);
  assert.match(source, /querySelectorAll<HTMLElement>\("\[data-annotation-id\]"\)/);
  assert.match(source, /hiddenIds\.has\(annotation\.dataset\.annotationId/);
});
