#[derive(Clone)]
pub enum AndaxError {
    // rhai_fn, fn_src, E
    RustReport(String, String, std::rc::Rc<color_eyre::Report>),
    RustError(String, String, std::rc::Rc<dyn std::error::Error>),
    Others
}
