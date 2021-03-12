
use std::time::Duration;
use std::thread::sleep;
use opencv::prelude::*;

mod scene;
use scene::SceneManager;

/* 戦歴を管理するクラス */
struct BattleHistory {

}
impl BattleHistory {

}

/* スマブラを管理するクラス */
struct SmashBrogEngine {
    sceneManager: SceneManager,
}
impl SmashBrogEngine {
    fn new() -> SmashBrogEngine {
        SmashBrogEngine{
            sceneManager: SceneManager::new()
        }
    }

    fn main(&self) -> bool {
        self.sceneManager.whichScene();

        return false;
    }
}

/* メインループ */
fn main() {
    let engine = SmashBrogEngine::new();

    loop {
        if engine.main() {
            break;
        }

        sleep(Duration::from_millis(1));
    }
}
