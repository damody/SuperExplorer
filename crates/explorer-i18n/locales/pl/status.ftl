# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] Kopiuj { $count } element
        [few] Kopiuj { $count } elementy
        [many] Kopiuj { $count } elementów
       *[other] Kopiuj { $count } elementy
    }

move-items =
    { $count ->
        [one] Przenieś { $count } element
        [few] Przenieś { $count } elementy
        [many] Przenieś { $count } elementów
       *[other] Przenieś { $count } elementy
    }

recycle-items =
    { $count ->
        [one] Przenieś { $count } element do Kosza
        [few] Przenieś { $count } elementy do Kosza
        [many] Przenieś { $count } elementów do Kosza
       *[other] Przenieś { $count } elementy do Kosza
    }

permanent-delete-items =
    { $count ->
        [one] Usuń trwale { $count } element
        [few] Usuń trwale { $count } elementy
        [many] Usuń trwale { $count } elementów
       *[other] Usuń trwale { $count } elementy
    }

gdrive-trash-items =
    { $count ->
        [one] Przenieś { $count } element do kosza Google Drive
        [few] Przenieś { $count } elementy do kosza Google Drive
        [many] Przenieś { $count } elementów do kosza Google Drive
       *[other] Przenieś { $count } elementy do kosza Google Drive
    }

shortcut-items =
    { $count ->
        [one] Utwórz skrót dla { $count } elementu
        [few] Utwórz skrót dla { $count } elementów
        [many] Utwórz skrót dla { $count } elementów
       *[other] Utwórz skrót dla { $count } elementu
    }

extra-items =
    { $count ->
        [one] { $first } ({ $count } element więcej)
        [few] { $first } ({ $count } elementy więcej)
        [many] { $first } ({ $count } elementów więcej)
       *[other] { $first } ({ $count } elementy więcej)
    }

clipboard-source = Źródło pochodzi z systemowego schowka

op-new-folder = Nowy folder | { $path }

op-new-file = Nowy plik | { $path }

op-rename = Zmień nazwę | { $from } → { $to }

op-chmod = Zmień uprawnienia | { $path } → { $mode }

op-copy-route = Kopiuj { $count } elementów | { $source } → { $destination }

op-move-route = Przenieś { $count } elementów | { $source } → { $destination }

op-recycle-route = Do Kosza { $count } elementów | { $source }

op-permanent-delete-route = Usuń trwale { $count } elementów | { $source }

op-gdrive-trash-route = Kosz Google Drive { $count } elementów | { $source }

op-shortcut-route = Utwórz skrót { $count } elementów | { $source }

op-preparing-copy = Przygotowywanie kopiowania

op-copying = Kopiowanie

op-copy-complete = Kopiowanie ukończone

op-preparing-move = Przygotowywanie przenoszenia

op-moving = Przenoszenie

op-move-complete = Przenoszenie ukończone

op-preparing = Przygotowywanie

op-processing = Przetwarzanie

op-complete = Gotowe

op-finalizing = Kończenie

op-progress-items = { $summary } | { $phase } | Postęp { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Postęp { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Postęp { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Gotowe

op-cancelled = { $summary } | Anulowano

op-failed = { $summary } | Niepowodzenie: { $error }

op-partial = { $summary } | Częściowo: { $succeeded }/{ $total } powiodło się

op-cancelling = { $summary } | Anulowanie

op-success-route = Powodzenie | { $route }

op-skipped-route = Pominięto | { $route }

op-cancelled-route = Anulowano | { $route }

op-partial-status = Częściowo

op-failed-status = Niepowodzenie

op-error-code =  | Kod błędu { $code }

op-no-destination = Nie podano miejsca docelowego

op-no-source = Nie podano źródła

apk-installing = Instalowanie: { $name } → { $target }

apk-installed = Zainstalowano: { $name } → { $target }

apk-cancelled = Anulowano instalację { $name } na { $target }

apk-timeout = Przekroczono limit czasu instalacji { $name } na { $target }

apk-failed = Niepowodzenie instalacji { $name } na { $target }: { $error }

status-no-selection = Nie zaznaczono żadnych elementów

status-details-unavailable = Nie można wczytać szczegółów

status-item-count =
    { $count ->
        [one] { $count } element
        [few] { $count } elementy
        [many] { $count } elementów
       *[other] { $count } elementy
    }

status-preview-select-one = Wybierz element do podglądu

status-preview-failed = Nie można wygenerować podglądu tego pliku

status-preview-loading = Wczytywanie podglądu…

status-preview-item-failed = Nie można wczytać elementu podglądu

status-preview-select-single = Wybierz jeden element do podglądu

status-no-transfers = Brak operacji na plikach w tej sesji

status-thumbnail-cleared = Wyczyszczono pamięć podręczną miniaturek

status-thumbnail-clear-partial = Nie udało się w pełni wyczyścić pamięci podręcznej miniaturek; można ponowić, przeglądanie nadal działa

status-invalid-folder-name = Nieprawidłowa nazwa folderu. Popraw i spróbuj ponownie.

status-name-conflict = Element o tej nazwie już istnieje.

status-context-menu-unresponsive = Menu kontekstowe nie odpowiedziało. Możesz kontynuować pracę.

status-cannot-go-forward = Nie można przejść dalej

status-partial-folders = Nie można wyświetlić niektórych folderów.

status-cannot-list-folder = Nie można wyświetlić folderu.

status-cannot-cancel-disconnected = Nie można anulować: usługa plików nie jest połączona.

status-cannot-cancel-operation = Nie można anulować operacji na pliku: { $error }

status-cannot-load-folder = Nie można wczytać folderu. Spróbuj ponownie.

status-drive-free-of = { $free } wolne z { $total }

status-no-media = Brak nośnika

status-disconnected = Rozłączono

status-access-denied = Odmowa dostępu

status-capacity-unavailable = Pojemność niedostępna

status-waiting-file-count = Oczekiwanie na File Count…

status-file-count-limit = Zależy od File Count, więc nie uruchomiono

status-file-count-over-limit = File Count przekroczył limit, więc nie uruchomiono

status-file-count-pending-label = Limit

transfer-upload = Wysyłanie do miejsca docelowego

transfer-download = Pobieranie ze źródła

transfer-cancelling = Anulowanie

transfer-cancel = Anuluj

status-details-view = Widok szczegółów
status-icon-view = Widok ikon
status-error = Błąd · 
status-loading = Ładowanie · 
status-items-selected = { $count } elementów — zaznaczono { $selected }
status-operation-progress = Operacja { $completed }/{ $total } · { $name }
status-search-cancelled = Wyszukiwanie anulowane · 
status-search-error = Błąd wyszukiwania · 
status-search-fallback = Wyszukiwanie (system plików; indeks niedostępny) · 
status-search-indexed = Wyszukiwanie (indeks + zapasowe) · 
status-search-partial = Częściowe wyniki wyszukiwania · 
status-search-results = Wyniki wyszukiwania · 
status-lock-close-cancelled = Zamykanie aplikacji zostało anulowane.
status-lock-discovery-timeout = Wykrywanie właściciela blokady przekroczyło limit czasu.
status-lock-finding = Wyszukiwanie aplikacji używających wybranego elementu…
status-lock-owners-found =
    { $count ->
        [one] { $count } aplikacja używa wybranego elementu.
        [few] { $count } aplikacje używają wybranego elementu.
        [many] { $count } aplikacji używa wybranego elementu.
       *[other] { $count } aplikacje używają wybranego elementu.
    }
status-lock-partial-close = Niektóre aplikacje nie zostały zamknięte. Żaden proces nie został wymuszony.
status-lock-retry-limit = Osiągnięto limit ponowień. Element nie został usunięty.
status-lock-retrying = Ponawianie operacji usuwania…
status-lock-unidentified = System Windows nie mógł zidentyfikować aplikacji używającej tego elementu.
transfer-cancel-operation = Anuluj operację na plikach
transfer-open-location = Otwórz lokalizację transferu

status-generic-item = element
