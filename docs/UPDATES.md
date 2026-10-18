# Atualizações automáticas

Mesmo esquema do Nexus: uma tag `vX.Y.Z` dispara `.github/workflows/release.yml`, que cria uma
pre-release, anexa os artefatos e só no fim a promove a "latest" (os atualizadores ignoram
pre-releases, então ninguém baixa uma release pela metade).

```bash
node scripts/release.mjs 0.2.0      # versão em todos os manifestos, commit e tag
git push origin HEAD v0.2.0
```

| App | Como atualiza |
|---|---|
| Windows | `tauri-plugin-updater` lê `releases/latest/download/latest.json`, confere a assinatura minisign com a chave pública do `tauri.conf.json` e instala em modo passivo. Checa ao abrir e a cada 6 h; botão "Atualizar" no topo e em Configurações → Sobre. |
| Android | Consulta a API de releases; havendo versão nova, mostra um aviso. O APK (`micnexus-android.apk`) é baixado pelo DownloadManager e entregue ao instalador do sistema, que confirma com o usuário e exige a mesma chave de assinatura. |

## Segredos do GitHub

| Segredo | Valor |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | conteúdo de `updater.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | `updater.key.password` |
| `ANDROID_KEYSTORE_BASE64` | `base64 -w0 release.keystore` |
| `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_PASSWORD` | `release.keystore.password` |
| `ANDROID_KEY_ALIAS` | `micnexus` |

As chaves foram geradas nesta máquina em `D:\nexus-build\micnexus-keys` — **faça backup**: sem
elas os apps já instalados não aceitam atualizações.
