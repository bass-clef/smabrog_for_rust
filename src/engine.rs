
use crate::data::*;
use crate::scene::*;


/// スマブラを管理するクラス
pub struct SmashBrogEngine {
    battle_history: BattleHistory,
    scene_manager: SceneManager,
}
impl Default for SmashBrogEngine {
    fn default() -> Self { Self::new() }
}
impl SmashBrogEngine {
    fn new() -> Self {
        Self {
            battle_history: BattleHistory::default(),
            scene_manager: SceneManager::default(),
        }
    }

    pub fn update(&mut self) -> opencv::Result<()> {
        self.scene_manager.update()?;

        Ok(())
    }
}
