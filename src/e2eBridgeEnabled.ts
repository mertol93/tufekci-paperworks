import "@wdio/tauri-plugin";

declare global {
  interface Window {
    __paperworksE2eOpenPaths?: string[];
    __paperworksE2ePrintRequests?: number;
    __paperworksE2eSavePath?: string;
  }
}

export function takeE2eOpenSelection() {
  const selection = window.__paperworksE2eOpenPaths;
  delete window.__paperworksE2eOpenPaths;
  return Array.isArray(selection) && selection.every((path) => typeof path === "string" && path)
    ? [...selection]
    : null;
}

export function takeE2eSaveSelection() {
  const selection = window.__paperworksE2eSavePath;
  delete window.__paperworksE2eSavePath;
  return typeof selection === "string" && selection ? selection : null;
}

export function requestSystemPrint() {
  window.__paperworksE2ePrintRequests = (window.__paperworksE2ePrintRequests ?? 0) + 1;
}
