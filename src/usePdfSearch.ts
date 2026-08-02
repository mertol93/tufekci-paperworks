import { useEffect, useState } from "react";
import { type PDFDocumentProxy } from "./pdf";
import { extractPdfPageText, getPdfPageTextContent } from "./pdfText";
import {
  classifyPdfSearchError,
  countPdfSearchOccurrences,
  normalisePdfSearchText,
  type PdfSearchErrorCode
} from "./pdfSearch";

export type { PdfSearchErrorCode } from "./pdfSearch";

export type PdfSearchMatch = {
  count: number;
  pageNumber: number;
};

export type PdfSearchPage = {
  document: PDFDocumentProxy | null;
  sourcePage: number;
};

export function usePdfSearch(pages: PdfSearchPage[], query: string, locale: string) {
  const [matches, setMatches] = useState<PdfSearchMatch[]>([]);
  const [pagesSearched, setPagesSearched] = useState(0);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<PdfSearchErrorCode | null>(null);

  useEffect(() => {
    const normalisedQuery = normalisePdfSearchText(query, locale);

    if (pages.length === 0 || normalisedQuery.length < 2) {
      setMatches([]);
      setPagesSearched(0);
      setSearching(false);
      setError(null);
      return;
    }

    let alive = true;
    const found: PdfSearchMatch[] = [];

    setMatches([]);
    setPagesSearched(0);
    setSearching(true);
    setError(null);

    const search = async () => {
      for (let index = 0; index < pages.length; index += 1) {
        if (!alive) {
          return;
        }

        const plannedPage = pages[index];
        const text = plannedPage.document
          ? await getPageText(plannedPage.document, plannedPage.sourcePage)
          : "";
        const count = countPdfSearchOccurrences(
          normalisePdfSearchText(text, locale),
          normalisedQuery
        );

        if (count > 0) {
          found.push({ count, pageNumber: index + 1 });
        }

        if (alive) {
          setPagesSearched(index + 1);
          if (count > 0 || index + 1 === pages.length || (index + 1) % 8 === 0) {
            setMatches([...found]);
          }
        }
      }
    };

    const timer = window.setTimeout(() => {
      search()
        .catch((reason: unknown) => {
          if (alive) {
            setError(classifyPdfSearchError(reason));
          }
        })
        .finally(() => {
          if (alive) {
            setSearching(false);
          }
        });
    }, 240);

    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [locale, pages, query]);

  const totalMatches = matches.reduce((total, match) => total + match.count, 0);

  return {
    error,
    matches,
    pagesSearched,
    searching,
    totalMatches,
    totalPages: pages.length
  };
}

async function getPageText(
  document: PDFDocumentProxy,
  pageNumber: number
) {
  return extractPdfPageText(await getPdfPageTextContent(document, pageNumber));
}
