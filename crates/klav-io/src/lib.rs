//! # klav-io
//!
//! Import/export do klav. Neste primeiro corte: leitura e escrita do formato nativo
//! **`.klav`**, que é JSON (via `serde`) com um envelope versionado.
//!
//! O arquivo não guarda o [`Score`] cru: ele é embrulhado num [`KlavDocument`] que carrega
//! `format_version`, permitindo evoluir o formato sem quebrar arquivos antigos.
//!
//! Ao carregar, o conteúdo é validado com [`Score::validate`]; use as variantes `*_unchecked`
//! para pular a validação (ex.: carregar um rascunho parcial durante edição).

use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use klav_core::{Score, ValidationError};

/// Versão atual do formato `.klav` escrito por esta crate.
pub const KLAV_FORMAT_VERSION: u32 = 1;

/// Envelope serializado em disco. Mantém o `format_version` junto do `Score`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KlavDocument {
    pub format_version: u32,
    pub score: Score,
}

impl KlavDocument {
    /// Cria um documento com a versão de formato atual.
    pub fn new(score: Score) -> KlavDocument {
        KlavDocument {
            format_version: KLAV_FORMAT_VERSION,
            score,
        }
    }
}

/// Erros possíveis de I/O do klav.
#[derive(Debug)]
pub enum KlavIoError {
    /// Falha de leitura/escrita no sistema de arquivos.
    Io(std::io::Error),
    /// Falha ao (de)serializar o JSON.
    Json(serde_json::Error),
    /// O arquivo foi escrito por uma versão de formato que esta build não entende.
    UnsupportedVersion { found: u32, supported: u32 },
    /// O conteúdo carregou, mas viola um invariante do modelo.
    Invalid(ValidationError),
}

impl fmt::Display for KlavIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KlavIoError::Io(e) => write!(f, "erro de I/O: {e}"),
            KlavIoError::Json(e) => write!(f, "erro de JSON: {e}"),
            KlavIoError::UnsupportedVersion { found, supported } => write!(
                f,
                "versão de formato .klav não suportada: arquivo={found}, suportada={supported}"
            ),
            KlavIoError::Invalid(e) => write!(f, "modelo inválido: {e:?}"),
        }
    }
}

impl std::error::Error for KlavIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            KlavIoError::Io(e) => Some(e),
            KlavIoError::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for KlavIoError {
    fn from(e: std::io::Error) -> Self {
        KlavIoError::Io(e)
    }
}

impl From<serde_json::Error> for KlavIoError {
    fn from(e: serde_json::Error) -> Self {
        KlavIoError::Json(e)
    }
}

/// Resultado conveniente para esta crate.
pub type Result<T> = std::result::Result<T, KlavIoError>;

// ---------------------------------------------------------------------------
// Serialização para String
// ---------------------------------------------------------------------------

/// Serializa um [`Score`] para uma string JSON `.klav` (formatada, com envelope versionado).
pub fn to_json_string(score: &Score) -> Result<String> {
    let doc = KlavDocument::new(score.clone());
    Ok(serde_json::to_string_pretty(&doc)?)
}

/// Desserializa um [`Score`] de uma string JSON `.klav`, validando versão e invariantes.
pub fn from_json_str(s: &str) -> Result<Score> {
    let score = from_json_str_unchecked(s)?;
    score.validate().map_err(KlavIoError::Invalid)?;
    Ok(score)
}

/// Como [`from_json_str`], mas sem validar os invariantes do modelo.
/// A versão de formato ainda é verificada.
pub fn from_json_str_unchecked(s: &str) -> Result<Score> {
    let doc: KlavDocument = serde_json::from_str(s)?;
    check_version(doc.format_version)?;
    Ok(doc.score)
}

// ---------------------------------------------------------------------------
// Leitura/escrita em arquivo
// ---------------------------------------------------------------------------

/// Salva um [`Score`] num arquivo `.klav` (JSON com envelope versionado).
pub fn save_to_path(score: &Score, path: impl AsRef<Path>) -> Result<()> {
    let json = to_json_string(score)?;
    fs::write(path, json)?;
    Ok(())
}

/// Carrega um [`Score`] de um arquivo `.klav`, validando versão e invariantes.
pub fn load_from_path(path: impl AsRef<Path>) -> Result<Score> {
    let s = fs::read_to_string(path)?;
    from_json_str(&s)
}

/// Como [`load_from_path`], mas sem validar os invariantes do modelo.
pub fn load_from_path_unchecked(path: impl AsRef<Path>) -> Result<Score> {
    let s = fs::read_to_string(path)?;
    from_json_str_unchecked(&s)
}

fn check_version(found: u32) -> Result<()> {
    if found == KLAV_FORMAT_VERSION {
        Ok(())
    } else {
        Err(KlavIoError::UnsupportedVersion {
            found,
            supported: KLAV_FORMAT_VERSION,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klav_core::{
        ChordOrRest, Duration, DurationBase, ElementId, Fraction, Measure, Note, NoteId, Pitch,
        Score, StaffType, TimeSignature, Voice, Clef,
    };

    fn sample_score() -> Score {
        let q = Fraction::new(1, 4);
        let voice = Voice {
            voice_id: 0,
            elements: vec![
                cr(1, Fraction::ZERO, 10, 'C', 4),
                cr(2, q, 11, 'D', 4),
                cr(3, q + q, 12, 'E', 4),
                cr(4, q + q + q, 13, 'F', 4),
            ],
        };
        Score {
            title: "teste".into(),
            tempo_bpm: 120,
            staff_type: StaffType::Single(Clef::Treble),
            measures: vec![Measure {
                index: 0,
                time_signature: Some(TimeSignature { numerator: 4, denominator: 4 }),
                key_signature: Some(0),
                chord_symbols: vec![],
                voices: vec![voice],
                tuplets: vec![],
            }],
        }
    }

    fn cr(id: u64, start: Fraction, note_id: u64, step: char, octave: i8) -> ChordOrRest {
        ChordOrRest {
            id: ElementId(id),
            start,
            duration: Duration::new(DurationBase::Quarter, 0),
            is_rest: false,
            notes: vec![Note {
                id: NoteId(note_id),
                pitch: Pitch::new(step, octave, None),
                velocity: 80,
                tie_to: None,
            }],
            tuplet: None,
        }
    }

    #[test]
    fn round_trip_em_string() {
        let score = sample_score();
        let json = to_json_string(&score).unwrap();
        let back = from_json_str(&json).unwrap();
        assert_eq!(back, score);
    }

    #[test]
    fn json_inclui_versao_de_formato() {
        let json = to_json_string(&sample_score()).unwrap();
        assert!(json.contains("\"format_version\""));
        assert!(json.contains(&format!("{KLAV_FORMAT_VERSION}")));
    }

    #[test]
    fn versao_desconhecida_e_erro() {
        let json = to_json_string(&sample_score()).unwrap();
        // Troca a versão para uma futura inexistente.
        let bumped = json.replacen(
            &format!("\"format_version\": {KLAV_FORMAT_VERSION}"),
            "\"format_version\": 999",
            1,
        );
        match from_json_str(&bumped) {
            Err(KlavIoError::UnsupportedVersion { found: 999, supported }) => {
                assert_eq!(supported, KLAV_FORMAT_VERSION);
            }
            other => panic!("esperava UnsupportedVersion, veio {other:?}"),
        }
    }

    #[test]
    fn carregar_valida_invariantes() {
        // Monta um documento cujo Score estoura o compasso (5 semínimas em 4/4).
        let mut score = sample_score();
        score.measures[0].voices[0]
            .elements
            .push(cr(5, Fraction::ONE, 14, 'G', 4));
        let json = to_json_string(&score).unwrap();
        // unchecked passa; checked falha.
        assert!(from_json_str_unchecked(&json).is_ok());
        assert!(matches!(from_json_str(&json), Err(KlavIoError::Invalid(_))));
    }

    #[test]
    fn time_signature_denominador_zero_e_erro_nao_panic() {
        // Corrompe a fórmula de compasso para 4/0 no JSON de um score válido.
        let json = to_json_string(&sample_score()).unwrap();
        let bad = json.replacen("\"denominator\": 4", "\"denominator\": 0", 1);
        match from_json_str(&bad) {
            Err(KlavIoError::Invalid(_)) => {}
            other => panic!("esperava Invalid por denominador zero, veio {other:?}"),
        }
    }

    #[test]
    fn fraction_denominador_zero_e_erro_de_parsing() {
        // Injeta uma Fraction com den=0 num `start`; deve falhar já na desserialização.
        let json = to_json_string(&sample_score()).unwrap();
        let bad = json.replacen("\"den\": 4", "\"den\": 0", 1);
        match from_json_str(&bad) {
            Err(KlavIoError::Json(_)) => {}
            other => panic!("esperava Json por Fraction den=0, veio {other:?}"),
        }
        // E também pelo caminho unchecked (a validação de versão não mascara o erro).
        assert!(matches!(from_json_str_unchecked(&bad), Err(KlavIoError::Json(_))));
    }

    #[test]
    fn round_trip_em_arquivo() {
        let score = sample_score();
        let mut path = std::env::temp_dir();
        path.push(format!("klav_io_test_{}.klav", std::process::id()));
        save_to_path(&score, &path).unwrap();
        let back = load_from_path(&path).unwrap();
        assert_eq!(back, score);
        let _ = fs::remove_file(&path);
    }
}
