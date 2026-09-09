# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = Параметри папок

dialog-about = Про SuperExplorer

dialog-version = Версія

dialog-build-date = Дата збірки

dialog-author = Автор

dialog-purpose = Призначення: { $value }

dialog-author-line = Автор: { $name } — { $bio } · { $date }

dialog-community = Спільнота: { $url }

dialog-plugin-safe-mode-title = Безпечний режим плагінів

dialog-plugin-safe-mode-body = Безпечний режим плагінів увімкнуто. Виберіть плагіни для повторного ввімкнення, натисніть «Застосувати» або «ОК», потім перезапустіть SuperExplorer.

dialog-safe-mode-confirm-title = Безпечний режим потребує підтвердження

dialog-safe-mode-confirm-aria = Потрібне підтвердження безпечного режиму; підозрілий пакет: { $package }

dialog-suspect-package = Підозрілий пакет: { $package }

dialog-interface = Інтерфейс: { $value }

dialog-operation = Операція: { $value }

dialog-confirm-reenable = Підтвердити й увімкнути знову

dialog-bookmark-action = Дія закладки

dialog-delete-bookmark = Видалити закладку

dialog-delete-bookmark-prompt = Видалити закладку «{ $name }»?

dialog-delete-bookmark-note = Закладку буде вилучено. Файли на диску не видаляються.

dialog-delete-bookmark-folder = Видалити папку закладок

dialog-delete-bookmark-folder-prompt = Видалити папку закладок «{ $name }»?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] Папку та { $count } елемент у ній буде вилучено. Файли на диску не видаляються.
        [few] Папку та { $count } елементи в ній буде вилучено. Файли на диску не видаляються.
        [many] Папку та { $count } елементів у ній буде вилучено. Файли на диску не видаляються.
       *[other] Папку та { $count } елементи в ній буде вилучено. Файли на диску не видаляються.
    }

dialog-rename-bookmark-folder = Перейменувати папку закладок

dialog-bookmark-library = Бібліотека

dialog-new-shortcut = Створити ярлик

dialog-new-remote-shortcut = Новий віддалений ярлик

dialog-shortcut-name = Ім’я ярлика

dialog-shortcut-target = Цільовий шлях

dialog-create-remote-shortcut = Створити віддалений ярлик

dialog-cancel-new-shortcut = Скасувати створення ярлика

dialog-properties = { $name } - Властивості

dialog-properties-aria = Властивості { $name }

dialog-general = Загальні

dialog-file-type = Тип: { $value }

dialog-location = Розташування: { $value }

dialog-size = Розмір: { $value }

dialog-date-created = Дата створення: { $value }

dialog-date-modified = Дата змінення: { $value }

dialog-permissions = Права: { $mode }

dialog-file-in-use = Файл використовується

dialog-items-in-use = Деякі елементи використовуються

dialog-lock-owner-body =
    { $count ->
        [one] Windows не може видалити { $count } вибраний елемент, бо інша програма його використовує.
        [few] Windows не може видалити { $count } вибрані елементи, бо інша програма їх використовує.
        [many] Windows не може видалити { $count } вибраних елементів, бо інша програма їх використовує.
       *[other] Windows не може видалити { $count } вибрані елементи, бо інша програма їх використовує.
    }

dialog-apps-using-file = Програми, що використовують файл

dialog-close-results = Результати закриття програм

dialog-new-bookmark = Нова закладка

dialog-edit-bookmark = Змінити закладку

dialog-name-accelerator = Ім’я (N)

dialog-location-accelerator = Розташування (L)

dialog-url-accelerator = URL (L)
dialog-path-accelerator = Path (U)
dialog-tags-accelerator = Tags (T)
dialog-tags-placeholder = Comma-separated tags
dialog-tags-hint = Use tags to search and organize bookmarks

dialog-show-editor-on-save = Показувати редактор під час збереження (S)

dialog-lua-source = Вихідний код Lua (лише current_folder лише для читання)

dialog-folder-path-editable = Шлях папки (можна змінити)

dialog-file-path-editable = Шлях файлу (можна змінити)

dialog-root = Корінь

dialog-folder-options-scrollbar = Вертикальна смуга прокручування параметрів папок

dialog-new-bookmark-default = Нова закладка

dialog-new-folder-default = Нова папка

dialog-new-shortcut-default = Новий ярлик

dialog-shortcut-name-invalid = Ім’я ярлика має бути припустимим іменем у поточній папці.

dialog-shortcut-target-invalid = Введіть припустимий цільовий шлях.

dialog-shortcut-window-closed = Головне вікно закрито, тому ярлик не вдалося створити.

dialog-folder-changed = Поточна папка змінилася. Знову відкрийте вікно «Створити ярлик».

dialog-remote-unavailable = Віддалена служба зараз недоступна.

dialog-shortcut-in-progress = Інший ярлик уже створюється.

dialog-allowed = Дозволено

dialog-not-allowed = Не дозволено

dialog-read = Читання

dialog-write = Запис

dialog-execute = Виконання

dialog-owner = Власник

dialog-group = Група

dialog-others = Інші

dialog-remote-folder = Віддалена папка

dialog-remote-file = Віддалений файл

dialog-unavailable = Недоступно

dialog-bytes = { $value } байт

dialog-extension-author-bio = Автор SuperExplorer і офіційних зразків розширень

dialog-extension-purpose-folder-size = Показує розміри файлів і рекурсивно підсумовує розмір папок у фоні.

dialog-extension-purpose-size-map = Показує у вигляді карти площ, скільки місця займає кожен елемент поточної папки.

dialog-extension-purpose-tokei = Підраховує рядки коду у файлах або папках за допомогою Rust і tokei.

dialog-extension-purpose-lua-tokei = Приклад розширення Lua, яке рахує рядки коду.

dialog-extension-purpose-lock-owner = Показує програму або службу, яка зараз блокує файл.

dialog-extension-purpose-exif = Пропонує пакетне перейменування за даними зйомки EXIF світлин.

dialog-extension-purpose-7z = Показує архіви 7-Zip як віртуальні папки, які можна переглядати.

dialog-extension-purpose-bulk-folder = Створює одразу багато папок за шаблоном користувача.

dialog-extension-folder-size = Стовпець розміру папки

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Основні рядки коду

dialog-extension-code-lines = Рядки коду

dialog-extension-lock-owner = Власник блокування

dialog-extension-exif = Перейменування за EXIF

dialog-extension-7z = Віртуальна папка 7-Zip

dialog-extension-bulk-folder = Масове створення папок

dialog-git-hash = Геш Git
dialog-release-date-line = Дата випуску: { $value }
dialog-reset = Скинути
dialog-reset-prompt = Скинути { $label }? Буде видалено лише збережений стан; поточні файли не зміняться.
dialog-reset-session-label = збережені вікна та вкладки
dialog-reset-view-label = збережені параметри подання
dialog-reset-quick-access-label = закріплення швидкого доступу
dialog-reset-all-label = увесь збережений стан Explorer
dialog-permanent-delete-prompt =
    { $count ->
        [one] Видалити остаточно { $count } елемент? Цю дію не можна скасувати.
        [few] Видалити остаточно { $count } елементи? Цю дію не можна скасувати.
        [many] Видалити остаточно { $count } елементів? Цю дію не можна скасувати.
       *[other] Видалити остаточно { $count } елементи? Цю дію не можна скасувати.
    }
dialog-gdrive-trash-prompt =
    { $count ->
        [one] Перемістити { $count } елемент до кошика Google Drive? Їх можна відновити на drive.google.com протягом 30 днів. Це не Кошик Windows.
        [few] Перемістити { $count } елементи до кошика Google Drive? Їх можна відновити на drive.google.com протягом 30 днів. Це не Кошик Windows.
        [many] Перемістити { $count } елементів до кошика Google Drive? Їх можна відновити на drive.google.com протягом 30 днів. Це не Кошик Windows.
       *[other] Перемістити { $count } елементи до кошика Google Drive? Їх можна відновити на drive.google.com протягом 30 днів. Це не Кошик Windows.
    }
dialog-permanent-delete-aria =
    { $count ->
        [one] Видалити остаточно { $count } елемент
        [few] Видалити остаточно { $count } елементи
        [many] Видалити остаточно { $count } елементів
       *[other] Видалити остаточно { $count } елементи
    }
dialog-gdrive-trash-aria =
    { $count ->
        [one] Перемістити { $count } елемент до кошика Google Drive
        [few] Перемістити { $count } елементи до кошика Google Drive
        [many] Перемістити { $count } елементів до кошика Google Drive
       *[other] Перемістити { $count } елементи до кошика Google Drive
    }

ftp-sign-in = Вхід на { $host }
ftp-sign-in-unencrypted = Вхід на { $host } — це підключення не зашифровано
