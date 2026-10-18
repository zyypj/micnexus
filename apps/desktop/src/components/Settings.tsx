import type { AudioEndpoint, DesktopSettings, DesktopState } from "@micnexus/protocol";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { Updater } from "../lib/updater";
import { HotkeyInput, Toggle } from "./Controls";
import { Icon } from "./Icon";

const CABLE_LABEL: Record<string, string> = {
  vbcable: "VB-CABLE",
  voicemeeter: "VoiceMeeter",
  steam: "Steam",
  vac: "Virtual Audio Cable",
  other: "cabo virtual",
};

export function Settings({
  state,
  settings,
  onChange,
  onClose,
  updater,
}: {
  state: DesktopState;
  settings: DesktopSettings;
  onChange: (patch: Partial<DesktopSettings>) => Promise<void>;
  onClose: () => void;
  updater: Updater;
}) {
  const [outputs, setOutputs] = useState<AudioEndpoint[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState(settings.pcName);
  useEffect(() => {
    void api.listOutputs().then(setOutputs);
  }, []);
  const set = async (patch: Partial<DesktopSettings>) => {
    setError(null);
    try {
      await onChange(patch);
    } catch (e) {
      setError(patch.muteHotkey !== undefined ? "Esse atalho já está em uso por outro programa. Escolha outro." : String(e));
    }
  };
  const cables = outputs.filter((o) => o.cable);
  const others = outputs.filter((o) => !o.cable);

  return (
    <div className="settings">
      <header className="settings-head">
        <button className="icon-btn" onClick={onClose} aria-label="Voltar">
          <Icon name="arrowLeft" />
        </button>
        <h2>Configurações</h2>
      </header>
      <div className="settings-body">
        {error && <div className="error-box">{error}</div>}

        <section className="group">
          <div className="group-title">Microfone virtual</div>
          <label className="field">
            <span>Enviar a voz do celular para</span>
            <select value={settings.outputId ?? ""} onChange={(e) => set({ outputId: e.target.value || null })}>
              <option value="">Automático (melhor cabo virtual)</option>
              {cables.length > 0 && (
                <optgroup label="Cabos virtuais">
                  {cables.map((o) => (
                    <option key={o.id} value={o.id}>
                      {o.name} — {CABLE_LABEL[o.cable!]}
                    </option>
                  ))}
                </optgroup>
              )}
              {others.length > 0 && (
                <optgroup label="Saídas de som (não viram microfone)">
                  {others.map((o) => (
                    <option key={o.id} value={o.id}>
                      {o.name}
                    </option>
                  ))}
                </optgroup>
              )}
            </select>
          </label>
          <Toggle
            checked={settings.setDefaultMic}
            onChange={(v) => set({ setDefaultMic: v })}
            label="Virar o microfone padrão do Windows"
            hint="Enquanto o celular estiver conectado; ao desconectar, volta o microfone de antes."
          />
          <Toggle
            checked={settings.monitor}
            onChange={(v) => set({ monitor: v })}
            label="Ouvir o celular neste PC"
            hint="Para testar: toca a sua voz também nos alto-falantes/fones."
          />
          {settings.monitor && (
            <label className="field">
              <span>Tocar em</span>
              <select value={settings.monitorId ?? ""} onChange={(e) => set({ monitorId: e.target.value || null })}>
                <option value="">Saída padrão do Windows</option>
                {others.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.name}
                  </option>
                ))}
              </select>
            </label>
          )}
        </section>

        <section className="group">
          <div className="group-title">Mutar</div>
          <div className="field-row">
            <div className="toggle-text">
              <span className="toggle-label">Atalho global</span>
              <span className="toggle-hint">Funciona com o MicNexus minimizado, em qualquer app.</span>
            </div>
            <HotkeyInput value={settings.muteHotkey} onChange={(v) => set({ muteHotkey: v })} />
          </div>
        </section>

        <section className="group">
          <div className="group-title">Conexões</div>
          <Toggle checked={settings.wifiEnabled} onChange={(v) => set({ wifiEnabled: v })} label="Wi‑Fi" hint="Procura celulares na mesma rede." />
          <Toggle
            checked={settings.usbEnabled}
            onChange={(v) => set({ usbEnabled: v })}
            label="Cabo USB"
            hint="Usa a depuração USB do Android (adb)."
          />
          <Toggle
            checked={settings.usbAutoLaunch}
            disabled={!settings.usbEnabled}
            onChange={(v) => set({ usbAutoLaunch: v })}
            label="Abrir o app no celular ao ligar o cabo"
          />
          <Toggle
            checked={settings.bluetoothEnabled}
            onChange={(v) => set({ bluetoothEnabled: v })}
            label="Bluetooth"
            hint="Celulares pareados com este PC."
          />
          <label className="field">
            <span>Nome deste PC no celular</span>
            <input
              value={name}
              placeholder={state.pc.name}
              maxLength={40}
              onChange={(e) => setName(e.target.value)}
              onBlur={() => name !== settings.pcName && set({ pcName: name.trim() })}
              onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
            />
          </label>
        </section>

        <section className="group">
          <div className="group-title">Geral</div>
          <Toggle
            checked={settings.startWithWindows}
            onChange={(v) => set({ startWithWindows: v })}
            label="Iniciar com o Windows"
            hint="Fica na bandeja, pronto para o celular."
          />
          <Toggle
            checked={settings.notifyOnConnect}
            onChange={(v) => set({ notifyOnConnect: v })}
            label="Avisar quando um celular conectar ou cair"
          />
        </section>

        <section className="group">
          <div className="group-title">Celulares permitidos no Wi‑Fi</div>
          {settings.trusted.length === 0 ? (
            <div className="empty">Nenhum ainda. O primeiro acesso pelo Wi‑Fi pede sua permissão.</div>
          ) : (
            settings.trusted.map((d) => (
              <div key={d.id} className="trusted">
                <Icon name="phone" size={16} />
                <span className="trusted-name">{d.name}</span>
                <span className="muted small">desde {new Date(d.addedAt).toLocaleDateString("pt-BR")}</span>
                <button
                  className="icon-btn"
                  title="Esquecer (vai pedir permissão de novo)"
                  aria-label={`Esquecer ${d.name}`}
                  onClick={async () => {
                    await api.forgetDevice(d.id);
                    await onChange({});
                  }}
                >
                  <Icon name="trash" size={15} />
                </button>
              </div>
            ))
          )}
        </section>

        <section className="group about">
          <div className="group-title">Sobre</div>
          <div className="about-row">
            <img src="/logo.svg" alt="" width={36} height={36} />
            <div>
              <div className="about-name">MicNexus {state.pc.version}</div>
              <div className="muted small">{updater.label}</div>
            </div>
            {updater.action && (
              <button className="btn small" onClick={updater.action.run} disabled={updater.busy}>
                {updater.action.label}
              </button>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
