# App iPhone (React Native + Swift)

iOS 17+. A interface é a mesma do Android (`apps/mobile/src`, React Native); o motor é Swift
(`apps/mobile/ios/MicNexus/Native`). Distribuição **sem assinatura paga**: o GitHub gera um
`.ipa` sem assinatura e você instala com o **Sideloadly** usando o seu Apple ID gratuito.

## Instalar (Windows + Sideloadly)

1. Baixe `micnexus-ios.ipa` da [última release](https://github.com/zyypj/micnexus/releases/latest).
2. Instale o **Sideloadly** (https://sideloadly.io) e o app **Dispositivos Apple** da Microsoft
   Store (ou o iTunes) — são os drivers USB do iPhone. Eles também são o que o MicNexus do
   Windows usa para conectar o iPhone **pelo cabo**.
3. Ligue o iPhone no PC pelo cabo e toque em **Confiar** no iPhone.
4. No Sideloadly: arraste o `.ipa`, escolha o iPhone, digite o seu **Apple ID** e clique em
   **Start** (a senha vai para a Apple, não para o MicNexus).
5. No iPhone: **Ajustes → Geral → VPN e Gerenciamento de Dispositivos** → toque no seu Apple ID →
   **Confiar**. No iOS 16+, ative também **Ajustes → Privacidade e Segurança → Modo de
   Desenvolvedor** (o iPhone reinicia).

### Limites do Apple ID gratuito

- O app **expira em 7 dias**. Reinstale pelo Sideloadly, ou ative o **auto-refresh** dele (o
  Sideloadly renova sozinho com o PC ligado e o iPhone na mesma rede Wi‑Fi).
- Sem atualização automática: o app avisa quando há versão nova e abre a página da release; é
  só instalar o novo `.ipa` por cima (as configurações ficam).
- No máximo 3 apps instalados assim ao mesmo tempo (limite da Apple para contas grátis).

## Diferenças em relação ao Android

| | Android | iPhone |
|---|---|---|
| Wi‑Fi | responde à descoberta UDP do PC | anuncia Bonjour `_micnexus._tcp`; o PC pergunta por mDNS *legacy unicast* (sem regra de firewall) |
| Cabo | adb (depuração USB); o PC abre o app | usbmuxd (drivers da Apple); abra o app com o cabo ligado |
| Bluetooth | clássico (RFCOMM), celular pareado no Windows | **BLE**: toque em "Conectar por Bluetooth" (sem parear); 1ª vez pede aprovação no PC |
| Em segundo plano | serviço + notificação com Mutar/Desconectar | sessão de áudio + **Live Activity** (tela de bloqueio e Dynamic Island) com Mutar/Desconectar |
| Mutado | o microfone é liberado | o microfone continua aberto sem enviar nada (é a sessão de áudio que mantém o app vivo em segundo plano; o indicador laranja fica aceso) |

O iPhone não recebe a descoberta do PC: ele monta a lista de PCs pelos `INFO` das conexões de
"presença" que o PC faz a cada ~3 s (docs/PROTOCOL.md).

## Estrutura

```
apps/mobile/ios/
  project.yml                  projeto Xcode (XcodeGen): app + extensão da Live Activity
  Podfile                      React Native 0.87
  MicNexus/AppDelegate.swift   inicia o Engine e o React Native
  MicNexus/Native/
    Engine.swift               estado, Bonjour (NWListener + TXT want), sessões, reconexão 60 s
    Session.swift              INFO → HELLO → WELCOME, ping, mute, BYE
    Capture.swift              AVAudioEngine + AVAudioSinkNode (~5 ms), PCM ou Opus (AVAudioConverter)
    BlePeripheral.swift        periférico GATT: rx (write) / tx (notify), fila sem acumular atraso
    LiveActivityController.swift
    MicNexusModule.swift/.m    ponte React Native (mesma API do módulo Kotlin)
  Shared/                      atributos da Live Activity e os botões (App Intents)
  LiveActivity/                extensão WidgetKit (tela de bloqueio + Dynamic Island)
```

## Build

Só em macOS (o CI usa `macos-15`):

```bash
brew install xcodegen
cd apps/mobile && npm ci
bash scripts/build-ipa.sh       # ios/build/micnexus-ios.ipa (sem assinatura)
```
