# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] Copiar { $count } elemento
       *[other] Copiar { $count } elementos
    }

move-items =
    { $count ->
        [one] Mover { $count } elemento
       *[other] Mover { $count } elementos
    }

recycle-items =
    { $count ->
        [one] Enviar { $count } elemento a la Papelera
       *[other] Enviar { $count } elementos a la Papelera
    }

permanent-delete-items =
    { $count ->
        [one] Eliminar definitivamente { $count } elemento
       *[other] Eliminar definitivamente { $count } elementos
    }

gdrive-trash-items =
    { $count ->
        [one] Mover { $count } elemento a la papelera de Google Drive
       *[other] Mover { $count } elementos a la papelera de Google Drive
    }

shortcut-items =
    { $count ->
        [one] Crear acceso directo para { $count } elemento
       *[other] Crear acceso directo para { $count } elementos
    }

extra-items =
    { $count ->
        [one] { $first } ({ $count } elemento más)
       *[other] { $first } ({ $count } elementos más)
    }

clipboard-source = Origen proporcionado por el Portapapeles del sistema

op-new-folder = Nueva carpeta | { $path }

op-new-file = Nuevo archivo | { $path }

op-rename = Cambiar nombre | { $from } → { $to }

op-chmod = Cambiar permisos | { $path } → { $mode }

op-copy-route = Copiar { $count } elementos | { $source } → { $destination }

op-move-route = Mover { $count } elementos | { $source } → { $destination }

op-recycle-route = Papelera { $count } elementos | { $source }

op-permanent-delete-route = Eliminar definitivamente { $count } elementos | { $source }

op-gdrive-trash-route = Papelera de Google Drive { $count } elementos | { $source }

op-shortcut-route = Crear acceso directo { $count } elementos | { $source }

op-preparing-copy = Preparando la copia

op-copying = Copiando

op-copy-complete = Copia completada

op-preparing-move = Preparando el traslado

op-moving = Moviendo

op-move-complete = Traslado completado

op-preparing = Preparando

op-processing = En curso

op-complete = Listo

op-finalizing = Finalizando

op-progress-items = { $summary } | { $phase } | Progreso { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Progreso { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Progreso { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Listo

op-cancelled = { $summary } | Cancelado

op-failed = { $summary } | Error: { $error }

op-partial = { $summary } | Parcial: { $succeeded }/{ $total } correctos

op-cancelling = { $summary } | Cancelando

op-success-route = Correcto | { $route }

op-skipped-route = Omitido | { $route }

op-cancelled-route = Cancelado | { $route }

op-partial-status = Parcial

op-failed-status = Error

op-error-code =  | Código de error { $code }

op-no-destination = Destino no proporcionado

op-no-source = Origen no proporcionado

apk-installing = Instalando: { $name } → { $target }

apk-installed = Instalado: { $name } → { $target }

apk-cancelled = Se canceló la instalación de { $name } en { $target }

apk-timeout = Se agotó el tiempo de instalación de { $name } en { $target }

apk-failed = Error al instalar { $name } en { $target }: { $error }

status-no-selection = No hay elementos seleccionados

status-details-unavailable = No se pudieron cargar los detalles

status-item-count =
    { $count ->
        [one] { $count } elemento
       *[other] { $count } elementos
    }

status-preview-select-one = Seleccione un elemento para obtener una vista previa

status-preview-failed = No se pudo generar una vista previa de este archivo

status-preview-loading = Cargando vista previa…

status-preview-item-failed = No se pudo cargar el elemento de vista previa

status-preview-select-single = Seleccione un único elemento para obtener una vista previa

status-no-transfers = No hay operaciones de archivo en esta sesión

status-thumbnail-cleared = Caché de miniaturas borrada

status-thumbnail-clear-partial = No se pudo borrar por completo la caché de miniaturas; puede reintentar y la exploración sigue funcionando

status-invalid-folder-name = Nombre de carpeta no válido. Corríjalo e inténtelo de nuevo.

status-name-conflict = Ya existe un elemento con ese nombre.

status-context-menu-unresponsive = El menú contextual no respondió. Puede seguir trabajando.

status-cannot-go-forward = No se puede avanzar

status-partial-folders = No se pudieron enumerar algunas carpetas.

status-cannot-list-folder = No se pudo enumerar la carpeta.

status-cannot-cancel-disconnected = No se puede cancelar: el servicio de archivos no está conectado.

status-cannot-cancel-operation = No se pudo cancelar la operación de archivo: { $error }

status-cannot-load-folder = No se pudo cargar la carpeta. Inténtelo de nuevo.

status-drive-free-of = { $free } libres de { $total }

status-no-media = No hay ningún medio

status-disconnected = Desconectado

status-access-denied = Acceso denegado

status-capacity-unavailable = Capacidad no disponible

status-waiting-file-count = Esperando File Count…

status-file-count-limit = Depende de File Count, por lo que no se inició

status-file-count-over-limit = File Count superó el límite, por lo que no se inició

status-file-count-pending-label = Limit

transfer-upload = Carga en destino

transfer-download = Descarga de origen

transfer-cancelling = Cancelando

transfer-cancel = Cancelar
