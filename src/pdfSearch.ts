export type PdfSearchErrorCode = "text-unavailable";

export function classifyPdfSearchError(_reason: unknown): PdfSearchErrorCode {
  return "text-unavailable";
}

export function normalisePdfSearchText(value: string, locale: string) {
  return value.normalize("NFKC").trim().toLocaleLowerCase(locale);
}

export function countPdfSearchOccurrences(text: string, query: string) {
  if (!query) {
    return 0;
  }

  let count = 0;
  let cursor = 0;

  while (cursor <= text.length - query.length) {
    const match = text.indexOf(query, cursor);
    if (match === -1) {
      break;
    }

    count += 1;
    cursor = match + query.length;
  }

  return count;
}
