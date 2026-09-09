# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = Параметры папок

dialog-about = О SuperExplorer

dialog-version = Версия

dialog-build-date = Дата сборки

dialog-author = Автор

dialog-purpose = Назначение: { $value }

dialog-author-line = Автор: { $name } — { $bio } · { $date }

dialog-community = Сообщество: { $url }

dialog-plugin-safe-mode-title = Безопасный режим плагинов

dialog-plugin-safe-mode-body = Безопасный режим плагинов включён. Выберите плагины для повторного включения, нажмите «Применить» или «ОК», затем перезапустите SuperExplorer.

dialog-safe-mode-confirm-title = Безопасный режим требует подтверждения

dialog-safe-mode-confirm-aria = Требуется подтверждение безопасного режима; подозрительный пакет: { $package }

dialog-suspect-package = Подозрительный пакет: { $package }

dialog-interface = Интерфейс: { $value }

dialog-operation = Операция: { $value }

dialog-confirm-reenable = Подтвердить и включить снова

dialog-bookmark-action = Действие закладки

dialog-delete-bookmark = Удалить закладку

dialog-delete-bookmark-prompt = Удалить закладку «{ $name }»?

dialog-delete-bookmark-note = Закладка будет удалена. Файлы на диске не удаляются.

dialog-delete-bookmark-folder = Удалить папку закладок

dialog-delete-bookmark-folder-prompt = Удалить папку закладок «{ $name }»?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] Папка и { $count } элемент внутри будут удалены. Файлы на диске не удаляются.
        [few] Папка и { $count } элемента внутри будут удалены. Файлы на диске не удаляются.
        [many] Папка и { $count } элементов внутри будут удалены. Файлы на диске не удаляются.
       *[other] Папка и { $count } элемента внутри будут удалены. Файлы на диске не удаляются.
    }

dialog-rename-bookmark-folder = Переименовать папку закладок

dialog-bookmark-library = Библиотека

dialog-new-shortcut = Создать ярлык

dialog-new-remote-shortcut = Новый удалённый ярлык

dialog-shortcut-name = Имя ярлыка

dialog-shortcut-target = Целевой путь

dialog-create-remote-shortcut = Создать удалённый ярлык

dialog-cancel-new-shortcut = Отменить создание ярлыка

dialog-properties = { $name } - Свойства

dialog-properties-aria = Свойства { $name }

dialog-general = Общие

dialog-file-type = Тип: { $value }

dialog-location = Расположение: { $value }

dialog-size = Размер: { $value }

dialog-date-created = Дата создания: { $value }

dialog-date-modified = Дата изменения: { $value }

dialog-permissions = Права: { $mode }

dialog-file-in-use = Файл используется

dialog-items-in-use = Некоторые элементы используются

dialog-lock-owner-body =
    { $count ->
        [one] Windows не может удалить { $count } выбранный элемент, потому что другое приложение его использует.
        [few] Windows не может удалить { $count } выбранных элемента, потому что другое приложение их использует.
        [many] Windows не может удалить { $count } выбранных элементов, потому что другое приложение их использует.
       *[other] Windows не может удалить { $count } выбранных элемента, потому что другое приложение их использует.
    }

dialog-apps-using-file = Приложения, использующие файл

dialog-close-results = Результаты закрытия приложений

dialog-new-bookmark = Новая закладка

dialog-edit-bookmark = Изменить закладку

dialog-name-accelerator = Имя (N)

dialog-location-accelerator = Расположение (L)

dialog-url-accelerator = Адрес (L)
dialog-path-accelerator = Path (U)
dialog-tags-accelerator = Tags (T)
dialog-tags-placeholder = Comma-separated tags
dialog-tags-hint = Use tags to search and organize bookmarks

dialog-show-editor-on-save = Показывать редактор при сохранении (S)

dialog-lua-source = Исходный код Lua (только current_folder только для чтения)

dialog-folder-path-editable = Путь папки (можно изменить)

dialog-file-path-editable = Путь файла (можно изменить)

dialog-root = Корень

dialog-folder-options-scrollbar = Вертикальная полоса прокрутки параметров папок

dialog-new-bookmark-default = Новая закладка

dialog-new-folder-default = Новая папка

dialog-new-shortcut-default = Новый ярлык

dialog-shortcut-name-invalid = Имя ярлыка должно быть допустимым именем в текущей папке.

dialog-shortcut-target-invalid = Введите допустимый целевой путь.

dialog-shortcut-window-closed = Главное окно закрыто, поэтому ярлык не удалось создать.

dialog-folder-changed = Текущая папка изменилась. Откройте окно «Создать ярлык» снова.

dialog-remote-unavailable = Удалённая служба сейчас недоступна.

dialog-shortcut-in-progress = Уже создаётся другой ярлык.

dialog-allowed = Разрешено

dialog-not-allowed = Не разрешено

dialog-read = Чтение

dialog-write = Запись

dialog-execute = Выполнение

dialog-owner = Владелец

dialog-group = Группа

dialog-others = Остальные

dialog-remote-folder = Удалённая папка

dialog-remote-file = Удалённый файл

dialog-unavailable = Недоступно

dialog-bytes = { $value } байт

dialog-extension-author-bio = Автор SuperExplorer и официальных примеров расширений

dialog-extension-purpose-folder-size = Показывает размеры файлов и рекурсивно суммирует размер папок в фоне.

dialog-extension-purpose-size-map = Показывает, сколько места занимает каждый элемент текущей папки, в виде карты площади.

dialog-extension-purpose-tokei = Подсчитывает строки кода в файлах или папках с помощью Rust и tokei.

dialog-extension-purpose-lua-tokei = Пример расширения Lua, которое считает строки кода.

dialog-extension-purpose-lock-owner = Показывает программу или службу, которая сейчас блокирует файл.

dialog-extension-purpose-exif = Предлагает пакетное переименование по данным съёмки EXIF фотографий.

dialog-extension-purpose-7z = Представляет архивы 7-Zip как просматриваемые виртуальные папки.

dialog-extension-purpose-bulk-folder = Создаёт сразу много папок по шаблону пользователя.

dialog-extension-folder-size = Столбец размера папки

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Основные строки кода

dialog-extension-code-lines = Строки кода

dialog-extension-lock-owner = Владелец блокировки

dialog-extension-exif = Переименование по EXIF

dialog-extension-7z = Виртуальная папка 7-Zip

dialog-extension-bulk-folder = Массовое создание папок

dialog-git-hash = Хеш Git
dialog-release-date-line = Дата выпуска: { $value }
dialog-reset = Сбросить
dialog-reset-prompt = Сбросить { $label }? Будет удалено только сохранённое состояние; текущие файлы не изменятся.
dialog-reset-session-label = сохранённые окна и вкладки
dialog-reset-view-label = сохранённые параметры представления
dialog-reset-quick-access-label = закрепления быстрого доступа
dialog-reset-all-label = всё сохранённое состояние Explorer
dialog-permanent-delete-prompt =
    { $count ->
        [one] Удалить безвозвратно { $count } элемент? Это действие нельзя отменить.
        [few] Удалить безвозвратно { $count } элемента? Это действие нельзя отменить.
        [many] Удалить безвозвратно { $count } элементов? Это действие нельзя отменить.
       *[other] Удалить безвозвратно { $count } элемента? Это действие нельзя отменить.
    }
dialog-gdrive-trash-prompt =
    { $count ->
        [one] Переместить { $count } элемент в корзину Google Drive? Их можно восстановить на drive.google.com в течение 30 дней. Это не Корзина Windows.
        [few] Переместить { $count } элемента в корзину Google Drive? Их можно восстановить на drive.google.com в течение 30 дней. Это не Корзина Windows.
        [many] Переместить { $count } элементов в корзину Google Drive? Их можно восстановить на drive.google.com в течение 30 дней. Это не Корзина Windows.
       *[other] Переместить { $count } элемента в корзину Google Drive? Их можно восстановить на drive.google.com в течение 30 дней. Это не Корзина Windows.
    }
dialog-permanent-delete-aria =
    { $count ->
        [one] Удалить безвозвратно { $count } элемент
        [few] Удалить безвозвратно { $count } элемента
        [many] Удалить безвозвратно { $count } элементов
       *[other] Удалить безвозвратно { $count } элемента
    }
dialog-gdrive-trash-aria =
    { $count ->
        [one] Переместить { $count } элемент в корзину Google Drive
        [few] Переместить { $count } элемента в корзину Google Drive
        [many] Переместить { $count } элементов в корзину Google Drive
       *[other] Переместить { $count } элемента в корзину Google Drive
    }

ftp-sign-in = Вход на { $host }
ftp-sign-in-unencrypted = Вход на { $host } — это подключение не зашифровано
