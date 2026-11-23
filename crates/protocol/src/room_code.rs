//! 방 코드입니다. 클라이언트는 실행 인자를 읽을 때, 서버는 `Join` 메시지를 받을 때 같은 함수로 해석합니다.

use std::fmt;

/// 1~16자의 `a-z`, `0-9`, `-`, `_`입니다. 해석할 때 소문자로 바꾸므로 "Test"와 "test"는 같은 방입니다.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RoomCode(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoomCodeError {
    Empty,
    TooLong { max: usize },
    BadChar(char),
}

impl RoomCode {
    pub const MAX_LEN: usize = 16;

    pub fn parse(raw: &str) -> Result<RoomCode, RoomCodeError> {
        let code = raw.trim().to_ascii_lowercase();
        if code.is_empty() {
            return Err(RoomCodeError::Empty);
        }
        if let Some(bad) = code
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_'))
        {
            return Err(RoomCodeError::BadChar(bad));
        }
        // 위에서 ASCII 문자만 남겼으므로 바이트 수가 글자 수와 같습니다.
        if code.len() > Self::MAX_LEN {
            return Err(RoomCodeError::TooLong { max: Self::MAX_LEN });
        }
        Ok(RoomCode(code))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RoomCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for RoomCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RoomCodeError::Empty => write!(f, "room code is empty"),
            RoomCodeError::TooLong { max } => {
                write!(f, "room code is longer than {max} characters")
            }
            RoomCodeError::BadChar(c) => {
                write!(f, "room code contains '{c}'; use a-z, 0-9, - or _")
            }
        }
    }
}
