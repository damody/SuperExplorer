# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = Mappa beállításai

dialog-about = A SuperExplorer névjegye

dialog-version = Verzió

dialog-build-date = Fordítás dátuma

dialog-author = Szerző

dialog-purpose = Cél: { $value }

dialog-author-line = Szerző: { $name } — { $bio } · { $date }

dialog-community = Közösség: { $url }

dialog-plugin-safe-mode-title = Bővítmény csökkentett mód

dialog-plugin-safe-mode-body = A bővítmény csökkentett mód be van kapcsolva. Válassza ki az újraengedélyezendő bővítményeket, kattintson az Alkalmaz vagy az OK gombra, majd indítsa újra a SuperExplorer programot.

dialog-safe-mode-confirm-title = A csökkentett mód megerősítést igényel

dialog-safe-mode-confirm-aria = A csökkentett mód megerősítése szükséges; Gyanús csomag: { $package }

dialog-suspect-package = Gyanús csomag: { $package }

dialog-interface = Interfész: { $value }

dialog-operation = Művelet: { $value }

dialog-confirm-reenable = Megerősítés és újbóli engedélyezés

dialog-bookmark-action = Könyvjelzőművelet

dialog-delete-bookmark = Könyvjelző törlése

dialog-delete-bookmark-prompt = Törli a(z) „{ $name }” könyvjelzőt?

dialog-delete-bookmark-note = Ezzel a könyvjelző eltávolításra kerül. A lemezen lévő fájlok nem törlődnek.

dialog-delete-bookmark-folder = Könyvjelzőmappa törlése

dialog-delete-bookmark-folder-prompt = Törli a(z) „{ $name }” könyvjelzőmappát?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] A mappa és a benne lévő { $count } elem eltávolításra kerül. A lemezen lévő fájlok nem törlődnek.
       *[other] A mappa és a benne lévő { $count } elem eltávolításra kerül. A lemezen lévő fájlok nem törlődnek.
    }

dialog-rename-bookmark-folder = Könyvjelzőmappa átnevezése

dialog-bookmark-library = Könyvtár

dialog-new-shortcut = Új parancsikon

dialog-new-remote-shortcut = Új távoli parancsikon

dialog-shortcut-name = Parancsikon neve

dialog-shortcut-target = Célútvonal

dialog-create-remote-shortcut = Távoli parancsikon létrehozása

dialog-cancel-new-shortcut = Új parancsikon megszakítása

dialog-properties = { $name } - Tulajdonságok

dialog-properties-aria = { $name } tulajdonságai

dialog-general = Általános

dialog-file-type = Típus: { $value }

dialog-location = Hely: { $value }

dialog-size = Méret: { $value }

dialog-date-created = Létrehozás dátuma: { $value }

dialog-date-modified = Módosítás dátuma: { $value }

dialog-permissions = Jogosultságok: { $mode }

dialog-file-in-use = A fájl használatban van

dialog-items-in-use = Néhány elem használatban van

dialog-lock-owner-body =
    { $count ->
        [one] A Windows nem tudja törölni a kijelölt { $count } elemet, mert egy másik alkalmazás használja.
       *[other] A Windows nem tudja törölni a kijelölt { $count } elemet, mert egy másik alkalmazás használja.
    }

dialog-apps-using-file = A fájlt használó alkalmazások

dialog-close-results = Alkalmazások bezárásának eredményei

dialog-new-bookmark = Új könyvjelző

dialog-edit-bookmark = Könyvjelző szerkesztése

dialog-name-accelerator = Név (N)

dialog-location-accelerator = Hely (L)

dialog-url-accelerator = URL (L)

dialog-show-editor-on-save = Szerkesztő megjelenítése mentéskor (S)

dialog-lua-source = Lua-forrás (csak írásvédett current_folder)

dialog-folder-path-editable = Mappaútvonal (szerkeszthető)

dialog-file-path-editable = Fájlútvonal (szerkeszthető)

dialog-root = Gyökér

dialog-folder-options-scrollbar = Mappa beállításai függőleges görgetősáv

dialog-new-bookmark-default = Új könyvjelző

dialog-new-folder-default = Új mappa

dialog-new-shortcut-default = Új parancsikon

dialog-shortcut-name-invalid = A parancsikon nevének érvényes névnek kell lennie az aktuális mappában.

dialog-shortcut-target-invalid = Adjon meg egy érvényes célútvonalat.

dialog-shortcut-window-closed = A főablak bezárult, ezért a parancsikont nem sikerült létrehozni.

dialog-folder-changed = Az aktuális mappa megváltozott. Nyissa meg újra az Új parancsikon ablakot.

dialog-remote-unavailable = A távoli szolgáltatás jelenleg nem érhető el.

dialog-shortcut-in-progress = Már készül egy másik parancsikon.

dialog-allowed = Engedélyezve

dialog-not-allowed = Nincs engedélyezve

dialog-read = Olvasás

dialog-write = Írás

dialog-execute = Végrehajtás

dialog-owner = Tulajdonos

dialog-group = Csoport

dialog-others = Egyebek

dialog-remote-folder = Távoli mappa

dialog-remote-file = Távoli fájl

dialog-unavailable = Nem érhető el

dialog-bytes = { $value } bájt

dialog-extension-author-bio = A SuperExplorer és a hivatalos mintabővítmények szerzője

dialog-extension-purpose-folder-size = Fájlméreteket jelenít meg, és a háttérben rekurzívan összegzi a mappaméretet.

dialog-extension-purpose-size-map = Területi térképként mutatja, mennyi helyet foglal az aktuális mappa egyes elemei.

dialog-extension-purpose-tokei = Rust és tokei segítségével kódsorokat számol fájlokban vagy mappákban.

dialog-extension-purpose-lua-tokei = Mintapélda Lua-bővítmény, amely kódsorokat számol.

dialog-extension-purpose-lock-owner = Megjeleníti a fájlt jelenleg zároló programot vagy szolgáltatást.

dialog-extension-purpose-exif = Fénykép EXIF-felvételi adatok alapján kötegelt átnevezést javasol.

dialog-extension-purpose-7z = 7-Zip-archívumokat böngészhető virtuális mappákként jelenít meg.

dialog-extension-purpose-bulk-folder = Felhasználói sablonból egyszerre sok mappát hoz létre.

dialog-extension-folder-size = Mappaméret oszlop

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Fő kódsorok

dialog-extension-code-lines = Kódsorok

dialog-extension-lock-owner = Zárolás tulajdonosa

dialog-extension-exif = Átnevezés EXIF alapján

dialog-extension-7z = 7-Zip virtuális mappa

dialog-extension-bulk-folder = Tömeges mappagenerátor

dialog-git-hash = Git-kivonat
dialog-release-date-line = Kiadás dátuma: { $value }
dialog-reset = Visszaállítás
dialog-reset-prompt = Visszaállítja: { $label }? Csak a mentett állapotot távolítja el; a jelenlegi fájlok nem változnak.
dialog-reset-session-label = mentett ablakok és lapok
dialog-reset-view-label = mentett nézetbeállítások
dialog-reset-quick-access-label = Gyors elérés kitűzései
dialog-reset-all-label = minden mentett Explorer-állapot
dialog-permanent-delete-prompt = { $count } elem végleges törlése? A művelet nem vonható vissza.
dialog-gdrive-trash-prompt = { $count } elem áthelyezése a Google Drive lomtárába? 30 napig visszaállítható a drive.google.com oldalon. Ez nem a Windows Lomtár.
dialog-permanent-delete-aria = { $count } elem végleges törlése
dialog-gdrive-trash-aria = { $count } elem áthelyezése a Google Drive lomtárába

ftp-sign-in = Bejelentkezés ide: { $host }
ftp-sign-in-unencrypted = Bejelentkezés ide: { $host } — Ez a kapcsolat nincs titkosítva
