import { useState } from "react";
import { Eye, EyeOff, ShieldCheck } from "lucide-react";
import {
  outputProtectionIsValid,
  type OutputProtectionDraft
} from "./outputProtection";
import { useI18n } from "./I18nProvider";

type OutputProtectionFieldsProps = {
  disabled: boolean;
  onChange: (value: OutputProtectionDraft) => void;
  qpdfAvailable: boolean;
  value: OutputProtectionDraft;
};

export function OutputProtectionFields({
  disabled,
  onChange,
  qpdfAvailable,
  value
}: OutputProtectionFieldsProps) {
  const { t } = useI18n();
  const [showPasswords, setShowPasswords] = useState(false);
  const confirmationStarted =
    Boolean(value.openPasswordConfirmation) || Boolean(value.ownerPasswordConfirmation);

  const update = (change: Partial<OutputProtectionDraft>) => {
    onChange({ ...value, ...change });
  };

  return (
    <section className="output-protection-fields">
      <label className="output-protection-toggle">
        <input
          checked={value.enabled && qpdfAvailable}
          disabled={disabled || !qpdfAvailable}
          onChange={(event) => update({ enabled: event.target.checked })}
          type="checkbox"
        />
        <span>
          <strong>{t("protection.title")}</strong>
          <small>
            {qpdfAvailable
              ? t("protection.description.available")
              : t("protection.description.unavailable")}
          </small>
        </span>
        <ShieldCheck size={17} aria-hidden="true" />
      </label>

      {value.enabled ? (
        <fieldset disabled={disabled || !qpdfAvailable}>
          <legend>{t("protection.legend")}</legend>
          <PasswordField
            label={t("protection.openingPassword")}
            onChange={(openPassword) => update({ openPassword })}
            showPassword={showPasswords}
            value={value.openPassword}
          />
          <PasswordField
            label={t("protection.confirmOpeningPassword")}
            onChange={(openPasswordConfirmation) => update({ openPasswordConfirmation })}
            showPassword={showPasswords}
            value={value.openPasswordConfirmation}
          />
          <PasswordField
            label={t("protection.administratorPassword")}
            onChange={(ownerPassword) => update({ ownerPassword })}
            showPassword={showPasswords}
            value={value.ownerPassword}
          />
          <PasswordField
            label={t("protection.confirmAdministratorPassword")}
            onChange={(ownerPasswordConfirmation) => update({ ownerPasswordConfirmation })}
            showPassword={showPasswords}
            value={value.ownerPasswordConfirmation}
          />
          <button
            className="show-passwords"
            disabled={disabled || !qpdfAvailable}
            onClick={() => setShowPasswords((current) => !current)}
            type="button"
          >
            {showPasswords ? (
              <EyeOff size={16} aria-hidden="true" />
            ) : (
              <Eye size={16} aria-hidden="true" />
            )}
            {showPasswords ? t("common.hidePasswords") : t("common.showPasswords")}
          </button>
          {confirmationStarted && !outputProtectionIsValid(value, qpdfAvailable) ? (
            <p className="field-error">{t("protection.validation")}</p>
          ) : null}
          <small>{t("protection.permissions")}</small>
        </fieldset>
      ) : null}
    </section>
  );
}

function PasswordField({
  label,
  onChange,
  showPassword,
  value
}: {
  label: string;
  onChange: (value: string) => void;
  showPassword: boolean;
  value: string;
}) {
  return (
    <label className="protection-field">
      {label}
      <input
        autoComplete="new-password"
        maxLength={127}
        onChange={(event) => onChange(event.target.value)}
        spellCheck={false}
        type={showPassword ? "text" : "password"}
        value={value}
      />
    </label>
  );
}
