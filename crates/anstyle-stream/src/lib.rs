pub mod adapter;

/// Only pass printable data to the inner `Write`
pub struct StripStream<W> {
    write: W,
    state: adapter::StripBytes,
}

impl<W> StripStream<W>
where
    W: std::io::Write,
{
    /// Only pass printable data to the inner `Write`
    #[inline]
    pub fn new(write: W) -> Self {
        Self {
            write,
            state: Default::default(),
        }
    }
}

impl<W> std::io::Write for StripStream<W>
where
    W: std::io::Write,
{
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // `clone_hack` is close enough until we can support clone as it only breaks if
        // - `buf` ends in the middle of a UTF-8 character
        // - `self.write` does not write the full buf
        let initial_state = self.state.clone_hack();

        let mut written = 0;
        let mut possible = 0;
        for printable in self.state.strip_next(buf) {
            possible += printable.len();
            written += self.write.write(printable)?;
            if possible != written {
                let divergence = &printable[written..];
                let offset = offset_to(buf, divergence);
                let consumed = &buf[offset..];
                self.state = initial_state;
                self.state.strip_next(consumed).last();
                break;
            }
        }
        Ok(written)
    }
    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        self.write.flush()
    }

    // Provide explicit implementations of trait methods
    // - To reduce bookkeeping
    // - Avoid acquiring / releasing locks in a loop

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        for printable in self.state.strip_next(buf) {
            self.write.write_all(printable)?;
        }
        Ok(())
    }

    // Not bothering with `write_fmt` as it just calls `write_all`
}

#[inline]
fn offset_to(total: &[u8], subslice: &[u8]) -> usize {
    let total = total.as_ptr();
    let subslice = subslice.as_ptr();

    debug_assert!(
        total <= subslice,
        "`Offset::offset_to` only accepts slices of `self`"
    );
    subslice as usize - total as usize
}
