//! 서버가 만드는 새 방의 코드와 자리의 표식입니다.
//!
//! 서버는 운영체제의 무작위를 직접 읽지 않으므로, 호출하는 쪽이 실행할 때마다 달라지는 비밀 값(`ServerSecret`)을 넘깁니다.
//! 이 파일은 그 비밀 값과 지금까지 만든 수를 표준 라이브러리의 해시(`DefaultHasher`)에 넣어서 값을 만듭니다.
//! 표식은 그런 값 두 개(16바이트)이고, 방 코드는 값 하나에서 다섯 글자를 뽑습니다. 표식은 서버 프로세스 하나의 수명 동안만 쓰이므로 해시 알고리즘이
//! 바뀌어도 문제가 없습니다. 테스트는 고정된 비밀 값으로 같은 값을 다시 만듭니다.

use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hasher};

use caravan_protocol::{RoomCode, SeatToken};

use crate::ServerSecret;
use crate::room::Room;

/// 새 방의 코드에 쓰는 문자입니다. 헷갈리는 0, o, 1, l, i를 뺀 31자입니다.
const CODE_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
/// 새 방의 코드의 길이입니다. 31의 5제곱은 약 2,860만 가지입니다.
const CODE_LEN: usize = 5;

pub(crate) struct Mint {
    secret: ServerSecret,
    /// 지금까지 만든 값의 수입니다. 같은 값을 두 번 넣지 않습니다.
    issued: u64,
}

impl Mint {
    pub fn new(secret: ServerSecret) -> Mint {
        Mint { secret, issued: 0 }
    }

    /// 64비트 값 하나를 만듭니다.
    fn next(&mut self) -> u64 {
        self.issued += 1;
        let mut hasher = DefaultHasher::new();
        hasher.write_u64(self.secret.k0);
        hasher.write_u64(self.secret.k1);
        hasher.write_u64(self.issued);
        hasher.finish()
    }

    /// 새 자리의 표식입니다. 비밀 값에서 나오고 이미 만든 어느 값과도 겹치지 않습니다.
    pub fn token(&mut self) -> SeatToken {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&self.next().to_le_bytes());
        bytes[8..].copy_from_slice(&self.next().to_le_bytes());
        SeatToken(bytes)
    }

    /// 지금 없는 방의 새 코드입니다. 있는 방과 겹치면 다시 만듭니다.
    pub fn room_code(&mut self, rooms: &BTreeMap<RoomCode, Room>) -> RoomCode {
        loop {
            let mut value = self.next();
            let mut text = String::with_capacity(CODE_LEN);
            for _ in 0..CODE_LEN {
                text.push(char::from(
                    CODE_ALPHABET[(value % CODE_ALPHABET.len() as u64) as usize],
                ));
                value /= CODE_ALPHABET.len() as u64;
            }
            // 문자표가 방 코드의 문자만 담고 있으므로 해석에 실패하지 않습니다. 실패하면 다시 만듭니다.
            if let Ok(code) = RoomCode::parse(&text)
                && !rooms.contains_key(&code)
            {
                return code;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: ServerSecret = ServerSecret { k0: 1, k1: 2 };

    #[test]
    fn a_new_code_has_five_unambiguous_characters() {
        let mut mint = Mint::new(SECRET);
        for _ in 0..200 {
            let code = mint.room_code(&BTreeMap::new());
            assert_eq!(code.as_str().len(), 5);
            assert!(
                code.as_str()
                    .chars()
                    .all(|c| !"0o1li".contains(c) && RoomCode::allows(c)),
                "{code}"
            );
        }
    }

    #[test]
    fn the_same_secret_gives_the_same_codes_and_another_secret_gives_other_codes() {
        let codes = |secret| {
            let mut mint = Mint::new(secret);
            (0..5)
                .map(|_| mint.room_code(&BTreeMap::new()))
                .collect::<Vec<_>>()
        };
        assert_eq!(codes(SECRET), codes(SECRET));
        assert_ne!(codes(SECRET), codes(ServerSecret { k0: 3, k1: 4 }));
    }

    #[test]
    fn tokens_differ_each_time_and_follow_the_secret() {
        let tokens = |secret| {
            let mut mint = Mint::new(secret);
            (0..50).map(|_| mint.token()).collect::<Vec<_>>()
        };
        let first = tokens(SECRET);
        assert_eq!(
            first,
            tokens(SECRET),
            "the same secret gives the same tokens"
        );
        assert_ne!(first, tokens(ServerSecret { k0: 3, k1: 4 }));
        let distinct: std::collections::BTreeSet<_> = first.iter().map(|t| t.0).collect();
        assert_eq!(distinct.len(), first.len(), "no token twice");
    }

    #[test]
    fn successive_codes_differ() {
        let mut mint = Mint::new(SECRET);
        let first = mint.room_code(&BTreeMap::new());
        let second = mint.room_code(&BTreeMap::new());
        assert_ne!(first, second);
    }
}
