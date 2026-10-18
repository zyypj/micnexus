# MicNexus

Transforma o celular **Android** ou **iPhone** em microfone do PC **Windows**, por **Wi‑Fi**,
**cabo USB** ou **Bluetooth** — conectando sozinho. Botão de mutar nos dois lados, app em segundo
plano no celular com notificação permanente (só enquanto conectado) com **Mutar** e
**Desconectar** — no iPhone, uma Live Activity na tela de bloqueio e na Dynamic Island.
Mesmo padrão e identidade visual do [Nexus](https://github.com/zyypj/nexus).

## 📱 Como instalar no iPhone (passo a passo)

> Leva uns 10 minutos na primeira vez. Você precisa de: o **iPhone**, o **cabo** dele, um **PC
> com Windows** e o seu **Apple ID** (o e-mail e a senha da sua conta Apple — é de graça).

### Parte 1 — Baixar as coisas no PC

Baixe e instale estas **3 coisas** no PC, nesta ordem:

1. **Dispositivos Apple** (é o que faz o PC enxergar o iPhone):
   abra este link e clique em **Obter** → https://apps.microsoft.com/detail/9np83lwlpz9k
2. **Sideloadly** (é o programa que coloca o app no iPhone):
   entre em https://sideloadly.io, clique em **Download** para **Windows** e instale
   (é só ir clicando em "Avançar").
3. **O app MicNexus para iPhone** (um arquivo chamado `micnexus-ios.ipa`):
   clique aqui para baixar → https://github.com/zyypj/micnexus/releases/latest/download/micnexus-ios.ipa
   (ele vai para a pasta **Downloads**).

### Parte 2 — Ligar o iPhone no PC

4. Ligue o iPhone no PC com o **cabo**.
5. **Desbloqueie o iPhone** (tire da tela de bloqueio).
6. Vai aparecer no iPhone a pergunta **"Confiar neste computador?"** → toque em **Confiar** e
   digite a **senha do iPhone**.

### Parte 3 — Instalar com o Sideloadly

7. Abra o **Sideloadly**. A tela é assim:

   ```
   ┌────────┐  iDevice:   [ nome do seu iPhone        ▼ ]
   │  IPA   │  Apple ID:  [ seu e-mail da Apple       ▼ ]
   └────────┘  [            Advanced Options            ]
               [                 Start                  ]
   ```

8. **iDevice:** precisa aparecer o **nome do seu iPhone**.
   Se aparecer `<no devices detected>` (nenhum aparelho), veja "Deu problema?" lá embaixo.
9. **O quadrado escrito IPA** (à esquerda): **arraste** para dentro dele o arquivo
   `micnexus-ios.ipa` que está na pasta **Downloads** (ou clique no quadrado e escolha o arquivo).
10. **Apple ID:** digite o **e-mail da sua conta Apple**.
11. Clique em **Start**.
12. O Sideloadly vai pedir a **senha da sua conta Apple** → digite.
    Se aparecer um **código de 6 números no iPhone**, digite esse código no Sideloadly.
13. Espere. Quando aparecer **Done** (pronto) lá embaixo, o app já está no iPhone. 🎉
    **Não tire o cabo antes disso.**

### Parte 4 — Liberar o app no iPhone (só na primeira vez)

O iPhone bloqueia apps instalados assim até você liberar:

14. No iPhone, abra **Ajustes → Geral → VPN e Gerenciamento de Dispositivos**.
15. Toque no **seu e-mail** → toque em **Confiar** → **Confiar** de novo.
16. Ainda nos **Ajustes**, vá em **Privacidade e Segurança** → role até o fim → **Modo de
    Desenvolvedor** → **ligue** a chave → o iPhone pede para **reiniciar** → toque em
    **Reiniciar**. Quando ligar de novo, aparece um aviso → toque em **Ativar** e digite a
    senha do iPhone.
17. Pronto! Abra o **MicNexus** no iPhone e toque em **Permitir** em tudo que ele pedir
    (microfone e rede local).

### Parte 5 — Usar

18. No PC, instale o **MicNexus para Windows**: baixe o arquivo que termina em
    `x64-setup.exe` na página https://github.com/zyypj/micnexus/releases/latest e abra.
19. Com o iPhone e o PC **no mesmo Wi‑Fi**, abra o MicNexus nos dois. O nome do PC aparece no
    iPhone → **toque nele**. Na primeira vez, clique em **Permitir** no PC.
20. No Discord (ou outro app), escolha como microfone o que o MicNexus mostra no PC
    (algo como **CABLE Output**).

### ⏰ Importante: o app vence a cada 7 dias

Como a conta Apple é grátis, o app **para de abrir depois de 7 dias**. Para renovar, é só
repetir a **Parte 2** e a **Parte 3** (ligar no cabo e clicar em **Start** de novo no
Sideloadly). Não precisa apagar nada: as configurações continuam.

### Deu problema?

| O que aparece | O que fazer |
|---|---|
| `<no devices detected>` no Sideloadly | Desbloqueie o iPhone, toque em **Confiar**, confira se instalou o **Dispositivos Apple** (passo 1), tente outro cabo ou outra porta USB e reabra o Sideloadly. |
| Erro de senha / login no Sideloadly | Confira o e-mail e a senha da conta Apple. Se pedir código, ele aparece no iPhone. |
| "Desenvolvedor não confiável" ao abrir o app | Faça a **Parte 4** (passos 14 e 15). |
| Não acho "Modo de Desenvolvedor" | Ele só aparece **depois** de instalar o app pelo Sideloadly. Instale primeiro (Parte 3). |
| O app abre e fecha sozinho | Passou dos 7 dias: renove (Parte 2 + Parte 3). |
| O PC não aparece no iPhone | iPhone e PC no **mesmo Wi‑Fi**, MicNexus **aberto no PC**, e o iPhone com **rede local** permitida (Ajustes → MicNexus → Rede Local). Ou use o **cabo**: com o app aberto no iPhone, ele conecta sozinho. |
| "Limite de apps" | Contas grátis só podem ter **3 apps** instalados assim ao mesmo tempo: apague um outro que você instalou pelo Sideloadly. |

Detalhes técnicos do iPhone em [docs/IOS.md](docs/IOS.md).

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
