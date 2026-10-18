# Protocolo MicNexus (v1)

Os três transportes carregam exatamente o mesmo fluxo de bytes, então o código de sessão é um
só nos dois lados.

| Transporte | Quem abre a conexão | Como | Confiança |
|---|---|---|---|
| Wi‑Fi | **PC → celular** | descoberta UDP (o PC pergunta, o celular responde), depois TCP no celular | aprovar no PC na 1ª vez |
| Cabo USB | **PC → celular** | `adb forward tcp:<livre> tcp:47820`; o PC conecta em `127.0.0.1:<livre>` | automática (acesso físico) |
| Bluetooth | celular → PC | RFCOMM no UUID do serviço (registro SDP feito pelo PC) | automática (pareado no sistema) |

**Por que o PC conecta no celular:** o Firewall do Windows bloqueia conexões *de entrada* por
padrão (e liberar exige administrador). Conexões de saída e respostas unicast a broadcast
(janela de 3 s, padrão do Windows) passam sem configurar nada. O Android não tem firewall de
entrada. O Bluetooth não passa pelo firewall, então ali o celular conecta no PC.

### iPhone

O iOS não deixa um app receber broadcast UDP (exige um direito especial da Apple), não tem adb
nem Bluetooth clássico. Por isso, com iPhone:

| Transporte | Quem abre a conexão | Como | Confiança |
|---|---|---|---|
| Wi‑Fi | **PC → iPhone** | o iPhone se anuncia por **Bonjour** (`_micnexus._tcp`); o PC pergunta por mDNS *legacy unicast* (de uma porta ≠ 5353, então a resposta vem em unicast e o firewall deixa passar) e abre o TCP | aprovar no PC na 1ª vez |
| Cabo USB | **PC → iPhone** | **usbmuxd** (serviço "Apple Mobile Device" do app Dispositivos Apple/iTunes, `127.0.0.1:27015`): `Connect` à porta 47820 do iPhone | automática |
| Bluetooth (BLE) | **PC → iPhone** | o iPhone é periférico GATT; o PC (central) vê o anúncio, conecta e troca os mesmos frames por duas características | aprovar no PC na 1ª vez |

Como o iPhone não ouve a descoberta do PC, ele monta a lista de PCs pelos `INFO` que recebe: o PC
conecta em cada iPhone encontrado a cada ~3 s (presença) e o iPhone responde `BYE idle` quando
não quer aquele PC. O registro TXT do Bonjour traz `want` para o PC conectar na hora.

**Bonjour** (`_micnexus._tcp`, porta 47820), TXT: `v=1`, `id=<id do celular>`, `name=<nome>`,
`want=<id do PC>` (vazio se nenhum).

**BLE:**

| | UUID |
|---|---|
| Serviço | `a3c8e0f2-5b17-4d9a-8e61-0c4f2b9d7a16` |
| `rx` (PC → iPhone, write without response) | `a3c8e0f2-5b17-4d9a-8e61-0c4f2b9d7a17` |
| `tx` (iPhone → PC, notify) | `a3c8e0f2-5b17-4d9a-8e61-0c4f2b9d7a18` |

As duas características carregam o **mesmo fluxo de bytes** do TCP, fatiado no MTU. O nome local
do anúncio é `MNX*` (quer qualquer PC — o usuário tocou em "Bluetooth") ou `MNX` + os 8 primeiros
caracteres do id do PC que ele quer. Codec no BLE: Opus 48 kHz (como no Bluetooth do Android).

## Constantes

| Nome | Valor |
|---|---|
| Porta TCP (no celular) | `47820` |
| Porta UDP de descoberta (no celular) | `47821` |
| UUID Bluetooth (RFCOMM/SDP) | `a3c8e0f2-5b17-4d9a-8e61-0c4f2b9d7a15` |
| Versão do protocolo | `1` |

## Descoberta (Wi‑Fi)

O PC envia a cada 1 s (3 s com um celular já conectado), para o broadcast de cada interface e
para `255.255.255.255`, porta `47821`:

```
"MNX?" + JSON {"v":1,"id":"<id do PC>","name":"DESKTOP-ABC","version":"0.1.0","busy":false}
```

O celular (app aberto ou serviço rodando) responde em unicast para a origem:

```
"MNX!" + JSON {"v":1,"id":"<id do celular>","name":"Galaxy S23","model":"SM-S911B",
               "port":47820,"want":"<id do PC>"|null}
```

`want` é o PC com que o celular quer falar agora (o usuário tocou nele, ou é o último PC e a
conexão automática está ligada). Quando `want` é o próprio id, o PC abre o TCP para
`<ip do celular>:<port>`. O celular monta a lista de PCs a partir das perguntas que recebe.

## Fluxo (TCP / RFCOMM)

Cada mensagem é um frame:

```
u8   tipo
u16  tamanho do payload (little-endian)
...  payload
```

| Tipo | Nome | Direção | Payload |
|---|---|---|---|
| 1 | `HELLO` | celular → PC | JSON (abaixo) |
| 2 | `WELCOME` | PC → celular | JSON (abaixo) |
| 3 | `PENDING` | PC → celular | vazio — aguardando o usuário aprovar no PC |
| 4 | `AUDIO` | celular → PC | `u32 seq` (LE) + dados do codec |
| 5 | `MUTE` | ambos | 1 byte: `1` mutado, `0` aberto |
| 6 | `PING` | celular → PC | `u64` relógio do celular em ms + `u32` último RTT medido (opcional), LE |
| 7 | `PONG` | PC → celular | os mesmos 8 bytes do relógio do PING |
| 8 | `BYE` | ambos | JSON `{"reason":"user"|"rejected"|"busy"|"replaced"|"idle"|"version"|"timeout"}` |
| 10 | `INFO` | PC → celular | primeiro frame de toda conexão: `{"v":1,"id","name","version","busy"}` |
| 11 | `STATS` | PC → celular | JSON `{"bufferMs":40,"underruns":0}` a cada 1 s |

Tipos desconhecidos são ignorados (compatibilidade futura).

### HELLO

```json
{
  "v": 1,
  "id": "6f1c…",            // UUID gerado na instalação do app
  "name": "Galaxy S23",     // nome do aparelho
  "model": "SM-S911B",
  "version": "0.1.0",
  "codecs": ["opus", "pcm16"],
  "muted": false
}
```

### WELCOME

```json
{ "v": 1, "id": "<id do PC>", "name": "DESKTOP-ABC", "version": "0.1.0",
  "codec": "pcm16", "rate": 48000, "frameMs": 10 }
```

Escolha do codec pelo PC:

- Wi‑Fi e USB: `pcm16` 48 kHz mono, frames de 10 ms (960 bytes) — sem perda e sem atraso de
  codificação.
- Bluetooth: `opus` 48 kHz mono 20 ms (~64 kbit/s) se o celular oferecer (MediaCodec, Android 10+);
  senão `pcm16` a 24 kHz (384 kbit/s).

### Sequência

```
celular                          PC
         ◀─────────────────────── INFO             (assim que a conexão abre)
  HELLO  ───────────────────────▶                  (ou BYE {"reason":"idle"|"busy"})
         ◀─────────────────────── PENDING          (só Wi‑Fi de aparelho novo)
         ◀─────────────────────── WELCOME          (ou BYE {"reason":"rejected"})
  AUDIO… ───────────────────────▶
  PING   ───────────────────────▶ (1/s)
         ◀─────────────────────── PONG, STATS
  MUTE 1 ───────────────────────▶ (o celular é a fonte da verdade do mute)
         ◀─────────────────────── MUTE 1           (pedido do PC; o celular aplica e confirma)
  BYE    ───────────────────────▶ (ou ◀── BYE)
```

- Numa conexão aberta pelo PC, o celular decide pelo `INFO`: aceita se é o PC que ele quer
  (`want`) ou se veio pelo cabo; senão responde `BYE idle` e o PC tenta de novo mais tarde.
- O **celular é dono do estado de mute**: o PC manda `MUTE` como pedido, o celular aplica, para o
  microfone e devolve `MUTE` com o estado real. O PC silencia na hora (não espera a confirmação).
- Mutado, o celular **libera o microfone** (o indicador verde do Android apaga) e não envia
  `AUDIO`; o PC entrega silêncio ao microfone virtual.
- Sem nenhum frame por 6 s, qualquer lado considera a conexão morta.
- Quando a conexão cai sem `BYE`, o celular continua pedindo o mesmo PC (`want`, ou reconectando
  o Bluetooth) por até 60 s.
- Um segundo celular recebe `BYE {"reason":"busy"}`; o mesmo celular reconectando substitui a
  sessão antiga (`replaced`).

## Atraso

O PC mantém um buffer adaptativo: alvo inicial de 30 ms (Wi‑Fi/USB) ou 60 ms (Bluetooth), sobe
quando falta áudio e desce devagar quando a rede está estável. Se o buffer passa de 2× o alvo
+ 20 ms (rajada depois de uma retransmissão TCP), o excesso é descartado com um crossfade de
2 ms: o atraso nunca acumula.
