# App Windows (Tauri 2 + Rust)

Windows 10 1803+ / 11. Fica na **bandeja** (fechar a janela não encerra) e inicia com o Windows
(minimizado). Interface em React; todo o áudio e a rede são Rust.

## Estrutura

```
apps/desktop/
  src/                       interface (React): aguardando, conectado, aprovação, configurações
    lib/mock.ts              backend simulado para trabalhar na interface no navegador
  src-tauri/src/
    lib.rs                   janela, bandeja (ícone muda: aguardando / ao vivo / mutado),
                             comandos, atalho global, notificações, início com o Windows
    core.rs                  estado e sessões (independente do Tauri)
    protocol.rs              formato de fio (docs/PROTOCOL.md)
    lan.rs                   Wi‑Fi: descoberta por broadcast e conexão de saída ao celular
    adb.rs                   USB: protocolo do servidor adb (track-devices, forward, am start)
    bluetooth.rs             RFCOMM + registro SDP (Winsock), estado do rádio (WinRT Radios)
    audio/
      jitter.rs              buffer adaptativo (alvo 30/60 ms, nunca acumula atraso)
      render.rs              WASAPI compartilhado, evento, 48 kHz mono float + AUTOCONVERTPCM
      devices.rs             saídas, cabo virtual ↔ microfone, microfone padrão (IPolicyConfig),
                             alinhamento de taxa dos dois lados do cabo
      mod.rs                 decodificação (PCM, Opus em Rust puro), medidor de nível
    cable.rs                 instalação do VB-CABLE com 1 clique
    settings.rs              configurações em JSON (%APPDATA%\app.micnexus.desktop)
  src-tauri/examples/        headless, fake_phone, formats, bt_probe (testes sem interface)
```

## Microfone virtual

O Windows não deixa um app comum criar um microfone; é preciso um driver de áudio virtual.
O MicNexus usa um **cabo virtual** já instalado e escolhe sozinho, nesta ordem:
**VB-CABLE**, Virtual Audio Cable, VoiceMeeter, **Steam Streaming Microphone** (vem com o Steam).
Sem nenhum, o app mostra **"Instalar"**: baixa o pacote oficial do VB-CABLE (gratuito,
donationware da VB-Audio), confere a assinatura Authenticode e roda o instalador com
`-i -h`. O Windows ainda pede UAC e "Instalar este software de dispositivo?" — é o
consentimento do usuário para um driver.

- **Taxas dos dois lados do cabo:** o driver copia as amostras cruas de "Alto-falantes" para
  "Microfone"; se um lado roda a 48 kHz e o outro a 44,1 kHz, a voz sai mais grave e com falhas.
  O MicNexus põe os dois a 48 kHz (o mesmo que o painel Som → Avançado faz) **ao abrir**, antes de
  outros apps abrirem o microfone. Medido: o Steam Streaming Microphone vinha 48 kHz/44,1 kHz
  (tom de 440 Hz chegava como 404 Hz); depois do ajuste, 440 Hz exatos.
- **Microfone padrão:** com "Virar o microfone padrão do Windows" (padrão: ligado), ao conectar o
  lado microfone do cabo vira o padrão (console, multimídia e comunicações); ao desconectar, volta o
  anterior — só se ninguém trocou no meio. Se o app fechar à força, a restauração acontece na
  próxima abertura.

## Conexões

- **Wi‑Fi:** broadcast UDP a cada 1 s em cada interface + `255.255.255.255`; o celular responde
  quem ele quer; o PC abre o TCP. Tudo de saída: **nenhuma regra de firewall**. Celular novo pelo
  Wi‑Fi pede aprovação na primeira vez (janela + notificação); a lista fica em Configurações.
- **USB:** o `adb.exe` (Apache 2.0, AOSP) vai junto no instalador (`scripts/fetch-adb.mjs`), mas só
  é usado para iniciar o servidor adb se nenhum estiver rodando — um adb do Android Studio
  continua funcionando. Ao plugar: `pm path` confere o app, `am start ... --ez usb true` abre o app,
  `forward tcp:<livre> tcp:47820` e conecta. Se o celular já estava no Wi‑Fi, muda para o cabo.
- **Bluetooth:** servidor RFCOMM com registro SDP no UUID do MicNexus. "Ligar" liga o rádio pela API
  de rádios do Windows; "Parear" abre as configurações de Bluetooth.

## Mutar

Botão grande (com o nível da voz ao vivo), item no menu da bandeja e **atalho global**
(padrão `Ctrl+Shift+M`, configurável, funciona com o app minimizado). O PC silencia na hora e pede
ao celular para mutar; o celular é a fonte da verdade e libera o microfone dele.

## Build

```bash
npm install
npm run desktop:build     # apps/desktop/src-tauri/target/release/bundle/nsis/MicNexus_<v>_x64-setup.exe
```

Nesta máquina: `CARGO_TARGET_DIR=D:/nexus-build/target-micnexus`.
