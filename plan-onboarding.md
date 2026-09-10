# Onboarding (první spuštění) — plán

## Cíl
Při prvním spuštění provést uživatele: Welcome → API klíč → oprávnění (mikrofon + Accessibility). Design hotový v `gettype.pen` (06 Welcome, 07 API key, 08 Permissions).

## Současný stav
- `permissions.rs` umí jen ČÍST stav (mic `authorizationStatus`, accessibility `AXIsProcessTrusted`) — neumí vyvolat systémové prompty.
- `settings.rs` `Config` nemá `onboarded` flag; detekce prvního spuštění = **chybějící `config.json`** (existující uživatelé soubor mají → onboarding se jim neukáže).
- Žádné onboarding okno neexistuje; `lib.rs` vytváří jen `settings` + `pill`.
- API key flow existuje (`save_api_key`, `verify_api_key`) — znovu použít.

## Návrh řešení (varianta A — samostatné okno dle designu)
1. Nové okno `onboarding` (`src/onboarding.html/js/css`, 3 kroky dle `.pen` stylu, Continue sjednocené Y=389).
2. `lib.rs` setup: pokud `config.json` neexistuje → otevřít `onboarding` místo tichého startu (dock ikona během onboardingu jako u Settings).
3. Krok API key: stejné commandy jako Settings (`save_api_key` + `verify_api_key`).
4. Krok Permissions:
   - Mikrofon: nový command `request_mic_permission` (`requestAccessForMediaType`, objc2-av-foundation už v dependencích) → systémový prompt.
   - Accessibility: `AXIsProcessTrustedWithOptions` s prompt flagem (otevře systémový dialog → System Settings); tlačítko „Zkontrolovat znovu" čte stav přes existující command.
5. Dokončení: uložit `config.json` (tím se označí hotovo) + zavřít okno + návrat do Accessory režimu.
6. Stav kroků (Step 1/2/3) + Back navigace; texty ponechat stručné jako v designu.

## Varianta B (fallback)
První spuštění otevře existující Settings s průvodcem — méně práce, ale neodpovídá schválenému designu.

## Verifikace
- Smazat `config.json` → start → onboarding se ukáže; projít kroky; restart → už se neukáže.
- Mic Denied path: návod kam do System Settings.
- `cargo check` + manuální test na macOS.
