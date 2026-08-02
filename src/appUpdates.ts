import type { Translate } from "./i18n";

export type UpdateDownloadEvent =
  | { event: "Started"; data: { contentLength: number | null } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };

export type UpdateProgress = {
  downloaded: number;
  total: number | null;
};

export function applyUpdateDownloadEvent(
  progress: UpdateProgress,
  event: UpdateDownloadEvent
): UpdateProgress {
  if (event.event === "Started") {
    return {
      downloaded: 0,
      total: validByteCount(event.data.contentLength) ? event.data.contentLength : null
    };
  }
  if (event.event === "Progress") {
    const chunkLength = validByteCount(event.data.chunkLength) ? event.data.chunkLength : 0;
    return {
      downloaded: Math.min(Number.MAX_SAFE_INTEGER, progress.downloaded + chunkLength),
      total: progress.total
    };
  }
  return {
    downloaded: progress.total ?? progress.downloaded,
    total: progress.total
  };
}

export function updateProgressPercentage({ downloaded, total }: UpdateProgress) {
  if (!total) {
    return null;
  }
  return Math.min(100, Math.round((downloaded / total) * 100));
}

export function updateChannelLabel(channel: string | null, t: Translate) {
  switch (channel) {
    case "alpha":
      return t("update.channel.alpha");
    case "beta":
      return t("update.channel.beta");
    case "stable":
      return t("update.channel.stable");
    default:
      return t("update.channel.unknown");
  }
}

function validByteCount(value: number | null) {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}
