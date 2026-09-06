# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] Копировать { $count } элемент
        [few] Копировать { $count } элемента
        [many] Копировать { $count } элементов
       *[other] Копировать { $count } элемента
    }

move-items =
    { $count ->
        [one] Переместить { $count } элемент
        [few] Переместить { $count } элемента
        [many] Переместить { $count } элементов
       *[other] Переместить { $count } элемента
    }

recycle-items =
    { $count ->
        [one] Отправить { $count } элемент в корзину
        [few] Отправить { $count } элемента в корзину
        [many] Отправить { $count } элементов в корзину
       *[other] Отправить { $count } элемента в корзину
    }

permanent-delete-items =
    { $count ->
        [one] Удалить безвозвратно { $count } элемент
        [few] Удалить безвозвратно { $count } элемента
        [many] Удалить безвозвратно { $count } элементов
       *[other] Удалить безвозвратно { $count } элемента
    }

gdrive-trash-items =
    { $count ->
        [one] Переместить { $count } элемент в корзину Google Drive
        [few] Переместить { $count } элемента в корзину Google Drive
        [many] Переместить { $count } элементов в корзину Google Drive
       *[other] Переместить { $count } элемента в корзину Google Drive
    }

shortcut-items =
    { $count ->
        [one] Создать ярлык для { $count } элемента
        [few] Создать ярлык для { $count } элементов
        [many] Создать ярлык для { $count } элементов
       *[other] Создать ярлык для { $count } элемента
    }

extra-items =
    { $count ->
        [one] { $first } (ещё { $count } элемент)
        [few] { $first } (ещё { $count } элемента)
        [many] { $first } (ещё { $count } элементов)
       *[other] { $first } (ещё { $count } элемента)
    }

clipboard-source = Источник предоставлен системным буфером обмена

op-new-folder = Создать папку | { $path }

op-new-file = Создать файл | { $path }

op-rename = Переименовать | { $from } → { $to }

op-chmod = Изменить права | { $path } → { $mode }

op-copy-route = Копировать { $count } элементов | { $source } → { $destination }

op-move-route = Переместить { $count } элементов | { $source } → { $destination }

op-recycle-route = В корзину { $count } элементов | { $source }

op-permanent-delete-route = Удалить безвозвратно { $count } элементов | { $source }

op-gdrive-trash-route = В корзину Google Drive { $count } элементов | { $source }

op-shortcut-route = Создать ярлык { $count } элементов | { $source }

op-preparing-copy = Подготовка копирования

op-copying = Копирование

op-copy-complete = Копирование завершено

op-preparing-move = Подготовка перемещения

op-moving = Перемещение

op-move-complete = Перемещение завершено

op-preparing = Подготовка

op-processing = Обработка

op-complete = Готово

op-finalizing = Завершение

op-progress-items = { $summary } | { $phase } | Ход { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Ход { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Ход { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Готово

op-cancelled = { $summary } | Отменено

op-failed = { $summary } | Сбой: { $error }

op-partial = { $summary } | Частично: { $succeeded }/{ $total } успешно

op-cancelling = { $summary } | Отмена

op-success-route = Успешно | { $route }

op-skipped-route = Пропущено | { $route }

op-cancelled-route = Отменено | { $route }

op-partial-status = Частично

op-failed-status = Сбой

op-error-code =  | Код ошибки { $code }

op-no-destination = Назначение не указано

op-no-source = Источник не указан

apk-installing = Установка: { $name } → { $target }

apk-installed = Установлено: { $name } → { $target }

apk-cancelled = Установка { $name } на { $target } отменена

apk-timeout = Истекло время установки { $name } на { $target }

apk-failed = Сбой установки { $name } на { $target }: { $error }

status-no-selection = Ничего не выбрано

status-details-unavailable = Не удалось загрузить сведения

status-item-count =
    { $count ->
        [one] { $count } элемент
        [few] { $count } элемента
        [many] { $count } элементов
       *[other] { $count } элемента
    }

status-preview-select-one = Выберите элемент для просмотра

status-preview-failed = Не удалось создать просмотр этого файла

status-preview-loading = Загрузка просмотра…

status-preview-item-failed = Не удалось загрузить элемент просмотра

status-preview-select-single = Выберите один элемент для просмотра

status-no-transfers = В этом сеансе нет файловых операций

status-thumbnail-cleared = Кэш эскизов очищен

status-thumbnail-clear-partial = Не удалось полностью очистить кэш эскизов; можно повторить, просмотр по-прежнему работает

status-invalid-folder-name = Недопустимое имя папки. Исправьте и повторите попытку.

status-name-conflict = Элемент с таким именем уже существует.

status-context-menu-unresponsive = Контекстное меню не ответило. Можно продолжать работу.

status-cannot-go-forward = Невозможно перейти вперёд

status-partial-folders = Некоторые папки не удалось перечислить.

status-cannot-list-folder = Не удалось перечислить папку.

status-cannot-cancel-disconnected = Невозможно отменить: служба файлов не подключена.

status-cannot-cancel-operation = Не удалось отменить файловую операцию: { $error }

status-cannot-load-folder = Не удалось загрузить папку. Повторите попытку.

status-drive-free-of = свободно { $free } из { $total }

status-no-media = Нет носителя

status-disconnected = Отключено

status-access-denied = Отказано в доступе

status-capacity-unavailable = Ёмкость недоступна

status-waiting-file-count = Ожидание File Count…

status-file-count-limit = Зависит от File Count, поэтому не запущено

status-file-count-over-limit = File Count превысил ограничение, поэтому не запущено

status-file-count-pending-label = Limit

transfer-upload = Отправка в назначение

transfer-download = Скачивание из источника

transfer-cancelling = Отмена

transfer-cancel = Отмена
