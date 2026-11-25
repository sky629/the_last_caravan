# THE LAST CARAVAN

THE LAST CARAVAN은 2~4명이 이동식 마을(마차)을 함께 지키는 협동 액션 로그라이크 게임입니다.
이 저장소에는 낮과 밤이 이어지는 프로토타입이 있습니다. 플레이어들은 맵 한가운데의 마차를 사흘 밤 동안 몰려오는 몬스터로부터 지킵니다. 낮에는 먼 곳의 채집 지점에서 자원을 모아 마차에 가져다 두고, 그 자원으로 마차 가까이에 감시탑을 세웁니다. 또 몬스터가 지키는 무기 상자를 열어서 무기를 얻습니다. 플레이어는 무기를 두 개까지 가지고 다니며 `Q`로 바꿔 듭니다. 밤에는 감시탑과 함께 몬스터를 막고, 쓰러진 동료를 부활시키고, 손상된 마차를 수리합니다. 전용 서버가 이동, 전투, 채집, 건설, 부활, 수리, 상자 열기, 무기 줍기의 결과를 모두 판정합니다.

## 설치하기

아래 명령은 macOS에서 확인했습니다.

1. rustup을 설치합니다. 설치가 끝나면 새 터미널을 엽니다.

   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. 저장소를 내려받고 저장소 폴더로 이동합니다.

   ```sh
   git clone https://github.com/sky629/the_last_caravan.git
   cd the_last_caravan
   ```

3. 고정된 툴체인을 설치합니다. `rust-toolchain.toml`이 Rust 1.100.0, rustfmt, clippy, `wasm32-unknown-unknown` 타깃을 지정하므로, 저장소 폴더에서 이 명령 하나로 웹 빌드용 타깃까지 설치됩니다.

   ```sh
   rustup toolchain install
   ```

   `rustc --version`이 `rustc 1.100.0`을 출력하고 `rustup target list --installed`에 `wasm32-unknown-unknown`이 있으면 설치가 끝난 것입니다.

4. 웹 빌드 도구인 trunk 0.22.2를 설치합니다. trunk는 처음 웹 빌드를 할 때 `Cargo.lock`에 맞는 버전의 wasm-bindgen을 내려받습니다.

   ```sh
   cargo install --locked trunk --version 0.22.2
   ```

Ubuntu에서 클라이언트를 빌드하려면 Bevy가 링크하는 라이브러리를 먼저 설치합니다. CI가 설치하는 목록과 같습니다.

```sh
sudo apt-get install pkg-config libudev-dev libwayland-dev
```

## 실행하기

### 전용 서버를 실행합니다

클라이언트보다 서버를 먼저 실행합니다. 서버는 `listening on ws://127.0.0.1:7777`을 출력하고, 그 뒤에는 일어난 일만 한 줄씩 출력합니다. 출력하는 일은 플레이어의 입장과 퇴장(방 코드와 인원), 방의 생성과 제거, 한도 때문에 서버가 닫은 접속과 그 이유, 시작 오류입니다. `Ctrl-C`로 종료합니다.

```sh
cargo run -p caravan_dedicated
```

플레이어 한 명이 방 `test`에 들어갔다가 나가면 출력은 다음과 같습니다.

```text
listening on ws://127.0.0.1:7777
room test created (seed 8201742364921307566)
player 1 joined room test (1 members)
player 1 left room test (0 members)
room test removed
```

| 옵션 | 설명 |
|---|---|
| `--addr HOST:PORT` | 접속을 받을 주소입니다. 기본값은 `127.0.0.1:7777`입니다. 다른 컴퓨터에서 접속하려면 `0.0.0.0:7777`을 지정합니다. |
| `--debug-net` | 주고받는 모든 메시지를 읽을 수 있는 형태로 출력합니다. 스냅샷은 몬스터 목록을 수로 줄여서 출력합니다. |
| `--stats` | 1초마다 방, 플레이어, 몬스터의 수와 Tick 시간의 평균과 최대를 한 줄씩 출력합니다. 예를 들면 `stats rooms=1 members=2 monsters=87 ticks=20 tick_avg_ms=0.31 tick_max_ms=1.20`입니다. |
| `--waves SIZE,SIZE,.../SIZE,...` | 밤마다 무리의 수와 무리마다 몬스터의 수를 바꿉니다. 밤은 `/`로, 무리는 `,`로 나누고, 밤의 수가 원정의 날 수입니다. `--waves 100,200,300`은 밤 하나에 무리 셋(100마리, 200마리, 300마리)을, `--waves 8,12/20,30,40`은 밤 둘(첫째 밤 8마리와 12마리, 둘째 밤 20마리, 30마리, 40마리)을 정합니다. |
| `--stress` | 마차, 감시탑, 플레이어가 죽지 않게 하고, 감시탑을 상한(40개)만큼 미리 세우고, 낮을 한 Tick으로 줄이고, 몬스터를 Tick마다 한 마리씩 등장시킵니다. 새 원정이 시작될 때마다 감시탑이 다시 서 있습니다. 플레이어는 검과 활을 함께 가지고 시작합니다. |
| `--seed N` | 모든 방의 무작위를 씨앗 `N`(0 이상의 정수) 하나로 정합니다. 주지 않으면 방마다, 서버를 실행할 때마다 다른 씨앗을 씁니다. 방을 만들 때 출력하는 `seed` 값을 이 옵션에 주면 같은 상태에서 다시 시작하므로, 판에서 본 일을 다시 만들 수 있습니다. 예를 들어 `--seed 7`이면 첫 원정의 첫날 상자는 (17, 32)에 창, (32, 17)에 활, (49, 49)에 대검이 든 채로 놓입니다. `crates/rules/src/tests/chests.rs`의 `seed_7_places_these_three_chests_on_the_first_day`가 이 값을 고정합니다. |

`--waves`와 `--stress`는 몬스터가 수백 마리일 때 서버의 Tick 시간과 스냅샷의 크기를 재기 위한 부하 시험용 옵션입니다. Tick 시간은 `--stats`를 붙여야 출력됩니다.
부하 시험용 클라이언트(`crates/dedicated/examples/load_client.rs`)는 Bevy 없이 한 방에 접속해서 Tick마다 입력을 보내고, 받은 스냅샷의 크기와 몬스터 수를 1초마다 출력합니다.

```sh
cargo run --release -p caravan_dedicated -- --stress --waves 100,200,300 --stats
cargo run --release -p caravan_dedicated --example load_client -- --addr 127.0.0.1:7777 --room load --secs 60
```

부하 시험용 클라이언트의 옵션은 `--addr HOST:PORT`, `--room CODE`, `--secs SECONDS`, `--passive`, `--bow`입니다. `--passive`를 붙이면 공격하지 않으므로 몬스터가 가장 많이 쌓입니다. `--bow`를 붙이면 원정마다 한 번 활로 바꾸고 탄환을 쏩니다(서버를 `--stress`로 실행해야 활을 가지고 있습니다). 활은 쿨다운이 있으므로 플레이어 넷이 `--bow`로 쏴도 동시에 날아다니는 탄환은 한 자릿수입니다(감시탑 40개가 서 있고 몬스터가 260마리쯤 몰릴 때 측정한 최대는 6개입니다). 탄환이 100개 넘게 날아다니는 부하는 이 시험이 아니라 `cargo test --release -p caravan_rules perf -- --nocapture`의 시험이 만듭니다. 부하 시험용 클라이언트가 출력하는 `projectiles=`와 마지막 줄의 `peak_projectiles`로 날아다니는 탄환의 수를 확인할 수 있습니다. `--secs 60`으로는 몬스터가 181마리까지만 모이므로, 260마리 넘게 모으려면 `--secs 150`이 필요합니다.

### PC 클라이언트를 실행합니다

두 클라이언트를 같은 방에 넣으려면 터미널 두 개에서 같은 명령을 실행합니다. 두 창에 같은 맵과 두 캐릭터가 나타납니다.

```sh
cargo run -p caravan_client -- --room test
```

다른 방 코드로 실행한 클라이언트는 다른 방에 들어가므로, 위의 두 클라이언트와 서로 보이지 않습니다.

```sh
cargo run -p caravan_client -- --room other
```

| 옵션 | 설명 |
|---|---|
| `--room CODE` | 방 코드입니다. 기본값은 `test`입니다. 1~16자의 `a-z`, `0-9`, `-`, `_`를 쓰고, 대문자는 소문자로 바뀝니다. 처음 들어간 사람이 방을 만들고, 마지막 사람이 나가면 방이 사라집니다. |
| `--server URL` | 서버 주소입니다. 기본값은 `ws://127.0.0.1:7777`입니다. `ws://`나 `wss://`로 시작해야 합니다. |
| `--debug-net` | 주고받는 메시지를 로그에 출력합니다. |

옵션이 잘못되었거나 서버에 접속하지 못하면 창 가운데에 이유가 표시됩니다. 접속이 한 번도 열리지 않았을 때는 접속하려 한 서버 주소와 서버가 실행 중인지 확인하라는 안내가 표시됩니다. 클라이언트는 1초마다 프레임 속도, 가장 최근에 받은 서버 Tick, 그 스냅샷의 몬스터 수를 로그로 출력합니다.

### 웹 클라이언트를 실행합니다

`crates/client`에서 `trunk serve`를 실행하면 trunk가 클라이언트를 wasm으로 빌드하고 `http://127.0.0.1:8080/`에서 페이지를 제공합니다.

```sh
cd crates/client
trunk serve
```

브라우저에서 `http://127.0.0.1:8080/?room=test`를 엽니다. 주소의 쿼리는 PC 클라이언트의 옵션과 같은 규칙으로 해석됩니다.

| 쿼리 | 설명 |
|---|---|
| `room=CODE` | 방 코드입니다. 기본값은 `test`입니다. |
| `server=URL` | 서버 주소입니다. 기본값은 `ws://127.0.0.1:7777`입니다. |
| `debug-net` | 주고받는 메시지를 브라우저 개발자 도구의 콘솔에 출력합니다. |

모든 쿼리를 쓴 주소는 `http://127.0.0.1:8080/?room=test&server=ws://127.0.0.1:7777&debug-net`입니다. 웹 클라이언트와 PC 클라이언트에 같은 방 코드를 주면 같은 방에서 함께 플레이합니다.

### 게임의 진행

원정은 첫날의 낮으로 시작하고, 화면 오른쪽 위에 지금이 몇 번째 날의 낮인지 밤인지(`DAY 1 / 3`, `NIGHT 1 / 3`)와 낮의 남은 시간이나 무리의 번호가 나옵니다.

1. 낮은 90초 동안 이어지고, 플레이어들은 이 시간에 감시탑을 세우고 채집 지점에서 자원을 모읍니다. 보관함에는 처음부터 자원이 10 있어서 감시탑 하나를 바로 세울 수 있습니다. 채집 지점은 여덟 곳이고, 지점마다 낮의 몬스터 둘이 곁에서 지킵니다. 낮의 몬스터는 자기 자리에서 6타일 안으로 들어온 플레이어를 쫓고, 플레이어가 자리에서 6타일보다 멀어지면 자리로 돌아갑니다. 낮이 시작될 때 무기 상자 세 개가 후보 자리 여덟 곳 가운데 서로 다른 세 곳에 놓이고, 상자마다 낮의 몬스터 셋이 곁에서 지킵니다. 상자를 열어서 무기를 얻는 방법은 아래의 「무기」에 있습니다.
2. 낮이 끝나면 밤이 시작되고, 처치하지 않은 낮의 몬스터는 자리를 떠나 마차로 향하고, 열지 않은 상자는 사라집니다. 3초 뒤에 첫 무리가 마차에서 18타일 떨어진 곳에 한 마리씩 나타납니다. 무리마다 나타나는 방향이 달라지고, 여덟 방향을 모두 씁니다.
3. 무리의 몬스터를 모두 처치하면 4초 뒤에 그 밤의 다음 무리가 나타납니다. 그 밤의 마지막 무리를 처치하면 4초 뒤에 다음 날의 낮이 시작됩니다. 첫째 밤은 8마리와 12마리, 둘째 밤은 20마리, 30마리, 40마리, 셋째 밤은 60마리, 90마리, 150마리입니다.
4. 다음 낮이 시작될 때 모든 플레이어가 최대 HP의 살아 있는 상태가 됩니다. 쓰러졌거나 사망한 플레이어도 그 자리에서 되살아납니다. 채집 지점은 다시 가득 차고, 낮의 몬스터가 지점마다 다시 놓이고, 무기 상자가 새로 놓입니다. 감시탑과 그 HP, 마차의 HP, 보관함과 플레이어가 든 자원, 땅에 떨어진 자원은 그대로 이어집니다.
5. 셋째 밤을 막으면 `EXPEDITION SUCCESS`가 나옵니다. 마차의 HP가 0이 되거나 모든 플레이어가 쓰러졌거나 사망하면 낮이든 밤이든 `EXPEDITION FAILED`가 나옵니다.
6. 결과를 5초 동안 보여 준 뒤 같은 방에서 새 원정이 첫날부터 시작되고, 감시탑과 보관함과 마차의 HP가 처음으로 돌아갑니다.

살아 있는 플레이어가 마차에서 3타일 안에 있으면 낮이든 밤이든, 공격받는 중이든 HP가 1초에 5씩 회복됩니다. 쓰러진 플레이어는 회복되지 않습니다. 마차의 HP는 날이 바뀌어도 저절로 회복되지 않으므로, 플레이어가 마차 곁에서 `F`를 눌러 수리합니다. 수리는 감시탑을 짓는 것과 같은 보관함의 자원을 씁니다.

한 밤에 동시에 살아 있는 몬스터는 300마리까지이고, 이 수에 이르면 무리의 등장이 멈췄다가 몬스터가 줄면 이어집니다.

### 무기

플레이어는 무기를 두 칸에 가집니다. 원정은 검 하나와 빈 칸 하나로 시작하고, 나머지 무기는 낮에 상자에서 얻습니다. 무기마다 다른 수치는 `crates/rules/src/tuning.rs`의 무기 표(`WeaponTable`)에서 무기 하나가 한 행입니다. 부채꼴의 거리는 플레이어의 가장자리에서 몬스터의 가장자리까지 잰 값이고, 각도는 조준 방향의 양쪽으로 잰 절반 각도입니다.

| 무기 | 화면의 이름 | 공격 방식 | 피해량 | 쿨다운 | 닿는 곳 | 상자에서 나옴 |
|---|---|---|---|---|---|---|
| 검 | `SWORD` | 부채꼴 | 100 | 0.5초 | 1.0타일, 양쪽으로 60도씩 | 아니요 (처음에 듭니다) |
| 대검 | `GREATSWORD` | 부채꼴 | 180 | 1.0초 | 1.3타일, 양쪽으로 90도씩 | 예 |
| 창 | `SPEAR` | 부채꼴 | 120 | 0.6초 | 2.2타일, 양쪽으로 15도씩 | 예 |
| 활 | `BOW` | 탄환 | 60 | 0.4초 | 초당 14타일의 속도, 사거리 9타일, 반지름 0.15타일 | 예 |

피해량은 정상 값(검 25, 대검 45, 창 30, 활 15)에 테스트용 배율 4(`PLAYTEST_DAMAGE_MULTIPLIER`)를 곱한 값입니다. 서비스하기 전에 이 배율을 1로 내려야 하고, 무시된 시험 `before_a_release_the_damage_is_the_normal_damage`가 정상 값을 확인합니다. 몬스터(`Grunt`)의 HP는 60입니다.

무기를 얻고 가지는 규칙은 다음과 같습니다.

1. 상자는 낮이 시작될 때마다 세 개가 놓입니다. 후보 자리 여덟 곳(`MapSpec::chest_sites`) 가운데 서로 다른 세 곳이 뽑히고, 마을의 범위 밖입니다. 상자 안의 무기는 상자마다 따로, 검을 뺀 무기 가운데 같은 확률로 하나가 정해집니다. 그래서 같은 무기가 두 상자에 들어 있을 수도 있습니다. 어느 자리에 어떤 무기가 든 상자가 놓이는지는 방의 씨앗이 정하므로, 같은 씨앗이면 같고(`--seed`), 화면에는 상자의 안이 보이지 않습니다.
2. 상자는 채집 지점처럼 타일 하나를 막고, 상자마다 낮의 몬스터 셋이 곁에서 지킵니다. 이 몬스터는 채집 지점을 지키는 몬스터와 같은 규칙을 따라서, 자리에서 6타일 안으로 들어온 플레이어를 쫓고 밤이 되면 마차로 향합니다. 열지 않은 상자는 밤이 시작될 때 사라집니다.
3. 상자 곁에서 `F`를 1.5초 끊지 않고 누르고 있으면 상자가 사라지고, 상자가 있던 타일의 중심에 상자 안의 무기가 떨어집니다. 플레이어의 가장자리에서 상자의 가장자리까지 1타일 안이면 상자를 열 수 있습니다. 여는 동안에는 이동과 공격을 할 수 없고, 피해를 받거나 `F`를 놓으면 처음부터 다시 눌러야 합니다. 지키는 몬스터가 살아 있어도 맞지 않는 동안에는 열 수 있습니다. 여는 도중에 동료가 쓰러져도 여는 일은 이어지고, 다 연 뒤에 `F`를 계속 누르고 있으면 부활이 시작됩니다.
4. 바닥의 무기 곁에서 `F`를 누른 순간 무기를 줍습니다. 플레이어의 가장자리에서 무기가 놓인 점까지 1.75타일 안이면 줍습니다. 이 거리는 상자를 열 수 있는 거리(1타일)에 타일의 대각선 절반(약 0.71타일)을 더한 값보다 짧을 수 없으므로(`Tuning::validate`가 거절합니다), 상자를 열 수 있는 어느 자리에서든 걷지 않고 `F`를 한 번 눌러서 그 무기를 주울 수 있습니다. 빈 칸이 있으면 빈 칸에 들어가고, 칸이 모두 차 있으면 든 무기와 바뀌어서 이전에 든 무기가 바닥의 그 자리에 놓입니다. 그래서 무기를 동료에게 넘길 수 있습니다.
5. 가진 무기는 쓰러져도, 날이 바뀌어도 유지됩니다. 플레이어가 방에서 나가면 가진 무기가 그 자리에 떨어지지만, 시작 무기와 같은 종류의 무기(기본 수치에서는 검)는 떨어지지 않습니다. 입장하는 플레이어가 공짜로 받는 무기이기 때문입니다. 바닥의 무기는 원정이 끝날 때까지 남고, 새 원정에서는 모든 플레이어가 검 하나로 돌아가며 바닥의 무기와 상자가 사라집니다.

### 조작

| 입력 | 동작 |
|---|---|
| `W` `A` `S` `D` | 화면을 기준으로 위, 왼쪽, 아래, 오른쪽으로 이동합니다. |
| `Shift`를 누르고 있기 | 움직이는 동안 걷는 속도의 1.6배로 달립니다. 달리는 동안 화면 왼쪽 위 HP 아래의 `STAMINA` 막대가 줄고, 키를 떼면 막대가 다시 찹니다. 왼쪽과 오른쪽 `Shift`가 같습니다. |
| 마우스 | 커서가 가리키는 지면 위치로 조준합니다. |
| 왼쪽 클릭 | 들고 있는 무기로 공격합니다. 검, 대검, 창은 조준한 방향의 부채꼴 안에 있는 모든 몬스터를 베고, 활은 조준한 방향으로 탄환을 쏩니다. 부채꼴의 거리와 각도, 쿨다운은 무기마다 다릅니다. |
| `Q` | 가진 두 칸의 무기를 서로 바꿉니다. 한 칸만 차 있으면 아무것도 바뀌지 않습니다. 누르고 있어도 한 번만 바뀝니다. 화면 오른쪽 위의 `WEAPON  [SWORD]  -  [Q]`나 `WEAPON  [BOW]  SWORD  [Q]`가 가진 무기를 알려 주고, 대괄호가 든 무기입니다. 부활, 수리, 채집, 건설, 상자 열기 중이거나 쓰러졌을 때는 바뀌지 않습니다. |
| `B`를 누르고 있기 | 바라보는 쪽의 이웃한 타일에 감시탑을 짓습니다(비용 10, 3초). 키를 누르기 전에도 그 타일이 밝은 테두리로 보입니다. 마차에서 10타일 안이어야 하고, 마차로 가는 길을 막을 수 없습니다. 세울 수 없으면 그 타일을 빨간 테두리로 표시하고 이유를 화면 아래에 알립니다. 짓는 동안 이동과 공격을 할 수 없고, 짓는 중에 `B`를 떼거나 쓰러지면 건설이 취소되고 비용을 돌려받습니다. |
| `F`를 누르고 있기 | 가까이 쓰러진 동료가 있으면 부활시키고(5초), 없으면 가까이 있는 손상된 감시탑을 수리하고(1.5초에 HP 30, 자원 1), 그것도 없으면 가까이 있는 손상된 마차를 수리하고(1.5초에 HP 30, 자원 1), 그것도 없으면 가까이 있는 채집 지점에서 자원을 0.5초에 1씩 채집합니다(최대 10). 그것도 없으면 가까이 있는 무기 상자를 1.5초 걸려 엽니다(피해를 받으면 처음으로 돌아갑니다). 그동안 이동과 공격을 할 수 없습니다. 채집한 자원은 마차 곁(1.5타일 안)에 가면 자동으로 보관함에 들어가고, 쓰러지거나 접속을 끊으면 그 자리에 떨어지며, 살아 있는 플레이어가 가까이 가면 자동으로 줍습니다. |
| `F`를 누른 순간 | 바닥의 무기 곁(1.75타일 안)에서 누르면 그 무기를 줍습니다. 빈 칸이 있으면 빈 칸에 들어가고, 없으면 든 무기와 바뀌어서 이전에 든 무기가 그 자리에 떨어집니다. 누르고 있어도 한 번만 줍고, 위의 부활, 수리, 채집, 상자 열기를 할 수 있는 곳에서는 줍지 않습니다. 줍을 수 있는 무기 곁에 서면 `F  TAKE SPEAR` 같은 안내가 나오고, 열 수 있는 상자 곁에서는 `F  OPEN CHEST`가 나옵니다. 상자를 여는 동안에는 `OPENING CHEST` 막대가 진행을 보여 줍니다. |

수리는 한 번(1.5초)마다 보관함에서 자원 1을 내고, 마지막 한 번이 HP를 30보다 적게 회복시켜도 같습니다. HP가 거의 없는 감시탑을 가득 수리하면 자원 10(건설 비용과 같습니다)이 들고, 마차는 17이 듭니다. 여럿이 같은 Tick에 수리하면 번호가 작은 플레이어부터 내고, 보관함이 모자라면 나중 플레이어의 그 수리는 회복시키지 못합니다. 보관함이 비용을 낼 수 없는 동안에도 플레이어는 그 수리를 계속하고 채집으로 바뀌지 않으며, 화면 아래에 `CANNOT REPAIR: NOT ENOUGH IN THE STASH`가 나옵니다. 자원이 생기면 이 문구가 사라지고 다음 수리부터 회복시킵니다. 마차 곁의 회복은 자원을 쓰지 않습니다.

달리기 게이지는 가득 찬 막대로 4초를 달릴 수 있고, 빈 막대가 가득 차는 데 10초가 걸립니다. 서버는 이 게이지를 정수 점수로 세고 스냅샷에는 0에서 100의 백분율 하나만 싣습니다. 달리는 Tick에는 막대가 줄고, `Shift`를 뗀 Tick에는 움직이든 서 있든 막대가 찹니다. `Shift`를 누르고 있지만 달리지 못하는 동안(막대가 비었거나 제자리에 서 있을 때)에는 막대가 줄지도 차지도 않으므로, 빈 막대에서 `Shift`를 누르고 있으면 달리기와 걷기가 번갈아 나오지 않고 걷기만 합니다. 부활, 수리, 채집, 건설을 하는 동안과 쓰러졌을 때는 달릴 수 없습니다. 막대는 입장할 때, 낮이 시작될 때, 새 원정이 시작될 때, 부활한 직후에 가득 찹니다.

카메라는 내 캐릭터를 따라갑니다. 마차가 화면 밖에 있으면 마차가 있는 쪽의 화면 가장자리에 `WAGON 23`처럼 내 캐릭터에서 마차까지의 타일 수가 나옵니다.

## 검사하기

CI(`.github/workflows/ci.yml`)는 `main`에 푸시할 때와 풀 리퀘스트마다 Ubuntu 24.04에서 다음 명령을 모두 실행합니다. 저장소 폴더에서 같은 명령을 실행해서 로컬에서도 확인할 수 있습니다.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p caravan_client --target wasm32-unknown-unknown -- -D warnings
cargo test --workspace
sh scripts/check-deps.sh
cd crates/client && trunk build
```

- `cargo test --workspace`는 게임 규칙, 프로토콜, 서버, 클라이언트 로직을 시험합니다. 대부분의 테스트는 화면과 네트워크 없이 실행되고, `crates/dedicated/tests/real_socket.rs`, `crates/ws_server/tests/roundtrip.rs`, `crates/ws_client/tests/message_limit.rs`만 `127.0.0.1`의 실제 소켓을 씁니다.
- `sh scripts/check-deps.sh`는 계층 사이의 금지된 의존이 없는지 검사하고, 통과하면 `dependency rules hold`를 출력합니다.
- `trunk build`는 웹 클라이언트를 `crates/client/dist`에 빌드합니다.
- 감시탑이 상한만큼 서 있고 몬스터가 300마리 있을 때 Tick 하나의 계산 시간은 릴리스 빌드에서만 한계를 확인합니다. 평균은 Tick 간격(50ms)의 10% 이하, 감시탑을 짓거나 부순 Tick과 상자가 열리는 Tick은 한 간격 이하여야 합니다. `cargo test --release -p caravan_rules perf -- --nocapture`가 시간을 출력하고, 일반 `cargo test`는 이 상황이 말한 대로 만들어졌는지만 확인합니다. 활을 든 플레이어 넷이 쏘는 상황과 탄환 144개가 동시에 날아다니는 상황의 시간도 같은 명령이 출력하고, 이때 날아다니는 탄환의 평균과 최대 수가 함께 출력됩니다. 스냅샷의 크기는 `crates/protocol/tests/snapshot_size.rs`와 `crates/server/tests/load.rs`가 확인합니다.
- 측정한 값은 다음과 같습니다. 컴퓨터는 macOS(Mac16,8, 14코어)이고, 릴리스 빌드이며, 같은 명령을 세 번 실행한 값을 실행한 순서대로 적었습니다.
  - Tick 하나의 평균(감시탑 40개, 몬스터 300마리, 1000 Tick)은 224.7µs, 226.1µs, 223.2µs입니다. 활을 든 플레이어 넷이 쏘면 227.5µs, 228.8µs, 229.6µs이고, 탄환 144개가 날아다니면 235.9µs, 236.0µs, 237.3µs입니다. 한계는 5ms입니다.
  - 상자가 열리는 Tick(열린 Tick과 다음 Tick 가운데 느린 쪽)은 1.72ms, 1.67ms, 1.64ms입니다. 한계는 50ms입니다. 평균보다 긴 원인은 확인하지 않았습니다.
  - 스냅샷 하나는 `snapshot_size.rs`의 상황(몬스터 300마리, 감시탑 40개, 플레이어 4명, 바닥의 무기 4개)에서 3,417바이트입니다. 플레이어 한 명이 있는 `load.rs`의 실제 규칙 상태에서는 3,362바이트입니다.
  - 전용 서버와 부하 시험용 클라이언트 하나를 실제 소켓으로 150초 실행했을 때(`--stress --waves 100,200,300 --stats`) 몬스터는 최대 267마리였고 가장 큰 스냅샷은 3,220바이트였습니다. 1초 구간의 Tick 평균은 중앙값 0.17ms, 가장 큰 구간이 1.26ms였습니다. 구간 안의 가장 느린 Tick은 한 번 16.52ms였고 원인은 확인하지 않았습니다.

## 구조

워크스페이스는 크레이트 여덟 개로 나뉩니다. 의존은 안쪽 계층(표의 아래쪽)으로만 향합니다.

| 계층 | 크레이트 | 역할 | 의존할 수 있는 것 |
|---|---|---|---|
| 도메인 | `caravan_rules` | 게임 규칙 전체입니다. 이동과 달리기, 충돌, 전투, 몬스터와 낮의 몬스터, 무기 상자와 바닥의 무기, 낮과 밤과 밤의 무리, 채집과 자원, 마차와 감시탑과 건설과 수리, 마차 곁의 회복, 쓰러짐과 부활, 원정의 성공과 실패를 계산합니다. | 없음 |
| 계약 | `caravan_protocol` | 메시지, 규칙 상태를 전송 형식으로 바꾸는 변환, 바이트 변환, 접속 방식 인터페이스 두 개, 프로세스 내부 전달, 입장 안내의 상한 검사입니다. 서버는 시작할 때, 클라이언트는 입장 안내를 받을 때 같은 검사를 합니다. | `caravan_rules`, serde, postcard |
| 애플리케이션 | `caravan_server` | 방, Tick 일정, 입장과 퇴장, 입력 처리입니다. 소켓, 스레드, 현재 시각을 직접 쓰지 않습니다. | `caravan_rules`, `caravan_protocol` |
| 애플리케이션 | `caravan_client_core` | 클라이언트에서 Bevy가 필요하지 않은 로직입니다. 접속 상태, 입력 전송, 보간, 쿼터뷰 투영, 카메라의 계산, 건설할 타일의 미리보기, HUD의 내용, 실행 옵션을 다룹니다. | `caravan_rules`, `caravan_protocol` |
| 어댑터 | `caravan_ws_server` | WebSocket으로 서버 쪽 접속 방식 인터페이스를 구현합니다 (PC 전용). | `caravan_protocol`, tokio, tokio-tungstenite, futures-util |
| 어댑터 | `caravan_ws_client` | WebSocket으로 클라이언트 쪽 접속 방식 인터페이스를 구현합니다 (PC와 웹). | `caravan_protocol`, ewebsock |
| 조립 | `caravan_dedicated` | 전용 서버 실행 파일입니다. 게임 서버에 WebSocket 어댑터를 연결해서 실행합니다. | `caravan_rules`, `caravan_protocol`, `caravan_server`, `caravan_ws_server` |
| 조립 | `caravan_client` | Bevy 앱입니다. 입력, 화면, HUD를 맡고 클라이언트 코어와 WebSocket 어댑터를 연결합니다. Bevy를 쓰는 크레이트는 이것 하나입니다. | `caravan_rules`, `caravan_protocol`, `caravan_client_core`, `caravan_ws_client`, bevy, web-sys (웹 빌드만) |

아래 그림에서 화살표는 의존하는 방향입니다. 표의 마지막 열이 각 크레이트의 `Cargo.toml`에 적힌 실제 의존 목록이고, 그림은 방향만 보여 줍니다.

```text
caravan_dedicated                       caravan_client              조립
   |          |                            |          |
   v          |                            |          v
caravan_ws_server                          |    caravan_ws_client   어댑터
   |          v                            v          |
   |    caravan_server       caravan_client_core      |             애플리케이션
   |          |                            |          |
   v          v                            v          v
   +-----------------> caravan_protocol <-------------+             계약
                              |
                              v
                        caravan_rules                               도메인
```

각 크레이트의 `Cargo.toml`이 허용되는 의존을 정하고, `scripts/check-deps.sh`가 `cargo tree`로 다섯 개의 배포 플랫폼에서 금지된 크레이트(예: 규칙과 서버에 Bevy, 계약과 애플리케이션 계층에 소켓 크레이트)가 직접 의존과 간접 의존 어디에도 없는지 검사합니다.

## Tick 하나의 처리 순서

`Sim::step`(`crates/rules/src/sim.rs`)은 한 Tick에 다음 순서로 상태를 바꿉니다. 모든 공격과 피해는 같은 상태를 읽고 결정한 뒤 14번에서 한꺼번에 적용합니다.

1. `advance_phase`가 낮, 밤, 새벽, 결과 화면의 카운트다운을 줄이고, 때가 되면 밤이나 낮을 시작하거나 몬스터를 한 마리 등장시킵니다. 낮이 시작되면 상자를 놓고 상자마다 몬스터를 세우고(`loot::start_day`), 밤이 시작되면 열지 않은 상자를 치웁니다(`loot::clear_chests`). 결과 화면의 동안에는 여기서 Tick이 끝납니다.
2. `Paths::sync`가 마차와 감시탑으로 가는 이동 경로를 필요하면 다시 만듭니다.
3. `MonsterGrid::rebuild`가 몬스터의 위치를 격자에 넣습니다.
4. `update_interactions`가 `F`와 `B`를 처리합니다. 부활, 감시탑 수리, 마차 수리, 채집, 상자 열기의 순서로 대상을 고르고, 수리의 비용을 보관함에서 내고(플레이어 번호 순서), 건설의 시작과 거절을 판정합니다. 부활, 수리, 채집 다음에 상자 열기가 오고, `F`를 누른 순간에 더 높은 순위의 대상이 없으면 가장 가까운 바닥의 무기를 줍습니다(플레이어 번호 순서).
5. `advance_construction`이 건설을 진행하고 끝내거나 취소합니다.
6. 건설이 이동 경로를 바꿨을 수 있으므로 `Paths::sync`를 한 번 더 부릅니다.
7. `update_sprint`가 플레이어마다 이번 Tick에 달리는지 정하고 달리기 게이지를 줄이거나 채웁니다. 4번과 5번이 정한 하고 있는 일을 읽으므로 그 뒤에 부릅니다.
8. `move_players`가 플레이어를 움직입니다.
9. `deposit_and_pickup`이 마차 곁의 플레이어가 든 자원을 보관함에 넣고, 땅에 떨어진 자원을 줍게 합니다.
10. `use_weapons`가 `Q`를 누른 플레이어의 무기를 바꾸고, 플레이어의 공격을 결정하고, 들고 있는 무기의 전달 방식에 따라 처리합니다. 검, 대검, 창은 부채꼴 안의 몬스터를 맞히고, 활은 탄환을 만듭니다.
11. `advance_projectiles`가 날아가는 탄환을 한 Tick만큼 움직입니다. 탄환은 이번 Tick에 쏜 것도 움직이고, 지나간 선분에서 처음 닿는 몬스터 한 마리의 피해를 결정하며, 막힌 타일이나 사거리의 끝에서는 피해 없이 사라집니다. 끝난 탄환은 한 Tick 더 남았다가 다음 Tick의 처음에 지워집니다. 결과 화면 동안에는 날던 탄환이 그 자리에 멈추고, 끝난 탄환은 지워집니다.
12. `towers_fire`가 감시탑의 사격을 결정합니다.
13. `monsters_act`가 몬스터의 이동과 공격을 결정합니다. 낮의 몬스터는 자리를 지키고, 밤의 몬스터는 마차로 향합니다.
14. `World::apply_hits`가 이번 Tick의 모든 피해를 적용하고, HP가 0이 된 몬스터와 감시탑을 지우고, 쓰러진 플레이어가 든 자원을 그 자리에 떨어뜨립니다. 상자를 열던 플레이어가 피해를 받으면 열기가 여기서 취소됩니다.
15. `recover_near_wagon`이 마차 곁의 살아 있는 플레이어의 HP를 회복합니다. 피해를 적용한 뒤에 부르므로, 같은 Tick의 회복이 쓰러뜨리는 피해를 되돌리지 않습니다.
16. `advance_life_timers`가 부활을 마무리하고 출혈 시간을 줄입니다.
17. `finish_openings`가 열기를 마친 플레이어의 상자를 열어서 무기를 바닥에 놓습니다. 피해를 적용한 뒤에 부르므로, 열기가 끝나는 Tick에 맞은 플레이어는 열지 못합니다.
18. `settle_outcome`이 원정의 성공과 실패, 무리를 모두 처치했는지를 판정합니다.

## 키 입력 하나가 다른 플레이어의 화면에 그려지기까지

플레이어 A가 `D`를 누르면, 다음 순서를 거쳐 플레이어 B의 화면에서 A의 캐릭터가 움직입니다.

1. `crates/client/src/input.rs`의 `read_controls`가 이번 프레임의 키보드와 마우스 상태를 `Controls`로 읽습니다. 커서의 화면 위치는 지면 위치로 되돌립니다.
2. `crates/client/src/net.rs`의 `send_input`이 `Session::input_due`(`crates/client_core/src/session.rs`)를 부릅니다. `InputSender::poll`(`crates/client_core/src/input.rs`)은 서버 Tick 간격(20 Hz에서 50ms)마다 한 번 `Controls::to_input`으로 순서 번호가 붙은 `InputMsg`를 만들고, 이때 화면 기준의 WASD 방향이 지면의 이동 방향으로 바뀝니다.
3. `WsClient::send`(`crates/ws_client/src/lib.rs`)가 `encode_client`(`crates/protocol/src/codec.rs`)로 메시지를 postcard 바이트로 바꾸고 WebSocket 이진 프레임으로 보냅니다.
4. 서버의 네트워크 스레드에서 `serve_link`(`crates/ws_server/src/lib.rs`)가 프레임을 `decode_client`로 해석하고, 크기가 정해진 채널로 게임 스레드에 넘깁니다.
5. `crates/dedicated/src/main.rs`의 반복이 `GameServer::pump`(`crates/server/src/lib.rs`)를 부릅니다. `pump`는 `WsServer::poll`로 꺼낸 메시지를 `Hub::handle_message`(`crates/server/src/hub.rs`)에 넘기고, 입력은 `Room::submit_input`(`crates/server/src/room.rs`)을 거쳐 `MemberInput::accept`(`crates/server/src/input.rs`)에 쌓입니다. 이미 받은 순서 번호보다 크지 않은 입력은 여기서 버립니다.
6. Tick 시각이 되면(`crates/server/src/clock.rs`의 `TickClock::due`) `Room::tick`이 플레이어마다 `MemberInput::for_tick`으로 이번 Tick의 입력 하나를 정하고 `Sim::step`(`crates/rules/src/sim.rs`)을 한 번 부릅니다.
7. `Sim::step` 안에서 `move_players`(`crates/rules/src/player.rs`)가 이동 방향에 속도(`Player::stats`가 돌려주는 값이고, 달리는 중이면 걷는 속도에 달리기 배율을 곱한 값입니다)와 Tick 길이를 곱해서 캐릭터를 옮기고, `crates/rules/src/collision.rs`가 장애물과 겹친 만큼 캐릭터를 밀어냅니다.
8. `Snapshot::from_sim`(`crates/protocol/src/snapshot.rs`)이 방 전체의 상태를 전송 형식으로 바꿉니다. `Room::tick`은 같은 스냅샷을 방의 모든 접속에 보내라는 `Out::Send`를 돌려주고, `GameServer::apply`가 `WsServer::send`를 불러서 `encode_server`로 바꾼 바이트를 보냅니다.
9. B의 클라이언트에서 `pump_network`(`crates/client/src/net.rs`)가 `WsClient::poll`로 스냅샷을 받고(`decode_server`), `Session::on_event`가 스냅샷을 `SnapshotBuffer`(`crates/client_core/src/interp.rs`)에 넣습니다.
10. `advance_playback`(`crates/client/src/net.rs`)이 재생 시각을 프레임 시간만큼 앞으로 보냅니다. `SnapshotBuffer`는 가장 최근 스냅샷보다 2 Tick(100ms) 이전 시점의 위치를 그 앞뒤 스냅샷 두 개 사이에서 선형으로 보간해서 `Frame`을 만듭니다.
11. `sync_characters`(`crates/client/src/characters.rs`)가 `Frame`의 지면 위치를 `convert::placement`(`crates/client/src/convert.rs`)로 화면 위치와 z 값으로 바꿉니다. 이 변환은 `crates/client_core/src/iso.rs`의 `ground_to_screen`과 `depth`를 씁니다. Bevy가 바뀐 `Transform`대로 도형을 그리면 B의 화면에서 A의 캐릭터가 움직입니다.

## 코드를 읽는 순서

Rust와 게임 개발을 처음 공부한다면 다음 순서로 읽습니다. 규칙에서 시작해서 프로토콜, 서버, 어댑터, 클라이언트 코어, 클라이언트로 나아갑니다.

1. `crates/rules/src/tuning.rs`에서는 게임의 모든 수치가 한 구조체에 모인 모습과, `Default` 트레이트 구현, 그리고 초를 정수 Tick으로 바꾸는 `Tuning::ticks`를 봅니다.
2. `crates/rules/src/sim.rs`의 `Sim::step`은 Tick 하나의 처리 순서를 나열하며, `world`만 가변 참조로 넘기고 `tuning`과 `paths` 같은 다른 필드는 `Ctx`에 공유 참조로 빌려 주는 방식이 구조체 필드를 따로 빌리는 소유권 규칙의 첫 예입니다.
3. `crates/rules/src/player.rs`의 `Life`는 데이터를 가지는 열거형의 예이고, 쓰러진 플레이어가 HP나 쿨다운을 가지는 상태를 타입으로 표현할 수 없게 만듭니다. `crates/rules/src/loadout.rs`의 `Loadout`은 무기 두 칸을 타입으로 표현합니다. 든 무기(`held`)가 `Option`이 아니라서 「칸이 둘 다 빈 상태」를 만들 수 없고, 필드가 비공개라서 상태를 바꾸는 방법이 `swap`(`Q`)과 `take`(`F`로 줍기) 둘뿐입니다. `crates/rules/src/weapon.rs`는 공격을 두 단계(`decide`가 공격을 모으고, `deliver`가 `Delivery`의 값에 따라 전달합니다)로 나누고, 무기마다 다른 수치는 `crates/rules/src/tuning.rs`의 무기 표(`WeaponTable`) 한 행에 둡니다. `crates/rules/src/projectile.rs`는 탄환을 쏜 플레이어와 따로 세계의 개체 목록에 두고, `crates/rules/src/sweep.rs`는 한 Tick에 움직인 선분 전체를 검사해서 빠른 탄환이 얇은 몬스터를 건너뛰지 못하게 합니다. `crates/rules/src/stamina.rs`의 `Stamina`는 정수 하나를 새 타입(newtype)으로 감싸서, 달리기 게이지를 소수 없이 정수 점수로 세는 방법을 한 곳에 모읍니다.
4. `crates/rules/src/world/tiles.rs`의 `TileMap`은 어디가 막혔는지 아는 유일한 곳이고, `crates/rules/src/build.rs`는 감시탑을 놓을 수 있는 조건을 한 함수(`check_site`)로 판정합니다. `crates/rules/src/expedition.rs`는 낮, 밤, 새벽, 결과 화면의 단계를 열거형(`Phase`)으로 두고 단계를 바꾸는 곳을 두 함수로 모았으며, 낮이 시작될 때 일어나는 일은 `begin_day` 하나에 있습니다. `crates/rules/src/loot.rs`는 무기 상자와 바닥의 무기를 다루고, 규칙에서 난수를 쓰는 곳은 이 모듈의 `dice` 하나입니다(상자의 자리와 내용을 씨앗에서 뽑습니다). `Chest`의 내용(`contents`)은 규칙 크레이트 안에서만 보이므로, 프로토콜 크레이트가 상자 안의 무기를 읽어서 스냅샷에 실으면 컴파일되지 않습니다. `crates/rules/src/resources.rs`는 자원이 채집 지점, 플레이어의 손, 땅, 보관함 가운데 한 곳에만 있도록 옮기는 규칙입니다. `crates/rules/src/monster.rs`, `crates/rules/src/nav.rs`, `crates/rules/src/spatial.rs`에서는 몬스터의 명령(`Order`: 낮에는 자리를 지키고 밤에는 마차로 향합니다), 목표 선택, 흐름장(flow field)으로 장애물을 피하는 경로 탐색, 균일 격자로 주변 몬스터를 찾는 방법을 배웁니다.
5. `crates/rules/src/tests/support.rs`와 같은 폴더의 테스트는 화면과 네트워크 없이 개체를 원하는 자리에 놓고 `step`을 반복해서 규칙을 시험하는 방법을 보여 줍니다. 상자는 `chests.rs`(자리와 내용, 씨앗, 지키는 몬스터)와 `opening.rs`(여는 일)가, 바닥의 무기는 `loot.rs`가 시험합니다.
6. `crates/protocol/src/messages.rs`의 `ClientMsg`와 `ServerMsg`는 serde의 derive로 직렬화하는 열거형이며, 클라이언트가 보낼 수 있는 게임 정보가 입력뿐이라는 서버 권위 방식이 이 타입에 드러납니다.
7. `crates/protocol/src/snapshot.rs`의 `Snapshot::from_sim`은 게임 상태를 전송 형식으로 바꾸는 유일한 함수이며, 규칙의 상태 타입과 전송용 타입을 분리한 모습을 보여 줍니다.
8. `crates/protocol/src/units.rs`는 실수 좌표를 1/128 타일 단위의 정수로 줄이는 양자화(quantization)로 스냅샷의 크기를 줄입니다.
9. `crates/protocol/src/transport.rs`의 `ServerTransport`와 `ClientTransport`는 접속 방식을 트레이트로 정의한 인터페이스이고, 구현이 지켜야 하는 약속이 문서 주석에 있습니다.
10. `crates/protocol/src/codec.rs`의 `decode_exact`는 트레이트 경계(`T: DeserializeOwned`)를 가진 제네릭 함수의 예입니다.
11. `crates/protocol/src/loopback.rs`는 같은 트레이트의 두 번째 구현이며, `std::sync::mpsc` 채널로 같은 프로세스 안에서 메시지를 옮깁니다.
12. `crates/server/src/input.rs`는 불규칙하게 도착하는 입력을 Tick마다 하나씩 넘기는 입력으로 바꾸고, `Option`으로 「아직 받은 입력이 없음」을 표현합니다. 입력이 늦는 Tick에는 마지막 입력을 다시 쓰는데, 누른 순간인 `Q`와 `F`는 `PlayerInput::held`가 지워서 되풀이되지 않게 합니다.
13. `crates/server/src/room.rs`와 `crates/server/src/hub.rs`에서는 허브가 접속 방식을 직접 부르지 않고 할 일을 `Out` 목록으로 돌려주는 방식과, 접속 상태를 열거형(`LinkState`)으로 표현하는 방식을 봅니다.
14. `crates/server/src/clock.rs`는 고정 간격의 Tick 일정과 밀린 Tick의 상한을 다루며, 현재 시각을 인자로 받으므로 테스트가 가짜 시각을 쓸 수 있습니다.
15. `crates/server/src/lib.rs`의 `GameServer`는 접속 방식을 `Box<dyn ServerTransport>`로 가지는 트레이트 객체의 예입니다.
16. `crates/ws_server/src/lib.rs`에서는 tokio의 async와 await, `tokio::select!`, 크기가 정해진 채널, 그리고 값이 버려질 때 접속 자리를 돌려주는 `Drop` 구현(`Seat`)을 봅니다.
17. `crates/ws_client/src/lib.rs`는 외부 라이브러리(ewebsock)를 이 프로젝트의 트레이트 뒤에 감싸는 어댑터이고, PC와 웹에서 같은 코드가 동작합니다.
18. `crates/dedicated/src/main.rs`는 실행 인자를 해석하고, 다른 트레이트 객체를 감싸서 메시지를 기록하는 `DebugTransport`를 끼운 뒤, `pump`를 부르고 쉬는 서버의 반복을 실행합니다.
19. `crates/client_core/src/session.rs`의 `Session`은 열거형으로 만든 상태 기계이며, 방에 들어간 뒤에만 필요한 값을 `InRoom(Joined)` 안에만 둡니다.
20. `crates/client_core/src/interp.rs`에서는 스냅샷을 보관하고 일정한 지연을 두고 두 스냅샷 사이를 보간(interpolation)하는 방법을 배웁니다. 탄환은 번호로 앞뒤 스냅샷의 같은 탄환을 찾아서 위치를 섞고, 앞쪽 스냅샷에만 있는 탄환(끝난 탄환)은 끝난 자리에 그립니다.
21. `crates/client_core/src/iso.rs`는 지면 좌표를 쿼터뷰 화면 좌표로 바꾸는 투영과 그 역변환, 그리고 화면에서 아래쪽에 있는 개체를 앞에 그리는 순서(`depth`)를 다룹니다. `crates/client_core/src/camera.rs`는 카메라가 내 캐릭터를 따라가는 속도와, 화면 밖의 마차를 가리키는 표시의 자리를 Bevy 없이 계산하는 순수 함수(`follow`, `wagon_marker`)이고, `crates/client_core/src/preview.rs`는 건설 키를 누르기 전에 감시탑이 놓일 타일을 서버와 같은 함수(`front_tile`)로 정합니다.
22. `crates/client_core/src/input.rs`의 `InputSender`는 프레임보다 느린 주기로 입력을 보내면서, 두 입력 사이에 눌렀다 뗀 클릭과 `Q`와 `F`를 잃지 않도록 기억하고 입력에 한 번만 싣습니다.
23. `crates/client/src/main.rs`에서는 Bevy의 `App`, 플러그인, `.chain()`으로 순서를 고정한 시스템 목록, 그리고 `#[cfg(target_arch = "wasm32")]`로 PC와 웹의 코드를 나누는 조건부 컴파일을 봅니다.
24. `crates/client/src/net.rs`는 Bevy의 리소스와 시스템 매개변수(`Res`, `ResMut`, `NonSendMut`)의 예이고, 다른 스레드로 옮길 수 없는 웹의 WebSocket을 non-send 리소스로 보관합니다.
25. `crates/client/src/loot.rs`는 상자와 바닥의 무기를 스냅샷의 번호 목록에 맞춰 도형으로 만들고 갱신합니다. `crates/client/src/effects.rs`는 부채꼴 공격과 탄환과 감시탑의 사격을 상태에서 선으로 그리고, 부채꼴의 거리와 각도는 입장 안내의 무기 표에서 읽습니다. `crates/client/src/view.rs`, `crates/client/src/towers.rs`, `crates/client/src/supplies.rs`, `crates/client/src/characters.rs`에서는 Bevy의 개체(entity)와 컴포넌트를 만들고 갱신하는 방법을 배우며, `characters.rs`의 테스트는 `MinimalPlugins`로 창 없이 Bevy 시스템을 시험합니다.
26. `crates/client/src/hud.rs`는 `caravan_client_core::Hud`가 정한 내용(무기 두 칸, `F`의 안내, 상자를 여는 진행 막대)을 Bevy UI 노드에 옮깁니다.

## 현재의 한계

- 클라이언트 예측이 없습니다. 내 캐릭터도 서버의 스냅샷으로만 움직이므로, 키를 누른 뒤 서버까지의 왕복 시간과 보간 지연(100ms)이 지나야 화면에서 움직입니다.
- 재접속이 없습니다. 접속이 끊기면 화면에 `DISCONNECTED`와 이유가 표시되고, 서버는 그 플레이어의 캐릭터를 즉시 지웁니다. 클라이언트를 다시 실행하면 새 플레이어로 참가합니다.
- 서버는 Tick마다 방 전체의 스냅샷(모든 플레이어, 몬스터, 시설, 날아가는 탄환, 상자, 바닥의 무기)을 방의 모든 접속에 보냅니다. 바뀐 부분만 보내지 않습니다.
- 브라우저 탭이 백그라운드로 가면 브라우저가 페이지의 실행을 제한해서 입력 전송이 멈춥니다. 서버는 입력 없이 10 Tick(0.5초)보다 오래 지나면 그 캐릭터를 아무것도 누르지 않은 상태로 움직이고, 메시지 없이 30초가 지나면 접속을 닫고 플레이어를 방에서 뺍니다. 탭으로 돌아와도 다시 접속하지 않으므로 페이지를 새로 고쳐야 합니다.
- 서버는 TLS를 처리하지 않으므로 `ws://` 접속만 받습니다.
- 서버의 기본 주소와 `trunk serve`의 주소(`crates/client/Trunk.toml`)는 `127.0.0.1`이므로, 기본 설정에서는 같은 컴퓨터에서만 접속할 수 있습니다.
- 한 방의 최대 인원은 4명입니다(`crates/rules/src/tuning.rs`의 `max_players`). 가득 찬 방에 접속한 클라이언트는 거절되고 화면에 이유가 표시됩니다.
- 로비 화면이 없으므로 방 코드는 실행할 때 지정합니다.
- 맵은 64 x 64 타일 하나이고, 몬스터는 한 종류, 시설은 감시탑 하나, 자원은 한 종류입니다.
- 화면의 문구는 영어입니다. Bevy의 기본 글꼴이 한글을 표시하지 못하기 때문입니다. 무기의 이름도 영어입니다.
- 탄환과 공격 효과는 선으로 그리고, 상자와 바닥의 무기는 단순한 도형으로 그립니다.
- 바닥의 검을 늘리는 방법이 하나 남아 있습니다. 상자에서 나온 무기 둘이 바닥에 있을 때, 입장해서 둘을 줍고(두 번째를 주울 때 시작 무기인 검이 바닥에 놓입니다) 퇴장하면, 줍은 두 무기는 다시 떨어지고 검은 바닥에 남습니다. 이 일을 되풀이할 때마다 바닥의 검이 하나씩 늘어납니다. 바닥의 검을 주운 채로 퇴장하면 검이 떨어지지 않고 사라지므로 바닥의 검을 치울 수는 있습니다.
- `trunk serve`와 `trunk build`는 최적화하지 않은 디버그 빌드를 만들므로 wasm 파일이 약 110MB입니다.
