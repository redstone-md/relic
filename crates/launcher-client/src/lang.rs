//! Every word the launcher says, in each language it speaks.
//!
//! The words live in `locales/<code>.ftl`, one [Fluent] file per language,
//! all built into the launcher. A new language is a new file and nothing
//! else. [`Msg`] names every message the code asks for; `en.ftl` has them
//! all and stands in for whatever another file leaves out.
//!
//! The language in use is one process-wide setting, since there is one
//! window and one player.
//!
//! [Fluent]: https://projectfluent.org

use std::borrow::Cow;
use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, OnceLock};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use include_dir::{Dir, include_dir};
use unic_langid::LanguageIdentifier;

static FILES: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/locales");

/// The language that has every message.
const FALLBACK: &str = "en";

/// The message in which a file says what its language calls itself.
const NAME: &str = "language-name";

type Bundle = FluentBundle<FluentResource>;

/// One file of `locales`.
struct Locale {
    code: &'static str,
    id: LanguageIdentifier,
    source: &'static str,
    /// The file, parsed the first time the language is needed.
    bundle: OnceLock<Bundle>,
    name: OnceLock<String>,
    /// Messages without blanks, kept once said so they can be lent out for
    /// as long as the launcher runs.
    said: Box<[OnceLock<String>]>,
}

impl Locale {
    fn bundle(&self) -> &Bundle {
        self.bundle.get_or_init(|| {
            // An entry that does not parse is left out and English stands in
            // for it; the tests keep the files that ship clean.
            let resource = FluentResource::try_new(self.source.to_owned())
                .unwrap_or_else(|(resource, _)| resource);
            let mut bundle = FluentBundle::new_concurrent(vec![self.id.clone()]);
            // The marks Fluent puts around blanks for mixed left-to-right
            // and right-to-left text show up as boxes in the pixel font.
            bundle.set_use_isolating(false);
            let _ = bundle.add_resource(resource);
            bundle
        })
    }

    /// What the file says under `key`, if it says it without mistakes.
    fn say(&self, key: &str, args: Option<&FluentArgs>) -> Option<Cow<'_, str>> {
        let bundle = self.bundle();
        let pattern = bundle.get_message(key)?.value()?;
        let mut mistakes = Vec::new();
        let text = bundle.format_pattern(pattern, args, &mut mistakes);
        mistakes.is_empty().then_some(text)
    }
}

struct Locales {
    /// Sorted by code.
    all: Vec<Locale>,
    fallback: usize,
}

static LOCALES: LazyLock<Locales> = LazyLock::new(|| {
    let mut all: Vec<Locale> = FILES
        .files()
        .filter(|file| file.path().extension().is_some_and(|ext| ext == "ftl"))
        .filter_map(|file| {
            let code = file.path().file_stem()?.to_str()?;
            Some(Locale {
                code,
                id: code.parse().ok()?,
                source: file.contents_utf8()?,
                bundle: OnceLock::new(),
                name: OnceLock::new(),
                said: Msg::ALL.iter().map(|_| OnceLock::new()).collect(),
            })
        })
        .collect();
    all.sort_by_key(|locale| locale.code);
    let fallback = all
        .iter()
        .position(|locale| locale.code == FALLBACK)
        .expect("locales/en.ftl is built in");
    Locales { all, fallback }
});

/// A language the launcher speaks: one of the files of `locales`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lang(usize);

impl Lang {
    /// Every language, in the order of their codes.
    pub fn all() -> impl Iterator<Item = Lang> {
        (0..LOCALES.all.len()).map(Lang)
    }

    pub fn english() -> Lang {
        Lang(LOCALES.fallback)
    }

    fn locale(self) -> &'static Locale {
        &LOCALES.all[self.0]
    }

    /// The name of the language's file, which is also how the settings
    /// file writes it down: `ru`, `pt-BR`.
    pub fn code(self) -> &'static str {
        self.locale().code
    }

    /// What the language calls itself.
    pub fn name(self) -> &'static str {
        let locale = self.locale();
        locale.name.get_or_init(|| match locale.say(NAME, None) {
            Some(name) => name.into_owned(),
            None => locale.code.to_owned(),
        })
    }

    /// Reads a locale such as `ru`, `ru-RU` or `ru_RU.UTF-8` and finds the
    /// closest language the launcher speaks: the same one to the letter,
    /// else the same language of the same region or script, else the same
    /// language at all.
    pub fn from_tag(tag: &str) -> Option<Lang> {
        let tag = tag.split(['.', '@']).next()?.replace('_', "-");
        let wanted: LanguageIdentifier = tag.parse().ok()?;
        let speaks = |close_enough: &dyn Fn(&LanguageIdentifier) -> bool| {
            Lang::all().find(|lang| {
                let id = &lang.locale().id;
                id.language == wanted.language && close_enough(id)
            })
        };
        speaks(&|id| *id == wanted)
            .or_else(|| speaks(&|id| id.region.is_some() && id.region == wanted.region))
            .or_else(|| speaks(&|id| id.script.is_some() && id.script == wanted.script))
            .or_else(|| speaks(&|id| id.region.is_none() && id.script.is_none()))
            .or_else(|| speaks(&|_| true))
    }

    /// The first of the languages the system prefers that the launcher
    /// speaks. English when it speaks none of them.
    pub fn system() -> Lang {
        pick(sys_locale::get_locales())
    }
}

fn pick(preferred: impl Iterator<Item = String>) -> Lang {
    preferred
        .filter_map(|tag| Lang::from_tag(&tag))
        .next()
        .unwrap_or_else(Lang::english)
}

/// English until something else is set.
const UNSET: usize = usize::MAX;
static CURRENT: AtomicUsize = AtomicUsize::new(UNSET);

/// The language everything is said in from now on.
pub fn set(lang: Lang) {
    CURRENT.store(lang.0, Ordering::Relaxed);
}

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        UNSET => Lang::english(),
        ix => Lang(ix),
    }
}

macro_rules! messages {
    ($($name:ident => $key:literal $(($($blank:ident),+))?;)*) => {
        /// A message, by what it is for. Its words are in the files.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Msg {
            $($name,)*
        }

        impl Msg {
            const ALL: &'static [Msg] = &[$(Msg::$name,)*];

            /// What the files call the message.
            const fn key(self) -> &'static str {
                match self {
                    $(Msg::$name => $key,)*
                }
            }

            /// The blanks the message has, in the order [`tf`] fills them.
            const fn blanks(self) -> &'static [&'static str] {
                match self {
                    $(Msg::$name => &[$($(stringify!($blank),)+)?],)*
                }
            }
        }
    };
}

messages! {
    Language => "language";
    LanguageAuto => "language-auto";
    LanguageFollowsSystem => "language-follows-system" (name);

    ConnectHint => "connect-hint";
    ServerAddress => "server-address";
    Connect => "connect";
    RegisterApprovalHint => "register-approval-hint";
    RegisterInviteHint => "register-invite-hint";
    ResetHint => "reset-hint";
    SignInHint => "sign-in-hint";
    Username => "username";
    Password => "password";
    NewPassword => "new-password";
    CurrentPassword => "current-password";
    ResetCode => "reset-code";
    CodeFromAdmin => "code-from-admin";
    InviteCode => "invite-code";
    SignIn => "sign-in";
    CreateAccount => "create-account";
    ChangePassword => "change-password";
    ResetPassword => "reset-password";
    OtherServer => "other-server";
    GameFolder => "game-folder";
    GameFolderHint => "game-folder-hint";
    OpenFolder => "open-folder";
    ChangeFolder => "change-folder";
    DefaultFolder => "default-folder";
    PutGameHere => "put-game-here";
    EnterCredentials => "enter-credentials";
    NotApproved => "not-approved";
    PickCredentials => "pick-credentials";
    NeedsInvite => "needs-invite";
    AccountCreated => "account-created";
    EnterReset => "enter-reset";
    PasswordChangedSignIn => "password-changed-sign-in";
    SessionEnded => "session-ended";

    Online => "online";
    Starting => "starting";
    Installing => "installing";
    Stopping => "stopping";
    Offline => "offline";
    Down => "down";
    GameWithLoader => "game-with-loader" (version, loader);
    OnlinePlaying => "online-playing" (online, max);
    NothingToPlay => "nothing-to-play";
    Servers => "servers";
    Server => "server";
    PlayingNow => "playing-now";
    Mods => "mods" (count);
    News => "news";
    NewsDated => "news-dated" (date);
    NoNews => "no-news";
    ChangeSkin => "change-skin";
    Play => "play";
    Playing => "playing";
    TryAgain => "try-again";
    RestartToUpdate => "restart-to-update" (version);

    Skin => "skin";
    Arms => "arms";
    ArmsClassic => "arms-classic";
    ArmsSlim => "arms-slim";
    ArmsHint => "arms-hint";
    UploadSkin => "upload-skin";
    Remove => "remove";
    File => "file";
    SkinFileHint => "skin-file-hint";
    TurnHint => "turn-hint";
    SkinUpdated => "skin-updated";
    SkinRemoved => "skin-removed";
    CannotReadFile => "cannot-read-file" (reason);
    SkinTooBig => "skin-too-big";

    Settings => "settings";
    Memory => "memory";
    MemoryHint => "memory-hint";
    Gigabytes => "gigabytes" (amount);
    Megabytes => "megabytes" (amount);
    BlockIsGigabyte => "block-is-gigabyte";
    OptionalMods => "optional-mods";
    NoMods => "no-mods";
    ModsOn => "mods-on" (all);
    ModsRule => "mods-rule";
    AlsoInstalls => "also-installs" (mods);
    NeededBy => "needed-by" (mods);
    NeededByPack => "needed-by-pack";
    PasswordHint => "password-hint";
    PasswordChanged => "password-changed";
    EnterPasswords => "enter-passwords";
    WrongCurrentPassword => "wrong-current-password";
    Account => "account";
    OnServer => "on-server" (address);
    SignOut => "sign-out";

    AddressEmpty => "address-empty";
    AddressInvalid => "address-invalid";
    AddressInsecure => "address-insecure";
    NetworkFailed => "network-failed" (reason);
    ProtocolFailed => "protocol-failed" (reason);
    LauncherOutdated => "launcher-outdated";
    TimedOut => "timed-out";
    NothingAnswered => "nothing-answered";

    CheckingUpdates => "checking-updates";
    CheckingMods => "checking-mods";
    DownloadingMods => "downloading-mods" (size);
    SigningInToGame => "signing-in-to-game";
    StartingGame => "starting-game";
    InstallingLoader => "installing-loader";
    CheckingGameFiles => "checking-game-files";
    DownloadingJava => "downloading-java";
    StartFailed => "start-failed" (reason);
    InstanceNameUnusable => "instance-name-unusable";
    StoppedWithCode => "stopped-with-code" (code, log);
    StoppedUnexpectedly => "stopped-unexpectedly" (log);
    PathUnusable => "path-unusable" (path);
    DownloadFailed => "download-failed" (path, reason);
    ServerAnswered => "server-answered" (status);
    Corrupt => "corrupt" (path);
    FilesFailed => "files-failed" (reason);

    Date => "date" (day, month, year);
    Month1 => "month-1";
    Month2 => "month-2";
    Month3 => "month-3";
    Month4 => "month-4";
    Month5 => "month-5";
    Month6 => "month-6";
    Month7 => "month-7";
    Month8 => "month-8";
    Month9 => "month-9";
    Month10 => "month-10";
    Month11 => "month-11";
    Month12 => "month-12";
}

/// A message in `lang`, in English if `lang` does not have it, and failing
/// even that its name in the files.
fn say(lang: Lang, msg: Msg, args: Option<&FluentArgs>) -> Cow<'static, str> {
    let key = msg.key();
    lang.locale()
        .say(key, args)
        .or_else(|| Lang::english().locale().say(key, args))
        .unwrap_or(Cow::Borrowed(key))
}

/// A message in the language in use.
pub fn t(msg: Msg) -> &'static str {
    let lang = current();
    lang.locale().said[msg as usize].get_or_init(|| say(lang, msg, None).into_owned())
}

fn filled(lang: Lang, msg: Msg, with: &[&dyn Display]) -> String {
    let blanks = msg.blanks();
    debug_assert_eq!(blanks.len(), with.len(), "{msg:?} has other blanks");
    let mut args = FluentArgs::new();
    for (blank, value) in blanks.iter().zip(with) {
        args.set(*blank, value.to_string());
    }
    say(lang, msg, Some(&args)).into_owned()
}

/// A message with its blanks filled in, in the order the table of
/// messages lists them.
pub fn tf(msg: Msg, with: &[&dyn Display]) -> String {
    filled(current(), msg, with)
}

/// A message about `count` things, which each language words by its own
/// rules: 1 мод, 2 мода, 5 модов, 21 мод.
fn counted(lang: Lang, msg: Msg, count: usize) -> String {
    let mut args = FluentArgs::new();
    args.set("count", count);
    say(lang, msg, Some(&args)).into_owned()
}

/// `3 mods`.
pub fn mods(n: usize) -> String {
    counted(current(), Msg::Mods, n)
}

const MONTHS: [Msg; 12] = [
    Msg::Month1,
    Msg::Month2,
    Msg::Month3,
    Msg::Month4,
    Msg::Month5,
    Msg::Month6,
    Msg::Month7,
    Msg::Month8,
    Msg::Month9,
    Msg::Month10,
    Msg::Month11,
    Msg::Month12,
];

/// A Unix timestamp as a calendar date (UTC), e.g. `12 Nov 2026`.
pub fn date(unix: i64) -> String {
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = unix.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let number = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(number <= 2);
    let month = t(MONTHS[(number as usize - 1).min(11)]);
    tf(Msg::Date, &[&day, &month, &year])
}

#[cfg(test)]
mod tests {
    use fluent_syntax::ast::Entry;

    use super::*;

    fn lang(code: &str) -> Lang {
        Lang::all()
            .find(|lang| lang.code() == code)
            .unwrap_or_else(|| panic!("no locales/{code}.ftl"))
    }

    /// The messages a file has, by what it calls them.
    fn keys(source: &str) -> Vec<String> {
        let resource = FluentResource::try_new(source.to_owned())
            .unwrap_or_else(|(_, mistakes)| panic!("does not parse: {mistakes:?}"));
        resource
            .entries()
            .filter_map(|entry| match entry {
                Entry::Message(message) => Some(message.id.name.to_owned()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_file_is_a_language() {
        let files = FILES.files().count();
        assert_eq!(Lang::all().count(), files, "a file is not <code>.ftl");
        for lang in Lang::all() {
            let locale = lang.locale();
            let said = keys(locale.source);
            assert!(
                said.iter().any(|key| key == NAME),
                "{}.ftl does not name its language",
                locale.code
            );
            for key in &said {
                assert!(
                    key == NAME || Msg::ALL.iter().any(|msg| msg.key() == key),
                    "{}.ftl has {key}, which the launcher never says",
                    locale.code
                );
            }
        }
    }

    #[test]
    fn english_says_everything() {
        let said = keys(Lang::english().locale().source);
        for msg in Msg::ALL {
            assert!(said.iter().any(|key| key == msg.key()), "{msg:?}");
        }
    }

    /// A blank a file misspells would otherwise only show once the message
    /// came up on screen.
    #[test]
    fn every_message_fills_its_blanks() {
        for lang in Lang::all() {
            let locale = lang.locale();
            for &msg in Msg::ALL {
                if locale.bundle().get_message(msg.key()).is_none() {
                    continue;
                }
                let counts: &[usize] = if msg == Msg::Mods {
                    &[0, 1, 2, 5, 21]
                } else {
                    &[0]
                };
                for &count in counts {
                    let mut args = FluentArgs::new();
                    for blank in msg.blanks() {
                        args.set(*blank, count);
                    }
                    let text = locale.say(msg.key(), Some(&args));
                    assert!(
                        text.is_some_and(|text| !text.trim().is_empty()),
                        "{}.ftl: {} does not come out",
                        locale.code,
                        msg.key()
                    );
                }
            }
        }
    }

    #[test]
    fn a_missing_message_is_said_in_english() {
        let russian = lang("ru");
        assert_eq!(say(russian, Msg::Play, None), "ИГРАТЬ");
        // Nothing fills the blank, so Russian fails and so does English.
        assert_eq!(say(russian, Msg::OnServer, None), "on-server");
    }

    #[test]
    fn locales_name_their_language() {
        let (en, ru) = (lang("en"), lang("ru"));
        assert_eq!(Lang::from_tag("ru"), Some(ru));
        assert_eq!(Lang::from_tag("ru-RU"), Some(ru));
        assert_eq!(Lang::from_tag("ru_RU.UTF-8"), Some(ru));
        assert_eq!(Lang::from_tag("EN-us"), Some(en));
        assert_eq!(Lang::from_tag("tlh-AQ"), None);
        assert_eq!(Lang::from_tag(""), None);
        // Not Russian, whatever it starts with.
        assert_eq!(Lang::from_tag("rue"), None);
        assert_eq!(en.name(), "English");
        assert_eq!(ru.name(), "Русский");
    }

    #[test]
    fn the_first_language_we_speak_wins() {
        let prefers = |tags: &[&str]| pick(tags.iter().map(|tag| (*tag).to_owned()));
        assert_eq!(prefers(&["tlh-AQ", "ru-RU", "en-US"]), lang("ru"));
        assert_eq!(prefers(&["en-GB", "ru-RU"]), lang("en"));
        assert_eq!(prefers(&["tlh"]), Lang::english());
        assert_eq!(prefers(&[]), Lang::english());
    }

    #[test]
    fn codes_read_back() {
        for lang in Lang::all() {
            assert_eq!(Lang::from_tag(lang.code()), Some(lang));
        }
    }

    #[test]
    fn blanks_are_filled_in_order() {
        let en = Lang::english();
        assert_eq!(
            filled(en, Msg::GameWithLoader, &[&"1.21.1", &"Fabric"]),
            "Minecraft 1.21.1 with Fabric"
        );
        assert_eq!(
            filled(lang("ru"), Msg::OnlinePlaying, &[&3, &20]),
            "Онлайн, играют 3 из 20"
        );
    }

    #[test]
    fn each_language_counts_its_own_way() {
        let ru = |n| counted(lang("ru"), Msg::Mods, n);
        assert_eq!(ru(1), "1 мод");
        assert_eq!(ru(21), "21 мод");
        assert_eq!(ru(3), "3 мода");
        assert_eq!(ru(104), "104 мода");
        assert_eq!(ru(0), "0 модов");
        assert_eq!(ru(11), "11 модов");
        assert_eq!(ru(112), "112 модов");
        let en = |n| counted(Lang::english(), Msg::Mods, n);
        assert_eq!(en(1), "1 mod");
        assert_eq!(en(2), "2 mods");
    }

    // Nothing else in the tests changes the language, so it is English here.
    #[test]
    fn timestamps_become_dates() {
        assert_eq!(date(0), "1 Jan 1970");
        assert_eq!(date(951_782_400), "29 Feb 2000");
        assert_eq!(date(1_794_441_600), "12 Nov 2026");
        assert_eq!(date(-1), "31 Dec 1969");
        assert_eq!(say(lang("ru"), Msg::Month11, None), "ноя");
    }
}
