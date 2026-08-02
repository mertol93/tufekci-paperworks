import type { AnnotationKind } from "./annotationDraft";
import type { Translate, TranslationKey } from "./i18n";

const annotationKindKeys = {
  ellipse: "annotation.kind.ellipse",
  freehand: "annotation.kind.freehand",
  highlight: "annotation.kind.highlight",
  image: "annotation.kind.image",
  line: "annotation.kind.line",
  rectangle: "annotation.kind.rectangle",
  stamp: "annotation.kind.stamp",
  text: "annotation.kind.text"
} as const satisfies Record<AnnotationKind, TranslationKey>;

const annotationStampKeys = {
  APPROVED: "annotation.stamp.approved",
  CONFIDENTIAL: "annotation.stamp.confidential",
  COPY: "annotation.stamp.copy",
  DRAFT: "annotation.stamp.draft",
  REVIEWED: "annotation.stamp.reviewed"
} as const satisfies Record<string, TranslationKey>;

export function localiseAnnotationKind(kind: AnnotationKind, t: Translate) {
  return t(annotationKindKeys[kind]);
}

export function localiseAnnotationStamp(stamp: string, t: Translate) {
  return stamp in annotationStampKeys
    ? t(annotationStampKeys[stamp as keyof typeof annotationStampKeys])
    : stamp;
}

export function localiseAnnotationWarnings(
  warnings: string[],
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  const localised = warnings.map((warning) => {
    const editable = warning.match(
      /^(\d+) existing standard annotations? can be moved, restyled, duplicated, or deleted in this workspace\.$/u
    );
    if (editable) {
      return t(
        Number(editable[1]) === 1
          ? "annotation.warning.editable.one"
          : "annotation.warning.editable.other",
        { count: formatNumber(Number(editable[1])) }
      );
    }

    const readOnly = warning.match(
      /^(\d+) existing annotations? (?:is|are) unsupported, structurally complex, or beyond workspace limits\. (?:It|They) remain visible and are preserved read-only\.$/u
    );
    if (readOnly) {
      return t(
        Number(readOnly[1]) === 1
          ? "annotation.warning.readOnly.one"
          : "annotation.warning.readOnly.other",
        { count: formatNumber(Number(readOnly[1])) }
      );
    }

    const substituted = warning.match(
      /^(\d+) text appearances? contained characters outside the built-in Windows Latin font\. The full Unicode text remains in the annotation contents, while unsupported appearance glyphs use question marks\.$/u
    );
    if (substituted) {
      return t(
        Number(substituted[1]) === 1
          ? "annotation.warning.substituted.one"
          : "annotation.warning.substituted.other",
        { count: formatNumber(Number(substituted[1])) }
      );
    }

    const exactKeys: Record<string, TranslationKey> = {
      "Annotation editing changed the PDF and invalidated its existing certificate signatures.":
        "annotation.warning.signatureInvalidated",
      "Editing annotations rewrites this certificate-signed PDF and invalidates its existing signatures.":
        "annotation.warning.signatureReview",
      "New and updated items use standard PDF annotations and remain editable in compatible readers. Unsupported existing annotations are preserved unchanged.":
        "annotation.warning.standard",
      "The annotated copy is not password-protected. Use Protect to apply new encryption.":
        "annotation.warning.unprotected",
      "The annotated copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application.":
        "annotation.warning.protected"
    };
    return exactKeys[warning] ? t(exactKeys[warning]) : t("annotation.warning.generic");
  });

  return [...new Set(localised)];
}
