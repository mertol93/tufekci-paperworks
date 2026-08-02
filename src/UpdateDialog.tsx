import { useEffect, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import {
  AlertCircle,
  CheckCircle2,
  Download,
  Loader2,
  RefreshCw,
  ShieldCheck,
  X
} from "lucide-react";
import { useDialogFocus } from "./accessibility";
import { useI18n } from "./I18nProvider";
import { type Translate } from "./i18n";
import {
  applyUpdateDownloadEvent,
  updateChannelLabel,
  updateProgressPercentage,
  type UpdateDownloadEvent,
  type UpdateProgress
} from "./appUpdates";

type UpdateReadiness = {
  channel: string | null;
  configured: boolean;
  currentVersion: string;
  managedByStore: boolean;
  restartRequired: boolean;
};

type UpdateMetadata = {
  channel: string;
  currentVersion: string;
  version: string;
};

type UpdateStage =
  | "available"
  | "checking"
  | "current"
  | "idle"
  | "installing"
  | "loading"
  | "ready"
  | "restarting"
  | "store"
  | "unavailable";

type UpdateDialogProps = {
  desktopMode: boolean;
  onClose: () => void;
  visible: boolean;
};

const emptyProgress: UpdateProgress = { downloaded: 0, total: null };

export function UpdateDialog({ desktopMode, onClose, visible }: UpdateDialogProps) {
  const { formatNumber, t } = useI18n();
  const [error, setError] = useState<string | null>(null);
  const [metadata, setMetadata] = useState<UpdateMetadata | null>(null);
  const [progress, setProgress] = useState<UpdateProgress>(emptyProgress);
  const [readiness, setReadiness] = useState<UpdateReadiness | null>(null);
  const [stage, setStage] = useState<UpdateStage>("loading");
  const closeDisabled = stage === "installing" || stage === "restarting";
  const dialogRef = useDialogFocus<HTMLElement>({
    active: visible,
    escapeDisabled: closeDisabled,
    onEscape: onClose
  });

  useEffect(() => {
    if (!visible) {
      return;
    }

    if (!desktopMode) {
      setError(null);
      setMetadata(null);
      setProgress(emptyProgress);
      setReadiness(null);
      setStage("unavailable");
      return;
    }

    if (readiness) {
      return;
    }

    let active = true;
    setError(null);
    setMetadata(null);
    setProgress(emptyProgress);
    setStage("loading");
    void invoke<UpdateReadiness>("update_readiness")
      .then((result) => {
        if (!active) {
          return;
        }
        setReadiness(result);
        setStage(
          result.managedByStore
            ? "store"
            : result.restartRequired
              ? "ready"
              : result.configured
                ? "idle"
                : "unavailable"
        );
      })
      .catch(() => {
        if (!active) {
          return;
        }
        setError(t("update.error.configuration"));
        setStage("unavailable");
      });

    return () => {
      active = false;
    };
  }, [desktopMode, readiness, t, visible]);

  if (!visible) {
    return null;
  }

  const checkForUpdate = async () => {
    setError(null);
    setMetadata(null);
    setProgress(emptyProgress);
    setStage("checking");
    try {
      const result = await invoke<UpdateMetadata | null>("check_for_update");
      setMetadata(result);
      setStage(result ? "available" : "current");
    } catch {
      setError(t("update.error.check"));
      setStage("idle");
    }
  };

  const installUpdate = async () => {
    setError(null);
    setProgress(emptyProgress);
    setStage("installing");
    const onEvent = new Channel<UpdateDownloadEvent>((event) => {
      setProgress((current) => applyUpdateDownloadEvent(current, event));
    });
    try {
      await invoke("install_update", { onEvent });
      setProgress((current) => ({
        downloaded: current.total ?? current.downloaded,
        total: current.total
      }));
      setStage("ready");
    } catch {
      setMetadata(null);
      setError(t("update.error.install"));
      setStage("idle");
    }
  };

  const restartAfterUpdate = async () => {
    setError(null);
    setStage("restarting");
    try {
      await invoke("restart_after_update");
    } catch {
      setError(t("update.error.restart"));
      setStage("ready");
    }
  };

  const percentage = updateProgressPercentage(progress);
  const percentageLabel =
    percentage === null
      ? null
      : formatNumber(percentage / 100, { maximumFractionDigits: 0, style: "percent" });
  const status = updateStatus(
    stage,
    desktopMode,
    readiness,
    metadata,
    percentageLabel,
    t
  );
  const canCheck = Boolean(readiness?.configured) && (stage === "idle" || stage === "current");

  return (
    <div className="dialog-backdrop update-backdrop" role="presentation">
      <section
        aria-describedby="update-dialog-description"
        aria-labelledby="update-dialog-title"
        aria-modal="true"
        className="update-dialog"
        data-dialog-root
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
      >
        <header>
          <div className="dialog-icon" aria-hidden="true">
            <ShieldCheck size={24} />
          </div>
          <div>
            <span className="eyebrow">
              {stage === "store" ? t("update.eyebrow.store") : t("update.eyebrow")}
            </span>
            <h2 id="update-dialog-title">{t("update.title")}</h2>
            <p id="update-dialog-description">
              {stage === "store" ? t("update.description.store") : t("update.description")}
            </p>
          </div>
          <button
            aria-label={t("update.close.aria")}
            className="icon-button"
            data-dialog-initial-focus
            disabled={closeDisabled}
            onClick={onClose}
            title={closeDisabled ? t("update.close.wait") : t("common.close")}
            type="button"
          >
            <X size={18} aria-hidden="true" />
          </button>
        </header>

        <div className="update-summary">
          <div>
            <span>{t("update.summary.installed")}</span>
            <strong>{readiness?.currentVersion ?? t("update.summary.notAvailable")}</strong>
          </div>
          <div>
            <span>{t("update.summary.channel")}</span>
            <strong>
              {readiness?.managedByStore
                ? t("update.channel.appStore")
                : updateChannelLabel(readiness?.channel ?? null, t)}
            </strong>
          </div>
          <div>
            <span>{t("update.summary.available")}</span>
            <strong>
              {metadata?.version ??
                (stage === "store"
                  ? t("update.summary.storeManaged")
                  : stage === "current"
                  ? t("update.summary.upToDate")
                  : t("update.summary.notChecked"))}
            </strong>
          </div>
        </div>

        <div className={`update-status is-${stage}`} aria-live="polite" role="status">
          {stage === "checking" ||
          stage === "installing" ||
          stage === "restarting" ||
          stage === "loading" ? (
            <Loader2 className="spin" size={20} aria-hidden="true" />
          ) : stage === "available" ||
            stage === "current" ||
            stage === "ready" ||
            stage === "store" ? (
            <CheckCircle2 size={20} aria-hidden="true" />
          ) : (
            <AlertCircle size={20} aria-hidden="true" />
          )}
          <div>
            <strong>{status.heading}</strong>
            <span>{status.detail}</span>
          </div>
        </div>

        {stage === "installing" ? (
          <div className="update-progress">
            <div>
              <span>{t("update.progress.label")}</span>
              <strong>{percentageLabel ?? t("update.progress.inProgress")}</strong>
            </div>
            <progress
              aria-label={t("update.progress.aria")}
              max={percentage === null ? undefined : 100}
              value={percentage === null ? undefined : percentage}
            />
          </div>
        ) : null}

        {error ? (
          <div className="operation-audit-message is-error" role="alert">
            <AlertCircle size={17} aria-hidden="true" />
            <span>{error}</span>
          </div>
        ) : null}

        <div className="update-assurance">
          <ShieldCheck size={18} aria-hidden="true" />
          <span>
            {stage === "store" ? t("update.assurance.store") : t("update.assurance")}
          </span>
        </div>

        <footer>
          <button disabled={closeDisabled} onClick={onClose} type="button">
            {t("common.close")}
          </button>
          {canCheck ? (
            <button className="primary" onClick={() => void checkForUpdate()} type="button">
              <RefreshCw size={16} aria-hidden="true" />
              {stage === "current"
                ? t("update.action.checkAgain")
                : t("update.action.check")}
            </button>
          ) : null}
          {stage === "available" ? (
            <button className="primary" onClick={() => void installUpdate()} type="button">
              <Download size={16} aria-hidden="true" />
              {t("update.action.install")}
            </button>
          ) : null}
          {stage === "ready" ? (
            <button className="primary" onClick={() => void restartAfterUpdate()} type="button">
              <RefreshCw size={16} aria-hidden="true" />
              {t("update.action.restart")}
            </button>
          ) : null}
        </footer>
      </section>
    </div>
  );
}

function updateStatus(
  stage: UpdateStage,
  desktopMode: boolean,
  readiness: UpdateReadiness | null,
  metadata: UpdateMetadata | null,
  percentage: string | null,
  t: Translate
) {
  switch (stage) {
    case "loading":
      return {
        heading: t("update.status.loading.heading"),
        detail: t("update.status.loading.detail")
      };
    case "checking":
      return {
        heading: t("update.status.checking.heading", {
          channel: updateChannelLabel(readiness?.channel ?? null, t)
        }),
        detail: t("update.status.checking.detail")
      };
    case "available":
      return {
        heading: t("update.status.available.heading", {
          version: metadata?.version ?? t("update.version.unknown")
        }),
        detail: t("update.status.available.detail")
      };
    case "installing":
      return {
        heading:
          percentage === null
            ? t("update.status.installing.heading")
            : t("update.status.installing.headingProgress", { percent: percentage }),
        detail: t("update.status.installing.detail")
      };
    case "ready":
      return {
        heading: t("update.status.ready.heading"),
        detail: t("update.status.ready.detail")
      };
    case "restarting":
      return {
        heading: t("update.status.restarting.heading"),
        detail: t("update.status.restarting.detail")
      };
    case "current":
      return {
        heading: t("update.status.current.heading"),
        detail: t("update.status.current.detail", {
          channel: updateChannelLabel(readiness?.channel ?? null, t)
        })
      };
    case "store":
      return {
        heading: t("update.status.store.heading"),
        detail: t("update.status.store.detail")
      };
    case "unavailable":
      return desktopMode
        ? {
            heading: t("update.status.unavailableBuild.heading"),
            detail: t("update.status.unavailableBuild.detail")
          }
        : {
            heading: t("update.status.unavailableBrowser.heading"),
            detail: t("update.status.unavailableBrowser.detail")
          };
    default:
      return {
        heading: t("update.status.idle.heading"),
        detail: t("update.status.idle.detail")
      };
  }
}
