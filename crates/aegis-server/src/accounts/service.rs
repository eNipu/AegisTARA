//! The public `Accounts` gRPC service (login options and password login),
//! and the mapping of [`AccountError`] to gRPC statuses.
//!
//! This service is reachable **without credentials**: the authentication
//! layer lets `/aegis.v1.Accounts/*` through (see `auth_layer`). It must
//! therefore never read a [`Principal`](crate::Principal) and never return
//! anything but login options and the caller's own new session.

use std::sync::Arc;

use tonic::metadata::MetadataValue;
use tonic::{Code, Request, Response, Status};

use super::{AccountError, Accounts, UserRecord};
use crate::error::ERROR_CODE_HEADER;
use crate::pb;
use crate::pb::get_login_options_response::Method;

/// Path prefix of the public service, which the authentication layer lets
/// through.
pub const PUBLIC_PATH_PREFIX: &str = "/aegis.v1.Accounts/";

/// Metadata key with the seconds to wait after a throttled login.
pub const RETRY_AFTER_HEADER: &str = "x-aegis-retry-after";

/// Largest `LogIn` request read, in bytes: name, password and client name
/// with room to spare (R6).
const MAX_LOGIN_REQUEST_BYTES: usize = 4096;

/// How clients log in to this server, as `GetLoginOptions` reports it.
#[derive(Clone, Debug)]
pub struct LoginOptions {
    /// The method.
    pub method: pb::get_login_options_response::Method,
    /// OIDC issuer, client ID and resource, for the OIDC method.
    pub oidc_issuer: String,
    /// The public client ID of the desktop app.
    pub oidc_client_id: String,
    /// The API the access token is for, if the provider needs it.
    pub oidc_resource: String,
    /// Shown on the login screen.
    pub server_name: String,
}

impl LoginOptions {
    /// Password login.
    #[must_use]
    pub fn password(server_name: &str) -> Self {
        LoginOptions {
            method: pb::get_login_options_response::Method::Password,
            oidc_issuer: String::new(),
            oidc_client_id: String::new(),
            oidc_resource: String::new(),
            server_name: server_name.to_owned(),
        }
    }

    /// Development login.
    #[must_use]
    pub fn development(server_name: &str) -> Self {
        LoginOptions {
            method: pb::get_login_options_response::Method::Development,
            ..Self::password(server_name)
        }
    }
}

/// The `Accounts` service.
pub struct AccountsService {
    accounts: Option<Arc<Accounts>>,
    options: LoginOptions,
}

impl AccountsService {
    /// `accounts` is `None` when password login is off; `LogIn` is then
    /// refused.
    #[must_use]
    pub fn new(accounts: Option<Arc<Accounts>>, options: LoginOptions) -> Self {
        AccountsService { accounts, options }
    }

    /// The service ready to mount, with the request size cap (R6).
    #[must_use]
    pub fn into_server(self) -> pb::accounts_server::AccountsServer<Self> {
        pb::accounts_server::AccountsServer::new(self)
            .max_decoding_message_size(MAX_LOGIN_REQUEST_BYTES)
    }
}

/// The API view of a user.
#[must_use]
pub fn user_info(user: &UserRecord, sessions: u32) -> pb::UserInfo {
    pb::UserInfo {
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        admin: user.admin,
        disabled: user.disabled,
        created_ms: user.created_ms,
        last_login_ms: user.last_login_ms,
        sessions,
    }
}

impl AccountError {
    /// The stable code clients branch on.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            AccountError::InvalidCredentials => "invalid_credentials",
            AccountError::Throttled { .. } => "throttled",
            AccountError::Rule(_) => "weak_password",
            AccountError::InvalidUsername => "invalid_username",
            AccountError::InvalidDisplayName => "invalid_display_name",
            AccountError::UnknownUser => "unknown_user",
            AccountError::UserExists => "user_exists",
            AccountError::LastAdmin => "last_admin",
            AccountError::PasswordLoginOff => "password_login_off",
            AccountError::NoCompanyLogin => "no_company_login",
            AccountError::Full => "account_limit",
            AccountError::Busy => "busy",
            AccountError::Store(_) => "storage",
            AccountError::Internal => "internal",
        }
    }

    fn grpc_code(&self) -> Code {
        match self {
            AccountError::InvalidCredentials => Code::Unauthenticated,
            AccountError::Throttled { .. } | AccountError::Full => Code::ResourceExhausted,
            AccountError::Rule(_)
            | AccountError::InvalidUsername
            | AccountError::InvalidDisplayName => Code::InvalidArgument,
            AccountError::UnknownUser => Code::NotFound,
            AccountError::UserExists => Code::AlreadyExists,
            AccountError::LastAdmin
            | AccountError::PasswordLoginOff
            | AccountError::NoCompanyLogin => Code::FailedPrecondition,
            AccountError::Busy | AccountError::Store(_) => Code::Unavailable,
            AccountError::Internal => Code::Internal,
        }
    }
}

impl From<AccountError> for Status {
    fn from(e: AccountError) -> Status {
        if let AccountError::Store(fault) = &e {
            tracing::error!(%fault, "account storage failure");
        }
        let mut status = Status::new(e.grpc_code(), e.to_string());
        let md = status.metadata_mut();
        md.insert(ERROR_CODE_HEADER, MetadataValue::from_static(e.code()));
        if let AccountError::Throttled { retry_after_s } = &e
            && let Ok(v) = retry_after_s.to_string().parse()
        {
            md.insert(RETRY_AFTER_HEADER, v);
        }
        status
    }
}

/// The refusal when the server keeps no event log.
#[must_use]
pub fn no_event_log() -> Status {
    let mut status = Status::failed_precondition("this server keeps no event log");
    status.metadata_mut().insert(
        ERROR_CODE_HEADER,
        MetadataValue::from_static("no_event_log"),
    );
    status
}

/// The refusal when password login is off.
#[must_use]
pub fn password_login_off() -> Status {
    let mut status = Status::failed_precondition("password login is off on this server");
    status.metadata_mut().insert(
        ERROR_CODE_HEADER,
        MetadataValue::from_static("password_login_off"),
    );
    status
}

#[tonic::async_trait]
impl pb::accounts_server::Accounts for AccountsService {
    async fn get_login_options(
        &self,
        _request: Request<pb::GetLoginOptionsRequest>,
    ) -> Result<Response<pb::GetLoginOptionsResponse>, Status> {
        let o = &self.options;
        Ok(Response::new(pb::GetLoginOptionsResponse {
            method: o.method.into(),
            oidc_issuer: o.oidc_issuer.clone(),
            oidc_client_id: o.oidc_client_id.clone(),
            oidc_resource: o.oidc_resource.clone(),
            server_name: o.server_name.clone(),
            password_enabled: match o.method {
                Method::Password => true,
                Method::Development | Method::Unspecified => false,
                Method::Oidc => self
                    .accounts
                    .as_ref()
                    .is_some_and(|a| a.password_login_enabled()),
            },
        }))
    }

    async fn log_in(
        &self,
        request: Request<pb::LogInRequest>,
    ) -> Result<Response<pb::LogInResponse>, Status> {
        let Some(accounts) = &self.accounts else {
            return Err(password_login_off());
        };
        let req = request.into_inner();
        let session = accounts
            .log_in(&req.username, &req.password, &req.client_name)
            .await?;
        let now = accounts.now_ms();
        let sessions = accounts.user(&session.user.username).map_or(1, |(_, n)| n);
        Ok(Response::new(pb::LogInResponse {
            session_token: session.token,
            expires_in_ms: session.expires_ms.saturating_sub(now),
            user: Some(user_info(&session.user, sessions)),
        }))
    }
}
