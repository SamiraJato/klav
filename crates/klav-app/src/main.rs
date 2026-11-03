//! klav-app — Fatia 3a: esqueleto de IDE (explorer de pasta, abas, mixer placeholder,
//! status bar) sobre o render/playback das fatias anteriores.
//!
//! Frontend em Dioxus (Rust/WASM), rodando como janela desktop via webview nativo.

mod document;
mod edit;
mod keyboard;
mod project;
mod synth;
mod theme;

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use dioxus::desktop::{use_window, Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

use document::{close_tab, open_file, Tab};
use edit::{
    add_measure_active, add_note_with_active, add_rest_with_active, delete_selected_active,
    duration_base_from_key, navigate_active, pick_duration, set_accidental_active, set_step_active,
    transpose_active,
};
use keyboard::{keyboard_geometry, BLACK_W, WHITE_W};
use project::scan_klav;
use synth::{SynthCmd, SynthRuntime};
use theme::build_css;

use klav_core::{Accidental, DurationBase};
use klav_engine::{key_intervals, render_events, total_duration_whole, whole_to_seconds};
use klav_layout::{layout_single_staff, StaffLayout, MAX_SYSTEM_WIDTH};

/// Pixels por staff-space. No SMuFL, 1 em da fonte = 4 staff-spaces.
const SP_PX: f64 = 10.0;
const LEFT: f64 = 16.0;
const TOP: f64 = 40.0;

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

    // Duração ativa para a "escrita" de novas notas (barra de espaço / botões / 1–5).
    let current_dur = use_signal(|| DurationBase::Quarter);

    // Áudio.
    let runtime = use_hook(|| Rc::new(RefCell::new(SynthRuntime::default_output())));
    let devices = use_hook(synth::list_output_devices);
    let mut device_name = use_signal(|| synth::default_output_name().unwrap_or_default());

    // Derivados da aba ativa (None se não houver aba).
    let active_score = use_memo(move || tabs.read().get(active()).map(|t| t.score.clone()));
    let tempo = use_memo(move || {
        active_score
            .read()
            .as_ref()
            .map(|s| s.tempo_bpm)
            .unwrap_or(120)
    });
    let layout = use_memo(move || {
        active_score
            .read()
            .as_ref()
            .map(|s| layout_single_staff(s, MAX_SYSTEM_WIDTH))
    });
    // Intervalos (tecla, início, fim) em semibreves — pareados no engine, preservando
    // instâncias simultâneas da mesma tecla.
    let intervals = use_memo(move || {
        active_score
            .read()
            .as_ref()
            .map(|s| {
                key_intervals(&render_events(s))
                    .into_iter()
                    .map(|iv| (iv.key, iv.start.to_f64(), iv.end.to_f64()))
                    .collect::<Vec<(u8, f64, f64)>>()
            })
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
            if let Some(dir) = rfd::AsyncFileDialog::new()
                .set_parent(&w.window)
                .pick_folder()
                .await
            {
                let path = dir.path().to_path_buf();
                let files = scan_klav(&path);
                status.set(format!(
                    "{} arquivo(s) .klav em {}",
                    files.len(),
                    path.display()
                ));
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

    let add_note = move |_| add_note_with_active(tabs, active, status, *current_dur.peek());
    let add_rest = move |_| add_rest_with_active(tabs, active, status, *current_dur.peek());
    let add_meas = move |_| add_measure_active(tabs, active, status);
    let delete = move |_| delete_selected_active(tabs, active, status);

    // Entrada por teclado (step-time): ←/→ navegam, espaço escreve nota com a duração
    // ativa, 1–5 escolhem a duração ativa (e aplicam à nota selecionada), ↑/↓ transpõem,
    // A–G altura, #/+ e - acidentes, n natural, 0/R pausa, Del/Backspace apaga.
    let on_key = move |evt: KeyboardEvent| match evt.key() {
        Key::ArrowRight => navigate_active(tabs, active, true),
        Key::ArrowLeft => navigate_active(tabs, active, false),
        Key::ArrowUp => transpose_active(tabs, active, true),
        Key::ArrowDown => transpose_active(tabs, active, false),
        Key::Delete | Key::Backspace => delete_selected_active(tabs, active, status),
        Key::Character(s) => {
            let k = s.to_lowercase();
            match k.as_str() {
                " " => add_note_with_active(tabs, active, status, *current_dur.peek()),
                "a" | "b" | "c" | "d" | "e" | "f" | "g" => {
                    set_step_active(tabs, active, status, k.chars().next().unwrap())
                }
                "1" | "2" | "3" | "4" | "5" => {
                    if let Some(base) = duration_base_from_key(k.chars().next().unwrap()) {
                        pick_duration(current_dur, tabs, active, status, base);
                    }
                }
                "#" | "+" => set_accidental_active(tabs, active, status, Some(Accidental::Sharp)),
                "-" => set_accidental_active(tabs, active, status, Some(Accidental::Flat)),
                "n" => set_accidental_active(tabs, active, status, None),
                "0" | "r" => add_rest_with_active(tabs, active, status, *current_dur.peek()),
                _ => {}
            }
        }
        _ => {}
    };

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
    let title = tabs
        .read()
        .get(active())
        .map(|t| t.title())
        .unwrap_or_else(|| "—".into());
    let bpm = tempo();
    let glyph_count = lay_opt.as_ref().map(|l| l.glyphs.len()).unwrap_or(0);

    let pos_secs = position();
    let total = total_secs();
    let pos_whole = pos_secs * bpm as f64 / 240.0;
    // Posição do cursor no sistema que contém o instante atual (ou None se não há pauta).
    let cursor = lay_opt.as_ref().and_then(|l| l.cursor_at(pos_whole));
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

    // Durações selecionáveis: (tecla, base, rótulo, nome).
    let durations = [
        ('1', DurationBase::Whole, "1", "Semibreve"),
        ('2', DurationBase::Half, "½", "Mínima"),
        ('3', DurationBase::Quarter, "¼", "Semínima"),
        ('4', DurationBase::Eighth, "⅛", "Colcheia"),
        ('5', DurationBase::Sixteenth, "1/16", "Semicolcheia"),
    ];

    rsx! {
        style { {css} }
        div { class: "app", tabindex: "0", autofocus: true, onkeydown: on_key,
            // Barra superior
            div { class: "titlebar",
                span { class: "brand", "klav" }
                button { class: "btn", onclick: open_folder, "Abrir pasta" }
                button { class: "btn", onclick: save, "Salvar" }
                span { class: "divider" }
                // Seletor de duração ativa (destaca a selecionada).
                for (key , base , label , name) in durations {
                    button {
                        class: if current_dur() == base { "btn dur active" } else { "btn dur" },
                        title: "{name} — tecla {key}",
                        onclick: move |_| pick_duration(current_dur, tabs, active, status, base),
                        "{label}"
                    }
                }
                span { class: "divider" }
                button {
                    class: "btn",
                    title: "Escreve nota com a duração ativa (barra de espaço); ←/→ navega",
                    onclick: add_note,
                    "＋ nota"
                }
                button {
                    class: "btn",
                    title: "Adiciona pausa com a duração ativa (tecla 0 ou R)",
                    onclick: add_rest,
                    "＋ pausa"
                }
                button { class: "btn", title: "Adiciona um compasso", onclick: add_meas, "＋ compasso" }
                button {
                    class: "btn",
                    title: "Apaga a nota selecionada (Del)",
                    disabled: sel.is_none(),
                    onclick: delete,
                    "🗑 Apagar"
                }
                span { class: "divider" }
                button {
                    class: "btn",
                    title: "Transpõe a nota um grau acima (↑); acidentes: #/+ e -",
                    disabled: sel.is_none(),
                    onclick: up,
                    "♪ ↑"
                }
                button {
                    class: "btn",
                    title: "Transpõe a nota um grau abaixo (↓)",
                    disabled: sel.is_none(),
                    onclick: down,
                    "♪ ↓"
                }
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
                            svg { width: "{px_x(lay.width) + LEFT}", height: "{px_y(lay.height)}",
                                // Linhas da pauta de cada sistema.
                                for sys in lay.systems.iter() {
                                    for i in 0..lay.line_count {
                                        line {
                                            x1: "{px_x(0.0)}", y1: "{px_y(sys.y_top + i as f64)}",
                                            x2: "{px_x(sys.width)}", y2: "{px_y(sys.y_top + i as f64)}",
                                            stroke: "#333", stroke_width: "1",
                                        }
                                    }
                                }
                                // Barras de compasso.
                                for b in lay.barlines.iter() {
                                    line {
                                        x1: "{px_x(b.x)}", y1: "{px_y(b.y_top)}",
                                        x2: "{px_x(b.x)}", y2: "{px_y(b.y_top + (lay.line_count - 1) as f64)}",
                                        stroke: "#333", stroke_width: "1",
                                    }
                                }
                                // Hastes das notas.
                                for s in lay.stems.iter() {
                                    line {
                                        x1: "{px_x(s.x)}", y1: "{px_y(s.y_top)}",
                                        x2: "{px_x(s.x)}", y2: "{px_y(s.y_bottom)}",
                                        stroke: "#111", stroke_width: "1.3",
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
                                if let (true, Some((cx, cy))) = (show_cursor, cursor) {
                                    line {
                                        x1: "{px_x(cx)}", y1: "{px_y(cy - 1.0)}",
                                        x2: "{px_x(cx)}", y2: "{px_y(cy + 5.0)}",
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
