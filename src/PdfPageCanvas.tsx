import { useEffect, useRef, useState } from "react";
import { FileWarning, Loader2 } from "lucide-react";
import { AnnotationMode, TextLayer } from "pdfjs-dist";
import {
  AnnotationLayerBuilder,
  EventBus,
  LinkTarget,
  SimpleLinkService
} from "pdfjs-dist/web/pdf_viewer.mjs";
import { useI18n } from "./I18nProvider";
import { type PDFDocumentProxy } from "./pdf";
import { type PageRotation } from "./usePagePlan";

type PdfPageCanvasProps = {
  document: PDFDocumentProxy;
  hiddenAnnotationIds?: string[];
  pageNumber: number;
  rotation?: PageRotation;
  scale?: number;
  targetWidth?: number;
  variant: "page" | "thumbnail";
};

export function PdfPageCanvas({
  document,
  hiddenAnnotationIds,
  pageNumber,
  rotation = 0,
  scale = 1,
  targetWidth,
  variant
}: PdfPageCanvasProps) {
  const { formatNumber, t } = useI18n();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const textLayerRef = useRef<HTMLDivElement>(null);
  const annotationLayerRef = useRef<HTMLDivElement>(null);
  const [renderFailed, setRenderFailed] = useState(false);
  const [rendering, setRendering] = useState(true);
  const hiddenAnnotationIdsKey = hiddenAnnotationIds?.join("\u0000") ?? "";

  useEffect(() => {
    let alive = true;
    let renderTask: { cancel: () => void; promise: Promise<void> } | null = null;
    let textLayer: TextLayer | null = null;
    let annotationLayer: AnnotationLayerBuilder | null = null;

    setRenderFailed(false);
    setRendering(true);

    document
      .getPage(pageNumber)
      .then((page) => {
        if (!alive || !canvasRef.current) {
          return;
        }

        const appliedRotation = (page.rotate + rotation) % 360;
        const unscaledViewport = page.getViewport({ rotation: appliedRotation, scale: 1 });
        const renderScale = targetWidth ? targetWidth / unscaledViewport.width : scale;
        const viewport = page.getViewport({ rotation: appliedRotation, scale: renderScale });
        const outputScale = Math.min(window.devicePixelRatio || 1, 2);
        const canvas = canvasRef.current;

        canvas.width = Math.max(1, Math.floor(viewport.width * outputScale));
        canvas.height = Math.max(1, Math.floor(viewport.height * outputScale));
        canvas.style.width = `${Math.floor(viewport.width)}px`;
        canvas.style.height = `${Math.floor(viewport.height)}px`;

        renderTask = page.render({
          annotationMode: hiddenAnnotationIdsKey ? AnnotationMode.DISABLE : AnnotationMode.ENABLE,
          canvas,
          transform:
            outputScale === 1 ? undefined : [outputScale, 0, 0, outputScale, 0, 0],
          viewport
        });

        if (variant === "page" && textLayerRef.current && annotationLayerRef.current) {
          const container = textLayerRef.current;
          const annotationHost = annotationLayerRef.current;
          container.replaceChildren();
          annotationHost.replaceChildren();
          container.style.setProperty("--total-scale-factor", String(viewport.scale));
          container.style.width = `${Math.floor(viewport.width)}px`;
          container.style.height = `${Math.floor(viewport.height)}px`;
          annotationHost.style.setProperty("--total-scale-factor", String(viewport.scale));
          annotationHost.style.width = `${Math.floor(viewport.width)}px`;
          annotationHost.style.height = `${Math.floor(viewport.height)}px`;
          textLayer = new TextLayer({
            container,
            textContentSource: page.streamTextContent(),
            viewport
          });

          const linkService = new SimpleLinkService({
            eventBus: new EventBus(),
            externalLinkRel: "noopener noreferrer nofollow",
            externalLinkTarget: LinkTarget.BLANK
          });
          linkService.externalLinkEnabled = false;
          linkService.setDocument(document);
          annotationLayer = new AnnotationLayerBuilder({
            annotationStorage: document.annotationStorage,
            enableComment: false,
            enableScripting: false,
            fieldObjectsPromise: Promise.resolve(null),
            hasJSActionsPromise: Promise.resolve(false),
            imageResourcesPath: new URL("pdfjs/images/", window.document.baseURI).toString(),
            linkService,
            onAppend: (layer: HTMLDivElement) => {
              if (alive) {
                annotationHost.replaceChildren(layer);
              }
            },
            pdfPage: page,
            renderForms: true
          });

          const annotationPromise = annotationLayer
            .render({
              intent: "display",
              optionalContentConfigPromise: document.getOptionalContentConfig({
                intent: "display"
              }),
              viewport
            })
            .then(() => {
              if (!alive) {
                return;
              }
              annotationHost
                .querySelectorAll<HTMLElement>("a, button, input, select, textarea")
                .forEach((control) => {
                  control.inert = true;
                  control.setAttribute("aria-disabled", "true");
                  control.setAttribute("tabindex", "-1");
                  control.title = t("pdfCanvas.displayOnly");
                });
              if (hiddenAnnotationIdsKey) {
                const hiddenIds = new Set(hiddenAnnotationIdsKey.split("\u0000"));
                annotationHost
                  .querySelectorAll<HTMLElement>("[data-annotation-id]")
                  .forEach((annotation) => {
                    if (hiddenIds.has(annotation.dataset.annotationId ?? "")) {
                      annotation.hidden = true;
                    }
                  });
              }
            })
            .catch(() => {
              if (alive) {
                annotationHost.replaceChildren();
              }
            });

          return Promise.all([renderTask.promise, textLayer.render(), annotationPromise]).then(
            () => undefined
          );
        }

        return renderTask.promise;
      })
      .then(() => {
        if (alive) {
          setRendering(false);
        }
      })
      .catch((reason: unknown) => {
        if (!alive || (reason instanceof Error && reason.name === "RenderingCancelledException")) {
          return;
        }

        setRenderFailed(true);
        setRendering(false);
      });

    return () => {
      alive = false;
      renderTask?.cancel();
      textLayer?.cancel();
      annotationLayer?.cancel();
    };
  }, [
    document,
    hiddenAnnotationIdsKey,
    pageNumber,
    rotation,
    scale,
    targetWidth,
    t,
    variant
  ]);

  return (
    <span className={`pdf-canvas-container is-${variant}`}>
      <canvas
        aria-label={t("pdfCanvas.pageAria", { page: formatNumber(pageNumber) })}
        className="pdf-page-canvas"
        ref={canvasRef}
      />
      {variant === "page" ? <div className="pdf-text-layer" ref={textLayerRef} /> : null}
      {variant === "page" ? (
        <div
          aria-label={t("pdfCanvas.annotationLayerAria")}
          className="pdf-annotation-layer"
          ref={annotationLayerRef}
          role="group"
        />
      ) : null}
      {rendering ? (
        <span className="pdf-render-state" role={variant === "page" ? "status" : undefined}>
          <Loader2 className="spin" size={variant === "page" ? 24 : 15} aria-hidden="true" />
          <span className="visually-hidden">
            {t("pdfCanvas.rendering", { page: formatNumber(pageNumber) })}
          </span>
        </span>
      ) : null}
      {renderFailed ? (
        <span
          className="pdf-render-state is-error"
          role={variant === "page" ? "alert" : undefined}
          title={t("pdfCanvas.error")}
        >
          <FileWarning size={variant === "page" ? 24 : 15} aria-hidden="true" />
          {variant === "page" ? <span>{t("pdfCanvas.error")}</span> : null}
        </span>
      ) : null}
    </span>
  );
}

type LazyPdfThumbnailProps = {
  document: PDFDocumentProxy;
  pageNumber: number;
  rotation?: PageRotation;
};

export function LazyPdfThumbnail({ document, pageNumber, rotation = 0 }: LazyPdfThumbnailProps) {
  const containerRef = useRef<HTMLSpanElement>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const element = containerRef.current;

    if (!element || visible) {
      return;
    }

    if (!("IntersectionObserver" in window)) {
      setVisible(true);
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setVisible(true);
          observer.disconnect();
        }
      },
      { rootMargin: "240px 0px" }
    );

    observer.observe(element);
    return () => observer.disconnect();
  }, [visible]);

  return (
    <span className="pdf-thumbnail-canvas" ref={containerRef}>
      {visible ? (
        <PdfPageCanvas
          document={document}
          pageNumber={pageNumber}
          rotation={rotation}
          targetWidth={62}
          variant="thumbnail"
        />
      ) : (
        <span className="thumbnail-sheet" aria-hidden="true">
          <span />
          <span />
          <span />
        </span>
      )}
    </span>
  );
}
