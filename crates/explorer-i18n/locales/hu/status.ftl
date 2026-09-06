# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] { $count } elem másolása
       *[other] { $count } elem másolása
    }

move-items =
    { $count ->
        [one] { $count } elem áthelyezése
       *[other] { $count } elem áthelyezése
    }

recycle-items =
    { $count ->
        [one] { $count } elem áthelyezése a lomtárba
       *[other] { $count } elem áthelyezése a lomtárba
    }

permanent-delete-items =
    { $count ->
        [one] { $count } elem végleges törlése
       *[other] { $count } elem végleges törlése
    }

gdrive-trash-items =
    { $count ->
        [one] { $count } elem áthelyezése a Google Drive lomtárába
       *[other] { $count } elem áthelyezése a Google Drive lomtárába
    }

shortcut-items =
    { $count ->
        [one] Parancsikon létrehozása { $count } elemhez
       *[other] Parancsikon létrehozása { $count } elemhez
    }

extra-items =
    { $count ->
        [one] { $first } ({ $count } további elem)
       *[other] { $first } ({ $count } további elem)
    }

clipboard-source = A forrást a rendszer vágólapja adta

op-new-folder = Új mappa | { $path }

op-new-file = Új fájl | { $path }

op-rename = Átnevezés | { $from } → { $to }

op-chmod = Jogosultságok módosítása | { $path } → { $mode }

op-copy-route = { $count } elem másolása | { $source } → { $destination }

op-move-route = { $count } elem áthelyezése | { $source } → { $destination }

op-recycle-route = Lomtár { $count } elem | { $source }

op-permanent-delete-route = Végleges törlés { $count } elem | { $source }

op-gdrive-trash-route = Google Drive lomtár { $count } elem | { $source }

op-shortcut-route = Parancsikon { $count } elem | { $source }

op-preparing-copy = Másolás előkészítése

op-copying = Másolás

op-copy-complete = Másolás kész

op-preparing-move = Áthelyezés előkészítése

op-moving = Áthelyezés

op-move-complete = Áthelyezés kész

op-preparing = Előkészítés

op-processing = Feldolgozás

op-complete = Kész

op-finalizing = Befejezés

op-progress-items = { $summary } | { $phase } | Folyamat { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Folyamat { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Folyamat { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Kész

op-cancelled = { $summary } | Megszakítva

op-failed = { $summary } | Sikertelen: { $error }

op-partial = { $summary } | Részleges: { $succeeded }/{ $total } sikerült

op-cancelling = { $summary } | Megszakítás

op-success-route = Sikeres | { $route }

op-skipped-route = Kihagyva | { $route }

op-cancelled-route = Megszakítva | { $route }

op-partial-status = Részleges

op-failed-status = Sikertelen

op-error-code =  | Hibakód { $code }

op-no-destination = Nincs megadva cél

op-no-source = Nincs megadva forrás

apk-installing = Telepítés: { $name } → { $target }

apk-installed = Telepítve: { $name } → { $target }

apk-cancelled = { $name } telepítése ide: { $target } megszakítva

apk-timeout = { $name } telepítése ide: { $target } túllépte az időkorlátot

apk-failed = { $name } telepítése ide: { $target } sikertelen: { $error }

status-no-selection = Nincs kijelölt elem

status-details-unavailable = A részletek nem tölthetők be

status-item-count =
    { $count ->
        [one] { $count } elem
       *[other] { $count } elem
    }

status-preview-select-one = Válasszon egy elemet az előnézethez

status-preview-failed = Ehhez a fájlhoz nem készíthető előnézet

status-preview-loading = Előnézet betöltése…

status-preview-item-failed = Az előnézeti elem nem tölthető be

status-preview-select-single = Az előnézethez egyetlen elemet válasszon

status-no-transfers = Nincsenek fájlműveletek ebben a munkamenetben

status-thumbnail-cleared = Bélyegkép-gyorsítótár törölve

status-thumbnail-clear-partial = A bélyegkép-gyorsítótárat nem sikerült teljesen törölni; újrapróbálható, a böngészés továbbra is működik

status-invalid-folder-name = Érvénytelen mappanév. Javítsa, majd próbálja újra.

status-name-conflict = Már létezik ilyen nevű elem.

status-context-menu-unresponsive = A helyi menü nem válaszolt. Tovább dolgozhat.

status-cannot-go-forward = Nem lehet előre lépni

status-partial-folders = Néhány mappa nem listázható.

status-cannot-list-folder = A mappa nem listázható.

status-cannot-cancel-disconnected = Nem szakítható meg: a fájlszolgáltatás nincs csatlakoztatva.

status-cannot-cancel-operation = A fájlművelet nem szakítható meg: { $error }

status-cannot-load-folder = A mappa nem tölthető be. Próbálja újra.

status-drive-free-of = { $free } szabad / { $total }

status-no-media = Nincs adathordozó

status-disconnected = Leválasztva

status-access-denied = Hozzáférés megtagadva

status-capacity-unavailable = A kapacitás nem érhető el

status-waiting-file-count = Várakozás a File Count-ra…

status-file-count-limit = A File Count-tól függ, ezért nem indult

status-file-count-over-limit = A File Count túllépte a korlátot, ezért nem indult

status-file-count-pending-label = Limit

transfer-upload = Célfeltöltés

transfer-download = Forrásletöltés

transfer-cancelling = Megszakítás

transfer-cancel = Mégse

status-details-view = Részletek nézet
status-icon-view = Ikonnézet
status-error = Hiba · 
status-loading = Betöltés · 
status-items-selected = { $count } elem — { $selected } kijelölve
status-operation-progress = Művelet { $completed }/{ $total } · { $name }
status-search-cancelled = Keresés megszakítva · 
status-search-error = Keresési hiba · 
status-search-fallback = Keresés (fájlrendszer-tartalék; az index nem érhető el) · 
status-search-indexed = Keresés (index + tartalék) · 
status-search-partial = Részleges keresési eredmények · 
status-search-results = Keresési eredmények · 
status-lock-close-cancelled = Az alkalmazások bezárása megszakítva.
status-lock-discovery-timeout = A zárolás tulajdonosának felderítése elérte a határidőt.
status-lock-finding = A kijelölt elemet használó alkalmazások keresése…
status-lock-owners-found = { $count } alkalmazás használja a kijelölt elemet.
status-lock-partial-close = Egyes alkalmazások nem záródtak be. Egy folyamatot sem kényszerítettünk le.
status-lock-retry-limit = Elérte az újrapróbálkozási korlátot. Az elem nem lett törölve.
status-lock-retrying = A törlési művelet újrapróbálása…
status-lock-unidentified = A Windows nem tudta azonosítani az elemet használó alkalmazást.
transfer-cancel-operation = Fájlművelet megszakítása
transfer-open-location = Átvitel helyének megnyitása

status-generic-item = elem
