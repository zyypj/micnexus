# App Android (React Native + Kotlin)

Android 7.0+ (minSdk 24, targetSdk 36). React Native 0.87 (New Architecture, Hermes) para a
interface; **tudo que importa roda em Kotlin**, sem depender da interface: descoberta, conexões,
captura, notificação e o bloco de Configurações rápidas.

## Estrutura

```
apps/mobile/
  src/
    App.tsx                  permissões, cabeçalho, navegação simples
    screens/Welcome.tsx      primeira abertura (microfone, notificações, dispositivos próximos)
    screens/Home.tsx         PCs encontrados (Wi‑Fi agora, Bluetooth pareados), conectando, aprovação
    screens/Connected.tsx    botão de mutar com o nível da voz, PC, transporte, atraso, desconectar
    screens/Settings.tsx     conexão automática, redução de ruído, volume (50–400%), atualização
    native/MicNexus.ts       ponte para o módulo Kotlin
  android/app/src/main/java/app/micnexus/android/
    Engine.kt                máquina de estados: descoberta, servidor TCP, Bluetooth, sessão,
                             reconexão (60 s), mute
    Session.kt               uma conexão (HELLO/WELCOME, ping, mute, BYE)
    MicCapture.kt            AudioRecord → ganho → PCM 16-bit ou Opus (MediaCodec)
    MicService.kt            serviço em primeiro plano (microfone) + notificação + wake/Wi‑Fi lock
    MicTileService.kt        bloco "MicNexus" nas Configurações rápidas (mutar sem abrir o app)
    MicNexusModule.kt        ponte React Native
    ApkInstaller.kt          atualização pelo GitHub Releases
```

## Em segundo plano

- O serviço em primeiro plano do tipo `microphone` existe **só enquanto há um PC conectado** (ou
  reconectando); a notificação some ao desconectar. Ela mostra o PC, o transporte e o tempo
  conectado, com **Mutar/Desmutar** e **Desconectar**.
- O Android 14+ só deixa ligar o microfone em segundo plano se o serviço começou com o app na
  frente; por isso a conexão automática pelo Wi‑Fi só é anunciada com o app aberto, e o PC abre o
  app antes de conectar pelo cabo.
- Wake lock parcial e Wi‑Fi lock de baixa latência enquanto conectado (tela desligada sem picos de
  atraso).
- Mutado, o `AudioRecord` é liberado: o indicador verde de microfone do Android apaga.

## Áudio

- Com **Redução de ruído** (padrão): fonte `VOICE_COMMUNICATION` + `NoiseSuppressor`,
  `AcousticEchoCanceler` e `AutomaticGainControl` quando o aparelho tem. Sem: fonte `MIC` crua.
- Wi‑Fi/USB: PCM 48 kHz mono, frames de 10 ms (sem atraso de codificação). Bluetooth: Opus
  64 kbit/s pelo `MediaCodec` (Android 10+) ou PCM 24 kHz.

## Build

Pré-requisitos: Node 22+, JDK 17, Android SDK (platform 36/37, build-tools 37, NDK 27.1).

```bash
cd apps/mobile
npm install
npm run typecheck && npx jest
cd android
./gradlew assembleRelease    # assinatura: MICNEXUS_KEYSTORE, MICNEXUS_KEYSTORE_PASSWORD, MICNEXUS_KEY_ALIAS, MICNEXUS_KEY_PASSWORD
```

Sem as variáveis de keystore o release é assinado com a chave de debug. Nesta máquina:
`JAVA_HOME=D:\nexus-build\android\jdk17`, `ANDROID_HOME=D:\nexus-build\android\sdk`,
`GRADLE_USER_HOME=D:\nexus-build\gradle`; `android/app/build` é uma junção para
`D:\nexus-build\micnexus-android-out\app-build` (o C: está quase cheio).
