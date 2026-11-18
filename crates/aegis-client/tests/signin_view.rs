//! The sign-in view model: forms, stages and texts (job 208).

use aegis_client::connection::{LoginMethod, LoginOptions};
use aegis_client::error::{ClientError, ServerCode};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::signin::{
    LoginForm, SignInView, Stage, login_error, login_form, options_error, stage_of, status_text,
    transport_kind,
};
use aegis_client::state::ConnectionView;
use aegis_client::transport::TransportKind;

const IDS: [&str; 15] = [
    "signin-choose-server",
    "signin-connecting",
    "signin-online",
    "signin-offline",
    "signin-login-company",
    "signin-login-password",
    "signin-login-dev",
    "signin-error-credentials",
    "signin-error-throttled",
    "signin-error-password-off",
    "signin-error-password-turned-off",
    "signin-error-unreachable",
    "signin-error-not-reached",
    "signin-error-too-old",
    "signin-error-login-unfinished",
];

fn options(method: LoginMethod, password_enabled: bool) -> LoginOptions {
    LoginOptions {
        method,
        server_name: "Test".into(),
        password_enabled,
    }
}

fn oidc() -> LoginMethod {
    LoginMethod::Oidc {
        issuer: "https://idp".into(),
        client_id: "desktop".into(),
        resource: None,
    }
}

fn server(code: ServerCode) -> ClientError {
    ClientError::Server {
        code,
        message: "refused by the server".into(),
    }
}

/// A real translation: not empty and not the bracketed id of a gap.
fn assert_translated(text: &str, id: &str) {
    assert!(!text.is_empty(), "{id} is empty");
    assert_ne!(text, id, "{id} is missing");
    assert_ne!(text, format!("[{id}]"), "{id} is missing");
}

#[test]
fn login_form_follows_the_login_method() {
    assert_eq!(
        login_form(&options(LoginMethod::Password, true)),
        LoginForm::Password
    );
    assert_eq!(
        login_form(&options(oidc(), true)),
        LoginForm::Company {
            password_also: true
        }
    );
    assert_eq!(
        login_form(&options(oidc(), false)),
        LoginForm::Company {
            password_also: false
        }
    );
    assert_eq!(
        login_form(&options(LoginMethod::Development, false)),
        LoginForm::Development
    );
}

#[test]
fn stage_follows_the_connection() {
    assert_eq!(stage_of(&ConnectionView::Connecting), Stage::Connecting);
    let online = ConnectionView::Online {
        server_version: "1".into(),
        engine_ok: true,
        license: None,
    };
    assert_eq!(stage_of(&online), Stage::Online);
    let offline = ConnectionView::Offline {
        reason: "down".into(),
    };
    assert_eq!(stage_of(&offline), Stage::Offline);
    assert_eq!(
        stage_of(&ConnectionView::LoginRequired),
        Stage::LoginRequired
    );
    assert_eq!(SignInView::default().stage, Stage::ChooseServer);
}

#[test]
fn status_text_per_stage_and_form() {
    let c = Catalog::load(Language::En);
    let url = "https://tara.example";
    let offline = ConnectionView::Offline {
        reason: "Timed out.".into(),
    };
    let cases: Vec<(String, String, &str)> = vec![
        (
            status_text(Stage::ChooseServer, None, url, None, "", &c),
            c.tr("signin-choose-server"),
            "signin-choose-server",
        ),
        (
            status_text(Stage::Connecting, None, url, None, "", &c),
            c.tr_with("signin-connecting", &[("url", url)]),
            "signin-connecting",
        ),
        (
            status_text(Stage::Online, None, url, None, "alice", &c),
            c.tr_with("signin-online", &[("user", "alice")]),
            "signin-online",
        ),
        (
            status_text(Stage::Offline, None, url, Some(&offline), "", &c),
            c.tr_with("signin-offline", &[("reason", "Timed out.")]),
            "signin-offline",
        ),
        (
            status_text(Stage::LoginRequired, None, url, None, "", &c),
            c.tr("signin-login-company"),
            "signin-login-company",
        ),
        (
            status_text(
                Stage::LoginRequired,
                Some(LoginForm::Company {
                    password_also: true,
                }),
                url,
                None,
                "",
                &c,
            ),
            c.tr("signin-login-company"),
            "signin-login-company",
        ),
        (
            status_text(
                Stage::LoginRequired,
                Some(LoginForm::Password),
                url,
                None,
                "",
                &c,
            ),
            c.tr("signin-login-password"),
            "signin-login-password",
        ),
        (
            status_text(
                Stage::LoginRequired,
                Some(LoginForm::Development),
                url,
                None,
                "",
                &c,
            ),
            c.tr("signin-login-dev"),
            "signin-login-dev",
        ),
    ];
    for (got, want, id) in &cases {
        assert_eq!(got, want, "{id}");
        assert_translated(got, id);
    }
    assert!(cases[1].0.contains(url));
    assert!(cases[2].0.contains("alice"));
    assert!(cases[3].0.contains("Timed out."));

    // Offline without an Offline connection: no reason, still the text.
    assert_eq!(
        status_text(Stage::Offline, None, url, None, "", &c),
        c.tr_with("signin-offline", &[("reason", "")])
    );
}

#[test]
fn login_errors_in_words() {
    let c = Catalog::load(Language::En);
    let credentials = login_error(&server(ServerCode::InvalidCredentials), &c);
    assert_eq!(credentials, c.tr("signin-error-credentials"));
    assert_translated(&credentials, "signin-error-credentials");

    let seven = login_error(
        &server(ServerCode::Throttled {
            retry_after_s: Some(7),
        }),
        &c,
    );
    assert_eq!(
        seven,
        c.tr_with("signin-error-throttled", &[("seconds", "7")])
    );
    assert!(seven.contains('7'));
    let sixty = login_error(
        &server(ServerCode::Throttled {
            retry_after_s: None,
        }),
        &c,
    );
    assert!(sixty.contains("60"), "{sixty}");

    let off = login_error(&server(ServerCode::PasswordLoginOff), &c);
    assert_eq!(off, c.tr("signin-error-password-off"));
    assert_translated(&off, "signin-error-password-off");

    let refused = login_error(&ClientError::Transport("refused".into()), &c);
    assert_eq!(
        refused,
        c.tr_with("signin-error-unreachable", &[("error", "refused")])
    );
    assert!(refused.contains("refused"));

    let protocol = ClientError::Protocol("x".into());
    assert_eq!(login_error(&protocol, &c), protocol.to_string());

    // A server refusal with another code shows the server's sentence.
    let other = server(ServerCode::Busy);
    assert_eq!(login_error(&other, &c), "refused by the server");
}

#[test]
fn options_errors_in_words() {
    let c = Catalog::load(Language::En);
    let url = "https://tara.example";
    let old = options_error(
        url,
        &server(ServerCode::Other("Not implemented".into())),
        &c,
    );
    assert_eq!(old, c.tr("signin-error-too-old"));
    assert_translated(&old, "signin-error-too-old");

    let transport = ClientError::Transport("refused".into());
    let unreached = options_error(url, &transport, &c);
    assert!(unreached.contains(url), "{unreached}");
    assert!(unreached.contains("refused"), "{unreached}");
    assert_eq!(
        unreached,
        c.tr_with(
            "signin-error-not-reached",
            &[("url", url), ("error", &login_error(&transport, &c))]
        )
    );

    // Another "Other" code is not the too-old text.
    let other = options_error(url, &server(ServerCode::Other("internal".into())), &c);
    assert_ne!(other, old);
    assert!(other.contains(url));
}

#[test]
fn every_signin_id_is_translated_in_every_language() {
    for language in [Language::En, Language::Ja, Language::Pseudo] {
        let c = Catalog::load(language);
        for id in IDS {
            assert_translated(&c.tr(id), id);
        }
    }
}

#[test]
fn transport_names_resolve() {
    assert_eq!(
        transport_kind("grpc-web", "http://x"),
        TransportKind::GrpcWeb
    );
    assert_eq!(transport_kind("grpc", "https://x"), TransportKind::Grpc);
    assert_eq!(
        transport_kind("nonsense", "https://x"),
        TransportKind::auto("https://x")
    );
    assert_eq!(transport_kind("auto", "http://x"), TransportKind::Grpc);
}
