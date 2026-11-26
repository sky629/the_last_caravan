//! `WireReport::from_report`가 규칙의 결과를 전송 값으로 옮기는지 확인합니다. 기록의 네 값은 서로 다른 수로 정해서, 자리가 바뀌면 드러나게 합니다.

use caravan_protocol::{WireRecordRow, WireReport};
use caravan_rules::{Outcome, PlayerId, Record, Report};

fn report() -> Report {
    Report {
        outcome: Outcome::Victory,
        day: 3,
        elapsed_ticks: 872 * 20 + 19,
        tower_kills: 41,
        rows: vec![
            (
                PlayerId(2),
                Record {
                    kills: 88,
                    revives: 2,
                    downs: 1,
                    deposited: 34,
                },
            ),
            (
                PlayerId(5),
                Record {
                    kills: 300,
                    revives: 0,
                    downs: 3,
                    deposited: 52,
                },
            ),
        ],
    }
}

fn name(player: PlayerId) -> String {
    format!("NAME{}", player.0)
}

#[test]
fn the_summary_and_a_row_for_each_player_are_carried_in_the_order_of_the_report() {
    let wire = WireReport::from_report(&report(), 4, 20, name);

    assert_eq!(wire.expedition, 4);
    assert_eq!(wire.day, 3);
    assert_eq!(wire.tower_kills, 41);
    assert_eq!(
        wire.rows,
        vec![
            WireRecordRow {
                name: "NAME2".to_string(),
                kills: 88,
                revives: 2,
                downs: 1,
                deposited: 34,
            },
            WireRecordRow {
                name: "NAME5".to_string(),
                kills: 300,
                revives: 0,
                downs: 3,
                deposited: 52,
            },
        ]
    );
}

#[test]
fn the_time_is_whole_seconds_and_the_part_of_a_second_is_dropped() {
    let wire = WireReport::from_report(&report(), 1, 20, name);
    assert_eq!(wire.elapsed_secs, 872, "872 seconds and 19 ticks");

    let mut exact = report();
    exact.elapsed_ticks = 0;
    assert_eq!(WireReport::from_report(&exact, 1, 20, name).elapsed_secs, 0);

    let faster = WireReport::from_report(&report(), 1, 40, name);
    assert_eq!(faster.elapsed_secs, 436, "the same ticks at 40 Hz");
}

#[test]
fn a_time_too_long_to_count_in_seconds_stops_at_the_largest_number() {
    let mut endless = report();
    endless.elapsed_ticks = u64::MAX;
    assert_eq!(
        WireReport::from_report(&endless, 1, 20, name).elapsed_secs,
        u32::MAX
    );
}
