#[cfg(feature = "auto")]
use crate::ColorChoice;
use crate::IsTerminal;
use crate::Lockable;
use crate::RawStream;
use crate::StripStream;
#[cfg(all(windows, feature = "wincon"))]
use crate::WinconStream;

/// [`std::io::Write`] that adapts ANSI escape codes to the underlying `Write`s capabilities
#[derive(Debug)]
pub struct AutoStream<S: RawStream> {
    inner: StreamInner<S>,
}

#[derive(Debug)]
enum StreamInner<S: RawStream> {
    PassThrough(S),
    Strip(StripStream<S>),
    #[cfg(all(windows, feature = "wincon"))]
    Wincon(WinconStream<S>),
}

impl<S> AutoStream<S>
where
    S: RawStream,
{
    /// Runtime control over styling behavior
    #[cfg(feature = "auto")]
    #[inline]
    pub fn new(raw: S, choice: ColorChoice) -> Self {
        match choice {
            ColorChoice::Auto => Self::auto(raw),
            ColorChoice::AlwaysAnsi => Self::always_ansi(raw),
            ColorChoice::Always => Self::always(raw),
            ColorChoice::Never => Self::never(raw),
        }
    }

    /// Auto-adapt for the stream's capabilities
    #[cfg(feature = "auto")]
    #[inline]
    pub fn auto(raw: S) -> Self {
        let choice = Self::choice(&raw);
        debug_assert_ne!(choice, ColorChoice::Auto);
        Self::new(raw, choice)
    }

    /// Report the desired choice for the given stream
    #[cfg(feature = "auto")]
    pub fn choice(raw: &S) -> ColorChoice {
        choice(raw)
    }

    /// Force ANSI escape codes to be passed through as-is, no matter what the inner `Write`
    /// supports.
    #[inline]
    pub fn always_ansi(raw: S) -> Self {
        #[cfg(feature = "auto")]
        {
            if raw.is_terminal() {
                let _ = anstyle_query::windows::enable_ansi_colors();
            }
        }
        Self::always_ansi_(raw)
    }

    #[inline]
    fn always_ansi_(raw: S) -> Self {
        let inner = StreamInner::PassThrough(raw);
        AutoStream { inner }
    }

    /// Force color, no matter what the inner `Write` supports.
    #[inline]
    pub fn always(raw: S) -> Self {
        if cfg!(windows) {
            #[cfg(feature = "auto")]
            let use_wincon = raw.is_terminal()
                && !anstyle_query::windows::enable_ansi_colors().unwrap_or(true)
                && !anstyle_query::term_supports_ansi_color();
            #[cfg(not(feature = "auto"))]
            let use_wincon = true;
            if use_wincon {
                Self::wincon(raw).unwrap_or_else(|raw| Self::always_ansi_(raw))
            } else {
                Self::always_ansi_(raw)
            }
        } else {
            Self::always_ansi(raw)
        }
    }

    /// Only pass printable data to the inner `Write`.
    #[inline]
    pub fn never(raw: S) -> Self {
        let inner = StreamInner::Strip(StripStream::new(raw));
        AutoStream { inner }
    }

    #[inline]
    fn wincon(raw: S) -> Result<Self, S> {
        #[cfg(all(windows, feature = "wincon"))]
        {
            Ok(Self {
                inner: StreamInner::Wincon(WinconStream::new(raw)),
            })
        }
        #[cfg(not(all(windows, feature = "wincon")))]
        {
            Err(raw)
        }
    }

    /// Get the wrapped [`RawStream`]
    #[inline]
    pub fn into_inner(self) -> S {
        match self.inner {
            StreamInner::PassThrough(w) => w,
            StreamInner::Strip(w) => w.into_inner(),
            #[cfg(all(windows, feature = "wincon"))]
            StreamInner::Wincon(w) => w.into_inner(),
        }
    }

    #[inline]
    #[cfg(feature = "auto")]
    pub fn is_terminal(&self) -> bool {
        match &self.inner {
            StreamInner::PassThrough(w) => w.is_terminal(),
            StreamInner::Strip(w) => w.is_terminal(),
            #[cfg(all(windows, feature = "wincon"))]
            StreamInner::Wincon(_) => true, // its only ever a terminal
        }
    }
}

#[cfg(feature = "auto")]
fn choice(raw: &dyn RawStream) -> ColorChoice {
    let choice = ColorChoice::global();
    match choice {
        ColorChoice::Auto => {
            let clicolor = anstyle_query::clicolor();
            let clicolor_enabled = clicolor.unwrap_or(false);
            let clicolor_disabled = !clicolor.unwrap_or(true);
            if raw.is_terminal()
                && !anstyle_query::no_color()
                && !clicolor_disabled
                && (anstyle_query::term_supports_color()
                    || clicolor_enabled
                    || anstyle_query::is_ci())
                || anstyle_query::clicolor_force()
            {
                ColorChoice::Always
            } else {
                ColorChoice::Never
            }
        }
        ColorChoice::AlwaysAnsi | ColorChoice::Always | ColorChoice::Never => choice,
    }
}

#[cfg(feature = "auto")]
impl<S> IsTerminal for AutoStream<S>
where
    S: RawStream,
{
    #[inline]
    fn is_terminal(&self) -> bool {
        self.is_terminal()
    }
}

impl AutoStream<std::io::Stdout> {
    /// Get exclusive access to the `AutoStream`
    ///
    /// Why?
    /// - Faster performance when writing in a loop
    /// - Avoid other threads interleaving output with the current thread
    #[inline]
    pub fn lock(self) -> <Self as Lockable>::Locked {
        let inner = match self.inner {
            StreamInner::PassThrough(w) => StreamInner::PassThrough(w.lock()),
            StreamInner::Strip(w) => StreamInner::Strip(w.lock()),
            #[cfg(all(windows, feature = "wincon"))]
            StreamInner::Wincon(w) => StreamInner::Wincon(w.lock()),
        };
        AutoStream { inner }
    }
}

impl AutoStream<std::io::Stderr> {
    /// Get exclusive access to the `AutoStream`
    ///
    /// Why?
    /// - Faster performance when writing in a loop
    /// - Avoid other threads interleaving output with the current thread
    #[inline]
    pub fn lock(self) -> <Self as Lockable>::Locked {
        let inner = match self.inner {
            StreamInner::PassThrough(w) => StreamInner::PassThrough(w.lock()),
            StreamInner::Strip(w) => StreamInner::Strip(w.lock()),
            #[cfg(all(windows, feature = "wincon"))]
            StreamInner::Wincon(w) => StreamInner::Wincon(w.lock()),
        };
        AutoStream { inner }
    }
}

impl<S> std::io::Write for AutoStream<S>
where
    S: RawStream,
{
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match &mut self.inner {
            StreamInner::PassThrough(w) => w.write(buf),
            StreamInner::Strip(w) => w.write(buf),
            #[cfg(all(windows, feature = "wincon"))]
            StreamInner::Wincon(w) => w.write(buf),
        }
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.inner {
            StreamInner::PassThrough(w) => w.flush(),
            StreamInner::Strip(w) => w.flush(),
            #[cfg(all(windows, feature = "wincon"))]
            StreamInner::Wincon(w) => w.flush(),
        }
    }

    // Provide explicit implementations of trait methods
    // - To reduce bookkeeping
    // - Avoid acquiring / releasing locks in a loop

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match &mut self.inner {
            StreamInner::PassThrough(w) => w.write_all(buf),
            StreamInner::Strip(w) => w.write_all(buf),
            #[cfg(all(windows, feature = "wincon"))]
            StreamInner::Wincon(w) => w.write_all(buf),
        }
    }

    // Not bothering with `write_fmt` as it just calls `write_all`
}

impl Lockable for AutoStream<std::io::Stdout> {
    type Locked = AutoStream<<std::io::Stdout as Lockable>::Locked>;

    #[inline]
    fn lock(self) -> Self::Locked {
        self.lock()
    }
}

impl Lockable for AutoStream<std::io::Stderr> {
    type Locked = AutoStream<<std::io::Stderr as Lockable>::Locked>;

    #[inline]
    fn lock(self) -> Self::Locked {
        self.lock()
    }
}
