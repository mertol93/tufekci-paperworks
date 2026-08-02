import { invoke } from "@tauri-apps/api/core";
import {
  GlobalWorkerOptions,
  PasswordResponses,
  PDFDataRangeTransport,
  getDocument,
  type PDFDocumentLoadingTask,
  type PDFDocumentProxy
} from "pdfjs-dist";
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { classifyPdfRangeFailure, type PdfOpenErrorCode } from "./pdfPassword";

GlobalWorkerOptions.workerSrc = pdfWorkerUrl;

export type PdfMemorySource = {
  data: ArrayBuffer;
  name: string;
  size: number;
};

export type PdfRangeSource = {
  initialData: number[];
  modifiedAtMs: number | null;
  name: string;
  path: string;
  size: number;
};

export type PdfSource = PdfMemorySource | PdfRangeSource;

export type PdfLoadProgress = {
  loaded: number;
  total?: number;
};

export function createPdfLoadingTask(
  source: PdfSource,
  password?: string | null
): PDFDocumentLoadingTask {
  const assetBaseUrl = new URL("pdfjs/", document.baseURI).toString();
  const commonOptions = {
    cMapPacked: true,
    cMapUrl: `${assetBaseUrl}cmaps/`,
    enableXfa: false,
    iccUrl: `${assetBaseUrl}iccs/`,
    password: password || undefined,
    standardFontDataUrl: `${assetBaseUrl}standard_fonts/`,
    stopAtErrors: false,
    wasmUrl: `${assetBaseUrl}wasm/`
  };

  if ("data" in source) {
    return getDocument({
      ...commonOptions,
      data: new Uint8Array(source.data.slice(0))
    });
  }

  const transport = new TauriPdfRangeTransport(source);
  const task = getDocument({
    ...commonOptions,
    disableAutoFetch: true,
    disableStream: true,
    range: transport,
    rangeChunkSize: PDF_RANGE_CHUNK_SIZE
  });
  transport.attach(task);
  rangeTransports.set(task, transport);
  return task;
}

export function isIncorrectPasswordReason(reason: number) {
  return reason === PasswordResponses.INCORRECT_PASSWORD;
}

export function pdfRangeFailure(task: PDFDocumentLoadingTask): PdfOpenErrorCode | null {
  return rangeTransports.get(task)?.failure ?? null;
}

const PDF_RANGE_CHUNK_SIZE = 64 * 1024;
const rangeTransports = new WeakMap<PDFDocumentLoadingTask, TauriPdfRangeTransport>();

class TauriPdfRangeTransport extends PDFDataRangeTransport {
  failure: PdfOpenErrorCode | null = null;
  private aborted = false;
  private task: PDFDocumentLoadingTask | null = null;

  constructor(private readonly source: PdfRangeSource) {
    super(
      source.size,
      new Uint8Array(source.initialData),
      true,
      source.name
    );
  }

  attach(task: PDFDocumentLoadingTask) {
    this.task = task;
  }

  requestDataRange(begin: number, end: number) {
    if (this.aborted) {
      return;
    }
    void invoke<ArrayBuffer>("read_local_pdf_range", {
      request: {
        begin,
        end,
        expectedModifiedAtMs: this.source.modifiedAtMs,
        expectedSize: this.source.size,
        path: this.source.path
      }
    })
      .then((data) => {
        if (!this.aborted) {
          this.onDataRange(begin, new Uint8Array(data));
        }
      })
      .catch((reason: unknown) => {
        if (this.aborted) {
          return;
        }
        this.failure = classifyPdfRangeFailure(reason);
        this.onDataRange(begin, null);
        void this.task?.destroy();
      });
  }

  abort() {
    this.aborted = true;
    this.task = null;
  }
}

export type { PDFDocumentProxy };
