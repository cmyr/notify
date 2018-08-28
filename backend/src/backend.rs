//! The `Backend` trait and related types.

use super::{capability::Capability, stream};
use futures::Stream;
use mio::event::Evented;
use std::{ffi, fmt::Debug, io, path::PathBuf, sync::Arc};

/// Convenient type alias for the Backend trait object.
pub type BoxedBackend = Box<Backend<Item = stream::Item, Error = stream::Error>>;

/// Convenient type alias for the `::new()` function return signature.
pub type NewResult = Result<BoxedBackend, ErrorWrap>;

/// A trait for types that implement Notify backends.
///
/// Be sure to thoroughly read the [`Evented`] and [`Stream`] documentations when implementing a
/// `Backend`, as the semantics described are relied upon by Notify, and incorrectly or
/// incompletely implementing them will result in bad behaviour.
///
/// Take care to correctly free all resources via the `Drop` trait. For ease of debugging, the
/// [`Debug`] trait is required. Often this can be derived automatically, but for some backends
/// a manual implementation may be needed. Additionally, a backend may want to provide a custom
/// Debug to add useful information rather than e.g. opaque FD numbers.
///
/// [`Debug`]: https://doc.rust-lang.org/std/fmt/trait.Debug.html
/// [`Evented`]: https://docs.rs/mio/0.6/mio/event/trait.Evented.html
/// [`Stream`]: https://docs.rs/futures/0.1/futures/stream/trait.Stream.html
pub trait Backend: Stream + Send + Drop + Debug {
    /// Creates an instance of a `Backend` that watches over a set of paths.
    ///
    /// While the `paths` argument is a `Vec` for implementation simplicity, Notify guarantees that
    /// it will only contain unique entries. Notify will also _try_ to make sure that they are
    /// pointing to unique trees on the filesystem but cannot offer a guarantee because of the very
    /// nature of filesystems aka "if trees or links are moved by someone else".
    ///
    /// This function must initialise all resources needed to watch over the paths, and only those
    /// paths. When the set of paths to be watched changes, the `Backend` will be `Drop`ped, and a
    /// new one recreated in its place. Thus, the `Backend` is immutable in this respect.
    fn new(paths: Vec<PathBuf>) -> NewResult
    where
        Self: Sized;

    /// Returns the operational capabilities of this `Backend`.
    ///
    /// See the [`Capability` documentation][cap] for details.
    ///
    /// The function may perform checks and vary its response based on environmental factors.
    ///
    /// If the function returns an empty `Vec`, the `Backend` will be assumed to be inoperable at
    /// the moment (and another one may be selected). In general this should not happen, and
    /// instead an `Unavailable` error should be returned from `::new()`.
    ///
    /// [cap]: ../capability/enum.Capability.html
    fn capabilities() -> Vec<Capability>
    where
        Self: Sized;

    /// Returns an [`Evented`] implementation that is used to efficently drive the event loop.
    ///
    /// Backends often wrap kernel APIs, which can also be used to drive the Tokio event loop to
    /// avoid busy waiting or inefficient polling. If no such API is available, for example in the
    /// case of a polling `Backend`, this mechanism may be implemented in userspace and use
    /// whatever clues and cues the `Backend` has available to drive the readiness state.
    ///
    /// There is currently no facility or support for a `Backend` to opt out of registering an
    /// `Evented` driver. If this is needed, request it on the issue tracker. In the meantime, a
    /// workaround is to implement a `Registration` that immediately sets itself as ready.
    ///
    /// [`Evented`]: https://docs.rs/mio/0.6/mio/event/trait.Evented.html
    fn driver(&self) -> Box<Evented>;

    /// Returns the name of this Backend.
    ///
    /// This is used for primarily for debugging and post-processing/filtering. Having two backends
    /// with the same name running at once is undefined behaviour and may be disallowed by Notify.
    /// The value should not change.
    fn name() -> &'static str
    where
        Self: Sized;

    /// The version of the Backend trait this implementation was built against.
    fn trait_version() -> String
    where
        Self: Sized,
    {
        env!("CARGO_PKG_VERSION").into()
    }
}

/// Any error which may occur during the initialisation of a `Backend`.
#[derive(Clone, Debug)]
pub enum Error {
    /// An error represented by an arbitrary string.
    Generic(String),

    /// An I/O error.
    Io(Arc<io::Error>),

    /// An error indicating that this Backend's implementation is incomplete.
    ///
    /// This is mostly to be used while developing Backends.
    NotImplemented,

    /// An error indicating that this Backend is unavailable, likely because its upstream or native
    /// API is inoperable. An optional reason may be supplied.
    Unavailable(Option<String>),

    /// An error indicating that one or more paths passed to the Backend do not exist. This should
    /// be translated from the native API or upstream's response: the frontend is responsible for
    /// pre-checking that paths exist.
    ///
    /// This error exists to cover cases where we lose a data race against the filesystem and the
    /// path is gone between the time the frontend checks it and the Backend initialises.
    ///
    /// It may contain the list of files that are reported to be non-existent if that is known.
    ///
    /// `io::Error`s of kind `NotFound` will be auto-converted to this variant for convenience, but
    /// whenever possible this should be done manually to populate the paths argument.
    NonExistent(Vec<PathBuf>),

    /// An error indicating that one or more of the paths given is not supported by the `Backend`,
    /// with the relevant unsupported `Capability` passed along.
    NotSupported(Capability),

    /// A string conversion issue (nul byte found) from an FFI binding.
    FfiNul(ffi::NulError),

    /// A string conversion issue (UTF-8 error) from an FFI binding.
    FfiIntoString(ffi::IntoStringError),

    /// A str conversion issue (nul too early or absent) from an FFI binding.
    FfiFromBytes(ffi::FromBytesWithNulError),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::NotFound => Error::NonExistent(vec![]),
            _ => Error::Io(Arc::new(err)),
        }
    }
}

impl From<Capability> for Error {
    fn from(cap: Capability) -> Self {
        Error::NotSupported(cap)
    }
}

impl From<ffi::NulError> for Error {
    fn from(err: ffi::NulError) -> Self {
        Error::FfiNul(err)
    }
}

impl From<ffi::IntoStringError> for Error {
    fn from(err: ffi::IntoStringError) -> Self {
        Error::FfiIntoString(err)
    }
}

impl From<ffi::FromBytesWithNulError> for Error {
    fn from(err: ffi::FromBytesWithNulError) -> Self {
        Error::FfiFromBytes(err)
    }
}

/// A composite error wrapper type.
///
/// When initialising a `Backend`, errors that occur may either be general or only affect certain
/// paths. This special type encodes which case is the situation, and comes with implementations to
/// make it easier and less verbose to use in most common ways.
///
/// In all the error scenarios described below that affect _subsets_ of paths, the assumption is
/// that if _only_ the _non-erroring_ paths were passed again, the creation of the `Backend` would
/// be _likely_ to succeed.
#[derive(Clone, Debug)]
pub enum ErrorWrap {
    /// An error about the backend itself or in general.
    General(Error),

    /// An error that affects all paths passed in.
    ///
    /// May be also represented by a `Multiple` or a `Single` with all the paths associated to
    /// errors. However, this variant is more efficient.
    All(Error),

    /// An error that only affects some paths.
    ///
    /// This is for a single _error_ that affects a subset of the paths that were passed in.
    Single(Error, Vec<PathBuf>),

    /// Several errors associated with different paths.
    ///
    /// This is for multiple _errors_ that affect subsets of paths. The subsets may all be the
    /// same, or may be empty to denote a general error as well as specific ones, or may duplicate
    /// paths. It is however expected that within `Vec`s, paths are unique (but this will not be
    /// enforced strictly).
    Multiple(Vec<(Error, Vec<PathBuf>)>),
}

impl ErrorWrap {
    /// Reduces to a set of errors, discarding all path information.
    pub fn as_error_vec(&self) -> Vec<&Error> {
        match self {
            ErrorWrap::Multiple(ve) => ve.iter().map(|(e, _)| e).collect(),
            ErrorWrap::General(ref err)
            | ErrorWrap::All(ref err)
            | ErrorWrap::Single(ref err, _) => vec![err],
        }
    }
}

impl From<Error> for ErrorWrap {
    fn from(err: Error) -> Self {
        ErrorWrap::General(err)
    }
}

impl<'a> From<&'a Error> for ErrorWrap {
    fn from(err: &'a Error) -> Self {
        ErrorWrap::General(err.clone())
    }
}

impl From<io::Error> for ErrorWrap {
    fn from(err: io::Error) -> Self {
        let e: Error = err.into();
        e.into()
    }
}

impl From<Capability> for ErrorWrap {
    fn from(cap: Capability) -> Self {
        let e: Error = cap.into();
        e.into()
    }
}

impl From<ffi::NulError> for ErrorWrap {
    fn from(err: ffi::NulError) -> Self {
        let e: Error = err.into();
        e.into()
    }
}

impl From<ffi::IntoStringError> for ErrorWrap {
    fn from(err: ffi::IntoStringError) -> Self {
        let e: Error = err.into();
        e.into()
    }
}

impl From<ffi::FromBytesWithNulError> for ErrorWrap {
    fn from(err: ffi::FromBytesWithNulError) -> Self {
        let e: Error = err.into();
        e.into()
    }
}
