import type { DesktopSession, DesktopSettings, DesktopState, Transport, UsbDevice } from "@micnexus/protocol";
import type { IconName } from "@micnexus/ui";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState, type ReactNode } from "react";
import { api, isMock } from "../lib/api";
import { Modal } from "./Controls";
import { Icon } from "./Icon";
import { MuteButton } from "./MuteButton";

export const RELEASES_URL = "https://github.com/zyypj/micnexus/releases/latest";
/** Apple's USB drivers for Windows (Microsoft Store), needed for iPhones on the cable. */
const APPLE_DEVICES_URL = "https://apps.microsoft.com/detail/9np83lwlpz9k";

const TRANSPORT_ICON: Record<Transport, IconName> = { wifi: "wifi", usb: "usb", bluetooth: "bluetooth" };
const TRANSPORT_NAME: Record<Transport, string> = { wifi: "Wi‑Fi", usb: "Cabo USB", bluetooth: "Bluetooth" };

export function open(url: string) {
  if (isMock) window.open(url, "_blank");
  else void openUrl(url);
}

function useNow(intervalMs: number) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(t);
  }, [intervalMs]);
  return now;
}

function duration(ms: number) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const pad = (n: number) => String(n).padStart(2, "0");
  return h ? `${h}:${pad(m)}:${pad(s % 60)}` : `${pad(m)}:${pad(s % 60)}`;
}

/** Rough mouth-to-app delay: capture + half the round trip + playout buffer + device. */
function latency(s: DesktopSession) {
  return Math.round(10 + (s.rttMs ?? 0) / 2 + s.bufferMs + 15);
}

// ------------------------------------------------------------ connected --

export function Connected({ state, settings }: { state: DesktopState; settings: DesktopSettings }) {
  const s = state.session!;
  const now = useNow(1000);
  return (
    <section className="card connected">
      <div className="device-row">
        <div className="device-avatar">
          <Icon name="phone" size={22} />
        </div>
        <div className="device-text">
          <div className="device-name">{s.deviceName}</div>
          <div className="device-meta">
            <span className={`chip chip-${s.transport}`}>
              <Icon name={TRANSPORT_ICON[s.transport]} size={13} />
              {TRANSPORT_NAME[s.transport]}
            </span>
            <span>{duration(now - s.connectedAt)}</span>
            <span title="Atraso estimado entre a sua voz e os apps do PC">~{latency(s)} ms</span>
          </div>
        </div>
        <button className="btn ghost danger" onClick={() => api.disconnect()} title="Desconectar o celular">
          <Icon name="power" size={16} />
          Desconectar
        </button>
      </div>
      <MuteButton muted={s.muted} hotkey={settings.muteHotkey} />
    </section>
  );
}

// -------------------------------------------------------------- waiting --

function Row({ icon, title, status, tone, children }: { icon: IconName; title: string; status: ReactNode; tone: "ok" | "warn" | "off"; children?: ReactNode }) {
  return (
    <div className={`transport tone-${tone}`}>
      <div className="transport-icon">
        <Icon name={icon} size={18} />
      </div>
      <div className="transport-text">
        <div className="transport-title">
          {title}
          <span className="dot" aria-hidden />
        </div>
        <div className="transport-status">{status}</div>
      </div>
      {children && <div className="transport-action">{children}</div>}
    </div>
  );
}

function usbStatus(devices: UsbDevice[], enabled: boolean, onHelp: () => void): { status: ReactNode; tone: "ok" | "warn" | "off" } {
  if (!enabled) return { status: "Desativado nas configurações", tone: "off" };
  const d = devices[0];
  const help = (
    <button className="link" onClick={onHelp}>
      Como ativar?
    </button>
  );
  if (!d) return { status: <>Ligue o cabo com a depuração USB ativa · {help}</>, tone: "ok" };
  switch (d.state) {
    case "unauthorized":
      return { status: <>Toque em <b>Permitir</b> no aviso de depuração USB do {d.model}</>, tone: "warn" };
    case "no-app":
      return { status: <>Instale o MicNexus no {d.model}</>, tone: "warn" };
    case "closed":
      return { status: <>Abra o MicNexus no iPhone para conectar pelo cabo</>, tone: "warn" };
    case "offline":
      return { status: <>{d.model} não respondeu; reconecte o cabo</>, tone: "warn" };
    default:
      return { status: <>{d.model} pelo cabo — conectando…</>, tone: "ok" };
  }
}

export function Waiting({ state, settings }: { state: DesktopState; settings: DesktopSettings }) {
  const [usbHelp, setUsbHelp] = useState(false);
  const [btBusy, setBtBusy] = useState(false);
  const phones = state.wifi.phones;
  const usb = usbStatus(state.usb.devices, settings.usbEnabled, () => setUsbHelp(true));
  const bt = state.bluetooth;
  return (
    <>
      <section className="card hero">
        <div className="radar" aria-hidden>
          <span />
          <span />
          <span />
          <div className="radar-core">
            <Icon name="phone" size={26} />
          </div>
        </div>
        <h1>Pronto para conectar</h1>
        <p>
          Abra o <b>MicNexus</b> no celular e toque em <b className="pc-name">{state.pc.name}</b>.
        </p>
        {phones.length > 0 && (
          <div className="found">
            {phones.map((p) => (
              <span key={p.id} className="found-chip">
                <Icon name="phone" size={13} />
                {p.name} está com o app aberto
              </span>
            ))}
          </div>
        )}
        <button className="link small" onClick={() => open(RELEASES_URL)}>
          Ainda não tem o app no celular? Baixe para Android ou iPhone
        </button>
      </section>

      <section className="card transports">
        <div className="card-title">Formas de conectar</div>
        <Row
          icon="wifi"
          title="Wi‑Fi"
          tone={!settings.wifiEnabled ? "off" : state.wifi.ok ? "ok" : "warn"}
          status={
            !settings.wifiEnabled
              ? "Desativado nas configurações"
              : state.wifi.ok
                ? "Celular e PC na mesma rede"
                : "Este PC não está em nenhuma rede"
          }
        />
        <Row
          icon="usb"
          title="Cabo USB"
          tone={state.usb.state === "no-adb" ? "warn" : usb.tone}
          status={
            <>
              {state.usb.state === "no-adb" ? "Componente adb não encontrado (reinstale o MicNexus)" : usb.status}
              {settings.usbEnabled && !state.usb.apple && (
                <span className="sub-hint">
                  iPhone pelo cabo: instale o app{" "}
                  <button className="link" onClick={() => open(APPLE_DEVICES_URL)}>
                    Dispositivos Apple
                  </button>
                </span>
              )}
            </>
          }
        />
        <Row
          icon="bluetooth"
          title="Bluetooth"
          tone={!settings.bluetoothEnabled || bt.state === "unsupported" ? "off" : bt.state === "ready" ? "ok" : "warn"}
          status={
            !settings.bluetoothEnabled
              ? "Desativado nas configurações"
              : bt.state === "ready"
                ? "Android: pareie uma vez · iPhone: toque em Bluetooth no app"
                : bt.state === "off"
                  ? "O Bluetooth do PC está desligado"
                  : bt.state === "unsupported"
                    ? "Este PC não tem Bluetooth"
                    : `Erro: ${bt.error ?? "desconhecido"}`
          }
        >
          {settings.bluetoothEnabled && bt.state === "off" && (
            <button
              className="btn small"
              disabled={btBusy}
              onClick={async () => {
                setBtBusy(true);
                try {
                  await api.bluetoothTurnOn();
                } catch {
                  await api.openBluetoothSettings();
                } finally {
                  setBtBusy(false);
                }
              }}
            >
              {btBusy ? "Ligando…" : "Ligar"}
            </button>
          )}
          {settings.bluetoothEnabled && bt.state === "ready" && (
            <button className="btn small ghost" onClick={() => api.openBluetoothSettings()}>
              Parear
            </button>
          )}
        </Row>
      </section>
      {usbHelp && <UsbHelp onClose={() => setUsbHelp(false)} />}
    </>
  );
}

function UsbHelp({ onClose }: { onClose: () => void }) {
  return (
    <Modal onClose={onClose}>
      <h2>Conectar pelo cabo</h2>
      <p className="muted">O cabo tem o menor atraso e não depende do Wi‑Fi. É preciso ativar uma vez:</p>
      <ol className="steps">
        <li>
          No celular, abra <b>Configurações → Sobre o telefone</b> e toque <b>7 vezes</b> em <b>Número da versão</b> (ou
          "Número de compilação").
        </li>
        <li>
          Volte e abra <b>Opções do desenvolvedor</b> (em Sistema, em alguns aparelhos) e ative <b>Depuração USB</b>.
        </li>
        <li>Ligue o cabo no PC e toque em <b>Permitir</b> (marque "Sempre permitir deste computador").</li>
      </ol>
      <p className="muted small">Pronto: daqui em diante é só plugar o cabo — o MicNexus abre no celular e conecta sozinho.</p>
      <div className="modal-actions">
        <button className="btn primary" onClick={onClose}>
          Entendi
        </button>
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------- virtual mic --

export function MicTarget({ state }: { state: DesktopState }) {
  const out = state.output;
  const install = state.cableInstall;
  if (!out) {
    const busy = install === "downloading" || install === "installing";
    return (
      <section className="card target warn">
        <div className="target-icon">
          <Icon name="alert" size={20} />
        </div>
        <div className="target-text">
          <div className="target-title">Falta o microfone virtual</div>
          <div className="target-sub">
            {install === "downloading"
              ? "Baixando o VB-CABLE…"
              : install === "installing"
                ? "Confirme a instalação do driver nas janelas do Windows…"
                : install === "failed"
                  ? "A instalação não terminou. Tente de novo ou instale pelo site."
                  : "Para o celular aparecer como microfone nos apps, o MicNexus usa o VB-CABLE (gratuito)."}
          </div>
        </div>
        <div className="target-actions">
          <button className="btn primary small" disabled={busy} onClick={() => api.installCable()}>
            {busy ? <span className="spinner" /> : <Icon name="download" size={15} />}
            {busy ? "Instalando" : "Instalar"}
          </button>
          {install === "failed" && (
            <button className="link small" onClick={() => open("https://vb-audio.com/Cable/")}>
              Site do VB-CABLE
            </button>
          )}
        </div>
      </section>
    );
  }
  if (!out.captureName) {
    return (
      <section className="card target warn">
        <div className="target-icon">
          <Icon name="volume" size={20} />
        </div>
        <div className="target-text">
          <div className="target-title">Tocando em {out.name}</div>
          <div className="target-sub">Isso é uma saída de som, não um microfone. Escolha um cabo virtual nas configurações.</div>
        </div>
      </section>
    );
  }
  return (
    <section className="card target">
      <div className="target-icon ok">
        <Icon name="mic" size={20} />
      </div>
      <div className="target-text">
        <div className="target-sub">Nos seus apps (Discord, OBS, jogos), use o microfone</div>
        <div className="target-title" title={out.captureName}>
          {out.captureName}
        </div>
        {state.isDefaultMic && (
          <div className="badge">
            <Icon name="check" size={12} /> Microfone padrão do Windows agora
          </div>
        )}
      </div>
      <button className="icon-btn" onClick={() => api.openSoundSettings()} title="Abrir as configurações de som do Windows" aria-label="Configurações de som">
        <Icon name="external" size={16} />
      </button>
    </section>
  );
}

// -------------------------------------------------------------- pending --

export function Pending({ state }: { state: DesktopState }) {
  const p = state.pending!;
  return (
    <Modal>
      <div className="pending-icon">
        <Icon name="phone" size={28} />
      </div>
      <h2 className="center">Novo celular</h2>
      <p className="center">
        <b>{p.deviceName}</b>
        {p.model && p.model !== p.deviceName ? ` (${p.model})` : ""} quer usar este PC como microfone pelo Wi‑Fi.
      </p>
      <p className="center muted small">{p.address} · você só precisa permitir uma vez</p>
      <div className="modal-actions">
        <button className="btn ghost" onClick={() => api.decide(false)}>
          Recusar
        </button>
        <button className="btn primary" onClick={() => api.decide(true)} autoFocus>
          <Icon name="check" size={16} />
          Permitir
        </button>
      </div>
    </Modal>
  );
}
