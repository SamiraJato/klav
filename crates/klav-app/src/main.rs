//! klav-app — Fatia 1: renderiza um `Score` em SVG (com a fonte Bravura) e abre/salva `.klav`.
//!
//! Frontend em Dioxus (Rust/WASM), rodando como janela desktop via webview nativo.
//! Consome a geometria do `klav-layout` com os tipos do core direto (sem serialização).

mod synth;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use dioxus::desktop::{use_window, Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

use synth::{SynthCmd, SynthRuntime};

use klav_core::{
    ChordOrRest, Clef, Duration as NoteDuration, DurationBase, ElementId, Fraction, Measure, Note,
    NoteId, Pitch, Score, StaffType, TimeSignature, Voice,
};
use klav_engine::{render_events, total_duration_whole, whole_to_seconds, MidiEvent, MidiMessage};
use klav_layout::{layout_single_staff, StaffLayout, CLEF_WIDTH, MARGIN, SPACE_PER_WHOLE};

/// Fonte Bravura (SMuFL, SIL OFL) embarcada no binário.
const BRAVURA_WOFF2: &[u8] = include_bytes!("../assets/Bravura.woff2");

/// Pixels por staff-space. No SMuFL, 1 em da fonte = 4 staff-spaces.
const SP_PX: f64 = 10.0;
const LEFT: f64 = 16.0;
const TOP: f64 = 60.0;

// Teclado de 88 teclas.
const WHITE_W: f64 = 15.0;
const BLACK_W: f64 = 9.0;

fn px_x(x_sp: f64) -> f64 {
    LEFT + x_sp * SP_PX
}
fn px_y(y_sp: f64) -> f64 {
    TOP + y_sp * SP_PX
}

fn main() {
    // `always_on_top(false)` sobrescreve o default de debug do Dioxus (que fixa a janela
    // sobre tudo durante o desenvolvimento).
    let window = WindowBuilder::new()
        .with_title("klav")
        .with_always_on_top(false)
        .with_inner_size(LogicalSize::new(1000.0, 640.0));
    dioxus::LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window))
        .launch(App);
}

#[component]
fn App() -> Element {
    let mut score = use_signal(sample_score);
    let mut selected = use_signal(|| Option::<u64>::None);
    let mut status = use_signal(String::new);

    // Geometria recalculada automaticamente quando o Score muda (tipos do core, sem serializar).
    let layout = use_memo(move || layout_single_staff(&score.read()));

    // CSS (com a Bravura embarcada como data URL) montado uma única vez.
    let css = use_hook(build_css);

    // Janela nativa — usada como pai dos diálogos (corrige o z-order no Windows).
    let window = use_window();

    let w_open = window.clone();
    let open = move |_| {
        let w = w_open.clone();
        spawn(async move {
            let picked = rfd::AsyncFileDialog::new()
                .add_filter("klav", &["klav"])
                .set_parent(&w.window)
                .pick_file()
                .await;
            if let Some(handle) = picked {
                let path = handle.path().to_path_buf();
                match klav_io::load_from_path(&path) {
                    Ok(s) => {
                        score.set(s);
                        selected.set(None);
                        status.set(format!("aberto: {}", path.display()));
                    }
                    Err(e) => status.set(format!("erro ao abrir: {e}")),
                }
            }
        });
    };

    let w_save = window.clone();
    let save = move |_| {
        let w = w_save.clone();
        spawn(async move {
            let picked = rfd::AsyncFileDialog::new()
                .add_filter("klav", &["klav"])
                .set_file_name("partitura.klav")
                .set_parent(&w.window)
                .save_file()
                .await;
            if let Some(handle) = picked {
                let path = handle.path().to_path_buf();
                let result = klav_io::save_to_path(&score.read(), &path);
                match result {
                    Ok(()) => status.set(format!("salvo: {}", path.display())),
                    Err(e) => status.set(format!("erro ao salvar: {e}")),
                }
            }
        });
    };

    // Edição: move a altura da nota selecionada um passo diatônico para cima/baixo.
    let up = move |_| {
        if transpose_selected(score, selected, true) {
            status.set("nota movida ↑".into());
        }
    };
    let down = move |_| {
        if transpose_selected(score, selected, false) {
            status.set("nota movida ↓".into());
        }
    };

    // --- Playback ---
    let tempo = score.read().tempo_bpm;
    // Intervalos (tecla, início, fim) em semibreves, recalculados quando o Score muda.
    let intervals = use_memo(move || build_intervals(&render_events(&score.read())));
    // Duração total da peça em segundos.
    let total_secs = use_memo(move || whole_to_seconds(total_duration_whole(&score.read()), tempo));

    let mut playing = use_signal(|| false);
    let mut position = use_signal(|| 0.0_f64); // posição atual em segundos

    // Runtime de áudio (stream + canal), reconstruível ao trocar o dispositivo de saída.
    let runtime = use_hook(|| Rc::new(RefCell::new(SynthRuntime::default_output())));
    let devices = use_hook(synth::list_output_devices);
    let mut device_name = use_signal(|| synth::default_output_name().unwrap_or_default());

    // Relógio: avança a posição em tempo real e dispara NoteOn/NoteOff no synth (~60 fps).
    let rt_clock = runtime.clone();
    use_future(move || {
        let rt = rt_clock.clone();
        async move {
            let mut last = Instant::now();
            // Cada nota é identificada por (tecla, início) para separar repetições da mesma altura.
            let mut sounding: HashSet<(u8, i64)> = HashSet::new();
            loop {
                futures_timer::Delay::new(Duration::from_millis(16)).await;
                let now = Instant::now();
                let dt = (now - last).as_secs_f64();
                last = now;

                // sender atual (reflete troca de dispositivo de saída)
                let tx = rt.borrow().tx.clone();

                if *playing.peek() {
                    let total = *total_secs.peek();
                    let mut np = *position.peek() + dt;
                    if np >= total {
                        np = total;
                        playing.set(false);
                    }
                    position.set(np);

                    // notas que soam na nova posição, identificadas por (tecla, início)
                    let pw = np * tempo as f64 / 240.0;
                    let current: HashSet<(u8, i64)> = intervals
                        .peek()
                        .iter()
                        .filter(|(_, s, e)| *s <= pw && pw < *e)
                        .map(|(k, s, _)| (*k, (s * 1_000_000.0) as i64))
                        .collect();
                    if let Some(tx) = &tx {
                        // NoteOff antes de NoteOn para re-articular notas repetidas na fronteira.
                        for (k, _) in sounding.difference(&current) {
                            let _ = tx.send(SynthCmd::NoteOff(*k));
                        }
                        for (k, _) in current.difference(&sounding) {
                            let _ = tx.send(SynthCmd::NoteOn(*k));
                        }
                    }
                    sounding = current;
                } else if !sounding.is_empty() {
                    // parou/pausou: silencia tudo
                    if let Some(tx) = &tx {
                        let _ = tx.send(SynthCmd::AllOff);
                    }
                    sounding.clear();
                }
            }
        }
    });

    let play_pause = move |_| {
        let is_playing = *playing.peek();
        if !is_playing && *position.peek() >= *total_secs.peek() {
            position.set(0.0); // recomeça se estava no fim
        }
        playing.set(!is_playing);
    };
    let rt_sel = runtime.clone();
    let change_device = move |evt: Event<FormData>| {
        let name = evt.value();
        device_name.set(name.clone());
        if let Some(dev) = synth::device_by_name(&name) {
            *rt_sel.borrow_mut() = SynthRuntime::on_device(&dev);
        }
    };

    let stop = move |_| {
        playing.set(false);
        position.set(0.0);
    };

    let lay: StaffLayout = layout.read().clone();
    let title = score.read().title.clone();
    let sel_label = match selected() {
        Some(id) => format!("nota selecionada: id {id}"),
        None => "clique numa nota".to_string(),
    };

    let width_px = px_x(lay.width) + LEFT;
    let height_px = px_y(9.0);

    // Posição musical atual (em semibreves) e cursor na pauta.
    let pos_secs = position();
    let total = total_secs();
    let pos_whole = pos_secs * tempo as f64 / 240.0;
    let cursor_x = px_x(MARGIN + CLEF_WIDTH + SPACE_PER_WHOLE * pos_whole);
    let show_cursor = playing() || pos_secs > 0.0;

    // Teclas que soam agora (para acender no teclado).
    let active: HashSet<u8> = {
        let iv = intervals.read();
        iv.iter()
            .filter(|(_, s, e)| *s <= pos_whole && pos_whole < *e)
            .map(|(k, _, _)| *k)
            .collect()
    };

    // Geometria do teclado de 88 teclas (A0=21 .. C8=108).
    let (whites, blacks, kb_width) = keyboard_geometry();

    rsx! {
        style { {css} }
        div { class: "app",
            div { class: "titlebar",
                span { class: "brand", "klav" }
                button { class: "btn", onclick: open, "Abrir" }
                button { class: "btn", onclick: save, "Salvar" }
                span { class: "divider" }
                button { class: "btn", disabled: selected().is_none(), onclick: up, "♪ ↑" }
                button { class: "btn", disabled: selected().is_none(), onclick: down, "♪ ↓" }
                span { class: "divider" }
                button { class: "btn", onclick: play_pause, if playing() { "⏸ Pausar" } else { "▶ Tocar" } }
                button { class: "btn", onclick: stop, "⏹ Parar" }
                span { class: "divider" }
                select { class: "device", title: "Saída de áudio", onchange: change_device,
                    for d in devices.iter() {
                        option { value: "{d}", selected: *d == device_name(), "🔊 {d}" }
                    }
                }
            }
            div { class: "statusbar",
                span { "{title}" }
                span { "• {tempo} BPM" }
                span { "• {lay.glyphs.len()} glifos" }
                span { "• ⏱ {pos_secs:.1}s / {total:.1}s" }
                span { class: "sel", "• {sel_label}" }
            }
            div { class: "canvas",
                svg { width: "{width_px}", height: "{height_px}",
                    // linhas da pauta
                    for i in 0..lay.line_count {
                        line {
                            x1: "{px_x(0.0)}", y1: "{px_y(i as f64)}",
                            x2: "{px_x(lay.width)}", y2: "{px_y(i as f64)}",
                            stroke: "#333", stroke_width: "1",
                        }
                    }
                    // glifos SMuFL (clave, cabeças, pausas) desenhados com a Bravura
                    for g in lay.glyphs.iter() {
                        {
                            let ch = g.glyph.codepoint();
                            let x = px_x(g.pos.x);
                            let y = px_y(g.pos.y);
                            let nid = g.note_id;
                            let is_sel = nid.is_some() && selected() == nid;
                            let fill = if is_sel { "crimson" } else { "#111" };
                            rsx! {
                                text {
                                    x: "{x}", y: "{y}",
                                    font_family: "Bravura",
                                    font_size: "{SP_PX * 4.0}",
                                    fill: "{fill}",
                                    style: if nid.is_some() { "cursor: pointer;" } else { "" },
                                    onclick: move |_| { if let Some(id) = nid { selected.set(Some(id)); } },
                                    "{ch}"
                                }
                            }
                        }
                    }
                    // cursor de playback
                    if show_cursor {
                        line {
                            x1: "{cursor_x}", y1: "{px_y(-1.0)}",
                            x2: "{cursor_x}", y2: "{px_y(5.0)}",
                            stroke: "crimson", stroke_width: "1.5",
                        }
                    }
                }
            }
            // teclado de 88 teclas (acende as notas que soam)
            div { class: "keyboard",
                svg { width: "{kb_width}", height: "70",
                    for (midi, x) in whites.iter() {
                        rect {
                            x: "{x}", y: "0", width: "{WHITE_W - 1.0}", height: "70",
                            rx: "2",
                            fill: if active.contains(midi) { "crimson" } else { "#fff" },
                            stroke: "#333", stroke_width: "1",
                        }
                    }
                    for (midi, x) in blacks.iter() {
                        rect {
                            x: "{x}", y: "0", width: "{BLACK_W}", height: "44",
                            rx: "1",
                            fill: if active.contains(midi) { "crimson" } else { "#111" },
                        }
                    }
                }
            }
            if !status.read().is_empty() {
                div { class: "toast", "{status}" }
            }
        }
    }
}

/// Score de exemplo exibido ao abrir o app: escala de Dó subindo em dois compassos 4/4.
fn sample_score() -> Score {
    let q = Fraction::new(1, 4);
    let steps = [('C', 4), ('D', 4), ('E', 4), ('F', 4), ('G', 4), ('A', 4), ('B', 4), ('C', 5)];

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
                Some(TimeSignature { numerator: 4, denominator: 4 })
            } else {
                None
            },
            key_signature: Some(0),
            chord_symbols: vec![],
            voices: vec![Voice { voice_id: 0, elements }],
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

/// Pareia NoteOn/NoteOff em intervalos (tecla, início, fim) em semibreves.
fn build_intervals(events: &[MidiEvent]) -> Vec<(u8, f64, f64)> {
    let mut open: HashMap<u8, f64> = HashMap::new();
    let mut out = Vec::new();
    for e in events {
        match e.message {
            MidiMessage::NoteOn { key, .. } => {
                open.insert(key, e.time.to_f64());
            }
            MidiMessage::NoteOff { key } => {
                if let Some(start) = open.remove(&key) {
                    out.push((key, start, e.time.to_f64()));
                }
            }
        }
    }
    out
}

/// Geometria das 88 teclas (A0=21 .. C8=108): (brancas, pretas, largura total).
fn keyboard_geometry() -> (Vec<(u8, f64)>, Vec<(u8, f64)>, f64) {
    let mut whites = Vec::new();
    let mut blacks = Vec::new();
    let mut wx = 0.0;
    for midi in 21u8..=108 {
        let pc = midi % 12;
        if matches!(pc, 1 | 3 | 6 | 8 | 10) {
            // tecla preta: centrada na fronteira entre a branca anterior e a próxima
            blacks.push((midi, wx - BLACK_W / 2.0));
        } else {
            whites.push((midi, wx));
            wx += WHITE_W;
        }
    }
    (whites, blacks, wx)
}

/// Move a nota selecionada um passo diatônico. Retorna `true` se mudou algo.
fn transpose_selected(mut score: Signal<Score>, selected: Signal<Option<u64>>, up: bool) -> bool {
    let Some(id) = *selected.peek() else {
        return false;
    };
    let mut changed = false;
    let mut s = score.write();
    for m in s.measures.iter_mut() {
        for v in m.voices.iter_mut() {
            for cr in v.elements.iter_mut() {
                for n in cr.notes.iter_mut() {
                    if n.id.value() == id {
                        n.pitch = step_pitch(n.pitch, up);
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}

/// Sobe/desce uma nota um passo diatônico (C→D→…→B→C+8ª), preservando o acidente.
fn step_pitch(p: Pitch, up: bool) -> Pitch {
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

/// Monta o CSS do app, embutindo a Bravura como data URL base64.
fn build_css() -> String {
    let font_b64 = base64_encode(BRAVURA_WOFF2);
    format!(
        r#"
        @font-face {{
            font-family: 'Bravura';
            src: url('data:font/woff2;base64,{font_b64}') format('woff2');
        }}
        body {{ margin: 0; }}
        .app {{ font-family: 'Segoe UI', sans-serif; color: #111; }}
        .titlebar {{
            background: #1e1e2e; color: #cdd6f4; padding: 8px 14px;
            display: flex; align-items: center; gap: 10px;
        }}
        .titlebar .brand {{ font-weight: 700; font-size: 15px; margin-right: 6px; }}
        .btn {{
            background: #45475a; color: #cdd6f4; border: none; border-radius: 4px;
            padding: 5px 12px; font-size: 13px; cursor: pointer;
        }}
        .btn:hover {{ background: #585b70; }}
        .btn:disabled {{ opacity: 0.4; cursor: default; }}
        .divider {{ width: 1px; align-self: stretch; background: #45475a; margin: 0 4px; }}
        .device {{
            background: #45475a; color: #cdd6f4; border: none; border-radius: 4px;
            padding: 5px 8px; font-size: 12px; max-width: 220px; cursor: pointer;
        }}
        .statusbar {{
            background: #f3f3f3; border-bottom: 1px solid #ddd;
            padding: 6px 14px; font-size: 12px; color: #555;
            display: flex; gap: 10px;
        }}
        .statusbar .sel {{ margin-left: auto; color: crimson; }}
        .canvas {{ padding: 16px; overflow: auto; background: #fff; }}
        .keyboard {{ padding: 8px 16px; background: #faf8f5; border-top: 1px solid #ddd; overflow-x: auto; }}
        .toast {{
            position: fixed; bottom: 14px; right: 14px;
            background: #1e1e2e; color: #cdd6f4; padding: 8px 14px;
            border-radius: 6px; font-size: 12px;
        }}
    "#
    )
}

/// Base64 padrão (sem dependência externa) para embutir a fonte como data URL.
fn base64_encode(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { T[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    out
}
