
use crate::scene::*;


/* スマブラを管理するクラス */
#[derive(Default)]
pub struct SmashBrogEngine {
    sceneManager: SceneManager,

}
impl SmashBrogEngine {
    pub fn update(&mut self) -> opencv::Result<()> {
        self.sceneManager.update()?;

        Ok(())
    }
}
