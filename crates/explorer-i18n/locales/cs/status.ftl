# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] Kopírovat { $count } položku
        [few] Kopírovat { $count } položky
        [many] Kopírovat { $count } položek
       *[other] Kopírovat { $count } položek
    }

move-items =
    { $count ->
        [one] Přesunout { $count } položku
        [few] Přesunout { $count } položky
        [many] Přesunout { $count } položek
       *[other] Přesunout { $count } položek
    }

recycle-items =
    { $count ->
        [one] Přesunout { $count } položku do koše
        [few] Přesunout { $count } položky do koše
        [many] Přesunout { $count } položek do koše
       *[other] Přesunout { $count } položek do koše
    }

permanent-delete-items =
    { $count ->
        [one] Odstranit trvale { $count } položku
        [few] Odstranit trvale { $count } položky
        [many] Odstranit trvale { $count } položek
       *[other] Odstranit trvale { $count } položek
    }

gdrive-trash-items =
    { $count ->
        [one] Přesunout { $count } položku do koše Google Drive
        [few] Přesunout { $count } položky do koše Google Drive
        [many] Přesunout { $count } položek do koše Google Drive
       *[other] Přesunout { $count } položek do koše Google Drive
    }

shortcut-items =
    { $count ->
        [one] Vytvořit zástupce pro { $count } položku
        [few] Vytvořit zástupce pro { $count } položky
        [many] Vytvořit zástupce pro { $count } položek
       *[other] Vytvořit zástupce pro { $count } položek
    }

extra-items =
    { $count ->
        [one] { $first } (ještě { $count } položka)
        [few] { $first } (ještě { $count } položky)
        [many] { $first } (ještě { $count } položek)
       *[other] { $first } (ještě { $count } položek)
    }

clipboard-source = Zdroj poskytnutý systémovou schránkou

op-new-folder = Nová složka | { $path }

op-new-file = Nový soubor | { $path }

op-rename = Přejmenovat | { $from } → { $to }

op-chmod = Změnit oprávnění | { $path } → { $mode }

op-copy-route = Kopírovat { $count } položek | { $source } → { $destination }

op-move-route = Přesunout { $count } položek | { $source } → { $destination }

op-recycle-route = Do koše { $count } položek | { $source }

op-permanent-delete-route = Odstranit trvale { $count } položek | { $source }

op-gdrive-trash-route = Koš Google Drive { $count } položek | { $source }

op-shortcut-route = Vytvořit zástupce { $count } položek | { $source }

op-preparing-copy = Příprava kopírování

op-copying = Kopírování

op-copy-complete = Kopírování dokončeno

op-preparing-move = Příprava přesunu

op-moving = Přesouvání

op-move-complete = Přesun dokončen

op-preparing = Příprava

op-processing = Zpracování

op-complete = Hotovo

op-finalizing = Dokončování

op-progress-items = { $summary } | { $phase } | Průběh { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Průběh { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Průběh { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Hotovo

op-cancelled = { $summary } | Zrušeno

op-failed = { $summary } | Selhalo: { $error }

op-partial = { $summary } | Částečně: { $succeeded }/{ $total } úspěšných

op-cancelling = { $summary } | Rušení

op-success-route = Úspěšné | { $route }

op-skipped-route = Přeskočeno | { $route }

op-cancelled-route = Zrušeno | { $route }

op-partial-status = Částečně

op-failed-status = Selhalo

op-error-code =  | Kód chyby { $code }

op-no-destination = Cíl není zadán

op-no-source = Zdroj není zadán

apk-installing = Instalace: { $name } → { $target }

apk-installed = Nainstalováno: { $name } → { $target }

apk-cancelled = Instalace { $name } na { $target } zrušena

apk-timeout = Vypršel časový limit instalace { $name } na { $target }

apk-failed = Instalace { $name } na { $target } selhala: { $error }

status-no-selection = Nevybrány žádné položky

status-details-unavailable = Podrobnosti se nepodařilo načíst

status-item-count =
    { $count ->
        [one] { $count } položka
        [few] { $count } položky
        [many] { $count } položek
       *[other] { $count } položek
    }

status-preview-select-one = Vyberte položku pro náhled

status-preview-failed = Nelze vytvořit náhled tohoto souboru

status-preview-loading = Načítání náhledu…

status-preview-item-failed = Položku náhledu se nepodařilo načíst

status-preview-select-single = Vyberte jednu položku pro náhled

status-no-transfers = V této relaci nejsou žádné souborové operace

status-thumbnail-cleared = Mezipaměť miniatur vymazána

status-thumbnail-clear-partial = Mezipaměť miniatur se nepodařilo úplně vymazat; můžete to zkusit znovu a procházení stále funguje

status-invalid-folder-name = Neplatný název složky. Opravte ho a zkuste to znovu.

status-name-conflict = Položka s tímto názvem již existuje.

status-context-menu-unresponsive = Místní nabídka neodpověděla. Můžete pokračovat v práci.

status-cannot-go-forward = Nelze jít vpřed

status-partial-folders = Některé složky se nepodařilo vypsat.

status-cannot-list-folder = Složku se nepodařilo vypsat.

status-cannot-cancel-disconnected = Nelze stornovat: souborová služba není připojena.

status-cannot-cancel-operation = Souborovou operaci se nepodařilo stornovat: { $error }

status-cannot-load-folder = Složku se nepodařilo načíst. Zkuste to znovu.

status-drive-free-of = { $free } volných z { $total }

status-no-media = Žádné médium

status-disconnected = Odpojeno

status-access-denied = Přístup odepřen

status-capacity-unavailable = Kapacita není k dispozici

status-waiting-file-count = Čekání na File Count…

status-file-count-limit = Závisí na File Count, proto se nespustilo

status-file-count-over-limit = File Count překročil limit, proto se nespustilo

status-file-count-pending-label = Limit

transfer-upload = Nahrání do cíle

transfer-download = Stažení ze zdroje

transfer-cancelling = Rušení

transfer-cancel = Storno

status-details-view = Zobrazení Podrobnosti
status-icon-view = Zobrazení Ikony
status-error = Chyba · 
status-loading = Načítání · 
status-items-selected = { $count } položek — vybráno { $selected }
status-operation-progress = Operace { $completed }/{ $total } · { $name }
status-search-cancelled = Hledání zrušeno · 
status-search-error = Chyba hledání · 
status-search-fallback = Hledání (záloha souborového systému; index není k dispozici) · 
status-search-indexed = Hledání (index + záloha) · 
status-search-partial = Částečné výsledky hledání · 
status-search-results = Výsledky hledání · 
status-lock-close-cancelled = Zavírání aplikací bylo zrušeno.
status-lock-discovery-timeout = Zjišťování vlastníka zámku dosáhlo limitu.
status-lock-finding = Hledání aplikací používajících vybranou položku…
status-lock-owners-found =
    { $count ->
        [one] { $count } aplikace používá vybranou položku.
        [few] { $count } aplikace používají vybranou položku.
        [many] { $count } aplikací používá vybranou položku.
       *[other] { $count } aplikací používá vybranou položku.
    }
status-lock-partial-close = Některé aplikace se nezavřely. Žádný proces nebyl násilně ukončen.
status-lock-retry-limit = Byl dosažen limit opakování. Položka nebyla odstraněna.
status-lock-retrying = Opakování odstranění…
status-lock-unidentified = Windows nemohl identifikovat aplikaci používající tuto položku.
transfer-cancel-operation = Zrušit souborovou operaci
transfer-open-location = Otevřít umístění přenosu
