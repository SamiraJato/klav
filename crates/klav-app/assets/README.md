# Assets do klav-app

## Bravura.woff2
Fonte musical **Bravura** (padrão SMuFL), por Steinberg Media Technologies GmbH.
- Origem: https://github.com/steinbergmedia/bravura (`redist/woff/Bravura.woff2`)
- Licença: **SIL Open Font License 1.1** (ver `OFL.txt`), com Reserved Font Name "Bravura".
- Uso no klav: embarcada no binário (`include_bytes!`) e injetada como `@font-face`
  (data URL) para renderizar os glifos da partitura.
