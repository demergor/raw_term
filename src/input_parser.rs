use std::{collections::VecDeque, io};

use crate::{
    Key, terminal::{self, Terminal},
};

const DEFAULT_TIMEOUT_MS: i32 = 30;
const BACKSPACE: u8 = b'\x08';

// UTF8
const _ONE_BYTE_TEST: u8 = 0b1000_0000;
const _ONE_BYTE_EXPECTED: u8 = 0b0000_0000;
const TWO_BYTE_TEST: u8 = 0b1110_0000;
const TWO_BYTE_EXPECTED: u8 = 0b1100_0000;
const THREE_BYTE_TEST: u8 = 0b1111_0000;
const THREE_BYTE_EXPECTED: u8 = 0b1110_0000;
const FOUR_BYTE_TEST: u8 = 0b1111_1000;
const FOUR_BYTE_EXPECTED: u8 = 0b1111_0000;
const CONTINUATION_BYTE_TEST: u8 = 0b1100_0000;
const CONTINUATION_BYTE_EXPECTED: u8 = 0b1000_0000;

pub struct InputParser {
    pub pending_keys: VecDeque<Key>,

    state: ParseState,
    multi_byte_buf: Vec<u8>,

    poll_timeout_ms: i32,
}

impl InputParser {
    pub fn new(poll_timeout_ms: i32) -> Option<InputParser> {
        if poll_timeout_ms < 0 {
            return None;
        }

        Some(InputParser {
            pending_keys: VecDeque::new(),
            state: ParseState::Normal,
            multi_byte_buf: Vec::new(),
            poll_timeout_ms,
        })
    }

    pub fn poll_key(&mut self) -> bool {
        let Ok(ready) = Terminal::byte_ready(self.poll_timeout_ms) else {
            return true;
        };

        if !ready {
            match self.state {
                ParseState::Escape => self.pending_keys.push_back(Key::Escape),
                ParseState::Utf8(_) => self.multi_byte_buf.clear(),
                ParseState::Csi => {
                    self.pending_keys.push_back(Key::Escape);
                    self.pending_keys.push_back(Key::Char('['));
                }
                ParseState::CsiDel => {
                    self.pending_keys.push_back(Key::Escape);
                    self.pending_keys.push_back(Key::Char('['));
                    self.pending_keys.push_back(Key::Char('3'));
                }
                ParseState::Normal => (),
            }

            self.state = ParseState::Normal;
            return true;
        }

        match read_byte() {
            Ok(Some(byte)) => {
                self.handle_byte(byte);
                true
            }
            Ok(None) => false,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => true,
            Err(_) => false,
        }
    }

    fn handle_byte(&mut self, byte: u8) {
        match self.state {
            ParseState::Normal => self.handle_normal(byte),
            ParseState::CsiDel => match byte {
                b'~' => {
                    self.pending_keys.push_back(Key::Delete);
                    self.state = ParseState::Normal;
                }
                terminal::ESC => {
                    self.pending_keys.push_back(Key::Escape);
                    self.pending_keys.push_back(Key::Char('['));
                    self.pending_keys.push_back(Key::Char('3'));
                    self.state = ParseState::Escape;
                }
                byte => {
                    self.pending_keys.push_back(Key::Escape);
                    self.pending_keys.push_back(Key::Char('['));
                    self.pending_keys.push_back(Key::Char('3'));

                    self.state = ParseState::Normal;
                    self.handle_normal(byte);
                }
            }
            ParseState::Escape => match byte {
                b'[' => self.state = ParseState::Csi,
                terminal::ESC => self.pending_keys.push_back(Key::Escape),
                byte => {
                    self.pending_keys.push_back(Key::Escape);
                    self.state = ParseState::Normal;
                    self.handle_normal(byte);
                }
            },
            ParseState::Utf8(remaining_bytes) => {
                assert!(remaining_bytes <= 3 && remaining_bytes > 0);

                // Discard malformed utf8 code points
                if byte & CONTINUATION_BYTE_TEST != CONTINUATION_BYTE_EXPECTED {
                    self.multi_byte_buf.clear();
                    self.state = ParseState::Normal;
                    self.handle_normal(byte);
                    return;
                }

                self.multi_byte_buf.push(byte);
                if remaining_bytes > 1 {
                    self.state = ParseState::Utf8(remaining_bytes - 1);
                } else {
                    let Ok(s) = std::str::from_utf8(&self.multi_byte_buf) else {
                        self.multi_byte_buf.clear();
                        self.state = ParseState::Normal;
                        return;
                    };

                    let mut it = s.chars();
                    let Some(ch) = it.next() else {
                        return;
                    };

                    if it.next().is_some() {
                        panic!("Malformed UTF-8 sequence!");
                    }

                    self.multi_byte_buf.clear();
                    self.pending_keys.push_back(Key::Char(ch));
                    self.state = ParseState::Normal;
                };
            }
            ParseState::Csi => {
                self.state = ParseState::Normal;
                match byte {
                    b'3' => self.state = ParseState::CsiDel,
                    b'A' => self.pending_keys.push_back(Key::ArrowUp),
                    b'B' => self.pending_keys.push_back(Key::ArrowDown),
                    b'C' => self.pending_keys.push_back(Key::ArrowRight),
                    b'D' => self.pending_keys.push_back(Key::ArrowLeft),
                    byte => {
                        self.pending_keys.push_back(Key::Escape);
                        self.pending_keys.push_back(Key::Char('['));
                        self.handle_normal(byte);
                    }
                }
            }
        }
    }

    fn handle_normal(&mut self, byte: u8) {
        match byte {
            b'\x03' => self.pending_keys.push_back(Key::CtrlC),
            terminal::ESC => self.state = ParseState::Escape,
            b'\r' | b'\n' => self.pending_keys.push_back(Key::Enter),
            byte if byte & TWO_BYTE_TEST == TWO_BYTE_EXPECTED => {
                self.multi_byte_buf.push(byte);
                self.state = ParseState::Utf8(1)
            }
            byte if byte & THREE_BYTE_TEST == THREE_BYTE_EXPECTED => {
                self.multi_byte_buf.push(byte);
                self.state = ParseState::Utf8(2)
            }
            byte if byte & FOUR_BYTE_TEST == FOUR_BYTE_EXPECTED => {
                self.multi_byte_buf.push(byte);
                self.state = ParseState::Utf8(3)
            }
            BACKSPACE | b'\x7f' => self.pending_keys.push_back(Key::Backspace),
            byte if byte.is_ascii() => 
                self.pending_keys.push_back(Key::Char(char::from(byte))),
            _ => (),
        }
    }
}

impl Default for InputParser {
    fn default() -> InputParser {
        InputParser::new(DEFAULT_TIMEOUT_MS).unwrap()
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ParseState {
    Normal,
    Csi, // ESC [
    CsiDel, // ESC [ 3
    Escape,
    Utf8(u8),
}

fn read_byte() -> io::Result<Option<u8>> {
    let mut buf = [0u8; 1];
    let n = unsafe {
        libc::read(
            libc::STDIN_FILENO,
            buf.as_mut_ptr().cast::<libc::c_void>(),
            1,
        )
    };

    match n {
        -1 => Err(io::Error::last_os_error()),
        0 => Ok(None),
        _ => Ok(Some(buf[0])),
    }
}
