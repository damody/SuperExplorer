//! Cross-platform child-process creation policy.

use std::process::Command;

/// Configures a product-owned background child process.
///
/// Windows console-subsystem programs must not inherit or create a console for
/// internal work. Other platforms keep the command unchanged.
#[cfg(windows)]
pub fn configure_background_command(command: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt as _;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW)
}

/// Configures a product-owned background child process.
#[cfg(not(windows))]
pub const fn configure_background_command(command: &mut Command) -> &mut Command {
    command
}

#[cfg(all(test, windows))]
mod tests {
    use std::process::{Command, Stdio};

    use super::configure_background_command;

    const CHILD_MARKER: &str = "SUPEREXPLORER_BACKGROUND_COMMAND_TEST_CHILD";

    #[allow(unsafe_code)]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleWindow() -> *mut core::ffi::c_void;
    }

    #[test]
    #[allow(unsafe_code)]
    fn configured_console_child_has_no_console_and_keeps_captured_output() {
        if std::env::var_os(CHILD_MARKER).is_some() {
            // SAFETY: GetConsoleWindow takes no arguments and returns a borrowed HWND.
            assert!(unsafe { GetConsoleWindow() }.is_null());
            println!("background-output-captured");
            return;
        }

        let executable = std::env::current_exe().expect("current test executable");
        let mut command = Command::new(executable);
        command
            .args([
                "--exact",
                "process::tests::configured_console_child_has_no_console_and_keeps_captured_output",
                "--nocapture",
            ])
            .env(CHILD_MARKER, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_background_command(&mut command);
        let output = command.output().expect("start hidden test child");

        assert!(
            output.status.success(),
            "hidden child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("background-output-captured"));
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use std::process::Command;

    #[test]
    fn background_configuration_is_a_noop() {
        let mut command = Command::new("true");
        let address_before = (&raw const command).cast::<()>();
        let configured = super::configure_background_command(&mut command);
        let address_after = (&raw const *configured).cast::<()>();
        assert_eq!(address_before, address_after);
    }
}
