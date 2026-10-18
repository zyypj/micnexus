import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { useCallback, useEffect, useState } from "react";
import { isMock } from "./api";

export interface Updater {
  label: string;
  busy: boolean;
  /** A newer version is ready to install. */
  available: string | null;
  action: { label: string; run: () => void } | null;
}

const SIX_HOURS = 6 * 60 * 60 * 1000;

/** Checks GitHub Releases (tauri-plugin-updater) at startup and every 6 h. */
export function useUpdater(): Updater {
  const [update, setUpdate] = useState<Update | null>(null);
  const [status, setStatus] = useState<"idle" | "checking" | "latest" | "downloading" | "error">("idle");
  const [progress, setProgress] = useState(0);

  const runCheck = useCallback(async () => {
    if (isMock) return setStatus("latest");
    setStatus("checking");
    try {
      const u = await check();
      setUpdate(u);
      setStatus("latest");
    } catch {
      setStatus("error");
    }
  }, []);

  useEffect(() => {
    const first = setTimeout(runCheck, 4000);
    const timer = setInterval(runCheck, SIX_HOURS);
    return () => {
      clearTimeout(first);
      clearInterval(timer);
    };
  }, [runCheck]);

  const install = useCallback(async () => {
    if (!update) return;
    setStatus("downloading");
    let total = 0;
    let got = 0;
    try {
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        if (e.event === "Progress") {
          got += e.data.chunkLength;
          if (total) setProgress(Math.round((got / total) * 100));
        }
      });
      await relaunch();
    } catch {
      setStatus("error");
    }
  }, [update]);

  if (status === "downloading") return { label: `Baixando atualização… ${progress}%`, busy: true, available: update?.version ?? null, action: null };
  if (update)
    return {
      label: `Versão ${update.version} disponível`,
      busy: false,
      available: update.version,
      action: { label: "Atualizar", run: () => void install() },
    };
  if (status === "checking") return { label: "Procurando atualizações…", busy: true, available: null, action: null };
  if (status === "error")
    return { label: "Não foi possível procurar atualizações", busy: false, available: null, action: { label: "Tentar de novo", run: () => void runCheck() } };
  return { label: "Você está na versão mais recente", busy: false, available: null, action: { label: "Procurar", run: () => void runCheck() } };
}
