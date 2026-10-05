import { useState } from "react";
import "./App.css";

type ViewState = "empty" | "loading" | "error" | "unsupported";

const stateCopy: Record<Exclude<ViewState, "empty">, { title: string; detail: string }> = {
  loading: { title: "Syncing provider data", detail: "The first snapshot will appear here when available." },
  error: { title: "Provider sync unavailable", detail: "No usable snapshot is available yet. Check the connection and try again." },
  unsupported: { title: "Provider not supported yet", detail: "This adapter cannot report quota data in the current version." },
};

function StatusBubble({ expanded, onToggle, state }: { expanded: boolean; onToggle: () => void; state: ViewState }) {
  const statusLabel = state === "empty" ? "No providers connected" : stateCopy[state].title;
  return (
    <button className={`status-bubble status-bubble--${state}`} type="button" aria-expanded={expanded} aria-label={`${statusLabel}. ${expanded ? "Collapse dashboard" : "Open dashboard"}`} onClick={onToggle}>
      <span className="bubble-ring" aria-hidden="true" /><span className="bubble-mark" aria-hidden="true">TO</span><span className="bubble-dot" aria-hidden="true" />
    </button>
  );
}

function App() {
  const [expanded, setExpanded] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [state, setState] = useState<ViewState>("empty");
  const [providerName, setProviderName] = useState("");

  const handleSubmit = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!providerName.trim()) return;
    setShowForm(false); setProviderName(""); setState("loading");
  };

  return (
    <main className="app-shell">
      <StatusBubble expanded={expanded} onToggle={() => setExpanded((value) => !value)} state={state} />
      {expanded && <section className="dashboard" aria-label="TokenOwl dashboard">
        <header className="topbar"><div className="brand-lockup"><span className="brand-symbol" aria-hidden="true">TO</span><div><p className="eyebrow">TokenOwl</p><h1>Quota overview</h1></div></div><div className="topbar-actions"><span className="sync-state"><span className="state-dot" />No providers</span><button className="icon-button" type="button" aria-label="Open settings" title="Settings">•••</button></div></header>
        <div className="summary-grid" aria-label="Account summary"><div className="summary-cell"><span>Accounts</span><strong>—</strong><small>Not connected</small></div><div className="summary-cell"><span>Healthy</span><strong>—</strong><small>Waiting for data</small></div><div className="summary-cell"><span>Next reset</span><strong>—</strong><small>Unknown</small></div></div>
        <div className="content-heading"><div><p className="eyebrow">Workspace</p><h2>Providers</h2></div><button className="text-button" type="button" onClick={() => setShowForm((value) => !value)}>{showForm ? "Cancel" : "Add provider"}</button></div>
        {showForm && <form className="provider-form" onSubmit={handleSubmit}><label htmlFor="provider-name">Provider name</label><div className="form-row"><input id="provider-name" value={providerName} onChange={(event) => setProviderName(event.currentTarget.value)} placeholder="e.g. Personal workspace" autoFocus /><button className="primary-button" type="submit">Connect</button></div><p className="form-note">Credentials stay on this device. Provider support is checked during setup.</p></form>}
        <div className={`state-panel state-panel--${state}`} role={state === "error" ? "alert" : undefined}><div className="state-icon" aria-hidden="true">{state === "empty" ? "+" : state === "loading" ? "~" : state === "error" ? "!" : "?"}</div><div className="state-copy"><h3>{state === "empty" ? "Connect your first provider" : stateCopy[state].title}</h3><p>{state === "empty" ? "Your dashboard will show remaining quota and reset windows once an account is connected." : stateCopy[state].detail}</p>{state === "empty" && <button className="primary-button" type="button" onClick={() => setShowForm(true)}>Add a provider</button>}{state !== "empty" && <button className="text-button" type="button" onClick={() => setState(state === "error" ? "empty" : "loading")}>{state === "unsupported" ? "Choose another provider" : "Try again"}</button>}</div></div>
        <footer className="dashboard-footer"><span>Data appears after a successful sync</span><button className="quiet-button" type="button" onClick={() => setState("loading")}>Refresh</button></footer>
      </section>}
    </main>
  );
}

export default App;
