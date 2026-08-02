import assert from "node:assert/strict";
import test from "node:test";
import {
  bookmarkBranchEnd,
  deleteBookmarkBranch,
  indentBookmarkBranch,
  moveBookmarkBranch,
  outdentBookmarkBranch
} from "../src/bookmarkTree.ts";

const tree = () => [
  { id: "a", level: 0 },
  { id: "a1", level: 1 },
  { id: "a1i", level: 2 },
  { id: "a2", level: 1 },
  { id: "b", level: 0 },
  { id: "b1", level: 1 }
];

test("finds and deletes complete bookmark branches", () => {
  assert.equal(bookmarkBranchEnd(tree(), 1), 3);
  assert.deepEqual(deleteBookmarkBranch(tree(), 1).map((item) => item.id), ["a", "a2", "b", "b1"]);
});

test("moves sibling branches without separating descendants", () => {
  const moved = moveBookmarkBranch(tree(), 1, 1);
  assert.deepEqual(moved.map((item) => item.id), ["a", "a2", "a1", "a1i", "b", "b1"]);
  assert.deepEqual(moved.map((item) => item.level), [0, 1, 1, 2, 0, 1]);
});

test("indents a branch beneath its previous sibling", () => {
  const indented = indentBookmarkBranch(tree(), 3, 6);
  assert.deepEqual(indented.map((item) => item.level), [0, 1, 2, 2, 0, 1]);
});

test("outdents a branch after its parent's remaining children", () => {
  const promoted = outdentBookmarkBranch(tree(), 1);
  assert.deepEqual(promoted.map((item) => item.id), ["a", "a2", "a1", "a1i", "b", "b1"]);
  assert.deepEqual(promoted.map((item) => item.level), [0, 1, 0, 1, 0, 1]);
});
