//! The launcher's one view: what it knows, and what the player can do.
//! Drawing lives next door, one file per screen.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, StyledExt, Theme};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Entity, Image, ImageFormat, ObjectFit, PathPromptOptions, SharedString,
    Subscription, Window, div, img,
};
use image::RgbaImage;
use relic_proto::{
    Handshake, InstanceInfo, InstanceStatus, Manifest, NewsItem, Palette, RegistrationPolicy, Scene,
};

use super::pixels::{self, Stage, Subject};
use super::theme;
use super::widgets::{FRAME, canvas};
use crate::backend::api::{self, Account, Client, Texture};
use crate::backend::game::{self, Launch};
use crate::backend::store::{Config, MAX_MEMORY_MB, MIN_MEMORY_MB, Server};
use crate::backend::{self, address, assets, secrets, store, update};
use crate::lang::{self, Lang, Msg, t, tf};

/// A skin is a 64×64 PNG; anything near this size is not one.
const MAX_TEXTURE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Screen {
    Connect,
    Login,
    Register,
    Reset,
    Home,
    News,
    Skin,
    Settings,
    Language,
    Mods,
}

/// How often the launcher asks the server what has changed: modpacks,
/// their state, news, the theme.
const REFRESH_EVERY: Duration = Duration::from_secs(30);

/// How many refreshes pass between looks for a newer launcher.
const UPDATE_EVERY: u32 = 60;

/// The face everything is set in unless a server brings its own.
const BODY: &str = "JetBrains Mono";

/// The least and the most memory the game may be given.
pub(super) const MEMORY_RANGE_MB: (u32, u32) = (MIN_MEMORY_MB, MAX_MEMORY_MB);

/// How far one press of a memory button moves the setting.
const MEMORY_STEP_MB: u32 = 512;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum PlayState {
    Idle,
    Working {
        label: SharedString,
        /// 0 to 1, when it can be told.
        progress: Option<f32>,
    },
    Running,
    Failed(SharedString),
}

/// Everything learned by connecting to a server.
struct Opened {
    client: Client,
    handshake: Handshake,
    news: Vec<NewsItem>,
    account: Option<Account>,
}

async fn open(origin: String) -> api::Result<Opened> {
    let mut client = Client::new(origin.clone());
    let handshake = client.handshake().await?;
    let news = client.news().await.unwrap_or_default();

    let saved = {
        let origin = origin.clone();
        backend::spawn_blocking(move || secrets::load(&origin))
            .await
            .ok()
            .flatten()
    };
    let mut account = None;
    if let Some(token) = saved {
        let signed = client.clone().with_token(Some(token));
        match signed.me().await {
            Ok(me) if me.status == "active" => {
                account = Some(me);
                client = signed;
            }
            Ok(_) => {}
            Err(err) if err.is_unauthorized() => {
                drop(backend::spawn_blocking(move || secrets::forget(&origin)));
            }
            Err(err) => return Err(err),
        }
    }
    Ok(Opened {
        client,
        handshake,
        news,
        account,
    })
}

/// Server-supplied links may be relative to the server itself.
fn absolute(origin: &str, url: &str) -> String {
    if url.starts_with('/') {
        format!("{origin}{url}")
    } else {
        url.to_owned()
    }
}

/// What a mod's picture goes by among the launcher's pictures.
pub(super) fn icon_key(sha256: &str) -> String {
    format!("icon:{sha256}")
}

/// A mod's picture: from disk when it was fetched before, else from the
/// server. It is named by its hash, so what arrives is checked against it.
async fn mod_icon(
    client: &backend::api::Client,
    dir: &std::path::Path,
    instance: &str,
    sha256: &str,
) -> Option<Vec<u8>> {
    use sha2::{Digest, Sha256};
    let path = dir.join(sha256);
    if let Ok(bytes) = tokio::fs::read(&path).await {
        return Some(bytes);
    }
    let bytes = client.icon(instance, sha256).await?;
    if !hex::encode(Sha256::digest(&bytes)).eq_ignore_ascii_case(sha256) {
        return None;
    }
    if tokio::fs::create_dir_all(dir).await.is_ok() {
        drop(tokio::fs::write(&path, &bytes).await);
    }
    Some(bytes)
}

/// Wraps downloaded bytes as an image, if they look like one.
fn decode(bytes: Vec<u8>) -> Option<Arc<Image>> {
    let format = match bytes.as_slice() {
        [0x89, b'P', b'N', b'G', ..] => ImageFormat::Png,
        [0xff, 0xd8, 0xff, ..] => ImageFormat::Jpeg,
        [b'G', b'I', b'F', b'8', ..] => ImageFormat::Gif,
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => ImageFormat::Webp,
        [b'B', b'M', ..] => ImageFormat::Bmp,
        _ => return None,
    };
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

pub struct Launcher {
    dir: PathBuf,
    pub(super) config: Config,
    pub(super) screen: Screen,
    /// A request the current form waits on is in flight.
    pub(super) busy: bool,
    pub(super) error: Option<SharedString>,
    pub(super) notice: Option<SharedString>,
    /// Bumped whenever the server or the account changes, so answers to
    /// questions asked before that are dropped.
    generation: u64,
    default_font: SharedString,

    pub(super) client: Option<Client>,
    pub(super) handshake: Option<Handshake>,
    pub(super) news: Vec<NewsItem>,
    pub(super) account: Option<Account>,
    pub(super) manifest: Option<Manifest>,
    pub(super) logo: Option<Arc<Image>>,
    pub(super) background: Option<Arc<Image>>,
    /// Pictures the server points at, by address: modpack banners and news
    /// images. `None` while one is on its way, or if it never came.
    pictures: HashMap<String, Option<Arc<Image>>>,
    pub(super) head: Option<Arc<Image>>,
    /// The whole skin, for the figure on the skin screen.
    skin: Option<Arc<RgbaImage>>,
    pub(super) slim: bool,
    /// The news item being read, if the player picked one.
    pub(super) news_open: Option<i64>,
    /// The shaft behind the chosen server.
    mine: Entity<Stage>,
    /// The player's figure on the skin screen.
    pub(super) doll: Entity<Stage>,
    pub(super) play: PlayState,
    /// The version a finished self-update put in place; it runs after a
    /// restart.
    pub(super) updated: Option<SharedString>,

    pub(super) address: Entity<InputState>,
    pub(super) username: Entity<InputState>,
    pub(super) password: Entity<InputState>,
    pub(super) invite: Entity<InputState>,
    pub(super) code: Entity<InputState>,
    pub(super) current_password: Entity<InputState>,
    pub(super) new_password: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

/// The language the settings name, if they name one the launcher speaks.
fn chosen_language(config: &Config) -> Option<Lang> {
    config.language.as_deref().and_then(Lang::from_tag)
}

impl Launcher {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Relic's own colors until a server brings its own.
        theme::apply(&Palette::default(), Some(BODY.into()), cx);
        let dir = store::data_dir();
        let config = Config::load(&dir);
        lang::set(chosen_language(&config).unwrap_or_else(Lang::system));
        let saved = config.server.clone();

        let text = |window: &mut Window, cx: &mut Context<Self>, placeholder: &'static str| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let secret = |window: &mut Window, cx: &mut Context<Self>, placeholder: &'static str| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(true)
            })
        };
        let address = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("play.example.com")
                .default_value(
                    saved
                        .as_ref()
                        .map(|s| s.address.clone())
                        .unwrap_or_default(),
                )
        });
        let username = text(window, cx, "Steve");
        let password = secret(window, cx, t(Msg::Password));
        let invite = text(window, cx, t(Msg::InviteCode));
        let code = text(window, cx, t(Msg::CodeFromAdmin));
        let current_password = secret(window, cx, t(Msg::CurrentPassword));
        let new_password = secret(window, cx, t(Msg::NewPassword));
        let mine = cx.new(|_| Stage::new(Subject::Shaft));
        let doll = cx.new(|_| Stage::new(Subject::Figure));

        let mut subscriptions = Vec::new();
        for input in [
            &address,
            &username,
            &password,
            &invite,
            &code,
            &current_password,
            &new_password,
        ] {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.submit(window, cx);
                    }
                },
            ));
        }

        let mut this = Self {
            dir,
            config,
            screen: Screen::Connect,
            busy: false,
            error: None,
            notice: None,
            generation: 0,
            default_font: Theme::global(cx).font_family.clone(),
            client: None,
            handshake: None,
            news: Vec::new(),
            account: None,
            manifest: None,
            logo: None,
            background: None,
            pictures: HashMap::new(),
            head: None,
            skin: None,
            slim: false,
            news_open: None,
            mine,
            doll,
            play: PlayState::Idle,
            updated: None,
            address,
            username,
            password,
            invite,
            code,
            current_password,
            new_password,
            _subscriptions: subscriptions,
        };
        if let Some(server) = saved {
            this.connect_to(server.address, server.origin, false, window, cx);
        }
        this.self_update(window, cx);
        this.keep_fresh(window, cx);
        this
    }

    /// Asks the server again every so often, for as long as the launcher
    /// is open, and looks for a newer launcher now and then.
    fn keep_fresh(&mut self, window: &Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let mut round = 0u32;
            loop {
                cx.background_executor().timer(REFRESH_EVERY).await;
                round = round.wrapping_add(1);
                let alive = this.update_in(cx, |this, window, cx| {
                    this.refresh(window, cx);
                    if round.is_multiple_of(UPDATE_EVERY) && this.updated.is_none() {
                        this.self_update(window, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Takes what the server says now in place of what it said before.
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.run(
            window,
            cx,
            async move {
                let handshake = client.handshake().await.ok();
                let news = client.news().await.ok();
                (handshake, news)
            },
            |this, (handshake, news), window, cx| {
                if let Some(news) = news {
                    this.news = news;
                }
                if let Some(handshake) = handshake
                    && this.handshake.as_ref() != Some(&handshake)
                {
                    let build = this.selected().map(|i| (i.id.clone(), i.build_id));
                    let restyled =
                        this.handshake.as_ref().map(|h| &h.theme) != Some(&handshake.theme);
                    if restyled {
                        theme::apply(&handshake.theme.palette, None, cx);
                    }
                    this.handshake = Some(handshake);
                    if restyled {
                        this.load_branding(window, cx);
                    }
                    // A new build of the chosen pack: the list of optional
                    // mods may have changed with it.
                    if this.account.is_some()
                        && this.selected().map(|i| (i.id.clone(), i.build_id)) != build
                    {
                        this.reload_manifest(window, cx);
                    }
                }
                this.load_pictures(window, cx);
            },
        );
    }

    /// A picture the server pointed at, once it has arrived.
    pub(super) fn picture(&self, url: &str) -> Option<Arc<Image>> {
        self.pictures.get(url).cloned().flatten()
    }

    /// Fetches every banner and news image not asked for yet.
    fn load_pictures(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = &self.client else {
            return;
        };
        let http = client.http().clone();
        let origin = client.origin().to_owned();
        let cache = self.cache_dir();
        let banners = self
            .handshake
            .iter()
            .flat_map(|h| &h.instances)
            .filter_map(|i| i.banner_url.clone());
        let wanted: Vec<String> = banners
            .chain(self.news.iter().filter_map(|n| n.image_url.clone()))
            .filter(|url| !self.pictures.contains_key(url))
            .collect();
        for url in wanted {
            self.pictures.insert(url.clone(), None);
            let address = absolute(&origin, &url);
            // The server chooses these; only ever fetch a web address.
            if !address.starts_with("https://") && !address.starts_with("http://") {
                continue;
            }
            let (http, cache) = (http.clone(), cache.clone());
            self.run(
                window,
                cx,
                async move { assets::fetch(&http, &cache, &address).await },
                move |this, bytes, _, _| {
                    this.pictures.insert(url, bytes.and_then(decode));
                },
            );
        }
    }

    /// Fetches the pictures of the pack's optional mods not asked for yet.
    fn load_icons(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(client), Some(instance)) = (self.client.clone(), self.selected()) else {
            return;
        };
        let instance = instance.id.clone();
        let dir = self.cache_dir().join("icons");
        let wanted: BTreeSet<String> = self
            .manifest
            .iter()
            .flat_map(|manifest| &manifest.files)
            .filter(|file| file.optional)
            .filter_map(|file| file.icon.clone())
            // The server chooses these, and they end up in a file name.
            .filter(|sha256| sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit()))
            .filter(|sha256| !self.pictures.contains_key(&icon_key(sha256)))
            .collect();
        for sha256 in wanted {
            let key = icon_key(&sha256);
            self.pictures.insert(key.clone(), None);
            let (client, dir, instance) = (client.clone(), dir.clone(), instance.clone());
            self.run(
                window,
                cx,
                async move { mod_icon(&client, &dir, &instance, &sha256).await },
                move |this, bytes, _, _| {
                    this.pictures.insert(key, bytes.and_then(decode));
                },
            );
        }
    }

    /// Fetches and installs a newer launcher in the background, if this
    /// build follows a release feed.
    fn self_update(&mut self, window: &Window, cx: &mut Context<Self>) {
        let dir = self.dir.clone();
        let task = backend::spawn(async move {
            let Some(found) = update::check().await? else {
                return Ok(None);
            };
            update::install(&found, &dir).await?;
            Ok::<_, update::Error>(Some(found.version.to_string()))
        });
        cx.spawn_in(window, async move |this, cx| match task.await {
            Ok(Ok(Some(version))) => {
                this.update(cx, |this, cx| {
                    this.updated = Some(version.into());
                    cx.notify();
                })
                .ok();
            }
            Ok(Ok(None)) => {}
            Ok(Err(err)) => tracing::warn!("self-update failed: {err}"),
            Err(err) => tracing::warn!("self-update stopped: {err}"),
        })
        .detach();
    }

    /// Starts the freshly installed version and leaves.
    pub(super) fn restart(&mut self, cx: &mut Context<Self>) {
        match update::restart() {
            Ok(()) => cx.quit(),
            Err(err) => tracing::warn!("could not restart: {err}"),
        }
    }

    // -- plumbing ----------------------------------------------------------

    /// Runs `work` off the UI thread and hands its result to `done`, unless
    /// the player has moved to another server or account in the meantime.
    fn run<T: Send + 'static>(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
        work: impl Future<Output = T> + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let generation = self.generation;
        let task = backend::spawn(work);
        cx.spawn_in(window, async move |this, cx| {
            let Ok(output) = task.await else { return };
            this.update_in(cx, |this, window, cx| {
                if this.generation == generation {
                    done(this, output, window, cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn save_config(&self) {
        if let Err(err) = self.config.save(&self.dir) {
            tracing::warn!("could not save settings: {err}");
        }
    }

    /// Where the game is kept: the folder the player chose, or the
    /// launcher's own.
    pub(super) fn game_root(&self) -> PathBuf {
        self.config
            .game_dir
            .clone()
            .unwrap_or_else(|| self.dir.clone())
    }

    /// The folder of the chosen modpack, or of the whole game when none is.
    pub(super) fn pack_dir(&self) -> PathBuf {
        let root = self.game_root();
        match (&self.client, self.selected()) {
            (Some(client), Some(instance)) => {
                game::game_dir(&root, client.origin(), &instance.id).unwrap_or(root)
            }
            _ => root,
        }
    }

    pub(super) fn playing(&self) -> bool {
        matches!(self.play, PlayState::Working { .. } | PlayState::Running)
    }

    /// Shows the modpack's folder in the system's file manager.
    pub(super) fn open_folder(&mut self, cx: &mut Context<Self>) {
        let dir = self.pack_dir();
        // Nothing is there before the first launch.
        if let Err(err) = std::fs::create_dir_all(&dir) {
            self.fail(err);
            cx.notify();
            return;
        }
        cx.open_with_system(&dir);
    }

    /// Lets the player pick another folder for the game.
    pub(super) fn choose_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.playing() {
            return;
        }
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(t(Msg::PutGameHere).into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update(cx, |this, cx| this.set_folder(Some(path), cx))
                .ok();
        })
        .detach();
    }

    /// Keeps the game in `dir` from the next launch on; `None` is the
    /// launcher's own folder.
    pub(super) fn set_folder(&mut self, dir: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.playing() {
            return;
        }
        self.config.game_dir = dir.filter(|dir| *dir != self.dir);
        self.save_config();
        self.play = PlayState::Idle;
        cx.notify();
    }

    fn cache_dir(&self) -> PathBuf {
        self.dir.join("cache")
    }

    pub(super) fn go(&mut self, screen: Screen, cx: &mut Context<Self>) {
        self.screen = screen;
        self.error = None;
        self.notice = None;
        self.busy = false;
        cx.notify();
    }

    fn fail(&mut self, reason: impl ToString) {
        self.busy = false;
        self.notice = None;
        self.error = Some(reason.to_string().into());
    }

    fn begin(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        self.error = None;
        self.notice = None;
        cx.notify();
    }

    fn value(input: &Entity<InputState>, cx: &Context<Self>) -> String {
        input.read(cx).value().trim().to_owned()
    }

    fn clear(input: &Entity<InputState>, window: &mut Window, cx: &mut Context<Self>) {
        input.update(cx, |input, cx| input.set_value("", window, cx));
    }

    /// Enter in any field does what the screen's main button does.
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        match self.screen {
            Screen::Connect => self.connect(window, cx),
            Screen::Login => self.login(window, cx),
            Screen::Register => self.register(window, cx),
            Screen::Reset => self.reset_password(window, cx),
            Screen::Settings => self.change_password(window, cx),
            Screen::Home | Screen::News | Screen::Skin | Screen::Language | Screen::Mods => {}
        }
    }

    /// Gives the game more memory, or less, one step at a time.
    pub(super) fn nudge_memory(&mut self, up: bool, cx: &mut Context<Self>) {
        let now = self.config.memory_mb;
        let next = if up {
            now.saturating_add(MEMORY_STEP_MB)
        } else {
            now.saturating_sub(MEMORY_STEP_MB)
        };
        self.set_memory(next, cx);
    }

    pub(super) fn set_memory(&mut self, mb: u32, cx: &mut Context<Self>) {
        self.config.memory_mb = mb.clamp(MIN_MEMORY_MB, MAX_MEMORY_MB);
        self.save_config();
        cx.notify();
    }

    /// The language the player chose, or `None` while the system decides.
    pub(super) fn language(&self) -> Option<Lang> {
        chosen_language(&self.config)
    }

    /// Says everything in `choice` from now on; `None` follows the system.
    pub(super) fn set_language(
        &mut self,
        choice: Option<Lang>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.config.language = choice.map(|lang| lang.code().to_owned());
        lang::set(choice.unwrap_or_else(Lang::system));
        self.save_config();
        // What was said before the change stays in the old language.
        self.error = None;
        self.notice = None;
        for (input, hint) in [
            (&self.password, Msg::Password),
            (&self.invite, Msg::InviteCode),
            (&self.code, Msg::CodeFromAdmin),
            (&self.current_password, Msg::CurrentPassword),
            (&self.new_password, Msg::NewPassword),
        ] {
            input.update(cx, |input, cx| input.set_placeholder(t(hint), window, cx));
        }
        cx.notify();
    }

    pub(super) fn read_news(&mut self, id: Option<i64>, cx: &mut Context<Self>) {
        self.news_open = id;
        self.go(Screen::News, cx);
    }

    pub(super) fn registration(&self) -> RegistrationPolicy {
        self.handshake
            .as_ref()
            .map(|h| h.registration)
            .unwrap_or(RegistrationPolicy::Closed)
    }

    pub(super) fn selected(&self) -> Option<&InstanceInfo> {
        let instances = &self.handshake.as_ref()?.instances;
        let wanted = self.config.server.as_ref()?.instance.as_deref();
        instances
            .iter()
            .find(|i| Some(i.id.as_str()) == wanted)
            .or_else(|| instances.first())
    }

    // -- server ------------------------------------------------------------

    pub(super) fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = Self::value(&self.address, cx);
        match address::origin(&typed) {
            Ok(origin) => self.connect_to(typed, origin, true, window, cx),
            Err(err) => {
                self.fail(err);
                cx.notify();
            }
        }
    }

    fn connect_to(
        &mut self,
        address: String,
        origin: String,
        // Ask DNS where the panel is; off for an origin found earlier.
        discover: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.generation += 1;
        self.screen = Screen::Connect;
        self.begin(cx);
        let typed = address.clone();
        self.run(
            window,
            cx,
            async move {
                let origin = match discover {
                    true => address::discover(&typed).await.unwrap_or(origin),
                    false => origin,
                };
                open(origin).await
            },
            move |this, result, window, cx| match result {
                Ok(opened) => this.opened(address, opened, window, cx),
                Err(err) => this.fail(err),
            },
        );
    }

    fn opened(
        &mut self,
        address: String,
        opened: Opened,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Opened {
            client,
            handshake,
            news,
            account,
        } = opened;
        let origin = client.origin().to_owned();
        let previous = self.config.server.take().filter(|s| s.origin == origin);
        let server = Server {
            address,
            origin,
            username: previous
                .as_ref()
                .map(|s| s.username.clone())
                .unwrap_or_default(),
            instance: previous.and_then(|s| s.instance),
        };
        self.username.update(cx, |input, cx| {
            input.set_value(server.username.clone(), window, cx)
        });
        self.config.server = Some(server);
        self.save_config();

        theme::apply(&handshake.theme.palette, None, cx);
        self.client = Some(client);
        self.handshake = Some(handshake);
        self.news = news;
        self.manifest = None;
        self.logo = None;
        self.background = None;
        self.pictures.clear();
        self.head = None;
        self.skin = None;
        self.news_open = None;
        self.play = PlayState::Idle;
        self.load_branding(window, cx);
        self.load_pictures(window, cx);

        match account {
            Some(account) => self.signed_in(account, window, cx),
            None => self.go(Screen::Login, cx),
        }
    }

    fn load_branding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(client), Some(handshake)) = (&self.client, &self.handshake) else {
            return;
        };
        let http = client.http().clone();
        let origin = client.origin().to_owned();
        let cache = self.cache_dir();
        let branding = handshake.theme.clone();

        let fetch = |url: &Option<String>| {
            let url = url.as_deref().map(|url| absolute(&origin, url));
            let (http, cache) = (http.clone(), cache.clone());
            async move { assets::fetch(&http, &cache, &url?).await }
        };
        self.run(
            window,
            cx,
            fetch(&branding.logo_url),
            |this, bytes, _, _| this.logo = bytes.and_then(decode),
        );
        self.run(
            window,
            cx,
            fetch(&branding.background_url),
            |this, bytes, _, _| this.background = bytes.and_then(decode),
        );
        let font = fetch(&branding.font_url);
        self.run(
            window,
            cx,
            async move {
                let bytes = font.await?;
                let family = assets::font_family(&bytes)?;
                Some((bytes, family))
            },
            move |_, font, _, cx| {
                let Some((bytes, family)) = font else { return };
                if cx.text_system().add_fonts(vec![Cow::Owned(bytes)]).is_ok() {
                    theme::apply(&branding.palette, Some(family.into()), cx);
                }
            },
        );
    }

    /// Back to the address form, looking like no server in particular.
    pub(super) fn change_server(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self.client = None;
        self.handshake = None;
        self.account = None;
        self.manifest = None;
        self.news.clear();
        self.logo = None;
        self.background = None;
        self.pictures.clear();
        self.head = None;
        self.skin = None;
        self.news_open = None;
        self.play = PlayState::Idle;
        theme::apply(&Palette::default(), Some(self.default_font.clone()), cx);
        self.go(Screen::Connect, cx);
    }

    // -- account -----------------------------------------------------------

    fn signed_in(&mut self, account: Account, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(server) = &mut self.config.server {
            server.username = account.username.clone();
        }
        self.save_config();
        self.slim = account.skin_model == "slim";
        self.account = Some(account);
        for input in [&self.password, &self.invite, &self.code] {
            Self::clear(input, window, cx);
        }
        self.go(Screen::Home, cx);
        self.load_head(window, cx);
        self.load_manifest(window, cx);
        self.reload_news(window, cx);
    }

    /// Keeps the token the server just issued and enters the launcher.
    fn accept(&mut self, login: api::Login, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let (Some(token), Some(client)) = (login.token, self.client.take()) else {
            return false;
        };
        let origin = client.origin().to_owned();
        let saved = token.clone();
        drop(backend::spawn_blocking(move || {
            secrets::save(&origin, &saved)
        }));
        self.client = Some(client.with_token(Some(token)));
        self.generation += 1;
        self.signed_in(login.user, window, cx);
        true
    }

    pub(super) fn login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let username = Self::value(&self.username, cx);
        let password = self.password.read(cx).value().to_string();
        if username.is_empty() || password.is_empty() {
            self.fail(t(Msg::EnterCredentials));
            cx.notify();
            return;
        }
        self.begin(cx);
        self.run(
            window,
            cx,
            async move { client.login(&username, &password).await },
            |this, result, window, cx| match result {
                Ok(login) => {
                    if !this.accept(login, window, cx) {
                        this.fail(t(Msg::NotApproved));
                    }
                }
                Err(err) => this.fail(err),
            },
        );
    }

    pub(super) fn register(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let username = Self::value(&self.username, cx);
        let password = self.password.read(cx).value().to_string();
        let invite = Self::value(&self.invite, cx);
        let needs_invite = self.registration() == RegistrationPolicy::StrictInvite;
        if username.is_empty() || password.is_empty() {
            self.fail(t(Msg::PickCredentials));
        } else if needs_invite && invite.is_empty() {
            self.fail(t(Msg::NeedsInvite));
        }
        if self.error.is_some() {
            cx.notify();
            return;
        }
        self.begin(cx);
        self.run(
            window,
            cx,
            async move {
                let invite = (!invite.is_empty()).then_some(invite.as_str());
                client.register(&username, &password, invite).await
            },
            |this, result, window, cx| match result {
                Ok(login) => {
                    if !this.accept(login, window, cx) {
                        Self::clear(&this.password, window, cx);
                        this.go(Screen::Login, cx);
                        this.notice = Some(t(Msg::AccountCreated).into());
                    }
                }
                Err(err) => this.fail(err),
            },
        );
    }

    pub(super) fn reset_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let username = Self::value(&self.username, cx);
        let code = Self::value(&self.code, cx);
        let password = self.password.read(cx).value().to_string();
        if username.is_empty() || code.is_empty() || password.is_empty() {
            self.fail(t(Msg::EnterReset));
            cx.notify();
            return;
        }
        self.begin(cx);
        self.run(
            window,
            cx,
            async move { client.reset_password(&username, &code, &password).await },
            |this, result, window, cx| match result {
                Ok(()) => {
                    Self::clear(&this.code, window, cx);
                    Self::clear(&this.password, window, cx);
                    this.go(Screen::Login, cx);
                    this.notice = Some(t(Msg::PasswordChangedSignIn).into());
                }
                Err(err) => this.fail(err),
            },
        );
    }

    /// Drops the session on this side, whatever the server thinks of it.
    fn signed_out(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        if let Some(client) = self.client.take() {
            let origin = client.origin().to_owned();
            drop(backend::spawn_blocking(move || secrets::forget(&origin)));
            self.client = Some(client.with_token(None));
        }
        self.account = None;
        self.manifest = None;
        self.head = None;
        self.skin = None;
        self.news_open = None;
        self.play = PlayState::Idle;
        self.go(Screen::Login, cx);
    }

    pub(super) fn logout(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = self.client.clone() {
            drop(backend::spawn(async move { client.logout().await }));
        }
        self.signed_out(cx);
    }

    /// Handles a failed request made on the player's behalf.
    fn refused(&mut self, err: api::Error, cx: &mut Context<Self>) {
        if err.is_unauthorized() {
            self.signed_out(cx);
            self.error = Some(t(Msg::SessionEnded).into());
        } else {
            self.fail(err);
        }
    }

    pub(super) fn change_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let current = self.current_password.read(cx).value().to_string();
        let new = self.new_password.read(cx).value().to_string();
        if current.is_empty() || new.is_empty() {
            self.fail(t(Msg::EnterPasswords));
            cx.notify();
            return;
        }
        self.begin(cx);
        self.run(
            window,
            cx,
            async move { client.change_password(&current, &new).await },
            |this, result, window, cx| match result {
                Ok(()) => {
                    Self::clear(&this.current_password, window, cx);
                    Self::clear(&this.new_password, window, cx);
                    this.busy = false;
                    this.notice = Some(t(Msg::PasswordChanged).into());
                }
                Err(err) if err.is_unauthorized() => {
                    this.fail(t(Msg::WrongCurrentPassword));
                }
                Err(err) => this.fail(err),
            },
        );
    }

    fn reload_news(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.run(
            window,
            cx,
            async move { client.news().await },
            |this, news, window, cx| {
                if let Ok(news) = news {
                    this.news = news;
                }
                this.load_pictures(window, cx);
            },
        );
    }

    // -- skin --------------------------------------------------------------

    fn load_head(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.head = None;
        self.skin = None;
        let (Some(client), Some(handshake), Some(account)) =
            (&self.client, &self.handshake, &self.account)
        else {
            return;
        };
        let Some(skin) = &account.skin_sha256 else {
            return;
        };
        let url = format!(
            "{}/textures/{skin}",
            handshake.yggdrasil_url.trim_end_matches('/')
        );
        let http = client.http().clone();
        let cache = self.cache_dir();
        self.run(
            window,
            cx,
            async move {
                let skin = assets::fetch(&http, &cache, &url).await?;
                let head = assets::skin_head(&skin, 96)?;
                let whole = image::load_from_memory(&skin).ok()?.to_rgba8();
                Some((head, pixels::whole_skin(whole)?))
            },
            |this, loaded, _, _| {
                let Some((head, skin)) = loaded else { return };
                this.head = Some(Arc::new(Image::from_bytes(ImageFormat::Png, head)));
                this.skin = Some(Arc::new(skin));
            },
        );
    }

    pub(super) fn choose_skin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Use as skin".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| this.upload_skin(path, window, cx))
                .ok();
        })
        .detach();
    }

    fn upload_skin(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let slim = self.slim;
        self.begin(cx);
        self.run(
            window,
            cx,
            async move {
                let png = tokio::fs::read(&path)
                    .await
                    .map_err(|err| api::Error::Protocol(tf(Msg::CannotReadFile, &[&err])))?;
                if png.len() > MAX_TEXTURE_BYTES {
                    return Err(api::Error::Protocol(t(Msg::SkinTooBig).into()));
                }
                client.put_texture(Texture::Skin, png, slim).await
            },
            |this, result, window, cx| this.skin_changed(result, t(Msg::SkinUpdated), window, cx),
        );
    }

    pub(super) fn remove_skin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.begin(cx);
        self.run(
            window,
            cx,
            async move { client.delete_texture(Texture::Skin).await },
            |this, result, window, cx| this.skin_changed(result, t(Msg::SkinRemoved), window, cx),
        );
    }

    fn skin_changed(
        &mut self,
        result: api::Result<Account>,
        notice: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(account) => {
                self.busy = false;
                self.notice = Some(notice.into());
                self.account = Some(account);
                self.load_head(window, cx);
            }
            Err(err) => self.refused(err, cx),
        }
    }

    // -- instance ----------------------------------------------------------

    fn load_manifest(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.manifest = None;
        self.reload_manifest(window, cx);
    }

    /// Like [`Self::load_manifest`], with the old one staying up meanwhile.
    fn reload_manifest(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(client), Some(instance)) = (self.client.clone(), self.selected()) else {
            return;
        };
        let instance = instance.id.clone();
        self.run(
            window,
            cx,
            async move { client.manifest(&instance).await },
            |this, result, window, cx| match result {
                Ok(manifest) => {
                    this.manifest = Some(manifest);
                    this.load_icons(window, cx);
                }
                Err(err) if err.is_unauthorized() => this.refused(err, cx),
                Err(err) => tracing::warn!("could not load the pack manifest: {err}"),
            },
        );
    }

    pub(super) fn select(&mut self, instance: String, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.play, PlayState::Working { .. } | PlayState::Running) {
            return;
        }
        if let Some(server) = &mut self.config.server {
            server.instance = Some(instance);
        }
        self.save_config();
        self.play = PlayState::Idle;
        self.load_manifest(window, cx);
        cx.notify();
    }

    pub(super) fn set_optional(&mut self, path: &str, enabled: bool, cx: &mut Context<Self>) {
        let Some(instance) = self.selected().map(|i| i.id.clone()) else {
            return;
        };
        self.config.set_optional(&instance, path, enabled);
        self.save_config();
        cx.notify();
    }

    pub(super) fn open_link(&self, url: &str, cx: &mut Context<Self>) {
        // The server chooses these; only ever hand the system a web address.
        if url.starts_with("https://") || url.starts_with("http://") {
            cx.open_url(url);
        }
    }

    pub(super) fn play(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.play, PlayState::Working { .. } | PlayState::Running) {
            return;
        }
        let (Some(client), Some(handshake), Some(instance)) = (
            self.client.clone(),
            self.handshake.as_ref(),
            self.selected().cloned(),
        ) else {
            return;
        };
        let mut events = game::start(Launch {
            client,
            yggdrasil_url: handshake.yggdrasil_url.clone(),
            memory_mb: self.config.memory_mb,
            optional: self
                .config
                .optional
                .get(&instance.id)
                .cloned()
                .unwrap_or_default(),
            instance,
            data_dir: self.game_root(),
        });
        self.play = PlayState::Working {
            label: t(Msg::CheckingUpdates).into(),
            progress: None,
        };
        cx.notify();

        let generation = self.generation;
        cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = events.recv().await {
                let heard = this.update_in(cx, |this, _, cx| {
                    // The game keeps running on its own if the player has
                    // moved on; it just is no longer this screen's business.
                    if this.generation != generation {
                        return false;
                    }
                    this.play = match event {
                        game::Event::Step { label, progress } => PlayState::Working {
                            label: label.into(),
                            progress,
                        },
                        game::Event::Started => PlayState::Running,
                        game::Event::Exited { crashed: None } => PlayState::Idle,
                        game::Event::Exited {
                            crashed: Some(reason),
                        } => PlayState::Failed(reason.into()),
                        game::Event::Failed {
                            unauthorized: true, ..
                        } => {
                            this.signed_out(cx);
                            this.error = Some(t(Msg::SessionEnded).into());
                            PlayState::Idle
                        }
                        game::Event::Failed { reason, .. } => PlayState::Failed(reason.into()),
                    };
                    cx.notify();
                    true
                });
                if !matches!(heard, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }
}

impl Launcher {
    /// The picture behind a screen: the chosen modpack's banner, else the
    /// server's wallpaper, else the shaft.
    pub(super) fn scene(&self) -> AnyElement {
        let banner = self
            .selected()
            .filter(|_| matches!(self.screen, Screen::Home))
            .and_then(|instance| instance.banner_url.as_deref())
            .and_then(|url| self.picture(url));
        match banner.as_ref().or(self.background.as_ref()) {
            Some(wallpaper) => img(wallpaper.clone())
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => self.mine.clone().into_any_element(),
        }
    }

    /// Tells the pictures what they are showing this frame.
    fn direct(&mut self, cx: &mut Context<Self>) {
        let accent = cx.theme().colors.primary;
        let home = matches!(self.screen, Screen::Home);
        let selected = self.selected().filter(|_| home);
        let seed = match (selected, &self.handshake) {
            (Some(instance), _) => pixels::seed_of(&instance.id),
            (None, Some(handshake)) => pixels::seed_of(&handshake.server_name),
            (None, None) => pixels::seed_of("relic"),
        };
        let online = selected.is_some_and(|i| i.status == InstanceStatus::Running);
        let scene = selected.map_or(Scene::default(), |i| i.scene);
        let launch = match &self.play {
            _ if !home => 0.,
            PlayState::Working { progress, .. } => 0.25 + progress.unwrap_or(0.) * 0.75,
            PlayState::Running => 1.,
            PlayState::Idle | PlayState::Failed(_) => 0.,
        };
        self.mine.update(cx, |stage, _| {
            stage.set_accent(accent);
            stage.set_shaft(scene, seed, launch, online);
        });
        let (skin, slim) = (self.skin.clone(), self.slim);
        self.doll.update(cx, |stage, _| {
            stage.set_accent(accent);
            stage.set_figure(skin, slim);
        });
    }
}

impl Render for Launcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.direct(cx);
        let theme = cx.theme();
        let foreground = theme.colors.foreground;
        let font = theme.font_family.clone();

        let screen: AnyElement = match self.screen {
            Screen::Connect => self.connect_screen(window, cx),
            Screen::Login | Screen::Register | Screen::Reset => self.auth_screen(window, cx),
            Screen::Home => self.home_screen(window, cx),
            Screen::News => self.news_screen(window, cx),
            Screen::Skin => self.skin_screen(window, cx),
            Screen::Settings => self.settings_screen(window, cx),
            Screen::Language => self.language_screen(window, cx),
            Screen::Mods => self.mods_screen(window, cx),
        };

        // The window is one board: the title bar on top, the screen's tiles
        // under it, the canvas showing in the gaps between them.
        div()
            .v_flex()
            .size_full()
            .gap(FRAME)
            .bg(canvas(cx))
            .text_color(foreground)
            .text_sm()
            .font_family(font)
            .child(self.title_bar(window, cx))
            .child(div().flex_1().min_h_0().w_full().child(screen))
    }
}
