# PROJECT_STATUS

Última atualização: 2026-10-18 (versão 0.1.0, ainda não publicada).

## Estado

| Parte | Estado |
|---|---|
| Protocolo (docs/PROTOCOL.md) | ✅ implementado nos dois lados; frames testados contra os bytes do Rust (teste JVM do Kotlin) |
| Windows: núcleo Rust (sessões, buffer, WASAPI, cabo, microfone padrão) | ✅ testado ponta a ponta nesta máquina com um celular falso (ver abaixo) |
| Windows: Wi‑Fi (descoberta + conexão de saída) | ✅ testado (celular falso na mesma máquina, via IP da rede) |
| Windows: USB (adb) | ⚠️ compila, parser testado; **sem aparelho para testar** |
| Windows: Bluetooth (RFCOMM + SDP) | ⚠️ compila; o Bluetooth deste PC está desligado → detectado como "off" corretamente; servidor RFCOMM **não exercitado** |
| Windows: instalação do VB-CABLE | ⚠️ implementada (download oficial, Authenticode, `-i -h`); **não executada** (instalaria um driver nesta máquina) |
| Windows: interface | ✅ revisada no navegador com o backend simulado e **no app real** (build de release, WebView2 via CDP): aguardando → aprovação → ao vivo com nível da voz → mutado (o pedido de mute chegou ao celular) |
| Android: Kotlin (Engine, Session, captura, serviço, notificação, bloco rápido) | ✅ compila; `Session` testada em JVM contra um PC roteirizado; ⚠️ **não executado num aparelho** |
| Android: interface | ✅ typecheck + 7 testes de renderização (boas-vindas, procurando, lista, aprovação, conectado, mutado/reconectando, configurações) |
| Releases/atualizações | ⚠️ workflows prontos; repositório `zyypj/micnexus` e segredos **ainda não criados** |

## Medições desta sessão (Windows, celular falso na mesma máquina)

`cargo run --example headless` + `cargo run --example fake_phone -- 10 7`, gravando o lado
microfone do cabo (Steam Streaming Microphone):

- descoberta → TCP → INFO → HELLO → PENDING → aprovado → WELCOME em ~120 ms;
- tom de 440 Hz chega como **440 Hz, RMS 0,346** (= 0,5/√2, sem perda), 0 underruns, buffer 30–48 ms;
- mute: o áudio no cabo vai a zero na hora; desconexão por BYE limpa a sessão;
- **interoperabilidade Kotlin ↔ Rust:** a `Session.kt` do app Android (teste JVM
  `InteropTest`, com a descoberta respondida como o `Engine` faz) conectada ao núcleo real do
  Windows: aprovação, WELCOME `pcm16/48000/10`, STATS, e 440 Hz / RMS 0,346 no cabo;
- o Steam Streaming Microphone vinha com 48 kHz (saída) / 44,1 kHz (microfone): a voz saía
  8% mais grave (404 Hz). Corrigido com o alinhamento automático de taxas. O formato original
  (44,1 kHz) do "Microfone (Steam Streaming Microphone)" desta máquina foi restaurado após os testes.

## Artefatos desta sessão

`D:\nexus-build\micnexus-dist`: `MicNexus_0.1.0_x64-setup.exe` (5,8 MB) e
`micnexus-android-0.1.0.apk` (29 MB, arm64 + armv7, **assinado com a chave de debug** — o APK
assinado com a `release.keystore` sai pelo workflow de release).

## Chaves (backup obrigatório!)

`D:\nexus-build\micnexus-keys`: `updater.key` (+ senha) do atualizador do Windows (a pública já
está no `tauri.conf.json`) e `release.keystore` (alias `micnexus`) do Android. Como configurar
os segredos: docs/UPDATES.md.

## Ambiente desta máquina

- Rust: `CARGO_TARGET_DIR=D:/nexus-build/target-micnexus`.
- Android: `JAVA_HOME=D:\nexus-build\android\jdk17`, `ANDROID_HOME=D:\nexus-build\android\sdk`,
  `GRADLE_USER_HOME=D:/nexus-build/gradle`; `apps/android/android/app/build` é junção para
  `D:\nexus-build\micnexus-android-out\app-build`.
- npm: `npm_config_cache=D:/nexus-build/npm-cache`.
- Preview da interface do Windows no navegador: `npx vite apps/desktop` (porta 1430) com
  `?mock=...`.

## Próximas tarefas

1. Rodar o APK num celular real: Wi‑Fi (descoberta, aprovação, conexão automática), cabo (abrir o
   app pelo adb, troca Wi‑Fi → cabo), Bluetooth (com o Bluetooth do PC ligado), notificação e
   bloco rápido com a tela desligada; medir o atraso real.
2. Testar o instalador NSIS e a instalação do VB-CABLE num Windows limpo.
3. Criar o repositório `zyypj/micnexus`, configurar os segredos e publicar a v0.1.0.
