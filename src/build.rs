
extern crate winres;

fn main() {
    if cfg!(target_os = windows) {
        let mut exe_resource = winres::WindowsResource::new();
        exe_resource.set_icon("icon/smabrog.ico");
        exe_resource.compile().unwrap();
    }
}
