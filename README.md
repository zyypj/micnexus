# MicNexus

Transforma o celular **Android** ou **iPhone** em microfone do PC **Windows**, por **Wi‑Fi**,
**cabo USB** ou **Bluetooth** — conectando sozinho. Botão de mutar nos dois lados, app em segundo
plano no celular com notificação permanente (só enquanto conectado) com **Mutar** e
**Desconectar** — no iPhone, uma Live Activity na tela de bloqueio e na Dynamic Island.
Mesmo padrão e identidade visual do [Nexus](https://github.com/zyypj/nexus).

## Como funciona em uma frase

O app do Windows recebe o áudio do celular e toca num **cabo de áudio virtual**; o outro lado do
cabo aparece para o Discord, OBS, jogos etc. como um **microfone** (e o MicNexus pode torná-lo o
microfone padrão do Windows enquanto o celular estiver conectado).

```
celular (Kotlin)  ── Wi‑Fi / USB (adb) / Bluetooth RFCOMM ──▶  PC (Rust)  ──▶  cabo virtual  ──▶  apps
 AudioRecord → PCM/Opus                                   buffer adaptativo     "CABLE Output"
```

## Conectar

| | O que fazer | Atraso |
|---|---|---|
| **Wi‑Fi** | Abrir o app no celular e tocar no PC (1ª vez: "Permitir" no PC). Depois conecta sozinho ao abrir o app. | baixo |
| **Cabo USB** | Ativar a depuração USB uma vez. Depois é só plugar: o PC abre o app no celular e conecta. | o menor |
| **Bluetooth** | Parear celular e PC uma vez; tocar no PC na lista. | maior (Opus + buffer de 60 ms) |

Nenhuma configuração de firewall: no Wi‑Fi e no cabo é o PC que conecta no celular
(detalhes em [docs/PROTOCOL.md](docs/PROTOCOL.md)).

**iPhone:** instale o `micnexus-ios.ipa` da release com o **Sideloadly** e o seu Apple ID
gratuito (vale 7 dias; renove pelo Sideloadly). Wi‑Fi por Bonjour, cabo pelos drivers da Apple
(app Dispositivos Apple), Bluetooth por BLE — passo a passo em [docs/IOS.md](docs/IOS.md).

## Estrutura

```
apps/desktop        Windows — Tauri 2 + Rust + React + TypeScript + Vite
apps/mobile         Android e iPhone — React Native + TypeScript + Kotlin (android/) e Swift (ios/)
packages/protocol   constantes e formatos de estado compartilhados
packages/ui         tokens visuais e ícones (mesma identidade do Nexus)
scripts/            versão/release, ícones, adb empacotado
docs/               protocolo, Windows, Android, atualizações
```

## Desenvolvimento (Windows)

```bash
npm install
npm run desktop:dev                   # app do Windows (Tauri)
cd apps/mobile && npm install && npm run android   # app Android (aparelho com depuração USB)
```

Sem o Tauri, `npx vite apps/desktop` abre a interface do Windows no navegador com um backend
simulado (`?mock=idle|connected|muted|pending|nocable|usb-auth`).

## Testes

```bash
cd apps/desktop/src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
npm run typecheck
cd apps/mobile && npm run typecheck && npx jest
cd apps/mobile/android && ./gradlew :app:testDebugUnitTest   # protocolo e sessão em Kotlin
# ponta a ponta sem celular: PC sem interface + celular falso na mesma máquina,
# gravando o lado "microfone" do cabo virtual
cargo run --example headless -- 15     # terminal 1
cargo run --example fake_phone -- 10 7 # terminal 2 (tom de 440 Hz, muta aos 7 s)
# ou a Session Kotlin real no lugar do celular falso:
MICNEXUS_INTEROP=1 ./gradlew :app:testDebugUnitTest --tests '*InteropTest*'   # em apps/mobile/android
```

## Releases e atualizações automáticas

`node scripts/release.mjs 0.2.0 && git push origin HEAD v0.2.0` publica uma GitHub Release com o
instalador do Windows, o APK e o `.ipa` do iPhone; Windows e Android se atualizam a partir dela
([docs/UPDATES.md](docs/UPDATES.md)).

## Documentação

[Protocolo](docs/PROTOCOL.md) · [Windows](docs/WINDOWS.md) · [Android](docs/ANDROID.md) · [iPhone](docs/IOS.md) ·
[Atualizações](docs/UPDATES.md) · [Status do projeto](PROJECT_STATUS.md)
