use rhai::EvalAltResult;
use smartstring::{LazyCompact, SmartString};
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

pub(crate) trait AndaxRes<T> {
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

pub(crate) const EARTH: &str = r#"
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
