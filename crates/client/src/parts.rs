//! 개체 하나를 이루는 자식 도형들입니다. 몸통, 바라보는 방향을 알려 주는 점, HP 막대가 있습니다.
//!
//! 개체의 뿌리(그림자)는 `Parts`를 가지고, 자식 도형은 `Parts`가 없습니다. 이 차이로 시스템이 뿌리와 자식의 `Transform`을
//! 한꺼번에 가변으로 빌려도 서로 겹치지 않는다는 것을 Bevy가 확인할 수 있습니다.

use bevy::prelude::*;

use crate::config;
use crate::palette::Palette;

#[derive(Clone, Copy)]
pub struct Bar {
    back: Entity,
    fill: Entity,
}

#[derive(Component, Clone, Copy)]
pub struct Parts {
    pub body: Entity,
    pub nose: Entity,
    pub bar: Bar,
}

/// 자식 도형의 `Transform`과 `Visibility`를 읽고 쓰는 쿼리입니다.
pub type PartQuery<'w, 's> =
    Query<'w, 's, (&'static mut Transform, &'static mut Visibility), Without<Parts>>;

/// 자식 도형의 위치와 보이는지를 정합니다. 값이 같으면 바꾸지 않아서 수백 마리를 매 프레임 갱신해도 불필요한 변경 표시가 생기지 않습니다.
pub fn set_part(parts: &mut PartQuery, entity: Entity, transform: Transform, visible: bool) {
    if let Ok((mut current, mut visibility)) = parts.get_mut(entity) {
        current.set_if_neq(transform);
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// 막대의 뒤판과 채움의 위치입니다. `y`는 뿌리에서 막대까지의 높이(픽셀)이고, `fraction`이 없으면 막대를 숨깁니다.
pub struct BarLayout {
    pub back: Transform,
    pub fill: Transform,
    pub visible: bool,
}

pub fn bar_layout(y: f32, fraction: Option<f32>) -> BarLayout {
    let width = config::BAR_SIZE.x;
    let filled = width * fraction.unwrap_or(0.0).clamp(0.0, 1.0);
    let border = 2.0 * config::BAR_BORDER;
    BarLayout {
        back: Transform::from_xyz(0.0, y, 3.0 * config::PART_Z_STEP).with_scale(Vec3::new(
            width + border,
            config::BAR_SIZE.y + border,
            1.0,
        )),
        // 채움은 왼쪽 끝에서부터 차므로, 가운데를 채운 만큼의 절반만 오른쪽으로 옮깁니다.
        fill: Transform::from_xyz((filled - width) / 2.0, y, 4.0 * config::PART_Z_STEP)
            .with_scale(Vec3::new(filled.max(0.001), config::BAR_SIZE.y, 1.0)),
        visible: fraction.is_some(),
    }
}

/// 개체의 자식으로 HP 막대 도형 두 개를 만듭니다. 부모에 붙이는 일은 부른 쪽이 합니다.
pub fn spawn_bar(
    commands: &mut Commands,
    palette: &Palette,
    fill_material: &Handle<ColorMaterial>,
    y: f32,
    fraction: Option<f32>,
) -> Bar {
    let layout = bar_layout(y, fraction);
    let visibility = if layout.visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    let back = commands
        .spawn((
            Mesh2d(palette.square.clone()),
            MeshMaterial2d(palette.bar_back.clone()),
            layout.back,
            visibility,
        ))
        .id();
    let fill = commands
        .spawn((
            Mesh2d(palette.square.clone()),
            MeshMaterial2d(fill_material.clone()),
            layout.fill,
            visibility,
        ))
        .id();
    Bar { back, fill }
}

impl Bar {
    pub fn entities(self) -> [Entity; 2] {
        [self.back, self.fill]
    }

    pub fn place(self, parts: &mut PartQuery, y: f32, fraction: Option<f32>) {
        let layout = bar_layout(y, fraction);
        set_part(parts, self.back, layout.back, layout.visible);
        set_part(parts, self.fill, layout.fill, layout.visible);
    }
}
