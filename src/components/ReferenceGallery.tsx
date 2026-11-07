import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  deleteAsset, importAsset, isAppliedCleanup, listAssets, mediaErrorKey, readAssetPreview, updateAsset, validateAssetInput,
  type Asset,
} from "../data/assets";
import { ConfirmDialog } from "./ConfirmDialog";

interface ReferenceGalleryProps {
  characterId: string;
  busy?: boolean;
  disabled?: boolean;
  onChanged: (characterId: string) => void;
  onBusyChange: (characterId: string, busy: boolean) => void;
  onCleanupPending: () => void;
  onCheckCleanup?: () => void;
}

// A keyed session isolates list, form and preview state even if navigation reuses the detail view.
export function ReferenceGallery(props: ReferenceGalleryProps) {
  return <GallerySession key={props.characterId} {...props} />;
}

// Keep one preview IPC in flight across rapid switches and gallery unmounts.
// Only the latest waiting selection is retained; already-running IPC cannot be cancelled.
let previewRunning = false;
let waitingPreview: { run: () => Promise<void>; cancel: () => void } | null = null;
function queuePreview(characterId: string, assetId: string, isCurrent: () => boolean): Promise<Blob | null> {
  return new Promise((resolve, reject) => {
    const request = {
      cancel: () => resolve(null),
      run: async () => {
        previewRunning = true;
        try {
          if (!isCurrent()) { resolve(null); return; }
          const blob = await readAssetPreview(characterId, assetId);
          resolve(isCurrent() ? blob : null);
        } catch (reason) { if (isCurrent()) reject(reason); else resolve(null); }
        finally {
          previewRunning = false;
          const next = waitingPreview;
          waitingPreview = null;
          if (next) void next.run();
        }
      },
    };
    if (previewRunning) { waitingPreview?.cancel(); waitingPreview = request; }
    else void request.run();
  });
}

function AssetPreview({ characterId, asset }: { characterId: string; asset: Asset }) {
  const { t } = useTranslation();
  const [attempt, setAttempt] = useState(0);
  const [preview, setPreview] = useState<{ url?: string; failed?: boolean }>({});
  const ownedUrlRef = useRef<string | undefined>(undefined);
  useEffect(() => {
    let active = true;
    let ownedUrl: string | undefined;
    void queuePreview(characterId, asset.id, () => active).then((blob) => {
      if (!active || !blob) return;
      ownedUrl = URL.createObjectURL(blob);
      ownedUrlRef.current = ownedUrl;
      setPreview({ url: ownedUrl });
    }).catch(() => { if (active) setPreview({ failed: true }); });
    return () => {
      active = false;
      if (ownedUrl && ownedUrlRef.current === ownedUrl) {
        URL.revokeObjectURL(ownedUrl);
        ownedUrlRef.current = undefined;
      }
    };
  }, [characterId, asset.id, attempt]);
  return (
    <div className="reference-preview">
      {preview.url ? <img src={preview.url} alt={asset.originalFilename} onError={() => {
        if (ownedUrlRef.current) { URL.revokeObjectURL(ownedUrlRef.current); ownedUrlRef.current = undefined; }
        setPreview({ failed: true });
      }} /> : preview.failed ? (
        <div><p role="status">{t("gallery.previewError")}</p><button className="secondary-button" type="button" onClick={() => { setPreview({}); setAttempt((value) => value + 1); }}>{t("gallery.retryPreview")}</button></div>
      ) : <p role="status">{t("gallery.previewLoading")}</p>}
    </div>
  );
}

function GallerySession({ characterId, busy: externalBusy = false, disabled = false, onChanged, onBusyChange, onCleanupPending, onCheckCleanup }: ReferenceGalleryProps) {
  const { t } = useTranslation();
  const [assets, setAssets] = useState<Asset[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [operation, setOperation] = useState<"import" | "save" | "delete" | null>(null);
  const [editing, setEditing] = useState(false);
  const [artistName, setArtistName] = useState("");
  const [sourceUrl, setSourceUrl] = useState("");
  const [notes, setNotes] = useState("");
  const [confirming, setConfirming] = useState(false);
  const active = useRef(false);
  const inFlight = useRef(false);
  const listRequest = useRef(0);
  const foreignOperation = useRef(externalBusy);
  const removeButton = useRef<HTMLButtonElement>(null);
  const busy = externalBusy || operation !== null || disabled;
  const selected = assets.find((asset) => asset.id === selectedId) ?? assets[0];

  useEffect(() => {
    active.current = true;
    const request = ++listRequest.current;
    void listAssets(characterId).then((result) => {
      if (!active.current || request !== listRequest.current) return;
      setAssets(result);
      setLoaded(true);
    }).catch((reason: unknown) => {
      if (!active.current || request !== listRequest.current) return;
      setError(mediaErrorKey(reason));
    });
    return () => { active.current = false; listRequest.current += 1; };
  }, [characterId]);

  const reload = useCallback(async () => {
    const request = ++listRequest.current;
    setError(null);
    try {
      const result = await listAssets(characterId);
      if (active.current && request === listRequest.current) { setAssets(result); setLoaded(true); }
    } catch (reason) {
      if (active.current && request === listRequest.current) setError(mediaErrorKey(reason));
    }
  }, [characterId]);

  useEffect(() => {
    if (externalBusy) {
      if (!inFlight.current) foreignOperation.current = true;
      return;
    }
    if (!foreignOperation.current) return;
    foreignOperation.current = false;
    let current = true;
    void Promise.resolve().then(() => { if (current) void reload(); });
    return () => { current = false; };
  }, [externalBusy, reload]);

  function begin(next: "import" | "save" | "delete") {
    if (busy || inFlight.current) return false;
    inFlight.current = true;
    listRequest.current += 1;
    setOperation(next); setError(null);
    onBusyChange(characterId, true);
    return true;
  }
  function end() {
    inFlight.current = false;
    if (active.current) setOperation(null);
    // Do not release this lock at unmount: native selection may still be in flight.
    onBusyChange(characterId, false);
  }
  async function importReference() {
    if (!loaded || !begin("import")) return;
    try {
      const imported = await importAsset(characterId);
      if (imported) {
        if (active.current) { setAssets((current) => [...current, imported]); setSelectedId(imported.id); }
        onChanged(characterId);
      }
    } catch (reason) {
      if (active.current) setError(mediaErrorKey(reason));
      onCheckCleanup?.();
      if (isAppliedCleanup(reason)) onCleanupPending();
    } finally { end(); }
  }
  async function saveMetadata(event: React.FormEvent) {
    event.preventDefault();
    if (!selected || busy || inFlight.current) return;
    let input;
    try { input = validateAssetInput({ artistName, sourceUrl, notes }); }
    catch (reason) { setError(mediaErrorKey(reason)); return; }
    if (!begin("save")) return;
    try {
      const updated = await updateAsset(characterId, selected.id, input);
      if (active.current) { setAssets((current) => current.map((asset) => asset.id === updated.id ? updated : asset)); setEditing(false); }
      onChanged(characterId);
    } catch (reason) { if (active.current) setError(mediaErrorKey(reason)); }
    finally { end(); }
  }
  async function removeReference() {
    if (!selected || !begin("delete")) return;
    let applied = false;
    try { await deleteAsset(characterId, selected.id); applied = true; }
    catch (reason) {
      if (isAppliedCleanup(reason)) { applied = true; onCleanupPending(); }
      else if (active.current) setError(mediaErrorKey(reason));
    } finally {
      if (applied) {
        if (active.current) { setAssets((current) => current.filter((asset) => asset.id !== selected.id)); setConfirming(false); setSelectedId(null); }
        onChanged(characterId);
      }
      end();
    }
  }

  return (
    <section className="reference-gallery" aria-label={t("detail.references")} aria-busy={busy || undefined}>
      <div className="reference-heading"><h2>{t("detail.references")}</h2><button className="secondary-button" type="button" onClick={() => void importReference()} disabled={busy || !loaded || editing}>{operation === "import" ? t("gallery.importing") : t("gallery.import")}</button></div>
      <p className="reference-hint">{t("gallery.supported")}</p>
      {error && !confirming && <div className="reference-error"><p className="error-banner" role="alert">{t(error)}</p>{!loaded && <button className="secondary-button" type="button" onClick={() => void reload()}>{t("actions.retry")}</button>}</div>}
      {!loaded && !error && <p role="status">{t("gallery.loading")}</p>}
      {loaded && assets.length === 0 && <div className="reference-empty"><p>{t("gallery.empty")}</p><p>{t("gallery.emptyHint")}</p></div>}
      {selected && <>
        <ul className="reference-list" aria-label={t("gallery.images")}>
          {assets.map((asset, index) => <li key={asset.id}><button type="button" aria-label={t("gallery.view", { name: asset.originalFilename })} aria-pressed={asset.id === selected.id} disabled={busy || editing} onClick={() => { setSelectedId(asset.id); setError(null); }}><span className="reference-number" aria-hidden="true">{String(index + 1).padStart(2, "0")}</span><span>{asset.originalFilename}</span></button></li>)}
        </ul>
        <AssetPreview key={selected.id} characterId={characterId} asset={selected} />
        <div className="reference-caption"><strong>{selected.originalFilename}</strong><span>{selected.width} × {selected.height}</span></div>
        {editing ? <form className="reference-form" onSubmit={(event) => void saveMetadata(event)}>
          <label>{t("gallery.artist")}<input value={artistName} maxLength={200} disabled={busy} onChange={(event) => setArtistName(event.target.value)} /></label>
          <label>{t("gallery.source")}<input type="text" inputMode="url" value={sourceUrl} maxLength={2048} disabled={busy} onChange={(event) => setSourceUrl(event.target.value)} /></label>
          <p className="reference-hint">{t("gallery.sourceHint")}</p>
          <label>{t("gallery.notes")}<textarea value={notes} maxLength={10000} rows={4} disabled={busy} onChange={(event) => setNotes(event.target.value)} /></label>
          <div className="reference-actions"><button className="secondary-button" type="button" disabled={busy} onClick={() => { setEditing(false); setError(null); }}>{t("actions.cancel")}</button><button className="primary-button" type="submit" disabled={busy}>{operation === "save" ? t("gallery.saving") : t("gallery.save")}</button></div>
        </form> : <>
          <dl className="reference-metadata">
            <div><dt>{t("gallery.artist")}</dt><dd>{selected.artistName || t("gallery.unset")}</dd></div>
            <div><dt>{t("gallery.source")}</dt><dd>{selected.sourceUrl || t("gallery.unset")}</dd></div>
            <div><dt>{t("gallery.notes")}</dt><dd>{selected.notes || t("gallery.unset")}</dd></div>
          </dl>
          <div className="reference-actions"><button className="secondary-button" type="button" disabled={busy} onClick={() => { setArtistName(selected.artistName ?? ""); setSourceUrl(selected.sourceUrl ?? ""); setNotes(selected.notes ?? ""); setEditing(true); setError(null); }}>{t("gallery.edit")}</button><button ref={removeButton} className="danger-link" type="button" disabled={busy} onClick={() => { setError(null); setConfirming(true); }}>{t("gallery.remove")}</button></div>
        </>}
        <ConfirmDialog open={confirming} title={t("gallery.removeTitle", { name: selected.originalFilename })} description={t("gallery.removeDescription")} confirmLabel={t("gallery.remove")} confirming={operation === "delete"} error={error ? t(error) : null} returnFocusRef={removeButton} onCancel={() => { setConfirming(false); setError(null); }} onConfirm={() => void removeReference()} />
      </>}
    </section>
  );
}
