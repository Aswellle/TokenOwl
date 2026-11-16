import { useState } from "react";
import { copy } from "../../../styles/copy";
import { IconRefresh } from "../../../components/icons";
import type {
  DashboardBridge,
  ManifestPlaygroundInput,
  ManifestPlaygroundViewModel,
} from "../../../types";

const DEFAULT_MANIFEST = `schemaVersion: 1
id: acme-relay
version: 1
name: Acme Relay
auth: [api_key]
baseUrl: https://relay.example/v1
endpoints:
  models: /models
  usage: /usage
mapping:
  usage:
    value: data.used
    unit: token
    role: used
capability:
  level: L3
  accountTypes: [api]
  quotaUnits: [token]
  targetWindows: [provider-defined]
`;

const DEFAULT_BODY = `{
  "data": {
    "used": 42.5
  }
}
`;

type Endpoint = ManifestPlaygroundInput["endpoint"];

const ENDPOINTS: Endpoint[] = ["usage", "quota", "balance", "reset", "models"];

/**
 * Offline authoring aid for community manifests: validate a pasted pack and
 * run its field mapping against a pasted sample response. No request leaves
 * the process, and only normalized metrics come back.
 */
export function ManifestPlayground({ bridge }: { bridge: DashboardBridge }) {
  const [manifest, setManifest] = useState(DEFAULT_MANIFEST);
  const [endpoint, setEndpoint] = useState<Endpoint>("usage");
  const [body, setBody] = useState(DEFAULT_BODY);
  const [result, setResult] = useState<ManifestPlaygroundViewModel | null>(null);
  const [running, setRunning] = useState(false);
  const [failed, setFailed] = useState(false);

  const run = async () => {
    setRunning(true);
    setFailed(false);
    try {
      const view = await bridge.manifestPlayground({
        manifestYaml: manifest,
        endpoint,
        sampleBody: body,
      });
      setResult(view);
    } catch {
      setResult(null);
      setFailed(true);
    } finally {
      setRunning(false);
    }
  };

  const invalidate = () => {
    setResult(null);
    setFailed(false);
  };

  return (
    <details className="section manifest-playground">
      <summary className="matrix__summary">
        <span className="section__title">{copy.playground.title}</span>
        <span className="matrix__summary-count">{copy.playground.summary}</span>
      </summary>
      <div className="manifest-playground__body">
        <p className="section__intro">{copy.playground.intro}</p>
        <div className="manifest-playground__grid">
          <div className="field">
            <label className="field__label" htmlFor="playground-manifest">
              {copy.playground.manifestLabel}
            </label>
            <textarea
              className="field__input mono manifest-playground__editor"
              id="playground-manifest"
              rows={12}
              value={manifest}
              spellCheck={false}
              onChange={(event) => {
                setManifest(event.currentTarget.value);
                invalidate();
              }}
            />
          </div>
          <div className="field">
            <label className="field__label" htmlFor="playground-body">
              {copy.playground.bodyLabel}
            </label>
            <textarea
              className="field__input mono manifest-playground__editor"
              id="playground-body"
              rows={12}
              value={body}
              spellCheck={false}
              onChange={(event) => {
                setBody(event.currentTarget.value);
                invalidate();
              }}
            />
          </div>
        </div>
        <div className="manifest-playground__actions">
          <div className="field field--inline">
            <label className="field__label" htmlFor="playground-endpoint">
              {copy.playground.endpointLabel}
            </label>
            <select
              className="field__input"
              id="playground-endpoint"
              value={endpoint}
              onChange={(event) => {
                setEndpoint(event.currentTarget.value as Endpoint);
                invalidate();
              }}
            >
              {ENDPOINTS.map((value) => (
                <option key={value} value={value}>
                  {copy.playground.endpointNames[value] ?? value}
                </option>
              ))}
            </select>
          </div>
          <button
            className="btn btn--quiet"
            type="button"
            onClick={() => void run()}
            disabled={running || manifest.trim().length === 0 || body.trim().length === 0}
          >
            <IconRefresh size={14} className={running ? "is-spinning" : undefined} />
            {running ? copy.playground.running : copy.playground.run}
          </button>
        </div>
        {failed && (
          <p className="form__error" role="alert">
            {copy.playground.failed}
          </p>
        )}
        {result && (
          <div className="manifest-playground__result" aria-live="polite">
            <p className={result.valid ? "history__note" : "form__error"} role="status">
              {result.valid ? copy.playground.valid : copy.playground.invalid}
              {result.id ? ` · ${result.id}` : ""}
              {result.schemaVersion ? ` · schema v${result.schemaVersion}` : ""}
              {result.capabilityLevel ? ` · ${result.capabilityLevel}` : ""}
            </p>
            {result.declaredEndpoints.length > 0 && (
              <p className="manifest-playground__line">
                {copy.playground.declared}：{result.declaredEndpoints.join("、")}
              </p>
            )}
            {result.errors.length > 0 && (
              <div className="manifest-playground__errors">
                <strong>{copy.playground.errors}</strong>
                <ul>
                  {result.errors.map((error, index) => (
                    <li key={`${index}-${error}`}>{error}</li>
                  ))}
                </ul>
              </div>
            )}
            {result.warnings.length > 0 && (
              <div className="manifest-playground__warnings">
                <strong>{copy.playground.warnings}</strong>
                <ul>
                  {result.warnings.map((warning, index) => (
                    <li key={`${index}-${warning}`}>{warning}</li>
                  ))}
                </ul>
              </div>
            )}
            {result.metrics.length > 0 && (
              <div className="manifest-playground__metrics">
                <strong>{copy.playground.metrics}</strong>
                <ul>
                  {result.metrics.map((metric, index) => (
                    <li key={`${index}-${metric.metricId ?? "metric"}`} className="num">
                      {metric.metricName ?? metric.metricId ?? "metric"}
                      {" · "}
                      {metric.unit ?? "unknown"}
                      {metric.used !== null && metric.used !== undefined
                        ? ` · used ${metric.used}`
                        : ""}
                      {metric.limit !== null && metric.limit !== undefined
                        ? ` · limit ${metric.limit}`
                        : ""}
                      {metric.remaining !== null && metric.remaining !== undefined
                        ? ` · remaining ${metric.remaining}`
                        : ""}
                      {` · ${metric.status}`}
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {result.reset && (
              <p className="manifest-playground__line num">
                {copy.playground.resetObserved}：{result.reset.resetAt}
                {result.reset.windowId ? ` · ${result.reset.windowId}` : ""}
              </p>
            )}
            {result.valid &&
              result.errors.length === 0 &&
              result.warnings.length === 0 &&
              result.metrics.length === 0 &&
              !result.reset && <p className="history__note">{copy.playground.none}</p>}
          </div>
        )}
        <p className="history__note">{copy.playground.note}</p>
      </div>
    </details>
  );
}
