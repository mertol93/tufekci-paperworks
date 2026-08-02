export type BookmarkTreeItem = {
  id: string;
  level: number;
};

export function bookmarkBranchEnd<T extends BookmarkTreeItem>(items: T[], start: number) {
  const level = items[start]?.level ?? 0;
  let end = start + 1;
  while (end < items.length && items[end].level > level) {
    end += 1;
  }
  return end;
}

export function bookmarkHasChildren<T extends BookmarkTreeItem>(items: T[], index: number) {
  return Boolean(items[index + 1] && items[index + 1].level > items[index].level);
}

export function bookmarkHasPreviousSibling<T extends BookmarkTreeItem>(
  items: T[],
  index: number
) {
  return previousSiblingStart(items, index) >= 0;
}

export function deleteBookmarkBranch<T extends BookmarkTreeItem>(items: T[], index: number) {
  if (index < 0 || index >= items.length) {
    return items;
  }
  const end = bookmarkBranchEnd(items, index);
  return [...items.slice(0, index), ...items.slice(end)];
}

export function indentBookmarkBranch<T extends BookmarkTreeItem>(
  items: T[],
  index: number,
  maximumLevel: number
) {
  if (
    index < 0 ||
    index >= items.length ||
    items[index].level >= maximumLevel ||
    !bookmarkHasPreviousSibling(items, index)
  ) {
    return items;
  }
  const end = bookmarkBranchEnd(items, index);
  return items.map((item, itemIndex) =>
    itemIndex >= index && itemIndex < end ? withLevel(item, item.level + 1) : item
  );
}

export function outdentBookmarkBranch<T extends BookmarkTreeItem>(items: T[], index: number) {
  if (index < 0 || index >= items.length || items[index].level === 0) {
    return items;
  }
  const parent = parentStart(items, index);
  if (parent < 0) {
    return items;
  }
  const end = bookmarkBranchEnd(items, index);
  const parentEnd = bookmarkBranchEnd(items, parent);
  const promoted = items
    .slice(index, end)
    .map((item) => withLevel(item, item.level - 1));
  const without = [...items.slice(0, index), ...items.slice(end)];
  const insertAt = parentEnd - promoted.length;
  return [...without.slice(0, insertAt), ...promoted, ...without.slice(insertAt)];
}

export function moveBookmarkBranch<T extends BookmarkTreeItem>(
  items: T[],
  index: number,
  direction: -1 | 1
) {
  if (index < 0 || index >= items.length) {
    return items;
  }
  const end = bookmarkBranchEnd(items, index);
  if (direction === -1) {
    const previousStart = previousSiblingStart(items, index);
    if (previousStart < 0) {
      return items;
    }
    return [
      ...items.slice(0, previousStart),
      ...items.slice(index, end),
      ...items.slice(previousStart, index),
      ...items.slice(end)
    ];
  }

  const nextStart = end;
  if (nextStart >= items.length || items[nextStart].level !== items[index].level) {
    return items;
  }
  const nextEnd = bookmarkBranchEnd(items, nextStart);
  return [
    ...items.slice(0, index),
    ...items.slice(nextStart, nextEnd),
    ...items.slice(index, end),
    ...items.slice(nextEnd)
  ];
}

function previousSiblingStart<T extends BookmarkTreeItem>(items: T[], index: number) {
  const level = items[index]?.level;
  if (level === undefined) {
    return -1;
  }
  for (let candidate = index - 1; candidate >= 0; candidate -= 1) {
    if (items[candidate].level < level) {
      return -1;
    }
    if (items[candidate].level === level) {
      return candidate;
    }
  }
  return -1;
}

function parentStart<T extends BookmarkTreeItem>(items: T[], index: number) {
  const level = items[index]?.level;
  if (level === undefined || level === 0) {
    return -1;
  }
  for (let candidate = index - 1; candidate >= 0; candidate -= 1) {
    if (items[candidate].level === level - 1) {
      return candidate;
    }
  }
  return -1;
}

function withLevel<T extends BookmarkTreeItem>(item: T, level: number): T {
  return { ...item, level };
}
