import type { PDFDocumentProxy, PDFPageProxy } from "pdfjs-dist";

type PdfTextDocument = Pick<PDFDocumentProxy, "getPage">;
type PdfTextContent = Awaited<ReturnType<PDFPageProxy["getTextContent"]>>;

const pageTextCache = new WeakMap<PdfTextDocument, Map<number, Promise<string>>>();

export function cacheRenderedPdfPageText(
  document: PdfTextDocument,
  pageNumber: number,
  renderedText: Promise<string>
) {
  const documentCache = getDocumentCache(document);
  const cached = documentCache.get(pageNumber);
  if (cached) {
    void renderedText.catch(() => undefined);
    return cached;
  }

  return cachePageText(documentCache, pageNumber, renderedText);
}

export function getPdfPageText(document: PdfTextDocument, pageNumber: number) {
  const documentCache = getDocumentCache(document);

  const cached = documentCache.get(pageNumber);
  if (cached) {
    return cached;
  }

  const pending = document
    .getPage(pageNumber)
    .then((loadedPage) => loadedPage.getTextContent({ disableNormalization: false }))
    .then(extractPdfPageText);
  return cachePageText(documentCache, pageNumber, pending);
}

function getDocumentCache(document: PdfTextDocument) {
  let documentCache = pageTextCache.get(document);
  if (!documentCache) {
    documentCache = new Map();
    pageTextCache.set(document, documentCache);
  }
  return documentCache;
}

function cachePageText(
  documentCache: Map<number, Promise<string>>,
  pageNumber: number,
  text: Promise<string>
) {
  let pending: Promise<string>;
  pending = text.catch((reason: unknown) => {
    if (documentCache.get(pageNumber) === pending) {
      documentCache.delete(pageNumber);
    }
    throw reason;
  });
  documentCache.set(pageNumber, pending);
  void pending.catch(() => undefined);
  return pending;
}

export function extractPdfPageText(content: PdfTextContent) {
  return joinPdfTextItems(content.items.map((item) => ("str" in item ? item.str : "")));
}

export function joinPdfTextItems(items: readonly string[]) {
  return items
    .join(" ")
    .replace(/\s+/g, " ")
    .trim();
}
