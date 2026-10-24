use std::{
    error, fmt::{self, Display}, io::{self, Write}, mem::MaybeUninit,
};

pub const ESC: u8 = b'\x1b';

const ENTER_ALT_SCREEN: &str = "\x1b[?1049h";
const LEAVE_ALT_SCREEN: &str = "\x1b[?1049l";

#[derive(Debug)]
pub struct Terminal {
    pub width: u16,
    pub height: u16,
    og_termios: libc::termios,
    alt_screen: bool,
}

impl Terminal {
    pub fn new(enter_alt_screen: bool) -> Result<Self, TermPrepError> {
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        return Err(TermPrepError::UnsupportedOperatingSystem);

        let (width, height) = term_dimensions()?;
        let og_termios = term_attrs()?;

        let mut raw = og_termios;
        unsafe { libc::cfmakeraw(&mut raw) };
        set_term_attrs(&raw)?;

        let mut stdout = io::stdout();
        if enter_alt_screen {
            stdout.write_all(ENTER_ALT_SCREEN.as_bytes())?;
            stdout.write_all("\x1b[H".as_bytes())?;
            stdout.flush()?;
        }

        Ok(Self {
            width,
            height,
            og_termios,
            alt_screen: enter_alt_screen,
        })
    }

    pub fn byte_ready(timeout_ms: i32) -> io::Result<bool> {
        let mut pollfd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };

        let result = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(result > 0 && (pollfd.revents & libc::POLLIN) != 0)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = set_term_attrs(&self.og_termios);
        if self.alt_screen {
            let _ = io::stdout().write_all(LEAVE_ALT_SCREEN.as_bytes());
            let _ = io::stdout().flush();
        }

    }
}

#[derive(Debug)]
pub enum TermPrepError {
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    UnsupportedOperatingSystem,
    BadTerminalSize,
    Io(io::Error),
}

impl From<io::Error> for TermPrepError {
    fn from(error: io::Error) -> TermPrepError {
        Self::Io(error)
    }
}

impl Display for TermPrepError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            Self::UnsupportedOperatingSystem => {
                write!(f, "This program is not supported on your operating system!")
            }
            Self::BadTerminalSize => write!(f, "Couldn't fetch terminal size!"),
            Self::Io(err) => write!(f, "Terminal: {err}"),
        }
    }
}

impl error::Error for TermPrepError {}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn term_attrs() -> Result<libc::termios, TermPrepError> {
    let mut termios = MaybeUninit::<libc::termios>::uninit();
    let result = unsafe { libc::tcgetattr(libc::STDIN_FILENO, termios.as_mut_ptr()) };

    if result == -1 {
        return Err(TermPrepError::Io(io::Error::last_os_error()));
    }

    Ok(unsafe { termios.assume_init() })
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn set_term_attrs(
    termios: &libc::termios,
) -> Result<(), TermPrepError> {
    let result =
        unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, termios) };
    if result == -1 {
        return Err(TermPrepError::Io(io::Error::last_os_error()));
    }

    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn term_dimensions() -> Result<(u16, u16), TermPrepError> {
    let mut winsz = MaybeUninit::<libc::winsize>::uninit();
    let result = unsafe {
        libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, winsz.as_mut_ptr())
    };

    if result == -1 {
        return Err(TermPrepError::Io(io::Error::last_os_error()));
    }

    let winsz = unsafe {winsz.assume_init() };
    if winsz.ws_col == 0 || winsz.ws_row == 0 {
        return Err(TermPrepError::BadTerminalSize);
    }

    Ok((winsz.ws_col, winsz.ws_row))
}
