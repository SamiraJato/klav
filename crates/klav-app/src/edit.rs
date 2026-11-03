//! Edição da partitura: transposição diatônica da nota selecionada.
//!
//! A lógica musical é pura (testável sem UI); só `transpose_active` toca nos `Signal`.

use std::collections::HashMap;

use dioxus::prelude::*;

use klav_core::{
    Accidental, ChordOrRest, Duration, DurationBase, ElementId, Fraction, Measure, Note, NoteId,
    Pitch, Score, TimeSignature, Voice,
};

use crate::document::Tab;

/// Altura padrão de uma nota recém-adicionada (ajustável com ↑/↓).
const DEFAULT_STEP: char = 'C';
const DEFAULT_OCTAVE: i8 = 4;
/// Duração padrão de uma nota/compasso novo.
const DEFAULT_TS: TimeSignature = TimeSignature {
    numerator: 4,
    denominator: 4,
};

/// Transpõe (um passo diatônico) as notas do elemento selecionado da aba ativa.
pub fn transpose_active(mut tabs: Signal<Vec<Tab>>, active: Signal<usize>, up: bool) {
    let a = *active.peek();
    let mut w = tabs.write();
    if let Some(tab) = w.get_mut(a) {
        if let Some(id) = tab.selected {
            transpose_element(&mut tab.score, id, up);
        }
    }
}

/// Encontra o `ChordOrRest` de id `elem_id` para edição.
fn element_mut(score: &mut Score, elem_id: u64) -> Option<&mut ChordOrRest> {
    score
        .measures
        .iter_mut()
        .flat_map(|m| m.voices.iter_mut())
        .flat_map(|v| v.elements.iter_mut())
        .find(|cr| cr.id.value() == elem_id)
}

/// Transpõe diatonicamente todas as notas do elemento `elem_id` (pausa: nada a fazer).
pub fn transpose_element(score: &mut Score, elem_id: u64, up: bool) {
    if let Some(cr) = element_mut(score, elem_id) {
        for n in cr.notes.iter_mut() {
            n.pitch = step_pitch(n.pitch, up);
        }
    }
}

/// Sobe/desce uma nota um passo diatônico (C→D→…→B→C+8ª), preservando o acidente.
pub fn step_pitch(p: Pitch, up: bool) -> Pitch {
    const ORDER: [char; 7] = ['C', 'D', 'E', 'F', 'G', 'A', 'B'];
    let Some(idx) = ORDER.iter().position(|&c| c == p.step.to_ascii_uppercase()) else {
        return p;
    };
    let (new_idx, new_oct) = if up {
        if idx == 6 {
            (0, p.octave + 1)
        } else {
            (idx + 1, p.octave)
        }
    } else if idx == 0 {
        (6, p.octave - 1)
    } else {
        (idx - 1, p.octave)
    };
    Pitch::new(ORDER[new_idx], new_oct, p.accidental)
}

// ---------------------------------------------------------------------------
// Edição estrutural (Fatia 4a): adicionar nota/compasso, apagar nota.
// Funções puras sobre o Score; os invariantes são garantidos por quem chama,
// que valida o Score antes de efetivar a edição (ver convenção §8.2 da arquitetura).
// ---------------------------------------------------------------------------

/// Próximo `NoteId` livre no documento (maior id existente + 1).
pub fn next_note_id(score: &Score) -> u64 {
    let mut max = 0u64;
    for m in &score.measures {
        for v in &m.voices {
            for cr in &v.elements {
                for n in &cr.notes {
                    max = max.max(n.id.value());
                }
            }
        }
    }
    max + 1
}

/// Próximo `ElementId` livre no documento (maior id existente + 1).
pub fn next_element_id(score: &Score) -> u64 {
    let mut max = 0u64;
    for m in &score.measures {
        for v in &m.voices {
            for cr in &v.elements {
                max = max.max(cr.id.value());
            }
        }
    }
    max + 1
}

/// Duração do compasso na posição `mi` do `Vec`, herdando a fórmula anterior (4/4 por padrão).
fn measure_dur_at(score: &Score, mi: usize) -> Fraction {
    let mut dur = DEFAULT_TS.measure_duration();
    for m in score.measures.iter().take(mi + 1) {
        if let Some(ts) = m.time_signature {
            dur = ts.measure_duration();
        }
    }
    dur
}

/// Garante a existência da voz `voice_id` no compasso e devolve seu índice no `Vec`.
fn ensure_voice(measure: &mut Measure, voice_id: u8) -> usize {
    if let Some(i) = measure.voices.iter().position(|v| v.voice_id == voice_id) {
        return i;
    }
    measure.voices.push(Voice {
        voice_id,
        elements: vec![],
    });
    measure.voices.len() - 1
}

/// Acrescenta um compasso vazio ao final e devolve seu `index`.
///
/// Herda a fórmula do compasso anterior (`time_signature: None`); se for o primeiro
/// compasso do documento, assume 4/4 (senão o Score ficaria sem fórmula inicial).
pub fn add_measure(score: &mut Score) -> u32 {
    let index = score.measures.last().map(|m| m.index + 1).unwrap_or(0);
    let time_signature = if score.measures.is_empty() {
        Some(DEFAULT_TS)
    } else {
        None
    };
    score.measures.push(Measure {
        index,
        time_signature,
        key_signature: None,
        chord_symbols: vec![],
        voices: vec![],
        tuplets: vec![],
    });
    index
}

/// Acha a próxima posição livre para um elemento de duração `frac` na voz 0, começando a
/// procurar no compasso `start_mi` e avançando (criando compassos no fim quando precisa).
/// Devolve `(mi, vi, start)`, ou `None` se nem num compasso vazio o elemento couber.
fn free_slot_from(
    score: &mut Score,
    start_mi: usize,
    frac: Fraction,
) -> Option<(usize, usize, Fraction)> {
    if score.measures.is_empty() {
        add_measure(score);
    }
    let mut mi = start_mi.min(score.measures.len() - 1);
    loop {
        let measure_dur = measure_dur_at(score, mi);
        let vi = ensure_voice(&mut score.measures[mi], 0);
        let next_start = score.measures[mi].voices[vi]
            .elements
            .last()
            .map(|e| e.start + e.duration.as_fraction())
            .unwrap_or(Fraction::ZERO);

        if next_start + frac <= measure_dur {
            return Some((mi, vi, next_start));
        }
        // Não coube. Num compasso já vazio significa que a duração não cabe em lugar nenhum.
        if next_start == Fraction::ZERO {
            return None;
        }
        // Senão, transborda para o próximo compasso (criando-o se for o último).
        if mi + 1 >= score.measures.len() {
            add_measure(score);
        }
        mi += 1;
    }
}

/// Posição livre a partir do último compasso (apêndice no fim do documento).
fn free_slot(score: &mut Score, frac: Fraction) -> Option<(usize, usize, Fraction)> {
    let last = score.measures.len().saturating_sub(1);
    free_slot_from(score, last, frac)
}

/// Como [`add_note_with`], mas começa a procurar a posição livre no compasso `start_mi`
/// (o compasso onde a edição está), transbordando para os seguintes quando ele enche.
pub fn add_note_with_at(score: &mut Score, start_mi: usize, base: DurationBase) -> Option<u64> {
    let duration = Duration::new(base, 0);
    let (mi, vi, start) = free_slot_from(score, start_mi, duration.as_fraction())?;
    let nid = next_note_id(score);
    let eid = next_element_id(score);
    score.measures[mi].voices[vi].elements.push(ChordOrRest {
        id: ElementId(eid),
        start,
        duration,
        is_rest: false,
        notes: vec![Note {
            id: NoteId(nid),
            pitch: Pitch::new(DEFAULT_STEP, DEFAULT_OCTAVE, None),
            velocity: 80,
            tie_to: None,
        }],
        tuplet: None,
    });
    Some(eid)
}

/// Adiciona uma pausa de duração `base` na próxima posição livre. Devolve o `ElementId`.
pub fn add_rest_with(score: &mut Score, base: DurationBase) -> Option<u64> {
    let duration = Duration::new(base, 0);
    let (mi, vi, start) = free_slot(score, duration.as_fraction())?;
    let eid = next_element_id(score);
    score.measures[mi].voices[vi].elements.push(ChordOrRest {
        id: ElementId(eid),
        start,
        duration,
        is_rest: true,
        notes: vec![],
        tuplet: None,
    });
    Some(eid)
}

/// Zera qualquer `tie_to` que aponte para uma nota em `gone`.
fn clear_ties_to(score: &mut Score, gone: &[u64]) {
    for m in score.measures.iter_mut() {
        for v in m.voices.iter_mut() {
            for cr in v.elements.iter_mut() {
                for n in cr.notes.iter_mut() {
                    if n.tie_to
                        .map(|t| t.value())
                        .is_some_and(|t| gone.contains(&t))
                    {
                        n.tie_to = None;
                    }
                }
            }
        }
    }
}

/// Posição `(mi, vi, ei)` do elemento de id `elem_id`.
fn locate_element(score: &Score, elem_id: u64) -> Option<(usize, usize, usize)> {
    for (mi, m) in score.measures.iter().enumerate() {
        for (vi, v) in m.voices.iter().enumerate() {
            for (ei, cr) in v.elements.iter().enumerate() {
                if cr.id.value() == elem_id {
                    return Some((mi, vi, ei));
                }
            }
        }
    }
    None
}

/// Remove o elemento `elem_id` (nota/acorde ou pausa): zera ties que apontavam para suas
/// notas e o tira de membros de quiálteras. Devolve `true` se achou.
pub fn delete_element(score: &mut Score, elem_id: u64) -> bool {
    let Some((mi, vi, ei)) = locate_element(score, elem_id) else {
        return false;
    };
    let gone: Vec<u64> = score.measures[mi].voices[vi].elements[ei]
        .notes
        .iter()
        .map(|n| n.id.value())
        .collect();
    clear_ties_to(score, &gone);
    score.measures[mi].voices[vi].elements.remove(ei);
    for t in score.measures[mi].tuplets.iter_mut() {
        t.members.retain(|e| e.value() != elem_id);
    }
    true
}

/// Transforma o elemento `elem_id` em pausa: remove as notas (mantendo duração e posição) e
/// zera ties que apontavam para elas. Devolve `true` se achou.
pub fn convert_to_rest(score: &mut Score, elem_id: u64) -> bool {
    let Some((mi, vi, ei)) = locate_element(score, elem_id) else {
        return false;
    };
    let gone: Vec<u64> = score.measures[mi].voices[vi].elements[ei]
        .notes
        .iter()
        .map(|n| n.id.value())
        .collect();
    clear_ties_to(score, &gone);
    let cr = &mut score.measures[mi].voices[vi].elements[ei];
    cr.notes.clear();
    cr.is_rest = true;
    true
}

// --- Wrappers que tocam os Signal da UI, validando antes de efetivar. ---

/// Adiciona uma nota de duração `base` à aba ativa (seleciona-a) — a "escrita" por teclado.
/// Escreve no compasso do elemento selecionado (transbordando para os seguintes quando enche);
/// sem seleção, acrescenta no fim do documento.
pub fn add_note_with_active(
    mut tabs: Signal<Vec<Tab>>,
    active: Signal<usize>,
    mut status: Signal<String>,
    base: DurationBase,
) {
    let a = *active.peek();
    let Some((mut score, sel)) = tabs.read().get(a).map(|t| (t.score.clone(), t.selected)) else {
        return;
    };
    let start_mi = sel
        .and_then(|id| locate_element(&score, id))
        .map(|(mi, _, _)| mi)
        .unwrap_or_else(|| score.measures.len().saturating_sub(1));
    match add_note_with_at(&mut score, start_mi, base) {
        Some(nid) => match score.validate() {
            Ok(()) => {
                if let Some(t) = tabs.write().get_mut(a) {
                    t.score = score;
                    t.selected = Some(nid);
                }
                status.set(format!("nota {nid} adicionada"));
            }
            Err(e) => status.set(format!("edição inválida: {e:?}")),
        },
        None => status.set("sem espaço no compasso — use “+ compasso”".into()),
    }
}

/// Adiciona uma pausa de duração `base` à aba ativa (não altera a seleção).
pub fn add_rest_with_active(
    mut tabs: Signal<Vec<Tab>>,
    active: Signal<usize>,
    mut status: Signal<String>,
    base: DurationBase,
) {
    let a = *active.peek();
    let Some(mut score) = tabs.read().get(a).map(|t| t.score.clone()) else {
        return;
    };
    match add_rest_with(&mut score, base) {
        Some(_) => match score.validate() {
            Ok(()) => {
                if let Some(t) = tabs.write().get_mut(a) {
                    t.score = score;
                }
                status.set("pausa adicionada".into());
            }
            Err(e) => status.set(format!("edição inválida: {e:?}")),
        },
        None => status.set("sem espaço no compasso — use “+ compasso”".into()),
    }
}

/// Com elemento selecionado: transforma-o em pausa. Sem seleção: adiciona uma pausa nova com
/// a duração ativa. Valida antes de efetivar.
pub fn rest_or_convert_active(
    mut tabs: Signal<Vec<Tab>>,
    active: Signal<usize>,
    mut status: Signal<String>,
    current: Signal<DurationBase>,
) {
    let a = *active.peek();
    let sel = tabs.peek().get(a).and_then(|t| t.selected);
    let Some(id) = sel else {
        add_rest_with_active(tabs, active, status, *current.peek());
        return;
    };
    let Some(mut score) = tabs.read().get(a).map(|t| t.score.clone()) else {
        return;
    };
    if !convert_to_rest(&mut score, id) {
        return;
    }
    match score.validate() {
        Ok(()) => {
            if let Some(t) = tabs.write().get_mut(a) {
                t.score = score;
            }
            status.set("virou pausa".into());
        }
        Err(e) => status.set(format!("edição inválida: {e:?}")),
    }
}

/// Define a duração ativa (para novas notas) e, se houver nota selecionada, aplica-a a ela
/// também — sem reclamar quando não há seleção (só trocar a duração ativa é válido).
pub fn pick_duration(
    mut current: Signal<DurationBase>,
    tabs: Signal<Vec<Tab>>,
    active: Signal<usize>,
    status: Signal<String>,
    base: DurationBase,
) {
    current.set(base);
    let a = *active.peek();
    let has_sel = tabs.peek().get(a).and_then(|t| t.selected).is_some();
    if has_sel {
        set_duration_active(tabs, active, status, base);
    }
}

/// Acrescenta um compasso vazio à aba ativa.
pub fn add_measure_active(
    mut tabs: Signal<Vec<Tab>>,
    active: Signal<usize>,
    mut status: Signal<String>,
) {
    let a = *active.peek();
    let Some(mut score) = tabs.read().get(a).map(|t| t.score.clone()) else {
        return;
    };
    let idx = add_measure(&mut score);
    match score.validate() {
        Ok(()) => {
            if let Some(t) = tabs.write().get_mut(a) {
                t.score = score;
            }
            status.set(format!("compasso {idx} adicionado"));
        }
        Err(e) => status.set(format!("edição inválida: {e:?}")),
    }
}

/// Apaga o elemento selecionado (nota/acorde ou pausa) da aba ativa.
pub fn delete_selected_active(
    mut tabs: Signal<Vec<Tab>>,
    active: Signal<usize>,
    mut status: Signal<String>,
) {
    let a = *active.peek();
    let Some((mut score, sel)) = tabs.read().get(a).map(|t| (t.score.clone(), t.selected)) else {
        return;
    };
    let Some(id) = sel else {
        status.set("selecione algo para apagar".into());
        return;
    };
    if !delete_element(&mut score, id) {
        status.set("elemento não encontrado".into());
        return;
    }
    match score.validate() {
        Ok(()) => {
            if let Some(t) = tabs.write().get_mut(a) {
                t.score = score;
                t.selected = None;
            }
            status.set("apagado".into());
        }
        Err(e) => status.set(format!("edição inválida: {e:?}")),
    }
}

// ---------------------------------------------------------------------------
// Entrada por teclado (Fatia 4b): altura (A–G), acidente (#/b), duração (1–5)
// e navegação entre notas. Funções puras + wrappers que validam antes de efetivar.
// ---------------------------------------------------------------------------

/// Define o nome (A–G) do elemento `elem_id`, preservando a oitava e zerando o acidente de
/// cada nota. Se o elemento for uma pausa, vira uma nota dessa altura (C4 por padrão).
pub fn set_step(score: &mut Score, elem_id: u64, step: char) -> bool {
    let step = step.to_ascii_uppercase();
    let new_note_id = next_note_id(score);
    let Some(cr) = element_mut(score, elem_id) else {
        return false;
    };
    if cr.notes.is_empty() {
        // pausa -> nota
        cr.is_rest = false;
        cr.notes.push(Note {
            id: NoteId(new_note_id),
            pitch: Pitch::new(step, DEFAULT_OCTAVE, None),
            velocity: 80,
            tie_to: None,
        });
    } else {
        for n in cr.notes.iter_mut() {
            n.pitch = Pitch::new(step, n.pitch.octave, None);
        }
    }
    true
}

/// Define (ou remove, com `None`) o acidente de cada nota do elemento (pausa: nada a fazer).
pub fn set_accidental(score: &mut Score, elem_id: u64, acc: Option<Accidental>) -> bool {
    let Some(cr) = element_mut(score, elem_id) else {
        return false;
    };
    for n in cr.notes.iter_mut() {
        n.pitch.accidental = acc;
    }
    true
}

/// Recalcula os `start` de uma voz como durações contíguas (reflow), aplicando o fator
/// de cada quiáltera quando houver.
fn reflow(elements: &mut [ChordOrRest], ratios: &HashMap<u64, Fraction>) {
    let mut cursor = Fraction::ZERO;
    for cr in elements.iter_mut() {
        cr.start = cursor;
        let mut d = cr.duration.as_fraction();
        if let Some(tid) = cr.tuplet {
            if let Some(r) = ratios.get(&tid.value()) {
                d = d * *r;
            }
        }
        cursor = cursor + d;
    }
}

/// Troca a duração (base, sem pontos) do elemento `elem_id` e refaz o encadeamento (reflow)
/// da voz, mantendo-a contígua. Devolve `true` se achou.
///
/// Não checa se cabe no compasso: quem chama valida o Score e desfaz se estourar.
pub fn set_duration(score: &mut Score, elem_id: u64, base: DurationBase) -> bool {
    for m in score.measures.iter_mut() {
        let ratios: HashMap<u64, Fraction> = m
            .tuplets
            .iter()
            .filter(|t| t.actual != 0 && t.normal != 0)
            .map(|t| {
                (
                    t.id.value(),
                    Fraction::new(t.normal as i64, t.actual as i64),
                )
            })
            .collect();
        for v in m.voices.iter_mut() {
            if let Some(ei) = v.elements.iter().position(|cr| cr.id.value() == elem_id) {
                v.elements[ei].duration = Duration::new(base, 0);
                reflow(&mut v.elements, &ratios);
                return true;
            }
        }
    }
    false
}

/// Base de duração a partir da tecla numérica (1=semibreve … 5=semicolcheia).
pub fn duration_base_from_key(d: char) -> Option<DurationBase> {
    Some(match d {
        '1' => DurationBase::Whole,
        '2' => DurationBase::Half,
        '3' => DurationBase::Quarter,
        '4' => DurationBase::Eighth,
        '5' => DurationBase::Sixteenth,
        _ => return None,
    })
}

/// Todos os `ElementId` do documento em ordem (compasso → voz → elemento).
pub fn element_ids_in_order(score: &Score) -> Vec<u64> {
    let mut out = Vec::new();
    for m in &score.measures {
        for v in &m.voices {
            for cr in &v.elements {
                out.push(cr.id.value());
            }
        }
    }
    out
}

/// Vizinho de `current` na ordem do documento (notas E pausas). Sem seleção, pega o primeiro
/// (ou último) elemento. Nas pontas, permanece no atual. `None` se não houver elementos.
pub fn neighbor_element(score: &Score, current: Option<u64>, forward: bool) -> Option<u64> {
    let ids = element_ids_in_order(score);
    if ids.is_empty() {
        return None;
    }
    match current.and_then(|c| ids.iter().position(|&x| x == c)) {
        Some(i) => {
            if forward {
                Some(ids.get(i + 1).copied().unwrap_or(ids[i]))
            } else if i > 0 {
                Some(ids[i - 1])
            } else {
                Some(ids[i])
            }
        }
        None => Some(if forward {
            ids[0]
        } else {
            *ids.last().unwrap()
        }),
    }
}

/// Move a seleção da aba ativa para o elemento anterior/seguinte (nota ou pausa).
pub fn navigate_active(mut tabs: Signal<Vec<Tab>>, active: Signal<usize>, forward: bool) {
    let a = *active.peek();
    let mut w = tabs.write();
    if let Some(t) = w.get_mut(a) {
        if let Some(next) = neighbor_element(&t.score, t.selected, forward) {
            t.selected = Some(next);
        }
    }
}

/// `true` se uma nota de duração `base` ainda cabe no último compasso da voz 0 **sem** criar
/// um compasso novo (ou seja, o compasso atual ainda não está completo).
pub fn fits_without_new_measure(score: &Score, base: DurationBase) -> bool {
    let frac = Duration::new(base, 0).as_fraction();
    if score.measures.is_empty() {
        return true; // sem compassos: escrever cria o primeiro
    }
    let mi = score.measures.len() - 1;
    let measure_dur = measure_dur_at(score, mi);
    let next_start = score.measures[mi]
        .voices
        .iter()
        .find(|v| v.voice_id == 0)
        .and_then(|v| v.elements.last())
        .map(|e| e.start + e.duration.as_fraction())
        .unwrap_or(Fraction::ZERO);
    next_start + frac <= measure_dur
}

/// Navega para a direita, mas se já estiver na última nota e o compasso atual ainda tiver
/// espaço, **escreve** a próxima nota (duração ativa) em vez de parar. Para a esquerda, só
/// navega. É o avanço do fluxo de escrita por teclado.
pub fn navigate_or_write(
    tabs: Signal<Vec<Tab>>,
    active: Signal<usize>,
    status: Signal<String>,
    current: Signal<DurationBase>,
    forward: bool,
) {
    if !forward {
        navigate_active(tabs, active, false);
        return;
    }

    let a = *active.peek();
    // Há um próximo elemento distinto do atual? Então é só navegar.
    let next = {
        let w = tabs.peek();
        w.get(a).and_then(|t| {
            let n = neighbor_element(&t.score, t.selected, true);
            match (t.selected, n) {
                (sel, Some(n)) if sel != Some(n) => Some(n),
                _ => None,
            }
        })
    };

    if let Some(n) = next {
        navigate_to(tabs, active, n);
        return;
    }

    // Estamos no fim: escreve, se couber no compasso atual (não cria compasso novo).
    let base = *current.peek();
    let fits = tabs
        .peek()
        .get(a)
        .map(|t| fits_without_new_measure(&t.score, base))
        .unwrap_or(false);
    if fits {
        add_note_with_active(tabs, active, status, base);
    }
}

/// Seleciona a nota `id` na aba ativa.
fn navigate_to(mut tabs: Signal<Vec<Tab>>, active: Signal<usize>, id: u64) {
    let a = *active.peek();
    if let Some(t) = tabs.write().get_mut(a) {
        t.selected = Some(id);
    }
}

/// Aplica `f` à nota selecionada da aba ativa, validando o Score antes de efetivar.
fn edit_selected(
    mut tabs: Signal<Vec<Tab>>,
    active: Signal<usize>,
    mut status: Signal<String>,
    f: impl FnOnce(&mut Score, u64),
) {
    let a = *active.peek();
    let Some((mut score, sel)) = tabs.read().get(a).map(|t| (t.score.clone(), t.selected)) else {
        return;
    };
    let Some(id) = sel else {
        status.set("selecione uma nota primeiro".into());
        return;
    };
    f(&mut score, id);
    match score.validate() {
        Ok(()) => {
            if let Some(t) = tabs.write().get_mut(a) {
                t.score = score;
            }
        }
        Err(e) => status.set(format!("edição inválida: {e:?}")),
    }
}

/// Define a altura (A–G) da nota selecionada.
pub fn set_step_active(
    tabs: Signal<Vec<Tab>>,
    active: Signal<usize>,
    status: Signal<String>,
    step: char,
) {
    edit_selected(tabs, active, status, |s, id| {
        set_step(s, id, step);
    });
}

/// Define o acidente da nota selecionada.
pub fn set_accidental_active(
    tabs: Signal<Vec<Tab>>,
    active: Signal<usize>,
    status: Signal<String>,
    acc: Option<Accidental>,
) {
    edit_selected(tabs, active, status, |s, id| {
        set_accidental(s, id, acc);
    });
}

/// Define a duração da nota selecionada (com reflow da voz).
pub fn set_duration_active(
    tabs: Signal<Vec<Tab>>,
    active: Signal<usize>,
    status: Signal<String>,
    base: DurationBase,
) {
    edit_selected(tabs, active, status, |s, id| {
        set_duration(s, id, base);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use klav_core::Accidental;

    #[test]
    fn sobe_dentro_da_oitava() {
        let p = step_pitch(Pitch::new('C', 4, None), true);
        assert_eq!((p.step, p.octave), ('D', 4));
    }

    #[test]
    fn sobe_de_b_para_c_da_proxima_oitava() {
        let p = step_pitch(Pitch::new('B', 4, None), true);
        assert_eq!((p.step, p.octave), ('C', 5));
    }

    #[test]
    fn desce_de_c_para_b_da_oitava_anterior() {
        let p = step_pitch(Pitch::new('C', 4, None), false);
        assert_eq!((p.step, p.octave), ('B', 3));
    }

    #[test]
    fn preserva_o_acidente() {
        let p = step_pitch(Pitch::new('C', 4, Some(Accidental::Sharp)), true);
        assert_eq!(
            (p.step, p.octave, p.accidental),
            ('D', 4, Some(Accidental::Sharp))
        );
    }

    #[test]
    fn step_invalido_nao_muda() {
        let orig = Pitch::new('H', 4, None);
        let p = step_pitch(orig, true);
        assert_eq!((p.step, p.octave), ('H', 4));
    }

    // --- Edição estrutural ---

    fn count_notes(s: &Score) -> usize {
        s.measures
            .iter()
            .flat_map(|m| &m.voices)
            .flat_map(|v| &v.elements)
            .map(|cr| cr.notes.len())
            .sum()
    }

    /// Compasso 4/4 com `n` semínimas (C4), ids 1.. para elementos e notas.
    fn n_quarters(n: u64) -> Score {
        let q = Fraction::new(1, 4);
        let elements = (0..n)
            .map(|i| ChordOrRest {
                id: ElementId(i + 1),
                start: q * Fraction::new(i as i64, 1),
                duration: Duration::new(DurationBase::Quarter, 0),
                is_rest: false,
                notes: vec![Note {
                    id: NoteId(i + 1),
                    pitch: Pitch::new('C', 4, None),
                    velocity: 80,
                    tie_to: None,
                }],
                tuplet: None,
            })
            .collect();
        Score {
            title: "t".into(),
            tempo_bpm: 120,
            staff_type: klav_core::StaffType::Single(klav_core::Clef::Treble),
            measures: vec![Measure {
                index: 0,
                time_signature: Some(DEFAULT_TS),
                key_signature: Some(0),
                chord_symbols: vec![],
                voices: vec![Voice {
                    voice_id: 0,
                    elements,
                }],
                tuplets: vec![],
            }],
        }
    }

    #[test]
    fn ids_livres_sao_max_mais_um() {
        let s = n_quarters(3);
        assert_eq!(next_note_id(&s), 4);
        assert_eq!(next_element_id(&s), 4);
    }

    #[test]
    fn add_note_preenche_posicao_livre_sem_criar_compasso() {
        let mut s = n_quarters(1); // cabem mais 3
        let nid = add_note_with_at(&mut s, 0, DurationBase::Quarter).unwrap();
        assert_eq!(s.measures.len(), 1);
        assert_eq!(s.validate(), Ok(()));
        // a nova entra em start = 1/4
        let el = &s.measures[0].voices[0].elements;
        assert_eq!(el.len(), 2);
        assert_eq!(el[1].start, Fraction::new(1, 4));
        assert_eq!(el[1].notes[0].id.value(), nid);
    }

    #[test]
    fn write_escreve_no_compasso_da_selecao_nao_no_fim() {
        // 2 compassos: o 0 tem 2 semínimas (cabe mais), o 1 está vazio.
        let mut s = n_quarters(2);
        add_measure(&mut s);
        // escrevendo a partir do compasso 0, a nota entra nele (em 1/2), não no compasso 1.
        add_note_with_at(&mut s, 0, DurationBase::Quarter).unwrap();
        assert_eq!(s.measures[0].voices[0].elements.len(), 3);
        assert_eq!(
            s.measures[0].voices[0].elements[2].start,
            Fraction::new(1, 2)
        );
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn write_transborda_para_o_proximo_quando_cheio() {
        // Compasso 0 cheio (4 semínimas) + compasso 1 vazio; escrever a partir do 0 vai pro 1.
        let mut s = n_quarters(4);
        add_measure(&mut s);
        add_note_with_at(&mut s, 0, DurationBase::Quarter).unwrap();
        assert_eq!(s.measures[1].voices[0].elements.len(), 1);
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn add_note_cria_compasso_quando_cheio() {
        let mut s = n_quarters(4); // compasso 4/4 cheio
        let notes_before = count_notes(&s);
        let nid = add_note_with_at(&mut s, 0, DurationBase::Quarter).unwrap();
        assert_eq!(s.validate(), Ok(()));
        assert_eq!(count_notes(&s), notes_before + 1);
        assert_eq!(s.measures.len(), 2, "devia ter criado um compasso");
        // a nota nova fica no novo compasso, no início
        assert_eq!(s.measures[1].voices[0].elements[0].notes[0].id.value(), nid);
        assert_eq!(s.measures[1].voices[0].elements[0].start, Fraction::ZERO);
    }

    #[test]
    fn add_note_with_usa_a_duracao_dada() {
        let mut s = n_quarters(0);
        s.measures.clear();
        let nid = add_note_with_at(&mut s, 0, DurationBase::Half).unwrap();
        let cr = s.measures[0].voices[0]
            .elements
            .iter()
            .find(|cr| cr.notes.iter().any(|n| n.id.value() == nid))
            .unwrap();
        assert_eq!(cr.duration.base, DurationBase::Half);
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn add_rest_coloca_pausa_na_posicao_livre() {
        let mut s = n_quarters(1); // uma semínima em 0; pausa entra em 1/4
        let eid = add_rest_with(&mut s, DurationBase::Quarter).unwrap();
        let el = &s.measures[0].voices[0].elements;
        assert_eq!(el.len(), 2);
        assert!(el[1].is_rest);
        assert!(el[1].notes.is_empty());
        assert_eq!(el[1].id.value(), eid);
        assert_eq!(el[1].start, Fraction::new(1, 4));
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn add_note_em_score_vazio_cria_compasso() {
        let mut s = n_quarters(0);
        s.measures.clear();
        let nid = add_note_with_at(&mut s, 0, DurationBase::Quarter).unwrap();
        assert_eq!(s.measures.len(), 1);
        assert_eq!(s.validate(), Ok(()));
        assert!(nid >= 1);
    }

    #[test]
    fn add_measure_herda_formula_e_valida() {
        let mut s = n_quarters(4);
        let idx = add_measure(&mut s);
        assert_eq!(idx, 1);
        assert_eq!(s.measures[1].time_signature, None); // herda
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn delete_element_remove_o_elemento() {
        let mut s = n_quarters(1);
        assert!(delete_element(&mut s, 1));
        assert_eq!(count_notes(&s), 0);
        assert!(s.measures[0].voices[0].elements.is_empty());
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn delete_element_inexistente_retorna_false() {
        let mut s = n_quarters(1);
        assert!(!delete_element(&mut s, 999));
        assert_eq!(count_notes(&s), 1);
    }

    #[test]
    fn delete_element_remove_acorde_inteiro() {
        let mut s = n_quarters(1);
        // acorde de 2 notas no elemento 1; apagar o elemento remove tudo.
        s.measures[0].voices[0].elements[0].notes.push(Note {
            id: NoteId(2),
            pitch: Pitch::new('E', 4, None),
            velocity: 80,
            tie_to: None,
        });
        assert!(delete_element(&mut s, 1));
        assert!(s.measures[0].voices[0].elements.is_empty());
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn delete_element_limpa_tie_pendurado() {
        let mut s = n_quarters(2); // elementos/notas 1 e 2 (C4), adjacentes
        s.measures[0].voices[0].elements[0].notes[0].tie_to = Some(NoteId(2));
        assert_eq!(s.validate(), Ok(())); // tie válido (mesma altura, imediato)
        assert!(delete_element(&mut s, 2)); // apaga o elemento alvo do tie
        let n1 = &s.measures[0].voices[0].elements[0].notes[0];
        assert_eq!(n1.tie_to, None); // a nota 1 não pode ficar com tie pendurado
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn convert_to_rest_transforma_nota_em_pausa() {
        let mut s = n_quarters(1);
        assert!(convert_to_rest(&mut s, 1));
        let cr = &s.measures[0].voices[0].elements[0];
        assert!(cr.is_rest);
        assert!(cr.notes.is_empty());
        assert_eq!(s.validate(), Ok(()));
    }

    // --- Entrada por teclado / navegação ---

    fn pitch_of(s: &Score, id: u64) -> Pitch {
        s.measures
            .iter()
            .flat_map(|m| &m.voices)
            .flat_map(|v| &v.elements)
            .flat_map(|cr| &cr.notes)
            .find(|n| n.id.value() == id)
            .unwrap()
            .pitch
    }

    #[test]
    fn set_step_troca_nome_e_zera_acidente() {
        let mut s = n_quarters(1);
        set_accidental(&mut s, 1, Some(Accidental::Sharp));
        assert!(set_step(&mut s, 1, 'f'));
        let p = pitch_of(&s, 1);
        assert_eq!((p.step, p.octave, p.accidental), ('F', 4, None));
    }

    #[test]
    fn set_accidental_define_bemol() {
        let mut s = n_quarters(1);
        assert!(set_accidental(&mut s, 1, Some(Accidental::Flat)));
        assert_eq!(pitch_of(&s, 1).accidental, Some(Accidental::Flat));
    }

    #[test]
    fn set_duration_refaz_os_starts_da_voz() {
        // duas semínimas (starts 0, 1/4); troca a 1ª para mínima -> starts 0, 1/2.
        let mut s = n_quarters(2);
        assert!(set_duration(&mut s, 1, DurationBase::Half));
        let el = &s.measures[0].voices[0].elements;
        assert_eq!(el[0].start, Fraction::ZERO);
        assert_eq!(el[1].start, Fraction::new(1, 2));
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn set_duration_que_estoura_e_pego_pela_validacao() {
        // 4 semínimas; alongar a última para semibreve estoura o compasso 4/4.
        let mut s = n_quarters(4);
        assert!(set_duration(&mut s, 4, DurationBase::Whole));
        assert!(matches!(
            s.validate(),
            Err(klav_core::ValidationError::VoiceExceedsMeasure { .. })
        ));
    }

    #[test]
    fn duration_base_from_key_mapeia_1_a_5() {
        assert_eq!(duration_base_from_key('1'), Some(DurationBase::Whole));
        assert_eq!(duration_base_from_key('3'), Some(DurationBase::Quarter));
        assert_eq!(duration_base_from_key('5'), Some(DurationBase::Sixteenth));
        assert_eq!(duration_base_from_key('9'), None);
    }

    #[test]
    fn navegacao_anda_e_para_nas_pontas() {
        let s = n_quarters(3); // elementos 1,2,3
        assert_eq!(neighbor_element(&s, None, true), Some(1));
        assert_eq!(neighbor_element(&s, None, false), Some(3));
        assert_eq!(neighbor_element(&s, Some(1), true), Some(2));
        assert_eq!(neighbor_element(&s, Some(2), false), Some(1));
        assert_eq!(neighbor_element(&s, Some(3), true), Some(3)); // último: permanece
        assert_eq!(neighbor_element(&s, Some(1), false), Some(1)); // primeiro: permanece
    }

    #[test]
    fn navegacao_inclui_pausas() {
        let mut s = n_quarters(1);
        add_rest_with(&mut s, DurationBase::Quarter); // elemento 2 = pausa
        assert_eq!(neighbor_element(&s, Some(1), true), Some(2)); // navega para a pausa
    }

    #[test]
    fn navegacao_em_score_sem_elementos_e_none() {
        let mut s = n_quarters(0);
        s.measures.clear();
        assert_eq!(neighbor_element(&s, None, true), None);
    }

    #[test]
    fn fits_respeita_o_espaco_restante_do_compasso() {
        let s = n_quarters(3); // 3/4 usados em 4/4; falta 1/4
        assert!(fits_without_new_measure(&s, DurationBase::Quarter)); // 1/4 cabe
        assert!(!fits_without_new_measure(&s, DurationBase::Half)); // 1/2 não cabe
        let cheio = n_quarters(4);
        assert!(!fits_without_new_measure(&cheio, DurationBase::Quarter)); // compasso cheio
    }
}
