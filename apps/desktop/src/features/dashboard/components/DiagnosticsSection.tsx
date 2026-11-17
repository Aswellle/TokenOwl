import { useState } from "react";
import { copy } from "../../../styles/copy";
import { IconDownload, IconRefresh } from "../../../components/icons";
import type {
  DashboardBridge,
  DiagnosticsExportResult,
  DiagnosticsPreview,
} from "../../../types";

/**
 * Consumer-facing diagnostics: the entry point is "something is wrong, send
 * the developer a file". The backend refuses to export a document the user
 * has not seen, so the flow stays preview → export even though the wording
 * hides the token mechanics.
 */
export function DiagnosticsSection({ bridge }: { bridge: DashboardBridge }) {
  const [preview, setPreview] = useState<DiagnosticsPreview | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [previewFailed, setPreviewFailed] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [exportResult, setExportResult] = useState<DiagnosticsExportResult | null>(null);
  const [exportFailed, setExportFailed] = useState(false);

  const generate = async () => {
    setPreviewing(true);
    setPreviewFailed(false);
    setExportResult(null);
    setExportFailed(false);
    try {
      setPreview(await bridge.diagnosticsPreview());
    } catch {
      setPreview(null);
      setPreviewFailed(true);
    } finally {
      setPreviewing(false);
    }
  };

  const exportBundle = async () => {
    if (!preview) return;
    setExporting(true);
    setExportFailed(false);
    setExportResult(null);
    try {
      const result = await bridge.exportDiagnostics(preview.token);
      setExportResult(result);
    } catch {
      // State changed between preview and export; the safe response is to ask
      // for a fresh preview rather than silently writing either copy.
      setPreview(null);
      setExportFailed(true);
    } finally {
      setExporting(false);
    }
  };

  return (
    <details className="section settings-card diagnostics-section">
      <summary className="matrix__summary">
        <h2 className="section__title" id="diagnostics-title">
          {copy.diagnostics.title}
        </h2>
        <span className="matrix__summary-count">{copy.diagnostics.summary}</span>
      </summary>
      <div className="diagnostics__body" aria-labelledby="diagnostics-title">
        <p className="section__intro">{copy.diagnostics.intro}</p>
        <div className="diagnostics__actions">
          <button
            className="btn btn--outline"
            type="button"
            onClick={() => void generate()}
            disabled={previewing}
          >
            <IconRefresh size={14} className={previewing ? "is-spinning" : undefined} />
            {previewing ? copy.diagnostics.previewing : copy.diagnostics.preview}
          </button>
          {preview && (
            <button
              className="btn btn--outline"
              type="button"
              onClick={() => void exportBundle()}
              disabled={exporting}
            >
              <IconDownload size={14} />
              {exporting ? copy.diagnostics.exporting : copy.diagnostics.export}
            </button>
          )}
        </div>
        {previewFailed && (
          <p className="form__error" role="alert">
            {copy.diagnostics.previewFailed}
          </p>
        )}
        {exportFailed && (
          <p className="form__error" role="alert">
            {copy.diagnostics.stale}
          </p>
        )}
        {exportResult && (
          <p className="history__note" role="status">
            {copy.diagnostics.exported(exportResult.path, exportResult.bytes)}
          </p>
        )}
        {preview && (
          // The full document stays collapsed: a consumer wants the file, not
          // a JSON wall, but the reviewed content must stay reachable.
          <details className="diagnostics__content">
            <summary>{copy.diagnostics.viewContent}</summary>
            <pre className="diagnostics__json" tabIndex={0} aria-label={copy.diagnostics.summary}>
              {preview.json}
            </pre>
          </details>
        )}
        <p className="history__note">{copy.diagnostics.note}</p>
      </div>
    </details>
  );
}
