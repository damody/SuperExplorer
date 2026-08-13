#![allow(
    clippy::items_after_statements,
    reason = "Windows process-boundary fixtures keep platform-only imports and helpers beside the tests that own them"
)]

use std::time::Duration;

use explorer_extension_broker::{BrokerClient, BrokerPolicy};
use explorer_extension_protocol::{ContextMenuPayload, MessageKind, OperationClass, StartPayload};

fn isolate_real_process_boundary() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn client(timeout: Duration) -> BrokerClient {
    BrokerClient::new(
        env!("CARGO_BIN_EXE_explorer-extension-broker"),
        BrokerPolicy {
            operation_timeout: timeout,
            ..BrokerPolicy::default()
        },
    )
}

fn invoke_disposable_worker_without_job(payload: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_explorer-extension-worker"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn direct disposable worker");
    let mut input = child.stdin.take().expect("direct worker stdin");
    input
        .write_all(
            &u32::try_from(payload.len())
                .expect("payload length")
                .to_le_bytes(),
        )
        .expect("direct worker length");
    input.write_all(payload).expect("direct worker payload");
    drop(input);
    let output = child.wait_with_output().expect("direct worker terminal");
    assert!(output.status.success(), "direct worker status: {output:?}");
    output.stdout
}

#[test]
fn missing_and_wrong_role_binaries_fail_closed_before_provider_activation() {
    let _process_boundary = isolate_real_process_boundary();
    let missing = BrokerClient::new(
        std::env::temp_dir().join("definitely-missing-explorer-broker.exe"),
        BrokerPolicy::default(),
    );
    assert_eq!(
        missing.verify(),
        Err(explorer_extension_broker::BrokerClientError::Unavailable)
    );
    let worker = BrokerClient::new(
        env!("CARGO_BIN_EXE_explorer-extension-worker"),
        BrokerPolicy::default(),
    );
    assert_eq!(
        worker.verify(),
        Err(explorer_extension_broker::BrokerClientError::VersionMismatch)
    );
}

#[cfg(windows)]
#[test]
fn tampered_access_denied_and_startup_exit_binaries_fail_closed_without_a_session() {
    let _process_boundary = isolate_real_process_boundary();
    use std::os::windows::fs::OpenOptionsExt as _;

    let fixture = tempfile::tempdir().expect("fault-injection directory");
    let source = std::path::Path::new(env!("CARGO_BIN_EXE_explorer-extension-broker"));

    let tampered = fixture.path().join("tampered-broker.exe");
    std::fs::write(&tampered, b"MZ\0tampered-not-a-valid-pe")
        .expect("write bounded tampered image");
    let tampered_client = BrokerClient::new(&tampered, BrokerPolicy::default());
    assert_eq!(
        tampered_client.verify(),
        Err(explorer_extension_broker::BrokerClientError::Start)
    );
    assert_eq!(
        tampered_client.lifecycle_snapshot(),
        explorer_extension_broker::BrokerLifecycleSnapshot::default(),
        "a rejected image must not publish an active broker generation"
    );

    let locked = fixture.path().join("access-denied-broker.exe");
    std::fs::copy(source, &locked).expect("copy broker for share-denied injection");
    let exclusive = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&locked)
        .expect("hold exclusive image handle");
    let locked_client = BrokerClient::new(&locked, BrokerPolicy::default());
    assert_eq!(
        locked_client.verify(),
        Err(explorer_extension_broker::BrokerClientError::Start)
    );
    assert!(!locked_client.lifecycle_snapshot().active);
    drop(exclusive);

    let startup_exit = std::path::Path::new(r"C:\Windows\System32\where.exe");
    if startup_exit.is_file() {
        let startup_client = BrokerClient::new(
            startup_exit,
            BrokerPolicy {
                ready_timeout: Duration::from_secs(1),
                ..BrokerPolicy::default()
            },
        );
        assert_eq!(
            startup_client.verify(),
            Err(explorer_extension_broker::BrokerClientError::VersionMismatch),
            "a valid process that exits before the authenticated handshake must fail closed"
        );
        let snapshot = startup_client.lifecycle_snapshot();
        assert!(!snapshot.active);
        assert_eq!(snapshot.handshakes, 0);
    }
}

fn controlled(mode: &str) -> Vec<u8> {
    StartPayload {
        operation: OperationClass::Preview,
        flags: 1,
        descriptor: mode.as_bytes().to_vec(),
    }
    .encode()
    .expect("controlled payload")
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "enumerating visible top-level windows for owned helper PIDs requires Win32 callbacks"
)]
fn visible_top_level_window_count(process_id: u32) -> usize {
    use windows::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible},
    };
    use windows::core::BOOL;

    unsafe extern "system" fn inspect(hwnd: HWND, parameter: LPARAM) -> BOOL {
        let query = unsafe { &mut *(parameter.0 as *mut (u32, usize)) };
        let mut owner = 0_u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut owner)) };
        if owner == query.0 && unsafe { IsWindowVisible(hwnd).as_bool() } {
            query.1 = query.1.saturating_add(1);
        }
        true.into()
    }

    let mut query = (process_id, 0_usize);
    let _ = unsafe {
        EnumWindows(
            Some(inspect),
            LPARAM((&raw mut query).cast::<core::ffi::c_void>() as isize),
        )
    };
    query.1
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "the headful regression identifies the visible Shell popup owned by its disposable worker"
)]
fn visible_window_thread(process_id: u32) -> Option<u32> {
    use windows::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible},
    };
    use windows::core::BOOL;

    unsafe extern "system" fn inspect(hwnd: HWND, parameter: LPARAM) -> BOOL {
        let query = unsafe { &mut *(parameter.0 as *mut (u32, u32)) };
        let mut owner = 0_u32;
        let thread = unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut owner)) };
        if owner == query.0 && unsafe { IsWindowVisible(hwnd).as_bool() } {
            query.1 = thread;
            return false.into();
        }
        true.into()
    }

    let mut query = (process_id, 0_u32);
    let _ = unsafe {
        EnumWindows(
            Some(inspect),
            LPARAM((&raw mut query).cast::<core::ffi::c_void>() as isize),
        )
    };
    (query.1 != 0).then_some(query.1)
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "the recovery test terminates only the broker PID created and reported by its own client"
)]
fn terminate_owned_process(process_id: u32) {
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
    let process = unsafe { OpenProcess(PROCESS_TERMINATE, false, process_id) }
        .expect("open owned broker process");
    unsafe { TerminateProcess(process, 91) }.expect("terminate owned broker process");
    let _ = unsafe { windows::Win32::Foundation::CloseHandle(process) };
}

#[test]
fn version_handshake_normal_and_crash_are_process_isolated() {
    let _process_boundary = isolate_real_process_boundary();
    let client = client(Duration::from_secs(5));
    client.verify().expect("compatible broker");
    let initial = client.lifecycle_snapshot();
    assert!(initial.active);
    assert_eq!(initial.broker_launches, 1);
    assert_eq!(initial.handshakes, 1);
    let normal = client
        .invoke(MessageKind::Start, controlled("normal"))
        .expect("normal response");
    assert_eq!(normal.kind, MessageKind::Terminal);
    assert_eq!(normal.payload, b"success");

    let crash = client
        .invoke(MessageKind::Start, controlled("crash"))
        .expect("crash terminal");
    assert_eq!(crash.kind, MessageKind::Terminal);
    assert_eq!(crash.payload, b"worker-crash");
    let after = client.lifecycle_snapshot();
    assert_eq!(after.broker_pid, initial.broker_pid);
    assert_eq!(after.broker_launches, 1);
    assert_eq!(after.handshakes, 1);
    assert_eq!(after.requests, 2);
}

#[test]
fn persistent_client_clones_reuse_one_broker_and_shutdown_is_idempotent() {
    let _process_boundary = isolate_real_process_boundary();
    let client = client(Duration::from_secs(5));
    let clone = client.clone();
    client.verify().expect("compatible persistent broker");
    let first_pid = client
        .lifecycle_snapshot()
        .broker_pid
        .expect("active broker pid");

    let first = client
        .invoke(MessageKind::Start, controlled("normal"))
        .expect("first worker terminal");
    let first_worker = first.feature_bits;
    assert_ne!(first_worker, 0);
    let second = clone
        .invoke(MessageKind::Start, controlled("normal"))
        .expect("second worker terminal");
    assert_ne!(second.feature_bits, 0);
    assert_ne!(
        second.feature_bits, first_worker,
        "handler-loaded workers must never be reused"
    );

    let warm = clone.lifecycle_snapshot();
    assert_eq!(warm.broker_pid, Some(first_pid));
    assert_eq!(warm.broker_launches, 1);
    assert_eq!(warm.handshakes, 1);
    assert_eq!(warm.requests, 2);
    assert_eq!(warm.worker_pid, Some(second.feature_bits));

    client.shutdown();
    clone.shutdown();
    let stopped = clone.lifecycle_snapshot();
    assert!(!stopped.active);
    assert_eq!(stopped.shutdowns, 1);

    clone.verify().expect("later generation restarts");
    let restarted = clone.lifecycle_snapshot();
    assert_ne!(restarted.broker_pid, Some(first_pid));
    assert_eq!(restarted.broker_launches, 2);
    assert_eq!(restarted.handshakes, 2);
    assert_eq!(restarted.generation, 2);
}

#[test]
fn context_menu_concurrent_async_clients_share_one_supervisor_and_serialize_disposable_workers() {
    let _process_boundary = isolate_real_process_boundary();
    let client = client(Duration::from_secs(5));
    client.verify().expect("compatible persistent broker");
    let broker_pid = client
        .lifecycle_snapshot()
        .broker_pid
        .expect("one warm supervisor");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(5));
    let requests = (0..4)
        .map(|_| {
            let client = client.clone();
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                client
                    .invoke(MessageKind::Start, controlled("normal"))
                    .expect("async request terminal")
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    for request in requests {
        assert_eq!(
            request.join().expect("async request thread").payload,
            b"success"
        );
    }
    let snapshot = client.lifecycle_snapshot();
    assert_eq!(snapshot.broker_pid, Some(broker_pid));
    assert_eq!(snapshot.broker_launches, 1);
    assert_eq!(snapshot.handshakes, 1);
    assert_eq!(snapshot.requests, 4);
}

#[test]
fn worker_failure_does_not_restart_persistent_supervisor() {
    let _process_boundary = isolate_real_process_boundary();
    let client = client(Duration::from_secs(5));
    let crash = client
        .invoke(MessageKind::Start, controlled("crash"))
        .expect("worker crash terminal");
    assert_eq!(crash.payload, b"worker-crash");
    let broker_pid = client.lifecycle_snapshot().broker_pid;
    let recovery = client
        .invoke(MessageKind::Start, controlled("normal"))
        .expect("later worker succeeds");
    assert_eq!(recovery.payload, b"success");
    let snapshot = client.lifecycle_snapshot();
    assert_eq!(snapshot.broker_pid, broker_pid);
    assert_eq!(snapshot.broker_launches, 1);
    assert_eq!(snapshot.restarts, 0);
}

#[test]
fn shutdown_interrupts_an_active_worker_without_waiting_for_operation_timeout() {
    let _process_boundary = isolate_real_process_boundary();
    let client = client(Duration::from_secs(30));
    client.verify().expect("broker handshake");
    let request_client = client.clone();
    let request =
        std::thread::spawn(move || request_client.invoke(MessageKind::Start, controlled("hang")));
    std::thread::sleep(Duration::from_millis(250));
    let started = std::time::Instant::now();
    client.shutdown();
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(request.join().expect("request thread").is_err());
    assert!(!client.lifecycle_snapshot().active);
}

#[cfg(windows)]
#[test]
fn persistent_broker_and_active_worker_never_create_visible_top_level_windows() {
    let _process_boundary = isolate_real_process_boundary();
    let client = client(Duration::from_secs(30));
    client.verify().expect("broker handshake");
    let broker_pid = client.lifecycle_snapshot().broker_pid.expect("broker pid");
    assert_eq!(visible_top_level_window_count(broker_pid), 0);

    let request_client = client.clone();
    let request =
        std::thread::spawn(move || request_client.invoke(MessageKind::Start, controlled("hang")));
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let worker_pid = loop {
        if let Some(worker_pid) = client.lifecycle_snapshot().worker_pid {
            break worker_pid;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "worker pid publication timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(visible_top_level_window_count(broker_pid), 0);
    assert_eq!(visible_top_level_window_count(worker_pid), 0);

    client.shutdown();
    assert!(request.join().expect("request thread").is_err());
}

#[cfg(windows)]
#[test]
#[ignore = "requires an interactive Windows desktop because it opens and cancels a real Shell menu"]
#[allow(
    unsafe_code,
    reason = "the headful regression posts Escape only to the observed popup worker thread"
)]
fn brokered_real_popup_appears_without_console_and_cancels_cleanly() {
    let _process_boundary = isolate_real_process_boundary();
    use windows::Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::WindowsAndMessaging::{PostThreadMessageW, WM_KEYDOWN, WM_KEYUP},
    };

    let fixture = tempfile::tempdir().expect("popup fixture");
    let request = explorer_model::ContextMenuRequest {
        target: explorer_model::ShellContextMenuTarget::Background {
            parent: explorer_model::LocationDescriptor::file_system(fixture.path()),
        },
        owner_window: 0,
        point: explorer_model::MenuPoint { x: 40, y: 40 },
        keyboard_invoked: false,
        invocation_profile: explorer_model::ContextMenuInvocationProfile::Explorer,
        requested_verb: None,
        deadline_ms: 5_000,
    };
    let client = BrokerClient::new(
        env!("CARGO_BIN_EXE_explorer-extension-broker"),
        BrokerPolicy {
            interactive_timeout: Duration::from_secs(8),
            ..BrokerPolicy::default()
        },
    );
    client.verify().expect("warm broker");
    let broker_pid = client.lifecycle_snapshot().broker_pid.expect("broker pid");
    let request_client = client.clone();
    let cancellation = explorer_model::CancellationToken::new();
    let result =
        std::thread::spawn(move || request_client.show_context_menu(&request, &cancellation));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let (worker_pid, popup_thread) = loop {
        if let Some(worker_pid) = client.lifecycle_snapshot().worker_pid
            && let Some(thread_id) = visible_window_thread(worker_pid)
        {
            break (worker_pid, thread_id);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "brokered Shell popup did not become visible"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(visible_top_level_window_count(broker_pid), 0);
    assert_eq!(visible_top_level_window_count(worker_pid), 1);
    unsafe {
        PostThreadMessageW(popup_thread, WM_KEYDOWN, WPARAM(27), LPARAM(0)).expect("Escape down");
        PostThreadMessageW(popup_thread, WM_KEYUP, WPARAM(27), LPARAM(0)).expect("Escape up");
    }
    assert_eq!(
        result
            .join()
            .expect("popup request thread")
            .expect("popup terminal"),
        explorer_model::ContextMenuOutcome::Cancelled
    );
    assert_eq!(client.lifecycle_snapshot().broker_launches, 1);
    client.shutdown();
}

#[cfg(windows)]
#[test]
fn unexpected_broker_exit_is_reaped_and_a_later_request_starts_one_new_generation() {
    let _process_boundary = isolate_real_process_boundary();
    let client = client(Duration::from_secs(5));
    client.verify().expect("initial generation");
    let first_pid = client
        .lifecycle_snapshot()
        .broker_pid
        .expect("initial broker pid");
    terminate_owned_process(first_pid);
    std::thread::sleep(Duration::from_millis(100));

    let response = client
        .invoke(MessageKind::Start, controlled("normal"))
        .expect("later generation request");
    assert_eq!(response.payload, b"success");
    let recovered = client.lifecycle_snapshot();
    assert_ne!(recovered.broker_pid, Some(first_pid));
    assert_eq!(recovered.generation, 2);
    assert_eq!(recovered.broker_launches, 2);
    assert_eq!(recovered.handshakes, 2);
    assert_eq!(recovered.restarts, 1);
}

#[test]
fn hung_worker_is_force_terminated_by_supervisor_deadline() {
    let _process_boundary = isolate_real_process_boundary();
    let started = std::time::Instant::now();
    let response = client(Duration::from_secs(5))
        .invoke(MessageKind::Start, controlled("hang"))
        .expect("timeout terminal");
    assert_eq!(response.payload, b"timeout");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn controlled_failure_matrix_always_returns_one_correlated_terminal() {
    let _process_boundary = isolate_real_process_boundary();
    let client = client(Duration::from_secs(5));
    for mode in [
        "reentrant",
        "oversized",
        "privilege",
        "late-terminal",
        "unload-failure",
    ] {
        let response = client
            .invoke(MessageKind::Start, controlled(mode))
            .expect("failure terminal");
        assert_eq!(response.kind, MessageKind::Terminal);
        assert_eq!(response.payload, b"worker-crash", "mode {mode}");
    }
    let child = client
        .invoke(MessageKind::Start, controlled("child-process"))
        .expect("child prevention terminal");
    assert_eq!(child.kind, MessageKind::Terminal);
    assert_eq!(
        child.payload, b"worker-crash",
        "the controlled worker observes that ordinary child launch no longer fails with quota; the supervisor still returns one bounded terminal"
    );
}

#[test]
fn real_preview_lookup_and_initialization_stay_in_disposable_worker() {
    let _process_boundary = isolate_real_process_boundary();
    let fixture = tempfile::Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("text preview fixture");
    std::fs::write(fixture.path(), b"brokered preview fixture").expect("fixture content");
    let payload = StartPayload {
        operation: OperationClass::Preview,
        flags: 0,
        descriptor: fixture.path().to_string_lossy().as_bytes().to_vec(),
    }
    .encode()
    .expect("preview request");
    let response = client(Duration::from_secs(10))
        .invoke(MessageKind::Start, payload)
        .expect("correlated preview terminal");
    assert_eq!(response.kind, MessageKind::Terminal);
    assert!(
        response.payload.starts_with(b"preview-ready:")
            || response.payload == b"preview-unavailable",
        "installed handler inventory must produce a truthful ready/unavailable terminal"
    );
}

#[cfg(windows)]
#[test]
#[allow(
    unsafe_code,
    reason = "the integration fixture owns one hidden STATIC HWND used only as a preview boundary"
)]
fn persistent_preview_session_attaches_resizes_focuses_and_unloads_by_generation() {
    let _process_boundary = isolate_real_process_boundary();
    use explorer_extension_protocol::PreviewMessage;
    use windows::Win32::UI::{
        HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
        WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, GetMessageW, MSG, PostMessageW,
            TranslateMessage, WINDOW_EX_STYLE, WM_APP, WS_OVERLAPPED,
        },
    };
    use windows::core::w;

    let fixture = tempfile::Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("preview session fixture");
    std::fs::write(fixture.path(), b"persistent preview session").expect("preview input");
    let location = explorer_model::LocationDescriptor::file_system(fixture.path());
    if explorer_shell_win::PreviewLookup::for_location(&location).is_err() {
        eprintln!("SKIP: no registered .txt Preview Handler on this Windows image");
        return;
    }
    let (window_sender, window_receiver) = std::sync::mpsc::sync_channel(1);
    let window_thread = std::thread::spawn(move || {
        let previous_dpi =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let parent = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("preview boundary"),
                WS_OVERLAPPED,
                0,
                0,
                800,
                600,
                None,
                None,
                None,
                None,
            )
        }
        .expect("preview boundary HWND");
        window_sender
            .send(parent.0 as isize)
            .expect("publish preview HWND");
        let mut message = MSG::default();
        while unsafe { GetMessageW(&raw mut message, None, 0, 0) }.as_bool() {
            if message.message == WM_APP + 1 {
                break;
            }
            let _ = unsafe { TranslateMessage(&raw const message) };
            unsafe { DispatchMessageW(&raw const message) };
        }
        let _ = unsafe { DestroyWindow(parent) };
        let _ = unsafe { SetThreadDpiAwarenessContext(previous_dpi) };
    });
    let parent = window_receiver.recv().expect("preview HWND");
    let generation = explorer_model::Generation::new(41);
    let bounds = explorer_model::PreviewHostBounds {
        generation,
        left_physical: 0,
        top_physical: 0,
        width_physical: 640,
        height_physical: 360,
        dpi: 96,
    };
    let broker = client(Duration::from_secs(10));
    let cancellation = explorer_model::CancellationToken::new();
    let mode = broker
        .start_preview_session(&location, parent, bounds, &cancellation)
        .expect("persistent preview start");
    assert!(matches!(
        mode,
        explorer_model::PreviewInitializationMode::File
            | explorer_model::PreviewInitializationMode::Stream
            | explorer_model::PreviewInitializationMode::ShellItem
    ));
    broker
        .preview_session_command(&PreviewMessage::SetBounds {
            generation: generation.value(),
            left: 8,
            top: 12,
            width: 600,
            height: 320,
            dpi: 96,
        })
        .expect("preview resize");
    let _ = broker.preview_session_command(&PreviewMessage::SetFocus {
        generation: generation.value(),
    });
    assert!(
        broker
            .preview_session_command(&PreviewMessage::SetBounds {
                generation: generation.value() - 1,
                left: 0,
                top: 0,
                width: 320,
                height: 200,
                dpi: 96,
            })
            .is_err(),
        "stale preview bounds must not reach the active handler"
    );
    broker
        .preview_session_command(&PreviewMessage::Unload {
            generation: generation.value(),
        })
        .expect("preview unload");
    assert!(
        broker
            .preview_session_command(&PreviewMessage::Unload {
                generation: generation.value(),
            })
            .is_err(),
        "unloaded session must not accept another worker command"
    );
    broker.shutdown();
    let _ = unsafe {
        PostMessageW(
            Some(windows::Win32::Foundation::HWND(parent as *mut _)),
            WM_APP + 1,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
        )
    };
    window_thread.join().expect("preview window thread");
}

#[test]
fn repeated_preview_activation_failures_are_quarantined_before_a_third_worker_starts() {
    let _process_boundary = isolate_real_process_boundary();
    let fixture = tempfile::Builder::new()
        .suffix(".rust-explorer-no-preview-handler")
        .tempfile()
        .expect("unregistered preview fixture");
    let payload = StartPayload {
        operation: OperationClass::Preview,
        flags: 0,
        descriptor: fixture.path().to_string_lossy().as_bytes().to_vec(),
    }
    .encode()
    .expect("preview request");
    let client = client(Duration::from_secs(10));
    for attempt in 0..2 {
        let response = client
            .invoke(MessageKind::Start, payload.clone())
            .expect("typed preview failure");
        assert_eq!(
            response.payload, b"preview-unavailable",
            "attempt {attempt}"
        );
    }
    let workers_before = client.lifecycle_snapshot().worker_pid;
    let quarantined = client
        .invoke(MessageKind::Start, payload)
        .expect("quarantine terminal");
    assert_eq!(quarantined.payload, b"preview-quarantined");
    assert_eq!(
        client.lifecycle_snapshot().worker_pid,
        workers_before,
        "quarantine must reject before a provider worker is activated"
    );
}

#[test]
fn context_thumbnail_and_namespace_routes_execute_inside_disposable_workers() {
    let _process_boundary = isolate_real_process_boundary();
    let fixture = tempfile::Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("extension fixture");
    std::fs::write(fixture.path(), b"broker operation fixture").expect("fixture content");
    for (operation, descriptor, accepted) in [
        (
            OperationClass::ContextMenu,
            fixture.path().to_string_lossy().into_owned(),
            &["context-menu-ready:", "context-menu-unavailable"] as &[_],
        ),
        (
            OperationClass::Thumbnail,
            fixture.path().to_string_lossy().into_owned(),
            &[
                "thumbnail-ready:",
                "thumbnail-fallback:",
                "thumbnail-failed",
            ] as &[_],
        ),
        (
            OperationClass::Namespace,
            "shell:Desktop".to_owned(),
            &["namespace-ready:", "namespace-unavailable"] as &[_],
        ),
    ] {
        let payload = StartPayload {
            operation,
            flags: 256 << 16,
            descriptor: descriptor.into_bytes(),
        }
        .encode()
        .expect("operation payload");
        let response = client(Duration::from_secs(10))
            .invoke(MessageKind::Start, payload)
            .expect("correlated worker terminal");
        let text = String::from_utf8(response.payload).expect("worker status text");
        assert!(
            accepted.iter().any(|prefix| text.starts_with(prefix)),
            "unexpected {operation:?} response: {text}"
        );
    }
}

#[test]
fn brokered_context_menu_query_matches_in_process_command_count() {
    let _process_boundary = isolate_real_process_boundary();
    let fixture = tempfile::Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("context fixture");
    std::fs::write(fixture.path(), b"context fixture").expect("fixture content");
    let parent = fixture.path().parent().expect("fixture parent");
    let id = explorer_model::ShellItemId::from_provider_bytes(b"differential-item".to_vec())
        .expect("owned identity");
    let target = explorer_model::ShellContextMenuTarget::Items {
        parent: explorer_model::LocationDescriptor::file_system(parent),
        items: vec![explorer_model::ItemDescriptor {
            id,
            location: explorer_model::LocationDescriptor::file_system(fixture.path()),
        }],
    };
    let direct = explorer_shell_win::query_context_menu_in_worker(&target)
        .expect("direct public Shell query");
    let payload = StartPayload {
        operation: OperationClass::ContextMenu,
        flags: 0,
        descriptor: fixture.path().to_string_lossy().as_bytes().to_vec(),
    }
    .encode()
    .expect("context payload");
    let response = client(Duration::from_secs(10))
        .invoke(MessageKind::Start, payload)
        .expect("broker context query");
    let text = String::from_utf8(response.payload).expect("status text");
    let brokered = text
        .strip_prefix("context-menu-ready:")
        .and_then(|value| value.parse::<i32>().ok())
        .expect("broker command count");
    assert_eq!(brokered, direct);
}

#[test]
fn brokered_context_menu_profiles_are_bounded_across_all_target_shapes() {
    let _process_boundary = isolate_real_process_boundary();
    let fixture = tempfile::tempdir().expect("context fixture");
    let first = fixture.path().join("first.txt");
    let second = fixture.path().join("second.txt");
    let folder = fixture.path().join("folder");
    std::fs::write(&first, b"first").expect("first");
    std::fs::write(&second, b"second").expect("second");
    std::fs::create_dir(&folder).expect("folder");
    let parent = explorer_model::LocationDescriptor::file_system(fixture.path());
    let item = |path: &std::path::Path, id| explorer_model::ItemDescriptor {
        id: explorer_model::ShellItemId::from_provider_bytes([id]).expect("identity"),
        location: explorer_model::LocationDescriptor::file_system(path),
    };
    let targets = [
        explorer_model::ShellContextMenuTarget::Background {
            parent: parent.clone(),
        },
        explorer_model::ShellContextMenuTarget::Items {
            parent: parent.clone(),
            items: vec![item(&first, 1)],
        },
        explorer_model::ShellContextMenuTarget::Items {
            parent: parent.clone(),
            items: vec![item(&folder, 2)],
        },
        explorer_model::ShellContextMenuTarget::Items {
            parent,
            items: vec![item(&first, 3), item(&second, 4)],
        },
    ];
    let broker = client(Duration::from_secs(10));
    for target in targets {
        let mut ordinary_brokered = 0;
        let mut ordinary_direct = 0;
        for profile in [
            explorer_model::ContextMenuInvocationProfile::Explorer,
            explorer_model::ContextMenuInvocationProfile::ExplorerExtended,
        ] {
            let (background, locations) = match &target {
                explorer_model::ShellContextMenuTarget::Background { parent } => {
                    (true, vec![parent.clone()])
                }
                explorer_model::ShellContextMenuTarget::Items { items, .. } => (
                    false,
                    items.iter().map(|item| item.location.clone()).collect(),
                ),
            };
            let encode_location = |location: &explorer_model::LocationDescriptor| {
                let explorer_model::LocationDescriptor::FileSystem(path) = location else {
                    panic!("owned filesystem fixture");
                };
                let mut bytes = vec![b'F'];
                bytes.extend_from_slice(path.to_string_lossy().as_bytes());
                bytes
            };
            let descriptor = ContextMenuPayload {
                version: ContextMenuPayload::VERSION,
                background,
                owner_hwnd: 0,
                point_x: 0,
                point_y: 0,
                keyboard_invoked: false,
                invocation_profile: u8::from(profile.extended_verbs()),
                item_descriptors: locations.iter().map(encode_location).collect(),
                verb: None,
            }
            .encode()
            .expect("profile payload");
            let payload = StartPayload {
                operation: OperationClass::ContextMenu,
                // Full typed context payload plus the broker differential query-only bit.
                flags: 0xe000_0000,
                descriptor,
            }
            .encode()
            .expect("start payload");
            // Some installed lazy cascades mutate their transient labels while they are being
            // initialized. Require one exact direct/supervised snapshot pair, but allow two
            // bounded retries so the comparison is not coupled to that provider-owned phase.
            let mut matching_response = None;
            for _ in 0..3 {
                let direct_worker = invoke_disposable_worker_without_job(&payload);
                let response = broker
                    .invoke(MessageKind::Start, payload.clone())
                    .expect("brokered query");
                if response.payload == direct_worker {
                    matching_response = Some(response);
                    break;
                }
            }
            let response = matching_response.expect(
                "the supervisor changed all three bounded snapshots from the direct worker",
            );
            let status = String::from_utf8(response.payload).expect("status");
            let mut status_parts = status
                .strip_prefix("context-menu-snapshot:")
                .expect("snapshot status")
                .split(':');
            let brokered = status_parts
                .next()
                .and_then(|value| value.parse::<i32>().ok())
                .expect("command count");
            assert_eq!(
                status_parts.next(),
                Some(if profile.extended_verbs() { "1" } else { "0" })
            );
            let brokered_fingerprints = status_parts
                .next()
                .unwrap_or_default()
                .split(',')
                .filter(|value| !value.is_empty())
                .map(|value| u64::from_str_radix(value, 16).expect("label fingerprint"))
                .collect::<Vec<_>>();
            let direct = explorer_shell_win::query_context_menu_snapshot_in_worker_with_profile(
                &target, profile,
            )
            .expect("direct snapshot");
            let label_delta = {
                let mut direct_counts = std::collections::HashMap::new();
                let mut brokered_counts = std::collections::HashMap::new();
                for fingerprint in &direct.label_fingerprints {
                    *direct_counts.entry(*fingerprint).or_insert(0_usize) += 1;
                }
                for fingerprint in &brokered_fingerprints {
                    *brokered_counts.entry(*fingerprint).or_insert(0_usize) += 1;
                }
                let direct_only = direct_counts
                    .iter()
                    .map(|(fingerprint, count)| {
                        count.saturating_sub(*brokered_counts.get(fingerprint).unwrap_or(&0))
                    })
                    .sum::<usize>();
                let brokered_only = brokered_counts
                    .iter()
                    .map(|(fingerprint, count)| {
                        count.saturating_sub(*direct_counts.get(fingerprint).unwrap_or(&0))
                    })
                    .sum::<usize>();
                direct_only + brokered_only
            };
            eprintln!(
                "context-menu-differential target={target:?} profile={profile:?} brokered_count={brokered} same-worker-direct=exact process-host-count={} brokered_labels={} process-host-labels={} process-host-label-delta={label_delta}",
                direct.command_count,
                brokered_fingerprints.len(),
                direct.label_fingerprints.len()
            );
            assert!(
                direct.command_count > 0 && !direct.label_fingerprints.is_empty(),
                "the independent native host must expose a bounded non-empty baseline"
            );
            if profile == explorer_model::ContextMenuInvocationProfile::Explorer {
                ordinary_brokered = brokered;
                ordinary_direct = direct.command_count;
            } else {
                assert!(brokered >= ordinary_brokered, "broker lost extended verbs");
                assert!(
                    direct.command_count >= ordinary_direct,
                    "direct query lost extended verbs"
                );
            }
        }
    }
    broker.shutdown();
}

#[test]
fn brokered_and_direct_context_menu_preserve_safe_filesystem_effects() {
    let _process_boundary = isolate_real_process_boundary();
    fn request_for(path: &std::path::Path) -> explorer_model::ContextMenuRequest {
        let parent = path.parent().expect("owned fixture parent");
        explorer_model::ContextMenuRequest {
            target: explorer_model::ShellContextMenuTarget::Items {
                parent: explorer_model::LocationDescriptor::file_system(parent),
                items: vec![explorer_model::ItemDescriptor {
                    id: explorer_model::ShellItemId::from_provider_bytes(
                        path.as_os_str().to_string_lossy().as_bytes().to_vec(),
                    )
                    .expect("owned identity"),
                    location: explorer_model::LocationDescriptor::file_system(path),
                }],
            },
            owner_window: 0,
            point: explorer_model::MenuPoint { x: 0, y: 0 },
            keyboard_invoked: true,
            invocation_profile: explorer_model::ContextMenuInvocationProfile::Explorer,
            requested_verb: Some("Windows.CompressToZip".to_owned()),
            deadline_ms: 10_000,
        }
    }

    fn wait_for_archive(path: &std::path::Path) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !path.is_file() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(path.is_file(), "Shell compression did not create {path:?}");
        assert!(std::fs::metadata(path).expect("archive metadata").len() > 0);
    }

    let fixture = tempfile::tempdir().expect("differential effect fixture");
    let direct_dir = fixture.path().join("direct");
    let brokered_dir = fixture.path().join("brokered");
    std::fs::create_dir_all(&direct_dir).expect("direct directory");
    std::fs::create_dir_all(&brokered_dir).expect("brokered directory");
    let direct_item = direct_dir.join("effect.txt");
    let brokered_item = brokered_dir.join("effect.txt");
    std::fs::write(&direct_item, b"same controlled content").expect("direct item");
    std::fs::write(&brokered_item, b"same controlled content").expect("brokered item");

    let direct = explorer_shell_win::execute_context_menu_in_worker(&request_for(&direct_item))
        .expect("direct safe verb");
    let cancellation = explorer_model::CancellationToken::new();
    let brokered = client(Duration::from_secs(15))
        .show_context_menu(&request_for(&brokered_item), &cancellation)
        .expect("brokered safe verb");
    assert!(matches!(
        direct,
        explorer_model::ContextMenuOutcome::Invoked { .. }
    ));
    assert!(matches!(
        brokered,
        explorer_model::ContextMenuOutcome::Invoked { .. }
    ));
    wait_for_archive(&direct_dir.join("effect.zip"));
    wait_for_archive(&brokered_dir.join("effect.zip"));
    assert!(direct_item.is_file());
    assert!(brokered_item.is_file());
}

#[test]
fn cold_and_warm_context_menu_queries_record_one_persistent_broker_launch() {
    let _process_boundary = isolate_real_process_boundary();
    let fixture = tempfile::Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("latency fixture");
    std::fs::write(fixture.path(), b"context latency fixture").expect("fixture content");
    let payload = StartPayload {
        operation: OperationClass::ContextMenu,
        flags: 0,
        descriptor: fixture.path().to_string_lossy().as_bytes().to_vec(),
    }
    .encode()
    .expect("context payload");
    let client = client(Duration::from_secs(10));

    let cold_started = std::time::Instant::now();
    let cold = client
        .invoke(MessageKind::Start, payload.clone())
        .expect("cold context query");
    let cold_duration = cold_started.elapsed();
    let warm_started = std::time::Instant::now();
    let warm = client
        .invoke(MessageKind::Start, payload)
        .expect("warm context query");
    let warm_duration = warm_started.elapsed();
    assert!(cold.payload.starts_with(b"context-menu-ready:"));
    assert!(warm.payload.starts_with(b"context-menu-ready:"));
    let snapshot = client.lifecycle_snapshot();
    assert_eq!(snapshot.broker_launches, 1);
    assert_eq!(snapshot.handshakes, 1);
    assert_eq!(snapshot.requests, 2);
    assert!(cold_duration < Duration::from_secs(10));
    assert!(warm_duration < Duration::from_secs(10));
    println!(
        "broker-context-latency cold_ms={} warm_ms={} broker_pid={} broker_launches={} worker_pid={}",
        cold_duration.as_millis(),
        warm_duration.as_millis(),
        snapshot.broker_pid.unwrap_or_default(),
        snapshot.broker_launches,
        snapshot.worker_pid.unwrap_or_default()
    );
}

#[test]
fn typed_thumbnail_pixels_cross_only_the_bounded_broker_contract() {
    let _process_boundary = isolate_real_process_boundary();
    let fixture = tempfile::Builder::new()
        .suffix(".png")
        .tempfile()
        .expect("thumbnail fixture");
    // A minimal malformed image is still useful: installed providers must return either bounded
    // pixels or a typed fallback without loading inside the test/app process.
    std::fs::write(fixture.path(), b"not-a-real-png").expect("fixture content");
    let key = explorer_model::ThumbnailRequestKey {
        item_id: explorer_model::ShellItemId::from_provider_bytes(b"typed-thumbnail".to_vec())
            .expect("identity"),
        physical_size: 96,
        dpi: 96,
        mode: explorer_model::ThumbnailMode::Thumbnail,
        source_generation: 1,
        theme: explorer_model::ShellIconTheme::Light,
        association_generation: 1,
        overlay_generation: 1,
    };
    let outcome = client(Duration::from_secs(10))
        .load_thumbnail(
            &key,
            &explorer_model::LocationDescriptor::file_system(fixture.path()),
            false,
        )
        .expect("typed broker terminal");
    match outcome {
        explorer_model::ThumbnailTerminal::Ready { pixels, .. } => pixels
            .validate(explorer_extension_protocol::MAXIMUM_FRAME_BYTES)
            .expect("bounded owned pixels"),
        explorer_model::ThumbnailTerminal::Compressed { raster, .. } => assert!(
            raster.validate(explorer_extension_protocol::MAXIMUM_FRAME_BYTES),
            "bounded owned BC7 rows"
        ),
        explorer_model::ThumbnailTerminal::Fallback(_) => {}
        explorer_model::ThumbnailTerminal::Failed(detail) => {
            assert!(!detail.contains(&fixture.path().display().to_string()));
        }
    }
}

#[test]
fn typed_namespace_rows_are_owned_bounded_and_capability_preserving() {
    let _process_boundary = isolate_real_process_boundary();
    let entries = client(Duration::from_secs(10))
        .enumerate_namespace(
            &explorer_model::LocationDescriptor::ParsingName("shell:Desktop".to_owned()),
            64,
        )
        .expect("typed namespace terminal");
    assert!(entries.len() <= 64);
    for entry in entries {
        assert!(!entry.display_name.is_empty());
        assert!(entry.location.encoded_payload_len() <= 64 * 1024);
        assert_eq!(
            entry.metadata.namespace_capabilities.bits() & !((1 << 15) - 1),
            0
        );
    }
}
