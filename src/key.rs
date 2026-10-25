#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    Char(char),

    Enter,
    Backspace,
    Delete,
    Escape,

    ArrowUp,
    ArrowDown,
    ArrowRight,
    ArrowLeft,

    CtrlC,
}
