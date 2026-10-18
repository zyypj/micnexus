import { useSyncExternalStore } from 'react';
import { APK_ASSET, GITHUB_REPO } from '../buildInfo';
import { MicNexus, UPDATE_ERROR_EVENT, events } from '../native/MicNexus';

/**
 * Self-update from GitHub Releases (no Play Store). Optional: a banner and a
 * button in Settings; Android always asks the user to confirm the install and
 * only accepts an APK signed with the same key.
 */
export interface UpdateState {
  current: string | null;
  available: { version: string; url: string } | null;
  status: 'idle' | 'downloading' | 'needs-permission' | 'error';
  error: string | null;
}

let state: UpdateState = { current: null, available: null, status: 'idle', error: null };
const subs = new Set<() => void>();
function set(patch: Partial<UpdateState>) {
  state = { ...state, ...patch };
  subs.forEach((cb) => cb());
}

export function useUpdate(): UpdateState {
  return useSyncExternalStore(
    (cb) => {
      subs.add(cb);
      return () => subs.delete(cb);
    },
    () => state,
  );
}

/** "0.10.0" > "0.9.3" */
export function isNewer(tag: string, current: string): boolean {
  const parse = (v: string) => v.replace(/^v/, '').split('.').map((n) => parseInt(n, 10) || 0);
  const a = parse(tag);
  const b = parse(current);
  for (let i = 0; i < 3; i++) {
    if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) > (b[i] ?? 0);
  }
  return false;
}

interface GithubRelease {
  tag_name: string;
  draft: boolean;
  prerelease: boolean;
  assets: { name: string; browser_download_url: string }[];
}

export async function checkForUpdate(): Promise<void> {
  const abort = new AbortController();
  const t = setTimeout(() => abort.abort(), 15_000);
  try {
    const current = await MicNexus.getAppVersion();
    set({ current });
    const res = await fetch(`https://api.github.com/repos/${GITHUB_REPO}/releases/latest`, {
      headers: { Accept: 'application/vnd.github+json' },
      signal: abort.signal,
    });
    if (!res.ok) return;
    const rel = (await res.json()) as GithubRelease;
    const asset = rel.assets.find((a) => a.name === APK_ASSET);
    if (rel.draft || rel.prerelease || !asset || !isNewer(rel.tag_name, current)) return;
    set({ available: { version: rel.tag_name.replace(/^v/, ''), url: asset.browser_download_url } });
  } catch {
    // offline: next check
  } finally {
    clearTimeout(t);
  }
}

export async function installUpdate(): Promise<void> {
  const a = state.available;
  if (!a) return;
  try {
    const r = await MicNexus.installUpdate(a.url, a.version);
    set({ status: r === 'permission' ? 'needs-permission' : 'downloading', error: null });
  } catch (e) {
    set({ status: 'error', error: (e as Error).message });
  }
}

events?.addListener(UPDATE_ERROR_EVENT, (e) => {
  set({ status: 'error', error: (e as { message?: string }).message ?? 'Falha na atualização' });
});
