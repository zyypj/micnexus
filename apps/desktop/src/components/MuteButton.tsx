import { useEffect, useRef } from "react";
import { api } from "../lib/api";
import { Icon } from "./Icon";

/**
 * The big round mute button. The glow around it follows the phone's voice
 * (level events, ~20/s) through a CSS variable, without re-rendering React.
 */
export function MuteButton({ muted, hotkey }: { muted: boolean; hotkey: string | null }) {
  const ref = useRef<HTMLButtonElement>(null);
  const meter = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let level = 0;
    return api.onLevel(({ peak }) => {
      // Fast attack, slow release, perceptual curve.
      const target = Math.min(1, Math.sqrt(peak) * 1.15);
      level = target > level ? target : level * 0.82 + target * 0.18;
      ref.current?.style.setProperty("--level", level.toFixed(3));
      meter.current?.style.setProperty("--level", level.toFixed(3));
    });
  }, []);
  useEffect(() => {
    if (muted) {
      ref.current?.style.setProperty("--level", "0");
      meter.current?.style.setProperty("--level", "0");
    }
  }, [muted]);
  return (
    <div className="mute-wrap">
      <button
        ref={ref}
        className={`mute-btn${muted ? " muted" : ""}`}
        onClick={() => api.setMuted(!muted)}
        aria-pressed={muted}
        aria-label={muted ? "Desmutar microfone" : "Mutar microfone"}
      >
        <span className="mute-glow" aria-hidden />
        <span className="mute-core">
          <Icon name={muted ? "micOff" : "mic"} size={44} />
        </span>
      </button>
      <div className="mute-label">{muted ? "Microfone mutado" : "Microfone ativo"}</div>
      <div className="mute-hint">
        {muted ? "Clique para voltar a falar" : "Clique para mutar"}
        {hotkey && (
          <>
            {" · "}
            <kbd>{hotkey.replace(/\+/g, " + ")}</kbd>
          </>
        )}
      </div>
      <div ref={meter} className={`meter${muted ? " off" : ""}`} aria-hidden>
        <div className="meter-fill" />
      </div>
    </div>
  );
}
