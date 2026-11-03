# 🎹 klav

Aplicativo de partituras e arranjos **moderno, ultraleve e cross-platform**, com navegação
estilo IDE (inspirada no VS Code) e foco em **instrumentos de teclas** (piano, teclado,
sintetizador). Escrito em **Rust**, renderizando em **SVG** e rodando como app desktop via
**Dioxus** (webview nativo) — com o mesmo núcleo compilável para **WebAssembly**.

> 🚧 **Em desenvolvimento.** O MVP foca em pauta dupla (Sol/Fá), pauta simples (Sol) e
> lead sheets (cifras).

---

## Arquitetura

O núcleo é **headless** (não depende da UI) e dividido em crates:

| Crate | Responsabilidade |
|-------|------------------|
| [`klav-core`](crates/klav-core) | Modelo de dados musical puro (`Score` → `Measure` → `Voice` → `ChordOrRest` → `Note`), `Fraction` racional para tempo, e validação de invariantes. |
| [`klav-io`](crates/klav-io) | Formato nativo `.klav` (JSON versionado via `serde`): salvar/carregar. |
| [`klav-engine`](crates/klav-engine) | Playback lógico: converte o `Score` em eventos MIDI temporizados (trata ties, tuplets) e tempo musical → segundos. |
| [`klav-layout`](crates/klav-layout) | Geometria agnóstica de backend: posição das notas em *staff-spaces* + glifos SMuFL. |
| [`klav-app`](crates/klav-app) | App Dioxus: render SVG, abas/explorer, playback (cursor + teclado + áudio) e edição. |

Decisões de arquitetura e o plano de fatias estão em [`docs/arquitetura.md`](docs/arquitetura.md);
a visão de produto em [`docs/klav_roadmap.md`](docs/klav_roadmap.md).

## Funcionalidades atuais

- Renderização da partitura em SVG com a fonte musical **Bravura** (SMuFL)
- Abrir/salvar arquivos `.klav`; explorer de pasta e abas (múltiplos arquivos)
- Playback: tocar/pausar/parar, cursor deslizando, teclado de 88 teclas acendendo
- Áudio por síntese (oscilador + envelope), com seleção de dispositivo de saída
- Edição inicial: selecionar nota e mover a altura (↑/↓)

## Como rodar

Pré-requisitos:

- **Rust** (via [rustup](https://rustup.rs))
- **Windows:** Visual Studio **Build Tools** com o workload *C++* (o toolchain MSVC precisa do
  linker `link.exe`). Rode o `cargo` a partir de um *Developer PowerShell / VS Dev Shell*.
- Linux/macOS: toolchain nativo padrão.

```bash
# app desktop
cargo run -p klav-app

# testes do núcleo
cargo test
```

Antes de commitar, rode as validações (formatação, clippy com warnings como erro e testes)
de uma vez — silencioso no sucesso, detalha só o que falhar:

```powershell
pwsh scripts/check.ps1          # tudo
pwsh scripts/check.ps1 clippy   # só uma etapa: fmt | clippy | test
```

## Roadmap (resumo)

Próximo foco: **editor de conteúdo** (adicionar/remover notas e compassos, entrada por teclado),
depois undo/redo, mixer multi-pauta, empacotamento com Tauri e som via SoundFont. Detalhes em
[`docs/arquitetura.md`](docs/arquitetura.md) §7.

## Licença

- Código: **MIT**.
- Fonte **Bravura** embarcada em `crates/klav-app/assets/`: **SIL Open Font License 1.1**
  (ver `crates/klav-app/assets/OFL.txt`), © Steinberg Media Technologies GmbH.
