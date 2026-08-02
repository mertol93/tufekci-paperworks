export type IdentifiedPage = {
  id: string;
};

export type PageSelectionMode = "extend-range" | "range" | "single" | "toggle";

export type PageSelectionState = {
  activeId: string;
  anchorId: string;
  selectedIds: string[];
};

export function resolvePageSelection(
  orderedIds: readonly string[],
  selectedIds: readonly string[],
  activeId: string | null,
  anchorId: string | null,
  clickedId: string,
  mode: PageSelectionMode
): PageSelectionState | null {
  const clickedIndex = orderedIds.indexOf(clickedId);
  if (clickedIndex < 0) {
    return null;
  }

  const validSelection = orderedPageSelection(orderedIds, selectedIds);
  const validActiveId = activeId && orderedIds.includes(activeId) ? activeId : clickedId;
  const validAnchorId = anchorId && orderedIds.includes(anchorId) ? anchorId : validActiveId;

  if (mode === "single") {
    return { activeId: clickedId, anchorId: clickedId, selectedIds: [clickedId] };
  }

  if (mode === "toggle") {
    if (!validSelection.includes(clickedId)) {
      return {
        activeId: clickedId,
        anchorId: clickedId,
        selectedIds: orderedPageSelection(orderedIds, [...validSelection, clickedId])
      };
    }
    if (validSelection.length <= 1) {
      return { activeId: clickedId, anchorId: clickedId, selectedIds: [clickedId] };
    }

    const remaining = validSelection.filter((id) => id !== clickedId);
    const nextActiveId =
      validActiveId !== clickedId && remaining.includes(validActiveId)
        ? validActiveId
        : nearestPageId(orderedIds, remaining, clickedIndex);
    return {
      activeId: nextActiveId,
      anchorId: nextActiveId,
      selectedIds: remaining
    };
  }

  const anchorIndex = orderedIds.indexOf(validAnchorId);
  const rangeStart = Math.min(anchorIndex, clickedIndex);
  const rangeEnd = Math.max(anchorIndex, clickedIndex);
  const rangeIds = orderedIds.slice(rangeStart, rangeEnd + 1);
  return {
    activeId: clickedId,
    anchorId: validAnchorId,
    selectedIds:
      mode === "extend-range"
        ? orderedPageSelection(orderedIds, [...validSelection, ...rangeIds])
        : rangeIds
  };
}

export function orderedPageSelection(
  orderedIds: readonly string[],
  selectedIds: readonly string[]
) {
  const selected = new Set(selectedIds);
  return orderedIds.filter((id) => selected.has(id));
}

export function reorderPagesAtDrop<T extends IdentifiedPage>(
  pages: readonly T[],
  selectedIds: readonly string[],
  draggedId: string,
  targetId: string
): T[] {
  const draggedIndex = pages.findIndex((page) => page.id === draggedId);
  const targetIndex = pages.findIndex((page) => page.id === targetId);
  if (draggedIndex < 0 || targetIndex < 0 || draggedId === targetId) {
    return pages as T[];
  }

  const validSelectedIds = orderedPageSelection(
    pages.map((page) => page.id),
    selectedIds
  );
  const movingIds = validSelectedIds.includes(draggedId) ? validSelectedIds : [draggedId];
  const movingSet = new Set(movingIds);
  if (movingSet.has(targetId)) {
    return pages as T[];
  }

  const movingPages = pages.filter((page) => movingSet.has(page.id));
  const remainingPages = pages.filter((page) => !movingSet.has(page.id));
  const remainingTargetIndex = remainingPages.findIndex((page) => page.id === targetId);
  if (remainingTargetIndex < 0) {
    return pages as T[];
  }

  const insertIndex =
    draggedIndex < targetIndex ? remainingTargetIndex + 1 : remainingTargetIndex;
  const reordered = [...remainingPages];
  reordered.splice(insertIndex, 0, ...movingPages);
  return samePageOrder(pages, reordered) ? (pages as T[]) : reordered;
}

export function movePagesByStep<T extends IdentifiedPage>(
  pages: readonly T[],
  selectedIds: readonly string[],
  direction: -1 | 1
): T[] {
  const selected = new Set(orderedPageSelection(pages.map((page) => page.id), selectedIds));
  if (selected.size === 0) {
    return pages as T[];
  }

  const moved = [...pages];
  if (direction === -1) {
    for (let index = 1; index < moved.length; index += 1) {
      if (selected.has(moved[index].id) && !selected.has(moved[index - 1].id)) {
        [moved[index - 1], moved[index]] = [moved[index], moved[index - 1]];
      }
    }
  } else {
    for (let index = moved.length - 2; index >= 0; index -= 1) {
      if (selected.has(moved[index].id) && !selected.has(moved[index + 1].id)) {
        [moved[index], moved[index + 1]] = [moved[index + 1], moved[index]];
      }
    }
  }
  return samePageOrder(pages, moved) ? (pages as T[]) : moved;
}

export function canMovePagesByStep<T extends IdentifiedPage>(
  pages: readonly T[],
  selectedIds: readonly string[],
  direction: -1 | 1
) {
  const selected = new Set(orderedPageSelection(pages.map((page) => page.id), selectedIds));
  return direction === -1
    ? pages.some(
        (page, index) => index > 0 && selected.has(page.id) && !selected.has(pages[index - 1].id)
      )
    : pages.some(
        (page, index) =>
          index < pages.length - 1 &&
          selected.has(page.id) &&
          !selected.has(pages[index + 1].id)
      );
}

function nearestPageId(
  orderedIds: readonly string[],
  candidates: readonly string[],
  originIndex: number
) {
  const candidateSet = new Set(candidates);
  for (let distance = 1; distance < orderedIds.length; distance += 1) {
    const after = orderedIds[originIndex + distance];
    if (after && candidateSet.has(after)) {
      return after;
    }
    const before = orderedIds[originIndex - distance];
    if (before && candidateSet.has(before)) {
      return before;
    }
  }
  return candidates[0];
}

function samePageOrder<T extends IdentifiedPage>(left: readonly T[], right: readonly T[]) {
  return left.length === right.length && left.every((page, index) => page.id === right[index]?.id);
}
