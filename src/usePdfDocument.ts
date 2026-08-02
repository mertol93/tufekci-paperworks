import { useEffect, useState } from "react";
import {
  createPdfLoadingTask,
  isIncorrectPasswordReason,
  type PdfLoadProgress,
  pdfRangeFailure,
  type PDFDocumentProxy,
  type PdfSource
} from "./pdf";
import {
  classifyPdfOpenError,
  type PdfOpenErrorCode,
  type PdfPasswordRequest,
  validPdfOpeningPasswordInput
} from "./pdfPassword";

export function usePdfDocument(source: PdfSource | null) {
  const [document, setDocument] = useState<PDFDocumentProxy | null>(null);
  const [error, setError] = useState<PdfOpenErrorCode | null>(null);
  const [loading, setLoading] = useState(false);
  const [openingPassword, setOpeningPassword] = useState<string | null>(null);
  const [passwordRequest, setPasswordRequest] = useState<PdfPasswordRequest | null>(null);
  const [progress, setProgress] = useState<PdfLoadProgress | null>(null);

  useEffect(() => {
    setDocument(null);
    setError(null);
    setPasswordRequest(null);
    setOpeningPassword(null);
    setProgress(null);

    if (!source) {
      setLoading(false);
      return;
    }

    let alive = true;
    let cancelledByUser = false;
    const task = createPdfLoadingTask(source);

    setLoading(true);

    task.onProgress = ({ loaded, total }: { loaded: number; total?: number }) => {
      if (alive) {
        setProgress({ loaded, total });
      }
    };

    task.onPassword = (updatePassword: (password: string) => void, reason: number) => {
      if (!alive) {
        return;
      }

      setPasswordRequest({
        incorrect: isIncorrectPasswordReason(reason),
        cancel: () => {
          cancelledByUser = true;
          setPasswordRequest(null);
          setError("cancelled");
          setLoading(false);
          void task.destroy();
        },
        submit: (password: string) => {
          if (!password || !validPdfOpeningPasswordInput(password)) {
            return;
          }
          setOpeningPassword(password);
          updatePassword(password);
        }
      });
    };

    task.promise
      .then((loadedDocument) => {
        if (!alive || cancelledByUser) {
          return;
        }

        setPasswordRequest(null);
        setDocument(loadedDocument);
        setProgress(null);
      })
      .catch((reason: unknown) => {
        if (alive && !cancelledByUser) {
          setPasswordRequest(null);
          setError(classifyPdfOpenError(reason, pdfRangeFailure(task)));
        }
      })
      .finally(() => {
        if (alive && !cancelledByUser) {
          setLoading(false);
        }
      });

    return () => {
      alive = false;
      setPasswordRequest(null);
      void task.destroy();
    };
  }, [source]);

  return { document, error, loading, openingPassword, passwordRequest, progress };
}
