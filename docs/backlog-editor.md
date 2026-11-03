# Backlog do editor (a arrumar)

Itens conhecidos, anotados para resolver depois. Não bloqueiam o fluxo atual.

## Layout

- [ ] **Linhas suplementares limitadas.** Notas muito acima/abaixo da pauta não aparecem
  (nem as cabeças, nem as suplementares). Permitir pelo menos **4 linhas suplementares** para
  cima e para baixo. Investigar: `klav_layout::vertical::note_y` pode estar retornando `None`
  fora de uma faixa, e/ou a altura/overflow do SVG no `klav-app` corta o que passa da pauta.

## Edição / UX

- [ ] **Compasso incompleto em vermelho.** Destacar (ex.: barra/fundo vermelho) o compasso cuja
  soma de durações da voz é menor que a fórmula de compasso, para sinalizar que falta preencher.
- [ ] **Acidentes por teclado não funcionam.** Teclas `#` / `+` (sustenido), `-` (bemol) e `n`
  (natural) não estão aplicando o acidente à nota selecionada. Verificar como o `evt.key()` do
  Dioxus entrega esses caracteres (layout de teclado pt-BR, `Key::Character`), já que `#` e `+`
  costumam exigir modificadores.
