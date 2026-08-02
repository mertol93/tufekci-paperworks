import { type FormEvent, useEffect, useRef, useState } from "react";
import { KeyRound, LockKeyhole } from "lucide-react";
import { useDialogFocus } from "./accessibility";
import { useI18n } from "./I18nProvider";
import {
  MAX_PDF_OPENING_PASSWORD_BYTES,
  type PdfPasswordRequest,
  validPdfOpeningPasswordInput
} from "./pdfPassword";

type PdfPasswordDialogProps = {
  documentName: string;
  request: PdfPasswordRequest;
};

export function PdfPasswordDialog({ documentName, request }: PdfPasswordDialogProps) {
  const { t } = useI18n();
  const [password, setPassword] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useDialogFocus<HTMLFormElement>({
    active: true,
    onEscape: request.cancel
  });

  useEffect(() => {
    setPassword("");
    inputRef.current?.focus();
  }, [request]);

  const submitPassword = (event: FormEvent) => {
    event.preventDefault();

    if (password) {
      request.submit(password);
    }
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <form
        aria-describedby="pdf-password-help"
        aria-labelledby="pdf-password-title"
        aria-modal="true"
        className="password-dialog"
        data-dialog-root
        onSubmit={submitPassword}
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
      >
        <div className="dialog-icon" aria-hidden="true">
          <LockKeyhole size={24} />
        </div>
        <div>
          <span className="eyebrow">{t("pdfPassword.eyebrow")}</span>
          <h2 aria-live="polite" id="pdf-password-title">
            {request.incorrect
              ? t("pdfPassword.title.incorrect")
              : t("pdfPassword.title.initial")}
          </h2>
          <p>{documentName}</p>
        </div>

        <label>
          {t("pdfPassword.field.label")}
          <div className="password-dialog-field">
            <KeyRound size={17} aria-hidden="true" />
            <input
              aria-invalid={request.incorrect}
              autoCapitalize="none"
              autoComplete="off"
              data-dialog-initial-focus
              maxLength={MAX_PDF_OPENING_PASSWORD_BYTES}
              onChange={(event) => {
                if (validPdfOpeningPasswordInput(event.target.value)) {
                  setPassword(event.target.value);
                }
              }}
              ref={inputRef}
              spellCheck={false}
              type="password"
              value={password}
            />
          </div>
        </label>

        <p id="pdf-password-help">
          {t("pdfPassword.help")}
        </p>

        <div className="dialog-actions">
          <button onClick={request.cancel} type="button">
            {t("pdfPassword.action.cancel")}
          </button>
          <button className="primary" disabled={!password} type="submit">
            {t("pdfPassword.action.open")}
          </button>
        </div>
      </form>
    </div>
  );
}
