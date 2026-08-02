import {
  type ChangeEvent,
  type DragEvent,
  useCallback,
  useEffect,
  useMemo,
  useState
} from "react";
import {
  Copy,
  Eye,
  EyeOff,
  ImagePlus,
  Loader2,
  LockKeyhole,
  PenLine,
  Plus,
  Redo2,
  ShieldCheck,
  Sparkles,
  Trash2,
  Type,
  Undo2
} from "lucide-react";
import { CertificateStudio } from "./CertificateStudio";
import { useI18n } from "./I18nProvider";
import { SignatureDrawPad } from "./SignatureDrawPad";
import { SignatureVault } from "./SignatureVault";
import {
  createTypedSignature,
  processSignatureImage,
  SignatureArtworkError,
  type ProcessedSignature,
  type SignatureArtworkErrorCode,
  type SignatureInkColour,
  type TypedSignatureStyle
} from "./signature";
import type { Translate, TranslationKey } from "./i18n";
import {
  createVisualSignatureAsset,
  createVisualSignatureId,
  MAX_VISUAL_SIGNATURE_ASSETS,
  VISUAL_SIGNATURE_DRAG_TYPE,
  type VisualMarkKind,
  type VisualMarkMethod,
  type VisualSignatureAsset,
  type VisualSignaturePlacement
} from "./visualSignatures";

type SignatureStudioProps = {
  assets: VisualSignatureAsset[];
  canRedoPlacement: boolean;
  canUndoPlacement: boolean;
  certificateSigningAvailable: boolean;
  desktopMode: boolean;
  documentLockOpenPassword: string;
  documentLockOpenPasswordConfirmation: string;
  documentLockOwnerPassword: string;
  documentLockOwnerPasswordConfirmation: string;
  documentLockPasswordsValid: boolean;
  documentLocked: boolean;
  hasPlacements: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  onAssetAdd: (asset: VisualSignatureAsset) => void;
  onAssetRemove: (assetId: string) => void;
  onAssetSelect: (assetId: string) => void;
  onDocumentLockedChange: (locked: boolean) => void;
  onDocumentLockOpenPasswordChange: (password: string) => void;
  onDocumentLockOpenPasswordConfirmationChange: (password: string) => void;
  onDocumentLockOwnerPasswordChange: (password: string) => void;
  onDocumentLockOwnerPasswordConfirmationChange: (password: string) => void;
  onPlaceSelected: () => void;
  onPlacementDelete: (placementId: string) => void;
  onPlacementDuplicate: (placementId: string) => void;
  onPlacementLockChange: (placementId: string, locked: boolean) => void;
  onPlacementResize: (placementId: string, widthRatio: number) => void;
  onPlacementRotate: (placementId: string, rotationDegrees: number) => void;
  onRedoPlacement: () => void;
  onUndoPlacement: () => void;
  pyhankoAvailable: boolean;
  qpdfAvailable: boolean;
  selectedAssetId: string | null;
  selectedPlacement: VisualSignaturePlacement | null;
  workspaceHasPendingChanges: boolean;
};

export function SignatureStudio({
  assets,
  canRedoPlacement,
  canUndoPlacement,
  certificateSigningAvailable,
  desktopMode,
  documentLockOpenPassword,
  documentLockOpenPasswordConfirmation,
  documentLockOwnerPassword,
  documentLockOwnerPasswordConfirmation,
  documentLockPasswordsValid,
  documentLocked,
  hasPlacements,
  initialSourcePassword,
  initialSourcePath,
  onAssetAdd,
  onAssetRemove,
  onAssetSelect,
  onDocumentLockedChange,
  onDocumentLockOpenPasswordChange,
  onDocumentLockOpenPasswordConfirmationChange,
  onDocumentLockOwnerPasswordChange,
  onDocumentLockOwnerPasswordConfirmationChange,
  onPlaceSelected,
  onPlacementDelete,
  onPlacementDuplicate,
  onPlacementLockChange,
  onPlacementResize,
  onPlacementRotate,
  onRedoPlacement,
  onUndoPlacement,
  pyhankoAvailable,
  qpdfAvailable,
  selectedAssetId,
  selectedPlacement,
  workspaceHasPendingChanges
}: SignatureStudioProps) {
  const { t } = useI18n();
  const [kind, setKind] = useState<VisualMarkKind>("signature");
  const [method, setMethod] = useState<VisualMarkMethod>("image");
  const [name, setName] = useState(() => t("signature.name.defaultSignature"));
  const [sourceFile, setSourceFile] = useState<File | null>(null);
  const [typedValue, setTypedValue] = useState("");
  const [typedStyle, setTypedStyle] = useState<TypedSignatureStyle>("script");
  const [tolerance, setTolerance] = useState(42);
  const [inkColour, setInkColour] = useState<SignatureInkColour>("original");
  const [draft, setDraft] = useState<ProcessedSignature | null>(null);
  const [processing, setProcessing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showPasswords, setShowPasswords] = useState(false);
  const [dragActive, setDragActive] = useState(false);
  const selectedAsset = assets.find((asset) => asset.id === selectedAssetId) ?? null;

  useEffect(() => {
    if (method !== "image" || !sourceFile) return;
    let cancelled = false;
    setProcessing(true);
    setError(null);
    processSignatureImage(sourceFile, {
      feather: 30,
      inkColour,
      padding: 10,
      tolerance
    })
      .then((result) => {
        if (!cancelled) setDraft(result);
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setDraft(null);
          setError(localiseArtworkError(reason, t, "signature.error.prepareImage"));
        }
      })
      .finally(() => {
        if (!cancelled) setProcessing(false);
      });
    return () => {
      cancelled = true;
    };
  }, [inkColour, method, sourceFile, tolerance]);

  useEffect(() => {
    if (method !== "type") return;
    if (!typedValue.trim()) {
      setDraft(null);
      setError(null);
      return;
    }
    try {
      setDraft(
        createTypedSignature(
          typedValue,
          typedStyle,
          inkColour === "blue" ? "blue" : "black",
          kind === "initials" ? "typed-initials.png" : "typed-signature.png"
        )
      );
      setError(null);
    } catch (reason) {
      setDraft(null);
      setError(localiseArtworkError(reason, t, "signature.error.prepareTyped"));
    }
  }, [inkColour, kind, method, typedStyle, typedValue]);

  const selectSignatureImage = (file: File | null) => {
    if (file && !isSupportedSignatureImage(file)) {
      setError(t("signature.error.chooseImage"));
      return;
    }
    setError(null);
    setSourceFile(file);
    if (!file) setDraft(null);
  };

  const handleImage = (event: ChangeEvent<HTMLInputElement>) => {
    selectSignatureImage(event.target.files?.[0] ?? null);
  };

  const handleImageDrop = (event: DragEvent<HTMLLabelElement>) => {
    event.preventDefault();
    setDragActive(false);
    selectSignatureImage(event.dataTransfer.files?.[0] ?? null);
  };

  const addDraft = () => {
    if (!draft || assets.length >= MAX_VISUAL_SIGNATURE_ASSETS) return;
    try {
      const asset = createVisualSignatureAsset(
        createVisualSignatureId("asset"),
        name,
        kind,
        method,
        draft
      );
      onAssetAdd(asset);
      setName(
        kind === "initials"
          ? t("signature.name.defaultInitials")
          : t("signature.name.defaultSignature")
      );
      setError(null);
    } catch (reason) {
      setError(localiseArtworkError(reason, t, "signature.error.add"));
    }
  };

  const handleDrawPreparationError = useCallback(
    (reason: unknown | null) => {
      setError(
        reason === null
          ? null
          : localiseArtworkError(reason, t, "signature.error.prepareVisual")
      );
    },
    [t]
  );

  const creationInkColours = useMemo<SignatureInkColour[]>(
    () => (method === "image" ? ["original", "black", "blue"] : ["black", "blue"]),
    [method]
  );

  useEffect(() => {
    if (!creationInkColours.includes(inkColour)) setInkColour("black");
  }, [creationInkColours, inkColour]);

  return (
    <section className="signature-studio">
      <div className="signature-heading">
        <div>
          <h3>{t("signature.heading.title")}</h3>
          <p>{t("signature.heading.description")}</p>
        </div>
        <Sparkles size={18} aria-hidden="true" />
      </div>

      <fieldset className="signature-fieldset">
        <legend>{t("signature.kind.legend")}</legend>
        <div className="signature-kind-control">
          {(["signature", "initials"] as VisualMarkKind[]).map((value) => (
            <button
              className={kind === value ? "is-active" : ""}
              key={value}
              onClick={() => {
                setKind(value);
                if (
                  name === t("signature.name.defaultSignature") ||
                  name === t("signature.name.defaultInitials")
                ) {
                  setName(
                    value === "initials"
                      ? t("signature.name.defaultInitials")
                      : t("signature.name.defaultSignature")
                  );
                }
              }}
              type="button"
            >
              {t(value === "signature" ? "signature.kind.signature" : "signature.kind.initials")}
            </button>
          ))}
        </div>
      </fieldset>

      <fieldset className="signature-fieldset">
        <legend>{t("signature.creation.legend")}</legend>
        <div className="signature-method-control">
          {([
            ["draw", PenLine, "signature.method.draw"],
            ["image", ImagePlus, "signature.method.image"],
            ["type", Type, "signature.method.type"]
          ] as const).map(([value, Icon, label]) => (
            <button
              className={method === value ? "is-active" : ""}
              key={value}
              onClick={() => {
                setMethod(value);
                setDraft(null);
                setError(null);
              }}
              type="button"
            >
              <Icon size={15} aria-hidden="true" />
              {t(label)}
            </button>
          ))}
        </div>
      </fieldset>

      {method === "image" ? (
        <>
          <label
            className={`button-like wide-button signature-image-picker ${dragActive ? "is-drop-target" : ""}`}
            onDragEnter={() => setDragActive(true)}
            onDragLeave={() => setDragActive(false)}
            onDragOver={(event) => {
              event.preventDefault();
              event.dataTransfer.dropEffect = "copy";
            }}
            onDrop={handleImageDrop}
          >
            <ImagePlus size={17} aria-hidden="true" />
            {t("signature.image.choose")}
            <input
              accept="image/png,image/jpeg,image/webp,image/bmp,image/tiff"
              aria-label={t("signature.image.aria")}
              className="visually-hidden"
              onChange={handleImage}
              type="file"
            />
          </label>
          <label className="signature-field">
            {t("signature.background.label")}
            <input
              max="110"
              min="8"
              onChange={(event) => setTolerance(Number(event.target.value))}
              type="range"
              value={tolerance}
            />
            <small>{t("signature.background.help")}</small>
          </label>
        </>
      ) : method === "draw" ? (
        <SignatureDrawPad
          colour={inkColour === "blue" ? "blue" : "black"}
          onPreparedChange={setDraft}
          onPreparationError={handleDrawPreparationError}
          sourceName={kind === "initials" ? "drawn-initials.png" : "drawn-signature.png"}
        />
      ) : (
        <div className="signature-typed-fields">
          <label className="signature-field">
            {t(kind === "initials" ? "signature.typed.initials" : "signature.typed.signature")}
            <input
              maxLength={80}
              onChange={(event) => setTypedValue(event.target.value)}
              placeholder={t(
                kind === "initials"
                  ? "signature.typed.initialsPlaceholder"
                  : "signature.typed.signaturePlaceholder"
              )}
              value={typedValue}
            />
          </label>
          <label className="signature-field">
            {t("signature.style.label")}
            <select
              onChange={(event) => setTypedStyle(event.target.value as TypedSignatureStyle)}
              value={typedStyle}
            >
              <option value="script">{t("signature.style.script")}</option>
              <option value="classic">{t("signature.style.classic")}</option>
              <option value="modern">{t("signature.style.modern")}</option>
            </select>
          </label>
        </div>
      )}

      <fieldset className="signature-fieldset">
        <legend>{t("signature.ink.legend")}</legend>
        <div className={`segmented-control signature-ink-${creationInkColours.length}`}>
          {creationInkColours.map((colour) => (
            <button
              className={inkColour === colour ? "is-active" : ""}
              key={colour}
              onClick={() => setInkColour(colour)}
              type="button"
            >
              {t(signatureInkColourKeys[colour])}
            </button>
          ))}
        </div>
      </fieldset>

      <div className="signature-preview" aria-live="polite">
        {processing ? (
          <div className="signature-placeholder">
            <Loader2 className="spin" size={23} aria-hidden="true" />
            <span>{t("signature.preview.processing")}</span>
          </div>
        ) : draft ? (
          <img src={draft.dataUrl} alt={t("signature.preview.alt")} />
        ) : (
          <div className="signature-placeholder">
            <PenLine size={23} aria-hidden="true" />
            <span>{t("signature.preview.empty")}</span>
          </div>
        )}
      </div>

      <label className="signature-field">
        {t("signature.name.label")}
        <input maxLength={80} onChange={(event) => setName(event.target.value)} value={name} />
      </label>
      {error ? <p className="signature-error">{error}</p> : null}
      <button
        className="wide-button"
        disabled={!draft || !name.trim() || assets.length >= MAX_VISUAL_SIGNATURE_ASSETS}
        onClick={addDraft}
        type="button"
      >
        <Plus size={16} aria-hidden="true" />
        {t("signature.action.addSession")}
      </button>

      <section className="signature-session-library" aria-labelledby="signature-session-title">
        <div className="signature-session-heading">
          <div>
            <strong id="signature-session-title">{t("signature.session.title")}</strong>
            <small>
              {t("signature.session.count", {
                current: assets.length,
                maximum: MAX_VISUAL_SIGNATURE_ASSETS
              })}
            </small>
          </div>
          <div className="signature-history-actions">
            <button
              aria-label={t("signature.history.undoAria")}
              disabled={!canUndoPlacement}
              onClick={onUndoPlacement}
              title={t("common.undo")}
              type="button"
            >
              <Undo2 size={15} aria-hidden="true" />
            </button>
            <button
              aria-label={t("signature.history.redoAria")}
              disabled={!canRedoPlacement}
              onClick={onRedoPlacement}
              title={t("common.redo")}
              type="button"
            >
              <Redo2 size={15} aria-hidden="true" />
            </button>
          </div>
        </div>
        {assets.length === 0 ? (
          <p className="signature-session-empty">{t("signature.session.empty")}</p>
        ) : (
          <div
            className="signature-asset-list"
            role="listbox"
            aria-label={t("signature.session.listAria")}
          >
            {assets.map((asset) => (
              <div
                aria-selected={asset.id === selectedAssetId}
                className={`signature-asset ${asset.id === selectedAssetId ? "is-selected" : ""}`}
                draggable
                key={asset.id}
                onDragStart={(event) => {
                  event.dataTransfer.effectAllowed = "copy";
                  event.dataTransfer.setData(VISUAL_SIGNATURE_DRAG_TYPE, asset.id);
                }}
                onClick={() => onAssetSelect(asset.id)}
                role="option"
                tabIndex={0}
              >
                <span className="signature-asset-preview"><img alt="" src={asset.dataUrl} /></span>
                <span className="signature-asset-copy">
                  <strong>{asset.name}</strong>
                  <small>
                    {t("signature.asset.meta", {
                      kind: t(visualMarkKindKeys[asset.kind]),
                      method: t(visualMarkMethodKeys[asset.method])
                    })}
                  </small>
                </span>
                <button
                  aria-label={t("signature.asset.removeAria", { name: asset.name })}
                  onClick={(event) => {
                    event.stopPropagation();
                    onAssetRemove(asset.id);
                  }}
                  title={t("signature.asset.removeTitle")}
                  type="button"
                >
                  <Trash2 size={14} aria-hidden="true" />
                </button>
              </div>
            ))}
          </div>
        )}
        <button
          className="primary wide-button"
          disabled={!selectedAsset}
          onClick={onPlaceSelected}
          type="button"
        >
          {t("signature.action.placePage")}
        </button>
      </section>

      {selectedPlacement ? (
        <section className="signature-placement-controls">
          <div className="signature-session-heading">
            <div>
              <strong>{t("signature.placement.title")}</strong>
              <small>
                {t(
                  selectedPlacement.locked
                    ? "signature.placement.locked"
                    : "signature.placement.editable"
                )}
              </small>
            </div>
          </div>
          <label className="signature-field">
            {t("signature.placement.size")}
            <input
              disabled={selectedPlacement.locked}
              max="68"
              min="4"
              onChange={(event) =>
                onPlacementResize(selectedPlacement.id, Number(event.target.value) / 100)
              }
              type="range"
              value={Math.round(selectedPlacement.widthRatio * 100)}
            />
          </label>
          <label className="signature-field">
            {t("signature.placement.rotation")}
            <input
              disabled={selectedPlacement.locked}
              max="180"
              min="-180"
              onChange={(event) =>
                onPlacementRotate(selectedPlacement.id, Number(event.target.value))
              }
              step="1"
              type="number"
              value={Math.round(selectedPlacement.rotationDegrees)}
            />
          </label>
          <div className="signature-placement-actions">
            <button onClick={() => onPlacementDuplicate(selectedPlacement.id)} type="button">
              <Copy size={15} aria-hidden="true" />
              {t("signature.placement.duplicate")}
            </button>
            <button
              onClick={() => onPlacementLockChange(selectedPlacement.id, !selectedPlacement.locked)}
              type="button"
            >
              <LockKeyhole size={15} aria-hidden="true" />
              {t(
                selectedPlacement.locked
                  ? "signature.placement.unlock"
                  : "signature.placement.lock"
              )}
            </button>
            <button
              className="danger-action"
              disabled={selectedPlacement.locked}
              onClick={() => onPlacementDelete(selectedPlacement.id)}
              type="button"
            >
              <Trash2 size={15} aria-hidden="true" />
              {t("signature.placement.delete")}
            </button>
          </div>
        </section>
      ) : null}

      <SignatureVault
        asset={selectedAsset}
        desktopMode={desktopMode}
        onAssetLoad={onAssetAdd}
      />

      <label className="signature-toggle">
        <input
          checked={documentLocked && qpdfAvailable}
          disabled={!qpdfAvailable || !hasPlacements}
          onChange={(event) => onDocumentLockedChange(event.target.checked)}
          type="checkbox"
        />
        <span>
          <strong>{t("signature.documentLock.title")}</strong>
          <small>
            {qpdfAvailable
              ? t("signature.documentLock.available")
              : t("signature.documentLock.unavailable")}
          </small>
        </span>
        <ShieldCheck size={17} aria-hidden="true" />
      </label>

      {documentLocked ? (
        <fieldset className="signing-lock-fields">
          <legend>{t("signature.password.legend")}</legend>
          <SignaturePasswordField
            autoComplete="new-password"
            label={t("protection.openingPassword")}
            onChange={onDocumentLockOpenPasswordChange}
            showPassword={showPasswords}
            value={documentLockOpenPassword}
          />
          <SignaturePasswordField
            autoComplete="new-password"
            label={t("protection.confirmOpeningPassword")}
            onChange={onDocumentLockOpenPasswordConfirmationChange}
            showPassword={showPasswords}
            value={documentLockOpenPasswordConfirmation}
          />
          <SignaturePasswordField
            autoComplete="new-password"
            label={t("protection.administratorPassword")}
            onChange={onDocumentLockOwnerPasswordChange}
            showPassword={showPasswords}
            value={documentLockOwnerPassword}
          />
          <SignaturePasswordField
            autoComplete="new-password"
            label={t("protection.confirmAdministratorPassword")}
            onChange={onDocumentLockOwnerPasswordConfirmationChange}
            showPassword={showPasswords}
            value={documentLockOwnerPasswordConfirmation}
          />
          <button
            className="show-passwords"
            onClick={() => setShowPasswords((current) => !current)}
            type="button"
          >
            {showPasswords ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
            {showPasswords ? t("common.hidePasswords") : t("common.showPasswords")}
          </button>
          {documentLockOpenPasswordConfirmation &&
          documentLockOwnerPasswordConfirmation &&
          !documentLockPasswordsValid ? (
            <p className="field-error">
              {t("signature.password.validation")}
            </p>
          ) : null}
          <small>
            {t("signature.password.adminHelp")}
          </small>
        </fieldset>
      ) : null}

      <div className="signature-security-note">
        {t("signature.security.visualNote")}
      </div>

      <CertificateStudio
        desktopMode={certificateSigningAvailable}
        initialSourcePassword={initialSourcePassword}
        initialSourcePath={initialSourcePath}
        pyhankoAvailable={pyhankoAvailable}
        workspaceHasPendingChanges={workspaceHasPendingChanges}
      />
    </section>
  );
}

const supportedSignatureMimeTypes = new Set([
  "image/bmp",
  "image/jpeg",
  "image/png",
  "image/tiff",
  "image/webp"
]);

const signatureInkColourKeys = {
  black: "signature.ink.black",
  blue: "signature.ink.blue",
  original: "signature.ink.original"
} as const;

const visualMarkKindKeys = {
  initials: "signature.kind.initials",
  signature: "signature.kind.signature"
} as const;

const visualMarkMethodKeys = {
  draw: "signature.method.draw",
  image: "signature.method.image",
  type: "signature.method.type"
} as const;

const artworkErrorKeys = {
  "crop-image": "signature.error.cropImage",
  "crop-visual": "signature.error.cropVisual",
  "no-image-ink": "signature.error.noImageInk",
  "no-visual-ink": "signature.error.noVisualInk",
  "open-image": "signature.error.openImage",
  "prepare-image": "signature.error.prepareImage",
  "prepare-typed": "signature.error.prepareTyped",
  "prepare-visual": "signature.error.prepareVisual",
  "typed-length": "signature.error.typedLength"
} satisfies Record<SignatureArtworkErrorCode, TranslationKey>;

function localiseArtworkError(
  reason: unknown,
  t: Translate,
  fallback: TranslationKey
) {
  return reason instanceof SignatureArtworkError ? t(artworkErrorKeys[reason.code]) : t(fallback);
}

function isSupportedSignatureImage(file: File) {
  return (
    supportedSignatureMimeTypes.has(file.type.toLocaleLowerCase("en-GB")) ||
    /\.(?:bmp|jpe?g|png|tiff?|webp)$/iu.test(file.name)
  );
}

type SignaturePasswordFieldProps = {
  autoComplete: string;
  label: string;
  onChange: (value: string) => void;
  showPassword: boolean;
  value: string;
};

function SignaturePasswordField({
  autoComplete,
  label,
  onChange,
  showPassword,
  value
}: SignaturePasswordFieldProps) {
  return (
    <label className="protection-field">
      {label}
      <input
        autoComplete={autoComplete}
        maxLength={127}
        onChange={(event) => onChange(event.target.value)}
        spellCheck={false}
        type={showPassword ? "text" : "password"}
        value={value}
      />
    </label>
  );
}
