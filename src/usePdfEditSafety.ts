import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isActivePdfJob, type PdfJobSnapshot } from "./pdfJobs";
import {
  boundedEditSafetyError,
  checksFromInspection,
  failedChecks,
  toPendingCheck,
  type PdfEditSafetyCheck,
  type PdfEditSafetyInspectionResult,
  type PdfEditSafetySource
} from "./pdfEditSafety";
import { usePdfJob } from "./usePdfJob";

export type {
  PdfEditSafetyCheck,
  PdfEditSafetyInspectionResult,
  PdfEditSafetyResult,
  PdfEditSafetySource
} from "./pdfEditSafety";

type SafetyBatch = {
  checks: PdfEditSafetyCheck[];
  token: symbol;
};

type ActiveSafetyJob = {
  jobId: string;
  token: symbol;
};

const JOB_STATUS_POLL_MS = 200;
const MAX_STATUS_FAILURES = 8;

export function usePdfEditSafety(
  desktopMode: boolean,
  sources: PdfEditSafetySource[],
  storageScope: string,
  delayMs = 350
) {
  const [retrySequence, setRetrySequence] = useState(0);
  const token = useMemo(
    () => Symbol("pdf-edit-safety"),
    [desktopMode, retrySequence, sources, storageScope]
  );
  const [batch, setBatch] = useState<SafetyBatch | null>(null);
  const [activeJob, setActiveJob] = useState<ActiveSafetyJob | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const execution = useRef<Promise<void>>(Promise.resolve());
  const currentJobId = useRef<string | null>(null);
  const editSafetyJob = usePdfJob<PdfEditSafetyInspectionResult>(
    desktopMode,
    "edit-safety-inspection",
    storageScope
  );
  const recoveredJob = useRef(editSafetyJob.job);
  recoveredJob.current = editSafetyJob.job;

  useEffect(() => {
    if (!desktopMode || sources.length === 0) {
      setBatch({ checks: [], token });
      setActiveJob(null);
      setCancelling(false);
      return;
    }

    let active = true;
    const pendingChecks = sources.map(toPendingCheck);
    setBatch({ checks: pendingChecks, token });
    setActiveJob(null);
    setCancelling(false);
    if (!editSafetyJob.recoveryComplete) {
      return () => {
        active = false;
      };
    }

    const timer = window.setTimeout(() => {
      const task = execution.current
        .catch(() => undefined)
        .then(async () => {
          if (!active) {
            return;
          }

          const previousJob = recoveredJob.current;
          if (isActivePdfJob(previousJob)) {
            await cancelPdfJobAndWait(previousJob.jobId, () => active);
          }
          if (!active) {
            return;
          }

          let startedJobId: string | null = null;
          try {
            const result = await editSafetyJob.startJobAndWait(
              {
                sources: sources.map((source) => ({
                  inputPassword: source.password || null,
                  inputPath: source.path
                }))
              },
              (snapshot) => {
                startedJobId = snapshot.jobId;
                currentJobId.current = snapshot.jobId;
                if (active) {
                  setActiveJob({ jobId: snapshot.jobId, token });
                } else {
                  void requestPdfJobCancellation(snapshot.jobId);
                }
              }
            );
            if (active) {
              setBatch({ checks: checksFromInspection(sources, result), token });
            }
          } catch (reason) {
            if (active) {
              setBatch({ checks: failedChecks(sources, errorMessage(reason)), token });
            }
          } finally {
            if (currentJobId.current === startedJobId) {
              currentJobId.current = null;
            }
            if (active) {
              setCancelling(false);
            }
          }
        });
      execution.current = task;
    }, delayMs);

    return () => {
      active = false;
      window.clearTimeout(timer);
      const jobId = currentJobId.current;
      if (jobId) {
        void requestPdfJobCancellation(jobId);
      }
    };
  }, [
    delayMs,
    desktopMode,
    editSafetyJob.recoveryComplete,
    editSafetyJob.startJobAndWait,
    sources,
    token
  ]);

  const checks = batch?.token === token ? batch.checks : sources.map(toPendingCheck);
  const errors = checks.filter(
    (check): check is PdfEditSafetyCheck & { error: string } =>
      check.status === "error" && Boolean(check.error)
  );
  const signedSources = checks.filter(
    (check) => check.status === "ready" && check.result?.certificateSignature
  );
  const job =
    activeJob?.token === token && editSafetyJob.job?.jobId === activeJob.jobId
      ? editSafetyJob.job
      : null;
  const isReady =
    desktopMode &&
    sources.length > 0 &&
    batch?.token === token &&
    checks.length === sources.length &&
    checks.every((check) => check.status === "ready");
  const isChecking =
    desktopMode &&
    sources.length > 0 &&
    (batch?.token !== token ||
      !editSafetyJob.recoveryComplete ||
      checks.some((check) => check.status === "checking") ||
      isActivePdfJob(job));

  const cancelJob = useCallback(async () => {
    if (!job || !isActivePdfJob(job)) {
      return;
    }
    setCancelling(true);
    try {
      await requestPdfJobCancellation(job.jobId);
    } catch {
      setCancelling(false);
    }
  }, [job]);

  const retry = useCallback(() => {
    setRetrySequence((value) => value + 1);
  }, []);

  return {
    cancelJob,
    cancelling,
    checks,
    connectionError: editSafetyJob.connectionError,
    errors,
    isChecking,
    isReady,
    job,
    retry,
    signedSources
  };
}

export type PdfEditSafetyState = ReturnType<typeof usePdfEditSafety>;

async function requestPdfJobCancellation(jobId: string) {
  return invoke<PdfJobSnapshot<unknown>>("cancel_pdf_job", { jobId });
}

async function cancelPdfJobAndWait(jobId: string, shouldContinue: () => boolean) {
  try {
    const cancelled = await requestPdfJobCancellation(jobId);
    if (!isActivePdfJob(cancelled)) {
      return;
    }
  } catch {
    return;
  }

  let failures = 0;
  while (shouldContinue()) {
    await wait(JOB_STATUS_POLL_MS);
    try {
      const snapshot = await invoke<PdfJobSnapshot<unknown>>("get_pdf_job", { jobId });
      failures = 0;
      if (!isActivePdfJob(snapshot)) {
        return;
      }
    } catch {
      failures += 1;
      if (failures >= MAX_STATUS_FAILURES) {
        return;
      }
    }
  }
}

function wait(milliseconds: number) {
  return new Promise<void>((resolve) => window.setTimeout(resolve, milliseconds));
}

function errorMessage(reason: unknown) {
  return boundedEditSafetyError(reason instanceof Error ? reason.message : String(reason));
}
