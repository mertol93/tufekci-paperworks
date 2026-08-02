export type PdfEditSafetySource = {
  id: string;
  label: string;
  password?: string;
  path: string;
};

export type PdfEditSafetyResult = {
  certificateSignature: boolean;
  encrypted: boolean;
  formFields: boolean;
  pageCount: number;
  sourceModifiedAtMs: number | null;
  sourceSize: number;
  xfa: boolean;
};

export type PdfEditSafetyInspectionItem = {
  error?: string | null;
  result?: PdfEditSafetyResult | null;
  sourceIndex: number;
};

export type PdfEditSafetyInspectionResult = {
  failedCount: number;
  inspectedCount: number;
  items: PdfEditSafetyInspectionItem[];
  sourceCount: number;
};

export type PdfEditSafetyCheck = {
  error?: string;
  id: string;
  label: string;
  path: string;
  result?: PdfEditSafetyResult;
  status: "checking" | "error" | "ready";
};

export function checksFromInspection(
  sources: PdfEditSafetySource[],
  inspection: PdfEditSafetyInspectionResult
): PdfEditSafetyCheck[] {
  if (
    inspection.sourceCount !== sources.length ||
    inspection.items.length !== sources.length ||
    inspection.inspectedCount + inspection.failedCount !== sources.length
  ) {
    return mismatchedChecks(sources);
  }

  const checks = new Array<PdfEditSafetyCheck>(sources.length);
  for (const item of inspection.items) {
    if (
      !Number.isInteger(item.sourceIndex) ||
      item.sourceIndex < 0 ||
      item.sourceIndex >= sources.length ||
      checks[item.sourceIndex]
    ) {
      return mismatchedChecks(sources);
    }
    const source = sources[item.sourceIndex];
    if (item.result && !item.error) {
      checks[item.sourceIndex] = {
        ...toPendingCheck(source),
        result: item.result,
        status: "ready"
      };
    } else if (!item.result && item.error) {
      checks[item.sourceIndex] = {
        ...toPendingCheck(source),
        error: boundedEditSafetyError(item.error),
        status: "error"
      };
    } else {
      return mismatchedChecks(sources);
    }
  }

  return checks.filter(Boolean).length === sources.length
    ? checks
    : mismatchedChecks(sources);
}

export function toPendingCheck(source: PdfEditSafetySource): PdfEditSafetyCheck {
  return {
    id: source.id,
    label: source.label,
    path: source.path,
    status: "checking"
  };
}

export function failedChecks(sources: PdfEditSafetySource[], error: string) {
  return sources.map((source) => ({
    ...toPendingCheck(source),
    error: boundedEditSafetyError(error),
    status: "error" as const
  }));
}

export function boundedEditSafetyError(value: string) {
  return value.slice(0, 4_096);
}

function mismatchedChecks(sources: PdfEditSafetySource[]) {
  return failedChecks(
    sources,
    "The edit-safety result did not match the current source selection. Run the check again."
  );
}
