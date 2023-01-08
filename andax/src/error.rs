#[derive(Clone)]
pub enum AndaxError {
    // rhai_fn, fn_src, E
    RustError(String, String, std::rc::Rc<color_eyre::Report>),
    Others
}
