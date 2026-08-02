import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  buildPageSearchIndex,
  commitRedactionHistory,
  createRedactionHistory,
  findPageSearchMatches,
  isUsableRedactionRect,
  rectBetween,
  redoRedactionHistory,
  toRedactionRegionInput,
  translateRedactionRect,
  undoRedactionHistory
} from "../src/redactionDraft.ts";

const redactionStudioSource = readFileSync(
  new URL("../src/RedactionStudio.tsx", import.meta.url),
  "utf8"
);

test("redaction history commits, undoes, and redoes explicit changes", () => {
  const draft = {
    colour: "black",
    id: "redaction-1",
    pageNumber: 1,
    rect: { x: 0.1, y: 0.2, width: 0.3, height: 0.1 },
    source: "manual"
  };
  const committed = commitRedactionHistory(createRedactionHistory(), [draft]);
  assert.deepEqual(committed.present, [draft]);
  assert.deepEqual(undoRedactionHistory(committed).present, []);
  assert.deepEqual(redoRedactionHistory(undoRedactionHistory(committed)).present, [draft]);
});

test("redaction history retains at most one hundred reversible steps", () => {
  let history = createRedactionHistory();
  for (let index = 0; index < 110; index += 1) {
    history = commitRedactionHistory(history, [
      {
        colour: "black",
        id: `redaction-${index}`,
        pageNumber: 1,
        rect: { x: 0.1, y: 0.2, width: 0.3, height: 0.1 },
        source: "manual"
      }
    ]);
  }
  assert.equal(history.past.length, 100);
});

test("drawn and moved rectangles stay normalised to the page", () => {
  const rect = rectBetween({ x: 0.8, y: 0.7 }, { x: 0.2, y: 0.1 });
  assert.deepEqual(rect, { x: 0.2, y: 0.1, width: 0.6000000000000001, height: 0.6 });
  assert.equal(isUsableRedactionRect(rect), true);
  assert.equal(
    isUsableRedactionRect({ x: 0.2, y: 0.1, width: 0.001, height: 0.1 }),
    false
  );
  assert.deepEqual(
    translateRedactionRect(rect, { x: 0.2, y: 0.2 }, { x: 0.9, y: 0.9 }),
    { x: 0.3999999999999999, y: 0.4, width: 0.6000000000000001, height: 0.6 }
  );
});

test("native redaction payloads contain only reviewed colour and geometry", () => {
  const region = toRedactionRegionInput({
    colour: "white",
    id: "redaction-private-id",
    label: "recognised private text",
    pageNumber: 7,
    rect: { x: 0.1, y: 0.2, width: 0.3, height: 0.1 },
    source: "search"
  });

  assert.deepEqual(region, {
    colour: "white",
    height: 0.1,
    width: 0.3,
    x: 0.1,
    y: 0.2
  });
  assert.equal(JSON.stringify(region).includes("private"), false);
});

test("the interface sends a clean page raster and delegates mask painting to native code", () => {
  assert.match(redactionStudioSource, /regions: pageRedactions\.map\(toRedactionRegionInput\)/);
  assert.doesNotMatch(redactionStudioSource, /redaction\.rect\.x \* width/);
  assert.doesNotMatch(redactionStudioSource, /fillStyle = redaction\.colour/);
});

test("literal search joins neighbouring PDF text items and returns a reviewed box", () => {
  const index = buildPageSearchIndex(
    [
      { str: "Mert", transform: [10, 0, 0, 10, 10, 20], width: 20 },
      { str: "Tufekci", transform: [10, 0, 0, 10, 35, 20], width: 35 }
    ],
    [1, 0, 0, -1, 0, 100],
    100,
    100
  );
  assert.equal(index.text, "Mert Tufekci");
  const result = findPageSearchMatches(index, "literal", "mert tufekci", false);
  assert.equal(result.error, null);
  assert.equal(result.matches.length, 1);
  assert.equal(result.matches[0].rects.length, 1);
  assert.ok(result.matches[0].rects[0].width > 0.55);
});

test("email assistance finds addresses without a query", () => {
  const index = buildPageSearchIndex(
    [
      {
        str: "Contact person@example.co.uk today",
        transform: [10, 0, 0, 10, 5, 20],
        width: 90
      }
    ],
    [1, 0, 0, -1, 0, 100],
    100,
    100
  );
  const result = findPageSearchMatches(index, "email", "", false);
  assert.equal(result.error, null);
  assert.deepEqual(result.matches.map((match) => match.text), ["person@example.co.uk"]);
});

test("bounded wildcard patterns support digits and reject an all-star expression", () => {
  const index = buildPageSearchIndex(
    [{ str: "Case AB-1234 ready", transform: [10, 0, 0, 10, 5, 20], width: 90 }],
    [1, 0, 0, -1, 0, 100],
    100,
    100
  );
  const result = findPageSearchMatches(index, "pattern", "AB-####", false);
  assert.equal(result.error, null);
  assert.deepEqual(result.matches.map((match) => match.text), ["AB-1234"]);
  assert.match(findPageSearchMatches(index, "pattern", "***", false).error ?? "", /only asterisks/);
});

test("search matching reports truncation at its explicit result limit", () => {
  const index = buildPageSearchIndex(
    [{ str: "secret secret secret", transform: [10, 0, 0, 10, 5, 20], width: 90 }],
    [1, 0, 0, -1, 0, 100],
    100,
    100
  );
  const result = findPageSearchMatches(index, "literal", "secret", false, 2);
  assert.equal(result.matches.length, 2);
  assert.equal(result.truncated, true);
});
