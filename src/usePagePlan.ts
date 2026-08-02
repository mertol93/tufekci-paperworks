import { useCallback, useEffect, useRef, useState } from "react";
import { movePagesByStep, reorderPagesAtDrop } from "./pageSelection";

export type PageRotation = 0 | 90 | 180 | 270;

export type PlannedPage =
  | {
      id: string;
      kind: "source";
      rotation: PageRotation;
      sourceId: string;
      sourcePage: number;
    }
  | {
      id: string;
      kind: "blank";
      heightPt: number;
      paperName: string;
      rotation: PageRotation;
      widthPt: number;
    };

type PagePlanHistory = {
  future: PlannedPage[][];
  past: PlannedPage[][];
  present: PlannedPage[];
};

const emptyHistory: PagePlanHistory = {
  future: [],
  past: [],
  present: []
};

export function usePagePlan(documentKey: string | null, pageCount: number) {
  const [history, setHistory] = useState<PagePlanHistory>(emptyHistory);
  const idCounter = useRef(0);

  useEffect(() => {
    idCounter.current = pageCount;
    const present = documentKey
      ? Array.from({ length: pageCount }, (_, index): PlannedPage => ({
          id: `${documentKey}:source:${index + 1}`,
          kind: "source",
          rotation: 0,
          sourceId: "primary",
          sourcePage: index + 1
        }))
      : [];

    setHistory({ future: [], past: [], present });
  }, [documentKey, pageCount]);

  const commit = useCallback((transform: (pages: PlannedPage[]) => PlannedPage[]) => {
    setHistory((current) => {
      const next = transform(current.present);
      if (next === current.present) {
        return current;
      }

      return {
        future: [],
        past: [...current.past, current.present],
        present: next
      };
    });
  }, []);

  const rotate = useCallback(
    (index: number) => {
      commit((pages) => {
        if (!pages[index]) {
          return pages;
        }

        return pages.map((page, pageIndex) =>
          pageIndex === index
            ? {
                ...page,
                rotation: ((page.rotation + 90) % 360) as PageRotation
              }
            : page
        );
      });
    },
    [commit]
  );

  const rotateMany = useCallback(
    (pageIds: readonly string[]) => {
      commit((pages) => {
        const selected = new Set(pageIds);
        if (!pages.some((page) => selected.has(page.id))) {
          return pages;
        }
        return pages.map((page) =>
          selected.has(page.id)
            ? {
                ...page,
                rotation: ((page.rotation + 90) % 360) as PageRotation
              }
            : page
        );
      });
    },
    [commit]
  );

  const remove = useCallback(
    (index: number) => {
      commit((pages) => {
        if (!pages[index] || pages.length <= 1) {
          return pages;
        }

        return pages.filter((_, pageIndex) => pageIndex !== index);
      });
    },
    [commit]
  );

  const removeMany = useCallback(
    (pageIds: readonly string[]) => {
      commit((pages) => {
        const selected = new Set(pageIds);
        const selectedCount = pages.reduce(
          (count, page) => count + Number(selected.has(page.id)),
          0
        );
        if (selectedCount === 0 || pages.length - selectedCount < 1) {
          return pages;
        }
        return pages.filter((page) => !selected.has(page.id));
      });
    },
    [commit]
  );

  const duplicate = useCallback(
    (index: number) => {
      commit((pages) => {
        const page = pages[index];
        if (!page) {
          return pages;
        }

        idCounter.current += 1;
        const copy = {
          ...page,
          id: `${documentKey ?? "document"}:copy:${idCounter.current}`
        } as PlannedPage;
        const next = [...pages];
        next.splice(index + 1, 0, copy);
        return next;
      });
    },
    [commit, documentKey]
  );

  const duplicateMany = useCallback(
    (pageIds: readonly string[]) => {
      commit((pages) => {
        const selected = new Set(pageIds);
        if (!pages.some((page) => selected.has(page.id))) {
          return pages;
        }

        const next: PlannedPage[] = [];
        pages.forEach((page) => {
          next.push(page);
          if (selected.has(page.id)) {
            idCounter.current += 1;
            next.push({
              ...page,
              id: `${documentKey ?? "document"}:copy:${idCounter.current}`
            } as PlannedPage);
          }
        });
        return next;
      });
    },
    [commit, documentKey]
  );

  const insertBlank = useCallback(
    (index: number, widthPt: number, heightPt: number, paperName: string) => {
      commit((pages) => {
        if (widthPt <= 0 || heightPt <= 0 || !Number.isFinite(widthPt + heightPt)) {
          return pages;
        }

        idCounter.current += 1;
        const blank: PlannedPage = {
          heightPt,
          id: `${documentKey ?? "document"}:blank:${idCounter.current}`,
          kind: "blank",
          paperName,
          rotation: 0,
          widthPt
        };
        const next = [...pages];
        next.splice(Math.min(index + 1, pages.length), 0, blank);
        return next;
      });
    },
    [commit, documentKey]
  );

  const insertSourcePages = useCallback(
    (index: number, sourceId: string, sourcePages: number[]) => {
      commit((pages) => {
        if (!sourceId.trim() || sourcePages.length === 0 || sourcePages.some((page) => page < 1)) {
          return pages;
        }

        const imported = sourcePages.map((sourcePage): PlannedPage => {
          idCounter.current += 1;
          return {
            id: `${documentKey ?? "document"}:import:${idCounter.current}`,
            kind: "source",
            rotation: 0,
            sourceId,
            sourcePage
          };
        });
        const next = [...pages];
        next.splice(Math.min(index + 1, pages.length), 0, ...imported);
        return next;
      });
    },
    [commit, documentKey]
  );

  const move = useCallback(
    (fromIndex: number, toIndex: number) => {
      commit((pages) => {
        if (
          fromIndex === toIndex ||
          fromIndex < 0 ||
          toIndex < 0 ||
          fromIndex >= pages.length ||
          toIndex >= pages.length
        ) {
          return pages;
        }

        const next = [...pages];
        const [page] = next.splice(fromIndex, 1);
        next.splice(toIndex, 0, page);
        return next;
      });
    },
    [commit]
  );

  const moveManyAtDrop = useCallback(
    (pageIds: readonly string[], draggedId: string, targetId: string) => {
      commit((pages) => reorderPagesAtDrop(pages, pageIds, draggedId, targetId));
    },
    [commit]
  );

  const moveManyByStep = useCallback(
    (pageIds: readonly string[], direction: -1 | 1) => {
      commit((pages) => movePagesByStep(pages, pageIds, direction));
    },
    [commit]
  );

  const undo = useCallback(() => {
    setHistory((current) => {
      const previous = current.past[current.past.length - 1];
      if (!previous) {
        return current;
      }

      return {
        future: [current.present, ...current.future],
        past: current.past.slice(0, -1),
        present: previous
      };
    });
  }, []);

  const redo = useCallback(() => {
    setHistory((current) => {
      const next = current.future[0];
      if (!next) {
        return current;
      }

      return {
        future: current.future.slice(1),
        past: [...current.past, current.present],
        present: next
      };
    });
  }, []);

  const restore = useCallback((pages: PlannedPage[]) => {
    if (pages.length === 0) {
      return;
    }
    const next = pages.map((page) =>
      page.kind === "source"
        ? { ...page, sourceId: page.sourceId || "primary" }
        : { ...page }
    );
    idCounter.current = next.reduce((maximum, page) => {
      const suffix = page.id.match(/:(\d+)$/)?.[1];
      return Math.max(maximum, suffix ? Number(suffix) : 0);
    }, next.length);
    setHistory({ future: [], past: [], present: next });
  }, []);

  return {
    canRedo: history.future.length > 0,
    canUndo: history.past.length > 0,
    duplicate,
    duplicateMany,
    insertBlank,
    insertSourcePages,
    move,
    moveManyAtDrop,
    moveManyByStep,
    pages: history.present,
    redo,
    remove,
    removeMany,
    restore,
    rotate,
    rotateMany,
    undo
  };
}
