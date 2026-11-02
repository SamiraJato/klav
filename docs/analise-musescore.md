# Análise do MuseScore — Referência de Arquitetura para o klav

> Documento de referência do projeto **klav**.
> Baseado na análise do código-fonte C++ do MuseScore Studio (branch `master`, commit `f4194eb421`),
> clonado em `../MuseScore` (pasta irmã deste repositório).
> Data da análise: 2026-11-02.

O código do modelo de dados fica em `src/engraving/dom/`. Playback fica em `src/playback/`,
o áudio no framework `muse` (`src/framework/audio`) e o cursor da UI em `src/notationscene/`.

---

## 1. Mapeamento da Estrutura de Dados

### 1.1. Hierarquia geral

```
Score
 ├── m_measures : MeasureBaseList          // lista encadeada de MeasureBase (Measure | Box | ...)
 ├── m_staves   : std::vector<Staff*>       // pautas do documento (nível Score, não Measure)
 ├── m_systems  : std::vector<System*>      // layout: compassos agrupados em linhas
 └── m_pages    : std::vector<Page*>

Measure (final : MeasureBase)
 ├── m_segments : SegmentList               // lista encadeada de Segment, ordenada por tick (rtick)
 └── m_mstaves  : std::vector<MStaff*>      // dados por-pauta DENTRO do compasso (ex.: m_hasVoices)

Segment (tem um SegmentType: ChordRest, Clef, TimeSig, KeySig, BarLine, ...)
 └── m_elist    : std::vector<EngravingItem*>   // INDEXADO POR TRACK (não por voz!)

Chord / Rest (final : ChordRest : DurationElement : EngravingItem)
 └── Chord::m_notes : std::vector<Note*>

Note (final : EngravingItem)
 ├── m_pitch : int           // MIDI 0–127
 ├── m_tpc[2]: int           // tonal pitch class (concert / transposto)
 ├── m_tieFor / m_tieBack : Tie*
 └── m_userVelocity : int
```

Referências:
- `Score::m_measures` — [score.h:955](../../MuseScore/src/engraving/dom/score.h#L955)
- `Segment::m_elist` — [segment.h:178](../../MuseScore/src/engraving/dom/segment.h#L178)
- `Chord::m_notes` — [chord.h:179](../../MuseScore/src/engraving/dom/chord.h#L179)
- `Note` membros — [note.h:537-548](../../MuseScore/src/engraving/dom/note.h#L537-L548)

### 1.2. O modelo de vozes — PONTO CRÍTICO

**O MuseScore NÃO tem uma classe `Voice`.** A voz não é um container que "possui" elementos.
Em vez disso, cada `Segment` guarda um vetor plano `m_elist` indexado por **track**, e a voz é
derivada aritmeticamente do índice do track:

```cpp
static constexpr size_t VOICES = 4;                 // mscore.h:30 — máximo de 4 vozes por pauta

track = staffIdx * VOICES + voiceIdx                // staff2track()
staffIdx = track / VOICES                           // track2staff()
voiceIdx = track % VOICES                           // track2voice()
```

Referência: [mscore.h:30-50](../../MuseScore/src/engraving/dom/mscore.h#L30-L50).

Consequências do modelo "track-centric":
- Um `ChordRest` numa voz é acessado por `segment->element(track)`. Posições vazias de uma voz
  num dado instante são simplesmente `nullptr` naquele slot do `m_elist`.
- O alinhamento rítmico entre vozes é **implícito**: tudo que compartilha o mesmo `Segment`
  ocorre no mesmo tick. É isso que mantém as vozes sincronizadas no tempo.
- `MStaff::m_hasVoices` sinaliza se um compasso usa mais de uma voz (otimização de layout).

### 1.3. Duração, pontos e quiálteras (tuplets)

- `ChordRest` herda de **`DurationElement`**, que carrega `m_tuplet : Tuplet*`
  ([durationelement.h:77](../../MuseScore/src/engraving/dom/durationelement.h#L77)).
- A duração fica em `m_durationType : TDuration`; **os pontos de aumento são parte do `TDuration`**
  (`durationType().dots()`), não um campo separado — ver [chordrest.h:99-116](../../MuseScore/src/engraving/dom/chordrest.h#L99-L116).
- `Tuplet` é ele próprio um `DurationElement` ([tuplet.h:45](../../MuseScore/src/engraving/dom/tuplet.h#L45))
  que agrupa os `DurationElement`s cujo `tuplet()` aponta pra ele. Tuplets podem ser aninhados
  (`topTuplet()`).

### 1.4. Pitch e alterações

- Pitch MIDI em `m_pitch` (0–127). A grafia (nome da nota + alteração) vem do **TPC — Tonal Pitch
  Class** (`m_tpc[2]`), que codifica nota + acidente num único inteiro, separando versão de concerto
  da transposta. O MuseScore **não** guarda `step`/`octave`/`accidental` como campos separados — isso
  é derivado do par (pitch, tpc).

### 1.5. ChordSymbol / Cifras (Lead Sheets)

- O equivalente ao seu `ChordSymbol` é a classe **`Harmony`**, que herda de `TextBase`
  ([harmony.h:212](../../MuseScore/src/engraving/dom/harmony.h#L212)) — ou seja, é tratada como
  texto rico com parsing harmônico, não como uma struct de dados pura.
- Cifras ficam como **anotações de segmento** (`Segment::m_annotations`), não dentro do `ChordRest`.

---

## 2. Fluxo de Playback (tick de áudio → cursor na UI)

O áudio roda num thread próprio do framework `muse`; a sincronização com a UI é feita por
**canais assíncronos** (`muse::async::Channel`) e por um **timer de interpolação** na view.

```
[thread de áudio / player do framework muse]
        │  emite posição em segundos (secs_t)
        ▼
IPlayer::playbackPositionChanged()  ──► Channel<secs_t>
        │
        ▼
PlaybackController::setupPlayer()                         playbackcontroller.cpp:1749
   onReceive([](secs_t pos){
        m_playbackPositionChanged.send(pos);              // repassa p/ a UI
        m_currentTick = notationPlayback()->secToTick(pos); // converte segundos → tick MIDI
        updateCurrentTempo();
        if (pos ~>= endSecs) doStop();
   })
        │  Channel<secs_t> PlaybackController::playbackPositionChanged()
        ▼
AbstractNotationPaintView::onNotationSetup()              abstractnotationpaintview.cpp:152
   playbackState()->playbackPositionChanged().onReceive([](secs_t secs){
        m_lastPlaybackPosition = secs;                    // só ARMAZENA + timestamp
        m_lastPlaybackPositionUpdateTimeNs = elapsed;
   })
        │
        │  QTimer periódico (m_updatePlaybackCursorInterpolatedTimer)   linha 77
        ▼
updatePlaybackCursorInterpolated()
        │  interpola o tick a partir de (última posição + tempo decorrido)
        ▼
AbstractNotationPaintView::movePlaybackCursor(tick)       linha 1610
   m_playbackCursor->move(tick)   → calcula o RectF do cursor
   onPlaybackCursorRectChanged()  → agenda redraw
   adjustCanvasPosition(...)      → auto-scroll / pan seguindo o cursor
```

Pontos de entrada:
- Controlador: [playbackcontroller.cpp:1749](../../MuseScore/src/playback/internal/playbackcontroller.cpp#L1749)
- Cursor na view: [abstractnotationpaintview.cpp:1610](../../MuseScore/src/notationscene/qml/MuseScore/NotationScene/abstractnotationpaintview.cpp#L1610)
- `PlaybackCursor`: `src/notationscene/qml/MuseScore/NotationScene/playbackcursor.cpp`

**Lição de arquitetura para o klav:** o áudio nunca mexe direto na UI. Ele publica `secs_t` num
canal; a UI converte pra tick e **interpola** entre atualizações com um timer, pra o cursor andar
suave mesmo que o áudio reporte posição em passos grandes. Esse desacoplamento (áudio empurra tempo,
UI puxa/interpola) é um bom modelo pra replicar em Rust/WASM.

---

## 3. Validação do modelo Rust do klav

Comparando sua struct planejada com o modelo real do MuseScore:

| Área | Seu modelo klav | MuseScore | Veredito |
|------|-----------------|-----------|----------|
| Score → Measures | `Vec<Measure>` | `MeasureBaseList` (encadeada) + `System`/`Page` separados pro layout | ✅ OK pro MVP. Separe DADOS de LAYOUT desde já. |
| Vozes | `Measure.voices: Vec<Voice>`, cada `Voice` possui `Vec<ChordOrRest>` | sem classe `Voice`; `Segment.m_elist` indexado por track; voz = `track % 4` | ⚠️ Modelo diferente. Veja §3.1. |
| Pitch | `step`, `octave`, `accidental`, `midi_number` explícitos | `m_pitch` (MIDI) + `m_tpc` (TPC codificado) | ✅ Seu modelo é mais explícito/legível. Veja §3.2. |
| Pontos | `dots: u8` separado de `duration` | `dots` embutido no `TDuration` | ✅ OK, só uma escolha de design. |
| Tie | `Note.is_tied: bool` | `Tie* m_tieFor` + `Tie* m_tieBack` (ponteiros direcionais) | ⚠️ `bool` perde direção/destino. Veja §3.3. |
| Cifras | `Measure.chord_symbols: Vec<ChordSymbol>` | `Harmony : TextBase` em `Segment::m_annotations` | ✅ OK pro MVP (lead sheet). |
| StaffType | `Single(Clef)` / `GrandStaff` | pautas no nível do `Score` + `MStaff` por compasso | ✅ Suficiente pro foco em teclas. |

### 3.1. Vozes — a maior divergência

Seu modelo (voz *possui* uma lista de elementos) é **mais simples e mais limpo** que o do MuseScore,
e provavelmente melhor para o MVP focado em teclado. Mas atenção a uma armadilha:

- No modelo "voz possui lista", o **alinhamento temporal entre vozes é implícito** — você precisa
  somar as durações pra saber qual nota da voz 2 toca junto com qual nota da voz 1. O MuseScore
  resolve isso de graça porque tudo no mesmo `Segment` está no mesmo tick.
- **Recomendação:** adicione um campo de posição temporal (ex.: `start_tick: u32` ou
  `start: Fraction`) em `ChordOrRest`, ou garanta no código que as vozes sempre somam a duração
  do compasso. Sem isso, renderizar pauta dupla com 2 vozes (ex.: SATB no piano) fica frágil.
- Mantenha `VOICES = 4` como limite (igual ao MuseScore) — é o padrão da notação ocidental.

### 3.2. Pitch — você está mais explícito (bom), mas cuidado com redundância

Ter `midi_number` **e** `step`/`octave`/`accidental` ao mesmo tempo cria risco de os dois ficarem
dessincronizados (ex.: Dó# = Réb têm o mesmo MIDI 61 mas grafias diferentes). O MuseScore evita isso
usando o par (pitch, TPC). Opções:
- **(a)** Tratar `midi_number` como derivado (uma função de `step`+`octave`+`accidental`), não como
  campo armazenado independente; ou
- **(b)** Adotar algo como o TPC. Recomendo a opção **(a)** pro MVP — é mais legível e evita a tabela
  TPC. Documente que `step/octave/accidental` são a fonte da verdade e `midi_number` é cache.

### 3.3. Tie (ligadura de valor) vs. Slur (ligadura de expressão)

- `is_tied: bool` no `Note` é insuficiente: uma tie **liga duas notas específicas** e tem direção
  (nota de origem → nota de destino). Com um `bool` você não sabe pra onde a ligadura vai.
- Sugestão mínima: `tie_to: Option<NoteId>` (ou `tie_forward`/`tie_backward`) em vez de `bool`.
- **Não confunda com slur:** tie liga duas notas de MESMA altura somando a duração; slur é fraseado
  entre notas diferentes e pertence ao nível do compasso/voz, não da nota. Seu MVP provavelmente
  precisa dos dois, mas comece só com tie.

### 3.4. Lacunas a considerar (fora do MVP, mas anote)

- **Tuplets/quiálteras:** nenhum campo no seu modelo. Tercinas são comuns até em lead sheets simples.
  Mínimo: um `Option<TupletId>` em `ChordOrRest` + uma tabela de tuplets no `Measure`.
- **Grace notes (apojaturas):** o MuseScore tem `GraceNotesGroup` dentro do `Chord`.
- **Fraction para tempo:** o MuseScore usa `Fraction` (não float) para ticks/posições, evitando erro
  de arredondamento rítmico. Vale adotar um tipo racional em Rust.

---

## Resumo executivo das recomendações

1. **Separe dados de layout** desde o início (como `Score` vs `System`/`Page` no MuseScore).
2. **Adicione posição temporal explícita** (`start_tick`/`Fraction`) aos elementos, já que seu modelo
   de vozes não garante alinhamento implícito.
3. **Troque `is_tied: bool` por `Option<NoteId>`** direcional.
4. **Trate `midi_number` como derivado** de step/octave/accidental.
5. **Use tipo racional (Fraction)** para durações/posições, não float.
6. **Reserve espaço para tuplets** (nem que seja só um ID opcional) antes que fique caro mudar.
