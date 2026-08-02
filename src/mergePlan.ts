export type MergePlanItem = {
  id: string;
};

export function displayMergePath(path: string) {
  if (path.startsWith("\\\\?\\UNC\\")) {
    return `\\\\${path.slice(8)}`;
  }
  return path.startsWith("\\\\?\\") ? path.slice(4) : path;
}

export function reorderMergePlan<T extends MergePlanItem>(
  items: T[],
  draggedId: string,
  targetId: string
): T[] {
  if (draggedId === targetId) {
    return items;
  }
  const sourceIndex = items.findIndex((item) => item.id === draggedId);
  const targetIndex = items.findIndex((item) => item.id === targetId);
  if (sourceIndex < 0 || targetIndex < 0) {
    return items;
  }
  const reordered = [...items];
  const [dragged] = reordered.splice(sourceIndex, 1);
  const adjustedTarget = reordered.findIndex((item) => item.id === targetId);
  reordered.splice(adjustedTarget, 0, dragged);
  return reordered;
}
