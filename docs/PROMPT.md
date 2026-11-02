# CONTEXTO DO PROJETO
Estamos projetando o **klav**, um novo aplicativo de partituras e arranjos moderno, ultraleve e cross-platform (Rust + WebAssembly + Tauri), focado em performance, navegação estilo IDE (VS Code) e workflow fluido.

O MVP do **klav** é focado em instrumentos de teclas (Piano, Teclado, Sintetizador), suportando tanto Pauta Dupla (Sol/Fá) quanto Pauta Simples (Clave de Sol) + linha de Cifras (Lead Sheets).

---

# REPOSITÓRIO ALVO DA ANÁLISE
- **Repositório GitHub:** `https://github.com/musescore/MuseScore`
- **Caminho Local:** `./` (ou indique o caminho relativo/absoluto para o clone do MuseScore)

---

# INSTRUÇÕES DE ANÁLISE COM O PLUGIN REA (Reverse Engineer Anything)

Use o plugin REA para inspecionar estruturalmente o repositório C++ indicado acima antes de sintetizar as respostas:

1. **Mapeamento da Estrutura de Dados:**
   - Analise as classes/interfaces do C++ focando em: `Score`, `Staff`, `Measure`, `Segment`, `Chord`, `Note` e `ChordSymbol`.
   - Mapeie a hierarquia de memória e como gerencia vozes paralelas (1 a 4).

2. **Rastreamento do Fluxo de Playback:**
   - Localize o ponto de entrada do motor de playback de áudio/MIDI.
   - Rastreie a cadeia de chamadas (*call graph*) a partir do tick do temporizador até o evento de atualização do cursor na UI.

3. **Validação do Modelo Rust de Referência (klav):**
   - Compare o modelo C++ do MuseScore com a nossa struct Rust planejada para o MVP do **klav**:

```rust
pub enum Clef { Treble, Bass }
pub enum StaffType { Single(Clef), GrandStaff }

pub struct Pitch { pub midi_number: u8, pub step: char, pub octave: i8, pub accidental: Option<Accidental> }
pub struct Note { pub id: String, pub pitch: Pitch, pub is_tied: bool, pub velocity: u8 }
pub struct ChordOrRest { pub id: String, pub duration: Duration, pub dots: u8, pub is_rest: bool, pub notes: Vec<Note> }
pub struct Voice { pub voice_id: u8, pub elements: Vec<ChordOrRest> }
pub struct Measure { pub index: u32, pub time_signature: Option<TimeSignature>, pub key_signature: Option<i8>, pub chord_symbols: Vec<ChordSymbol>, pub voices: Vec<Voice> }
pub struct Score { pub title: String, pub tempo_bpm: u16, pub staff_type: StaffType, pub measures: Vec<Measure> }
```