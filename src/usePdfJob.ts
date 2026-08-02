import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  isActivePdfJob,
  selectRecoverablePdfJob,
  type PdfJobConnectionErrorCode,
  type PdfJobKind,
  type PdfJobSnapshot
} from "./pdfJobs";

export function usePdfJob<TResult>(
  desktopMode: boolean,
  kind: PdfJobKind,
  storageScope?: string
) {
  const [job, setJob] = useState<PdfJobSnapshot<TResult> | null>(null);
  const [connectionError, setConnectionError] =
    useState<PdfJobConnectionErrorCode | null>(null);
  const [recoveredStorageKey, setRecoveredStorageKey] = useState<string | null>(null);
  const jobRef = useRef<PdfJobSnapshot<TResult> | null>(job);
  jobRef.current = job;
  const recoveryStartedForKey = useRef<string | null>(null);
  const waiters = useRef(
    new Map<
      string,
      { reject: (reason: Error) => void; resolve: (result: TResult) => void }
    >()
  );
  const normalisedStorageScope = normaliseStorageScope(storageScope);
  const storageKey = `paperworks.pdf-job.${kind}${normalisedStorageScope ? `.${normalisedStorageScope}` : ""}`;
  const recoveryComplete = !desktopMode || recoveredStorageKey === storageKey;

  const settleWaiter = useCallback((snapshot: PdfJobSnapshot<TResult>) => {
    if (snapshot.status === "queued" || snapshot.status === "running") {
      return;
    }
    const waiter = waiters.current.get(snapshot.jobId);
    if (!waiter) {
      return;
    }
    waiters.current.delete(snapshot.jobId);
    if (
      snapshot.status === "succeeded" &&
      snapshot.result !== undefined &&
      snapshot.result !== null
    ) {
      waiter.resolve(snapshot.result);
    } else if (snapshot.status === "cancelled") {
      waiter.reject(new Error("The PDF job was cancelled."));
    } else {
      waiter.reject(new Error(snapshot.error || "The PDF job could not complete."));
    }
  }, []);

  useEffect(
    () => () => {
      for (const waiter of waiters.current.values()) {
        waiter.reject(
          new Error("The PDF job view closed while processing. The native job may still complete.")
        );
      }
      waiters.current.clear();
    },
    []
  );

  useEffect(() => {
    if (!desktopMode) {
      return;
    }
    if (recoveryStartedForKey.current === storageKey) {
      return;
    }
    recoveryStartedForKey.current = storageKey;
    let active = true;

    const recover = async () => {
      try {
        const storedJobId = readStoredJobId(storageKey);
        if (storedJobId) {
          try {
            const snapshot = await invoke<PdfJobSnapshot<TResult>>("get_pdf_job", {
              jobId: storedJobId
            });
            if (active) {
              setJob(snapshot);
            }
            return;
          } catch {
            removeStoredJobId(storageKey);
          }
        }

        if (normalisedStorageScope) {
          return;
        }

        try {
          const jobs = await invoke<PdfJobSnapshot<TResult>[]>("list_pdf_jobs", { kind });
          const recoverable = selectRecoverablePdfJob(jobs);
          if (active && recoverable) {
            writeStoredJobId(storageKey, recoverable.jobId);
            setJob(recoverable);
          }
        } catch {
          if (active) {
            setConnectionError("history-unavailable");
          }
        }
      } finally {
        if (active) {
          setRecoveredStorageKey(storageKey);
        }
      }
    };

    void recover();
    return () => {
      active = false;
      if (recoveryStartedForKey.current === storageKey) {
        recoveryStartedForKey.current = null;
      }
    };
  }, [desktopMode, kind, normalisedStorageScope, storageKey]);

  useEffect(() => {
    if (!desktopMode || !isActivePdfJob(job)) {
      return;
    }
    let active = true;
    let timer: number | undefined;
    let failures = 0;
    const jobId = job.jobId;

    const poll = async () => {
      try {
        const snapshot = await invoke<PdfJobSnapshot<TResult>>("get_pdf_job", { jobId });
        if (!active) {
          return;
        }
        failures = 0;
        setConnectionError(null);
        setJob(snapshot);
        settleWaiter(snapshot);
        if (isActivePdfJob(snapshot)) {
          timer = window.setTimeout(poll, 250);
        }
      } catch {
        if (!active) {
          return;
        }
        failures += 1;
        if (failures >= 4) {
          setConnectionError("status-unavailable");
        }
        const retryDelay = Math.min(2_000, 250 * 2 ** Math.min(failures, 3));
        timer = window.setTimeout(poll, retryDelay);
      }
    };

    timer = window.setTimeout(poll, 150);
    return () => {
      active = false;
      if (timer !== undefined) {
        window.clearTimeout(timer);
      }
    };
  }, [desktopMode, job?.jobId, job?.status, settleWaiter]);

  const startJob = useCallback(
    async (request: unknown) => {
      const snapshot = await invoke<PdfJobSnapshot<TResult>>("start_pdf_job", {
        request: { kind, request }
      });
      writeStoredJobId(storageKey, snapshot.jobId);
      setConnectionError(null);
      setJob(snapshot);
      return snapshot;
    },
    [kind, storageKey]
  );

  const cancelJob = useCallback(async () => {
    if (!job || !isActivePdfJob(job)) {
      return job;
    }
    const snapshot = await invoke<PdfJobSnapshot<TResult>>("cancel_pdf_job", {
      jobId: job.jobId
    });
    setJob(snapshot);
    settleWaiter(snapshot);
    return snapshot;
  }, [job, settleWaiter]);

  const startJobAndWait = useCallback(
    async (
      request: unknown,
      onStarted?: (snapshot: PdfJobSnapshot<TResult>) => void
    ): Promise<TResult> => {
      const snapshot = await startJob(request);
      onStarted?.(snapshot);
      return new Promise<TResult>((resolve, reject) => {
        waiters.current.set(snapshot.jobId, { reject, resolve });
        settleWaiter(snapshot);
      });
    },
    [settleWaiter, startJob]
  );

  const clearJob = useCallback(() => {
    if (isActivePdfJob(jobRef.current)) {
      return;
    }
    removeStoredJobId(storageKey);
    setJob(null);
    setConnectionError(null);
  }, [storageKey]);

  return {
    cancelJob,
    clearJob,
    connectionError,
    isActive: isActivePdfJob(job),
    job,
    recoveryComplete,
    startJob,
    startJobAndWait
  };
}

function readStoredJobId(key: string) {
  try {
    return window.sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStoredJobId(key: string, jobId: string) {
  try {
    window.sessionStorage.setItem(key, jobId);
  } catch {
    // Job reattachment still works through the native active-job list.
  }
}

function removeStoredJobId(key: string) {
  try {
    window.sessionStorage.removeItem(key);
  } catch {
    // The in-memory job remains usable when browser storage is unavailable.
  }
}

function normaliseStorageScope(scope?: string) {
  return scope
    ?.toLowerCase()
    .replace(/[^a-z0-9-]+/gu, "-")
    .replace(/^-+|-+$/gu, "")
    .slice(0, 64);
}
