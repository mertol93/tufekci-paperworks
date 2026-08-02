/// <reference types="vite/client" />

declare module "paperworks-e2e-bridge" {
  export function requestSystemPrint(): void;
  export function takeE2eOpenSelection(): string[] | null;
  export function takeE2eSaveSelection(): string | null;
}
