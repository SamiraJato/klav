# 🎹 klav — Product Roadmap & Architectural Vision

> **klav** é um aplicativo moderno, ultraleve e cross-platform para notação, arranjo e composição musical. Projetado para substituir ferramentas legadas com uma experiência orientada a alta performance (Rust/WASM), visual estilo IDE (inspirado no VS Code) e foco em instrumentos de teclado.

---

## 🎯 Visão Geral do Ecossistema

- **Core Engine:** Rust (Compilado para código nativo no Desktop via Tauri e WebAssembly para Web).
- **Interface/UX:** UI moderna baseada em componentes estilo IDE, layout configurável por abas, árvore de arquivos e Command Palette (`Cmd+Shift+P`).
- **Filosofia de Design:** Teclado em primeiro lugar (*Keyboard-first workflow*), concorrência segura, baixa latência de áudio e arquitetura modular.

---

## 🚀 Fase 1: MVP (Minimum Viable Product) — *Focus: Keys & Core Notation*

O objetivo do MVP é entregar uma ferramenta extremamente ágil para escrita de piano, sintetizadores e lead sheets (clave de sol + cifras).

### 1.1 Modelo de Dados & Engine de Notação (Rust)
- [ ] Implementação da estrutura hierárquica `Score` $\rightarrow$ `Measure` $\rightarrow$ `Voice` $\rightarrow$ `ChordOrRest` $\rightarrow$ `Note`.
- [ ] Suporte a tipos de pauta:
  - **Grand Staff:** Pauta dupla de piano (Clave de Sol + Clave de Fá).
  - **Single Staff:** Pauta simples (Clave de Sol) para Lead Sheets ou arranjos de uma mão.
- [ ] Representação de notas (Pitch MIDI 0–127, acidentes $\sharp, \flat, \natural$, ligaduras e durações de $1/1$ a $1/32$).
- [ ] Camada de cifras ancoradas temporalmente aos compassos (`ChordSymbol`).
- [ ] Sistema de Polifonia: Suporte a até 4 vozes paralelas por compasso.

### 1.2 Layout & Renderização
- [ ] Algoritmo de espaçamento horizontal proporcional às durações das notas.
- [ ] Mapeamento e renderização de fontes musicais alinhadas ao padrão **SMuFL**.
- [ ] Distribuição dinâmica de compassos por sistema/página.

### 1.3 Audio & Sincronização MIDI
- [ ] Relógio temporal de alta precisão em thread dedicada (Rust).
- [ ] Leitor leve de SoundFont (`.sf2`/`.sf3`) para sons de piano e teclados.
- [ ] Visualizador de Teclado Interativo (88 teclas) no painel inferior, acendendo as notas em tempo real sincronizadas com o cursor de reprodução.

### 1.4 Interface & UX (VS Code Style)
- [ ] Estrutura básica da workspace:
  - **Explorer (Esquerda):** Navegação em árvore pelo projeto.
  - **Canvas Central:** Visualizador/Editor de partitura com suporte a abas.
  - **Mixer Rápido (Direita):** Volume, Pan, Mute e Solo por pauta.
  - **Painel Inferior:** Visualizador de Teclado / Piano Roll.
  - **Barra de Status:** Informações de tempo, tom, compasso e estado do engine.
- [ ] Gestão de estado e Undo/Redo (*Command Pattern*).

---

## ⚡ Fase 2: v1.1 — Produtividade & Workflow IDE

Expansão da usabilidade para tornar o processo de composição e arranjo rápido e totalmente customizável.

### 2.1 Teclado & Produtividade
- [ ] Sistema de atalhos totalmente configurável via `keybindings.json`.
- [ ] **Command Palette (`Cmd+Shift+P` / `Ctrl+Shift+P`):** Acesso rápido a todos os comandos do app por busca textual.
- [ ] Mapeamento numérico avançado para entrada rápida de notas e duração no teclado numérico.

### 2.2 Estrutura & Navegação Temporal
- [ ] Marcadores de estrutura na linha do tempo (*Intro, Intro Solo, A, B, Chorus, Outro*).
- [ ] Modos de visualização flexíveis:
  - **Page View:** Visualização tradicional para impressão.
  - **Continuous View:** Rolo contínuo horizontal para composição sem quebras de página.
  - **Split View:** Comparação de duas pautas ou versões lado a lado.

### 2.3 Ferramentas Avançadas de Transposição
- [ ] Transposição inteligente mantendo coerência harmônica (evitando enarmonias estranhas).
- [ ] Transposição automática de linhas de cifras simultaneamente com a partitura.

---

## 🔄 Fase 3: v1.2 — Interoperabilidade & Multi-Instrumentos

Abertura do ecossistema para integração com outros softwares do mercado e suporte a mais instrumentos.

### 3.1 Parsers de Formatos Legados
- [ ] Importação e Exportação nativa do formato MuseScore (`.mscz` / `.mscx`).
- [ ] Leitor de arquivos Guitar Pro (`.gp3`, `.gp4`, `.gp5`, `.gpx`).
- [ ] Suporte completo a **MusicXML** (Import/Export).
- [ ] Exportação MIDI e renderização de áudio (WAV/MP3).

### 3.2 Suporte Completo a Multi-Instrumentos
- [ ] Estrutura multi-track (`Score` $\rightarrow$ `Tracks`).
- [ ] Claves adicionais (Dó na 3ª/4ª linha, Clave de Percussão).
- [ ] Suporte a instrumentos transpositores (Sax, Trompete, Trompa) com alternância entre *Concert Pitch* e *Transposed Pitch*.
- [ ] Suporte a pautas de Tablatura (Violão, Guitarra, Baixo).

---

## 🧠 Fase 4: v2.0 — Recursos Avançados (Git de Partitura & IA)

Recursos inovadores que diferenciam o **klav** de qualquer outro editor de notação existente.

### 4.1 Controle de Versão Nativo (Git-like Workflow)
- [ ] Sistema de histórico visual estilo Git (Commits, Branches e Merges de arranjos).
- [ ] **Visual Diff:** Comparador gráfico entre duas versões de uma mesma partitura, destacando notas alteradas, adicionadas ou removidas em cores.
- [ ] Reversão instantânea para snapshots anteriores do projeto.

### 4.2 Assistente de IA & Automação por Linguagem Natural
- [ ] Prompt em linguagem natural para alterações no arranjo (ex: *"Rearmonize este trecho para Jazz"*, *"Crie uma segunda voz em terças para a mão direita"*).
- [ ] Análise harmônica automática com identificação de funções tonais e sugestão de cifras.

---

## 🏛️ Diretrizes Arquiteturais & Boas Práticas

1. **Separação Estrita de Responsabilidades:** O engine em Rust nunca depende da camada visual. Toda a lógica de partitura pode ser rodada em Headless (ex: CLI para conversão de arquivos).
2. **Performance Sem Lag:** O playback e a renderização gráfica operam de forma assíncrona para garantir 60 FPS na UI.
3. **Persistência Limpa:** Formato de projeto em JSON/Toml legível por humanos para facilitar a integração com sistemas de controle de versão tradicionais (Git).