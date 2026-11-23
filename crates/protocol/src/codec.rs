//! 메시지와 바이트 사이의 변환입니다. 전송 형식(postcard)을 아는 곳은 이 파일 하나이고, 접속 방식 어댑터는 이 함수들을 호출합니다.
//!
//! postcard는 필드 이름을 기록하지 않고 선언 순서대로 값만 기록합니다. 정수는 가변 길이로 기록하므로 작은 값은 1바이트입니다.

use std::fmt;

use crate::messages::{ClientMsg, ServerMsg};

/// 받은 바이트가 메시지 하나로 해석되지 않습니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError(String);

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot decode message: {}", self.0)
    }
}

pub fn encode_client(msg: &ClientMsg) -> Vec<u8> {
    postcard::to_allocvec(msg).expect(ENCODE_CANNOT_FAIL)
}

pub fn decode_client(bytes: &[u8]) -> Result<ClientMsg, DecodeError> {
    decode_exact(bytes)
}

pub fn encode_server(msg: &ServerMsg) -> Vec<u8> {
    postcard::to_allocvec(msg).expect(ENCODE_CANNOT_FAIL)
}

pub fn decode_server(bytes: &[u8]) -> Result<ServerMsg, DecodeError> {
    decode_exact(bytes)
}

/// postcard가 메모리 안의 `Vec`에 기록할 때 실패하는 경우는 길이를 미리 알 수 없는 목록뿐입니다.
/// serde의 derive가 만드는 `Vec`과 `String`의 기록은 항상 길이를 먼저 알려 주므로 실패하지 않습니다.
const ENCODE_CANNOT_FAIL: &str =
    "postcard encoding into a Vec cannot fail for derived message types";

/// 메시지 하나를 읽은 뒤에 남는 바이트가 있으면 오류로 처리합니다. 다른 형식의 데이터를 메시지로 잘못 읽는 일을 막습니다.
fn decode_exact<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, DecodeError> {
    let (msg, rest) =
        postcard::take_from_bytes::<T>(bytes).map_err(|err| DecodeError(err.to_string()))?;
    if !rest.is_empty() {
        return Err(DecodeError(format!(
            "{} unexpected trailing bytes",
            rest.len()
        )));
    }
    Ok(msg)
}
