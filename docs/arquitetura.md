# klav — Plano de Arquitetura (v1)

> Modelo de dados e arquitetura do MVP do **klav**: app de partituras ultraleve e cross-platform
> (Rust + WebAssembly + Tauri), focado em instrumentos de teclas.
> Incorpora as lições da análise do MuseScore — ver [analise-musescore.md](analise-musescore.md).
> Data: 2026-11-02.

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
klav-io          → import/export (MusicXML, MIDI, formato nativo .klav)
klav-engine       → playback: modelo → eventos MIDI temporizados + relógio
klav-layout       → modelo → geometria (sistemas, pautas, posições) — camada derivada
klav-app (Tauri)  → shell desktop + bridge WASM + render (canvas/webgl)
```

O `klav-core` é o coração e o foco deste documento.

---

## 3. Modelo de dados revisado (`klav-core`)

### 3.1. Tipos fundamentais

```rust
/// Posição ou duração musical como fração de semibreve (whole note).
/// Ex.: semínima = 1/4, colcheia pontuada = 3/16. Sempre mantida reduzida.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Fraction { pub num: i32, pub den: i32 }

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

pub const VOICES: usize = 4;

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
- Todo `ElementId` em `Tuplet::members` existe no compasso.

---

## 4. Playback (`klav-engine`) — modelo desacoplado

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
