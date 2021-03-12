
struct SceneUnknown {}
struct SceneDialog {}
/* シーン単体(動作は継承先による)を管理する */
trait Scene {
    fn isScene(&self) -> bool;
}

/* 状態不明のシーン */
impl SceneUnknown {
    fn new() -> SceneUnknown {
        SceneUnknown {
        }
    }
}
impl Scene for SceneUnknown {
    fn isScene(&self) -> bool {
        false
    }
}

/* ダイアログが表示されているシーン */
impl SceneDialog {
    fn new() -> SceneUnknown {
        SceneUnknown {
        }
    }
}

/* シーン全体を非同期で管理するクラス */
pub struct SceneManager {
    sceneList: Vec<Box<dyn Scene>>,
}
impl SceneManager {
    pub fn new() -> SceneManager {
        SceneManager{
            sceneList: vec![ Box::new(SceneUnknown::new()) ]
        }
    }

    pub fn whichScene(&self) {

    }
}
