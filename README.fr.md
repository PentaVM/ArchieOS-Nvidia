<div align="center">

[English](README.md) · [Español](README.es.md) · [Português](README.pt-BR.md) · **Français** · [中文](README.zh-CN.md)

<br>

<img src="docs/readme/boot.gif" width="760" alt="L'animation de démarrage d'ArchieOS : le symbole d'Archie se dessine et s'allume">

<br>
<br>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="display/shell/brand/archieos-logo-dark.png">
  <img src="display/shell/brand/archieos-logo-light.png" width="360" alt="ArchieOS">
</picture>

### Une IA personnelle qui vit dans le système d'exploitation — et qui demande avant de toucher à quoi que ce soit.

[![Licence : Apache-2.0](https://img.shields.io/badge/licence-Apache--2.0-20d79d?style=flat-square)](LICENSE)
![Rust](https://img.shields.io/badge/core-Rust-45e3f5?style=flat-square)
![C++ · Qt 6](https://img.shields.io/badge/display-C%2B%2B%20%C2%B7%20Qt%206-9a75ff?style=flat-square)
![NVIDIA Nemotron](https://img.shields.io/badge/NVIDIA-Nemotron-76b900?style=flat-square)
![Nebius Token Factory](https://img.shields.io/badge/Nebius-Token%20Factory-ffca70?style=flat-square)

</div>

---

## Qu'est-ce qu'ArchieOS ?

ArchieOS est un système d'exploitation avec un assistant intégré, **Archie**. Vous lui parlez
avec vos propres mots, en anglais ou en espagnol. Il répond à voix haute, ouvre vos applications
et s'en sert comme le ferait une personne : il déplace son propre pointeur, clique, tape et
dessine.

Ce qui le distingue, c'est **qui décide**. Le modèle d'IA ne touche jamais directement votre
ordinateur. Il peut seulement *proposer* une action. Le système d'exploitation décide si cette
action est autorisée, la laisse se produire une seule fois, la vérifie encore au dernier moment
et en garde un reçu. Dès que vous bougez la souris, vous reprenez la main.

<div align="center">
<img src="docs/readme/voice.svg" width="900" alt="Vous dites : Archie, dessine la Joconde. Nemotron propose draw_picture. La politique l'autorise. Archie dessine dans Photopea.">
</div>

## Voyez par vous-même

**« Archie, draw the Mona Lisa. »** Archie ouvre Photopea dans le navigateur, prend le pinceau et
dessine le tableau trait par trait avec son propre pointeur — d'abord la sépia, puis la
sanguine, comme travaillait Léonard — en racontant ce qu'il fait.

<div align="center">
<img src="docs/readme/mona-lisa-timelapse.gif" width="760" alt="Accéléré : Archie dessine la Joconde dans Photopea, trait par trait">
</div>

**Le bureau.** Une seule fenêtre pour tout : l'orbe qui montre ce que fait la machine, le dock
(Vault, Forge, Sentry, World, Stage, Réglages) et ce qu'Archie ouvre par-dessus. Il suit votre
thème clair ou sombre.

<div align="center">
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/readme/desktop-dark.jpg">
  <img src="docs/readme/desktop-light.jpg" width="760" alt="Le bureau d'ArchieOS : l'orbe, la barre du haut et le dock">
</picture>
</div>

**World.** La Terre éclairée par le vrai soleil, sous le vrai ciel nocturne, avec une recherche
discrète (un lieu ou « lat, lon »), des calques, une règle et la console d'Archie.

<div align="center">
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/readme/world-dark.jpg">
  <img src="docs/readme/world-light.jpg" width="760" alt="World : le globe, éclairé par le soleil, avec les constellations derrière">
</picture>
</div>

## Comment vous gardez la main

<div align="center">
<img src="docs/readme/authority.svg" width="900" alt="Le modèle propose, Cerebra décide, la politique accorde un bail pour une seule opération, le Stage vérifie encore et agit, et un reçu revient.">
</div>

1. **Le modèle propose.** Nemotron (chez Nebius) ou un modèle local transforme ce que vous avez
   dit en une action suggérée, comme `draw_picture("Mona Lisa")` ou `open_app("Sentry")`.
2. **Cerebra décide.** La couche de raisonnement propre à ArchieOS vérifie que la demande a du
   sens — ou vous pose d'abord une question.
3. **La politique autorise.** Un simple fichier texte (`apps.toml`) dit quelles applications
   Archie peut ouvrir et ce qu'il peut y faire. Ce qui est permis reçoit un bail scellé valable
   pour une seule opération ; tout le reste est refusé.
4. **Le Stage agit.** Le Stage est le système de fenêtres dans lequel Archie travaille. Il
   vérifie une dernière fois le bail et la fenêtre visée, fait exactement cette seule chose et
   renvoie un reçu.
5. **Vous gagnez, toujours.** Touchez la souris ou le clavier et l'écran affiche
   **CONTRÔLE HUMAIN** ; Archie s'arrête et attend. Après quelques secondes de calme, il peut
   reprendre.

## Ce que vous pouvez faire aujourd'hui

| | |
| --- | --- |
| 🎙️ **Parler** | La voix entre, la voix sort, sur votre machine. Reconnaissance vocale en streaming NVIDIA Nemotron 3.5, Nemotron chez Nebius pour la réponse et une voix locale (Piper sur CPU, Magpie sur GPU). Archie répond dans la langue que vous avez utilisée. |
| 🖱️ **Agir** | Ouvrir des applis, cliquer, taper, faire défiler, glisser et dessiner — toujours par le Stage et sa politique. Archie trouve les boutons d'abord par l'accessibilité et ne regarde l'écran que lorsqu'il le faut. |
| 🎨 **Dessiner** | « Dessine la Joconde » (et d'autres tableaux qu'il connaît) dans Photopea, à un rythme humain qu'on peut suivre en vidéo. |
| 🌍 **Explorer** | World : le globe, la vraie lumière du soleil, les vraies étoiles, des lieux par nom ou coordonnées. |
| 🛡️ **Surveiller** | Sentry : ce qui se trouve sur votre réseau et qui parle à qui. |
| 👁️ **Voir** | IRIS, les yeux d'Archie : dites-lui quoi chercher, elle vous dit où c'est. Les visages sont toujours masqués — elle n'identifie jamais personne. |
| 🎬 **Monter des vidéos** | Des montages de clips, titres, musique et fondus rendus par MLT (le moteur de Kdenlive), sans fenêtre. |
| 🧩 **Connecter** | Les outils d'Archie pour les agents de programmation via MCP. |

## Essayez-le

ArchieOS fonctionne comme un système Linux : sur un PC, dans une machine virtuelle ou sous
**WSL2 sous Windows**.

**Sous Windows (WSL2).** Construisez l'image et installez-la :

```bash
sudo ./scripts/wsl/build-rootfs.sh ./dist
```

```powershell
wsl --install --from-file .\dist\ArchieOS.wsl
```

Suivez ensuite le [premier démarrage](docs/FIRST-RUN.md). Le [guide de construction WSL](docs/WSL-BUILD.md)
et la [frontière de sécurité](docs/SECURITY-BOUNDARY.md) expliquent ce que contient l'image et ce
qu'elle tient à l'écart.

**La démo de la Joconde.** Dans ArchieOS (ou tout Linux où les pièces sont compilées), une seule
commande ouvre le bureau, le navigateur et Archie à l'écoute en anglais et en espagnol :

```bash
display/session/nebius-demo.sh
```

Ce qu'il faut compiler avant, les options et un scénario proposé sont dans
[`docs/DEMO-NEBIUS.md`](docs/DEMO-NEBIUS.md).

<details>
<summary><b>Compiler depuis les sources</b></summary>

<br>

Le plan de contrôle est en Rust (1.88+) ; le bureau et le Stage sont en C++ avec Qt 6.

```bash
# La voix, les mains et les outils d'Archie
cd core
cargo build --release -p archie-voice --features speech
cargo test --workspace --locked

# Le Stage (compositeur Wayland) et le bureau
cmake -S display/stage  -B build/stage   -G Ninja && cmake --build build/stage
cmake -S display/native -B build/display -G Ninja && cmake --build build/display
```

`--features speech` nécessite NeMo-Speech.cpp (`NEMO_SPEECH_PREFIX`) et PulseAudio ;
`scripts/models/install-nemotron-speech.sh` télécharge le modèle de reconnaissance épinglé, et
`scripts/models/install-piper-runtime.sh` / `install-piper-voices.sh` les voix locales.

Parlez à Archie avec le Stage, avec une clé Nebius Token Factory :

```bash
NEBIUS_TOKEN_FACTORY_KEY=... display/stage/run-archie.sh public
```

Le modèle qui répond est interchangeable : NVIDIA NIM, Nebius Token Factory ou tout serveur local
compatible OpenAI. Les clés sont lues au moment de chaque requête et ne sont jamais écrites dans
le dépôt ni dans l'image. `scripts/check.sh` lance les vérifications utiles avant un push.

</details>

## Sous le capot

```text
 micro ─► Nemotron 3.5 ASR ─► tour de parole ─► Nemotron (Nebius) ─► voix (Piper / Magpie)
                                                   │ propose
 caméra ─► IRIS ─► scène + mémoire ────────────────┤
                                                   ▼
                                    Cerebra ─► Politique ─► Stage ─► applications
                                                   │
                             signaux TFOG ─► archie-displayd ─► bureau (Orbe · Forge · World)
```

- **Rust** pour l'autorité, la politique, l'ordonnanceur, le routeur de modèles, la mémoire et
  l'état (PostgreSQL/TimescaleDB), Cerebra et le Vault.
- **C++ / Qt Quick 3D** pour le bureau, l'Orbe, World, Cortex, Sentry et le Stage.
- L'identité, la mémoire, la politique et l'autorité vivent dans ArchieOS ; les modèles de langage
  et de vision sont des ressources interchangeables qu'il appelle.

## État du projet

C'est une alpha, conçue pour le hackathon NVIDIA × Nebius. En toute honnêteté :

- **Fonctionne et testé :** la voix de bout en bout en anglais et en espagnol, le Stage et sa
  politique, ouvrir et utiliser des applis à la voix, le dessin de la Joconde, World, le bureau
  clair et sombre, la détection d'IRIS, le rendu vidéo, l'image WSL2.
- **En cours :** l'entraînement d'IRIS Beta 2 ; Cortex pilotant la vue du bureau sur commande ;
  IRIS proposant des actions au-delà des clics ; la compilation CUDA du runtime IRIS sans Python.
- **Limites connues :** le dessin avance au rythme d'une personne (des minutes, pas des
  secondes) ; dans l'image ArchISO, les applis s'ouvrent par le Stage et non depuis Forge ; la CI
  tourne en local tant que GitHub Actions est en pause.

## Pour aller plus loin

[Architecture](ARCHITECTURE.md) · [Archie](ARCHIE.md) · [Provenance du hackathon](HACKATHON.md) ·
[Stage](docs/STAGE.md) · [Parole](docs/SPEECH-NEMOTRON.md) · [IRIS](docs/IRIS.md) ·
[Runtime IRIS](docs/IRIS-RUNTIME.md) · [Sécurité](docs/SECURITY.md) ·
[World](docs/WORLD-EXPLORATION.md) · [Rendu de l'orbe](docs/TFOG-ORB-RENDERING.md)

## Crédits

- **PentaVM** — l'architecture, l'implémentation en Rust et C++, le plan de contrôle, IRIS, la
  voix et le bureau natif.
- **Xavi** ([`ByXavi270`](https://github.com/ByXavi270) / XaviCottier) — le design : le bureau,
  la sphère neuronale, le logo et son animation de démarrage.
- **Santiago Rearte** (Sheriff) — l'intégration de Nebius Token Factory et de Nemotron, et les
  tests sur son propre WSL.
- Construit sur NVIDIA Nemotron, NeMo et NIM, Nebius Token Factory, Piper, MLT, Qt et Wayland.

## Licence

Apache License 2.0 — voir [LICENSE](LICENSE) et [NOTICE](NOTICE). Apache plutôt que MIT pour sa
concession de brevets : ce dépôt contient du contrôle d'appareils, de l'actionnement physique et
du routage d'inférence. Les fournisseurs de données gardent leurs propres conditions ;
`providers/` consigne l'attribution que chacun exige, et ArchieOS refuse d'afficher des données
sans attribution.

<div align="center">

<br>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="display/shell/brand/archieos-icon-256.png">
  <img src="display/shell/brand/archieos-icon-light-256.png" width="56" alt="">
</picture>

**Made with love ♥**

*par PentaVM, Xavi et Sheriff*

</div>
