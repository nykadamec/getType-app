# Pill pulse — plán (doplněk k Escape-cancel)

## Cíl
Aktivní pilulka (stav recording) má lehký, minimální efekt pulzování — jen náznak života, ne agresivní animace.

## Rozsah
- Pouze design/animace, žádná změna logiky nahrávání.
- Soubor: `src/pill.css` (případně čtení `src/pill.html`, `src/pill.js` pro správný selektor, ale zápis jen do CSS).
- Styl: light minimal, monochrome — pulz přes opacity/scale v jednotkách procent, krátká smyčka, respektovat `prefers-reduced-motion` pokud dává smysl.

## Nesmí
- Neměnit Rust backend, hotkey, STT ani output.
- Neměnit `pill.js` eventy (vlastní je Escape-cancel lane).

## Verifikace
- Vizuální kontrola vedle 06/07/08 onboardingu + nahrávací pilulky.
- Žádný clipped layout, kontrast OK.

## Doplnění uživatele (2026-09-09)
- Fluidní, vodní pulzování s proměnlivými rozměry, pořád lehce a minimálně; zkusit knihovnu motion (`motion` v dependencies, `src/vendor/`).
- Efekt má sedět v pozadí mini pilulky (za ní), lehce a minimálně.
- Stará session zamčená → restart v nové session, sloučeno do jednoho zadání.
- Uživatel: pulz skáče (0→100, skok na 0). Oprava: směr `alternate` (ping-pong 0→100→0), ať dýchá plynule.
- Uživatel: pilulka při stop→transcribing problikne (zmizí/znovu najede). Oprava: `fadeIn` v `pill.js` nepřhrávat dissolve, když je pilulka už viditelná — jen vyměnit obsah.
- Uživatel: trvalý `box-shadow` pryč — jen čistá bílá pilulka.
- Uživatel: přidat bílé ohraničení + lehký bílý blur kolem pilulky (@designer).
- Obě části patří k jednomu celkovému úkolu (viz `plan-escape-cancel.md`).
- Write-scope oddělený: Escape-cancel vlastní `*.rs` + `pill.js` event, Pulse vlastní pouze `pill.css`.
