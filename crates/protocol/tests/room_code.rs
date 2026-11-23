use caravan_protocol::{RoomCode, RoomCodeError};

#[test]
fn codes_are_trimmed_and_lowercased_so_case_does_not_split_a_room() {
    assert_eq!(RoomCode::parse("  Test ").unwrap().as_str(), "test");
    assert_eq!(RoomCode::parse("TEST"), RoomCode::parse("test"));
    assert_eq!(RoomCode::parse("a-b_9").unwrap().as_str(), "a-b_9");
}

#[test]
fn empty_too_long_and_odd_characters_are_rejected() {
    assert_eq!(RoomCode::parse("   "), Err(RoomCodeError::Empty));
    assert_eq!(
        RoomCode::parse(&"a".repeat(16)).map(|c| c.as_str().len()),
        Ok(16)
    );
    assert_eq!(
        RoomCode::parse(&"a".repeat(17)),
        Err(RoomCodeError::TooLong { max: 16 })
    );
    assert_eq!(RoomCode::parse("my room"), Err(RoomCodeError::BadChar(' ')));
    assert_eq!(RoomCode::parse("방"), Err(RoomCodeError::BadChar('방')));
}
