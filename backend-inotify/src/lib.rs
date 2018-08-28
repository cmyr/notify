//! Notify Backend crate for Linux's inotify.

#![deny(missing_docs)]
#![forbid(unsafe_code)]
#![cfg_attr(feature = "cargo-clippy", deny(clippy_pedantic))]

extern crate inotify;
extern crate notify_backend as backend;

use backend::prelude::*;
use backend::Buffer;

use inotify::{EventMask, Events, Inotify, WatchMask};
use std::{fmt, os::unix::io::AsRawFd};

const BACKEND_NAME: &str = "inotify";

/// A Notify Backend for Linux's [inotify].
///
/// Inotify requires kernel version 2.6.13.
///
/// This backend can natively:
///
///  - emit Access events
///  - follow symlinks
///  - track related changes (for renames)
///  - watch individual files
///  - watch folders (but not recursively)
///
/// The backend reads events into a ~4KB buffer, corresponding to 200 events (24 bytes per event on
/// 64-bit architectures, and 20 bytes on 32-bit architectures), then pushes them to an internal
/// [Buffer] after translation into Notify events.
///
/// Inotify emits an event when a filesystem whose mountpoint is watched is unmounted. In this
/// backend, this event is mapped as `RemoveKind::Other("unmount")`.
///
/// [inotify]: http://man7.org/linux/man-pages/man7/inotify.7.html
/// [Buffer]: ../notify_backend/buffer/struct.Buffer.html
pub struct Backend {
    buffer: Buffer,
    driver: OwnedEventedFd,
    inotify: Inotify,
}

#[cfg(target_pointer_width = "64")]
const BUFFER_SIZE: usize = 4800;

#[cfg(not(target_pointer_width = "64"))]
const BUFFER_SIZE: usize = 4000;

impl NotifyBackend for Backend {
    fn name() -> &'static str {
        BACKEND_NAME
    }

    fn new(paths: Vec<PathBuf>) -> NewBackendResult {
        let mut inotify = Inotify::init()?;

        for path in paths {
            // TODO: extract io NotFound errors manually for richer NonExistent error
            inotify.add_watch(&path, WatchMask::ALL_EVENTS)?;
        }

        Ok(Box::new(Self {
            buffer: Buffer::new(),
            driver: OwnedEventedFd(inotify.as_raw_fd()),
            inotify,
        }))
    }

    fn driver(&self) -> Box<Evented> {
        Box::new(self.driver)
    }

    fn capabilities() -> Vec<Capability> {
        vec![
            Capability::EmitOnAccess,
            Capability::FollowSymlinks,
            Capability::TrackRelated,
            Capability::WatchFiles,
            Capability::WatchFolders,
        ]
    }
}

impl Drop for Backend {
    fn drop(&mut self) {}
}

impl fmt::Debug for Backend {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Backend")
            .field("buffer", &self.buffer)
            .field("driver", &self.driver)
            .field("inotify", &self.inotify.as_raw_fd())
            .finish()
    }
}

impl Stream for Backend {
    type Item = StreamItem;
    type Error = StreamError;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.buffer.closed() {
            return self.buffer.poll();
        }

        let mut buf = [0; BUFFER_SIZE];
        let from_kernel = self.inotify.read_events(&mut buf)?;

        self.process_events(from_kernel)?;
        self.buffer.poll()
    }
}

impl Backend {
    fn process_events(&mut self, events: Events) -> Result<(), StreamError> {
        for e in events {
            if e.mask.contains(EventMask::Q_OVERFLOW) {
                // Currently, futures::Stream don't terminate on Error, so we
                // close the buffer such that the rest of the events trickle
                // through and the stream ends with Ready(None) after all are
                // through. If futures::Stream change so they terminate on
                // Error, we'll need to change the Buffer so it may carry an
                // Error value, and output it at the end of the stream. In
                // either case, it's important that we do continue to provide
                // the received events, even in the case of an error/overflow
                // upstream.
                self.buffer.close();
                return Err(StreamError::UpstreamOverflow);
            }

            if e.mask.contains(EventMask::IGNORED) {
                self.buffer.close();
                break;
            }

            self.buffer.push(Event {
                kind: if e.mask.contains(EventMask::ACCESS) {
                    EventKind::Access(AccessKind::Any)
                } else if e.mask.contains(EventMask::ATTRIB) {
                    EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any))
                } else if e.mask.contains(EventMask::CLOSE_WRITE) {
                    EventKind::Access(AccessKind::Close(AccessMode::Write))
                } else if e.mask.contains(EventMask::CLOSE_NOWRITE) {
                    EventKind::Access(AccessKind::Close(AccessMode::Read))
                } else if e.mask.contains(EventMask::CREATE) {
                    EventKind::Create(if e.mask.contains(EventMask::ISDIR) {
                        CreateKind::Folder
                    } else {
                        CreateKind::File
                    })
                } else if e.mask.contains(EventMask::DELETE)
                    || e.mask.contains(EventMask::DELETE_SELF)
                {
                    EventKind::Remove(if e.mask.contains(EventMask::ISDIR) {
                        RemoveKind::Folder
                    } else {
                        RemoveKind::File
                    })
                } else if e.mask.contains(EventMask::MODIFY) {
                    EventKind::Modify(ModifyKind::Data(DataChange::Any))
                } else if e.mask.contains(EventMask::MOVE_SELF) {
                    EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                } else if e.mask.contains(EventMask::MOVED_FROM) {
                    EventKind::Modify(ModifyKind::Name(RenameMode::From))
                } else if e.mask.contains(EventMask::MOVED_TO) {
                    EventKind::Modify(ModifyKind::Name(RenameMode::To))
                } else if e.mask.contains(EventMask::OPEN) {
                    EventKind::Access(AccessKind::Open(AccessMode::Any))
                } else if e.mask.contains(EventMask::UNMOUNT) {
                    EventKind::Remove(RemoveKind::Other("unmount".into()))
                } else {
                    EventKind::Any
                },
                paths: e.name.map_or_else(|| vec![], |s| vec![s.into()]),
                relid: match e.cookie {
                    0 => None,
                    c => Some(c as usize),
                },
                attrs: AnyMap::new(),
                source: BACKEND_NAME,
            })
        }

        Ok(())
    }
}
