//! klav-app — Fatia 3a: esqueleto de IDE (explorer de pasta, abas, mixer placeholder,
//! status bar) sobre o render/playback das fatias anteriores.
//!
//! Frontend em Dioxus (Rust/WASM), rodando como janela desktop via webview nativo.

mod synth;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
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
const TOP: f64 = 40.0;

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
    let window = WindowBuilder::new()
        .with_title("klav")
        .with_always_on_top(false)
        .with_inner_size(LogicalSize::new(1180.0, 720.0));
    dioxus::LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window))
        .launch(App);
}

/// Uma aba aberta: um arquivo (ou documento novo) com seu Score e nota selecionada.
#[derive(Clone)]
struct Tab {
    path: Option<PathBuf>,
    score: Score,
    selected: Option<u64>,
}

impl Tab {
    fn sample() -> Self {
        Tab { path: None, score: sample_score(), selected: None }
    }
    fn title(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.score.title.clone())
    }
}

#[component]
fn App() -> Element {
    let css = use_hook(build_css);
    let window = use_window();
    let mut status = use_signal(String::new);

    // Projeto (pasta) e arquivos .klav encontrados.
    let mut project_dir = use_signal(|| Option::<PathBuf>::None);
    let mut project_files = use_signal(Vec::<PathBuf>::new);

    // Abas e aba ativa.
    let mut tabs = use_signal(|| vec![Tab::sample()]);
    let mut active = use_signal(|| 0usize);

    // Playback.
    let mut playing = use_signal(|| false);
    let mut position = use_signal(|| 0.0_f64);

    // Áudio.
    let runtime = use_hook(|| Rc::new(RefCell::new(SynthRuntime::default_output())));
    let devices = use_hook(synth::list_output_devices);
    let mut device_name = use_signal(|| synth::default_output_name().unwrap_or_default());

    // Derivados da aba ativa (None se não houver aba).
    let active_score = use_memo(move || tabs.read().get(active()).map(|t| t.score.clone()));
    let tempo = use_memo(move || active_score.read().as_ref().map(|s| s.tempo_bpm).unwrap_or(120));
    let layout = use_memo(move || active_score.read().as_ref().map(layout_single_staff));
    let intervals = use_memo(move || {
        active_score
            .read()
            .as_ref()
            .map(|s| build_intervals(&render_events(s)))
            .unwrap_or_default()
    });
    let total_secs = use_memo(move || {
        active_score
            .read()
            .as_ref()
            .map(|s| whole_to_seconds(total_duration_whole(s), s.tempo_bpm))
            .unwrap_or(0.0)
    });

    // Relógio de playback (~60 fps) + disparo de NoteOn/NoteOff no synth.
    let rt_clock = runtime.clone();
    use_future(move || {
        let rt = rt_clock.clone();
        async move {
            let mut last = Instant::now();
            let mut sounding: HashSet<(u8, i64)> = HashSet::new();
            loop {
                futures_timer::Delay::new(Duration::from_millis(16)).await;
                let now = Instant::now();
                let dt = (now - last).as_secs_f64();
                last = now;

                let tx = rt.borrow().tx.clone();

                if *playing.peek() {
                    let total = *total_secs.peek();
                    let mut np = *position.peek() + dt;
                    if np >= total {
                        np = total;
                        playing.set(false);
                    }
                    position.set(np);

                    let pw = np * *tempo.peek() as f64 / 240.0;
                    let current: HashSet<(u8, i64)> = intervals
                        .peek()
                        .iter()
                        .filter(|(_, s, e)| *s <= pw && pw < *e)
                        .map(|(k, s, _)| (*k, (s * 1_000_000.0) as i64))
                        .collect();
                    if let Some(tx) = &tx {
                        for (k, _) in sounding.difference(&current) {
                            let _ = tx.send(SynthCmd::NoteOff(*k));
                        }
                        for (k, _) in current.difference(&sounding) {
                            let _ = tx.send(SynthCmd::NoteOn(*k));
                        }
                    }
                    sounding = current;
                } else if !sounding.is_empty() {
                    if let Some(tx) = &tx {
                        let _ = tx.send(SynthCmd::AllOff);
                    }
                    sounding.clear();
                }
            }
        }
    });

    // --- Handlers ---
    let w_folder = window.clone();
    let open_folder = move |_| {
        let w = w_folder.clone();
        spawn(async move {
            if let Some(dir) = rfd::AsyncFileDialog::new().set_parent(&w.window).pick_folder().await {
                let path = dir.path().to_path_buf();
                let files = scan_klav(&path);
                status.set(format!("{} arquivo(s) .klav em {}", files.len(), path.display()));
                project_dir.set(Some(path));
                project_files.set(files);
            }
        });
    };

    let w_save = window.clone();
    let save = move |_| {
        let a = *active.peek();
        let existing = tabs.read().get(a).and_then(|t| t.path.clone());
        if let Some(path) = existing {
            let score = tabs.read()[a].score.clone();
            match klav_io::save_to_path(&score, &path) {
                Ok(()) => status.set(format!("salvo: {}", path.display())),
                Err(e) => status.set(format!("erro ao salvar: {e}")),
            }
        } else {
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
                    let a = *active.peek();
                    let Some(score) = tabs.read().get(a).map(|t| t.score.clone()) else {
                        return;
                    };
                    match klav_io::save_to_path(&score, &path) {
                        Ok(()) => {
                            if let Some(t) = tabs.write().get_mut(a) {
                                t.path = Some(path.clone());
                            }
                            status.set(format!("salvo: {}", path.display()));
                        }
                        Err(e) => status.set(format!("erro ao salvar: {e}")),
                    }
                }
            });
        }
    };

    let up = move |_| transpose_active(tabs, active, true);
    let down = move |_| transpose_active(tabs, active, false);

    let play_pause = move |_| {
        let is_playing = *playing.peek();
        if !is_playing && *position.peek() >= *total_secs.peek() {
            position.set(0.0);
        }
        playing.set(!is_playing);
    };
    let stop = move |_| {
        playing.set(false);
        position.set(0.0);
    };

    let rt_sel = runtime.clone();
    let change_device = move |evt: Event<FormData>| {
        let name = evt.value();
        device_name.set(name.clone());
        if let Some(dev) = synth::device_by_name(&name) {
            *rt_sel.borrow_mut() = SynthRuntime::on_device(&dev);
        }
    };

    // --- Dados para render ---
    let lay_opt: Option<StaffLayout> = layout.read().clone();
    let sel = tabs.read().get(active()).and_then(|t| t.selected);
    let title = tabs.read().get(active()).map(|t| t.title()).unwrap_or_else(|| "—".into());
    let bpm = tempo();
    let glyph_count = lay_opt.as_ref().map(|l| l.glyphs.len()).unwrap_or(0);

    let pos_secs = position();
    let total = total_secs();
    let pos_whole = pos_secs * bpm as f64 / 240.0;
    let cursor_x = px_x(MARGIN + CLEF_WIDTH + SPACE_PER_WHOLE * pos_whole);
    let show_cursor = playing() || pos_secs > 0.0;

    let active_keys: HashSet<u8> = {
        let iv = intervals.read();
        iv.iter()
            .filter(|(_, s, e)| *s <= pos_whole && pos_whole < *e)
            .map(|(k, _, _)| *k)
            .collect()
    };
    let (whites, blacks, kb_width) = keyboard_geometry();

    let dir_label = project_dir
        .read()
        .as_ref()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "nenhuma pasta".into());

    rsx! {
        style { {css} }
        div { class: "app",
            // Barra superior
            div { class: "titlebar",
                span { class: "brand", "klav" }
                button { class: "btn", onclick: open_folder, "Abrir pasta" }
                button { class: "btn", onclick: save, "Salvar" }
                span { class: "divider" }
                button { class: "btn", disabled: sel.is_none(), onclick: up, "♪ ↑" }
                button { class: "btn", disabled: sel.is_none(), onclick: down, "♪ ↓" }
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

            // Corpo: explorer | centro | mixer
            div { class: "body",
                // Explorer
                div { class: "explorer",
                    div { class: "panel-head", "EXPLORER" }
                    div { class: "dir", "📁 {dir_label}" }
                    for p in project_files.read().iter() {
                        {
                            let path = p.clone();
                            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                            rsx! {
                                div {
                                    class: "file",
                                    onclick: move |_| open_file(tabs, active, playing, position, status, path.clone()),
                                    "🎵 {name}"
                                }
                            }
                        }
                    }
                    if project_files.read().is_empty() {
                        div { class: "hint", "Abra uma pasta para listar os .klav" }
                    }
                }

                // Centro: abas + partitura + teclado
                div { class: "center",
                    div { class: "tabs",
                        for (i, tab) in tabs.read().iter().enumerate() {
                            {
                                let is_active = i == active();
                                let t_title = tab.title();
                                rsx! {
                                    div {
                                        class: if is_active { "tab active" } else { "tab" },
                                        onclick: move |_| {
                                            active.set(i);
                                            playing.set(false);
                                            position.set(0.0);
                                        },
                                        span { "{t_title}" }
                                        span {
                                            class: "close",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                close_tab(tabs, active, playing, position, i);
                                            },
                                            "×"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "canvas",
                        if let Some(lay) = lay_opt.clone() {
                            svg { width: "{px_x(lay.width) + LEFT}", height: "{px_y(9.0)}",
                                for i in 0..lay.line_count {
                                    line {
                                        x1: "{px_x(0.0)}", y1: "{px_y(i as f64)}",
                                        x2: "{px_x(lay.width)}", y2: "{px_y(i as f64)}",
                                        stroke: "#333", stroke_width: "1",
                                    }
                                }
                                for g in lay.glyphs.iter() {
                                    {
                                        let ch = g.glyph.codepoint();
                                        let x = px_x(g.pos.x);
                                        let y = px_y(g.pos.y);
                                        let nid = g.note_id;
                                        let is_sel = nid.is_some() && sel == nid;
                                        let fill = if is_sel { "crimson" } else { "#111" };
                                        rsx! {
                                            text {
                                                x: "{x}", y: "{y}",
                                                font_family: "Bravura",
                                                font_size: "{SP_PX * 4.0}",
                                                fill: "{fill}",
                                                style: if nid.is_some() { "cursor: pointer;" } else { "" },
                                                onclick: move |_| {
                                                    if let Some(id) = nid {
                                                        let a = *active.peek();
                                                        if let Some(t) = tabs.write().get_mut(a) {
                                                            t.selected = Some(id);
                                                        }
                                                    }
                                                },
                                                "{ch}"
                                            }
                                        }
                                    }
                                }
                                if show_cursor {
                                    line {
                                        x1: "{cursor_x}", y1: "{px_y(-1.0)}",
                                        x2: "{cursor_x}", y2: "{px_y(5.0)}",
                                        stroke: "crimson", stroke_width: "1.5",
                                    }
                                }
                            }
                        } else {
                            div { class: "welcome", "Nenhum arquivo aberto — abra uma pasta e clique num .klav" }
                        }
                    }

                    div { class: "keyboard",
                        svg {
                            width: "100%", height: "70",
                            view_box: "0 0 {kb_width} 70",
                            preserve_aspect_ratio: "none",
                            for (midi, x) in whites.iter() {
                                rect {
                                    x: "{x}", y: "0", width: "{WHITE_W - 1.0}", height: "70", rx: "2",
                                    fill: if active_keys.contains(midi) { "crimson" } else { "#fff" },
                                    stroke: "#333", stroke_width: "1",
                                }
                            }
                            for (midi, x) in blacks.iter() {
                                rect {
                                    x: "{x}", y: "0", width: "{BLACK_W}", height: "44", rx: "1",
                                    fill: if active_keys.contains(midi) { "crimson" } else { "#111" },
                                }
                            }
                        }
                    }
                }

                // Mixer (placeholder)
                div { class: "mixer",
                    div { class: "panel-head", "MIXER" }
                    div { class: "channel",
                        div { class: "ch-name", "Pauta 1" }
                        div { class: "ch-btns",
                            button { class: "mini", disabled: true, "M" }
                            button { class: "mini", disabled: true, "S" }
                        }
                        input { r#type: "range", min: "0", max: "100", value: "80", disabled: true }
                    }
                    div { class: "hint", "controles em breve" }
                }
            }

            // Barra de status
            div { class: "statusbar",
                span { "{title}" }
                span { "• {bpm} BPM" }
                span { "• {glyph_count} glifos" }
                span { "• ⏱ {pos_secs:.1}s / {total:.1}s" }
                span { class: "sel",
                    if let Some(id) = sel { "• nota id {id}" } else { "• clique numa nota" }
                }
            }

            if !status.read().is_empty() {
                div { class: "toast", "{status}" }
            }
        }
    }
}

/// Abre um `.klav` numa aba (ou ativa a existente, se já aberto).
fn open_file(
    mut tabs: Signal<Vec<Tab>>,
    mut active: Signal<usize>,
    mut playing: Signal<bool>,
    mut position: Signal<f64>,
    mut status: Signal<String>,
    path: PathBuf,
) {
    match klav_io::load_from_path(&path) {
        Ok(s) => {
            let existing = tabs.read().iter().position(|t| t.path.as_deref() == Some(path.as_path()));
            match existing {
                Some(i) => active.set(i),
                None => {
                    tabs.write().push(Tab { path: Some(path.clone()), score: s, selected: None });
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
fn close_tab(
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

/// Move a nota selecionada da aba ativa um passo diatônico.
fn transpose_active(mut tabs: Signal<Vec<Tab>>, active: Signal<usize>, up: bool) {
    let a = *active.peek();
    let mut w = tabs.write();
    if let Some(tab) = w.get_mut(a) {
        if let Some(id) = tab.selected {
            transpose_in_score(&mut tab.score, id, up);
        }
    }
}

fn transpose_in_score(score: &mut Score, id: u64, up: bool) {
    for m in score.measures.iter_mut() {
        for v in m.voices.iter_mut() {
            for cr in v.elements.iter_mut() {
                for n in cr.notes.iter_mut() {
                    if n.id.value() == id {
                        n.pitch = step_pitch(n.pitch, up);
                    }
                }
            }
        }
    }
}

/// Varre uma pasta (até 4 níveis) por arquivos `.klav`.
fn scan_klav(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    scan_into(dir, &mut out, 0);
    out.sort();
    out
}

fn scan_into(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 4 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            scan_into(&p, out, depth + 1);
        } else if p.extension().map(|e| e.eq_ignore_ascii_case("klav")).unwrap_or(false) {
            out.push(p);
        }
    }
}

/// Score de exemplo: escala de Dó subindo em dois compassos 4/4.
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
            blacks.push((midi, wx - BLACK_W / 2.0));
        } else {
            whites.push((midi, wx));
            wx += WHITE_W;
        }
    }
    (whites, blacks, wx)
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
        html, body {{ margin: 0; height: 100%; }}
        .app {{
            font-family: 'Segoe UI', sans-serif; color: #111;
            display: grid; grid-template-rows: auto 1fr auto; height: 100vh;
        }}
        .titlebar {{
            background: #1e1e2e; color: #cdd6f4; padding: 8px 14px;
            display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
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
            padding: 5px 8px; font-size: 12px; max-width: 200px; cursor: pointer;
        }}
        .body {{
            display: grid; grid-template-columns: 220px 1fr 200px;
            min-height: 0; overflow: hidden;
        }}
        .explorer, .mixer {{
            background: #252537; color: #cdd6f4; overflow-y: auto; font-size: 13px;
        }}
        .panel-head {{
            padding: 8px 12px; font-size: 11px; letter-spacing: 1px; color: #9399b2;
            border-bottom: 1px solid #1e1e2e;
        }}
        .dir {{ padding: 6px 12px; color: #a6adc8; font-size: 12px; }}
        .file {{ padding: 5px 12px 5px 18px; cursor: pointer; }}
        .file:hover {{ background: #313145; }}
        .hint {{ padding: 10px 12px; color: #6c7086; font-size: 11px; font-style: italic; }}
        .center {{ display: flex; flex-direction: column; min-width: 0; min-height: 0; }}
        .tabs {{ display: flex; background: #e9e9ef; border-bottom: 1px solid #ddd; overflow-x: auto; }}
        .tab {{
            display: flex; align-items: center; gap: 8px; padding: 6px 12px;
            border-right: 1px solid #ddd; cursor: pointer; font-size: 13px; color: #555;
            white-space: nowrap;
        }}
        .tab.active {{ background: #fff; color: #111; font-weight: 600; }}
        .tab .close {{ color: #999; font-size: 15px; line-height: 1; }}
        .tab .close:hover {{ color: crimson; }}
        .canvas {{
            flex: 1; padding: 16px; overflow: auto; background: #fff;
            display: flex; justify-content: safe center; align-items: flex-start;
        }}
        .welcome {{ color: #888; padding: 40px; font-style: italic; }}
        .keyboard {{ padding: 8px 16px; background: #faf8f5; border-top: 1px solid #ddd; overflow: hidden; }}
        .keyboard svg {{ display: block; }}
        .mixer .channel {{ padding: 10px 12px; }}
        .mixer .ch-name {{ font-size: 12px; margin-bottom: 6px; }}
        .mixer .ch-btns {{ display: flex; gap: 6px; margin-bottom: 8px; }}
        .mini {{
            background: #45475a; color: #cdd6f4; border: none; border-radius: 3px;
            width: 26px; height: 24px; font-size: 12px;
        }}
        .mixer input[type=range] {{ width: 100%; }}
        .statusbar {{
            background: #1e1e2e; color: #bac2de; padding: 5px 14px; font-size: 12px;
            display: flex; gap: 10px;
        }}
        .statusbar .sel {{ margin-left: auto; color: #f38ba8; }}
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
