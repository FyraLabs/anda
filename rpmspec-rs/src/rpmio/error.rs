/// This exists beacuse yes

#[derive(Debug)]
pub enum MacroErr {
    MacroDepthExceeded,
}

impl std::fmt::Display for MacroErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MacroDepthExceeded => {
                write!(f, "Too many levels of recursion in macro expansion. It is likely caused by recursive macro declaration.");
            }
        }
        Ok(())
    }
}

impl std::error::Error for MacroErr {}
