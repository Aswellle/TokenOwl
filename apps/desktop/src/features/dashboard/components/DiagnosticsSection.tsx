import { useState } from "react";
import { copy } from "../../../styles/copy";
import { IconDownload, IconRefresh } from "../../../components/icons";
import type {
  DashboardBridge,
  DiagnosticsExportResult,
  DiagnosticsPreview,
} from "../../../types";

/**
 * Local support bundle: preview first, then export the exact document that
 * was reviewed. The backend rejects a stale preview token, so a changed
 * bundle cannot be written without another look.
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
      // A changed backend state invalidates the token; the safe response is
      // to ask for a fresh preview rather than silently writing either copy.
      setPreview(null);
      setExportFailed(true);
    } finally {
      setExporting(false);
    }
  };

  return (
    <details className="section diagnostics-section">
      <summary className="matrix__summary">
        <span className="section__title">{copy.diagnostics.title}</span>
        <span className="matrix__summary-count">{copy.diagnostics.summary}</span>
      </summary>
      <div className="diagnostics__body">
        <p className="section__intro">{copy.diagnostics.intro}</p>
        <div className="diagnostics__actions">
          <button
            className="btn btn--quiet"
            type="button"
            onClick={() => void generate()}
            disabled={previewing}
          >
            <IconRefresh size={14} className={previewing ? "is-spinning" : undefined} />
            {previewing ? copy.diagnostics.previewing : copy.diagnostics.preview}
          </button>
          {preview && (
            <button
              className="btn btn--quiet"
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
          <pre className="diagnostics__json" aria-label={copy.diagnostics.summary}>
            {preview.json}
          </pre>
        )}
        <p className="history__note">{copy.diagnostics.note}</p>
      </div>
    </details>
  );
}
