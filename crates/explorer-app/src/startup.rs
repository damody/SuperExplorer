//! Ordered application startup with reverse-order failure unwinding.

use std::time::Duration;

use anyhow::Error;
use explorer_shell_win::{ShellStaError, ShellStaHandle};
use thiserror::Error as ThisError;

const SHELL_JOIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Stable application startup order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupStageKind {
    Diagnostics,
    WindowsPrerequisites,
    ShellSta,
    Gpui,
    Window,
}

const REQUIRED_ORDER: [StartupStageKind; 5] = [
    StartupStageKind::Diagnostics,
    StartupStageKind::WindowsPrerequisites,
    StartupStageKind::ShellSta,
    StartupStageKind::Gpui,
    StartupStageKind::Window,
];

/// One owned stage in the process composition root.
pub trait StartupStage: Send {
    fn kind(&self) -> StartupStageKind;

    /// Acquires this stage's owned resources.
    ///
    /// # Errors
    ///
    /// Returns the stage-specific initialization failure.
    fn start(&mut self) -> Result<(), Error>;

    /// Releases resources previously acquired by this stage.
    ///
    /// # Errors
    ///
    /// Returns the stage-specific cleanup failure after making its bounded cleanup attempt.
    fn shutdown(&mut self) -> Result<(), Error>;
}

/// A cleanup error retained while reporting the original startup failure.
#[derive(Debug)]
pub struct StageCleanupError {
    pub stage: StartupStageKind,
    pub source: Error,
}

/// Invalid composition plan.
#[derive(Debug, ThisError)]
pub enum StartupPlanError {
    #[error("startup plan must contain exactly {expected} stages, found {actual}")]
    WrongStageCount { expected: usize, actual: usize },
    #[error("startup stage {index} must be {expected:?}, found {actual:?}")]
    WrongStageOrder {
        index: usize,
        expected: StartupStageKind,
        actual: StartupStageKind,
    },
}

/// Startup failure plus any cleanup failures encountered during unwind.
#[derive(Debug, ThisError)]
#[error("startup stage {stage:?} failed: {source}")]
pub struct StartupFailure {
    pub stage: StartupStageKind,
    #[source]
    pub source: Error,
    pub cleanup_errors: Vec<StageCleanupError>,
}

/// Errors from an explicit normal shutdown.
#[derive(Debug, ThisError)]
#[error("{count} startup stage(s) failed to shut down")]
pub struct ShutdownFailure {
    pub count: usize,
    pub errors: Vec<StageCleanupError>,
}

/// Owns started application stages and enforces ordered startup/unwind.
pub struct StartupCoordinator {
    stages: Vec<Box<dyn StartupStage>>,
    started: Vec<usize>,
}

impl StartupCoordinator {
    /// Validates the complete startup plan before any side effects occur.
    ///
    /// # Errors
    ///
    /// Returns an error if a stage is missing, duplicated, or out of order.
    pub fn new(stages: Vec<Box<dyn StartupStage>>) -> Result<Self, StartupPlanError> {
        if stages.len() != REQUIRED_ORDER.len() {
            return Err(StartupPlanError::WrongStageCount {
                expected: REQUIRED_ORDER.len(),
                actual: stages.len(),
            });
        }
        for (index, (stage, expected)) in stages.iter().zip(REQUIRED_ORDER).enumerate() {
            let actual = stage.kind();
            if actual != expected {
                return Err(StartupPlanError::WrongStageOrder {
                    index,
                    expected,
                    actual,
                });
            }
        }
        Ok(Self {
            stages,
            started: Vec::with_capacity(REQUIRED_ORDER.len()),
        })
    }

    /// Starts every stage in the documented order.
    ///
    /// # Errors
    ///
    /// Returns the original stage error and preserves any reverse-order cleanup errors.
    pub fn start(&mut self) -> Result<(), StartupFailure> {
        for index in 0..self.stages.len() {
            if let Err(source) = self.stages[index].start() {
                let stage = self.stages[index].kind();
                let cleanup_errors = self.unwind();
                return Err(StartupFailure {
                    stage,
                    source,
                    cleanup_errors,
                });
            }
            self.started.push(index);
        }
        Ok(())
    }

    /// Shuts down all successfully started stages in reverse order; repeated calls are harmless.
    ///
    /// # Errors
    ///
    /// Returns every cleanup error after attempting all remaining stages.
    pub fn shutdown(&mut self) -> Result<(), ShutdownFailure> {
        let errors = self.unwind();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ShutdownFailure {
                count: errors.len(),
                errors,
            })
        }
    }

    fn unwind(&mut self) -> Vec<StageCleanupError> {
        let mut errors = Vec::new();
        while let Some(index) = self.started.pop() {
            if let Err(source) = self.stages[index].shutdown() {
                errors.push(StageCleanupError {
                    stage: self.stages[index].kind(),
                    source,
                });
            }
        }
        errors
    }
}

impl Drop for StartupCoordinator {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// Production adapter that gives the composition root sole ownership of the Shell STA.
pub struct ShellStaStage {
    starter: Option<Box<dyn FnOnce() -> Result<ShellStaHandle, ShellStaError> + Send>>,
    handle: Option<ShellStaHandle>,
}

impl ShellStaStage {
    pub fn new() -> Self {
        Self::with_starter(ShellStaHandle::start)
    }

    fn with_starter<F>(starter: F) -> Self
    where
        F: FnOnce() -> Result<ShellStaHandle, ShellStaError> + Send + 'static,
    {
        Self {
            starter: Some(Box::new(starter)),
            handle: None,
        }
    }
}

impl Default for ShellStaStage {
    fn default() -> Self {
        Self::new()
    }
}

impl StartupStage for ShellStaStage {
    fn kind(&self) -> StartupStageKind {
        StartupStageKind::ShellSta
    }

    fn start(&mut self) -> Result<(), Error> {
        let starter = self
            .starter
            .take()
            .ok_or_else(|| anyhow::anyhow!("Shell STA stage cannot be started twice"))?;
        self.handle = Some(starter()?);
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), Error> {
        if let Some(handle) = self.handle.take() {
            handle.shutdown_and_join(SHELL_JOIN_TIMEOUT)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        REQUIRED_ORDER, ShellStaStage, StartupCoordinator, StartupStage, StartupStageKind,
    };
    use anyhow::Error;
    use explorer_shell_win::ShellStaError;
    use std::sync::{Arc, Mutex};

    type Events = Arc<Mutex<Vec<String>>>;

    struct FakeStage {
        kind: StartupStageKind,
        fail_start: bool,
        fail_shutdown: bool,
        events: Events,
    }

    impl StartupStage for FakeStage {
        fn kind(&self) -> StartupStageKind {
            self.kind
        }

        fn start(&mut self) -> Result<(), Error> {
            self.events
                .lock()
                .expect("lock events")
                .push(format!("start:{:?}", self.kind));
            if self.fail_start {
                Err(anyhow::anyhow!("injected {:?} startup failure", self.kind))
            } else {
                Ok(())
            }
        }

        fn shutdown(&mut self) -> Result<(), Error> {
            self.events
                .lock()
                .expect("lock events")
                .push(format!("stop:{:?}", self.kind));
            if self.fail_shutdown {
                Err(anyhow::anyhow!("injected {:?} shutdown failure", self.kind))
            } else {
                Ok(())
            }
        }
    }

    fn fake_plan(events: &Events, fail_index: Option<usize>) -> Vec<Box<dyn StartupStage>> {
        REQUIRED_ORDER
            .into_iter()
            .enumerate()
            .map(|(index, kind)| {
                Box::new(FakeStage {
                    kind,
                    fail_start: fail_index == Some(index),
                    fail_shutdown: false,
                    events: Arc::clone(events),
                }) as Box<dyn StartupStage>
            })
            .collect()
    }

    #[test]
    fn startup_and_shutdown_follow_documented_order() {
        let events = Events::default();
        let mut coordinator =
            StartupCoordinator::new(fake_plan(&events, None)).expect("valid plan");
        coordinator.start().expect("start all stages");
        coordinator.shutdown().expect("stop all stages");
        coordinator.shutdown().expect("repeated stop");

        let actual = events.lock().expect("lock events").clone();
        let expected = [
            "start:Diagnostics",
            "start:WindowsPrerequisites",
            "start:ShellSta",
            "start:Gpui",
            "start:Window",
            "stop:Window",
            "stop:Gpui",
            "stop:ShellSta",
            "stop:WindowsPrerequisites",
            "stop:Diagnostics",
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn every_startup_failure_unwinds_completed_stages_once_in_reverse() {
        for fail_index in 0..REQUIRED_ORDER.len() {
            let events = Events::default();
            let mut coordinator =
                StartupCoordinator::new(fake_plan(&events, Some(fail_index))).expect("valid plan");
            let failure = coordinator.start().expect_err("injected failure");
            assert_eq!(failure.stage, REQUIRED_ORDER[fail_index]);
            assert!(failure.cleanup_errors.is_empty());
            coordinator.shutdown().expect("already unwound");

            let actual = events.lock().expect("lock events");
            let starts = actual
                .iter()
                .filter(|event| event.starts_with("start:"))
                .count();
            let stops = actual
                .iter()
                .filter(|event| event.starts_with("stop:"))
                .count();
            assert_eq!(starts, fail_index + 1);
            assert_eq!(stops, fail_index);
            let stopped: Vec<_> = actual
                .iter()
                .filter_map(|event| event.strip_prefix("stop:"))
                .collect();
            let expected: Vec<_> = REQUIRED_ORDER[..fail_index]
                .iter()
                .rev()
                .map(|kind| format!("{kind:?}"))
                .collect();
            assert_eq!(stopped, expected);
        }
    }

    #[test]
    fn shell_initialization_error_crosses_composition_boundary_and_unwinds() {
        let events = Events::default();
        let mut stages = fake_plan(&events, None);
        stages[2] = Box::new(ShellStaStage::with_starter(|| {
            Err(ShellStaError::ComInitialization {
                hresult: -2_147_467_259_i32,
            })
        }));
        let mut coordinator = StartupCoordinator::new(stages).expect("valid plan");

        let failure = coordinator.start().expect_err("injected Shell failure");
        assert_eq!(failure.stage, StartupStageKind::ShellSta);
        assert!(failure.source.to_string().contains("0x80004005"));
        assert!(failure.cleanup_errors.is_empty());
        assert_eq!(
            events.lock().expect("lock events").as_slice(),
            [
                "start:Diagnostics",
                "start:WindowsPrerequisites",
                "stop:WindowsPrerequisites",
                "stop:Diagnostics"
            ]
        );
    }
}
