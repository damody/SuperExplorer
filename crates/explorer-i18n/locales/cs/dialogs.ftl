# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = Možnosti složky

dialog-about = O aplikaci SuperExplorer

dialog-version = Verze

dialog-build-date = Datum sestavení

dialog-author = Autor

dialog-purpose = Účel: { $value }

dialog-author-line = Autor: { $name } — { $bio } · { $date }

dialog-community = Komunita: { $url }

dialog-plugin-safe-mode-title = Nouzový režim pluginů

dialog-plugin-safe-mode-body = Nouzový režim pluginů je zapnutý. Vyberte pluginy k opětovnému povolení, zvolte Použít nebo OK a restartujte SuperExplorer.

dialog-safe-mode-confirm-title = Nouzový režim vyžaduje potvrzení

dialog-safe-mode-confirm-aria = Je vyžadováno potvrzení nouzového režimu; Podezřelý balíček: { $package }

dialog-suspect-package = Podezřelý balíček: { $package }

dialog-interface = Rozhraní: { $value }

dialog-operation = Operace: { $value }

dialog-confirm-reenable = Potvrdit a znovu povolit

dialog-bookmark-action = Akce záložky

dialog-delete-bookmark = Odstranit záložku

dialog-delete-bookmark-prompt = Odstranit záložku „{ $name }“?

dialog-delete-bookmark-note = Tím se záložka odebere. Soubory na disku se neodstraní.

dialog-delete-bookmark-folder = Odstranit složku záložek

dialog-delete-bookmark-folder-prompt = Odstranit složku záložek „{ $name }“?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] Složka a { $count } položka v ní budou odebrány. Soubory na disku se neodstraní.
        [few] Složka a { $count } položky v ní budou odebrány. Soubory na disku se neodstraní.
        [many] Složka a { $count } položek v ní budou odebrány. Soubory na disku se neodstraní.
       *[other] Složka a { $count } položek v ní budou odebrány. Soubory na disku se neodstraní.
    }

dialog-rename-bookmark-folder = Přejmenovat složku záložek

dialog-bookmark-library = Knihovna

dialog-new-shortcut = Nový zástupce

dialog-new-remote-shortcut = Nový vzdálený zástupce

dialog-shortcut-name = Název zástupce

dialog-shortcut-target = Cílová cesta

dialog-create-remote-shortcut = Vytvořit vzdáleného zástupce

dialog-cancel-new-shortcut = Zrušit nového zástupce

dialog-properties = { $name } - Vlastnosti

dialog-properties-aria = Vlastnosti { $name }

dialog-general = Obecné

dialog-file-type = Typ: { $value }

dialog-location = Umístění: { $value }

dialog-size = Velikost: { $value }

dialog-date-created = Datum vytvoření: { $value }

dialog-date-modified = Datum změny: { $value }

dialog-permissions = Oprávnění: { $mode }

dialog-file-in-use = Soubor se používá

dialog-items-in-use = Některé položky se používají

dialog-lock-owner-body =
    { $count ->
        [one] Windows nemůže odstranit { $count } vybranou položku, protože ji používá jiná aplikace.
        [few] Windows nemůže odstranit { $count } vybrané položky, protože je používá jiná aplikace.
        [many] Windows nemůže odstranit { $count } vybraných položek, protože je používá jiná aplikace.
       *[other] Windows nemůže odstranit { $count } vybraných položek, protože je používá jiná aplikace.
    }

dialog-apps-using-file = Aplikace používající soubor

dialog-close-results = Výsledky ukončení aplikací

dialog-new-bookmark = Nová záložka

dialog-edit-bookmark = Upravit záložku

dialog-name-accelerator = Název (N)

dialog-location-accelerator = Umístění (L)

dialog-url-accelerator = Adresa (L)
dialog-path-accelerator = Path (U)
dialog-tags-accelerator = Tags (T)
dialog-tags-placeholder = Comma-separated tags
dialog-tags-hint = Use tags to search and organize bookmarks

dialog-show-editor-on-save = Zobrazit editor při ukládání (S)

dialog-lua-source = Zdroj Lua (pouze current_folder jen pro čtení)

dialog-folder-path-editable = Cesta ke složce (upravitelná)

dialog-file-path-editable = Cesta k souboru (upravitelná)

dialog-root = Kořen

dialog-folder-options-scrollbar = Svislý posuvník možností složky

dialog-new-bookmark-default = Nová záložka

dialog-new-folder-default = Nová složka

dialog-new-shortcut-default = Nový zástupce

dialog-shortcut-name-invalid = Název zástupce musí být platný název v aktuální složce.

dialog-shortcut-target-invalid = Zadejte platnou cílovou cestu.

dialog-shortcut-window-closed = Hlavní okno se zavřelo, proto se zástupce nepodařilo vytvořit.

dialog-folder-changed = Aktuální složka se změnila. Znovu otevřete okno Nový zástupce.

dialog-remote-unavailable = Vzdálená služba není momentálně k dispozici.

dialog-shortcut-in-progress = Jiný zástupce se již vytváří.

dialog-allowed = Povoleno

dialog-not-allowed = Nepovoleno

dialog-read = Čtení

dialog-write = Zápis

dialog-execute = Spouštění

dialog-owner = Vlastník

dialog-group = Skupina

dialog-others = Ostatní

dialog-remote-folder = Vzdálená složka

dialog-remote-file = Vzdálený soubor

dialog-unavailable = Nedostupné

dialog-bytes = { $value } bajtů

dialog-extension-author-bio = Autor SuperExploreru a oficiálních ukázkových rozšíření

dialog-extension-purpose-folder-size = Zobrazuje velikosti souborů a na pozadí rekurzivně sčítá velikost složek.

dialog-extension-purpose-size-map = Zobrazuje jako mapu ploch, kolik místa zabírá každá položka aktuální složky.

dialog-extension-purpose-tokei = Počítá řádky kódu v souborech nebo složkách pomocí Rustu a tokei.

dialog-extension-purpose-lua-tokei = Ukázkové rozšíření Lua, které počítá řádky kódu.

dialog-extension-purpose-lock-owner = Zobrazuje program nebo službu, která aktuálně soubor uzamyká.

dialog-extension-purpose-exif = Navrhuje hromadné přejmenování podle dat EXIF pořízení fotografií.

dialog-extension-purpose-7z = Prezentuje archivy 7-Zip jako procházené virtuální složky.

dialog-extension-purpose-bulk-folder = Vytvoří najednou mnoho složek podle uživatelem zadané šablony.

dialog-extension-folder-size = Sloupec velikosti složky

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Hlavní řádky kódu

dialog-extension-code-lines = Řádky kódu

dialog-extension-lock-owner = Vlastník zámku

dialog-extension-exif = Přejmenovat podle EXIF

dialog-extension-7z = Virtuální složka 7-Zip

dialog-extension-bulk-folder = Hromadný generátor složek

dialog-git-hash = Git hash
dialog-release-date-line = Datum vydání: { $value }
dialog-reset = Resetovat
dialog-reset-prompt = Resetovat { $label }? Odstraní se jen uložený stav; aktuální soubory se nemění.
dialog-reset-session-label = uložená okna a karty
dialog-reset-view-label = uložená nastavení zobrazení
dialog-reset-quick-access-label = připnutí Rychlého přístupu
dialog-reset-all-label = veškerý uložený stav Průzkumníka
dialog-permanent-delete-prompt =
    { $count ->
        [one] Odstranit trvale { $count } položku? Tuto akci nelze vrátit zpět.
        [few] Odstranit trvale { $count } položky? Tuto akci nelze vrátit zpět.
        [many] Odstranit trvale { $count } položek? Tuto akci nelze vrátit zpět.
       *[other] Odstranit trvale { $count } položek? Tuto akci nelze vrátit zpět.
    }
dialog-gdrive-trash-prompt =
    { $count ->
        [one] Přesunout { $count } položku do koše Google Drive? Obnova na drive.google.com po 30 dní. Toto není Koš Windows.
        [few] Přesunout { $count } položky do koše Google Drive? Obnova na drive.google.com po 30 dní. Toto není Koš Windows.
        [many] Přesunout { $count } položek do koše Google Drive? Obnova na drive.google.com po 30 dní. Toto není Koš Windows.
       *[other] Přesunout { $count } položek do koše Google Drive? Obnova na drive.google.com po 30 dní. Toto není Koš Windows.
    }
dialog-permanent-delete-aria =
    { $count ->
        [one] Odstranit trvale { $count } položku
        [few] Odstranit trvale { $count } položky
        [many] Odstranit trvale { $count } položek
       *[other] Odstranit trvale { $count } položek
    }
dialog-gdrive-trash-aria =
    { $count ->
        [one] Přesunout { $count } položku do koše Google Drive
        [few] Přesunout { $count } položky do koše Google Drive
        [many] Přesunout { $count } položek do koše Google Drive
       *[other] Přesunout { $count } položek do koše Google Drive
    }

ftp-sign-in = Přihlásit se k { $host }
ftp-sign-in-unencrypted = Přihlásit se k { $host } — Toto připojení není šifrované
