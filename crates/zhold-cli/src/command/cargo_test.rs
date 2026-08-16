use std::{collections::VecDeque, io, process::ExitStatus};

use super::cargo::{
    SupervisedChild,
    supervisor::{ChildDisposition, SpawnError, WaitError},
    wait_for_cargo,
};

#[derive(Debug)]
struct FakeChild {
    observations: VecDeque<Result<Option<ExitStatus>, io::Error>>,
    cleanup: Result<(), io::Error>,
}

impl SupervisedChild for FakeChild {
    fn try_wait(&mut self) -> Result<Option<ExitStatus>, io::Error> {
        self.observations
            .pop_front()
            .unwrap_or_else(|| Err(io::Error::other("missing observation")))
    }

    fn terminate_and_wait(&mut self) -> Result<(), io::Error> {
        match &self.cleanup {
            Ok(()) => Ok(()),
            Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
        }
    }
}

#[test]
fn wait_failure_requires_cleanup_proof() -> Result<(), Box<dyn std::error::Error>> {
    let mut confirmed = failed_child(Ok(()));
    let Err(failure) = wait_for_cargo(&mut confirmed) else {
        return Err(io::Error::other("wait unexpectedly succeeded").into());
    };
    assert_eq!(failure.disposition(), ChildDisposition::CleanupConfirmed);

    let mut unconfirmed = failed_child(Err(io::Error::other("cannot reap group")));
    let Err(failure) = wait_for_cargo(&mut unconfirmed) else {
        return Err(io::Error::other("wait unexpectedly succeeded").into());
    };
    assert_eq!(failure.disposition(), ChildDisposition::CleanupUnconfirmed);
    Ok(())
}

#[test]
fn cleanup_proof_controls_post_spawn_disposition() {
    let confirmed = SpawnError::after_child(io::Error::other("setup"), Ok(()));
    assert_eq!(confirmed.disposition(), ChildDisposition::CleanupConfirmed);

    let unconfirmed = WaitError::after_child(
        io::Error::other("wait"),
        Err(io::Error::other("group still live")),
    );
    assert_eq!(
        unconfirmed.disposition(),
        ChildDisposition::CleanupUnconfirmed
    );
    assert!(
        unconfirmed
            .into_source()
            .to_string()
            .contains("cleanup could not be confirmed")
    );
}

#[test]
fn pre_spawn_failure_never_claims_cleanup_proof() {
    let failure = SpawnError::before_child(io::Error::other("spawn"));
    assert_eq!(failure.disposition(), ChildDisposition::NeverCreated);
}

fn failed_child(cleanup: Result<(), io::Error>) -> FakeChild {
    FakeChild {
        observations: VecDeque::from([Err(io::Error::other("observation failed"))]),
        cleanup,
    }
}
