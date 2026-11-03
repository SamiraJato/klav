//! Aparência: CSS do app e a fonte musical embarcada.
//!
//! Isolado da composição da UI para manter `main.rs` focado no layout dos componentes.

/// Fonte Bravura (SMuFL, SIL OFL) embarcada no binário.
const BRAVURA_WOFF2: &[u8] = include_bytes!("../assets/Bravura.woff2");

/// Monta o CSS do app, embutindo a Bravura como data URL base64.
pub fn build_css() -> String {
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
        .btn.dur {{ min-width: 30px; text-align: center; }}
        .btn.dur.active {{ background: #89b4fa; color: #1e1e2e; font-weight: 700; }}
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
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64_encode;

    #[test]
    fn base64_casos_conhecidos() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
