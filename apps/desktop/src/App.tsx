import type { DesktopSettings, DesktopState } from "@micnexus/protocol";
import { useCallback, useEffect, useState } from "react";
import { Icon } from "./components/Icon";
import { Connected, MicTarget, Pending, Waiting } from "./components/Home";
import { Settings } from "./components/Settings";
import { api } from "./lib/api";
import { useUpdater } from "./lib/updater";

export function App() {
  const [state, setState] = useState<DesktopState | null>(null);
  const [settings, setSettings] = useState<DesktopSettings | null>(null);
  const [view, setView] = useState<"home" | "settings">("home");
  const updater = useUpdater();

  useEffect(() => {
    const stop = api.onState(setState);
    void api.getState().then(setState);
    void api.getSettings().then(setSettings);
    return stop;
  }, []);

  const changeSettings = useCallback(async (patch: Partial<DesktopSettings>) => {
    const next = Object.keys(patch).length ? await api.setSettings(patch) : await api.getSettings();
    setSettings(next);
  }, []);

  if (!state || !settings) return <div className="boot" />;
  const session = state.session;
  const status = session ? (session.muted ? "muted" : "live") : "idle";

  return (
    <div className={`app status-${status}`}>
      <header className="topbar">
        <img src="/logo.svg" alt="" width={28} height={28} />
        <div className="brand">MicNexus</div>
        <div className={`pill pill-${status}`}>
          <span className="pill-dot" />
          {session ? (session.muted ? "Mutado" : "Ao vivo") : "Aguardando"}
        </div>
        <div className="spacer" />
        {updater.available && view === "home" && (
          <button className="btn small update" onClick={updater.action?.run} disabled={updater.busy}>
            <Icon name="download" size={14} />
            Atualizar
          </button>
        )}
        <button
          className={`icon-btn${view === "settings" ? " active" : ""}`}
          onClick={() => setView(view === "settings" ? "home" : "settings")}
          aria-label="Configurações"
          title="Configurações"
        >
          <Icon name="settings" />
        </button>
      </header>

      {view === "settings" ? (
        <Settings state={state} settings={settings} onChange={changeSettings} onClose={() => setView("home")} updater={updater} />
      ) : (
        <main className="home">
          {session ? <Connected state={state} settings={settings} /> : <Waiting state={state} settings={settings} />}
          <MicTarget state={state} />
        </main>
      )}

      {state.pending && <Pending state={state} />}
    </div>
  );
}
