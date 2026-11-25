//! 창을 보지 않고도 동기화와 부하를 확인할 수 있도록, 1초에 한 줄씩 상태를 기록합니다.
//! PC에서는 표준 출력으로, 브라우저에서는 개발자 도구의 콘솔로 나갑니다.

use bevy::prelude::*;
use caravan_client_core::FpsMeter;

use crate::net::ClientFlow;

/// 프레임 속도, 서버가 가장 최근에 보낸 Tick, 그 스냅샷의 몬스터와 탄환의 수를 기록합니다.
pub fn report(time: Res<Time<Real>>, flow: Res<ClientFlow>, mut meter: Local<FpsMeter>) {
    let Some(fps) = meter.frame(time.elapsed_secs_f64()) else {
        return;
    };
    match flow
        .0
        .session()
        .and_then(|session| session.newest_snapshot())
    {
        Some(snapshot) => info!(
            "fps {fps:.0} | newest tick {} | monsters {} | projectiles {}",
            snapshot.tick,
            snapshot.monsters.len(),
            snapshot.projectiles.len()
        ),
        None => info!("fps {fps:.0} | no snapshot yet"),
    }
}
