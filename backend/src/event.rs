//! The `Event` type and the hierarchical `EventKind` descriptor.

use anymap::{any::CloneAny, Map};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// An `AnyMap` convenience type with the needed bounds for events.
pub type AnyMap = Map<CloneAny + Send + Sync>;

/// An event describing open or close operations on files.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum AccessMode {
    /// The catch-all case, to be used when the specific kind of event is unknown.
    Any,

    /// An event emitted when the file is executed, or the folder opened.
    Execute,

    /// An event emitted when the file is opened for reading.
    Read,

    /// An event emitted when the file is opened for writing.
    Write,

    /// An event which specific kind is known but cannot be represented otherwise.
    Other(String),
}

/// An event describing non-mutating access operations on files.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum AccessKind {
    /// The catch-all case, to be used when the specific kind of event is unknown.
    Any,

    /// An event emitted when the file is read.
    Read,

    /// An event emitted when the file, or a handle to the file, is opened.
    Open(AccessMode),

    /// An event emitted when the file, or a handle to the file, is closed.
    Close(AccessMode),

    /// An event which specific kind is known but cannot be represented otherwise.
    Other(String),
}

/// An event describing creation operations on files.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CreateKind {
    /// The catch-all case, to be used when the specific kind of event is unknown.
    Any,

    /// An event which results in the creation of a file.
    File,

    /// An event which results in the creation of a folder.
    Folder,

    /// An event which specific kind is known but cannot be represented otherwise.
    Other(String),
}

/// An event emitted when the data content of a file is changed.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DataChange {
    /// The catch-all case, to be used when the specific kind of event is unknown.
    Any,

    /// An event emitted when the size of the data is changed.
    Size,

    /// An event emitted when the content of the data is changed.
    Content,

    /// An event which specific kind is known but cannot be represented otherwise.
    Other(String),
}

/// An event emitted when the metadata of a file or folder is changed.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MetadataKind {
    /// The catch-all case, to be used when the specific kind of event is unknown.
    Any,

    /// An event emitted when the access time of the file or folder is changed.
    AccessTime,

    /// An event emitted when the write or modify time of the file or folder is changed.
    WriteTime,

    /// An event emitted when the permissions of the file or folder are changed.
    Permissions,

    /// An event emitted when the ownership of the file or folder is changed.
    Ownership,

    /// An event emitted when an extended attribute of the file or folder is changed.
    Extended(String),

    /// An event which specific kind is known but cannot be represented otherwise.
    Other(String),
}

/// An event emitted when the name of a file or folder is changed.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum RenameMode {
    /// The catch-all case, to be used when the specific kind of event is unknown.
    Any,

    /// An event emitted on the file or folder resulting from a rename.
    To,

    /// An event emitted on the file or folder that was renamed.
    From,

    /// An event which specific kind is known but cannot be represented otherwise.
    Other(String),
}

/// An event describing mutation of content, name, or metadata.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ModifyKind {
    /// The catch-all case, to be used when the specific kind of event is unknown.
    Any,

    /// An event emitted when the data content of a file is changed.
    Data(DataChange),

    /// An event emitted when the metadata of a file or folder is changed.
    Metadata(MetadataKind),

    /// An event emitted when the name of a file or folder is changed.
    Name(RenameMode),

    /// An event which specific kind is known but cannot be represented otherwise.
    Other(String),
}

/// An event describing removal operations on files.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum RemoveKind {
    /// The catch-all case, to be used when the specific kind of event is unknown.
    Any,

    /// An event emitted when a file is removed.
    File,

    /// An event emitted when a folder is removed.
    Folder,

    /// An event which specific kind is known but cannot be represented otherwise.
    Other(String),
}

/// Top-level event kind.
///
/// This is arguably the most important classification for events. All subkinds below this one
/// represent details that may or may not be available for any particular backend, but most tools
/// and Notify systems will only care about which of these four general kinds an event is about.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum EventKind {
    /// The catch-all event kind, for unsupported/unknown events.
    ///
    /// This variant should be used as the "else" case when mapping native kernel bitmasks or
    /// bitmaps, such that if the mask is ever extended with new event types the backend will not
    /// gain bugs due to not matching new unknown event types.
    Any,

    /// An event describing non-mutating access operations on files.
    ///
    /// This event is about opening and closing file handles, as well as executing files, and any
    /// other such event that is about accessing files, folders, or other structures rather than
    /// mutating them.
    ///
    /// Only backends with the `EmitOnAccess` capability will generate these.
    Access(AccessKind),

    /// An event describing creation operations on files.
    ///
    /// This event is about the creation of files, folders, or other structures but not about e.g.
    /// writing new content into them.
    Create(CreateKind),

    /// An event describing mutation of content, name, or metadata.
    ///
    /// This event is about the mutation of files', folders', or other structures' content, name
    /// (path), or associated metadata (attributes).
    Modify(ModifyKind),

    /// An event describing removal operations on files.
    ///
    /// This event is about the removal of files, folders, or other structures but not e.g. erasing
    /// content from them. This may also be triggered for renames/moves that move files _out of the
    /// watched subpath_.
    ///
    /// Some editors also trigger Remove events when saving files as they may opt for removing (or
    /// renaming) the original then creating a new file in-place.
    Remove(RemoveKind),

    /// An event not fitting in any of the above four categories.
    ///
    /// This may be used for meta-events about the watch itself, but generally should not be used.
    Other(String),
}

impl EventKind {
    /// Indicates whether an event is an Access variant.
    pub fn is_access(&self) -> bool {
        match *self {
            EventKind::Access(_) => true,
            _ => false,
        }
    }

    /// Indicates whether an event is a Create variant.
    pub fn is_create(&self) -> bool {
        match *self {
            EventKind::Create(_) => true,
            _ => false,
        }
    }

    /// Indicates whether an event is a Modify variant.
    pub fn is_modify(&self) -> bool {
        match *self {
            EventKind::Modify(_) => true,
            _ => false,
        }
    }

    /// Indicates whether an event is a Remove variant.
    pub fn is_remove(&self) -> bool {
        match *self {
            EventKind::Remove(_) => true,
            _ => false,
        }
    }
}

impl Default for EventKind {
    fn default() -> Self {
        EventKind::Any
    }
}

/// Notify event.
#[derive(Clone, Debug)]
pub struct Event {
    /// Kind of the event.
    ///
    /// This is a hierarchy of enums describing the event as precisely as possible. All enums in
    /// the hierarchy have two variants always present, `Any` and `Other(String)`, accompanied by
    /// one or more specific variants.
    ///
    /// `Any` should be used when more detail about the event is not known beyond the variant
    /// already selected. For example, `AccessMode::Any` means a file has been accessed, but that's
    /// all we know.
    ///
    /// `Other` should be used when more detail _is_ available, but cannot be encoded as one of the
    /// defined variants. For example, `CreateKind::Other("mount")` may indicate the binding of a
    /// mount. The documentation of the particular backend should indicate if any `Other` events
    /// are generated, and what their description means.
    ///
    /// The `EventKind::Any` variant should be used as the "else" case when mapping native kernel
    /// bitmasks or bitmaps, such that if the mask is ever extended with new event types the
    /// backend will not gain bugs due to not matching new unknown event types.
    pub kind: EventKind,

    /// Paths that the event is about.
    ///
    /// Generally that will be a single path, but it may be more in backends that track e.g.
    /// renames by path instead of "cookie".
    pub paths: Vec<PathBuf>,

    /// Relation ID for events that are related.
    ///
    /// This will only be `Some` for events generated by backends with the `TrackRelated`
    /// capability. Those backends _may_ emit events that are related to each other, and tag those
    /// with an identical `relid` or "cookie". The value is normalised to `usize`.
    pub relid: Option<usize>,

    /// Additional attributes of the event.
    ///
    /// Arbitrary data may be added to this field, without restriction beyond the `Sync` and
    /// `Clone` properties. Any data added here is _not_ considered for comparing and hashing.
    ///
    /// For vendor or custom information, it is recommended to use type wrappers to differentiate
    /// entries within the `AnyMap` container and avoid conflicts. For interoperability, one of the
    /// “well-known” types (or propose a new one) should be used instead. See the list on the wiki:
    /// https://github.com/passcod/notify/wiki/Well-Known-Event-Attrs
    pub attrs: AnyMap,

    /// Source of the event.
    ///
    /// This is the same string returned by the `name()` method of a backend.
    pub source: &'static str,
}

impl Default for Event {
    fn default() -> Self {
        Self {
            kind: EventKind::default(),
            paths: Vec::with_capacity(1),
            relid: None,
            attrs: AnyMap::new(),
            source: ""
        }
    }
}

impl Eq for Event {}
impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        self.kind.eq(&other.kind) &&
        self.paths.eq(&other.paths) &&
        self.relid.eq(&other.relid) &&
        self.source.eq(other.source)
    }
}

impl Hash for Event {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        self.paths.hash(state);
        self.relid.hash(state);
        self.source.hash(state);
    }
}
