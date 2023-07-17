use rhai::EvalAltResult;
use smartstring::{LazyCompact, SmartString};
use std::fmt::Display;
use std::rc::Rc;
use tracing::instrument;
use tracing::trace;

type SStr = SmartString<LazyCompact>;

#[derive(Clone, Debug)]
pub enum AndaxError {
    // rhai_fn, fn_src, E
    RustReport(SStr, SStr, Rc<color_eyre::Report>),
    RustError(SStr, SStr, Rc<dyn std::error::Error>),
    Exit(bool),
}

#[derive(Debug)]
pub enum TbErr {
    Report(Rc<color_eyre::Report>),
    Arb(Rc<dyn std::error::Error + 'static>),
    Rhai(EvalAltResult),
}

impl Display for TbErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Report(o) => f.write_fmt(format_args!("From: {o:#}")),
            Self::Arb(o) => f.write_fmt(format_args!("Caused by: {o}")),
            Self::Rhai(o) => f.write_fmt(format_args!("Rhai: {o}")),
        }
    }
}

pub trait AndaxRes<T> {
    fn ehdl(self, ctx: &rhai::NativeCallContext) -> Result<T, Box<EvalAltResult>>;
}

impl<T, E> AndaxRes<T> for Result<T, E>
where
    E: std::error::Error + 'static,
{
    #[instrument(skip(self, ctx))]
    fn ehdl(self, ctx: &rhai::NativeCallContext<'_>) -> Result<T, Box<rhai::EvalAltResult>>
    where
        Self: Sized,
    {
        self.map_err(|err| {
            trace!(func = ctx.fn_name(), source = ctx.source(), "Oops!");
            Box::new(EvalAltResult::ErrorRuntime(
                rhai::Dynamic::from(AndaxError::RustError(
                    ctx.fn_name().into(),
                    ctx.source().unwrap_or("").into(),
                    std::rc::Rc::from(err),
                )),
                ctx.position(),
            ))
        })
    }
}

pub const EARTH: &str = r#"
.    .    *  .   .  .   .  *     .  .        . .   .     .  *   .     .  .   .
   *  .    .    *  .     .         .    * .     .  *  .    .   .   *   . .    .
. *      .   .    .  .     .  *      .      .        .     .-o--.   .    *  .
 .  .        .     .     .      .    .     *      *   .   :O o O :      .     .
____   *   .    .      .   .           .  .   .      .    : O. Oo;    .   .
 `. ````.---...___      .      *    .      .       .   * . `-.O-'  .     * . .
   \_    ;   \`.-'```--..__.       .    .      * .     .       .     .        .
   ,'_,-' _,-'             ``--._    .   *   .   .  .       .   *   .     .  .
   -'  ,-'                       `-._ *     .       .   *  .           .    .
    ,-'            _,-._            ,`-. .    .   .     .      .     *    .   .
    '--.     _ _.._`-.  `-._        |   `_   .      *  .    .   .     .  .    .
        ;  ,' ' _  `._`._   `.      `,-''  `-.     .    .     .    .      .  .
     ,-'   \    `;.   `. ;`   `._  _/\___     `.       .    *     .    . *
     \      \ ,  `-'    )        `':_  ; \      `. . *     .        .    .    *
      \    _; `       ,;               __;        `. .           .   .     . .
       '-.;        __,  `   _,-'-.--'''  \-:        `.   *   .    .  .   *   .
          )`-..---'   `---''              \ `.        . .   .  .       . .  .
        .'                                 `. `.       `  .    *   .      .  .
       /                                     `. `.      ` *          .       .
      /                                        `. `.     '      .   .     *
     /                                           `. `.   _'.  .       .  .    .
    |                                              `._\-'  '     .        .  .
    |                                                 `.__, \  *     .   . *. .
    |                                                      \ \.    .         .
    |                                                       \ \ .     * jrei  *"#;
