# Settings v docku — plán

## Cíl
Když se otevře Settings panel, aplikace je viditelná v docku. Po zavření/skrytí se vrací do menu-bar-only režimu.

## Současný stav
- `src-tauri/src/lib.rs:79-80` — v `setup()` se nastavuje `ActivationPolicy::Accessory` (bez dock ikony, jen tray).
- Tray menu `lib.rs:200-206` — „settings" jen `show()` + `set_focus()`.
- `lib.rs:215-223` — zavření Settings se zachytí (`CloseRequested → prevent_close + hide`), appka žije v trayi.
- Tauri 2 `AppHandle::set_activation_policy` existuje (použito v setupu) — mělo by jít volat i za běhu.

## Návrh řešení
1. Při otevření Settings (tray menu „settings"): `app.set_activation_policy(Regular)` + `show()` + `set_focus()` (+ případně `set_dock_visibility` ekvivalent dle API).
2. Při skrytí/zavření Settings (`CloseRequested` handler): `hide()` + zpět `ActivationPolicy::Accessory`.
3. Pozor: přepnutí na Regular aktivuje appku (může vzít focus) — přijatelné, protože uživatel právě otevřel Settings. Přepnutí zpět nesmí nechat viset dock ikonu.
4. Ověřit na macOS 13+: tray ikona zůstává, dock ikona se objeví/zmizí s oknem.

## Varianty (rozhodnuto)
- A: Přepínat tam a zpět (dock jen když je Settings viditelné). ← vybráno
- B: Po prvním otevření už dock nechat.

## Rozhodnutí uživatele (2026-09-09)
- Přepínat tam a zpět: dock ikona jen když je Settings viditelné.
- Stav: schváleno 2026-09-09, realizace běží.
- A: Přepínat tam a zpět (dock jen když je Settings viditelné).
- B: Po prvním otevření už dock nechat (jednodušší, ale dock zůstává i po zavření).

## Implementační kroky (až po schválení)
1. `src-tauri/src/lib.rs` — policy switch v menu handleru + window eventu.
2. `cargo check`, manuální test: otevřít Settings → dock ikona je, zavřít → dock ikona pryč, tray žije.

## Verifikace
- Build projde, manuální test na macOS (dock se objeví/zmizí, focus chování OK).

## Poznámka k ostatním běžícím věcem
- Escape-cancel (hotovo, necommitnuto v pracovním stromu) ani pill pulse (čeká na designéra) se tímto nemění.
