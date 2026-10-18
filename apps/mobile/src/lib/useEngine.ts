import { useEffect, useState } from 'react';
import { MicNexus, PCS_EVENT, STATE_EVENT, events, type EngineState, type Pc } from '../native/MicNexus';

/** Live engine state and PC list (pushed by Kotlin on every change). */
export function useEngine(): { state: EngineState | null; pcs: Pc[] } {
  const [state, setState] = useState<EngineState | null>(null);
  const [pcs, setPcs] = useState<Pc[]>([]);
  useEffect(() => {
    const subs = [
      events?.addListener(STATE_EVENT, (s) => setState(s as EngineState)),
      events?.addListener(PCS_EVENT, (p) => setPcs(p as Pc[])),
    ];
    MicNexus.getState().then(setState).catch(() => {});
    MicNexus.getPcs().then(setPcs).catch(() => {});
    return () => subs.forEach((s) => s?.remove());
  }, []);
  return { state, pcs };
}
