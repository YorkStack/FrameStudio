import { translateError } from "./i18n";
import { t } from "./i18n";
import {
  useEffect,
  useRef,
  useState,
  useId,
  Children,
  cloneElement,
  isValidElement,
  type ReactNode,
} from "react";
import { X, ImageOff, LoaderCircle } from "lucide-react";
import { rpc } from "./api";
import { de } from "./i18n";
const thumbnails = new Map<string, string>();
export function AssetImage({
  id,
  thumbnail = true,
  alt = "",
  className = "",
}: {
  id: string;
  thumbnail?: boolean;
  alt?: string;
  className?: string;
}) {
  const [src, setSrc] = useState(thumbnail ? thumbnails.get(id) : undefined);
  const [error, setError] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let live = true;
    setError(false);
    setSrc(thumbnail ? thumbnails.get(id) : undefined);
    const load = async () => {
      if (thumbnail && thumbnails.has(id)) { if (live) setSrc(thumbnails.get(id)); return; }
      try {
        const data = await rpc("asset.read", { id, thumbnail });
        const value = `data:${data.mime};base64,${data.data_base64}`;
        if (thumbnail) {
          if (thumbnails.size > 1200)
            thumbnails.delete(thumbnails.keys().next().value!);
          thumbnails.set(id, value);
        }
        if (live) setSrc(value);
      } catch {
        if (live) setError(true);
      }
    };
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          void load();
          observer.disconnect();
        }
      },
      { rootMargin: "180px" },
    );
    if (ref.current) observer.observe(ref.current);
    return () => {
      live = false;
      observer.disconnect();
    };
  }, [id, thumbnail]);
  return (
    <div className={`asset-image ${className}`} ref={ref}>
      {src ? (
        <img src={src} alt={alt} draggable={false} />
      ) : error ? (
        <span className="image-fallback">
          <ImageOff size={24} />
          <span>{t("Datei nicht verfügbar")}</span>
        </span>
      ) : (
        <LoaderCircle className="spin" size={20} aria-label={de.loading} />
      )}
    </div>
  );
}
export function Modal({
  title,
  children,
  onClose,
  wide = false,
  className = "",
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  wide?: boolean;
  className?: string;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  useEffect(() => {
    ref.current?.showModal();
    return () => ref.current?.close();
  }, []);
  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      className={`${wide ? "modal wide" : "modal"} ${className}`}
      onCancel={(event) => { event.preventDefault(); onClose(); }}
      onClick={(e) => {
        if (e.target === ref.current) onClose();
      }}
    >
      <div className="modal-heading">
        <h2 id={titleId}>{title}</h2>
        <button className="icon-button" aria-label={de.close} onClick={onClose}>
          <X size={20} />
        </button>
      </div>
      {children}
    </dialog>
  );
}
export function Field({
  label,
  children,
  hint,
}: {
  label: string;
  children: ReactNode;
  hint?: string;
}) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      {Children.map(children, (child) =>
        isValidElement(child) &&
        ["input", "textarea", "select"].includes(String(child.type))
          ? cloneElement(child as React.ReactElement<any>, { id })
          : child,
      )}
      {hint && <small>{hint}</small>}
    </div>
  );
}
export function Empty({
  icon,
  title,
  children,
}: {
  icon: ReactNode;
  title: string;
  children?: ReactNode;
}) {
  return (
    <div className="empty">
      <div className="empty-icon">{icon}</div>
      <h2>{title}</h2>
      {children}
    </div>
  );
}
export function ErrorBox({ error }: { error: string }) {
  return error ? (
    <div className="error-box" role="alert">
      {translateError(error)}
    </div>
  ) : null;
}
export function FormActions({
  onClose,
  busy,
  disabled,
  label = de.save,
  cancelLabel = de.cancel,
}: {
  onClose: () => void;
  busy?: boolean;
  disabled?: boolean;
  label?: string;
  cancelLabel?: string;
}) {
  return (
    <div className="form-actions">
      <button type="button" className="button ghost" onClick={onClose}>
        {cancelLabel}
      </button>
      <button className="button primary" disabled={busy || disabled}>
        {busy ? de.busy : label}
      </button>
    </div>
  );
}
