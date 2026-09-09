# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = Opcje folderów

dialog-about = Informacje o SuperExplorer

dialog-version = Wersja

dialog-build-date = Data kompilacji

dialog-author = Autor

dialog-purpose = Przeznaczenie: { $value }

dialog-author-line = Autor: { $name } — { $bio } · { $date }

dialog-community = Społeczność: { $url }

dialog-plugin-safe-mode-title = Tryb awaryjny wtyczek

dialog-plugin-safe-mode-body = Tryb awaryjny wtyczek jest włączony. Wybierz wtyczki do ponownego włączenia, wybierz Zastosuj lub OK, a następnie uruchom ponownie SuperExplorer.

dialog-safe-mode-confirm-title = Tryb awaryjny wymaga potwierdzenia

dialog-safe-mode-confirm-aria = Wymagane potwierdzenie trybu awaryjnego; Podejrzany pakiet: { $package }

dialog-suspect-package = Podejrzany pakiet: { $package }

dialog-interface = Interfejs: { $value }

dialog-operation = Operacja: { $value }

dialog-confirm-reenable = Potwierdź i włącz ponownie

dialog-bookmark-action = Akcja zakładki

dialog-delete-bookmark = Usuń zakładkę

dialog-delete-bookmark-prompt = Usunąć zakładkę „{ $name }”?

dialog-delete-bookmark-note = Spowoduje to usunięcie zakładki. Pliki na dysku nie zostaną usunięte.

dialog-delete-bookmark-folder = Usuń folder zakładek

dialog-delete-bookmark-folder-prompt = Usunąć folder zakładek „{ $name }”?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] Folder i { $count } element w nim zostaną usunięte. Pliki na dysku nie zostaną usunięte.
        [few] Folder i { $count } elementy w nim zostaną usunięte. Pliki na dysku nie zostaną usunięte.
        [many] Folder i { $count } elementów w nim zostanie usuniętych. Pliki na dysku nie zostaną usunięte.
       *[other] Folder i { $count } elementy w nim zostaną usunięte. Pliki na dysku nie zostaną usunięte.
    }

dialog-rename-bookmark-folder = Zmień nazwę folderu zakładek

dialog-bookmark-library = Biblioteka

dialog-new-shortcut = Nowy skrót

dialog-new-remote-shortcut = Nowy skrót zdalny

dialog-shortcut-name = Nazwa skrótu

dialog-shortcut-target = Ścieżka docelowa

dialog-create-remote-shortcut = Utwórz skrót zdalny

dialog-cancel-new-shortcut = Anuluj nowy skrót

dialog-properties = { $name } - Właściwości

dialog-properties-aria = Właściwości { $name }

dialog-general = Ogólne

dialog-file-type = Typ: { $value }

dialog-location = Lokalizacja: { $value }

dialog-size = Rozmiar: { $value }

dialog-date-created = Data utworzenia: { $value }

dialog-date-modified = Data modyfikacji: { $value }

dialog-permissions = Uprawnienia: { $mode }

dialog-file-in-use = Plik jest używany

dialog-items-in-use = Niektóre elementy są używane

dialog-lock-owner-body =
    { $count ->
        [one] Windows nie może usunąć { $count } zaznaczonego elementu, ponieważ używa go inna aplikacja.
        [few] Windows nie może usunąć { $count } zaznaczonych elementów, ponieważ używa ich inna aplikacja.
        [many] Windows nie może usunąć { $count } zaznaczonych elementów, ponieważ używa ich inna aplikacja.
       *[other] Windows nie może usunąć { $count } zaznaczonych elementów, ponieważ używa ich inna aplikacja.
    }

dialog-apps-using-file = Aplikacje używające pliku

dialog-close-results = Wyniki zamykania aplikacji

dialog-new-bookmark = Nowa zakładka

dialog-edit-bookmark = Edytuj zakładkę

dialog-name-accelerator = Nazwa (N)

dialog-location-accelerator = Lokalizacja (L)

dialog-url-accelerator = Adres (L)
dialog-path-accelerator = Path (U)
dialog-tags-accelerator = Tags (T)
dialog-tags-placeholder = Comma-separated tags
dialog-tags-hint = Use tags to search and organize bookmarks

dialog-show-editor-on-save = Pokaż edytor podczas zapisywania (S)

dialog-lua-source = Kod źródłowy Lua (tylko tylko do odczytu current_folder)

dialog-folder-path-editable = Ścieżka folderu (edytowalna)

dialog-file-path-editable = Ścieżka pliku (edytowalna)

dialog-root = Katalog główny

dialog-folder-options-scrollbar = Pionowy pasek przewijania opcji folderów

dialog-new-bookmark-default = Nowa zakładka

dialog-new-folder-default = Nowy folder

dialog-new-shortcut-default = Nowy skrót

dialog-shortcut-name-invalid = Nazwa skrótu musi być prawidłową nazwą w bieżącym folderze.

dialog-shortcut-target-invalid = Wprowadź prawidłową ścieżkę docelową.

dialog-shortcut-window-closed = Okno główne zostało zamknięte, więc nie można utworzyć skrótu.

dialog-folder-changed = Bieżący folder uległ zmianie. Otwórz ponownie okno Nowy skrót.

dialog-remote-unavailable = Usługa zdalna jest obecnie niedostępna.

dialog-shortcut-in-progress = Inny skrót jest już tworzony.

dialog-allowed = Dozwolone

dialog-not-allowed = Niedozwolone

dialog-read = Odczyt

dialog-write = Zapis

dialog-execute = Wykonywanie

dialog-owner = Właściciel

dialog-group = Grupa

dialog-others = Inni

dialog-remote-folder = Folder zdalny

dialog-remote-file = Plik zdalny

dialog-unavailable = Niedostępne

dialog-bytes = { $value } bajtów

dialog-extension-author-bio = Autor SuperExplorer i oficjalnych przykładowych rozszerzeń

dialog-extension-purpose-folder-size = Pokazuje rozmiary plików i rekurencyjnie sumuje rozmiar folderu w tle.

dialog-extension-purpose-size-map = Pokazuje na mapie powierzchni, ile miejsca zajmuje każdy element bieżącego folderu.

dialog-extension-purpose-tokei = Zlicza wiersze kodu w plikach lub folderach za pomocą Rust i tokei.

dialog-extension-purpose-lua-tokei = Przykładowe rozszerzenie Lua zliczające wiersze kodu.

dialog-extension-purpose-lock-owner = Pokazuje program lub usługę, która obecnie blokuje plik.

dialog-extension-purpose-exif = Sugeruje zbiorczą zmianę nazw na podstawie danych EXIF zdjęć.

dialog-extension-purpose-7z = Przedstawia archiwa 7-Zip jako przeglądalne foldery wirtualne.

dialog-extension-purpose-bulk-folder = Tworzy wiele folderów naraz na podstawie szablonu użytkownika.

dialog-extension-folder-size = Kolumna rozmiaru folderu

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Główne wiersze kodu

dialog-extension-code-lines = Wiersze kodu

dialog-extension-lock-owner = Właściciel blokady

dialog-extension-exif = Zmień nazwę na podstawie EXIF

dialog-extension-7z = Wirtualny folder 7-Zip

dialog-extension-bulk-folder = Masowe tworzenie folderów

dialog-git-hash = Skrót Git
dialog-release-date-line = Data wydania: { $value }
dialog-reset = Resetuj
dialog-reset-prompt = Zresetować { $label }? Usunięty zostanie tylko zapisany stan; bieżące pliki nie ulegną zmianie.
dialog-reset-session-label = zapisane okna i karty
dialog-reset-view-label = zapisane ustawienia widoku
dialog-reset-quick-access-label = przypięcia Szybkiego dostępu
dialog-reset-all-label = cały zapisany stan Eksploratora
dialog-permanent-delete-prompt =
    { $count ->
        [one] Usunąć trwale { $count } element? Tej operacji nie można cofnąć.
        [few] Usunąć trwale { $count } elementy? Tej operacji nie można cofnąć.
        [many] Usunąć trwale { $count } elementów? Tej operacji nie można cofnąć.
       *[other] Usunąć trwale { $count } elementy? Tej operacji nie można cofnąć.
    }
dialog-gdrive-trash-prompt =
    { $count ->
        [one] Przenieść { $count } element do kosza Google Drive? Można je odzyskać na drive.google.com przez 30 dni. To nie jest Kosz systemu Windows.
        [few] Przenieść { $count } elementy do kosza Google Drive? Można je odzyskać na drive.google.com przez 30 dni. To nie jest Kosz systemu Windows.
        [many] Przenieść { $count } elementów do kosza Google Drive? Można je odzyskać na drive.google.com przez 30 dni. To nie jest Kosz systemu Windows.
       *[other] Przenieść { $count } elementy do kosza Google Drive? Można je odzyskać na drive.google.com przez 30 dni. To nie jest Kosz systemu Windows.
    }
dialog-permanent-delete-aria =
    { $count ->
        [one] Usuń trwale { $count } element
        [few] Usuń trwale { $count } elementy
        [many] Usuń trwale { $count } elementów
       *[other] Usuń trwale { $count } elementy
    }
dialog-gdrive-trash-aria =
    { $count ->
        [one] Przenieś { $count } element do kosza Google Drive
        [few] Przenieś { $count } elementy do kosza Google Drive
        [many] Przenieś { $count } elementów do kosza Google Drive
       *[other] Przenieś { $count } elementy do kosza Google Drive
    }

ftp-sign-in = Zaloguj się do { $host }
ftp-sign-in-unencrypted = Zaloguj się do { $host } — To połączenie nie jest szyfrowane
