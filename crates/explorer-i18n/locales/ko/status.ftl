# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] 항목 { $count }개 복사
       *[other] 항목 { $count }개 복사
    }

move-items =
    { $count ->
        [one] 항목 { $count }개 이동
       *[other] 항목 { $count }개 이동
    }

recycle-items =
    { $count ->
        [one] 항목 { $count }개를 휴지통으로 이동
       *[other] 항목 { $count }개를 휴지통으로 이동
    }

permanent-delete-items =
    { $count ->
        [one] 항목 { $count }개 영구 삭제
       *[other] 항목 { $count }개 영구 삭제
    }

gdrive-trash-items =
    { $count ->
        [one] 항목 { $count }개를 Google Drive 휴지통으로 이동
       *[other] 항목 { $count }개를 Google Drive 휴지통으로 이동
    }

shortcut-items =
    { $count ->
        [one] 항목 { $count }개의 바로 가기 만들기
       *[other] 항목 { $count }개의 바로 가기 만들기
    }

extra-items =
    { $count ->
        [one] { $first } (항목 { $count }개 더)
       *[other] { $first } (항목 { $count }개 더)
    }

clipboard-source = 원본은 시스템 클립보드에서 제공됨

op-new-folder = 새 폴더 | { $path }

op-new-file = 새 파일 | { $path }

op-rename = 이름 바꾸기 | { $from } → { $to }

op-chmod = 사용 권한 변경 | { $path } → { $mode }

op-copy-route = 항목 { $count }개 복사 | { $source } → { $destination }

op-move-route = 항목 { $count }개 이동 | { $source } → { $destination }

op-recycle-route = 휴지통 { $count }개 | { $source }

op-permanent-delete-route = 영구 삭제 { $count }개 | { $source }

op-gdrive-trash-route = Google Drive 휴지통 { $count }개 | { $source }

op-shortcut-route = 바로 가기 { $count }개 | { $source }

op-preparing-copy = 복사 준비 중

op-copying = 복사 중

op-copy-complete = 복사 완료

op-preparing-move = 이동 준비 중

op-moving = 이동 중

op-move-complete = 이동 완료

op-preparing = 준비 중

op-processing = 작업 중

op-complete = 완료

op-finalizing = 마무리 중

op-progress-items = { $summary } | { $phase } | 진행률 { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | 진행률 { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | 진행률 { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | 완료

op-cancelled = { $summary } | 취소됨

op-failed = { $summary } | 실패: { $error }

op-partial = { $summary } | 부분 완료: { $succeeded }/{ $total } 성공

op-cancelling = { $summary } | 취소 중

op-success-route = 성공 | { $route }

op-skipped-route = 건너뜀 | { $route }

op-cancelled-route = 취소됨 | { $route }

op-partial-status = 부분 완료

op-failed-status = 실패

op-error-code =  | 오류 코드 { $code }

op-no-destination = 대상을 제공하지 않음

op-no-source = 원본을 제공하지 않음

apk-installing = 설치 중: { $name } → { $target }

apk-installed = 설치됨: { $name } → { $target }

apk-cancelled = { $name }을(를) { $target }에 설치하는 작업이 취소됨

apk-timeout = { $name }을(를) { $target }에 설치하는 시간이 초과됨

apk-failed = { $name }을(를) { $target }에 설치하지 못함: { $error }

status-no-selection = 선택한 항목 없음

status-details-unavailable = 세부 정보를 로드할 수 없음

status-item-count =
    { $count ->
        [one] 항목 { $count }개
       *[other] 항목 { $count }개
    }

status-preview-select-one = 미리 볼 항목을 선택하세요

status-preview-failed = 이 파일의 미리 보기를 생성할 수 없음

status-preview-loading = 미리 보기 로드 중…

status-preview-item-failed = 미리 보기 항목을 로드할 수 없음

status-preview-select-single = 미리 볼 항목을 하나만 선택하세요

status-no-transfers = 이 세션 동안 파일 작업 없음

status-thumbnail-cleared = 미리 보기 캐시가 지워짐

status-thumbnail-clear-partial = 미리 보기 캐시를 완전히 지우지 못했습니다. 다시 시도할 수 있으며 탐색은 계속 작동합니다

status-invalid-folder-name = 폴더 이름이 잘못되었습니다. 수정한 후 다시 시도하세요.

status-name-conflict = 같은 이름의 항목이 이미 있습니다.

status-context-menu-unresponsive = 바로 가기 메뉴가 응답하지 않았습니다. 계속 작업할 수 있습니다.

status-cannot-go-forward = 앞으로 이동할 수 없음

status-partial-folders = 일부 폴더를 나열할 수 없습니다.

status-cannot-list-folder = 폴더를 나열할 수 없습니다.

status-cannot-cancel-disconnected = 취소할 수 없음: 파일 서비스가 연결되지 않았습니다.

status-cannot-cancel-operation = 파일 작업을 취소할 수 없음: { $error }

status-cannot-load-folder = 폴더를 로드할 수 없습니다. 다시 시도하세요.

status-drive-free-of = { $total } 중 { $free } 사용 가능

status-no-media = 미디어 없음

status-disconnected = 연결이 끊어짐

status-access-denied = 액세스가 거부됨

status-capacity-unavailable = 용량을 사용할 수 없음

status-waiting-file-count = File Count 대기 중…

status-file-count-limit = File Count에 의존하므로 시작되지 않음

status-file-count-over-limit = File Count가 한도를 초과하여 시작되지 않음

status-file-count-pending-label = Limit

transfer-upload = 대상 업로드

transfer-download = 원본 다운로드

transfer-cancelling = 취소 중

transfer-cancel = 취소

status-details-view = 자세히 보기
status-icon-view = 아이콘 보기
status-error = 오류 · 
status-loading = 로드 중 · 
status-items-selected = { $count }개 항목 — { $selected }개 선택됨
status-operation-progress = 작업 { $completed }/{ $total } · { $name }
status-search-cancelled = 검색 취소됨 · 
status-search-error = 검색 오류 · 
status-search-fallback = 검색 중(파일 시스템 대체, 인덱스 사용 불가) · 
status-search-indexed = 검색 중(인덱스 + 대체) · 
status-search-partial = 일부 검색 결과 · 
status-search-results = 검색 결과 · 
status-lock-close-cancelled = 애플리케이션 닫기가 취소되었습니다.
status-lock-discovery-timeout = 잠금 소유자 검색이 제한 시간에 도달했습니다.
status-lock-finding = 선택한 항목을 사용하는 애플리케이션을 찾는 중…
status-lock-owners-found = { $count }개 애플리케이션이 선택한 항목을 사용 중입니다.
status-lock-partial-close = 일부 애플리케이션이 닫히지 않았습니다. 프로세스를 강제 종료하지 않았습니다.
status-lock-retry-limit = 다시 시도 한도에 도달했습니다. 항목이 삭제되지 않았습니다.
status-lock-retrying = 삭제 작업을 다시 시도하는 중…
status-lock-unidentified = Windows가 이 항목을 사용하는 애플리케이션을 식별하지 못했습니다.
transfer-cancel-operation = 파일 작업 취소
transfer-open-location = 전송 위치 열기
