use alloc::vec::Vec;

use axerrno::{AxError, AxResult};
use bitmaps::Bitmap;

use super::FileDescriptor;
use crate::task::AX_FILE_LIMIT;

#[derive(Clone, Default)]
pub struct FileTable {
    slots: Vec<Option<FileDescriptor>>,
    occupied: Bitmap<AX_FILE_LIMIT>,
    always_ready: Bitmap<AX_FILE_LIMIT>,
}

impl FileTable {
    pub fn get(&self, fd: usize) -> Option<&FileDescriptor> {
        self.slots.get(fd)?.as_ref()
    }

    pub fn set_cloexec(&mut self, fd: usize, enabled: bool) -> AxResult<()> {
        self.slots
            .get_mut(fd)
            .and_then(Option::as_mut)
            .ok_or(AxError::BadFileDescriptor)?
            .cloexec = enabled;
        Ok(())
    }

    pub fn ids(&self) -> impl DoubleEndedIterator<Item = usize> + '_ {
        (&self.occupied).into_iter()
    }

    pub fn count(&self) -> usize {
        self.occupied.len()
    }

    pub fn always_ready(&self) -> Bitmap<AX_FILE_LIMIT> {
        self.always_ready
    }

    pub fn add(&mut self, entry: FileDescriptor) -> AxResult<usize> {
        let fd = self
            .occupied
            .first_false_index()
            .ok_or(AxError::TooManyOpenFiles)?;
        self.add_at(fd, entry)
    }

    pub fn add_at(&mut self, fd: usize, entry: FileDescriptor) -> AxResult<usize> {
        if fd >= AX_FILE_LIMIT || self.occupied.get(fd) {
            return Err(AxError::BadFileDescriptor);
        }
        if fd >= self.slots.len() {
            self.slots.resize_with(fd + 1, || None);
        }
        self.always_ready
            .set(fd, entry.inner.always_read_write_ready());
        self.slots[fd] = Some(entry);
        self.occupied.set(fd, true);
        Ok(fd)
    }

    pub fn remove(&mut self, fd: usize) -> Option<FileDescriptor> {
        let old = self.slots.get_mut(fd)?.take()?;
        self.occupied.set(fd, false);
        self.always_ready.set(fd, false);
        if fd + 1 == self.slots.len() {
            self.slots
                .truncate(self.occupied.last_index().map_or(0, |last| last + 1));
        }
        Some(old)
    }
}
