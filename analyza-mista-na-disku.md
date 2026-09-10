# Analýza místa na disku – /Users/nykadamec

> Datum: 2026-09-10
> Stav disku: **228 GB celkem, 176 GB použito, 6,6 GB volných (97 % zaplněno)**
> Velikost home: **107 GB**
> Účel: zjistit, co zabírá místo a co lze smazat. **Nic nebylo smazáno.**

---

## 1. Přehled podle oblastí

| Oblast | Velikost | Poznámka |
|---|---:|---|
| `~/Library` | 55 GB | aplikační data, cache, kontejnery |
| `~/Documents` | 24 GB | z toho 23 GB `New project/projects` |
| skryté složky v home (`~/.xxx`) | ~19 GB | cache nástrojů, toolchainy |
| `~/Downloads` | 3,8 GB | převážně instalační `.dmg` |
| `~/Pictures` | 2,6 GB | Photos Library 2,4 GB |
| `~/Music` | 171 MB | |
| `~/Movies` | 149 MB | |
| `~/Desktop` | 74 MB | |
| **CELKEM home** | **~107 GB** | |

---

## 2. Kategorie „bezpečně smazatelné" (regenerovatelné cache a buildy)

| Co | Cesta | Velikost | Jak vrátit / riziko |
|---|---|---:|---|
| Rust debug build gettype | `~/Documents/New project/projects/gettype/src-tauri/target` | **8,2 GB** | `cargo build` znovu; **nulové riziko** |
| bun cache | `~/.bun/install/cache` | 2,9 GB | `bun pm cache rm`; stáhne se znovu |
| Homebrew download cache | `~/Library/Caches/Homebrew/downloads` | 1,8 GB | `brew cleanup -s` |
| pnpm store | `~/Library/pnpm/store` | 2,1 GB | `pnpm store prune` |
| opencode plugin cache | `~/.cache/opencode/packages` | 1,9 GB | stáhne se znovu při použití |
| npm `_npx` | `~/.npm/_npx` | 1,4 GB | npx si stáhne znovu |
| ms-playwright | `~/Library/Caches/ms-playwright` | 1,3 GB | jen pokud nepoužíváš Playwright |
| Xcode DerivedData | `~/Library/Developer/Xcode/DerivedData` | 1,0 GB | regeneruje Xcode |
| VSCode ShipIt | `~/Library/Caches/com.microsoft.VSCode.ShipIt` | 932 MB | aktualizační cache |
| npm `_cacache` | `~/.npm/_cacache` | 716 MB | `npm cache clean --force` |
| pen updater | `~/Library/Caches/pen-updater` | 716 MB | aktualizační cache |
| pip cache | `~/Library/Caches/pip` | 644 MB | `pip cache purge` |
| CoreSimulator zařízení | `~/Library/Developer/CoreSimulator/Devices` | 563 MB | jen pokud nepoužíváš simulátor |
| puppeteer cache | `~/.cache/puppeteer` | 539 MB | stáhne se znovu |
| electron cache | `~/Library/Caches/electron` | 442 MB | stáhne se znovu |
| comfyui updater | `~/Library/Caches/comfyui-desktop-2-updater` | 320 MB | aktualizační cache |
| opencode desktop updater | `~/Library/Caches/@opencode-aidesktop-updater` | 271 MB | aktualizační cache |
| **Mezisoučet B** | | **~28 GB** | |

### Pozor u těchto (nejsou stoprocentně „cache")
- `~/.rustup/toolchains` – **1,3 GB** – nutné pro build Rustu (gettype). Nemazat, pokud vyvíjíš Rust.
- `~/.cargo/registry` – **1,2 GB** – cache crates; regeneruje se, ale build bude pomalejší.
- `~/.local/share/opencode/worktree` – **1,2 GB** – git worktrees z opencode session; **může obsahovat necommitnutou práci**, nejdřív zkontrolovat.
- `~/.local/share/opencode/opencode.db` – 467 MB – databáze historií; nemazat bez rozmyslu.

---

## 3. Velké modely a data aplikací (rozhodnout podle používání)

| Co | Cesta | Velikost | Poznámka |
|---|---|---:|---|
| **Draw Things** | `~/Library/Containers/com.liuliu.draw-things` | **17 GB** | 13 GB AI modely + 3,3 GB `Untitled-79182.sqlite3` (projekt/DB). Pokud appku nepoužíváš, největší úspora. |
| Comet (Perplexity browser) | `~/Library/Application Support/Comet` | 3,6 GB | profil 2,6 GB; smazat jen s odinstalací prohlížeče |
| superwhisper modely | ~~2,6 GB~~ → **SMAZÁNO 2026-09-10** (zbylo 41 MB: database, agent, vad/seg/emb onnx) | uvolněno ~2,6 GB |
| Comet cache | `~/Library/Caches/Comet` | 1,2 GB | bezpečnější než profil |
| VidCap projekty (Setapp) | `~/Library/Containers/io.fadel.VidCap-setapp` | 1,8 GB | 2× velké `video.mp4` – smazat staré projekty |
| openchamber speech modely | `~/.config/openchamber` | 640 MB | TTS/STT modely |
| Photos Library | `~/Pictures/Photos Library.photoslibrary` | 2,4 GB | osobní fotky – nemanipulovat ručně |
| DrawThingsLoRASorter | `~/Downloads/DrawThingsLoRASorter` | 426 MB | + různé `.safetensors` |

---

## 4. Instalátory a stažené soubory (`~/Downloads` = 3,8 GB)

Převážně jednorázové instalační balíčky, které se dají smazat:

| Soubor | Velikost |
|---|---:|
| `Pen-mac-arm64.dmg` | 354 MB |
| `Cherry-Studio-2.0.9-arm64.dmg` | 339 MB |
| `OpenCode Desktop (1).dmg` | 220 MB |
| `Krea 2 - Cameltoe v3.safetensors` | 219 MB |
| `Krea_Amateur_V4.safetensors` | 218 MB |
| `CodeBuddy-darwin-arm64-4.12.0…dmg` | 187 MB |
| `OpenCode Desktop.dmg` | 144 MB |
| `Zed-aarch64.dmg` | 141 MB |
| `Grok_Bot_0.44.0.dmg` | 135 MB |
| `Krea2Weight_v3.safetensors` | 102 MB |
| + další desítky `.dmg`/`.zip` | … |

---

## 5. Projekty – `~/Documents/New project/projects` (23 GB)

Největší projekt je `gettype` (8,2 GB, celé v `src-tauri/target` = kategorie výše).

### `node_modules` napříč projekty (~vybráno, celkem cca 6 GB)
| Projekt | node_modules |
|---|---:|
| opencodev2-adapter | 823 MB |
| krea2 | 733 MB |
| opendraw/frontend | 584 MB |
| memu_dev | 503 MB |
| Prompt-Recreator | 396 MB |
| zit | 395 MB |
| getprompts | 328 MB |
| openclaw_ui_dev | 275 MB |
| iprompt | 243 MB |
| opendraw/proxy | 237 MB |
| … (~20 dalších) | … |

> `node_modules` lze kdykoli smazat a obnovit přes `npm/pnpm install`.

### Možné duplicity / zálohy
- `zit` (395 MB) vs `zit 2` (176 MB) vs `zit.zip` (131 MB) vs `zit-enhance` (76 MB)
- `openmail` (599 MB) vs `openmail-0.0.29` (115 MB)
- `mes2` vs `mes2.BACKUP_20260427_112044` (150 MB)
- `opendraw` vs `opendraw-bun` vs `opendr`
- `~/.config/opencode copy` (94 MB), `~/.config/opencode.zip` (22 MB)

---

## 6. Shrnutí – kolik se dá uvolnit

| Scénář | Úspora |
|---|---:|
| Jen build + cache (kategorie 2) | **~28 GB** |
| + Downloads instalátory | ~31 GB |
| + node_modules napříč projekty | ~37 GB |
| + Draw Things (pokud nepoužíváš) | ~54 GB |
| + Comet cache/profil, superwhisper modely | ~60 GB |

**Doporučený první krok (bez rizika, ~28 GB):** kategorie 2 (buildy + cache).
**Největší jednotlivá položka k rozhodnutí:** Draw Things 17 GB.

---

## 7. Poznámky
- Vše v kategorii 2 je regenerovatelné – smazání pouze zpomalí příští build/instalaci.
- `~/.local/share/opencode/worktree` před smazáním zkontrolovat (může držet neuloženou práci).
- Nic z výše uvedeného zatím **nebylo** smazáno.
