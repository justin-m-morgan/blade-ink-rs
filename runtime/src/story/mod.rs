//! Public Ink story API and shared runtime contracts.

mod player;
#[cfg(feature = "load-profile")]
pub use player::LoadProfile;
pub use player::{ChoiceInfo, Story};

pub mod error;
pub mod external_functions;
pub mod variable_observer;

/// `Send + Sync` when the `sync` feature is enabled, and no bound otherwise.
///
/// Callbacks handed to a [`Story`] (external functions, variable observers,
/// error handlers and time sources) must implement it, so that with `sync` the
/// story itself is `Send + Sync`.
#[cfg(feature = "sync")]
pub trait MaybeSendSync: Send + Sync {}
#[cfg(feature = "sync")]
impl<T: ?Sized + Send + Sync> MaybeSendSync for T {}

/// `Send + Sync` when the `sync` feature is enabled, and no bound otherwise.
///
/// Callbacks handed to a [`Story`] (external functions, variable observers,
/// error handlers and time sources) must implement it, so that with `sync` the
/// story itself is `Send + Sync`.
#[cfg(not(feature = "sync"))]
pub trait MaybeSendSync {}
#[cfg(not(feature = "sync"))]
impl<T: ?Sized> MaybeSendSync for T {}

/// Supplies elapsed time to time-limited story continuation.
pub trait TimeSource: MaybeSendSync {
    /// Returns a monotonically increasing duration.
    fn now(&self) -> core::time::Duration;
}

impl<F> TimeSource for F
where
    F: Fn() -> core::time::Duration + MaybeSendSync,
{
    fn now(&self) -> core::time::Duration {
        self()
    }
}

/// The current version of the Ink story file format.
pub const INK_VERSION_CURRENT: i32 = 21;
/// The minimum Ink version accepted by the runtime.
pub const INK_VERSION_MINIMUM_COMPATIBLE: i32 = 18;
