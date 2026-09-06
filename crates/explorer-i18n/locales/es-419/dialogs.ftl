# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = Opciones de carpeta

dialog-about = Acerca de SuperExplorer

dialog-version = Versión

dialog-build-date = Fecha de compilación

dialog-author = Autor

dialog-purpose = Propósito: { $value }

dialog-author-line = Autor: { $name } — { $bio } · { $date }

dialog-community = Comunidad: { $url }

dialog-plugin-safe-mode-title = Modo seguro de complementos

dialog-plugin-safe-mode-body = El modo seguro de complementos está activado. Seleccione los complementos que desea volver a habilitar, elija Aplicar o Aceptar y reinicie SuperExplorer.

dialog-safe-mode-confirm-title = El modo seguro requiere confirmación

dialog-safe-mode-confirm-aria = Se requiere confirmación del modo seguro; Paquete sospechoso: { $package }

dialog-suspect-package = Paquete sospechoso: { $package }

dialog-interface = Interfaz: { $value }

dialog-operation = Operación: { $value }

dialog-confirm-reenable = Confirmar y volver a habilitar

dialog-bookmark-action = Acción de marcador

dialog-delete-bookmark = Eliminar marcador

dialog-delete-bookmark-prompt = ¿Eliminar el marcador “{ $name }”?

dialog-delete-bookmark-note = Esto quita el marcador. Los archivos del disco no se eliminan.

dialog-delete-bookmark-folder = Eliminar carpeta de marcadores

dialog-delete-bookmark-folder-prompt = ¿Eliminar la carpeta de marcadores “{ $name }”?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] Se quita la carpeta y { $count } elemento de su interior. Los archivos del disco no se eliminan.
       *[other] Se quita la carpeta y { $count } elementos de su interior. Los archivos del disco no se eliminan.
    }

dialog-rename-bookmark-folder = Cambiar nombre de carpeta de marcadores

dialog-bookmark-library = Biblioteca

dialog-new-shortcut = Nuevo acceso directo

dialog-new-remote-shortcut = Nuevo acceso directo remoto

dialog-shortcut-name = Nombre del acceso directo

dialog-shortcut-target = Ruta de destino

dialog-create-remote-shortcut = Crear acceso directo remoto

dialog-cancel-new-shortcut = Cancelar nuevo acceso directo

dialog-properties = { $name } - Propiedades

dialog-properties-aria = Propiedades de { $name }

dialog-general = General

dialog-file-type = Tipo: { $value }

dialog-location = Ubicación: { $value }

dialog-size = Tamaño: { $value }

dialog-date-created = Fecha de creación: { $value }

dialog-date-modified = Fecha de modificación: { $value }

dialog-permissions = Permisos: { $mode }

dialog-file-in-use = Archivo en uso

dialog-items-in-use = Algunos elementos están en uso

dialog-lock-owner-body =
    { $count ->
        [one] Windows no puede eliminar el { $count } elemento seleccionado porque otra aplicación lo está usando.
       *[other] Windows no puede eliminar los { $count } elementos seleccionados porque otra aplicación los está usando.
    }

dialog-apps-using-file = Aplicaciones que usan el archivo

dialog-close-results = Resultados al cerrar aplicaciones

dialog-new-bookmark = Nuevo marcador

dialog-edit-bookmark = Editar marcador

dialog-name-accelerator = Nombre (N)

dialog-location-accelerator = Ubicación (L)

dialog-url-accelerator = URL (L)

dialog-show-editor-on-save = Mostrar editor al guardar (S)

dialog-lua-source = Código fuente Lua (solo current_folder de solo lectura)

dialog-folder-path-editable = Ruta de carpeta (editable)

dialog-file-path-editable = Ruta de archivo (editable)

dialog-root = Raíz

dialog-folder-options-scrollbar = Barra de desplazamiento vertical de Opciones de carpeta

dialog-new-bookmark-default = Nuevo marcador

dialog-new-folder-default = Nueva carpeta

dialog-new-shortcut-default = Nuevo acceso directo

dialog-shortcut-name-invalid = El nombre del acceso directo debe ser un nombre válido en la carpeta actual.

dialog-shortcut-target-invalid = Escriba una ruta de destino válida.

dialog-shortcut-window-closed = La ventana principal se cerró, por lo que no se pudo crear el acceso directo.

dialog-folder-changed = La carpeta actual cambió. Vuelva a abrir la ventana Nuevo acceso directo.

dialog-remote-unavailable = El servicio remoto no está disponible en este momento.

dialog-shortcut-in-progress = Ya se está creando otro acceso directo.

dialog-allowed = Permitido

dialog-not-allowed = No permitido

dialog-read = Lectura

dialog-write = Escritura

dialog-execute = Ejecución

dialog-owner = Propietario

dialog-group = Grupo

dialog-others = Otros

dialog-remote-folder = Carpeta remota

dialog-remote-file = Archivo remoto

dialog-unavailable = No disponible

dialog-bytes = { $value } bytes

dialog-extension-author-bio = Autor de SuperExplorer y de las extensiones de ejemplo oficiales

dialog-extension-purpose-folder-size = Muestra tamaños de archivo y totaliza recursivamente el tamaño de carpeta en segundo plano.

dialog-extension-purpose-size-map = Muestra cuánto espacio ocupa cada elemento de la carpeta actual como un mapa de áreas.

dialog-extension-purpose-tokei = Cuenta líneas de código en archivos o carpetas con Rust y tokei.

dialog-extension-purpose-lua-tokei = Extensión Lua de ejemplo que cuenta líneas de código.

dialog-extension-purpose-lock-owner = Muestra el programa o servicio que actualmente bloquea un archivo.

dialog-extension-purpose-exif = Sugiere cambios de nombre por lotes a partir de datos EXIF de captura de fotos.

dialog-extension-purpose-7z = Presenta archivos 7-Zip como carpetas virtuales examinables.

dialog-extension-purpose-bulk-folder = Crea muchas carpetas a la vez a partir de una plantilla especificada por el usuario.

dialog-extension-folder-size = Columna de tamaño de carpeta

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Líneas de código principales

dialog-extension-code-lines = Líneas de código

dialog-extension-lock-owner = Propietario del bloqueo

dialog-extension-exif = Cambiar nombre desde EXIF

dialog-extension-7z = Carpeta virtual 7-Zip

dialog-extension-bulk-folder = Generador de carpetas por lotes
