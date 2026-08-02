import test from "node:test";
import assert from "node:assert/strict";
import {
  boundedContentRect,
  commitContentEditHistory,
  contentDraftFromInspection,
  contentEditCount,
  contentEditPayload,
  createContentEditHistory,
  redoContentEditHistory,
  translateContentImage,
  undoContentEditHistory,
  updateContentImage,
  updateContentText
} from "../src/contentEditDraft.ts";

const sourceText = {
  fontLabel: "Helvetica",
  fontSize: 12,
  pageNumber: 1,
  rect: { height: 0.04, width: 0.2, x: 0.1, y: 0.1 },
  sourceId: `text-${"a".repeat(64)}`,
  text: "Reviewed text"
};

const sourceImage = {
  pageNumber: 1,
  pixelHeight: 600,
  pixelWidth: 800,
  rect: { height: 0.3, width: 0.4, x: 0.2, y: 0.25 },
  sourceId: `image-${"b".repeat(64)}`
};

test("builds a clean unchanged draft from native-reviewed objects", () => {
  const draft = contentDraftFromInspection([sourceText], [sourceImage]);

  assert.equal(contentEditCount(draft), 0);
  assert.deepEqual(contentEditPayload(draft), { imageEdits: [], textEdits: [] });
  assert.notEqual(draft.images[0].rect, sourceImage.rect);
  assert.notEqual(draft.images[0].originalRect, sourceImage.rect);
});

test("creates a minimal native payload without display-only review metadata", () => {
  let draft = contentDraftFromInspection([sourceText], [sourceImage]);
  draft = updateContentText(draft, sourceText.sourceId, "Replacement");
  draft = updateContentImage(draft, sourceImage.sourceId, {
    rect: { height: 0.25, width: 0.35, x: 0.3, y: 0.2 },
    replacementImageDataUrl: "data:image/png;base64,AA=="
  });

  const payload = contentEditPayload(draft);

  assert.equal(contentEditCount(draft), 2);
  assert.deepEqual(payload.textEdits, [
    { replacementText: "Replacement", sourceId: sourceText.sourceId }
  ]);
  assert.deepEqual(payload.imageEdits, [
    {
      delete: false,
      rect: { height: 0.25, width: 0.35, x: 0.3, y: 0.2 },
      replacementImageDataUrl: "data:image/png;base64,AA==",
      sourceId: sourceImage.sourceId
    }
  ]);
  assert.doesNotMatch(JSON.stringify(payload), /Helvetica|pixelWidth|Reviewed text/u);
});

test("deletion strips replacement data while retaining exact reviewed identity", () => {
  let draft = contentDraftFromInspection([], [sourceImage]);
  draft = updateContentImage(draft, sourceImage.sourceId, {
    deleted: true,
    replacementImageDataUrl: "data:image/png;base64,private"
  });

  assert.deepEqual(contentEditPayload(draft).imageEdits[0], {
    delete: true,
    rect: sourceImage.rect,
    replacementImageDataUrl: null,
    sourceId: sourceImage.sourceId
  });
});

test("bounds image movement and sizing to the page", () => {
  const draft = contentDraftFromInspection([], [sourceImage]);
  const translated = translateContentImage(draft.images[0], 0.8, -0.8);

  assert.deepEqual(translated, { height: 0.3, width: 0.4, x: 0.6, y: 0 });
  assert.deepEqual(
    boundedContentRect({ height: Number.NaN, width: 2, x: -2, y: 9 }),
    { height: 0.002, width: 1, x: 0, y: 0.998 }
  );
});

test("keeps one hundred bounded undo states and clears redo after a new edit", () => {
  const base = contentDraftFromInspection([sourceText], []);
  let history = createContentEditHistory(base);
  for (let index = 0; index < 120; index += 1) {
    history = commitContentEditHistory(
      history,
      updateContentText(history.present, sourceText.sourceId, `Revision ${index}`)
    );
  }

  assert.equal(history.past.length, 100);
  history = undoContentEditHistory(history);
  assert.equal(history.present.text[0].text, "Revision 118");
  history = redoContentEditHistory(history);
  assert.equal(history.present.text[0].text, "Revision 119");
  history = undoContentEditHistory(history);
  history = commitContentEditHistory(
    history,
    updateContentText(history.present, sourceText.sourceId, "Fresh branch")
  );
  assert.equal(history.future.length, 0);
});
