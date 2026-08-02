import { useCallback, useState } from "react";

export type BoundedHistory<T> = {
  future: T[];
  past: T[];
  present: T;
};

type HistoryTransform<T> = (current: T) => T;
type HistorySnapshot<T> = (value: T) => T;

const DEFAULT_HISTORY_LIMIT = 100;

export function createBoundedHistory<T>(present: T): BoundedHistory<T> {
  return {
    future: [],
    past: [],
    present
  };
}

export function commitBoundedHistory<T>(
  history: BoundedHistory<T>,
  next: T,
  snapshot: HistorySnapshot<T> = identity,
  limit = DEFAULT_HISTORY_LIMIT
): BoundedHistory<T> {
  if (Object.is(next, history.present)) {
    return history;
  }
  return {
    future: [],
    past: [...history.past, snapshot(history.present)].slice(-Math.max(1, limit)),
    present: next
  };
}

export function replaceBoundedHistory<T>(
  history: BoundedHistory<T>,
  present: T
): BoundedHistory<T> {
  if (Object.is(present, history.present)) {
    return history;
  }
  return { ...history, present };
}

export function undoBoundedHistory<T>(
  history: BoundedHistory<T>,
  snapshot: HistorySnapshot<T> = identity
): BoundedHistory<T> {
  const previous = history.past[history.past.length - 1];
  if (previous === undefined) {
    return history;
  }
  return {
    future: [snapshot(history.present), ...history.future],
    past: history.past.slice(0, -1),
    present: previous
  };
}

export function redoBoundedHistory<T>(
  history: BoundedHistory<T>,
  snapshot: HistorySnapshot<T> = identity,
  limit = DEFAULT_HISTORY_LIMIT
): BoundedHistory<T> {
  const next = history.future[0];
  if (next === undefined) {
    return history;
  }
  return {
    future: history.future.slice(1),
    past: [...history.past, snapshot(history.present)].slice(-Math.max(1, limit)),
    present: next
  };
}

export function useBoundedHistory<T>(
  initialValue: T,
  snapshot: HistorySnapshot<T> = identity,
  limit = DEFAULT_HISTORY_LIMIT
) {
  const [history, setHistory] = useState(() => createBoundedHistory(initialValue));

  const commit = useCallback(
    (transform: HistoryTransform<T>) => {
      setHistory((current) =>
        commitBoundedHistory(current, transform(current.present), snapshot, limit)
      );
    },
    [limit, snapshot]
  );

  const replace = useCallback((transform: HistoryTransform<T>) => {
    setHistory((current) => replaceBoundedHistory(current, transform(current.present)));
  }, []);

  const reset = useCallback((present: T) => {
    setHistory(createBoundedHistory(present));
  }, []);

  const undo = useCallback(() => {
    setHistory((current) => undoBoundedHistory(current, snapshot));
  }, [snapshot]);

  const redo = useCallback(() => {
    setHistory((current) => redoBoundedHistory(current, snapshot, limit));
  }, [limit, snapshot]);

  return {
    canRedo: history.future.length > 0,
    canUndo: history.past.length > 0,
    commit,
    present: history.present,
    redo,
    replace,
    reset,
    undo
  };
}

function identity<T>(value: T) {
  return value;
}
