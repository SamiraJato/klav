# klav — Plano de Arquitetura (v1)

> Modelo de dados e arquitetura do MVP do **klav**: app de partituras ultraleve e cross-platform
> (Rust + WebAssembly; desktop via Dioxus/webview, empacotamento Tauri planejado), focado em
> instrumentos de teclas.
> Incorpora as lições da análise do MuseScore — ver [analise-musescore.md](analise-musescore.md).
> Data: 2026-11-02.

> **Como ler este documento.** É o plano de arquitetura (alvo), não um retrato do código.
> O que já existe e o que é roadmap estão separados na seção **0. Estado de implementação**;
> ao longo do texto, 🔭 marca pontos ainda planejados.

---

## 0. Estado de implementação (2026-11-02)

**Já implementado:**

- `klav-core`: `Fraction` (racional reduzida, `i64`), IDs tipados, `Pitch::midi_number()`,
  o modelo das §§3.2–3.7 e `Score::validate()` cobrindo todos os invariantes da §3.8
  (unicidade de IDs, ties completos, tuplets, `is_rest`↔`notes`, fórmula de compasso válida).
- `klav-io`: formato nativo **`.klav`** (JSON versionado via `serde`), com validação no load.
- `klav-engine`: `render_events` (modelo → eventos MIDI, tratando ties e tuplets),
  `key_intervals` (pareamento por tecla, preservando instâncias simultâneas) e `tick → segundos`.
- `klav-layout`: geometria SVG da **pauta simples** (posições em *staff-spaces* + glifos SMuFL).
- `klav-app`: UI Dioxus com render **SVG**, explorer/abas, playback (cursor + teclado de 88 teclas
  + áudio por síntese senoidal), e edição de **altura** da nota selecionada (↑/↓).

**Ainda planejado (roadmap — não confundir com o estado atual):**

- 🔭 Empacotamento/distribuição com **Tauri** (Fatia 7). Hoje o desktop roda via webview do Dioxus.
- 🔭 Backend de render **Canvas/WebGL** para partituras grandes (o backend atual é SVG).
- 🔭 Import de **MusicXML** e **MIDI** em `klav-io` (hoje só o formato nativo `.klav`).
- 🔭 Canal de posição áudio↔UI da §4 (hoje o app usa um relógio por *polling* ~60 fps na própria UI).
- 🔭 Editor de conteúdo rico, undo/redo e mixer funcional (ver §7).

---

## 1. Princípios de arquitetura

Derivados da análise do MuseScore e das metas do klav:

1. **Separar DADOS de LAYOUT.** O modelo musical (`Score`/`Measure`/...) não conhece pixels. O layout
   (sistemas, páginas, posições em tela) é uma camada derivada e descartável, reconstruída a partir
   dos dados. (MuseScore: `Score` vs `System`/`Page`.)
2. **Tempo é racional, nunca float.** Toda duração e posição usa `Fraction` (i32/i32 reduzido). Elimina
   erro de arredondamento rítmico. (MuseScore usa `Fraction` para ticks.)
3. **Posição temporal explícita.** Como o klav não usa o modelo "segment indexado por track" do
   MuseScore, cada evento carrega sua posição (`start`) no compasso, tornando o alinhamento entre
   vozes verificável.
4. **Playback desacoplado da UI.** O motor de áudio publica tempo; a UI consome e interpola. Sem
   chamadas diretas áudio→render. (MuseScore: `Channel<secs_t>` + timer de interpolação.)
5. **IDs estáveis.** Todo elemento musical tem ID estável para edição incremental, undo/redo e
   referências cruzadas (ties, tuplets) sem ponteiros.

## 2. Camadas (crates)

```
klav-core        → modelo de dados musical puro (sem UI, sem áudio). #![no_std]-friendly quando der.
klav-io          → import/export: formato nativo .klav ✅  |  MusicXML, MIDI 🔭
klav-engine      → playback: modelo → eventos MIDI temporizados ✅  |  relógio/canal de posição 🔭 (§4)
klav-layout      → modelo → geometria (sistemas, pautas, posições) — camada derivada
klav-app         → UI Dioxus (webview) + render SVG ✅  |  empacotamento Tauri 🔭, backend Canvas/WebGL 🔭
```

O `klav-core` é o coração e o foco deste documento.

---

## 3. Modelo de dados revisado (`klav-core`)

### 3.1. Tipos fundamentais

```rust
/// Posição ou duração musical como fração de semibreve (whole note).
/// Ex.: semínima = 1/4, colcheia pontuada = 3/16. Sempre mantida reduzida, den > 0.
/// Campos privados: construída por Fraction::new / try_new (que normaliza e rejeita den == 0),
/// lida por num()/den(). Implementada com i64.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Fraction { num: i64, den: i64 }

/// ID estável e tipado. Newtype sobre u64 (ou String, se precisar serializar legível).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoteId(pub u64);
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ElementId(pub u64);   // ChordOrRest
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TupletId(pub u64);
```

### 3.2. Claves, pautas e fórmula de compasso

```rust
pub enum Clef { Treble, Bass }

pub enum StaffType {
    Single(Clef),   // pauta simples (ex.: lead sheet em clave de Sol)
    GrandStaff,     // pauta dupla Sol/Fá (piano)
}

pub struct TimeSignature { pub numerator: u8, pub denominator: u8 }

/// Armadura de clave: nº de sustenidos (+) ou bemóis (-). 0 = Dó maior.
pub type KeySignature = i8;
```

### 3.3. Altura (Pitch) — grafia é a fonte da verdade

```rust
pub enum Accidental { DoubleFlat, Flat, Natural, Sharp, DoubleSharp }

/// step/octave/accidental são a FONTE DA VERDADE.
/// midi_number() é DERIVADO (não armazenado), evitando dessincronização.
pub struct Pitch {
    pub step: char,                     // 'A'..='G'
    pub octave: i8,                     // convenção científica (C4 = Dó central)
    pub accidental: Option<Accidental>,
}

impl Pitch {
    /// MIDI 0–127 derivado da grafia. Nunca armazenado como campo.
    pub fn midi_number(&self) -> u8 { /* step+octave+accidental → midi */ }
}
```

> Decisão: adotamos a opção (a) da análise — `midi_number` derivado. Legível e sem tabela TPC.

### 3.4. Nota, com tie direcional

```rust
pub struct Note {
    pub id: NoteId,
    pub pitch: Pitch,
    pub velocity: u8,              // 0–127
    /// Ligadura de VALOR: aponta para a próxima nota ligada (mesma altura).
    /// None = sem tie. Substitui o antigo `is_tied: bool`.
    pub tie_to: Option<NoteId>,
}
```

> `tie_to` dá direção e destino. O `tie_from` pode ser reconstruído por índice quando necessário,
> então não duplicamos aqui. Slur (fraseado) NÃO entra na nota — ver §3.7 (fora do MVP inicial).

### 3.5. Duração e Chord/Rest com posição explícita

```rust
pub enum DurationBase { Whole, Half, Quarter, Eighth, Sixteenth, ThirtySecond /* ... */ }

pub struct Duration {
    pub base: DurationBase,
    pub dots: u8,                  // pontos de aumento
}
impl Duration {
    /// Duração efetiva como fração (já considerando os pontos; tuplet é aplicado fora).
    pub fn as_fraction(&self) -> Fraction { /* ... */ }
}

pub struct ChordOrRest {
    pub id: ElementId,
    /// Posição de início DENTRO do compasso (0 = início). Torna o alinhamento
    /// entre vozes explícito e verificável. (Lição-chave da análise.)
    pub start: Fraction,
    pub duration: Duration,
    pub is_rest: bool,
    pub notes: Vec<Note>,          // vazio se is_rest
    /// Pertence a uma quiáltera? (tercina etc.) Resolvido via Measure::tuplets.
    pub tuplet: Option<TupletId>,
}
```

### 3.6. Tuplets (quiálteras)

```rust
/// Ex.: tercina de colcheias = 3 no tempo de 2 → Tuplet { actual: 3, normal: 2, ... }
pub struct Tuplet {
    pub id: TupletId,
    pub actual: u8,                // quantas notas são tocadas
    pub normal: u8,                // no tempo de quantas
    pub base: DurationBase,        // unidade da quiáltera
    pub members: Vec<ElementId>,   // ChordOrRest que a compõem
}
```

### 3.7. Voz, Compasso e Score

```rust
pub struct Voice {
    pub voice_id: u8,              // 0..=3 (VOICES = 4, igual ao MuseScore)
    pub elements: Vec<ChordOrRest>,
}

pub struct ChordSymbol {
    pub start: Fraction,           // posição no compasso
    pub text: String,              // ex.: "Cm7", "G/B" — parsing fica em camada separada
}

pub struct Measure {
    pub index: u32,
    pub time_signature: Option<TimeSignature>,   // None = herda do compasso anterior
    pub key_signature: Option<KeySignature>,
    pub chord_symbols: Vec<ChordSymbol>,         // cifras / lead sheet
    pub voices: Vec<Voice>,                       // 1..=4
    pub tuplets: Vec<Tuplet>,                     // quiálteras deste compasso
}

pub const VOICES: u8 = 4;

pub struct Score {
    pub title: String,
    pub tempo_bpm: u16,
    pub staff_type: StaffType,
    pub measures: Vec<Measure>,
}
```

### 3.8. Invariantes (validados por `Score::validate()`)

- Em cada `Voice`, a soma de `duration` (com tuplets aplicados) ≤ duração do compasso, e os `start`
  são monotônicos e não se sobrepõem.
- Todo `start` de `ChordOrRest`/`ChordSymbol` cai dentro do compasso.
- `tie_to` referencia um `NoteId` existente de mesma altura, no futuro imediato da mesma voz.
- `voice_id` único por compasso e `< VOICES`.
- `NoteId` e `ElementId` únicos no documento; `TupletId` único por compasso.
- `is_rest` é coerente com `notes` (pausa não tem notas; acorde tem ≥1).
- Fórmula de compasso com numerador/denominador zero é rejeitada.
- Tuplets: `actual`/`normal` > 0, todo `ElementId` em `Tuplet::members` existe no compasso e a
  associação elemento↔tuplet é coerente nos dois sentidos.

> Todos os itens acima já são verificados por `Score::validate()`; cada um tem uma variante
> específica de `ValidationError` com contexto (compasso, id envolvido).

---

## 4. Playback (`klav-engine`) — modelo desacoplado

> **Estado:** `klav-engine` já produz os eventos MIDI e a conversão tick→segundos. O **canal de
> posição áudio↔UI** descrito abaixo ainda é 🔭 planejado: hoje o `klav-app` avança o cursor com um
> relógio por *polling* (~60 fps) na própria UI e dispara `NoteOn`/`NoteOff` ao sintetizador.

Replicando o padrão do MuseScore (áudio publica tempo, UI interpola):

```
klav-engine (relógio / síntese, idealmente em worker/thread separado)
      │  publica posição:  PlaybackPosition { tick: Fraction, secs: f64 }
      ▼
  canal assíncrono (ex.: channel / signal)   ← fronteira áudio↔UI
      │
      ▼
klav-app (UI):  armazena (última posição + timestamp)
      │  requestAnimationFrame / timer
      ▼
  cursor_tick = interpolar(última_posição, tempo_decorrido, bpm)
      ▼
  mover cursor + auto-scroll
```

Regras:
- O áudio **nunca** chama render diretamente.
- A UI **interpola** entre atualizações do áudio para o cursor andar suave (60fps mesmo que o áudio
  reporte a passos grandes).
- Conversão tick↔segundos considera `tempo_bpm` (e futuros `tempo changes`).

---

## 5. Roadmap de implementação (MVP)

1. **`klav-core` tipos base:** `Fraction` (+ aritmética e redução), IDs, `Pitch::midi_number()`.
2. **Modelo musical** (§3.2–3.7) + `Score::validate()` (§3.8).
3. **`klav-io`:** parser/serializer do formato nativo `.klav` (JSON/serde pra começar);
   import de MusicXML num segundo momento.
4. **`klav-layout`:** posicionar compassos/notas numa pauta (geometria, sem estética fina ainda).
5. **`klav-engine`:** modelo → eventos MIDI + relógio + canal de posição.
6. **`klav-app` (Tauri + WASM):** render em canvas, cursor de playback interpolado, navegação IDE.

## 6. Decisões tomadas (2026-11-02)

- **IDs:** `u64` (newtype), compacto e performático para WASM. Debug menos legível é aceitável.
- **`Fraction`:** implementação própria, zero dependências e compatível com `no_std`.
- **Tempo:** fixo por `Score` (`tempo_bpm`) no MVP. Mudanças de andamento ficam para depois — a struct
  já isola isso num único campo, então a evolução não quebra o modelo.
- **Formato nativo `.klav`:** JSON via `serde`. Legível, versionável no git; otimização binária depois.
- **Backend de render:** **SVG** no MVP. `klav-layout` gera geometria **agnóstica** (posições em staff-spaces + glifos SMuFL), então o backend é trocável — na Fase 3 (partituras grandes/Continuous View) pode-se adicionar um backend Canvas/WebGL sem mexer no motor de layout. SVG dá nitidez/impressão (Page View), hit-testing nativo (edição) e estilo por nota (Visual Diff da Fase 4) com menos esforço.
- **Fonte musical:** **Bravura** (SMuFL, Steinberg, licença SIL OFL — livre e redistribuível). O `klav-layout` fica agnóstico à fonte.
- **Frontend:** **Dioxus** (UI em Rust/WASM). Decisão validada por dois spikes (Dioxus desktop vs Svelte+WASM) renderizando a mesma cena do `klav-layout` em SVG: os resultados visuais ficaram equivalentes, e o Dioxus venceu por **menos overhead de execução** (um `cargo run` abre a janela, sem Node/Vite/wasm-pack no caminho), **tipos compartilhados com o core sem serialização** e coesão (tudo Rust). O backend desktop usa o webview nativo via Tauri/wry; o build web compila o mesmo código para WASM. SVG continua sendo o backend de desenho dentro do Dioxus.

---

## 7. Plano de fatias do klav-app (revisado em 2026-11-02)

Estado atual: Fatias 1, 2 (playback+áudio) e 3a (esqueleto de IDE) concluídas. A edição hoje
só **move a altura** de uma nota existente (↑/↓) — ainda **não há adição de conteúdo**.

**Decisão de reprioritização:** amadurecer o **editor** antes de undo/redo e mixer, porque
(a) sem adicionar notas/compassos o app ainda é quase um visualizador, e (b) undo/redo só tem o
que desfazer depois que a edição é rica. Entrada **por teclado primeiro** (mais simples que mouse:
não precisa de mapeamento inverso de coordenadas), mouse depois.

- **Fatia 4 — Editor de conteúdo (prioridade):**
  - 4a — Adicionar/remover: botão "+ nota" (acrescenta na próxima posição livre do compasso/voz
    ativa, altura padrão ajustável com ↑/↓ que já existe), "+ compasso", e **deletar** a nota
    selecionada. Mínimo viável reutilizando as primitivas atuais.
  - 4b — Entrada por teclado: tecla A–G define a altura, teclas numéricas definem a duração
    (1=semibreve … 5=semicolcheia), setas movem, Del apaga, acidentes (#/b) e pausas.
  - 4c — Entrada por mouse: clicar na pauta insere nota (y→altura via inverso de `note_y`,
    x→posição), com snap. (Depende de hit-testing em posições vazias.)
- **Fatia 5 — Undo/Redo (Command Pattern):** agora com operações ricas pra desfazer; base do
  "Git de partitura" (Fase 4 do roadmap de produto).
- **Fatia 6 — Multi-pauta + Mixer funcional:** Grand Staff completo (2 pautas) e mixer por pauta.
- **Fatia 7 — Tauri:** empacotamento/distribuição.
- **Fatia 2c (quando quiser):** SoundFont para som de piano real no lugar da síntese senoidal.
