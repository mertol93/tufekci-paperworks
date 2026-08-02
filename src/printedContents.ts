export const PRINTED_CONTENTS_ENTRIES_PER_PAGE = 38;
export const MAX_PRINTED_CONTENTS_PAGES = 64;
export const MAX_PRINTED_CONTENTS_LEVEL = 6;
export const MAX_PRINTED_CONTENTS_TITLE_CHARACTERS = 128;
export const MAX_PRINTED_CONTENTS_TITLE_BYTES = 512;

export type PrintedContentsBookmark = {
  level: number;
  pageNumber: number | null;
  title: string;
};

export type PrintedContentsDraft = {
  addBookmark: boolean;
  enabled: boolean;
  maximumLevel: number;
  title: string;
};

export type PdfPrintedContents = {
  addBookmark: boolean;
  maximumLevel: number;
  title: string;
};

export function createPrintedContentsDraft(title = "Contents"): PrintedContentsDraft {
  return {
    addBookmark: true,
    enabled: false,
    maximumLevel: 2,
    title
  };
}

export function selectPrintedContentsEntries<T extends PrintedContentsBookmark>(
  bookmarks: readonly T[],
  maximumLevel: number
) {
  return bookmarks.filter((bookmark) => bookmark.level <= maximumLevel);
}

export function estimatePrintedContentsPageCount(entryCount: number) {
  return entryCount > 0
    ? Math.ceil(entryCount / PRINTED_CONTENTS_ENTRIES_PER_PAGE)
    : 0;
}

export function printedContentsValidationMessage(
  draft: PrintedContentsDraft,
  bookmarks: readonly PrintedContentsBookmark[]
) {
  if (!draft.enabled) {
    return null;
  }
  const title = draft.title.trim();
  if (!title) {
    return "Enter a title for the printed contents pages.";
  }
  if (
    [...title].length > MAX_PRINTED_CONTENTS_TITLE_CHARACTERS ||
    new TextEncoder().encode(title).length > MAX_PRINTED_CONTENTS_TITLE_BYTES
  ) {
    return `The contents title must contain at most ${MAX_PRINTED_CONTENTS_TITLE_CHARACTERS} characters.`;
  }
  if (/\p{Cc}/u.test(title)) {
    return "The contents title cannot contain control characters.";
  }
  if (
    !Number.isInteger(draft.maximumLevel) ||
    draft.maximumLevel < 0 ||
    draft.maximumLevel > MAX_PRINTED_CONTENTS_LEVEL
  ) {
    return "Choose a valid bookmark level for the printed contents.";
  }
  const entryCount = selectPrintedContentsEntries(bookmarks, draft.maximumLevel).length;
  if (entryCount === 0) {
    return "Add a bookmark at one of the included levels before creating printed contents.";
  }
  if (estimatePrintedContentsPageCount(entryCount) > MAX_PRINTED_CONTENTS_PAGES) {
    return `Printed contents can contain at most ${MAX_PRINTED_CONTENTS_PAGES} pages. Include fewer bookmark levels.`;
  }
  return null;
}

export function printedContentsIsValid(
  draft: PrintedContentsDraft,
  bookmarks: readonly PrintedContentsBookmark[]
) {
  return printedContentsValidationMessage(draft, bookmarks) === null;
}

export function toPdfPrintedContents(
  draft: PrintedContentsDraft,
  bookmarks: readonly PrintedContentsBookmark[]
): PdfPrintedContents | null {
  if (!draft.enabled) {
    return null;
  }
  const validationMessage = printedContentsValidationMessage(draft, bookmarks);
  if (validationMessage) {
    throw new Error(validationMessage);
  }
  return {
    addBookmark: draft.addBookmark,
    maximumLevel: draft.maximumLevel,
    title: draft.title.trim()
  };
}
