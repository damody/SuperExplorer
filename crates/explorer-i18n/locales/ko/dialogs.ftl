# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = 폴더 옵션

dialog-about = SuperExplorer 정보

dialog-version = 버전

dialog-build-date = 빌드 날짜

dialog-author = 작성자

dialog-purpose = 용도: { $value }

dialog-author-line = 작성자: { $name } — { $bio } · { $date }

dialog-community = 커뮤니티: { $url }

dialog-plugin-safe-mode-title = 플러그인 안전 모드

dialog-plugin-safe-mode-body = 플러그인 안전 모드가 켜져 있습니다. 다시 사용할 플러그인을 선택한 다음 적용 또는 확인을 누르고 SuperExplorer를 다시 시작하세요.

dialog-safe-mode-confirm-title = 안전 모드에는 확인이 필요함

dialog-safe-mode-confirm-aria = 안전 모드 확인 필요; 의심스러운 패키지: { $package }

dialog-suspect-package = 의심스러운 패키지: { $package }

dialog-interface = 인터페이스: { $value }

dialog-operation = 작업: { $value }

dialog-confirm-reenable = 확인하고 다시 사용

dialog-bookmark-action = 책갈피 동작

dialog-delete-bookmark = 책갈피 삭제

dialog-delete-bookmark-prompt = 책갈피 “{ $name }”을(를) 삭제할까요?

dialog-delete-bookmark-note = 책갈피가 제거됩니다. 디스크의 파일은 삭제되지 않습니다.

dialog-delete-bookmark-folder = 책갈피 폴더 삭제

dialog-delete-bookmark-folder-prompt = 책갈피 폴더 “{ $name }”을(를) 삭제할까요?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] 폴더와 그 안의 항목 { $count }개가 제거됩니다. 디스크의 파일은 삭제되지 않습니다.
       *[other] 폴더와 그 안의 항목 { $count }개가 제거됩니다. 디스크의 파일은 삭제되지 않습니다.
    }

dialog-rename-bookmark-folder = 책갈피 폴더 이름 바꾸기

dialog-bookmark-library = 라이브러리

dialog-new-shortcut = 새 바로 가기

dialog-new-remote-shortcut = 새 원격 바로 가기

dialog-shortcut-name = 바로 가기 이름

dialog-shortcut-target = 대상 경로

dialog-create-remote-shortcut = 원격 바로 가기 만들기

dialog-cancel-new-shortcut = 새 바로 가기 취소

dialog-properties = { $name } - 속성

dialog-properties-aria = { $name } 속성

dialog-general = 일반

dialog-file-type = 유형: { $value }

dialog-location = 위치: { $value }

dialog-size = 크기: { $value }

dialog-date-created = 만든 날짜: { $value }

dialog-date-modified = 수정한 날짜: { $value }

dialog-permissions = 사용 권한: { $mode }

dialog-file-in-use = 파일이 사용 중임

dialog-items-in-use = 일부 항목이 사용 중임

dialog-lock-owner-body =
    { $count ->
        [one] 다른 애플리케이션이 사용 중이므로 Windows가 선택한 항목 { $count }개를 삭제할 수 없습니다.
       *[other] 다른 애플리케이션이 사용 중이므로 Windows가 선택한 항목 { $count }개를 삭제할 수 없습니다.
    }

dialog-apps-using-file = 파일을 사용 중인 애플리케이션

dialog-close-results = 애플리케이션 닫기 결과

dialog-new-bookmark = 새 책갈피

dialog-edit-bookmark = 책갈피 편집

dialog-name-accelerator = 이름 (N)

dialog-location-accelerator = 위치 (L)

dialog-url-accelerator = URL (L)
dialog-path-accelerator = Path (U)
dialog-tags-accelerator = Tags (T)
dialog-tags-placeholder = Comma-separated tags
dialog-tags-hint = Use tags to search and organize bookmarks

dialog-show-editor-on-save = 저장 시 편집기 표시 (S)

dialog-lua-source = Lua 소스(읽기 전용 current_folder만)

dialog-folder-path-editable = 폴더 경로(편집 가능)

dialog-file-path-editable = 파일 경로(편집 가능)

dialog-root = 루트

dialog-folder-options-scrollbar = 폴더 옵션 세로 스크롤 막대

dialog-new-bookmark-default = 새 책갈피

dialog-new-folder-default = 새 폴더

dialog-new-shortcut-default = 새 바로 가기

dialog-shortcut-name-invalid = 바로 가기 이름은 현재 폴더의 유효한 이름이어야 합니다.

dialog-shortcut-target-invalid = 유효한 대상 경로를 입력하세요.

dialog-shortcut-window-closed = 기본 창이 닫혀 바로 가기를 만들 수 없습니다.

dialog-folder-changed = 현재 폴더가 변경되었습니다. 새 바로 가기 창을 다시 여세요.

dialog-remote-unavailable = 원격 서비스를 현재 사용할 수 없습니다.

dialog-shortcut-in-progress = 다른 바로 가기가 이미 만들어지고 있습니다.

dialog-allowed = 허용됨

dialog-not-allowed = 허용되지 않음

dialog-read = 읽기

dialog-write = 쓰기

dialog-execute = 실행

dialog-owner = 소유자

dialog-group = 그룹

dialog-others = 기타

dialog-remote-folder = 원격 폴더

dialog-remote-file = 원격 파일

dialog-unavailable = 사용할 수 없음

dialog-bytes = { $value }바이트

dialog-extension-author-bio = SuperExplorer 및 공식 예제 확장 작성자

dialog-extension-purpose-folder-size = 파일 크기를 표시하고 백그라운드에서 폴더 크기를 재귀적으로 합산합니다.

dialog-extension-purpose-size-map = 현재 폴더의 각 항목이 차지하는 공간을 면적 맵으로 표시합니다.

dialog-extension-purpose-tokei = Rust와 tokei로 파일 또는 폴더의 코드 줄 수를 계산합니다.

dialog-extension-purpose-lua-tokei = 코드 줄 수를 계산하는 Lua 확장 예제입니다.

dialog-extension-purpose-lock-owner = 현재 파일을 잠근 프로그램 또는 서비스를 표시합니다.

dialog-extension-purpose-exif = 사진 EXIF 촬영 데이터로 일괄 이름 바꾸기를 제안합니다.

dialog-extension-purpose-7z = 7-Zip 보관 파일을 탐색 가능한 가상 폴더로 표시합니다.

dialog-extension-purpose-bulk-folder = 사용자가 지정한 템플릿으로 한 번에 여러 폴더를 만듭니다.

dialog-extension-folder-size = 폴더 크기 열

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = 기본 코드 줄

dialog-extension-code-lines = 코드 줄

dialog-extension-lock-owner = 잠금 소유자

dialog-extension-exif = EXIF에서 이름 바꾸기

dialog-extension-7z = 7-Zip 가상 폴더

dialog-extension-bulk-folder = 대량 폴더 생성기

dialog-git-hash = Git 해시
dialog-release-date-line = 출시 날짜: { $value }
dialog-reset = 다시 설정
dialog-reset-prompt = { $label }을(를) 다시 설정할까요? 저장된 상태만 제거되며 현재 파일은 변경되지 않습니다.
dialog-reset-session-label = 저장된 창과 탭
dialog-reset-view-label = 저장된 보기 설정
dialog-reset-quick-access-label = 빠른 액세스 고정 항목
dialog-reset-all-label = 저장된 모든 Explorer 상태
dialog-permanent-delete-prompt = { $count }개 항목을 영구 삭제할까요? 이 작업은 취소할 수 없습니다.
dialog-gdrive-trash-prompt = Google 드라이브 휴지통으로 { $count }개 항목을 이동할까요? drive.google.com에서 30일 동안 복구할 수 있습니다. Windows 휴지통이 아닙니다.
dialog-permanent-delete-aria = { $count }개 항목 영구 삭제
dialog-gdrive-trash-aria = Google 드라이브 휴지통으로 { $count }개 항목 이동

ftp-sign-in = { $host }에 로그인
ftp-sign-in-unencrypted = { $host }에 로그인 — 이 연결은 암호화되지 않음
