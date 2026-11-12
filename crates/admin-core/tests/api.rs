//! Drives the HTTP API over a real socket. No outside network involved.

use std::net::SocketAddr;

use admin_core::api::{self, App, AppState};
use admin_core::instances::{self, NewInstance};
use admin_core::settings;
use admin_core::users::{self, Role, Status};
use relic_proto::{Loader, LoaderKind, Manifest, RegistrationPolicy};
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde_json::{Value, json};

struct Server {
    app: AppState,
    base: String,
    origin: String,
    http: Client,
    _dir: tempfile::TempDir,
}

impl Server {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        // Generating the real 4096-bit key would dominate every test.
        static KEY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let key = KEY.get_or_init(|| admin_core::yggdrasil::generate_key_pem(2048).unwrap());
        std::fs::create_dir(dir.path().join("keys")).unwrap();
        std::fs::write(dir.path().join("keys/yggdrasil.pem"), key).unwrap();
        let app = App::open(dir.path(), false).await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let service = api::router(app.clone()).into_make_service_with_connect_info::<SocketAddr>();
        tokio::spawn(async move { axum::serve(listener, service).await.unwrap() });
        Self {
            app,
            base: format!("{origin}/api/v1"),
            origin,
            http: Client::new(),
            _dir: dir,
        }
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http.request(method, format!("{}{path}", self.base))
    }

    /// A request the way the panel's own pages send it.
    fn panel(&self, method: Method, path: &str, cookie: &str) -> RequestBuilder {
        self.request(method, path)
            .header("cookie", cookie)
            .header("origin", &self.origin)
    }

    /// Completes first-run setup and returns the admin session cookie.
    async fn admin_cookie(&self) -> String {
        let token = self.app.issue_setup_token().await.unwrap().unwrap();
        let response = self
            .request(Method::POST, "/setup")
            .json(&json!({ "token": token, "username": "root", "password": "correct horse" }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        cookie_of(&response)
    }

    async fn add_instance(&self) {
        instances::create(
            &self.app.db,
            NewInstance {
                id: "main".into(),
                name: "Main".into(),
                description: String::new(),
                minecraft_version: "1.21.1".into(),
                loader: Loader {
                    kind: LoaderKind::Fabric,
                    version: "0.16.9".into(),
                },
                java_major: 21,
                port: 25565,
                memory_mb: 2048,
            },
        )
        .await
        .unwrap();
    }
}

fn cookie_of(response: &reqwest::Response) -> String {
    let header = response.headers()["set-cookie"].to_str().unwrap();
    assert!(header.contains("HttpOnly") && header.contains("SameSite=Strict"));
    header.split(';').next().unwrap().to_owned()
}

#[tokio::test]
async fn setup_creates_the_only_admin() {
    let server = Server::start().await;
    let status: Value = server
        .request(Method::GET, "/setup")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status["needed"], true);

    let token = server.app.issue_setup_token().await.unwrap().unwrap();
    let wrong = server
        .request(Method::POST, "/setup")
        .json(&json!({ "token": "AAAA-AAAA-AAAA-AAAA", "username": "root", "password": "correct horse" }))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::FORBIDDEN);

    let body = json!({ "token": token, "username": "root", "password": "correct horse" });
    let done = server
        .request(Method::POST, "/setup")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(done.status(), StatusCode::OK);
    let cookie = cookie_of(&done);

    let again = server
        .request(Method::POST, "/setup")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::CONFLICT);

    let me: Value = server
        .panel(Method::GET, "/auth/me", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["username"], "root");
    assert_eq!(me["role"], "admin");
    assert!(me.get("password_hash").is_none());
}

#[tokio::test]
async fn admin_routes_need_a_session_and_a_same_site_origin() {
    let server = Server::start().await;
    let cookie = server.admin_cookie().await;

    let anonymous = server
        .request(Method::GET, "/admin/instances")
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let error: Value = anonymous.json().await.unwrap();
    assert_eq!(error["code"], "unauthorized");

    let news = json!({ "title": "Hello", "body": "First post" });
    let forged = server
        .request(Method::POST, "/admin/news")
        .header("cookie", &cookie)
        .header("origin", "https://evil.example")
        .json(&news)
        .send()
        .await
        .unwrap();
    assert_eq!(forged.status(), StatusCode::FORBIDDEN);
    let no_origin = server
        .request(Method::POST, "/admin/news")
        .header("cookie", &cookie)
        .json(&news)
        .send()
        .await
        .unwrap();
    assert_eq!(no_origin.status(), StatusCode::FORBIDDEN);

    let created = server
        .panel(Method::POST, "/admin/news", &cookie)
        .json(&news)
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let public: Value = server
        .request(Method::GET, "/news")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(public[0]["title"], "Hello");

    // A launcher session of a player opens nothing under /admin.
    users::create(
        &server.app.db,
        "steve",
        "hunter2hunter2",
        Role::Player,
        Status::Active,
    )
    .await
    .unwrap();
    let login: Value = server
        .request(Method::POST, "/auth/login")
        .json(&json!({ "username": "steve", "password": "hunter2hunter2" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let player = server
        .request(Method::GET, "/admin/users")
        .bearer_auth(login["token"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(player.status(), StatusCode::UNAUTHORIZED);
    let as_admin = server
        .request(Method::POST, "/auth/login")
        .json(&json!({ "username": "steve", "password": "hunter2hunter2", "admin": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(as_admin.status(), StatusCode::FORBIDDEN);

    let logout = server
        .panel(Method::POST, "/auth/logout", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    let after = server
        .panel(Method::GET, "/admin/users", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn uploaded_files_reach_the_launcher_by_side() {
    let server = Server::start().await;
    let cookie = server.admin_cookie().await;
    server.add_instance().await;

    let shared = b"shared = true\n".repeat(64);
    let uploaded = server
        .panel(
            Method::PUT,
            "/admin/instances/main/files/config/shared.toml?side=both",
            &cookie,
        )
        .body(shared.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(uploaded.status(), StatusCode::OK);
    let shared_record: Value = uploaded.json().await.unwrap();
    assert_eq!(shared_record["side"], "both");
    let secret: Value = server
        .panel(
            Method::PUT,
            "/admin/instances/main/files/config/secret.toml?side=server",
            &cookie,
        )
        .body("rcon = 'hunter2'\n")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let escape = server
        .panel(
            Method::PUT,
            "/admin/instances/main/files/config/..%2F..%2Fescape.txt",
            &cookie,
        )
        .body("x")
        .send()
        .await
        .unwrap();
    assert_eq!(escape.status(), StatusCode::BAD_REQUEST);

    let listed: Value = server
        .panel(Method::GET, "/admin/instances/main/files", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 2);

    let anonymous = server
        .request(Method::GET, "/instances/main/manifest")
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    users::create(
        &server.app.db,
        "alex",
        "hunter2hunter2",
        Role::Player,
        Status::Active,
    )
    .await
    .unwrap();
    let login: Value = server
        .request(Method::POST, "/auth/login")
        .json(&json!({ "username": "alex", "password": "hunter2hunter2" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = login["token"].as_str().unwrap();

    let manifest: Manifest = server
        .request(Method::GET, "/instances/main/manifest")
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["config/shared.toml"]);

    let sha = shared_record["sha256"].as_str().unwrap();
    let ranged = server
        .request(Method::GET, &format!("/instances/main/blobs/{sha}"))
        .bearer_auth(token)
        .header("range", "bytes=7-12")
        .send()
        .await
        .unwrap();
    assert_eq!(ranged.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(ranged.bytes().await.unwrap().as_ref(), &shared[7..=12]);

    // Server-only content is invisible to players, but not to the admin.
    let secret_path = format!(
        "/instances/main/blobs/{}",
        secret["sha256"].as_str().unwrap()
    );
    let denied = server
        .request(Method::GET, &secret_path)
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::NOT_FOUND);
    let granted = server
        .panel(Method::GET, &secret_path, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(granted.status(), StatusCode::OK);

    let handshake: Value = server
        .request(Method::GET, "/handshake")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(handshake["instances"][0]["address"], "127.0.0.1:25565");
    assert_eq!(handshake["instances"][0]["status"], "stopped");

    let start = server
        .panel(Method::POST, "/admin/instances/main/start", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(
        start.status(),
        StatusCode::CONFLICT,
        "EULA not accepted yet"
    );
}

#[tokio::test]
async fn repeated_bad_logins_are_throttled() {
    let server = Server::start().await;
    server.admin_cookie().await;
    let attempt = |password: &'static str| {
        server
            .request(Method::POST, "/auth/login")
            .json(&json!({ "username": "root", "password": password }))
            .send()
    };
    for _ in 0..10 {
        assert_eq!(
            attempt("wrong password").await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }
    let locked = attempt("correct horse").await.unwrap();
    assert_eq!(locked.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ---------------------------------------------------------------------------
// Accounts, Yggdrasil, skins
// ---------------------------------------------------------------------------

const PLAYER_PASSWORD: &str = "hunter2 hunter2";

impl Server {
    fn ygg(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, format!("{}/api/yggdrasil{path}", self.origin))
    }

    async fn add_player(&self, name: &str) -> users::User {
        users::create(
            &self.app.db,
            name,
            PLAYER_PASSWORD,
            Role::Player,
            Status::Active,
        )
        .await
        .unwrap()
    }

    /// Signs in the way the launcher does and returns the bearer token.
    async fn launcher_token(&self, name: &str, password: &str) -> String {
        let response = self
            .request(Method::POST, "/auth/login")
            .json(&json!({ "username": name, "password": password }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        body["token"].as_str().unwrap().to_owned()
    }

    async fn set_registration(&self, policy: RegistrationPolicy) {
        let mut general = settings::general(&self.app.db).await.unwrap();
        general.registration = policy;
        settings::set_general(&self.app.db, &general).await.unwrap();
    }
}

async fn status_of(request: RequestBuilder) -> StatusCode {
    request.send().await.unwrap().status()
}

async fn json_of(request: RequestBuilder) -> Value {
    let response = request.send().await.unwrap();
    assert!(
        response.status().is_success(),
        "unexpected status {}",
        response.status()
    );
    response.json().await.unwrap()
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&vec![120u8; (width * height * 4) as usize])
        .unwrap();
    writer.finish().unwrap();
    bytes
}

/// Checks every property's signature against the key the API publishes and
/// returns the decoded `textures` payload.
fn verified_textures(profile: &Value, public_pem: &str) -> Value {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use rsa::pkcs8::DecodePublicKey;

    let public = rsa::PublicKey::from_public_key_pem(public_pem).unwrap();
    let properties = profile["properties"].as_array().unwrap();
    for property in properties {
        let value = property["value"].as_str().unwrap();
        let signature = STANDARD
            .decode(property["signature"].as_str().unwrap())
            .unwrap();
        public
            .pkcs1v15_verify::<sha1::Sha1>(value.as_bytes(), &signature)
            .unwrap();
    }
    let textures = properties
        .iter()
        .find(|property| property["name"] == "textures")
        .unwrap();
    serde_json::from_slice(
        &STANDARD
            .decode(textures["value"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn yggdrasil_signs_players_in_and_confirms_joins() {
    let server = Server::start().await;
    let alex = server.add_player("Alex").await;
    let profile_id = alex.id.simple().to_string();

    // The metadata root, with and without the slash authlib-injector adds.
    let root = server.ygg(Method::GET, "/").send().await.unwrap();
    assert_eq!(root.status(), StatusCode::OK);
    assert_eq!(
        root.headers()["x-authlib-injector-api-location"],
        "/api/yggdrasil/"
    );
    let meta: Value = root.json().await.unwrap();
    assert_eq!(meta["meta"]["feature.non_email_login"], true);
    assert_eq!(meta["skinDomains"][0], "127.0.0.1");
    let public_pem = meta["signaturePublickey"].as_str().unwrap().to_owned();
    assert!(public_pem.starts_with("-----BEGIN PUBLIC KEY-----"));
    assert_eq!(status_of(server.ygg(Method::GET, "")).await, StatusCode::OK);

    let unknown = server.ygg(Method::GET, "/nope").send().await.unwrap();
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    let unknown: Value = unknown.json().await.unwrap();
    assert!(unknown["errorMessage"].is_string());

    // Signing in.
    let refused = server
        .ygg(Method::POST, "/authserver/authenticate")
        .json(&json!({ "username": "Alex", "password": "not the password" }))
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    let refused: Value = refused.json().await.unwrap();
    assert_eq!(refused["error"], "ForbiddenOperationException");

    let session = json_of(
        server
            .ygg(Method::POST, "/authserver/authenticate")
            .json(&json!({
                "username": "alex",
                "password": PLAYER_PASSWORD,
                "clientToken": "launcher-1",
                "requestUser": true,
            })),
    )
    .await;
    assert_eq!(session["clientToken"], "launcher-1");
    assert_eq!(session["selectedProfile"]["id"], profile_id.as_str());
    assert_eq!(session["selectedProfile"]["name"], "Alex");
    assert_eq!(session["availableProfiles"].as_array().unwrap().len(), 1);
    assert_eq!(session["user"]["id"], profile_id.as_str());
    let access = session["accessToken"].as_str().unwrap().to_owned();

    let validate = |access: &str, client: Option<&str>| {
        let mut body = json!({ "accessToken": access });
        if let Some(client) = client {
            body["clientToken"] = json!(client);
        }
        status_of(server.ygg(Method::POST, "/authserver/validate").json(&body))
    };
    assert_eq!(validate(&access, None).await, StatusCode::NO_CONTENT);
    assert_eq!(
        validate(&access, Some("launcher-1")).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        validate(&access, Some("someone else")).await,
        StatusCode::FORBIDDEN
    );
    // A launcher token is not a game token.
    let launcher = server.launcher_token("Alex", PLAYER_PASSWORD).await;
    assert_eq!(validate(&launcher, None).await, StatusCode::FORBIDDEN);

    // A skin, uploaded the way third-party launchers do it.
    let skin_path = format!("/api/user/profile/{profile_id}/skin");
    let form = || {
        reqwest::multipart::Form::new().text("model", "slim").part(
            "file",
            reqwest::multipart::Part::bytes(png(64, 64))
                .file_name("skin.png")
                .mime_str("image/png")
                .unwrap(),
        )
    };
    assert_eq!(
        status_of(server.ygg(Method::PUT, &skin_path).multipart(form())).await,
        StatusCode::UNAUTHORIZED
    );
    let other = server.add_player("Steve").await;
    assert_eq!(
        status_of(
            server
                .ygg(
                    Method::PUT,
                    &format!("/api/user/profile/{}/skin", other.id.simple())
                )
                .bearer_auth(&access)
                .multipart(form())
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status_of(
            server
                .ygg(Method::PUT, &skin_path)
                .bearer_auth(&access)
                .multipart(form())
        )
        .await,
        StatusCode::NO_CONTENT
    );

    // Joining a server.
    let join = |profile: &str, server_id: &str| {
        status_of(
            server
                .ygg(Method::POST, "/sessionserver/session/minecraft/join")
                .json(&json!({
                    "accessToken": access,
                    "selectedProfile": profile,
                    "serverId": server_id,
                })),
        )
    };
    assert_eq!(
        join(&other.id.simple().to_string(), "abc").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(join(&profile_id, "abc").await, StatusCode::NO_CONTENT);

    let has_joined = |username: &str, server_id: &str| {
        server
            .ygg(
                Method::GET,
                &format!(
                    "/sessionserver/session/minecraft/hasJoined?username={username}&serverId={server_id}"
                ),
            )
            .send()
    };
    assert_eq!(
        has_joined("Alex", "other").await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        has_joined("Steve", "abc").await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    let joined = has_joined("Alex", "abc").await.unwrap();
    assert_eq!(joined.status(), StatusCode::OK);
    let joined: Value = joined.json().await.unwrap();
    assert_eq!(joined["id"], profile_id.as_str());
    let textures = verified_textures(&joined, &public_pem);
    assert_eq!(textures["profileName"], "Alex");
    assert_eq!(textures["textures"]["SKIN"]["metadata"]["model"], "slim");

    // The skin is public at the signed URL, and nothing else is.
    let url = textures["textures"]["SKIN"]["url"].as_str().unwrap();
    assert!(url.starts_with(&format!("{}/api/yggdrasil/textures/", server.origin)));
    let image = server.http.get(url).send().await.unwrap();
    assert_eq!(image.status(), StatusCode::OK);
    assert_eq!(image.headers()["content-type"], "image/png");
    assert!(image.bytes().await.unwrap().starts_with(b"\x89PNG"));
    assert_eq!(
        status_of(server.ygg(Method::GET, &format!("/textures/{}", "0".repeat(64)))).await,
        StatusCode::NOT_FOUND
    );

    // Profile lookups.
    let profile_path = format!("/sessionserver/session/minecraft/profile/{profile_id}");
    let unsigned = json_of(server.ygg(Method::GET, &profile_path)).await;
    assert!(unsigned["properties"][0].get("signature").is_none());
    let signed = json_of(server.ygg(Method::GET, &format!("{profile_path}?unsigned=false"))).await;
    verified_textures(&signed, &public_pem);
    assert_eq!(
        status_of(server.ygg(
            Method::GET,
            &format!(
                "/sessionserver/session/minecraft/profile/{}",
                uuid::Uuid::new_v4().simple()
            )
        ))
        .await,
        StatusCode::NO_CONTENT
    );
    let found = json_of(
        server
            .ygg(Method::POST, "/api/profiles/minecraft")
            .json(&json!(["alex", "Nobody", "ALEX"])),
    )
    .await;
    assert_eq!(found, json!([{ "id": profile_id, "name": "Alex" }]));

    // Taking the skin off again.
    assert_eq!(
        status_of(server.ygg(Method::DELETE, &skin_path).bearer_auth(&access)).await,
        StatusCode::NO_CONTENT
    );
    let bare = json_of(server.ygg(Method::GET, &profile_path)).await;
    assert_eq!(bare["properties"][0].get("signature"), None);
    assert_eq!(status_of(server.http.get(url)).await, StatusCode::NOT_FOUND);

    // Token life cycle.
    let refreshed = json_of(
        server
            .ygg(Method::POST, "/authserver/refresh")
            .json(&json!({ "accessToken": access, "clientToken": "launcher-1" })),
    )
    .await;
    let fresh = refreshed["accessToken"].as_str().unwrap().to_owned();
    assert_ne!(fresh, access);
    assert_eq!(refreshed["selectedProfile"]["id"], profile_id.as_str());
    assert_eq!(validate(&access, None).await, StatusCode::FORBIDDEN);
    assert_eq!(validate(&fresh, None).await, StatusCode::NO_CONTENT);

    assert_eq!(
        status_of(
            server
                .ygg(Method::POST, "/authserver/invalidate")
                .json(&json!({ "accessToken": fresh }))
        )
        .await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(validate(&fresh, None).await, StatusCode::FORBIDDEN);

    let again = json_of(
        server
            .ygg(Method::POST, "/authserver/authenticate")
            .json(&json!({ "username": "Alex", "password": PLAYER_PASSWORD })),
    )
    .await;
    let again = again["accessToken"].as_str().unwrap().to_owned();
    assert_eq!(
        status_of(
            server
                .ygg(Method::POST, "/authserver/signout")
                .json(&json!({ "username": "Alex", "password": PLAYER_PASSWORD }))
        )
        .await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(validate(&again, None).await, StatusCode::FORBIDDEN);
    // Signing out of the game leaves the launcher signed in.
    assert_eq!(
        status_of(
            server
                .request(Method::GET, "/auth/me")
                .bearer_auth(&launcher)
        )
        .await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn registration_follows_the_policy_and_admins_manage_accounts() {
    let server = Server::start().await;
    let cookie = server.admin_cookie().await;
    let register = |name: &str, invite: Option<&str>| {
        server
            .request(Method::POST, "/auth/register")
            .json(&json!({ "username": name, "password": PLAYER_PASSWORD, "invite": invite }))
            .send()
    };
    let login = |name: &str, password: &str| {
        status_of(
            server
                .request(Method::POST, "/auth/login")
                .json(&json!({ "username": name, "password": password })),
        )
    };

    server.set_registration(RegistrationPolicy::Closed).await;
    assert_eq!(
        register("Alex", None).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    // Invites.
    server
        .set_registration(RegistrationPolicy::StrictInvite)
        .await;
    assert_eq!(
        register("Alex", None).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status_of(
            server
                .request(Method::POST, "/admin/invites")
                .json(&json!({ "note": "nope" }))
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
    let created = server
        .panel(Method::POST, "/admin/invites", &cookie)
        .json(&json!({ "note": "for Alex", "expires_in_hours": 24 }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: Value = created.json().await.unwrap();
    let code = created["code"].as_str().unwrap().to_owned();
    assert_eq!(created["invite"]["note"], "for Alex");
    assert_eq!(created["invite"]["created_by"], "root");

    assert_eq!(
        register("Alex", Some("AAAA-AAAA-AAAA-AAAA"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let registered = register("Alex", Some(&code)).await.unwrap();
    assert_eq!(registered.status(), StatusCode::CREATED);
    let registered: Value = registered.json().await.unwrap();
    assert_eq!(registered["user"]["status"], "active");
    let alex_token = registered["token"].as_str().unwrap().to_owned();
    let alex_id = registered["user"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        register("Steve", Some(&code)).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        register("alex", None).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let invites = json_of(server.panel(Method::GET, "/admin/invites", &cookie)).await;
    assert_eq!(invites[0]["used_by"], "Alex");
    assert!(invites[0].get("code").is_none() && invites[0].get("code_hash").is_none());
    let invite_path = format!("/admin/invites/{}", invites[0]["id"]);
    assert_eq!(
        status_of(server.panel(Method::DELETE, &invite_path, &cookie)).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        status_of(server.panel(Method::DELETE, &invite_path, &cookie)).await,
        StatusCode::NOT_FOUND
    );

    // Approval.
    server
        .set_registration(RegistrationPolicy::ApprovalRequired)
        .await;
    let pending = register("Steve", None).await.unwrap();
    assert_eq!(pending.status(), StatusCode::CREATED);
    let pending: Value = pending.json().await.unwrap();
    assert_eq!(pending["user"]["status"], "pending");
    assert!(pending.get("token").is_none());
    let steve_path = format!("/admin/users/{}", pending["user"]["id"].as_str().unwrap());
    assert_eq!(login("Steve", PLAYER_PASSWORD).await, StatusCode::FORBIDDEN);
    let approved = json_of(
        server
            .panel(Method::PATCH, &steve_path, &cookie)
            .json(&json!({ "status": "active" })),
    )
    .await;
    assert_eq!(approved["status"], "active");
    assert_eq!(login("Steve", PLAYER_PASSWORD).await, StatusCode::OK);
    assert_eq!(
        status_of(
            server
                .panel(Method::PATCH, &steve_path, &cookie)
                .json(&json!({ "status": "pending" }))
        )
        .await,
        StatusCode::BAD_REQUEST
    );

    // A ban signs the player out everywhere.
    let alex_path = format!("/admin/users/{alex_id}");
    let me = || {
        status_of(
            server
                .request(Method::GET, "/auth/me")
                .bearer_auth(&alex_token),
        )
    };
    assert_eq!(me().await, StatusCode::OK);
    let banned = json_of(
        server
            .panel(Method::PATCH, &alex_path, &cookie)
            .json(&json!({ "status": "banned" })),
    )
    .await;
    assert_eq!(banned["status"], "banned");
    assert_eq!(me().await, StatusCode::UNAUTHORIZED);
    assert_eq!(login("Alex", PLAYER_PASSWORD).await, StatusCode::FORBIDDEN);
    json_of(
        server
            .panel(Method::PATCH, &alex_path, &cookie)
            .json(&json!({ "status": "active" })),
    )
    .await;

    // A forgotten password.
    let reset =
        json_of(server.panel(Method::POST, &format!("{alex_path}/reset-code"), &cookie)).await;
    let reset_code = reset["code"].as_str().unwrap().to_owned();
    assert!(reset["expires_at"].as_i64().unwrap() > 0);
    let new_password = "a brand new password";
    let trade = |code: &str| {
        status_of(server.request(Method::POST, "/auth/reset").json(&json!({
            "username": "Alex",
            "code": code,
            "password": new_password,
        })))
    };
    assert_eq!(trade("AAAA-AAAA-AAAA-AAAA").await, StatusCode::FORBIDDEN);
    assert_eq!(trade(&reset_code).await, StatusCode::NO_CONTENT);
    assert_eq!(trade(&reset_code).await, StatusCode::FORBIDDEN);
    assert_eq!(
        login("Alex", PLAYER_PASSWORD).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(login("Alex", new_password).await, StatusCode::OK);

    // Accounts made by hand, roles, and the limits on acting on oneself.
    let made = server
        .panel(Method::POST, "/admin/users", &cookie)
        .json(&json!({ "username": "Robin", "password": PLAYER_PASSWORD, "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(made.status(), StatusCode::CREATED);
    let made: Value = made.json().await.unwrap();
    assert_eq!(made["role"], "admin");
    let robin_path = format!("/admin/users/{}", made["id"].as_str().unwrap());
    let demoted = json_of(
        server
            .panel(Method::PATCH, &robin_path, &cookie)
            .json(&json!({ "role": "player" })),
    )
    .await;
    assert_eq!(demoted["role"], "player");

    let all = json_of(server.panel(Method::GET, "/admin/users", &cookie)).await;
    let root_id = all
        .as_array()
        .unwrap()
        .iter()
        .find(|user| user["username"] == "root")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let root_path = format!("/admin/users/{root_id}");
    assert_eq!(
        status_of(
            server
                .panel(Method::PATCH, &root_path, &cookie)
                .json(&json!({ "status": "banned" }))
        )
        .await,
        StatusCode::CONFLICT
    );
    assert_eq!(
        status_of(server.panel(Method::DELETE, &root_path, &cookie)).await,
        StatusCode::CONFLICT
    );

    assert_eq!(
        status_of(server.panel(Method::DELETE, &robin_path, &cookie)).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        status_of(server.panel(Method::DELETE, &robin_path, &cookie)).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        login("Robin", PLAYER_PASSWORD).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn players_manage_their_own_account() {
    let server = Server::start().await;
    server.add_player("Alex").await;
    let token = server.launcher_token("Alex", PLAYER_PASSWORD).await;
    let account = |method: Method, path: &str| {
        server
            .request(method, &format!("/account{path}"))
            .bearer_auth(&token)
    };

    // Skins and capes.
    assert_eq!(
        status_of(
            server
                .request(Method::PUT, "/account/skin")
                .body(png(64, 64))
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        status_of(account(Method::PUT, "/skin").body("not an image")).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status_of(account(Method::PUT, "/skin").body(png(64, 48))).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status_of(account(Method::PUT, "/skin?model=wide").body(png(64, 64))).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status_of(account(Method::PUT, "/hat").body(png(64, 64))).await,
        StatusCode::NOT_FOUND
    );
    let dressed = json_of(account(Method::PUT, "/skin?model=slim").body(png(64, 64))).await;
    assert_eq!(dressed["skin_model"], "slim");
    let skin = dressed["skin_sha256"].as_str().unwrap().to_owned();
    let caped = json_of(account(Method::PUT, "/cape").body(png(64, 32))).await;
    let cape = caped["cape_sha256"].as_str().unwrap().to_owned();
    assert_ne!(skin, cape);
    for hash in [&skin, &cape] {
        assert_eq!(
            status_of(server.ygg(Method::GET, &format!("/textures/{hash}"))).await,
            StatusCode::OK
        );
    }
    // A sweep keeps what accounts wear.
    assert_eq!(server.app.modpack.collect_garbage().await.unwrap(), 0);

    let undressed = json_of(account(Method::DELETE, "/skin")).await;
    assert_eq!(undressed["skin_sha256"], Value::Null);
    assert_eq!(undressed["skin_model"], "classic");
    assert_eq!(undressed["cape_sha256"], cape.as_str());
    assert_eq!(
        status_of(server.ygg(Method::GET, &format!("/textures/{skin}"))).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(server.app.modpack.collect_garbage().await.unwrap(), 1);

    // The session the launcher starts the game with.
    let game = json_of(account(Method::POST, "/game-session")).await;
    assert_eq!(game["profile"]["name"], "Alex");
    let validate =
        |body: Value| status_of(server.ygg(Method::POST, "/authserver/validate").json(&body));
    assert_eq!(
        validate(json!({
            "accessToken": game["access_token"],
            "clientToken": game["client_token"],
        }))
        .await,
        StatusCode::NO_CONTENT
    );

    // Changing the password keeps this session and drops the others.
    let elsewhere = server.launcher_token("Alex", PLAYER_PASSWORD).await;
    let new_password = "a brand new password";
    assert_eq!(
        status_of(
            account(Method::POST, "/password")
                .json(&json!({ "current": "not the password", "new": new_password }))
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status_of(
            account(Method::POST, "/password")
                .json(&json!({ "current": PLAYER_PASSWORD, "new": "short" }))
        )
        .await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status_of(
            account(Method::POST, "/password")
                .json(&json!({ "current": PLAYER_PASSWORD, "new": new_password }))
        )
        .await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        status_of(server.request(Method::GET, "/auth/me").bearer_auth(&token)).await,
        StatusCode::OK
    );
    assert_eq!(
        status_of(
            server
                .request(Method::GET, "/auth/me")
                .bearer_auth(&elsewhere)
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        validate(json!({ "accessToken": game["access_token"] })).await,
        StatusCode::FORBIDDEN
    );
    server.launcher_token("Alex", new_password).await;
}
