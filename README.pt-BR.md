<div align="center">

[English](README.md) · [Español](README.es.md) · **Português** · [Français](README.fr.md) · [中文](README.zh-CN.md)

<br>

<img src="docs/readme/boot.gif" width="760" alt="A animação de inicialização do ArchieOS: a marca do Archie se desenha e acende">

<br>
<br>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="display/shell/brand/archieos-logo-dark.png">
  <img src="display/shell/brand/archieos-logo-light.png" width="360" alt="ArchieOS">
</picture>

### Uma IA pessoal que vive dentro do sistema operacional — e pede licença antes de tocar em qualquer coisa.

[![Licença: Apache-2.0](https://img.shields.io/badge/licen%C3%A7a-Apache--2.0-20d79d?style=flat-square)](LICENSE)
![Rust](https://img.shields.io/badge/core-Rust-45e3f5?style=flat-square)
![C++ · Qt 6](https://img.shields.io/badge/display-C%2B%2B%20%C2%B7%20Qt%206-9a75ff?style=flat-square)
![NVIDIA Nemotron](https://img.shields.io/badge/NVIDIA-Nemotron-76b900?style=flat-square)
![Nebius Token Factory](https://img.shields.io/badge/Nebius-Token%20Factory-ffca70?style=flat-square)

</div>

---

## O que é o ArchieOS?

O ArchieOS é um sistema operacional com um assistente, o **Archie**, embutido. Você fala com ele
do seu jeito, em inglês ou espanhol. Ele responde em voz alta, abre seus aplicativos e os usa
como uma pessoa faria: move o próprio ponteiro, clica, digita e desenha.

O que o torna diferente é **quem está no comando**. O modelo de IA nunca toca no seu computador
diretamente. Ele só pode *sugerir* uma ação. O sistema operacional decide se essa ação é
permitida, deixa que ela aconteça uma única vez, confere de novo no último instante e guarda um
recibo. No momento em que você mexe o mouse, o controle volta para você.

<div align="center">
<img src="docs/readme/voice.svg" width="900" alt="Você diz: Archie, desenhe a Mona Lisa. O Nemotron propõe draw_picture. A política permite. O Archie desenha no Photopea.">
</div>

## Veja

**"Archie, draw the Mona Lisa."** O Archie abre o Photopea no navegador, pega o pincel e
desenha o quadro linha por linha com o próprio ponteiro — primeiro sépia, depois sanguínea, como
Leonardo trabalhava — e vai contando o que está fazendo.

<div align="center">
<img src="docs/readme/mona-lisa-timelapse.gif" width="760" alt="Time-lapse: o Archie desenha a Mona Lisa no Photopea, traço a traço">
</div>

**A área de trabalho.** Uma janela só para tudo: o orbe que mostra o que a máquina está fazendo,
o dock (Vault, Forge, Sentry, World, Stage, Ajustes) e o que o Archie abrir por cima. Ela segue
o seu tema claro ou escuro.

<div align="center">
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/readme/desktop-dark.jpg">
  <img src="docs/readme/desktop-light.jpg" width="760" alt="A área de trabalho do ArchieOS: o orbe, a barra superior e o dock">
</picture>
</div>

**World.** A Terra iluminada pelo sol real, sob o céu noturno real, com uma busca discreta (um
lugar ou "lat, lon"), camadas, uma régua e o console do Archie.

<div align="center">
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/readme/world-dark.jpg">
  <img src="docs/readme/world-light.jpg" width="760" alt="World: o globo, iluminado pelo sol, com as constelações atrás">
</picture>
</div>

## Como você continua no comando

<div align="center">
<img src="docs/readme/authority.svg" width="900" alt="O modelo propõe, o Cerebra decide, a política concede uma permissão para uma única operação, o Stage confere de novo e age, e um recibo volta.">
</div>

1. **O modelo propõe.** O Nemotron (na Nebius) ou um modelo local transforma o que você disse em
   uma ação sugerida, como `draw_picture("Mona Lisa")` ou `open_app("Sentry")`.
2. **O Cerebra decide.** A camada de raciocínio do próprio ArchieOS verifica se o pedido faz
   sentido — ou pergunta a você antes.
3. **A política autoriza.** Um arquivo de texto simples (`apps.toml`) diz quais aplicativos o
   Archie pode abrir e o que ele pode fazer em cada um. O que é permitido recebe uma permissão
   selada que vale para uma única operação; todo o resto é recusado.
4. **O Stage age.** O Stage é o sistema de janelas onde o Archie trabalha. Ele confere mais uma
   vez a permissão e a janela de destino, faz exatamente aquela única coisa e devolve um recibo.
5. **Você ganha, sempre.** Toque no mouse ou no teclado e a tela mostra **CONTROLE HUMANO**; o
   Archie para e espera. Depois de alguns segundos de calma, ele pode continuar.

## O que dá para fazer hoje

| | |
| --- | --- |
| 🎙️ **Conversar** | Voz entra, voz sai, na sua máquina. Reconhecimento de fala em streaming NVIDIA Nemotron 3.5, Nemotron na Nebius para a resposta e uma voz local (Piper na CPU, Magpie na GPU). O Archie responde no idioma que você usou. |
| 🖱️ **Agir** | Abrir apps, clicar, digitar, rolar, arrastar e desenhar — sempre pelo Stage e sua política. O Archie encontra botões primeiro pela acessibilidade e só olha a tela quando precisa. |
| 🎨 **Desenhar** | "Desenhe a Mona Lisa" (e outros quadros que ele conhece) no Photopea, em ritmo humano, fácil de acompanhar em vídeo. |
| 🌍 **Explorar** | World: o globo, a luz do sol real, as estrelas reais, lugares por nome ou coordenadas. |
| 🛡️ **Vigiar** | Sentry: o que existe na sua rede e quem fala com quem. |
| 👁️ **Ver** | IRIS, os olhos do Archie: diga o que procurar e ela diz onde está. Rostos são sempre cobertos — ela nunca identifica pessoas. |
| 🎬 **Editar vídeo** | Linhas do tempo com clipes, títulos, música e transições renderizadas pelo MLT (o motor do Kdenlive), sem janela. |
| 🧩 **Conectar** | As ferramentas do Archie para agentes de programação via MCP. |

## Experimente

O ArchieOS roda como um sistema Linux: em um PC, em uma máquina virtual ou no **WSL2 no
Windows**.

**No Windows (WSL2).** Gere a imagem e instale:

```bash
sudo ./scripts/wsl/build-rootfs.sh ./dist
```

```powershell
wsl --install --from-file .\dist\ArchieOS.wsl
```

Depois siga a [primeira execução](docs/FIRST-RUN.md). O [guia de build do WSL](docs/WSL-BUILD.md)
e a [fronteira de segurança](docs/SECURITY-BOUNDARY.md) explicam o que a imagem contém e o que
ela mantém separado.

**A demo da Mona Lisa.** Dentro do ArchieOS (ou de qualquer Linux com as peças compiladas), um
único comando abre a área de trabalho, o navegador e o Archie ouvindo em inglês e espanhol:

```bash
display/session/nebius-demo.sh
```

O que compilar antes, as opções e um roteiro sugerido estão em
[`docs/DEMO-NEBIUS.md`](docs/DEMO-NEBIUS.md).

<details>
<summary><b>Compilar a partir do código</b></summary>

<br>

O plano de controle é Rust (1.88+); a área de trabalho e o Stage são C++ com Qt 6.

```bash
# A voz, as mãos e as ferramentas do Archie
cd core
cargo build --release -p archie-voice --features speech
cargo test --workspace --locked

# O Stage (compositor Wayland) e a área de trabalho
cmake -S display/stage  -B build/stage   -G Ninja && cmake --build build/stage
cmake -S display/native -B build/display -G Ninja && cmake --build build/display
```

`--features speech` precisa do NeMo-Speech.cpp (`NEMO_SPEECH_PREFIX`) e do PulseAudio;
`scripts/models/install-nemotron-speech.sh` baixa o modelo de reconhecimento fixado, e
`scripts/models/install-piper-runtime.sh` / `install-piper-voices.sh` as vozes locais.

Converse com o Archie e o Stage usando uma chave da Nebius Token Factory:

```bash
NEBIUS_TOKEN_FACTORY_KEY=... display/stage/run-archie.sh public
```

O modelo que responde é substituível: NVIDIA NIM, Nebius Token Factory ou qualquer servidor local
compatível com OpenAI. As chaves são lidas no momento de cada requisição e nunca são gravadas no
repositório nem na imagem. `scripts/check.sh` roda as verificações que valem a pena antes de um
push.

</details>

## Por dentro

```text
 microfone ─► Nemotron 3.5 ASR ─► turno ─► Nemotron (Nebius) ─► voz (Piper / Magpie)
                                              │ propõe
 câmera ─► IRIS ─► cena + memória ────────────┤
                                              ▼
                                Cerebra ─► Política ─► Stage ─► aplicativos
                                              │
                        sinais TFOG ─► archie-displayd ─► área de trabalho (Orbe · Forge · World)
```

- **Rust** para a autoridade, a política, o escalonador, o roteador de modelos, a memória e o
  estado (PostgreSQL/TimescaleDB), o Cerebra e o Vault.
- **C++ / Qt Quick 3D** para a área de trabalho, o Orbe, o World, o Cortex, o Sentry e o Stage.
- Identidade, memória, política e autoridade vivem no ArchieOS; os modelos de linguagem e de visão
  são recursos substituíveis que ele chama.

## Status

É uma versão alfa, feita para o hackathon NVIDIA × Nebius. Com sinceridade:

- **Funciona e foi testado:** voz para voz em inglês e espanhol, o Stage e sua política, abrir e
  usar apps por voz, o desenho da Mona Lisa, o World, a área de trabalho clara e escura, a
  detecção da IRIS, a renderização de vídeo, a imagem WSL2.
- **Em andamento:** o treinamento da IRIS Beta 2; o Cortex conduzindo a vista da área de trabalho
  por comando; a IRIS propondo ações além de cliques; o build CUDA do runtime da IRIS sem Python.
- **Limites conhecidos:** o desenho vai em ritmo de pessoa (minutos, não segundos); na imagem
  ArchISO os apps abrem pelo Stage e não pelo Forge; a CI roda localmente enquanto o GitHub
  Actions está pausado.

## Saiba mais

[Arquitetura](ARCHITECTURE.md) · [Archie](ARCHIE.md) · [Procedência do hackathon](HACKATHON.md) ·
[Stage](docs/STAGE.md) · [Fala](docs/SPEECH-NEMOTRON.md) · [IRIS](docs/IRIS.md) ·
[Runtime da IRIS](docs/IRIS-RUNTIME.md) · [Segurança](docs/SECURITY.md) ·
[World](docs/WORLD-EXPLORATION.md) · [Renderização do orbe](docs/TFOG-ORB-RENDERING.md)

## Créditos

- **PentaVM** — a arquitetura, a implementação em Rust e C++, o plano de controle, a IRIS, a voz
  e a área de trabalho nativa.
- **Xavi** ([`ByXavi270`](https://github.com/ByXavi270) / XaviCottier) — o design: a área de
  trabalho, a esfera neural, o logo e sua animação de inicialização.
- **Santiago Rearte** (Sheriff) — a integração com a Nebius Token Factory e o Nemotron, e os
  testes no seu próprio WSL.
- Feito sobre NVIDIA Nemotron, NeMo e NIM, Nebius Token Factory, Piper, MLT, Qt e Wayland.

## Licença

Apache License 2.0 — veja [LICENSE](LICENSE) e [NOTICE](NOTICE). Apache e não MIT pela concessão
de patentes: este repositório contém controle de dispositivos, atuação física e roteamento de
inferência. Os provedores de dados mantêm seus próprios termos; `providers/` registra a atribuição
que cada um exige, e o ArchieOS se recusa a mostrar dados sem atribuição.

<div align="center">

<br>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="display/shell/brand/archieos-icon-256.png">
  <img src="display/shell/brand/archieos-icon-light-256.png" width="56" alt="">
</picture>

**Made with love ♥**

*por PentaVM, Xavi e Sheriff*

</div>
