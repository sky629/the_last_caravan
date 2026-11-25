//! 방이 지금 무엇을 하는지입니다. 방은 야영지, 원정, 결과 화면을 차례로 돌고, 단계마다 그 단계에서만 의미 있는 값을 가집니다.
//! 그래서 야영지에 낮의 남은 시간이 있거나, 원정에 결과가 있는 상태는 표현할 수 없습니다.
//!
//! ```text
//!   방이 만들어짐 --> Camp --(모두가 준비하고 카운트다운이 끝남)--> Expedition(Day 1) --(성공이나 실패)--> Debrief
//!   Debrief --(모두가 확인하거나 결과를 보여 주는 시간이 지남)--> Camp
//! ```
//!
//! `CampTuning::start`가 `Start::AtOnce`이면 야영지를 건너뜁니다. 방은 곧바로 원정을 시작하고, 결과 화면이 끝나면 다음 원정을 시작합니다.
//! 단계를 바꾸는 곳은 `Sim`의 `begin_expedition`, `conclude`, `enter_camp` 셋뿐입니다.

use crate::camp::Camp;
use crate::debrief::Debrief;
use crate::expedition::Phase;

#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    Camp(Camp),
    /// 원정의 낮, 밤, 새벽입니다.
    Expedition(Phase),
    Debrief(Debrief),
}
