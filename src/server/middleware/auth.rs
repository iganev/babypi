use actix_web::{
    body::EitherBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    http::header::{AUTHORIZATION, WWW_AUTHENTICATE},
    Error, HttpResponse, Result,
};
use base64::{engine::general_purpose, Engine as _};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};

#[derive(Clone)]
pub struct AuthCredentials {
    pub basic_username: Option<String>,
    pub basic_password: Option<String>,
    pub bearer_token: Option<String>,
}

#[derive(Clone)]
pub struct AuthMiddleware {
    credentials: AuthCredentials,
}

impl AuthMiddleware {
    pub fn new(
        basic_username: Option<String>,
        basic_password: Option<String>,
        bearer_token: Option<String>,
    ) -> Self {
        Self {
            credentials: AuthCredentials {
                basic_username,
                basic_password,
                bearer_token,
            },
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = AuthMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthMiddlewareService {
            service,
            credentials: self.credentials.clone(),
        }))
    }
}

pub struct AuthMiddlewareService<S> {
    service: S,
    credentials: AuthCredentials,
}

impl<S, B> Service<ServiceRequest> for AuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let mut authenticated = false;

        // check if auth is even enabled
        if self.credentials.basic_username.is_none()
            && self.credentials.basic_password.is_none()
            && self.credentials.bearer_token.is_none()
        {
            authenticated = true;
        } else if let Some(Ok(auth_str)) = req.headers().get(AUTHORIZATION).map(|h| h.to_str()) {
            // check for Bearer token
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                if let Some(bearer_token) = self.credentials.bearer_token.as_deref() {
                    if token == bearer_token {
                        authenticated = true;
                    }
                }
            }
            // check for Basic auth
            else if let Some(encoded_auth_str) = auth_str.strip_prefix("Basic ") {
                if self.credentials.basic_username.is_some()
                    && self.credentials.basic_password.is_some()
                {
                    if let Ok(decoded) = general_purpose::STANDARD.decode(encoded_auth_str) {
                        if let Ok(auth_credentials) = String::from_utf8(decoded) {
                            let expected = format!(
                                "{}:{}",
                                self.credentials
                                    .basic_username
                                    .as_deref()
                                    .unwrap_or_default(),
                                self.credentials
                                    .basic_password
                                    .as_deref()
                                    .unwrap_or_default()
                            );
                            if auth_credentials == expected {
                                authenticated = true;
                            }
                        }
                    }
                }
            }
        }

        if authenticated {
            // keep walking

            let res = self.service.call(req);

            Box::pin(async move { res.await.map(ServiceResponse::map_into_left_body) })
        } else {
            // drop dead

            let (request, _pl) = req.into_parts();
            let mut response_builder = HttpResponse::Unauthorized();

            // this makes sense only when basic auth is enabled
            if self.credentials.basic_username.is_some()
                && self.credentials.basic_password.is_some()
            {
                response_builder.insert_header((WWW_AUTHENTICATE, "Basic realm=\"BabyPi\""));
            }

            let response = response_builder.finish().map_into_right_body();

            Box::pin(async { Ok(ServiceResponse::new(request, response)) })
        }
    }
}
