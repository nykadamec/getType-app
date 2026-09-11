# Dark mode + přepínač theme — plán (ke schválení, NEIMPLEMENTOVAT bez souhlasu)

## Cíl
Přepínač vzhledu v Settings → General (Light / Dark / System) + tmavé téma pro celou aplikaci podle návrhu v `gettype.pen` (09 Settings — Dark, 10 Pill — Dark + tabulka tokenů).

## Současný stav
- Design dark varianty hotový v Pencilu, kód jen light.
- Barvy natvrdo v `settings.css`, `pill.css`, `popover.css`, `onboarding.css` (žádné CSS proměnné pro theme).
- `Config` v `settings.rs` nemá `theme` (serde `default` → nutno přidat s defaultem).
- Okna: settings, pill, popover, onboarding — každé vlastní HTML/CSS/JS.

## Návrh řešení
1. `Config.theme: String` (`light`/`dark`/`system`, default `system`), `settings.rs` + uložení přes existující `save_config`.
2. CSS každého okna: barvy převést na proměnné (`:root` light hodnoty = současnost) + `documentElement[data-theme="dark"]` přepis dle token tabulky z návrhu (bg #101010, surface #1A1A1A, border #2A2A2A, text #F5F5F5, CTA bílé…).
3. Každé okno při startu: načíst `theme` z `load_settings`, `system` vyhodnotit přes `matchMedia("(prefers-color-scheme: dark)")` (live listener na změnu systému).
4. Settings → General: segmented Light/Dark/System → uložit + event `theme-changed`, ostatní okna (pill, popover, onboarding) ho poslouchají a přepnou okamžitě bez restartu.
5. Ověřit kontrast footnote/hint textů a CTA hierarchii dle checklistu z návrhu.

## Rozhodnutí uživatele (schváleno)
- Rozsah: všechna 4 okna (settings, pill, popover, onboarding).
- Volby: Light / Dark / System; default pro nové instalace: System.
- Kontrakt: `Config.theme: String` (`light|dark|system`, default `system`); frontend čte přes `load_settings`, změna eventem `theme-changed`.
- Stav: realizace běží (designér CSS/UI + fixer `settings.rs` paralelně).

## Verifikace
- Přepnout v General → všechna okna hned tmavá/světlá; restart drží volbu; System reaguje na změnu macOS vzhledu.
- `cargo check` (jen kdyby se sahalo do Rustu — návrh počítá čistě s frontendem + `theme` polem v Configu).
