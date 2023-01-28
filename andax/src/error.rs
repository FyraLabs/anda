use ariadne::{ColorGenerator, Report, ReportBuilder, ReportKind};
use rhai::{EvalAltResult, Position};
use smartstring::{LazyCompact, SmartString};
use std::ops::Range;
use std::rc::Rc;
use std::{fmt::Display, path::Path};
use tracing::{debug, error, instrument, trace, warn};

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

impl Default for TbErr {
    fn default() -> Self {
        Self::Report(Rc::new(color_eyre::Report::msg("Default val leak (bug!)")))
    }
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

#[derive(Default)]
pub struct ErrHdlr {
    name: SStr,
    scr: Option<Box<Path>>,
    tbe: TbErr,
    pos: Position,
    rfn: SStr,
    fsrc: SStr,
    cg: ColorGenerator,
    rp: Option<ReportBuilder<(&'static str, Range<usize>)>>,
    f: String,
}

impl ErrHdlr {
    #[instrument]
    pub fn new(name: &str, scr: &Path, err: EvalAltResult) -> Option<Self> {
        trace!("{name}: Generating traceback");
        if let EvalAltResult::ErrorRuntime(ref run_err, pos) = err {
            match run_err.clone().try_cast::<AndaxError>() {
                Some(AndaxError::RustReport(rhai_fn, fn_src, oerr)) => {
                    return Some(Self {
                        name: name.into(),
                        scr: Some(scr.into()),
                        tbe: TbErr::Report(oerr),
                        pos,
                        rfn: rhai_fn.into(),
                        fsrc: fn_src.into(),
                        ..Self::default()
                    })
                }
                Some(AndaxError::RustError(rhai_fn, fn_src, oerr)) => {
                    return Some(Self {
                        name: name.into(),
                        scr: Some(scr.into()),
                        tbe: TbErr::Arb(oerr),
                        pos,
                        rfn: rhai_fn.into(),
                        fsrc: fn_src.into(),
                        ..Self::default()
                    })
                }
                Some(AndaxError::Exit(b)) => {
                    if b {
                        warn!("世界を壊している。\n{}", crate::error::EARTH);
                        error!("生存係為咗喵？打程式幾好呀。仲喵要咁憤世嫉俗喎。還掂おこちゃま戦争係政治家嘅事……");
                        trace!("あなたは世界の終わりにずんだを食べるのだ");
                    }
                    debug!("Exit from rhai at: {pos}");
                    return None;
                }
                None => return None,
            }
        }
        trace!("Rhai moment: {err:#?}");
        let pos = err.position();
        Some(Self {
            name: name.into(),
            scr: Some(scr.into()),
            tbe: TbErr::Rhai(err),
            pos,
            rfn: "".into(),
            fsrc: "".into(),
            ..Self::default()
        })
    }
    fn gen_rp(&mut self, offset: usize) {
        let scr = self.scr.clone().map_or("??".to_string(), |a| a.to_string_lossy().to_string());
        let scr: &'static str = Box::leak(Box::new(scr));
        self.rp = Some(std::mem::take(&mut self.rp).unwrap_or(Report::build(
            ReportKind::Error,
            scr,
            offset,
        )));
    }
    #[inline]
    fn _gpos(&self) -> Option<(usize, usize)> {
        self.pos.line().map(|l| (l, self.pos.position().unwrap_or(0)))
    }

    /// # Why does this exist?
    /// Apparently, ariadne does not take an offset by a position, but instead
    /// by no. of characters to that position (`usize`). That means we have to
    /// manually count the no. of chars to that pos by reading the file to str
    /// then store it for later use.
    /// # Notes
    /// Before using this function, check if `self.scr` is `Some`
    fn get_offset(&mut self) -> std::io::Result<usize> {
        let (a, b) = self._gpos().expect("No position data in traceback.");
        self.f = std::fs::read_to_string(self.scr.as_ref().unwrap())?;
        let mut passed_lines = 0;
        let mut chn = 0;
        for (n, ch) in self.f.chars().enumerate() {
            if passed_lines == a - 1 {
                chn += 1;
                if chn == b {
                    return Ok(n);
                }
            }
            if ch == '\n' { // ignore \r for now
                passed_lines += 1;
            }
        }
        Ok(0)
    }
    pub fn print(&mut self) {}
}
