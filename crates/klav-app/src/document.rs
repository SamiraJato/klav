//! Abas e documentos: tipo [`Tab`], abertura/fechamento de arquivos e a partitura de exemplo.
//!
//! Concentra as operações de documento/projeto (abrir, fechar, listar abas) que antes
//! ficavam diluídas em `main.rs`. As funções que mexem no estado recebem os `Signal`
//! da composição, sem conhecer o resto da UI.

use std::path::PathBuf;

use dioxus::prelude::*;

use klav_core::{
    ChordOrRest, Clef, Duration as NoteDuration, DurationBase, ElementId, Fraction, Measure, Note,
    NoteId, Pitch, Score, StaffType, TimeSignature, Voice,
};

/// Uma aba aberta: um arquivo (ou documento novo) com seu Score e nota selecionada.
#[derive(Clone)]
pub struct Tab {
    pub path: Option<PathBuf>,
    pub score: Score,
    pub selected: Option<u64>,
}

impl Tab {
    /// Aba inicial com a partitura de exemplo.
    pub fn sample() -> Self {
        Tab {
            path: None,
            score: sample_score(),
            selected: None,
        }
    }

    /// Título exibido: nome do arquivo, ou o título da partitura se não salva.
    pub fn title(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.score.title.clone())
    }
}

/// Abre um `.klav` numa aba (ou ativa a existente, se já aberto).
pub fn open_file(
    mut tabs: Signal<Vec<Tab>>,
    mut active: Signal<usize>,
    mut playing: Signal<bool>,
    mut position: Signal<f64>,
    mut status: Signal<String>,
    path: PathBuf,
) {
    match klav_io::load_from_path(&path) {
        Ok(s) => {
            let existing = tabs
                .read()
                .iter()
                .position(|t| t.path.as_deref() == Some(path.as_path()));
            match existing {
                Some(i) => active.set(i),
                None => {
                    tabs.write().push(Tab {
                        path: Some(path.clone()),
                        score: s,
                        selected: None,
                    });
                    let i = tabs.read().len() - 1;
                    active.set(i);
                }
            }
            playing.set(false);
            position.set(0.0);
            status.set(format!("aberto: {}", path.display()));
        }
        Err(e) => status.set(format!("erro ao abrir: {e}")),
    }
}

/// Fecha a aba `i`, ajustando a aba ativa.
pub fn close_tab(
    mut tabs: Signal<Vec<Tab>>,
    mut active: Signal<usize>,
    mut playing: Signal<bool>,
    mut position: Signal<f64>,
    i: usize,
) {
    {
        let mut w = tabs.write();
        if i < w.len() {
            w.remove(i);
        }
    }
    let len = tabs.read().len();
    let a = *active.peek();
    if len == 0 {
        active.set(0);
    } else if a >= len {
        active.set(len - 1);
    } else if a > i {
        active.set(a - 1);
    }
    playing.set(false);
    position.set(0.0);
}

/// Score de exemplo: escala de Dó subindo em dois compassos 4/4.
pub fn sample_score() -> Score {
    let q = Fraction::new(1, 4);
    let steps = [
        ('C', 4),
        ('D', 4),
        ('E', 4),
        ('F', 4),
        ('G', 4),
        ('A', 4),
        ('B', 4),
        ('C', 5),
    ];

    let mut measures = Vec::new();
    for m in 0..2u32 {
        let mut elements = Vec::new();
        for i in 0..4u64 {
            let (step, oct) = steps[(m as usize) * 4 + i as usize];
            let nid = m as u64 * 100 + i;
            elements.push(ChordOrRest {
                id: ElementId(nid),
                start: q * Fraction::new(i as i64, 1),
                duration: NoteDuration::new(DurationBase::Quarter, 0),
                is_rest: false,
                notes: vec![Note {
                    id: NoteId(nid),
                    pitch: Pitch::new(step, oct, None),
                    velocity: 80,
                    tie_to: None,
                }],
                tuplet: None,
            });
        }
        measures.push(Measure {
            index: m,
            time_signature: if m == 0 {
                Some(TimeSignature {
                    numerator: 4,
                    denominator: 4,
                })
            } else {
                None
            },
            key_signature: Some(0),
            chord_symbols: vec![],
            voices: vec![Voice {
                voice_id: 0,
                elements,
            }],
            tuplets: vec![],
        });
    }

    Score {
        title: "Escala de Dó".into(),
        tempo_bpm: 120,
        staff_type: StaffType::Single(Clef::Treble),
        measures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_e_valido() {
        // A partitura de exemplo precisa satisfazer os invariantes do modelo.
        assert_eq!(sample_score().validate(), Ok(()));
    }

    #[test]
    fn titulo_usa_nome_do_arquivo_quando_salvo() {
        let mut t = Tab::sample();
        assert_eq!(t.title(), "Escala de Dó");
        t.path = Some(PathBuf::from("/x/minha.klav"));
        assert_eq!(t.title(), "minha.klav");
    }
}
