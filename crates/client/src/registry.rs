//! 서버의 개체 번호에서 Bevy 개체로 가는 표입니다. 플레이어, 몬스터, 감시탑이 같은 방식으로 씁니다.
//!
//! 한 프레임에 한 번 보인 번호를 기억해서, 보이지 않은 번호의 개체를 지웁니다.
//! 새 번호만 만들고 사라진 번호만 지우므로 프레임마다 모든 개체를 다시 만들지 않습니다.

use std::collections::HashMap;

use bevy::prelude::*;

/// 방을 그리려고 만든 개체의 뿌리입니다. 방을 떠나면 이 표시가 있는 개체를 모두 지웁니다(`view::clear_world`).
/// 번호로 추적하는 개체는 `Registry::insert`가 표시하고, 지형은 `view`가 표시합니다. 자식은 뿌리와 함께 지워집니다.
#[derive(Component)]
pub struct RoomEntity;

#[derive(Default)]
pub struct Registry {
    entities: HashMap<u32, Tracked>,
    frame: u64,
}

struct Tracked {
    entity: Entity,
    seen_in_frame: u64,
}

impl Registry {
    pub fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// 번호의 개체가 이미 있으면 이번 프레임에도 보였다고 표시하고 돌려줍니다.
    pub fn find(&mut self, id: u32) -> Option<Entity> {
        let tracked = self.entities.get_mut(&id)?;
        tracked.seen_in_frame = self.frame;
        Some(tracked.entity)
    }

    /// 새로 만든 개체를 번호에 연결하고, 방을 떠날 때 지워지도록 표시합니다.
    pub fn insert(&mut self, commands: &mut Commands, id: u32, entity: Entity) {
        commands.entity(entity).insert(RoomEntity);
        self.entities.insert(
            id,
            Tracked {
                entity,
                seen_in_frame: self.frame,
            },
        );
    }

    /// 이번 프레임에 보이지 않은 번호의 개체를 지웁니다. 몬스터가 죽거나, 플레이어가 나가거나, 감시탑이 부서졌을 때입니다.
    pub fn despawn_unseen(&mut self, commands: &mut Commands) {
        let frame = self.frame;
        self.entities.retain(|_, tracked| {
            let seen = tracked.seen_in_frame == frame;
            if !seen {
                commands.entity(tracked.entity).despawn();
            }
            seen
        });
    }

    /// 번호의 개체를 찾기만 합니다. `find`와 달리 이번 프레임에 보였다고 표시하지 않습니다.
    pub fn entity_of(&self, id: u32) -> Option<Entity> {
        self.entities.get(&id).map(|tracked| tracked.entity)
    }
}
