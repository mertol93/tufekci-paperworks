export type FormFieldKind =
  | "text"
  | "checkbox"
  | "radio"
  | "choice"
  | "button"
  | "signature"
  | "unsupported";

export type FormOption = {
  label: string;
  value: string;
};

export type FormWidget = {
  exportValue: string | null;
  pageNumber: number | null;
  rect: { height: number; width: number; x: number; y: number } | null;
  widgetId: string;
};

export type FormField = {
  combo: boolean;
  editable: boolean;
  editableChoice: boolean;
  fieldId: string;
  flattenable: boolean;
  kind: FormFieldKind;
  maxLength: number | null;
  multiline: boolean;
  multiSelect: boolean;
  name: string;
  options: FormOption[];
  password: boolean;
  readOnly: boolean;
  required: boolean;
  signaturePresent: boolean;
  values: string[];
  widgets: FormWidget[];
};

export type FormValues = Record<string, string[]>;

export type FormHistory = {
  future: FormValues[];
  past: FormValues[];
  present: FormValues;
};

export type FormFieldUpdate = {
  fieldId: string;
  values: string[];
};

const HISTORY_LIMIT = 100;

export function initialFormValues(fields: FormField[]): FormValues {
  return Object.fromEntries(fields.map((field) => [field.fieldId, [...field.values]]));
}

export function createFormHistory(fields: FormField[]): FormHistory {
  return { future: [], past: [], present: initialFormValues(fields) };
}

export function commitFormValues(history: FormHistory, next: FormValues): FormHistory {
  if (formValuesEqual(history.present, next)) {
    return history;
  }
  return {
    future: [],
    past: [...history.past.slice(-(HISTORY_LIMIT - 1)), history.present],
    present: next
  };
}

export function updateFormField(
  history: FormHistory,
  fieldId: string,
  values: string[]
): FormHistory {
  return commitFormValues(history, { ...history.present, [fieldId]: [...values] });
}

export function undoFormValues(history: FormHistory): FormHistory {
  const previous = history.past[history.past.length - 1];
  if (!previous) {
    return history;
  }
  return {
    future: [history.present, ...history.future].slice(0, HISTORY_LIMIT),
    past: history.past.slice(0, -1),
    present: previous
  };
}

export function redoFormValues(history: FormHistory): FormHistory {
  const next = history.future[0];
  if (!next) {
    return history;
  }
  return {
    future: history.future.slice(1),
    past: [...history.past.slice(-(HISTORY_LIMIT - 1)), history.present],
    present: next
  };
}

export function changedFormUpdates(
  fields: FormField[],
  values: FormValues
): FormFieldUpdate[] {
  return fields
    .filter(
      (field) =>
        field.editable && !stringArraysEqual(field.values, values[field.fieldId] ?? [])
    )
    .map((field) => ({ fieldId: field.fieldId, values: [...(values[field.fieldId] ?? [])] }));
}

export function validateFormDraft(fields: FormField[], values: FormValues) {
  const errors: Record<string, string> = {};
  for (const field of fields.filter((field) => field.editable)) {
    const fieldValues = values[field.fieldId] ?? [];
    const nonEmpty = fieldValues.filter((value) => value.length > 0);
    if (field.required && nonEmpty.length === 0) {
      errors[field.fieldId] = "This required field cannot be empty.";
      continue;
    }
    if (field.kind === "text") {
      if (
        field.maxLength !== null &&
        fieldValues[0] &&
        Array.from(fieldValues[0]).length > field.maxLength
      ) {
        errors[field.fieldId] = `Use at most ${field.maxLength} characters.`;
      } else if (!field.multiline && fieldValues[0]?.match(/[\r\n]/)) {
        errors[field.fieldId] = "This field does not allow multiple lines.";
      }
      continue;
    }
    if ((field.kind === "checkbox" || field.kind === "radio") && fieldValues.length > 1) {
      errors[field.fieldId] = "Choose one value.";
      continue;
    }
    if (field.kind === "choice") {
      if (!field.multiSelect && fieldValues.length > 1) {
        errors[field.fieldId] = "Choose one value.";
        continue;
      }
      if (
        !field.editableChoice &&
        nonEmpty.some(
          (value) => !field.options.some((option) => option.value === value)
        )
      ) {
        errors[field.fieldId] = "Choose a listed option.";
      }
    }
  }
  return errors;
}

export function formFieldLabel(kind: FormFieldKind) {
  switch (kind) {
    case "button":
      return "Push button";
    case "checkbox":
      return "Checkbox";
    case "choice":
      return "Choice";
    case "radio":
      return "Radio group";
    case "signature":
      return "Signature";
    case "text":
      return "Text";
    case "unsupported":
      return "Unsupported";
  }
}

function formValuesEqual(left: FormValues, right: FormValues) {
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  return [...keys].every((key) => stringArraysEqual(left[key] ?? [], right[key] ?? []));
}

function stringArraysEqual(left: string[], right: string[]) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}
