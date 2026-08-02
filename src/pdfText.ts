import type { PDFDocumentProxy, PDFPageProxy } from "pdfjs-dist";

type PdfTextDocument = Pick<PDFDocumentProxy, "getPage">;
type PdfTextPage = Pick<PDFPageProxy, "getTextContent">;
type PdfTextContent = Awaited<ReturnType<PdfTextPage["getTextContent"]>>;

const textContentCache = new WeakMap<
  PdfTextDocument,
  Map<number, Promise<PdfTextContent>>
>();

export function getPdfPageTextContent(
  document: PdfTextDocument,
  pageNumber: number,
  page?: PdfTextPage
) {
  let documentCache = textContentCache.get(document);
  if (!documentCache) {
    documentCache = new Map();
    textContentCache.set(document, documentCache);
  }

  const cached = documentCache.get(pageNumber);
  if (cached) {
    return cached;
  }

  const pagePromise = page ? Promise.resolve(page) : document.getPage(pageNumber);
  let pending: Promise<PdfTextContent>;
  pending = pagePromise
    .then((loadedPage) => loadedPage.getTextContent({ disableNormalization: false }))
    .catch((reason: unknown) => {
      if (documentCache?.get(pageNumber) === pending) {
        documentCache.delete(pageNumber);
      }
      throw reason;
    });
  documentCache.set(pageNumber, pending);
  return pending;
}

export function extractPdfPageText(content: PdfTextContent) {
  return content.items
    .map((item) => ("str" in item ? item.str : ""))
    .join(" ")
    .replace(/\s+/g, " ")
    .trim();
}
