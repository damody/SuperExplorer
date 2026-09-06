# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] Копіювати { $count } елемент
        [few] Копіювати { $count } елементи
        [many] Копіювати { $count } елементів
       *[other] Копіювати { $count } елементи
    }

move-items =
    { $count ->
        [one] Перемістити { $count } елемент
        [few] Перемістити { $count } елементи
        [many] Перемістити { $count } елементів
       *[other] Перемістити { $count } елементи
    }

recycle-items =
    { $count ->
        [one] Надіслати { $count } елемент до кошика
        [few] Надіслати { $count } елементи до кошика
        [many] Надіслати { $count } елементів до кошика
       *[other] Надіслати { $count } елементи до кошика
    }

permanent-delete-items =
    { $count ->
        [one] Видалити остаточно { $count } елемент
        [few] Видалити остаточно { $count } елементи
        [many] Видалити остаточно { $count } елементів
       *[other] Видалити остаточно { $count } елементи
    }

gdrive-trash-items =
    { $count ->
        [one] Перемістити { $count } елемент до кошика Google Drive
        [few] Перемістити { $count } елементи до кошика Google Drive
        [many] Перемістити { $count } елементів до кошика Google Drive
       *[other] Перемістити { $count } елементи до кошика Google Drive
    }

shortcut-items =
    { $count ->
        [one] Створити ярлик для { $count } елемента
        [few] Створити ярлик для { $count } елементів
        [many] Створити ярлик для { $count } елементів
       *[other] Створити ярлик для { $count } елемента
    }

extra-items =
    { $count ->
        [one] { $first } (ще { $count } елемент)
        [few] { $first } (ще { $count } елементи)
        [many] { $first } (ще { $count } елементів)
       *[other] { $first } (ще { $count } елементи)
    }

clipboard-source = Джерело надано системним буфером обміну

op-new-folder = Створити папку | { $path }

op-new-file = Створити файл | { $path }

op-rename = Перейменувати | { $from } → { $to }

op-chmod = Змінити права | { $path } → { $mode }

op-copy-route = Копіювати { $count } елементів | { $source } → { $destination }

op-move-route = Перемістити { $count } елементів | { $source } → { $destination }

op-recycle-route = До кошика { $count } елементів | { $source }

op-permanent-delete-route = Видалити остаточно { $count } елементів | { $source }

op-gdrive-trash-route = Кошик Google Drive { $count } елементів | { $source }

op-shortcut-route = Створити ярлик { $count } елементів | { $source }

op-preparing-copy = Підготовка копіювання

op-copying = Копіювання

op-copy-complete = Копіювання завершено

op-preparing-move = Підготовка переміщення

op-moving = Переміщення

op-move-complete = Переміщення завершено

op-preparing = Підготовка

op-processing = Обробка

op-complete = Готово

op-finalizing = Завершення

op-progress-items = { $summary } | { $phase } | Хід { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Хід { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Хід { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Готово

op-cancelled = { $summary } | Скасовано

op-failed = { $summary } | Збій: { $error }

op-partial = { $summary } | Частково: { $succeeded }/{ $total } успішно

op-cancelling = { $summary } | Скасування

op-success-route = Успішно | { $route }

op-skipped-route = Пропущено | { $route }

op-cancelled-route = Скасовано | { $route }

op-partial-status = Частково

op-failed-status = Збій

op-error-code =  | Код помилки { $code }

op-no-destination = Призначення не вказано

op-no-source = Джерело не вказано

apk-installing = Інсталяція: { $name } → { $target }

apk-installed = Інстальовано: { $name } → { $target }

apk-cancelled = Інсталяцію { $name } на { $target } скасовано

apk-timeout = Час інсталяції { $name } на { $target } минув

apk-failed = Збій інсталяції { $name } на { $target }: { $error }

status-no-selection = Нічого не вибрано

status-details-unavailable = Не вдалося завантажити відомості

status-item-count =
    { $count ->
        [one] { $count } елемент
        [few] { $count } елементи
        [many] { $count } елементів
       *[other] { $count } елементи
    }

status-preview-select-one = Виберіть елемент для попереднього перегляду

status-preview-failed = Не вдалося створити попередній перегляд цього файлу

status-preview-loading = Завантаження попереднього перегляду…

status-preview-item-failed = Не вдалося завантажити елемент попереднього перегляду

status-preview-select-single = Виберіть один елемент для попереднього перегляду

status-no-transfers = У цьому сеансі немає файлових операцій

status-thumbnail-cleared = Кеш ескізів очищено

status-thumbnail-clear-partial = Не вдалося повністю очистити кеш ескізів; можна повторити, перегляд надалі працює

status-invalid-folder-name = Неприпустиме ім’я папки. Виправте й повторіть спробу.

status-name-conflict = Елемент із таким іменем уже існує.

status-context-menu-unresponsive = Контекстне меню не відповіло. Можна продовжувати роботу.

status-cannot-go-forward = Неможливо перейти вперед

status-partial-folders = Деякі папки не вдалося перелічити.

status-cannot-list-folder = Не вдалося перелічити папку.

status-cannot-cancel-disconnected = Неможливо скасувати: служба файлів не підключена.

status-cannot-cancel-operation = Не вдалося скасувати файлову операцію: { $error }

status-cannot-load-folder = Не вдалося завантажити папку. Повторіть спробу.

status-drive-free-of = вільно { $free } з { $total }

status-no-media = Немає носія

status-disconnected = Відключено

status-access-denied = Відмовлено в доступі

status-capacity-unavailable = Ємність недоступна

status-waiting-file-count = Очікування File Count…

status-file-count-limit = Залежить від File Count, тому не запущено

status-file-count-over-limit = File Count перевищив обмеження, тому не запущено

status-file-count-pending-label = Limit

transfer-upload = Надсилання до призначення

transfer-download = Завантаження з джерела

transfer-cancelling = Скасування

transfer-cancel = Скасувати

status-details-view = Подання таблиці
status-icon-view = Подання піктограм
status-error = Помилка · 
status-loading = Завантаження · 
status-items-selected = { $count } елементів — вибрано: { $selected }
status-operation-progress = Операція { $completed }/{ $total } · { $name }
status-search-cancelled = Пошук скасовано · 
status-search-error = Помилка пошуку · 
status-search-fallback = Пошук (обхідний шлях файлової системи; індекс недоступний) · 
status-search-indexed = Пошук (індекс + обхідний шлях) · 
status-search-partial = Часткові результати пошуку · 
status-search-results = Результати пошуку · 
status-lock-close-cancelled = Закриття програм скасовано.
status-lock-discovery-timeout = Пошук власника блокування досяг граничного часу.
status-lock-finding = Пошук програм, які використовують вибраний елемент…
status-lock-owners-found =
    { $count ->
        [one] { $count } програма використовує вибраний елемент.
        [few] { $count } програми використовують вибраний елемент.
        [many] { $count } програм використовують вибраний елемент.
       *[other] { $count } програми використовують вибраний елемент.
    }
status-lock-partial-close = Деякі програми не закрилися. Жоден процес не було завершено примусово.
status-lock-retry-limit = Досягнуто ліміт повторних спроб. Елемент не видалено.
status-lock-retrying = Повторне видалення…
status-lock-unidentified = Windows не зміг визначити програму, яка використовує цей елемент.
transfer-cancel-operation = Скасувати файлову операцію
transfer-open-location = Відкрити розташування передавання
