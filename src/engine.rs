
use crate::scene::*;


/* スマブラを管理するクラス */
#[derive(Default)]
pub struct SmashBrogEngine {
    scene_manager: SceneManager,

}
impl SmashBrogEngine {
    pub fn update(&mut self) -> opencv::Result<()> {
        self.scene_manager.update()?;

        Ok(())
    }
}
